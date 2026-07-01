# Resolve `ajax/refactor-web` Merge Conflicts

## Context

- PR branch: `ajax/refactor-web`
- Base branch: `origin/main`
- Strategy: merge `origin/main` into the PR branch, preserving both the mobile terminal refactor and the base branch terminal scroll interception fix.
- Backup branch created before conflict work: `backup/conflict-resolution-20260701-*`

## Baseline

Merge base: `7c4fb150ff3f56e34ace4d43a3c00639f9ddc0c9`

PR branch adds:

- grouped/mobile tmux terminal sizing and orphaned session cleanup
- iOS keyboard resize suppression and readable mobile font sizing
- sticky Ctrl behavior across bar keys
- reconnecting raw terminal with foreground resume
- send-keys route and read-only pane snapshot route
- default mobile terminal split into snapshot/composer and raw terminal views
- raw terminal reconnect overlay clearing

Base branch adds:

- terminal touch/scroll gesture interception fix in `TerminalPanel.svelte`
- test coverage in `TerminalPanel.test.ts`
- generated web bundle update in `web/dist/app.js`
- planning docs for the scroll interception fix

Previewed conflicted files:

- `crates/ajax-web/web/dist/app.js`
- `crates/ajax-web/web/src/components/TerminalPanel.svelte`
- `crates/ajax-web/web/src/components/TerminalPanel.test.ts`

## Tasks

### Task 1: Resolve Svelte source conflict

- Failing behavior test to write/run: start the merge, inspect conflicted stages for `TerminalPanel.svelte`, then run the existing focused Svelte/Vitest checks that should fail while conflict markers are present:
  - `npm run web:check`
  - `npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts`
- Code to implement: merge base branch scroll/touch interception behavior into the refactored `TerminalPanel.svelte` shell without reintroducing removed raw-terminal responsibilities that moved into `TerminalRawView.svelte`.
- Verification: `npm run web:check` passes, and symbol searches confirm scroll/touch interception handlers are wired to the intended terminal container after the refactor.

### Task 2: Resolve `TerminalPanel.test.ts` conflict

- Failing behavior test to write/run: preserve or adapt the base branch scroll interception behavior test in `crates/ajax-web/web/src/components/TerminalPanel.test.ts`, then run it focused first to show failure before implementation if Task 1 did not already make it pass.
- Code to implement: keep the test aligned with the post-refactor `TerminalPanel` responsibilities, updating only conflict resolution and any expectations required to verify the same user-visible scroll interception behavior.
- Verification: `npm run web:test -- --run crates/ajax-web/web/src/components/TerminalPanel.test.ts` passes.

### Task 3: Regenerate generated web bundle conflict

- Failing behavior test to write/run: verify `crates/ajax-web/web/dist/app.js` still contains conflict markers after the merge starts using `rg '<<<<<<<|=======|>>>>>>>' crates/ajax-web/web/dist/app.js`.
- Code to implement: run the project web build/check process that regenerates `web/dist/app.js` from resolved source, then stage the regenerated bundle.
- Verification:
  - `npm run web:build`
  - `rg '<<<<<<<|=======|>>>>>>>' crates/ajax-web/web/dist/app.js crates/ajax-web/web/src/components/TerminalPanel.svelte crates/ajax-web/web/src/components/TerminalPanel.test.ts` returns no matches.

### Task 4: Finish merge and validate PR branch

- Failing behavior test to write/run: after all conflicts are staged, `git merge --continue` should fail if any conflict remains unresolved.
- Code to implement: complete the merge commit without changing unrelated files.
- Verification:
  - `git diff --check`
  - `cargo fmt --check`
  - `cargo check -p ajax-web --all-targets`
  - `npm run web:check`
  - `npm run web:test -- --run`
  - `gh pr view 263 --json mergeable,mergeStateStatus,statusCheckRollup`

## Notes

- Approval of this plan explicitly permits editing only the conflicted test file `crates/ajax-web/web/src/components/TerminalPanel.test.ts`.
- No smoke tests will be edited.
- If additional conflicts appear after merge continuation, pause and report the new conflict set before proceeding.
