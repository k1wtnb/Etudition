---
name: etudition-conventions
description: Use when writing or reviewing Rust or TypeScript code in the Etudition repository. Rust and TypeScript coding conventions (D4). / Etudition の Rust と TypeScript のコードを書く・レビューするときに使う。規約（D4）。
---

# Etudition の Rust と TypeScript の規約

共通 Skill（coding-standards）の命名規則は Python と C/C++ だけを定めているので、Rust と TypeScript はここで定める（計画の D4。置き場所は 2026-11-25 に人間が承認）。C/C++（F767 のファーム）は共通 Skill のハンガリアン記法に従う。

## Rust

- 整形は rustfmt の既定（`cargo fmt`）。設定ファイルは置かない。
- clippy の警告は誤りとして扱う（`cargo clippy --workspace --all-targets -- -D warnings`）。
- 命名は Rust の慣習：型とトレイトは UpperCamelCase、関数・変数・モジュールは snake_case、定数は UPPER_SNAKE_CASE。
- `unsafe` は使わない（ワークスペースの lint で forbid）。
- 公開する項目には、何をするかを1文以上の doc コメント（`///`）で書く。

## TypeScript（`app/ui`）

- `tsconfig.json` の `strict` を有効にする。
- 整形と lint は Biome（`pnpm -C app/ui run lint`）。
- 命名：型とクラスは UpperCamelCase、関数と変数は lowerCamelCase、定数は UPPER_SNAKE_CASE。

## 共通

- 頼まれていない機能、1回しか使わない抽象、頼まれていない設定を作らない（CLAUDE.md の1章）。
- 確かめは `./scripts/check-container.sh` を Dev Container の中で通す。
