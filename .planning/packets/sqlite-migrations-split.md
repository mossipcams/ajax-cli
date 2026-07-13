# TDD Implementation Packet — Extract sqlite migrations into their own module

```yaml
PACKET_STATUS: READY
TASK_KIND: refactor
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: REQUIRED
```

## Task contract

`crates/ajax-core/src/registry/sqlite.rs` is 3,997 lines (1,683 production). It buries
seven in-place schema migrations — the highest-blast-radius code in the repo — in the
same file as row parsers, string codecs, and save/load orchestration. Its graph cohesion
is 0.05, the weakest community in the codebase.

Move the **migration cluster only** into a child module. This is a **pure motion
refactor**: behavior must not change, and `git diff -M` should read as moved code plus
mechanical call-site updates.

**Scope is deliberately limited to migrations.** Do NOT also extract row parsers, the
`string_codec!` macro, or the codec functions. That half would force relocating a macro
and re-pointing six generated `parse_*` imports used by the test module — out of scope
for this packet. Leave them exactly where they are.

## Allowed files

- `crates/ajax-core/src/registry/sqlite.rs` (edit)
- `crates/ajax-core/src/registry/sqlite/migrations.rs` (create)

## Forbidden changes

- Any other file.
- Any change to public API. `SqliteRegistryStore::{new, load, save, load_tasks_only,
  current_revision, save_if_revision, save_if_revision_allowing_empty_rewrite}` must keep
  identical signatures and behavior. `crates/ajax-cli/src/cockpit_backend.rs` calls
  `SqliteRegistryStore::new(...)` — it must keep compiling untouched.
- Any change to SQL text, migration order, schema version, or error messages. Copy SQL
  strings **byte-for-byte**. A single altered SQL string silently corrupts user
  databases — this is the one thing that must not go wrong.
  **Cut and paste the bodies; do not retype them.** Two shapes are especially easy to
  silently reflow: the 16 multi-line `r#"..."#` raw strings (95-219, 311-394, 440-503),
  and `migrate_v6_to_v7`'s **backslash-continued** string literal at 295-300 (which has no
  closing quote on its first line). Preserve their interiors exactly, whitespace included.
- No renames, no reordering of migration steps, no "improvements" to the moved code.
- Do NOT extract row parsers / codecs / `string_codec!` (see Task contract).

## Code anchors

### Move these items OUT of `sqlite.rs` INTO `sqlite/migrations.rs`

Line numbers are from the current file, top of file = 1.

| Item | Current line | Note |
|---|---|---|
| `const SQLITE_SCHEMA_VERSION: i64 = 9;` | 21 | make `pub(super)` |
| `fn migrate` | 56 | **currently an assoc fn** of `impl SqliteRegistryStore` — becomes a free `pub(super) fn migrate(connection: &Connection)` |
| `fn create_current_schema` | 92 | **currently an assoc fn** — becomes a free fn, private to `migrations` (both callers move with it) |
| `fn migrate_v6_to_v7` | 280 | free fn |
| `fn migrate_v7_to_current_schema` | 308 | free fn; calls `create_current_schema` at 316 |
| `fn migrate_v5_to_v6` | 402 | free fn |
| `fn registry_tasks_has_column` | 420 | free fn |
| `fn migrate_v4_to_v5` | 437 | free fn |
| `fn migrate_v3_to_v4` | 450 | free fn |
| `fn migrate_v2_to_v3` | 472 | free fn |
| `fn sqlite_user_version` | 1476 | free fn |
| `fn has_legacy_payload_schema` | 1482 | free fn |
| `fn table_has_column` | 1487 | free fn |

Verified (independently, twice): the move list is **closed under call** — every one of these
symbols is referenced only by the others in this list, plus the 6 edits below (5 call sites +
1 test path). No moved body calls `col()`, `req()`, a timestamp helper, a `parse_*` codec,
`string_codec!`, or anything from `crate::models`. Nothing outside `sqlite.rs` uses them.

**`migrate` and `create_current_schema` are removed FROM the `impl SqliteRegistryStore`
block that starts at line 28 — the impl block itself STAYS.** There are two impl blocks
(28-278 and 508-552) holding `new`, `open`, `load_tasks_only`, `current_revision`,
`save_if_revision*`, `load`, and `save`. Do NOT delete an impl block wholesale; remove only
those two associated fns from it.

### Keep in `sqlite.rs` (do NOT move)

