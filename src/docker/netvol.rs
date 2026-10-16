//! 网络列表 + 卷列表

use bollard::query_parameters::ListNetworksOptionsBuilder;

use super::connect;
use crate::models::{NetworkInfo, VolumeInfo};

pub async fn list_networks() -> Result<Vec<NetworkInfo>, String> {
    let docker = connect()?;
    let nets = docker
        .list_networks(Some(ListNetworksOptionsBuilder::default().build()))
        .await
        .map_err(|e| format!("网络列表失败: {e}"))?;
    Ok(nets
        .into_iter()
        .map(|n| {
            let id = n.id.unwrap_or_default();
            let short_id = if id.len() > 12 {
                id[..12].to_string()
            } else {
                id.clone()
            };
            NetworkInfo {
                id,
                short_id,
                name: n.name.unwrap_or_default(),
                driver: n.driver.unwrap_or_default(),
                scope: n.scope.unwrap_or_default(),
                containers: 0,
            }
        })
        .collect())
}

pub async fn list_volumes() -> Result<Vec<VolumeInfo>, String> {
    let docker = connect()?;
    let resp = docker
        .list_volumes(None::<bollard::query_parameters::ListVolumesOptions>)
        .await
        .map_err(|e| format!("卷列表失败: {e}"))?;
    Ok(resp
        .volumes
        .unwrap_or_default()
        .into_iter()
        .map(|v| VolumeInfo {
            name: v.name,
            driver: v.driver,
            mountpoint: v.mountpoint,
            size: String::new(),
        })
        .collect())
}
