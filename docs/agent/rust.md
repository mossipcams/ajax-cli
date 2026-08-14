# Rust Changes

Read this document before changing Rust code in Ajax.

Prefer existing Ajax patterns over new frameworks or wrappers.

- Prefer concrete functions and structs.
- Add traits only for real external boundaries, test seams, or multiple
  implementations.
- Prefer explicit domain names over generic `manager`, `service`, `handler`, or
  `util` names.
- Prefer `Result` with useful context over panics.
- Avoid `unwrap` and `expect` in production code unless the invariant is
  obvious and local.
- Avoid `unsafe` and unnecessary cloning.
- Keep ownership simple and modules understandable without abstraction layers
  for their own sake.
- Preserve public APIs unless the task explicitly changes them.

Root `AGENTS.md` carries the repository-wide Rust file-size limit because it is
checked for changed files. The cohesive split policy and shared-kernel admission
rules live in [`architecture.md`](../../architecture.md#file-size-and-split-policy).

Prefer focused Rust verification before full-workspace checks, for example:

```bash
cargo nextest run -p ajax-core
cargo nextest run -p ajax-cli
cargo nextest run -p ajax-web
cargo test -p <crate> <test_name>
```

If Nextest is unavailable, use `cargo test` and report the substitution. The
slice-local and broader command catalogs live in
[`architecture.md`](../../architecture.md#validation-fast--slice-local) and
[`README.md`](../../README.md#validation).
