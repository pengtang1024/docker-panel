//! 列表刷新去重（in-flight + 脏标记）
//!
//! Docker 慢/无响应时不能每来一次事件就再叠 4 个 list 调用（会耗尽 tokio
//! 工作线程）。这里用「所有权 + 脏标记」两件套：持有所有权的一方跑刷新循环，
//! 期间到来的请求只置脏标记，本轮结束立刻补跑。
//!
//! ⚠️ 顺序纪律：**先释放所有权，再复查脏标记**。反过来（先复查、后释放）
//! 会丢刷新——窗口内到来的请求看到所有权仍被占，只置脏就返回；随后所有权
//! 被释放，那条脏标记再没人消费，列表就停在旧数据上。

use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::mpsc;
use std::sync::Arc;

use crate::docker::{list_containers, list_networks, list_volumes};
use crate::models::Msg;

/// 要刷新哪些资源列表（位掩码）。
///
/// 事件驱动路径按事件类型收窄：容器启停只拉 `/containers/json`（≈33ms），
/// 不再顺带拉镜像 / 网络 / 卷（合计 ≈70ms）——镜像的「被引用容器数」在
/// 应用侧用新容器列表本地重算（`Msg::Containers` 处理里），不需要重拉。
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Kinds(u8);

impl Kinds {
    pub const CONTAINERS: Kinds = Kinds(1);
    pub const IMAGES: Kinds = Kinds(2);
    pub const NETWORKS: Kinds = Kinds(4);
    pub const VOLUMES: Kinds = Kinds(8);
    /// 全量：连接建立、手动刷新、prune 兜底走这一档。
    pub const ALL: Kinds = Kinds(15);
    /// 空掩码：脏标记为空表示「没有待补刷的资源」。
    pub const NONE: Kinds = Kinds(0);

    pub fn insert(&mut self, other: Kinds) {
        self.0 |= other.0;
    }

    pub fn contains(self, other: Kinds) -> bool {
        self.0 & other.0 != 0
    }

    pub fn is_empty(self) -> bool {
        self.0 == 0
    }

    fn bits(self) -> u8 {
        self.0
    }

    fn from_bits(bits: u8) -> Self {
        Kinds(bits)
    }
}

impl Default for Kinds {
    fn default() -> Self {
        Kinds::NONE
    }
}

/// 刷新去重的所有权闸门
pub struct RefreshGate {
    inflight: Arc<AtomicBool>,
    dirty: Arc<AtomicU8>,
}

impl RefreshGate {
    pub fn new(inflight: Arc<AtomicBool>, dirty: Arc<AtomicU8>) -> Self {
        Self { inflight, dirty }
    }

    /// 请求开始一轮刷新：返回 true = 调用方获得所有权，应自己跑刷新循环；
    /// false = 已有任务在跑，把**本次要刷的资源种类**并进脏标记后返回。
    pub fn acquire(&self, kinds: Kinds) -> bool {
        if self.inflight.swap(true, Ordering::AcqRel) {
            self.dirty.fetch_or(kinds.bits(), Ordering::Release);
            false
        } else {
            true
        }
    }

    /// 一轮结束：先释放所有权，后来的请求才能直接接手。
    pub fn release(&self) {
        self.inflight.store(false, Ordering::Release);
    }

    /// 复查脏标记：非空 = 持有所有权期间有人请求过刷新，返回**待补刷的种类**。
    pub fn take_dirty(&self) -> Kinds {
        Kinds::from_bits(self.dirty.swap(0, Ordering::AcqRel))
    }

    /// 为下一轮重新获取所有权：期间若已有人接手，把待刷种类还回去（他跑完
    /// 复查时能看见），返回 false 表示本循环应退出。
    pub fn try_reacquire(&self, kinds: Kinds) -> bool {
        if self.inflight.swap(true, Ordering::AcqRel) {
            self.dirty.fetch_or(kinds.bits(), Ordering::Release);
            false
        } else {
            true
        }
    }

    #[cfg(test)]
    fn dirty_kinds(&self) -> Kinds {
        Kinds::from_bits(self.dirty.load(Ordering::Acquire))
    }
}

/// 触发资源列表刷新（锁内安全：只 spawn，不做 IO）。
///
/// `kinds` 决定本轮拉哪几路：容器事件只刷容器单路（≈33ms），连接建立 /
/// 手动刷新 / prune 兜底用 `Kinds::ALL`（四路并发，≈70ms）。
pub fn spawn(
    rt: &tokio::runtime::Runtime,
    tx: mpsc::Sender<Msg>,
    inflight: Arc<AtomicBool>,
    dirty: Arc<AtomicU8>,
    kinds: Kinds,
) {
    let gate = RefreshGate::new(inflight, dirty);
    if !gate.acquire(kinds) {
        return;
    }
    rt.spawn(async move {
        let mut kinds = kinds;
        loop {
            refresh_once(&tx, kinds).await;
            gate.release();
            // 顺序纪律：先释放所有权，再复查脏标记（反了会丢刷新）
            let dirty = gate.take_dirty();
            if dirty.is_empty() {
                break;
            }
            if !gate.try_reacquire(dirty) {
                break;
            }
            kinds = dirty;
        }
    });
}

