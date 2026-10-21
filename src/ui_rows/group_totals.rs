//! 分组资源汇总：组内成员的资源之和
//!
//! 分组带（项目带 / 服务带）显示的不是某个容器的资源，而是**组内成员的总和**：
//! 汇总描述的是「这个组占了多少」，与组是否展开无关 → 收起时数字照旧。
//!
//! 口径与容器行保持一致：
//!   - CPU / 内存只累加**运行中**成员（停止的容器没有实时占用，行上显示 "—"）
//!   - 磁盘累加**全部**成员（可写层占用是累计值，停止后依然成立）

use slint::SharedString;

use crate::detail_fmt::format_bytes_compact;
use crate::models::ContainerInfo;

/// 分组资源汇总（组内成员之和）
#[derive(Default, Clone, Copy)]
pub struct GroupTotals {
    /// 运行中成员数（= CPU / 内存汇总是否有效的判据）
    running: usize,
    cpu: f64,
    mem: u64,
    disk: u64,
}

impl GroupTotals {
    /// 按成员列表累加
    pub fn of(members: &[&ContainerInfo]) -> Self {
        let mut t = Self::default();
        for c in members {
            if c.is_running() {
                t.running += 1;
                t.cpu += c.cpu_percent;
                t.mem += c.mem_usage;
            }
            t.disk += c.size_rw;
        }
        t
    }

    /// CPU 汇总（%）：只含运行中成员
    pub fn cpu(&self) -> f64 {
        self.cpu
    }

    /// 内存汇总（字节）：只含运行中成员
    pub fn mem(&self) -> u64 {
        self.mem
    }

    /// CPU 汇总文本（无运行成员 → "—"，与容器行口径一致）
    pub fn cpu_text(&self) -> SharedString {
        if self.running == 0 {
            SharedString::from("—")
        } else {
            SharedString::from(format!("{:.1}%", self.cpu))
        }
    }

    /// 内存汇总文本
    pub fn mem_text(&self) -> SharedString {
        if self.running == 0 {
            SharedString::from("—")
        } else {
            SharedString::from(format_bytes_compact(self.mem))
        }
    }

    /// 磁盘汇总文本（全部成员累加）
    pub fn disk_text(&self) -> SharedString {
        SharedString::from(format_bytes_compact(self.disk))
    }
}

/// 各分组**总和**里的 CPU / 内存最大值（分组带微条的刻度）
///
/// 微条两套刻度：行按「视图内最大的单容器」归一，分组带按本函数给出的
/// 「各分组总和的最大值」归一。行与分组是两种量纲（个体 / 聚合），
/// 共用一个刻度必然失真（详见 `compose_tree.rs` 的 4b 注释）。
pub fn max_base(totals: &[&GroupTotals]) -> (f64, u64) {
    (
        totals.iter().map(|t| t.cpu()).fold(0.0f64, f64::max),
        totals.iter().map(|t| t.mem()).max().unwrap_or(0),
    )
}

/// 微条比例：值 / 基准（基准为 0 → 0）
pub fn ratio(value: f64, max: f64) -> f32 {
    if max > 0.0 {
        (value / max).clamp(0.0, 1.0) as f32
    } else {
        0.0
    }
}
