# Slice 11 — implement terminal.js chunk (codemod)

Parent: `react-migration-cleanup.md`
Reverses the 2026-07-19 deferral (`03d9e2b`).

## Decision

**Implement** a deferred `terminal.js` chunk. Single-file embed contract expands
to four assets: `index.html`, `app.js`, `app.css`, `terminal.js`.

## Method

Temporary `scripts/terminal-chunk-codemod.mjs` applies the chunking + install
contract fix (slice 9 shape). Script is deleted in the same change.

`Delegation decision: not delegated because` the codemod is the implementation
vehicle; parent writes/runs/deletes it and owns the embed-contract gate.

## Scope

- Lazy-load `TaskTerminal` from `TaskDetail`
- Vite: sole dynamic import → deterministic `terminal.js` (no `manualChunks` —
  forcing TaskTerminal/@xterm into a named chunk pulled `api.ts` into the
  deferred file and left `/api/operations` out of `app.js`)
- Embed + serve `/terminal.js`; fingerprint includes it
- Amend `web-build-check` (require `app.js` + `terminal.js`; HTML still one script)
- Update install/assets tests + `architecture.md`

## Non-goals

- No hashed chunk names
- No `terminal.js` preload in HTML (dynamic import only)
- No ghostty/wasm revival
- No further Radix/code splits

## Checklist

- [x] Codemod written and applied; script deleted
- [x] `app.js` gzip down; `terminal.js` present and served
  - Measured: `app.js` ~303 KiB / ~95 KiB gzip; `terminal.js` ~356 KiB / ~92 KiB gzip
  - (pre-split single `app.js` was ~644 KiB / ~182 KiB gzip)
- [x] `web:check` / `web:test` (387) / `web:lint` / `web:sg` / `web:build:check`
- [x] `cargo nextest -p ajax-web` (161 passed)
- [x] terminal e2e still green (lazy mount) — 68 passed / 68 skipped (desktop project)
- [x] Commit

## Deviations

- First apply used `manualChunks` for TaskTerminal/@xterm; install test failed
  because `/api/operations` lived only in `terminal.js`. Codemod re-run replaced
  that with natural dynamic-import chunking + `chunkFileNames` → `terminal.js`.
