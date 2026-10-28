# Chat-first core: Phase 4 (chat verbs) discovery

Status: read-only discovery, 2026-10-28, branch `ajax/architectural-flaws`
(HEAD `e8f75612`). Scope: facts with file:line references. Speculation is
marked **(speculation)**. Plan section reviewed: `.planning/agent-plans/chat-first-core.md`
"Phase 4: chat verbs" (line 202), with Context item 4 and Phase 0 items.

Target (from the plan): chat tasks show Continue (compose), Stop (cancel turn),
Restart agent, Fix CI, Resolve conflicts, Create PR, Merge PR, Drop. Resume /
Repair / Ship only for `TaskKind::TerminalAgent`. Create PR, Fix CI and
Resolve conflicts are prompts through the chat queue. Merge PR runs
`gh pr merge`. `PullRequestRef` gains additions/deletions.

Reading gaps: `architecture.md` and `docs/architecture/web-cockpit.md` were not
read in full (only AGENTS.md lines 1-150 and the plan). `docs/agent/rust.md`
and `docs/defect-process.md` were not needed for a read-only packet.

---

## Summary of findings

- Today a chat task's action list is produced by core (`available_operator_actions`)
  and is almost always `Resume` + `Drop`, with `Ship` added when reviewable. `Resume`
  is primary for every chat task because `Open` is always eligible.
- Of the 8 target verbs, **Continue**, **Stop**, **Fix CI**, **Resolve conflicts**
  and **Drop** exist. **Restart agent**, **Create PR** and **Merge PR** do not.
  Continue and Stop are WebSocket messages, not operator actions.
- Fix CI and Resolve conflicts already go through the chat FIFO for chat tasks
  (Phase 0.2). They are only offered when a CI-failure or conflict flag is set.
- No `gh pr merge` or `gh pr create` exists anywhere in Rust. Ship is a local
  `git merge --ff-only` in the repo root.
- `PullRequestRef` has no diff stats. The `gh pr list --json` field list
  (`additions`, `deletions` are valid, verified with `gh pr list --json` help)
  is pinned by tests, so adding fields there changes pinned assertions.
- **One direct contradiction:** `recommended.rs:703`
  `chat_task_without_tmux_session_or_window_is_not_invalid` asserts that a chat
  task offers Ship. Target says Ship is TerminalAgent-only. See section 6.
- **Second contradiction class:** `OperatorAction::all()` is pinned to exactly
  six labels (`models/tests.rs:580`). Adding new `OperatorAction` variants breaks
  that test and two others. New verbs must not be new `OperatorAction` variants
  unless that test is updated with Matt's approval.

---

## 1. How operator actions are computed today

### 1.1 Core computation

- `OperatorAction` enum: `crates/ajax-core/src/models/projection.rs:181-190`
  (`Start, Resume, Review, Ship, Drop, Repair`); `all()` at `:190-199`;
  `from_label` at `:213-218` (iterates `all()`).
- `TaskKind { Chat, TerminalAgent }`: `crates/ajax-core/src/models/task.rs:51-54`.
  `Task::kind()` at `:154-160` returns `Chat` iff `skip_interactive_agent()`
  (`:148-151`, metadata key `"1"`).
- `operator_action(task)`: `crates/ajax-core/src/recommended.rs:19-55`.
  - Known-invalid → Drop (`:20-26`).
  - Candidate = primary annotation's `suggests` if it is in the available list,
    else `fallback_operator_action` (`:137-160`), else first available (`:45-50`).
  - **If `Resume` is in the available list it always wins** (`:47-50`:
    `action = Resume`, reason rewritten to `resume` for ship/review). Because
    `Open` is always eligible for a non-removed, non-substrate-missing task
    (`operation.rs:49-99`, no Open rule), **every chat task that is not invalid or
    missing substrate has `primary_action = Resume`.** This matches the memory
    note "never order web actions by `primary_action` (it's always Resume)".
- `available_operator_actions(task)`: `recommended.rs:188-250`. Output values
  are only `Repair`, `Resume` (from `TaskOperation::Open`), `Ship` (from
  `Merge`), `Drop` (from `Clean` or `Remove`). **`Review` is never returned** by
  this function; `Review` only appears in hand-built test cards
  (`cockpit/tests.rs:294-299`, `:373-377`).
  - Checkout mismatch → `[Repair, Resume, Drop]` (`:195-202`).
  - Missing substrate (non-shell) → `[Repair, Drop]` (`:216-217`).
  - Ship suppressed when `ci_failed || has_open_pull_request` (`:231-232`, filter
    `:242`).
  - Chat tasks: `task_is_known_invalid` ignores tmux for chat (`:261`), so a
    chat task with no tmux is not invalid (test at `:703`).
- Eligibility rules: `crates/ajax-core/src/operation.rs:49-99`
  (`Merge` needs Reviewable/Mergeable, `:55-62`; `Clean` needs Merged/Cleanable/
  Removing/TeardownIncomplete, `:63-70`; `Check`/`Diff` need a worktree, `:72-80`).
- `has_open_pull_request`: `operation.rs:101-104` (reads stored PRs).
- `attention::annotate` supplies the annotation severity used at `recommended.rs:24-31`.

### 1.2 Web filtering and rendering

- `web_action` / `supported_web_action`: `crates/ajax-web/src/slices/actions.rs:16-24`,
  `:82-89`. Supported web actions are **Review, Ship, Drop, Resume** only.
  Labels at `:26-37`. `Drop` is destructive + confirmation (`:21-22`).
- `browser_actions(context, card)`: `actions.rs:48-67`. Remediations (`fix-ci`,
  `resolve-merge-conflicts`) are prepended from `card.remediations`
  (`:49-54`), then `card.available_actions` filtered by `supported_web_action`
  (`:55-61`), de-duplicated by name (`:63-65`). Ship's `confirmation_required`
  comes from `merge_task_plan` (`:69-80`).
