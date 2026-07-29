# ACP v2 Status Replacement Plan

Date: 2026-07-28  
Status: approved and resumed 2026-07-28

## Intent

Replace Ajax's provider hooks, canonical JSONL event stream, wrapper runtime
snapshot, pane inference, and generic evidence reducer with one Agent Client
Protocol (ACP) v2 session host. Enhance status explanations with authoritative
tool, plan, permission, and context-usage detail.

This is a clean cutover:

- ACP v2 is the only agent runtime/status authority.
- There is no ACP v1 negotiation, hook/event compatibility, old-cache reader,
  pane fallback, or native-agent fallback.
- A v1-only or otherwise incompatible adapter fails closed with an actionable
  protocol error.
- Existing legacy cache files and user-installed hooks are left untouched, but
  Ajax never reads or invokes them.

ACP below means **Agent Client Protocol**, not a new Ajax-specific protocol.

## Scope

- Add a foreground Ajax ACP client/terminal host for task tmux windows.
- Launch installed ACP adapters for the selected agent.
- Start or resume one ACP session and consume v2 `state_update` notifications.
- Persist one atomically replaced runtime snapshot per task.
- Project ACP state through core into the existing `LiveStatusKind`,
  `TaskStatus`, `TaskCard`, and attention/notification paths.
- Include current tool, plan progress, permission/input request, and context
  usage in the existing status explanation.
- Remove all superseded status producers, caches, commands, socket plumbing,
  provider capability logic, and prompt parsing.
- Update `architecture.md` and affected operator/development documentation.

## Non-goals

- No browser-owned session state, browser composer, service worker, or alternate
  Web Cockpit terminal model.
- No new public task/card schema; enhanced detail uses the existing status
  explanation.
- No ACP registry downloader, adapter installer, or network-on-launch behavior.
- No provider-specific `_meta` support until a concrete requirement exists.
- No changes to durable task lifecycle semantics; `idle/end_turn` means
  response-ready, not lifecycle completion.
- No rewrite of the separate one-shot `ajax-supervisor` state machine.
- No automatic deletion of old cache data or mutation of users' agent config.

## Architecture

```text
task tmux window
  -> ajax __agent-acp
       -> installed ACP v2 adapter over stdio JSON-RPC
       -> existing terminal stdin/stdout for prompts, output, cancel,
          permission, and elicitation
       -> atomic cache/agent-acp/<task>.json snapshot + heartbeat
  -> runtime refresh reads the ACP snapshot
  -> ajax-core projects authoritative ACP state
  -> existing CLI/Web task card and notification projections
```

Core continues to own task truth and headline selection. The CLI host owns wire
I/O and snapshot persistence, but it does not invent a user-facing status.
Web Cockpit continues to stream the tmux terminal and consume backend task
projections.

Use the official Rust SDK pinned to `agent-client-protocol = "=2.0.0"` with its
`unstable_protocol_v2` feature. The exact pin is intentional because ACP v2 is
currently a draft. Do not hand-roll JSON-RPC.

## ACP Projection Contract

| ACP observation | Ajax live status | Explanation/detail |
| --- | --- | --- |
| adapter start, initialize, session new/resume | `AgentRunning` | connecting/resuming selected agent |
| session `running` | `AgentRunning` | active tool, plan progress, context usage when present |
| permission request / permission-required action | `WaitingForApproval` | request title and safe option labels |
| elicitation or other required user data | `WaitingForInput` | requested input summary |
| `idle` + `end_turn` | `Done` | response ready; no completion notification |
| `idle` + `cancelled` | `ShellIdle` | cancelled |
| `idle` + `max_tokens` | `ContextLimit` | token limit reached |
| `idle` + `max_turn_requests` | `ContextLimit` | turn limit reached |
| `idle` + `refusal` | `Blocked` | agent refused the request |
| authentication challenge/failure | `AuthRequired` | adapter-provided safe summary |
| protocol/version/transport failure or unexpected child exit | `CommandFailed` | actionable failure summary |
| unknown future state/stop reason | `Unknown` | preserved safe protocol value; never panic |

Rules:

- A failed tool is detail while the session is still `running`; it does not
  override the foreground session into task error.
- Background tool updates received while the session is `idle` update detail
  only and do not fabricate `AgentRunning`.
- Plan entries are reduced by ACP IDs and statuses; the explanation reports a
  compact completed/total count and current item.
- Usage reports compact used/size context information; missing usage is simply
  omitted.
- Only approval, input, authentication, and execution failures are actionable
  notifications. Tool/plan/usage detail and `Done` remain silent.
- A stale or malformed snapshot cannot fabricate activity. A live host refreshes
  its heartbeat even when the ACP session is idle.

## Adapter Launch Contract

Use fixed, documented defaults and PATH lookup only:

| Ajax selection | Adapter command |
| --- | --- |
| Codex | `codex-acp` |
| Claude | `claude-agent-acp` |
| Cursor | `cursor-agent acp` |
| Pi | `pi-acp` |
| Other | the requested executable, with no inferred arguments |

An installed adapter that does not negotiate ACP v2 is an explicit launch
failure. There is no v1 or native CLI fallback.

## Test-file Authorization Needed

Approval of this plan explicitly authorizes edits only to these existing files
under a `tests/` directory:

- `crates/ajax-cli/tests/live_cli.rs`
- `crates/ajax-cli/tests/smoke_user_flows.rs`

Those edits will replace native-wrapper/hook expectations with ACP host
expectations. Assertions will not be deleted or weakened. No other `tests/`
directory file may be modified without separate approval.

## Delegation Decision

Delegation decision: not delegated because this turn is planning-only. After
approval, rerun `model-router`, create a complete TDD implementation packet,
and delegate one bounded task at a time as required by `AGENTS.md`. The primary
agent reviews every diff and runs validation personally.

Approval update: the user approved delegation through completion on 2026-07-28.
This is standing continuation approval between tasks; the red/green result for
every task must still be recorded before the next task starts.

Execution delegation decision: delegated via model-router.

## TDD Task Checklist

Each task is sized for roughly 5–15 minutes. After every task, update this
ledger with the failing and passing commands/results and stop for the required
user continuation approval.

- [x] **1. Pin the v2 SDK and fail closed during negotiation**
  - Test: add inline CLI tests with a scripted ACP peer proving v2 initializes
    and a v1/unsupported peer produces a typed protocol-mismatch failure.
  - Implement: pin the SDK/feature and add the smallest hidden ACP host command
    needed to initialize an adapter over stdio.
  - Verify: `cargo test -p ajax-cli agent_acp::tests::negotiation`

