# Continuous speech-to-text input

## Scope

Add host-side continuous STT for the Web Cockpit terminal, with an explicit
editable composer, authenticated streaming transport, replaceable local
provider, standalone `pause` completion command, and a Paste-adjacent Mic
shortcut. Remove only the visible toolbar `^C` button.

## Non-goals

- No cloud STT requirement or speech model in the browser.
- No service worker, manifest, offline mutation queue, or public STT endpoint.
- No change to PTY keyboard input, Ctrl+C, SIGINT handling, tmux ownership, or
  task lifecycle.
- No automatic Enter, execution, or prompt submission.

## Delegation

Delegation decision: delegated via model-router to the Cursor lane for bounded
implementation and review. The initial Pi lane hit monthly quota `429`; the
phrase-threshold wire-up and final audit fixes were subsequently handled through
Cursor with parent review.

## Execution ledger

- [x] Task 1 — update `architecture.md` before application code.
- [x] Task 2 (config slice) — `SttConfig` on `Config` with defaults/TOML tests.
- [x] Task 2 (protocol slice) — versioned `stt.*` types and audio frame helpers in `slices/stt.rs`.
- [x] Task 3 — pure `speechState` reducer and focused Vitest tests.
- [x] Task 4 — supervised `SttProvider` / `MoonshineProvider` adapter and focused tests.
- [x] Task 5 — authenticated task-scoped STT WebSocket (`/api/tasks/{handle}/stt`).
- [x] Task 6 — browser speech transport (`speechTransport.ts`) with focused Vitest tests.
- [x] Task 7 — safe editable `TerminalComposer` component and focused tests.
- [x] Task 8 — TaskTerminal speech wiring: Mic after Paste, remove toolbar `⌃C`.
- [x] Task 9 — update setup/operator documentation and examples.
- [x] Task 10 — run focused, full, and physical-iOS validation (physical device
  unavailable in this environment; documented as follow-up).
- [x] STT config wiring — `phrase_end_silence_ms` → provider session / sidecar start.

## Task details

### Task 1 — architecture

- Test: no application test; validate the required architecture headings and
  boundaries with repository search.
- Implementation: document ownership, provider interface, PCM transport,
  explicit state transitions, transcript lifecycle, VAD, spoken controls,
  terminal/authentication safety, failure recovery, iOS constraints, and Mic
  behavior.
- Verification: `architecture.md` contains the speech architecture section and
  the existing raw xterm/composer discrepancy is explicit.
- Result: complete. First modified file was `architecture.md`; 198 lines added.

### Task 2 — configuration and protocol

- Test: add failing Rust tests for STT config defaults/TOML parsing, centralized
  timing values, protocol version/session validation, and ordered audio sequence
  metadata.
- Implementation: add `SttConfig` and versioned control/event types using
  existing serde/config conventions; reject invalid or unbounded values.
- Verification: focused `ajax-core`/`ajax-web` tests, then `cargo fmt --check`.
- Gate: ask before starting implementation and again after the task passes.
- Config slice result: complete. `SttConfig` added with packet defaults,
  `deny_unknown_fields`, and two focused tests; `commands.rs` and
  `task_operations.rs` fixtures use `..Config::default()`.
- Protocol slice result: complete. `slices/stt.rs` adds `SttClientMessage`
  (start/stop/cancel), `SttServerEvent` (ready/partial/final/speech_started/
  speech_ended/error), and bounded audio frame encode/decode helpers.

### Task 3 — frontend state and transcripts

- Test: add failing Vitest tests for valid/invalid transitions, one active
  session, standalone `pause` normalization, pause timer cancellation, stale
  callbacks, partial replacement, final deduplication, and out-of-order finals.
- Implementation: add a small pure reducer/controller with explicit states and
  session identity checks.
- Verification: focused speech tests and `npm run web:check`.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. `speechState.ts` reducer with session/timer guards,
  standalone pause normalization (`.!?` terminal punctuation), explicit `error`
  action with `errorMessage`, ordered final segments, and eight focused tests.

