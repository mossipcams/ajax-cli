# TDD Implementation Packet — Collapse duplicated web-shell guard tests

```yaml
PACKET_STATUS: READY
TASK_KIND: test-refactor
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
```

## Task contract

`crates/ajax-cli` re-asserts the *content* of the web shell and the JS bundle that
`crates/ajax-web` already owns. Editing `crates/ajax-web/web/*` therefore trips
string assertions in **two** crates. Make `ajax-web` the single owner of shell/bundle
**content** assertions; leave `ajax-cli` owning only **serving/wiring** assertions
(status code, content type, and that the router serves exactly the bytes ajax-web
renders).

This is a test-only change. **No production behavior may change.**

Why it is safe: `render_mobile_shell()` (`web_backend.rs:41`) is itself `#[cfg(test)]`
and is a one-line passthrough to `web_install::browser_shell()`. Every content
assertion in ajax-cli is therefore asserting ajax-web's string through a test-only
wrapper.

## Allowed files

- `crates/ajax-cli/src/web_backend.rs`
- `crates/ajax-web/src/slices/install.rs`

## Forbidden changes

- Any file other than the two above.
- Any non-`#[cfg(test)]` code path. Do not change `handle_http_request`, routing,
  asset serving, or `browser_shell()`.
- Do not weaken or delete any assertion whose subject is not proven duplicated below.
- No formatting sweeps, renames, import reordering, or drive-by cleanup.

## Code anchors

### A. Delete these four ajax-cli tests (content assertions owned by ajax-web)

In `crates/ajax-cli/src/web_backend.rs`, inside `mod tests`:

1. `mobile_shell_is_responsive_and_loads_cockpit_data` (~line 417)
   → superseded by ajax-web `install.rs::shell_is_the_bundled_svelte_mount_point`
     (asserts `<!doctype html>`, viewport, width=device-width, `/app.css`, `/app.js`).
2. `mobile_shell_is_the_bundled_svelte_mount_point` (~line 428)
   → superseded by ajax-web `shell_is_the_bundled_svelte_mount_point`
     + `shell_no_longer_carries_the_legacy_imperative_dom` (its legacy list is a
       **9-entry superset** of ajax-cli's 7 — it additionally covers
       `id="connection-status"` and `id="task-detail"`)
     + `shell_advertises_safe_pwa_browser_metadata_without_install_surface`
       (theme-color, color-scheme, mobile-web-app-capable, apple-* metadata)
     + `retired_pwa_install_assets_are_absent` (manifest.webmanifest).
3. `app_script_wires_cockpit_actions` (~line 527)
   → superseded by ajax-web `bundle_targets_the_same_origin_api_and_never_registers_a_worker`
     (asserts `/api/cockpit`, `/api/operations`, `/api/server/restart`, `#/settings`,
     `request_id`, `no-store`).
4. `app_script_is_worker_and_push_free` (~line 544)
   → superseded by the same ajax-web test, **after step B** (see below).

### B. Tighten ONE ajax-web assertion so no coverage is lost

`crates/ajax-web/src/slices/install.rs:177` currently asserts:

```rust
assert!(!script.contains("serviceWorker.register"));
```

ajax-cli's deleted assertion was **stricter** (`!app_text.contains("serviceWorker")` —
no mention at all). Preserve the stricter form. Change that single line to:

```rust
assert!(!script.contains("serviceWorker"));
```

Leave every other line of `install.rs` untouched. (ajax-web's existing
`!script.contains("/api/push")` is already broader than ajax-cli's
`/api/push/config` + `/api/push/subscribe`, so nothing else is lost.)

### C. Delete the now-dead test-only helper

After step A, `render_mobile_shell()` (`web_backend.rs:40-43`, `#[cfg(test)]`) has
zero callers (verified: its only uses were in the two deleted tests). Delete it, and
remove `render_mobile_shell` from the `use super::{...}` list at ~line 399. Keep the
`web_install` import — step D needs it. If the compiler reports any import as newly
unused, remove exactly that import and nothing else.

### D. Add the wiring assertions ajax-cli MUST own (MANDATORY — this is what makes step A safe)

**Step D is not optional and not a bonus. It is the precondition that makes deleting the
four tests in step A safe.** After steps A+C, `install.rs` greps the bytes of
`static_asset("/app.js")`, while ajax-cli's surviving router tests only assert 200 +
content-type + non-empty. Without step D, *nothing anywhere proves the `/app.js` route
actually serves the bytes `install.rs` is grepping* — the two crates would test two
unconnected things. Step D's byte-equality is the sole remaining seam between the layers.

