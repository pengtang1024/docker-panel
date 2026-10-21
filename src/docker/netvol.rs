//! 网络列表 + 卷列表 + 网络/卷删除

use std::collections::HashMap;

use bollard::query_parameters::{ListNetworksOptionsBuilder, RemoveVolumeOptionsBuilder};

use super::connect;
use super::raw_http;
use crate::models::{NetworkInfo, VolumeInfo};

/// Docker 内置网络：不可删除、不可改名
const BUILTIN_NETS: [&str; 3] = ["bridge", "host", "none"];

pub async fn list_networks() -> Result<Vec<NetworkInfo>, String> {
    let docker = connect()?;
    let nets = docker
        .list_networks(Some(ListNetworksOptionsBuilder::default().build()))
        .await
        .map_err(|e| crate::i18n::trf("Failed to list networks: {0}", &[&e.to_string()]))?;
    Ok(nets
        .into_iter()
        .map(|n| {
            let id = n.id.unwrap_or_default();
            let short_id = if id.len() > 12 {
                id[..12].to_string()
            } else {
                id.clone()
            };
            let name = n.name.unwrap_or_default();
            // IPAM 第一段配置即主网段；host/none 之类内置网络没有 IPAM
            let cfg = n
                .ipam
                .as_ref()
                .and_then(|i| i.config.as_ref())
                .and_then(|c| c.first());
            NetworkInfo {
                builtin: BUILTIN_NETS.contains(&name.as_str()),
                id,
                short_id,
                name,
                driver: n.driver.unwrap_or_default(),
                scope: n.scope.unwrap_or_default(),
                subnet: cfg.and_then(|c| c.subnet.clone()).unwrap_or_default(),
                gateway: cfg.and_then(|c| c.gateway.clone()).unwrap_or_default(),
            }
        })
        .collect())
}

pub async fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    let docker = connect()?;
    let resp = docker
        .list_volumes(None::<bollard::query_parameters::ListVolumesOptions>)
        .await
        .map_err(|e| crate::i18n::trf("Failed to list volumes: {0}", &[&e.to_string()]))?;
    Ok(resp
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| VolumeInfo {
            anonymous: is_anonymous(&v.name),
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            created: v.created_at.unwrap_or_default(),
        })
        .collect())
}

/// `/system/df?type=volume` → 每个卷的磁盘占用（name → 字节）
///
/// 列表 API 不返回卷大小，必须单独走 system df。该接口会遍历卷目录统计磁盘
/// 占用（有 I/O 成本），故只由低频 interval 调用，不做 3s 轮询。
///
/// ⚠️ **不走 bollard 的 `df()`**：bollard 0.21 的 `SystemDataUsageResponse` 只建模了
/// 废弃的 `VolumeUsage` 键，而守护进程（API ≥1.25）返回的是 `Volumes` 数组
/// → `df()` 解析出来的卷占用恒空。这里自己发一条裸 HTTP 请求（见 `raw_http.rs`），
/// 直接读 `Volumes` 键。
pub async fn volume_sizes() -> Result<HashMap<String, u64>, String> {
    let body = raw_http::get("/system/df?type=volume").await?;
    let resp: serde_json::Value = serde_json::from_slice(&body)
        .map_err(|e| crate::i18n::trf("Failed to read volume sizes: {0}", &[&e.to_string()]))?;
    let mut out = HashMap::new();
    let volumes = resp
        .get("Volumes")
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    for item in volumes {
        let name = item.get("Name").and_then(|v| v.as_str());
        let size = item
            .get("UsageData")
            .and_then(|u| u.get("Size"))
            .and_then(|s| s.as_i64());
        // Size = -1 表示「不可用」（非 local 驱动的卷）
        if let (Some(name), Some(size)) = (name, size) {
            if size >= 0 {
                out.insert(name.to_string(), size as u64);
            }
        }
    }
    Ok(out)
}

/// 匿名卷启发式：Docker 自动生成的卷名为 64 位小写十六进制
fn is_anonymous(name: &str) -> bool {
    name.len() == 64
        && name
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// 删除网络（Docker 接受网络名或 ID）
pub async fn remove_network(id: &str) -> Result<String, String> {
    let docker = connect()?;
    docker
        .remove_network(id)
        .await
        .map_err(|e| crate::i18n::trf("Failed to remove network: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Network removed"))
}

/// 删除卷
pub async fn remove_volume(name: &str) -> Result<String, String> {
    let docker = connect()?;
    let opts = RemoveVolumeOptionsBuilder::default().force(false).build();
    docker
        .remove_volume(name, Some(opts))
        .await
        .map_err(|e| crate::i18n::trf("Failed to remove volume: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::t("Volume removed"))
}
