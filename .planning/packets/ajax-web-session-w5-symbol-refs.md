PACKET_STATUS: READY
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []
TASK_KIND: behavior

## Task

Make known symbol references in Ajax Web Session assistant responses tappable, opening a lightweight symbol detail sheet (name, path, source, kind) with an Attach action for the next message. Also land a brief architecture note for the POC path.

## Scope

### Allowed
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx
- crates/ajax-web/web/src/features/session/AjaxWebSessionView.test.tsx
- crates/ajax-web/web/src/features/session/SymbolDetailSheet.tsx
- crates/ajax-web/web/src/features/session/SymbolDetailSheet.test.tsx
- crates/ajax-web/web/src/features/session/types.ts
- crates/ajax-web/web/src/features/session/renderMessage.tsx (optional helper)
- crates/ajax-web/web/src/features/session/renderMessage.test.tsx
- crates/ajax-web/web/src/styles.css
- docs/architecture/web-cockpit.md
- .planning/agent-plans/ajax-web-session-poc.md
- .planning/packets/ajax-web-session-w5-symbol-refs.md

### Forbidden
- Full code editor
- Dependency graphs / AST visualization
- Terminal integration
- Rust/backend changes unless a tiny bug blocks frontend (prefer none)
- Commits / branch changes
- Enabling for non-Cursor agents

## Acceptance

1. When rendering assistant (and optionally user) messages, detect references to known symbols from the session’s attached/known symbol set (at minimum: exact name matches in backticks like `` `foo` ``, and plain `Name` / `Type.method` tokens that uniquely match a known symbol).
2. Tappable refs open `SymbolDetailSheet` showing: symbol name, file path, source code, symbol kind/type. Mobile sheet/drawer.
3. Sheet has “Attach to next message” that adds the symbol to composer chips (reuse Wave 4 attach state).
4. Maintain a session-local known-symbol map seeded from attached symbols (and optionally successful search picks). POC-quality matching is fine; do not build a full parser.
5. Brief subsection in `docs/architecture/web-cockpit.md` describing Ajax Web Session: feature-flagged Cursor-only alternate task surface, `agent acp` chat backend, presentation-only, no terminal replacement when flag off.
6. Focused vitest for linkification + detail sheet attach.
7. Wave 5 + docs checklist items done in the plan.

## Constraints

- Keep matching conservative to avoid over-linking every word.
- Reuse existing sheet styling patterns from SymbolSearchSheet.
- No desktop multi-panel IDE layout.

## Verification

```yaml
verification:
  methods:
    - type: test
      command: npm run web:test -- --run src/features/session
      expected: pass
  broader_checks: []
  reason: Frontend-only Wave 5; vitest covers linkify + detail attach.
```

## Stop if

- Need large Rust changes
- Edits outside Allowed
- Exceed ~400 changed lines
- Would build a full code editor

## Code anchors

- Chat + chips: `crates/ajax-web/web/src/features/session/AjaxWebSessionView.tsx`
- Search sheet pattern: `crates/ajax-web/web/src/features/session/SymbolSearchSheet.tsx`
- Symbol types: `crates/ajax-web/web/src/features/session/types.ts`
- Architecture doc: `docs/architecture/web-cockpit.md` (add short POC subsection near Web Cockpit slices / presentation)

## Edit instructions

1. Add conservative symbol linkifier + SymbolDetailSheet.
2. Wire taps from message render into detail sheet + attach.
3. Document Ajax Web Session in web-cockpit.md (short).
4. Check off Wave 5 + docs in the plan.
