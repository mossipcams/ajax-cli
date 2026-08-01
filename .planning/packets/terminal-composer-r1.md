# Safe terminal composer — implementation packet

## Scope

Add the editable transcript destination as a small React component at
`crates/ajax-web/web/src/features/task/TerminalComposer.tsx` with focused
component tests. This is the composer surface for speech and manual editing;
it is not a second terminal or PTY connection.

## Tests

- Existing composer text remains editable and is not cleared when a partial
  transcript is shown.
- Partial transcript is visibly distinct from final composer text and does not
  become submitted text automatically.
- The explicit Insert action is the only path that calls the parent callback;
  rendering, partial updates, and finalization do not call it.
- Pause-pending renders the supplied `Pausing in N…` and `Speak to continue`
  status associated with the composer.
- Disabled/finalizing state prevents duplicate Insert activation while keeping
  the composer accessible.

## Constraints

- May edit only `crates/ajax-web/web/src/features/task/TerminalComposer.tsx`,
  its focused test, and this plan.
- No xterm or WebSocket imports, no PTY writes, no Enter synthesis, no automatic
  execution, and no shortcut bar changes in this packet.
- Keep the visible input editable and preserve parent-owned text. Use existing
  button/terminal styling hooks; detailed Mic toolbar integration is later.

## Verification

```text
npm run web:test -- --run crates/ajax-web/web/src/features/task/TerminalComposer.test.tsx
npm run web:check
```

## Stop conditions

Stop after component tests and `web:check` pass. TaskTerminal wiring and Mic
shortcut behavior belong to later packets.
