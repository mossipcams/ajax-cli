# TaskTerminal speech integration — implementation packet

## Scope

Wire the existing `speechState`, `speechTransport`, and `TerminalComposer` into
`crates/ajax-web/web/src/features/task/TaskTerminal.tsx`; update only the
TaskTerminal styles needed for the composer and shortcut bar.

## Tests

- TaskTerminal renders the composer and wires one Mic activation to one speech
  transport session; duplicate activation is ignored while non-idle.
- The visible shortcut sequence has Paste immediately followed by Mic; Mic is
  text-labeled, accessible, primary-layout visible, and stays labeled Mic in
  connecting/listening/pause_pending/finalizing/error states.
- The visible `⌃C` toolbar entry is absent, while the existing Ctrl modifier and
  xterm `onData`/PTY path remain present.
- Partial/final callbacks preserve composer text, hide standalone pause, show
  pause countdown, and never call PTY send until explicit Insert.
- Cleanup cancels speech transport; error/interruption leaves finalized text and
  an actionable retry/cancel state.

## Implementation

- Add TaskTerminal-owned speech model/transport refs and a monotonic pause
  countdown effect. Pass final editable text and partial/status state to
  `TerminalComposer`.
- Generate the session ID before dispatching `start` so the reducer and
  transport share identity; ignore stale callbacks through the reducer.
- Make Mic the existing shortcut-bar button immediately after Paste, using the
  same `terminal-key` chrome/focus/pointer behavior and an active state without
  replacing the visible label.
- Preserve Ctrl+C by deleting only the visible `⌃C` entry; do not change xterm
  keyboard handlers, `sendKey`, Ctrl modifier, or PTY bridge code.

## Constraints

- May edit `TaskTerminal.tsx`, `TaskTerminal.test.tsx`, `styles.css`,
  `speechTransport.ts` only for the session-ID injection needed by integration,
  and this plan.
- No second terminal, PTY writes for partials, automatic Enter, auto-submit,
  cloud STT, or new dependencies. Keep composer Insert explicit.
- Preserve existing terminal focus, Paste, resize, toolbar touch behavior, and
  mobile layout. Do not put Mic in overflow.

## Verification

```text
npm run web:test -- --run crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx
npm run web:check
```

## Stop conditions

Stop after focused TaskTerminal tests and `web:check` pass. Full browser and
physical iOS lifecycle validation belongs to the final packet.
