# Cursor Notification wait hooks (AoE non-ACP)

## Scope

Copy AoE’s **non-ACP** Cursor wait hooks into Ajax’s **Cursor-specific**
native-event path (not Claude’s Notification installer, not ACP):

- Install `Notification` matchers `permission_prompt` + `elicitation_dialog`
  into `~/.cursor/hooks.json` via `install_cursor_hooks` only
- Translate those events → `AttentionRequested` (Permission / Question)
- Install + translate `ElicitationResult` → resume Working (`turn_started`)
- Mark Cursor `permission_wait` / `question_wait` as `Native` in capability
- Update `architecture.md` sentences that still say Cursor has no native wait/ask

## Non-goals

- AoE ACP / structured-view path
- Changing Claude Notification installers
- Full AoE Cursor pane detector (B3)
- Supervisor stream-json rewrite (already maps `request` / approval text)
- Web UI changes

## Delegation decision

`Delegation decision: delegated via model-router`

## Checklist

- [x] Failing tests: cursor translate Notification:* + ElicitationResult; install
      writes matched Cursor Notification (+ ElicitationResult); capability Native
- [x] `install_cursor_hooks` + cursor merge-with-matcher helper
- [x] `translate_native_event` cursor arms only
- [x] `cursor_profile` + tests
- [x] `architecture.md` Cursor wait wording
- [x] Parent validation + reinstall harness hooks

## Validation

```bash
cargo test -p ajax-cli --lib -- agent_hooks agent_event  # 31 passed (parent)
cargo test -p ajax-core --lib -- agent_capability pane_  # 18 passed (parent)
cargo check -p ajax-cli -p ajax-core                     # ok
./target/release/ajax-cli agent-hooks install            # cursor: installed
cargo install --path crates/ajax-cli --locked --force    # PATH binary has new translate arms
```

## Deviations

- GLM 429 → Cursor Composer 2.5
- Delegate report schema invalid; parent gated on diff + re-validation + hook reinstall
- Scoped to Cursor native-event adapter (`install_cursor_hooks` + cursor translate
  arms), not supervisor stream-json `CursorAdapter` (already maps `request` waits)cargo test -p ajax-core --lib agent_capability           # pass
cargo check -p ajax-cli -p ajax-core                     # pass
./target/release/ajax-cli agent-hooks install   # after green (parent)
```
