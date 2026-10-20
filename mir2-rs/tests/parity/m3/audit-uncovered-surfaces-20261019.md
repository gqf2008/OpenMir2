# Lane D — 尚未审计的对外面清单（只读盘点）

- 基线：`049771a9b`（master，2026-10-19 工作树快照）。
- 方法：只读 `grep`/读文件；未建 worktree、未改树、未起进程；部署件只在 `E:/MirServer` 只读核对（未改动）。
- 口径：优先「客户端可见 + 生产可达」；每条给 oracle 锚、Rust 现状、缺口、可达性、tier、以及**能让改坏变红的具名断言（牙）**。
- 已有盘点（`tests/parity/m3/M3⑦⑧⑩…报告`）仍然有效；本表只列**盘点之后仍未接线/仍未审计**的面，不重复已覆盖项。
- 说明：`tier=R` 按仓库政策（RNG / 时序 / 持久化 / 战斗数值）；纯逻辑/帧编码为 `L`。裁决以审查者为准。

## 可派卡清单（P0 最高）

| # | P | 面 | oracle 入口（文件:行） | Rust 对应实现 | 已知缺口 / 未建模项 | 可达性（部署内容） | tier | 牙（改坏必红的具名断言） |
|---|---|---|---|---|---|---|---|---|
| 1 | P0 | 行会会话层：9 个 `CM_GUILD*` + `SM_GUILD*`/`RM_BUILDGUILD*` 帧 + 角色侧 `MyGuild/GuildRankNo/AllowGuild` + 登录 750 + 掉线 `DelHumanObj` + `HAM_GUILD` | `src/M2Server/Player/PlayObject.Message.cs:975-1007`；`PlayObject.Operate.cs:1229-1560`；`PlayObject.Base.cs:1629-1633`；`PlayObject.Message.cs:2145-2147`；`PlayObject.cs:4249-4256` | 状态机：`crates/world/src/guild.rs`（`GuildManager` @ `:864`）；**无派发、无帧**：`bins/game-svr/src/main.rs` 全文无 `CM_GUILD`；`smframe.rs` 73 个 `sm_*` 无 guild；`main.rs:2579-2580` 硬编码 `has_guild:false/guild_rank_no:0`；`smframe.rs` 无 750 | ① 9 个 CM 全 no-match；② 面板/成员/封号/公告/联盟/解散帧全缺；③ `MyGuild` 不随登录/建会物化；④ 掉线不 `DelHumanObj`；⑤ `HAM_GUILD` 明文「不剔除」（`crates/world/src/combat.rs:525`） | `Envir/Npc_Def/比奇国王-0122.txt` 提供 `@@buildguildnow`/`@@guildwar`（见 #3）；`GuildBase/GuildList.txt` 存在（0 行，建会后才有内容） | L（纯状态+帧；文件落盘另见 #2） | `guild_dispatch_vectors`：对 9 个 CM 各造一帧，断言回包 ident/Recog/Param/尾逐字节；删任一 match 臂即红；登录断言 `SM_CHANGEGUILDNAME(750)` 尾=`名/封号` 或空尾 |
| 2 | P0 | 行会文件加载/落盘 + `GuildManager.Run`（30s 节流、行会战到期、`SaveGuildList`/`<名>.txt`/`GuildConf`） | `src/M2Server/Guild/GuildManager.cs:78-143/145-167`；`GuildInfo.cs:366-430/525-550`；tick 入口 `src/GameSrv/Word/Threads/SystemProcessor.cs:75` | `crates/data/src/guild.rs:195-283`（`GuildStore` 已实现，**仅测试引用**）；`crates/world/src/guild.rs:864` Run 已实现；`game-svr` 无任何调用点 | ① 启动不 `LoadGuildInfo`；② 建会/解散/改公告/宣战不落盘；③ 30s 节流与到期结算不跑；④ `ServerIndex != 0` 的 no-op 未接线 | `GuildBase/GuildList.txt` 存在（空）；`Server.conf:61-62 GuildDir/GuildFile` 指向该目录 | R（持久化 + 时序） | `guild_store_roundtrip`：加载部署件→建会→`SaveGuildList`/`SaveGuildFile`→重载逐字节等（CRLF）；把 `\r\n` 改 `\n` 即红；`guild_run_tick_vectors`：注入 now，30s 前后各一次断言 `SaveStatus` 与到期事件 |
| 3 | P0 | 建会 / 宣战 NPC 标签（`@@buildguildnow`、`@@guildwar`、`@requestcastlewarnow` 的费用/物品/掌门门） | `src/GameSrv/Npc/GuildOfficial.cs:55-90`（UserSelect）、`:112-178`（ReQuestBuildGuild）、`:180-201`（ReQuestGuildWar）、`:206-233`（ReQuestCastleWar） | `crates/world/src/npc.rs:352` 只实现 `GuildOfficial.Run` 的抽数/转身；**UserSelect 4 支全无**；`crates/script` 无 `@requestcastlewarnow` 消费点 | ① `BuildGuildPrice`/`WomaHorn`/`AddGuild` 副作用链；② `GuildWarPrice`/`ServerIndex` 门；③ `ZumaPiece`+`@requestcastlewarnowN`；④ 回 `RM_BUILDGUILD_OK/FAIL` 文案 | 部署脚本明确可达：`比奇国王-0122.txt:6/7/146/189`（`@@buildguildnow`/`@@guildwar`/`@requestcastlewarnow`） | L | `guild_official_label_vectors`：无钱/无号角/重名/成功四例断言返回码与背包金币差；把 `CheckItems` 判断挪后即红；城门申请断言 N1 no-op（T-38） |
| 4 | P0 | 攻城文件 + 状态机接线（`CastleMgr.Initialize/LoadCastleList`、`UserCastle.Run` 每日开战/时限/围墙/回收 Ghost） | `src/GameSrv/GameApp.cs:213/246`；`src/M2Server/Castle/CastleManager.cs:62-88/154-192`；`UserCastle.cs:311-448`；tick 入口 `SystemProcessor.cs:76` | `crates/envir/src/castle.rs`（文件解析器）、`crates/world/src/castle.rs:967`（`initialize`）、`:1165-1181`（Run 事件）、`castle_tax.rs` 均已实现；`bins/game-svr` **零引用**（`CastleMgr` 只出现在 `ctor_fold.rs` 注释） | ① 启动不加载 `Castle/List.txt`；② 每 tick 不跑开战/结束/围墙 StoneMode；③ `AttackSabukWall.txt` 读写不接；④ `SS_212` 跨服广播不接 | `Castle/List.txt`=`0`/`1` 两行（两座城），`Castle/0/SabukW.txt` 完整（MainDoorHP=9999 等）；`Server.conf:65/71` 指向 | R（时序 + 文件持久化） | `castle_run_golden`：注入 `date/hour/tick`，断言跨天重置→20 点开战→2h 结束的事件序列；删跨天分支即红；`castle_attack_sabuk_wall_roundtrip`：写/读逐字节（含日期原串） |
| 5 | P0 | 攻城实体挂图（`UserCastle.Initialize`：主门/左中右墙/12 弓手/守卫，HP 取 `SabukW.txt`；`CastleDoor/WallStructure.Run/Die/RefStatus`） | `src/M2Server/Castle/UserCastle.cs:108-227`；`CastleDoor.cs:9-116`；`WallStructure.cs:7-82`；`WorldServer.MonGen.cs:653-661` | `crates/world/src/castle.rs:175-410`（门/墙纯状态函数）、`crates/world/src/monai.rs:709`（race 11/12/20/110/111/112 归 `GuardUnit`）、`bins/game-svr/src/mongen.rs:343`（ctor 抽数）；**无 `RegenMonsterByName` 挂图**、无受击→`CastleDoor.Die` 路由 | ① 不造 20 个实体（1 门+3 墙+12 弓手+4 守卫）；② 不把 `MainDoor.nStatus`/`MainDoorHP` 写成实体 HP；③ 攻击门/墙不会触发 `Die`→`SetMapXyFlag(2)`；④ `Run` 的 `RM_DIGUP/DIGDOWN/TURN` 帧不接 | `Castle/0/SabukW.txt` 有门/墙/弓手坐标与 HP；race 110/111 在 `sql/mir2_data.sql:366-369` | R（怪物 ctor 抽数 + 战斗） | `castle_entities_from_sabukw`：解析部署 `SabukW.txt` → 断言实体数、坐标、`WAbil.MaxHP`；把 HP 来源改回 DB 即红；`door_die_sets_flag2`：HP 归零后 9 格通行掩码逐格断言（B-15 已红检过，扩到实体路径） |
| 6 | P0 | 攻城 NPC 标签（`@openmaindoor`/`@closemaindoor`/`@repairdoornow`/`@repairwallnowN`/`@hirearchernowN`/`@@withdrawal`/`@@receipts`） | `src/GameSrv/Npc/CastleOfficial.cs:93-406`（UserSelect/RepairDoor/RepairWall/Hire*）；N1 断点见盘点 §1.L | `crates/world/src/castle.rs:478-510/824-840`（`CastleOfficialAction` + repair/cost 纯函数）已实现；`bins/game-svr/src/npc.rs` **无 CastleOfficial UserSelect** | ① 标签→动作映射未接；② 金库提取/上缴按 N1 必须 no-op（T-38）；③ 雇佣链 N5 `Substring` 越界在 oracle 不可达，Rust 需显式 no-op 而非 panic；④ 修门/修墙 60s 门 | `Envir/Npc_Def/沙巴克城堡管理-0150.txt:32/35/40/55/61-75` 明确使用这些标签 | L | `castle_official_label_vectors`：每个标签断言 `CastleOfficialAction` 与金币/城墙 HP 差；把 N1 改成「真实现」即红（应保持 no-op）；`hirearchernowN` 断言不 panic 且不改状态 |
| 7 | P0 | 攻城区语义：`InCastleWarArea`、进图落点改写（`HomeX-4+Random(9)`）、帧 766 bit4、行会/敌对名字色 | `src/M2Server/Castle/UserCastle.cs:454-479`；`src/GameSrv/Word/WorldServer.cs:298-334`；`PlayObject.Base.cs:271-286`；`PlayObject.cs:3585-3656` | `bins/game-svr/src/enter.rs:352` 明写 `InGuildWarArea` 恒 false；`PLAY_NAME_COLOR=255` 恒；`enter.rs:201` 注明城堡/行会分支不触发 | ① 进图不判攻城区；② 皇宫/战时落点不改写；③ 766 的 bit4 永不置位；④ 行会色/敌对色不生效；⑤ `GetHomeX/Y` 的 `Random(9)` 会改变全局 RNG 流（当前不抽） | 城堡图 `3/0150/0151..0156/D701` 均在部署 `SabukW.txt`/`MapInfo` | R（RNG + 进图时序） | `castle_area_login_vectors`：固定种子，在攻城区/皇宫各一例断言落点与 766 `Recog` 位；把 `Random(9)` 去掉即红（RNG 流偏移） |
| 8 | P1 | 城堡税 / 成员价接线（商人卖出/修理按 `Castle.IncRateGold` 计税；`CastleMemberPriceRate=80`） | `src/GameSrv/Npc/Merchant.cs:544-552/1250-1258/1385-1393/1637-1645`；`Merchant.cs:765-782`；`UserCastle.cs:768-812` | `crates/world/src/castle_tax.rs` 已实现税公式与 B-16；`bins/game-svr/src/shop.rs:935/1040/1215` 仅 `let _tax = ...` **丢弃**，注释「城堡对象未建模」；`main.rs:3515` `castle_member:false` | ① 沙巴克商人交易不累积 `TotalGold`；② 城堡成员价永不打折；③ `GetAllNpcTax` 税目标不接；④ `SaveTick` 10min 落盘不接 | `MerChant.txt` 第 6 列决定 `CastleMerchant`（部署 131 商人里含沙巴克商人）；`Server.conf:250-260` 税/价格键存在 | L（纯算术；接落盘时升 R） | `castle_tax_shop_vectors`：买/卖/修各一例，断言金币差 = `amount*rate/100`；把 `_tax` 改回丢弃即红 |
| 9 | P0 | 频道聊天入口：`!!`（组队）、`!~`（行会）、`!`（喊话，10s 门） | `src/M2Server/Player/PlayObject.Chat.cs:163-208`；`PlayObject.cs:5185-5197`（SendGroupText）；`PlayObject.cs:705-719` | `bins/game-svr/src/chat.rs:17-18/39` 明写「喊话/组队/行会 `!` 未接」；`is_plain_say` 把 `!` 全部排除；组队状态机已有 `party_send_group_text` | ① `!!` 不发 `RM_GROUPMESSAGE`/`SM_SYSMESSAGE`；② `!~` 无行会消息；③ `!` 无 `RM_CRY`/`CryCry`、无等级门、无 10s 节流；④ `SS_208` 跨服不接 | 客户端聊天框天然可达（所有在线玩家） | R（队列出队序 + 10s 时序） | `channel_chat_vectors`：`!!`/`!~`/`!` 各一条，断言目标集合、帧序（立即→排队按优先级）、喊话 10s 内第二次的提示；把 `!` 落回普通 say 即红 |
| 10 | P1 | 武器升级 NPC（`@upgradenow` / `@getbackupgnow`；`UpgradeWapon`/`GetBackupgWeapon` + `UpgradeWaponAddValue`） | `src/GameSrv/Npc/Merchant.cs:527-590`（升级）、`:592-...`（取回）、`:373-...`（加值）、`:887/894` 标签派发；`ScriptFlagConst.cs:52-60` | `crates/script/src/parser.rs:958-960` 只解析 `is_upgradenow` 标志；`bins/game-svr/src/merchant.rs:65` 只透传；**无 `UpgradeWeaponList` 状态、无两个标签处理器**；`smframe.rs:207` 明写「本仓未建模 ⇒ 登记」 | ① 扣金币/黑铁矿；② 武器移出装备并算 DC/SC/MC/Dura 加成；③ `Random(Dura)` 升级成功判定；④ `UPgradeWeaponGetBackTime` 取回门；⑤ `~@upgradenow_*` 分支文案 | 部署明确可达：`Envir/Market_Def/沙巴克铁匠-3.txt` 头标签 `(@upgradenow @getbackupgnow)` | R（RNG + 时序） | `weapon_upgrade_vectors`：固定种子 + 金币/黑铁，断言升级后属性与 `GetBackTick`；把 `Random(Dura)` 改成常量即红；取回在时限前/后各一例断言 `~@getbackupgnow_ing/_ok` |
| 11 | P1 | GameGate 速度/间隔规则（Move/Attack/Spell/Turn/Butch/Sit/Pickup/Eat + 补偿 + `OverSpeedPunishMethod=3 DelaySend`） | `src/GameGate/Services/ClientSession.cs:212-480`；`SessionSpeedRule.cs`；`Conf/GateConfig.cs:99-224` | `bins/game-gate/src/rules.rs` 只有 `ChatThrottle`；`config.rs:50-57` 只读 `ChatInterval`/`NomClientPacketSize`；其余键不读 | ① 8 类命令的间隔门全缺；② 攻击/施法/移动互相补偿；③ 超速惩罚（DelaySend/丢包/转包）；④ `MaxItemSpeed*` 物品速度 | 部署 `RunGate/config.conf`：`MoveInterval=1/Turn=1/Spell=1/Attack=1/Butch=1/SitDown=1/Pickup=1/Eat=1`，`OverSpeedPunishMethod=3`（全部开） | R（时序） | `gate_speed_vectors`：喂时间戳序列，断言每类命令转发/延迟/丢弃；把 `MoveInterval` 开关置 0 即红；真机抽样：连续 `CM_WALK` 超速帧的线上到达时间 |
| 12 | P1 | LoginGate / SelGate 60s 空闲超时两阶段（先 `SM_OUTOFCONNECTION`，下一轮关会话） | `src/LoginGate/Services/ClientSession.cs:128-155`；`src/SelGate/Services/ClientSession.cs:92-118`；`Config.ClientTimeOutTime=60000` | `bins/login-gate/src/main.rs`、`bins/sel-gate/src/main.rs` 无定时器/无 `SM_OUTOFCONNECTION`；config 只读连接四元组（`config.rs:18-30`） | ① 空闲连接永不超时；② 不先发 `SM_OUTOFCONNECTION` 再关；③ `KeepConnectTimeOut` 键读而不消费 | 部署 `LoginGate/config.conf:15 ClientTimeOutTime=60000`、`SelGate/config.conf` 同 | R（时序） | `login_gate_timeout_test`：无流量 60s（虚拟时钟）→ 断言先发 `SM_OUTOFCONNECTION` 再关连接；删定时器即红 |
| 13 | P1 | LoginGate/SelGate→LoginSrv/DBSrv 的 `Enter` 帧体形状（`%K{sid}/{ip}/{ip}$` / `{ip}/{ip}`） | `src/LoginGate/Services/ClientSession.cs:204-216`；`src/SelGate/Services/ClientSession.cs:166-180`；消费端 `LoginServer.cs:185-205`、`UserService.cs:293-323` | `bins/login-gate/src/main.rs:134-140`、`bins/sel-gate/src/main.rs:134-140` 发 `peer` 原文（`IP:port`，无 `/`、无 `%K…$`） | ① LoginSrv 解出 `user_ipaddr=""`、`gate_ipaddr="IP:port"`；② DBSrv `arrest` 找不到 `%…$` ⇒ `user_ip=""`、`gate_ip=""`；③ `DynamicIPMode=0` 时不影响路由，但会话 IP/日志/证书 `CertUser.addr` 错 | 部署两端都启动即触发；`DynamicIPMode=0` 故不改变选服路由 | L（字节形状；用 wire 测试即可） | `enter_payload_wire`：断言网关发出的 Enter `data` 等于 C# 形态，DBSrv `open_user` 后 `user_ip/gate_ip` 等于客户端 IP；把发送改回 `peer` 原文即红 |
| 14 | P1 | `account_protection` 字段级创建/更新（13 列） | `src/LoginSrv/Storage/AccountStorage.cs:295-310`（INSERT）、`:498-513`（UPDATE）；`ClientSession.cs:196/386-391` | `crates/storage/src/account.rs:251-260`（INSERT 只写 `UserName/Quiz2`，其余写空串）、`:358-372`（UPDATE 只写 `UserName/Quiz2`）；`bins/login-srv/src/login_server.rs:785-791` 只取 4 字段 | ① `SSNo/Phone/Quiz/Answer/EMail/BirthDay/MobilePhone/Answer2` 丢失；② C# 的 `Phone=MobilePhone=Birthday=BirthDay` 怪写未复刻（当前全空）；③ `CM_ADDNEWUSER` 与 `CM_UPDATEUSER` 两条都受影响 | `LoginSrv/config.conf` 启用建号（`EnableMakingID`）时客户端可达 | R（持久化字段级 diff） | `account_protection_roundtrip`：造一条带全部 13 字段的 `UserAccountPacket`，建号后 `SELECT * FROM account_protection` 与 C# 逐列同值；删 `EMail` 绑定即红 |
| 15 | P2 | 交易栏物品的 `GetUpgradeStdItem` desc 升级差量 + `CustomItemSystem.GetItemName` 自定义名（T-24） | `PlayObject.Base.cs:2383-2403`（`SendAddDealItem`）/`:2357-2381`；`GameItemSystem.cs`；`CustomItemSystem` | `bins/game-svr/src/trade.rs:49-54` 明写未建模；`item_display_name` 对 `Desc[13]==1` 返回空串 | ① `ClientItem.Desc` 升级差量不写；② 自定义名物品在交易栏/寄售栏显示空名；③ 名字比对走空串可能误判 | 交易本身可达；现网物品名多为中文，升级过的武器有差量 | L（帧编码；抽样即可） | `deal_item_desc_vectors`：对带升级属性的物品断言 `SM_ADDITEM/SM_DEALADDITEM` 的 `ClientItem` 逐字节；把 desc 清零即红 |
| 16 | P2 | `OPENYBDEAL` 脚本命令 + 登录 `GetSellOffGlod`（元宝寄售入口） | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:125/2329-2356`；`PlayObject.Sale.cs:387-429`；`PlayObject.cs:1486` | `crates/script/src/engine/dispatch.rs` 的 `IMPLEMENTED_EXECUTIONS` **不含** `ActionOfOpenSaleDeal`；`bins/game-svr/src/selloff.rs:79` 明写 `SaleDeal` 无 setter ⇒ 登录路径 no-op | ① 脚本 `OPENYBDEAL` 落 `NotImplemented`；② 所有 `CM_SELLOFF*` 因 `SaleDeal=false` 不可用 | 部署 `Envir/` **无** `OPENYBDEAL`/`dealybme` 脚本（grep 0 命中）⇒ 当前不可达；内容回归即可见 | L | `openybdeal_vectors`：一条 `#ACT OPENYBDEAL` 后断言 `SaleDeal=true`；当前应红（未实现），实现后断言开窗帧 |
| 17 | P2 | GM 命令面中影响组队/交易/行会的命令（`@LetGuild`/`@LetTrade`/`@GroupRecall`/`@GuildRecall`/`@AllowGroupReCall`/`@AllowGuildReCall`） | `src/Modules/GameCommand/Commands/*.cs`（177 个命令）；`GameCommands.cs:74-79/367-368`；`Command.conf:12/14` | `bins/game-svr/src/chat.rs:17-18` 明写 `@` 命令系统未接；`crates/world` 有 `allow_group` 字段但无 `allow_guild`/无消费点 | ① `@` 命令全 no-op；② `AllowGuild` 未进实体（C# 加行会成员的 `!AllowGuild` 门）；③ `AllowGroupReCall`/`AllowGuildReCall` 无消费点；④ 组队/行会召回 GM 命令 | 部署 `Command.conf` 定义了 `LetTrade`/`LetGuild`（带权限）；普通客户端不可达，GM/管理端可达 | L | `gm_command_vectors`：`@LetGuild` 两次切换 `AllowGuild` 并断言提示帧；`@LetTrade` 后交易应被拒；当前应红（未实现） |
| 18 | P2 | GameGate↔GameSvr 保活：`GM_CHECKCLIENT`（10s）/`CheckServerTimeOut`（3min）/会话超时（硬编码 15 天）/断链重连 | `src/GameGate/Services/ClientThread.cs:152-161/186-220/265-300/332-346/410-480`；`TimedService.cs:36-42/151-173`；`GateShare.cs:22-26` | `bins/game-gate/src/main.rs`：`connected_state_action`（10s 健康帧/3min 严格边界/失败计数）、`link_loop`+`spawn_connect`+`Downstream`（断链重连/`Stop()` 全关/连接代次）、`session_sweep_loop`（15 天清扫 + `idle_eligible` 复刻 `RestSessionArray` 的 `Socket=null`）；GameSvr 侧 `bins/game-svr/src/main.rs:17701-17705` 收 `GM_CHECKCLIENT` 置保活标志 | ① **已修**（`GM_CHECKCLIENT` 每 10s）；② **该分支在无重连/无写失败竞态下不可达**：`TimeoutStop`（3min）要求 `!gate_ready && !check_server_fail`，而所有置 `gate_ready=false` 的路径（读 EOF / 写失败判死）同时置 `check_server_fail=true`，`GM_CHECKSERVER` 只清 `check_server_fail`、不恢复 `gate_ready` ⇒ 构造级单测只钉严格 `>` 边界与「写失败判死后仍收到 `GM_CHECKSERVER`」的窄竞态（`timeout_branch_needs_the_write_failure_race`），真实可达性需 owner 裁决；③ **已修**（15 天扫描排除 `RestSessionArray` 摘除的会话）；④ **已修**（下行重连，见 T-119） | 部署 GameGate 与 GameSvr 都启动即触发；断链重连在 GameSvr 重启/网络闪断时可达；影响链路僵死/会话泄漏，不直接改客户端帧 | R（`tier_why`：时序——10s 判定周期、3min 严格 `>`、15 天清扫与断链/重连两侧 `RestSessionArray` 的先后关系；改坏只在时序/竞态上暴露） | `keepalive_period_is_strictly_greater` / `timeout_branch_needs_the_write_failure_race` / `gm_checkclient_waits_for_the_period_before_first_frame`（首帧 elapsed ≥ 注入周期）；`rest_session_array_detaches_idle_but_keeps_sessions` / `downstream_drop_detaches_existing_sessions_from_idle_close`（断链后不得 15 天清理）；`reconnects_after_downstream_drop_and_resumes_keepalive`（断链后恢复健康帧）；`idle_session_without_traffic_is_closed_after_timeout`（EOF elapsed ≥ 阈值）；删 `last_recv_ms.store` 或把 `>` 改 `>=` 即红 |
| 19 | P2 | 跨服/跨进程行会/攻城广播（`SS_205/206/207/211/212`） | `GuildOfficial.cs:147`（SS_205）；`PlayObject.Operate.cs:1399`（SS_206）；`:1200/1442`（SS_207）；`PlayObject.Message.cs:330`（SS_211）；`UserCastle.cs:368`（SS_212） | 协议常量在 `crates/protocol/src/messages.rs:595-602`；`crates/world/src/castle.rs:1169-1171` 会产 `SS_212` 事件；**game-svr 无发送路径** | ① 建会/解散/联盟/攻城归属不广播；② 单服部署可视为无操作，多服/分区语义缺失 | 单区单服部署下不可见；跨服部署才触发 | L（单服等价；若只做单服可登记「不实现」） | `ss_broadcast_vectors`：断言各动作产生的 `SS_*` 事件 code/tail；单服「无发送 = oracle 在单服也无接收端」需 owner 裁决 |
| 20 | P2 | 组队召回扩展 `AllowGroupReCall`（`@GroupRecall` 消费） | `src/Modules/GameCommand/Commands/GroupRecallCommand.cs:11-57`；`AllowGroupReCallCommand.cs`；`PlayObject.cs:224` | `crates/script/src/engine/actions.rs:90` 已实现 `ActionOfGroupRecall`；`crates/world/src/party.rs:53-56` 明写 `AllowGroupReCall` 字段**无消费点** | ① 字段在实体就位但没人读；② `GroupRcallTime` 冷却未接；③ `NoGuildReCall` 地图旗标未接（行会召回用） | 部署 `Envir/` 无 `RECALLGROUPMEMBERS`/`GroupRecall` 脚本（grep 0）⇒ 当前不可达；GM 命令侧仍可达 | L | `party_recall_flag_vectors`：`AllowGroupReCall=false` 时召回被拒、`true` 时成功并设 `GroupRcallTime`；删字段读取即红 |

