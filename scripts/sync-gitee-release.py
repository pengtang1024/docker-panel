#!/usr/bin/env python3
"""把一个已发布的 GitHub Release 同步成 Gitee Release（tag 必须先存在）。

为什么要有这个脚本：Gitee 侧没有 CI，发版只能手工补齐；漏一次就会出现
「GitHub 有新版本、Gitee 没有」，用户点 Gitee 的下载链接 404。发版流程见
docs/git-flow.md 的「双平台发布」一节。

用法：
    export GITEE_TOKEN=xxxxxxxx          # 必填，gitee 个人令牌（projects 权限）
    scripts/sync-gitee-release.py 0.7.15            # 同步该 tag
    scripts/sync-gitee-release.py 0.7.15 --dry-run  # 只打印计划，不发请求

环境变量：
    GITEE_TOKEN   必填
    GITEE_OWNER   默认 pengtang1024
    GITEE_REPO    默认 docker-panel
    GH_TOKEN      GitHub 读取用（可选；公开仓库匿名亦可，但容易触发限流）

行为：
    1. 读 GitHub Release 的 name / body / 资产清单；
    2. 解析 tag 指向的 commit（Gitee release 的 target_commitish 用它）；
    3. Gitee 上已有同名 release → 复用，只补缺失资产；没有 → 新建；
    4. 逐资产上传（本地已下载的优先，未下载的从 GitHub 资产直链取）；
    5. 回读 Gitee release，逐个核对资产名，缺一个就非零退出。

幂等：重复执行只会补缺失资产，不会重复创建 release。
"""

from __future__ import annotations

import json
import os
import ssl
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request

GH_API = "https://api.github.com/repos/{owner}/{repo}"
GT_API = "https://gitee.com/api/v5/repos/{owner}/{repo}"

OWNER = os.environ.get("GITEE_OWNER", "pengtang1024")
REPO = os.environ.get("GITEE_REPO", "docker-panel")
GITEE_TOKEN = os.environ.get("GITEE_TOKEN", "")
GH_TOKEN = os.environ.get("GH_TOKEN", "")

# 本机（以及不少国内环境）到 gitee.com / api.github.com 的 TLS 链不完整，
# 与仓库内既有脚本保持一致：显式不校验证书，只用于读公开数据与自家令牌调用。
CTX = ssl.create_default_context()
CTX.check_hostname = False
CTX.verify_mode = ssl.CERT_NONE


def die(msg: str) -> "NoReturn":  # type: ignore[name-defined]
    print(f"错误: {msg}", file=sys.stderr)
    raise SystemExit(1)


def http_get_json(url: str, token: str | None = None) -> dict:
    req = urllib.request.Request(url)
    if token:
        req.add_header("Authorization", f"token {token}")
    with urllib.request.urlopen(req, context=CTX, timeout=60) as resp:
        return json.loads(resp.read())


def gh_release(tag: str) -> dict:
    try:
        return http_get_json(f"{GH_API.format(owner=OWNER, repo=REPO)}/releases/tags/{tag}", GH_TOKEN)
    except urllib.error.HTTPError as e:
        if e.code == 404:
            die(f"GitHub 上没有 {tag} 的 Release（tag 未发布，或该 Release 已被删除）")
        die(f"读取 GitHub Release 失败（HTTP {e.code}）")


def gt_call(method: str, path: str, data: dict | None = None) -> tuple[int, str]:
    """调用 Gitee v5 API。返回 (HTTP 状态码, 响应体)。"""
    q = urllib.parse.urlencode({"access_token": GITEE_TOKEN})
    url = f"{GT_API.format(owner=OWNER, repo=REPO)}{path}?{q}"
    body = json.dumps(data).encode() if data is not None else None
    req = urllib.request.Request(url, data=body, method=method)
    if body is not None:
        req.add_header("Content-Type", "application/json;charset=UTF-8")
    try:
        with urllib.request.urlopen(req, context=CTX, timeout=60) as resp:
            return resp.status, resp.read().decode("utf-8", "replace")
    except urllib.error.HTTPError as e:  # type: ignore[attr-defined]
        return e.code, e.read().decode("utf-8", "replace")


def gitee_release(tag: str) -> dict | None:
    code, text = gt_call("GET", f"/releases/tags/{tag}")
    if code != 200:
        return None
    data = json.loads(text)
    # 不存在的 tag 也返回 200 + null，必须按内容判断
    return data if isinstance(data, dict) and data.get("id") else None


