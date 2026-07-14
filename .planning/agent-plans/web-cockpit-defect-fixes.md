# Web Cockpit defect fixes — execution plan

Source: `.planning/agent-plans/web-cockpit-defect-list.md` (2026-07-13 Playwright/Semgrep pass).
Delegation decision: **delegated via model-router** (per AGENTS.md default; bounded code changes).
Mode: Behavior Change (failing e2e repros already exist per defect).

## Scope

Fix C1–C6, S1, M1, M2 in `crates/ajax-web/web`. Non-goals: L1 tap-target
resize (conflicts with C3 key-row width; deferred, non-blocking), terminal
model changes, any backend/Rust change.

## Root-cause analysis (verified against source)

- **C1** — `NewTaskSheet.svelte`: sheet card is `overflow-y:auto` with
  `max-height: calc(100% - 40px)`. In a 400px keyboard band the card scrolls
  internally and `.sheet-actions` (Start) sits below the fold (y≈483 vs band
  end 450). FullscreenLayer band-tracking itself is correct.
  Fix: make `.sheet-actions` `position: sticky; bottom: 0` with card
  background (+ small negative-margin/padding so content scrolls under it).
- **C2** — `TerminalRawView.svelte:1547` `.terminal-status.is-empty {
  visibility: hidden }` reserves 28px + flex gap ≈ 41px dead space.
  Fix: `display: none` (the keyboard-open rule at :1557 already does this).
- **C3** — `.terminal-keys` row (Esc Tab ⌃C ← ↑ ↓ → Ctrl Paste ⌄) overflows
  390px with invisible `overflow-x:auto`; Paste/Hide-keyboard unreachable.
  Fix: at phone widths let the row fit: `flex-wrap: wrap` is simplest but
  costs a second row's height under keyboard; preferred: shrink per-key
  `min-width`/padding at ≤430px and let keys flex (`flex: 1 1 0`) so all 10
  fit one row; keep `⌄` (Hide keyboard) pinned last and always visible.
  Delegate iterates against the failing test at 390/375/320.
- **C4/C6** — `fitNow` (`TerminalRawView.svelte:657`): when
  `allowLocalFit:false` (keyboard-open, `terminalLayoutPolicy.ts:43`) it only
  does `container.scrollTop = scrollHeight - clientHeight` (crop-to-bottom of
  the pre-keyboard canvas). Fresh shells have the prompt near row 0 → prompt
  scrolls off the top, empty rows fill the band. Host shrink variants
  (reconnect strip, paste fallback, keyboard settle) deepen the crop.
  Fix direction: while keyboard-open, crop to **content** not canvas bottom —
  offset by cursor/content extent (clamp so the cursor row lands just above
  the key bar and never crops content above the host when content fits), and
  re-run that crop on host resize while frozen. PTY resize stays withheld
  (no SIGWINCH until keyboard closes / discrete intent).
- **C5** — keyboard dismiss restores `display:none` chrome (`styles.css:639`,
  `:651`) → host jumps 49→146. `viewport.ts:94` also `resetDocumentScroll()`.
  Fix direction: on keyboard close, scroll-compensate the route scroller
  (`[data-testid="route-scroll"]`) so the terminal host keeps its viewport y
  (clamped to scrollable range) instead of resetting to top.
- **S1** — Svelte escapes interpolation (no XSS), but JWT-shaped strings in
  `status_explanation`/`title` are displayed verbatim → visible in rendered
  HTML. Fix: redact JWT-shaped substrings (three dot-joined base64url segments
  starting with "eyJ") in display
  fields at the API mapping layer (`api.ts` / where cockpit+detail responses
  are adapted) with a small unit-tested `redactJwts()` helper.
- **M1** — `app.html:5` `interactive-widget=resizes-content` — WebKit rejects
  with console noise; iOS handling lives in `viewport.ts` anyway. Chromium
  honors it and it costs nothing there. Fix: keep behavior but silence WebKit
  is not possible via meta — decision: remove the key only if the C4/C5 work
  proves `viewport.ts` fully covers Chromium too; otherwise accept + document.
  Default: leave as-is, mark defect wont-fix with rationale (console-only).
