```yaml
PACKET_STATUS: READY
TASK_KIND: behavior
TEST_FIRST: REQUIRED
PRODUCTION_EDIT: REQUIRED
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
```

## Goal

Unstick two operator-visible false positives:

1. Claude idle with a *filled* composer line (`❯` + typed text, including NBSP)
   plus strong chrome must recognize as `IdlePrompt` so sticky `CommandFailed`
   can be overwritten.
2. `CiChecksObservation::Healthy` must clear `SideFlag::TestsFailed` unless the
   live status is a local check failure (`CiFailed` + summary `"check failed"`).

## Allowed files

- `crates/ajax-core/src/live_recognize.rs`
- `crates/ajax-core/src/runtime_refresh.rs`
- `architecture.md`

## Forbidden changes

- No new keyword classification tables or scrollback capture
- Do not make wrapper `running` alone assert `AgentRunning`
- Do not clear local `CiFailed`/`"check failed"` on github Healthy
- Do not clear `TestsFailed` merely because live becomes `AgentRunning`
- No commits, pushes, branch changes, or edits outside Allowed files
- No drive-by cleanup / renames / formatting sweeps

## Context evidence

- Desired behavior: filled Claude composer + chrome → IdlePrompt; Healthy CI →
  drop TestsFailed except local check failure.
  Anchor: `.planning/agent-plans/sticky-status-fp-recovery.md`
- Claude recognizer currently requires bare `❯`/`>`:
  `crates/ajax-core/src/live_recognize.rs:133-154` (`recognize_claude_prompt`)
- Strong chrome helper already exists:
  `crates/ajax-core/src/live_recognize.rs:156-165` (`is_strong_claude_chrome_line`)
- Permission menu already checked first (filled `❯ 1. Yes` + cues → Approval):
  `crates/ajax-core/src/live_recognize.rs:171-198`
- Precedence busy-before-prompt:
  `crates/ajax-core/src/live_recognize.rs:69-85`
- Existing idle/busy tests:
  `crates/ajax-core/src/live_recognize.rs:378-409`
- Healthy arm clears only github live status today:
  `crates/ajax-core/src/runtime_refresh.rs:601-607`
- Prefix/local distinction:
  `GITHUB_CI_FAILED_PREFIX = "ci failed"` at
  `crates/ajax-core/src/runtime_refresh.rs:72`;
  local summary `"check failed"` at
  `crates/ajax-core/src/commands/task_state.rs:50`
- Existing Healthy test (extend, do not weaken):
  `crates/ajax-core/src/runtime_refresh.rs:2365-2390`
  (`github_healthy_checks_clear_only_github_ci_live_status`)
- Architecture wording to update:
  `architecture.md:378-389` (github checks) and
  `architecture.md:436-442` (Claude bare `❯` wording)

## Code anchors

- `fn recognize_claude_prompt` — add composer-line helper; accept
  `starts_with('❯'|'>')` (typed text allowed) when strong chrome present
- `fn apply_github_checks_observation` Healthy arm — `remove_side_flag(TestsFailed)`
  unless live is local check failure
- Doc lines cited above

## Test-first instructions

1. In `live_recognize.rs` tests, add:

```rust
#[test]
fn claude_filled_composer_with_chrome_is_idle() {
    let pane = "done.\n\n────────────────────────────────────────\n❯\u{00a0}watch CI and tell me when it's green\n────────────────────────────────────────\n  Opus 4.8 │ ajax-pwa █░░░░░░░░░ 18%\n  ⏵⏵ bypass permissions on (shift+tab to cycle) · ← for agents\n";
    assert_eq!(hint(AgentClient::Claude, pane), Some(PaneHint::IdlePrompt));
}
```

   Keep existing permission-menu and busy-footer tests green (no edits that
   weaken them).

2. In `runtime_refresh.rs`, extend
   `github_healthy_checks_clear_only_github_ci_live_status` (or add a sibling
   test) so that:
   - github `CiFailed` + `TestsFailed` → Healthy clears live **and** flag
   - `AgentRunning` + `TestsFailed` → Healthy clears flag, keeps `AgentRunning`
   - local `CiFailed`/`"check failed"` + `TestsFailed` → Healthy keeps live and flag

RED command (must fail before production edits):

```bash
cargo nextest run -p ajax-core claude_filled_composer_with_chrome_is_idle github_healthy
```

## Edit instructions

1. `recognize_claude_prompt`: introduce a small `is_claude_composer_line(line)`
   that is true when `line.trim_start()` starts with `❯` or `>` (bare or filled).
   Use it for the chrome+composer idle path (replace bare-only `has_bare_prompt`
   check when paired with strong chrome). Keep last-line bare path.
2. `apply_github_checks_observation` Healthy arm: after existing live clear,
   if live is not local check failure (`CiFailed` && summary == `"check failed"`),
   `task.remove_side_flag(SideFlag::TestsFailed)`.
3. Update the two `architecture.md` anchors to mention filled Claude composer
   and Healthy clearing `TestsFailed` except local check failure.

## Verification commands

```bash
cargo nextest run -p ajax-core claude_filled_composer_with_chrome_is_idle github_healthy
cargo nextest run -p ajax-core live_recognize
cargo clippy -p ajax-core --all-targets -- -D warnings
```

## Acceptance criteria

- Filled Claude composer + chrome → `IdlePrompt`
- Permission menu still `ApprovalPrompt`; busy footer still beats composer
- Healthy clears `TestsFailed` for github-stale and AgentRunning+flag cases
- Healthy preserves local `check failed` live + `TestsFailed`
- Docs match behavior; only Allowed files changed

## Stop conditions

- Need to edit files outside Allowed files
- Would reintroduce keyword tables or change wrapper-running semantics
- Focused tests fail for unrelated reasons after a green attempt
- Patch would exceed ~400 changed lines
