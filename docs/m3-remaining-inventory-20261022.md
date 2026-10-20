# M3 剩余工作清点与窗口就绪报告（2026-10-22）

- **卡片**：`openmir2-m3-remaining-inventory`（tier=L，纯文档 + 只读）
- **基线**：`master = c83a9ae24`（本报告写作时的主检出 HEAD）
- **写集**：仅本文件（`docs/m3-remaining-inventory-20261022.md`）；未改 `src/**`、`sql/**`、`MirClient/`、`mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起任何服务/客户端/代理；未占端口；未写数据库。
- **输入**：walgit 看板（`walgit collab board --format json`，2026-10-20 快照）、`mir2-rs/tests/parity/M3-report.md`、`mir2-rs/tests/parity/m3/plan-run-gap-batches-20261018.md`、`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md`、`mir2-rs/tests/parity/m3/audit-*.md`、`mir2-rs/tests/parity/whitelist.md`、`docs/window-run-sheet-20261021.md`、`docs/window-owner-decisions-20261018.md`。
- **口径**：
  1. 只写「有文件/卡号/提交可追溯」的条目；查不到的写 **UNKNOWN**，不猜。
  2. 看板 `work` 字段是**最后一条 status 的快照**，部分条目的文字早于当前 master（例如 `m3-*-align` 的 `窗口 r3 (master d4d5b89c)`）。凡是看板文字与当前源码冲突的，本报告以**只读源码复核**为准，并在「注」中标明看板文字可能 stale。
  3. 判级沿用 `AGENTS.md`「判据分级」；判不准按 R。
  4. 「可达性」指当前部署内容（`E:/MirServer`）下是否真的会被触发；「部署不可达」不等于「无需实现」，仍按 R 登记。

> **本报告不得被读成「M3 已达成」**。M3-report 的十项状态仍为 3 covered / 2 partial / 1 oracle_absent / 5 not_covered；本报告回答的是「还剩什么」与「下次开窗最少要什么」。

---

## 0. 结论摘要

1. **代码实现已大幅领先 M3-report**：看板 156 张 in-progress 卡中，**95 张已合入 master、因 R 级规则不 closed**（等真机窗口）；真正仍在实现中的只有 5 张（`loader-fields`、`hutil32-parity`、`mb5-monsters`、`encoding-callsites`、`guild-official-npc`），在飞 3 张（`run-gap-mb2`、`spell-shared-tail`、`window-checklist`），设计 2 张（`castle-official-design`、`castle-tax-item-design`）。
2. **当前开窗的第一阻塞不是代码，而是机器前置**：`docs/window-run-sheet-20261021.md §0.2` 实测 **R6/R7/R8 三红**——镜像 `f3675f69aa` 落后基线 `c83a9ae24` 的 **93** 件生产路径（实跑 guard：`python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_rust_window_preflight.py --repo E:/tmp/m2svr-window-origin --ref HEAD --baseline-ref c83a9ae24 --target-dir ''` ⇒ `RED R6: 构建源 HEAD=f3675f69aa 落后于基线 c83a9ae24@c83a9ae248 的生产路径（93 件：…）`；原「66 件」为旧读数）、`E:/tmp/m2svr-window-target` 与 `window-provenance.json` 不存在、九个游戏端口全部 LISTENING。三项修绿前不应开窗。
3. **第二阻塞是 owner 授权边界 E1–E4**：E1 未裁决则所有构造（都会写库）不可开窗；E4 未裁决则所有依赖 C# oracle 的块不可开窗；E3 只影响 G6；E2 影响行会 750、特殊怪 MonGen、非 TestServer 余量。
4. **剩余未实现面已收敛**（A 表 22 条；其中 A-04 `account_protection` 13 字段已在分支 `account-protection` 实现、只剩真机窗口证据）：行会/城堡 NPC 标签派发、城堡税接线、`OPENYBDEAL`、GM `@` 命令面、`Market_Def` 商人脚本装载、`GetUpgradeStdItem` 升级差量/自定义名、`SETOFFLINE` 的自动经验四字段、18 个数据不可达怪物的 `Run` 覆写（MB-5..MB-9）、MB-2（在飞）、怪物 AI 移动的 `SM_WALK` 下行、编码 E-3..E-8（在飞）、LB-10 持久化证据、M3 系统级夹具驱动、PL-3 的三个残余项、`CastleMgr` 持久化排水。
5. **窗口内当前可收的组只有 D.1/G1、D.2/G3a、D.3/G2a+G5a**；G4（法术驱动）、G6（6000/5100 抓包 + MySQL 停起）、G7（网关真机 capture）、G8（内容周期）、G9（整包 W0）、G10（夹具链）、G11（24h 长跑）在本报告写作时**不具备收口条件**（工具/授权/内容未齐）。
6. **数据不可达面不应占用窗口**（C 表 20 条）：18 个怪物 race、`RunAway`、`WhiteSkeleton`、P07/P22/P25/P26/P27/P31 的部署内容、邮件、5700 市场、11 张死表、攻城相关死面等，全部只登记、不排期。
7. **M3 系统级 `m3-*-align` 卡的最后看板文字明显 stale**：⑦/⑩/⑧/⑨ 的 `run_*_pair.ps1` 当前都在仓内，`quest_state_producer.py` 也已存在；④ 的 `run_drop_exp_pair.ps1`（543 行）与 BotSrv 固定掉落/经验 100 杀驱动（`-DropExpSequence` / `--selftest-dropseq`）**也已在仓内**（缺口只剩「0137 上运行时 pin 出单只固定怪」，归 `openmir2-m3-rng-scene-driver`）；⑥ 的 marker 前缀与 C# k=0 截断仍标 BLOCKED。**下一次开窗前必须重跑这些卡的静态 readiness，不能按 stale 文字排期。**

---

## A 表：未实现 / 未接线面（22 条）

> 索引列优先引用 `audit-uncovered-surfaces-20261019.md` 的 `#`、`plan-run-gap-batches-20261018.md` 的批次号、`whitelist.md` 的 `T-*`。Rust 侧只引符号/文件路径，不写行号（并行车道会位移）；oracle 侧给文件:行号。

