//! 后台任务启动：stats 采样 interval + 首屏数据加载

use crate::docker::{
    fetch_stats, inspect_container, list_containers, list_images, list_networks, list_volumes,
    volume_sizes,
};
use crate::models::Msg;
use crate::ui_state::{SharedState, VOLUME_SIZE_INTERVAL};

/// 6s stats 采样：对所有运行中容器并发 docker.stats()，双采样 CPU% 在 consumer 侧计算
pub fn spawn_stats_interval(state: &SharedState) {
    let state_s = state.clone();
    let rt_h = {
        let Ok(s) = state_s.lock() else { return };
        s.rt.handle().clone()
    };
    rt_h.spawn(async move {
        let mut tick = tokio::time::interval(std::time::Duration::from_secs(6));
        tick.tick().await; // 跳过首次立即触发
        loop {
            tick.tick().await;
            let (tx, ids): (_, Vec<String>) = {
                let Ok(s) = state_s.lock() else { continue };
                (
                    s.tx.clone(),
                    s.containers
                        .iter()
                        .filter(|c| c.is_running())
                        .map(|c| c.id.clone())
                        .collect(),
                )
            };
            let futs: Vec<_> = ids
                .into_iter()
                .map(|id| {
                    let tx = tx.clone();
                    async move {
                        if let Ok(snap) = fetch_stats(&id).await {
                            let _ = tx.send(Msg::Stats(id, snap));
                        }
                    }
                })
                .collect();
            futures::future::join_all(futs).await;
        }
    });
}

/// 30s 卷占用采样：`/system/df?type=volume` 需遍历卷目录，单独低频跑；
/// 首帧立即执行一次（否则打开卷视图时体积列会空到第一个 30s 周期）
pub fn spawn_volume_size_interval(state: &SharedState) {
    let state_s = state.clone();
    let rt_h = {
        let Ok(s) = state_s.lock() else { return };
        s.rt.handle().clone()
    };
    rt_h.spawn(async move {
        let mut tick = tokio::time::interval(VOLUME_SIZE_INTERVAL);
        loop {
            tick.tick().await;
            let tx = {
                let Ok(s) = state_s.lock() else { continue };
                s.tx.clone()
            };
            if let Ok(sizes) = volume_sizes().await {
                let _ = tx.send(Msg::VolumeSizes(sizes));
            }
        }
    });
}

/// 首屏：连接 Docker + 并发拉四类列表 + inspect 第一个容器填充详情面板
pub fn spawn_initial_load(state: &SharedState) {
    let Ok(s) = state.lock() else { return };
    let tx = s.tx.clone();
    let rt = s.rt.handle().clone();
    drop(s);
    rt.spawn(async move {
        let _ = crate::docker::connect();
        let (c_res, i_res, n_res, v_res) = futures::join!(
            list_containers(true),
            list_images(),
            list_networks(),
            list_volumes(),
        );
        let first_id = c_res
            .as_ref()
            .ok()
            .and_then(|list| list.first().map(|c| c.id.clone()));
        let _ = tx.send(Msg::Containers(c_res));
        let _ = tx.send(Msg::Images(i_res));
        let _ = tx.send(Msg::Networks(n_res));
        let _ = tx.send(Msg::Volumes(v_res));
        if let Some(id) = first_id {
            if let Ok(detail) = inspect_container(&id).await {
                let _ = tx.send(Msg::ContainerDetail(Ok(detail)));
            }
        }
    });
}
