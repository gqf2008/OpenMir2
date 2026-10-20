# Lane castledesign — 攻城区语义与攻城状态机实施简报（2026-10-21）

- 基线：`e9c1e8654`（master，2026-10-20 工作树快照）。
- 车道性质：**tier=L，纯文档 + 只读**。本文件是唯一改动；未改 `src/**`、`sql/**`、`MirClient/`、
  `mir2-rs/crates/{protocol,storage}`、`E:/MirServer/**`；未起服务端/客户端/代理，未占端口，未写库。
- 输入：`mir2-rs/tests/parity/m3/audit-uncovered-surfaces-20261019.md` #4 / #7 与评审更正 D-2/D-3；
  oracle `src/M2Server/Castle/{UserCastle.cs,CastleManager.cs,CastleConf.cs,CastleConst.cs}`、
  `src/GameSrv/GameApp.cs`、`src/GameSrv/Word/WorldServer.cs`、`src/GameSrv/Word/Threads/SystemProcessor.cs`、
  `src/M2Server/Player/{PlayObject.cs,PlayObject.Base.cs,PlayObject.Message.cs}`；
  Rust `crates/world/src/castle.rs`、`crates/envir/src/castle.rs`、`crates/envir/src/registry.rs`、
  `bins/game-svr/src/{castle.rs,enter.rs,mapwire.rs,main.rs,downlink.rs}`。
- 口径：**行为等价优先于修 bug**（硬约束③）。本文把「已实现但未接线」「oracle 自身死链（N1）」
  「必须照抄的怪行为」三类分开写，不把既有缺口当新发现。
- 判级：本文档本身 **L**；它定义的实现判据按仓库 tiered 政策逐条标 tier（RNG/时序/持久化 ⇒ **R**），
  R 项未过窗口前不得 `closed`。

---

## 0. 摘要与关键结论

1. **攻城区语义主链四处全缺**：`InCastleWarArea` 谓词、进图落点改写（成员/非成员两臂）、
   死亡落点改写（三臂）、766 `SM_AREASTATE` 的 bit4 动态置位/清除。Rust 现状是
   `enter::area_state` 只算 `FightZone|1`、`SafeArea|2`，`PLAY_NAME_COLOR=255` 恒，进图无城堡判定。
2. **状态机世界态已实现、调用点与落盘全缺**：`crates/world/src/castle.rs` 已有
   `UserCastle::run`（跨天/每日开战/回收 Ghost/围墙 StoneMode/结束预告/超时停战）、
   `initialize`、`add_attacker_info`、`main_door_control`、`repair_*`、10 个 N1 no-op；
   `bins/game-svr` 只在启动调 `initialize_castle`（`main.rs:14995-15033`），**没有**
   `CastleMgr.Run`（10s）与 `CastleMgr.Save`（10min）调用点，也没有 `LoadCastleList` 接线。
3. **RNG 账本**：进图城堡分支只在两条路抽数——非成员 `Random(5)`×2（X 先 Y 后）、
   城主行会成员在皇宫 `Random(9)`×2（`GetHomeX` 先、`GetHomeY` 后）；
   死亡回落另有 `Random(9)`×2 / `Random(5)`×2 / `Random(13)`×2 三臂。
   这些抽数发生在 **地形移位环（`Random(6)`×2/轮）之前**，与 `GetHomeInfo` 的 3 抽同一条 `world.rng`；
   漏抽/多抽会让后续全局 RNG 流整体偏移。
4. **部署可达性（D-2，按真实可达性分层）**：`Castle/0/AttackSabukWall.txt` 0 字节、
   `OwnGuild=` 空、`AddAttackerInfo(IGuild)` 与 `GetCastle(IGuild)` 抛 `NotImplementedException` ⇒
   **无城主、无攻击者、日常 20 点开战不可达**；`UnderWar` 只有 GM 命令
   `@ForcedWallConQuestWar`（`Command.conf:159/330`，权限 10）或测试注入可达。
   **但「进图改写」不依赖 `UnderWar`**：外层条件是 `(PalaceEnvir == envir || UnderWar)`
   （`WorldServer.cs:300`），部署 `Envir/MapInfo.txt:857-866` 有 `3 -> 0150` 的正常行走路线 ⇒
   **非成员走入皇宫 0150 时即触发非成员改写臂（2×`Random(5)`）**，此路径自然可达；
   成员臂（需 `MasterGuild != null`）、死亡臂、战时分支仍需注入/GM。因此 #4/#7 的 R 级窗口
   **不能整体推迟**：至少「非成员进皇宫」必须在窗口内真机对拍，其余构造级判据先绿。
5. **名字色必须照抄 N1 异常**：`GetChrColor` 的城堡分支在 `MyGuild != null` 时走到
   `castle.IsAttackGuild(IGuild)`（`UserCastle.cs:1045-1048`）⇒ 抛异常被
   `PlayObject.Run` 的消息段 `catch` 吞掉（`PlayObject.Message.cs:356-400`）⇒
   **当轮剩余消息全部不再出队**（不只是名字帧）。无行会 viewer 才会正常得到
   `InFreePKAreaNameColor=221`。不得「补全」成真实攻守色。
6. **文件持久化怪癖（必须逐字节照抄）**：
   `CastleConf.SaveConfig` 的 `WriteString/Save` 走的是 **ctor 时**绑定的文件路径
   （`Castle/<CastleDir>/SabukW.txt`），不是 `ConfigDir` 指向的目录；
   且 `LoadConfig` 对 `EnvirList` **只 append 不清空**，`CastleManager.Initialize` 又追加 6 张图 ⇒
   每次启动 `CastleMapList` **+6**。部署件实测 1314 项 = 6×219（219 次启动），是这条怪行为的活证据。
   `Castle/List.txt` 则稳定在 1↔2 行（启动时先写 1 行，装载后第一次 10min Save 写 1+L 行）。

---

## 1. 可达性与判级口径

### 1.1 D-2 复核（以本节为准）

| 说法 | 复核结果 | 证据 |
| --- | --- | --- |
| 有城主 | **否**。`OwnGuild=` 空；且 `MasterGuild = GuildMgr.FindGuild(OwnGuild)` 在 `CastleMgr.Initialize`（`GameApp.cs:213`）内执行，早于 `GuildMgr.LoadGuildInfo`（`GameApp.cs:245`）⇒ 即使 `OwnGuild` 非空也解析为 null | `Castle/Castle/SabukW.txt` `[Setup] OwnGuild=`；`UserCastle.cs:228-232`；`GameApp.cs:213/245` |
| 有攻城申请 | **否**。`Castle/0/AttackSabukWall.txt` 0 字节；`AddAttackerInfo(IGuild)` 抛 N1 | 部署件；`UserCastle.cs:670-673` |
| 日常 20 点自动开战 | **不可达**（`AttackWarList` 恒空 ⇒ `UnderWar` 不会置真） | `UserCastle.cs:340-376` |
| 自动占领 / 改归属 | **不可达**（`CanGetCastle(IGuild)`/`GetCastle(IGuild)` 抛 N1） | `UserCastle.cs:644-647/675-678`；`ChangeSabukLordCommand.cs:39` |
| `UnderWar` 唯一入口 | GM `@ForcedWallConQuestWar`（切换 `UnderWar`，权限 10） | `ForcedWallconquestWarCommand.cs:19-53`；`Command.conf:159/330` |
| 城门/城墙/弓手实体 | **可达**（启动刷出，已由实体挂图片实现） | `design-castle-entities-20261020.md`；`main.rs:14995-15033` |
| 城堡 NPC 文本 | 可达；动作受 `IsMasterGuild && IsGuildMaster` 门限制 | `CastleOfficial.cs:107` |
| 进入皇宫 `0150` | **可达**（自然行走）。`3 -> 0150` 有 6 条路线；进图改写外层条件命中的是 `PalaceEnvir == envir`，**不要求** `UnderWar` | `Envir/MapInfo.txt:857-866`（其中 `3 -> 0150` 共 6 条）；`WorldServer.cs:300` |

> 结论：#4/#7 的 **R 级判据必须设计**。当前部署下：**非成员进皇宫 0150 自然可达**
> （`3 -> 0150` 行走 + `PalaceEnvir == envir` 分支），必须在真机窗口对拍；
> 成员/死亡/战时分支不可达，需「注入世界态」（`UnderWar=true`、`MasterGuild=Some`）驱动，
> 不依赖 GM 命令或改内容。

### 1.2 判级

