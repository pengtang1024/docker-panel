//! 镜像列表行构建（多 tag 芯片 + 层数缓存 + 相对时间 + 排序）

use std::cmp::Ordering;

use slint::SharedString;

use crate::detail_fmt::{dash, hit_any, rel_time};
use crate::models::ImageInfo;
use crate::slint_gen::ImageRow;
use crate::ui_rows::sort;
use crate::ui_state::AppState;

pub fn build_image_rows(s: &AppState) -> Vec<ImageRow> {
    sort::build(
        &s.images,
        s.sort_images,
        |img| {
            let tags_join = img.tags.join(" ");
            hit_any(&s.image_query, &[&img.name, img.short_id(), &tags_join])
        },
        cmp_images,
        |img| {
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
                size: SharedString::from(crate::format::format_bytes(img.size.max(0) as u64)),
                layers: SharedString::from(layers),
                created: SharedString::from(rel_time(img.created)),
                containers: img.containers as i32,
                tag1: SharedString::from(tags.first().cloned().unwrap_or_default()),
                tag2: SharedString::from(tags.get(1).cloned().unwrap_or_default()),
                tag3: SharedString::from(tags.get(2).cloned().unwrap_or_default()),
                tags_more: (tags.len() as i32 - 3).max(0),
                tags_text: SharedString::from(tags.join(", ")),
            }
        },
    )
}

/// 镜像字段比较器（0=名称 1=大小 2=创建时间）
///
/// ⚠️ 基线一律自然升序（小的在前），`desc` 只负责反向 —— 与容器台账同口径。
fn cmp_images(a: &ImageInfo, b: &ImageInfo, field: u8) -> Ordering {
    match field {
        1 => a.size.cmp(&b.size),
        2 => a.created.cmp(&b.created),
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ResourceSort;

    fn image(name: &str, size: i64, created: i64) -> ImageInfo {
        ImageInfo {
            id: format!("id-{name}"),
            name: name.to_string(),
            tag: "latest".to_string(),
            tags: vec!["latest".to_string()],
            size,
            created,
            containers: 0,
            dangling: false,
        }
    }

    /// 过滤 → 排序 → 映射 走与生产同一条流水线，只保留名称便于断言
    fn sorted_names(list: &[ImageInfo], sort: ResourceSort) -> Vec<String> {
        self::sort::build(list, sort, |_| true, cmp_images, |i| i.name.clone())
    }

    fn by_field(field: u8, desc: bool) -> ResourceSort {
        ResourceSort { field, desc }
    }

    /// 基线一律自然升序，desc 只负责反向（规范 #21 的同口径延伸到资源列表）
    #[test]
    fn size_sort_is_ascending_then_reversed() {
        let list = [
            image("mid", 500, 0),
            image("big", 900, 0),
            image("small", 100, 0),
        ];
        assert_eq!(
            sorted_names(&list, by_field(1, false)),
            vec!["small", "mid", "big"]
        );
        assert_eq!(
            sorted_names(&list, by_field(1, true)),
            vec!["big", "mid", "small"]
        );
    }

    #[test]
    fn created_sort_is_ascending_then_reversed() {
        let list = [image("b", 0, 200), image("c", 0, 300), image("a", 0, 100)];
        assert_eq!(sorted_names(&list, by_field(2, false)), vec!["a", "b", "c"]);
        assert_eq!(sorted_names(&list, by_field(2, true)), vec!["c", "b", "a"]);
    }

    /// 名称列忽略大小写（与台账名称列一致）
    #[test]
    fn name_sort_is_case_insensitive() {
        let list = [image("Bravo", 0, 0), image("alpha", 0, 0)];
        assert_eq!(
            sorted_names(&list, by_field(0, false)),
            vec!["alpha", "Bravo"]
        );
    }
}
