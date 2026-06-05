# Ajax CLI Post-Startup Code Review Brief

You are performing a deep post-startup code review of the Ajax CLI repository.

## Context

Ajax CLI is a Rust-based operator CLI for managing isolated AI coding tasks. It creates and manages Git worktrees, tmux sessions, agent runs, task lifecycle state, reviewable work, cleanup flows, and terminal/mobile SSH operator workflows.

This review is specifically about what happens AFTER the CLI has started.

Do NOT focus on startup time unless startup work directly affects post-startup command responsiveness. The target is perceived speed and reliability while the operator is already using Ajax: menus, task switching, dropping into tmux, opening tasks, refreshing status, cleanup, backend API calls, and repeated command usage.

This is not a nitpick review.

Do NOT focus on:

- Tiny style issues
- Minor formatting
- Small naming preferences
- Theoretical micro-optimizations
- Cosmetic refactors
- Generic Rust advice
- Startup-only issues
- Issues that do not affect post-startup speed, reliability, behavior, or maintainability

## Goal

Find medium-sized Ajax-specific issues that compound into noticeable performance problems, UX slowdown, command lag, slow task switching, slow drop/open behavior, backend API latency, flaky terminal behavior, or hard-to-maintain code. Also find any major architectural or implementation issues that could have a large negative impact on real operator use.

The review should focus on real Ajax workflows, not generic CLI theory.

## Primary Ajax Workflows To Review

### 1. Task list / menu interaction

- How Ajax gathers task state before rendering menus
- Whether task lists require repeated filesystem scans, Git calls, tmux calls, process checks, or status hydration
- Whether menu rendering waits on expensive status collection
- Whether the UI can render fast with stale/basic state and hydrate expensive details later
- Whether repeated menu navigation causes repeated backend work
- Whether mobile SSH/tmux use makes the menu feel slow or fragile

### 2. Task open / resume

- Review the full path from selecting a task to opening/resuming it
- Look for repeated worktree discovery, repo validation, task store reads, tmux session detection, process checks, or agent status checks
- Identify anything that makes open feel delayed, inconsistent, or unreliable
- Check whether open is idempotent when the tmux session already exists
- Check behavior when the worktree exists but state is stale, or state exists but worktree/session is missing

### 3. Drop into tmux / return to operator window

- Deeply inspect tmux boundary code
- Look for excessive tmux queries, repeated session/window/pane lookups, slow attach/switch logic, fragile shell-outs, or blocking checks
- Check whether Ajax treats tmux as an unreliable external dependency with timeouts and recovery paths
- Identify defects around missing sessions, orphaned sessions, renamed windows, killed panes, SSH disconnects, and repeated drop calls
- Evaluate whether drop behavior is optimized for fast operator task switching

### 4. Task create

- Review the create path after Ajax is already running
- Look for unnecessary full task refreshes, repeated repo discovery, repeated config reads, blocking Git/worktree setup, slow prompts, or slow agent/session initialization
- Check whether Ajax shows useful progress before expensive work
- Identify whether task creation blocks the UI longer than necessary
- Check whether failure leaves partial state, orphaned worktrees, or orphaned tmux sessions

### 5. Status refresh / agent status detection

- Review how Ajax determines task status, agent status, process status, tmux status, reviewable status, and cleanable status
- Look for N+1 shell-outs, repeated process checks, polling loops, unbounded checks, excessive serialization, stale state, or inaccurate status derivation
- Identify where status should be cached, batched, bounded, or split into cheap vs expensive status
- Check whether expensive status checks are performed just to render a menu or respond to a simple command

### 6. Cleanup / remove / archive behavior

- Review cleanup and removal paths for speed, idempotency, and safety
- Look for repeated scans, unsafe assumptions, race conditions, stale task state, missing tmux cleanup, missing worktree cleanup, or destructive actions without enough validation
- Check whether cleanup is safely repeatable after partial failure
- Identify performance issues when many tasks exist

### 7. Backend/internal API speed

- Review Ajax internal APIs used by UI commands
- Look for APIs that force callers to do expensive hydration when they only need lightweight state
- Look for chatty APIs, N+1 calls, coarse-grained APIs, excessive cloning, repeated deserialization, repeated filesystem reads, global locks, or blocking IO in hot paths
- Recommend better API shapes for Ajax specifically, such as:
  - list task summaries
  - hydrate one task
  - batch tmux session lookup
  - cheap status snapshot
  - expensive status refresh
  - cached repo context
  - cached task index with invalidation

### 8. Code quality that affects speed or behavior

- Find badly written or poorly structured code only when it increases performance risk, defect risk, or future feature cost
- Look for command handlers that mix UI, orchestration, Git, tmux, process execution, task state, persistence, and rendering
- Look for duplicated logic across open/drop/create/status/cleanup paths
- Look for hidden side effects, unclear ownership, weak state transitions, error swallowing, or overbroad abstractions
- Do not report style-only issues

## Reference-Quality Public Repo Patterns

Use mature public Rust/CLI repos as pattern references, not authority. Compare Ajax against implementation patterns from tools like ripgrep, fd, bat, cargo, helix, zellij, starship, and nushell only where the pattern applies to Ajax post-startup behavior.

Relevant patterns to compare against:

### 1. Lazy IO after command intent is known

Good tools avoid expensive work until it is actually needed.

In Ajax, check whether task menus, open, drop, status, cleanup, and create paths load or refresh more state than needed.

### 2. Bounded shell-outs

Good tools avoid repeated unbounded external process calls.

