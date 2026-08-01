```yaml
PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior
dispatch_level: compact

goal: |
  Timed ProcessCommandRunner Capture must not deadlock when child stdout/stderr
  exceeds the OS pipe buffer. Diff Review's `git diff` / `gh pr diff` (~hundreds
  of KB) currently hit TimedOut at 30s even though the command finishes in ms
  when stdout is drained.

allowed_scope:
  - crates/ajax-core/src/adapters/process.rs

forbidden_scope:
  - crates/ajax-web/**
  - architecture.md
  - any other adapter

acceptance:
  - New unit test proves Capture+timeout succeeds for stdout larger than 64KiB
    (e.g. 256KiB of printable bytes) within a few seconds.
  - Existing process timeout/kill tests still pass.
  - Implementation drains stdout and stderr concurrently while waiting for exit
    (reader threads or equivalent); do not only read after try_wait succeeds.
  - Prefer stdin Stdio::null() for Capture so writers cannot block on stdin.
  - On timeout: still kill+wait the child; join reader threads so pipes close.
  - No new dependencies.

verification:
  methods:
    - type: test
      command: cargo test -p ajax-core --lib adapters::process -- --test-threads=4
      expected: pass including new large-stdout test
  broader_checks:
    - cargo test -p ajax-core --lib diff_review -- --test-threads=4
  reason: focused process runner regression plus nearby diff_review suite

anchors:
  - crates/ajax-core/src/adapters/process.rs fn run_capture (~L47-89)
  - existing tests capture_command_times_out_when_configured, timed_out_command_is_terminated
```
