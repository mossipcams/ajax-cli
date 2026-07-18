PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Migrate `TaskDetail.svelte` → `TaskDetail.tsx` (React island parent) with byte-behavior parity. It composes the already-migrated `TaskTerminal.tsx`, `ActionBar.tsx`, and `TestInDevPanel.tsx` **natively** (no inner `ReactIsland`). Move its scoped `<style>` into global `styles.css` (de-scoped). Port `TaskDetail.test.ts` → `.test.tsx` (RTL + repointed `?raw` source-contract greps). Delete the frozen `ActionBar.svelte` + its Svelte test (`ActionBar.tsx` is sole impl). Swap `App.svelte` to mount TaskDetail via `ReactIsland`. Repoint the `legacyTerminalRemoval` guard path.

Worktree: `/Users/matt/Desktop/Projects/ajax-cli__worktrees/react-s6`, branch `ajax/react-s6` (Round A already committed at 852d71c).

## Allowed files

- `crates/ajax-web/web/src/components/TaskDetail.tsx` (create)
- `crates/ajax-web/web/src/components/TaskDetail.test.tsx` (create)
- `crates/ajax-web/web/src/styles.css` (append de-scoped TaskDetail styles)
- `crates/ajax-web/web/src/components/App.svelte` (import + mount swap only)
- `crates/ajax-web/web/src/legacyTerminalRemoval.test.ts` (repoint one path)
- delete `crates/ajax-web/web/src/components/TaskDetail.svelte`
- delete `crates/ajax-web/web/src/components/TaskDetail.test.ts`
- delete `crates/ajax-web/web/src/components/ActionBar.svelte`
- delete `crates/ajax-web/web/src/components/ActionBar.test.ts`

## Forbidden changes

- No edits to frozen TS modules (`state.ts`, `taskActions.ts`, `diagnostics.ts`, `api.ts`, `types.ts`, `polling.ts`, `viewport.ts`, terminal modules) beyond imports.
- No edits to `ActionBar.tsx`, `TaskTerminal.tsx`, `TestInDevPanel.tsx`, `ReactIsland.svelte`, `mountIsland.tsx`.
- No e2e edits. No testid, DOM-hook, class-name, or behavior changes. No `data-mobile-chrome` renames.
- Do not weaken any assertion in the ported test — adapt `?raw` greps to the post-migration source of truth, preserving intent.
- This is an approved large mechanical port (like S5 TaskTerminal): the ~400-line soft cap does not force a stop; do the whole file.
- Do not commit, push, merge, rebase, or change branches.

## Context evidence

- **Component**: `TaskDetail.svelte:1-146` markup (header, `.interact-panel` w/ ActionBar, TaskTerminal island, `<details class="meta-details" bind:open={metaOpen}>` w/ Dev/Branch/Agent/Activity/Attempts/Notes). Styles `:148-447` incl. `@media` mobile block and `:global(.action-row)`/`:global(.action)` reaching into ActionBar DOM.
- **Props** (unchanged): `{ detail, onBack, onCockpit, onResult, onMutated, onDismiss }`. Anchor `TaskDetail.svelte:11-20`.
- **Derived**: `meta = statusMeta(detail.status)`, `actions = visibleTaskActions(detail.actions)`, `activityLine` (agent_activity/live_status_summary, null if === status_explanation), `metaOpen` state (default false), `nowSecs()`, `absoluteTime()`. Anchor `:22-36`.
- **Native children now**: TaskTerminal — replace `<ReactIsland component={TaskTerminal} props={{ handle }}/>` with `<TaskTerminal handle={detail.qualified_handle} />`. TestInDevPanel — replace `<ReactIsland component={TestInDevPanel} props={{ taskHandle, onResult }}/>` with `<TestInDevPanel taskHandle={detail.qualified_handle} onResult={onResult} />`. ActionBar — `<ActionBar actions={actions} handle={detail.qualified_handle} onCockpit=… onResult=… onMutated=… onDismiss=… />`. `ActionBar.tsx` props already match (`ActionBar.tsx:6-22`).
- **ActionBar.tsx is the impl** already used by TaskList (S2); `ActionBar.svelte` is the frozen duplicate to delete.
- **Test surface** (`TaskDetail.test.ts:1-288`): behavioral render tests + CSS/source-contract `?raw` greps. `?raw` imports: `TaskDetail.svelte?raw` (CSS greps + `ajax-task-open` absence), `RouteScroll.svelte?raw`, `App.svelte?raw` (task-outlet mount grep). Mocks `../api` `fetchDevDeploy`. Stubs `ResizeObserver`.
- **App.svelte mount**: import `./TaskDetail.svelte` at `App.svelte:15`; `<TaskDetail {detail} onBack=… onCockpit={applyCockpit} onResult={showResult} onMutated=… onDismiss=… />` at `App.svelte:296-303`, inside `<section data-outlet="task" …>` gated by `{#if detail}`.
- **Guard**: `legacyTerminalRemoval.test.ts:73-76` — `collectSymbolViolations("…/TaskDetail.svelte", ["TerminalSurfaceSelector"])`. Repoint path to `TaskDetail.tsx`.
- **CSS relocation precedent (S5)**: scoped `<style>` → `styles.css`; `:global(x)` wrappers dropped (global scope reaches child DOM directly). Source-contract CSS greps repointed to `styles.css?raw`.

## Code anchors

