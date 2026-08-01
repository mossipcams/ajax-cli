# Packet: recover native paste from xterm helper textarea

```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
```

## Task

When native paste into the Web Cockpit task terminal has empty/unusable sync
`clipboardData` (common for Safari link pastes, especially over LAN HTTP),
still send the pasted text to the PTY.

Root cause: xterm `handlePasteEvent` does not `preventDefault`; on empty
`text/plain` it calls `paste("")` and sets `textarea.value = ""`. The browser
then inserts the real clip into the helper textarea. Our `input` handler only
handles delete, so the URL never reaches the PTY.

## Scope

### Allowed

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/dist/**` (only if the repo’s normal web build step
  regenerates these; do not hand-edit minified bundles)

### Forbidden

- `crates/ajax-web/web/src/shared/lib/clipboard.ts` (unless a one-line href
  regex fix is required; prefer not)
- xterm / ghostty dependency bumps
- Toolbar Paste / paste-fallback tray redesign
- Rust / architecture.md / unrelated web UI

## Acceptance

1. Capture `paste` with non-empty `readPasteText` still `preventDefault` +
   `stopImmediatePropagation` + `sendPastedText` (unchanged).
2. Capture `paste` with empty sync text: `stopImmediatePropagation` **without**
   `preventDefault` (block xterm empty clear; allow browser insert). Do **not**
   use async `navigator.clipboard.readText` on this native-paste empty path.
3. After that gesture, `input` with `insertFromPaste` /
   `insertFromPasteAsQuotation` (or a paste-expect flag set in step 2) reads
   helper textarea value, strips `\u200B` sentinel, resets sentinel, and
   `sendPastedText(raw)` when `raw` is non-empty.
4. Do not treat ordinary `insertText` typing as paste recovery (xterm owns
   that path).
5. Existing uri-list / plain URL / beforeinput paste e2e tests still pass.
6. New e2e: empty sync clipboardData paste + simulated browser insert into
   helper textarea sends the URL in one terminal input frame.

## Constraints

- Smallest diff; reuse `sendPastedText` / `pasteThroughTerm` / `claimPasteHandle`.
- Preserve delete/`BACKSPACE_SENTINEL` reseed behavior and source-contract tests
  (update contracts that intentionally change).
- Dedup: if beforeinput already claimed the paste, textarea recovery must no-op
  via `claimPasteHandle`.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/shared/lib/clipboard.test.ts
      expected: pass
    - type: test
      command: npm run web:smoke -- --grep "paste"
      expected: pass (including new empty-clipboardData textarea-recovery case)
    - type: typecheck
      command: npm run web:check
      expected: pass
  broader_checks: []
  reason: Focused unit/source contracts plus paste e2e cover the empty-clipboardData recovery path without full suite.
```

## Stop if

- Fix requires changing xterm internals or adding dependencies
- Delete/backspace hold-repeat path would need redesign
- Diff grows past ~400 lines or outside Allowed scope
- Verification cannot be run

## Code anchors

- `onTextareaPaste` / `onTextareaInput` / `sendPastedText` in
  `crates/ajax-web/web/src/features/task/TaskTerminal.tsx` (~371–457)
- xterm `handlePasteEvent` (empty plain → `paste("")` + `textarea.value=""`) in
  `node_modules/@xterm/xterm/lib/xterm.js`
- Existing e2e paste cases in
  `crates/ajax-web/web/e2e/terminal-behavior.test.ts` (~764–841)

## Edit instructions

1. Add failing e2e for empty sync paste + textarea insert recovery.
2. Update `TaskTerminal.test.tsx` source contracts for empty-paste
   `stopImmediatePropagation` without `preventDefault`, and input recovery.
3. Implement the paste/input wiring in `TaskTerminal.tsx`.
4. Run verification commands; fix until green.
