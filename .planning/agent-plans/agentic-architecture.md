# Agentic Architecture Optimization

Mode: Architecture Change (behavior-preserving).
Delegation decision: not delegated because Architecture Change mode — parent
implements docs + mechanical slice split after approved plan (AGENTS
delegation default applies to Small Fix / Behavior Change).

## Scope

- Slim root `architecture.md`; focused docs under `docs/architecture/`
- Kernel admission, layer rules, LOC/split policy, slice-local verify
- Split `task_command` → `resume` / `review` / `repair` / `ship` + thin
  `operator_dispatch`
- Architecture tests for new slices + layer direction
- `verify:arch` / `verify:core` / `verify:slice`

## Non-goals

- Rename `task_operations` → `slices`
- Absorb `commands/*` into slices
- Bulk peel of warn-level files
- Behavior / lifecycle / registry / Web security changes

## Checklist

- [x] Slim `architecture.md` + focused subsystem docs
- [x] Update `AGENTS.md` Read First, docs table, verify pointers
- [x] Split `task_command` + thin `operator_dispatch`
- [x] Layer-direction architecture tests + directory-backed slice support
- [x] Add verify scripts
- [x] Validate (architecture + focused tests); record results

## Deviations

- Tightened `source_mentions_path` so bare `crate::{...}` does not false-positive
  on kernel import checks.

## Validation

| Command | Result |
| --- | --- |
| `cargo test -p ajax-core architecture` | 8 passed |
| `cargo nextest run -p ajax-core -E 'test(task_operations)'` | 49 passed |
| `npm run verify:arch` | ajax-core/web/tui/supervisor architecture ok |
| `npm run verify:slice -- repair` | 73 passed |
| `cargo check -p ajax-cli -p ajax-web --all-features` | ok |
| `cargo nextest run -p ajax-web --lib` | 226 passed |
| `cargo nextest run -p ajax-cli --lib -E 'not test(binary_prints_cli_errors)'` | 353 passed, 1 skipped |

Skipped/failed unrelated: `binary_prints_cli_errors_with_display_formatting` needs an
installed `ajax-cli` binary (`Os NotFound`); not caused by this change.

Full `npm run verify` not run (not required to finish the architecture edit).
