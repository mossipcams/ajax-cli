# Ajax Chat tool rows name the target, not the tool

Status: **approved for implementation.** User asked to address the visual
defects in conversation flow / tool calling after reviewing the live
`ajax-cli/acp-reliability` transcript.

Branch: `ajax/chat-best-practices`.

## Why

`web-session-behavior.md` says a tool row is one line on the activity grid
and that conversation text is proportional while paths and commands are
monospace. The presentation layer already assumes the useful field is the
target: `toolRowLabel` is documented as "a path beats the tool's name —
`Read File` is the same on every read."

On the live reliability session that contract is not met. 210 unique tool
calls, 622 events:

- `locations` never present
- `content` present on 2 edits only
- 121 reads titled only `Read File` → UI **Read Read File**
- 38 searches titled `grep` / `Find` → UI **Searched files**
- 2 edits titled `Edit File` → UI **Edited Edit File**
- execute titles are the raw command, including multi-KB `python -c` dumps

Cursor sends the path, query, and command on ACP `rawInput`. Ajax maps only
`title` / `kind` / `status` / `locations` / `content`. Updates then arrive as
`{ title: "", kind: "", status: "in_progress" }` with still no locations, so
the generic first title sticks and the target never appears. Always-visible
tool rows (B1) made that emptiness the conversation.

This is a confirmed defect against the existing row contract, not a new
capability. `rawInput` / `rawOutput` as richer activity cards stays out of
scope (still the deferred line in `acp-utilization.md`).

Dedup 2026-08-26: no open issue. New issue opened with this change.

## Scope

- Host mapping of an existing ACP tool-call field into the existing
  `locations` (or title) the browser already renders
- `toolRowLabel` / `toolRowTarget` so a generic tool name is not verb-prefixed
  into `Read Read File`, and execute titles do not dump whole scripts
- `docs/architecture/web-session-behavior.md` §Transcript composition: one
  sentence that the row target comes from location, else `rawInput` path /
  query / command, else a cleaned title
- focused host and presentation tests

## Non-goals

- No `rawOutput` rendering, syntax highlighting, or new card chrome
- No change to reducer keying, turn grouping, disclosure preference, or B1
  (rows stay visible; they must become readable)
- No ACP wire / protocol change beyond mapping a field Ajax already receives
- No Rust crate outside `ajax-web` web-session mapping
- No always-expand of tool bodies

## Implementation

- [x] Open GitHub defect (Web Cockpit).
      → [#1090](https://github.com/mossipcams/ajax-cli/issues/1090)
- [x] Host: when `locations` is empty, derive one target from `rawInput`
      (`path`, then `query` / `pattern` / `glob`, then `command`) and from a
      diff's `path` if content already has one. Empty update fields must not
      wipe a previously derived target (existing `title || previous` /
      `locations.length ? incoming : previous` merge stays).
- [x] Presentation: `toolRowLabel` uses the derived target. If the only
      remaining title is a generic tool name (`Read File`, `Edit File`,
      `Find`, `grep`, `Search files`), do not emit `Read Read File` — show
      `Read` / `Searched` / `Edited` until a real target exists. Execute
      rows show a short command (first line / first clause), not a prompt
      dump.
- [x] Regression: host fixture with Cursor-shaped `rawInput.path` and no
      `locations` produces a location the browser would render as
      `Read serve.rs`. Presentation fixture: `Read File` + no location is
      not `Read Read File`; `Read File` + path is `Read serve.rs`. Name the
      issue number.
- [x] Update `web-session-behavior.md` in the same change.

## Approval status

User requested the defects addressed. Implement now. Update this checklist
as work lands.

## Validation

- focused `ajax-web` mapping tests → pass (parent reran the three mapping tests)
- `npm run web:test -- --run` → 1409 passed, 9 skipped (delegate)
- `npm run web:check` → clean (delegate)
- `npm run web:lint` → clean (delegate)

## Material deviations

None yet. `rawInput` as a full arguments panel remains deferred.
