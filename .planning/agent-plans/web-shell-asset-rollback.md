# Surgical rollback of fragile Web shell asset cache busting

## Status

Approved by the user on 2026-07-20 with advance authorization to delegate and
continue through all tasks until finished. Cursor produced the bounded
implementation and the parent independently passed every packet verification
command. The user explicitly overrode the report-format-only router stop on
2026-07-20 and directed execution to continue. Bundle rebuild and broader
validation passed. The hook-backed commit completed successfully; the plan
ledger update is being amended before push, PR creation, and CI.

## Worktree and base

- Clean isolated worktree:
  `/Users/matt/Desktop/Projects/ajax-cli__worktrees/web-shell-asset-rollback`
- Branch: `fix/web-shell-asset-rollback`
- Base: `origin/main` at `18aa87387da70da3d6b750df4960e5332a26a4a8`
- The dirty `ajax/connections` worktree and its prior connection-hardening work
  remain untouched and will not enter this PR.

## Inspected July 20 changes

- **#595 / `f9c3a319`** added `tower-http` gzip, query-string rewriting in
  `browser_shell_html`, served-JavaScript rewriting in `assets.rs`, immutable
  cache responses in `http.rs`, URI-query branching in `runtime.rs`, and tests
  that required versioned asset URLs.
- **#608 / `e875003d` + `c7ae6993`** moved the hidden-document guard from the
  shared Cockpit loader to the background interval, added stale terminal
  WebSocket session renewal, and limited #595 immutability to non-empty `?v=`
  requests. The PWA/session fixes are independent of asset serving.
- **#609 / `18e271ff`** made both served chunks rewrite sibling imports to the
  same versioned identity, fixing #595's duplicate React module graph. It also
  added accurate ErrorBoundary messages and component-stack logging. The
  diagnostic source/tests are independent and stay unchanged.
- **#616 / `fe2791ac`** added fresh `AbortSignal.timeout(10_000)` signals to GET
  and session-renewal requests. The transport source/tests are independent and
  stay unchanged.

Historical #595 failure trace recorded in #609:

```text
/app.js?v=<version>
/terminal.js?v=<version>
/app.js
```

Current pre-rollback behavior is consistently versioned after #609, but still
depends on runtime JavaScript rewriting and marks query-versioned,
non-content-addressed assets immutable.

## Scope

- Restore one bare module identity for `/app.js` and `/terminal.js`, and the
  bare `/app.css` stylesheet URL.
- Serve `dist/app.js` and `dist/terminal.js` byte-for-byte without runtime
  rewriting.
- Serve shell assets through the existing `no-store` response path for both
  bare URLs and legacy query-string requests.
- Keep `CompressionLayer` and the existing `tower-http` gzip dependency.
- Add a mobile-WebKit regression that loads the shell and task route from the
  real release-built Rust HTTPS server while mocking only API/WebSocket data.
- Add one polling recovery test that proves a timed-out GET releases the
  in-flight guard and a later interval succeeds.
- Rebuild the tracked embedded web bundle and retain only deterministic output.

## Non-goals

- No blind revert and no Cargo dependency/lockfile rollback.
- No changes to `api.ts`, `terminalConnection.ts`, `ErrorBoundary.tsx`, their
  retained #608/#609/#616 behavior, task lifecycle, registry truth, sessions,
  auth policy, TLS, terminal backend, service workers, or manifests.
- No ETag/cache framework, new cache abstraction, new retry abstraction, or
  content-hashed filename migration.
- No unrelated July 20 cleanup and no changes from the dirty
  `ajax/connections` worktree.
- No files under a `tests/` directory and no weakened assertions.

## Delegation decision

Delegation decision: delegated via model-router after approval. The rollback is
one bounded cross-layer Web asset behavior packet. The normal backend lane is
Pi/GLM, but the user reported Pi rate-limited and explicitly selected Cursor;
dispatch to Cursor Composer 2.5, then parent-review the diff and rerun every
verification command independently.

```yaml
ROUTING_DECISION:
  ACTION: LOCAL
  LANE: local
  MODE: NONE
  MODEL: NONE
  PACKET_STATUS: NOT_REQUIRED
  PACKET_REBUILD_COUNT: NONE
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE: [.planning/agent-plans/web-shell-asset-rollback.md]
  REASON: The active action is architecture-aware planning and evidence review before the required approval gate.
  ESCALATE_IF: [The user changes the rollback boundary or declines the clean-worktree plan]
```

## Task checklist

### Task 1 — Pin and implement the bare, raw, no-store asset contract (15 min)

