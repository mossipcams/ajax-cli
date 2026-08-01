PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Stop Diff Review GET handlers from failing with 409 when cockpit revision
moves during a slow `gh`/`git` projection.

`axum_task_pull_requests` and `axum_task_diff` (`runtime.rs` ~1014-1118) run
inside `spawn_blocking` but still call `run_optimistic`, which commits the
cloned context only if revision is unchanged. Concurrent cockpit/terminal
mutations during a 30s Diff load return 409; the UI shows a generic load error.

Add a read-oriented path for these two GETs: run the projection against a
cloned context and return 200 without requiring an optimistic revision commit.
If PR metadata persistence is still desired, do it as a separate best-effort
write that must not gate the HTTP success of the Diff response.

## Allowed files

- `crates/ajax-web/src/runtime.rs`
- `crates/ajax-web/src/slices/diff_review.rs`

## Forbidden changes

- Frontend DiffReview.tsx / swipe
- ajax-core `diff_review.rs` domain rewrite (call-site only if required)
- Removing Diff observation entirely without a replacement persist strategy note in the report
- Commits, pushes, branch changes

## Acceptance

1. Diff pull-requests and diff GET handlers no longer return 409 solely because another request advanced cockpit revision during the projection.
2. Focused test: while a Diff GET is in-flight (gated slow CommandRunner), another revision-bumping path runs; Diff still returns 200 with a valid JSON body (or documented soft error that is not conflict).
3. Existing Diff status mapping for TaskNotFound / Unobservable / PrNotFound stays intact.
4. Health/async isolation from the earlier spawn_blocking work remains green.

## Constraints

- Prefer a `run_read` / clone-without-commit helper beside `run_optimistic` rather than weakening all optimistic writers.
- Smallest diff; do not refactor unrelated Axum routes.
- Estimated scope ≤ ~150 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: cargo test -p ajax-web --lib axum_diff -- --nocapture
      expected: new non-409-under-concurrent-revision test passes; existing axum_diff tests pass
    - type: build
      command: cargo check -p ajax-web
      expected: success
  reason: Proves read Diff GETs are not gated on optimistic write revision.
```

## Stop if

- Fix requires changing global optimistic concurrency for all routes
- Persist-vs-read semantics for PR metadata cannot be decided without architecture approval
- Patch would exceed ~400 changed lines
