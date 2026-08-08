# Orchestration session chat — UX redesign

Status: in progress
Mode: Behavior Change (user-visible), Operate surface, established visual world
Delegation decision: **not delegated** — the user explicitly directed execution
through the `impeccable` skill, which owns the design + build path for this
request. (AGENTS.md exception: "the user explicitly says not to delegate" —
here, the user named the execution path.)

## Direction contract

- **THESIS** — the session is an instrument with a live head, not a message
  list. What the agent is doing *right now* (tool, file, decision it needs)
  holds one fixed panel that never scrolls away; finished turns fall into a
  settled transcript below that reads freely because nothing pushes it. It
  refuses the messenger arrangement the category ships, where streaming
  output, reasoning noise and the one approval you must give all compete in
  the same auto-scrolling column.
- **OWN-WORLD** — Ajax Cockpit, unchanged (DESIGN.md preserved, not rewritten).
  Soft Charcoal paper steps, hairline rules, Soft Steel Blue as the running
  signal, `--tone` for status, mono only where the CLI speaks (tool names,
  paths, code), uppercase tracked micro-labels for chrome, pill actions ≥44px,
  flat depth.
- **STORY** — the operator opens a session on a phone, sees one panel saying
  what the agent is doing and whether it needs them, answers if asked, scrolls
  the transcript for history, types to steer.
- **FIRST VIEWPORT** — header (back / title / status tone) → live head
  (activity + running tool + decision block when one exists) → settled
  transcript → composer. Primary action is whatever the head asks for; with
  nothing asked, the composer is primary.
