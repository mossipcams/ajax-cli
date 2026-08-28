# Ajax Web Exploratory Testing (CI-only)

Cloud-only exploratory testing for Ajax Web. Everything runs on a
GitHub-hosted `ubuntu-latest` runner. It is intentionally **not** part of local
development, `npm test`, husky, or `npm run verify`.

## Secrets

| Secret | Required | Purpose |
| --- | --- | --- |
| `CURSOR_API_KEY` | yes | Authenticate the Cursor Agent CLI |

Create the secret in the repository settings (or via `gh secret set CURSOR_API_KEY`).
The workflow fails clearly when the secret is missing. Do not commit API keys.

## Workflow

`.github/workflows/exploratory-testing.yml`

- `schedule`: daily `17 6 * * *` UTC only (no manual dispatch)
- One Cursor **scout** process per run (no relaunch / continuation)
- Job timeout: 45 minutes
- Artifacts uploaded with `if: always()`

## Architecture

```text
GitHub-hosted Actions VM
  ├── checkout Ajax
  ├── restore exploration memory (cache; optional)
  ├── install Node / Rust / Playwright WebKit / tmux / Cursor CLI
  ├── build Ajax Web + ajax-cli
  ├── prepare isolated ajax-cli web (target/exploratory-instance; agent stubs on PATH)
  ├── plan deterministic mission (primary + fallback)
  ├── start server + wait for readiness
  ├── seed mission product state via authenticated /api/* (POST /api/session first)
  │     └── on primary seed failure → activate fallback mission and re-seed
  ├── preflight fake ACP wrapper + seeded task verification when required
  │     └── on primary preflight blocked → activate fallback, re-seed if needed, retry
  ├── prepare oracles + mission prompt
  ├── Cursor Agent + Playwright MCP (WebKit) scouts under charter
  ├── validate findings (verifier confirmation gate) + read-only checkout guard
  ├── classify independently verified findings (novel / known / regression)
  ├── update bounded campaign memory
  ├── file novel/regression independently verified findings
  └── fail on infrastructure, blocked preflight, or empty skeleton runs
```

The exploration budget is a **maximum**, not a target. WebKit only — no Chromium/Firefox fallback.

## Scout vs verifier

The nightly Cursor process is a **scout**, not the final authority on defects.

| Role | Who | Writes |
| --- | --- | --- |
| Scout | Cursor Agent (Composer 2.5) | `findings.json`, `observations.json`, scout evidence under `traces/` / `screenshots/` |
| Verifier | Host workflow (deterministic) | `verifier.json` + evidence under `exploratory-results/verifier/` |

The scout may mark a hypothesis `confirmed` after two reproduction cycles, but
`validate-run.mjs` applies a **verifier confirmation gate**:

- Agent `confirmed` findings **without** matching host verifier evidence are
  **demoted to `observation`** (reproduction count cleared, classification removed).
- Only findings that still read `confirmed` after the gate — backed by a
  `verifier.json` entry with `source: "deterministic-verifier"`,
  `reproductionSuccesses >= 2`, and on-disk files under
  `exploratory-results/verifier/` — proceed to classification and issue filing.

`.github/exploratory/cli.json` denies the scout write access to
`exploratory-results/verifier/**` so the agent cannot self-verify.

`classify-findings.mjs` assigns `novel` / `known` / `regression` only to
independently verified confirmed findings. `file-issues.mjs` files only those
same eligible findings.

## Mission selection and fallback activation

`scripts/exploratory/plan-mission.mjs` writes `exploratory-results/mission.json`:

- Compares web-related changes since validated `memory.lastRunSha`
- Rotates least-recently tested missions
- Avoids missions tied to known confirmed fingerprints
- Emits one **primary** mission and one **fallback**

The prompt assigns exactly one mission per nightly run. If infrastructure blocks
the primary before exploration starts, the host activates the fallback
executable mission instead of aborting immediately:

| Step | Script | Fallback behavior |
| --- | --- | --- |
| Seed | `plan-mission.mjs --seed` | Primary seed failure → promote fallback in `mission.json`, re-seed, record `run.mission.fallbackActivated` |
| Preflight | `preflight-fake-acp.mjs` | Primary preflight `blocked` → promote fallback, re-seed when the fallback requires it, retry preflight |

If both primary and fallback seed/preflight fail, the job stops before the scout runs.

## Time budget and finalization reserve

Nightly budget env vars (defaults in the workflow):

