//! 镜像列表行构建（多 tag 芯片 + 层数缓存 + 相对时间 + 排序）

use slint::SharedString;

use crate::detail_fmt::{dash, format_bytes, hit_any, rel_time};
use crate::models::{ImageInfo, ResourceSort};
use crate::slint_gen::ImageRow;
use crate::ui_state::AppState;

pub fn build_image_rows(s: &AppState) -> Vec<ImageRow> {
    let mut list: Vec<&ImageInfo> = s
        .images
        .iter()
        .filter(|img| {
            let tags_join = img.tags.join(" ");
            hit_any(&s.image_query, &[&img.name, img.short_id(), &tags_join])
        })
        .collect();
    sort_images(&mut list, s.sort_images);
    list.into_iter()
        .map(|img| {
            // 列表 API 不返回层数，由后台 inspect 回填；未回填时占位
            let layers = match s.image_layers.get(&img.id) {
                Some(n) if *n > 0 => n.to_string(),
                _ => dash(),
            };
            let tags = &img.tags;
            ImageRow {
                id: SharedString::from(img.key()),
                short_id: SharedString::from(img.short_id()),
                name: SharedString::from(img.name.clone()),
                size: SharedString::from(format_bytes(img.size.max(0) as u64)),
                layers: SharedString::from(layers),
                created: SharedString::from(rel_time(img.created)),
                containers: img.containers as i32,
                tag1: SharedString::from(tags.first().cloned().unwrap_or_default()),
                tag2: SharedString::from(tags.get(1).cloned().unwrap_or_default()),
                tag3: SharedString::from(tags.get(2).cloned().unwrap_or_default()),
                tags_more: (tags.len() as i32 - 3).max(0),
                tags_text: SharedString::from(tags.join(", ")),
            }
        })
        .collect()
}

/// 镜像排序（0=名称 1=大小 2=创建时间）
///
/// ⚠️ 基线比较器一律自然升序（小的在前），`desc` 只负责反向 —— 与容器台账同口径。
fn sort_images(list: &mut [&ImageInfo], sort: ResourceSort) {
    list.sort_by(|a, b| {
        let cmp = match sort.field {
            1 => a.size.cmp(&b.size),
            2 => a.created.cmp(&b.created),
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        };
        if sort.desc {
            cmp.reverse()
        } else {
            cmp
        }
    });
}
