//! 分组资源汇总：组内成员的资源之和
//!
//! 分组带（项目带 / 服务带）显示的不是某个容器的资源，而是**组内成员的总和**：
//! 汇总描述的是「这个组占了多少」，与组是否展开无关 → 收起时数字照旧。
//!
//! 口径与容器行保持一致：
//!   - CPU / 内存只累加**运行中**成员（停止的容器没有实时占用，行上显示 "—"）
//!   - 磁盘累加**全部**成员（可写层占用是累计值，停止后依然成立）
//!
//! 微条口径 = **docker stats 口径**（见 `containers::container_row`）：
//!   - CPU：相对**单核**的百分比（100% = 占满一核）
//!   - 内存：占 **limit** 的比例（100% = 占满配额）
//!
//! 分组带的条 = **组内成员条之和**（成员条首尾相接的长度），
//! 故父子关系自洽：组条的视觉长度 = 它的成员条长度之和。

use slint::SharedString;

use crate::detail_fmt::format_bytes_compact;
use crate::models::ContainerInfo;
use crate::ui_rows::containers::bar_ratio;

/// 分组资源汇总（组内成员之和）
#[derive(Default, Clone, Copy)]
pub struct GroupTotals {
    /// 运行中成员数（= CPU / 内存汇总是否有效的判据）
    running: usize,
    cpu: f64,
    mem: u64,
    disk: u64,
    /// 组内成员 **CPU 条** 之和（每人 cpu%/100）
    cpu_ratio: f64,
    /// 组内成员 **内存条** 之和（每人 usage/limit）
    mem_ratio: f64,
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
                // 条比例累加**补偿后**的成员条 → 组条 = 成员条之和严格成立
                t.cpu_ratio += bar_ratio(c.cpu_percent / 100.0);
                t.mem_ratio += bar_ratio(crate::ui_rows::containers::mem_ratio(c));
            }
            t.disk += c.size_rw;
        }
        t
    }

    /// 运行中的成员数（分组带的「全部运行中」判定依据）
    pub fn running(&self) -> usize {
        self.running
    }

    /// CPU 微条比例（= 成员条之和，1.0 = 占满一核）
    pub fn cpu_ratio(&self) -> f64 {
        self.cpu_ratio
    }

    /// 内存微条比例（= 成员条之和，1.0 = 占满配额）
    pub fn mem_ratio(&self) -> f64 {
        self.mem_ratio
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

/// 微条比例：docker 口径的绝对占比（1.0 = 满格），超出的部分截断到 1.0
///
/// 分组带的条 = 成员条之和，组内成员合计超过一核 / 满配额时只能满格
/// （数字仍显示真实值，不截断）。
pub fn ratio_abs(value: f64) -> f32 {
    value.clamp(0.0, 1.0) as f32
}
