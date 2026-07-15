# PR 510 repair and xterm implementation split

## Scope

- Make PR 510 honest and green as a behavior-contract/removal PR without adding
  the replacement terminal to it.
- Preserve the current uncommitted xterm work on a separate stacked branch and
  open a separate implementation PR.
- Fix the automated correctness findings in
  `.planning/agent-plans/xterm-implementation-review.md` with TDD before the
  implementation PR is opened.
- Keep the Rust PTY/backend, authentication, task lifecycle, registry truth,
  and public API unchanged.

## Non-goals

- Do not restore Ghostty, Surface V2, or deleted legacy helper layers.
- Do not edit anything under `tests/`, weaken assertions, or hide unrelated
  failures.
- Do not merge the implementation PR. PR 510 merged as `6bbef9c`; the new
  implementation PR targets `main` directly.
- Do not claim the physical-iPhone checklist passed without a real-device run.

## Approval and delegation

- Approval status: **approved 2026-07-15**. The user's instruction to delegate
  until finished supersedes per-task continuation pauses.
- Delegation decision: **delegated via model-router**. Continue the existing
  Cursor implementation lane with one bounded TDD packet per review fix; the
  parent reviews every diff and runs validation independently.
- PR 510 exception requiring explicit approval: because PR 510 intentionally
  removes the terminal, its 27 runtime assertions cannot pass there. Mark that
  file as an explicit Playwright expected failure in PR 510, so every case still
  executes and an unexpected pass fails CI. Remove the annotation in the
  implementation PR. This changes expected status temporarily but does not
  skip a test or change an assertion.

## Task checklist

- [x] **Task 1 — Preserve the implementation in a dedicated worktree (5-15 min)**
  - Test/write: no behavior test; this is mechanical Git isolation. Capture the
    current tracked/untracked implementation manifest and hashes first.
  - Implementation: create sibling worktree/branch
    `ajax/xterm-implementation` from PR 510's head, transfer only the current
    implementation/docs/generated-assets/plans, and byte-verify the transfer.
  - Verify: compare file manifests and hashes; confirm the new worktree contains
    the implementation diff and the PR 510 worktree is restored to its original
    head before its focused fix.

- [x] **Task 2 — Make PR 510's intentionally red contract explicit (5-15 min)**
  - Test/write: use the existing failing Web CI and focused 27-case run as RED;
    failure is the absent `task-terminal-panel`, not a product regression.
  - Implementation: add one temporary, file-scoped Playwright expected-failure
    annotation to `e2e/terminal-behavior.test.ts`; do not skip cases or modify
    assertions. Update the PR body verification wording if its counts change.
  - Verify: run the focused file and confirm all 27 execute as expected failures
    with exit 0; run full mobile-WebKit smoke and `npm run verify`; commit and
    push only the PR 510 repair, then wait for all PR checks.

- [x] **Task 3 — Align xterm's logical grid with the PTY (5-15 min)**
  - Test/write: add a focused component test proving a narrow FitAddon proposal
    makes xterm itself resize to at least 80 columns and the exact same pair is
    sent to the connection. Run it and show the current mismatch as RED.
  - Implementation: keep one concrete fit path; resize the logical xterm grid to
    the 80-column floor and scale it to the host width without restoring deleted
    geometry abstractions.
  - Verify: focused component test, resize-related Playwright cases, and
    `npm run web:check`.

- [x] **Task 4 — Correct keyboard-open fit policy and cleanup (5-15 min)**
  - Test/write: add black-box coverage proving fullscreen enter while
    `keyboard-open` produces one fresh discrete resize, plus focused component
    coverage proving ordinary keyboard viewport bursts do not fit locally and
    scheduled frames do not run after disposal. Show RED first.
  - Implementation: freeze ordinary keyboard-open fit/resize, add an explicit
    discrete-intent override for pinch-end and expand-enter, and track/cancel all
    post-layout frames.
  - Verify: new tests, existing keyboard/fullscreen/viewport cases, and
    `npm run web:check`.

- [x] **Task 5 — Restore seeded reconnect semantics (5-15 min)**
  - Test/write: add the black-box case that scrolls away, performs a manual
    seeded reconnect, and proves the surface restores live follow at the bottom;
    show it fails first.
  - Implementation: consume `(isReconnect, seeded)`; reset xterm and follow UI
    only for seeded reconnects, while retaining the local buffer on unseeded
    reconnects.
  - Verify: new case, existing reconnect/input cases, and connection unit tests.

