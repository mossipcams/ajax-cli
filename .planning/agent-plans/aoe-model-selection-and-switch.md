# Plan: AoE model selection and switch

**Status:** approved — copy AoE exactly
**Approval:** granted in chat 2026-08-20 (“Copy it exactly”)
**Branch:** `ajax/re-engineer-model-selection-and-switch`
**Reference:** Agent of Empires structured view (ACP `configOptions`,
`POST /acp/config-option`, `POST /acp/switch-agent`)

Exact-copy decisions (supersede the earlier Ajax-extension options):

1. Persist **after** a successful live model pick, not before.
2. Connected model/effort UI opens from a **composer hotbar model control** (modal
   sheet), except **Cursor model** which lists the grouped exploded session-models
   catalog (`GET /api/session/models`) and applies catalog ids via `set_model`.
   Switch is harness-only.
3. New Task / idle Switch picker sources:
   - **Cursor:** exploded catalog ids from `GET /api/session/models` (`agent models`),
     grouped by family with selectable variant ids. Persist the catalog model id;
     never remap the default variant to `auto`.
   - **Codex / Claude / Pi:** last-advertised option-catalog advertised model ids
     (`GET /api/session/option-catalog` + handshake probe). Flat list is fine.
4. Persist **catalog model id only**. Do not persist an effort/Fast map.
   Effort/Fast are live advertised options; spawn forwards `model`;
   unadvertised effort is skipped with a warning, not a failed session.

## Problem

Ajax already applies live ACP `configOptions` in-band, but the operator
surface is still an Ajax-invented catalog:

- New Task and Switch list `GET /api/session/models` (Cursor `agent models`,
  exploded bases, pipe-form `grok-4.6|effort=high|fast=false`).
- Live `sessionConfigOptions` only seed the current pin and encode apply.
- Same-harness model change is a Switch **Apply** (or WebSocket `set_model`
  with a composite string), not a per-option live pick.
- Persist happens **before** apply. AoE persists **after** the live pick
  succeeds.

Agent of Empires treats advertised `configOptions` as the picker. Ajax still
treats the Ajax catalog as the picker.

## What AoE actually does

