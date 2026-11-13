//! `ResourceSort` 位打包契约的往返 / 越界回落测试。
//!
//! 编码：低 3 位 = 列下标，第 3 位 = 降序标记（与 `ui/list-area.slint`
//! 的 `SortCodec` 共用同一套约定）。注意 **8 / 9 是合法编码**
//! （列 0 / 列 1 的降序），真正越界的是 3..=7、11+、255 与负数。

use super::ResourceSort;

/// 三个视图的合法列下标都是 0..=2
const FIELDS: [u8; 3] = [0, 1, 2];

#[test]
fn roundtrip_every_column_and_direction() {
    for field in FIELDS {
        for desc in [false, true] {
            let sort = ResourceSort { field, desc };
            assert_eq!(ResourceSort::from_config(sort.to_config()), sort);
            assert_eq!(
                ResourceSort::from_config(i64::from(sort.to_ui())),
                sort,
                "to_ui 与 to_config 必须是同一编码"
            );
        }
    }
}

#[test]
fn packed_layout_is_column_plus_desc_bit() {
    for field in FIELDS {
        assert_eq!(
            ResourceSort { field, desc: false }.to_ui(),
            i32::from(field)
        );
        assert_eq!(
            ResourceSort { field, desc: true }.to_ui(),
            i32::from(field) + 8
        );
    }
}

#[test]
fn eight_and_nine_are_legal_desc_encodings() {
    assert_eq!(
        ResourceSort::from_config(8),
        ResourceSort {
            field: 0,
            desc: true
        }
    );
    assert_eq!(
        ResourceSort::from_config(9),
        ResourceSort {
            field: 1,
            desc: true
        }
    );
    assert_eq!(
        ResourceSort::from_config(10),
        ResourceSort {
            field: 2,
            desc: true
        }
    );
}

#[test]
fn out_of_range_falls_back_to_default() {
    let default = ResourceSort::default();
    // 3..=7：列下标越界；11+ / 255：高位置位；负数：非法
    for v in [3, 4, 5, 6, 7, 11, 12, 255, -1, -8, i64::MIN] {
        assert_eq!(ResourceSort::from_config(v), default, "值 {v} 应回落默认");
    }
}
