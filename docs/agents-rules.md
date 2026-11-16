# 规则配置层（规则地图 + 红线约束）

> **一句话**：本文件规定「什么绝对不能做、动手前要过哪几道门、改了代码必须同步哪些文档」——它是 agent 行为规则的**唯一权威配置层**，与执行主体彻底解耦；[AGENTS.md](../AGENTS.md) 只保留规则地图索引与红线摘要。
>
> **首次进入仓库：先读 AGENTS.md，再读完本文件，才允许开始任何开发活动。**

**对应源码**：无（纯规则）｜**相关页面**：[文档风格与排版规范](style-guide.md)、[Git Flow 规范](git-flow.md)、[架构设计](architecture.md)

## 0. 规则体系说明

- 两个核心维度：**规则地图**（§1，全部规则索引与权威源）+ **红线约束**（§2，不可逾越的硬性约束）。
- 优先级：红线 > 流程门禁 > 惯例约定；规则间冲突按 R3（架构优先）裁决。
- 规则变更只能发生在本文件，变更后同步 AGENTS.md 规则地图（见 §6.1 触发表）。

## 1. 规则地图

| ID | 规则 | 维度 | 校验方式 | 权威定义 |
|----|------|------|----------|----------|
| R0 | 项目名禁止出现废弃旧名 | 红线 | `grep -ri dvkit .`（命中仅允许存在于 AGENTS.md / 本文件的规则声明文本中） | AGENTS.md 顶部硬约束 |
| R1 | UI 验证统一走 agkit 截图 | 红线 | UI 修复必须附 `agkit shot --app docker-panel` 截图证据；slint/mcp 仅作调试数据暴露（元素树 / 属性） | AGENTS.md 顶部硬约束 |
| R2 | 去头部与注释后正文 >300 行必须拆分 | 红线 | 提交前逐文件核行数 | AGENTS.md 顶部硬约束 |
| R3 | 架构设计优先级绝对高于代码实现 | 红线 | §2.1 + §3 阶段门禁 | 本文件 §2.1 |
| R4 | 复用率 ≥80%，不足触发二次校验 | 红线 | §2.2 + §4 二次校验流程 | 本文件 §2.2 |
| R5 | 结构设计强制规范 | 红线 | §2.3 结构检查单 | 本文件 §2.3 |
| R6 | 冲突时架构/结构无条件优先 + 自动记录 | 红线 | §2.4 + §5 冲突记录表 | 本文件 §2.4 |
| R7 | 锁纪律 / Receiver 独占 / spawn 走 handle | 红线 | code review | AGENTS.md 代码规范 #2-5 |
| R8 | 文档同步触发表 | 流程 | §6 | 本文件 §6 |
| R9 | 版本号三同步 | 流程 | 发版检查单 | docs/git-flow.md |
| R10 | 命名 / 错误处理 / Slint 隔离等惯例 | 惯例 | code review | AGENTS.md 代码规范 #6-9 |

## 2. 红线约束

### 2.1 R3 架构设计优先级绝对高于代码实现

优秀的架构设计是系统长期可维护性、可扩展性与迭代演进能力的核心底层保障，**架构设计的优先级绝对高于单纯的代码实现**：

1. 所有开发活动（新功能 / 修 bug / 重构）动手前，必须先校验方案与现有架构（三线程模型、消息流、模块边界）的一致性。
2. 所有决策环节（需求取舍、方案选型、实现顺序）中，架构合理性校验优先于实现便利性。
3. 代码实现需求与架构设计冲突时**直接驳回**：先修订架构方案并同步文档，再回到实现；禁止为迁就实现而妥协架构。

### 2.2 R4 代码复用性核心评估机制

代码复用性纳入核心评估指标体系，作为方案选型的核心决策依据，**强制优先采用高复用性方案**，从根源上避免重复造轮子、冗余代码与重复开发：

1. 生成代码前**必须**先检索现有可复用资产：`ui/components-*.slint` 原子组件库（9 个按职能拆分）、`src/` 各模块公开函数、`Cargo.toml` 已有依赖、标准库。
2. 复用率 =（现有资产直接满足的功能点数 ÷ 方案功能点总数）× 100%。功能点清单在需求解析阶段列出，方案生成阶段逐项标注「复用来源 / 新建」。
3. **复用率 < 80% 的方案自动触发二次校验流程**（§4），未经二次校验通过不得进入代码输出阶段。

### 2.3 R5 结构设计强制规范

所有模块开发必须遵循清晰、合理、可扩展的结构设计规范，确保产出代码具备良好的可读性、可维护性与可演进性：

