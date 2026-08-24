# Behavioral regression suite refactor

## Scope

Organize Ajax regression coverage around observable operator behavior and
durable Core, CLI, Cockpit, ACP, tmux, persistence, and external-evidence
contracts. Preserve product behavior and keep architecture-invariant tests
separate from product regressions.

## Non-goals

- Production redesign or new public behavior.
- A generic test framework, giant snapshots, or live GitHub/agent dependencies.
- Repeating the complete Core suite at every presentation layer.

## Approval

Approved by the user on 2026-08-24. Parent-local implementation bypass is also
approved after two rejected delegate attempts.

## Checklist

- [x] Task 1 — start and interrupted provisioning
- [x] Task 2 — resume, review, repair, ship, and invalid-action contracts
- [x] Task 3 — drop, tidy, and interrupted cleanup
- [x] Task 4 — runtime reconciliation and canonical status transitions
- [x] Task 5 — GitHub/CI evidence contracts
- [x] Task 6 — stable CLI JSON, human-error, selection, and exit contracts
- [x] Task 7 — cross-surface semantic consistency
- [x] Task 8 — Native and Web Cockpit behavioral organization
- [x] Task 9 — Ajax Chat/ACP behavioral contracts
- [x] Task 10 — terminal/tmux boundary contracts
- [x] Task 11 — critical browser journey against server truth

## Task 1 result

- Preserved the existing first-command provisioning-failure regression.
- Added a distinct partial-provisioning scenario that proves completed receipts
  survive while the task becomes discoverable in `Error` with recovery metadata.
- Strengthened the successful-start test around discoverable `Active` state.
- Extended the CLI smoke journey through durable state, Repair, Resume, and
  post-recovery discoverability using public JSON.

TDD red: `interrupted_start_preserves_completed_steps_for_recovery` temporarily
expected `Active`; Nextest failed with actual `Error`. The documented expectation
was restored before green verification.

## Validation log

- Task 1: Core partial/success/start tests and CLI interrupted-start smoke pass.
- Task 2 red: Resume temporarily expected `Merged`; Nextest failed with actual
  `Reviewable`. Behavior-named Resume, Review, Repair, and Ship contracts pass.
- Task 3 red: confirmed-absence Drop temporarily expected the task to remain;
  Nextest failed because Ajax correctly removed it. Drop/Tidy behavior and
  separately named adapter contracts pass.
- Task 4 red: dead tmux evidence temporarily expected `Idle`; Nextest failed
  with canonical `Error`. Reconciliation tests now name operator outcomes, and
  one duplicate Waiting assertion was removed.
- Task 5 red: failed CI temporarily expected `Waiting`; Nextest failed with
  canonical `Error`. CI projection tests are behavior-named; cadence/transport
  checks are explicitly adapter contracts.
- Task 6 red: missing title temporarily expected exit 0; the live binary returned
  the stable error exit 1. JSON assertions now select semantic contract fields.
- Task 7 red: Native Cockpit temporarily expected missing substrate to project
  `Waiting`; it projected canonical `Error`. Native and Web now also assert the
  Core-owned attention projection.
- Task 8 red: the committed Web fixture temporarily expected `running`; the
  server contract projected `waiting`. Whole-object fixture comparisons now
  select stable operator fields; Native Cockpit behavior is covered by the
  thin CLI surface contract without restructuring its legacy test file.
- Task 9 red: queued ACP prompts temporarily expected reverse order; the adapter
  produced FIFO order. Session tests now name queue, replay, cancellation,
  capability, permission, transcript, and reconnect behavior.
- Task 10 red: a reconnectable browser terminal temporarily expected teardown
  commands; the boundary correctly preserves its tmux session. Exact tmux
  command checks are labeled adapter contracts.
- Task 11 red: the browser journey temporarily expected `Agent sleeping`; both
  browser engines projected the authoritative `Agent working` state. The final
  journey covers list, selection, workspace, action, server result, and reload.

## Audit classification

- Behavioral: retained and behavior-named lifecycle, reconciliation, CI, CLI,
  Cockpit, ACP, terminal, and browser journeys.
- Architecture invariant: retained separately and verified with
  `npm run verify:arch`; exact Git/tmux command shapes are explicitly labeled
  adapter contracts rather than product journeys.
- Implementation-coupled: replaced internal update-count, module-exposure,
  generated ACP item-id, full-JSON, and dispatch-module assertions with the
  nearest observable or boundary contract.
- Duplicate: removed the second identical Core `NeedsInput -> Waiting` test.
- Low-value: replaced internal update-count and whole-object fixture assertions;
  retained operator-visible assertions in stronger scenarios.

## Final verification

- `cargo nextest run -p ajax-core`: 927 passed.
- `cargo nextest run -p ajax-cli`: 376 passed.
- `cargo nextest run -p ajax-tui`: 202 passed.
- `cargo nextest run -p ajax-web`: 585 passed.
- `npm run web:check`: passed.
- `npm run web:lint`: passed.
- `npm run web:test -- --run`: 1,317 passed, 9 skipped; jsdom emitted its
  existing canvas-not-implemented diagnostics without failing the command.
- `npm run verify:arch`: 34 architecture tests passed.
- Focused Playwright operator journey: passed in desktop Chromium and mobile
  WebKit.
- Focused mobile WebKit Chat, Terminal reconnect, Diff Review, and authoritative
  reload journeys: 4 passed.
- `cargo fmt --check` and `git diff --check`: passed.

## Deviations and remaining risk

- No production behavior changed. Browser journeys control server boundaries;
  live GitHub, live agent, and destructive live tmux cleanup remain covered at
  adapter/domain layers rather than invoked from browser tests.
