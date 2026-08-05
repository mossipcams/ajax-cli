PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Fix Web Cockpit process restart so the server never `exit(0)` unless a successor process (respawn or restart script) was successfully spawned. Apply the same rule to Test-in-Stable scheduling.

Today both `schedule_process_restart` and `schedule_test_in_stable` log spawn failure then still `std::process::exit(0)`, which can leave the operator with no listener and no replacement.

## Allowed files

- `crates/ajax-web/src/adapters/server.rs`

## Forbidden changes

- Restart delay, env var names, or script argument shapes
- Blue/green / dual-bind listeners
- Health endpoint (R1), cockpit refresh / CAS (R2/R4)
- Commits, pushes, branch changes
- Frontend or architecture docs

## Acceptance

1. Successful `launch_restart` / script spawn still leads to process exit (production path).
2. If spawn/launch returns `Err`, process does **not** exit; logs and stays running.
3. Test-in-Stable: exit only after successful wrapper spawn; if `RESTART_SCRIPT_ENV` missing or spawn fails, do **not** exit.
4. Under `cfg(test)`, scheduling remains a no-op (does not terminate the test runner).
5. Extract a pure exit-policy helper (e.g. `should_exit_after_launch(Result<(), String>) -> bool`) with unit tests that never call `process::exit`.

## Constraints

Smallest diff; no new dependencies. Keep production bodies behind `cfg(not(test))`.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo nextest run -p ajax-web -- adapters::server
      expected: all pass including new exit-policy tests
    - type: build
      command: cargo check -p ajax-web --all-targets
      expected: compiles
  reason: Pure exit policy; focused adapter tests suffice.
```

## Stop if

- Requires supervisor/launchd contract changes beyond stay-up-on-failure
- Diff > ~80 lines or files outside Allowed
- Ambiguity on missing Test-in-Stable env — use Acceptance #3 (no exit without successful spawn)