- 本文档：**L**（纯文档 + 只读复算）。
- 第 8 节每条实现判据各自标 tier；R = 必须真机全量对拍（RNG / 时序 / 文件持久化）。
- 按仓库政策：R 级卡 `merge_result` 后保持 `in-progress`，窗口项见第 9 节。

---

## 2. oracle 锚 → Rust 现状 → 缺口（逐条）

### 2.1 启动装载与列表

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| A1 | `GameApp.cs:213` | `CastleMgr.Initialize()`：在 `LoadMapQuest`(`:184`) 之后、`LoadMerchant`(`:227`) 之前 | 已接线：`main.rs:14995-15033` 调 `initialize_castle`（RNG 同 `world.rng`） | 仅实体半；缺列表/落盘半 |
| A2 | `CastleManager.cs:62-88` | 启动时 `CastleList` 空 ⇒ `new UserCastle(Config.CastleDir)`（ctor `ConfigDir="Castle"`，读 `Castle/Castle/SabukW.txt`）→ `castle.Initialize()`（造实体）→ `ConfigDir="0"` → `EnvirList.Add("0151".."0156")` → 入表 → `Save()` | `bins/game-svr/src/castle.rs::load_sabuk_specs` 固定读 `Castle/<castle_dir>/<castle_dir>/SabukW.txt`；`initialize_castle` 入 `World.castles` | ① 没有「1 + L 座城」模型；② 没有 `EnvirList` 追加；③ 没有 `Save()` |
| A3 | `GameApp.cs:246` | `CastleMgr.LoadCastleList()`：读 `Castle/List.txt`，逐行 `Trim()`、空行跳过、`new UserCastle(dir)`，**只 append、不 Initialize** | `envir::MapLoader::load_castle_list`（`crates/envir/src/castle.rs:561`）已实现；`CastleManager` 未建模 | 未接线；未建模「装载城 `MapName=null` ⇒ `Run` 早退、`Save` NRE」 |
| A4 | `CastleManager.cs:154-176` | 装载后 `CastleList` = 1 + L；装载城 `MapName`/`MasterGuild`/`EnvirList`/`Archers[]` 均空 | 无 | 同 A3 |
| A5 | `CastleManager.cs:178-192` | `SaveCastleList` 写下标 `i.ToString()`，`StringList.SaveToFile` = CRLF、无 BOM | `envir::serialize_castle_list(count)` 已实现 | 未接线 |
| A6 | `SystemProcessor.cs:54-62` | `CastleMgr.Save()` 每 `ConsoleShowUserCountTime`（部署 600000ms）一次 | 无 | 10min 门 + 落盘 |
| A7 | `SystemProcessor.cs:70-77` | `CastleMgr.Run()` 每 10s 一次（与 `GuildMgr.Run` 同一块） | `UserCastle::run` 已实现；`sim_loop` 只在 `16940-16975` 跑 `guild_runtime::step` | 10s 门 + 城堡 run 事件排水 |

### 2.2 攻城区谓词

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| B1 | `UserCastle.cs:454-478` | `InCastleWarArea(envir,x,y)`：`envir==null` ⇒ false；`envir==CastleEnvir` 且 `abs(HomeX-x)<WarRangeX` 且 `abs(HomeY-y)<WarRangeY` ⇒ true；`envir==PalaceEnvir` 或 `envir==SecretEnvir` ⇒ true；`EnvirList` 逐项 `OrdinalIgnoreCase` 比 `envir.MapName` ⇒ true；否则 false | **无** | 谓词整体缺失 |
| B2 | `CastleManager.cs:38-59` | 两个重载：按 actor 取 `Envir/CurrX/CurrY`，按 `Envir`+坐标；按 `CastleList` 顺序返回首个命中 | **无** | 需在 `World` 侧提供「act/坐标 → 城堡」查询 |
| B3 | `CastleManager.cs:115-124` | `IsCastleMember(playObject)`：按列表序 `Castle.IsMember` = `IsMasterGuild(MyGuild)`（只认**城主行会**，不含盟友） | **无**；`World.actor_guilds` 已有读写口（`world.rs:3430-3446`），bins 尚未写 | 会话 `MyGuild` → `GuildId` 映射 + 查询 |
| B4 | `UserCastle.cs:639-642`；`UserCastle.cs:228-232` | `IsMasterGuild(guild)`：`MasterGuild != null && MasterGuild == guild`；`MasterGuild = GuildMgr.FindGuild(OwnGuild)` 在 `LoadConfig`（`Initialize` 内）执行 | `is_master_guild(GuildId)` 已有；`master_guild: Option<GuildId>`（`castle.rs:882`） | **读序**：`CastleMgr.Initialize`（`GameApp.cs:213`）早于 `GuildMgr.LoadGuildInfo`（`GameApp.cs:245`）⇒ 启动时 `FindGuild` 必返回 null；即使 `OwnGuild` 非空，`MasterGuild` 也是 null。Rust 启动装载应照抄该读序（`master_guild=None`），不要「先读行会表再反查」 |

### 2.3 进图/死亡落点改写

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| C1 | `WorldServer.cs:298` | `MyGuild = GuildMgr.MemberOfGuild(ChrName)`（进图时物化） | G2 已在登录物化 `guild_session.guild_name`（`main.rs:16410-16419`）；`World.actor_guilds` 未写 | 写 `set_actor_guild` |
| C2 | `WorldServer.cs:299-318` | `InCastleWarArea(envir,CurrX,CurrY)`；若 `envir!=null` 且 `castle!=null` 且（`castle.PalaceEnvir==envir` 或 `UnderWar`）：① `IsCastleMember==null` ⇒ `MapName=HomeMap; CurrX=HomeX-2+Random(5); CurrY=HomeY-2+Random(5)`；② 成员且 `PalaceEnvir==envir` ⇒ `MapName=castle.GetMapName(); CurrX=GetHomeX(); CurrY=GetHomeY()`（各 `Random(9)`） | **无** | 整段缺失；须插在 `resolve_enter_map` 之后、`shift_enter_position` 之前 |
| C3 | `WorldServer.cs:319-322` | `FindMap(MapName)==null` ⇒ `Abil.HP=0` | `resolve_enter_map` 的 Err 分支拒绝进图（已登记差异） | 改写后需按新 `MapName` 重新解析 `map_id` |
| C4 | `WorldServer.cs:324-350` | `HP<=0` 时：`PvpLevel<2` 且 `IsCastleMember!=null && UnderWar` ⇒ `MapName=castle.HomeMap; CurrX=GetHomeX(); CurrY=GetHomeY()`；否则本人 `HomeMap` + `Random(5)`×2；`PvpLevel>=2` ⇒ `RedDieHomeMap` + `Random(13)`×2 | 进图路径无死亡回落；`hp<=0` 的复活/回城属其他片 | 三臂整段缺失；`RedDieHomeMap/X/Y` 需配置层输入 |
| C5 | `WorldServer.cs:371-395` | 地形移位环：首格可走 0 抽；否则每轮 `-3+Random(6)` X 先 Y 后，最多 5 轮；仍不可走 ⇒ 回落 `[Setup] HomeMap/HomeX/HomeY` | `mapwire::shift_enter_position` 已实现 | **顺序**：城堡改写必须在其之前；改写后 `map_id` 需重解析 |

### 2.4 766 `SM_AREASTATE` 与 `InGuildWarArea`

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| D1 | `PlayObject.Base.cs:270-286` | `RefUserState`：`n8 = FightZone bit0`、`SafeArea bit1`、`InGuildWarArea bit2`；`SendDefMessage(SM_AREASTATE, n8, 0,0,0)` | `enter::area_state(flag)`（`enter.rs:366`）只算 1/2；`sm_areastate`（`smframe.rs:585`）编码正确 | 加 bit4 参数/输入 |
| D2 | `PlayObject.cs:905` | ctor `InGuildWarArea=false` | 无对应会话字段 | 新增会话位 |
| D3 | `PlayObject.Message.cs:286-290` | **每 tick**：`castle=InCastleWarArea(this); if (castle!=null && UnderWar) ChangePkStatus(true)` | 无 | 每 tick 城堡区检查 |
| D4 | `PlayObject.Message.cs:291-343` | **1s 门**（`GetTickCount()-DiscountForNightTick>1000`）内：`if (castle!=null && UnderWar) {皇宫自动占领链(N1)} else ChangePkStatus(false)` | 无 | 清除路径 + N1 |
| D5 | `PlayObject.Message.cs:344-349` | 1s 门内 `if (NameColorChanged) { NameColorChanged=false; RefUserState(); RefShowName(); }` | 无 | 766 重发 + 名牌重发 |
| D6 | `PlayObject.cs:4343-4350` | `ChangePkStatus(bo)`：值不同才写 `InGuildWarArea` 并置 `NameColorChanged=true` | 无 | 状态位与变更标记 |
| D7 | `PlayObject.Message.cs:1687/1777` | `RM_LOGON` / `RM_CHANGEMAP` 无条件 `RefUserState()` | 登录帧 766 已接线（`enter.rs:650-651`），但 bit4 恒 0 | 登录 766 需带 bit4 |
| D8 | `PlayObject.Message.cs:1677-1679` | `RM_CHANGENAMECOLOR` ⇒ `SM_CHANGENAMECOLOR(656, ActorId, GetChrColor(target))` | 无 | 名字色分支（见 2.5） |

