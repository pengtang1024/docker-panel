#!/usr/bin/env python3
"""文档版面自检：按 docs/style-guide.md §10 检查受控 Markdown。

用法：
    python3 scripts/check-docs.py            # 检查全部受控文档
    python3 scripts/check-docs.py README.md  # 只检查指定文件

检查项（任一不通过即非 0 退出）：
  1. ``` 栅栏数量为偶数（代码块必须闭合）
  2. <details> 与 </details> 数量相等
  3. 页内锚点 [](#x) 能在本文件标题里找到对应 slug
  4. 相对链接目标文件存在
  5. 禁用写法：[[Page|Label]]、内联 style="、裸 raw.githubusercontent 图片
  6. 遗留占位符：TODO / FIXME / 待补
  7. R2：去除代码块与 HTML 注释后正文 ≤ 300 行
  8. 页头三件套：首个 `> **一句话**` 存在；`##` 章节编号连续
"""

from __future__ import annotations

import os
import re
import sys

# 受控文档：对外可见、需要遵守排版规范的文件
DOCS = [
    "README.md",
    "CONTRIBUTING.md",
    "SECURITY.md",
    "SUPPORT.md",
    "CODE_OF_CONDUCT.md",
    "AGENTS.md",
    "docs/architecture.md",
    "docs/agents-rules.md",
    "docs/git-flow.md",
    "docs/style-guide.md",
    "docs/configuration.md",
    "docs/troubleshooting.md",
    "docs/translations.md",
    "docs/roadmap.md",
    "docs/arch/threading.md",
    "docs/arch/ui-interaction.md",
    "docs/arch/list-layout.md",
    "docs/arch/docker-bollard.md",
    "docs/arch/update-manifest.md",
]

BODY_MAX = 300

FENCE = re.compile(r"^```", re.M)
HEADING = re.compile(r"^(#{1,4})\s+(.+)$", re.M)
NUM_HEADING = re.compile(r"^## (\d+)\.\s+", re.M)


def slug(text: str) -> str:
    """GitHub / Gitee 一致的标题锚点：小写、去标点、**每个空格各自**转连字符、保留中文。

    注意不能把连续空格压缩成一个连字符：两站都实测把「列表不刷新 / 数据不更新」
    渲染成 `#4-列表不刷新--数据不更新`（斜杠被删、左右空格各留一个连字符）。
    """
    text = text.strip().lower()
    text = re.sub(r"[^\w\s\u4e00-\u9fff-]", "", text)
    return text.replace(" ", "-")


def check(path: str) -> list[str]:
    errs: list[str] = []
    raw = open(path, encoding="utf-8").read()
    # 去掉代码块后的文本，用于标题/链接/正文统计
    nocode = re.sub(r"```.*?```", "", raw, flags=re.S)

    fences = len(FENCE.findall(raw))
    if fences % 2:
        errs.append(f"代码块栅栏数为奇数（{fences}）——有未闭合的 ```")

    # style-guide 用内联代码示范折叠语法，示例本身不含闭合标签，跳过配对检查
    if path != "docs/style-guide.md":
        open_details = nocode.count("<details>")
        close_details = nocode.count("</details>")
        if open_details != close_details:
            errs.append(f"<details> 与 </details> 数量不等（{open_details} / {close_details}）")

    heads = [slug(m.group(2)) for m in HEADING.finditer(nocode)]
    anchors = re.findall(r"\]\(#([^)]+)\)", nocode)
    for anchor in anchors:
        if anchor not in heads:
            errs.append(f"页内锚点 #{anchor} 没有对应标题")

    rel = re.findall(r"\]\((?!https?://|#|mailto:)([^)#\s]+)", nocode)
    base = os.path.dirname(path)
    for target in rel:
        if target.startswith("assets/") or target.endswith(".png"):
            continue
        if not os.path.exists(os.path.join(base, target)):
            # 排掉行内代码里的语法示例：形如「相对路径」这种不含扩展名的占位
            if "." in os.path.basename(target) or target.startswith("docs/"):
                errs.append(f"相对链接目标不存在：{target}")

    if "[[" in nocode and "[[Page|Label]]" not in nocode and "[[…]]" not in nocode:
        errs.append("出现 [[ 语法（Gitee 不支持，会渲染成纯文本）")
    if 'style="' in nocode:
        errs.append("出现内联 style=（会被 Gitee 剥掉）")

    # 只拦「把 raw.githubusercontent 当图片源」；规则文本与实测端点表里提到域名是合法的
    raw_img = re.search(
        r"!\[[^\]]*\]\(https?://raw\.githubusercontent\.com"
        r"|<img[^>]+src=[\"']https?://raw\.githubusercontent\.com",
        nocode,
    )
    if raw_img:
        errs.append("图片走 raw.githubusercontent.com（国内常不可达，应改 jsDelivr）")

    # R0 允许 AGENTS.md 与规则配置层出现旧名（规则声明文本），其余文件零容忍
    if "dvkit" in nocode and path not in ("AGENTS.md", "docs/agents-rules.md"):
        errs.append("出现废弃旧名（R0 禁止）")

    # 占位符只查正文；style-guide 里出现的是「被禁写法」的示例，不算
    if path != "docs/style-guide.md":
        for ph in ("TODO", "FIXME", "待补"):
            if re.search(rf"\b{ph}\b", nocode):
                errs.append(f"遗留占位符 {ph}")

    body = re.sub(r"<!--.*?-->", "", nocode, flags=re.S)
    body_lines = sum(1 for line in body.splitlines() if line.strip())
    if body_lines > BODY_MAX:
        errs.append(f"正文 {body_lines} 行 > R2 上限 {BODY_MAX}")

    if path not in ("CODE_OF_CONDUCT.md", "docs/style-guide.md"):
        # 侧边栏是导航片段，不是独立页面，不要求页头三件套
        if "> **一句话**" not in raw and not path.endswith("_Sidebar.md"):
            errs.append("缺少页头「> **一句话**」")

    nums = [int(n) for n in NUM_HEADING.findall(nocode)]
    if nums and nums != list(range(nums[0], nums[0] + len(nums))):
        errs.append(f"`## N.` 章节编号不连续：{nums}")

    return errs


def main() -> int:
    targets = sys.argv[1:] or DOCS
    failed = 0
    for path in targets:
        if not os.path.exists(path):
            print(f"✗ {path}: 文件不存在")
            failed += 1
            continue
        errs = check(path)
        if errs:
            failed += 1
            print(f"✗ {path}")
            for e in errs:
                print(f"    - {e}")
        else:
            print(f"✓ {path}")
    print()
    print(f"{'全部通过' if failed == 0 else f'{failed} 个文件未通过'}（共 {len(targets)} 个）")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
