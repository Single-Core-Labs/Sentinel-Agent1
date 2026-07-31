#!/bin/sh
# workspace-guard (Unix): veto write/edit/apply_patch when file_path escapes the workspace.
# Contract: invoked as `guard <event> <tool_name>`, full event JSON on stdin.
# stdout first line: "allow" | "veto <reason>".
# Unix install: symlink `guard -> guard.sh` inside this plugin dir (git cannot
# store a symlink that Windows also checks out correctly).
tool="$2"
case "$tool" in
  write|edit|apply_patch) ;;
  *) echo "allow"; exit 0 ;;
esac

path=$(cat | sed -n 's/.*"file_path"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p')
case "$path" in
  *".."*) echo "veto path traversal: $path"; exit 0 ;;
esac

cwd=$(pwd)
case "$path" in
  /*) case "$path" in "$cwd"/*) echo "allow" ;; *) echo "veto file escapes workspace: $path" ;; esac ;;
  *) echo "allow" ;;
esac
exit 0
