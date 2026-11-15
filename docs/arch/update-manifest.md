# 更新清单（version.json）与静态 CDN 分发：可行性与方案

> 隶属 [architecture.md 总纲](../architecture.md)。本文件是「静态更新清单 + 静态托管 / CDN」的**唯一权威源**：
> 字段契约、CI 生成顺序、客户端回退链、失败降级与安全边界。AGENTS.md 只留指针，不维护副本。
>
> 状态：**已实施**（客户端三级回退 + CI 生成步骤）。实测时间 2026-11-15，测试对象 `pengtang1024/docker-panel`，本机单出口网络。
> 实现落点：`src/update/manifest.rs`（解析 / 派生 / 断言）、`src/update/discover.rs`（回退链）、`src/update/mod.rs`（`build_info` 透传）、`.github/workflows/release.yml`（生成步骤）。

## 0. 结论

1. **可行**：静态清单能替代 `api.github.com` 做常规版本广播，前两级来源（Release 资产直链、jsDelivr）**不吃 API 配额**。
2. **但清单不能当信任根**：清单只提供「有个新版本号」这一条不可信广播。资产 URL 必须由版本号派生，SHA-256 必须只认 Release 的 `checksums.txt`（见 §7）。
3. **「更新后几分钟延迟」已由 CI purge 覆盖**：jsDelivr 分支端点 `s-maxage=43200`（12h 边沿缓存），tag 端点才是 `immutable`；发布流水线在提交清单后调官方 purge 接口（§3.7），purge 成功即「发布即可见」，失败才退回最长 12h 陈旧。主源 `releases/latest/download/` 任何时候都即时生效，不受缓存影响。
4. **不应替换 API，而应前置**：清单链全部失败时才回落 API，保留今天的行为作为兜底。

## 1. 实测基线

| 目标 | 次数 / 成功率 | 耗时 | 关键响应头 |
|------|---------------|------|------------|
| `cdn.jsdelivr.net/gh/…@develop/<文件>` | 16 / 16 | 0.52–0.58s（预热）；冷启 1.10–1.32s | `cache-control: public, max-age=604800, s-maxage=43200`、`x-jsd-version-type: branch`、`x-cache: MISS, MISS`、`age: 9/18` |
| `@v0.7.12` / `@0.7.12`（tag） | 均可解析 | 同上区间 | `cache-control: immutable, max-age=31536000`；`x-jsd-version` 归一化为 `0.7.12` |
| `@<commit sha>` | 可解析 | — | 同 tag 端点（不可变） |
| `raw.githubusercontent.com/…` | 10 / 10 | 0.50–0.56s | `cache-control: max-age=300`、`age: 0` |
| `api.github.com/repos/…/releases/latest` | 3 / 3 | 0.53–0.84s | 计入 60/h 配额 |
| 资产直链 `HEAD`（`releases/download/…`） | 1 / 1 | — | **302** → `release-assets.githubusercontent.com`，响应**无** `x-ratelimit` 头 |
| 资产直链 `GET -L checksums.txt` | 1 / 1 | 1.93s | 200；证明下载不吃 API 配额 |
| `rate_limit` 快照 | 1 次 | — | `limit 60 / remaining 26 / used 34`（同出口 IP 已被别人用掉 34） |

补充事实：

- `Content-Type` 可信：`.toml` → `application/toml`，`.md` → `text/markdown`；`version.json` 目前不存在，返回 **404 + `text/plain`（76B）**。
- 缓存协商：带 `If-None-Match` 实测返回 **304**（弱 ETag 生效）。
- 内容一致性：同一文件分别从 jsDelivr 与 `raw` 拉取，SHA-256 前 16 位相同（`5cc542bdb1ccf0d7`）。
- `releases/latest/download/version.json` 对**不存在**的资产也返回 **302**（跳到 `releases/download/<tag>/version.json` 再 404）→ 判存在性只能靠 `curl -f`（HTTP 错误 → 退出码 22），不能看状态码是否 302。
- `data.jsdelivr.com/…/resolved?specifier=develop` 返回 `version: null`，**不能**用于版本发现。

## 2. 清单字段与可信度

`version.json`（放 Release 资产里；发布成功后由 CI 自动提交到 `master` 供 jsDelivr 读取，见 §3）：

