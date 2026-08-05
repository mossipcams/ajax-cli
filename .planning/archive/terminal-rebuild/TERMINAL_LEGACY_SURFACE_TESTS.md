# Removable terminal surface characterization (historical inventory)

**Status:** Task 12 complete. The Ghostty and Surface V2 files and test seams
listed below are **already deleted** from the tree. This file is preserved as
**historical evidence** of the removed surface, not as a live inventory. Do not
re-add the listed files; do not re-enable the listed suites.

Permanent suites that remain the contract for the (post-rebuild) terminal
surface:

- `e2e/terminal-behavior.test.ts` — mobile-webkit only automated regression
  proxy for the rebuilt terminal surface
- `src/terminalConnection.test.ts` — frontend socket / seed / resize boundary
  unit tests
- Rust PTY / runtime tests in `crates/ajax-web/src/slices/terminal.rs`,
  `crates/ajax-web/src/adapters/terminal_pty.rs`, and
  `crates/ajax-web/src/runtime.rs` (see `TERMINAL_BEHAVIOR_CONTRACT.md` §8)
- `src/legacyTerminalRemoval.test.ts` — living hygiene guard that fails if any
  of the removed legacy surfaces or test seams reappear

These are **not** acceptance criteria and **not** a shared adapter contract for
the new surface. See `TERMINAL_REBUILD_ACCEPTANCE.md` for the rebuild matrix.

## Ghostty integration, component, probe, and workaround tests

| File | Why removable | Permanent replacement |
| --- | --- | --- |
| `src/components/TerminalRawView.test.ts` | Ghostty `Terminal`/`FitAddon` mocks, mount lifecycle, wasm load, `attachCustomKeyEventHandler`, ZWS sentinel, scroll-follow leading-edge gate, zero-lag overlay position | `e2e/terminal-behavior.test.ts` (lifecycle, I/O, scroll, input) |
| `src/terminalPreload.test.ts` | `/ghostty-vt.wasm` preload cache and `warmTerminalAssets` per surface | none (asset wiring deleted with Ghostty) |
| `e2e/fixtures.ts` → `terminalPanel` | Selects `[data-terminal-engine='ghostty']` | `terminalSurface` (engine-neutral `task-terminal-panel`) |
| `e2e/terminal-scroll-garble.test.ts` | `__ajaxTerminalProbe`, `__ajaxTerminalProbeEnable`, canvas buffer reads | scroll-follow / no-yank outcomes in `terminal-behavior.test.ts` |
| `e2e/terminal-scroll.test.ts` | `terminalPanel`, `.terminal-host`, canvas locators, scroll-follow via probe | `terminal-behavior.test.ts` (`New output ↓`, scrollback read) |
| `e2e/terminal-zero-lag.test.ts` | canvas + `[data-testid='terminal-zero-lag-input']` overlay | typed-echo product row; overlay mechanics are Legacy Ghostty |
| `e2e/smoke.test.ts` (terminal rows) | ~~`terminalPanel`, visible canvas assertions~~ — terminal canvas rows already removed in Task 12; smoke no longer asserts the terminal | `terminal-behavior.test.ts` surface visibility |
| `e2e/fullscreen-refit.test.ts` | `terminalPanel`, canvas, `.is-expanded` class | `terminal-behavior.test.ts` fullscreen resize + input continuity |
| `e2e/actions.test.ts` (terminal row) | ~~canvas visibility via `terminalPanel`~~ — terminal canvas assertion already removed; actions covers chrome/buttons only | `terminal-behavior.test.ts` |

Production test seams (delete with renderer):

- `window.__ajaxTerminalProbe` / `__ajaxTerminalProbeEnable`
  (`TerminalRawView.svelte:1106-1118,1176-1177`)
- `TERMINAL_PLACEHOLDER_KEY` (`localStorage.ajax.debug.terminalPlaceholder`)
  (`TerminalRawView.svelte:48-51,1197-1199`)

## Experimental xterm selector, component, settings, and preload tests