## 附 A：已审计、无需派卡（本轮复核结论）

| 面 | 结论与证据 |
|---|---|
| 组队状态机 + 4 个 `CM_GROUP*` + 帧 | 已接线：`bins/game-svr/src/main.rs:1333-1380`（派发）、`party.rs:event_body`（帧）；死亡/掉线退组 `main.rs:13360/14370`；`HAM_GROUP` 已按团长成员表剔除（`crates/world/src/combat.rs:512-525`） |
| 交易状态机 + 6 个 `CM_DEAL*` + 帧 + 掉线中断 | 已接线：`main.rs:2334-2361`；掉线 `DealCancelA` `main.rs:13313/14230/14246`；容量四检/时间门有单测（`trade.rs`） |
| 元宝寄售 7 个 `CM_SELLOFF*` + 世界态 | 已接线：`main.rs:3581-3725`；`selloff.rs` 1958 行，含 oracle 缺陷 1:1（空槽中止、金刚石 65535/32767、扣款帧错位） |
| 拍卖行 5700 `MarketService` | 已 1:1 实现：`bins/db-srv/src/market.rs`/`marketdata.rs`（含 `MarketItem` MemoryPack 恒抛、`QueryMarketItemsCount` 恒 0、表不存在时 SQL 错不回包）；**部署未加载** `MarketSystem.dll`（`M2GameSvr` 下无 `modules.json`/`MarketSystem.dll`）⇒ 不可达 |
| 邮件 | oracle 全仓无实现（仅客户端开关 `src/OpenMir2/Packets/ClientPackets/ClientConf.cs:20 boShowMail`）；Rust 同为不实现（M3 T-36）⇒ **不派卡** |
| `characters` / `_ablity` / `_bagitem` / `_bonusability` / `_indexes` / `_item` / `_item_attr` / `_magic` / `_status` / `_storageitem` | 均有 Rust 读写点（`bins/db-srv/src/playdata.rs` + `db.rs`）；`characters` 38 列与 C# `SaveRecord` 逐列相同（`Job = Job` 怪写照抄）；`characters_status` 同 C# 只写 Status0..14；`bonusability` 两侧都只有 SELECT/UPDATE（`db.rs:464-467`） |
| `characters_quest` | oracle `SaveQuest` 只 DELETE、无调用点 ⇒ 永不落库；Rust 由 `bins/db-srv/tests/quest_no_persist.rs` 反向钉死（0 引用） |
| `characters_item_attr` / `GameGold` | 已知缺陷 B-116/B-117 已登记（无 INSERT / `GameGold` 与 `Gold` 同列、重登回档），本轮复核与代码一致 |
| `goldsales` | 读路径已接（`bins/game-svr/src/goldsales.rs`），C# 列名错 `DealChrName/BuyChrName` 照抄（D-5/B-5）；`SaveSellOffItemList` 三个调用点在 C# 全注释 ⇒ 两侧都不写 |
| `account` / `account_protection` 的行数/建号事务 | 建号 + 读账号已 1:1；**唯一缺口见 #14（字段未透传）** |

