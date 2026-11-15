//! 行模型代际账（per-list epoch）
//!
//! consumer 锁内按「当时的筛选 / 排序 / 搜索条件」建行，锁外才灌给 UI。
//! 这段间隙里用户可能又改了条件（UI 回调在 UI 线程立即重建行并上屏），
//! 因此应用前必须确认「该列表的代际没变」，变了就丢弃这批旧行。
//!
//! 拆成按列表计数而不是单一全局计数器：搜索会同时影响容器与镜像
//! （`container_query` / `image_query` 同源），但网络 / 卷完全不受影响。
//! 全局计数器会把网络 / 卷的合法批次一并丢弃 → 表现为「改搜索后网络列表闪一下
//! 或短暂回退到旧数据」。

use std::sync::atomic::{AtomicU64, Ordering};

/// 容器台账（行 + 概览带 + 筛选芯片计数）
pub const ROWS_CONTAINERS: u8 = 1 << 0;
/// 镜像列表
pub const ROWS_IMAGES: u8 = 1 << 1;
/// 网络列表
pub const ROWS_NETWORKS: u8 = 1 << 2;
/// 卷列表
pub const ROWS_VOLUMES: u8 = 1 << 3;

/// 各列表的行模型代际（单调递增）
#[derive(Default)]
pub struct RowsEpoch {
    containers: AtomicU64,
    images: AtomicU64,
    networks: AtomicU64,
    volumes: AtomicU64,
}

/// 某一时刻的代际快照（consumer 建行时取，上屏前比对）
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RowsSnapshot {
    containers: u64,
    images: u64,
    networks: u64,
    volumes: u64,
}

impl RowsEpoch {
    /// 按位掩码作废行模型（只递增受影响列表的代际）
    pub fn mark(&self, mask: u8) {
        if mask & ROWS_CONTAINERS != 0 {
            self.containers.fetch_add(1, Ordering::AcqRel);
        }
        if mask & ROWS_IMAGES != 0 {
            self.images.fetch_add(1, Ordering::AcqRel);
        }
        if mask & ROWS_NETWORKS != 0 {
            self.networks.fetch_add(1, Ordering::AcqRel);
        }
        if mask & ROWS_VOLUMES != 0 {
            self.volumes.fetch_add(1, Ordering::AcqRel);
        }
    }

    pub fn snapshot(&self) -> RowsSnapshot {
        RowsSnapshot {
            containers: self.containers.load(Ordering::Acquire),
            images: self.images.load(Ordering::Acquire),
            networks: self.networks.load(Ordering::Acquire),
            volumes: self.volumes.load(Ordering::Acquire),
        }
    }

    /// `mask` 覆盖的列表是否都还停留在快照那一代（true = 可以安全上屏）
    pub fn unchanged(&self, snap: &RowsSnapshot, mask: u8) -> bool {
        (mask & ROWS_CONTAINERS == 0 || self.containers.load(Ordering::Acquire) == snap.containers)
            && (mask & ROWS_IMAGES == 0 || self.images.load(Ordering::Acquire) == snap.images)
            && (mask & ROWS_NETWORKS == 0 || self.networks.load(Ordering::Acquire) == snap.networks)
            && (mask & ROWS_VOLUMES == 0 || self.volumes.load(Ordering::Acquire) == snap.volumes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 作废只影响掩码里的列表：搜索（容器 + 镜像）不得连带网络 / 卷
    #[test]
    fn mark_is_scoped_to_mask() {
        let epoch = RowsEpoch::default();
        let snap = epoch.snapshot();
        epoch.mark(ROWS_CONTAINERS | ROWS_IMAGES);

        assert!(!epoch.unchanged(&snap, ROWS_CONTAINERS));
        assert!(!epoch.unchanged(&snap, ROWS_IMAGES));
        assert!(epoch.unchanged(&snap, ROWS_NETWORKS));
        assert!(epoch.unchanged(&snap, ROWS_VOLUMES));
        assert!(epoch.unchanged(&snap, ROWS_NETWORKS | ROWS_VOLUMES));
        assert!(!epoch.unchanged(
            &snap,
            ROWS_CONTAINERS | ROWS_IMAGES | ROWS_NETWORKS | ROWS_VOLUMES
        ));
    }

    /// 新快照之后可以继续上屏：代际判定只针对「建行那一刻」
    #[test]
    fn fresh_snapshot_matches_current_state() {
        let epoch = RowsEpoch::default();
        epoch.mark(ROWS_CONTAINERS | ROWS_IMAGES | ROWS_NETWORKS | ROWS_VOLUMES);
        let snap = epoch.snapshot();
        assert!(epoch.unchanged(
            &snap,
            ROWS_CONTAINERS | ROWS_IMAGES | ROWS_NETWORKS | ROWS_VOLUMES
        ));
        epoch.mark(ROWS_VOLUMES);
        assert!(!epoch.unchanged(&snap, ROWS_VOLUMES));
        assert!(epoch.unchanged(&snap, ROWS_CONTAINERS));
    }
}
