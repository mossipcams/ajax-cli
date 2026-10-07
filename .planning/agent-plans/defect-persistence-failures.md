# Swallowed persistence failures (defects #1219–#1228, #1240, #1241)

## Approval

User directed immediate implementation on 2026-10-07 ("review the open
defects, let's start tackling them" → "Do it"). No lifecycle, registry-truth,
or security-assumption change: every step only stops reporting success for a
write or read that failed.

## Root cause

`crates/ajax-web/src/adapters/web_session_store/mod.rs` returns `()` /
defaults from every operation:

- `load` returns `StoredSession::default()` for a missing file **and** for an
  unreadable one, and skips any unparsable line wherever it sits.
- `save_meta` / `clear_acp_session_id` do load → mutate → `persist`. When the
  load failed, they rewrite the file from an empty default, erasing the
  transcript.
- `append_events` and `persist` log a warning and return.
- `SessionEvidence::transcript_durability_fault` is read by the outbound and
  exit paths but nothing ever sets it.

The registry-snapshot, session-activity, and push-subscription defects are the
same pattern at different stores.

## Scope

- `crates/ajax-web/src/adapters/web_session_store/`
- `crates/ajax-web/src/slices/web_session/` (callers)
- Later steps: the registry-snapshot persistence callers, the session
  activity reporter, the push subscription store (located by the delegate).

## Non-goals

- No change to the transcript format, the 2000-event cap, or the 64 KB
  compaction trigger (#1234 is tracked separately).
- No retry/queueing layer. A failed write is reported, not retried.
- No HTTP idempotency store (#1233).

## Steps

- [x] **1. Store returns errors** (#1228, #1240; foundation for 2).
  `load`, `save_meta`, `clear_acp_session_id`, `append_events` return
  `io::Result`. `load`: missing file → `Ok(default)`; open/read failure →
  `Err`; unparsable final line tolerated as a truncated tail; unparsable
  interior lines counted on `StoredSession` (`corrupt_lines`). `save_meta` and
  `clear_acp_session_id` never rewrite when the load errored. Callers adapted
  to compile with their current behavior (warn and continue).
- [x] **2. Session callers act on the errors** (#1219, #1220, #1221, #1228,
  #1240). Append failure sets `transcript_durability_fault` and
  `append_to_log` returns `Err`. A failed resume-id save is surfaced and the
  session is not treated as resumable. Clear/switch reports failure instead of
  success. Unreadable metadata at spawn surfaces an error instead of starting
  a fresh ACP context. Interior corruption reaches the snapshot
  `transcriptError`.
- [ ] **3. Registry snapshot persistence** (#1224, #1225, #1226). Model
  change, ACP task-mode promotion, and harness switch return the
  `persist_registry_snapshot` failure instead of reporting success.
- [ ] **4. Session activity reporter** (#1227). SQLite failure is returned;
  in-memory status and revision do not advance past it.
- [ ] **5. Push subscription store** (#1241). Atomic write before
  acknowledging; a corrupt file is an error, not an empty store.

Each step: one delegate dispatch, one regression test per issue named with
its number, one local commit.

## Validation

Per step: `cargo nextest run -p ajax-web` (plus `-p ajax-cli` when touched),
`cargo clippy --all-targets --all-features -- -D warnings`, `cargo fmt
--check`, `node scripts/check-file-loc.mjs`. Each regression test confirmed
failing on the pre-fix source.

## Results

- 2026-10-07, steps 1+2 (one change, implemented in-process: every delegate
  route was out of quota or failing, and the user approved the bypass).
  Deviations from the step text: the store keeps `load` / `save_meta` /
  `append_events` as `#[cfg(test)]` fixtures and adds `try_load` /
  `try_save_meta` / `try_append_events` for production, so no existing test
  file changed; a failed compaction after a durable append is logged, not
  returned; the corrupt-row count is stored in the meta row because the next
  rewrite drops the bad row; corruption warns through `transcriptError` but
  does not block prompts; an operator clear/harness switch clears the
  durability fault so writes are retried.
  Validation: 8 new `issue_12xx` tests pass (the 5 session-level ones fail on
  the pre-fix source); `cargo nextest run --workspace` 2409/2409; clippy
  `--all-targets --all-features -D warnings` clean; `cargo fmt --check` clean.
- Remaining: steps 3–5. "Identifies the affected range" from #1240 is not
  done: the warning carries a row count only.