### 2.5 名字色 / 名牌

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| E1 | `PlayObject.cs:3585-3657` | `GetChrColor` 玩家分支：先 `NameColor`（ctor 255），`PvpLevel<2` 时套 `PvpFlag`/`GetGuildRelation`/`Fight3Zone`；再套城堡分支 | `enter::PLAY_NAME_COLOR=255` 常量；`downlink` 用展示表 `name_color`（恒 255） | 分支整体缺失；见 §5 的 N1 异常结论 |
| E2 | `PlayObject.Base.cs:3529-3559` | `GetGuildRelation`：两边有会、都不在安全区、`play.MyGuild.GuildWarList.Count>0` 才有效；2=互相宣战、1=同会、3=互相联盟；副作用 `GuildWarArea` | `World.is_ally_guild` 有；战争列表/安全区判定未接 | 依赖行会战状态 |
| E3 | `PlayObject.cs:3300-3388`（城堡段 `:3311-3327`） | `GetShowName`：`IsCastleMember!=null` ⇒ `CastleGuildName` 替换 `%castlename/%guildname/%rankname`；否则 `InCastleWarArea!=null` 且（`ShowGuildName` 或 `UnderWar` 或 `InGuildWarArea`）⇒ `NoCastleGuildName` | `enter::show_name` 把 `%guildname` 恒替换为 `""`（`enter.rs:338`） | 两个模板键 + 城堡/战争门 |
| E4 | `MessageSettings.cs:228-229`；`StringConf.cs:236-237` | 代码默认模板：`CastleGuildName="(%castlename)%guildname[%rankname]"`，`NoCastleGuildName="%guildname[%rankname]"`；**部署 `String.conf:223-224` 实有这两键**，值分别为 `%guildname(%castlename)` 与 `%guildname`（均无 `[%rankname]`） | `StringConf` 只有 `guild_*` 六键，无这两个 | 配置读侧补两键；期望串**以部署值为准**，不得用 `MessageSettings` 默认值 |
| E5 | `BaseObject.cs:1174-1176` | `RefShowName` = `SendRefMsg(RM_USERNAME, 0,0,0,0, GetShowName())`（发给同屏观察者） | `sm_username`/AOI 已有 | 名字色 + 城堡名牌接入 |
| E6 | `PlayObject.cs:3589/3616` | 观者/目标都 `InGuildWarArea` 且 `UnderWar` 时先给 `InFreePKAreaNameColor`，随后 **`GuildWarArea=true`**（副作用） | `GuildWarArea` 未建模 | 副作用必须照搬（影响同 tick 的攻击判定） |

### 2.6 状态机与文件

| # | oracle 锚 | 行为 | Rust 现状 | 缺口 |
| --- | --- | --- | --- | --- |
| F1 | `UserCastle.cs:311-446` | `Run`：`MapName` 空⇒return；非本服⇒return；跨天重置；每日 20 点开战；回收 Ghost；战时 StoneMode=false/预告/超时停战；非战 StoneMode=true | `UserCastle::run`（`castle.rs:1155-1274`）逐行实现 | 无调用点（见 A7） |
| F2 | `UserCastle.cs:564-572` | `StartWallconquestWar`：皇宫范围 100 内玩家 `RefShowName` | `start_wallconquest_war` 产 `RefreshShowName` 事件 | 宿主排水 |
| F3 | `UserCastle.cs:577-597` | `StopWallconquestWar`：清 `UnderWar`/`AttackGuildList` + 广播；玩家处理循环因 `GetMapOfRangeHumanCount` 丢弃返回值而**恒空** | `stop_wallconquest_war` 照搬（无位移事件） | 广播排水 |
| F4 | `UserCastle.cs:242-288` | `LoadAttackSabukWall`：每行 `会名<空白>带引号日期`，未知行会丢弃，`TryParse` 失败用 `Now` | `envir::parse_attack_sabuk_wall_line` 已实现 | 接到 `UserCastle.attack_war_list` |
| F5 | `UserCastle.cs:293-309` | `SaveAttackSabukWall`：`会名 + 7 空格 + 引号日期`，CRLF | `envir::serialize_attack_sabuk_wall` 已实现 | 接线 |
| F6 | `UserCastle.cs:377-389` | 回收 Ghost：先 `Guards` 后 `Archers`，只清 `BaseObject` | `run` 已实现 | — |
| F7 | `UserCastle.cs:890-909` | `MainDoorControl(bool)`：`BaseObject==null` 或 `Ghost` 不动；只在状态不符时开/关 | `main_door_control` 已实现 | 事件排水 |
| F8 | `CastleConf.cs:15-94` | `LoadConfig` 全键 + 默认；`EnvirList` 逐段 append；构造 `Archers/Guards` 12 项 | `envir::SabukWConfig::from_ini` 全实现 | `CastleDefenseSpecs` 只带 map/palace/secret/门墙弓守，**缺** `envir_list/home_x/home_y/war_range` |
| F9 | `CastleConf.cs:96-...` | `SaveConfig`：归属服门；`CastleMapList` 逗号串 append；`ChangeDate/WarDate/IncomeToday` 恒写；`TotalGold/TodayIncome` 仅非 0 时**更新缓存值**；`MainDoorOpen/HP` 仅 `BaseObject!=null` 时更新；`Archers/Guards` 的 `HP` 无论 `BaseObject` 都写（空写 0）。**写侧是 `ConfigFile` 的全量缓存重写**：`LoadConfig` 的 `ReadWrite*` 会把缺键的默认值写入 `iniCahce`（`ConfigFile.cs:56-64` 等），`Save()` 遍历 `iniCahce` 全量写回（`ConfigFile.cs:363-377`）⇒ 上述「非 0/非空才写」只决定**值是否更新**，不决定键是否存在；缺键会以默认值出现在输出中 | **无写侧** | 全部缺失；实现必须把「键存在性」与「条件写值」分开建模 |
| F10 | `CastleManager.cs:144-152` | `Save()` = `SaveCastleList()` 然后逐城 `Save()` | 无 | 同上 |

---

## 3. RNG 抽数账本（进图路径）

`M2Share.RandomNumber.Random(n)` = `System.Random.Next(n)` ⇒ **[0, n-1]**；
Rust 对应 `RandomNumber::random_below(n)`（`crates/shared/src/rng.rs:274`）。

### 3.1 抽数点与精确位置

`WorldServer.AddUser`（`src/GameSrv/Word/WorldServer.cs`）的同步序：

```text
:250-279   HomeMap 空 ⇒ GetHomeInfo(0/1 抽) + GetRandHomeX/Y(各 1×Random(3))
:280-297   GetMapInfo + Fight3Zone 处理（0 抽）
:298       MyGuild = MemberOfGuild(ChrName)                       （0 抽）
:299       InCastleWarArea(envir, CurrX, CurrY)                   （0 抽）
:300-318   城堡改写：见下表                                       （0/2 抽）
:319-322   FindMap(MapName)==null ⇒ HP=0                          （0 抽）
:324-350   HP<=0 死亡改写：见下表                                  （0/2 抽）
:371-395   地形移位环：每轮 X 后 Y 各 1×Random(6)，最多 5 轮     （0..10 抽）
```

