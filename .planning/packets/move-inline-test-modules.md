# TDD Implementation Packet — Move inline test modules out of declaration-only files

```yaml
PACKET_STATUS: READY
TASK_KIND: refactor
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
```

## Task contract

Two files are module-declaration files that have become test dumping grounds:

| File | Production lines | Test lines |
|---|---|---|
| `crates/ajax-core/src/task_operations.rs` | **6** (five `pub mod` decls) | 2,004 |
| `crates/ajax-tui/src/lib.rs` | **11** (module decls + a lint attr) | 3,956 |

Move each inline `#[cfg(test)] mod tests { ... }` body into a sibling `tests.rs` file, leaving
a declaration behind. **Pure motion refactor: behavior must not change and no test may be
added, removed, weakened, or edited.**

This follows the pattern the repo already uses at `crates/ajax-cli/src/lib.rs:452-454`
(`#[cfg(test)] #[path = "lib/tests.rs"] mod tests;` → `src/lib/tests.rs`). Match it exactly.

## Allowed files

- `crates/ajax-core/src/task_operations.rs` (edit)
- `crates/ajax-core/src/task_operations/tests.rs` (create)
- `crates/ajax-tui/src/lib.rs` (edit)
- `crates/ajax-tui/src/lib/tests.rs` (create)

## Forbidden changes

- Any other file. In particular do NOT touch `crates/ajax-core/src/commands.rs`,
  `crates/ajax-web/src/runtime.rs`, or `crates/ajax-core/src/registry/sqlite.rs` — they have
  real production content and are explicitly out of scope.
- Do NOT change, weaken, delete, rename, reorder, or reformat any test, assertion, helper,
  or `use` statement. **Cut and paste the body; do not retype it.**
- Do NOT touch the pre-existing uncommitted changes in `crates/ajax-cli/src/web_backend.rs`,
  `crates/ajax-web/src/slices/install.rs`, `crates/ajax-core/src/registry/sqlite.rs`, or
  `crates/ajax-core/src/registry/sqlite/migrations.rs` (accepted work from earlier tasks).
- Do NOT add `#![allow(...)]` or any lint suppression to make it compile.

## Code anchors

### A. `crates/ajax-core/src/task_operations.rs`

Current shape:

```rust
pub mod drop_task;
pub mod kernel;
pub mod start;
pub mod sweep_cleanup;
pub mod task_command;

#[cfg(test)]
mod tests {
    ...2,004 lines, closes at EOF...
}
```

- Move the **body** of `mod tests` (everything between `mod tests {` and its closing brace at
  EOF) into a new file `crates/ajax-core/src/task_operations/tests.rs`, de-indented by one
  level. The `task_operations/` directory already exists (it holds `drop_task.rs`, `kernel.rs`,
  `start.rs`, `sweep_cleanup.rs`, `task_command.rs`).
- Leave behind exactly:

```rust
#[cfg(test)]
mod tests;
```

  No `#[path]` attribute is needed here — `mod tests;` inside `task_operations.rs` resolves to
  `task_operations/tests.rs` natively (Rust 2021).
- **Paths do not change.** `tests.rs`'s parent module is still `task_operations`, so the
  existing `use super::drop_task::{...}` / `use crate::...` lines keep resolving. Do not
  rewrite them.

### B. `crates/ajax-tui/src/lib.rs`

Current shape: `#![deny(unsafe_op_in_unsafe_fn)]`, `mod actions;` … `mod runtime;`,
`#[cfg(test)] mod architecture;`, then the inline `#[cfg(test)] mod tests { ... }` closing at EOF.

- Move the **body** of `mod tests` into a new file `crates/ajax-tui/src/lib/tests.rs`,
  de-indented by one level.
- Leave behind exactly (this **must** carry `#[path]` — at a crate root a bare `mod tests;`
  would resolve to `src/tests.rs`, not `src/lib/tests.rs`):

```rust
#[cfg(test)]
#[path = "lib/tests.rs"]
mod tests;
```

- Keep `#[cfg(test)] mod architecture;` exactly where it is. Do not move or touch it.
- Keep the `#![deny(unsafe_op_in_unsafe_fn)]` crate attribute at the top of `lib.rs`.
- **Paths do not change.** `tests.rs`'s parent is still the crate root, so existing
  `use super::...` / `use crate::...` lines keep resolving. Do not rewrite them.

## Verification commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features --test-threads=1
```

All must exit 0.

### Proof 1 — test count is unchanged (required)

Baseline measured on the current tree: **974**.

```bash
cargo nextest list -p ajax-core -p ajax-tui --all-features 2>/dev/null | grep -c '::'
```

Must print exactly **974**. A different number means you added, dropped, or renamed a test —
STOP and report.

### Proof 2 — pure motion (required, paste the FULL output)

Because this is a pure move, every non-blank line must survive, modulo indentation:

```bash
for pair in \
  "crates/ajax-core/src/task_operations.rs:crates/ajax-core/src/task_operations/tests.rs" \
  "crates/ajax-tui/src/lib.rs:crates/ajax-tui/src/lib/tests.rs"; do
  src="${pair%%:*}"; new="${pair##*:}"
  echo "### $src"
  git show "HEAD:$src" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$' | sort > /tmp/b.txt
  cat "$src" "$new"   | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$' | sort > /tmp/a.txt
  diff /tmp/b.txt /tmp/a.txt
done
```

For **each** file the diff must contain **only** these lines and nothing else:

- removed: `mod tests {` and its final closing `}` (the wrapper)
- added: `mod tests;` — plus, for ajax-tui only, `#[path = "lib/tests.rs"]`

If a single test name, assertion, `use`, or helper line appears in that diff, you have altered
a test — **STOP and report immediately.**

(Note: `#[cfg(test)]` appears on both sides and will not show in the diff. A stray `}` may
appear removed/added once per file due to the de-indent — that is expected for the wrapper
brace only.)

## Stop conditions

- Any verification command fails and the fix requires editing a Forbidden file.
- Proof 1 prints anything other than 974.
- Proof 2 shows any line that is not the `mod tests` wrapper / declaration.
- You are tempted to also move tests out of `commands.rs`, `runtime.rs`, or `sqlite.rs` — do
  not; out of scope.
- You are tempted to add a lint `allow` to make it compile.
