# Optimization top 4 (A/B/C + mic mute)

## Scope

Sequential PRs from the approved plan:

1. Presence-only `AGENT_PROCESS_ALIVE_KEY` (stop per-second SQLite thrash)
2. Cache agent-events JSONL observations by mtime/len
3. Skip GitHub CI probes on `RefreshTier::Live`
4. Mute ScriptProcessor mic via GainNode(0)

## Non-goals

- Seed capture sizing, DiffReview/boot lazy-load, write-batching fold-in

## Delegation decision

`Delegation decision: delegated via model-router` — one PR per dispatch.

## Checklist

### PR1 alive stamp
- [x] Implement + tests
- [x] architecture.md one-line clarify
- [x] Parent validation + accept — **Accepted** (cursor escalate; GLM 429)

### PR2 JSONL cache
- [x] Implement + tests
- [x] Parent validation + accept — **Accepted**

### PR3 Live skip gh
- [x] Implement + tests
- [x] architecture.md Live vs Full
- [x] Parent validation + accept — **Accepted**

### PR3b notify tick uses Full tier
- [x] `handle_refreshed_cockpit_request`: Full when `deliver_notifications`, else Live
- [x] Parent validation + accept — **Accepted** (notify Full / browser Live)

### PR4 mute mic
- [x] Implement + tests
- [x] Parent validation + accept — **Accepted**

## Deviations

- PR3b: notify tick now uses Full so CI probes still run when browser disconnected; browser polls stay Live.
- GLM 5.2 hit monthly usage limit; Cursor Composer used for implementation.

## Validation

### PR1
```bash
cargo nextest run -p ajax-core process_alive_stamp  # EXIT 0 — 3 passed
```

### PR2
```bash
cargo nextest run -p ajax-cli agent_status  # EXIT 0 — 10 passed
```

### PR3 (+ PR3b)
```bash
cargo nextest run -p ajax-core live_refresh_skips_github  # EXIT 0
cargo nextest run -p ajax-web suite_3  # EXIT 0 — 17 passed
```

### PR4
```bash
npm run web:test -- crates/ajax-web/web/src/shared/lib/speechTransport.test.ts --run  # EXIT 0 — 19 passed
```

