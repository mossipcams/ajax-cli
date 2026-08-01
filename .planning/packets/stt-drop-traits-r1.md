PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

`crates/ajax-web/src/adapters/stt_provider.rs` defines two traits that exist for a
single implementation and are never used generically:

- `trait SttProvider` with `type Session`, `health`, `start_session`, `shutdown`.
- `trait SttProviderSession` with `push_audio`, `poll_event`, `finalize`, `cancel`.

`MoonshineProvider` and `MoonshineSession` already expose every one of these as an
inherent method, and each trait impl body does nothing but forward to the inherent
method of the same name (`Self::health(self)`, `Self::push_audio(self, pcm)`, and so
on). The only consumer, `bridge_task_stt_socket`, takes the concrete
`MoonshineProvider` / `MoonshineSession` types, not the traits.

Delete both trait definitions and both `impl` blocks. Keep every inherent method on
`MoonshineProvider` and `MoonshineSession` exactly as it is — signatures, visibility,
bodies, and doc comments unchanged.

This is a pure deletion. No behavior changes.

## Allowed files

- `crates/ajax-web/src/adapters/stt_provider.rs`

## Forbidden changes

- Any file outside `Allowed files`.
- Do not change any inherent method on `MoonshineProvider` or `MoonshineSession`.
- Do not change `ProviderHealth`, `ProviderError`, `ProviderEvent`,
  `ProviderSessionConfig`, or any of their derives.
- Do not change `bridge_task_stt_socket`, the finalization deadline logic, the writer
  thread, `poll_event`'s disconnect handling, or the frame encoders.
- Do not delete or modify any test.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- A repository grep for `SttProvider` and `SttProviderSession` returns no hits other
  than unrelated substrings such as the module name `stt_provider` and the
  `stt_provider::` path segment.
- `MoonshineProvider::health`, `::start_session`, `::shutdown` and
  `MoonshineSession::push_audio`, `::poll_event`, `::finalize`, `::cancel` all remain
  as inherent methods with unchanged signatures.
- No `use` statement becomes unused; if removing the traits orphans an import,
  remove only that import.
- The full `ajax-web` test suite passes unchanged — same test count as before, with
  no test edited.

## Verification

Run and report actual results for:

- `cargo clippy -p ajax-web --all-targets --all-features` — must pass with no new
  warnings, in particular no `dead_code` warning introduced by the deletion.
- `cargo test -p ajax-web --lib` — must pass, and the reported test count must be
  identical to the pre-change count of 213.

No new tests are expected; this is a deletion with no behavior change.

## Stop if

- Any inherent method turns out to be reachable only through a trait object or a
  generic bound somewhere in the repository — report where and stop.
- Removing the traits requires editing any file outside `Allowed files`.
- The test count changes.
