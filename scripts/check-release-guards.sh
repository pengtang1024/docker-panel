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

echo "== 1. 流水线：CI 只建草稿，签名与发布在维护者本机 =="
wf=.github/workflows/release.yml
signer=scripts/sign-release.sh
if grep -q 'draft: true' "$wf"; then ok "action-gh-release 使用 draft: true"; else bad "release.yml 里没有 draft: true（会被 immutable 拒绝上传）"; fi
# 一步到位写法：action 里同时出现 draft: false 与 files: → immutable 下必失败
if awk '/softprops\/action-gh-release/,/^      - name:/' "$wf" | grep -q 'draft: false'; then
  bad "action-gh-release 里出现 draft: false（不要一步到位发布）"
else
  ok "action-gh-release 未使用 draft: false"
fi
# 私钥不进 CI：流水线里不能出现签名私钥或 openssl 签名，签名必须在本机脚本里
if grep -qE 'UPDATE_SIGNING_KEY|pkeyutl -sign' "$wf"; then
  bad "release.yml 里出现签名私钥/签名动作（私钥不得进入 CI）"
else
  ok "release.yml 不持有签名私钥"
fi
if [ -f "$signer" ] && grep -q -- 'draft=false' "$signer"; then
  ok "发布动作在 scripts/sign-release.sh（离线签名后 draft=false）"
else
  bad "找不到 scripts/sign-release.sh 或它没有 draft=false 发布步骤"
fi
# 资产齐全断言：CI 校验 8 个构建产物，签名脚本校验补传后的 10 个
for f in docker-panel checksums.txt version.json LICENSE LICENSING.md THIRD-PARTY-NOTICES.md licenses.tar.gz; do
  grep -q -- "$f" "$wf" || bad "CI 资产清单缺少 $f"
done
grep -q 'docker-panel_\${tag}_amd64.deb' "$wf" && ok "CI 资产清单含 deb" || bad "CI 资产清单缺少 deb"
for f in version.json.sig checksums.txt.sig; do
  grep -q -- "$f" "$signer" || bad "sign-release.sh 没有补传 $f"
done

echo "== 2. 发版文档：双平台补齐与不可变处置必须写清 =="
grep -q 'sync-gitee-release' docs/git-flow.md && ok "git-flow 记录了 Gitee 同步步骤" || bad "docs/git-flow.md 缺 Gitee 同步步骤"
grep -q 'immutable' docs/git-flow.md && ok "git-flow 记录了不可变产物处置" || bad "docs/git-flow.md 缺不可变产物处置"
grep -q '破坏性操作' AGENTS.md && ok "AGENTS.md 有破坏性操作门禁" || bad "AGENTS.md 缺破坏性操作门禁"

# 新 tag 必须签名：配置与文档约定缺一不可（历史 tag 不改写、不重签，脚本不触碰任何 ref）
[ "$(git config --get gpg.format 2>/dev/null)" = ssh ] && ok "本地 git 使用 SSH 签名（gpg.format=ssh）" || bad "gpg.format 不是 ssh（应为 ssh）"
[ -n "$(git config --get user.signingKey 2>/dev/null)" ] && ok "本地 git 已设置 user.signingKey" || bad "本地 git 缺 user.signingKey"
[ "$(git config --get tag.gpgSign 2>/dev/null)" = true ] && ok "tag.gpgSign=true（新 tag 自动签名）" || bad "tag.gpgSign 不是 true（新 tag 可能无签名）"
[ -n "$(git config --get gpg.ssh.allowedSignersFile 2>/dev/null)" ] && ok "已设置 gpg.ssh.allowedSignersFile（git tag -v 可验签）" || bad "缺 gpg.ssh.allowedSignersFile（git tag -v 无法给出 Good signature）"
for kw in 'tag -s' 'Good signature' '历史 tag'; do
  grep -q -- "$kw" docs/git-flow.md || bad "docs/git-flow.md 缺签名 tag 约定关键词：$kw"
done
grep -q 'tag -s' CONTRIBUTING.md || bad "CONTRIBUTING.md 缺签名 tag 约定"

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
