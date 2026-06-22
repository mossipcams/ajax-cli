# Release Please Remote Build Failure Plan

## Failure Evidence

- Failing run: `Release Please` push workflow run `27967741920` on `main` at `326c032`.
- Failed step: `Run Release Please`.
- Error: `GitHub Actions is not permitted to create or approve pull requests`.
- The normal `CI` workflow passed for the same commit.
- The repository has a `RELEASE_PLEASE_TOKEN` secret, but the current workflow forces `github.token` for Release Please and checkout.

## Task 1: Restore Secret-Backed Release Please Token Selection

- Failing behavior test to write:
  - Update `crates/ajax-cli/tests/repo_hooks.rs` with a workflow contract test that reads `.github/workflows/release-please.yml` and asserts:
    - the workflow resolves `secrets.RELEASE_PLEASE_TOKEN`;
    - it falls back to `github.token` only when the secret is absent;
    - `googleapis/release-please-action@v4` receives `${{ steps.token.outputs.token }}`;
    - the release PR branch checkout uses the same resolved token.
- Code to implement:
  - Reintroduce a `Resolve release token` step in `.github/workflows/release-please.yml`.
  - Use `${{ steps.token.outputs.token }}` for both Release Please and the release PR branch checkout.
  - Keep existing `contents`, `issues`, and `pull-requests` permissions and keep the Cargo.lock sync behavior unchanged.
- Verification:
  - Run the focused failing test before the workflow change:
    - `rtk cargo nextest run -p ajax-cli github_actions_release_please_uses_release_token_when_available`
  - Run the same focused test after the workflow change and show it passing.
  - Run `rtk cargo fmt --check`.

## Task 2: Validate Release Workflow Contract Locally

- Failing behavior test to write:
  - No new failing test; this is validation of the Task 1 contract and YAML content after the behavior fix is green.
- Code to implement:
  - No production code beyond Task 1 unless validation exposes a workflow syntax issue.
- Verification:
  - Read `.github/workflows/release-please.yml` to confirm the release token is used consistently.
  - Run `rtk cargo nextest run -p ajax-cli repo_hooks`.
  - Report that remote confirmation requires pushing and rerunning the `Release Please` workflow.

Plan ready. Approve to proceed.
