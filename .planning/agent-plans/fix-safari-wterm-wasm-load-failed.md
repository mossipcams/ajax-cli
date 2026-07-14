# Fix Safari "Load failed" for wterm WASM

## Scope

Replace blob-URL re-fetch in `loadWtermGhosttyCore` with in-memory
`WebAssembly.instantiate` + `GhosttyCore` construction. Safari reports
opaque `TypeError: Load failed` on `fetch(blob:…)`.

## Non-goals

- Changing the served `/wterm-ghostty-vt.wasm` route
- Ghostty default path

## Delegation decision

`Delegation decision: not delegated because focused hotfix smaller than a
packet/delegate round.`

## Checklist

- [ ] Instantiate from validated bytes (no blob URL)
- [ ] Wrap fetch/instantiate errors with path context
- [ ] Update tests; rebuild dist; new PR from main
