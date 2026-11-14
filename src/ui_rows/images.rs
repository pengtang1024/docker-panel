//! 镜像列表行构建（多 tag 芯片 + 层数缓存 + 相对时间 + 排序）

use slint::SharedString;

use crate::detail_fmt::{dash, hit_any, rel_time};
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

#[cfg(test)]
mod tests {
    use super::*;

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

    fn names(sorted: &[&ImageInfo]) -> Vec<String> {
        sorted.iter().map(|i| i.name.clone()).collect()
    }

    /// 基线一律自然升序，desc 只负责反向（规范 #21 的同口径延伸到资源列表）
    #[test]
    fn size_sort_is_ascending_then_reversed() {
        let list = [
            image("mid", 500, 0),
            image("big", 900, 0),
            image("small", 100, 0),
        ];
        let mut refs: Vec<&ImageInfo> = list.iter().collect();
        sort_images(
            &mut refs,
            ResourceSort {
                field: 1,
                desc: false,
            },
        );
        assert_eq!(names(&refs), vec!["small", "mid", "big"]);

        let mut refs: Vec<&ImageInfo> = list.iter().collect();
        sort_images(
            &mut refs,
            ResourceSort {
                field: 1,
                desc: true,
            },
        );
        assert_eq!(names(&refs), vec!["big", "mid", "small"]);
    }

    #[test]
    fn created_sort_is_ascending_then_reversed() {
        let list = [image("b", 0, 200), image("c", 0, 300), image("a", 0, 100)];
        let mut refs: Vec<&ImageInfo> = list.iter().collect();
        sort_images(
            &mut refs,
            ResourceSort {
                field: 2,
                desc: false,
            },
        );
        assert_eq!(names(&refs), vec!["a", "b", "c"]);

        let mut refs: Vec<&ImageInfo> = list.iter().collect();
        sort_images(
            &mut refs,
            ResourceSort {
                field: 2,
                desc: true,
            },
        );
        assert_eq!(names(&refs), vec!["c", "b", "a"]);
    }

    /// 名称列忽略大小写（与台账名称列一致）
    #[test]
    fn name_sort_is_case_insensitive() {
        let list = [image("Bravo", 0, 0), image("alpha", 0, 0)];
        let mut refs: Vec<&ImageInfo> = list.iter().collect();
        sort_images(
            &mut refs,
            ResourceSort {
                field: 0,
                desc: false,
            },
        );
        assert_eq!(names(&refs), vec!["alpha", "Bravo"]);
    }
}
