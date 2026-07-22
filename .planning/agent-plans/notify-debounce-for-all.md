# Stamp-agnostic notify debounce (for all)

Mode: Small Fix.
Delegation decision: not delegated because refinement of in-progress uncommitted
notify work — smaller than a new work order (one confirm helper + tests).

## Scope

`NOTIFY_CONFIRMATION_DWELL` is one shared clock for any actionable attention.
Waiting→Error mid-dwell does **not** restart the 15s window. Stamp key removed.

Keep allowlist (input/approval Waiting + all Error). Response ready still silent.

## Checklist

- [x] Drop `NOTIFY_CANDIDATE_STAMP_KEY`; confirm only on `since`
- [x] Update class-change test → shared dwell across Waiting→Error
- [x] Fix wall-clock test seeds (stamp key removed)
- [x] architecture.md shared-dwell wording
- [x] Validate attention + notify e2e

## Validation

```bash
cargo nextest run -p ajax-core attention  # 65 passed
cargo nextest run -p ajax-cli notify web_refresh_cockpit_*  # 7 passed
cargo fmt --check && cargo clippy -p ajax-core -p ajax-cli ...  # clean
```
