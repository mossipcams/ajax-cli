# Status and Lifecycle Alignment Plan

Date: 2026-06-09

## Goal

Give every operator-facing Cockpit projection one canonical task status:

- `Running`
- `Waiting`
- `Idle`
- `Error`

Core owns that status and one optional presentation-ready explanation. Lifecycle,
live observations, acknowledgements, runtime health, annotations, and policy remain
typed internal inputs. They continue to control capabilities and diagnostics, but
they do not become competing browser statuses.

Ajax Web must render the canonical status unchanged in the task list, inbox, and
detail hero. It must render only actions that the browser can execute now.

No file under a `tests/` directory will be modified. Every test named below is a
module-local `#[cfg(test)]` test in the listed source file. Approval of this plan
explicitly approves modifications to those listed source files and their local
test modules only.

## Current-State Findings

The implementation already has most of the required boundaries, but they are
connected in the wrong shape:

- `crates/ajax-core/src/ui_state.rs` is the current status reduction point, but
  it emits nine UI states plus a free-form label.
- `TaskCard` repeats `ui_state`, `status_label`, `live_summary`, and
  `primary_action`; Ajax Web repeats them again in separate card/detail DTOs.
- Ajax Web derives headline state again from `ui_state`, `live_status_kind`, and
  pane interaction state.
- Cockpit inbox rows are assembled in `crates/ajax-core/src/commands.rs`, not in
  `attention.rs`; their reason text currently comes from evidence labels.
- `Evidence::label` incorrectly groups shell/running evidence with `CiFailed`,
  allowing non-CI evidence to be labeled `ci failed`.
- Acknowledgement is agent-specific: Claude waiting evidence is erased, while
  Codex waiting evidence remains actionable.
- The task model does not persist the observation time of the reduced live
  status. Without that timestamp, core cannot reliably distinguish acknowledged
  evidence from newer same-kind evidence without deleting the evidence.
- Web action filtering already removes unsupported actions in most paths, but
  the DTO and JavaScript still carry `primary_action`, `available_actions`, and
  status-bearing action-state records.

These findings determine the task order below. In particular, acknowledgement
must gain a durable live-evidence timestamp before the four-state projection can
be correct across refreshes and SQLite reloads.

## Locked Contract

### Status Semantics

| Status | Operator meaning |
| --- | --- |
| `Running` | Current trusted evidence says work is actively progressing. |
| `Waiting` | A current, unacknowledged operator-visible boundary is ready. |
| `Idle` | No work is currently running and no unacknowledged response is waiting. |
| `Error` | Current evidence says normal progress is blocked by failure or broken substrate. |

### Status Precedence

Core evaluates current evidence in this order:

1. Current failure or broken-substrate evidence -> `Error`.
2. Current running evidence -> `Running`.
3. Current unacknowledged waiting/completion evidence -> `Waiting`.
4. Otherwise -> `Idle`.

The reducer must not treat stale cached annotations as current evidence. Existing
live/status reducers remain responsible for replacing or clearing superseded
evidence before the canonical projection runs.

### Explanation Copy

The explanation is optional, presentation-ready text from core. It is not a
second browser enum.

| Input | Status | Explanation |
| --- | --- | --- |
| Agent activity | `Running` | `Agent working` |
| Command activity | `Running` | `Running command` |
| Test activity | `Running` | `Running tests` |
| Approval request | `Waiting` | `Waiting for approval` |
| Input request | `Waiting` | `Waiting for input` |
| Authentication request | `Waiting` | `Authentication required` |
| Rate limit | `Waiting` | `Rate limited` |
| Context limit | `Waiting` | `Context limit reached` |
| Ordinary `Done` evidence | `Waiting` | `Response ready` |
| Reviewable or mergeable lifecycle | `Waiting` | `Ready for review` |
| CI/test failure | `Error` | `CI failed` or `Tests failed` |
| Merge conflict | `Error` | `Merge conflict` |
| Command failure | `Error` | `Command failed` |
| Explicit blocker/dead agent | `Error` | `Agent blocked` or `Agent unavailable` |
| Missing substrate | `Error` | Specific missing-resource text |
| Probe failure | `Error` | `Status unavailable` |
| Teardown failure | `Error` | `Teardown incomplete` |
| Healthy inactive state | `Idle` | none |

Arbitrary pane text and raw hook summaries remain diagnostics. They do not become
status explanations.

### Lifecycle and Capabilities