- [x] **2. Make core the sole ACP-to-Ajax status projector**
  - Test: add table-driven core tests for every state/stop mapping above,
    authentication, missing values, and unknown extensible values.
  - Implement: replace the generic evidence/status reducer with one small ACP
    observation model and a pure projection into existing `LiveObservation`.
    Keep the old reducer temporarily compileable for its legacy hidden-command
    consumers; remove it from runtime refresh in task 5 and delete it with those
    consumers in task 11.
  - Verify: `cargo test -p ajax-core acp_status::tests::projects`

- [x] **3. Fold enhanced tool, plan, and usage detail**
  - Test: prove tool upserts, plan replacement/progress, usage updates, failed
    background tools, and out-of-order unknown updates produce compact detail
    without changing the authoritative foreground state.
  - Implement: add one session fold keyed by ACP IDs; retain only detail needed
    by the console and status explanation.
  - Verify: `cargo test -p ajax-cli agent_acp::tests::folds_updates`

- [x] **4. Persist one atomic snapshot with a trustworthy heartbeat**
  - Test: use a temp directory to prove atomic replacement, round-trip,
    generation mismatch, malformed input, and stale-running rejection.
  - Implement: write/read `cache/agent-acp/<task-stem>.json` via temp-file
    rename; update it on meaningful changes and a modest heartbeat interval.
  - Verify: `cargo test -p ajax-cli agent_acp_snapshot::tests`

- [x] **5a. Replace core runtime refresh input with observed ACP status**
  - Test: prove refresh derives running/waiting/done/failure from the ACP
    observation model and preserves acknowledgment suppression.
  - Implement: remove `AgentStatusSource`/`NoAgentStatusSource`; accept one
    concrete task-ID map of timestamped ACP observations and apply the core
    projector as authoritative runtime evidence.
  - Verify: `cargo test -p ajax-core runtime_refresh::tests::acp`

- [x] **5b. Cut CLI and Web refresh over to ACP snapshot files**
  - Test: prove fresh snapshot files feed refresh while legacy JSONL/runtime
    files and stale activity do not fabricate status.
  - Implement: collect the per-task ACP snapshots for CLI/Web refresh, call the
    concrete core ACP path, and delete `agent_status_cache.rs`.
  - Verify: `cargo test -p ajax-cli agent_acp_snapshot::tests::collects` and
    `cargo test -p ajax-cli cockpit_backend::tests::acp`

- [x] **6. Start, resume, close, and fail sessions correctly**
  - Test: scripted peers prove `session/new` without a valid cached session,
    `session/resume` with a matching session, graceful close/EOF, cancellation,
    and unexpected adapter exit.
  - Implement: keep the session ID in the same runtime snapshot; add the minimal
    host lifecycle and failure reporting.
  - Verify: `cargo test -p ajax-cli agent_acp::tests::session_lifecycle`

- [x] **7. Preserve terminal-first prompting, output, and cancellation**
  - Test: a scripted terminal/peer proves user input calls `session/prompt`,
    agent message/terminal output reaches stdout, interrupt sends ACP cancel,
    and a queued final idle update is folded before simultaneous adapter EOF is
    classified.
  - Implement: add a minimal terminal loop using existing terminal primitives;
    do not create a second UI framework or browser composer.
  - Verify: `cargo test -p ajax-cli agent_acp_console::tests::prompt_output_cancel`
  - Delegation decision: delegated via model-router using the READY packet
    `.planning/agent-plans/packets/acp-07-terminal-console.md`.

- [x] **8. Handle approval, elicitation, and authentication at the trust boundary**
  - [x] **8a1. Permission requests**
    - Test: prove displayed choices map back to exact ACP option IDs, unknown
      input never becomes approval, and pending permission immediately
      projects actionable status with safe labels.
    - Implement: add one concrete pending-permission path through the existing
      terminal console.
    - Verify: `cargo test -p ajax-cli
      agent_acp_console::tests::requests_permission`
    - Delegation decision: delegated to Cursor via model-router using the READY
      packet `.planning/agent-plans/packets/acp-08a1-permission.md`.
  - [x] **8a2. Typed form elicitation requests**
    - [x] **8a2a. Form transport and pending interaction**
      - Test: prove form-only capability advertisement, session-scoped empty
        form input, immediate input-required state, and fail-closed unsupported
        mode/scope behavior.
      - Implement: enable form elicitation and minimally generalize 8a1's
        one-active terminal request path; accept only an empty object for an
        empty schema in this slice.
      - Verify: `cargo test -p ajax-cli
        agent_acp_console::tests::requests_elicitation_transport`
    - [x] **8a2b. Required string and integer form values**
      - Test: prove required string/integer values are typed, malformed input
        stays pending, unknown keys fail safely, and entered values never print.
      - Implement: add local JSON-object conversion for string/integer fields
        and required/unknown-key checks.
      - Verify: `cargo test -p ajax-cli
        agent_acp_console::tests::requests_elicitation_values`
      - Delegation decision: delegated to Cursor via model-router using
        `.planning/agent-plans/packets/acp-08a2b-elicitation-values.md`.
    - [x] **8a2c. Remaining primitive constraints**
      - [x] **8a2c1. Number, boolean, and string-array values**
        - Test: prove the three remaining ACP primitive variants parse to exact
          typed content, labels remain safe, and unsupported constraints still
          decline.
        - Implement: extend the existing predicate/labels/converter only for
          unconstrained numbers/booleans and bounded-choice string arrays.
        - Verify: `cargo test -p ajax-cli elicitation_primitives`
        - Delegation decision: delegated to Cursor via model-router using
          `.planning/agent-plans/packets/acp-08a2c1-elicitation-primitives.md`.
      - [x] **8a2c2. Declared constraints and annotations**
        - Test: prove string choices/length, numeric bounds, array bounds, and
          fail-closed contradictory schemas without echoing supplied values.
        - Implement: enforce declared bounds/choices in the existing local
          converter and treat default/format as ignored annotations.
        - Verify: `cargo test -p ajax-cli elicitation_constraints`
        - Delegation decision: delegated to Cursor via model-router using
          `.planning/agent-plans/packets/acp-08a2c2-elicitation-constraints.md`.
    - Delegation decision: delegated to Cursor via model-router as sequential
      bounded packets beginning with
      `.planning/agent-plans/packets/acp-08a2a-compact-transport.md`.
  - [x] **8b. Authentication challenge and login**
    - [x] **8b1. Authentication success path**
      - Test: prove an auth-required new session displays only supported
        methods, invalid input calls nothing, exact selection logs in, and
        session start retries only after login succeeds.
      - Implement: add the minimal pre-session authentication selection/login
        loop and fixed failure outcomes.
      - Verify: `cargo test -p ajax-cli
        agent_acp_console::tests::requests_authentication_success`
      - Delegation decision: delegated to Cursor via model-router using
        `.planning/agent-plans/packets/acp-08b1-authentication-success.md`.
    - [x] **8b2. Authentication fail-closed coverage**
      - Test: compactly prove unsupported-only, login failure, and cancelled/
        closed input perform no forbidden login/retry and publish fixed safe
        outcomes; retain the original pre-implementation RED evidence.
      - Implement: tests only unless a focused assertion exposes a defect.
      - Verify: `cargo test -p ajax-cli
        agent_acp_console::tests::requests_authentication_failure`
      - Delegation decision: delegated to Cursor via model-router after 8b1.

