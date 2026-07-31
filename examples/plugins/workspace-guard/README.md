# Workspace Guard

Vetoes `write` / `edit` / `apply_patch` calls whose `file_path` escapes the
workspace (path traversal via `..`, or absolute paths outside the current
working directory).

## Install

```
sentinel plugin install examples/plugins/workspace-guard
sentinel plugin list          # shows workspace-guard
```

Works out of the box on Windows (the `.cmd` entry point is found via PATHEXT).
On Unix, create the dispatcher symlink once:

```
ln -s guard.sh guard
```

## Behavior

- Non-mutating tools: always `allow`.
- `file_path` containing `..`: `veto path traversal: <path>`.
- Absolute `file_path` outside the CLI's working directory: `veto file escapes workspace: <path>`.
- Relative paths inside the workspace: `allow`.

The hook is fail-closed for these patterns: a veto verdict always wins over
anything else the script may print.