| 字段 | 类型 | 来源 | 可信度 | 用途 |
|------|------|------|--------|------|
| `schema` | int | 常量 `1` | 低 | 结构版本；不认识就整份丢弃 |
| `version` | string | CI 的 `GITHUB_REF_NAME` | **低（仅用于比较）** | 新版本号，喂给现有 `version::is_newer` |
| `version_code` | int | 由 `version` 派生 `major*10^6 + minor*10^3 + patch` | 低 | 纯数字比较的备用键（避免再解析字符串） |
| `channel` | string | 常量 `stable` | 低 | 预留；本期恒 `stable`，其它值整份丢弃 |
| `released_at` | string(RFC3339) | CI 构建时间 | 低 | 仅展示 / 诊断，**不参与任何判定** |
| `mandatory` | bool | 人工填（本期恒 `false`） | 低 | 预留「强制更新」；本期**解析后丢弃**（不落模型，避免死代码），行为同 `false`（§9） |
| `notes_url` | string | 固定 `…/releases/tag/{version}` | 低 | 更新说明页；只允许本仓库域名前缀，否则改回派生值 |
| `assets.deb.name` / `.size` | string / int | CI 从产出的 deb 读 | 低 | 资产名 + 字节数（大小只做「下载前预检」） |
| `assets.deb.sha256` | string(64 hex) | **从 `checksums.txt` 抽取** | **低** | 只能与 `checksums.txt` 交叉核对（§7） |
| `assets.binary.*` | 同上 | 同上（裸二进制） | 同上 | 同上 |

`version` 允许带 `v` 前缀（历史兼容），客户端一律归一化成 `major.minor.patch`；
`version_code` 必须与归一化后的 `version` 逐字对应（`major*10^6 + minor*10^3 + patch`），不一致整份丢弃。

**刻意不提供 `download_url`**：URL 由客户端按 `version` + `name` 拼成
`https://github.com/pengtang1024/docker-panel/releases/download/{version}/{name}`
（本仓库 tag **无** `v` 前缀，`git tag` 实测：`0.7.12`；写成 `v{version}` 会 404）。
这样即使清单被 CDN / 中间人篡改，也改不掉下载地址（只能改版本号，而版本号会立刻被 `checksums.txt` 与 404 拦住）。
若将来确实需要字段化 URL，必须加前缀白名单校验，且 `sha256` 仍只能与 `checksums.txt` 比对。

## 3. CI 生成顺序与提交时机

`.github/workflows/release.yml` 的 `release` job 内，**严格按此顺序**（`set -euo pipefail`）：

1. `cargo build --release --locked`
2. 生成 deb（`dpkg-deb --build`）
3. **生成 `checksums.txt`**（deb + `target/release/docker-panel` 两条，现有 step 不动）
4. **生成 `version.json`**：`version` 取 `steps.ver.outputs.version`，`sha256` 用 `awk` **从第 3 步的 `checksums.txt` 抽取**，`size` 用 `stat -c%s` 读产物，禁止在清单 step 里重新算一份独立来源；生成后 `python3 -m json.tool` 自检，坏了直接让 job 失败
5. `softprops/action-gh-release` 上传：现有 `files:` 追加 `version.json`（与 `checksums.txt` 同一批）
6. **提交 `version.json` 到 `master`**（已实现，见下）

顺序约束的理由：

- 4 在 3 之后：清单里的 `sha256` 必须与首发的校验文件逐字一致，两份来源分头计算迟早漂移。
- 5 在 4 之后：清单不能先于资产出现在任何可访问位置，否则存在「清单指向不存在的资产」窗口。
- 6 在 5 之后：**这是「清单指向的 Release 一定已存在」的来源**——`action-gh-release` 步骤成功即代表
  Release 与全部资产已上传完毕，此时才允许清单进入 jsDelivr 读取的 `master`；提前提交会让备源指向
  不存在的 Release（客户端会下载 404）。jsDelivr 的 `@master` 端点另有最长 12h 缓存，紧随其后的
  「刷新 jsDelivr 清单缓存」步骤会把这份缓存打掉（§3.7），主源时效不受这一步影响。
- 不做「清单先行、资产后补」：客户端失败会退化成「下载 404」，比「发现不了新版本」更难解释。

