# Plan: Ajax product direction

Mode: Planning-Only. Status: **draft, unapproved — for Matt's decision**.
Delegation decision: not delegated — broad product planning is on the
do-not-delegate list (`AGENTS.md` → Delegation).

## Scope

Answer two linked questions:

1. Is Ajax meant to have users beyond its author? (currently undecided)
2. What is worth building next? (currently unranked)

## Non-goals

- No code changes. No architecture changes.
- Not a commitment to ship any item below; this ranks candidates and supplies a
  decision rule.
- Does not revisit `PRODUCT.md` positioning — that document is sound.

## 1. Findings

### The direction slot is already occupied

| Signal | Value |
| --- | --- |
| Life | v0.55.1, 218 releases, ~2 months (2026-06-03 → 2026-08-05) |
| Changelog bullets by scope | **web 322**, core 23, cli 6, scripts 3, repair 2 |
| Changelog sections | **176 Bug Fixes** vs 42 Features (4.2 : 1) |
| Last 6 days of commits | 39 of 50 `web`-scoped; 30 `fix` / 10 `feat` |
| Roadmap or vision doc | none in repo |
| Shelved plans | 1 of 256 (`feat-cost-tracking.md`) |

~90% of everything ever shipped is Web Cockpit, at four fixes per feature. The
problem is not a shortage of ideas. There is no capacity to plan into.

### Two maintenance treadmills, one root cause

Ajax depends on two surfaces it does not control, and both generate unbounded
work:

1. **iOS Safari rendering.** `scrollOnErase` CSI chunk latching, page-swipe
   dead-zones, WASM load failures, double-tap-select vs. swipe conflicts,
   keyboard geometry. Apple sets the pace.
2. **Agent CLI terminal chrome.** `crates/ajax-core/src/pane_fallback.rs` is
   deliberately conservative string matching against bottom-anchored pane text
   ("Visible-pane text is weak evidence", `pane_fallback.rs:3`). Every vendor UI
   change can silently break it.

The differentiated asset — `ajax-core` task truth, 38k LOC of lifecycle,
reconciliation, receipts, ghost-task handling — sits on **git and tmux**, which
are stable. It is not on a treadmill, and it is being starved: 23 changelog
bullets against web's 322.

### The stated promise is not yet true

`crates/ajax-core/src/agent_capability.rs:85` declares per-agent coverage:

| Agent | PermissionWait | QuestionWait |
| --- | --- | --- |
| Claude | Native | Native |
| Codex | Native | **Unavailable** |
| Cursor | **Unavailable** | **Unavailable** |
| Pi | **Unavailable** | **Unavailable** |

Hooks are installed for all four (`crates/ajax-cli/src/agent_hooks.rs`), so this
is an upstream emission gap, not missing wiring. "Never miss an approval"
(`PRODUCT.md:17`) holds natively for one agent in four; the rest fall back to
pane scraping.

### The fleet cockpit does not rank the fleet

`crates/ajax-core/src/commands/projection.rs:113` sorts the inbox by `severity`,
then **alphabetically by task handle**. There is no dwell time, staleness, or
blocked-duration input. `recommended.rs::operator_action` reasons about one task
at a time. With ten waiting tasks the operator gets an arbitrary order — a list,
not a triage queue.

### Distribution is capped at one user, but instrumented for many

- No prebuilt binaries; release workflows are tag-only. Install is
  `cargo install` from a source clone.
- Access requires WireGuard plus a self-signed cert with manual iOS profile
  trust (`README.md:62`).
- `architecture.md:161` forbids a public-internet path by invariant.
- PostHog telemetry ships with a default write key (`architecture.md:167`).

Product analytics are pointed at an install base of one. That gap needs a
conscious call either way.

## 2. Decision rule

**Revised 2026-08-05** after Matt's input: *agent limits and intelligence are
changing weekly.* The first rule ("prefer substrates you control") was right but
too weak — it ranks by stability and misses direction of travel. Superseded by:

