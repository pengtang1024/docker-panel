//! 容器 stats —— CPU / 内存 / 网络使用率
//!
//! Docker stats API 返回**累计值**，CPU% 需双采样差值：
//!   cpu_delta = curr - prev
//!   cpu_percent = (cpu_delta / system_delta) * online_cpus * 100
//!
//! bollard 的 docker.stats() 返回 Stream，stream=false 时也需要
//! `.next().await` 取第一帧。

use bollard::query_parameters::StatsOptionsBuilder;
use futures_util::StreamExt;

use super::connect;
use super::with_timeout;

#[derive(Debug, Clone, Copy, Default)]
pub struct ContainerStatsSnapshot {
    pub cpu_total_usage: u64,
    pub system_cpu_usage: u64,
    pub online_cpus: u32,
    pub mem_usage: u64,
    pub mem_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
    /// Docker 自带的「上一次采样」CPU 累计值（precpu_stats）
    ///
    /// 非流式 stats 请求里，守护进程会先采一帧 pre、等约 1s 再采当前帧，
    /// 两者一并返回 → **首次采样就能算出 CPU%**，不必等下一个采样周期。
    pub pre_cpu_total_usage: u64,
    pub pre_system_cpu_usage: u64,
    pub pre_online_cpus: u32,
}

impl ContainerStatsSnapshot {
    /// 把 Docker 返回的 pre 帧还原成一份快照，可直接充当 `compute_cpu_delta` 的 prev
    ///
    /// 首次采样时本地还没有 prev（要等 6s 后的下一轮才有）→ 用它兜底，
    /// 启动后第一帧就有真实 CPU%，而不是先显示一轮 0.0%。
    pub fn precpu(&self) -> Option<ContainerStatsSnapshot> {
        if self.pre_system_cpu_usage == 0 {
            return None;
        }
        Some(ContainerStatsSnapshot {
            cpu_total_usage: self.pre_cpu_total_usage,
            system_cpu_usage: self.pre_system_cpu_usage,
            online_cpus: self.pre_online_cpus.max(1),
            ..Default::default()
        })
    }
}

pub async fn fetch_stats(id: &str) -> Result<ContainerStatsSnapshot, String> {
    let docker = connect()?;
    let opts = StatsOptionsBuilder::default().stream(false).build();
    let resp = docker
        .stats(id, Some(opts))
        .next()
        .await
        .ok_or_else(|| crate::i18n::t("Empty stats response"))?
        .map_err(|e| crate::i18n::trf("Failed to fetch stats: {0}", &[&e.to_string()]))?;

    let cpu = resp.cpu_stats.as_ref();
    let cpu_usage = cpu.and_then(|c| c.cpu_usage.as_ref());
    let mem = resp.memory_stats.as_ref();
    let networks = resp.networks.as_ref();
    let pre = resp.precpu_stats.as_ref();
    let pre_usage = pre.and_then(|c| c.cpu_usage.as_ref());

    Ok(ContainerStatsSnapshot {
        cpu_total_usage: cpu_usage.and_then(|u| u.total_usage).unwrap_or(0),
        system_cpu_usage: cpu.and_then(|c| c.system_cpu_usage).unwrap_or(0),
        online_cpus: cpu.and_then(|c| c.online_cpus).unwrap_or(1) as u32,
        mem_usage: mem.and_then(|m| m.usage).unwrap_or(0),
        mem_limit: mem.and_then(|m| m.limit).unwrap_or(0),
        net_rx: networks
            .map(|n| n.values().filter_map(|v| v.rx_bytes).sum())
            .unwrap_or(0),
        net_tx: networks
            .map(|n| n.values().filter_map(|v| v.tx_bytes).sum())
            .unwrap_or(0),
        pre_cpu_total_usage: pre_usage.and_then(|u| u.total_usage).unwrap_or(0),
        pre_system_cpu_usage: pre.and_then(|c| c.system_cpu_usage).unwrap_or(0),
        pre_online_cpus: pre.and_then(|c| c.online_cpus).unwrap_or(1) as u32,
    })
}

/// 并发采样一批容器的 stats（失败的容器跳过）
///
/// 整轮采样打包成一条消息发出：逐容器单发会让 UI 数值分批阶梯跳动。
pub async fn sample_all(ids: &[String]) -> Vec<(String, ContainerStatsSnapshot)> {
    let futs: Vec<_> = ids
        .iter()
        .map(|id| async move {
            with_timeout(fetch_stats(id))
                .await
                .ok()
                .map(|snap| (id.clone(), snap))
        })
        .collect();
    futures::future::join_all(futs)
        .await
        .into_iter()
        .flatten()
        .collect()
}

/// 纯函数：传入上一次和当前快照，算出 CPU 百分比
/// 返回 0.0 表示无法计算（prev 为空 / delta 为 0）
pub fn compute_cpu_delta(
    prev: Option<&ContainerStatsSnapshot>,
    curr: &ContainerStatsSnapshot,
) -> f64 {
    let prev = match prev {
        Some(p) => p,
        None => return 0.0,
    };
    let cpu_delta = curr.cpu_total_usage.saturating_sub(prev.cpu_total_usage);
    let sys_delta = curr.system_cpu_usage.saturating_sub(prev.system_cpu_usage);
    if sys_delta == 0 || cpu_delta == 0 {
        return 0.0;
    }
    let online = curr.online_cpus.max(1) as f64;
    (cpu_delta as f64 / sys_delta as f64) * online * 100.0
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 首次采样（本地还没有 prev）时，用 Docker 同帧返回的 pre 帧兜底 → CPU% 立即可用
    #[test]
    fn precpu_frame_yields_cpu_on_first_sample() {
        let curr = ContainerStatsSnapshot {
            cpu_total_usage: 2_000_000,
            system_cpu_usage: 100_000_000,
            online_cpus: 4,
            pre_cpu_total_usage: 1_000_000,
            pre_system_cpu_usage: 50_000_000,
            pre_online_cpus: 4,
            ..Default::default()
        };
        let pre = curr.precpu().expect("pre frame present");
        // (2M - 1M) / (100M - 50M) * 4 核 * 100 = 8.0%
        let cpu = compute_cpu_delta(Some(&pre), &curr);
        assert!((cpu - 8.0).abs() < 0.01, "cpu = {cpu}");
    }

    /// 守护进程没给 pre 帧时退回 None（调用方再走「无 prev → 0」的老路）
    #[test]
    fn missing_precpu_frame_returns_none() {
        assert!(ContainerStatsSnapshot::default().precpu().is_none());
    }

    /// 没有 prev 也没有 pre 帧 → CPU 为 0（而不是算出离谱的值）
    #[test]
    fn no_prev_yields_zero_cpu() {
        let curr = ContainerStatsSnapshot {
            cpu_total_usage: 2_000_000,
            system_cpu_usage: 100_000_000,
            online_cpus: 4,
            ..Default::default()
        };
        assert_eq!(compute_cpu_delta(None, &curr), 0.0);
    }
}