| # | 面 | 索引 | oracle 入口 | Rust 现状 | 可达性 | tier | 建议批次 / 卡 |
|---|---|---|---|---|---|---|---|
| A-01 | 行会 NPC 标签：`@@buildguildnow` / `@@guildwar` / `@requestcastlewarnow` 的 UserSelect 四支 | audit #3 | `src/GameSrv/Npc/GuildOfficial.cs:55-90`（UserSelect）、`:112-178`（ReQuestBuildGuild）、`:180-201`（ReQuestGuildWar）、`:206-233`（ReQuestCastleWar） | `crates/world/src/npc.rs` 只有 `GuildOfficial.Run` 的转身/抽数；无 UserSelect、无 `BuildGuildPrice`/`WomaHorn`/`AddGuild` 副作用链；`crates/script` 无 `@requestcastlewarnow` 消费点 | 部署明确可达：`Envir/Npc_Def/比奇国王-0122.txt:6/7/146/189` | L | 既有卡 `openmir2-guild-official-npc`（看板：实现中 wave7） |
| A-02 | 城堡 NPC 标签：`@openmaindoor` / `@closemaindoor` / `@repairdoornow` / `@repairwallnowN` / `@hirearchernowN` / `@@withdrawal` / `@@receipts` 的 UserSelect | audit #6 | `src/GameSrv/Npc/CastleOfficial.cs:93-406` | `crates/world/src/castle.rs` 已有 `castle_official_action` 纯函数与 repair/cost；`bins/game-svr/src/npcs.rs` **无** CastleOfficial 标签派发（只在测试里出现 `NpcType::CastleOfficial`） | 部署可达（对话文本）：`Envir/Npc_Def/沙巴克城堡管理-0150.txt:32/35/40/55/61-75`；N1 金库支为 no-op（T-38） | L | 设计卡 `openmir2-castle-official-design`（起草中）；设计合入后另开实现卡 |
| A-03 | 城堡税 / 成员价接线：商人卖出/修理按 `Castle.IncRateGold` 计税、`CastleMemberPriceRate=80` | audit #8 | `src/GameSrv/Npc/Merchant.cs:544-552/1250-1258/1385-1393/1637-1645`、`Merchant.cs:765-782`、`UserCastle.cs:768-812` | `crates/world/src/castle_tax.rs` 已有税公式；`bins/game-svr/src/shop.rs` 三处仍是 `let _tax = ...` **丢弃**（注释：城堡对象未建模） | 仅在沙巴克商人交易时可达；当前税被丢弃（零效果） | L（接落盘时升 R） | 设计卡 `openmir2-castle-tax-item-design`（起草中）；实现另开 |
| A-04 | `account_protection` 字段级创建/更新（13 列） | audit #14 | `src/LoginSrv/Storage/AccountStorage.cs:295-310`（INSERT）、`:498-513`（UPDATE） | **已实现（分支 `account-protection`）**：`crates/storage/src/account.rs` 的 INSERT 13 列、UPDATE 12 列 + `WHERE AccountId` 与 oracle 逐列同序同值（含 INSERT 的 `Phone`/`MobilePhone` 取 `BirthDay`、UPDATE 取 `MobilePhone` 怪行为）；`bins/login-srv` 的封包解析/记录映射补齐 13 字段，`CM_UPDATEUSER` 的 `PassWord` 按 oracle 覆盖封包值。**真机 R 级证据未执行**：`mir2-rs/tests/parity/m3/account-protection/`（`account_protection_roundtrip`，`window_required: true`） | `LoginSrv/config.conf` 启用建号（`EnableMakingID`）时客户端可达 | R | 卡 `openmir2-account-protection-fields`（本分支）；窗口跑绿前不 closed |
| A-05 | 脚本命令 `OPENYBDEAL`（元宝寄售入口） | audit #16 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:125/2329-2356`；`PlayObject.Sale.cs:387-429` | `crates/script/src/handlers.rs` 只登记 `(342, "ActionOfOpenSaleDeal")`，`actions.rs` 无实现分支；`bins/game-svr/src/selloff.rs` 模块头明写未移植 | 部署 `Envir/` 无 `OPENYBDEAL`/`dealybme`（grep 0）⇒ 当前内容不可达；内容回归即可见 | L | 新卡（元宝寄售世界侧已由 `openmir2-m3-selloff-consignment-world` 合入，本项是其脚本入口残余） |
| A-06 | GM `@` 命令面（`@LetGuild` / `@LetTrade` / `@GroupRecall` / `@GuildRecall` / `@AllowGroupReCall` / `@AllowGuildReCall` 等） | audit #17 | `src/Modules/GameCommand/Commands/*.cs`；`GameCommands.cs:74-79/367-368`；`Command.conf:12/14` | `bins/game-svr/src/chat.rs` 模块头明写 `@` 命令系统未接；`guild_cmd.rs` 明写 `allow_guild` 唯一生产写点 `@Letguild` 未接 ⇒ 恒 false | 普通客户端不可达，GM/管理端可达 | L | 新卡（建议 `openmir2-gm-command-surface`） |
| A-07 | 跨服/跨进程行会/攻城广播 `SS_205/206/207/211/212` 的发送路径 | audit #19 | `GuildOfficial.cs:147`、`PlayObject.Operate.cs:1399/1200/1442`、`PlayObject.Message.cs:330`、`UserCastle.cs:368` | 协议常量在 `crates/protocol`；`guild_cmd.rs` 会产 `SS_206/207` 事件、`castle.rs` 会产 `SS_212` 事件；**game-svr 无发送路径**，单服无接收端（T-113） | 单区单服部署不可见；跨服部署才触发 | L | owner 裁决 D-08；若只做单服则登记「不实现」 |
| A-08 | `Market_Def` 商人脚本装载 + `GotoLable` 说词 | T-47③ / T-95④ | `src/GameSrv/Npc/Merchant.cs:1089`（LoadScriptFile）、`Merchant.cs:1652/1659`（REPAIROK/SUPERREPAIROK） | `bins/game-svr/src/main.rs` 明写「Market_Def 商人脚本未装载（T-47③）⇒ 只登记、不执行 say」；`shop` 只有标签解析/CanJmp 机制，无说词生产者 | 部署 131 个商人天然可达 | L | 新卡（建议 `openmir2-merchant-script-load`） |
| A-09 | `GetUpgradeStdItem` 的 Desc 升级差量 + `CustomItemSystem.GetItemName` 自定义名（T-24 残余） | audit #15 / T-24 / T-95② | `GameItemSystem.cs:731`、`PlayObject.Base.cs:2383-2403` | `bins/game-svr/src/bag.rs` 模块头明写 `GetUpgradeStdItem` 为恒等拷贝、`Desc[13]==1` 返回空串；`ability.rs` 同 | 交易/装备/寄售栏可达；升级过的武器与自定义名物品字节会不同 | R（判不准按 R；同族 T-95/B-39 已是 R） | 新卡（建议并入 T-95 后续批次 `openmir2-weapon-upgrade-desc`） |
| A-10 | `SETOFFLINE` 的 `AutoGetExp*` 四字段写入 | T-86 | `ExecutionProcessingSys.cs:2723-2726` | `crates/script/src/engine/mod.rs:541` 的 `ScriptPlayer::set_offline` 是**空默认实现**；`bins/game-svr/src/npc.rs:449` 的 `impl ScriptPlayer for NpcPlayer` **未覆写**该方法（`actions.rs:608` 的 `ActionOfOffLine` 调它 ⇒ `AutoGetExp*` 四字段、`OffLineFlag`、`KickOffLineTick` 均不写） | 内容侧 `SETOFFLINE` 若使用即可达；当前内容 grep 未确认 | R | 新卡（PL-2 复验登记项） |
| A-11 | MB-2：`DualAxeMonster`(87/93/104)、`LightingZombi`(94)、`ChickenDeer`(52)、`ZilKinZombi`(96) 的 `Run` + 同批非-Run 覆写 | plan §2.3 MB-2 | `DualAxeMonster.cs:84-137`、`LightingZombi.cs:32-58`、`ChickenDeer.cs:13-63`、`ZilKinZombi.cs:35-47`、`DualAxeMonster.cs:33-72`、`ZilKinZombi.cs:24-33` | 看板 `openmir2-run-gap-mb2`：**车道在飞**（`wf_6845f065-267`） | 真机可达（MonGen 有 87/93/104/94/96/52） | R | 在飞卡 `openmir2-run-gap-mb2` |
| A-12 | MB-5：`GreenMonster`(208)、`RedMonster`(209)、`Khazard`(206)、`FrostTiger`(210)、`TeleMonster`(203) 的 `Run` | plan §2.3 MB-5 | `GreenMonster.cs:13-36`、`RedMonster.cs:13-36`、`Khazard.cs:13-59`、`FrostTiger.cs:14-36`、`TeleMonster.cs:12-31` | 未开始；看板 `openmir2-monster-run-overrides` 明写实现未开、分期 P3 未启动 | 数据不可达（部署 `MonGen` 0 条目） | R | 新批次 MB-5（依赖 PR-1 已落地） |
| A-13 | MB-6：`ElectronicScolpionMon`(200)、`MagicMonObject`(133)、`RonObject`(131)、`ExplosionSpider`(117) 的 `Run` | plan §2.3 MB-6 | `ElectronicScolpionMon.cs:33-70`、`MagicMonObject.cs:23-60`、`RonObject.cs:35-51`、`ExplosionSpider.cs:79-90` | 未开始 | 数据不可达 | R | 新批次 MB-6（PR-1 已落地） |
| A-14 | MB-7：`BoneKingMonster`(134)、`ElfMonster`(113)、`ElfWarriorMonster`(114) 的 `Run` | plan §2.3 MB-7 | `BoneKingMonster.cs:48-76`、`ElfMonster.cs:43-86`、`ElfWarriorMonster.cs:112-166` | 未开始 | 数据不可达 | R | 新批次 MB-7 |
| A-15 | MB-8：`FireMonster`(214)、`SandMobObject`(132)、`SoccerBall`(120)、`RunAway`(死类)、`WhiteSkeleton`(100) 的 `Run` | plan §2.3 MB-8 | `FireMonster.cs:23-82`、`SandMobObject.cs:14-57`、`SoccerBall.cs:19-74`、`RunAway.cs:12-64`、`WhiteSkeleton.cs:29-39` | 未开始 | 数据不可达（R42 为 L，race 100 MonGen 0 条） | R（R42=L） | 新批次 MB-8 |
| A-16 | MB-9：`FireballMonster`(215) 全链（含 `MagicMonster.Run` 两 pass / `Think` 无 HolySeize 门 / `AttackTarget` 不伤害） | plan §2.3 MB-9 | `FireballMonster.cs:14-62`；`MagicMonster` 链 | 未开始；`monster-run-overrides` 的 P1 设计已存在但未实现 | 数据不可达（race 215 部署 0 行） | R | 新批次 MB-9 |
| A-17 | 怪物 AI 移动的 `SM_WALK(11)` 完全不下发 | `audit-monster-attack-cadence-20261019.md §B.1` | `BaseObject.cs:601`（WalkTo）→`:675`（Walk RM_WALK）→`:1525`（Walk）→`:1571`（SendRefMsg）；观察者 `PlayObject.Message.cs:1403-1410` 回 `SM_WALK` | `crates/world/src/monai.rs::walk_to` → `World::try_step_now`/`try_step` 只改 `e.x/e.y`，**不产生事件**；`downlink.rs::move_frame` 只被 `CM_WALK` 使用 | 真机可达；客户端看不到怪移动（服务端坐标与攻击时点不变） | R（判不准按 R；同族 `openmir2-monster-attack-hit-frame` B-08 为 R） | 新卡（建议 `openmir2-monster-walk-frame`）；与已合的 `openmir2-monster-attack-hit-frame` 同族 |
| A-18 | 文本编码 E-3..E-8：DBSrv 字节边界/`arrest`、两套 StringList、`Server.conf`/`String.conf`/`AddrTable`/`UserLimit` 配置口径、DB `CONVERT(... USING gbk)`、网关合成登录包 decode→re-encode | `audit-encoding-v2-20261021.md §3` | `HUtil32.cs`、`StringList`、`ConfigFile`、`PlayDataStorage.Add.cs`、`LoginGate/SelGate ClientSession` 等（审计件逐条给锚） | E-1/E-2 已合入 master（`aff62c980`/`22a2ff847`）；E-3..E-8 由 `openmir2-encoding-callsites` 实现中（wave7）。当前生产语料 639 Envir + 1357 部署文本 + 46102 DB 值**未命中**可达分歧字符 | 当前内容不可达；一旦出现 GBK 域外/PUA/非法字节即改变帧长与字节 | **分项**：E-3/E-4/E-5/E-6/E-8 = **L**（审计件逐条 `tier=L`，均无落库面）；**E-7 = R**（DB `ChrName` 是 utf8 列、`CONVERT(… USING gbk)` 绑定，落库 + 字段级 diff，见 `audit-encoding-v2-20261021.md` E-7 `tier=R`） | 在飞卡 `openmir2-encoding-callsites`（E-7 须占 R 窗口） |
| A-19 | LB-10 持久化证据：9 表 + `characters_indexes` 的跨实现逐字段 diff、NEWCHR/DELCHR/SELCHR 库后态、错误路径故障注入 | `audit-dbsrv-persistence-20261020.md` LB-10 | `WorldServer.cs:1231-1382`、`DataService.cs:151-257`、`UserService.cs:439-735` | 现有 `live_data.rs` 只覆盖 5 表；`scoped_diff.TABLE_COLUMNS` 未登记 `mir2_test_*`（C10#11②）；M3 `characters_ablity` 仍 RED | 每次登录/存档；错误路径需故障注入 | R | `openmir2-dbsrv-scheme-a` + `openmir2-dbsrv-fault-paths` 的证据项 |
| A-20 | M3 系统级夹具/驱动缺口：④ 掉落经验夹具只剩「0137 运行时 pin 单只固定怪」未接；⑥ 任务 marker 前缀 + C# k=0 事件 JSON 截断 + quest 态生产者历史缺口 | `M3-report.md §4`；`openmir2-m3-drop-exp-align` / `openmir2-m3-quest-align` 的最后 status | 见各 align 卡 | **④ 已存在**：`mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1`（543 行；`-SelfCheckNoStack` / `-DiffOnly` / `-PurityOnly` / real-run `-Go`）+ BotSrv 固定种子 100 杀驱动（`-DropExpSequence` → `M3DROPEXPSEQ`，`BotSrv.exe --selftest-dropseq`），经 `run_pair.ps1`/`run_bots.ps1` 透传；缺口 = 0137 上运行时 pin 单只固定怪（`rng-trace-scene/DRIVER-CONTRACT.md §7`，归 `openmir2-m3-rng-scene-driver`），未 pin 时 fail-closed RED；未获 owner `-Go` 拒跑（exit 2）。`run_quest_pair.ps1` 与 `quest_state_producer.py` **现在存在**，看板 r3 文字可能 stale；`openmir2-m3-fixture-marker-prefix-bug` 仍在看板 | ④ 静态夹具/驱动已就绪、real-run 待 pin 怪 + 窗口；⑥ 仍 BLOCKED；**看板文字 stale，开窗前重跑 readiness** | R | `openmir2-m3-drop-exp-align`、`openmir2-m3-quest-align`、`openmir2-m3-fixture-marker-prefix-bug`、`openmir2-m3-rng-scene-driver` |
| A-21 | PL-3 残余：⑤ `ShowScriptActionMsg` 自消息未实现；⑥ `SETMPAMODE DECHP` 未实现；⑦ `HungerDecPower` 是死键 | T-85 | `ExecutionProcessingSys.cs`（197/198/`SETMPAMODE`）；`Server.conf` Hunger 段 | `crates/script/src/engine/mod.rs:347` 的 `ScriptPlayer::sys_msg` 是**空默认实现**，`bins/game-svr/src/npc.rs:449` 的 `impl ScriptPlayer for NpcPlayer` **未覆写**；`actions.rs` 无 `SETMPAMODE DECHP`；`HungerDecPower` 配置面已接但无消费者 | 部署内容全部不可达（MapInfo 0 条 DECHP/INCHP、HungerSystem=0、Envir 0 处 KILLMONEXPRATE/POWERRATE） | R（内容启用后） | `openmir2-run-gap-pl3` 的后续片 |
| A-22 | `CastleMgr.Run/Save` 持久化排水：`SaveAttackSabukWall`、10min `CastleMgr.Save`、`LoadCastleList`、`CastleMapList` 启动 +6、`Broadcast`/`SS_204/212` | T-127 | `src/M2Server/Castle/CastleManager.cs:78-143/145-167`；tick 入口 `SystemProcessor.cs:75-76` | 10s `UserCastle::run` 已接；`Broadcast`/`ServerGroupMsg(SS_204/212)`/`SaveAttackSabukWall`/`RefreshShowName` 只落 DBG 日志 | 单服 `UnderWar=false`（无城主/无申请）⇒ 当前不可达；多服/城主场景可达 | R | 新卡（建议 `openmir2-castle-persist-drain`）；随 castlewar 线 |