### 6. 清单提交到 master（jsDelivr 备源，已实现）

`release.yml` 的「提交更新清单到 master（jsDelivr 备源）」步骤，关键行为：

| 项 | 约定 |
|----|------|
| 触发条件 | 只在 Release 流水线（push `X.Y.Z` tag）中、**「发布 GitHub Release」步骤成功后**执行；发布失败则整步不运行 |
| 目标分支 | 固定 `master`（jsDelivr 端点读 `@master/version.json`，与 `develop` 无关） |
| 提交内容 | 只 `git add -- version.json` 一个文件；deb / 二进制 / checksums 等构建产物都是未跟踪文件，不入库 |
| 获取最新基线 | `git fetch --depth=1 origin master` → `git reset --hard FETCH_HEAD`（checkout 是 tag 的浅克隆，必须显式 fetch） |
| 并发保护 | 每次循环都重新 fetch 最新 `master`，再把发布产物覆盖回 `version.json` 后提交；push 非快进被拒时最多重试 3 次，**覆盖式重建、不做合并** → 冲突场景下清单内容始终以本次 release 产物为准 |
| 防自我循环 | 提交信息带 `[skip ci]`；且 `GITHUB_TOKEN` 推送的提交本身不会再触发新的 workflow，Release 也只由 tag 触发 |
| 权限 | job 级 `permissions: contents: write`（已有）；`master` 实测无分支保护 / 无 ruleset（`Branch not protected`） |
| 文件已一致时 | `git diff --cached --quiet` 直接成功退出，不产生空提交 |

**分叉影响（git flow）**：`master` 每发一版会比 `develop` 多一条 CI 清单提交。下一次
`git flow release finish` 时，release 分支合入 `master` 是普通三方合并——`master` 侧有 `version.json`、
release 侧没有该文件，合并结果保留 `master` 的旧清单，**不产生冲突**；随后 finish 把 tag 回灌 `develop`
（本项目历史为 `Merge tag 'X.Y.Z' into develop`），这条 CI 提交随 tag 进入 `develop`，分叉自动收敛。
因此不需要手工把 `master` 合回 `develop`；版本三同步与 `scripts/check-version.sh` 都不检查 `version.json`。
若某次流水线推送失败（权限 / 并发 / 网络），`master` 会停留在上一版清单，jsDelivr 备源保持陈旧（主源不受影响），
下一次发版或手工补提交即可。

### 7. 刷新 jsDelivr 缓存（已实现）

「提交更新清单到 master」之后紧跟一步 `刷新 jsDelivr 清单缓存`，调用官方去缓存接口
`https://purge.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json`，把 §0 第 3 条里的 12h 窗口消掉。

| 项 | 约定 |
|----|------|
| 触发条件 | 紧跟在「提交更新清单到 master」之后；上一步没跑（Release 发布失败）时本步虽然会执行，但清单仍是旧的，purge 也只是无害空刷 |
| 鉴权 | 免费公开接口，**不需要 token / secret**；仓库也无需任何额外配置 |
| 成功判定 | 响应 JSON 的 `status == finished`（最多 3 次，间隔 5s）；随后回读 CDN 校验 `"version"` 已是本版，最多 3 次 |
| 失败处理 | `continue-on-error: true` + 内部所有失败都只 `::warning` → **备源刷新失败不阻断发版**，退回最长 12h 陈旧 |
| 只刷清单 | purge 接口只能清 jsDelivr 自己缓存的路径。deb / 裸二进制 / `checksums.txt` 的下载地址在 `github.com/.../releases/download/...`（302 到 `release-assets.githubusercontent.com`），不经过 jsDelivr，其缓存由 GitHub 负责，因此**不刷也不存在可刷的对象**（§1 实测资产直链无 `x-ratelimit`、也不带 CDN 缓存头） |

purge 只影响备源时效：主源 `releases/latest/download/version.json` 发布即生效，不依赖这一步。

## 4. 客户端改动点（文件级）

