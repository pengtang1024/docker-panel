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

/// 一次批量 drain 的聚合结果。
///
/// consumer 用它决定本轮要刷新哪些 UI 区域：日志洪峰时只更新 `log_text`，
/// 不再每收一条日志就把容器/镜像/网络/卷/抽屉全部重建一遍。
#[derive(Default)]
pub struct DrainResult {
    /// 本批实际处理的 Msg 数（0 = 只是被 recv 超时叫醒）
    pub processed: usize,
    /// 还有积压（本批达到上限被截断）
    pub has_more: bool,
    /// AppState.shutdown 已置位
    pub shutdown: bool,
    pub lists_changed: bool,
    pub drawer_changed: bool,
    pub logs_changed: bool,
    /// Containers/Images 的最新结果（None = 本批没有列表结果）
    pub docker_ok: Option<bool>,
    /// 需要写入 status_text 的文本（toast / pull 进度）
    pub status_text: Option<String>,
    /// 本批出现过操作结果 toast（2s 后恢复 Ready）
    pub toast: bool,
    pub close_pull: bool,
    pub reset_pull_busy: bool,
    /// 操作完成，需要立即触发一次列表刷新
    pub refresh_now: bool,
}

impl DrainResult {
    /// 本批是否产生了需要落到 UI 的变化。
    ///
    /// 被 epoch 丢弃的旧日志只会增加 `processed`，不应触发无意义的 `apply_ui`/invoke。
    pub fn has_ui_changes(&self) -> bool {
        self.lists_changed
            || self.drawer_changed
            || self.logs_changed
            || self.docker_ok.is_some()
            || self.status_text.is_some()
            || self.toast
            || self.close_pull
            || self.reset_pull_busy
            || self.refresh_now
    }

