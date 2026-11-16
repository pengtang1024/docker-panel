//! 用已校验的 fd 替换当前进程：`execveat(fd, "", argv, envp, AT_EMPTY_PATH)`。
//!
//! 为什么不用 `Command::new(path).exec()`：那会重新按路径解析，校验与执行之间留出被替换
//! 的窗口。这里把「校验的 inode」和「执行的 inode」钉成同一个 fd，竞态消失。

use std::ffi::CString;
use std::fs::File;
use std::io;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::io::AsRawFd;
use std::path::Path;

/// 用当前进程的命令行参数 exec（生产路径：参数原样透传给运行体）。
///
/// 若返回，说明 exec 失败（成功则进程映像已被替换，本函数永不返回）。
pub fn exec_verified(file: &File, display: &Path) -> io::Error {
    let args: Vec<std::ffi::OsString> = std::env::args_os().skip(1).collect();
    exec_with_args(file, display, &args)
}

/// 同上，但由调用方给定参数（单测要能构造确定的 argv）。
pub fn exec_with_args(file: &File, display: &Path, args: &[std::ffi::OsString]) -> io::Error {
    let argv_text = match build_argv(display, args) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let envp_text = match build_envp() {
        Ok(v) => v,
        Err(e) => return e,
    };
    // CString 必须活到 execveat 返回：这里的 Vec 在调用期间一直持有所有权
    let mut argv = to_ptr_vec(&argv_text);
    let mut envp = to_ptr_vec(&envp_text);
    // 空字符串不含 NUL，构造必然成功；失败时退回空 CString 也只是让 exec 报错
    let empty = CString::new("").unwrap_or_default();

    unsafe {
        libc::execveat(
            file.as_raw_fd(),
            empty.as_ptr(),
            argv.as_mut_ptr(),
            envp.as_mut_ptr(),
            libc::AT_EMPTY_PATH,
        );
    }
    io::Error::last_os_error()
}

/// `argv[0]` 换成运行体的可执行路径（进程名才对得上），其余参数原样保留。
fn build_argv(display: &Path, args: &[std::ffi::OsString]) -> Result<Vec<CString>, io::Error> {
    let mut raw = Vec::with_capacity(args.len() + 1);
    raw.push(CString::new(display.as_os_str().as_bytes()).map_err(|_| invalid("argv[0]"))?);
    for arg in args {
        raw.push(CString::new(arg.as_bytes()).map_err(|_| invalid("参数"))?);
    }
    Ok(raw)
}

fn build_envp() -> Result<Vec<CString>, io::Error> {
    let mut raw = Vec::new();
    for (key, value) in std::env::vars_os() {
        let mut entry = key.as_bytes().to_vec();
        entry.push(b'=');
        entry.extend_from_slice(value.as_bytes());
        raw.push(CString::new(entry).map_err(|_| invalid("环境变量"))?);
    }
    Ok(raw)
}

fn to_ptr_vec(raw: &[CString]) -> Vec<*mut libc::c_char> {
    let mut ptrs: Vec<*mut libc::c_char> = raw.iter().map(|s| s.as_ptr() as *mut _).collect();
    ptrs.push(std::ptr::null_mut());
    ptrs
}

fn invalid(what: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, format!("{what} 含 NUL 字节"))
}
