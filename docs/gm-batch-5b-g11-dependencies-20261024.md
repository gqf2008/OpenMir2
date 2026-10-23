# GM 第 5 批 B 片（跨服/行会落库族）的 **G11 依赖清单**

> 卡：`openmir2-gm-batch-5b-cross-server`（tier=R，**blocked-by G11 跨服基础设施**）
> 分支 `card/gm-batch-5b-cross-server`，base `333b8cfde`。
> 写集：`docs/`。**本卡只交清单**：不写 `gmcmd.rs`、不动 `whitelist.md`/`gm_registry.json`、不在宿主里造假的跨服通道。
> 依据：父卡裁定 `9e7d2227`（A/B/C 拆分）、A 片交付 `docs/gm-batch-5a-guild-20261023.md`、oracle `src/**` 只读核对。

## 0. 状态与结论

- **本卡保持 blocked**：G11（跨服/位面子系统：`PlanesServer`/`PlanesClient` 传输 + 主/分服拓扑 + 跨服状态同步）未就绪。
- 已交付：B 片 **6 条**的 oracle 出处、期望帧、**逐条 G11 依赖点**、落库面、两个怪癖的照抄口径。
- **不在本卡**：C 片（`StartContest`/`EndContest`/`ContestPoint`）部署 `GMEXECUTE` 0 命中 ⇒ 已登记「不实现」（父卡裁定 `9e7d2227` c_piece）。
- **不登记为「已实现」**：`gm_registry.json` 里 6 条的 handler 记录保持现状；`whitelist.md` 不动（G11 就绪、逐条 1:1 移植后再登记 `T-*`）。

## 1. Rust 侧现状（这些**不是**从零补）

| 已有物 | 位置 | 与 B 片的关系 |
|---|---|---|
| `GuildManager`（`AddGuild`/`DelGuild`/`FindGuild`/`MemberOfGuild`/`LoadGuildInfo`/`Run`） | `mir2-rs/crates/world/src/guild.rs:23`（oracle `GuildManager.cs:19-167`） | ①③⑥ 的行会可写态基础 |
| `UserCastle::get_castle_n1` | `mir2-rs/crates/world/src/castle.rs:29`（oracle `UserCastle.cs:646`，`ChangeSabukLordCommand.cs:39`） | ⑤ 城堡归属改写 |
| `GuildServerEvent`（`SS_206`/`SS_207` 只落事件、不产客户端帧） | `mir2-rs/bins/game-svr/src/guild_cmd.rs:46`（注释段） | 跨服消息在**单区**的既有替身 |
| `SwitchDataState`/`process_switch_data` 脚手架（生产恒 no-op） | `mir2-rs/bins/game-svr/src/lib.rs:14731/14766` | G11 的搬家面已有壳（见 `docs/orphan-encoder-trigger-audit-20261023.md:36`） |
| `gm_registry` 6 条 handler 记录（perm=10） | `mir2-rs/tests/parity/coverage/gm_registry.json`（`ShowSbkGold` @3064、`AddGuild` @3526、`DelGuild` @3568…） | 表项已在；**命令体未实现** |

缺的只有两件：**① 服务组消息的发送/接收语义（G11）**；**② 城堡战状态与行会态在跨服视图下的可写/同步口径**。

## 2. G11 必须提供的接口面（先定这个，6 条才能动手）

### 2.1 发送口：`WorldEngine.SendServerGroupMsg(code, serverIdx, body)`

oracle `src/GameSrv/Word/WorldServer.cs:885-896`：

```csharp
if (M2Share.ServerIndex == 0) PlanesServer.Instance.SendServerSocket(nCode + "/" + nServerIdx + "/" + sMsg);
else                          PlanesClient.Instance.SendSocket(nCode + "/" + nServerIdx + "/" + sMsg);
```

⇒ 线格式是**明文三段** `code/serverIdx/body`，**不是** SM 二进制帧；单区（`srvArray` 空、无对端）时是纯旁路。

