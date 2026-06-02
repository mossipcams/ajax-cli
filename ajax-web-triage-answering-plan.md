# Ajax Web Triage Answering Plan

Branch: `ajax/brainstorming`

**Status: implemented (P1–P5).** Per-agent Codex adapter + confidence floor
(`ajax-core::agent_prompt`), guarded `/answer` endpoint with stale-fingerprint
rejection, structured one-tap answering in the PWA with terminal escalation, the
free-form `/input` surface removed, and actionable approval push notifications.
A latent UTF-8 bug in pane `strip_ansi` (mangled the Codex `›` glyph) was fixed
along the way. Codex **approval** pane shapes (numbered / y-n) remain
fixture-synthetic — confirm against a live approval capture before trusting them
at high confidence; the composer shape is anchored to a real capture.

## Goal

Make the Web Cockpit able to **answer a blocked agent in one tap** without ever
becoming a terminal or a conversational driver. Web stays a **triage surface**:
it surfaces bounded decisions, routes a single guarded answer, and escalates
anything it can't safely structure to native/SSH.

This plan is scoped by three operator decisions:

1. **Web's role: triage-only.** No free-form input, no terminal, no open
   session. The unit of work is a bounded decision.
2. **Anchor pains: (a) can't answer a blocked agent from the phone, and
   (b) notifications are dumb** — they tell you something is wrong but you can't
   act from them.
3. **Structuring mechanism: per-agent adapters.** Parse a known agent's prompt
   into real choices; never screen-scrape generically into a button.

## Relationship to the shipped full-control surface

The full-control plan is **implemented end-to-end and live on main**. This plan
is a **refactor-in-place of a working system**, not a greenfield build. What
ships today:

- `tmux capture-pane` polling with a server-side monotonic `sequence`
  (`crates/ajax-core/src/slices/pane.rs`, `crates/ajax-web/src/slices/pane.rs`).
- `tmux send-keys` plumbing (`core/slices/pane.rs::send_keys`).
- `GET /api/tasks/{handle}/pane` + `POST /api/tasks/{handle}/input`.
- A full interact panel in `app.js`: activity log, command card with
  **Approve/Deny**, prompt card, and a **free-form input bar** with optimistic
  echoes.
- `PaneState { command, prompt }` DTO fields — **plumbed but stubbed to `None`**
  (`core/slices/pane.rs:199-200`).

Two consequences for this plan:

1. **Approve/Deny is a shipped bug, not a future risk.** `app.js:675-678` wires
   Approve → `sendInput("y", …)` and Deny → `sendInput("n", …)` literally. Codex's
   interactive approval is a numbered-select list, so `y` is the wrong key.
   Phase 1 *corrects live behavior*, it doesn't just fill a stub.
2. **Removal must not open a capability gap.** The free-form bar + `/input` are
   the conversational-driver surface this plan retires (Phase 5, decision B), but
   they are *working* today. The guarded structured answer path (Phase 2) must
   land **before** `/input` is removed, so there is never a window where a blocked
   agent can't be answered at all.

We keep the watching substrate and the `send-keys` primitive; the primitive
becomes reachable only through the guarded structured answer path.

## Out of scope (deliberately)

- Streaming / WebSocket transport. Polling is sufficient for triage.
- xterm.js / raw PTY broker.
- Free-form text or arbitrary key input from the browser.
- Offline mutation, IndexedDB.

---

## Phase 1 — Codex prompt adapter (core) — unblocks everything

Fill the `command`/`prompt` seam (`core/slices/pane.rs:199-200`, hardcoded
`None`) with structured, confidence-scored parsing keyed on `AgentClient`.

### Scope correction (grounded in captured panes)

There are **two Codex surfaces**, and only one is on our path:

- `crates/ajax-supervisor/src/agent/codex.rs` parses **JSON** from
  `codex exec --json` — the *supervised* one-shot flow. **Not reusable here.**
- Our pane path captures the **interactive Codex TUI** (alt-screen), which emits
  **no JSON**. The adapter is therefore a **pane-text parser** over the cleaned
  `lines: Vec<String>` from `core::slices::pane`. The only prior art is
  `live::looks_like_idle_codex_prompt`.

This also exposes a latent #105 bug: Codex's interactive approval is a
**numbered selection list**, not `y/n`, so #105's "Approve → send `y`" is wrong
for the real Codex UI. Per-agent answer mapping is the fix.

### Data structures

