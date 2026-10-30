# Tech debt: 15% code reduction

Status: Stage 1 and Stage 2 done 2026-10-30, removing about 2,400 lines net (0.9%).
Stages 3 to 5 are not approved. D4 is rejected: its code is live.

## Goal

Remove at least 15% of the repository's code through architectural
simplification, without changing supported behavior except where a removal is
explicitly approved below.

## Baseline (2026-10-30, main 3b82c51a)

Tracked `.rs .ts .tsx .css .mjs .js .sh .mts .py`, measured with `wc -l`.

| Area | Prod | Tests |
|---|---:|---:|
| ajax-web (Rust) | 39,279 | 42,668 |
| ajax-web/web (TS/CSS) | 34,235 | 44,246 |
| ajax-core | 30,694 | 21,879 |
| ajax-cli | 9,446 | 14,912 |
| scripts | 5,085 | 2,945 |
| ajax-supervisor | 4,059 | 0 |
| ajax-tui | 2,410 | 4,247 |
| fixtures/generated | | 1,554 |

- Total: 257,659 lines. Prod and test files: 125,208 / 130,897.
- 21,563 of the prod-file lines are inline `#[cfg(test)]`, so true production
  code is about 103,600 and test code about 152,500.
- **15% target: 38,650 lines removed.** Tests must go with the code they cover,
  so at the repo's 1.5:1 ratio this is about 15,500 production lines.

## Non-goals and constraints

- No weakening of tests. Test lines leave only with the behavior they cover, or
  as proven duplicates, and only under explicit approval.
- No lifecycle, registry-truth, or ownership-boundary changes without approval.
- Removing a public command or documented behavior is an `AGENTS.md` stop
  condition: each such item is called out for sign-off.
- `lifecycle.rs` guard requires ajax-core tests to stay inline; CSS section
  comments are load-bearing; guard tests read `Cargo.toml`/`ci.yml`.

## Result

Discovery complete. **15% is not reachable through debt removal alone.** Debt
plus the frozen surfaces gives about 8%. Every remaining option removes a live
feature, and the separable ones stop at about 13.5%. Tier 0 and 1 credits below
are the audit's estimates; implementation found several were too high. See
"Corrections from implementation".

**Direction (user, 2026-10-30): prefer deleting, and prefer reusing code.** When
two things do the same job, delete the copy and call the one that already exists.
Consolidation that needs a new shared abstraction is dropped, and a rewrite must
not simply become more code. This is a preference, not approval of any tier: each
removal still needs approval under `AGENTS.md`.

| Tier | Contents | Lines | Cumulative | Approval |
|---|---|---:|---:|---|
| 0 | Dead code, literal duplicates, copies of existing code | 1,648 net, actual | 0.6% | Done |
| 1 | Compatibility cutoffs (D7 done; B-02 deferred) | 764 net, actual | 0.9% | Done |
| 2 | Remove TUI and supervisor (A1, A2) | ~18,140 | 8.2% | **Reverses a documented decision** |
| 3 | Stop creating terminal-agent tasks (A3/G-02) | unmeasured, est. 3-5k | ~9.7% | Lifecycle and registry change |
| 4 | Remove separable live features (STT, exploratory, Cloudflare Access) | ~9,760 | ~13.5% | Product decision |

Restored by the reuse rule, because the replacement already exists: E-1's local
helper copies (~220), deleted in favor of `test_support.rs`, and B-01's client
forwarders (104), with callers using the existing `core()`.

Still dropped, because each needs new shared code: B-01 sidecar framing (55 net);
E1b shared fakes and parameterization (~265). B3's queue and registry
consolidation stay out: they need replacement designs, not deletion.

### Rejected: delete the native harness stacks

Decided 2026-10-30: the native harness transports (Claude SDK, Cursor SDK, Pi
RPC; `Transport::Native` in `session_client.rs`) stay. This was the "keeping
native" instruction, read as harnesses rather than the native TUI. Falling back
to ACP would have removed up to ~12,000 lines and lost capabilities.

### Tier 0: no intended behavior change

