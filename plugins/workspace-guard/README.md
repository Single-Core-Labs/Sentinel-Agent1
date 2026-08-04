# Workspace Guard

Fail-closed file-system guard: vetoes `write` / `edit` / `apply_patch` when
the resolved target path escapes the workspace root (the directory where
`sentinel ai` was launched).

- Paths are canonicalized (`..` collapse; `realpath` / `GetFullPath`) then
  prefix-checked against the workspace root.
- `apply_patch` is scanned for `+++ <path>` targets; absolute paths escaping
  the workspace are vetoed. Git-style relative targets (`+++ b/src/x.rs`)
  pass.
- Relative `file_path` values are resolved against the workspace root.

## Install

```bash
sentinel plugin install plugins/workspace-guard
sentinel plugin remove workspace-guard   # uninstall
```

## Notes

- If the agent legitimately manages files outside the workspace (e.g.
  `~/.config`), remove this plugin rather than relaxing it.
- Symlinks are not resolved; the sandbox jail remains the containment
  boundary for adversarial paths.
