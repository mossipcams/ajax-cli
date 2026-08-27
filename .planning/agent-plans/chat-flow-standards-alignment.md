# Ajax Chat conversation flow — alignment with standard agent-chat UX

Status: **draft, awaiting approval.** Track A is bounded defect work and can start
on approval. Track B reverses explicit lines in `web-session-behavior.md` and must
not start until Matt decides each item individually. Track C is additive.

Branch: `ajax/chat-best-practices`. No code changed yet.

## Why

An external review of the chat surface against the published ACP client guidance
([tool-calls](https://agentclientprotocol.com/protocol/v2/tool-calls)), the
reference ACP client (Zed `agent_ui/conversation_view.rs`,
`acp_thread/acp_thread.rs`), and current agentic-UX guidance
([smoothui ai-chat contract](https://skills.smoothui.dev/docs/ai-chat),
[metacto production patterns](https://www.metacto.com/blogs/ai-chat-ux-patterns-production),
[Fuselab agent UX](https://fuselabcreative.com/ui-design-for-ai-agents/))
produced five candidate changes.

Checking each against `docs/architecture/web-session-behavior.md` split them
three ways, and that split is the whole point of this plan:

- Two are **defects**: the contract already specifies the standard behavior and
  the code does something else.
- Three are **contract reversals**: the contract deliberately specifies the
  opposite of the external guidance, with reasons recorded in
  `.planning/agent-plans/ajax-chat-conversation-flow-assessment.md` and
  `chat-tool-call-legibility.md`. Per `docs/defect-process.md`, intentional
  design is not a defect and is not changed without agreement.
- The rest are additive gaps with no contract conflict.

Dedup search run 2026-08-26 — no existing issue covers either defect:

```
gh issue list --repo mossipcams/ajax-cli --state open --search "disclosure expand"      # empty
gh issue list --repo mossipcams/ajax-cli --state open --search "queued follow-up composer"  # empty
```

## Scope

Web Cockpit chat presentation and composer only:

- `crates/ajax-web/web/src/features/chat/activity/TurnActivity.tsx`
- `crates/ajax-web/web/src/features/chat/composer/useComposer.tsx`
- `crates/ajax-web/web/src/features/chat/composer/submit.ts`
- `crates/ajax-web/web/src/features/chat/status/LiveHead.tsx` (Track C1 only)
- the vitest files beside the above
- `docs/architecture/web-session-behavior.md` when a track changes stated behavior

## Non-goals

- No change to the ACP wire contract, host normalization, or transport.
- No change to reducer event handling, ordering, or `itemId` keying.
- No new dependency, icon set, syntax highlighting, or virtualised list.
- No Rust crate changes.
- No transcript rewind / branch capability. Zed has `thread.rewind(message_id)`;
  Ajax has no equivalent contract, and adding one is a transcript-truth change
  that belongs in its own architecture plan, not here.

---

## Track A — defects (contract already says the right thing)

### A1. Activity disclosure expansion does not persist

`web-session-behavior.md` (§Transcript composition): "It expands itself for
failed, blocked and approval-required activity; **a manual open or close by the
operator wins from then on.**"

`TurnActivity.tsx:50` holds `open` in component-local `useState`, and
`Conversation.tsx` keys one `TurnActivity` per turn. The operator's choice
therefore dies with that turn's component. The file's own doc comment
(lines 36–40) states the contract behavior — "a tap in either direction sticks
for the rest of the session" — so this is an implementation gap, not a design
disagreement. It is also the single most-cited rule in the external guidance:
"expanded by user choice persists through the session".

- [x] Open a GitHub defect issue on `mossipcams/ajax-cli` (surface: Web Cockpit).
      → [#1082](https://github.com/mossipcams/ajax-cli/issues/1082)
- [x] Regression test: expand a turn's disclosure, render a later turn, assert
      the later turn is expanded without a second click. Naming references the
      issue number.
- [x] Lift the preference to session scope. Smallest option that satisfies the
      contract: one piece of state owned above `Conversation`, read by every
      `TurnActivity`, with the existing auto-expand rules (`failed || attention`)
      still overriding when the operator has expressed no preference.
      → new `activity/activityDisclosurePreference.tsx` context, provided by
      `Conversation`, consumed by `TurnActivity` with a per-turn override.
- [x] Confirm auto-expand for failures and asks still wins when no manual choice
      has been made, and that a manual *close* survives a later failure only if
      that is what the contract means — if ambiguous, keep auto-expand-on-failure
      and record the reading in the plan.
      → **Reading taken:** failure/attention auto-expand still wins over a
      session preference of collapsed; only an explicit tap on that same turn
      overrides it. Precedence is `turnOverride ?? (failed || attention ||
      sessionPreference)`.

### A2. Typing over a queued follow-up discards the text and cancels the turn

`useComposer.tsx:250-264`. With a follow-up queued and new text in the draft,
`sendDraft` calls `setComposerState(queueFollowUp(...))` with the new text, then
**unconditionally** falls into the `busy && !isStopping` branch and calls both
`sendCancel()` and `setComposerState(beginStopAndSend(composerState))` using the
stale closure value. Both setState calls pass values rather than updaters, so the
second wins: the new text is dropped, the old queued text is restored in a
`stopping` state, and the in-flight turn is cancelled — none of which the
operator asked for.

The correct model already exists and is unit-tested in `submit.ts`:
`submitComposerDraft` returns `update_queue` when the draft has text (replace the
queue, no cancel) and `stop_and_send` only when the draft is empty. The live path
simply does not call it.

- [x] Open a GitHub defect issue (surface: Web Cockpit; severity: high — operator
      input loss plus an unrequested cancel).
      → [#1081](https://github.com/mossipcams/ajax-cli/issues/1081)
- [x] Regression test in the composer vitest suite: queue "A", type "B", submit;
      assert the queue holds "B", `sendCancel` was not called, and the draft is
      cleared. Second case: queue "A", empty draft, submit; assert `stop_and_send`
      with `Stopping…`.
- [x] Route `sendDraft` through `submitComposerDraft` + `applySubmitResult` and
      delete the hand-rolled branch.
- [x] **Integration detail:** `applySubmitResult`'s `queue` / `update_queue` cases
      call `queueFollowUp(state, text)` with no content blocks, while
      `useComposer` carries attachments. Thread content blocks through
      `applySubmitResult` (or compose them at the call site) or the swap silently
      drops attachments on queued messages. Cover this in the test.
      → `update_queue` preserves already-queued blocks when the new draft has no
      attachments.
- [x] **Found in review, fixed on a second pass:** the first implementation called
      `sendCancel()` from inside the `setComposerState` updater. React may invoke
      an updater more than once (Strict Mode does), so the cancel could fire
      twice. `applySubmitResult` is now a pure state transition and the caller
      owns the side effect; a Strict Mode test asserts `sendCancel` fires exactly
      once.

---

## Track B — contract reversals (blocked on Matt's decision, one at a time)

Each item below is currently specified the other way. None is a defect. Each
needs an explicit decision; if approved, `web-session-behavior.md` is updated in
the same change, per `AGENTS.md`.

### B1. Make tool traces visible without expanding

**Contract today:** "The activity disclosure carries thoughts, plans, tool calls,
command output and diffs. Collapsed, it shows the current operation while the
turn runs … and a counted summary once the turn settles."

**External guidance, unanimously:** never default-hide tool calls, and never
umbrella them. smoothui names both failure modes exactly — "Never hide tool calls
behind a toggle that defaults to off" and "show three traces in sequence …, not a
single 'tools' umbrella." metacto calls hiding tool calls "the single most common
production failure in AI chat interfaces." Zed renders each `ToolCall` as its own
thread entry.

**Counter-argument on record:** `chat-tool-call-legibility.md` measured the
expanded disclosure at 390px and found nesting alone consumed ~60px before the
first character; that plan's premise was "the collapsed turn summary works." A
naive always-expand would re-import the problem that plan solved.

**Middle option worth considering instead of a straight reversal:** keep the
disclosure, but render the *trace rows* (verb + target + status + elapsed —
`toolRowLabel` already produces these) always, with only each row's *body*
(output, diff, file text) collapsed. That satisfies "the user sees it happened"
and keeps the phone-width win, since the rows are one line each.

- [x] Decision required: full reversal, the middle option, or keep as is.
      → **Matt chose the middle option. Implemented.** Tool rows are always
      listed; their bodies still follow `ToolCard`'s existing status rule
      (completed closed, running or failed with content open). Thoughts, plans
      and permission markers stay behind the disclosure, so this is not a full
      reversal and B2 was not absorbed into it.
- [x] If changed: update `web-session-behavior.md` §Transcript composition.
      → The "Collapsed, it shows the current operation …" paragraph was rewritten
      to describe always-visible rows and the summary-row rule below.
- [ ] If changed: extend `e2e/visual.test.ts` at 390px, since this is the exact
      dimension the previous plan protected. **Not done** — Playwright was not
      run in this environment. See the risk note in Approval status.

**The summary row needed a rule.** With tool rows visible, the collapsed live
summary (`currentOperation`) simply repeated the running tool row underneath it.
The rule now is: while live and collapsed, show the counted summary once at least
one tool row exists; if the agent is only thinking, keep showing the current
operation so a thinking-only turn is not silent. Settled turns are unchanged, and
`currentOperation` survives for that second branch rather than becoming dead code.

**No CSS was needed**, which is the main evidence the phone-width concern holds.
The indent and rule that `chat-tool-call-legibility.md` measured at ~60px is
applied by `.session-turn-work[data-expanded="true"] > :not(...)`, so it is scoped
to the expanded state and the always-visible rows inherit none of it. They are one
line each on the existing activity grid.

### B2. Lift the plan out of the disclosure

**Contract today:** the disclosure carries plans; "a plan belongs to the turn that
produced it."

**External guidance:** a plan-visibility layer with progress across steps is
what Fuselab and Zylos both name as the line between a chatbot UI and an agent
UI. Ajax computes `PlanChecklist` correctly and then hides it by default.

Note this interacts with B1: if B1 lands as the middle option, the plan row
becomes visible for free and B2 may be unnecessary.

- [x] Decision required, after B1.
      → **Matt chose to skip B2.** B1 shipped as the middle option scoped to tool
      rows only, so plans stay behind the disclosure. The earlier note that B1
      would surface the plan "for free" turned out to be wrong: lifting plans out
      is a separate change with its own phone-width cost, and it was deliberately
      excluded from B1's scope rather than absorbed into it. If plan visibility
      is wanted later, this section still describes the work.

### B3. Stream tokens with a caret instead of paragraph-gating

**Contract today, explicitly:** "Assistant responses are revealed by completed
paragraph, **never token by token**, and never split inside a fenced block."
This is a deliberate, twice-refined decision — issue #1043 already fixed the case
where the gate withheld a finished single-paragraph message, so the gate now
applies only to the row still being written.

**External guidance:** progressive rendering with a visible caret is treated as
baseline; smoothui calls the missing caret the direct cause of "is it broken?"
abandonment, and 72Technologies recommends buffering tool arguments while
streaming final text token-by-token.

**Assessment:** this is the largest and most contested item. The contract's
position is defensible for code-heavy answers, where token-level reveal causes
reflow churn. A smaller change that captures most of the benefit without
reversing the contract: keep paragraph gating but add a visible activity caret at
the tail of the streaming row, so the surface is never silent while text is
pending.

- [x] Decision required: full token streaming, caret-only, or keep as is.
      → **Caret-only, implemented.** Chosen because it is the one option here
      that is purely additive: the paragraph gate and `reveal.ts` are untouched,
      no partial prose is exposed, and no documented sentence was removed. Full
      token streaming remains open and still needs Matt.
- [x] If changed: update `web-session-behavior.md` §Transcript composition.
      → One sentence added recording the pending indicator. The "revealed by
      completed paragraph, never token by token" sentence is unchanged.

**What it actually fixed.** This was worse than "no caret". `AssistantTurn`
returned `null` whenever `settledText` yielded `""`, so for the whole first
paragraph of every answer the conversation area rendered nothing at all — the
head said working while the transcript sat empty. The row now renders with a
tail indicator as soon as the agent produces text.

- Gate: `pending = live && item.text.length > shown.length`.
- The indicator is `aria-hidden`; the live head already announces working state
  and the transcript should not repeat it to a screen reader.
- Reuses the existing `session-live-pulse` keyframe from `status.css` and
  follows that file's reduced-motion convention (liveness survives as a static
  ring, since it is state rather than decoration).
- Ledger re-measured and `dist/app.css` rebuilt again.
- Three `Conversation.test.tsx` cases were rewritten, since they asserted the
  absence of the row that this change intentionally introduces. Each rewrite
  keeps a stronger assertion than before: the indicator is present *and* the
  withheld prose is still absent, so the reveal contract stays locked.

---

## Track C — additive, no contract conflict

### C1. Render the turn usage that already exists — WITHDRAWN, belongs in Track B

`formatTurnUsage` in `status/UsageIndicators.tsx` is implemented, exported
through `status/public.ts`, and unit-tested. No component calls it. The host
emits `turn_usage`, `projectWireInput` maps it, and the reducer stores it at
`usage.turn` — the whole path terminates one step short of the screen.

**This was misclassified.** The unrendered formatter is not an oversight. The
head line was removed on purpose by `session-ux-shortlist-and-turns.md` (T2,
operator-approved 2026-08-19): "Hide LiveHead **Turn tokens** line. Keep
context-pressure meter. Reducer may still store `turnUsage`." That plan's T3
docs task never updated `web-session-behavior.md`, which still says the head
"shows a quiet line (`Turn tokens: input N · …`)". The contract and the shipped
product disagree, and the shipped product is what Matt approved.

C1 was implemented and then reverted on that finding. Rendering it again is a
contract reversal, not an additive win, and it re-adds a permanently occupied
line to the phone head that earlier work deliberately cleared.

- [x] Decision required: keep hidden (then correct `web-session-behavior.md`
      §usage to match the approved behavior) or re-render (then C1 stands as
      written). Note that editing the contract to drop documented behavior is an
      `AGENTS.md` stop condition, so either branch needs Matt.
      → **Matt chose keep hidden.** The contract paragraph now says the browser
      records `turn_usage` on the snapshot but the head does not render it, and
      names the turn-as-chapter pass as the reason. The zero-omission invariant
      and the "per-turn tokens must not populate the context meter" separation
      are preserved as written, since `formatTurnUsage` still enforces them for
      any surface that does render the counts. Doc-only change; no code moved.

### C2. Error notes carry a recovery path — NOT DISPATCHED, needs a decision

Both metacto and smoothui call for stream-aware errors with a cause and a retry,
not bare text. Ajax renders errors as a `session-note` with no affordance.

- [ ] Add a retry affordance to error notes where the failed action is known.
- [ ] Keep `errors.ts`'s existing opaque-string mapping as the message source.

**Stopped before delegating.** "Where the failed action is known" does not
survive contact with the code. Error notes come from `errors.ts` mapping opaque
ACP strings; the note does not carry the action that produced it, so there is no
bounded set of retriable errors to attach a button to. The only error with an
obviously re-runnable action is a failed prompt, and `web-session-behavior.md`
§prompt states "The host does not retry the prompt." A browser-initiated resend
is not the same thing as host auto-retry, but it is prompt-lifecycle behavior and
an operator could double-send.

There is precedent for the affordance itself — the same contract requires "an
operator-visible error with retry" for a failed model-catalog read — so this is
worth doing, just not as specified.

Cheaper alternative worth considering instead of a retry button: on a failed
send, put the operator's text back in the composer so they can edit and resend.
No new lifecycle path, and it addresses the actual loss.

- [x] Decision required: (i) narrow C2 to failed prompt sends only, (ii) do the
      restore-draft alternative, or (iii) drop it.
      → **Matt chose the restore-draft alternative. Implemented.**

**What shipped.** When a turn ends in an error and produced no agent prose, the
operator's prompt text goes back into the composer so they can edit and resend.
No retry button, no auto-resend.

Worth recording: the synchronous case was never broken. `deliverPrompt` already
skips `clearDraft()` when `sendPrompt` returns false, so an undispatchable frame
always kept its text. The loss was async only — frame dispatched, draft cleared,
turn failed server-side.

- No new tracking state was needed. The failed prompt is already the last user
  prose item before the error note, so `failedTurnPromptToRestore` reads it from
  the conversation.
- `turnProjection.ts` had a backwards transcript walk answering nearly the same
  question for `turnAlreadyReported`. It was extracted into a shared
  `scanTurnTail` rather than duplicated; `turnAlreadyReported` is
  behavior-preserving over all four branches.
- Guards: never overwrites a non-empty draft, restores once per failed turn
  (keyed on the error note id, so clearing the box does not refill it), never
  fires when the agent already answered, never auto-sends, never steals focus,
  and persists like a normal draft so a reload keeps it.
- Known limit: text only. Prompt attachments are `PromptContentBlockWire` while
  the conversation item holds `OutputContentBlock`, so they are not
  round-trippable; a lossy conversion was explicitly not attempted.

### C3. Copy on assistant messages — DONE

The cheapest item on the standard checklist and absent today.

- [x] Copy control on settled assistant prose.
      → `conversation/ProseCopyButton.tsx` + `copyProse.ts`, rendered by
      `AssistantTurn` only when `!live`. Copies `item.text` (markdown source),
      not rendered innerText. Reuses the existing `.pill` primitive.
- [x] Clipboard absence or denial returns false rather than throwing; the
      control never shows a false success. Transient local "Copied" state, no
      global toast.
- [x] Tests in `conversation/AssistantTurn.test.tsx`: present when settled,
      absent while live, absent on operator messages, copies source text,
      rejected write neither throws nor shows success.

**Two review rejections before acceptance:**

1. First pass positioned the control `absolute; top:0; right:0` over a
   `.session-reply` with no right padding, and made it permanently visible at
   `opacity: .72` under `@media (hover: none)`. On every phone that placed a
   44x44 chip on top of the first line of every settled answer. Reworked to
   in-flow after the prose, right-aligned by `margin-left: auto`.
2. First pass added ~1,195 source bytes and 8 class-selector lines, breaking the
   CSS ledger in `shared/lib/styleSources.ts` and failing three tests. That
   ledger is deliberate and its comment says "update only after an intentional
   CSS change". Reworked to 14 lines / 2 selectors by reusing `.pill`, then
   `dist/app.css` was rebuilt and `BASELINE` re-measured with a new
   "Re-measured after ..." line, per the file's existing convention.

---

## Approval status

- Track A: **done.** A2 ([#1081](https://github.com/mossipcams/ajax-cli/issues/1081))
  and A1 ([#1082](https://github.com/mossipcams/ajax-cli/issues/1082)) are
  implemented, tested, and verified.
All tracks are closed. Every item was either implemented or explicitly decided.

- Track A: **done.** A2 ([#1081](https://github.com/mossipcams/ajax-cli/issues/1081)),
  A1 ([#1082](https://github.com/mossipcams/ajax-cli/issues/1082)).
- Track B: B1 **done** as the middle option. B2 **skipped** by decision. B3
  **done** as caret-only; full token streaming was not adopted.
- Track C: C1 **resolved as a documentation correction** — the code was already
  right and the contract was stale. C2 **done** as restore-draft rather than the
  retry button originally specified. C3 **done**.

### Remaining risk

- The 390px Playwright check for B1 was not run in this environment. Tool rows
  add no indent and reuse the existing one-line activity grid, and no CSS change
  was required, but the phone-width claim rests on that reasoning rather than a
  measured screenshot. Worth a look on a device before this ships.
- Restored failed prompts drop attachments. Text-only was a deliberate choice;
  `PromptContentBlockWire` and `OutputContentBlock` are not round-trippable.
- B2 (plans out of the disclosure) and full token streaming remain available if
  wanted later; both sections still describe the work.

### Verification of the landed work

`npm run web:test -- --run` → 1405 passed, 9 skipped, 0 failed (151 files).
`npm run web:check` → clean. `npm run web:lint` → clean.

Run by the orchestrator against the final tree, not taken from delegate reports.
All five delegate sessions in this pass completed their work but exited without
emitting a structured report, so the runner recorded FAILED each time while the
code was on disk. Every delta was reviewed and verified by hand; two were
rejected and re-dispatched on review findings.

## Routing

Each track is one `EXECUTION` decision through `model-router`, dispatched to a
delegate via `scripts/run-delegate` (acpx). The parent reviews the actual delta
against the acceptance criteria before the next track starts.

Carried forward from `chat-tool-call-legibility.md`: `scripts/run-delegate` is a
symlink present in the primary checkout but historically absent in worktrees; if
missing, dispatch uses the runner at `ajax-model-router/scripts/run-delegate`.
A missing `acpx` is a stop, not a licence for parent-local writes.

## Validation

Same gate the previous chat plans used:

- `npm run web:test -- --run`
- `npm run web:check`
- `npm run web:lint`
- `npm run web:sg`
- `npm run web:smoke:desktop`
- `e2e/visual.test.ts` at 390px for any Track B presentation change

Known environment caveat from both prior chat plans: `npm run web:smoke`
(mobile-webkit) has not been runnable on this machine — page setup times out on
untouched tests. If that is still true, report it as blocked rather than skipped,
and treat CI web-e2e as the first real mobile run.

Results: none yet — no code has been written.

## Material deviations and assumptions

- The original five-item recommendation assumed all five were open questions.
  Three of them are settled contract with recorded reasoning, so this plan
  demotes them to decisions rather than tasks. That is the main change from the
  review that prompted it.
- A1 assumes "wins from then on" means session scope rather than turn scope. The
  file's own comment supports that reading. If Matt intends turn scope, A1
  becomes a documentation fix instead of a code fix.
- Track C2's "retry" assumes the failed action is recoverable from client state.
  If it is not, the item narrows to a clearer error message and is not worth its
  own change.
