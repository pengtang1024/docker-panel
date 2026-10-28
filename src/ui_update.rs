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
use crate::ui_state::{AppState, LOG_ROWS_MAX};

mod update;

/// 一次批量 drain 的聚合结果。
///
/// consumer 用它决定本轮要刷新哪些 UI 区域：日志洪峰时只更新日志行列表，
/// 不再每收一条日志就把容器/镜像/网络/卷/抽屉全部重建一遍。
#[derive(Default)]
pub struct DrainResult {
    /// 本批实际处理的 Msg 数（0 = 只是被 recv 超时叫醒）
    pub processed: usize,
    /// 还有积压（本批达到上限被截断）
    pub has_more: bool,
    /// AppState.shutdown 已置位
    pub shutdown: bool,
    /// 容器卡片行变化（列表刷新 / stats 采样 / 选中态变化）
    pub containers_changed: bool,
    /// 镜像卡片行变化
    pub images_changed: bool,
    /// 网络卡片行变化
    pub networks_changed: bool,
    /// 卷卡片行变化（含 30s 磁盘占用回填）
    pub volumes_changed: bool,
    /// 抽屉内容变化（详情 / 环境变量 / 容器 stats）
    pub drawer_changed: bool,
    /// 日志行列表变化
    pub logs_changed: bool,
    /// Containers/Images 的最新结果（None = 本批没有列表结果）
    pub docker_ok: Option<bool>,
    /// 需要写入 status_text 的文本（toast / pull 进度）
    pub status_text: Option<String>,
    /// 日志流状态提示（重连中 / 已恢复 / 已停止）——独立通道，不覆盖状态栏
    pub log_status: Option<String>,
    /// 本批出现过操作结果 toast（2s 后恢复 Ready）
    pub toast: bool,
    pub close_pull: bool,
    pub reset_pull_busy: bool,
    /// 操作完成，需要立即触发一次列表刷新
    pub refresh_now: bool,
    /// 更新检查完成（成功/失败都置位；manual 失败才显示到弹窗）
    pub update_check_done: bool,
    /// 更新下载/安装进度变化
    pub update_progress: bool,
    /// 更新安装完成（成功或失败）
    pub update_done: bool,
}

impl DrainResult {
    /// 本批是否有列表需要重建。
    ///
    /// 按列表分标记：一条 stats 消息只重建容器行，
    /// 不再连带重建镜像 / 网络 / 卷（纯浪费：这些行没变，还要逐行比对）。
    pub fn any_list_changed(&self) -> bool {
        self.containers_changed
            || self.images_changed
            || self.networks_changed
            || self.volumes_changed
    }

    /// 本批是否产生了需要落到 UI 的变化。
    ///
    /// 被 epoch 丢弃的旧日志只会增加 `processed`，不应触发无意义的 `apply_ui`/invoke。
    pub fn has_ui_changes(&self) -> bool {
        self.any_list_changed()
            || self.drawer_changed
            || self.logs_changed
            || self.docker_ok.is_some()
            || self.status_text.is_some()
            || self.log_status.is_some()
            || self.toast
            || self.close_pull
            || self.reset_pull_busy
            || self.refresh_now
            || self.update_check_done
            || self.update_progress
            || self.update_done
    }

