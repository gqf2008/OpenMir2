# R 卡记账债分诊（2026-10-23）

- 卡：`openmir2-r-card-ledger-sweep`（owner `openmir2-worker-43`，tier=L，write_set=本文件）
- 基线：`master 2ecd4b887`；worktree `E:/tmp/wt-worker-43`，分支 `card/ledger-sweep`
- 输入：`walgit collab board --format json` 快照 + 逐卡 `walgit collab thread <id>`（203 张 merged-not-closed 全量读线程）
- **`tools/goal/goal_gate.py` 在 master `2ecd4b887` 上不存在**（`ls tools/goal/` = No such file），故本表按卡面口径自行重建：issue/review 任一声明 `tier=R` 即计入 R；一条 `tier` 都没有的 16 张另行登记（附录 A，按 AGENTS.md「缺 tier 按 R」提示补 tier，不占主集）。

## 0. 汇总

| 项 | 数量 |
|---|---:|
| merged-not-closed 全量（board 快照） | 203 |
| 其中声明 tier=R（主集） | 147 |
| **A 已具备登记**（已补 `status: in-progress`，窗口项写进 `work`） | 107 |
| **B1 无需窗口**（只发 comment 建议 closed，未改状态） | 1 |
| **B2 真等窗口但缺登记**（只发 comment，保留 in-progress） | 35 |
| **U 未合并 / 等返工**（纠正性 status，恢复 needs-review） | 1 |
| SKIP「有人在跟」<1h（只登记不动作） | 3 |
| 无 tier 声明的 merged-not-closed（附录 A，未动作） | 16 |
| 实发条目 | 144（另 +1 条 F-1 纠正） |

> 口径：A ⇒ 一条 `status`（状态维持 in-progress、owner/worktree/branch 原样保留，`work` 显式列窗口项与登记依据）；
> B ⇒ 一条 `comment`（说明缺什么；B1 附建议 closed 的理由，B2 附从原 `status.work` 摘出的窗口项）。**除 `openmir2-monster-showhp-53-broadcast` 的 1 条 F-1 纠正外，每卡恰好一条；未 closed 任何卡，未改代码/白名单。**

> **T-* 口径（宽/窄）**：A 判定用宽口径——卡片线程任意位置（含 review 正文转引）出现 whitelist 已登记的 `T-*` 即算关联；逐卡表「依据」列把「issue/status 自述 + whitelist 行点名本卡」标为 `whitelist`，仅 review 正文出现的标为「线程提及（宽口径）」。按窄口径（只认自述/点名）只有 `openmir2-mapcell-attributes-loading` 会落到 B2；该卡已按 A 补记（窗口项取自原 `status.work`，仍可被机判读到），故 A=107 / B2=35 / U=1 不变。
>
> **SKIP 3 张（最近 status <1h，只登记不动作）**：`openmir2-castle-merchant-disappear-downlink`、`openmir2-gm-admin-mode-entity-seam`、`openmir2-menu-ok-unify-pickwire`。

## 1. 逐卡表（按最近一次 status 时间升序，最陈旧先处理）

