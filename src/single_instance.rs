//! 单实例保障：同一用户只允许跑一个 docker-panel GUI 实例。
//!
//! 互斥是**两道锁**，权威的是不落盘的那一道：
//!   1. 抽象命名空间 socket——`@docker-panel-lock-<uid>` 按用户全局唯一（**与锁文件路径无关**，
//!      TMPDIR / XDG_RUNTIME_DIR / 运行目录怎么变都撞同一个名字），另加一个路径派生名
//!      `@docker-panel-<16hex>` 用来和 0.8.1 及更早版本互斥。没有文件、外部删不掉，
//!      进程退出时内核自动回收。
//!   2. 文件系统 socket（`/run/user/<uid>/docker-panel-<uid>.sock`）：只承担
//!      「请把已有窗口提到前台」的 IPC 与可见性（`ls` / `ss` 看得见锁）。
//!      拿不到它（运行目录不可写等）**不影响互斥**，也不会因此放行第二个实例。
//!
//! # 权威锁名为什么按 uid 而不是按路径
//!
//! 早先的抽象名由锁路径哈希而来。路径虽然已按 uid 派生，但 `lock_dir()` 在
//! `/run/user/<uid>` 不可用时会退到 `std::env::temp_dir()`——而 TMPDIR 是逐进程环境变量：
//! 从不同 shell / systemd 单元启动就可能算出不同路径 → 不同抽象名 → 互斥失效、照样多开。
//! 改成「名字只由 uid 决定」后，多开防护不再依赖任何文件路径算得一致。
//!
//! # 为什么权威锁必须不落盘（2026-11-16 的真实事故）
//!
//! 本机曾同时出现 **8 个** docker-panel 窗口：锁没坏，是**锁文件被外部删掉了**。
//! 那 8 个进程的可执行文件是 A/B 基准脚本拷出来的副本（`bin-old` / `bin-new`），脚本每轮
//! 先 `rm -f` 掉 socket 再起新进程，而正在跑的老进程既不察觉、也不知道自己已经不再是
//! 「唯一实例」，于是每个新进程都能 `bind` 成功。文件锁的互斥完全建立在「那个文件还在」
//! 之上，任何能写该目录的进程都能把它删掉、把互斥关掉 —— 这是机制性缺陷，不是 bug。
//! 抽象 socket 没有文件可删，从机制上封死这条路（也因此不需要「定时 stat 自愈」轮询）。
//!
//! # 锁路径为什么按 uid 派生、不读 `XDG_RUNTIME_DIR`
//!
//! 从桌面、终端、systemd 启动时 `XDG_RUNTIME_DIR` 可能不同、甚至缺失（旧实现缺失时退到
//! /tmp，连目录都换了）→ 两套路径互不冲突，照样多开。改成 `/run/user/<uid>`（不可用 /
//! 不可写时退系统临时目录），同一用户在任何启动路径下算出的锁都相同。
//!
//! # 抽象锁为什么不进 raise 线程
//!
//! listener 一旦移交给线程，guard 释放后仍有 fd 引用它 → 抽象名不释放 → 自动重启
//! （`release_then` 拉起新进程）时新进程判「已运行」后退出、旧进程也在退出 → 应用直接
//! 消失（0.7.12–0.7.14 修过的老缺陷）。所以抽象锁**只由 guard 持有**，raise IPC 继续走
//! 文件 socket（零改动、零空闲开销）。
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

use std::io::{ErrorKind, Read, Write};
use std::os::linux::net::SocketAddrExt;
use std::os::unix::net::{SocketAddr, UnixListener, UnixStream};
use std::path::{Path, PathBuf};

/// 获取结果：拿到锁（主实例）/ 已有实例在跑。
pub enum Acquire {
    Primary(InstanceGuard),
    AlreadyRunning,
}

/// 主实例持有的锁；`Drop` 时删除 socket 文件（正常退出不留残留）。
pub struct InstanceGuard {
    /// 文件 socket 路径；极端环境（运行目录不可写且临时目录也不可用）下没有文件锁，
    /// 只剩抽象锁——互斥不受影响，只是少了 raise 通道与可见性。
    path: Option<PathBuf>,
    /// 文件系统 socket：raise IPC 通道 + 可见的锁文件（可能已移交 raise 线程）
    listener: Option<UnixListener>,
    /// 抽象命名空间互斥锁：不落盘、删不掉，**只由本结构体持有**。
    /// 只为了持有而持有——读它没有任何意义，锁的生命周期就是本结构体的生命周期。
    #[expect(dead_code, reason = "仅持有抽象锁，Drop 时由内核回收")]
    exclusive: Vec<UnixListener>,
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
        if let Some(path) = &self.path {
            let _ = std::fs::remove_file(path);
        }
        // `exclusive` 在本体执行完后随结构体销毁 → 抽象名由内核立即回收
    }
}

