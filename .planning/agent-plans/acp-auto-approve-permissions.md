# ACP auto-approve permissions

**Status:** approved
**Approval:** operator 2026-08-20: all ACP Ajax chats should be full access; implement now.
**Branch:** `ajax/acp-permissions`
**Risk:** high (security assumption)

## Problem

Ajax Chat already discloses that orchestration sessions are trusted local
automation: Settings says supported agents run with full tool access and without
approval prompts. The host currently only *tries* to get there:

- After session create/restore, it sends `session/set_config_option` for an
  exact advertised `mode` value: `agent-full-access` (Codex) or
  `bypassPermissions` (Claude).
- Cursor and Pi are ignored (`preferred_permission_config` returns `None`).
- Missing, unadvertised, or refused config keeps `session/request_permission`
  as a **manual** operator prompt.

That fallback is what still blocks Cursor (and any harness that keeps asking).
The disclosed product behavior is already “no prompts.” The host should make
that true for every ACP permission request, not only the two known mode values.

## Goal

Auto-approve every ACP `session/request_permission` on trusted local Ajax Chat
sessions by selecting an advertised allow option, so agents never wait on the
operator.

After create/restore, apply an advertised `category: mode` (else id `mode`)
select value from the documented full-access IDs, any harness: `agent-full-access`
(Codex), `bypassPermissions` (Claude), `agent` (Cursor ACP), `code` (ACP spec
example). Never invent an id. Auto-approve remaining `session/request_permission`
with the official selected/cancelled outcome schema.

## Non-goals

- A Settings toggle to re-enable prompts.
- Advertising `fs/*` or `terminal/*` client capabilities.
- Changing PTY/terminal approval-prompt detection outside ACP Chat.
- Changing Claude `--dangerously-skip-permissions` on non-ACP launches.
- Removing the operator `permission` WebSocket command; unused in the live path
  is fine if tests still need it.
- Auto-approving anything that is not ACP `session/request_permission`.

## Replacement contract

1. **Config-option apply stays first and is schema-driven.** After create/restore,
   find the advertised mode option (`category == mode`, else id `mode`). Send
   `session/set_config_option` with an advertised select value from the closed
   full-access list (`agent-full-access`, `bypassPermissions`, `agent`, `code`).
   Do not invent ids or key off harness identity.
2. **Incoming permission requests are host-answered immediately.** In the ACP
   adapter `on_receive_request` handler, pick:
   - `AllowAlways` if advertised, else
   - `AllowOnce` if advertised, else
   - `Cancelled` (cannot approve; warn).
   Respond with the standard ACP selected/cancelled outcome on the same
   request. Do not wait for the browser.
3. **Do not surface auto-answered prompts.** Do not emit `permission_request` to
   the host transcript/browser for a request the host already answered. The
   transcript contract stays: only permission asks the operator still owes.
4. **Cancel still cancels leftovers.** `session/cancel` continues to resolve any
   still-pending permission with `Cancelled` before sending cancel. In the
   normal path there should be none.
5. **Scope is Ajax Chat ACP only.** Trusted local orchestration, already
   disclosed. No change to registry, lifecycle, or PTY authority.

## Implementation

Smallest place: `crates/ajax-web/src/adapters/web_session_acp/sdk_connection.rs`
permission receive path. Reuse the existing option-kind matching in
`respond_permission`; prefer `AllowAlways` over `AllowOnce`.

Owning docs in the same change:

- `docs/architecture/web-cockpit.md` — replace “manual permission remains the
  fallback” with host auto-approve of remaining requests.
- `docs/architecture/web-session-behavior.md` — same for the trusted-session
  and permission-persistence sections.

## Tests

Failing first, then the minimal adapter change:

- Adapter: fake ACP `--permission` prompt auto-selects `allow-once` (or
  `allow-always` when advertised) with no operator `respond_client_request`.
- Adapter: when only reject options are advertised, the host cancels rather
  than inventing an allow id.
- Host session tests that currently drive operator `answer_permission` against
  a live fake prompt should either observe auto-resolve (no pending prompt) or
  keep the explicit answer path for already-pending ids if that path remains.
- Do not weaken frontend permission-prompt tests; they still cover the wire
  types. Live Chat should no longer emit those events for auto-answered asks.

## Validation

- Parent review: auto-approve on receive, AllowAlways then AllowOnce, no
  `permission_request` surface, Codex/Claude config apply unchanged. One
  revision removed unused `json` import and unused test helper so
  `clippy -D warnings` passes.
- `cargo test -p ajax-web -- adapters::web_session_acp`: 68 passed (parent rerun).
- `cargo test -p ajax-web -- slices::web_session::task_session_tests`: 15 passed
  (parent rerun).
- `cargo clippy -p ajax-web --all-targets -- -D warnings`: clean (parent rerun).
- `cargo fmt --check`: passed.
- Schema follow-up (parent rerun): `cargo test -p ajax-web -- adapters::web_session_acp` 70 passed; `cargo clippy -p ajax-web --all-targets -- -D warnings` clean; `cargo fmt --check` passed.
- Cursor mode apply regression: fake `--cursor-mode` advertises `category:mode` with
  `currentValue: default` and option `agent`; spawn asserts
  `model:session/set_config_option:mode:agent` echo. `cargo test -p ajax-web -- adapters::web_session_acp` 71 passed.

## Checklist

- [x] Task 1: failing auto-approve adapter regression.
- [x] Task 2: auto-respond on receive; prefer AllowAlways then AllowOnce.
- [x] Task 3: do not emit unanswered `permission_request` for auto-answered ids.
- [x] Task 4: update `web-cockpit.md` and `web-session-behavior.md`.
- [x] Task 5: focused ajax-web verification.
- [x] Task 6: apply advertised full-access mode values for any harness; lock
  permission response JSON to ACP selected/cancelled schema.

## Approval

Approved by the user on 2026-08-20: all ACP Ajax chats should be full access;
implement until the checklist is done.
