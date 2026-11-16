//! 运行体校验：Ed25519 验 `checksums.txt` → 用它的 sha256 校 `docker-panel`。
//!
//! 信任链是「离线私钥 → checksums.txt → 运行体字节」，公钥编译进启动器。用户目录里的
//! 文件不受 dpkg 保护，所以这里**只认签名**：checksums.txt 或运行体被改过一律拒绝，
//! 不做「差不多就放行」的降级。

use std::fs::{self, File};
use std::io::Read;
use std::os::unix::fs::OpenOptionsExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

use base64::Engine;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use sha2::{Digest, Sha256};

pub const PAYLOAD_NAME: &str = "docker-panel";
pub const CHECKSUMS_NAME: &str = "checksums.txt";
pub const SIGNATURE_NAME: &str = "checksums.txt.sig";

/// 内置信赖锚：仓库里的公钥清单（`#` 注释、每行一把 base64 raw 32 字节公钥）。
const EMBEDDED_KEYS: &str = include_str!("../../packaging/release-signing.pub");

/// 已通过校验的运行体：`file` 是打开着的那一个 inode，exec 必须用它、不要再按路径 open。
#[derive(Debug)]
// 集成测试只跑校验路径、不 exec，字段在测试构建里会「没被读」
#[cfg_attr(test, allow(dead_code))]
pub struct VerifiedPayload {
    pub file: File,
    pub sha256: String,
}

pub fn embedded_keys() -> Result<Vec<VerifyingKey>, String> {
    let keys = parse_keys(EMBEDDED_KEYS)?;
    if keys.is_empty() {
        return Err("启动器未内置发布公钥".into());
    }
    Ok(keys)
}

pub fn verify_checksums(
    keys: &[VerifyingKey],
    checksums: &str,
    signature_b64: &str,
) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| "发布签名格式损坏".to_string())?;
    let signature = Signature::from_slice(&bytes).map_err(|_| "发布签名格式损坏".to_string())?;
    if keys
        .iter()
        .any(|key| key.verify(checksums.as_bytes(), &signature).is_ok())
    {
        return Ok(());
    }
    Err("发布签名校验失败".into())
}

pub fn parse_keys(text: &str) -> Result<Vec<VerifyingKey>, String> {
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(line)
            .map_err(|_| "发布公钥格式损坏".to_string())?;
        let bytes: [u8; 32] = bytes
            .try_into()
            .map_err(|_| "发布公钥格式损坏".to_string())?;
        keys.push(VerifyingKey::from_bytes(&bytes).map_err(|_| "发布公钥格式损坏".to_string())?);
    }
    Ok(keys)
}

/// 从 `checksums.txt` 里取某个资产名的 sha256（格式：`<hex>  <name>`，名字前可有 `*`）。
pub fn checksum_for(checksums: &str, asset_name: &str) -> Option<String> {
    for line in checksums.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut it = line.split_whitespace();
        let hash = it.next()?;
        let name = it.next()?;
        if name.trim_start_matches('*') != asset_name {
            continue;
        }
        let valid = hash.len() == 64 && hash.chars().all(|c| c.is_ascii_hexdigit());
        return valid.then(|| hash.to_ascii_lowercase());
    }
    None
}

pub fn verify_dir(dir: &Path, keys: &[VerifyingKey]) -> Result<VerifiedPayload, String> {
    let checksums = read_text(&dir.join(CHECKSUMS_NAME))?;
    let signature = read_text(&dir.join(SIGNATURE_NAME))?;
    verify_checksums(keys, &checksums, &signature)?;
    let expected = checksum_for(&checksums, PAYLOAD_NAME)
        .ok_or_else(|| format!("{CHECKSUMS_NAME} 里没有 {PAYLOAD_NAME} 的校验和"))?;

    let file = open_for_exec(&dir.join(PAYLOAD_NAME))?;
    let actual = sha256_of_open_file(&file)?;
    if actual != expected {
        return Err("运行体 sha256 与签名清单不一致".into());
    }
    Ok(VerifiedPayload {
        file,
        sha256: actual,
    })
}

