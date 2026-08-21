---
context: default
slug: architecture-ci-gate
status: complete
approval: user-directed 2026-08-21 — create an enforceable architecture CI gate
last_updated: 2026-08-21
---

# Architecture CI gate

## Goal

Make Ajax chat (and crate) architecture ownership rules a named, required GitHub
Actions check. Today those rules live in `architecture.rs` tests and
`npm run verify:arch`, but CI only runs them as anonymous needles inside
path-filtered `rust-test` nextest.

This follows `.planning/agent-plans/ajax-chat-architecture.md` Phase 5
dependency-direction tests and the path-filter contract in
`.planning/agent-plans/ci-balanced-speed.md`.

## Evidence

Chat architecture target ownership (slice vs ACP adapter vs JSONL store vs
runtime vs thin WebSocket) is already asserted in
`crates/ajax-web/src/architecture.rs`:

- session mechanism adapters must not import `web_session` or each other
- `web_session` may call those adapters
- adapters must not import slices or `runtime`
- slices stay isolated from siblings and `runtime`
- declared adapters/slices must have architecture guards

Sibling crates have the same pattern (`ajax-core`, `ajax-tui`,
`ajax-supervisor`). `npm run verify:arch` runs `cargo test -p <crate>
architecture` for all four.

CI gap:

- `rust-test` runs those tests only when the rust lane is on
- web-only PRs skip them, even though `ajax-web` architecture tests also pin
  Vite/CSS embed contracts under `crates/ajax-web/web/`
- there is no named Architecture check; a failure is buried in nextest
- docs/agent-only PRs correctly skip them (keep that fast path)

## Scope

- Add a path-filtered `architecture` job to `.github/workflows/ci.yml`
- Wire it through `scripts/verify-ci-workflows.mjs` and the aggregate `CI` job
- Document it in `docs/agent/pull-requests.md` and the Validation section of
  `architecture.md`

## Non-goals

- Do not always-run on docs/agent-only diffs (preserve the cheap path)
- Do not extract architecture.rs into a Node grep tool
- Do not add new architecture rules beyond wiring the existing suite
- Do not filter architecture tests out of rust-test nextest (defense in depth)
- Do not change task lifecycle, registry, or session protocol

## Design

New job `architecture` (display name `Architecture`):

- Skip on `release-please--branches--main`
- Run when `rust` or `web` or `full`
- Checkout, pinned Rust toolchain, `Swatinem/rust-cache` `shared-key: ci`
- Run `npm run verify:arch` (no `npm ci`; the script only shells out to cargo)
- No Node setup beyond the runner default

Aggregate `CI` (normal PRs):

- Require `architecture` success when `full || rust || web`
- Keep docs/agent-only as title + file-loc + invariants

`scripts/verify-ci-workflows.mjs`:

- Add `architecture` to `RELEASE_SKIP_JOBS` and `PATH_FILTERED_JOBS`
- Assert the job runs `npm run verify:arch`
- Assert aggregate `needs` and `needs.architecture.result` enforcement

Rust PRs will run architecture tests twice (this job + nextest). Accept that:
the named job fails faster and is visible; nextest remains the full suite.

## Implementation checklist

- [x] Add `architecture` job to `.github/workflows/ci.yml`
- [x] Aggregate `CI` needs the job and requires it on rust|web|full
- [x] Update `scripts/verify-ci-workflows.mjs`
- [x] Update `docs/agent/pull-requests.md` CI trigger contract
- [x] Update `architecture.md` Validation to name the CI gate
- [x] Run `npm run ci:verify` and `npm run verify:arch`; record results

## Stop conditions

Stop and revise before:

- running architecture tests on every docs-only PR
- weakening path-filtered rust/web lanes or Release Please cheap path
- replacing or deleting existing `architecture.rs` assertions
- adding a second architecture policy engine besides the crate tests

## Validation

```bash
npm run ci:verify   # pass 2026-08-21 (73 script tests + workflow invariants)
npm run verify:arch # pass 2026-08-21 (ajax-core 8, ajax-web 13, ajax-tui 2, ajax-supervisor 2)
```

## Approval and status

- Plan creation: approved by user request 2026-08-21 to create the gate.
- Implementation: complete 2026-08-21.
