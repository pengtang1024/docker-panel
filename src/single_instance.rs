//! 单实例保障：同一用户只允许跑一个 docker-panel GUI 实例。
//!
//! 互斥对象是一个 Unix domain socket：`$XDG_RUNTIME_DIR/docker-panel-<user>.sock`，
//! `XDG_RUNTIME_DIR` 缺失/为空时回退 `/tmp`（仍带用户名后缀，避免多用户互撞）。
//!
//! 为什么用 socket 而不是 flock 锁文件：
//!   1. `bind` 本身就有「路径已存在即失败」的互斥语义，不需要额外依赖；
//!   2. 同一个 socket 顺带就是「请把已有窗口提到前台」的 IPC 通道；
//!   3. 进程被 SIGKILL / panic=abort 后残留的 socket 文件 `connect` 会失败
//!      （ECONNREFUSED），据此判陈旧并自动清理 —— 不存在需要人工删除的残留锁。
//!
//! # 窗口前置的能力边界（**只能是尽力而为**）
//!
//! 本项目只面向 Wayland。raise 的实现只有 `show()` + `set_minimized(false)` + `request_redraw()`，
//! 客户端没有激活自己的权限，**是否置顶由 compositor 决定**；不要指望它一定生效。
//!
//! Wayland 下三条合法路径逐一核对后都不成立：
//!   1. `xdg_activation_v1` 要求 token 先由**持有焦点的客户端/获得用户手势的启动器**
//!      用 `get_activation_token` 创建，再转交给目标客户端；从终端启动的第二个进程
//!      环境里没有 `XDG_ACTIVATION_TOKEN`（本机实测为空），拿不到 token。
//!   2. 即便拿到 token，对已有 surface 调 `activate(token, surface)` 需要
//!      wl_display / wl_surface / xdg_activation 代理，Slint 1.18 不暴露这些原始对象；
//!      用 raw-window-handle + unsafe 重连 wl_display 属于"塞重型/脆弱依赖"。
//!   3. 桌面文件没有 `DBusActivatable`，应用也不是 GApplication，
//!      `org.freedesktop.Application.Activate` 无从实现；即便补上 D-Bus，
//!      底层仍落到 GTK 的 `present()` —— 本机 GTK4 探针实测：窗口失焦后调用
//!      `present()`（内部即 xdg_activation）不会让它重新变成 active。
//!
//! 已核对过的 winit 0.30.13 行为（也是不引入它的理由）：Wayland 下 `focus_window()`
//! 是空函数、`set_minimized()` 被忽略（无法 unminimize），Slint 的后端也只在最小化
//! 状态**变化**时才下发 `set_minimized` —— 所以这里的 raise 通常只等于一次
//! `show()` + 重绘。
//!
//! 未来若要彻底解决：把应用改成 `DBusActivatable` 的 GApplication（gio 运行时
//! 属重型依赖，且仍受合成器策略约束）。当前刻意不引入 xdotool / zbus /
//! 原生 wayland-client 等外部手段 —— 它们在无会话 / 不同合成器下只会带来新的失败模式。

use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// 获取结果：拿到锁（主实例）/ 已有实例在跑。
pub enum Acquire {
    Primary(InstanceGuard),
    AlreadyRunning,
}

/// 主实例持有的锁；`Drop` 时删除 socket 文件（正常退出不留残留）。
pub struct InstanceGuard {
    path: PathBuf,
    listener: Option<UnixListener>,
}

impl InstanceGuard {
    /// 把监听交给后台线程：任何进来的连接都视为「请把已有窗口提到前台」。
    ///
    /// 回调节点由调用方提供（通常是 `invoke_from_event_loop` 包一层弱引用），
    /// 本模块不依赖 Slint，便于单测。
    pub fn spawn_raise_listener(&mut self, notify: impl Fn() + Send + 'static) {
        let Some(listener) = self.listener.take() else {
            return;
        };
        let _ = std::thread::Builder::new()
            .name("instance-raise".into())
            .spawn(move || {
                for stream in listener.incoming() {
                    let Ok(mut stream) = stream else { continue };
                    let mut buf = [0u8; 16];
                    let n = stream.read(&mut buf).unwrap_or(0);
                    // 协议只有一条消息；空连接（探活）也按 raise 处理，代价可忽略。
                    if n == 0 || buf[..n].starts_with(b"raise") {
                        notify();
                    }
                }
            });
    }
}

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        // listener 先于文件删除释放（字段顺序：path 在前，这里显式丢弃监听）
        self.listener.take();
        let _ = std::fs::remove_file(&self.path);
    }
}

/// 用默认路径获取锁（主实例 / 已运行 / IO 错误）。
pub fn acquire() -> std::io::Result<Acquire> {
    acquire_in(&default_path())
}

