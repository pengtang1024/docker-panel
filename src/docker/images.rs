//! 镜像相关 API：列表、删除、拉取

use std::collections::HashMap;

use bollard::query_parameters::{ListImagesOptionsBuilder, RemoveImageOptionsBuilder};
use futures::StreamExt;

use crate::models::ImageInfo;

use super::{connect, list_containers};

/// 拉取镜像列表 + 关联容器数量
///
/// 为什么 list_images 还要偷偷调一遍 list_containers：
///   Docker API 的 list_images 不返回"这个镜像被几个容器引用"。
///   所以我们额外调 list_containers(true)，然后：
///     1. 优先按 image_id 精确匹配（sha256: 前缀可选）
///     2. 匹配不到时按"镜像名:tag"字符串匹配兜底
///   这样能给 UI 展示"未使用镜像"黄色高亮提示。
pub async fn list_images() -> Result<Vec<ImageInfo>, String> {
    let docker = connect()?;
    let options = ListImagesOptionsBuilder::new().all(true).build();
    let images = docker
        .list_images(Some(options))
        .await
        .map_err(|e| crate::i18n::trf("Failed to fetch image list: {0}", &[&e.to_string()]))?;
    let containers = list_containers(true).await.unwrap_or_default();

    // 镜像名:tag → 关联容器数（在 image_id 匹配不到时兜底）
    let mut count: HashMap<String, i64> = HashMap::new();
    for c in &containers {
        *count.entry(c.image.clone()).or_insert(0) += 1;
    }

    Ok(images
        .into_iter()
        .map(|img| {
            let id = img.id.clone();
            let short = id.trim_start_matches("sha256:").to_string();
            // 关联容器：优先按 image_id 精确匹配，其次按镜像名匹配
            let by_id = containers
                .iter()
                .filter(|c| {
                    let cid = c.image_id.trim_start_matches("sha256:");
                    !cid.is_empty() && (c.image_id == id || cid == short)
                })
                .count() as i64;
            let by_name = if by_id > 0 {
                0
            } else {
                img.repo_tags
                    .iter()
                    .map(|t| count.get(t.as_str()).copied().unwrap_or(0))
                    .sum()
            };
            let (name, tag) = match img.repo_tags.first() {
                Some(rt) => match rt.rsplit_once(':') {
                    Some((n, t)) => (n.to_string(), t.to_string()),
                    None => (rt.clone(), "latest".to_string()),
                },
                None => ("<none>".to_string(), "<none>".to_string()),
            };
            ImageInfo {
                id,
                name,
                tag,
                size: img.size,
                created: img.created,
                containers: by_id + by_name,
                dangling: img.repo_tags.is_empty()
                    || img.repo_tags.iter().any(|t| t == "<none>:<none>"),
            }
        })
        .collect())
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
