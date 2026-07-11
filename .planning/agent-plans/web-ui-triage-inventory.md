# Web Cockpit UI triage inventory

Date: 2026-07-10  
Mode: planning-only (no code changes)  
Branch: `ajax/current-state` (at release 0.39.0; behind `origin/main` by #429 scrollback seed)

## Scope

Rank the operator-critical Web Cockpit flows by user impact, with repro notes
and current code/plan status. Goal: decide what to stabilize next — not fix yet.

## Non-goals

- No implementation in this pass
- No architecture / terminal-engine rewrite
- No Native TUI triage

## How to read severity

| Rank | Meaning |
| --- | --- |
| **P0** | Blocks primary iPhone Safari operator loop (open task → type → act) |
| **P1** | Painful but workaround exists; still daily-driver friction |
| **P2** | Polish, coverage gaps, structural debt that causes future regressions |

Status tags:

- `needs-device` — unit/e2e green or plan marked done; only iPhone confirms
- `landed` — fix commit on main; confirm it still holds
- `open` — incomplete plan or known gap
- `structural` — root cause is ownership/complexity, not one bug

## Critical path (owners)

```text
Dashboard (App/TaskList)
  → Task detail (TaskDetail)
    → Terminal mount (TerminalRawView ~1542 LOC)
      ↔ viewport.ts (keyboard / --app-*)
      ↔ terminalRefit / Geometry / Gestures / OutputPolicy / Connection
      ↔ styles.css + AppViewport (band / chrome hide)
    → Actions (ActionBar)
```

Anti-pattern already documented in `crates/ajax-web/web/TERMINAL.md`:
do not add more `*FlushPending` one-shots in `TerminalRawView` — yet
`pinchFlushPending` + `expandFlushPending` still live there.

---

## P0 — confirm on device first

These are the flows that keep regenerating `fix(web):` commits. Many have
landed patches; treat them as **suspect until you say they feel right on iPhone**.

### P0-1 Keyboard-open layout band (blank strip / jump)

- **Flow:** open task → tap terminal → soft keyboard opens
- **Symptom:** large blank space above keyboard; chrome/nav still eating band;
  terminal not filling remaining height
- **Repro:** iPhone Safari, task route, tap terminal once, do not expand
- **Code status:** `landed` via #395 (`AppViewport` fixed to `--app-band-*`;
  `styles.css` hides `.bottom-nav` / `.cockpit-chrome` under `keyboard-open`)
- **Plan debt:** `.planning/agent-plans/fix-ios-keyboard-jump-and-native-paste.md`
  still has unchecked boxes (stale relative to code)
- **Tag:** `needs-device`

### P0-2 Fullscreen expand fit + focus-zoom

