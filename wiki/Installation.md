# 安装与构建

> 一句话：给要在一台 Linux 机器上把 Docker Panel 装起来的人看——先确认平台与 Docker 前提，再选 deb 或源码两条路之一，装完知道怎么卸干净。

**相关页面**：[Home](Home) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)

> 版本说明：本页不写死“当前版本”。下载文件名中的版本号请以 Releases 页面实际资产为准；版本历史见 [Changelog](Changelog)。

## 1. 平台范围（先读）

**Linux 是当前唯一验证与支持的平台**，Debian / Ubuntu 的 amd64 deb 是唯一官方发行物。

源码是可移植的 Rust + Slint 纯软件渲染程序，理论上可交叉编译到 macOS / Windows，但这两平台未验证；自动更新、deb 安装、pkexec 授权、Wayland `xdg app_id` 全链路仅 Linux。请勿把“源码可移植”当成“其它平台已获支持”。

## 2. 前提：Docker 已运行

Docker Panel 本身不携带 Docker，系统里必须已经有 Docker 守护进程在跑：

```bash
docker version          # 能正常打印版本即可
systemctl status docker # Linux 上确认服务 active
```

默认通过本机 unix socket `/var/run/docker.sock` 连接；`DOCKER_HOST=unix://...` 可指定其它本机 socket 路径。当前只识别 `unix://`，不读取 tcp / ssh 远程地址，见 [FAQ](FAQ)。

## 3. 安装 deb 包（Debian / Ubuntu amd64）

