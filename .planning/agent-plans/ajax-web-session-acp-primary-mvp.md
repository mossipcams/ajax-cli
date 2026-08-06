# Ajax Web Session: ACP-primary Operate MVP

## Scope

Make Ajax Web Session the real Cursor operate agent when the flag is on:
host preference, skip interactive Cursor at start, hub lifetime = task,
structured ACP progress, live_status from ACP, Open Terminal escape.

## Non-goals

Default-on; non-Cursor agents; classic PWA; session/load-only durability;
plan editor; symbol indexer rewrite.

## Approval

User locked B + MVP and approved ACP-primary architecture for Cursor when flag on.

## Delegation decision

`Delegation decision: not delegated because the implementation delegate tool was
unavailable; executed locally in wave-sized slices.` ACP-primary spans core
start, hub lifetime, wire DTOs, and live_status, so the work stayed in one
worktree and was reviewed after each seam.

## Checklist

- [x] Wave 0 — docs + host preference
- [x] Wave 1 — start path + hub lifetime + Open Terminal
- [x] Wave 2 — tool/diff progress wire + cards
- [x] Wave 3 — ACP → live_status
- [x] Wave 4 — validation + docs freeze

## Deviations

- Codebase-intel MCP bootstrap/decision calls were rejected by the host; source
  inspection and existing architecture docs were used instead.
- ACP live_status uses one bridge callback into the runtime registry; no browser
  task truth or duplicate hub sink was added.
- Cursor `session/load` is attempted on peer respawn, with `session/new` fallback;
  durable session catalog storage remains out of scope.
- Legacy dual-agent tasks documented; no silent TUI kill.

## Validation

```text
cargo fmt --all -- --check                              → pass
cargo check -p ajax-core --all-targets                  → pass
cargo check -p ajax-cli --all-targets                   → pass
cargo check -p ajax-web --all-targets                   → pass
cargo nextest run -p ajax-core new_task                 → 36 passed, 838 skipped
cargo nextest run -p ajax-web web_session               → 23 passed, 250 skipped
cargo nextest run -p ajax-web <focused ACP/pref tests>  → 4 passed, 269 skipped
npm run web:test -- --run src/features/session/webSessionTransport.test.ts
  → 9 passed
npm run web:test -- --run                              → 751 passed (77 files)
git diff --check                                        → pass
```

The full frontend run prints existing jsdom xterm canvas diagnostics but exits
successfully. Manual iOS Safari smoke was not run. Initial failing tests were
intentional red-phase checks and passed after implementation. Checklist above is
complete; no commits, pushes, or branch changes were made.
