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
//! 轮换：`MANIFEST_PUBLIC_KEYS_HEX` 是**有序**列表，新钥放最前，旧钥保留到所有在用
//! 版本都升级完毕再删。任一内置公钥验签通过即接受，因此换钥不需要先发「过渡版本」，
//! 但**删旧钥**必须等旧版本全部淘汰——旧版本只认自己的那份列表。

/// 内置可信公钥列表（32 字节 Ed25519 公钥的十六进制），**顺序即优先级**。
///
/// 公钥指纹记录在 `docs/arch/update-manifest.md`，发布说明里也公布，
/// 便于用户核对「这个版本确实由项目维护者签名」。
pub const MANIFEST_PUBLIC_KEYS_HEX: &[&str] = &[
    // 0.7.19 起使用的第一把发布钥
    "39f7d9d80fc007cba7f34f7d5155b0107458b64cb6653565298662c4c5365eae",
];

/// Ed25519 detached signature 的固定长度。
pub const SIGNATURE_LEN: usize = 64;

/// 清单体积上限：签名只保护「内容」，但读取端也要有界（防被喂超大文件）。
pub const MANIFEST_MAX_BYTES: usize = 256 * 1024;

/// 校验 `manifest` 的 detached 签名。返回 `Err` 时**不得**继续安装。
///
/// 依次尝试每个内置公钥：任一通过即接受（轮换过渡期新旧钥并存）。
pub fn verify_manifest_signature(manifest: &[u8], signature: &[u8]) -> Result<(), String> {
    verify_with_keys(MANIFEST_PUBLIC_KEYS_HEX, manifest, signature)
}

/// 校验主体：公钥列表可注入，便于单测覆盖轮换与坏条目。
fn verify_with_keys(keys: &[&str], manifest: &[u8], signature: &[u8]) -> Result<(), String> {
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
    let sig = Signature::from_slice(signature)
        .map_err(|e| format!("manifest signature is malformed: {e}"))?;

    let mut usable = 0usize;
    for key_hex in keys {
        // 单把公钥写坏不该拖垮其它公钥：跳过并继续。
        let Some(raw) = decode_hex32(key_hex) else {
            continue;
        };
        let Ok(verifying) = VerifyingKey::from_bytes(&raw) else {
            continue;
        };
        usable += 1;
        if verifying.verify(manifest, &sig).is_ok() {
            return Ok(());
        }
    }
    if usable == 0 {
        return Err("built-in update public keys are malformed".to_string());
    }
    Err("manifest signature verification failed".to_string())
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
    fn built_in_keys_parse() {
        assert!(!MANIFEST_PUBLIC_KEYS_HEX.is_empty());
        for key in MANIFEST_PUBLIC_KEYS_HEX {
            assert_eq!(key.len(), 64, "公钥必须是 64 个十六进制字符：{key}");
            assert!(decode_hex32(key).is_some(), "公钥不是合法十六进制：{key}");
        }
    }

    /// 轮换：列表里任一公钥都能通过；坏条目被跳过，而不是拖垮整条链。
    #[test]
    fn any_embedded_key_verifies() {
        let manifest = br#"{"schema":1,"version":"0.9.9"}"#;
        let active = SigningKey::from_bytes(&[9u8; 32]);
        let sig = active.sign(manifest).to_bytes();
        let hex = |k: &SigningKey| {
            k.verifying_key()
                .to_bytes()
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect::<String>()
        };
        let other = hex(&test_key());
        let active_hex = hex(&active);

        // 第一把不匹配 → 落到第二把 → 通过
        assert!(verify_with_keys(&[&other, &active_hex], manifest, &sig).is_ok());
        // 坏条目（非法十六进制）被跳过，不影响后面的好钥匙
        assert!(verify_with_keys(&["zz", &active_hex], manifest, &sig).is_ok());
        // 全是坏条目 → 明确报「公钥损坏」而不是「验签失败」
        let err = verify_with_keys(&["zz"], manifest, &sig).unwrap_err();
        assert!(err.contains("malformed"), "{err}");
        // 都不匹配 → 拒绝
        assert!(verify_with_keys(&[&other], manifest, &sig).is_err());
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
