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
# 開発用のビルド（流し込みのコマンドを含む。REQ-106、AC-23）も確かめる
run cargo clippy -p etudition-app --all-targets --features dev-feed -- -D warnings
run cargo test -p etudition-app --features dev-feed
run cargo build -p etudition-core --target wasm32-unknown-unknown
run cargo deny check

run pnpm -C app/ui install --frozen-lockfile
# ts-rs が `cargo test` のときに書き出した画面の型を Biome の形にそろえ、commit したものと同じかを確かめる
# （Rust の型を変えたら、書き出しを commit する。Biome の検査は書き出しにもかける）
run pnpm -C app/ui exec biome format --write src/lib/bindings
run git diff --exit-code -- app/ui/src/lib/bindings
run pnpm -C app/ui run check
run pnpm -C app/ui run lint
run pnpm -C app/ui run test
run pnpm -C app/ui run licenses

echo "== すべて通った"
