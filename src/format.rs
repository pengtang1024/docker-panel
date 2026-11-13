//! 中立的格式化工具（不属于 UI 层，也不属于 Docker 层，双方都可依赖）
//!
//! 放在 crate 根部是为了避免 `docker` 反向依赖 `detail_fmt`（UI 展示层的
//! 命名空间）。这里只允许放无状态、无 UI 类型、无 Docker 类型的纯函数。

/// 字节数 → 人类可读文本（1024 进制，保留 1 位小数；小于 1KB 用整数 B）
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let mut v = bytes as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    if i == 0 {
        format!("{bytes} {}", UNITS[i])
    } else {
        format!("{v:.1} {}", UNITS[i])
    }
}

#[cfg(test)]
mod tests {
    use super::format_bytes;

    #[test]
    fn formats_bytes_with_1024_steps() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(1023), "1023 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(1024 * 1024), "1.0 MB");
        assert_eq!(format_bytes(5 * 1024 * 1024 * 1024), "5.0 GB");
        // 溢出边界：>TB 时停在 TB（不产生 PB 单位）
        assert_eq!(format_bytes(u64::MAX), "16777216.0 TB");
    }
}
