# Xterm paste fix — preserve merged PR 510 assertions

## Status

```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
```

## Goal

Pass the paste behavior merged in PR 510 without rewriting its expected payload,
while also preserving the new bracketed-paste and focus-ownership coverage.

## Allowed files

- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`
- `crates/ajax-web/web/src/components/TaskTerminal.svelte`

## RED

Restore the two original PR 510 assertions exactly:

```ts
expect(frames.at(-1)?.data).toBe(MULTILINE_UNICODE_CLIPBOARD);
```

Remove the added `_PTY` expectation constant. Run the two original paste cases
and show they fail because xterm's default paste conversion changes LF to CR.
Do not change any other existing assertion.

## Implementation

Use xterm's public `term.modes.bracketedPasteMode` to apply DEC 2004 wrappers
when active, but send the original clipboard/native-fallback text byte-for-byte
inside those wrappers so PR 510's LF and Unicode content remain intact. Preserve
the accepted conditional focus ownership behavior. Do not use private xterm
APIs, normalize line endings, or weaken/delete/rewrite assertions.

Adjust the new bracketed-paste assertion only if needed to expect the original
LF payload inside the DEC wrappers; that strengthens alignment with the merged
contract rather than replacing it.

## Verification

1. Original two paste cases plus bracketed paste, fallback, and Paste-focus
   cases: all green.
2. Full `terminal-behavior.test.ts`, Mobile WebKit, one worker: all green.
3. `npm run web:check`.
4. `git diff --check`.

No other files, helpers, fixtures, dependencies, debug code, plans/packets,
generated assets, commits, or branch changes.
