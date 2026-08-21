---
context: default
slug: architecture-granular-rules
status: complete
approval: user-directed 2026-08-21 — granular architecture rules
last_updated: 2026-08-21
---

# Granular web-session architecture rules

## Goal

Make Ajax chat ownership enforceable at module granularity, not only at
slice/adapter crate boundaries. The CI Architecture job already runs
`npm run verify:arch`; these rules give it intra-session teeth.

Follows `.planning/agent-plans/ajax-chat-architecture.md` target ownership and
the implemented layout under `crates/ajax-web/src/slices/web_session/`.

## Evidence

`crates/ajax-web/src/architecture.rs` already forbids:

- session adapters importing the `web_session` slice or each other
- adapters importing slices/`runtime`
- slices importing sibling slices/`runtime`

It does **not** distinguish protocol vs command loop vs ACP drain vs store vs
thin WebSocket. A mapping module could import `AcpStdioClient` or the runtime
route could load JSONL and still pass.

Implemented modules (do not rename to match the original plan names):

| Module | Owns |
| --- | --- |
| `protocol` | v2 snapshot/event envelopes |
| `acp_map` | ACP update → `SessionServerEvent` |
| `normalize` | host stream normalization / item ids |
| `acp_usage` | usage dedupe |
| `replay` | cursor replay planning |
| `transcript` | in-memory cursor and permission filter |
| `ws_bridge` | socket forward to directory |
| `session_cleanup` | registry-owned JSONL retention |
| `model_change` | harness-switch reset via directory |
| `task_session*` / `acp_drain` | command loop, spawn, ACP poll (may call adapters) |
| `adapters::web_session_acp` | ACP stdio only |
| `adapters::web_session_store` | JSONL only |
| `runtime` production | cookie/origin, attach plan, delegate to `ws_bridge` |

## Scope

- Add architecture tests for the forbidden-import table below (existing source
  must already pass; do not refactor production to satisfy a new rule).
- Document the module table in `docs/architecture/web-cockpit.md` and point at
  it from root `architecture.md`.
- Keep helpers in `architecture.rs`; if that file would exceed ~600 lines,
  peel session rules into `crates/ajax-web/src/architecture_web_session.rs`
  (`#[cfg(test)]` sibling in `lib.rs`) and reuse the same scan helpers.
- Update this plan checklist after verification.

## Non-goals

- No new crate, linter, or generic architecture framework.
- No production behavior change.
- No browser ESLint rewrite (feature `public.ts` rules already exist).
- Do not filter architecture tests out of rust-test.
- Do not always-run the Architecture CI job on docs-only PRs.

## Forbidden-import table (production `.rs` only)

Substring / path checks, same matcher as existing architecture tests
(`source_mentions_path`). Skip `*_tests.rs`, `test_support.rs`, and
`src/runtime/tests/**`.

| Target | Must not mention |
| --- | --- |
| `protocol`, `acp_map`, `normalize`, `acp_usage` | `web_session_store`, `AcpStdioClient`, `task_session_directory`, `task_session_spawn`, `acp_drain`, `crate::runtime`, `ajax-web::runtime` |
| `replay` | `web_session_store`, `AcpStdioClient`, `task_session_spawn`, `acp_drain`, `ws_bridge`, `crate::runtime`, `ajax-web::runtime` |
| `transcript`, `model_change` | `AcpStdioClient`, `task_session_spawn`, `acp_drain`, `ws_bridge`, `crate::runtime`, `ajax-web::runtime` |
| `ws_bridge` | `web_session_store`, `AcpStdioClient`, `acp_drain`, `acp_map`, `StreamNormalizer`, `crate::runtime`, `ajax-web::runtime` |
| `session_cleanup` | `AcpStdioClient`, `task_session_spawn`, `ws_bridge`, `crate::runtime`, `ajax-web::runtime` |
| `adapters::web_session_acp` | `SessionClientMessage`, `SessionServerEvent`, `TaskSessionDirectory`, `bridge_task_session` (plus existing slice import ban) |
| `adapters::web_session_store` | `AcpStdioClient`, `AcpClientEvent`, `SessionClientMessage`, `SessionServerEvent`, `TaskSessionDirectory` (plus existing slice import ban) |
| `runtime` production | `web_session_store`, `AcpStdioClient`, `web_session::task_session::`, `web_session::acp_drain`, `web_session::acp_map` |

Allowed (do not forbid):

- `protocol` / `replay` importing `ConfigOptionDescriptor`
- `transcript` / `session_cleanup` / command-loop modules importing the store
- `acp_drain` / `task_session*` importing `AcpStdioClient`
- runtime calling `prepare_task_session`, `bridge_task_session_socket`,
  `TaskSessionDirectory`, `parse_client_cursor`
- test modules exercising adapters

Add one matcher self-check (same style as
`architecture_rule_rejects_adapter_importing_specific_slice`) proving a fake
`AcpStdioClient` import in `protocol` would fail.

## Implementation checklist

- [x] Granular session/runtime/adapter architecture tests
- [x] Matcher self-check
- [x] `docs/architecture/web-cockpit.md` module table
- [x] Root `architecture.md` pointer
- [x] `cargo test -p ajax-web architecture` and record results here

## Stop conditions

Stop and revise before changing production imports to paper over a rule, adding
a second policy engine, or weakening existing slice/adapter guards.

## Validation

```bash
cargo test -p ajax-web architecture -- --nocapture
```

Result (2026-08-21): **pass** — 22 architecture tests (including 9 granular
web-session import guards and matcher self-check).

## Approval and status

- Approved by user request 2026-08-21 for granular architecture rules.
- Implementation: complete (2026-08-21).
