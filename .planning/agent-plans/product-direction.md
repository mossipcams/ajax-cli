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

**Revised twice on 2026-08-05.** First rule ("prefer substrates you control")
ranked by stability and missed direction of travel. Second rule ("prefer work
whose value increases as agents improve") was closer but still framed the
harnesses as a *dependency*. Matt's clarification — *AI harnesses and models
change weekly* — makes the real relationship clear: the layer below Ajax is not
merely churning, it is **expanding into Ajax's territory**. Final rule:

> **Bet only on what is cross-vendor or host-level. Assume anything a single
> harness can ship for itself will be absorbed within ~2 quarters.**

### Why absorption is the governing force

Harnesses ship weekly and are themselves becoming orchestrators: background
tasks, subagents, session resume, checkpoints, native notifications, per-session
cost display. Every Ajax capability that mirrors a *single-harness* feature is on
a countdown.

The irony is already visible in the code: Ajax's attention detection is *built
on* harness hooks (`crates/ajax-cli/src/agent_hooks.rs` installs Claude, Codex,
Cursor and Pi hooks). Ajax is downstream of the exact layer expanding into it.

### What cannot be absorbed

1. **Cross-vendor.** No vendor will orchestrate competitors well. Anthropic has
   no incentive to make Claude Code a good manager of Codex and Cursor tasks.
   That is structural, not a gap someone closes next quarter.
2. **Host-level truth.** A harness knows its own session. It does not know you
   have twelve worktrees across four repos driven by three vendors. `ajax-core`
   reconciles against git and tmux, which no harness sees.
3. **The operator's own lifecycle.** Review, ship, drop, tidy — the git workflow
   *around* agent output. Harnesses produce diffs; they do not run your merge
   queue across repos.

Run the filter:

| Candidate | Cross-vendor / host-level? | Verdict |
| --- | --- | --- |
| Open the agent set (config, not enum) | It *is* the breadth | **Existential** |
| Cross-vendor normalized task truth | Yes | **Core moat** |
| Review / ship lifecycle across repos | Yes — host-level git | **Durable** |
| Fleet triage across vendors | Yes | **Durable** |
| Cross-vendor limit/cost rollup | Yes, only in rollup form | **Durable** |
| Per-agent approval detection | No — harness ships it | **Absorbed** |
| Per-task token display | No — harnesses show natively | **Absorbed** |
| iOS Safari terminal fidelity | No | **Cap it** |

The second rule's conclusion survives inside this one and is worth stating
directly, because it is the part that indicts `PRODUCT.md`. The premise Ajax was
built on — an operator catching interrupts — is a function of agent
*immaturity*, so that value decays on its own even before absorption. What
grows is deciding what ships: more agents producing more changes that a human
trusts less per unit. **The durable seat is coordination and trust, not
interrupt handling.**

### Structural consequence: the closed enum is now existential

`Config` (`crates/ajax-core/src/config.rs:206`) carries `repos`, `test_commands`,
`stt` — and **nothing about agents**, under `deny_unknown_fields`. Every agent
fact is compiled in:

- capability profiles: `const fn claude_profile()` … (`agent_capability.rs:85`)
- launch args, including a special-cased `AgentClient::Other if program ==
  "cursor"` (`adapters/agent.rs:21`)
- `AgentClient` is a closed enum (`models/intent.rs:19`)
- pane needles (`pane_fallback.rs`)

Previously filed as an ergonomics problem. Under the absorption rule it is the
central one: **cross-vendor breadth is the moat, and breadth is gated behind a
Rust release.**

This is not hypothetical — the wall has already been hit. OpenCode appears
throughout `.planning/`, and
`.planning/agent-plans/web-terminal-scroll-yank-and-opencode.md` records the
cost: adding it required a code change and shipped asymmetrically — *"No
CLI/TUI agent-picker changes; web only for opencode."* One new harness produced
a code change plus surface drift between Web Cockpit and CLI/TUI.

If new harnesses keep appearing and each costs a release and widens that drift,
Ajax loses the cross-vendor race precisely when breadth is the whole point.

## 3. Ranked candidates

### Tier 1 — worth building under every audience outcome

**T1.1 — Open the agent set: config-driven, not a closed enum (medium).**
Add an agent manifest to `Config`: launch program and args, capability profile,
hook wiring, pane needles. Keep built-in defaults, but a new harness must be
addable by editing config — no recompile, no surface drift. Target: adopting the
next OpenCode-equivalent is a config edit that lands in Web, CLI, and TUI at
once. Capability honesty falls out free — once profiles are data, the UI can
show what Ajax can and cannot detect per agent.
*Files: `config.rs:206`, `agent_capability.rs`, `adapters/agent.rs`,
`models/intent.rs:19`.*
*This is the moat. Everything else in Tier 1 is worth less if breadth stays
gated behind a release.*

**T1.2 — Deepen diff review and judgment (medium–large).**
`diff_review.rs` already has the right bones — `DiffFileRole`,
`classify_diff_path`, `DiffFlag`/`DiffFlagSeverity`, `assess_diff_judgment`.
Host-level and cross-vendor by nature: it operates on git output, not on any
harness's internals. Invest in what makes a diff *safe to accept* — risk-weighted
file roles, blast radius, test-coverage signal, what changed since last look.
The volume of changes needing this judgment rises under every scenario where
harnesses improve.

**T1.3 — Fleet triage across vendors (medium).**
Replace severity-then-alphabetical (`commands/projection.rs:113`) with real
ranking: dwell time, staleness, blocked duration. Value is specifically the
*cross-vendor* view — one queue over Claude, Codex, Cursor, OpenCode work. No
harness will ever render that queue.

**T1.4 — Cross-vendor limit and burn rollup (new; small–medium).**
Merges the old rate-limit and cost items, because under the absorption rule they
have the same durable core. Per-vendor token counts and limit banners are
absorbed — every harness shows its own. What no harness shows is the rollup:
*across all vendors I'm running, where is the headroom right now.* Today
`RateLimited` is deliberately silenced as transient noise
(`attention/tests.rs:655`); revisit that if limit headroom is the binding
constraint on fleet throughput.
Note `feat-cost-tracking.md` (draft v2, the only shelved plan in 256) already
scoped the cross-agent rollup and already excluded dollars because "prices
drift" — that instinct now looks clearly right. Build the rollup, skip the
per-task display.

**T1.5 — Declare a web terminal fidelity bar (policy, not code).**
Unchanged and still load-bearing. A budget, not a feature. Nothing above gets
capacity until this lands.

### Explicitly not worth further investment

**Approval/wait detection parity for Cursor and Pi.** Previously ranked first;
now doubly demoted. It depends on vendor chrome that churns weekly, on agents
staying limited enough to need frequent approval, *and* it is the single most
likely capability for harnesses to ship natively — Claude and Codex already do,
which is where Ajax's own hooks come from. Surface the limitation (free, via
T1.1) and stop.

