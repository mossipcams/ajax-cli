# Refactor Root Agent Contract

## Scope

- Reduce always-loaded context in root `AGENTS.md` while preserving Ajax
  repository requirements.
- Keep the root contract portable across Cursor, Codex, Pi, and other coding
  harnesses.
- Move conditional runbooks into focused shared documents under `docs/agent/`.
- Update documentation links that would otherwise become stale.

## Non-goals

- No application source, tests, workflows, configuration, or behavior changes.
- No harness-specific rules in the shared repository contract.
- No new RTK behavior or local-machine-only correctness dependency.
- No duplication of architecture or defect-process documentation.

## Approval

- Plan approved by the user with: “Implement until all tasks are finished.”
- That approval covers all listed tasks without per-task pauses.
- PR and worktree rollout approved by the user with: “create pr for ajax-cli
  with these changes, update every ajax-cli worktree with these changes”.

## Existing worktree state

Before editing, explicit work-tree status showed unrelated existing changes in:

- `.agents/skills/model-router`
- `.claude/skills/model-router`
- `.codex/skills/model-router`
- `.cursor/skills/model-router`
- untracked `scripts/route`

These paths are outside this task and must remain untouched.

## Section classification

| Original section | Classification | Destination |
| --- | --- | --- |
| Instruction Priority | universal | concise root contract |
| Read First | universal plus task-category | conditional links in root |
| Local RTK Guidance | universal rule plus machine-specific locator | portable root rule; remove absolute-path dependency |
| Work Strategy | generic plus task-category | compress; plan procedure in `docs/agent/plans.md` |
| Planning-Only / Code Change | generic plus task-category | retain only ownership, scope, and verification requirements |
| Architecture Change | task-category | root trigger, `architecture.md`, and `docs/agent/plans.md` |
| Persistent Plans | task-category | `docs/agent/plans.md`; concise root trigger |
| Model Routing / Work Orders / Review | task-category with harness examples | `docs/agent/routing.md`; concise root rules |
| Non-Negotiable Rules | mixed | universal safety in root; generic advice removed |
| Ajax Architecture Guardrails | universal | root, unchanged in meaning |
| Web Cockpit Guardrails | task-category | `architecture.md` and `docs/architecture/web-cockpit.md` |
| Testing and Verification | universal | concise root expectations |
| Defect Process | task-category | root requirement plus `docs/defect-process.md` |
| Validation Commands | task-category | architecture/README guidance plus PR document |
| Rust Conventions | task-category; file limit universal | `docs/agent/rust.md`; file limit in root |
| Search and Code Navigation | generic | remove from repository contract |
| Dependency Policy | generic | remove from repository contract |
| Cleanup Policy | generic | remove from repository contract |
| Documentation Policy | conditional | concise root rule and owning docs |
| PR / CI / release / local gate | task-category | `docs/agent/pull-requests.md` |
| Stop Conditions | universal | concise root contract |
| Maintaining This File | universal metadata | concise root contract |

## Tasks

- [x] Add the persistent ledger and shared routing/planning documents.
- [x] Add focused Rust and pull-request documents and repair stale links.
- [x] Rewrite root `AGENTS.md` as the concise shared contract.
- [x] Validate requirement coverage, referenced paths, size reduction, diff
  scope, and unchanged application/test/workflow files.
- [ ] Create an isolated PR worktree and commit only the approved documentation
  changes through the repository's local verification gate.
- [ ] Push the branch, create the PR, and wait for GitHub checks to finish.
- [ ] Apply the committed documentation delta to every pre-existing Ajax
  worktree without overwriting unrelated local changes.

## Validation

- Baseline `AGENTS.md`: 589 lines, 3,249 words, 21,618 bytes.
- Expected-failure check before Task 1: exit 1 because this plan,
  `docs/agent/routing.md`, and `docs/agent/plans.md` did not exist.
- Task 1 focused assertion: exit 0; all three documents exist and contain the
  required classification, routing, and persistent-plan contracts.
- Expected-failure check before Task 2: exit 1 because the Rust and pull-request
  documents were missing and `architecture.md` still pointed at `AGENTS.md` for
  the PR gate.
- Task 2 focused assertion: exit 0; Rust conventions, PR-title rules, CI and
  release invariants, bypass prohibitions, and the corrected architecture link
  are present.
- Expected-failure check before Task 3: exit 1 for the 3,249-word root, absolute
  RTK path, embedded generic/runbook sections, and missing focused-doc links.
- The first Task 3 post-change assertion caught a line break inside the required
  defect-issue wording; the implementation wording was adjusted and the
  identical assertion passed. The coverage assertion likewise caught a split
  same-harness prohibition in `routing.md`; the document was adjusted and the
  identical assertion passed.
- Final root assertion: exit 0 with all required rules present, moved runbooks
  absent, no absolute RTK path, and 1,067 words.
- Requirement coverage assertion: exit 0 for safety, architecture, Web
  Cockpit, defect, RTK, Rust, planning, routing, verification, PR, CI, and
  release requirements.
- Markdown/path assertion: exit 0; every referenced document exists.
- Trailing-whitespace assertion: exit 0.
- `git diff --check -- AGENTS.md architecture.md`: exit 0.
- Final root size: 164 lines, 1,067 words, 7,695 bytes. Reduction from baseline:
  72.2% lines, 67.2% words, and 64.4% bytes.
- Explicit work-tree status succeeded and still shows the unrelated pre-existing
  model-router paths and `scripts/route`, plus only this task's documentation
  paths. A scoped diff query for `crates`, `tests`, `.github/workflows`,
  `package.json`, `release-please-config.json`, and `.husky/pre-commit` returned
  no paths.
- Application tests and `npm run verify` were skipped because this is a
  documentation-only refactor and no PR was requested.
- Initial plain `git status --short` and `rtk git status --short` probes failed
  with `fatal: this operation must be run in a work tree` because this checkout's
  `.git/config` has `core.bare=true`; explicit `--git-dir=.git --work-tree=.`
  status and diff commands succeeded.

## Deviations

- None.

## PR and worktree rollout

- Branch: `chore/trim-agent-contract`.
- Dedicated worktree: `/Users/matt/Desktop/Projects/ajax-cli__worktrees/chore-trim-agent-contract`.
- Existing dirty worktrees will receive the documentation delta as uncommitted
  working-tree changes; their branch history and unrelated edits remain intact.
