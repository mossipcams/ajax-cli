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

- `schedule`: daily UTC
- `workflow_dispatch`: manual runs from GitHub Actions UI
- Job timeout: 45 minutes
- Artifacts uploaded with `if: always()`

## Architecture

```text
GitHub-hosted Actions VM
  ├── checkout Ajax
  ├── restore exploration memory (cache; optional)
  ├── install Node / Rust / Playwright Chromium / Cursor CLI
  ├── build Ajax Web + ajax-cli
  ├── start isolated ajax-cli web (target/exploratory-instance)
  ├── Cursor Agent + Playwright MCP explores under charter
  ├── update memory + upload exploratory-results/
  └── fail only on infrastructure/explorer failure (not every product bug)
```

## Agent constraints

- Model: **Composer 2.5** (`composer-2.5`) — fixed in the workflow and runner
- Charter: `.github/exploratory/charter.md`
- MCP: `.github/exploratory/mcp.json` (Playwright MCP, Chromium headless)
- CLI permissions: `.github/exploratory/cli.json` (deny source edits / git mutation)
- Post-run `git` dirty check fails the job if product source changed

## Outputs

`exploratory-results/`

- `run.json` — run metadata and infrastructure status
- `findings.json` — confirmed / observation / rejected items
- `observations.json` — lower-confidence notes
- `memory-delta.json` — adaptive hints for the next run
- `traces/`, `screenshots/`, `logs/`

## Issue automation

Automatic GitHub issue creation is deferred. Confirmed findings land in
artifacts first. A later slice can add duplicate-aware issue filing without
changing the explorer core.

## Local note

Developers cloning Ajax should never auto-run this suite. Optional tooling
checks live under `scripts/exploratory-*.test.mjs` and only validate schemas /
memory helpers — they do not launch the agent or a browser.
