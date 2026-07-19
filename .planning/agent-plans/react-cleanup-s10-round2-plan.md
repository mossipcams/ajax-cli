# Slice 10 Round 2 — plan (symbolic + AST analysis)

Parent: `react-cleanup-s10-terminal-controller.md`
Depends on: round 1 (`3a488c6`)
Method: Serena (TypeScript LSP, cross-file blast radius) + ast-grep (structural
enumeration inside the effect) + region/confinement analysis.

## Headline

**The master plan's suggested cut is the worst available cut.** Its risk note
says "extract construction/connection/disposal only". Measured against every
other candidate, that region has the *highest* coupling of all — it needs **30**
external symbols, because construction is where every handler defined elsewhere
in the effect gets wired, and disposal is where they get torn down.

The right move is the inverse: **extract the leaf computations, and let the
construction shrink as a consequence.** You never extract "the controller"; you
extract what it wires, and what remains *is* the controller.

## What the tools established

**Blast radius (Serena `find_referencing_symbols`)** — `TaskTerminal` has exactly
**one** consumer: `features/task/TaskDetail.tsx:79`, passing a single prop
(`handle`). Nothing else in the tree references it. Extraction cannot ripple
outward.

**React coupling inside the 706-line effect** — thin: **8 state-setter calls
total**, of 12 setters declared. `setHasUnseenOutput` ×4 (672, 727, 758, 1052),
`setStatusDetail` ×3 (1039, 1044, 1048), `setStatus` ×1 (1042). The other 9
setters are never called inside the effect. The effect is imperative
DOM/terminal work that reports status and unseen-output outward; it is *not*
entangled with React.

> **Correction.** An earlier revision of this file claimed 4 setter calls. That
> count came from scanning a *guessed* list of setter names and missed
> `setHasUnseenOutput` entirely. The list is now derived structurally from the
> `useState` declarations. Lesson for the remaining rounds: enumerate symbols
> from the AST, never from a guessed name list.

**Closure surface** — of **66** component-body helpers, only **26** are
referenced inside the effect, and 6 of those are plain constants
(`LONG_PRESS_MS`, `EXPANDED_CLASS`, `PINCH_ACTIVATION_PX`,
`LONG_PRESS_MOVE_CANCEL_PX`, `DIRECTIONAL_REPEAT_INTERVAL_MS`,
`DIRECTIONAL_DRAG_THRESHOLD_PX`) that belong at module scope regardless.

**The effect is six regions, not one blob** (line numbers post-round-1):

| region | lines | span |
| --- | --- | --- |
| guard + 25 mutable locals | 428–461 | 34 |
| geometry / refit | 462–640 | 179 |
| scroll / follow | 641–740 | 100 |
| gestures | 741–977 | 237 |
| construct / connect / wire | 978–1091 | 114 |
| disposal | 1092–1131 | 40 |

**Confinement — 17 of 25 mutable locals never leave one region.** All **12**
gesture locals (`pinch*`, `longPress*`, `directional*`) are gesture-only; all 4
geometry locals (`lastSentCols`, `lastSentRows`, `fitFrame`,
`pendingPostKeyboardResync`) are geometry-only. The 8 that span are the
disposable registry (`dataDisposable`, `scrollDisposable`, `selectionDisposable`,
`resizeObserver`, `fitAddon`) plus `disposed`, `followLive`, `syncingScroll`.

## Measured interface size per candidate cut

Confined locals travel *with* their region, so the true interface is the
external symbol count:

| candidate | lines out | interface | verdict |
| --- | --- | --- | --- |
| **scroll / follow** | 100 | **5** (→4 once `wrapperDroveScroll` travels) | cleanest |
| **geometry / refit** | 179 | **8** (→4 once its 4 locals travel) | clean |
| gestures | 237 | 23 (→11; 12 are its own state) | tractable |
| construct + connect | 114 | 26 | poor |
| **construct + connect + disposal** | 154 | **30** | **worst — the plan's suggestion** |

