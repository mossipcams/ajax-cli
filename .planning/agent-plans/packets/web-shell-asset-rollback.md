# TDD Implementation Packet: Web shell asset rollback

PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Goal

Roll back only #595's fragile query-string shell cache busting, runtime module
rewriting, and immutable caching. Serve the generated `app.js`, `app.css`, and
`terminal.js` at bare URLs with `no-store`, while retaining gzip and the
independent #608 hidden-mount/session, #609 ErrorBoundary, and #616 timeout
protections. Prove the task-opening journey through the release-built Rust HTTPS
server in mobile WebKit.

## Allowed files

- `crates/ajax-web/src/adapters/assets.rs`
- `crates/ajax-web/src/adapters/http.rs`
- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/install.rs`
- `crates/ajax-web/web/src/app/App.test.tsx`
- `crates/ajax-web/web/e2e/rust-server-assets.test.ts` (new)
- `crates/ajax-web/web/playwright.rust-server.config.mts` (new)
- `package.json`

## Forbidden changes

- Do not edit any file under a directory named `tests/`.
- Do not edit `api.ts`, `polling.ts`, `terminalConnection.ts`, `ErrorBoundary.tsx`,
  their tests, `App.tsx`, `useCockpitResource.ts`, `cockpitPoll.ts`, Cargo
  manifests/lockfile, architecture/docs, generated `web/dist`, or existing e2e
  fixtures/tests.
- Do not remove or change `CompressionLayer` or the `tower-http` dependency.
- Do not alter browser session/authentication, API contracts, task lifecycle,
  terminal semantics, polling cadences, TLS, or server startup behavior.
- Do not add a service worker, manifest/PWA mutation model, dependency, cache
  abstraction, ETag implementation, hashed filenames, or replacement URL
  rewriting scheme.
- Do not weaken/delete assertions, broadly format/refactor, commit, push, change
  branches, or touch unrelated July 20 work.

## Context evidence

- **Desired behavior:** PR #595 commit `f9c3a319` added query URL rewriting,
  served-JavaScript rewriting, immutable cache responses, and gzip. Only gzip
  remains wanted. PR #609 commit `18e271ff` records the broken WebKit module
  trace `/app.js?v=<version>`, `/terminal.js?v=<version>`, `/app.js` and fixed
  its duplicate React graph by making all runtime rewrites symmetrical; this
  packet removes the rewriting premise altogether.
- **Production anchors:** `assets.rs::browser_shell_html` rewrites shell URLs;
  `static_asset` calls `versioned_app_js`/`versioned_terminal_js`; and
  `version_chunk_refs` mutates raw Rollup chunks. `http.rs` exposes
  `static_bytes_axum_response`/`apply_immutable_cache`. `runtime.rs` asset
  handlers inspect `Uri`, `static_asset_response` branches on `version_busted`,
  and `version_busted` parses `?v=`. `runtime.rs::axum_app` independently layers
  `CompressionLayer::new()`.
- **Test anchors:** `assets.rs` tests currently require versioned sibling edges;
  `install.rs::shell_is_the_bundled_react_mount_point` requires versioned shell
  URLs; `runtime.rs::static_shell_assets_are_long_cached_and_gzipped` requires
  immutable versioned responses; and `shell_versions_static_asset_urls`
  requires rewritten HTML/JS. These are the intended RED assertions to invert.
- **Retained patterns:** `App.test.tsx::loads the cockpit on mount while hidden,
  but skips the background poll` proves #608. `api.test.ts` proves GET/session
  signals, `terminalConnection.test.ts` proves stale-session redial, and
  `ErrorBoundary.test.tsx` proves #609 diagnostics. `e2e/fixtures.ts::mockFetch`
  and `mockTerminalWebSocket` mock only runtime data and WebSocket behavior, so
  a new test can still load HTML/JS/CSS from a real Rust server.
- **Architecture boundary:** per `architecture.md`, Axum owns static transport
  and the browser renders server-authoritative projections. This change stays
  inside asset transport and test harnesses; it does not move task truth or
  runtime policy into the client.

## Code anchors

- `crates/ajax-web/src/adapters/assets.rs`: `browser_shell_html`, `static_asset`,
  `version_chunk_refs`, `versioned_app_js`, `versioned_terminal_js`, and the two
  versioned-edge tests.
- `crates/ajax-web/src/adapters/http.rs`: `static_bytes_axum_response` and
  `apply_immutable_cache`; retain `bytes_axum_response`, `apply_no_store`, and
  security headers.
- `crates/ajax-web/src/runtime.rs`: imports near line 30; `axum_app_css`,
  `axum_app_js`, `axum_terminal_js`; `static_asset_response` and
  `version_busted`; asset tests near `static_shell_assets_are_long_cached_and_gzipped`.
- `crates/ajax-web/src/slices/install.rs`:
  `shell_is_the_bundled_react_mount_point` URL assertions.
- `crates/ajax-web/web/src/app/App.test.tsx`: insert the polling recovery test
  immediately after the existing hidden-mount test; use its fake-timer and
  `jsonResponse` patterns.
- `crates/ajax-web/web/playwright.config.mts`: reuse its `fileURLToPath`, iPhone
  device, WebKit, reporter, trace, and `webServer` patterns in the new config.
- `crates/ajax-web/web/e2e/smoke.test.ts`: reuse its task-route expectations;
  import `mockFetch`, `mockTerminalWebSocket`, and `terminalSurface` from
  `./fixtures` in the new test.
- `package.json`: add one `web:smoke:rust` script adjacent to `web:smoke`.

## Test-first instructions

1. Before any production edit, invert/add the Rust tests:
   - In `assets.rs`, add `static_assets_are_raw_and_use_one_bare_module_graph`.
     Assert `/app.js` body equals `include_bytes!("../../web/dist/app.js")`,
     `/terminal.js` equals its raw generated bytes, neither served body contains
     `?v=`, and every present sibling edge is a bare `"./app.js"` or
     `"./terminal.js"` edge. Assert the terminal back-import is bare.
   - In `install.rs`, require exact `src="/app.js"` and `href="/app.css"`, and
     reject `src="/app.js?` / `href="/app.css?`.
   - Rename/update the runtime tests to
     `static_shell_assets_are_no_store_and_gzipped` and
     `shell_uses_bare_static_asset_urls`. For each asset, assert both its bare
     response and a legacy `?v=<app_version>` response have `cache-control:
     no-store` and do not contain `immutable`; keep the negotiated gzip
     assertion. Require bare shell URLs and raw bare module imports.
2. Add `App.test.tsx::timed-out cockpit GET releases polling for recovery`.
   The first `/api/cockpit` promise must reject only when the fresh request
   `init.signal` emits `abort`; the second cockpit request resolves with the
   fixture. With fake timers, render, advance through the 10,000 ms #616
   timeout and then the next visible cockpit interval, and assert two cockpit
   calls plus connected/ready UI. Do not change production polling code.
3. Add the real-server config and `rust-server-assets.test.ts` before production
   edits. The config must:
   - use only `mobile-webkit` / `devices["iPhone 15 Pro"]`;
   - set `baseURL` to `https://127.0.0.1:18789`, `ignoreHTTPSErrors: true`, and
     no server reuse;
   - start the real release Rust server with
     `cargo run --release -p ajax-cli -- --config target/web-smoke/config.toml --state target/web-smoke/ajax.db --worktree-root target/web-smoke/worktrees web --host 127.0.0.1 --port 18789`;
   - wait on `https://127.0.0.1:18789/api/health` with a build-capable timeout.
   The test must install `mockFetch` and `mockTerminalWebSocket`, observe actual
   network requests/responses, navigate to `/#/t/web%2Ffix-login`, await a
   rendered task outlet and terminal surface, and attach a JSON request/header
   trace via `testInfo.attach`. Assert exactly one request whose pathname is
   `/app.js`, exactly one whose pathname is `/terminal.js`, no query on either,
   bare `/app.css`, `cache-control: no-store` and no `immutable` on all three,
   and no console/page error containing React error `#321`, `NotFoundError`, or
   `Incompatible server response`.
