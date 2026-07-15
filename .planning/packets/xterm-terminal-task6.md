# Xterm terminal rebuild — Task 6 packet

## 1. Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## 2. Goal

Prevent xterm initialization in a DOM environment that lacks required browser
media APIs, so existing TaskDetail/App unit tests render normally while real
mobile-WebKit terminal behavior remains unchanged.

## 3. Allowed files

- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## 4. Forbidden changes

- Do not edit tests, test setup, dependencies, docs, other components,
  connection/backend code, or generated assets.
- Do not add a test-only environment flag, user-agent check, mock, fallback
  renderer, helper module, or broad error swallowing.
- Do not alter terminal behavior in browsers that provide required APIs.
- Do not commit or touch another path.

## 5. Context evidence

- Graphify: `NOT_REQUIRED`; this is a local renderer capability guard with no
  ownership change.
- Serena: `NOT_REQUIRED`; exact failing component and initialization anchor are
  known.
- ast-grep: `NOT_REQUIRED`; Svelte onMount markup is the exact source anchor and
  no structural cross-file change is involved.
- RED evidence: `npm run web:test -- --run` exits 1 with 16 TaskDetail failures
  and unhandled xterm errors at `TaskTerminal.svelte` `liveTerm.open(hostEl)`;
  jsdom reports missing `window.matchMedia` and canvas context.

## 6. Code anchors

- In `TaskTerminal.svelte` `onMount`, the existing early return checks
  `!hostEl || !interactionEl` immediately before persisted font load and
  `new Terminal(...)`.

## 7. Test-first instructions

The parent already ran and recorded this exact RED command. Rerun it before
editing to prove the same failure:

```bash
rtk npm run web:test -- --run
```

Expected RED: 16 TaskDetail failures at xterm open because jsdom lacks
`matchMedia`/canvas. Do not modify tests.

## 8. Edit instructions

Extend the existing pre-initialization guard by the minimum required browser
capability check so xterm is never constructed/opened when `window.matchMedia`
is absent. Keep the panel markup mounted; simply skip renderer/socket setup in
that unsupported environment. Do not catch unrelated runtime failures.

## 9. Verification commands

```bash
rtk npm run web:test -- --run
rtk npx playwright test crates/ajax-web/web/e2e/terminal-behavior.test.ts --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit
rtk npm run web:check
rtk npm run web:build:check
```

## 10. Acceptance criteria

- Full Vitest passes 245/245 with no unhandled xterm errors.
- All 27 mobile-WebKit cases remain green.
- Web checks and build check pass.
- Only `TaskTerminal.svelte` changes in this round; tests remain untouched.

## 11. Stop conditions

- Passing requires a test/test-setup edit or dependency.
- A real supported browser skips terminal initialization.
- Mobile-WebKit behavior regresses or another file is required.