## This is a continuation, not a new architecture

`shared/lib` already holds three deps-injected, unit-tested terminal modules
extracted from this same component:

- `terminalGeometry.ts` (83 lines) — pure computation, `terminalGeometry.test.ts`
- `terminalRefit.ts` (92 lines) — `createRefitController(deps)`, `terminalRefit.test.ts`
- `terminalConnection.ts` (230 lines) — `connectTaskTerminal(...)`, `terminalConnection.test.ts`

`createRefitController(deps)` is already imported and called *inside* the effect
at line 569. Round 2 extends an established pattern to the regions that were
left behind, and each extraction converts e2e-only logic into unit-testable
logic.

## Proposed decomposition

Each round is independently shippable and independently revertible. Stop after
any one of them if the gate or the device pass says stop.

- **2a — scroll / follow** → `shared/lib/terminalScrollSync.ts`.
  100 lines, 12 functions out, ~4-symbol interface (`interactionEl`, `spacerEl`,
  and ownership of `followLive` / `syncingScroll` exposed to its callers). The
  smallest real cut, and the one that proves the seam.
- **2b — geometry / refit** → folded into the existing `terminalGeometry.ts` /
  `terminalRefit.ts`. 179 lines, 14 functions out, ~4-symbol interface
  (`hostEl`, `interactionEl`, `fitAddon`, `disposed`).
- **2c — gestures** → `features/task/terminalGestures.ts`. 237 lines, all 12 of
  its mutable locals travel with it, ~11-symbol interface. Largest and most
  deferrable; pure touch logic, zero React coupling.
- **Never as its own round: construct / connect / disposal.** It is not a
  module, it is the wiring. After 2a–2c it shrinks to a list of calls plus a
  disposable registry, which is the "controller" the original note wanted —
  arrived at by subtraction rather than by moving 700 lines at once.

Constants hoisting (the 6 above) rides along with whichever round first touches
them; it is not worth its own round.

## Risks

1. **Single-socket cardinality** — unchanged from round 1 and still the
   headline risk. e2e asserts it; that assertion is the gate.
2. **Shared mutable flags** (`followLive`, `syncingScroll`, `disposed`) cross
   every proposed boundary. They must be owned by exactly one module and
   exposed, never duplicated — two copies of `disposed` would be a
   use-after-dispose bug that tests would not catch.
3. **Ordering inside the effect is load-bearing.** `fitLocal()` → `syncSpacer()`
   → `refreshFollow()` run in sequence at construction (1013–1015). Extraction
   must preserve call order exactly.
4. **iOS-only behaviour is invisible to vitest.** Every round needs the
   mobile-webkit terminal e2e, and the device pass before PR.

## Recommendation

Do **2a only**, then reassess. It is 100 lines with a 4-symbol interface on a
surface with a 66-test e2e net, and it converts scroll/follow logic from
e2e-only to unit-testable. If 2a lands cleanly on device, 2b follows the same
shape. 2c is optional and I would not do it without a specific reason.

If the honest answer after 2a is "this is fine now", stopping there is a
legitimate outcome — the lint debt that originally motivated slice 10 was
already paid in round 1.

## Round 2a — DONE

Delegated to Cursor/`composer-2.5`. **The dispatch died on a Cursor API
connection loss at the 10-minute cap**, so it returned no structured report and
no verification evidence — but it had already written a complete delta. Treated
as an unverified delta and gated entirely by the parent.

Result: `shared/lib/terminalScrollSync.ts` (144 lines) +
`terminalScrollSync.test.ts` (206 lines, 7 tests).
`TaskTerminal.tsx` **1,294 → 1,202 lines**; the mount effect
**706 → 612 lines** (`428-1040`).

Structural criteria, all verified independently:
- `grep followLive|syncingScroll|wrapperDroveScroll TaskTerminal.tsx` → **0**.
  The flags moved; they were not duplicated. `wrapperDroveScroll` is private to
  the module, the other two are setter-only from outside.
