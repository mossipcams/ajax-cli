# Audit Policy

Ajax treats `cargo audit` vulnerabilities and warnings as release blockers.

## No Accepted Warnings

There are no accepted `cargo audit` warnings. Release validation must run:

```sh
cargo audit -D warnings
```

Do not add ignored advisories or warning exceptions without documenting the
production risk, owner, and removal plan in this file.
