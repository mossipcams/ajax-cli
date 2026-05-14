# Ajax TUI UX Rework Plan

Status note: this plan predates the information-model redesign. Current Cockpit
inbox rows are annotation digests keyed by task id and operator action rather
than `AttentionItem` rows with recommended actions.

## Goal

Make the cockpit readable and useful on a narrow mobile/SSH terminal (iOS
Terminus over SSH is the reference target). Two intertwined problems:

1. The current 3-zone layout (header / feed / status bar) overloads the header
   on the Projects view, hides keybinding hints whenever a notice is showing,
   and gives the operator no persistent "what should I do next" anchor.
2. Notifications use a single global slot that silently overwrites itself,
   making bursty errors invisible and giving every message the same visual
   weight regardless of importance.

The target experience: the operator always sees the next thing that needs
attention, never loses keybinding hints, and at most one notice per task is
visible at a time — the most important one.

## Non-Goals

- No new dependencies. Stay on the existing `ratatui` + `crossterm` stack.
- No hotkey/chord shortcuts. Mobile-first means noun-first select-then-Enter.
- Help view becomes a card layout — out of scope for this plan, tracked
  separately.
- Do not change command surface, action dispatch, or `ActionOutcome` variants.
- Do not modify files under `tests/`.

## Design Decisions

### Layout: 5-zone stack

Replace the current 3-zone stack (rendering.rs:41) with five zones, each
owning one job. Zones marked *(conditional)* render only when relevant and
collapse to zero height otherwise.

```
breadcrumb                                 row 0      always
attention line                             row 1      conditional (inbox non-empty)
counts strip                               row 2      conditional (Projects view only)
feed                                       flex       always
notice                                     row n-2    conditional (active notice)
hints                                      row n-1    always
```

- **Breadcrumb** carries view location only ("Ajax > web > web/fix-login").
  Drop the right-aligned `[AJAX]` brand (lib.rs:357-388) — redundant with the
  leading "Ajax" and burns ~8 columns.
- **Attention line** shows the single highest-priority inbox item:
  `→ web/fix-login: respond to question`. Danger-styled. Auto-hides when the
  inbox is empty. This is the cockpit's reason to exist; it deserves a
  permanent address.
- **Counts strip** is the existing chip stream (repos / tasks / inbox / review
  / clean), pulled out of the header and shown only on the Projects view.
  Subviews don't need it.
- **Notice row** carries the active per-task or system notice. Severity is
  conveyed by glyph + color (no longer by position, since there's only one
  slot). Hints stay visible underneath.
- **Hints** become unconditional. Status bar is hints only.

Additional feed changes:

- **Named section headers** replace blank-row group separators (lib.rs:716-720).
  `— inbox —`, `— tasks —`, `— actions —` in dim text. Same row cost,
  communicates the boundary.
- **Pinned task summary** at the top of the feed in `AppView::TaskActions` so
  the user keeps the task's handle/status/title in view while picking an
  action. Non-selectable.
- **Selection prefix glyph** (`▸` in primary accent) on the selected row in
  addition to the existing background highlight (lib.rs:443-447), so the
  cursor survives on terminals with weak 256-color rendering.

### Notification hierarchy: most-important-wins per task

Replace the single `flash: Option<(String, u8)>` (cockpit_state.rs:163) with a
per-task notice map plus a small global slot for system messages.

```rust
pub(crate) notices: HashMap<TaskId, TaskNotice>,
pub(crate) system_notice: Option<SystemNotice>,

struct TaskNotice {
    msg: String,
    severity: Severity,
    origin: Origin,
    ticks_remaining: u8,
}

enum Severity { Confirm, Error, Success, Hint }
enum Origin { UserAction, BackgroundEvent }
```

**Per-task rule — most important wins, always:**

- At most one notice per task. New notice with severity >= existing → replace.
- New notice with severity < existing → drop silently.
- Within the same severity, UserAction outranks BackgroundEvent. Background
  polls cannot overwrite a user-initiated notice on the same task.
- Identical `(msg, severity)` resets the timer instead of replacing — no
  repaint, no flicker.

**Severity ladder:**