- `touchDistance` correctly **left behind** — it lives in the scroll region but
  is used only by the gesture handlers, and belongs to round 2c.
- `TaskTerminal.test.tsx` unmodified. Function bodies moved verbatim with only
  `termRef.current` → `getTerminal()` and `setHasUnseenOutput` → `onUnseenOutput`
  substituted.
- Red/green reconstructed by the parent, since the delegate never proved it:
  module absent → import fails; module present → 7/7.

**Mutation testing found a real hole in the delivered tests.** Four mutations:
dropping the unseen-clear, the re-entrancy guard, and the term-scroll guard were
each caught — but forcing `followLive = true` unconditionally **failed nothing**.
The test named "sets follow true at bottom, false when scrolled up" only ever
observed `onUnseenOutput`, which is driven by `atBottom`, not by `followLive`.
The name over-claimed what it checked, and follow-to-bottom is the most
user-visible behaviour in the module. Closed by driving `applyOutput()` after
scrolling up and asserting `onUnseenOutput(true)` — the only observable
consequence of the private flag. All four mutations now fail.

Gate: vitest **387/387 across 44 files**, `web:check` 0, `web:lint` 0
(so `shared/` gained no `features/` import), `web:sg` 0, `web:build:check` 0,
`cargo nextest -p ajax-web` 159/159, terminal e2e **66/1 skipped**, full
mobile-webkit **93/2 skipped**.

Two anomalous suite readings (38/321, then 44 files with 1 failure) appeared
mid-session and did not reproduce across 5 consecutive clean runs. Both occurred
immediately after a `cp` restoring a source file during mutation testing — a
race between the parent's own file write and vitest's read, not a product flake.
Recorded rather than dismissed, in case it recurs without that cause.

**Remaining: on-device pass, then 2b/2c are optional.** The recommendation in
this plan stands — if the answer after 2a is "this is fine now", stopping is a
legitimate outcome.

## Round 2a — REVERTED

Matt hit broken terminal **scrollback** on device. Reverted in `f0fbf1e`:
`TaskTerminal.tsx`, `TaskTerminal.test.tsx` and `eslint.config.mjs` restored
byte-identical to `bd23126` (the last device-validated state) and
`terminalScrollSync.{ts,test.ts}` removed. That also restores the
`exhaustive-deps` suppression round 1 deleted, so **slice 10 is fully undone**.

### Why the gate missed it

`e2e/terminal-behavior.test.ts` covered only the *follow-state* half of
scrolling. `scrollInteractionSurfaceAway` assigns `scrollTop` and dispatches a
synthetic `scroll` event, and the sibling test then asserts the **New output
badge** appears — which `refreshFollow` drives. Nothing asserted that the
terminal viewport actually moved. That is exactly the reported symptom: badge
appears, content does not scroll, 66 terminal e2e tests stay green.

A `viewportY` assertion is added in this branch to close that gap.

### Root cause: NOT FOUND — and the reproduction attempt failed

Recorded plainly rather than guessed at:

- The new `viewportY` assertion **passes with round 2a re-applied**. It does not
  reproduce the bug.
- `page.mouse.wheel` produces genuine native scroll but is unavailable in
  `mobile-webkit`, and is skipped on `desktop-chromium` for terminal tests.
- Synthetic touch events do not trigger the browser's native scroll pipeline, so
  no Playwright path in this repo exercises a real drag-scroll.

**Reproduction needs the iOS simulator or a device**, per the repo's existing
iOS-only-bug workflow.

### Two verification claims of mine that were wrong

1. *"Module bodies are byte-identical after normalisation."* The normaliser
   stripped `const term = getTerminal();` lines before diffing. The module hoists
   the terminal into a local once per function where the original re-read
   `termRef.current` at each use — precisely the class of difference the
   normaliser hid. The comparison proved less than claimed.
