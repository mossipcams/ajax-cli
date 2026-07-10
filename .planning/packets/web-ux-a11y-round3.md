# Packet: cockpit shell accessible-feedback & orientation polish

## 1. Goal

Make the cockpit shell's feedback and navigation accessible and orienting:
error toasts announce assertively, project pills expose per-repo attention
counts and the active selection, the bottom nav and document title reflect the
current route, the new-task dialog receives focus when opened, and the empty
state points at the New-task CTA. Presentation-only; all data already arrives
from the server.

## 2. Allowed files

Production:
- `crates/ajax-web/web/src/components/ResultPanel.svelte`
- `crates/ajax-web/web/src/components/TaskList.svelte`
- `crates/ajax-web/web/src/components/App.svelte`
- `crates/ajax-web/web/src/components/NewTaskSheet.svelte`
- `crates/ajax-web/web/src/types.ts`

Tests:
- `crates/ajax-web/web/src/components/ResultPanel.test.ts`
- `crates/ajax-web/web/src/components/TaskList.test.ts`
- `crates/ajax-web/web/src/components/App.test.ts`
- `crates/ajax-web/web/src/components/NewTaskSheet.test.ts`

## 3. Forbidden changes

- `TerminalRawView.svelte`, all `terminal*.ts`, `gestures/`, `styles.css`,
  `viewport.ts`, `polling.ts`, `api.ts`, `contracts.ts`, `state.ts`,
  fixtures under `src/fixtures/`, anything under `crates/*/src/**/*.rs`,
  `dist/` (parent rebuilds it).
- No sort/ordering changes, no inbox visuals, no new dependencies.
- Do not weaken or delete any existing test or assertion.
- No formatting sweeps or renames outside the named edits.
- Do not change the four-status contract or derive any task state in the
  browser: `attention_items` must be read from `cockpit.repos.repos[n]`,
  never recomputed from cards/inbox.

## 4. Architecture context

Rust owns all task truth; the browser renders server projections
(`BrowserCockpitView` from `/api/cockpit`). `RepoSummary.attention_items`
is already serialized by `ajax-core::output::ReposResponse` (see committed
fixture `src/fixtures/cockpit.json` → `repos.repos[0].attention_items: 1`)
and is currently absorbed by the `[key: string]: unknown` index signature in
`types.ts`. UI components receive projections via props from `App.svelte`;
routing is hash-based via `routes.ts` (`parseRoute`, `Route.kind`:
dashboard | project | task | settings).

## 5. Code anchors

- `ResultPanel.svelte` line 25:
  `<div class="result-panel" class:is-error={isError} role="status" aria-live="polite">`
- `TaskList.svelte`:
  - pills loop: `{#each projects as project (project)}` → `<button … class="project-pill" class:is-active={selectedProject === project}`
  - `let projects = $derived([...new Set([…cards.map(card.repo), …repos.map(name)])].sort())` — pills are strings; repo summaries available via the `cockpit.repos.repos` prop.
  - empty state: `<p class="empty">{selectedProject ? `No tasks in ${selectedProject}` : "All quiet"}</p>`
- `App.svelte`:
  - nav snippet: `<button type="button" data-bottom-route="#/" onclick={() => go(dashboardHash())}>Dashboard</button>`
  - route state: `let route = $state<Route>(parseRoute(…))`; route kinds from `../routes`.
  - existing `$effect` blocks show the mount/teardown pattern to copy for a title effect.
- `NewTaskSheet.svelte`:
  - dialog root: `<div id="new-task-sheet" data-testid="new-task-sheet" role="dialog" aria-modal="true" … tabindex="-1"`
  - title input: `<input id="new-task-title-input" type="text" maxlength="80" …>`
  - reuse Svelte 5 patterns already in file: `$state`, `$effect`, `untrack`.
- `types.ts`: `export interface RepoSummary { name: string; [key: string]: unknown; }`
- Test patterns to reuse: `render(Component, { props })` from
  `@testing-library/svelte`; TaskList tests use a module-level `cockpit`
  const (already carries `repos: { repos: [{ name: "web" }, { name: "api" }] }` —
  extend with `attention_items`); NewTaskSheet tests use
  `const repos = [{ name: "web" }, { name: "api" }]`; App tests already
  mock fetch and drive `location.hash`.

## 6. Test-first instructions

Add each test, run the focused file, confirm it fails for the stated reason,
then implement. Focused command per file:
`npm run web:test -- crates/ajax-web/web/src/components/<File>.test.ts --run`

1. `ResultPanel.test.ts` — `"announces errors assertively"`: render with
   `{ message: "x", isError: true }`; assert the `.result-panel` element has
   `role="alert"` and `aria-live="assertive"`. Companion assertion: with
   `isError: false` it keeps `role="status"` / `aria-live="polite"`.
   Fails: role is hardcoded `status`.