**A 表计数：22 条。**

---

## B 表：已实现但待真机窗口的 R 卡（60 行，覆盖 69 张卡）

> 说明：本表只列**已合入 master、R 级、尚未 closed** 的卡。窗口项栏优先引用看板 `work` 的「余量」；未逐项展开的写「见卡 review.window_items」。E 依赖栏：`E1` = 写库边界（生产 vs 克隆）、`E2` = 影子内容（Envir/GuildBase/MonGen/非 TestServer）、`E3` = MySQL 停起、`E4` = 外部 C# 栈身份；`O1-O7` = `docs/window-owner-decisions-20261018.md §2` 的 RNG 场景 pin 决策。工具栏：`C1/C2/C4/B9/C6` 等指 run sheet 的判据工具；「就绪」表示 `--selftest` 已绿、「缺」表示本报告写作时无可用判据或读数。

| # | 卡 id | 窗口项 | E 依赖 | 所需工具 |
|---|---|---|---|---|
| B-01 | `openmir2-smframe-minor-gaps` | 登录帧面逐字节（T-79/T-80 不计入本窗余量） | E1+E4 | C1/C2/B9 就绪 |
| B-02 | `openmir2-login-frame-order-52-752` | 非 TestServer 部署 / 发光装备 / IsSpirit 的槽位变化未采 | E2+E4 | B9、C10#1 就绪 |
| B-03 | `openmir2-login-numeric-gaps` | 52 的 Weight/MaxExp 等 3 项（B2 的 Weight=14） | E1+E4 | C10#1 就绪 |
| B-04 | `openmir2-healthspell-emission-model` | 53 的触发/尾长 4 项 | E4 | C4 部分；C10#3 服务端事件读数缺 |
| B-05 | `openmir2-orphan-encoder-wiring` | 种子断言与生产路径未覆盖项 | E4 | 部分（run sheet D.1 明列） |
| B-06 | `openmir2-window-collector-preflight` | 两侧种子交叉断言 / C# site 列 / 前置门不误拦 | E4 | 部分 |
| B-07 | `openmir2-outbound-encoders-red-batch` | 制药抽数流水 + 帧序 + MYSTATUS 派生落库 | E1+E4 | 缺（G4 无法术驱动） |
| B-08 | `openmir2-monster-attack-hit-frame` | `SM_HIT(14)` 头字段/收件人/250ms 前导 | E4 | C1+C4 就绪 |
| B-09 | `openmir2-monster-attack-cadence-lead` | 同 HP 受控重采样 + 同怪 interval（`recog=SELF` 过滤） | E4 | C4 就绪 |
| B-10 | `openmir2-player-dir-supply` | Dir 变化的可观测帧 | E4 | 就绪 |
| B-11 | `openmir2-aoi-verify-maptime` | `vis>=1`、`nearest<=vr`、`SM_TURN(281,661)`+`SM_FEATURECHANGED`、`SM_WALK=0`（W0-4） | E4 | 就绪 |
| B-12 | `openmir2-monster-view-throttle` | GuardUnit 的 6 格/CanPassiveAttack、自扫相位、多玩家投递序 | E4 | 部分 |
| B-13 | `openmir2-attacktarget-overrides` | 82/119/130/91/107/105 的目标选择与抽数；114/117/118/215 不可达 | E4 | 部分（需构造特殊怪） |
| B-14 | `openmir2-active-seek-race-missing` | 9 个补入 seek race 的真机索敌 | E4 | 部分 |
| B-15 | `openmir2-proper-target-unfolded` | 攻击已死目标的抽数流水 | E4 | 部分（需尸体构造） |
| B-16 | `openmir2-search-target-clear-target` | `ClearTargetCreat`/`RemoveAt` 跳项怪癖 | E4 | 部分 |
| B-17 | `openmir2-run-gap-mb1` | race 101/95 各 ≥1、R38 抽样；`CellType.Event` 未落格 | E4 | 部分 |
| B-18 | `openmir2-run-gap-mb3` | 107/105/102/112；`flush_sculture_spawns` 排水时序 | E4 | 部分 |
| B-19 | `openmir2-run-gap-mb4` | race 92 的 SpaceMove/MapRandomMove 落点与狂暴 500/400 节拍；CastleDoor HoldPlace | E4 | 部分 |
| B-20 | `openmir2-guardlist-guards-loading` | GuardList 帧一致（`SM_TURN(281,661)` 对） | E4 | C3 就绪 |
| B-21 | `openmir2-oracle-humitems-align` | `HumItems` load/save 1:1（B3） | E1+E4 | C6 就绪；E1-a 下只能行数/关键值 |
| B-22 | `openmir2-delmagic-addmagic-chain` | 210/212 两轮 DB 状态（B5） | E1+E4 | 就绪 |
| B-23 | `openmir2-save-on-shutdown` | `DB_SAVEHUMANRCD` 帧与落库 diff、`-Stop` graceful 日志；D-12「oracle 是否真的落库」 | E1+E4 | 部分（E1-a 不能快照/字段级 diff） |
| B-24 | `openmir2-persist-useitems-wiring` | 耐久磨损/破零/存档往返 | E1+E4 | 部分 |
| B-25 | `openmir2-persist-create-rows` | 建角/兜底建角往返 | E1+E4 | 部分 |
| B-26 | `openmir2-save-misc-gaps` | G11 传输面、G7 读到一半失败 | E1+E4 | 部分 |
| B-27 | `openmir2-persist-item-attr-and-gold` | G-01 不落库（B-117）、G-02 丢失（B-116）、建角/存档往返 | E1+E4 | 部分 |
| B-28 | `openmir2-snap-cell-fidelity` | 两个前置未解（评审 D2） | E1+E4 | 缺；本窗不收 |
| B-29 | `openmir2-run-gap-pl1` | 装备复检 + 复活戒指 4 项 | E4 | 部分 |
| B-30 | `openmir2-run-gap-pl2` | P31 经验帧 + P07 死代码结论 | E4 | 部分 |
| B-31 | `openmir2-run-gap-pl3` | 内容启用后的 HP/倍率/饥饿数值与帧序 + T-85 八条 | E4 | 部分（内容不可达） |
| B-32 | `openmir2-run-gap-pl4` | 元宝自动增减/DB 金币轨迹 3 项 | E1+E4 | 部分 |
| B-33 | `openmir2-chat-channels` | `!!`/`!~`/`!` 的帧序 + 10s 门 + T-118 零帧确认 | E4 | 部分 |
| B-34 | `openmir2-castle-war-area-impl` | W-1..W-5：四臂落点 RNG/766 bit4/名字色/10s Run + T-125..T-132 | E1+E2+E4 | 部分（内容/城主需构造） |
| B-35 | `openmir2-castle-entities` | 部署 0 实体/4 告警、夹具 16 实体 96 抽、毒攻门、目标选择、1-tick 相位 + T-96 | E1+E2+E4 | 部分 |
| B-36 | `openmir2-castle-guard-target` | 同城/盟友/UnderWar 目标选择 + T-96⑥④ | E1+E2+E4 | 部分 |
| B-37 | `openmir2-guild-g3-persistence` | 104 扇出/SS_208/四段字节/相位/实际收件人集 + T-97..T-117 | E1+E2+E4 | 部分 |
| B-38 | `openmir2-guild-g2-dispatch` | 750 真机出队序等 3 项 | E1+E2+E4 | 部分 |
| B-39 | `openmir2-upgrade-npc` | 抽数序 / `Desc[10]` 字节 / 帧序 / 654·153 / 破零链 + T-95 | E1+E4 | 部分 |
| B-40 | `openmir2-gategate-keepalive` | 10s 周期、3min 窄竞态可达性（T-119②）、断链重连与 15 天清理 | E4 | C8 工具 v2.5 就绪；真机 capture 未采 |
| B-41 | `openmir2-gate-idle-timeout` | LoginGate/SelGate 60s 两阶段（含畸形帧）两项 | E4 | `gate_timer_gate.py` 就绪（32/32） |
| B-42 | `openmir2-selgate-reconnect` | KeepAlive + 下行 EOF 回连两项 | E4 | `gate_timer_gate.py` 就绪 |
| B-43 | `openmir2-gategate-slots-gm` | 槽位回收、GM_STOP、槽满复用三项 | E4 | C8 工具就绪 |
| B-44 | `openmir2-gategate-speed-rules` | 登录包相位、0-210ms 未丢帧先后、超长帧回归、ItemSpeed/SpeedLimit | E4 | 缺（0-210ms 丢帧/10ms 拍不在 C8 覆盖） |
| B-45 | `openmir2-dbsrv-scheme-a` | W-DB-1..7（慢存档/全序/闸门/独立失败）+ T-120 | E1+E4 | 部分（LB-08 判据已有） |
| B-46 | `openmir2-dbsrv-fault-paths` | 真机故障注入 + LB-10（LB-08/09 已由 `53d7aacc8`/`f7dc09d31` 落地） | E1+E3+E4 | 部分 |
| B-47 | `openmir2-db-error-reply` | 失败注入的 C# 对拍 | E1+E3+E4 | 部分 |
| B-48 | `openmir2-encoding-core-e1-e2` | 非 ASCII 文本逐字节 + 两条 BOM 路径 | E4 | 就绪（dotnet 9 + CodePages 全表探测） |
| B-49 | `openmir2-run-gap-checkdupobj`、`openmir2-run-gap-corpse-ghost-revive`、`openmir2-run-gap-monster-state-expiry`、`openmir2-run-gap-monster-specials`、`openmir2-run-gap-monster-master-follow`、`openmir2-movetomovingobject-quirk`、`openmir2-disappear-recipients-gridscan`（7 卡合一） | 同格推挤身份序列/AOI 相位；3min 变尸/复活/5min 移除；Crazy 色变/FireHit-TwinHit；产仔抽数/SM_HIT；异图/超距/SlaveRelax；移动路径源/目标格；节流窗收件人 | E4 | 部分（需多 actor/尸体/特殊怪构造） |
| B-50 | `openmir2-m3-rng-scene-driver` | 两侧账本对拍（W9）+ BotSrv real wiring | O1-O7+E2+E4 | 缺（`-Go -PinScene` 仍 `PREREQ_MISSING`） |
| B-51 | `openmir2-ctor-draws-fold` | 68 抽/17 站点逐项 diff；barrier A 的 1 抽另立片 | E4 | 缺 |
| B-52 | `openmir2-npc-ctor-draws` | F/M 裁决（8 处）+ `Robot.txt` 行数实测 | E4 + owner | 缺 |
| B-53 | `openmir2-default-arm-spell-surface` | 逐帧对拍 + 效果体（MP/53/MAGICFIRE/45 case） | E4 | 缺（G4 无法术驱动） |
| B-54 | `openmir2-spell-shared-tail` | 注入时钟下 656 帧序 + T-78 三偏差 | E4 | 部分 |
| B-55 | `openmir2-buff-system`、`openmir2-buff-producer-wiring` | 真机窗口 + A-5 生产接线（四块 snap 改 `cetin_near_xy`） | E4 | 部分 |
| B-56 | `openmir2-world-tick-expansion`、`openmir2-transmap-maptrigger`、`openmir2-maptrigger-monster-and-door` | RNG 流水/时钟/公告节拍；1658 route；W-R1-a/b/c | E4 | 部分 |
| B-57 | `openmir2-m3-clock-injection` | C# 时钟注入（O3 的前置） | O3 | 缺 |
| B-58 | `openmir2-m5-convergence-drills` | P99 三档收敛演练 | — | 部分（需独占窗口） |
| B-59 | `openmir2-m3-final-acceptance` | 300 假人 24h 长跑（DoD②） | E1+E4 | 方案/工具就绪；需独占 24h 窗口 |
| B-60 | `openmir2-gate-gmclose-race` | 无单独窗口项；观察点=长跑下控制帧 flush（tier=R 不 closed） | — | 随长跑 |