| 文件 | 改动（已实施） | 说明 |
|------|------|------|
| `src/update/manifest.rs` | 新增 `parse_manifest` / `assets_from` / `assert_asset_sha` | 全部纯函数、无 IO；覆盖「合法 / 缺字段 / schema 不匹配 / sha256 不一致 / 版本回退 / tag 无 `v` 前缀」 |
| `src/update/discover.rs` | 新增三级回退 `fetch_release()` + 更新说明补全 | fetcher 可注入，单测覆盖「1 级命中 / 回落 2 级 / 全落 3 级 / 全失败」 |
| `src/update/mod.rs` | `spawn_check` 改调 `discover::fetch_release()`；`build_info` 透传 `manifest` | 调度、`update_busy` 互斥、节流不变 |
| `src/update/github.rs` | 保留 `curl_get` / `curl_error` / `parse_release`；`RawRelease` 加 `manifest` 字段 | 删掉不再被调用的 `fetch_latest`（回退链统一走 `discover`） |
| `src/models/update.rs` | 新增 `ManifestAsset` / `UpdateManifest`；`UpdateInfo` 加 `manifest: Option<UpdateManifest>` | `mandatory` 不落模型（未消费字段会触发 `dead_code`，而本期又不接 UI） |
| `src/update/install.rs` | **校验路径不变**，仅在 `checksum_for` 之后追加清单一致性断言 | `checksums.txt` 仍是唯一校验源；清单不一致 → 拒绝安装 |
| `src/ui_update/update.rs` / `src/callbacks_update.rs` | **不改** | 「无更新才写 `last_update_check`」的语义对清单路径同样成立 |
| `translations/zh_CN/LC_MESSAGES/docker-panel.po` | 新增 1 条 | 「更新清单与校验文件不一致，已拒绝安装」；清单不可用只走 `eprintln`，不进 UI |
| `.github/workflows/release.yml` | 新增「生成更新清单」step + `files:` 追加 + 发布后「提交更新清单到 master」step | 见 §3 |

清单解析失败是**诊断信息**（`eprintln` 带来源 URL），不新增用户可见文案：
三级全失败时用户看到的仍是第 3 级（API）的错误，与实施前一致。

## 5. 三级回退链

| 级 | 来源 | 优点 | 缺点 | 实测 |
|----|------|------|------|------|
| 1 | `https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json` | 发布即生效、零 API 配额 | 依赖 GitHub 可达；不存在时也要先走一次 302→404 | 首跳 302、`GET -L` 需一次重定向 |
| 2 | `https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json` | 国内可达性好、零配额；文件由发布流水线在 Release 成功后提交到 `master`（§3），并由下一步 purge 掉 12h 缓存（§3.7） | 依赖 CI 提交 + purge 步骤成功；purge 失败时退回最长 12h 陈旧（`s-maxage=43200`） | 0.7.13 发布时仍 404（当版无提交步骤）；0.7.14 实测：purge 前返回 `0.7.13`（`age: 12489`），purge 后立即返回 `0.7.14` |
| 3 | `https://api.github.com/repos/pengtang1024/docker-panel/releases/latest` | 现役路径，信息最全（含 notes / assets / size） | 60/h 未认证配额，同出口 IP 共享 | 3/3，0.53–0.84s；配额耗尽时 403 `remaining 0`（0.7.13 发布实测） |

编排规则：**上一级「可解析且结构合法」即停**，不再往下要。只有「请求失败 / 404 / JSON 非法 / `schema` 不认识 / `channel ≠ stable` / `version_code` 与 `version` 不符」才进下一级。
清单可解析但「版本不新」→ 直接判「已是最新」，**不回落 API**（避免每次启动两次请求；最长接受 12h 发现延迟，而现有节流本来就是 24h）。

**唯一的例外是「补更新说明」**：清单命中且 `is_newer` 为真时，额外 best-effort 拉一次第 3 级 API，
只为拿到 `body`（`notes`）与 `html_url`；版本号与清单不一致、或请求失败，都保留清单结果照常继续。
理由：清单刻意不承载 notes，而更新弹窗需要它；这条请求只在「确实有新版本」时发生（每个版本每台机器一次），
不会把「每次启动两次请求」的代价带回来。

## 6. 失败与降级行为

