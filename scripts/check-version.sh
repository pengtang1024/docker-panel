#!/usr/bin/env bash
# 版本三同步核对：Cargo.toml / packaging/debian/control / CHANGELOG.md
#
# 发版与 hotfix 收尾时跑一次，避免出现「deb 版本号与源码不一致」或「有 tag 没更新日志」。
#
# 用法：
#   scripts/check-version.sh            # 核对三处一致性（不要求已打 tag）
#   scripts/check-version.sh --tag     # 额外核对当前分支最新 tag 与三处一致
#
# 退出码：0 = 全部一致；1 = 存在不一致（逐条打印差异）

set -euo pipefail

cd "$(dirname "$0")/.."

fail=0

# ---- 读取三处版本 ----
cargo_ver=$(sed -n 's/^version = "\(.*\)"$/\1/p' Cargo.toml | head -1)
deb_ver=$(sed -n 's/^Version: \(.*\)$/\1/p' packaging/debian/control | head -1)
changelog_ver=$(grep -oE '^## \[[0-9]+\.[0-9]+\.[0-9]+\]' CHANGELOG.md \
  | head -1 | sed -E 's/^## \[([0-9.]+)\].*/\1/')

# CHANGELOG 里必须有该版本的段落与底部链接
has_section=$(grep -cE "^## \[$cargo_ver\]" CHANGELOG.md || true)
has_link=$(grep -cE "^\[$cargo_ver\]: " CHANGELOG.md || true)
has_unreleased_body=$(sed -n '/^## \[Unreleased\]/,/^## \[/p' CHANGELOG.md \
  | grep -cE '^(###|- )' || true)

echo "Cargo.toml            : $cargo_ver"
echo "packaging/debian/control: $deb_ver"
echo "CHANGELOG 最新版本段落 : ${changelog_ver:-<无>}"

if [ "$cargo_ver" != "$deb_ver" ]; then
  echo "✗ Cargo.toml($cargo_ver) 与 deb control($deb_ver) 不一致" >&2
  fail=1
fi

if [ "$cargo_ver" != "$changelog_ver" ]; then
  echo "✗ Cargo.toml($cargo_ver) 与 CHANGELOG 最新段落($changelog_ver) 不一致" >&2
  fail=1
fi

if [ "$has_section" -eq 0 ]; then
  echo "✗ CHANGELOG.md 缺少 ## [$cargo_ver] 版本段落（发版要把 [Unreleased] 改写过来）" >&2
  fail=1
fi

if [ "$has_link" -eq 0 ]; then
  echo "✗ CHANGELOG.md 底部缺少 [$cargo_ver] 链接行" >&2
  fail=1
fi

if [ "$has_unreleased_body" -gt 0 ]; then
  echo "✗ CHANGELOG.md 的 [Unreleased] 段仍有内容，发版时应改写为 ## [$cargo_ver]" >&2
  fail=1
fi

# ---- 可选：核对最新 tag ----
if [ "${1:-}" = "--tag" ]; then
  latest_tag=$(git tag --list '[0-9]*.[0-9]*.[0-9]*' --sort=-v:refname | head -1)
  echo "最新数字 tag          : ${latest_tag:-<无>}"
  if [ -z "$latest_tag" ]; then
    echo "✗ 仓库里没有任何数字版本 tag" >&2
    fail=1
  elif [ "$latest_tag" != "$cargo_ver" ]; then
    echo "✗ 最新 tag($latest_tag) 与版本号($cargo_ver) 不一致" >&2
    fail=1
  fi
fi

if [ "$fail" -ne 0 ]; then
  echo "版本同步核对失败" >&2
  exit 1
fi

echo "✓ 版本三同步一致"
