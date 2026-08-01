# Tech debt review: 0.56.0 release surface (PR #668)

## Delegation decision

`Delegation decision: delegated via model-router` (Cursor / composer-2.5; GLM usage-limited)

## Wave 1 — P0 (done)

| Packet | Result |
| --- | --- |
| status reconcile client coverage | ACCEPT |
| terminal WS transport races | ACCEPT + parent close-guard |
| Diff stale PR load race | ACCEPT |
| Diff readonly GETs | ACCEPT |

## Wave 2 — P1 (done)

| Packet | Result |
| --- | --- |
| `fix-claude-idle-prompt-not-done` | ACCEPT — idle_prompt → AttentionRequested(Question) |
| `fix-diff-hybrid-fallback-signal` | ACCEPT — `fell_back_from_pr` + banner |
| `chore-delete-dashboard-muster-scrap` | ACCEPT — Muster helpers/CSS deleted |
| `fix-terminal-link-reject-userinfo` | ACCEPT (parent) — code+tests green; transaction `after_delegate` failed (`git diff /dev/null`) |

## Still open (later waves)

- [ ] Status: PaneEvidence through reducer; delegated-run IDs / stop advertising
- [ ] Status: Cursor `ElicitationResult` → TurnStarted thrash
- [ ] Diff: duplicate `gh pr list`; brittle gh string errors; hunk parse gaps
- [ ] Terminal: dead SerializeAddon/snapshot; dual link paths; seed reset ordering
- [ ] Dashboard: `attention` wire vs TS; web `inbox` DTO; sort-authority comment
- [ ] Ponytail: FloatingContextMenu / @floating-ui; Confidence always-High; dual ActivityKind

## Validation (wave 2)

- `cargo test -p ajax-cli --lib agent_event` → pass (via transaction)
- `cargo test -p ajax-core --lib diff_review` → 11 passed
- `npm run web:test -- --run DiffReview` → 12 passed
- `npm run web:test -- --run state TaskList` → 32 passed
- `npm run web:test -- --run terminalLinkService` → 12 passed
