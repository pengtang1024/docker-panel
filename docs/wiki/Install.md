# 安装与构建

## 1. 前提：Docker 已运行

dvkit 本身不携带 Docker，需要系统里已经有 Docker 守护进程：

```bash
docker version          # 能正常打印版本即可
systemctl status docker # Linux 上确认服务 active
```

默认通过本机 unix socket `/var/run/docker.sock` 连接，也支持
`DOCKER_HOST` 环境变量指向远程守护进程，见 [常见问题](FAQ.md)。

## 2. 安装 deb 包（Debian / Ubuntu）

从[仓库 Release 页面](https://github.com/pengtang1024/dvkit/releases)
下载 `dvkit_0.1.0_amd64.deb`，然后：

```bash
sudo dpkg -i dvkit_0.1.0_amd64.deb
```

安装后应用菜单里会出现 **dvkit**（System 分类），点击启动。

### Docker socket 权限

如果启动后右上角状态点是红色、提示权限不足（`permission denied while
trying to connect to the Docker daemon socket`），把当前用户加入
`docker` 组并**重新登录**（或重启）：

```bash
sudo usermod -aG docker $USER
```

验证：新开一个终端执行 `docker ps` 不再需要 sudo 即可。

## 3. 从源码构建

需要 Rust stable（edition 2021）：

```bash
git clone https://github.com/pengtang1024/dvkit.git
cd dvkit

cargo run              # 调试运行
cargo build --release  # 发布构建
./target/release/dvkit
```

### 手工安装到系统

```bash
sudo install -m 755 target/release/dvkit /usr/local/bin/dvkit
sudo install -Dm 644 packaging/dvkit.svg /usr/share/icons/hicolor/scalable/apps/dvkit.svg
sudo install -Dm 644 packaging/dvkit.desktop /usr/share/applications/dvkit.desktop
update-desktop-database ~/.local/share/applications 2>/dev/null || true
```

> 注意：`packaging/dvkit.desktop` 里的 `Exec` 默认指向开发构建目录，
> 手工安装时请改成 `Exec=dvkit` 或 `/usr/local/bin/dvkit`。

## 4. Windows / macOS

代码跨平台，但主要在 Linux 上验证：

- **macOS**：启动 Docker Desktop 后 `cargo run` 即可，bollard 会自动
  找到 Docker Desktop 的 socket。
- **Windows**：同样需要 Docker Desktop；在 PowerShell 里
  `cargo run --release`。

## 5. 中文字体

界面默认字体不含中文，dvkit 启动时会自动扫描以下目录：

- `/usr/share/fonts`
- `/usr/local/share/fonts`
- `~/.local/share/fonts`
- `~/.fonts`

按 **思源黑体 → Noto Sans/Serif CJK → 文泉驿 → Droid Sans Fallback →
Uming** 的优先级挑选，同族优先 Regular 字重，找不到时中文会显示为方块，
但程序仍可正常使用。

安装一款中文字体即可解决：

```bash
# Debian / Ubuntu
sudo apt install fonts-noto-cjk        # 或 fonts-wqy-zenhei

# Fedora
sudo dnf install google-noto-sans-cjk-fonts

# Arch
sudo pacman -S noto-fonts-cjk
```

图标使用内置的 Material Icons（已编译进二进制），无需额外安装。

## 6. 卸载

```bash
sudo dpkg -r dvkit     # deb 安装
# 或删除手工放置的二进制、desktop、图标文件
```

dvkit 不写任何配置文件、不常驻后台，卸载无残留。
