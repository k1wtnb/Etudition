# Etudition の開発用コンテナ（Dev Container）。
# 計画（Etudition-dev の planning/implementation-plan.md の 2-6）の「コンテナの中身」を入れる。
# 版はすべて固定する。版を上げるときは、この先頭の ARG だけを変える。
# ベース：Debian bookworm（ダイジェストで固定。2026-11-24 に取得）
FROM mcr.microsoft.com/devcontainers/base@sha256:06701e75787b0b948709e803f95b42cabf37607b1c308c57471d6f0252149d87

ARG RUST_VERSION=1.100.0
ARG NODE_VERSION=24.22.3
ARG PNPM_VERSION=12.23.0
ARG CARGO_DENY_VERSION=0.22.0

ENV DEBIAN_FRONTEND=noninteractive
# Podman の既定の形式（OCI）では SHELL の指定が無視されるので、RUN はパイプの途中の失敗に頼らない書き方にする

# OS のパッケージ（bookworm の版で固定される）
#  - Tauri 2 の Linux のビルドに要るもの（Tauri 公式の前提条件の文書の Debian の一覧）
#  - ALSA（midir と cpal の Linux の裏側）
#  - F767 のファーム（arm-none-eabi-gcc、newlib、cmake、ninja）
#  - 外への通信の制限（iptables、ipset ほか）
RUN apt-get update && apt-get install -y --no-install-recommends \
      libwebkit2gtk-4.1-dev build-essential curl wget file libxdo-dev libssl-dev \
      libayatana-appindicator3-dev librsvg2-dev pkg-config \
      libasound2-dev \
      gcc-arm-none-eabi libnewlib-arm-none-eabi cmake ninja-build \
      iptables ipset iproute2 dnsutils aggregate jq sudo xz-utils ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Node.js（公式の配布物を SHA-256 で確かめて入れる）と pnpm（corepack で版を固定）
RUN arch="$(dpkg --print-architecture)"; case "$arch" in amd64) node_arch=x64;; arm64) node_arch=arm64;; *) echo "unsupported arch $arch"; exit 1;; esac; \
    cd /tmp \
 && curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" \
 && curl -fsSLO "https://nodejs.org/dist/v${NODE_VERSION}/SHASUMS256.txt" \
 && grep " node-v${NODE_VERSION}-linux-${node_arch}.tar.xz\$" SHASUMS256.txt > node.sha256 \
 && sha256sum -c node.sha256 \
 && rm -f node.sha256 \
 && tar -xJf "node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" -C /usr/local --strip-components=1 --no-same-owner \
 && rm -f "node-v${NODE_VERSION}-linux-${node_arch}.tar.xz" SHASUMS256.txt \
 && corepack enable \
 && COREPACK_HOME=/usr/local/share/corepack corepack prepare "pnpm@${PNPM_VERSION}" --activate \
 && COREPACK_HOME=/usr/local/share/corepack pnpm --version \
 && chown -R vscode:vscode /usr/local/share/corepack
ENV COREPACK_HOME=/usr/local/share/corepack

# Rust（rustup-init を SHA-256 で確かめて入れる）。rustfmt、clippy、wasm32 の対象、cargo-deny
ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN arch="$(dpkg --print-architecture)"; case "$arch" in amd64) triple=x86_64-unknown-linux-gnu;; arm64) triple=aarch64-unknown-linux-gnu;; *) echo "unsupported arch $arch"; exit 1;; esac; \
    cd /tmp \
 && curl -fsSLO "https://static.rust-lang.org/rustup/dist/${triple}/rustup-init" \
 && curl -fsSL -o rustup-init.sha256.orig "https://static.rust-lang.org/rustup/dist/${triple}/rustup-init.sha256" \
 && echo "$(cut -d' ' -f1 rustup-init.sha256.orig)  rustup-init" > rustup-init.sha256 \
 && sha256sum -c rustup-init.sha256 \
 && rm -f rustup-init.sha256 rustup-init.sha256.orig \
 && chmod +x rustup-init \
 && ./rustup-init -y --no-modify-path --profile minimal --default-toolchain "${RUST_VERSION}" \
      --component rustfmt --component clippy --target wasm32-unknown-unknown \
 && rm -f rustup-init \
 && cargo install --locked cargo-deny --version "${CARGO_DENY_VERSION}" \
 && rm -rf /usr/local/cargo/registry /usr/local/cargo/git \
 && mkdir -p /usr/local/cargo/registry \
 && chown -R vscode:vscode /usr/local/rustup /usr/local/cargo \
 && printf '%s\n' 'export RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo COREPACK_HOME=/usr/local/share/corepack' 'export PATH=/usr/local/cargo/bin:$PATH' > /etc/profile.d/etudition-tools.sh

# 外への通信を許可リストに絞るスクリプト。vscode ユーザーは、このスクリプトだけを sudo で実行できる
# （claude-code の feature が /usr/local/bin/init-firewall.sh を上書きするため、別の名前で置く）
COPY init-firewall.sh /usr/local/bin/etudition-firewall.sh
RUN chmod 755 /usr/local/bin/etudition-firewall.sh \
 && rm -f /etc/sudoers.d/vscode \
 && echo "vscode ALL=(root) NOPASSWD: /usr/local/bin/etudition-firewall.sh" > /etc/sudoers.d/vscode-firewall \
 && chmod 0440 /etc/sudoers.d/vscode-firewall \
 && mkdir -p /home/vscode/.claude /home/vscode/.config/gh /home/vscode/.local/share/pnpm \
 && chown -R vscode:vscode /home/vscode/.claude /home/vscode/.config /home/vscode/.local