/// 指定路径获取锁（单测用）。
///
/// 语义：
///   - `bind` 成功 → Primary；
///   - `bind` 报已存在 → 尝试连接：连得上说明对方活着（AlreadyRunning，并已发出
///     raise 请求）；连不上说明是崩溃残留 → 删掉重绑。
pub fn acquire_in(path: &Path) -> std::io::Result<Acquire> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    for _ in 0..2 {
        match UnixListener::bind(path) {
            Ok(listener) => {
                return Ok(Acquire::Primary(InstanceGuard {
                    path: path.to_path_buf(),
                    listener: Some(listener),
                }))
            }
            Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
                if request_raise(path) {
                    return Ok(Acquire::AlreadyRunning);
                }
                // 陈旧 socket / 普通文件残留：没有监听者，清理后重试一次
                let _ = std::fs::remove_file(path);
            }
            Err(e) => return Err(e),
        }
    }
    // 两轮都失败：期间有别的实例抢到了 → 按已运行处理，绝不启动第二个 GUI
    Ok(Acquire::AlreadyRunning)
}

/// 通知已有实例前置窗口；连不上返回 false（是否陈旧由调用方判定）。
fn request_raise(path: &Path) -> bool {
    match UnixStream::connect(path) {
        Ok(mut stream) => {
            let _ = stream.write_all(b"raise\n");
            let _ = stream.flush();
            true
        }
        Err(_) => false,
    }
}

/// `$XDG_RUNTIME_DIR/docker-panel-<user>.sock`，缺失时用系统临时目录。
fn default_path() -> PathBuf {
    let dir = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_else(|_| "default".into());
    let user: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    let user = if user.is_empty() { "default" } else { &user };
    dir.join(format!("docker-panel-{user}.sock"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// 每个用例独占一个临时目录，避免并行测试互相干扰。
    fn temp_path(tag: &str) -> PathBuf {
        let n = SEQ.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir()
            .join(format!("dp-si-{}-{n}-{tag}", std::process::id()))
            .join("docker-panel.sock")
    }

    fn cleanup(path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::remove_dir_all(dir);
        }
    }

    #[test]
    fn acquire_primary_succeeds() {
        let path = temp_path("primary");
        let acquired = acquire_in(&path).expect("acquire");
        match acquired {
            Acquire::Primary(_) => assert!(path.exists(), "socket 文件应已创建"),
            Acquire::AlreadyRunning => panic!("首次获取不应判定为已运行"),
        }
        cleanup(&path);
    }

    #[test]
    fn second_acquire_reports_already_running() {
        let path = temp_path("second");
        let first = acquire_in(&path).expect("first");
        assert!(matches!(first, Acquire::Primary(_)));
        let second = acquire_in(&path).expect("second");
        assert!(
            matches!(second, Acquire::AlreadyRunning),
            "第二个实例必须被判定为已运行"
        );
        drop(first);
        cleanup(&path);
    }

    #[test]
    fn raise_request_reaches_listener() {
        let path = temp_path("raise");
        let mut first = match acquire_in(&path).expect("first") {
            Acquire::Primary(g) => g,
            Acquire::AlreadyRunning => panic!("首个实例应拿到锁"),
        };
        let (tx, rx) = std::sync::mpsc::channel();
        first.spawn_raise_listener(move || {
            let _ = tx.send(());
        });

        let second = acquire_in(&path).expect("second");
        assert!(
            matches!(second, Acquire::AlreadyRunning),
            "第二个实例必须被判定为已运行"
        );
        // 前置请求是否生效由 compositor 决定，但「请求确实送达主实例」必须成立
        rx.recv_timeout(std::time::Duration::from_secs(2))
            .expect("主实例的 raise 监听线程应收到请求");

        drop(second);
        drop(first);
        cleanup(&path);
    }

    #[test]
    fn release_allows_reacquire() {
        let path = temp_path("release");
        let first = acquire_in(&path).expect("first");
        drop(first);
        assert!(!path.exists(), "Drop 后应删除 socket 文件");
        let again = acquire_in(&path).expect("again");
        assert!(matches!(again, Acquire::Primary(_)), "释放后可再次获取");
        cleanup(&path);
    }

    #[test]
    fn stale_socket_does_not_block() {
        let path = temp_path("stale");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        // 模拟崩溃残留：绑上监听后直接丢弃（不删文件、不关路径）
        drop(UnixListener::bind(&path).expect("bind stale"));
        assert!(path.exists(), "残留 socket 文件应在");

        let acquired = acquire_in(&path).expect("acquire over stale");
        match acquired {
            Acquire::Primary(_) => {}
            Acquire::AlreadyRunning => panic!("陈旧锁不应阻塞启动"),
        }
        cleanup(&path);
    }

    #[test]
    fn stale_regular_file_does_not_block() {
        let path = temp_path("file");
        std::fs::create_dir_all(path.parent().unwrap()).expect("mkdir");
        std::fs::write(&path, b"not a socket").expect("write");
        let acquired = acquire_in(&path).expect("acquire over file");
        assert!(matches!(acquired, Acquire::Primary(_)));
        cleanup(&path);
    }
}
