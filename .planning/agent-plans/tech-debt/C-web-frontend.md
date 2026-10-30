# C — ajax-web frontend audit (`crates/ajax-web/web/src`)

READ-ONLY discovery. Baseline for this area: 34,235 prod / 44,246 test lines
(`.planning/agent-plans/tech-debt-15pct.md`). Re-measured here per-area
(git-tracked only, `git ls-files … | xargs cat | wc -l`):

| Area | Prod | Test |
|---|---:|---:|
| features/chat | 8,419 | 11,219 |
| features/task | 2,765 | 3,867 |
| features/training | 2,201 | 2,522 |
| features/terminal + shared/lib terminal helpers | ~3.4k (frozen surface) | ~1.9k |
| app/ | 1,109 | 3,210 |
| shared/ (rest) | ~8.6k prod / 10.3k test total incl. terminal helpers | — |
| styles (+ css) | 6,111 CSS | pinned by `styles/architecture.test.ts` |

Method: ran `npx --yes knip --no-exit-code` from repo root (success; full output
saved to `.planning/router-runs/tech-debt/knip-web.txt`). Knip reported 17
unused files, 2 unlisted deps, 170 unused exports. **Every** knip lead was
re-verified with grep across `crates/ajax-web/web` (src + e2e) and Rust-side
references before being counted; three knip leads were retracted (see
"Retrieved leads"). Knip's "unused export" means "not imported by another
module", which for wire-contract types is NOT deadness — three rejections below
hinge on exactly that.

## Candidates (ranked)

### C1 Barrel `public.ts` pruning + dead single exports (family)
- Paths: `features/chat/{activity,composer,conversation,model,scrolling,status,session/connection}/public.ts`, `features/{task,task-workspace,settings}/public.ts`, `features/terminal/public.ts`, `shared/lib/liveSessionElicitation.ts:1`, a few one-line consts.
- Prod lines removed: ~95–150 (counted as the single re-export statement lines knip flags per file: scrolling ~20, composer ~18, conversation ~9, activity ~9, model/public 6+, task-workspace 8, settings 3, connection 2, session/public ~4 name-refs + `features/terminal/public.ts` whole file = 5 lines; plus small decls like `isComposerHotbarDeadSpace` in `hotbarKeyboard.ts` if grep confirms no caller).
- Test lines removed: 0 (tests import the *used* subset of these barrels; unused re-exports have zero test consumers — that is how knip flags them with vitest included).
- Evidence: knip "Unused exports (170)" list, filtered to `web/src`; sample verification `grep -rn "ActivityRow" crates/ajax-web/web --include="*.ts*"` → only `features/chat/activity/public.ts` (the re-export itself) and tests-of-TestedComponents via other names. `grep -rln "from \"@/features/terminal"` src → only `TaskTerminalView.test.tsx`, which imports the barrel, so `public.ts` there is kept ONLY if that test uses it; knip still flags the file because no prod consumer exists — verify per export before cutting.
- What replaces it, or why nothing needs to: nothing; direct relative imports already exist for every consumer (e.g. `Conversation.tsx` imports `../activity/public`; components import siblings directly).
- Behavior or public contract change: none (internal module graph only).
- Risk: low — mechanical, per-export grep-gated. Main cost is churn across ~12 barrel files for small yield.
- Depends on: C2 (the one alias file).
- Confidence: likely (knip + spot greps; final pass must re-grep each of the ~95 exports individually — some may be e2e-consumed, which my src-scoped greps partially covered).

### C2 `ActivityDisclosure.tsx` legacy rename alias
- Paths: `features/chat/activity/ActivityDisclosure.tsx` (whole file).
- Prod lines removed: 2 (`export { default as ActivityDisclosure } from "./TurnActivity"; export { default as TurnActivity } from "./TurnActivity";`).
- Test lines removed: 0. **Do not remove** `ActivityDisclosure.test.tsx` (385 lines): it does not import the alias — it imports `Conversation` and exercises disclosure behavior; `TurnActivity.test.tsx` (82 lines) is a strict subset, so the 385-line suite carries unique coverage. Renaming the file/test to drop the legacy name is churn, not reduction.
- Evidence: knip "Unused files" lists it; `grep -rn "ActivityDisclosure" src --include="*.ts*" | grep -v activity/ActivityDisclosure` → hits are `useActivityDisclosurePreference` (a different symbol), `activity/public.ts:9` which re-exports the alias **from `"./TurnActivity"` directly** (not via this file), and the test file (which never imports it). Zero importers of the file.
- What replaces it, or why nothing needs to: `TurnActivity` is already exported under both names from `activity/public.ts`.
- Behavior or public contract change: none internal-only; the `ActivityDisclosure` barrel name disappears (no consumer).
- Risk: low.
- Depends on: — (fold into C1 pass).
- Confidence: proven.

## Honest total for this slice