- **FORM** — candidate 6 of 7 on the grounded list ("instrument stack: live
  head over settled transcript"). Staging fused from the wound-medium
  challenger: live head distinct from settled tape, honest position readout,
  jump-to-live. Seed key `361116ac`, scope surface, mode operate.
- **FINISH** — unreviewed and undocumented is unfinished; this build ends with
  the finish review, the verdict, and DESIGN.md.

DESIGN.md is *not* rewritten: this is a surface inside an established world,
which new-work.md classes as an ordinary extension.

## Grounded structural candidates (ordered by resonance)

1. Standard messenger thread (the incumbent — the rut)
2. Thread + persistent status rail
3. Tabbed Chat | Work | Terminal
4. Transcript-as-operator-log, one dense column with gutter markers
5. Turn-carded thread, each prompt→response one collapsible card
6. **Instrument stack: live head over settled transcript** ← assigned
7. Command-bar first, results streaming downward

Challengers weighed and rejected: *visible transit* (spatial metaphor costs the
upper field, buys nothing for a linear ACP turn on a phone), *tethered bench*
(no "alongside" at 390px; diff already has its own route). *Wound medium*
partially fused — its live-head-vs-settled-tape split and honest position
readout carried into candidate 6.

## Diagnosed defects (evidence for the redesign)

1. **Tool calls are raw JSON dumps.** ACP's `tool_call` / `tool_call_update`
   updates are unmapped in `slices/web_session.rs`, so they hit the
   `other if !other.is_empty()` catch-all and render as collapsed
   `▸ tool_call` rows containing `serde_json::Value::to_string()`. Every file
   read/edit/command = one unlabeled JSON blob. Root cause of the wall.
2. **Reasoning is rendered as chat.** `thought`/`thought_chunk` map to role
   `system` and render as permanent centered messages, drowning the answer.
3. **No markdown.** Agent prose renders as `<p>` + `pre-wrap`; code blocks,
   lists and inline code are unreadable.
4. **Forced autoscroll.** `node.scrollTop = node.scrollHeight` on every item
   change — you cannot scroll up while streaming.
5. **No turn boundary.** `RequestFinished(Ok)` is dropped, so the client never
   learns a turn ended; no busy state, no settle point.
6. **Status spam.** Every `state_update` appends a new "Status: running" line.
7. **Everything real is two taps deep** behind `⋯` — status, activity,
   annotations, actions, diff, terminal. Contradicts PRODUCT.md principle 2.
8. **Permission banner is detached** from where the thread is, and duplicates
   itself as a system message.
9. **Transport churn.** `useEffect([handle, starterContext])` disposes the
   socket when the starter object identity changes; last release kills the ACP
   child process mid-work.
10. **No reconnect** and no visible disconnected state.

## Tasks

- [x] T1 Rust: map `tool_call` / `tool_call_update` to a first-class
      `SessionServerEvent::ToolCall`; add `thought` role; add `TurnEnd`;
      structure `plan`. Verify: `cargo nextest run -p ajax-web`.
- [x] T2 Rust: emit `TurnEnd` from `RequestFinished` for `session/prompt` in
      `web_session_acp/hub.rs`. Verify: unit test in hub.rs.
- [x] T3 Web: `sessionThread.ts` pure reducer (events → live head + settled
      turns + decision + connection). Verify: `sessionThread.test.ts`.
- [x] T4 Web: `Markdown.tsx` — dependency-free markdown-lite (fences, inline
      code, lists, headings, bold) to React nodes. Verify: unit test.
- [x] T5 Web: `LiveHead.tsx` instrument + rewritten `SessionChat.tsx` shell
      (bottom-anchored transcript that only pins when already at bottom,
      jump-to-live, inline decision, composer with contextual Stop).
- [x] T6 Web: `SessionStarter.tsx` flow pass — repo + title above the fold,
      optional brief fields folded, one primary.
- [x] T7 Web: replace the `.session-*` CSS block in `styles.css`.
- [x] T8 Update `SessionChat.test.tsx` to the new structure (structural
      assertions only; no assertion weakened).
- [x] T9 Detector + batched desktop/mobile screenshot inspection, fix in one
      batch, confirm once.
- [x] T10 Finish review (impeccable-finish-reviewer).
- [x] T11 Apply the review's 8 material fixes in one batch, rebuild, recapture.
- [x] T12 Verdict pass — all 8 scored **resolved**; reviewer named 2 regressions
      introduced by the batch + 1 remaining item.
- [x] T13 Second (final) correction round for those 3.
- [x] T14 Final verdict: R2 **resolved**, remaining item **resolved**, R1
      **partial** (evidence gap, not a code gap) — closed below.

## Final verdict and R1 close-out

The reviewer scored R1 partial because every capture showed the composer at
rest (empty draft), where the shared `.pill:disabled { opacity: 0.4 }` governs —
so the recaptures were pixel-identical and the fix, which only changes the
*enabled* state, was unverifiable.

Closed by capturing the missing state rather than by changing code:
`session-decision-typed-{mobile-webkit,desktop-chromium}.png` holds a decision
open with a typed draft. Send renders with a legible Ink label and Rule Strong
border while Approve keeps the accent fill — the head keeps primacy and the
send affordance stays findable, which is exactly what the finding asked for.

The reviewer's alternative — exempting this control from the 0.4 disabled floor
— was **declined**: DESIGN.md §5 sets `Disabled: opacity: 0.4` as the system
convention, at rest there is genuinely nothing to send, and carving out one
control would drift the system to satisfy a capture.

Correction-round ceiling reached (two rounds); no third verdict pass was run.

## Open, needs Matt's decision

1. **Ceiling items** — terminal absent from the session first viewport, and no
   keyboard-open chrome-collapse band. Both are governed by AGENTS.md Web
   Cockpit Guardrails and were deliberately left alone.
2. **DESIGN.md edit** — the §5 scoped exception can be reverted if the doc
   should stay untouched; it currently makes the doc match the ship.
3. **`.impeccable/design.json` sidecar** is stale because of that edit.
   Refreshing it is `/impeccable document`; not run, since repairing drift as a
   side effect of a design task is explicitly out of process.
4. **Two false-positive hook rules** (`bounce-easing` on `var(--ease-spring)`,
   which is easeOutQuint with no overshoot; `design-system-color` on the `#000`
   mask alpha stop). Not suppressed — a waiver needs explicit confirmation.

## Second correction round

- **Send went invisible during a decision** (regression from fix 4). Kept
  `secondary` so the head owns primacy, restored control contrast via
  `.session-composer .pill[data-variant="secondary"]`.
- **Destructive `Drop` landed in the head's fast-tap row** (regression from fix
  6) at the same coordinates `Approve` occupies one state over. The head now
  filters to `!action.destructive`; Drop stays in Task details. Matches the
  earlier product call that took Drop off the dashboard. Test asserts removal
  never becomes unreachability.
- **Approve/Reject looked enabled while disconnected.** Now `disabled` on
  `!connected`, so the guard is visible instead of a silent no-op.

## Review fixes applied (one batch)

1. **Reconnect dropped approvals and prompts silently.** `sendJson` returns on a
   closed socket, but `respondDecision` had already cleared the decision and
   `sendDraft` had already recorded the message as sent — the agent stayed
   blocked and the transcript lied. Both now early-return on `!connected`; Send
   disables, the placeholder reads "Reconnecting…", the decision stays up.
2. **Two status vocabularies.** Header pill (`WAITING`, lifecycle) contradicted
   the head (`WORKING`, turn state). The pill now renders only at head-idle.
