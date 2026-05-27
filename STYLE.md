# Style

## Workspace Hygiene

Keep Ajax as a small Rust workspace with clear crate boundaries and boring
repository-level defaults. The root manifest owns shared package metadata,
third-party dependency versions, and lint policy. Member crates should inherit
those settings unless a crate needs an explicit local feature set.

Releasable internal crate dependencies are the main exception: keep those
`path` + `version` declarations in the consuming crate manifests so
Release Please can rewrite version links during workspace releases.

Prefer manifest and configuration cleanups that leave runtime behavior
unchanged. Do not split crates, introduce new workspace structure, or move
business logic as part of style-only work.

Keep feature choices visible where they define a crate boundary. For example,
`ajax-tui` should continue to show the Ratatui feature set it relies on even
when the dependency version comes from the workspace.
