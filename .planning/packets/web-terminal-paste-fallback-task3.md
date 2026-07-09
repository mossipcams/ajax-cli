# Task 3 packet: name paste fallback decisions

## 1. Goal

Reduce paste fallback weirdness in `TerminalRawView.svelte` by replacing
scattered inline state mutations with small named component-local functions.

Behavior must stay the same: missing/rejected clipboard opens fallback, Send
pastes non-empty text and closes, empty Send closes without paste, Cancel
closes, and native paste still sends text.

## 2. Allowed files

Test files:

- `crates/ajax-web/web/src/components/TerminalRawView.test.ts`

Production files:

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

Planning files:

- `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`

## 3. Forbidden changes

- Do not edit root `tests/`.
- Do not edit terminal gesture, geometry, refit, viewport, CSS, backend Rust,
  generated `dist`, lockfiles, package metadata, or diagnostics helpers.
- Do not change clipboard copy behavior, selection behavior, socket behavior,
  Ghostty options, keyboard focus policy, or terminal paste semantics.
- Do not add a new file or dependency.
- Do not weaken or delete existing paste assertions.

## 4. Architecture context

`TerminalRawView.svelte` is still the terminal transport/UI integration point.
This task does not move terminal transport out of it. It only names the
component-local paste fallback decisions so error handling remains visible and
less scattered.

Web Cockpit remains raw Ghostty/tmux-first; browser code must not add alternate
terminal modes or task-control APIs.

## 5. Code anchors

Current state in `crates/ajax-web/web/src/components/TerminalRawView.svelte`:

- State:
  - `let pasteNotice = $state("");`
  - `let pasteFallbackOpen = $state(false);`
  - `let pasteFallbackInput = $state<HTMLTextAreaElement | undefined>();`
- Function anchor:
  - `pasteToTerm = (text: string) => { term?.paste(text); term?.focus(); };`
  - `requestPaste = () => { const clipboard = navigator.clipboard; ... }`
- Inline fallback mutations:
  - missing clipboard branch: `pasteFallbackOpen = true;`
  - rejected clipboard catch: `pasteFallbackOpen = true;`
  - native `onpaste`: sets `pasteFallbackOpen = false` and calls `pasteToTerm`
  - Send button: reads `pasteFallbackInput?.value`, sets
    `pasteFallbackOpen = false`, clears textarea, conditionally pastes
  - Cancel button: sets `pasteFallbackOpen = false`

Existing tests in `TerminalRawView.test.ts` already cover:

- `surfaces a clipboard read failure instead of silently doing nothing`
- `sends paste fallback textarea value through term.paste and closes the tray`
- `does not paste when Send is tapped with an empty fallback value`
- `opens a paste fallback sheet when the async clipboard API is unavailable`
- `closes the paste fallback sheet without pasting when Cancel is tapped`

Text anchors gathered:

- `rtk rg -n "Paste|pasteFallback|clipboard|readText|term\\.paste|pasteToTerm|terminal-paste-fallback|Send|Cancel|pasteNotice|surfaces" crates/ajax-web/web/src/components/TerminalRawView.test.ts crates/ajax-web/web/src/components/TerminalRawView.svelte`

## 6. Test-first instructions

Add a small source-contract test to `TerminalRawView.test.ts` near the paste
tests:

- Test name: `names paste fallback state transitions`.
- Assert `terminalRawViewSource` contains small named functions for:
  - `openPasteFallback`
  - `closePasteFallback`
  - `sendPasteFallbackText`
- Assert `requestPaste` still exists and the markup still has
  `data-testid="terminal-paste-fallback"`.

Focused command that must fail before production edits:

```bash
rtk npm run web:test -- --run TerminalRawView.test.ts
```

Report the expected failure before editing production code. If it passes before
implementation, stop and report.

## 7. Production edit instructions

In `TerminalRawView.svelte`, add small component-local helpers near the paste
state declarations or near `requestPaste`:

- `openPasteFallback()` sets `pasteFallbackOpen = true`.
- `closePasteFallback()` sets `pasteFallbackOpen = false`.
- `sendPasteFallbackText(text: string)` closes the fallback, clears
  `pasteFallbackInput.value` if present, and calls `pasteToTerm(text)` only when
  `text` is non-empty.

Use these helpers in:

- missing clipboard branch;
- rejected clipboard catch;
- native fallback textarea `onpaste`;
- Send button;
- Cancel button.

Keep `pasteToTerm` as the only function that directly calls `term?.paste(text)`
for user-provided paste text. If this requires moving the helper definitions
below `pasteToTerm`, do that minimally.

Do not add stores, classes, components, or external helpers.

Update `.planning/agent-plans/web-viewport-terminal-design-cleanup.md`:

- Mark Task 3 checklist items complete only after verification.
- Record expected failing command and final passing commands/results.

## 8. Verification commands

Focused:

```bash
rtk npm run web:test -- --run TerminalRawView.test.ts
```

If focused passes:

```bash
rtk npm run web:check
```

## 9. Acceptance criteria

- The new source-contract test fails before production edits.
- All existing paste behavior tests still pass.
- `TerminalRawView.svelte` has named paste fallback transition helpers.
- Inline paste fallback open/close/send decisions are replaced by those helpers.
- No behavior outside paste fallback state handling changes.
- No out-of-scope files are changed.

## 10. Stop conditions

- Stop if the new test passes before production edits.
- Stop if helper extraction requires changing paste semantics or focus policy.
- Stop if existing paste behavior tests fail after implementation.
- Stop if required edits fall outside allowed files.
