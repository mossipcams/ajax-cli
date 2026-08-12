# Issue reporting (deferred)

Automatic GitHub issue creation is intentionally **not** part of the first
exploratory testing slice.

Confirmed findings are persisted in `exploratory-results/findings.json` and
uploaded as workflow artifacts. A later change can add duplicate-aware issue
filing (`gh issue list` + create) without changing the explorer charter,
Playwright MCP config, or memory model.

If/when implemented:

- only `status: confirmed` + high confidence (and optionally high/critical severity)
- search open issues for matching fingerprint/title before creating
- request the minimum token permission (`issues: write`) on that job only
- keep default workflow permissions otherwise restrictive
