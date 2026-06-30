# TDD Implementation Packet — Mobile Web Terminal (iOS Safari)

**Branch:** `ajax/ajax-web-ui-ux`
**Owner area:** `crates/ajax-web/web/` (Svelte frontend) + `crates/ajax-web/src/slices/install.rs` (Rust asset snapshots)
**Approach:** Full-screen, keyboard-aware terminal takeover, ported from the sibling project **Codeman** (`Ark0N/Codeman`) — the project whose `xterm-zerolag-input` package ajax already imports (`TerminalPanel.svelte:5`).

---

## 1. Problem statement

On iOS Safari the task terminal is unusable:

1. **Scrolling fights / doesn't work** — the xterm `.xterm-viewport` (`overflow-y: auto`, `styles.css:296`) is a scroll container nested inside `<main>`, which is itself the page scroller. iOS scroll-chains/rubber-bands between them. No `-webkit-overflow-scrolling: touch`, no `overscroll-behavior: contain`.
2. **Terminal too small** — fixed `75dvh` (`TerminalPanel.svelte:236`) sits below the sticky header, status hero, and "Next action" card inside a scrolling page; after browser chrome + keyboard the visible slice is tiny.
3. **Can't see what you type** — the terminal is in document flow, not pinned to `visualViewport`. When the keyboard opens, iOS scrolls the document to chase xterm's hidden textarea, pushing the cursor row and the control-key bar behind the keyboard.

## 2. Target behaviour (definition of done)

- On phones/tablets (`max-width: 767px`), opening a task renders the detail view as a single `position: fixed` flex column sized to `var(--app-height)` (= `visualViewport.height`).
- The xterm viewport is the **only** scroll container, with momentum scrolling and contained overscroll.
- When the soft keyboard opens, the container shrinks to the visible band automatically (because `--app-height` tracks `visualViewport.height`); the cursor row and control-key bar stay visible above the keyboard; the document never scrolls away.
- Double-tap / pinch zoom is suppressed; input stays ≥16px so focus never zooms.
- Desktop (`min-width: 768px`) is visually unchanged.
- All existing JS unit tests, Rust asset-snapshot tests, and `svelte-check` stay green.

## 3. Hard constraints (do not violate)

| Constraint | Source | Consequence |
|---|---|---|
| **No `100vh` in any CSS** (scoped or global) | `install.rs::stylesheet_preserves_the_safari_first_visual_language` asserts `!compact.contains("100vh")` | Use `100dvh` / `var(--app-height)` only. (`100dvh` is safe — not a `100vh` substring.) |
| Preserve tokens: `.cockpit-chrome`, `env(safe-area-inset-*)`, `scrollbar-width:none`, `::-webkit-scrollbar`, `font-size:16px`, palette hexes `#f4eee0 #251e1a #c9a24a #367069 #bc5c3e` | same test | Additive CSS only; never delete these. |
| Rust snapshot tests read built `dist/` | `static_asset(...)` in `install.rs` | **`npm run web:build` before `cargo test`.** |
| Editing `web/*` trips snapshots in `ajax-web/slices/install.rs` **and** `ajax-cli` web_backend | memory: web_asset_snapshot_tests | Run both crates' tests after build. |
| Fresh worktree lacks `node_modules` | memory: worktree_npm_install | `npm install` at repo root before any `npm run`. |
| No `Co-Authored-By` / "Claude" in commits or PRs | global CLAUDE.md | Plain commit subjects only. |
| Don't modify files under `tests/`; fix impl not tests | global CLAUDE.md | New tests live beside source under `web/src/`. |
| Keep existing test hooks | `TerminalPanel.test.ts`, `TaskDetail.test.ts` | Preserve `data-testid="task-terminal-panel"`, `.task-terminal-viewport`, aria-label `Task terminal`, control keys, `← Back`. |

## 4. Reference — proven Codeman techniques (ported)

From Codeman `src/web/public/mobile-handlers.js` + `mobile.css`:

