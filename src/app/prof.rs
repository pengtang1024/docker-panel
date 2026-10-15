//! 帧耗时插桩：DV_PROFILE=1 时统计 CPU 更新耗时与帧间隔，分离 CPU/GPU 嫌疑
//!
//! 判定逻辑：
//! - cpu 短、delta 长 → 时间花在 tessellate/render/present（GPU 呈现路径）
//! - cpu 长 → 应用自身布局/绘制代码慢

use std::time::Instant;

/// 每积累这么多帧输出一行统计
const REPORT_EVERY: u64 = 120;

/// 单组累计统计（拖动中与静止分开记）
#[derive(Default)]
struct Acc {
    frames: u64,
    cpu_us: u128,
    delta_us: u128,
    cpu_max_us: u128,
    delta_max_us: u128,
}

impl Acc {
    fn push(&mut self, cpu_us: u128, delta_us: u128) {
        self.frames += 1;
        self.cpu_us += cpu_us;
        self.delta_us += delta_us;
        self.cpu_max_us = self.cpu_max_us.max(cpu_us);
        self.delta_max_us = self.delta_max_us.max(delta_us);
    }

    /// 输出并重置，返回是否有效（无帧时不打印）
    fn report(&mut self, tag: &str) {
        if self.frames == 0 {
            return;
        }
        let n = self.frames as u128;
        eprintln!(
            "[prof] {tag}: n={} cpu_avg={:.1}ms cpu_max={:.1}ms delta_avg={:.1}ms delta_max={:.1}ms",
            self.frames,
            self.cpu_us as f64 / n as f64 / 1000.0,
            self.cpu_max_us as f64 / 1000.0,
            self.delta_us as f64 / n as f64 / 1000.0,
            self.delta_max_us as f64 / 1000.0,
        );
        *self = Acc::default();
    }
}

pub struct FrameProfiler {
    enabled: bool,
    begin: Instant,
    last_end: Option<Instant>,
    total: u64,
    idle: Acc,
    drag: Acc,
}

impl FrameProfiler {
    pub fn new() -> Self {
        Self {
            enabled: std::env::var_os("DV_PROFILE").is_some(),
            begin: Instant::now(),
            last_end: None,
            total: 0,
            idle: Acc::default(),
            drag: Acc::default(),
        }
    }

    /// ui() 起点打点
    pub fn begin(&mut self) {
        if self.enabled {
            self.begin = Instant::now();
        }
    }

    /// ui() 终点打点：累计并按周期输出
    pub fn end(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }
        let now = Instant::now();
        let cpu_us = (now - self.begin).as_micros();
        // 首帧无上一帧，跳过 delta 统计
        let Some(last) = self.last_end.replace(now) else {
            return;
        };
        let delta_us = (now - last).as_micros();
        self.total += 1;

        let dragging = ctx.input(|i| i.pointer.is_decidedly_dragging());
        if dragging {
            self.drag.push(cpu_us, delta_us);
        } else {
            self.idle.push(cpu_us, delta_us);
        }

        if self.total.is_multiple_of(REPORT_EVERY) {
            eprintln!("[prof] ---- {} 帧 ----", REPORT_EVERY);
            self.idle.report("静止");
            self.drag.report("拖动");
        }
    }
}
