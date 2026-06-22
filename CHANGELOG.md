# Changelog

All notable Ajax CLI changes should be recorded here.

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