## 附 B：死表 / 不可达面（登记，不派卡）

- `mir2_account.sql` 中 11 张表在 C# `src/**` 与 Rust 均 **0 引用**：`account_friend`、`tbl_conncheckuser`、`tbl_connectlogs`、`tbl_countlogs`、`tbl_gmip`、`tbl_itemgive`、`tbl_m2premiumuser`、`tbl_pubips`、`tbl_selectgateips`、`tbl_serverinfo`、`tbl_usercount`（大小写不敏感 grep 亦 0）。判据：`rg -il <table> src --glob '*.cs'` 与 `mir2-rs --glob '*.rs'` 均无输出。
- `upgradeinfo` / `upgradeinfo_item` / `upgradeinfo_item_attr`：C# 存取器是 **返回 -1 的空桩**（`src/GameSrv/DB/MySqlDB.cs:339-348`），全仓无 SQL ⇒ Rust 不实现即等价；**功能缺口**在 #10（武器升级 NPC），不在 DB。
- `marketitems` / `marketitem`：仓库 DDL 无这两张表，C# SQL 会报错且异常被吞（`MarketStoageService.cs:28-29`）；Rust 照抄。不可达见附 A。
- GameGate 的 `ChatFilter=1` 但 C# 中 `AbusiveFilter.CheckChatFilter` 两处调用**均被注释**（`ClientSession.cs:566/589`）⇒ 过滤器是死代码；`ChatCmdFilter=0`、`ProcClientCount=0`、`UseCloudGate=0`、`EnableOtp` 的校验代码亦被注释（`:179-187/1229`）⇒ 均不派卡。
- `MaxConnectOfIP`：GameGate 的 `Filter.cs:160-185` 整段注释、LoginGate/SelGate 的 `IsConnLimited` 循环也注释（`GateShare.cs:82-99`）⇒ oracle 无实际 IP 限流，Rust 未实现即等价。
- GameGate `ClientTimeOutTime=15000` 是死键：真正的会话超时是硬编码 15 天（`GateShare.cs:26`），见 #18。
- `CastleNameLen` / `GuildNotice/GuildWar/GuildAll/GuildMember` 段头在 `ServerConf.LoadConfig` 无读取点（`crates/data/src/guild_castle.rs` 已登记）⇒ 不做配置读取。

