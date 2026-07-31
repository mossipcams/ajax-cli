# Revert web to PR 683

## Scope
Restore `crates/ajax-web/web` + root `package.json`/`package-lock.json` to post-#683 (`a48ef6e`).
Brings back MusterBar TaskList; drops later dashboard rebuilds and xterm link/floating-menu UI.

## Non-goals / kept
- Do not revert #701 ACP removal (core/cli stay)
- Do not revert #704 agent wait detection
- Keep current `crates/ajax-web/src` (attention band contract + terminal reconnect APIs)
- Keep current `scripts/dev-web-restart.sh` (#688 npm ci)

## Delegation decision
`Delegation decision: not delegated because mechanical restore from known commit`

## Checklist
- [x] Restore web/ + package.json/lock from #683
- [x] Remove post-#683-only terminal link modules
- [x] Keep ajax-web Rust on main (compiles with attention)
- [ ] Commit via husky / verify
- [ ] Open PR
