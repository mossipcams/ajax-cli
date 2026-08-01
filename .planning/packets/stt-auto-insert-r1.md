PACKET_STATUS: READY
DISPATCH_LEVEL: compact
UNRESOLVED_UNCERTAINTY: NONE
BLOCKERS: []

## Task

Speech currently stages text in a `TerminalComposer` textarea that the operator must
review and then press "Insert transcript" to send. That staging step is being
removed: final transcripts must go straight into the terminal as they arrive.

1. **Delete** `TerminalComposer.tsx` and `TerminalComposer.test.tsx`.

2. In `TaskTerminal.tsx`, remove the composer wiring: the `TerminalComposer` import
   and its JSX element, `composerValue` / `setComposerValue`, `speechPrefixRef`,
   `joinComposerTranscript`, and `insertComposerTranscript`.

3. **Auto-insert each new final segment.** Add a ref that tracks which final
   sequences have already been written, e.g.
   `const insertedFinalsRef = useRef<Set<number>>(new Set());`

   In the `onFinal(sequence, text)` callback passed to `createSpeechTransport`,
   before dispatching the reducer action:
   - skip when `isStandalonePause(text)` is true (import it from
     `@/shared/lib/speechState`) — that word is a control command, not dictation;
   - skip when `insertedFinalsRef.current.has(sequence)` — the provider may resend;
   - otherwise add the sequence to the set and call `pasteThroughTerm(text, false)`.

   Then dispatch the existing `final` action exactly as it does today.

   Clear the set in `activateMic()` when a new session starts and in
   `cancelSpeechInput()`.

   **The paste must not happen inside a `setState` updater.** React may invoke an
   updater twice under StrictMode, which would write the text to the PTY twice. Do
   the insertion in the `onFinal` callback, not inside `dispatchSpeech`.

4. Remove the now-dead composer branch in `dispatchSpeech` — the block that compares
   `next.finalTranscript` to `previous.finalTranscript` and called
   `setComposerValue`. `dispatchSpeech` becomes a plain
   `setSpeechModel((previous) => speechReducer(previous, action))`.

5. **Keep a compact status line** where the composer used to be, so the operator
   still gets mic feedback. Render, inside a `role="status"` element with class
   `terminal-speech-status`, only:
   - `Connecting…` when state is `connecting`
   - `Listening` when state is `listening`
   - `Finalizing…` when state is `finalizing`
   - `Pausing in {pauseCountdownSeconds}…` plus `Speak to continue` when state is
     `pause_pending` and the countdown is defined
   - the error message when state is `error`

   No textarea, no partial-transcript display, no insert button. Leave the existing
   "Cancel voice input" button and the Mic toolbar button exactly as they are.

6. In `styles.css`, delete the `.terminal-composer`, `.terminal-composer-input`,
   `.terminal-composer-partial`, and `.terminal-composer-status` rules and the
   iOS-focus-zoom `.terminal-composer-input` override in the media query. Add a
   `.terminal-speech-status` rule carrying the same compact styling the old
   `.terminal-composer-status` had, including its `:empty { display: none }`.

7. In `TaskTerminal.test.tsx`, update assertions that reference `TerminalComposer` so
   they cover the new behavior instead. Do not weaken unrelated assertions.

## Allowed files

- `crates/ajax-web/web/src/features/task/TaskTerminal.tsx`
- `crates/ajax-web/web/src/features/task/TerminalComposer.tsx` (delete)
- `crates/ajax-web/web/src/features/task/TerminalComposer.test.tsx` (delete)
- `crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`
- `crates/ajax-web/web/src/styles.css`

## Forbidden changes

- Any file outside `Allowed files`. Do not touch `speechState.ts`,
  `speechTransport.ts`, `dist/`, or any Rust file.
- Do not modify `CONTROL_KEYS`, the terminal toolbar, the Mic button, the Cancel
  voice input button, `pasteThroughTerm`, or any layout outside the composer slot.
  The absence of a `⌃C` entry is intentional — do not re-add it.
- Do not change the reducer, its actions, or `speechReducer` behavior.
- Do not change the pause countdown effect or `pauseCountdownSeconds`.
- Renames, formatting sweeps, import reordering, drive-by cleanup.
- Commits, branches, pushes, merges, rebases.

## Acceptance

- `TerminalComposer.tsx` and `TerminalComposer.test.tsx` no longer exist, and a
  repository grep for `TerminalComposer` returns no hits.
- No textarea is rendered by `TaskTerminal`, and there is no "Insert transcript"
  button.
- Each new final transcript segment is written to the terminal exactly once, via
  `pasteThroughTerm`, at the moment it arrives.
- A final whose text is a standalone `pause` is never written to the terminal.
- A repeated final for a sequence already inserted is not written twice.
- Starting a new mic session, or cancelling, clears the inserted-sequence set so a
  later session with overlapping sequence numbers still inserts.
- No call to `pasteThroughTerm` occurs inside a `setState` updater callback.
- The status line shows the listed states and renders nothing when idle.
- `.terminal-composer*` rules are gone from `styles.css`.

## Verification

Run and report actual results for, from the repository root:

- `npm run web:check` — must pass.
- `npm run web:lint` — must pass.
- `npm run web:sg` — must pass.
- `npm run web:test -- --run` — must pass.

Add a test proving a standalone `pause` final is not inserted while an ordinary
final is, and a test proving the same sequence is not inserted twice.

## Stop if

- The change cannot be made without editing a file outside `Allowed files`.
- Removing the composer breaks a `TaskTerminal.test.tsx` assertion that is unrelated
  to speech — report which and stop.
- The patch would exceed roughly 250 changed lines.