- Cards are built in `crates/ajax-web/src/slices/cockpit/mod.rs:79-90`
  (`browser_task_card`, `actions: browser_actions(...)` at `:90`); detail view at
  `:137-147`.
- `session_capable` (used to decide chat-session attach) is
  `skip_interactive_agent && acp_launch_for_agent` at `cockpit/mod.rs:69-77`.
- Web client:
  - `WebAction` / `BrowserTaskCard` types: `crates/ajax-web/web/src/shared/lib/types.ts:76-81`, `:110-123` (`actions` at `:119`; no PR or diff fields on the card).
  - `visibleTaskActions` hides `resume` and `open`: `crates/ajax-web/web/src/features/task/taskActions.ts:3-5`.
    So a chat task's visible server actions are `drop` (plus `ship`/`review`
    when present) and remediations. Resume is computed server-side but never shown.
  - `TaskRow` renders `visibleActions`; the swipe-reveal action is Drop
    (`features/task/TaskList.tsx:89`). Row meta is status + time (`TaskList.tsx:192`, `:197-199`).
  - Head actions for chat mode: `features/task-workspace/TaskWorkspace.tsx:113-124`
    (ActionBar with `safeActions`). `ActionBar.tsx:37` styles `fix-ci` and
    `resolve-merge-conflicts` as remediation buttons.
  - Dispatch: `features/task/taskMutations.ts` (Drop-specific handling at `:58`,
    `:133`); action sent to `/api/actions` (see section 2).

### 1.3 Which actions a chat task gets today (per state)

"Server" = `available_operator_actions` + web filter. "Client" = after
`visibleTaskActions`. Remediations are added on top when their flag is set.

| Chat-task state (skip=1, ACP agent) | Core available (`recommended.rs:188`) | Web server actions (`actions.rs:82`) | Client-visible |
|---|---|---|---|
| Active / running, no PR, no CI flag | Resume, Drop | resume, drop | drop |
| Reviewable, clean, no open PR | Resume, Ship, Drop | resume, ship, drop | ship, drop |
| Reviewable, open PR (ship blocked) | Resume, Drop | resume, drop | drop |
| CI failed (`TestsFailed` flag or `CiFailed` live) | Resume, Drop (Ship blocked `:231`) | fix-ci (remediation), resume, drop | fix-ci, drop |
| Conflicted (flag / `git_status.conflicted` / live `MergeConflict`) | Resume, Ship?, Drop (ship only if not blocked) | resolve-merge-conflicts, [ship], resume, drop | resolve-merge-conflicts, drop |
| Worktree missing, branch present | Repair, Drop (`:216-217`; `task_is_known_invalid` is false, `:257-258`) | drop (Repair unsupported, `actions.rs:82-89`) | drop |
| Worktree and branch missing | Drop only (`:211-213`) | drop | drop |
| Checkout mismatch | Repair, Resume, Drop (`:195-202`) | resume, drop | drop |
| Merged | Drop (Clean/Remove eligible; `operation.rs:63-70`) | drop | drop |
| Cleanable / removing / removed | Drop | drop | drop |

Notes:
- The "Conflicted" row's Ship depends on `merge_preflight_blocked_reasons`
  (`commands/merge.rs:81-90`), which refuses when conflicted; expect no Ship. Verified by reading.
- Chat tasks never get `Review` from core (section 1.1).
- Chat tasks never get `Start`, `Repair` (web-hidden) or a merge-PR action.

### 1.4 Which target verbs exist

| Target verb | Exists? | Name / ID | Route | Handler |
|---|---|---|---|---|
| Continue (compose) | Yes, client-side | WS message `prompt` | `GET /api/tasks/{*handle}` with `/session` suffix → WS upgrade (`runtime/task_routes/cockpit.rs:412` dispatch in `axum_task_get` `:376`; `live.rs:158` `axum_task_session`; upgrade `live.rs:~250`) | `SessionClientMessage::Prompt` (`slices/web_session/mod.rs:78-…`) → `apply_client_message` (`task_session_directory.rs:854-875`) → `submit_prompt_with_id` (`:537`). Composer: `features/chat/composer/useComposer.tsx`, transport `features/chat/session/transport/webSessionTransport.ts`. |
| Stop (cancel turn) | Yes, WS message | `cancel` `{keepQueue}` (`mod.rs:86-89`) | same WS | `apply_client_message` `Cancel` arm (`task_session_directory.rs:876-879`) → `directory.cancel` (`:555-563`) → `TaskSessionCommand::Cancel` → `task_session_exit.rs:683-709` (`cancel`). Client: composer Enter while busy → `sendCancel(true)` (`useComposer.tsx:401-405`; transport `webSessionTransport.ts:212-217`). Not an operator action; no button. |
| Restart agent | **No** | none | none | none (building blocks in section 2) |
| Fix CI | Yes (remediation) | `fix-ci` (`remediation.rs:8`, label "Fix CI" `:33-38`) | `POST /api/actions` and `/api/operations` (`runtime/mod.rs:115-116`) → `axum_action` (`live.rs:625`) → chat branch when `is_chat_remediation` (`live.rs:574-583`) | `chat_remediation_response` (`live.rs:598`) → `deliver_remediation` (`slices/ci_agent_delivery.rs:30-53`) → `submit_chat_prompt` (`:58-…`) |
| Resolve conflicts | Yes (remediation) | `resolve-merge-conflicts` (`remediation.rs:9`, label "Resolve merge conflicts" `:24-31`) | same | same |
| Create PR | **No** | none | none | none |
| Merge PR | **No** | none | none | none (Ship is different, section 4) |
| Drop | Yes | `drop` (`OperatorAction::Drop`, `projection.rs:181`) | `POST /api/actions` | `operate/mod.rs:108` → `execute_drop` (`:484`); `live.rs:744-745` `cleanup_session` after success |

