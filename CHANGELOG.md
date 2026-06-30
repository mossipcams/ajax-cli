# Changelog

All notable Ajax CLI changes should be recorded here.

## [0.26.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.26.0...ajax-cli-v0.26.1) (2026-06-30)


### Bug Fixes

* **web:** lock document scroll and shrink chrome for full-screen mobile terminal ([#243](https://github.com/mossipcams/ajax-cli/issues/243)) ([3c64715](https://github.com/mossipcams/ajax-cli/commit/3c64715e891b649c890a56af31d890d7d9e0a9aa))

## [0.26.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.25.1...ajax-cli-v0.26.0) (2026-06-30)


### Features

* **web:** full-screen, keyboard-aware mobile terminal for iOS Safari ([#241](https://github.com/mossipcams/ajax-cli/issues/241)) ([e189df3](https://github.com/mossipcams/ajax-cli/commit/e189df37fff7b5809f1355923f58183d4e356a1d))

## [0.25.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.25.0...ajax-cli-v0.25.1) (2026-06-30)


### Bug Fixes

* **web:** repair terminal delete key and zero-lag overlay tracking ([#239](https://github.com/mossipcams/ajax-cli/issues/239)) ([9de77e2](https://github.com/mossipcams/ajax-cli/commit/9de77e2a07fb97f46175db54eff99c1982d9855f))

## [0.25.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.24.0...ajax-cli-v0.25.0) (2026-06-30)


### Features

* overhaul web task terminal for mobile and remove pane fallback ([#238](https://github.com/mossipcams/ajax-cli/issues/238)) ([f517ec7](https://github.com/mossipcams/ajax-cli/commit/f517ec75e663995079eaef2d61a9d2dbcad1cb99))


### Bug Fixes

* prevent web server wedge from blocking terminal PTY cleanup ([#236](https://github.com/mossipcams/ajax-cli/issues/236)) ([9b96229](https://github.com/mossipcams/ajax-cli/commit/9b962290c1bf2ba349bb1508aba8da2895e118ce))

## [0.24.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.23.0...ajax-cli-v0.24.0) (2026-06-30)


### Features

* add authenticated web task terminal bridge ([#234](https://github.com/mossipcams/ajax-cli/issues/234)) ([bde33d8](https://github.com/mossipcams/ajax-cli/commit/bde33d8952f37707cc8a3c7608cf6b1817937dda))

## [0.23.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.8...ajax-cli-v0.23.0) (2026-06-30)


### Features

* enable web cockpit resume and free-form task input ([#232](https://github.com/mossipcams/ajax-cli/issues/232)) ([b01045b](https://github.com/mossipcams/ajax-cli/commit/b01045b8e68620691a61bedb9c84158ec90ca9d3))

## [0.22.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.7...ajax-cli-v0.22.8) (2026-06-30)


### Bug Fixes

* harden web operations and new-task repo validation ([#230](https://github.com/mossipcams/ajax-cli/issues/230)) ([681d414](https://github.com/mossipcams/ajax-cli/commit/681d41420449c2388e4bbca2fa2914bb1437ed59))

## [0.22.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.6...ajax-cli-v0.22.7) (2026-06-29)


### Bug Fixes

* ensure recommended primary action is in available actions ([#227](https://github.com/mossipcams/ajax-cli/issues/227)) ([f86417e](https://github.com/mossipcams/ajax-cli/commit/f86417ea560fd122e52deb0674281281140ebde8))


### Code Refactoring

* extract task_operations into file-backed modules ([#226](https://github.com/mossipcams/ajax-cli/issues/226)) ([04cabac](https://github.com/mossipcams/ajax-cli/commit/04cabac16007fdeb81650f94d48ed536e16fb5f0))

## [0.22.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.5...ajax-cli-v0.22.6) (2026-06-29)


### Bug Fixes

* remove legacy web router and stabilize smoke hashes ([#224](https://github.com/mossipcams/ajax-cli/issues/224)) ([f12025c](https://github.com/mossipcams/ajax-cli/commit/f12025c7b5aeb27fd99550b9d6bf4f7b4ad24799))

## [0.22.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.4...ajax-cli-v0.22.5) (2026-06-29)


### Bug Fixes

* **web:** harden api session renewal ([#222](https://github.com/mossipcams/ajax-cli/issues/222)) ([e8eb0e6](https://github.com/mossipcams/ajax-cli/commit/e8eb0e6b8e8b58f77f30a5a008a68ca5f47b7178))

## [0.22.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.3...ajax-cli-v0.22.4) (2026-06-29)


### Bug Fixes

* sharpen ajax operator loop ([#220](https://github.com/mossipcams/ajax-cli/issues/220)) ([dc63907](https://github.com/mossipcams/ajax-cli/commit/dc639072eff1c8dbeaf03ac7144b3462edcd7aa2))

## [0.22.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.2...ajax-cli-v0.22.3) (2026-06-27)


### Bug Fixes

* **web:** renew browser session after stale API cookie ([#217](https://github.com/mossipcams/ajax-cli/issues/217)) ([38ed536](https://github.com/mossipcams/ajax-cli/commit/38ed5365085922998084f91aae161e4ac000e029))

## [0.22.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.1...ajax-cli-v0.22.2) (2026-06-27)


### Bug Fixes

* gate web API with browser session ([#215](https://github.com/mossipcams/ajax-cli/issues/215)) ([e1ab2a1](https://github.com/mossipcams/ajax-cli/commit/e1ab2a1126c15c821416fe83c02bf75aec01d035))

## [0.22.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.22.0...ajax-cli-v0.22.1) (2026-06-27)


### Bug Fixes

* send Access credentials with web API fetches ([#213](https://github.com/mossipcams/ajax-cli/issues/213)) ([4849122](https://github.com/mossipcams/ajax-cli/commit/48491229f4bba5685999f9a23ceb7fb453a8fd71))

## [0.22.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.21.2...ajax-cli-v0.22.0) (2026-06-26)


### Features

* **web:** modernize mobile cockpit with touch gestures, skeleton loads, and depth ([#210](https://github.com/mossipcams/ajax-cli/issues/210)) ([b280991](https://github.com/mossipcams/ajax-cli/commit/b2809915eab85448ad5adf5df6ed99c943e21d72))


### Bug Fixes

* allow empty registry wipe save ([#211](https://github.com/mossipcams/ajax-cli/issues/211)) ([7778248](https://github.com/mossipcams/ajax-cli/commit/77782486bf36303b1f3ba9963b4cf1789d1ddadd))

## [0.21.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.21.1...ajax-cli-v0.21.2) (2026-06-26)


### Bug Fixes

* **web:** classify reachable backend errors ([#207](https://github.com/mossipcams/ajax-cli/issues/207)) ([5b456b6](https://github.com/mossipcams/ajax-cli/commit/5b456b6a7a38a4901f9d2ce68ea89e7e44df00de))
* **web:** prevent tls accept starvation ([#209](https://github.com/mossipcams/ajax-cli/issues/209)) ([febb44e](https://github.com/mossipcams/ajax-cli/commit/febb44e07546d46f16d2787d829ad8d3c38d605d))

## [0.21.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.21.0...ajax-cli-v0.21.1) (2026-06-26)


### Bug Fixes

* **web:** restore cockpit styling and guard against CSS regressions ([#205](https://github.com/mossipcams/ajax-cli/issues/205)) ([88378ed](https://github.com/mossipcams/ajax-cli/commit/88378edfa471b93db31b678b8211dd2bc1fe1e4e))

## [0.21.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.6...ajax-cli-v0.21.0) (2026-06-25)


### Features

* **web:** begin Svelte+TS migration — contracts, toolchain, typed boundaries ([#196](https://github.com/mossipcams/ajax-cli/issues/196)) ([ad3d2f7](https://github.com/mossipcams/ajax-cli/commit/ad3d2f7d7a7865402f7e0ed26833e0e29e68738a))
* **web:** build Svelte entry and mount the shell ([#197](https://github.com/mossipcams/ajax-cli/issues/197)) ([1508627](https://github.com/mossipcams/ajax-cli/commit/1508627415113fc10d3248d977f902952505757f))
* **web:** migrate cockpit UI areas to Svelte and switch Rust serving to bundle ([#200](https://github.com/mossipcams/ajax-cli/issues/200)) ([31c16e7](https://github.com/mossipcams/ajax-cli/commit/31c16e7afe8b7250de644b2b3dbab2e960e36367))


### Bug Fixes

* **web:** enforce frontend contract parity ([#204](https://github.com/mossipcams/ajax-cli/issues/204)) ([184821c](https://github.com/mossipcams/ajax-cli/commit/184821c4e2ee47f64570abe7a10551ba49beebd6))

## [0.20.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.5...ajax-cli-v0.20.6) (2026-06-25)


### Bug Fixes

* balance bottom nav after dropping Settings button ([#193](https://github.com/mossipcams/ajax-cli/issues/193)) ([07e205e](https://github.com/mossipcams/ajax-cli/commit/07e205e9926f5198b74432c149bab3ab799acad7))
* reconcile renamed task tmux sessions ([#198](https://github.com/mossipcams/ajax-cli/issues/198)) ([f4a8918](https://github.com/mossipcams/ajax-cli/commit/f4a89186f3b2055bad2411ad09a6158501906cf5))
* reload cockpit state on sqlite revision changes ([#194](https://github.com/mossipcams/ajax-cli/issues/194)) ([13a54bb](https://github.com/mossipcams/ajax-cli/commit/13a54bb61905f4fdf525af6f446b4ee67b5cc372))

## [0.20.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.4...ajax-cli-v0.20.5) (2026-06-25)


### Bug Fixes

* streamline web cockpit detail view and stop poll jitter ([#191](https://github.com/mossipcams/ajax-cli/issues/191)) ([4a72879](https://github.com/mossipcams/ajax-cli/commit/4a7287939fcff8305d28d70da491d2667869555f))

## [0.20.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.3...ajax-cli-v0.20.4) (2026-06-25)


### Bug Fixes

* prevent ctrl-q save conflicts after cockpit reload ([#188](https://github.com/mossipcams/ajax-cli/issues/188)) ([2687b6d](https://github.com/mossipcams/ajax-cli/commit/2687b6d1372cf1b6c6f8c479c36fe9d413801f09))
* prevent stale autosnooze task reappearance ([#190](https://github.com/mossipcams/ajax-cli/issues/190)) ([1e5c803](https://github.com/mossipcams/ajax-cli/commit/1e5c8036a24b46e7508a31b5503a05e059f3fac7))

## [0.20.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.2...ajax-cli-v0.20.3) (2026-06-24)


### Bug Fixes

* consolidate drop teardown resource metadata ([#186](https://github.com/mossipcams/ajax-cli/issues/186)) ([6791322](https://github.com/mossipcams/ajax-cli/commit/6791322475ab1094550d8e530c0fe729675282fd))

## [0.20.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.1...ajax-cli-v0.20.2) (2026-06-24)


### Bug Fixes

* prevent empty registry saves from wiping state ([#184](https://github.com/mossipcams/ajax-cli/issues/184)) ([02a944d](https://github.com/mossipcams/ajax-cli/commit/02a944d50c698398b56b8070a03933fe9c35d525))

## [0.20.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.20.0...ajax-cli-v0.20.1) (2026-06-22)


### Bug Fixes

* restore release please token selection ([#182](https://github.com/mossipcams/ajax-cli/issues/182)) ([8135d9a](https://github.com/mossipcams/ajax-cli/commit/8135d9a6e49dfc357e46f11eb30b578575831b34))

## [0.20.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.7...ajax-cli-v0.20.0) (2026-06-22)


### Features

* normalize sqlite registry schema to v8 ([#179](https://github.com/mossipcams/ajax-cli/issues/179)) ([bf1b602](https://github.com/mossipcams/ajax-cli/commit/bf1b6026402af8f758d35a23cf298e41f63e6177))


### Bug Fixes

* use github.token for release please ([#180](https://github.com/mossipcams/ajax-cli/issues/180)) ([5679ab8](https://github.com/mossipcams/ajax-cli/commit/5679ab8e62580b595a74dfbbe476a87ac2e7387b))

## [0.19.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.6...ajax-cli-v0.19.7) (2026-06-18)


### Bug Fixes

* simplify sqlite registry mapping and adapter command builders ([#175](https://github.com/mossipcams/ajax-cli/issues/175)) ([57c187e](https://github.com/mossipcams/ajax-cli/commit/57c187e1c0028800d550cd343a33861450426c14))

## [0.19.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.5...ajax-cli-v0.19.6) (2026-06-13)


### Bug Fixes

* generate graphify output per task worktree ([#172](https://github.com/mossipcams/ajax-cli/issues/172)) ([f4bae08](https://github.com/mossipcams/ajax-cli/commit/f4bae08acd9f8a0f0406390f5bde24871c88a843))

## [0.19.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.4...ajax-cli-v0.19.5) (2026-06-12)


### Bug Fixes

* prune worktree before dropping branch ([#170](https://github.com/mossipcams/ajax-cli/issues/170)) ([d504c31](https://github.com/mossipcams/ajax-cli/commit/d504c31014a2cf73815855d9e5869d29a2ab5e50))

## [0.19.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.3...ajax-cli-v0.19.4) (2026-06-12)


### Performance Improvements

* speed up task launch and cleanup flows ([#168](https://github.com/mossipcams/ajax-cli/issues/168)) ([c11669d](https://github.com/mossipcams/ajax-cli/commit/c11669d0e689b8ef5f484b12a692831e54490bef))

## [0.19.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.2...ajax-cli-v0.19.3) (2026-06-12)


### Bug Fixes

* keep confirmed cockpit action selected ([#166](https://github.com/mossipcams/ajax-cli/issues/166)) ([8aff147](https://github.com/mossipcams/ajax-cli/commit/8aff147421301eb8d5429ca8f4fe859e41a5f5d7))

## [0.19.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.1...ajax-cli-v0.19.2) (2026-06-12)


### Bug Fixes

* preserve cockpit drop confirmation across refresh ([#163](https://github.com/mossipcams/ajax-cli/issues/163)) ([470d9b8](https://github.com/mossipcams/ajax-cli/commit/470d9b8b15b2baa5c8b8dd89c432648a44b00872))
* recover web cockpit when registry state diverges from disk ([#164](https://github.com/mossipcams/ajax-cli/issues/164)) ([55e386b](https://github.com/mossipcams/ajax-cli/commit/55e386b2acf45fb4b54b7003c572c18138171913))

## [0.19.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.19.0...ajax-cli-v0.19.1) (2026-06-10)


### Bug Fixes

* keep native cockpit in sync with web cockpit state ([#160](https://github.com/mossipcams/ajax-cli/issues/160)) ([3477afe](https://github.com/mossipcams/ajax-cli/commit/3477afe68604bbe5763577e5ff4e0c63d72e96ae))

## [0.19.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.18.0...ajax-cli-v0.19.0) (2026-06-10)


### Features

* align task status lifecycle across cockpit surfaces ([#158](https://github.com/mossipcams/ajax-cli/issues/158)) ([7f04508](https://github.com/mossipcams/ajax-cli/commit/7f04508116909d55e73529cbc3c48d35171aa006))

## [0.18.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.6...ajax-cli-v0.18.0) (2026-06-09)


### Features

* adopt agent-deck-inspired status derivation ([#156](https://github.com/mossipcams/ajax-cli/issues/156)) ([20d62ff](https://github.com/mossipcams/ajax-cli/commit/20d62ff23cf46c8e9f1d52c557f428990abb3843))

## [0.17.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.5...ajax-cli-v0.17.6) (2026-06-09)


### Bug Fixes

* make task runtime status authoritative ([#154](https://github.com/mossipcams/ajax-cli/issues/154)) ([b504ba0](https://github.com/mossipcams/ajax-cli/commit/b504ba028f437466c26dd61ab61a2417788d4a50))

## [0.17.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.4...ajax-cli-v0.17.5) (2026-06-08)


### Bug Fixes

* declutter web task page and fix terminal disclosure auto-collapse ([#152](https://github.com/mossipcams/ajax-cli/issues/152)) ([403dbfd](https://github.com/mossipcams/ajax-cli/commit/403dbfdb6cfaded7c803803bd6c8eb1274a02d2b))

## [0.17.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.3...ajax-cli-v0.17.4) (2026-06-08)


### Bug Fixes

* harden concurrent saves and web operation coordination ([#150](https://github.com/mossipcams/ajax-cli/issues/150)) ([a19e8fb](https://github.com/mossipcams/ajax-cli/commit/a19e8fb114d44de3301fe6b3eec7a0db53719c21))

## [0.17.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.2...ajax-cli-v0.17.3) (2026-06-07)


### Bug Fixes

* make dev web restart safer ([#148](https://github.com/mossipcams/ajax-cli/issues/148)) ([388d947](https://github.com/mossipcams/ajax-cli/commit/388d947adea0c1323f06173bb6d69cf222c2bb7b))

## [0.17.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.1...ajax-cli-v0.17.2) (2026-06-07)


### Bug Fixes

* make partial task drops resilient ([#146](https://github.com/mossipcams/ajax-cli/issues/146)) ([8af6f0c](https://github.com/mossipcams/ajax-cli/commit/8af6f0c6b70395a43501ea24c2f48d0319403c54))

## [0.17.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.17.0...ajax-cli-v0.17.1) (2026-06-06)


### Bug Fixes

* create task worktrees from origin default branch ([#144](https://github.com/mossipcams/ajax-cli/issues/144)) ([e534226](https://github.com/mossipcams/ajax-cli/commit/e534226c83f72165a8b6027c2317ff634c17ccc2))

## [0.17.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.16.0...ajax-cli-v0.17.0) (2026-06-05)


### Features

* speed up post-startup cockpit polling and web reliability ([#140](https://github.com/mossipcams/ajax-cli/issues/140)) ([c5353c3](https://github.com/mossipcams/ajax-cli/commit/c5353c3af1fa380921803ce523efd56a2c6f3d7d))


### Bug Fixes

* **cli:** rebuild cockpit snapshot when cached tasks are removed ([#142](https://github.com/mossipcams/ajax-cli/issues/142)) ([838e629](https://github.com/mossipcams/ajax-cli/commit/838e629d96965e425818b0d1786eecb7b0b6cdd3))

## [0.16.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.15.0...ajax-cli-v0.16.0) (2026-06-04)


### Features

* **web:** redesign Safari cockpit with inbox-first dashboard ([#137](https://github.com/mossipcams/ajax-cli/issues/137)) ([6134fe0](https://github.com/mossipcams/ajax-cli/commit/6134fe036c8e7f729cb3a5d4e7030ecddc5576da))

## [0.15.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.14.1...ajax-cli-v0.15.0) (2026-06-04)


### Features

* make web cockpit Safari-first ([#134](https://github.com/mossipcams/ajax-cli/issues/134)) ([8e4e89a](https://github.com/mossipcams/ajax-cli/commit/8e4e89a911d5ef78bc76fc727390da506eb2efbd))

## [0.14.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.14.0...ajax-cli-v0.14.1) (2026-06-04)


### Bug Fixes

* **web:** repair iOS PWA stale shell recovery ([#132](https://github.com/mossipcams/ajax-cli/issues/132)) ([3523d7b](https://github.com/mossipcams/ajax-cli/commit/3523d7b11bbfb44d9ba019c106ce98f1159b3051))

## [0.14.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.13.1...ajax-cli-v0.14.0) (2026-06-04)


### Features

* sync default branch and optional graphify before task worktrees ([#129](https://github.com/mossipcams/ajax-cli/issues/129)) ([351cf4d](https://github.com/mossipcams/ajax-cli/commit/351cf4d764e85df3ed1c91c92285f865e468b8b3))


### Bug Fixes

* avoid nested tmux attach flicker ([#131](https://github.com/mossipcams/ajax-cli/issues/131)) ([503f89b](https://github.com/mossipcams/ajax-cli/commit/503f89b168b69682cc93ccf956fb4346bc182a3d))
* **web:** defer iOS PWA service worker registration ([#128](https://github.com/mossipcams/ajax-cli/issues/128)) ([baeb875](https://github.com/mossipcams/ajax-cli/commit/baeb8756ba98b692b73861ffb58f0528d3606216))

## [0.13.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.13.0...ajax-cli-v0.13.1) (2026-06-03)


### Bug Fixes

* **web:** deliver push notifications instead of crashing the poller ([#125](https://github.com/mossipcams/ajax-cli/issues/125)) ([acf3684](https://github.com/mossipcams/ajax-cli/commit/acf36842d92449b8f5ba9b93fb78d9f2a822d342))

## 0.1.0

- Added production-readiness hardening for doctor checks, SQLite schema
  versioning, and state export backups.
- Documented install, configuration, first-run, and release expectations.
- Bootstrapped Release Please release automation.
