# More Flud bug fixes — Web Cockpit task page / terminal

## Scope

Five user-reported Web Cockpit defects on the task page (iOS Safari first):

1. **Status box** — must stay a single row (no multi-line sprawl eating the page).
2. **Inline + keyboard** — tapping the non-fullscreen terminal positions it flush above the iOS keyboard.
3. **Fullscreen + keyboard** — expanded terminal also sits flush above the keyboard; bottom must not be cut off.
4. **Default width** — non-expanded terminal should be as wide as fullscreen (full-bleed cols).
5. **Keep top chrome** — when not fullscreen and the keyboard is up, back button / statuses remain visible.

## Non-goals

- No Ghostty restore, architecture, registry, PTY protocol, or auth changes.
- No Home Screen PWA / service worker path.
- No commit / push / PR unless asked.
- No desktop layout redesign beyond what mobile rules require.

## Diagnosis (source-backed)

| # | Root cause |
| --- | --- |
| 1 | `.interact-summary` / `.interact-activity` wrap freely (`overflow-wrap: anywhere`, no line-clamp) inside `.interact-panel`. |
| 2+5 | `styles.css` hides `.detail-header` / `.interact-panel` / `.meta-details` under `html.keyboard-open`. Non-expanded terminal stays `min(38vh, 300px)` and does not fill the remaining keyboard band. |
| 3 | Expanded panel uses `height: var(--app-band-height)` plus `padding: env(safe-area-inset-top) 0 0` (FullscreenLayer does not). Content box shrinks; bottom keys / last rows clip. Interaction wrap may not flex-fill cleanly. |
| 4 | Task `route-scroll` still has `20px + safe-area` horizontal padding; fullscreen panel is `left:0; right:0`. Full-bleed task padding from prior plan never landed on this branch. |

## Delegation decision

`Delegation decision: delegated via model-router`

Sequential packets (frontend UI / layout). Parent reviews each diff and runs validation independently.

- Task 1 → MiniMax if ≤2 files / ~60 lines; else Cursor.
- Task 2 (layout bundle 2–5) → Cursor (multi-surface CSS/Svelte + e2e).

## Approval

User listed these defects to fix — authorized to implement. No architecture change.

## Task checklist

### Task 1 — Status box single row

- [x] Packet: `.planning/agent-plans/packets/task-status-single-row.md`
- [x] Test: status explanation / activity clamp to one line (ellipsis) on task detail
- [x] Impl: TaskDetail CSS (and markup only if needed for clamp)
- [x] Verify: focused TaskDetail tests + `npm run web:check`
- [x] Review gate: **ACCEPT** (MiniMax via opencode; parent re-ran 18/18)

### Task 2 — Terminal keyboard band, chrome, width, fullscreen clip

- [x] Packet: `.planning/agent-plans/packets/task-terminal-keyboard-band-layout.md`
- [x] Tests (RED first):
  - keyboard-open does **not** hide task `.detail-header` / `.interact-panel`
  - keyboard-open still hides bottom-nav (and cockpit chrome if still intended)
  - terminal-expanded still hides task chrome + bottom-nav
  - mobile task route-scroll zeros horizontal padding (full-bleed)
  - expanded panel matches FullscreenLayer band (`--app-band-top` / `--app-band-height`) without extra top safe-area padding that clips the bottom
  - non-expanded + keyboard-open: terminal panel fills remaining band above keyboard (flex), not capped at 38vh
- [x] Impl: `styles.css`, `TaskTerminal.svelte` scoped CSS; e2e updated
- [x] Verify: focused unit + e2e + `npm run web:check`
- [x] Review gate: **ACCEPT** (Composer 2.5 via cursor-agent; parent re-ran)

## Validation ledger

- Task 1: `npm run web:test -- --run src/components/TaskDetail.test.ts` → 18/18 PASS; `npm run web:check` → PASS (delegate + parent)
- Task 2: RED App.test 4 failing → GREEN `App.test.ts` + `TaskDetail.test.ts` 48/48 PASS; `web:check` PASS; `web:smoke --grep "keyboard-open hides|terminal-expanded hides|fullscreen band keeps expand"` → 6/6 PASS (parent)

## Deviations

- E2E runner is `npm run web:smoke` (playwright), not `web:e2e`.
- No iOS device on iwdp for live confirmation (`list_devices` empty).