    /// 把下一批结果并入累计结果（UI 刷新按时间窗口节流时会连续 drain 多批）
    pub fn merge(&mut self, o: DrainResult) {
        self.processed += o.processed;
        self.has_more = o.has_more;
        self.shutdown |= o.shutdown;
        self.lists_changed |= o.lists_changed;
        self.drawer_changed |= o.drawer_changed;
        self.logs_changed |= o.logs_changed;
        if o.docker_ok.is_some() {
            self.docker_ok = o.docker_ok;
        }
        if o.status_text.is_some() {
            self.status_text = o.status_text;
        }
        self.toast |= o.toast;
        self.close_pull |= o.close_pull;
        self.reset_pull_busy |= o.reset_pull_busy;
        self.refresh_now |= o.refresh_now;
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
            self.apply_msg(&msg, &mut res);
        }
        while res.processed < max {
            match rx.try_recv() {
                Ok(msg) => self.apply_msg(&msg, &mut res),
                Err(_) => break,
            }
        }
        // 环形缓冲整批处理完再裁剪一次（逐行 drain 是 O(cap)，洪峰下会被放大）
        if self.logs.len() > self.logs_cap {
            let excess = self.logs.len() - self.logs_cap;
            self.logs.drain(0..excess);
        }
        res.has_more = res.processed >= max;
        res.shutdown = self.shutdown.load(Ordering::Relaxed);
        res
    }

    /// 单条消息 → AppState + UI 意图。日志洪峰下每条只做 O(1) 状态更新。
    fn apply_msg(&mut self, msg: &Msg, res: &mut DrainResult) {
        res.processed += 1;

        // Stats：每条都写回 containers（累加）
        if let Msg::Stats(id, curr) = msg {
            let cpu = compute_cpu_delta(self.stats_prev.get(id), curr);
            self.stats_prev.insert(id.clone(), curr.clone());
            if let Some(c) = self.containers.iter_mut().find(|c| c.id == *id) {
                c.cpu_percent = cpu;
                c.mem_usage = curr.mem_usage;
                c.mem_limit = curr.mem_limit;
                c.net_rx = curr.net_rx;
                c.net_tx = curr.net_tx;
            }
            res.lists_changed = true;
        }

        match msg {
            Msg::Containers(r) => {
                res.lists_changed = true;
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
                        self.containers = list.clone();
                        for c in &mut self.containers {
                            if let Some(v) = old.remove(&c.id) {
                                (c.cpu_percent, c.mem_usage, c.mem_limit, c.net_rx, c.net_tx) = v;
                            }
                        }
                    }
                    Err(e) => {
                        res.status_text =
                            Some(crate::i18n::trf("Container refresh failed: {0}", &[e]));
                        res.toast = true;
                    }
                }
            }
            Msg::Images(r) => {
                res.lists_changed = true;
                res.docker_ok = Some(r.is_ok());
                match r {
                    Ok(list) => {
                        self.images = list.clone();
                        // 列表 API 不给层数，首次/新增镜像后台补查一次
                        self.request_missing_layers();
                    }
                    Err(e) => {
                        res.status_text = Some(crate::i18n::trf("Image refresh failed: {0}", &[e]));
                        res.toast = true;
                    }
                }
            }
            Msg::Networks(r) => {
                res.lists_changed = true;
                if let Ok(list) = r {
                    self.networks = list.clone();
                }
            }
            Msg::Volumes(r) => {
                res.lists_changed = true;
                if let Ok(list) = r {
                    self.volumes = list.clone();
                }
            }
            Msg::ImageLayers(pairs) => {
                res.lists_changed = true;
                for (id, n) in pairs {
                    self.image_layers.insert(id.clone(), *n);
                }
            }
            Msg::VolumeSizes(map) => {
                res.lists_changed = true;
                self.volume_sizes = map.clone();
            }
            Msg::ContainerDetail(r) => {
                res.drawer_changed = true;
                if let Ok(d) = r {
                    self.detail = Some(d.clone());
                }
            }
            Msg::ImageDetail(r) => {
                res.drawer_changed = true;
                if let Ok(d) = r {
                    self.image_detail = Some(d.clone());
                }
            }
            Msg::LogLine(epoch, line) => {
                // 只接受当前流 epoch：切换容器/清空后，队列里旧流的行直接丢弃
                if *epoch == self.stream_epoch {
                    self.logs.push(line.clone());
                    // 收到首条（历史）日志后才算历史加载完成；空历史/首流失败可重试 tail=200
                    self.history_done = true;
                    res.logs_changed = true;
                }
            }
            Msg::LogStreamEnded(epoch) => {
                // 流自然结束（容器停止/Docker 异常）：允许下次 ensure_follow 重新建流
                if *epoch == self.stream_epoch {
                    self.follow_id.clear();
                    self.follow_cancel = None;
                }
            }
            Msg::ActionDone(desc, r) => {
                res.docker_ok = Some(true);
                res.status_text = Some(match r {
                    Ok(_) => crate::i18n::t(desc),
                    Err(e) => format!("{}: {e}", crate::i18n::t(desc)),
                });
                res.toast = true;
                res.refresh_now = true;
            }
            Msg::PullProgress(p) => {
                res.docker_ok = Some(true);
                res.status_text = Some(p.clone());
            }
            Msg::PullDone(r) => {
                res.docker_ok = Some(true);
                res.status_text = Some(match r {
                    Ok(succ) => succ.clone(),
                    Err(e) => crate::i18n::trf("Pull failed: {0}", &[e]),
                });
                res.toast = true;
                res.reset_pull_busy = true;
                res.close_pull = r.is_ok();
                res.refresh_now = true;
            }
            Msg::CleanupDone(r) => {
                res.docker_ok = Some(true);
                res.status_text = Some(match r {
                    Ok(succ) => succ.clone(),
                    Err(e) => crate::i18n::trf("Cleanup failed: {0}", &[e]),
                });
                res.toast = true;
                res.refresh_now = true;
            }
            // 已在上方统一处理
            Msg::Stats(..) => {}
        }
    }
}

#[cfg(test)]
mod tests;