| ID | What | Lines | Confidence |
|---|---|---:|---|
| D1 | `agent_prompt.rs`, unreachable | 486 | proven, parent verified |
| D3 | `RuntimeEvidence<T>` layer, net of two kept tests | ~220 | proven; tests retargeted |
| D2 | `validity.rs`; also update `architecture.rs:29` | 153 | proven, parent verified |
| D6, D5 | Small dead items; most of D6 was false, see below | ~75 | proven |
| D4 | Ghost disposition API | 0 | **rejected: live** |
| B-04 | Dead queue helper and PID termination | 166 | proven; pub Rust API caveat |
| C1, C2 | Unused barrel exports, alias file | ~150 | likely |
| closure | `refresh_github_check_evidence`, `sandbox_command_from_env`: no reference anywhere | 63 | proven by sweep |
| E | Literal test duplicates ~60, dead mock fields ~20, dead CSS 19 | ~100 | likely |
| E-1 | Local test helpers that copy `test_support.rs` | ~220 | proven definitions, sizes +/-30 |
| B-01 | Client forwarders; callers use existing `core()` | 104 | exact spans, not compiled |

Corrections from the recount (`.planning/router-runs/tech-debt/lit.py`): E4's
~230 "exact duplicates" compared bodies after blanking literals, so most test
different inputs and must stay; only 8 groups (~60 lines) are literally
identical. The sweep's other ~580 "dead" lines are reachable from tests. About
170 of those are the `run_with_*` CLI test seams, which stay. The remaining ~470
are production code only tests call; they are reviewed in PR 2, uncredited.

### Corrections from implementation

"No outside callers" did not mean dead, and that was the audit's main error.

- D4 was false. `registry_persistence_disposition` is called by live predicates
  in `ghost_task.rs`. The delegate's rewrite of `is_registry_ghost_task` and its
  test deletion were rejected, and `ghost_task.rs` is unchanged.
- D6 was mostly false. `ConfigParseError`, `ConfigPaths`, `for_home`,
  `CapabilitySupport`, `support_for`, `profile_for_hook_client`, `PaneWaitHint`,
  and `recognize_wait_hint` are live and stay.
- `reconcile_runtime` and `supports_native` were thin wrappers over live code.
  Their tests were the live coverage, so they stay, pointed at
  `reconcile_runtime_for_task` and `support_for` with assertions unchanged.

### Tier 1: compatibility cutoffs

- B-02 legacy JSONL and ledger import: 424. Old-only sessions would look empty.
- D7 v2-v7 migration ladder: ~390. Data loss risk on ancient databases.

### Tier 2: frozen surfaces

- A1 native TUI and CLI tmux verbs: ~13,380 (removes `ajax cockpit|stable|dev`).
- A2 supervisor: ~4,710 (removes `ajax supervise`).
- Second-order fallout: 52 lines.

`architecture.md:155-156` says of the TUI, tmux verbs, and terminal-agent tasks:
"No removal is planned." Tier 2 needs that decision reversed. The supervisor is
also an `AGENTS.md` ownership boundary. Test attribution inside shared
`lib/tests/suite_*.rs` files is a keyword heuristic, so A's totals are +/-10%.

The closure sweep (`.planning/router-runs/tech-debt/closure.py`) treats A1 and
A2 as deleted and iterates to a fixed point. It matches by name, so a same-named
symbol keeps an item alive: read 52 as a lower bound. The cause is that ajax-web
genuinely consumes ajax-core; removing the TUI does not cascade.

### Tier 3: terminal-agent tasks

About 30 `skip_interactive_agent` sites across ajax-core and ajax-web. Existing
tasks have no flag and resolve to TerminalAgent, so this needs a registry
migration. The estimate is the parent's, not a measurement.

### Tier 4: live features, priced

Whole-feature prod plus test lines, from slice F. Mostly "likely"; shared-file
wiring is estimated.