### 2.2 接收端语义（G11 必须**明确选择**：1:1 复刻空实现，还是建模）

`src/Modules/PlanesSystem/PlanesMessage.cs:15-129` 的 `ProcessData` 分派，B 片相关的 6 个码：

| 码 | 别名 | 接收实现 | 现状 |
|---|---|---|---|
| 204 | `SS_204` = `ISM_SYSOPMSG` | `MsgGetSysopMsg`（`:226-229`） | **整段注释**（`SendBroadCastMsg` 被注掉）⇒ 空操作 |
| 205 | `SS_205` = `ISM_ADDGUILD` | `MsgGetAddGuild`（`:231-236`） | 解析后 `M2Share.GuildMgr.AddGuild` **被注释** ⇒ 空操作 |
| 206 | `SS_206` = `ISM_DELGUILD` | `MsgGetDelGuild`（`:238-242`） | 同上，**被注释** |
| 207 | `SS_207` = `ISM_RELOADGUILD` | `MsgGetReloadGuild`（`:244+`） | **整段注释** |
| 211 | `SS_211` = `ISM_CHANGECASTLEOWNER` | `MsgGetChangeCastleOwner`（`:354-357`） | **直接 `throw new Exception("TODO MsgGetChangeCastleOwner...")`** |
| 212 | `SS_212` = `ISM_RELOADCASTLEINFO` | `MsgGetReloadCastleAttackers`（`:359-362`） | `//M2Share.CastleMgr.Initialize();` **注释** |

**结论（决定实现口径的关键）**：oracle 的跨服**接收**臂在单区下全部是空实现（211 是 throw，但单区收不到）。
⇒ G11 就绪后 B 片的 1:1 口径应是：**发送调用确实发生**（进入 server-group 通道、可观测/可断言），**接收端保持空实现**；
任何"接收后重载行会/城堡"的建模都会**偏离 oracle**，必须单开 `D-*`/`T-*` 登记。

### 2.3 行会可写态

- 主服守卫：`AddGuildCommand.cs:23` / `DelGuildCommand.cs:22` 都是 `if (SystemShare.ServerIndex != 0) { SysMsg("只能…主服务器…"); return; }`。
- 落盘只在主服：`GuildManager.SaveGuildList()`（`GuildManager.cs:124-141`）里有 `if (M2Share.ServerIndex != 0) return;`，写 `SystemShare.Config.GuildFile`。
- G11 需要提供：**ServerIndex 的会话/进程可见性**（谁是主服），以及分服上同样的"拒绝执行"分支可达。

### 2.4 城堡可写态

- `CastleMgr.Find(name)`、`castle.GetCastle(guild)`（`UserCastle.cs:539/644`）、`UnderWar`/`WarDate`/`StartCastleWarTick`、
  `StartWallconquestWar()`/`StopWallconquestWar()`、`MainDoorControl(bool boClose)`（注意 **true = 关**）、`TotalGold`。
- 落盘：`CastleManager.Save()`（`CastleManager.cs:150`）→ `UserCastle.Save()`（`UserCastle.cs:448`）→ `SaveConfigFile()` + `SaveAttackSabukWall()`；由 `SystemProcessor.cs:61` 周期调用。
- G11 需要提供：**开战中状态在分服的可见性**（oracle 靠 `SS_212` 触发 `CastleMgr.Initialize`，但接收端被注释 ⇒ 1:1 复刻 = 不重载）。

## 3. B 片 6 条逐条表

> 权限：6 条都是 `[Command(..., 10)]`（`gm_registry.json` 的 `perm_source = conf[Permission].<名>`）。
> 「期望帧」只列**客户端可见**帧；`SS_*` 是 server-group 明文，**不是** SM 帧。