4. Add the package script, then run this intended RED before production edits:

```bash
cargo nextest run -p ajax-web -E 'test(static_assets_are_raw_and_use_one_bare_module_graph) | test(static_shell_assets_are_no_store_and_gzipped) | test(shell_uses_bare_static_asset_urls)'
```

It must exit nonzero for current versioned/rewritten/immutable behavior. Also
run the new App test; it is a preservation regression and may already pass.
Run the real-server smoke and capture its versioned/request failure if the
release build fits the delegate window; otherwise report it as deferred to the
parent, but the Rust RED above is mandatory.

## Edit instructions

1. `assets.rs::browser_shell_html`: keep replacing only
   `__AJAX_APP_VERSION__` metadata and keep the existing boot-paint injection;
   remove `app.js`/`app.css` URL replacement. Keep `app_version` asset
   fingerprinting for `/api/version`.
2. `assets.rs::static_asset`: return raw `include_bytes!` for `app.js` and
   `terminal.js`. Delete `version_chunk_refs`, `versioned_app_js`,
   `versioned_terminal_js`, and their stale rewrite commentary/cache cells.
3. `http.rs`: delete `static_bytes_axum_response` and
   `apply_immutable_cache`. Keep normal `bytes_axum_response`, no-store, content
   type, and security headers unchanged.