| 路径 | oracle 行 | 公式 | 抽取次数 | 顺序 |
| --- | --- | --- | --- | --- |
| 非成员（`IsCastleMember==null`） | `:305-307` | `MapName=HomeMap; CurrX=HomeX-2+Random(5); CurrY=HomeY-2+Random(5)` | **2×Random(5)** | **X 先、Y 后** |
| 成员且 `PalaceEnvir==envir` | `:313-315` | `MapName=castle.MapName; CurrX=HomeX-4+Random(9); CurrY=HomeY-4+Random(9)` | **2×Random(9)** | **X 先、Y 后** |
| 成员但不在皇宫（城堡图/EnvirList 图战时） | — | 不改写 | **0** | — |
| 非皇宫图且 `!UnderWar`（含 0151..0156 非战） | — | 条件不成立 | **0** | — |
| 死亡：成员 + `UnderWar` | `:330-334` | `MapName=castle.HomeMap; CurrX=HomeX-4+Random(9); CurrY=HomeY-4+Random(9)` | **2×Random(9)** | X 先 Y 后 |
| 死亡：非成员/非战时 | `:336-341` | 本人 `HomeMap` + `HomeX-2+Random(5)`×2 | **2×Random(5)** | X 先 Y 后 |
| 死亡：红名 `PvpLevel>=2` | `:343-348` | `RedDieHomeMap` + `RedDieHomeX+Random(13)` / `RedDieHomeY+Random(13)` | **2×Random(13)** | X 先 Y 后 |

> **关键顺序约束**：城堡抽数在移位环**之前**；`GetHomeInfo` 的 3 抽在最前。
> 任何一条漏抽/多抽，都会让之后所有全局抽数（移位、刷怪、攻击…）整体错位。
> `Random(9)` 还出现在 `PlayObject.Base.cs:2272`（`castle.GetHomeX/GetHomeY` 的回家技能支），
> 因此 `GetHomeX/Y` 必须实现成**共享函数**，不能在两处各写一份公式。
> 另：成员/死亡两臂的 `HomeX/HomeY` 是 **`castle.HomeX/HomeY`**（`UserCastle` 的字段），
> 非成员/非战时两臂是 **`playObject.HomeX/HomeY`**（角色自己的回城点）；部署两者同为 644/290，
> 但契约上不可混用。

### 3.2 Rust 落点

- 抽数必须走 `world.rng`（`mem::replace` 移出/还回，与 `step_mon_gen`、`castle::initialize_castle` 同法）。
- 建议在 `crates/world/src/castle.rs` 增加：
  ```text
  get_home_x(&self, rng: &mut RandomNumber) -> i16   // HomeX - 4 + rng.random_below(9)
  get_home_y(&self, rng: &mut RandomNumber) -> i16
  ```
  这样 `WorldServer` 进图、死亡、回家技能三处共用同一实现与同一 trace 站点。
- `rng_trace` 站点：`random_below` 已 `#[track_caller]`，调用点会落在 `castle.rs`；
  需在 `tests/parity/m3/rng-trace-probe/consumer_groups.tsv` 登记新组（`castle_enter`）。

---

## 4. 766 bit4 规格

### 4.1 帧字节

`SendDefMessage(Messages.SM_AREASTATE, n8, 0, 0, 0)`：
ident=`766`，`Recog=n8`，`Param=0`，`Tag=0`，`Series=0`，**无尾**。
Rust 编码器 `smframe::sm_areastate(n8)` 已逐字节对上金标准。

`n8` 位定义（`PlayObject.Base.cs:270-286`）：

| bit | 值 | 来源 |
| --- | --- | --- |
| 0 | 1 | `Envir.Flag.FightZone`（`FIGHT`；`0151..0156` 为真，`3`/`0150`/`D701` 为假） |
| 1 | 2 | `Envir.Flag.SafeArea`（`SAFE`） |
| 2 | **4** | `InGuildWarArea`（本片新增） |

### 4.2 置位条件（每 tick）

`PlayObject.Message.cs:286-290`（在 `Run()` 的**消息段之前**执行）：

```text
castle = CastleMgr.InCastleWarArea(this);      // actor.Envir + CurrX/CurrY
if (castle != null && castle.UnderWar)
    ChangePkStatus(true);                      // 仅在值不同时：InGuildWarArea=true; NameColorChanged=true
```

- 条件只用 `castle != null && UnderWar`；**不要求**有行会、不要求是城主。
- `castle` 是 `CastleList` 顺序首个命中者。
- 每 tick 评估，值变化才置 `NameColorChanged`。

### 4.3 清除条件（1s 门内）

`PlayObject.Message.cs:291-343`：只有在
`GetTickCount() - DiscountForNightTick > 1000` 的 tick 才执行：

```text
if (castle != null && castle.UnderWar) {
    if (Envir == castle.PalaceEnvir && MyGuild != null) { /* 自动占领链，N1 抛异常 */ }
} else {
    ChangePkStatus(false);                     // InGuildWarArea=false; NameColorChanged=true
}
```

- **清除被 1s 门节流**：离开攻城区后，`InGuildWarArea` 最多再保持约 1s。
- 在皇宫且有行会时，自动占领链在 `IsAttackGuild(IGuild)` 抛 N1 ⇒ 该 tick 后面的
  `NameColorChanged` 块被跳过（见 §5.3）。

### 4.4 发送点

| 场景 | 锚 | 是否带 bit4 |
| --- | --- | --- |
| 登录 `RM_LOGON` | `PlayObject.Message.cs:1687` | **可能带**：`Run()` 先做城堡检查（286-290）再排消息段（356-366）⇒ `RM_LOGON` 的 `RefUserState` 读到的是本 tick 已置位的 `InGuildWarArea` |
| 换图 `RM_CHANGEMAP` | `:1777` | 同上（换图后由该 tick 的检查决定） |
| 状态变化 | `:344-349`（1s 门） | 是：`NameColorChanged` ⇒ `RefUserState()` + `RefShowName()` |
| `RM_CHANGENAMECOLOR` | `:1678` | 不是 766；是 `SM_CHANGENAMECOLOR(656)` |

> 实现注意：登录 766 的 bit4 不是「恒 0」。金标准里 bit4=0 是因为金标准场景不在攻城区/无战争。
> 必须在**同一 tick 的城堡检查之后**装配登录帧，或在装配前显式计算一次 `InGuildWarArea`。

### 4.5 `RefShowName` 的连坐

`PlayObject.Base.cs:1174-1176` ⇒ `SendRefMsg(RM_USERNAME, GetShowName())` 给同屏观察者；
观察者在 `Operate` 里用 `GetChrColor(被观察者)` 上色。由于 §5 的 N1 异常，
**有行会的观察者可能因此中断自己当轮的消息批次**。这不是本片要修的东西，是必须照抄的行为。

---

## 5. 名字色（行会 / 敌对 / 攻城区）

### 5.1 颜色值（部署 `Server.conf:740-745`）

| 键 | 部署值 | 十进制 | 用途 |
| --- | --- | --- | --- |
| `PKFlagNameColor` | `47` | 47 | PvpFlag 名字色 |
| `AllyAndGuildNameColor` | `180` | 180 | 同会/盟友 |
| `WarGuildNameColor` | `69` | 69 | 敌对行会 |
| `InFreePKAreaNameColor` | `221` | 221 | 攻城区底名色 |
| `PKLevel1NameColor` | `251` | 251 | 红名 1 级（**GetChrColor 不用**，见 5.2） |
| `PKLevel2NameColor` | `249` | 249 | 红名 2 级（**GetChrColor 不用**） |

缺省值（`GameSvrConf.cs:1731-1736`）与部署一致（0x2F/0xB4/0x45/0xDD/0xFB/0xF9）。

### 5.2 判定顺序（`PlayObject.cs:3585-3657`）

```text
result = baseObject.NameColor                    // 玩家 ctor = 255
if (target.PvpLevel() < 2) {
    if (target.PvpFlag) result = 47;
    switch (GetGuildRelation(this, target)) {
      1 | 3 -> 180;   2 -> 69;
    }
    if (target.Envir.Flag.Fight3Zone)
        result = (MyGuild == target.MyGuild) ? 180 : 69;
}
castle = InCastleWarArea(target);
if (castle != null && castle.UnderWar && InGuildWarArea && target.InGuildWarArea) {
    result = 221; GuildWarArea = true;
    if (MyGuild == null) return 221;
    if (castle.IsMasterGuild(MyGuild)) { ... }
    else { if (castle.IsAttackGuild(MyGuild)) { ... } }   // ← N1 抛异常
}
return result;
```

**怪行为（照抄，不修）**：`PvpLevel>=2` 时整个 `if` 被跳过，`result` 保持
`baseObject.NameColor = 255`；`PKLevel1/2NameColor`（251/249）只被
`PlayObject.GetNameColor()` 使用，而 `GetChrColor` 不用它。⇒ 本函数对红名玩家返回 **255**。

### 5.3 城堡分支的 N1 异常（本片最易做错的一处）

