# Hotbar look + inline header scroll + Drop survives back

**Date:** 2026-07-21  
**Mode:** Behavior Change (three sequential bounded fixes)

## Scope

1. **Hotbar (iOS PWA WebKit)** — tune `.terminal-key` font / spacing / size; remove the hotbar `⌄` Hide keyboard control (collapse/exit-looking key). Fullscreen exit stays on the corner `⛶` control.
2. **Inline terminal header** — when focused in the terminal (not fullscreen), keep the detail back header on-screen by resetting `route-scroll` along with document scroll.
3. **Drop after back** — confirming Drop from task detail, then navigating to the dashboard, must still commit the Drop after the undo window (unless Undo).

## Non-goals

- Changing fullscreen enter/exit geometry or PTY protocol.
- Redesigning the whole task page chrome.
- Moving Drop confirmation UX off the toast.
- Architecture / registry changes.

## Root causes

1. Hotbar packs 10 keys; `⌄` is redundant with system keyboard dismiss and burns width. Mobile keys use 12px label type and tight padding.
2. `resetDocumentScroll()` clears window/document scroll only. iOS focus/keyboard still scrolls `[data-testid="route-scroll"]`, pushing `.detail-header` above the visible band.
3. `ActionBar` clears `dropTimerRef` on unmount, so leaving the task cancels the ActionBar-side commit. `ResultPanel` lives in `App` but its dismiss timer restarts whenever `onDismiss` identity changes (inline arrow in `App`), so after navigation Drop may never commit.

## Delegation

`Delegation decision: delegated via model-router` — one packet per task, sequential implement → review gate.

Order: **T3 Drop** → **T2 scroll** → **T1 hotbar**.

- T3: pi-delegate/GLM blocked (opencode weekly limit) → cursor-delegate/`composer-2.5`
- T2: `not delegated because R-LOCAL-TINY`
- T1: cursor-delegate/`composer-2.5` (multi-surface frontend UI)

## Approval

User reported all three and asked for fixes — authorized to implement.

## Task checklist

### T3 — Drop survives navigate-to-dashboard

- [x] Packet: `.planning/agent-plans/packets/drop-survives-back.md`
- [x] Test: ActionBar unmount with armed Drop still posts after `DROP_UNDO_MS`; ResultPanel commit timer does not reset on parent re-render with new callback identity
- [x] Impl: leave pending Drop timer armed across ActionBar unmount; stabilize ResultPanel auto-commit effect via callback refs
- [x] Verify: focused ActionBar + ResultPanel vitest

### T2 — Inline terminal keeps whole header row visible

- [x] Packet: `.planning/agent-plans/packets/route-scroll-reset-on-focus.md`
- [x] Test: `resetDocumentScroll` also zeros `[data-testid="route-scroll"]` scrollTop
- [x] Impl: extend `resetDocumentScroll` in `viewport.ts`
- [x] Verify: focused viewport tests
- Delegation: `not delegated because R-LOCAL-TINY`
- [x] Follow-up (user clarify): whole `.detail-header` (back + title + status pill)
  - sticky detail-header in mobile task route-scroll
  - `padding-top: env(safe-area-inset-top)` on keyboard-open fixed `.task-detail` (chrome hidden)
  - App.test contracts + web:build

### T1 — Hotbar iOS look + remove Hide keyboard

- [x] Packet: `.planning/agent-plans/packets/hotbar-ios-look.md`
- [x] Test: no Hide keyboard / `⌄` in TaskTerminal; mobile hotbar CSS contracts for font/gap/size; e2e Hide keyboard test removed
- [x] Impl: remove Hide keyboard button; tune mobile `.terminal-keys` / `.terminal-key`
- [x] Verify: focused vitest + `npm run web:build` (dist ships CSS)

## Validation ledger

**T3:** `npm run web:test -- --run …ActionBar.test.tsx …ResultPanel.test.tsx` — 21/21 (parent re-ran).

**T2:** `viewport.test.ts` 24/24; follow-up App.test header-row contracts + `keyboardBandPin` — 55/55; `npm run web:build` refreshed dist.
**T2 e2e (mobile-webkit):** added `inline keyboard-open keeps the whole detail-header row inside the visible band`; focused smoke 5/5 passed.

**T1:** `npm run web:test -- --run …TaskTerminal.test.tsx …keyboardBandPin.test.ts …App.test.tsx` — 68/68 (parent re-ran). Delegate ran `npm run web:build`. Playwright e2e not re-run (Hide-keyboard test deleted).

## Deviations

- T3/T1: `scripts/run-delegate` exit 65 (MISSING_STRUCTURED_REPORT) despite successful work; parent gated on git diff + re-validation.
- T1: deleted e2e Hide-keyboard test rather than rewriting (packet allowed).
- T1 build also refreshed `dist/terminal.js` (incidental).
- Follow-up: Paste label hung off equal-flex key — shrink label font/pad/gap + nowrap/overflow clip (`R-LOCAL-TINY`).
