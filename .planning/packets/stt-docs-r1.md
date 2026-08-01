# STT documentation packet

## Scope

Update `README.md` and add a focused operator guide under `docs/` for the
host-side streaming STT feature already implemented in this worktree.

## Required content

- Moonshine Small Streaming provider setup on the Ajax Mac host.
- Centralized `[stt]` configuration with the actual snake_case keys.
- Provider health/availability behavior and the meaning of a disabled Mic
  control.
- Authenticated browser transport and iOS Safari/PWA permission, interruption,
  background, suspension, and recovery behavior.
- Safe transcript lifecycle: review/edit/Insert or explicit send; no automatic
  terminal execution.

## Boundaries

- Documentation only; do not alter Rust or TypeScript behavior.
- Do not reintroduce a manifest, service worker, public STT endpoint, or cloud
  STT dependency.
- Do not duplicate the full architecture document.

## Verification

- `rg` confirms the documented config keys and setup commands.
- Existing focused config/asset checks run by the parent agent.
