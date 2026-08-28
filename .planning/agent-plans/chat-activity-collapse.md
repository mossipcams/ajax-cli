# Ajax Chat activity disclosure — collapse the work log

Status: **implemented** (P2 review fix: whole-turn live disclosure).

## Review fix (P2)

**Bug:** `Conversation` passed `live={isLiveTurn && rowIndex === turn.rows.length - 1}`
into `TurnActivity`. `groupTurns` ends a work run when agent prose arrives, so a
still-`pending`/`in_progress` tool was no longer the last row and
`collapsedWorkItems(items, live)` hid the in-flight call while the session was
still busy.

**Fix:** Pass whole-turn liveness (`live={isLiveTurn}`) into `TurnActivity`.
Collapsed settled stays summary-only; collapsed live still surfaces only
`pending`/`in_progress` rows from tool status, including when prose follows that
work in the same busy turn. Prose streaming gating unchanged.

- [x] Regression: user prompt → completed tool → in_progress tool → agent prose,
      `busy=true` → in_progress card visible, completed cards hidden.

## Why

The transcript contract says the surface is a conversation, not the ACP event
stream. Collapsed activity still paints every tool call as a row. On a typical
turn that is a dozen lines of `Read` / `Searched` / `Used MCP: tool` between
the question and the answer, under a summary that already said `Read 13 files ·
ran 2 commands`.

Always-visible rows (B1) were a previous product choice. They are the thing
that looks awful. Missing targets make it worse; listing thirteen rows would
still be wrong with perfect filenames.

## Outcome

A settled turn reads: user message, one summary line, assistant prose.
The work log is behind the disclosure.

## Scope

Web Cockpit chat presentation, its tests, the transcript-composition contract,
and host `rawInput` mapping only if MCP rows still cannot name the tool.

## Non-goals

- No new visual language, icon set, or syntax highlighting.
- No change to reducer semantics, turn grouping, transport, or ACP wire types.
- No change to auto-expand on failure / attention, or to session open/close
  preference.
- No task-lifecycle, registry, or ownership change.

## Behavior

- [x] **Collapsed, settled:** summary row only. No tool cards.
- [x] **Collapsed, live:** summary row, plus only `pending` / `in_progress`
      tool rows (watch the current call). Completed calls in that turn stay
      hidden until expand.
- [x] **Expanded:** full work log as today (tools, thoughts, plans, permission
      markers, bodies).
- [x] **Summary counts what the dump was showing:** reads, edits, searches,
      commands, and other/MCP tools. A turn that searched and called MCP must
      not read as only `Read N files · ran M commands`.
- [x] **MCP / generic rows:** expanded (and live in-flight) labels must not
      render `Used MCP: tool`. Prefer a real target from location / `rawInput`
      (`toolName` / `tool` / `name` / query / path). Generic titles
      (`Read File`, `MCP: tool`, …) are not verb-prefixed into noise.
- [x] Update `docs/architecture/web-session-behavior.md` §Transcript
      composition: tool rows are **not** always visible; they follow the
      disclosure. Keep the row-target derivation sentence.

## Implementation files (expected)

- `crates/ajax-web/web/src/features/chat/activity/TurnActivity.tsx`
- `crates/ajax-web/web/src/features/chat/activity/TurnActivity.test.tsx`
- `crates/ajax-web/web/src/features/chat/activity/ActivityDisclosure.test.tsx`
- `crates/ajax-web/web/src/features/chat/activity/activitySummary.ts`
- `crates/ajax-web/web/src/features/chat/activity/activitySummary.test.ts`
- `crates/ajax-web/web/src/features/chat/activity/presentation.ts`
- `crates/ajax-web/web/src/features/chat/activity/presentation.test.ts`
- `crates/ajax-web/web/src/features/chat/activity/currentOperation.ts` (comment / live summary only if needed)
- `crates/ajax-web/web/src/styles/chat/conversation.css` (comments that still say tool rows always)
- `crates/ajax-web/web/e2e/session-chat-regression.test.ts`
- `docs/architecture/web-session-behavior.md`
- `crates/ajax-web/src/slices/web_session/acp_map.rs` and its tests — only if
  presentation cannot name MCP tools from fields the host already forwards

## Validation

- [x] Invert the collapsed-row tests: settled collapsed → 0 tool cards; live
  collapsed with one in-progress call → that card only.
- [x] Summary fixtures cover search + MCP alongside reads/commands.
- [x] `toolRowLabel` fixtures: generic MCP title is not `Used MCP: tool`; a real
  tool name or path still reads verb-first.
- [x] E2E collapsed assertion no longer expects tool cards on the grid.
- [x] `npm run web:test -- --run` for the touched vitest files
- [x] `npm run web:check` and `npm run web:lint` if TS/CSS changed
- [x] focused `ajax-web` mapping tests if `acp_map.rs` changed

## Approval status

User requested the defects addressed (`it's awful` on the live dump).
Implement now. Update this checklist as work lands.