- `MyGuild == null`（无行会观者）：`return 221`，**不抛**。
- `MyGuild != null`：
  - `castle.IsMasterGuild(MyGuild)` 是**真实实现**（`UserCastle.cs:639-642`）。
  - 非城主 ⇒ `castle.IsAttackGuild(MyGuild)`；静态类型是 `IUserCastle`，
    接口签名 `bool IsAttackGuild(IGuild)` ⇒ 选中 `UserCastle.cs:1045-1048` 的
    `throw new NotImplementedException()`。
  - 城主但目标非同会/盟友 ⇒ `castle.IsAttackGuild(targetObject.MyGuild)`，同样抛。
- 异常沿 `Operate` 冒泡到 `PlayObject.Run` 的 `catch`（`PlayObject.Message.cs:392-400`）⇒
  **当轮 `while (GetMessage(...))` 剩余消息全部不再处理**（不是只丢一条名字帧）。
- 部署 `MasterGuild == null`，所以「城主」分支实际不可达；但 `else` 分支的抛异常是可达的
  （任何有行会玩家在攻城区看到别人名字时）。

> 结论：Rust 必须把「有行会观者在攻城区」建模为**确定性失败**（返回 `Err`/中止本 tick 消息排水），
> 而不是「回落到 255 继续发帧」。否则逐帧对比必红。

### 5.4 `GetShowName` 的行会/城堡名牌

`PlayObject.cs:3311-3327`（以下整段在 `if (MyGuild != null)` 内，`PlayObject.cs:3310`）：

| 条件 | 结果 |
| --- | --- |
| `IsCastleMember(this) != null` | `CastleGuildName`（部署值 `%guildname(%castlename)`）替换城堡名/会名；模板无 `%rankname` 占位 |
| 否则 `InCastleWarArea(this) != null` 且（`ShowGuildName` **或** `castle.UnderWar` **或** `InGuildWarArea`） | `NoCastleGuildName`（部署值 `%guildname`）；模板无 `%rankname` 占位 |
| 其余 | `sGuildName=""` |

部署 `ShowGuildName=0` ⇒ 平时不显示会名；但**战时/攻城区内仍显示**。
两个模板键在部署 `String.conf:223-224` 中**存在**，实值为
`CastleGuildName=%guildname(%castlename)`、`NoCastleGuildName=%guildname`
（均无 `[%rankname]`）；`MessageSettings` 的代码默认值仅作缺键回落，**不用于本部署的期望串**。

---

## 6. 攻城状态机规格

### 6.1 启动读序（`GameApp.cs`）

```text
:124 LoadMapInfo            // MapMgr 就绪（FindMap/GetMapInfo/GetMapOfServerIndex）
:131 LoadMonsterDB
:147 LoadMonGen
:155 InitializeMonster
:184 LoadMapQuest
:213 CastleMgr.Initialize   // ctor 读 Castle/Castle/SabukW.txt → Initialize() 造实体
                            // → LoadConfig 内 MasterGuild = FindGuild(OwnGuild)（行会表尚未装载）
                            // → ConfigDir="0" → EnvirList += 0151..0156 → Save()
:214 DataServer.Initialize
:227 LoadMerchant
:229 LoadNpcs
:233 LoadStartPoint
:244 WorldEngine.Initialize
:245 GuildMgr.LoadGuildInfo  // 行会表在此才装载（晚于 :213 的 MasterGuild 反查）
:246 CastleMgr.LoadCastleList// 读 Castle/List.txt，append 未初始化城
```

**读序结论（Minor 3 订正）**：`MasterGuild` 的反查发生在 `:213` 的 `CastleMgr.Initialize` 内，
早于 `:245` 的 `GuildMgr.LoadGuildInfo` ⇒ 启动时 `FindGuild` 必为 null；**`OwnGuild` 非空也不能让
`MasterGuild` 非 null**。部署 `OwnGuild=` 空只是第二重保证。Rust 若先装载行会再初始化城堡，
会凭空制造 oracle 不可达的城主态。

**列表形状**：`CastleList = [默认城(ConfigDir="0")] + L 座装载城`。
装载城 `MapName` 为 null（从未 `LoadConfig`）⇒ `Run` 在 `UserCastle.cs:318-321` 早退；
`MasterGuild` 为 null；`EnvirList` 为空。它们对 `InCastleWarArea`/`IsCastleMember` 无贡献，
但会让 `CastleMgr.Save()` 在 `UserCastle.SaveConfig` 的 `Archers[i]` 处 NRE
（`CastleConf.cs` 的 Archers 循环只在 `LoadConfig` 里创建对象）。

### 6.2 tick 门

| 调用 | 位置 | 周期 | 说明 |
| --- | --- | --- | --- |
| `CastleMgr.Run()` | `SystemProcessor.cs:70-77` | **10s**（`GetTickCount()-RunTimeTick>10000`），与 `GuildMgr.Run` 同一块、**Guild 先** | 城堡状态机 |
| `CastleMgr.Save()` | `SystemProcessor.cs:54-62` | `ConsoleShowUserCountTime`，部署 **600000ms（10min）** | 名单 + 每城配置 + 攻城申请 |

### 6.3 `UserCastle.Run` 状态转移表（每 10s 一跳）

| 行 | 条件 | 动作 | 事件 |
| --- | --- | --- | --- |
| 318-321 | `MapName` 空 | return | — |
| 323-326 | `ServerIndex != GetMapOfServerIndex(MapName)` | return | — |
| 328-339 | `date != IncomeToday`（日粒度） | `TodayIncome=0; IncomeToday=now; IsStartWar=false` | — |
| 340-343 | `!IsStartWar && !UnderWar && hour==StartCastlewarTime(20)` | `IsStartWar=true` | — |
| 346 | 同上 | `AttackGuildList.Clear()` | — |
| 347-362 | 同上，**倒序**遍历 `AttackWarList` | 日期==今天者：`UnderWar=true; ShowOverMsg=false; WarDate=now; StartCastleWarTick=now; AttackGuildList.Add(g); RemoveAt(i)` | — |
| 363-374 | 上述循环后 `UnderWar` | `AttackGuildList.Add(MasterGuild)`（null 时 `Add(null)`）→ `StartWallconquestWar` → `SaveAttackSabukWall` → `SS_212` → 广播开战 → `SS_204` → `MainDoorControl(true)` | `RefreshShowName`、文件、`SS_212`、广播、`SS_204`、门掩码/帧 |
| 377-390 | 每跳 | `Guards[i].BaseObject.Ghost` ⇒ null，然后 `Archers[i]` | — |
| 391-406 | `UnderWar` | 三墙 `StoneMode=false` | — |
| 408-418 | `UnderWar && !ShowOverMsg && now-Start > (CastleWarTime-ShowCastleWarEndMsgTime)` | `ShowOverMsg=true`；广播「离结束还有 N 分钟」 | 广播 + `SS_204` |
| 419-422 | `UnderWar && now-Start > CastleWarTime`（**严格大于**） | `StopWallconquestWar` | `UnderWar=false`、清列表、广播 + `SS_204` |
| 424-440 | `!UnderWar` | 三墙 `StoneMode=true` | — |

部署时限：`CastleWarTime=7200000`（2h）、`ShowCastleWarEndMsgTime=600000`（10min）、
`StartCastlewarTime=20`、`GetCastleTime=600000`、`StartCastleWarDays=2`。
⇒ 预告点 = 开战后 1h50m；结束点 = 开战后 2h。

### 6.4 开战入口与 N1

- 日常入口：`Run` 的 `AttackWarList`；该表只能由 `AddAttackerInfo(GuildInfo)` 写，
  而接口调用点选中 `AddAttackerInfo(IGuild)`（`:670-673`）⇒ 抛 N1 ⇒ **表恒空**。
- GM 入口：`@ForcedWallConQuestWar` 直接翻转 `UnderWar` 并广播/关门（不写 `AttackWarList`）。
- `@ChangeSabukLord`：`castle.GetCastle(guild)` 选中 `GetCastle(IGuild)`（`:644-647`）⇒ 抛 N1 ⇒
  改归属不可达。
- 自动占领（`PlayObject.Message.cs:319-338`）：`IsAttackGuild(IGuild)` 抛 N1 ⇒ 整链不可达。
  Rust 已用 `try_auto_occupy_n1`/`*_n1` 建模为 no-op（`castle.rs:1384-1510`）。
