# Task model picker and switcher MVP

Status: approved for immediate implementation.

Approval: the user requested architectural planning and implementation on
2026-08-20.

Reference: [PR 1015](https://github.com/mossipcams/ajax-cli/pull/1015) is
prototype evidence. This plan rebuilds the feature from current `main`.

## Definition of done

The MVP is complete when all of these statements are true:

- New Task keeps one model catalog contract through
  `GET /api/session/models?agent=<harness>`.
- A connected Ajax Chat shows model, effort, and Fast controls only when ACP
  advertises those options.
- A live pick sends the exact advertised `configId` and string or boolean value.
- The control stays on its confirmed value until a replacement snapshot arrives.
- A successful live pick keeps the ACP child, ACP session, TaskSession slot,
  WebSocket identity, and JSONL transcript.
- Ajax persists a restart pin only after ACP confirms the change.
- A refused pick does not persist or change the confirmed browser value.
- A persistence failure keeps the confirmed live change and reports that restart
  may restore the prior pin.
- Harness Switch changes only the harness. It clears the prior harness model pin
  and starts the new harness with empty context while keeping the transcript.
- Focused Rust and Web tests cover model, effort, Fast, refusal, persistence,
  restart, and harness switching.
- A real or fixture-backed product flow proves create, live switch, reload, and
  cross-harness reset behavior.

## Scope

The change may update the existing Web Session vertical slice, its ACP adapter,
the Web Chat controls, Harness Switch, focused tests, generated Web assets, and
the two owning architecture documents.

Expected size is about 17 production and test files, two architecture documents,
and generated Web assets. The implementation must stop if it needs a second
catalog endpoint, a database migration, or a task-lifecycle change.

## Non-goals

- No `/api/session/option-catalog` endpoint or second catalog cache.
- No generic editor for every ACP config category.
- No Native Cockpit model controls.
- No settings defaults editor.
- No task lifecycle, registry ownership, terminal, permission, or transcript
  redesign.
- No broad migration of `session_model` to a new core type.
- No attempt to infer every future Cursor model name from suffixes.
- No compatibility removal for existing bare, exploded, bracket, or pipe values.

## Data shape

The command models one exact advertised change:

```rust
enum SessionConfigValue {
    Select(String),
    Boolean(bool),
}

struct SessionConfigChange {
    config_id: String,
    value: SessionConfigValue,
}

struct ConfirmedConfigChange {
    generation: u64,
    restart_pin: Option<String>,
}
```

The three state forms stay separate:

- `Task.session_model` is desired restart state.
- `sessionConfigOptions` is confirmed live ACP state.
- Browser controls are a presentation of the confirmed live state.

`None` means Auto. Ajax never persists the literal `auto`.

## Usage

New Task keeps the current flow:

```text
NewTaskSheet
  -> ModelPicker
  -> GET /api/session/models
  -> POST /api/tasks { agent, model }
  -> Task.session_model
```

A connected pick uses the live descriptor:

```text
snapshot.sessionConfigOptions
  -> SessionConfigChange { configId, value }
  -> TaskSession command loop
  -> session/set_config_option
  -> replacement configOptions
  -> confirmed restart pin
  -> core-owned task persistence
  -> replacement snapshot
```

Harness Switch uses only the target harness:

```text
HarnessSwap
  -> POST /api/tasks/{handle} { agent }
  -> clear session_model
  -> reset ACP context
  -> keep TaskSession, WebSocket, and JSONL
```

## Architecture decision

The browser forwards exact advertised values. It does not build task metadata.
The ACP adapter validates the descriptor and applies one exact option. The
per-task command loop serializes the live change. After success, the Web Session
slice derives a restart pin from the complete confirmed descriptor list and
persists it through the existing core operation.

The restart pin uses the existing pipe grammar:

```text
<exact confirmed model value>|<thought-level id>=<value>|<model-config id>=<value>
```

String and boolean option values retain their wire type during live apply.
Boolean restart values encode as `true` or `false`. The encoder uses a stable
order and rejects values that the current bounded pipe grammar cannot represent.
If confirmed state cannot be encoded after a successful live change, Ajax keeps
the live change, retains the prior pin, and reports a warning.

For Cursor task creation, the catalog slug remains the spawn hint. If the live
handshake cannot map an effort or thinking suffix onto a writable advertised
option, Ajax accepts the spawned model and records the confirmed descriptor
state instead of failing the session. A later restart may spawn the safe default
and reapply the exact confirmed pipe in-band.

## Synthesis decision

The minimal advertised-option design is the base. It reuses the existing catalog,
task persistence, command loop, and harness-reset flow.

The direct `apply_config_option` path from PR 1015 is retained because exact wire
values are the feature's main invariant. The candidate suggestion to route a
one-change pin through `apply_model_pin` is rejected. That mapper interprets
Cursor strings and caused issues 1010, 1011, and 1013.

The typed `SessionModelSelection` core migration is deferred. It would touch an
estimated 36 to 42 files and move stable task, command, and API code. The MVP
needs a typed live command, not a repository-wide model migration.

## Tradeoffs accepted

- We accept the existing string persistence boundary in exchange for a smaller
  change that leaves the database and public request shapes intact.
- We accept spawn-default-then-apply behavior for opaque confirmed Cursor values
  in exchange for never guessing a CLI slug from unstable model-name grammar.
- We accept a warning after post-apply persistence failure because ACP and SQLite
  cannot commit atomically.
- We accept explicit model, effort, and Fast controls instead of a generic ACP
  config editor.

## Alternatives considered

### Merge or trim PR 1015

Rejected. The prototype changes 63 files and adds a second catalog system. It
also leaves the real New Task and connected-session checks incomplete.

### Canonical `SessionModelSelection` in core

Deferred. The type is a reasonable later cleanup, but the migration is larger
than the MVP and does not remove the need to preserve exact ACP values.

### Reuse `apply_model_pin` for live changes

Rejected. It reconstructs option meaning from strings instead of forwarding the
advertised pair. That behavior is the root of the open Cursor mapping defects.

## Implementation units

- [x] Unit 1. Add `SessionConfigChange` and the direct ACP apply path.
  - Validate the ID, value type, and advertised choice before ACP I/O.
  - Replace the complete option list from the successful response.
  - Keep the child, session ID, generation, and transcript.
  - Verify with focused fake-ACP tests.

- [x] Unit 2. Persist confirmed restart state after successful apply.
  - Derive model, thought-level, and model-config values from the replacement
    descriptors.
  - Preserve string and boolean live types.
  - Persist through the existing core operation.
  - Report post-apply persistence or encoding failures without rolling back ACP.
  - Cover effort-only and Fast-only restart persistence.

- [x] Unit 3. Add pessimistic connected controls.
  - Render model, effort, and Fast from `sessionConfigOptions`.
  - Send exact `configId` and value.
  - Keep the displayed value unchanged until the snapshot confirms it.
  - Show a dismissible error for refusal or persistence warning.

- [x] Unit 4. Make Harness Switch harness-only.
  - Remove its model picker.
  - Send only a different target harness.
  - Clear the previous harness pin.
  - Keep the existing context-reset and transcript behavior.

- [x] Unit 5. Update architecture docs and generated Web assets.
  - Update `docs/architecture/web-cockpit.md`.
  - Update `docs/architecture/web-session-behavior.md`.
  - Rebuild committed Web assets through the repository hook or build command.

- [x] Unit 6. Give the connected sheet New Task's picker vocabulary.
  - Replace the native select, native checkbox, and the unstyled
    `.session-config-chip` / `.session-config-segment` wrappers with
    `.model-option` radio rows and `.reasoning-option` Effort / Fast rows.
  - Cap only the model list so Effort and Fast stay reachable, removing the
    nested scroller created by capping the whole picker group.
  - Show a host-reported but unadvertised model as a disabled selected row
    instead of leaving the list with nothing selected.
  - Add the missing focus-visible and disabled states to the shared option
    classes, which New Task also uses.
  - Leave the apply contract and pessimistic binding unchanged.

## Verification

Baseline before changes:

- [x] `cargo test -p ajax-web --lib adapters::web_session_acp::apply_model`
  passed 5 tests.
- [x] `cargo test -p ajax-web --lib slices::web_session` passed 122 tests.
- [x] `npm run web:test -- --run ModelPicker HarnessSwap ChatSurface
  useTaskSession liveSessionConfig` passed 81 tests in 6 files after `npm ci`.
- [x] `npm ci` completed. npm reported three existing high-severity advisories.

Implementation checks:

- [x] `cargo test -p ajax-web --lib adapters::web_session_acp::apply_model`
  passed 5 tests.
- [x] `cargo test -p ajax-web --lib adapters::web_session_acp::config_options`
  passed 16 tests.
- [x] `cargo test -p ajax-web --lib slices::web_session` passed 124 tests
  (`--test-threads=1`).
- [x] `cargo test -p ajax-web --lib runtime::tests` passed 95 tests
  (`--test-threads=1`).
- [x] `npm run web:test -- --run ModelPicker NewTaskSheet HarnessSwap ChatSurface
  SessionModelControls useTaskSession liveSessionConfig webSessionTransport
  TaskDetailsSheet TaskTerminalView` passed 191 tests in 11 files.
- [x] `npm run web:check`
- [x] `npm run web:lint`
- [x] `npm run verify:arch` (ajax-core 8, ajax-web 13, ajax-tui 2,
  ajax-supervisor 2).
- [x] `git diff --check`
- [x] `npm run web:build` regenerated `crates/ajax-web/web/dist`.

Product-flow check (fixture-backed; re-run `.audit/prove-task-model-mvp.sh`):

- [x] Create a Cursor orchestration task with a non-default effort or thinking
  model.
- [x] Send a prompt with a unique nonce.
- [x] Change model, effort, and Fast from the connected control.
- [x] Confirm that the controls move only after the host snapshot.
- [x] Confirm that the same child and transcript retain the nonce.
- [x] Reload Web Cockpit and confirm the selected values return.
- [x] Switch to another harness and confirm a context-reset divider, retained
  transcript, and a working new prompt.

## Approval and deviations

Immediate implementation is approved by the user's request. Record material
deviations here before changing the design.

Current deviations: none accepted. The first delegated pass and its one bounded
revision were rejected. An explicit per-request in-process bypass then completed
the corrective pass: live apply validates string vs boolean before ACP I/O,
effort-only and Fast-only refresh the complete restart pin, Auto encodes as
`auto`, and Harness Switch is harness-only. `SessionConfigValue` lives in
`ajax-web`, not `ajax-core`. Product-flow was proven with fake ACP and the
public HTTP swap route (`.audit/task-model-mvp-proof.txt`); a live Cursor
binary in Web Cockpit was not clicked.

A second per-request bypass covered Unit 6, because `scripts/run-delegate` does
not exist in this repository and the documented acpx dispatch path is therefore
unavailable. The redesign changes presentation only; the proof script still
passes unchanged. The rendered sheet was measured in Chromium at iPhone 13
width for tap targets, group spacing, and long-list scroll containment; it was
not checked on physical iOS Safari.
