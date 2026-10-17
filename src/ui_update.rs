/// UI 更新意图（从最新消息推导，由 main 负责应用）
pub struct UiUpdate {
    pub docker_ok: bool,
    pub status_text: String,
    pub close_pull: bool,
    pub reset_pull_busy: bool,
}

use crate::i18n::{t, trf};
use crate::models::Msg;

/// derive_update — 把"最后一条 Msg"翻译成 UI 想知道的状态
///
/// 它只看 latest_msg（而不是完整消息流），说明 UI 是"按最新状态渲染"，
/// 不关心历史过程。ActionDone / PullDone / CleanupDone 的结果都通过
/// 这里变成 status_text。返回的 UiUpdate 由 main.rs 的 pump_and_apply
/// 在 invoke_from_event_loop 里一次性应用到 Slint Model。
///
/// status_text 的 msgid 统一为英文，翻译在 i18n 模块按系统 locale 查表。
pub fn derive_update(containers_len: usize, latest_msg: &Option<Msg>) -> UiUpdate {
    match latest_msg {
        Some(Msg::Containers(Ok(_))) => UiUpdate {
            docker_ok: true,
            status_text: trf("Loaded {0} containers", &[&containers_len.to_string()]),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::Containers(Err(e))) => UiUpdate {
            docker_ok: false,
            status_text: trf("Container refresh failed: {0}", &[e]),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::Images(Err(e))) => UiUpdate {
            docker_ok: false,
            status_text: trf("Image refresh failed: {0}", &[e]),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::ActionDone(desc, Ok(_))) => UiUpdate {
            docker_ok: true,
            status_text: t(desc),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::ActionDone(desc, Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: format!("{}: {e}", t(desc)),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::PullProgress(progress)) => UiUpdate {
            docker_ok: true,
            status_text: progress.clone(),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::PullDone(Ok(succ))) => UiUpdate {
            docker_ok: true,
            status_text: succ.clone(),
            close_pull: true,
            reset_pull_busy: true,
        },
        Some(Msg::PullDone(Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: trf("Pull failed: {0}", &[e]),
            close_pull: false,
            reset_pull_busy: true,
        },
        Some(Msg::CleanupDone(Ok(succ))) => UiUpdate {
            docker_ok: true,
            status_text: succ.clone(),
            close_pull: false,
            reset_pull_busy: false,
        },
        Some(Msg::CleanupDone(Err(e))) => UiUpdate {
            docker_ok: true,
            status_text: trf("Cleanup failed: {0}", &[e]),
            close_pull: false,
            reset_pull_busy: false,
        },
        _ => UiUpdate {
            docker_ok: true,
            status_text: t("Ready"),
            close_pull: false,
            reset_pull_busy: false,
        },
    }
}
