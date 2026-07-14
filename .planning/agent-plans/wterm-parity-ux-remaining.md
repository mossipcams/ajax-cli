# wterm remaining Ghostty parity + UX

## Scope
Surface V2 (`ajax.terminal.surfaceV2`) Ghostty-parity + mobile Safari UX.

## Hard gates
1. Experimental flag only — `WtermTerminalView*` / wterm loaders; never Ghostty path.
2. Target: **mobile Safari WebKit only**.

## Delegation decision
`Delegation decision: delegated via model-router` (cursor/composer-2.5), one
iOS-relevant behavior per round.

## Completed rounds
- [x] R1 cooler theme + 13px font + remove hardcoded 8×17 forceFit
- [x] R2 WASM validate fetch allows HTTP cache
- [x] R3/R4 remove N/A pan/readable todos; persisted font on mount
- [x] R5 pinch font grow/shrink + persist
- [x] R7 freeze PTY resize while keyboard open
- [x] R8 flush exactly one resize on keyboard close
- [x] R9 visualViewport debounce via `createRefitScheduler`
- [x] R10 safe-area pad drop on `.terminal-keys` when keyboard open
- [x] R11 snap to newest when keyboard opens while scrolled up
- [x] R12 fullscreen toggle (`html.terminal-expanded` + `is-expanded`)
- [x] R13 focus terminal on expand (open iOS keyboard)
- [x] R14 blur on exit expand (close iOS keyboard)
- [x] R15 expand allows resize while keyboard open (`expandEnter`)

## Remaining (deferred)
- [ ] R6 agent-sized 80-col floor + CSS scale — **architecture**: conflicts with
      wterm `autoResize` owning the grid; needs a dedicated design (take grid
      ownership vs scale-layer) before TDD rounds. Mobile-critical for agent TUIs.
- [ ] Copy write fallback — **blocked**: no Ajax-side copy/selection path under
      wterm native selection yet; `createTerminalClipboardUi` ready when wired.
- [ ] Backspace key-repeat — bake-off only (device), keep `it.todo`.

## Removed (wterm already solves / N/A)
- Horizontal pan (DOM scroll)
- Readable/compact font (DEFAULT_FONT_SIZE 13px)
- Measured cell metrics (autoResize + remove forceFit)
- Scroll-follow / snap-on-type / DECCKM / bracketed paste / iOS input (native + prior rounds)

## Validation (parent)
```bash
cd crates/ajax-web/web && npx vitest run src/components/WtermTerminalView.test.ts src/terminalWtermGhosttyCore.test.ts
# last: 46+ passed, 3 todo; core loader 6 passed
npm run web:build
```

## Branch
`ajax/wterm-parity-ux-remaining` — uncommitted; behind origin/main by ≥1 (rebase
before PR).
