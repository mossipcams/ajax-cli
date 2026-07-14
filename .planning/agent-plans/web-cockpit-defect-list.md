# Web Cockpit defect list

Open defects found in the ajax/jwts Playwright / Semgrep pass (2026-07-13).
Status: **open** until fixed and the cited repro goes green.

> **2026-07-13 fix pass complete** — all defects below (C1–C6, S1, M1, M2, L1)
> are FIXED; every cited repro plus the full mobile-webkit (78) and
> desktop-chromium (52) suites, vitest (513), and ajax-web nextest (128) are
> green. Execution ledger: `.planning/agent-plans/web-cockpit-defect-fixes.md`.
> Notable: C5's fix reorders the mobile task route (terminal anchored under the
> cockpit bar; detail header + interact strip now render BELOW the terminal) —
> review that UX change.

Harsh re-pass notes: prior “zero defects” terminal visual run was under-specified.
Adversarial geometry + key-row visibility hunts found real failures below.
Canvas-tap→contentEditable was a **false positive** (Ghostty’s scale-layer is
`contentEditable` + `role=textbox` and keystrokes reach `__terminalFrames`).

Operator follow-up (new-task blank above keyboard + delayed jump): confirmed as
**C4** + **C5** via mocked `visualViewport` keyboard open/close.

---

## Critical

### C1 — New Task Start sits below the soft-keyboard band (WebKit)

- **Severity:** Critical — blocks primary create-task flow on iPhone Safari while keyboard is open
- **Surface:** Web Cockpit · mobile WebKit
- **Repro:**
  1. `playwright test e2e/explore-webkit-critical.test.ts -g "Start remains hittable" --project=mobile-webkit`
  2. Or manually: New task → focus title → soft keyboard open → Start is below the visible band
- **Evidence:** Start bottom **y≈483.5** vs keyboard band end **450** (`--app-top:50`, `--app-height:400`)
- **Likely code:** `NewTaskSheet.svelte` / sheet layout under `html.keyboard-open`
- **Status:** fixed (R2) — sticky sheet actions

### C2 — Empty terminal status reserves ~41px dead space above key bar

- **Severity:** High visual/layout — keys look detached from the PTY; steals scrollback height on every connected task view (portrait, landscape, after expand/collapse)
- **Surface:** Web Cockpit · mobile WebKit (all viewports using `.terminal-status.is-empty`)
- **Repro:**
  1. `playwright test e2e/explore-terminal-visual.test.ts -g "empty status" --project=mobile-webkit`
- **Evidence:** host→keys gap **41px** = controls `padding-top:6` + empty status `min-height:28` (`visibility:hidden`) + flex `gap:6`. Status is correctly `display:none` only under `html.keyboard-open`.
- **Likely code:** `TerminalRawView.svelte` — `.terminal-status.is-empty { visibility: hidden }` → should be `display: none`
- **Status:** fixed (R1) — is-empty display:none

### C3 — Paste and Hide keyboard clipped off the visible key row (iPhone width)

- **Severity:** Critical / High — on a standard 390×844 viewport the **Paste** and **Hide keyboard** buttons sit past the right edge of the key row. The row uses scrollbar-less `overflow-x: auto`, so there is no affordance. Hide keyboard is the designed iOS soft-keyboard dismiss path and is invisible precisely when the keyboard is open.
- **Surface:** Web Cockpit · mobile WebKit · terminal key toolbar
- **Repro:**
  1. `playwright test e2e/explore-terminal-visual.test.ts -g "Paste and Hide keyboard" --project=mobile-webkit`
- **Evidence (390px, keyboard-open):** Paste `left≈353 right≈403` vs rowRight **382**; Hide keyboard `left≈407 right≈445` (fully off-screen). Same clipping on iPhone SE; worse on 320px (Ctrl also clipped).
- **Likely code:** `TerminalRawView.svelte` `.terminal-keys` / `.terminal-key` sizing — too many keys for phone width without wrapping, priority pinning, or a visible overflow cue
- **Status:** fixed (R3) — key row wraps at phone widths

### C4 — Soft keyboard freezes fit → cropped empty band above keys (new-task / focus)

- **Severity:** Critical — after creating a task (or tapping the PTY), the soft keyboard opens and the visible terminal above the key bar is mostly empty; the CLI prompt is supposed to sit just above the keyboard.
- **Root cause:** `terminalLayoutPolicy` sets `allowLocalFit: false` while `keyboard-open` (avoids SIGWINCH spam). `fitNow` then only `cropToBottom` on the **pre-keyboard** oversized canvas. Fresh shells keep the prompt near row 0; crop scrolls that off the top (~**172px** above the host) and leaves empty lower rows in the band.
- **Surface:** Web Cockpit · mobile WebKit · especially new-task → focus terminal / keyboard still up
- **Repro:**
  1. `playwright test e2e/explore-keyboard-blank-jump.test.ts -g "cropped empty band" --project=mobile-webkit`
- **Evidence:** `canvasAboveHost≈172`, `fillY≈1.46`, `canvas.top≈-123` with keyboard band host height ~374
- **Likely code:** `terminalLayoutPolicy.ts` (`allowLocalFit`) + `TerminalRawView.svelte` `fitNow` early-return / crop path — need a **local-only** reflow for the keyboard band (or pin-to-cursor) without PTY SIGWINCH thrash
- **Status:** fixed (R4) — local-only row refit in the keyboard band

