# Local test cleanup

## Scope

Delete dead/duplicate/legacy/weak local tests across nextest, CLI smoke/live,
Vitest, and Playwright. Prefer deletion over new abstraction. Preserve
behavior coverage when a stronger sibling already exists.

## Non-goals

- Rewriting production code
- Weakening or skipping remaining high-value suites
- Deleting `smoke_user_flows.rs`, `terminal-behavior` acceptance rows (except
  project scoping), `legacyTerminalRemoval.test.ts`, architecture isolation
- Broad soft-assert hardening already completed in `harden-soft-nextests.md`

## Delegation decision

`Delegation decision: delegated via model-router`

Waves are sequential packets (tests-only / mechanical). Parent reviews every
diff and runs validation.

## Inventory summary (2026-07-16)

- Ghostty/Surface V2 renderer suites already deleted; hygiene guard remains
- Soft CLI/TUI nextests already hardened
- Main remaining work: overlap deletion, fixture merge, Playwright project
  scoping, weak live_cli rows, stale TERMINAL_*.md docs

## Checklist

### Wave 1 — Safest deletes + Playwright scope
- [x] Delete `live_cli.rs`: `ajax_start_creates_task_like_new`, `ajax_parses_new_operator_verbs`
- [x] Delete `smoke.test.ts`: keyboard-band duplicate, settings visibility-only
- [x] Delete `runtime.rs` legacy router source-scan tests (2)
- [x] Scope `terminal-behavior.test.ts` to `mobile-webkit`; keep desktop expanded-mode row on desktop
- [x] Focused nextest + web:test + web:smoke green

### Wave 2 — Fixture merge + live_cli verb merge + smoke overlap
- [x] `layout-scroll.test.ts` / `visual.test.ts` import shared `e2e/fixtures.ts`
- [x] Merge six `ajax_*_dispatches_like_*` live_cli tests into one parameterized test
- [x] Trim smoke rows duplicated by App.test / actions (connection error, update banner) if sibling coverage confirmed
- [x] Focused validation green

### Wave 3 — Harden weak rows + docs
- [x] Harden `ajax_tidy_dispatches_like_sweep` (asserts `title == "sweep cleanup"` and `commands.is_array()`; commands array may be empty for risky seeded task)
- [x] Harden `NewTaskSheet.test.ts` `request_id` (exact `expect.any(String)` + non-empty length)
- [x] Harden `diagnostics.test.ts` body (exact pretty-printed `JSON.stringify({ version: "0.1" }, null, 2)` — matches `diagnosticFetch` formatting)
- [x] Update `smoke.test.ts` top comment to drop removed connection-recovery / version-poll claims
- [x] Rewrite `TERMINAL_LEGACY_SURFACE_TESTS.md` top status to historical-inventory framing; mark `e2e/smoke.test.ts` terminal rows as already removed
- [x] Add Task 12 status note to `TERMINAL_REBUILD_ACCEPTANCE.md` (framing "Red after Task 12" cells as historical)
- [x] Update `viewport.test.ts` comment to drop deleted `TerminalPanel.test.ts` citation
- [x] Broader validation (Wave 3 focused suite; full PR gate still open)

### Validation (blocking for PR later)
- [ ] `cargo nextest run -p ajax-cli -p ajax-web --all-features`
- [ ] `npm run web:test -- --run`
- [ ] `npm run web:smoke` (Playwright)
- [ ] Record results here

## Deviations

- Wave 1 Playwright scope uses a single file-level `test.beforeEach` (preferred
  pattern from packet) instead of per-test skip helpers. Same effective
  coverage, less repetition.
- Wave 2: merged five verb dispatch tests (not six); tidy/ready kept separate.
- Wave 3: diagnostics assert uses pretty-printed JSON (`null, 2`) because
  `diagnosticFetch` re-stringifies parsed JSON; packet's compact literal was wrong.

## Validation results

Wave 1:
- `cargo nextest run -p ajax-cli -p ajax-web --all-features --test-threads=1` — 469 passed
- `npx vitest ... legacyTerminalRemoval.test.ts` — 1 passed
- Playwright `desktop-chromium` terminal-behavior — 1 passed, 63 skipped
- Playwright `mobile-webkit` `-g "desktop expanded mode"` — skipped as expected
- Full mobile-webkit terminal-behavior suite — not run this wave (cost); skip filter proven

Wave 2:
- `cargo nextest run -p ajax-cli --all-features --test live_cli` — 11 passed
- `npm run web:smoke -- --project=desktop-chromium` layout-scroll + visual + smoke — 12 passed

Wave 3:
- `cargo nextest run -p ajax-cli --all-features --test live_cli` — 11 passed (includes hardened `ajax_tidy_dispatches_like_sweep`)
- `npx vitest ... NewTaskSheet.test.ts diagnostics.test.ts` — 23 passed
- `npx vitest ... --run` (full web suite) — 278 passed
- `cargo nextest run -p ajax-cli --all-features` — 330 passed
- `cargo check/clippy -p ajax-cli --tests` — clean
- `TERMINAL_BEHAVIOR_CONTRACT.md` not touched (deferred follow-up)