**B 表计数：60 行（覆盖 69 张卡；B-49 合并 7 卡、B-55 合并 2 卡、B-56 合并 3 卡）。**

> 未列入本表但仍 in-progress 的已合入卡：`openmir2-castle-war-area-design`、`openmir2-dbsrv-connection-design`（tier=L 设计件，无窗口项）；`openmir2-rng-consumer-structure-parity`、`openmir2-tmp-cleanup-audit-redo`、`openmir2-m3-guild-file-store`（tier=L，已收口或只差归档）；其余夹具/工具卡（`m3-*-fixture-botsrv`、`m3-fixture-chain-gate`、`m3-shop-scenario-authority`、`m3-castle-file-load` 等）的窗口项已被 B-21..B-47 的对应系统卡覆盖，不重复计。

---

## C 表：数据不可达 / 死面（20 条，只登记不排期）

| # | 面 | 证据 | 结论 |
|---|---|---|---|
| C-01 | 18 个数据不可达怪物 `Run`：200/113/114/117/214/215/210/208/206/133/209/131/132/120/203/134/`RunAway`/100 | `plan-run-gap-batches-20261018.md §2.2`（MonGen/库 0 条目；2026-10-17 只读实测） | A-12..A-16 仍按 R 登记，但真机窗口无对象；review 写 `window_required: true` + owner 裁决替代证据 |
| C-02 | `RunAway` 死类 | `plan §2.2`：`CreateMonster` 无 case | 只能在构造级注入测试；不实例化 |
| C-03 | `WhiteSkeleton`(100) | `plan §2.2`：race 100 MonGen 0 条；R42 仅首次帧 | L 级，构造级 + 1 条抽样冒烟 |
| C-04 | P07 个人定时器（`@OnTimer0..19`） | `PlayObject.Message.cs:143-157`；`SETONTIMER` 仅枚举声明（`ExecutionCode.cs:620-621`），全仓无 handler；部署 Envir **1 个文件使用 `@OnTimer9`**（`E:/MirServer/M2GameSvr/Envir/MapQuest_def/QManage.txt:50` `[@OnTimer9]`，同段 `:55` `KillScTimer 9`）；但全仓无 `SETONTIMER` 生产者、oracle 也无 `SetOnTimer` handler ⇒ 标签永不被派发（可复现：`grep -an '@OnTimer' /e/MirServer/M2GameSvr/Envir/MapQuest_def/QManage.txt`；`grep -rn 'SETONTIMER' src --include=*.cs` 仅 `ExecutionCode.cs:620-621` 枚举声明；`grep -rni 'SetOnTimer' src --include=*.cs` 无 handler） | **死代码**；等价实现 = 不实现；单测断言 `@OnTimer` 调用计数恒 0 |
| C-05 | P22 元宝自动增减 | `plan §1.2 P22`：Envir 0 处 AUTOADDGAMEGOLD/AUTOSUBGAMEGOLD；MapInfo 0 条 DECGAMEGOLD/INCGAMEGOLD | 已实现（PL-4），但生产不可达；内容启用后按 R 对拍 |
| C-06 | P25/P26/P27 地图 HP/饥饿/倍率 | `plan §1.2`：MapInfo 0 条 DECHP/INCHP；`Server.conf:396/398 HungerSystem=0/HungerDecHP=0`；Envir 0 处 KILLMONEXPRATE/POWERRATE | 已实现（PL-3），但生产不可达；T-85 登记八条偏差 |
| C-07 | P31 自动经验 | `plan §1.2 P31`：唯一脚本行 `Market_Def/QFunction-0.txt:25` 为 `SETAUTOGETEXP 1400 1 1`（无地图参数）⇒ oracle `ScriptActionError`，不设置字段 | 当前内容不触发；实现仍按 R 保等价 |
| C-08 | 邮件 | M3-report §1.10；`whitelist.md` T-36：oracle 全仓无实现（仅客户端 `boShowMail`） | `oracle_absent`；不开实现卡，Rust 与 oracle 一致不实现 |
| C-09 | 拍卖 5700 `MarketService` | M3-report §1.10；`audit-uncovered-surfaces` 附 A：部署 `M2GameSvr` 下无 `modules.json`/`MarketSystem.dll` | Rust 已 1:1（含 MemoryPack 恒抛/计数恒 0/SQL 错吞）；部署未加载 ⇒ 不可达 |
| C-10 | 11 张死表：`account_friend`、`tbl_conncheckuser`、`tbl_connectlogs`、`tbl_countlogs`、`tbl_gmip`、`tbl_itemgive`、`tbl_m2premiumuser`、`tbl_pubips`、`tbl_selectgateips`、`tbl_serverinfo`、`tbl_usercount` | `audit-uncovered-surfaces` 附 B：`src/**` 与 `mir2-rs/**` 的 `.rs`/`.cs` **代码 0 引用**；`mir2-rs/**` 仅 3 个证据文档提及表名（`m3/audit-uncovered-surfaces-20261019.md`、`m3/evidence-m3-guild-castle-inventory-20261013/00-oracle-chain-lines.txt`、`.../02-frames-config-and-db.txt`） | 代码 0 引用，仅证据文档提及；不派卡 |
| C-11 | `upgradeinfo` / `upgradeinfo_item` / `upgradeinfo_item_attr` | `audit-uncovered-surfaces` 附 B：C# 存取器是返回 -1 的空桩（`MySqlDB.cs:339-348`），全仓无 SQL | 功能缺口在 A-09/武器升级 NPC，不在 DB |
| C-12 | `marketitems` / `marketitem` | `audit-uncovered-surfaces` 附 B：仓库 DDL 无表；C# SQL 报错被吞 | Rust 照抄；不可达 |
| C-13 | 聊天/连接死代码：`ChatFilter=1` 但 `AbusiveFilter.CheckChatFilter` 两处调用均注释；`ChatCmdFilter=0`、`ProcClientCount=0`、`UseCloudGate=0`、`EnableOtp` 校验被注释；`MaxConnectOfIP` 限流整段注释 | `audit-uncovered-surfaces` 附 B（`ClientSession.cs:566/589`、`Filter.cs:160-185`、`GateShare.cs:82-99`） | oracle 无实际行为；Rust 不实现即等价 |
| C-14 | GameGate `ClientTimeOutTime=15000` 死键 | 同上：真正会话超时是硬编码 15 天（`GateShare.cs:26`） | 不读该键；T-119 已覆盖真实超时 |
| C-15 | `SS_208` / `SS_205/206/207/211/212` 单服 | `whitelist.md` T-113、`guild_cmd.rs` 注释：单服无接收端 | 单服只落事件/日志；是否实现跨服见 D-08 |
| C-16 | 行会 `GuildAllList.Add` | `whitelist.md` T-96⑥②：两处均被注释 ⇒ 生产 `IsAllyGuild` 恒 false | Rust 同；盟友分支只构造级可达 |
| C-17 | 城堡攻城 #4/#7/#8 的当前可达性 | `audit-uncovered-surfaces` 评审员更正 D-2：`Castle/0/AttackSabukWall.txt` 0 字节、`Castle/1/SabukW.txt` 空、`Castle/0/SabukW.txt` `OwnGuild=` 空/`TotalGold=0`；N1 让 `AddAttackerInfo` 抛 NIE | 无城主/无攻击者/无开战入口 ⇒ 开战/时限、攻城区/766 bit4、城堡税当前不可达；仍按 R 保留 |
| C-18 | 城堡 N1/N5 支 | `whitelist.md` T-38（10 个 `IUserCastle` 重载抛 NIE）、T-40（`Substring` 越界被 catch 吞） | 照搬为可观测 no-op；`@hireguardnowN`/`@hirearchernowN` 不可达 |
| C-19 | 城堡部署 A 情景 0 实体 | `whitelist.md` T-96：`Castle/Castle/SabukW.txt` 0 实体、4 告警、0 抽；夹具 B 情景 16 实体/96 抽 | 真机窗口只能核「0 实体/4 告警」；实体行为靠夹具 |
| C-20 | `characters_quest` 永不落库 | `audit-uncovered-surfaces` 附 A：oracle `SaveQuest` 只 DELETE、无调用点；Rust `quest_no_persist.rs` 反向钉死 | 两侧都不写；任务系统验收应判「不落库 = 等价」 |

