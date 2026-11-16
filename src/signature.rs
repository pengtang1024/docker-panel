//! 更新清单的 Ed25519 签名校验。
//!
//! 信任链：**内置公钥** → `version.json.sig` 验签 `version.json` → 清单声明的
//! 包 SHA-256 → 实际下载的包。任何一环缺失或不匹配都 fail closed。
//!
//! 校验发生在**安装之前、应用自己的进程里**：验不过就不写盘、不改可执行文件，
//! 因此不需要任何特权参与（用户级自替换的信任闸门见 `src/update/install.rs`）。
//!
//! 私钥只存在于发布流水线（GitHub Actions secret `UPDATE_SIGNING_KEY`）与维护者
//! 本机，二进制里只有公钥；没有私钥就造不出能通过校验的清单 —— 中间人换掉下载包
//! 也过不了清单里的哈希断言。
//!
//! 轮换密钥必须同时发新版本：旧版用户只认旧公钥，换钥后旧版会拒绝更新清单。

/// 更新清单签名公钥（32 字节 Ed25519 公钥的十六进制）。
///
/// 指纹在 `docs/arch/update-manifest.md` 里同步记录，发布说明里也公布，
/// 便于用户核对「这个版本确实由项目维护者签名」。
pub const MANIFEST_PUBLIC_KEY_HEX: &str =
    "39f7d9d80fc007cba7f34f7d5155b0107458b64cb6653565298662c4c5365eae";

/// Ed25519 detached signature 的固定长度。
pub const SIGNATURE_LEN: usize = 64;

/// 清单体积上限：签名只保护「内容」，但读取端也要有界（防被喂超大文件）。
pub const MANIFEST_MAX_BYTES: usize = 256 * 1024;

/// 校验 `manifest` 的 detached 签名。返回 `Err` 时**不得**继续安装。
pub fn verify_manifest_signature(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

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
    let key = decode_hex32(MANIFEST_PUBLIC_KEY_HEX)
        .ok_or_else(|| "built-in update public key is malformed".to_string())?;
    let verifying = VerifyingKey::from_bytes(&key)
        .map_err(|e| format!("built-in update public key is invalid: {e}"))?;
    let sig = Signature::from_slice(signature)
        .map_err(|e| format!("manifest signature is malformed: {e}"))?;
    verifying
        .verify(manifest, &sig)
        .map_err(|_| "manifest signature verification failed".to_string())
}

/// 解析 64 个十六进制字符为 32 字节。
fn decode_hex32(text: &str) -> Option<[u8; 32]> {
    let bytes = text.as_bytes();
    if bytes.len() != 64 {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, chunk) in bytes.chunks(2).enumerate() {
        let hi = hex_val(chunk[0])?;
        let lo = hex_val(chunk[1])?;
        out[i] = (hi << 4) | lo;
    }
    Some(out)
}

fn hex_val(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// 测试专用固定私钥：与内置公钥无关，只用来验证「签名实现本身」是否自洽。
    fn test_key() -> SigningKey {
        SigningKey::from_bytes(&[7u8; 32])
    }

    #[test]
    fn built_in_key_parses() {
        assert!(decode_hex32(MANIFEST_PUBLIC_KEY_HEX).is_some());
        assert_eq!(MANIFEST_PUBLIC_KEY_HEX.len(), 64);
    }

    #[test]
    fn hex_decoder_rejects_bad_input() {
        assert!(decode_hex32("").is_none());
        assert!(decode_hex32("zz").is_none());
        assert!(decode_hex32(&"0".repeat(63)).is_none());
    }

    #[test]
    fn signature_roundtrip_with_embedded_key() {
        // 直接用生产公钥对应的私钥签不出来（私钥不在仓库里），
        // 这里用 dalek 自带的验签路径覆盖「签名格式正确 + 公钥不匹配 → 拒绝」，
        // 真正的端到端验签由 `scripts/sign-manifest.sh` + 真机安装验证覆盖。
        let key = test_key();
        let manifest = br#"{"schema":1,"version":"0.7.19"}"#;
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
