#!/bin/bash
# 計画（Etudition-dev の planning/implementation-plan.md の 2-4）の確かめをまとめたもの。
# Dev Container の中で、リポジトリのルートから実行する。どれか1つでも失敗したら、終了コード 1 で止まる。
set -euo pipefail
cd "$(dirname "$0")/.."

run() {
  echo "== $*"
  "$@"
}

run cargo fmt --all -- --check
run cargo clippy --workspace --all-targets -- -D warnings
run cargo test --workspace
run cargo build -p etudition-core --target wasm32-unknown-unknown
run cargo deny check

if [ -f app/ui/package.json ]; then
  run pnpm -C app/ui install --frozen-lockfile
  run pnpm -C app/ui run check
  run pnpm -C app/ui run lint
  run pnpm -C app/ui run test
  run pnpm -C app/ui run licenses
else
  echo "== app/ui はまだない（画面の確かめは飛ばす）"
fi

echo "== すべて通った"
