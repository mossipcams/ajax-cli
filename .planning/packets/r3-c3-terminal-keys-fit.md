# Packet R3 — C3: terminal key row must fit phone widths without hidden pan

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

On phone-width viewports (390×844 primary, 320×568 must not regress) with the
soft keyboard open, every button in the terminal key toolbar — including
`Paste` and `Hide keyboard` (`⌄`) — must sit fully inside the key-row rect and
the window, with no horizontal panning of a scrollbar-less `overflow-x: auto`
row. CSS-only change.

## 3. Allowed files

- `crates/ajax-web/web/src/components/TerminalRawView.svelte` — `<style>` block only

## 4. Forbidden changes

- Any markup or `<script>` change in `TerminalRawView.svelte` (button order,
  labels, handlers, `role`/`aria-*` stay identical)
- Removing or hiding any key
- Any other component, `styles.css`, PTY/fit/scroll logic
- Any file under `e2e/` or any test file

## 5. Context evidence

- **Graphify:** NOT_REQUIRED — single-component presentation change; no
  cross-boundary blast radius (key handlers, PTY, layout policy untouched).
- **Serena:** NOT_REQUIRED — no symbol-level change; anchors are exact CSS
  selectors collected by direct file read (lines cited below).
- **ast-grep:** NOT_REQUIRED — edit targets CSS inside a Svelte style block;
  structural code search does not apply. Anchors verified with `rg` + Read.

## 6. Code anchors

`crates/ajax-web/web/src/components/TerminalRawView.svelte`:

- ~1185–1216: toolbar markup — `div.terminal-keys[role=toolbar]` containing
  10 `button.terminal-key`: Esc, Tab, ⌃C, ←, ↑, ↓, →, Ctrl, Paste,
  `⌄` (aria-label "Hide keyboard"). Do not edit.
- ~1477: base rule
  ```css
  .terminal-keys { display: flex; gap: 4px; overflow-x: auto; /* scrollbar hidden */ padding: 2px 4px; background: var(--paper); }
  ```
- ~1488: `.terminal-keys::-webkit-scrollbar { display: none; ... }`
- ~1494: `.terminal-key { flex: none; min-width: 38px; min-height: 28px; padding: 3px 7px; ... font-size: 11px; }`
- ~1340 (inside the phone media block `@media (max-width: 767px), (pointer: coarse) and (max-height: 500px)` — check exact wrapper before editing): phone overrides `.terminal-keys { gap: 4px; padding: 2px 4px; }` and `.terminal-key { min-height: 28px; padding: 1px 7px; font-size: 11px; }`

Numbers may have drifted a few lines after the C2 fix (empty-status rule
deleted near 1547); re-locate by selector, not line.

## 7. Test-first instructions

Failing test already exists; do not author new tests.

- RED (must fail before edit with `Paste`/`Hide keyboard` "clipped off-screen"):
  ```bash
  npx playwright test e2e/explore-terminal-visual.test.ts -g "Paste and Hide keyboard" --project=mobile-webkit
  ```
  Run from `crates/ajax-web/web`. Assertion: each of Paste / Hide keyboard has
  `fullyInRow === true` (button rect inside row rect and window, ±1px).

## 8. Edit instructions

In the phone-width media block (and/or a `max-width: 500px` block if finer
targeting is needed), stop relying on invisible horizontal overflow: make the
`.terminal-keys` row lay out all 10 keys visibly, e.g.

```css
.terminal-keys { display: grid; grid-template-columns: repeat(5, 1fr); overflow-x: visible; }
.terminal-key { min-width: 0; }
```

(two rows of five) — or an equivalent `flex-wrap: wrap` solution. Constraints:

- All 10 keys fully visible at 390px and 320px widths.
- Keys remain ≥28px tall (do not shrink tap height).
- Desktop/tablet (>767px, fine pointer) layout unchanged.
- Keep the row visually attached above the keyboard (no new margins/gaps
  beyond what a second row inherently costs).

## 9. Verification commands

Run from `crates/ajax-web/web`:

```bash
npx playwright test e2e/explore-terminal-visual.test.ts -g "Paste and Hide keyboard" --project=mobile-webkit
npx playwright test e2e/explore-terminal-visual.test.ts --project=mobile-webkit
npx vitest run src/components/TerminalRawView.test.ts
```

## 10. Acceptance criteria

- RED shown before edit, GREEN after, for the focused repro.
- Full `explore-terminal-visual` run: "empty status" probe stays green; the
  narrow-phone blank-band probe (known open defect M2) must not get worse —
  record its before/after status; other probes unchanged.
- Vitest component tests pass.
- Diff touches only the `<style>` block of `TerminalRawView.svelte`.

## 11. Stop conditions

- The fix appears to require markup, script, or non-allowed-file changes.
- The cited selectors are missing or materially different from the anchors.
- Any previously passing probe in `explore-terminal-visual.test.ts` regresses.
- Unrelated test failures block verification.
- Patch would exceed ~60 changed lines.
