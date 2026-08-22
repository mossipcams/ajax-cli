# Get more from ACP

**Status:** complete — slices 1–6 implemented (2026-08-21)
**Approval:** operator requested uninterrupted implementation of slices 1–6 (2026-08-21)
**Branch:** `ajax/acp-extensions`
**Protocol baseline:** stable ACP v1; ACP v2 remains draft
**Related:** `.planning/agent-plans/acp-slash-commands.md` (paused first-slice spec)

## Problem

Ajax Chat is an ACP client, but it only consumes a slice of stable v1:
initialize, session new/resume/load, prompt/cancel, config options, messages,
thoughts, tool calls, diffs, plans, usage, and host auto-approve of
permissions. The rest is advertised false, dropped, or unused.

Slash-command pass-through is one operator-visible gap. It is not the whole
gap. An interrupted slash-only implementation was restored so this program
can be sequenced first.

Official surface: https://agentclientprotocol.com/llms.txt

## Ajax constraints that still win

- Core owns task truth. ACP session id is a child handle, not a second
  registry.
- JSONL under `web-session/` is the operator transcript. ACP `session/list`
  must not become transcript truth.
- Ajax Terminal is tmux/xterm. ACP `terminal/*` is in-client command
  execution for the agent, a different object.
- Trusted local automation: full tool access, auto-approve, no public
  internet product path. Advertising `fs`, `terminal`, URL elicitation, or a
  permission UI is a security/architecture change.
- Advertise only capabilities Ajax actually implements.

## Inventory (stable ACP v1 vs Ajax Chat)

### Already in use

| Surface | Ajax |
| --- | --- |
| `initialize`, protocol v1, client info | yes |
| `session/new`, `session/resume` then `session/load` | yes |
| `session/prompt`, `session/cancel` | yes (text only) |
| `session/set_config_option`, `config_option_update` | yes (live snapshot) |
| message / thought chunks, tool calls, diffs, plan, usage | yes |
| `session/request_permission` | auto-approved on host |

### Unused or dropped (this program)

| Surface | What it is | Ajax today | Fit |
| --- | --- | --- | --- |
| `available_commands_update` + `/name` in prompt text | Agent advertises slash commands; client sends them as prompt text | dropped | **Slice 1.** High value, same live-state pattern as config options |
| Prompt `image` / `embeddedContext` (`resource`, `resource_link`) | Screenshots, @-file context in `session/prompt` | text-only; capabilities not advertised | **Slice 2.** iOS camera/photos and file mentions. Audio ACP blocks are separate (speech today becomes text) |
| `session_info_update` | Agent-reported session title / info | dumped as artifact or ignored | **Slice 3.** First-class chrome, not transcript noise |
| `elicitation/create` form mode | Structured questions with a schema | not advertised | **Slice 4.** Operator Q&A without a security-model change. URL mode is **out** until security review |
| Non-text **output** (image / resource / resource_link in messages and tool cards) | Same content blocks on the way back | Ajax keeps `ContentBlock::Text` and diffs only | **Slice 5.** Completes slice 2; screenshots and file cards in the transcript. Does not advertise `terminal/*` or embed ACP terminals |
| `session/close` | Cancel work and free the ACP session when advertised | child kill / shutdown | **Slice 6.** Use `session/close` when the agent advertises it; do not treat it as Ajax Terminal or task Drop |
| Tool-call `rawInput` / `rawOutput` and location **line** | Arguments, result payload, follow-along line | title/kind/status/path/text+diff only | Out of sequence; richer activity cards without new capabilities |
| Plan entry `priority` | high / medium / low on each plan row | content + status only | Out of sequence; small add-on to existing plan UI |
| `additionalDirectories` | Extra absolute roots besides `cwd` | unused | Out of sequence; only if operators actually work across multiple trees |
| `mcpServers` on session new/resume/load | Client attaches MCP servers to the agent | out of this program | Explicitly removed from the sequence (2026-08-21) |
| Client `fs/read_text_file`, `fs/write_text_file` | Agent asks the **client** to read/write | advertised false | Architecture + worktree scoping. Do not start until slash/content/elicitation land |
| Client `terminal/*` | Agent-run commands with live output embeddable in tool calls | advertised false | Not Ajax Terminal. Complementary, high confusion/security risk |
| Permission UI | Operator decides tool calls | auto-approve by product policy | Product/security change, not "more ACP" |
| `session/list`, `session/delete` | Agent-side session catalog | Ajax tasks + JSONL | Do not adopt as truth |
| `authenticate` / `logout` | Agent auth | unused | Trusted local; leave |
| ACP v2, MCP-over-ACP RFD, proxy chains | draft / RFD | out | Stay on v1 until v2 is stable |

## Recommended order

Operator-set (2026-08-21):

1. Slash commands (advertise, complete, pass-through). Detail:
   `.planning/agent-plans/acp-slash-commands.md`.
2. Rich prompt content: `resource_link` is baseline ACP prompt content
   (Ajax currently sends text only); then image and optional embedded file
   bodies.
3. `session_info_update` as session chrome.
4. Form elicitation.
5. Non-text output: image / resource / resource_link in agent messages and
   tool cards (text + diffs stay; no ACP terminal embeds).
6. `session/close` when the agent advertises it.

MCP servers are **not** in this sequence.

Stop and write a dedicated architecture plan before: filesystem client
methods, ACP terminals, URL elicitation, permission prompts, or ACP v2.

## Non-goals for this program

- Replacing Ajax Terminal / tmux with ACP terminals.
- Making ACP `session/list` or `session/delete` own task or transcript truth.
- Advertising capabilities Ajax does not implement.
- Changing auto-approve or the trusted-local security model in the same
  change as a utilization slice.

## Approval gate

Operator requested implementation of slices 1–6 until finished (2026-08-21).
Each slice is one bounded EXECUTION. Architecture docs update in the same
change as the slice.

## Checklist

- [x] Inventory stable ACP v1 against Ajax Chat
- [x] Operator picks slice order (1→6: slash, prompt content, session info, form elicitation, non-text output, session/close)
- [x] Slice 5 — non-text output (image / resource / resource_link in messages and tool cards)
- [x] Slice 6 — session/close when advertised
- [x] Slice 4 — form elicitation (advertise form only, operator form UI, snapshot replay)
- [x] Slice 1 — slash commands (advertise, complete, pass-through); see `.planning/agent-plans/acp-slash-commands.md`
- [x] Slice 2 — rich prompt content (`resource_link`, image, embedded context)
- [x] Slice 3 — `session_info_update` as session chrome
- [x] Update `docs/architecture/web-session-behavior.md` and
      `docs/architecture/web-cockpit.md` in the same change as each slice
