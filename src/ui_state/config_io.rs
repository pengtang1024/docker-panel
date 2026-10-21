//! AppState ↔ 配置文件（`~/.config/docker-panel/config.json`）的读写
//!
//! 拆出来的原因：`ui_state.rs` 正文逼近 300 行上限（compose 侧栏接入后到 307 行），
//! 而配置读写是独立职责 —— 拆成子模块，父模块只留一行 `config_io::save(self)`。
//!
//! 纪律：这里只做**字段搬运**，不含任何筛选 / 排序逻辑；越界值一律回落默认值。

use crate::compose_scope::{ComposeScope, ComposeState};
use crate::config::AppConfig;
use crate::models::SortField;

use super::AppState;

/// 从配置文件读回的 UI 状态
pub struct UiConfig {
    pub sort_field: SortField,
    pub sort_desc: bool,
    pub filter: String,
    pub view: i64,
    pub theme: i64,
    pub log_follow: bool,
    pub query: String,
    pub compose: ComposeState,
}

/// 读配置
///
/// 无配置文件（首次启动）→ 内置默认：按 compose 分组排序 + 全量容器视图。
/// 有配置文件 → 以文件为准（字段级回落由 `AppConfig::load` 负责）。
pub fn load() -> UiConfig {
    match AppConfig::load() {
        Some(c) => UiConfig {
            sort_field: SortField::from_config_index(c.sort_field),
            sort_desc: c.sort_desc,
            filter: c.filter.clone(),
            view: c.view,
            theme: c.theme,
            log_follow: c.log_follow,
            query: c.search.clone(),
            // Compose 侧栏：范围 + 展开集合（越界 / 缺字段回落默认）
            compose: ComposeState {
                scope: ComposeScope::from_code(
                    c.compose_kind,
                    &c.compose_project,
                    &c.compose_service,
                ),
                expanded: c.compose_expanded.iter().cloned().collect(),
            },
        },
        None => UiConfig {
            sort_field: SortField::Group,
            sort_desc: false,
            filter: "all".into(),
            view: 0,
            theme: 0,
            log_follow: true,
            query: String::new(),
            compose: ComposeState::default(),
        },
    }
}

/// 把当前 UI 状态写入配置文件（低频回调 / 退出时调用；小文件原子写）
pub fn save(s: &AppState) {
    let (compose_kind, compose_project, compose_service) = s.compose.scope.to_code();
    AppConfig {
        theme: s.theme,
        view: s.view,
        filter: s.filter.clone(),
        sort_field: s.sort_field.to_config_index(),
        sort_desc: s.sort_desc,
        search: s.container_query.clone(),
        log_follow: s.log_follow,
        compose_kind,
        compose_project,
        compose_service,
        compose_expanded: s.compose.expanded.iter().cloned().collect(),
    }
    .save();
}
