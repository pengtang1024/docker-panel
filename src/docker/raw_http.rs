//! Docker 裸 HTTP 请求（Unix socket + HTTP/1.1）
//!
//! **为什么需要它**：bollard 0.21 的 `SystemDataUsageResponse` 只建模了**废弃**的
//! `VolumeUsage` 键，而守护进程（API ≥1.25）返回的是 `Volumes` 数组
//! → `docker.df()` 解析出来的卷占用恒空 → 卷列表「大小」列永远是「—」。
//! bollard 没有公开的裸请求入口（`process*` / `build_request` 都是 `pub(crate)`），
//! 这里自己发一条 HTTP/1.1 请求，只取需要的键。
//!
//! 约束：
//! - 只支持 **Unix socket**：路径与 bollard 的 `connect_with_unix_defaults()` 一致
//!   （`DOCKER_HOST` 以 `unix://` 开头就用它，否则 `/var/run/docker.sock`）
//! - 阻塞 IO 放 `spawn_blocking`，不占 tokio 工作线程
//! - 失败一律上抛错误，调用方降级（不崩、不影响其它功能）

use std::time::Duration;

/// 单次裸请求的上限（守护进程无响应时不能挂住后台任务）
const IO_TIMEOUT: Duration = Duration::from_secs(10);

/// Docker Unix socket 路径（与 bollard `connect_with_unix_defaults` 保持一致）
fn socket_path() -> std::path::PathBuf {
    std::env::var("DOCKER_HOST")
        .ok()
        .filter(|h| h.starts_with("unix://"))
        .map(|h| std::path::PathBuf::from(&h["unix://".len()..]))
        .unwrap_or_else(|| std::path::PathBuf::from("/var/run/docker.sock"))
}

/// GET 请求，返回响应体（字节）
///
/// 路径形如 `/system/df?type=volume`（不带 API 版本前缀 → 守护进程用默认版本）。
#[cfg(unix)]
pub async fn get(path: &str) -> Result<Vec<u8>, String> {
    let path = path.to_string();
    tokio::task::spawn_blocking(move || get_blocking(&path))
        .await
        .map_err(|e| format!("raw http task failed: {e}"))?
}

/// 非 unix 平台不支持裸 socket（Windows 的命名管道 / TLS 均未实现）
#[cfg(not(unix))]
pub async fn get(_path: &str) -> Result<Vec<u8>, String> {
    Err("raw http is only supported on unix".to_string())
}

#[cfg(unix)]
fn get_blocking(path: &str) -> Result<Vec<u8>, String> {
    use std::io::{Read, Write};
    use std::os::unix::net::UnixStream;

    let sock = socket_path();
    let mut stream = UnixStream::connect(&sock)
        .map_err(|e| format!("failed to connect {}: {e}", sock.display()))?;
    stream.set_read_timeout(Some(IO_TIMEOUT)).ok();
    stream.set_write_timeout(Some(IO_TIMEOUT)).ok();

    let req = format!(
        "GET {path} HTTP/1.1\r\n\
         Host: localhost\r\n\
         Accept: application/json\r\n\
         User-Agent: docker-panel\r\n\
         Connection: close\r\n\r\n"
    );
    stream
        .write_all(req.as_bytes())
        .map_err(|e| format!("failed to send request: {e}"))?;

    // Connection: close → 守护进程发完 body 就关连接，read 返回 0
    let mut raw = Vec::new();
    let mut buf = [0u8; 8192];
    loop {
        match stream.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => raw.extend_from_slice(&buf[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                // 读超时：已有数据就按收到的内容解析，没有就报错
                if raw.is_empty() {
                    return Err(format!("read timeout: {e}"));
                }
                break;
            }
            Err(e) => return Err(format!("failed to read response: {e}")),
        }
    }
    parse_response(&raw)
}

/// 解析 HTTP/1.1 响应：状态行 + 头 + 体
///
/// 支持 `Content-Length` 与 `chunked` 两种传输编码（守护进程两种都可能用）。
fn parse_response(raw: &[u8]) -> Result<Vec<u8>, String> {
    let sep = raw
        .windows(4)
        .position(|w| w == b"\r\n\r\n")
        .ok_or_else(|| "malformed http response (no header terminator)".to_string())?;
    let head = String::from_utf8_lossy(&raw[..sep]).to_string();
    let body = &raw[sep + 4..];

    let mut lines = head.split("\r\n");
    let status = lines.next().unwrap_or_default();
    if !status.contains(" 200 ") {
        return Err(format!("docker api returned: {status}"));
    }

    let mut chunked = false;
    let mut content_length: Option<usize> = None;
    for line in lines {
        if let Some((k, v)) = line.split_once(':') {
            match k.trim().to_ascii_lowercase().as_str() {
                "transfer-encoding" => chunked = v.to_ascii_lowercase().contains("chunked"),
                "content-length" => content_length = v.trim().parse().ok(),
                _ => {}
            }
        }
    }

    if chunked {
        decode_chunked(body)
    } else {
        let n = content_length.unwrap_or(body.len()).min(body.len());
        Ok(body[..n].to_vec())
    }
}

/// 解 chunked 传输编码：`<hex 长度>[;扩展]\r\n<数据>\r\n…` → 原始字节
fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut rest = body;
    loop {
        let nl = rest
            .windows(2)
            .position(|w| w == b"\r\n")
            .ok_or_else(|| "truncated chunked body".to_string())?;
        let size_line = String::from_utf8_lossy(&rest[..nl]).to_string();
        let size_str = size_line.split(';').next().unwrap_or("").trim();
        let size = usize::from_str_radix(size_str, 16)
            .map_err(|_| format!("invalid chunk size: {size_line}"))?;
        rest = &rest[nl + 2..];
        if size == 0 {
            break;
        }
        if rest.len() < size {
            return Err("truncated chunk data".to_string());
        }
        out.extend_from_slice(&rest[..size]);
        rest = &rest[size..];
        if rest.starts_with(b"\r\n") {
            rest = &rest[2..];
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_content_length_response() {
        let raw = b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 7\r\n\r\n{\"a\":1}";
        let body = parse_response(raw).unwrap();
        assert_eq!(body, b"{\"a\":1}");
    }

    #[test]
    fn parses_chunked_response() {
        let raw =
            b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n7\r\n{\"a\":1}\r\n0\r\n\r\n";
        let body = parse_response(raw).unwrap();
        assert_eq!(body, b"{\"a\":1}");
    }

    #[test]
    fn rejects_non_200_status() {
        let raw = b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n";
        assert!(parse_response(raw).is_err());
    }

    #[test]
    fn socket_path_follows_docker_host() {
        // 默认路径（不改动进程环境变量）
        let p = socket_path();
        assert!(p.ends_with("docker.sock"));
    }
}
