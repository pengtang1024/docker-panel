//! 共享状态锁的中毒诊断
//!
//! 所有 `state.lock()` 的失败分支（中毒 = 持锁线程 panic）必须打印一次
//! 上下文再按原语义跳过。此前是静默 `return`，运行时状态一旦不一致
//! 完全没有痕迹，排查只能靠猜。

use std::sync::{Mutex, MutexGuard};

/// 打印锁中毒上下文。调用方拿到 `None` / `Err` 后继续按原语义退出。
pub fn poison(ctx: &str) {
    eprintln!("[lock] poisoned at {ctx}: state may be inconsistent, operation skipped");
}

/// 取锁并记录中毒：正常拿到 `Some(guard)`；中毒时打印上下文并返回 `None`。
///
/// 适合 `if let Some(mut st) = acquire(&s, "...")` 这种原本就用 `if let` 的站点。
pub fn acquire<'a, T>(m: &'a Mutex<T>, ctx: &str) -> Option<MutexGuard<'a, T>> {
    match m.lock() {
        Ok(g) => Some(g),
        Err(_) => {
            poison(ctx);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    fn poisoned_mutex() -> Mutex<u32> {
        let m = Arc::new(Mutex::new(0u32));
        let m2 = m.clone();
        let _ = std::panic::catch_unwind(move || {
            let _g = m2.lock().unwrap();
            panic!("boom");
        });
        Arc::try_unwrap(m).unwrap()
    }

    /// 持锁 panic 后互斥量中毒：再次 lock 必须返回 Err（调用方据此走 poison 分支）
    #[test]
    fn lock_after_panic_is_poisoned() {
        let m = poisoned_mutex();
        assert!(m.lock().is_err(), "持锁线程 panic 后互斥量应为中毒状态");
    }

    /// acquire 在中毒时打印上下文并返回 None，调用方按原语义跳过
    #[test]
    fn acquire_returns_none_when_poisoned() {
        let m = poisoned_mutex();
        assert!(super::acquire(&m, "test").is_none());
    }
}
