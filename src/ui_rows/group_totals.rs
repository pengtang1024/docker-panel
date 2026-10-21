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

/// 各分组**总和**里的 CPU / 内存最大值（分组带参与列基准的那一半）
///
/// 列基准 = max(行的单容器最大值, 分组总和的最大值) —— 行与分组**共用**同一个基准，
/// 微条才具备「列内可比」的意义（条长与数值成正比）。
/// 分组的和 ≥ 组内任一成员 → 分组总和通常就是本列最大值，行的条按比例缩短。
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