| # | 卡 id | 分类 | 依据 | 已补条目 oid | 卡片最近 status ts |
|---:|---|---|---|---|---|
| 1 | `openmir2-m3-party-attribution` | A | approve.window_required=true | `2f24bdda0b0d6e917fefd1a464112382cfafe55f` | 1791883549 |
| 2 | `openmir2-m3-party-gainexp-npe` | A | approve.window_required=true；线程提及 T-32（review 正文，宽口径） | `32de96ab8e892ff2c749f102162a10d59fa19b9d` | 1791886861 |
| 3 | `openmir2-m3-castle-gold-tax` | A | approve.window_required=true；whitelist T-38 | `7565076f7b902cc101c0df58ce1f07f506548e82` | 1791890093 |
| 4 | `openmir2-m3-pet-slave-summon` | A | approve.window_required=true；whitelist T-33 | `96145dc5a33f2f415d59fd1d48ef8f9f2d086e31` | 1791896055 |
| 5 | `openmir2-m3-pet-slave-lifecycle` | A | approve.window_required=true；whitelist T-33,T-42,T-49；线程提及 T-4,T-45,T-46（review 正文，宽口径） | `32283b1bbe41414ba88bd1f59498d0aa36a1b896` | 1791898219 |
| 6 | `openmir2-m3-clock-injection` | A | approve.window_required=true；线程提及 T-30（review 正文，宽口径） | `a1b5d86db8447f6a99ba5e3ed807552365bda634` | 1791899952 |
| 7 | `openmir2-m3-shop-dialog-wiring` | A | approve.window_required=true；whitelist T-47；线程提及 T-24（review 正文，宽口径） | `26da34a36242abe0fc7764825b8ac557e516a179` | 1791906176 |
| 8 | `openmir2-m3-pet-slave-clone46` | A | approve.window_required=true；whitelist T-51 | `edc4bbc8501df9dfeafbec853ef7a84ccdab2497` | 1791935507 |
| 9 | `openmir2-m3-rng-trace-scene-groups` | A | approve.window_required=true；线程提及 T-1,T-2,T-41,T-42,T-43（review 正文，宽口径） | `9be7769c85f5718afd296224af6b320afd5dd9a1` | 1791946567 |
| 10 | `openmir2-m3-pet-slave-masterdie-wire` | A | approve.window_required=true | `6cbf0b6c54dc5e68c445e3ef6c38bf38b7d13386` | 1791949583 |
| 11 | `openmir2-m3-rng-scene-driver` | A | approve.window_required=true；whitelist T-50,T-59；线程提及 T-42,T-57,T-58（review 正文，宽口径） | `e13059c5f404f98528202a612bf28f6f5f6c5d77` | 1792130965 |
| 12 | `openmir2-m3-drop-exp-wiring` | A | approve.window_required=true；whitelist T-19,T-25,T-32,T-33,T-57,T-58,T-59；线程提及 T-1,T-17,T-28,T-30,T-31,T-35,T-4,T-50（review 正文，宽口径） | `d898ee293017bee405737a3edabacf4397a93e63` | 1792132336 |
| 13 | `openmir2-m3-monster-active-seek` | A | approve.window_required=true | `26c41b0a71ef1120217549aa43d780d8f0efc92c` | 1792136288 |
| 14 | `openmir2-outbound-encoders-red-batch` | A | approve.window_required=true；whitelist T-19,T-25 | `d68b0466eb6d828df5dc4f2e1987fd0b0f48aa5c` | 1792139532 |
| 15 | `openmir2-window-collector-preflight` | A | approve.window_required=true | `169f7157ef95ab85145248e75020fdb09dcb92f9` | 1792144577 |
| 16 | `openmir2-transmap-maptrigger` | A | approve.window_required=true | `0f46d4fbdda703b0ba7439e9329b3cff858d10b1` | 1792146242 |
| 17 | `openmir2-orphan-encoder-wiring` | A | approve.window_required=true | `fd606fc30e2b1b432374789d6a450e7bd27e5583` | 1792158472 |
| 18 | `openmir2-world-tick-expansion` | A | approve.window_required=true；线程提及 T-64（review 正文，宽口径） | `42e0333e21917b7f0987b225bc3ff16cf48cf633` | 1792165907 |
| 19 | `openmir2-maptrigger-monster-and-door` | A | approve.window_required=true | `98fad62029ae590a6761cf022156160b62b0b5dc` | 1792169355 |
| 20 | `openmir2-buff-system` | A | approve.window_required=true | `369886bb184a9a8e299d2919ea17f96cb7b750f0` | 1792173172 |
| 21 | `openmir2-npc-ctor-draws` | A | approve.window_required=true | `86f9630307426abea6a3289a5e7e3b80cbcd9e93` | 1792178898 |
| 22 | `openmir2-buff-producer-wiring` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `58c2a34ecc73d96f6c8c7bf4abf4ffb9b3396c25` | 1792181652 |
| 23 | `openmir2-default-arm-spell-surface` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `8b39e2c4510bc9b03635343a0eb43a958887c21c` | 1792183041 |
| 24 | `openmir2-snap-cell-fidelity` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `60a1328d41f42344b7040709c9a27b3748802480` | 1792183820 |
| 25 | `openmir2-ctor-draws-fold` | A | whitelist T-70,T-71 | `2fd98a16fe537f5ad9e6853cb0e59b09591073c4` | 1792186272 |
| 26 | `openmir2-ctor-barrier-a-draw` | A | whitelist T-72 | `013b2cbe23a25533859ebfbb3aaad0cbe5d3f5b8` | 1792189051 |
| 27 | `openmir2-mapcell-attributes-loading` | A | 线程提及 T-65（review 正文，宽口径） | `a56e793e992c28a09cc3f363d56c0f0f7bd60753` | 1792191685 |
| 28 | `openmir2-recall-slave-shift-live` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `637f951ba23769375c83ffdfd254aa48441990fd` | 1792203025 |
| 29 | `openmir2-map-random-move-shift-loop` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `6368330138b39ba44ca405c8338b9310cd30051a` | 1792209736 |
| 30 | `openmir2-enter-shift-loop` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `2ad122c243f67ebc61933c92dfdb3863b9baf4a2` | 1792209745 |
| 31 | `openmir2-enter-map-cell-rejected-vs-oracle` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `7cc681d248937356450ad53b1ff27c9f7523e89c` | 1792224446 |
| 32 | `openmir2-magic-8-21-wiring` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `dc7f2f046b769b7fd7166566ea6236dd9437e25b` | 1792226043 |
| 33 | `openmir2-spell-mp-wrapper` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `74ef4efb351fae94936bd62aaa33cf06c2b58cd9` | 1792239462 |
| 34 | `openmir2-login-notice-clienttick` | A | whitelist T-73；线程提及 T-65（review 正文，宽口径） | `e8cd2c8a1f35e8c54ce2efd7a76aef78962dab53` | 1792243334 |
| 35 | `openmir2-spell-shared-tail` | A | whitelist T-75,T-78；线程提及 T-17,T-71（review 正文，宽口径） | `2b1e54bdd3b0104acfab257e84460b513cb0507b` | 1792289542 |
| 36 | `openmir2-aoi-verify-maptime` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `79e5fbb21b8de7f8afed4ca837a1dc3cf39adfbf` | 1792291008 |
| 37 | `openmir2-player-searchviewrange-cadence` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `20c9e3259e49d851279676f9df91369ee534a0bb` | 1792293898 |
| 38 | `openmir2-run-gap-player-run-quantification` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `36b78721c0769ad6490be1c56acdf3a50e6fe917` | 1792294777 |
| 39 | `openmir2-run-gap-monster-state-expiry` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `0ad2eeebf3938ef239bf4b344c9aec23f5cd298e` | 1792295243 |
| 40 | `openmir2-run-gap-corpse-ghost-revive` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `bcc40249424d1db5192075df41d45296be01ab8e` | 1792297093 |
| 41 | `openmir2-movetomovingobject-quirk` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `6873c3b164aa7292982c5f2a6c6a7ce515dbb6da` | 1792300298 |
| 42 | `openmir2-run-gap-monster-master-follow` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `bb84835a7ccc2be0b6f5b4459cb8274a834e2c66` | 1792300306 |
| 43 | `openmir2-run-gap-checkdupobj` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `d566b2b50300eb549395a650649028b3017647e4` | 1792300796 |
| 44 | `openmir2-run-gap-monster-specials` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `576ef079a7113efe0c76d4f104195201778bcb01` | 1792309010 |
| 45 | `openmir2-disappear-recipients-gridscan` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `0fbe28786145724faad3ef8bd0a8d5d27870e868` | 1792311538 |
| 46 | `openmir2-run-gap-pr1-head-blocking` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `b436268a327df094bc707faac2a128a2efd3554b` | 1792315070 |
| 47 | `openmir2-run-gap-mb1` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `5b9e49ce4ad49de26aafe1a442ade10125a3520f` | 1792318300 |
| 48 | `openmir2-monster-view-throttle` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `9a0c5b84de3d97ebf1a37edbadfbf67a2313facc` | 1792320248 |
| 49 | `openmir2-persist-create-rows` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `1f1424838a55610626c3f3f194ad4498b6b125c0` | 1792327508 |
| 50 | `openmir2-persist-useitems-wiring` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `31de76ecfecf544ff3a44736172fa3f57948e255` | 1792329879 |
| 51 | `openmir2-persist-item-attr-and-gold` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `0056197e64f9cf9bd82517e916a5fc835ceba6bf` | 1792331816 |
| 52 | `openmir2-run-gap-mb3` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `7ad3c9293c962817f807727552607861773f0fe6` | 1792332963 |
| 53 | `openmir2-save-on-shutdown` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `7c12a4f5b2405723c82267b28346e664fe3abacb` | 1792336074 |
| 54 | `openmir2-rm-refmessage-delay-layer` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `a574901d0057f11abd4fe7350fab6d06fe841bf4` | 1792337140 |
| 55 | `openmir2-active-seek-race-missing` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `c28988431f76e52ffad94e1d1900e03af2e4bf60` | 1792340567 |
| 56 | `openmir2-proper-target-unfolded` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `60d637599842728083702e880b11c303839cb7ce` | 1792340578 |
| 57 | `openmir2-search-target-clear-target` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `70a6b49ad34933d8396f909a91ba1ca077bd1fda` | 1792340591 |
| 58 | `openmir2-sm-additem-tail` | A | whitelist T-79,T-80 | `d5fd1d05f682a02537e7dc279f3cbcea99f9c5f0` | 1792341959 |
| 59 | `openmir2-sm-updateitem` | A | whitelist T-79,T-80 | `b0a9f40b1cf35dcbb2c29003efb0059a57198ac9` | 1792341972 |
| 60 | `openmir2-login-useitems-magic` | A | whitelist T-79,T-80 | `bd4a700876273dbf399df01c2f570ec6d1917baf` | 1792341984 |
| 61 | `openmir2-sm-merchantsay-recog` | A | whitelist T-79,T-80 | `44d97977a6e4c479723a77535e410305f9bcc1e5` | 1792341997 |
| 62 | `openmir2-smframe-minor-gaps` | A | whitelist T-79,T-80 | `9470fe35ecc78fb75061c25a4e7adf2a522aec04` | 1792342009 |
| 63 | `openmir2-attacktarget-overrides` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `cd707f202860467e48dba99cead0a0b7f991ca26` | 1792346571 |
| 64 | `openmir2-persist-single-sided-columns` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `1e90c59e4923b7e1dfcaab0b9e23671d6320f56f` | 1792346580 |
| 65 | `openmir2-db-error-reply` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `66ef3480cf2e08a28334366ebcba192a71564682` | 1792346589 |
| 66 | `openmir2-save-misc-gaps` | B2 | approve 无 window_required / 无 T-*；余量仅见 status.work | `e8e0f4438228510866658903cc7ef4042f0e8a8e` | 1792346598 |
| 67 | `openmir2-rm-delaymagic-six-ids` | A | whitelist T-81,T-83 | `f3baa06289f91fbb3e22530ef850f255e69fea10` | 1792351304 |
| 68 | `openmir2-rm-delaypushed` | A | whitelist T-81,T-83 | `ebf842f5a4bc0374de75314bb736fb9609d89805` | 1792351314 |
| 69 | `openmir2-player-drop-useitems` | A | whitelist T-81,T-82 | `597466041458bdc8334896ffd77dd4853c2aa400` | 1792351322 |
| 70 | `openmir2-player-dir-supply` | A | whitelist T-81,T-84 | `328bf6495c10510e715df79442b90b209e695fbf` | 1792351331 |
| 71 | `openmir2-run-gap-mb4` | A | approve.window_required=true | `9b999ec51965b2c1234058cf3395c668be597799` | 1792384781 |
| 72 | `openmir2-run-gap-pl1` | A | approve.window_required=true | `74a11d3370f9e6af898eba4060ed62debf8826a7` | 1792391561 |
| 73 | `openmir2-monster-attack-hit-frame` | A | approve.window_required=true | `dac70c83ef2dcfbf59c522cd8ed3cc8df21cfb70` | 1792392954 |
| 74 | `openmir2-healthspell-emission-model` | A | approve.window_required=true | `b2140b8dae6db8b5136ef2d0f6cafd5c2d6882df` | 1792399204 |
| 75 | `openmir2-gategate-speed-rules` | A | approve.window_required=true | `1724943346c69e84e14f073b851b6acef344be20` | 1792400531 |
| 76 | `openmir2-login-frame-order-52-752` | A | approve.window_required=true；whitelist T-80 | `694c9c5165c74b6cf03345e1c08ddf0c77c825e3` | 1792403503 |
| 77 | `openmir2-delmagic-addmagic-chain` | A | approve.window_required=true | `b6c597b779918f39fc1b210fb15790a276505b86` | 1792416757 |
| 78 | `openmir2-run-gap-pl3` | A | approve.window_required=true；whitelist T-85 | `4ec62d9465109e88b9d01a5bfb5b4eb4906f1594` | 1792421339 |
| 79 | `openmir2-login-numeric-gaps` | A | approve.window_required=true | `845b6da6a7f9babb57a7c2636543d02020abd8a2` | 1792425126 |
| 80 | `openmir2-gategate-slots-gm` | A | approve.window_required=true | `f0673ad4c471486919a2d4e2405ca13c03758604` | 1792430550 |
| 81 | `openmir2-run-gap-pl2` | A | approve.window_required=true；线程提及 T-86（review 正文，宽口径） | `fd99dd615dc46da00214c53a4c795c1b8baeb294` | 1792431313 |
| 82 | `openmir2-gate-idle-timeout` | A | approve.window_required=true；线程提及 T-86,T-87,T-88（review 正文，宽口径） | `ded45b19add86ba24a1801731611fcb3a99f2ddd` | 1792431354 |
| 83 | `openmir2-dbsrv-fault-paths` | A | approve.window_required=true；whitelist T-120 | `ce0e7044b50d742a06502a8c44b917ac45d8af37` | 1792461374 |
| 84 | `openmir2-selgate-reconnect` | A | approve.window_required=true | `6f3202cf7de6d47eb051e424bf02c908d045ca79` | 1792461391 |
| 85 | `openmir2-run-gap-pl4` | A | approve.window_required=true；线程提及 T-89,T-94（review 正文，宽口径） | `3a9f56695e160534db7452719bdfafa5ff1cd9c5` | 1792463713 |
| 86 | `openmir2-guild-g2-dispatch` | A | approve.window_required=true | `768738847aca5de741543ba6d70a490b969cb83a` | 1792470737 |
| 87 | `openmir2-upgrade-npc` | A | approve.window_required=true；whitelist T-95 | `aa2384a0ac253beb06cf19a34c7f8a0f5e31f3f0` | 1792480476 |
| 88 | `openmir2-castle-entities` | A | approve.window_required=true；whitelist T-96 | `1d9d08f6fc256079144a38436022f56c3264805b` | 1792483831 |
| 89 | `openmir2-castle-guard-target` | A | approve.window_required=true；whitelist T-96 | `4be5ec56062ae16d81f5e735517062fcfc2cfb01` | 1792488161 |
| 90 | `openmir2-guild-g3-persistence` | A | approve.window_required=true；whitelist T-117,T-9,T-97；线程提及 T-101,T-102,T-96（review 正文，宽口径） | `f4da1725a18140688aa293308a833bbea2a85438` | 1792489476 |
| 91 | `openmir2-chat-channels` | A | approve.window_required=true；whitelist T-118 | `48e92eb74867c2742f8c1fe49b10c94c9f48299e` | 1792494009 |
| 92 | `openmir2-encoding-core-e1-e2` | A | approve.window_required=true | `968fdf5f5d77147049aeb39eacd9e0aa176aa297` | 1792494015 |
| 93 | `openmir2-gategate-keepalive` | A | approve.window_required=true；线程提及 T-118,T-119（review 正文，宽口径） | `eb82597425bd52f2f8e404fe6486cc86c3272af1` | 1792494021 |
| 94 | `openmir2-dbsrv-scheme-a` | A | approve.window_required=true；whitelist T-120 | `2e3c3cc49b33dfbbca84e76cdf67fb2e6edfcee3` | 1792499316 |
| 95 | `openmir2-castle-war-area-impl` | A | approve.window_required=true；whitelist T-125,T-132；线程提及 T-119,T-128（review 正文，宽口径） | `25530090f2073d5371a916f1395a2580d12b23ee` | 1792499322 |
| 96 | `openmir2-gate-gmclose-race` | B1 | status.work 自述「无待窗口项」 | `d7630aab349497b0b366b4d274d2d383e127b410` | 1792499332 |
| 97 | `openmir2-loader-fields` | A | approve.window_required=true | `804181908e6b6e1a03e797844ecbdef0b2224015` | 1792505913 |
| 98 | `openmir2-mb5-monsters` | A | approve.window_required=true；whitelist T-133,T-135 | `80b2ddbc805c54b28d11247c78680bca5c75f3cb` | 1792506363 |
| 99 | `openmir2-guild-official-npc` | A | approve.window_required=true；whitelist T-137；线程提及 T-133,T-139,T-140（review 正文，宽口径） | `5fb4d222091f83f8e0bf70d0ab4e2534e4874158` | 1792507704 |
| 100 | `openmir2-hutil32-parity` | A | approve.window_required=true | `9f23251447aee6a5c535fcd518e0b26287a0367b` | 1792507716 |
| 101 | `openmir2-mb2-monsters` | A | approve.window_required=true；whitelist T-141 | `884c097f0b3cc541ff254e702bf17d99c3f70199` | 1792514693 |
| 102 | `openmir2-encoding-crate` | A | approve.window_required=true | `97722dbb35434e50c9b0ff58cc6cdfbe7cdffd5d` | 1792515020 |
| 103 | `openmir2-script-playdice-openybdeal` | A | approve.window_required=true；whitelist T-142；线程提及 T-141（review 正文，宽口径） | `cc2d69e405320a5e26a2d295c01e059e504a91ea` | 1792515567 |
| 104 | `openmir2-login-654-order` | A | approve.window_required=true；线程提及 T-80（review 正文，宽口径） | `06669b53aaf8acea4ab857df35a24d1425d4c085` | 1792515972 |
| 105 | `openmir2-mb7-monsters` | A | approve.window_required=true；whitelist T-143,T-144 | `ed118ed86262404a08b77ed1128ff0fa0d6c76a1` | 1792519136 |
| 106 | `openmir2-strlist-bom` | A | approve.window_required=true | `d974eab3a60c5fda00b508440ad6f2026e0f6b72` | 1792519445 |
| 107 | `openmir2-account-protection-13` | A | approve.window_required=true | `f940fabbb4f466b783ef4aac42127e10b50b8df4` | 1792519484 |
| 108 | `openmir2-market-def-impl` | A | approve.window_required=true；whitelist T-145；线程提及 T-143（review 正文，宽口径） | `91fb3c2d346ed5c723515f7aa655e9986e8c1ea3` | 1792520767 |
| 109 | `openmir2-castle-pure` | A | approve.window_required=true | `3ed41194bd31794055090eeba4fe3f8a5312c033` | 1792546368 |
| 110 | `openmir2-db-encoding` | A | approve.window_required=true | `a3db0e590387350035d5546b7130ab854b92004e` | 1792546384 |
| 111 | `openmir2-login-charstatus-657` | A | approve.window_required=true；whitelist T-147；线程提及 T-146（review 正文，宽口径） | `ba448557844e551d4e547b6ae551029a285d2ff7` | 1792563445 |
| 112 | `openmir2-monster-walk-frames` | A | approve.window_required=true；whitelist T-146 | `82f80d585920c225a52a992b81a9162ae8b24dd3` | 1792563461 |
| 113 | `openmir2-mb8-9-monsters` | A | approve.window_required=true；whitelist T-148 | `356a4a671fccf6e0250a55c03cc2d142a87994ed` | 1792565254 |
| 114 | `openmir2-superman-normnpc` | A | approve.window_required=true；whitelist T-31 | `9981eccf5890c0d0f3f1055e5a17f2731cd1a93c` | 1792570337 |
| 115 | `openmir2-openybdeal-host-wiring` | A | approve.window_required=true；whitelist T-142 | `0b9fd4b7c62ae77f2c0929ed8caea828a28ecfe0` | 1792571604 |
| 116 | `openmir2-npc-def-loading` | A | approve.window_required=true；whitelist T-137,T-140；线程提及 T-40（review 正文，宽口径） | `2d8393ecfac5352a6abce6ec1602cba8710b029f` | 1792575732 |
| 117 | `openmir2-merchant-race-bias` | A | approve.window_required=true；whitelist T-31 | `125148cb68510fb8e35a119b5b308e329d3a5296` | 1792581324 |
| 118 | `openmir2-a09-upgrade-item` | A | approve.window_required=true；whitelist T-95；线程提及 T-24,T-47（review 正文，宽口径） | `fdc3d38591c818e60cd73a8d9ec3e4a756fd2e1d` | 1792581335 |
| 119 | `openmir2-single-binary-roles` | A | approve.window_required=true | `c93a120d8ff1c64bd5d8f71822fc233d61bd1bb5` | 1792595544 |
| 120 | `openmir2-npc-display-table` | A | approve.window_required=true；whitelist T-137,T-154 | `e5309c4a0b4ed9d9cf515200f37ac0361fffd5ad` | 1792603157 |
| 121 | `openmir2-t138-request-castle-war` | A | approve.window_required=true；whitelist T-138 | `70ca1fa93642c01204e6fafc2fa091065ea27bd0` | 1792603165 |
| 122 | `openmir2-npc-ctor-constants` | A | approve.window_required=true；whitelist T-155 | `0015a94fa9602a918640248a6e17dc90d7e60292` | 1792603182 |
| 123 | `openmir2-a22-castle-persistence` | A | approve.window_required=true；线程提及 T-152,T-153（review 正文，宽口径） | `dfa57c70d0cba192e6529f769fe361515ebb1ae9` | 1792603184 |
| 124 | `openmir2-guard-ctor-constants` | A | approve.window_required=true；线程提及 T-155（review 正文，宽口径） | `995cd6ffa1b714bca65d91053a8327baf9d8170c` | 1792633708 |
| 125 | `openmir2-monster-ref-drains` | A | approve.window_required=true；whitelist T-134,T-135,T-148 | `68c66c88d8029ffa2caedc00efbdff92fa71f5ac` | 1792644097 |
| 126 | `openmir2-npc-hit-frames` | A | approve.window_required=true；线程提及 T-154（review 正文，宽口径） | `477e852f6036d9b6cc00e9536593f3496e673c57` | 1792659899 |
| 127 | `openmir2-weapon-upgrade-values` | A | approve.window_required=true；whitelist T-95 | `7ceff41cd9067918b47d22c185c93c7e250168e1` | 1792661888 |
| 128 | `openmir2-script-take-persist` | A | approve.window_required=true；whitelist T-157,T-158,T-159；线程提及 T-142,T-160,T-19,T-36（review 正文，宽口径） | `a56850e98bee5a4ace8bf993c6ab9545f4028bbf` | 1792673855 |
| 129 | `openmir2-action-line-variable-text` | A | approve.window_required=true；whitelist T-161,T-165；线程提及 T-36（review 正文，宽口径） | `efe0e661ae54f9f84a3231be36a6c668b603cd4d` | 1792677344 |
| 130 | `openmir2-click-path-bag-writeback` | A | approve.window_required=true；whitelist T-160；线程提及 T-157,T-158,T-159（review 正文，宽口径） | `816451f140b4b0f040a63ec5db5d92f94c198afa` | 1792680183 |
| 131 | `openmir2-gm-commands-design` | A | approve.window_required=true；whitelist T-161,T-162,T-163,T-164,T-166,T-167,T-168,T-169,T-170,T-171；线程提及 T-156,T-172（review 正文，宽口径） | `eb6a5166439f22955578ed2e08118e57fb72c501` | 1792681528 |
| 132 | `openmir2-login-529-status-text` | A | approve.window_required=true | `98b65509d72ecc2a4de7f9c6f60481396172dc4e` | 1792691228 |
| 133 | `openmir2-searchtick-origin-parity` | A | approve.window_required=true；whitelist T-184,T-73；线程提及 T-174,T-181,T-183（review 正文，宽口径） | `267dd796e2220e644f07c1242b2f8596494e8cd2` | 1792693754 |
| 134 | `openmir2-verify-tick-base` | A | approve.window_required=true；whitelist T-73；线程提及 T-184（review 正文，宽口径） | `a1c015e8a2effd21d2f6f0dd630f2d580f4b06fe` | 1792695708 |
| 135 | `openmir2-sm-closehealth-wire` | A | approve.window_required=true；whitelist T-177,T-79,T-89；线程提及 T-174,T-183,T-36（review 正文，宽口径） | `f55ebe4ec02abe09d1c691c4a7101cbcc5cecaa2` | 1792696829 |
| 136 | `openmir2-unbindlist-consumer` | A | approve.window_required=true；线程提及 T-17（review 正文，宽口径） | `59b95691d3433a8f361c7b373692a9f5a84ee388` | 1792698704 |
| 137 | `openmir2-setoffline-field-writeback` | A | approve.window_required=true；whitelist T-100,T-86 | `3e386a8ad569bc60a355c9090d5004490dc571e0` | 1792699799 |
| 138 | `openmir2-monster-showhp-53-broadcast` | **U（未合并/等返工）** | **无 merge_result；patch head 7721c8bbc/5e29dc369 均非 master 祖先；最近 review=request_changes（R3-1/R3-2 Important）**；窗口项自 patch body 摘录 | `ffc6f32b248b9db81df50f6ba9523c1a2029351b`（纠正）；原误判条目 `0f711d85` | 1792700946 |
| 139 | `openmir2-gamesvr-serverinfo-report` | A | approve.window_required=true；whitelist T-101,T-6,T-7,T-8,T-9；线程提及 T-1,T-2,T-3,T-4,T-5,T-65（review 正文，宽口径） | `ffce97c44a3408a427c48604dae7088bdd8ac839` | 1792701044 |
| 140 | `openmir2-setmapmode-dechp` | A | approve.window_required=true；whitelist T-85 | `93fa31b3f7d1a57d39f302bf3f4afe24e7a9b2f6` | 1792705282 |
| 141 | `openmir2-gm-batch-4-numeric` | A | approve.window_required=true；whitelist T-174,T-179,T-186,T-187,T-188,T-189,T-190,T-191；线程提及 T-172,T-173,T-175,T-176,T-177,T-178,T-183,T-184（review 正文，宽口径） | `aa3078317cf522f7833d875f41edbbde11fa04d5` | 1792705514 |
| 142 | `openmir2-door-chain-wiring` | A | approve.window_required=true；whitelist T-176,T-65；线程提及 T-18,T-180,T-184,T-186,T-191（review 正文，宽口径） | `e9db25706cf89d51ff5a8929f10bfa1ef2b2aa24` | 1792706189 |
| 143 | `openmir2-castle-merchant-fixedhide-hit` | A | approve.window_required=true；whitelist T-192；线程提及 T-154,T-193（review 正文，宽口径） | `0e339b8a92cc17022d9b157b313ad69805e0bb0e` | 1792709200 |
| 144 | `openmir2-ya52-menu-ok-missing` | A | approve.window_required=true；whitelist T-142,T-193；线程提及 T-192（review 正文，宽口径） | `3aeb7919f548dd3313dd429ace4a798860ce8787` | 1792710398 |
| 145 | `openmir2-menu-ok-unify-pickwire` | SKIP | approve.window_required=true；whitelist T-142,T-193,T-194；最近 status <1h，有人在跟 | `—` | 1792712406 |
| 146 | `openmir2-castle-merchant-disappear-downlink` | SKIP | approve.window_required=true；whitelist T-142,T-192,T-193；线程提及 T-154（review 正文，宽口径）；最近 status <1h，有人在跟 | `—` | 1792712500 |
| 147 | `openmir2-gm-admin-mode-entity-seam` | SKIP | whitelist T-193,T-194,T-195；线程提及 T-187（review 正文，宽口径）；最近 status <1h，有人在跟 | `—` | 1792712935 |

