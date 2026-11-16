//! 更新清单的 Ed25519 签名校验（支持公钥轮换）。
//!
//! 信任链：**内置公钥** → `version.json.sig` 验签 `version.json` → 清单声明的
//! 包 SHA-256 → 实际下载的包。任何一环缺失或不匹配都 fail closed。
//!
//! 校验发生在**安装之前、应用自己的进程里**：验不过就不写盘、不改可执行文件，
//! 因此不需要任何特权参与（用户级自替换的信任闸门见 `src/update/install.rs`）。
//!
//! **私钥不进入 CI**：流水线只构建草稿 Release，签名由维护者离线完成
//! （`scripts/sign-release.sh`），签完才发布。CI 里没有私钥，构建机被攻破也签不出
//! 能通过校验的清单；二进制里只有公钥。
//!
//! 公钥清单是仓库里的 `packaging/release-signing.pub`（编译期内嵌），启动器
//! `launcher/` 读的是同一个文件 → 应用与启动器共用唯一信任锚，不会各存一份漂移。
//!
//! 轮换：文件是**有序**列表，新钥追加到末尾，旧钥保留到所有在用版本都升级完毕再删。
//! 任一内置公钥验签通过即接受，因此换钥不需要先发「过渡版本」，但**删旧钥**必须等
//! 旧版本全部淘汰——旧版本只认自己编译进去的那份列表。

use base64::Engine as _;
use ed25519_dalek::{Signature, Verifier, VerifyingKey};

/// 编译期内嵌的公钥清单（与启动器共用）。
const EMBEDDED_KEYS: &str = include_str!("../packaging/release-signing.pub");

/// Ed25519 detached signature 的固定长度。
pub const SIGNATURE_LEN: usize = 64;

/// 清单体积上限：签名只保护「内容」，但读取端也要有界（防被喂超大文件）。
pub const MANIFEST_MAX_BYTES: usize = 256 * 1024;

/// 解析内置公钥清单（`#` 注释与空行忽略，每行一把 base64 raw 32 字节）。
pub fn embedded_keys() -> Result<Vec<VerifyingKey>, String> {
    let keys = parse_keys(EMBEDDED_KEYS)?;
    if keys.is_empty() {
        return Err("no built-in update public key is configured".to_string());
    }
    Ok(keys)
}

/// 解析任意一份公钥清单文本（单测注入用）。
pub fn parse_keys(text: &str) -> Result<Vec<VerifyingKey>, String> {
    let mut keys = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(line)
            .map_err(|_| format!("update public key is malformed: {line}"))?;
        let raw: [u8; 32] = bytes
            .try_into()
            .map_err(|_| format!("update public key must be 32 bytes: {line}"))?;
        let key = VerifyingKey::from_bytes(&raw)
            .map_err(|_| format!("update public key is not a valid Ed25519 key: {line}"))?;
        keys.push(key);
    }
    Ok(keys)
}

/// 校验 `manifest` 的 detached 签名。返回 `Err` 时**不得**继续安装。
///
/// 依次尝试每个内置公钥：任一通过即接受（轮换过渡期新旧钥并存）。
pub fn verify_manifest_signature(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    verify_with_keys(&embedded_keys()?, manifest, signature)
}

/// 校验 `checksums.txt` 的 detached 签名（base64 文本，启动器读的是同一份文件）。
///
/// 启动器这条信任链用它把关「用户目录里的运行体」：签名通过 → 从清单取 sha256 →
/// 再校运行体字节。失败一律 fail closed。
pub fn verify_checksums_signature(checksums: &str, signature_b64: &str) -> Result<(), String> {
    if checksums.is_empty() {
        return Err("checksums is empty".to_string());
    }
    if checksums.len() > MANIFEST_MAX_BYTES {
        return Err(format!("checksums is too large: {} bytes", checksums.len()));
    }
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(signature_b64.trim())
        .map_err(|_| "checksums signature is malformed".to_string())?;
    verify_with_keys(&embedded_keys()?, checksums.as_bytes(), &bytes)
}

