//! 多选批量操作状态 + 方法

use std::collections::HashSet;

/// 多选状态（AppState 单字段 `pub batch: BatchState`）
#[derive(Default)]
pub struct BatchState {
    pub selected_ids: HashSet<String>,
}

impl BatchState {
    pub fn toggle(&mut self, id: String) {
        if !id.is_empty() {
            if self.selected_ids.contains(&id) {
                self.selected_ids.remove(&id);
            } else {
                self.selected_ids.insert(id);
            }
        }
    }

    pub fn clear(&mut self) {
        self.selected_ids.clear();
    }

    /// 清理已被移除的容器 id，返回当前选中数量
    pub fn purge_dead(&mut self, live_ids: impl IntoIterator<Item = String>) -> usize {
        let live: HashSet<String> = live_ids.into_iter().collect();
        self.selected_ids.retain(|id| live.contains(id));
        self.selected_ids.len()
    }

    pub fn contains(&self, id: &str) -> bool {
        self.selected_ids.contains(id)
    }

    pub fn ids_sorted(&self) -> Vec<String> {
        let mut v: Vec<String> = self.selected_ids.iter().cloned().collect();
        v.sort();
        v
    }
}
