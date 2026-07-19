# Issue Resolution Attempt Template

Use this template when attempting to resolve an issue.  Post as a comment
on the issue before starting work.

```markdown
## Attempt

**Issue**: #<number>
**Assigned**: @<your-github-handle>

## Approach

<Brief description of the fix or implementation strategy>

## Risks

- <Potential regressions or edge cases>
- <Dependencies that may be affected>

## Testing plan

1. Unit tests for new/changed code.
2. Integration test for the reported scenario.
3. Manual verification steps (if applicable).

## Timeline

- Expected duration: <X> hours/days.
- Will update this comment with progress.
```

## After resolution

- [ ] PR opened with `Closes #<issue>` in description.
- [ ] Regression test added.
- [ ] Issue labelled `resolved` after merge.
