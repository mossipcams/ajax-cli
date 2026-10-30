# E3b: scripts/ and CSS dead-selector sweep

Read-only audit. Counts are tracked files only (`git ls-files`, `wc -l`). Frontend test duplication is out of scope for this slice (dropped per revision). Scratch sweep script and output: `.planning/router-runs/tech-debt/css-sweep.mjs`, `css-sweep.out`.

Bottom line: scripts/ has no proven dead code. The CSS sweep proves 2 dead rules (19 lines). Both are small, so the slice is far short of the target and does not contribute meaningfully to the 15% goal.

## Part 1: scripts/ (question 1)

### What each script does and who invokes it

| Script | Lines | Invoked by (code / CI / docs) |
|---|---:|---|
| `scripts/exploratory/*` (about 2,900 lines incl. stubs, `lib.mjs` 609) | live | `.github/workflows/exploratory-testing.yml` runs each stage on `schedule: cron "17 6 * * *"` (daily). Documented in `.github/exploratory/README.md`. |
| `scripts/exploratory/http-api.mjs` (41) | live | Imported by `plan-mission.mjs:6` and `preflight-fake-acp.mjs:6`. |
| `scripts/exploratory-*.test.mjs` (5 files, 1,847 total) | live | Run by `package.json` `ci:verify`: `node --test 'scripts/*.test.mjs'`. They import `./exploratory/lib.mjs`, `missions.mjs`, and spawn `plan-mission.mjs`, `validate-run.mjs`, `prepare-instance.mjs`, `prepare-prompt.mjs`, `update-memory.mjs`. |
| `scripts/check-file-loc.mjs` (361) | live | `.github/workflows/ci.yml:81`, `.husky/pre-commit:9`, `architecture.md:141`. |
| `scripts/check-release-version.mjs` (75) | live | `ci:verify` in `package.json`. |
| `scripts/ci-changed-paths.mjs` (187) | live | 4 code references (CI path filtering). |
| `scripts/dev-web-restart.sh` (487) | live | 6 code refs, 4 docs refs. Per project memory, this is the dev web restart path. |
| `scripts/gh-pr-create` (73) | live | AGENTS.md says PRs must use it (`AGENTS.md:177-179`). |
| `scripts/strip-cursor-attribution` (8) | live | 3 code refs. |
| `scripts/task-bootstrap.sh` (38) | live | 3 code refs, 2 docs refs. |
| `scripts/test-in-stable.sh` (40) | live | 11 code refs. |
| `scripts/verify-ci-workflows.mjs` (488) | live | `ci:verify` in `package.json`. |
| `scripts/verify-slice.mjs` (113) | live | `verify:slice`, `verify:arch`, `verify:core` in `package.json`. |
| `scripts/web-build-check.mjs` (117) | live | `web:build:check` in `package.json`. |
| `scripts/setup-stt.sh` (117) | live | Named in the runtime error message at `crates/ajax-web/src/adapters/stt_provider/bridge.rs:172` and in `docs/speech-input.md:45`. |
| `scripts/ajax-moonshine-sidecar` (401, no extension) | live | Copied to `~/.ajax-dev/bin` by `setup-stt.sh` (`docs/speech-input.md:39-49`). |
| `scripts/llm-host/gpu-ctl` (455) + `test_gpu_ctl.py` (403) | live off-repo | Deployed to `/srv/llm/bin/gpu-ctl` on the training host (`docs/architecture/web-cockpit.md:1559-1563`). No in-repo caller, but the host is the consumer. Not removable from this repo. |

Scripts not listed above, `scripts/exploratory/agent-stubs/*` (acp-probe 62, agent 13, claude/codex/cursor/pi/fake-acp 3-4 each): these are PATH stubs that `prepare-instance` and the README (`.github/exploratory/README.md:115`) depend on.

### Caller-search commands

- `git grep -l -F -- <name>` for every script, excluding `.planning` and `scripts/exploratory`. Every script returned at least one reference, except `exploratory-file-issues.test.mjs`, `exploratory-helpers.test.mjs`, `exploratory-memory.test.mjs`, `exploratory-missions.test.mjs`, `exploratory-oracles.test.mjs`, and `scripts/exploratory/http-api.mjs`. Those six had zero hits because the search ignored file names; their callers are the test glob (`ci:verify`) and the `import` graph (`http-api.mjs` is imported by two scripts).
- `git grep -n -E "^import|from \"\./"` over `scripts/exploratory` and `scripts/exploratory-*.test.mjs` to build the import graph.
- `.github/workflows/exploratory-testing.yml` lines 95-153 invoke 16 exploratory steps; `on:` is `schedule` only (line 3-5).

### Candidates

No proven-removable script found. The following were checked and kept:

- **Exploratory harness (whole subsystem)**: not dead. Runs on a daily cron (`.github/workflows/exploratory-testing.yml:3-5`). Removing it would delete a live CI job, which is a product decision (PUBLIC: scheduled CI behavior and filed issues). Not proposed as a candidate.
- **Superseded?** Nothing was found that a newer script replaces. `scripts/exploratory/start-server.sh` and `stop-server.sh` (41, 18) are both called from the workflow.

## Part 2: CSS dead-selector sweep (question 2)

### Method

