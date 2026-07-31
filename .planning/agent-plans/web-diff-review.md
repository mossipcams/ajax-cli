# Web Diff Review (vibe-coding centered)

Mode: Architecture Change → Behavior Change (approved for immediate implementation).
Status: in progress.

## Product decisions (accepted)

1. Entry: swipe left on **task detail** → Diff Review. Dashboard one-tap unchanged.
2. Diff source: **hybrid** — GitHub PR file diffs when available; else local `base...HEAD`.
3. MVP: **read-only** PR picker + file list + mobile hunk viewer + Open on GitHub.

## Delegation decision

`Delegation decision: not delegated because` the approved plan exceeds one bounded
behavior (multi-slice architecture + API + mobile UI). Parent implements as one
coherent change set under the approved plan; model-router `R-STOP` for
multi-bounded work.

## Scope

- Core: GitHub PR association + structured diff projection + metadata PR refs.
- ajax-web: read-only `pull-requests` / `diff` task subroutes.
- Web shell: `#/t/<handle>/diff`, DiffReview UI, task-detail swipe-left.

## Non-goals

- Approve / request-changes / inline comments.
- AI review notes.
- Dashboard swipe-reveal restore.
- Browser-owned PR/task truth.

## Task checklist

- [x] Confirm product defaults
- [x] Slice 0 — architecture.md + this ledger
- [x] Slice 1 — core PR/diff projection (TDD)
- [x] Slice 2 — web API routes
- [x] Slice 3 — hash route + DiffReview shell
- [x] Slice 4 — task-detail swipe navigation
- [x] Slice 5 — file list + hunk viewer

## Validation

```bash
cargo test -p ajax-core --lib diff_review   # 7 passed
cargo test -p ajax-web --lib diff_review    # 4 passed
npm run web:test -- --run routes DiffReview navigateSwipe TaskDetail  # 47 passed
npm run web:check  # passed
npm run web:smoke -- e2e/diff-review.test.ts  # 1 passed (mobile-webkit)
```

Status: checklist complete.

## Deviations

- Slice 1: `local_diff` uses hardcoded `"git"` program (not `GithubChecksAdapter.program`) since it runs git, not gh.
- Validation used `cargo test` (not nextest) per slice handoff where noted.
- Ship/create-PR path does not yet write `ajax_pull_requests` at merge time; Diff Review observation persists live `gh pr list` into task metadata on read (covers history while the head branch still lists). Explicit ship-time append remains a follow-up if heads disappear before Diff Review is opened.
- Playwright Diff Review smoke uses in-page touch dispatch (same pattern as swipe-reveal), mobile-webkit only.