## 2. A 类逐条依据说明

A 类每条 `status.work` 形如：
```
【R 级记账补登 20261023 · openmir2-worker-43】已合并待真机窗口（tier=R，不 closed）。窗口项：<逐项>。登记依据：approve.window_required=true；whitelist T-xx。
```
其中 79 张的窗口项直接取自 `review.window_items`；其余 29 张只有 `window_required:true` 或只有 T-* 关联、没有 `window_items`，窗口项取原 `status.work` 的「余量/剩余/待窗口」段原文。

## 3. B1 建议 closed 清单

- `openmir2-gate-gmclose-race`：原 status.work 自述『无待窗口项；按 R 级规则不 closed，观察点=长跑下控制帧 flush』；tier_why=时序/竞态，但该片为控制帧 flush 与进程退出，卡面没有可判的真机窗口项。

## 4. B2 缺登记清单（保留 in-progress，等窗口）

这些卡确实要真机窗口（`status.work` 明写「真机窗口/真机两侧同序列 diff=0/W0/W9」等），但 approve 没写 `window_required`、whitelist 也没有 T-* 关联 ⇒ 机判读不到。已在卡上留一条 comment 摘出原窗口项，建议窗口前先补一条 `status.work`。

| 卡 id | 原窗口项摘要 |
|---|---|
| `openmir2-buff-producer-wiring` | 余量=真机窗口项 + 新开片：四块 snap 改用忠实镜像 `cetin_near_xy`（复验新发现的格子层差异）+ 补窗口宽度上界牙。 |
| `openmir2-default-arm-spell-surface` | 余量：① 真机逐帧对拍（窗口项）；② 效果体层未接（MP 扣减 + `RM_HEALTHSPELLCHANGED` 先于 RM_SPELL ⇒ 仍缺一帧、相对序前移；版本门无生产调用点；`RM_MAGICFIRE`/训练尾含 `Random(3)` 抽数 + 落库；45 条 case 效果体；`RM_MAGICFIRE |
| `openmir2-snap-cell-fidelity` | 余量：① 真机窗口项；② 两个前置（.map 格子属性未灌进 world.maps；apply_structure_events 无生产调用方）——卡 openmir2-mapcell-attributes-loading；③ 补「异图 actorspell 不得 snap」端到端用例并修/删牙③ 的过强注释；④ 召唤 |
| `openmir2-recall-slave-shift-live` | 余量：① 真机窗口（W0 §2e：两侧 RNG 账本 `VERDICT IDENTICAL` + 回召落点同坐标）；② 同窗登记的 `merchant_run → map_random_move` 实为**真缺口**（oracle 移位环继续抽数）⇒ 卡 `openmir2-map-random-move-shift- |
| `openmir2-map-random-move-shift-loop` | 余量：① 真机窗口（商人到点移动：RNG 流水 + 落点）；② 登记未做：201 轮回滚的 `DeleteFromMap`/`AddMapObject` 语义（现只 return）、落格 `CellType`、`SpaceMove` 出帧面。 |
| `openmir2-enter-shift-loop` | 余量：① 真机窗口（墙格出生登录：RNG 流水 + 落点；默认零抽）；② HomeMap 不可解析分支的 oracle `goto ReGetMap` 语义未建模（当前不可达，已登记）；③ `ed9ab035d` 的「墙格出生边缘可达」措辞已按本片订正为宿主路径不可达。 |
| `openmir2-enter-map-cell-rejected-vs-oracle` | 余量：① 真机窗口（切图失败路径；当前生产无 `switch_map` 调用者 ⇒ 真机不可达）；② **帧面**：oracle 在失败前已发 `RM_CLEAROBJECTS`/`RM_CHANGEMAP`、切图路径的 `RM_DISAPPEAR`/`RM_TURN`，本仓未建模（既有约定）；③ `route_ent |
| `openmir2-magic-8-21-wiring` | 余量：① 真机窗口（37 的 Random(20)/Random(2) 流水 + 逐格 SM_BACKSTEP，可与 8 同场景）；② 能量排斥的「37 是否可学/可放」属数据面；③ 本卡另两项既有余量不变：魔号 8 的逐格 SM_BACKSTEP 邻域、魔号 21 的 806 ±12 含自己 + 换图落点；④ 登记未 |
| `openmir2-spell-mp-wrapper` | 余量：① 真机窗口（失败回帧 + MP/53/尾时序）；② 6/13/14/15/16/18/19/36 的护身符/失败门未建（14/15 的 A-5 路径连 CheckAmulet 都未建）；③ `:167` 版本门（`ClientTick` 未建模，设计件在修）；④ 8 个显式 case 的 MP 逻辑；⑤ Sho |
| `openmir2-aoi-verify-maptime` | 余量：① **真机窗口**——同缩窗场景需验 vis>=1、nearest<=vr、capture 出 SM_TURN(281,661) + 配对 SM_FEATURECHANGED、SM_WALK=0（W0-4 复跑）；② DenyRefStatus 未建模（当前恒等价）；③ SearchViewRange 节拍差另 |
| `openmir2-player-searchviewrange-cadence` | 余量：① **真机窗口**——首扫约 1.2s、之后每 1.2s；② 非玩家扫描节拍仍每 tick（另卡）；③ `RM_DISAPPEAR` 收件人近似（误差窗口随节流放大）——应改格扫或窗口验证；④ `GameTimeChanged()` 未建模（既有）。 |
| `openmir2-run-gap-player-run-quantification` | 余量：① **裁定证据**——按 §3 方案取数（B1 静置压血看 53 填充间隔：50/s≈6.0s vs 40/s≈7.5s）；② 若判 50/s ⇒ 改 `stamina.rs` 节拍（50ms→300ms、+2→+15）与 `drain_recovery` 调用节拍；③ A2 时钟注入需 owner 裁决 O3 |
| `openmir2-run-gap-monster-state-expiry` | 余量：① **真机窗口**（CrazyMode 色变 0xF9 / RefNameColor 帧面 / FireHit-TwinHit 到期消息）；② FireHit/TwinHit 写入点依赖未移植的技能层——接线后到期段自动生效；③ `RefNameColor`、`GetChrColor` 色支、`IsAttack |
| `openmir2-run-gap-corpse-ghost-revive` | 余量：① **真机窗口**（3 分钟变尸 / 可复活支 ZenTime-20s / 5 分钟移除 / 两帧 RM_TURN）；② `ReAliveEx` 未建模面（`RecalcAbilitys`、专用怪标志、`StatusChanged`、`MonsterSayMessage`）；③ 怪物下行帧面；④ 复活节拍按 2 |
| `openmir2-movetomovingobject-quirk` | 余量：① **真机窗口**（多对象格移动时的源/目标格身份序列）；② `WorldGolden` 桩的怪物 CellType 仍硬编码 Play（若未来 dump 格内类型需另卡）；③ 该怪癖同样影响走/跑/SpaceMove/召回——本片已全部经同一 `move_object`，窗口对拍需覆盖移动路径。 |
| `openmir2-run-gap-monster-master-follow` | 余量：① **真机窗口**（异图/超距/SlaveRelax 两侧流水）；② 玩家主的 `Dir` 供给另卡 `openmir2-player-dir-supply`；③ 怪物 SpaceMove 出帧与跨服支未建模；④ `CastleDoor.HoldPlace` 特例未建模；⑤ `MonGen.ActiveCoun |
| `openmir2-run-gap-checkdupobj` | 余量：① **真机窗口**——3 人同格推挤的源/目标格身份序列 + AOI 相位（1000ms 口径）；② `player_run` 组在五批 trace 中未出现，需活栈覆盖相对次序；③ `GetStartPoint` per-entity Home 与 `CastleDoor.HoldPlace` 支未建模。 |
| `openmir2-run-gap-monster-specials` | 余量：① **真机窗口**（同 tick 产仔抽数相对次序、`SM_HIT` 帧面、Clone 扣 MP）；② `RM_RANDOMSPACEMOVE`/`RM_DELAYPUSHED` 投递未建模；③ `CloneMonster.Run` 追击/闪电攻击；④ `SendRefMsg` 冷路径既有偏差。 |
| `openmir2-disappear-recipients-gridscan` | 余量：① **真机窗口**（节流窗口内进入 ±12 的观察者、离世广播与 AOI/移动帧的全局次序）；② 冷路径 vs 缓存的既有偏差；③ `ObMode/FixedHideMode` 只发自己未建模。 |
| `openmir2-run-gap-pr1-head-blocking` | 余量：① 真机窗口；② 跨 actor 泵序（首次入队序 vs 实体序）；③ 宿主 `pending_mag_struck` 排水序与 7 参 `SendStruckDelayMsg` 生产者（MB-9）。 |
| `openmir2-run-gap-mb1` | 余量：① **真机窗口**（race 101/95 各 ≥1、R38 抽样）；② `CellType.Event` 格子对象未落格；③ 通用怪 `RM_HIT` 未接；④ `MeltStoneAll` 的 102 属 MB-3；⑤ `EventProcessor` 20ms 节拍。 |
| `openmir2-monster-view-throttle` | 余量：① **真机窗口**；② GuardUnit 的 6 格/CanPassiveAttack；③ 自扫相位差 ≤1 tick；④ parity 驱动不启 AI；⑤ 多玩家投递序。 |
| `openmir2-persist-create-rows` | 余量：① 真机窗口（建角/兜底建角往返）；② `name_quick`/`name_text` 分裂的同名判重（另卡）。 |
| `openmir2-persist-useitems-wiring` | 余量：① **真机窗口**（耐久磨损/破零/存档往返）；② 破零后的 oracle 收尾未接（能力重算/`SendDelItems`/`FeatureChanged`/`RM_DURACHANGE`）；③ `DAMAGEARMOR` 1.2× 既有登记。 |
| `openmir2-persist-item-attr-and-gold` | 余量：① G-01 功能上仍不落库（oracle 门决定，`B-117`）；若要修，单开批次（upsert + 按 MakeIndex 关联 + 读全 14 列）；② G-02 仍丢失（`B-116`，schema 冻结）；③ 真机窗口（建角/存档往返，虽实测零语句）。 |
| `openmir2-run-gap-mb3` | 余量：① **真机窗口**（107/105/102/112）；② **`SendStruckDelayMsg` 延时面**（归 `openmir2-rm-refmessage-delay-layer`）；③ `flush_sculture_spawns` 排水时序；④ `GuardUnit.IsProperTarget |
| `openmir2-save-on-shutdown` | 余量：① **真机窗口**（`DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；并裁决 D-12 的「oracle 是否真的落库」）；② `CTRL_CLOSE_EVENT` 5s 压缩；③ 请求排序口径；④ 命名管道无鉴权。 |
| `openmir2-rm-refmessage-delay-layer` | 余量：① **真机窗口**（200ms 后二次 `Struck` 的全局帧序）；② 未接的 7 参调用点（AttackDirect/Fireball/ExplosionSpider/DoubleCritical/MagCow/SpitSpider/ElectronicScolpion/BoneKing/CowKing） |
| `openmir2-active-seek-race-missing` | 余量：① 真机窗口；② 这些 race 的 `AttackTarget` 覆写属 `openmir2-attacktarget-overrides`；③ 118/130 数据不可达。 |
| `openmir2-proper-target-unfolded` | 余量：① 真机窗口（攻击已死目标的抽数流水）；② 其余折入版调用点（特殊怪覆写/显示路径）未扩大范围。 |
| `openmir2-search-target-clear-target` | 余量：① 真机窗口；② 其余折入版调用点未扩大范围。 |
| `openmir2-attacktarget-overrides` | 余量：真机窗口（82/119/130/91/107/105）；114/117/118/215 不可达；`UndeadPower` 口径另卡。 |
| `openmir2-persist-single-sided-columns` | 余量：真机窗口；失败移动后 `DisplayTable.dir` 未随实体（建议单卡）。 |
| `openmir2-db-error-reply` | 余量：真机窗口（失败注入的 C# 对拍）。 |
| `openmir2-save-misc-gaps` | 余量：真机窗口；G11 传输面未移植；G7 读到一半失败未构造。 |

## 4b. U 类：未合并 / 等返工（1 张，复审 F-1 纠正）

`openmir2-monster-showhp-53-broadcast` 此前被 sweep 误判为 A（依据是某条 review 带 `window_required:true`，但那条不是 approve）。复审实测：

- 线程 **无任何 `merge_result`**；
- 两个 patch head `7721c8bbc8e203eaf3b1d40a82d4aaf450384c02` / `5e29dc3697712e665de3c4736a3005bdd6ce29fa` 均**不是 master 祖先**（`git merge-base --is-ancestor` 退出码 1）；
- 最近一次 review 是 **`verdict=request_changes`**，含 R3-1（Play 源 ObMode/FixedHideMode 自见 53 少 1 条）、R3-2（±12 边界缺机器判据）两条 Important，orchestrator 已裁定 R3-1 必须修。

已补纠正性 status `ffc6f32b248b9db81df50f6ba9523c1a2029351b`：owner 保持 `openmir2-worker-4`、worktree `wt-probe`、branch `showhp53`，**status 恢复 `needs-review`**，并写明「此前 sweep 条目误判为已合并，本条目纠正」；真实窗口项从 patch body 摘录为「观察者帧集合 / ±12 边界 / 与 1100 同时段顺序」。

**全量同型扫描**：对 144 张 sweep 卡逐卡重读线程，判据「有 patch 且 patch head 非 master 祖先且无 merge_result」⇒ **仅此 1 张**；其余 143 张均有 `merge_result`（其中 142 张的原始 patch head 因 rebase/重放不是当前 master 祖先，但 merge_result 在场，不算未合并）。

---

## 附录 A：merged-not-closed 但无任何 tier 声明（16 张，未动作）

AGENTS.md 规定「缺 tier 的旧 approve 按 R 处理」。这些卡 issue/review 均无 `tier`，本表不臆造分类，只登记为待补 tier：

| 卡 id | 最近 status ts | 最近 owner | 备注 |
|---|---|---|---|
| `openmir2-m3-drop-exp-align` | 1791875689 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-final-acceptance` | 1791852056 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-m3-fixture-matrix` | 1791876183 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-m3-guild-castle-align` | 1791875675 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-hero-pet-align` | 1791875680 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-mail-auction-align` | 1791875685 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-npc-shop-align` | 1791875659 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-party-trade-align` | 1791875670 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-party-trade-fixture-botsrv` | 1791864637 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-m3-quest-align` | 1791875665 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-quest-fixture-botsrv` | 1791864563 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-m3-quest-persist-observability` | 1791880333 | openmir2-worker-21 | issue/review 无 tier 字段 |
| `openmir2-m3-scoped-diff-ability` | 1791875652 | openmir2-svc-1 | issue/review 无 tier 字段 |
| `openmir2-m3-selloff-fixture-botsrv` | 1791864710 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-m3-shop-scenario-authority` | 1791876566 | openmir2-worker-5 | issue/review 无 tier 字段 |
| `openmir2-world-bevy-migration` | 1791708805 | openmir2-worker-14 | issue/review 无 tier 字段 |

## 附录 B：残留物与复现命令

- 投递异常（已披露）：`openmir2-run-gap-mb4` 首次 `--push` 遇远端 500，返回非零但本地已建 ref；执行器按重试逻辑补发一条同 content 条目，造成**本地**重复。远端只收到第二条（`9b999ec5`，现为 head）；未推送的本地孤儿 ref `295840a9` 已 `git update-ref -d` 删除。**共享真源上该卡仍只有一条**。其余 143 张单次成功，0 失败。
- 临时物：`E:/tmp/w43-board.json`、`E:/tmp/w43-threads/`（203 份线程快照）、`E:/tmp/w43-R3.json`、`E:/tmp/w43-plan.json`、`E:/tmp/w43-posted.json`、`E:/tmp/w43-exec.py`。属分诊中间产物，保留至本卡复核后清理。
- 条目计数口径（回应复审 F-2）：`refs/collab/inbox/openmir2-worker-43` = **147 refs / 145 distinct**；其中本卡自身 3 条（claim / patch / status），其余 144 张目标卡**各恰好 1 条、0 重复**；加上本次纠正条目共 148 refs / 146 distinct。
- 复现：
```sh
walgit collab board --format json > /e/tmp/w43-board.json
walgit collab thread <card-id> --repo .   # 逐卡读，取 issue/review.tier、review.window_required、status.work
grep "^| T-" mir2-rs/tests/parity/whitelist.md   # T-* 与卡片的关联
```

> **未做**：未 closed 任何卡；未改代码/白名单/board.toml；未动其他 agent 的 worktree 或 inbox。本线只写本文件 + 各卡一条签名条目。
