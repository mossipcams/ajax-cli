# Ajax Web Session — behaviour and feel brief

Paste this as the standing brief for any work on the Ajax Web Session surface.
It defines the target. It is not an implementation plan; sequencing lives in
`session-quick-actions-ux.md`.

---

## The brief

You are designing the **Ajax Web Session**: the surface where an operator directs
coding agents from a phone, from the moment a task is created until its pull
request is merged. The host does the work; the phone directs it. Assume iOS
Safari, one thumb, and an operator who is not at a desk.

### The rule everything derives from

**One utterance in. One decision at a time out. Never type twice.**

Typing is the expensive act on a phone; reading is cheap. Every screen either
proposes a choice or accepts one spoken sentence. Any flow that asks the operator
to compose prose twice is a defect.

### Behaviour — what is on screen

Three fixed regions, each answering one question:

| Region | Answers | Behaviour |
| --- | --- | --- |
| **Now** — pinned, one line | What is it doing this second? | `Editing src/auth.rs` · `Running cargo test`. Never scrolls away. |
| **Timeline** — scrolling | How did it get here? | Strictly chronological. Every card collapsed to one line; tap to expand. |
| **Changed files** — persistent chip | What has it actually changed? | `12 files changed` opens the cumulative diff. |

Collapsed forms: reasoning → one-line summary · tool call → `⚙ cargo test — 3
failed` · file edit → `± src/auth.rs +12/−4` · decision → stays open, it is the
one thing allowed to interrupt.

The agent's state proposes the next moves. The operator recognises an action;
they never have to recall or compose one.

| State | Primary | Secondary |
| --- | --- | --- |
| starting | — | Cancel |
| `running` | Stop | Show diff so far |
| `waiting` · permission | Approve | Deny · Allow this session |
| `waiting` · question | Reply | Parsed choices when the question is closed-form |
| `settled` | Run tests | Show diff · Continue · Ship |
| `failed` | Retry | Show error · Stop |
| `review` ready | Open diff | Ship · Request changes |
| shipped, CI running | — | Open PR |
| CI green | **Merge** | Open PR |
| CI red | **Fix it** — sends the failure back as the next prompt | Open PR · Show failure |

Starting a task is one utterance: *"What do you want done?"* plus Mic. The title
is derived from it. Repo comes from context, agent from whichever has quota.
Overrides collapse behind one chevron.

### Feel — the qualitative bar

Aim for three sensations, in order:

1. **Oriented in two seconds.** Opening the app answers "is anything waiting on
   me?" before the operator reads a word. Status is carried by tone and position,
   not by prose.
2. **Actionable with one thumb.** Every action a running task needs sits in the
   bottom third. Nothing important lives in a corner or behind a long scroll.
3. **Closable without anxiety.** This is the one that matters most and is easiest
   to miss. Operators babysit agents because they fear missing something. The
   surface must make it *safe to stop looking*: "nothing needs you" must be
   communicated as clearly and as truthfully as "something needs you." Never show
   a confident state you cannot back with evidence — degrade to "unknown" instead.

It should feel like a **pilot's console or a good remote control**: immediate,
sparse, decisive, composed under load. Motion is short state feedback
(≈140–220ms), never page choreography.

It must never feel like:

- **a chat app** — the transcript is an audit trail, not a conversation to enjoy
- **a dashboard** — metrics the operator cannot act on are noise
- **an IDE on a phone** — panels and tabs fighting for a 390px viewport
- **a form** — anything that greets the operator with empty fields has failed

When in doubt, remove a control and let state imply it.

### Hard rules

- Terminal stays exactly one tap away as the escape hatch, never the default path.
- Free-form composer stays reachable, collapsed by default; Mic stays promoted.
- Actions derive from ACP state only — no per-vendor special-casing. Whatever is
  built must work identically for any conforming agent.
- Task truth stays on the host. The browser renders projections and submits typed
  intents; it never becomes a second source of truth.
- Follow `DESIGN.md`: one `--tone` status vocabulary, pill actions ≥44px, Soft
  Charcoal paper depth over shadows, no side-stripe status accents, respect
  `prefers-reduced-motion`.
- At most one primary and two secondary actions per card.

### Acceptance test

**Start a task by speaking one sentence, and take it to merged without typing
again.**

Run it on a real phone, one-handed, while walking. Every step that forces the
keyboard open is a defect in the design, not in the implementation. Count the
taps; if any state needs more than two to reach its obvious next move, that state
is wrong.

---

## Notes for use

- For implementation work, pair this with a bounded packet naming files and
  acceptance criteria — this brief sets the target, not the scope.
- Current surface lives in `crates/ajax-web/web/src/features/session/`.
- Known defects to fix before judging any design against this brief:
  `buildSessionFeed` does not sort chronologically, and `NewTaskSheet` captures
  no prompt.