### Task 4 — provider service

- Test: add failing Rust tests for provider health, startup failure, bounded
  buffering/backpressure, finalization, cancellation, restart isolation, and
  clean shutdown.
- Implementation: add the narrow provider interface and supervised local
  Moonshine sidecar adapter; keep model-specific code outside the main Ajax
  domain and PTY path.
- Verification: focused backend tests and provider health/startup checks.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. `SttProvider`/`SttProviderSession` traits, `MoonshineProvider`
  with unavailable/startup-failure paths, `BoundedAudioBuffer`, and typed
  sequence-aware `ProviderEvent`s; four focused tests pass.
  Sidecar seam: binary audio frames (`encode_sidecar_audio_frame`), NDJSON
  event parsing (`parse_sidecar_event_line`), stdin start/audio/finalize writes,
  and a bounded stdout reader feeding `poll_event`; six focused tests pass.

### Task 5 — authenticated STT WebSocket

- Test: add failing route tests for browser-session auth, same-origin checks,
  start/stop/cancel controls, bounded binary audio frames, provider-ready/events,
  stale session rejection, and provider errors.
- Implementation: add a task-scoped STT route separate from the PTY WebSocket;
  reuse the browser-session cookie and origin policy.
- Verification: focused `ajax-web` tests and protocol round-trip checks.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. `/api/tasks/{handle}/stt` reuses browser-session auth and
  same-origin upgrade gates; typed socket loop in `bridge_task_stt_socket`
  (separate from PTY); provider from `Config.stt` on `WebAppState`.

### Task 6 — browser audio and recovery

- Test: add failing frontend tests for one-tap capture, duplicate activation,
  PCM framing, ordinary silence, immediate speech resume, interruption,
  backgrounding, bounded queues, reconnect behavior, and resource release.
- Implementation: add user-gesture `getUserMedia`, host-compatible PCM16 audio,
  responsive frontend VAD, provider VAD event handling, and explicit errors.
- Verification: focused Vitest tests, type check, lint, and browser capability
  checks.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. `speechTransport.ts` owns one-shot mic/session/WebSocket
  lifecycle, PCM16 framing, local RMS VAD, visibility interruption, and resource
  release behind an injectable platform seam.

### Task 7 — safe composer integration

- Test: add failing component tests for preserving existing text, replacing
  partials, appending finals once, hiding the control word, preserving finals on
  cancel/error, no PTY writes, and explicit insert/send only.
- Implementation: add the minimum TaskTerminal-owned editable composer; keep
  xterm helper textarea and normal terminal input unchanged.
- Verification: focused component tests and existing terminal behavior tests.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. `TerminalComposer.tsx` renders editable value, separate
  partial preview, pause/error status, and explicit Insert-only submission.

### Task 8 — shortcut bar

- Test: add failing UI/source tests for `[Paste] [Mic] [remaining shortcuts]`,
  exact visible `Mic` text, accessible name/tooltip, active/connecting/
  finalizing/error states, mobile visibility, and no visible `^C` control.
- Implementation: remove only the toolbar `^C` entry and exclusive styling/assets;
  add Mic using existing shortcut styles and focus/touch behavior.
- Verification: focused UI tests plus keyboard Ctrl+C/PTy regression tests.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. TaskTerminal owns speech model/transport, composer, Mic
  after Paste (`aria-label="Start voice input"`), Cancel voice recovery; toolbar
  `⌃C` removed; Ctrl modifier and xterm Ctrl+C path retained.

### Task 9 — documentation

- Test: run existing documentation/configuration/static-asset checks; no fake
  application tests for prose-only changes.
- Implementation: document Moonshine installation, provider health, config
  examples, iOS permission/interruption behavior, and recovery operations.
- Verification: relevant Rust asset/config tests and documentation search.
- Gate: ask before starting implementation and again after the task passes.
- Result: complete. Added `docs/speech-input.md`; README links in Web Cockpit
  and Configuration sections. `rg` confirms all `[stt]` keys and setup/recovery
  topics. No application code or tests changed.

