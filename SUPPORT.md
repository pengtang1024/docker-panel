# 获取帮助

> **一句话**：给「装不上、连不上、用不对、想提 issue」的用户看的一页——先自查哪几页、去哪儿提问、提问时要带什么信息。

**相关页面**：[故障排查](docs/troubleshooting.md)、[配置手册](docs/configuration.md)、[安全策略](SECURITY.md)｜**适用范围**：最终用户（本页不涉及源码）

## 1. 自助优先

**结论：下面 8 类问题的答案基本都在同一批文档里，先查表通常比等回复快一轮。**

> [!TIP]
> 提问前按表过一遍，多数问题能当场解决；确实要提问时，第 3 节的信息带齐就能少一轮往返。

| 症状类型 | 先看这里 |
|----------|---------|
| 装不上、连不上 Docker、顶栏「未连接」 | [docs/troubleshooting.md](docs/troubleshooting.md) · Wiki [安装与构建](https://github.com/pengtang1024/docker-panel/wiki/Install) |
| 界面操作、快捷键、日志抽屉、批量操作 | Wiki [使用指南](https://github.com/pengtang1024/docker-panel/wiki/Usage) |
| 想改配置 / 关掉自动更新 / 重置界面状态 | [docs/configuration.md](docs/configuration.md) |
| 更新失败、校验失败、pkexec 没反应 | [docs/troubleshooting.md](docs/troubleshooting.md) · Wiki [常见问题](https://github.com/pengtang1024/docker-panel/wiki/FAQ) |
| 中文字体、Wayland 显示、桌面集成 | Wiki [常见问题](https://github.com/pengtang1024/docker-panel/wiki/FAQ) |
| 版本改了什么 | [CHANGELOG.md](CHANGELOG.md) |
| 想改代码 / 提 PR | [CONTRIBUTING.md](CONTRIBUTING.md) |
| 想加翻译 | [docs/translations.md](docs/translations.md) |

两个站的 Wiki 内容完全一致，国内外访问速度不同可互为备份：

| 站点 | 地址 |
|------|------|
| GitHub Wiki | <https://github.com/pengtang1024/docker-panel/wiki> |
| Gitee Wiki | <https://gitee.com/pengtang1024/docker-panel/wikis> |

## 2. 提问渠道

**结论：缺陷、功能建议、使用疑问都走两个站的 Issue，安全漏洞必须走私用渠道。**

| 用途 | 渠道 |
|------|------|
| 缺陷报告 | [GitHub Issues](https://github.com/pengtang1024/docker-panel/issues/new/choose) · [Gitee Issues](https://gitee.com/pengtang1024/docker-panel/issues) |
| 功能建议 | 同上（GitHub 侧可直接选「功能建议」模板） |
| 使用疑问 | 同上（用 issue 提问即可，本项目没有单独的 Q&A 区） |
| 安全漏洞 | **不要发公开 Issue**，走 [SECURITY.md](SECURITY.md) 的私有渠道 |

> [!IMPORTANT]
> **目前没有**：邮件列表、Matrix / Discord / 微信群、论坛、商业支持或付费 SLA。除了上表的 Issue 入口与私有安全公告，没有其它官方渠道。

## 3. 提问前请附上这些信息

**结论：缺信息的问题无法定位，会被要求补充一轮——把下面五条命令输出和四项描述一起贴出来。**

```text
docker-panel --version     # 应用版本
docker version             # Docker Server 版本
cat /etc/os-release         # 发行版（至少给 ID 与 VERSION_ID）
echo $XDG_SESSION_TYPE      # 显示会话（本项目仅验证 Wayland）
echo $LANG                  # 语言环境（影响界面语言与字体回退）
```

| 必附内容 | 说明 |
|----------|------|
| 具体症状 | 报错原文、状态栏文案、顶栏连接状态（● 已连接 / ○ 未连接） |
| 复现步骤 | 从启动到出问题的完整操作序列 |
| 截图 | UI 问题尽量附截图 |
| 是否手改过配置 | 配置文件在 `$XDG_CONFIG_HOME/docker-panel/config.json`（未设 `XDG_CONFIG_HOME` 时为 `~/.config/docker-panel/config.json`）；贴内容前请自行确认其中没有敏感信息（该文件只存界面状态，不含凭据） |

## 4. 已知不受理的类型

**结论：下面 4 类属于已记录的设计边界，报告通常会被指向文档而不是修复。**

| 类型 | 为什么不受理 |
|------|--------------|
| 无 polkit 授权代理环境下 deb 自动安装失败 | 设计边界：只提示并打开下载页 |
| 远程 Docker（tcp / ssh）连接 | 设计边界：只识别 `unix://` 形式的 `DOCKER_HOST` |
| 非 Linux 平台上的构建与运行问题 | 项目仅在 Linux 上验证 |
| 单实例窗口在 GNOME Wayland 下不会自动置前 | 合成器限制，应用已做到 `show()` + `request_redraw()` |

完整列表见 [README.md](README.md)「已知限制」与 [SECURITY.md](SECURITY.md)「不视为漏洞的设计边界」。

## 5. 相关文档

**结论：除了本页，最常被需要的是排障、配置与贡献三篇。**

| 文档 | 内容 |
|------|------|
| [docs/troubleshooting.md](docs/troubleshooting.md) | 按症状排障（启动 / 连接 / 列表 / 更新 / 日志 / 安装卸载 / 数据安全） |
| [docs/configuration.md](docs/configuration.md) | config.json 字段全表与开关 |
| [CHANGELOG.md](CHANGELOG.md) | 每个版本改了什么 |
| [CONTRIBUTING.md](CONTRIBUTING.md) | 开发环境、门禁、PR 流程 |

---

| ◀ 上一页 | 文档地图 | 下一页 ▶ |
|----------|----------|----------|
| [行为准则](CODE_OF_CONDUCT.md) | [README 文档地图](README.md#文档地图) | [安全策略](SECURITY.md) |
