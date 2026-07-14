# Packet R8 — L1: 44×44 tap targets (Apple HIG) for keys and critical buttons

All paths relative to `crates/ajax-web/web`; run all commands from there.

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Controls asserted by `e2e/explore-webkit-qa.test.ts` must have ≥44×44 CSS px
bounding boxes and be their own hit targets on mobile-webkit (iPhone 12,
390×844): every terminal key (Esc, Tab, ⌃C, Ctrl, arrows, Paste, Hide
keyboard), Settings "Restart server" and "Run diagnostics", and the
connection-status "Retry" button. The three deep-explore tests must pass end
to end (they also exercise navigation, expand, keyboard band, landscape).

## 3. Allowed files

- `src/components/TerminalRawView.svelte` — `<style>` only (`.terminal-key`,
  `.terminal-keys` sizing)
- `src/components/SettingsView.svelte` — `<style>` only
- `src/components/ConnectionStatus.svelte` — `<style>` only
- `src/styles.css` — only if one of those buttons is styled there

## 4. Forbidden changes

- No markup/JS changes anywhere.
- No key removed/reordered; toolbar behavior (wrap) stays.
- Desktop (>767px fine-pointer) visual density: prefer scoping bumps to the
  existing mobile media patterns where the rule already lives in one; a global
  bump is acceptable only for the three standalone buttons (Retry/Restart/
  diagnostics) if it does not break desktop visual tests.
- No e2e file edits.

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — CSS sizing on presentation controls.
- **Serena:** NOT_REQUIRED — direct-read anchors below.
- **ast-grep:** NOT_REQUIRED — CSS-only edit.

## 6. Code anchors

- `src/components/TerminalRawView.svelte`: base `.terminal-key`
  (`min-width: 38px; min-height: 28px; padding: 3px 7px; font-size: 11px`),
  phone media block override (`min-height: 28px; padding: 1px 7px`), and the
  trailing `@media (max-width: 767px)` block from the C3 fix
  (`.terminal-keys { flex-wrap: wrap; overflow-x: visible } .terminal-key
  { flex: 1 1 0; min-width: 0 }`). With 44px minimums the wrapped row becomes
  2 rows at 390px — fine; the terminal refits rows dynamically (C4 fix).
  NOTE: `min-width: 0` in that block currently lets keys shrink below
  min-content; the L1 bump must give keys ≥44px width at 390 AND 320 viewports
  (e.g. `flex: 1 1 auto; min-width: 44px; min-height: 44px` in the wrap
  block).
- `src/components/SettingsView.svelte`: Restart / Run diagnostics buttons
  (measured height ~36).
- `src/components/ConnectionStatus.svelte`: `.terminal-status-reconnect`-like
  Retry button (small measured hits) — actual class in this component; locate
  by role name "Retry".
- Tests: `e2e/explore-webkit-qa.test.ts` `MIN_TAP_PX = 44` (:15),
  `assertMinTapTarget` (:66–71), the three tests at :198, :290, :335.

## 7. Test-first instructions

Existing failing tests are the red:

```bash
npx playwright test e2e/explore-webkit-qa.test.ts -g "deep explore" --project=mobile-webkit
```

Expect three failures mentioning `key Esc tap width < 44` / `Restart` /
`Retry` before the edit.

## 8. Edit instructions

1. Terminal keys: in the trailing wrap media block raise to
   `min-width: 44px; min-height: 44px` (adjust `flex` so keys stay even and
   wrap to 2 rows); neutralize the older 28px phone override so it cannot win
   the cascade. Keep font size readable (≥12px is fine).
2. Settings Restart + Run diagnostics: `min-height: 44px` (and horizontal
   padding so width ≥44).
3. Connection Retry (and Copy Diagnostics if shared class): `min-height: 44px`,
   `min-width: 44px`.
4. Do not touch expand-corner or action-bar buttons unless a deep-explore
   assertion names them (review/drop already pass).

## 9. Verification commands

```bash
npx playwright test e2e/explore-webkit-qa.test.ts --project=mobile-webkit
npx playwright test e2e/explore-terminal-visual.test.ts e2e/explore-keyboard-blank-jump.test.ts e2e/explore-c4c5-siblings.test.ts e2e/explore-webkit-critical.test.ts e2e/fullscreen-refit.test.ts --project=mobile-webkit
npx playwright test --project=mobile-webkit
npx playwright test --project=desktop-chromium
npx vitest run
```

## 10. Acceptance criteria

- RED (three deep-explore failures) shown before; after the edit all of
  `explore-webkit-qa.test.ts` passes (including the viewport-meta pin, which
  is already green).
- The focused terminal suites stay green (taller key bar must not regress C2,
  C3, C4/C6, C5, landscape, narrow-phone probes — the C4 local refit should
  absorb the height change).
- Full mobile-webkit run: zero failures expected now; report any remainder
  with evidence.
- Desktop-chromium run: no new failures (visual.test.ts included).
- Full vitest suite passes.
- Diff confined to `<style>` blocks / styles.css.

## 11. Stop conditions

- A 44px key bar makes a C-defect suite fail and fixing it needs JS changes.
- Desktop visual tests regress and the fix requires markup changes.
- Patch exceeds ~120 changed lines.
