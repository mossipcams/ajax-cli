# Terminal sloppy-behavior fixes

From the close behavior review. Five slices, smallest-risk order; each
behavior change gets its test updated/added first.

1. **"New output ↓" must not focus the terminal** (same bug class as the
   #280 expand fix): tapping it is a user gesture, so `term.focus()` pops the
   iOS keyboard — shrinking the band and freezing the grid just to *read*
   output. Update the existing test's `focus` assertion to `not.toHaveBeenCalled`
   (deliberate behavior fix, mirroring the expand contract), then drop the
   `term.focus()` from `jumpToBottom`.
2. **Split clipboard feedback out of the connection banner state.**
   `statusDetail` currently carries both server error frames and clipboard
   messages: a paste failure renders under a "Connected" label, and a
   successful paste clears server errors it never owned. New `pasteNotice`
   state rendered in the same status strip; paste success clears only its own
   notice. Failing test first: a server error frame stays visible after a
   successful paste.
3. **Pass the whole scroll amount to `term.scrollLines`.** The ±1 loop
   re-implements what xterm's API already takes, and the tests pin the loop.
   Update assertions to net-lines-scrolled (sum of call args) — equivalent
   behavior coverage — then delete the loop.
4. **Narrow the connection's catch to JSON parsing.** Today any exception in
   decode/write is swallowed and "handled" by writing raw JSON into the
   terminal. Add a characterization test for the intended fallback (a
   non-JSON frame passes through as raw text), then scope the try to
   `JSON.parse` only so decode/write bugs surface instead of being hidden.
5. **Fix lying docs/names** (mechanical, no behavior): terminalTouchScroll's
   header still describes the deleted TerminalPanel + synthetic wheel events;
   the notch-clamp comment claims it protects the PTY (touch scroll never
   reaches the PTY); the test file's identifiers/describe still say
   "TerminalPanel".

Out of scope (flagged separately): dead Binary input path in terminal_pty.rs
and multi-format decode (WS contract narrowing), app-wide pinch suppression
(product/accessibility call), refit scheduler naming (pinned + load-bearing).

Validation per slice: focused vitest; final: web:check, web:test --run,
web:build (dist snapshots), cargo fmt/clippy/nextest.
