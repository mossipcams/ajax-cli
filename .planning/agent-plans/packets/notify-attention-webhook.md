# TDD Packet: attention notification transitions + curl webhook spec (T1+T2)

## 1. Goal

When a task's derived operator status crosses INTO `Waiting` or `Error` (rising
edge), produce a typed `AttentionTransition` exactly once, and build a `curl`
`CommandSpec` that posts a one-line message to a webhook URL. This packet covers
detection (T1, ajax-core) and spec building (T2, ajax-cli) ONLY. Config plumbing
and wiring into refresh callers is a later round — do NOT wire anything.

## 2. Allowed files

Production:
- `crates/ajax-core/src/notify.rs` (new)
- `crates/ajax-core/src/lib.rs` (only: add `pub mod notify;` in alphabetical position among existing `pub mod` lines)
- `crates/ajax-cli/src/notify.rs` (new)
- `crates/ajax-cli/src/lib.rs` (only: add `mod notify;` among existing mod declarations)

Tests: inline `#[cfg(test)] mod tests` inside each new `notify.rs`. No other
test files.

## 3. Forbidden changes

- Do not modify `ui_state.rs`, `derive_operator_status`, or any status logic.
- Do not modify `registry.rs`, `registry/sqlite.rs`, schema, or migrations.
- Do not modify `runtime_refresh.rs`, `cockpit_backend.rs`, `web_backend.rs`,
  `config.rs` (wiring/config is a later round).
- Do not touch `crates/ajax-cli/tests/`, `crates/ajax-web/`, anything under
  `web/` (snapshot tests trip), or `crates/ajax-supervisor/`.
- No new dependencies in any Cargo.toml.
- No renames, no formatting sweeps, no refactors, no doc rewrites.
- Never commit, push, branch, or change branches.

## 4. Architecture context

- `ajax-core` is substrate/OS-agnostic: it computes projections; it must not
  build `curl` commands. Detection (T1) lives in core; the curl spec builder
  (T2) lives in the `ajax-cli` shell. Dependency direction: ajax-cli → ajax-core.
- `Task.metadata: HashMap<String, String>` (`models.rs:250`) is persisted by the
  registry snapshot save; mutating it on a `&mut Task` is the correct
  persistence path here. Do not add new persistence.
- Status truth is `ui_state::derive_operator_status(&Task) -> OperatorStatus
  { status: TaskStatus, explanation: Option<String> }`; `TaskStatus` is
  `Running | Waiting | Idle | Error` with `as_str()` → "Running"/"Waiting"/
  "Idle"/"Error" (`ui_state.rs:6-22`).

## 5. Code anchors

- `crates/ajax-core/src/ui_state.rs:30` `pub fn derive_operator_status(task: &Task) -> OperatorStatus`
- `crates/ajax-core/src/ui_state.rs:6` `pub enum TaskStatus` (has `as_str`)
- `crates/ajax-core/src/models.rs:250` `pub metadata: HashMap<String, String>`
- `crates/ajax-core/src/adapters/command.rs:5`
  `pub struct CommandSpec { pub program: String, pub args: Vec<String>, pub cwd: Option<PathBuf>, pub mode: CommandMode, pub timeout: Option<Duration> }`
- `crates/ajax-core/src/adapters/command.rs` `CommandSpec::new(program, [args])`
  (const-generic `[&str; N]`, sets `mode: CommandMode::Capture`, no timeout) and
  builder `with_timeout(Duration)`.
- Test fixture pattern to copy (do not import across crates): `base_task()` in
  `ui_state.rs:230-243` constructs `Task::new(TaskId::new("task-1"), "web",
  "fix-login", "Fix login", "ajax/fix-login", "main",
  "/tmp/worktrees/web-fix-login", "ajax-web-fix-login", "task",
  AgentClient::Codex)`.
- Ways to force statuses in tests (from existing ui_state tests):
  - Waiting: `crate::lifecycle::mark_active(&mut task).unwrap(); task.add_side_flag(SideFlag::NeedsInput);`
  - Error: `task.add_side_flag(SideFlag::Conflicted);` (after mark_active)
  - Running: `task.agent_status = AgentRuntimeStatus::Running; task.add_side_flag(SideFlag::AgentRunning);`
  - Idle: `mark_active` only.

## 6. Test-first instructions

### T1 — `crates/ajax-core/src/notify.rs` tests (write first, must fail to compile/run before impl)

Test names and assertions (use the fixture recipes above):
1. `idle_to_waiting_fires_once`: task forced Waiting, empty metadata →
   `take_attention_transition(&mut task)` returns
   `Some(AttentionTransition { status: TaskStatus::Waiting, .. })` with
   `explanation == Some("Waiting for input".to_string())`; calling it again
   immediately returns `None`.
