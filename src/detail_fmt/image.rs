//! 镜像抽屉内容（概览 + 配置 2 节），对齐 prototype detailHTML 的 image 分支

use crate::i18n::{t, trf};
use crate::models::ImageDetail;
use crate::slint_gen::DetailPair;
use crate::ui_state::AppState;

use super::{head, kbox, kv, or_dash, DrawerData};

pub(crate) fn build(s: &AppState) -> Option<DrawerData> {
    let d = s.image_detail.as_ref()?;
    let tags = or_dash(&d.repo_tags);
    Some(DrawerData {
        title: repo_name(&tags),
        sub: or_dash(&d.size),
        tabs_visible: false,
        detail: detail_rows(s, d, tags),
        env: Vec::new(),
    })
}

fn detail_rows(s: &AppState, d: &ImageDetail, tags: String) -> Vec<DetailPair> {
    let layers = if d.layers > 0 {
        trf("{0} layers", &[&d.layers.to_string()])
    } else {
        super::dash()
    };
    // 层数优先用 inspect 结果；列表接口不返回层数时回落缓存
    let layers = if d.layers > 0 {
        layers
    } else {
        match s.image_layers.get(&d.id) {
            Some(n) => trf("{0} layers", &[&n.to_string()]),
            None => layers,
        }
    };

    vec![
        head(&t("Overview")),
        kv(&t("Repo"), tags, true),
        kv(&t("Size"), or_dash(&d.size), false),
        kv(&t("Layers"), layers, false),
        kv(&t("Created"), or_dash(&d.created), false),
        kv("ID", or_dash(&d.short_id), true),
        head(&t("Config")),
        kbox(&t("Entrypoint"), or_dash(&d.entrypoint)),
        kbox(&t("Command"), or_dash(&d.cmd)),
        kbox(&t("Environment"), or_dash(&d.env)),
    ]
}

/// "nginx:latest, foo:1.0" → "nginx"（列表里的仓库名展示，对齐 prototype .repo）
fn repo_name(tags: &str) -> String {
    let first = tags.split(',').next().unwrap_or(tags).trim();
    let repo = match first.rsplit_once(':') {
        Some((name, tag)) if !tag.contains('/') => name,
        _ => first,
    };
    if repo.is_empty() {
        super::dash()
    } else {
        repo.to_string()
    }
}