| 情况 | 行为 | 是否打扰用户 |
|------|------|--------------|
| 三级全部失败 | 自动检查：静默（与今天一致，`eprintln` 留痕）；手动检查：更新弹窗显示错误文案 | 仅手动 |
| 清单 404（首发前 / 老 tag） | 进下一级 | 否 |
| 清单 JSON 非法 / `schema` 不认识 | 进下一级，`eprintln` 记来源 | 否 |
| 清单版本 ≤ 当前 | 判定已是最新，写 `last_update_check` | 否 |
| 清单命中但补说明的 API 失败 / 版本不一致 | 照常提示更新，更新说明留空（可打开下载页看） | 否 |
| 资产 404（清单先于资产可见） | 安装阶段失败 → 提示「下载失败，可打开下载页」 | 是（仅安装） |
| 清单 `sha256` 与 `checksums.txt` 不一致 / 清单未声明该资产 | **拒绝安装**（fail closed），提示校验失败 | 是（仅安装） |
| `mandatory = true`（本期） | 解析后**丢弃**，行为同 `false` | 否 |

## 7. 安全边界

现状：只有 Release 内 `checksums.txt` 的 SHA-256，**无代码签名**；上游 Release 被篡改无法防御（AGENTS.md「当前已知限制」已记录）。

新增的篡改面 = 清单本身：攻击者若能改清单，可改 `version`（诱导升级/降级）、`mandatory`（诱导强制）与 `sha256`（诱导接受任意包）。
因此清单**必须**按下面三条收紧，否则 CDN 被劫持等于远程代码执行：

1. **`version` 只用于比较**，不用于拼任何可执行路径之外的语义；`channel` / `schema` 不认识就丢弃。
2. **`download_url` 不来自清单**，由 `version` + `name` 派生，域名与路径前缀写死在客户端代码里。
3. **`sha256` 以 `checksums.txt` 为准**：清单的 `sha256` 只能做交叉核对；`checksums.txt` 仍从同一 Release 的 assets 取（现有 `install.rs` 逻辑不变），两者不一致即拒绝安装。

清单与「什么」绑定才有意义：**与 `checksums.txt` 同源**（同一次 Release 上传、同一批资产）。
若把清单提交到 `master` 供 jsDelivr 读，它就与 Release 是**两个可独立修改的渠道** → 这正是不能把它当信任根的原因。
本期**不**引入 minisign / 自建签名（不在方案范围内），也不新增其它信任锚。

## 8. 文档与版本号三同步影响

- **版本号仍只有三处**（`Cargo.toml` / `packaging/debian/control` / `CHANGELOG.md`），`version.json` **不是**第四处手写源：它的 `version` 由 `GITHUB_REF_NAME` 派生，无需人工同步，`scripts/check-version.sh` 也**不检查**它。
- 本轮已同步：本文件（状态改为「已实施」+ 补实测值）、`AGENTS.md` 模块地图（`update/` 增 `discover` / `manifest`）与「当前已知限制」的「更新包无代码签名」条目、`docs/architecture.md` 的模块树与依赖表、`src/update/{mod,github}.rs` 模块注释、`CHANGELOG.md` 的 `[Unreleased]`。
- `docs/git-flow.md` 的发布流程不变（清单由 CI 生成，不进入人工发版清单）。

## 9. 不建议做

1. 把 jsDelivr 当**唯一**来源（12h 陈旧 + 单点）。
2. 把 `version.json` 当信任根，或在清单里放 `download_url`。
3. 用 `data.jsdelivr.com` 的 `resolved` 接口做版本发现（实测 `version: null`）。
4. 用 CDN 的 tag 端点做「最新版」探测（必须先知道 tag；tag 端点只是「已知版本取内容」）。
5. 用 `releases/latest/download/*` 的 **302** 判断资产是否存在（不存在的资产也 302）。
6. 在 Release 上传完成前把清单提交到 `master`。
7. 用 GitHub Pages 另起一套静态站（多一套部署 + 与 Release 双写，收益不抵维护）。
8. 本期做 `mandatory` 的强制更新 UI（没有「拒绝升级」的退出路径设计，弹窗只能变成打扰）。
9. 让清单的 `sha256` 绕过或替代 `checksums.txt`。
10. 为清单缩短 24h 节流或加轮询（清单不会比 Release 更实时，只会多打请求）。
11. 把清单塞进 `Cargo.toml` / `README.md`（Content-Type 与语义都不对，且污染源码文件）。
12. 为齐全信息而放弃 API 兜底（手动检查时用户需要 `notes.en` / 资产列表，清单不承载这些）；反过来也不要把 API 挪回主路径——它只做第 3 级兜底与「已确认有新版本时补说明」。

