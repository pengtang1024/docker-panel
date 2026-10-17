//! 网络 / 卷抽屉内容，对齐 prototype detailHTML 的 network / volume 分支

use crate::i18n::{t, trfp};
use crate::ui_state::AppState;

use super::{head, kv, or_dash, DrawerData};

pub(crate) fn build_network(s: &AppState) -> Option<DrawerData> {
    let n = s.networks.iter().find(|n| n.id == s.sel_key)?;
    let count = super::network_container_count(s, &n.name);
    Some(DrawerData {
        title: n.name.clone(),
        sub: or_dash(&n.driver),
        tabs_visible: false,
        detail: vec![
            head(&t("Overview")),
            kv(&t("Name"), or_dash(&n.name), true),
            kv(&t("Driver"), or_dash(&n.driver), false),
            kv(&t("Scope"), or_dash(&n.scope), false),
            kv(
                &t("Containers"),
                trfp(
                    "{0} container",
                    "{0} containers",
                    count,
                    &[&count.to_string()],
                ),
                false,
            ),
            head(&t("Subnet")),
            kv(&t("Subnet"), or_dash(&n.subnet), true),
            kv(&t("Gateway"), or_dash(&n.gateway), true),
        ],
        env: Vec::new(),
    })
}

pub(crate) fn build_volume(s: &AppState) -> Option<DrawerData> {
    let v = s.volumes.iter().find(|v| v.name == s.sel_key)?;
    let size = match s.volume_sizes.get(&v.name) {
        Some(bytes) => super::format_bytes(*bytes),
        None => super::dash(),
    };
    Some(DrawerData {
        title: v.name.clone(),
        sub: or_dash(&v.driver),
        tabs_visible: false,
        detail: vec![
            head(&t("Overview")),
            kv(&t("Name"), or_dash(&v.name), true),
            kv(&t("Driver"), or_dash(&v.driver), false),
            kv(&t("Size"), size, false),
            kv(&t("Created"), or_dash(&v.created), false),
            kv(&t("Mountpoint"), or_dash(&v.mountpoint), true),
        ],
        env: Vec::new(),
    })
}
