# Diagnose: Ajax web unreachable / crashing after recent PRs

## Scope

- Root-cause why stable/dev web feels unreachable or crashed after recent merges.
- Non-goals: implement fix in this pass (diagnosis only unless follow-up requested).

## Verdict

**Primary cause of unreachability today is intentional restart downtime from Test in Stable / `dev-web-restart.sh` after each merge**, not a process abort loop.

**Secondary soft-fail risk (alive but looks dead):** recent Diff Review + status-reconcile work runs longer sync `gh`/`git`/`tmux` work on Tokio worker threads; cockpit client aborts at 10s → UI "backend unreachable".

**Historical hard crash (nested Tokio runtime in `push.rs`) is already gone** (deleted in #134); still present only as old lines in `web-stable.log`.

## Evidence

### 1. Restart cadence tracks PR merges (2026-07-31 UTC)

`~/.local/state/ajax/logs/ajax.log` `listening` events:

| Listen time | Nearby merge |
| --- | --- |
| 18:38 | #710 CSP |
| 18:55 | #712 Diff Review |
| 19:39 | #713 Diff harden |
| 20:52 / 21:01 | #716 signal/noise |
| 21:28 | #718 Diff swipe |
| 21:52 | #719 Diff swipe direction |
| 22:08 | another Test-in-Stable |

Measured gaps from last `operate end` → next `listening`: **~35s–4+ min** while rebuild kills the listener.

Stable is healthy right now: `https://127.0.0.1:8787/api/health` → `{"ok":true}` (~2ms), pid 29053 idle.

### 2. Soft-wedge code paths from recent PRs

- **#712+ Diff Review** (`runtime.rs` `axum_task_pull_requests` / `axum_task_diff`):
  - Sync via `run_optimistic` on the async handler thread (no `spawn_blocking`).
  - UI load: `pull-requests` then `diff`; `task_diff_projection` also re-runs `observe_task_pull_requests` → up to **2× `gh pr list` + 1× `gh pr diff`/`git diff`**, each with **30s** `GH_PR_CHECKS_TIMEOUT`.
  - Client Diff timeout is 45s; cockpit polls stay at **10s** (`GET_REQUEST_TIMEOUT_MS`).

- **#711 / #714 status reconcile** (`runtime_refresh.rs`):
  - Live cockpit refresh now `capture-pane`s for ActivelyWorking Claude/Codex/Cursor (and some idle/unknown gates).
  - Each capture is sync `ProcessCommandRunner` with **8s** `TMUX_PROBE_TIMEOUT`, still on the async path while holding `control_lane` for the refresh that won the lock.

### 3. Client noise that feels like "crash"

- Phone (`192.168.1.92`) floods `TLS handshake eof` during reconnect windows.
- Other LAN clients: `CertificateUnknown` storms.
- Repeated `terminal child cleanup timed out after 2s` (non-fatal by design).

### 4. Not current: push nested-runtime panic

`web-stable.log` still shows:

```text
thread 'tokio-rt-worker' panicked at crates/ajax-web/src/adapters/push.rs:211:37:
Cannot start a runtime from within a runtime...
```

`push.rs` was deleted in #134; current tree has no such file. Do not chase this as today's bug.

## Likely user-visible story

1. Merge web/status PR → Test in Stable kills `:8787` → phone shows unreachable for 1–4 minutes → server returns → TLS eof noise.
2. While Diff Review loads a large PR, or Live refresh captures many active panes, cockpit polls can exceed 10s → banner "backend unreachable" even though process is up.
3. Opening Diff right after a restart compounds (1)+(2).

## Recommended fixes (follow-up)

1. **Offload Diff Review + cockpit refresh command runs to `spawn_blocking`** (or a dedicated blocking pool), same pattern as terminal PTY teardown (#236).
2. **Skip duplicate `observe_task_pull_requests` on `/diff` when `/pull-requests` just ran** (or coalesce client to one round-trip).
3. **Test in Stable UX**: keep old binary serving until new binary health-checks (blue/green or brief drain), or surface "updating" in `/api/health` during restart.
4. Optional: raise cockpit timeout slightly **or** keep refresh under 10s by capping parallel capture-pane work.

## Checklist

- [x] Inspect stable/dev health, logs, process
- [x] Correlate listen events with today's merges
- [x] Trace Diff Review + cockpit refresh blocking paths
- [x] Rule out current push.rs nested-runtime panic
- [ ] Implement mitigation (await approval)

## Delegation decision

`Delegation decision: not delegated because diagnosis/planning-only`

## Live incident 2026-07-31 ~17:18 CDT

Caught while user reported "it just happened again":

- Process **still alive**: same pid `29053`, up since 17:08, no new `listening` line.
- Localhost `/api/health` and authenticated `/api/cockpit` both OK (cockpit ~90ms cold / ~2ms cached).
- Phone `192.168.1.92` actively flooding `TLS handshake eof` and many `terminal child cleanup timed out after 2s`.
- No stuck `gh`/`git` children; no panic in current process.

Interpretation for this incident: **not a server crash**. Phone-side reconnect / terminal websocket thrash made the PWA report unreachable while stable kept serving.

## Validation

- `curl -sk https://127.0.0.1:8787/api/health` → 200 `{"ok":true}`
- Log sources: `~/.local/state/ajax/logs/ajax.log`, `web-stable.log`, tmux `ajax-web-stable`
- Live recheck at 17:18 CDT confirmed soft client failure, not process death
