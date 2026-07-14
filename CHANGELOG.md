# Changelog

All notable Ajax CLI changes should be recorded here.

## [0.42.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.8...ajax-cli-v0.42.9) (2026-07-14)


### Bug Fixes

* **web:** stop Surface V2 full-terminal yellow wash (wterm inline bg smear) ([#480](https://github.com/mossipcams/ajax-cli/issues/480)) ([458dfef](https://github.com/mossipcams/ajax-cli/commit/458dfefd872e24a14de746506d20381b7ba1687c))

## [0.42.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.7...ajax-cli-v0.42.8) (2026-07-14)


### Bug Fixes

* **web:** stop iOS Surface V2 solid olive terminal paint ([#478](https://github.com/mossipcams/ajax-cli/issues/478)) ([dc06f6a](https://github.com/mossipcams/ajax-cli/commit/dc06f6a1abf1effe061b229015e8f474c64abcad))

## [0.42.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.6...ajax-cli-v0.42.7) (2026-07-14)


### Bug Fixes

* **web:** catch Surface V2 yellow banner on mobile WebKit ([#476](https://github.com/mossipcams/ajax-cli/issues/476)) ([25064f7](https://github.com/mossipcams/ajax-cli/commit/25064f72f59654f6f12e61e392d02c613fb6d084))

## [0.42.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.5...ajax-cli-v0.42.6) (2026-07-14)


### Bug Fixes

* **web:** prove wterm GhosttyCore init with real WASM tests ([#474](https://github.com/mossipcams/ajax-cli/issues/474)) ([deb68ae](https://github.com/mossipcams/ajax-cli/commit/deb68ae8923f50f78a0262eb61ba29a35ce3d027))

## [0.42.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.4...ajax-cli-v0.42.5) (2026-07-14)


### Bug Fixes

* **web:** pass options when constructing GhosttyCore for wterm ([#472](https://github.com/mossipcams/ajax-cli/issues/472)) ([cbdccde](https://github.com/mossipcams/ajax-cli/commit/cbdccdee24c3e5d8452b982ab1a145a5ae64497b))

## [0.42.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.3...ajax-cli-v0.42.4) (2026-07-14)


### Bug Fixes

* **web:** instantiate wterm WASM without Safari blob fetch ([#469](https://github.com/mossipcams/ajax-cli/issues/469)) ([0ac5de9](https://github.com/mossipcams/ajax-cli/commit/0ac5de997e7fd5421bc6a47af2477d8bf3123fa4))

## [0.42.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.2...ajax-cli-v0.42.3) (2026-07-14)


### Bug Fixes

* **web:** validate wterm WASM before GhosttyCore.init ([#467](https://github.com/mossipcams/ajax-cli/issues/467)) ([f519010](https://github.com/mossipcams/ajax-cli/commit/f519010a14ac4751bde349fd556b5a679eaa8dcf))

## [0.42.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.1...ajax-cli-v0.42.2) (2026-07-14)


### Bug Fixes

* **web:** align wterm Surface V2 sizing with PTY output ([#465](https://github.com/mossipcams/ajax-cli/issues/465)) ([2f202c3](https://github.com/mossipcams/ajax-cli/commit/2f202c382a2d96bd6df9311170f845afc3639440))

## [0.42.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.0...ajax-cli-v0.42.1) (2026-07-14)


### Bug Fixes

* **web:** serve wterm Ghostty WASM on a distinct path ([#463](https://github.com/mossipcams/ajax-cli/issues/463)) ([ecf5fd8](https://github.com/mossipcams/ajax-cli/commit/ecf5fd8902463b7dd5a52c96ea14dce32ded22c4))

## [0.42.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.41.2...ajax-cli-v0.42.0) (2026-07-14)


### Features

* **web:** experimental wterm Terminal Surface V2 spike ([#461](https://github.com/mossipcams/ajax-cli/issues/461)) ([5d43c98](https://github.com/mossipcams/ajax-cli/commit/5d43c98ee81e749453dec6a4fd5ec75383afb1f6))

## [0.41.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.41.1...ajax-cli-v0.41.2) (2026-07-14)


### Bug Fixes

* **web:** trail output flushes while reading scrollback ([#459](https://github.com/mossipcams/ajax-cli/issues/459)) ([e3e007c](https://github.com/mossipcams/ajax-cli/commit/e3e007c7eb5b8a8d330647a47b772260ce2bc8cc))

## [0.41.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.41.0...ajax-cli-v0.41.1) (2026-07-14)


### Bug Fixes

* **web:** touch must not re-pin scrollback; align sub-cell translate with renderer frame ([#457](https://github.com/mossipcams/ajax-cli/issues/457)) ([367cd49](https://github.com/mossipcams/ajax-cli/commit/367cd49af6a72232e3fd4b3a7b521cf6fc403576))

## [0.41.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.7...ajax-cli-v0.41.0) (2026-07-14)


### Features

* **web:** fluid terminal scrolling, typing echo, and task-open latency ([#455](https://github.com/mossipcams/ajax-cli/issues/455)) ([f200f8e](https://github.com/mossipcams/ajax-cli/commit/f200f8e33aa735cae02af6befe3d9bbad961ad26))

## [0.40.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.6...ajax-cli-v0.40.7) (2026-07-14)


### Bug Fixes

* **web:** anchor keyboard input and speed up terminal load ([#453](https://github.com/mossipcams/ajax-cli/issues/453)) ([4304bce](https://github.com/mossipcams/ajax-cli/commit/4304bce29df98413c01fc3fba2bec2fb2ac1d1ac))

## [0.40.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.5...ajax-cli-v0.40.6) (2026-07-14)


### Reverts

* **web:** undo keyboard-band terminal fit, JWT redaction, HIG taps ([#448](https://github.com/mossipcams/ajax-cli/issues/448)) ([#451](https://github.com/mossipcams/ajax-cli/issues/451)) ([67b6fee](https://github.com/mossipcams/ajax-cli/commit/67b6fee0c91d9477faa6d43e7da45eb2fc1e5301))

## [0.40.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.4...ajax-cli-v0.40.5) (2026-07-14)


### Bug Fixes

* **cli:** keep cockpit open after ctrl-q save error ([#447](https://github.com/mossipcams/ajax-cli/issues/447)) ([b49d7a9](https://github.com/mossipcams/ajax-cli/commit/b49d7a9a1c3353a2dbcf40342d234f9ea960e577))
* **web:** keyboard-band terminal fit, key row reach, JWT redaction, HIG taps ([#448](https://github.com/mossipcams/ajax-cli/issues/448)) ([46cc305](https://github.com/mossipcams/ajax-cli/commit/46cc305b6f3322057386ecffce24124743742532))

## [0.40.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.3...ajax-cli-v0.40.4) (2026-07-13)


### Bug Fixes

* **scripts:** always pull origin/main on web restart ([#444](https://github.com/mossipcams/ajax-cli/issues/444)) ([6fbf242](https://github.com/mossipcams/ajax-cli/commit/6fbf2429781d2f07bc882adbc48bf60876b35436))
* **web:** fill scaled terminal height and authorize last-task Drop save ([#445](https://github.com/mossipcams/ajax-cli/issues/445)) ([561df9f](https://github.com/mossipcams/ajax-cli/commit/561df9fdc2dc5fa1fa076d7e1043e86bda0e1b4e))

## [0.40.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.2...ajax-cli-v0.40.3) (2026-07-13)


### Bug Fixes

* **web:** scale terminal on inner layer so expand stays tappable ([#442](https://github.com/mossipcams/ajax-cli/issues/442)) ([494156c](https://github.com/mossipcams/ajax-cli/commit/494156c9cea5e09c1104d8467ee88a06d74487b9))

## [0.40.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.1...ajax-cli-v0.40.2) (2026-07-13)


### Bug Fixes

* **web:** scale phone terminal to agent width and stabilize task order ([#440](https://github.com/mossipcams/ajax-cli/issues/440)) ([137c4b7](https://github.com/mossipcams/ajax-cli/commit/137c4b76d331071ed4b6686400e41266de0a0422))

## [0.40.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.40.0...ajax-cli-v0.40.1) (2026-07-13)


### Bug Fixes

* **web:** stabilize Waiting/Running status and notify once per episode ([#438](https://github.com/mossipcams/ajax-cli/issues/438)) ([94dca90](https://github.com/mossipcams/ajax-cli/commit/94dca909f0e9ae21d437dc6171730fdde719d911))

## [0.40.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.39.2...ajax-cli-v0.40.0) (2026-07-13)


### Features

* **web:** retire ajax-cli feature lattice and collapse duplicated guards ([#435](https://github.com/mossipcams/ajax-cli/issues/435)) ([e09e0a6](https://github.com/mossipcams/ajax-cli/commit/e09e0a6424c9a1926a5a3d688f4329bbb980bdf6))


### Performance Improvements

* **web:** defer terminal bundle and skip Git probe on browser resume ([#437](https://github.com/mossipcams/ajax-cli/issues/437)) ([83bedbe](https://github.com/mossipcams/ajax-cli/commit/83bedbe95e5b7fbff0fb853fe768a67c3c8d2c95))

## [0.39.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.39.1...ajax-cli-v0.39.2) (2026-07-11)


### Code Refactoring

* **core:** ratify task_operations as the vertical slice layer ([#433](https://github.com/mossipcams/ajax-cli/issues/433)) ([6954be3](https://github.com/mossipcams/ajax-cli/commit/6954be352437f7ff181b465f174e6bec021588b8))
* **web:** extract terminal layout, scroll, zero-lag, and clipboard owners ([#432](https://github.com/mossipcams/ajax-cli/issues/432)) ([a02fc20](https://github.com/mossipcams/ajax-cli/commit/a02fc20eb975f62e5f78d5f278992183beb7a857))

## [0.39.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.39.0...ajax-cli-v0.39.1) (2026-07-10)


### Bug Fixes

* hold notification re-arm for a cooldown after each delivery ([#431](https://github.com/mossipcams/ajax-cli/issues/431)) ([8d7e02c](https://github.com/mossipcams/ajax-cli/commit/8d7e02cfae55be24d075fc129681ba988955b4d8))
* **web:** seed terminal scrollback from tmux history ([#429](https://github.com/mossipcams/ajax-cli/issues/429)) ([9f7a0e4](https://github.com/mossipcams/ajax-cli/commit/9f7a0e4a57324135c39857e5e852a0d40ac22da0))

## [0.39.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.38.2...ajax-cli-v0.39.0) (2026-07-10)


### Features

* confirm waiting status before notifying and poll in background ([#427](https://github.com/mossipcams/ajax-cli/issues/427)) ([071bcb0](https://github.com/mossipcams/ajax-cli/commit/071bcb0219fad71848b46ef0a1c30e4b7cedea22))

## [0.38.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.38.1...ajax-cli-v0.38.2) (2026-07-10)


### Bug Fixes

* **core:** force-delete unmerged branches on cleanup drop ([#426](https://github.com/mossipcams/ajax-cli/issues/426)) ([2d56235](https://github.com/mossipcams/ajax-cli/commit/2d56235f9b5b6829e5b8c7bfb81d64954562293f))
* **web:** align task page full-bleed and reset terminal on reconnect ([#424](https://github.com/mossipcams/ajax-cli/issues/424)) ([262ab16](https://github.com/mossipcams/ajax-cli/commit/262ab160b668f3fd92ddf4b75699bf8a925967f2))

## [0.38.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.38.0...ajax-cli-v0.38.1) (2026-07-10)


### Bug Fixes

* **web:** separate selection teal from attention mustard ([#422](https://github.com/mossipcams/ajax-cli/issues/422)) ([042046f](https://github.com/mossipcams/ajax-cli/commit/042046f1a0a129ec6280ea1fb3ca055447c19d16))

## [0.38.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.11...ajax-cli-v0.38.0) (2026-07-10)


### Features

* send webhook notifications when tasks need attention ([#419](https://github.com/mossipcams/ajax-cli/issues/419)) ([b565a45](https://github.com/mossipcams/ajax-cli/commit/b565a45d91de1b9e9b33b7b58f3711bc8ab96db0))
* **web:** task recency, remembered defaults, and cockpit a11y polish ([#421](https://github.com/mossipcams/ajax-cli/issues/421)) ([a0ddeeb](https://github.com/mossipcams/ajax-cli/commit/a0ddeeb2c726c174244fe4fc6312302b494ed9d9))

## [0.37.11](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.10...ajax-cli-v0.37.11) (2026-07-10)


### Bug Fixes

* **web:** hide terminal key row overflow scrollbar ([#417](https://github.com/mossipcams/ajax-cli/issues/417)) ([532ca1e](https://github.com/mossipcams/ajax-cli/commit/532ca1e6ba8b876b3143caee3ca38eb2b6b03ce1))
* **web:** share start agent allowlist and delete dead web code ([#416](https://github.com/mossipcams/ajax-cli/issues/416)) ([787a136](https://github.com/mossipcams/ajax-cli/commit/787a136612fd0fdfbe7e27ef476f25c8a683ca40))

## [0.37.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.9...ajax-cli-v0.37.10) (2026-07-09)


### Bug Fixes

* **web:** stop fullscreen translate and hide route-scroll gutter ([#414](https://github.com/mossipcams/ajax-cli/issues/414)) ([db6c025](https://github.com/mossipcams/ajax-cli/commit/db6c025488fc8f7a065b46951871f9e6a37ac807))

## [0.37.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.8...ajax-cli-v0.37.9) (2026-07-09)


### Bug Fixes

* **web:** repair worktree action, overlay toast, clear zero-lag echo ghost ([#412](https://github.com/mossipcams/ajax-cli/issues/412)) ([1a43b38](https://github.com/mossipcams/ajax-cli/commit/1a43b38b1261eaaee5aa743f198527a3acccfe0e))

## [0.37.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.7...ajax-cli-v0.37.8) (2026-07-09)


### Bug Fixes

* **web:** position zero-lag overlay with renderer cell metrics ([#410](https://github.com/mossipcams/ajax-cli/issues/410)) ([9113133](https://github.com/mossipcams/ajax-cli/commit/9113133e69f1167e7483a6b9320b99b0a843979c))

## [0.37.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.6...ajax-cli-v0.37.7) (2026-07-09)


### Bug Fixes

* **web:** clear zero-lag overlay on char-by-char PTY echo ([#408](https://github.com/mossipcams/ajax-cli/issues/408)) ([e568b16](https://github.com/mossipcams/ajax-cli/commit/e568b165d84f811eb0c6f5808f8cdfb44d4d9c6e))

## [0.37.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.5...ajax-cli-v0.37.6) (2026-07-09)


### Bug Fixes

* **web:** stop iOS typing echo stretch and tighten keyboard chrome ([#406](https://github.com/mossipcams/ajax-cli/issues/406)) ([7697e85](https://github.com/mossipcams/ajax-cli/commit/7697e853aefc9bbd44ff1aaee8355c2f1fb0be40))

## [0.37.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.4...ajax-cli-v0.37.5) (2026-07-09)


### Performance Improvements

* **web:** cut mobile Cockpit battery cost without hurting terminal UX ([#404](https://github.com/mossipcams/ajax-cli/issues/404)) ([b83e544](https://github.com/mossipcams/ajax-cli/commit/b83e544160661b862bf825491f0bbf9182993098))

## [0.37.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.3...ajax-cli-v0.37.4) (2026-07-09)


### Bug Fixes

* **core:** recreate missing worktrees on repair and lock terminal ownership ([#401](https://github.com/mossipcams/ajax-cli/issues/401)) ([2061092](https://github.com/mossipcams/ajax-cli/commit/2061092d734d20ab81f5b0eed2e32bfa853aca7f))

## [0.37.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.2...ajax-cli-v0.37.3) (2026-07-09)


### Bug Fixes

* **web:** stabilize task terminal viewport chrome ([#399](https://github.com/mossipcams/ajax-cli/issues/399)) ([d91b2a8](https://github.com/mossipcams/ajax-cli/commit/d91b2a889e38f858ba9f8833c72c8e9af7297bac))

## [0.37.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.1...ajax-cli-v0.37.2) (2026-07-09)


### Bug Fixes

* **web:** repair fullscreen blank column, off-terminal echo, and iOS backspace repeat ([#397](https://github.com/mossipcams/ajax-cli/issues/397)) ([c7d2911](https://github.com/mossipcams/ajax-cli/commit/c7d29112b7aed157224f0635b8dcb168c3413650))

## [0.37.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.37.0...ajax-cli-v0.37.1) (2026-07-09)


### Bug Fixes

* **web:** pin keyboard-open shell to the visual viewport band ([#395](https://github.com/mossipcams/ajax-cli/issues/395)) ([cec3c56](https://github.com/mossipcams/ajax-cli/commit/cec3c561a923e7dd226004d114496c803dd10367))

## [0.37.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.36.1...ajax-cli-v0.37.0) (2026-07-09)


### Features

* **web:** fix inline terminal fill and paste/copy on iOS ([#393](https://github.com/mossipcams/ajax-cli/issues/393)) ([7ad8edc](https://github.com/mossipcams/ajax-cli/commit/7ad8edc6379e40314a9262997fc561c435b228d0))

## [0.36.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.36.0...ajax-cli-v0.36.1) (2026-07-09)


### Code Refactoring

* **core:** cut over-engineering from ponytail audit ([#391](https://github.com/mossipcams/ajax-cli/issues/391)) ([8a825bb](https://github.com/mossipcams/ajax-cli/commit/8a825bb8f7a41aa616af6191a107deeb09e8ec7e))

## [0.36.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.15...ajax-cli-v0.36.0) (2026-07-08)


### Features

* **web:** migrate terminal engine to rcarmo/ghostty-web v0.9.4 ([#389](https://github.com/mossipcams/ajax-cli/issues/389)) ([f29dd7d](https://github.com/mossipcams/ajax-cli/commit/f29dd7d041a1bc85fccb4190a730271191ec5b75))

## [0.35.15](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.14...ajax-cli-v0.35.15) (2026-07-08)


### Bug Fixes

* **web:** keep mobile task-view scroll inside the terminal and remove dead Wide hotkey ([#387](https://github.com/mossipcams/ajax-cli/issues/387)) ([cf9a2d7](https://github.com/mossipcams/ajax-cli/commit/cf9a2d7b32e0f23a5055f9c9be64b092cbcd69a1))

## [0.35.14](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.13...ajax-cli-v0.35.14) (2026-07-08)


### Bug Fixes

* **web:** refit expanded terminal with keyboard open ([#385](https://github.com/mossipcams/ajax-cli/issues/385)) ([fda4cd2](https://github.com/mossipcams/ajax-cli/commit/fda4cd2bbb673a2590ac13b22ea564f511ac69d0))

## [0.35.13](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.12...ajax-cli-v0.35.13) (2026-07-08)


### Bug Fixes

* **web:** repair mobile cockpit regressions ([#383](https://github.com/mossipcams/ajax-cli/issues/383)) ([ec37399](https://github.com/mossipcams/ajax-cli/commit/ec373998c4a1fb47ec3b62bf3e977c290f8e9db9))

## [0.35.12](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.11...ajax-cli-v0.35.12) (2026-07-08)


### Bug Fixes

* **web:** correct terminal fullscreen chrome peek-through and zero-lag echo sizing ([#381](https://github.com/mossipcams/ajax-cli/issues/381)) ([800abcb](https://github.com/mossipcams/ajax-cli/commit/800abcbcc4a7c8034c5c5b9b14bf07d6bd43765c))

## [0.35.11](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.10...ajax-cli-v0.35.11) (2026-07-08)


### Bug Fixes

* **web:** cap viewport zoom to stop iOS fullscreen focus-zoom ([#379](https://github.com/mossipcams/ajax-cli/issues/379)) ([ab18a90](https://github.com/mossipcams/ajax-cli/commit/ab18a90b877f58bbc3154295122d040aa05ef0d1))

## [0.35.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.9...ajax-cli-v0.35.10) (2026-07-08)


### Bug Fixes

* **web:** re-fit terminal after fullscreen viewport settles ([#375](https://github.com/mossipcams/ajax-cli/issues/375)) ([004c74f](https://github.com/mossipcams/ajax-cli/commit/004c74fc2390e0309f3056f92dc618aff4561337))

## [0.35.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.8...ajax-cli-v0.35.9) (2026-07-08)


### Bug Fixes

* **web:** harden iOS terminal — pinch deadzone + fullscreen button safe-area ([#373](https://github.com/mossipcams/ajax-cli/issues/373)) ([79200d8](https://github.com/mossipcams/ajax-cli/commit/79200d8e1d9e56d35bfcc4c248964b025f6e7e07))

## [0.35.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.7...ajax-cli-v0.35.8) (2026-07-07)


### Bug Fixes

* **web:** resume task on open and surface dead terminal sessions ([#370](https://github.com/mossipcams/ajax-cli/issues/370)) ([bb8fda1](https://github.com/mossipcams/ajax-cli/commit/bb8fda10e3f0b9204a51e69ad9f672ffa985ea17))

## [0.35.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.6...ajax-cli-v0.35.7) (2026-07-07)


### Bug Fixes

* **web:** keep terminal scrollback scrollable and add OpenCode agent option ([#368](https://github.com/mossipcams/ajax-cli/issues/368)) ([6ce5d59](https://github.com/mossipcams/ajax-cli/commit/6ce5d596148ac73ee11f7b0818f54c6eb0db2481))

## [0.35.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.5...ajax-cli-v0.35.6) (2026-07-07)


### Code Refactoring

* **web:** refactor web cockpit viewport ownership ([6b311db](https://github.com/mossipcams/ajax-cli/commit/6b311db58a00c949124cc85425eece5298fba561))

## [0.35.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.4...ajax-cli-v0.35.5) (2026-07-07)


### Bug Fixes

* **core:** make terminal statuses track live pane reality ([#363](https://github.com/mossipcams/ajax-cli/issues/363)) ([32c4d36](https://github.com/mossipcams/ajax-cli/commit/32c4d3677ffb2b671978b6145578b1e54bc0ed2c))
* **web:** pin new-task sheet to the visual viewport band ([#362](https://github.com/mossipcams/ajax-cli/issues/362)) ([47c2eaa](https://github.com/mossipcams/ajax-cli/commit/47c2eaaef91f983850ac5d7de647ae541a9f06be))

## [0.35.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.3...ajax-cli-v0.35.4) (2026-07-07)


### Bug Fixes

* **web:** polish mobile terminal gestures ([#360](https://github.com/mossipcams/ajax-cli/issues/360)) ([704e23d](https://github.com/mossipcams/ajax-cli/commit/704e23d885fb587663c712b92e7849e35c1c59dd))

## [0.35.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.2...ajax-cli-v0.35.3) (2026-07-07)


### Bug Fixes

* **web:** remove terminal shell edge padding ([#358](https://github.com/mossipcams/ajax-cli/issues/358)) ([0d5b18c](https://github.com/mossipcams/ajax-cli/commit/0d5b18cd7dad09fd56f2bb6fd91610ae0d11b03f))

## [0.35.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.1...ajax-cli-v0.35.2) (2026-07-06)


### Reverts

* **web:** roll back rcarmo ghostty-web migration ([#356](https://github.com/mossipcams/ajax-cli/issues/356)) ([1143bee](https://github.com/mossipcams/ajax-cli/commit/1143bee42a8fc935bc683333b7fd65e5149c8e57))

## [0.35.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.35.0...ajax-cli-v0.35.1) (2026-07-06)


### Bug Fixes

* **web:** repair ghostty terminal shell contracts ([#354](https://github.com/mossipcams/ajax-cli/issues/354)) ([3991429](https://github.com/mossipcams/ajax-cli/commit/3991429192b50421694ccc54155b3231e3c668c8))

## [0.35.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.5...ajax-cli-v0.35.0) (2026-07-06)


### Features

* **web:** migrate to rcarmo/ghostty-web and edge-to-edge fullscreen terminal ([#353](https://github.com/mossipcams/ajax-cli/issues/353)) ([f54ec11](https://github.com/mossipcams/ajax-cli/commit/f54ec11af3349d249a87a005107c20337c405474))


### Reverts

* **web:** terminal width takeover changes ([#351](https://github.com/mossipcams/ajax-cli/issues/351)) ([4d93dd8](https://github.com/mossipcams/ajax-cli/commit/4d93dd8e5d642f9b26e053beb59c5cbb7e74a394))

## [0.34.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.4...ajax-cli-v0.34.5) (2026-07-06)


### Bug Fixes

* **web:** size terminal takeover to visual viewport ([#349](https://github.com/mossipcams/ajax-cli/issues/349)) ([22d09e6](https://github.com/mossipcams/ajax-cli/commit/22d09e68cfefb568d074134081610bf29e1c8d4b))

## [0.34.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.3...ajax-cli-v0.34.4) (2026-07-06)


### Bug Fixes

* **web:** widen terminal shell and remove fit gutters ([#347](https://github.com/mossipcams/ajax-cli/issues/347)) ([621355b](https://github.com/mossipcams/ajax-cli/commit/621355b2c7971ae9220961af20314df4c88b57d8))

## [0.34.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.2...ajax-cli-v0.34.3) (2026-07-06)


### Bug Fixes

* **web:** stabilize mobile terminal fullscreen gestures ([#345](https://github.com/mossipcams/ajax-cli/issues/345)) ([59a82a2](https://github.com/mossipcams/ajax-cli/commit/59a82a267287347c4d5a91b239b8bd0158d19ed6))

## [0.34.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.1...ajax-cli-v0.34.2) (2026-07-06)


### Bug Fixes

* **web:** pinch rewrap with keyboard open, kill page zoom at touchdown ([#343](https://github.com/mossipcams/ajax-cli/issues/343)) ([5a66477](https://github.com/mossipcams/ajax-cli/commit/5a66477ea95e6c91bc2f3b42c165abcd5b5156ea))

## [0.34.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.34.0...ajax-cli-v0.34.1) (2026-07-06)


### Bug Fixes

* **web:** flush pinch rewrap, block page zoom, center terminal ([#341](https://github.com/mossipcams/ajax-cli/issues/341)) ([97ff8d6](https://github.com/mossipcams/ajax-cli/commit/97ff8d66683331bac55fa147ec276ce6059887f8))

## [0.34.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.33.2...ajax-cli-v0.34.0) (2026-07-06)


### Features

* **web:** fit terminal geometry to viewport width ([#339](https://github.com/mossipcams/ajax-cli/issues/339)) ([2ac0340](https://github.com/mossipcams/ajax-cli/commit/2ac034011cbfcf3fdee283f5dfb97f8fa6e23e63))

## [0.33.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.33.1...ajax-cli-v0.33.2) (2026-07-06)


### Bug Fixes

* **web:** contain iOS PWA terminal width ([#336](https://github.com/mossipcams/ajax-cli/issues/336)) ([8b86482](https://github.com/mossipcams/ajax-cli/commit/8b8648209d81d68e78ff0fa0eb22fb30e42c3ff9))

## [0.33.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.33.0...ajax-cli-v0.33.1) (2026-07-06)


### Bug Fixes

* **web:** keep mobile terminal within viewport ([#334](https://github.com/mossipcams/ajax-cli/issues/334)) ([5cdb812](https://github.com/mossipcams/ajax-cli/commit/5cdb812314900e8a843da79ddcb063c982374ffa))

## [0.33.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.11...ajax-cli-v0.33.0) (2026-07-06)


### Features

* **web:** fit terminal text to the viewport width ([#331](https://github.com/mossipcams/ajax-cli/issues/331)) ([54600e6](https://github.com/mossipcams/ajax-cli/commit/54600e6122b2b0eff1a02366ae369a7b7fac364b))


### Bug Fixes

* **web:** allow terminal task detail route ([#330](https://github.com/mossipcams/ajax-cli/issues/330)) ([93ba6d1](https://github.com/mossipcams/ajax-cli/commit/93ba6d151e47e5dbbf2f3e6f649c460d90090112))

## [0.32.11](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.10...ajax-cli-v0.32.11) (2026-07-04)


### Bug Fixes

* **web:** refit terminal after pinch layout settles ([#328](https://github.com/mossipcams/ajax-cli/issues/328)) ([5e992d8](https://github.com/mossipcams/ajax-cli/commit/5e992d8c833113a5a868a309827268c11ded4575))

## [0.32.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.9...ajax-cli-v0.32.10) (2026-07-04)


### Bug Fixes

* **web:** clamp terminal pan after refit ([#326](https://github.com/mossipcams/ajax-cli/issues/326)) ([226d468](https://github.com/mossipcams/ajax-cli/commit/226d46854e995d78bb2d739306c6f5c877bfc123))

## [0.32.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.8...ajax-cli-v0.32.9) (2026-07-04)


### Bug Fixes

* **web:** stabilize mobile terminal viewport ([#324](https://github.com/mossipcams/ajax-cli/issues/324)) ([7a7d1f1](https://github.com/mossipcams/ajax-cli/commit/7a7d1f1a2fd29ad002ef570a084e80f677e3dcfc))

## [0.32.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.7...ajax-cli-v0.32.8) (2026-07-03)


### Bug Fixes

* **web:** echo mobile terminal input earlier ([#322](https://github.com/mossipcams/ajax-cli/issues/322)) ([aba3185](https://github.com/mossipcams/ajax-cli/commit/aba318550face2efa40e246a0f7a1d214687431c))

## [0.32.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.6...ajax-cli-v0.32.7) (2026-07-03)


### Bug Fixes

* **web:** reduce iOS terminal input lag ([#320](https://github.com/mossipcams/ajax-cli/issues/320)) ([5be4da1](https://github.com/mossipcams/ajax-cli/commit/5be4da1a7b20768adb9cd76fd28bc227db98eca0))

## [0.32.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.5...ajax-cli-v0.32.6) (2026-07-03)


### Bug Fixes

* **web:** set terminal type for tmux attach ([#318](https://github.com/mossipcams/ajax-cli/issues/318)) ([b84bfe9](https://github.com/mossipcams/ajax-cli/commit/b84bfe991ebb49f870fb3740acc9dc51a55fa992))

## [0.32.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.4...ajax-cli-v0.32.5) (2026-07-03)


### Bug Fixes

* merge compatible context task facts ([#316](https://github.com/mossipcams/ajax-cli/issues/316)) ([bfb4ba3](https://github.com/mossipcams/ajax-cli/commit/bfb4ba36727a147c363918e13080c911535b749c))

## [0.32.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.3...ajax-cli-v0.32.4) (2026-07-03)


### Bug Fixes

* **web:** keep ghostty fullscreen scroll interactive ([#313](https://github.com/mossipcams/ajax-cli/issues/313)) ([62a05f0](https://github.com/mossipcams/ajax-cli/commit/62a05f00bca6e100ebd374d7a4f94ccc86b78215))

## [0.32.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.2...ajax-cli-v0.32.3) (2026-07-03)


### Bug Fixes

* **web:** refine pwa terminal fullscreen ([#311](https://github.com/mossipcams/ajax-cli/issues/311)) ([dac821a](https://github.com/mossipcams/ajax-cli/commit/dac821a3f5db844ce08e28bc540adb35283f08c5))

## [0.32.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.1...ajax-cli-v0.32.2) (2026-07-03)


### Bug Fixes

* **web:** refine pwa terminal fullscreen ([#309](https://github.com/mossipcams/ajax-cli/issues/309)) ([1797f6c](https://github.com/mossipcams/ajax-cli/commit/1797f6cb12ad24fe8a874b8a313f9f1cc20e21e4))

## [0.32.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.32.0...ajax-cli-v0.32.1) (2026-07-03)


### Bug Fixes

* **web:** improve terminal fullscreen tapping ([#307](https://github.com/mossipcams/ajax-cli/issues/307)) ([0777c37](https://github.com/mossipcams/ajax-cli/commit/0777c3788146c0edfeeeea7bee9d8028d6fc426f))

## [0.32.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.31.0...ajax-cli-v0.32.0) (2026-07-03)


### Features

* release task window migration cleanup ([#305](https://github.com/mossipcams/ajax-cli/issues/305)) ([159536d](https://github.com/mossipcams/ajax-cli/commit/159536d334c569097fba0490fa1f6a019f100343))

## [0.31.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.30.3...ajax-cli-v0.31.0) (2026-07-03)


### Features

* introduce Ajax task window substrate ([#303](https://github.com/mossipcams/ajax-cli/issues/303)) ([dd65f37](https://github.com/mossipcams/ajax-cli/commit/dd65f374fe20d5a7bd7d30c903181af9bc00678c))


### Bug Fixes

* **web:** compact dashboard task rows for mobile density ([#301](https://github.com/mossipcams/ajax-cli/issues/301)) ([bb05157](https://github.com/mossipcams/ajax-cli/commit/bb05157d6e659fe8b6174c57faf8664e913b64e9))
* **web:** remove redundant detail resume action ([#300](https://github.com/mossipcams/ajax-cli/issues/300)) ([c2984b5](https://github.com/mossipcams/ajax-cli/commit/c2984b5cd9e151043a20bef00e620e525eaaf306))

## [0.30.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.30.2...ajax-cli-v0.30.3) (2026-07-03)


### Bug Fixes

* **web:** remove redundant task open controls ([#298](https://github.com/mossipcams/ajax-cli/issues/298)) ([8ae673d](https://github.com/mossipcams/ajax-cli/commit/8ae673d19ab0e082f44a11ff16ca3bdcaca97cc3))

## [0.30.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.30.1...ajax-cli-v0.30.2) (2026-07-03)


### Bug Fixes

* **web:** remove server-issued confirmation-token gate for destructive actions ([#296](https://github.com/mossipcams/ajax-cli/issues/296)) ([b80eacb](https://github.com/mossipcams/ajax-cli/commit/b80eacb882809ff35512595973a5f50f66f69466))

## [0.30.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.30.0...ajax-cli-v0.30.1) (2026-07-02)


### Bug Fixes

* **web:** terminal full-screen, keyboard auto-scroll, arrow-key jump, task page redesign ([#291](https://github.com/mossipcams/ajax-cli/issues/291)) ([0519939](https://github.com/mossipcams/ajax-cli/commit/05199394b39964338fcd84738417c0ed07c2f544))

## [0.30.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.29.4...ajax-cli-v0.30.0) (2026-07-02)


### Features

* migrate web terminal to ghostty ([#289](https://github.com/mossipcams/ajax-cli/issues/289)) ([84198ae](https://github.com/mossipcams/ajax-cli/commit/84198ae560e0cf182fe1a1c8fe8cb7b50d600c96))

## [0.29.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.29.3...ajax-cli-v0.29.4) (2026-07-02)


### Bug Fixes

* **web:** keep composer text when the terminal socket is not open ([#288](https://github.com/mossipcams/ajax-cli/issues/288)) ([d447e67](https://github.com/mossipcams/ajax-cli/commit/d447e67026bd30939563e74cbaa1ded3e5e3f8b4))
* **web:** terminal keyboard alignment, behavior fixes, module extraction, and [#284](https://github.com/mossipcams/ajax-cli/issues/284) shell CSS conflict fix ([#286](https://github.com/mossipcams/ajax-cli/issues/286)) ([944847f](https://github.com/mossipcams/ajax-cli/commit/944847fca52a0f24d6687000dc956bb3daed4f76))

## [0.29.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.29.2...ajax-cli-v0.29.3) (2026-07-02)


### Bug Fixes

* add mobile terminal app shell ([#284](https://github.com/mossipcams/ajax-cli/issues/284)) ([6d32ce8](https://github.com/mossipcams/ajax-cli/commit/6d32ce8cf32a799ca602d0c206c217fcd0c03d4c))

## [0.29.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.29.1...ajax-cli-v0.29.2) (2026-07-02)


### Code Refactoring

* **web:** consolidate patch-layered terminal code behind behavior tests ([#282](https://github.com/mossipcams/ajax-cli/issues/282)) ([d4e6428](https://github.com/mossipcams/ajax-cli/commit/d4e64283085cbd3dd6402d6a0ccfd776db6462c8))

## [0.29.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.29.0...ajax-cli-v0.29.1) (2026-07-01)


### Bug Fixes

* **web:** mobile terminal polish — bigger text, no DOM scrollbar, taller terminal, smooth expand ([#280](https://github.com/mossipcams/ajax-cli/issues/280)) ([53337f9](https://github.com/mossipcams/ajax-cli/commit/53337f9cecc8d14b08828856cd32a25f718c29fb))

## [0.29.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.6...ajax-cli-v0.29.0) (2026-07-01)


### Features

* **web:** 80-col terminal floor, pan/pinch/fling, and keyboard input-line fix ([#278](https://github.com/mossipcams/ajax-cli/issues/278)) ([8dd1ef5](https://github.com/mossipcams/ajax-cli/commit/8dd1ef575782ce7b2a504aca14a6fcd5c5e080ae))

## [0.28.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.5...ajax-cli-v0.28.6) (2026-07-01)


### Bug Fixes

* **web:** strip scrollback-hostile PTY sequences before browser output ([#276](https://github.com/mossipcams/ajax-cli/issues/276)) ([d80779f](https://github.com/mossipcams/ajax-cli/commit/d80779faa67bd3432dbff6101b34a97d8fd812fe))

## [0.28.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.4...ajax-cli-v0.28.5) (2026-07-01)


### Bug Fixes

* merge compatible context save task facts ([#272](https://github.com/mossipcams/ajax-cli/issues/272)) ([a0373e0](https://github.com/mossipcams/ajax-cli/commit/a0373e08fdb6137a1016432a92ef9517f7e31081))
* **web:** make iOS Safari terminal actually scroll on touch ([#275](https://github.com/mossipcams/ajax-cli/issues/275)) ([07bdc73](https://github.com/mossipcams/ajax-cli/commit/07bdc7392275fd793c2c68c3b70fa3084abaef0f))
* **web:** shrink mobile terminal font to 10px for usable column fit ([#273](https://github.com/mossipcams/ajax-cli/issues/273)) ([19e2fd5](https://github.com/mossipcams/ajax-cli/commit/19e2fd56e2b18d2bde9e62e451e12f4c48c024f7))

## [0.28.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.3...ajax-cli-v0.28.4) (2026-07-01)


### Bug Fixes

* **web:** iOS Safari terminal scroll corruption + compact sizing ([#270](https://github.com/mossipcams/ajax-cli/issues/270)) ([bcec33f](https://github.com/mossipcams/ajax-cli/commit/bcec33f3d95e9c6a9cce9f65b21661d2e652b34c))

## [0.28.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.2...ajax-cli-v0.28.3) (2026-07-01)


### Bug Fixes

* **web:** compact iOS Safari terminal sizing and touch scroll tests ([#268](https://github.com/mossipcams/ajax-cli/issues/268)) ([83b185c](https://github.com/mossipcams/ajax-cli/commit/83b185c6108f6ba17785ee08e4ca5ec92c2ed605))

## [0.28.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.1...ajax-cli-v0.28.2) (2026-07-01)


### Bug Fixes

* bypass CI for release-please PRs ([#266](https://github.com/mossipcams/ajax-cli/issues/266)) ([e9b16a9](https://github.com/mossipcams/ajax-cli/commit/e9b16a959d85f404c492c7a953efc75b11a209a0))

## [0.28.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.28.0...ajax-cli-v0.28.1) (2026-07-01)


### Bug Fixes

* **web:** raw-first task terminal on mobile and desktop ([#264](https://github.com/mossipcams/ajax-cli/issues/264)) ([6f4bdb7](https://github.com/mossipcams/ajax-cli/commit/6f4bdb742b009665c8c5ff7c4aed71c85e5ba762))

## [0.28.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.6...ajax-cli-v0.28.0) (2026-07-01)


### Features

* **web:** refactor mobile terminal experience ([#263](https://github.com/mossipcams/ajax-cli/issues/263)) ([08bba40](https://github.com/mossipcams/ajax-cli/commit/08bba4066c2a7521e26e20b3a7f8c2db1f3e987d))


### Bug Fixes

* intercept terminal scroll gestures ([#261](https://github.com/mossipcams/ajax-cli/issues/261)) ([a5d4542](https://github.com/mossipcams/ajax-cli/commit/a5d45428516b88dc2c9d695b7780a7a71898d26e))

## [0.27.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.5...ajax-cli-v0.27.6) (2026-07-01)


### Bug Fixes

* improve mobile terminal scrolling ([#259](https://github.com/mossipcams/ajax-cli/issues/259)) ([c9e2393](https://github.com/mossipcams/ajax-cli/commit/c9e2393e8492d6c57a74823939dd4e0db461b2cf))

## [0.27.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.4...ajax-cli-v0.27.5) (2026-07-01)


### Bug Fixes

* **web:** shrink the terminal font to 6px ([#257](https://github.com/mossipcams/ajax-cli/issues/257)) ([7d08879](https://github.com/mossipcams/ajax-cli/commit/7d0887997e0c64d3f8542706aa19d48c303d239e))

## [0.27.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.3...ajax-cli-v0.27.4) (2026-07-01)


### Bug Fixes

* **web:** shrink the terminal font to 10px for more rows and columns ([#255](https://github.com/mossipcams/ajax-cli/issues/255)) ([15eb4aa](https://github.com/mossipcams/ajax-cli/commit/15eb4aae8513e442233d415e819d3a5f990d2cb7))

## [0.27.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.2...ajax-cli-v0.27.3) (2026-07-01)


### Bug Fixes

* **web:** keep the mobile terminal usable when the iOS keyboard is open ([#253](https://github.com/mossipcams/ajax-cli/issues/253)) ([9d9ddc7](https://github.com/mossipcams/ajax-cli/commit/9d9ddc76b742c1fcbdf41c38596f41f50a51b841))

## [0.27.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.1...ajax-cli-v0.27.2) (2026-07-01)


### Bug Fixes

* **web:** make the mobile terminal scrollable via touch drag ([#251](https://github.com/mossipcams/ajax-cli/issues/251)) ([db070a0](https://github.com/mossipcams/ajax-cli/commit/db070a03ebcc8aece8eb6ad02632af5c02bfb779))

## [0.27.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.27.0...ajax-cli-v0.27.1) (2026-06-30)


### Bug Fixes

* **web:** stop forced auto-scroll from blocking terminal scrollback ([#249](https://github.com/mossipcams/ajax-cli/issues/249)) ([58e6e9c](https://github.com/mossipcams/ajax-cli/commit/58e6e9c53051f81be6c6b258de7fa4ee40b808ca))

## [0.27.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.26.2...ajax-cli-v0.27.0) (2026-06-30)


### Features

* **web:** full-screen mobile terminal with keyboard-aware viewport ([#247](https://github.com/mossipcams/ajax-cli/issues/247)) ([9a26cb8](https://github.com/mossipcams/ajax-cli/commit/9a26cb88601f7ec2fc7cdb557ec7fea24bbdf9c6))

## [0.26.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.26.1...ajax-cli-v0.26.2) (2026-06-30)


### Bug Fixes

* **web:** lock document scroll and shrink chrome for full-screen mobile terminal ([#245](https://github.com/mossipcams/ajax-cli/issues/245)) ([e39d550](https://github.com/mossipcams/ajax-cli/commit/e39d5504cae74687df54c0c134cebc9c42d47779))

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
