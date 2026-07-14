# TDD Implementation Packet — R2 wterm WASM load cache

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
BLOCKERS: []
```

## Goal
Speed first Surface V2 load: stop forcing `cache: "no-store"` on the WASM
validate fetch so the browser (and `warmTerminalAssets` preload) can reuse
bytes; keep validate-then-`GhosttyCore.load` path and security checks.

## Allowed files
- `crates/ajax-web/web/src/terminalWtermGhosttyCore.ts`
- `crates/ajax-web/web/src/terminalWtermGhosttyCore.test.ts`

## Forbidden changes
- Do not remove WASM `init` export validation
- Do not change WtermTerminalView
- Do not commit/push/branch

## Context evidence
- Graphify: `NOT_REQUIRED`
- Serena: `NOT_REQUIRED`
- ast-grep: `NOT_REQUIRED`
- Anchor: `fetchWtermWasmBytes` uses `fetch(WTERM_GHOSTTY_WASM_URL, { cache: "no-store" })`
- `warmTerminalAssets` already `fetch(WTERM_GHOSTTY_WASM_URL)` without no-store — wasted when validate bypasses cache
- Comment in loader says second fetch is intentional and must stay on HTTP URL

## Code anchors
- `crates/ajax-web/web/src/terminalWtermGhosttyCore.ts` → `fetchWtermWasmBytes`
- Unit tests stub `fetch` and assert load options; add assertion on fetch init `cache` absent or `"default"` / `"force-cache"` (not `"no-store"`)

## Test-first instructions
1. Add/extend unit test: when `loadWtermGhosttyCore` succeeds, the validate `fetch` call must **not** pass `cache: "no-store"`. Prefer asserting `cache` is undefined or `"force-cache"`.
2. RED: `cd crates/ajax-web/web && npx vitest run src/terminalWtermGhosttyCore.test.ts`

## Edit instructions
1. Change validate fetch to omit `cache: "no-store"` (use default browser HTTP cache), or explicitly `cache: "force-cache"`.
2. Keep error messages and `wasmExportsInclude(..., "init")` check.
3. Keep `GhosttyCore.load({ wasmPath, scrollbackLimit })` second fetch on the HTTP URL.

## Verification commands
```bash
cd crates/ajax-web/web && npx vitest run src/terminalWtermGhosttyCore.test.ts
```

## Acceptance criteria
- Validate fetch no longer uses `no-store`
- Existing failure-path tests still pass
- RED then GREEN proven

## Stop conditions
- Rewriting to single-fetch by changing `@wterm/ghostty` internals
- Removing validation