| # | 命令 | oracle（`src/**` file:line） | 期望客户端帧 | 落库/落盘面 | **对 G11 的具体依赖点** |
|---|---|---|---|---|---|
| ① | `AddGuild` | `AddGuildCommand.cs:11`（属性）/ `:15-57` 主体：`:23` 主服守卫、`:34` `GetPlayObject`、`:42` `GuildMgr.AddGuild`、**`:44` `SendServerGroupMsg(SS_205, ServerIndex, gname+'/'+chief)`**、`:45` 绿字 | `SM_SYSMESSAGE(100)`：`这个命令只能使用在主服务器上`（非主服）/ `行会名称: X 掌门人: Y`（成功）/ `CommandHelp`（缺参）/ `NowNotOnLineOrOnOtherServer`（找不到掌门人） | 主服 `GuildBase/GuildList.txt`（`[Share] GuildFile`，`GameSvrConf.cs:1290`/`ServerConf.cs:70`）+ `GuildBase/<行会>.txt`（`GuildInfo.cs:366-375`）；**无 MySQL 表** | ① `SendServerGroupMsg(205)` 通道（2.1）② 主服判定 `ServerIndex`（2.3）③ 行会可写态 `GuildMgr.AddGuild`（Rust 已有）④ 接收端 205 空实现（2.2） |
| ② | `DelGuild` | `DelGuildCommand.cs:11` / `:15-40`：`:22` 主服守卫、`:32` `GuildMgr.DelGuild`、**`:34` `SendServerGroupMsg(SS_206, ServerIndex, gname)`** | `SM_SYSMESSAGE(100)`：`只能在主服务器上才可以使用此命令删除行会!!!` / `没找到X这个行会!!!` | `DelGuild` 前置 `BackupGuildFile()` + `SaveGuildList()`（`GuildManager.cs:34-52`）⇒ `GuildBase/GuildList.txt` 与备份；**无 MySQL 表** | 同 ①，码换 206；另需 `RankList.Count > 1` 的拒删行为（`GuildManager.cs:41-44`） |
| ③ | `EndGuild` | `EndGuildCommand.cs:11` / `:14-39`：`:19` `GuildRankNo > 1` 门、`:21` `IsMember`+`DelMember`、**`:23` `SendServerGroupMsg(SS_207, ServerIndex, guildName)`**、`:24-27` 会话态清写与两帧 | `SM_SYSMESSAGE(100)`：`行会掌门人不能这样退出行会!!!` / `你都没加入行会!!!` / `你已经退出行会。`；`SM_CHANGEGUILDNAME(750)`（`RefRankInfo(0,"")`）；`SM_USERNAME(42)`（`RefShowName`） | `DelMember` → `GuildInfo.SaveGuildInfoFile()`（`GuildInfo.cs:480/588`）⇒ `GuildBase/<行会>.txt`；**无 MySQL 表** | ① `SendServerGroupMsg(207)` ② 玩家会话行会字段可写（`MyGuild`/`GuildRankNo`/`GuildRankName`，Rust `GuildSession` 已有骨架）③ 帧序：750 → 42 → 100（`guild_cmd.rs` 已记录同口径）④ 接收端 207 空实现 |
| ④ | `ForcedWallconquestWar` | `ForcedWallconquestWarCommand.cs:11` / `:15-52`：`:30` `UnderWar = !UnderWar`、`:33-36` `ShowOverMsg=false`/`WarDate=Now`/`StartCastleWarTick`/`StartWallconquestWar()`、**`:37` `SS_212`**、`:39` `SendBroadCastMsg("[X 攻城战已经开始]", System)`、**`:40` `SS_204`**、`:41` `MainDoorControl(true)`（=**关**主门）；`:45` 停战支 `StopWallconquestWar()` | `SM_SYSMESSAGE(100)` 全服广播（`:39`）；主门被关：`SM_DIGDOWN(21)`（`CastleDoor.Close` → `RM_DIGDOWN` → `PlayObject.Message.cs:1902`）；`城堡名` 不存在 ⇒ 100 红字；停战支穿 `StopWallconquestWar` 的 `ChangePkStatus(false)`（`UserCastle.cs:587`） | 城堡 `Castle/<n>/SabukW.txt` 的 `[Setup]`（`UnderWar`/`WarDate`）随 `UserCastle.Save()`（`:448`）落盘；**无 MySQL 表** | ① `SS_212` + `SS_204` 通道 ② 城堡战状态可写（`UnderWar`/`WarDate`/`StartCastleWarTick`）③ 城门 actor 可操作（`MainDoorControl(true)`=关）④ 全服广播面（`SendBroadCastMsg` → 100）⑤ 接收端 212 空实现（**不重载城堡**） |
| ⑤ | `ChangeSabukLord` | `ChangeSabukLordCommand.cs:11` / `:15-51`：**`:23` `bool boFlag = @params.Length > 2 && bool.Parse(@params[2]);`**、`:29` `CastleMgr.Find`、`:35` `FindGuild`、`:39` `castle.GetCastle(guild)`、**`:42` `SS_211`**（仅 `boFlag`）、`:44` 绿字 | `SM_SYSMESSAGE(100)`：`X 所属行会已经更改为 Y`（成功）/ `行会 Y还没建立!!!` / 城堡不存在红字 | `GetCastle` 改写城堡 `OwnGuild`（`UserCastle.cs:539/644`）⇒ `Castle/<n>/SabukW.txt` 的 `[Setup] OwnGuild`（随 `Save()`） | ① `SS_211` 通道 ② 城堡归属可写 ③ 接收端 211：oracle 是 **throw**，单区不可达 ⇒ G11 必须保证单区不触发该分支（或 1:1 保留 throw 且不加调用者） |
| ⑥ | `ShowSbkGold` | `ShowSbkGoldCommand.cs:11` / `:15-68`：`:24` `?` 前缀拒参、`:31-37` 无城堡名 ⇒ `CastleMgr.GetCastleGoldInfo` 逐条绿字、`:45-51` 参数校验（`=-+`、`0..1e8`）、`:52-63` 改 `castle.TotalGold`、`:64-67` 负数归零 | `SM_SYSMESSAGE(100)`：`CommandHelp`/`参数错误`/城堡不存在；成功 = **仅绿字列表**，无其它帧 | `TotalGold` 只在内存，随 `UserCastle.Save()` → `SaveConfigFile` 落 `SabukW.txt`；**无 MySQL 表、无 SS 消息** | **不发任何 SS**：唯一依赖是城堡可写态（`TotalGold` + `GetCastleGoldInfo`）与城堡周期保存 —— G11 只需保证"城堡不是只读快照" |

