//! 自动更新消息 → AppState / DrainResult（从 ui_update.rs 拆出，满足 R2）。
//!
//! 只做状态搬运，不碰 Slint：UI 灌入仍由 consumer::apply_ui 统一完成。

use crate::models::Msg;
use crate::ui_state::AppState;

use super::{DrainResult, UiFlag};

/// 自动重启的退出延时：先让「restarting automatically…」状态文案上屏。
const AUTO_RESTART_DELAY_SECS: u64 = 3;

impl AppState {
    pub(super) fn apply_update_msg(&mut self, msg: Msg, res: &mut DrainResult) {
        match msg {
            Msg::UpdateCheckDone(manual, auto_installing, r) => {
                res.mark(UiFlag::UpdateCheckDone);
                self.update.update_progress = 0;
                self.update.update_busy_ui = auto_installing;
                match r {
                    Ok(info) => {
                        let newer = crate::update::version::is_newer(
                            &info.version,
                            crate::update::CURRENT_VERSION,
                        );
                        // 有更新时不节流：下次启动/6h 轮询继续检查，避免重启后徽标丢失；
                        // 失败也不节流，网络恢复后可以尽快重试。
                        if !newer {
                            self.update.last_update_check = crate::update::now_secs();
                            self.persist();
                        }
                        let was_available = self.update.update_available;
                        self.update.update_info = Some(info.clone());
                        // 已经装好、只差重启：标记待重启，不再提示「可更新」，
                        // 也不再让用户点安装（点了也只会被助手以「已安装」拒绝）。
                        if info.already_installed {
                            self.update.update_restart_needed = true;
                            self.update.update_status = crate::i18n::trf(
                                "Version {0} is already installed, restart to apply",
                                &[&info.version],
                            );
                        }
                        self.update.update_available =
                            newer && !self.update.update_restart_needed && !info.already_installed;
                        // 任何一次成功检查都清掉旧状态（含“已有任务在运行”的占位提示）
                        self.update.update_status.clear();
                        // 启动检查发现新版本：只提示一次，不弹窗
                        if !manual && newer && !auto_installing && !was_available {
                            res.status_text = Some(crate::i18n::trf(
                                "New version {0} available",
                                &[&info.version],
                            ));
                            res.mark(UiFlag::Toast);
                        }
                    }
                    Err(e) => {
                        // 启动检查失败静默；手动检查把错误显示在弹窗里
                        if manual {
                            self.update.update_status = e;
                        }
                    }
                }
            }
            Msg::UpdateProgress(percent, text) => {
                self.update.update_progress = percent.clamp(0, 100);
                self.update.update_status = text;
                self.update.update_busy_ui = true;
                res.mark(UiFlag::UpdateProgress);
            }
            Msg::UpdateDone(r, restart_needed) => {
                res.mark(UiFlag::UpdateDone);
                self.update.update_busy_ui = false;
                match r {
                    Ok(msg) => {
                        self.update.update_restart_needed |= restart_needed;
                        self.update.update_available = false;
                        self.update.update_progress = 100;
                        // 自动重启：安装成功且开关开启 → 提示后延时重启
                        // （pkexec 授权框本身就是一次用户在场确认；开关默认开，
                        //   想留在当前会话的可在设置里关掉）
                        if restart_needed
                            && self.update.update_restart_needed
                            && self.update.auto_restart
                        {
                            self.update.update_status =
                                crate::i18n::t("Update installed, restarting automatically...");
                            self.update.restart_on_exit = true;
                            Self::schedule_auto_restart(&self.rt);
                        } else {
                            self.update.update_status = msg.clone();
                        }
                        res.status_text = Some(msg);
                        res.mark(UiFlag::Toast);
                    }
                    Err(e) => {
                        self.update.update_progress = 0;
                        self.update.update_status = e;
                    }
                }
            }
            _ => {}
        }
    }

    /// 延时退出事件循环；「释放单实例锁 → 拉起新二进制」由 `main` 的退出流程统一执行。
    ///
    /// 不能在这里 spawn 新进程：此刻旧进程仍持有单实例锁，新进程启动时会连上旧
    /// listener、判为 `AlreadyRunning` 后退出，旧进程随后也退出 → 应用直接消失
    /// （0.7.12–0.7.14 的真实缺陷）。顺序必须是「先释放锁、后 spawn」。
    fn schedule_auto_restart(rt: &tokio::runtime::Runtime) {
        rt.handle().spawn(async move {
            tokio::time::sleep(std::time::Duration::from_secs(AUTO_RESTART_DELAY_SECS)).await;
            let _ = slint::quit_event_loop();
        });
    }
}
