# Web UX round 2: dashboard recency + new-task defaults

## Scope

1. Surface task recency on the dashboard: add `last_activity_at` to the core
   `TaskCard` projection, project it as `last_activity_unix_secs` on
   `BrowserTaskCard`, render a relative "active Xm ago" on task rows, tick it
   every 60s, and use recency as the sort tiebreak within a status band.
2. NewTaskSheet remembers the last-used agent and repo (localStorage,
   presentation-only preference; server truth unchanged).

## Non-goals

- No lifecycle/priority policy changes — recency is a presentation tiebreak.
- No terminal-view changes.
- No changes to inbox ordering (severity stays authoritative).

## Delegation decision

`Delegation decision: not delegated` — continuation of the same
user-directed session ("keep going… any route you want"); scope is
self-chosen discovery work, which is on the do-not-delegate list.

## Checklist

- [x] Core: `TaskCard.last_activity_at` populated in `task_card` (+ fix
      struct literals in ajax-tui/ajax-web tests; ajax-cli uses struct spread)
- [x] ajax-web: `BrowserTaskCard.last_activity_unix_secs` + test assertion
- [x] Focused Rust tests: `cargo test -p ajax-web cockpit` (29) / `operate` (12)
- [x] Web failing tests: row shows relative activity time; sortCards recency
      tiebreak; NewTaskSheet restores last agent/repo
- [x] Implement TaskList time + 60s ticker; state.ts sort tiebreak;
      NewTaskSheet localStorage
- [x] Update TS type + fixtures (`cockpit.json`, `operation.json`) + test
      card builders
- [x] `npm run web:check` + full `npm run web:test -- --run`
- [x] `npm run web:build` + `cargo test -p ajax-web install` +
      `cargo nextest run -p ajax-cli -E 'test(web_backend)'`
- [x] `cargo fmt --check`, clippy, broader nextest for touched crates

## Deviations

- `committed_cockpit_fixture_matches_production_serialization` pinned the
  cockpit fixture to production JSON — updated `fixtures/cockpit.json` and
  `fixtures/operation.json` with `last_activity_unix_secs: 1700001000` (the
  contract-context timestamp).
- ajax-cli's TaskCard literal uses `..clone()` spread, so no edit was needed.

## Validation

- `cargo test -p ajax-web cockpit` — 29 passed (incl. fixture parity)
- `cargo test -p ajax-web operate` — 12 passed
- `cargo check -p ajax-tui -p ajax-cli --all-targets` — clean
- `npm run web:test -- <state,TaskList,NewTaskSheet> --run` — 38 passed
- `npm run web:check` — 0 errors (164 files)
- `npm run web:test -- --run` — 34 files / 454 tests passed
- `npm run web:build` — dist rebuilt
- `cargo test -p ajax-web install` — 7 passed;
  `cargo nextest run -p ajax-cli -E 'test(web_backend)'` — 19 passed
- `cargo fmt --check` + clippy (core/web/tui/cli, all targets) — clean
- `cargo nextest run -p ajax-core -p ajax-tui -p ajax-web` — 1090 passed
- `cargo nextest run -p ajax-cli` — 332 passed
- Not run: `npm run web:smoke` (Playwright) — same caveat as round 1; worth a
  visual pass on a real device before merge.
