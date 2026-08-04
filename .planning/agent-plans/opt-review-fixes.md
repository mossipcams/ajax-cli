# Fix review findings on top-4 optimizations

## Scope

Resolve the two HIGH code-review defects:

1. **PR2 wiring:** `AgentStatusFiles` cache must survive across refreshes (web + CLI cockpit).
2. **PR3/PR3b CI freeze:** notify tick must keep running `RefreshTier::Full` while the browser is connected; only webhook delivery stays suppressed.

Also take the cheap MEDIUM poison recovery on the agent-status mutex while touching that file.

## Non-goals

- PR1 Task-clone micro-optimization
- Write-batching fold-in / commit
- Changing CI rate limits or Live browser poll tier

## Delegation decision

`Delegation decision: delegated via model-router` — two sequential packets.

## Design

### Fix A — shared AgentStatusFiles

- Add `AgentStatusFiles::shared_from_runtime_cache(cache_dir)` that returns `Arc<AgentStatusFiles>` reused for the same `cache_dir` in-process (replace entry when path changes).
- Wire `web_backend::refresh_runtime_context_for_web` and `cockpit_backend::refresh_live_context` to use it.
- Mutex: `lock().unwrap_or_else(PoisonError::into_inner)`.
- Add a test that two shared handles for the same dir share the stamp cache (second observations call does not re-read when file unchanged).

### Fix B — notify tick Full while connected

- Notify tick always refreshes; `deliver_notifications = !browser_connected()`.
- Decouple tier from deliver flag: notify path always `RefreshTier::Full`; browser `/api/cockpit` stays Live + deliver=false.
- Update `architecture.md` notify-tick wording if it still implies the whole tick is skipped.
- Tests: tick with browser connected still requests Full (and deliver=false); disconnected still Full + deliver=true.

## Checklist

### Fix A shared status cache
- [x] Packet + delegate (Cursor composer-2.5; GLM escalated)
- [x] Parent review + validation — `cargo nextest run -p ajax-cli agent_status` EXIT 0 (11 passed)
- [x] Accept

### Fix B notify Full while connected
- [x] Packet + delegate (Cursor composer-2.5)
- [x] Parent review + validation — `cargo nextest run -p ajax-web suite_2 suite_3` EXIT 0 (39 passed)
- [x] Accept

## Deviations

- Fix A: process-local single-slot `SHARED_AGENT_STATUS` instead of storing on `CliRuntimeBridge` (covers CLI cockpit loop too).
- Fix B: residual — if `[notify]` is unset, no background Full tick; browser polls alone stay Live.

## Results

Both HIGH review findings fixed. Uncommitted alongside prior top-4 + write-batching work.