PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Make Ajax web dashboard task list order sticky within each UI section (Active
and Idle). Rows must not reshuffle above/below peers while they remain in the
same section. A task may move only when it crosses Active ↔ Idle.

## Scope

### Allowed

- `crates/ajax-web/web/src/shared/lib/state.ts`
- `crates/ajax-web/web/src/shared/lib/state.test.ts`

### Forbidden

- Backend/status derivation, poll intervals, Muster Bar `fleetSegments`
- `TaskList.tsx` wiring changes unless required (stableOrder already works)
- Unrelated refactors, renames, formatting sweeps
- Commits, pushes, branch changes

## Acceptance

1. With `previousOrder` supplied, two Active cards that flip between `running`
   and `waiting` keep their relative order (no within-Active reshuffle).
2. With `previousOrder` supplied, same-status activity leapfrogs still keep
   relative order (existing sticky behavior preserved).
3. A card that moves Idle → Active (or Active → Idle) still changes section
   despite `previousOrder`.
4. Cold start (`previousOrder` empty) still produces a deterministic order
   (section, then existing status/activity/handle tie-break is fine).
5. Existing TaskList Active/Idle band split continues to work; no UI redesign.

## Constraints

- Presentation-only. Do not invent new task truth or status policy.
- Prefer smallest diff: change the `sortCards` comparator to section-first
  (`idle` vs non-`idle`, matching TaskList bands), then sticky `previousOrder`,
  then existing activity/handle fallback for unknown/new handles.
- Keep `statusRank` / `STATUS_ORDER` for badges and cold-start tie-break if useful;
  do not use fine-grained status rank to reorder when both cards already appear
  in `previousOrder` and share a section.
- Match TaskList section rule: Active = `status !== "idle"` (unknown stays Active).

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cd crates/ajax-web/web && npx vitest run src/shared/lib/state.test.ts
      expected: all tests pass, including new same-section stability cases
    - type: existing_test
      command: cd crates/ajax-web/web && npx vitest run src/features/task/TaskList.test.tsx
      expected: all tests pass
  broader_checks: []
  reason: Pure sort helper; focused unit tests prove section stickiness; TaskList tests guard band split.
```

## Stop if

- Fix requires changing Rust cockpit projection or architecture.md
- Diff exceeds ~100 lines or needs more than the Allowed files
- Tests cannot express the behavior without UI e2e

## Code anchors

- `sortCards` in `crates/ajax-web/web/src/shared/lib/state.ts` (approx lines 54–78)
- Existing sticky tests in `crates/ajax-web/web/src/shared/lib/state.test.ts`
- TaskList bands: Active = `status !== "idle"`, Idle = `status === "idle"` in
  `crates/ajax-web/web/src/features/task/TaskList.tsx`

## Context evidence

Current comparator sorts by `statusRank` first, so sticky `previousOrder` only
runs when ranks match. Running↔waiting flaps therefore reshuffle inside Active.
`TaskList` already passes `stableOrder` into `sortCards` and refreshes it after
each calm order — wiring is fine; comparator is the bug.

## Edit instructions

1. Add/adjust tests in `state.test.ts`:
   - same-section `running`↔`waiting` with previous order preserves order
   - Idle↔Active still reorders by section despite previous order
   - Update any test that incorrectly expects within-Active status-rank
     reshuffle when previous order is present
2. In `sortCards`, compare section first (`idle` last / non-idle first), then
   existing previousOrder sticky logic, then activity/handle (and optionally
   statusRank only for cold/new cards).
3. Run the verification commands above.
