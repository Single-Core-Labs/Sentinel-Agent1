#!/usr/bin/env bash
# ---------------------------------------------------------------------------
# Sentinel AI — Secure container firewall
#
# Blocks all outbound traffic except whitelisted domains.
# Uses iptables + ipset for efficient rule matching.
# IPv6 is fully blocked.
# ---------------------------------------------------------------------------
set -euo pipefail

WHITELIST_DOMAINS=(
  "github.com"
  "api.github.com"
  "crates.io"
  "static.crates.io"
  "index.crates.io"
  "pypi.org"
  "files.pythonhosted.org"
  "npmjs.org"
  "registry.npmjs.org"
  "nodejs.org"
  "objects.githubusercontent.com"
  "api.sentinel-ai.dev"
)

# --- IPv6: block everything ---
ip6tables -P INPUT DROP 2>/dev/null || true
ip6tables -P OUTPUT DROP 2>/dev/null || true
ip6tables -P FORWARD DROP 2>/dev/null || true
ip6tables -A INPUT -j DROP 2>/dev/null || true
ip6tables -A OUTPUT -j DROP 2>/dev/null || true

# --- IPv4: create ipset ---
ipset create sentinel-whitelist hash:ip 2>/dev/null || ipset flush sentinel-whitelist

for domain in "${WHITELIST_DOMAINS[@]}"; do
  while IFS= read -r ip; do
    [[ -n "$ip" ]] && ipset add sentinel-whitelist "$ip" 2>/dev/null || true
  done < <(dig +short "$domain" 2>/dev/null || host -t A "$domain" 2>/dev/null || nslookup "$domain" 2>/dev/null | grep -oE '([0-9]{1,3}\.){3}[0-9]{1,3}')
done

# --- Default deny outbound ---
iptables -P OUTPUT DROP

# --- Allow loopback ---
iptables -A OUTPUT -o lo -j ACCEPT

# --- Allow established connections ---
iptables -A OUTPUT -m state --state ESTABLISHED,RELATED -j ACCEPT

# --- Allow whitelisted IPs ---
iptables -A OUTPUT -m set --match-set sentinel-whitelist dst -j ACCEPT

# --- Allow DNS (port 53 UDP) ---
iptables -A OUTPUT -p udp --dport 53 -j ACCEPT

# --- Log blocked packets (rate-limited) ---
iptables -A OUTPUT -m limit --limit 5/min -j LOG --log-prefix "FW-BLOCK: "

echo "[init-firewall] Rules applied: ${#WHITELIST_DOMAINS[@]} domains whitelisted"
