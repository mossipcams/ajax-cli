# Commands and Adapters

Command plan helpers versus mechanism adapters.

`commands/*` are substrate-oriented plan helpers (not the shared kernel).
Operator slices may call them; they must not import operator slices except
the thin `execute_plan` compatibility wrapper into `task_operations::kernel`.

## Command Architecture


Command planning and command execution are separate.

`ajax-core::commands` builds command plans and typed command responses.

`CommandSpec` describes external commands. `CommandRunner` executes them through
capture or inherited-stdio modes.

Command modules are split by use case:

- `commands/doctor.rs`
- `commands/check.rs`
- `commands/diff.rs`
- `commands/merge.rs`
- `commands/new_task.rs` — `NewTaskRequest.agent_start` is `AgentStartMode`:
  `InteractiveCli` (default) plans worktree, detached tmux, and agent send-keys;
  `PreparedSession` plans the same worktree and tmux session but skips send-keys
  so a different conversation host can attach. Presentation flags such as HTTP
  `orchestration_chat` stay in `ajax-web`; they must not appear on
  `NewTaskRequest`.
- `commands/open.rs`
- `commands/projection.rs`
- `commands/teardown.rs`
- `commands/task_window.rs`
- `commands/lookup.rs`

The public CLI vocabulary is operator-facing: `start`, `resume`, `repair`,
`review`, `ship`, `drop`, `tidy`, and `ready`. Some internal command modules
still carry substrate-oriented names where they wrap the underlying git, tmux,
or test-command operation.

Runtime profile names such as `stable` and `dev` are runtime selections, not
task-operation commands or separate operator domains.

## Adapter Architecture


`ajax-core::adapters` is the adapter facade.

- `adapters/command.rs` defines command specs and the command-runner port.
- `adapters/github.rs` observes PR checks and Diff Review PR list / patch
  payloads through `gh` (browser never runs or parses these itself).
- `adapters/process.rs` executes subprocesses.
- `adapters/git.rs` builds and parses Git commands.
- `adapters/tmux.rs` builds and parses tmux commands.
- `adapters/agent.rs` builds and parses agent commands.
- `adapters/environment.rs` probes operator environment facts.