/// 校验主体：公钥可注入，便于单测覆盖轮换与坏条目。
fn verify_with_keys(
    keys: &[VerifyingKey],
    manifest: &[u8],
    signature: &[u8],
) -> Result<(), String> {
    if manifest.is_empty() {
        return Err("manifest is empty".to_string());
    }
    if manifest.len() > MANIFEST_MAX_BYTES {
        return Err(format!("manifest is too large: {} bytes", manifest.len()));
    }
    if signature.len() != SIGNATURE_LEN {
        return Err(format!(
            "manifest signature must be {SIGNATURE_LEN} bytes, got {}",
            signature.len()
        ));
    }
    if keys.is_empty() {
        return Err("no built-in update public key is configured".to_string());
    }
    let sig = Signature::from_slice(signature)
        .map_err(|e| format!("manifest signature is malformed: {e}"))?;
    if keys.iter().any(|key| key.verify(manifest, &sig).is_ok()) {
        return Ok(());
    }
    Err("manifest signature verification failed".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    fn b64(key: &VerifyingKey) -> String {
        base64::engine::general_purpose::STANDARD.encode(key.to_bytes())
    }

    #[test]
    fn embedded_keys_parse_and_are_distinct() {
        let keys = embedded_keys().expect("内置公钥必须可解析");
        assert!(!keys.is_empty());
        // 轮换列表里不该重复同一把钥（重复只说明抄错了）
        let mut seen = std::collections::HashSet::new();
        for key in &keys {
            assert!(seen.insert(key.to_bytes()), "公钥清单里有重复条目");
        }
    }

    /// 轮换：列表里任一公钥都能通过；坏条目被跳过，而不是拖垮整条链。
    #[test]
    fn any_embedded_key_verifies() {
        let manifest = br#"{"schema":1,"version":"0.9.9"}"#;
        let active = SigningKey::from_bytes(&[9u8; 32]);
        let sig = active.sign(manifest).to_bytes();
        let other = test_key().verifying_key();
        let active_key = active.verifying_key();

        // 第一把不匹配 → 落到第二把 → 通过
        assert!(verify_with_keys(&[other, active_key], manifest, &sig).is_ok());
        // 都不匹配 → 拒绝
        assert!(verify_with_keys(&[other], manifest, &sig).is_err());
        // 空列表 → 明确报「没有公钥」而不是「验签失败」
        let err = verify_with_keys(&[], manifest, &sig).unwrap_err();
        assert!(err.contains("no built-in"), "{err}");
    }

    #[test]
    fn key_list_ignores_comments_and_blank_lines_and_skips_empty() {
        let key = test_key().verifying_key();
        let text = format!("# 注释\n\n  {}  \n", b64(&key));
        assert_eq!(parse_keys(&text).unwrap(), vec![key]);
        assert!(parse_keys("# 只有注释\n\n").unwrap().is_empty());
    }

    #[test]
    fn malformed_key_lines_are_rejected() {
        assert!(parse_keys("不是base64").is_err());
        // 长度不对（16 字节）
        let short = base64::engine::general_purpose::STANDARD.encode([0u8; 16]);
        let err = parse_keys(&short).unwrap_err();
        assert!(err.contains("32 bytes"), "{err}");
        // 任何 32 字节在解析阶段都算「形状合法」（ed25519-dalek 的解压校验发生在验签时），
        // 所以这里不假装能靠 key 解析挡住垃圾条目：真正把关的是验签结果。
        let any32 = base64::engine::general_purpose::STANDARD.encode([0xffu8; 32]);
        assert_eq!(parse_keys(&any32).map(|k| k.len()), Ok(1));
    }

    #[test]
    fn signature_roundtrip_with_embedded_key() {
        // 生产私钥不在仓库里，这里覆盖「格式正确 + 公钥不匹配 → 拒绝」，
        // 真正的端到端验签由 `scripts/sign-manifest.sh` / `sign-release.sh` 覆盖。
        let key = test_key();
        let manifest = br#"{"schema":1,"version":"0.8.0"}"#;
        let sig = key.sign(manifest);
        assert!(verify_manifest_signature(manifest, &sig.to_bytes()).is_err());
    }

    #[test]
    fn empty_manifest_is_rejected() {
        assert!(verify_manifest_signature(b"", &[0u8; SIGNATURE_LEN]).is_err());
    }

    #[test]
    fn wrong_signature_length_is_rejected() {
        assert!(verify_manifest_signature(b"{}", &[0u8; 32]).is_err());
    }

    #[test]
    fn oversized_manifest_is_rejected() {
        let big = vec![b'a'; MANIFEST_MAX_BYTES + 1];
        assert!(verify_manifest_signature(&big, &[0u8; SIGNATURE_LEN]).is_err());
    }
}