- **已登记偏差**：`AttackGuildList.Add(MasterGuild)` 在 `MasterGuild == null` 时会 `Add(null)`
  ⇒ oracle 的 `AttackGuildList.Count` 比 Rust 多 1（Rust 当前 `if let Some` 跳过）。
  唯一读 `Count` 的点是 `InPalaceGuildCount()`（`UserCastle.cs:604-607`），而它的调用点
  `PlayObject.Message.cs:331` 已被 N1 提前抛异常截断 ⇒ 当前不可观测。建议把列表元素改成
  `Option<GuildId>` 以精确对齐 `Count`，或在本卡显式登记该偏差。

### 6.5 文件格式与落盘怪癖

1. **`SaveConfig` 写 ctor 路径**：`CastleConf` 在 `UserCastle` ctor 里用
   `Path.Combine(BasePath, CastleDir, ConfigDir)` 构造（ConfigDir 当时 = "Castle"），
   `SaveConfig` 里的 `WriteString/WriteDateTime/Save` 都走这个 **ConfigFile 实例的文件名**；
   函数开头的 `filePath = .../userCastle.ConfigDir` 只用于 `Directory.CreateDirectory`。
   ⇒ 默认城的 `SaveConfig` 始终写 `Castle/Castle/SabukW.txt`（装载城 ctor 路径是 `Castle/<N>`，但它们 `Save` 前已 NRE）；`SaveAttackSabukWall` 用 `ConfigDir` ⇒ 写 `Castle/0/AttackSabukWall.txt`。
   部署 mtime 证据：`Castle/Castle/SabukW.txt` 与 `Castle/List.txt`、
   `Castle/0/AttackSabukWall.txt` 同为 `10-20 17:55`；`Castle/0/SabukW.txt` 仍为 `10-09`（未被运行期写）。
2. **`CastleMapList` 每次启动 +6 且带尾随逗号**：`LoadConfig` 只 append 不清空
   （`CastleConf.cs:24-37`），`CastleManager.Initialize` 又 `EnvirList.Add` 6 张图
   （`CastleManager.cs:70-75`），每次启动 `SaveConfig` 写出「上次全部 + 6」。
   写侧按 `图名 + ","` 逐项拼接 ⇒ 行尾**保留尾随逗号**（部署件即 `...,0156,`）。
   部署实测 **1314 项 = 6 × 219**（219 次启动），逐项为 0151..0156。
3. **`List.txt` 1↔2 行**：启动时 `SaveCastleList` 在装载前写 `0`（1 行）；装载后首次 10min Save
   写 `0\r\n1\r\n`（1+L 行）。部署当前 2 行（有服务在跑）。
4. **`Castle/1/SabukW.txt` 0 字节**：装载城 ctor 的 `ConfigFile` 建空文件。
5. **写侧条件与「键存在性」**：`TotalGold/TodayIncome` 为 0 时**不更新**缓存值；
   `MainDoorOpen/MainDoorHP` 仅 `BaseObject != null` 时更新；`Archers/Guards` 的 `HP`
   无论 `BaseObject` 是否为空都写（空写 0）。**注意：这不等于「键不出现」**——`LoadConfig`
   的每个 `ReadWrite*` 在缺键时会把默认值写入 `iniCahce`（`ConfigFile.cs:56-64`），
   `Save()` 再遍历 `iniCahce` 全量写回（`ConfigFile.cs:363-377`）⇒ 缺键会以默认值被写入，
   输出文件里该键仍存在。实现分两层：缓存键集合（读入 ∪ `ReadWrite*` 默认）+ 条件更新值。
6. **CRLF / GBK**：`StringList.SaveToFile` 用 `Environment.NewLine`（Windows CRLF）；
   `ConfigFile` 保持 GBK 编码。
7. **时间字段**：`ChangeDate/WarDate/IncomeToday` 由 `WriteDateTime` 写成固定格式
   `yyyy-MM-dd HH:mm:ss`（`ConfigFile.cs:449-460`）；读侧 `DateTime.Parse`（`:155-168`）。
   Rust 的 `CastleNow` 只有 `date+hour`，`war_date/income_today/attack_date` 只有日粒度
   ⇒ 落盘前必须补完整时间戳（或保存原始字符串），否则字节对不上；日粒度判定仍只用 `date`。

---

## 7. 写集切分与依赖

> 原则：**先世界态、后宿主排水**；同一文件一次只动一路。`enter.rs`/`main.rs` 进图段是热点，
> 与行会 G2/G3、PL-1/P32 有写集重叠，合并前必须 rebase 并重跑四门禁。

| WS | 文件 | 内容 | 依赖 | 冲突面 |
| --- | --- | --- | --- | --- |
| **WS-1 谓词/落点** | `crates/world/src/castle.rs` | `CastleDefenseSpecs` 增 `home_map/home_x/home_y/war_range_x/y/envir_list`；`UserCastle` 增 `palace_map_id/secret_map_id/castle_map_id/envir_map_ids`；`in_castle_war_area(map_id,x,y)`；`get_home_x/y(&mut rng)`；`relocate_on_enter`/`relocate_on_death` 纯函数（产 `Relocation`） | 无（纯 world） | `design-castle-entities` 已改 `initialize`，别回改 |
| **WS-2 宿主适配** | `bins/game-svr/src/castle.rs` | `specs_from_sabuk` 补字段；`map_owner_is_self` 改读 `MapRegistry::entries().server_index`（**登记已过期**：`entries()` 是 pub，见 `registry.rs:305`）；`CastleManager` 薄封装：`initialize`/`load_list`/`run`/`save` | WS-1；`crates/envir::castle` 读侧 | 无 |
| **WS-3 进图接线** | `bins/game-svr/src/main.rs` 进图段（`:16028-16170`）、`mapwire.rs` | 在 `resolve_enter_map` 后、`shift_enter_position` 前插入：`MyGuild` 物化 → `in_castle_war_area` → 两臂改写 → 重解析 `map_id` → 死亡三臂；RNG 走 `world.rng` | WS-1/2；`guild_cmd::materialize`；`World::set_actor_guild` | **高**：行会 G2/G3、PL-1/P32 |
| **WS-4 766** | `bins/game-svr/src/enter.rs`、`main.rs` 每 tick 玩家段 | `area_state(flag, in_guild_war_area)`；会话 `InGuildWarArea/NameColorChanged/DiscountForNightTick`；每 tick 城堡检查 + 1s 门 + `RefUserState`/`RefShowName` 排水 | WS-1；`smframe::sm_areastate`；`downlink` | **高**：enter/行会 |
| **WS-5 名字色/名牌** | `bins/game-svr/src/downlink.rs`、`enter.rs`（`show_name`）、`strings.rs` | `get_chr_color(viewer,target,...) -> Result<u8, NameColorN1>`（含 N1 中止）；`StringConf` 增 `CastleGuildName/NoCastleGuildName`；`show_name` 接会名/城堡名；`SM_USERNAME`/`SM_CHANGENAMECOLOR`/`RM_USERNAME` 接线 | WS-1；`GuildSession`；`World.is_ally_guild` | `downlink.rs` 与 AOI 片 |
| **WS-6 tick/落盘** | `main.rs` 10s 块（`:16940-16990`）、新增 `castle_runtime.rs` | 10s `CastleManager::run` + 事件排水；10min `Save`（`SaveCastleList` + `SaveConfig` + `SaveAttackSabukWall`） | WS-2；`World.clock_ms`；`envir::serialize_*` | 与 `guild_runtime::step` 同块，**Guild 先、Castle 后** |
| **WS-7 会话位** | `main.rs::SessionGround`、`guild_cmd.rs` | `my_guild` → `World.actor_guilds`；`InGuildWarArea` 等三个位 | 行会会话 | 同 WS-3/4 |

**非目标（本片不做，登记）**：
- 城堡税 / 成员价（#8）、城堡 NPC 动作（#6）、攻城实体受击路由（#5，已实现）。
- `SS_205/206/207/211/212` 跨服发送（单服无接收端）。
- 城堡 `RepairDoor/Wall` 的 NPC 入口（已实现纯函数，接线属 #6）。

---

## 8. 判据清单（每条带牙草案）

> 牙 = 把生产路径改坏后**指定断言必红**。实现时先写红、再逐字节还原并 `sha256sum -c`。
> 标注「源码序门」的判据用 `include_str!`/行号断言，防「函数写了但没被调用」。

### 8.1 构造级（用例本身不占窗口；标 R 者仍需第 9 节窗口）

