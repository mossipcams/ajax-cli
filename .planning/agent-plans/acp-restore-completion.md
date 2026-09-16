# ACP restore completion

## Scope

- Fix the end-to-end restore deadline and prevent ambiguous resume timeouts from falling through to load on the same child.
- Replace string-only restore classification with a typed adapter-to-slice error contract.
- Add explicit Retry and Start fresh recovery controls to Ajax Chat.
- Add focused regression coverage and update the web-session behavior contract.

## Non-goals

- No task registry ownership, ACP protocol version, or unrelated UI changes.
- Preserve the existing touch-scrolling changes.

## Checklist

- [completed] Trace restore, timeout, error, and browser recovery paths.
- [completed] Implement shared restore deadline and timeout isolation.
- [completed] Introduce typed restore failures across adapter and slice.
- [completed] Add Retry/Start fresh operator recovery flow.
- [completed] Add regression tests and update architecture docs.
- [completed] Run focused and PR verification checks.

## Approval

- Immediate implementation explicitly requested by the user.

## Validation

- `cargo fmt --all -- --check` passed.
- `cargo check -p ajax-web --all-targets` passed.
- `cargo test -p ajax-web --lib` passed (690 tests).
- `npm run web:check` passed with a temporary shared-checkout dependency symlink.
- `npm run web:lint` passed with the temporary dependency symlink.
- `npm run web:test -- --run` passed (1511 passed, 9 skipped).
- `git diff --check` passed.

## Deviations

- Implementation proceeded locally because the user explicitly said not to
  delegate this work.
