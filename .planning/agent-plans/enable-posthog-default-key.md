# Enable PostHog project write key by default

**Mode:** Small Fix  
**Delegation decision:** not delegated because smaller than a work order (default key + test/docs sync)

## Scope

- Default to the Ajax PostHog Cloud project write key when `VITE_POSTHOG_KEY` is unset
- Allow env override; allow explicit disable via `off` / `0` / `disabled`
- Sync tests + architecture wording

## Checklist

- [x] Default key in `telemetry.ts`
- [x] Tests + docs
- [x] `npm run web:test -- --run .../telemetry` (44 passed) + `web:check`
