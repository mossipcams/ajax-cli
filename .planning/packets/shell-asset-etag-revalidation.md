# Packet: shell asset ETag revalidation

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Serve `ETag: W/"<app_version()>"` and `Cache-Control: no-cache` on the three
static shell asset routes (`/app.js`, `/app.css`, `/terminal.js`) and return
`304 Not Modified` with an empty body when the request's `If-None-Match` matches.
Every other route (HTML shell, all `/api/*`) keeps `Cache-Control: no-store` and
gains no `ETag`.

## Allowed files

- `crates/ajax-web/src/adapters/http.rs`
- `crates/ajax-web/src/runtime.rs`

## Forbidden changes

- Do not add `immutable`, `max-age`, or any `?v=` URL cache-busting. Both were
  landed and reverted in #620; `immutable` suppresses revalidation even on an
  explicit reload and strands an installed PWA on a stale bundle.
- Do not change `apply_no_store` or `apply_security_headers` behavior for any
  non-static-asset response. The shell and `/api/*` must stay `no-store` with no
  `ETag`.
- Do not add a service worker, manifest, icons, or any offline cache.
- Do not touch `crates/ajax-web/web/**`, `install.rs`, `assets.rs`, or any
  Vite/JS build config.
- Do not reorder or remove `CompressionLayer` or the session middleware.
- Do not use a strong ETag.
- No formatting sweeps, renames, or drive-by cleanup.

## Context evidence

| Category | Finding | Anchor |
|---|---|---|
| Desired behavior | All responses are built by `bytes_axum_response`, which unconditionally stamps `no-store`, so a shell asset is refetched in full on every launch. | `crates/ajax-web/src/adapters/http.rs:98-111`, `:123-127` |
| Source anchor | The three asset handlers take no arguments and delegate to one helper. | `crates/ajax-web/src/runtime.rs:609-619` |
| Source anchor | The shared helper looks the asset up and returns 200 or a 404 text body. | `crates/ajax-web/src/runtime.rs:1202-1208` |
| Source anchor | Routes are registered on the same router that `CompressionLayer` wraps. | `crates/ajax-web/src/runtime.rs:387-389`, `:411` |
| Architecture boundary | `CompressionLayer` gzips the body *after* the handler returns, so identity and gzip bodies share one handler-set ETag. This is exactly why the ETag must be **weak** (`W/`) — a strong ETag asserts byte-equality per representation. | `crates/ajax-web/src/runtime.rs:411`; existing gzip assertion at `:1900-1913` |
| Reuse pattern | `install::app_version()` is already an FNV-1a fingerprint over `index.html` + `app.js` + `app.css` + `terminal.js`, stable within a build and changed by any bundle edit. Use it verbatim as the ETag value; do not compute a new hash. | `crates/ajax-web/src/adapters/assets.rs:31-67` |
| Reuse pattern | `HeaderMap` and `header` are already imported in `runtime.rs`; no new import needed for the handler signatures. | `crates/ajax-web/src/runtime.rs:22` |
| Test anchor | `static_shell_assets_are_no_store_and_gzipped` asserts `no-store` on all three asset paths and must be updated; its shell/`/api/cockpit` `no-store` assertions and its gzip assertion stay as-is. | `crates/ajax-web/src/runtime.rs:1869-1913` |
| Test pattern | `app_with(context, TestBridge::default(), "<tag>")` builds the router; `get_public(&app, path)` GETs without a session cookie; a custom-header request is built with `AxumRequest::builder().uri(..).header(..).body(Body::empty())` + `.oneshot()`; body read via `to_bytes(resp.into_body(), usize::MAX)`. | `crates/ajax-web/src/runtime.rs:1641-1668`, `:1900-1913`, `:1860-1866` |

## Code anchors

- `crates/ajax-web/src/adapters/http.rs:123` — `pub fn apply_no_store`; add the
  new revalidation helper next to it.
- `crates/ajax-web/src/adapters/http.rs:98` — `bytes_axum_response`; the new
  helper must reuse `apply_security_headers` so static assets keep CSP, nosniff,
  referrer-policy, and permissions-policy. Do not duplicate those headers.
- `crates/ajax-web/src/runtime.rs:609-619` — `axum_app_css`, `axum_app_js`,
  `axum_terminal_js`.
- `crates/ajax-web/src/runtime.rs:1202` — `fn static_asset_response`.
- `crates/ajax-web/src/runtime.rs:1869` — existing cache test to update.

## Test-first instructions

Add one new inline test in `crates/ajax-web/src/runtime.rs`'s existing
`mod tests`, immediately after `static_shell_assets_are_no_store_and_gzipped`:

```rust
#[tokio::test]
async fn static_shell_assets_revalidate_with_a_weak_etag() { ... }
```

It must assert, for each of `/app.js`, `/app.css`, `/terminal.js`:

