# 安全策略

docker-panel 是一个直接与本机 Docker Engine 守护进程通信的桌面应用（通过 `/var/run/docker.sock`），因此**运行它的账户实际上等同于拥有 root 等价权限**。理解这一点是理解本项目安全模型的前提。

## 报告漏洞

**请勿在公开 Issue、讨论区或社交平台上披露安全问题。**

使用 GitHub 的私有漏洞报告通道：

> <https://github.com/pengtang1024/docker-panel/security/advisories/new>

该通道只在维护者与你之间可见，适合附带复现步骤、PoC 或敏感日志。报告时请尽量包含：

- 受影响的版本（`docker-panel --version`）
- 发行版与显示会话（本项目仅验证 Wayland）
- 复现步骤与预期 / 实际行为
- 是否需要特定 `DOCKER_HOST` 或配置组合

## 支持范围

| 范围 | 状态 |
|------|------|
| 最新 Release（当前 v0.7.x） | 接受报告并修复 |
| 更早的 0.7.x | 视严重程度决定是否回补 |
| 0.4.3 之前 | **不再支持**——该批 Release 与 tag 已因第三方许可文件与 Slint 署名缺失全部下架 |

## 响应预期

维护者会评估每一份报告并回复结论（确认 / 需要更多信息 / 判定为设计边界）。本项目由个人维护，**不承诺固定的响应时限或修复 SLA**；请预留合理等待时间，不要因为未即时回复就在公开渠道披露细节。

## 现有安全模型（如实告知）

这一节描述的是**已知边界，不是待修漏洞**。发现下列行为符合预期时无需报告。

### 更新包没有代码签名

自动更新只依赖两层保护：

1. **HTTPS** 传输（含静态清单与 Release 资产）
2. **同一 Release 内的 `checksums.txt`（SHA-256）** 校验下载产物

没有 GPG / minisign 签名，也没有独立的公钥信任链。**如果上游 GitHub Release 本身被篡改，客户端无法防御**——这是当前设计接受的已知风险，不是遗漏。

### 更新发现的三级回退与 fail closed

应用按顺序尝试三个来源获取版本清单：

1. `https://github.com/pengtang1024/docker-panel/releases/latest/download/version.json`
2. `https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json`
3. `https://api.github.com/repos/pengtang1024/docker-panel/releases/latest`

关键约束：

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

- deb 自动安装走 `pkexec dpkg -i`，依赖桌面环境提供 polkit 授权代理。授权框由系统弹出，应用不收集也不缓存你的密码。
- 裸二进制在安装目录可写时**原地原子替换**（tmp + rename），不额外提权。
- 开发构建、非 Linux 平台、无匹配资产、安装目录不可写、无可用 polkit 时，应用只提示并打开下载页，**不会半途安装**。

## 不视为漏洞的设计边界

| 行为 | 说明 |
|------|------|
| 无 polkit 授权代理时 deb 自动安装失败 | 只提示并打开下载页，需手动 `sudo dpkg -i` |
| `DOCKER_HOST` 只识别 `unix://` | 不支持 tcp / ssh 远程 Docker，属当前设计范围 |
| 删除操作无二次确认 | 容器删除带 `force=true`，不可恢复；批量删除前需自行确认选择范围 |
| 单实例窗口前置在 GNOME Wayland 下不生效 | 合成器限制：激活令牌只能由焦点客户端创建，无 `XDG_ACTIVATION_TOKEN` 时无法抢焦点；应用只做到 `show()` + `request_redraw()` |
| 日志窗口最多保留 2000 行 | 「复制日志」复制的是窗口内容，不是容器完整历史 |
| 更新包无代码签名 | 见上文「现有安全模型」 |
| 镜像行 ▶ 尚未接入 `docker run` | 功能未实现，不是权限绕过 |

## 相关文档

- [README.md](README.md) — 功能、安装、已知限制
- [docs/troubleshooting.md](docs/troubleshooting.md) — 更新失败 / 校验失败等排障步骤
- [docs/arch/update-manifest.md](docs/arch/update-manifest.md) — 清单三级回退与校验的完整设计
- [LICENSING.md](LICENSING.md) · [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md) — 许可与第三方组件
- [CONTRIBUTING.md](CONTRIBUTING.md) — 贡献流程