**C 表计数：20 条。**

---

## D 表：owner 待裁决项（14 条）

| # | 裁决项 | 现状 / 事实 | 不裁决的后果 | 依据 |
|---|---|---|---|---|
| D-01 | **E1 写库边界**：生产 `mir2_db/mir2_account` vs 克隆 `mir2_test_*` | 所有构造都会写库；卡的 acceptance 明写「不写生产库」；v2.4 起起栈器已支持 `-DbName mir2_test*`，但 `scoped_diff.TABLE_COLUMNS` 未登记测试库表名 | Block A/B/C 全部不可开窗；E1-a 下 G5 只能行数/关键值断言，字段级 diff 只在 E1-b 可收 | `docs/window-run-sheet-20261021.md §E1`、§C10#11② |
| D-02 | **E2 内容改动**：影子 Envir/GuildBase/MonGen/非 TestServer | G2 行会 750 需行会内容；G3b/G3c 特殊怪需影子 MonGen；B10 非 TestServer 需影子 Server.conf + 整棵 Envir | G2 行会半、G3b/G3c、非 TestServer 余量不可开窗 | `§E2`、`§B10` |
| D-03 | **E3 MySQL 停/起**：故障注入 | 本机 mysqld 是进程（`D:\mysql\mariadb-10.11.19-winx64\bin\mysqld.exe`），不是 Windows 服务；`start-all -Stop` 会连带杀掉 | G6 不可开窗；`start-all -Stop` 永禁用 | `§E3` |
| D-04 | **E4 C# DBSrv/LoginSrv 身份**：E4-a 允许在主检出 `dotnet build` 并录 SHA256/HEAD，或 E4-b 改用全 Rust 栈 | 六件 Rust provenance 不含 6000/5100/5700/5500/5600；`check_csharp_capture_preflight.py` 只查 GameSvr | 所有依赖 C# oracle 的块（A/B/C 的 C# 侧）不可开窗 | `§E4` |
| D-05 | **3min `TimeoutStop` 分支的可达性是否可接受** | T-119②：置 `gate_ready=false` 的路径同时置 `check_server_fail=true`，`GM_CHECKSERVER` 不恢复 `gate_ready` ⇒ 该分支只在写失败竞态下可达；构造级只钉边界 | 网关 keepalive 卡无法判「3min 档」是等价还是缺陷 | `whitelist.md` T-119② |
| D-06 | **NRE 喊话意图**：是否批准 bug-fix 批次把 `!` 喊话从「零帧」改为真正投递 | T-118：oracle `CryCry` 的 `SendMsg(null,RM_CRY,...)` 抛 NRE 被吞 ⇒ 成功喊话零帧；已按硬约束③ 1:1 搬运 | 若不批准，`!` 永远零帧（与 oracle 等价）；若批准，需单开 R 卡并改线上可见行为 | `whitelist.md` T-118、`chat.rs` 模块头 |
| D-07 | **N1 城堡 no-op 与 #4/#7/#8 的优先级重裁** | owner 2026-10-13 已裁 N1/N2/N5 照搬；但评审 D-2 证明无城主/无攻击者 ⇒ `audit #4/#7/#8` 的 P0 可达性不成立 | 若继续按 P0 排期，会把窗口时间花在不可达面上 | `M3-report §1.8`、`audit-uncovered-surfaces` D-2 |
| D-08 | **跨服 `SS_*` 广播**：实现或登记不实现 | 单服无接收端（T-113）；多服语义未设计 | A-07 无法收口；若只做单服可显式登记「不实现」 | `audit #19`、`whitelist.md` T-113 |
| D-09 | **RNG 场景 pin 路径 O1-O7 是否仍然有效** | `docs/window-owner-decisions-20261018.md §2` 的 O1-O7 勾选栏全空；run sheet v2.5 未再引用 | 整包 W0/`m3-rng-scene-driver`/`ctor-draws` 的路线无法确定；状态 **UNKNOWN** | `docs/window-owner-decisions-20261018.md §2/§5` |
| D-10 | **`E:/tmp` 清理** | `docs/tmp-cleanup-proposal-20261017.md`：A 类可回收 448.1 GiB（cargo target 443.6 GiB）+ `m3bag-toolseol` 639 MiB；C 类非本项目 24.9 GiB 不动；D 类 29.5 GiB 须人核；卡 `openmir2-tmp-cleanup-audit-redo` 仍 in-progress | 磁盘压力；且 D 类误删会不可逆销毁他人数据 | `docs/tmp-cleanup-proposal-20261017.md §0/§2` |
| D-11 | **D-12：`save-on-shutdown` 的 oracle 是否真的落库** | 看板 `openmir2-save-on-shutdown` 明写「并裁决 D-12 的『oracle 是否真的落库』」 | 关服存档卡无法 closed；窗口判据的金标准不确定 | `openmir2-save-on-shutdown` work |
| D-12 | **NPC ctor draws 的 F/M 裁决（8 处）** | 看板 `openmir2-npc-ctor-draws`：owner 需裁决「只补抽（F）」还是「建实体（M）」；窗口前还须实测 `Robot.txt` 行数 | RNG 账本锚点仍可能错；窗口对拍 seq=0 不可信 | `openmir2-npc-ctor-draws` work |
| D-13 | **`GateCount>1` 多实例** | `audit-session-lifecycle-20261020.md #10`：三网关 config 只读 `*0`，单 listener；现网文档为 1 | 配置 >1 即不可用；当前部署不可达 | `audit-session-lifecycle-20261020.md §1` |
| D-14 | **编码 E-1/E-2 是否强制 cp936 全表** | `audit-encoding-v2-20261021.md`：影响高/可达性未证；当前部署语料 0 命中 | 若不强制，PUA/非法字节路径仍是近似等价；若强制，需改所有 one-shot `GBK.encode` 调用点 | `audit-encoding-v2-20261021.md §0/§4` |

