# Slice 10 — Terminal controller extraction

Master plan: `react-migration-cleanup.md`
Depends on: slice 9 (`b8da246`), on-device PASS 2026-07-19

## Measurement before planning

- `features/task/TaskTerminal.tsx` is **1,279 lines**. It contains exactly **one**
  `useEffect`, spanning **lines 412–1117 — 705 lines** — keyed on `[handle]`.
  That single effect owns terminal construction, socket connection, resize/refit,
  gestures, clipboard, fullscreen and disposal.
- The `react-hooks/exhaustive-deps` suppression at `eslint.config.mjs` names
  three culprits: `consumeCtrl`, `hardenMobileTextarea`, `scheduleBandSettle`.
- Those three account for **4 call sites inside the effect**, not hundreds:
  - `hardenMobileTextarea` — line 989 only
  - `consumeCtrl` — line 1006 (`liveTerm.onData`)
  - `scheduleBandSettle` — lines 735, 1070
- **Two of the three are also called from outside the effect**, which decides the
  implementation:
  - `consumeCtrl` at line 1233, inside a toolbar `onClick`
  - `scheduleBandSettle` at lines 406 and 409, inside `toggleExpanded`
  `useEffectEvent` functions may only be called from effects, so those two cannot
  simply be converted — they keep their plain form for the JSX callers and gain
  thin effect-only wrappers.
- `useEffectEvent` is available (React **19.2.7**) and **already established in
  this codebase for this exact situation**: `app/App.tsx:95-105` wraps
  `onShellMount`, `onShellResume`, `onShellVisibilityChange`, and slice 2's plan
  calls that "the sanctioned use — an external subscription that must mount once
  but needs the latest non-reactive callbacks — **not concealment of a
  dependency**" (`react-cleanup-s2-eslint.md:499-501`).

## Two slices hiding under one name

The suppression comment says the fix is "moving that imperative ownership behind
a disposable controller … Delete this block in the same change." That conflates
two things of very different size and risk:

**A. Satisfy the rule honestly (≈15 lines).** Route the four in-effect call sites
through `useEffectEvent`. The deps stay `[handle]` legitimately and the last
react-hooks suppression in the tree is deleted. No behaviour change, no
restructuring, and it uses the pattern slice 2 already blessed.

**B. Break up the 705-line effect (≈700 lines moved).** The actual structural
debt, on the most fragile surface in the app, on a branch that has just been
hand-validated on device.

**A does not substitute for B**, and doing A alone has a trap: the suppression is
currently the *only* marker in the tree that the 705-line effect is debt. Deleting
it without paying the debt would erase the marker. So round 1 replaces it with an
explicit `ponytail:` note naming the ceiling and the upgrade path.

The master plan already pre-authorises this split (risk 3): "If clipboard or
fullscreen ownership cannot move cleanly, extract construction/connection/disposal
only and continue in separate tested slices."

## Rounds

- [x] **Round 1 — `useEffectEvent`, delete the suppression.** Delegated to
  pi/`opencode-go/minimax-m3`, ACCEPTED after the parent relaxed two over-coupled
  test assertions.
- [ ] **Round 2 — controller extraction.** Planned, not started. See
  `react-cleanup-s10-round2-plan.md` for the Serena + ast-grep analysis.
  **Key finding: this plan's own suggested cut (construction/connection/disposal)
  is the worst available cut — 30-symbol interface.** The leaf computations cut
  at 4–5. Proposed order: 2a scroll/follow, 2b geometry/refit, 2c gestures;
  construction shrinks by subtraction rather than being extracted.

## Round 1 design

- `hardenMobileTextarea` → `useEffectEvent` directly (no outside caller).
- `consumeCtrl` and `scheduleBandSettle` keep their current plain definitions,
  because JSX handlers call them. Add effect-only wrappers alongside:
  - `onTermData = useEffectEvent((data: string) => sendKey(consumeCtrl(data)))`
  - `onBandSettle = useEffectEvent(() => scheduleBandSettle())`
