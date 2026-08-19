# Plan: Slim Cursor catalog; persist split session_model

## Scope

`GET /api/session/models?agent=cursor` serves **unique model bases**, not
Fast/effort exploded ids. New Task and Switch persist Cursor selections as the
existing pipe form (`grok-4.6|effort=high|fast=false`), the same shape bridges
already use. Apply/spawn keep accepting legacy exploded catalog ids.

GitHub: [#979](https://github.com/mossipcams/ajax-cli/issues/979).

## Approval

Requested in chat 2026-08-19 (“i want that split”) after the UI picker already
collapsed Fast/effort twins but still required exploded catalog ids to persist.

## Non-goals

- New task-metadata columns or a second catalog source in the browser
- Changing ACP handshake (`parameterizedModelPicker`) or Fast-off-by-default apply
- Native `<select>` (#936)
- Slimming non-Cursor harness catalogs
- Rewriting stored `session_model` on existing tasks (read both forms)

## Contract

- Cursor `agent models` remains the source of truth. Ajax collapses it; the
  browser does not invent models.
- Catalog `models[]` ids are bases (`composer-2.5`, `grok-4.6`, `auto`). Each
  non-Auto row carries the axes derived from exploded siblings:
  `efforts: string[]` (may be empty) and `hasFast: boolean`.
- `catalog.default` stays [`CURSOR_DEFAULT_MODEL`] (`cursor-grok-4.6-high`) so
  attach-plan and spawn mapping do not move. The picker parses that id.
- Picker `onChange` emits `encodeModelSelection(base, { effort?, fast })`.
  Omit `fast=false` only if that hides Off; persist `fast=false` when the row
  has Fast so Auto vs explicit Off stay distinct from a missing key.
  **Do persist** `fast=false` for non-Fast Cursor picks (except Auto).
- Auto remains `auto` with no Fast/effort extras.
- Apply, spawn argv, and pin matching parse **both** pipe form and legacy
  exploded ids (`cursor-grok-4.6-high`, `composer-2.5-fast`) via
  `parse_cursor_model_intent` / `parse_model_selection`.
- Grok spawn still needs a `cursor-grok-*` argv token. Pipe form
  `grok-4.6|effort=high|fast=false` must reconstruct `cursor-grok-4.6-high`,
  not only a bracket id (live Cursor ignores brackets on `--model`).
- `apply_model.rs` must not grow; intent/token mapping lives in `ajax-core`
  `agent.rs` (split a sibling module if that file would exceed ~600 lines).

## Task checklist

- [x] Collapse Cursor `agent models` in `session_models` (parse exploded, emit
      bases + `efforts` / `hasFast`). Tests on sample `agent models` text.
- [x] `parse_cursor_model_intent` (and spawn / in-band token helpers) accept
      pipe form; Grok spawn reconstructs `cursor-grok-*`. Tests in ajax-core.
- [x] ModelPicker persists pipe form; reads legacy exploded ids. Catalog rows
      drive Effort/Fast from API fields, not sibling ids.
- [x] Shortlist ranks unique bases; no Fast twins possible from a slim catalog.
- [x] Update `docs/architecture/web-session-behavior.md` and
      `docs/architecture/web-cockpit.md`.
- [x] Focused tests: session_models collapse, agent intent/spawn tokens,
      sessionModel compose/parse, ModelPicker persist, existing #979 apply tests
      still pass with both id forms.

## Validation

```bash
cargo test -p ajax-core parse_cursor_model_intent cursor_catalog
cargo test -p ajax-web session_models
cargo clippy -p ajax-core -p ajax-web --all-targets -- -D warnings
npm run web:test -- --run ModelPicker sessionModel modelShortlist NewTaskSheet HarnessSwap
npx tsc -p crates/ajax-web/web/tsconfig.check.json --noEmit
```

## Deviations

Reverses the UI-picker plan rule “must not send `grok-4.6|fast=false` as
`session_model`”. That rule existed only because persist required a catalog id.
