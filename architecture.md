# Ajax Architecture

Ajax is an operator cockpit for isolated AI coding tasks. Native Cockpit and
Web Cockpit are sibling operator surfaces over the same backend contracts. The
CLI, JSON contract, Rust core, TUI, and browser adapter provide deterministic
operator surfaces used by Cockpit, scripts, and tests.

The codebase is a **modular monolith** organized around **vertical slices**.
This root document records durable ownership, dependency rules, invariants, and
a navigation map. Detailed subsystem behavior lives under
[`docs/architecture/`](docs/architecture/).

## Documentation Sources

Root `architecture.md` is not a parking lot for implementation plans, TDD
packets, review notes, or one-off migration playbooks. Task-specific plans
belong in `.planning/` or outside the repo. When a plan discovers durable
architecture, move only the lasting decision here or into the nearest focused
architecture doc, then retire the plan artifact.

Authoritative references for agents, in order: explicit user instruction →
`AGENTS.md` → this file → focused docs linked below → source and tests.

## Crates

| Crate | Owns |
| --- | --- |
| `ajax-core` | Domain model, registry facade, lifecycle, command planning, policy, live-status reduction, projections, typed output contracts, operator slices |
| `ajax-cli` | Argument parsing, context load/save, command dispatch, human/JSON rendering, process execution wiring |
| `ajax-tui` | Native Cockpit UI over `ajax-core` JSON-backed responses |
| `ajax-web` | Browser Cockpit adapter: HTTP, shell assets, DTOs, local HTTPS identity, server wiring — presentation only, not a second task domain |
| `ajax-supervisor` | Supervised agent execution, process monitoring, repository observation, monitor events |

## External Substrates

Ajax coordinates external tools but does not replace them.

- Git owns repository truth, branches, merges, and worktrees.
- tmux owns durable interactive sessions.
- Agent CLIs are opaque workers.
- SQLite stores Ajax registry state as Ajax-owned task intent, events, and
  cached projections — durable Ajax facts and a fast read model, not authority
  for Git, tmux, or process reality.

Ajax owns task intent, lifecycle decisions, naming, policy, operation history,
live projection, command plans, and registry state.

## Dependency Layers

Dependency direction points **inward / downward**. Higher layers may depend on
lower layers; never the reverse. Sibling operator slices must not import each
other except the documented `sweep_cleanup` → `drop_task` exception.

```text
Composition / presentation   ajax-cli, ajax-tui, ajax-web
        ↓
Operator slices              ajax-core::task_operations::{start,resume,review,
                             repair,ship,drop_task,sweep_cleanup}
                             (+ shared slice plumbing: kernel)
                             (+ thin operator_dispatch multiplex — not a slice)
        ↓
Plan helpers                 ajax-core::commands/*  (not kernel)
        ↓
Shared kernel                models, lifecycle, live, policy, output,
                             registry traits, typed events, ghost_task,
                             validity, and other cross-slice task-truth
        ↓
Mechanisms                   adapters/*, registry/sqlite, ajax-supervisor
                             substrates
```

Hand-rolled architecture tests in each crate's `architecture.rs` enforce slice
isolation and layer direction. See Validation below for fast local commands.

### Shared-kernel admission

The shared kernel must not become a dumping ground.

A type or module belongs in the kernel only when **all** of the following hold:

1. It is task-truth or a stable port (not a convenience wrapper).
2. It is needed by ≥2 operator slices **or** ≥2 operator surfaces.
3. It is not operator-verb-specific, not substrate I/O/parsing, and not
   presentation.

Rules:

- Default new code into the owning operator slice (or plan helper / adapter).
- Promote into the kernel only when a second real consumer appears.
- Never create generic `utils` / `helpers` modules as a home for leftovers.
- New core items default to `pub(crate)` unless they are part of a consumed
  contract.

`commands/*` are substrate-oriented **plan helpers**, not kernel. Slices may
call them. Plan helpers must not import operator slices (the thin
`execute_plan` → `task_operations::kernel` wrapper is the documented exception).

`task_operations::kernel` is shared execution plumbing for slices, not an
operator verb and not part of the shared domain kernel above.

`task_operations::operator_dispatch` (if present) is composition glue that
multiplexes `resume` / `review` / `repair` / `ship` for CLI and Web call sites.
It is **not** a vertical slice and may call those four slices.

### Vertical slices

A slice is a vertical use-case module inside its owning crate — not a new crate
and not a cosmetic facade over unrelated layered code.

`ajax-core::task_operations` is the core slice layer. Each operator verb is a
file-backed or directory-backed submodule:

- `start`, `resume`, `review`, `repair`, `ship`, `drop_task`, `sweep_cleanup`

`start` may skip interactive agent send-keys when the caller requests a
provisioned Cursor launch; worktree and tmux are still created.

Slice names use operator language, not substrate language (Git diff, tmux)
attach, process cleanup). `ajax-web::slices` is the sibling slice layer for
browser capabilities.

**Slice contract:**

- `plan_*` — pure: fresh evidence in, command plan out, no registry mutation
- `execute_*` — owns external effects and step receipts
- Post-execution state decisions live in private reducers inside the slice

**Growth form:** a slice starts as one file. When a single-file capsule no
longer fits cohesive ownership, it becomes a directory with focused modules
such as `plan`, `execute`, `reduce`, and `validation` — still one slice, one
small public surface (`mod.rs` re-exports entry points only).

Slices must not import sibling slices, except `sweep_cleanup` composing
`drop_task` teardown because tidy sweeps what drop leaves behind.

### File size and split policy