/// 先释放锁、再拉起新实例（更新安装后重启的唯一正确顺序）。
///
/// `launch` 里 spawn 的新进程启动时会调用 `acquire()`：如果旧进程还持有锁，
/// 新进程会判为 `AlreadyRunning` 后退出，而旧进程此时也在退出
/// → 应用直接消失（0.7.12–0.7.14 的自动重启缺陷）。所以顺序不能反过来。
///
/// 只调用一次、不重试：锁被别的实例长期持有时新进程会立刻退出，不会堆积进程。
pub fn release_then<T>(guard: Option<InstanceGuard>, launch: impl FnOnce() -> T) -> T {
    drop(guard);
    launch()
}

/// 用默认路径获取锁（主实例 / 已运行 / IO 错误）。
pub fn acquire() -> std::io::Result<Acquire> {
    let primary = default_path();
    // 升级过渡期：旧二进制（≤0.7.17）只认 `docker-panel-<user>.sock` 那道文件锁。
    // 它还在跑时必须认出来，否则新旧同时启动会出现两个窗口。
    if let Some(legacy) = legacy_running(&primary) {
        request_raise(&legacy);
        return Ok(Acquire::AlreadyRunning);
    }
    // 权威互斥**先于任何文件系统操作**：抽象锁不落盘，运行目录不可写、TMPDIR 不同、
    // XDG_RUNTIME_DIR 缺失都不影响它——这正是多开防护不能再依赖「文件路径算得一致」的原因。
    let names = exclusive_names(&primary);
    let Some(exclusive) = bind_exclusives(&names, Some(&primary))? else {
        return Ok(Acquire::AlreadyRunning);
    };
    // 文件 socket 只做 raise IPC 与可见性：拿不到也照样能互斥，绝不因此放行第二个实例。
    let (path, listener) = attach_file_socket(&primary);
    Ok(Acquire::Primary(InstanceGuard {
        path,
        listener,
        exclusive,
    }))
}

/// 生产环境的权威锁名：
///   1. `docker-panel-lock-<uid>`：按用户全局唯一，**与锁文件路径无关**
///      （两个进程从不同 TMPDIR / XDG_RUNTIME_DIR 出发也能撞上）；
///   2. 路径派生名（`@docker-panel-<16hex>`）：与 0.8.1 及更早版本互斥用，它们只认这个。
fn exclusive_names(primary: &Path) -> Vec<String> {
    let mut names = vec![uid_lock_name()];
    let path_name = path_lock_name(primary);
    if !names.contains(&path_name) {
        names.push(path_name);
    }
    names
}

/// 按 uid 全局唯一的抽象锁名。
fn uid_lock_name() -> String {
    match current_uid() {
        Some(uid) => format!("docker-panel-lock-{uid}"),
        None => format!("docker-panel-lock-{}", sanitized_user()),
    }
}

/// 绑定一个抽象命名空间监听器（纯内核命名，没有文件可删）。
fn bind_abstract(name: &str) -> std::io::Result<UnixListener> {
    let addr = SocketAddr::from_abstract_name(name.as_bytes())?;
    UnixListener::bind_addr(&addr)
}

/// 依次绑定全部权威锁。
///
/// 返回 `Ok(None)` 表示有锁名已被占用（= 已有实例在跑），调用方应放弃启动；
/// 返回 `Err` 只可能是真正的系统错误（随后由调用方按 fail-closed 处理）。
fn bind_exclusives(
    names: &[String],
    raise_path: Option<&Path>,
) -> std::io::Result<Option<Vec<UnixListener>>> {
    let mut held = Vec::with_capacity(names.len());
    for name in names {
        match bind_abstract(name) {
            Ok(listener) => held.push(listener),
            Err(e) if e.kind() == ErrorKind::AddrInUse => {
                if let Some(path) = raise_path {
                    request_raise(path);
                }
                return Ok(None);
            }
            Err(e) => return Err(e),
        }
    }
    Ok(Some(held))
}