1. 单一职责：新模块先定边界再写码，一个文件只承担一类职责。
2. 依赖方向固定：`ui/`（声明层）← `slint_gen` ← `callbacks*` → `docker/` → `models`；禁止反向依赖与跨层直连。
3. UI 控件必须复用 `ui/components-*.slint` 原子组件，禁止零散硬编码同名控件。
4. 错误统一 `Result<T, String>` 上抛，UI 层展示中文文本；命名遵循 AGENTS.md 代码规范 #8。
5. 单文件去头部与注释后 ≤ 300 行（R2 口径）。

**违规处置**：结构设计不符合规范的代码输出直接标记为**不合格**，必须重构至合规后才允许交付或提交。

### 2.4 R6 冲突处理校验规则

代码实现与架构设计、结构设计出现冲突时：

1. **无条件优先保障架构与结构的合理性**。裁决顺序：架构 > 结构 > 代码实现细节。
2. 处理步骤：停止代码层面继续修补 → 回到架构/结构层确认合理性 → 确认无误后再进行代码细节优化；若架构/结构本身有误，先修架构/结构（含文档同步），再回到实现。
3. **升级红线**：同一问题连续 ≥ 2 轮局部补丁仍未收敛，强制升级为架构/结构层处理（必要时整模块重写），禁止继续打补丁。
4. **自动记录**：每次冲突必须当次追加到 §5 冲突记录表，作为规则迭代依据。

## 3. 全链路四阶段门禁

R3 / R4 / R5 作为硬性约束覆盖从需求解析到最终校验的完整链路，**任一阶段不达标不得进入下一阶段**：

| 阶段 | 门禁检查单 | 不达标处置 |
|------|-----------|-----------|
| S1 需求解析 | 列出功能点清单；识别架构影响面（线程模型 / 消息流 / 模块边界）；标注复用候选；判断是否触发 §6 文档同步 | 补齐解析后再进入 S2 |
| S2 方案生成 | 架构合理性校验（R3）；复用率评估，<80% → §4 二次校验（R4）；结构设计含行数预算与职责边界（R5） | 方案驳回，重新设计 |
| S3 代码输出 | 逐条自检 R0-R7；复用优先实现；结构规范落地 | 重写不合格部分 |
| S4 最终校验 | `cargo fmt` / `clippy` / `check` 通过；废弃旧名 grep 零残留；行数核验（R2）；§6 文档同步已执行；UI 变更附 `agkit shot` 截图证据（R1） | 修复后重新校验 |

## 4. 复用率二次校验流程（<80% 触发）

1. 重新检索组件库：`ui/components-*.slint`、`src/` 各模块公开函数、`Cargo.toml` 已有依赖、标准库。
2. 逐项说明不可复用理由（能力缺失 / 语义不符 / 改造成本高于新建）。
3. 复核后仍 <80%：允许新建，但必须在方案中记录豁免理由；后续出现同类功能点重复新建时，触发提炼公共组件的重构项。

## 5. 冲突记录表（append-only）

> R6 的自动记录载体。每次架构/结构与代码实现的冲突必须当次追加一行；历史条目不可删改，规则迭代时复查本表。

| 日期 | 场景 | 冲突描述 | 裁决结果 | 规则迭代动作 |
|------|------|----------|----------|--------------|
| — | — | — | — | — |

## 6. 文档同步规范

> 代码变更后，必须判断是否需要同步文档。以下是**触发时机**和**文档清单**的对应关系。

### 6.1 触发事件 → 需要更新的文档

