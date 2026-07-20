# Rebuild web dist so Open Dev is actually gone

## Scope

- Rebuild `crates/ajax-web/web/dist/*` from current source so the embedded shell matches `TestInDevPanel.tsx` (no Open Dev pill).
- Commit the refreshed dist assets.

## Non-goals

- No source/UI logic changes (already removed in #589).
- No backend changes.
- No PR unless asked.

## Delegation decision

`Delegation decision: not delegated because mechanical asset rebuild (npm run web:build) — smaller than a work order; R-LOCAL-TINY / mechanical exception.`

## Task checklist

- [x] Run `npm run web:build` (needed `npm install` first — vite missing)
- [x] Confirm `dist/app.js` has no `Open Dev` / `open-dev-button` / `ajaxdev.mossyhome`
- [ ] Commit refreshed `crates/ajax-web/web/dist/*`
- [x] Record validation

## Approval

User authorized rebuild + land (2026-07-19).

## Deviations

- Fresh worktree had no `node_modules`; ran `npm install` before `web:build`.

## Validation

```bash
npm install   # exit 0
npm run web:build   # exit 0 — app.js/css/terminal.js + index.html
rg -n 'Open Dev|open-dev-button|ajaxdev\.mossyhome' crates/ajax-web/web/dist/app.js
# no matches (CLEAN)
```
