# Plan: Session quick-action UX

Mode: Planning-Only. Status: **draft, unapproved**.
Delegation decision: not delegated — UX model design; delegate the waves once the
matrix below is approved.

## Problem

`AjaxWebSessionView` currently carries two competing interaction models.

**State-driven intent — works.** `SessionAttentionBanner.tsx` renders actions
derived from agent state: `permission` → Approve / Deny, `question` → Reply,
`failed` → Retry / Stop, `review` → Open. The operator recognises the action; it
is never recalled or composed. This needs no per-agent tuning and no learning.

**Character entry — does not.** `SessionComposerKeys.tsx` renders `Esc`, `Tab`,
`←↑↓→`, `⌫`, `Ctrl` (armed, 4s timeout), `Paste`, `Mic`, with hold-to-repeat.
These are terminal keys: a soft keyboard rebuilt inside the surface whose purpose
is to replace the terminal. The component name says *Keys*, not *Actions*.

A composer cannot be made intuitive, because it has no semantics. It expresses
anything, so it guides nothing, and every affordance added to it is a guess about
what the operator might type. That is why this UX feels hard — the effort is
going into the one element that cannot repay it.

## The rule

> **The agent's state proposes the next moves. Typing is the escape hatch.**

Recognition over recall. On a phone, reading three buttons is nearly free and
typing a sentence one-handed is expensive, so the surface must propose rather
than await. The composer becomes to the session what the terminal is to the
product: one tap away, never the default path.

## Scope

- Define the state → actions matrix below as the session's primary surface.
- Move quick actions onto transcript cards where they are contextual.
- Demote `SessionComposerKeys` from primary strip to collapsed escape hatch.

## Non-goals

- Removing the composer or the terminal escape. Both stay.
- Removing `SessionComposerKeys`. Free-form and key input remain reachable.
- Per-agent action tuning. Actions derive from ACP state, so they must work for
  any conforming agent.
- New backend capability. Everything below maps to states #775 already models.

## State → actions matrix

Primary action first; it is the one a thumb reaches without looking.

| Agent state | Primary | Secondary | Notes |
| --- | --- | --- | --- |
| `running` | Stop | Show diff so far | Watching is the default; do not demand input |
| `waiting` · permission | Approve | Deny · Allow for this session | Already built; keep verbatim |
| `waiting` · question | Reply | Suggested answers when closed-form | Offer parsed choices before the text box |
| `settled` (turn done) | Run tests | Show diff · Continue · Ship | The highest-value screen; currently the emptiest |
| `failed` | Retry | Show error · Stop | Already built |
| `review` ready | Open diff | Ship · Request changes | Bridges session into the existing ship path |
| `idle` / no session | Start task | Resume last | Entry point |

`settled` is the gap that matters. When a turn ends, the operator today faces a
blank composer — maximum ambiguity at the exact moment a small set of moves is
obvious. This is where "quick actions direct an agent" is won or lost.

## Placement

1. **Card-level actions.** `renderMessage.tsx` / `sessionCards.ts` already render
   structured tool and diff cards. Actions belong on the card they concern — a
   diff card carries Apply / Revert / Explain; a tool-call card carries Approve /
   Skip. Contextual beats a global toolbar, and it scales as card kinds grow.
2. **One action bar above the composer.** Holds the current state's primary and
   secondaries from the matrix. It replaces the key strip as the resting surface.
3. **Composer collapsed by default.** Tap to expand for free-form. `Esc`/`Tab`/
   arrows/`Ctrl` move inside the expanded composer, where they make sense,
   instead of occupying the primary strip.
4. **Mic stays promoted.** Speech is the cheapest way to express free-form intent
   on a phone and is the right partner to a proposed-action UI.

## Why this also delivers cross-vendor

Actions derive from ACP state, which is uniform across conforming agents. A
state-driven surface gets the same UX on every ACP agent for free, whereas a
composer-plus-keys surface has to be tuned to each agent's text conventions.
`PRODUCT.md` principle 3 ("every harness is a peer, over one protocol") is
delivered by this design rather than merely asserted by it.

## Task checklist

- [ ] Approve the matrix; adjust labels to operator vocabulary
- [ ] Wave 1 — action bar driven by session state; `settled` row first
- [ ] Wave 2 — card-level actions on diff and tool cards
- [ ] Wave 3 — collapse composer + key strip behind expand; keep Mic promoted
- [ ] Wave 4 — suggested answers for closed-form questions

## Validation

```bash
cd crates/ajax-web/web && npm test -- --run src/features/session
npm run verify:slice -- operate
```

Per-wave focused vitest first. The real test is manual and on a phone: from a
locked iPhone, take a settled task to shipped without opening the composer once.
If that is impossible, the matrix is wrong, not the implementation.

## Risks

- The matrix hard-codes an opinion about what operators want next. It is drawn
  from Ajax's own verbs (`resume`, `review`, `ship`, `repair`, `drop`), so it
  should be checked against real usage before Wave 2 widens it.
- Card-level actions can re-create the "overbuilt IDE shell" anti-reference if
  every card sprouts a button row. Cap it: at most one primary plus two
  secondaries per card.
- `settled` actions like Run tests depend on a configured `[[test_commands]]`
  entry for the repo; degrade to hidden, not broken, when absent.