### C5 — Terminal jumps down ~100px when soft keyboard dismisses

- **Severity:** Critical / High — “after a few seconds of use” the terminal suddenly drops. Matches keyboard dismiss (Hide keyboard, iOS settle, or leaving the field): chrome that was `display:none` under `keyboard-open` returns (detail header + interact + bottom-nav) and pushes the host from **y≈49 → y≈146** (~**97–105px** jump).
- **Surface:** Web Cockpit · mobile WebKit · task terminal after keyboard open→close
- **Repro:**
  1. `playwright test e2e/explore-keyboard-blank-jump.test.ts -g "must not jump" --project=mobile-webkit`
  2. Also fails on new-task handoff dismiss (`-g "new-task handoff"`)
- **Evidence:** `hostTop` 49→146 (mid-session) or 41→146 (new-task handoff); header `display` none→flex
- **Likely code:** `styles.css` keyboard-open chrome collapse is correct for typing, but there is no stable terminal anchor / reserved chrome slot — dismiss restores ~100px of chrome above the PTY in one frame
- **Status:** fixed (R5) — terminal anchored; chrome reordered below (UX change)

### C6 — Host shrink under keyboard-open deepens the C4 crop (reconnect / paste fallback)

- **Severity:** High — same freeze as C4; any UI that steals host height while the keyboard is up makes the empty band worse because local fit cannot run.
- **Variants:**
  1. **Reconnect status** — “Reconnecting… / Reconnect” appears → host **374→336**, crop **172→210**
  2. **Paste fallback** — clipboard unavailable opens paste UI → host **374→306**, crop **172→240** (+68px)
  3. **Keyboard height settle** — iOS keyboard animation (480→420) grows crop **112→172**
- **Surface:** Web Cockpit · mobile WebKit · keyboard-open task terminal
- **Repro:**
  1. `playwright test e2e/explore-c4c5-siblings.test.ts --project=mobile-webkit`
- **Likely code:** same as C4 — allow **local** refit (or CSS-scale-only reflow) when host size changes under `keyboard-open`, still withholding PTY SIGWINCH until settle
- **Status:** fixed (R4) — frozen fit re-crops to content on host resize

---

## High (security)

### S1 — JWT-shaped API text rendered into HTML

- **Severity:** High — if a JWT ever appears in API display fields (`status_explanation`, `title`), it is reflected into the DOM
- **Surface:** Web Cockpit · all browsers
- **Repro:** `playwright test e2e/jwt-adversarial.test.ts --project=desktop-chromium` (or mobile-webkit)
- **Evidence:** hostile canary in cockpit/detail fields appears in `document.documentElement.outerHTML`
- **Status:** fixed (R6) — redactJwts at the api.ts fetch boundary

---

## Medium / low (compat, visual, HIG)

### M1 — Viewport meta `interactive-widget` rejected by WebKit

- **Evidence:** console `Viewport argument key "interactive-widget" not recognized and ignored.`
- **File:** `crates/ajax-web/web/app.html`
- **Status:** fixed (R7) — interactive-widget removed; dist rebuilt

### M2 — Narrow (320×568) scaled canvas leaves ~15px blank band

- **Severity:** Medium visual — host not fully filled on very small phones
- **Repro:** `playwright test e2e/explore-terminal-visual.test.ts -g "narrow phone" --project=mobile-webkit`
- **Evidence:** `blankBelow≈15`, `fillY≈0.94` on 320×568
- **Status:** fixed — repro green after R1/R3 side effects

### L1 — Undersized tap targets (Apple HIG 44×44)

- Terminal keys: CSS `min-width:38; min-height:28` (`TerminalRawView.svelte`)
- Connection Retry / status reconnect: small measured hits
- Settings Restart: measured height ~36
- **Status:** fixed (R8) — 44px keys/Retry/Restart/diagnostics; mobile characterization updated

---

## Harsh terminal visual suite results

```bash
playwright test e2e/explore-terminal-visual.test.ts --project=mobile-webkit
playwright test e2e/explore-keyboard-blank-jump.test.ts --project=mobile-webkit
```

| Probe | Result |
| --- | --- |
| Empty status dead spacer | **FAIL → C2** |
| Paste + Hide keyboard visible @390 | **FAIL → C3** |
| Narrow 320 blank band | **FAIL → M2** |
| Keyboard cropped empty band | **FAIL → C4** |
| Keyboard dismiss host jump | **FAIL → C5** |
| New-task handoff dismiss jump | **FAIL → C5** |
| Reconnect / paste-fallback deepen crop | **FAIL → C6** |
| Keyboard height settle grows crop | **FAIL → C6** (C4 aggravation) |

## Closed / not reproduced (sibling hunt)

- Drop confirm does **not** leak across task A→B hash navigation
- Expand hit-target after scale + landscape: OK
- Canvas tap “dead focus”: **false positive** — Ghostty contentEditable works
- Zero-lag overlay outside canvas: not reproduced
- Keys overlapping bottom-nav: not reproduced (~17px clearance)
- Expand → collapse host jump: **not a defect** — restores to pre-expand `hostTop` (fullscreen→chrome is intentional)
- Expand while keyboard-open: refit window clears crop (workaround path works)
- `visualViewport.offsetTop` drift without keyboard: no chrome-sized yank
- Task hash remount while keyboard-open: mild crop only (~10px), not C4-scale
- Orientation flip after keyboard: returns to normal fill (only residual **C2** spacer)