**D 表计数：14 条。**

---

## 下一次开窗最少需要什么（命令级）

> 目标：让 Block A（G0+G1）、Block B（G3a）、Block C（G2a+G5a）可跑；G4/G6/G7/G8/G9/G10/G11 不在本次最小集。所有命令按 `docs/window-run-sheet-20261021.md` 的约定；开窗当天必须**重新解析 `origin/master`**，不把本报告的 `c83a9ae24` 当长期合同。

### 0. 授权（人类，先做）

1. 在 `docs/window-run-sheet-20261021.md §E` 勾选 **E1-a 或 E1-b**、**E2**、**E4-a 或 E4-b**；G6 才需要 **E3**。
2. 若本次要跑 RNG 账本/W9，另勾 `docs/window-owner-decisions-20261018.md §5` 的 **O1-O7**。
3. 确认窗口独占：`E:/MirServer` 十端口无其它车道、无并发 cargo/rustc/msbuild、MySQL 3306 归属明确。

### 1. 修 R6/R7/R8（只读 guard 三红 → 绿）

```bash
# R6：镜像必须与当天 origin/master 一致（不要沿用 f3675f69aa）
cd /e/tmp/m2svr-window-origin
git fetch origin
git checkout --detach "$(git rev-parse origin/master)"
git status --porcelain    # 期望空

# R7：在镜像内重编六件，target 与 guard 完全一致，并重录 provenance
export CARGO_TARGET_DIR=/e/tmp/m2svr-window-target
cargo build --offline -p game-svr -p game-gate -p login-srv -p login-gate -p sel-gate -p db-srv
python -I -B /e/Users/gxh/Documents/GitHub/OpenMir2/mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/record_window_provenance.py \
  --ref "$(git rev-parse HEAD)" --record-provenance

# R8：九端口必须全 DOWN（先由编排/owner 按 pid 文件停掉旧栈，不得直接 Stop-Process）
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1

# guard（期望 WINDOW_GUARD_OK）
cd /e/Users/gxh/Documents/GitHub/OpenMir2
bash mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/client-e2e-window.fixed.sh.txt guard
```

