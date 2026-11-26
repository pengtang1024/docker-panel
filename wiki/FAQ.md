# 常见问题（FAQ）

> 一句话：按症状查的排障清单——启动、连接、列表、日志、更新、界面行为、删除与数据安全。

**相关页面**：[Home](Home) · [Installation](Installation) · [Usage](Usage) · [Changelog](Changelog) · [Contributing](Contributing)

提问前请先备齐四行信息：`docker-panel --version`、`docker version`、`echo $XDG_SESSION_TYPE`、`echo $LANG`。完整排障步骤见仓库 docs/troubleshooting.md，求助渠道见 SUPPORT.md。

## 1. 启动与窗口

### 双击图标没反应 / 一闪就退？

先排除「其实已经启动过」：本项目是单实例应用，第二个实例会提示已有实例运行并以非 0 退出。`pgrep -a docker-panel` 有进程就是已在跑；用 `docker-panel --version` 确认二进制本身正常，再命令行启动看 stderr 报错。GNOME Wayland 下窗口不会被自动拉到前台，去面板坞或任务栏切换。

### 窗口空白 / 中文字体全是方块？

项目不内嵌字体，只从系统字体读字形。请安装中文字体（如 Noto CJK / 文泉驿）后重启应用；软件渲染器不依赖 GPU，空白窗口与显卡驱动无关。

### 配置文件损坏导致启动异常？

删除 `$XDG_CONFIG_HOME/docker-panel/config.json`（未设 `XDG_CONFIG_HOME` 时为 `~/.config/docker-panel/config.json`）即可回默认，详见 [Installation](Installation) §6。

## 2. 连不上 Docker

### 顶栏显示未连接？

按顺序确认：① `docker ps` 能否免 sudo 执行；不行就 `sudo usermod -aG docker $USER` 后重新登录。② 默认 socket 是 `/var/run/docker.sock`，`DOCKER_HOST` 只认 `unix://` 本机路径，tcp / ssh 远程地址不会被读取。③ 确认 Docker 服务 active（`systemctl status docker`）。

### 列表不刷新？

列表即时性只靠 Docker `/events` 事件订阅，400ms 合并窗口内首事件立刻刷、窗口内后续事件到期补刷一次；断流退避重连、连接建立补一次全量刷新，无定时兜底轮询。可先 `Ctrl+R` 手动刷新；若事件通道异常，stderr 会打印 `[events] stream error`。空闲零请求是设计，不是卡死。

## 3. 更新与校验

### 点“检查更新”没反应 / 更新报错？

应用先查静态清单（GitHub Releases → jsDelivr → GitHub API 三级回退），清单只广播版本号，资产地址从版本号派生。校验失败（签名不匹配 / 哈希与 `checksums.txt` 冲突）一律拒绝安装，这是 fail closed 设计，不是 bug。启动检查有节流，可在设置里手动检查。

### 自动安装的 deb 不弹授权框？

deb 自动安装依赖桌面 polkit 授权代理；没有代理时只提示并打开下载页，请手动 `sudo dpkg -i`。装在 `~/.local/bin` 的用户级二进制由 App 静默自替换，全程不需要 root（见 [Installation](Installation)）。

### 更新包安全吗？

0.8.0 起签名私钥不再进 CI：流水线只产草稿，维护者本机离线签名 `version.json` 与 `checksums.txt`（`.sig`），公钥清单 `packaging/release-signing.pub` 是唯一信任锚，应用与验签启动器都内置它。手工下载可用 `openssl pkeyutl -verify -pubin -inkey <公钥> -rawin -in version.json -sigfile version.json.sig` 离线复核。历史边界与不视为漏洞的设计见仓库 SECURITY.md。

## 4. 日志面板

### 日志为什么只有 2000 行？复制为什么不全？

日志窗口最多保留 2000 行，超出分块裁剪保留 1000 行；“复制日志”复制的是窗口内容，不是容器全部历史。

### 断流期间的日志能补回来吗？

仅 `json-file` 驱动生效：跟随流用时间戳游标，断裂后的重连按游标补回断流窗口内的行且不重复（单次回放上限 2000 行）；其它驱动 / 探测失败仍不补回。首次建流与 Clear 永不带游标参数，避免历史复活。时钟回拨时安全阀会丢弃游标退回 `tail=0`。

### 日志面板有哪几种提示？

四态：「日志流中断，正在重连…」/「日志流已恢复连接」/「日志流已停止」/「日志洪峰：已按比例抽样显示，省略 N 行」。洪峰抽样按通道积压反馈收敛，只影响显示、不破坏游标；容器停止后不自动重连，需重新切到日志 Tab。

## 5. 界面行为

### 为什么折叠 Compose 分组后计数变小？

概览带与状态 chips 的计数口径是可见容器集合（实际渲染成行的那些容器），折叠后同步变小，保证“数字 = 行数”。分组带汇总描述“这个组占了多少”，与是否展开无关。

### 为什么 CPU / 内存微条那么短？

微条用 docker stats 绝对口径（CPU = 相对单核百分比 / 100，内存 = 用量 / 上限），不是“视图内最大值归一”；极小但非零的比例有约 1px 可见下限。

### 镜像行的 ▶ 按钮为什么没反应？

▶ 当前只回显状态提示，尚未接入真正的 `docker run`。需要启动容器请用 Docker CLI / Compose；网络 / 卷也无行内删除入口，请用清理弹窗或 CLI。

### 主题“跟随系统”为什么没变化？

自动主题靠 gsettings（GNOME）或 GTK settings.ini 检测；非 GNOME 或检测不到时直接在顶栏 / 设置页切浅色 / 深色。界面文案按 `LC_ALL` > `LC_MESSAGES` > `LANG` 选择七语言，缺失译文回落英文源串。

## 6. 删除与数据安全

### 删除有二次确认吗？

没有。容器删除带 `force=true`，立即执行且不可恢复；批量删除前请自行确认选择范围。清理弹窗会先显示可清理数量再执行，清理同样不可恢复。

### 删不掉镜像，提示 image is being used？

该镜像正被某个容器（含已停止的）引用，先删除相关容器。只清悬空镜像层用清理弹窗的“悬空镜像”；带 tag 的未使用镜像需自行执行 `docker image prune -a`。

### Cleanup 的四个选项有什么区别？

| 选项 | 等价命令 | 说明 |
| --- | --- | --- |
| 已停止的容器 | `docker container prune` | 只清 exited / created 状态容器 |
| 悬空镜像 | `docker image prune` | 只清 `<none>:<none>` 的 dangling 镜像 |
| 未使用的网络 | `docker network prune` | 清未被任何容器引用的自定义网络（内置网络不动） |
| 未使用的卷 | `docker volume prune` | 清未被引用的匿名卷（Docker 默认口径，不含具名卷） |

## 7. 仍无法解决

带症状描述、复现步骤、截图与 §0 四行命令输出，到 [GitHub Issues](https://github.com/pengtang1024/docker-panel/issues) 或 [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues) 提问。安全漏洞不要发公开 Issue，走 SECURITY.md 的私有渠道。更多操作细节见 [Usage](Usage)，版本差异见 [Changelog](Changelog)。

## 页面导航

[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)