## 附 C：复核命令（只读，可复算）

```bash
# 行会派发/帧缺口
grep -n "CM_GUILD" mir2-rs/bins/game-svr/src/main.rs            # 无输出
grep -n "GUILD" mir2-rs/bins/game-svr/src/smframe.rs            # 无输出
# 行会/攻城状态机存在但未接线
grep -rn "GuildStore\|load_guild_list" mir2-rs/crates mir2-rs/bins --include=*.rs | grep -v crates/data/src/guild.rs
grep -rn "CastleState\|CastleManager\|mir2_envir::castle" mir2-rs/bins/game-svr/src --include=*.rs   # 仅 ctor_fold.rs 注释
# 城堡税丢弃
grep -n "let _tax" mir2-rs/bins/game-svr/src/shop.rs             # 935/1040/1215
# 频道聊天未接
sed -n '17,18p;39p' mir2-rs/bins/game-svr/src/chat.rs
# 网关速度规则只实现聊天
grep -n "IsChatInterval\|ChatInterval" mir2-rs/bins/game-gate/src/config.rs
# 网关 Enter 帧体
sed -n '134,140p' mir2-rs/bins/login-gate/src/main.rs mir2-rs/bins/sel-gate/src/main.rs
# account_protection 字段
grep -n "UPDATE account_protection" mir2-rs/crates/storage/src/account.rs   # 只 UserName/Quiz2
# 武器升级未建模
grep -rn "UpgradeWapon\|getbackupgnow" mir2-rs --include=*.rs                # 只有 parser 标志
# 死表
for t in account_friend tbl_conncheckuser tbl_connectlogs tbl_countlogs tbl_gmip tbl_itemgive tbl_m2premiumuser tbl_pubips tbl_selectgateips tbl_serverinfo tbl_usercount; do grep -ril "$t" src --include=*.cs | wc -l; done
```

