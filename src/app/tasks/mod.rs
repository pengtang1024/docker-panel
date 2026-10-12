//! 后台任务与消息泵：列表刷新、Docker 动作发起、channel 消息处理、定时轮询

mod messages;

use super::*;

/// 容器操作类型（enum 而非中文字符串，拼错变体在编译期即报错）
#[derive(Clone, Copy)]
pub(super) enum ActionKind {
    Start,
    Stop,
    Restart,
    Remove,
}

impl ActionKind {
    /// 展示用文案（拼进操作描述前缀）
    pub(super) fn desc(self) -> &'static str {
        match self {
            ActionKind::Start => "启动",
            ActionKind::Stop => "停止",
            ActionKind::Restart => "重启",
            ActionKind::Remove => "删除",
        }
    }

    /// 对单个容器执行该操作（spawn_action 与 spawn_batch_action 共用的分发）
    async fn run(self, id: &str) -> Result<String, String> {
        match self {
            ActionKind::Start => docker::start_container(id).await,
            ActionKind::Stop => docker::stop_container(id).await,
            ActionKind::Restart => docker::restart_container(id).await,
            ActionKind::Remove => docker::remove_container(id).await,
        }
    }
}

impl DvApp {
    // ---------- 后台任务发起 ----------

    /// 拉取容器列表
    pub(super) fn refresh_containers(&mut self) {
        self.containers_loading = true;
        self.last_container_refresh = Instant::now();
        let tx = self.tx.clone();
        let all = self.filter == Filter::All;
        self.rt.spawn(async move {
            let msg = Msg::Containers(docker::list_containers(all).await);
            let _ = tx.send(msg);
        });
    }

    /// 拉取镜像列表
    pub(super) fn refresh_images(&mut self) {
        self.images_loading = true;
        self.last_image_refresh = Instant::now();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let msg = Msg::Images(docker::list_images().await);
            let _ = tx.send(msg);
        });
    }

    /// 采样所有运行中容器的资源占用（并发上限 4）
    pub(super) fn refresh_stats(&mut self) {
        self.last_stats = Instant::now();
        let mut ids: Vec<String> = self
            .containers
            .iter()
            .filter(|c| c.is_running())
            .map(|c| c.id.clone())
            .collect();
        if let Some(rv) = &self.resource_view {
            if !ids.contains(&rv.id) {
                ids.push(rv.id.clone());
            }
        }
        if ids.is_empty() {
            return;
        }
        self.stats_pending = ids.len();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let tasks = futures::stream::iter(ids.into_iter().map(|id| async move {
                let r = docker::sample_container(&id).await;
                (id, r)
            }))
            .buffer_unordered(STATS_CONCURRENCY);
            let results: Vec<_> = tasks.collect().await;
            for (id, r) in results {
                if tx.send(Msg::Sample(id, r)).is_err() {
                    break;
                }
            }
        });
    }

    /// 执行容器操作（启动/停止/重启/删除）
    pub(super) fn spawn_action(&mut self, kind: ActionKind, id: String, name: String) {
        let tx = self.tx.clone();
        let desc = format!("{} {name}", kind.desc());
        self.rt.spawn(async move {
            let r = kind.run(&id).await;
            let _ = tx.send(Msg::ActionDone(desc, r));
        });
    }

    /// 批量执行容器操作（启动/停止/重启/删除），错误聚合为一条多行消息
    pub(super) fn spawn_batch_action(&mut self, kind: ActionKind, items: Vec<(String, String)>) {
        let tx = self.tx.clone();
        let n = items.len();
        let desc = format!("{} {n} 个容器", kind.desc());
        self.rt.spawn(async move {
            let mut errs: Vec<String> = Vec::new();
            for (id, name) in items {
                if let Err(e) = kind.run(&id).await {
                    errs.push(format!("{name}: {e}"));
                }
            }
            let r = if errs.is_empty() {
                Ok(format!("成功 {n} 项"))
            } else {
                Err(format!(
                    "部分失败（{} 项）：\n{}",
                    errs.len(),
                    errs.join("\n")
                ))
            };
            let _ = tx.send(Msg::ActionDone(desc, r));
        });
    }

    /// 批量删除镜像，错误聚合为一条多行消息
    pub(super) fn spawn_batch_delete_images(&mut self, items: Vec<(String, String)>) {
        let tx = self.tx.clone();
        let n = items.len();
        let desc = format!("删除 {n} 个镜像");
        self.rt.spawn(async move {
            let mut errs: Vec<String> = Vec::new();
            for (name, tag) in items {
                if let Err(e) = docker::remove_image(&name, &tag).await {
                    errs.push(format!("{name}:{tag}: {e}"));
                }
            }
            let r = if errs.is_empty() {
                Ok(format!("成功 {n} 项"))
            } else {
                Err(format!(
                    "部分失败（{} 项）：\n{}",
                    errs.len(),
                    errs.join("\n")
                ))
            };
            let _ = tx.send(Msg::ActionDone(desc, r));
        });
    }

    /// 拉取镜像
    pub(super) fn start_pull(&mut self) {
        let spec = self.pull_spec.trim().to_string();
        if spec.is_empty() || self.pull_busy {
            return;
        }
        self.pull_busy = true;
        self.pull_progress.clear();
        let tx = self.tx.clone();
        self.rt.spawn(async move {
            let progress_tx = tx.clone();
            let r = docker::pull_image(&spec, move |text| {
                let _ = progress_tx.send(Msg::PullProgress(text));
            })
            .await;
            let _ = tx.send(Msg::PullDone(r));
        });
    }

    /// 打开日志窗口（后台持续读日志流）
    pub(super) fn open_log(&mut self, id: String, name: String) {
        self.close_log();
        let (tx, rx) = channel();
        let task_id = id.clone();
        let handle = self.rt.spawn(async move {
            let _ = docker::follow_logs(&task_id, LOG_TAIL_LINES, tx).await;
        });
        self.log_view = Some(LogView {
            id,
            name,
            lines: Vec::new(),
            follow: true,
            ended: false,
            rx,
            handle: Some(handle),
        });
    }

    /// 关闭日志窗口并中断日志流任务
    pub(super) fn close_log(&mut self) {
        if let Some(mut lv) = self.log_view.take() {
            if let Some(h) = lv.handle.take() {
                h.abort();
            }
        }
    }

    /// 打开/切换资源监控窗口
    pub(super) fn open_resource(&mut self, id: String, name: String) {
        match &self.resource_view {
            Some(rv) if rv.id == id => {}
            _ => {
                self.resource_view = Some(ResourceView {
                    id,
                    name,
                    history: VecDeque::new(),
                });
                self.refresh_stats();
            }
        }
    }
}
