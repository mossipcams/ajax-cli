# iOS Home Screen PWA Debug Plan

## Verdict

Partially fixable in our code.

The repo already has several correct safeguards, but there are concrete app-side
gaps that can make an installed iOS Home Screen PWA stay stale or fail to
recover cleanly after suspension. Those are worth fixing before pivoting away
from the PWA.

If the added in-app diagnostics later show that Safari can connect while the
standalone PWA cannot reach `/api/health` or `/api/cockpit` despite no stale
service worker/cache state, the remaining failure is likely iOS/WebKit, TLS
trust, certificate, or network behavior. At that point the product decision is:

- Keep Safari web as the recommended iOS path if Home Screen reliability remains
  poor.
- Consider Capacitor only if native lifecycle/networking control is worth the
  maintenance cost.

## Repo Evidence

Service worker:

- `crates/ajax-web/web/sw.js` precaches the app shell with cache
  `ajax-cockpit-v23`.
- The current `SHELL` list includes `/`, `/app.css`, `/app.js`,
  `/manifest.webmanifest`, `/sw.js`, and icons.
- The fetch handler returns without interception for `url.pathname.startsWith("/api/")`,
  so `/api/*` routes are not intentionally cached.
- The worker uses network-first behavior for shell files and cache fallback on
  network failure.
- The worker calls `self.skipWaiting()` on install and `self.clients.claim()` on
  activate.

Server/runtime:

- `crates/ajax-web/src/runtime.rs` serves `/api/health`, `/api/version`,
  `/api/cockpit`, `/api/tasks`, `/api/tasks/{handle}`, pane, answer, operations,
  restart, and push routes.
- `crates/ajax-web/src/adapters/http.rs` applies `Cache-Control: no-store` to
  Axum responses through `bytes_axum_response`.
- `crates/ajax-web/src/adapters/assets.rs` computes `app_version()` from the
  crate version plus HTML/JS/CSS/SW bytes.
- `/api/version` reports the server build hash, but the currently loaded shell
  does not expose its own embedded build hash to the browser diagnostics.

Client lifecycle:

- `crates/ajax-web/web/app.js` checks `/api/version` on initial load, on interval,
  and on `visibilitychange` foreground.
- It refreshes data on `online` and `visibilitychange`.
- It does not currently wire `pageshow` or `focus`.
- It registers the service worker, listens for `controllerchange`, and reloads
  once after an update when there was already a controller.
- It does not currently force `registration.update()` or set
  `updateViaCache: "none"`.
- It distinguishes iOS standalone mode through `isStandalonePwa()`, but this is
  used only for notification guidance today.

Diagnostics/repair:

- Settings currently exposes server restart controls.
- There is no in-app diagnostic report showing standalone mode, online state,
  service-worker controller, loaded app version, `/api/health`, `/api/version`,
  `/api/cockpit`, current task API status, fetch status codes, and fetch errors.
- There is no "Repair PWA" action to unregister service workers, clear only Ajax
  Cache Storage entries, clear safe transient UI state, and reload with a
  cache-busting query parameter.

## Implementation Status

Implemented in this worktree:

- `crates/ajax-web/web/sw.js` now uses service worker cache
  `ajax-cockpit-v24` and no longer precaches `/sw.js`.
- `crates/ajax-web/web/app.js` now refreshes on `visibilitychange`,
  `pageshow`, `focus`, `online`, and initial launch, registers the service
  worker with `updateViaCache: "none"`, and calls `registration.update()`.
- `crates/ajax-web/web/index.html` now carries an `ajax-app-version` meta tag
  stamped from the server-side app version.
- `crates/ajax-web/src/adapters/assets.rs`, `crates/ajax-web/src/runtime.rs`,
  and `crates/ajax-cli/src/web_backend.rs` now serve the rendered versioned
  shell.
- Settings now includes Run diagnostics and Repair PWA controls.
- Diagnostics report standalone mode, online state, service-worker controller
  state, loaded app version, current location, `/api/health`, `/api/version`,
  `/api/cockpit`, and current task-detail API when a detail handle is active.
- Repair PWA unregisters service workers, deletes only `ajax-cockpit-*` Cache
  Storage entries, resets transient in-memory UI state, and reloads with a
  cache-busting query parameter.

Validation run after implementation:

- `cargo fmt --check` passed.
- `cargo check --all-targets --all-features` passed.
- `cargo clippy --all-targets --all-features -- -D warnings` passed.
- `cargo nextest run --all-features` passed with 1373 tests in the clean PR
  worktree.
- `cargo audit` exited successfully and reported two allowed warnings already in
  the dependency tree: unmaintained `ansi_term` through `rust_arkitect`, and
  yanked `aes` through `web-push`.

