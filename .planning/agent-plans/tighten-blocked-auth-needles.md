# Tighten blocked/authenticate pane needles

## Scope

Stop casual agent prose from classifying as `Blocked` / `AuthRequired` by
removing bare substring needles `"blocked"` and `"authenticate"` from
`pane_evidence`. Keep stronger phrases that still catch real stuck panes.

Non-goals: attention↔attention dwell, notify stamp coarsening, CLI suppress,
Rate limited (already fixed).

## Delegation decision

`Delegation decision: delegated via model-router` → `pi-delegate` / MiniMax.
Parent Review Gate: ACCEPT after independent validation.

## Tasks

- [x] Flip characterization test to desired `None` for casual prose (RED)
- [x] Keep real stuck phrases still classifying (blocked+cannot continue, please login, auth required)
- [x] Remove `"blocked"` and `"authenticate"` from `pane_evidence` lists
- [x] Validate focused live nextest + fmt/clippy

## Validation

```bash
cargo nextest run -p ajax-core broad_stuck_needles_do_not_match_casual_agent_prose pane_stuck_states_survive try_again_later
# parent: passed

cargo fmt --check  # exit 0
```
