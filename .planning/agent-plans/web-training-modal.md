# Web training modal

Approval status: **APPROVED 2026-10-05** ("Delegate until finished"); Decisions defaults apply. Security/architecture change: browser-initiated control of a remote GPU host.
Last updated: 2026-10-05.

## Goal

Rename the bottom-nav `Dashboard` button (default label: `Train`) and make it open a modal with three sections:

1. **Training runs**: live status and progress for Unsloth (SaySo LFM) and LiveKit wake-word, with Start / Stop controls.
2. **Data generation**: live progress of `./sayso generate` (rows written vs target, stage, elapsed), with a Start control.
3. **Llama models**: current served model (or "stopped"), list of model profiles, switch control.

The dashboard stays reachable at `#/` (logo/Back); only the nav button changes. Today: `App.tsx:884` button -> `go(dashboardHash())`.

## Non-goals

- Validate, canary, promote-dataset, promote-model stay on the CLI.
- No arbitrary command execution from the browser.
- SaySo LFM is not a switchable chat model (it is a training artifact only).
- No new daemon on the VM (unless decision 1 is changed).

## Evidence (read-only, VM `llm` probed 2026-10-05)

- All GPU work goes through `/srv/llm/bin/gpu` (source: SaySo `scripts/llm-host/gpu`): `status`, `serve [llama|vllm]`, `stop`, `train lfm|wake <cmd>`, `run <label> <cmd>`. State file `/srv/llm/run/gpu.state` is `idle | serve | train:<label> <owner> <worker> <started>`. Each turn logs to `/srv/llm/run/train-<label>-<ts>.log`. A babysitter records error signatures and stalls as incidents.
- SaySo drives Unsloth via `./sayso generate|validate|canary|promote-dataset|train|eval|promote-model`; records in `training/runs/promotion/`. `train` goes through `gpu train lfm`.
- Wake training runs under `gpu train wake` with `/srv/llm/data/wake/.venv`.
- `./sayso generate` runs `gpu serve`, then `generators.cli` writing `training/datasets/candidates/<run-id>.jsonl`, then `gpu stop`. It has no `gpu.state` entry of its own, so progress comes from the growing candidate file (row count vs the target in `configs/generation/full_sft_v5.yaml`) plus the process list.
- **Model switching today:** no mechanism. `gpu serve llama` runs `docker compose -f /srv/llm/sayso/scripts/llm-host/llama-compose.yml up -d`; that file hardcodes `--model`, MTP draft, alias, ctx 122880, 2 slots, image tag and KV types. Models are not interchangeable by changing one path. Past swaps (`qwen-swap-babysit.sh`, `ud-swap-babysit.sh`, `xl-swap-babysit.sh`) back up the compose file, atomically `mv` a different whole compose into place, then `compose down/up` inside a `gpu run` turn. The full prior configs survive as `llama-compose.yml.bak-pre-*`.
- GGUFs live as symlinks in `/srv/llm/data/llama.cpp/models` (Qwen3.8-27B Q4_K_M / attnQ6 / UD-Q4_K_M / UD-Q4_K_XL, Swift-1.5).
- Current state: llama is **not running** (`gpu status`: default runtime up: no). Only the Unsloth container is up.

## Design

### Backend (`crates/ajax-web`, new slice `slices/training.rs`)

Thin adapter. Ajax core owns no training truth; the `gpu` wrapper, its state file and the SaySo records stay authoritative (AGENTS.md: external tools own their own reality). The slice shells out and reshapes output only.

- `GET /api/training/status`: generation progress (newest candidate `.jsonl` line count / target, running or finished) + `gpu status` + `gpu.state` + progress parse of the active `train-*.log` (step/total, loss, ETA for Unsloth; epoch / FA-per-hr for wake).
- `POST /api/training/{generate|lfm-train|lfm-eval|wake-train}/start` and `POST /api/training/stop`: fixed allowlisted commands only; **no browser-supplied argv**.
- `GET /api/training/models`: profiles + active + running/stopped. `POST /api/training/models/switch`: profile name must be in the server-side list.
- Auth: same session/auth layer as the other mutating web routes. Start / Stop / Switch need an explicit confirm in the UI (switch/stop evicts a live runtime or kills a run).

