# Multi-harness ACP for tasks started from the New Task sheet

**Status:** implemented on this branch (uncommitted); operator direction given in chat 2026-08-14
**Branch:** `ajax/older-task-create`
**Related:** web session ACP work (#863–#872), model selection in the New Task
sheet (uncommitted on this branch)

## Goal

A task started from the old New Task sheet maps to *its own harness's* ACP the
way a Cursor task already does, so orchestration chat is not a Cursor-only
surface.

## Verified substrate (this machine, 2026-08-14)

Each bridge was spawned and sent a real ACP `initialize`; all answer
`protocolVersion: 1`.

| Agent | ACP entry point | loadSession | Model selection |
|---|---|---|---|
| cursor | `agent acp` / `cursor agent acp` (native) | yes | `--model` pinned at spawn (today's respawn-to-switch) |
| codex | `codex-acp` (`@agentclientprotocol/codex-acp` 1.1.7) | yes | in-band `session/set_model` |
| claude | `claude-agent-acp` (`@agentclientprotocol/claude-agent-acp` 0.63.0) | yes | none advertised |
| pi | `pi-acp` (`pi-acp` 0.0.32) | yes | in-band `setSessionModel`; terminal-login auth method |

None of the three bridges accept `--model` on argv. Pinning the model at spawn
is a Cursor-specific mechanism, so the launch map must say per harness whether
the model pins at spawn or switches in-band.

The bridges are global npm binaries, not guaranteed present on every host.

## Scope

1. **Core owns the harness → ACP mapping.** `acp_launch_for_agent(AgentClient)
   -> Option<AcpLaunch>` beside `agent_launch_spec` in
   `ajax-core/src/adapters/agent.rs`: program candidates, base args, and a
   `model_pins_at_spawn` bit. Cursor keeps its existing two candidates.
2. **Provisioned launch stops being Cursor-only.** `new_task.rs` currently ands
   `skip_interactive_agent` with `AgentClient::Cursor` (plan + task metadata).
   Any agent with an ACP launch may start provisioned; an agent without one
   keeps tmux send-keys.
3. **Admission by capability, not by name.**
   `web_session::prepare_task_session` admits when the task's agent has an ACP
   launch **and** carries the provisioned bit. Interactive (send-keys) tasks keep
   returning 409 `NotOrchestrationChat`.
4. **Spawn from the map.** `web_session_acp::client::spawn_cursor_acp_process`
   becomes agent-driven; `--model` is inserted only when the map says the harness
   pins at spawn.
5. **Missing bridge is an operator error, not a spawn failure.** Typed error
   ("ACP bridge not installed for <agent>: install <package>") plus an
   `ajax doctor` entry.
6. **Model catalog per agent** (phase 2, shippable after 1–5):
   `/api/session/models?agent=` — Cursor keeps `agent models`; codex/pi report
   what the ACP session advertises and switch in-band; claude reports
   default-only. Until then, non-Cursor agents show no model picker.
7. **UI**: SessionStarter's hard-coded "Cursor" pill becomes the task's agent.
   The New Task sheet keeps the model picker for harnesses with a catalog.
8. **Docs in the same change**: `docs/architecture/web-session-behavior.md`
   (Launch, model switching) and `docs/architecture/web-cockpit.md`.

## Non-goals

- Attaching ACP to a task whose agent is already live in tmux (see open item).
- Making provisioned launch the default with the orchestration-chat flag off.
- Vendoring or auto-installing the ACP bridges.
- A second registry or session truth in the browser.

## Resolved scope

Direction given in chat: use each harness's ACP **when creating a task**, put the
model choice on a second page of the New task sheet, and allow a harness swap
after creation from the swipe-left (Diff Review) page.

Legacy interactive tasks still return 409 and cannot swap harness: an ACP child on
that worktree would be a second agent alongside the live tmux one. A "stop the
tmux agent, then attach" operation remains unbuilt.

## Task checklist

- [x] Core ACP launch map + unit tests per agent
- [x] Provisioned launch un-gated from Cursor (plan + task metadata) + tests
- [x] Admission policy by ACP capability + slice tests (incl. 409 for interactive)
- [x] Adapter spawns from the map; model pin only where supported
- [x] Missing-bridge error carries the install hint
- [ ] `ajax doctor` entry for missing ACP bridges — not built
- [x] Per-agent model catalog (`?agent=`) + parser tests for both ACP shapes
- [x] Two-step New task sheet (harness → model page) + unit and e2e tests
- [x] Model stored on the task and used at session attach
- [x] Harness swap after creation (core command, `POST /api/tasks/{handle}`,
      Diff Review panel) + tests at each layer
- [ ] SessionStarter agent pill still hard-codes Cursor (that surface is the
      flag-gated starter, not the New task sheet)
- [x] Contract docs updated in the same change

## Validation commands

`cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`,
`cargo nextest run --all-features --test-threads=1`, `cargo test --doc`,
`npm run web:check`, `npm run web:lint`, `npm run web:sg`,
`npm run web:test -- --run`, `CI=1 npm run web:smoke`, plus a live check against
each installed bridge (`codex-acp`, `claude-agent-acp`, `pi-acp`).

## Results

All gates green on 2026-08-14: `cargo fmt --check`, `cargo clippy --all-targets
--all-features` (0 warnings), `cargo nextest run --all-features
--test-threads=1` (1954 passed), `cargo test --doc`, rustdoc `-D warnings`,
`npm run web:check|web:lint|web:sg`, `npm run web:test -- --run` (838 passed),
`CI=1 npm run web:smoke` (114 passed, 1 flaky retry in terminal-behavior).

Verified against the real bridges on this machine: `codex-acp`,
`claude-agent-acp`, and `pi-acp` all answer ACP `initialize` and advertise model
catalogs on `session/new`.