- Lifecycle remains the workflow authority for Review, Ship, Drop, and related
  operation eligibility.
- Acknowledging a review-ready task may make its status `Idle`; it must not erase
  `Reviewable`/`Mergeable` lifecycle or remove valid Review/Ship actions.
- `Cleanable`, `Merged`, `Removing`, and hidden `Removed` tasks are `Idle` unless
  current running or error evidence says otherwise.
- Trusted `Done` evidence may still transition lifecycle to `Reviewable`.
  Ordinary low-confidence `Done` evidence does not.

### Acknowledgement

- Reduced live evidence has its own durable `observed_at` timestamp.
- An acknowledgement suppresses waiting/completion evidence only when
  `observed_at <= acknowledged_at`.
- The same rule applies to Claude, Codex, and other supported agents.
- A newer prompt, completion, or running observation becomes visible normally.
- Acknowledgement does not delete live evidence, change lifecycle, clear errors,
  or fabricate shell/process state.
- Runtime refresh must accept newer evidence even when its kind matches the
  currently stored kind.

### Browser Contract

Cards and details expose:

```text
status: running | waiting | idle | error
status_explanation: string | null
actions: [browser-supported action metadata]
```

Web action metadata contains the action id, optional label, destructive flag,
and confirmation requirement. Unsupported actions are absent. The browser may
style the first returned action as prominent, but `primary_action` is not part
of the Web contract.

Lifecycle, raw live status, pane state, and runtime diagnostics may remain in
the detail diagnostics payload. They may not override `status` or
`status_explanation`.

### Compatibility Boundary

- The Ajax Web contract removes `ui_state`, `status_label`, `live_summary`,
  `primary_action`, `available_actions`, and action `status` records.
- Native Cockpit moves to the canonical status but may continue to use
  lifecycle, annotations, and action policy internally for grouping and action
  selection.
- Existing CLI JSON may retain `status_label` for one compatibility window.
  If retained, it is derived from the canonical projection as
  `status_explanation.unwrap_or(status)` and is never independently computed.

## Task 1: Fix Evidence Labels Before Reusing Them

### Failing behavior test

Add a table-driven test in `crates/ajax-core/src/models.rs` proving every
`LiveStatusKind` has the intended human label, including:

- `ShellIdle`, `CommandRunning`, `TestsRunning`, and `AgentRunning` never label
  themselves `ci failed`.
- `CiFailed` alone labels itself `ci failed`.
- Waiting, completion, failure, and missing-substrate labels remain distinct.

### Code to implement

- Make `Evidence::label` exhaustive for each live status instead of grouping
  unrelated statuses in the `CiFailed` arm.
- Keep this change behavior-only; do not redesign annotations in this task.

### Verification

```sh
rtk cargo nextest run -p ajax-core models::tests
```

## Task 2: Record the Reduced Live-Evidence Timestamp

### Failing behavior test

Add module-local tests in `crates/ajax-core/src/models.rs` and
`crates/ajax-core/src/live_application.rs` proving:

- New tasks have no reduced-live observation timestamp.
- Applying an observation at a supplied time stores that time separately from
  lifecycle and acknowledgement timestamps.
- Applying newer same-kind evidence updates the timestamp.
- Clearing `Unknown` live evidence clears its timestamp.
- Existing convenience APIs still use the current time when no source time is
  supplied.

### Code to implement

- Add an optional reduced-live observation timestamp to `Task` with serde
  defaults compatible with older task JSON.
- Add timestamp-aware observation application functions while preserving the
  existing convenience entry points.
- Ensure trusted observations keep their lifecycle behavior and store the
  supplied evidence time.

### Verification

```sh
rtk cargo nextest run -p ajax-core live_application::tests
rtk cargo nextest run -p ajax-core models::tests
```

## Task 3: Persist the Timestamp in SQLite

### Failing behavior test

Add tests in `crates/ajax-core/src/registry/sqlite.rs` proving:

- SQLite round-trips the optional reduced-live timestamp exactly.
- A v6 database migrates to v7.
- Migration uses the persisted `last_activity_at` as the best available
  timestamp only when a legacy row has live status; rows without live status
  migrate to `NULL`.
- Half-present seconds/nanoseconds are rejected.

### Code to implement

- Bump the SQLite schema from v6 to v7.
- Add nullable seconds/nanoseconds columns for the reduced-live timestamp.
- Implement strict pair parsing, save/load support, and `migrate_v6_to_v7`.
- Preserve existing acknowledgement and runtime-observation columns unchanged.