| Severity | Trigger                                          | Lifetime                   | Style         |
|----------|--------------------------------------------------|----------------------------|---------------|
| Confirm  | `ActionOutcome::Confirm` (destructive op)        | Sticky until resolved      | bold accent `›` |
| Error    | Action error, refresh failure                    | 5s, or until next action   | red `!`         |
| Success  | `ActionOutcome::Message`                         | 2s (current FLASH_TICKS)   | green ·         |
| Hint     | Form validation ("task name required")           | 1s; cleared on next key    | dim text        |

**Display rule — only one notice visible at a time:**

The notice row renders the single highest-priority notice across the whole
cockpit, selected by:

1. Any Confirm (only one possible at a time via existing `pending_confirmation`).
2. Otherwise, the highest-severity notice belonging to the currently
   selected task.
3. Otherwise, the `system_notice` (refresh errors, `initial_flash`, form
   validation that has no task).
4. Otherwise, nothing — row collapses.

This makes the active selection the focus: as the operator moves the cursor,
the notice row reflects the state of whatever they're pointing at. Other
tasks' notices still exist in the map (and can be surfaced inline on their
rows in a follow-up), but only one shows in the notice row at any time.

**Realtime / refresh handling:**

- On `apply_refresh` success: clear all `Origin::BackgroundEvent` Errors;
  prune notices whose task id no longer exists in `tasks`.
- On a task's `lifecycle_status` change during refresh: drop its Success and
  Hint (stale by definition). Keep Confirm and Error.
- On task disappearance: drop the task's notice and any pending Confirm
  bound to it.
- On view change (e.g. user navigates away from `TaskActions`): drop
  the pending Confirm and replace it with a Hint "confirm again — context
  changed."

## Phases

Five small commits. Each phase is independently mergeable; later phases
assume earlier ones.

### Phase 1 — Drop the brand, extract counts strip

- Remove `[AJAX]` rendering and `show_brand` (lib.rs:357-388).
- Pull the counts chip stream out of `render_header`'s `AppView::Projects`
  arm (lib.rs:262-321) into a new `render_counts_strip` rendered on row 2
  only when `matches!(app.view, AppView::Projects)`.
- Adjust `render_ui` (rendering.rs:41) to a conditional 4-zone layout that
  inserts the counts row only when shown.

### Phase 2 — Persistent attention line

- New `render_attention_line` rendered between breadcrumb and feed when
  `!app.inbox.items.is_empty()`. Reads the highest-priority item:
  `→ {task_handle}: {reason}` (or recommended action when reason is plain
  status like "WaitingForInput"). Danger-colored.
- Suppress on subviews where it would conflict (`TaskActions`, `NewTaskInput`,
  `Help`).

### Phase 3 — Notice row + severity (replaces flash)

- Replace `flash: Option<(String, u8)>` with `notices: HashMap<TaskId,
  TaskNotice>` and `system_notice: Option<SystemNotice>`.
- New `notify` API replacing `flash()`. Call sites in
  `handle_action_result` (input.rs:115-142) pass the originating
  `AttentionItem.task_id`; refresh-error and submit-input paths use
  `system_notice`.
- New `render_notice_row` selects the visible notice using the display rule
  above. Hints row (lib.rs:390) reverts to always rendering hints.
- Update `tick_flash` → `tick_notices` to decrement every entry and prune.

### Phase 4 — Realtime correctness for notices

- In `reload` (cockpit_state.rs:365): prune notices for vanished tasks;
  clear `BackgroundEvent` Errors after successful refresh; drop Success/Hint
  for tasks whose `lifecycle_status` changed.
- In `pending_confirmation` flow: on view change or task-identity change,
  drop the confirm and post a Hint via `system_notice`.
- Add tests covering: same-severity replace, lower-severity drop,
  UserAction vs BackgroundEvent tiebreaker, stale-task pruning, confirm
  invalidation.

### Phase 5 — Feed polish

- Replace blank-row group separators with named dim section headers
  (lib.rs:716-720).
- Pin a frozen task summary row at the top of the feed in
  `AppView::TaskActions` so context survives the navigation into the
  actions menu.
- Add a `▸` selection prefix on the selected row in `render_selectable`
  output. Bg highlight stays.

## Out of Scope (Follow-ups)

- Help view as a card layout instead of feed-row list (lib.rs:666-697).
- Inline per-row notice badges so non-selected tasks can show their notices
  passively. The map already supports it; this is a render-only change.
- Notice history / scrollback view.
