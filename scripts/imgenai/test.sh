#!/usr/bin/env bash
# 跑 Rust 测试的安全方式：上游有测试会直接写 $HOME 下的真实配置
# （gateway_e2e 会把 ~/.claude/settings.json 改成指向本地测试网关，还会在 ~/.config、~/ 下建状态目录），
# 所以这里一律把 HOME 指到临时目录，Cargo / rustup 的目录显式指回真实位置。
#
# 用法：bash scripts/imgenai/test.sh [cargo test 的参数...]
#   例：bash scripts/imgenai/test.sh -p codex-plus-core --test all bundled_builtins::
#   不带参数时跑整个工作区（--workspace --no-fail-fast）。
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REAL_HOME="$HOME"
FAKE_HOME="$(mktemp -d "${TMPDIR:-/tmp}/imgenai-test-home.XXXXXX")"
trap 'rm -rf "${FAKE_HOME:?}"' EXIT

if [ "$#" -eq 0 ]; then
  set -- --workspace --no-fail-fast
fi

cd "$ROOT"
# 去掉代理：本地网络测试会被代理劫持而假失败。编译产物放在桌面（iCloud 同步）以外。
exec env -u all_proxy -u ALL_PROXY -u http_proxy -u HTTP_PROXY -u https_proxy -u HTTPS_PROXY -u NO_PROXY -u no_proxy \
  PATH="/opt/homebrew/opt/rustup/bin:$PATH" \
  CARGO_HOME="${CARGO_HOME:-$REAL_HOME/.cargo}" \
  RUSTUP_HOME="${RUSTUP_HOME:-$REAL_HOME/.rustup}" \
  CARGO_TARGET_DIR="${CARGO_TARGET_DIR:-$REAL_HOME/Library/Caches/imgenai-code-build/dev-target}" \
  HOME="$FAKE_HOME" \
  cargo test "$@"