    /// 把下一批结果并入累计结果（UI 刷新按时间窗口节流时会连续 drain 多批）
    pub fn merge(&mut self, o: DrainResult) {
        self.processed += o.processed;
        self.has_more = o.has_more;
        self.shutdown |= o.shutdown;
        self.containers_changed |= o.containers_changed;
        self.images_changed |= o.images_changed;
        self.networks_changed |= o.networks_changed;
        self.volumes_changed |= o.volumes_changed;
        self.drawer_changed |= o.drawer_changed;
        self.logs_changed |= o.logs_changed;
        if o.docker_ok.is_some() {
            self.docker_ok = o.docker_ok;
        }
        if o.status_text.is_some() {
            self.status_text = o.status_text;
        }
        if o.log_status.is_some() {
            self.log_status = o.log_status;
        }
        self.toast |= o.toast;
        self.close_pull |= o.close_pull;
        self.reset_pull_busy |= o.reset_pull_busy;
        self.refresh_now |= o.refresh_now;
        self.update_check_done |= o.update_check_done;
        self.update_progress |= o.update_progress;
        self.update_done |= o.update_done;
    }
}

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
                res.containers_changed = true;
                // 抽屉详情含容器 CPU/内存，选中的是容器时也要重建
                if self.sel == crate::ui_state::SelKind::Container {
                    res.drawer_changed = true;
                }
            }
            Msg::Containers(r) => {
                res.containers_changed = true;
                res.docker_ok = Some(r.is_ok());
                match r {
                    Ok(list) => {
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
                    }
                    Err(e) => {
                        res.status_text =
                            Some(crate::i18n::trf("Container refresh failed: {0}", &[&e]));
                        res.toast = true;
                    }
                }
            }
            Msg::Images(r) => {
                res.images_changed = true;
                res.docker_ok = Some(r.is_ok());
                match r {
                    Ok(list) => {
                        self.images = list;
                        // 列表 API 不给层数，首次/新增镜像后台补查一次
                        self.request_missing_layers();
                    }
                    Err(e) => {
                        res.status_text =
                            Some(crate::i18n::trf("Image refresh failed: {0}", &[&e]));
                        res.toast = true;
                    }
                }
            }
            Msg::Networks(r) => {
                res.networks_changed = true;
                if let Ok(list) = r {
                    self.networks = list;
                }
            }
            Msg::Volumes(r) => {
                res.volumes_changed = true;
                if let Ok(list) = r {
                    self.volumes = list;
                }
            }
            Msg::ImageLayers(pairs) => {
                res.images_changed = true;
                for (id, n) in pairs {
                    self.image_layers.insert(id, n);
                }
            }
            Msg::VolumeSizes(map) => {
                res.volumes_changed = true;
                self.volume_sizes = map;
            }
            Msg::ContainerDetail(r) => {
                // 分组日志模式下抽屉不是容器详情：迟到的旧 inspect 结果
                // 不能覆盖组标题 / 把抽屉切回详情模式
                if self.sel != crate::ui_state::SelKind::Group {
                    res.drawer_changed = true;
                    if let Ok(d) = r {
                        self.detail = Some(d);
                    }
                }
            }
            Msg::ImageDetail(r) => {
                res.drawer_changed = true;
                if let Ok(d) = r {
                    self.image_detail = Some(d);
                }
            }
            Msg::LogBatch(epoch, mut lines) => {
                // 只接受当前流 epoch：切换容器/清空后，队列里旧流的行直接丢弃
                if epoch == self.stream_epoch {
                    // 洪峰下单批可达数万行：只保留最后 LOG_ROWS_MAX 行。
                    // 中间行既显示不出来（全被新行挤出视口），保留下来还会把
                    // 行模型撑到十万级 —— 裁剪退化成 O(n²)，洪峰下直接打满 CPU。
                    let start = lines.len().saturating_sub(LOG_ROWS_MAX);
                    // 整批搬进窗口（移动，不是拷贝）：洪峰下逐行拷贝会打满 CPU
                    self.logs_total += (lines.len() - start) as u64;
                    self.logs.extend(lines.drain(start..));
                    self.trim_logs();
                    // 收到首条（历史）日志后才算历史加载完成；空历史/首流失败可重试 tail=200
                    self.history_done = true;
                    res.logs_changed = true;
                }
            }
            Msg::LogStreamEnded(epoch) => {
                // 流结束（Eof / Cancelled）：清空 follow_id，允许下次 ensure_follow 重新建流。
                // 分组合并日志有 N 条流：只有**全部**退出才算结束
                //（单条结束不代表整组日志停止，其余成员仍在跟随）
                if epoch == self.stream_epoch && !self.follow_is_alive() {
                    self.follow_id.clear();
                    self.follow_targets.clear();
                    self.follow_cancel = None;
                    self.follow_active.clear();
                }
            }
            Msg::LogStreamStatus(epoch, text) => {
                // 日志流状态提示（重连中 / 已恢复 / 已停止）：独立通道，不占用状态栏。
                // 运行时就只在状态**变化**时发消息，这里直接透传给 UI。
                if epoch == self.stream_epoch {
                    res.log_status = Some(text);
                }
            }
            Msg::ActionDone(desc, r) => {
                res.docker_ok = Some(true);
                res.status_text = Some(match r {
                    Ok(_) => crate::i18n::t(&desc),
                    Err(e) => format!("{}: {e}", crate::i18n::t(&desc)),
                });
                res.toast = true;
                res.refresh_now = true;
            }
            Msg::PullProgress(p) => {
                res.docker_ok = Some(true);
                res.status_text = Some(p);
            }
            Msg::PullDone(r) => {
                res.docker_ok = Some(true);
                let ok = r.is_ok();
                res.status_text = Some(match r {
                    Ok(succ) => succ,
                    Err(e) => crate::i18n::trf("Pull failed: {0}", &[&e]),
                });
                res.toast = true;
                res.reset_pull_busy = true;
                res.close_pull = ok;
                res.refresh_now = true;
            }
            Msg::CleanupDone(r) => {
                res.docker_ok = Some(true);
                res.status_text = Some(match r {
                    Ok(succ) => succ,
                    Err(e) => crate::i18n::trf("Cleanup failed: {0}", &[&e]),
                });
                res.toast = true;
                res.refresh_now = true;
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