2. `TaskList.test.ts` — `"shows per-repo attention counts on project pills"`:
   give the module cockpit `repos.repos = [{ name: "web", attention_items: 2 }, { name: "api", attention_items: 0 }]`;
   assert the `web` pill contains a `.pill-badge` with text `2` and an
   `aria-label` of `web — 2 need attention`; assert the `api` pill has no
   `.pill-badge`. Fails: badge markup absent.
3. `TaskList.test.ts` — `"marks the active project pill for assistive tech"`:
   render with `selectedProject: "api"`; assert the api pill has
   `aria-current="true"` and the All pill does not. Fails: attribute absent.
4. `TaskList.test.ts` — `"empty state points at the new-task CTA"`: render
   with `selectedProject: "api"` and a cockpit whose cards exclude api…
   simpler: reuse existing filter test setup with a project that has no
   cards (add `{ name: "docs" }` repo) and assert the empty text is
   `No tasks in docs yet — start one below.`; with an all-empty cockpit
   (cards: [], inbox empty) assert `All quiet — start a new task below.`
   Fails: current copy is `No tasks in docs` / `All quiet`.
5. `App.test.ts` — `"sets the document title per route"`: after initial
   render assert `document.title === "Ajax"`; set `location.hash = "#/settings"`,
   dispatch `hashchange`, `await tick()`, assert `"Settings — Ajax"`; for a
   task route (reuse an existing task-route test's setup) assert
   `"web/fix-login — Ajax"`. Fails: title never set.
6. `App.test.ts` — `"marks the dashboard nav button as current"`: on `#/`
   the `[data-bottom-route="#/"]` button has `aria-current="page"`; on
   `#/settings` it does not. Fails: attribute absent.
7. `NewTaskSheet.test.ts` — `"moves focus onto the dialog when opened"`:
   render, assert `document.activeElement === getByTestId("new-task-sheet")`.
   Fails: nothing focuses the container.
8. `NewTaskSheet.test.ts` — `"hints the go key on the title input"`: assert
   `#new-task-title-input` has `enterkeyhint="go"`. Fails: attribute absent.

## 7. Production edit instructions

1. `ResultPanel.svelte`: replace the hardcoded attributes with
   `role={isError ? "alert" : "status"}` and
   `aria-live={isError ? "assertive" : "polite"}`. No other changes.
2. `types.ts`: add `attention_items?: number;` to `RepoSummary` (keep the
   index signature).
3. `TaskList.svelte`:
   - derive a lookup once: `let attentionByRepo = $derived(new Map((cockpit.repos?.repos ?? []).map((repo) => [repo.name, repo.attention_items ?? 0])));`
   - in the pills `{#each}`, read `const count = attentionByRepo.get(project) ?? 0` (inline `{@const}`), render
     `{#if count}<span class="pill-badge" aria-hidden="true">{count}</span>{/if}`
     inside the button, add `aria-label={count ? `${project} — ${count} need attention` : project}` and
     `aria-current={selectedProject === project ? "true" : undefined}`.
   - style `.pill-badge` in the component `<style>` using existing tokens
     (mustard background like `.section-head.attention .section-head-count`:
     `background: var(--mustard); color: #1c1714; border-radius: 999px;
     min-width: 16px; height: 16px; font-size: 10px; font-weight: 700;
     display: inline-flex; align-items: center; justify-content: center;
     padding: 0 4px; margin-left: 6px;`).
   - empty state: `` {selectedProject ? `No tasks in ${selectedProject} yet — start one below.` : "All quiet — start a new task below."} ``
4. `App.svelte`:
   - add a `$derived` or `$effect` that computes the title from `route` and
     assigns `document.title`: task → `${route.handle} — Ajax`; settings →
     `Settings — Ajax`; project → `${route.project} — Ajax`; dashboard →
     `Ajax`.
   - nav Dashboard button: `aria-current={route.kind === "dashboard" || route.kind === "project" ? "page" : undefined}`.
5. `NewTaskSheet.svelte`: add a mount effect that focuses the dialog root
   (`let sheetEl = $state<HTMLDivElement | null>(null)` + `bind:this` +
   `$effect(() => { sheetEl?.focus(); })`); add `enterkeyhint="go"` to the
   title input.

## 8. Verification commands

```bash
npm run web:test -- crates/ajax-web/web/src/components/ResultPanel.test.ts crates/ajax-web/web/src/components/TaskList.test.ts crates/ajax-web/web/src/components/App.test.ts crates/ajax-web/web/src/components/NewTaskSheet.test.ts --run
npm run web:check
npm run web:test -- --run
```

## 9. Acceptance criteria

- Each new test fails before its production edit and passes after.
- All previously passing tests still pass (454 web tests before this packet).
- `npm run web:check` reports 0 errors.
- No file outside Allowed files changed (`git status --short`).
- Badge counts come only from `repos.repos[n].attention_items`.

## 10. Stop conditions

- An anchor above does not match the current source.
- A new test passes before the production edit.
- Any existing test fails for reasons unrelated to the packet edits.
- The change would require editing a forbidden file (including `styles.css`).
- Patch would exceed ~400 changed lines.