3. **Jump-to-live lied.** `behind` fired on the unpin itself and the count was
   the session total. Now keyed off entries actually changing while unpinned,
   labelled `N new steps` from a delta captured at the live edge.
4. **Primary taps were 34px.** Scoped `min-height: 44px` for this route's
   decisive actions; global `.pill` untouched. Send drops its accent fill while
   a decision is pending so the head keeps primacy.
5. **DESIGN.md contradicted the ship.** Added a scoped exception under §5 for
   the flag-gated session route (composer as work surface, terminal as escape
   hatch, ACP not PTY) and pointed §6's "Don't" clause at it.
6. **`attention` head offered no action.** It now renders the task's own
   `ActionBar` (server order — `primary_action` is always Resume and must never
   drive ordering).
7. **`aria-live` wrapped the whole head**, re-announcing Stop/Details on every
   thought chunk. Scoped to the state line and working block; decision gets
   `role="alert"`.
8. **Head/transcript seam clipped mid-glyph.** 12px top mask on the thread,
   shorter than the resting padding so nothing softens at rest.

Deliberately not done, disclosed to the reviewer: terminal primacy in the first
viewport and keyboard-open chrome collapse are governed by AGENTS.md Web Cockpit
Guardrails and need Matt's sign-off. Reduced-motion already collapses
`session-live-pulse` via the existing global `prefers-reduced-motion` rule.

## Validation

| Command | Result |
| --- | --- |
| `cargo fmt --check` | pass |
| `cargo clippy --all-targets --all-features -- -D warnings` | pass |
| `cargo nextest run --all-features --test-threads=1` | pass — 1849 tests |
| `cargo test --doc` | pass — 0 tests |
| `npm run verify:arch` | pass — 2 tests |
| `npm run ci:verify` | pass — 19 tests |
| `npm run web:check` | pass |
| `npm run web:lint` | pass |
| `npm run web:sg` | pass |
| `npm run web:test -- --run` | pass — 749 tests, 74 files |
| `npm run web:build` + `web:build:check` | pass — dist/ rebuilt (vendored, byte-equality matters) |
| `CI=1 npx playwright test e2e/session-chat.test.ts` (both projects) | pass — 12/12, no retries |
| `detect.mjs` (5 session components) | pass — `[]` |

### Full smoke suite — pre-existing flakes, verified not mine

`CI=1 npm run web:smoke` fails 1–2 mobile-webkit tests per run, always in
`terminal-behavior.test.ts` / `actions.test.ts`, never in the session route,
and each passes in isolation. Verified against the base by stashing this whole
change: the base run also failed two (different) `terminal-behavior.test.ts`
tests. Different tests fail each run — flaky, pre-existing, unrelated.

| Run | Failures |
| --- | --- |
| with change | `actions:226 connection Reload`, `terminal-behavior:1545 compact keys` |
| with change, session spec excluded | `terminal-behavior:1328 fullscreen keyboard-open` |
| **base (changes stashed)** | `terminal-behavior:1358 inline keyboard-open`, `terminal-behavior:2025 80-column grid` |

### Design-hook findings left unchanged

4 findings in `styles.css`, all in pre-existing components outside this diff
(`.test-in-dev` 8px radius + `rgba(188,92,62,.12)`, `.diff-flag` `#c47b1a`,
sheet overlay `rgba(0,0,0,.6)`). Not introduced here; fixing them would widen
the diff past the request. The one finding that *was* mine — a literal `4px`
radius on `.md-code` — is fixed to `var(--radius-sm)`.

## Deviations

- `SessionChat.test.tsx` rewritten against the new structure. A redesign
  invalidates structural assertions by definition; behaviour coverage was
  preserved and extended (turn settling, decision inline, autoscroll pinning,
  reconnect, transport stability), not weakened. 8 tests → 18.
- No interview round. The recorded user preference is "Build, don't interview —
  on UI work Matt rejects question rounds; pick a direction, build it, show a
  Playwright screenshot." Disclosed to the user in the first reply.
- DESIGN.md deliberately not rewritten: established world, ordinary extension.
- Two bugs found by the layout probe rather than by reading, both fixed:
  the `[data-outlet="session"]` section defaulted to `display:block`, breaking
  the flex chain so the whole route scrolled and the composer floated over the
  transcript; and the session sheets had no scrim or bottom anchor (they
  rendered top-aligned over a live page), now following the `NewTaskSheet`
  pattern already in the repo.
- A WebKit-only subpixel bleed let clipped transcript text smear into the head
  at DPR 3; fixed by giving the head its own paint layer.
