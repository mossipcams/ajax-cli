# Ajax Chat shows "Response ready" while background agents still work

Approval: **approved 2026-10-24, option A**. Revision 4 (2026-10-23): retargeted to Ajax Chat, which
is the only surface Matt uses. Evidence is kept
locally in `~/.ajax-router/spikes/delegate-run-identity/` and not committed,
because it holds raw process data from Matt's machine (`code-map-acp.md`, `census.md`,
`results2.md`, `code-map-launch.md`, `code-map-events.md`).

Constraint: harness- and router-agnostic. It must work whichever harness is
the parent and whatever starts the background work (model-router/acpx,
`claude -p`, a Monitor watcher, anything later).

## Problem

In Ajax Chat, status comes only from ACP turn events. A background delegate
works like this: the parent's Bash call returns at once, the parent ends its
turn, and `TurnEnd` reaches `AcpExecutionState`. `RootTurnState::Settled` then
maps to `Done` (`observe.rs:13`), and the task shows **"Response ready"**
(`session_activity.rs:34,134`, `ui_state.rs:211,277`) while the delegate is
still working. Nothing in the ACP path can see it:
`ChildStarted/ChildUpdated/ChildEnded` exist and fold correctly, but no
production code emits them (`code-map-acp.md` Q2).

## Evidence (live, 5 Ajax Chat sessions, `census.md`)

- Tree: `ajax-cli web` → `nono` → `node` sidecar → **harness** → children. The
  host `ajax-cli` is not sandboxed and can read every session's tree
  (`code-map-acp.md` Q4c).
- A background delegate is rooted at a **shell that is a direct child of the
  harness** (`zsh -c …` → `run_transaction.py` → `run_delegate.py` → `acpx` →
  `claude-agent-acp` → `claude`), and it outlives the turn.
- The other direct children of the harness are MCP servers (iwdp-mcp, codeburn,
  opensafari). None of them is a shell, and each is as old as the harness.
- Every shell child seen across 5 sessions was background work (delegates, a
  Monitor `tail -F`, a `sayso-watch.sh` loop). No idle shell was seen.
- Name matching is unsafe: Claude's built-in grep runs as the `claude` binary
  with argv `ugrep`.

## Design

**Rule (no harness logic, no per-turn state):** while a session's root turn is
*not* running, every live **topmost shell** (`sh`/`bash`/`zsh`/`dash`/`fish`,
basename with any leading `-` stripped) in the subtree of the session's
recorded child pid (nono, or the adapter when the sandbox is off) is one
background job. Topmost means it has no shell ancestor inside the subtree.
The rule never identifies the harness, and a delegate's own inner shells are
counted once, under its top shell.

**Debounce:** a job is reported ended only after it has been gone for ≥5 s
with no new turn. When a background job exits, the harness usually starts a
new turn to handle the result, and the debounce stops a "Response ready"
flash (and a possible push) in that gap.

1. **Detect** in the ACP host (unsandboxed). Take one `ps -A -o
   pid=,ppid=,lstart=,comm=` snapshot (macOS + Linux), shared across sessions. The host
   already uses `ps` in `process_identity.rs`. Do it on `TurnEnd`, then every
   ~2 s from the session actor tick (`task_session.rs:404`) while any
   background job is tracked. Sessions with no background job cost nothing.
2. **Emit** `AcpExecutionEvent::ChildStarted { run_id: "bg-<pid>-<lstart>",
   completion_holding: true }` when a job appears and `ChildEnded` when it
   disappears. The id embeds the start time, so pid reuse can't alias. Reduce
   and report from the tick, because a child can end without any ACP event
   (`code-map-acp.md` Q2c.2).
3. **Show it truthfully.** The reducer already returns
   `ParentPhase::CompletedLocallyChildrenActive` (`agent_status.rs:317-330`),
   but `session_activity.rs:133` collapses it to "Waiting for input". Map
   that phase to a new `SessionActivity::BackgroundRunning`, labelled
   "Background work running" in chat and on the dashboard (`ui_state.rs`).
   It must not demand attention like "Response ready".