/// 校验 deb 自带的种子运行体。
///
/// 种子**不能**用签名校验：它的 `checksums.txt.sig` 要到发布前离线签名时才存在，
/// 而 deb 是 CI 构建的（那时还没有签名），打不进包里。种子的信任来源是 dpkg：
/// 文件属于 root、对组/其他不可写 —— 用户想改它就得先拿到 root，本启动器也就
/// 没有「防同权限用户」的职责了。用户副本（`versions/<v>/`）仍然必须验签。
pub fn verify_seed(dir: &Path) -> Result<VerifiedPayload, String> {
    use std::os::unix::fs::MetadataExt;

    let file = open_for_exec(&dir.join(PAYLOAD_NAME))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("读取 {} 属性失败：{e}", dir.join(PAYLOAD_NAME).display()))?;
    if meta.uid() != 0 {
        return Err("deb 种子运行体不属于 root（可能被替换过）".into());
    }
    if meta.mode() & 0o022 != 0 {
        return Err("deb 种子运行体对组/其他可写".into());
    }
    let sha256 = sha256_of_open_file(&file)?;
    Ok(VerifiedPayload { file, sha256 })
}

fn read_text(path: &Path) -> Result<String, String> {
    fs::read_to_string(path).map_err(|e| format!("读取 {} 失败：{e}", path.display()))
}

/// 以 `O_PATH` 打开：只持有 inode 引用，不建立读写权限，专供 `execveat` 使用。
fn open_for_exec(path: &Path) -> Result<File, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .map_err(|e| format!("打开 {} 失败：{e}", path.display()))?;
    let meta = file
        .metadata()
        .map_err(|e| format!("读取 {} 属性失败：{e}", path.display()))?;
    if !meta.is_file() {
        return Err(format!("{} 不是普通文件", path.display()));
    }
    if meta.len() == 0 {
        return Err(format!("{} 是空文件", path.display()));
    }
    Ok(file)
}

