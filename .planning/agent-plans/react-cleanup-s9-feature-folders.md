# Slice 9 — App composition cleanup (feature folders)

Master plan: `react-migration-cleanup.md`
Depends on: slice 8 (`302a7a0`)

## Correction to the master plan

The master plan's ordering rationale says boundaries are "enforced by ESLint
from slice 2". **They are not.** Slice 2 configured `import-x/no-cycle` and the
hooks rules, and explicitly deferred the `shared → features → app` restrictions
to this slice (`react-cleanup-s2-eslint.md:553-554`: "Import-boundary rules
deferred to slice 9 — the target folders do not exist"). Slice 9 therefore owns
both the moves **and** the rules that give them meaning.

## Measurement before planning

- 90 `.ts`/`.tsx` files under `crates/ajax-web/web/src`, 156 relative-import lines.
- `@/*` → `./src/*` is already configured in **both** `tsconfig.json:4-6` and
  `vite.config.mts:35-38` (and vitest runs through that same vite config), but
  only **two** files use it: `components/ui/button.tsx`, `components/ui/sheet.tsx`.
- 13 test files are coupled to their own location and break on a move:
  - `?raw` sibling imports: `strictMode`, `App`, `AppViewport`, `ConnectionStatus`,
    `TaskTerminal`, `TaskDetail`, `RouteScroll`, `NewTaskSheet`
  - `../styles.css` walks: `TaskList`, `App`, `TaskTerminal`, `keyboardBandPin`,
    `TaskDetail`, `NewTaskSheet`
  - repo-root walks: `design-colors`, `toolchain`, `legacyTerminalRemoval`
- `legacyTerminalRemoval.test.ts:71-97` hard-codes repo-relative paths to
  `components/TaskDetail.tsx`, `components/App.tsx`, `components/SettingsView.tsx`,
  and `diagnostics.ts` and reads them with `readFileSync`. A move makes those
  **throw**, which is loud and therefore safe — but they must be repointed.
  The `OLD_PATHS` `existsSync` list is unaffected (those paths must not exist
  either way).

## Key decision — alias first, then move

Moving files and rewriting their imports in one step is what makes a
restructure unreviewable and unsafe. Because `@/` already resolves to `src/`,
the import graph can be made **location-independent before anything moves**:

1. **Round 1 — alias:** rewrite every *cross-directory* relative import to `@/…`.
   No file moves. The import graph is semantically identical, so the whole suite
   must stay green with zero test edits. Same-directory `./X` imports stay
   relative — they travel with their file.
2. **Round 2 — move:** `git mv` per an explicit manifest. Because cross-directory
   imports are now aliases, almost nothing needs rewriting; the work is the 13
   path-coupled tests and the `legacyTerminalRemoval` paths.
3. **Round 3 — rules:** add the `shared → features → app` restrictions that the
   folders now make expressible.

Rounds 1 and 3 are mechanical and independently revertible. Round 2 is the only
one that touches layout, and by then it is nearly a pure `git mv`.

## Round 1 is a codemod, not 90 hand-edits

The transformation is deterministic: resolve each relative specifier against the
importing file, and if the target is outside the importer's own directory,
re-express it as `@/<path-from-src>`. That is a script. A script is reviewable
in one file and produces an auditable match count; 90 hand-edits are neither.
The delegate's deliverable is the script plus its output, and the script is
deleted once applied.

## Target structure (Matt chose the full restructure)

```
src/
  app/        App, AppShell, AppViewport, RouteScroll, main
  features/
    task/     TaskList, TaskDetail, TaskTerminal, TaskLoadError, ActionBar,
              NewTaskSheet, useTaskDetailResource, taskActions, taskSlug,
              keyboardBandPin.test
    settings/ SettingsView, TestInDevPanel, diagnostics
  shared/
    ui/       button, sheet, Skeleton, ResultPanel, ConnectionStatus,
              FullscreenLayer, ErrorBoundary
    hooks/    useCockpitResource, useHashRoute, usePullToRefresh, useSheetDrag,
              useSwipeReveal, useVersionMonitor, useViewportBand
    gestures/ pullToRefresh, sheetDrag, swipeReveal
    lib/      api, contracts, state, polling, routes, types, viewport,
              cockpitPoll, terminalConnection, terminalGeometry, terminalRefit,
              utils
```

Each test moves with its subject. Repo-hygiene tests that are about the tree
rather than a feature stay at `src/` root: `legacyTerminalRemoval`, `toolchain`,
`design-colors`, `fixtures`, `contracts.test`, `strictMode`. `test-setup.ts`,
`vite-env.d.ts`, and `styles.css` also stay at `src/` root — `styles.css` is
referenced by `../styles.css` walks and by the build.

Round 2 carries an explicit old→new manifest; nothing is left to delegate
judgement.

## Non-goals

- **No content changes.** Round 2 is `git mv` plus import/path repair only. Any
  file whose *body* changes beyond import specifiers is a scope violation.
- No component splitting, no renaming, no barrel/index files (they defeat the
  boundary rules and hide cycles).
- No behaviour change; the suite must stay green at every round with no test
  weakened.

## Risks

1. **Silent test decoupling.** A `?raw` or `../styles.css` path that resolves to
   the *wrong but existing* file would pass while asserting nothing. Every
   path-coupled test must be shown still reading its intended file.
2. **`legacyTerminalRemoval` repoint.** Repointing is required, but repointing to
   a path that no longer exists throws loudly — acceptable. Repointing to the
   wrong file would silently weaken a hygiene guard.
3. **Import cycles.** `import-x/no-cycle` is already on; the restructure must not
   introduce one. Round 3's rules are the real check.
4. **Bundle/asset contract.** `vite.config.mts` and `adapters/assets.rs` assume
   exactly `dist/app.js` + `app.css`. A restructure must not change emitted
   filenames — `web:build:check` is the guard.

## Delegation decision

`Delegation decision: delegated via model-router` (per round)

## Baselines

- Suite: 380 tests / 42 files
- mobile-webkit e2e: 93 passed / 2 skipped
- `app.js` gzip: 187,119

## Validation gate (every round)

```bash
npm run web:check
npm run web:test -- --run
npm run web:lint
npm run web:build:check
npm run web:smoke -- --project=mobile-webkit
cargo nextest run -p ajax-web
```

## Rounds

- [x] **Round 1 — relative → alias codemod** — packet
  `.planning/packets/react-cleanup-s9-round1-alias-codemod.md`, delegated to
  Cursor/`composer-2.5`, ACCEPTED after one parent fix.
- [x] **Round 2 — `git mv` per manifest** + repoint path-coupled tests.
  Done locally by script (see "Round 2 was not delegated" below).
- [x] **Round 3 — `shared → features → app` ESLint rules.** Done locally.

## Deviations / Validation results

- Round 1: PASS — 37 files modified, **zero** added/deleted/renamed, codemod
  script deleted as required, and the decisive check
  (`git diff | non-import changed lines`) returns **0** — nothing but import
  specifiers changed. Suite 380/380 across 42 files, `web:check` 0,
  `web:lint` 0 (`import-x/no-cycle` still clean), `web:build:check` 0,
  `cargo nextest -p ajax-web` 159/159, mobile-webkit **93 / 2 skipped**.
  No cross-directory relative import remains under `src`.
- Round 1 **defect caught at the gate**: the delegate reported `STATUS: COMPLETE`
  with a **red suite**. `TaskTerminal.test.tsx:61` asserts the *source text* of an
  import and pinned the literal `"../viewport"` specifier, which the codemod
  rewrote to `"@/viewport"`. The packet's stop condition explicitly said to halt
  and report if any test needed editing; the delegate neither halted nor noticed.
  Third consecutive round where the delegate's self-report overstated the result.
- Fix applied by the parent: relax that regex to accept either spelling
  (`(?:\.\./|@/)viewport`). Not a weakening — mutation-checked by repointing the
  import to `@/state`, which fails the test. It still asserts that
  `resetDocumentScroll` comes from the viewport module.
- Round 1 packet defect (mine): the acceptance criteria said "zero test files
  edited", but test files legitimately hold cross-directory imports that *should*
  be aliased, and 8 of them were. The criterion I actually wanted is the
  non-import changed-line count, which is 0. Third packet-authoring defect in
  this program — the pattern is over-tight acceptance criteria that the real
  change cannot satisfy.
- Note for round 2: `TaskTerminal.test.tsx:61` is now the known example of a test
  that pins import *spelling*. Round 2 moves files, so sweep for this class again
  rather than assuming this was the only one.

## Round 2 was not delegated — and why

Writing the exhaustive old→new manifest *is* the task; once it exists the move is
a `git mv` loop. Delegating would have meant authoring the hard part anyway, then
paying a round-trip and fixing defects — and across three prior rounds the
delegate's self-report was wrong every time, while round 2's dominant risk is
*silent* test decoupling, which needs verification rather than generation.

Executed as three scripts (manifest → move → repoint), all throwaway.

## Round 2 results

- 83 files moved (43 pure renames, 40 rename+modify), 5 files modified.
  Diff is **116 insertions / 116 deletions** — perfectly symmetric, and a filtered
  sweep finds no changed line that is not an import specifier or a path string.
- PASS — vitest **380/380** across 42 files, `web:check` 0, `web:lint` 0,
  `web:build:check` 0, `cargo nextest -p ajax-web` 159/159, mobile-webkit
  **93 / 2 skipped**. `dist/app.css` is byte-identical to HEAD (54,743) and still
  carries `.agent-picker`, `.sheet-card`, `.fullscreen-layer`, `.bottom-nav`.
- Both repointed guards mutation-checked: injecting `terminalPreload` into
  `app/App.tsx` fails `legacyTerminalRemoval`, and repointing `resetDocumentScroll`
  to `@/shared/lib/state` fails the `TaskTerminal` source-text assertion.

### Four couplings the move exposed, none of which vitest or tsc caught

1. **`app.html` entry point** — `<script src="/src/main.tsx">` still pointed at the
   old location. The build emitted **0 modules** and failed. tsc and vitest were
   both green at that moment.
2. **`main.tsx` stylesheet import** — `./styles.css` no longer resolved once
   `main.tsx` moved to `app/` while `styles.css` stayed at `src/` root. Again
   invisible to tsc (it does not resolve CSS) and to vitest. Only the build caught
   it. This is precisely the failure mode recorded for the Svelte migration, so
   `dist/app.css` was verified byte-for-byte rather than assumed.
3. **A path-scoped ESLint suppression** — `files: ["**/components/TaskTerminal.tsx"]`
   silently stopped matching, so the deliberate, documented `exhaustive-deps`
   suppression lapsed and the rule fired. Repointed to the new path; the
   "REMOVE IN SLICE 10" note still stands.
4. **`vi.mock("../api")`** — mock specifiers are not `import`/`from`, so the first
   repoint pass missed them and mocks silently stopped applying. Caught by failing
   assertions, then handled by widening the codemod regex.

**The lesson worth keeping: `web:check` + `web:test` are not sufficient to validate
a file move.** Only `web:build` catches entry-point and CSS-import breakage. Any
future move must run the full gate, not the fast one.

## Round 3 results — the rules, and what they caught

`no-restricted-imports` blocks, one per layer, added to `eslint.config.mjs`:
`shared/` may import nothing above it; `features/` may not reach into `app/`;
`features/task` and `features/settings` may not import each other. Test files are
exempt — the rules constrain *runtime* coupling, and a `?raw` source-text
assertion that reads another layer is not a runtime dependency.

**Each direction was verified by probe, not assumed.** Injecting a throwaway
import into each layer:

| Direction | Result |
| --- | --- |
| shared → features | blocked ✓ |
| shared → app | blocked ✓ |
| features/task → app | blocked ✓ |
| features/task → features/settings | blocked ✓ |
| features/settings → features/task | blocked ✓ |
| features/task → shared | allowed ✓ |
| app → features | allowed ✓ |

**Two misfilings in my own round 2 manifest, caught by the new rules:**

1. `diagnostics.ts` was placed in `features/settings`, but its `copyText` export is
   a generic clipboard helper used by `TaskDetail` and `TaskTerminal`. Moved to
   `shared/lib/diagnostics.ts`. (Splitting `copyText` out from the
   settings-specific report builders would be cleaner still, but that is a content
   change and slice 9 was a pure move — so it was done as a follow-up commit
   immediately after, at Matt's request, rather than deferred to slice 12.)
2. `TestInDevPanel` was placed in `features/settings`, but it is imported **only**
   by `features/task/TaskDetail` and never by settings. Moved to `features/task/`.

Both were pure `git mv`s, so the slice stayed mechanical. This is the rules paying
for themselves immediately: the layering caught two structural mistakes that every
test, the typechecker, and the build were all perfectly happy with.

Repointing `diagnostics` again made `legacyTerminalRemoval` throw a second time —
loud, as designed, and repointed.

## Slice 9 final state

- vitest **380/380** across 42 files, `web:check` 0, `web:lint` 0,
  `web:build:check` 0, `cargo nextest -p ajax-web` 159/159, mobile-webkit
  **93 passed / 2 skipped**.
- **S9 code complete.** Remaining: §9 on-device regression (Matt), then PR.

## Follow-up — copyText split out of diagnostics

Slice 9 deliberately excluded content changes, so round 3 left `diagnostics.ts`
whole in `shared/lib` even though only `copyText` belonged there. Done as a
follow-up:

- `shared/lib/clipboard.ts` — `copyText` alone (clipboard API + the execCommand
  fallback that plain-http LAN origins still need).
- `features/settings/diagnostics.ts` — `DiagnosticCheck`, `diagnosticFetch`,
  `buildDiagnosticsReport` move back down to the feature that owns them.
- Tests split the same way; `SettingsView.test.tsx` now spies on each module
  separately, since `vi.spyOn` must target the module the subject actually imports.

Result: **no file under `features/task` or `shared/` references `features/settings`
at all any more.** The layering holds by construction rather than by exemption.

- PASS — vitest **380/380** across 43 files (one more file, same test count, so
  nothing was lost in the split), `web:check` 0, `web:lint` 0, `web:build:check` 0,
  `cargo nextest -p ajax-web` 159/159, mobile-webkit **93 / 2 skipped**.
- Both split test files mutation-checked: breaking the `execCommand` return fails
  `clipboard.test.ts`, and changing `browser_mode` fails `diagnostics.test.ts`.

## On-device gate

- **PASS (Matt, 2026-07-19)** — validated on iPhone. S9 cleared for PR.
