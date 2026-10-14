# Display Claude usage errors in Web Chat

## Scope and non-goals

- Show Claude's actual session-limit or other run error in Ajax Web Chat when the Claude SDK run fails.
- Preserve the session's existing turn-end behavior and show the failure once.
- Add focused regression coverage for the recorded limit failure, verify the affected slice, and create the requested PR with `scripts/gh-pr-create` referencing #1287.
- Do not change Claude quota, authentication, task lifecycle, registry authority, terminal behavior, or public network exposure.

## Checklist

- [x] Match a Web Chat failure to the underlying native Claude error.
- [x] Inspect the Claude sidecar, event mapper, and adjacent tests; choose the smallest error-propagation fix.
- [x] Add a focused regression test that fails before the fix and passes afterward.
- [x] Run focused checks and the repository's PR verification gate; record results.
- [x] Create or reuse the PR with `scripts/gh-pr-create`, include `Fixes #1287`, and review its final delta.

## Approval and evidence

- The user explicitly requested a PR to display the Claude usage error and, after the branch-name blocker was explained, replied “Open pr.” That authorizes the required `fix/web/claude-usage-error` branch creation and switch for this request. No PR has been created yet.
- At 2026-10-13 22:40:03 UTC, a native Claude session reported `error=rate_limit` and “You've hit your session limit · resets 6:30pm (America/Chicago).” The matching Ajax Web Chat event displayed `Claude run failed: success`.
- Other native Claude sessions and a direct delegate probe reported the same account limit. The account limit is external; this task fixes the display of its error.
- The earlier message-only patch was restored before this work, so the source files start from the pre-fix state.

## Validation log

- Read-only event/transcript comparison: passed. Native Claude `rate_limit` aligns with Ajax Web Chat's misleading event.
- Prior message-only mapper experiment: mapper tests and clippy passed, but it was restored because it did not prove delivery of the recorded usage text.
- Focused regression `cargo test -p ajax-web --lib rate_limited_turn_displays_claude_session_limit_text`: failed against the original mapper (exit 101), then passed within `cargo test -p ajax-web --lib claude_sdk_map_tests` (exit 0).
- `cargo test -p ajax-web --lib claude_sdk`, `cargo fmt --check`, `cargo clippy -p ajax-web --lib --all-targets`, and `git diff --check`: passed (exit 0). The first format check failed (exit 1) before the delegate formatted the test; the rerun passed.
- PR creation gate and `scripts/gh-pr-create`: passed. Husky hooks ran on the PR commits; PR #1296 is open.
- The first implementation delegate added a mapper change and regression test but timed out after 900 seconds before reporting verification or creating a PR. The two-file delta is retained for review by the router-selected fallback.
- The fallback stopped before verification because the current branch is `ajax/defects`, while `scripts/gh-pr-create` requires `type/scope/description`. The user has since authorized opening the PR after that blocker was explained; the PR delegate may create and switch to `fix/web/claude-usage-error`.
- The code-only delegate completed the mapper, test, and documentation edits but its model quota expired before a structured report. Its raw tool log records the red regression and passing focused checks listed above; the parent reviewed the resulting delta.
- 2026-10-14 (PR step): created and switched to `fix/web/claude-usage-error` from `ajax/defects` at `e872f534` (an ancestor of `origin/main`, so the PR delta is only the reviewed files). Installed Husky with `npm ci` plus `npx husky`; hooksPath is `.husky/_`. `cargo fmt --check`: passed (exit 0). No implementation changes since the recorded passing focused runs, so no test rerun was needed. The pre-commit hook rebuilds/stages web dist when web sources are staged; the diff touches only Rust and markdown, so no dist rebuild is expected.
- `scripts/gh-pr-create`: passed; created [PR #1296](https://github.com/mossipcams/ajax-cli/pull/1296) as `fix(web): claude usage error` at commit `506d29e0`, containing only the four scoped files. The PR body includes `Fixes #1287`. Parent review found an imprecise sentence in the body; the follow-up corrected it and pushed the completed checklist at `46341af5`.

## Deviations and risks

- The reported run failure itself came from account quota. The user subsequently requested a PR for accurate error display rather than a quota workaround.
- If the current branch does not satisfy the repository PR script's naming contract, stop before changing branches and report the specific constraint.
- The installed Claude SDK result path stores API-error text in `result` even when `subtype` is `success` and `is_error` is true. The regression models this shape; a live limited-session run remains an integration check for CI or manual review.
