# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: validation-only-temporary-mutations
TEST_FIRST: NOT_APPLICABLE
FINAL_PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Perform the requested representative mutation proof against the permanent
mobile-WebKit behavior suite: one input break, one resize break, and one
lifecycle break must each fail the relevant test, then be restored exactly.

# Allowed file for temporary edits

- `crates/ajax-web/web/src/components/TerminalRawView.svelte`

# Forbidden changes

- Any lasting file change, any test/docs/plan edit, git commands, stash,
  checkout/reset, skips, or assertion changes.
- Use patch-based edits only. Capture the file SHA-256 before the first break
  and prove it is identical after every restore/finally.

# Mutation sequence

Run sequentially. Restore immediately after each failing command even if an
unexpected error occurs.

1. **Input:** temporarily make the existing
   `sendKey = (data) => connection.sendInput(data)` send the same data twice.
   Run the permanent test named `supported Ctrl toolbar combinations send exact
   control codes and disarm sticky Ctrl`. It must fail because the ordered PTY
   frame slice contains duplicates. Restore the exact original line and verify
   the focused test passes.
2. **Resize:** temporarily suppress the existing
   `connection.sendResize(cols, rows)` call inside the resize-dedupe callback
   without changing other fit logic. Run the permanent test named `initial open
   eventually sends at least one valid positive-integer PTY size`. It must fail
   because no resize frame arrives. Restore and verify focused pass.
3. **Lifecycle:** temporarily omit `connection.dispose()` from the component
   cleanup. Run `navigation away closes the active socket and removes the
   surface`. It must fail because the active socket remains. Restore and verify
   focused pass.

# Commands

Use the root script with mobile WebKit and exact `-g` names. Record exit codes
and the assertion/timeout excerpt proving each expected failure. After all
restores run:

```bash
npm run web:smoke -- --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Three intended failures captured for the intended reasons.
- Three focused passes after restore.
- Full permanent file passes.
- Final SHA-256 equals initial SHA-256; the only existing production diff (the
  stable interaction test ID) is unchanged.
