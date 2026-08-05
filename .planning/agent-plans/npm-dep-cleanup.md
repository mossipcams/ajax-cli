# npm dependency cleanup

## Scope
- Remove redundant top-level npm pins (`@testing-library/dom`, `tailwindcss`)
- Add explicit `@eslint/js` devDependency (direct import in ESLint config)
- Deduplicate `@eslint/js` usage in `eslint.config.mjs`
- Open PR from `ajax/depcruiser`

## Non-goals
- Rust dependency changes (machete + udeps already clean)
- knip/depcruiser tooling setup
- Removing transitive packages from the tree

## Delegation decision
not delegated because the diff is smaller than the work order (package.json + one config file + lockfile)

## Checklist
- [x] Edit `package.json`
- [x] Fix `crates/ajax-web/web/eslint.config.mjs`
- [x] Regenerate `package-lock.json` via `npm install`
- [x] Run `npm run web:lint` — exit 0
- [x] Run `npm run web:build` — exit 0
- [x] Run `npm run web:test -- --run` — 704 passed, exit 0
- [ ] Commit, push, open PR

## Validation results
- `npm run web:lint` — exit 0
- `npm run web:build` — exit 0
- `npm run web:test -- --run` — 704 passed, exit 0
- Full `npm run verify` — skipped (focused web gate only for dep-cleanup scope)