**Import — read carefully, this is where you will otherwise hit E0433.** `web_install` is
imported at `web_backend.rs:10`, which is the **parent** module. `mod tests` (line ~398)
does NOT inherit it: it has an explicit `use super::{cockpit_json, handle_http_request,
handle_http_request_with_runner_and_paths, render_mobile_shell};` list. So inside
`mod tests` you must either reference it as `super::web_install::…` or add `web_install`
to that `use super::{...}` list. Do one of those. Do NOT "fix" a path error by deleting
step D.

In `http_router_serves_mobile_shell_and_cockpit_json` (~line 471), keep the existing
status-code and content-type assertions and add:

```rust
// ajax-web owns the shell's content; ajax-cli only proves it serves those bytes.
assert_eq!(String::from_utf8_lossy(&shell.body), super::web_install::browser_shell());
```

Also **delete** this one line from that same test:

```rust
assert!(String::from_utf8_lossy(&shell.body).contains("Ajax Cockpit"));
```

It is a raw shell-*content* assertion (ajax-web's job) and is strictly subsumed by the
byte-equality above. Leaving it in means editing the shell `<title>` still reddens two
crates, which defeats the whole point of this packet. Keep every other assertion in that
test (status code, content type, and the whole `/api/cockpit` JSON block).

In `http_router_serves_static_css_js_and_ghostty_wasm` (~line 489), keep the existing
status/content-type/non-empty assertions and add byte-equality for the two bundled assets.
**Preserve this exact operand order** — `Vec<u8>` on the left, `&'static [u8]` on the right.
It relies on `impl PartialEq<&[U]> for Vec<T>`; reversing the operands will not compile:

```rust
assert_eq!(js.body, super::web_install::static_asset("/app.js").unwrap().body);
assert_eq!(css.body, super::web_install::static_asset("/app.css").unwrap().body);
```

## Explicitly keep (do NOT delete)

- `http_router_serves_mobile_shell_and_cockpit_json` — routing, ajax-cli's job. Keep the
  test; per step D, remove only its single `contains("Ajax Cockpit")` line.
- `http_router_serves_static_css_js_and_ghostty_wasm` — routing, ajax-cli's job.
- `http_router_does_not_serve_retired_pwa_install_assets` — asserts HTTP **404**
  behavior, which is routing, not content. ajax-web's version asserts
  `static_asset(..) == None`, a different layer. Both must survive.
- Every other test in either file.

## Verification commands

Run all of them; report exact exit codes.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-web -p ajax-cli --all-features --test-threads=1
```

Expected: green. The ajax-cli test count drops by exactly 4; ajax-web's count is
unchanged. No production code is in the diff.

**Test count alone is NOT sufficient proof** — the invalid outcome in "Hard invalidation
rules" also drops exactly 4 tests and is also green. So additionally prove step D landed:

```bash
git diff -- crates/ajax-cli/src/web_backend.rs | grep -c 'web_install::'
```

This must be **≥ 3** (one `browser_shell()` + two `static_asset(...)` assertions added).
If it is 0, the change is invalid regardless of a green suite.

## Stop conditions

- Any verification command fails and the fix would require editing a Forbidden file.
- Deleting a test whose assertion subject is NOT in the superseded list above.
- `clippy -D warnings` reports dead code that would require a production-code edit.
- The diff exceeds ~120 changed lines (this task should be well under that).
- Any production (non-`cfg(test)`) line appears in `git diff`.

### Hard invalidation rules (read before you "fix" anything)

1. **If step D's assertions are not present in the final diff, the change is INVALID.**
   Deleting the four tests without step D is silent coverage loss, even though the suite
   will be green and clippy clean. Green is not the bar here.
2. **Never resolve a `web_install` unused-import warning by deleting the import.** After
   steps A+C, `web_install` (line 10) has no users *until you write step D*. If clippy
   flags it as unused, that is a signal you have not finished step D — it is not a
   licence to delete the import. Deleting it is an automatic FAILED result.
3. If step D will not compile, STOP and report the exact compiler error. Do not drop
   step D to reach green.

## Notes for the parent (not the delegate)

Post-merge coverage proof, run by the parent at the review gate: mutate a node id in
`crates/ajax-web/web/app.html` (or the built shell), re-run both crates' tests, and
confirm **exactly one** test now fails (in ajax-web), not two. Revert the mutation.
