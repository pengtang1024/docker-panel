# 故障排查

> **一句话**：给遇到启动失败、连不上 Docker、列表不动、更新装不上或日志异常的 Linux 桌面用户——先按症状表定位章节，每节都给出确认方式、修复动作与上报路径。

**相关页面**：[配置手册](configuration.md)（`config.json` 字段与重置）｜[README](../README.md)（功能与已知限制）

## 0. 症状速查

**结论**：先在这张表里按症状定位，再进对应章节；每节的结构都是「怎么确认 → 怎么修 → 修不好往哪报」。

| 症状 | 先看这里 |
|------|----------|
| 双击图标没反应 / 一闪就退 | [启动与窗口](#1-启动与窗口) |
| 窗口空白 / 花屏 / 字体全是方块 | [启动与窗口](#1-启动与窗口) |
| 配置文件损坏导致启动异常 | [启动与窗口](#1-启动与窗口) |
| 顶栏显示 `Disconnected` / 连不上 Docker | [连不上 Docker](#2-连不上-docker) |
| 列表不刷新 / 数据不更新 | [列表不刷新 / 数据不更新](#3-列表不刷新--数据不更新) |
| 点「检查更新」没反应 / 更新报错 | [自动更新](#4-自动更新) |
| 日志面板提示异常 / 复制不全 | [日志面板](#5-日志面板) |
| 二进制跑不起来 / 卸载有残留 | [安装与卸载](#6-安装与卸载) |
| 误删容器、镜像或卷 | [删除与数据安全](#7-删除与数据安全) |
| 以上都无法解决 | [仍无法解决](#8-仍无法解决) |

提 issue 时先备齐这四行底线信息（也用于逐节排查）：

```bash
docker-panel --version     # 应用版本
docker version             # Docker Server 版本（Server 段）
echo $XDG_SESSION_TYPE     # 显示会话（本项目仅验证 Wayland）
echo $LANG                 # 语言环境（影响界面语言与字体回退）
```

## 1. 启动与窗口

**结论**：先确认是不是「已经有实例在跑」，再区分 GUI 初始化失败与二进制本身失败，最后才怀疑配置文件。

### 双击图标没反应 / 一闪就退

**先排除「其实已经启动过」**：本项目是单实例应用，第二个实例会提示已有实例运行并以非 0 退出，同时向已有实例发一次前置请求。检查进程：

```bash
pgrep -a docker-panel
```

有进程就说明它已经在跑——但**窗口不会被自动拉到前台**（GNOME Wayland 下无合法的窗口激活途径，属已知限制）。去面板坞或任务栏切过去。

**命令行启动看真实报错**：

```bash
docker-panel --version     # 只打印版本后退出，不初始化 GUI、不连 Docker
docker-panel               # 正常启动，报错会打到 stderr
```

`--version` 能输出但 GUI 不出现 → 问题在 GUI 初始化；`--version` 也失败 → 问题在二进制本身（架构不符 / 依赖缺失，见 §6）。

### 窗口空白 / 花屏 / 字体全是方块

**结论**：这类问题基本都是系统字体缺失——本项目不内嵌字体，只从系统字体读字形。

按顺序排查：

```bash
# ① 中文字体是否被 fontconfig 索引到
fc-list :lang=zh | head

# ② 软件渲染器从系统字体读字形，没有内嵌 CJK 回退
#    为空就装一套中文字体
sudo apt install fonts-noto-cjk        # Debian/Ubuntu
sudo dnf install google-noto-sans-cjk-fonts   # Fedora
sudo pacman -S noto-fonts-cjk          # Arch

# ③ 装完刷新缓存后重启应用
fc-cache -fv
```

> [!NOTE]
> 本项目用纯软件渲染器（`software-renderer-systemfonts`），**不内嵌字体**，完全依赖系统字体。字体已装但仍显示方块 → 用 `fc-match "sans:lang=zh"` 确认实际命中的字体文件是否存在（不是只有配置项）。

### 配置文件损坏导致启动异常

**结论**：单个字段损坏或越界不会导致启动失败，只有整份 JSON 损坏才按默认填充；想彻底重置直接删文件。

配置文件在 `$XDG_CONFIG_HOME/docker-panel/config.json`，未设 `XDG_CONFIG_HOME` 时为 `~/.config/docker-panel/config.json`。

单个字段损坏 / 越界**不会**导致启动失败——加载时字段级回落到默认值。但想彻底重置界面状态，直接删掉即可：

```bash
rm -f "${XDG_CONFIG_HOME:-$HOME/.config}/docker-panel/config.json"
```

应用下次启动会用全默认值重建。字段含义见 [configuration.md](configuration.md)。

## 2. 连不上 Docker

**结论**：顶栏显示 **○ Disconnected**（未连接）是这一类问题的统一表现；按「权限 → 守护进程 → socket 路径」顺序查，绝大多数是第一条。

状态文本有三态：`Connecting...`（正在连接）/ `Connected`（已连接）/ `Disconnected`（未连接）。

### 权限不足（最常见）

```bash
# 症状：docker ps 需要 sudo，或直接报 permission denied
docker ps

# 修：把当前用户加入 docker 组
sudo usermod -aG docker $USER
# 必须重新登录（或至少 newgrp docker）才生效
```

重新登录后验证 `docker ps` 免 sudo，再重启 docker-panel。

### 守护进程没跑

```bash
systemctl is-active docker       # 应输出 active
sudo systemctl start docker
```

### socket 路径不是默认值

应用只识别 `unix://` 形式的 `DOCKER_HOST`（可指向其它本机 socket 路径），**不支持 tcp / ssh 远程地址**。

```bash
echo $DOCKER_HOST          # 为空则用默认 /var/run/docker.sock
ls -l /var/run/docker.sock
```

如果 Docker 装在了非默认位置（如 rootless 模式），把 `DOCKER_HOST=unix:///run/user/1000/docker.sock` 写进启动环境再启动应用。

### 应用内报错文案

状态栏或抽屉里可能出现：

| 文案 | 含义 |
|------|------|
| `Failed to connect to Docker: <详情>` | 建连失败，详情通常是 socket 不存在或权限被拒 |
| `Docker API timeout` | 单次请求超过 10s 未返回，通常是 dockerd 卡住或负载过高 |

超时不等于断开：列表刷新有 in-flight 去重，短时抖动会自愈。持续超时请查 `journalctl -u docker -n 100`。

## 3. 列表不刷新 / 数据不更新

**结论**：本项目**没有定时兜底轮询**，列表即时性完全由 Docker `/events` 事件流驱动（另加 6s stats、30s 磁盘占用两个定时任务）；「列表不动」通常是事件流断了。

```bash
# 手动验证事件流是否可用（应持续输出 JSON 事件）
curl --unix-socket /var/run/docker.sock -N http://localhost/events
```

| 验证结果 | 判断 | 下一步 |
|----------|------|--------|
| 事件流持续输出，但应用不动 | 应用侧断流重连中 | 重连按 1s→2s→4s→8s→10s 退避，并 `eprintln!("[events] stream error")` 打到 stderr；用命令行启动看有没有这行 |
| 事件流不可用（curl 也无输出） | 问题在 Docker 侧 | 代理 / socket 转发不支持长连接，不是应用的问题 |

顶栏出现「实时数据可能已过期」提示（`Live data may be stale` / `Disk data may be stale` / 两者合并文案）表示采样连续多轮无结果：stats 连续 3 轮（约 18s）、磁盘占用连续 2 轮（约 60s）才提示，恢复一轮即自动清除。**这是提示，不是错误**；持续不消失再查 Docker。

## 4. 自动更新

**结论**：更新链路是「检查版本 → 下载 → 校验 SHA-256 → 安装」，每步都可能独立失败；国内网络下前两级检查源可能超时，回退耗时较长属正常。

### 检查更新没反应 / 一直检查中

检查走三级回退（GitHub 静态清单 → jsDelivr → `api.github.com`），全程用系统 `curl`。**国内网络下前两级可能超时**，应用会依次回退，耗时较长属正常。

```bash
# 手动验证三级来源是否可达
curl -sIL https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json | head -1
curl -sIL https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json | head -1
curl -sIL https://api.github.com/repos/pengtang1024/docker-panel/releases/latest | head -1
```

三级全不可达 → 网络问题，手动去 Releases 页下载。

### 报错对照表

**结论**：报错文案是唯一可靠的判据——先按文案定位原因，再决定手动安装还是报告。

<details>
<summary><b>完整错误文案对照表（11 条）</b></summary>

| 文案 | 原因 | 处理 |
|------|------|------|
| `No installable asset for this release` | 该 Release 没有匹配当前平台的资产 | 手动下载 deb |
| `Release checksums.txt is missing` | Release 缺校验文件 | 手动下载，**不要装没有校验文件的包** |
| `Release checksums.txt does not contain this asset` | 校验文件里没有该资产条目 | 同上，建议报告 |
| `Update manifest does not match checksums.txt` | 清单与 Release 校验文件冲突，**客户端 fail closed 拒绝安装** | 属安全设计；手动下载并核对 |
| `Update manifest signature is not verifiable: <详情>` | 清单签名缺失 / 被改写 / 与内置公钥不匹配（`version.json.sig` 下载失败或内容不对） | 属安全设计，**不要绕过**：先确认网络没被代理改写，再用 `openssl pkeyutl -verify` 配 `src/signature.rs` 里的公钥离线复验；持续出现请报告 |
| `Update manifest is malformed` / `does not match the release` / `does not match the package` | 签名清单结构错、版本对不上、或清单声明的 SHA-256 与实际 deb 不符 | 同上，fail closed；不要用「跳过校验」的方式安装 |
| `Version X is already installed, restart to apply` | 清单版本已经装在系统里（dpkg 记录 ≥ 清单版本），只需重启应用 | 重启即可；不会再重复下载与授权 |
| `Package installation failed: <输出>` | `pkexec dpkg -i` 失败（依赖冲突 / 授权被拒 / 无 polkit） | 按提示核对；也可手动 `sudo dpkg -i` 装下载好的 deb |
| `Update checksum mismatch` | 下载产物与声明的 SHA-256 不符 | 删除缓存重下；反复不符请报告 |
| `Automatic install is not supported` | 开发构建 / 非 Linux / 目录不可写 | 手动 `sudo dpkg -i` |
| `Failed to locate install directory` | 找不到当前可执行文件所在目录 | 从固定路径启动，或手动安装 |
| `Failed to replace binary: <详情>` | 裸二进制自替换失败（目录只读等） | 手动替换 |
| `pkexec not found, please install the deb manually` | 系统没有 pkexec | `sudo apt install pkexec` 或手动装 |
| `Failed to run pkexec: <详情>` | pkexec 启动失败 | 见下节「pkexec 授权框不弹 / 弹了但点取消」 |
| `Package installation failed: <详情>` | dpkg 返回失败 | 详情通常含依赖冲突，见下节「deb 依赖冲突」 |

</details>

### pkexec 授权框不弹 / 弹了但点取消

**装在用户目录（推荐）时不需要任何授权**：目录可写 → App 验签后自替换（见上一节）。只有系统级 deb 安装（`/usr/bin`，不可写）才走 `pkexec dpkg -i`，**依赖桌面环境的 polkit 授权代理**。

```bash
which pkexec                  # 没有就装
systemctl status polkit       # 应为 active
```

| 现象 | 原因 | 处理 |
|------|------|------|
| 授权框完全不出现 | 无桌面环境（纯终端 / SSH 会话），polkit 没有代理 | 授权框不可能弹出，手动 `sudo dpkg -i` |
| 授权框弹出后取消 | 安装中止 | 重新点一次即可 |
| 授权框不出现且 polkit 正常 | 可能是会话类型不匹配（远程桌面 / 嵌套会话） | 同样走手动安装 |

### deb 依赖冲突

```bash
sudo dpkg -i docker-panel_<版本>_amd64.deb
# 若报依赖问题：
sudo apt install -f
```

本项目 deb **不依赖 libgl1**（纯软件渲染），所以 GPU 相关依赖冲突与本应用无关。若与已装版本冲突：

```bash
sudo dpkg -r docker-panel        # 先卸载旧版
sudo dpkg -i docker-panel_<版本>_amd64.deb
```

### Release 下载 404 / tag 不存在

| 现象 | 原因 | 处理 |
| --- | --- | --- |
| 0.7.15 及以后的版本打不开 | 版本号写错，或该 Release 尚未发布 | 在 [Releases 列表](https://github.com/pengtang1024/docker-panel/releases) 里挑实际存在的版本 |
| **0.7.16 找不到 Release** | 该版本 CI 构建失败（`Cargo.lock` 未同步版本号），只推了 tag 没发 Release；tag 受保护不可重指 | 直接用 0.7.17（已包含该版本的全部改动） |
| **0.4.3 – 0.7.14 的 GitHub 链接 404** | 这些 Release 与 tag 在 GitHub 侧因 immutable releases 的 tag 记录限制暂不可重建 | 改从 [Gitee Releases](https://gitee.com/pengtang1024/docker-panel/releases) 下载；资产名与 `checksums.txt` 与 GitHub 完全一致 |
| 0.4.2 及更早 | 缺少第三方许可文件与 Slint 署名，已按许可要求下架 | 不提供下载；变更记录仍在 [CHANGELOG](../CHANGELOG.md) |

### 手动安装与校验

```bash
# 下载 deb（或裸二进制）与同目录的 checksums.txt 后
sha256sum -c checksums.txt
# 看到 OK 再安装
sudo dpkg -i docker-panel_<版本>_amd64.deb
```

`checksums.txt` 每行格式为 `<64 位 SHA-256>  <文件名>`。Gitee Release 与 GitHub Release 的资产名一致，可互为备份（国内建议 Gitee）。

```bash
# 0.7.19 起清单本身带 Ed25519 签名：先验签，再按清单里的哈希核对 deb
openssl pkeyutl -verify -pubin -inkey panel-pub.pem -rawin \
  -in version.json -sigfile version.json.sig
# → signature verified: version 0.7.19 deb sha256 360ac7370e40
```

`version.json` 的 `assets.deb.sha256` 必须与 `checksums.txt` 里该 deb 的哈希一致，两者不一致说明产物被改过。

> [!WARNING]
> **如实告知**：deb **本身没有 dpkg / debsig 签名**；0.7.19 起由 `version.json` 的 Ed25519 签名把「版本号 + 资产哈希」绑到发布私钥上，应用内更新只安装「签名清单声明且哈希匹配」的包。手工从 Release 页下载安装（`sudo dpkg -i`）仍只有 HTTPS + `checksums.txt` 这两道，无法证明上游未被攻破。详见 [SECURITY.md](../SECURITY.md)。

## 5. 日志面板

**结论**：工具条文案就是日志流状态机的状态位——看到哪条提示，就知道流处于哪个分支，多数情况不需要干预。

| 现象 | 说明 |
|------|------|
| 工具条显示「日志流中断，正在重连…」 | 流断了，正在按 1s→2s→…→10s 退避重连，不会放弃 |
| 显示「日志流已恢复连接」 | 重连成功并收到首行 / 或容器仍存活（探针可达 / 空闲保活判定） |
| 显示「日志流已停止」 | 容器已停止或被删除；切到别的 Tab 再切回来才会重新建流 |
| 显示「日志洪峰：已按比例抽样显示，省略 N 行」 | 日志量超出 UI 消费能力，**只影响显示**，容器内的完整日志不受影响 |
| 只看到最近 2000 行 | 日志窗口上限 2000 行，超出后分块丢最旧一段（保留 1000 行） |
| 「复制日志」复制不全 | 复制的就是窗口内内容（与面板所见一致），不是容器完整历史 |

断流窗口内的日志**会被补回**（仅 `json-file` 日志驱动生效），单次最多补 2000 行。容器用其它日志驱动时断流期间的行不会补回。

## 6. 安装与卸载

**结论**：官方只发 **amd64** 包；卸载分 deb 与手工安装两种，配置目录不会随卸载自动删除。

### 二进制跑不起来（架构 / 依赖）

```bash
file $(which docker-panel)     # 应为 x86-64 ELF
ldd $(which docker-panel)      # 看缺失的共享库
```

官方只发 **amd64** 包。ARM 或其它架构需自行从源码构建，见 [CONTRIBUTING.md](../CONTRIBUTING.md) 的开发环境一节。

### 卸载

**deb 安装：**

```bash
sudo dpkg -r docker-panel
```

**手工安装（源码构建后 `install` 到 /usr/local）：**

```bash
sudo rm -f /usr/local/bin/docker-panel
sudo rm -f /usr/share/applications/docker-panel.desktop
sudo rm -f /usr/share/icons/hicolor/scalable/apps/docker-panel.svg
sudo update-desktop-database 2>/dev/null || true
```

**清理配置（两种安装方式通用，配置不会随卸载自动删除）：**

```bash
rm -rf "${XDG_CONFIG_HOME:-$HOME/.config}/docker-panel"
```

单实例锁由两道机制组成：权威互斥是**抽象命名空间 socket**（`@docker-panel-<16hex>`，不落盘、进程退出由内核回收），另有一个文件 socket `/run/user/<uid>/docker-panel-<uid>.sock`（无法创建时回退 `/tmp`）只承担前置请求（raise）与可见性。进程正常退出时文件 socket 会被删除；异常退出（SIGKILL / panic abort）留下的陈旧 socket 下次启动会自动清理并重绑，**不需要手动删**——即使手动删掉，抽象锁仍然拦住第二个实例。查询是否已有实例在跑用 `docker-panel --status`（退出码 0=在跑 / 1=没跑）。

### 桌面图标不对 / Dock 显示「未识别程序」

Wayland 下应用显式设置了 xdg app_id 为 `docker-panel`，用于匹配 `docker-panel.desktop`。图标异常通常是 desktop 文件没装到标准路径：

```bash
ls /usr/share/applications/docker-panel.desktop
sudo update-desktop-database
```

从源码跑（`./target/release/docker-panel`）而没执行 `install` 时没有 desktop 文件，Dock 只能显示通用图标——这是预期行为。

## 7. 删除与数据安全

> [!CAUTION]
> 删除操作没有二次确认，且容器删除带 `force=true`，不可恢复。

| 对象 | 后果 | 注意 |
|------|------|------|
| 容器 | 删掉后只能从镜像重建，容器内可写层数据随之丢失 | 挂载的具名卷不会被删除（数据仍在），匿名卷会被一并清理 |
| 镜像 | `image is being used` 表示仍有容器在用 | 先删容器 |
| 批量删除 | 台账勾选是跨行累积的，筛选变化不会自动清空选择 | 删除前确认选择范围 |

**误删后的处理**：本项目不提供回收站或操作撤销。容器需重建；卷数据可在 `docker volume ls` 中确认是否仍存在。

## 8. 仍无法解决

**结论**：先按 [SUPPORT.md](../SUPPORT.md) 收集信息再提 issue；安全漏洞不要走公开 issue。

| 类型 | 渠道 |
|------|------|
| 缺陷 | [GitHub Issues](https://github.com/pengtang1024/docker-panel/issues/new/choose) · [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues) |
| 安全漏洞 | **不要发公开 issue**，走 [SECURITY.md](../SECURITY.md) 私有渠道 |
| 概念解释类问题（为什么不轮询、洪峰抽样口径等） | Wiki [常见问题](https://github.com/pengtang1024/docker-panel/wiki/FAQ) |

**相关文档**：

| 文档 | 内容 |
|------|------|
| [配置手册](configuration.md) | config.json 字段全表与重置 |
| [SUPPORT.md](../SUPPORT.md) | 提问渠道 |
| [SECURITY.md](../SECURITY.md) | 安全模型 |
| [README](../README.md) | 功能与已知限制 |
| Wiki [安装与构建](https://github.com/pengtang1024/docker-panel/wiki/Install) | 安装步骤与构建方式 |
| Wiki [使用指南](https://github.com/pengtang1024/docker-panel/wiki/Usage) | 日常操作说明 |
| Wiki [常见问题](https://github.com/pengtang1024/docker-panel/wiki/FAQ) | 概念与设计口径 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [配置手册](configuration.md) | [README 文档地图](../README.md#文档地图) | [翻译与 i18n](translations.md) |
