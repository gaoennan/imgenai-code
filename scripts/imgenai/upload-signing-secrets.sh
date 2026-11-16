#!/usr/bin/env bash
# 把 macOS 签名与公证凭据存进 GitHub 仓库 Secrets（在你自己的 Mac 上运行一次）。
#
# 需要的文件都放在签名目录（默认 ~/Library/Application Support/imgenai-code-signing，不会被清缓存也不进 iCloud）：
#   developer-id.key   签名私钥（生成 CSR 时一起生成的）
#   *.cer              Apple 开发者网站用 CSR 签发的「Developer ID Application」证书
#   AuthKey_*.p8       App Store Connect API 密钥（公证用）
# 运行时会问 API 密钥的 Key ID 与 Issuer ID。凭据只在本机处理，直接经 gh 上传，不打印出来。
#
# 用法：bash scripts/imgenai/upload-signing-secrets.sh [owner/repo]
set -euo pipefail

REPO="${1:-gaoennan/imgenai-code}"
DIR="${IMGENAI_SIGNING_DIR:-$HOME/Library/Application Support/imgenai-code-signing}"

gh auth status >/dev/null 2>&1 || { echo "先运行 gh auth login 登录 GitHub" >&2; exit 1; }
[ -f "$DIR/developer-id.key" ] || { echo "找不到私钥 $DIR/developer-id.key" >&2; exit 1; }
CER="$(ls -t "$DIR"/*.cer 2>/dev/null | head -1 || true)"
[ -n "$CER" ] || { echo "找不到证书：把 Apple 开发者网站下载的 .cer 放进 $DIR" >&2; exit 1; }
# 浏览器下载的 API 密钥默认在「下载」文件夹：挪进签名目录（仅本人可读），下载文件夹里不留副本。
for found in "$HOME/Downloads"/AuthKey_*.p8; do
  [ -f "$found" ] || continue
  mv "$found" "$DIR/" && chmod 600 "$DIR/$(basename "$found")"
done
P8="$(ls -t "$DIR"/AuthKey_*.p8 2>/dev/null | head -1 || true)"
[ -n "$P8" ] || { echo "找不到公证用的 API 密钥：在 App Store Connect 下载 AuthKey_*.p8（会存到「下载」文件夹）" >&2; exit 1; }

TMP="$(mktemp -d)"
trap 'rm -rf "${TMP:?}"' EXIT

# 证书是 DER 格式；先转成 PEM，确认它和私钥是一对，再取证书全名作签名身份。
openssl x509 -inform DER -in "$CER" -out "$TMP/cert.pem" 2>/dev/null || cp "$CER" "$TMP/cert.pem"
CERT_MOD="$(openssl x509 -noout -modulus -in "$TMP/cert.pem" | openssl sha256)"
KEY_MOD="$(openssl rsa -noout -modulus -in "$DIR/developer-id.key" 2>/dev/null | openssl sha256)"
[ "$CERT_MOD" = "$KEY_MOD" ] || { echo "证书和私钥不是一对：请用 $DIR/developer-id.csr 重新申请证书" >&2; exit 1; }
IDENTITY="$(openssl x509 -noout -subject -nameopt multiline -in "$TMP/cert.pem" | sed -n 's/^ *commonName *= *//p')"
case "$IDENTITY" in
  "Developer ID Application:"*) ;;
  *) echo "这张证书不是 Developer ID Application（是：${IDENTITY}）" >&2; exit 1 ;;
esac

# 合成 .p12：密码随机生成；用旧式加密算法，macOS 的 security import 才认得（OpenSSL 3 默认算法它不认）。
P12_PASSWORD="$(openssl rand -hex 24)"
LEGACY=""
openssl pkcs12 -help 2>&1 | grep -q -- "-legacy" && LEGACY="-legacy"
openssl pkcs12 -export $LEGACY -inkey "$DIR/developer-id.key" -in "$TMP/cert.pem" -out "$TMP/cert.p12" \
  -passout "pass:$P12_PASSWORD" -name "$IDENTITY" -keypbe PBE-SHA1-3DES -certpbe PBE-SHA1-3DES -macalg sha1

# Key ID / Issuer ID 不是机密，可经环境变量传入；没传时从文件名取 Key ID、再问 Issuer ID。
KEY_ID="${IMGENAI_ASC_KEY_ID:-$(basename "$P8" .p8 | sed 's/^AuthKey_//')}"
ISSUER_ID="${IMGENAI_ASC_ISSUER_ID:-}"
[ -n "$ISSUER_ID" ] || read -r -p "Issuer ID：" ISSUER_ID
[ -n "$KEY_ID" ] && [ -n "$ISSUER_ID" ] || { echo "Key ID 和 Issuer ID 都要填" >&2; exit 1; }

base64 -i "$TMP/cert.p12" | gh secret set MACOS_CERT_P12 --repo "$REPO"
printf '%s' "$P12_PASSWORD" | gh secret set MACOS_CERT_PASSWORD --repo "$REPO"
printf '%s' "$IDENTITY" | gh secret set MACOS_SIGNING_IDENTITY --repo "$REPO"
base64 -i "$P8" | gh secret set APPLE_API_KEY_P8 --repo "$REPO"
printf '%s' "$KEY_ID" | gh secret set APPLE_API_KEY_ID --repo "$REPO"
printf '%s' "$ISSUER_ID" | gh secret set APPLE_API_ISSUER_ID --repo "$REPO"

echo "已存入 ${REPO}：签名身份「${IDENTITY}」与公证密钥。下次推 imgenai-v* 标签，macOS 包就会正式签名并公证。"
gh secret list --repo "$REPO"
