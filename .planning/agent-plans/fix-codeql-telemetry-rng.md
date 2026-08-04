# Fix CodeQL insecure randomness in telemetry IDs

**Mode:** Small Fix  
**Delegation decision:** not delegated because one-line RNG fallback fix  
**Cause:** CodeQL `js/insecure-randomness` on `Math.random()` fallback in `generateId()` (PR #759)

## Fix

Replace `Math.random()` fallback with `crypto.getRandomValues` so event/install/session IDs never use insecure RNG.

## Checklist

- [ ] Fix `generateId` in `telemetryContext.ts`
- [ ] Focused test still passes
- [ ] Push to PR branch
