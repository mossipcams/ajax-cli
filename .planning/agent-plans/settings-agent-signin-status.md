# Settings: agent sign-in status

Approval: APPROVED (Matt, 2026-10-28)

## Scope

Read-only sign-in status rows on web Settings for Claude, Codex, Cursor, Pi.

- New host-side slice `crates/ajax-web/src/slices/agent_auth.rs` exposing
  `GET /api/agent-auth`, behind the browser-session guard (add to the access
  test list in `runtime/mod.rs`, register in `slices/mod.rs` and
  `architecture.rs`).
- Response: `{ harnesses: [{ harness, installed, signedIn: bool|null, detail }] }`.
  No email, org, token, or raw CLI output crosses to the browser.
- Probes (fixed argv, no shell, `resolve_program`, stdin null, short timeout,
  run concurrently, TTL cache like `acp_launch.rs`):
  - Claude: `claude auth status --json` → `loggedIn`.
  - Codex: `codex login status` → exit code 0.
  - Pi: `pi auth check --provider <default provider> --json --no-refresh` →
    `status == "ready"`. Never `--credentials` or `print-*`.
  - Cursor: reuse `CursorLogin::status()` `valid`.
- UI: one `AgentSignIn` list at the top of Settings. Cursor keeps its existing
  Sign in flow inline on its row; other rows show the host command to run
  (`claude auth login`, `codex login`, `pi auth login`) when signed out.

## Non-goals

- Login/logout actions for Claude, Codex, Pi.
- Token validity beyond what each CLI reports (`claude auth status` reports a
  stored login, not whether the OAuth token was revoked server-side).
- Changing `ajax doctor`.

## Tasks

- [x] Rust slice + probes + route + access test (fmt, clippy, ajax-web nextest
      1542/1542, rustdoc -D warnings pass)
- [x] api.ts client + `AgentSignIn` component + tests
- [x] Update `docs/architecture/web-cockpit.md` (credentials section)
- [x] Verify (2026-10-28): fmt, workspace clippy --all-targets --all-features,
      workspace nextest 3450/3450, rustdoc -D warnings, file-loc, web:test
      1833 passed, web:check, web:lint, web:build all pass
- [ ] web:smoke (Playwright WebKit cannot run in the nono sandbox; CI Web E2E)

## Evidence

- Probe output shapes confirmed live 2026-10-28: claude `{"loggedIn":true,...}`,
  codex "Logged in using ChatGPT" exit 0, pi `{"status":"ready",...}`.

## Deviations

- Pi provider is the prefix of `PI_DEFAULT_MODEL` (`local`); live
  `pi auth check --provider local --json --no-refresh` returns ready.
- `runtime/mod.rs` was already over 600 lines (854 → 859); not split here.
- Cursor status keeps its own 30s timeout inside `CursorLogin::status`; the
  endpoint stops waiting at 10s and reports `signedIn: null`.
- No TTL cache: a cached report showed Cursor signed out right after sign-in.
- Standalone Cursor section removed; its controls live under the Agents list.
  Cursor email/expiry text is no longer shown.
