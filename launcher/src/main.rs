//! docker-panel 启动器。
//!
//! 安装形态（deb）：
//! - `/usr/bin/docker-panel` = 本程序（root 所有、非 setuid、无 capabilities）
//! - `/usr/lib/docker-panel/seed/docker-panel` = deb 自带的种子运行体
//!
//! 自动更新把新版本写到 `~/.local/opt/docker-panel/versions/<ver>/`（普通用户可写），
//! 本程序在「用户副本」和「种子」之间挑版本更高的、**签名校验通过**的那份 exec 起来。
//! 私钥离线保管、公钥编译进本程序，所以用户目录里的运行体即使被篡改也起不来，
//! 全程不需要 root、不需要授权框。
//!
//! 退出码：0 = 正常（`--print-target` 只打印不启动）；1 = 找不到或没有一份验签通过。

mod exec;
mod verify;
mod version;

use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::process;

use version::Version;

/// deb 包里的种子运行体目录。
const SEED_DIR: &str = "/usr/lib/docker-panel/seed";
/// 用户态运行副本根目录（相对 `$HOME`）。
const USER_SUBDIR: &str = ".local/opt/docker-panel";
/// 运行体目录里的版本文件名，内容如 `0.7.18`。
const VERSION_NAME: &str = "version";

const ENV_USER_ROOT: &str = "DOCKER_PANEL_RUNTIME_ROOT";
const ENV_SEED_DIR: &str = "DOCKER_PANEL_SEED_DIR";
/// 打运行时日志到 stderr（排障用，默认安静）。
const ENV_VERBOSE: &str = "DOCKER_PANEL_LAUNCHER_VERBOSE";
/// 只打印将被启动的目录，不 exec：给打包校验脚本和 CI 冒烟用。
const FLAG_PRINT_TARGET: &str = "--print-target";

/// 候选来源：决定用哪种方式建立信任。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    /// 用户副本：必须验签（`checksums.txt` + `.sig` + 运行体 sha256）。
    UserCopy,
    /// deb 种子：root 所有、不可被普通用户改写，按属主信任。
    Seed,
}

struct Candidate {
    /// 用于报错的来源说明
    label: &'static str,
    kind: Kind,
    dir: PathBuf,
    version: Version,
}

fn main() {
    process::exit(run());
}

fn run() -> i32 {
    let keys = match verify::embedded_keys() {
        Ok(keys) => keys,
        Err(e) => {
            eprintln!("docker-panel: {e}，拒绝启动");
            return 1;
        }
    };

    let mut candidates = collect_candidates();
    // 版本高的优先；同版本时用户副本优先（它才是自动更新装的那一份）
    candidates.sort_by(|a, b| {
        b.version
            .cmp(&a.version)
            .then_with(|| rank(a.label).cmp(&rank(b.label)))
    });
    if candidates.is_empty() {
        eprintln!("docker-panel: 找不到任何运行体（已查找 {SEED_DIR} 与 ~/{USER_SUBDIR}）");
        return 1;
    }

    let print_only = env::args_os()
        .skip(1)
        .any(|a| a == OsStr::new(FLAG_PRINT_TARGET));
    for candidate in &candidates {
        let checked = match candidate.kind {
            Kind::Seed => verify::verify_seed(&candidate.dir),
            Kind::UserCopy => verify::verify_dir(&candidate.dir, &keys),
        };
        match checked {
            Ok(payload) => {
                let payload_path = candidate.dir.join(verify::PAYLOAD_NAME);
                if env::var_os(ENV_VERBOSE).is_some() {
                    eprintln!(
                        "docker-panel: 使用 {}（{} {}，sha256 {}）",
                        payload_path.display(),
                        candidate.label,
                        candidate.version,
                        payload.sha256
                    );
                }
                if print_only {
                    println!("{}", payload_path.display());
                    return 0;
                }
                let err = exec::exec_verified(&payload.file, &payload_path);
                eprintln!("docker-panel: 启动 {} 失败：{err}", payload_path.display());
                return 1;
            }
            Err(e) => eprintln!(
                "docker-panel: 跳过 {}（{}）：{e}",
                candidate.dir.display(),
                candidate.label
            ),
        }
    }
    eprintln!("docker-panel: 没有任何运行体通过签名校验，拒绝启动");
    1
}

fn collect_candidates() -> Vec<Candidate> {
    let mut out = Vec::new();
    if let Some(c) = candidate(user_root(), "用户副本", Kind::UserCopy, 0) {
        out.push(c);
    }
    if let Some(c) = candidate(seed_dir(), "deb 种子", Kind::Seed, 1) {
        out.push(c);
    }
    out
}

fn candidate(dir: PathBuf, label: &'static str, kind: Kind, _rank: u8) -> Option<Candidate> {
    let text = fs::read_to_string(dir.join(VERSION_NAME)).ok()?;
    let version = Version::parse(&text).or_else(|| {
        eprintln!(
            "docker-panel: {} 里的版本号无法解析：{text:?}",
            dir.display()
        );
        None
    })?;
    Some(Candidate {
        label,
        kind,
        dir,
        version,
    })
}

/// 用户副本目录：`$DOCKER_PANEL_RUNTIME_ROOT/current`（默认 `~/.local/opt/docker-panel/current`）。
fn user_root() -> PathBuf {
    if let Some(root) = env::var_os(ENV_USER_ROOT) {
        return PathBuf::from(root).join("current");
    }
    match env::var_os("HOME") {
        Some(home) => PathBuf::from(home).join(USER_SUBDIR).join("current"),
        None => PathBuf::from("/nonexistent/docker-panel"),
    }
}

fn seed_dir() -> PathBuf {
    env::var_os(ENV_SEED_DIR)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(SEED_DIR))
}

fn rank(label: &str) -> u8 {
    if label.starts_with("用户") {
        0
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn candidate_needs_parseable_version() {
        let dir = tempfile::tempdir().unwrap();
        assert!(candidate(dir.path().to_path_buf(), "种子", Kind::Seed, 1).is_none());
        fs::write(dir.path().join(VERSION_NAME), "0.7.18\n").unwrap();
        let c = candidate(dir.path().to_path_buf(), "种子", Kind::Seed, 1).unwrap();
        assert_eq!(c.version, Version::parse("0.7.18").unwrap());
        fs::write(dir.path().join(VERSION_NAME), "not-a-version").unwrap();
        assert!(candidate(dir.path().to_path_buf(), "种子", Kind::Seed, 1).is_none());
    }

    #[test]
    fn higher_version_wins_and_user_wins_ties() {
        let mut list = [
            Candidate {
                label: "deb 种子",
                kind: Kind::Seed,
                dir: PathBuf::from("/seed"),
                version: Version::parse("0.8.0").unwrap(),
            },
            Candidate {
                label: "用户副本",
                kind: Kind::UserCopy,
                dir: PathBuf::from("/user"),
                version: Version::parse("0.7.19").unwrap(),
            },
        ];
        list.sort_by(|a, b| {
            b.version
                .cmp(&a.version)
                .then_with(|| rank(a.label).cmp(&rank(b.label)))
        });
        assert_eq!(list[0].dir, PathBuf::from("/seed"));

        list[0].version = Version::parse("0.7.19").unwrap();
        list.sort_by(|a, b| {
            b.version
                .cmp(&a.version)
                .then_with(|| rank(a.label).cmp(&rank(b.label)))
        });
        assert_eq!(list[0].dir, PathBuf::from("/user"));
    }
}