> **Prefer work whose value increases as agents improve. Discount work whose
> value depends on agents staying limited.**

Applied:

| Candidate | As agents improve | Verdict |
| --- | --- | --- |
| Agent knowledge as config, not code | churn accelerates | **Appreciates** |
| Diff review / judgment depth | more diffs, less human-written | **Appreciates** |
| Fleet triage / ranked inbox | more parallel tasks per operator | **Appreciates** |
| Rate-limit headroom as scheduling input | more agents contending | **Appreciates** |
| Lifecycle & reconciliation depth | stable (git, tmux) | Holds |
| Approval / wait detection parity | fewer approvals needed | **Depreciates** |
| Pane-scraped wait chrome | vendor churn, shrinking payoff | **Depreciates** |
| iOS Safari terminal fidelity | flat | **Cap it** |

The premise Ajax was built on — an operator catching interrupts — is a function
of agent *immaturity*. That value decays. What appreciates is deciding what
ships: more agents producing more changes that a human trusts less per unit.
The durable seat is coordination and trust, not interrupt handling.

### Structural consequence

`Config` (`crates/ajax-core/src/config.rs:206`) carries `repos`, `test_commands`,
`stt` — and **nothing about agents**, under `deny_unknown_fields`. Every agent
fact is compiled in:

- capability profiles: `const fn claude_profile()` … (`agent_capability.rs:85`)
- launch args, including a special-cased `AgentClient::Other if program ==
  "cursor"` (`adapters/agent.rs:21`)
- `AgentClient` is a closed enum (`models/intent.rs:19`)
- pane needles (`pane_fallback.rs`)

So 100% of Ajax's agent knowledge ships on a Rust release cycle, in a world
changing weekly. Under churn, the highest-leverage work is whatever lowers the
cost of adapting. That is now T1.1.

## 3. Ranked candidates

### Tier 1 — worth building under every audience outcome

**T1.1 — Move agent knowledge from code to config (medium).**
Add an agent manifest to `Config`: launch program and args, capability profile,
hook wiring, pane needles. Keep `AgentClient` for built-in defaults but let
config override and add clients without a recompile. This is the adaptation-cost
fix, and it delivers capability honesty for free — once profiles are data, the
UI can show what Ajax can and cannot detect per agent.
*Files: `config.rs:206`, `agent_capability.rs`, `adapters/agent.rs`,
`models/intent.rs:19`.*
*Under weekly churn this is the highest-leverage item in the repo: it converts
every future agent change from a release into an edit.*

**T1.2 — Deepen diff review and judgment (medium–large).**
`diff_review.rs` already has the right bones — `DiffFileRole`,
`classify_diff_path`, `DiffFlag`/`DiffFlagSeverity`, `assess_diff_judgment`.
This is the surface whose value rises fastest as agents improve: more changes,
each one trusted less per unit, arriving faster than a human can read them.
Invest in what makes a diff *safe to accept* — risk-weighted file roles, blast
radius, test-coverage signal, what changed since last look.

**T1.3 — Fleet triage (medium).**
Replace severity-then-alphabetical (`commands/projection.rs:113`) with real
ranking: dwell time, staleness, blocked duration. Appreciates directly with
parallelism — the smarter agents get, the more of them run at once.

**T1.4 — Rate-limit headroom as a scheduling input (new; small–medium).**
Today `RateLimited` is deliberately silenced as transient noise
(`attention/tests.rs:655`). If limits move weekly and many agents contend, limit
headroom is the binding constraint on fleet throughput, not an annoyance.
Minimum viable: surface per-agent limit state so "which agent has room for this
task right now" is answerable. Reframes T1.5 as well — the useful metric is burn
rate against a window, not cumulative tokens.

