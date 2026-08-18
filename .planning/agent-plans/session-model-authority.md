# Plan: Session model authority

## Scope

Ajax chat must run the model the operator picks. Task `session_model` is
desired state. ACP handshake evidence is applied state. Snapshot `model` must
not echo the desired pin.

GitHub: [#952](https://github.com/mossipcams/ajax-cli/issues/952).

Non-goals: catalog-only picker patches, native `<select>` (#936), browser as a
second registry, task lifecycle / ACP permission mode changes, vendoring model
lists.

## Approval

Approved in chat 2026-08-18 (“Delegate until finished”).

## Task checklist

- [x] Open/link GitHub defect ([#952](https://github.com/mossipcams/ajax-cli/issues/952))
- [x] Host tests: snapshot.model is ACP-reported applied id; Cursor applies in-band when advertised
- [x] Resolve `auto` at persist and attach (start_task still stores "auto"; prepare_task_session still uses it as a pin)
- [x] Apply after handshake and read back current model into SessionSnapshot
- [x] Bind Ajax chat picker to snapshot applied model (#942 revert-on-model-failure only)
- [x] Update `docs/architecture/web-session-behavior.md` and `web-cockpit.md`

## Validation

- `cargo nextest run -p ajax-web` (session spawn / ws_bridge / catalog / operate)
- `npm run web:test -- --run ModelPicker sessionModel SessionChat useTaskSession`

## Results

Implemented in bounded delegate: `apply_model.rs`, snapshot applied-model authority,
Cursor in-band apply when advertised, auto→None persistence, host/web #952 regressions.

## Deviations

None.
