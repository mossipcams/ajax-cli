# Revert dashboard to PR 696

## Scope
Revert post-696 dashboard PRs so main’s web dashboard matches #696 (one-tap control panel).

## Non-goals
- Do not revert #704 (agent wait detection)
- Do not revert #701 (ACP undo) or other non-dashboard work

## Delegation decision
`Delegation decision: not delegated because mechanical git revert of known squash commits`

## Checklist
- [x] Create branch from `origin/main`
- [x] Revert #705, #703, #702, #697 (newest first)
- [x] Confirm `Dashboard.tsx` / `styles.css` match tree at #696 merge `e6bf9cc`
- [ ] `npm run verify`
- [ ] Open revert PR

## Validation
```bash
npm run verify
```
