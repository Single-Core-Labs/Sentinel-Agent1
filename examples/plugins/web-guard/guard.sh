#!/bin/sh
# web-guard (Unix): deny web_fetch/web_search unless the target is allowlisted.
# Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
# stdout first line: "allow" | "veto <reason>". Default deny.
# Unix install: symlink `guard -> guard.sh` inside this plugin dir.
tool="$2"
case "$tool" in
  web_fetch|web_search) ;;
  *) echo "allow"; exit 0 ;;
esac

json=$(cat)
url=$(printf '%s' "$json" | sed -n 's/.*"url"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
query=$(printf '%s' "$json" | sed -n 's/.*"query"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')

if [ -n "$url" ]; then
  host=$(printf '%s' "$url" | sed -n 's|^[a-zA-Z]*://\([^/]*\).*|\1|p' | sed 's/^www\.//')
  case "$host" in
    github.com|docs.rs|en.wikipedia.org|opencode.ai) echo "allow" ;;
    *) echo "veto domain not allowlisted: $host" ;;
  esac
  exit 0
fi
if [ -n "$query" ]; then
  echo "veto web_search is not allowlisted (add 'search:*' to the allowlist to enable)"
  exit 0
fi
echo "allow"
exit 0
