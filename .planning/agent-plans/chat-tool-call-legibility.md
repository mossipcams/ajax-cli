# Chat conversation flow and tool-call detail legibility

Status: **approved**; pass 1 and pass 2 complete and accepted; opening PR from the
worktree. Mobile-webkit smoke remains blocked locally by a broken Playwright WebKit
(see pass 2 verification); CI web-e2e is the first real run.

Tracking issue: [mossipcams/ajax-cli#1020](https://github.com/mossipcams/ajax-cli/issues/1020)
(covers the two defect-class findings: mid-token truncation and clipped output).

## Why

The collapsed turn summary works. Everything behind the turn disclosure does
not: the "extra details" a tool call reveals are mostly content the operator
did not ask for, rendered so that the one line worth reading is the one that
gets clipped.

Evidence was gathered by mocking a realistic turn (search → read → edit →
failing `cargo nextest`, plus a plan and two reasoning items) through the real
session WebSocket and rendering it in Playwright at 390px and 1024px. Findings,
worst first:

1. **A read's detail is the whole file.** `ToolCard` renders every ACP content
   block it is handed; for a read that content is the file text, so opening a
   read spends ~600px of phone screen on source that answers nothing the row's
   path already answered.
2. **Rows do not say what happened.** `toolTarget` returns only the path, so a
   read and an edit of the same file are two visually identical lines separated
   by a faint one-character glyph (`◦` vs `±`). With no location it falls back
   to the tool's own name, so the column mixes `Search files` with
   `…/gateway/serve.rs` and a shell command — three kinds of noun in one list.
3. **Truncation produces gibberish.** `middleSplit` holds back a fixed 14
   characters, so the phone renders `$ cargo nextest run -p… ures gateway::`.
4. **Machine text does not wrap.** `.session-tool-output` and
   `.session-diff-body` are `white-space: pre` with `overflow: auto`, so the
   assertion explaining a failure is cut at the right edge and every block is
   its own horizontal scroller.
5. **A failure looks like a file dump.** Search results, read dump, diff and
   failing test output are four identical tinted boxes with the same hairline;
   the only failure signal is micro-uppercase `FAILED` in the far-right column.
6. **Nesting eats the phone.** Thread padding + 12px disclosure rail + 12px
   margin + 24px card-body indent + block padding ≈ 60px of a 390px screen
   before the first character of code.
7. **Reasoning truncates mid-word** (`…search for the li…`) and sits in the same
   grid as tool calls, so it reads as another tool call.
8. **Status chrome is duplicated.** The task header shows `? WAITING` while the
   sticky head repeats `NEEDS YOU / Waiting for review / Review` — roughly a
   quarter of the phone viewport, permanently, saying the same thing twice.

## Scope

Web Cockpit chat presentation only:

- `crates/ajax-web/web/src/features/chat/ToolCard.tsx`
- `crates/ajax-web/web/src/features/chat/toolPresentation.ts`
- `crates/ajax-web/web/src/features/chat/Transcript.tsx`
- `crates/ajax-web/web/src/features/chat/LiveHead.tsx` (pass 2 only)
- `crates/ajax-web/web/src/features/chat/sessionThread.ts` (`thoughtSnippet` only)
- `crates/ajax-web/web/src/styles/session/activity.css`
- `crates/ajax-web/web/src/styles/session/transcript.css`
- the vitest files beside the above

## Non-goals

- No change to `sessionThread.ts` reducer semantics, event handling, or ordering.
- No change to the ACP wire contract, transport, or what the host sends.
- No new dependency, no icon set, no syntax highlighting, no virtualised list.
- No change to the collapsed turn summary's information (it already works);
  only the filler phrase `1 other step` is in play.
- No change to task lifecycle, registry truth, or any Rust crate.

## Pass 1 — what the detail says (content)

One bounded change: the rows name their action and the blocks stop dumping.

- [x] Rows read verb-first: `Read serve.rs`, `Edited serve.rs`,
      `Ran cargo nextest …`, `Searched files`. Reuse the `OPERATION_VERBS` map
      that already exists in `Transcript.tsx` for the live line rather than
      adding a second vocabulary; move it to `toolPresentation.ts` so the row
      and the live line cannot drift.
- [x] Two calls of different kinds against the same path never render identical
      row text.
- [x] Text content is capped to a short preview (~8 lines) with the remaining
      line count on a control that expands to the full text. The preview shows
      the **tail** for a failed call (a panic is at the end) and the head
      otherwise.
- [x] A completed read is not auto-expanded regardless of preview length; its
      row is the whole answer.
- [x] `middleSplit` breaks at a token boundary (path separator, whitespace, or
      punctuation) so a shortened command still reads as a command. Issue #1020.
- [x] Output and diff blocks wrap long lines instead of clipping; a failing
      assertion is readable at 390px with no horizontal scrolling. Diff lines
      keep their leading alignment when wrapped (hanging indent on
      `.session-diff-line`). Issue #1020.
- [x] `thoughtSnippet` cuts on a word boundary.

Verification for pass 1:

- [x] New/updated vitest cases in `toolPresentation.test.ts` for verb-led
      labels, token-boundary truncation, and word-boundary snippets.
- [x] New/updated vitest cases beside `ToolCard`/`Transcript` for the preview
      cap, the tail-preview-on-failure rule, and the not-auto-expanded read.
- [x] `npm run web:test -- --run` (116 files, 1206 tests, pass),
      `npm run web:check`, `npm run web:lint`, `npm run web:sg` — all pass.

## Pass 2 — how the detail looks (layout and emphasis)

Starts only after pass 1 is accepted.

- [x] One indent for the whole disclosure: blocks sit flush to the rail instead
      of stacking a second 24px card-body indent, recovering roughly 40px of a
      390px screen.
- [x] A failed call's expanded block is visually distinct from a file dump
      (danger edge on the block, not just a far-right word), and the failure is
      the strongest thing in the expanded turn.
- [x] The four block types are distinguishable at a glance rather than four
      identical tinted boxes. First attempt declared the `.session-block-*`
      rules before `.session-tool-output` at equal specificity, so they were
      dead CSS; fixed with compound selectors and a computed-style guard.
- [x] Reasoning rows are visually distinct from tool rows.
- [x] Re-scoped and done, with the user's approval to touch the shared header.
      `LiveHead` no longer repeats `NEEDS YOU / Waiting for review`, showing one
      explanation plus actions (`isTaskLevelAttention`). The header pill is now
      opt-out through a `showStatusPill` prop on `TaskWorkspaceHeader`, and only
      `TaskWorkspace`'s `mode === "chat"` branch passes `false` — the terminal
      and diff surfaces keep the pill, since they have no live head and it is
      their only statement of task state. An earlier attempt suppressed it with
      `.session-chat-surface .detail-header .interact-pill { display: none }`;
      that was rejected as editing across the boundary and reverted.
      The `?` was a mangled glyph in `.interact-pill.tone-waiting::before` (every
      other tone in that rule set uses a real mark: `▸ ! ✓ ·`), restored to `◦` —
      the same mark the chat row column already renders in `var(--mono)`, which
      is the face the pill uses. Defect tracked by
      [#1020](https://github.com/mossipcams/ajax-cli/issues/1020); regression
      test in `TaskWorkspaceHeader.test.tsx`.
- [x] Drop the `1 other step` filler from the collapsed summary.

Verification for pass 2:

- [x] `crates/ajax-web/web/e2e/visual.test.ts` extended with two 390px
      computed-style guards: the failure block paints its own surface and the
      disclosure indents once; and the three text block kinds do not all paint
      the same surface.
- [x] `npm run web:test -- --run` (1214 tests pass), `npm run web:check`,
      `npm run web:lint`, `npm run web:sg` — all pass.
- [x] `npm run web:smoke:desktop`: 54 passed, 2 failed. Both failures are in
      `e2e/session-chat-keyboard.test.ts`, asserting iOS visual-viewport
      keyboard geometry (`surfacePaddingBottom >= 250`) that only the mobile
      path produces; that file and the composer/layout/shell CSS it depends on
      are untouched by this change.
- [ ] **Blocked, environment.** `npm run web:smoke` (mobile-webkit) cannot run
      on this machine: every test fails with `timeout while setting up "page"`,
      and a bare `webkit.launch()` + `newPage()` outside the repo hangs past
      two minutes. Needs a Playwright WebKit reinstall before the mobile gate
      can be trusted.
- [ ] Re-render the mocked transcript at 390px and confirm findings 1–8 are gone.

## Routing

Each pass is one `EXECUTION` decision through `model-router`, dispatched to the
Cursor delegate via `run-delegate` (acpx). Note: `scripts/run-delegate` is a
symlink present in the primary checkout but absent in this worktree; dispatch
uses the runner at `ajax-model-router/scripts/run-delegate` directly. The parent
reviews the delta against the acceptance criteria above before pass 2 starts.

## Deviations and open questions

- Wrapping diff lines is a judgement call: `pre` alignment is what makes a diff
  readable, and wrapping breaks column alignment. Plan is to wrap with a hanging
  indent so the sign column stays legible; if that reads badly, diffs keep
  horizontal scroll and only output blocks wrap.
- Pass 2's removal of duplicated status chrome touches the shared task
  workspace header, not just the chat feature. If the delegate finds the header
  is owned by another surface with its own contract, it stops and the parent
  re-scopes rather than editing across the boundary.