1. A plain `get_public` GET returns `200`, with
   `etag == format!("W/\"{}\"", crate::slices::install::app_version())` and
   `cache-control == "no-cache"`.
2. A GET carrying `if-none-match` set to that exact etag returns
   `StatusCode::NOT_MODIFIED`, an empty body, and the same `etag` and
   `cache-control` headers.
3. A GET carrying `if-none-match: W/"stale"` returns `200` with a non-empty body.

Also assert in the same test that the HTML shell (`get_public(&app, "/")`) has
**no** `etag` header and still reports `cache-control: no-store`.

Use tag `"axum-static-etag"` for `app_with`.

Red command (must fail before the edit, on the missing/incorrect `etag` header
assertion):

```bash
cargo test -p ajax-web static_shell_assets_revalidate_with_a_weak_etag
```

## Edit instructions

1. In `crates/ajax-web/src/adapters/http.rs`, add:

   ```rust
   /// Static shell assets revalidate instead of refetching.
   ///
   /// `no-cache` means "store it, but check with me every time" — unlike
   /// `no-store`, which forbids storing and so makes revalidation impossible.
   /// The ETag is **weak** because `CompressionLayer` gzips the body after this
   /// runs, so one handler-set validator covers both representations.
   pub fn static_asset_revalidated_response(
       content_type: &'static str,
       body: &'static [u8],
       if_none_match: Option<&str>,
   ) -> AxumResponse { ... }
   ```

   - Build `etag = format!("W/\"{}\"", crate::adapters::assets::app_version())`.
     Use the **adapter**, not `crate::slices::install::app_version()` — `install`
     only re-exports it (`install.rs:11-13`), and an adapter must not depend on a
     slice. `adapters/mod.rs:3` already declares `pub mod assets`.
   - When `if_none_match` equals `etag`, respond `304` with an empty body;
     otherwise `200` with `body.to_vec()` and the `Content-Type`.
   - Both branches set `ETag`, `Cache-Control: no-cache`, and call
     `apply_security_headers`. Neither calls `apply_no_store`.
   - `HeaderValue::from_str(&etag)` is fallible; on error fall back to
     `bytes_axum_response(200, content_type, body.to_vec())` rather than
     panicking (`app_version()` is ASCII, so this is unreachable in practice).

2. In `crates/ajax-web/src/runtime.rs`, change the three handlers to accept
   `headers: HeaderMap` and forward the `if-none-match` value:

   ```rust
   async fn axum_app_css(headers: HeaderMap) -> AxumResponse {
       static_asset_response("/app.css", &headers)
   }
   ```

   Do the same for `axum_app_js` (`/app.js`) and `axum_terminal_js`
   (`/terminal.js`). Route registrations at `:387-389` need no change — axum
   extracts `HeaderMap` automatically.

3. Change `static_asset_response` at `runtime.rs:1202` to take
   `headers: &HeaderMap`, read
   `headers.get(header::IF_NONE_MATCH).and_then(|v| v.to_str().ok())`, and call
   `static_asset_revalidated_response` for the `Some(asset)` arm. The `None` arm
   keeps `text_axum_response(404, "not found")` unchanged (404s stay
   `no-store`). Import `static_asset_revalidated_response` in the existing
   `use crate::adapters::http::{...}` block at `runtime.rs:44`.

4. Update `static_shell_assets_are_no_store_and_gzipped` at `runtime.rs:1869`:
   the three asset paths now assert `cache-control == "no-cache"` (keep the
   `!cache_control.contains("immutable")` assertion). The shell and
   `/api/cockpit` `no-store` assertions and the gzip assertion are unchanged.
   Rename the test to `static_shell_assets_revalidate_and_are_gzipped`.

## Verification commands

```bash
cargo test -p ajax-web static_shell_assets_revalidate_with_a_weak_etag
cargo nextest run -p ajax-web
cargo clippy -p ajax-web --all-targets --all-features -- -D warnings
cargo fmt --check
```

## Acceptance criteria

- The new test passes and demonstrably failed before the production edit.
- `/app.js`, `/app.css`, `/terminal.js` return `no-cache` + a weak `ETag`, and
  `304` on a matching `If-None-Match`.
- The HTML shell and every `/api/*` route still return `no-store` and carry no
  `ETag`.
- No response anywhere contains `immutable` or `max-age`.
- Gzip negotiation still works on `/app.js`.
- The whole `ajax-web` suite is green.

## Stop conditions

- Any edit needed outside the two allowed files.
- `CompressionLayer` strips, rewrites, or duplicates the `ETag` header, making
  the assertions unsatisfiable.
- The 304 path requires changing the session middleware or CSP layer.
- The patch would exceed roughly 120 changed lines.
- Any pre-existing `ajax-web` test fails for a reason unrelated to caching.
