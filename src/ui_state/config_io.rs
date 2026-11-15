//! AppState ↔ 配置文件（`~/.config/docker-panel/config.json`）的读写
//!
//! 拆出来的原因：`ui_state.rs` 正文逼近 300 行上限（compose 侧栏接入后到 307 行），
//! 而配置读写是独立职责 —— 拆成子模块，父模块只留一行 `config_io::save(self)`。
//!
//! 纪律：这里只做**字段搬运**，不含任何筛选 / 排序逻辑；越界值一律回落默认值。

use crate::config::AppConfig;
use std::collections::BTreeSet;

use crate::models::{ResourceSort, SortField};

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
    /// 台账内联树里**收起**的分组键
    pub compose_collapsed: BTreeSet<String>,
    /// 镜像列表排序
    pub sort_images: ResourceSort,
    /// 网络列表排序
    pub sort_networks: ResourceSort,
    /// 卷列表排序
    pub sort_volumes: ResourceSort,
    /// 启动时检查更新
    pub check_updates: bool,
    /// 自动下载并安装
    pub auto_install: bool,
    /// 安装成功后自动重启
    pub auto_restart: bool,
    /// 上次检查时间（unix 秒）
    pub last_update_check: i64,
}

/// 无配置文件（首次启动）时的内置默认：按 compose 分组排序 + 全量容器视图
impl Default for UiConfig {
    fn default() -> Self {
        Self {
            sort_field: SortField::Group,
            sort_desc: false,
            filter: "all".into(),
            view: 0,
            theme: 0,
            log_follow: true,
            query: String::new(),
            compose_collapsed: BTreeSet::new(),
            sort_images: ResourceSort::default(),
            sort_networks: ResourceSort::default(),
            sort_volumes: ResourceSort::default(),
            check_updates: true,
            auto_install: true,
            auto_restart: true,
            last_update_check: 0,
        }
    }
}

impl UiConfig {
    /// 配置文件 → UI 状态（纯映射，不做 IO）
    ///
    /// 字段级回落（越界值 / 类型不符）由 `AppConfig::load` 负责，
    /// 这里的输入已经是「可信的配置值」，只做编码转换。
    pub fn from_app_config(c: &AppConfig) -> Self {
        Self {
            sort_field: SortField::from_config_index(c.sort_field),
            sort_desc: c.sort_desc,
            filter: c.filter.clone(),
            view: c.view,
            theme: c.theme,
            log_follow: c.log_follow,
            query: c.search.clone(),
            // 内联树：收起的分组（缺字段 = 全展开）
            compose_collapsed: c.compose_collapsed.iter().cloned().collect(),
            sort_images: ResourceSort::from_config(c.sort_images),
            sort_networks: ResourceSort::from_config(c.sort_networks),
            sort_volumes: ResourceSort::from_config(c.sort_volumes),
            check_updates: c.check_updates,
            auto_install: c.auto_install,
            auto_restart: c.auto_restart,
            last_update_check: c.last_update_check,
        }
    }
}

/// UI 状态 → 待写入的配置（纯映射，不做 IO）
fn app_config_from_state(s: &AppState) -> AppConfig {
    AppConfig {
        theme: s.theme,
        view: s.view,
        filter: s.filter.clone(),
        sort_field: s.sort_field.to_config_index(),
        sort_desc: s.sort_desc,
        search: s.container_query.clone(),
        log_follow: s.log_follow,
        compose_collapsed: s.compose_collapsed.iter().cloned().collect(),
        sort_images: s.sort_images.to_config(),
        sort_networks: s.sort_networks.to_config(),
        sort_volumes: s.sort_volumes.to_config(),
        check_updates: s.update.check_updates,
        auto_install: s.update.auto_install,
        auto_restart: s.update.auto_restart,
        last_update_check: s.update.last_update_check,
    }
}

/// 读配置
///
/// 无配置文件（首次启动）→ 内置默认：按 compose 分组排序 + 全量容器视图。
/// 有配置文件 → 以文件为准（字段级回落由 `AppConfig::load` 负责）。
pub fn load() -> UiConfig {
    match AppConfig::load() {
        Some(c) => UiConfig::from_app_config(&c),
        None => UiConfig::default(),
    }
}

/// 把当前 UI 状态写入配置文件（低频回调 / 退出时调用；小文件原子写）
pub fn save(s: &AppState) {
    app_config_from_state(s).save();
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::ResourceSort;
    use crate::ui_state::AppState;

    /// 配置 → UI 状态 → 配置 的往返必须逐字段一致（编码转换不丢字段）
    #[test]
    fn config_maps_through_ui_state_without_loss() {
        let (state, _rx) = AppState::new_test();
        let mut s = state.lock().unwrap();
        s.theme = 2;
        s.view = 3;
        s.filter = "running".into();
        s.sort_field = SortField::Disk;
        s.sort_desc = false;
        s.container_query = "nginx".into();
        s.log_follow = false;
        s.compose_collapsed.insert("project:web".into());
        s.sort_images = ResourceSort {
            field: 1,
            desc: true,
        };
        s.sort_networks = ResourceSort {
            field: 2,
            desc: false,
        };
        s.sort_volumes = ResourceSort {
            field: 0,
            desc: true,
        };
        s.update.check_updates = false;
        s.update.auto_install = false;
        s.update.auto_restart = false;
        s.update.last_update_check = 42;

        let cfg = app_config_from_state(&s);
        assert_eq!(cfg.theme, 2);
        assert_eq!(cfg.view, 3);
        assert_eq!(cfg.filter, "running");
        assert_eq!(cfg.sort_field, SortField::Disk.to_config_index());
        assert_eq!(cfg.search, "nginx");
        assert!(cfg.compose_collapsed.contains(&"project:web".to_string()));
        assert_eq!(cfg.sort_images, s.sort_images.to_config());

        let back = UiConfig::from_app_config(&cfg);
        assert_eq!(back.theme, s.theme);
        assert_eq!(back.view, s.view);
        assert_eq!(back.filter, s.filter);
        assert_eq!(back.sort_field, s.sort_field);
        assert_eq!(back.sort_desc, s.sort_desc);
        assert_eq!(back.query, s.container_query);
        assert_eq!(back.log_follow, s.log_follow);
        assert_eq!(back.compose_collapsed, s.compose_collapsed);
        assert_eq!(back.sort_images, s.sort_images);
        assert_eq!(back.sort_networks, s.sort_networks);
        assert_eq!(back.sort_volumes, s.sort_volumes);
        assert_eq!(back.check_updates, s.update.check_updates);
        assert_eq!(back.auto_install, s.update.auto_install);
        assert_eq!(back.auto_restart, s.update.auto_restart);
        assert_eq!(back.last_update_check, s.update.last_update_check);
    }

    /// 资源列表排序的打包编码在配置 ↔ UI 之间走同一套约定
    #[test]
    fn resource_sort_uses_packed_encoding() {
        let cfg = AppConfig {
            sort_images: 9,
            sort_networks: 10,
            sort_volumes: 0,
            ..AppConfig::default()
        };
        let ui = UiConfig::from_app_config(&cfg);
        assert_eq!(
            ui.sort_images,
            ResourceSort {
                field: 1,
                desc: true
            }
        );
        assert_eq!(
            ui.sort_networks,
            ResourceSort {
                field: 2,
                desc: true
            }
        );
        assert_eq!(
            ui.sort_volumes,
            ResourceSort {
                field: 0,
                desc: false
            }
        );
        assert_eq!(ui.sort_images.to_config(), 9);
    }
}
