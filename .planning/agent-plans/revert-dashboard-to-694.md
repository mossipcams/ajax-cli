# Revert dashboard to PR 694

## Scope
Restore Web Cockpit home UI to the post-#694 tree (TaskList + swipe reveal). Removes the #696+ Dashboard / SystemPanel / RepoPanel control-panel rebuild.

## Non-goals
- Do not revert #701 (ACP removal)
- Do not revert #704 (agent wait detection)
- Do not touch terminal reconnect (#694 itself stays)

## Delegation decision
`Delegation decision: not delegated because mechanical restore from known commit 40b0f28`

## Checklist
- [x] Branch from `origin/main`
- [x] Restore web UI paths from `40b0f28` (#694 merge)
- [x] Delete `features/dashboard` and `features/repositories` (post-#696)
- [ ] `npm run verify`
- [ ] Open PR

## Validation
```bash
npm run verify
cargo build --release -p ajax-cli
cargo install --path crates/ajax-cli --locked --force
```
