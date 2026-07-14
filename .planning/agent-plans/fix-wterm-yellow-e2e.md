# Catch Surface V2 yellow banner in mobile-webkit e2e

## Scope

1. Add Playwright mobile-webkit e2e that enables `ajax.terminal.surfaceV2`,
   asserts no `#terminal-surface-v2-error`, asserts wterm mounts and paints.
2. Simplify loader to official `GhosttyCore.load({ wasmPath, scrollbackLimit })`
   after validating bytes (no private constructor, no production smoke double-init).
3. Surface full error text + `console.error` so device failures are copyable.

## Why

Unit/jsdom tests stayed green while device still showed yellow. CI never ran
Surface V2 on WebKit. Local WebKit e2e already mounts wterm — make that the gate.

## Delegation decision

`Delegation decision: not delegated because parent owns the failing device
contract and e2e gap.`

## Checklist

- [x] e2e terminal-surface-v2.test.ts (mobile-webkit) — settle on grid vs yellow
- [x] Loader uses GhosttyCore.load after validate
- [x] Remove production smokeInit (keep in integration tests only)
- [x] Run mobile-webkit e2e green locally
- [x] web unit/integration + rebuild dist
- [ ] PR

## Why prior tests missed yellow

Init failure unmounts `[data-terminal-engine=wterm]` and shows the mustard
banner. Asserting “panel visible” alone races the brief pre-failure mount.
Production `smokeInitWtermGhosttyCore` also called `core.init` before
`WTerm.init` (double-init) — integration/mocks could pass while Safari failed.

## Validation

| Command | Result |
|---|---|
| focused vitest (4 files / 22) | pass |
| `web:smoke --project=mobile-webkit e2e/terminal-surface-v2.test.ts` | pass (2) |
| `web:check` | pass |
| `web:build` | pass |

## Deviations

None vs plan scope.