- `fn database_error` (1419). It is used ~100 times across the whole file. `migrations.rs`
  imports it with `use super::database_error;`.
- `struct SqliteRegistryStore` and all its `pub` methods.
- All `*_from_row` fns, `col`, `req`, timestamp helpers, `macro_rules! string_codec`
  (1506), all `string_codec!` invocations, and every `parse_*` / `*_name` codec fn.
- All save/load orchestration.

### Call-site edits in `sqlite.rs` (5 sites)

`Self::migrate(&connection)?` → `migrations::migrate(&connection)?` at lines
**35, 226, 257, 511, 540**. Add `mod migrations;` near the top.

Module layout note: `sqlite.rs` + a sibling `sqlite/` directory is valid (Rust 2018+).
`mod migrations;` inside `src/registry/sqlite.rs` resolves to
`src/registry/sqlite/migrations.rs`. Do not convert `sqlite.rs` into `sqlite/mod.rs`.

### One test-module edit

`mod tests` (starts line 1684) references `super::SQLITE_SCHEMA_VERSION` at **line ~3621**.
After the move, `sqlite.rs` no longer uses that const in production, so re-importing it
into the parent would trip `clippy -D warnings` as an unused import. Instead, update the
test reference to `super::migrations::SQLITE_SCHEMA_VERSION`.

This is a mechanical path update. **Do not weaken, delete, or otherwise change any test
assertion.** No other test edit is permitted.

### Imports for `migrations.rs`

Let the compiler drive these. Expect roughly:

```rust
use super::database_error;
use crate::registry::RegistrySnapshotError;
use rusqlite::Connection;
```

That is the **complete and exact** import set — verified against every moved body. In
particular **no moved function uses `rusqlite::params`**; importing it would be an unused
import and would fail the `clippy -D warnings` gate. Add nothing beyond these three lines.

## Verification commands

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-core --all-features --test-threads=1
cargo nextest run --all-features --test-threads=1
```

All must exit 0. The ajax-core test count must be **unchanged** — this refactor adds and
removes zero tests.

### Behavior-preservation proof (required — run it and paste the output)

The schema SQL in this file lives in **multi-line `r#"..."#` raw strings** passed to
`execute_batch` (see lines 94-95, 310-311, 318-319, 439-440, 452-453, 474-475), and some
is built with `format!` (e.g. `PRAGMA table_info({table})`). A grep for single-line SQL
literals would miss all of it and print a false "identical" — do not use one.

Use this **line-subset proof** instead. Because this is a pure move, every non-blank line
of the original file must still exist somewhere in the union of the two resulting files,
except the handful of lines you intentionally edited:

```bash
git show HEAD:crates/ajax-core/src/registry/sqlite.rs \
  | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$' | sort > /tmp/before.txt
cat crates/ajax-core/src/registry/sqlite.rs crates/ajax-core/src/registry/sqlite/migrations.rs \
  | sed 's/^[[:space:]]*//;s/[[:space:]]*$//' | grep -v '^$' | sort > /tmp/after.txt
diff /tmp/before.txt /tmp/after.txt
```

Trimming is required because the moved fns get de-indented when they leave the `impl` block.
**Paste the full `diff` output in your report.** It must contain **only** these intentional
edits, and nothing else:

- `Self::migrate(&connection)?;` (the old call form, now `migrations::migrate(...)`)
- `fn migrate(connection: &Connection) -> Result<(), RegistrySnapshotError> {` and
  `fn create_current_schema(...)` (indentation/visibility changed when they left the `impl`)
- `const SQLITE_SCHEMA_VERSION: i64 = 9;` (now `pub(super) const ...`)
- `supported: super::SQLITE_SCHEMA_VERSION` (test path update)
- possibly a `use` line

**If a single line of SQL — a `CREATE TABLE`, a column definition, a `PRAGMA` — appears in
that output, you have altered the schema. STOP immediately and report it.** Verified: this
command prints nothing on the unmodified tree, so any output is a real change.

## Stop conditions

- Any verification command fails and the fix requires editing a Forbidden file.
- The SQL diff above is non-empty.
- Any public method signature changes.
- Any test assertion is changed (the single `super::migrations::SQLITE_SCHEMA_VERSION`
  path update is the only permitted test edit).
- You are tempted to also move row parsers or codecs — do not; that is out of scope.
- The change requires editing `crates/ajax-cli/` or any file outside Allowed files.
