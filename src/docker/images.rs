//! 镜像相关 API：列表、删除、拉取

use std::collections::HashMap;

use bollard::query_parameters::{ListImagesOptionsBuilder, RemoveImageOptionsBuilder};
use futures::StreamExt;

use crate::models::ImageInfo;

use super::connect;

/// 拉取镜像列表（`containers` 字段恒为 0，交给 [`attach_container_refs`] 回填）
///
/// Docker 的 `list_images` 不返回「这个镜像被几个容器引用」，要用容器列表现算。
/// **容器列表由调用方传入**：刷新与首屏本来就会拉一遍 `/containers/json`，
/// 以前在 `list_images` 内部再拉一遍 → 每次刷新多一次 30ms 请求 + 一次解析
///（实测 `/containers/json` 30ms、`/images/json` 88ms，重复那次把整轮从 88ms 拖到 118ms）。
pub async fn list_images_raw() -> Result<Vec<ImageInfo>, String> {
    let docker = connect()?;
    let options = ListImagesOptionsBuilder::new().all(true).build();
    let images = docker
        .list_images(Some(options))
        .await
        .map_err(|e| crate::i18n::trf("Failed to fetch image list: {0}", &[&e.to_string()]))?;

    Ok(images
        .into_iter()
        .map(|img| {
            let id = img.id.clone();
            let (name, tag) = match img.repo_tags.first() {
                Some(rt) => match rt.rsplit_once(':') {
                    Some((n, t)) => (n.to_string(), t.to_string()),
                    None => (rt.clone(), "latest".to_string()),
                },
                None => ("<none>".to_string(), "<none>".to_string()),
            };
            // 多 tag：repo_tag 形如 "registry:5000/name:tag"，取最后一个 ':' 之后部分
            let tags: Vec<String> = img
                .repo_tags
                .iter()
                .map(|rt| match rt.rsplit_once(':') {
                    Some((_, t)) if !t.contains('/') => t.to_string(),
                    _ => "latest".to_string(),
                })
                .collect();
            ImageInfo {
                id,
                name,
                tag,
                tags,
                size: img.size,
                created: img.created,
                // 关联容器数由 attach_container_refs 回填
                containers: 0,
                dangling: img.repo_tags.is_empty()
                    || img.repo_tags.iter().any(|t| t == "<none>:<none>"),
            }
        })
        .collect())
}

/// 用**已有**的容器列表回填每个镜像的「关联容器数」
///
/// 匹配规则：优先按 `image_id` 精确匹配（`sha256:` 前缀可选），
/// 匹配不到再按「镜像名:tag」字符串兜底 —— UI 靠它给「未使用镜像」打黄色高亮。
pub fn attach_container_refs(
    images: &mut [ImageInfo],
    containers: &[crate::models::ContainerInfo],
) {
    if images.is_empty() {
        return;
    }
    // 镜像名:tag → 引用该名字的容器数（image_id 匹配不到时的兜底）
    let mut by_name: HashMap<&str, i64> = HashMap::new();
    for c in containers {
        *by_name.entry(c.image.as_str()).or_insert(0) += 1;
    }
    for img in images.iter_mut() {
        let short = img.id.trim_start_matches("sha256:");
        let by_id = containers
            .iter()
            .filter(|c| {
                let cid = c.image_id.trim_start_matches("sha256:");
                !cid.is_empty() && (c.image_id == img.id || cid == short)
            })
            .count() as i64;
        img.containers = if by_id > 0 {
            by_id
        } else {
            img.tags
                .iter()
                .map(|t| {
                    // tags 只保留 tag 部分，需拼回 name:tag 才能与容器 image 字段对上
                    let full = format!("{}:{}", img.name, t);
                    by_name.get(full.as_str()).copied().unwrap_or(0)
                })
                .sum()
        };
    }
}

/// 单个镜像的层数（`RootFS.Layers.len()`）
///
/// 列表 API 不返回层数，必须 inspect；调用方负责缓存，避免重复 inspect。
pub async fn image_layer_count(id: &str) -> Result<usize, String> {
    let docker = connect()?;
    let img = docker
        .inspect_image(id)
        .await
        .map_err(|e| crate::i18n::trf("Image inspect failed ({0}): {1}", &[id, &e.to_string()]))?;
    Ok(img
        .root_fs
        .and_then(|r| r.layers)
        .map(|l| l.len())
        .unwrap_or(0))
}

