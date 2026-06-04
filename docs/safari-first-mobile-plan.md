# Safari-First Mobile Plan

## Graphify

Graphify was initially blocked because this worktree did not have
`.planning/config.json`. The block was fixed by adding:

```json
{
  "graphify": {
    "enabled": true
  }
}
```

The graph was rebuilt on 2026-06-04 and refreshed after implementation:

- Source commit: `9dcd994` (current)
- Nodes: 4856
- Edges: 11616
- Hyperedges: 0
- Freshness: fresh

Artifacts were copied to `.planning/graphs/`.

## Decision

Ajax should stop treating iOS Home Screen PWA mode as the primary mobile
experience. The recommended iPhone path is normal iOS Safari browser mode.
Home Screen mode is experimental and not recommended for operations because it
can get stuck in iOS lifecycle or cache states where Safari works but the
installed app cannot reach the backend.

Notifications are out of scope for this pivot. Ajax should not implement Web
Push, PushManager flows, Notification API prompts, VAPID keys, push
subscriptions, service-worker push handlers, notification click handlers, or
PWA-specific notification UI.

## Current Repo Findings

- `crates/ajax-web/web/index.html` still contains PWA-oriented iOS metadata,
  manifest links, an alerts banner, and a Repair PWA settings section.
- `crates/ajax-web/web/app.js` still contains notification environment checks,
  `Notification.requestPermission`, `PushManager`, push subscription calls,
  service-worker registration, iOS standalone update logic, diagnostics, and
  PWA repair logic.
- `crates/ajax-web/web/sw.js` currently caches shell assets and implements push
  and notification click handling.
- `crates/ajax-web/src/adapters/push.rs` owns VAPID keys, subscriptions, push
  payloads, and delivery.
- `crates/ajax-web/src/runtime.rs` exposes `/api/push/*` routes and starts an
  attention poller that sends Web Push notifications.
- `crates/ajax-web/src/slices/attention.rs` currently exists only to support
  notification deduplication for push delivery.
- `crates/ajax-web/src/slices/install.rs`, `crates/ajax-web/src/runtime.rs`,
  and `crates/ajax-cli/src/web_backend.rs` include tests that assert the
  current PWA, push, and service-worker behavior.
- `README.md` and `architecture.md` still document Web Cockpit as a PWA path
  with install and notification support.

## Implementation Plan

### Task 1: Save This Strategy Plan

- Test to write: none; markdown-only.
- Code/documentation to implement: save this file at
  `docs/safari-first-mobile-plan.md`.
- Verification: read this file and search it for Safari-first policy terms.

### Task 2: Update Architecture and README Messaging

- Test to write: none; markdown-only.
- Code/documentation to implement: update `architecture.md` and `README.md` so
  Safari is the recommended iPhone path, Home Screen mode is experimental and
  not recommended, native app work is future-only, and notifications are out of
  scope.
- Verification: search the docs for stale Web Push, install-first, and
  notification-support claims.

### Task 3: Remove Client Notification Prompts and PWA Repair UX

- Failing behavior test to write: update `crates/ajax-web/src/slices/install.rs`
  asset tests so the browser shell contains no notification permission prompt,
  `PushManager`, push subscription calls, `/api/push/*` calls, or "Add Ajax to
  your Home Screen to enable alerts" copy.
- Code to implement: remove alert opt-in and Repair PWA controls from
  `index.html` and remove corresponding notification/PWA repair code from
  `app.js`. Keep standalone detection only for the Safari recommendation
  warning and diagnostics.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 4: De-Risk Service Worker Behavior

- Failing behavior test to write: update `install.rs` tests so `sw.js` contains
  no `push`, `notificationclick`, `showNotification`, or API caching logic.
- Code to implement: replace `sw.js` with a non-critical cleanup/static-safe
  worker or stop registering it from `app.js`. The browser must work without a
  service worker, and the worker must never intercept `/api/*`.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 5: Remove Backend Web Push Infrastructure

- Failing behavior test to write: update `crates/ajax-web/src/runtime.rs` and
  `crates/ajax-cli/src/web_backend.rs` tests so `/api/push/config`,
  `/api/push/subscribe`, and `/api/push/unsubscribe` are unsupported.
- Code to implement: remove push routes, push handlers, the push attention
  poller, `crates/ajax-web/src/adapters/push.rs`, and the `web-push`
  dependency.
- Verification: `cargo nextest run -p ajax-web runtime` and
  `cargo nextest run -p ajax-cli web_backend`.

### Task 6: Add Permanent Connection State and Recovery Controls

- Failing behavior test to write: add asset tests for explicit connection
  states: connected, checking, reconnecting, disconnected, backend unreachable,
  and stale session; and actions: Retry, Reload, Copy Diagnostics, Open Health
  URL.
- Code to implement: add a small connection state machine to `app.js`, status
  controls to `index.html`, and compact mobile styling to `app.css`.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 7: Make Safari Resume and Reload Checks Explicit

- Failing behavior test to write: assert `app.js` forces backend health and
  cockpit refresh on initial load, `visibilitychange`, `pageshow`, `focus`, and
  `online`.
- Code to implement: add explicit health checking before route refresh and
  track last successful connection plus last fetch error/status.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 8: Expand Mobile Diagnostics

- Failing behavior test to write: assert diagnostics include browser mode,
  backend URL, `navigator.onLine`, app version/build hash, server version,
  service-worker controller presence, `/api/health`, `/api/version`,
  `/api/cockpit`, last successful connection timestamp, last fetch
  error/status, and Copy Diagnostics.
- Code to implement: expand `runDiagnostics()` and add a copy action.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 9: Tighten iOS Safari Layout Basics

- Failing behavior test to write: assert viewport metadata is
  `width=device-width, initial-scale=1, viewport-fit=cover`, inputs are at
  least 16px, and bottom controls use safe-area padding.
- Code to implement: adjust viewport metadata and focused CSS layout rules.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 10: Improve Attention-First Dashboard Affordances

- Failing behavior test to write: assert the default list view exposes
  attention, running, review-ready, and failed summaries plus Open and Copy
  Summary actions.
- Code to implement: group existing server-authoritative task cards without
  adding browser persistence or a second client-side task model.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 11: Add Focused Terminal Secondary Shortcuts

- Failing behavior test to write: assert terminal details expose reload-safe
  Copy Visible Output, Copy Last Error, Show Diff, and approval shortcut
  affordances where current server data supports them.
- Code to implement: add shortcut buttons around existing pane/detail APIs; do
  not add raw terminal command typing or unsupported backend operations.
- Verification: `cargo nextest run -p ajax-web install`.

### Task 12: Final Validation

Run:

```sh
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

Report every failure with the command, concise failure explanation, and whether
it was fixed or remains unresolved.
