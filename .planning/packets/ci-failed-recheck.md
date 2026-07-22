```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Unstick github `CiFailed` after a fix push: clear it when checks are `Pending`, and re-probe failed CI on a 30s interval instead of waiting the full 300s.

## Allowed files

- `crates/ajax-core/src/runtime_refresh.rs`
- `architecture.md`

## Forbidden changes

- Do not change `GithubChecksAdapter` classification (keep `failure_mixed_with_pending_is_failure`)
- Do not clear local `CiFailed` / summary `"check failed"` on Pending or Healthy
- Do not add git SHA / ahead-based probe invalidation
- No UI/web edits
- No commits, pushes, branch changes, or edits outside Allowed files
- No drive-by cleanup / renames / formatting sweeps

## Context evidence

- Desired behavior: Pending clears sticky github CI failure; while github `CiFailed`, probe every 30s.
  Anchor: `.planning/agent-plans/ci-failed-recheck.md`
- Pending arm currently only removes probe error:
  `crates/ajax-core/src/runtime_refresh.rs:598-600`
- Healthy arm already clears github live + TestsFailed (except local check failure):
  `crates/ajax-core/src/runtime_refresh.rs:584-597`
- Default probe interval is 300s:
  `crates/ajax-core/src/runtime_refresh.rs:71` (`CI_CHECKS_PROBE_INTERVAL`)
  and `should_probe_github_checks` at `:543-562`
- Helpers: `is_github_ci_failure` `:618-620`, `is_local_check_failure` `:622-624`
- Existing Healthy test to mirror for Pending:
  `github_healthy_checks_clear_only_github_ci_live_status` `:2273-2310`
- Existing probe-interval test pattern (CiChecksRunner + metadata stamp):
  `github_ci_probe_reuses_fresh_timestamp_and_refreshes_stale_timestamp` `:2352-2393`
- Architecture wording to update:
  `architecture.md:404-416` (github checks probe / clear behavior)

## Code anchors

- `const CI_CHECKS_PROBE_INTERVAL` — add sibling `CI_CHECKS_FAILED_PROBE_INTERVAL = 30s`
- `fn should_probe_github_checks` — choose 30s when live is github CI failure, else 300s
- `fn apply_github_checks_observation` Pending arm — same clear path as Healthy for github live + TestsFailed
- Extract a tiny shared helper from Healthy/Pending if that shrinks the diff
- Docs at `architecture.md:409-412`

## Test-first instructions

1. Add `github_pending_checks_clear_github_ci_live_status` next to the Healthy sibling:

```rust
#[test]
fn github_pending_checks_clear_github_ci_live_status() {
    let now = SystemTime::now();
    let mut github = task_with_live(LiveStatusKind::CiFailed, "ci failed: ci");
    github.add_side_flag(SideFlag::TestsFailed);
    let mut local = task_with_live(LiveStatusKind::CiFailed, "check failed");
    local.add_side_flag(SideFlag::TestsFailed);
    let mut conflict = task_with_live(LiveStatusKind::MergeConflict, "merge failed");

    super::apply_github_checks_observation(&mut github, CiChecksObservation::Pending, now);
    super::apply_github_checks_observation(&mut local, CiChecksObservation::Pending, now);
    super::apply_github_checks_observation(&mut conflict, CiChecksObservation::Pending, now);

    assert!(github.live_status.is_none());
    assert!(!github.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        local
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::CiFailed, "check failed"))
    );
    assert!(local.has_side_flag(SideFlag::TestsFailed));
    assert_eq!(
        conflict
            .live_status
            .as_ref()
            .map(|status| (status.kind, status.summary.as_str())),
        Some((LiveStatusKind::MergeConflict, "merge failed"))
    );
}
```

2. Add `github_ci_failure_reprobes_sooner_than_default_interval`:

```rust
#[test]
fn github_ci_failure_reprobes_sooner_than_default_interval() {
    let now = unix_seconds_for_test(SystemTime::now());
    let failed_stdout = ci_failed_stdout("ci");

    let mut failed_context = context_with_active_task();
    {
        let task = failed_context
            .registry
            .get_task_mut(&TaskId::new(TASK_ID))
            .unwrap();
        task.live_status = Some(LiveObservation::new(
            LiveStatusKind::CiFailed,
            "ci failed: ci",
        ));
        task.metadata
            .insert("ci_checks_probed_at".to_string(), (now - 31).to_string());
    }
    let mut failed_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);
    refresh_runtime_context_with_tier(
        &mut failed_context,
        &mut failed_runner,
        &NoAgentStatusCache,
        RefreshTier::Full,
    )
    .unwrap();
    assert_eq!(failed_runner.gh_command_count(), 1);

    let mut healthy_context = context_with_active_task();
    healthy_context
        .registry
        .get_task_mut(&TaskId::new(TASK_ID))
        .unwrap()
        .metadata
        .insert("ci_checks_probed_at".to_string(), (now - 31).to_string());
    let mut healthy_runner = CiChecksRunner::with_gh(&failed_stdout, "", 1);
    refresh_runtime_context_with_tier(
        &mut healthy_context,
        &mut healthy_runner,
        &NoAgentStatusCache,
        RefreshTier::Full,
    )
    .unwrap();
    assert_eq!(healthy_runner.gh_command_count(), 0);
}
```

RED command (must fail before production edits):

```bash
cargo nextest run -p ajax-core github_pending_checks_clear github_ci_failure_reprobes
```

## Edit instructions

1. Add `CI_CHECKS_FAILED_PROBE_INTERVAL: Duration = Duration::from_secs(30)`.
2. In `should_probe_github_checks`, after resolving `probed_at`, use
   `CI_CHECKS_FAILED_PROBE_INTERVAL` when `task.live_status.as_ref().is_some_and(is_github_ci_failure)`,
   else `CI_CHECKS_PROBE_INTERVAL`.
3. Make `Pending` apply the same clear logic as `Healthy` (github live clear + TestsFailed unless local check failure). Prefer a small shared helper called from both arms.
4. Update `architecture.md:409-412` to say: Pending and Healthy clear GitHub-sourced CI evidence / TestsFailed (except local check failure); while github `CiFailed`, probes use a 30-second interval; otherwise 300 seconds.

## Verification commands

```bash
cargo nextest run -p ajax-core github_pending_checks_clear github_ci_failure_reprobes github_healthy
cargo nextest run -p ajax-core runtime_refresh
cargo clippy -p ajax-core --all-targets -- -D warnings
```

## Acceptance criteria

- Pending clears github `CiFailed` + `TestsFailed`
- Pending preserves local `check failed` + `TestsFailed` and unrelated live status
- With github `CiFailed` and probed_at ≈ now−31s, refresh issues `gh`
- Without CI failure and probed_at ≈ now−31s, refresh skips `gh`
- Docs match; only Allowed files changed

## Stop conditions

- Need to edit files outside Allowed files
- Would change adapter classification or local-check semantics
- Focused tests fail for unrelated reasons after a green attempt
- Patch would exceed ~400 changed lines
