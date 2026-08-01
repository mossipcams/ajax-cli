PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
DISPATCH_LEVEL: compact

## Task

Reject terminal link URLs that carry embedded credentials before opening.

`terminalLinkService.onOpen` allows any `http:`/`https:` `parsed.href`, including
`https://user:pass@host/...` from untrusted pane text.

Reject when `parsed.username` or `parsed.password` is non-empty (or open only
`origin + pathname + search + hash` without userinfo). Keep existing rejection
of non-http(s) schemes. Add a unit test for userinfo URLs.

## Allowed files

- `crates/ajax-web/web/src/shared/lib/terminalLinkService.ts`
- `crates/ajax-web/web/src/shared/lib/terminalLinkService.test.ts`

## Forbidden changes

- FloatingContextMenu / TaskTerminal / WebLinksAddon wiring
- Commits, pushes, branch changes

## Acceptance

1. `onOpen("https://user:pass@example.com/x")` does not create/click an anchor
   and does not navigate.
2. Normal `https://example.com/path` still opens via the existing temp-anchor path.
3. Unit tests cover userinfo rejection alongside existing scheme tests.

## Constraints

- Smallest guard in `onOpen`; no new deps.
- Estimated scope ≤ ~30 changed lines.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run terminalLinkService
      expected: userinfo rejection test passes; existing link tests pass
  reason: Proves credential URLs are blocked at the open boundary.
```

## Stop if

- Patch would exceed ~400 changed lines
