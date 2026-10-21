# Cursor SDK sign-in from Web Cockpit (T3 Code pattern)

Status: proposed 2026-10-20, awaiting approval. Supersedes closed PR #1374.

## Evidence

- Reproduced on the host 2026-10-20: `~/.cursor/sdk/auth.json` exists (0600,
  minted 2026-10-13, expires 2027-01-12). `Cursor.auth.status()` reports
  `logged-in` (it checks expiry locally only), but `Cursor.me()` and
  `Cursor.models.list()` both throw `AuthenticationError: Invalid User API Key`.
  The stored key was rejected server-side (most likely revoked).
- Ajax never handles the key, so Ajax is not rejecting a valid credential. The
  defect is operational: Ajax has no in-product way to replace a dead key, and
  its hint tells a mobile operator to run `Cursor.auth.login()` by hand.
- T3 Code (`apps/server/src/provider/CursorAuth.ts` @37de6cb): server-owned
  login flow with phases `idle → waiting(authorizationUrl) → verifying →
  succeeded | failed`; `Cursor.auth.login({ openBrowser: false, onLoginUrl,
  store: new InMemoryCredentialStore(), signal })`; credentials are committed
  to the real store only after verification; a failed verify leaves the prior
  credential untouched; the UI observes the phase, so the URL shows while the
  login is still pending.
- `@cursor/sdk` 1.0.37 exports `FileCredentialStore` (default
  `~/.cursor/sdk/auth.json`, 0600/0700), `InMemoryCredentialStore`,
  `Cursor.me(options?: { apiKey })`, and `login({ store, signal, apiKeyName })`.

## Decision

