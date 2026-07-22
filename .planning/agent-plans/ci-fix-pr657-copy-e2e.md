# CI fix plan: PR #657 Web Copy e2e failures

## Status

**Approved** — implementing product selection guard + e2e harden.

## Delegation decision

`Delegation decision: not delegated because` mechanical e2e/product harden of
named tests and a two-line selection guard; smaller than a delegate work order
(GLM unavailable this week).

## Deviation during execution

Root cause of Copy detach: `onOpen` now calls `scheduleImmediate(true)`, and
`discreteIntent` previously bypassed the selection guard, so a late rAF could
`term.resize` and unmount Copy under the tap. Applied product fix first (skip
all fits while selection is live), plus e2e stability waits for the two failing
tests and the flaky scroll Copy geometry test.

## PR / checks

- PR: https://github.com/mossipcams/ajax-cli/pull/657
- Run: https://github.com/mossipcams/ajax-cli/actions/runs/29921232985
- Failing: **Web** (root), **CI** (aggregate cascade only)
- Passing: Format, Cargo Check, Clippy, Nextest, Docs, Audit, PR Title, CodeQL, Socket

## Failure summary

`npm run web:smoke -- --project=mobile-webkit` — 2 failed, 1 flaky, 94 passed.

| Test | Result | Symptom |
| --- | --- | --- |
| `Copy writes selected text to clipboard and shows Copied notice` | failed ×3 | `Copy` click: not stable → detached from DOM; retry: Copy not found |
| `Copy opens read-only fallback when clipboard write fails` | failed ×3 | same Copy click instability / detach |
| `selection Copy stays pinned beside expand after scrolling…` | flaky (passed on retry) | `boundingBox()` null on first try |

These e2e tests use **mocked** terminal WebSockets (`mockTerminalWebSocket`), so the server-side resize-settle change in `terminal_pty.rs` is almost certainly not on the hot path. The only client source change in the PR that Vite serves is `scheduleImmediate(true)` on open.

## Likely cause

Copy overlay mounts from xterm selection, then a late `fitLocal` / `term.resize` (or selection clear) unmounts it mid-click (“element is not stable” → “detached”). That race can be:

1. **Pre-existing flake** coincident with this PR (main Web has been green recently, but Copy races are timing-sensitive on mobile-webkit), or
2. **Amplified by open-time discrete resize** if a deferred open fit lands after `programTerminalSelection`.

## Proposed fix (smallest first)

1. **Harden the two failing Copy e2e tests** (preferred first cut):
   - After `programTerminalSelection`, wait until Copy is visible **and** stays attached (e.g. short stability poll / `expect(copy).toBeVisible()` with retry after a settle, or `toPass`).
   - Click with `{ force: true }` only if stability wait is insufficient (last resort).
   - Optionally wait for resize quiet: no new resize frames for ~100–200ms before selecting.
2. **If still red locally**, narrow product timing:
   - Keep `scheduleImmediate(true)` for keyboard-open correctness, but ensure open-path fit does not run after the first successful size send (already mostly true via dedupe), or skip `fitLocal` when selection length &gt; 0 even for this discrete open path after the socket is connected.
3. Re-run focused smoke:
   `npm run web:smoke -- --project=mobile-webkit -g 'Copy writes selected|Copy opens read-only'`
4. Push fix commit to `ajax/scroll-history` and confirm Web + CI green.

## Non-goals

- Do not weaken Copy product behavior or remove clipboard fallback coverage.
- Do not revert the settle-rows history-seed fix unless Web stays red after e2e hardening and a product race is proven.
- Do not touch unrelated Playwright tests.

## Approval

Reply **approve** (or adjust the plan) to implement. No code changes until then.
