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

- [ ] **Task 4 — Correct keyboard-open fit policy and cleanup (5-15 min)**
  - Test/write: add black-box coverage proving fullscreen enter while
    `keyboard-open` produces one fresh discrete resize, plus focused component
    coverage proving ordinary keyboard viewport bursts do not fit locally and
    scheduled frames do not run after disposal. Show RED first.
  - Implementation: freeze ordinary keyboard-open fit/resize, add an explicit
    discrete-intent override for pinch-end and expand-enter, and track/cancel all
    post-layout frames.
  - Verify: new tests, existing keyboard/fullscreen/viewport cases, and
    `npm run web:check`.

- [ ] **Task 5 — Restore seeded reconnect semantics (5-15 min)**
  - Test/write: add the black-box case that scrolls away, performs a manual
    seeded reconnect, and proves the surface restores live follow at the bottom;
    show it fails first.
  - Implementation: consume `(isReconnect, seeded)`; reset xterm and follow UI
    only for seeded reconnects, while retaining the local buffer on unseeded
    reconnects.
  - Verify: new case, existing reconnect/input cases, and connection unit tests.

- [ ] **Task 6 — Restore terminal paste semantics (5-15 min)**
  - Test/write: add a black-box bracketed-paste case and a clipboard-unavailable
    fallback case; show raw paste/no fallback failures first.
  - Implementation: route successful paste through `term.paste` and expose the
    smallest native textarea/notice fallback for unavailable or denied clipboard
    access.
  - Verify: new cases, existing Unicode/Paste transition cases, and
    `npm run web:check`.

- [ ] **Task 7 — Fix focus behavior and visual token (5-15 min)**
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
- PR 510 merged as `6bbef9c` on 2026-07-15. Post-merge cleanup twice removed an
  uncommitted branch that was fully reachable from `main`; the pre-delegation
  archive restored every implementation file byte-for-byte. The working branch
  is temporarily anchored at unsquashed PR head `04cd1e8` so cleanup cannot
  delete it, and only later implementation commits will be replayed onto main.

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
