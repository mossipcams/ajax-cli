# Suppress Cursor stop → "Response ready" notify false positives

Mode: Behavior Change.
Delegation decision: delegated via model-router

## Scope

Cursor (and any client) `stop`/`TurnSettled` projects `done` → Waiting
`"Response ready"`. That explanation is inbox-visible but must **not**
phone-ping — same class as `"Ready for review"`. Cursor has no native wait/ask;
every turn-end `stop` was firing a false-positive webhook.

## Non-goals

- Changing Cursor hook install or translation (`stop` still → `done`)
- Suppressing real Error from `stop` with `status=error` → `failed`
- Notify confirmation dwell / episode-clear timing
- UI status vocabulary changes

## Checklist

- [x] Failing test: Done / "Response ready" → `take_attention_transition` = None
- [x] `is_actionable_attention` excludes `"Response ready"`
- [x] Parent validation: attention nextest + focused filter
- [x] architecture.md one-line note (parent after ACCEPT)

## Validation

```bash
cargo nextest run -p ajax-core attention -- response_ready rate_limited Ready
# 61 passed
cargo fmt --check  # clean
cargo clippy -p ajax-core --all-targets --all-features -- -D warnings  # clean
```

## Deviations

- MiniMax (`opencode-go/minimax-m3`) hit GoUsageLimitError; escalated to
  cursor-delegate `composer-2.5`. Report envelope missing but diff in-scope;
  parent ACCEPT after independent validation.
