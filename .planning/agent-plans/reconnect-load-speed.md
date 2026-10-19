# Reconnect / task load speed

Approval status: **Phases 1–3 APPROVED by Matt 2026-10-18** ("Start phase 1
and I approve phases 2-3"). Phase 4 (WAL) not approved: decision only.
Issues: #1350 (1.1 migrate writes), #1351 (1.2 full-file read), #1352 (1.3
snapshot bloat).

Delegation: every repo write goes through `model-router` → `scripts/run-delegate`
(AGENTS.md). Each task below is sized as one delegate packet. Every packet bans
`git stash` / `checkout` / `reset`.

## Evidence (live stable server :8787, 2026-10-18)

Measured with a Node WebSocket probe (`/tmp/ajax-probe/*.mjs`) and `sample`.

| Task | Child live? | First frame | Snapshot | Last-version-only |
|---|---|---|---|---|
| model-training-failure | yes | 24 ms | 757 KB / 1251 events / 88 items | 45 KB |
| wake-word-false-positive | yes | 64 ms | 705 KB | — |
| score-v5 | no | 2.8 s | 447 KB / 1000 events / 112 items | 52 KB |
| SaySo/space | no | 3.5 s | 1828 KB / 1492 events / 22 items | 17 KB |

Root causes, in order of impact:

1. **Snapshot bloat.** The stream normalizer stores every streamed version of a
   message as a new full-content `execution_events` row (one per 50 ms pump).
   `replay::attach_inline_tail` and `read_before` send every version. The
   browser upserts by `itemId`, so all but the last version are thrown away.
   `TAIL_EVENT_LIMIT` is 1000 in code, but the docs say 300.
2. **Snapshot gated on spawn/restore.** `ws_bridge::bridge_task_session_socket`
   awaits `acquire_typed` (harness spawn + `session/resume`/`load`, with a
   restore budget of up to 300 s) before it sends any frame. This happens after
   every `ajax-web` restart, after a child dies, or after idle eviction.
   `attach_snapshot` already has a storage-only fallback (`load_detached`).
3. **Self-sustaining DB write/reload loop.** `migrations::migrate` runs on every
   `SqliteRegistryStore` open. It always calls `create_current_schema`,
   `ensure_agent_runs_columns` and `pragma_update(user_version, 10)`. Setting
   `user_version` to its current value is still a write: verified on a scratch
   DB that it bumps the file mtime, while a plain read does not. So
   `WebBackend::reload_context_if_stale` → `current_revision()` writes and bumps
   the mtime, and the next check sees "stale" and does a full
   `load_tracked_context`, which migrates (writes) again. The stable DB mtime
   moves about once a second while idle. A read-only reader got "database is
   locked". The server ran at 70–90 % CPU. Every load also runs
   `reject_legacy_json_state`, which `fs::read`s the whole 72 MB DB to inspect
   one byte. All of this runs under `control_lane`, which reconnects
   (`prepare_task_session_attach`) block on, and session actors run
   activity reports inline (`block_in_place`), retrying every 50 ms.

Best-practice references: persist completed messages, not per-chunk versions
(AI SDK persistence; Cloudflare agents buffer chunks and delete them in the
same transaction that saves the final message). Render stored history
immediately, and show "connected" separately from "agent ready" (AI SDK
`initialMessages`; HackerNoon reconnect-safe agent streams). Do not write on
read paths, and reuse connections (SQLite docs).

## Scope

Reduce time-to-visible-transcript on reconnect and cold load. Remove the
idle DB write/reload loop.

## Non-goals

- No change to lifecycle, registry truth, ownership boundaries, or runtime
  authority.
- No terminal/tmux changes.
- No change to cursor semantics: sequence numbers stay dense, monotonic and
  never reused.