### Verification

```sh
rtk cargo nextest run -p ajax-core registry::sqlite::tests
```

## Task 4: Make Acknowledgement Agent-Neutral and Non-Destructive

### Failing behavior test

Add tests in `crates/ajax-core/src/live.rs`,
`crates/ajax-core/src/live_application.rs`,
`crates/ajax-core/src/runtime_refresh.rs`, and
`crates/ajax-core/src/commands/open.rs` proving:

- Opening/acknowledging Claude and Codex waiting evidence records the timestamp
  without deleting live status, flags, or lifecycle.
- Waiting or `Done` candidates at/before the acknowledgement are suppressed for
  every agent and do not trigger pane fallback.
- `CommandFailed`, missing substrate, and other errors are never suppressed.
- Newer same-kind waiting evidence is applied and becomes actionable again.
- Newer running evidence becomes `Running` evidence after acknowledgement.
- Trusted newer `Done` evidence still transitions lifecycle to `Reviewable`.

### Code to implement

- Make acknowledgement record-only; remove Claude-specific state mutation.
- Extend status decisions to return the selected candidate timestamp.
- Apply acknowledgement filtering to waiting and `Done` evidence for all
  agents, while preserving failures.
- Make acknowledged-hold behavior agent-neutral.
- Update runtime refresh to compare both kind and observation timestamp so a
  newer same-kind candidate is not skipped.
- Pass source timestamps into authoritative/trusted observation application.

### Verification

```sh
rtk cargo nextest run -p ajax-core live::tests
rtk cargo nextest run -p ajax-core live_application::tests
rtk cargo nextest run -p ajax-core runtime_refresh::tests
rtk cargo nextest run -p ajax-core commands::open::tests
```

## Task 5: Add the Canonical Four-State Core Projection

### Failing behavior test

Replace/add table-driven tests in `crates/ajax-core/src/ui_state.rs` proving the
locked status and explanation matrix, including:

- Error evidence outranks running/waiting evidence.
- Running evidence outranks waiting evidence.
- Unacknowledged approval/input/auth/rate/context requests are `Waiting`.
- Ordinary `Done` is `Waiting` with `Response ready`.
- Reviewable and mergeable lifecycle are `Waiting` with `Ready for review`.
- Acknowledged waiting/completion evidence is `Idle`.
- Review/Ship capabilities survive that `Idle` projection.
- Healthy inactive, cleanable, merged, removing, and removed states are `Idle`.
- No canonical projection emits a fifth status.

### Code to implement

- Introduce a serializable four-value `TaskStatus` using lowercase wire names.
- Change `OperatorStatus` to contain `status` and
  `explanation: Option<String>`.
- Centralize precedence, acknowledgement comparison, and fixed explanation copy
  in `derive_operator_status`.
- Keep the existing richer `UiState` only as a temporary compatibility adapter
  until Task 7; new code must consume `TaskStatus`.
- Do not change lifecycle transitions or operation eligibility in this task.

### Verification

```sh
rtk cargo nextest run -p ajax-core ui_state::tests
```

## Task 6: Project Canonical Status Into Cards, Inbox, and CLI JSON

### Failing behavior test

Add tests in `crates/ajax-core/src/commands/projection.rs`,
`crates/ajax-core/src/commands.rs`, and `crates/ajax-core/src/output.rs` proving:

- `TaskCard` carries canonical status and explanation from one reduction call.
- Cockpit inbox includes exactly `Waiting` and `Error` cards.
- Cockpit inbox reason equals the card explanation and is never snake_case.
- `Running` and acknowledged `Idle` cards are absent from the Cockpit inbox.
- Reviewable/mergeable cards enter the inbox until acknowledged.
- Action selection remains driven by lifecycle, annotations, operation policy,
  and substrate evidence rather than the four-state status alone.
- CLI JSON exposes canonical `status` and `status_explanation`.
- Any retained `status_label` compatibility field is derived from those fields.

### Code to implement

- Add canonical status fields to core task summaries/cards and next-step output.
- Build Cockpit inbox inclusion from canonical status; retain annotation severity
  and action policy for ordering/recommendations.
- Reuse the canonical explanation for Cockpit inbox reason text.
- Keep the general CLI `inbox` annotation behavior unchanged unless a failing
  compatibility test proves it already promises Cockpit semantics.