- [x] **Failing tests first:**
  - Replace the version-rewrite assertions in
    `crates/ajax-web/src/adapters/assets.rs` with assertions that served
    `app.js` and `terminal.js` equal their generated `dist` bytes, contain only
    bare sibling imports, and contain no `?v=` module edge.
  - Update `crates/ajax-web/src/slices/install.rs` to require bare
    `/app.js` and `/app.css` shell URLs and reject query-versioned shell URLs.
  - Update the focused `runtime.rs` asset tests to require `no-store` and no
    `immutable` token for all three assets, including legacy `?v=` requests,
    while retaining the gzip assertion.
  - Add a real-Rust-server mobile-WebKit test/config/script that opens
    `/#/t/web%2Ffix-login`, records module requests, and requires exactly one
    bare `/app.js`, one bare `/terminal.js`, a rendered task/terminal surface,
    and no React #321, `NotFoundError`, or false “Incompatible server response.”
  - Add an App test whose first Cockpit fetch rejects only when its #616 abort
    signal fires, then prove the next poll succeeds. Existing hidden-mount,
    stale-terminal-session, ErrorBoundary, and timeout-signal tests remain
    unchanged and are part of the focused preservation gate.
  - Run the focused Rust and WebKit assertions before production edits. The
    bare/raw/no-store assertions must fail against current `origin/main` for
    the intended versioned/immutable behavior; the #608/#609/#616 retention
    tests should remain green.
- [x] **Minimal implementation:**
  - In `assets.rs`, keep `app_version` metadata/fingerprinting but delete shell
    URL rewriting, `version_chunk_refs`, both rewritten-bundle caches, and their
    obsolete tests; return raw `include_bytes!` bundles.
  - In `http.rs`, delete the now-unused immutable static-response helper and
    immutable header helper; keep the existing `bytes_axum_response` no-store
    path.
  - In `runtime.rs`, remove URI-query classification and always serve the
    static asset through `bytes_axum_response`; leave `CompressionLayer` intact.
  - In `install.rs`, restore bare shell URL assertions.
  - Add only the smallest Playwright real-server configuration/test and package
    script needed for the task-opening request trace. Mock APIs/WebSocket in the
    page, never the Rust-served HTML/JS/CSS modules.
- [x] **Verification:** Re-run every focused RED command to GREEN, review the
      exact request trace, run `cargo nextest run -p ajax-web`, the App/API/
      terminal/ErrorBoundary tests, type checking, lint, and `git diff --check`.
- [x] **Gate:** Parent accepts, sends one focused Cursor revision, or discards.

### Task 2 — Rebuild and exercise both mobile-WebKit paths (10–15 min)

- [x] **Generated bundle:** Run `npm run web:build:check`; verify tracked
      `dist/index.html`, `app.js`, `app.css`, and `terminal.js` are regenerated
      from source and retain only deterministic changes. No new behavior test
      is needed for generation itself because Task 1's raw-byte and real-server
      tests consume these exact embedded files.
- [x] **Vite mobile smoke:** Run `npm run web:smoke` (the complete existing
      `mobile-webkit` suite).
- [x] **Real Rust server smoke:** Run the new real-server mobile-WebKit command
      against the release-built `ajax-cli`; record the final module request
      trace and response cache headers for the PR body.
- [x] **Preservation checks:** Explicitly rerun the hidden iOS mount, stale
      terminal session renewal, ErrorBoundary diagnostics, GET/session timeout,
      and hung-GET polling recovery tests.
- [x] **Gate:** Record results and continue under the user's advance approval.

### Task 3 — Full repository gate and PR delivery (15 min plus CI)

- [x] Run `npm prepare` and `npm run verify` from the clean rollback worktree.
- [x] Run/confirm the remaining Husky gate through a normal verified commit:
      `cargo build --release -p ajax-cli` and
      `cargo install --path crates/ajax-cli --locked --force`. Never bypass
      hooks.
- [x] Review `git diff`, generated assets, and `git status`; commit only this
      rollback with `fix(web): roll back fragile shell asset cache busting`.
- [ ] Push `fix/web-shell-asset-rollback` and create one PR targeting `main`
      with the same release-please-compatible title.
- [ ] PR body sections must separately document:
  - #595 behavior removed,
  - #608/#609/#616 protections retained,
  - historical and local before/after WebKit request traces,
  - focused, smoke, real-server, full verify, build/install, and CI results.
- [ ] Watch GitHub checks to completion; do not report finished while CI is
      pending or failing.

## Focused and final validation commands

