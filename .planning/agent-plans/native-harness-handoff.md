# Handoff: native harnesses through one shared layer + in-session durability

Written 2026-10-13 from task `ajax-cli/durability-v2`
(`/Users/matt/Desktop/Projects/ajax-cli__worktrees/ajax-durability-v2`,
branch `ajax/durability-v2`). This task: `ajax-cli/native-harness-durability`,
branch `ajax/native-harness-durability`, based on `origin/main` @ `e872f534`.

## Goal (Matt's words)

In-session durability, native Cursor SDK, native Pi RPC, native Claude SDK
through a shared layer for Ajax web, while still supporting Codex ACP for
Ajax web.

Read as: every Ajax Chat harness except Codex runs on a native (non-ACP)
transport, all of them plug into ONE shared native layer
(`AgentEvent::Native(NativeEvent)` + `SessionClient`), Codex keeps ACP, and a
chat session survives an `ajax-web` restart/crash mid-session.

## ARCHITECTURE REVISION 2026-10-14, rev 2 (planning only; APPROVAL PENDING)

Evidence base: `origin/main` 964f945f plus PR #1293 head `45cb0291` (stack
#1289 → #1290 → #1291 → #1292 → #1293, none merged). Supersedes "Remaining
work" tasks 3 and 7 below and durability stage 3 (`HarnessRuntime` trait +
`Capabilities`). Nothing is implemented.

Rev 1 of this section (same day) proposed collapsing the enum to `Acp`/`Native`
behind a new `NativeAdapter` trait, a new `harness_factory.rs`, holding #1293,
and 8 PRs. **Withdrawn**: it relocated code into a trait and two new files
rather than deleting it, forced a test edit, and discarded reviewed work in
#1293. Rev 2 below reaches the same branching reduction with no new trait, no
new file, and no test edits.

### Verdict

**Keep `SessionClient` as an enum with one variant per harness. Do not add a
`HarnessSession` trait, a `NativeAdapter` trait, `Box<dyn …>`, a factory
module, or a registry.** Remove the duplicated arms with one private accessor
that routes the shared methods to the `RpcClientCore` all three native clients
already wrap.

| Question | Evidence | Answer |
| --- | --- | --- |
| Is the boundary sealed? | `SessionClient::<Variant>` appears nowhere outside `session_client.rs` | Yes. A trait adds nothing |
| Where is the cost? | 17 `match self` blocks; 50 arms with 3 variants, 66 with 4. Only 3 methods really differ per native harness (`begin_prompt`, `apply_model_pin`, `apply_config_option`), plus per-harness error strings in `respond_*` (asserted by tests) | In ~12 byte-identical native arms per method, not in the enum |
| Trait object instead? | 4 impls × ~15 methods ≈ 60 bodies vs 66 arms; ACP-only methods become stubs on 3 impls | Same size, more concepts, hides real differences |
| Will harness #5 arrive? | No plan names one. The set (Codex ACP, Pi, Claude, Cursor) is the stated goal | Optimize for four, keep a fifth cheap, no extension machinery |

The accessor (shape, final names at implementation):

```rust
// private, in session_client.rs
enum Transport<'a> { Acp(&'a mut AcpStdioClient), Native(&'a RpcClientCore) }

impl SessionClient {
    fn transport(&mut self) -> Transport<'_> {
        match self {
            SessionClient::Acp(c) => Transport::Acp(c),
            SessionClient::Pi(s) => Transport::Native(s.client.core()),
            SessionClient::Claude(s) => Transport::Native(s.client.core()),
            SessionClient::Cursor(s) => Transport::Native(s.client.core()),
        }
    }
    pub fn host_exited(&mut self) -> bool {
        match self.transport() {
            Transport::Acp(c) => c.host_exited(),
            Transport::Native(core) => core.host_exited(),
        }
    }
    // same 2-arm shape for: shutdown, detach, child_pid, poll_event, wait_event,
    // prompt_in_flight, cancel, session_id, take_available_commands
}
```

`RpcClientCore` methods all take `&self` (internal `Mutex`), so one accessor
serves `&self` and `&mut self` callers (a `&self` twin if the borrow checker
asks). Per-harness logic stays where it is: the three divergent methods keep
one arm per harness, and those arms call code already in the harness files.

