//! channel 消息处理与定时轮询

use super::*;

impl DvApp {
    // ---------- 消息处理 ----------

    /// 轮询后台消息队列
    pub(in crate::app) fn pump_messages(&mut self) {
        while let Ok(msg) = self.rx.try_recv() {
            match msg {
                Msg::Containers(Ok(list)) => {
                    self.containers_loading = false;
                    self.docker_ok = Some(true);
                    // 保留上一次的资源数据，避免刷新瞬间指标归零
                    let old: HashMap<String, ContainerInfo> = self
                        .containers
                        .iter()
                        .map(|c| (c.id.clone(), c.clone()))
                        .collect();
                    self.containers = list
                        .into_iter()
                        .map(|mut c| {
                            if let Some(prev) = old.get(&c.id) {
                                c.cpu_percent = prev.cpu_percent;
                                c.mem_usage = prev.mem_usage;
                                c.mem_limit = prev.mem_limit;
                                c.block_read = prev.block_read;
                                c.block_write = prev.block_write;
                                c.net_rx = prev.net_rx;
                                c.net_tx = prev.net_tx;
                            }
                            c
                        })
                        .collect();
                    self.status = format!("容器 {} 个", self.containers.len());
                    // 剪除已消失容器的选择
                    let alive: HashSet<&String> = self.containers.iter().map(|c| &c.id).collect();
                    self.selected_containers.retain(|id| alive.contains(id));
                }
                Msg::Containers(Err(e)) => {
                    self.containers_loading = false;
                    self.docker_ok = Some(false);
                    self.show_error(e);
                }
                Msg::Images(Ok(list)) => {
                    self.images_loading = false;
                    self.docker_ok = Some(true);
                    self.images = list;
                    // 剪除已消失镜像的选择
                    let alive: HashSet<String> = self.images.iter().map(|i| i.key()).collect();
                    self.selected_images.retain(|k| alive.contains(k));
                }
                Msg::Images(Err(e)) => {
                    self.images_loading = false;
                    self.docker_ok = Some(false);
                    self.show_error(e);
                }
                Msg::Sample(id, Ok(sample)) => {
                    self.stats_pending = self.stats_pending.saturating_sub(1);
                    self.apply_sample(id, sample);
                }
                Msg::Sample(_, Err(_)) => {
                    // 容器可能刚好停止，忽略采样失败
                    self.stats_pending = self.stats_pending.saturating_sub(1);
                }
                Msg::PullProgress(text) => {
                    self.pull_progress.push(text);
                    if self.pull_progress.len() > PULL_PROGRESS_CAP {
                        self.pull_progress.drain(..PULL_PROGRESS_DROP);
                    }
                }
                Msg::PullDone(r) => {
                    self.pull_busy = false;
                    match r {
                        Ok(msg) => {
                            self.pull_spec.clear();
                            self.status = msg.clone();
                            self.result = Some(msg);
                            self.refresh_images();
                        }
                        Err(e) => self.show_error(e),
                    }
                }
                Msg::ActionDone(desc, r) => match r {
                    Ok(msg) => {
                        self.status = format!("{desc}: {msg}");
                        self.refresh_containers();
                        self.refresh_images();
                    }
                    Err(e) => self.show_error(e),
                },
                Msg::CleanupDone(r) => match r {
                    Ok(summary) => {
                        self.status = "清理完成".into();
                        self.result = Some(summary);
                        self.refresh_containers();
                        self.refresh_images();
                    }
                    Err(e) => self.show_error(e),
                },
            }
        }
    }

    /// 把采样结果应用到表格行与监控曲线
    pub(in crate::app) fn apply_sample(&mut self, id: String, s: ResourceSample) {
        if let Some(c) = self.containers.iter_mut().find(|c| c.id == id) {
            c.cpu_percent = s.cpu_percent;
            c.mem_usage = s.mem_usage;
            c.mem_limit = s.mem_limit;
            c.block_read = s.block_read;
            c.block_write = s.block_write;
            c.net_rx = s.net_rx;
            c.net_tx = s.net_tx;
        }
        if let Some(rv) = self.resource_view.as_mut().filter(|rv| rv.id == id) {
            rv.history.push_back(ChartPoint {
                cpu: s.cpu_percent,
                mem_usage: s.mem_usage,
                mem_limit: s.mem_limit,
            });
            if rv.history.len() > HISTORY_CAP {
                rv.history.pop_front();
            }
        }
    }

    /// 记录日志流事件
    pub(in crate::app) fn poll_logs(&mut self) {
        if let Some(lv) = self.log_view.as_mut() {
            while let Ok(ev) = lv.rx.try_recv() {
                match ev {
                    LogEvent::Line(line) => {
                        lv.lines.push(line);
                        if lv.lines.len() > LOG_LINES_CAP {
                            lv.lines.drain(..LOG_LINES_DROP);
                        }
                    }
                    LogEvent::Closed => lv.ended = true,
                }
            }
        }
    }

    /// 同一错误不重复弹窗
    pub(in crate::app) fn show_error(&mut self, e: String) {
        if e != self.last_error {
            self.last_error = e.clone();
            self.error = Some(e);
        }
    }

    /// 定时刷新容器/镜像/资源采样
    pub(in crate::app) fn maybe_refresh(&mut self) {
        if !self.containers_loading
            && self.last_container_refresh.elapsed() >= Duration::from_secs(CONTAINER_REFRESH_SECS)
        {
            self.refresh_containers();
        }
        if self.tab == Tab::Images
            && !self.images_loading
            && self.last_image_refresh.elapsed() >= Duration::from_secs(IMAGE_REFRESH_SECS)
        {
            self.refresh_images();
        }
        if self.stats_pending == 0
            && self.last_stats.elapsed() >= Duration::from_secs(STATS_SAMPLE_SECS)
        {
            self.refresh_stats();
        }
    }
}