- **`--app-height` CSS var** = `window.visualViewport.height`, set on `document.documentElement`, updated on every `visualViewport` `resize`.
- **Keyboard detection:** keyboard *shown* when `initialHeight - currentHeight > 150`; *hidden* when within `100px` of baseline (100px absorbs iOS address-bar drift / the iOS 26 ~24px discrepancy). Toggle a class (Codeman: `keyboard-visible`; ours: `keyboard-open`).
- **Scroll guard:** while keyboard open, `window.scrollTo(0,0)` on the window `scroll` event — stops iOS scrolling the UI off-screen to reveal the hidden textarea.
- **Single fixed shell** sized to `--app-height` instead of per-element `translateY` (ajax has one terminal per route, so the whole detail column is the shell — simpler than Codeman's separate fixed toolbar).
- **One-shot refit + send-resize** on keyboard open/close so the PTY rows match the visible area (already wired in `TerminalPanel` via `scheduleRefit`; add `scrollToBottom()`).
- **Momentum scroll** `-webkit-overflow-scrolling: touch`; **zoom prevention** `touch-action: manipulation` + `gesturestart`/`gesturechange` `preventDefault`.

## 4.5 Dependency map (grounded via serena AST + graphify)

Built with serena's symbol tools (Rust gate side) + a graphify AST graph of `crates/ajax-web/web/src` (253 nodes / 402 edges; artifacts in `graphify-out/graph.html`, `GRAPH_REPORT.md`, `graph.json`). Key facts that bound the work:

**The terminal UI files are leaf nodes — low fan-in, so the overhaul is peripheral and will not ripple into the cockpit/api core.** All graphify "god nodes" (`assertOperationResponse`, `getJson`, `postJson`, `OperationResponse`, `classifyStatus`) live in the api/contracts/test cluster — *none* on the terminal render path.

| File | Imported by (fan-in) | Imports (out) | Blast radius |
|---|---|---|---|
| `TerminalPanel.svelte` | `TaskDetail.svelte`, `TerminalPanel.test.ts` | `../api`, `@xterm/*`, `xterm-zerolag-input`, `svelte` | edits touch only TaskDetail + its own test |
| `TaskDetail.svelte` | `App.svelte`, `TaskDetail.test.ts` | `ActionBar`, `TerminalPanel`, `../types`, `../state`, `../diagnostics` | edits touch App render path + its own test |
| `App.svelte` | `main.ts`, `App.test.ts` | `TaskDetail`, `TaskList`, `../api`, `../polling`, `../routes`, … | wiring `viewport.ts` here only affects `App.test.ts` |
| `viewport.ts` (new) | — (greenfield) | `svelte`? no — plain module | sole consumer will be `App.svelte`; zero existing dependents |
| `api.ts` | `api.test.ts`, `ActionBar.test.ts`, `NewTaskSheet.test.ts`, `SettingsView.test.ts` | `contracts.ts`, `types.ts`, `polling.ts` | **hub — do NOT modify.** Owns `openTaskTerminalSocket()` + `taskTerminalWebSocketUrl()`; keep resize logic inline in `TerminalPanel` to avoid 4-test fan-out. |

**Implication for the plan:** the resize/socket already flows `TerminalPanel → api.openTaskTerminalSocket()` and `TerminalPanel` sends `{type:"resize"}` frames inline — so no `api.ts` change is needed (confirmed: keeps blast radius to TerminalPanel + TaskDetail + App and their three tests).

**Rust gate symbols (serena AST):**
- `crates/ajax-web/src/slices/install.rs` → `mod tests`: the **strict** gate is `stylesheet_preserves_the_safari_first_visual_language` (asserts `!css.contains("100vh")` + safe-area/scrollbar/`font-size:16px`/palette tokens). Also `shell_is_the_bundled_svelte_mount_point`, `retired_pwa_install_assets_are_absent`.
- `crates/ajax-cli/src/web_backend.rs` → `mod tests`: `http_router_serves_static_css_and_js` and `mobile_shell_is_responsive_and_loads_cockpit_data` are **tolerant** — they only require non-empty `/app.css` + `/app.js` with correct content-type and the shell linking them. So this second snapshot surface just needs a successful `npm run web:build`, not specific content.

## 5. Shared contract (introduce once, consume everywhere)

CSS custom property + class set on `document.documentElement` by `viewport.ts`:

| Name | Meaning | Default when JS/visualViewport absent |
|---|---|---|
| `--app-height` | current visible viewport height in px | unset → CSS falls back to `100dvh` |
| `.keyboard-open` (class) | soft keyboard currently visible | absent |

CSS consumers use `height: var(--app-height, 100dvh)`.

---

## 6. Task breakdown (red → green → refactor)

> Run `npm install` once at repo root first (worktree has no `node_modules`).
> Per-task loop command: `npm run web:test -- run` (vitest) and `npm run web:check` (svelte-check).

### Task 0 — Baseline green
**Goal:** prove the suite is green before touching anything.
- `npm install`
- `npm run web:test -- run` → all pass
- `npm run web:check` → no errors
- `npm run web:build && (cd crates/ajax-web && cargo test) && (cd crates/ajax-cli && cargo test)` → green
**Done when:** baseline recorded; if dev-DB hook flake appears (memory: dev_db_schema10_hook_flake) note it and proceed.

---

### Task 1 — `viewport.ts`: visualViewport → CSS var + keyboard class + guards
**New files:** `crates/ajax-web/web/src/viewport.ts`, `crates/ajax-web/web/src/viewport.test.ts`

**RED — `viewport.test.ts`** (mirror the `visualViewport` mock style already in `TerminalPanel.test.ts`):
- `initViewport()` sets `document.documentElement.style.getPropertyValue("--app-height")` to `${visualViewport.height}px` on call.
- On a `visualViewport` `resize` where height drops by `> 150`, `document.documentElement.classList.contains("keyboard-open")` becomes `true` and `--app-height` updates to the new height.
- On a subsequent `resize` back within `100px` of the initial baseline, `keyboard-open` is removed.
- While `keyboard-open`, a window `scroll` event triggers `window.scrollTo(0, 0)` (spy on `scrollTo`).
- `initViewport()` returns a cleanup fn that removes all listeners and the class.
- No-op safety: calling `initViewport()` with `window.visualViewport === undefined` does not throw and sets no var.

**GREEN — `viewport.ts`:**
```ts
export function initViewport(): () => void
```
- Read `visualViewport`; if absent, return a no-op cleanup.
- `setAppHeight()` → set `--app-height` from `visualViewport.height`.
- Track `initialHeight`; on `resize` apply the 150/100px thresholds, toggle `keyboard-open`, keep `--app-height` synced, update baseline only while keyboard closed.
- `scroll` (window) guard: if `keyboard-open`, `window.scrollTo(0,0)`.
- Register `visualViewport` `resize` + `scroll`, `window` `resize` + `scroll`; return cleanup that unregisters and removes the class.

**REFACTOR:** extract the 150/100 thresholds to named consts with a comment citing iOS address-bar drift.

**Verify:** `npm run web:test -- run viewport` green; `npm run web:check` clean.

---

### Task 2 — Wire `viewport.ts` into the app shell
**Edit:** `crates/ajax-web/web/src/components/App.svelte` (+ assertion in `App.test.ts`)

**RED — `App.test.ts`:** after `render(App)` with a mocked `visualViewport` (height e.g. 700), `document.documentElement.style.getPropertyValue("--app-height")` equals `"700px"`. (Guard the existing App tests still pass — they run in jsdom without `visualViewport`, so `initViewport` must no-op there; verify no new failures.)

**GREEN:** in App's existing `$effect`, call `const disposeViewport = initViewport();` and return it from the cleanup alongside the current teardown.

**REFACTOR:** none expected.

**Verify:** `npm run web:test -- run App` green.

---

### Task 3 — `TerminalPanel`: fill parent, momentum scroll, font, scroll-to-bottom
**Edit:** `crates/ajax-web/web/src/components/TerminalPanel.svelte` (+ `TerminalPanel.test.ts`)
**Blast radius (§4.5):** consumers = `TaskDetail.svelte` + `TerminalPanel.test.ts` only. No `api.ts` change.

**RED — `TerminalPanel.test.ts`:** add `scrollToBottom = vi.fn()` to the mock `Terminal`; assert:
- after an `output` message is written, `term.scrollToBottom()` is called (keeps newest output visible as the viewport resizes);
- after a `visualViewport` `resize` (existing test dispatches this), `scrollToBottom()` is also called following the refit.
Keep every existing assertion intact.

**GREEN:**
- Call `term.scrollToBottom()` after `term.write(...)` in the message handler and inside `scheduleRefit`'s rAF (after `fitAddon.fit()` + `sendResize()`).

**GREEN — CSS (scoped `<style>`):**
- `.terminal-panel`: remove the hard `height: 75dvh` / `min-height: 360px`; make it `flex: 1 1 auto; min-height: 0;` so the parent column drives height. Keep the desktop `@media (min-width: 768px)` block (`height: min(58vh, 560px)`) so desktop is unchanged.
- `:global(.terminal-panel .xterm-viewport)`: add `-webkit-overflow-scrolling: touch; overscroll-behavior: contain;`.
- Bump `fontSize: 13` → `14` in the `new Terminal({...})` options for readability (acceptable col count at phone widths).

**REFACTOR:** comment why `scrollToBottom` follows refit (viewport shrink must not strand the cursor above the fold).

**Verify:** `npm run web:test -- run TerminalPanel` green; `npm run web:check` clean.

---

### Task 4 — `TaskDetail`: mobile full-screen flex column
**Edit:** `crates/ajax-web/web/src/components/TaskDetail.svelte` (scoped `<style>` only; DOM unchanged to preserve tests)
**Blast radius (§4.5):** consumers = `App.svelte` (render path) + `TaskDetail.test.ts` only.

**RED:** existing `TaskDetail.test.ts` must stay green (renders pill, "Review", terminal panel, fires `onBack`). No new unit assertion — CSS layout is verified by the manual iOS checklist (§7) + the Rust snapshot staying green. Add a brief inline `<!-- -->` note so the intent is discoverable.

**GREEN — CSS, gated behind `@media (max-width: 767px)`:**
- `.task-detail`: `position: fixed; inset: 0; z-index: 30;` (above `.cockpit-chrome` z=10 and `.bottom-nav` z=20) `height: var(--app-height, 100dvh); display: flex; flex-direction: column; padding: env(safe-area-inset-top) ... env(safe-area-inset-bottom); overflow: hidden;`
- `.detail-header`: slim, `flex: none`, reduced margins.
- `.interact-panel` / `.next-action`: compact (tighter padding/margins) so they don't steal terminal height; keep visible (CLI-parity actions matter — memory: agent_orchestration_pivot).
- `TerminalPanel` slot: `flex: 1 1 auto; min-height: 0;` (the star).
- `.meta-details`: stays a collapsed `<details>` (≈1 line); `flex: none`.
- Desktop (`min-width: 768px`): leave the current `min-height: calc(100dvh - 148px)` flow layout untouched.

**REFACTOR:** ensure `.task-detail` mobile rule does not leak to desktop; confirm no `100vh` introduced.

**Verify:** `npm run web:test -- run TaskDetail` green.

---

### Task 5 — Global styles: zoom prevention + safe areas
**Edit:** `crates/ajax-web/web/src/styles.css`

**GREEN:**
- Add under a `@media (max-width: 767px)` block: `html { touch-action: manipulation; }`.
- (Optional, matches Codeman) add `gesturestart`/`gesturechange` `preventDefault` — but prefer placing this in `viewport.ts` (Task 1) so it's covered by tests rather than untested global JS. If added, extend the Task 1 RED to assert `preventDefault` is called on a dispatched `gesturestart`.
- Confirm inputs remain `font-size: 16px` (already at `styles.css:124-131`) — **do not** change.

**REFACTOR:** none.

**Verify:** `npm run web:check`; grep the file for `100vh` → must be absent.

---

### Task 6 — Rebuild bundle + Rust asset snapshots
**Goal:** the built `dist/` reflects new CSS/JS and Rust snapshot tests pass.
- `npm run web:build` (emits non-hashed `dist/app.js`, `dist/app.css`, `dist/index.html`).
- `npm run web:build:check`
- `(cd crates/ajax-web && cargo test)` → `install.rs` suite green; **specifically** `stylesheet_preserves_the_safari_first_visual_language` (the `100vh` ban + token checks).
- `(cd crates/ajax-cli && cargo test)` → web_backend snapshots green.
**Done when:** both crates green. If a snapshot is an intentional, reviewed change, update it deliberately — do not blanket-accept.

---

### Task 7 — Full verification gate
- `npm run web:test -- run` (all unit) → green
- `npm run web:check` → clean
- `npm run web:build:check` → ok
- `npm run web:smoke` (Playwright) if runnable in env → green
- Both Rust crates `cargo test` → green
- Manual iOS checklist (§7)

---

## 7. Manual verification — iOS Safari (real device or simulator)

`npm run web:dev`, open over LAN on an iPhone in Safari, open a task:

- [ ] Terminal fills the screen; header/status/actions are compact above it; bottom nav is covered.
- [ ] Scrolling terminal history is smooth (momentum); the page itself does **not** scroll/rubber-band behind it.
- [ ] Tap into the terminal → keyboard opens → cursor row **and** the control-key bar (`Esc Tab ⌃C ← ↑ ↓ → Ctrl`) remain visible directly above the keyboard.
- [ ] Typing a long line keeps the cursor visible (no scroll-away).
- [ ] Rotate portrait↔landscape → terminal refits, no dead space, no overflow.
- [ ] Dismiss keyboard → terminal grows back, output still pinned to bottom.
- [ ] Double-tap and pinch do not zoom; focusing input does not zoom.
- [ ] Desktop browser at ≥768px is visually unchanged (inline panel, `min(58vh,560px)`).

## 8. Commit plan (atomic, no Co-Authored-By)

1. `feat(web): add visualViewport keyboard-aware viewport helper` (Tasks 1–2)
2. `feat(web): make task terminal fill viewport with momentum scroll` (Task 3)
3. `feat(web): full-screen mobile task detail layout` (Tasks 4–5)
4. `chore(web): rebuild dist for mobile terminal overhaul` (Task 6)

> Release note: a `feat`-titled PR cuts a release (memory: release_trigger_convention); use `feat` intentionally.

## 9. Rollback

Each task is isolated. Revert order 4→1. The CSS-only tasks (3 CSS / 4 / 5) are independently revertible; `viewport.ts` no-ops without `visualViewport`, so reverting Task 2's wiring fully disables the new behaviour without touching layout.

## 10. Out of scope (note, don't build)

- Swipe-to-switch-session (Codeman has it; ajax is one-terminal-per-route).
- Slash-command quick keys (`/init /clear /compact`) in the key bar — possible follow-up.
- CJK/IME composition handling — revisit only if reported.
