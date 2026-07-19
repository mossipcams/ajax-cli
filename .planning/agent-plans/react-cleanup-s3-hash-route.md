# Slice 3 — useHashRoute as an external store

Master plan: `react-migration-cleanup.md`
Depends on: slice 2c (`d0d0d17`)

## Scope

Model the URL hash as an external store via `useSyncExternalStore` instead of
copying it into component state through an effect.

## Delegation decision

`Delegation decision: not delegated because the change is one 30-line file whose
entire risk is a subtle identity contract` — smaller than the work order needed
to describe it, and the failure mode is exactly the kind delegates missed in
slice 2b.

## The trap this slice had to avoid

The reference pattern parses in render:

```ts
const hash = useSyncExternalStore(subscribe, getSnapshot, () => "#/");
return parseRoute(hash);          // new object EVERY render
```

`App.tsx` holds `useEffect(..., [route])` for the document title. A route object
rebuilt each render would fire that effect on **every render** rather than only
on navigation — a real regression the naive conversion introduces silently.

Two identity guards were written **first** and pass against the old
`useState` implementation, so they characterize existing behaviour:

1. one route identity across re-renders while the hash is unchanged
2. a new identity after the hash changes

Implementation keeps `useMemo(() => parseRoute(hash), [hash])`. The program
brief says "use `useMemo` only if measurement shows the calculation is expensive
or **stable identity is required**" — the second clause applies exactly.

## Two details that matter

- **Snapshot is the raw hash string, not a parsed Route.** Snapshots are compared
  by identity; returning a fresh object from `getSnapshot` makes every check
  report a change and loops forever.
- **`subscribe` / `getSnapshot` are module-scope**, so their identities are
  stable and the store never re-subscribes.

## Guard proven, not assumed

Dropping the `useMemo` fails exactly one test — the identity guard — and passes
again on restore. The memo is load-bearing, not decoration.

## Validation

| Command | Result |
| --- | --- |
| `web:test` (hook) | 5 passed |
| `web:test` (full) | 37 files / **332 tests** |
| `web:lint` | 0 |
| `web:check` | 0 |
| `web:smoke` mobile-webkit | 92 passed / 0 failed |
| `verify` | see below |

## Device validation

Routing is user-visible, but the observable contract is unchanged and the
mobile-webkit e2e corpus covers routing, outlets, and the update banner. Folded
into the slice 2c iPhone pass rather than issuing a separate checklist — item:
navigate dashboard → task → back → settings and confirm titles and outlets track
the hash.

## Deviations

- Kept `useMemo` rather than parsing bare in render (justified above).