> 所有 grep 均为只读；未写入仓库、未改动 `E:/MirServer`。


---

## 评审员的更正（2026-10-19，**以本节为准**）

本节由独立评审者逐条抽查后追加；上表保留原样以便对照，但下列更正优先。

### D-1（#13）LoginSrv 的 `user_ip/gate_ip` 写反
- `src/LoginSrv/Services/LoginServer.cs:189`：`HUtil32.GetValidStr3(sIPaddr, ref sUserIPaddr, '/')` —— 该函数把**第一段写进 dest**、**返回第二段**（`src/OpenMir2/HUtil32.cs:384-389`）。
- Rust 发 `"IP:port"`（无 `/`）⇒ `sUserIPaddr="IP:port"`、`sGateIPaddr=""`。原文写反了。
- 另：LoginGate 的 `UserEnter` 是 `$"{ClientIP}/{ClientIP}"`（`src/LoginGate/Services/ClientSession.cs:204-207`），**没有 `%K{sid}`**；只有 SelGate 是 `$"%K{SocketId}/{IP}/{IP}$"`（`src/SelGate/Services/ClientSession.cs:166-169`）。DBSrv 的后果是两者都为空（`ArrestStringEx` 找不到 `%…$`，`src/DBSrv/Services/Impl/UserService.cs:217-221`）。

