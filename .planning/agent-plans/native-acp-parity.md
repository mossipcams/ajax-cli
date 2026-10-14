# Native transports: feature parity with ACP on Ajax web

## Status

APPROVED 2026-10-13 — Matt: "Implement until finished." Implementing via
model-router (Qwen, small packets). Branch `ajax/native-harness-durability`.
Order: Cursor SDK transport first (`cursor-native-sdk.md` tasks 2–6), then the
shared-layer gaps below, then durability stages 3, 5, 8, 12.

## Request (Matt, 2026-10-13)

"The native implementations should have feature parity with ACP for each
native transport on web."

Read as: for every harness that runs on a native transport (Claude Agent SDK,
Pi RPC, and Cursor SDK once it lands), anything Ajax web can do over ACP must
also work over the native transport, unless the harness itself cannot do it.
Codex stays on ACP and is the reference.

## Scope

- The shared native layer (`NativeEvent`, `SessionClient`, `native_map`) and
  the three native clients / sidecars.
- A parity guard so a new gap cannot land silently.

## Non-goals

- Changing ACP behaviour or the wire events the browser consumes.
- Terminal / tmux work (parked in the durability plan).
- Inventing a feature a harness does not have. Such a cell is recorded as
  "harness cannot" with evidence, not papered over.

## Gaps confirmed from source (2026-10-13, `964f945f`)

| # | ACP on web | Native today | Evidence |
| --- | --- | --- | --- |
| 1 | Image and embedded-context prompt blocks | Flattened to text; images become the literal `[image omitted]` | `session_client.rs` `prompt_text` (`ponytail: images are not forwarded yet`) |
| 2 | Permission prompts answered from the browser | Pi: none. Claude: auto-approved in the sidecar | `SessionClient::respond_client_request` returns `Err` for Pi and Claude |
| 3 | Form elicitation | Pi: none. Claude: declined in the sidecar | `SessionClient::respond_elicitation` returns `Err` for Pi and Claude |
| 4 | Cancel returns the permissions / elicitations it resolved | Always empty for native | `SessionClient::cancel` |
| 5 | Session title (`SessionInfoUpdated`) | No `NativeEvent` variant | `agent_event.rs` |
| 6 | Config options are open-ended (`apply_config_option` passes any id) | Claude: `model`, `effort` only. Pi: `model`, `thought_level` only; anything else is "unsupported … option" | `SessionClient::apply_config_option` |
| 7 | Tool-call content: text, diffs, terminal (`map_tool_content`) | Text only (`native_tool_content`); no diff or terminal content | `native_map.rs` |
| 8 | Messages carry content blocks and a message id | Plain text, no blocks, no id | `native_map.rs` `native_message` |
| 9 | Slash commands | Done for Claude and Pi (#1288) | `take_available_commands` |

## Not yet checked (audit before ordering work)

Each needs the ACP behaviour read from `acp_map` / `web-session-behavior.md`
and the native side read from the client, map and harness docs:

- Session modes (`session/set_mode`: agent / plan / ask) and the approval
  policy that drives them.
- Tool-call detail: diffs, file locations, terminal output. `NativeEvent`
  carries only `content: Option<String>` on an update.
- Usage: ACP has per-turn token usage and a context meter; `NativeEvent` has
  one `Usage { used, size }`.
- Restore: ACP `session/resume` vs `session/load` with transcript replay,
  typed `Unsupported` / `Rejected` failures, and `session/close`.
- Typed authentication errors and the reconnect dedupe that depends on them.
- Queue, steer and mid-turn follow-up behaviour.
- Which of gaps 1–6 each harness can actually satisfy (e.g. does Pi RPC accept
  images or ask for permission at all).

## Cursor SDK (in progress, see `cursor-native-sdk.md`)

Cursor must not regress when it leaves ACP. Known from the spike and the SDK
types: images (`send({ text, images })`), modes (`SendOptions.mode`), steer,
cancel, per-turn usage and resume exist. Not yet in the sidecar packet: images
and modes. Permission prompts and elicitation over the SDK: unknown.

## Tasks

- [ ] 1. Audit: fill a matrix of web feature × {Codex ACP (reference), Claude
      SDK, Pi RPC, Cursor SDK} with one of: parity / gap / harness cannot, each
      with a source or harness-doc citation. Output goes in this file.
- [ ] 2. Matt approves the gap list and the order.
- [ ] 3. Close gaps in the shared layer first (prompt blocks, then
      permission / elicitation round trip, then title, then config), one
      bounded packet each, so every native transport inherits the fix.
- [ ] 4. Per-transport packets for whatever the shared change exposes.
- [ ] 5. Parity guard test: every `SessionClient` method and every web wire
      event has a native arm or a recorded "harness cannot" entry.
- [ ] 6. Update `docs/architecture/web-session-behavior.md` (the "Native …
      events" section currently documents gaps 1–3 as intended behaviour).

## Stop conditions to respect

Gap 2 changes a security assumption: Claude tool calls are auto-approved in
the sidecar today. Parity means the SAME web permission policy ACP harnesses
already use (auto-approve setting + persistence, see "Permission
persistence") governs native transports — stricter than today, never looser.
Covered by the parity requirement + "Implement until finished"; done last so
Matt can object first.

## Validation

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-web -p ajax-core --no-fail-fast
npm run verify
```

No results yet; nothing implemented under this plan.
