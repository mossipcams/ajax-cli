# E4 — Frontend test audit (crates/ajax-web/web)

Read-only audit of the frontend test suites, slice E4 of the 15% tech-debt effort.
Scope: `crates/ajax-web/web` (43,326 tracked test lines measured; baseline doc says 44,246 —
measurement difference below). Goal: how many test lines are redundant while keeping every
behavior covered. NOT a proposal to weaken coverage; every removal names what still covers it.

Method: script `.planning/router-runs/tech-debt/ts-tests.mjs` extracted every `it()/test()` and
hook body (208 files, 1,995 tests, 191 hooks), normalized bodies (strings/numbers blanked,
whitespace collapsed), and grouped exact + near duplicates. Raw output:
`.planning/router-runs/tech-debt/ts-tests-out.json`. Findings below were then confirmed by hand.

Headline script numbers:
- `it.each` / table-driving usage: **15** across the whole suite (adoption is ~0%).
- Exact-duplicate normalized bodies: 42 groups, 96 members, ≈229 lines estimated waste.
- Cross-file near duplicates (Jaccard ≥ 0.82): 26 pairs, several at Jaccard 1.0
  (fully literal-identical) — these point at parallel implementations, not just verbose style.

Constraints from AGENTS.md / architecture.md Chat-first section: terminal-agent tasks, the
native TUI, and tmux verbs are **frozen** ("supported and tested, fixes only … No removal is
planned"). Tests that exist solely to keep frozen behavior passing are therefore NOT reduction
candidates; they are listed under "Not worth it" at the end.

Baseline line counts (tracked files):

```
$ git ls-files 'crates/ajax-web/web' | grep -E '\.(test|spec)\.' | xargs wc -l | tail -1
43,326 total
```

## Candidates (ranked by lines removed)

### E4-01 Chat status view: LiveHead.test.tsx duplicates headView.test.ts
- Paths: `src/features/chat/status/LiveHead.test.tsx`, `src/features/chat/status/headView.test.ts`
- (under investigation — two files test overlapping/mapped status logic; script found 3
  Jaccard-1.0 duplicate pairs between them, e.g. "prefers permission decision over agent status"
  and "maps waiting and error … to attention")
- Prod lines removed: TBD
- Test lines removed: TBD
- Evidence: pending hand confirmation