Evidence: AoE `docs/structured-view.md`, `docs/structured-view/controls.md`,
`web/tests/acp-config-pickers-ui.spec.ts`, PR
[#2771](https://github.com/agent-of-empires/agent-of-empires/pull/2771),
`POST /api/sessions/:id/acp/switch-agent`.

### Connected session (model / effort)

- Composer footer shows a **model chip** and a **reasoning selector** only
  when the adapter advertises those options. No advertisement → no chrome.
- The list is the advertised `options[]`. The active value is
  `current_value`. Each `ConfigOptionsUpdated` **replaces** the list.
- Clicking a value POSTs `{ config_id, value }` immediately. No composite
  pin, no Apply button.
- UI is **pessimistic**: the chip stays on the previous value until the
  confirming snapshot lands. Rejection shows a dismissable notice; the chip
  does not move.
- After a successful **model** pick, persist the advertised model id onto the
  instance so respawn/reconnect does not re-inject the old value.

### New session

- Pick an agent. Model defaults come from last-advertised option catalog /
  `[acp.acp_defaults.<agent>]`, not a hand-maintained list.
- Spawn forwards `model`. `effort` and `mode` are applied once advertised;
  an unadvertised default is skipped with a warning, not a failed session.
- Agents that have never run get a handshake-only capability probe so the
  picker is not empty.

### Switch agent

- Same-harness model change is **not** switch. It is `set_config_option`.
- Cross-agent: `POST /acp/switch-agent` `{ target, model?, reason }`.
  Stop the old worker, spawn the target, persist `agent_name`, **clear**
  `acp_session_id`, emit `AgentSwitched`, keep the transcript. Optional
  model is a spawn override for the new agent.

## Ajax mapping (keep Ajax ownership)

Core still owns task truth. The browser still presents it. Do not make the
browser a second catalog, registry, or apply engine.

| AoE | Ajax |
| --- | --- |
| Advertised `configOptions` | Protocol v2 `snapshot.sessionConfigOptions` (already on the wire) |
| `POST /acp/config-option` | WebSocket `set_config_option` `{ configId, value }` on the live slot |
| Persist `agent_model` after success | Core `session_model` (advertised model id) after a successful model pick |
| `POST /acp/switch-agent` | Existing HTTP Switch, harness-only: persist agent, reset ACP context, keep TaskSession + JSONL |
| Option catalog / probe | `GET /api/session/option-catalog` (last-advertised `configOptions` per harness; handshake probe when empty) |
| Composer model sheet | Ajax Chat composer hotbar opens a model sheet; Switch stops being the in-session model control |

## Replacement contract

1. **Connected picker is advertised options (bridge harnesses) or grouped catalog (Cursor).**
   - **Cursor:** model sheet lists exploded ids from `GET /api/session/models`
     grouped by family; picks send `set_model` with the catalog id.
   - **Codex / Claude / Pi:** bind chips to `sessionConfigOptions` by category
     (`model`, `thought_level`, `model_config`). Id fallback only.
   - No advertisement → no model/effort/Fast chrome (AoE: empty, not Auto).

2. **Immediate per-option apply.**
   - Choosing a value sends `set_config_option` with the advertised
     `configId` and advertised value (select id or boolean).
   - Never send Ajax catalog ids or pipe-form on the ACP wire.
   - Keep process, `sessionId`, and JSONL. Respawn only when the child is
     dead or no model control is advertised.

3. **Pessimistic UI.**
   - Chip/segment stays on the last confirmed `currentValue` until the host
     snapshot (or `config_option_update`) confirms.
   - Refusal: typed error / dismissable notice; child keeps running;
     picker does not move.

4. **Persist after success, not before.**
   - On a successful **model** option apply, persist the advertised
     `currentValue` as task `session_model`.
   - Persist failure after a live apply is a typed warning; the running
     child keeps the new model (AoE: live pick already happened).
   - Auto/unspecified stays `None`. Never store the literal `auto`.
   - Do not persist thought_level or model_config. Effort/Fast exist only
     as live advertised options. Spawn forwards `model`; unadvertised
     effort is skipped with a warning.

5. **New Task / idle Switch.**
   - **Cursor:** picker lists exploded ids from `GET /api/session/models`. Group
     header is the model family name (non-selectable); each variant id is a
     selectable radio whose value is that catalog id. Prefer catalog labels when
     present; otherwise humanize effort/Fast from the id. `onChange` emits the
     catalog id; never remap the default variant to `auto`.
   - **Codex / Claude / Pi:** picker lists advertised model choices from the
     harness option catalog (last-advertised `configOptions` / handshake probe).
   - Failed catalog read: operator-visible error with retry. No silent Auto
     fallback (#948).
   - Create stores the catalog model id only. Spawn uses Cursor `--model` as a
     launch hint only.

6. **Switch is harness change.**
   - Same harness: in-session chips, not Switch Apply.
   - Cross harness: persist agent + optional spawn model, cancel in-flight
     work, shut down old ACP child, clear stored resume id, `session/new`
     with empty context, host note that the harness switched, keep
     TaskSession / JSONL / WebSocket identity.
   - No live slot: persist and clear resume id only.
   - Interactive (tmux send-keys) tasks still cannot switch.

7. **`snapshot.model`** stays the model option’s advertised `currentValue`.
   Desired spawn pin is task metadata. Applied state is harness evidence.

## Non-goals

- AoE context-primer / composer prefill on switch.
- AoE rate-limit recovery flow.
- AoE Settings `acp_defaults` editor.
- Changing task lifecycle, registry ownership, JSONL ownership, tmux
  terminal, or ACP permission-mode policy.
- Vendoring harness model lists.
- Filesystem / terminal client capabilities.
- Native Cockpit / TUI model chips in this change.

## Implementation tasks

- [x] Task 1 — Live `set_config_option` + persist-after-success
  - Replace composite `set_model` as the live pick path.
  - Host: apply advertised option; on success persist **model id only**;
    publish replaced `sessionConfigOptions`.
  - Tests: child/`sessionId` kept; persist-after-success; persist failure
    does not revert the live child; refusal is typed + child kept.

- [x] Task 2 — Composer footer chips (pessimistic)
  - Ajax Chat binds to `sessionConfigOptions`. Immediate pick. Confirming
    snapshot moves the chip; refusal notice does not.
  - Switch modal is harness-only when connected.

- [x] Task 3 — New Task / idle Switch catalogs
  - Cursor lists exploded ids from `GET /api/session/models` grouped by family.
  - Codex / Claude / Pi list advertised model choices from option catalog.
  - Keep a compatibility decode for existing stored pipe-form /
    exploded ids so old tasks still attach.

- [x] Task 4 — Cross-harness Switch matches AoE switch-agent
  - Confirm/adjust HTTP Switch to the harness-only reset above.
  - Same-harness HTTP Switch with only a model change should not exist as
    the operator path (chips own it). Keep a narrow API for idle tasks
    with no live slot if needed (persist only).

- [x] Task 5 — Architecture docs
  - Update `docs/architecture/web-session-behavior.md` and
    `docs/architecture/web-cockpit.md` in the same change.

## Validation

- `cargo test -p ajax-web --lib adapters::web_session_acp::apply_model` — pass (parent after_execute, 2026-08-20)
- `cargo test -p ajax-web --lib slices::web_session` — pass 122/122
- `npm run web:test -- --run ModelPicker HarnessSwap ChatSurface useTaskSession liveSessionConfig ConfigPickers` — pass 71/71
- rustfmt on changed Rust
- `git diff --check`

## Material deviation (2026-08-20)

User instruction **supersedes** the approved plan's "persist catalog model id" and
"Cursor live sheet applies catalog ids via `set_model`". The five-layer identity
contract is authoritative:

1. **Canonical Ajax state:** structured fields (`base`, `effort`, `fast`, `thinking`).
2. **Ajax storage:** pipe string encoding those fields (e.g.
   `grok-4.6|effort=high|fast=false`,
   `claude-opus-5|thinking=true|effort=medium|fast=false`). Auto stays unset / `auto`.
3. **Cursor startup:** CLI slug on `--model` (`cursor-grok-4.6-high`,
   `claude-opus-5-thinking-medium`); convert stored pipe → slug at spawn.
4. **Cursor ACP live switch:** advertised value via `session/set_config_option` only;
   never catalog ids, pipe-form, or reconstructed brackets on the wire; live Cursor UI
   must not call WebSocket `set_model` with catalog ids.
5. **UI:** friendly labels; New Task / idle persist emits pipe; live switch emits
   advertised `configId` + value.

### Checklist (identity split)

- [x] `encode_cursor_intent_to_storage_pipe` + spawn pipe → CLI slug (incl. thinking)
- [x] `applied_model_id_for_persist` returns pipe from advertised options
- [x] Persist-after-success uses pipe, not wire model id
- [x] ModelPicker New Task emits pipe on change
- [x] ConfigPickers live Cursor uses advertised options (`set_config_option`), not catalog + `set_model`
- [x] Docs updated (`web-session-behavior.md`, `web-cockpit.md`)

## Remaining

- `crates/ajax-web/.fake-acp-spawn-gen` is regenerated by `fake_acp.js` tests; do not commit.
- `applied_model_id_for_persist` is wired for spawn persist and live `set_config_option`; only `clear_option_catalog_cache` may still warn dead_code outside tests.
- `collapse_cursor_catalog` remains for legacy callers/tests; Cursor `GET /api/session/models` now returns exploded ids for the picker.

## Stop / approval

Approved 2026-08-20: copy AoE exactly (persist-after-success, composer
chips, option-catalog route, model-id-only persist). Implemented.