- **M2** — narrow 320px scaled canvas leaves ~15px blank (`fillY≈0.94`) —
  rounding in `scaledLogicalRows`/`fitScale` path; fold into C4 round since it
  touches the same fit math (`terminalGeometry.ts`).

## Delegation rounds (one bounded task each, sequential)

| # | Task | Files | Repro (must go green) | Status |
|---|------|-------|----------------------|--------|
| R1 | C2 empty-status dead space | TerminalRawView.svelte (CSS) | `explore-terminal-visual.test.ts -g "empty status"` | **done** — routed LOCAL (≤10-line CSS rule), repro red→green |
| R2 | C1 sticky sheet actions | NewTaskSheet.svelte (CSS) | `explore-webkit-critical.test.ts -g "Start remains hittable"` | **done** — routed LOCAL (small CSS), repro red→green, 15 unit tests pass |
| R3 | C3 key row fits phone widths | TerminalRawView.svelte | `explore-terminal-visual.test.ts -g "Paste and Hide keyboard"` | **done** — cursor-delegate (composer-2.5), red→green proven, gate ACCEPT |
| R4 | C4+C6+M2 keyboard-band crop/fit | TerminalRawView.svelte, terminalLayoutPolicy.ts, terminalGeometry.ts | `explore-keyboard-blank-jump.test.ts -g "cropped empty band"`, `explore-c4c5-siblings.test.ts`, `explore-terminal-visual.test.ts -g "narrow phone"` | **done** — cursor-delegate; local-only row refit (`applyFrozenKeyboardBandFit`); gate REVISE→ACCEPT (see deviations) |
| R5 | C5 dismiss jump anchor | TaskDetail.svelte (CSS order) | `explore-keyboard-blank-jump.test.ts -g "must not jump"` + `-g "new-task handoff"` | **done** — cursor-delegate; CSS `order` anchors terminal under cockpit bar, header/interact below; gate ACCEPT (72/6 mobile, 509 vitest) |
| R6 | S1 JWT redaction in display fields | api.ts + api.test.ts | `jwt-adversarial.test.ts` (+ `explore-ui` JWT half) | **done** — cursor-delegate; `redactJwts` at readJson boundary; gate ACCEPT after local 1-line fix (error-text path also redacted); all JWT suites + 513 vitest green |
| R7 | M1 viewport meta interactive-widget | app.html + dist rebuild | `explore-webkit-qa.test.ts -g "DEFECT pin: viewport meta"` + `explore-ui` console half | **done** — LOCAL; key removed, `npm run web:build` refreshed vendored dist, both repros green, ajax-web nextest 128/128 |
| R8 | L1 tap targets ≥44px | TerminalRawView/SettingsView/ConnectionStatus CSS | `explore-webkit-qa.test.ts` (3 deep-explore tests) | **done** — cursor-delegate (honest FAILED report: two blockers outside its scope) + two local corrections; gate ACCEPT |

Each round: model-router ROUTING_DECISION → tdd-implementation-packet →
delegate → I review diff → I run the repro + `npm test` (vitest) myself.

## Validation

- Per-round Playwright repro (`npx playwright test <file> --project=mobile-webkit`)
- `npm run check` / unit tests in `crates/ajax-web/web`
- `cargo nextest run -p ajax-web` if Rust assets/snapshots touched (they should not be)

## Risks

- C4 crop-to-content needs care not to fight scrollFollow pinning or pinch/expand exemptions.
- C5 scroll compensation must clamp when route-scroll lacks range; test threshold 24px.
- C3 shrinking keys trades against L1 (HIG 44px) — record the tension, don't resolve it here.

## Deviations

