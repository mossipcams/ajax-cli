# Session UX: model shortlist + turn-as-chapter chat

**Date:** 2026-08-19
**Mode:** Behavior change (Web Cockpit presentation only)
**Approval:** Confirmed 2026-08-19 — operator chose A/A/A
  (curated Cursor top 10 + Show all; turn-as-chapter; both in one change)

## Outcome

1. New-task and in-session model pickers show a **shortlist of ~10**, not the full harness catalog, with **Show all** for the rest.
2. Ajax chat conversation is **turn-as-chapter**: the agent answer leads; ACP work (tools/thoughts/plan) collapses to one summary per turn.

## Non-goals

- ACP protocol, JSONL transcript ownership, `sessionReducer` event semantics, or `GET /api/session/models` payload shape.
- Dashboard, raw terminal route, permission-buttons-in-head.
- New visual world / DESIGN.md identity replacement. Inherit Ajax Cockpit.
- Adding a markdown library or HTML injection.

## Scope

- `crates/ajax-web/web/src/features/session/**`
- `crates/ajax-web/web/src/styles.css`
- `crates/ajax-web/web/e2e/session-chat-regression.test.ts` (only if assertions break)
- `DESIGN.md` (session scoped exception)
- `docs/architecture/web-cockpit.md` (model-page sentence)

## T1 — Model shortlist

Shared `ModelPicker` (New task, Session Model Change, Harness Switch).

- Keep fetching the **full** catalog. Filter in the UI.
- **Cursor rank** (first catalog match wins a slot): `auto`, `composer-2.5` else `composer`, `cursor-grok-4.6-high` else `cursor-grok`/`grok`, then GPT (`gpt-5.6`, `gpt-5`, `gpt`), Claude opus, Claude sonnet, Gemini. Fill remaining slots from advertised order until 10.
- Other harnesses: advertised order, cap 10.
- Always include **Auto** (when present), harness **default**, and **current selection**, even if that makes the visible list 11.
- **Show all** when the catalog is longer than the shortlist. Collapse label **Show fewer**.
- Replace ModelPicker test `#948` (“lists every model”) with shortlist + Show all + current-selection pinning.
- Host still accepts any advertised id (Show all / unknown current).

## T2 — Turn-as-chapter transcript

Presentation grouping in `Transcript` over existing `ConversationItem[]`. Do not invent a second transcript store.

- A **turn** is user prose through the next user prose.
- Order in a settled turn: operator bubble → **work chapter** (thoughts, tools, plan, permission markers) → **agent answer**.
- Settled successful work is **one summary row** (`Edited N files · ran tests` / tool counts + elapsed). Expand to the existing activity log.
- Live, failed, or in-progress work stays open. Permission markers stay in history; buttons stay in LiveHead.
- Agent markdown: extend the existing parser (no `innerHTML`, no library) for **tables, links (`http`/`https` only), nested lists, blockquotes**.
- Stop live-reply `white-space: pre-wrap` fighting markdown. Stream into the same renderer.
- Hide LiveHead **Turn tokens** line. Keep context-pressure meter. Reducer may still store `turnUsage`.

## T3 — Docs

- DESIGN.md scoped exception: conversation is turn-as-chapter; model Change shows shortlist + Show all (not the full catalog in the first viewport).
- web-cockpit.md: model page presents a shortlist of popular options with Show all, catalog API unchanged.

## Checklist

- [x] T1 shortlist + Show all + tests
- [x] T2 turn grouping + markdown + live CSS + tests
- [x] T3 DESIGN.md + web-cockpit.md
- [x] Review revise: delete unused TurnChapter.tsx; Show all appends after shortlist; work chapter stays collapsed while the answer streams; quote border 1px

## Validation

```bash
npm run web:test -- --run src/features/session/ModelPicker.test.tsx src/features/session/Markdown.test.tsx src/features/session/Transcript.test.tsx src/features/session/sessionThread.test.ts src/features/session/SessionChat.test.tsx src/features/session/LiveHead.test.tsx src/features/session/modelShortlist.test.ts src/features/session/sessionTurns.test.ts
npm run web:check
```

### Results (2026-08-19)

| Command | Exit |
| --- | --- |
| `npm run web:test -- --run …` (8 files, 139 tests) | 0 |
| `npm run web:check` | 0 |

## Deviations

- `workSummary` is still exported from `Transcript.tsx` with no remaining importer after `TurnChapter.tsx` was deleted.
- Impeccable detect on `styles.css` reported preexisting spring-easing / palette advisories outside this change; left untouched.

## Risks

- Shortlist rank uses substring match against live catalog ids; a future Cursor rename can shuffle which 10 appear until the rank list is updated.
- Grouping is presentation-only; replayed history with no user prompt still uses the legacy preamble path.
