# Status and task contract

```yaml
PACKET_STATUS: READY
TASK_KIND: tests-only
TEST_FIRST: NOT_APPLICABLE
PRODUCTION_EDIT: FORBIDDEN
BLOCKERS: []
```

# Goal

Pin Ajax's engine-neutral PTY-output delivery contract: streaming UTF-8,
Unicode/ANSI/CRLF pass-through, rapid/large ordered chunks, and a live browser
surface that remains connected and error-free while that corpus crosses the
mocked WebSocket boundary.

# Allowed files

- `crates/ajax-web/web/src/terminalConnection.test.ts`
- `crates/ajax-web/web/e2e/fixtures.ts`
- `crates/ajax-web/web/e2e/terminal-behavior.test.ts`

# Forbidden changes

- All production code and every other test/config/dependency/lock/generated/
  docs/planning file.
- No renderer class/type, DOM, canvas, buffer/probe, screenshot, output cadence,
  batching constant, or future adapter/controller assertion.
- Never run git mutation commands (including stash).

# Context evidence

- Graphify: architecture already fixes the relevant boundary: backend emits PTY
  bytes; `terminalConnection.ts` decodes them and invokes public `onOutput`;
  browser surface remains presentation-only.
- Serena: NOT_REQUIRED; public `connectTaskTerminal` and exact existing mock
  anchors are known; no production semantic edit is allowed.
- ast-grep anchors: `describe("connectTaskTerminal"` in
  `terminalConnection.test.ts`, `emitMessage` in the e2e mock socket, and the
  existing permanent suite imports in `terminal-behavior.test.ts`.
- Existing transport tests cover ArrayBuffer, Blob, legacy base64 JSON, error
  JSON, and resize JSON, but do not cover split UTF-8 or ordered burst corpora.

# Code anchors

- Append focused `connectTaskTerminal` tests using its public events:
  1. split the byte sequence of an emoji/wide character across consecutive
     binary frames and assert joined `onOutput` equals the original text once;
  2. pass a corpus containing ASCII, emoji, combining mark, CJK wide glyph,
     ANSI SGR/erase, `\r`, `\n`, and `\r\n`; assert exact order/content;
  3. emit a rapid bounded burst and a bounded large payload; assert joined
     output has no loss, duplication, reorder, or thrown error.
- Generalize only the e2e mock's `emitMessage` payload type as needed and export
  `emitLatestTerminalOutput(page, chunks: Array<string | number[]>)`, where
  `number[]` becomes `Uint8Array` inside the page. Target the latest `/terminal`
  socket and preserve current helpers/defaults.
- Add one browser test that installs a `pageerror` collector before route load,
  sends the same representative corpus through the helper, then asserts the
  stable surface remains visible, exactly one socket remains active, and no
  application error occurred. Do not claim glyph visibility automatically;
  the acceptance matrix will mark that physical/manual.

# Test-first instructions

NOT_APPLICABLE per tests-only contract. Optional OTHER evidence may show a
missing e2e helper before it is added; no router RED claim is required.

# Edit instructions

- Reuse the existing `MockWebSocket` and event callbacks; no new test framework.
- Await asynchronous message delivery deterministically with `vi.waitFor` or
  `expect.poll`; no sleeps.
- Keep burst sizes bounded enough for fast CI while still detecting duplication
  and reorder (for example 100 numbered chunks and one 128KB payload).
- Do not test terminal-emulation correctness or inspect Ghostty output.

# Verification commands

```bash
npx vitest run src/terminalConnection.test.ts
npx playwright test --config crates/ajax-web/web/playwright.config.mts --project=mobile-webkit crates/ajax-web/web/e2e/terminal-behavior.test.ts
rg -n "ghostty|xterm|canvas|__ajaxTerminalProbe|data-terminal-engine|waitForTimeout" crates/ajax-web/web/e2e/terminal-behavior.test.ts
```

# Acceptance criteria

- Split UTF-8 reassembles exactly once.
- Unicode/control corpus and bursts preserve exact bytes/text order.
- Large bounded output causes no test/application error or event duplication.
- Permanent browser suite remains engine-neutral and connected.
- All focused tests pass; forbidden-token rg has no matches.

# Stop conditions

- Production edit or renderer seam is required.
- More than three allowed files are needed.
- Current public transport loses/reorders output (leave failing test and report).
- Any git mutation is attempted.