| 触发事件 | 需同步更新的文档 |
|----------|------------------|
| 源码文件新增 / 删除 / 重命名 | README 项目结构树、AGENTS.md 模块地图、architecture.md 模块地图 |
| Slint 文件增删（ui/*.slint） | 同上 + docs/arch/ui-interaction.md（Slint 模块交互） |
| 依赖版本升级（slint / bollard / tokio / futures） | README 依赖表、AGENTS.md 技术栈表、architecture.md（技术栈 + 模块地图）、Wiki Architecture |
| 架构变更（线程模型、消息流 Msg 变体、模块交互方向调整） | AGENTS.md 线程模型 + 消息类型表、architecture.md 与 docs/arch/ 主题文档对应章节、README 架构概览 ASCII 图、Wiki Architecture |
| 功能特性变更（新增 / 删除 / 修改用户可见功能） | CHANGELOG.md、README 功能特性、AGENTS.md 当前已知限制、Wiki Home / Usage |
| 新增 bollard 类型坑条 | docs/arch/docker-bollard.md（唯一权威源） |
| 规则 / 红线变更 | 本文件对应章节 + AGENTS.md 规则地图同步 |
| 配置文件字段增删（src/config.rs） | docs/configuration.md 字段表 + Wiki Usage 设置段 |
| 命令行参数 / 全局快捷键变更（src/cli.rs、ui/app.slint） | docs/configuration.md + Wiki Usage |
| 用户可感知的错误文案 / 安装与卸载路径变更 | docs/troubleshooting.md 报错对照表 + Wiki FAQ |
| 新增翻译条目或新增语言支持 | docs/translations.md（msgid 计数）+ CONTRIBUTING.md 翻译入口 |
| 功能增删 / 方向调整 | `docs/roadmap.md`（当前阶段与候选方向）+ `README.md` 已知限制 |
| 新增文档文件 | README 文档导航 + 项目结构树 + 本文件 §6.2 清单；新文档必须按 [文档风格与排版规范](style-guide.md) 写 |
| 文档排版 / 结构改造 | `docs/style-guide.md` 是唯一规范；改造后按其中 §10 自检清单过一遍，两站渲染各抽查一次 |
| 发版（打 tag） | CHANGELOG.md 新增版本段落、三个版本号同步（Cargo.toml / debian control / CHANGELOG）、Wiki Home 版本声明；**Gitee 侧必须用 `scripts/sync-gitee-release.py` 补齐 tag 与 Release**（两平台并列，无主从） |
| 改发版流程 / 发布脚本 / `.github/workflows/release.yml` | `docs/git-flow.md` §3（双平台发布、不可变产物处置）+ `AGENTS.md` 发版流程段；**禁止把发布步骤改回一步到位的 `draft: false` + `files:`**（immutable 下必失败） |

> 表中所有 `Wiki` 均指 **GitHub Wiki 与 Gitee Wiki 两份**，同一变更必须两处都改（见 §6.2 / §6.3），避免两站内容漂移。

### 6.2 文档清单与职责

**主仓库内（随主仓库 commit）**：

| 文件 | 职责 | 谁是权威源 |
|------|------|------------|
| `AGENTS.md` | AI 协作入口：硬约束红线、规则地图索引、技术栈版本约束、线程模型/消息流速查、代码规范、已知限制 | — 入口型；规则全文以 agents-rules.md 为准 |
| `docs/agents-rules.md` | 规则配置层：规则地图 + 红线约束 + 四阶段门禁 + 冲突记录 + 文档同步规范 | ✅ **规则唯一权威源**（本文件） |
| `docs/architecture.md` | 架构总纲：技术栈、模块地图与依赖方向、文件规范 + 主题文档导航 | ✅ 总纲权威源 |
| `docs/arch/*.md` | 架构主题文档：threading（线程/消息/回调）· ui-interaction（Slint 交互/拆分原则）· list-layout（列表布局与列宽契约）· docker-bollard（Docker 接入 + bollard 坑条）· update-manifest（更新清单与 CDN 分发） | ✅ 各主题权威源 |
| `docs/configuration.md` | 面向使用者：config.json 15 个字段全表、环境变量、命令行参数、快捷键、自动更新调度 | ✅ 配置项权威源（字段以 src/config.rs 为准） |
| `docs/troubleshooting.md` | 面向使用者：按症状排查（启动/连接/列表/日志/更新/安装卸载/数据安全）+ 真实报错文案对照表 | ✅ 排障权威源（文案以 src/ 内 i18n 调用为准） |
| `docs/translations.md` | 翻译机制与贡献流程：264 条 msgid、t/trf/trfp 用法、新增语言步骤 | ✅ 翻译权威源 |
| `docs/roadmap.md` | 路线图：当前阶段、候选方向（未排期，不构成承诺）、明确不做的事 | ✅ 方向权威源（只写已想清楚的方向，不写日期） |
| `docs/style-guide.md` | 文档风格与排版规范：页头三件套、两站可用语法白名单、提示块口径、折叠与页脚导航、提交前 9 项自检 | ✅ **排版唯一规范** |
| `CONTRIBUTING.md` | 外部贡献入口：开发环境、分支与 PR、代码规范速览、文档同步门禁 | — 引用型，规范全文仍以 AGENTS.md / 本文件为准 |
| `SECURITY.md` | 漏洞报告渠道、支持范围、安全模型与不视为漏洞的设计边界 | ✅ 安全策略权威源 |
| `CODE_OF_CONDUCT.md` | 贡献者行为准则（Contributor Covenant 2.1）与四级执行阶梯 | — 上游模板，只在升级版本时改 |
| `SUPPORT.md` | 支持渠道（GitHub / Gitee Issues）与提问必附信息清单 | — 索引型 |
| `.github/ISSUE_TEMPLATE/`、`PULL_REQUEST_TEMPLATE.md` | Issue/PR 表单：版本、Docker 版本、发行版、会话类型等结构化字段；`config.yml` 关闭空白 issue 并把安全/排障/支持/Gitee 四条渠道前置 | — 与 SUPPORT.md 字段保持一致 |
| `README.md` | 人类读者的项目概览：功能特性、项目结构树、架构概览、安装使用、依赖表 | — 概览型，内容来自 AGENTS + architecture.md 精简版 |
| `docs/git-flow.md` | Git Flow 分支与发版规范 | — 只在发版流程变更时更新 |
| `CHANGELOG.md` | 版本变更记录，按 Keep a Changelog 格式 | ✅ 发版时必须更新 |
| `packaging/debian/control` | 版本号（与 Cargo.toml 同步） | ✅ 发版三同步之一 |

**Wiki（两份独立 git 仓库，内容须保持一致）**：

| 站点 | 仓库地址 | 推送目标 |
|------|----------|----------|
| GitHub Wiki | `https://github.com/pengtang1024/docker-panel.wiki.git` | `origin HEAD:master` |
| Gitee Wiki | `https://gitee.com/pengtang1024/docker-panel.wiki.git` | `origin HEAD:master` |

```bash
# 更新流程（两份各自一次性 clone → 改 .md → push，内容用同一份）
cd /tmp && git clone https://github.com/pengtang1024/docker-panel.wiki.git gh-wiki
cd /tmp && git clone https://gitee.com/pengtang1024/docker-panel.wiki.git gitee-wiki
# 把同一批 .md 文件拷进两个工作树，分别：
git add -A && git commit -m "docs(wiki): xxx" && git push origin HEAD:master
```

> Gitee Wiki 未初始化时，v5 API `POST /wikis` 会返回 405、直接 push `.wiki.git` 返回 404；需用带会话 cookie + csrf + access_token 的网页会话 POST 触发惰性创建，创建成功后 `git ls-remote` 才会出现 `Initial wiki`，再按上表 clone。

| Wiki 页面 | 对应 in-tree 文档 | 更新时机 |
|-----------|------------------|----------|
| Home.md | README + CHANGELOG | 功能增删 / 发版 |
| Install.md | README 安装章节 | 安装方式变更 / 二进制名改 |
| Usage.md | README 使用说明 + 源码 UI 变化 | 用户可见交互变更 |
| Architecture.md | docs/architecture.md + AGENTS.md 架构章节 | 架构变更（**必须与 in-tree 内容同步**） |
| FAQ.md | —（独立维护） | 发现新常见问题 / 废弃旧问题 |
| _Sidebar.md | —（导航） | 页面增删时同步 |

> **Wiki 内链接统一用标准 Markdown 相对链接** `[Label](Page)`（如 `[架构设计](Architecture)`）：GitHub Gollum 与 Gitee CommonMark 都渲染成 `<a>`。**禁止**用 `[[Page|Label]]`（Gollum MediaWiki 语法）——Gitee 的 CommonMark 解析器会把它原样输出成纯文本，变成坏链（实测 `POST /api/v5/markdown` 返回未转义的 `[[Architecture|架构设计]]`）。`_Sidebar.md` 是 GitHub Gollum 约定，Gitee 会当作普通页面列入原生树形导航，导航本身仍可用。
> **图片统一用国内可达的 jsDelivr GH CDN 绝对地址** `https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@develop/assets/screenshots/<name>.png`，不要写死 `raw.githubusercontent.com` 外链（国内常不可达）；Wiki 仓库自身不放二进制图片，避免两个平台各自维护一份资产。

### 6.3 双份同步原则

- **AGENTS.md ↔ architecture.md**：线程模型 / 消息流两处都有，架构变更时**两边必须同步更新**。AGENTS 放 ASCII 图 + 速查表（AI 读），architecture.md 放完整深度版（人读）。
- **in-tree ↔ Wiki**：Architecture 页与 `docs/architecture.md`、Install 页与 README 安装章节、Usage 页与 README 使用章节，**同一变更必须两处都改**，避免双源漂移。
- **GitHub Wiki ↔ Gitee Wiki**：两份 Wiki 是**两个平台各自的正式发布物，不存在主从**；任何 Wiki 变更必须**同一批改动同时推送到两个仓库**，禁止只更新一侧；推送后分别 `git ls-remote` 核对 HEAD 一致。README 文档导航需同时给出两个 Wiki 入口。
- 提交前快速校验：`grep -ri dvkit .`（零残留）+ 核对 README 项目结构树 + 核对各文件行数（R2 口径）。

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [架构设计](architecture.md) | [README 文档导航](../README.md#文档地图) | [文档风格与排版规范](style-guide.md) |
