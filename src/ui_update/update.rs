//! 自动更新消息 → AppState / DrainResult（从 ui_update.rs 拆出，满足 R2）。
//!
//! 只做状态搬运，不碰 Slint：UI 灌入仍由 consumer::apply_ui 统一完成。

use std::sync::mpsc::Sender;

use crate::models::Msg;
use crate::ui_state::AppState;

use super::DrainResult;

impl AppState {
    pub(super) fn apply_update_msg(&mut self, msg: Msg, res: &mut DrainResult) {
        match msg {
            Msg::UpdateCheckDone(manual, auto_installing, r) => {
                res.update_check_done = true;
                self.update_progress = 0;
                self.update_busy_ui = auto_installing;
                match r {
                    Ok(info) => {
                        let newer = crate::update::version::is_newer(
                            &info.version,
                            crate::update::CURRENT_VERSION,
                        );
                        // 有更新时不节流：下次启动/6h 轮询继续检查，避免重启后徽标丢失；
                        // 失败也不节流，网络恢复后可以尽快重试。
                        if !newer {
                            self.last_update_check = crate::update::now_secs();
                            self.persist();
                        }
                        let was_available = self.update_available;
                        self.update_available = newer && !self.update_restart_needed;
                        self.update_info = Some(info.clone());
                        // 任何一次成功检查都清掉旧状态（含“已有任务在运行”的占位提示）
                        self.update_status.clear();
                        // 启动检查发现新版本：只提示一次，不弹窗
                        if !manual && newer && !auto_installing && !was_available {
                            res.status_text = Some(crate::i18n::trf(
                                "New version {0} available",
                                &[&info.version],
                            ));
                            res.toast = true;
                        }
                    }
                    Err(e) => {
                        // 启动检查失败静默；手动检查把错误显示在弹窗里
                        if manual {
                            self.update_status = e;
                        }
                    }
                }
            }
            Msg::UpdateProgress(percent, text) => {
                self.update_progress = percent.clamp(0, 100);
                self.update_status = text;
                self.update_busy_ui = true;
                res.update_progress = true;
            }
            Msg::UpdateDone(r, restart_needed) => {
                res.update_done = true;
                self.update_busy_ui = false;
                match r {
                    Ok(msg) => {
                        self.update_restart_needed |= restart_needed;
                        self.update_available = false;
                        self.update_progress = 100;
                        // 自动重启：安装成功且开关开启 → 提示后延时重启
                        // （pkexec 授权框本身就是一次用户在场确认；开关默认开，
                        //   想留在当前会话的可在设置里关掉）
                        if restart_needed && self.update_restart_needed && self.auto_restart {
                            self.update_status =
                                crate::i18n::t("Update installed, restarting automatically...");
                            Self::schedule_auto_restart(&self.tx, &self.rt);
                        } else {
                            self.update_status = msg.clone();
                        }
                        res.status_text = Some(msg);
                        res.toast = true;
                    }
                    Err(e) => {
                        self.update_progress = 0;
                        self.update_status = e;
                    }
                }
            }
            _ => {}
        }
    }

    /// 3s 后启动替换后的新二进制并退出事件循环。
    ///
    /// 纪律：这里只 clone tx / rt handle 后 spawn，绝不把锁或 self 带进异步任务；
    /// 重启失败通过 Msg 回灌（spawn_blocking 里跑子进程/文件 IO）。
    fn schedule_auto_restart(tx: &Sender<Msg>, rt: &tokio::runtime::Runtime) {
        let tx = tx.clone();
        rt.handle().spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(3)).await;
            let result = match tokio::task::spawn_blocking(
                crate::update::install::restart_current_exe,
            )
            .await
            {
                Ok(r) => r,
                Err(e) => Err(e.to_string()),
            };
            match result {
                Ok(()) => {
                    let _ = slint::quit_event_loop();
                }
                Err(e) => {
                    let _ = tx.send(Msg::UpdateProgress(100, e));
                }
            }
        });
    }
}