Cost of a fifth native harness after this: its client/spawn/map files, one
variant, one `from_x`, one arm in `transport()`, one arm each in the three
divergent methods and the `respond_*` reason, one `HarnessTransport` variant
and launch row. That is roughly 25 lines in `session_client.rs`, down from
~115, with no change to slices or persistence.

### What the audit found (verified in source)

| # | Finding | Where | Action |
| --- | --- | --- | --- |
| D1 | Transport selection done twice with the same cfg(test) override gating copied 3× (`uses_pi_rpc`, `uses_claude_sdk`, `uses_cursor_sdk`), as if-chains in spawn and catalog | `session_client.rs`, `claude_sdk_spawn.rs`, `cursor_sdk_spawn.rs`, `catalog.rs` | One `transport_for(agent) -> HarnessTransport` in `session_client.rs`; spawn and catalog `match` on it (exhaustive) |
| D2 | `sidecar_launch_args` byte-identical in Claude and Cursor clients; `init` models/commands parsing duplicated; Cursor imports `claude_sdk_spawn::global_npm_root` | clients, `cursor_sdk_spawn.rs` | `global_npm_root` moves to the existing `adapters/program.rs` (already owns `resolve_program`, `harness_command`); launch args + parse helpers into existing `rpc_handshake.rs` |
| D3 | One-shot commands `Mutex<Option<Vec<NativeCommand>>>` + `take_available_commands` ×3 | 3 clients | Into `RpcClientCore` |
| D4 | Fail-closed resume-id equality check in 3 places | Pi in `session_client.rs`, Claude/Cursor in client `spawn` | Leave. Each is 5 lines, behaviour-identical, tested. Moving it is relocation |
| D5/D6 | `SpawnReport` built ×3, pin application ×3 | spawn modules | Leave. The constants differ (images) and the pin parsers differ (Cursor accepts legacy encodings). Merging needs parameters that restate the differences |
| D7 | **Suspected:** neither spawner uses a process group; the orphan sweep signals only the leader (`orphan_sweep.rs:81` says so) | `jsonl_process.rs`, `client.rs:837-905` | Reproduce first (see P3). Not yet a confirmed defect: sidecars exit on stdin EOF and grandchildren may exit on their own pipe EOF |
| L1 | `session_models` special-cases Cursor (`read_cursor_catalog_if_sdk`, `agent models` CLI fallback) | `slices/session_models.rs` | Disappears with the Cursor flip (P4); no separate PR |
| L2 | `acp_drain` reads `/_meta/piAcp/startupInfo` | `acp_drain.rs:103` | Known, not scheduled |
| L3 | Blanket `#![allow(dead_code)]` on now-live files | adapters | Remove where it compiles clean, in P2 |

Corrections to rev 1: the "5 agent-name tables" were not 5 copies. Two are
identical (`operate/mod.rs`, `model_change.rs`), `session_models` differs
(`""` → Cursor), and `task_pane_agent` maps process names. They are not harness
selection; out of scope.

**Kept as is, after challenge:**

- **Event normalization.** Two vocabularies (`AcpClientEvent` → `acp_map`;
  `NativeEvent` → `native_map`) meet in one `match` in `acp_drain`.
  Unifying means widening `NativeEvent` with ~6 ACP-only variants and moving
  568 lines for no reduction in branching.
- **Execution kernel / SQLite.** Harness-independent already; guarded.
- **No `Capabilities` struct.** The slice consumes exactly `restore_advertised`,
  `prompt_capabilities`, `config_options`, commands, all already in
  `SpawnReport`. `agent_runs.capabilities_json` is not written by the web host;
  leave it.
- **No reconcile hook.** No adapter can reconcile a dispatched prompt, so
  dispatched prompts become `interrupted` on recovery (`durable-execution.md`
  §5). Add a method when one adapter can (likely Pi `get_state`).
- **Sidecar JS duplication** (~150 lines of stdio framing in two `.mjs`):
  they ship as single-file `include_str!` sources; merging needs a bundler.
- **Directory rename** `web_session_acp/`: churn without behaviour.

### Ownership (one owner each)