## Implementation Plan

### Task 1: Remove service worker self-caching

Failing behavior test:

- Use the existing assertions in `crates/ajax-cli/src/web_backend.rs` and
  `crates/ajax-web/src/slices/install.rs`.
- Run:
  `cargo nextest run -p ajax-cli service_worker_and_app_handle_push_notifications`
- Run:
  `cargo nextest run -p ajax-web pwa_shell_is_local_only_and_service_worker_caches_only_static_shell`
- Expected failure: current `sw.js` still contains `"/sw.js"` in `SHELL` and
  still uses `ajax-cockpit-v23`.

Code to implement:

- In `crates/ajax-web/web/sw.js`, bump `CACHE` from `ajax-cockpit-v23` to
  `ajax-cockpit-v24`.
- Remove `"/sw.js"` from `SHELL`.

Verification:

- Re-run both focused tests and show they pass.

### Task 2: Strengthen iOS standalone resume/update checks

Failing behavior test:

- Add/update asset assertions that `app.js` wires:
  - `pageshow`
  - `focus`
  - `registration.update()`
  - `updateViaCache: "none"`
  - a shared foreground/resume refresh path

Code to implement:

- Add a small shared function that runs forced update/version checks and live
  reload after iOS resumes the app.
- Call it from `visibilitychange`, `pageshow`, `focus`, `online`, and initial
  launch.
- Register the service worker with `{ updateViaCache: "none" }`.
- After registration, call `registration.update()` when available.

Verification:

- Run the focused app-script asset test.

### Task 3: Embed loaded shell version

Failing behavior test:

- Assert served `/` contains the actual Ajax app version/build hash.
- Assert `app.js` reads that loaded shell version from the DOM or a bootstrap
  value.

Code to implement:

- Add a placeholder in `index.html`, for example a `<meta>` value.
- Render the shell through an install-slice function that replaces the
  placeholder with `install::app_version()`.
- Use the rendered shell in Axum and test routing.
- Keep static asset embedding small; no new frontend framework.

Verification:

- Run focused install/runtime shell tests.

### Task 4: Add minimal in-PWA diagnostics

Failing behavior test:

- Assert Settings exposes diagnostics controls.
- Assert `app.js` probes and renders:
  - standalone mode true/false
  - `navigator.onLine`
  - service-worker controller present
  - loaded app version/build hash
  - `/api/health` result
  - `/api/version` result
  - `/api/cockpit` result
  - current `/api/tasks/{handle}` result when a task detail route is active
  - fetch error message and status code

Code to implement:

- Add a compact Settings diagnostics section.
- Add a Run diagnostics button.
- Add a simple diagnostic function that fetches with `cache: "no-store"` and
  records `{ ok, status, error, body/version summary }`.
- Keep output text/json compact and readable inside the PWA.

Verification:

- Run focused install/web backend asset tests.
- Manual follow-up: open installed PWA, run diagnostics, and compare Safari vs
  Home Screen output.

### Task 5: Add minimal Repair PWA action

Failing behavior test:

- Assert Settings exposes Repair PWA.
- Assert `app.js`:
  - unregisters service workers
  - deletes only Cache Storage entries whose names start with `ajax-cockpit-`
  - clears only safe transient in-memory/UI state
  - reloads with a cache-busting query parameter
  - does not clear auth/session/local storage

Code to implement:

- Add a Repair PWA button under Settings.
- Implement minimal repair:
  - `navigator.serviceWorker.getRegistrations()`
  - `registration.unregister()`
  - `caches.keys()` and delete only `ajax-cockpit-*`
  - reset transient JS UI state
  - `window.location.replace("/?repair=<timestamp>")`

Verification:

- Run focused asset tests.
- Manual follow-up: install PWA, run repair, confirm it reloads and
  re-registers a fresh service worker without wiping auth/session data.

## Final Validation

After all tasks:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

If available and relevant:

```sh
cargo audit
```

Do not report any command as passing unless it was actually run and passed.

## Decision Criteria

Stay with the PWA if diagnostics show:

- standalone mode is true
- service-worker controller is present and fresh
- loaded shell version matches `/api/version`
- `/api/health` succeeds
- `/api/cockpit` succeeds
- repair clears stale SW/cache state and restores connectivity

Pivot to Safari web only if diagnostics show:

- Safari reliably reaches health/cockpit
- standalone PWA intermittently cannot reach the same API endpoints
- service-worker/cache state is fresh or absent
- repair does not restore connectivity

Consider Capacitor only if diagnostics show:

- the backend and TLS setup are healthy
- Safari web is not operationally acceptable
- Home Screen PWA lifecycle/networking remains unreliable after the repair path
- native lifecycle/networking control is worth owning a native wrapper
