# Fix CI: File LOC on terminal-behavior e2e

## Failure summary (PR #752)

| Check | Result | URL |
| --- | --- | --- |
| File LOC | **FAIL** | https://github.com/mossipcams/ajax-cli/actions/runs/30921471776/job/92032923806 |
| CI (aggregate) | FAIL (cascades from File LOC) | https://github.com/mossipcams/ajax-cli/actions/runs/30921471776 |
| Web / Clippy / Nextest / Format / … | SUCCESS | same run |
| CodeQL | SUCCESS | separate workflow |

### Root cause

`scripts/check-file-loc.mjs` fails any **changed** source file ≥ 1000 lines.

```
crates/ajax-web/web/e2e/terminal-behavior.test.ts is 3237 lines (limit 1000).
```

On `main` that file is already ~3051 lines. This PR only *edited* it (added double-tap helpers + one test), so the gate sees it as changed and fails on total size. Not a flaky test failure.

Also: soft warning only — `mountTaskTerminalSession.ts` is 929 lines (warn @ 600, hard limit 1000). No action required for merge.

## Fix plan (awaiting approval)

1. Restore `terminal-behavior.test.ts` to the exact `origin/main` contents so it leaves the changed-file set.
2. Move the new double-tap-hold-drag e2e coverage into a new file under `crates/ajax-web/web/e2e/` (e.g. `terminal-copy-selection.test.ts`), reusing existing fixtures / copy-spy helpers with the smallest shared extract needed (prefer import from `fixtures.ts` over duplicating).
3. Keep production code as-is (`mountTaskTerminalSession.ts` / `terminalTouchSelection.ts`).
4. Verify locally:
   - `node scripts/check-file-loc.mjs` (or `GITHUB_BASE_SHA`/`GITHUB_HEAD_SHA` against main)
   - `npm run web:smoke -- e2e/terminal-copy-selection.test.ts` (or the new filename)
5. Commit + push to update PR #752.

### Non-goals

- Full split of the 3k-line `terminal-behavior.test.ts` (pre-existing debt; out of this PR)
- Raising the LOC limit or exempting e2e
- Changing gesture production behavior

## Approval

Approved by user. Implementing.

## Delegation decision

`Delegation decision: not delegated because approved gh-fix-ci plan is a mechanical peel with exact steps; parent implements.`

## Checklist

- [x] Restore `terminal-behavior.test.ts` to `origin/main`
- [ ] Add `terminal-copy-selection.test.ts`
- [ ] Verify LOC + Playwright
- [ ] Commit and push