### 2. DB 快照 / 还原（按 E1 选一组，二选一，不可混用）

```powershell
# E1-a：测试库克隆（推荐；G5 只能行数/关键值断言）
python -I -B tools/dbsnap/dbsnap.py --password=<pw> snap --db test --dir E:/tmp/window/20261022-a/db/base
# E1-b：生产库 + 书面授权（G5 可做字段级 diff）
python -I -B tools/dbsnap/dbsnap.py --password=<pw> snap --dir E:/tmp/window/20261022-a/db/base
```

### 3. 起栈（混合栈：C# DBSrv/LoginSrv + Rust game-svr/三网关）

```powershell
# 若 E1-a：追加 -DbName mir2_test（v2.4 起支持）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -RustBinDir E:/tmp/m2svr-window-target/debug -Seed 42 `
  -LogDir E:/tmp/window/20261022-a/rust/stack
  # 追加：-DbName mir2_test [-DbHost/-DbPort/-DbUser/-DbPassword]
```

### 4. 抓包拓扑（A2，必须按顺序）

```powershell
# 4.1 停真 gate(7200)，起影子 gate(7600)，代理占 7200
#     影子 conf：cp936 原样，只改 Count 与 GatePort1
# 4.2 断言 7200 DOWN / 7600 UP（port-status.ps1 不含 7600，必须单独断言）
# 4.3 起代理：
python -I -B tools/gw-capture/mir2_proxy.py --listen 7200 --upstream 7600 --out E:/tmp/window/20261022-a/capture
```

### 5. 跑最小块（每块 2 小时，块间收窗/还原）

```powershell
# Block A：G0 + G1（登录/入图基线）
#   全收：openmir2-smframe-minor-gaps
#   部分：login-frame-order-52-752 / login-numeric-gaps / healthspell-emission-model /
#         orphan-encoder-wiring / window-collector-preflight / outbound-encoders-red-batch

