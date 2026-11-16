#!/usr/bin/env bash
# 独立复核一个已发布 Release 的签名与资产（维护者与用户都能跑）。
#
# 校验链路与 App 内一致：内置公钥 → version.json.sig 验签 version.json
# → 清单里声明的 deb / 二进制 SHA-256。另外复核 checksums.txt.sig，
# 让「手工下载后核对」也有可验证的锚点。
#
# 用法：
#   scripts/verify-release.sh 0.8.0
#   scripts/verify-release.sh 0.8.0 --pubkey ~/pub.pem   # 只用公钥复核（无需私钥）
#
# 公钥来源：--pubkey 指定，或从本地私钥派生（默认同 sign-release.sh 的位置）。
set -euo pipefail

version="${1:-}"
[ -n "$version" ] || { echo "用法: scripts/verify-release.sh <version> [--pubkey <pem>]" >&2; exit 1; }
shift || true

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
repo="pengtang1024/docker-panel"
pubkey=""
if [ "${1:-}" = "--pubkey" ] && [ -n "${2:-}" ]; then pubkey="$2"; fi

work="$(mktemp -d)"
trap 'rm -rf "$work"' EXIT

if [ -z "$pubkey" ]; then
  key="${DOCKER_PANEL_SIGNING_KEY:-$HOME/.config/docker-panel/signing/update-signing-key.pem}"
  [ -f "$key" ] || {
    echo "没有公钥：用 --pubkey 指定公钥，或把私钥放在 $key" >&2
    exit 1
  }
  openssl pkey -in "$key" -pubout -out "$work/pub.pem" 2>/dev/null
  pubkey="$work/pub.pem"
fi

base="https://github.com/$repo/releases/download/$version"
say() { printf '\n=== %s ===\n' "$*"; }

say "1/4 下载清单与签名"
for f in version.json version.json.sig checksums.txt checksums.txt.sig; do
  curl -fsSL --max-time 30 "$base/$f" -o "$work/$f" \
    || { echo "下载失败：$base/$f" >&2; exit 1; }
done

say "2/4 验签"
openssl pkey -pubin -in "$pubkey" -outform DER | tail -c 32 | od -An -tx1 | tr -d ' \n' > "$work/fingerprint"
fingerprint="$(cat "$work/fingerprint")"
grep -q "$fingerprint" "$repo_root/src/signature.rs" \
  || { echo "公钥指纹 $fingerprint 不在 src/signature.rs 的可信列表里" >&2; exit 1; }
echo "公钥指纹 $fingerprint（在 src/signature.rs 中）"

for name in version.json checksums.txt; do
  openssl pkeyutl -verify -pubin -inkey "$pubkey" -rawin \
    -in "$work/$name" -sigfile "$work/$name.sig" > /dev/null
  echo "✅ $name 签名有效（sha256 $(sha256sum "$work/$name" | awk '{print $1}')）"
done

say "3/4 核对清单与 checksums.txt 的哈希"
python3 - "$work/version.json" "$work/checksums.txt" <<'PY'
import json, re, sys
manifest = json.load(open(sys.argv[1]))
sums = {}
for line in open(sys.argv[2]):
    parts = line.split()
    if len(parts) == 2:
        sums[parts[1].lstrip('*')] = parts[0]
problems = []
for kind in ("deb", "binary"):
    asset = manifest["assets"][kind]
    name, sha = asset["name"], asset["sha256"]
    if name not in sums:
        problems.append(f"checksums.txt 缺 {name}")
    elif sums[name] != sha:
        problems.append(f"{name} 哈希不一致：清单 {sha} / checksums {sums[name]}")
if problems:
    print("\n".join(problems)); sys.exit(1)
print(f"✅ 清单版本 {manifest['version']}，deb 与二进制哈希与 checksums.txt 一致")
PY

say "4/4 核对已发布资产"
gh api "repos/$repo/releases/tags/$version" \
  --jq '"draft=\(.draft) immutable=\(.immutable) assets=\(.assets | length)"'
gh api "repos/$repo/releases/tags/$version" --jq '.assets[].name' | sort | tr '\n' ' '
echo
