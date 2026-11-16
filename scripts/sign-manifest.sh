#!/usr/bin/env bash
# 给更新清单生成 detached Ed25519 签名（version.json.sig）——单文件版本。
#
# 正式发版走 `scripts/sign-release.sh <version>`（签名 + 补传 + 发布 + 同步 master）；
# 本脚本只做「对一个本地清单文件签名」，用于复核签名链、手工补签或排障。
#
# 用法：
#   scripts/sign-manifest.sh [version.json]
#   DOCKER_PANEL_SIGNING_KEY=/path/to/key.pem scripts/sign-manifest.sh /tmp/version.json
#
# 私钥查找顺序：$DOCKER_PANEL_SIGNING_KEY → ~/.config/docker-panel/signing/update-signing-key.pem
# 产出：与清单同目录的 version.json.sig（64 字节），并把公钥指纹打印出来
#       与 packaging/release-signing.pub（应用与启动器共用的公钥清单）核对。
set -euo pipefail

manifest="${1:-version.json}"
repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
key="${DOCKER_PANEL_SIGNING_KEY:-$HOME/.config/docker-panel/signing/update-signing-key.pem}"

[ -f "$manifest" ] || { echo "找不到清单：$manifest" >&2; exit 1; }
[ -f "$key" ] || { echo "找不到私钥：$key（或用 DOCKER_PANEL_SIGNING_KEY 指定）" >&2; exit 1; }
command -v openssl >/dev/null || { echo "需要 openssl >= 3.0（pkeyutl -rawin）" >&2; exit 1; }

sig="${manifest}.sig"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT

openssl pkey -in "$key" -pubout -out "$tmp/pub.pem" 2>/dev/null
fingerprint="$(openssl pkey -pubin -in "$tmp/pub.pem" -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n')"
pubkey_b64="$(openssl pkey -pubin -in "$tmp/pub.pem" -outform DER | tail -c 32 | base64 -w0)"
if ! grep -qF "$pubkey_b64" "$repo_root/packaging/release-signing.pub"; then
  echo "公钥指纹 $fingerprint 不在 packaging/release-signing.pub 里，拒绝签名" >&2
  exit 1
fi

openssl pkeyutl -sign -inkey "$key" -rawin -in "$manifest" -out "$sig"
size="$(stat -c%s "$sig")"
[ "$size" = "64" ] || { echo "签名长度 $size 字节（期望 64）" >&2; exit 1; }
openssl pkeyutl -verify -pubin -inkey "$tmp/pub.pem" -rawin -in "$manifest" -sigfile "$sig" >/dev/null

echo "已签名：$sig（$size 字节）"
echo "公钥指纹：$fingerprint"
echo "公钥（base64）：$pubkey_b64"
echo "清单 sha256：$(sha256sum "$manifest" | awk '{print $1}')"