- Remove `OperatorStatusKind` checks from action policy; inspect the specific
  underlying error/capability evidence instead.

### Verification

```sh
rtk cargo nextest run -p ajax-core commands::projection::tests
rtk cargo nextest run -p ajax-core commands::tests
rtk cargo nextest run -p ajax-core output::tests
```

## Task 7: Migrate CLI Summaries and Human Rendering

### Failing behavior test

Add/update module-local tests in `crates/ajax-cli/src/render.rs` and
`crates/ajax-cli/src/cockpit_backend.rs` proving:

- Human task summaries show only `Running`, `Waiting`, `Idle`, or `Error` plus
  the optional explanation.
- CLI JSON emits canonical status fields and any compatibility `status_label`
  is mechanically derived from them.
- Running/idle evidence is never rendered as CI failure.

### Code to implement

- Replace CLI rendering reads of independently derived `status_label` with the
  canonical status/explanation.
- Keep the compatibility JSON field only where existing command contracts
  require it.
- Preserve unrelated CLI wording and command behavior.

### Verification

```sh
rtk cargo nextest run -p ajax-cli render::tests
rtk cargo nextest run -p ajax-cli cockpit_backend
rtk cargo check -p ajax-core -p ajax-cli
```

## Task 8: Migrate Native Cockpit Rendering and Grouping

### Failing behavior test

Add/update module-local tests in `crates/ajax-tui/src/rendering.rs`,
`crates/ajax-tui/src/cockpit_state.rs`, and `crates/ajax-tui/src/lib.rs` proving:

- Visible badges and explanations use only the canonical four-state projection.
- Attention ordering and action availability do not regress.
- Review/Ship/Drop selection still follows lifecycle and policy even when the
  canonical status is `Waiting` or `Idle`.
- Internal grouping may distinguish actionable/done work, but that grouping is
  never rendered as another task status.

### Code to implement

- Replace Native Cockpit reads of `ui_state`/`status_label` with canonical
  status/explanation.
- Derive non-visible grouping from lifecycle, annotations, and available actions
  where the TUI still needs it.
- Remove the temporary `UiState` adapter and obsolete core card fields after all
  Rust consumers compile against the canonical projection.
- Preserve existing actions and unrelated layout.

### Verification

```sh
rtk cargo nextest run -p ajax-tui
rtk cargo check -p ajax-core -p ajax-cli -p ajax-tui
```

## Task 9: Collapse Web Actions to One Executable Collection

### Failing behavior test

Add tests in `crates/ajax-web/src/action_vocabulary.rs` proving:

- The browser action collection contains only supported actions.
- Remediations come first, followed by valid core actions in stable order.
- Duplicate action ids are removed.
- Action entries retain label, destructive, and confirmation metadata.
- Request-time action validation still rejects unsupported action ids.

### Code to implement

- Replace status-bearing `WebActionState` output with browser-executable action
  metadata.
- Remove unsupported-action status/reason fields from response construction.
- Keep `supported_browser_action` as the request validation boundary.
- Do not change core operation eligibility.

### Verification

```sh
rtk cargo nextest run -p ajax-web action_vocabulary::tests
```

## Task 10: Simplify the Ajax Web Status DTO

### Failing behavior test

Add serialization tests in `crates/ajax-web/src/slices/cockpit.rs` proving:

- Card and detail payloads expose identical `status` and
  `status_explanation` values.
- Status serializes only as `running`, `waiting`, `idle`, or `error`.
- Card payloads omit `ui_state`, `status_label`, `live_summary`,
  `primary_action`, `available_actions`, and `action_states`.
- The single `actions` collection contains only browser-supported actions.
- Action entries retain label, destructive, and confirmation metadata.
- Detail diagnostics may retain lifecycle/live/pane inputs without changing the
  canonical status fields.

### Code to implement

- Replace the current browser card/detail status fields with the canonical pair.
- Replace parallel action fields with one ordered `actions` collection.
- Update operation responses that return refreshed Cockpit state.

### Verification

```sh
rtk cargo nextest run -p ajax-web slices::cockpit::tests
rtk cargo nextest run -p ajax-web runtime::tests
```

## Task 11: Render Canonical Status in the Web List and Inbox

### Failing behavior test

Update static-asset contract tests in `crates/ajax-web/src/slices/install.rs`
proving bundled JavaScript/CSS:

