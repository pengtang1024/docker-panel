#!/usr/bin/env bash
# 从 CHANGELOG.md 提取指定版本的段落，输出到 stdout。
#
# 用途：GitHub Release 的更新日志（release body）——发版页面要显示
#       CHANGELOG.md 里对应版本的完整段落，而不是 GitHub 自动生成的
#       "**Full Changelog**: <compare 链接>"（本仓库没有 PR，自动生成基本是空的）。
#
# 用法：
#   scripts/release-notes.sh 0.3.1            # 打印 ## [0.3.1] 段落
#   scripts/release-notes.sh 0.3.1 > notes.md # 写文件，供流水线 body_path 使用
#
# 退出码：0 = 取到段落；1 = CHANGELOG.md 里没有该版本段落（流水线据此报错）

set -euo pipefail

cd "$(dirname "$0")/.."

version="${1:-}"
if [ -z "$version" ]; then
  echo "用法: scripts/release-notes.sh X.Y.Z" >&2
  exit 1
fi

notes=$(awk -v v="$version" '
  index($0, "## [" v "]") == 1 { in_section = 1; print; next }
  in_section && /^## / { exit }
  in_section { print }
' CHANGELOG.md)

# 去掉段落尾部的空行，避免 body 结尾一堆换行
notes=$(printf '%s\n' "$notes" | sed -e :a -e '/^\n*$/{$d;N;ba' -e '}' )

if [ -z "$(printf '%s' "$notes" | tr -d '[:space:]')" ]; then
  echo "CHANGELOG.md 里没有 ## [$version] 段落" >&2
  exit 1
fi

printf '%s\n' "$notes"