/// 用 `/proc/self/fd/<n>` 读回同一 inode 的字节，避免「先按路径校验、再按路径 exec」的竞态。
fn sha256_of_open_file(file: &File) -> Result<String, String> {
    let fd_path = format!("/proc/self/fd/{}", file.as_raw_fd());
    let mut reader = File::open(&fd_path).map_err(|e| format!("打开 {fd_path} 失败：{e}"))?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    loop {
        let n = reader
            .read(&mut buf)
            .map_err(|e| format!("读取 {fd_path} 失败：{e}"))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push_str(&format!("{b:02x}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};
    use std::io::Write;

    const PAYLOAD: &[u8] = b"#!/bin/sh\necho docker-panel\n";

    fn test_keys() -> (SigningKey, Vec<VerifyingKey>) {
        let key = SigningKey::from_bytes(&[7u8; 32]);
        let vk = key.verifying_key();
        (key, vec![vk])
    }

    fn payload_sha() -> String {
        hex_lower(&Sha256::digest(PAYLOAD))
    }

    fn write_fixture(dir: &Path, key: &SigningKey) -> String {
        let checksums = format!("{}  {PAYLOAD_NAME}\n", payload_sha());
        let signature = base64::engine::general_purpose::STANDARD
            .encode(key.sign(checksums.as_bytes()).to_bytes());
        fs::write(dir.join(CHECKSUMS_NAME), &checksums).unwrap();
        fs::write(dir.join(SIGNATURE_NAME), &signature).unwrap();
        fs::write(dir.join(PAYLOAD_NAME), PAYLOAD).unwrap();
        signature
    }

    #[test]
    fn accepts_signed_payload() {
        let (key, keys) = test_keys();
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path(), &key);
        let verified = verify_dir(dir.path(), &keys).expect("签名正确的运行体应通过");
        assert_eq!(verified.sha256, payload_sha());
    }

    #[test]
    fn rejects_tampered_payload() {
        let (key, keys) = test_keys();
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path(), &key);
        let mut f = fs::OpenOptions::new()
            .append(true)
            .open(dir.path().join(PAYLOAD_NAME))
            .unwrap();
        f.write_all(b"echo pwned\n").unwrap();
        let err = verify_dir(dir.path(), &keys).unwrap_err();
        assert!(err.contains("sha256"), "错误应指向哈希不一致：{err}");
    }

    #[test]
    fn rejects_missing_signature() {
        let (key, keys) = test_keys();
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path(), &key);
        fs::remove_file(dir.path().join(SIGNATURE_NAME)).unwrap();
        assert!(verify_dir(dir.path(), &keys).unwrap_err().contains("读取"));
    }

    #[test]
    fn rejects_checksums_from_another_key() {
        let (_key, keys) = test_keys();
        let other = SigningKey::from_bytes(&[3u8; 32]);
        let dir = tempfile::tempdir().unwrap();
        write_fixture(dir.path(), &other);
        let err = verify_dir(dir.path(), &keys).unwrap_err();
        assert!(err.contains("签名校验失败"), "{err}");
    }

    #[test]
    fn rejects_checksums_without_payload_entry() {
        let (key, keys) = test_keys();
        let dir = tempfile::tempdir().unwrap();
        let checksums = "0000000000000000000000000000000000000000000000000000000000000000  other\n";
        let signature = base64::engine::general_purpose::STANDARD
            .encode(key.sign(checksums.as_bytes()).to_bytes());
        fs::write(dir.path().join(CHECKSUMS_NAME), checksums).unwrap();
        fs::write(dir.path().join(SIGNATURE_NAME), signature).unwrap();
        fs::write(dir.path().join(PAYLOAD_NAME), PAYLOAD).unwrap();
        assert!(verify_dir(dir.path(), &keys).unwrap_err().contains("没有"));
    }

    #[test]
    fn parses_checksum_lines() {
        let text = format!("{}  {PAYLOAD_NAME}\n", payload_sha());
        assert_eq!(checksum_for(&text, PAYLOAD_NAME), Some(payload_sha()));
        // 二进制模式的星号挂在文件名前（coreutils 格式），不是挂在哈希前
        let starred = format!("{}  *{PAYLOAD_NAME}\n", payload_sha());
        assert_eq!(checksum_for(&starred, PAYLOAD_NAME), Some(payload_sha()));
        assert_eq!(checksum_for(&text, "other"), None);
        // 长度不对的哈希不算校验和
        assert_eq!(
            checksum_for(&format!("abc  {PAYLOAD_NAME}\n"), PAYLOAD_NAME),
            None
        );
    }

    #[test]
    fn parses_key_list() {
        let (_key, keys) = test_keys();
        let text = format!(
            "# 注释\n\n{}\n",
            base64::engine::general_purpose::STANDARD.encode(keys[0].to_bytes())
        );
        assert_eq!(parse_keys(&text).unwrap().len(), 1);
        assert!(parse_keys("不是base64").is_err());
    }

    #[test]
    fn seed_check_rejects_symlink_target_or_missing() {
        let dir = tempfile::tempdir().unwrap();
        // 种子目录里没有运行体 → 明确报错，不 panic
        let err = verify_seed(dir.path()).unwrap_err();
        assert!(err.contains("打开"), "{err}");
        // 普通文件但属主不是 root（测试进程是普通用户）→ 拒绝
        fs::write(dir.path().join(PAYLOAD_NAME), PAYLOAD).unwrap();
        let err = verify_seed(dir.path()).unwrap_err();
        assert!(err.contains("root"), "{err}");
    }

    #[test]
    fn embedded_keys_parse() {
        assert!(!embedded_keys().unwrap().is_empty());
    }
}
