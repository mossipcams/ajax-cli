# Why CLI/TUI notify fires too much (deep diagnosis)

## Decision

Do **not** suppress webhooks while watching. Root cause is **attention-reason
churn**: distinct `status|explanation` stamps re-fire immediately, and CLI/TUI
Full refresh (~1s) + loose pane needles produce those flips.

## Proof (characterization tests — green = documents today's behavior)

```bash
cargo nextest run -p ajax-core \
  distinct_attention_reasons_refire_immediately_without_quiet_window \
  waiting_to_error_applies_immediately_when_not_running \
  broad_stuck_needles_currently_match_casual_agent_prose
# 3 passed
```

| Test | What it proves |
| --- | --- |
| `distinct_attention_reasons_refire_immediately_without_quiet_window` | Waiting→Blocked→Waiting in **3 seconds** = **3 phone pings**. No 30s quiet needed; stamp is `status\|explanation`. |
| `waiting_to_error_applies_immediately_when_not_running` | 4s dwell only gates **Running→attention**. Once already Waiting, Error applies on first sample. |
| `broad_stuck_needles_currently_match_casual_agent_prose` | `"blocked by the lockfile"` → `Blocked`; `"authenticate the webhook"` → `AuthRequired`. |

Existing tests even **encode** Waiting→Error re-fire as desired:
`attention::waiting_to_error_fires`, `error_within_episode_still_fires`.

## Pipeline (CLI/TUI path)

```
TUI ~1s tick
  → refresh_live_context (RefreshTier::Full)
  → capture-pane every probed task
  → project_pane_activity OR project_pane_stuck_status
  → apply_observation_at (dwell only if shows_running_evidence)
  → notify_attention_transitions
       fires iff episode_stamp changed
```

Web path samples less often (Live tier + 30s tick when no browser), so the
**same matcher bugs** look like a “CLI/TUI” problem.

## Dwell gaps (the real amplifier)

`WAITING_CONFIRMATION_DWELL` (4s) applies only when:

- incoming is Waiting/Error **and** `shows_running_evidence`, or
- incoming is Running **and** `shows_waiting_evidence`

`shows_waiting_evidence` is narrow: only `WaitingForInput` /
`WaitingForApproval` / `NeedsInput` / agent Waiting — **not** AuthRequired,
RateLimited, ContextLimit, or any Error.

So these are **ungated**:

| From | To | Dwell? | Notify? |
| --- | --- | --- | --- |
| Running | Waiting/Error | 4s | after confirm |
| Waiting(input) | Error(blocked) | **none** | **immediate new stamp** |
| Error(blocked) | Waiting(input) | **none** | **immediate new stamp** |
| Waiting(input) | Waiting(auth) | **none** | **immediate new stamp** |
| Idle | Waiting | **none** | **immediate** |

## Matcher fuel (stuck needles)

`pane_evidence` single-token / short needles that match agent prose:

| Needle | Status | Example false hit |
| --- | --- | --- |
| `"blocked"` | Error / Agent blocked | "path is blocked by the lockfile" |
| `"authenticate"` | Waiting / Auth required | "authenticate the webhook signature" |
| `"y/n"` | Waiting / approval | often real, but also in docs/examples in scrollback |
| `"exit code"` / `"failed with"` | Command failed (activity path) | prior command output in tail |

Stuck path (`project_pane_stuck_status`) only runs when activity projection
returns `None` (no busy chrome in last `BUSY_WINDOW=8` lines). When the agent
is mid-turn without busy markers, scrollback needles win — then a busy line
returns → Running (dwell) → needle again → flip.

## Why “CLI/TUI” specifically

1. **1 Hz Full refresh** re-captures panes every second → maximum flap samples.
2. Interactive cockpit **starts web companion** by default → second notify
   writer (tick) racing the same stamps (usually deduped; still first-writer
   wins each new stamp).
3. Web alone is quieter (slower poll), not immune.

## Ranked fix options (do not suppress watching)

1. **Tighten stuck needles** (same as Rate limited fix)
   - Drop bare `"blocked"` / `"authenticate"`; require stronger phrases
     (`"manual intervention required"`, `"auth required"`, `"please login"`).
2. **Dwell attention→attention flips**
   - Gate Waiting↔Error and Waiting(reason A)↔Waiting(reason B) with the same
     4s candidate window when evidence is untrusted pane.
3. **Coarser notify stamp**
   - Key episode on status class only (`Waiting` / `Error`), not explanation —
     reason changes would not re-ping. Tradeoff: miss “now it’s a merge
     conflict not just waiting”.
4. **CLI interactive use Live tier** (orthogonal; fewer probes, not a root fix)

Recommended first cut: **(1) + characterization tests flipped to desired
`None`**, then **(2)** if spam remains.

## Status

Deep diagnosis complete. No suppress. Characterization tests kept as evidence.
Await sample ntfy bodies or approval to implement matcher/dwell fixes.
