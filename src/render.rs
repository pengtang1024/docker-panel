//! Slint 列表模型复用 —— 心跳 push 绝不允许重建模型
//!
//! 每次 pump 都 `ModelRc::from(slice)` 会造出**新的** model 对象，Slint 视为
//! 数据源更换并**重置整个 Repeater**：全部行组件被销毁重建。
//! 后果：hover 态、下拉菜单、动画进度等行内本地状态全丢（3s 心跳闪一次），
//! 且 MCP 拿到的元素句柄会失效、悬停 tooltip 永久闪烁。
//!
//! 这里把 5 个列表模型缓存在 Slint（UI）线程的 thread_local 里：
//! 长度不齐用 push/remove_row 调整，逐行 `PartialEq` 比对，**只对变化行**
//! 调 `set_row_data`（发 RowDataChanged，属就地更新，不销毁组件）。
//!
//! 模型基于 `Rc`（非 Send），故只能活在 UI 线程；thread_local 让
//! `invoke_from_event_loop` 闭包无需捕获任何非 Send 值。

use std::rc::Rc;

use slint::{Model, ModelRc, VecModel};

use crate::slint_gen::{
    ComposeRow, ContainerRow, DetailPair, ImageRow, LogLineRow, NetworkRow, VolumeRow,
};

thread_local! {
    static CONTAINERS: Rc<VecModel<ContainerRow>> = Rc::new(VecModel::default());
    static IMAGES: Rc<VecModel<ImageRow>> = Rc::new(VecModel::default());
    static NETWORKS: Rc<VecModel<NetworkRow>> = Rc::new(VecModel::default());
    static VOLUMES: Rc<VecModel<VolumeRow>> = Rc::new(VecModel::default());
    static LOGS: Rc<VecModel<LogLineRow>> = Rc::new(VecModel::default());
    static DETAIL: Rc<VecModel<DetailPair>> = Rc::new(VecModel::default());
    static ENV: Rc<VecModel<DetailPair>> = Rc::new(VecModel::default());
    static COMPOSE: Rc<VecModel<ComposeRow>> = Rc::new(VecModel::default());
}

/// 就地同步一个缓存模型：对齐长度 → 逐行比对 → 仅变化行 set_row_data。
fn sync<T: PartialEq + Clone + 'static>(model: Rc<VecModel<T>>, data: Vec<T>) -> ModelRc<T> {
    let n = data.len();
    while model.row_count() > n {
        model.remove_row(model.row_count() - 1).ok();
    }
    for (i, row) in data.into_iter().enumerate() {
        if i >= model.row_count() {
            model.push(row);
        } else if model.row_data(i).as_ref() != Some(&row) {
            model.set_row_data(i, row);
        }
    }
    // ⚠️ 必须用 ModelRc::from（From<Rc<M>> 复用同一 Rc 指针）。
    // ModelRc::new(model) 内部是 `Rc::new(model)`，会把 Rc<VecModel> 再包一层，
    // 每次调用都产生**新的**指针 → ModelRc::eq 为假 → Slint 视为换模型并重置 Repeater。
    ModelRc::from(model)
}

/// 容器列表模型（须在 UI 线程调用）
pub fn containers(data: Vec<ContainerRow>) -> ModelRc<ContainerRow> {
    CONTAINERS.with(|m| sync(Rc::clone(m), data))
}

/// 镜像列表模型（须在 UI 线程调用）
pub fn images(data: Vec<ImageRow>) -> ModelRc<ImageRow> {
    IMAGES.with(|m| sync(Rc::clone(m), data))
}

/// 网络列表模型（须在 UI 线程调用）
pub fn networks(data: Vec<NetworkRow>) -> ModelRc<NetworkRow> {
    NETWORKS.with(|m| sync(Rc::clone(m), data))
}

/// 卷列表模型（须在 UI 线程调用）
pub fn volumes(data: Vec<VolumeRow>) -> ModelRc<VolumeRow> {
    VOLUMES.with(|m| sync(Rc::clone(m), data))
}

/// 日志列表模型（须在 UI 线程调用）
pub fn logs(data: Vec<LogLineRow>) -> ModelRc<LogLineRow> {
    LOGS.with(|m| sync(Rc::clone(m), data))
}

/// 抽屉详情行模型（须在 UI 线程调用）
pub fn detail_rows(data: Vec<DetailPair>) -> ModelRc<DetailPair> {
    DETAIL.with(|m| sync(Rc::clone(m), data))
}

/// 抽屉环境行模型（须在 UI 线程调用）
pub fn env_rows(data: Vec<DetailPair>) -> ModelRc<DetailPair> {
    ENV.with(|m| sync(Rc::clone(m), data))
}

/// Compose 项目筛选芯片模型（须在 UI 线程调用）
pub fn compose_rows(data: Vec<ComposeRow>) -> ModelRc<ComposeRow> {
    COMPOSE.with(|m| sync(Rc::clone(m), data))
}
