#!/usr/bin/env bash
# 离线签名并发布一个已经由 CI 建好草稿的 Release（0.8.0 起的发版第二步）。
#
# 为什么不放在 CI：签名私钥一旦进 secret，构建机/CI 供应链任何一环被攻破都能签出
# 「能通过校验」的更新包。现在流水线只产草稿（对用户不可见），签名在本机离线完成：
#
#   tag 推送 → CI 构建草稿（8 个资产） → 本脚本签名补传（2 个 .sig） → 发布
#            → 同步 master 的 version.json（jsDelivr 备源） → 刷 CDN
#
# 用法：
#   scripts/sign-release.sh 0.8.0
#   DOCKER_PANEL_SIGNING_KEY=/path/to/key.pem scripts/sign-release.sh 0.8.0
#
# 私钥查找顺序：$DOCKER_PANEL_SIGNING_KEY → ~/.config/docker-panel/signing/update-signing-key.pem
set -euo pipefail

usage() { sed -n '2,16p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//'; }

version="${1:-}"
case "$version" in
  ''|-h|--help) usage; exit 1 ;;
esac

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="pengtang1024/docker-panel"
key="${DOCKER_PANEL_SIGNING_KEY:-$HOME/.config/docker-panel/signing/update-signing-key.pem}"

command -v openssl >/dev/null || { echo "需要 openssl >= 3.0（pkeyutl -rawin）" >&2; exit 1; }
command -v gh >/dev/null || { echo "需要 gh CLI 并已登录（gh auth login）" >&2; exit 1; }
[ -f "$key" ] || { echo "找不到私钥：$key（或用 DOCKER_PANEL_SIGNING_KEY 指定）" >&2; exit 1; }

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

say() { printf '\n=== %s ===\n' "$*"; }

# --- 1. 定位草稿 ----------------------------------------------------------------
say "1/6 定位草稿 Release $version"
rel_id="$(gh api "repos/$repo/releases?per_page=100" \
  --jq ".[] | select(.tag_name == \"$version\" and .draft) | .id" | head -1)"
if [ -z "$rel_id" ]; then
  echo "找不到 tag $version 的草稿 Release。先让 CI 跑完（.github/workflows/release.yml）。" >&2
  exit 1
fi
echo "草稿 release id = $rel_id"

asset_id() { # $1 = 资产名
  gh api "repos/$repo/releases/$rel_id/assets" --jq ".[] | select(.name == \"$1\") | .id" | head -1
}

download_asset() { # $1 = 资产名, $2 = 输出文件名
  local id; id="$(asset_id "$1")"
  [ -n "$id" ] || { echo "草稿里没有资产 $1" >&2; exit 1; }
  gh api -H "Accept: application/octet-stream" "repos/$repo/releases/assets/$id" > "$2"
}

# --- 2. 取回待签文件 -------------------------------------------------------------
say "2/6 下载 version.json 与 checksums.txt"
download_asset version.json "$work/version.json"
download_asset checksums.txt "$work/checksums.txt"
python3 -m json.tool "$work/version.json" > /dev/null
grep -q "\"version\": \"$version\"" "$work/version.json" \
  || { echo "草稿里的清单版本不是 $version" >&2; exit 1; }

# --- 3. 核对私钥 ----------------------------------------------------------------
say "3/6 核对私钥指纹"
openssl pkey -in "$key" -pubout -out "$work/pub.pem" 2>/dev/null
fingerprint="$(openssl pkey -pubin -in "$work/pub.pem" -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')"
pubkey_b64="$(openssl pkey -pubin -in "$work/pub.pem" -outform DER | tail -c 32 | base64 -w0)"
grep -qF "$pubkey_b64" "$repo_root/packaging/release-signing.pub" \
  || { echo "私钥指纹 $fingerprint 不在 packaging/release-signing.pub 里，拒绝签名" >&2; exit 1; }
echo "公钥指纹 $fingerprint（base64 $pubkey_b64）"