- Reads only `status` and `status_explanation` for list rows and inbox cards.
- Supports exactly four status labels/tones.
- Does not contain `STATUS_META` entries for legacy states or fallback from
  lifecycle/live/pane values.
- Uses the backend `actions` collection directly and renders no disabled
  unsupported actions.
- Does not reference `primary_action`, `status_label`, `live_summary`,
  `available_actions`, or `action_states` in list rendering.

### Code to implement

- Replace legacy status metadata/order with the four canonical statuses.
- Share one status rendering helper between task rows and inbox cards.
- Use `status_explanation` as the only optional list/inbox subtitle.
- Treat the first supplied action as visually prominent without inventing a
  primary action contract.
- Reduce CSS status tones to running, waiting, idle, and error while leaving
  unrelated browser connection/progress styles intact.

### Verification

```sh
rtk cargo nextest run -p ajax-web slices::install::tests
```

## Task 12: Render Canonical Status in Web Detail Without Losing Interaction

### Failing behavior test

Add static-asset contract tests in `crates/ajax-web/src/slices/install.rs`
proving:

- The detail hero reads the same `status` and `status_explanation` as cards.
- `INTERACT_STATE_COPY` and live-kind-to-headline translation are absent.
- Pane/live kinds may still select approval/input controls and diagnostic text.
- Lifecycle and raw live status remain secondary diagnostics.
- The action band renders only supplied valid actions and no unsupported-action
  notes.

### Code to implement

- Make the detail hero use the shared canonical status renderer.
- Remove the separate interaction-state headline vocabulary.
- Keep `WaitingForApproval`/`WaitingForInput` checks only for guarded pane
  controls and descriptive interaction content.
- Use status explanation in the current-status card and raw live/pane data in
  diagnostics/milestones only.
- Render the first supplied action as prominent and remove disabled-action copy.

### Verification

```sh
rtk cargo nextest run -p ajax-web slices::install::tests
rtk cargo nextest run -p ajax-web
```

## Task 13: Update Architecture and Graph Documentation

### Documentation to update

Update `architecture.md` to record:

- The canonical four-state operator projection and explanation contract.
- The distinction between status, lifecycle, annotations, and capabilities.
- Durable reduced-live observation timestamps and v7 SQLite persistence.
- Agent-neutral, non-destructive acknowledgement semantics.
- Cockpit inbox inclusion from canonical Waiting/Error status.
- Ajax Web's single status/action DTO and prohibition on browser-side status
  derivation.

Because this changes cross-module projection flow, refresh `graphify-out/` with
the repository's documented Graphify update command after code and docs are
green. Do not hand-edit generated Graphify artifacts.

### Verification

```sh
rtk rg -n "Running|Waiting|Idle|Error|acknowledg|observed_at|Web Cockpit" architecture.md
rtk graphify extract --update
rtk git diff --check
```

Verify only expected generated Graphify artifacts changed.

## Task 14: Full Validation and Contract Inspection

### Test to run

No new test is introduced in this task. Fix only implementation defects exposed
by validation; do not weaken assertions or perform unrelated refactors.

### Verification

```sh
rtk cargo fmt --check
rtk cargo check --all-targets --all-features
RUSTFLAGS="-D warnings" rtk cargo check --all-targets --all-features
rtk cargo check --no-default-features
rtk cargo clippy --all-targets --all-features -- -D warnings
rtk cargo nextest run --all-features
rtk cargo doc --no-deps --all-features
RUSTDOCFLAGS="-D warnings" rtk cargo doc --no-deps --all-features
```

If `cargo audit` is installed, also run:

```sh
rtk cargo audit
```

Inspect representative serialized card/detail payloads from the Web slice tests
and confirm:

- exactly one of `running`, `waiting`, `idle`, or `error`;
- zero or one matching plain-language explanation;
- identical status fields in card and detail;
- only executable browser actions;
- no legacy Web status/action fields.

## Out of Scope

- Replacing lifecycle, task events, substrate evidence, or persistence enums.
- Treating lifecycle names as visible statuses.
- Adding a frontend reason enum parallel to the explanation string.
- Redesigning browser connection, toast, or operation-progress state.
- Rewriting pane classification beyond acknowledgement and headline separation.
- Changing operation eligibility or lifecycle transitions except where trusted
  live evidence already owns those transitions.
- Editing integration/smoke files under any `tests/` directory, including
  `crates/ajax-cli/tests/smoke_user_flows.rs`.
