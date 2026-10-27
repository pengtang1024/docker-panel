//! 自动更新消息 → AppState / DrainResult（从 ui_update.rs 拆出，满足 R2）。
//!
//! 只做状态搬运，不碰 Slint：UI 灌入仍由 consumer::apply_ui 统一完成。

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
                        self.update_status = msg.clone();
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
}