从 [GitHub Releases](https://github.com/pengtang1024/docker-panel/releases) 或 [Gitee Releases](https://gitee.com/pengtang1024/docker-panel/releases) 下载最新 deb 与同目录的 `checksums.txt`，先校验再安装：

```bash
sha256sum -c checksums.txt --ignore-missing
sudo dpkg -i docker-panel_<版本>_amd64.deb
```

安装后应用菜单里会出现 **Docker Panel**，点击启动。两站资产名与 `checksums.txt` 完全一致，下载慢时可换站点。

### Release 资产清单

| 资产 | 说明 |
| --- | --- |
| `docker-panel_<版本>_amd64.deb` | Debian / Ubuntu amd64 官方安装包 |
| `docker-panel` | 裸二进制（不含桌面入口 / 图标） |
| `version.json` / `version.json.sig` | 更新发现用静态清单及 Ed25519 签名 |
| `checksums.txt` / `checksums.txt.sig` | SHA-256 校验清单及签名 |
| `LICENSING.md` / `THIRD-PARTY-NOTICES.md` | 许可与第三方通知 |

### 校验下载完整性（务必做）

`checksums.txt` 每行格式为 `<64 位 hex>  <文件名>`（两个空格分隔），把 deb 与它放在同一目录后执行 `sha256sum -c checksums.txt`，全部回 OK 再安装。只下载 deb 时可单独核对 deb 那一行。

0.8.0 起签名清单与校验清单都有 Ed25519 签名（公钥清单 `packaging/release-signing.pub` 是唯一信任锚）；手工下载时可离线复核签名，做法见仓库 SECURITY.md。

### Docker socket 权限

启动后顶栏显示未连接、或提示 permission denied while trying to connect to the Docker daemon socket 时，是当前用户没有 socket 权限：

```bash
sudo usermod -aG docker $USER
# 重新登录（或 newgrp docker）后验证
docker ps
```

### 用户级免授权安装（推荐）

把二进制放到自己的目录，之后每次升级由 App 静默自替换，全程不需要 root：

```bash
install -Dm 0755 docker-panel ~/.local/bin/docker-panel
install -Dm 0644 packaging/docker-panel.desktop ~/.local/share/applications/docker-panel.desktop
sed -i "s|^Exec=.*|Exec=$HOME/.local/bin/docker-panel|" ~/.local/share/applications/docker-panel.desktop
```

更新弹窗里检查更新 → 立即更新：清单与 `checksums.txt` 都验签、SHA-256 复核通过才会写入磁盘，旧二进制留一份 `<exe>.previous` 供手工回滚。

系统级 deb 安装的 `/usr/bin/docker-panel` 是验签启动器：运行体在 `/usr/lib/docker-panel/seed/`，更新落到 `~/.local/opt/docker-panel/versions/`，启动器挑版本更高且验签通过的那份执行——deb 用户同样不用授权。历史版本下载与 immutable releases 说明见仓库 README“安装与构建”一节。

## 4. 源码构建与手工安装

需要 Rust stable（edition 2021，MSRV 1.99；`rust-toolchain.toml` 固定 stable 通道）。Debian / Ubuntu 上先装系统依赖：

```bash
sudo apt install build-essential pkg-config \
  libfontconfig1-dev libxkbcommon-dev libwayland-dev
```

缺 `libfontconfig1-dev` 会直接编译失败（不是警告）：`yeslogic-fontconfig-sys` 的 build script 找不到 `fontconfig.pc`。

```bash
git clone https://github.com/pengtang1024/docker-panel.git
cd docker-panel
cargo run                          # 调试运行
cargo build --release --locked     # 发布构建（opt-level=z + LTO + strip）
./target/release/docker-panel      # 启动
```

Gitee 站点：`git clone https://gitee.com/pengtang1024/docker-panel.git`。纯软件渲染器不依赖 `libgl1` / GPU / OpenGL / Vulkan，运行时只读取系统字体。

### 手工安装到系统

```bash
sudo install -m 755 target/release/docker-panel /usr/local/bin/docker-panel
sudo install -Dm 644 packaging/docker-panel.svg /usr/share/icons/hicolor/scalable/apps/docker-panel.svg
sudo install -Dm 644 packaging/docker-panel.desktop /usr/share/applications/docker-panel.desktop
update-desktop-database /usr/share/applications 2>/dev/null || true
```

贡献者向的构建、代码检查与 UI 截图验收流程见 [Contributing](Contributing)。

## 5. 其它 Linux 发行版

官方目前只产 amd64 deb；Fedora / RHEL、Arch 请走源码构建 + 手工安装（包名为等价映射，非实测）：

| 发行版 | 等价依赖安装命令 |
| --- | --- |
| Debian / Ubuntu | `apt install build-essential pkg-config libfontconfig1-dev libxkbcommon-dev libwayland-dev` |
| Fedora / RHEL | `dnf install gcc make pkgconf-pkg-config fontconfig-devel libxkbcommon-devel wayland-devel` |
| Arch | `pacman -S base-devel pkgconf fontconfig libxkbcommon wayland` |

暂不提供 rpm / AUR / AppImage / Flatpak / snap 包。Wayland 下程序显式设置 `xdg app_id = docker-panel`，让面板坞正确匹配应用图标。

## 6. 卸载与配置

| 安装方式 | 卸载做法 |
| --- | --- |
| deb | `sudo dpkg -r docker-panel` |
| 手工安装 | 删除 `/usr/local/bin/docker-panel`、desktop 与图标文件，再 `update-desktop-database` |

程序不常驻后台、不注册系统服务，卸载后只剩一个界面状态文件：

```bash
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/docker-panel/config.json"
```

字段含义、默认值与取值范围见仓库 docs/configuration.md；删掉即回默认。

## 7. 单实例

同一用户只允许一个 GUI 实例：再次启动会提示已有实例运行并以非 0 退出，同时向已有实例发一次前置请求。窗口前置只是尽力而为，GNOME Wayland 下通常不会自动到前台，需从面板坞 / 任务栏切换。`--version` / `-V` / `--help` / `--status` 在拿单实例锁之前处理，细节见 [Usage](Usage) 与 [FAQ](FAQ)。

## 页面导航

[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)