/// 删除镜像
pub async fn remove_image(name: &str, tag: &str) -> Result<String, String> {
    let docker = connect()?;
    let reference = if tag.is_empty() || tag == "<none>" {
        name.to_string()
    } else {
        format!("{name}:{tag}")
    };
    let options = RemoveImageOptionsBuilder::new().force(true).build();
    docker
        .remove_image(&reference, Some(options), None)
        .await
        .map_err(|e| crate::i18n::trf("Failed to remove image: {0}", &[&e.to_string()]))?;
    Ok(crate::i18n::trf("Image {0} removed", &[&reference]))
}

/// 拉取镜像（spec 形如 "nginx:latest"，进度通过 `on_progress` 回调上报）
pub async fn pull_image(spec: &str, on_progress: impl Fn(String)) -> Result<String, String> {
    let spec = spec.trim();
    if spec.is_empty() {
        return Err(crate::i18n::t(
            "Please enter an image name, e.g. nginx:latest",
        ));
    }
    let docker = connect()?;

    // 拆分镜像名与 tag（最后一个 ':' 需位于最后一段路径之后，兼容 registry:5000/img）
    let (name, tag) = match spec.rsplit_once(':') {
        Some((n, t)) if !n.is_empty() && !t.contains('/') => (n.to_string(), t.to_string()),
        _ => (spec.to_string(), "latest".to_string()),
    };

    let options = bollard::query_parameters::CreateImageOptionsBuilder::new()
        .from_image(&name)
        .tag(&tag)
        .build();

    let mut stream = docker.create_image(Some(options), None, None);
    while let Some(item) = stream.next().await {
        match item {
            Ok(info) => {
                if let Some(err) = info.error_detail.as_ref().and_then(|e| e.message.clone()) {
                    return Err(crate::i18n::trf(
                        "Failed to pull image: {0}",
                        &[&err.to_string()],
                    ));
                }
                let mut text = info.status.unwrap_or_default();
                if let Some(detail) = info.progress_detail {
                    if let (Some(cur), Some(total)) = (detail.current, detail.total) {
                        if total > 0 {
                            text = format!("{text} ({:.1}%)", cur as f64 / total as f64 * 100.0);
                        }
                    }
                }
                if !text.trim().is_empty() {
                    on_progress(text);
                }
            }
            Err(e) => {
                return Err(crate::i18n::trf(
                    "Failed to pull image: {0}",
                    &[&e.to_string()],
                ))
            }
        }
    }
    Ok(crate::i18n::trf("{0}:{1} pulled", &[&name, &tag]))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ContainerInfo;

    fn container(image_id: &str, image: &str) -> ContainerInfo {
        ContainerInfo {
            id: format!("c-{image_id}"),
            name: "c".into(),
            image: image.into(),
            image_id: image_id.into(),
            state: "running".into(),
            status: String::new(),
            created: 0,
            ports: String::new(),
            cpu_percent: 0.0,
            mem_usage: 0,
            mem_limit: 0,
            net_rx: 0,
            net_tx: 0,
            size_rw: 0,
            disk_used: 0,
            compose_project: None,
            compose_service: None,
            networks: Vec::new(),
            volumes: Vec::new(),
        }
    }

    fn image(id: &str, name: &str, tag: &str) -> ImageInfo {
        ImageInfo {
            id: id.into(),
            name: name.into(),
            tag: tag.into(),
            tags: vec![tag.into()],
            size: 0,
            created: 0,
            containers: 0,
            dangling: false,
        }
    }

    #[test]
    fn attach_counts_by_image_id_and_skips_sha_prefix() {
        let mut images = vec![
            image("sha256:aaa", "demo/one", "local"),
            image("bbb", "demo/two", "local"),
        ];
        let containers = vec![
            container("aaa", "demo/one:local"),
            container("sha256:aaa", "demo/one:local"),
            container("bbb", "demo/two:local"),
        ];
        attach_container_refs(&mut images, &containers);
        // 两条 id 引用都算（容器侧带不带 sha256: 前缀都要命中）
        assert_eq!(images[0].containers, 2);
        assert_eq!(images[1].containers, 1);
    }

    #[test]
    fn attach_falls_back_to_name_tag_when_id_misses() {
        let mut images = vec![image("sha256:ccc", "demo/three", "dev")];
        // 容器记录的 image_id 与镜像 id 不一致（重新 tag 过），靠 name:tag 兜底
        let containers = vec![container("other", "demo/three:dev")];
        attach_container_refs(&mut images, &containers);
        assert_eq!(images[0].containers, 1);
    }

    #[test]
    fn attach_leaves_unused_images_at_zero() {
        let mut images = vec![image("sha256:ddd", "demo/unused", "local")];
        attach_container_refs(&mut images, &[]);
        assert_eq!(images[0].containers, 0);
    }
}