### Task 10 — final validation

- Test: run the repository's focused and full suites; manually exercise the
  real-iPhone Safari microphone lifecycle where automation cannot prove it.
- Implementation: no new feature work; fix only verified defects within scope.
- Verification: record every command and result, including skipped physical-iOS
  checks and the required local verify gate.

Task 10 status: complete.

## Deviations and findings

- The checkout has no persistent editable terminal composer. The architecture
  now records that the minimum composer surface must be added inside
  `TaskTerminal`; it is not a second terminal or PTY path.
- Existing Web Cockpit has an authenticated PTY WebSocket but no STT transport;
  speech must use a separate socket so binary terminal input semantics remain
  unchanged.
- Existing Web Cockpit intentionally has no manifest/service-worker dependency;
  speech will support Safari lifecycle behavior without reintroducing one.
- Pre-existing untracked `scripts/*` files were observed and will be preserved.
- Ajax Model Router dispatches to Pi MiniMax and its one allowed fallback, Pi
  GLM, both failed before execution with the provider's monthly usage-limit
  `429`; no delegate changed source files. Parent gated the existing
  `config.rs` implementation after local verification passed.
- Task 2 packet scope covered `SttConfig` only in `stt-config-r1`; protocol wire
  types completed in `stt-protocol-r1` slice (`slices/stt.rs`).

## Validation results

- Architecture search: pass.
- Task 2 red test: `rtk cargo test -p ajax-core config::tests::stt_ --lib` failed
  as expected because `Config`/`SttConfig` are not implemented yet.
- Protocol red test: `rtk cargo test -p ajax-web slices::stt::tests --lib` failed
  as expected because the versioned protocol types and audio helpers are not
  implemented yet.
- Delegated implementation: blocked before execution by provider quota; see
  `.planning/router-runs/stt-config-r1.log` and `stt-config-r2.log`.
- Local implementation (config slice): pass.
  - `rtk cargo test -p ajax-core config::tests::stt_ --lib`: 2 passed.
  - `rtk cargo test -p ajax-core config::tests --lib`: 20 passed.
  - `rtk cargo check -p ajax-core --all-targets`: pass.
  - `rtk cargo fmt --check`: pass.
- Two `Config { .. }` test helpers in `commands.rs` and `task_operations.rs`
  required `..Config::default()` for compilation after adding `stt`.
- Local implementation (protocol slice): pass.
  - `rtk cargo test -p ajax-web slices::stt::tests --lib`: 3 passed.
  - `rtk cargo check -p ajax-web --all-targets`: pass.
  - `rtk cargo fmt --check`: pass.
- Parent review gate for the protocol slice: `rtk git diff --check` also
  passed; the first delegated implementation was revised once to make the
  documented maximum payload inclusive.
- Local implementation (speech state slice): pass.
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechState.test.ts`: 8 passed.
  - `rtk npm run web:check`: pass.
- Parent review gate for the speech state slice: `rtk git diff --check`
  passed; one delegated revision added punctuation normalization and the
  explicit error transition.
- Local implementation (provider slice): pass.
  - `rtk cargo test -p ajax-web adapters::stt_provider::tests --lib`: 7 passed.
  - `rtk cargo check -p ajax-web --all-targets`: pass.
  - `rtk cargo fmt --check`: pass.
  Revised once for the sidecar binary frame + NDJSON event seam (parent review).
  Revised again so finalize stays open for event drain (`finalizing` vs `closed`).
- Local implementation (STT WebSocket slice): pass.
  - `rtk cargo test -p ajax-web runtime::tests::axum_task_stt --lib`: 3 passed.
  - `rtk cargo test -p ajax-web slices::stt::tests --lib`: 3 passed.
  - `rtk cargo test -p ajax-web adapters::stt_provider::tests --lib`: 7 passed.
  - `rtk cargo check -p ajax-web --all-targets`: pass.
  - `rtk cargo fmt --check`: pass.
- Parent review gate: `rtk git diff --check` passed. An attempted combined
  Cargo test command with two filters was invalid and was rerun as the two
  focused commands above; both reruns passed.
- Local implementation (speech transport slice): pass.
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/shared/lib/speechTransport.test.ts`: 5 passed.
  - `rtk npm run web:check`: pass.
