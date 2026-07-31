# Web Guard

Allowlist for `web_fetch` / `web_search`. Default posture: **deny everything** —
the agent must not reach the open web until an operator whitelists domains.

## Install

```
sentinel plugin install examples/plugins/web-guard
```

On Unix, create the dispatcher symlink once: `ln -s guard.sh guard`.

## Allowlist

Default allowlisted hosts (edit the script to change):

- `github.com`, `docs.rs`, `en.wikipedia.org`, `opencode.ai`
- `web_search` is denied unless `search:*` is added to the allowlist.

Anything else → `veto domain not allowlisted: <host>` (fail-closed).