### Model switching = swap whole config files

- Each chat model is one complete compose file at `scripts/llm-host/profiles/<name>.yml` in the SaySo repo. No flag editing anywhere; each profile carries its own ctx, slots, draft model, template and image.
- Seed from configs already on the VM:
  - `atomic`: current `llama-compose.yml` (alias `qwen3.8-27b`, UD-Q4_K_XL). "Atomic" is Matt's name for the config currently on the VM.
  - `swift-1.5`: `llama-compose.yml.bak-pre-qwen-swap-20261004-174306`.
  - `qwen27b-q4km`: `.bak-pre-ud-swap-20261004-211905`.
  - `qwen27b-udq4km`, `qwen27b-attnq6`: matching `.bak-pre-*` files.
  - More models are just new files.
- Active config is the symlink `/srv/llm/run/llama-compose.active`; `gpu`'s `LLAMA_COMPOSE` default points at it. New `gpu` subcommand (e.g. `gpu profile list|set <name>`) replaces the hand-rolled swap-script pattern: repoint symlink atomically, then `gpu stop; gpu serve` inside the lock. Refused while a `train:*` state holds the GPU. Works while stopped (sets the profile; Start serves it).

### Frontend (`web/src/features/training/`, exported via `public.ts`)

`TrainingModal` on the existing `shared/ui/sheet.tsx`; polls status like other resources; nav wiring in `App.tsx`. Modal state only, no new hash route. Shows "stopped" as a first-class state with a Start button.

### Docs

Update `docs/architecture/web-cockpit.md` in the same change (new slice, external-authority note, security assumptions).

## Decisions (defaults apply on approval)

1. **Transport:** web host runs `ssh llm gpu ...` through a restricted key with a forced-command allowlist on the VM. Alternative: small HTTP agent on the VM.
2. **Start scope:** generate + Unsloth train + eval + wake train. Wake config chosen from a server-side list. Generation target total parsed server-side from the yaml; if impractical, show row count only.
3. **Model switch:** profiles as above. Decided by Matt (swap model configs; chat models only; Atomic = current config).
4. **Button label:** `Train` (alternatives: `GPU`, `Models`).

## Tasks

- [x] T1 backend slice + tests (status/log parsing, allowlist, auth, requests without confirm rejected)
- [~] T2 VM side (live on VM; repo port to SaySo NOT done): `gpu profile` subcommand, profiles dir, forced-command wrapper (live-host work done directly; the repo copy in SaySo ported via router)
- [x] T3 frontend modal + nav rename + tests (existing nav tests assert the `Dashboard` label; update for the rename, do not weaken)
- [x] T4 architecture docs
- [x] T5 gate: exact CI commands, `web:lint`, vitest, nextest, clippy/fmt/rustdoc `-D warnings`, Playwright `web:smoke` mobile-webkit (`pkill -f vite; CI=1 npm run web:smoke`)

## Risks

- Browser gains control of a GPU host: mitigated by allowlist, auth, confirm step, no argv from the client, forced-command key.
- Stop/switch while generation (which owns serve/stop itself) is running could leave the run half-done; switch must also refuse during `generate`.
- `gpu` and compose edits on the live VM are hard to reverse: back up before changing, switch only via atomic rename.

## Verification

2026-10-05: cargo nextest --workspace, clippy --all-targets --all-features -D warnings, fmt --check, `RUSTDOCFLAGS=-D warnings cargo doc --no-deps --all-features`, web:check, web:lint, full vitest (1529 passed), web:build:check, Playwright web:smoke mobile-webkit (152 passed, 2 skipped): all pass.
Deviations: (1) App.test.tsx test "marks the dashboard nav button as current" became "never marks bottom-nav action buttons as current" (button removed); e2e specs re-target `#/` navigation. Needs Matt's sign-off. (2) styleSources.ts BASELINE numbers updated for the intentional CSS addition (training styles in settings.css). (3) Host contract gained additive `run.running` and `profile_details`. (4) Host script gpu-ctl, profiles, `gpu` edit, forced-command key exist only on the VM.

## Execution rules

All repo writes go through `model-router` (one EXECUTION per task). No writes before approval.
