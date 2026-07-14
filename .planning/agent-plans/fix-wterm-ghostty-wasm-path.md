# Fix wterm Ghostty WASM path collision

## Scope
Serve `@wterm/ghostty`'s WASM at `/wterm-ghostty-vt.wasm` and pass that path
into `GhosttyCore.load`. Keep `/ghostty-vt.wasm` as ghostty-web's binary.

## Non-goals
- Changing Ghostty default terminal
- Upstream wterm patches

## Delegation decision
`Delegation decision: not delegated because focused hotfix with exact root cause
and anchors; smaller than a packet/delegate round.`

## Checklist
- [x] Vite copies + serves `/wterm-ghostty-vt.wasm`
- [x] WtermTerminalView passes `wasmPath`
- [x] Rust assets + route embed/serve the new file
- [x] Fingerprint includes new wasm
- [x] Tests + web:build:check
- [ ] Commit and push to PR branch

## Root cause
Both `ghostty-web` and `@wterm/ghostty` ship `ghostty-vt.wasm` with different
export APIs. Ajax served only ghostty-web's binary at `/ghostty-vt.wasm`, so
wterm called missing `init`.
