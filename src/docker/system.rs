//! 系统级 API：清理（prune）
//!
//! cleanup(stopped, dangling, unused_net, unused_vol) 四个 bool 可以任意组合，
//! 与清理弹窗的四个复选框**一一对应**：
//!   - stopped    → prune_containers（删所有已停止的容器）
//!   - dangling   → prune_images(dangling=true)（只删悬空镜像）
//!   - unused_net → prune_networks（删所有未被容器引用的网络）
//!   - unused_vol → prune_volumes（删所有未被容器引用的卷）
//!
//! ⚠️ 历史包袱：早期实现只有三个开关（stopped / dangling / unused-images），
//!    其中 unused-images 走 prune_images(dangling=false)（= `docker image prune -a`，
//!    会连带删掉**带 Tag** 的未使用镜像）。UI 改成四复选框（原型口径：
//!    容器 / 镜像 / 网络 / 卷）后，网络开关被错接到了这个「未使用镜像」分支上——
//!    勾「未使用的网络」实际删的是未使用镜像，「未使用的卷」则完全无效果。
//!    现按 UI 语义一一对应；「所有未使用镜像」不再从弹窗暴露（原型口径里就没有这一项）。
//!
//! 错误处理：每项独立 try，失败不中断其他项（Docker API 独立）。
//! 如果四项全勾都没效果 → 返回 Err("请至少勾选一项清理内容")。

use std::collections::HashMap;

use bollard::query_parameters::{PruneImagesOptionsBuilder, PruneVolumesOptionsBuilder};

use crate::i18n::trf;

use super::connect;

fn format_bytes(bytes: u64) -> String {
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

/// 清理：已停止容器 / 悬空镜像 / 未使用网络 / 未使用卷（可组合勾选）
pub async fn cleanup(
    stopped: bool,
    dangling: bool,
    unused_net: bool,
    unused_vol: bool,
) -> Result<String, String> {
    let docker = connect()?;
    let mut lines: Vec<String> = Vec::new();

    if stopped {
        let resp = docker
            .prune_containers(None)
            .await
            .map_err(|e| trf("Cleanup stopped containers failed: {0}", &[&e.to_string()]))?;
        let n = resp
            .containers_deleted
            .as_ref()
            .map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(trf(
            "Stopped containers: removed {0}, freed {1}",
            &[&n.to_string(), &format_bytes(space as u64)],
        ));
    }

    if dangling {
        let mut filters = HashMap::new();
        filters.insert("dangling".to_string(), vec!["true".to_string()]);
        let options = PruneImagesOptionsBuilder::new().filters(&filters).build();
        let resp = docker
            .prune_images(Some(options))
            .await
            .map_err(|e| trf("Cleanup dangling images failed: {0}", &[&e.to_string()]))?;
        let n = resp.images_deleted.as_ref().map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(trf(
            "Dangling images: removed {0}, freed {1}",
            &[&n.to_string(), &format_bytes(space as u64)],
        ));
    }

    if unused_net {
        let resp = docker
            .prune_networks(None)
            .await
            .map_err(|e| trf("Cleanup unused networks failed: {0}", &[&e.to_string()]))?;
        let n = resp.networks_deleted.as_ref().map_or(0, std::vec::Vec::len);
        lines.push(trf("Unused networks: removed {0}", &[&n.to_string()]));
    }

    if unused_vol {
        // 不加 all 过滤器：Docker 默认只 prune 匿名卷（与弹窗回填的「匿名卷」计数同口径）
        let options = PruneVolumesOptionsBuilder::new().build();
        let resp = docker
            .prune_volumes(Some(options))
            .await
            .map_err(|e| trf("Cleanup unused volumes failed: {0}", &[&e.to_string()]))?;
        let n = resp.volumes_deleted.as_ref().map_or(0, std::vec::Vec::len);
        let space = resp.space_reclaimed.unwrap_or(0);
        lines.push(trf(
            "Unused volumes: removed {0}, freed {1}",
            &[&n.to_string(), &format_bytes(space as u64)],
        ));
    }

    if lines.is_empty() {
        return Err(crate::i18n::t("Nothing to clean up"));
    }
    Ok(lines.join("\n"))
}