`.planning/router-runs/tech-debt/css-sweep.mjs` (scratch):
- Stripped CSS comments (section markers are not touched, only blanked for parsing). Parsed rules recursively (descending into `@media`/`@supports`).
- Collected class selectors per rule prelude. A rule is dead only if every selector entry contains a class with no match.
- Reference set: every identifier token (`[A-Za-z_][A-Za-z0-9_-]*`) in `crates/ajax-web/web/src/**` (tsx/ts/js/html/css excluded, tests included), plus `crates/ajax-web/src` Rust sources.
- Dynamic classes: any prefix ending in `-` that precedes `${` or is a concatenation fragment. This is deliberately over-inclusive, so the results are a lower bound.

Result: 37 CSS files, 5,968 lines, 644 source files scanned. 3 rules flagged DEAD, 1 PARTIAL.

### Hand check of flagged rules

- `git grep -n "training-log\|training-page-secondary\|xterm-viewport" -- crates/ajax-web/web/src`:
  - `training-log`: only `TrainingPage.tsx:312,315` (`aria-labelledby="training-log-heading"`, `id="training-log-heading"`). No `className` with `training-log`. **Dead.**
  - `training-page-secondary`: no match in TSX or TS. **Dead.**
  - `.terminal-host .xterm-viewport`: class is generated by xterm.js at runtime, not by this repo. **False positive, keep.**

### Candidates

### E3b-01 CSS `.training-log` rule
- Paths: `crates/ajax-web/web/src/styles/settings.css:370-383`
- Prod lines removed: 14 (rule block 370-383 including the blank line after it; counted as the sweep's `endLine - startLine + 1`)
- Test lines removed: 0
- Evidence: `git grep -n "training-log" crates/ajax-web/web/src` returns `TrainingPage.tsx:312,315` (ids only) and `settings.css:372`. The sweep's reference set includes dynamic prefixes, and `training-` is not one.
- What replaces it: nothing. The page uses `id="training-log-heading"` with no class.
- Behavior or public contract change: none (visual: the log block is not styled by this class; if a log list is ever added it would need a class).
- Risk: low. Caveat: the training page is recent (#1522). Check that no runtime-built class (`"training-" + x`) exists before deleting. The sweep found none.
- Depends on: none. Byte-baseline CSS tests pin CSS (see the `web:test` CSS BASELINE ledger) and must be updated in the same change. Section-marker comments must not be touched.
- Confidence: likely (dead under the sweep's conservative rules; no runtime-constructed class found).

### E3b-02 CSS `.training-page-secondary` rule
- Paths: `crates/ajax-web/web/src/styles/settings.css:1120-1124`
- Prod lines removed: 5
- Test lines removed: 0
- Evidence: `git grep -n "training-page-secondary" -- crates/ajax-web/web/src` returns only `settings.css:1122`.
- What replaces it: nothing.
- Behavior or public contract change: none.
- Risk: low. Same baseline caveat as E3b-01.
- Depends on: none.
- Confidence: likely.

## Not checked or not proven

- The sweep's PARTIAL result (1 rule: a selector list where one class is unmatched and others are live) was not hand-checked. It is not counted.
- Dynamic class prefixes in the sweep: `sign-`, `tone-`, `session-block-`, `surface-`, `activity-`, `color-`, and the very short prefixes `h-`, `r-`, `q-`, `row-` are suppressed. Real dynamic class construction exists for `sign-${...}`, `tone-${...}`, `session-block-${...}` (`ToolCard.tsx:66,107,169`, `TurnActivity.tsx:22`), so classes under those prefixes are treated as live. Some of the short prefixes come from React keys and test strings, not classes, so this suppression is too broad. A narrower sweep could find more dead rules. I did not run it (time limit).
- `scripts/llm-host/test_gpu_ctl.py`: not checked for a CI caller (`git grep test_gpu_ctl`).

## Totals

| Confidence | Prod lines | Test lines |
|---|---:|---:|
| proven | 0 | 0 |
| likely | 19 | 0 |
| speculative | 0 | 0 |
| **Total** | **19** | **0** |

Against the baseline (scripts 5,085 prod / 2,945 test; CSS in ajax-web/web 34,235 prod TS/CSS combined), this slice is about 0.4% of scripts and under 0.1% of the 15,500-line production target.

## Not worth it

- **Exploratory harness (`scripts/exploratory/`, about 2,900 prod lines and 1,847 lines of its tests)**: live daily CI job with a documented pipeline. Trimming it would change CI behavior, which is PUBLIC. Not a candidate.
- **`scripts/llm-host/gpu-ctl` (455) + test (403)**: deployed on the training host outside this repo. Looks unused from inside the repo but is the production path. Keep.
- **`scripts/ajax-moonshine-sidecar` (401) + `setup-stt.sh` (117)**: the speech-input path, referenced from runtime error text and docs.
- **`scripts/dev-web-restart.sh` (487)**: active dev-web restart path (6 code refs).
- **`scripts/verify-ci-workflows.mjs` (488)**: CI gate (`ci:verify`).
- **`scripts/check-file-loc.mjs` (361)**: pre-commit and CI gate.
- **Xterm `.xterm-viewport` rule (`terminal.css:58-66`)**: third-party xterm.js class; the sweep flagged it, and it is live.
- **Exploratory `agent-stubs/`**: PATH stubs used by `prepare-instance` and the exploratory README.
