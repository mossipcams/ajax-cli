# Session chat visual UX pass

## Scope

Make orchestration chat read as a conversation, not a TaskDetail dump.

- Thread = messages + transport artifacts only
- Task meta (status/activity/annotations/WebActions/Show diff) in a collapsed strip
- Style agent bubbles; `pre-wrap`; auto-scroll
- Composer: Enter sends; Terminal + Cancel secondary; Send primary
- Drop draft-clobber Retry / Try another approach macros

## Non-goals

- Markdown renderer
- Durable history / ACP grace
- Flag-off TaskDetail restyle

## Delegation

`Delegation decision: not delegated because presentation pass is a single tight UI edit the lead already inspected end-to-end; smaller than a useful work-order round-trip.`

## Checklist

- [x] SessionChat layout + styles + tests
- [x] Verify session tests (13 pass) + web:check
- [x] Commit + push PR 779
