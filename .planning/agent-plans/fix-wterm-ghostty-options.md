# Fix GhosttyCore options undefined on construct

## Scope

Pass `{}` as GhosttyCore constructor options so `init()` can read
`scrollbackLimit`.

## Cause

`new Core(wasm)` left `_options` undefined → Safari
`undefined is not an object (evaluating 'this._options.scrollbackLimit')`.

## Delegation decision

`Delegation decision: not delegated because one-line constructor fix.`

## Checklist

- [x] Pass `{}` options
- [x] Assert in test
- [ ] Rebuild dist, PR from main