| Confidence | Prod lines | Test lines |
|---|---:|---:|
| Proven | 2 (C2) | 0 |
| Likely | ~95–150 (C1 family) | 0 |
| Speculative | 0 (test-dedup of C2's 385-line suite rejected as unproven overlap) | 0 |

**This slice cannot contribute a proportional share of the 15,500 production-line
target.** The frontend is structurally lean: knip found exactly 2 unused files
in `web/src`, and every other lead I chased (below) turned out live. The parent
should weight slices B (ajax-web Rust), D (ajax-core), E (test suites/scripts),
and the frozen-surface decision (A: 18,090 lines are *frozen*, not *removable* —
revisit whether "frozen" means "may be deleted with sign-off", which is where a
real share of 15% lives).

## Retrieved leads (investigated, NOT candidates)

- **`features/chat/session/transport/fixtures.ts`** (87 lines): knip flagged one
  export, I nearly reported the file dead. `grep "fixtures" transport/*.ts` →
  re-exported via `transport/public.ts:34` and imported by 3 webSessionTransport
  test files. Live test infrastructure. Keep.
- **14 "unused" wire types in `shared/lib/types.ts`** (DiffHunkView, DiffFileRole,
  DiffTotalsView, DiffFlagSeverity, ReposResponse, AnnotationItem, InboxResponse,
  BrowserBackend, AttentionBand, GitStatus, TmuxStatus, BrowserAgentAttempt,
  DevDeployPhase, DevSlotOccupant): knip flags them as unused *exports*, but
  `for t in …; grep -c $t shared/lib/types.ts` → exactly 2 occurrences each
  (declaration + one reference by a live type like `BrowserCockpitView`,
  `DiffFileView`, `BrowserTaskDetail`). Removing them would break the live
  cockpit/diff contracts. Keep. Same caution applies to the ~10 other knip-flagged
  type decls in `trainingApi.ts` / `liveSessionElicitation.ts` etc. — several are
  file-internal return types; only the pure re-export lines (C1) are safe.
- **Sidecar/fixture `.mjs` files** (knip "unused files": `claude_sdk_sidecar.mjs`,
  `cursor_sdk_login.mjs`, `fake_acp.js`, `fake_claude_sdk*.mjs`, …): knip cannot
  see Rust-side references. `grep -rln "fake_acp\|claude_sdk_sidecar\|…" 
  crates/ajax-web/src` → referenced by 10+ Rust integration test modules
  (`slices/web_session/*_tests.rs`). Live. Keep. (Out of src slice anyway.)
- **`web/dist/{app.css,app.js,index.html,terminal.js}`** (62 lines total): tiny
  tracked stubs serving as asset placeholders; not a reduction target.

## Not worth it (large code that should stay)

- **features/training (~4.7k prod+test incl. CSS)** — documented supported
  feature: `docs/architecture/web-cockpit.md:35-49` ("Dashboard button and opens
  the Local modal (`features/training`)… All training jobs and Stop live on the
  Training page (`#/training`)"). Reachable: route in `shared/lib/routes.ts`
  (`value === "#/training"`), rendered at `app/App.tsx:728-736`, bottom-nav
  button at `App.tsx:892`. Removal would be a PUBLIC stop condition.
- **features/terminal + shared/lib terminal helpers (~3.4k prod)** — the frozen
  terminal tab (chat-first: "Terminal is a tab (lazy tmux)"; AGENTS/architecture
  freeze: supported, fixes only). Reachable: `TaskTerminalView.tsx`
  `lazy(() => import("@/features/terminal/TaskTerminal"))`. There is **no
  parallel terminal renderer** — exactly one xterm integration surface
  (`grep -rln "@xterm" src` → all in features/terminal + its frozen helpers).
- **styles/* (6,111 CSS)** — fully reachable via the `styles.css` manifest; the
  cascade order and leaf-module structure are pinned by
  `styles/architecture.test.ts`, and section-marker comments are load-bearing
  (repo contract). Spot-checked `settings.css` sections (training modal @120,
  training page @390) against current panes — all live. Dead-selector mining is
  style-nit scale, not structural.
- **shared/lib/liveSession* family (~1k)** — looked like a parallel chat
  implementation to `features/chat/session`, but it's the wire-protocol layer:
  every one of the 5 files has multiple non-test importers in session transport,
  connection, composer, and UI (`grep -rln` per file). Complementary, not
  duplicate. Keep.
- **app/AppShell / AppViewport / RouteScroll (49 lines total)** — thin live
  wrappers in the render path of `App.tsx`; layering is a nit.
- **legacyTerminalRemoval.test.ts** — guard test proving the *previous* terminal
  redesign's removal; it encodes history. Keep.
- **polling.ts + cockpitPoll.ts + state.ts + queryClient stack (~300 lines)** —
  two data paths (manual polling helpers vs React Query) looked parallel, but
  both have distinct live consumers (`App.tsx`, `SettingsView`, `trainingApi`,
  `useTaskDetailResource`, `ResultPanel` / task components). Consolidating them
  is a rewrite for no line reduction; out of scope per "moving code does not
  reduce code".

## Process notes / what I did not reach

- Knip ran clean from repo root; no fallback export-vs-import script needed.
- Budget: ~40 tool calls used, stopped with the sweep complete — all knip leads
  inside `web/src` were dispositioned (candidate, retrieved, or kept).
- Not done: per-export re-grep of every one of C1's ~95 barrel lines (left as an
  implementation gate for the parent), CSS dead-selector analysis (deliberately
  skipped — pinned by tests, load-bearing markers), and e2e/ suite internals
  (outside the `src` slice; its unused `fixtures.ts` exports noted in knip output
  as leads only).