> **没有一条写 MySQL**：这 6 条的"落库面"全是 **Envir 文件**（`GuildBase/*` 与 `Castle/<n>/SabukW.txt`）。父卡/卡面写"落库"时按文件口径理解，别去找 `guild` 表。

## 4. 两个怪癖的照抄口径（父卡点名）

### 4.1 `ChangeSabukLord` 的 `bool.Parse(@params[2])` —— **父卡措辞需订正**

父卡写「缺参抛 `IndexOutOfRange`」。实测 oracle（`ChangeSabukLordCommand.cs:23`）：

```csharp
bool boFlag = @params.Length > 2 && bool.Parse(@params[2]);
```

- **`@params.Length > 2` 是短路守卫** ⇒ 参数不足 3 个时**不会** `IndexOutOfRange`，`boFlag=false`。
- 真正会抛的是 **`FormatException`**：`GMEXECUTE` 的动作把参数拼成**固定 6 段**（`ActionOfGmExecute`，`ExecutionProcessingSys.cs:4925-4947`：`"@%s %s %s %s %s %s"`）⇒ `parameters.Split(' ')` 长度恒 6，
  脚本没给第三段时 `@params[2]` 是**空串** ⇒ `bool.Parse("")` 抛 `FormatException`；
  同理脚本写 `Gmexecute ChangeSabukLord 沙巴克 行会 abc` ⇒ `bool.Parse("abc")` 抛。
