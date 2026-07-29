# Web Cockpit: the dashboard as a control panel

The dashboard must answer `PRODUCT.md`'s four questions — what is active, what
needs input, what is ready to review, which action is safe next — **and let the
operator act on the answer without leaving the screen**. Today it is a list you
read and then navigate away from.

## Scope

- `ajax-core`: carry the operator attention band on `TaskCard`.
- `ajax-web` (Rust): project `attention` on `BrowserTaskCard`; order
  `browser_actions` primary-first; close the `Start` parity gap.
- `ajax-web` (browser): group the list by `attention`; put each row's safe next
  control inline and executable; route answer-class rows to the terminal.
- `architecture.md`: update the `slices::cockpit` card contract paragraph.

## Non-goals

- No steering console, interrupt, or send-keys UI on the dashboard. The terminal
  stays the sole task interaction surface; the dashboard routes to it.
- No change to `TaskStatus`, lifecycle semantics, notify/ping policy, ordering
  (`sortCards` stays Rust's), or task detail.
- No new endpoint, no new dependency, no browser-owned task truth.
- No fleet gauge, muster bar, card shell, or second element restating counts
  (all previously rejected — see `dashboard_one_summary_dense_list` memory).

## Findings that drive this

1. `OperatorStatus.actionable` (`ui_state.rs:33`) already separates real operator
   gates from soft waits. `task_card()` drops it, so the browser sees only a
   5-value `TaskStatus` and flattens "waiting for YOUR approval", "ready for
   review", "rate limited" and "auth required" into one `Waiting` pile.
2. `LifecycleStatus::Reviewable | Mergeable` collapses to soft-`Waiting`
   ("Ready for review", `ui_state.rs:188`). Once opened,
   `workflow_boundary_is_acknowledged` drops it through to `Idle` — a task ready
   to ship sinks into the collapsed Idle tail.
3. `browser_actions` (`slices/actions.rs:55`) emits remediations then
   `available_actions` in eligibility order, never honouring
   `card.primary_action`; `visibleTaskActions` then strips `resume`. For a
   Waiting task (`available_actions = [Resume, Drop]`) the single revealed swipe
   action is **Drop** — a destructive action offered as the next step on a task
   that needs input. Contradicts `architecture.md:791-793`.
4. Every control is hidden behind a swipe. Nothing on screen names what to do
   until the operator drags a row — the opposite of a control panel.
5. `supported_web_action` (`slices/actions.rs:105`) omits `Start`. A task you
   could start cannot be started from the browser.
6. `cockpit.inbox` — severity-ranked, each item carrying `action` + `reason` — is
   fetched every poll and rendered nowhere.
7. The header reads `"3 running"` / `"7 tasks"` — the metric strip `PRODUCT.md`
   names as an anti-reference.

## Architectural decision

`architecture.md:790` bars the browser from deriving headline status from
`lifecycle`; `:791` bars a browser-side `primary_action` contract. Grouping the
list *is* headline derivation, so the band is core-derived and projected, never
computed in TypeScript. One field joins the card status contract:

```rust
// ajax-core::ui_state
#[serde(rename_all = "kebab-case")]
pub enum AttentionBand { NeedsYou, Review, Active, Idle }
```

| Task state | Band |
| --- | --- |
| `Error` | `needs-you` |
| `Waiting` + `actionable` (input/approval gate) | `needs-you` |
| lifecycle `Reviewable \| Mergeable` | `review` |
| `Running` | `active` |
| `Waiting` + `!actionable` (rate limited, auth, context, response ready) | `active` |
| `Idle` / `Unknown` | `idle` |

Precedence mirrors `derive_task_status`: **actionable is checked before the
review boundary**, or a card reading "Waiting for approval" files under Ready to
review. The band is read from lifecycle directly, so an acknowledged reviewable
task stays in `review` instead of sinking to `idle`.

Soft waits sit in `active` to keep `needs-you` trustworthy — every row there is
something the operator can act on now; the row still reads "Rate limited". One
match arm to move later.

## Control model

Each row carries **one visible, executable control** — the safe next action,
never destructive:

| Band | Control | Dispatch |
| --- | --- | --- |
| `needs-you`, agent gate | `Answer` | open task → terminal (the only surface that takes a reply) |
| `needs-you`, fault | remediation label (`Fix CI`) or `Repair` | `POST /api/operations` inline |
| `review` | `Ship` | `POST /api/operations` inline |
| `active` | none — state line only | — |
| `idle` | `Start` / `Resume` | `POST /api/operations` inline |

`Drop` stays off the dashboard entirely (task detail only). The swipe gesture is
**kept** — Matt restored it deliberately in #685 — but it now reveals the
*remaining* actions, never a duplicate of the inline control.

## Tasks

- [x] **T1 — core band.** DONE. `AttentionBand` + `attention_band()` in
      `ui_state.rs:16-51`, seven precedence tests at `ui_state.rs:1070-1186`,
      `attention` on `TaskCard` populated in `commands/projection.rs:104`.
- [x] **T2 — web DTO.** DONE (reorder withdrawn, see Deviations).
      `BrowserTaskCard.attention` at `slices/cockpit.rs:37`, set at `:78`,
      asserted by `browser_cockpit_json_carries_attention_band`
      (`slices/cockpit.rs:686`).
- [x] **T3 — grouped panel.** DONE. `TaskList.tsx:206-241` groups on
      `card.attention` only; four groups render Needs you / Ready to review /
      Active / Idle. `state.ts` untouched — `statusRank`/`STATUS_ORDER` still
      back `sortCards`, so nothing was deleted.
- [x] **T4 — inline controls.** DONE. `TaskList.tsx:54-56` filters destructive
      actions out entirely; `inlineAction` renders through the existing
      `ActionBar`, `revealActions` keeps the swipe fed. Rows with no
      non-destructive action fall back to an `Answer`/`Open` navigation control
      (`TaskList.tsx:134-140`). `Drop` no longer appears on the dashboard at all.
- [x] **T5 — header line.** DONE. `App.tsx:61-68` reports
      `n need you` → `n ready to review` → `All clear`.
      **Deviation:** derived from `attention` counts rather than
      `inbox.items[0]`. The inbox `reason` is a snake_case evidence label
      (`waiting_for_approval`) that would need a browser-side label map — task
      truth the browser must not own. Counts answer the same question with no new
      mapping. `cockpit.inbox` therefore remains unrendered.
- [x] **T6 — architecture.md.** DONE. `slices::cockpit` paragraph now records the
      `attention` band, its derivation source, its precedence, and the rule that
      the browser must never re-derive it. Action ordering and `Start` were
      withdrawn (see Deviations), so neither is documented.

## Delegation decision

`Delegation decision: delegated via model-router` — rounds: T1+T2 (Rust), then
T3–T5 (browser), then T6. Diffs reviewed and validation run locally per round.

## Deviations

- **T2 drops the `Start` item (2026-07-28).** `available_operator_actions`
  (`recommended.rs:185`) only ever yields `Repair`, `Resume`, `Ship`, `Drop`.
  `OperatorAction::Start` is a TUI cockpit-menu affordance
  (`ajax-tui/src/cockpit_state.rs:93`), never a task-card action, and web already
  starts tasks through `NewTaskSheet`. Adding it to `supported_web_action` would
  be dead code. Finding 5 above is withdrawn.
- **T2's primary-first action reorder is WITHDRAWN (round 1 gate).** The premise
  was wrong. `operator_action` (`recommended.rs:45-50`) overrides
  `action = OperatorAction::Resume` unconditionally whenever Resume is available,
  so `primary_action` is `Resume` for nearly every live task — and the browser
  strips `resume` in `visibleTaskActions`. Ordering by it is a no-op at best; at
  worst it demotes `Repair`, breaking `slices/cockpit.rs:414,432` which assert
  `actions[0] == "repair"` for checkout-mismatch and missing-worktree. Those
  assertions encode correct operator behavior and were not touched. The existing
  Rust order (remediations, then `Repair`/`Ship`, then `Drop`) is already right;
  the "never offer Drop as the next step" fix belongs in T4, in the browser.
- Round 1 delegate also edited `web/src/fixtures/{cockpit,operation}.json`
  (one line each) — a scope violation against an over-broad Forbidden list, since
  a new DTO field requires the mirrored fixtures. Accepted; no browser behavior
  changed.
- Evidence pass confirmed adding `attention` breaks no contract test:
  `assertCockpit` (`contracts.ts:82`) ignores unknown fields, and the Rust
  cockpit tests assert JSON substrings, not exact shape. Construction sites to
  fix up: `commands/projection.rs:92` (production), plus test fixtures at
  `ajax-cli/src/cockpit_backend.rs:1308`, `ajax-tui/src/lib/tests.rs:52`,
  `ajax-web/src/slices/actions.rs:157,183`,
  `ajax-web/src/slices/cockpit.rs:488,536,565,611`.

## Validation

| Command | Status |
| --- | --- |
| `cargo nextest run -p ajax-core -p ajax-web -p ajax-cli -p ajax-tui` | PASS — 1572/1572 |
| `cargo clippy --all-targets --all-features -- -D warnings` | PASS — exit 0 |
| `cargo fmt --check` | PASS — exit 0 |
| `npm run web:test -- --run` | PASS — 52 files, 493/493 |
| `npm run web:check` | PASS — exit 0 |
| `npm run web:lint` | PASS — exit 0 |
| `npm run web:smoke` (mobile-webkit) | PASS — 87 passed, 3 skipped |
| `npm run verify` | PASS — exit 0 |

Nothing was skipped. No commit was made — the tree is left dirty for review.

## Open

- Swipe now reveals only non-inline actions. If a row's only action is the inline
  one, there is no reveal. Confirm at review — do not delete `swipeReveal.ts`.