| File | Why removable | Permanent replacement |
| --- | --- | --- |
| `src/components/XtermTerminalView.test.ts` | `@xterm/xterm` mount and control bar | one functioning surface (`terminal-behavior.test.ts`) |
| `src/components/TerminalSurfaceSelector.test.ts` | Ghostty vs xterm switch + error/retry | removed with Surface V2 flag (Task 12) |
| `src/terminalSurfaceSetting.test.ts` | `ajax.terminal.surfaceV2` Dev toggle | not a rebuild setting |
| `src/components/SettingsView.test.ts` (surface V2 portions) | Settings UI for experimental flag | pinch persistence only (`terminal-behavior.test.ts`) |
| `src/terminalPreload.test.ts` (xterm branch) | `warmTerminalAssets` preloads xterm chunk when V2 on | deleted with selector |

## Renderer policy and math tests (may delete or rewrite)

These freeze current scheduling, fit math, and library workarounds—not the
Product outcomes they support.

| File | Characterizes | Outcome covered permanently by |
| --- | --- | --- |
| `src/terminalRefit.test.ts` | 100ms debounce, rAF coalescing, `scheduleImmediate`/`schedulePostLayout` | `terminal-behavior.test.ts` resize dedupe + settled sizes |
| `src/terminalOutputPolicy.test.ts` | write-batcher leading/trailing edge, once-per-rAF paint, resize dedupe helpers | `terminal-behavior.test.ts` scroll-follow / no-yank |
| `src/terminalLayoutPolicy.test.ts` | keyboard freeze, pinch/expand exemptions | `terminal-behavior.test.ts` keyboard burst + fullscreen |
| `src/terminalGeometry.test.ts` | `flooredCols`, `fitScale`, `fitFontSize`, pinch bounds, scrollback caps | 80-column Product cites `architecture.md:700-704`; pinch persistence in `terminal-behavior.test.ts` |
| `src/terminalZeroLag.test.ts` | overlay painter, canvas metrics, idle clear | echo-before-PTY product row |
| `src/viewport.test.ts` | 150px/100px keyboard thresholds, pinch-guard, `--app-height` | `terminal-behavior.test.ts` + Physical iOS rows in contract §9 |
| `src/terminalSelection.test.ts` | Ghostty `SelectionManager` coordinate math | `terminal-behavior.test.ts` proves touch long-press sends no PTY input only; native selection fidelity stays Physical iOS (contract §9) |
| `src/terminalTouchScroll.test.ts` | native vertical pan on Ghostty host | `terminal-behavior.test.ts` proves synthetic touch/scroll sends no PTY input only; native vertical pan and momentum stay Physical iOS (contract §9) |

## Engine-specific Playwright files

All use `terminalPanel`, canvas, `.terminal-host`, probes, generated DOM, or
old class names. **Inventory only** for `desktop-chromium`; not in-scope for
the `mobile-webkit` compatibility contract.

| File | Engine-specific hooks |
| --- | --- |
| `e2e/terminal-scroll.test.ts` | canvas, `.terminal-host` |
| `e2e/terminal-scroll-garble.test.ts` | `__ajaxTerminalProbe`, canvas |
| `e2e/terminal-zero-lag.test.ts` | canvas, zero-lag overlay test id |
| `e2e/fullscreen-refit.test.ts` | canvas, `.is-expanded` |
| `e2e/layout-scroll.test.ts` | `TERMINAL_PLACEHOLDER_KEY`, terminal placeholder |
| `e2e/smoke.test.ts` | ~~`terminalPanel`, canvas~~ — terminal rows already removed in Task 12 |
| `e2e/actions.test.ts` | ~~`terminalPanel`, canvas~~ — no longer asserts terminal canvas |
| `e2e/visual.test.ts` | dashboard visual baselines (not terminal contract) |

## Marker

> **Status note (post-Task 12):** the index, listed suites, Ghostty/xterm
> production paths, WASM assets, Surface V2 settings, and test seams were
> removed together in Task 12. This file is retained as historical evidence
> only. Keep `e2e/terminal-behavior.test.ts`, `terminalConnection.test.ts`,
> Rust PTY/runtime tests, and `legacyTerminalRemoval.test.ts` as the permanent
> boundary contract.
