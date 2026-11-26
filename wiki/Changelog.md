# 更新日志（Wiki 摘要）

> 一句话：本页只做近期版本的高亮摘要；完整、逐条的版本记录以仓库 CHANGELOG.md 为唯一权威源。

**完整日志**：[CHANGELOG.md（GitHub）](https://github.com/pengtang1024/docker-panel/blob/develop/CHANGELOG.md) · [CHANGELOG.md（Gitee）](https://gitee.com/pengtang1024/docker-panel/blob/develop/CHANGELOG.md)

**相关页面**：[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Contributing](Contributing)

> 版本说明：Wiki 不写死“当前版本”。版本号的权威来源是 Cargo.toml / packaging/debian/control / CHANGELOG.md / Cargo.lock 四处同步结果；发布资产以 Releases 页为准。

## 近期版本高亮（按 CHANGELOG.md 实际条目归纳）

| 版本 | 类别 | 高亮 |
| --- | --- | --- |
| 0.8.3 | 修复 | 操作反馈不再挤进底栏，按归属分流到浮层 / 拉取弹窗 / 清理结果视图；修复更新弹窗说明区只显示 8 行、尾部被裁的问题 |
| 0.8.2 | 修复 | 堵掉三处多开漏口（抽象锁按 uid 全局唯一、文件 socket 不参与互斥、绑定失败 fail closed）；`auto_restart` 开关真正生效；“重启生效”提示不再残留 |
| 0.8.2 | 变更 | 自动检查节流 24h → 1h、轮询 6h → 30min；已下载且哈希匹配的更新包直接复用 |
| 0.8.1 | 修复 | 自替换后重启跑回旧版本（`/proc/self/exe` 指向 `.previous`）；启动自愈 |
| 0.8.0 | 新增 / 安全 | deb 安装改验签启动器 + 用户目录版本目录，静默升级免授权；公钥清单成为唯一信任锚；签名私钥不再进 CI，流水线只产草稿、维护者离线签名发布；`checksums.txt` 也签名 |
| 0.7.19 | 新增 / 性能 | 用户级自替换 + 签名清单，升级零 root；CPU% / 内存显示死区使空闲重绘大幅下降（实测 UI 主线程约 0.2% → 0.08%）；stats 保持 6s 整批投递 |
| 0.7.18 | 修复 / 变更 | 抽象 socket + 文件 socket 两道单实例锁（删锁文件不再多开）；事件合并窗口改双沿；UI 节流按批分档（8 / 120 / 250ms）；首屏请求提前 |

## 更长的版本线

| 阶段 | 关键变化 |
| --- | --- |
| 0.2.0 | 从 egui 完全重写为 Slint + 纯软件渲染器 |
| 0.3.x | 三态主题、右侧抽屉、拉取 / 清理弹窗、i18n、日志流自愈与虚拟化 |
| 0.4.x | 四视图统一列表与台账内联 Compose 树；许可证合规 |
| 0.5.0 | Compose 分组带组级操作与分组合并日志 |
| 0.6.x | 应用内自动更新（检查 / 下载 / SHA-256 / 安装 / 重启） |
| 0.7.x | 设置页、单实例、事件驱动刷新、静态清单三级回退、数据过期可见化等 |

0.4.3 之前的版本因第三方许可文件与 Slint 署名缺失，Release 与 tag 已下架，历史记录保留但不再提供下载。逐版本完整条目（含修复细节与设计取舍）请读仓库 CHANGELOG.md；发版与版本号同步规则见 [Contributing](Contributing) 与仓库 docs/git-flow.md。

## 页面导航

[Home](Home) · [Installation](Installation) · [Usage](Usage) · [FAQ](FAQ) · [Changelog](Changelog) · [Contributing](Contributing)