| Variable | Default | Meaning |
| --- | --- | --- |
| `AJAX_EXPLORATORY_BUDGET_MINUTES` | `12` | Total nightly time budget |
| `AJAX_EXPLORATORY_FINALIZATION_MINUTES` | `2` | Reserved for artifact finalization — **not** given to the scout |

`run-agent.sh` enforces scout runtime as `budget − reserve` (10 minutes by
default). The reserve is held back so a budget timeout still leaves wall-clock
time for the scout to write `findings.json`, `observations.json`,
`memory-delta.json`, and `run.json`. A timeout (exit 124) is treated as a
controlled stop, not an infrastructure failure.

## Fake ACP preflight

Session missions seed tasks with agent **`cursor`**. Ajax launches Cursor via
`agent [--model ID] acp`; `scripts/exploratory/agent-stubs/agent` on the server
PATH delegates to `fake-acp`, which wraps `crates/ajax-web/tests/fixtures/fake_acp.js`.
`preflight-fake-acp.mjs` proves initialize + session/new + session/prompt through that
wrapper and, when the server is up, verifies the seeded task is readable.

## Intelligence model

Exploration is driven by **assigned mission + oracles**, not a codebase index.

Before each scout run, `scripts/exploratory/prepare-oracles.mjs` writes
`exploratory-results/oracles.json`:

| Oracle | Source |
| --- | --- |
| `openBugs` | Open GitHub `bug` issues (prefers Web Cockpit / `[defect]`) |
| `closedBugs` | Closed GitHub `bug` issues (regression fingerprints) |
| `recentWebCommits` | Last 20 web-related `git log` lines |
| `routes` | Static list matching `routes.ts` |
| `boundaryHashes` | Known routing defect neighborhood |
| `memory` | Durable corpus hints (`dullActions`, focus, fingerprints) |

## Scout constraints

- Model: **Composer 2.5** (`composer-2.5`) — fixed in the workflow and runner
- Role: scout only — hypotheses and scout evidence; no issue filing or verifier writes
- Charter: `.github/exploratory/charter.md`
- MCP: `.github/exploratory/mcp.json` (Playwright MCP, WebKit headless, iOS-ish viewport 390×844)
- CLI permissions: `.github/exploratory/cli.json` (WebKit MCP + results writes only; sandbox enabled, shell/network denied; **`exploratory-results/verifier/**` denied**)
- Post-run `git` dirty check fails the job if product source changed

## Outputs

`exploratory-results/`

- `mission.json` — primary/fallback mission for this run (primary may be replaced when fallback activates)
- `oracles.json` — oracle pack for this run
- `run.json` — run metadata, mission/fallback flags, preflight, usefulness, infrastructure, classification, issue summary
- `findings.json` — confirmed / observation / rejected items after the verifier gate (+ `classification` when independently verified)
- `verifier.json` — host deterministic verifier results keyed by finding id
- `issues.json` — GitHub issue filing results for eligible findings
- `observations.json` — lower-confidence notes (includes scout-confirmed items demoted by the gate)
- `memory-delta.json` — adaptive hints for the next run
- `seed.json` — optional mission seeding via public API
- `traces/`, `screenshots/`, `verifier/`, `logs/`

## Issue automation

After validate + classify succeed, `scripts/exploratory/file-issues.mjs` processes
**novel** and **regression** independently verified confirmed findings for creation,
and records **known** findings as duplicates (commenting only when materially new
evidence signatures appear). Agent `relatedIssues` are hints only and do not
suppress filing.

Eligible findings require all of:

- `status === "confirmed"` after the verifier confirmation gate
- host `deterministic-verifier` evidence in `verifier.json` with
  `reproductionSuccesses >= 2` and files under `exploratory-results/verifier/`
- `classification` is `novel` or `regression`
- a fingerprint and scout evidence paths under `exploratory-results/`

Scout-only confirmations (no host verifier entry) are observations and are never
classified or filed.

See `issue-reporting.md`.

## Local note

Developers cloning Ajax should never auto-run this suite. Optional tooling
checks live under `scripts/exploratory-*.test.mjs` and only validate schemas /
mission helpers — they do not launch the scout or a browser.

Fixture-backed local checks:

```bash
node scripts/exploratory/prepare-instance.mjs
node scripts/exploratory/plan-mission.mjs
node scripts/exploratory/preflight-fake-acp.mjs
node scripts/exploratory/validate-run.mjs --fixture --skip-readonly
```