/// 绑定文件系统 socket（raise IPC + 可见性）：先主路径，再临时目录；都不行就放弃。
fn attach_file_socket(primary: &Path) -> (Option<PathBuf>, Option<UnixListener>) {
    for path in [
        primary.to_path_buf(),
        std::env::temp_dir().join(lock_name()),
    ] {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        match UnixListener::bind(&path) {
            Ok(listener) => return (Some(path), Some(listener)),
            Err(e) if e.kind() == ErrorKind::AddrInUse => {
                // 活着的实例占着文件锁：raise 一下，然后不带文件 socket 继续
                // （互斥已由抽象锁保证，文件 socket 只是 IPC 通道）。
                let _ = request_raise(&path);
            }
            Err(_) => {}
        }
    }
    (None, None)
}

/// 是否已有实例在跑（`--status` 用；零副作用，不建锁、不打扰运行中的实例）。
pub fn is_running() -> bool {
    let primary = default_path();
    match bind_exclusives(&exclusive_names(&primary), None) {
        // 能绑上全部锁名 → 没人持有；随后释放并探一眼文件 socket（兼容旧版本）
        Ok(Some(held)) => {
            drop(held);
            UnixStream::connect(&primary).is_ok()
        }
        Ok(None) => true,
        // 绑定出错（极少见）时按文件 socket 判定，宁可少报也不打扰运行中的实例
        Err(_) => UnixStream::connect(&primary).is_ok(),
    }
}

/// 指定路径查询是否已有实例（单测用）。
///
/// 判定依据是**能不能绑上抽象锁**：能绑上说明没人在跑（随即释放），再去探一眼旧版本
/// 的文件锁；绑不上（AddrInUse）说明有实例持有。刻意不 `connect` 抽象 listener ——
/// 它不 accept 连接，探一次就会在 backlog 里留一条永不释放的连接。
#[cfg(test)]
pub fn is_running_in(path: &Path) -> bool {
    match bind_exclusive(path) {
        Ok(listener) => {
            drop(listener);
            UnixStream::connect(path).is_ok()
        }
        Err(e) if e.kind() == ErrorKind::AddrInUse => true,
        Err(_) => UnixStream::connect(path).is_ok(),
    }
}