```rust
pub enum PromptKind { Approval, Choice, FreeText }
pub enum Confidence { High, Low }
pub enum OperatorAnswer { Approve, Deny, Select(u8) }
pub struct Choice { pub label: String, pub answer: OperatorAnswer }

pub struct AgentPrompt {
    pub kind: PromptKind,
    pub question: String,
    pub command: Option<String>,
    pub choices: Vec<Choice>,
    pub confidence: Confidence,
    pub fingerprint: String, // sha256 of prompt-relevant lines; Phase 2 guard
}

pub trait AgentPromptAdapter {
    fn parse(&self, lines: &[String]) -> Option<AgentPrompt>;
    fn answer_keys(&self, prompt: &AgentPrompt, answer: &OperatorAnswer)
        -> Result<SendKeys, AnswerError>;
}
```

Registry keyed on `AgentClient`. **Codex first.** Claude / Cursor return `None`
(→ safe escalation); captured reference panes for both live in
`tests/fixtures/_other/` and confirm their TUIs differ enough to require their
own adapters later.

### Pane shapes

**C. Free-text composer — CONFIRMED real** (`tests/fixtures/codex/composer_idle.txt`):

```
─ Worked for 7m 39s ────────────────────────────────────────────────
› Write tests for @filename
  gpt-5.4 high · ~/.ajax-dev/worktrees/ajax-cli-cbeb640c/release-please-attach…
```

- `› <text>` on an empty composer is Codex's **ghost placeholder**, not a real
  prompt — must not be parsed as a question or answer.
- Footer is `gpt-<model> <reasoning> · ~/<path>…` (the existing heuristic's
  `starts_with("gpt-") && contains("~/")` matches but is fragile vs. this `·`
  form — tighten it).
- → `AgentPrompt { kind: FreeText, question: <last assistant block above ›>,
  confidence: Low }`. Under decision B, `FreeText` is **always Low** and carries
  no answerable affordance — it drives the "Open in terminal" escalate chip only.

**A. Command approval (numbered select) — SHAPE TO CONFIRM** (no live approval
was available at capture time; this is the recognition target, not verified
glyphs):

```
  Allow Codex to run this command?
    cargo test --all-features
  ❯ 1. Yes, run it
    2. Yes, and don't ask again this session
    3. No, and tell Codex what to do differently
```

→ `Approval`, `command: Some("cargo test --all-features")`,
`choices: [Yes(Select 1), YesAlways(Select 2), No(Select 3)]`, `confidence: High`.

**B. Command approval (inline y/n) — SHAPE TO CONFIRM:**

```
  Run `cargo test`? [y/n]
```

→ `Approval`, `command: Some("cargo test")`, `choices: [Approve("y"), Deny("n")]`,
`confidence: High`.

### `answer_keys` mapping (the part per-agent gets right)

| Shape | Answer | Keys |
|---|---|---|
| A numbered | Approve | `"1"` + `Enter` |
| A numbered | Deny | `"3"` + `Enter` |
| B y/n | Approve | `"y"` + `Enter` |
| B y/n | Deny | `"n"` + `Enter` |
| C free-text | — | refuse (`AnswerError`) — escalate |

The operator's intent (`Approve`/`Deny`/`Select(n)`) is decoupled from keys; the
web layer never sees keystrokes.

### Confidence rules (the safe floor)

`High` only when **all** hold: (1) recognized approval header **and** a parsed
command line (A) or explicit `[y/n]` token (B); (2) every choice line parses to
number + label; (3) the prompt block is the **last** meaningful content (no
`looks_like_active_agent_status` line after it). Else `Low`/`None`.
`answer_keys` **refuses** any `Low`/`FreeText` prompt — defense in depth so a
misclassification can never emit a keystroke.

### Wiring + tests

- Wire into `classify_state` (`core/slices/pane.rs:184`): populate
  `command`/`prompt` **only at High confidence**; otherwise keep today's `None`.
  Generic `classify_pane` stays the floor.
- Fixtures in `crates/ajax-core/tests/fixtures/codex/`:
  `composer_idle.txt` (captured, real), plus `approval_numbered.txt`,
  `approval_yn.txt`, `redraw_garbled.txt` **still to capture from a live
  approval**.
- Tests: each fixture → expected `AgentPrompt`; `redraw_garbled` → `None`;
  `answer_keys` per the table; `answer_keys` on Low/FreeText → `AnswerError`.

### Phase 1 status

