//! 镜像详情：inspect_image → ImageDetail

use bollard::config::ImageInspect;

use super::connect;
use crate::models::ImageDetail;

/// inspect 单个镜像
pub async fn inspect_image(name: &str) -> Result<ImageDetail, String> {
    let docker = connect()?;
    let img = docker
        .inspect_image(name)
        .await
        .map_err(|e| format!("镜像详情失败 ({name}): {e}"))?;
    Ok(map_image_inspect(img))
}

fn map_image_inspect(img: ImageInspect) -> ImageDetail {
    let id = img.id.unwrap_or_default();
    let short_id = if id.len() > 12 {
        id[7..19].to_string() // sha256:xxxxxx... 的 12 字符
    } else {
        id.clone()
    };

    let fmt_list = |list: Option<Vec<String>>| -> String { list.unwrap_or_default().join(", ") };

    let entrypoint = img
        .config
        .as_ref()
        .and_then(|c| c.entrypoint.as_ref())
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let cmd = img
        .config
        .as_ref()
        .and_then(|c| c.cmd.as_ref())
        .map(|v| v.join(" "))
        .unwrap_or_default();
    let working_dir = img
        .config
        .as_ref()
        .and_then(|c| c.working_dir.clone())
        .unwrap_or_default();
    let labels = img
        .config
        .as_ref()
        .and_then(|c| c.labels.as_ref())
        .map(|m| {
            m.iter()
                .map(|(k, v)| format!("{k}={v}"))
                .collect::<Vec<_>>()
                .join("\n")
        })
        .unwrap_or_default();
    let env = img
        .config
        .as_ref()
        .and_then(|c| c.env.as_ref())
        .map(|v| v.join("\n"))
        .unwrap_or_default();
    let exposed_ports = img
        .config
        .as_ref()
        .and_then(|c| c.exposed_ports.as_ref())
        .map(|v| v.join(", "))
        .unwrap_or_default();
    let volumes = img
        .config
        .as_ref()
        .and_then(|c| c.volumes.as_ref())
        .map(|v| v.join(", "))
        .unwrap_or_default();

    let size = img
        .size
        .map(|s| {
            if s > 1024 * 1024 * 1024 {
                format!("{:.1} GB", s as f64 / (1024.0 * 1024.0 * 1024.0))
            } else if s > 1024 * 1024 {
                format!("{:.1} MB", s as f64 / (1024.0 * 1024.0))
            } else if s > 1024 {
                format!("{:.1} KB", s as f64 / 1024.0)
            } else {
                format!("{s} B")
            }
        })
        .unwrap_or_default();

    ImageDetail {
        id,
        short_id,
        repo_tags: fmt_list(img.repo_tags),
        repo_digests: fmt_list(img.repo_digests),
        created: img.created.unwrap_or_default(),
        author: img.author.unwrap_or_default(),
        size,
        os: img.os.unwrap_or_default(),
        architecture: img.architecture.unwrap_or_default(),
        entrypoint,
        cmd,
        working_dir,
        labels,
        env,
        exposed_ports,
        volumes,
    }
}
