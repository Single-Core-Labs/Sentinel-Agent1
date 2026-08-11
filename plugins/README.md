# Sentinel Guard Plugins

Fail-closed safety plugins for the agent tool loop. Each plugin is a
directory with a `sentinel-plugin.toml` manifest plus cross-platform hook
scripts: `guard.cmd` → `guard.ps1` (Windows, via PowerShell) and an
executable `guard` shell script (Unix).

## Install

```bash
# from the repo root
sentinel plugin install plugins/workspace-guard
sentinel plugin install plugins/web-guard
sentinel plugin install plugins/command-guard
sentinel plugin list
```

Plugins load automatically at the next `sentinel ai` run. To uninstall:

```bash
sentinel plugin remove workspace-guard
```

## Guards

| Plugin | Rule |
|---|---|
| `workspace-guard` | Veto `write` / `edit` / `apply_patch` when the canonicalized target path escapes the workspace root. |
| `web-guard` | Veto `web_fetch` / `web_search` unless the host is in `allowlist.txt`. Deny-by-default; `web_search` needs `search:*` enabled. |
| `command-guard` | Veto `run_shell_command` matching destructive patterns in `patterns.txt` (`rm -rf /`, `format c:`, `mkfs`, force-push, ...). |

## Hook contract

Scripts are invoked as `guard <event_type> <tool_name>` with the full event
JSON on stdin:

```json
{"type":"before_tool_call","tool_name":"write","args":{"file_path":"C:\\evil\\x"}}
```

The first stdout line decides the outcome:

- `veto <reason>` — block the tool call; execution continues with the next
  tool in the batch
- `deny <reason>` — block the tool call and abort the entire batch; the agent
  run terminates with the reason (fail-closed, highest priority)
- `allow` (or empty) — continue

## Notes

- The Unix `guard` scripts are POSIX `sh`; the executable bit is tracked by
  git, so clean checkouts are directly runnable. Windows uses `guard.cmd`
  (resolved automatically by `cmd /C`).
- Scripts do not resolve symlinks; the sandbox jail remains the containment
  boundary for adversarial paths.
- See each plugin's `README.md` for details and `docs/design/policy-moat.md`
  for the threat model.