# --- 4. 签名 -------------------------------------------------------------------
say "4/6 签名"
for name in version.json checksums.txt; do
  openssl pkeyutl -sign -inkey "$key" -rawin -in "$work/$name" -out "$work/$name.sig"
  size="$(stat -c%s "$work/$name.sig")"
  [ "$size" = "64" ] || { echo "$name.sig 长度 $size（期望 64）" >&2; exit 1; }
  openssl pkeyutl -verify -pubin -inkey "$work/pub.pem" -rawin \
    -in "$work/$name" -sigfile "$work/$name.sig" > /dev/null
  echo "已签名 $name.sig（64 字节，sha256 $(sha256sum "$work/$name" | awk '{print $1}')）"
done

# --- 5. 补传并发布 ---------------------------------------------------------------
say "5/6 补传签名并发布"
# 用 `gh release upload` 而不是裸 assets 接口：它对草稿 Release 的二进制上传
# 走 uploads 端点并自行处理重定向，裸接口在这个仓库上会 404。
for name in version.json.sig checksums.txt.sig; do
  gh release upload "$version" "$work/$name" --repo "$repo" --clobber > /dev/null
  echo "已上传 $name"
done

assets="$(gh api "repos/$repo/releases/$rel_id/assets" --jq '.[].name' | sort | tr '\n' ' ')"
count="$(printf '%s' "$assets" | wc -w)"
echo "草稿资产（${count}）：${assets}"
for f in docker-panel_${version}_amd64.deb docker-panel checksums.txt version.json \
         version.json.sig checksums.txt.sig LICENSE LICENSING.md THIRD-PARTY-NOTICES.md licenses.tar.gz; do
  case " $assets " in
    *" $f "*) ;;
    *) echo "缺少资产 $f，拒绝发布" >&2; exit 1 ;;
  esac
done

gh api -X PATCH "repos/$repo/releases/$rel_id" -F draft=false > /dev/null
read -r draft immutable <<<"$(gh api "repos/$repo/releases/tags/$version" \
  --jq '[.draft, .immutable] | @tsv' | tr '\t' ' ')"
echo "已发布 $version：draft=$draft immutable=$immutable"
[ "$draft" = "false" ] || { echo "Release $version 仍是 draft" >&2; exit 1; }

# --- 6. 同步 master（jsDelivr 备源）并刷 CDN --------------------------------------
say "6/6 同步 master 清单并刷新 jsDelivr"
put_file() { # $1 = 工作区路径, $2 = 仓库路径
  local path="$2" sha b64 payload
  sha="$(gh api "repos/$repo/contents/$path?ref=master" --jq .sha 2>/dev/null || true)"
  b64="$(base64 -w0 "$1")"
  payload="$(python3 - "$path" "$b64" "$sha" "$version" <<'PY'
import json, sys
path, content, sha, version = sys.argv[1:5]
body = {
    "message": f"chore(update): 同步 {version} 更新清单与签名到 master [skip ci]",
    "content": content,
    "branch": "master",
}
if sha:
    body["sha"] = sha
print(json.dumps(body))
PY
)"
  printf '%s' "$payload" | gh api -X PUT "repos/$repo/contents/$path" --input - > /dev/null
  echo "已提交 $path 到 master"
}
put_file "$work/version.json" version.json
put_file "$work/version.json.sig" version.json.sig

for target in version.json version.json.sig; do
  curl -fsS --max-time 20 "https://purge.jsdelivr.net/gh/pengtang1024/docker-panel@master/$target" > /dev/null 2>&1 || true
done
for attempt in 1 2 3; do
  body="$(curl -fsS --max-time 20 "https://cdn.jsdelivr.net/gh/pengtang1024/docker-panel@master/version.json" 2>/dev/null || true)"
  if printf '%s' "$body" | grep -q "\"version\": \"$version\""; then
    echo "jsDelivr 已返回 $version"
    break
  fi
  [ "$attempt" = "3" ] && echo "::warning::jsDelivr 仍返回旧清单（CDN 传播延迟），主源不受影响"
  sleep 5
done

cat <<EOF

✅ $version 已发布并签名完成。
后续：
  scripts/sync-gitee-release.py $version   # Gitee 侧资产对齐
  scripts/verify-release.sh $version       # 独立复核签名与资产
  scripts/check-release-guards.sh          # 收尾护栏
EOF
