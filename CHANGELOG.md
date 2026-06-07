# Changelog

All notable Ajax CLI changes should be recorded here.

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
