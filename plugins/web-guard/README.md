# Web Guard

Fail-closed web access guard: vetoes `web_fetch` (and `web_search` unless
explicitly enabled) when the target is not in `allowlist.txt`.

- Default posture: **deny**. Only hosts equal to an allowlist entry or a
  subdomain of one are allowed.
- `web_search` has no URL to verify, so it is vetoed unless `search:*` is
  uncommented in `allowlist.txt`.
- A missing or empty allowlist vetoes everything (fail-closed).

## Install

```bash
sentinel plugin install plugins/web-guard
sentinel plugin remove web-guard   # uninstall
```

## Customize

Edit `allowlist.txt`: one domain per line, `#` comments, subdomains match
automatically (`docs.rs` allows `std.rs.docs.rs`).
