# Changelog

All notable Ajax CLI changes should be recorded here.

## [0.58.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.57.0...ajax-cli-v0.58.0) (2026-09-05)


### Features

* **core:** poll task PR CI and notify agents on failure ([#1048](https://github.com/mossipcams/ajax-cli/issues/1048)) ([0fde4f4](https://github.com/mossipcams/ajax-cli/commit/0fde4f46220a8c903888b80d5e714b93711db131))
* **core:** seed ajax-model-router dispatch links on new tasks ([#1094](https://github.com/mossipcams/ajax-cli/issues/1094)) ([055754c](https://github.com/mossipcams/ajax-cli/commit/055754c9475c786f49178b74c24c265b4d756846))
* **web:** add /clear slash command to Ajax Chat ([#1130](https://github.com/mossipcams/ajax-cli/issues/1130)) ([ab08f9b](https://github.com/mossipcams/ajax-cli/commit/ab08f9bf32e43130fae1dfbe93ad4e427f545dc7))
* **web:** add runtime control panel for restart and update ([#1102](https://github.com/mossipcams/ajax-cli/issues/1102)) ([e044477](https://github.com/mossipcams/ajax-cli/commit/e0444779f94f597b2b7690558cedea3efdb24d35))
* **web:** auto-approve ACP chat permissions for full access ([#1017](https://github.com/mossipcams/ajax-cli/issues/1017)) ([13d327c](https://github.com/mossipcams/ajax-cli/commit/13d327c9f01cb2ac01b8149679ba59cd515b2875))
* **web:** chronological ACP chat stick-to-bottom with recent-first history window ([#1088](https://github.com/mossipcams/ajax-cli/issues/1088)) ([d1d9828](https://github.com/mossipcams/ajax-cli/commit/d1d9828a9131fe67fd67480a25800a9a4c97a434))
* **web:** collapse Ajax chat activity into one row per turn ([#964](https://github.com/mossipcams/ajax-cli/issues/964)) ([538bdd0](https://github.com/mossipcams/ajax-cli/commit/538bdd07ce66ae2bbe5f6cd7b428412861c9e12a))
* **web:** collapse Ajax Chat tool rows behind activity disclosure ([#1097](https://github.com/mossipcams/ajax-cli/issues/1097)) ([463aa5c](https://github.com/mossipcams/ajax-cli/commit/463aa5c5bb8c9938d872942b398ea31da555c618))
* **web:** default new tasks to Ajax chat with a persistent terminal switch ([#932](https://github.com/mossipcams/ajax-cli/issues/932)) ([24b1c7f](https://github.com/mossipcams/ajax-cli/commit/24b1c7f67d13e1d040d52bba47aa7899e41734db))
* **web:** full-width Ajax Chat composer with icon hotbar ([#1032](https://github.com/mossipcams/ajax-cli/issues/1032)) ([75bdb02](https://github.com/mossipcams/ajax-cli/commit/75bdb0242df494b04779c59b0a1ce15fd93ebfa8))
* **web:** give orchestration chat one per-task session runtime ([#915](https://github.com/mossipcams/ajax-cli/issues/915)) ([a11d1f4](https://github.com/mossipcams/ajax-cli/commit/a11d1f46fb757d47a44e63eab6ab650e4cdce4ec))
* **web:** keep iPhone screen awake during active Cockpit use ([#1134](https://github.com/mossipcams/ajax-cli/issues/1134)) ([b4fffaf](https://github.com/mossipcams/ajax-cli/commit/b4fffaf830c1e5e8957d7c1258c4d85a681a227f))
* **web:** make Ajax chat Task details a thin operator dossier ([#961](https://github.com/mossipcams/ajax-cli/issues/961)) ([2741741](https://github.com/mossipcams/ajax-cli/commit/2741741835f3f0ac3827230b82fd6628d749657a))
* **web:** merge Control tab into Settings ([#1105](https://github.com/mossipcams/ajax-cli/issues/1105)) ([519b962](https://github.com/mossipcams/ajax-cli/commit/519b962beb1a034a65b887338be57bddef963722))
* **web:** move harness switch from Diff Review to task details ([#916](https://github.com/mossipcams/ajax-cli/issues/916)) ([637d2c3](https://github.com/mossipcams/ajax-cli/commit/637d2c334692954a8ca6997d204fa4f2cce4fa63))
* **web:** orchestration session chat with live ACP thinking ([#917](https://github.com/mossipcams/ajax-cli/issues/917)) ([988a91e](https://github.com/mossipcams/ajax-cli/commit/988a91e594c979cc780a525989a3539c21854e03))
* **web:** parse Cursor ACP per-turn token usage ([#966](https://github.com/mossipcams/ajax-cli/issues/966)) ([d8062ca](https://github.com/mossipcams/ajax-cli/commit/d8062cafbd86d338c5c8d1a4f0e57d184d8eea6c))
* **web:** persist unsent composer drafts and queued follow-up across navigation. ([#1068](https://github.com/mossipcams/ajax-cli/issues/1068)) ([2a20f81](https://github.com/mossipcams/ajax-cli/commit/2a20f81d6c97b5ebee606cb2df5b6314cf9d6e94))
* **web:** pick and switch task models from advertised ACP options ([#1016](https://github.com/mossipcams/ajax-cli/issues/1016)) ([38aaf8f](https://github.com/mossipcams/ajax-cli/commit/38aaf8fb84875f8010356f05b8675bfec4525c93))
* **web:** redesign the ACP conversation as a mobile chat ([#986](https://github.com/mossipcams/ajax-cli/issues/986)) ([654ba02](https://github.com/mossipcams/ajax-cli/commit/654ba02169e48e71b27413315525e60e15f5a52b))
* **web:** render the ACP conversation as typed items ([#903](https://github.com/mossipcams/ajax-cli/issues/903)) ([4264709](https://github.com/mossipcams/ajax-cli/commit/4264709cfe6bc977369f08a9f37c7fb882e18786))
* **web:** report ACP run-state as task truth, and fix Ajax Chat conversation flow ([#1054](https://github.com/mossipcams/ajax-cli/issues/1054)) ([04c2e42](https://github.com/mossipcams/ajax-cli/commit/04c2e42dd09372ce149e1222879e6a6f6d3b1330))
* **web:** restore Ajax Chat ACP context after reconnect ([#1099](https://github.com/mossipcams/ajax-cli/issues/1099)) ([7c0f6a2](https://github.com/mossipcams/ajax-cli/commit/7c0f6a2d732dc7d2a836cf8a5fc51c525b038230))
* **web:** run each harness through its own ACP with a model page ([#879](https://github.com/mossipcams/ajax-cli/issues/879)) ([eb0b3e5](https://github.com/mossipcams/ajax-cli/commit/eb0b3e515a6c16201016fb1272476131ce8f6a98))
* **web:** shortlist session models and turn-as-chapter chat ([#973](https://github.com/mossipcams/ajax-cli/issues/973)) ([4d7ec44](https://github.com/mossipcams/ajax-cli/commit/4d7ec4491a54c97d56a3b7be95ed82f8264e3706))
* **web:** show ACP agent status in Ajax chat ([#958](https://github.com/mossipcams/ajax-cli/issues/958)) ([bc278c2](https://github.com/mossipcams/ajax-cli/commit/bc278c29b4dad57e6cc0cf8ca56f993cac50c741))
* **web:** show ACP session context usage in chat ([#956](https://github.com/mossipcams/ajax-cli/issues/956)) ([225484d](https://github.com/mossipcams/ajax-cli/commit/225484d30b84f3357fe24e382a62a4b052c629fb))
* **web:** show Cursor ACP turn tokens in Ajax chat ([#967](https://github.com/mossipcams/ajax-cli/issues/967)) ([eb045fb](https://github.com/mossipcams/ajax-cli/commit/eb045fb4d33cf386e174d3ac9d6469bb3d23d31c))
* **web:** show Test in Dev in session chat details ([#919](https://github.com/mossipcams/ajax-cli/issues/919)) ([b19cc14](https://github.com/mossipcams/ajax-cli/commit/b19cc146c2566866c8f3f1da2095c12c0634d6a5))
* **web:** show Test in Stable on Ajax web dev Settings ([#946](https://github.com/mossipcams/ajax-cli/issues/946)) ([3133879](https://github.com/mossipcams/ajax-cli/commit/3133879dd8deeed47e883fb4904f1aa5eab7dbb0))
* **web:** show tool rows, copy answers, and recover failed prompts in chat ([#1085](https://github.com/mossipcams/ajax-cli/issues/1085)) ([71d431a](https://github.com/mossipcams/ajax-cli/commit/71d431a66a37b87c3122329155e7dad2f2c76259))
* **web:** slim Cursor catalog and persist split Fast/effort ([#991](https://github.com/mossipcams/ajax-cli/issues/991)) ([59c5599](https://github.com/mossipcams/ajax-cli/commit/59c55999366393e93b50f5378732cfa3787c71db))
* **web:** surface more ACP session capabilities in Chat ([#1030](https://github.com/mossipcams/ajax-cli/issues/1030)) ([d00a59b](https://github.com/mossipcams/ajax-cli/commit/d00a59b85bc98124ddcb7b285b132d089a37a34d))


### Bug Fixes

* align run-state ownership, ACP session, and cockpit chrome with architecture ([#1108](https://github.com/mossipcams/ajax-cli/issues/1108)) ([ee5d7ee](https://github.com/mossipcams/ajax-cli/commit/ee5d7eee58f932bfce3a931bf277b728dd473b4e))
* **ci:** normalize observation findings missing expected/actual ([#924](https://github.com/mossipcams/ajax-cli/issues/924)) ([be6b821](https://github.com/mossipcams/ajax-cli/commit/be6b82189f288ebfb7ac0e9fef753a3abbe82334))
* **ci:** stream exploratory prompt via stdin ([#1121](https://github.com/mossipcams/ajax-cli/issues/1121)) ([b451972](https://github.com/mossipcams/ajax-cli/commit/b4519723f46da6408a9163e81cccbd1fb1923acd))
* **core:** isolate git env so Drop survives bare/worktree config ([#943](https://github.com/mossipcams/ajax-cli/issues/943)) ([9c9f9a3](https://github.com/mossipcams/ajax-cli/commit/9c9f9a3aab1f53338cae5fc16ddfea0bbf110358))
* **core:** prompt ACP agent on first CI failure ([#1060](https://github.com/mossipcams/ajax-cli/issues/1060)) ([1117d83](https://github.com/mossipcams/ajax-cli/commit/1117d8384d5570ff9ae7ef4b13ac68319c71b5a0))
* **core:** prune stale origin/ajax tracking refs on drop ([#845](https://github.com/mossipcams/ajax-cli/issues/845)) ([79fc1e4](https://github.com/mossipcams/ajax-cli/commit/79fc1e4454caad3c3cae151f65a0813f771b6fab))
* **core:** rebuild kernel run-state with launch-episode attempts ([#1123](https://github.com/mossipcams/ajax-cli/issues/1123)) ([68d4195](https://github.com/mossipcams/ajax-cli/commit/68d41956f8b154293b818adde3cfef78b857221f))
* **core:** retract stale AgentRunning claim when no agent evidence remains ([#897](https://github.com/mossipcams/ajax-cli/issues/897)) ([53162b4](https://github.com/mossipcams/ajax-cli/commit/53162b45aec58040b34c4441c5b2bac62d35a1ad))
* hold CI and merge-conflict pings until GitHub checks settle ([#1072](https://github.com/mossipcams/ajax-cli/issues/1072)) ([768e891](https://github.com/mossipcams/ajax-cli/commit/768e89187d95865953934078a11d84a0309421f7))
* restore run-state ownership, ACP session, and cockpit chrome ([#1114](https://github.com/mossipcams/ajax-cli/issues/1114)) ([13e39c5](https://github.com/mossipcams/ajax-cli/commit/13e39c5a3d704898e0731993967d6b957e9b4f4a))
* split-axis pin matching and PTY drain before attach close ([#1024](https://github.com/mossipcams/ajax-cli/issues/1024)) ([bf7d6d0](https://github.com/mossipcams/ajax-cli/commit/bf7d6d0014b81dae35f067b324db82716c31e20c))
* **web:** apply advertised ACP session config options ([#999](https://github.com/mossipcams/ajax-cli/issues/999)) ([b78d328](https://github.com/mossipcams/ajax-cli/commit/b78d328280fbcd24f17666ba8eaea421fe3a4c56))
* **web:** apply Switch models in-band and reset harness context ([#985](https://github.com/mossipcams/ajax-cli/issues/985)) ([3b065ea](https://github.com/mossipcams/ajax-cli/commit/3b065ead329deeb5b7fd5f33c799b6b0dcd5d41e))
* **web:** avoid control-lane deadlock during session activity reporting ([#1084](https://github.com/mossipcams/ajax-cli/issues/1084)) ([98b3c18](https://github.com/mossipcams/ajax-cli/commit/98b3c1839f8acb8f3c4e126b0e3f0dcde803fe1b))
* **web:** close current cockpit defects ([#1062](https://github.com/mossipcams/ajax-cli/issues/1062)) ([b88ec0a](https://github.com/mossipcams/ajax-cli/commit/b88ec0a9b0ce69ba6cccc5a6b6fdc9707be36c28))
* **web:** close high and medium defects ([#849](https://github.com/mossipcams/ajax-cli/issues/849)) ([08e8379](https://github.com/mossipcams/ajax-cli/commit/08e83799c426bd4cf764aebeeb6b1dc801bae7e8))
* **web:** commit drop when Dismiss is clicked during undo window ([#914](https://github.com/mossipcams/ajax-cli/issues/914)) ([26761c6](https://github.com/mossipcams/ajax-cli/commit/26761c65687126fbf26279c64ce2bdc3194f7da1))
* **web:** compress chat photos on attach so Send dispatches ([#1117](https://github.com/mossipcams/ajax-cli/issues/1117)) ([449e5dc](https://github.com/mossipcams/ajax-cli/commit/449e5dc7ace17b9cd5bb566b1e414b3d532104aa))
* **web:** dismiss stuck ACP approve/reject prompt ([#1019](https://github.com/mossipcams/ajax-cli/issues/1019)) ([6a738cc](https://github.com/mossipcams/ajax-cli/commit/6a738cc92bc455ef609c23de0b311e7d04644f79))
* **web:** dock session composer without nested pin and restyle Mic ([#921](https://github.com/mossipcams/ajax-cli/issues/921)) ([1567b71](https://github.com/mossipcams/ajax-cli/commit/1567b71a32963193c93030b4571a62432ad5f9dd))
* **web:** drain idle chat ACP child during reconnect grace ([#1029](https://github.com/mossipcams/ajax-cli/issues/1029)) ([3cf6ab8](https://github.com/mossipcams/ajax-cli/commit/3cf6ab8b410486651673cdcb96b094b93b393f7f))
* **web:** harden ACP chat reliability ([#891](https://github.com/mossipcams/ajax-cli/issues/891)) ([efdee46](https://github.com/mossipcams/ajax-cli/commit/efdee46a8e13aba9a013f07d3dec878b25aa90c0))
* **web:** harden orchestration chat flow ([#890](https://github.com/mossipcams/ajax-cli/issues/890)) ([0705251](https://github.com/mossipcams/ajax-cli/commit/070525125818935e0e47fd2248d3ff2e89454a34))
* **web:** hide dashboard task actions behind swipe-left ([#1131](https://github.com/mossipcams/ajax-cli/issues/1131)) ([d1ee8ce](https://github.com/mossipcams/ajax-cli/commit/d1ee8ce3c88ed92b3befcbe45dfb4f3c25b5d389))
* **web:** ignore invalid hashes, late Start, and task-404 disconnects ([#909](https://github.com/mossipcams/ajax-cli/issues/909)) ([fcc692e](https://github.com/mossipcams/ajax-cli/commit/fcc692e2e35b84aa3b41b1f2e5e956868f8c930d))
* **web:** keep ACP model context across ajax-web restart ([#1063](https://github.com/mossipcams/ajax-cli/issues/1063)) ([53332b8](https://github.com/mossipcams/ajax-cli/commit/53332b8dc09ee3d9671d3793e2514d7328e70f76))
* **web:** keep ACP permission pending until resolved ([#894](https://github.com/mossipcams/ajax-cli/issues/894)) ([4257330](https://github.com/mossipcams/ajax-cli/commit/42573302fba9fda9466517a4e9afcff6ffcedda3))
* **web:** keep Ajax chat header below the iPhone notch ([#1000](https://github.com/mossipcams/ajax-cli/issues/1000)) ([d3b52b8](https://github.com/mossipcams/ajax-cli/commit/d3b52b82e19b79a433b3ea19d39a635db576fc35))
* **web:** keep Ajax chat Task details on-screen and scrolling on mobile ([#980](https://github.com/mossipcams/ajax-cli/issues/980)) ([10f2cdc](https://github.com/mossipcams/ajax-cli/commit/10f2cdcead9157b95727f04b720578cf8a720dcb))
* **web:** keep Ajax chat visible in terminal Task details ([#972](https://github.com/mossipcams/ajax-cli/issues/972)) ([8e5100a](https://github.com/mossipcams/ajax-cli/commit/8e5100ac5957ee0641ce2bb5a4f7c75a410bf7f0))
* **web:** keep chat sessions off the runtime and stop silent input loss ([#902](https://github.com/mossipcams/ajax-cli/issues/902)) ([1058587](https://github.com/mossipcams/ajax-cli/commit/1058587d560d6a3482d4e67f88cf8cc9f43586b5))
* **web:** keep composer Send/Attach taps during PWA keyboard restore ([#1113](https://github.com/mossipcams/ajax-cli/issues/1113)) ([a7d479a](https://github.com/mossipcams/ajax-cli/commit/a7d479ad54c54aafcffcdc8445589db5e9048e43))
* **web:** keep distinct chat replies and persist queued follow-ups ([#1140](https://github.com/mossipcams/ajax-cli/issues/1140)) ([d380f2a](https://github.com/mossipcams/ajax-cli/commit/d380f2ab5f1af2dea499ec6452b92c43066d7234))
* **web:** keep grok effort and thinking models selectable ([#1005](https://github.com/mossipcams/ajax-cli/issues/1005)) ([80aee7e](https://github.com/mossipcams/ajax-cli/commit/80aee7e4f75fafd0c99a8d23aa89bb04a642fa4e))
* **web:** keep keyboard open when tapping the hotbar ([#1057](https://github.com/mossipcams/ajax-cli/issues/1057)) ([93c74ca](https://github.com/mossipcams/ajax-cli/commit/93c74cab02fbe14087942257dc10aa38dd4c8720))
* **web:** keep New Task chrome visible after iOS app switch ([#837](https://github.com/mossipcams/ajax-cli/issues/837)) ([6e5fc62](https://github.com/mossipcams/ajax-cli/commit/6e5fc62428316465def9c5ca013bb6ec99bcd6ac))
* **web:** keep Reload tappable after a failed attempt ([#978](https://github.com/mossipcams/ajax-cli/issues/978)) ([8d7a3a4](https://github.com/mossipcams/ajax-cli/commit/8d7a3a41700dfc2ba211ea92b3c8b3a5c3126ca6))
* **web:** keep sentence-boundary stream chunks on one chat bubble ([#1142](https://github.com/mossipcams/ajax-cli/issues/1142)) ([950500e](https://github.com/mossipcams/ajax-cli/commit/950500e3771105cebd504bdc9a7e0925d10ebf5f))
* **web:** keep task workspace Details tappable and cap dashboard swipe-back ([#1127](https://github.com/mossipcams/ajax-cli/issues/1127)) ([f6cb6e3](https://github.com/mossipcams/ajax-cli/commit/f6cb6e3ec39123cfbae7979ae520092ec3748232))
* **web:** keep Test in Dev in-progress after start ([#1036](https://github.com/mossipcams/ajax-cli/issues/1036)) ([083b07d](https://github.com/mossipcams/ajax-cli/commit/083b07dbdc8b51e1a4b16f8e63ba2827bd75ed83))
* **web:** keep Test in Stable up after a stale pid file ([#944](https://github.com/mossipcams/ajax-cli/issues/944)) ([9ffadbb](https://github.com/mossipcams/ajax-cli/commit/9ffadbb179d64036fc2b977da08b513fbf3a344d))
* **web:** keep Test in Stable up until dedicated main is healthy ([#951](https://github.com/mossipcams/ajax-cli/issues/951)) ([1c446f3](https://github.com/mossipcams/ajax-cli/commit/1c446f332bbee91c4974c4d6b9cb6c3b6816164e))
* **web:** keep the Ajax chat model picker on the chosen model ([#933](https://github.com/mossipcams/ajax-cli/issues/933)) ([afcbe7d](https://github.com/mossipcams/ajax-cli/commit/afcbe7de5e912b57c34fa0aee30b4f842e140a7a))
* **web:** land Ajax Chat on the last message when opening a task ([#1067](https://github.com/mossipcams/ajax-cli/issues/1067)) ([6f8bef6](https://github.com/mossipcams/ajax-cli/commit/6f8bef6ee32a928ca6c1747aabec716237a0e3c9))
* **web:** launch selected Cursor ACP models without Fast ([#988](https://github.com/mossipcams/ajax-cli/issues/988)) ([6807f09](https://github.com/mossipcams/ajax-cli/commit/6807f091abefd77ed0767e9950d72000d0d15c03))
* **web:** let operators select the model on an Ajax chat task ([#940](https://github.com/mossipcams/ajax-cli/issues/940)) ([a37d5b0](https://github.com/mossipcams/ajax-cli/commit/a37d5b01e9e3a53ca99a67b663b3cbf25241137e))
* **web:** load a fresh shell when Update ready is tapped ([#1009](https://github.com/mossipcams/ajax-cli/issues/1009)) ([d3a5308](https://github.com/mossipcams/ajax-cli/commit/d3a53083238cb15f7ca2d3f9bb85254987567206))
* **web:** make Ajax chat run the selected model ([#953](https://github.com/mossipcams/ajax-cli/issues/953)) ([bb79c63](https://github.com/mossipcams/ajax-cli/commit/bb79c6381cec4f3d64d4201ade11ef97a0947580))
* **web:** make chat tool-call detail legible on a phone ([#1025](https://github.com/mossipcams/ajax-cli/issues/1025)) ([281ea2a](https://github.com/mossipcams/ajax-cli/commit/281ea2a90daf1e33aa13d424aacd024c7c5e3e6f))
* **web:** make page switching one continuous cross-slide ([#1074](https://github.com/mossipcams/ajax-cli/issues/1074)) ([69ded3b](https://github.com/mossipcams/ajax-cli/commit/69ded3b05ea0909714499302e7845026129b6370))
* **web:** make session Mic text-only steel blue ([#923](https://github.com/mossipcams/ajax-cli/issues/923)) ([b9790ff](https://github.com/mossipcams/ajax-cli/commit/b9790ff1126ad0a21240fb68db37291de5cd305f))
* **web:** make swipe-reveal ActionBar a real hit target ([#1038](https://github.com/mossipcams/ajax-cli/issues/1038)) ([#1126](https://github.com/mossipcams/ajax-cli/issues/1126)) ([f602185](https://github.com/mossipcams/ajax-cli/commit/f602185f728960955a05bb89c9e5da86b811b116))
* **web:** make Update ready reload retryable without legacy cleanup ([#1012](https://github.com/mossipcams/ajax-cli/issues/1012)) ([8422458](https://github.com/mossipcams/ajax-cli/commit/8422458da456701710ab9d4015c4a73e2eafc98d))
* **web:** map Cursor pins to split-axis or exploded ACP ids ([#1021](https://github.com/mossipcams/ajax-cli/issues/1021)) ([76d7dea](https://github.com/mossipcams/ajax-cli/commit/76d7dea24c855d96353acc63660705b2b3da3b98))
* **web:** name Ajax Chat tool rows from path, query, or command ([#1091](https://github.com/mossipcams/ajax-cli/issues/1091)) ([577c281](https://github.com/mossipcams/ajax-cli/commit/577c2815a25f4d426ab3d845310740266115ca17))
* **web:** pass explicit Cursor catalog ids on spawn argv ([#992](https://github.com/mossipcams/ajax-cli/issues/992)) ([21db6de](https://github.com/mossipcams/ajax-cli/commit/21db6dee75d25a8da6b59736830bedbbb536e1ca))
* **web:** persist provisioned launch and close session attach holes ([#872](https://github.com/mossipcams/ajax-cli/issues/872)) ([56385dc](https://github.com/mossipcams/ajax-cli/commit/56385dcbf47928f2857772013d43ec6662ff3c3e))
* **web:** pin Cursor chat model and select it from Switch ([#981](https://github.com/mossipcams/ajax-cli/issues/981)) ([43f0a19](https://github.com/mossipcams/ajax-cli/commit/43f0a190a35ef880f3f71db008b0a8a7c26ae072))
* **web:** pin session chat to the keyboard band and arm Mic ([#920](https://github.com/mossipcams/ajax-cli/issues/920)) ([f39730f](https://github.com/mossipcams/ajax-cli/commit/f39730f49573a3e5049098a5b302ab564eb92412))
* **web:** preserve chat text selection ([#1037](https://github.com/mossipcams/ajax-cli/issues/1037)) ([3f77548](https://github.com/mossipcams/ajax-cli/commit/3f775484048f6abda3d87f8f5054f886abc29517))
* **web:** preserve Cursor reasoning level in model switches ([#993](https://github.com/mossipcams/ajax-cli/issues/993)) ([4ed4f48](https://github.com/mossipcams/ajax-cli/commit/4ed4f482a92618b0c3efe571da97606c81fd4135))
* **web:** preserve idle session state after replay ([#996](https://github.com/mossipcams/ajax-cli/issues/996)) ([80b2e58](https://github.com/mossipcams/ajax-cli/commit/80b2e58ae17dbc2dfd9ce71445f92a2dd2d5cd34))
* **web:** preserve replayed session history ([#882](https://github.com/mossipcams/ajax-cli/issues/882)) ([83ee7e4](https://github.com/mossipcams/ajax-cli/commit/83ee7e41cd8d746ecfee93ec53797d3006364d92))
* **web:** preserve transcript selection during swipe ([#1052](https://github.com/mossipcams/ajax-cli/issues/1052)) ([4061335](https://github.com/mossipcams/ajax-cli/commit/406133593eb22f4c457a1e7f5b61674d40876f0f))
* **web:** prune stale orchestration sessions on drop ([#983](https://github.com/mossipcams/ajax-cli/issues/983)) ([c997229](https://github.com/mossipcams/ajax-cli/commit/c9972296ab51d172bb45545e0966074b1bcd035f))
* **web:** raise chat prompt ceiling and restore keyboard scroll ([#934](https://github.com/mossipcams/ajax-cli/issues/934)) ([86f1c88](https://github.com/mossipcams/ajax-cli/commit/86f1c887ef2091540dd5f48696726743887c8b84))
* **web:** rebuild chat session host as feature slices ([#1124](https://github.com/mossipcams/ajax-cli/issues/1124)) ([35f253d](https://github.com/mossipcams/ajax-cli/commit/35f253d9740c4eead202b7b70147848731672187))
* **web:** reconcile split config after model set_config ([#997](https://github.com/mossipcams/ajax-cli/issues/997)) ([#1002](https://github.com/mossipcams/ajax-cli/issues/1002)) ([2a2cb1f](https://github.com/mossipcams/ajax-cli/commit/2a2cb1fa5f090833a9b6a3f1f56c66237c87c0d4))
* **web:** reconstruct Cursor ACP spawn tokens rejected on --model ([#1079](https://github.com/mossipcams/ajax-cli/issues/1079)) ([#1080](https://github.com/mossipcams/ajax-cli/issues/1080)) ([38e3546](https://github.com/mossipcams/ajax-cli/commit/38e3546389fd2367959c6abf3e0fa08cd34c38bf))
* **web:** recover Ajax Chat ACP prompts after child death ([#1089](https://github.com/mossipcams/ajax-cli/issues/1089)) ([2a89ca8](https://github.com/mossipcams/ajax-cli/commit/2a89ca8c245373b7d82449d7ca948f84d4eeb79f))
* **web:** recover Ajax Chat after ACP disconnect ([#1093](https://github.com/mossipcams/ajax-cli/issues/1093)) ([4280535](https://github.com/mossipcams/ajax-cli/commit/42805355acfcde9824a8c20d087674aa8095ba0a))
* **web:** recover Cockpit after Test in Stable restart ([#852](https://github.com/mossipcams/ajax-cli/issues/852)) ([033bca9](https://github.com/mossipcams/ajax-cli/commit/033bca91c61cf692f5fd5c7451cb0bc7f729eb6c))
* **web:** release mutation gate if start worker panics ([#1076](https://github.com/mossipcams/ajax-cli/issues/1076)) ([03afe24](https://github.com/mossipcams/ajax-cli/commit/03afe2455edbc95d307d2bbe725fc47b2ecb278c))
* **web:** reload the shell when Update ready is tapped ([#1008](https://github.com/mossipcams/ajax-cli/issues/1008)) ([e71f9de](https://github.com/mossipcams/ajax-cli/commit/e71f9de987d66170cde1f620a62c2b1a499809db))
* **web:** render the ACP response as paragraphs, not a chunk stream ([#905](https://github.com/mossipcams/ajax-cli/issues/905)) ([6c83209](https://github.com/mossipcams/ajax-cli/commit/6c83209fbd3142316443284f03a72e46b69af20a))
* **web:** replace cached chat transcript when ACP session loads ([#1033](https://github.com/mossipcams/ajax-cli/issues/1033)) ([d48bd01](https://github.com/mossipcams/ajax-cli/commit/d48bd013db06a83bb0675a85835db3942dbfba4e))
* **web:** restore Ajax chat on terminal Task details ([#960](https://github.com/mossipcams/ajax-cli/issues/960)) ([ba9ab35](https://github.com/mossipcams/ajax-cli/commit/ba9ab35c506b36528387e0624f4446a1aad00a67))
* **web:** restore iOS PWA composer dock after keyboard dismiss ([#1107](https://github.com/mossipcams/ajax-cli/issues/1107)) ([44520d4](https://github.com/mossipcams/ajax-cli/commit/44520d42273abe69959d9d3be6eb0254fca78623))
* **web:** restore model-list scrolling in new-task sheet ([#1023](https://github.com/mossipcams/ajax-cli/issues/1023)) ([25536b0](https://github.com/mossipcams/ajax-cli/commit/25536b0d8e8880dffb03a678cb581cd72f686799))
* **web:** restore session chat layout after iOS keyboard dismiss ([#927](https://github.com/mossipcams/ajax-cli/issues/927)) ([d1ca08a](https://github.com/mossipcams/ajax-cli/commit/d1ca08aeebcc538716799267b340aead6a3a06be))
* **web:** restore session composer controls ([#892](https://github.com/mossipcams/ajax-cli/issues/892)) ([b9c9560](https://github.com/mossipcams/ajax-cli/commit/b9c9560f72e47d5cd0b77a612901b2a9865ff1dc))
* **web:** restore the full Ajax chat model catalog ([#949](https://github.com/mossipcams/ajax-cli/issues/949)) ([df1f05a](https://github.com/mossipcams/ajax-cli/commit/df1f05adf420241195142d4c0b68a37560885034))
* **web:** restore transcript swiping outside active highlighting ([#1050](https://github.com/mossipcams/ajax-cli/issues/1050)) ([6976c6b](https://github.com/mossipcams/ajax-cli/commit/6976c6b40b02366b8f4462f263124700d6c2e54c))
* **web:** retry deferred ACP turn_end so dashboard drops Agent working ([#1133](https://github.com/mossipcams/ajax-cli/issues/1133)) ([cf58add](https://github.com/mossipcams/ajax-cli/commit/cf58add84cd90c4c0bd310663378348772489275))
* **web:** send bounded ACP image attachments inline over WebSocket ([#1120](https://github.com/mossipcams/ajax-cli/issues/1120)) ([74bb531](https://github.com/mossipcams/ajax-cli/commit/74bb5318aaa43f89d8ca9467de01aa011d79ba81))
* **web:** show Ajax chat first in terminal Task details ([#969](https://github.com/mossipcams/ajax-cli/issues/969)) ([23112cb](https://github.com/mossipcams/ajax-cli/commit/23112cb19807a511a3d7f9741053913ba11c4c22))
* **web:** show Ajax chat in the terminal Details sheet ([#937](https://github.com/mossipcams/ajax-cli/issues/937)) ([a21b42a](https://github.com/mossipcams/ajax-cli/commit/a21b42aadbfd405353511f0ecfe2ca03237112fb))
* **web:** show full model catalog and hide single-level effort picker ([#1003](https://github.com/mossipcams/ajax-cli/issues/1003)) ([51d1492](https://github.com/mossipcams/ajax-cli/commit/51d1492d19fffaed0231887935f991026abc1c03))
* **web:** show missing-task errors and dock the session composer ([#913](https://github.com/mossipcams/ajax-cli/issues/913)) ([3f5d16d](https://github.com/mossipcams/ajax-cli/commit/3f5d16d01c0546be893e7c9bdda1d91586482329))
* **web:** shut down live ACP child before session/new on model switch ([#990](https://github.com/mossipcams/ajax-cli/issues/990)) ([bc10293](https://github.com/mossipcams/ajax-cli/commit/bc102937f49db1062a1a89f7b26338573e152567))
* **web:** sit session composer above the home indicator ([#1041](https://github.com/mossipcams/ajax-cli/issues/1041)) ([d472f25](https://github.com/mossipcams/ajax-cli/commit/d472f25fbffab664328e6c08959518d13b77659b))
* **web:** skip lingered terminal session in per-connect reaper ([#857](https://github.com/mossipcams/ajax-cli/issues/857)) ([ecdb356](https://github.com/mossipcams/ajax-cli/commit/ecdb356088fe20bd4637abbc6d8aec97d12d4870))
* **web:** split Cursor spawn and in-band model tokens ([#987](https://github.com/mossipcams/ajax-cli/issues/987)) ([1b7b3fa](https://github.com/mossipcams/ajax-cli/commit/1b7b3fa7b3e26a67efab9c27ed97f88e1ccfd2a3))
* **web:** stop ACP tasks from staying Agent working ([#1073](https://github.com/mossipcams/ajax-cli/issues/1073)) ([e895f15](https://github.com/mossipcams/ajax-cli/commit/e895f15a3bd675be980760b62029a7cc68808d74))
* **web:** stop refusing Cursor catalog model ids ([#955](https://github.com/mossipcams/ajax-cli/issues/955)) ([30d4345](https://github.com/mossipcams/ajax-cli/commit/30d4345d378370c2bcd74bf9496bc83e16e60916))
* **web:** stop rendering ACP session title in the task header ([#1056](https://github.com/mossipcams/ajax-cli/issues/1056)) ([af8cb94](https://github.com/mossipcams/ajax-cli/commit/af8cb9404a331a3d67b426d855ffd8185c471d47))
* **web:** stop set_model persist from panicking the ACP socket ([#963](https://github.com/mossipcams/ajax-cli/issues/963)) ([4ce226b](https://github.com/mossipcams/ajax-cli/commit/4ce226bd9b2197225daac7c6058c19f695f9272a))
* **web:** stop the Ajax chat transcript mangling the text it renders ([#971](https://github.com/mossipcams/ajax-cli/issues/971)) ([fddcc42](https://github.com/mossipcams/ajax-cli/commit/fddcc421abf7f4adee3dc802f57ca4ec06162e58))
* **web:** stop typewriting orchestration chat and show operator turns immediately ([#876](https://github.com/mossipcams/ajax-cli/issues/876)) ([342aafd](https://github.com/mossipcams/ajax-cli/commit/342aafdf11efe6b927c9a2d8d56ae0afab517b0b))
* **web:** surface Ajax chat Drop confirm and dismiss ([#950](https://github.com/mossipcams/ajax-cli/issues/950)) ([a5aa255](https://github.com/mossipcams/ajax-cli/commit/a5aa2555c29d2b6db035f764bf5babb796de1ea2))
* **web:** switch the live Ajax chat model when the picker changes ([#945](https://github.com/mossipcams/ajax-cli/issues/945)) ([7f8656f](https://github.com/mossipcams/ajax-cli/commit/7f8656fac7df625492965235d86e1165ecfcee1d))
* **web:** treat ACP HTTP/2 CANCEL as a cancelled turn ([#1071](https://github.com/mossipcams/ajax-cli/issues/1071)) ([e21b43c](https://github.com/mossipcams/ajax-cli/commit/e21b43c59f6da24a6b6996ce84e46f8ebdc687cd))
* **web:** treat unsolicited ACP HTTP/2 cancel as interrupt ([#1104](https://github.com/mossipcams/ajax-cli/issues/1104)) ([5cba9a5](https://github.com/mossipcams/ajax-cli/commit/5cba9a5e070dc210249bba87fd3a7f7f5ca0804b))
* **web:** unstick page switching after a skipped cross-slide flip ([#1078](https://github.com/mossipcams/ajax-cli/issues/1078)) ([ce770e4](https://github.com/mossipcams/ajax-cli/commit/ce770e47b233bc8dd7549137f952b48f6b2fa31d))
* **web:** wrap overflowing Ajax Chat markdown ([#1137](https://github.com/mossipcams/ajax-cli/issues/1137)) ([59a3995](https://github.com/mossipcams/ajax-cli/commit/59a399522a7bf7c34c3b41a5bb8b53221619457e))


### Performance Improvements

* **web:** keep chat ACP child during reconnect grace ([#1026](https://github.com/mossipcams/ajax-cli/issues/1026)) ([4eb660e](https://github.com/mossipcams/ajax-cli/commit/4eb660e9e87f1c458fe0046bc9d7ab85256cdf8a))


### Code Refactoring

* clear Chat/API structural dependency leftovers ([#1129](https://github.com/mossipcams/ajax-cli/issues/1129)) ([684c9ed](https://github.com/mossipcams/ajax-cli/commit/684c9ed015e0af45b89daee755b02e7b357655b2))
* **cli:** rename tmux session module and type agent-event hooks ([#1125](https://github.com/mossipcams/ajax-cli/issues/1125)) ([2a8e329](https://github.com/mossipcams/ajax-cli/commit/2a8e32905c4cca68744d76bd03583a73b7a6a9dc))
* **web:** extract Task Workspace with a shared chat header ([#995](https://github.com/mossipcams/ajax-cli/issues/995)) ([15fc172](https://github.com/mossipcams/ajax-cli/commit/15fc172d52a5f6e8e3a162e75890b67b6c42308d))
* **web:** migrate cockpit reads and mutations to TanStack Query ([#939](https://github.com/mossipcams/ajax-cli/issues/939)) ([fd9ff7d](https://github.com/mossipcams/ajax-cli/commit/fd9ff7db5411d746261be2566437b364a9df6993))
* **web:** split Ajax Chat into independently owned capabilities ([#1027](https://github.com/mossipcams/ajax-cli/issues/1027)) ([41ad9c6](https://github.com/mossipcams/ajax-cli/commit/41ad9c6b3e7937e1e9a34736afff00f5bc2a4950))
* **web:** split cockpit CSS into ownership modules ([#982](https://github.com/mossipcams/ajax-cli/issues/982)) ([583d69f](https://github.com/mossipcams/ajax-cli/commit/583d69f23d1dcce14d8b5edde3539850e0b1bee7))


### Reverts

* align run-state ownership, ACP session, and cockpit chrome with architecture ([#1109](https://github.com/mossipcams/ajax-cli/issues/1109)) ([708a3aa](https://github.com/mossipcams/ajax-cli/commit/708a3aa1deb30fa93c9f7be3fff5c1e281d0de6b))
* Friday Aug 28 and Saturday Aug 29 merges ([#1118](https://github.com/mossipcams/ajax-cli/issues/1118)) ([d829ed5](https://github.com/mossipcams/ajax-cli/commit/d829ed5ef617d273e793c0c3b17ab2b73d5f51cb))
* **web:** restore iOS PWA composer dock after keyboard dismiss ([#1116](https://github.com/mossipcams/ajax-cli/issues/1116)) ([b483605](https://github.com/mossipcams/ajax-cli/commit/b483605e1789d7ee838807b99cf031777d03dd25))

## [0.57.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.56.0...ajax-cli-v0.57.0) (2026-08-12)


### Features

* **core:** delete origin/ajax/* branches during drop and tidy ([#834](https://github.com/mossipcams/ajax-cli/issues/834)) ([d046403](https://github.com/mossipcams/ajax-cli/commit/d0464035b271f40c572206a17ab6f2554da9ea63))


### Bug Fixes

* **core:** prefer durable substrate over stale live Missing ([#792](https://github.com/mossipcams/ajax-cli/issues/792)) ([395e1f1](https://github.com/mossipcams/ajax-cli/commit/395e1f12f23772b6dc367c36dad383c8c7ba0b32))
* pin Cursor default model to Grok 4.6 high ([#833](https://github.com/mossipcams/ajax-cli/issues/833)) ([1c32bd4](https://github.com/mossipcams/ajax-cli/commit/1c32bd48109d1ce7b4616e10f62adf7ee87ba108))
* **web:** add sync click latches across Cockpit double-tap surfaces ([#830](https://github.com/mossipcams/ajax-cli/issues/830)) ([811e95c](https://github.com/mossipcams/ajax-cli/commit/811e95c70757bd5f8c6d798b1e4f6c607349dddd))
* **web:** Drop leave-latch and terminal link paste ([#786](https://github.com/mossipcams/ajax-cli/issues/786)) ([294a1e7](https://github.com/mossipcams/ajax-cli/commit/294a1e7f25bbe4ef72b9850b2c7bd4f9536aff93))
* **web:** gate ActionBar during Drop confirm and dismiss New Task on Settings ([#826](https://github.com/mossipcams/ajax-cli/issues/826)) ([bf5060f](https://github.com/mossipcams/ajax-cli/commit/bf5060f65ef0a122143a8cee0e22e607f0e7a86c))
* **web:** gate push presence on foreground cockpit polls ([#794](https://github.com/mossipcams/ajax-cli/issues/794)) ([5d820f9](https://github.com/mossipcams/ajax-cli/commit/5d820f9ef4e73f4400634844b4e1f1a341e70c86))
* **web:** keep cockpit and detail truth from stale races ([#822](https://github.com/mossipcams/ajax-cli/issues/822)) ([6ec5d14](https://github.com/mossipcams/ajax-cli/commit/6ec5d146335f3e6c3e1ad437c00c1ea309389290))
* **web:** keep Drop on switched task, bottom toast, restore link paste ([#783](https://github.com/mossipcams/ajax-cli/issues/783)) ([34ad9ae](https://github.com/mossipcams/ajax-cli/commit/34ad9aeb1e5b6ca1a1ccfbdce41faf402e6a4347))
* **web:** land seeded task opens at the CLI bottom ([#825](https://github.com/mossipcams/ajax-cli/issues/825)) ([9a7aa7c](https://github.com/mossipcams/ajax-cli/commit/9a7aa7c94a0e7e7a60eab0f380c4ca51beb51d8a))
* **web:** latch New Task, mic speech, and resumeOnOpen races ([#824](https://github.com/mossipcams/ajax-cli/issues/824)) ([fc1ea4e](https://github.com/mossipcams/ajax-cli/commit/fc1ea4e3986725fd8727266b052e7e643b33c1a1))
* **web:** open seeded tasks already at the CLI without scrolling ([#827](https://github.com/mossipcams/ajax-cli/issues/827)) ([88db556](https://github.com/mossipcams/ajax-cli/commit/88db556bddeb7d3e3e0aeda79a40c108dd925001))
* **web:** restore Test in Stable via worktree layout fallback ([#777](https://github.com/mossipcams/ajax-cli/issues/777)) ([781b925](https://github.com/mossipcams/ajax-cli/commit/781b9259023d870189b084ca43e87a37bc58899a))
* **web:** show seeded task opens already at the CLI ([#832](https://github.com/mossipcams/ajax-cli/issues/832)) ([d24701b](https://github.com/mossipcams/ajax-cli/commit/d24701b4858f049cd1e9c83dc9cb92c5524b65fc))
* **web:** stop xterm DA replies from entering PTY stdin ([#782](https://github.com/mossipcams/ajax-cli/issues/782)) ([cff9b81](https://github.com/mossipcams/ajax-cli/commit/cff9b81d6d72dd5616700816a28a38516401102b))
* **web:** yield before open-task nav to cut dashboard INP ([#828](https://github.com/mossipcams/ajax-cli/issues/828)) ([8350a83](https://github.com/mossipcams/ajax-cli/commit/8350a837e21ac0f2deb9e50a224f38964b1ef7d9))

## [0.56.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.55.1...ajax-cli-v0.56.0) (2026-08-05)


### Features

* replace agent statuses with ACP ([#695](https://github.com/mossipcams/ajax-cli/issues/695)) ([dfe216d](https://github.com/mossipcams/ajax-cli/commit/dfe216d411cfd43445a21852ece0e9cf19b2d55d))
* **status:** Cursor wait hooks and ack-safe pane reconcile ([#714](https://github.com/mossipcams/ajax-cli/issues/714)) ([1f232f3](https://github.com/mossipcams/ajax-cli/commit/1f232f321bd79e1a18d1d695e7e111edceba3d4b))
* **status:** improve agent wait detection ([#704](https://github.com/mossipcams/ajax-cli/issues/704)) ([3a51f8b](https://github.com/mossipcams/ajax-cli/commit/3a51f8bccd76b6b015713ee590f1598c29523359))
* **status:** native-hook-first agent status, remove legacy paths ([#678](https://github.com/mossipcams/ajax-cli/issues/678)) ([d3f8212](https://github.com/mossipcams/ajax-cli/commit/d3f82124db8cba8fa47d6077867034c0b2339e9b))
* **status:** reconcile mid-turn waits from pane chrome ([#711](https://github.com/mossipcams/ajax-cli/issues/711)) ([765a3e4](https://github.com/mossipcams/ajax-cli/commit/765a3e43f7e94c379b86092d39f6a1e75993c1c6))
* **web:** add Diff Review vibe-judgment projection ([#726](https://github.com/mossipcams/ajax-cli/issues/726)) ([8268d66](https://github.com/mossipcams/ajax-cli/commit/8268d66251b50ecaee36cab587b56e889ed42cee))
* **web:** add iOS-like hotbar hold-to-repeat ([#667](https://github.com/mossipcams/ajax-cli/issues/667)) ([7c765e3](https://github.com/mossipcams/ajax-cli/commit/7c765e3470bce84964d9ea06a6fd6223dbad6e1b))
* **web:** add PostHog Cloud telemetry for UX performance baselines ([#755](https://github.com/mossipcams/ajax-cli/issues/755)) ([badb8f2](https://github.com/mossipcams/ajax-cli/commit/badb8f2979fa743d6071c0e6dd0cca307f3419fb))
* **web:** add read-only Diff Review for task PRs ([#712](https://github.com/mossipcams/ajax-cli/issues/712)) ([3848fd6](https://github.com/mossipcams/ajax-cli/commit/3848fd666bf5ce8cfe8f509f30697a6e43ea40cd))
* **web:** add stable error codes for operate recovery toasts ([#774](https://github.com/mossipcams/ajax-cli/issues/774)) ([2ac810d](https://github.com/mossipcams/ajax-cli/commit/2ac810de8759b33177a4024dd919356cfe41dea8))
* **web:** add start over voice command for speech dictation ([#736](https://github.com/mossipcams/ajax-cli/issues/736)) ([5f570af](https://github.com/mossipcams/ajax-cli/commit/5f570af400d48aa437e2e59c0c87df870257839b))
* **web:** add Test in Stable and fix Attempts run-on text ([#665](https://github.com/mossipcams/ajax-cli/issues/665)) ([9d00845](https://github.com/mossipcams/ajax-cli/commit/9d008452bc72523e8df93310d2a7e14f824082a2))
* **web:** add xterm web-links, serialize, and floating link menu ([#686](https://github.com/mossipcams/ajax-cli/issues/686)) ([9a86838](https://github.com/mossipcams/ajax-cli/commit/9a8683893a299eee339aff9710c844b832d47ac6))
* **web:** auto-insert speech transcripts and ship a working STT sidecar ([#734](https://github.com/mossipcams/ajax-cli/issues/734)) ([2be1c64](https://github.com/mossipcams/ajax-cli/commit/2be1c64cc123cf8877b85e49fde79e6e328aa73f))
* **web:** continuous speech-to-text input in the task terminal ([#729](https://github.com/mossipcams/ajax-cli/issues/729)) ([d9aaa60](https://github.com/mossipcams/ajax-cli/commit/d9aaa60c5a498d295575428ef4c50ca352180671))
* **web:** dock the dashboard armed channel for iOS thumbs ([#705](https://github.com/mossipcams/ajax-cli/issues/705)) ([b6f8e71](https://github.com/mossipcams/ajax-cli/commit/b6f8e71e134269bc4d0df80a82c857cbfb0ff461))
* **web:** double-tap-hold drag to select terminal text for copy ([#752](https://github.com/mossipcams/ajax-cli/issues/752)) ([fd3074c](https://github.com/mossipcams/ajax-cli/commit/fd3074c9102fce20237374acdd9fe6297f92e6c9))
* **web:** drop the Needs-you inbox for one calm task list ([#676](https://github.com/mossipcams/ajax-cli/issues/676)) ([ca0b3f3](https://github.com/mossipcams/ajax-cli/commit/ca0b3f363376902beac3131f2a731b3ea6e725a3))
* **web:** enrich Cockpit PostHog telemetry for actionable UX queries ([#763](https://github.com/mossipcams/ajax-cli/issues/763)) ([0e0e403](https://github.com/mossipcams/ajax-cli/commit/0e0e4035f4fa03f92e422edad18135f39b6c530c))
* **web:** group the dashboard by attention band and put controls on rows ([#691](https://github.com/mossipcams/ajax-cli/issues/691)) ([888f68b](https://github.com/mossipcams/ajax-cli/commit/888f68b90c0b33bac48b6999a1a12905e657bb00))
* **web:** harden PostHog Safari PWA telemetry with durable queue ([#759](https://github.com/mossipcams/ajax-cli/issues/759)) ([99f1274](https://github.com/mossipcams/ajax-cli/commit/99f127419766d31cf77a279697b88013f7be116a))
* **web:** keep the swipe reveal for row actions ([#685](https://github.com/mossipcams/ajax-cli/issues/685)) ([d15f5f3](https://github.com/mossipcams/ajax-cli/commit/d15f5f3dd6b6e1e71c53cd85e23332d21365db49))
* **web:** lead the dashboard with a fleet-health muster bar ([#677](https://github.com/mossipcams/ajax-cli/issues/677)) ([2cda5fe](https://github.com/mossipcams/ajax-cli/commit/2cda5fe59d66395566d91dd7f2205012cd85dd32))
* **web:** make dashboard actions a primary-key lattice ([#697](https://github.com/mossipcams/ajax-cli/issues/697)) ([b1f36d7](https://github.com/mossipcams/ajax-cli/commit/b1f36d71180f3ae7899360381a56d1eb5039d84b))
* **web:** put task actions on dashboard rows, drop the fleet gauge ([#684](https://github.com/mossipcams/ajax-cli/issues/684)) ([80be364](https://github.com/mossipcams/ajax-cli/commit/80be36409cee412ee818c78ea05a7a317a250b73))
* **web:** rank Diff Review files by signal vs noise ([#716](https://github.com/mossipcams/ajax-cli/issues/716)) ([18522c2](https://github.com/mossipcams/ajax-cli/commit/18522c2e1c61e658ea2052ed7c2f359d3e2494b7))
* **web:** rebuild dashboard as a one-tap control panel ([#696](https://github.com/mossipcams/ajax-cli/issues/696)) ([e6bf9cc](https://github.com/mossipcams/ajax-cli/commit/e6bf9cc9a62324b6849ae1cf7fc5317d47be1651))
* **web:** rebuild dashboard as a roster with a peg rail ([#703](https://github.com/mossipcams/ajax-cli/issues/703)) ([063b991](https://github.com/mossipcams/ajax-cli/commit/063b99170e9c4e5d4e15a5bc385b4f34e34d6609))
* **web:** redesign dashboard as an urgency-ordered decision queue ([#671](https://github.com/mossipcams/ajax-cli/issues/671)) ([6729eed](https://github.com/mossipcams/ajax-cli/commit/6729eed923c1dfe326d851e9294192ee0e8ffba5))
* **web:** replace notify with declarative Web Push ([#761](https://github.com/mossipcams/ajax-cli/issues/761)) ([db099dd](https://github.com/mossipcams/ajax-cli/commit/db099ddb4c22fee08931f6db895fa47b9dab7018))
* **web:** show push status, cancel mic deny, move hotbar delete ([#765](https://github.com/mossipcams/ajax-cli/issues/765)) ([6319175](https://github.com/mossipcams/ajax-cli/commit/6319175bcb0d9ae4ea3fccf92b8cb906609df75b))
* **web:** speed terminal auto-reconnect via stable tmux session ([#692](https://github.com/mossipcams/ajax-cli/issues/692)) ([538de07](https://github.com/mossipcams/ajax-cli/commit/538de07228577e933cdc95e26a0d92904632b560))


### Bug Fixes

* clear 0.56.0 tech-debt P0/P1 findings ([#728](https://github.com/mossipcams/ajax-cli/issues/728)) ([1401ace](https://github.com/mossipcams/ajax-cli/commit/1401acebb74a2dd2354fb4907bcdf926985a3528))
* **cli:** arm Cursor approval waits for attention push ([#768](https://github.com/mossipcams/ajax-cli/issues/768)) ([b6fc488](https://github.com/mossipcams/ajax-cli/commit/b6fc488ca667935b7e054987f8ed3ea2aa52d663))
* **cli:** default ACP host to v1 with AJAX_ACP_V2 opt-in ([#698](https://github.com/mossipcams/ajax-cli/issues/698)) ([26682ac](https://github.com/mossipcams/ajax-cli/commit/26682ac509aff3eb7d9a1200806b37c59b42062b))
* **cli:** make ACP create sessions usable with ready prompt ([#699](https://github.com/mossipcams/ajax-cli/issues/699)) ([249e69c](https://github.com/mossipcams/ajax-cli/commit/249e69cbc46625ed83f1521d3c3e9a50e410417d))
* **core:** drain timed command pipes to avoid Diff Review hangs ([#731](https://github.com/mossipcams/ajax-cli/issues/731)) ([5808479](https://github.com/mossipcams/ajax-cli/commit/5808479bb45a59255bf974a923b5c54d65aa55b4))
* **scripts:** detach Test in Stable from the pane that spawns it ([#673](https://github.com/mossipcams/ajax-cli/issues/673)) ([1de1e27](https://github.com/mossipcams/ajax-cli/commit/1de1e2746f07121d1f9cc6e65f4aab2863957799))
* **scripts:** reinstall agent hooks when definitions change ([#664](https://github.com/mossipcams/ajax-cli/issues/664)) ([ae2cd63](https://github.com/mossipcams/ajax-cli/commit/ae2cd634134174afc7f5de1ce3a891a37045ec66))
* **status:** stop CI evidence masking attention gates and outliving its probe ([#680](https://github.com/mossipcams/ajax-cli/issues/680)) ([e8b3645](https://github.com/mossipcams/ajax-cli/commit/e8b36450a68b29c38ae2ddf3bf1c1fd21d9b3e41))
* **web:** accept unknown task status in cockpit contract ([#682](https://github.com/mossipcams/ajax-cli/issues/682)) ([8b8d9b7](https://github.com/mossipcams/ajax-cli/commit/8b8d9b71fdc8c645b440b24f3808fdd3d94c2e65))
* **web:** allow CSP retirement script ([#710](https://github.com/mossipcams/ajax-cli/issues/710)) ([708804e](https://github.com/mossipcams/ajax-cli/commit/708804e4e794d501fdf11a9f88dc9079fa59beba))
* **web:** allow PostHog US hosts in Cockpit CSP for web vitals ([#760](https://github.com/mossipcams/ajax-cli/issues/760)) ([843cea0](https://github.com/mossipcams/ajax-cli/commit/843cea0164bf1cfd8bbb1fd8a2390b694179dfc3))
* **web:** always npm ci before Test in Stable web build ([#688](https://github.com/mossipcams/ajax-cli/issues/688)) ([4d457ef](https://github.com/mossipcams/ajax-cli/commit/4d457ef3937d8fc20779cf4bef0c34cfeb63109f))
* **web:** always offer Drop on checkout-mismatch tasks ([#717](https://github.com/mossipcams/ajax-cli/issues/717)) ([1eb400f](https://github.com/mossipcams/ajax-cli/commit/1eb400f4207be131ba9c6d4eb608a3f7a0b1710d))
* **web:** block page swipe during pending terminal double-tap select ([#758](https://github.com/mossipcams/ajax-cli/issues/758)) ([322304a](https://github.com/mossipcams/ajax-cli/commit/322304a7ab1770e52984101c8550dc975c8ca22d))
* **web:** clear new-task sheet on task route so swipe-back stays clean ([#769](https://github.com/mossipcams/ajax-cli/issues/769)) ([fd723c5](https://github.com/mossipcams/ajax-cli/commit/fd723c5fe5a7ccba1532d50691d03cb30e1bb985))
* **web:** correct continuous STT lifecycle and restore composer ([#740](https://github.com/mossipcams/ajax-cli/issues/740)) ([f54f146](https://github.com/mossipcams/ajax-cli/commit/f54f146960b80b48ac43f8d9955aa3c186c075ea))
* **web:** cut ghost route_visible, Drop false unmount, and gesture INP ([#773](https://github.com/mossipcams/ajax-cli/issues/773)) ([4feb54d](https://github.com/mossipcams/ajax-cli/commit/4feb54d82c3c0821464c9b1691e96bac4e56084c))
* **web:** disable page swipe during terminal double-tap select ([#756](https://github.com/mossipcams/ajax-cli/issues/756)) ([84f7d8a](https://github.com/mossipcams/ajax-cli/commit/84f7d8af793f7c86534ac5ddad00bf2eb244b298))
* **web:** drop the redundant Open/Answer control from dashboard rows ([#693](https://github.com/mossipcams/ajax-cli/issues/693)) ([30e91df](https://github.com/mossipcams/ajax-cli/commit/30e91df3b6d25badc5b8dc87053f1fe5a1d8271f))
* **web:** finish button transitions and keep Drop on switched task ([#741](https://github.com/mossipcams/ajax-cli/issues/741)) ([6215308](https://github.com/mossipcams/ajax-cli/commit/62153087318511fd1f1312ce07f99b3ca0b11d96))
* **web:** force-reinstall ajax-cli on Test in Stable ([#683](https://github.com/mossipcams/ajax-cli/issues/683)) ([a48ef6e](https://github.com/mossipcams/ajax-cli/commit/a48ef6e626b059ca59c67c2153508f2da5aee887))
* **web:** harden Cockpit server restart, refresh, and CAS recovery ([#766](https://github.com/mossipcams/ajax-cli/issues/766)) ([01ab04c](https://github.com/mossipcams/ajax-cli/commit/01ab04cb30aa13bff01314bfd5313b42c98806ed))
* **web:** harden Diff Review load and task swipe ([#713](https://github.com/mossipcams/ajax-cli/issues/713)) ([d77ec01](https://github.com/mossipcams/ajax-cli/commit/d77ec010651ff934da519647097ffb7ea6e4d92f))
* **web:** harden speech start-over undo and STT ready config ([#737](https://github.com/mossipcams/ajax-cli/issues/737)) ([f48aa81](https://github.com/mossipcams/ajax-cli/commit/f48aa81279f3afe02b64e1f2c421294f6c458f4c))
* **web:** keep Diff open swipe alive across cockpit polls ([#718](https://github.com/mossipcams/ajax-cli/issues/718)) ([4d3c699](https://github.com/mossipcams/ajax-cli/commit/4d3c699a0aca6e2f4df2cd8ab03f844091b8c912))
* **web:** keep health responsive during Diff Review ([#721](https://github.com/mossipcams/ajax-cli/issues/721)) ([0cd735f](https://github.com/mossipcams/ajax-cli/commit/0cd735fa62b3583d4ce58384defb5e184e1501a0))
* **web:** keep health responsive during task create ([#724](https://github.com/mossipcams/ajax-cli/issues/724)) ([81c314f](https://github.com/mossipcams/ajax-cli/commit/81c314fa766345884f01f93fb32bbaaeb4500344))
* **web:** keep link Open/Copy usable when keyboard is closed ([#739](https://github.com/mossipcams/ajax-cli/issues/739)) ([7b31d0e](https://github.com/mossipcams/ajax-cli/commit/7b31d0e92130277619e23040edcfd1a81cc93e78))
* **web:** keep seeded terminal hidden across seed→attach gap ([#732](https://github.com/mossipcams/ajax-cli/issues/732)) ([0d0796e](https://github.com/mossipcams/ajax-cli/commit/0d0796e277d573037012cda6bb55492286dab7cf))
* **web:** keep the terminal put when iOS Backspace reveals the caret ([#679](https://github.com/mossipcams/ajax-cli/issues/679)) ([c61772c](https://github.com/mossipcams/ajax-cli/commit/c61772c72d8fa7f5f2a656d50d7602df8ecd770f))
* **web:** latch scrollOnErase across split CSI erase chunks ([#762](https://github.com/mossipcams/ajax-cli/issues/762)) ([d913987](https://github.com/mossipcams/ajax-cli/commit/d9139873b5bb463cc7a4be10fb1c4ef4df5ab957))
* **web:** latch scrollOnErase to the seed window only ([#749](https://github.com/mossipcams/ajax-cli/issues/749)) ([453ceaf](https://github.com/mossipcams/ajax-cli/commit/453ceaf3f7c009251a2d2123d4e027fcc2591d49))
* **web:** mobile hotbar repeat cadence and iOS keyboard textarea handling ([#675](https://github.com/mossipcams/ajax-cli/issues/675)) ([b16ebd2](https://github.com/mossipcams/ajax-cli/commit/b16ebd28cb28b0e70af71085862207e782252aee))
* **web:** open Diff Review with a plain swipe-right ([#719](https://github.com/mossipcams/ajax-cli/issues/719)) ([ea13fe3](https://github.com/mossipcams/ajax-cli/commit/ea13fe3f11c3ce18b5fc6ab68d85bf3e086f9025))
* **web:** open task terminal at CLI input without scroll animation ([#670](https://github.com/mossipcams/ajax-cli/issues/670)) ([b0d6c08](https://github.com/mossipcams/ajax-cli/commit/b0d6c08179766d2979250300564024609fc76c51))
* **web:** open task terminal at the CLI input without a load scroll ([#672](https://github.com/mossipcams/ajax-cli/issues/672)) ([fb5b1f7](https://github.com/mossipcams/ajax-cli/commit/fb5b1f78bd95c7fae2b1cc9a26e7d2665ac97b78))
* **web:** open terminal link menu from click hit-test ([#689](https://github.com/mossipcams/ajax-cli/issues/689)) ([12c9b28](https://github.com/mossipcams/ajax-cli/commit/12c9b286b92e1e013030b67c53fab7bbc5b61c12))
* **web:** open terminal links without replacing the PWA ([#690](https://github.com/mossipcams/ajax-cli/issues/690)) ([dbd3a3e](https://github.com/mossipcams/ajax-cli/commit/dbd3a3e3d893cc2ab8fca8a93016151613b2fc05))
* **web:** paste rich links into the task terminal ([#725](https://github.com/mossipcams/ajax-cli/issues/725)) ([ec3c581](https://github.com/mossipcams/ajax-cli/commit/ec3c581419c13de5336131c074f07bbb7ca1f8e2))
* **web:** preserve seed scrollback without live ED2 dumps ([#751](https://github.com/mossipcams/ajax-cli/issues/751)) ([8e1caba](https://github.com/mossipcams/ajax-cli/commit/8e1caba3e2a34f010bec1e4d9d21b28edbbf4757))
* **web:** preserve seeded scrollback across attach clear ([#666](https://github.com/mossipcams/ajax-cli/issues/666)) ([7543044](https://github.com/mossipcams/ajax-cli/commit/7543044d447d2a300b13fa5a81f19dd8da84f2ef))
* **web:** raise page-swipe engage dead-zone for iOS PWA ([#767](https://github.com/mossipcams/ajax-cli/issues/767)) ([9c3a676](https://github.com/mossipcams/ajax-cli/commit/9c3a6765050f7d505ab4b61a21a42264a4371fd3))
* **web:** reap detached ephemeral tmux sessions on connect ([#727](https://github.com/mossipcams/ajax-cli/issues/727)) ([4862f42](https://github.com/mossipcams/ajax-cli/commit/4862f42aa1beb455f55cb75da16d679cea792e63))
* **web:** recover terminal paste when clipboardData is empty ([#733](https://github.com/mossipcams/ajax-cli/issues/733)) ([22100f0](https://github.com/mossipcams/ajax-cli/commit/22100f06694aaf895b96178e9261d2bc668c389c))
* **web:** remove speech Insert composer and auto-insert finals ([#742](https://github.com/mossipcams/ajax-cli/issues/742)) ([812cf38](https://github.com/mossipcams/ajax-cli/commit/812cf387c4157d795673d3c1b0513e8c81facde1))
* **web:** restore floating link menu above the terminal ([#735](https://github.com/mossipcams/ajax-cli/issues/735)) ([cfe2032](https://github.com/mossipcams/ajax-cli/commit/cfe203214405570e0a508523b20303b28d80dbf5))
* **web:** restore seeded terminal landing at the CLI input ([#723](https://github.com/mossipcams/ajax-cli/issues/723)) ([b66583a](https://github.com/mossipcams/ajax-cli/commit/b66583a1b9bb20e8193b38ed9c7472cd2fd7649b))
* **web:** restore speech start-over and fail stuck Connecting ([#743](https://github.com/mossipcams/ajax-cli/issues/743)) ([69030f5](https://github.com/mossipcams/ajax-cli/commit/69030f58c9d1eb7fbaa5f88a7509a4af7dd12ee8))
* **web:** restore Test in Stable when restart env is missing ([#771](https://github.com/mossipcams/ajax-cli/issues/771)) ([1691127](https://github.com/mossipcams/ajax-cli/commit/1691127651f1226a2a75a7276ea134e4f36d04a2))
* **web:** run Test in Stable detached from the server's log pipe ([#669](https://github.com/mossipcams/ajax-cli/issues/669)) ([cfe6439](https://github.com/mossipcams/ajax-cli/commit/cfe6439e7802c461ff99c856a6c604b82a08137c))
* **web:** silence redundant success toasts ([#745](https://github.com/mossipcams/ajax-cli/issues/745)) ([1312597](https://github.com/mossipcams/ajax-cli/commit/1312597c39719980a4f5ba440ac8335415a15b05))
* **web:** smooth finish-the-slide Diff swipe transitions ([#722](https://github.com/mossipcams/ajax-cli/issues/722)) ([3d7d236](https://github.com/mossipcams/ajax-cli/commit/3d7d236cedc109d8b90f326efefdb7a25b5f7fee))
* **web:** stop Active/Idle task rows from reshuffling ([#715](https://github.com/mossipcams/ajax-cli/issues/715)) ([484154d](https://github.com/mossipcams/ajax-cli/commit/484154d8b3b4c67c4e4756805a359ecef01fc900))
* **web:** stop clipping Drop confirm pill on dashboard swipe ([#681](https://github.com/mossipcams/ajax-cli/issues/681)) ([0e38ea9](https://github.com/mossipcams/ajax-cli/commit/0e38ea90e060bd555f559f3754469cbb89031e33))
* **web:** stop missing-window attach from spamming scrollback ([#754](https://github.com/mossipcams/ajax-cli/issues/754)) ([2b9905e](https://github.com/mossipcams/ajax-cli/commit/2b9905efc9d4c953d6e255032d07009dcb3e713b))
* **web:** stop swallowing native terminal paste on empty clipboardData ([#730](https://github.com/mossipcams/ajax-cli/issues/730)) ([c80aecf](https://github.com/mossipcams/ajax-cli/commit/c80aecfff281996a7f8f77ad1a5a42e5b37b4131))
* **web:** stop tmux -A attach during terminal reconnect setup ([#694](https://github.com/mossipcams/ajax-cli/issues/694)) ([40b0f28](https://github.com/mossipcams/ajax-cli/commit/40b0f285f4b790fc1fe1b86ff3e9d9ca51755c3b))
* **web:** swipe-left opens Diff; swipe-right goes back ([#720](https://github.com/mossipcams/ajax-cli/issues/720)) ([3dae0e9](https://github.com/mossipcams/ajax-cli/commit/3dae0e90f7de80a2e732662abb6e78bb928ec4dc))
* **web:** wipe invalid web-push subscription files on boot ([#764](https://github.com/mossipcams/ajax-cli/issues/764)) ([1933d9d](https://github.com/mossipcams/ajax-cli/commit/1933d9d0edc523555f39c25107759efd64f5257e))


### Performance Improvements

* **core:** fold task bootstrap into launch and drop graphify ([#772](https://github.com/mossipcams/ajax-cli/issues/772)) ([5ce2234](https://github.com/mossipcams/ajax-cli/commit/5ce2234005145b60ea69cb74182797870df6a5c6))
* cut refresh thrash and mute STT monitor echo ([#750](https://github.com/mossipcams/ajax-cli/issues/750)) ([7588649](https://github.com/mossipcams/ajax-cli/commit/758864971154fc51eba56560404dac201d4d2096))
* **web:** cut cockpit poll and terminal battery drain ([#757](https://github.com/mossipcams/ajax-cli/issues/757)) ([7d95aec](https://github.com/mossipcams/ajax-cli/commit/7d95aec046e23e2ae18af12cfda2f90569dec8d8))


### Code Refactoring

* **core:** split operator slices for agentic architecture ([#753](https://github.com/mossipcams/ajax-cli/issues/753)) ([1bfe491](https://github.com/mossipcams/ajax-cli/commit/1bfe49154328a72d22b3dacdf2b8c920d441c89d))
* **core:** split oversized modules under the LOC gate ([#748](https://github.com/mossipcams/ajax-cli/issues/748)) ([93aa15e](https://github.com/mossipcams/ajax-cli/commit/93aa15ee9b25b5bf9d7d85ec25a20d92e6ca949b))
* **web:** drop dashboard leftovers from today's rebuild ([#702](https://github.com/mossipcams/ajax-cli/issues/702)) ([fa76f13](https://github.com/mossipcams/ajax-cli/commit/fa76f13b2273a9348e0707eb7f71afe2a54ae4f6))
* **web:** split ajax-web modules under the LOC gate ([#747](https://github.com/mossipcams/ajax-cli/issues/747)) ([d1bac47](https://github.com/mossipcams/ajax-cli/commit/d1bac47a3409cbada26dc2a7a0e26d2a82373ec4))


### Reverts

* restore dashboard to one-tap control panel ([#696](https://github.com/mossipcams/ajax-cli/issues/696)) ([#706](https://github.com/mossipcams/ajax-cli/issues/706)) ([8005d6e](https://github.com/mossipcams/ajax-cli/commit/8005d6e4ccc340b02a5595ccc5d988365d893e6f))
* undo today's ACP status and host PRs ([#701](https://github.com/mossipcams/ajax-cli/issues/701)) ([f807ffe](https://github.com/mossipcams/ajax-cli/commit/f807ffea799898be3bbb6e07634c689ca2c42702))
* **web:** restore [#672](https://github.com/mossipcams/ajax-cli/issues/672) Active/Idle dashboard design ([#709](https://github.com/mossipcams/ajax-cli/issues/709)) ([4aec193](https://github.com/mossipcams/ajax-cli/commit/4aec193f3b6f86d49b620139899fb975608798d3))
* **web:** restore dashboard to TaskList state at [#694](https://github.com/mossipcams/ajax-cli/issues/694) ([#707](https://github.com/mossipcams/ajax-cli/issues/707)) ([b83f422](https://github.com/mossipcams/ajax-cli/commit/b83f422ec05b9e5724894f8fcb8bf99ceeb2d730))
* **web:** restore MusterBar dashboard; keep terminal links ([#708](https://github.com/mossipcams/ajax-cli/issues/708)) ([ee8dd45](https://github.com/mossipcams/ajax-cli/commit/ee8dd45a46a36ea808686d43e98731ce1e00a2f1))

## [0.55.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.55.0...ajax-cli-v0.55.1) (2026-07-22)


### Bug Fixes

* **web:** revert zero-lag overlay and restore scrollback depth ([#662](https://github.com/mossipcams/ajax-cli/issues/662)) ([d714330](https://github.com/mossipcams/ajax-cli/commit/d71433084dbf95b6ab15dd94f1e9eab6ba91c0a6))

## [0.55.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.54.4...ajax-cli-v0.55.0) (2026-07-22)


### Features

* **core:** add structured operator logging to logs_dir ([#658](https://github.com/mossipcams/ajax-cli/issues/658)) ([c1aa66a](https://github.com/mossipcams/ajax-cli/commit/c1aa66a0c952894f869afdaf91c3a54d70af710f))
* **web:** add xterm zero-lag typed-echo overlay ([#661](https://github.com/mossipcams/ajax-cli/issues/661)) ([4d2cdc1](https://github.com/mossipcams/ajax-cli/commit/4d2cdc143a07c07d58da3a4a723ca8e4ded4b351))


### Bug Fixes

* **cli:** discover Cursor identity under XDG ~/.cache/ajax ([#656](https://github.com/mossipcams/ajax-cli/issues/656)) ([5dbcaa0](https://github.com/mossipcams/ajax-cli/commit/5dbcaa0d9d240dc47825907a4e27cfc66d9fed89))
* **core:** clear sticky CI failed on pending checks ([#660](https://github.com/mossipcams/ajax-cli/issues/660)) ([1262caf](https://github.com/mossipcams/ajax-cli/commit/1262cafb288e79071b8b5d76ad1017a9c278dd5e))
* **web:** settle terminal resize before history seed pad ([#657](https://github.com/mossipcams/ajax-cli/issues/657)) ([7023815](https://github.com/mossipcams/ajax-cli/commit/7023815be116d0fb12e8df26ca7230337065faba))

## [0.54.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.54.3...ajax-cli-v0.54.4) (2026-07-22)


### Bug Fixes

* **cli:** resolve Cursor hook identity via cwd-index ([#654](https://github.com/mossipcams/ajax-cli/issues/654)) ([b193e9e](https://github.com/mossipcams/ajax-cli/commit/b193e9e51c2e530c56d4414617f5855a720d4054))

## [0.54.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.54.2...ajax-cli-v0.54.3) (2026-07-22)


### Bug Fixes

* **core:** treat Cursor/Pi as first-class agents for status hooks ([#653](https://github.com/mossipcams/ajax-cli/issues/653)) ([fd000b2](https://github.com/mossipcams/ajax-cli/commit/fd000b26aa0ca4bac46d81743befe09c2d5810fa))
* **web:** keep Drop confirm pill readable and danger-colored ([#651](https://github.com/mossipcams/ajax-cli/issues/651)) ([236c157](https://github.com/mossipcams/ajax-cli/commit/236c15758d138d97df9cb1fff3d8bb3e91555b99))

## [0.54.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.54.1...ajax-cli-v0.54.2) (2026-07-22)


### Bug Fixes

* **core:** debounce attention webhooks and allowlist wait/ask ([#650](https://github.com/mossipcams/ajax-cli/issues/650)) ([12d7487](https://github.com/mossipcams/ajax-cli/commit/12d74873cced7e842c6cc015c3f62fd11ee45232))
* **web:** keep seeded terminal history above attach clear ([#648](https://github.com/mossipcams/ajax-cli/issues/648)) ([e5acc71](https://github.com/mossipcams/ajax-cli/commit/e5acc71bb7f774358d52f7087e3812524ae819b6))

## [0.54.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.54.0...ajax-cli-v0.54.1) (2026-07-21)


### Bug Fixes

* **web:** hug bottom nav to the screen edge and stop scrollback drift ([#646](https://github.com/mossipcams/ajax-cli/issues/646)) ([286584c](https://github.com/mossipcams/ajax-cli/commit/286584ca90ab2b7281e1ac39af0e0974ea14c42a))

## [0.54.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.53.1...ajax-cli-v0.54.0) (2026-07-21)


### Features

* **core:** canonical agent-event facts pipeline ([#644](https://github.com/mossipcams/ajax-cli/issues/644)) ([b9175f3](https://github.com/mossipcams/ajax-cli/commit/b9175f337a8daa2e5af8f85076152fc2979b4d15))

## [0.53.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.53.0...ajax-cli-v0.53.1) (2026-07-21)


### Bug Fixes

* **core:** fully tear down drops and GC orphan ajax worktrees ([#642](https://github.com/mossipcams/ajax-cli/issues/642)) ([e0a0629](https://github.com/mossipcams/ajax-cli/commit/e0a0629788477a1531d4cb8ba5c8e71add60e583))

## [0.53.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.52.3...ajax-cli-v0.53.0) (2026-07-21)


### Features

* rewire attention webhooks onto lifecycle hooks ([#641](https://github.com/mossipcams/ajax-cli/issues/641)) ([ed26c9e](https://github.com/mossipcams/ajax-cli/commit/ed26c9e500d2877cf91c47fca93052fcafd3b03e))


### Bug Fixes

* **web:** keep hotbar Paste label inside equal-flex keys ([#639](https://github.com/mossipcams/ajax-cli/issues/639)) ([ab317b8](https://github.com/mossipcams/ajax-cli/commit/ab317b85317ff3722e00c495f3ea01f34f4fdfe0))

## [0.52.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.52.2...ajax-cli-v0.52.3) (2026-07-21)


### Bug Fixes

* **web:** keep task chrome visible and Drop after navigate-back ([#637](https://github.com/mossipcams/ajax-cli/issues/637)) ([a59e3f1](https://github.com/mossipcams/ajax-cli/commit/a59e3f19e4a2c8a2d1709e94d2be94366284922e))

## [0.52.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.52.1...ajax-cli-v0.52.2) (2026-07-21)


### Bug Fixes

* **web:** drop shell asset ?v= cache busting ([#635](https://github.com/mossipcams/ajax-cli/issues/635)) ([f92a432](https://github.com/mossipcams/ajax-cli/commit/f92a43266d4ac0cae863d87cb7cce4cb52b8d38f))

## [0.52.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.52.0...ajax-cli-v0.52.1) (2026-07-21)


### Bug Fixes

* **web:** bust Cloudflare-stale shell assets without rewriting the module graph ([#633](https://github.com/mossipcams/ajax-cli/issues/633)) ([611f0fd](https://github.com/mossipcams/ajax-cli/commit/611f0fdde8dce3d9caa27757ed5b6956d6daf431))

## [0.52.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.8...ajax-cli-v0.52.0) (2026-07-21)


### Features

* add per-task bootstrap for Node 22, husky, and router symlinks ([#632](https://github.com/mossipcams/ajax-cli/issues/632)) ([b8e9ae6](https://github.com/mossipcams/ajax-cli/commit/b8e9ae61bb2f7d0a90243c77c605649c54672516))
* native agent lifecycle events replace pane-text status heuristics ([#629](https://github.com/mossipcams/ajax-cli/issues/629)) ([3199734](https://github.com/mossipcams/ajax-cli/commit/3199734ac0837b56b018b2410b7dc1d6ae0326e9))


### Bug Fixes

* **scripts:** restore ajax-model-router symlinks after main sync ([#627](https://github.com/mossipcams/ajax-cli/issues/627)) ([fbc308f](https://github.com/mossipcams/ajax-cli/commit/fbc308ff73190ed23bdc60850d33335e92a60006))
* **web:** stop shell asset caching and prefer Resume primary ([#631](https://github.com/mossipcams/ajax-cli/issues/631)) ([79a1a7d](https://github.com/mossipcams/ajax-cli/commit/79a1a7ddd6933bb3212963538f9635d00d79ab20))

## [0.51.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.7...ajax-cli-v0.51.8) (2026-07-21)


### Bug Fixes

* **core:** eliminate status false positives from pane keyword classification ([#626](https://github.com/mossipcams/ajax-cli/issues/626)) ([093be4d](https://github.com/mossipcams/ajax-cli/commit/093be4d0dfb9406752ed008ff12cddb03294cae2))
* **web:** keep Task details meta grid inside the phone width ([#623](https://github.com/mossipcams/ajax-cli/issues/623)) ([4dd660a](https://github.com/mossipcams/ajax-cli/commit/4dd660a99cbb738246b68da84ab15828bed7ef56))


### Performance Improvements

* **web:** revalidate shell assets with a weak ETag instead of refetching ([#625](https://github.com/mossipcams/ajax-cli/issues/625)) ([cec2045](https://github.com/mossipcams/ajax-cli/commit/cec2045875c0a4aea880de0196a68a7841436ccd))

## [0.51.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.6...ajax-cli-v0.51.7) (2026-07-21)


### Bug Fixes

* **web:** keep cockpit reads responsive and recover failed iOS PWA launches ([#621](https://github.com/mossipcams/ajax-cli/issues/621)) ([99f7d78](https://github.com/mossipcams/ajax-cli/commit/99f7d78bb33b0f5b67a5cc9a07ffecef64d614e8))

## [0.51.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.5...ajax-cli-v0.51.6) (2026-07-21)


### Bug Fixes

* **web:** keep open Task details scrollable on mobile WebKit ([#618](https://github.com/mossipcams/ajax-cli/issues/618)) ([18aa873](https://github.com/mossipcams/ajax-cli/commit/18aa87387da70da3d6b750df4960e5332a26a4a8))
* **web:** roll back fragile shell asset cache busting ([#620](https://github.com/mossipcams/ajax-cli/issues/620)) ([1e52ea8](https://github.com/mossipcams/ajax-cli/commit/1e52ea82c4d3c187cc9126732dce9857fbb810ea))

## [0.51.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.4...ajax-cli-v0.51.5) (2026-07-20)


### Bug Fixes

* **web:** bound cockpit GETs with a 10s timeout so a hung fetch cannot stall PWA startup ([#616](https://github.com/mossipcams/ajax-cli/issues/616)) ([6de8847](https://github.com/mossipcams/ajax-cli/commit/6de8847c2c4188bffe88fabaff6de27c0684b445))

## [0.51.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.3...ajax-cli-v0.51.4) (2026-07-20)


### Bug Fixes

* **core:** block start when worktree path or branch is occupied ([#611](https://github.com/mossipcams/ajax-cli/issues/611)) ([582d4ae](https://github.com/mossipcams/ajax-cli/commit/582d4ae138a01b1579de7c49c6c1af4d553b281e))
* **core:** separate worktree presence from checkout state ([#613](https://github.com/mossipcams/ajax-cli/issues/613)) ([113244c](https://github.com/mossipcams/ajax-cli/commit/113244c3845bdfd28a75031e653153540c3f8959))
* **core:** stop false and redundant attention webhook spam ([#615](https://github.com/mossipcams/ajax-cli/issues/615)) ([de1cc44](https://github.com/mossipcams/ajax-cli/commit/de1cc4409b8919e9ffa1142ea20e0863443051f0))


### Code Refactoring

* **web:** polish Task details dropdown density and typography ([#614](https://github.com/mossipcams/ajax-cli/issues/614)) ([c2779a6](https://github.com/mossipcams/ajax-cli/commit/c2779a6c988c89aa562f005f00487ad2b0a5a89d))

## [0.51.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.2...ajax-cli-v0.51.3) (2026-07-20)


### Bug Fixes

* **web:** version sibling chunk imports so the app loads once ([#609](https://github.com/mossipcams/ajax-cli/issues/609)) ([ebe5d48](https://github.com/mossipcams/ajax-cli/commit/ebe5d4810fa13db191a8c6b32af1b857b9c48fe6))

## [0.51.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.1...ajax-cli-v0.51.2) (2026-07-20)


### Bug Fixes

* **web:** recover the PWA connection on load ([#608](https://github.com/mossipcams/ajax-cli/issues/608)) ([f2ec2c4](https://github.com/mossipcams/ajax-cli/commit/f2ec2c4844f4ce7dc5ae2b6373c893f1f327e345))


### Code Refactoring

* **web:** extract TaskMetaDetails from task details dropdown ([#606](https://github.com/mossipcams/ajax-cli/issues/606)) ([d19687c](https://github.com/mossipcams/ajax-cli/commit/d19687c99845dc2841f73af6f38a352e5eba12d0))

## [0.51.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.51.0...ajax-cli-v0.51.1) (2026-07-20)


### Bug Fixes

* **web:** keep PWA first paint dark and harden restart probe ([#604](https://github.com/mossipcams/ajax-cli/issues/604)) ([8b42360](https://github.com/mossipcams/ajax-cli/commit/8b42360eb7088019c47d59247456aa1e6b1da799))

## [0.51.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.14...ajax-cli-v0.51.0) (2026-07-20)


### Features

* **web:** skip permission prompts on agent launch, swap OpenCode for Pi ([#600](https://github.com/mossipcams/ajax-cli/issues/600)) ([83ef47c](https://github.com/mossipcams/ajax-cli/commit/83ef47c2df50069e2bd94ae97b66e521df2ab2e6))

## [0.50.14](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.13...ajax-cli-v0.50.14) (2026-07-20)


### Bug Fixes

* **core:** restore pane stuck statuses and align notifications ([#598](https://github.com/mossipcams/ajax-cli/issues/598)) ([d735dfb](https://github.com/mossipcams/ajax-cli/commit/d735dfb2252cf7be38b9e5f972d17f84129428b5))

## [0.50.13](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.12...ajax-cli-v0.50.13) (2026-07-20)


### Bug Fixes

* **core:** block repair for occupied worktree path ([#597](https://github.com/mossipcams/ajax-cli/issues/597)) ([bf4ce14](https://github.com/mossipcams/ajax-cli/commit/bf4ce14c0460ab8561528dd39a4217882e9a2f7e))


### Performance Improvements

* **web:** gzip and long-cache version-busted shell assets ([#595](https://github.com/mossipcams/ajax-cli/issues/595)) ([5902dab](https://github.com/mossipcams/ajax-cli/commit/5902dab6d56499dc2e9aa5d53e555182f53b96b4))

## [0.50.12](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.11...ajax-cli-v0.50.12) (2026-07-20)


### Bug Fixes

* **web:** rebuild dist so Open Dev removal ships ([#593](https://github.com/mossipcams/ajax-cli/issues/593)) ([f2c9a07](https://github.com/mossipcams/ajax-cli/commit/f2c9a070b84fa3ee785742cb254830c9855135d7))

## [0.50.11](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.10...ajax-cli-v0.50.11) (2026-07-19)


### Bug Fixes

* **core:** stop false AgentRunning from process liveness alone ([#591](https://github.com/mossipcams/ajax-cli/issues/591)) ([89e61f3](https://github.com/mossipcams/ajax-cli/commit/89e61f3cabccff913bcb29d8d71ab1d97a199f1e))

## [0.50.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.9...ajax-cli-v0.50.10) (2026-07-19)


### Bug Fixes

* **web:** nest Test in Dev, drop Open Dev, harden connection banner ([#589](https://github.com/mossipcams/ajax-cli/issues/589)) ([c3d86b6](https://github.com/mossipcams/ajax-cli/commit/c3d86b6ccfe60de4315c6d987018320cd10eeaae))

## [0.50.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.8...ajax-cli-v0.50.9) (2026-07-19)


### Code Refactoring

* **web:** React migration cleanup — tooling, lifecycle correctness, and shadcn foundation ([#587](https://github.com/mossipcams/ajax-cli/issues/587)) ([e733e53](https://github.com/mossipcams/ajax-cli/commit/e733e538623bd42ebc5ac90a6668776c8d297211))

## [0.50.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.7...ajax-cli-v0.50.8) (2026-07-18)


### Code Refactoring

* **web:** invert shell to React and remove Svelte — migration complete (react S7) ([#585](https://github.com/mossipcams/ajax-cli/issues/585)) ([e5c593e](https://github.com/mossipcams/ajax-cli/commit/e5c593ed41ba03d89186e5802d02ef9dddae9704))

## [0.50.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.6...ajax-cli-v0.50.7) (2026-07-18)


### Code Refactoring

* **web:** migrate TaskDetail + TestInDevPanel to React islands (react S6) ([#583](https://github.com/mossipcams/ajax-cli/issues/583)) ([dd0af7e](https://github.com/mossipcams/ajax-cli/commit/dd0af7e7a5c51026efae24f14077bdd55e700825))

## [0.50.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.5...ajax-cli-v0.50.6) (2026-07-18)


### Bug Fixes

* **web:** surface Test in Dev on the detail page as name-only pills ([#581](https://github.com/mossipcams/ajax-cli/issues/581)) ([2bc8e20](https://github.com/mossipcams/ajax-cli/commit/2bc8e20ca830ffa04a3e632b5b0a46d5aeb61b96))

## [0.50.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.4...ajax-cli-v0.50.5) (2026-07-17)


### Bug Fixes

* **web:** move Test in Dev under Task details and compact it ([#580](https://github.com/mossipcams/ajax-cli/issues/580)) ([a2015b2](https://github.com/mossipcams/ajax-cli/commit/a2015b23821504f53d7c5ed37036a7668b07118d))


### Code Refactoring

* **web:** migrate TaskTerminal to React island (react S5) ([#578](https://github.com/mossipcams/ajax-cli/issues/578)) ([7e53844](https://github.com/mossipcams/ajax-cli/commit/7e53844138dc87d097291a626c2f26c5b7399f4b))

## [0.50.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.3...ajax-cli-v0.50.4) (2026-07-17)


### Code Refactoring

* **web:** migrate NewTaskSheet and FullscreenLayer to React (react S4) ([#577](https://github.com/mossipcams/ajax-cli/issues/577)) ([4672466](https://github.com/mossipcams/ajax-cli/commit/4672466db08bd97e98d869988dfee68cec483324))
* **web:** migrate SettingsView and ResultPanel to React islands (react S3) ([#575](https://github.com/mossipcams/ajax-cli/issues/575)) ([5b14879](https://github.com/mossipcams/ajax-cli/commit/5b1487945e584ebce4fd1ba656263005123d0429))

## [0.50.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.2...ajax-cli-v0.50.3) (2026-07-17)


### Code Refactoring

* **web:** migrate TaskList and ActionBar to React islands (react S2) ([#573](https://github.com/mossipcams/ajax-cli/issues/573)) ([2e5322a](https://github.com/mossipcams/ajax-cli/commit/2e5322a5850f3aef7c5ff9b2261f910dbba2a7a3))

## [0.50.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.1...ajax-cli-v0.50.2) (2026-07-17)


### Code Refactoring

* **web:** React island seam with ConnectionStatus and Skeleton (react S1) ([#571](https://github.com/mossipcams/ajax-cli/issues/571)) ([960b9af](https://github.com/mossipcams/ajax-cli/commit/960b9af2ff8c3c96c802bbbbff8b72edb5274714))

## [0.50.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.50.0...ajax-cli-v0.50.1) (2026-07-17)


### Bug Fixes

* **web:** clear fullscreen terminal expand button below the notch ([#569](https://github.com/mossipcams/ajax-cli/issues/569)) ([0e61c99](https://github.com/mossipcams/ajax-cli/commit/0e61c9954e3c080b1945fe7fd8c643206e634317))
* **web:** don't inherit ambient GIT_DIR in dev-deploy git probes ([#568](https://github.com/mossipcams/ajax-cli/issues/568)) ([b633ff3](https://github.com/mossipcams/ajax-cli/commit/b633ff3a646591c9b06f542c9a2d7299156030b2))

## [0.50.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.49.0...ajax-cli-v0.50.0) (2026-07-17)


### Features

* acknowledge web terminal input and surface GitHub CI in task status ([#565](https://github.com/mossipcams/ajax-cli/issues/565)) ([e63ef9f](https://github.com/mossipcams/ajax-cli/commit/e63ef9f2a43e26244682d48b8b1a7e5d9823e92f))
* **web:** add shared Test in Dev deployment for ajax-cli tasks ([#567](https://github.com/mossipcams/ajax-cli/issues/567)) ([ccfe21f](https://github.com/mossipcams/ajax-cli/commit/ccfe21f96b80ca5c023eca52e27195649b0e5561))

## [0.49.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.48.0...ajax-cli-v0.49.0) (2026-07-17)


### Features

* **web:** polish Cockpit alignment and lock DESIGN.md colors ([#563](https://github.com/mossipcams/ajax-cli/issues/563)) ([5ae780f](https://github.com/mossipcams/ajax-cli/commit/5ae780f773934f628e4c1f1dc2b20c70186879f4))

## [0.48.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.10...ajax-cli-v0.48.0) (2026-07-17)


### Features

* **web:** typeset Cockpit ramp and harden Drop undo ([#561](https://github.com/mossipcams/ajax-cli/issues/561)) ([575aef6](https://github.com/mossipcams/ajax-cli/commit/575aef631e1bf66ca123bc268f6093a3bf0ab6af))

## [0.47.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.9...ajax-cli-v0.47.10) (2026-07-17)


### Bug Fixes

* **web:** ship rebuilt dist for fullscreen hotbar keyboard gap ([#559](https://github.com/mossipcams/ajax-cli/issues/559)) ([cc527cd](https://github.com/mossipcams/ajax-cli/commit/cc527cd40e689f772db7b05e4562f313fe036e48))

## [0.47.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.8...ajax-cli-v0.47.9) (2026-07-16)


### Bug Fixes

* **web:** drop fullscreen safe-area hotbar pad while the keyboard is open ([#557](https://github.com/mossipcams/ajax-cli/issues/557)) ([7fe16a5](https://github.com/mossipcams/ajax-cli/commit/7fe16a54a49170cce71d7d2f127c8a0125a02d11))

## [0.47.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.7...ajax-cli-v0.47.8) (2026-07-16)


### Bug Fixes

* **web:** revert terminal back hold repeat ([#554](https://github.com/mossipcams/ajax-cli/issues/554)) ([bc1bb4a](https://github.com/mossipcams/ajax-cli/commit/bc1bb4a572da1589de0171e70dc0a4cab8c88f79))
* **web:** serialize cockpit mutations on a control lane and extract terminal geometry/refit ([#555](https://github.com/mossipcams/ajax-cli/issues/555)) ([1885e4b](https://github.com/mossipcams/ajax-cli/commit/1885e4b171168454d59722b6f61b4dcce52bff54))

## [0.47.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.6...ajax-cli-v0.47.7) (2026-07-16)


### Bug Fixes

* **web:** stabilize terminal Back and Space input ([#551](https://github.com/mossipcams/ajax-cli/issues/551)) ([f313bf6](https://github.com/mossipcams/ajax-cli/commit/f313bf6f1591996ad4eac441a60905f6e829d32a))

## [0.47.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.5...ajax-cli-v0.47.6) (2026-07-16)


### Bug Fixes

* **web:** rebuild terminal browser bundle ([#548](https://github.com/mossipcams/ajax-cli/issues/548)) ([f9bcbdd](https://github.com/mossipcams/ajax-cli/commit/f9bcbddbc5f4c84dbcd0dde2d0e309174a2196ca))

## [0.47.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.4...ajax-cli-v0.47.5) (2026-07-16)


### Bug Fixes

* **web:** repair terminal history and touch controls ([#546](https://github.com/mossipcams/ajax-cli/issues/546)) ([49c447d](https://github.com/mossipcams/ajax-cli/commit/49c447dcbb8569e49a9a943f3c0b3a915a139289))

## [0.47.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.3...ajax-cli-v0.47.4) (2026-07-16)


### Bug Fixes

* **web:** flush details line under hotbar and stop keyboard-transient layout teardown ([#544](https://github.com/mossipcams/ajax-cli/issues/544)) ([1b276af](https://github.com/mossipcams/ajax-cli/commit/1b276afbab7f566cd0cb6f5ad498cb5dc9f79147))

## [0.47.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.2...ajax-cli-v0.47.3) (2026-07-16)


### Bug Fixes

* **web:** suppress attention webhooks while cockpit is connected ([#542](https://github.com/mossipcams/ajax-cli/issues/542)) ([6ad2a67](https://github.com/mossipcams/ajax-cli/commit/6ad2a67df3f7aa8ba2b4297601471502b6b0161e))

## [0.47.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.1...ajax-cli-v0.47.2) (2026-07-16)


### Bug Fixes

* **web:** flex-fill inline mobile terminal so details line sits at page bottom ([#540](https://github.com/mossipcams/ajax-cli/issues/540)) ([c615cbe](https://github.com/mossipcams/ajax-cli/commit/c615cbe5ba0c43cbc410ee672a158c92b836c8aa))

## [0.47.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.47.0...ajax-cli-v0.47.1) (2026-07-16)


### Reverts

* **web:** restore mobile terminal cap after overly-tight height ([#537](https://github.com/mossipcams/ajax-cli/issues/537)) ([2a5eb05](https://github.com/mossipcams/ajax-cli/commit/2a5eb05cb5ed9ddce80241693b36f82d3b3988d3))

## [0.47.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.9...ajax-cli-v0.47.0) (2026-07-16)


### Features

* **web:** adopt CLI cockpit palette and redesign task page ([#534](https://github.com/mossipcams/ajax-cli/issues/534)) ([0e3fc79](https://github.com/mossipcams/ajax-cli/commit/0e3fc792726b99ed28da0d7d0540424bbd98bc0f))


### Bug Fixes

* **web:** tighten mobile terminal cap for near-flush PTY rows ([#533](https://github.com/mossipcams/ajax-cli/issues/533)) ([5da1948](https://github.com/mossipcams/ajax-cli/commit/5da194899c098bb4a2cd52c74699ef0c867cc454))

## [0.46.9](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.8...ajax-cli-v0.46.9) (2026-07-16)


### Bug Fixes

* **web:** cap mobile terminal height to shrink empty PTY band ([#531](https://github.com/mossipcams/ajax-cli/issues/531)) ([887d7ec](https://github.com/mossipcams/ajax-cli/commit/887d7ec31991d2a092487d707cb97dbf02021157))

## [0.46.8](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.7...ajax-cli-v0.46.8) (2026-07-16)


### Bug Fixes

* **web:** fill mobile terminal host and equalize hotbar keys ([#529](https://github.com/mossipcams/ajax-cli/issues/529)) ([2c2d3c2](https://github.com/mossipcams/ajax-cli/commit/2c2d3c2e2b163ec58778cc0092d6fccd191d5b4a))

## [0.46.7](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.6...ajax-cli-v0.46.7) (2026-07-16)


### Bug Fixes

* **web:** flex-fill mobile terminal and proportion hotbar keys ([#527](https://github.com/mossipcams/ajax-cli/issues/527)) ([2c14753](https://github.com/mossipcams/ajax-cli/commit/2c14753dacbcca988128505105223e78cca09d79))

## [0.46.6](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.5...ajax-cli-v0.46.6) (2026-07-16)


### Bug Fixes

* **web:** restore height-based keyboard band pin for iOS flush ([#525](https://github.com/mossipcams/ajax-cli/issues/525)) ([1c510ea](https://github.com/mossipcams/ajax-cli/commit/1c510eab968297240d5f537b9527ec41f549850d))

## [0.46.5](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.4...ajax-cli-v0.46.5) (2026-07-16)


### Bug Fixes

* **web:** unify keyboard-band settle and pin Copy beside expand ([#522](https://github.com/mossipcams/ajax-cli/issues/522)) ([572714f](https://github.com/mossipcams/ajax-cli/commit/572714fdc141dc006c4647c41d2447cfe996bdce))

## [0.46.4](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.3...ajax-cli-v0.46.4) (2026-07-16)


### Bug Fixes

* **web:** stop nested fixed task-detail from offsetting fullscreen under keyboard ([#520](https://github.com/mossipcams/ajax-cli/issues/520)) ([86a9a31](https://github.com/mossipcams/ajax-cli/commit/86a9a315576d6bf13b3f7f0f6b64136d96f08058))

## [0.46.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.2...ajax-cli-v0.46.3) (2026-07-15)


### Bug Fixes

* **web:** port xterm iOS keyboard textarea anchor and expand settle ([#518](https://github.com/mossipcams/ajax-cli/issues/518)) ([a5d2715](https://github.com/mossipcams/ajax-cli/commit/a5d2715f9b84c328f2220d37991934e80c1b4d2b))

## [0.46.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.1...ajax-cli-v0.46.2) (2026-07-15)


### Bug Fixes

* **web:** keep task chrome above keyboard and full-bleed terminal ([#515](https://github.com/mossipcams/ajax-cli/issues/515)) ([8452c2a](https://github.com/mossipcams/ajax-cli/commit/8452c2aa7e6704114967cf87600fa0c89b9c8037))

## [0.46.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.46.0...ajax-cli-v0.46.1) (2026-07-15)


### Bug Fixes

* **web:** restore iOS xterm fullscreen, keyboard chrome, and copy ([#513](https://github.com/mossipcams/ajax-cli/issues/513)) ([645b498](https://github.com/mossipcams/ajax-cli/commit/645b49837d7c162e13ff5073e90bebfdb91e1a20))

## [0.46.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.45.3...ajax-cli-v0.46.0) (2026-07-15)


### Features

* **web:** implement xterm task terminal ([#512](https://github.com/mossipcams/ajax-cli/issues/512)) ([24b0ba1](https://github.com/mossipcams/ajax-cli/commit/24b0ba1f4594bd861b137a4cf1b65cfa4e0c6a9e))


### Code Refactoring

* **web:** capture terminal behavior and remove legacy surfaces ([#510](https://github.com/mossipcams/ajax-cli/issues/510)) ([6bbef9c](https://github.com/mossipcams/ajax-cli/commit/6bbef9c10715910a0c5aa7f8ab1b4dd806057632))

## [0.45.3](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.45.2...ajax-cli-v0.45.3) (2026-07-15)


### Bug Fixes

* **web:** seed terminal history at client width and keep scrollback on auto-reconnect ([#506](https://github.com/mossipcams/ajax-cli/issues/506)) ([e4c6bc3](https://github.com/mossipcams/ajax-cli/commit/e4c6bc33149cf61a8ea25a7083d4d24b2fa316ac))

## [0.45.2](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.45.1...ajax-cli-v0.45.2) (2026-07-15)


### Bug Fixes

* **web:** smooth Ghostty terminal scrolling via native scroll proxy ([#504](https://github.com/mossipcams/ajax-cli/issues/504)) ([046ef5b](https://github.com/mossipcams/ajax-cli/commit/046ef5bb829e01368b6a7cbbd0cf2c01ba3e2ac0))

## [0.45.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.45.0...ajax-cli-v0.45.1) (2026-07-15)


### Bug Fixes

* **web:** run dev-web-restart script from Reload app ([#501](https://github.com/mossipcams/ajax-cli/issues/501)) ([7c4d25c](https://github.com/mossipcams/ajax-cli/commit/7c4d25ce99e88244eff051da10e82f0959e3bef4))
* **web:** stop xterm Surface V2 dual background and resize spam ([#502](https://github.com/mossipcams/ajax-cli/issues/502)) ([73fc9b1](https://github.com/mossipcams/ajax-cli/commit/73fc9b1d3ff67f7e86b18104ca007aad795ffb11))

## [0.45.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.44.1...ajax-cli-v0.45.0) (2026-07-15)


### Features

* **web:** replace wterm Surface V2 with xterm.js spike ([#499](https://github.com/mossipcams/ajax-cli/issues/499)) ([a765c36](https://github.com/mossipcams/ajax-cli/commit/a765c368555b1ab4cba67eb746192790812d6ce4))

## [0.44.1](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.44.0...ajax-cli-v0.44.1) (2026-07-15)


### Bug Fixes

* **web:** normalize captured terminal history line endings ([#497](https://github.com/mossipcams/ajax-cli/issues/497)) ([a1d4cdd](https://github.com/mossipcams/ajax-cli/commit/a1d4cdda5899f1650cc685237f733c4b59739f29))

## [0.44.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.43.0...ajax-cli-v0.44.0) (2026-07-15)


### Features

* add authenticated web task terminal bridge ([#234](https://github.com/mossipcams/ajax-cli/issues/234)) ([bde33d8](https://github.com/mossipcams/ajax-cli/commit/bde33d8952f37707cc8a3c7608cf6b1817937dda))
* add Ctrl+T shortcut to open create-task from cockpit and task session ([#119](https://github.com/mossipcams/ajax-cli/issues/119)) ([94fda3c](https://github.com/mossipcams/ajax-cli/commit/94fda3c64e02f09185c049cc3e8bec9bfc8b229b))
* adopt agent-deck-inspired status derivation ([#156](https://github.com/mossipcams/ajax-cli/issues/156)) ([20d62ff](https://github.com/mossipcams/ajax-cli/commit/20d62ff23cf46c8e9f1d52c557f428990abb3843))
* align task status lifecycle across cockpit surfaces ([#158](https://github.com/mossipcams/ajax-cli/issues/158)) ([7f04508](https://github.com/mossipcams/ajax-cli/commit/7f04508116909d55e73529cbc3c48d35171aa006))
* confirm waiting status before notifying and poll in background ([#427](https://github.com/mossipcams/ajax-cli/issues/427)) ([071bcb0](https://github.com/mossipcams/ajax-cli/commit/071bcb0219fad71848b46ef0a1c30e4b7cedea22))
* enable web cockpit resume and free-form task input ([#232](https://github.com/mossipcams/ajax-cli/issues/232)) ([b01045b](https://github.com/mossipcams/ajax-cli/commit/b01045b8e68620691a61bedb9c84158ec90ca9d3))
* improve Cursor supervisor live status monitoring ([#97](https://github.com/mossipcams/ajax-cli/issues/97)) ([808483d](https://github.com/mossipcams/ajax-cli/commit/808483d746376dd4d49e6fbafee14df8b932b81d))
* introduce Ajax task window substrate ([#303](https://github.com/mossipcams/ajax-cli/issues/303)) ([dd65f37](https://github.com/mossipcams/ajax-cli/commit/dd65f374fe20d5a7bd7d30c903181af9bc00678c))
* make web cockpit Safari-first ([#134](https://github.com/mossipcams/ajax-cli/issues/134)) ([8e4e89a](https://github.com/mossipcams/ajax-cli/commit/8e4e89a911d5ef78bc76fc727390da506eb2efbd))
* migrate web terminal to ghostty ([#289](https://github.com/mossipcams/ajax-cli/issues/289)) ([84198ae](https://github.com/mossipcams/ajax-cli/commit/84198ae560e0cf182fe1a1c8fe8cb7b50d600c96))
* normalize sqlite registry schema to v8 ([#179](https://github.com/mossipcams/ajax-cli/issues/179)) ([bf1b602](https://github.com/mossipcams/ajax-cli/commit/bf1b6026402af8f758d35a23cf298e41f63e6177))
* overhaul web task terminal for mobile and remove pane fallback ([#238](https://github.com/mossipcams/ajax-cli/issues/238)) ([f517ec7](https://github.com/mossipcams/ajax-cli/commit/f517ec75e663995079eaef2d61a9d2dbcad1cb99))
* release task window migration cleanup ([#305](https://github.com/mossipcams/ajax-cli/issues/305)) ([159536d](https://github.com/mossipcams/ajax-cli/commit/159536d334c569097fba0490fa1f6a019f100343))
* send webhook notifications when tasks need attention ([#419](https://github.com/mossipcams/ajax-cli/issues/419)) ([b565a45](https://github.com/mossipcams/ajax-cli/commit/b565a45d91de1b9e9b33b7b58f3711bc8ab96db0))
* speed up post-startup cockpit polling and web reliability ([#140](https://github.com/mossipcams/ajax-cli/issues/140)) ([c5353c3](https://github.com/mossipcams/ajax-cli/commit/c5353c3af1fa380921803ce523efd56a2c6f3d7d))
* sync default branch and optional graphify before task worktrees ([#129](https://github.com/mossipcams/ajax-cli/issues/129)) ([351cf4d](https://github.com/mossipcams/ajax-cli/commit/351cf4d764e85df3ed1c91c92285f865e468b8b3))
* **web:** 80-col terminal floor, pan/pinch/fling, and keyboard input-line fix ([#278](https://github.com/mossipcams/ajax-cli/issues/278)) ([8dd1ef5](https://github.com/mossipcams/ajax-cli/commit/8dd1ef575782ce7b2a504aca14a6fcd5c5e080ae))
* **web:** add settings page with web server restart ([#118](https://github.com/mossipcams/ajax-cli/issues/118)) ([73abed8](https://github.com/mossipcams/ajax-cli/commit/73abed81f27ac25b500b3d7659e5a56145a002c2))
* **web:** auto-update installed PWA shell instead of requiring re-add ([#123](https://github.com/mossipcams/ajax-cli/issues/123)) ([c6e573a](https://github.com/mossipcams/ajax-cli/commit/c6e573a037c10630a24062fce15355e585f625fa))
* **web:** begin Svelte+TS migration — contracts, toolchain, typed boundaries ([#196](https://github.com/mossipcams/ajax-cli/issues/196)) ([ad3d2f7](https://github.com/mossipcams/ajax-cli/commit/ad3d2f7d7a7865402f7e0ed26833e0e29e68738a))
* **web:** build Svelte entry and mount the shell ([#197](https://github.com/mossipcams/ajax-cli/issues/197)) ([1508627](https://github.com/mossipcams/ajax-cli/commit/1508627415113fc10d3248d977f902952505757f))
* **web:** experimental wterm Terminal Surface V2 spike ([#461](https://github.com/mossipcams/ajax-cli/issues/461)) ([5d43c98](https://github.com/mossipcams/ajax-cli/commit/5d43c98ee81e749453dec6a4fd5ec75383afb1f6))
* **web:** fit terminal geometry to viewport width ([#339](https://github.com/mossipcams/ajax-cli/issues/339)) ([2ac0340](https://github.com/mossipcams/ajax-cli/commit/2ac034011cbfcf3fdee283f5dfb97f8fa6e23e63))
* **web:** fit terminal text to the viewport width ([#331](https://github.com/mossipcams/ajax-cli/issues/331)) ([54600e6](https://github.com/mossipcams/ajax-cli/commit/54600e6122b2b0eff1a02366ae369a7b7fac364b))
* **web:** fix inline terminal fill and paste/copy on iOS ([#393](https://github.com/mossipcams/ajax-cli/issues/393)) ([7ad8edc](https://github.com/mossipcams/ajax-cli/commit/7ad8edc6379e40314a9262997fc561c435b228d0))
* **web:** fluid terminal scrolling, typing echo, and task-open latency ([#455](https://github.com/mossipcams/ajax-cli/issues/455)) ([f200f8e](https://github.com/mossipcams/ajax-cli/commit/f200f8e33aa735cae02af6befe3d9bbad961ad26))
* **web:** full cockpit control with pane view and agent input ([#105](https://github.com/mossipcams/ajax-cli/issues/105)) ([e6cdc7e](https://github.com/mossipcams/ajax-cli/commit/e6cdc7e085aef14bcbb03d6ca0c18abe290037bb))
* **web:** full-screen mobile terminal with keyboard-aware viewport ([#247](https://github.com/mossipcams/ajax-cli/issues/247)) ([9a26cb8](https://github.com/mossipcams/ajax-cli/commit/9a26cb88601f7ec2fc7cdb557ec7fea24bbdf9c6))
* **web:** full-screen, keyboard-aware mobile terminal for iOS Safari ([#241](https://github.com/mossipcams/ajax-cli/issues/241)) ([e189df3](https://github.com/mossipcams/ajax-cli/commit/e189df37fff7b5809f1355923f58183d4e356a1d))
* **web:** migrate cockpit UI areas to Svelte and switch Rust serving to bundle ([#200](https://github.com/mossipcams/ajax-cli/issues/200)) ([31c16e7](https://github.com/mossipcams/ajax-cli/commit/31c16e7afe8b7250de644b2b3dbab2e960e36367))
* **web:** migrate terminal engine to rcarmo/ghostty-web v0.9.4 ([#389](https://github.com/mossipcams/ajax-cli/issues/389)) ([f29dd7d](https://github.com/mossipcams/ajax-cli/commit/f29dd7d041a1bc85fccb4190a730271191ec5b75))
* **web:** migrate to rcarmo/ghostty-web and edge-to-edge fullscreen terminal ([#353](https://github.com/mossipcams/ajax-cli/issues/353)) ([f54ec11](https://github.com/mossipcams/ajax-cli/commit/f54ec11af3349d249a87a005107c20337c405474))
* **web:** modernize mobile cockpit with touch gestures, skeleton loads, and depth ([#210](https://github.com/mossipcams/ajax-cli/issues/210)) ([b280991](https://github.com/mossipcams/ajax-cli/commit/b2809915eab85448ad5adf5df6ed99c943e21d72))
* **web:** overhaul Settings into Dev settings ([#491](https://github.com/mossipcams/ajax-cli/issues/491)) ([82c10c9](https://github.com/mossipcams/ajax-cli/commit/82c10c980e405ada58402155a03247b472bd3dd9))
* **web:** redesign Safari cockpit with inbox-first dashboard ([#137](https://github.com/mossipcams/ajax-cli/issues/137)) ([6134fe0](https://github.com/mossipcams/ajax-cli/commit/6134fe036c8e7f729cb3a5d4e7030ecddc5576da))
* **web:** refactor mobile terminal experience ([#263](https://github.com/mossipcams/ajax-cli/issues/263)) ([08bba40](https://github.com/mossipcams/ajax-cli/commit/08bba4066c2a7521e26e20b3a7f8c2db1f3e987d))
* **web:** retire ajax-cli feature lattice and collapse duplicated guards ([#435](https://github.com/mossipcams/ajax-cli/issues/435)) ([e09e0a6](https://github.com/mossipcams/ajax-cli/commit/e09e0a6424c9a1926a5a3d688f4329bbb980bdf6))
* **web:** task recency, remembered defaults, and cockpit a11y polish ([#421](https://github.com/mossipcams/ajax-cli/issues/421)) ([a0ddeeb](https://github.com/mossipcams/ajax-cli/commit/a0ddeeb2c726c174244fe4fc6312302b494ed9d9))
* **web:** triage-only structured agent answering with guarded approvals ([#109](https://github.com/mossipcams/ajax-cli/issues/109)) ([b6cbf0a](https://github.com/mossipcams/ajax-cli/commit/b6cbf0a90ffab071e2512265e3b5241fdbc8f295))


### Bug Fixes

* add mobile terminal app shell ([#284](https://github.com/mossipcams/ajax-cli/issues/284)) ([6d32ce8](https://github.com/mossipcams/ajax-cli/commit/6d32ce8cf32a799ca602d0c206c217fcd0c03d4c))
* allow empty registry wipe save ([#211](https://github.com/mossipcams/ajax-cli/issues/211)) ([7778248](https://github.com/mossipcams/ajax-cli/commit/77782486bf36303b1f3ba9963b4cf1789d1ddadd))
* avoid nested tmux attach flicker ([#131](https://github.com/mossipcams/ajax-cli/issues/131)) ([503f89b](https://github.com/mossipcams/ajax-cli/commit/503f89b168b69682cc93ccf956fb4346bc182a3d))
* balance bottom nav after dropping Settings button ([#193](https://github.com/mossipcams/ajax-cli/issues/193)) ([07e205e](https://github.com/mossipcams/ajax-cli/commit/07e205e9926f5198b74432c149bab3ab799acad7))
* bypass CI for release-please PRs ([#266](https://github.com/mossipcams/ajax-cli/issues/266)) ([e9b16a9](https://github.com/mossipcams/ajax-cli/commit/e9b16a959d85f404c492c7a953efc75b11a209a0))
* **cli:** keep cockpit open after ctrl-q save error ([#447](https://github.com/mossipcams/ajax-cli/issues/447)) ([b49d7a9](https://github.com/mossipcams/ajax-cli/commit/b49d7a9a1c3353a2dbcf40342d234f9ea960e577))
* **cli:** rebuild cockpit snapshot when cached tasks are removed ([#142](https://github.com/mossipcams/ajax-cli/issues/142)) ([838e629](https://github.com/mossipcams/ajax-cli/commit/838e629d96965e425818b0d1786eecb7b0b6cdd3))
* consolidate drop teardown resource metadata ([#186](https://github.com/mossipcams/ajax-cli/issues/186)) ([6791322](https://github.com/mossipcams/ajax-cli/commit/6791322475ab1094550d8e530c0fe729675282fd))
* **core:** force-delete unmerged branches on cleanup drop ([#426](https://github.com/mossipcams/ajax-cli/issues/426)) ([2d56235](https://github.com/mossipcams/ajax-cli/commit/2d56235f9b5b6829e5b8c7bfb81d64954562293f))
* **core:** make terminal statuses track live pane reality ([#363](https://github.com/mossipcams/ajax-cli/issues/363)) ([32c4d36](https://github.com/mossipcams/ajax-cli/commit/32c4d3677ffb2b671978b6145578b1e54bc0ed2c))
* **core:** recreate missing worktrees on repair and lock terminal ownership ([#401](https://github.com/mossipcams/ajax-cli/issues/401)) ([2061092](https://github.com/mossipcams/ajax-cli/commit/2061092d734d20ab81f5b0eed2e32bfa853aca7f))
* create task worktrees from origin default branch ([#144](https://github.com/mossipcams/ajax-cli/issues/144)) ([e534226](https://github.com/mossipcams/ajax-cli/commit/e534226c83f72165a8b6027c2317ff634c17ccc2))
* declutter web task page and fix terminal disclosure auto-collapse ([#152](https://github.com/mossipcams/ajax-cli/issues/152)) ([403dbfd](https://github.com/mossipcams/ajax-cli/commit/403dbfdb6cfaded7c803803bd6c8eb1274a02d2b))
* ensure recommended primary action is in available actions ([#227](https://github.com/mossipcams/ajax-cli/issues/227)) ([f86417e](https://github.com/mossipcams/ajax-cli/commit/f86417ea560fd122e52deb0674281281140ebde8))
* gate web API with browser session ([#215](https://github.com/mossipcams/ajax-cli/issues/215)) ([e1ab2a1](https://github.com/mossipcams/ajax-cli/commit/e1ab2a1126c15c821416fe83c02bf75aec01d035))
* generate graphify output per task worktree ([#172](https://github.com/mossipcams/ajax-cli/issues/172)) ([f4bae08](https://github.com/mossipcams/ajax-cli/commit/f4bae08acd9f8a0f0406390f5bde24871c88a843))
* harden concurrent saves and web operation coordination ([#150](https://github.com/mossipcams/ajax-cli/issues/150)) ([a19e8fb](https://github.com/mossipcams/ajax-cli/commit/a19e8fb114d44de3301fe6b3eec7a0db53719c21))
* harden web operations and new-task repo validation ([#230](https://github.com/mossipcams/ajax-cli/issues/230)) ([681d414](https://github.com/mossipcams/ajax-cli/commit/681d41420449c2388e4bbca2fa2914bb1437ed59))
* hold notification re-arm for a cooldown after each delivery ([#431](https://github.com/mossipcams/ajax-cli/issues/431)) ([8d7e02c](https://github.com/mossipcams/ajax-cli/commit/8d7e02cfae55be24d075fc129681ba988955b4d8))
* improve mobile terminal scrolling ([#259](https://github.com/mossipcams/ajax-cli/issues/259)) ([c9e2393](https://github.com/mossipcams/ajax-cli/commit/c9e2393e8492d6c57a74823939dd4e0db461b2cf))
* intercept terminal scroll gestures ([#261](https://github.com/mossipcams/ajax-cli/issues/261)) ([a5d4542](https://github.com/mossipcams/ajax-cli/commit/a5d45428516b88dc2c9d695b7780a7a71898d26e))
* keep confirmed cockpit action selected ([#166](https://github.com/mossipcams/ajax-cli/issues/166)) ([8aff147](https://github.com/mossipcams/ajax-cli/commit/8aff147421301eb8d5429ca8f4fe859e41a5f5d7))
* keep native cockpit in sync with web cockpit state ([#160](https://github.com/mossipcams/ajax-cli/issues/160)) ([3477afe](https://github.com/mossipcams/ajax-cli/commit/3477afe68604bbe5763577e5ff4e0c63d72e96ae))
* make dev web restart safer ([#148](https://github.com/mossipcams/ajax-cli/issues/148)) ([388d947](https://github.com/mossipcams/ajax-cli/commit/388d947adea0c1323f06173bb6d69cf222c2bb7b))
* make partial task drops resilient ([#146](https://github.com/mossipcams/ajax-cli/issues/146)) ([8af6f0c](https://github.com/mossipcams/ajax-cli/commit/8af6f0c6b70395a43501ea24c2f48d0319403c54))
* make task runtime status authoritative ([#154](https://github.com/mossipcams/ajax-cli/issues/154)) ([b504ba0](https://github.com/mossipcams/ajax-cli/commit/b504ba028f437466c26dd61ab61a2417788d4a50))
* merge compatible context save task facts ([#272](https://github.com/mossipcams/ajax-cli/issues/272)) ([a0373e0](https://github.com/mossipcams/ajax-cli/commit/a0373e08fdb6137a1016432a92ef9517f7e31081))
* merge compatible context task facts ([#316](https://github.com/mossipcams/ajax-cli/issues/316)) ([bfb4ba3](https://github.com/mossipcams/ajax-cli/commit/bfb4ba36727a147c363918e13080c911535b749c))
* preserve cockpit drop confirmation across refresh ([#163](https://github.com/mossipcams/ajax-cli/issues/163)) ([470d9b8](https://github.com/mossipcams/ajax-cli/commit/470d9b8b15b2baa5c8b8dd89c432648a44b00872))
* prevent ctrl-q save conflicts after cockpit reload ([#188](https://github.com/mossipcams/ajax-cli/issues/188)) ([2687b6d](https://github.com/mossipcams/ajax-cli/commit/2687b6d1372cf1b6c6f8c479c36fe9d413801f09))
* prevent empty registry saves from wiping state ([#184](https://github.com/mossipcams/ajax-cli/issues/184)) ([02a944d](https://github.com/mossipcams/ajax-cli/commit/02a944d50c698398b56b8070a03933fe9c35d525))
* prevent stale autosnooze task reappearance ([#190](https://github.com/mossipcams/ajax-cli/issues/190)) ([1e5c803](https://github.com/mossipcams/ajax-cli/commit/1e5c8036a24b46e7508a31b5503a05e059f3fac7))
* prevent web server wedge from blocking terminal PTY cleanup ([#236](https://github.com/mossipcams/ajax-cli/issues/236)) ([9b96229](https://github.com/mossipcams/ajax-cli/commit/9b962290c1bf2ba349bb1508aba8da2895e118ce))
* prune worktree before dropping branch ([#170](https://github.com/mossipcams/ajax-cli/issues/170)) ([d504c31](https://github.com/mossipcams/ajax-cli/commit/d504c31014a2cf73815855d9e5869d29a2ab5e50))
* reconcile renamed task tmux sessions ([#198](https://github.com/mossipcams/ajax-cli/issues/198)) ([f4a8918](https://github.com/mossipcams/ajax-cli/commit/f4a89186f3b2055bad2411ad09a6158501906cf5))
* recover web cockpit when registry state diverges from disk ([#164](https://github.com/mossipcams/ajax-cli/issues/164)) ([55e386b](https://github.com/mossipcams/ajax-cli/commit/55e386b2acf45fb4b54b7003c572c18138171913))
* register workspace crates in release please ([#98](https://github.com/mossipcams/ajax-cli/issues/98)) ([b4f083e](https://github.com/mossipcams/ajax-cli/commit/b4f083e0718dcf8c131e1f7d8ba64aec61574af6))
* **release:** collapse workspace to one releasable path ([#114](https://github.com/mossipcams/ajax-cli/issues/114)) ([2d09612](https://github.com/mossipcams/ajax-cli/commit/2d09612811bb07aba1206dd8579008a6a8400324))
* **release:** keep one shared workspace release line ([#113](https://github.com/mossipcams/ajax-cli/issues/113)) ([2e49262](https://github.com/mossipcams/ajax-cli/commit/2e492625bf5b9837f1b5fae1b162180a1ae04456))
* reload cockpit state on sqlite revision changes ([#194](https://github.com/mossipcams/ajax-cli/issues/194)) ([13a54bb](https://github.com/mossipcams/ajax-cli/commit/13a54bb61905f4fdf525af6f446b4ee67b5cc372))
* remove legacy web router and stabilize smoke hashes ([#224](https://github.com/mossipcams/ajax-cli/issues/224)) ([f12025c](https://github.com/mossipcams/ajax-cli/commit/f12025c7b5aeb27fd99550b9d6bf4f7b4ad24799))
* **repair:** recreate missing worktrees ([#486](https://github.com/mossipcams/ajax-cli/issues/486)) ([5bd2641](https://github.com/mossipcams/ajax-cli/commit/5bd26416d193ca76e913a52441a33a95f67b6157))
* restore release please token selection ([#182](https://github.com/mossipcams/ajax-cli/issues/182)) ([8135d9a](https://github.com/mossipcams/ajax-cli/commit/8135d9a6e49dfc357e46f11eb30b578575831b34))
* **scripts:** always pull origin/main on web restart ([#444](https://github.com/mossipcams/ajax-cli/issues/444)) ([6fbf242](https://github.com/mossipcams/ajax-cli/commit/6fbf2429781d2f07bc882adbc48bf60876b35436))
* send Access credentials with web API fetches ([#213](https://github.com/mossipcams/ajax-cli/issues/213)) ([4849122](https://github.com/mossipcams/ajax-cli/commit/48491229f4bba5685999f9a23ceb7fb453a8fd71))
* sharpen ajax operator loop ([#220](https://github.com/mossipcams/ajax-cli/issues/220)) ([dc63907](https://github.com/mossipcams/ajax-cli/commit/dc639072eff1c8dbeaf03ac7144b3462edcd7aa2))
* simplify sqlite registry mapping and adapter command builders ([#175](https://github.com/mossipcams/ajax-cli/issues/175)) ([57c187e](https://github.com/mossipcams/ajax-cli/commit/57c187e1c0028800d550cd343a33861450426c14))
* stabilize release please workspace version rewrites ([#102](https://github.com/mossipcams/ajax-cli/issues/102)) ([160eafa](https://github.com/mossipcams/ajax-cli/commit/160eaface52cc633156fb3e9d2613f4359f61879))
* stop release please phantom sync release loop ([#107](https://github.com/mossipcams/ajax-cli/issues/107)) ([f5174a3](https://github.com/mossipcams/ajax-cli/commit/f5174a30d629fd6cf3134fa98c76d11633ce1992))
* streamline web cockpit detail view and stop poll jitter ([#191](https://github.com/mossipcams/ajax-cli/issues/191)) ([4a72879](https://github.com/mossipcams/ajax-cli/commit/4a7287939fcff8305d28d70da491d2667869555f))
* sync release please manifest paths on grouped releases ([#108](https://github.com/mossipcams/ajax-cli/issues/108)) ([d582aa2](https://github.com/mossipcams/ajax-cli/commit/d582aa24f264a941dc55116d9745978f29d62321))
* **task-session:** propagate terminal resize to PTY master ([#121](https://github.com/mossipcams/ajax-cli/issues/121)) ([32b7402](https://github.com/mossipcams/ajax-cli/commit/32b74025320f1c4dc621ecb2fccf94201c01f495))
* unify ghost-task classification across persistence and Cockpit ([#99](https://github.com/mossipcams/ajax-cli/issues/99)) ([142c2fd](https://github.com/mossipcams/ajax-cli/commit/142c2fdb5dc8a9c30c5b73cc7c58049bebbd8c7a))
* use agent-specific launch commands for new tasks ([#116](https://github.com/mossipcams/ajax-cli/issues/116)) ([c011a68](https://github.com/mossipcams/ajax-cli/commit/c011a682950a8f22ad45aecd44e6a893746182cb))
* use github.token for release please ([#180](https://github.com/mossipcams/ajax-cli/issues/180)) ([5679ab8](https://github.com/mossipcams/ajax-cli/commit/5679ab8e62580b595a74dfbbe476a87ac2e7387b))
* **web:** align task page full-bleed and reset terminal on reconnect ([#424](https://github.com/mossipcams/ajax-cli/issues/424)) ([262ab16](https://github.com/mossipcams/ajax-cli/commit/262ab160b668f3fd92ddf4b75699bf8a925967f2))
* **web:** align wterm Surface V2 sizing with PTY output ([#465](https://github.com/mossipcams/ajax-cli/issues/465)) ([2f202c3](https://github.com/mossipcams/ajax-cli/commit/2f202c382a2d96bd6df9311170f845afc3639440))
* **web:** allow terminal task detail route ([#330](https://github.com/mossipcams/ajax-cli/issues/330)) ([93ba6d1](https://github.com/mossipcams/ajax-cli/commit/93ba6d151e47e5dbbf2f3e6f649c460d90090112))
* **web:** anchor keyboard input and speed up terminal load ([#453](https://github.com/mossipcams/ajax-cli/issues/453)) ([4304bce](https://github.com/mossipcams/ajax-cli/commit/4304bce29df98413c01fc3fba2bec2fb2ac1d1ac))
* **web:** cap viewport zoom to stop iOS fullscreen focus-zoom ([#379](https://github.com/mossipcams/ajax-cli/issues/379)) ([ab18a90](https://github.com/mossipcams/ajax-cli/commit/ab18a90b877f58bbc3154295122d040aa05ef0d1))
* **web:** catch Surface V2 yellow banner on mobile WebKit ([#476](https://github.com/mossipcams/ajax-cli/issues/476)) ([25064f7](https://github.com/mossipcams/ajax-cli/commit/25064f72f59654f6f12e61e392d02c613fb6d084))
* **web:** clamp terminal pan after refit ([#326](https://github.com/mossipcams/ajax-cli/issues/326)) ([226d468](https://github.com/mossipcams/ajax-cli/commit/226d46854e995d78bb2d739306c6f5c877bfc123))
* **web:** classify reachable backend errors ([#207](https://github.com/mossipcams/ajax-cli/issues/207)) ([5b456b6](https://github.com/mossipcams/ajax-cli/commit/5b456b6a7a38a4901f9d2ce68ea89e7e44df00de))
* **web:** clear zero-lag overlay on char-by-char PTY echo ([#408](https://github.com/mossipcams/ajax-cli/issues/408)) ([e568b16](https://github.com/mossipcams/ajax-cli/commit/e568b165d84f811eb0c6f5808f8cdfb44d4d9c6e))
* **web:** compact dashboard task rows for mobile density ([#301](https://github.com/mossipcams/ajax-cli/issues/301)) ([bb05157](https://github.com/mossipcams/ajax-cli/commit/bb05157d6e659fe8b6174c57faf8664e913b64e9))
* **web:** compact iOS Safari terminal sizing and touch scroll tests ([#268](https://github.com/mossipcams/ajax-cli/issues/268)) ([83b185c](https://github.com/mossipcams/ajax-cli/commit/83b185c6108f6ba17785ee08e4ca5ec92c2ed605))
* **web:** contain iOS PWA terminal width ([#336](https://github.com/mossipcams/ajax-cli/issues/336)) ([8b86482](https://github.com/mossipcams/ajax-cli/commit/8b8648209d81d68e78ff0fa0eb22fb30e42c3ff9))
* **web:** correct terminal fullscreen chrome peek-through and zero-lag echo sizing ([#381](https://github.com/mossipcams/ajax-cli/issues/381)) ([800abcb](https://github.com/mossipcams/ajax-cli/commit/800abcbcc4a7c8034c5c5b9b14bf07d6bd43765c))
* **web:** defer iOS PWA service worker registration ([#128](https://github.com/mossipcams/ajax-cli/issues/128)) ([baeb875](https://github.com/mossipcams/ajax-cli/commit/baeb8756ba98b692b73861ffb58f0528d3606216))
* **web:** deliver push notifications instead of crashing the poller ([#125](https://github.com/mossipcams/ajax-cli/issues/125)) ([acf3684](https://github.com/mossipcams/ajax-cli/commit/acf36842d92449b8f5ba9b93fb78d9f2a822d342))
* **web:** echo mobile terminal input earlier ([#322](https://github.com/mossipcams/ajax-cli/issues/322)) ([aba3185](https://github.com/mossipcams/ajax-cli/commit/aba318550face2efa40e246a0f7a1d214687431c))
* **web:** enforce frontend contract parity ([#204](https://github.com/mossipcams/ajax-cli/issues/204)) ([184821c](https://github.com/mossipcams/ajax-cli/commit/184821c4e2ee47f64570abe7a10551ba49beebd6))
* **web:** fill scaled terminal height and authorize last-task Drop save ([#445](https://github.com/mossipcams/ajax-cli/issues/445)) ([561df9f](https://github.com/mossipcams/ajax-cli/commit/561df9fdc2dc5fa1fa076d7e1043e86bda0e1b4e))
* **web:** flush pinch rewrap, block page zoom, center terminal ([#341](https://github.com/mossipcams/ajax-cli/issues/341)) ([97ff8d6](https://github.com/mossipcams/ajax-cli/commit/97ff8d66683331bac55fa147ec276ce6059887f8))
* **web:** harden api session renewal ([#222](https://github.com/mossipcams/ajax-cli/issues/222)) ([e8eb0e6](https://github.com/mossipcams/ajax-cli/commit/e8eb0e6b8e8b58f77f30a5a008a68ca5f47b7178))
* **web:** harden iOS terminal — pinch deadzone + fullscreen button safe-area ([#373](https://github.com/mossipcams/ajax-cli/issues/373)) ([79200d8](https://github.com/mossipcams/ajax-cli/commit/79200d8e1d9e56d35bfcc4c248964b025f6e7e07))
* **web:** hide PWA scrollbars on iOS Safari ([#101](https://github.com/mossipcams/ajax-cli/issues/101)) ([216c1fb](https://github.com/mossipcams/ajax-cli/commit/216c1fb858abb9c9a5d990093441f5dfd3e3dd46))
* **web:** hide terminal key row overflow scrollbar ([#417](https://github.com/mossipcams/ajax-cli/issues/417)) ([532ca1e](https://github.com/mossipcams/ajax-cli/commit/532ca1e6ba8b876b3143caee3ca38eb2b6b03ce1))
* **web:** improve terminal fullscreen tapping ([#307](https://github.com/mossipcams/ajax-cli/issues/307)) ([0777c37](https://github.com/mossipcams/ajax-cli/commit/0777c3788146c0edfeeeea7bee9d8028d6fc426f))
* **web:** instantiate wterm WASM without Safari blob fetch ([#469](https://github.com/mossipcams/ajax-cli/issues/469)) ([0ac5de9](https://github.com/mossipcams/ajax-cli/commit/0ac5de997e7fd5421bc6a47af2477d8bf3123fa4))
* **web:** iOS Safari terminal scroll corruption + compact sizing ([#270](https://github.com/mossipcams/ajax-cli/issues/270)) ([bcec33f](https://github.com/mossipcams/ajax-cli/commit/bcec33f3d95e9c6a9cce9f65b21661d2e652b34c))
* **web:** keep composer text when the terminal socket is not open ([#288](https://github.com/mossipcams/ajax-cli/issues/288)) ([d447e67](https://github.com/mossipcams/ajax-cli/commit/d447e67026bd30939563e74cbaa1ded3e5e3f8b4))
* **web:** keep ghostty fullscreen scroll interactive ([#313](https://github.com/mossipcams/ajax-cli/issues/313)) ([62a05f0](https://github.com/mossipcams/ajax-cli/commit/62a05f00bca6e100ebd374d7a4f94ccc86b78215))
* **web:** keep mobile task-view scroll inside the terminal and remove dead Wide hotkey ([#387](https://github.com/mossipcams/ajax-cli/issues/387)) ([cf9a2d7](https://github.com/mossipcams/ajax-cli/commit/cf9a2d7b32e0f23a5055f9c9be64b092cbcd69a1))
* **web:** keep mobile terminal within viewport ([#334](https://github.com/mossipcams/ajax-cli/issues/334)) ([5cdb812](https://github.com/mossipcams/ajax-cli/commit/5cdb812314900e8a843da79ddcb063c982374ffa))
* **web:** keep terminal scrollback scrollable and add OpenCode agent option ([#368](https://github.com/mossipcams/ajax-cli/issues/368)) ([6ce5d59](https://github.com/mossipcams/ajax-cli/commit/6ce5d596148ac73ee11f7b0818f54c6eb0db2481))
* **web:** keep the mobile terminal usable when the iOS keyboard is open ([#253](https://github.com/mossipcams/ajax-cli/issues/253)) ([9d9ddc7](https://github.com/mossipcams/ajax-cli/commit/9d9ddc76b742c1fcbdf41c38596f41f50a51b841))
* **web:** keyboard-band terminal fit, key row reach, JWT redaction, HIG taps ([#448](https://github.com/mossipcams/ajax-cli/issues/448)) ([46cc305](https://github.com/mossipcams/ajax-cli/commit/46cc305b6f3322057386ecffce24124743742532))
* **web:** lock document scroll and shrink chrome for full-screen mobile terminal ([#243](https://github.com/mossipcams/ajax-cli/issues/243)) ([3c64715](https://github.com/mossipcams/ajax-cli/commit/3c64715e891b649c890a56af31d890d7d9e0a9aa))
* **web:** lock document scroll and shrink chrome for full-screen mobile terminal ([#245](https://github.com/mossipcams/ajax-cli/issues/245)) ([e39d550](https://github.com/mossipcams/ajax-cli/commit/e39d5504cae74687df54c0c134cebc9c42d47779))
* **web:** make cockpit dashboard-first and split releases per crate ([#111](https://github.com/mossipcams/ajax-cli/issues/111)) ([89906d9](https://github.com/mossipcams/ajax-cli/commit/89906d92243600694b26cd241d677006070ed7f8))
* **web:** make iOS Safari terminal actually scroll on touch ([#275](https://github.com/mossipcams/ajax-cli/issues/275)) ([07bdc73](https://github.com/mossipcams/ajax-cli/commit/07bdc7392275fd793c2c68c3b70fa3084abaef0f))
* **web:** make the mobile terminal scrollable via touch drag ([#251](https://github.com/mossipcams/ajax-cli/issues/251)) ([db070a0](https://github.com/mossipcams/ajax-cli/commit/db070a03ebcc8aece8eb6ad02632af5c02bfb779))
* **web:** mobile terminal polish — bigger text, no DOM scrollbar, taller terminal, smooth expand ([#280](https://github.com/mossipcams/ajax-cli/issues/280)) ([53337f9](https://github.com/mossipcams/ajax-cli/commit/53337f9cecc8d14b08828856cd32a25f718c29fb))
* **web:** pass options when constructing GhosttyCore for wterm ([#472](https://github.com/mossipcams/ajax-cli/issues/472)) ([cbdccde](https://github.com/mossipcams/ajax-cli/commit/cbdccdee24c3e5d8452b982ab1a145a5ae64497b))
* **web:** pin keyboard-open shell to the visual viewport band ([#395](https://github.com/mossipcams/ajax-cli/issues/395)) ([cec3c56](https://github.com/mossipcams/ajax-cli/commit/cec3c561a923e7dd226004d114496c803dd10367))
* **web:** pin new-task sheet to the visual viewport band ([#362](https://github.com/mossipcams/ajax-cli/issues/362)) ([47c2eaa](https://github.com/mossipcams/ajax-cli/commit/47c2eaaef91f983850ac5d7de647ae541a9f06be))
* **web:** pinch rewrap with keyboard open, kill page zoom at touchdown ([#343](https://github.com/mossipcams/ajax-cli/issues/343)) ([5a66477](https://github.com/mossipcams/ajax-cli/commit/5a66477ea95e6c91bc2f3b42c165abcd5b5156ea))
* **web:** polish mobile terminal gestures ([#360](https://github.com/mossipcams/ajax-cli/issues/360)) ([704e23d](https://github.com/mossipcams/ajax-cli/commit/704e23d885fb587663c712b92e7849e35c1c59dd))
* **web:** position zero-lag overlay with renderer cell metrics ([#410](https://github.com/mossipcams/ajax-cli/issues/410)) ([9113133](https://github.com/mossipcams/ajax-cli/commit/9113133e69f1167e7483a6b9320b99b0a843979c))
* **web:** prevent tls accept starvation ([#209](https://github.com/mossipcams/ajax-cli/issues/209)) ([febb44e](https://github.com/mossipcams/ajax-cli/commit/febb44e07546d46f16d2787d829ad8d3c38d605d))
* **web:** prove wterm GhosttyCore init with real WASM tests ([#474](https://github.com/mossipcams/ajax-cli/issues/474)) ([deb68ae](https://github.com/mossipcams/ajax-cli/commit/deb68ae8923f50f78a0262eb61ba29a35ce3d027))
* **web:** raw-first task terminal on mobile and desktop ([#264](https://github.com/mossipcams/ajax-cli/issues/264)) ([6f4bdb7](https://github.com/mossipcams/ajax-cli/commit/6f4bdb742b009665c8c5ff7c4aed71c85e5ba762))
* **web:** re-fit terminal after fullscreen viewport settles ([#375](https://github.com/mossipcams/ajax-cli/issues/375)) ([004c74f](https://github.com/mossipcams/ajax-cli/commit/004c74fc2390e0309f3056f92dc618aff4561337))
* **web:** reduce iOS terminal input lag ([#320](https://github.com/mossipcams/ajax-cli/issues/320)) ([5be4da1](https://github.com/mossipcams/ajax-cli/commit/5be4da1a7b20768adb9cd76fd28bc227db98eca0))
* **web:** refine pwa terminal fullscreen ([#309](https://github.com/mossipcams/ajax-cli/issues/309)) ([1797f6c](https://github.com/mossipcams/ajax-cli/commit/1797f6cb12ad24fe8a874b8a313f9f1cc20e21e4))
* **web:** refine pwa terminal fullscreen ([#311](https://github.com/mossipcams/ajax-cli/issues/311)) ([dac821a](https://github.com/mossipcams/ajax-cli/commit/dac821a3f5db844ce08e28bc540adb35283f08c5))
* **web:** refit expanded terminal with keyboard open ([#385](https://github.com/mossipcams/ajax-cli/issues/385)) ([fda4cd2](https://github.com/mossipcams/ajax-cli/commit/fda4cd2bbb673a2590ac13b22ea564f511ac69d0))
* **web:** refit terminal after pinch layout settles ([#328](https://github.com/mossipcams/ajax-cli/issues/328)) ([5e992d8](https://github.com/mossipcams/ajax-cli/commit/5e992d8c833113a5a868a309827268c11ded4575))
* **web:** remove redundant detail resume action ([#300](https://github.com/mossipcams/ajax-cli/issues/300)) ([c2984b5](https://github.com/mossipcams/ajax-cli/commit/c2984b5cd9e151043a20bef00e620e525eaaf306))
* **web:** remove redundant task open controls ([#298](https://github.com/mossipcams/ajax-cli/issues/298)) ([8ae673d](https://github.com/mossipcams/ajax-cli/commit/8ae673d19ab0e082f44a11ff16ca3bdcaca97cc3))
* **web:** remove server-issued confirmation-token gate for destructive actions ([#296](https://github.com/mossipcams/ajax-cli/issues/296)) ([b80eacb](https://github.com/mossipcams/ajax-cli/commit/b80eacb882809ff35512595973a5f50f66f69466))
* **web:** remove terminal shell edge padding ([#358](https://github.com/mossipcams/ajax-cli/issues/358)) ([0d5b18c](https://github.com/mossipcams/ajax-cli/commit/0d5b18cd7dad09fd56f2bb6fd91610ae0d11b03f))
* **web:** renew browser session after stale API cookie ([#217](https://github.com/mossipcams/ajax-cli/issues/217)) ([38ed536](https://github.com/mossipcams/ajax-cli/commit/38ed5365085922998084f91aae161e4ac000e029))
* **web:** repair fullscreen blank column, off-terminal echo, and iOS backspace repeat ([#397](https://github.com/mossipcams/ajax-cli/issues/397)) ([c7d2911](https://github.com/mossipcams/ajax-cli/commit/c7d29112b7aed157224f0635b8dcb168c3413650))
* **web:** repair ghostty terminal shell contracts ([#354](https://github.com/mossipcams/ajax-cli/issues/354)) ([3991429](https://github.com/mossipcams/ajax-cli/commit/3991429192b50421694ccc54155b3231e3c668c8))
* **web:** repair iOS PWA stale shell recovery ([#132](https://github.com/mossipcams/ajax-cli/issues/132)) ([3523d7b](https://github.com/mossipcams/ajax-cli/commit/3523d7b11bbfb44d9ba019c106ce98f1159b3051))
* **web:** repair mobile cockpit regressions ([#383](https://github.com/mossipcams/ajax-cli/issues/383)) ([ec37399](https://github.com/mossipcams/ajax-cli/commit/ec373998c4a1fb47ec3b62bf3e977c290f8e9db9))
* **web:** repair terminal delete key and zero-lag overlay tracking ([#239](https://github.com/mossipcams/ajax-cli/issues/239)) ([9de77e2](https://github.com/mossipcams/ajax-cli/commit/9de77e2a07fb97f46175db54eff99c1982d9855f))
* **web:** repair worktree action, overlay toast, clear zero-lag echo ghost ([#412](https://github.com/mossipcams/ajax-cli/issues/412)) ([1a43b38](https://github.com/mossipcams/ajax-cli/commit/1a43b38b1261eaaee5aa743f198527a3acccfe0e))
* **web:** restore cockpit styling and guard against CSS regressions ([#205](https://github.com/mossipcams/ajax-cli/issues/205)) ([88378ed](https://github.com/mossipcams/ajax-cli/commit/88378edfa471b93db31b678b8211dd2bc1fe1e4e))
* **web:** restore wterm scrollback and stop viewport resize rebuilds ([#490](https://github.com/mossipcams/ajax-cli/issues/490)) ([5639e05](https://github.com/mossipcams/ajax-cli/commit/5639e0560da0638233f5d4731ccd6a6f233fbb41))
* **web:** resume task on open and surface dead terminal sessions ([#370](https://github.com/mossipcams/ajax-cli/issues/370)) ([bb8fda1](https://github.com/mossipcams/ajax-cli/commit/bb8fda10e3f0b9204a51e69ad9f672ffa985ea17))
* **web:** scale phone terminal to agent width and stabilize task order ([#440](https://github.com/mossipcams/ajax-cli/issues/440)) ([137c4b7](https://github.com/mossipcams/ajax-cli/commit/137c4b76d331071ed4b6686400e41266de0a0422))
* **web:** scale terminal on inner layer so expand stays tappable ([#442](https://github.com/mossipcams/ajax-cli/issues/442)) ([494156c](https://github.com/mossipcams/ajax-cli/commit/494156c9cea5e09c1104d8467ee88a06d74487b9))
* **web:** seed terminal scrollback from tmux history ([#429](https://github.com/mossipcams/ajax-cli/issues/429)) ([9f7a0e4](https://github.com/mossipcams/ajax-cli/commit/9f7a0e4a57324135c39857e5e852a0d40ac22da0))
* **web:** separate selection teal from attention mustard ([#422](https://github.com/mossipcams/ajax-cli/issues/422)) ([042046f](https://github.com/mossipcams/ajax-cli/commit/042046f1a0a129ec6280ea1fb3ca055447c19d16))
* **web:** serve wterm Ghostty WASM on a distinct path ([#463](https://github.com/mossipcams/ajax-cli/issues/463)) ([ecf5fd8](https://github.com/mossipcams/ajax-cli/commit/ecf5fd8902463b7dd5a52c96ea14dce32ded22c4))
* **web:** set terminal type for tmux attach ([#318](https://github.com/mossipcams/ajax-cli/issues/318)) ([b84bfe9](https://github.com/mossipcams/ajax-cli/commit/b84bfe991ebb49f870fb3740acc9dc51a55fa992))
* **web:** share start agent allowlist and delete dead web code ([#416](https://github.com/mossipcams/ajax-cli/issues/416)) ([787a136](https://github.com/mossipcams/ajax-cli/commit/787a136612fd0fdfbe7e27ef476f25c8a683ca40))
* **web:** shrink mobile terminal font to 10px for usable column fit ([#273](https://github.com/mossipcams/ajax-cli/issues/273)) ([19e2fd5](https://github.com/mossipcams/ajax-cli/commit/19e2fd56e2b18d2bde9e62e451e12f4c48c024f7))
* **web:** shrink the terminal font to 10px for more rows and columns ([#255](https://github.com/mossipcams/ajax-cli/issues/255)) ([15eb4aa](https://github.com/mossipcams/ajax-cli/commit/15eb4aae8513e442233d415e819d3a5f990d2cb7))
* **web:** shrink the terminal font to 6px ([#257](https://github.com/mossipcams/ajax-cli/issues/257)) ([7d08879](https://github.com/mossipcams/ajax-cli/commit/7d0887997e0c64d3f8542706aa19d48c303d239e))
* **web:** size terminal takeover to visual viewport ([#349](https://github.com/mossipcams/ajax-cli/issues/349)) ([22d09e6](https://github.com/mossipcams/ajax-cli/commit/22d09e68cfefb568d074134081610bf29e1c8d4b))
* **web:** stabilize experimental wterm surface ([#495](https://github.com/mossipcams/ajax-cli/issues/495)) ([67e549d](https://github.com/mossipcams/ajax-cli/commit/67e549d207c0e016c15afc4b6fdae62b3f063be5))
* **web:** stabilize mobile terminal fullscreen gestures ([#345](https://github.com/mossipcams/ajax-cli/issues/345)) ([59a82a2](https://github.com/mossipcams/ajax-cli/commit/59a82a267287347c4d5a91b239b8bd0158d19ed6))
* **web:** stabilize mobile terminal viewport ([#324](https://github.com/mossipcams/ajax-cli/issues/324)) ([7a7d1f1](https://github.com/mossipcams/ajax-cli/commit/7a7d1f1a2fd29ad002ef570a084e80f677e3dcfc))
* **web:** stabilize task terminal viewport chrome ([#399](https://github.com/mossipcams/ajax-cli/issues/399)) ([d91b2a8](https://github.com/mossipcams/ajax-cli/commit/d91b2a889e38f858ba9f8833c72c8e9af7297bac))
* **web:** stabilize Waiting/Running status and notify once per episode ([#438](https://github.com/mossipcams/ajax-cli/issues/438)) ([94dca90](https://github.com/mossipcams/ajax-cli/commit/94dca909f0e9ae21d437dc6171730fdde719d911))
* **web:** stop forced auto-scroll from blocking terminal scrollback ([#249](https://github.com/mossipcams/ajax-cli/issues/249)) ([58e6e9c](https://github.com/mossipcams/ajax-cli/commit/58e6e9c53051f81be6c6b258de7fa4ee40b808ca))
* **web:** stop fullscreen translate and hide route-scroll gutter ([#414](https://github.com/mossipcams/ajax-cli/issues/414)) ([db6c025](https://github.com/mossipcams/ajax-cli/commit/db6c025488fc8f7a065b46951871f9e6a37ac807))
* **web:** stop iOS Surface V2 solid olive terminal paint ([#478](https://github.com/mossipcams/ajax-cli/issues/478)) ([dc06f6a](https://github.com/mossipcams/ajax-cli/commit/dc06f6a1abf1effe061b229015e8f474c64abcad))
* **web:** stop iOS typing echo stretch and tighten keyboard chrome ([#406](https://github.com/mossipcams/ajax-cli/issues/406)) ([7697e85](https://github.com/mossipcams/ajax-cli/commit/7697e853aefc9bbd44ff1aaee8355c2f1fb0be40))
* **web:** stop Surface V2 full-terminal yellow wash (wterm inline bg smear) ([#480](https://github.com/mossipcams/ajax-cli/issues/480)) ([458dfef](https://github.com/mossipcams/ajax-cli/commit/458dfefd872e24a14de746506d20381b7ba1687c))
* **web:** strip scrollback-hostile PTY sequences before browser output ([#276](https://github.com/mossipcams/ajax-cli/issues/276)) ([d80779f](https://github.com/mossipcams/ajax-cli/commit/d80779faa67bd3432dbff6101b34a97d8fd812fe))
* **web:** Surface V2 mobile Safari parity — theme, font, keyboard, expand ([#487](https://github.com/mossipcams/ajax-cli/issues/487)) ([2d7e5f4](https://github.com/mossipcams/ajax-cli/commit/2d7e5f413669a55977023e6b8674ac5b6391ce7d))
* **web:** terminal full-screen, keyboard auto-scroll, arrow-key jump, task page redesign ([#291](https://github.com/mossipcams/ajax-cli/issues/291)) ([0519939](https://github.com/mossipcams/ajax-cli/commit/05199394b39964338fcd84738417c0ed07c2f544))
* **web:** terminal keyboard alignment, behavior fixes, module extraction, and [#284](https://github.com/mossipcams/ajax-cli/issues/284) shell CSS conflict fix ([#286](https://github.com/mossipcams/ajax-cli/issues/286)) ([944847f](https://github.com/mossipcams/ajax-cli/commit/944847fca52a0f24d6687000dc956bb3daed4f76))
* **web:** touch must not re-pin scrollback; align sub-cell translate with renderer frame ([#457](https://github.com/mossipcams/ajax-cli/issues/457)) ([367cd49](https://github.com/mossipcams/ajax-cli/commit/367cd49af6a72232e3fd4b3a7b521cf6fc403576))
* **web:** trail output flushes while reading scrollback ([#459](https://github.com/mossipcams/ajax-cli/issues/459)) ([e3e007c](https://github.com/mossipcams/ajax-cli/commit/e3e007c7eb5b8a8d330647a47b772260ce2bc8cc))
* **web:** validate wterm WASM before GhosttyCore.init ([#467](https://github.com/mossipcams/ajax-cli/issues/467)) ([f519010](https://github.com/mossipcams/ajax-cli/commit/f519010a14ac4751bde349fd556b5a679eaa8dcf))
* **web:** widen terminal shell and remove fit gutters ([#347](https://github.com/mossipcams/ajax-cli/issues/347)) ([621355b](https://github.com/mossipcams/ajax-cli/commit/621355b2c7971ae9220961af20314df4c88b57d8))
* **web:** wterm Surface V2 Ghostty parity improvements ([#484](https://github.com/mossipcams/ajax-cli/issues/484)) ([2e62d56](https://github.com/mossipcams/ajax-cli/commit/2e62d56f962baa668490dbad727161cc3a8df219))


### Performance Improvements

* speed up task launch and cleanup flows ([#168](https://github.com/mossipcams/ajax-cli/issues/168)) ([c11669d](https://github.com/mossipcams/ajax-cli/commit/c11669d0e689b8ef5f484b12a692831e54490bef))
* tiered runtime refresh and reduce steady-state substrate churn ([#117](https://github.com/mossipcams/ajax-cli/issues/117)) ([0e68e5d](https://github.com/mossipcams/ajax-cli/commit/0e68e5dcff5b990e47592df891ad9bdc89683d4e))
* **web:** cut mobile Cockpit battery cost without hurting terminal UX ([#404](https://github.com/mossipcams/ajax-cli/issues/404)) ([b83e544](https://github.com/mossipcams/ajax-cli/commit/b83e544160661b862bf825491f0bbf9182993098))
* **web:** defer terminal bundle and skip Git probe on browser resume ([#437](https://github.com/mossipcams/ajax-cli/issues/437)) ([83bedbe](https://github.com/mossipcams/ajax-cli/commit/83bedbe95e5b7fbff0fb853fe768a67c3c8d2c95))


### Code Refactoring

* **core:** cut over-engineering from ponytail audit ([#391](https://github.com/mossipcams/ajax-cli/issues/391)) ([8a825bb](https://github.com/mossipcams/ajax-cli/commit/8a825bb8f7a41aa616af6191a107deeb09e8ec7e))
* **core:** ratify task_operations as the vertical slice layer ([#433](https://github.com/mossipcams/ajax-cli/issues/433)) ([6954be3](https://github.com/mossipcams/ajax-cli/commit/6954be352437f7ff181b465f174e6bec021588b8))
* extract task_operations into file-backed modules ([#226](https://github.com/mossipcams/ajax-cli/issues/226)) ([04cabac](https://github.com/mossipcams/ajax-cli/commit/04cabac16007fdeb81650f94d48ed536e16fb5f0))
* sharpen ajax operator loop ([#219](https://github.com/mossipcams/ajax-cli/issues/219)) ([2278da8](https://github.com/mossipcams/ajax-cli/commit/2278da8082feced92020e89c5b8f1006e5f8957d))
* simplify runtime path/profile resolution ([#177](https://github.com/mossipcams/ajax-cli/issues/177)) ([5040192](https://github.com/mossipcams/ajax-cli/commit/5040192661311ed6a4c16d7e96f1d880cafeba1b))
* **web:** consolidate patch-layered terminal code behind behavior tests ([#282](https://github.com/mossipcams/ajax-cli/issues/282)) ([d4e6428](https://github.com/mossipcams/ajax-cli/commit/d4e64283085cbd3dd6402d6a0ccfd776db6462c8))
* **web:** extract terminal layout, scroll, zero-lag, and clipboard owners ([#432](https://github.com/mossipcams/ajax-cli/issues/432)) ([a02fc20](https://github.com/mossipcams/ajax-cli/commit/a02fc20eb975f62e5f78d5f278992183beb7a857))
* **web:** refactor web cockpit viewport ownership ([6b311db](https://github.com/mossipcams/ajax-cli/commit/6b311db58a00c949124cc85425eece5298fba561))


### Reverts

* **web:** roll back rcarmo ghostty-web migration ([#356](https://github.com/mossipcams/ajax-cli/issues/356)) ([1143bee](https://github.com/mossipcams/ajax-cli/commit/1143bee42a8fc935bc683333b7fd65e5149c8e57))
* **web:** terminal width takeover changes ([#351](https://github.com/mossipcams/ajax-cli/issues/351)) ([4d93dd8](https://github.com/mossipcams/ajax-cli/commit/4d93dd8e5d642f9b26e053beb59c5cbb7e74a394))
* **web:** undo keyboard-band terminal fit, JWT redaction, HIG taps ([#448](https://github.com/mossipcams/ajax-cli/issues/448)) ([#451](https://github.com/mossipcams/ajax-cli/issues/451)) ([67b6fee](https://github.com/mossipcams/ajax-cli/commit/67b6fee0c91d9477faa6d43e7da45eb2fc1e5301))

## [0.43.0](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.11...ajax-cli-v0.43.0) (2026-07-14)


### Features

* **web:** overhaul Settings into Dev settings ([#491](https://github.com/mossipcams/ajax-cli/issues/491)) ([82c10c9](https://github.com/mossipcams/ajax-cli/commit/82c10c980e405ada58402155a03247b472bd3dd9))


### Bug Fixes

* **web:** restore wterm scrollback and stop viewport resize rebuilds ([#490](https://github.com/mossipcams/ajax-cli/issues/490)) ([5639e05](https://github.com/mossipcams/ajax-cli/commit/5639e0560da0638233f5d4731ccd6a6f233fbb41))

## [0.42.11](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.10...ajax-cli-v0.42.11) (2026-07-14)


### Bug Fixes

* **repair:** recreate missing worktrees ([#486](https://github.com/mossipcams/ajax-cli/issues/486)) ([5bd2641](https://github.com/mossipcams/ajax-cli/commit/5bd26416d193ca76e913a52441a33a95f67b6157))
* **web:** Surface V2 mobile Safari parity — theme, font, keyboard, expand ([#487](https://github.com/mossipcams/ajax-cli/issues/487)) ([2d7e5f4](https://github.com/mossipcams/ajax-cli/commit/2d7e5f413669a55977023e6b8674ac5b6391ce7d))

## [0.42.10](https://github.com/mossipcams/ajax-cli/compare/ajax-cli-v0.42.9...ajax-cli-v0.42.10) (2026-07-14)


### Bug Fixes

* **web:** wterm Surface V2 Ghostty parity improvements ([#484](https://github.com/mossipcams/ajax-cli/issues/484)) ([2e62d56](https://github.com/mossipcams/ajax-cli/commit/2e62d56f962baa668490dbad727161cc3a8df219))

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
