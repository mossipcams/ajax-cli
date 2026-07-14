# Harden wterm WASM load (yellow error)

## Scope

Stop Terminal Surface V2 yellow init banner caused by loading the wrong
WASM (`exports.init is not a function`) or a stale/non-WASM response.

## Non-goals

- Switching off `@wterm/ghostty` to the built-in Zig core
- Ghostty layout changes

## Delegation decision

`Delegation decision: not delegated because focused hotfix with exact root
cause; smaller than a packet/delegate round.`

## Checklist

- [ ] `loadWtermGhosttyCore()` fetches `/wterm-ghostty-vt.wasm` with
      `cache: "no-store"`, validates magic + `init` export, loads via blob URL
- [ ] `WtermTerminalView` uses that helper (never bare `GhosttyCore.load()`)
- [ ] Tests + rebuild dist + push PR 465
