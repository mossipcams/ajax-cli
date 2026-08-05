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

The filter that rules candidates out:

> **Prefer work that stands on substrates you control. Discount work that
> depends on surfaces you don't.**

Applied:

| Candidate | Depends on | Verdict |
| --- | --- | --- |
| Fleet triage / ranked inbox | Ajax task truth | **Compounds** |
| Lifecycle & reconciliation depth | git, tmux | **Compounds** |
| Cost tracking | agent session JSONL on disk | Medium — semi-stable |
| iOS Safari terminal fidelity | Apple | **Cap it** |
| Pane-scraped wait detection | vendor CLI chrome | **Cap it** |

This rule is the actual deliverable. It survives the audience decision.

## 3. Ranked candidates

### Tier 1 — worth building under every audience outcome

**T1.1 — Surface capability honesty (small).**
The capability matrix exists in code but never reaches the operator. Show, per
task, whether Ajax can natively detect an approval wait for that agent. Telling
the operator "Ajax cannot detect approval waits for Cursor" is more valuable —
and far cheaper — than a brittle scraper that fails silently. Converts an
invisible reliability hole into a known limitation.
*Files: `agent_capability.rs` (read), web/TUI task detail projection.*

**T1.2 — Fleet triage (medium).**
Replace severity-then-alphabetical with real ranking: dwell time in current
status, staleness, blocked duration. This is the "operator of a fleet" promise,
and it gets more valuable the more parallel tasks run — which is the author's
own use case and the one a second user would hit immediately.
*Files: `commands/projection.rs:108`, `recommended.rs`.*

**T1.3 — Ship cost tracking (small–medium).**
The only shelved plan in 256. `feat-cost-tracking.md` is draft v2 with data
sources already probed and verified on the host. Tokens per task via read-time
scan of Claude/Codex session JSONL; no schema change, no supervisor change.
Answers "which of my twelve parallel tasks burned the rate limit" — a question
that exists only because Ajax works.

**T1.4 — Declare a web terminal fidelity bar (policy, not code).**
Not a feature: a budget. Write down which iOS Safari behaviors are supported and
close the rest won't-fix. Nothing in Tier 1 gets built until this frees capacity.

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

## 5. Counter-argument (recorded honestly)

If Ajax is genuinely personal-only, T1.2 matters less: the author feels terminal
quality every day and may never run enough parallel tasks for ranking to pay
off. The rebuttal is the 4.2:1 fix ratio — terminal work is not converging, so
more of it is unlikely to be the highest-value use of the next cycle.

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

- Capping web terminal work (T1.4) is the load-bearing move; without it Tier 1
  will not get capacity and this plan changes nothing.
- T1.1 makes a reliability limitation visible. That is the point, but it will
  make Ajax feel weaker on Cursor/Pi before it feels stronger.
- T1.3's data sources are third-party on-disk formats and can drift; the plan
  file already flags the cumulative-vs-incremental check for Codex.
