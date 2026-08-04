PACKET_STATUS: READY
TASK_KIND: behavior
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Stop live agent/tmux `CSI 2 J` clears from dumping successive viewport frames
into scrollback (the “overwriting words” look when scrolling history), while
keeping attach-clear seed preservation via `scrollOnEraseInDisplay` only for the
seeded-open window.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/shared/lib/scrollbackOverwriteProbe.test.ts`
- `crates/ajax-web/web/TERMINAL_BEHAVIOR_CONTRACT.md`
- `crates/ajax-web/web/dist/terminal.js` (via `npm run web:build` only)
- `.planning/agent-plans/scrollback-overwrite-diagnose-fix.md`

## Forbidden changes

- Do not commit, push, merge, rebase, or change branches.
- Do not reintroduce server CRLF history pad, doubled pad, or `-E -` capture.
- Do not change hostile-sequence list, `seed=0` dial policy, or architecture docs.
- Do not restore Ghostty/wterm.
- Do not hand-edit `dist/*`; rebuild with `npm run web:build`.
- Do not edit files outside Allowed files.

## Context evidence

Root cause (proven):
- tmux attach sends `\x1b[?1049h` then `\x1b[H\x1b[2J`. Filter strips `1049h`,
  keeps `ED2`, so clears hit the normal buffer.
- `#666` set permanent `scrollOnEraseInDisplay: true` so attach ED2 pushes seed
  into scrollback. That also dumps every later live ED2 into scrollback → stacked
  near-duplicate frames when scrolling (“overwriting words”).
- Probe file already shows: permanent scrollOnErase dumps FRAME-N copies;
  bootstrap latch (true → first ED2 → false) keeps seed and stops later dumps.

Desired behavior:
- Seeded first dial / seeded manual reconnect: `scrollOnEraseInDisplay` starts
  `true` so attach ED2 preserves seed, then turns `false` when seed reveal
  completes (`revealSeed`).
- Unseeded auto-reconnect (`seeded: false`): set `false` immediately on open
  (local buffer already held; no seed wipe to defend).
- Seeded reconnect that calls `term.reset()`: re-enable `true` before seed
  writes arrive.

## Code anchors

- `TaskTerminal.tsx` constructor ~1516–1521: keep `scrollOnEraseInDisplay: true`.
- `revealSeed` ~1042–1053: after reveal work, set
  `termRef.current.options.scrollOnEraseInDisplay = false`.
- `onOpen` ~1619–1634: if `!seeded`, set option `false`; if seeded reconnect
  `reset()`, set option `true` again after reset.
- `TaskTerminal.test.tsx`: existing source assert for `scrollOnEraseInDisplay: true`;
  extend with source asserts that revealSeed / unseeded open / seeded reset latch
  the option (string match on the production source is the established pattern).
- `scrollbackOverwriteProbe.test.ts`: keep the bootstrap-latch buffer test as
  product coverage; remove or narrow pure diagnostic cases that are not needed
  once TaskTerminal wiring is asserted (keep at least the latch buffer proof).
- `TERMINAL_BEHAVIOR_CONTRACT.md` history-seed row: note scrollOnErase is
  seed-window only, then disabled for live output.

## Verification

```yaml
methods:
  - type: test
    command: npm run web:test -- --run src/features/task/TaskTerminal.test.tsx src/shared/lib/scrollbackOverwriteProbe.test.ts
  - type: lint
    command: npm run web:lint
  - type: typecheck
    command: npm run web:check
  - type: build
    command: npm run web:build
```

## Acceptance criteria

1. Seeded open still constructs Terminal with `scrollOnEraseInDisplay: true`.
2. `revealSeed` disables `scrollOnEraseInDisplay` after the quiet/cap reveal.
3. Unseeded `onOpen` disables it immediately.
4. Seeded reconnect `reset()` path re-enables it before seed.
5. Probe (or equivalent) still proves: first ED2 with option true preserves seed;
   after option false, a later ED2 does not retain the prior live frame in
   scrollback.
6. Contract doc matches seed-window-only behavior.
7. `npm run web:build` refreshes `dist/terminal.js`.

## Stop conditions

- Stop if fixing requires changing hostile-filter alt-screen policy or server pad.
- Stop if seed reveal can complete before attach ED2 with no other latch point
  (escalate with evidence; do not invent pad).
- Stop if changes exceed Allowed files.