| ID | tier | 断言（要点） | 牙（改坏 ⇒ 必红） |
| --- | --- | --- | --- |
| **C1** `castle_war_area_predicate` | L | 部署 `CastleHomeX/Y=644/290`、`CastleWarRangeX/Y=100/100`（`SabukW.txt:14-17`），oracle 严格 `<`（`UserCastle.cs:460`）：`(3,643,289)=true`；`(3,544,290)=false`（dx=100 边界）；`(3,543,290)=false`（dx=101）；`(3,743,290)=true`（dx=99）；`(3,744,290)=false`；`(3,644,389)=true`（dy=99）；`(3,644,390)=false`（dy=100）；`(0150)=true`（Palace）；`(D701)=true`（Secret）；`(0151)=true`（EnvirList）；未知图 false；`envir=None` false | `<` 改 `<=` ⇒ `(3,544,290)`/`(3,644,390)` 翻真；`&&` 改 `||` ⇒ 同上（另一轴为 0）；删 EnvirList 扫描 ⇒ 0151 红；删 Palace/Secret 分支 ⇒ 0150/D701 红；删 null 门 ⇒ panic |
| **C2** `castle_enter_nonmember_relocation` | **R** | 固定种子；(a) **自然可达**：非成员经 `3 -> 0150` 走入皇宫（`UnderWar=false` 也触发，因 `PalaceEnvir==envir`）⇒ `MapName=HomeMap`，X=`HomeX-2+r0`、Y=`HomeY-2+r1`，trace 恰 2 抽 `Random(5)`、顺序 X→Y；(b) 注入 `UnderWar=true` 的非成员在城堡图 `3` 的 (644,290) ⇒ 同一改写 | `Random(5)` 改 `Random(9)` ⇒ 坐标/流红；交换 X/Y ⇒ 固定种子下 r0≠r1 时红；把成员判定写成「有行会即成员」⇒ 红；外层条件写成只认 `UnderWar` ⇒ (a) 不改写红 |
| **C3** `castle_enter_member_palace_relocation` | **R** | `MasterGuild=Some` 的成员在 `0150`（`UnderWar=false` 也可）⇒ `MapName="3"`，X=`HomeX-4+r0`、Y=`HomeY-4+r1`；RNG trace 2 抽 `Random(9)`；随后移位环从该点继续抽 | 去掉一次 `Random(9)` ⇒ 流偏移红；`HomeX-4` 改 `-2` ⇒ 红；先 Y 后 X ⇒ 红 |
| **C4** `castle_death_relocation_arms` | **R** | `HP<=0` 三臂各一例：成员+UnderWar ⇒ 城堡 `HomeMap` + 2×`Random(9)`；非成员 ⇒ 本人 `HomeMap` + 2×`Random(5)`；红名 ⇒ `RedDieHomeMap` + 2×`Random(13)`；trace 计数与顺序 | 三臂顺序写反 ⇒ 红；红名支用 `Random(5)` ⇒ 红；`PvpLevel>=2` 判定写成 `>2` ⇒ 红 |
| **C5** `area_state_bit4_encoding` | L | `area_state(FIGHT,0,false)=1`；`(0,SAFE,false)=2`；`(0,0,true)=4`；`(FIGHT,SAFE,true)=7`；**同一 bit 重复置位幂等**：对已含 bit4 的 `n8` 再置 bit4 仍等于原值（实现应暴露 `set_flag(&mut n8, 4, cond)` 或等价累加器供此用例）；766 `Recog` 等于 n8，`Param/Tag/Series=0`、无尾 | 位常量 4 改 8 ⇒ 红；把累加器的 `|=` 改 `^=` ⇒ 「重复置位」用例红（`4` 变 `0`；单次 `1|2|4` 因位互不重叠仍为 `7`，不能靠它咬）；`|=` 改 `=` ⇒ `(FIGHT,SAFE,true)` 变 4 红；把 n8 写进 `Param` ⇒ 红 |
| **C6** `castle_name_color_vectors` | L | 无行会观者 + 双方 `InGuildWarArea` + `UnderWar` + 目标在攻城区 ⇒ **221**；有行会观者同一场景 ⇒ `Err(NameColorN1)` 且本 tick 后续消息不出队；`PvpLevel>=2` ⇒ 255（怪行为）；`PvpFlag` ⇒ 47；同会/盟友 ⇒ 180；宣战 ⇒ 69 | 「补全」城主/攻方色 ⇒ 红（必须抛）；221 改 255 ⇒ 红；红名改回 249 ⇒ 红 |
| **C7** `castle_show_name_war_branch` | L | 读部署 `String.conf:223-224`：`ShowGuildName=0`、`UnderWar=true`、有会 ⇒ 名牌 = `会名`（`NoCastleGuildName=%guildname`，无封号）；`IsCastleMember` ⇒ `会名(城堡名)`（`CastleGuildName=%guildname(%castlename)`）；平时 ⇒ 会名为空 | 删 `UnderWar` 门 ⇒ 战时不显示红；用 `MessageSettings` 默认模板（带 `[%rankname]`/前置城堡名）⇒ 串不等红 |
| **C8** `castle_ghost_reclaim` | L | `guards[i].ghost/archers[j].ghost` ⇒ 下一跳槽位 None；实体本身不被删；非 ghost 槽不变 | 只清 Archers ⇒ 红；删 reclaim ⇒ 红；清实体 ⇒ 实体数红 |
| **C9** `map_owner_self_from_entries` | L | 部署 MapInfo 的 `3` 号图 `server_index=0`、`ServerIndex=0` ⇒ 初始化继续；把注册表该项改成 1 ⇒ `initialize_castle` 0 实体 + 无告警（oracle 归属门早退） | 硬编码 `true` ⇒ 第二例红；读错字段 ⇒ 红 |

### 8.2 状态机/时序

| ID | tier | 断言 | 牙 |
| --- | --- | --- | --- |
| **C10** `castle_run_daily_war` | **R** | 注入 `CastleNow`：19:00 无动作；20:00 且 `AttackWarList` 有今日申请 ⇒ `UnderWar=true`、攻方列表 = 倒序申请者 + `MasterGuild`、申请被移除、事件序 `RefreshShowName → Save → SS_212 → Broadcast → SS_204 → MainDoorControl`；同日 21:00 不再重复；次日 00:00 `TodayIncome=0/IsStartWar=false`；20:00 无申请 ⇒ `IsStartWar=true` 但 `UnderWar=false` 且当日不再重试 | 正序遍历 ⇒ 列表序红；删 `IsStartWar` 闩 ⇒ 重复开战红；跨天不清 ⇒ 红；事件序换位 ⇒ 红 |
| **C11** `castle_run_timeout_and_stonemode` | **R** | `UnderWar` 起 T：`T+6_600_000`（2h-10min，**恰好**）仍无预告；`T+6_600_001` ⇒ 预告一次；`T+7_200_000`（恰好）⇒ **不**停战；`+1ms` ⇒ 停战（`UnderWar=false`、清列表、广播）；战时三墙 `StoneMode=false`，非战 `true` | `>` 改 `>=` ⇒ 边界红；预告重复 ⇒ 红；StoneMode 漏切 ⇒ 红 |
| **C12** `castle_run_10s_save_10min_gate` | **R** | 源码序：`CastleMgr::run` 与 `guild_runtime::step` 同在 10s 块、**Guild 先**；`CastleMgr::save` 在 600000 门；注入 tick 断言 10000ms **恰好**不跑、10001ms 跑、600000ms 恰好不存、600001ms 存（两处都是严格 `>`） | `run` 移出 10s 门 ⇒ 红；`save` 周期写错 ⇒ 红；Castle 先于 Guild ⇒ 红 |
| **C13** `castle_in_guild_war_area_setclear` | **R** | 每 tick 置位；离开后最多 1s 清除；`NameColorChanged` 只在变化时置；766 只在 1s 门内随变化发一次，且紧邻 `RefShowName`；登录 766 在「本 tick 城堡检查之后」装配 | 清除移出 1s 门 ⇒ 提前清红；置位移进 1s 门 ⇒ 迟到红；无变化也发 766 ⇒ 重复帧红；登录帧先于检查 ⇒ bit4 红 |

### 8.3 持久化 / 启动形状

