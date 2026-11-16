#!/usr/bin/env bash
# 打 imgenai Code 的 macOS 安装包（本机架构；未设签名身份时用 ad-hoc 签名）。
#
# 1. prepare-src.mjs：把当前工作区复制到构建目录的 src/，带上 ClaudeMom 依赖，改名为 imgenai Code；
# 2. 构建前端与 Rust，再用上游的 package-dmg.sh 组装 .app 与 DMG。
#
# 构建目录默认放在 ~/Library/Caches/imgenai-code-build/release：仓库在桌面上，桌面由 iCloud 同步，
# 好几个 GB 的编译产物不能放进去；iCloud 给文件加的 FinderInfo 等扩展属性还会让 codesign 失败。
#
# 环境变量：
#   IMGENAI_BUILD_DIR        构建目录（默认见上）
#   IMGENAI_VERSION          写进安装包的版本号 x.y.z（默认沿用源码里的版本号）
#   IMGENAI_UPDATE_REPO      自动更新读取的 GitHub 仓库 owner/name
#   MACOS_SIGNING_IDENTITY   Developer ID 签名身份；不设时 ad-hoc 签名，用户首次打开要手动放行
#   MACOS_NOTARY_PROFILE     公证凭据（notarytool keychain profile）
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BUILD="${IMGENAI_BUILD_DIR:-$HOME/Library/Caches/imgenai-code-build/release}"
SRC="$BUILD/src"

# sherpa-onnx 的构建脚本下载静态库时不支持 socks5 代理。
unset all_proxy ALL_PROXY
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
export MACOSX_DEPLOYMENT_TARGET="12.0"

node "$ROOT/scripts/imgenai/prepare-src.mjs" "$SRC" ${IMGENAI_VERSION:+--version "$IMGENAI_VERSION"}
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$SRC/Cargo.toml" | head -1)"
# 去掉从 iCloud 同步目录带过来的扩展属性（FinderInfo、fileprovider），否则 codesign 拒签。
xattr -cr "$SRC" 2>/dev/null || true

echo "==> 构建前端" >&2
(cd "$SRC/apps/codex-plus-manager" && npm ci --no-audit --no-fund >/dev/null && npm run vite:build >/dev/null)

echo "==> 构建 Rust（release）" >&2
(cd "$SRC" && CARGO_TARGET_DIR="$BUILD/target" cargo build --release -p codex-plus-manager)

echo "==> 打包 DMG（版本 $VERSION）" >&2
# 二进制先复制到单独目录并清掉扩展属性，再交给打包脚本（它会原样 cp 进 .app 后签名）。
BIN="$BUILD/bin"
rm -rf "${BIN:?}" && mkdir -p "$BIN"
cp "$BUILD/target/release/codex-plus-plus" "$BUILD/target/release/codex-plus-legacy-shim" "$BIN/"
xattr -c "$BIN"/* 2>/dev/null || true
(cd "$SRC" && BINARY_DIR="$BIN" bash scripts/installer/macos/package-dmg.sh "$VERSION" "$(uname -m)")

ls -la "$SRC/dist/macos/"*.dmg
