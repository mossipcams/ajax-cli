PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Delete dead Muster Bar / redesign leftovers that have zero production callers
after dashboard reverts #706–#709.

Remove from `state.ts` / `state.test.ts`: `fleetSegments`, `FleetSegment`,
`ActiveStatus`, `isQuiet`, `QUIET_THRESHOLD_SECS`, `reposWithFault`,
`severityBucket`, and Muster Bar comments that describe removed UI.

Remove unused CSS: `.pill-fault-dot`, `.task-row.is-quiet` (and quiet-row
helpers if unused). Do not change Active/Idle TaskList behavior or
`attention_items` badges.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/state.ts`
- `crates/ajax-web/web/src/shared/lib/state.test.ts`
- `crates/ajax-web/web/src/styles.css`

## Forbidden changes

- TaskList.tsx behavior / sortCards / attention_items pills
- Dropping `attention` / `inbox` from cockpit DTOs (separate packet)
- Commits, pushes, branch changes

## Acceptance

1. Named helpers above no longer exist; no production imports remain.
2. Their dedicated unit tests are deleted (not skipped).
3. Unused quiet/fault-dot CSS rules removed.
4. `npm run web:test -- --run state` (or equivalent) passes; TaskList tests still pass if run.

## Constraints

- Delete only; no new abstractions.
- Estimated scope ≤ ~150 deleted lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run state TaskList
      expected: state + TaskList tests pass after deletions
    - type: other
      command: rg -n "fleetSegments|isQuiet|reposWithFault|severityBucket|pill-fault-dot|is-quiet" crates/ajax-web/web/src --glob '!**/dist/**'
      expected: no matches in production src (tests may be gone too)
  reason: Proves dead Muster scrap is gone without breaking TaskList.
```

## Stop if

- A production caller is found (then stop and report it)
- Patch would exceed ~400 changed lines
