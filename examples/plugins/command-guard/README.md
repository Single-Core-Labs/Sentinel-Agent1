# Command Guard

Vetoes `run_shell_command` calls that match destructive patterns, before the
command reaches the sandbox:

- `rm -rf /`, `rm -rf ~`, `rm -rf *`
- `format <drive>:`, `del /s`, `rmdir /s`, `Remove-Item -Recurse`
- `mkfs`, `diskpart`, `dd if=` (raw device writes), `> /dev/sdX`
- `shutdown`, fork-bombs (`:(){ :|:& };:`)

## Install

```
sentinel plugin install examples/plugins/command-guard
```

On Unix, create the dispatcher symlink once: `ln -s guard.sh guard`.

## Verify

Live check (an actual agent attempt gets vetoed):

```
sentinel plugin list                                   # shows command-guard
sentinel ai --prompt "delete everything with rm -rf /" --yolo
# → tool result: "Vetoed by plugin policy: veto destructive command: rm -rf /"
```

The sandbox (Job Object / bubblewrap) remains the last line of defense; the
hook is the fast, auditable gate in front of it.
