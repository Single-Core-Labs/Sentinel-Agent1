# Code Review Guidelines

## General

- Review for correctness, security, performance, and maintainability.
- Every public API addition must include doc comments.
- Every new feature must include tests (unit + integration).
- No unsafe code without `// SAFETY:` justification.
- All errors must be handled — no `.unwrap()` or `.expect()` in production code.

## Checklist

- [ ] Code compiles with `cargo clippy -- -D warnings`
- [ ] Tests pass: `cargo test --workspace`
- [ ] Formatting: `cargo fmt --check`
- [ ] No TODO / FIXME / HACK comments without an associated issue.
- [ ] Public APIs documented.
- [ ] Breaking changes called out in PR description.

## Label guide

| Label | Meaning |
|-------|---------|
| `review:changes-requested` | Blocking issues found |
| `review:approved` | No blocking issues |
| `review:needs-info` | More context needed from author |