- [x] **9. Cut task creation and Web start-over to the ACP host**
  - Test: first update the two explicitly authorized integration files so they
    fail while expecting `__agent-acp`, each fixed adapter command, `Other`
    passthrough, task handle/cache arguments, and no native wrapper flags.
  - Implement: replace `agent_launch_spec`/`agent_runtime_command` call sites in
    core task creation and Web operation with the ACP host launch contract, then
    delete the superseded wrapper runtime.
  - Verify: `cargo test -p ajax-cli --test live_cli` and
    `cargo test -p ajax-cli --test smoke_user_flows`
  - Delegation decision: delegated to Cursor via model-router using the READY
    packet `.planning/agent-plans/packets/acp-09-launch-cutover.md`.

- [x] **10. Enhance existing explanations without expanding public schemas**
  - Test: prove CLI/Web cards retain the canonical headline while explanations
    show compact tool/plan/usage detail; approval/input/error notify and
    done/detail-only updates stay silent.
  - Implement: feed the richer core projection through the existing task-card
    and attention fields only.
  - Verify: `cargo test -p ajax-core ui_state::tests` and
    `cargo test -p ajax-web attention`
  - Delegation decision: delegated to Cursor via model-router using the READY
    packet `.planning/agent-plans/packets/acp-10-enhanced-explanations.md`.
  - Fixture correction delegation: after two Task 10 rounds exposed one final
    shared Web operation fixture, delegated the one-field tests-only correction
    to a fresh Cursor task using
    `.planning/agent-plans/packets/acp-10b-operation-fixture.md`.

- [x] **11. Remove the remaining legacy surface and align documentation**
  - Test: add a failing CLI/core contract test proving `agent-hooks`,
    `__agent-event`, and `__agent-runtime` are unavailable and legacy cache
    contents cannot affect status.
  - Implement: delete the remaining hook installer, provider event translator,
    notify socket, pane/prompt fallback, capability matrix, obsolete module
    exports, and Web socket drain. Remove hook reinstall behavior from
    development scripts and update `architecture.md` with ACP ownership,
    v2-only negotiation, snapshot authority, mapping rules, and unchanged
    tmux/Web boundaries. Leave old on-disk user files untouched.
  - Verify: run the focused contract test, `cargo check --all-targets
    --all-features`, `cargo fmt --check`, and
    `rg "agent-hooks|__agent-event|__agent-runtime|agent-events|agent-runtime"
    architecture.md scripts crates`, which must return only intentional negative
    tests or migration notes.
  - Source-removal delegation: delegated to Cursor via model-router using the
    READY packet
    `.planning/agent-plans/packets/acp-11a-legacy-source-removal.md`.
  - [x] 11a source removal: retired commands/modules/socket removed; surviving
    delegated-waiting helpers relocated into `live.rs`.
  - [x] 11b legacy-cache negative integration and development-script cleanup.
  - [x] 11c architecture documentation alignment.

- [x] **12. Clear final ACP clippy findings without behavior changes**
  - Test: no new test is meaningful for lint-only simplifications; preserve the
    failing `cargo clippy --all-targets --all-features -- -D warnings` result
    and run the existing elicitation, lifecycle, and console behavior filters.
  - Implement: simplify the reported boolean, remove redundant trait bounds and
    let binding, accept `&Path`, and scope the test mutex guard before `await`.
  - Verify: `cargo clippy --all-targets --all-features -- -D warnings`,
    focused ACP regressions, `cargo fmt --check`, and `git diff --check`.
  - Delegation decision: delegated to Cursor via model-router using the READY
    packet `.planning/agent-plans/packets/acp-12-clippy-cleanup.md`.

## Final Validation

Run after every task is approved and complete:

```bash
cargo fmt --check
cargo check --all-targets --all-features
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
npm run verify
```

If `cargo nextest` is unavailable, run `cargo test --all-features` and record
the substitution. Do not open a PR unless the separate full local PR gate in
`AGENTS.md` is requested and passes.

Also perform a manual task-window smoke test for every installed adapter that
negotiates v2. Record v1-only adapters as expected fail-closed compatibility
failures, not as successful integrations.

Final result:

- [x] `cargo fmt --check`
- [x] `cargo check --all-targets --all-features`
- [x] `cargo clippy --all-targets --all-features -- -D warnings`
- [x] `cargo nextest run --all-features` — 1,668 passed
- [x] `npm run verify` — Rust/doc/Web/CI checks passed; 52 Vitest files,
  485 Web tests, and 11 script tests passed
- [x] installed-adapter audit — only `cursor-agent` was installed; it negotiated
  v1 and Ajax failed closed as required, so no installed v2 adapter was
  available for a successful task-window smoke

## Known Risks and Decisions

- ACP v2 is a draft. The official guidance recommends dual-version migration,
  but this plan intentionally rejects that because the requested cutover has no
  legacy compatibility.
- The current public ACP registry matrix demonstrates ACP support, not v2
  negotiation. Codex, Claude, Cursor, or Pi may therefore fail closed until
  their installed adapters support v2.
- Existing Ajax-installed hooks may remain in users' agent config. This plan
  does not mutate external config or retain an uninstall compatibility command;
  operators must remove those hooks before cutover.
- Replacing native agent TUIs with the Ajax ACP terminal host changes the
  program inside the existing tmux/xterm path, but does not replace that
  terminal transport or make the browser a source of truth.
- The exact SDK pin must be consciously updated alongside protocol tests when
  ACP v2 changes.

