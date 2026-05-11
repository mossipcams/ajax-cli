# Live Status Classifier Findings

## Context

Ajax live status is currently derived from captured tmux pane text and monitor
events. The TUI displays `TaskSummary.live_status`, so classifier mistakes are
operator-facing.

Recent fixes addressed stale pane history causing incorrect statuses, such as
old "running" text making a finished task look like tests were still running.

## Findings

- Whole-pane substring matching is fragile. Old lines can override the current
  pane state when they contain words like `running`, `approval`, `error:`, or
  `login`.
- Source-order priority is doing too much hidden work. The current classifier
  effectively treats the order of `if contains_any(...)` checks as policy.
- Completion and final prompt evidence should be treated as current-state
  evidence, not just another substring match.
- Some phrases are too broad:
  - `running ` can match ordinary narration.
  - `done` can match non-final prose.
  - `login` can match task titles like "fix login".
- Status persistence and text classification should remain separate:
  - The classifier should turn current pane evidence into one observation.
  - The reducer/state machine should combine prior status with the new
    observation.

## Recommended Architecture

Keep the classifier stateless, but make it evidence-driven:

```text
PaneCapture
  -> recent meaningful lines
  -> PaneEvidence flags
  -> priority-based classification
```

Introduce explicit evidence names, for example:

```rust
enum PaneEvidence {
    FinalPrompt,
    Completion,
    ApprovalPrompt,
    InputPrompt,
    AuthRequired,
    RateLimited,
    ContextLimit,
    MergeConflict,
    Failure,
    CommandRunning,
    TestsRunning,
    AgentRunning,
}
```

Then classify by explicit priority instead of scattered substring order:

```text
Completion > FinalPrompt > current attention/blockers > current failures > running > unknown
```

Keep persistence rules in the reducer/state machine:

```text
Missing substrate > Failure/Blocked > Waiting > Running > Done/ShellIdle/Unknown
```

## Follow-Up Tasks

- Extract pane evidence collection from `classify_pane`.
- Classify from recent meaningful lines before considering full-pane fallback
  evidence.
- Add tests for negative phrasing such as "not done yet".
- Add tests for stale blocker/failure history followed by a current prompt.
- Add tests for current failures after prior success to ensure recent failure
  evidence can still win.
- Consider storing a short explanation of which evidence produced the live
  status for easier debugging.