# Block B：G3a（单客户端战斗/AOI/GuardList/反向点）
#   全收：monster-attack-hit-frame / monster-attack-cadence-lead / player-dir-supply
#   部分：aoi-verify-maptime / monster-view-throttle / attacktarget-overrides /
#         active-seek-race-missing / proper-target-unfolded / search-target-clear-target /
#         run-gap-mb1/mb3/mb4 / guardlist-guards-loading

# Block C：G2a（装备/技能登录态）+ G5a（优雅关服/存档）
#   全收（E1-b）：oracle-humitems-align / delmagic-addmagic-chain / save-on-shutdown
#   全收（E1-a）：oracle-humitems-align / delmagic-addmagic-chain（save-on-shutdown 部分收）
#   部分：persist-useitems-wiring / persist-create-rows / save-misc-gaps
```

### 6. 判据命令（工具已就绪，逐条带自检）

```powershell
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_sm_hit.py --selftest
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_attack_observability.py --selftest
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_timing.py --selftest
python -I -B mir2-rs/tests/parity/world/client-e2e-enter-gaps-20261013/check_client_s2c_frames.py --selftest
python -I -B tools/m2-acceptance/s2c_diff.py --selftest
python -I -B tools/gw-capture/gate_timer_gate.py --selftest
python -I -B tools/gw-capture/selftest_gate_shadow.py
python -I -B tools/dbsnap/dbsnap.py tables-selftest
powershell -NoProfile -ExecutionPolicy Bypass -File tools/client/account_provision.ps1 -Selftest
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/check_db_paths.ps1 -Selftest
```

### 7. 收窗（顺序不可换）

```powershell
# 1) 停代理、停 7600 影子 gate（各自的 pid）
# 2) 停 Rust 栈（-Stop 跳过 MySQL）
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 `
  -LogDir E:/tmp/window/20261022-a/rust/stack -Stop
# 3) C# 栈：CTRL_BREAK 优雅停 GameSvr，等 3 秒，再 start-csharp-hook.ps1 -Stop
# 4) MySQL 默认不动；只有 E3 授权才停/起，且先完成 DB 快照
# 5) DB 还原（测试库加 --allow-db）
python -I -B tools/dbsnap/dbsnap.py --password=<pw> restore --dir E:/tmp/window/20261022-a/db/base --yes-apply
# 6) 端口 + 7600 单独断言
powershell -NoProfile -ExecutionPolicy Bypass -File E:/MirServer/port-status.ps1
# 7) 工作树/证据
git status --porcelain    # 期望空
```

### 8. 本窗口**不要**排的组（工具/授权未齐）

| 组 | 原因 | 对应卡 |
|---|---|---|
| G4 法术/延时 | 无法术按键/`AutoSpell` 驱动（C10#6）；只能摆前置 | `default-arm-spell-surface`、`outbound-encoders-red-batch`、`rm-refmessage-delay-layer` |
| G6 DB 故障 | 需 E3 停/起 mysqld + 6000/5100 无抓包（C9） | `dbsrv-fault-paths`、`db-error-reply` |
| G7 网关 | 工具 v2.5 已闭环，但真机 capture 未采；0-210ms 丢帧/10ms 拍不在覆盖 | `gate-idle-timeout`、`selgate-reconnect`、`gategate-slots-gm`、`gategate-speed-rules` |
| G8 内容周期 | 内容全部未启用 + C# 时钟未落（O3） | `world-tick-expansion`、`transmap-maptrigger` 等 |
| G9 整包 W0 | O1-O7 未裁决 | `m3-rng-scene-driver`、`ctor-draws-fold` |
| G10 M3 夹具链 | fixture/PREREQ 未齐；④/⑥ readiness 需重跑 | `m3-*-align`、`m3-fixture-chain-gate` |
| G11 24h 长跑 | 独占 24h+ | `m3-final-acceptance`、`m5-convergence-drills` |

---

## 附录 A：本报告的只读复核命令（可复算）

```bash
# 看板（只读）
walgit collab board --format json

# A 表关键源码复核（只读）
grep -rn "CM_GUILD" mir2-rs/bins/game-svr/src/main.rs
grep -n "castle_official_action" -r mir2-rs --include=*.rs
grep -n "let _tax" mir2-rs/bins/game-svr/src/shop.rs
grep -n "PROTECTION_INSERT_SQL\|PROTECTION_UPDATE_SQL" mir2-rs/crates/storage/src/account.rs
grep -rn "OPENYBDEAL\|ActionOfOpenSaleDeal" mir2-rs/crates mir2-rs/bins --include=*.rs
grep -n "Market_Def 商人脚本未装载" mir2-rs/bins/game-svr/src/main.rs
grep -n "GetUpgradeStdItem\|Desc\[13\]" -r mir2-rs/bins/game-svr/src/bag.rs
grep -rn "AutoTimerStatus" src --include=*.cs
grep -rn "SETONTIMER" src --include=*.cs

# C 表数据不可达复核（只读；部署件）
wc -c /e/MirServer/M2GameSvr/Castle/0/AttackSabukWall.txt
grep -a -c "DECHP\|INCHP" /e/MirServer/M2GameSvr/MapInfo.txt
grep -a -c "KILLMONEXPRATE\|POWERRATE" -r /e/MirServer/M2GameSvr/Envir | head
```

## 附录 B：残留物清单

- 本报告只新增 `docs/m3-remaining-inventory-20261022.md`；worktree `/e/tmp/wt-inventory` 内无缓存、无 target、无临时库、无进程。
- 报告写作期间创建的仓外临时物：`E:/tmp/inv-board.json`（看板 JSON 快照）、`E:/tmp/inv-cards.txt`（看板卡摘要）。两者均为只读分析中间件，可随时删除；本报告不依赖它们存在。
- 未触碰 `E:/MirServer`、主检出、`src/**`、`sql/**`、`MirClient/`、`mir2-rs/crates/{protocol,storage}`。
