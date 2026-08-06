# Plan: Session UX, task creation → PR merge

Mode: Planning-Only. Status: **draft, unapproved**.
Delegation decision: not delegated — journey design; delegate the waves once the
shape below is approved.

## The through-line

> **One utterance in. One decision at a time out. Never type twice.**

Text entry is the expensive act on a phone and reading is cheap, so every stage
should either propose a choice or accept a single spoken sentence. Anything that
asks the operator to compose prose twice is a design failure.

## Two defects to fix before any design work

**1. Task creation captures no prompt.** `NewTaskSheet.tsx` holds `repo`,
`title`, `agent` — grep for `prompt` returns zero. The operator fills a
three-field form, the task starts, and the actual instruction is typed again
into the session composer. Two text-entry moments, the first of which names a
thing before it exists.

**2. The feed is not ordered.** `sessionCards.ts::buildSessionFeed` maps
messages to cards, then `push`es tool progress, then file progress, then
decisions. There is no timestamp field and no `sort` anywhere in the file. Tool
calls and file edits therefore accumulate at the bottom regardless of when they
happened, detached from the reasoning that produced them. On a phone showing one
or two cards at a time this is disorienting, and no layout design survives it.

Fix 2 first; it is a bug, not a preference.

## Stage 1 — Starting: the prompt *is* the task

Replace the three-field form with a single utterance.

- One field: **"What do you want done?"** with Mic promoted beside it. Speaking a
  sentence is the cheapest input on a phone and the natural way to start work.
- **Title is derived, never typed** — first clause of the prompt, or generated.
- **Repo defaults** from current project context; **agent defaults** to whichever
  has quota headroom (see `product-direction.md` step 6).
- Repo / agent / branch overrides collapse behind one chevron for the minority of
  starts that need them.
- Submit sends the prompt as the first turn. There is no gap between "task
  exists" and "agent knows what to do".

Result: one utterance starts work, and the first thing on screen is the agent
already working rather than an empty composer.

## Stage 2 — Reading: three regions, not one log

An agent turn is a firehose — reasoning, tool calls, file edits, test output. A
chat log is the wrong shape: it scrolls forever and answers no question quickly.
Split the surface by the three questions an operator actually asks.

| Region | Question it answers | Behaviour |
| --- | --- | --- |
| **Now** (pinned, one line) | *What is it doing this second?* | `Editing src/auth.rs` · `Running cargo test`. Glanceable, never scrolls away. |
| **Timeline** (scroll) | *How did it get here?* | Chronological. Every card **collapsed to one line** by default; tap to expand. |
| **Changed files** (persistent chip) | *What has it actually changed?* | `12 files changed` opens the cumulative diff. |

**Collapse by default is the core move.** A phone fits one or two expanded cards
but roughly fifteen collapsed lines. Density is comprehension. Collapsed forms:

- reasoning → one-line summary
- tool call → `⚙ cargo test — 3 failed`
- file edit → `± src/auth.rs +12/−4`
- decision → stays expanded; it is the exception that must interrupt

The changed-files chip matters most for the merge end of the journey. In a chat
log the diff is scattered across forty cards; a persistent cumulative view is
what makes reviewing on a phone possible at all.

## Stage 3 — Acting: three tiers by urgency

| Tier | Trigger | Placement | Status |
| --- | --- | --- | --- |
| **Blocking** | permission, question | Pinned banner, interrupts | Built — `SessionAttentionBanner` |
| **Proposed** | turn settles | Action bar above composer | Missing |
| **Contextual** | a specific card | On the card itself | Missing |

State → proposed actions:

| State | Primary | Secondary |
| --- | --- | --- |
| `running` | Stop | Show diff so far |
| `waiting` · permission | Approve | Deny · Allow this session |
| `waiting` · question | Reply | Parsed choices when closed-form |
| `settled` | Run tests | Show diff · Continue · Ship |
| `failed` | Retry | Show error · Stop |

`settled` is the emptiest screen in the product today and the one where "quick
actions direct an agent" is won or lost: a turn ends and the operator faces a
blank composer at the exact moment the useful moves are obvious.

Contextual examples: a diff card carries Apply / Revert / Explain; a tool card
carries Approve / Skip. Cap at one primary plus two secondaries per card, or
this becomes the "overbuilt IDE shell" anti-reference.

## Stage 4 — The last mile: ship → CI → merge

`crates/ajax-core/src/commands/merge.rs` already has `merge_task_plan` with
preflight blocking, and CI evidence already arrives via `gh`. None of it is
reachable from the session, so the journey dead-ends at ship and the operator
leaves Ajax to merge. Extend the state machine to the end:

| State | Primary | Secondary |
| --- | --- | --- |
| `review` ready | Open diff | Ship · Request changes |
| shipped, CI running | — (watch) | Open PR |
| **CI green** | **Merge** | Open PR |
| **CI red** | **Fix it** | Open PR · Show failure |

**Fix it** is the highest-value action in the whole flow: one tap sends the CI
failure back to the agent as the next prompt. The loop closes without the
operator typing anything, which is the entire thesis in one button.

## Task checklist

- [ ] Approve the shape; adjust labels to operator vocabulary
- [ ] Wave 0 — order `buildSessionFeed` chronologically (bug)
- [ ] Wave 1 — prompt-is-the-task creation; derive title; collapse overrides
- [ ] Wave 2 — three regions: Now band, collapsed timeline, changed-files chip
- [ ] Wave 3 — proposed action bar, `settled` row first
- [ ] Wave 4 — contextual actions on diff and tool cards
- [ ] Wave 5 — ship → CI → merge states, including **Fix it**

## Validation

```bash
cd crates/ajax-web/web && npm test -- --run src/features/session src/features/task
npm run verify:slice -- operate
```

The real test is manual and on a phone: **start a task by speaking one sentence,
and take it to merged without typing again.** Every step that forces the keyboard
open is a defect in this design, not in its implementation.

## Risks

- Deriving titles from prompts changes task handles, which appear in worktree
  paths and tmux session names. Check `start` naming before Wave 1.
- Collapse-by-default can hide the one line that mattered. Keep expansion sticky
  per card kind so an operator who always opens diffs stops re-tapping.
- **Fix it** re-prompts an agent whose session may have settled or been dropped;
  it must degrade to "open PR" rather than silently failing.
- Wave 5 assumes `gh` is authenticated in the worktree; README already documents
  that Ajax skips CI evidence when it is not. Merge must stay hidden, not broken.
