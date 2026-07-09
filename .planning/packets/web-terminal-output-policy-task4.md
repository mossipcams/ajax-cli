# Task 4 packet: pure terminal output/resize policy

## 1. Goal

Extract the pure output-follow and resize-size validation decisions from
`TerminalRawView.svelte` into a tiny terminal helper module.

Keep Ghostty, WebSocket, DOM, and Svelte state in `TerminalRawView.svelte`.
Only move pure math/decision logic.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/terminalOutputPolicy.test.ts` (new)
- `crates/ajax-web/web/src/components/TerminalRawView.test.ts` only if needed
  for a focused component-level invalid resize assertion

Production files:

- `crates/ajax-web/web/src/terminalOutputPolicy.ts` (new)
- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

Planning files:

- `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`

## 3. Forbidden changes

- Do not edit root `tests/`.
- Do not edit terminal gesture, geometry, refit, connection, viewport, CSS,
  backend Rust, generated `dist`, lockfiles, or package metadata.
- Do not change WebSocket frame formats, terminal connection behavior, Ghostty
  options, paste/copy behavior, keyboard policy, or scroll gesture behavior.
- Do not add classes, stores, dependencies, or a broad terminal abstraction.
- Do not weaken existing output-follow or resize assertions.

## 4. Architecture context

`TerminalRawView.svelte` remains the browser terminal integration component.
`terminalOutputPolicy.ts` should only contain pure, DOM-free helpers, similar in
spirit to existing `terminalGeometry.ts`, `terminalRefit.ts`, and the pure
sections of `terminalGestures.ts`.

Architecture boundary: browser terminal frontend modules may own mobile
scrolling/fitting/presentation, but do not own task truth or tmux target
selection.

## 5. Code anchors

Current output-follow logic in `TerminalRawView.svelte`:

- `const writeOutput = (text: string) => {`
- pinned branch: `if (pinnedToBottom) { writeToTerminal(text); }`
- unpinned branch:
  - `const scrollbackBefore = scrollbackLines();`
  - `writeToTerminal(text);`
  - `const growth = scrollbackLines() - scrollbackBefore;`
  - `if (growth > 0) term?.scrollLines(-growth);`
- follow/unseen branch:
  - `if (pinnedToBottom) { snapScrollbackToBottom(); } else { hasUnseenOutput = true; }`

Current resize send logic in `TerminalRawView.svelte`:

- `const sendResize = () => {`
- keyboard guard:
  - `if (isKeyboardOpen() && !pinchFlushPending && !expandFlushPending) return;`
- current send:
  - `if (!term) return;`
  - `connection.sendResize(term.cols, term.rows);`

Existing behavior tests in `TerminalRawView.test.ts` already cover:

- `does not yank the view back down while the user has scrolled up`
- `holds the reading position steady when writes grow the scrollback`
- `shows a New output control while the user is scrolled away from bottom`
- resize frame expectations via `resizeFramesOf(socket!)`

Text anchors gathered:

- `rtk rg -n "writeOutput|scrollbackBefore|scrollbackLines|hasUnseenOutput|sendResize|connection\\.sendResize|term\\.cols|term\\.rows|resizeFramesOf|invalid|NaN" crates/ajax-web/web/src/components/TerminalRawView.svelte crates/ajax-web/web/src/components/TerminalRawView.test.ts crates/ajax-web/web/src/terminal*.test.ts crates/ajax-web/web/src/terminal*.ts`

## 6. Test-first instructions

Create `crates/ajax-web/web/src/terminalOutputPolicy.test.ts` first. It should
fail before production edits because `terminalOutputPolicy.ts` does not exist.

Tests to add:

- `compensates positive scrollback growth while preserving reader position`
  - import `scrollbackGrowthCompensation`
  - expect `(40, 42)` to return `-2`
  - expect `(40, 40)` and `(42, 40)` to return `0`
  - invalid/non-finite inputs return `0`
- `maps pinned state to output follow effects`
  - import `outputFollowEffects`
  - pinned `true` returns `{ snapToBottom: true, markUnseenOutput: false }`
  - pinned `false` returns `{ snapToBottom: false, markUnseenOutput: true }`
- `accepts only finite positive integer resize sizes`
  - import `validTerminalSize`
  - `(80, 24)` returns `{ cols: 80, rows: 24 }`
  - `NaN`, `Infinity`, `0`, negative, and fractional cols/rows return
    `undefined`

Focused failing command:

```bash
rtk npm run web:test -- --run terminalOutputPolicy.test.ts
```

Report the expected failure before editing production code. If it passes before
implementation, stop and report.

## 7. Production edit instructions

Create `crates/ajax-web/web/src/terminalOutputPolicy.ts`:

- `scrollbackGrowthCompensation(before: number, after: number): number`
  - return `0` unless both inputs are finite numbers and `after > before`
  - return `before - after` for positive growth
- `outputFollowEffects(pinnedToBottom: boolean): { snapToBottom: boolean; markUnseenOutput: boolean }`
  - return snap true / unseen false when pinned
  - return snap false / unseen true when not pinned
- `validTerminalSize(cols: number, rows: number): { cols: number; rows: number } | undefined`
  - require finite, positive integers for both values
  - return the size object or `undefined`

In `TerminalRawView.svelte`:

- Import the three helpers from `../terminalOutputPolicy`.
- In `sendResize`, after `if (!term) return;`, call
  `const size = validTerminalSize(term.cols, term.rows);`
  and return if undefined; otherwise `connection.sendResize(size.cols, size.rows);`.
- In `writeOutput`, replace inline growth math with
  `const compensation = scrollbackGrowthCompensation(scrollbackBefore, scrollbackLines());`
  and scroll by `compensation` when non-zero.
- Replace the final pinned/unseen branch with `outputFollowEffects(pinnedToBottom)`.
  Keep `snapScrollbackToBottom()` and `hasUnseenOutput = true` effects in the
  component.

Do not otherwise refactor `TerminalRawView.svelte`.

Update `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`:

- Mark Task 4 checklist items complete only after verification.
- Record expected failing command and final passing commands/results.

## 8. Verification commands

Focused:

```bash
rtk npm run web:test -- --run terminalOutputPolicy.test.ts TerminalRawView.test.ts
```

If focused passes:

```bash
rtk npm run web:check
```

## 9. Acceptance criteria

- New helper test fails before production implementation.
- New helper tests pass after implementation.
- Existing TerminalRawView output-follow and resize tests pass.
- Invalid resize sizes fail closed in the pure helper and are not sent from
  `TerminalRawView.svelte`.
- No out-of-scope files are changed.

## 10. Stop conditions

- Stop if new tests pass before production edits.
- Stop if integration requires changing terminal connection frame format or
  resize scheduling semantics.
- Stop if existing output-follow/resize tests fail after implementation.
- Stop if required edits fall outside allowed files.