| ID | tier | 断言 | 牙 |
| --- | --- | --- | --- |
| **C14** `castle_startup_list_shape` | **R** | 用部署件只读副本：`List.txt` 2 行 ⇒ `world.castles.len()=3`；装载城 `map_name=None/master_guild=None`；默认城 `EnvirList.len()=原 1314 + 6`；`save` 后 `Castle/Castle/SabukW.txt` 的 `CastleMapList` 项数 = 1320（**不 dedupe**） | dedupe ⇒ 1314 红；初始化装载城 ⇒ `Some` 红；少加 6 ⇒ 红；读错文件（`Castle/0`）⇒ 项数 6 红 |
| **C15** `castle_save_path_and_bytes` | **R** | `SaveConfig` 写 `Castle/Castle/SabukW.txt`（ctor 路径）；`SaveAttackSabukWall` 写 `Castle/<ConfigDir>/AttackSabukWall.txt`；`Castle/0/SabukW.txt` sha256 不变；CRLF + GBK；键序与 oracle 一致；**输入缺键 ⇒ 输出仍带该键且值为默认值**（`ReadWrite*` 入缓存 + `Save()` 全量重写） | 用 `ConfigDir` 拼 `SaveConfig` 路径 ⇒ `Castle/0/SabukW.txt` hash 红；用 `\n` ⇒ 字节红；删掉 `TotalGold` 非 0 的更新 ⇒ 值红（键仍在）；实现成「未显式 `WriteXxx` 就不输出该键」⇒ 缺键夹具红 |
| **C16** `attack_sabuk_wall_roundtrip` | **R** | 行格式 `会名 + 7 空格 + 引号日期`；未知行会丢弃；`TryParse` 失败 = Now；0 字节文件 ⇒ 空表。**日期串不是逐字节契约**：oracle `SaveAttackSabukWall` 写 `attackerInfo.AttackDate` 的 `DateTime.ToString()`（culture 相关，`UserCastle.cs:306`）；Rust 保留原串属**已登记偏差**（`crates/envir/src/castle.rs:41/461-496`），逐字节判据只覆盖结构部分（会名 + 7 空格 + 引号 + 原样日期串） | 7 改 6 空格 ⇒ 红；不丢未知行会 ⇒ 红；把日期原串参与逐字节比较 ⇒ 判据本身红（应移出）；结构分隔符写错 ⇒ 红 |
| **C17** `castle_enter_map_null_guard` | L | `envir=None`（`GetMapInfo` 未命中）⇒ 谓词 false、0 抽、不改写、不 panic | 删 null 门 ⇒ panic/多抽红 |

> 每个 R 项在实现卡里必须附「先红」证据（改坏生产路径的失败输出）与还原后的 `sha256sum -c`。
> L 项至少附 1 条抽样冒烟（构造级或进程级）。

---

## 9. 窗口项（R 级未过窗口不得 closed）

| 窗口项 | 内容 | 为什么必须窗口 |
| --- | --- | --- |
| **W-1 进图 RNG 对拍** | 固定种子，两侧各跑：**非成员经 `3 -> 0150` 走入皇宫（自然可达，`UnderWar=false`）** / 注入 `UnderWar` 的非成员进战区 / 成员进皇宫（注入 `MasterGuild`）/ 死亡三臂；逐 k 比 RNG trace 与最终 `CurrX/CurrY/MapName` | RNG 流水位置 + 进图时序；皇宫非成员臂无需改内容即可真机复现 |
| **W-2 766 动态序列** | GM `@ForcedWallConQuestWar`（或注入 `UnderWar`）后，真实进程观察进入/离开战区的 766 帧序、bit4 变化与 1s 节流 | 时序（1s/每 tick） |
| **W-3 姓名色/名牌真机** | 有行会/无行会观察者各一；确认有行会观察者触发 N1 时的**消息批次中断**与客户端可见结果 | 客户端可见 + 异常路径 |
| **W-4 文件字节** | 只读副本上跑启动 + 一次 Save，对比 `Castle/Castle/SabukW.txt`、`Castle/List.txt`、`Castle/0/AttackSabukWall.txt` 的 sha256（含 CRLF/GBK） | 持久化逐字节 |
| **W-5 长时状态机** | 2h 时限 + 10s tick：预告/结束/StoneMode 的绝对时刻 | 时序 |

> 窗口执行禁止写 `E:/MirServer`：把 `Castle/` 复制到 `E:/tmp/...` 只读副本，改 `BasePath` 指向副本。

---

## 10. 未决问题（需 owner/reviewer 裁决或后续片确认）

1. **N1 异常的建模粒度**：`GetChrColor` 抛异常会中止**当轮全部消息**。Rust 的消息排水是按 tick 批量；
   是建模为「`Err` ⇒ 本 tick 剩余消息丢弃」，还是只跳过该帧？两者客户端可见性不同。
   建议按 oracle 的批量语义（丢弃本 tick 剩余），并在判据里钉死。
2. **`Save()` 在未初始化城 NRE**：Rust 无异常语义。建议建模为「保存完已初始化城后停止，
   未初始化城无输出」；需确认是否需要复刻 `Castle/1/SabukW.txt` 的建空文件副作用。
3. **`SaveConfig` 写 ctor 路径**是否按怪行为照抄：建议**照抄**（硬约束③），并在实现卡登记。
4. **`EnvirList` 的 `OrdinalIgnoreCase`**：Rust `MapRegistry` 的索引与查找都先过
   `normalize_name = to_ascii_lowercase`（`crates/envir/src/registry.rs:248/364/388-390`），
   对 ASCII 图名等价 oracle 的 `OrdinalIgnoreCase`（`UserCastle.cs:469-474`），**不是精确匹配**。
   建议启动时把 `EnvirList` 归一成 `map_id` 集合；保留构造级用例（`d701` vs `D701`）钉住该等价性。
5. **红名 `GetChrColor` 返回 255**：与 `GetNameColor` 的 249/251 不一致，属 oracle 怪行为。
   请 reviewer 确认「照抄 255」而不是「修正为 249」。
6. **`GetMapInfo(ServerIndex, MapName)` 的 server_index 门**：Rust 进图目前只按 `id_of`。
   单服部署 `ServerIndex=0` 等价；多服需补门（本片登记，不实现）。
7. **登录 766 的 bit4 时序**：`Run` 的城堡检查在消息段之前，`RM_LOGON` 的 766 可能已带 bit4。
   需真机确认首帧是否如此（金标准场景不在战区，未覆盖）。
8. **`UnderWar` 测试驱动**：自然流程不可达（无城主/无攻击者；除 GM `@ForcedWallConQuestWar`），
   建议测试专用注入（世界态 setter），不实现 GM 命令、不改内容。**注意**：皇宫 `0150`
   的进图改写**不依赖** `UnderWar`，其非成员臂自然可达，不能一并归入「只能注入」。

---

## 11. 复算命令（只读）

```bash
# 谓词与落点
grep -n "InCastleWarArea\|GetHomeX\|GetHomeY" src/M2Server/Castle/UserCastle.cs
awk 'NR>=298 && NR<=350 {printf "%d\t%s\n", NR, $0}' src/GameSrv/Word/WorldServer.cs
# 766 置位/清除
awk 'NR>=286 && NR<=350 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.Message.cs
grep -n "RefUserState" src/M2Server/Player/PlayObject.Base.cs src/M2Server/Player/PlayObject.Message.cs
# 名字色
awk 'NR>=3585 && NR<=3657 {printf "%d\t%s\n", NR, $0}' src/M2Server/Player/PlayObject.cs
grep -n "NotImplementedException" src/M2Server/Castle/UserCastle.cs
# 状态机/门
grep -n "CastleMgr.Run\|CastleMgr.Save\|CastleMgr.Initialize\|CastleMgr.LoadCastleList" \
  src/GameSrv/GameApp.cs src/GameSrv/Word/Threads/SystemProcessor.cs
# 部署件（只读）
od -c E:/MirServer/M2GameSvr/Castle/List.txt
grep '^CastleMapList=' E:/MirServer/M2GameSvr/Castle/Castle/SabukW.txt | tr ',' '\n' | sed '/^$/d' | wc -l
ls -la --time-style=long-iso E:/MirServer/M2GameSvr/Castle/Castle E:/MirServer/M2GameSvr/Castle/0
# Rust 现状
grep -rn "castles\|initialize_castle" mir2-rs/bins/game-svr/src/main.rs
grep -n "pub fn in_castle_war_area\|pub fn get_home" mir2-rs/crates/world/src/castle.rs   # 无输出
```

---

## 12. 残留物清单

- 本文件（唯一改动）；无临时库、无临时目录、无进程、无端口占用。
- 只读访问：`E:/MirServer/M2GameSvr/Castle/**`（未写）；`src/**`、`mir2-rs/**`（未改）。
- 构建隔离：本车道未跑 cargo（纯文档），不产生 target 产物。
