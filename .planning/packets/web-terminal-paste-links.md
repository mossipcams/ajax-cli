PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact
TASK_KIND: behavior

# Packet: web terminal paste rich links

## Goal

Native paste into the Web Cockpit task terminal must send HTTP(S) link URLs to
the PTY even when the clipboard only exposes the URL via `text/uri-list` or
`text/html` (empty or non-URL `text/plain`).

Today `@xterm/xterm` `handlePasteEvent` only reads `text/plain`, so rich link
pastes appear to do nothing.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/clipboard.ts`
- `crates/ajax-web/web/src/shared/lib/clipboard.test.ts`
- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts` (only if needed for one
  native paste / rich-link case)

## Forbidden changes

- Commits, pushes, merges, rebases, branch changes
- Editing `architecture.md`, CLI/TUI, xterm package version, link Open/Copy menu
  behavior (except incidental paste wiring)
- Weakening or deleting existing tests
- New dependencies
- Files outside Allowed files

## Code anchors

- xterm paste only reads plain:
  `node_modules/@xterm/xterm/src/browser/Clipboard.ts` `handlePasteEvent`
- Reuse `pasteToPty` / `pasteThroughTerm` in
  `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- Helper textarea: `textarea.xterm-helper-textarea` after `liveTerm.open`;
  ZWS `BACKSPACE_SENTINEL`
- Wire capture-phase paste after textarea listeners (~1225); cleanup with them

## Constraints

- Prefer http(s) URL from plain / uri-list / html href; else first non-empty
- Capture paste: `preventDefault` + `stopImmediatePropagation` (no xterm double)
- Reseed ZWS sentinel after paste
- Preserve bracketed paste and disconnect-retain via `pasteThroughTerm`

## Acceptance criteria

- Paste with only `text/uri-list: https://example.com/a` sends that URL once
  (bracket-wrapped when DEC 2004 is on).
- Paste with only HTML `<a href="https://example.com/b">label</a>` sends
  `https://example.com/b`.
- Paste with `text/plain: https://example.com/c` still sends once (no double).
- Paste with `text/plain: Click here` plus html href `https://example.com/d`
  sends `https://example.com/d`.
- Empty clipboard paste sends nothing and does not throw.
- Existing toolbar paste / fallback / disconnect-retain tests stay green.
- ZWS backspace sentinel still reseeds after paste.

## Verification

- type: test
  command: `cd crates/ajax-web/web && npm run test -- --run src/shared/lib/clipboard.test.ts src/features/task/TaskTerminal.test.tsx`
  expected: pass; new `readPasteText` cases green
- type: test
  command: `cd crates/ajax-web/web && npm run test -- --run e2e/terminal-behavior.test.ts -g "paste"`
  expected: existing paste cases pass; new rich-link case green if added
- broader: `cd crates/ajax-web/web && npm run check`

## Stop if

- Need to patch xterm itself or change addon APIs
- Paste ownership conflicts with iOS delete/sentinel path needing architecture
  review
- Scope grows beyond Allowed files
