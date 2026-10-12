# dvkit

极简 Docker 桌面管理工具，基于 Rust + egui 原生 GUI。单二进制、无 Electron、无运行时依赖，启动即用。

文档导航：[Wiki 首页](https://github.com/pengtang1024/dvkit/wiki) · [安装](https://github.com/pengtang1024/dvkit/wiki/Install) · [使用指南](https://github.com/pengtang1024/dvkit/wiki/Usage) · [架构设计](docs/architecture.md) · [常见问题](https://github.com/pengtang1024/dvkit/wiki/FAQ)

## 功能特性

### 容器管理（容器 Tab）

- 容器列表：短 ID、名称、镜像、状态、运行时长、端口映射
- 实时资源列：CPU%、内存占用（含百分比）、磁盘读写、网络 IO，CPU 超过 80% 自动标红
- 搜索过滤：即时匹配名称 / ID / 镜像 / 端口 / 状态，不区分大小写
- 筛选：仅正在运行 / 全部容器；手动刷新；自动刷新（每 2 秒，无感刷新不清空当前数据）
- 列排序：点表头按任意列排序，再点翻转升降序；以 ID 兜底保证刷新时行序不跳
- 自定义显示列：表头右侧「显示列」菜单勾选，可拖拽列宽，支持恢复默认
- Compose 分组：把 compose 启动的按项目折叠成组，名称旁有 `compose` 角标，点击组头折叠/展开
- 多选批量：复选框 / 单击行 / 表头全选，批量启动、停止、重启、删除（删除前列清单二次确认）
- 行内操作：日志、监控、启动、停止、重启、删除，Material 图标按钮 + 语义配色
- 右键菜单：查看日志、资源监控、启停重启、复制容器 ID、删除
- 双击行快速打开日志；单击行空白处切换选中
- 日志窗口：tail 200 行起步实时跟随，可暂停滚动、清空，最多保留 5000 行
- 资源监控窗口：CPU / 内存指标卡 + 折线小图，每 2 秒采样，保留约 4 分钟历史

### 镜像管理（镜像 Tab）

- 镜像列表：名称、Tag、大小、创建时间、关联容器数；悬空镜像黄色高亮
- 搜索过滤：即时匹配名称 / Tag / ID
- 拉取镜像：输入 `nginx:latest` 等规格，回车或点按钮拉取，实时显示分层下载进度
- 删除镜像：单个或多选批量删除，二次确认；被容器引用的镜像由 Docker 拒绝删除
- 右键菜单：复制镜像 ID、删除；同样支持列排序、自定义显示列、全选与批量操作
- 自动刷新（15 秒，仅停留在镜像 Tab 时）

### 简易清理

- 可勾选组合：已停止的容器 / 悬空镜像（dangling）/ 未使用镜像（等价 `docker image prune -a`）
- 执行前有不可恢复警告，完成后汇报各项删除数量与释放空间

### 界面与体验

- GitHub Dark 风格暗色主题，圆角、柔和投影、浮动滚动条
- 内置 Material Icons 图标字体；启动时自动扫描并注入系统中文字体（思源黑体 / Noto CJK / 文泉驿）
- 顶部 Docker 连接状态角标（黄=连接中 / 绿=正常 / 红=异常），不随刷新闪烁
- 破坏性操作全部二次确认；同一错误不重复弹窗；底部状态栏显示最近操作结果

## 环境要求

- Linux / macOS / Windows（主要在 Linux 上验证）
- 本机已安装并运行 Docker，默认通过 unix socket `/var/run/docker.sock` 连接；也支持 `DOCKER_HOST` 指定远程守护进程
- 构建需要 Rust stable（edition 2021）

## 安装与构建

### Debian / Ubuntu（deb 包）

```bash
sudo dpkg -i dvkit_0.1.0_amd64.deb
```

安装后在应用菜单中找到 dvkit；若点了没反应，多半是当前用户没有 Docker socket
权限，把自己加入 docker 用户组后重新登录即可：

```bash
sudo usermod -aG docker $USER
```

### 从源码构建

```bash
# 调试运行
cargo run

# 发布构建
cargo build --release
./target/release/dvkit
```

> 若系统缺少中文字体，界面中文会显示为方块。程序启动时会自动扫描
> `/usr/share/fonts` 等目录，优先加载思源黑体 / Noto CJK / 文泉驿，
> 安装 `fonts-noto-cjk` 或 `wqy-zenhei` 即可解决。

## 使用说明

1. 启动后默认进入 **容器** 页，只显示正在运行的容器；点「全部」可看历史容器。
2. 搜索框输入关键词即时过滤；点表头排序；「显示列」菜单自定义可见列；勾选「Compose 分组」按项目折叠。
3. 勾选多行后，工具条下方会出现批量操作条（启动 / 停止 / 重启 / 删除 / 清除选择）。
4. 行内图标按钮直接操作，也可以**右键**行打开菜单（含复制 ID）；**双击**行打开实时日志。
5. 点「监控」打开该容器的 CPU / 内存曲线窗口。
6. 切换到 **镜像** 页，顶部输入镜像规格回车即可拉取，进度显示在输入框下方。
7. 点「简易清理…」勾选清理范围，确认后执行。
8. 底部状态栏显示最近一次操作结果与容器数量；Docker 调用失败会弹错误窗（同一错误不重复弹）。

## 架构概览

```
UI 线程 (egui/eframe)                    tokio 后台运行时
┌──────────────────────┐    mpsc channel  ┌──────────────────────┐
│ app/                 │ ──── spawn ────▶ │ docker/ (bollard)    │
│  每帧 pump_messages  │ ◀── Msg 回传 ─── │  list/sample/pull/…  │
│  定时 maybe_refresh  │                  │  全部 async 不阻塞UI │
└──────────────────────┘                  └──────────────────────┘
   models.rs（数据结构） / utils/（格式化、字体、主题） / widgets/（无状态 UI 组件）
```

- 所有 Docker 调用都在 tokio 后台任务执行，结果经 `std::mpsc` channel 回传，UI 永不阻塞。
- 表格渲染期间只收集 `TableOutcome`（选中切换、排序点击、行内动作），渲染结束后统一应用，规避 egui 借用冲突。
- 资源采样并发上限 4（`buffer_unordered(4)`），且上一次采样未完成不会发起新的轮询。
- 列表刷新时保留上一次的资源数据并剪除已失效的选择，避免指标闪归零和批量操作误伤。

详见 [docs/architecture.md](docs/architecture.md) 与 [Wiki 架构设计](https://github.com/pengtang1024/dvkit/wiki/Architecture)。

## 项目结构

```
dvkit/
├── Cargo.toml              # 依赖与构建配置
├── assets/
│   └── MaterialIcons-Regular.ttf
├── packaging/              # desktop 入口与图标（deb 打包用）
├── src/
│   ├── main.rs             # 入口：创建 eframe 窗口
│   ├── models.rs           # 数据结构：ContainerInfo / ImageInfo / Msg 等
│   ├── app/                # GUI 主状态与渲染
│   │   ├── mod.rs          #   DvApp、全局状态、帧调度
│   │   ├── columns.rs      #   列定义、排序比较器、显示列菜单
│   │   ├── containers/     #   容器 Tab：工具条 / 预处理 / 表格 / 行
│   │   ├── images/         #   镜像 Tab：工具条 / 表格
│   │   ├── tasks/          #   后台任务发起、消息泵、定时刷新
│   │   └── windows/        #   日志、监控、清理、确认、错误/结果弹窗
│   ├── docker/             # bollard 封装（containers / images / system）
│   ├── utils/              # 主题色板、格式化、CJK + 图标字体加载
│   └── widgets/            # 无状态 UI 组件：按钮、表头、选择框、折线图
└── docs/                   # 项目文档
```

## 主要依赖

| crate | 用途 |
|-------|------|
| eframe / egui / egui_extras | 即时模式 GUI、表格 |
| bollard | Docker Engine API 客户端 |
| tokio | 多线程异步运行时 |
| futures | 流式处理（日志流、拉取进度、并发采样） |
| anyhow / serde_json | 错误与 JSON 处理 |

## 文档

- [项目 Wiki](https://github.com/pengtang1024/dvkit/wiki)（安装、使用指南、架构、常见问题）
- [架构设计](docs/architecture.md)（随仓库版本管理的开发文档）

## 许可证

MIT，见 [LICENSE](LICENSE)。
