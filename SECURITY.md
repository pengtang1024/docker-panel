# 安全策略

> **一句话**：给「发现了安全问题想私下报」和「想确认某个行为到底算不算漏洞」的人看的一页——报告渠道、支持范围、以及哪些边界是已知且有意为之。

**对应源码**：[src/update/install.rs](src/update/install.rs)（下载、SHA-256 校验与安装）、[src/update/discover.rs](src/update/discover.rs)（清单三级回退）、[src/single_instance.rs](src/single_instance.rs)（单实例锁）｜**相关页面**：[docs/arch/update-manifest.md](docs/arch/update-manifest.md)

> [!IMPORTANT]
> **运行本应用的账户等同于拥有 root 等价权限。** 应用直接与本机 Docker Engine 守护进程通信（`/var/run/docker.sock`），能启动、删除容器与镜像。理解这一点，是理解本项目安全模型的前提。

## 1. 报告漏洞

**结论：安全问题不要发公开 Issue、讨论区或社交平台，走 GitHub 私有漏洞报告通道。**

> <https://github.com/pengtang1024/docker-panel/security/advisories/new>

该通道只在维护者与你之间可见，适合附带复现步骤、PoC 或敏感日志。报告时尽量补齐下面四项：

| 要附的信息 | 为什么需要 |
|------------|------------|
| 受影响的版本（`docker-panel --version`） | 支持范围按版本划分，缺版本号无法判断是否已修 |
| 发行版与显示会话（本项目仅验证 Wayland） | 显示会话与系统库差异直接影响能否复现 |
| 复现步骤与预期 / 实际行为 | 无复现步骤的报告只能靠猜，通常会先被要求补一轮 |
| 是否需要特定 `DOCKER_HOST` 或配置组合 | 连接方式与配置组合会改变可达的攻击面 |

## 2. 支持范围

**结论：只对最新 Release 与近期的 0.7.x 修复，0.4.3 之前的版本已全部下架、不再支持。**

| 范围 | 状态 |
|------|------|
| 最新 Release（当前 v0.7.x） | ✅ 接受报告并修复 |
| 更早的 0.7.x | ⚠️ 视严重程度决定是否回补 |
| 0.4.3 之前 | ❌ 不再支持——该批 Release 与 tag 已因第三方许可文件与 Slint 署名缺失全部下架 |

## 3. 响应预期

**结论：维护者会评估每一份报告并回复结论，但不承诺固定响应时限或修复 SLA。**

本项目由个人维护，请预留合理等待时间，不要因为未即时回复就在公开渠道披露细节。每份报告都会得到一个明确结论：确认 / 需要更多信息 / 判定为设计边界。

## 4. 现有安全模型

**结论：这一节描述的是已知边界而不是待修漏洞——撞上下列行为符合预期，不需要报告。**

### 更新包没有代码签名

自动更新只依赖两层保护，没有 GPG / minisign 签名，也没有独立的公钥信任链：

| 保护层 | 作用 |
|--------|------|
| HTTPS | 传输加密，覆盖静态清单与 Release 资产 |
| 同一 Release 内的 `checksums.txt`（SHA-256） | 校验下载产物的完整性 |

> [!WARNING]
> **上游 GitHub Release 本身被篡改时客户端无法防御。** 这是当前设计接受的已知风险，不是遗漏——签名与公钥信任链都不在现有实现里，报告这一点会被判定为设计边界。

### 更新发现的三级回退与 fail closed

应用按顺序尝试三个来源获取版本清单：

1. `https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json`
2. `https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json`
3. `https://api.github.com/repos/pengtang1024/docker-panel/releases/latest`

关键约束两条：

- 清单**只用作版本号广播**。清单里刻意不提供 `download_url`，资产地址一律由版本号派生为 `releases/download/{version}/{asset}`，避免 CDN 成为可伪造的分发入口。
- 完整性校验**只认** Release 内的 `checksums.txt`。清单声明的 sha256 与该文件冲突、或清单未声明某资产时，客户端 **fail closed，拒绝安装**。

### 校验和格式与自助验证

`checksums.txt` 每行是 `<64 位十六进制 SHA-256>  <文件名>`，例如 0.7.14 含两条：

```text
6d505826459a45e32395915126f4fd99d14c67e0f99af983e49eb87a9c8002b5  docker-panel_0.7.14_amd64.deb
a734c36c565be6dcf6a2da79dec2acd7a047a5c2ef11e2afc2118c56dac03584  docker-panel
```

把 `checksums.txt` 与下载的资产放在同一目录后执行：

```bash
sha256sum -c checksums.txt
```

看到 `OK` 再安装。校验通过只能证明「下载内容与 Releases 页声明的一致」，**不能证明上游未被攻破**。

### 提权与安装

| 安装方式 | 行为 |
|----------|------|
| deb 自动安装 | 走 `pkexec dpkg -i`，依赖桌面环境提供 polkit 授权代理。授权框由系统弹出，应用不收集也不缓存你的密码 |
| 裸二进制 | 在安装目录可写时**原地原子替换**（tmp + rename），不额外提权 |
| 无法安装时 | 开发构建、非 Linux 平台、无匹配资产、安装目录不可写、无可用 polkit——只提示并打开下载页，**不会半途安装** |

## 5. 不视为漏洞的设计边界

**结论：下面 7 种行为都是已记录的设计边界，报上来通常只会被指向文档。**

> [!CAUTION]
> **容器删除带 `force=true`，不可恢复。** 批量删除前必须自己核对选择范围——删除操作没有二次确认。

| 行为 | 说明 |
|------|------|
| 无 polkit 授权代理时 deb 自动安装失败 | 只提示并打开下载页，需手动 `sudo dpkg -i` |
| `DOCKER_HOST` 只识别 `unix://` | 不支持 tcp / ssh 远程 Docker，属当前设计范围 |
| 删除操作无二次确认 | 容器删除带 `force=true`，不可恢复；批量删除前需自行确认选择范围 |
| 单实例窗口前置在 GNOME Wayland 下不生效 | 合成器限制：激活令牌只能由焦点客户端创建，无 `XDG_ACTIVATION_TOKEN` 时无法抢焦点；应用只做到 `show()` + `request_redraw()` |
| 日志窗口最多保留 2000 行 | 「复制日志」复制的是窗口内容，不是容器完整历史 |
| 更新包无代码签名 | 见第 4 节「现有安全模型」 |
| 镜像行 ▶ 尚未接入 `docker run` | 功能未实现，不是权限绕过 |

## 6. 相关文档

**结论：排障步骤看 troubleshooting，清单设计看 update-manifest，许可义务看最后两篇。**

| 文档 | 内容 |
|------|------|
| [README.md](README.md) | 功能、安装、已知限制 |
| [docs/troubleshooting.md](docs/troubleshooting.md) | 更新失败 / 校验失败等排障步骤 |
| [docs/arch/update-manifest.md](docs/arch/update-manifest.md) | 清单三级回退与校验的完整设计 |
| [LICENSING.md](LICENSING.md) | 自有 MIT 与 Slint 许可选择 |
| [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) | 第三方组件许可与署名 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | 贡献流程 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [支持与求助](SUPPORT.md) | [README 文档地图](README.md#文档地图) | [贡献指南](CONTRIBUTING.md) |