4. `runtime.rs`: remove `Uri` only from the three static asset handlers (retain
   it where the fallback still needs it), make those handlers call a one-arg
   `static_asset_response`, and always serve a found asset through
   `bytes_axum_response`. Delete `version_busted` and the immutable-response
   import. Leave `CompressionLayer::new()` exactly active.
5. `install.rs`: change only shell serving contract comments/assertions to bare
   URLs.
6. Keep the new tests/config/script minimal. Do not edit retained production
   fixes just to make preservation coverage pass.

## Verification commands

```bash
cargo nextest run -p ajax-web -E 'test(static_assets_are_raw_and_use_one_bare_module_graph) | test(static_shell_assets_are_no_store_and_gzipped) | test(shell_uses_bare_static_asset_urls)'
npm run web:test -- --run src/app/App.test.tsx -t 'loads the cockpit on mount while hidden|timed-out cockpit GET releases polling for recovery'
npm run web:test -- --run src/shared/lib/api.test.ts src/shared/lib/terminalConnection.test.ts src/shared/ui/ErrorBoundary.test.tsx
npm run web:check
npm run web:lint
npm run web:smoke:rust
git diff --check
```

## Acceptance criteria

- Shell HTML exposes bare `/app.js` and `/app.css`; raw chunks use one bare
  `app.js`/`terminal.js` module identity with no runtime rewriting.
- Served JavaScript bytes exactly equal the embedded generated bundle bytes.
- Bare and legacy-query JS/CSS requests are `no-store`, never immutable.
- Negotiated gzip remains enabled.
- Real release-built Rust server + mobile WebKit requests `/app.js` once and
  `/terminal.js` once, both bare, and renders the task/terminal surface without
  React #321, `NotFoundError`, or a false incompatible-response message.
- Hidden mount, hung-GET polling recovery, stale terminal session recovery,
  ErrorBoundary diagnostics, and GET/session timeout tests pass without
  production changes to those protections.
- All verification commands run by the delegate exit 0, except a clearly
  reported environment-only real-WebKit limitation that the parent must rerun.

## Stop conditions

- Any required production/test edit falls outside Allowed files.
- The generated raw chunks do not already form one bare import graph; do not
  invent another rewrite scheme.
- Gzip requires changing/restoring immutable caching.
- A #608/#609/#616 preservation test fails and would require editing its
  retained production source.
- The real-server test requires auth/session/runtime changes rather than
  pre-page API/WebSocket mocks.
- The patch approaches 400 changed lines, adds a dependency, changes Cargo
  files, or grows into an unrelated refactor.
