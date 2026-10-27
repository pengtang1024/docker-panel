//! 用户配置持久化：~/.config/docker-panel/config.json
//!
//! 零额外依赖：读取走 serde_json::Value（serde_json 已在依赖树），
//! 写入是 7 个固定字段的手工 JSON。任何损坏/越界值都回落到 Default。

use std::fs;
use std::path::PathBuf;

/// 可持久化的 UI 状态（枚举值一律存 int，加载时校验范围）
#[derive(Clone, Debug)]
pub struct AppConfig {
    /// 0=auto 1=light 2=dark
    pub theme: i64,
    /// 0=containers 1=images 2=networks 3=volumes
    pub view: i64,
    /// all / running / stopped
    pub filter: String,
    /// 0=name 1=cpu 2=mem 3=disk 4=status
    pub sort_field: i64,
    pub sort_desc: bool,
    pub search: String,
    pub log_follow: bool,
    /// 台账内联树里**收起**的分组键（`project:<名>` / `svc:<项目>/<服务>`）
    pub compose_collapsed: Vec<String>,
    /// 镜像列表排序（低 3 位=列下标，第 3 位=降序标记，见 ResourceSort）
    pub sort_images: i64,
    /// 网络列表排序（同上）
    pub sort_networks: i64,
    /// 卷列表排序（同上）
    pub sort_volumes: i64,
    /// 启动时检查更新
    pub check_updates: bool,
    /// 检查到新版本后自动下载安装
    pub auto_install: bool,
    /// 上次检查时间（unix 秒）
    pub last_update_check: i64,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            theme: 0,
            view: 0,
            filter: "all".into(),
            sort_field: 0,
            sort_desc: true,
            search: String::new(),
            log_follow: true,
            compose_collapsed: Vec::new(),
            sort_images: 0,
            sort_networks: 0,
            sort_volumes: 0,
            check_updates: true,
            auto_install: true,
            last_update_check: 0,
        }
    }
}

impl AppConfig {
    /// 从配置文件加载。
    /// - 文件不存在 → None（首次启动，调用方使用内置默认）
    /// - 文件存在但损坏/字段越界 → 字段级回落默认值
    pub fn load() -> Option<Self> {
        let path = config_path()?;
        let text = fs::read_to_string(&path).ok()?;
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            return Some(Self::default());
        };
        let d = Self::default();
        Some(Self {
            theme: v
                .get("theme")
                .and_then(|x| x.as_i64())
                .filter(|x| (0..=2).contains(x))
                .unwrap_or(d.theme),
            view: v
                .get("view")
                .and_then(|x| x.as_i64())
                .filter(|x| (0..=3).contains(x))
                .unwrap_or(d.view),
            filter: v
                .get("filter")
                .and_then(|x| x.as_str())
                .filter(|x| matches!(*x, "all" | "running" | "stopped"))
                .unwrap_or(&d.filter)
                .to_string(),
            sort_field: v
                .get("sort_field")
                .and_then(|x| x.as_i64())
                .filter(|x| (-1..=4).contains(x))
                .unwrap_or(d.sort_field),
            sort_desc: v
                .get("sort_desc")
                .and_then(|x| x.as_bool())
                .unwrap_or(d.sort_desc),
            search: v
                .get("search")
                .and_then(|x| x.as_str())
                .unwrap_or(&d.search)
                .to_string(),
            log_follow: v
                .get("log_follow")
                .and_then(|x| x.as_bool())
                .unwrap_or(d.log_follow),
            sort_images: v
                .get("sort_images")
                .and_then(|x| x.as_i64())
                .filter(|x| (0..=10).contains(x))
                .unwrap_or(d.sort_images),
            sort_networks: v
                .get("sort_networks")
                .and_then(|x| x.as_i64())
                .filter(|x| (0..=10).contains(x))
                .unwrap_or(d.sort_networks),
            sort_volumes: v
                .get("sort_volumes")
                .and_then(|x| x.as_i64())
                .filter(|x| (0..=10).contains(x))
                .unwrap_or(d.sort_volumes),
            check_updates: v
                .get("check_updates")
                .and_then(|x| x.as_bool())
                .unwrap_or(d.check_updates),
            auto_install: v
                .get("auto_install")
                .and_then(|x| x.as_bool())
                .unwrap_or(d.auto_install),
            last_update_check: v
                .get("last_update_check")
                .and_then(|x| x.as_i64())
                .filter(|x| *x >= 0)
                .unwrap_or(d.last_update_check),
            compose_collapsed: v
                .get("compose_collapsed")
                .and_then(|x| x.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|x| x.as_str())
                        .map(str::to_string)
                        .collect()
                })
                .unwrap_or_default(),
        })
    }

    /// 原子写入（tmp + rename）；失败静默——配置丢失不影响主流程
    pub fn save(&self) {
        let Some(path) = config_path() else { return };
        if let Some(dir) = path.parent() {
            if fs::create_dir_all(dir).is_err() {
                return;
            }
        }
        // compose_collapsed 是字符串数组：手工拼 JSON（分组键走 json_escape）
        let collapsed = self
            .compose_collapsed
            .iter()
            .map(|s| format!("\"{}\"", json_escape(s)))
            .collect::<Vec<_>>()
            .join(", ");
        let json = format!(
            "{{\n  \"theme\": {},\n  \"view\": {},\n  \"filter\": \"{}\",\n  \"sort_field\": {},\n  \"sort_desc\": {},\n  \"search\": \"{}\",\n  \"log_follow\": {},\n  \"sort_images\": {},\n  \"sort_networks\": {},\n  \"sort_volumes\": {},\n  \"check_updates\": {},\n  \"auto_install\": {},\n  \"last_update_check\": {},\n  \"compose_collapsed\": [{}]\n}}\n",
            self.theme,
            self.view,
            json_escape(&self.filter),
            self.sort_field,
            self.sort_desc,
            json_escape(&self.search),
            self.log_follow,
            self.sort_images,
            self.sort_networks,
            self.sort_volumes,
            self.check_updates,
            self.auto_install,
            self.last_update_check,
            collapsed,
        );
        let tmp = path.with_extension("json.tmp");
        if fs::write(&tmp, json).is_ok() {
            let _ = fs::rename(&tmp, &path);
        }
    }
}

/// $XDG_CONFIG_HOME/docker-panel/config.json，缺 XDG 时用 ~/.config
fn config_path() -> Option<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))?;
    Some(base.join("docker-panel").join("config.json"))
}

/// 最小 JSON 字符串转义（只需处理引号/反斜杠/换行）
fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}
