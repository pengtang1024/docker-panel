//! 资源列表（镜像 / 网络 / 卷）通用的排序与行构建骨架
//!
//! 三个列表结构同构：按查询过滤 → 按字段排序 → 映射成 Slint 行。
//! 差异只在「过滤字段 / 字段比较器 / 行类型」，统一由本模块承载，
//! 各列表只保留自己的比较器与映射闭包。

use std::cmp::Ordering;

use crate::models::ResourceSort;

/// 过滤 → 排序 → 映射 的标准流水线
///
/// `cmp` 必须返回**自然升序**的比较结果：`sort.desc` 只负责整体反向
/// （规范 #21：排序基线一律升序，否则 ↓ 箭头与顺序不一致）。
pub fn build<T, R>(
    items: &[T],
    sort: ResourceSort,
    keep: impl Fn(&T) -> bool,
    cmp: impl Fn(&T, &T, u8) -> Ordering,
    map: impl Fn(&T) -> R,
) -> Vec<R> {
    let mut list: Vec<&T> = items.iter().filter(|x| keep(x)).collect();
    list.sort_by(|a, b| {
        let ord = cmp(*a, *b, sort.field);
        if sort.desc {
            ord.reverse()
        } else {
            ord
        }
    });
    list.into_iter().map(map).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn asc() -> ResourceSort {
        ResourceSort {
            field: 0,
            desc: false,
        }
    }

    /// desc 只负责反向，基线永远是自然升序（规范 #21）
    #[test]
    fn desc_only_reverses_natural_order() {
        let items = [2, 3, 1];
        let asc_out = build(&items, asc(), |_| true, |a, b, _| a.cmp(b), |v| *v);
        assert_eq!(asc_out, vec![1, 2, 3]);

        let desc_out = build(
            &items,
            ResourceSort {
                field: 0,
                desc: true,
            },
            |_| true,
            |a, b, _| a.cmp(b),
            |v| *v,
        );
        assert_eq!(desc_out, vec![3, 2, 1]);
    }

    /// 过滤先于排序：被过滤掉的项不参与排序也不出现在结果里
    #[test]
    fn filter_runs_before_sort() {
        let items = ["b", "a", "c"];
        let out = build(
            &items,
            asc(),
            |s| *s != "c",
            |a, b, _| a.cmp(b),
            |s| s.to_string(),
        );
        assert_eq!(out, vec!["a".to_string(), "b".to_string()]);
    }
}
