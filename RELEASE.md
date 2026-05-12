# Release Process

Use this checklist before tagging or publishing an Ajax CLI release.

1. Update `CHANGELOG.md` with user-facing changes and any migration notes.
2. Confirm install and first-run docs in `README.md` still match the command
   surface.
3. Run the required validation:

```sh
cargo fmt --check
cargo check --all-targets --all-features
RUSTFLAGS="-D warnings" cargo check --all-targets --all-features
cargo check --no-default-features
cargo check --locked
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features
cargo test --locked
RUSTDOCFLAGS="-D warnings" cargo doc --no-deps --all-features
cargo audit -D warnings
```

4. Export a local state backup before testing migrations against real data:

```sh
ajax state export --output ~/ajax-state-backup.json
```

5. Build the release binary:

```sh
cargo build --release -p ajax-cli
```

6. Smoke test the release binary with `ajax doctor`, `ajax repos`, `ajax tasks`,
   one full fake-tool workflow, state export checks, and a partial-failure
   recovery journey:

```sh
scripts/smoke.sh
```

7. Tag the release after CI passes on the release branch.
