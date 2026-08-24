# Ajax Chat conversation flow — full assessment

**Date:** 2026-08-22
**Branch:** `ajax/conversation-flow`
**Method:** drove `#/session/<handle>` in a real browser through 15 scripted
states (Playwright, 393×852, `desktop-chromium`), read every screenshot, then
traced each observation to source. Contract measured against
`docs/architecture/web-session-behavior.md` §Transcript composition,
§In-flight activity freshness, §Queue and cancellation.

**Verification caveat:** `mobile-webkit` cannot launch on this machine (page
setup times out on untouched tests too, before and after
`playwright install webkit`), so the sweep ran on chromium at an iPhone
viewport. Layout findings should be re-checked on webkit before landing fixes.

Two apparent findings were discounted as fixture artifacts, not product
defects: the model control not rendering (my `choices` used `{id,name}`; the
parser wants `{value,name}` — `liveSessionConfig.ts:38`), and failed-tool output
not showing (I sent `type: "content"`; the wire type is `text`).

---

## Ranked findings

### 1. Interim agent messages are invisible for the entire turn — FIXED, #1043

`settledText` (`conversation/reveal.ts:5`) returns `""` when the text contains
no `\n\n`. A single-paragraph message — "Let me look at the handler.", "That
needs a destructive command." — never contains one, so it renders as nothing
until `turn_end` flips `live` off.

Observed: with a permission ask pending, the agent's one-line explanation of
*why* it wants `rm -rf target/debug` was absent from the transcript while the
operator was being asked to approve it.

The paragraph rule is right for the *streaming tail*. It is wrong for a message
that is already complete because something else arrived after it. Fix: gate
reveal only on the trailing prose row when nothing follows it — a tool call, a
permission, or a later message is proof the previous message finished.

### 2. The live turn is narrated twice, ~1000px apart, with a void between — FIXED (design change, no issue: not a defect)

While working, the head prints `WORKING` + `$ npm run web:test` + `RUNNING` +
the current plan step + context meter. The transcript tail prints
`Running npm run web:test…` — the same fact, verbatim, at the other end of the
screen, with an empty black band between them.

`LiveHead.tsx:84-111` and `TurnActivity.tsx:68` (`currentOperation`) are two
renderers of one state. On a phone the operator's eye has two live regions and
neither is where the conversation is.

