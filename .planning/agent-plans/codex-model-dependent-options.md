# Codex model-dependent option restore

Issue: https://github.com/mossipcams/ajax-cli/issues/1145

## Scope and approval

Fix saved Codex selections rejected before their base model is applied. The user
approved direct investigation/delegation bypass and implementation of Task 1,
then requested continuing to completion and opening a PR. Commit/push/PR are
authorized; the per-task continuation gate has been waived for the remaining work.
Use the same shared ACP pin-application path for fresh and restored sessions.
Do not edit any `tests/` directory, weaken assertions, alter the live wake-word
session, change registry/lifecycle authority, merge, or deploy.

## Evidence

- `SaySo/wake-word` uses Codex and stores
  `gpt-5.6-sol|reasoning_effort=max|fast-mode=false`.
- Its transcript has Low, High, and Max refusals immediately after context-reset
  notes. Historical raw handshake payloads were not retained in that transcript.
- A separate prompt-free probe with installed codex-acp 1.1.7, in the same
  worktree, returned only mode/collaboration_mode/model on session/new with
  gpt-6-astra. Selecting gpt-5.6-sol returned reasoning_effort including max and
  boolean fast-mode. The probe was terminated without touching the task session.
- `apply_model_pin` maps the entire saved pin before sending any request;
  `map_selection_to_steps` refuses the absent reasoning_effort immediately.
- Initial option-renaming hypothesis was superseded by this confirmed
  model-dependent advertisement/ordering defect.

## Task 1 — restore model before dependent settings (approximately 15 minutes)

- [x] Inspect source, callers, existing tests, installed adapter, and task evidence.
- [x] Deduplicate and file defect #1145.
- [x] Obtain implementation approval.
- [x] Write a failing fake-ACP regression beside the existing adapter tests
  (`client_spawn_model_tests.rs`, or a focused sibling if needed). Put any new
  fake under `crates/ajax-web/testdata/`, outside `tests/` directories. Start with
  a different model and no reasoning/Fast controls; expose them only after the
  target model is selected. Assert model-first request order and confirmed
  model/effort/Fast for low, high, and max, for fresh and restored sessions.
  Check unsupported options still refuse and report the actual applied model.
- [x] Run the regression and report its failing assertion before production edits.
- [x] Implement the smallest staged apply in the shared ACP adapter: validate and
  select the advertised bridge base model when it differs, replace descriptors
  from the response, then map/apply dependent settings against those descriptors.
  Preserve strict advertised-value checks, truthful partial-apply reporting, and
  Cursor's existing full-intent mapping.
- [x] Document model-dependent sequencing in the owning session behavior document.
- [x] Run the regression and relevant existing adapter tests; report passes.
- [x] Review diff and file sizes. Task 1 complete; user approved continuation.

The regressions live in `apply_model_tests.rs`, with a 63-line fake in
`testdata/web_session/model_dependent_acp.cjs`. No files under `tests/` changed.
The production fix reuses `apply_config_option` for the base-model stage and
keeps Cursor's existing mapping unchanged.

## Task 2 — publish the verified fix (approximately 5–15 minutes plus CI)

- [x] Confirm the existing branch/worktree, default PR base, and installed hooks.
- [x] Reproduce and resolve temporary-directory collisions in the new fixture
  runner without weakening assertions; verify repeated runs.
- [ ] Commit through the repository's Husky gate and push `ajax/model-defect`.
- [ ] Open the PR with `scripts/gh-pr-create`, linking #1145.
- [ ] Follow GitHub CI and report its result and mergeability.

No additional product implementation is planned in this task. If CI reveals a
related implementation failure, reproduce it first and fix the implementation.

## Verification

- `rtk cargo test -p ajax-web config_options_tests --lib`: PASS, 28 tests.
- Prompt-free installed-adapter probe: PASS, observed absent controls before and
  present controls after base-model selection; no model prompt was sent.
- `rtk cargo test -p ajax-web model_dependent --lib`: RED before implementation,
  2 failures: exact missing reasoning_effort refusal and old applied base model.
- First post-fix run: 1 passed, 1 failed during Node initialize with an incoming
  transport closure. Diagnostic rerun with
  `rtk proxy cargo test -p ajax-web model_dependent --lib -- --nocapture`: PASS,
  2 tests covering 9 fresh/resume/load effort combinations and 2 unsupported
  setting cases. No assertions were weakened. Repeated testing then reproduced
  startup failure as `ENOENT`. Strengthening temporary-directory creation from
  `create_dir_all` to `create_dir` demonstrated `AlreadyExists`: concurrent tests
  can share a timestamp. Added an atomic counter to directory names. Ten repeated
  runs then passed (20 tests, 110 fake sessions).
- `rtk cargo test -p ajax-web web_session_acp --lib`: PASS, 102 tests.
- `rtk cargo fmt --all -- --check`: PASS.
- `rtk git diff --check`: PASS.
- Repository LOC evaluator before fixture race fix: PASS; changed Rust files
  were 375 and 197 lines. The commit hook rechecks final staged files.
- GitNexus returned outdated file paths; current source was used instead.
- Several initial path/glob searches failed on nonexistent paths; corrected
  searches found the actual source and state directory.
- Earlier Cursor delegate did not investigate: initial prompt-path invocation
  failed, corrected invocation returned `Upgrade your plan to continue`.

## Remaining limits

Product implementation is complete; PR publication and CI are pending.
Historical handshake details are inferred from the confirmed live reproduction,
not recovered from old task sessions. Deployment and live task recovery were not
performed. The observed fixture startup failure was resolved by unique directory
names; the failure and its red/green verification are recorded above.
