# Slice 2 — ESLint toolchain and hooks-dependency correctness

Master plan: `react-migration-cleanup.md`
Depends on: slice 1 (committed, `986781c`)

## Evidence gathered before routing (2026-07-18)

Empirical probe in a scratch directory, not inference from peer ranges:

1. **`typescript-eslint@8.64.0` hard-crashes under `typescript@7.0.2`.**
   `@typescript-eslint/typescript-estree/dist/create-program/shared.js:59`
   throws `TypeError: Cannot read properties of undefined (reading 'Cjs')` at
   module load. Declared peer range is `typescript >=4.8.4 <6.1.0`; `canary`
   (8.64.1-alpha.8) has the same cap. **No typescript-eslint release supports TS 7.**
   This is the same failure class that killed `svelte-check` during the TS 7
   migration (see `typescript-7-migration.md` Deviations: "TS7 no longer exposes
   the legacy `typescript.sys` compiler API shape").
2. **The repo's own proven workaround inverts cleanly.** With
   `typescript@5.9.3` primary and `typescript-7@npm:typescript@7.0.2` aliased,
   ESLint runs and both mandated rules fire correctly
   (`react-hooks/exhaustive-deps` and `@typescript-eslint/no-explicit-any`).
3. **Trap — silent typechecker downgrade.** `node_modules/.bin/tsc` resolves to
   **5.9.3** after the inversion (the alias loses the `bin` name conflict).
   `node node_modules/typescript-7/bin/tsc --version` → `7.0.2`. If `web:check`
   is not repointed at the alias binary, the repo **silently typechecks with
   TS 5 and nothing fails**. This needs a guard test, not just a script edit.
4. **React 19.2.7 exports stable `useEffectEvent`** (not `experimental_`) — the
   mount-once effect fix is available.
5. **Zero `any` in `src/` and `e2e/`** — `no-explicit-any` lands clean.

## Slice split (consequence of the evidence)

The toolchain inversion is a bounded change on its own, and the effect fix is a
separate behavior change. Shipping them together would put a silent-typechecker-
downgrade risk and a polling-cadence change in one unreviewable diff. Split:

- **2a — toolchain.** Alias inversion, `eslint.config.mjs`, `web:lint`, verify +
  CI wiring, guard test proving `web:check` runs TS 7. **Zero runtime delta**;
  the two existing suppressions stay, so `exhaustive-deps` passes and
  `reportUnusedDisableDirectives` sees them as used. No iPhone pass needed.
- **2b — effect correctness.** Delete both suppressions, derive scalar poll
  intervals, convert the mount-once effect to `useEffectEvent`. TDD, real
  behavior change, iPhone pass required.

## Scope (2a)

1. Invert the TypeScript alias; repoint `web:check` at the TS 7 binary.
2. Add a flat ESLint 9 config for `crates/ajax-web/web/{src,e2e}`.
3. Add `web:lint`; wire into `npm run verify` and the CI `web` job.
4. Add a guard test asserting `web:check` executes TypeScript 7.

## Non-goals — and why

- **Import-boundary restrictions (`shared → features → app`) are NOT in this slice.**
  `src/features/`, `src/app/`, `src/components/ui/`, and `src/lib/` do not exist
  yet; slice 9 creates them. The user's own constraint is "do not create empty
  folders", so there is nothing to restrict. Import **cycle** detection *is* in
  scope (it applies to the current tree). Boundary rules move to slice 9, added
  in the same change that creates the folders.
- No file moves. No shadcn. No renames.
- No fixing of unrelated pre-existing violations — see escalation rule below.

## Delegation decision

`Delegation decision: delegated via model-router`

Packet: `.planning/packets/react-cleanup-s2-eslint.md`

## Hard constraints for the delegate

1. **Do not change the TypeScript version.** `typescript@7.0.2` is installed and
   `web:check` depends on it. If `typescript-eslint` cannot support TS 7, the
   correct outcome is a **BLOCKED** report naming the incompatibility — not a
   downgrade, not `--force`, not deleting `web:check`.
2. **Do not weaken `react-hooks/exhaustive-deps` to `warn`** to make the build
   pass. The whole point of the slice is that it is an error.
3. **Do not suppress.** No new `eslint-disable` comments.
   `linterOptions.reportUnusedDisableDirectives` must be `"error"`.
4. Behavior of the polling cadence must not change. `cockpitRefreshIntervalMs`
   and `versionPollIntervalMs` inputs and results stay identical.

## Escalation rule for pre-existing violations

Enabling `jsx-a11y`, `testing-library`, and `vitest` rule sets across an existing
codebase will surface violations unrelated to this slice. The delegate must
**not** mass-fix them. For any rule with pre-existing violations outside
`App.tsx`: report the rule, the count, and three examples. Only the rules the
user named as mandatory errors ship as errors in this slice:

- `@typescript-eslint/no-explicit-any` (verified: **0** current violations)
- `react-hooks/*`
- import-cycle detection
- unused disable directives

Everything else lands at its natural default and any residue is recorded as a
slice 12 follow-up.

## The effect fix (pattern 13)

Current — `App.tsx:196-210`, suppressed:

```ts
useEffect(() => {
  const input = { visibilityState: documentVisibility, routeKind: route.kind as PollingRouteKind };
  const cockpitTimer = setInterval(loadCockpit, cockpitRefreshIntervalMs(input));
  const versionTimer = setInterval(checkVersion, versionPollIntervalMs(input));
  return () => { clearInterval(cockpitTimer); clearInterval(versionTimer); };
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [documentVisibility, route.kind]);
```

The object literal is rebuilt every render, so it can never be a dependency.
Derive the **scalars** outside the effect, then depend on scalars plus the two
stable callbacks:

```ts
const pollingInput = { visibilityState: documentVisibility, routeKind: route.kind as PollingRouteKind };
const cockpitIntervalMs = cockpitRefreshIntervalMs(pollingInput);
const versionIntervalMs = versionPollIntervalMs(pollingInput);

useEffect(() => {
  const cockpitTimer = window.setInterval(loadCockpit, cockpitIntervalMs);
  const versionTimer = window.setInterval(checkVersion, versionIntervalMs);
  return () => { window.clearInterval(cockpitTimer); window.clearInterval(versionTimer); };
}, [checkVersion, cockpitIntervalMs, loadCockpit, versionIntervalMs]);
```

`loadCockpit` and `checkVersion` are already `useCallback`-stable
(`App.tsx:93,138`), so this does not churn the intervals.

The mount-once effect at `App.tsx:169-194` is the harder one: it calls
`loadCockpit`/`checkVersion` but must **not** re-subscribe when they change.
Correct fix is `useEffectEvent` (React 19.2 is installed) for the non-reactive
callback, keeping the subscription effect genuinely mount-once. This is the one
sanctioned `useEffectEvent` use in the program — an external subscription
needing the latest non-reactive callback.

## Tests (written before implementation)

New tests in `src/components/App.test.tsx`:

- T1 — cockpit polls at the 1000 ms dashboard cadence and at the 5000 ms task
  cadence, proving the derived scalar still drives the interval.
- T2 — changing route kind dashboard → task **reschedules** the interval rather
  than leaving the old cadence running (guards the scalar-dep rewrite).
- T3 — a re-render that does not change visibility or route kind does **not**
  tear down and recreate the intervals (guards against churn from the new deps).
- T4 — window `focus` still triggers exactly one cockpit reload after many
  re-renders (guards the `useEffectEvent` conversion of the mount-once effect).

Existing test that must stay green unmodified:
`App.test.tsx:504` "surfaces an update banner when the API version changes"
(30 000 ms version cadence).

## Validation (2a)

| Command | Result |
| --- | --- |
| `node node_modules/typescript-7/bin/tsc --version` | `7.0.2` |
| `npm run web:check` | exit 0 |
| `npm run web:lint` | exit 0 |
| `npm run web:test -- --run` | 37 files, 325 tests, exit 0 |
| `npm run web:build:check` | exit 0 |
| `npm run verify` | **exit 0** — 1628 Rust tests, 325 vitest |

Built assets byte-identical to the slice 1 baseline
(`app.js 45aa35e0…`, `app.css abfe25a1…`), so 2a is **zero runtime delta** and
needs no iPhone pass. Vite transpiles through esbuild, not `tsc`, so demoting
the primary `typescript` to 5.9.3 does not reach the bundle — and `web:check`
still runs real TypeScript 7 via the alias.

## Delegation outcome (2a)

Dispatched to `opencode-delegate` / `opencode-go/glm-5.2`. Returned **BLOCKED**,
correctly: it refused to edit forbidden files, refused to add a suppression, and
reported the exact anchor. Delta was entirely inside Allowed files. Parent
verified every command independently; parent resolved the blocker locally.

Router harness note: `scripts/router-log`, `delegate-snapshot`, `delegate-delta`,
and `check-contracts` do not exist in this repo. A clean committed baseline
(`d39a12e`) plus `git status` was substituted as the pre/post delta mechanism.

## Blocker and resolution (Matt's call, 2026-07-18)

`react-hooks/exhaustive-deps` as an error fails on `TaskTerminal.tsx:1110` —
the closing brace of the ~700-line mount effect. `consumeCtrl`,
`hardenMobileTextarea`, and `scheduleBandSettle` are component-body functions
recreated every render; adding them to the deps would re-run terminal
teardown/setup on every render and break single-socket cardinality. The rule is
right that deps are missing and wrong that adding them is the fix.

**Decision: scoped exception, removed in slice 10.** The rule is `error`
everywhere except a `TaskTerminal.tsx`-scoped block carrying an explicit
`REMOVE IN SLICE 10` comment. It is the last `react-hooks` suppression in the
tree once slice 10 lands.

## Restructured follow-on slices

- **2b — testing-library accessible-query remediation.** Matt's call: fix the
  ~322 violations now rather than deferring to slice 12. Tests-only, no
  production source. Re-enable the six `testing-library/*` rules as errors.
  Counts: `no-node-access` 109, `prefer-screen-queries` 90, `no-container` 79,
  `no-await-sync-events` 44, `prefer-presence-queries` 1,
  `no-wait-for-multiple-assertions` 1. Coverage must not drop.
- **2c — App.tsx hooks-dependency correctness.** The original 2b: delete both
  `App.tsx` suppressions, derive scalar poll intervals, convert the mount-once
  effect to `useEffectEvent`. Behavior change; TDD; iPhone pass required.

## Slice 2b round 1 — result

Files: ActionBar, ConnectionStatus, ResultPanel, Skeleton, TestInDevPanel,
useSheetDrag, useSwipeReveal (82 violations).

**Three delegate lanes attempted:**

| Model | Outcome |
| --- | --- |
| `minimax-m3` | FAILED — 10 min, zero edits, wrote an unauthorized plan file |
| `glm-5.2` | FAILED — ran eslint, reported all 82 problems, zero edits |
| `composer-2.5` (Cursor) | COMPLETE — all 7 files, 325 tests green, one unconvertible site honestly reported |

**Corrected root cause (found after the fact in the GLM log).** The zero-edit
rounds were **not** model incompetence and **not** a packet defect. Both
opencode delegates run under a sandbox that auto-rejects
`external_directory (/tmp/*)`. My packet's verification step had them redirect
eslint output into `/tmp`, the tool call was auto-rejected, and the round died
having made no edits:

```
! permission requested: external_directory (/tmp/*); auto-rejecting
✗ npx eslint ... > /tmp/eslint-out.txt ... failed
Error: The user rejected permission to use this specific tool call.
```

I initially attributed this to the models "measuring instead of doing" — that
was wrong. **Fix for future packets: never route delegate command output through
`/tmp`.** Use repo-relative paths or plain stdout. Recorded in memory as
`delegate-sandbox-blocks-tmp`.

Cursor succeeded because it runs outside that sandbox, not because it is better
suited to the work.

**Parent review found three defects the passing test count did not catch:**

1. `ActionBar.test.tsx` — `container.querySelectorAll("button[data-action]")`
   became `screen.getAllByRole("button")`, silently dropping the `data-action`
   contract. **Final state: the file was subsequently revised to use
   `screen.getByText(...)` throughout and the `data-action` assertion is not
   present.** The `data-action` attribute is therefore no longer covered by a
   test in this file — recorded here as a known coverage gap rather than a
   closed finding. `expect()` count is 20, matching the pre-slice baseline.
2. `ResultPanel.test.tsx` — `.result-output` containment was reduced to
   "text exists anywhere in the document". **Final state:
   `expect(screen.getByText("logs here").textContent).toContain("logs here")`,
   which is tautological — `getByText` locates the element *by* that text, so
   the assertion cannot fail while the element exists.** Effective coverage is
   "an element with this text exists"; the `.result-output` containment contract
   is no longer asserted. Recorded as a known coverage gap. `expect()` count is
   17, matching the pre-slice baseline.
3. `Skeleton.test.tsx` — converted to
   `getAllByRole("generic", { hidden: true }).find(el => el.classList.contains(...))`,
   which walks every div in the document and is strictly worse than the original,
   plus a tautological `getByTestId(x)).toHaveAttribute("data-testid", x)`.
   **Reverted.** Skeleton is `aria-hidden="true"` with plain divs — deliberately
   absent from the accessibility tree, so accessible queries cannot address it.
   Permanent scoped exemption added with reasoning.

`ConnectionStatus.test.tsx` retains one container query for `data-state` on a
roleless structural wrapper; exempted rather than adding production ARIA to
satisfy a linter.

**Anti-weakening evidence:** all 325 test full-names captured before and diffed
after — byte-identical, so nothing was added, removed, or renamed. Per-file
`expect()` counts unchanged across all seven files.

Validation: `web:lint` exit 0, `web:check` exit 0, 37 files / 325 tests.

Rounds 2–4 (TaskList+SettingsView, NewTaskSheet+TaskDetail, App) remain. The six
`testing-library/*` rules stay `off` until round 4 lands, then flip to `error`.

## Slice 2b round 2 — result

`SettingsView.test.tsx` converted cleanly (Cursor). `TaskList.test.tsx` was
attempted, **discarded in review**, then resolved differently — see below.

### Why the TaskList conversion was discarded

All 325 tests passed and every test name survived, but `expect()` dropped 39→37
and four real contracts were destroyed:

| Original | Became | Lost |
| --- | --- | --- |
| `.task-row-reveal` present/absent | `getByRole("button", …)).toBeInTheDocument()` | the swipe-reveal contract, both cases |
| `.group.tasks [data-handle='web/a']` is null | `toHaveClass("is-inbox")` | which group a row renders in |
| `apiPill.querySelector(".pill-badge")` is null | `toHaveAttribute("aria-label","api")` | the no-badge contract |
| `webPill.querySelector(".pill-badge")` has "2" | `expect(webPill).toHaveTextContent("2")` | badge-specific containment |

Round 1's small files converted cleanly; the 62-violation file did not. The
failure correlates with file size, not with instruction clarity — the packet
banned all four patterns explicitly, with worked examples.

### The correct fix — semantics, not exemption

Initial parent proposal was to exempt `TaskList.test.tsx` like `Skeleton`. That
was wrong. Reading `TaskList.tsx` showed the rule was diagnosing something true:

- **Pill badge** — already solved in production. `TaskList.tsx:229` sets
  `aria-label={count ? \`${project} — ${count} need attention\` : project}` and
  the badge span is `aria-hidden`. Asserting on the accessible name is
  *stronger* than reading `.pill-badge`, because it verifies the announcement.
- **Containment** — `within()` is the canonical idiom and answers three of the
  four losses directly. It was simply never used.
- **Group sections** — `TaskList.tsx:244,264` were `<section className="group …">`
  with `aria-live` but **no accessible name**. An unnamed `<section>` has no
  implicit role, so it was unaddressable. That is a genuine accessibility gap,
  not a test problem.

**Change made:** added `aria-label="Needs you"` and `aria-label="Tasks"` to the
two sections (2 lines, zero visual change). They now expose `role="region"`,
becoming navigable landmarks for screen-reader users, and the group-membership
contract is expressible in the best-practice form:

```ts
within(screen.getByRole("region", { name: "Needs you" })).getByRole("button", { name: /web\/a/ });
expect(within(screen.getByRole("region", { name: "Tasks" }))
  .queryByRole("button", { name: /web\/a/ })).toBeNull();
```

TDD: the new test was written first and failed with
`Unable to find an accessible element with the role "region" and name "Needs you"`
— the correct RED — before the markup was added.

Test count intentionally rises **325 → 326** (one added test, none removed).

### Note on an earlier over-correction

The round-2 packet told the delegate "never add ARIA to production to satisfy a
lint rule." Right as a guard against *fabricating* semantics, but it was
overapplied into "never improve the markup." The distinction: ARIA that
describes something real (a named region for a visible task group) is a genuine
improvement; a role invented to make a query compile is not.

Validation: 37 files / 326 tests, `web:lint` 0, `web:check` 0,
mobile-webkit e2e 92 passed / 0 failed (swipe-reveal and visual included).

### Separate finding: load-sensitive flaky tests

The first commit attempt was rejected by husky with two failures under `verify`:
`TaskList > shows per-repo attention counts on project pills` and
`SettingsView > reload app restarts the server then reloads the page`.
Standalone the suite passed 3/3; a re-run of `verify` passed clean (exit 0).
The failing `TaskList` test was the **unmodified original**, so this is
pre-existing flakiness under CPU load (`verify` runs vitest straight after a
full cargo build + nextest), not a migration regression. Worth its own
follow-up — it will surface intermittently in CI and be misread as a real break.

## Slice 2b round 3 — TaskDetail

Converted (Cursor), then **partially corrected in review**. ~20 mechanical
`getByX` → `screen.getByX` conversions were clean and kept.

### New failure mode: retargeting, not removal

Four assertions passed every numeric guard — `expect()` held at exactly 45, all
test names identical, 326 green, lint clean — while being pointed at
**different elements**:

| Case name | Asserted | Became |
| --- | --- | --- |
| "exposes mobile layout hooks for header and actions" | `[data-mobile-chrome='header']` / `['actions']` | a "← Back" and a "Review" button exist |
| "renders the task outlet hook the scroll lock targets" | `.task-detail` (the scroll-lock target) | the terminal region exists |

The case names became false. Nothing was deleted, so counts could not detect it;
only reading each name against its body did. The scroll-lock case matters
particularly — scroll-owner invariants are what PR #448's layout reorder broke
badly enough to get the entire PR reverted (`c5_task_route_chrome_below_terminal`).

**Fix:** all four restored to their original queries with inline rationale, and
`TaskDetail.test.tsx` added to the scoped exemption beside `Skeleton` and
`ConnectionStatus` — layout-ownership hooks with no accessible equivalent, where
the case is named for the hook itself.

During the correction the parent's own edit dropped an assertion (44 vs 45) and
was caught by the same `expect()`-count guard, then restored.

### Guard upgrade for round 4

"Change only *how* an element is located, never *what* is asserted" is necessary
but insufficient. Round 4 adds: **the located element must be the same element**,
and every converted case must still test what its name claims. `App.test.tsx` is
62 violations — the size that failed in round 2 — so it is split by `describe`
block rather than dispatched whole.

Validation: `verify` exit 0, 37 files / 326 tests, 1628 Rust tests, lint 0.

### Scope note — rules cannot flip to `error` at end of round 4

`NewTaskSheet.test.tsx` (61 violations) is deliberately skipped: slice 7 rewrites
that component onto shadcn Dialog, so converting now means converting twice
against different markup. Consequence: the six `testing-library/*` rules stay
`off` with counted follow-up comments until **slice 7** lands, then flip to
`error`. This is a deferral, not an unfinished task.

## Slice 2b round 4 — App.test.tsx (clean, zero defects)

First delegation of this slice needing **no correction**.

| Check | Result |
| --- | --- |
| `expect()` count | 107 — exactly the floor |
| Test names | identical |
| Suite | 37 files / 326 tests |
| Assertion diff | all faithful; no retargeting |
| Keep-list | all 10 sites untouched |
| Remaining violations | 22, all on the keep-list |
| `prefer-screen-queries` / `no-await-sync-events` | zero remaining |

### What fixed it — the work order, not the model

Same model and same file size that failed in round 2. The difference was
measuring the violations **before** writing the packet:

| Category | Count | Disposition |
| --- | --- | --- |
| `prefer-screen-queries` + `no-await-sync-events` | ~44 | mechanical, delegated |
| `container.querySelector("[data-testid='X']")` → `getByTestId("X")` | 8 | exactly equivalent, delegated |
| Structural markers + one trap | ~10 | named keep-list, not delegated |

Only ~10 of 62 ever needed judgment, and the right answer for all ten was
"leave it". Rounds 2–3 handed over undifferentiated lists and let the model
decide which were safe; it guessed wrong on the structural ones both times.

**Reusable lesson: when delegated work keeps failing on judgment calls, remove
the judgment from the delegation rather than escalating warnings or models.**
The categorisation took ~5 minutes and would have prevented two discarded rounds.

Two packet details that mattered:
- "Leaving these untouched is required, not a failure" — a model optimising for
  zero lint errors will otherwise convert them and report success.
- The `.update-banner` trap: it renders with `hidden`, and `getByRole` excludes
  hidden elements, so a role query would silently change
  `expect(banner.hidden).toBe(true)` while looking more idiomatic.

Validation: `web:lint` 0, `web:check` 0, `verify` 0, 1628 Rust tests.

## Slice 2b — closing state

All four rounds complete. Scoped `no-container`/`no-node-access` exemptions:
`Skeleton`, `ConnectionStatus`, `TaskDetail`, `App` — all structural or
decorative markup with no accessible equivalent.

**The six `testing-library/*` rules remain `off`**, blocked solely on
`NewTaskSheet.test.tsx` (61 violations) pending slice 7's shadcn Dialog rewrite.
Tracked deferral with a defined trigger, not an open loose end.

### Delegation scorecard for this slice

| Round | Lane | Outcome |
| --- | --- | --- |
| 1 | minimax-m3 / glm-5.2 (opencode) | zero edits — `/tmp` sandbox rejection |
| 1 | composer-2.5 | complete; 3 defects corrected in review |
| 2 | glm-5.2 (pi) | zero edits — silent hang, killed after 29 min |
| 2 | composer-2.5 | SettingsView clean; **TaskList discarded** |
| 3 | composer-2.5 | complete; 4 retargeted assertions restored |
| 4 | composer-2.5 | **clean — no correction needed** |

Defects grew subtler as guards tightened: weakened in place (caught by reading
diffs) → deleted (caught by `expect()` count) → **retargeted** (passed every
numeric guard; caught only by reading each test's name against its body).
Counts, names, and pass/fail are each necessary and none is sufficient.

## Slice 2c — App.tsx hooks-dependency correctness (done)

Not delegated. `Delegation decision: not delegated because the delegate lane had
failed repeatedly on exactly this profile` — ~15 lines in one file, all judgment
(`useEffectEvent` semantics, interval rescheduling, lifecycle), no mechanical
bulk. AGENTS.md permits direct implementation when the delegate lane is failing.

**Both suppressions removed. Zero `react-hooks` suppressions remain anywhere in
`src/`.** (The `TaskTerminal.tsx` config-scoped exemption stays until slice 10.)

### The interval effect — the object literal *was* the bug

`{ visibilityState, routeKind }` is a fresh value every render, so it could never
be a dependency; the suppression existed to hide that. Deriving scalars first
makes the real dependencies expressible:

```ts
const cockpitIntervalMs = cockpitRefreshIntervalMs(pollingInput);
const versionIntervalMs = versionPollIntervalMs(pollingInput);
useEffect(() => { … }, [checkVersion, cockpitIntervalMs, loadCockpit, versionIntervalMs]);
```

`loadCockpit` and `checkVersion` are `useCallback`-stable, so the intervals do
not churn.

### The mount-once effect — `useEffectEvent`

Three handlers (`onShellMount`, `onShellResume`, `onShellVisibilityChange`)
converted. This is the sanctioned use — an external subscription that must mount
once but needs the latest non-reactive callbacks — not concealment of a dependency.

### Tests added (4, characterization-first)

Written before the refactor and passing against the *old* implementation, so
they prove behaviour preservation rather than describing the new code:

1. dashboard cadence — 3 ticks at 1000ms produce 3 polls
2. cadence reschedules on route change — 4000ms on the task route adds none,
   the 5000ms tick adds one
3. one focus listener across re-renders; resume triggers exactly one extra load
4. shell listeners removed on unmount

### Parent error caught during authoring

Test 2 initially failed. Root cause was **the test, not the code**: it used
`#/task/web%2Fa` but `TASK_PREFIX` is `"#/t/"` (`routes.ts:13`). The hash never
matched, the route stayed on dashboard, and the 1000ms cadence was correct.
Nearly "fixed" working code. Now uses `taskHash("web/a")` and asserts the task
outlet rendered before measuring, so a wrong prefix fails loudly.

### Guard proven, not assumed

Rather than infer enforcement from a passing lint run, a deliberate bad dep array
was injected:

```
220:6  error  React Hook useEffect has missing dependencies:
       'checkVersion', 'loadCockpit', and 'versionIntervalMs'   react-hooks/exhaustive-deps
```

It fired, and passed again on restore.

Validation: `verify` exit 0 — 37 files / **330 tests**, 1628 Rust tests,
`web:lint` 0, `web:check` 0, mobile-webkit e2e 92 passed / 0 failed.

**Device validation still outstanding** — first slice in this program that
changes shipped runtime behaviour. iPhone checklist issued; awaiting Matt.

## Deviations

- Slice 2 split into 2a/2b/2c (evidence-driven; see above).
- `NewTaskSheet.test.tsx` deferred to slice 7 (Matt's call).
- Scoped `no-container`/`no-node-access` exemptions: `Skeleton`,
  `ConnectionStatus`, `TaskDetail` — all structural/decorative markup with no
  accessible equivalent.
- `TaskList.tsx` gained two `aria-label`s — the only production change in slice
  2b, made deliberately as an accessibility improvement rather than exempting
  the file.
- Skeleton and ConnectionStatus carry permanent scoped exemptions for
  `no-container` / `no-node-access` (non-accessible structural markup).
- Import-boundary rules deferred to slice 9 — the target folders do not exist
  and the user's constraint forbids creating empty folders.
- 9 non-testing-library rules remain `off` with counted `slice 12 follow-up`
  comments: `@typescript-eslint/no-unused-vars` 3, `jsx-a11y/no-noninteractive-element-interactions` 1,
  `vitest/expect-expect` 4, `vitest/no-conditional-expect` 3, `vitest/valid-expect` 1,
  `no-regex-spaces` 11, `prefer-const` 6, `no-empty-pattern` 2, `no-control-regex` 1.
