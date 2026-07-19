# Rust-Specific Code Review Checklist

## Safety

- [ ] No `unsafe` without `// SAFETY:` comment.
- [ ] Raw pointer arithmetic bounds-checked.
- [ ] FFI functions validate inputs before calling.
- [ ] `Send`/`Sync` impls are correct.

## Error handling

- [ ] `Result` types used instead of panics.
- [ ] `anyhow::Context` or `thiserror` for error chains.
- [ ] No `unwrap()`/`expect()` in library code.

## Performance

- [ ] Hot paths avoid allocations (prefer `Vec::with_capacity`).
- [ ] Clones justified (prefer borrowing).
- [ ] No O(n²) patterns where O(n log n) is feasible.

## Concurrency

- [ ] Shared state uses `Arc<Mutex<T>>` or `tokio::sync` primitives.
- [ ] No `std::mem::forget` on async tasks.
- [ ] Channels bounded where backpressure matters.

## Dependencies

- [ ] No unnecessary crate imports.
- [ ] Version bounds specified (e.g. `"1"` not `"*"`).
- [ ] Workspace dependencies used where available.
