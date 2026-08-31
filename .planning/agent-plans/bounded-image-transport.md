# Bounded image transport redesign

## Scope

Replace the current 256 KiB image workaround with a bounded, inline image
transport over the existing authenticated WebSocket and ACP `ContentBlock`
path. Large normal photos should be resized/re-encoded and sent; impossible
inputs must be rejected before ACP dispatch. Queued attachment bytes remain
in memory only so browser storage is never used as a binary blob store.

The proposed wire envelope is 8 MiB, with a smaller per-image budget derived
from the prompt envelope and a maximum of eight image blocks. The exact
constants remain centralized in the shared transport contract and mirrored by
the Rust bridge.

## Non-goals

- No upload endpoint, multipart protocol, blob service, or ACP extension.
- No server-side attachment replay store or cross-machine attachment sync.
- No changes to task lifecycle, registry truth, session ownership, or terminal
  behavior.
- No new image-processing dependency unless the existing toolchain cannot
  perform the required validation.
- Do not modify files under `tests/`.

## Approval

- Status: approved — implementation complete; independently reviewed and verified.

## Tasks

1. Define the bounded transport contract — **complete**
   - Test: add failing Rust and TypeScript assertions for the shared 8 MiB
     frame limit, per-image budget, block-count cap, and backward-compatible
     text-only frames.
   - Code: replace the duplicated 256 KiB constants with one documented
     bounded contract mirrored in `ws_bridge.rs` and `transport/contracts.ts`;
     keep the WebSocket message shape unchanged.
   - Verify: focused bridge/transport tests and `npm run web:check`.

2. Normalize large browser images — **complete**
   - Test: add failing browser/unit coverage for a photo larger than 256 KiB
     that becomes a valid JPEG within the new budget, plus an uncompressible
     image that produces the existing actionable error without dispatch.
   - Code: reuse the existing canvas pipeline to enforce a maximum dimension,
     reduce JPEG quality, and reduce dimensions until the per-image budget is
     met; keep capability gating and attachment-only prompts intact.
   - Verify: focused composer, prompt-content, and transport Vitest suites.

3. Add authoritative host-side validation — **complete**
   - Test: add failing Rust coverage for oversized base64, invalid base64,
     unsupported image MIME, too many blocks, and capability rejection;
     retain image-only ACP payload and transcript-summary assertions.
   - Code: validate decoded image payload size/format and prompt limits in the
     existing `prompt_content` boundary before mapping to ACP blocks. Preserve
     ACP `ContentBlock::Image` and the existing JSONL summary behavior.
   - Verify: `cargo test -p ajax-web prompt_content` and relevant WebSocket
     bridge tests.

4. Make queued binary state memory-only — **complete**
   - Test: add failing storage coverage proving text-only queues persist while
     attachment-bearing queued rows are not written to localStorage and are
     not degraded into text-only sends after reload.
   - Code: persist only queued text; retain attachment-bearing rows in the
     in-memory composer state until drain, removal, or session teardown. Keep
     queued replacement, stop-and-send, and reconnect behavior unchanged.
   - Verify: composer queue/storage Vitest suites and the full web test suite.

5. Verify the real browser path and update the contract — **complete**
   - Test: run mobile WebKit against the served bundle with a real file picker
     flow; assert a >256 KiB photo produces an ACP image block whose outbound
     frame is within the new cap, and assert the impossible-fit error is shown.
   - Code: update `docs/architecture/web-session-behavior.md` and the owning
     web-cockpit documentation to describe the larger bounded inline path and
     memory-only attachment queue semantics; regenerate `dist/app.js`.
   - Verify: `npm run web:smoke`, `npm run web:check`, `npm run web:lint`,
     `npm run web:build:check`, `cargo fmt --check`, and `git diff --check`.

## Validation commands

- `npm run web:test -- --run`
- `cargo test -p ajax-web prompt_content`
- `cargo test -p ajax-web ws_bridge`
- `npm run web:check`
- `npm run web:lint`
- `npm run web:smoke -- <focused attachment simulator>`
- `npm run web:build:check`
- `cargo fmt --check`
- `git diff --check`

## Deviations

- A pre-existing Rust prompt-capabilities test fixture declared non-PNG bytes as
  `image/png`; it was corrected to a valid tiny PNG signature without changing
  the assertion after host format validation was added.
- A full parallel `cargo test -p ajax-web` run exposed one unrelated,
  environment-sensitive `session_models::a_failed_catalog_read_is_not_cached`
  failure; its exact isolated test passes, as do the attachment-focused suites.
