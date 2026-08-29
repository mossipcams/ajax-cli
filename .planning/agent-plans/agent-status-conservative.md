# Agent status: conservative observation architecture

**Status: complete.** Finished in Wave 3 of
`.planning/agent-plans/system-debt-run-state-and-session.md`.

The conservative observation/run model landed in `crates/ajax-core/src/agent_status.rs`
and runtime refresh. Ownership is now explicit:

- **`reduce_agent_status`** — observation → live (single agent reducer)
- **`apply_reduced_observation`** — writer (`live_application`, via `live::apply_*`)
- **`derive_operator_status`** — operator projector (`ui_state`)

See `docs/architecture/core-subsystems.md` (Live Status) and the system-debt
program plan for ongoing waves (model pin, session peel, etc.).

Historical checklist and validation logs from the original implementation are
preserved in git history of this file.
