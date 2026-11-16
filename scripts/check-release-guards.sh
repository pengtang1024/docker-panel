#!/usr/bin/env bash
# 核对发布护栏是否还在——这些护栏一旦被悄悄改掉，就会重演 2026-11-16 的
# 「删 release → tag 名永久不可重建」事故。
#
# 用法：
#   scripts/check-release-guards.sh                 # 只检查仓库内文件（离线可用）
#   GH_TOKEN=xxx scripts/check-release-guards.sh    # 额外核对 GitHub 侧设置
#
# 退出码：0 全绿；1 有护栏缺失。

set -uo pipefail

cd "$(dirname "$0")/.."

fail=0
ok()   { printf '  ✓ %s\n' "$1"; }
bad()  { printf '  ✗ %s\n' "$1"; fail=1; }

echo "== 1. 流水线：发布顺序必须是 draft → 上传 → 发布 =="
wf=.github/workflows/release.yml
if grep -q 'draft: true' "$wf"; then ok "action-gh-release 使用 draft: true"; else bad "release.yml 里没有 draft: true（会被 immutable 拒绝上传）"; fi
if grep -q 'gh api -X PATCH .*releases/\${rel_id}" -F draft=false' "$wf"; then
  ok "发布走 gh api -X PATCH draft=false"
else
  bad "找不到「校验资产后再 draft=false」的发布步骤"
fi
# 一步到位写法：action 里同时出现 draft: false 与 files: → immutable 下必失败
if awk '/softprops\/action-gh-release/,/^      - name:/' "$wf" | grep -q 'draft: false'; then
  bad "action-gh-release 里出现 draft: false（不要一步到位发布）"
else
  ok "action-gh-release 未使用 draft: false"
fi
for f in docker-panel checksums.txt version.json LICENSE LICENSING.md THIRD-PARTY-NOTICES.md licenses.tar.gz; do
  grep -q -- "$f" "$wf" || bad "发布资产清单缺少 $f"
done
grep -q 'docker-panel_\${tag}_amd64.deb' "$wf" && ok "发布资产清单含 deb" || bad "发布资产清单缺少 deb"

echo "== 2. 发版文档：双平台补齐与不可变处置必须写清 =="
grep -q 'sync-gitee-release' docs/git-flow.md && ok "git-flow 记录了 Gitee 同步步骤" || bad "docs/git-flow.md 缺 Gitee 同步步骤"
grep -q 'immutable' docs/git-flow.md && ok "git-flow 记录了不可变产物处置" || bad "docs/git-flow.md 缺不可变产物处置"
grep -q '破坏性操作' AGENTS.md && ok "AGENTS.md 有破坏性操作门禁" || bad "AGENTS.md 缺破坏性操作门禁"

echo "== 3. GitHub 侧设置（需要 GH_TOKEN）=="
repo="${GUARD_REPO:-pengtang1024/docker-panel}"
if [ -n "${GH_TOKEN:-}" ] && command -v gh >/dev/null 2>&1; then
  imm="$(gh api "repos/$repo/immutable-releases" --jq '.enabled' 2>/dev/null || echo unknown)"
  [ "$imm" = "true" ] && ok "immutable releases 已开启" || bad "immutable releases 未开启（当前：$imm）"
  names="$(gh api "repos/$repo/rulesets" --jq '.[].name' 2>/dev/null | tr '\n' ' ' || echo unknown)"
  case "$names" in *protect-release-tags*) ok "ruleset protect-release-tags 存在";; *) bad "缺 ruleset protect-release-tags";; esac
  case "$names" in *protect-main-branches*) ok "ruleset protect-main-branches 存在";; *) bad "缺 ruleset protect-main-branches";; esac
else
  echo "  - 跳过（未设置 GH_TOKEN 或没有 gh CLI）"
fi

if [ "$fail" -eq 0 ]; then
  echo "护栏完整"
else
  echo "护栏有缺失，按上面的 ✗ 逐项修复" >&2
fi
exit "$fail"
