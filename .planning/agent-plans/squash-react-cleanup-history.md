# Squash plan — react-migration-cleanup history

## Goal

Collapse `origin/main..HEAD` into one commit per slice (plus two chores),
dropping extract/revert/re-land and per-round plan noise.

## New history (parent → child)

| Commit | Tip tree | Notes |
| --- | --- | --- |
| `chore(web): remove obsolete Svelte…` | `986781c` | Slice 1 |
| `feat(web): ESLint + accessible queries…` | `d0d0d17` | Slice 2 (toolchain, 2b queries, App deps) |
| `refactor(web): read URL hash as external store` | `7dd4223` | Slice 3 |
| `refactor(web): extract cockpit/version/task resources` | `777d41e` | Slice 4 |
| `fix(web): Strict Mode–safe terminal mount` | `44bfdde` | Slice 5 (+ svelte reappearance guard) |
| `refactor(web): shadcn Button` | `83802ac` | Slice 6 |
| `chore(ci): drop lint step…` | `f41bbb0` | Keep |
| `feat(web): shadcn Sheet + new-task focus trap` | `e8cbdc6` | Slice 7 |
| `feat(web): keyboard-navigable agent picker` | `302a7a0` | Slice 8 |
| `refactor(web): app/features/shared layout` | `bd23126` | Slice 9 (+ on-device note) |
| `chore(web): ast-grep structural scanning` | files from `866dd8b` | Keep separate |
| `refactor(web): terminal effect events + scroll sync` | `HEAD` | Slice 10 net (r1+r2a+viewportY) |

## Method

`commit-tree` from tip trees; backup branch `backup/pre-squash-react-cleanup`.
Rewrites commits already on `origin/ajax/react-migration-cleanup` → needs
force-with-lease to update remote when Matt asks.

## Delegation

`Delegation decision: not delegated because` history rewrite is operator work,
not a bounded TDD implementation.
