# Process probes without `/bin/ps` so real-process tests run inside nono

Approval: **approved 2026-10-26** (Matt: "Yes" to the plan below).

## Problem

Ajax Web reads process facts by executing `ps`. `/bin/ps` is setuid root
(mode 4755), and nono refuses to exec setuid binaries ("Operation not
permitted", rc 126). The production web server runs unsandboxed, so it is
unaffected. Tests run inside the agent's nono sandbox, so every real-process
web-session test stops at "start-time token unreadable", which is recorded in
`task_session_child.rs` before a run attaches. That covers ~147 `ajax-web`
tests and the #1416 end-to-end "Background work running" test.

In-process `proc_pidinfo`, `sysctl` and `kill(pid, 0)` work inside nono
(measured 2026-10-23, `delegate-run-identity` spike). No profile grant can
safely allow setuid exec, so the fix belongs in Ajax, not the sandbox.

## Scope

One small module in `crates/ajax-web/src/slices/web_session/` that reads
process facts in-process:

- macOS: `nix::libc::proc_pidinfo(PROC_PIDTBSDINFO)` (start time, ppid,
  status, comm) and `proc_listallpids` for the table. Unsafe blocks stay
  minimal and carry `SAFETY:` comments, as in `tmux_task_session/attach.rs`.
- Linux: `/proc/<pid>/stat`. Parse `comm` up to the last `)`; it can contain
  spaces and parentheses.

These call sites switch to it:

| Call site | Was | Now |
|---|---|---|
| `process_identity::start_token` | `ps -o lstart=` | start time from the kernel |
| `execution_ownership::process_alive` | `ps -o pid=` | `kill(pid, 0)`; EPERM = alive, ESRCH = dead |
| `orphan_sweep::pid_alive` | `kill -0` + `ps -o stat=` | exists and not a zombie |
| `process_tree::snapshot` | `ps -A -o pid=,ppid=,comm=` | the kernel process table |
| `session_client_pid_tests::assert_live_process` | `ps` | the new liveness helper |

`nix` is already a workspace dependency, so `ajax-web` adds
`nix = { workspace = true }` and nothing new enters the lockfile.

## Non-goals

- `ajax-cli/src/ci_agent_delivery.rs` keeps `ps -o comm=`. It runs through the
  mockable `CommandRunner` on the CLI/tmux path and does not block these tests.
- No nono profile change.
- No change to the identity rules themselves: never signal on a missing,
  mismatched or unreadable token, and treat unreaped zombies as gone.

## Deviations and risks

- **Token format changes.** `child_started` was a `ps lstart` string and now
  becomes a kernel start time (`<sec>.<usec>` on macOS, start ticks on Linux).
  A row recorded by the old binary mismatches, so it takes the existing safe
  path: the sweep never signals it and logs "token not confirmed". The cost is
  one-time. A harness child orphaned by a crash just before the upgrade is not
  reaped automatically.
- **Tests that only cover `ps` parsing go with the parser**: `parse_ps_*` in
  `process_tree_tests.rs` and `holder_is_dead_only_on_positive_evidence` in
  `execution_ownership_tests.rs`. Each removal is listed in the report and needs
  Matt's sign-off before a PR. The replacement gets its own focused tests.
- `durable-execution.md` documents "detected with `ps -p <pid>`". Update it in
  the same change.

## Tasks

- [ ] 1. Native probe module plus focused tests: a spawned child has a stable
      start token, is alive, and shows up in the table with the right ppid; a
      reaped pid is gone; a zombie is not alive; pid <= 0 yields `None`/false.
      Delivered in three slices, each 4 files or fewer (router QWEN size gate):
  - [x] A. `os_process::start_token` + `process_identity` (claude/HAIKU after the
        QWEN provider was down; accepted 2026-10-26).
  - [x] B. liveness + zombie: `execution_ownership`, `orphan_sweep` (claude/HAIKU;
        accepted 2026-10-26). Deviation: on macOS `proc_pidinfo` returns no
        record for a zombie, so `is_zombie` reads `p_stat` from
        `sysctl(KERN_PROC_PID)` at offset 36. Offset checked against the LP64
        `extern_proc` layout; a live-vs-zombie test exercises it.
  - [x] C. process table: `process_tree`, `session_client_pid_tests`
        (claude/HAIKU; one revision fixed a racy test, `sh -c 'sleep 30'` execs
        into `sleep`; accepted 2026-10-26).
- [x] 2. Switch the five call sites. Remove `ps` parsing that is no longer used.
      `grep -rn 'Command::new("ps")' crates/ajax-web` now prints nothing.
      Removed tests, pending Matt's sign-off: `parse_ps_reads_pid_ppid_and_the_spaced_command`,
      `parse_ps_keeps_full_command_paths_with_spaces`, `holder_is_dead_only_on_positive_evidence`.
      Added: 6 `os_process` tests, `snapshot_lists_a_spawned_shell_as_a_child_of_this_process`,
      and a ppid assertion in `snapshot_returns_table_containing_this_process`.
- [x] 3. Update `docs/architecture/durable-execution.md` and the matching
      process-table passage in `web-session-behavior.md` (claude/HAIKU; accepted).
- [ ] 4. Gate: `cargo fmt --check`,
      `cargo clippy --all-targets --all-features -- -D warnings`,
      `cargo nextest run -p ajax-web` inside this nono session (it failed
      before; it must now pass apart from documented non-ps environmental
      failures), and `cargo nextest run --workspace`.
- [x] 4. Gate (see Validation results).
- [x] 5. Independent review by a different delegate. Codex (gpt-6-astra) review,
      2026-10-26, found 3 issues, all confirmed and sent to one revision:
      - P1: the Linux token (starttime = ticks since boot) could match an unrelated
        pid after a reboot, which would let the sweep kill it. The old `lstart` was
        an absolute time. Fix: prefix the `/proc/sys/kernel/random/boot_id`.
      - P2: macOS `proc_listallpids` returns 0 on failure, which made an empty table
        `Some`. Fix: treat 0 as `None`.
      - P3: the process_tree test leaked a `sleep` grandchild. Fix: use the `read`
        builtin, and clean up before asserting.
      Fixes were made by claude/HAIKU and checked by the parent: fmt, clippy, and
      73 focused tests three times. A final full `ajax-web` run gave 1346/1348, the
      same 2 pre-existing `logging_*` failures.

## Remaining before a PR

- Matt to sign off on the 3 removed tests (task 2).
- Not committed; nothing pushed.
- CI's Linux jobs must compile and run the `/proc` branches, including
  `live_child_token_starts_with_the_boot_id`.
- Linux tokens move from bare starttime to `boot_id:starttime`, and macOS tokens
  replace `lstart`. Rows recorded by an older binary fail closed: never
  signalled, and never auto-reaped.

## Validation results

Run 2026-10-26 inside a nono-sandboxed session (NONO_CAP_FILE set), at f946e767 plus this change:

- `cargo fmt --all --check`: pass.
- `cargo clippy --workspace --all-targets --all-features -- -D warnings`: pass.
- `cargo nextest run -p ajax-web --no-fail-fast`: 1346/1348 pass. Before this change
  ~147 process-identity tests failed here ("start-time token unreadable", "ps must
  run"). The 2 remaining failures, `runtime::tests::suite_5::logging_*`, are the
  pre-existing sandbox log-file failures recorded on 2026-10-23, in code this
  change does not touch.
- Linux `/proc` branches: not compiled locally (no Linux target installed); CI's
  Linux jobs are the check.
- `cargo nextest run --workspace`: not run inside nono (tmux/ps-dependent tests in
  other crates fail there for unrelated reasons); left to CI.
