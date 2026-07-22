# Plan: Cursor hooks resolve Ajax identity (status detection)

## Diagnosis (2026-07-21)

Previous `fix-cursor-status-hooks` made `Cursor`/`Pi` first-class and gated
stale writes on runtime liveness. **Status still wrong because events never
land.**

Live evidence:

1. Cursor hooks fire (`ajax-cli __agent-event --client cursor --event
   postToolUse` visible in process list).
2. `~/.ajax-dev/cache/agent-events/` has **zero** event files (only
   `notify.sock`).
3. `__agent-event` no-ops without `AJAX_TASK_ID` + `AJAX_AGENT_EVENTS_DIR`
   (confirmed by probe).
4. Live Cursor `agent` process env has `AJAX_PROFILE=dev` only — no task
   identity. Cursor docs: session hook env comes from `sessionStart`
   `{"env":{...}}`, not reliably from the agent child process.
5. Lifecycle ingestion does **not** ignore `Other`; empty events is the
   blocker. (Existing `Other` rows still ignore *legacy Hook* files.)

## Scope

- Publish a worktree/cwd → identity index from `__agent-runtime`
- Cursor `__agent-event`: if env identity missing, resolve via
  `CURSOR_PROJECT_DIR` / stdin `workspace_roots[0]` + index
- Cursor `sessionStart`: when identity resolves, print
  `{"env":{AJAX_TASK_ID,AJAX_RUN_ID,AJAX_AGENT_EVENTS_DIR}}` so later hooks
  inherit the Cursor-documented session env
- Keep `runtime_hooks_accepted` gate (no ambient IDE pollution)

## Non-goals

- Pane chrome recognizers for Cursor
- SQLite Other→Cursor/Pi migration (separate; Lifecycle path already accepts)
- Notify / Response-ready policy changes
- Commits, pushes, branch changes

## Delegation decision

`Delegation decision: delegated via model-router`

```yaml
ROUTING_DECISION:
  ACTION: DELEGATE
  LANE: pi-delegate
  MODE: implement
  MODEL: opencode-go/glm-5.2
  PACKET_STATUS: READY
  PACKET_REBUILD_COUNT: 0
  PACKET_CRITIQUE_COUNT: NONE
  ALLOWED_SCOPE:
    - crates/ajax-cli/src/agent_runtime.rs
    - crates/ajax-cli/src/agent_event.rs
    - architecture.md
  REASON: Session/hook identity behavior change with READY packet; backend risk → GLM.
  ESCALATE_IF:
    - GLM usage limit
    - pi tool unavailable
    - scope creep outside allowed files
```

## Task checklist

- [x] Failing tests: cwd-index publish; cursor event without env but with
      CURSOR_PROJECT_DIR writes; sessionStart stdout carries env JSON; missing
      index still no-ops; exited runtime still rejected
- [x] Implement identity index write/clear in `agent_runtime`
- [x] Implement Cursor identity fallback + sessionStart env stdout in
      `agent_event`
- [x] Parent review + validation

## Validation results

Parent (2026-07-21):

```text
cargo test -p ajax-cli agent_event → 21 passed
cargo test -p ajax-cli agent_runtime → 6 passed
cargo clippy -p ajax-cli --all-targets -- -D warnings → clean
cargo install --path crates/ajax-cli --locked --force → installed
```

## Deviations

- GLM weekly limit → escalated to cursor-delegate `composer-2.5`
- Delegate report envelope invalid (`TEST_FIRST: NOT_PROVEN`); parent reviewed
  delta and re-ran tests
- Parent: do **not** clear cwd-index on exit (keep through settle grace);
  canonicalize cwd for index stem match; architecture.md one-sentence note