Adopt T3's flow shape, not its storage. Ajax is single-operator, so keep the
SDK's own `FileCredentialStore` as the only store. The chat sidecar keeps
passing no `apiKey` (the SDK resolves `CURSOR_API_KEY` or the store itself),
so the existing contract in `docs/architecture/web-cockpit.md` ("Ajax never
reads, logs, or stores one") still holds: the key exists only inside the
short-lived Node login process. Nothing is placed on argv, in Rust, in the
registry, in JSONL to the browser, or in logs.

## Scope and non-goals

Scope: a login sidecar, one process-wide login flow in ajax-web, three HTTP
routes, a Settings card, a server-verified credential status, and a chat
spawn hint that points to Settings.

Non-goals: Ajax-owned credential store; explicit `apiKey` plumbing; argv keys;
multiple accounts; CLI `ajax cursor auth` (Web is the operator surface; add
when wanted); sign-out (a new sign-in overwrites); auto-renew; terminal-mode
`cursor-agent`; other harnesses.

## Slices

- [x] **1. Login sidecar** `crates/ajax-web/sidecar/cursor_sdk_login.mjs`
  (new, separate from the chat sidecar; same `--sdk-module` resolution and
  stdout console redirect). Modes:
  - `login`: `Cursor.auth.login({ openBrowser: false, store: new
    InMemoryCredentialStore(), apiKeyName: "Ajax", signal, onLoginUrl })`.
    Emit `{type:"url",url}` the moment `onLoginUrl` fires. On resolve, emit
    `{type:"verifying"}`, verify with `Cursor.me({ apiKey })`, then
    `new FileCredentialStore().save(pending)` and emit
    `{type:"succeeded",email,expiresAtMs}`. Any throw (including verify) emits
    `{type:"failed",error}` and never touches the file store. stdin EOF or
    SIGTERM aborts via the signal.
  - `status`: load `FileCredentialStore`; if present, call `Cursor.me()` and
    emit `{present, valid, email?, expiresAtMs?, error?}`.
  - Errors are redacted (strip anything matching the loaded key).
  Verify: `node --test` with a fake SDK whose `login` calls `onLoginUrl`, then
  waits on a promise the test releases. Assert the url event arrives *before*
  release; a rejected `me` leaves a pre-seeded store file byte-identical;
  abort leaves it untouched; no stdout line contains the fake key.
- [x] **2. Flow state in ajax-web**. One `static Mutex<CursorLogin>` (single
  operator) with `phase: Idle | Waiting{url} | Verifying | Succeeded{email,
  expires_at} | Failed{message}`. `start()` is idempotent: if a flow is
  Waiting/Verifying, return it; otherwise spawn the sidecar and a reader thread
  that updates the phase per event, enforcing a 300s timeout (kill child →
  Failed). `cancel()` kills the child. Reuse `JsonlProcess` and
  `cursor_launch()`'s module resolution. Verify: Rust tests against the fake
  SDK prove `start()` returns before login resolves and that the phase reaches
  `Waiting{url}` while login is still blocked.
- [x] **3. Routes** (default `api_access_policy` = browser session required):
  `GET /api/cursor/auth` → `CursorCredentialStatus` (spawns the status
  sidecar, which calls `Cursor.me()`; loaded once per Settings view),
  `GET /api/cursor/auth/login` → current `CursorLoginPhase` (memory only; the
  2s poll target), `POST /api/cursor/auth/login` → `start()` phase
  (immediate), `POST /api/cursor/auth/login/cancel` → `cancel()` phase.
  Verify: inline tests for the access policy and the JSON shapes.
- [x] **4. Settings card**. "Cursor" row shows status (`email · expires
  <date>` / `Key rejected` / `Not signed in`). The sign-in button POSTs, then
  polls status every 2s while Waiting/Verifying. Waiting renders the URL as a
  normal link (`target=_blank`); success or failure stops polling. Verify:
  vitest for each phase, plus mobile-webkit `web:smoke`.
- [x] **5. Spawn hint + docs**. Replace `LOGIN_HINT` with "Cursor rejected the
  SDK credential, sign in again from Settings → Cursor"; update
  `docs/architecture/web-cockpit.md` §Cursor auth. Verify: the existing
  spawn-error test asserts the new hint.

## Gate

`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D
warnings`, `cargo nextest run -p ajax-web`, sidecar `node --test`, `npm run
web:check`, vitest, `pkill -f vite; CI=1 npm run web:smoke`, `npm run
verify:arch`. Live: sign in from the phone, start Cursor chat, get a turn.

## Approval

Approved by Matt 2026-10-20; implemented 2026-10-20 as a 3-PR stack ("Delegate until finished"). Security-relevant:
adds a credential-writing path (SDK store only).

Host unblocked manually 2026-10-20 with the same in-memory → `Cursor.me()` →
`FileCredentialStore.save` sequence; the new key is verified (47 models), and
the rejected file is kept as `~/.cursor/sdk/auth.json.rejected-2026-10-20`.

## Validation log

- 2026-10-20 slice 1: `node --test` login 7/7, chat sidecar 13/13 (after one
  revise round: cancel check moved before `store.save`).
- 2026-10-20 slice 2: fmt/clippy clean; `nextest cursor_login` 7/7 (incl. the
  unresponsive-sidecar deadline regression, negative-controlled);
  `nextest cursor_sdk` 45/45.
- 2026-10-20 slice 3: fmt/clippy clean; `nextest cursor_ runtime::tests` 206/206.
- 2026-10-20 security fix (folded into slices 1-2): all console methods and
  uncaught errors redacted in the login sidecar; sidecar stderr never reaches
  the UI. JS 9/9 + 13/13, nextest 207/207.
- 2026-10-20 slice 4: web:check, web:lint clean; settings vitest 47/47;
  `CI=1 npm run web:smoke` 158 passed, 1 flaky unrelated (#850 reload), 2 skipped.
- 2026-10-21 gate fixes (folded into their commits): rustdoc private link;
  `cursor_auth` added to the architecture guard SLICES list and its
  access-policy test moved into runtime (slices may not reference runtime);
  CursorSignIn polls only after start succeeds (a stale idle poll could hide
  a start failure). Final gate on login-flow and sign-in-ui branches: fmt,
  clippy, rustdoc -D warnings, doctests, `nextest --all-features
  --no-fail-fast` 3077/3077, sidecar node 22/22; sign-in-ui: web:check,
  web:lint, vitest 1659/1659 (4 consecutive full runs), `CI=1 web:smoke` 159
  passed, 2 skipped.
- Not yet run: live sign-in from the phone against a deployed ajax-web.
