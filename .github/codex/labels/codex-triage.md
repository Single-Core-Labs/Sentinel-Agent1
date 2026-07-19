# Issue Triage Template

## Required fields

- [ ] Title describes the issue in < 80 chars.
- [ ] Body includes:
  - Version / commit SHA where the issue occurs.
  - Steps to reproduce.
  - Expected behaviour.
  - Actual behaviour.
  - Environment (OS, Rust version, target).

## Triage actions

| Criteria | Action |
|----------|--------|
| Missing reproduction steps | Label `needs-repro` and ask author |
| Duplicate of existing issue | Close with reference |
| Feature request | Label `enhancement` and route to roadmap |
| Bug with repro | Label `bug`, assign priority |
| Security issue | Label `security` and notify maintainers privately |

## Priority definitions

| Priority | Definition |
|----------|------------|
| P0 | Blocks release — fix immediately |
| P1 | Important — fix this cycle |
| P2 | Nice to have — fix when possible |
| P3 | Backlog — revisit in roadmap planning |
