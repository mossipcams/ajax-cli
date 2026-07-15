# Xterm terminal rebuild — Task 5 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: docs-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 2. Goal

Update the durable terminal ownership documentation to describe the now-green,
single xterm.js task surface without changing code or resurrecting forbidden
legacy names.

## 3. Allowed files

- `architecture.md`
- `crates/ajax-web/web/TERMINAL.md`

## 4. Forbidden changes

- Do not edit production code, tests, fixtures, dependencies, generated assets,
  plans/packets, or any other file.
- Do not change task truth, terminal attach ownership, PTY route/security,
  lifecycle, registry, or backend boundaries.
- Do not reintroduce any symbol forbidden by
  `legacyTerminalRemoval.test.ts`, including the old component filenames or
  rollout-setting names.
- Do not commit or change branches.

## 5. Context evidence

- Graphify: `NOT_REQUIRED`; authoritative `architecture.md` is the document
  being corrected and no boundary changes.
- Serena: `NOT_REQUIRED`; docs-only exact text replacement has no semantic code
  graph.
- ast-grep: `NOT_REQUIRED`; no source syntax changes.
- Source of truth: current `TaskDetail.svelte` mounts `TaskTerminal.svelte`;
  `TaskTerminal.svelte` uses xterm.js plus `terminalConnection.ts`; parent full
  acceptance run is 27 passed, 0 failed.

## 6. Code anchors

- `architecture.md` `ajax-web::slices::install`: remove the stale statement
  that the shell serves a removed terminal WASM asset; keep the no-manifest,
  no-service-worker, no-offline boundary.
- `architecture.md` `ajax-web::slices::terminal`: replace raw Ghostty wording
  with raw xterm.js/tmux-first wording; replace the “frontend removed / suite
  intentionally red” paragraph with the current single component,
  `terminalConnection.ts`, permanent acceptance, and unchanged backend facts.
- `TERMINAL.md`: replace Task 12 removed/intentionally-red status with current
  xterm implementation status, add the frontend component to the ownership
  table, state the 27-case mobile-WebKit suite is green, and retain the rule
  that browser UI does not own task truth or tmux target selection.

## 7. Test-first instructions

`NOT_APPLICABLE`: docs-only correction after the production behavior suite is
already green; production and test edits are forbidden.

## 8. Edit instructions

Make the smallest factual documentation edits described by the anchors. Do not
add migration history, a feature tour, or duplicate the behavior contract.

## 9. Verification commands

```bash
rtk npm run web:test -- --run crates/ajax-web/web/src/legacyTerminalRemoval.test.ts
rtk npm run web:check
```

## 10. Acceptance criteria

- Both docs state that one xterm.js surface is mounted from TaskDetail and the
  permanent 27-case mobile-WebKit contract is green.
- Existing backend/security/task-truth boundaries are unchanged.
- Removal hygiene and web checks pass.
- Only the two allowed docs change in this round.

## 11. Stop conditions

- Correcting the docs would require changing architecture ownership or code.
- A forbidden legacy symbol is required or hygiene fails.
- Any unlisted file changes.