| Feature | Lines | Assessment |
|---|---:|---|
| Web terminal tab | ~8,050 | Keep. Chat-first keeps it, and it is the only terminal left if A1 lands |
| Training page | ~6,260 | Keep. The last five commits are all training work |
| Claude SDK harness | ~4,980 | Keep. Primary harness |
| STT and moonshine | ~4,770 | Separable. Browser dictation is a partial substitute |
| Exploratory harness | ~4,750 | Separable, but it files real defects (#925, #1096, #1135, #1136) |
| Cursor SDK harness | ~4,260 | Keep. Multi-harness direction |
| Pi RPC harness | ~2,770 | Keep. It is the local Qwen path |
| Push notifications | ~1,640 | Keep. Uses `window.pushManager` (Declarative Web Push), no worker needed |
| Cloudflare Access | ~240+ | In use: `~/.config/ajax/cloudflare-access.env` exists. Removing it changes authentication |

Rejected from slice F: `dev_deploy` and `ajax service` are live infrastructure
(this session runs under `AJAX_WEB_SUPERVISOR=launchd`), not leftovers.

## Implementation plan

Rules for every PR:

- Deletion and reuse only. Before keeping or writing anything, look for an
  existing implementation and call it. New code beyond a fail-closed error is out
  of scope, including new shared helpers.
- Keep list, untouched except B-01's forwarders: native harness stacks,
  `web_session_acp`, `session_shim`, core tmux adapter, web terminal tab, training, push, sandbox, `ajax service`,
  `dev_deploy`, and exploratory unless Tier 4 approves it.
- A test leaves only with the code it covers, or when literally identical to a
  kept test. Each PR lists its deleted tests and why.
- PRs land one at a time, each branched from current `main`. No stacked PRs: they
  have stranded work before. Re-check against `main` before each merge, since
  there is no CI on push to `main`.
- Size gate: `scripts/check-file-loc.mjs` uses `--diff-filter=ACMR`, so whole-file
  deletions do not count. Edits to surviving files do: 600 lines per file and
  1,500 per commit. Split shared-suite pruning across commits.
- Each PR records the tracked-line count before and after, and updates this plan.
- Branches are `type/scope/description`, opened with `scripts/gh-pr-create`.
  Creating them needs explicit authorization, because this worktree is on
  `ajax/tech-debt`. PRs that remove user-facing commands use `feat`, so they cut a
  release; the rest use `chore`.
- Routing: run `scripts/analyze-task --execution` per packet. Qwen packets stay
  at 4 files or fewer. Deletions that only compile across many files go to Haiku
  per the router size gate. Supervisor and PTY work is high risk: parent reviews
  and reruns the critical checks.

Gate for every Rust PR, run from a clean tree with `CARGO_INCREMENTAL=0`:

```bash
cargo fmt --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
env -u RUST_LOG cargo nextest run --workspace
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
npm run verify:arch
```

Disk was 97% full on 2026-10-30, so gates also set `CARGO_PROFILE_DEV_DEBUG=0` and
`CARGO_PROFILE_TEST_DEBUG=0`, and workspace nextest is left to CI where the local
gate cannot fit; focused crate tests always run.

Web PRs add `npm run web:test`, `npm run web:build`, and the Web E2E CI result;
WebKit cannot run inside the sandbox.

### Stage 1: Tier 0

Starts on approval of Tier 0. No public behavior changes.

- [x] PR 1 `chore/core/remove-dead-code`: +23, -1,027, net -1,004 lines. Gate passed
  2026-10-30: fmt, workspace clippy, workspace nextest (3,448 passed, 1 skipped),
  rustdoc, `verify:arch`.
  - Packet 1: D1 and D2; remove both `pub mod` lines from `lib.rs` and `validity`
    from `crates/ajax-core/src/architecture.rs:29`.
  - Packet 2: D3 in `runtime.rs`. Keep `RUNTIME_PROJECTION_FRESH_FOR`,
    `ObservedTaskRuntime`, and `reconcile_runtime_for_task`.
  - Packet 3: D5 after confirming no CLI verb reaches
    `registry_export_json_snapshot`; D6; the two closure items.
- [x] PR 2 `chore/core/remove-test-only-code` (#1530): +8, -206. Review cut the ~470
  estimate: most items are wrappers over live code, the shipped durable-execution
  family (`docs/architecture/durable-execution.md`), or supervisor-owned, and they
  stay. Nothing in production sets `SideFlag::Stale` now; its readers stay for
  existing tasks.
  - Review each test-only item in `ajax-core`: `plan_start_task_operation`,
    `mark_stale_tasks`, `unfinished_executions`, `resolve_recovery`, and the rest
    of the sweep list. Delete it with its tests only when no production path
    needs the behavior. Keep the `run_with_*` seams.
- [x] PR 3 `chore/web/remove-dead-code` (#1531): +11, -382, excluding `dist`. The
  four `TestBridge` fields are set and read, so they stay. Two of the eight
  duplicate groups cover different modules, and the speech pair documents two
  contracts, so those stay too.
  - B-04 Rust helpers; C1 and C2 barrel exports, re-grepping each export first;
    `ActivityDisclosure.tsx` only, not its test; dead `TestBridge` fields; two CSS
    rules, updating the CSS BASELINE ledger; the literal test duplicates.
- [x] PR 3b `chore/web/reuse-existing-helpers` (#1532): +169, -244. The E-1 copies
  under `adapters/web_session_acp` stay: `architecture.rs:157,209` forbid adapters
  from importing `slices`.
  - E-1: delete local `scratch_dir`, `fake_acp_fixture`, and `pump_until` copies
    where `test_support.rs` already covers them. Assertions do not change. The two
    `adapters` variants stay unless an existing helper fits; none gets created.
  - B-01: delete the ten direct forwarders from each client and
    `begin_prompt_with_images` from Claude and Cursor; callers use `core()`. This
    is the live chat path, so review it as medium risk.
- [x] D4 ghost disposition API: rejected, the code is live.

### Stage 2: Tier 1

Approved 2026-10-30 with the cutoff left to best practice. Decisions:

- **D7: minimum supported registry schema is v9.** v9 was current by 2026-07-03
  and is the schema immediately before today's v10, so the v9-to-v10 step stays.
  v2-v7 databases upgrade through ajax-cli v0.59.0, the latest tag, which still
  has the ladder.
- **B-02 is deferred, not done.** The legacy import landed in #1280 on 2026-10-13,
  after v0.59.0 (2026-10-07), so no tagged release contains it. It also runs
  lazily per session, so a session never reopened stays unimported. Removing it
  now would make v0.59.0 chat history look empty after an upgrade. Revisit once a
  tagged release has carried the import through at least one further release.

- [x] PR 5 `feat/core/drop-old-schema-migrations`: +36, -800. Disk was too low for the
  workspace gate, so local checks ran on `ajax-core`, and CI ran the workspace.
  - Delete `migrate_v2_to_v3` through `migrate_v7_to_current_schema` and the calls
    in `migrate()`. Keep `migrate_v9_to_v10`, `ensure_agent_runs_columns`, and
    `create_current_schema`.
  - No new code path: below v9, the existing `IncompatibleSchema` branch already
    refuses and leaves the database untouched. A new `found < supported` message
    arm names ajax-cli v0.59.0 as the upgrade path. The pinned message for a
    newer-than-supported database is unchanged.
  - Deleted the six tests that drove the removed steps and their v5, v6, and v7
    fixtures. Every
    `IncompatibleSchema` and `LegacySqlitePayloadSchema` test stays, and a new v7
    test asserts the refusal, the message, and an unchanged `user_version`.

### Stage 3: Tier 2

Blocked on approval to reverse `architecture.md:155-156`. Supervisor goes first:
it is smaller and independent, and both PRs edit `execution_dispatch.rs`.

- [ ] PR 6 `feat/cli/remove-supervise`, ~4,710 lines
  - Delete `crates/ajax-supervisor` and `ajax-cli/src/supervise.rs`.
  - Remove the `execution_dispatch.rs` arm and helpers, `snapshot_dispatch.rs:141`,
    `supervise_command`, and the manifest entries in both `Cargo.toml` files.
  - Update `ajax-core/src/logging.rs:11`, `scripts/verify-slice.mjs:48,79-80`, and
    the `CRATES` list in `ajax-core/tests/wave2_live_apply_contract.rs`.
  - Check whether `AJAX_SUPERVISE_EVENT_LOG` and `notify` still have users.
  - Rewrite the AGENTS.md "Supervisor observes" boundary and the crate rows in
    `architecture.md:32,69`; trim `docs/architecture/cli-supervisor.md`.
- [ ] PR 7 `feat/cli/remove-native-cockpit`, ~13,380 lines
  - Delete `crates/ajax-tui`, `cockpit_backend*`, `cockpit_actions.rs`,
    `cockpit_snapshot.rs`, and `tmux_task_session/`.
  - Remove dispatch arms, `cockpit_command`, `cockpit_alias_command`,
    `cockpit_args`, the `context.rs:66` profile inference, and the
    `lib.rs:472` watch handling. Profile selection stays on `--profile`.
  - Prune `lib/tests/suite_*` by test function, in commits under the size gate.
    Delete `ExecuteNewTaskWithSession`, `CockpitActionItem`, and other second-order
    items.
  - Update the `suite_5.rs:222` guard, which reads the TUI manifest;
    `verify-slice.mjs:73-74`; and `wave2_live_apply_contract.rs`.
  - Remove `ratatui` and `crossterm` from the workspace if unused, then run
    `cargo machete` and check `deny.toml`.
  - Docs: delete `docs/architecture/cockpit.md`; update `architecture.md`
    (`:30,:55,:155-156,:184`, navigation map), `web-cockpit.md:1458,1723`,
    `DESIGN.md`, `.github/exploratory/charter.md`, and `README.md`.

Operator impact: `ajax cockpit`, `ajax stable`, and `ajax dev` stop existing, so
SSH sessions from Terminus lose the terminal console. The web Cockpit becomes the
only console. Do this only once that is acceptable day to day.

### Stage 4: Tier 3

Blocked on a measurement and a separate approved plan, because it changes
lifecycle and registry truth. No implementation is planned here.

- [ ] Count lines per `skip_interactive_agent` site.
- [ ] Decide how existing tasks without the flag migrate.

### Stage 5: Tier 4

One decision per feature. Each removes user-visible behavior and gets its own PR.

- [ ] STT and moonshine, ~4,770
- [ ] Exploratory harness, ~4,750: also remove the workflow and
  `verify-ci-workflows.mjs` entries.
- [ ] Cloudflare Access, ~240+: changes authentication, so it needs its own review.

## Decisions required

1. Approve Stage 3 and the `architecture.md` reversal.
2. Which Tier 4 features go, if any.
3. When B-02 can go, once a tagged release has carried the import.

## Coverage and known gaps

- Reports: `.planning/agent-plans/tech-debt/`. Slices B3, E1, E3, and E4 ended
  in failure or timeout; E3b and E4's script recovered most of it.
- E1b, E3b, and G ran on Haiku after Codex failed at startup three times.
- Not audited: pass-through layers in `web_session` and `runtime` (retry budget
  spent); terminal-agent line counts; hand confirmation of frontend test
  near-duplicates (26 pairs; only 15 `it.each` uses, so table-driving may yield
  more than credited); about 12k lines of ajax-web tests checked by name only.
- B-02 and B-04 assume no external consumer of `ajax_web` public Rust APIs.

## Validation

Audit counts are `wc -l` and scripted sweeps. Parent verified D1-D3 reachability,
frozen verbs have no shell-outs, and `architecture.md:155-156` by direct search.
Implementation results are recorded per PR in the stage checklists.

## Checklist

- [x] Discovery across frozen surfaces, web backend, frontend, core, tests,
  scripts, CSS, feature costs, and transitive closure
- [x] Implementation plan written
- [x] Stage 1 and Stage 2 approved 2026-10-30
- [x] Stage 1 and Stage 2 implementation (see stage checklists)
- [ ] Stages 3 to 5: not approved