- **Flow:** tap ⛶ with or without keyboard already open
- **Symptom A:** blank column under expand button (grid narrower than panel)
- **Symptom B:** whole PWA zooms/latches until blur
- **Repro A:** expand while keyboard open; wait ~300ms; pinch should not be required
- **Repro B:** expand from cold; watch page chrome scale
- **Code status:** multi-round `landed` (#375 refit, #379 `maximum-scale=1`,
  #385/#397 expand flush settle)
- **Gap:** Playwright cannot prove iOS focus-zoom; e2e `scale===1` is false-green
- **Tag:** `needs-device` + `structural` (flush/settle timing still in component)

### P0-3 Typing echo / duplicate glyphs

- **Flow:** type in inline or expanded terminal
- **Symptom:** faint off-canvas echo, stretched overlay, or duplicate chars
  beside/over Ghostty paint
- **Repro:** type fast on iPhone; also type after paste-fallback softening
- **Code status:** `landed` stack (#381, #406, #408, #410, #412) + zero-lag
  idle/cursor clear backstops
- **Tag:** `needs-device`

### P0-4 Terminal open / reconnect history mess

- **Flow:** create task → open; or lose WS → reconnect
- **Symptom:** duplicated scrollback, start at top instead of bottom, ghost text
- **Repro:** create task, open immediately; force airplane mode then restore
- **Code status:** `landed` reset-on-reconnect (#424); #429 on `origin/main`
  seeds tmux history (not yet in this worktree — pull first)
- **Tag:** `needs-device` (after fast-forward to include #429)

### P0-5 Scrollback / follow-output fighting the reader

- **Flow:** scroll up to read, while agent keeps printing
- **Symptom:** jump to bottom, can't hold position, "New output" pill resizes
  host, yank/selection broken
- **Repro:** open busy task, drag scrollback up, wait for output; try select+copy
- **Code status:** partial `landed` (#368, #249, visual-glitch plan, PR7 noise);
  still the highest-churn interaction surface
- **Tag:** `needs-device` + `structural`

---

## P1 — daily friction

### P1-1 Native paste / copy on iOS

- **Flow:** long-press paste; select text → copy
- **Symptom:** clipboard API fails → fallback tray; selection Copy overlay
  awkward; long-press may still fight focus/keyboard
- **Repro:** deny clipboard permission; long-press empty area; drag-select text
- **Status:** paste/copy plan marked done; device UX notes still say
  "check needed"
- **Tag:** `needs-device`

### P1-2 Backspace hold-to-delete

- **Flow:** hold Backspace on iOS keyboard
- **Symptom:** only one delete; no key-repeat
- **Status:** `landed` in #397 (custom key handler + ZWS seed)
- **Tag:** `needs-device`

### P1-3 Task page chrome vs terminal alignment

- **Flow:** open any task on mobile
- **Symptom:** header/actions inset differently from terminal edges; gutter or
  full-bleed mismatch
- **Status:** `landed` full-bleed padding (#424)
- **Tag:** `needs-device`

### P1-4 Actions / drop / repair toasts

- **Flow:** Drop task; Repair worktree; destructive two-tap
- **Symptom:** wrong navigation after drop; duplicate toasts; repair action
  confusion
- **Status:** several `landed` (#412 repair/toast; drop→dashboard in regression
  packet)
- **Tag:** `needs-device`

### P1-5 Battery / wake jank while terminal open

- **Flow:** leave task terminal open on phone; switch apps; return
- **Symptom:** hot phone, constant network, janky resume
- **Status:** `landed` perf sequence on main (#404 adaptive poll, write batching,
  PTY batching, resize dedupe, poll guard, binary frames, scrollback caps)
- **Tag:** `needs-device` — if still bad, measure before more "optimization"
  work; do not assume unoptimized

---

## P2 — debt and coverage (fix after P0/P1 feel stable)

### P2-1 `TerminalRawView` god-component

- ~1542 LOC, ~185 colocated tests, still owns flush flags TERMINAL.md forbids
  growing
- Cleanup plan tasks marked done, but component size proves extraction incomplete
- **Tag:** `structural` — do only after P0 device confirmations, one policy seam
  at a time

### P2-2 On-device-only test gap

- Focus-zoom, true soft-keyboard geometry, native paste menu cannot be proven
  in headless webkit
- Mitigation: short manual UAT checklist per release (below), not more false-green e2e

### P2-3 Stale planning ledger

- Multiple agent-plans still show `[ ]` for work that shipped (keyboard band,
  three regressions, etc.)
- Cleanup: mark plans closed or archive — reduces false "everything is broken"
  signal

### P2-4 Polish already mostly shipped

- Colors/a11y/dashboard glance/UX round 3 — treat as P2 unless a specific
  regression reappears

### P2-5 Hygiene

- Unused `base64` dep leftover noted after binary frames
- Worktree behind `origin/main` by #429

---

## Suggested device UAT (15 min)

Run on real iPhone Safari against current `ajax-cli web`. Mark each Pass/Fail:

1. Dashboard loads; task cards glanceable
2. Open task; terminal attaches; pinned to bottom
3. Tap terminal → keyboard; **no blank strip**; terminal usable height
4. Type 20 chars fast → **no duplicate/echo ghost**
5. Hold Backspace → repeats
6. Scroll up during output → position holds; New output pill does not shrink host
7. ⛶ expand (keyboard closed) → full width, **no page zoom**
8. ⛶ expand (keyboard open) → full width without needing pinch
9. Long-press paste (clipboard denied) → fallback works
10. Select text → Copy works
11. Drop task → returns to dashboard cleanly
12. Kill network briefly → reconnect without duplicated history
13. Leave terminal open 2 min → phone not obviously cooking

Anything Fail in 3–8 or 12 is P0 for the next stabilize pass.

---

## Recommended next step after this inventory

1. You mark the UAT list (or dictate which rows still fail).
2. We open a **stabilize-only** plan for the failing P0s, one behavior per
   delegation round.
3. Freeze polish/perf until those pass on device.
4. Only then resume `TerminalRawView` surface-area cuts.

## Delegation decision

`Delegation decision: not delegated because planning-only / triage inventory.`

## Approval

N/A — inventory only. Implementation waits for your UAT marks or explicit
"fix P0-N" order.
