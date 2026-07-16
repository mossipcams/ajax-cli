# Packet: local-test-cleanup Wave 3

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 1. Status and task contract

READY. Harden remaining weak asserts; refresh stale terminal test docs to
post–Task 12 truth. No production edits.

## 2. Goal

Finish local-test-cleanup: stronger asserts where we keep tests, honest docs
where Ghostty suites are already gone.

## 3. Allowed files

- `crates/ajax-cli/tests/live_cli.rs`
- `crates/ajax-web/web/src/components/NewTaskSheet.test.ts`
- `crates/ajax-web/web/src/diagnostics.test.ts`
- `crates/ajax-web/web/e2e/smoke.test.ts` (comment-only fix at top)
- `crates/ajax-web/web/TERMINAL_LEGACY_SURFACE_TESTS.md`
- `crates/ajax-web/web/TERMINAL_REBUILD_ACCEPTANCE.md` (status banner / matrix notes only — do not invent new acceptance rows)
- `crates/ajax-web/web/src/viewport.test.ts` (comment-only if it cites deleted TerminalPanel.test.ts)
- `.planning/agent-plans/local-test-cleanup.md`

## 4. Forbidden changes

- Production code
- Deleting `legacyTerminalRemoval.test.ts` or `terminal-behavior.test.ts`
- Broad rewrites of TERMINAL_BEHAVIOR_CONTRACT.md (out of scope this wave)
- Commits / `/tmp` helper scripts

## 5. Context evidence

- Graphify: NOT_REQUIRED
- Serena: NOT_REQUIRED
- ast-grep: NOT_REQUIRED

## 6. Code anchors

### A. Harden `ajax_tidy_dispatches_like_sweep` in live_cli.rs

Current weak assert only checks `commands` is Some array. The seeded task is
**risky** (`NeedsInput`), so the plan may have an **empty** `commands` array —
do **not** assert non-empty. Replace with exact wiring checks:

```rust
assert_eq!(body["title"], "sweep cleanup");
assert!(
    body["commands"].is_array(),
    "tidy --json should expose a commands array: {body}"
);
```

(`CommandPlan` title from `sweep_cleanup_plan` is `"sweep cleanup"`.)

Do **not** delete this test; it is the binary wiring check for `tidy` → plan JSON.

### B. NewTaskSheet.test.ts

Change:
```ts
expect(arg.request_id).toBeTruthy();
```
to:
```ts
expect(arg.request_id).toEqual(expect.any(String));
expect(arg.request_id.length).toBeGreaterThan(0);
```

### C. diagnostics.test.ts

Change:
```ts
expect(result.body).toContain("version");
```
to:
```ts
expect(result.body).toBe('{"version":"0.1"}');
```

### D. smoke.test.ts header comment

Update the top comment so it no longer claims "connection-recovery" / version-poll flows if those tests were removed in Wave 2. Keep accurate description of remaining smoke coverage.

### E. Docs — TERMINAL_LEGACY_SURFACE_TESTS.md

Rewrite the top status to state clearly:

- Task 12 complete; listed Ghostty/Surface V2 files are **already deleted**
- This file is a historical inventory; permanent suites are `e2e/terminal-behavior.test.ts` + `terminalConnection.test.ts` + Rust PTY tests
- `legacyTerminalRemoval.test.ts` is the living hygiene guard

Do not re-add deleted files. Trim or strike claims that `e2e/smoke.test.ts` still has terminal canvas rows (they are gone).

### F. TERMINAL_REBUILD_ACCEPTANCE.md

Add a short status note at the top that Task 12 removal is done and the matrix’s “Red after Task 12” cells are historical pre-removal evidence, not current CI status. Do not rewrite the whole matrix.

### G. viewport.test.ts comment (optional)

If a comment cites deleted `TerminalPanel.test.ts`, update to current sibling (`terminal-behavior` / `keyboardBandPin`).

## 7. Test-first instructions

NOT_APPLICABLE — hardening existing asserts.

## 8. Edit instructions

Perform A–G. Mark Wave 3 checklist items complete in the plan (validation left for parent if preferred).

## 9. Verification commands

```bash
cargo nextest run -p ajax-cli --all-features --test live_cli
npx vitest --config crates/ajax-web/web/vite.config.mts --run \
  crates/ajax-web/web/src/components/NewTaskSheet.test.ts \
  crates/ajax-web/web/src/diagnostics.test.ts
```

## 10. Acceptance criteria

- tidy live test asserts non-empty commands + non-empty title
- request_id / diagnostics asserts are exact
- Legacy terminal docs no longer imply deleted suites still run
- No production changes

## 11. Stop conditions

- tidy JSON missing `title` field (report actual keys; do not invent)
- Scope exceeded