Also existing and hidden for chat: `resume` (`operate/mod.rs:115-118`), `ship`
(same arm, `task_command_kind` `:571-581`), `review` (same arm). `repair` is refused
by web (`operate/mod.rs:112-114`, `bridge.rs:196-212`).

---

## 2. Stop and Restart: existing APIs and re-provisioning

### 2.1 Session APIs (`crates/ajax-web/src/slices/web_session/`)

- `TaskSessionDirectory` (`task_session_directory.rs`, 986 lines, **at the 1000-line hard max**):
  - `acquire` / `acquire_typed` `:409-442` (ensures entry, sends Acquire).
  - `release` `:444-456` (holder release).
  - `drop_session` `:458-479` (sends `Shutdown{close:true}`, clears stored ACP session id).
  - `cleanup_session` `:495` (drop + delete session storage).
  - `submit_prompt_with_id` `:537`, `cancel` `:555-563`.
  - `reset_harness_context` `:618-641`, `clear_context` `:643`, `start_fresh` `:648`.
  - `attach_snapshot` `:675`.
- `TaskSessionCommand` variants: `task_session.rs:41-…`
  (Acquire, Release, SubmitPrompt, Cancel, AnswerPermission, AnswerElicitation,
  ApplyConfigOption, ResetHarness, ClearContext, …). **No `Respawn` variant.**
- `acquire` (task_session_spawn.rs:32-…): if a client exists and agent differs →
  `reset_harness_context`; if the slot is dead (`slot_must_replace`, `transcript.rs:248-250`:
  `!acp_alive || host_exited`) it releases the dead client, tries adopt, then
  `spawn_acp` with the stored resume id (`task_session_spawn.rs:49-71`). **This is the
  existing "re-spawn the child" path**: a dead child is replaced on every acquire.
- `respawn(state, worktree, model, force)` at `task_session_spawn.rs:245-277`: releases
  the live client and spawns a new one, keeping the resume id. Its only caller is
  the ApplyConfigOption path when the host exited (`task_session_spawn.rs:175`). It is
  not reachable as a verb.
- `reset_harness_context` (`task_session_spawn.rs:279`): switches harness/model.
- `cancel` in the session state: `task_session_exit.rs:683-709`. It removes queued
  prompts unless `keep_queue`, calls `client.cancel()`, and marks the active prompt
  cancel-requested.

### 2.2 WebSocket session messages (the control surface today)