2. `waiting_then_idle_then_waiting_fires_again`: force Waiting, take (Some);
   remove the side flag / restore Idle (`task.remove_side_flag(SideFlag::NeedsInput)`
   if that helper exists — otherwise clear via the same mechanism the flag was
   added; check `models.rs` for the side-flag remove API and use it), take
   (None, and metadata key cleared); force Waiting again, take → Some.
3. `waiting_to_error_fires`: force Waiting, take (Some); additionally force
   Error (add `SideFlag::Conflicted`), take → Some with `TaskStatus::Error`.
4. `running_and_idle_never_fire`: force Running → take is None and metadata
   key absent; Idle likewise.
5. `transition_carries_repo_and_handle`: fired transition has
   `repo == "web"`, `handle == "fix-login"`.

Failing command before implementation: `cargo test -p ajax-core notify` (fails
to compile — module doesn't exist yet; that counts as the red step).

### T2 — `crates/ajax-cli/src/notify.rs` tests

1. `webhook_spec_shape`: for
   `AttentionTransition { repo: "web", handle: "fix-login", status: TaskStatus::Waiting, explanation: Some("Waiting for input") }`
   and url `"https://ntfy.sh/topic"`, `webhook_command(url, &transition)`
   returns spec with `program == "curl"`,
   `args == ["-s", "--max-time", "10", "-d", "web/fix-login: Waiting — Waiting for input", "https://ntfy.sh/topic"]`,
   `mode == CommandMode::Capture`, `timeout == Some(Duration::from_secs(10))`.
2. `webhook_spec_without_explanation`: explanation `None` → body is
   `"web/fix-login: Error"` (no dash suffix).

Failing command: `cargo test -p ajax-cli notify`.

## 7. Production edit instructions

### T1 `crates/ajax-core/src/notify.rs`

```rust
use crate::models::Task;
use crate::ui_state::{derive_operator_status, TaskStatus};

pub const LAST_NOTIFIED_STATUS_KEY: &str = "last_notified_status";

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttentionTransition {
    pub repo: String,
    pub handle: String,
    pub status: TaskStatus,
    pub explanation: Option<String>,
}

/// Rising-edge detector. Mutates the task's metadata stamp.
/// ponytail: best-effort dedup via metadata stamp saved with the normal
/// registry snapshot; per-key CAS only if duplicate pings ever annoy.
pub fn take_attention_transition(task: &mut Task) -> Option<AttentionTransition> { ... }
```

Logic: derive status; if `Waiting`/`Error`: compare `as_str()` against
`task.metadata.get(LAST_NOTIFIED_STATUS_KEY)`; if different, insert the new
value and return `Some(transition)` (clone `task.repo`/`task.handle` — check
`models.rs` for exact field names on `Task` and use them); if equal → `None`.
If `Running`/`Idle`: `task.metadata.remove(LAST_NOTIFIED_STATUS_KEY)`, return
`None`.

Register `pub mod notify;` in `crates/ajax-core/src/lib.rs`.

### T2 `crates/ajax-cli/src/notify.rs`

```rust
use ajax_core::adapters::CommandSpec; // check actual re-export path used elsewhere in ajax-cli and match it
use ajax_core::notify::AttentionTransition;
use std::time::Duration;

pub(crate) fn webhook_command(webhook_url: &str, transition: &AttentionTransition) -> CommandSpec { ... }
```

Body format: `"{repo}/{handle}: {status}"` plus `" — {explanation}"` when
present (em dash, exactly as in the tests). Build with
`CommandSpec::new("curl", [])` then assign
`spec.args = vec![...]` (args field is pub), or construct the struct literally
— match whichever style existing ajax-cli code uses for CommandSpec; then
`.with_timeout(Duration::from_secs(10))`. Register `mod notify;` in
`crates/ajax-cli/src/lib.rs`.

## 8. Verification commands

```bash
cargo test -p ajax-core notify
cargo test -p ajax-cli notify
cargo test -p ajax-core -p ajax-cli
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

## 9. Acceptance criteria

- Red step observed: notify tests fail (or fail to compile) before production code.
- All 7 tests above pass afterward.
- Full `cargo test -p ajax-core -p ajax-cli`, clippy `-D warnings`, and
  `fmt --check` pass.
- Diff touches only the four allowed files.
- No config, wiring, registry, or dependency changes.

## 10. Stop conditions

Stop and report instead of guessing when:
- `Task` lacks accessible `repo`/`handle` string fields, or side-flag removal
  has no public API (report the actual API you found).
- `CommandSpec`/`CommandMode` are not importable from ajax-cli via an existing
  path (report the paths you tried).
- Any test outside `notify` breaks.
- The change would exceed ~400 changed lines or require editing a forbidden file.
- A notify test passes BEFORE production code exists (means the test is wrong).
