//! Msg 聚合与 drain
//!
//! consumer 线程独占 `mpsc::Receiver<Msg>`，本模块负责把一批消息合并成
//! 一次 UI 刷新意图（`DrainResult`）。日志洪峰下必须限制单批条数并只标记
//! 真正变化的区域，否则长时间持锁会卡住 UI 回调。

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::mpsc;

use crate::docker::compute_cpu_delta;
use crate::models::Msg;
use crate::ui_state::AppState;

mod df;
mod drain;
mod log;
mod update;

pub use drain::{DrainResult, UiFlag};

impl AppState {
    /// 消费一批 Msg（最多 `max` 条），返回聚合的 UI 刷新意图。
    ///
    /// ⚠️ 必须有上限：日志洪峰时无界 `try_recv` 会让 consumer 长时间持有 state 锁，
    /// UI 线程的任何 `state.lock()` 回调（点击/搜索/切 Tab）都会被卡住。
    pub fn drain_messages(
        &mut self,
        rx: &mpsc::Receiver<Msg>,
        first: Option<Msg>,
        max: usize,
    ) -> DrainResult {
        let mut res = DrainResult::default();
        if let Some(msg) = first {
            self.apply_msg(msg, &mut res);
        }
        while res.processed < max {
            match rx.try_recv() {
                Ok(msg) => self.apply_msg(msg, &mut res),
                Err(_) => break,
            }
        }
        // 环形缓冲整批处理完再裁剪一次（逐行 drain 是 O(cap)，洪峰下会被放大）
        self.trim_logs();
        res.has_more = res.processed >= max;
        res.shutdown = self.shutdown.load(Ordering::Relaxed);
        res
    }