- `TaskDetail.svelte:39-146` — markup to port to JSX (`class`→`className`, `{#if}`→`&&`/ternary, `{#each x as y (key)}`→`.map`, `bind:open`→controlled `open={metaOpen}` + `onToggle`, `onclick`→`onClick`).
- `TaskDetail.svelte:148-447` — styles to append to `styles.css`, removing `:global(...)` wrappers (`.interact-panel :global(.action-row)` → `.interact-panel .action-row`; `.interact-panel :global(.action)` → `.interact-panel .action`).
- `TaskDetail.test.ts:57-122` behavioral + `:125-288` projection/CSS tests.
- `App.svelte:15,296-303`; `legacyTerminalRemoval.test.ts:73-76`.

## Test-first instructions

Port `TaskDetail.test.ts` → `TaskDetail.test.tsx` first, then implement to green.

1. **Behavioral tests** → `@testing-library/react`; `import TaskDetail from "./TaskDetail";`; `render(<TaskDetail detail={detail()} />)` (props as JSX attrs, e.g. `onBack={onBack}`). Keep every assertion identical (testids, `.interact-pill`/`.task-detail` queries, `← Back`, relative-times, attempts, annotations, observation-error, agent-activity, Test-in-Dev-in-meta-details waitFor). Keep the `../api` mock + `ResizeObserver` stub.
2. **Source-contract `?raw` greps — repoint, do not weaken**:
   - CSS greps currently on `taskDetailSource` (`.interact-summary` block; `@media` mobile block; `.meta-details margin-top:0`; `.interact-panel flex-direction:row`; `min-height:0`; compacted paddings) → import `stylesSource from "../styles.css?raw"` and run the same regexes against it. The scoped mobile `@media` block delimiter `\n  \}` (2-space indent) may differ in `styles.css`; adjust the block-capture regex to the actual `styles.css` indentation while asserting the **same** properties.
   - `.interact-panel :global(\.action)` greps (lines 281-286) → match `.interact-panel .action` (no `:global`) in `styles.css`.
   - `taskDetailSource` absence check `not.toMatch(/ajax-task-open/)` (line 109) → keep, import `taskDetailSource from "./TaskDetail?raw"` (the `.tsx`).
   - `appSource` task-outlet grep (line 94) → update the regex to the new mount form: `/<section[^>]*data-outlet="task"[^>]*>[\s\S]*?<ReactIsland[^>]*component=\{TaskDetail\}/` (intent preserved: TaskDetail mounts inside the task outlet).
   - `routeScrollSource` grep (line 110) unchanged.
3. Red command (component absent → import/assert fails):
   ```bash
   npm run web:test -- --run crates/ajax-web/web/src/components/TaskDetail.test.tsx
   ```
   Record nonzero exit + failing assertion as RED evidence, then implement to GREEN.

## Edit instructions

1. **Create `TaskDetail.tsx`** — functional component, props interface as above. `metaOpen` via `useState(false)`; `<details open={metaOpen} onToggle={(e) => setMetaOpen(e.currentTarget.open)}>`. Compute `meta`, `actions`, `activityLine`, `nowSecs`, `absoluteTime` in render. Port markup 1:1 preserving every `className`, `data-testid`, `data-mobile-chrome`, and text. Children composed natively (TaskTerminal/ActionBar/TestInDevPanel imported from `./TaskTerminal`, `./ActionBar`, `./TestInDevPanel`). Keep the `detail.repo === "ajax-cli"` gate around the Dev group.
2. **Append TaskDetail styles to `styles.css`** verbatim from `TaskDetail.svelte:149-446`, dropping the `:global(...)` wrappers. Do not alter values.
3. **`App.svelte`**: change import line 15 to `import TaskDetail from "./TaskDetail";`; replace the `<TaskDetail … />` block (296-303) with `<ReactIsland component={TaskDetail} props={{ detail, onBack: () => go(selectedProject ? projectHash(selectedProject) : dashboardHash()), onCockpit: applyCockpit, onResult: showResult, onMutated: () => route.kind === "task" && route.handle && loadDetail(route.handle), onDismiss: () => go(dashboardHash()) }} />` (mirror the existing callbacks exactly).
4. **`legacyTerminalRemoval.test.ts`**: change the `collectSymbolViolations` path `"…/TaskDetail.svelte"` → `"…/TaskDetail.tsx"` (symbol list unchanged).
5. **Delete** `TaskDetail.svelte`, `TaskDetail.test.ts`, `ActionBar.svelte`, `ActionBar.test.ts`.

## Verification commands

```bash
npm run web:test -- --run
npm run web:check
npm run web:build
grep -c serviceWorker crates/ajax-web/web/dist/app.js   # expect 0
npm run web:build:check
cargo nextest run -p ajax-web
```

## Acceptance criteria

- Full `web:test` green (ported TaskDetail + ActionBar coverage still holds via `ActionBar.test.tsx`).
- `web:check` clean; `web:build` + `build:check` pass; `serviceWorker`=0; `cargo nextest -p ajax-web` green.
- No `.svelte` TaskDetail/ActionBar files remain; `App.svelte` mounts TaskDetail via `ReactIsland`; guard repointed.
- No testid/behavior/CSS-value change; diff limited to Allowed files.

## Stop conditions

- Any needed edit outside Allowed files (esp. a frozen TS module, `ActionBar.tsx`, or e2e).
- A `?raw` source-contract grep cannot be repointed without weakening its intent — stop and escalate (do not delete the assertion).
- `ActionBar.test.tsx` (the React suite) starts failing — indicates a shared-impl regression; stop.
- Behavior/visual parity cannot be preserved for the `:global` de-scoping.