- [x] **Task 6 — Restore terminal paste semantics (5-15 min)**
  - Test/write: add a black-box bracketed-paste case and a clipboard-unavailable
    fallback case; show raw paste/no fallback failures first.
  - Implementation: route successful paste through `term.paste` and expose the
    smallest native textarea/notice fallback for unavailable or denied clipboard
    access.
  - Verify: new cases, existing Unicode/Paste transition cases, and
    `npm run web:check`.

- [x] **Task 7 — Fix focus behavior and visual token (5-15 min)**
  - Test/write: add focused interaction coverage proving a toolbar click
    preserves terminal focus only when already owned and fullscreen exit blurs;
    show current unconditional refocus as RED. The token replacement is
    mechanical and needs no new test.
  - Implementation: use pointer focus prevention, conditional refocus,
    `preventScroll` where supported, blur on fullscreen exit, and replace
    `--surface-raised` with `--paper-raised`.
  - Verify: focused interaction cases, `npm run web:check`, and
    `git diff --check`.

- [ ] **Task 8 — Activate the permanent suite and validate the stacked PR (5-15 min plus gate runtime)**
  - Test/write: remove PR 510's expected-failure annotation; the implementation
    must make all 27 original cases plus the new review-regression cases pass.
  - Implementation: regenerate tracked `dist/app.js`/`dist/app.css`, update the
    implementation plan ledger and review status, and make no unrelated edits.
  - Verify: focused new cases; all 27 original mobile-WebKit cases; full
    mobile-WebKit smoke; full Vitest; `npm run web:check`;
    `npm run web:build:check`; `npm run verify`; Husky installation/hook gate;
    `cargo build --release -p ajax-cli`; and
    `cargo install --path crates/ajax-cli --locked --force`.

- [x] **Task 8a — Keep desktop task details reachable (5-15 min)**
  - Test/write: use the existing Copy-buttons action case as RED; it consistently
    times out because the xterm mount leaves `.meta-details summary` outside the
    viewport and route-scroll cannot expose it. Do not edit that test.
  - Implementation: diagnose terminal and route-scroll dimensions, then make
    the smallest terminal-local layout correction so normal desktop terminal
    height is bounded and following task metadata remains reachable. Preserve
    the phone 38vh rule, fullscreen, and the single route-scroll owner.
  - Verify: focused action case, full terminal behavior file, layout-scroll
    file, `npm run web:check`, and `git diff --check`.

- [x] **Task 8b — Preserve PR 510 paste assertions (5-15 min)**
  - Test/write: restore the two merged LF/Unicode payload assertions exactly and
    show xterm's default CR normalization as RED; do not modify other existing
    assertions.
  - Implementation: preserve the original text byte-for-byte and use public
    `term.modes.bracketedPasteMode` to add DEC wrappers when active, retaining
    the accepted focus-ownership behavior and native fallback.
  - Verify: original/new paste cases, full terminal file, `npm run web:check`,
    and `git diff --check`.

- [x] **Task 8c — Retain paste text across disconnect failure (5-15 min)**
  - Test/write: disconnect after typing fallback text and before a primary
    clipboard paste; prove exact text remains visible, no PTY frame is added,
    and a reconnect/unavailable notice is shown.
  - Implementation: return send success, clear fallback only on success, and
    retain/prefill exact unsent content on failure without hidden queuing.
  - Verify: new and existing paste/focus cases, full terminal file, web check,
    and diff check.

- [x] **Task 8d — Isolate expanded terminal from background controls (5-15 min)**
  - Test/write: expand on phone and prove representative cockpit/task/nav
    controls cannot receive focus or act; exit and prove restoration.
  - Implementation: apply native `inert` only to terminal siblings/shell chrome
    owned by the expanded state, restoring it on exit/unmount.
  - Verify: new/fullscreen/focus cases, full terminal file, web/diff checks.

- [x] **Task 8e — Prove exact keyboard-open discrete resizes (5-15 min)**
  - Test/write: require exactly one settled resize for expand-enter and add the
    mirrored keyboard-open pinch-end case before any production edit.
  - Implementation: only if RED, minimally dedupe discrete scheduling.
  - Verify: resize/pinch/keyboard group, full terminal file, web/diff checks.

- [x] **Task 8f — Make clipboard-unavailable fixtures deterministic (5-15 min)**
  - Test/write: no new test; this is mechanical test reliability. Replace
    competing init scripts with one mock option and retain all assertions.
  - Implementation: test helper/call sites only; no production change.
  - Verify: clipboard/paste cases three times, full terminal file, checks.

