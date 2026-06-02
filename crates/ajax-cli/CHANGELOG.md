# Changelog

## Unreleased

### Bug Fixes

* unify ghost-task classification so recoverable missing-substrate tasks survive save/load with events, receipts, and Cockpit visibility

## [0.11.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.11.0...ajax-cli-v0.11.1) (2026-06-01)


### Bug Fixes

* **release:** collapse workspace to one releasable path ([#114](https://github.com/mossipcams/ajax-cli/issues/114)) ([2d09612](https://github.com/mossipcams/ajax-cli/commit/2d09612811bb07aba1206dd8579008a6a8400324))
* **release:** keep one shared workspace release line ([#113](https://github.com/mossipcams/ajax-cli/issues/113)) ([2e49262](https://github.com/mossipcams/ajax-cli/commit/2e492625bf5b9837f1b5fae1b162180a1ae04456))
* **web:** make cockpit dashboard-first and split releases per crate ([#111](https://github.com/mossipcams/ajax-cli/issues/111)) ([89906d9](https://github.com/mossipcams/ajax-cli/commit/89906d92243600694b26cd241d677006070ed7f8))

## [0.11.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.10.1...ajax-cli-v0.11.0) (2026-05-29)


### Features

* **web:** triage-only structured agent answering with guarded approvals ([#109](https://github.com/mossipcams/ajax-cli/issues/109)) ([b6cbf0a](https://github.com/mossipcams/ajax-cli/commit/b6cbf0a90ffab071e2512265e3b5241fdbc8f295))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * ajax-core bumped from 0.10.1 to 0.11.0
    * ajax-supervisor bumped from 0.10.1 to 0.11.0
    * ajax-tui bumped from 0.10.1 to 0.11.0
    * ajax-web bumped from 0.10.1 to 0.11.0

## [0.10.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.10.0...ajax-cli-v0.10.1) (2026-05-28)


### Bug Fixes

* stop release please phantom sync release loop ([#107](https://github.com/mossipcams/ajax-cli/issues/107)) ([f5174a3](https://github.com/mossipcams/ajax-cli/commit/f5174a30d629fd6cf3134fa98c76d11633ce1992))
* sync release please manifest paths on grouped releases ([#108](https://github.com/mossipcams/ajax-cli/issues/108)) ([d582aa2](https://github.com/mossipcams/ajax-cli/commit/d582aa24f264a941dc55116d9745978f29d62321))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * ajax-core bumped from 0.10.0 to 0.10.1
    * ajax-supervisor bumped from 0.10.0 to 0.10.1
    * ajax-tui bumped from 0.10.0 to 0.10.1
    * ajax-web bumped from 0.10.0 to 0.10.1

## [0.10.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.9.0...ajax-cli-v0.10.0) (2026-05-28)


### Miscellaneous Chores

* **ajax-cli:** Synchronize ajax-cli versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * ajax-core bumped from 0.9.0 to 0.10.0
    * ajax-supervisor bumped from 0.9.0 to 0.10.0
    * ajax-tui bumped from 0.9.0 to 0.10.0
    * ajax-web bumped from 0.9.0 to 0.10.0

## [0.9.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.8.0...ajax-cli-v0.9.0) (2026-05-27)


### Miscellaneous Chores

* **ajax-cli:** Synchronize ajax-cli versions


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * ajax-core bumped from 0.8.0 to 0.9.0
    * ajax-supervisor bumped from 0.8.0 to 0.9.0
    * ajax-tui bumped from 0.8.0 to 0.9.0
    * ajax-web bumped from 0.8.0 to 0.9.0

## [0.8.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.7.4...ajax-cli-v0.8.0) (2026-05-27)


### Bug Fixes

* register workspace crates in release please ([#98](https://github.com/mossipcams/ajax-cli/issues/98)) ([b4f083e](https://github.com/mossipcams/ajax-cli/commit/b4f083e0718dcf8c131e1f7d8ba64aec61574af6))
* stabilize release please workspace version rewrites ([#102](https://github.com/mossipcams/ajax-cli/issues/102)) ([160eafa](https://github.com/mossipcams/ajax-cli/commit/160eaface52cc633156fb3e9d2613f4359f61879))
* unify ghost-task classification across persistence and Cockpit ([#99](https://github.com/mossipcams/ajax-cli/issues/99)) ([142c2fd](https://github.com/mossipcams/ajax-cli/commit/142c2fdb5dc8a9c30c5b73cc7c58049bebbd8c7a))


### Dependencies

* The following workspace dependencies were updated
  * dependencies
    * ajax-core bumped from 0.1.0 to 0.8.0
    * ajax-supervisor bumped from 0.1.0 to 0.8.0
    * ajax-tui bumped from 0.1.0 to 0.8.0
    * ajax-web bumped from 0.1.0 to 0.8.0

## [0.7.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.7.3...ajax-cli-v0.7.4) (2026-05-27)


### Bug Fixes

* **web:** keep PWA top banners inside iOS safe area ([#95](https://github.com/mossipcams/ajax-cli/issues/95)) ([f60561f](https://github.com/mossipcams/ajax-cli/commit/f60561f4e9d6b485533c32ea288a44d2c551872b))
* **web:** stabilize PWA drop confirm and modernize action buttons ([#94](https://github.com/mossipcams/ajax-cli/issues/94)) ([20a38a3](https://github.com/mossipcams/ajax-cli/commit/20a38a3f6a537f2dbc22c383e3c52a39a04bc1ef))

## [0.7.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.7.2...ajax-cli-v0.7.3) (2026-05-27)


### Bug Fixes

* resilient drop teardown, failure surfacing, and ghost task pruning ([#91](https://github.com/mossipcams/ajax-cli/issues/91)) ([cb29bcc](https://github.com/mossipcams/ajax-cli/commit/cb29bcc13312e122801ef9d1c8bf709e4b60af67))

## [0.7.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.7.1...ajax-cli-v0.7.2) (2026-05-27)


### Bug Fixes

* stop persisting cockpit-hidden missing-substrate task ghosts ([#88](https://github.com/mossipcams/ajax-cli/issues/88)) ([3b8a81e](https://github.com/mossipcams/ajax-cli/commit/3b8a81e4484b2510570f4842949c1a233097fb6a))

## [0.7.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.7.0...ajax-cli-v0.7.1) (2026-05-27)


### Bug Fixes

* surface missing-substrate cockpit tasks ([#83](https://github.com/mossipcams/ajax-cli/issues/83)) ([d649fa1](https://github.com/mossipcams/ajax-cli/commit/d649fa173cfcd091a73af291f66870e110289aad))

## [0.7.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.6.0...ajax-cli-v0.7.0) (2026-05-27)


### Features

* surface merge conflicts and CI failures with skill remediation actions ([#82](https://github.com/mossipcams/ajax-cli/issues/82)) ([3dfa937](https://github.com/mossipcams/ajax-cli/commit/3dfa9377ad1f258dd7588215c8dd573da7081486))


### Bug Fixes

* restore runtime refresh orphan and git recovery ([#80](https://github.com/mossipcams/ajax-cli/issues/80)) ([8594e7f](https://github.com/mossipcams/ajax-cli/commit/8594e7f7acbf6bc8a93d751fae46b3ca93e313fe))

## [0.6.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.5.0...ajax-cli-v0.6.0) (2026-05-27)


### Features

* complete mobile cockpit operator parity and fix alert spam ([#77](https://github.com/mossipcams/ajax-cli/issues/77)) ([099bf83](https://github.com/mossipcams/ajax-cli/commit/099bf83b9d2677e6ddcae17b5a3a35a4ce848e9f))


### Performance Improvements

* optimize runtime refresh and improve reliability ([#79](https://github.com/mossipcams/ajax-cli/issues/79)) ([474d8d3](https://github.com/mossipcams/ajax-cli/commit/474d8d3a1f4c816771845bb16bb12f034583cdfd))

## [0.5.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.4.0...ajax-cli-v0.5.0) (2026-05-27)


### Features

* expose visible PWA alerts opt-in with environment guidance ([#74](https://github.com/mossipcams/ajax-cli/issues/74)) ([3a68b52](https://github.com/mossipcams/ajax-cli/commit/3a68b52c97d04f847ef4a37ad0a675741b2a542a))


### Bug Fixes

* align Cursor supervisor events with task lifecycles ([#72](https://github.com/mossipcams/ajax-cli/issues/72)) ([51a6e0e](https://github.com/mossipcams/ajax-cli/commit/51a6e0ea9376a2a34ed57a72ef617263259bc4c4))

## [0.4.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.3.0...ajax-cli-v0.4.0) (2026-05-27)


### Features

* add Cursor agent support to supervisor for notifications ([#68](https://github.com/mossipcams/ajax-cli/issues/68)) ([9cdcf72](https://github.com/mossipcams/ajax-cli/commit/9cdcf729ff701e80ec6714cd16ea7fe1c1930ef7))


### Bug Fixes

* stop web cockpit from reintroducing missing-substrate ghost tasks ([#69](https://github.com/mossipcams/ajax-cli/issues/69)) ([8cabe9f](https://github.com/mossipcams/ajax-cli/commit/8cabe9f3220323b7121f9e923b9dad60307a7e8e))

## [0.3.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.2.2...ajax-cli-v0.3.0) (2026-05-26)


### Features

* serve web cockpit through axum runtime ([#62](https://github.com/mossipcams/ajax-cli/issues/62)) ([2c3347f](https://github.com/mossipcams/ajax-cli/commit/2c3347fe72767ab315d8c04b04fd9834cbec912e))


### Bug Fixes

* hard-delete dropped tasks ([#64](https://github.com/mossipcams/ajax-cli/issues/64)) ([bd70439](https://github.com/mossipcams/ajax-cli/commit/bd70439a0b09a85e6ecd5c1359cc4fd6713b3a4f))

## [0.2.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.2.1...ajax-cli-v0.2.2) (2026-05-26)


### Bug Fixes

* web cockpit reliability, zoom, registry view, drop pairing token ([#59](https://github.com/mossipcams/ajax-cli/issues/59)) ([233f8bb](https://github.com/mossipcams/ajax-cli/commit/233f8bba5c85d558ff101c473c72a4e37df5ed3a))

## [0.2.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.2.0...ajax-cli-v0.2.1) (2026-05-25)


### Bug Fixes

* require host-native pwa control backend ([#58](https://github.com/mossipcams/ajax-cli/issues/58)) ([29abd4d](https://github.com/mossipcams/ajax-cli/commit/29abd4dd7ea9bd3b2a5f5e48c540264d74de107e))

## [0.2.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.1.0...ajax-cli-v0.2.0) (2026-05-25)


### Features

* add ajax web pwa slice crate ([#42](https://github.com/mossipcams/ajax-cli/issues/42)) ([a54d202](https://github.com/mossipcams/ajax-cli/commit/a54d2025b4f4ecca1f3f41bf03fc4bb2017a627b))
* add launchd web server runtime ([#50](https://github.com/mossipcams/ajax-cli/issues/50)) ([52ebd53](https://github.com/mossipcams/ajax-cli/commit/52ebd53291db5295d420b50a2c383aab595f4565))
* add stable and dev mobile cockpit ([#34](https://github.com/mossipcams/ajax-cli/issues/34)) ([dcde47d](https://github.com/mossipcams/ajax-cli/commit/dcde47d0bb32368c520d32ec0c0bcd8b9032a177))
* mobile-first PWA web companion ([#38](https://github.com/mossipcams/ajax-cli/issues/38)) ([98b789d](https://github.com/mossipcams/ajax-cli/commit/98b789d206547a091a3a93244b3347c1a63d50f0))
* mobile-friendly cockpit polish ([#43](https://github.com/mossipcams/ajax-cli/issues/43)) ([e6408f9](https://github.com/mossipcams/ajax-cli/commit/e6408f9ed41f44a0ca02a8cf110137d36b9c4e8b))
* pwa cockpit redesign + ajax-web docker runtime ([#54](https://github.com/mossipcams/ajax-cli/issues/54)) ([3ff0c55](https://github.com/mossipcams/ajax-cli/commit/3ff0c55c2699716449a5f1e9d4f3168b3c8b1b21))
* terminal-blueprint cockpit aesthetic ([#45](https://github.com/mossipcams/ajax-cli/issues/45)) ([49f505a](https://github.com/mossipcams/ajax-cli/commit/49f505a405662f003519c922ee56bb5f460eedf4))


### Bug Fixes

* clean cockpit task session return ([af2fe8b](https://github.com/mossipcams/ajax-cli/commit/af2fe8baa1345f7f5d0fe152cb7083f84f0ed20a))
* clear codex working prompt attention ([072a4c0](https://github.com/mossipcams/ajax-cli/commit/072a4c07c2e6737a06220c6826b12d223bd7a0b3))
* clear tmux copy mode before task entry ([fe3d00d](https://github.com/mossipcams/ajax-cli/commit/fe3d00dbf91a8c6c917392d57414aa037b38bb79))
* drop git resources before tmux ([8a51076](https://github.com/mossipcams/ajax-cli/commit/8a5107617952da63343101b55c44857dd76e2a2d))
* force drop with stale worktree evidence ([be8f123](https://github.com/mossipcams/ajax-cli/commit/be8f123a848e6bc1fb99d458b7f76c4b3f5c3c2f))
* handle missing branch during drop ([#9](https://github.com/mossipcams/ajax-cli/issues/9)) ([f30e330](https://github.com/mossipcams/ajax-cli/commit/f30e330221374cd5652db0464142c0d5a9a6f682))
* honor selected runtime paths in writer entrypoint ([#49](https://github.com/mossipcams/ajax-cli/issues/49)) ([b2d7223](https://github.com/mossipcams/ajax-cli/commit/b2d7223fa02a6b8905c428742a34fd4aa68d7c42))
* ignore iOS flow-control resume byte ([54ccff0](https://github.com/mossipcams/ajax-cli/commit/54ccff0aa3c3d54b59296fab0f39790a5c9afc0b))
* install husky in task worktrees ([be66aa4](https://github.com/mossipcams/ajax-cli/commit/be66aa4b074e02c9c6d5b60a8bb0f045e25561ab))
* isolate ajax runtime profiles ([#36](https://github.com/mossipcams/ajax-cli/issues/36)) ([225f69f](https://github.com/mossipcams/ajax-cli/commit/225f69f764577ba7f097c47466ed4954dc81f9f1))
* isolate dev and stable state stores ([#35](https://github.com/mossipcams/ajax-cli/issues/35)) ([e20f9aa](https://github.com/mossipcams/ajax-cli/commit/e20f9aaa95c099bb084924fa8f03f17fa2304317))
* keep task session open across terminal app switches ([21430d1](https://github.com/mossipcams/ajax-cli/commit/21430d1cb3db1695df796250cb8e495d36a94757))
* launch dev web companion on dev runtime ([#37](https://github.com/mossipcams/ajax-cli/issues/37)) ([4f2a4ef](https://github.com/mossipcams/ajax-cli/commit/4f2a4ef348d30680d9edf18245ac67daafdce153))
* make drop idempotent ([217a1c4](https://github.com/mossipcams/ajax-cli/commit/217a1c490c8043d312a7d57d0556b12fd9682d6b))
* make task drop tolerate stale substrate ([6131699](https://github.com/mossipcams/ajax-cli/commit/6131699648a63ba3b936e3d7a58459f373ceeadb))
* migrate PWA shell to ajax-web ([#47](https://github.com/mossipcams/ajax-cli/issues/47)) ([a6ab644](https://github.com/mossipcams/ajax-cli/commit/a6ab644fb59e64f3343e26963b02f9ca969c60ed))
* optimistically remove dropped cockpit tasks ([f3a970e](https://github.com/mossipcams/ajax-cli/commit/f3a970e065403d5823991861376ac6a989885d7d))
* own cockpit task pty bridge ([8d54cb2](https://github.com/mossipcams/ajax-cli/commit/8d54cb2187bd71498cf9149b3b7d84466050e50f))
* pin explicit crate versions for release-please ([#46](https://github.com/mossipcams/ajax-cli/issues/46)) ([e9123f5](https://github.com/mossipcams/ajax-cli/commit/e9123f5449dc88dbae42591626735b1b2ba2cd05))
* point release please at crate manifest ([#51](https://github.com/mossipcams/ajax-cli/issues/51)) ([ee2047a](https://github.com/mossipcams/ajax-cli/commit/ee2047a7bf863632d194f07efb32e9ef3280706d))
* prepare task pty exec before fork ([65baa17](https://github.com/mossipcams/ajax-cli/commit/65baa17465d91cdb33c96d7498ce72180daef54a))
* reattach after interrupted task client ([5476fc0](https://github.com/mossipcams/ajax-cli/commit/5476fc0a27d49920d72019594a39ae220a0f4be7))
* reconcile task substrate before use ([1abe8ce](https://github.com/mossipcams/ajax-cli/commit/1abe8ce6b1fec34e6af1d7a776e55d3f87e0d369))
* recover missing ajax tasks from substrate ([b3c9262](https://github.com/mossipcams/ajax-cli/commit/b3c9262041d41df9ac6a79eb30f032468fafe1fc))
* recover task session on terminal interruption ([e2edd9d](https://github.com/mossipcams/ajax-cli/commit/e2edd9d9d05c9833be3dcffd9e7de912a81abd40))
* refresh json read commands regardless of projection freshness ([#40](https://github.com/mossipcams/ajax-cli/issues/40)) ([a89dcff](https://github.com/mossipcams/ajax-cli/commit/a89dcff6264b6d901029f223af5507367f346af2))
* refresh mobile web cockpit state ([#41](https://github.com/mossipcams/ajax-cli/issues/41)) ([4a23b67](https://github.com/mossipcams/ajax-cli/commit/4a23b67df7828e64a4190514ceceb650a5af3138))
* refresh stale live conflict tasks ([df67787](https://github.com/mossipcams/ajax-cli/commit/df6778752d8e95a835d77d983fdbff2de7759ddd))
* repair release please workspace config ([#48](https://github.com/mossipcams/ajax-cli/issues/48)) ([5f05082](https://github.com/mossipcams/ajax-cli/commit/5f05082beb0d145df17d40eb7ff73bf14ee17193))
* retry interrupted interactive terminal IO ([f70d6c1](https://github.com/mossipcams/ajax-cli/commit/f70d6c11e9b421f38250243aed81a15ba5935696))
* retry interrupted task PTY poll ([42fdabf](https://github.com/mossipcams/ajax-cli/commit/42fdabf9f7486dab7d8cdc64e6853fbed453c157))
* retry interrupted task PTY poll ([#2](https://github.com/mossipcams/ajax-cli/issues/2)) ([0312116](https://github.com/mossipcams/ajax-cli/commit/0312116df3800d6489cbd199829be4a0484fb725))
* return from cockpit task sessions with ctrl-q ([4468541](https://github.com/mossipcams/ajax-cli/commit/4468541a40a041e69f30a9704223193bd2610dc4))
* return from task to cockpit with control q ([0e843de](https://github.com/mossipcams/ajax-cli/commit/0e843decd45b44340e377cfe6458cc0d623b6395))
* route cockpit drop through observed teardown ([1f2d8f3](https://github.com/mossipcams/ajax-cli/commit/1f2d8f3478ac2c2e3045733183ec8688c32244f5))
* skip idle cockpit live probes ([#3](https://github.com/mossipcams/ajax-cli/issues/3)) ([8994e14](https://github.com/mossipcams/ajax-cli/commit/8994e147b64aff4996d485ed752fc25e99dfc0e0))
* surface action failures as json instead of dropping the connection ([#44](https://github.com/mossipcams/ajax-cli/issues/44)) ([fb9043d](https://github.com/mossipcams/ajax-cli/commit/fb9043def1d538e20bc07c9fd08f9de9ed602720))
* tighten cockpit sync refresh ([fa74372](https://github.com/mossipcams/ajax-cli/commit/fa7437245193215e4fadd7c3bdb01f65e5ecec8e))


### Performance Improvements

* lighten ajax refresh paths ([#7](https://github.com/mossipcams/ajax-cli/issues/7)) ([6400d2b](https://github.com/mossipcams/ajax-cli/commit/6400d2bf8ae11678cae07096e9d02aa3c8b62441))