**Per-task token/cost display.** Absorbed. Keep only the cross-vendor rollup
(T1.4).

### Tier 2 — gated on the audience decision

**T2.1 — Distribution prerequisites.** Prebuilt binaries in the release
workflow, install docs, an auth model beyond WireGuard + self-signed certs.
Requires explicitly amending `architecture.md:161`. Only if "users" wins.

**T2.2 — Telemetry call.** Either T2.1 makes PostHog useful, or remove it.

## 4. Settling the audience question

The absorption rule mostly answers this, which the earlier drafts could not.

If the only durable seat is cross-vendor and host-level, then the audience is
not "developers using AI agents" — most of them use one harness, and for them
the harness is already enough and getting better weekly. The audience is
specifically:

> **Operators running two or more agent harnesses in parallel, across multiple
> repos, on their own machine.**

That group is small today, structurally unserveable by any vendor, and — this is
the part that matters — **grows as harnesses proliferate**. Every new entrant
adds people whose fleet spans vendors. Ajax's addressable audience is a function
of exactly the churn that threatens it.

This also reframes the personal-vs-product question. Matt is already in that
group (Claude, Codex, Cursor, Pi, OpenCode). Building for himself and building
for the audience are, for once, the same act — provided Tier 1 is built
cross-vendor-first rather than for whichever harness he used most that week.

Proposed sequence:

1. Land T1.5 (the cap), then T1.1 — the moat — then T1.2 → T1.4.
2. Revisit distribution only after T1.1. Breadth gated behind a Rust release is
   not distributable regardless of install path.
3. Then run one cheap test: 3–5 people who run multiple harnesses in parallel.
   Nothing is wasted either way, because Tier 1 is worth building for one user.

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
decides what ships. But that is a bet, not a fact.

**The cross-vendor audience may stay too small.** The strongest objection to
this whole plan. If most operators consolidate onto one harness — plausible if
one pulls decisively ahead — the durable seat shrinks to near nothing, and Ajax
is a personal tool by default rather than by choice. This is the assumption to
re-check quarterly; it is load-bearing for everything in §4.

**A harness could go cross-vendor.** Less likely from a model vendor, but an
independent harness (an OpenCode-class project) has every incentive to. If one
does it well, Ajax's moat is contested by something with more surface area.
Ajax's counter is host-level truth — git and tmux reconciliation across repos —
which is further from any harness's centre of gravity than agent orchestration
is.

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

- Capping web terminal work (T1.5) is the load-bearing move; without it Tier 1
  will not get capacity and this plan changes nothing.
- T1.1 makes a reliability limitation visible. That is the point, but it will
  make Ajax feel weaker on Cursor/Pi before it feels stronger.
- T1.1 must not become a plugin framework. `AGENTS.md` forbids broad generic
  abstractions without concrete need — the concrete need is weekly agent churn,
  and the scope is a config-backed manifest, not an extension API.
- T1.4's data sources are third-party on-disk formats and can drift; the plan
  file already flags the cumulative-vs-incremental check for Codex.
- The ranking rests on the assumption that agents improve fast enough to shift
  load from approvals to review. Re-check that assumption quarterly; if approvals
  stay frequent, the demoted item comes back.
