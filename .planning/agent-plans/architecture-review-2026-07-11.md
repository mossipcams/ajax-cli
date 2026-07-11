# Architecture Review — 2026-07-11

Mode: Planning-Only (review/critique). No code edits.

## Question

Does Ajax follow "modular monolith with vertical slices" as architecture.md claims?

## Findings

1. **Modular monolith: yes, genuinely.** 5 crates, all dependency edges point
   inward to `ajax-core` (zero internal deps). `ajax-tui`/`ajax-web`/`ajax-supervisor`
   depend only on core; `ajax-cli` is the composition root. Ports (`Registry`,
   `CommandRunner`) + `adapters/` give core a clean hexagonal boundary.
2. **Vertical slices: true only in `ajax-web`.** 4 real slices (cockpit,
   operate, install, terminal) + shared `actions` vocabulary + adapters +
   runtime, enforced by `ajax-web/src/architecture.rs` tests.
3. **`ajax-core::slices` is vestigial.** Contains exactly one slice, `pane`
   (crates/ajax-core/src/slices.rs), which is (a) substrate-named — violating
   architecture.md's own naming rule — and (b) dead: no consumer anywhere in
   the workspace.
4. **The real core verticals live in `task_operations/`** (start,
   task_command = resume/review/repair/ship, drop_task, sweep_cleanup, kernel).
   Small files (~50–530 lines), typed outcomes. This IS the vertical-slice
   layer in all but name.
5. **architecture.md drift:**
   - Claims arch tests use `rust_arkitect` — dependency removed (#391 ponytail
     audit); tests are hand-rolled string matchers per crate.
   - Claims `ajax-core::slices` owns capability orchestration with slices
     named review/resume/ship/drop — the review slice was added May 2026
     (d7c4396) and deleted July 2026 (8a825bb). Never migrated further.
6. Shared domain kernel (models, lifecycle, live, policy, ui_state, runtime,
   registry) is layered by concern, not sliced — correct, since it's the
   single task-truth every surface consumes.
7. Size hotspots (non-test lines): sqlite.rs ~1700, live.rs ~1160,
   ajax-web runtime.rs ~1000, runtime_refresh.rs ~750. Watch, don't split yet.

## Recommendation

Adopt the de facto architecture and fix the story, not the code:
modular monolith + hexagonal core + vertical **operation** slices at the
use-case boundary (`task_operations`) + sliced presentation adapter (ajax-web).

Concrete follow-ups (each a Small Fix / doc change):
- [ ] Delete dead `ajax-core::slices::pane` and the `slices` module, or fold
      pane into where it belongs if a consumer is planned.
- [ ] Update architecture.md: drop rust_arkitect + Aroeira-migration claims;
      name `task_operations` as the vertical use-case layer.
- [ ] Optionally add a core architecture test asserting task_operations
      submodules stay isolated from each other (mirror of web slice test).

## Validation

None run (review only; no code changed).