/// 一轮刷新：只拉 `kinds` 选中的那几路，未选中的不发请求。
///
/// 四路都用 `join!` 包住（哪怕只有一路要拉），这样 `Kinds::ALL` 的行为与
/// 拆分前完全一致：仍然是并发，而不是退化成串行。
async fn refresh_once(tx: &mpsc::Sender<Msg>, kinds: Kinds) {
    let (c, i, n, v) = futures::join!(
        async {
            if kinds.contains(Kinds::CONTAINERS) {
                Some(crate::docker::with_timeout(list_containers(true)).await)
            } else {
                None
            }
        },
        async {
            if kinds.contains(Kinds::IMAGES) {
                Some(crate::docker::with_timeout(crate::docker::list_images_raw()).await)
            } else {
                None
            }
        },
        async {
            if kinds.contains(Kinds::NETWORKS) {
                Some(crate::docker::with_timeout(list_networks()).await)
            } else {
                None
            }
        },
        async {
            if kinds.contains(Kinds::VOLUMES) {
                Some(crate::docker::with_timeout(list_volumes()).await)
            } else {
                None
            }
        },
    );
    // 容器先发：镜像的「被引用容器数」在应用侧用它重算（Msg 处理里）
    if let Some(c) = c {
        let _ = tx.send(Msg::Containers(c));
    }
    if let Some(i) = i {
        let _ = tx.send(Msg::Images(i));
    }
    if let Some(n) = n {
        let _ = tx.send(Msg::Networks(n));
    }
    if let Some(v) = v {
        let _ = tx.send(Msg::Volumes(v));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate() -> RefreshGate {
        RefreshGate::new(Arc::new(AtomicBool::new(false)), Arc::new(AtomicU8::new(0)))
    }

    /// 修复前顺序（先复查脏标记、后释放所有权）会丢刷新：窗口内到来的请求
    /// 看到所有权仍被占，只置脏就返回，随后所有权释放，脏标记无人消费。
    #[test]
    fn request_in_release_window_is_consumed() {
        let g = gate();
        assert!(g.acquire(Kinds::ALL), "首轮应获得所有权");

        // 修复后的顺序：先释放，窗口内到来的请求能直接接手。
        g.release();
        assert!(
            g.acquire(Kinds::ALL),
            "释放后窗口内请求必须拿到所有权，而不是只置脏"
        );
        assert!(g.take_dirty().is_empty(), "旧循环不应再看到脏标记");

        // 对照修复前的顺序：复查在前 → 窗口内请求只置脏 → 释放后脏标记滞留。
        let old = gate();
        assert!(old.acquire(Kinds::CONTAINERS));
        assert!(old.take_dirty().is_empty(), "修复前：此刻还没人请求刷新");
        assert!(
            !old.acquire(Kinds::CONTAINERS),
            "窗口内请求只能置脏（所有权未释放）"
        );
        old.release();
        assert!(
            old.dirty_kinds().contains(Kinds::CONTAINERS),
            "修复前：脏标记滞留，没有任务会消费它"
        );
    }

    /// fetch 期间到来的请求只置脏，本轮结束复查时必须被看见并继续下一轮。
    #[test]
    fn request_during_fetch_continues_next_round() {
        let g = gate();
        assert!(g.acquire(Kinds::ALL));
        assert!(
            !g.acquire(Kinds::CONTAINERS),
            "刷新进行中：只置脏，不叠加任务"
        );
        g.release();
        let dirty = g.take_dirty();
        assert!(
            dirty.contains(Kinds::CONTAINERS),
            "复查脏标记：必须继续补跑一轮"
        );
        assert!(g.try_reacquire(dirty), "没有别的任务接手，本循环继续");
    }

    /// 复查到脏标记但所有权已被别人抢走：脏标记必须归还，不能吞掉。
    #[test]
    fn dirty_restored_when_another_owner_appeared() {
        let g = gate();
        assert!(g.acquire(Kinds::ALL));
        assert!(!g.acquire(Kinds::IMAGES)); // 置脏
        g.release();
        let dirty = g.take_dirty();
        assert!(dirty.contains(Kinds::IMAGES));
        assert!(g.acquire(Kinds::ALL), "另一个任务先抢到了所有权");
        assert!(!g.try_reacquire(dirty), "本循环抢不到所有权，应退出");
        assert!(
            g.dirty_kinds().contains(Kinds::IMAGES),
            "脏标记必须归还给新的所有者"
        );
        // 新所有者按同样顺序复查，能看见并补跑
        g.release();
        assert!(g.take_dirty().contains(Kinds::IMAGES));
    }

    /// 空掩码不该发任何请求 / 消息：这是「按类型收窄」的边界，否则
    /// 一次容器事件仍会顺带拉镜像 / 网络 / 卷。
    #[tokio::test(flavor = "multi_thread")]
    async fn empty_kinds_sends_nothing() {
        let (tx, rx) = mpsc::channel();
        refresh_once(&tx, Kinds::NONE).await;
        assert!(rx.try_recv().is_err(), "空掩码不应发任何 Msg");
    }

    /// 实测两档刷新的墙钟（需要本机 dockerd；只读，不动容器）。
    ///
    /// `cargo test --release bench_refresh_round -- --ignored --nocapture`
    #[tokio::test(flavor = "multi_thread")]
    #[ignore]
    async fn bench_refresh_round() {
        use std::time::Instant;
        let (tx, _rx) = mpsc::channel();
        for (label, kinds) in [
            ("containers 单路", Kinds::CONTAINERS),
            ("ALL 四路", Kinds::ALL),
        ] {
            let mut best = f64::MAX;
            for _ in 0..5 {
                let t = Instant::now();
                refresh_once(&tx, kinds).await;
                let ms = t.elapsed().as_secs_f64() * 1000.0;
                if ms < best {
                    best = ms;
                }
            }
            println!("[bench] {label}: 最优 {best:.1}ms");
        }
    }
}
