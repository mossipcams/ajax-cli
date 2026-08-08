# Web session orchestration chat (ACP-primary)

## Scope

Ajax Web Session behind Settings flag: Cursor-only ACP-primary chat, worktree +
tmux still created, terminal escape hatch only. PTY must not carry chat.

## Non-goals

- Codex / Claude / Pi ACP
- Restoring pre-#701 ACP status replacement across core
- Flag-off behavior changes
- PR until ACP chat path verified

## Decisions

- Cursor only for orchestration chat ACP
- ACP-primary (`session/prompt` / `session/update`); never PTY paste for chat
- Start still creates worktree + tmux; skip interactive Cursor CLI send-keys
- Minimal newline-delimited JSON-RPC ACP stdio client in `ajax-web` (no `agent-client-protocol` crate)
- `Delegation decision: delegated via model-router` for implementation rounds

## Rejected

- Hidden `connectTaskTerminal` composer bridge (PTY-as-chat) — removed

## Checklist

- [x] Architecture docs (ACP required)
- [x] Strip PTY composer bridge
- [x] Start path orchestration_chat + Cursor lock
- [x] ajax-web ACP host + authenticated session WS
- [x] Browser ACP transport + SessionChat wiring
- [x] Focused verification

## Validation

- Delegate: `cargo check -p ajax-web -p ajax-core` — pass
- Delegate: `cargo test -p ajax-web` — pass
- Delegate: core orchestration_chat send-keys skip — pass
- Delegate: `npm run web:check` + focused web tests — pass
- Parent: permission JSON-RPC id fix + Approve/Reject banner in SessionChat
- Parent: `cargo check -p ajax-web -p ajax-core` — pass
- Parent: `cargo test -p ajax-web --lib web_session` — 6 pass
- Parent: `cargo test -p ajax-web --lib orchestration_chat` — 2 pass
- Parent: `cargo test -p ajax-core orchestration_chat_cursor_plan_skips_agent_send_keys` — pass
- Parent: `npm run web:check` — pass
- Parent: focused web tests — 78 pass

PR: blocked until operator smoke with live Cursor ACP (optional follow-up).
