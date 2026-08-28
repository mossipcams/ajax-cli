# Issue reporting

After a successful exploratory validate + classify step, eligible confirmed
findings are filed as GitHub **Defect** issues on `mossipcams/ajax-cli` (or
`GH_REPO` when set).

Filing runs in `scripts/exploratory/file-issues.mjs` — not in the Cursor Agent
CLI (`gh` stays denied in `.github/exploratory/cli.json`).

## Eligibility

File a finding when:

- `status === "confirmed"`
- `reproductionSuccesses >= 2` (two successful reset/reproduction cycles)
- `classification` is `novel` or `regression`
- fingerprint and evidence paths are present (enforced by validate)

Do **not** file observations, rejected hypotheses, or **known** duplicates.

Agent `relatedIssues` are hints only; they do not suppress filing.

## Classification

`scripts/exploratory/classify-findings.mjs` assigns each confirmed finding:

| Class | Rule |
| --- | --- |
| `known` | fingerprint/title matches an **open** bug issue body comment or exploration memory |
| `regression` | fingerprint/title matches a **closed** bug issue or `memory.regressions` |
| `novel` | otherwise |

Known findings are recorded as duplicates during issue processing (before memory
update). Duplicate comments are emitted only for materially new evidence signatures
already absent from validated memory.

Only `novel` and `regression` findings are eligible for auto-filing as new issues.

## Duplicate detection

Before creating, list open `bug`-labeled issues and dedupe against:

1. An HTML comment in the issue body:
   `<!-- exploratory-fingerprint: <fingerprint> -->`
2. An open issue whose title contains the finding title (case-insensitive).

Fingerprint fallback when missing on the finding:
`${area}|${title lowercased with spaces → hyphens}`

If duplicate: record the existing issue URL in `issues.json`; do not create a
second issue.

## Issue format

Title: `[defect] Web Cockpit <finding.title>`

Body matches `.github/ISSUE_TEMPLATE/defect.yml` / `docs/defect-process.md`:

- Summary, Surface (`Web Cockpit`), Steps, Expected, Actual, Version / commit,
  Severity, Notes (fingerprint, Actions run URL, artifact name, evidence)
- Hidden fingerprint comment at the end of the body

Severity map: `critical` → `blocker`; `high` / `medium` / `low` unchanged.

## Artifacts

`exploratory-results/issues.json` — one entry per eligible finding:

`{ fingerprint, title, action, issueUrl, issueNumber, error? }`

`action` is `created`, `duplicate`, `skipped`, or `failed`.

`run.json` gains an `issues` summary: `{ created, duplicate, failed, skipped }`.

## Safety

- Default: file only when `GITHUB_ACTIONS=true`. Outside Actions, filing is
  skipped (`action: skipped`) unless `--force`.
- `--dry-run`: resolve duplicates and would-create without `gh issue create`.
- Duplicates never fail the job. `gh issue create` failure exits 1.
- Zero eligible findings → empty `issues.json`, exit 0.
