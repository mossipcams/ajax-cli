# ACP image prompt repair

## Scope

- Make image/content-only prompts valid through the Web Cockpit composer,
  browser WebSocket transport, and ACP prompt payload builder.
- Add focused regression coverage in existing source test files.
- Preserve capability validation, outbox behavior, and existing image
  compression/frame-size handling.

## Non-goals

- No new attachment types, ACP protocol changes, or server-side upload store.
- No changes to unrelated session lifecycle or transcript behavior.
- Do not modify files under `tests/`.

## Approval

- Status: complete.

## Tasks

1. Frontend content-only eligibility and transport — **done**
2. ACP prompt payload acceptance — **done**
3. Attachment-only queued follow-up persistence — **done**
4. Bundled web artifact and broader verification — **done**

## Validation

- `npm run web:test -- --run`
- `cargo test -p ajax-web prompt_content`
- `npm run web:check`
- `npm run web:lint`
- `npm run web:build:check`
- `npm run web:test -- --run crates/ajax-web/web/src/features/chat/composer/draftStorage.test.ts`
- `cargo fmt --check`

## Deviations

- Full Vitest emits existing jsdom/xterm canvas `getContext` warnings but exits
  successfully; they are unrelated to this change.
