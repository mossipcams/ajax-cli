# iOS xterm mobile bugs (fullscreen / keyboard / copy)

## Scope

Fix six Web Cockpit phone regressions reported against the new xterm terminal:

1. Fullscreen expand button disappears / cannot exit
2. Hotkeys too big
3. Terminal not attached above the iOS keyboard
4. After tapping the terminal, Dashboard / New / Settings / Back unusable
5. Two scrollbars visible
6. Copy does nothing

## Non-goals

- No Ghostty restore, architecture, registry, or PTY protocol changes
- No commit / push / PR unless asked
- No public-internet / auth changes

## IWDP status

- Device `iPhone` (`9222`) connected; Safari tab was briefly available at
  `https://ajax.mossyhome.net/#/t/ajax-cli%2Fxterm-move` then disconnected
  (empty `list_pages`). Re-verify on device after each green slice when Safari
  is open again.

## Diagnoses (source-backed)

| Bug | Root cause |
| --- | --- |
| Fullscreen exit + keyboard attach | Expanded panel uses `top: 0` + `height: var(--app-band-height)` instead of `top: var(--app-band-top)` like `FullscreenLayer` / pre-xterm `styles.css`. When `visualViewport.offsetTop > 0` or keyboard is open, the expand control sits above the visible band and cannot be tapped. |
| Chrome / nav unreachable after tap | Pre-xterm CSS hid `.cockpit-chrome`, `.bottom-nav`, and task chrome under `html.keyboard-open` / `html.terminal-expanded` so the terminal filled the band; exit via `⌄` or expand. That CSS was dropped in the xterm rewrite. Stuck fullscreen also leaves `inert` on chrome (`applyExpandedInert`). |
| Hotkeys too big | `.terminal-key` forced `min-width/min-height: 44px` (and e2e asserts ≥44). |
| Dual scrollbars | `.terminal-interaction-wrap` is `overflow-y: auto` without the scrollbar-hide treatment used by `route-scroll`. |
| Copy dead | Selection → Copy overlay / `terminalClipboard.ts` was removed with Ghostty and never re-ported onto xterm `getSelection()`. |

## Delegation decision

`Delegation decision: delegated via model-router`

Sequential Cursor packets (multi-surface frontend UI). Parent reviews each
diff and runs validation independently.

## Task checklist

- [x] **Task 1 — Fullscreen band + keyboard chrome collapse + scrollbar hide + compact keys**
  - Test: phone cases proving expanded panel uses `--app-band-top`, keyboard-open /
    terminal-expanded hide cockpit chrome + bottom-nav, expand button remains
    tappable after simulated keyboard band, interaction wrap hides scrollbars,
    key buttons are compact (< 44px tall). Show RED first.
  - Impl: restore lost mobile CSS; fix TaskTerminal expanded `top`; compact keys;
    hide terminal interaction scrollbar.
  - Verify: focused playwright + `npm run web:check`.

- [x] **Task 2 — Terminal Copy via xterm selection**
  - Test: select text → Copy control → clipboard write (with fallback). RED first.
  - Impl: smallest selection + Copy overlay using public xterm APIs + existing
    `copyText`.
  - Verify: focused cases + Task 1 group green.
  - **Revise (long-press)**: long-press on interaction surface maps touch to
    terminal cell, expands word boundaries, calls `term.select()`; zero PTY input;
    Copy overlay on non-empty selection. Clear `hostEl.__xterm` on dispose (DEV).

- [ ] **Task 3 — IWDP device confirmation**
  - Blocked: iPhone dropped from iwdp (`list_devices` empty after restart).
  - When Safari is open again: screenshot expand enter/exit, keyboard band,
    copy, single scrollbar, chrome restore after Hide keyboard (`⌄`).
  - Live `ajax.mossyhome.net` will not show these fixes until this worktree is
    built/redeployed.

## Approval

User reported these as bugs to fix (authorization to implement). No architecture
change.

## Deviations

- Copy long-press was a revise round after the first Copy packet only covered
  programmatic selection.

## Validation log

- Parent gate Task 1: ACCEPT — `web:check` 0; 8 focused e2e green.
- Parent gate Task 2 (+ long-press revise): ACCEPT — `web:check` 0; 10 focused
  e2e green (`Copy|long press|fullscreen band|keyboard-open hides|compact
  terminal keys|phone fullscreen keeps background|interaction wrap hides`).
- IWDP: initial page seen then lost; final `list_devices` empty — device verify
  not completed.
