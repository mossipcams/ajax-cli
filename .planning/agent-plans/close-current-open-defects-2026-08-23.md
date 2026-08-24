# Address current open defects — 2026-08-23

**Approval:** approved by user on 2026-08-24; Tasks 1–8 accepted on 2026-08-24
**Tracking:** GitHub issues on `mossipcams/ajax-cli`; every confirmed fix references its issue with `Fixes #N`.
**Mode:** bounded Web Cockpit/session behavior fixes; no lifecycle or registry redesign.

## Scope

Address the confirmed current defects:

- Chat transcript projection: #1042, #1043, #1044, #1045, #1046, #1047
- Session reconnect presentation: #1039, #1040
- Cursor model-intent matching: #1013

Revalidate #925 against current `main` before implementation. Its report is from
an older UI path and includes `ajax-cli: command not found`; implement only if
the contradiction still reproduces with current task-detail/runtime evidence.

## Non-goals

- #1038: obsolete against the current swipe/detail action design.
- #1010: its specific `reasoning` selection regression is covered by current
  `effort` preference logic; retain #1013 as the broader Cursor contract gap.
- No task lifecycle, registry truth, runtime authority, terminal-model, public
  network, or authentication redesign.
- No new dependencies, abstractions, or test harnesses.
- No commits, pushes, issue closure, or PR creation until the user requests
  shipping.

## Execution plan

Each task is executed TDD-style: add the focused failing regression test,
run it and show the failure, make the smallest implementation, rerun and show
the pass, then pause for confirmation before the next task.

### Task 1 — Preserve transcript event order (#1042)

- **Test:** extend `features/chat/conversation/groupTurns.test.ts` with an
  interleaved prose/tool/prose sequence and assert the returned rows preserve
  arrival order, including separate prose messages.
- **Code:** replace the bucketed `work`/`agents` rendering contract with the
  smallest ordered turn representation; keep the activity disclosure as the
  presentation boundary.
- **Verify:** focused `groupTurns` and `Conversation` tests; `npm run web:check`.

### Task 2 — Reveal completed agent prose (#1043)

- **Test:** extend `Conversation.test.tsx` with a one-paragraph agent message
  followed by a tool/permission event while the turn remains busy; assert the
  completed message is visible before `turn_end`.
- **Code:** apply live paragraph trimming only to the currently streaming
  agent row, not to an earlier completed row; preserve fenced-code handling.
- **Verify:** focused conversation and reveal tests; `npm run web:check`.

### Task 3 — Settle unfinished tools on turn end (#1044)

- **Test:** add a reducer regression with an in-progress tool followed by
  `turn_end`; assert no tool remains active and the resulting status is the
  documented cancelled/failed terminal state.
- **Code:** handle turn-end settlement in the shared session projection and
  extend the typed tool status only as required by the existing contract.
- **Verify:** focused reducer tests; `npm run web:check`.

### Task 4 — Avoid duplicate and false turn errors (#1045)

- **Test:** add reducer cases for (a) a typed error before `turn_end`, and (b)
  an agent response before an error stop; assert exactly one truthful error note
  and no “without a response” note in either case.
- **Code:** gate the generic `turn_end` fallback on the turn having produced
  neither response nor error evidence.
- **Verify:** focused reducer tests; `npm run web:check`.

### Task 5 — Normalize permission titles at the projection boundary (#1046)

- **Test:** add a permission-panel/reducer regression with a Markdown-delimited
  title; assert the approval control and transcript marker render the same
  cleaned title.
- **Code:** reuse the existing title-cleaning helper at the permission
  projection boundary so all consumers receive one normalized value.
- **Verify:** focused permission and reducer tests; `npm run web:check`.

### Task 6 — Scope plans to their producing turn (#1047)

- **Test:** add a reducer regression with two prompts and two plan updates;
  assert each turn owns its own plan and the first turn does not change.
- **Code:** replace the session-wide first-plan lookup with turn-scoped plan
  ownership using the existing conversation ordering/state; do not add a second
  browser registry.
- **Verify:** focused reducer/activity tests; `npm run web:check`.

### Task 7 — Make reconnect state mutually exclusive and deduplicated (#1039, #1040)

- **Test:** extend connection/head tests to assert a disconnected idle session
  cannot render both Ready and Reconnecting; add a reconnect retry case that
  emits one identical ACP auth error plus one connection-loss note.
- **Code:** make connection state authoritative for the head label and
  deduplicate identical retry errors at the existing projection/transport
  boundary. Preserve reconnect, replay-cursor, and composer behavior.
- **Verify:** focused connection, reducer, and LiveHead tests; `npm run web:check`.

### Task 8 — Complete Cursor intent matching (#1013)

- **Test:** add focused core/web mapper regressions for thinking suffix versus
  `thinking=true`, split-axis effort/Fast, exploded catalog ids, non-thinking
  siblings, and advertised-token verification. Include the reported Low and
  Thinking Medium cases without model-specific branches.
- **Code:** generalize the existing Cursor intent parser/matcher and map pins
  through the advertised split or exploded contract described in
  `docs/architecture/web-session-behavior.md`; preserve non-Cursor option
  mapping and exact advertised-value validation.
- **Verify:** focused `ajax-core` and `ajax-web` model tests, then the relevant
  `cargo nextest` packages and `npm run web:check`.

### Task 9 — Revalidate #925 and conditionally fix

- **Test:** reproduce the current task-detail payload with `status: running`,
  `agent_status: NotStarted`, and a failed runtime command; add a regression
  only if the current UI still contradicts the authoritative projection.
- **Code:** if confirmed, fix the owning projection boundary rather than adding
  a browser-only status heuristic. Stop and request a new architecture decision
  if this requires changing Core/runtime status authority.
- **Verify:** focused task workspace/API projection tests and the strongest
  relevant Core/Web checks.

## Final verification

- Run all focused tests for the changed slices.
- Run `npm run web:check`, `npm run web:lint`, and the relevant Rust package
  checks; run the full repository gate if practical.
- Inspect `git diff` and `git status`; report skipped/failed commands exactly.
- Review issue linkage and regression coverage before any future PR request.

## Checklist

- [x] User approval received
- [x] Restore the repository-mandated `scripts/run-delegate` and transaction wrappers
- [x] Task 1 complete
- [x] Task 2 complete
- [x] Task 3 complete
- [x] Task 4 complete
- [x] Task 5 complete
- [x] Task 6 complete
- [x] Task 7 complete
- [x] Task 8 complete
- [x] Task 9 revalidated — stale against current projection; no code change
- [x] Final verification complete — focused Web tests, web check/lint, full ajax-core, and web-session Rust suites pass; #925 remains no-op