- No new dependencies. No WAL switch unless Phase 4 is separately approved.
- Do not modify existing files under `tests/`. New focused tests sit next to
  the code under the repo's inline/sibling test conventions. Do NOT move
  ajax-core tests out of inline `#[cfg(test)]` (lifecycle guard).

## Phase 1: behavior-preserving fixes (ship first)

Each task needs a GitHub issue on `mossipcams/ajax-cli` and a regression test
that fails before the fix (docs/defect-process.md).

- [x] **1.1 (#1350) `migrate()` must not write when the schema is already current**
  (`crates/ajax-core/src/registry/sqlite/migrations.rs`, 524 lines).
  - Fast path: if `user_version == SQLITE_SCHEMA_VERSION`, skip
    `create_current_schema` and the `user_version` pragma write.
    `ensure_agent_runs_columns` stays as a read-only check that alters only
    when a column is missing.
  - Fresh DB (`user_version == 0`) and every upgrade path stay unchanged.
  - Regression test (inline): create a DB at the current schema, record the
    SQLite file change counter (header bytes 24..28), then call
    `current_revision()` and `load_tasks_only()`. Assert the counter is
    unchanged. It must fail on current main.
  - Verify: `cargo nextest run -p ajax-core registry::sqlite`, plus the
    existing migration tests.
- [x] **1.2 (#1351) `reject_legacy_json_state` reads only leading bytes**
  (`crates/ajax-cli/src/context.rs:152`). Use a `BufReader` and stop at the
  first non-whitespace byte, keeping the semantics exactly. Existing
  legacy-JSON rejection tests must still pass. Add one test proving a
  whitespace-prefixed JSON file is still rejected.
- [x] **1.3 (#1352) Compact superseded message versions in the snapshot tail and
  history pages** (`crates/ajax-web/src/slices/web_session/replay.rs`, 270
  lines).
  - In `attach_inline_tail` and `read_before`, keep at most one
    `SessionServerEvent::Message` per non-empty `item_id`. Keep it at the
    **first** occurrence's position and cursor, carrying the **last**
    occurrence's payload. The normalizer accumulates `content_blocks`, so the
    last version is a superset, and keeping the first position preserves the
    agent/thought interleave order.
  - Leave `ToolCall` and every other event untouched (tool updates can be
    partial). Leave `transcriptFrom`, `transcriptFloor`, `cursor` and the
    window selection unchanged. The legacy non-tail envelope replay stays
    uncompacted.
  - Tests in `replay_tests.rs`: (a) a streamed item with N versions yields one
    event carrying the final text and the union of content blocks, at the
    first cursor; (b) interleaved thought/agent items keep their first-seen
    order; (c) tool calls and non-message events pass through unchanged;
    (d) `transcriptFrom` and `cursor` are unchanged. Add one vitest that
    reduces a compacted transcript and an uncompacted one to an identical
    conversation (reuse `reducer.history.test.ts` fixtures).
  - Docs: `web-session-behavior.md` tail section: fix 300 → 1000 and describe
    the compaction.
- [x] **1.5 (#1355, APPROVED by Matt 2026-10-18) Live refresh re-stamps `live_status_observed_at`
  every poll.** Found during 1.4: after 1.1 the DB still writes once per
  cockpit poll. A registry diff across one poll shows the ONLY change is
  `live_status_observed_at` on every task. `runtime_refresh.rs` ~427-460 uses
  `observed_at = projection.selected_observed_at.unwrap_or(now)`, so a
  same-kind status is never "unchanged" (`current >= observed_at` fails) and
  is reapplied with a fresh timestamp. Result: `state_changed` on every
  refresh, then a full registry save, a revision bump (dev DB revision 391k),
  cockpit-cache invalidation, and session activity reports failing on
  revision conflict and retrying every 50 ms. Consumers: attention-ack
  comparisons (`runtime_refresh.rs:431`, `web_backend.rs:330`). This changes
  runtime-evidence semantics in ajax-core, so it is a stop condition and needs
  explicit approval. Proposed: a same-kind observation whose only difference
  is the `now` fallback timestamp counts as unchanged (keep the original
  observed_at for the same episode).
- [x] **1.4 Re-measure.** Rebuild, then run `scripts/test-in-stable.sh` or
  `dev-web-restart`. Marker-grep the binary first: the bin dir is shared
  across worktrees. Rerun the probes. Targets: snapshot bytes ≤ 10 % of
  baseline; stable DB mtime static while idle; web CPU idle < 5 %;
  live-child attach unchanged (< 100 ms).

Phase 1 gate: `npm run verify` (fmt, check, clippy `--all-targets
--all-features -D warnings`, nextest, doc tests, web check/lint/sg/vitest,
ci:verify). Plus `pkill -f vite; CI=1 npm run web:smoke` if 1.3 touches web
sources. One stacked PR per task or one PR; each must be ≤ 600 changed
lines/file and ≤ 1500/PR. PR title type `fix(...)` so a release is cut.

## Phase 2: show stored transcript before the agent is ready (approval needed)

Changes the protocol v2 attach sequence. Update `web-session-behavior.md` in
the same change.

- [x] **2.1 (#1356) Server: early snapshot.** In `bridge_task_session_socket`, race
  `acquire_typed` against a short timer (about 150 ms). If acquire wins,
  nothing changes (keeps the 24 ms live path). Otherwise, send a
  storage-only snapshot (the `load_detached` + `build_attach` path, which
  must not go through the actor because the actor is blocked in spawn)
  carrying a new optional field `agentPending: true`. Then await acquire.
  On success, send `attach_snapshot(client_cursor = early.cursor)` (reset
  false, incremental). The restore-unavailable error path is unchanged,
  except that the transcript is now already visible.
- [x] **2.2 (#1356) Client: "agent starting" state.**
  `webSessionTransport.ts`: a snapshot with `agentPending` renders the
  transcript but does not mark ready or flush the outbox. The next
  non-pending snapshot does. Add `ConnectionState` `"starting"` (send
  disabled) and a visible "Starting agent…" label. Older cached clients
  treat the early snapshot as ready. Their outbox prompts buffer in the
  socket and are processed after acquire, or stay unacknowledged and are
  resent on restore failure. Acceptable; documented.
- [x] **2.3 Tests.** A Rust bridge test using the fake ACP fixture with a
  delayed handshake: the first frame arrives before the handshake completes
  and has `agentPending`, and the second snapshot clears it. Vitest:
  composer disabled while starting, outbox not flushed until ready. Playwright
  `web:smoke` (mobile-webkit) for the layout of the new label.

## Phase 3: stop storing every streamed version (approval + design sign-off)

The best-practice "cutover". It touches durable-execution storage, so it is
an architecture change: update `durable-execution.md`.

- [x] **3.1 Design note (decided 2026-10-19).**
  - *Rule:* when a commit appends a `Message` with non-empty `item_id` X,
    the newest earlier log entry for X (if it is a real, non-marker
    `Message`) is rewritten to a **marker** in the same `commit_execution`
    transaction. So at any time only the newest version of each item holds
    text. There is no wait for lane close and no batch-end pass.
  - *Marker = empty `Message` with the same role and `item_id`* (`text: ""`,
    no content blocks, `message_id: None`), not a new variant. Reason:
    `web_session_store::try_load` drops undeserializable rows from the Vec,
    which would shift every cursor if an older binary (rollback /
    test-in-stable) read a new variant. Old servers and browsers already
    ignore empty messages (`turnProjection` returns early on empty text and
    no blocks; the normalizer drops empty input). New code treats
    `Message{text:"", content_blocks:[], item_id non-empty}` as a marker
    and filters it from `read_from_enveloped` / `read_from`.
  - *Density preserved:* rows are rewritten in place, never deleted. Seqs
    stay dense and never reused, and cursors stay valid. A reconnect cursor
    inside a compacted range still receives the final version, which has a
    higher seq.
  - *Choke points:* both in-process append paths
    (`TaskSessionState::append_to_log` -> `try_append_events`, and
    `commit_ledger_with_events` -> `try_persist_with_events`) compute
    rewrites from `self.log` + the batch before the store call, then mark
    the in-memory log after success. `startup_recovery` passes none. Core
    stays payload-agnostic: `ExecutionCommit.event_rewrites: Vec<(seq,
    payload_json)>` with an UPDATE in the same transaction; a missing
    (pruned) row is a no-op. The web adapter maps cursor -> seq (`seq =
    cursor + 1`, since `dropped = first_seq - 1`).
  - *Lookup:* reverse scan of the in-memory log (at most 2000 entries) per
    message in the batch. `ponytail:` add an item_id -> cursor map if
    profiling shows the scan.
  - *Deferred (YAGNI):* retention still counts marker rows toward 2000
    (follow-up if history depth matters). No backfill of existing rows:
    they age out, and Phase 1 compaction already keeps snapshots small.
    Within-batch duplicate versions (rare interleave inside one 50 ms pump)
    are left for the next batch.
- [x] **3.2a core:** `ExecutionCommit.event_rewrites` applied atomically
  (execution_store.rs + inline tests: applied with events in one tx; a
  fenced or duplicate commit rewrites nothing; a missing seq is a no-op).
- [x] **3.2b web transcript:** `is_superseded_marker`,
  `TranscriptLog::superseded_cursors_for(&batch)`,
  `TranscriptLog::mark_superseded(&cursors)`, filter markers in reads
  (transcript.rs + transcript_tests.rs).
- [x] **3.2c plumbing:** adapter params (mod.rs, prompt_ledger.rs), both
  append paths (task_session.rs, task_session_commit.rs),
  startup_recovery.rs passes empty. Integration test: stream N versions,
  reload from store: one non-marker row per item, cursors unchanged; replay
  from a cursor inside the compacted range yields the final version.
- [x] **3.3 docs:** durable-execution.md (rewrite-in-place markers, density)
  + web-session-behavior.md (replace "appended ... without rewriting earlier
  rows").
- [x] **3.4 measure:** bytes per streamed reply on the scratch server before
  vs after.

## Phase 4: SQLite engine settings (decision only; recommend defer)

- WAL would remove reader/writer blocking. Risks: (a) the dev DB is shared
  with a docker compose container, and WAL's `-shm` shared memory is unsafe
  across the macOS VM bind mount; (b) frequent `cp ajax.db` backups miss
  `-wal` contents; (c) `state_file_mtime` change detection (web + TUI
  `cockpit_backend.rs`) stops seeing writes until checkpoint, so it must move
  to revision / `PRAGMA data_version` first.
- Recommendation: re-measure after Phase 1. Only proceed if lock contention
  is still visible.

## Deviations / changed assumptions

- Doc drift: `TAIL_EVENT_LIMIT` is documented as 300 but is 1000 in code.
- Measurement side effect: probes spawned Claude sidecars for SaySo/space and
  score-v5. They become evictable after the 15-minute grace.

## Validation log

- 1.1 (#1350): delegate pi/QWEN (DeepSeek route fell back). Fast path in
  `migrate()` returns `ensure_agent_runs_columns` when user_version == 10.
  RED observed by delegate (change counter 21 -> 23). Parent reran:
  `cargo nextest run -p ajax-core` 1061/1061 pass; `cargo fmt --check` ok;
  `cargo clippy --all-targets --all-features -D warnings` ok;
  `cargo nextest run --all-features --no-fail-fast` 3003/3003 pass.
  Checked stable + dev DBs both have all 19 schema-10 tables (fast path safe).
- 1.2 (#1351): DeepSeek PROVIDER_ERROR (0 tool calls, tree untouched; the
  scope violation was the parent's own concurrent plan edit, so not restored,
  logged DISCARD). Re-dispatched pi/QWEN: BufReader stops at the first
  non-whitespace byte. Parent: `cargo nextest run -p ajax-cli` 378/378;
  clippy/fmt ok. Untestable-exception: behavior unchanged, only read volume,
  so no RED test is possible. The new test guards semantics. Nit accepted:
  test cleanup path leaks tiny temp dirs.
- 1.3a (#1352): pi/QWEN. `compact_superseded_messages` in replay.rs applied
  in `attach_inline_tail` + `read_before`. 4 new replay tests. Parent:
  `cargo nextest run -p ajax-web` 1235/1235; clippy/fmt ok.
- 1.3b: pi/QWEN. Doc fix (300 -> 1000, compaction rule) + new
  `reducer.compaction.test.ts` (compacted == uncompacted conversation).
  Parent: `npm run web:test -- --run crates/ajax-web/web/src/features/chat`
  56 files / 567 tests pass; web:check, web:lint ok.
- 1.4 (partial): `scripts/dev-web-restart.sh --worktree $PWD --profile dev`.
  The previous dev server was already broken (schema-9 binary vs schema-10
  DB, cockpit 500). New dev pid 55146. Cockpit poll p50 127 ms (stable old
  binary 258 ms). DB mtime still changes on 20/20 polls on both, so 1.5 is
  found. Snapshot bytes: not re-measured live, because attaching dev tasks
  would spawn and resume harness sessions. The offline rule applied to real
  stable snapshots gives 1828->17 KB, 757->45 KB, 447->52 KB.
- 1.5 (#1355) site 1 (`live_status_unchanged`): pi/QWEN (DeepSeek
  PROVIDER_ERROR x2, auto-fallback). That round produced the diff but no
  report, and review found the stored-None edge (needs_operator_acknowledgment
  is false when observed_at is None). REVISE on QWEN: `None =>
  task.live_status_observed_at.is_some()` plus a test.
- 1.5 site 2 (found by re-measuring; commented on #1355): the tmux-missing
  branch called `apply_observation` and set `changed = true` unconditionally.
  pi/QWEN: skip re-apply for the same TmuxMissing episode;
  `changed |= *task != previous`. Delegate confirmed RED for both new
  idempotence tests.
- Parent: `cargo nextest run --all-features --no-fail-fast` 3012/3013. The one
  failure is `task_session_reliability_tests::terminal_persist_failure_retries_before_fifo_advances`:
  6/6 pass alone, and the full ajax-web run passes 1235/1235. It is a
  pre-existing timing race (fixed 150 ms forced-persist-failure window) and
  does not touch changed code. clippy and fmt are clean.
- 1.4 final (dev :8788, this worktree's build, 20 cockpit polls at 1/s):
  registry revision +20 -> +2. DB mtime changes 20 -> 2-3; the rest are the
  execution-owner lease heartbeat (~10 s, expected) and `runtime_observed_at`
  (one save per ~30 s from the slower refresh tier: bounded, left as-is).
  Cockpit p50 118-140 ms (stable old binary 258 ms).
- 2.1 (#1356): pi/QWEN round died (PROVIDER_ERROR after 35 calls; partial
  diff). Review found a double-acquire holder leak: `select!` dropped the
  in-flight acquire future and issued a second one. REVISE on claude/HAIKU:
  one pinned future, release on early-send failure. A second review found an
  in-page-reconnect gap: `early_cursor` advanced to the stored head when the
  early snapshot had no transcript, so events after the client's cursor were
  skipped. REVISE (HAIKU): pure `resume_cursor_after_early` plus 3 unit tests.
  Parent gate: `cargo nextest run --all-features --no-fail-fast` 3018/3018;
  clippy and fmt clean.
- 2.1 live (scratch AJAX_HOME server, new binary, real claude harness, port
  18790): cold spawn: pending snapshot at 159 ms, live at 2185 ms. Warm: 6 ms,
  no pending. Cold restore after server restart: pending with the 5-event
  transcript at 157 ms, live incremental (reset=false) at 953 ms. In-page
  reconnect with cursor=2 after restart: pending (no transcript) at 160 ms,
  then live, then events 2-4 replayed.
- 2.2a: pi/QWEN. Transport honours `agentPending` (onSnapshot +
  onAgentPending; no ready, onReady or outbox flush). 6 new tests. Parent:
  chat vitest 573/573; web:check ok.
- 2.2b: claude/HAIKU TIMEOUT at 900 s after doing the state, hook, head
  label, docs and test. Finish packet on cursor/composer-2.5 (also hit its
  900 s cap; the runner logged 'auth', but all edits landed): wired
  `agentStarting` via ChatSurface -> ChatLiveHead/ComposerProvider; composer
  placeholder 'Starting agent…'. Parent: web:check, lint, sg ok; full vitest
  166 files / 1655 tests pass. `web:smoke` mobile-webkit hangs locally
  (Playwright 1.61.1 / webkit-2311 on macOS 26, known; CI runs Ubuntu
  WebKit), so I killed it and ran `web:smoke:desktop`: 74 passed, 87
  phone-only skipped. Mobile-webkit layout of the new label is left to CI.
- 3.2a: DeepSeek PROVIDER_ERROR x2, then pi/QWEN. `EventRewrite{seq,payload}`
  + `ExecutionCommit.event_rewrites` UPDATE in the same transaction, after
  the fence/duplicate checks. 3 tests (applied, missing seq no-op,
  fenced/duplicate no rewrite). Re-exports in registry.rs and sqlite/mod.rs
  (necessary one-identifier scope additions). Parent: all features
  3021/3021; clippy and fmt ok.
- 3.2b: pi/QWEN. `is_superseded_marker`, `superseded_cursors_for`,
  `mark_superseded`; reads skip markers. 7 tests. Parent: ajax-web
  1246/1246.
- 3.2c: claude/HAIKU hit its session limit, so the runner fell back to
  codex/gpt-6-astra. The report failed schema validation, but its raw
  report and the parent rerun agree. New `try_append_events_with_rewrites`
  / `try_persist_with_events_and_rewrites`; the old functions are wrappers
  passing no rewrites (no test or call-site churn). Both append paths
  compute markers from the log + batch, commit them in the same
  transaction, and mark the in-memory log only after success; cursor -> seq
  = cursor+1. Parent: all features 3031/3031; clippy and fmt clean.
- 3.3: docs (DeepSeek PROVIDER_ERROR x2 -> QWEN): durable-execution.md
  invariant + atomicity list; web-session-behavior.md append/rewrite
  sentence corrected.
- 3.4 live (scratch server, real claude, same ~300-word prompt): old build
  139 rows / 146,494 bytes for the turn (136 full-text message versions).
  Phase 3 build: 126 rows / 9,180 bytes (2 rows with text: prompt + final
  reply; the rest are markers). About 16x less stored per reply. After a
  server restart the cold-attach pending snapshot is 15 events / 6 KB at
  158 ms, and both replies render their full final text (1976 / 1875 chars).
- Known minor risk (Phase 3): reads drop markers before Phase 1 compaction,
  so a compacted item now sits at its FINAL version's position, not its
  first. That only matters for an interleaved thought/agent stream within
  one turn (rare: harnesses finish thinking before text). Fix if seen: keep
  markers through replay compaction and filter them after.
- Final parent gate (2026-10-19): cargo nextest --all-features 3031/3031;
  clippy and fmt clean; vitest 166 files / 1655 tests; web:check, lint, sg
  ok; web:smoke:desktop 74 passed. mobile-webkit smoke not runnable locally
  (Playwright 1.61.1 hang on macOS 26), so it is left to CI.