| Responsibility | Owner | Not allowed |
| --- | --- | --- |
| Harness selection + construction | `session_client.rs` (`transport_for` + spawn `match`), keyed on core `HarnessTransport` | Slice choosing a transport or naming an adapter type |
| Process / SDK lifecycle | `JsonlProcess` (native), ACP client (ACP); `RpcClientCore` is the native `host_exited` truth | Slice signalling pids directly |
| Init + restore mechanics | Adapter spawn: pass id, verify echo, typed `RestoreFailure` | Silent fresh session |
| Restore policy | Slice `task_session_spawn` (when to resume, clear stored id, Retry/Start fresh) | Adapter deciding policy |
| Prompt admission, queue, idempotency | Slice + kernel | Adapter persisting |
| Wire send, `prompt_in_flight`, cancel | Adapter | |
| Event normalization + usage | Adapter emits `AgentEvent`; slice maps once per vocabulary and dedupes usage | |
| Capabilities + model config | Adapter via `SpawnReport` / `config_options()`; slice caches in `AcpSlot` and persists the model | `match harness` in the slice |
| Durable state + recovery | Kernel, written only by the slice | Adapter touching the registry |
| Abandoned processes | Normal: adapter drop. Crash: `orphan_sweep` (consumes identity the slice recorded), gated by the owner lease | |

**Durable vs provider-managed.** Durable (Ajax, SQLite): run row (harness,
model, `native_session_id`, child pid / start token / boot id), prompt queue,
events, owner lease. Provider-managed (Ajax never reads it): Claude session
transcript, Pi session file, Cursor agent store, ACP agent session. The only
bridge is the opaque `native_session_id`; restore passes it back and a
mismatch or not-found fails closed.

### Components

```text
 browser WS
    ▼
 slices::web_session   TaskSession actor · AcpSlot · acp_drain · acp_map / native_map
    │                  queue, transcript, restore policy, child identity, orphan_sweep
    │  calls only SessionClient's methods
    ▼
 adapters::web_session_acp::SessionClient   enum { Acp, Pi, Claude, Cursor }
    │  transport_for(agent) ─ spawn/catalog `match`   transport() ─ shared methods (2 arms)
    ├─ Acp ──► AcpStdioClient     permissions · elicitation · session/close · typed restore
    └─ Pi | Claude | Cursor ──► *Client (config + pin + launch, per harness)
                                   └─ RpcClientCore ► RpcSession ► JsonlProcess  (shared)
 ─────────────────────────────────────────────────────────────────────────────
 ajax-core: HarnessTransport + launch table · execution kernel (SQLite)
            knows no adapter types; adapters never import slice or registry
```

### Capabilities: explicit, never faked

| | ACP (Codex; Cursor today) | Claude SDK | Pi RPC | Cursor SDK |
| --- | --- | --- | --- | --- |
| Restore | load/resume, typed `Unsupported`/`Rejected`, `session/close` if advertised | `resume`; mismatch → Rejected | `--session-id`; mismatch → Rejected | `Agent.resume`; not-found/mismatch → Rejected |
| Detach | skips `session/close` | = shutdown | = shutdown | = shutdown |
| Permissions | interactive; cancel returns pending ids | auto-approved in sidecar | none | none |
| Elicitation | yes | declined in sidecar | none | none |
| Images | per agent | no (`[image omitted]`) | no | yes |
| Thinking | `agent_thought_chunk` | `AgentThought` | `thinking_delta` | `AgentThought` |
| Usage | `usage_update` | from `result` | `get_session_stats` at run end | none (SDK gives no context size; do not invent `size`) |
| Config | open-ended, pushed | model, effort | model, thought_level | model, per-model params, mode |
| Commands | pushed | `supportedCommands`, one-shot | `get_commands`, one-shot | scanned `.cursor/commands`, one-shot |

Native gaps vs ACP (permissions, elicitation, title) are recorded, not closed
here. `native-acp-parity.md` gap 2 changes a security assumption and stays a
separate decision; its task 5 "parity guard" is replaced by P1 + this table.

### Invariants (guard test or contract assertion)

1. Slices never name `AcpStdioClient`, `PiRpcClient`, `ClaudeSdkClient`,
   `CursorSdkClient` or `SessionClient::<Variant>` (extend the existing
   `architecture_web_session.rs` guards).
2. Transport selection is one exhaustive `match` on `HarnessTransport`; no
   `uses_*` helpers and no `AgentClient::X` comparisons in chat code paths
   after P4.
3. Shared methods on `SessionClient` have exactly two arms (`Acp`, `Native`)
   via `transport()`; per-harness arms only in `begin_prompt`,
   `apply_model_pin`, `apply_config_option`, `respond_*`.
