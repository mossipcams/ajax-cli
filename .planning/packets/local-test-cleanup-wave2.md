# Packet: local-test-cleanup Wave 2

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

## 1. Status and task contract

READY tests-only. Merge duplicated e2e fixtures, parameterize live_cli verb
dispatch tests, trim smoke rows already covered by App.test.ts.

## 2. Goal

Cut maintenance duplicates without losing unique coverage.

## 3. Allowed files

- `crates/ajax-web/web/e2e/layout-scroll.test.ts`
- `crates/ajax-web/web/e2e/visual.test.ts`
- `crates/ajax-web/web/e2e/fixtures.ts` (read-only unless a tiny export tweak is required)
- `crates/ajax-cli/tests/live_cli.rs`
- `crates/ajax-web/web/e2e/smoke.test.ts`
- `.planning/agent-plans/local-test-cleanup.md`

## 4. Forbidden changes

- Production code
- Deleting `actions.test.ts`, `layout-scroll` test bodies (only fixture harness), `terminal-behavior`, `smoke_user_flows`
- Commits / branch changes
- Writing scripts under `/tmp` — edit files in the worktree only

## 5. Context evidence

- Graphify: NOT_REQUIRED — fixture/import cleanup
- Serena: NOT_REQUIRED — exact anchors below
- ast-grep: NOT_REQUIRED — mechanical delete/replace of named tests

## 6. Code anchors

### A. layout-scroll.test.ts

Delete local `COCKPIT_FIXTURE`, `DETAIL_FIXTURE`, `VERSION_A`, and local `mockFetch` (~L7–121). Import from `./fixtures`:

```ts
import { mockFetch } from "./fixtures";
```

Keep `MAX_TASK_ROW_HEIGHT_PX` and all existing `test(...)` bodies. Confirm calls still use `mockFetch(page)` / `mockFetch(page, extra)` matching fixtures.ts signature.

### B. visual.test.ts

Delete local `COCKPIT_FIXTURE`, `DETAIL_FIXTURE`, and local `mockFetch` (~L19–103). Import:

```ts
import { mockFetch } from "./fixtures";
```

Keep design-token constants and all visual `test(...)` bodies. Do not change asserted colors.

### C. live_cli.rs verb merge

Replace these five one-line tests:

- `ajax_resume_dispatches_like_open`
- `ajax_review_dispatches_like_diff`
- `ajax_ship_dispatches_like_merge`
- `ajax_drop_dispatches_like_clean`
- `ajax_repair_dispatches_like_check`

with one test:

```rust
#[test]
fn ajax_operator_verbs_dispatch_for_seeded_task() {
    for command in ["resume", "review", "ship", "drop", "repair"] {
        assert_task_verb_succeeds(command);
    }
}
```

Keep `ajax_tidy_dispatches_like_sweep` and `ajax_ready_dispatches_like_review` unchanged (different assert shape). Keep `assert_task_verb_succeeds` helper.

### D. smoke.test.ts overlap trims

Delete these two tests (covered by `App.test.ts`):

- `"connection error shows backend unreachable state"`
- `"update banner appears when version changes between polls"`

Keep remaining smoke tests (dashboard, project filter, task detail, action taps).

If deleting the update-banner test leaves `VERSION_A`/`VERSION_B` unused in smoke imports, drop unused imports.

## 7. Test-first instructions

NOT_APPLICABLE

## 8. Edit instructions

1. Perform A–D exactly.
2. Check off Wave 2 checklist items in the plan (leave validation unchecked).
3. Do not touch Wave 1 files except smoke.test.ts as listed.

## 9. Verification commands

```bash
cargo nextest run -p ajax-cli --all-features --test-threads=1 -E 'test(live_cli) or test(ajax_operator)'
npm run web:smoke -- --project=desktop-chromium e2e/layout-scroll.test.ts e2e/visual.test.ts e2e/smoke.test.ts
```

Parent may broaden validation.

## 10. Acceptance criteria

- No duplicated fixture objects in layout-scroll/visual
- Five live_cli verb tests → one looped test; tidy/ready remain
- Two smoke overlaps removed; App.test coverage untouched
- Diff limited to allowed files

## 11. Stop conditions

- fixtures.mockFetch behavior differs enough that visual/layout tests would need assertion changes
- Unused import / compile errors you cannot fix within allowed files
- Any production edit required