- Inside the effect, call the wrappers at lines 735, 989, 1006, 1070. Nothing
  else in the effect changes.
- Delete the suppression block from `eslint.config.mjs` and replace it with a
  `ponytail:` comment in `TaskTerminal.tsx` recording that the 705-line effect
  is still one unit and round 2 owns splitting it.

## Non-goals (round 1)

- No controller, no new module, no moved ownership.
- No change to terminal behaviour, socket cardinality, gestures, clipboard,
  fullscreen, or geometry.
- No test weakened. `TaskTerminal.test.tsx` and the 95KB
  `e2e/terminal-behavior.test.ts` are the net and are not edited.
- No `StrictMode`.

## Risks

1. **Single-socket cardinality.** The whole reason the deps were suppressed. If
   the effect re-runs, the terminal tears down and reconnects. e2e asserts one
   socket; that assertion is the guard.
2. **`useEffectEvent` misuse.** Calling an Effect Event outside an effect is a
   React error. `consumeCtrl` and `scheduleBandSettle` must keep their plain
   forms for the JSX call sites — converting them outright would break the
   toolbar and fullscreen toggle.
3. **Stale closure inversion.** The wrappers must delegate to the live functions,
   not capture them, or the effect would see first-render state.

## Delegation decision

`Delegation decision: delegated via model-router`

## Baselines

- Suite: 380 tests / 43 files
- mobile-webkit e2e: 93 passed / 2 skipped
- `web:lint` currently clean *only because* of the suppression block

## Validation gate

```bash
npm run web:check
npm run web:test -- --run
npm run web:lint
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
cargo nextest run -p ajax-web
```

Plus the terminal-specific e2e, which is the real net for this surface:

```bash
npm run web:smoke -- --project=mobile-webkit -g "terminal"
```

## Deviations / Validation results

- Round 1: PASS — vitest **380/380** across 43 files, `web:lint` 0 **with the
  suppression block deleted**, `web:check` 0, `web:build:check` 0,
  `cargo nextest -p ajax-web` 159/159, terminal e2e **66 passed / 1 skipped**
  (single-socket cardinality intact), full mobile-webkit **93 / 2 skipped**.
- `grep 'exhaustive-deps": "off"' eslint.config.mjs` → **0 matches**. The effect's
  dep array is still exactly `[handle]` (`TaskTerminal.tsx:1132`). Production diff
  is 20 insertions / 5 deletions.
- **Best delegate round of the program.** The delegate hit a genuine contradiction
  in the packet and *stopped and reported it* instead of claiming success or
  quietly editing tests: `TaskTerminal.test.tsx` asserts the literal source text
  `scheduleBandSettle()` inside two callback bodies, so routing those call sites
  through `onBandSettle()` necessarily breaks them — while the packet
  simultaneously demanded "0 failing" and "no test file modified". Its report
  named both criteria and explained why they were mutually exclusive.
- **Packet defect (mine, the fourth of this program).** The contradiction was
  real and avoidable: I forbade touching the test file without first checking
  whether the tests were coupled to the call-site *names* I was asking to change.
  The recurring shape is over-tight acceptance criteria written without reading
  what the tests actually assert.
- Parent fix: two assertions at `TaskTerminal.test.tsx:181,204` now accept
  `(?:schedule|on)BandSettle`. Not a weakening — they still require a band-settle
  call at those exact points. Both mutation-checked by neutering each call site
  in turn, each failing exactly its own test. The `toggleExpanded` assertions
  (`:194-196`) were untouched and still pass, because that call site legitimately
  did not change.
- The debt marker was preserved, not lost: the deleted suppression block was the
  only record that the effect is 705 lines, so `TaskTerminal.tsx:409-414` now
  carries a `ponytail:` comment naming the ceiling and pointing at round 2.