## Deviations and Current Blocker

- Task 1 delegate round 1 (`pi-delegate`, GLM) was discarded after the tool
  looped without returning the required structured report.
- Task 1 delegate round 2 (`codex-delegate`) proved the focused tests RED at an
  unimplemented handshake stub, then GREEN after implementation; its
  `cargo check -p ajax-cli` and final `cargo fmt --check` also passed.
- Round 2 returned `STATUS: BLOCKED` because adding the required ACP SDK expanded
  generated `Cargo.lock` content by roughly 700 lines and triggered the packet's
  roughly-400-line stop condition. The router forbids accepting a blocked report
  or starting a third delegate round.
- Both delegate deltas were restored exactly through the router snapshot tool.
  No source, manifest, lockfile, or test change from either round remains.
- The user explicitly approved a fresh routing audit with generated
  `Cargo.lock` churn excluded from the roughly-400-line cap. Hand-written
  changes remain subject to the cap.

## References

- [ACP v2 draft announcement](https://agentclientprotocol.com/announcements/acp-v2-draft)
- [ACP v2 overview](https://agentclientprotocol.com/protocol/v2/overview)
- [ACP v2 migration guide](https://agentclientprotocol.com/protocol/v2/migration)
- [ACP Rust SDK](https://agentclientprotocol.com/libraries/rust)
- [ACP agent registry](https://agentclientprotocol.com/get-started/registry)
- [Current registry protocol matrix](https://github.com/agentclientprotocol/registry/blob/main/.protocol-matrix/latest.md)

## Execution Log

- 2026-07-28: inspected architecture and current hook/event/runtime/pane status
  pipeline; routed planning locally; no source code changed.
- 2026-07-28: user approved the plan and explicitly requested delegation until
  finished, including the two scoped integration-test files above.
- 2026-07-28: task 1 stopped after two failed/blocked delegate rounds; all
  delegate changes were discarded and the plan was marked blocked.
- 2026-07-28: user approved excluding generated `Cargo.lock` churn from the
  delegation line cap and authorized a fresh Task 1 routing audit.
- 2026-07-28: task 1 accepted. Delegate RED:
  `rtk proxy env CARGO_NET_OFFLINE=true cargo test -p ajax-cli
  agent_acp::tests::negotiation -- --nocapture` (exit 101; both tests reached
  the handshake stub). Delegate GREEN: the same focused test without the stub
  (exit 0; 2 passed). Parent verification passed:
  `rtk cargo test -p ajax-cli agent_acp::tests::negotiation -- --nocapture`,
  `rtk cargo check -p ajax-cli`, and `rtk cargo fmt --check`.
- 2026-07-28: task 2 accepted. Delegate RED:
  `rtk cargo test -p ajax-core acp_status::tests::projects -- --nocapture`
  (exit 101; the table test reached the projector stub). Delegate GREEN: the
  same focused command (exit 0; 1 passed), followed by all three inline tests
  (exit 0). Parent verification passed:
  `rtk cargo test -p ajax-core acp_status::tests -- --nocapture`,
  `rtk cargo check -p ajax-core`, and `rtk cargo fmt --check`.
- 2026-07-28: task 3 accepted. Delegate RED:
  `rtk cargo test -p ajax-cli agent_acp::tests::folds_updates -- --nocapture`
  (exit 101; the update fold reached its stub). Delegate GREEN: the identical
  command (exit 0; 1 passed). Parent verification passed:
  `rtk cargo test -p ajax-cli agent_acp::tests -- --nocapture`,
  `rtk cargo check -p ajax-cli`, and `rtk cargo fmt --check`. `cargo check`
  reports four expected dead-code warnings because this bounded task could not
  wire the fold into the live connection; task 6 must remove them through use,
  not suppression.
- 2026-07-28: task 4 accepted. Delegate RED:
  `rtk cargo test -p ajax-cli agent_acp_snapshot::tests -- --nocapture`
  (exit 101; both publisher tests reached the atomic-claim stub). Delegate
  GREEN: the identical focused command (exit 0; 3 passed). The delegate report
  extractor returned `MISSING_STRUCTURED_REPORT` after validation, so the raw
  command evidence and scoped delta were reviewed directly. Parent verification
  passed: `rtk cargo test -p ajax-cli agent_acp_snapshot::tests -- --nocapture`,
  `rtk cargo check -p ajax-cli`, and `rtk cargo fmt --check`. The interim
  dead-code warnings are expected until tasks 5 and 6 wire the reader and
  publisher.
- 2026-07-28: task 5 was split into bounded core (5a) and CLI/Web filesystem
  (5b) cutovers after call-graph review. The legacy reducer cannot be deleted
  before its hidden event command without either breaking intermediate builds
  or adding a compatibility shim, so task 5 removes it from runtime authority
  and task 11 deletes it with the remaining legacy command surface.
- 2026-07-28: task 5a accepted after one focused review revision. Initial
  delegate RED reached the new ACP refresh stub (exit 101), then GREEN passed
  the focused ACP test and the 34-test runtime-refresh suite. Parent review
  found identical Running observations skipped recovery-flag repair. The fresh
  revision delegate proved RED with `AgentRunning` still absent (exit 101),
  then GREEN after repairing recovered substrate flags without re-stamping the
  observation time (exit 0). The report extractor again returned
  `MISSING_STRUCTURED_REPORT`; the complete report was present in the raw log,
  and the scoped delta touched only `runtime_refresh.rs`. Parent verification
  passed: `rtk cargo test -p ajax-core
  runtime_refresh::tests::acp_statuses -- --nocapture` (1 passed),
  `rtk cargo test -p ajax-core runtime_refresh::tests -- --nocapture`
  (34 passed), `rtk cargo check -p ajax-core`, and `rtk cargo fmt --check`.
- 2026-07-28: task 5b initial delegate round proved the collector RED
  (missing `collect_statuses`, exit 101), then GREEN (1 passed), cut CLI/Web
  refresh to ACP snapshots, and deleted `agent_status_cache.rs`. Its focused
  verification passed, but parent `rtk cargo test -p ajax-cli` found seven
  surviving inline expectations for hook-era lowercase/wording summaries
  (exit 101; 332 passed, 7 failed). Gate: REVISE. A narrow assertion-only
  delegate round must update those exact expectations to the already-approved
  ACP summaries and make the full crate suite pass without production edits.
- 2026-07-28: task 5b assertion revision changed exactly the seven allowed
  literals. Its sandboxed full-suite run then exposed two second-frame
  expectations that had been unreachable behind the first failures:
  `Input required` versus `waiting for input`. Three other sandbox-only
  permission/log failures were outside the packet. Parent reproduced both
  second-frame failures independently (exit 101 each). A final two-literal
  test-only micro-delegation was created; production remains frozen.
- 2026-07-28: task 5b accepted. The final micro-delegate changed exactly two
  second-frame expected literals and both focused tests passed. Parent review
  confirmed every delegate delta stayed within its allowlist and accepted the
  deletion of `agent_status_cache.rs`. Parent verification passed all packet
  commands, `rtk cargo test -p ajax-cli --lib` (339 passed), `rtk cargo check
  -p ajax-cli`, `rtk cargo fmt --check`, and `rtk git diff --check`. A broader
  `rtk cargo test -p ajax-cli` reached 339 passing unit tests but failed one
  pre-existing `crates/ajax-cli/tests/live_cli.rs` integration expectation
  because that test still writes a retired native event file; the plan's
  explicitly authorized integration cutover will replace that fixture in task
  9. No production compatibility reader was restored.
- 2026-07-28: task 6 accepted after one review revision. Round 1 proved the
  focused lifecycle filter RED then implemented the one-file host, but stopped
  without the required typed-peer new/resume/EOF coverage. The revision added
  those tests and proved a second focused RED (6 passed, generation-race
  failure), then GREEN after retaining the known session ID, propagating
  publisher-generation failure, using typed EOF outcomes, and disabling the
  closed update-channel branch (7 passed). Parent verification passed:
  `rtk cargo test -p ajax-cli agent_acp::tests::session_lifecycle -- --nocapture`
  (7 passed), the fold and negotiation filters (1 and 2 passed),
  `rtk cargo test -p ajax-cli --lib` (346 passed), `rtk cargo check -p
  ajax-cli`, `rtk cargo fmt --check`, and `rtk git diff --check`. The report
  extractor again returned `MISSING_STRUCTURED_REPORT`; the complete report was
  present in the raw log. `cargo check` retains the temporary test-only
  `negotiate_v2` warning plus one legacy event warning; task 11 removes the
  latter surface, and the negotiation helper must be test-gated before final
  clippy. Task 7 must prioritize a queued final state update over simultaneous
  EOF when it expands the same select loop for terminal input.
- 2026-07-28: task 7 evidence traced the existing task terminal descriptors,
  ACP v2 prompt/cancel/update types, SDK incoming-close ordering, existing
  workspace Base64 dependency, and the lifecycle call graph. Model-router
  selected a bounded delegated session/terminal task; READY packet:
  `.planning/agent-plans/packets/acp-07-terminal-console.md`. The console stays
  line-input/raw-output inside the existing tmux/xterm transport and adds no
  compatibility path or second UI.
- 2026-07-28: task 7 packet scope was corrected after Cursor's first gate:
  adding the already workspace-pinned `base64` dependency to `ajax-cli`
  necessarily adds one generated dependency entry to the existing
  `Cargo.lock`. The packet now permits only that lock update; no dependency
  version or root-manifest change is allowed. Cursor's hand-written plus
  generated delta is 392 changed lines, within the packet ceiling.
- 2026-07-28: parent code review rejected Task 7's first implementation despite
  its green reported tests. The production console was constructed before the
  Tokio runtime was entered, prompt acknowledgement blocked the lifecycle
  select loop, and console/input errors bypassed the Failed snapshot path.
  The same Cursor chat received a focused TDD correction order at
  `.planning/agent-plans/packets/acp-07-terminal-console.review-fix.prompt.md`;
  no Task 8 interaction work is included.
- 2026-07-28: task 7 accepted after the focused Cursor correction. Its new RED
  failed at the unhandled typed console-input error (exit 101); GREEN passed
  four tests covering prompt/output/cancel/idle-before-EOF, output while the
  prompt response remained pending, and Failed snapshots for console input and
  output errors. Parent verification passed:
  `rtk cargo test -p ajax-cli
  agent_acp_console::tests::prompt_output_cancel -- --nocapture` (4 passed),
  lifecycle/fold/negotiation filters (7/1/2 passed), `rtk cargo test -p
  ajax-cli --lib` (350 passed), `rtk cargo check --locked -p ajax-cli`,
  `rtk cargo fmt --check`, and `rtk git diff --check`. The report extractor
  rejected Cursor's block-list YAML, but the complete structured report and
  command evidence are present in the preserved raw log. Locked check retains
  only the known legacy `parse_envelopes_from_jsonl` warning scheduled for Task
  11. Real TTY Ctrl-C/stdin behavior remains part of the final manual adapter
  smoke gate.
- 2026-07-28: task 8 was split into bounded permission/elicitation (8a) and
  authentication (8b) Cursor rounds after tracing the SDK request-dispatch and
  auth/session-start boundaries. The approved behavior is unchanged. Task 8a
  uses retained typed responders outside the SDK dispatch loop, one active
  terminal request, exact numeric-to-protocol-ID permission mapping, and one
  JSON-object line for typed ACP form input. Task 8b owns all pre-session
  authentication behavior.
- 2026-07-28: the first combined Task 8a Cursor round was discarded exactly
  through the delegate snapshot after its 660-line addition exceeded the
  packet limit, its focused test hung past 90 seconds, and it returned no
  structured report. The delta stayed inside its three-file allowlist and is
  preserved in the router artifact, but no part was accepted. Task 8a is now
  split again into permission (8a1) and elicitation (8a2); this changes only
  execution granularity, not approved behavior.
- 2026-07-28: task 8a1 accepted after one focused Cursor revision. Parent
  review removed the handler-side session-ID mutex race by keeping session
  validation in the lifecycle that owns the active session, and required the
  behavior test to await and assert the peer task. The final source delta is
  403 added lines, at the packet's roughly-400 ceiling, and touches only
  `agent_acp.rs` and `agent_acp_console.rs`. Parent verification passed:
  permission/prompt/lifecycle/fold filters (1/4/7/1 passed), `rtk cargo test -p
  ajax-cli --lib` (351 passed), `rtk cargo check --locked -p ajax-cli`, `rtk
  cargo fmt --check`, and `rtk git diff --check`. The report extractor again
  rejected Cursor's block-list YAML; the complete report is preserved in the
  raw log. Locked check retains only the known legacy
  `parse_envelopes_from_jsonl` warning scheduled for task 11.
- 2026-07-28: the first standalone Task 8a2 Cursor delta was discarded and
  restored from its delegate snapshot. It stayed within its three-file
  allowlist and proved focused RED/GREEN, but added 750 lines—nearly twice the
  packet's hard roughly-400-line stop—and omitted the required regression
  evidence from its report. The rejected patch remains preserved at
  `/var/folders/t0/f16kndkx2gs1_9ncbzfjhmh00000gn/T/tmp.T7l0R6BeCT`.
  Task 8a2 is now split into transport (8a2a), required string/integer values
  (8a2b), and remaining primitive constraints (8a2c); approved behavior and
  file boundaries are unchanged.
- 2026-07-29: the first Task 8a2a transport round and its focused revision were
  also restored from their original delegate snapshot. The revision corrected
  a required-without-properties fail-open edge and removed sleep-only
  assertions, but expanded the final source delta to 489 additions past the
  packet's hard 450-line ceiling. The replacement compact packet keeps the same
  production behavior, uses one transport integration case plus one pure schema
  predicate assertion, and forbids general test-helper growth.
- 2026-07-29: task 8a2a accepted from the compact Cursor reroute. Its source
  delta is 419 additions across only the root SDK feature declaration,
  `agent_acp.rs`, and `agent_acp_console.rs`. Focused RED was the absent
  empty-schema predicate (exit 101); GREEN covered form-only capability,
  input-required publication, safe invalid-input pending behavior, empty-object
  acceptance, URL decline, and the required-without-properties predicate.
  Parent verification passed the transport/permission/prompt/lifecycle/fold/
  negotiation filters (1/1/4/7/1/2), `rtk cargo test -p ajax-cli --lib` (353
  passed), `rtk cargo check --locked -p ajax-cli`, `rtk cargo fmt --check`, and
  `rtk git diff --check`. Locked check retains only the known Task 11 legacy
  warning. Cursor's raw report used the wrong outer schema, but preserved every
  command and exit code.
- 2026-07-29: task 8a2b accepted after one focused Cursor correction. The
  initial two-file delta proved three RED/GREEN values tests and stayed within
  scope, but review found that its unknown-field error echoed the user-entered
  JSON key. The same chat replaced it with the fixed safe message
  `Unexpected field.` and a distinctive-secret assertion. The final source
  delta is 445 additions. Parent verification passed values/transport/
  permission/prompt/lifecycle/fold filters (3/1/1/4/7/1), `rtk cargo test -p
  ajax-cli --lib` (355 passed), `rtk cargo check --locked -p ajax-cli`, `rtk
  cargo fmt --check`, and `rtk git diff --check`. Locked check retains the
  known Task 11 legacy warning. Both Cursor reports used the wrong outer schema;
  complete evidence is preserved in raw logs.
- 2026-07-29: the first Task 8a2c Cursor round was discarded and restored from
  `/var/folders/t0/f16kndkx2gs1_9ncbzfjhmh00000gn/T/tmp.rUKcd3sJ0l`.
  It stayed within its one-file allowlist and reported focused RED/GREEN plus
  green regressions, but its actual delta added 492 lines and explicitly hit
  the packet's 400-line stop. The rejected patch remains recoverable in that
  snapshot artifact. Task 8a2c is split into primitive variants (8a2c1) and
  constraints/annotations (8a2c2); behavior and file boundaries are unchanged.
  `rtk scripts/router-log --help` and a no-argument probe both exited 2 because
  the logger has no help mode and requires all fields; the valid discard record
  was then written explicitly. `rtk scripts/delegate-delta restore`, `rtk git
  diff --check`, and the post-restore source probe passed.
- 2026-07-29: task 8a2c1 accepted from Cursor. The deterministic delta touches
  only `agent_acp.rs` and adds 250 lines, below the 320-line stop. Focused RED
  failed three new primitive cases (exit 101); GREEN passed four tests covering
  number, boolean, plain/titled string arrays, safe errors, labels, and
  fail-closed deferred constraints. Parent verification passed primitive/
  values/transport/permission/lifecycle filters (4/3/1/1/7), `rtk cargo test -p
  ajax-cli --lib` (359 passed), `rtk cargo check --locked -p ajax-cli`, `rtk
  cargo fmt --check`, and `rtk git diff --check`. Locked check retains only the
  known Task 11 legacy warning. The first delta inspection command failed
  because the pre-snapshot was captured under the task label instead of the
  required `pre` label; renaming that manifest, capturing `post`, and rerunning
  the same inspection produced a clean one-file allowlist result.
- 2026-07-29: the first Task 8a2c2 Cursor round was discarded and restored from
  `/var/folders/t0/f16kndkx2gs1_9ncbzfjhmh00000gn/T/tmp.Rr5QGUUvvz`.
  Its report passed schema validation, the one-file allowlist was clean, and
  its focused/regression commands were green, but the actual patch added 377
  lines (net growth 332) and explicitly hit the 320-line stop. The semantic
  implementation was local; most excess was five verbose tests. The same
  Cursor chat will receive a restored-baseline resume order requiring compact
  table-driven coverage and fewer than 300 additions.
- 2026-07-29: task 8a2c2 accepted after a compact Cursor reroute and one
  test-coverage correction. The final source delta is 257 additions and touches
  only `agent_acp.rs`; the combined snapshot also lists the parent-created
  review prompt because it was written after that snapshot's pre-state, while
  the dedicated revision snapshot proves Cursor changed only the allowed Rust
  file. Focused RED failed two of three new constraint tests (exit 101); GREEN
  covers string character bounds/enum/one-of, integer/number inclusive bounds,
  array item bounds, contradictory/unknown schema decline, ignored
  default/format annotations, optional omission, and non-echoing safe errors.
  Parent verification passed constraint/primitive/values/transport/permission/
  lifecycle filters (3/4/3/1/1/7), `rtk cargo test -p ajax-cli --lib` (362
  passed), `rtk cargo check --locked -p ajax-cli`, `rtk cargo fmt --check`, and
  `rtk git diff --check`. Locked check retains only the known Task 11 warning.
  The final Cursor report contained complete green evidence but used
  `TEST_FIRST: N/A`, so the extractor exited 65; the preserved raw report and
  parent reruns establish acceptance.
- 2026-07-29: the first Task 8b Cursor round was discarded and restored from
  `/var/folders/t0/f16kndkx2gs1_9ncbzfjhmh00000gn/T/tmp.IpmPbU461Y`.
  It produced real RED/GREEN evidence and green regressions in its raw log, but
  the report was not in the router schema and the actual source delta added 559
  lines, past the packet's 450-line stop. Review also found that closed/
  interrupted input returned Graceful while leaving the authentication snapshot
  pending, post-login retry errors could expose peer text, and lifecycle failure
  constants lived in the console module. Task 8b is split into a compact
  success implementation (8b1) and failure-only coverage (8b2); the original
  failed-before-production evidence covers both behaviors.
- 2026-07-29: task 8b1 accepted after a compact Cursor reroute and one test-race
  correction. The final source delta is 364 additions across only
  `agent_acp.rs` and `agent_acp_console.rs`, with 168 test additions. Focused
  RED timed out waiting for the absent Authentication snapshot (exit 101);
  GREEN proves stable Agent-method filtering, names-only prompt/detail, invalid
  input processing without login/retry, exact second-ID mapping, new/login/new
  order, and session ID publication only after retry. Production also uses
  fixed HostFailed summaries for unsupported methods, login failure,
  closed/interrupted/input-error cancellation, and post-login retry failure.
  Parent verification passed auth/permission/elicitation/prompt/lifecycle/
  negotiation filters (1/1/4/4/7/2), `rtk cargo test -p ajax-cli --lib` (363
  passed), `rtk cargo check --locked -p ajax-cli`, `rtk cargo fmt --check`, and
  `rtk git diff --check`. Locked check retains the known Task 11 warning.
  Cursor's report remained outside the router schema; complete raw evidence is
  preserved. The combined snapshot includes the parent-created review prompt,
  while the dedicated revision snapshot proves Cursor's correction touched
  only the console test file.
- 2026-07-29: the first Task 8b2 resume attempt produced no worktree delta.
  Cursor reconnected three times and ended with `resource_exhausted`, so the
  runner reported a missing terminal event and the router gate discarded it.
  `rtk git apply --numstat` on the intentionally empty generated patch exited
  nonzero with `No valid patches in input`; `rtk git diff --check` remained
  green. Task 8b2 is rerouted to a fresh Cursor chat with the same packet.
- 2026-07-29: the fresh Task 8b2 Cursor patch was also discarded and restored
  from `/var/folders/t0/f16kndkx2gs1_9ncbzfjhmh00000gn/T/tmp.9fsy9FZ0oL`.
  Its actual delta touched only the allowed console test file and all focused,
  regression, lib, check, format, and diff commands were green, but it added
  245 lines against the packet's hard 230-line stop (the report incorrectly
  claimed 229). The same chat receives a restored-baseline compact order
  requiring the same six cases and assertions in fewer than 210 additions.
- 2026-07-29: that compact resume produced no delta because the existing Cursor
  chat exhausted its resource after three reconnects and emitted no terminal
  event. The empty allowlist inspection passed; no restore was needed. The
  compact order is rerouted to a new Cursor chat because the failure was in the
  delegate session, not the packet or code.
- 2026-07-29: a fresh Composer 2.5 Cursor chat failed identically before making
  a delta. The fresh snapshot allowlist inspection is empty and clean. Per the
  Cursor lane's escalation rule, the unchanged compact packet is rerouted once
  to `grok-4.5-high`; this remains Cursor delegation and does not widen scope.
- 2026-07-29: task 8b2 and parent task 8 accepted from the Grok 4.5 High Cursor
  reroute. The deterministic delta touches only the console test module and
  adds 190 lines, below the 210-line compact stop. One table-driven test covers
  all six fail-closed branches with exact new/login counts, timeout-bounded
  lifecycle errors, Failed snapshots without session IDs, fixed details, and
  method/metadata/peer/input non-leakage. Parent verification passed failure/
  success/permission/elicitation/lifecycle filters (1/1/1/4/7), `rtk cargo
  test -p ajax-cli --lib` (364 passed), `rtk cargo check --locked -p ajax-cli`,
  `rtk cargo fmt --check`, and `rtk git diff --check`. Locked check retains the
  known Task 11 warning. The extractor rejected duplicated streaming envelope
  fragments, but the raw log contains the complete report and command evidence.
- 2026-07-29: task 9 tracing confirmed that core
  `new_task_plan_with_observation` is the single production owner of the native
  launch builder/wrapper and that Web start already consumes the same core
  plan. The replacement therefore uses one core ACP host launch builder rather
  than Web-specific logic. `search_decisions` first failed because the
  codebase-intel query parser treated `start-over` as syntax; the simplified
  query succeeded with no conflicting decision. The first packet check exited
  1 for three required heading names; adding those headings without changing
  scope made `rtk scripts/check-packet
  .planning/agent-plans/packets/acp-09-launch-cutover.md` pass.
- 2026-07-29: task 9 accepted from Cursor. The isolated delta touches exactly
  the eight allowed files and contains 181 additions/174 deletions. The two
  explicitly authorized integration tests first failed against the legacy
  runtime-wrapper launch (both exit 101), then passed after core switched to
  one `agent_acp_launch_spec`. Fixed Codex/Claude/Cursor/Pi mappings and exact
  Other passthrough are covered; planned launches contain no worktree, prompt,
  `--cd`, permission-skip, or native Cursor `agent` flags. Parent verification
  passed the two integrations (1/1), core new-task/adapter filters (23/1), Web
  start filter (8), full core/Web/CLI library suites (814/184/364), locked
  three-crate check, format, and diff checks. The final grep finds only the
  hidden legacy CLI definition/dispatch and its dedicated command test owned by
  Task 11. Locked check retains the known Task 11 dead-code warning. Cursor's
  streaming output duplicated the otherwise complete report, so the extractor
  exited 65; the raw log and parent reruns establish acceptance.
- 2026-07-29: task 10 Cursor round 1 proved the exact enhanced-detail test RED
  (five canonical explanations observed instead of ACP detail) and GREEN (five
  cases passed). Its four-file delta stayed inside the original allowlist and
  focused core/Web suites passed, but parent reproduced eight intentional
  `ajax-cli --lib` expectation failures (exit 101; 356 passed) for the new ACP
  fallbacks. Review also rejected Cursor's attempt to mutate deserialized Web
  fixtures inside strict equality tests. The READY packet now explicitly
  permits only the two fixture value updates and eight inline CLI expectation
  updates needed by the changed explanation contract; production scope is
  unchanged. `rtk scripts/run-delegate --help && rtk
  scripts/delegate-snapshot --help` exited 1 because the second script requires
  positional arguments and has no help mode; its usage text was sufficient.
  Cursor's complete BLOCKED report is preserved in the raw log, while the
  extractor exited 65 on the fenced envelope.
- 2026-07-29: task 10 Cursor round 2 restored strict Web fixture equality,
  updated the two allowed fixture values and eight CLI inline expectations,
  made whitespace-only summaries use canonical fallback, and passed every
  focused/core/CLI/check/format/diff command. Parent reproduced the one
  remaining Web failure (exit 101, 0 passed/1 failed): the operation-response
  fixture shares `browser_contract_context` but was not in the allowlist.
  Because Task 10 reached the router's two-round cap, that single JSON
  replacement is split into the fresh tests-only Task 10b packet rather than a
  third round. The first attempt to discover the exact test via a piped
  `--list` command exited 1 without output; the direct filtered command exposed
  the expected RED. Cursor's raw log contains a complete BLOCKED report; the
  extractor again exited 65 on duplicated streaming output.
- 2026-07-29: task 10b and parent task 10 accepted. The fresh Cursor delta was
  exactly one line replaced in `operation.json`; the complete Task 10 source
  delta is 167 additions across ten allowed files, with eight CLI and three
  fixture changes being expectation/value replacements only. Original RED was
  five enhanced-detail cases failing against fixed canonical explanations
  (exit 101); GREEN passed all five. Parent verification passed the
  enhanced-detail, full UI-state, attention, projection, and browser-cockpit
  filters (5/48/35/13/18), the operation fixture (1), full core/Web/CLI library
  suites (819/185/364), locked three-crate check, format, and diff checks.
  Locked check retains only the known Task 11 `parse_envelopes_from_jsonl`
  warning. Strict fixture equality remains intact and public schema shape is
  unchanged. The initial Task 10b packet check exited 1 because tests-only
  metadata and two required headings used the wrong router contract; correcting
  them made the packet check pass. Cursor again emitted a complete report in
  its raw log while the streaming extractor exited 65.
- 2026-07-29: task 11a accepted from Cursor. Focused RED failed the new
  table-driven invalid-subcommand contract because `agent-hooks`,
  `__agent-event`, and `__agent-runtime` were still registered (exit 101);
  GREEN passed after the removal. The deterministic delta touches exactly the
  17 allowed files, adds 54 lines, deletes 4,183 lines, and removes all eight
  retired source files. Only the six existing delegated-waiting constants and
  helpers survive as crate-private items in `live.rs`. Parent verification
  passed the negative command test (1), delegated/live filters (2/16), full
  core/Web/CLI library suites (786/185/339), locked three-crate check, format,
  and diff checks. The legacy module/reference grep returned no matches with
  the expected `rg` exit 1. Cursor's streaming report extractor exited 65, but
  the raw log contains complete RED/GREEN and verification evidence. The first
  deterministic delta invocation omitted the required `inspect` verb and
  exited with usage text; rerunning it correctly proved a clean allowlist. The
  earlier `delegate-snapshot --help` and `router-log --help` discovery probes
  exited 2 because those scripts have no help mode; neither changed state.
- 2026-07-29: task 11b1 accepted from Cursor. The single authorized integration
  test now writes both retired cache shapes and proves they cannot set task
  `live_status` or create a waiting-for-approval inbox item. The preserved RED
  failed against the pre-removal reader (exit 101); GREEN passed the renamed
  exact test, full `live_cli` suite (11), CLI library suite (339), format, and
  diff checks. Cursor touched only `crates/ajax-cli/tests/live_cli.rs`.
- 2026-07-29: task 11b2 accepted from Cursor. The development restart script no
  longer detects, compares, reinstalls, or warns about retired agent hooks.
  Parent verification passed `rtk bash -n scripts/dev-web-restart.sh`, its Node
  test suite (2), the no-hook-reference grep, and diff check. The patch is the
  requested deletion with one harmless usage-line rewrap (+1/-39), recorded as
  a deviation from the packet's literal deletion-only wording.
- 2026-07-29: task 11c accepted after one focused Cursor review revision.
  `architecture.md` now specifies the exact ACP SDK pin and v2 feature, fixed
  adapter commands, ACP-only snapshot authority and mapping, generation versus
  freshness semantics, no terminal/pane fallback, response-ready lifecycle
  behavior, notification actionability, and the non-destructive legacy
  migration boundary. Parent verification found no stale design phrases,
  found retired surface names only in the migration note, passed the focused
  Web legacy-terminal contract (2), and passed `rtk git diff --check`. The
  initial revision runner invocation used unsupported `--follow-up` with
  Cursor and exited 2; the supported `--resume` invocation then produced the
  accepted one-file revision. Cursor's streaming report extractor again exited
  65, while its complete raw report and the parent reruns were green.
- 2026-07-29: final validation first passed format and all-target/all-feature
  check, then exposed eight `-D warnings` clippy findings in the new ACP host
  and console. Task 12 recorded that clippy result as its runnable RED because
  the correction was mechanical and no new behavior test was meaningful.
- 2026-07-29: Task 12 Cursor round 1 produced correct green source changes but
  was discarded and restored because Cursor launched nested routing, edited the
  parent plan, and created out-of-scope run artifacts. The first delta
  inspection also exited 1 because `post.json` had not yet been captured;
  capturing it and rerunning exposed the scope violation deterministically.
- 2026-07-29: Task 12 accepted from a strict source-only Cursor resume. The
  deterministic delta contains only the two allowed ACP source files plus the
  outer runner's raw/report outputs. Parent verification passed clippy with no
  allowances, elicitation/lifecycle/console filters (3/7/4), CLI library tests
  (339), format, and diff checks. The source patch is +24/-25 rather than the
  packet's 30-raw-line target because rustfmt reindents the required direct
  builder return; review confirmed the semantic change remains only the six
  prescribed lint edits. The report extractor missed the complete streamed
  report, so acceptance uses the raw log and parent reruns.
- 2026-07-29: final validation passed all five planned commands. The legacy
  surface grep returns only the architecture migration note and intentional
  negative/legacy-cache tests; the retired-module import/declaration grep
  returns no matches with expected `rg` exit 1.
- 2026-07-29: installed-adapter smoke found no `codex-acp`,
  `claude-agent-acp`, or `pi-acp`. Installed `cursor-agent`
  `2026.07.23-e383d2b` exposes `acp` but negotiated protocol v1; the hidden host
  exited 1 with the expected v2-only error and wrote a safe Failed snapshot.
  The temporary smoke snapshot/state directory was then deleted. The `rtk find`
  inspection warned that its `-print` flag was ignored but still found the
  snapshot and exited 0.
