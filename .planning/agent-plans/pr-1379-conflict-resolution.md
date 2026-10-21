# PR #1379 Conflict Resolution Plan

## Goal

Resolve the conflict between `fix/web/cloudflare-session-expiry` and `origin/main` while preserving the PR's Cloudflare session-expiry recovery and the valid changes on `main`.

## Scope and non-goals

- Scope: files that conflict while merging `origin/main` into the PR branch, plus directly required callers, tests, generated assets, or documentation.
- Non-goals: merging PR #1379, unrelated cleanup, or changing the authentication design beyond conflict integration.
- Approval: user requested conflict resolution; prior instruction to commit and push to the PR remains in effect.

## Tasks

### T001: Resolve PR branch conflicts with `origin/main`

- Status: done
- Depends on: none
- Files: conflict paths discovered during merge

**Steps**

1. Compare ancestor, PR, and base versions for every conflicted path.
2. Combine valid behavior from both sides, stage resolved paths, and complete the merge commit.

**Acceptance Criteria**

- [x] No unresolved paths or conflict markers remain.
- [x] PR session-expiry recovery and relevant `main` behavior are both preserved.

**Verify**

- `git status --short`
- `git diff --check`
- Focused web tests for affected behavior

### T002: Verify and update PR branch

- Status: done
- Depends on: T001
- Files: conflict paths and any directly required companions

**Steps**

1. Review the merged diff against `origin/main` and run focused checks.
2. Commit and push the resolved branch to update PR #1379.

**Acceptance Criteria**

- [x] Focused verification passes, or failures are recorded with cause.
- [x] PR branch contains the resolution and the pushed commit is visible on PR #1379.

**Verify**

- `git diff --check`
- `npm run web:test -- --run <affected tests>`
- `git status --short --branch`
