# Notify debounce + allowlist to wait/ask statuses

Mode: Behavior Change.
Delegation decision: delegated via model-router (cursor-delegate; MiniMax usage-limited)

## Scope

1. Keep Response-ready exclusion (folded into allowlist).
2. **Allowlist** phone-pings for Waiting to only `"Waiting for input"` and
   `"Waiting for approval"` (hook wait/ask). All Error still notifies.
3. **Debounce** (`NOTIFY_CONFIRMATION_DWELL` = 15s): first actionable sighting
   stamps `notify_candidate_since` + stamp; fire only after sustained attention.
   Class change restarts the dwell clock.

## Non-goals

- Changing hook translation or UI status vocabulary
- Config knob for dwell (constant + ponytail)
- Suppressing Error-class notifies

## Checklist

- [x] Response ready does not notify (allowlist)
- [x] Allowlist Waiting explanations
- [x] 15s notify confirmation dwell + candidate metadata
- [x] Stamp-keyed candidate so Waiting→Error restarts dwell
- [x] Update attention / notify / web e2e / runtime_refresh callers
- [x] architecture.md
- [x] Parent validation

## Validation

```bash
cargo nextest run -p ajax-core attention  # 65 passed
cargo nextest run -p ajax-cli notify web_refresh_cockpit_lifecycle_wait web_refresh_cockpit_notify  # 7 passed
cargo nextest run -p ajax-core runtime_refresh -- github_failed_check  # passed
cargo fmt --check  # clean
cargo clippy -p ajax-core -p ajax-cli --all-targets --all-features -- -D warnings  # clean
```

## Deviations

- MiniMax usage-limited → cursor-delegate composer-2.5 (report envelope missing;
  parent reviewed diff).
- Parent fix: stamp-keyed candidate + clear on non-actionable; seed
  `NOTIFY_CANDIDATE_STAMP_KEY` in wall-clock tests; deliver-flag test seeds
  Error stamp (Full refresh projects missing substrate as Error).
