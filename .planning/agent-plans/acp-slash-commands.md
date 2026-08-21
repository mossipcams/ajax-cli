# ACP slash-command pass-through and tab completion

**Status:** complete — slice 1 of `.planning/agent-plans/acp-utilization.md`
**Approval:** operator requested uninterrupted implementation of slices 1–6 (2026-08-21)
**Branch:** `ajax/acp-extensions`
**Protocol:** stable ACP v1 (`available_commands_update` + `session/prompt` text)
**Plan:** `.planning/agent-plans/acp-slash-commands.md`

## Problem

Ajax Chat speaks ACP but drops most of what the protocol advertises. The first
gap an operator hits is slash commands.

ACP ([slash commands v1](https://agentclientprotocol.com/protocol/v1/slash-commands)):

- After `session/new` (and any time later) the agent **MAY** send
  `session/update` with `sessionUpdate: "available_commands_update"` and a
  complete `availableCommands` list.
- Each command has `name`, `description`, and optional `input` (v1: `{ hint }`).
- The client executes a command by sending ordinary `session/prompt` text
  beginning with `/name` (optional args after a space). There is no separate
  RPC.
- A later `available_commands_update` **replaces** the list; it is not a merge.

Ajax today:

- `acp_map` and the drain path treat `available_commands_update` as a
  capability announcement with nothing an operator can act on and drop it.
  The existing test `capability_announcements_are_dropped` encodes that.
- The composer already sends typed text as `session/prompt`, so `/web query`
  would pass through if the operator typed the full name. There is no
  advertisement, no completion, and no guarantee Ajax will not grow a competing
  local slash parser.
- Web Cockpit targets iOS Safari: there is often no Tab key, so completion must
  be a tappable list as well as Tab on a hardware keyboard.

## What ACP offers (inventory, not this slice)

Stable ACP v1, from the official schema and protocol docs. Ajax already uses
the marked rows.

| Surface | Ajax today |
| --- | --- |
| `initialize` / protocol v1 / client info | yes |
| `session/new`, `session/resume`, `session/load` | yes |
| `session/prompt`, `session/cancel` | yes |
| `session/set_config_option` + `config_option_update` | yes (live snapshot, not transcript) |
| message / thought chunks, tool calls, diffs, plan, usage | yes |
| `session/request_permission` | auto-approved on host |
| **`available_commands_update` + `/name` in prompt text** | **dropped / accidental** |
| `current_mode_update` | dropped (superseded by config `mode`) |
| `session_info_update` | artifact blob |
| Client `fs/*` (read/write text file) | advertised false |
| Client `terminal/*` | advertised false |
| Prompt image / audio / embedded context | not advertised |
| MCP HTTP/SSE | not advertised |
| `authenticate` / `logout` | unused (trusted local) |

Later slices (explicitly out of this plan): filesystem and terminal client
capabilities, rich prompt content, MCP, a permission UI instead of auto-approve,
and ACP v2.

## Goal

Keep advertised ACP slash commands as live session capability state (same
ownership pattern as `sessionConfigOptions`), complete them in the Ajax Chat
composer, and send `/name` plus args unchanged on `session/prompt`.

## Non-goals

- ACP v2 negotiation or v2 `input.type` discriminators as a requirement (v1
  `{ hint }` is enough; ignore unknown input objects for UX, keep the command).
- Ajax-owned slash commands (`/help` from Ajax, Cockpit actions, harness switch).
- Persisting the command list in JSONL (not conversation; reconnect uses the
  live slot, and the agent re-advertises after spawn/resume).
- Filesystem, terminal, MCP, image/audio prompts, or permission UI.
- Changing protocol v2 version number. Add an optional snapshot field.

## Contract

1. **Host capture.** Intercept typed `SessionUpdate::AvailableCommandsUpdate`
   the same way `ConfigOptionUpdate` is intercepted: it is applied state, not a
   transcript row. Replace the stored list. Do not append it to JSONL.
2. **Snapshot.** Protocol v2 `snapshot` carries optional `availableCommands`:
   `{ name, description, inputHint? }[]`. Replace the list; do not merge.
   Omit or send `[]` when none are advertised.
3. **Live refresh.** A connected browser must see a replacement without
   reconnect. Today snapshots republish mainly on generation change or
   `pending_model_snapshot`; command updates must republish even when the model
   did not change. Handshake drain must not drop an advertisement that arrives
   before the first attach snapshot.
4. **Pass-through.** Composer submit of text starting with `/` is
   `session/prompt` with that exact string. Ajax must not intercept, rewrite,
   or implement local slash handlers in this slice.
5. **Completion.** While the first token is `/` plus an optional name prefix
   (no whitespace yet) and the session has advertised commands:
   - show a filtered list (name prefix, case-insensitive);
   - hardware Tab inserts the selected command (`/name` plus a trailing space
     when `inputHint` is present) and does not submit;
   - ArrowUp/Down move the selection; Enter with the menu open inserts, does
     not submit;
   - tap/click a row inserts (required on iOS Safari);
   - after a space (args) or when the list is empty, hide the menu;
   - unadvertised `/foo` still submits as plain prompt text.
6. **Ownership.** Browser reducer holds the latest advertised list from
   snapshot (and any live snapshot refresh). It does not invent commands.
   Chat composer owns the menu. Workspace/task chrome does not.

## Implementation checklist

- [x] Task 1 — Host: capture `available_commands_update` as live state
  - Test: fake-agent / drain / map tests proving the list is stored and is
    **not** a JSONL transcript event; `current_mode_update` stays dropped.
    Split or replace `capability_announcements_are_dropped` so commands are
    asserted as live state rather than "nothing."
  - Code: adapter event + drain outcome + `TaskSessionState` storage; replace
    on each update.
  - Verify: focused `ajax-web` Rust tests for map/drain/session snapshot.

- [x] Task 2 — Protocol v2 snapshot + live republish
  - Test: attach snapshot includes `availableCommands`; a later replacement
    publishes a non-reset snapshot to the open socket; reconnect without a
    live advertisement yields omit/`[]`.
  - Code: optional snapshot field; pending-commands snapshot path that does
    not require a model change.
  - Verify: `ws_bridge` / `task_session` / protocol tests.

- [x] Task 3 — Browser: parse, hold, complete, pass through
  - Test: snapshot parse; prefix filter / insert helpers; composer Tab, arrows,
    Enter-inserts-not-submit, tap; submit of `/web query` calls `sendPrompt`
    with that exact string.
  - Code: transport contracts/parse, session view, composer menu (tappable),
    Chat-owned CSS. Keep `ChatComposer.tsx` thin; put matching in a pure
    helper. Wire advertised commands through existing composer context.
  - Verify: focused Vitest under `features/chat`.

- [x] Task 4 — Architecture docs
  - Update `docs/architecture/web-session-behavior.md` and
    `docs/architecture/web-cockpit.md`: live `availableCommands`, not
    transcript; pass-through `session/prompt`; composer completion including
    touch.
  - Verify: docs-only review against the shipped contract.

## Validation

```bash
cargo test -p ajax-web acp_map drain snapshot available_command -- --nocapture
# plus the focused names the implementer actually adds
cd crates/ajax-web/web && npx vitest run src/features/chat
```

Broader `ajax-web` tests if the focused set is green and the delta touches
spawn/handshake.

## Stop conditions

Stop and ask before: adding Ajax-local slash commands; persisting commands as
transcript; bumping the WebSocket protocol version; advertising filesystem or
terminal capabilities; changing auto-approve permissions.