- [x] **Task 8g — Keep large UTF-8 paste frames within the PTY limit (5-15 min)**
  - Test/write: add connection coverage proving a payload larger than 4 KiB is
    split into binary frames no larger than the backend limit and reconstructs
    byte-for-byte across a multibyte boundary; show the current one-frame send
    as RED.
  - Implementation: chunk the already encoded input at the connection boundary;
    keep small input as one frame and do not change the Rust PTY contract.
  - Verify: focused connection tests, full connection unit file, web check, and
    diff check.

- [x] **Task 8h — Do not reopen the keyboard from terminal action clicks (5-15 min)**
  - Test/write: prove a bubbled click from `New output` does not refocus the
    terminal or reopen keyboard ownership, while a direct surface click still
    focuses without scrolling; show RED first.
  - Implementation: ignore button-originated wrapper clicks and reuse the
    existing focus path with `preventScroll`; no new focus abstraction.
  - Verify: focused focus/scroll cases, full terminal file, web/diff checks.

- [ ] **Task 9 — Commit, push, open, and monitor the implementation PR (5-15 min plus CI runtime)**
  - Test/write: no new test; this is PR lifecycle work after the blocking local
    gate is green.
  - Implementation: commit through hooks, replay implementation commits onto
    `origin/main`, push `feat/web-xterm-terminal`, open a PR with base `main`,
    link merged PR 510, summarize review fixes and physical-device limits, and
    do not merge.
  - Verify: inspect the final PR diff, confirm the terminal suite is active (not
    expected/ignored), wait for all checks, and send focused delegated revisions
    for any implementation-owned failure.

## Validation ledger

- PR 510 GitHub `Web` — exit 1: 27 terminal behavior cases fail because the
  intentionally removed `task-terminal-panel` is absent; aggregate `CI` fails
  only because Web fails.
- Existing uncommitted implementation: original 27 mobile-WebKit cases — exit
  0; full Vitest — exit 0 (245); `npm run web:check` — exit 0;
  `npm run web:build:check` — exit 0; `npm run verify` — exit 0.
- Review status — changes requested; see
  `.planning/agent-plans/xterm-implementation-review.md`.
- Geometry review fix: RED `logical xterm grid` case failed because screen
  width equaled the host; GREEN focused case passed, the seven-case resize group
  passed, `npm run web:check` passed, and `git diff --check` passed. After two
  Cursor rounds left one existing resize failure, the parent used public xterm
  DOM cell measurements to remove proposal-rounding loss; no private API or new
  helper remains.
- Keyboard/cleanup review fix: the two new cases failed before implementation
  (keyboard-open expand emitted no resize; nested post-layout work survived
  navigation), then passed. The five-case keyboard/fullscreen/pinch group and
  `npm run web:check` passed.
- Seeded reconnect review fix: the new case failed with stale `New output` UI,
  then passed after consuming the connection metadata. The four-case reconnect,
  input, and scroll-follow group plus `npm run web:check` passed.
- Paste review fix: bracketed and fallback cases failed on raw direct send and
  missing UI, then passed through public `term.paste` and a native textarea.
  The four-case paste group plus `npm run web:check` passed.
- Focus/style final revision: seven focused cases and a nine-case broader group
  passed; conditional New output/Reconnect/Send/Cancel controls render at
  44x44, fallback focus ownership is preserved, and independent review approved.
- Desktop layout final revision: the normal and expanded terminal are bounded;
  focused Copy-buttons and desktop-expand cases, 41 terminal cases, four
  layout-scroll cases, and web check passed. The observed RED expanded height
  was 784,322px versus a 466px cap.
- Paste contract/disconnect final revision: original LF/Unicode assertions are
  restored unchanged; nine paste/fallback/focus cases and 43 terminal cases
  passed, with exact unsent text retained on socket failure.
- Fullscreen isolation: native owned `inert` state now covers task-detail
  siblings, cockpit chrome, bottom nav, and any existing Result panel. The
  focused case and six-case fullscreen/focus group passed; exit/unmount restore
  only state owned by the terminal.
- Exact resize intent: strengthened expand and new keyboard-open pinch cases
  passed without production changes; six resize cases and 45 terminal cases
  passed.
- Deterministic clipboard fixture: nine clipboard/paste/fallback cases passed
  in three consecutive runs; all 45 terminal cases and web check passed.
- Focus/style review fix, first delegated pass: rejected after independent
  review because its touch-target assertion covered only height, captured focus
  ownership could leak into keyboard activation, fullscreen-entry focus was not
  proven, and Paste still refocused unconditionally. A focused revision packet
  is active; Task 7 remains incomplete.