## 10. 风险与回归点

| 风险 | 影响 | 缓解 |
|------|------|------|
| 分支端点 12h 缓存 | 未 purge 时新版本最长 12h 才发现 | 发布流水线提交清单后调 jsDelivr purge 接口（§3.7）；purge 失败时第 1 级主源仍即时生效 |
| 共享出口 IP 的 60/h 配额（实测 `used 34`） | 第 3 级（API）失败 | 前两级零配额；失败即静默，手动检查才报错 |
| 清单与 `checksums.txt` 不一致 | 安装失败 | fail closed（§7），但不浮到自动检查路径 |
| CI 顺序写错（清单先于 checksums） | 清单 `sha256` 缺失/错值 | `set -euo pipefail` + 只从 `checksums.txt` 抽取，禁止手写常量 |
| 老版本客户端不认识清单 | 无 | 老客户端本来只走 API；新客户端对 404/非法结构一律回落 |
| `install.rs` 被顺手「优化」成只信清单 | 安全边界被拆掉 | `install.rs` 里已显式注释「期望值仍只来自 checksums.txt」，`manifest.rs` 与本文双向互指 |
| 本机出口 IP 配额耗尽（实测 `remaining 0`，API 返回 403） | 旧客户端（只有第 3 级）更新检查直接失败 | 新客户端前两级零配额；首个带 `version.json` 的 Release 发布后主路径不再依赖 API |
| `master` 多一条 CI 清单提交 | `develop` 与 `master` 每次发版后短暂分叉 | 下次 `release finish` 随 tag 回灌 `develop` 自动收敛，合并时无人工改动故不冲突（§3） |
| CI 提交步骤失败 / 被并发推送拒绝 | `master` 清单停留在上一版，jsDelivr 备源陈旧 | 步骤内 fetch 最新 `master` + 覆盖式重建 + 重试 3 次；主源与 API 兜底不受影响，下次发版自动补上 |

## 11. 已实测与待补测

0.7.13 正式发布已实测（2026-11-15）：

- 主源 `releases/latest/download/version.json` → 200（541 字节），`version` / `notes_url` / 资产名与 tag、Release 资产一致，两个资产的 `sha256` 与 `checksums.txt` **逐字节相同**，本地实下载的 `sha256sum` / 字节数同样吻合。
- 应用自身 `parse_manifest` 解析真实清单并派生 3 条资产 URL，HEAD 全部 200；`discover::fetch_release()` 在 API `remaining 0` / 403 时仍由清单命中（`from_manifest=true`），证明主路径不依赖 API。
- jsDelivr `@master/version.json` 与 `@0.7.13/version.json` 均为 404——该版流水线尚无提交步骤，符合预期。

0.7.14 发布已实测（2026-11-15）——「提交清单到 master」步骤首次真实运行：

- `master` 出现 `github-actions[bot]` 的 `chore(update): 同步 0.7.14 更新清单到 master [skip ci]`，只改 `version.json`；该文件 sha256 与 `releases/latest/download/version.json` **完全相同**，两个资产的 `sha256` 与 `checksums.txt` 逐字节一致。
- jsDelivr 在 purge 前返回 **200 但是 `0.7.13`**（`age: 12489`，正是 0.7.13 清单被边沿缓存的时长）；调 purge 接口后立即返回 `0.7.14`，sha256 一致 → **12h 窗口可被 purge 消掉**，这就是 §3.7 那一步的依据。
- 流水线整体 success，新增的 `刷新 jsDelivr 清单缓存` 步骤尚未随真实 Release 运行（本版发版时该步骤还不存在）。

以下内容**当前无证据**，后续版本补测：

- `刷新 jsDelivr 清单缓存` 步骤在真实 Release 中的表现（下一次发版观察 `purge 已受理` / 是否落到 `::warning`）。
- jsDelivr 在**不同运营商 / 省份**的成功率（本机单出口样本，不代表全国）。
- `If-None-Match` 304 只测了一次，未验证分支端点刷新后的 ETag 变更行为。
- `mandatory` 的 UI 交互与「拒绝升级」路径（本期不做，故无设计）。
