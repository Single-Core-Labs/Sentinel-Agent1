#!/bin/sh
# command-guard (Unix): veto run_shell_command matching destructive patterns.
# Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
# stdout first line: "allow" | "veto <reason>". Fail-closed on matches.
# Unix install: symlink `guard -> guard.sh` inside this plugin dir.
[ "$2" = "run_shell_command" ] || { echo "allow"; exit 0; }

json=$(cat)
cmd=$(printf '%s' "$json" | sed -n 's/.*"command"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
[ -n "$cmd" ] || { echo "allow"; exit 0; }

case "$cmd" in
  *"rm -rf /"*|*"rm -rf ~"*|*"rm -rf "*"*"*) echo "veto destructive command: $cmd" ;;
  *"format "*[a-zA-Z]":"*|*"del /s"*|*"rmdir /s"*|*"Remove-Item"*"-Recurse"*) echo "veto destructive command: $cmd" ;;
  *"mkfs"*|*"diskpart"*|*"dd if="*|*"> /dev/sd"*) echo "veto destructive command: $cmd" ;;
  *"shutdown -h"*|*":(){ :|:& };:"*) echo "veto destructive command: $cmd" ;;
  *) echo "allow" ;;
esac
exit 0
