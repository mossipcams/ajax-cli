# Release Process

Ajax releases are managed by Release Please. Merging the Release Please PR
updates versions and `CHANGELOG.md`; Release Please then creates the GitHub
release after that PR lands on `main`.

## Repository Setup

Configure `RELEASE_PLEASE_TOKEN` if you want a personal or app token.

If this token is not set, the workflow falls back to `github.token` (the
workflow's built-in token) and release automation still runs.

Use `RELEASE_PLEASE_TOKEN` if you need the PR lifecycle behavior you get from
a PAT/GitHub App token (for example, downstream CI expectations).

## Release Checklist

1. Confirm install and first-run docs in `README.md` still match the command
   surface.
2. Run the required local validation:

```sh
cargo fmt --check
cargo check --all-targets --all-features
RUSTFLAGS="-D warnings" cargo check --all-targets --all-features
cargo check --no-default-features
cargo check --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo nextest run --all-features
cargo test --doc
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo audit
npm run lint:duplication
```

3. Export a local state backup before testing migrations against real data:

```sh
ajax-cli state export --output ~/ajax-state-backup.json
```

4. Build the release binary:

```sh
cargo build --release -p ajax-cli
```

5. Smoke test the release binary with `ajax-cli doctor`, `ajax-cli repos`, `ajax-cli tasks`,
   one full fake-tool workflow, state export checks, and a partial-failure
   recovery journey:

```sh
scripts/smoke.sh
```

6. Merge normal feature and fix PRs into `main` with conventional commit
   titles such as `feat: ...`, `fix: ...`, or `chore: ...`.
   Library-only changes under `crates/ajax-core`, `crates/ajax-supervisor`,
   `crates/ajax-tui`, or `crates/ajax-web` still count toward the grouped
   `ajax-cli` release.
7. Wait for the Release Please PR to open or update.
8. Confirm the Release Please PR has green remote CI checks.
9. Merge the Release Please PR. Release Please will create the tag, changelog
   update, and GitHub release.