### D-2（#4/#7/#8）#4/#7/#8 的「P0 生产可达」不成立
- 部署件：`Castle/0/AttackSabukWall.txt` 0 字节；`Castle/1/` 只有空的 `SabukW.txt`；`Castle/0/SabukW.txt` 的 `OwnGuild=` 空、`TotalGold=0`。
- oracle 的攻城入口是 **N1**（`UserCastle.AddAttackerInfo` 抛 `NotImplementedException`，`src/M2Server/Castle/UserCastle.cs:670-673`；`ChangeSabukLordCommand.cs:39`）⇒ **无城主、无攻击者、无开战入口**。
- 因此 #4 的开战/时限、#7 的攻城区/766 bit4、#8 的城堡税/成员价**当前不可达**；#6 的 `CastleOfficial.UserSelect` 还有 `IsMasterGuild && IsGuildMaster` 门（`src/GameSrv/Npc/CastleOfficial.cs:107`），无城主时 oracle 直接返回。
- 仍可达：#5 的城门/城墙/弓手实体（启动刷出）、#6 的 NPC 对话文本。建议 #4/#7/#8 降为 P1/P2 或等 owner 对 N1 裁决。

### D-3（#7）漏了更可达的 `Random(5)` 路径
- `Random(9)` 只在「城主行会成员进皇宫」用（`src/M2Server/Castle/UserCastle.cs:649-657`）；无城主时走非成员分支 `HomeX-2+Random(5)`/`HomeY-2+Random(5)`（`src/GameSrv/Word/WorldServer.cs:305-307/339-340`）——断言须同时覆盖这两次抽取的顺序。

### D-4 多数条目是「已登记未接线」，不是「未审计」
- #1/#2（行会）、#4/#5（攻城）、#6（N1/N5）、#8（税）、#10（武器升级）、#3 的 `@requestcastlewarnow` 均已在 `M3⑧公会攻城现状盘点报告-20261013.md` 与各模块注释里登记（`guild.rs:44-54`、`castle.rs:963-966`、`shop.rs:935/1040/1215`、`smframe.rs:207`）。
- 本表的价值是**统一派卡清单**；派卡时请引用既有卡号，不要当新发现重复排队。
