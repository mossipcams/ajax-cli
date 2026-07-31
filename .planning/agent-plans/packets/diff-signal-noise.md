PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Make Diff Review functionally better for vibe review: core labels each changed file `signal` or `noise`; the Web Diff Review file list shows signal files first, collapses noise behind an expand control, and auto-opens the top signal file’s hunks on load.

## Scope

### Allowed
- `crates/ajax-core/src/diff_review.rs`
- `crates/ajax-web/src/slices/diff_review.rs`
- `crates/ajax-web/web/src/shared/lib/types.ts`
- `crates/ajax-web/web/src/features/diff/DiffReview.tsx`
- `crates/ajax-web/web/src/features/diff/DiffReview.test.tsx`
- `crates/ajax-web/web/src/styles.css` (minimal styles for noise collapse only)
- `crates/ajax-web/web/e2e/fixtures.ts` (only if existing diff fixture mocks must include `role`)

### Forbidden
- Guide strip / reading-order carousel / second nav band
- LLM or prose summaries
- Reordering or filtering `TaskDiffProjection.files` (keep parse order)
- Persisting role/guide into task metadata or registry
- Ship / nudge / CI / risk-chip product surfaces
- Architecture.md changes unless required for a one-line projection note
- Unrelated refactors

## Acceptance

1. After parse, each `DiffFile` has `role: Signal | Noise` from deterministic path heuristics in core.
2. Noise heuristics include at least: `Cargo.lock`, `package-lock.json`, `yarn.lock`, `pnpm-lock.yaml`, `*.lock`, and common generated dirs/extensions (`dist/`, `target/`, `.next/`, `*.min.js`, `*.map`). Everything else is Signal.
3. `files` array order remains parse/substrate order; role is annotation only.
4. DTO + TS types pass `role` through (`"signal"` | `"noise"`).
5. DiffReview file list: Signal first (within signal: higher additions+deletions first, then path asc); Noise hidden by default with a control like “N noise” that expands them.
6. When diff becomes ready and there is at least one Signal file, auto-select the first Signal file (after the same sort) so hunks show immediately; if only noise, stay on the file list (noise still collapsed).
7. Focused core + DiffReview tests cover classification, sort/collapse, and auto-open.

## Constraints

- Browser must not invent its own path-glob noise rules; it only consumes `role`.
- Empty `files` still shows existing empty state.
- Do not break PR strip, swipe-back, or Open on GitHub.
- Smallest diff; reuse existing list/hunk UI.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-core --lib diff_review
      expected: pass (includes new role classification tests)
    - type: test
      command: cargo test -p ajax-web --lib diff_review
      expected: pass
    - type: test
      command: npm run web:test -- --run DiffReview
      expected: pass (includes sort/collapse/auto-open)
    - type: typecheck
      command: npm run web:check
      expected: pass
  broader_checks: []
  reason: Pure classification + UI wiring; focused unit tests validate behavior.
```

## Stop if

- Required change exceeds Allowed files or needs browser-owned ranking
- Need to reorder/drop projection `files` to make UI work
- Patch would exceed ~400 changed lines
- Classification rules become content/NLP-based

## Code anchors

- `DiffFile` / `TaskDiffProjection` / `parse_unified_diff` / `project_task_diff` in `crates/ajax-core/src/diff_review.rs`
- `DiffFileDto` / `file_dto` / `diff_dto` in `crates/ajax-web/src/slices/diff_review.rs`
- `DiffFileView` in `crates/ajax-web/web/src/shared/lib/types.ts`
- File list + `selectedPath` in `crates/ajax-web/web/src/features/diff/DiffReview.tsx` (~L72–L268)

## Edit instructions

1. Add `DiffFileRole { Signal, Noise }` and `role` on `DiffFile`; set role in `parse_diff_section` (or a pure `classify_diff_path` called when building each file).
2. Unit-test classification + that parse order is unchanged.
3. Passthrough `role` in ajax-web DTO and TS types.
4. In DiffReview: derive sorted signal/noise lists from `role`; render signal rows; collapse noise; on ready, `setSelectedPath` to top signal once per load (reset when handle/pr changes as today).
5. Add/adjust DiffReview tests; minimal CSS if needed for the noise expand control.
