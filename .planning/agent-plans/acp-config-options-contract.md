# ACP config-options contract

**Status:** approved
**Approval:** operator requested AoE implementation (2026-08-19)
**Branch:** `ajax/acp-contract`
**Defect:** [#997](https://github.com/mossipcams/ajax-cli/issues/997)
**Protocol:** stable ACP v1 session config options (`agent-client-protocol` 2.0.0 / schema 1.5.0)

## Problem

Ajax chat cannot switch models because the live ACP contract is Ajax-invented, not ACP.

ACP practice (https://agentclientprotocol.com/protocol/session-config-options):

- Agents advertise `configOptions` on `session/new` / resume / load.
- Clients **SHOULD** use that list as the session configuration surface.
- Switch a value with `session/set_config_option` using the advertised `configId` and an advertised value.
- Select values are ids. Boolean values are `{ type: "boolean", value: bool }` and require `clientCapabilities.session.configOptions.boolean: {}`.
- Categories (`model`, `thought_level`, `model_config`, `mode`) are UX metadata. They **MUST NOT** be required for correctness, but they are how a client finds the model selector when `id` is not `"model"`.
- `session/set_model` and `models.availableModels` were removed (June 2026). `session/set_mode` is superseded when a `category: "mode"` option exists.
- `config_option_update` is applied-state, not conversation.

Ajax today:

| Ajax behavior | ACP practice |
| --- | --- |
| Catalog / pipe-form / reconstructed brackets (`cursor-grok-4.6-high`, `grok-4.6\|effort=high\|fast=false`, `grok-4.6[effort=high,fast=false]`) are the live identity | Advertised `currentValue` on each config option is applied identity |
| Apply looks up hardcoded ids `model` / `fast` / `effort` / `reasoning` | Use advertised `id`; group by `category` for UX |
| Fast is a select of `"true"` / `"false"` strings; boolean kinds are ignored | Fast is `category: model_config`, `type: boolean` |
| Initialize advertises Cursor `_meta.parameterizedModelPicker` only | Also advertise `session.configOptions.boolean` if we consume boolean options |
| `SetSessionConfigOptionRequest::new(..., string)` always sends a value id | Send `SessionConfigOptionValue::boolean` for boolean options |
| `config_option_update` maps to a transcript `artifact` | Update stored `configOptions` / `snapshot.model` |
| Handshake is mirrored into deprecated `models.availableModels` | `configOptions` is the catalog for a live session |
| Cursor still pins `--model` on spawn, then in-band, then respawn | Spawn argv is a Cursor launch hint; live switch is in-band config options |

Evidence:

- `apply_model.rs` / `cursor_config.rs` only handle `SessionConfigKind::Select`.
- `client_capabilities()` never sets `session.configOptions.boolean`.
- `preferred_permission_config` requires `id == "mode"`, not `category == Mode`.
- `acp_map.rs` maps `SessionUpdate::ConfigOptionUpdate` to `typed_artifact("config", …)`.
- Recent closed defects (#952, #954, #979, #989, #991–#993) patched spawn tokens, catalog mapping, Fast/effort split, and respawn. The operator still cannot switch models.

## Goal

Make advertised ACP `configOptions` the live session configuration contract so same-harness Switch / `set_model` works.

Task `session_model` stays **desired** Ajax state (catalog / pipe-form for New Task). Protocol snapshot applied state is **only** harness-reported config-option `currentValue`s.

## Non-goals

- ACP protocol v2 negotiation.
- Filesystem / terminal client capabilities.
- Changing task registry ownership, JSONL transcript ownership, or the browser wire version.
- Vendoring harness model lists.
- Making spawn `--model` the in-session switch mechanism.
- Cross-harness Switch policy (already a context reset on the same public Ajax session).

## Replacement contract

1. **Initialize**
   - Keep protocol v1.
   - Advertise `clientCapabilities.session.configOptions.boolean: {}`.
   - Keep Cursor `_meta.parameterizedModelPicker: true` as a vendor extra, not as the model contract.
   - Keep `fs` / `terminal` false.

2. **Live applied config**
   - After `session/new`, resume/load, every `set_config_option` response, and every `config_option_update`, store the complete `configOptions` list on the ACP live session.
   - Model selector: first option with `category == model`, else `id == "model"` if that id is a select.
   - Thought level: first `category == thought_level`.
   - Model extras (Fast, context size): `category == model_config` (boolean or select).
   - Mode: first `category == mode`, else advertised id `mode` — only for the existing full-access apply.

3. **In-band apply**
   - Map the Ajax desired pin onto **currently advertised** options. If any piece is missing, typed error; leave the child running.
   - Send one `session/set_config_option` per changed option:
     - select → advertised value id
     - boolean → `{ type: "boolean", value }`
   - Never send Ajax catalog ids (`cursor-grok-4.6-high`) as config values.
   - Read applied state from the response `configOptions`, not from a reconstructed bracket string.

4. **`snapshot.model`**
   - The model option's `currentValue` (advertised id).
   - Do not synthesize `base[effort=…,fast=…]`.
   - If thought_level / model_config are advertised, expose them as separate applied fields (or keep them only on the host until the browser picker reads live options). Bare model currentValue must not be treated as satisfying an effort/Fast pin.

5. **Switch / `set_model`**
   - Persist desired `session_model` first (existing core operation).
   - Live slot: in-band apply as above. Keep process, `sessionId`, JSONL.
   - No live slot: persist only.
   - Respawn (`session/new`, no resume) only when the child is dead or no model control is advertised at all.
   - `config_option_update` refreshes applied config without a transcript artifact.

6. **Catalog**
   - New Task before a session exists may still use `GET /api/session/models` (Cursor CLI `agent models`, bridge handshake).
   - A connected session's picker must bind to live advertised options, not to a reconstructed snapshot string.
   - Stop using deprecated `models.availableModels` as authority. Stop `enrich_session_new_value` mirroring.

## Implementation tasks

- [x] Task 1 — Client capabilities and typed config helpers
  - Advertise boolean config-option support.
  - One helper: find option by category with id fallback; read current value for select **and** boolean; build `SetSessionConfigOptionRequest` with the matching value type.
  - Test: initialize JSON includes `session.configOptions.boolean: {}`; boolean Fast current value is read; string `"false"` is not sent for a boolean option.

- [x] Task 2 — Apply uses advertised options only
  - Replace `apply_model_pin` / `cursor_config` hardcoded id + reconstruct-bracket path.
  - Map Ajax desired pin → advertised options; apply; read back `currentValue`.
  - Test: parameterized handshake (select model + thought_level + boolean Fast) switches Grok High non-Fast without catalog ids; missing advertisement is a typed error, child kept.

- [x] Task 3 — Live Switch / `set_model` + `config_option_update`
  - Same-harness Switch keeps the child when in-band apply succeeds.
  - `config_option_update` updates stored config / snapshot, not an artifact.
  - Test: `set_model` keeps `child_id` / ACP `sessionId`; agent-driven Fast change updates applied state; no `artifact` row.

- [x] Task 4 — Snapshot / picker bind to advertised current values
  - `snapshot.model` is the model option `currentValue`.
  - Connected picker decodes live applied options (not reconstructed brackets).
  - Docs: `docs/architecture/web-session-behavior.md` and `docs/architecture/web-cockpit.md`.
  - Regression: existing #954 guard (no catalog id on the wire) still holds.

## Review fixes

- [x] Pipe-form pins map by advertised option id (`reasoning`, `effort`, `fast`, …), not Cursor intent keys; regression test for `reasoning` id fixture.
- [x] Split apply skips thought_level when not advertised; only errors on Fast=true without a boolean Fast option.
- [x] HarnessSwap connected picker: pending `model` state seeds from live options, binds ModelPicker `value`, Apply sends full live pin.
- [x] Architecture docs: remove respawn-on-refuse and `models.availableModels` mirroring contradictions; snapshot field list includes `sessionConfigOptions`.

## Validation

- `cargo test -p ajax-web --lib adapters::web_session_acp::apply_model`
- `cargo test -p ajax-web --lib adapters::web_session_acp::client_tests`
- `cargo test -p ajax-web --lib slices::web_session`
- `npm run web:test -- --run ModelPicker useTaskSession`
- rustfmt on changed Rust
- `git diff --check`

## Stop / approval

Architecture change: approved for implementation.

Do not land more spawn-argv / bracket-reconstruction patches as a substitute.
