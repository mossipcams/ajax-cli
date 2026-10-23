# Training command center page (Local → Training)

Approval: user requested implementation directly (2026-10-22, "Add a new page
under the local tab specially for controlling training"). Security-relevant
(new forced-command verbs) — owning doc `docs/architecture/web-cockpit.md`
§ `ajax-web::slices::training` updated in the same change.

## Host truth (probed read-only 2026-10-22)

- Deployed `/srv/llm/bin/gpu-ctl` has DRIFTED from `scripts/llm-host/gpu-ctl`:
  deployed adds `TRAINING=/srv/llm/lfm/ajax-optimization/sayso-training`,
  `JOB_CWD={"wake-train": SAYSO}` (other jobs run in TRAINING),
  `GEN_YAML=…/full_sft_v6.yaml`, per-pid count-cache tmp. Repo still has the
  `GEN_PROGRESS` phase logic (deployed dropped it). Deployed = authoritative
  paths; keep repo's GEN_PROGRESS logic (harmless when file absent).
- SaySo pipeline (`$TRAINING/sayso`): generate → validate → canary →
  promote-dataset → train → eval → promote-model (+ select-checkpoint).
  Run records: `$TRAINING/training/runs/promotion/<run_id>.json` with flags
  `validated, canary_passed, dataset_promoted, trained, final_eval_passed`;
  `current.json = {"run_id"}` selects the dataset the next step acts on;
  `promoted_dataset.json`, `promoted_model.json` (champion, has `eval_metrics`).
- Evals: `EVAL_RESULTS=/srv/llm/lfm/runs/sayso-promotion/<group>/<eval_id>/`
  with `metadata.json` (`checkpoint`, `suite`, `gate_label`, `created_at`) and
  `summary.json` (`summary.{total,passed,overall_pass_rate,category_pass_rate}`,
  `promotion.{passed,failures}`). Checkpoint vs full = basename of
  `metadata.checkpoint` (`checkpoint-N` vs `final`).
- Device: training runs under the GPU lock (`gpu.state` = `train:<label>`);
  evals run CPU-only (`docker exec -e CUDA_VISIBLE_DEVICES=` …
  `eval_checkpoint_cpu.py`).

## Scope

1. `scripts/llm-host/gpu-ctl`: reconcile with deployed; then
   - `status` adds `device` (`"gpu"` when gpu.state starts `train:`, `"cpu"`
     when another job runs, else null) and `pipeline` (current run_id + the 5
     stage flags).
   - verb `datasets` → `{"datasets":[{run_id, current, promoted, size_bytes,
     validated, canary_passed, dataset_promoted, trained, final_eval_passed}]}`
     newest first.
   - verb `dataset-select <run_id>`: run_id must match
     `^\d{8}T\d{6}Z-[0-9a-f]{8}$` and an existing record; refused (exit 3)
     while busy; atomic write of `current.json`.
   - verb `evals` → `{"evals":[{id, group, checkpoint_label, kind
     ("checkpoint"|"final"), suite, gate_label, created_at, total, passed,
     pass_rate, promotion_passed, failures, category_pass_rate}],
     "champion": {run_id, checkpoint, overall_pass_rate}|null}` newest first,
     capped at 200.
   - JOBS add `validate`, `canary`, `promote-dataset`, `promote-model` (fixed
     argv `./sayso <verb>`; no override flags).
2. `crates/ajax-web/src/slices/training` + `runtime/mod.rs`: pass through
   `device`/`pipeline`; `GET /api/training/datasets`, `GET /api/training/evals`,
   `POST /api/training/datasets/select {run_id, confirm}` (run_id validated in
   Rust too, must be in host list, 409 when busy); extend TRAINING_JOBS.
3. Web: new page reachable from the Local modal (link) at its own route; page
   shows pipeline stepper + device badge + step/total/loss/ETA, dataset list
   with select, pipeline job buttons + stop (confirm dialogs), eval table
   (checkpoint vs final, pass rate, gate pass/fail, champion row), log tail.
   Polls status 3s, evals/datasets 15s, only while the page is mounted.

Non-goals: manual-override model promotion, select-checkpoint, editing
generation configs, charts. Existing TrainingModal tests untouched.

## Tasks

- [x] T1 host script (gpu-ctl) + py self-check (3 rounds: initial, evals
      field-mapping fix, champion per-suite `pass_rates` — eval_metrics on
      the host is keyed by suite, so `champion.overall_pass_rate` became
      `champion.pass_rates{suite: rate}`)
- [x] T2 Rust slice + routes + tests + web-cockpit.md (Qwen provider error →
      Haiku; fmt/clippy/38 training tests pass. NOT done: adding the 3 routes
      to the protected-route list in runtime/tests/suite_1.rs — test dir is
      user-gated; routes are protected anyway via api_access_policy default.
      Pre-existing unrelated failure on HEAD:
      install::tests::stylesheet_preserves_the_safari_first_visual_language
      (#87afd7 removed by #1383).)
- [x] T3 web page + api + tests (Haiku hit session limit mid-task → Cursor
      finished; gate-fix round (lint + CSS BASELINE/dist); polish round after
      live-data screenshot review: run-group row titles, ad-hoc suite label,
      20-row Show more, Select disabled while busy, champion basename)
- [x] T4 parent review of every delta + live-data mobile-WebKit screenshots
- [x] T5 gate (cargo fmt/clippy --all-targets --all-features/nextest, vitest,
      `pkill -f vite; CI=1 npm run web:smoke`)
- [ ] T6 deploy gpu-ctl to host (live op, ask first)

## Validation log

- 2026-10-22 `python3 scripts/llm-host/test_gpu_ctl.py` → 7 tests OK.
- 2026-10-22 read-only verbs piped to host (`SSH_ORIGINAL_COMMAND=<verb>
  python3 - < gpu-ctl`, not deployed): datasets → 5 runs; evals → 154 with
  correct suite/total/pass_rate; champion pass_rates {promotion: 0.7917,
  context: 1.0}.
- Note: run 20261022T120622Z-6e261f59 lives in a different checkout
  (`/srv/llm/lfm/ajax-optimization__nonstandard/sayso-training`) and is not
  listed — gpu-ctl reads only the deployed TRAINING path.
- 2026-10-22 gate: cargo fmt/clippy --all-targets --all-features OK;
  nextest --all-features --no-fail-fast 3084/3085 (only the pre-existing
  install palette failure; stt_provider flake passed 3/3 on rerun);
  web:check/lint/sg OK; vitest 1697 passed; `pkill -f vite; CI=1 npm run
  web:smoke` 159 passed.
- Remaining: T6 deploy gpu-ctl to /srv/llm/bin/gpu-ctl (awaiting user OK);
  commit/PR not requested; suite_1.rs protected-route list not updated
  (test dir, user-gated).

## Review + goal audit (2026-10-22, round 2)

- Code-review fixes landed: Confirm double-submit guard (page), JSON
  hardening (`read_obj`), stale catalog responses ignored, champion fixture
  uses the real shape, failure keys, modal nav telemetry.
- Goal gaps found and fixed: `./sayso train` always trains the PROMOTED
  dataset, so the page now says so and blocks Train when that record is
  missing. `select-checkpoint` ("Evaluate checkpoints") was added so checkpoint
  evals can be triggered.
- End-to-end (no deploy): the worktree server binary plus an ssh shim piping
  the worktree gpu-ctl to the host, read-only verbs only. Unauthenticated
  requests get 401; with a session, status/datasets/evals return 200; a bad
  run id gets 400. The live full train (step 575/4458) shows as GPU with the
  right step and ETA.
- OPEN (user decision): the live run comes from
  `/srv/llm/lfm/ajax-optimization__nonstandard/sayso-training` (current and
  promoted = 6e261f59), while gpu-ctl TRAINING points at
  `/srv/llm/lfm/ajax-optimization/sayso-training`, so Pipeline and Datasets
  show the stale checkout.
- OPEN: the pre-existing TrainingModal double-submit defect needs a GitHub
  issue (AGENTS defect process) before it is fixed.
- Gate: nextest 3086/3087 (pre-existing install failure only); vitest 1705;
  web:check/lint/sg OK.

## Round 3 (2026-10-22): main + deploy

- Decision (user): gpu-ctl reads the primary checkout
  `/srv/llm/lfm/ajax-optimization`, and that checkout should be on `main`.
  It is currently on `ajax/model-training-failure` (clean, fully pushed, 29
  ahead / 3 behind main; run state gitignored). Switch AFTER the live full
  train (`python3 ./sayso train`, PID 1642449, started from __nonstandard but
  executing train_unsloth_full.py from the primary path) finishes. Do not
  automate it: evals from __nonstandard also execute scripts from the primary
  path.
- main has no `sayso select-checkpoint`. gpu-ctl now reports
  `checkpoint_eval` (TRAINING/sayso mentions it) and the page shows Evaluate
  checkpoints only when it is true; start refuses it otherwise.
- DEPLOYED gpu-ctl to /srv/llm/bin/gpu-ctl (sha256 2be98ce4…, backup
  gpu-ctl.bak-20261022T175204Z, atomic rename). Verified via the llm-gpu
  forced key: status/datasets/evals OK; bogus verb, `../../etc` id and unknown
  job all exit 2.
- Final gate: vitest 1707; web check/lint/sg OK; clippy/fmt OK; nextest
  3085/3088 — install palette (pre-existing on HEAD) + 2 intermittent flakes
  in untouched crates (web_session transcript backlog, supervisor monitor);
  gpu-ctl 19 tests; web:smoke 159 passed.
- Remaining: checkout switch after the run; web UI goes live only after this
  branch ships (commit/PR not requested); TrainingModal double-submit defect
  issue (asked).
- #1386 opened (TrainingModal double-submit). Fix: actionInFlightRef guard and
  Confirm disabled while active, with a regression test that double-clicks
  inside one act() so only the ref guard can stop the second call. Cursor
  died on auth mid-run, and its file-wide stopTraining stub broke the 409
  test; Codex revised it and the test now passes. Codex's report failed
  schema validation, so it was accepted on delta review plus a rerun: vitest
  1708 passed, web check/lint/sg OK.
