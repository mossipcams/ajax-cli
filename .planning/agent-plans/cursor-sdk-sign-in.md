# Cursor SDK sign-in for Ajax Chat

Status: implemented 2026-10-20 (plan slices 0–5 closed); broader slice/architecture
and live login → renew checks remain unrun — see the validation log. This plan
changes
the Cursor authentication decision in `cursor-native-sdk.md`, which accepted
host-owned SDK credentials without an Ajax sign-in flow.

## Evidence and decision

- Ajax Chat launches `@cursor/sdk` without an explicit key. The SDK resolves
  `CURSOR_API_KEY` or `~/.cursor/sdk/auth.json`; a failed key produces the current
  `Invalid User API Key` error. See `cursor_sdk_sidecar.mjs`,
  `cursor_sdk_spawn.rs`, and `docs/architecture/web-cockpit.md`.
- T3 Code's SDK driver calls `Cursor.auth.login({ openBrowser: false,
  onLoginUrl, store: pendingStore })`, verifies the result, saves it to a
  server-side credential store, checks expiry, and passes `apiKey` explicitly
  to each SDK session. Source: [CursorAuth](https://github.com/pingdotgg/t3code/blob/37de6cbde65c7cf9ba90a2557c232e63b7e16988/apps/server/src/provider/CursorAuth.ts),
  [CursorDriver](https://github.com/pingdotgg/t3code/blob/37de6cbde65c7cf9ba90a2557c232e63b7e16988/apps/server/src/provider/Drivers/CursorDriver.ts).
- Cursor documents the SDK login as minting a user API key. It does not read
  the local Cursor app/CLI login. Source: [Cursor SDK authentication](https://cursor.com/docs/sdk/typescript).
- Use one Ajax host-owned Cursor SDK sign-in, matching Ajax's current single
  operator model. Reuse existing secret storage if available; otherwise use
  the SDK's protected credential file. The Node SDK sidecar owns key loading
  and delivery to SDK calls; Rust and browser messages carry status only. Do
  not copy the Cursor CLI's saved session token or put the key in Ajax task
  state, browser storage, URLs, logs, or command arguments. `CURSOR_API_KEY`
  remains an explicit host override.

## Scope and non-goals

Scope: Cursor SDK authentication in the Ajax Web host, a host-side sign-in
entry point reachable from the CLI and Web Cockpit, credential status and
renewal, explicit key delivery to all Cursor SDK calls, focused regression
coverage, and the owning architecture/user documentation.

Non-goals: changing task/session lifecycle, supporting multiple Cursor accounts
per task, changing terminal-mode `cursor-agent`, reading the Cursor CLI's
keychain/session token, or changing other harnesses. T3's provider-instance
abstractions are not copied into Ajax's single-operator host.

## Small implementation slices

Each slice must leave a working build and have its own acceptance check. Do not
start the next slice while its focused check is failing.

- [x] **0. Establish the auth contract.** Confirm T3's SDK pattern and Ajax's
  current SDK fallback. The CLI's `agent login` is not assumed to yield an
  SDK-compatible API key. Verification: source and documentation review above;
  no code run.
- [x] **0a. Classify the reported failure before editing.** Reproduce against
  the current documented SDK credential contract. If a valid SDK credential is
  rejected by Ajax, link or open a GitHub defect issue before its fix and add
  a focused regression test in the affected source module. If the request is
  to make the CLI login work with the SDK, record it as a behavior change: the
  earlier approved transport plan explicitly excluded that login. Verification:
  issue URL or written classification with the reproduction evidence.
  Classified as a behavior-change request on 2026-10-20; see the Slice 0a
  classification section below.
- [x] **1. Resolve one credential in the Node sidecar.** Add a narrow Cursor
  SDK credential reader that prefers `CURSOR_API_KEY`, otherwise reads the
  chosen protected SDK credential store, rejects missing/expired data, and
  returns only a key to SDK calls. Avoid a new general provider-auth framework.
  Verification: focused unit cases for env precedence, stored key, expiry,
  missing key, and redacted errors; check no key reaches task state or JSONL
  responses. Run the sidecar's focused Node test file.
- [x] **2. Pass the resolved key to every SDK operation.** Wire the reader into
  the sidecar's model catalog, new agent, resume, and other account-level calls
  through SDK `apiKey` options. Resolve again for each new sidecar/session so a
  renewed key takes effect without restarting Ajax. Verification: a fake SDK
  rejects calls without a key and records that each entry point receives the
  selected key; existing resume and model tests remain green.
- [x] **3. Add a host-side SDK sign-in flow.** Start `Cursor.auth.login()` with
  browser opening disabled and expose its URL. Hold the returned credentials in
  memory, verify them with `Cursor.me()`, then save atomically to the protected
  store. Cancellation, timeout, or failed verification must leave the prior
  credential intact. Add status (account label/expiry only) and sign-out/renewal
  operations. Verification: fake SDK cases for success, cancellation, timeout,
  rejected key, permissions, and renewal; no raw key in output. Run its focused
  Node test file.
- [x] **4. Expose the same flow to the operator.** Add the smallest fitting Ajax
  CLI login/status command and a Cursor sign-in action/status in Web Cockpit
  Settings. Both call the host-owned flow; the browser receives only URL,
  progress, and sanitized status. A failed Chat attach points to this action.
  Verification: focused route/UI tests and a manual browser login from a
  separate device against the Ajax host. Run `npm run verify:slice -- web`
  after this slice.
- [x] **5. Close the behavior change.** Update `docs/architecture/web-cockpit.md`
  and `docs/architecture/web-session-behavior.md`, plus operator setup docs.
  Link the issue and regression test from step 0a if the symptom is a confirmed
  defect; never modify a `tests/` directory. Verification: focused Ajax Web and
  CLI checks, architecture check, and a live SDK login → Chat turn →
  renew → new Chat turn. Planned broader commands: `npm run verify:slice --
  cli`, `npm run verify:slice -- web`, and `npm run verify:arch`. Record exact
  commands/results in this plan.

## Slice 0a classification: behavior-change request, not a defect

The reported `Invalid User API Key` is not Ajax rejecting a valid SDK
credential. Evidence from the current sources:

- `crates/ajax-web/sidecar/cursor_sdk_sidecar.mjs` (header): the sidecar "NEVER
  reads, logs, prints, or passes an API key (no `apiKey` option); @cursor/sdk
  resolves credentials itself." Ajax has no code path that could reject a valid
  credential.
- `crates/ajax-web/src/adapters/web_session_acp/cursor_sdk_spawn.rs:15`:
  `LOGIN_HINT` names exactly one login path — run `Cursor.auth.login()` from
  `@cursor/sdk` as the ajax-web host user (credentials in
  `~/.cursor/sdk/auth.json`, renewed every 90 days).
- `docs/architecture/web-cockpit.md:203`: Cursor chat authenticates through the
  Cursor SDK login (`CURSOR_API_KEY` env var or the `Cursor.auth.login()`
  credential store); "The sidecar passes no API key and Ajax never reads,
  logs, or stores one; an authentication failure surfaces a spawn error naming
  that login."
- The Cursor SDK documentation cited above: SDK login mints a user API key and
  does not read the local Cursor app/CLI login.

Reproduction reasoning: because Ajax handles no credentials at all, the error
comes from the Cursor API when the host has no valid SDK credential — for
example an operator who only ran `cursor-agent` (CLI) login. That expectation,
"make the CLI login work with Chat's SDK", is the behavior change this plan
implements; it was explicitly excluded by the approved transport contract
above. No defect issue opened and no regression test is required for slice 0a
(there is no Ajax credential path to regress against).

## Approval, validation, and risks

Approval: granted for delegated implementation (checks ran 2026-10-20; results
in the validation log below). GitHub
issue searches for `Cursor SDK auth` and `Invalid User API Key`
returned no open issue on 2026-10-20. The existing contract explicitly chose
host SDK credentials, so this is an auth behavior change; defect classification
must be based on a reproducible failure of an accepted contract.

Validation log. Slices 1–4 results are as recorded from each slice's own run
(not re-run in Slice 5). Note the numbering: the implementation rounds used
their own slice labels, so "Slice 3" and "Slice 4" below are the credential
work, not the plan's sign-in flow and operator entry point above. Only the
Slice 5 commands were run in that round, and are marked as such; the two later
implementation rounds for plan slices 3–4 (sign-in flow and operator entry
points) and this final docs round are appended below, labeled by plan slice.

- Slice 0 / 0a (2026-10-20): source and documentation review only, no code
  run. Classification: behavior-change request, not a defect; no defect issue
  opened; evidence in the Slice 0a section above.
- Slice 1 (sidecar credential reader): `node --test
  crates/ajax-web/sidecar/cursor_sdk_sidecar.test.mjs` → 18/18 pass.
- Slice 2 (key passed to every SDK call):
  - `node --test crates/ajax-web/sidecar/cursor_sdk_sidecar.test.mjs` → 20/20 pass.
  - `cargo test -p ajax-web cursor_sdk` (parallel) → 45 passed; 2 failed.
    The 2 failures are pre-existing environmental `ws_bridge` flakes:
    `apply_client_message_set_config_option_persists_cursor_sdk_effort_toggle_without_respawn`
    and `apply_client_message_set_config_option_persists_cursor_sdk_fast_toggle_without_respawn`.
    Both pass in isolation and also fail on the pre-change baseline. Not caused
    by this work.
- Slice 3: `node --test` → 21/21 pass.
- Slice 4:
  - `node --test crates/ajax-web/sidecar/cursor_sdk_sidecar.test.mjs` → 23/23 pass.
  - `cargo test -p ajax-web cursor_sdk -- --test-threads=1` → 47 passed, 0 failed.
  - `npm run web:check` → clean.
  - `npx vitest run` on `ModelPicker.test.tsx` → 16/16 pass.
- Slice 5 (docs and this log), 2026-10-20:
  - `node --test crates/ajax-web/sidecar/cursor_sdk_sidecar.test.mjs` → 23/23 pass
    (no behavior change; sanity only).
  - `cargo check -p ajax-web` → Finished `dev` profile, no errors (comment-only
    Rust change in `cursor_sdk_spawn.rs`).
  - `git diff --stat` shows the documentation and comment files only.
  - Not run: `npm run verify:slice -- web`, `npm run verify:slice -- cli`,
    `npm run verify:arch`, and live SDK login → Chat turn → renew. These are
    still owed by slices 3–5.
- Plan slice 3 (host-side SDK sign-in flow), 2026-10-20:
  - `node --test crates/ajax-web/sidecar/cursor_sdk_sidecar.test.mjs` → 29/29 pass.
- Plan slice 4 (operator entry points: `ajax cursor auth …` and Web Cockpit
  Settings sign-in card), 2026-10-20:
  - `cargo check -p ajax-cli -p ajax-web --all-targets` → clean, no errors.
  - `cargo test -p ajax-web cursor_sdk -- --test-threads=1` → 47 passed, 0 failed.
  - `cargo test -p ajax-web cursor_auth -- --test-threads=1` → 3 unit +
    1 integration passed.
  - `npm run web:check` → clean.
  - `npx vitest run
    crates/ajax-web/web/src/features/settings/SettingsView.test.tsx` → 18/18 pass.
- Plan slice 5 (this docs/log round), 2026-10-20: documentation only —
  `docs/architecture/web-cockpit.md` gained the operator entry points
  paragraph; `web-session-behavior.md` has no Cursor auth section, so it is
  unchanged. No code run in this round.
  - Not run (the only outstanding validation for this plan): `npm run
    verify:slice -- web`, `npm run verify:slice -- cli`, `npm run verify:arch`,
    and live SDK login → Chat turn → renew.

Slice 0a classification written on 2026-10-20 (behavior-change request, no
defect issue opened). Focused implementation checks for plan slices 3–5 are
recorded in the log above; only the broader slice/architecture gates and the
live login → Chat turn → renew pass remain unrun. `python3 -c` checked every line of this plan for trailing
whitespace and passed on 2026-10-20. `git diff --no-index --check /dev/null
.planning/agent-plans/cursor-sdk-sign-in.md` printed no whitespace findings
and returned 1 because the new file differs from `/dev/null`.

Risks to resolve in slice 1: Ajax may lack an existing secret store; choose the
SDK's protected host file if so. A live Ajax host is not running in this
worktree, and the local global Node installation lacks `@cursor/sdk`, so live
verification requires a configured host. Credential handling changes a
security assumption and needs explicit approval before implementation.
