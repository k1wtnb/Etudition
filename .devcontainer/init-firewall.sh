#!/bin/bash
# 開発用コンテナの外への通信を、許可リストの先だけに絞る。
# python-gui-app-env の init-firewall.sh（Anthropic 公式のリファレンスの devcontainer の方式：iptables + ipset）を、
# Etudition（Rust、Node、Debian）向けに許可する先を変えたもの。
set -euo pipefail
IFS=$'\n\t'

ALLOWED_DOMAINS=(
  # Claude Code
  api.anthropic.com
  claude.ai
  platform.claude.com
  console.anthropic.com
  statsig.anthropic.com
  statsig.com
  sentry.io
  # Rust（crates と rustup）
  crates.io
  index.crates.io
  static.crates.io
  static.rust-lang.org
  # Node（npm と pnpm）
  registry.npmjs.org
  # Debian のパッケージ
  deb.debian.org
  # VS Code の拡張とサーバー
  marketplace.visualstudio.com
  vscode.blob.core.windows.net
  update.code.visualstudio.com
  # GitHub の配布物（github.com の本体は下で GitHub の IP の範囲を足す）
  objects.githubusercontent.com
  release-assets.githubusercontent.com
  codeload.github.com
)

echo "[firewall] 前のルールを消す"
iptables -F
iptables -X
iptables -P INPUT ACCEPT
iptables -P OUTPUT ACCEPT
iptables -P FORWARD ACCEPT
ipset destroy allowed-domains 2>/dev/null || true
ipset create allowed-domains hash:net

echo "[firewall] GitHub の IP の範囲を取る"
gh_meta=$(curl -fsS --connect-timeout 10 https://api.github.com/meta)
echo "$gh_meta" | jq -r '(.web + .api + .git)[]' | grep -v ':' | aggregate -q | while read -r cidr; do
  ipset add -exist allowed-domains "$cidr"
done

echo "[firewall] 許可する先の名前を解決する"
for domain in "${ALLOWED_DOMAINS[@]}"; do
  ips=$(dig +short A "$domain" | grep -E '^[0-9]+(\.[0-9]+){3}$' || true)
  if [ -z "$ips" ]; then
    echo "[firewall] WARN: $domain を解決できなかった（飛ばす）"
    continue
  fi
  for ip in $ips; do
    ipset add -exist allowed-domains "$ip"
  done
done

echo "[firewall] ルールをかける"
iptables -A INPUT  -i lo -j ACCEPT
iptables -A OUTPUT -o lo -j ACCEPT
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT
iptables -A OUTPUT -p tcp --dport 53 -j ACCEPT
iptables -A INPUT  -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m conntrack --ctstate ESTABLISHED,RELATED -j ACCEPT
iptables -A OUTPUT -m set --match-set allowed-domains dst -j ACCEPT
iptables -A OUTPUT -j REJECT --reject-with icmp-admin-prohibited
iptables -P INPUT DROP
iptables -P FORWARD DROP
iptables -P OUTPUT DROP

# IPv6 はすべて止める（ループバックだけ通す）
if command -v ip6tables >/dev/null 2>&1; then
  ip6tables -F || true
  ip6tables -A INPUT  -i lo -j ACCEPT || true
  ip6tables -A OUTPUT -o lo -j ACCEPT || true
  ip6tables -P INPUT DROP || true
  ip6tables -P FORWARD DROP || true
  ip6tables -P OUTPUT DROP || true
fi

echo "[firewall] 確かめる"
if curl -fsS --connect-timeout 5 https://example.com >/dev/null 2>&1; then
  echo "[firewall] NG: example.com に届いてしまう"
  exit 1
fi
if ! curl -fsS --connect-timeout 5 https://api.github.com/zen >/dev/null 2>&1; then
  echo "[firewall] NG: api.github.com に届かない"
  exit 1
fi
if ! curl -fsS --connect-timeout 5 -o /dev/null https://index.crates.io/config.json; then
  echo "[firewall] NG: index.crates.io に届かない"
  exit 1
fi
if ! curl -fsS --connect-timeout 5 -o /dev/null https://registry.npmjs.org/pnpm; then
  echo "[firewall] NG: registry.npmjs.org に届かない"
  exit 1
fi
echo "[firewall] OK: 許可リストの先だけに届く"