```bash
# Task 1 focused RED/GREEN (exact test names finalized in the READY packet)
cargo nextest run -p ajax-web -E 'test(/raw_generated|bare_static|static_shell_assets/)'
npm run web:test -- --run src/app/App.test.tsx \
  -t 'loads the cockpit on mount while hidden|timed-out cockpit GET releases polling'
npm run web:test -- --run \
  src/shared/lib/api.test.ts \
  src/shared/lib/terminalConnection.test.ts \
  src/shared/ui/ErrorBoundary.test.tsx
npm run web:check
npm run web:lint
git diff --check

# Task 2 build and browser validation
npm run web:build:check
npm run web:smoke
npm run web:smoke:rust

# Task 3 full local PR gate
npm prepare
npm run verify
cargo build --release -p ajax-cli
cargo install --path crates/ajax-cli --locked --force
```

## Risks and stop conditions

- Stop if the raw Rollup chunks do not form one bare import graph after rebuild;
  do not replace runtime rewriting with a second rewriting scheme.
- Stop if the real Rust server test cannot isolate API/WebSocket fixtures
  without changing production auth/session/runtime behavior.
- Stop if preserving gzip requires restoring immutable caching (it should not;
  compression is an independent Axum layer).
- Stop if any #608/#609/#616 retention test regresses.
- Stop before PR creation on any failed local verify, release build/install,
  mobile-WebKit, or real-server smoke command.

## Deviations and command results

- `git show-ref --verify refs/heads/fix/web-shell-asset-rollback` exited 128 as
  expected because the branch did not yet exist; the isolated branch/worktree
  was then created successfully from current `origin/main`.
- One exploratory `sed` used the obsolete path
  `crates/ajax-web/web/index.html` and exited 1; the actual source shell is
  `crates/ajax-web/web/app.html`.
- One broad `rg` included generated `dist` output and was truncated; all
  relevant source files and each requested PR commit were then inspected
  directly.
- First `apply_patch` attempt for the READY packet had one malformed add-file
  line and made no change; the packet was added successfully on the next call.
- `router-log --help` exited 2 because that script has no help flag; its source
  was inspected and the required v2 routing records were then written.
- Cursor round 1 was interrupted while `web:smoke:rust` was running and left
  an on-scope patch but no report. Parent review found `cargo fmt --check`
  failed and the Rust Playwright config discovered all 99 Vite e2e tests.
- Cursor round 2 added the minimal `testMatch`, formatted the Rust files, and
  captured the required RED (3 intended failures) then GREEN (3 passes). Its
  raw report is complete, but Markdown code fences caused the router parser to
  emit `INVALID_STRUCTURED_REPORT`; the two-round gate therefore stopped.
- Parent verification after round 2:
  - PASS: focused Rust asset contract, 3 tests.
  - PASS: hidden iOS mount plus hung-GET recovery, 2 tests.
  - PASS: API timeout, stale terminal session, and ErrorBoundary preservation,
    51 tests.
  - PASS: `npm run web:check`, `npm run web:lint`, `cargo fmt --check`,
    and `git diff --check`.
  - PASS: `npm run web:smoke:rust`, one real release-built Rust-server
    mobile-WebKit task-route test in 2.2 seconds.
  - Expected/non-product failures: the pre-fix RED command exited 100; one
    pre-correction Rust smoke was stopped after proving it incorrectly
    discovered all 99 e2e tests; one retry exited 1 while the round-1 orphan
    still occupied port 18789.
- `npm run web:build:check` passed. It temporarily normalized generated
  trailing whitespace in `dist/app.js`; the required raw `npm run web:build`
  restored the tracked bundle byte-for-byte, leaving no generated diff.
- The first complete `npm run web:smoke` found the Rust-only test under the
  normal Vite config and exited 1 (96 passed, 2 skipped, 1 failed). The test now
  skips unless its `baseURL` is the dedicated Rust HTTPS server. Focused
  Vite discovery then passed with the test skipped, `npm run web:smoke:rust`
  passed 1/1, and the complete Vite mobile-WebKit rerun passed 96 with 3
  intentional skips.
- `cargo nextest run -p ajax-web` passed all 172 tests after the final bundle
  rebuild.
- `npm prepare` passed and installed the Husky hook.
- Full `npm run verify` passed: Cargo format/check/Clippy, 1,714 nextest
  tests, rustdoc, TypeScript, ESLint, ast-grep, and 417 Vitest tests. JSDOM
  printed its known xterm canvas-not-implemented warning, but the command
  exited 0.
- Final JSON-reporter Rust/WebKit trace:
  `/app.js` once (bare, no-store), `/app.css` once (bare, no-store), and
  `/terminal.js` once (bare, no-store).
- Normal commit `206e623` completed with the Husky hook enabled. The hook ran
  raw `web:build`, the full verify gate, release `ajax-cli` build, and locked
  force-install successfully; no bypass flags were used.
- A post-hook `ajax-cli --version` probe exited 2 because this CLI does not
  define a version flag. The installed binary exists at
  `/Users/matt/.cargo/bin/ajax-cli`; this was an exploratory probe, not a gate
  failure.