4. **When the next turn starts**, the root `Running` state outranks children,
   so status is normal. The children remain tracked until their processes
   exit.
5. **Recovery:** the rule is stateless, so after a host restart the first
   tick rediscovers live jobs from the process table.

## Decision for Matt: what counts as "background work"

| Option | Holds status for | Risk |
|---|---|---|
| **A (recommended): any background shell** | router delegates, Monitor watchers, background tests/builds | A forgotten background dev server keeps the task in "Background work running" until it exits. That is visible and true, but it never settles on its own. |
| B: only shells whose subtree contains an agent | router delegates only | Needs agent recognition, which is harness-specific and broken by `ugrep`-style multicall binaries. This violates the constraint. |

## Rejected

- Hook/env run assignment (revisions 1-3): ACP agents have no `AJAX_*` env,
  and it only sees agents that have Ajax hooks installed.
- Claude SDK `background_tasks_changed` / claude-agent-acp `async_task_*`
  (`code-map-acp.md` Q6): exact, but per harness. Pi exposes nothing. Could
  be added later as an enrichment.
- "Any descendant born during the turn": needs turn-boundary state, and it
  would flag lazily started servers.
- Matching harness binary names: the `ugrep` multicall false positive.

## Ceilings (`ponytail:` comments in code)

- Codex/Pi/Cursor are unobserved in the census (only Claude sessions were
  live). If a harness keeps a persistent idle shell (e.g. Codex unified exec),
  it would read as background work. Verify each harness in the live check
  before trusting it.
- In-process sub-agents (Claude Task tool) don't spawn processes and stay
  invisible. That is unchanged and fine, because they hold the parent's turn.
- `ps` polling has a ~2 s detection lag and one exec per tick across all
  sessions. It is only active while a job is tracked.

## Tasks

- [x] Open a `mossipcams/ajax-cli` issue for the defect: #1412.
- [ ] (deferred: Codex route unavailable, Pi has no working model) Spike: a live Codex and a live Pi Ajax Chat session. Census their idle
      shell children (reuse `census.py`).
- [x] Host: process snapshot + `background_shells(table, root)` (`process_tree.rs`) (pure function
      over the snapshot, unit tested with census-shaped fixtures, including
      MCP servers, ugrep-claude, and nested delegate trees).
- [x] Host: emit Child* from the TurnEnd hook and the gated actor tick (`background_jobs.rs`), and
      reduce/report on the tick.
- [x] `SessionActivity::BackgroundRunning` (reuses `AgentRunning`, summary
      "Background work running"; no new `LiveStatusKind`, no `ui_state` change).
- [x] Regression test (`background_jobs_tests.rs`): TurnEnd while a background job is live → "Background
      work running", never "Response ready". The job exits → "Response ready".
- [x] Docs: `web-cockpit.md` / `web-session-behavior.md` status section,
      `task-authority.md:139-146` (the ACP paragraph).
- [ ] Live check in Ajax Chat: dispatch a router delegate in the background
      and watch the dashboard and chat status through to completion.
- [ ] Route implementation through model-router.

## Verification

- `cargo nextest run -p ajax-web` (session_activity, task_session),
  `-p ajax-core` (acp_execution_state)
- `cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt --check`
- The live check above. Web UI change, so run `CI=1 npm run web:smoke` after
  `pkill -f vite`.

## Results

Matt chose option A on 2026-10-24. Slices 1, 2 and docs are implemented and
accepted (uncommitted).

- `cargo nextest run -p ajax-web`: 1186 passed, 148 failed. Every failure needs
  `ps` (the session-attach start token, or the `ps`-based tests), which nono
  denies. That matches the known sandbox baseline (~147 + 2 log tests) plus the
  new `process_tree` snapshot smoke test. All 18 new logic tests pass.
  CI (unsandboxed) is the real gate for the `task_session` suites.
- `cargo clippy -p ajax-web --all-targets --all-features -D warnings`: pass.
  `cargo fmt --check`: pass.
- Remaining: live Ajax Chat check (needs a rebuilt dev web, outside nono),
  Codex/Pi census, PR.