- Parent review gate: `rtk git diff --check` passed; one delegated revision
  kept the socket open through finalization and added native-rate resampling.
- Local implementation (terminal composer slice): pass.
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/features/task/TerminalComposer.test.tsx`: 4 passed.
  - `rtk npm run web:check`: pass.
  Dual-render pause/finalizing case uses explicit `view.unmount()` (no manual cleanup).
- Parent review gate: `rtk git diff --check` passed; composer remains
  PTY-free and requires the explicit Insert action.
- Local implementation (TaskTerminal speech slice): pass.
  - `rtk npm run web:test -- --run crates/ajax-web/web/src/features/task/TaskTerminal.test.tsx`: 27 passed.
  - `rtk npm run web:check`: pass.
  Source-contract regex for `children: Mic` adjusted to JSX text child; assistive
  Control C aria assertion updated after toolbar `⌃C` removal.
- Full verify: pending later tasks.
- Parent red-test follow-up (status announce / restart from error): pass.
  - Status region announces Connecting… / Listening / Finalizing…; pause/error preserved.
  - `start` from `error` opens a fresh connecting session; active states still block.
  - `activateMic` allowed from idle and error; cancels stale transport first.
  - `rtk npm run web:test -- --run …/TerminalComposer.test.tsx`: 7 passed.
  - `rtk npm run web:test -- --run …/TaskTerminal.test.tsx`: 28 passed.
  - `rtk npm run web:test -- --run …/speechState.test.ts`: 9 passed.
  - `rtk npm run web:check`: pass.
- `rtk npm run web:lint`: pass (removed manual `cleanup`; braced `case "final"`).
- Task 9 documentation: pass.
  - Added `docs/speech-input.md`; README cross-links in Web Cockpit and Configuration.
  - `rg` confirms all `[stt]` keys and setup/recovery topics in docs.
  - No application code or tests changed (prose-only per packet).
- Final audit fixes:
  - Added capture-setup failure regression coverage; transport now reports the
    error and releases the microphone/socket.
  - Added unexpected STT WebSocket close recovery; active speech transitions to
    an explicit error while finalization still completes normally.
  - Wired centralized `phrase_end_silence_ms` through the provider session and
    sidecar start metadata, with focused coverage.
  - Architecture guard now includes the new `stt` slice and `stt_provider`
    adapter.
- Verify gate: first run stopped at `clippy::manual_ok_err` in the new provider
  adapter; Cursor applied the idiomatic fix. A complete rerun then passed the
  workspace checks, 1,772 Rust tests, doc tests, 541 web tests, AST scan, and
  CI/release validation.
- Final verify rerun: pass. `npm run verify` completed formatting, all-target
  check, clippy, 1,773 Rust nextest tests, doc tests, TypeScript check, ESLint,
  AST scan, 543 Vitest tests, and CI/release scripts. jsdom emitted the known
  xterm canvas `getContext` warnings while tests still passed.
- Physical iPhone/Safari microphone lifecycle validation was not available in
  this environment; it remains the operator follow-up before release.
- STT config wiring (`stt-config-wiring-r1`): pass.
  - `ProviderSessionConfig.phrase_end_silence_ms` + sidecar `phraseEndSilenceMs`.
  - Wired through `MoonshineProvider`, `WebAppState`, and `bridge_task_stt_socket` from `Config.stt`.
  - `rtk cargo test -p ajax-web adapters::stt_provider::tests --lib`: 8 passed.
  - `rtk cargo test -p ajax-web runtime::tests::axum_task_stt --lib`: 3 passed.
  - `rtk cargo fmt --check`: pass.
  - `rtk cargo clippy -p ajax-web --all-targets --all-features -- -D warnings`: pass.

Task 8 status: complete.
Task 9 status: complete.