def tag_commit(tag: str) -> str:
    r = subprocess.run(["git", "rev-list", "-n1", tag], capture_output=True, text=True)
    sha = r.stdout.strip()
    if r.returncode != 0 or not sha:
        die(f"本地取不到 tag {tag} 指向的 commit（先 git fetch --tags）")
    return sha


def upload(release_id: int, path: str, name: str) -> bool:
    q = urllib.parse.urlencode({"access_token": GITEE_TOKEN})
    url = f"{GT_API.format(owner=OWNER, repo=REPO)}/releases/{release_id}/attach_files?{q}"
    with tempfile.NamedTemporaryFile("w", suffix=".json", delete=False) as f:
        out = f.name
    r = subprocess.run(
        ["curl", "-sS", "--max-time", "900", "-o", out, "-F", f"file=@{path}", url],
        capture_output=True, text=True,
    )
    body = open(out, encoding="utf-8", errors="replace").read()
    os.unlink(out)
    if r.returncode != 0:
        print(f"  上传失败 {name}: curl 退出 {r.returncode} {r.stderr.strip()[:120]}")
        return False
    # Gitee 成功时返回资产 JSON（含 id / name），失败时返回 message；没有可用的状态码字段
    if '"id"' in body:
        return True
    print(f"  上传失败 {name}: {body[:160]}")
    return False


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    dry = "--dry-run" in sys.argv
    if len(args) != 1:
        print(__doc__.strip().split("\n\n")[0], file=sys.stderr)
        die("需要一个 tag，例如 scripts/sync-gitee-release.py 0.7.15")
    tag = args[0]
    if not GITEE_TOKEN:
        die("缺少 GITEE_TOKEN 环境变量（gitee 个人令牌，projects 权限）")

    gh = gh_release(tag)
    assets = {a["name"]: a["browser_download_url"] for a in gh["assets"]}
    commit = tag_commit(tag)
    print(f"[{tag}] GitHub 资产 {len(assets)} 个 / commit {commit}")

    existing = gitee_release(tag)
    if existing:
        have = {a["name"] for a in existing.get("assets", [])}
        release_id = existing["id"]
        print(f"[{tag}] Gitee 已有 release id={release_id}，现有 {len(have)} 个资产")
    else:
        have = set()
        print(f"[{tag}] Gitee 无 release，将新建")
        if dry:
            print(f"  (dry-run) 会创建 release 并上传 {len(assets)} 个资产")
            return 0
        code, text = gt_call("POST", "/releases", {
            "tag_name": tag,
            "name": gh["name"],
            "body": gh.get("body") or "",
            "target_commitish": commit,
        })
        if code not in (200, 201):
            die(f"创建 Gitee release 失败（{code}）：{text[:300]}")
        release_id = json.loads(text)["id"]
        print(f"[{tag}] 已创建 release id={release_id}")

    missing = [n for n in assets if n not in have]
    if not missing:
        print(f"[{tag}] 资产已齐全，无需上传")
        return 0
    print(f"[{tag}] 待上传 {len(missing)} 个：{' '.join(sorted(missing))}")
    if dry:
        return 0

    with tempfile.TemporaryDirectory() as tmp:
        failures = []
        for name in sorted(missing):
            path = os.path.join(tmp, name)
            print(f"  下载 {name} …", flush=True)
            dl = subprocess.run(
                ["curl", "-sSL", "--max-time", "900", "-o", path, assets[name]],
                capture_output=True, text=True,
            )
            if dl.returncode != 0 or not os.path.exists(path) or os.path.getsize(path) == 0:
                failures.append(name)
                print(f"  下载失败 {name}: {dl.stderr.strip()[:120]}")
                continue
            size = os.path.getsize(path)
            if upload(release_id, path, name):
                print(f"  已上传 {name} ({size} B)")
            else:
                failures.append(name)

    final = gitee_release(tag) or {}
    final_names = {a["name"] for a in final.get("assets", [])}
    still_missing = [n for n in assets if n not in final_names]
    print(f"[{tag}] Gitee 现有 {len(final_names)} 个资产，失败={len(failures)}，缺失={still_missing}")
    if failures or still_missing:
        print("提示：Gitee 会额外附带 <tag>.tar.gz / <tag>.zip 两个源码包，属正常，不在核对范围内。",
              file=sys.stderr)
        return 1
    print(f"[{tag}] 双平台资产已对齐")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