4. Native files never name ACP types (existing guard; list already updated
   by #1291–#1293).
5. Adapters never import slice or registry; the kernel knows no adapter type.
6. Child identity is written only by the slice; `orphan_sweep` is its only
   reader.
7. Capability differences travel as `SpawnReport` data or absent events.

### Complexity

| Metric | Today | #1293 as is | Rev 2 |
| --- | --- | --- | --- |
| Variants | 3 | 4 | 4 |
| `match` arms in `session_client.rs` | 50 | 66 | ~40 (est.: 10 shared × 2 + 4 divergent × 4 + `transport()` 4) |
| `session_client.rs` lines | 552 | 667 | ~520 (est.) |
| New traits / files / `dyn` | — | — | 0 / 0 / 0 |
| Lines in `session_client.rs` per extra native harness | — | ~115 | ~25 (est.) |
| Copies: transport selector gating / sidecar launch args / commands one-shot | 2 / 1 / 2 | 3 / 2 / 3 | 1 / 1 / 1 |
| Test edits required | — | — | 0 |

### PR plan (≤600 lines/file, ≤1500/PR)

**Existing stack: merge #1289–#1292; fix #1293 before merging.**
Reviewed 2026-10-14 against the diffs:

| PR | Decision | Why |
| --- | --- | --- |
| #1289 orphan sweep | Merge | Independent, CI green, behind the owner lease. Group-kill gap is P3 |
| #1290 sidecar + plans | Merge after replacing its `native-harness-handoff.md` with this rev 2 and marking `native-acp-parity.md` task 5 superseded by P1 | JS + fixtures are fine; its plan copy is the stale rev 0 |
| #1291 mapper | Merge | Additive; the Claude client change is the `await_init_response` extraction, test-covered |
| #1292 client | Merge | Dead code behind `allow(dead_code)`; P2 removes its duplication |
| #1293 wiring | **Fix first** | Not behaviour-neutral: it deletes `read_cursor_acp_model_labels` / `overlay_cursor_acp_labels` while production Cursor still runs on ACP (launch table not flipped). Cursor picker labels regress from ACP names ("Grok 4.6") to `agent models` names ("Cursor Grok 4.6"), and 3 tests pinning a still-live mechanism are deleted (`cursor_model_labels_prefer_clean_base_names_over_bracket_variants`, `apply_cursor_label_overlay_replaces_cli_names_with_acp_names`, `…_fails_open_when_overlay_is_empty`). The approved test deletions cover a *removed* mechanism; this one is still live. Fix: restore the overlay and its tests in #1293 (keep the SDK branch ahead of it); move their deletion into P4 |

Status 2026-10-14: #1289, #1290, #1291 **merged** (squash). #1292 conflicted
with main because its parents were squash-merged; repaired by merging
`origin/main` into the branch (no force push). #1293 repaired the same way,
plus the Cursor ACP label overlay and its 3 tests restored.

**#1294 (`feat/web/cursor-native-sdk`, the flip): do not merge. Convert to
draft; it becomes P4.** Reviewed against its diff:

- It carries the whole stack again (#1289–#1293 commits) plus one flip commit
  `a9fe11c0`; after #1292/#1293 merge, rebuild it as only the flip.
- Flip gates are unmet: no live spike with a real `CURSOR_API_KEY`, and the
  "how does the key reach the ajax-web host" decision is open (this file's
  own condition, "DECIDED 2026-10-13").
- Test deletions overreach the approval (approval covers tests that pin only
  the removed Cursor-ACP mechanism):
  - `assert!(acp_launch_for_agent(AgentClient::Other).is_none())` removed —
    unrelated to Cursor; restore it.
  - `ws_bridge_tests::apply_client_message_set_config_option_persists_effort_and_fast_toggles`
    removed — it pins slice behaviour (effort/fast toggles persist into the
    pin without respawn) that Cursor SDK still has (`effort`/`fast` params).
    Port it to the Cursor SDK fixture instead of deleting it.
  - The rest (`issue_979/984/989/991/1079` spawn-arg tests, live `agent acp`
    tests) pin `agent acp --model` argv and are covered by the approval.
- CodeQL "cleartext logging of sensitive information" at
  `cursor_sdk_client_tests.rs:173` (`"error text: {err}"` with a fixture agent
  id) is a test-only false positive; dismiss it in the alert UI. `main` is not
  branch-protected, so it does not block.

The uncommitted flip WIP in `ajax-native-harness-durability` duplicates
`a9fe11c0`; discard it in favour of #1294 when P4 starts.

| PR | Scope | Files | Acceptance | Deps |
| --- | --- | --- | --- | --- |
| **P1** Contract suite (tests only) + this plan | Table-driven rows Acp, Pi, Claude now; Cursor row added once #1293 merges (one table entry). Each row drives `SessionClient::spawn_with_operator_pin` on the existing fixtures and asserts on drained events (what the slice sees). The PR also replaces main's stale copy of this plan | new `slices/web_session/harness_contract_tests.rs` + `mod` line | Every row: spawn reports id + pid; text prompt → ≥1 text event then exactly one turn end; cancel in flight → `cancelled`; idle cancel → empty outcome (#1272); shutdown and detach idempotent, no error (#1273); unknown resume id → `Restore` error, never fresh; matching resume id → `resumed`, same id; bad pin → `model_apply_error` but spawn OK. Capability rows assert what each fixture emits: images accepted only where advertised; `respond_client_request` Ok only for ACP; usage present for Pi/Claude/Cursor fixtures as emitted. **No fixture edits** (they live in `tests/`): a contract point a fixture cannot drive (e.g. Claude thinking) stays covered by its mapper unit test and is listed in the PR body | none (Cursor row after #1293) |
| **P2** Dedupe (no behaviour change) | `transport()` accessor + 2-arm shared methods; single `transport_for` with the cfg(test) gating once; spawn and catalog `match` on it; `global_npm_root` → `adapters/program.rs`; launch args + init parse helpers → `rpc_handshake.rs`; commands one-shot → `RpcClientCore`; drop blanket `allow(dead_code)` that compiles clean | `session_client.rs`, `catalog.rs`, `program.rs`, `rpc_client.rs`, `rpc_handshake.rs`, 3 clients, 2 spawn modules | P1 + all existing tests green **unmodified**; zero `uses_*`; `cursor_sdk_spawn` imports nothing from `claude_*`; net line count negative; may split in two (clients / `session_client`) to fit the size gate | P1 |
| **P3** Process groups (only if reproduced) | First a regression test: a fixture child that spawns a grandchild, then `shutdown()` and a foreign-boot sweep. **If the grandchild survives:** open the GitHub issue, then `process_group(0)` on both spawners, group kill on drop (`nix` is already a workspace dep; add it to `ajax-web`), sweep signals the group when the leader's token matches. **If it does not survive:** close as not-a-defect and drop the `ponytail:` note's upgrade path | `jsonl_process.rs`, `client.rs`, `orphan_sweep.rs`, `Cargo.toml`, new fixture (needs your OK: `tests/` dir) | Test fails before, passes after; token-mismatch tests unchanged; Codex and Cursor ACP still exit on stdin EOF | #1289 |
| **P4** Cursor flip | `acp_launch_for_agent(Cursor)` → `CursorSdk`; delete Cursor-ACP-only code + the tests that pin only it (already approved), including the ACP label overlay restored in #1293; L1 goes with it | core `agent.rs`/`adapters.rs`, `config_options.rs`, `catalog.rs`, `session_models.rs`, docs | Live spike done with a real `CURSOR_API_KEY`; you confirm how the key reaches the host; full gate + `web:smoke` | P2, spike |

Each PR updates its owning doc (`web-session-behavior.md`; `architecture.md`
for P2's invariants; durability plan stage 3 closed as "satisfied by
`SessionClient` + `SpawnReport`" and the stale stage 6/7 boxes ticked in P2).

### Checklist

- [ ] Approval of rev 2.
- [x] #1289, #1290, #1291 merged.
- [ ] #1292: merge `origin/main` in (conflict repair), then merge.
- [ ] #1293: merge updated #1292 in, restore the Cursor ACP label overlay + its 3 tests, then merge (no flip).
- [ ] #1294: convert to draft; rebuild as P4 after the spike (restore the `Other` assertion, port the effort/fast persistence test).
- [ ] Replace main's stale `native-harness-handoff.md` (from #1290) with this revision — lands with P1.
- [ ] P1 → P2 → (P3 if reproduced) → spike → P4.

Delegation: model-router, Qwen in small packets (one module per packet,
inline signatures, "do not explore", ban `git stash/checkout/reset`). P1 and
P2 are mechanical; P3's repro and P2's `session_client.rs` hunks get parent
review.

Risks: P2 touches the hottest adapter file (mitigated by P1 landing first);
the `transport()` borrow shape may need a `&self` twin; P1 cannot assert
contract points the current fixtures do not emit without a `tests/` edit you
have to approve.

## Read first

1. `AGENTS.md` (delegation through model-router is mandatory; parent does not
   implement), then `architecture.md`.
2. `docs/architecture/web-session-behavior.md` — section "Native Claude SDK
   and Pi RPC events" is the shared-layer contract.
3. `.planning/agent-plans/cursor-native-sdk.md` — this session's plan, with
   findings, deviations, and the per-task checklist. **Authoritative for
   Cursor work.**
4. `.planning/agent-plans/durable-execution-tmux-removal.md` — the durability
   plan (stages 1–12).
5. `.planning/agent-plans/native-claude-pi-events.md`,
   `claude-agent-sdk-pi-rpc.md` — how Claude/Pi went native.

## Where each piece stands

| Piece | State | Evidence |
| --- | --- | --- |
| Claude via Agent SDK sidecar | Shipped on main (#1264) | `acp_launch_for_agent(Claude).transport == ClaudeSdk`; `crates/ajax-web/sidecar/claude_sdk_sidecar.mjs` |
| Pi via native RPC | Shipped on main | `transport == PiRpc`; `pi_rpc_*.rs` |
| Codex via ACP | Stays, by design | `transport == Acp`, `codex-acp` bridge |
| Cursor | **Still ACP on main** (`agent acp`); native in progress, unwired | see "This session's work" |
| Shared native layer | Exists and is guard-tested | `agent_event.rs`, `rpc_client.rs`, `rpc_session.rs`, `rpc_handshake.rs`, `jsonl_process.rs`, `config_model.rs`, `session_client.rs`; guard `native_harness_code_never_names_the_acp_protocol` in `crates/ajax-web/src/architecture_web_session.rs` |
| `/commands` for native harnesses | Done this session, uncommitted | Packet 1 below |
| In-session durability | Partly shipped (#1280 "durable chat execution") | durability plan: stages 1, 2, 4 checked; 3, 5, 6, 7, 8, 12 unchecked; 9–11 PARKED (terminal/tmux) |

Caution: the durability plan's stage 6 (Claude SDK) and 7 (Pi RPC) boxes are
unchecked although both transports are live in the launch table. Verify
against source before trusting either the boxes or this sentence.

## This session's work (uncommitted, NOT in this worktree)

All of it sits uncommitted in the old worktree
(`ajax-cli__worktrees/ajax-durability-v2`). That worktree can be reaped. A
copy is in this worktree as a patch:

```
git apply --check .planning/agent-plans/native-harness-wip.patch   # verified OK 2026-10-13
git apply .planning/agent-plans/native-harness-wip.patch
```

17 files, +1890/−12, generated against `e872f534` (== this branch's base).
It was NOT applied here. **Superseded by the SDK decision below: apply only
`native-commands-packet1.patch`.**

| Packet | What | Status |
| --- | --- | --- |
| 1 | Native `/commands` passthrough: one-shot `take_available_commands()` on `ClaudeSdkClient`/`PiRpcClient`, `AcpEventSource::take_available_commands` (default empty), `acp_drain` seeds `session_available_commands`, ACP update replaces it | ACCEPTED (independent gate + full hunk review) |
| 2 | `cursor_cli_map.rs`: pure Cursor stream-json → `NativeEvent` mapper, 11 tests | ACCEPTED |
| 3a | `cursor_cli_client.rs`: per-turn process client (`begin_prompt`, `poll_event`/`wait_event`, `set_model`, `shutdown`, `turn_argv`) + `tests/fixtures/fake_cursor_cli.js` | ACCEPTED |
| 3b | `cancel()` + process-group kill (`process_group(0)`, `nix::killpg`), `nix = { workspace = true }` added to `ajax-web` | **Gate green, hunk review NOT done.** Delegate timed out before reporting; do the review before accepting |

Gate at handoff (run by the parent, not taken from delegate reports):
`cargo fmt --check` clean; `cargo clippy --all-targets --all-features -- -D
warnings` clean; `cargo nextest run -p ajax-web` 1063/1064.

Everything Cursor-native is `#![allow(dead_code)]` and unwired. Cursor chat
still runs on ACP, so nothing is broken in between.

## DECIDED 2026-10-13: Cursor goes through `@cursor/sdk`

Matt: "Use the sdk." This settles the earlier conflict (the mid-session
"native cursor global" instruction had been read as the global `agent` CLI,
and Packets 2–3 were built for that). Consequences:

- **Packets 2, 3a, 3b are discarded** (`cursor_cli_map.rs`,
  `cursor_cli_client.rs`, their tests, `fake_cursor_cli.js`, the `nix`
  dependency on `ajax-web`, and the two guard-list / `mod.rs` entries). Do not
  apply `native-harness-wip.patch`; it is kept only as reference.
- **Packet 1 is kept.** Apply only:
  `git apply .planning/agent-plans/native-commands-packet1.patch`
  (8 files, +277/−11, `git apply --check` verified 2026-10-13; gated green on
  its own before Packets 2–3 existed: nextest `-p ajax-web -p ajax-core`
  2094/2095).
- Accepted with the choice: Cursor chat authenticates with `CURSOR_API_KEY`,
  an `apiKey` option, or the SDK's own login store — **not** the `agent` CLI
  login ACP uses today. Node ≥ 22.13. `@cursor/sdk` is public beta (v1.0.37
  inspected). Ajax must never read or store the key: the sidecar passes
  nothing and lets the SDK resolve it from the host environment. Confirm with
  Matt how the key reaches the ajax-web host (launchd plist / docker compose /
  zshrc) before the flip in the last task.

What the SDK gives (read from `dist/esm/*.d.ts`, nothing executed):
`Agent.create({ local: { cwd, settingSources }, model: { id, params } })`,
`Agent.resume(agentId)`, `agent.send(string | { text, images? })` → `Run`
with `stream()`, `cancel()`, `steer?()`; events `SDKAssistantMessage`,
`SDKThinkingMessage`, `SDKToolUseMessage`, `SDKUsageMessage`,
`SDKStatusMessage`, `SDKTaskMessage`, `SDKSystemMessage`;
`Cursor.models.list()`; `SettingSource` = project|user|team|mdm|plugins|all.
No local slash-command list API.

## Remaining work (SDK path)

Mirror the Claude Agent SDK transport; prefer generalizing its plumbing over
copying it (`claude_sdk_client.rs`, `claude_sdk_spawn.rs`,
`sidecar/claude_sdk_sidecar.mjs`, fixture `tests/fixtures/fake_claude_sdk.mjs`).

- [ ] 0. Spike with a real `CURSOR_API_KEY`: event shapes, `agent_id` stable
      across turns, `Agent.resume` round trip, `run.cancel()`, model list
      shape incl. `params`, and whether `send("/cmd")` expands a
      `.cursor/commands/*.md` command. Gates tasks 2–4.
- [ ] 1. Apply Packet 1 patch, re-run the gate, commit when Matt asks.
- [ ] 2. `cursor_sdk_sidecar.mjs` + Rust client on the shared JSONL/RPC core
      (`jsonl_process.rs`, `rpc_session.rs`, `rpc_client.rs`): `init`
      (session id, models, commands), `prompt`, `cancel`, `set_model`; map
      events to `NativeEvent`. Tests against a `fake_cursor_sdk.mjs`.
- [ ] 3. Covered by #1293, unmerged (keep the `SessionClient::Cursor` variant per rev 2). Original text: `SessionClient::Cursor` + spawn (module resolution from the global
      npm root only, as Claude does; fail-closed restore via `Agent.resume`;
      Node ≥ 22.13 check without raising Claude's Node 20 floor).
- [ ] 4. Commands: SDK has no list API, so the sidecar scans
      `<worktree>/.cursor/commands/*.md` and `~/.cursor/commands/*.md` and
      returns them in `init`; they flow through Packet 1's
      `take_available_commands()`. Text is sent unchanged. If the spike shows
      the SDK does not expand them, STOP and ask Matt before adding
      Ajax-side expansion.
- [ ] 5. Models/config as `NeutralConfigOption` from `Cursor.models.list()`;
      persisted pins (`cursor-grok-4.6-high`, `model|k=v`) must still decode.
- [ ] 6. Flip `acp_launch_for_agent(Cursor)` to a new transport; delete
      Cursor-ACP-only code + the tests that pin only that mechanism (Matt
      approved those deletions); update `web-session-behavior.md`.
- [ ] 7. SUPERSEDED for stage 3 by the revision: no `HarnessRuntime` trait;
      `SessionClient` + `SpawnReport` already are that layer. Stage 5's sweep
      is #1289 (+ P3 if the group leak reproduces). Stages 8, 12 unchanged; P1 feeds 12. Original: Durability: stages 3, 5, 8, 12 of the durability plan (9–11 parked).
      Stage 3 (`HarnessRuntime` trait + `Capabilities`) is where "one shared
      layer" and durability meet — check whether `SessionClient` should
      become that trait rather than adding a second abstraction.

## Verified facts worth not re-deriving

- ACP session ids are NOT native chat ids: 0 overlap between
  `~/.cursor/acp-sessions` (5500) and `~/.cursor/chats/*/*` (590). Stored
  Cursor sessions fail closed to the existing "Retry / Start fresh".
- `agent acp` exists but is hidden from `agent --help`'s command list.
- The supervisor's `crates/ajax-supervisor/src/agent/cursor.rs` parses Cursor
  stream-json for lifecycle only (no tool ids/args/deltas) — reference, not
  reusable for chat.
- Cursor ACP already passes `/commands` through
  (`available_commands_update`); the swap buys transport parity, not commands.

## Unverified (needs an authenticated Cursor)

This environment could not authenticate (`agent models`, `agent -p`, and
`agent acp session/new` all return "Authentication required"). Built against
the documented stream format and a fake CLI only. Still unknown:

- everything in task 0 of the SDK path (no `CURSOR_API_KEY` was available);
- the CLI-path unknowns are moot now that the CLI packets are discarded.

## Known traps

- **Queue-first delivery breaks Pi tests.** Delivering commands as an event
  ahead of the turn stream broke three existing tests
  (`abort_flow_finishes_cancelled`, `pi_variant_*`). Handshake data goes
  through an accessor, never the turn stream.
- **Persistent accessor spams snapshots.** `pump()` drains on every outbound
  poll and `apply_drain_outcome` sets `pending_commands_snapshot` on every
  `Some`. Anything read at drain start must be one-shot.
- **Known edge, unfixed:** `AcpSlot.session_available_commands` is not reset
  when a client is replaced; swapping into a harness with no commands keeps
  the old list.
- **Known test failures, not caused by this work:**
  `spawn_tests::live_cursor_initialize_advertises_load_session` (needs a
  logged-in `agent`; pins the ACP path Task 6 deletes);
  `task_session_reliability_tests::terminal_persist_failure_retries_before_fifo_advances`
  failed once in a combined `-p ajax-web -p ajax-core` run, then passed 5/5
  alone, 2 full runs, and 40/40 under 2× CPU stress — not reproduced, cause
  unproven (it sleeps a fixed 150 ms before asserting a phase).
- **Router state (2026-10-13):** Qwen timed out at 900 s on 3 of 3 packets but
  left substantive work each time — judge by delta + your own gate, not the
  report. Haiku route is dead (`claude-haiku-5-5` rejected by the Claude ACP
  adapter; fix belongs in `ajax-model-router`). GLM completed 3 rounds then
  returned "Authentication required". Cursor route down (keychain locked).
  Codex route untested this session.
- A timed-out round is reported `FAILED` with `CHANGED_FILES: []` even when
  files changed. Always check `git status`.
- RTK rewrites `git diff` output; use `rtk proxy git diff` for raw hunks.
- `ajax-cli` here: binary `~/.cargo/bin/ajax-cli`; live tasks are in
  `--profile stable` (the ambient `AJAX_PROFILE=dev` shows a different,
  stale task list).

## Rules that bit or nearly bit

- Never edit, delete, or weaken an existing test to get green; fix the
  implementation. Test deletions are approved ONLY for tests that pin the
  removed Cursor-ACP mechanism.
- No `git stash/checkout/restore/reset/clean` — in delegate prompts too.
- Rust files: ~600 lines target, 1000 hard cap. `client.rs` (913) and
  `config_options.rs` (816) are close.
- Nothing was committed or pushed this session. Matt has not asked for a
  commit or PR.

## Verification commands

```
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run -p ajax-web -p ajax-core --no-fail-fast
npm run verify        # PR gate
```
