//! 启动器的冒烟测试：真正 fork 一个子进程走 execveat，确认「校验用的 fd」能被执行，
//! 且 argv 原样透传。放在集成测试里，是因为 exec 成功会替换进程映像、单测内无法收尾。

use std::env;
use std::fs::File;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

#[path = "../src/exec.rs"]
mod exec;

const CHILD_FLAG: &str = "DOCKER_PANEL_LAUNCHER_EXEC_CHILD";

#[test]
fn execs_verified_fd_with_passthrough_args() {
    if env::var_os(CHILD_FLAG).is_some() {
        // 子进程：把自己整成 /bin/sh，用 O_PATH fd 走 execveat，参数由测试给定
        let file = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_PATH)
            .open("/bin/sh")
            .expect("打开 /bin/sh 失败");
        let args: Vec<std::ffi::OsString> = [
            "-c",
            "echo PASSTHROUGH:$0:$1",
            "sh-name-check",
            "tail-marker",
        ]
        .iter()
        .map(std::ffi::OsString::from)
        .collect();
        let err = exec::exec_with_args(&file, Path::new("/dev/null-is-not-used"), &args);
        panic!("execveat 失败：{err}");
    }

    let exe = env::current_exe().expect("定位测试二进制失败");
    let out = Command::new(exe)
        .args([
            "--exact",
            "execs_verified_fd_with_passthrough_args",
            "--nocapture",
        ])
        .env(CHILD_FLAG, "1")
        .output()
        .expect("拉起子进程失败");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("PASSTHROUGH:sh-name-check:tail-marker"),
        "argv 没有被正确透传，stdout={stdout:?} stderr={:?}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn exec_reports_error_for_non_executable_fd() {
    // 非可执行文件：execveat 必须返回错误而不是静默成功
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("not-executable.bin");
    std::fs::write(&path, b"not a program\n").unwrap();
    let file = File::open(&path).unwrap();
    let err = exec::exec_verified(&file, &path);
    assert_ne!(err.raw_os_error(), Some(0));
}
