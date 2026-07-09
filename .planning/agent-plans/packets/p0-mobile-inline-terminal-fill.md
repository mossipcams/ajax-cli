# TDD Packet: Priority 0 — mobile inline terminal fills band

## 1. Goal

On mobile (and landscape-phone coarse/short viewports), the non-expanded task
terminal panel must flex-fill available height instead of capping at
`max-height: min(58vh, 560px)`, which leaves a large blank band and can hide
the typing/cursor row while the bottom key bar stays visible.

## 2. Allowed files

**Tests**

- `crates/ajax-web/web/src/styles.css` — only if a co-located source contract
  test lives elsewhere; prefer adding assertions in:
- `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- `crates/ajax-web/web/e2e/layout-scroll.test.ts` (optional one assertion if
  vitest source contracts are insufficient)

**Production**

- `crates/ajax-web/web/src/styles.css`

**Build artifacts only if required for green asset/snapshot tests after CSS change**

- `crates/ajax-web/web/dist/*` via `npm run web:build` (do not hand-edit dist)

## 3. Forbidden changes

- Do not edit `TerminalRawView.svelte` desktop `@media (min-width: 768px)`
  `height/max-height: min(58vh, 560px)` rules — those stay for desktop.
- Do not change fullscreen / `html.terminal-expanded` fixed overlay rules.
- Do not change keyboard-open chrome-hide rules except clearing mobile
  `max-height` on the panel if needed for fill.
- Do not touch paste/copy/gesture code (Priorities 1–3 are separate packets).
- Do not weaken existing overflow:hidden / route-scroll assertions.
- Do not change Rust crates, architecture.md, or unrelated CSS.

## 4. Architecture context

Web Cockpit UI only. Layout authority: `AppViewport` owns `--app-band-*`;
mobile task route uses `RouteScroll` + `TaskDetail` flex column;
`TerminalRawView` panel sits in `.terminal-primary`. Core/registry/terminal
substrate ownership unchanged. Blank space is a CSS max-height conflict, not
Ghostty.

## 5. Code anchors

**Bug (global 58vh cap — applies on mobile today):**

```558:564:crates/ajax-web/web/src/styles.css
.task-detail .terminal-panel,
.task-detail [data-testid="task-terminal-panel"] {
  flex: 1 1 auto;
  min-height: 280px;
  max-height: min(58vh, 560px);
  overflow: hidden;
}
```

**Mobile block already clears min-height but not max-height:**

```587:595:crates/ajax-web/web/src/styles.css
  .task-detail .terminal-panel,
  .task-detail .task-terminal-viewport {
    min-height: 0;
  }

  .task-detail [data-testid="task-terminal-panel"] {
    min-height: 200px;
  }
```

**Desktop-only height already correct in component:**

```1122:1127:crates/ajax-web/web/src/components/TerminalRawView.svelte
  @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
    .terminal-panel:not(.is-expanded) {
      height: min(58vh, 560px);
      max-height: min(58vh, 560px);
    }
  }
```

**Reuse test style:** source-regex contracts in `TaskDetail.test.ts`
(`defines mobile overlay height pins…`) and
`TerminalRawView.test.ts` desktop 58vh assertion (~line 2224).

## 6. Test-first instructions

1. Add a test in `TaskDetail.test.ts` (or a small styles contract test) that
   reads `styles.css` source (same `?raw` / readFile pattern already used for
   `taskDetailSource` / `terminalRawViewSource` — if styles are not imported
   raw yet, import `../styles.css?raw` or read via the same mechanism sibling
   tests use).

2. New test name:
   `mobile task terminal panel clears the 58vh max-height so it can flex-fill`

3. Assertions (must fail before impl):
   - The **base** (non-mobile) rule may still mention `58vh` for desktop, OR
     better: move 58vh into a desktop media query and assert:
   - Inside the mobile media block
     `@media (max-width: 767px), (pointer: coarse) and (max-height: 500px)`
     in `styles.css`, the task terminal panel selector sets
     `max-height: none` (or equivalent clear).
   - The mobile block must **not** leave the panel inheriting only the global
     `max-height: min(58vh, 560px)` without an override.
   - Keep asserting desktop `TerminalRawView.svelte` still has
     `min(58vh, 560px)` under `min-width: 768px`.

4. Run and confirm RED:
   ```bash
   cd crates/ajax-web/web && npm run web:test -- --run TaskDetail.test.ts
   ```

## 7. Production edit instructions

In `crates/ajax-web/web/src/styles.css`:

**Option A (preferred, minimal):**

1. Remove `max-height: min(58vh, 560px);` from the global
   `.task-detail .terminal-panel` / `[data-testid="task-terminal-panel"]` rule
   (keep `flex`, `min-height`, `overflow`).

2. Add a **desktop-only** media query matching TerminalRawView’s desktop gate:
   ```css
   @media (min-width: 768px) and (not ((pointer: coarse) and (max-height: 500px))) {
     .task-detail .terminal-panel,
     .task-detail [data-testid="task-terminal-panel"] {
       max-height: min(58vh, 560px);
     }
   }
   ```

3. In the existing mobile media block, explicitly set:
   ```css
   .task-detail .terminal-panel,
   .task-detail [data-testid="task-terminal-panel"] {
     max-height: none;
   }
   ```
   (May merge with the existing `min-height: 200px` rule.)

Do not change expanded fixed-panel `max-height: none` rules (already present).

After CSS is green in vitest, run `npm run web:build` if install/asset tests
require dist sync; only then touch dist via the build script.

## 8. Verification commands

```bash
cd crates/ajax-web/web && npm run web:test -- --run TaskDetail.test.ts
cd crates/ajax-web/web && npm run web:test -- --run TerminalRawView.test.ts
cd crates/ajax-web/web && npm run web:check
cd crates/ajax-web/web && npm run web:build
cd crates/ajax-web/web && npx playwright test e2e/layout-scroll.test.ts
```

From repo root if asset snapshots complain:
```bash
cargo nextest run -p ajax-web
```

## 9. Acceptance criteria

- New mobile max-height contract fails before CSS change, passes after.
- Mobile non-expanded panel is not capped at 58vh in `styles.css`.
- Desktop still caps at `min(58vh, 560px)` (styles desktop media and/or
  TerminalRawView desktop media).
- layout-scroll e2e still passes.
- No paste/copy/gesture files changed.

## 10. Stop conditions

- Stop if fixing blank space requires Ghostty/fit/scroll changes beyond CSS.
- Stop if desktop and mobile media queries cannot be aligned without editing
  TerminalRawView.svelte — ask parent before expanding Allowed files.
- Stop if unrelated tests fail for reasons outside this packet.
- Do not implement Priorities 1–3 in this packet.
