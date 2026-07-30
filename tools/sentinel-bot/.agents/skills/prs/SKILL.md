# PR Management Skill

Manages staging, PR descriptions, and branch targeting for the Sentinel Bot.

## Capabilities
- Stage file changes with `git add`
- Generate structured PR descriptions from template
- Target branches correctly (main vs feature branches)

## Usage
Load via `activate_skill` tool before staging any changes.

## PR Description Template
```markdown
## Summary
<one-line description>

## Changes
- `<file>` — <what changed and why>

## Related
Closes #<issue> (if applicable)
```

## Constraints
- Never push directly. Only stage with `git add`.
- Do not force-push or rewrite history.
