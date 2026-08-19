# Chat Task details module — top 3 P1s

**Date:** 2026-08-18
**Mode:** Presentation only (Web Cockpit session details sheet)
**Approval:** User scoped to top 3 P1s on the Task details module
**Critique:** `.impeccable/critique/2026-08-19T00-00-50Z__ajax-web-web-src-features-session-sessionchat-tsx.md` (21/40)

## Scope

Reshape the Ajax chat **Task details** sheet (`.session-details-sheet` / `session-task-panel` in `SessionChat.tsx`) so it is a thin operator dossier, not a kitchen-sink settings dump.

P1s only:

1. **Distill** — current model + “Change” disclosure; full catalog on demand (#948). Identity and Ajax terminal in the first viewport.
2. **Layout** — lead with title + handle (+ branch). Reuse or thin-embed `TaskMetaDetails` instead of the poorer `session-meta`.
3. **Clarify** — do not render Rust Debug annotation strings. Heading + human lines, or hide.

## Non-goals

- P2s (observation-error placement, dual pickers / tap-target drift) unless they fall out of the P1 work for free
- LiveHead, transcript, composer, ActionBar policy, Drop confirm
- Task lifecycle, registry, ACP, or backend annotation format (presentation only)
- Visual-world replacement

## Checklist

- [x] Distill model catalog behind disclosure
- [x] Lead sheet with task identity; share `TaskMetaDetails` if that is the smaller correct path
- [x] Human notes or hide Debug annotations
- [x] Focused SessionChat / TaskMetaDetails tests for the three behaviors
- [x] Update DESIGN.md scoped exception if the sheet’s first-viewport contract changes

## Verification

- `cd crates/ajax-web/web && npx vitest run src/features/session/SessionChat.test.tsx src/features/task/TaskMetaDetails.test.tsx`
- Detector once after UI edits: `node /Users/matt/.claude/skills/impeccable/scripts/detect.mjs --json` on changed markup

## Polish (impeccable)

Refinement of the same Task details module. Not a redesign. P1s stay; finish local defects so the path matches DESIGN.md.

- [x] Sheet field labels use tracked chrome (not unstyled `.field-label`)
- [x] Close / Change / Done are 44px pills in this sheet
- [x] Details and Change expose `aria-expanded` (and catalog `id` for `aria-controls`)
- [x] Observation error uses the same “Observation error:” prefix as task detail and sits under identity, not after the catalog
- [x] One live model surface: session catalog hidden while harness Switch is open
- [x] Sheet ActionBar does not steal primary fill for Ship
- [x] Dead CSS (`.session-model-switching` if still unused) and leftover `session-meta` if unused
- [x] Focused tests for the polish behaviors

## Validation

- SessionChat + TaskMetaDetails vitest: pass (2026-08-18 polish)
