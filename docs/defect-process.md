# Ajax Defect Process

How Ajax tracks and closes product defects.

Agent rules that point here live in root `AGENTS.md`. Issue filing uses
`.github/ISSUE_TEMPLATE/defect.yml` on
[`mossipcams/ajax-cli`](https://github.com/mossipcams/ajax-cli).

## What counts as a defect

A **defect** is incorrect product behavior relative to an expected contract:
broken CLI/API behavior, wrong registry or lifecycle outcomes, Web Cockpit
regressions, crashes, data loss, or security failures that should not happen.

| Kind | Track as defect? | Notes |
| --- | --- | --- |
| Confirmed broken expected behavior | Yes | User-reported or agent-proven with repro |
| Missing feature / new capability | No | Use enhancement / feature work instead |
| Intentional design or documented limitation | No | Explain; do not open a bug to “change the design” without agreement |
| Docs-only typo or wording | No | Fix docs directly unless it causes product misuse at scale |
| Local env / tool / machine misconfig | No | Fix the environment; open an issue only if Ajax itself is wrong |
| Speculative “might be wrong” | No | Gather a repro first |

## Tracking rule

Ajax defects are tracked **only** as GitHub issues on `mossipcams/ajax-cli`.

Do not treat chat history, agent plans, or local notes as the system of record.
Do not silently fix a confirmed defect without an issue.

## Before opening an issue

1. Confirm it is a defect (table above).
2. Dedup:

   ```bash
   gh issue list --repo mossipcams/ajax-cli --label bug --state open --search "<short symptom>"
   gh issue list --repo mossipcams/ajax-cli --state open --search "<short symptom>"
   ```

3. If a duplicate exists, comment with new repro evidence or link that issue from
   the fix PR. Do not open a second issue.

## Opening an issue

Prefer the GitHub **Defect** form (`.github/ISSUE_TEMPLATE/defect.yml`).

From the CLI:

```bash
gh issue create --repo mossipcams/ajax-cli --label bug --title "<short symptom>" --body "$(cat <<'EOF'
### Summary
<one sentence>

### Surface
CLI | core | Web Cockpit | native Cockpit | other: <name>

### Steps to reproduce
1.
2.
3.

### Expected
<what should happen>

### Actual
<what happens>

### Version / commit
<ajax-cli version, git SHA, or unknown>

### Severity
blocker | high | medium | low

### Notes
<logs, screenshots, related PRs>
EOF
)"
```

Required fields:

- Title (short symptom)
- Repro steps
- Expected vs actual
- Surface (CLI, core, Web Cockpit, native Cockpit, other)
- Severity when known
- Version or commit when known

Open the issue **before** or in the **same session** as the fix. Fix PRs must
reference it with `Fixes #N` or `Closes #N`.

## Fix workflow

1. **Issue** — existing or newly opened on `mossipcams/ajax-cli`.
2. **Regression test** — add a focused test that fails on the buggy behavior and
   passes after the fix. Prefer the nearest existing test module in the owning
   crate or web suite; do not invent a new test framework.
3. **Fix** — smallest change that restores correct behavior.
4. **Verify** — run the new/updated regression test, then the strongest practical
   focused checks for that slice (`cargo nextest`, `npm run web:test`, etc.).
5. **PR** — conventional `fix(...)` title; body links `Fixes #N` / `Closes #N`.

### Where regression tests usually live

| Surface | Typical home |
| --- | --- |
| `ajax-core` | crate unit/integration tests beside the owning module or slice |
| `ajax-cli` | `crates/ajax-cli` tests (including bridge/backend tests) |
| Web Cockpit (TS/UI) | `crates/ajax-web/web` vitest tests nearest the component/helper |
| Web/e2e behavior | existing Playwright suites under web e2e when unit tests cannot cover it |
| Operator slices | slice-local tests; prefer `npm run verify:slice -- <slice>` when applicable |

Name or comment the test so the defect symptom is obvious (issue number in the
test name or a one-line comment is enough).

### Untestable exception

If a regression test is not practical without substantial new harness work,
document in **both** the issue and the PR:

- why automated coverage is blocked
- the manual check that replaces it
- any follow-up to add harness coverage later

Do not use this exception to skip easy unit or integration coverage.

## Closing criteria

A defect is done when:

- the linked PR merges (or the issue is closed with a clear wontfix/duplicate
  reason)
- the regression test is green in the change (or the documented exception is
  accepted)
- expected behavior is restored for the reported surface

## Agent checklist

- [ ] Classified as defect (not enhancement / env / speculation)
- [ ] Searched for existing `mossipcams/ajax-cli` issue
- [ ] Opened or linked a GitHub issue
- [ ] Added or updated a regression test (or documented exception)
- [ ] Fix PR references `Fixes #N` / `Closes #N`
- [ ] Focused verification ran and passed