- 异常被**外层吞掉**：`PlayObject.ProcessUserLineMsg`（`src/M2Server/Player/PlayObject.Chat.cs:229`）里 `CommandSystem.Execute(this, sData)`（`:368`）位于 `try`（`:233`）之内，`catch (Exception e)`（`:402`）只记日志；`ActionOfGmExecute` 本身只有 `try/finally`（`ExecutionProcessingSys.cs:4939-4949`，**无 catch**）。
- **照抄口径**：① 保留 `Length > 2 &&` 短路（2 参时 `boFlag=false`，不抛）；② 第三段非 `true`/`false`（含空串）⇒ 抛 `FormatException` 并被 GM 入口吞（**不产任何帧、不改状态**）；③ 不要"顺手修成 `TryParse`"。

### 4.2 `GuildRecall` 的 `(short)dwValue` 强转

`GuildRecallCommand.cs:40-53`（**不在 B 片范围，照抄备查**）：

```csharp
int dwValue = (HUtil32.GetTickCount() - PlayerActor.GroupRcallTick) / 1000;   // :40
...
if (PlayerActor.GroupRcallTime > dwValue) PlayerActor.GroupRcallTime -= (short)dwValue;  // :46-48
else PlayerActor.GroupRcallTime = 0;                                                     // :52
```

`dwValue` 是 `int`，`(short)dwValue` 截断：> 32767 秒（≈9.1h 挂机）会溢出成负数 ⇒ `GroupRcallTime` 反而**变大**。
照抄即可（这是 oracle 的既有口径），**不要**改成 `int`，也不要加范围保护。

## 5. 明确不做

- **C 片**：`StartContest` / `EndContest` / `ContestPoint` —— 部署 `GMEXECUTE` **0 命中**，父卡 `9e7d2227` 已裁定「已登记不实现」，**不在本卡**。
- **假跨服通道**：不允许在宿主里造一个"看起来像跨服"的本地队列来让帧数/状态断言变绿；2.2 的空实现是**要照抄的现实**，不是偷懒。
- 不改 `src/**`、不起栈、不连库。

## 6. G11 就绪后的验收清单（给下一个 worker）

1. 逐条对拍：权限不足 ⇒ 恰好一条 `100`（`权限不够!!!`）且零其它帧（6 条一致）。
2. 权限满足：按 §3 的帧序断言（③ 的 750→42→100；④ 的 100 广播 + `21` 关门；⑤ 的 100 绿字）。
3. **server-group 可观测**：① ② ③ ④ ⑤ 必须各自产生**一次**对应码（205/206/207/212+204/211）的发送记录；⑥ **零次**。
   建议落点：现有 `GuildServerEvent`（`guild_cmd.rs`）扩一个 `SendServerGroup` 变体，或单独的 `server_group` 观测 channel。
4. 落盘断言：行会 `GuildBase/GuildList.txt` 与 `Castle/<n>/SabukW.txt` 的**行级 diff**（主服才写；分服分支必须"拒绝执行"）。
5. 两个怪癖的牙：⑤ 第三段写 `abc` ⇒ **零帧、零状态变化、不 panic**（异常被吞）；④ `MainDoorControl(true)` 必须是**关**门方向。
6. 登记：实现后在 `whitelist.md` 为"跨服接收端空实现"登记 `T-*`（理由：oracle `PlanesMessage` 各分支注释/throw），并在 `gm_registry.json` 更新状态；**本卡不做这两步**。

## 7. 未核 / 留给 G11 的问题

- `SS_*` 的实际线上编解码（`PlanesServer.DecodeSocStr` 的 `(...)` 包裹与 `\` 分隔，`PlanesServer.cs:63-90`）在本仓 Rust 侧**没有对应实现** —— G11 若只做"发送可观测"，需要明确是否也要复刻接收端解析。
- 单区部署（`ServerIndex=0`）下 `PlanesServer` 是否真的启动（`StartPlanesServer` 的调用点）——本卡未核；若生产不启动，则"发送"在现代码里等价于**写到无人接收的 socket**，这一点必须写进 G11 的验收口径。