- ✅ Codex composer (FreeText) shape confirmed against a live session.
- ⏳ Codex approval (A/B) shapes unconfirmed — need a capture while a Codex task
  is actually at an approval prompt (or a throwaway scratch session driven to
  one). Do **not** trust approval parsing at High confidence until pinned.

## Phase 2 — Stale-answer guard (core + web) — depends on P1

The version check that makes a delayed answer safe. #105's `/input` sends
unconditionally; `sequence_hint` is only a client poll hint
(`web/slices/pane.rs:204`).

- Add `prompt_fingerprint` (hash of the prompt-relevant pane region) to the pane
  snapshot DTO.
- New guarded intent `POST /api/tasks/{handle}/answer`:
  `{ answer, fingerprint, request_id }`.
  Server re-snapshots → verifies fingerprint still matches → maps answer→keys via
  the P1 adapter → `send-keys`.
- **Mismatch → `409 Stale` → re-surface the current state. Never send.**
- `send-keys` is reachable **only** through this path, with keys produced by the
  adapter (not the operator). No arbitrary key bytes cross the wire.

## Phase 3 — "Waiting on you" as a first-class attention item — depends on P1

Triage means it shows in the list, not only after you open the task.

- Extend the cockpit projection / `TaskCard` so a High-confidence waiting prompt
  attaches a structured decision — mirroring how remediations already ride cards
  (`crates/ajax-web/src/action_vocabulary.rs:55-90`).
- Web cockpit slice + cards: Approve/Deny or prompt + choices inline in the
  triage list; one tap resolves via `/answer`.
- **Low confidence → an "Open in terminal" escalate chip** (today's
  `needs_terminal` behavior). No in-app free-form fallback.

## Phase 4 — Actionable notifications (pain b) — depends on P2 + P3

Blocked agent → push that carries the decision.

- Extend `crates/ajax-web/src/adapters/push.rs` so a newly-waiting attention item
  emits a notification with the structured prompt; the notification action calls
  `/answer` with the fingerprint. P2 makes the delayed answer safe — this is
  exactly where staleness would otherwise bite.
- **iOS baseline:** PWA notification actions are thin / no freeform, so the
  dependable path is **deep-link into the task's decision**; action buttons are
  progressive enhancement where the platform supports them. Routed in `sw.js`
  `notificationclick`.

## Phase 5 — Remove the free-form driver surface (decision: B)

Keep web triage-pure by removing the conversational-driver surface that ships
today. **The decision is made now; the code removal is the *last* step, gated on
Phase 2 shipping** — otherwise we strand blocked agents with no answer path.

- **Frontend:** remove the free-form input bar and its quick keys (Enter, Ctrl-C,
  arbitrary text) from the task detail interact panel
  (full-control plan F1/F3). What remains is the structured decision affordance
  (Approve/Deny/Choice) plus the "Open in terminal" escalate chip.
- **Backend:** remove `POST /api/tasks/{handle}/input` and its rate-limit /
  dedup state in the web slice. Keep `core::slices::pane::send_keys` as an
  internal primitive used only by the `/answer` adapter mapping.
- **Escalation:** low-confidence or open-ended prompts route the operator to
  native/SSH rather than offering free text. This is acceptable because the
  operator already works native; web's job is to clear the easy decisions and
  get out of the way.

---

## Sequencing

```
P1 (keystone) → (P2 ∥ P3) → P4 → P5 removal
                   │
   P5 decision ────┘  (locked now; shapes P3/P4 UI)
```

P1 is the keystone: the only source of structured prompts and the confidence
signal that gates every downstream affordance. The **decision** to remove the
free-form bar is locked now because it shapes the Phase 3/4 surface, but the
**removal of working code** (Phase 5) lands last — only once Phase 2 has shipped
the guarded structured answer that replaces it. No capability-gap window.

## Verification

Backend, per PR:

```sh
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
```

- P1: fixture-based adapter tests (recorded Codex transcripts; garbled → `None`).
- P2: stale-fingerprint rejection test; happy-path answer→keys test.
- Frontend: manual demo + screenshots (the codebase has no JS test rig).

## Open risks

- **Codex alt-screen redraw variance.** Mitigated by design: low fidelity yields
  `None` (escalate), never a wrong key. P1 must validate against a live Codex
  session before any prompt shape is trusted at High confidence.
- **iOS notification action limits.** Mitigated: deep-link is the baseline;
  inline actions are enhancement-only.
- **Removing `/input` is a capability regression for anyone using it as a
  terminal.** Accepted per decision B — web is triage-only; terminal work is
  native/SSH.
</content>
</invoke>