    /// 单条消息 → AppState + UI 意图。日志洪峰下每条只做 O(1) 状态更新。
    ///
    /// ⚠️ 消息按值消费（`Msg` 而非 `&Msg`）：日志批里的成千上万行必须**移动**进
    /// 缓冲，逐行 clone 会在洪峰下把 CPU 直接打满；列表类消息同理，去掉整表 clone。
    fn apply_msg(&mut self, msg: Msg, res: &mut DrainResult) {
        res.processed += 1;

        // 自动更新消息拆到 ui_update/update.rs（保持本文件 R2 预算）
        if matches!(
            &msg,
            Msg::UpdateCheckDone(..) | Msg::UpdateProgress(..) | Msg::UpdateDone(..)
        ) {
            self.apply_update_msg(msg, res);
            return;
        }

        match msg {
            // Stats：整轮采样一次到达，逐容器写回 containers（累加）
            Msg::StatsBatch(batch) => {
                // 逐条 iter().find 是 O(批量×容器数)；先建一次 id→下标映射，
                // 整轮 O(容器数 + 批量)（几百容器时差一个数量级）。
                // 映射存 owned id 避免与后面的 containers 可变借用冲突。
                let mut idx: HashMap<String, usize> = HashMap::with_capacity(self.containers.len());
                for (i, c) in self.containers.iter().enumerate() {
                    idx.insert(c.id.clone(), i);
                }
                for (id, curr) in batch {
                    // CPU% 需要差值。本地还没有 prev 时（进程刚起 / 该容器首次采样）
                    // 用 Docker 同帧返回的 pre 帧兜底 → 首轮就有真实 CPU%，
                    // 不必空等一个采样周期显示 0.0%。
                    let prev = self.stats_prev.get(&id).copied().or_else(|| curr.precpu());
                    let cpu = compute_cpu_delta(prev.as_ref(), &curr);
                    self.stats_prev.insert(id.clone(), curr);
                    if let Some(&i) = idx.get(id.as_str()) {
                        let c = &mut self.containers[i];
                        c.cpu_percent = cpu;
                        c.mem_usage = curr.mem_usage;
                        c.mem_limit = curr.mem_limit;
                        c.net_rx = curr.net_rx;
                        c.net_tx = curr.net_tx;
                    }
                }
                res.mark(UiFlag::ContainersChanged);
                // 抽屉详情含容器 CPU/内存，选中的是容器时也要重建
                if self.sel == crate::ui_state::SelKind::Container {
                    res.mark(UiFlag::DrawerChanged);
                }
            }
            Msg::Containers(r) => {
                // 连通性只由列表结果表达；失败不置行变化标记（列表没变）
                res.docker_ok = Some(r.is_ok());
                match r {
                    Ok(list) => {
                        res.mark(UiFlag::ContainersChanged);
                        // ⚠️ 保留旧容器 stats，避免 3s 列表刷新冲掉 6s stats 写入
                        let mut old: HashMap<String, (f64, u64, u64, u64, u64)> = self
                            .containers
                            .iter()
                            .map(|c| {
                                (
                                    c.id.clone(),
                                    (c.cpu_percent, c.mem_usage, c.mem_limit, c.net_rx, c.net_tx),
                                )
                            })
                            .collect();
                        self.containers = list;
                        for c in &mut self.containers {
                            if let Some(v) = old.remove(&c.id) {
                                (c.cpu_percent, c.mem_usage, c.mem_limit, c.net_rx, c.net_tx) = v;
                            }
                        }
                        // stats_prev 只增不减会随容器删除无界增长：这里按本轮
                        // 列表剪掉已消失容器的旧快照
                        let alive: std::collections::HashSet<&str> =
                            self.containers.iter().map(|c| c.id.as_str()).collect();
                        self.stats_prev.retain(|id, _| alive.contains(id.as_str()));
                        // 列表不再带 size=1 → 磁盘列从 /system/df 缓存补回
                        self.apply_container_sizes();
                    }
                    // 后台 3s 刷新失败不写状态栏：每轮闪一次会让底栏数字被推挤，
                    // 且失败已由连接红点表达；仅留 stderr 供排查。
                    Err(e) => eprintln!("container refresh failed: {e}"),
                }
            }
            Msg::Images(r) => {
                res.docker_ok = Some(r.is_ok());
                match r {
                    Ok(list) => {
                        res.mark(UiFlag::ImagesChanged);
                        self.images = list;
                        // 列表 API 不给层数，首次/新增镜像后台补查一次
                        self.request_missing_layers();
                    }
                    // 同上：连接状态已由红点表达，不刷底栏文字
                    Err(e) => eprintln!("image refresh failed: {e}"),
                }
            }
            Msg::Networks(r) => {
                if let Ok(list) = r {
                    res.mark(UiFlag::NetworksChanged);
                    self.networks = list;
                }
            }
            Msg::Volumes(r) => {
                if let Ok(list) = r {
                    res.mark(UiFlag::VolumesChanged);
                    self.volumes = list;
                }
            }
            Msg::DockerVersion(r) => {
                // 取不到时显示占位符，绝不伪造版本号（底栏曾写死 v26.1）
                res.docker_version = Some(match r {
                    Ok(v) => format!("v{v}"),
                    Err(_) => crate::detail_fmt::dash(),
                });
            }
            Msg::ImageLayers(pairs) => {
                res.mark(UiFlag::ImagesChanged);
                for (id, n) in pairs {
                    self.image_layers.insert(id, n);
                }
            }
            // `/system/df` 采样在首屏（30s interval 首 tick 立即跑）常早于列表到达：
            // 此时行模型还是空的，置变化标记只会白占一个 UI 应用窗口，把首行整体
            // 推迟一整窗。数据本身照常写入，列表到达时 Msg::Containers / Msg::Volumes
            // 会各自重建并调用 apply_container_sizes，不会丢。
            Msg::VolumeSizes(map) => self.apply_volume_sizes_msg(map, res),
            Msg::ContainerSizes(map) => self.apply_container_sizes_msg(map, res),
            Msg::ContainerDetail(r) => {
                // 分组日志模式下抽屉不是容器详情：迟到的旧 inspect 结果
                // 不能覆盖组标题 / 把抽屉切回详情模式
                if self.sel != crate::ui_state::SelKind::Group {
                    res.mark(UiFlag::DrawerChanged);
                    if let Ok(d) = r {
                        self.detail = Some(d);
                    }
                }
            }
            Msg::ImageDetail(r) => {
                res.mark(UiFlag::DrawerChanged);
                if let Ok(d) = r {
                    self.image_detail = Some(d);
                }
            }
            Msg::LogBatch(epoch, lines) => self.on_log_batch(epoch, lines, res),
            Msg::LogStreamEnded(epoch) => self.on_log_stream_ended(epoch),
            Msg::LogStreamStatus(epoch, text) => self.on_log_stream_status(epoch, text, res),
            Msg::ActionDone(desc, r) => {
                res.status_text = Some(match r {
                    Ok(_) => crate::i18n::t(&desc),
                    Err(e) => format!("{}: {e}", crate::i18n::t(&desc)),
                });
                res.mark(UiFlag::Toast);
                // 不再 refresh_now：容器/镜像/网络/卷的增删启停都会发 /events
                // （实测 create/start/stop/die/destroy、network/volume 的
                // create/destroy、image 的 tag/untag/pull 都有），event_watch
                // 已有 400ms 合并窗，同一次操作只触发一轮刷新。此前这里再加
                // 一次立即刷新，每次操作要多付 4 个 list 调用（8 → 4 个请求）。
                // 操作失败不会有事件，但失败意味着列表没变，也不需要刷。
            }
            Msg::PullProgress(p) => {
                res.status_text = Some(p);
            }
            Msg::PullDone(r) => {
                let ok = r.is_ok();
                res.status_text = Some(match r {
                    Ok(succ) => succ,
                    Err(e) => crate::i18n::trf("Pull failed: {0}", &[&e]),
                });
                res.mark(UiFlag::Toast);
                res.mark(UiFlag::ResetPullBusy);
                if ok {
                    res.mark(UiFlag::ClosePull);
                }
                // 同上：`image/pull` 事件已实测存在，交给 event_watch 刷新
            }
            Msg::CleanupDone(r) => {
                res.status_text = Some(match r {
                    Ok(succ) => succ,
                    Err(e) => crate::i18n::trf("Cleanup failed: {0}", &[&e]),
                });
                res.mark(UiFlag::Toast);
                // prune 的事件支持按守护进程版本而异（本机 29.x 有
                // container/prune，旧版本没有）→ 保留立即刷新兜底。
                // 清理是低频的用户显式操作，多一轮刷新可接受。
                res.mark(UiFlag::RefreshNow);
            }
            // 已在函数开头提前分派到 apply_update_msg
            Msg::UpdateCheckDone(..) | Msg::UpdateProgress(..) | Msg::UpdateDone(..) => {
                unreachable!("update messages are dispatched before the main match")
            }
        }
    }
}

#[cfg(test)]
mod tests;
