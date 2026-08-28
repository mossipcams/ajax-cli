# Cursor ACP HTTP/2 CANCEL

**Date:** 2026-08-28
**Mode:** Behavior change (web session ACP drain + operator CLI transport)
**Related:** [#1066](https://github.com/mossipcams/ajax-cli/issues/1066) (closed), [#1071](https://github.com/mossipcams/ajax-cli/pull/1071), [#1103](https://github.com/mossipcams/ajax-cli/issues/1103)
**Approval:** granted 2026-08-28 (“Delegate until finished”)

## Problem

Ajax Chat Cursor sessions (`agent acp`) die mid-turn with:

`Error: RetriableError: [canceled] http/2 stream closed with error code CANCEL (0x8)`

That string is Cursor’s ConnectRPC/HTTP/2 client inside the ACP child, not Ajax’s WebSocket. #1071 only mapped the dump: every cancel-shaped `session/prompt` failure became `turn_end` / Stopped. The stream still dies. Unsolicited network CANCEL looks like the operator hit Stop. `Error: RetriableError:…` can still leak because host/browser matchers require the message to *start with* `RetriableError:`.

IDE **Network → HTTP/1.1** does not apply. `agent acp` reads `~/.cursor/cli-config.json` (`network.useHttp1ForAgent`). This machine currently has that flag `false`.

## Best approach

Same as other ACP hosts (JetBrains, Zed): **change the Cursor CLI transport**, do **not** retry `session/prompt`, and make Ajax’s cancel vs interrupt classification match whether *this host* sent `session/cancel`.

1. **Prevent the drop (operator, Cursor CLI).** Set `"network": { "useHttp1ForAgent": true }` in `~/.cursor/cli-config.json`, then recycle the ACP child (leave/reopen Ajax Chat or restart ajax-web). SSE over HTTP/1.1 is Cursor’s supported workaround. Cost: slightly heavier connections, coarser abort, off the default HTTP/2 path. Idle proxies can still cut long turns; you just stop getting HTTP/2 `CANCEL (0x8)`.
2. **Do not retry the prompt.** Keep the existing host rule. Tools may already have run. `cursor-agent --resume` is the interactive CLI, not ACP `session/load`.
3. **Classify from host intent (Ajax).** Revise the #1066 contract:
   - Host already sent `session/cancel` → `turn_end` `stopReason: cancelled` (Stopped).
   - Unsolicited cancel-shaped `session/prompt` failure (HTTP/2 `CANCEL (0x8)`, `[canceled]`, `RetriableError` with cancel text, including an `Error:` prefix) → typed `error` with the existing host-owned sentence: `The connection was interrupted. Try sending again.` Ledger: **interrupted**, not completed-cancelled. Composer restore for empty-agent-response already exists.
4. **Never persist or display raw harness dumps.** Widen `map_operator_visible_acp_error` and `explainAcpError` so `Error: RetriableError:…` and substring `RetriableError:` map the same as a leading `RetriableError:`.

Do **not** rewrite `~/.cursor/cli-config.json` from Ajax. Do **not** drop `.cursor/cli.json` into task worktrees. Do **not** copy IDE HTTP Compatibility into ACP spawn.

## Scope

- `crates/ajax-web/src/slices/web_session/acp_drain.rs` (+ tests)
- `crates/ajax-web/src/slices/web_session/task_session.rs` (operator-cancel flag on `ActivePrompt`)
- `crates/ajax-web/web/src/features/chat/session/errors.ts` (+ reducer tests)
- `docs/architecture/web-session-behavior.md` (#1066 bullet rewrite)
- New GitHub defect for unsolicited CANCEL-as-Stopped (do not silently reopen #1066’s “show dump” issue)
- Optional: `ajax doctor` read-only hint if Cursor CLI has `useHttp1ForAgent: false`

## Non-goals

- Auto-retry or replay `session/prompt`
- Mutating the operator’s Cursor CLI/IDE config from Ajax
- Forcing HTTP/1.1 via spawn env unless a documented Cursor env/flag is confirmed in a spike (then a new EXECUTION)
- Unsetting `HTTP_PROXY` / SOCKS5 on spawn (Zed workaround; only if we prove inherited proxy is in play)
- IDE Network settings, `cursor-agent --resume`, non-Cursor harnesses
- Filtering agent *prose* that happens to contain the dump (only RPC / `session_error` paths)

## Architecture impact

Yes. `web-session-behavior.md` currently says every cancel-shaped `session/prompt` abort is `turn_end cancelled`. This plan splits that on **host-sent cancel** vs **unsolicited transport abort**. Ownership stays in `web_session`; ACP stdio adapter still only returns typed events. No retry, no second registry of truth.

## Task checklist

- [x] T0 — Open a GitHub defect ([#1103](https://github.com/mossipcams/ajax-cli/issues/1103)): Ajax Chat treats unsolicited Cursor ACP HTTP/2 CANCEL as Stopped; raw `Error: RetriableError:` can leak. Link #1066/#1071. Operator HTTP/1.1 is notes, not the defect.
- [ ] T1 — Operator: `useHttp1ForAgent: true` is set. Recycle the ACP child (reopen Ajax Chat) and confirm a long Grok turn. Out of repo.
- [x] T2 — `ActivePrompt` records `cancel_requested` when `cancel()` successfully sends ACP `session/cancel`. Replacement/exit cancels that are host-initiated count as requested.
- [x] T3 — `map_request_finished` / `classify_prompt_terminal`: cancel-shaped prompt error + `cancel_requested` → Cancelled / `turn_end cancelled`. Cancel-shaped without that flag → Failed + `CONNECTION_INTERRUPTED_MESSAGE`. Genuine non-cancel `RetriableError` stays interrupted. Non-prompt RPCs unchanged.
- [x] T4 — Dump matching is content-based (`RetriableError:` / cancel family), not `starts_with`. Same in `explainAcpError` for replay.
- [x] T5 — Tests in `acp_drain_tests.rs` and `reducer.test.ts`: prefixed dump, unsolicited CANCEL → interrupted, host-cancel + CANCEL → Stopped, no retry. Existing #1071 cases that assumed all CANCEL → cancelled must be updated to the split.
- [x] T6 — Rewrite the #1066 paragraph in `web-session-behavior.md`. Mention Cursor ACP cloud transport is CLI HTTP/2; HTTP/1.1 is `useHttp1ForAgent` on the CLI config the `agent acp` child reads.
- [ ] T7 (optional) — `ajax doctor` warn-only if `~/.cursor/cli-config.json` has `useHttp1ForAgent` false/absent. No writes.

## Validation

```bash
cargo test -p ajax-web --lib web_session::acp_drain
cargo test -p ajax-web --lib web_session::task_session
npm run web:test -- --run src/features/chat/session/reducer.test.ts
```

After T3/T4, also run the crate’s existing `acp_drain` / session reliability tests that assert cancel vs interrupt ledger phases.

Manual (T1): one long Cursor ACP turn in Ajax Chat after HTTP/1.1; confirm no `CANCEL (0x8)` dump and that Stop still shows Stopped.

## Risks

- Cursor may still abort with cancel-shaped text after a *successful* host cancel; the flag must be set when we *send* cancel, not when ACP acknowledges the prompt result.
- HTTP/1.1 can fail some CLI users with a 50 MB request-size error. Leave it opt-in on the operator’s CLI config, not forced by Ajax.
- Long High/Grok turns behind idle proxies can still drop on HTTP/1.1; classification (T3) is what Ajax can still get right.

## Deviations

- T7 (`ajax doctor`) skipped to keep the change in `web_session`.
- Router `execute` reported `ACP_EVENT_FAILED` wrapping an inner `STATUS: COMPLETE`. Parent accepted after reviewing the delta and re-running tests.
