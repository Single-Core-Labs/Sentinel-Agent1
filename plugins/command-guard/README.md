# Command Guard

Fail-closed shell guard: vetoes `run_shell_command` calls whose command text
matches any regex in `patterns.txt`.

- Covers `rm -rf /`, `format c:`, `del /s`, `rd /s`, `Remove-Item -Recurse`,
  `mkfs`, `diskpart`, `dd if=`, `shutdown`/`reboot`, block-device redirection,
  fork bombs, `git reset --hard`, and force-pushes.
- Default posture: deny matches; everything else allowed (this guard does not
  gate non-destructive commands).

## Install

```bash
sentinel plugin install plugins/command-guard
sentinel plugin remove command-guard   # uninstall
```

## Customize

Edit `patterns.txt`: one regex per line, `#` comments, matched
case-insensitively. Patterns must compile in both PowerShell `-match`
(.NET regex) and POSIX `grep -E`.
