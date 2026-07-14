# TDD implementation packet: Playwright JWT leak scan

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 1. Status and task contract

READY. Tests-only. No production edits.

## 2. Goal

Fail Playwright if any compact JWT appears in localStorage, sessionStorage, URL/query, rendered HTML, console logs, API JSON string fields, or WebSocket messages while exploring Web Cockpit.

## 3. Allowed files

- `crates/ajax-web/web/e2e/jwtLeakScan.ts` (new)
- `crates/ajax-web/web/e2e/jwt-leak.test.ts` (new)

## 4. Forbidden changes

- `crates/ajax-web/web/e2e/fixtures.ts` and all other existing e2e files
- `package.json`
- `crates/ajax-web/src/**`, `crates/ajax-web/web/src/**`, other crates
- `architecture.md`, auth/cookie code
- commits, branches, new dependencies

## 5. Context evidence

- Graphify: `NOT_REQUIRED` — scanner only; CF Access JWT is server header validation; browser session is HttpOnly hex cookie, not JS-visible JWT.
- Serena: `NOT_REQUIRED` — file/line anchors below.
- ast-grep: `NOT_REQUIRED` — new TS e2e files only.

Repo anchors:

| Fact | Anchor |
| --- | --- |
| Playwright `desktop-chromium`, Vite `:5173` | `playwright.config.mts:7-19` |
| `mockFetch` replaces `fetch`, returns mocked JSON without calling prior wrapper | `fixtures.ts:84-98` |
| `mockTerminalWebSocket` replaces `WebSocket`, `__terminalSockets` | `fixtures.ts:122-195` |
| `terminalPanel` / `terminalToolbar` / `waitForTerminalSocket` | `fixtures.ts:202-220` |
| Dashboard / detail / settings / new-task flows | `smoke.test.ts:24-31`, `:46-49`, `:59-80`, `:136-140`, `:207-211` |

**Init-script order is not a contract.** `mockFetch` and `mockTerminalWebSocket` each replace globals (`fixtures.ts:85-86`, `:195`). The probe must remain correct whether it registers before or after those mocks.

## 6. Code anchors

Reuse: `mockFetch`, `mockTerminalWebSocket`, `waitForTerminalSocket`, `terminalPanel`, `terminalToolbar` from `./fixtures`.

### Probe robustness (required)

`installJwtLeakProbe(page)` must **not** depend on registration order:

- `addInitScript` installs a small re-armer (e.g. `setInterval`/microtask loop cleared on `pagehide`) that:
  - if `globalThis.fetch` is not already the probe wrapper (marker property), wrap the **current** fetch
  - if `globalThis.WebSocket` is not already the probe constructor, wrap the **current** WebSocket
- Wrapper records API `{ path, body }` via `response.clone().text()` (do not break consumers) into `window.__ajaxJwtApiBodies`
- WS wrapper records string/`ArrayBuffer` `send` + `message` into `window.__ajaxJwtWsMessages`
- Node side: `page.on('console')` accumulates messages for JWT scan

Suggested test call order (readability only, not a correctness dependency): mocks then probe, then `goto`.

### Per-route surface snapshots (required)

Because storage/URL/HTML can change per navigation, `collectJwtFindings` alone at the end is insufficient.

Export `snapshotBrowserSurfaces(page, label: string): Promise<JwtFinding[]>` that scans localStorage, sessionStorage, `location.href`, and `document.documentElement.outerHTML` **now**.

Clean test must call `snapshotBrowserSurfaces` after **each** explored route/step and concatenate with continuous console/API/WS captures before `assertNoJwts`.

Explore steps:

1. `goto("/app.html")` → wait `web/fix-login` → snapshot `"dashboard"`
2. `goto("/app.html#/t/web%2Ffix-login")` → terminal visible → toolbar Esc (`smoke.test.ts:70`) → snapshot `"task-detail"`
3. `goto("/app.html#/settings")` → `outlet-settings` → snapshot `"settings"`
4. `goto("/app.html")` → `.bottom-nav [data-bottom-action='new-task']` → `#new-task-title-input` → snapshot `"new-task"`

Then merge continuous captures (console/API/WS) + all snapshots → `assertNoJwts`.

## 7. Test-first instructions

`NOT_APPLICABLE: tests-only.`

Canary: `eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiJhamF4LWNhbmFyeSJ9.dGVzdC1zaWc`

1. **Detector self-check (all surfaces)** — after `installJwtLeakProbe` + `goto("/app.html")` (mocks optional), plant the same canary into each surface and assert `collectJwtFindings` / continuous captures include that surface:
   - `localStorage.setItem(...)`
   - `sessionStorage.setItem(...)`
   - `history.replaceState` / navigate so URL query or hash contains the canary (e.g. `?probe=<canary>` then snapshot)
   - inject a DOM text node containing the canary, then snapshot HTML
   - `console.log(canary)` then read probe console captures
   - `fetch` a data URL or call the wrapped fetch path that returns a JSON body containing the canary (smallest: `page.evaluate` push into `__ajaxJwtApiBodies` is **forbidden** — must go through wrapped `fetch`; use `page.route` only if needed; prefer temporarily overriding a path via in-page `fetch` to a same-origin URL that the final wrapper sees — simplest: `page.evaluate` `await fetch('/api/health')` after installing an extra init Response is hard without fixtures; instead have the self-check `addInitScript` **after** probe that makes one `/api/jwt-canary` return JSON `{ token: canary }`, then `fetch('/api/jwt-canary')` so the probe wrapper records it)
   - construct `new WebSocket('ws://localhost/canary')` (mocked or native wrap) and `send(canary)` / emit message with canary so WS capture records it
   Assert findings mention each of: `localStorage`, `sessionStorage`, `url`, `html`, `console`, `api`, `websocket` (exact surface labels may be those strings).
2. **Clean exploration** with per-route snapshots → zero findings.

## 8. Edit instructions

`PRODUCTION_EDIT: FORBIDDEN`.

Implement `jwtLeakScan.ts` + `jwt-leak.test.ts` per sections 6–7.

## 9. Verification commands

```bash
rtk npx playwright test e2e/jwt-leak.test.ts --config crates/ajax-web/web/playwright.config.mts --project=desktop-chromium
rtk git diff -- crates/ajax-web/web/e2e/fixtures.ts
rtk git diff --name-only
```

Expect: Playwright exit 0; empty fixtures diff; changed names only under Allowed files (plus pre-existing untracked `.planning/` is parent-owned and out of scope).

## 10. Acceptance criteria

- Canary detects localStorage JWT
- Clean exploration: zero findings across all snapshots + continuous captures
- `fixtures.ts` unchanged
- Only allowed new e2e files in the delegate delta

## 11. Stop conditions

- Any edit to `fixtures.ts` or production/auth
- Clean fail due to JWT-shaped fixture strings
- Diff outside Allowed files or >~400 lines
- Probe cannot observe mocked fetch/WS even with re-armer — STOP/report
- Playwright tooling/browsers unavailable — STOP with exact command + exit code