- Focus/style second review: stale keyboard ownership and fullscreen entry/exit
  proof are resolved. Task 7 remains open because fallback Send/Cancel still
  refocus unconditionally and the 44×44 test does not render/measure conditional
  New output, Reconnect, Send, or Cancel controls.
- Full Mobile WebKit integration run after that first pass: 62 passed, one
  existing visual skip, and the desktop Copy-buttons action failed because
  Task details could not be scrolled into view. A focused rerun reproduced the
  same timeout; Task 8a tracks the implementation-owned layout regression.
- Task 8a first pass made normal desktop Task details reachable (focused action,
  38 terminal cases, four layout-scroll cases, and web check green) but review
  rejected its `.not(.is-expanded)` gap: desktop expand dropped the only height
  cap. A desktop-expand RED/GREEN revision is required before acceptance.
- Acceptance diff review found two original PR 510 paste expectations had been
  rewritten from LF to CR to match xterm's default normalization. Task 8b
  restores those merged assertions before changing implementation; the new PR
  must pass the behavior contract rather than edit around it.
- Task 8b restored the two merged assertions and passed seven focused paste
  cases plus all 41 terminal cases. Paste review then found a closed-socket
  loss path: fallback state was cleared before the no-op send. Task 8c tracks
  explicit retention/notice behavior for both primary and fallback paste.
- Final test-gap review confirmed all 27 merged PR 510 cases/assertions remain
  active and unchanged, then found expanded background controls remained
  interactive, clipboard-unavailable setup used unordered init scripts, the
  “one resize” test did not count its slice, and keyboard-open pinch-end lacked
  coverage. Tasks 8d–8f track the actionable findings. Existing xterm DOM
  geometry/focus assertions are retained because the repo rules forbid weakening
  assertions; their upgrade-coupling is a documented follow-up risk.
- Task 8d's first focused inert case passed after removing orphaned Playwright
  workers and using direct native-inert assertions. Review found one remaining
  background sibling: a pre-existing Result panel/Dismiss button. The fullscreen
  packet now requires that state before Task 8d can close.
- Task 8e reopened after the full mobile-WebKit run exposed the new
  keyboard-open pinch-end exact-one-resize case as nondeterministic. The
  original 27 PR 510 cases still passed in that run; the focused production
  fix must be green before the PR gate can be rerun.
- PR 510 merged as `6bbef9c` on 2026-07-15. Post-merge cleanup twice removed an
  uncommitted branch that was fully reachable from `main`; the pre-delegation
  archive restored every implementation file byte-for-byte. The working branch
  is temporarily anchored at unsquashed PR head `04cd1e8` so cleanup cannot
  delete it, and only later implementation commits will be replayed onto main.
- Draft implementation PR 512 opened from `feat/web-xterm-terminal` after the
  normal commit hook passed `npm run verify`, release build, and locked local
  install. The user explicitly approved opening before the failed mobile-WebKit
  pinch case was rerun; the PR remains draft while Tasks 8e, 8g, and 8h close.
- Task 8g RED sent one oversized input frame. GREEN chunks encoded input into
  at most 4096-byte binary frames; the parent reran all 18 connection tests
  successfully, and delegate web/diff checks passed.
- Task 8h RED proved a bubbled New-output click refocused xterm. GREEN ignores
  button-originated wrapper clicks and uses `preventScroll` for direct-surface
  focus; the delegate ran all 46 terminal cases and the parent reran the new
  case successfully. One out-of-scope delegate plan was rejected and removed
  in the allowed revision round.
- PR 512's first Web run failed only the original `reopen with meaningful
  viewport change` case on all three CI attempts; the other 71 cases passed
  with one existing visual skip. The exact case then passed 10/10 locally with
  `CI=1`; focused read-only root-cause review remains in progress before any
  production change.
- After deferring keyboard-open pinch fitting to pinch end, the complete local
  mobile-WebKit run passed 73 tests with one existing visual skip. All 46
  terminal behavior cases passed, including all 27 original PR 510 cases and
  the previously failing CI reopen case. The reopened case also passed 10/10
  under `CI=1`. Web build, deterministic build check, type/Svelte check, and
  diff check passed.

## Risks and stop conditions

- The expected-failure annotation is temporary and must not reach the
  implementation PR's final diff. An active annotation is a hard stop.
- PR 510 must remain behavior/removal-only; implementation files in its diff are
  a hard stop.
- Physical iPhone selection/copy, horizontal touch ownership, keyboard chrome,
  and native paste UI require the documented real-device checklist. Automated
  green status does not prove them.
- Any task lifecycle, registry, terminal backend, authentication, or public
  network change is out of scope and requires a new plan.