In Ajax, look for repeated Git, tmux, process, worktree, or filesystem calls during task list, open, drop, and status refresh.

### 3. Cheap summary vs expensive hydration

Good tools separate fast list views from expensive detailed inspection.

In Ajax, task menus should not require full task hydration, full tmux inspection, full agent inspection, or full Git validation unless truly necessary.

### 4. Separated rendering and orchestration

Good terminal tools keep rendering separate from backend work.

In Ajax, flag UI code that directly performs Git, tmux, process, persistence, or lifecycle work.

### 5. Explicit state transitions

Good tools make lifecycle transitions idempotent and recoverable.

In Ajax, review task statuses such as Provisioning, Active, Reviewable, Cleanable, Removed, and Error for unclear transitions, stale state, partial failure bugs, and incorrect recovery behavior.

### 6. Session orchestration reliability

Tools like zellij treat terminal/session state as a first-class reliability concern.

In Ajax, review tmux session creation, detection, reuse, cleanup, switching, and recovery as core behavior, not helper logic.

### 7. Timeout and fallback around slow external calls

Tools like starship avoid letting slow external checks freeze the user experience.

In Ajax, Git/tmux/process calls used by menus, status, open, and drop should be bounded or avoidable.

### 8. Command/core boundary

Tools like cargo have a clearer boundary between command handling and core behavior.

In Ajax, flag command paths where business logic is trapped in CLI handlers and cannot be reused, tested, optimized, or batched.

### 9. Test doubles around external processes

Mature CLI repos make core behavior testable without always invoking real external tools.

In Ajax, inspect whether Git/tmux/process behavior can be tested with fakes or whether every behavior defect requires full integration testing.

## Output Format

### Top Findings

Rank findings by real-world Ajax impact.

For each finding, include:

- Severity: Critical / High / Medium
- Category: UI speed / command speed / drop speed / backend API speed / optimization / code quality / behavior defect
- Affected Ajax workflow: task list / create / open / drop / status / cleanup / tmux / backend API
- Impact: what the operator feels or what can break
- Evidence: specific files, functions, call paths, or code patterns
- Public repo pattern comparison: exact pattern Ajax is violating, such as lazy IO, bounded shell-outs, explicit state machine, separated rendering, command/core boundary, idempotent cleanup, timeout around external calls, cheap summary vs expensive hydration
- Why it matters: explain the compounding or major impact
- Recommended fix: concrete Ajax-specific implementation direction
- Risk of fix: Low / Medium / High

### Highest ROI Fixes

List the 5-10 changes most likely to improve Ajax's real-world responsiveness and reliability after startup.

For each:

- What to change
- Affected workflow
- Why it matters
- Expected impact
- Approximate implementation size: S / M / L
- Suggested first patch

### Ajax Hot Path Review

Review these paths specifically:

- task list/menu render
- task create
- task open
- task drop
- return to operator window
- status refresh
- agent process/status detection
- tmux session detection
- worktree discovery
- repo/config discovery after startup
- cleanup/remove
- backend API calls used by UI commands

For each path:

- Current bottlenecks
- Possible behavior defects
- Repeated work or unnecessary hydration
- External calls involved
- Optimization recommendation
- Whether the fix is high, medium, or low priority

### Backend API Shape Problems

Identify internal APIs that make Ajax slower or harder to maintain.

Look for APIs that:

- Return too much data
- Force full hydration
- Hide expensive IO
- Trigger repeated Git/tmux/process checks
- Cannot batch lookups
- Mix persistence with live status
- Make UI commands accidentally slow

Recommend Ajax-specific replacement shapes, such as:

- TaskSummary vs TaskDetail
- CheapStatusSnapshot vs LiveStatusRefresh
- TmuxSessionIndex
- RepoContextCache
- TaskStoreIndex
- WorktreeStateSnapshot
- AgentProcessSnapshot
- CleanupPlan before CleanupExecute

### Behavior Defects

Focus on defects that affect real usage:

- stale task status
- incorrect reviewable/cleanable state
- orphaned tmux sessions
- orphaned worktrees
- deleted worktree but task still active
- tmux session exists but Ajax thinks it does not
- repeated open/drop creates duplicate state
- cleanup is not safely repeatable
- interrupted create leaves broken state
- mobile SSH disconnect leaves inconsistent state
- process exited but status still active
- Git failure leaves partial task state

For each likely defect:

- Scenario
- Current behavior risk
- Evidence
- Expected behavior
- Recommended fix
- Test that should be added

### Code Quality Issues That Matter

Only include code quality issues that cause speed, reliability, or maintainability problems.

For each:

- File/function/module
- What is badly structured
- Why it affects Ajax performance or behavior
- What boundary should change
- Smallest useful refactor

### Do Not Fix Yet

List low-value issues noticed during review that are not worth addressing now.

Examples:

- style-only issues
- naming preferences
- micro-optimizations
- broad rewrites without clear payoff
- startup-only improvements unrelated to post-startup usage

## Review Rules

- Be direct and critical.
- Do not produce a generic checklist.
- Do not praise the code unless it explains a tradeoff.
- Do not suggest broad rewrites unless there is a clear speed or reliability payoff.
- Use concrete Ajax files, functions, call paths, and behavior as evidence.
- Public repos are pattern references only; do not claim Ajax is wrong just because another repo does it differently.
- Prioritize medium cumulative issues and major performance/behavior issues.
- Ignore small issues unless they compound across hot paths.
- Assume the goal is a fast, reliable operator CLI with minimal friction during real task work.
