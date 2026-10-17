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

#[derive(Debug, Clone, Default)]
pub struct ContainerStatsSnapshot {
    pub cpu_total_usage: u64,
    pub system_cpu_usage: u64,
    pub online_cpus: u32,
    pub mem_usage: u64,
    pub mem_limit: u64,
    pub net_rx: u64,
    pub net_tx: u64,
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
    })
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