**T1.5 — Ship cost tracking (small–medium).**
The only shelved plan in 256; `feat-cost-tracking.md` is draft v2 with data
sources already probed on the host. Its instinct to exclude dollars ("prices
drift") is now clearly correct. Reframe around burn-rate-vs-window per T1.4.

**T1.6 — Declare a web terminal fidelity bar (policy, not code).**
Unchanged and still load-bearing. A budget, not a feature. Nothing above gets
capacity until this lands.

### Explicitly not worth further investment

**Approval/wait detection parity for Cursor and Pi.** Previously ranked first;
demoted. Chasing native-quality wait detection through pane scraping spends
effort on a shrinking problem: it depends on vendor chrome that churns weekly,
and on agents needing frequent approval, which is exactly what improving agents
stop doing. Surface the limitation (free, via T1.1) and stop there.

### Tier 2 — gated on the audience decision

**T2.1 — Distribution prerequisites.** Prebuilt binaries in the release
workflow, install docs, an auth model beyond WireGuard + self-signed certs.
Requires explicitly amending `architecture.md:161`. Only if "users" wins.

**T2.2 — Telemetry call.** Either T2.1 makes PostHog useful, or remove it.

## 4. Settling the audience question

Do not decide in the abstract — the decision has no new information behind it
today. Note that Tier 1 is exactly the overlap: each item makes Ajax measurably
better for its author **and** is a prerequisite for a second user.

Proposed sequence:

1. Land T1.4 (the cap), then T1.1 → T1.3.
2. Set a decision date at the end of that run, not now.
3. If leaning toward users, run one cheap test first: put it in front of 3–5
   people who run parallel agent fleets. If install friction is the only
   objection, that is a signal. If they don't want it with a clean install, that
   is also a signal — and nothing is wasted, because Tier 1 was worth building
   regardless.

## 5. Counter-arguments (recorded honestly)

**Personal-only weakens T1.3.** If the author never runs enough parallel tasks,
ranking may not pay off, and terminal quality is felt daily instead. Rebuttal:
the 4.2:1 fix ratio says terminal work is not converging.

**Weekly churn is an argument against building anything large.** A real reading:
under high environmental churn, optionality beats commitment, and the correct
move is to stay thin. This is why T1.1 is ranked first — it is precisely the
item that buys optionality rather than spending it. T1.2 is the one genuinely
large bet here, and it is defensible only because review load is the one thing
that rises under *every* scenario where agents improve.

**The premise itself could be decaying.** If agents get good enough to run
unattended and self-review, the operator layer thins. The honest read is that
the *interrupt* layer thins while the *trust* layer thickens — someone still
decides what ships. But that is a bet, not a fact, and it is the assumption most
worth revisiting if the next few months invalidate it.

## 6. Checklist

- [x] Inspect product, design, architecture docs
- [x] Measure shipped-work distribution
- [x] Identify capability and triage gaps against the stated promise
- [x] Produce a decision rule and ranked candidates
- [ ] Matt selects audience track and confirms/edits Tier 1 order
- [ ] Convert selected items into per-item implementation plans

## 7. Validation

Planning-only; no code changed and no build/test commands were run. Findings are
reproducible from the repo:

```bash
grep -oE "^\* \*\*[a-z]+" CHANGELOG.md | sed 's/^\* \*\*//' | sort | uniq -c | sort -rn
grep -oE "^### .*" CHANGELOG.md | sort | uniq -c | sort -rn
git log --pretty=%s | grep -oE "^[a-z]+" | sort | uniq -c | sort -rn
```

## 8. Risks

- Capping web terminal work (T1.6) is the load-bearing move; without it Tier 1
  will not get capacity and this plan changes nothing.
- T1.1 makes a reliability limitation visible. That is the point, but it will
  make Ajax feel weaker on Cursor/Pi before it feels stronger.
- T1.1 must not become a plugin framework. `AGENTS.md` forbids broad generic
  abstractions without concrete need — the concrete need is weekly agent churn,
  and the scope is a config-backed manifest, not an extension API.
- T1.5's data sources are third-party on-disk formats and can drift; the plan
  file already flags the cumulative-vs-incremental check for Codex.
- The ranking rests on the assumption that agents improve fast enough to shift
  load from approvals to review. Re-check that assumption quarterly; if approvals
  stay frequent, the demoted item comes back.
