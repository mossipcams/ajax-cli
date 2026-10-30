# Training polish as the design system

Status: approved by explicit user request 2026-10-30 ("Update the design docs to
align with the polish of the training page. Then update the designs on the
dashboard and task pages").

## Scope

1. Rewrite `DESIGN.md` so the Training page's visual language is the system, and
   Chat is the work surface. Touch `PRODUCT.md` only where it conflicts.
2. Restyle the dashboard task list and the Task Workspace (shared task header,
   chat live head and chrome, task details) to match.

## Non-goals

- No change to task truth, status mapping, sort order, actions, or routing.
- No `styles/terminal.css` or terminal behavior change: terminal is frozen.
  Only the shared task header that sits above it changes.
- No tap-target changes (the 44px work in #448 was reverted; leave as is).
- No Training page redesign, beyond removing overrides that become global.
- No new dependencies, no new tokens unless a value recurs.

## Design decisions

Shape
- `--radius-sm` (6px) is the default for buttons, chips, rows, panels, inputs.
- `--radius` (10px) only for sheet top corners. Nothing else above 6px.
- Round only for status dots. No `999px` buttons or chips, no `20px` panels.
- Disabled controls use the Training treatment: transparent, Rule border,
  Ink Faint, full opacity.

Color
- One status palette: `--tone` / `--tone-bg` from `foundation.css`. Delete the
  `.tasks .tone-*` pastel overrides.
- Neutrals come from tokens only. No raw hex in surface CSS, no R!=G!=B grays.
- Blue is status only. The chat back control becomes neutral.

Type
- Task title: sans, `--text-body`, weight 600, on every screen.
- Status: dot in `--tone` plus a sentence-case label on every screen. The label
  is neutral, and takes `--tone` only for waiting and error. No glyph-and-uppercase
  variant.
- Back: one treatment on every screen: neutral, sans, not uppercase.
- Uppercase tracked micro labels only for eyebrows and stat labels.
- Mono only for code, commands, paths, hardware tags.
- Numbers and times use `font-variant-numeric: tabular-nums`.

Surfaces
- Panels: `--paper-tint`, 1px `--rule`, `--radius-sm`, no shadow.
- Section heading plus muted one-line context, as on Training.

Dashboard
- One bordered 6px panel with hairline row separators.
- A "Needs you" group heading above cards in core's `needs-you` and `review`
  attention bands, and an "Other tasks" heading above the rest when both groups
  exist. Grouping reads `card.attention` and keeps `sortCards` order inside each
  group. No new policy in the browser.
- Timestamps use `--ink-muted` (was 3.1:1 contrast).

## Tasks

- [x] S1 docs: `DESIGN.md` (`PRODUCT.md` had no conflicts); flag wording corrected in S2
- [x] S2 primitives: buttons and chips, disabled state, remove Training overrides
- [x] S3 dashboard: `TaskList.tsx`, `styles/task/list.css` (one `.task-list` declaration restored in S4)
- [x] S4 task header: `styles/task/detail.css`, `styles/chat/status.css` (CSS only)
- [x] S5 task details: `styles/task-workspace/sheets.css`, `styles/task/meta.css`
- [x] S6 radius sweep: chat, new-task sheet, diff review, test-in-dev, skeleton; remove `--radius-lg` (five packets, every hunk read by the parent)
- [x] Rebuild committed `dist` and run the full gate
- [x] Parent review of every delta

## Verification

Per slice: `npm run web:test`, `web:check`, `web:lint`, `web:sg`.
Final: `web:build`, then Rust asset tests that read `dist`.
Browser e2e cannot run in this sandbox: Chromium segfaults on launch and WebKit
is known not to run under nono. CI's Web job has to cover it.

## Deviations

- Terminal toolbar and overlay controls keep their pills because the terminal is frozen; `DESIGN.md` records the exception. `.task-list` keeps an invisible `var(--radius)` pinned by `TaskList.test.tsx`.
- The router flagged `dist/app.css` and `dist/app.js` as scope violations although `dist/` was allowed. Parent rebuilt and confirmed only those two generated files changed.
- `--radius-lg` and the 14px bottom nav were found during S1; nav joins S2, `--radius-lg` removal joins S6.
- The dashboard chrome keeps its fill for New; only its radius changes.
- The S2 delegate was killed when the session ended, before reporting. Its edits were complete; the parent closed the snapshot manually, confirmed scope, and ran the full gate.
- S3 was dispatched to Haiku, the first fallback, although the router selected Qwen for that slice: the dispatch script hardcoded the agent. Later slices copy the router's decision.
- Two spec corrections in S3: status text stays neutral except waiting and error (kept `TaskList.test.tsx` as is), and grouping reads core's `attention` band instead of tone.
- `TaskWorkspaceHeader.test.tsx:71` (#1020) pins a `◦` on `.interact-pill.tone-waiting::before`. The rule stays for that test, though the new dot makes it redundant. Retired with the user's approval.
- The S6 audit found pills in `new-task.css`, `diff-review.css` and `test-in-dev.css`, so those join the sweep: the doc rule covers every control.
- `App.status-and-results.test.tsx` (dismiss timer) failed in two full runs under load and passed every run alone, 5 of 5. Treated as a pre-existing load flake.
- A new test file is allowed for dashboard grouping because it adds a branch; existing test files stay untouched.

## Results

- `npm run web:build`: pass
- `npx vitest run`: 1899 passed, 9 skipped (baseline was 1894 passed, 9 skipped)
- `web:check`, `web:lint`, `web:sg`: exit 0
- `cargo nextest run --locked -p ajax-web -p ajax-cli --all-features`: 1953 passed, 1 skipped
- Not run: `web:smoke` and all browser e2e (no browser runs in this sandbox), full-workspace nextest, clippy (no Rust changed)
- No browser has rendered this design. Layout needs CI's Web job and a look on a phone.

The user approved retiring the #1020 `◦` test on 2026-10-30. The test and the `.interact-pill.tone-waiting::before` rule are deleted. Qwen hit a provider error mid-round, so the ledger update was re-routed.
