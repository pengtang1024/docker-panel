# 常见问题

## 连接与权限

### 启动后右上角红点，提示 permission denied … docker.sock

当前用户没有 Docker socket 的访问权限。把用户加入 `docker` 组并
重新登录：

```bash
sudo usermod -aG docker $USER
# 注销重新登录，或重启
docker ps   # 不再需要 sudo 即生效
```

临时方案：`sudo ./dvkit`（不推荐长期使用）。

### 如何连接远程 / 非默认位置的 Docker？

dvkit 尊重 `DOCKER_HOST` 环境变量，启动前设置即可：

```bash
# TCP（请确保网络可信，Docker daemon 默认不开放 TCP）
DOCKER_HOST=tcp://192.168.1.10:2375 ./dvkit

# SSH（需要本机已配置好免密或 ssh-agent）
DOCKER_HOST=ssh://user@host ./dvkit

# 自定义 socket 路径
DOCKER_HOST=unix:///run/user/1000/docker.sock ./dvkit
```

### 状态点一直是红色怎么办？

1. 确认 Docker 服务在运行：`systemctl status docker`；
2. 命令行验证：`docker ps` 是否正常；
3. 如果用了 `DOCKER_HOST`，检查地址、凭据与网络；
4. dvkit 每 2 秒会自动重试列表请求，恢复后状态点自动变绿，无需重启。

## 字体与显示

### 界面中文显示成方块 / 豆腐块

系统缺少中文字体。程序虽会自动扫描常见目录，但前提是系统里装过
CJK 字体：

```bash
sudo apt install fonts-noto-cjk     # Debian/Ubuntu
sudo dnf install google-noto-sans-cjk-fonts  # Fedora
sudo pacman -S noto-fonts-cjk       # Arch
```

装完重启 dvkit。正常情况下启动终端（若从终端启动）会打印
`[dvkit] 已加载中文字体: /path/to/font`。

### 图标位置显示方块

图标字体（Material Icons）已内嵌进二进制，正常不会缺失。
若自行修改了源码中的字体注入逻辑，检查 `load_cjk_fonts()` 里
`icons` 是否仍被追加到 `Proportional` 和 `Monospace` 两个家族末尾。

### Wayland 下能用吗？

可以。eframe 0.36 在 Linux 上走 winit，支持 Wayland 与 X11。
若个别合成器下窗口异常，可尝试强制 X11 兼容模式：

```bash
WINIT_X11_SCALE_FACTOR=1 ./dvkit
# 或设置 GDK_BACKEND=x11 配合 XWayland
```

## 资源数据

### 磁盘读写一直是 0？

宿主机如果是 **cgroup v2**（现代发行版默认，如 Ubuntu 22.04+、
Fedora），Docker stats API 不返回块设备 IO 明细，dvkit 显示 0，
这与 `docker stats` 的行为完全一致，不是 bug。查看 cgroup 版本：

```bash
stat -fc %T /sys/fs/cgroup   # cgroup2fs = v2
```

CPU、内存、网络数据在 v2 下不受影响。

### 内存数值和 docker stats 不一致？

dvkit 与 docker stats 采用同一公式：使用量扣除文件缓存
（v2 的 `inactive_file`、v1 的 `total_inactive_file`）。
若还有明显差异，留意是否在拿不同时刻的瞬时值对比——dvkit 每 2 秒
采样一次。

### CPU 偶尔超过 100%

正常。多核容器的 CPU% 可以超过 100%（如 2 核满载约 200%），
监控曲线的 Y 轴上限会随历史峰值自适应。

## 操作类

### 删不掉镜像，提示 image is being used

该镜像正被某个容器（包括已停止的容器）引用。先到容器 Tab 切到
「全部」，删除或重建相关容器后再删镜像。也可以用「简易清理」
按规则批量处理。

### 批量操作部分失败了？

批量启动 / 停止 / 重启 / 删除是逐项执行的。部分失败时错误窗会逐行
列出 `容器名: 错误原因`，成功项照常生效，修正问题后对剩余项重试即可。

### 日志窗口显示「已结束」

说明 Docker 侧日志流已关闭（常见于容器被删除）。容器重启后重新
双击该容器即可打开新的日志流。

### 清理「未使用镜像」会删掉我要用的镜像吗？

会删掉**所有没有被任何容器（含已停止容器）引用的镜像**，包括带
Tag 的，这与 `docker image prune -a` 语义相同。只勾「悬空镜像」
则只清理 `<none>:<none>`，更保守。执行前弹窗会再次提示，不可恢复。

## 构建类

### 编译报错：找不到 bollard / egui 指定版本

确认能访问 crates.io，或配置国内 cargo 镜像后 `cargo build`。
版本以仓库根目录 [Cargo.toml](https://github.com/pengtang1024/dvkit/blob/master/Cargo.toml)
为准。

### Windows 上想自己编译

安装 Rust（msvc 工具链）与 Docker Desktop，直接
`cargo build --release`。dvkit 不弹控制台窗口、无特殊系统依赖。

### dvkit 会写配置文件吗？

当前版本不持久化任何配置，也不常驻后台；关闭即退出，无残留。
列宽与显示列的调整只在本次运行内有效。

## 其他

### 项目名 dvkit 是什么意思？

**d**（Docker）+ **vkit**（小工具箱）：一个只做 Docker 日常管理的
小工具，保持单二进制、零守护、零账号。

### 支持容器 / 镜像的创建编辑吗？

不支持，也不在规划内。dvkit 定位是**轻量日常管理**（看状态、看日志、
看监控、启停删、拉取、清理）；`docker run`、compose 编辑、网络卷管理
请继续使用 Docker CLI / compose 文件。