- Worktree needed `npm install` + `npx playwright install webkit` before repros would run.
- R1 (C2): model-router routed LOCAL (one-file ≤10-line CSS rule; no delegation needed). Repro confirmed red first, then green after `visibility:hidden` → `display:none`. I also deleted the keyboard-open `display:none` rule as redundant — wrong: a unit contract pins it (`TerminalRawView.test.ts` "collapses empty terminal status while keyboard is open"). Restored during the R3 gate.
- R2 (C1): routed LOCAL. Sticky `.sheet-actions` footer (negative margins cancel card padding; 380px media block mirrors the 18px padding). Repro red→green.
- R3 (C3): user directed all remaining rounds through delegate lanes. cursor-delegate/composer-2.5 added a ≤767px grid (repeat(5,1fr), overflow visible, min-width 0) — all 10 keys visible in two rows. Delegate's verify caught the C2 fallout above plus the landscape blank-band probe: pre-existing C4-family (spacer removal grew the host; frozen fit leaves 41px unfilled). Landscape probe added to R4 acceptance.
- R4 (C4/C6): cursor-delegate replaced the frozen bottom-crop with `applyFrozenKeyboardBandFit` (local `term.resize` rows-only, no `sendResize`; content-aware viewport scroll). All target repros green, vitest 509/509. Gate found a regression the packet's verify list missed: `fullscreen-refit.test.ts` "expand button stays in viewport" — the R3 **grid** (not R4) shrank the host to 290px at 390 viewport. Verdict REVISE; corrected locally per route table (≤10-line CSS): grid → `flex-wrap: wrap` + `flex: 1 1 0`; all 17 affected tests green. Full mobile-webkit suite now 70 pass / 8 fail, every fail mapped to an open defect (2×C5, S1, 2×M1, 3×L1).
- M1 correction: `explore-webkit-qa.test.ts` has an explicit DEFECT-pin test requiring `interactive-widget` removed from the viewport meta — original wont-fix stance withdrawn; iOS keyboard handling lives in viewport.ts and does not need the key. Chromium `resizes-content` behavior loss is accepted (WebKit is the target).
- L1 correction: webkit-qa deep-explore tests pin 44×44 tap targets (terminal keys, connection Retry, settings Restart, new-task controls); L1 moves from deferred to R8. Terminal keys at 44px min-width force the wrapped key row to 2 rows — acceptable with R4's dynamic row refit.
- C5 design note (needs Matt's eyes): the only geometry that satisfies "dismiss must not jump the host" with an unscrollable task route is anchoring the terminal directly under the cockpit bar and rendering detail-header + interact strip BELOW the terminal on mobile (CSS order only). This is a visible task-route layout change, consistent with the terminal-first direction; flagged for review.
- R8 (L1): delegate raised keys/Retry/Restart/diagnostics to ≥44px and (deviation) bumped `.terminal-expand-corner` z-index 5→45 to fix a real occlusion its test surfaced (drop-error toast covering the expand button). Two blockers were outside its allowed files, fixed locally at the gate: (1) `TerminalRawView.test.ts` mobile characterization updated 28px→44px (pins the new HIG contract — deliberate contract change, NOT weakening; e2e enforces 44), (2) `explore-webkit-qa.test.ts` connection test gained `mockFetch(page)` so unmocked /api calls stop leaking through the vite proxy to the live dev server and 401-ing the console (fixture gap, no assertion changed — disclosed test-file edit).
- M2 verified green in the final full run (repro `-g "narrow phone"` passes; fixed as a side effect of R1 spacer removal + R3 row wrap + R4 refit).
- Vendored dist rebuilt twice (after R7 meta change, and again after R8 CSS) — `cargo nextest run -p ajax-web` 128/128 both times.

## Final validation (2026-07-13)

- `npx playwright test --project=mobile-webkit` — **78 passed, 0 failed**
- `npx playwright test --project=desktop-chromium` — **52 passed, 0 failed**
- `npx vitest run` — **513 passed, 0 failed**
- `cargo nextest run -p ajax-web` — **128 passed**
- `npm run web:check` (tsc + svelte-check) — **0 errors, 0 warnings**
- Not run: full `npm run verify` (repo-wide cargo fmt/clippy/nextest --all-features) — Rust sources untouched (only vendored dist bytes changed); run it before commit if desired.
