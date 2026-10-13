# PR #1264 — Review & Simplify (Claude Agent SDK + Pi RPC)

Goal: reliable native Claude Agent SDK and Pi RPC integrations with the smallest
maintainable implementation, compared against the existing ACP path. Preserve
architecture where justified; do not rewrite for line count.

Approval: user explicitly requested this review-and-implement effort on 2026-10-10
("Review and simplify ... before merging"), which authorizes implementation
rounds within the stated scope without per-round approval.

## Scope / non-goals

Scope (per user request):
1. Remove PR bloat: `.planning/packets/` + `.planning/router-runs/` artifacts out
   of the PR without destroying local copies; drop temp transcripts/diagnostic
   outputs/dev-only artifacts; retain useful architecture docs; remove redundant
   tests/fixtures without sacrificing meaningful coverage.
2. Architecture audit of `JsonlProcess`, `RpcSession`, `RpcClientCore`,
   `PiRpcClient`, `ClaudeSdkClient`, `SessionClient`: one responsibility per
   abstraction, consolidate real duplication, delete unnecessary state /
   forwarding methods / indirection / speculative flexibility; keep the shared
   RPC core if it materially reduces duplication; no new frameworks or generic
   abstractions without demonstrated need.
3. Correctness fixes vs ACP: mutex contention during blocking RPC + event
   polling; cancellation while another op waits; lifecycle/process cleanup;
   request/response correlation and event ordering; mid-session model/config
   changes; restore behavior + session-id preservation; permission/elicitation
   handling; tool-call streaming/completion; images/content replaced by
   placeholders. Preserve existing ACP functionality; no scope expansion.
4. Validate spawn/init, streaming+tool calls, cancel+next prompt, model
   selection, restore after restart, error/cleanup — with existing test infra;
   add focused regression tests only for actual failure modes.

Non-goals: task durability / tmux replacement / persistent process supervision
(separate PR); any change to Cursor/Codex ACP path (must stay unchanged); Ajax
web protocol or UI rewrites; renaming ACP internal event types; unrelated
refactors; LOC targets over correctness.

## Baseline metrics (2026-10-10, before any simplify commit)

vs `origin/main` merge base, HEAD `21d88024`:

| Bucket | Files | +lines | -lines |
|---|---|---|---|
| Whole PR | 221 | 18,288 | 89 |
| `.planning/` artifacts (PR bloat) | 153 | 8,605 | 0 |
| `crates/` total | 64 | 9,556 | 59 |
| — production .rs (non-test) | — | 3,588 | 33 |
| — tests + fixtures (.rs/.js/.mjs/.jsonl) | — | 5,968 | 26 |
| docs/README/architecture.md | 4 | ~140 | ~26 (part of the 87) |

Largest production additions: `crates/ajax-web/sidecar/claude_sdk_sidecar.mjs`
(372), fixture fakes `fake_claude_sdk.mjs` (247) + `fake_pi_rpc.js` (241),
session_client_tests.rs (239).

## Tasks & checklist

- [x] R0 — Plan + baseline metrics (this file).
- [ ] R1 — PR bloat removal: untrack new `.planning/packets/` and
  `.planning/router-runs/` files from the branch (keep working-tree copies),
  `.gitignore` so future runs stay untracked, keep
  `.planning/agent-plans/*.md`. Commit + push.
- [ ] R2 — Architecture & correctness audit (read-only investigation):
  per-abstraction responsibility review of JsonlProcess/RpcSession/
  RpcClientCore/PiRpcClient/ClaudeSdkClient/SessionClient; ACP behavior
  comparison across the 9 correctness areas; concrete retained/simplified/
  consolidated/removed list + redundant-test candidates. Output findings doc.
- [ ] R3+ — Bounded implementation rounds from R2 findings (one concern per
  round): simplifications, then correctness fixes, then focused regression
  tests. Each round: local gate green, reviewed delta, commit + push.
- [ ] FINAL — before/after metrics vs baseline above; PR body update with the
  deliverables summary (complexity removed + why, correctness fixed, remaining
  limitations/unverified behavior); CI green; report to user. No merge without
  explicit user go-ahead ("before merging" = prepare the PR).

## Validation

Per round: `cargo fmt --check`; `cargo clippy -p ajax-web -p ajax-core
--all-targets --all-features -- -D warnings`; `cargo nextest run -p ajax-core
-p ajax-web` (plus focused test filters per round); CI on PR #1264 green after
push. No live SDK verification is claimed unless actually performed against a
live Claude/Pi session.

## Notes / deviations

- Host has no `timeout` binary (C1b finding).
- Branch is 3 commits behind origin/main; do not merge/rebase the branch.
