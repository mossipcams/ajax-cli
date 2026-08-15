# Web Chat Harness Smoke Fixes

## Scope

- Route newly created ACP-capable tasks directly to orchestration chat.
- Preserve replayed chat history and host-reported busy state across reconnect.
- Classify Cursor and Pi live failures as Ajax defects or provider/environment failures.
- Re-run focused, full web, browser, and live harness verification.
- Keep the four-harness, three-turn browser flow as deterministic regression coverage.
- Select known advertised full-access ACP config options for trusted local sessions.
- Expose when an in-flight ACP turn has produced no recent activity (#889).

## Non-goals

- Bypassing provider subscription or authentication requirements.
- Weakening TLS or test assertions.
- Changing lifecycle, registry, or runtime authority boundaries.

## Approval

- Approved by the user on 2026-08-15: implement until finished.
- Regression extension approved by the user on 2026-08-15: implement until finished.
- Config-options-only ACP alignment approved by the user on 2026-08-15:
  implement until all tasks are done, with no legacy compatibility.

## Checklist

- [x] Task 1: failing post-create routing regression, minimal fix, focused pass (#883).
- [x] Task 2: failing replay/ready regressions, minimal fix, focused pass (#881).
- [x] Task 3: classify Cursor and Pi failures with live evidence.
- [x] Task 4: surface silent ACP error stop reasons with a TDD fix (#884).
- [x] Task 4a: synchronize late repository data in the task sheet (#885).
- [x] Task 4b: classify Cursor Auto as an upstream ACP/account limitation; no Ajax change.
- [x] Task 4c: classify Pi startup inventory as a note using adapter metadata (#887).
- [x] Task 5: full verification, live smoke matrix, and cleanup.
- [x] Task 6: add a failing four-harness, three-turn browser regression and its
  minimal deterministic session transport.
- [x] Task 7: cover Markdown, overflow, replay-before-ready, reconnect, reload,
  and error-stop behavior in the browser regression.
- [x] Task 8: run focused and full verification and record results.
- [x] Task 9: add failing exact-mode regressions, select advertised Codex and
  Claude full-access modes, and preserve manual permission fallback.
- [x] Task 10: add a failing activity-freshness regression and show honest
  elapsed inactivity without changing task truth (#889).
- [x] Task 11: update the owning architecture contracts and defect tracking.
- [x] Task 12: run focused and broad verification, rebuild assets, and publish
  the scoped PR.
- [x] Task 13: add a failing config-option policy regression, switch trusted
  full access to `session/set_config_option`, and remove legacy mode handling.
- [x] Task 14: add a failing Settings regression and disclose full tool access
  without approval prompts on the orchestration-chat toggle.
- [x] Task 15: update architecture, rebuild assets, run focused and broad
  verification, and update PR #890 through CI.

## Validation

- `cargo test -p ajax-web`: 370 passed.
- `npm run web:test -- --run`: 846 passed, 9 skipped.
- `npm run web:smoke`: 121 passed, 3 skipped.
- `session-chat-regression.test.ts`: 5 passed in Mobile WebKit and 5 passed in
  Desktop Chromium. Cursor, Codex, Claude, and Pi each create a task, complete
  three turns, render Markdown without overflow, and preserve three user/agent
  pairs across navigation and reload. The fifth case verifies an error turn
  settles with recovery guidance.
- `npm run web:check`, `npm run web:lint`, `cargo fmt --check`, and
  `git diff --check`: passed.
- Live Chromium mobile smoke, three prompts per harness:
  - Codex: passed creation routing, chat replies, Markdown, continuity,
    reconnect, and reload.
  - Claude: passed creation routing, chat replies, Markdown, continuity,
    reconnect, and reload.
  - Pi: passed creation routing, chat replies, Markdown, continuity,
    reconnect, and reload after updating local `pi-acp` to 0.0.33.
  - Cursor: task/session creation reaches Cursor ACP, but prompts return its
    account upgrade response; explicitly passing `--model auto` closes the ACP
    transport during `session/new`, so no Ajax code change was retained.
- Smoke tasks were dropped after testing and `/api/health` returned `{"ok":true}`.
- `trusted_permission_mode_must_be_exact_and_advertised`: passed.
- `fake_permission_request_returns_a_selected_acp_outcome`: passed unchanged.
- `SessionChat.test.tsx`: 8 passed, including one-minute inactivity, event
  reset, and turn-end cleanup.
- Final `cargo test -p ajax-web`: 371 passed.
- Final `npm run web:test -- --run`: 847 passed, 9 skipped; jsdom emitted its
  known non-failing xterm canvas warning.
- Final `npm run web:smoke`: 121 passed, 3 skipped.
- Impeccable detector, `npm run web:check`, `npm run web:lint`,
  `npm run web:build:check`, `cargo fmt --check`, and `git diff --check`: passed.
- Known non-product validation limits: direct WebKit live testing rejected the
  local self-signed certificate; the mock WebKit smoke suite passed. The optional
  Cursor live resume test failed its upstream `session/load` resume assertion.
- Config-options-only follow-up: `trusted_permission_config_must_be_exact_and_advertised`
  and the unchanged manual-permission fallback regression passed; `cargo test -p
  ajax-web` passed 371 tests on rerun after one known environment-variable race
  between the adjacent Test-in-Stable endpoint tests caused an initial 200/404
  mismatch. `cargo nextest run --all-features --test-threads=1` passed 1,984 tests.
- Config-options-only follow-up: `npm run web:test -- --run` passed 848 tests
  with 9 skipped; `npm run web:smoke` passed 121 with 3 skipped. Clippy, Cargo
  check/doc tests, TypeScript, ESLint, ast-grep, CI workflow checks, deterministic
  production build, formatting, diff checks, and the Impeccable detector passed.

## Deviations and assumptions

- The user waived the repository playbook's per-task continuation pauses by explicitly
  directing implementation to continue until finished.
- Cursor Auto was initially treated as a possible Ajax argument-mapping defect. Live
  ACP validation disproved that hypothesis, so the provisional code and test were
  reverted and issue #886 was closed with the corrected evidence.
- The installed Pi ACP adapter was updated from 0.0.32 to 0.0.33 because 0.0.33 fixes
  session settlement; no dependency or installation abstraction was added to Ajax.
- The new reload regression exposed a disposed pre-open WebSocket dispatching a
  false connection failure. It is tracked as #888 and fixed by ignoring the open
  rejection after transport disposal.