Pick one owner. The transcript tail is the better one (it is where the
conversation is, and it becomes the turn's history row when the turn settles);
the head keeps state + Stop.

### 3. A cancelled or errored turn leaves its tool calls `RUNNING` forever — FIXED, #1044

`activityProjection.ts` has no `turn_end` case. Nothing marks unsettled calls
terminal, so after Stop the head still advertises a command that is not running.

Observed on one screen: head says `$ npm run web:test … RUNNING`, transcript
says `Ran 1 command`, divider says `STOPPED`. Three states, one turn.

Fix: on `turn_end`, settle every `pending`/`in_progress` call in that turn
(`cancelled` for a cancel, `failed` for an error).

### 4. Turn rows rendered out of chronological order — FIXED, #1042

`groupTurns.ts` bucketed a turn into work/agents/other and rendered the buckets
in fixed order, hoisting every tool call above everything the agent said. Turns
now carry ordered `rows`; adjacent work still collapses into one disclosure.
Regression test in `groupTurns.test.ts`.

Doc drift this creates: web-session-behavior.md §Transcript composition still
says "one activity disclosure per turn". It is now one per contiguous work run.
Update that line when this lands.

### 5. A failed turn prints two error messages, one of them false — FIXED, #1045

`turnProjection.ts:304-322` appends "The agent stopped without a response.
Check the selected model or try again." on every `turn_end{error}` —
unconditionally. When the host already sent a typed `error`, the operator gets
both. When the agent *did* answer before failing, the sentence is simply untrue.

Fix: append it only when the turn produced no agent prose and no error note.

### 6. The permission ask shows raw markdown backticks — FIXED, #1046

`PermissionPanel.tsx:13` renders `decision.title` directly. The transcript
marker runs `cleanTitle`. Both were on screen at once: head
``Run `rm -rf target/debug` `` vs transcript `Run rm -rf target/debug`.

This is the sibling of #970 (which fixed case-folding on the same string), on
the same control, at the same moment — the one where the operator is deciding
whether to allow a destructive command.

### 7. The plan is a session singleton, not a per-turn row — FIXED, #1047

`activityProjection.ts:90` finds the first `kind === "plan"` item anywhere in
the conversation and overwrites it. A new plan in turn 5 silently rewrites the
plan shown inside turn 1's disclosure, and no turn after the first ever gets
its own plan row.

### 8. No way back to the bottom of a long transcript — FIXED (contract gap, no issue)

`useChatScroll.ts:52` sets `behind` only when the revision changes while
unpinned, i.e. only when *new content arrives* while you are scrolled up. I
wheel-scrolled to the top of a 12-turn settled transcript: no `Jump to latest`.
On a phone that is a long drag back.

Contract says the transcript "offers `Jump to latest` otherwise" — otherwise
than being at the bottom, not otherwise than idle.

### 9. Task attention suppresses the chat's own head line — FIXED (contract gap, no issue)

`headView.ts:190` sets `showHeadLine: !taskLevel`. In the resting state most
tasks sit in ("Waiting for review"), the head line is gone — and with it the
`Reconnecting` offline indicator (`LiveHead.tsx:62`), whose only host is that
line. The composer placeholder is then the sole disconnection signal.

The same block also spends ~190px of an 852px viewport on task-lifecycle chrome
that is not the conversation.

### 10. Dead space dominates the first screen — NOT DONE (deliberate; see below)

Short transcripts are bottom-anchored (`scrolling/scrolling.css:36`,
`margin-top: auto`), which is defensible on its own. Stacked under #9 the
opening screen of a new chat is ~80% header + attention block + black void, with
one centred line of placeholder text.

### 11. Turns do not visually chunk — NOT DONE (deliberate; see below)

`conversation.css:83` gives `.session-turn` `gap: 0`, and the thread's 16px gap
falls between turns as well as inside them. Ten turns of history read as one
undifferentiated column: no timestamps, no rule, no change in rhythm at a turn
boundary. Scrolling back to find "where did I ask about X" is a text hunt.

### 12. Presentation vocabulary is duplicated — FIXED (deleted with the head tool row)

`status/headView.ts:49-90` re-declares `TOOL_TONES`, `TOOL_MARKS`,
`TOOL_STATUS_LABELS`, `toolMark`, `toolStatusLabel`, `cleanTitle` and
`shortPath`, all of which already exist in `activity/presentation.ts`. Two
copies of the tone/mark tables is how the head and the transcript drift apart —
#6 is exactly that drift already happening.

---

## What is working well

- **Queued follow-up** (`QUEUED` label, Edit/Remove, "Press Enter again to stop
  and send now", placeholder that changes with state) is the clearest part of
  the surface, and the cancel handshake produced a correct `STOPPED` divider.
- **Markdown**: code blocks, tables and inline code all render legibly at 393px
  with no horizontal overflow of the column.
- **Permission and elicitation panels** are well-shaped — title, detail, real
  form controls from the JSON schema, three explicit actions.
- **Failures auto-expand** their disclosure and carry the red tone through the
  summary row.
- **Slash completion** looks and behaves correctly.
- **Replay after reconnect** did not duplicate rows.

---

## Suggested order of work

Each is independently shippable; 1–3 are what make it read as a conversation.

| # | Finding | Size |
| --- | --- | --- |
| 1 | Reveal completed interim messages | small |
| 2 | One owner for the live line | medium |
| 3 | Settle tool calls on `turn_end` | small |
| 5 | Stop double-reporting turn errors | small |
| 6 | `cleanTitle` in PermissionPanel | one line |
| 7 | Per-turn plan rows | small |
| 8 | `Jump to latest` whenever unpinned | small |
| 9 | Keep the head line under task attention | small |
| 11 | Turn separation | small (CSS) |
| 12 | Delete the duplicated tables in headView | small |

## Status 2026-08-22

Ten of twelve done. Defects filed and fixed with regression tests naming the
issue: #1042 (order), #1043 (hidden message), #1044 (stale tool call), #1045
(double error), #1046 (permission backticks), #1047 (shared plan).

Findings 2, 8, 9 and 12 were design/contract gaps rather than defects, fixed
without issues and recorded in `web-session-behavior.md`.

Deliberately not done:

- **10 (dead space)** — bottom-anchoring is what keeps the newest message near
  the thumb; the void is caused by the attention block above it, not the
  anchoring. Top-aligning would fix the screenshot and hurt the phone.
- **11 (turn separation)** — cosmetic, and "reads better" cannot be validated
  without Matt looking at it.
