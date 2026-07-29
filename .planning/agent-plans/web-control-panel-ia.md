# Ajax Web — control-panel information architecture

Follow-on to `dashboard-one-tap-rebuild.md` (landed as `6b3c0e5`), which rebuilt
the **task rows**. This round rebuilds what surrounds them: the dashboard's
information architecture, the repository surface, system status, and navigation.

Driving instruction (Matt, 2026-07-29): rebuild Ajax Web as a mobile-first
control panel; home screen is a live operational dashboard, not a terminal
wrapper. Six categories: needs attention / running now / ready for action /
recent tasks / repositories and worktrees / system status.

## Scope

- New dashboard IA: the six categories above, in that order.
- New `features/repositories/RepoPanel.tsx` — repositories rendered from the
  real `RepoSummary` projection (path, active/attention/reviewable/cleanable
  counts), not as filter pills only.
- New `features/dashboard/SystemPanel.tsx` — backend authority, control-enabled,
  backend warning, connection state, fleet size.
- Repo route (`#/p/<repo>`) gains a repo header instead of being a silent filter.
- Navigation: Settings promoted from a header link to a bottom-nav destination.
- Row sub-line always says what the status is when the server sends no distinct
  explanation (previously such rows said nothing).

## Non-goals

- Task pages, `TaskTerminal`, terminal transport, viewport/keyboard band,
  polling, connection recovery, `api.ts`/`contracts.ts`: untouched.
- No new backend endpoint, no new `OperatorAction`, no change to
  `supported_web_action`.
- No browser-derived task truth. Bands stay `card.attention`, ordering stays
  Rust's `sortCards` seed, repo counts stay `RepoSummary`.

## Backend-blocked (reported, not faked)

The request asks for controls Ajax's browser contract cannot express today.
None are stubbed; each needs Rust work first:

| Asked for | Blocker |
| --- | --- |
| Open / merge PR, view failing checks, open GitHub | No PR entity in any browser DTO. `ajax-core/src/adapters/github.rs` exists but is not projected through `slices/cockpit.rs`. |
| Create worktree, clean up completed worktree | No worktree operation in `OperatorAction`; `RepoSummary.cleanable_tasks` is the only worktree signal that reaches the browser. |
| Reconnect session, stop / restart agent | `OperatorAction` has no interrupt/stop/restart variant. |
| Start a task on an existing record | `slices/operate.rs:108` returns `UnsupportedCapability` for `OperatorAction::Start`. |
| Blocked / Completed as distinct statuses | `TaskStatus` is 5-valued (`running`/`waiting`/`idle`/`error`/`unknown`). The nuance arrives as `status_explanation` text only. |

`Stale` and `Disconnected` **are** delivered: stale as the existing quiet-row
derivation, disconnected as connection state in the system panel.

## Delegation decision

`Delegation decision: not delegated because` this session's harness forbids
spawning subagents unless the user asks ("Do not call the AgentTool unless the
user requested it"), and AGENTS.md puts design-direction and IA work on the
do-not-delegate side. Implemented, reviewed and validated directly.

## Tasks

- [x] T1 — Re-read the tree after `6b3c0e5` landed mid-session; inventory the
      retained task/terminal surface and the real backend contract.
- [x] T2 — Widen `RepoSummary` in `types.ts` to the Rust shape (optional fields:
      a server that omits them must not crash the panel).
- [x] T3 — `features/repositories/RepoPanel.tsx` + tests.
- [x] T4 — `features/dashboard/SystemPanel.tsx` + tests.
- [x] T5 — Re-order Dashboard bands to the requested hierarchy, relabel, mount
      the two new panels, add the repo header on `#/p/<repo>`.
- [x] T6 — `App.tsx`: feed `connection` through, Settings into the bottom nav.
- [x] T7 — CSS for the new sections in the dashboard block.
- [x] T8 — Validate: vitest, eslint, tsc, ast-grep, `web:smoke` on a fresh vite.
- [x] T9 — Playwright mobile-webkit screenshot review.

## Deviations

- **Band order now follows the requested hierarchy** (needs → running → ready →
  recent), which inverts the shipped order (needs → ready → active → idle) from
  `dashboard-one-tap-rebuild.md`. That earlier order encoded "review is more
  actionable than running". Explicit instruction wins; flagged for Matt.
- Band labels changed to the requested vocabulary: "Needs attention" /
  "Running now" / "Ready for action" / "Recent". `Dashboard.test.tsx`'s label
  assertions were updated to match — assertions retained, strings changed.
- Repositories render as a section, and the project pills are kept: the pills
  are the one-tap scope filter, the section is the entity view. Removing the
  pills would cost a tap on the most frequent gesture.
- System status is a dashboard section rather than its own route, to hold
  navigation depth at one level.

## Validation

| Command | Result |
| --- | --- |
| `npx vitest run --config .../vite.config.mts` | pass — 503 tests |
| `npm run web:check` (tsc `--noEmit`) | pass |
| `npm run web:lint` (eslint src + e2e) | pass |
| `npm run web:sg` (ast-grep scan) | pass |
| `npm run web:build:check` | pass — dist rebuilt, deterministic shell |
| `cargo nextest run -p ajax-web` | pass — 191 tests |
| `pkill -f vite; CI=1 npm run web:smoke` | pass — 105 passed, 3 skipped, **1 flaky** |

The flaky cell is real and pre-existing: two different `e2e/terminal-behavior.test.ts`
cases flaked across two runs (`interaction wrap hides scrollbar chrome`, then
`keyboard close then reopen still pins inline task-detail flush to the band`),
each green on retry. No terminal code was touched in this change.

Not run: the full `npm run verify` gate (`cargo fmt/check/clippy/nextest
--all-features` across the workspace). No Rust source changed, but that gate is
required before opening a PR.

## Screenshot review findings (mobile-webkit)

1. **Bottom nav overflowed.** `.bottom-nav` was `grid-template-columns:
   repeat(2, 1fr)`; the third destination wrapped to a second row, growing the
   bar 41px taller than the scroll band accounts for and covering the end of the
   page. Fixed with `grid-auto-flow: column`. New e2e
   `the last dashboard control clears the bottom nav when scrolled to the end`
   measures reachability; verified it fails on the old CSS (`overlap: 41`).
   A first attempt at this test measured buttons against the *nav's own* box and
   passed on the broken layout — a wrapped grid grows its container, so that
   framing could never fail. Kept the viewport/occlusion framing instead.
2. **Repo paths rendered with reversed slashes.** The left-truncation used
   `direction: rtl`, which reorders the neutral `/` characters
   (`Users/…/ajax-api/`). Fixed with an inner `<bdi>` isolate.