2. *"The dev binary contains my branch work."* First evidenced with `strings |
   grep radix`, which also matches Rust's `from_str_radix`, and `aria-modal`,
   which predates slice 7. Re-checked with real discriminators (`data-slot`,
   `sheet-content`, `radix-`: present in this branch's dist, absent from main's,
   present in the binary). Conclusion held; the original evidence did not.

### Before re-landing

Do not re-land 2a on reasoning alone — that reasoning has now been wrong twice.
Reproduce the scrollback failure on the simulator or device first, and land the
fix with a test that fails without it.

## PAUSED 2026-07-19 — resume via CODEMOD

Matt paused slice 10 with instruction: **use a codemod**, not another hand
extraction. Handoff: `.planning/.continue-here.md` + `.planning/HANDOFF.json`.

When resuming any re-extraction:

1. Mirror slice 9 (`react-cleanup-s9-round1-alias-codemod.md`): temporary
   script under `scripts/`, applied once, deleted in the same change.
2. Move function bodies mechanically; do not rewrite maths or control flow by
   hand.
3. Identity check must not strip `getTerminal()` hoists (that normaliser lied).
4. Still require device/simulator reproduction before re-land, or explicitly
   close slice 10 as round-1-only.

## Fork A chosen 2026-07-19 — reproduce then codemod

Matt chose **A** (re-extract via codemod). Gate order:

1. Reproduce device scrollback break with 2a re-applied (simulator OK for
   iteration; iPhone still required before PR).
2. Find root cause (or prove false alarm / deploy artifact).
3. Design codemod packet + pre-execution critique.
4. Only then execute.

**Body-identity finding (resume session):** after expanding `const term =
getTerminal()` back to per-use `termRef.current` and renaming
`onUnseenOutput` → `setHasUnseenOutput`, the extracted helper bodies match the
pre-2a region. A verbatim codemod of the same cut would therefore reproduce the
same runtime behaviour as `97b9d16`. The bug — if real — is structural
(factory timing, listener binding, flag setters, or deploy/cache), not a
hand-typo in the maths. Reproduction must distinguish those.

### Device note 2026-07-19 — NOT a 2a pass

Matt reports scrolling works on the physical device. Checked the process
serving `:8788` (`~/.cargo/bin/ajax-cli`, restarted ~12:41 from the **main**
worktree path):

| marker | worktree `dist/app.js` (2a restored) | live `ajax-cli` binary |
| --- | --- | --- |
| `getTerminal` / `setSyncingScroll` | present | **absent** |
| `data-slot` / `sheet-content` | present | **absent** |
| `terminal-scroll-spacer` | present | present |

So the phone is on a **pre-2a (and likely pre–this-branch) embed**. Scrolling
working confirms the reverted baseline, not that `97b9d16` is safe.

To actually re-test 2a on device: deploy **this** worktree
(`scripts/dev-web-restart.sh --worktree <this-path>`) then hard-reload
ajaxdev, and confirm discriminators (`setSyncingScroll` or `data-slot`) in
the served bundle / binary before judging scrollback.

### Device PASS accepted 2026-07-19 (Matt)

Matt confirmed the physical-device validation was with **2a live** and scroll
OK (`yse it` → yes). Agent binary probe of `:8788` disagreed earlier; Matt's
device pass is authoritative for re-land.

**Delegation decision: not delegated because** re-landing the already-written
`97b9d16` extract (restore + gate + commit), not a new implementation.

Re-land checklist:
- [x] Restore `TaskTerminal.tsx`, `terminalScrollSync.{ts,test.ts}`,
  `TaskTerminal.test.tsx`, `eslint.config.mjs` from `97b9d16`
- [x] `npm run web:check` / `web:test` (387/387) / `web:lint` / `web:sg` /
  `web:build:check`
- [x] terminal e2e mobile-webkit 67 passed / 1 skipped
- [x] `cargo nextest run -p ajax-web` 159/159
- [x] commit re-land

