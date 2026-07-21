# Shell asset ETag revalidation

## Goal

Stop refetching the full ~107 KB (gzipped) shell bundle on every PWA launch when
nothing changed. Serve `ETag` + `Cache-Control: no-cache` on `/app.js`,
`/app.css`, `/terminal.js` so a launch revalidates and gets `304 Not Modified`
instead of a fresh body.

## Scope

- `crates/ajax-web/src/adapters/http.rs` — helper that builds a revalidatable
  static-asset response (ETag + `no-cache`) and the 304 short-circuit.
- `crates/ajax-web/src/runtime.rs` — `axum_app_css` / `axum_app_js` /
  `axum_terminal_js` accept `HeaderMap`, pass `if-none-match` through
  `static_asset_response`.
- Tests in `crates/ajax-web/src/runtime.rs` (inline `mod tests`).

## Non-goals

- No `immutable`. No `?v=` URL busting. Both were tried and reverted in #620
  (`immutable` suppresses revalidation even on reload and strands an installed
  PWA on a stale bundle).
- No manifest, icons, or service worker — barred by `AGENTS.md` Web Cockpit
  Guardrails.
- HTML shell (`/`, `/index.html`) and every `/api/*` route stay `no-store`.

## Design

- ETag value: `W/"<app_version()>"`. `app_version()` is already an FNV
  fingerprint over all four dist assets, so it changes on any bundle edit.
- **Weak** ETag is required: `CompressionLayer` gzips the body after the handler
  runs, so a strong ETag would claim byte-equality across identity and gzip
  representations.
- `Cache-Control: no-cache` (revalidate every time, cache may store) — not
  `no-store` (which forbids storing at all, so revalidation can never happen).
- 304 responses carry `ETag` + `Cache-Control` and an empty body.

## Tasks

- [x] T1 — Failing test: `static_shell_assets_revalidate_with_etag`. For each of
      `/app.js`, `/app.css`, `/terminal.js`: first GET is 200, carries
      `etag == W/"{app_version()}"` and `cache-control: no-cache`; second GET
      with `if-none-match` set to that etag is `304` with an empty body and the
      same `etag`. A mismatched `if-none-match` still returns 200 with a body.
- [x] T2 — Implement `static_asset_revalidated_response` in `http.rs` and wire
      the three handlers in `runtime.rs` to pass `HeaderMap`.
- [x] T3 — Update `static_shell_assets_are_no_store_and_gzipped`: static assets
      now assert `no-cache` (still asserting **no** `immutable`); shell and
      `/api/cockpit` assertions stay `no-store`. Gzip assertion unchanged.
      Renamed to `static_shell_assets_revalidate_and_are_gzipped`.
- [x] T4 — Guard: shell HTML carries no `ETag` and stays `no-store`. `/api/*`
      is covered structurally — `static_asset_revalidated_response` has exactly
      one caller (`static_asset_response`), so no API route can reach it.
- [x] T5 (added at the Review Gate) — Assert the real production request shape:
      `/app.js` with **both** `accept-encoding: gzip` and `if-none-match` still
      returns 304 with the weak ETag intact, and the gzip 200 path keeps the
      same validator. `CompressionLayer` runs after the handler, so this was the
      one combination that could have silently killed the feature. It passes
      against the round-1 production code unchanged.

## Delegation decision

`Delegation decision: delegated via model-router`

## Validation

Run by the parent, not trusted from the delegate report:

- [x] `cargo nextest run -p ajax-web` → exit 0, **174 tests run: 174 passed, 0 skipped**
- [x] `cargo clippy -p ajax-web --all-targets --all-features -- -D warnings` → exit 0, no warnings
- [x] `cargo fmt --check` → exit 0
- [ ] `npm run verify` (full local gate) — **not run**; required before opening a
      PR, not for an uncommitted worktree change.

## Deviations

- `scripts/router-log`, `scripts/delegate-snapshot`, `scripts/delegate-delta`,
  and `scripts/run-delegate` do not exist in this environment (the router and
  pi-delegate skills ship only `SKILL.md` + `agents/`). Used `git diff --quiet
  HEAD -- crates` for the pre-dispatch baseline (clean) and `git diff HEAD` for
  the delta inspection; invoked `pi -p --model … --no-session
  --no-context-files --no-skills` directly per the documented interface. No
  calibration row logged.
- The ETag reads `crate::adapters::assets::app_version()`, not
  `slices::install::app_version()` — `install` only re-exports it, and an
  adapter must not depend on a slice.
- Round 2 was a Review Gate `REVISE` for a gap in **my** packet (T5), not for
  defective delegate work.

## Results

- Round 1 (pi / `opencode-go/glm-5.2`): implementation + tests. Red proven —
  `cargo test … static_shell_assets_revalidate_with_a_weak_etag` exit 101,
  `no entry found for key "etag"`, then green at exit 0.
- Round 2 (same lane): T5 assertions only, no production change.
- Review Gate: **ACCEPT**. Both files in `ALLOWED_SCOPE`, no `immutable`, no
  `max-age`, no `?v=`, no formatting sweep, all four validation commands re-run
  by the parent.
- Uncommitted in the worktree on `ajax/pwa` at base `0a41db0`. Not committed —
  no commit was requested.
