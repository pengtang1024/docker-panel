//! 命令行参数解析（纯函数，可单测）。
//!
//! 只识别 `--version` / `-V` / `--help` 三个参数；其余任何参数（含未知参数）
//! 一律按「正常启动 GUI」处理——桌面入口可能带额外参数，报错退出会破坏启动。

/// 解析结果：启动 GUI / 打印版本 / 打印帮助。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CliAction {
    Run,
    Version,
    Help,
}

/// 按出现顺序解析参数（调用方传入的参数不含 argv[0]）。
///
/// 第一个被识别的参数决定动作，未识别的参数直接忽略。
pub fn parse_args<I, S>(args: I) -> CliAction
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    for arg in args {
        match arg.as_ref() {
            "--version" | "-V" => return CliAction::Version,
            "--help" => return CliAction::Help,
            _ => {}
        }
    }
    CliAction::Run
}

/// `--version` 的输出行（`docker-panel <version>`）。
pub fn version_text(version: &str) -> String {
    format!("docker-panel {version}")
}

/// `--help` 的输出文本。
pub fn help_text() -> &'static str {
    "docker-panel — 极简 Docker Engine 桌面管理面板\n\
     \n\
     用法:\n\
     \x20   docker-panel [选项]\n\
     \n\
     选项:\n\
     \x20   -V, --version    打印版本号后退出\n\
     \x20       --help       打印本帮助后退出\n\
     \n\
     不带参数时启动图形界面。\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_args_runs_gui() {
        assert_eq!(parse_args(Vec::<String>::new()), CliAction::Run);
    }

    #[test]
    fn version_long_and_short() {
        assert_eq!(parse_args(["--version"]), CliAction::Version);
        assert_eq!(parse_args(["-V"]), CliAction::Version);
    }

    #[test]
    fn help_flag() {
        assert_eq!(parse_args(["--help"]), CliAction::Help);
    }

    #[test]
    fn unknown_args_keep_gui() {
        assert_eq!(parse_args(["--unknown", "-x"]), CliAction::Run);
    }

    #[test]
    fn first_recognized_flag_wins() {
        assert_eq!(parse_args(["--unknown", "--version"]), CliAction::Version);
        assert_eq!(parse_args(["--help", "--version"]), CliAction::Help);
        assert_eq!(parse_args(["--version", "--help"]), CliAction::Version);
    }

    #[test]
    fn version_text_format() {
        assert_eq!(version_text("0.7.10"), "docker-panel 0.7.10");
    }

    #[test]
    fn help_text_lists_flags() {
        let help = help_text();
        assert!(help.contains("--version"));
        assert!(help.contains("--help"));
        assert!(help.contains("用法"));
    }
}
