# Changelog

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
