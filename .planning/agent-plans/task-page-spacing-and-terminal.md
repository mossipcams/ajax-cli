# Task page spacing + post-create terminal

## Scope

Two user-reported Web Cockpit bugs on the task page:

1. **Alignment / spacing** — mobile task chrome and terminal do not share one
   horizontal inset. Terminal CSS assumes full-bleed; route-scroll still keeps
   20px side padding.
2. **Post-create terminal** — after creating a task and opening it, the
   terminal often starts at the top of scrollback with duplicated text
   (reconnect/history replay into a non-cleared Ghostty buffer).

## Non-goals

- No terminal engine swap / Live mode.
- No broad visual redesign.
- No architecture or registry changes.
- No NewTaskSheet auto-navigate (unless needed later).

## Diagnosis

### Alignment

`TerminalRawView.svelte` mobile rules strip side borders because:

> the task page drops its horizontal padding on mobile so the terminal runs
> edge to edge

But `styles.css` only sets `padding-top: 0` on
`[data-testid="route-scroll"]:has([data-outlet="task"])`. Left/right stay at
`20px + safe-area`. TaskDetail then adds another `12px + safe-area` on the
header/actions only → chrome and terminal misalign; terminal looks inset
without side borders.

### Post-create terminal

`connectTaskTerminal` passes `isReconnect` to `onOpen`, but
`TerminalRawView` ignores it and never `term.reset()`s. Fresh tasks often
fail the first tmux attach and reconnect; the second attach replays history
into the existing buffer → duplicated text + wrong scroll position.

## Delegation decision

`Delegation decision: delegated via model-router` — Cursor / composer-2.5
(frontend Svelte/CSS). One packet per behavior; alignment first.

## Approval

User reported both bugs and asked to look/fix — authorized to implement.

## Task checklist

### Task 1: Mobile task route full-bleed horizontal padding

- [x] Packet: `.planning/agent-plans/packets/task-page-full-bleed-padding.md`
- [x] Test: mobile task route-scroll zeros left/right padding (source contract)
- [x] Impl: `padding-left/right: 0` on task route-scroll mobile rule
- [x] Verify: focused App/TaskDetail tests + web:check

### Task 2: Clear terminal buffer on reconnect

- [x] Packet: `.planning/agent-plans/packets/terminal-reset-on-reconnect.md`
- [x] Test: reconnect calls `term.reset()`, re-pins bottom, no stacked history
- [x] Impl: honor `onOpen(isReconnect)`; reset + pin + snap when reconnecting
- [x] Verify: focused TerminalRawView tests + web:check

## Validation ledger

- Task 1: parent reviewed diff (styles.css + App.test.ts only);
  `rtk npm run web:test -- --run src/components/App.test.ts src/components/TaskDetail.test.ts` → PASS (48);
  `rtk npm run web:check` → PASS
- Task 2: parent reviewed diff (TerminalRawView.svelte + test only);
  `rtk npm run web:test -- --run src/components/TerminalRawView.test.ts` → PASS (141);
  `rtk npm run web:check` → PASS
- Dist: `rtk npm run web:build` → PASS (app.css/app.js updated)

## Deviations

- None.
