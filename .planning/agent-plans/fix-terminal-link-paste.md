# Fix terminal link paste (insertText + toolbar read)

**Date:** 2026-08-11  
**Issue:** https://github.com/mossipcams/ajax-cli/issues/787  
**Mode:** Behavior Change

## Scope

Native / keyboard paste of http(s) links into the Web Cockpit task terminal must
reach the PTY. Toolbar Paste must prefer rich clipboard types when available.

## Root cause

Safari/keyboard often delivers link paste as `beforeinput` `insertText` with the
full URL in `event.data`. That path was ignored (only `insertFromPaste*`).
Toolbar Paste used `readText()` only, losing html/uri-list hrefs.

## Checklist

- [x] Open #787
- [x] beforeinput: URL-shaped insertText / insertReplacementText → paste
- [x] readPasteText: unquoted href
- [x] toolbar Paste: clipboard.read typed → readPasteText, else readText
- [x] Unit + separate e2e file (avoid terminal-behavior LOC)

## Validation

```bash
npx vitest run src/shared/lib/clipboard.test.ts src/features/task/TaskTerminal.test.tsx
```
