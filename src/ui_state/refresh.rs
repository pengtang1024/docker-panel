//! 列表刷新去重（in-flight + 脏标记）
//!
//! Docker 慢/无响应时不能每来一次事件就再叠 4 个 list 调用（会耗尽 tokio
//! 工作线程）。这里用「所有权 + 脏标记」两件套：持有所有权的一方跑刷新循环，
//! 期间到来的请求只置脏标记，本轮结束立刻补跑。
//!
//! ⚠️ 顺序纪律：**先释放所有权，再复查脏标记**。反过来（先复查、后释放）
//! 会丢刷新——窗口内到来的请求看到所有权仍被占，只置脏就返回；随后所有权
//! 被释放，那条脏标记再没人消费，列表就停在旧数据上。

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::docker::{list_containers, list_images, list_networks, list_volumes};
use crate::models::Msg;

/// 刷新去重的所有权闸门
pub struct RefreshGate {
    inflight: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
}

impl RefreshGate {
    pub fn new(inflight: Arc<AtomicBool>, dirty: Arc<AtomicBool>) -> Self {
        Self { inflight, dirty }
    }

    /// 请求开始一轮刷新：返回 true = 调用方获得所有权，应自己跑刷新循环；
    /// false = 已有任务在跑，只置脏标记后返回。
    pub fn acquire(&self) -> bool {
        if self.inflight.swap(true, Ordering::AcqRel) {
            self.dirty.store(true, Ordering::Release);
            false
        } else {
            true
        }
    }

    /// 一轮结束：先释放所有权，后来的请求才能直接接手。
    pub fn release(&self) {
        self.inflight.store(false, Ordering::Release);
    }

    /// 复查脏标记：true = 持有所有权期间有人请求过刷新，需要继续下一轮。
    pub fn take_dirty(&self) -> bool {
        self.dirty.swap(false, Ordering::AcqRel)
    }

    /// 为下一轮重新获取所有权：期间若已有人接手，把脏标记还回去（他跑完
    /// 复查时能看见），返回 false 表示本循环应退出。
    pub fn try_reacquire(&self) -> bool {
        if self.inflight.swap(true, Ordering::AcqRel) {
            self.dirty.store(true, Ordering::Release);
            false
        } else {
            true
        }
    }

    #[cfg(test)]
    fn is_dirty(&self) -> bool {
        self.dirty.load(Ordering::Acquire)
    }
}

/// 触发四类资源列表刷新（锁内安全：只 spawn，不做 IO）。
pub fn spawn(
    rt: &tokio::runtime::Runtime,
    tx: mpsc::Sender<Msg>,
    inflight: Arc<AtomicBool>,
    dirty: Arc<AtomicBool>,
) {
    let gate = RefreshGate::new(inflight, dirty);
    if !gate.acquire() {
        return;
    }
    rt.spawn(async move {
        loop {
            let (c_res, i_res, n_res, v_res) = futures::join!(
                crate::docker::with_timeout(list_containers(true)),
                crate::docker::with_timeout(list_images()),
                crate::docker::with_timeout(list_networks()),
                crate::docker::with_timeout(list_volumes()),
            );
            let _ = tx.send(Msg::Containers(c_res));
            let _ = tx.send(Msg::Images(i_res));
            let _ = tx.send(Msg::Networks(n_res));
            let _ = tx.send(Msg::Volumes(v_res));
            gate.release();
            if !gate.take_dirty() {
                break;
            }
            if !gate.try_reacquire() {
                break;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> RefreshGate {
        RefreshGate::new(
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    /// 修复前顺序（先复查脏标记、后释放所有权）会丢刷新：窗口内到来的请求
    /// 看到所有权仍被占，只置脏就返回，随后所有权释放，脏标记无人消费。
    #[test]
    fn request_in_release_window_is_consumed() {
        let g = gate();
        assert!(g.acquire(), "首轮应获得所有权");

        // 修复后的顺序：先释放，窗口内到来的请求能直接接手。
        g.release();
        assert!(g.acquire(), "释放后窗口内请求必须拿到所有权，而不是只置脏");
        assert!(!g.take_dirty(), "旧循环不应再看到脏标记");

        // 对照修复前的顺序：复查在前 → 窗口内请求只置脏 → 释放后脏标记滞留。
        let old = gate();
        assert!(old.acquire());
        assert!(!old.take_dirty(), "修复前：此刻还没人请求刷新");
        assert!(!old.acquire(), "窗口内请求只能置脏（所有权未释放）");
        old.release();
        assert!(old.is_dirty(), "修复前：脏标记滞留，没有任务会消费它");
    }

    /// fetch 期间到来的请求只置脏，本轮结束复查时必须被看见并继续下一轮。
    #[test]
    fn request_during_fetch_continues_next_round() {
        let g = gate();
        assert!(g.acquire());
        assert!(!g.acquire(), "刷新进行中：只置脏，不叠加任务");
        g.release();
        assert!(g.take_dirty(), "复查脏标记：必须继续补跑一轮");
        assert!(g.try_reacquire(), "没有别的任务接手，本循环继续");
    }

    /// 复查到脏标记但所有权已被别人抢走：脏标记必须归还，不能吞掉。
    #[test]
    fn dirty_restored_when_another_owner_appeared() {
        let g = gate();
        assert!(g.acquire());
        assert!(!g.acquire()); // 置脏
        g.release();
        assert!(g.take_dirty());
        assert!(g.acquire(), "另一个任务先抢到了所有权");
        assert!(!g.try_reacquire(), "本循环抢不到所有权，应退出");
        assert!(g.is_dirty(), "脏标记必须归还给新的所有者");
        // 新所有者按同样顺序复查，能看见并补跑
        g.release();
        assert!(g.take_dirty());
    }
}
