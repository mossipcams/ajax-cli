# Strengthen wterm load: real-WASM init test + official options

## Scope

Stop shipping yellow-banner regressions that mocks never catch.

1. Keep in-memory instantiate (no Safari blob fetch).
2. Construct GhosttyCore with explicit `{ scrollbackLimit }` matching
   `GhosttyCore.load()`.
3. Add an **unmocked** integration test: real `@wterm/ghostty` WASM →
   `loadWtermGhosttyCore()` → `init` → `writeString` → `getCell`, plus
   `WTerm.init` smoke.

## Non-goals

- Production migration off Ghostty
- E2E Playwright on device (follow-up)

## Delegation decision

`Delegation decision: not delegated because parent owns the failing runtime
contract and test gap.`

## Checklist

- [ ] Explicit scrollbackLimit in constructor options
- [ ] Integration test without mocking GhosttyCore
- [ ] WTerm.init smoke with real core
- [ ] Rebuild dist + PR