`SessionClientMessage` (`web_session/mod.rs:78-…`): `prompt`, `cancel`,
`set_config_option`, `permission`, `elicitation`, `clear`, `retry_restore`,
`start_fresh`. `retry_restore` and `start_fresh` are only accepted during restore
recovery (`ws_bridge.rs:117-124`, else error "restore recovery requires Retry or
Start fresh" at `:126-131`).

### 2.3 Session attach preconditions

- `prepare_task_session` (`prepare_session.rs:13-…`): rejects non-ACP agents
  (`:19-21`, `NotOrchestrationChat`), **promotes a TerminalAgent task to chat**
  by calling `set_skip_interactive_agent(true)` when no live tmux agent is running
  (`:26-33`), then returns `WorktreeMissing` if `!task.worktree_path.exists()`
  (`:43-44`).
- The WS route maps `WorktreeMissing` → 409 `"worktree missing"` (`live.rs:217-219`).
- `submit_chat_prompt` (`ci_agent_delivery.rs:58-…`) does **not** check the worktree
  before `acquire`. (Fact from reading; behavior when missing is speculation.)

### 2.4 Worktree missing: detection and repair

- Detection: `Task::has_missing_worktree` (`models/task.rs:241-249`:
  `WorktreeMissing` flag or `git_status.worktree_exists == false` or runtime health
  `MissingWorktree` or live status). `has_missing_substrate` at `task.rs:204-206`.
- Repair plan: `task_operations/repair.rs:10-…` →
  `commands::task_window_repair_plan_with_open_mode` (`commands/task_window.rs:17-…`).
  - If `git_status.worktree_exists == false` and the branch exists, it plans
    `git -C repo worktree add <path> <branch>` (`task_window.rs:29-47`).
    If the branch is also missing it blocks: "task worktree is missing" (`:31-36`).
  - **It then unconditionally plans tmux**: `new_detached_task_session` if no
    session (`:49-58`), kill/ensure window (`:59-80`). It does not check
    `skip_interactive_agent`. For a chat task this would create tmux state. (Fact:
    no `skip` check in `task_window.rs:17-80`.)
  - Plan then adds `check` commands (`repair.rs:~30-45`).
- Repair is refused in web (`operate/mod.rs:112-114`). The only worktree re-provision
  path reachable from the web is therefore none. The CLI/TUI can run it (`ajax-cli/src/cockpit_actions.rs:531`).

### 2.5 What "Restart agent" would reuse (facts)

- Dead child → `directory.acquire` already respawns with the stored resume id
  (section 2.1). A `Restart` verb could call `acquire` after `drop_session`
  only if the resume id is kept; `drop_session` clears it (`task_session_directory.rs:458-479`).
  Choosing `release`+`acquire` vs `respawn(force=true)` is a design decision.
- Worktree re-provision for chat: needs a new plan with no tmux steps. The
  existing `repair` plan cannot be reused unchanged (tmux steps, section 2.4).
- Speculation: a new `TaskSessionCommand::Respawn` would be needed to run
  `respawn(force=true)` on a live directory entry; today it can only be reached
  from the ApplyConfigOption path.

---

## 3. Fix CI / Resolve conflicts and Create PR

### 3.1 How Phase 0.2 delivers remediations today

- Remediation IDs: `remediation.rs:8-9`. `is_remediation_action` `:45-47`.
- Offered when flagged: `remediations_for_task` `:26-43` (conflict:
  `has_merge_conflict` `:49-59`; CI: `has_ci_failure` `:61-67`).
- Brief text: `format_brief` `:69-91` (names skill path, task handle, base branch).
- Skill path: `skill_name` `:122-128` → `resolve_skill_path` (`ajax-web/src/adapters/skills.rs:3`).
  Missing skill → `UnsupportedCapability` (`remediation.rs:98-107` for chat; `operate/mod.rs:600-…` for tmux).
- **Chat path**: `POST /api/actions` → `axum_action` (`live.rs:625`) → if
  `is_chat_remediation` (`live.rs:574-583`: remediation ID and task `skip_interactive_agent()`),
  it takes a registry snapshot without the control lane (`live.rs:589-596`) and
  calls `chat_remediation_response` (`live.rs:598-621`) → `deliver_remediation`
  (`ci_agent_delivery.rs:30-53`) → `chat_remediation_brief` (`remediation.rs:93-113`)
  → `submit_chat_prompt` (`ci_agent_delivery.rs:58-…`): `acquire` → `attach_snapshot`
  (busy check) → `submit_prompt_with_id` (client message id
  `remediation:{id}:{handle}:{nanos}`, `ci_agent_delivery.rs:41`) → `release`.
  Output: "queued behind the active chat turn" when busy, else "sent to chat task"
  (`ci_agent_delivery.rs:49-50`).
- **Yes, these already go through the chat queue** (`submit_prompt_with_id` on
  the per-session FIFO). Test: `ci_agent_delivery/tests.rs:132-170`
  `chat_task_remediation_submits_brief_on_chat_queue_without_tmux`.
- **Tmux path (terminal tasks only)**: `execute_remediation` `remediation.rs:130-164`
  sends via `send_agent_command` into the tmux window (`:159`), and requires a
  live tmux status (`:145-156`). Still used by `operate/mod.rs:600-625` and CLI
  `cockpit_actions.rs:283, 434`.
- The non-chat `operate_inner` checks remediation before the chat/terminal split
  (`operate/mod.rs:99-101`), so a chat task that bypasses `live.rs` would hit the
  tmux path. Today `live.rs` intercepts all chat remediations first (`:673`).

### 3.2 Create PR: what exists today

- No Rust code calls `gh pr create` (grep over `crates/`: none).
- Repo tooling: `scripts/gh-pr-create` (repo-development agents only, per
  `docs/agent/pull-requests.md:115-118`). It requires a `type/scope/description`
  branch (`scripts/gh-pr-create:14-21`), **runs `git add -A` and commits dirty
  worktrees** (`:33-37`), pushes, and creates the PR.
- Docs rule: "Do not call raw `gh pr create`" (`docs/agent/pull-requests.md:115-117`).
  This is a repo rule for development agents. Whether it binds product chat
  agents is **a decision for Matt** (see section 8).
- The PR title rule is in `docs/agent/pull-requests.md:119-131`. Release
  trigger rules are in the memory notes, not in this repo.

### 3.3 What a chat-prompt Create PR would send (proposal, grounded)

- Shape: mirror `format_brief` (`remediation.rs:69-91`): a brief naming the task
  handle, branch (`task.pr_head_branch()`, `models/task.rs:265-273`), base branch
  (`task.base_branch`), and the instruction to open the PR. Deliver with
  `submit_chat_prompt`, the same as remediations.
- Gaps: no skill exists for PR creation in `resolve_skill_path` roots
  (`skills.rs:18-…`). A brief that does not need a skill avoids the
  `UnsupportedCapability` gate (`remediation.rs:98-107`).
- Speculation: the agent would run `gh pr create`; there is no product-side
  CommandSpec for it today.

---

## 4. Merge PR

### 4.1 Existing merge code

- **Ship** (the CLI `ajax ship` and web `ship`):
  - Plan: `commands/merge.rs:12-…` `merge_task_plan`. Requires Reviewable/Mergeable
    lifecycle (`:17-22`) and clean worktree evidence (`:81-90`
    `merge_preflight_blocked_reasons`). Commands are `git -C <repo> switch <base>`
    and `git -C <repo> merge --ff-only <branch>` (`commands/merge.rs:38-42`,
    `adapters/git.rs:160-169`). `requires_confirmation` when any side flag is set (`merge.rs:38`).
  - Execute: `task_operations/ship.rs:14-…` `execute_ship_operation` → `execute_ship_plan` (`ship.rs:53-…`)
    (`ship.rs:53`). After success `mark_task_merged` (`ship.rs:98`; body `commands/merge.rs:47-…`).
  - Ship is **local**: it switches the repo root to base and merges the branch.
    It does not call `gh`.
- Ship is withheld when a PR is open (`recommended.rs:231-242`) and when CI failed.
- `ajax-core/src/architecture.rs:77-86`: `commands/merge.rs` and `check.rs` must not
  contain `.get_task_mut(`. Any Merge-PR bookkeeping in `commands/merge.rs` must use
  typed helpers.

### 4.2 gh usage today

- All `gh` calls are built as `CommandSpec` by `GithubChecksAdapter`
  (`crates/ajax-core/src/adapters/github.rs`) and run through `CommandRunner`
  (`adapters/command.rs:47-49`). `GH_PR_CHECKS_TIMEOUT` = 30 s (`github.rs:7`).
- Specs: `pr_checks` (`github.rs:46-53`, `gh pr checks <branch> --json name,state,link`),
  `pr_checks_for_pr` (`:55-…`), `pr_list` (`:83-99`, `gh pr list --state all --head <branch> --json number,title,url,state,headRefName,headRefOid`),
  `pr_diff` (`:122`, `gh pr diff <n>`). Git `local_diff` is also in this file (`:128`).
- Callers of `pr_list`: `runtime_refresh/ci_monitor.rs:142-151` (discovery every
  `DISCOVERY_INTERVAL_SECS` = 300, `ci_monitor.rs:16`), `diff_review.rs:477-481`
  (`observe_task_pull_requests`), `slices/diff_review.rs` (web).
- Sandbox: `adapters/process.rs` has no sandbox reference (grep). `adapters/sandbox.rs`
  wraps agent launches (`wrap_argv` `:179`, `sandbox_command` `:223`); the web
  program adapter imports it (`ajax-web/src/adapters/program.rs:9`). **Fact:** gh
  calls via `CommandRunner` are not sandbox-wrapped in the code read here.
  **(speculation)** whether that is intended.
- No `gh pr merge` in any file (grep over `crates/`).

### 4.3 PullRequestRef and how the associated PR is found

- Struct: `crates/ajax-core/src/diff_review.rs:16-23`
  (`number, title, url, state: PullRequestState, head_ref, head_sha: Option<String>`).
  Constant `AJAX_PULL_REQUESTS_KEY = "ajax_pull_requests"` at `diff_review.rs:6`.
- Stored as JSON task metadata: `stored_pull_requests` `:319-324`, `remember_pull_requests` `:326-333`,
  `merge_pull_request_lists` `:335`.
- Discovery writes: `runtime_refresh/ci_monitor.rs:131-161` (`discover_open_pr`):
  `pr_list` by `task.pr_head_branch()`, store JSON at `:148-151`, clear probe error `:152-153`,
  then pick the newest Open PR (`:163-165`).
- Lifecycle reconcile from PRs: `ci_monitor.rs:366-…` (`reconcile_lifecycle_from_prs`).
- `has_open_pull_request` (`operation.rs:101-104`) reads the stored metadata.
  Therefore **Merge PR visibility depends on stored metadata**, refreshed by
  discovery (every 300 s) or the diff route (`diff_review.rs:472`).
- Web projection: `PullRequestDto` (`ajax-web/src/slices/diff_review.rs:14-21`);
  TS `PullRequestView` (`web/src/shared/lib/types.ts:13-20`).
- Head branch: `task.pr_head_branch()` (`models/task.rs:265-273`).

### 4.4 Proposed Merge PR (grounded)

- A new `CommandSpec` on `GithubChecksAdapter` (e.g. `pr_merge(worktree, number)`,
  args `["pr","merge",<n>,...]`), run via `CommandRunner` with the same 30 s
  timeout. Merge strategy flags are a decision (existing Ship uses `--ff-only` on
  git, not gh).
- Gate: needs an Open `PullRequestRef` (`stored_pull_requests`), no checks failing
  (`ci_monitor` state), no conflicts. Confirmation required (Ship sets
  `requires_confirmation` for any side flag, `merge.rs:38`).
- After success: lifecycle should move to Merged through the existing reconcile
  (`ci_monitor.rs:366-…`) or `mark_task_merged`. **(speculation)** which one.

---

## 5. Where additions/deletions could come from; dashboard rendering

### 5.1 Sources

- **`gh pr list --json`**: `additions` and `deletions` are valid fields (verified
  with `gh pr list --json` help in this environment). The current `pr_list` field
  list is `github.rs:94`, and it is pinned in tests (section 6). Adding the fields
  there changes pinned assertions.
- **Alternative without changing the pinned list**: a separate
  `gh pr view <n> --json additions,deletions` spec (new method on
  `GithubChecksAdapter`), called from `discover_open_pr` (`ci_monitor.rs:131-161`).
  **(speculation)** that it is acceptable cost (one extra gh call per discovery).
- Existing additions/deletions in the codebase are **file-level diff stats** from
  the local/PR diff, not PR totals: `DiffFile` (`diff_review.rs:40-45`),
  `DiffTotals` (`diff_review.rs:51-57`), `project_task_diff` (`diff_review.rs:487-…`).
  Summing them is possible but only when a diff is loaded.

### 5.2 Storage fields

- `PullRequestRef` (`diff_review.rs:16-23`) has no stats fields. Construction
  sites (17 `PullRequestRef {` literals across 11 files; some are production):
  `ajax-core/src/adapters/github.rs:163`, `ajax-core/src/diff_review.rs:530`,
  `ajax-core/src/recommended.rs:763, 797`,
  `ajax-core/src/diff_review/tests.rs:13, 57, 65`,
  `ajax-core/src/attention/tests.rs:341`,
  `ajax-core/src/attention/poll_notify_tests.rs:43, 104, 178`,
  `ajax-core/src/runtime_refresh/ci_monitor/tests.rs:38`,
  `ajax-core/src/runtime_refresh/tests/mod.rs:154`,
  `ajax-web/src/slices/diff_review.rs:319, 409`,
  `ajax-cli/src/lib/tests/mod.rs:50`,
  `ajax-cli/src/web_backend/tests/suite_2.rs:339`.
  **Adding a non-`Default` field forces every literal to change in one packet**
  (section 9, packet 4.3).
- Serde: stored metadata is JSON. `#[serde(default)]` on new fields keeps old
  stored records readable (`stored_pull_requests` uses `serde_json::from_str(...).unwrap_or_default()`,
  `diff_review.rs:319-324`, so an unreadable record silently becomes empty).
  **Fact:** that failure mode would drop PRs, not just stats. Note it for the design.

### 5.3 Dashboard rows today

- Web dashboard row: `features/task/TaskList.tsx` `TaskRow` renders title, status,
  handle and time (`TaskList.tsx:70-200`; time `:192`, meta `:197-199`).
  **No diff stats are rendered in dashboard rows today.**
- Diff stats are rendered only on the diff page: `features/diff/DiffReview.tsx`
  (uses `additions`/`deletions` from `DiffFileView`, `types.ts:29-37`,
  `DiffTotalsView` `types.ts:38-44`).
- `BrowserTaskCard` (`types.ts:110-123`) and its Rust builder
  `cockpit/mod.rs:79-90` carry no PR or diff stats. A dashboard field would need a
  new card field fed from `stored_pull_requests` (no network at cockpit read time).
- Contract fixtures compare only selected card fields
  (`cockpit/tests.rs:494-514` compares `repo, qualified_handle, title, status,
  status_explanation, attention, actions`), so adding a card field does not break
  those assertions. `browser_contract_fixture_has_stable_card_shape` (`:455-476`)
  checks `actions` shape only.

---

## 6. Tests that pin current action sets (and contradictions)

Files under a top-level `tests/` directory (cannot be modified without
Matt's approval): `crates/ajax-cli/tests/{live_cli.rs, session_shim.rs,
smoke_user_flows/suite_1.rs, suite_2.rs}`, `crates/ajax-core/tests/{wave0_run_state_characterization.rs, wave2_live_apply_contract.rs}`,
`crates/ajax-web/tests/fixtures/`. Their action assertions (`ship`, `drop`,
`resume`, `repair`) use terminal fixtures (no `skip_interactive_agent` set), so
they are compatible. `live_cli.rs:659` loops over `["resume","review","ship","drop","repair"]` for terminal CLI verbs.

Inline / sibling test modules (editable under the rules, but assertions must not
be weakened):

### 6.1 Direct contradictions with the target

- **`crates/ajax-core/src/recommended.rs:703-720`
  `chat_task_without_tmux_session_or_window_is_not_invalid`** asserts a chat
  task offers `Ship` (`:714-717`) and is not `Drop`. Target: Ship only for
  TerminalAgent. **BLOCKED** for any packet that hides Ship for chat tasks, until
  Matt approves changing this assertion (STOP rule: an existing test contradicts
  the required behavior).
- **`crates/ajax-core/src/models/tests.rs:580-594`
  `operator_action_labels_are_operator_facing`** asserts the exact six labels.
  Any new `OperatorAction` variant (Merge, Stop, Restart, CreatePr) breaks it.
  Also `crates/ajax-cli/src/lib/tests/suite_10.rs:115-130` (covered actions equal
  `OperatorAction::all()` labels plus `status`) and
  `crates/ajax-tui/src/lib/tests.rs:2774-2779`
  (`current_core_actions_have_dedicated_render_metadata` needs a TUI glyph for
  every variant). Also `ajax-tui/src/actions.rs:52-57` matches exhaustively.
  **Decision needed:** keep new chat verbs outside `OperatorAction` (recommended;
  no assertion changes) or approve updating these three assertions.

### 6.2 Tests that are compatible (terminal fixtures or chat-only behavior)

- `crates/ajax-web/src/slices/cockpit/tests.rs:14-52`
  `cards_mark_only_provisioned_acp_tasks_as_session_capable` (chat session_capable;
  no action assertion). `:54-68` (`issue_1092`). `:173-208` checkout mismatch
  (terminal; `resume`, `drop`). `:284-325` mobile card shape (terminal;
  `["resume","review","ship"]`). `:363-407` `cockpit_cards_expose_only_executable_web_actions`
  (terminal; `resume, review, drop`).
- `crates/ajax-web/src/slices/actions.rs:101-212` (in-file): resume/review
  supported; `sync` unsupported; fix-ci remediation button; ship confirmation.
  Pins `supported_browser_action("sync") == false`. Adding new web action names
  must keep `sync` unsupported.
- `crates/ajax-core/src/runtime_refresh/ci_monitor/tests.rs:385-400` and
  `:440-455`: red CI withholds Ship (terminal fixtures; no skip flag set). Compatible.
- `crates/ajax-core/src/recommended.rs:682-700`: shell repair and Ship for a
  clean reviewable **terminal** task. Compatible.
- `crates/ajax-web/src/slices/operate/tests.rs:68-135`
  (resume, ship via core; terminal fixture); `:177-210` fix-ci via tmux (terminal).
  Compatible.
- `crates/ajax-web/src/slices/ci_agent_delivery/tests.rs:17-170` (chat queue
  delivery, chat remediation). Compatible; it pins the chat path Phase 4 reuses.
- `crates/ajax-core/src/remediation.rs:271-350` (tmux remediation for terminal;
  `chat_remediation_brief_needs_no_tmux_session` for chat). Compatible.
- `crates/ajax-core/src/models/tests.rs:485-500` `kind_is_chat_exactly_when_interactive_agent_is_skipped`
  (kind only). Compatible.
- `crates/ajax-web/src/runtime/tests/suite_3.rs:333-395` uses chat fixtures for a
  socket health test only. Compatible.
- `crates/ajax-core/tests/wave0_run_state_characterization.rs:94-166` pins chat
  operator **status** (Idle), not actions. Compatible.

### 6.3 Other test-side facts

- `ajax-core/src/shuttle_tests.rs:88-89` and `lifecycle_state_machine_tests.rs:207-208,343-368,395-396`
  use `available_operator_actions`/`operator_action` on terminal fixtures (not checked
  for chat; **spot-check before packet 4.1**).

---

## 7. Architecture guard constraints

### 7.1 ajax-web (`crates/ajax-web/src/architecture.rs`, `architecture_web_session.rs`)

- `SLICES` list (`architecture.rs:121-134`, 12 names) and `ADAPTERS` list
  (`:135-150`, 13 names). A new slice or adapter directory must be added to these
  lists and to the `mod.rs` declarations (`guarded_modules_match_declared_modules`,
  `architecture.rs:455-…`).
- `each_web_slice_is_isolated_from_sibling_slices_and_runtime` (`:242-256`): slices
  cannot import each other or `runtime`. Create PR / Merge PR code in a slice must
  not import `operate`, `ci_agent_delivery` or `web_session`; use `ajax_core`.
- `actions_module_does_not_depend_on_sibling_slices_or_runtime` (`:224-240`):
  `slices/actions.rs` may not import sibling slices or runtime. Any new web verb
  gating in `actions.rs` must use core types only.
- `each_web_adapter_does_not_depend_on_slices_or_runtime` (`:208-222`).
- `architecture_web_session.rs`: protocol mapping modules (`protocol`, `acp_map`,
  `normalize`, `acp_usage`, `:7-24`) must not import `web_session_store`,
  `AcpStdioClient`, `task_session_directory`, `task_session_spawn`, `acp_drain`.
  `replay` (`:26-36`) and `transcript_model_change` (`:38-47`) have similar lists.
  `ws_bridge` (`:49-…`) may not import `web_session_store`, `AcpStdioClient`,
  `acp_drain`, `acp_map`, `StreamNormalizer`. A Stop/Restart WS wiring must go
  through `TaskSessionDirectory` only.
- `task_session_directory.rs` is 986 lines (hard max 1000, `AGENTS.md`). **Do not
  add Stop/Restart code to that file**; put new directory methods in a new
  sibling module.

### 7.2 ajax-core (`crates/ajax-core/src/architecture.rs`)

- `OPERATION_SLICES` = 7 names (`architecture.rs:5-14`); `each_task_operation_slice_is_isolated`
  (`:88-113`), `each_task_operation_slice_declares_its_operation_entry_points` (`:116-132`,
  requires `pub fn plan_*` and `pub fn execute_*`), `task_operations_submodules_are_file_backed`
  (`:32-58`, `task_operations.rs` must declare file-backed `kernel`, `operator_dispatch`,
  `start`, `resume`, `review`, `repair`, `ship`, `drop_task`, `sweep_cleanup`, …).
  A new `merge_pr` operation must follow the `plan_`/`execute_` shape and be
  file-backed. Adding it to `OPERATION_SLICES` is needed to get isolation coverage.
- `commands_do_not_import_operator_slices` (`:170-186`): `commands/` must not import
  `task_operations::{slice}`. Put any Merge-PR plan in `task_operations/`, not `commands/`.
- `adapters_do_not_depend_on_task_operations` (`:156-168`); shared kernel
  (`KERNEL_MODULES`, `architecture.rs:17`) must not depend on commands/task_operations (`:134-155`).
- `commands/merge.rs` and `commands/check.rs` must not contain `.get_task_mut(`
  (`:77-86`).
- Lifecycle inline tests: `lifecycle.rs:393` splits files at inline `#[cfg(test)]`
  (memory note). Do not move ajax-core inline tests to sibling `tests.rs` files.
- `ajax-core/src/models/projection.rs`: `OperatorAction` changes break
  `models/tests.rs:580` and TUI/CLI tests (section 6.1).

### 7.3 File sizes (AGENTS.md: target ~600, hard 1000)

- `task_session_directory.rs` 986 (at cap; do not grow).
- `recommended.rs` 811 (over the 600 target; under cap).
- `task_session_exit.rs` 773, `live.rs` 749, `operate/mod.rs` 685,
  `adapters/github.rs` 666, `task_session.rs` 655, `task_session_spawn.rs` 585,
  `diff_review.rs` 594, `commands/merge.rs` 90, `task_operations/ship.rs` 118,
  `ci_agent_delivery.rs` 116, `remediation.rs` 351, `operation.rs` 264.
- New Phase 4 code goes in new files (e.g. a chat-verb module) rather than
  `task_session_directory.rs`, `recommended.rs` or `live.rs`.

---

## 8. Decisions needed from Matt (before packets 4.1+ run)

1. **Ship for chat tasks (BLOCKING).** Approve updating
   `recommended.rs:703-720` so a chat task does not offer Ship, or keep Ship for chat and
   drop the Ship line from the target.
2. **New verbs outside `OperatorAction`.** Recommended: model Create PR, Merge PR and
   Restart as chat-verb IDs (like remediation IDs) so `models/tests.rs:580`,
   `suite_10.rs:115` and the TUI test are not touched. Confirm.
3. **Fix CI / Resolve conflicts visibility.** Today they appear only when flagged
   (`remediation.rs:26-43`). Target says chat tasks "show" them. Always show, or
   only when flagged? Recommend flagged-only plus Create PR when no PR exists.
4. **Create PR rule vs. `docs/agent/pull-requests.md:115-118`.** The repo says agents
   always use `scripts/gh-pr-create`. Confirm whether product chat agents may run
   `gh pr create` from a brief, or whether the product should call `gh pr create`
   itself through the CommandRunner. The repo script commits dirty worktrees
   (`scripts/gh-pr-create:33-37`); that is not safe as a product action.
5. **PR additions/deletions source.** (a) Add to the `pr_list` JSON (changes
   pinned strings in `github.rs:94`, `:594` and `ajax-cli/src/lib/tests/mod.rs:342,382`),
   or (b) a separate `gh pr view --json additions,deletions` call (no pinned-string
   change). Recommend (b).
6. **PullRequestRef struct change vs. separate key.** A new field touches 17 literal
   sites in 11 files (one mechanical packet over the 4-file cap). A separate
   metadata key keeps the struct unchanged and fits the cap. Confirm which.
7. **Restart semantics.** `release`+`acquire` (keeps resume id only if not dropped)
   vs. a new `Respawn` directory command for a live entry; and whether a missing
   worktree re-provision for chat must skip tmux (recommended: yes; the repair plan
   plans tmux unconditionally, `task_window.rs:49-80`).
8. **Merge PR strategy.** gh merge flags (merge/squash/rebase) and whether it
   should refuse when local Ship would not be ff-only.

---

## 9. Proposed slicing (packets, ≤4 files each)

Each packet must compile and pass its own nextest subset. "BLOCKED" = would
contradict an existing assertion; do not run until Matt decides.

**P4.1 Chat-only action gating: BLOCKED (§6.1).** Files: `recommended.rs`, `models/tests.rs`
(not allowed to change without approval), `ajax-web/src/slices/actions.rs`. Contradicts
`recommended.rs:703`. Do not start until Decision 1.

**P4.2 Chat verb IDs and brief builders (core, no action-set change).** Files:
`ajax-core/src/remediation.rs` (add `CREATE_PR`, `MERGE_PR`-free brief text; keep
`FIX_CI`/`RESOLVE_*` unchanged), `ajax-core/src/remediation/…` if split is needed (new
file, keeps remediation.rs under 600). Tests: new tests in the same module. Does
**not** touch `OperatorAction`, so the pinned tests stay green. Compiles alone.
Not BLOCKED (depends on Decision 2 for ID names).

**P4.3 Restart semantics: core plan (chat-only, no tmux).** Files:
`ajax-core/src/task_operations/restart_chat.rs` (new, `plan_`/`execute_` shape), 
`ajax-core/src/task_operations.rs` (submodule declaration), `ajax-core/src/architecture.rs`
(add `restart_chat` to `OPERATION_SLICES` only if isolation coverage is wanted).
Compiles alone; no existing assertion touched. Not BLOCKED; needs Decision 7.

**P4.4 Restart/Stop wiring in the web directory (new module).** Files:
`ajax-web/src/slices/web_session/task_session_restart.rs` (new; calls
`directory.drop_session`/`acquire`/worktree check), `ajax-web/src/slices/web_session/mod.rs`
(declare module + re-export), `ajax-web/src/slices/web_session/task_session_directory.rs`
(**no edits allowed, 986 lines**; wire from the new module instead), `ajax-web/src/architecture.rs`
(only if a new file must be listed). Compiles alone. Not BLOCKED.

**P4.5 Create PR and Fix CI/Resolve delivery (web, chat queue).** Files:
`ajax-web/src/slices/ci_agent_delivery.rs` (add `deliver_chat_verb`, reuse
`submit_chat_prompt`), `ajax-web/src/runtime/task_routes/live.rs` (route the new IDs
through `is_chat_remediation`; **live.rs is 749 lines, +≤20 only**), `ajax-web/src/slices/actions.rs`
(extend supported browser actions list, not OperatorAction), `ajax-core/src/remediation.rs`.
Compiles alone. Not BLOCKED; needs Decisions 2-4.

**P4.6 Merge PR: gh adapter method.** Files: `ajax-core/src/adapters/github.rs`
(`pr_merge` CommandSpec + tests; **666 lines**, ensure a new file if the parser grows),
`ajax-core/src/adapters/mod.rs` (if re-export needed). Pure spec + parser tests. Not BLOCKED.

**P4.7 Merge PR: core operation.** Files: `ajax-core/src/task_operations/merge_pr.rs`
(new, `plan_merge_pr_operation` / `execute_merge_pr_operation`), `ajax-core/src/task_operations.rs`
(declare), `ajax-core/src/architecture.rs` (add `merge_pr` to `OPERATION_SLICES`),
`ajax-core/src/runtime_refresh/ci_monitor.rs` only if reconcile hook is needed (430 lines).
Not BLOCKED. Depends on P4.6 and Decision 8.

**P4.8 PullRequestRef stats (separate key, Decision 6b).** Files:
`ajax-core/src/diff_review.rs` (new `AJAX_PULL_REQUEST_STATS_KEY` + helpers; no struct
change), `ajax-core/src/runtime_refresh/ci_monitor.rs` (call `gh pr view` after
discovery at `:131-161`), `ajax-core/src/adapters/github.rs` (`pr_view_stats` spec +
parser), `ajax-core/src/diff_review/tests.rs` (new tests). Compiles alone. Not BLOCKED.
**If Decision 6a (struct field) is chosen instead:** a single mechanical packet touching
11 files (17 literal sites, §5.2). That exceeds the 4-file cap; it can compile only as
one unit. Mark as exception-requiring.

**P4.9 Dashboard PR stats: web projection.** Files: `ajax-web/src/slices/cockpit/mod.rs`
(`BrowserTaskCard` new field from stored stats; 208 lines), `ajax-web/src/slices/diff_review.rs`
(PR DTO fields), `ajax-web/web/src/shared/lib/types.ts` (TS types),
`ajax-web/web/src/features/task/TaskList.tsx` (render). Compiles alone (Rust side). TS
side: `fixtures.test.ts` and `web/src/fixtures/cockpit.json` may need a shape update
**(speculation; not read)**. Not BLOCKED on Rust side; check TS fixtures before
merging.

**P4.10 Web client verbs (Continue/Stop/Restart/Create PR/Merge PR buttons).** Files:
`ajax-web/web/src/features/task/ActionBar.tsx` (remediation class set at `:37`),
`ajax-web/web/src/features/task/taskActions.ts` (`visibleTaskActions`, `:3-5`),
`ajax-web/web/src/features/task-workspace/TaskWorkspace.tsx` (chat head actions `:113-124`),
`ajax-web/web/src/shared/lib/types.ts`. Gate: `CI=1 npm run web:smoke` per the memory
note (layout changes). Not BLOCKED.

**P4.11 Docs.** `architecture.md` "Chat-first" section and `docs/architecture/web-cockpit.md`
(owners, per the plan's Phase 2–4 rule). Not BLOCKED; docs only.

Blocking summary: **P4.1 is the only packet BLOCKED by an existing assertion.**
Everything else avoids the pinned sets by keeping new verbs out of `OperatorAction`
and by not changing the pinned `pr_list` argument string.

---

## Verification (this discovery)

- No code changed. Only this file was written.
- `test -s .planning/agent-plans/chat-first-core-phase4-discovery.md` is run after writing.
- `gh pr list --json` (no args) was run to list valid fields; that is the only external command.