/// 指定路径获取锁（单测用）。
///
/// 语义：
///   - 抽象锁绑不上（AddrInUse）→ 已有实例；
///   - 抽象锁绑上、文件锁 `bind` 报已存在 → 尝试连接：连得上说明对方活着（AlreadyRunning，
///     并已发出 raise 请求，同时释放刚拿到的抽象锁）；连不上说明是崩溃残留 → 删掉重绑。
#[cfg(test)]
pub fn acquire_in(path: &Path) -> std::io::Result<Acquire> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // 1) 权威互斥：抽象命名空间 socket（不落盘 → 外部 rm 无法绕过）
    let mut exclusive = Some(match bind_exclusive(path) {
        Ok(listener) => listener,
        Err(e) if e.kind() == ErrorKind::AddrInUse => {
            request_raise(path);
            return Ok(Acquire::AlreadyRunning);
        }
        Err(e) => return Err(e),
    });
    // 2) 文件系统 socket：raise 通道 + 可见性（顺便挡住只认路径锁的旧版本）
    for _ in 0..2 {
        match UnixListener::bind(path) {
            Ok(listener) => {
                let exclusive = exclusive.take().expect("exclusive lock bound above");
                return Ok(Acquire::Primary(InstanceGuard {
                    path: Some(path.to_path_buf()),
                    listener: Some(listener),
                    exclusive: vec![exclusive],
                }));
            }
            Err(e) if e.kind() == ErrorKind::AddrInUse => {
                if request_raise(path) {
                    // 有活着的实例占着文件锁 → 放弃抽象锁（随作用域销毁）后按已运行返回
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

/// 绑定路径派生的互斥锁（`acquire_in` / 单测用）。
#[cfg(test)]
fn bind_exclusive(path: &Path) -> std::io::Result<UnixListener> {
    bind_abstract(&path_lock_name(path))
}

/// 路径派生的抽象名（FNV-1a 哈希）——兼容 0.8.1 及更早版本，单测也靠它做路径级隔离。
/// 抽象名上限 107 字节，这里只有 29 字节。
fn path_lock_name(path: &Path) -> String {
    use std::os::unix::ffi::OsStrExt;
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in path.as_os_str().as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("docker-panel-{hash:016x}")
}

/// `/run/user/<uid>/docker-panel-<uid>.sock`；目录不可用时落系统临时目录。
fn default_path() -> PathBuf {
    lock_dir().join(lock_name())
}

/// 锁目录**只按 uid 派生**，不读 `XDG_RUNTIME_DIR`（理由见文件头）。
fn lock_dir() -> PathBuf {
    if let Some(uid) = current_uid() {
        let run = PathBuf::from(format!("/run/user/{uid}"));
        if run.is_dir() {
            return run;
        }
    }
    std::env::temp_dir()
}

fn lock_name() -> String {
    match current_uid() {
        Some(uid) => format!("docker-panel-{uid}.sock"),
        None => format!("docker-panel-{}.sock", sanitized_user()),
    }
}

/// 当前 uid：直接解析 `/proc/self/status` 的 `Uid:` 行（不为了一个数字引入 libc 依赖）。
fn current_uid() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    let line = status.lines().find(|l| l.starts_with("Uid:"))?;
    line["Uid:".len()..].split_whitespace().next()?.parse().ok()
}

/// 用户名清洗（仅用于旧版本锁路径与 uid 不可得时的兜底命名）。
fn sanitized_user() -> String {
    let user = std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .unwrap_or_default();
    let user: String = user
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_')
        .collect();
    if user.is_empty() {
        "default".into()
    } else {
        user
    }
}

/// ≤0.7.17 的锁路径（`docker-panel-<user>.sock`）里有没有活着的监听者。
fn legacy_running(primary: &Path) -> Option<PathBuf> {
    let name = format!("docker-panel-{}.sock", sanitized_user());
    let mut dirs: Vec<PathBuf> = Vec::new();
    if let Some(dir) = primary.parent() {
        dirs.push(dir.to_path_buf());
    }
    let tmp = std::env::temp_dir();
    if !dirs.contains(&tmp) {
        dirs.push(tmp);
    }
    dirs.into_iter()
        .map(|dir| dir.join(&name))
        .find(|path| path != primary && UnixStream::connect(path).is_ok())
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

    fn primary(path: &Path) -> InstanceGuard {
        match acquire_in(path).expect("acquire") {
            Acquire::Primary(g) => g,
            Acquire::AlreadyRunning => panic!("首个实例应拿到锁"),
        }
    }

    #[test]
    fn acquire_primary_succeeds() {
        let path = temp_path("primary");
        let _guard = primary(&path);
        assert!(path.exists(), "socket 文件应已创建");
        cleanup(&path);
    }

    #[test]
    fn second_acquire_reports_already_running() {
        let path = temp_path("second");
        let first = primary(&path);
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
        let mut first = primary(&path);
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
        let first = primary(&path);
        drop(first);
        assert!(!path.exists(), "Drop 后应删除 socket 文件");
        let again = acquire_in(&path).expect("again");
        assert!(matches!(again, Acquire::Primary(_)), "释放后可再次获取");
        cleanup(&path);
    }

    #[test]
    fn launch_after_release_gets_lock() {
        let path = temp_path("launch");
        let guard = primary(&path);
        // 模拟「旧进程释放锁 → launcher 拉起新进程」：闭包里的 acquire 必须成功
        let acquired = release_then(Some(guard), || acquire_in(&path).expect("reacquire"));
        assert!(
            matches!(acquired, Acquire::Primary(_)),
            "锁释放后新进程必须能接管"
        );
        cleanup(&path);
    }

    #[test]
    fn release_with_raise_listener_allows_reacquire() {
        let path = temp_path("raise-release");
        let mut guard = primary(&path);
        // 生产路径：文件 socket 已移交 raise 线程，Drop 只剩 remove_file；
        // 抽象锁仍在 guard 里，必须在 Drop 时释放，否则新实例抢不到锁。
        guard.spawn_raise_listener(|| {});
        let acquired = release_then(Some(guard), || acquire_in(&path).expect("reacquire"));
        assert!(
            matches!(acquired, Acquire::Primary(_)),
            "监听线程仍持有旧 listener 时，unlink 后新实例也必须能接管"
        );
        cleanup(&path);
    }

    #[test]
    fn launch_while_lock_held_fails_fast() {
        let path = temp_path("held");
        let holder = primary(&path);
        // 锁一直没释放：只尝试一次，拿到 AlreadyRunning 就结束（没有重试循环）
        let mut attempts = 0;
        let acquired = release_then(None, || {
            attempts += 1;
            acquire_in(&path).expect("attempt")
        });
        assert!(matches!(acquired, Acquire::AlreadyRunning));
        assert_eq!(attempts, 1, "锁未释放时只尝试一次，不堆积进程");
        drop(holder);
        cleanup(&path);
    }

    #[test]
    fn deleted_socket_file_still_blocks_second_instance() {
        // 2026-11-16 事故的复现：外部把锁文件删掉后，旧实现里任何新进程都能 bind 成功
        let path = temp_path("deleted");
        let first = primary(&path);
        std::fs::remove_file(&path).expect("删掉锁文件");
        assert!(!path.exists());

        let second = acquire_in(&path).expect("second");
        assert!(
            matches!(second, Acquire::AlreadyRunning),
            "锁文件被删掉后仍必须判为已运行（抽象锁不落盘）"
        );
        drop(second);
        drop(first);
        cleanup(&path);
    }

    #[test]
    fn deleted_socket_file_still_visible_to_status() {
        let path = temp_path("deleted-status");
        let first = primary(&path);
        std::fs::remove_file(&path).expect("删掉锁文件");
        assert!(is_running_in(&path), "--status 也要能看见删掉文件的实例");
        drop(first);
        assert!(!is_running_in(&path), "实例退出后 --status 必须转 false");
        cleanup(&path);
    }

    #[test]
    fn status_reports_live_instance() {
        let path = temp_path("status");
        assert!(!is_running_in(&path), "空目录里不该有实例");
        let guard = primary(&path);
        assert!(is_running_in(&path), "拿锁后 --status 必须为真");
        drop(guard);
        assert!(!is_running_in(&path), "释放后 --status 必须为假");
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

    #[test]
    fn global_lock_name_depends_only_on_uid() {
        let name = uid_lock_name();
        if let Some(uid) = current_uid() {
            assert_eq!(name, format!("docker-panel-lock-{uid}"));
        }
        // 路径变化不改变全局锁名（这正是 TMPDIR 不同也能互斥的原因）
        let a = exclusive_names(Path::new("/run/user/1000/docker-panel-1000.sock"));
        let b = exclusive_names(Path::new("/tmp/docker-panel-1000.sock"));
        assert_eq!(a[0], b[0], "全局锁名必须与路径无关");
        assert_eq!(a[0], name);
        // 两个名字都必须参与绑定：全局名 + 路径派生名（后者兼容旧版本）
        assert_eq!(a.len(), 2);
        assert_ne!(a[0], a[1]);
    }

    #[test]
    fn occupied_global_lock_name_blocks_acquisition() {
        // 用测试专属名字，避免碰到生产锁（机器上可能正跑着 GUI）
        let name = format!("dp-test-{}-global", std::process::id());
        let _held = bind_abstract(&name).expect("首个抽象锁应能绑上");
        let names = vec![name, "dp-test-second".to_string()];
        match bind_exclusives(&names, None) {
            Ok(None) => {}
            Ok(Some(_)) => panic!("同名抽象锁已被占用，不应再绑上"),
            Err(e) => panic!("绑定出错：{e}"),
        }
    }

    #[test]
    fn default_path_is_uid_derived_and_ignores_xdg_runtime_dir() {
        let expected = match current_uid() {
            Some(uid) => format!("docker-panel-{uid}.sock"),
            None => format!("docker-panel-{}.sock", sanitized_user()),
        };
        // 关键回归：旧实现跟随 XDG_RUNTIME_DIR，桌面与终端两套路径 → 互不冲突 → 多开。
        // 这里伪造一个不同的 XDG_RUNTIME_DIR，锁文件名与所在父目录都不许跟着变。
        std::env::set_var("XDG_RUNTIME_DIR", "/nonexistent/dp-fake-xdg");
        let path = default_path();
        assert_eq!(
            path.file_name().and_then(|s| s.to_str()),
            Some(expected.as_str())
        );
        assert!(!path.to_string_lossy().contains("dp-fake-xdg"));
    }
}