- Target ~600 lines per handwritten source file; **hard max 1000 lines** on
  disk (including inline tests). Enforced for changed files by
  `scripts/check-file-loc.mjs` (`WARN_AT=600`, `FAIL_AT=1000`).
- Split by **cohesive responsibility**, never by arbitrary LOC chunks.
- Never invent generic util/helper modules to shed lines.
- Prefer peeling `#[cfg(test)] mod tests` into a sibling first, then split
  production code by ownership.
- Do not land new features into an already over-max file.

## Hard Invariants

- Core owns task truth. UI presents task truth. CLI dispatches commands.
  Supervisor observes and reports execution.
- Browser code must not become an alternate registry, policy engine, lifecycle
  owner, or task source of truth.
- Runtime state reconciles through core/backend contracts.
- Git, tmux, and supervised processes remain authoritative for their own
  reality; SQLite caches are staleable evidence.
- Operator status is exactly `Running`, `Waiting`, `Idle`, or `Error`, with one
  optional presentation-ready explanation; lifecycle and annotations stay
  separate typed inputs.
- Web Cockpit targets normal iOS Safari without requiring Home Screen install
  for core Cockpit use, without classic PWA packaging (manifest, icons, service
  worker), without a service-worker offline mutation model, and without
  replacing the raw xterm/tmux-first terminal model as the default path.
  Optional Home Screen install enables Declarative Web Push only.
- Optional orchestration chat uses Cursor ACP over stdio via an `ajax-web`
  host, not PTY paste. Transcripts persist as JSONL under ajax-web `state_dir`
  (`web-session/`), not registry or tmux.
- Do not add a public-internet product path unless the security model is
  explicitly changed.

Detailed task-authority, registry, live-status, and Web Cockpit rules live in
the focused docs below.

## Web Cockpit telemetry

Web Cockpit may send approved outbound product telemetry to PostHog Cloud using
the Ajax project write key by default (`phc_…` in `@/shared/lib/telemetry`).
`VITE_POSTHOG_KEY` overrides that key at build time; set it to `off` / `0` /
`disabled` to disable telemetry. Session replay stays off. Full init, storage
rules, and property schemas live in
[`docs/architecture/web-cockpit.md`](docs/architecture/web-cockpit.md) (PostHog
section).

Every **explicit** custom event (`track` / `captureEvent`) merges caller
properties with shared context (context wins on collision):

| Property | Type | Notes |
| --- | --- | --- |
| `event_id` | string | UUID per capture |
| `session_id` | string | Tab session (`sessionStorage`) |
| `install_id` | string | Stable install (`localStorage`) |
| `sequence` | number | Monotonic per install |
| `app_version` | string | Optional; from `meta[name="ajax-app-version"]` |
| `route` | string | Current hash route |
| `ios_version` | string | Optional; parsed from UA |
| `viewport_w` / `viewport_h` | number | Inner window size |
| `standalone` | boolean | Installed PWA vs browser tab (observational only) |

Custom event names (plus PostHog Web Vitals autocapture when initialized):

| Event | Purpose |
| --- | --- |
| `ajax_tap_to_feedback` | Tap → first visible feedback |
| `ajax_tap_to_operation_complete` | Tap → completed operation |
| `ajax_swipe` | Swipe gesture metrics |
| `ajax_route_visible` | Navigation → visible content |
| `ajax_pwa_launch` | Cold launch timing (once per boot) |
| `ajax_pwa_resume` | Resume from background |
| `ajax_telemetry_diagnostic` | Settings diagnostics snapshot |

## Agent Context Capsules

Optimize for minimizing the files an agent must inspect for a routine change:

1. Read this root file for layers, kernel admission, slice contract, and
   invariants.
2. Open only the focused architecture doc for the subsystem you touch.
3. Open only the owning operator slice (and its tests) plus the stable ports it
   already uses.
4. Run a slice-local verify command before full-workspace verify.

Do not load sibling slices, unrelated crates, or the entire old monolith doc
set for a single-verb change.

## Validation (fast / slice-local)

Prefer focused verification first:

```bash
npm run verify:arch          # architecture tests across crates
npm run verify:core          # check + nextest for ajax-core
npm run verify:slice -- repair   # example: one operator slice
npm run verify:slice -- operate  # example: web operate slice
```

Slice names for `verify:slice`: core verbs
`start|resume|review|repair|ship|drop_task|sweep_cleanup`; web
`operate|cockpit|terminal|install`; plus `cli` / `core` / `web` / `arch`.

Full gate before opening a PR: see
[`docs/agent/pull-requests.md`](docs/agent/pull-requests.md#local-verification-gate-before-a-pr).

## Navigation Map

| Topic | Doc |
| --- | --- |
| Task authority, checkout mismatch, agent events | [`docs/architecture/task-authority.md`](docs/architecture/task-authority.md) |
| Mutable ops, receipts, repair adoption, tidy | [`docs/architecture/task-operations.md`](docs/architecture/task-operations.md) |
| Registry, lifecycle, substrate evidence, live status | [`docs/architecture/core-subsystems.md`](docs/architecture/core-subsystems.md) |
| Command helpers and adapters | [`docs/architecture/commands-adapters.md`](docs/architecture/commands-adapters.md) |
| CLI composition and supervisor | [`docs/architecture/cli-supervisor.md`](docs/architecture/cli-supervisor.md) |
| Web Cockpit slices, runtime, terminal, speech | [`docs/architecture/web-cockpit.md`](docs/architecture/web-cockpit.md) |
| Native Cockpit views | [`docs/architecture/cockpit.md`](docs/architecture/cockpit.md) |
| Speech operator setup (not architecture ownership) | [`docs/speech-input.md`](docs/speech-input.md) |
