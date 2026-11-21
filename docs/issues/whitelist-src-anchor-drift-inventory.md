# whitelist 冻结面（`src/**`）锚漂移底账（Phase 0 报告，2026-11-21）

> 工具：`mir2-rs/tests/parity/m3/whitelist_anchor_gate.py report-src-drift`（**只报告、不判红**；默认 `check` 行为不变）。
> 口径：A（声明级/span 匹配）+ B（资格过滤）；**检测器已按 `10` 的两轮抽样修正**（§1b/§1c）。
> 数据：`/UserTemp/qa-b2/src-anchor-drift-v3.{json,tsv}`。

## 1. 方法与口径（先读）

- **B 资格过滤**：带 `::`/`.` 限定、或文件内唯一、或 CamelCase≥2 词/长度≥6/全大写 ⇒ 参与；否则「符号不合格」只 WARN。
- **主判据 = span 命中**（锚 ∈ 声明方法体的 `{声明行 .. 配对右花括号}` ⇒ dist=0）；**距离只在方法外使用**。
- **上界口径**：通用名可能撞别的成员 ⇒ 「已漂出」仍是**上界**，逐条裁决后才可移锚。

## 1b. 检测器修正 · 第一轮（8 条「符号不存在」全部是配对错）

首版报告的「符号不存在 8」经 `10` 逐条复核，**一条都不是锚漂，全部是提取器配对错**。已修正：
① 排除 `*Exception`；`SM_*/RM_*/CM_*` 视为帧/消息；② 否定/对照引用不配符号（否定词须紧邻符号）；
③ 语言一致（候选符号必须出现在锚的目标文件里）；④ 同句多锚按「距离 + 语言」配对。

| 条目 | 首版误配 | `10` 裁决（该行实际/正确符号） | 修正后 |
| --- | --- | --- | --- |
| `B-108` | `IndexOutOfRangeException` | 泛名；实为 `ActionOfAutoGetExp` | 无相邻符号 |
| `T-118` | `NullReferenceException` | 泛名；实为 `SendMsg` | 松命中·非声明 |
| `T-47` | `SM_WEIGHTCHANGED` | 帧名；实为 `WeightChanged` | 无相邻符号 |
| `T-97` | `ReloadAllGuild` | 命令名；实为 `LoadGuildInfo` | 无相邻符号 |
| `T-47` | `WeightChanged` | 原文「**无** WeightChanged」⇒ 否定引用 | 无相邻符号 |
| `T-263` | `ClientGuildAlly` | 原文「**不进** ClientGuildAlly」，实调 `ClientGuildBreakAlly` | 否定句不配；`PlayObject.Operate` 处按同语言配 `ClientGuildAlly` |
| `T-142` | `config_game_gold_name`/`NpcPlayer.frames` | Rust 名配 `.cs` 锚（跨语言） | 无相邻符号 |
| `T-142` | 同句多锚 | 一句多条锚混排 | 按距离+语言重配 |

## 1c. 检测器修正 · 第二轮（`10` 抽样 6 条「行号漂出」也全是误判）

**判据主次调换**：原按「到声明行距离」判漂，把**方法体长度**当成了漂移量；改为 **span 命中为主**、距离只在方法外。
另加：span 用**配对花括号**（不看缩进；覆盖嵌套块 / 本地函数 / 属性 getter）；DB/SQL 标识符（表名/列名/`GetByte("…")` 参数）不参与代码符号匹配。
回归集：`T-294`/`D-13`/`T-148`/`T-53`/`T-266` 现均 **严格命中（dist=0）**；`T-29` 的 `characters_ablity.Level` 是 SQL 列名 ⇒ **无相邻符号**（不参与匹配）。
检测器自带合成夹具 + **反向牙**（锚在方法外 5 行 ⇒ dist>3，见 `report-selftest`）。

> **结论（按 opc 措辞）**：**检测器把「方法体内锚」误判为「漂移」**——锚指向方法体是常态，而按「到声明行距离」判漂，会把方法体长度当成漂移量。修正后以 span 命中为一等判据、距离只在方法外使用。
> **重算效果**：已漂出 **18 → 1**（唯一剩 `T-216`，dist=720，待逐条裁决）；「锚漂移危机」的绝大部分是报告器造成的。**先不下终结论**，等 `T-216` 溯源。

## 2. 分布（两轮修正后）

| 桶 | 数量 | 首版 | 说明 |
| --- | ---: | ---: | --- |
| 严格命中（span） | 89 | 102 | 锚在声明体内（dist=0） |
| 无相邻符号（含否定/跨语言/混排/SQL 过滤） | 368 | 345 | 本方案不覆盖 |
| 已漂出（疑似真漂） | 1 | 26 | 仅 `T-216`，需溯源四步 |
| 松命中·非声明 | 65 | 48 | 常量/调用点；Phase 1 需补「全仓唯一定义」 |
| 符号不合格(通用名) | 4 | 4 | `Dir`/`Run`/`Teleport`/`ushort` |
| ±3 边缘 | 1 | 3 | `CHECKNAMEDATELIST` |

## 3. 已漂出（1 条，待溯源四步）
- T-216 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4933` | `GetLineVariableText` | dist=720

## 4. 松命中·非声明（65 条）
- D-14 | `src/M2Server/Player/PlayObject.Message.cs:398/399` | `LogService.Error` | dist=0
- B-118 | `src/GameSrv/Word/WorldServer.cs:1131/1285` | `BonusPoint` | dist=0
- T-60 | `src/M2Server/M2Share.cs:1782/1797` | `M2Share.GetNoClearMonList` | dist=0
- T-60 | `src/Modules/SystemModule/SystemShare.cs:1694/1708` | `SystemShare.GetNoClearMonList` | dist=0
- T-66 | `src/M2Server/M2Share.cs:182` | `M2Share.GameTime` | dist=0
- T-65 | `src/M2Server/Player/PlayObject.Operate.cs:363/376` | `ClientOpenDoor` | dist=0
- (散文) | `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:210/213` | `REPAIRCASTLEDOOR` | dist=0
- T-29 | `src/GameSrv/Word/WorldServer.cs:711/713` | `WorldServer.SaveHumanRcd` | dist=0
- T-51 | `src/M2Server/Player/PlayCloneObject.cs:3/23` | `PlayCloneObject` | dist=0
- T-36 | `src/OpenMir2/Packets/ClientPackets/ClientConf.cs:20` | `ShowMail` | dist=0
- T-47 | `src/GameSrv/Npc/Merchant.cs:1261/1396` | `CastleMgr.IncRateGold` | dist=0
- T-54 | `src/GameSrv/Npc/GuildOfficial.cs:186/187` | `GoldChanged` | dist=0
- T-54 | `src/M2Server/Player/PlayObject.Base.cs:3351` | `IsGuildMaster` | dist=0
- T-52 | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:2670/2726` | `ConditionOfCheckGroupCount` | dist=0
- T-55 | `src/M2Server/Player/PlayObject.Operate.cs:806/843` | `SystemShare.FunctionNPC.GotoLable` | dist=0
- T-72 | `src/Modules/SystemModule/Conf/ServerConf.cs:114` | `SystemShare.Config.ItemNumber` | dist=0
- T-80 | `src/M2Server/Player/PlayObject.cs:3240` | `SendMsg` | dist=0
- T-81 | `src/M2Server/Actor/BaseObject.cs:2429/2447` | `KillFunc` | dist=0
- T-81 | `src/M2Server/Player/PlayObject.cs:1787/1798` | `killObject.DropUseItems` | dist=0
- T-94 | `src/M2Server/Player/PlayObject.cs:1199` | `GetStartPoint` | dist=0
- T-95 | `src/M2Server/Player/PlayObject.cs:4110/4172` | `CheckWeaponUpgrade` | dist=0
- T-95 | `src/M2Server/Player/PlayObject.cs:2874/2876` | `StatusChanged` | dist=0
- T-99 | `src/GameSrv/Word/WorldServer.cs:648/653` | `RunNotice` | dist=0
- T-118 | `src/GameSrv/Word/WorldServer.cs:755/764` | `SendMsg` | dist=0
- T-118 | `src/M2Server/Player/PlayObject.Chat.cs:222/226` | `LogService.Error` | dist=0
- T-127 | `src/GameSrv/Word/Threads/SystemProcessor.cs:54/62` | `CastleMgr.Save` | dist=0
- T-127 | `src/GameSrv/GameApp.cs:246` | `LoadCastleList` | dist=0
- T-131 | `src/GameSrv/GameApp.cs:245` | `GuildMgr.LoadGuildInfo` | dist=0
- T-132 | `src/Modules/SystemModule/Conf/ServerConf.cs:509/512` | `PKFlagNameColor` | dist=0
- T-137 | `src/M2Server/Npc/NormNpc.cs:68/73` | `GotoLable` | dist=0
- T-140 | `src/M2Server/Player/PlayObject.Operate.cs:114/128` | `UserSelect` | dist=0
- T-142 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3089/3092` | `GameGoldChanged` | dist=0
- T-144 | `src/M2Server/Monster/Monsters/ElfMonster.cs:72` | `MakeClone` | dist=0
- T-147 | `src/M2Server/Player/PlayObject.cs:2899` | `AddAbil.HitSpeed` | dist=0
- T-149 | `src/OpenMir2/Common/ConfigFile.cs:263` | `Encoding.GetEncoding` | dist=0
- T-171 | `src/GameSrv/Maps/MapManager.cs:10` | `Dictionary` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeAdminModeCommand.cs:16` | `BaseObject.AdminMode` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeObModeCommand.cs:22` | `BaseObject.ObMode` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeSuperManModeCommand.cs:17` | `BaseObject.SuperMan` | dist=0
- T-196 | `src/M2Server/Actor/BaseObject.Base.cs:320` | `SuperMan` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeSalveStatusCommand.cs:20` | `BaseObject.SlaveRelax` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeAttackModeCommand.cs:18` | `CharacterObject.AttatckMode` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeAttackModeCommand.cs:65` | `SendDefMessage` | dist=0
- T-203 | `src/M2Server/Player/PlayObject.Sale.cs:475/514` | `SellOffInTime` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Message.cs:257/262` | `DecLightItemDrugTime` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Socket.cs:17` | `nRecog` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Message.cs:2015/2017` | `SendDefMessage` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Attack.cs:225/229` | `DoDamageWeapon` | dist=0
- T-204 | `src/M2Server/Actor/BaseObject.Message.cs:399/402` | `SystemShare.Config.SendRefMsgRange` | dist=0
- T-205 | `src/Modules/SystemModule/MagicEvent/MapEvent.cs:62` | `GetNextIdentity` | dist=0
- T-205 | `src/M2Server/CellObjectMgr.cs:10/15` | `TryAdd` | dist=0
- T-205 | `src/M2Server/Maps/Envirnoment.cs:666` | `CellObjectMgr.Remove` | dist=0
- T-205 | `src/OpenMir2/Grobal2.cs:48` | `ET_FIRE` | dist=0
- (散文) | `src/M2Server/Player/PlayObject.Socket.cs:98` | `NetChannel.AddGateBuffer` | dist=0
- (散文) | `src/GameSrv/GameShare.cs:57/86` | `GetRandpmRoute` | dist=0
- T-202 | `src/M2Server/Player/PlayObject.Base.cs:897/907` | `ChangeSpaceMove` | dist=0
- T-206 | `src/Modules/SystemModule/Data/MapCellInfo.cs:30` | `ObjList` | dist=0
- T-222 | `src/M2Server/Player/PlayObject.cs:1023` | `playerActor.DynamicVarMap` | dist=0
- T-223 | `src/GameSrv/Word/WorldServer.cs:455` | `humData.Account` | dist=0
- T-220 | `src/Modules/ScriptEngine/ProcessingBase.cs:1829` | `SystemShare.GetEnvirFilePath` | dist=0
- T-218 | `src/M2Server/Player/PlayObject.Message.cs:1870/1874` | `SendDelItemList` | dist=0
- T-224 | `src/GameSrv/Word/WorldServer.cs:1738/1765` | `ShowPreFixMsg` | dist=0
- T-224 | `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:110/1279` | `EnableChatServer` | dist=0
- T-224 | `src/M2Server/Player/PlayObject.cs:1969/1998` | `PlayObject.Die` | dist=0
- T-266 | `src/M2Server/Player/PlayObject.Attack.cs:31` | `CheckWeaponUpgrade` | dist=0

## 5. ±3 边缘（1 条）
- T-220 | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:642` | `CHECKNAMEDATELIST` | dist=3

## 6. 符号不合格（4 条，B 过滤；只 WARN）
- T-31 | `src/M2Server/Monster/Monsters/CastleDoor.cs:99/104` | `Dir` | dist=None
- T-94 | `src/M2Server/Player/PlayObject.Message.cs:266` | `Run` | dist=None
- T-169 | `src/M2Server/Player/PlayObject.cs:2235/2237` | `Teleport` | dist=None
- T-206 | `src/M2Server/Actor/AnimalObject.cs:42` | `ushort` | dist=None

## 7. 无相邻符号（368 条，按文件分组）
- `src/M2Server/Player/PlayObject.Message.cs`（40）：D-9, D-9, D-10, D-13, A-8, (散文), (散文), T-26, T-28, T-28, T-29, T-57, T-76, T-79, T-80, T-82, T-100, T-101, T-134, T-135, T-142, T-142, T-142, T-147, T-148, T-148, T-203, (散文), T-206, T-206, T-207, T-207, T-207, T-207, T-207, T-214, T-224, T-224, T-270, T-292
- `src/M2Server/Player/PlayObject.cs`（33）：B-2, T-62, T-65, T-17, T-29, T-29, T-69, T-43, T-47, T-54, T-73, T-79, T-79, T-80, T-80, T-99, T-118, T-142, T-142, (散文), (散文), T-192, T-203, T-201, T-205, T-205, T-205, T-205, T-205, T-205, T-206, T-266, T-266
- `src/M2Server/Actor/BaseObject.cs`（20）：B-3, T-61, T-28, T-32, T-33, T-57, T-47, T-80, T-142, T-147, T-154, T-196, T-196, T-196, T-196, T-206, T-206, T-206, T-206, T-224
- `src/GameSrv/Word/WorldServer.cs`（18）：T-67, (散文), (散文), (散文), (散文), (散文), (散文), T-80, T-94, T-120, T-129, T-171, (散文), (散文), T-196, (散文), (散文), T-272
- `src/M2Server/Actor/BaseObject.Base.cs`（15）：B-115, B-115, T-31, T-32, T-49, T-77, T-154, T-154, T-154, T-154, T-177, T-196, T-196, T-196, T-224
- `src/M2Server/Player/PlayObject.Base.cs`（15）：B-112, B-113, B-113, B-118, T-61, T-63, T-66, T-20, T-25, T-28, T-43, T-142, T-142, T-142, T-211
- `src/M2Server/Player/PlayObject.Operate.cs`（12）：T-65, (散文), (散文), (散文), T-16, T-43, T-47, T-47, T-79, T-140, T-211, T-292
- `src/GameSrv/Npc/Merchant.cs`（10）：D-16, B-16, T-47, T-95, T-95, T-137, T-154, T-192, T-199, T-289
- `src/M2Server/Actor/BaseObject.Message.cs`（9）：D-13, T-80, T-83, T-118, T-192, T-196, T-199, T-206, T-224
- `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`（9）：T-23, T-23, T-29, B-16, T-101, T-144, T-154, T-199, T-204
- `src/GameSrv/DB/MySqlDB.cs`（7）：B-4, B-5, B-6, T-61, T-44, T-44, T-266
- `src/GameSrv/Word/WorldServer.MonGen.cs`（7）：B-1, T-79, T-95, T-155, T-206, T-206, T-269
- `src/Modules/ScriptEngine/ScriptParsers.cs`（7）：B-8, B-8, B-11, B-12, B-13, B-14, T-47
- `src/M2Server/Monster/MonsterObject.cs`（6）：B-114, B-114, T-49, T-143, T-143, T-196
- `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`（6）：B-108, T-79, T-142, T-142, T-220, T-224
- `src/Modules/SystemModule/Conf/ServerConf.cs`（6）：B-113, B-113, T-101, T-128, T-142, T-144
- `src/GameSrv/DB/LocalDB.cs`（5）：T-56, T-140, T-193, T-212, T-219
- `src/M2Server/Maps/Envirnoment.cs`（5）：B-15, T-65, (散文), (散文), T-29
- `src/M2Server/Npc/NormNpc.cs`（5）：T-31, T-154, T-155, T-192, T-224
- `src/M2Server/Player/PlayObject.Chat.cs`（5）：T-113, T-210, T-210, T-212, T-216
- `src/M2Server/Magic/MagicManager.cs`（4）：T-285, T-285, T-286, T-286
- `src/Modules/GameCommand/CommandHelp.cs`（4）：T-212, T-216, T-217, T-268
- `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs`（4）：B-107, (散文), T-37, T-223
- `src/BotSrv/Player/RobotPlayer.Operate.cs`（3）：(散文), T-29, T-199
- `src/GameGate/Services/ClientSession.cs`（3）：A-1, A-2, T-206
- `src/GameSrv/GameApp.cs`（3）：D-21, (散文), (散文)
- `src/GameSrv/Npc/CastleOfficial.cs`（3）：(散文), T-40, T-142
- `src/M2Server/Castle/CastleConf.cs`（3）：T-127, T-152, T-152
- `src/M2Server/M2Share.cs`（3）：T-60, T-64, T-42
- `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs`（3）：T-79, T-143, T-143
- `src/M2Server/Player/PlayObject.Attack.cs`（3）：T-266, T-266, T-269
- `src/M2Server/Player/PlayObject.Socket.cs`（3）：T-100, T-142, T-202
- `src/Modules/SystemModule/Data/MapItem.cs`（3）：T-19, T-25, T-32
- `src/OpenMir2/HUtil32.cs`（3）：B-9, B-10, (散文)
- `src/OpenMir2/Messages.cs`（3）：D-15, T-44, T-196
- `src/GameSrv/Npc/GuildOfficial.cs`（2）：(散文), T-138
- `src/LoginGate/Services/ClientSession.cs`（2）：A-3, A-4
- `src/M2Server/Actor/AnimalObject.cs`（2）：T-31, T-96
- `src/M2Server/Castle/UserCastle.cs`（2）：T-38, T-153
- `src/M2Server/Items/GameItemSystem.cs`（2）：T-95, T-266
- `src/M2Server/Monster/Monsters/CastleDoor.cs`（2）：B-15, (散文)
- `src/M2Server/Monster/Monsters/PetSuperGuard.cs`（2）：T-155, T-155
- `src/M2Server/Monster/Monsters/SuperGuard.cs`（2）：T-155, T-155
- `src/Modules/GameCommand/Commands/ChangeObModeCommand.cs`（2）：T-196, T-196
- `src/Modules/GameCommand/Commands/ForcedWallconquestWarCommand.cs`（2）：(散文), (散文)
- `src/Modules/GameCommand/Commands/ShowEffectCommand.cs`（2）：T-215, T-215
- `src/Modules/GameCommand/Commands/StartQuestCommand.cs`（2）：T-219, T-219
- `src/Modules/SystemModule/ActorManager.cs`（2）：T-137, T-140
- `src/Modules/SystemModule/SystemShare.cs`（2）：T-63, T-64
- `src/OpenMir2/Common/StringList.cs`（2）：T-150, T-153
- `src/OpenMir2/Grobal2.cs`（2）：B-115, B-115
- `src/OpenMir2/RandomNumber.cs`（2）：B-7, T-269
- `src/BotSrv/Maps/TMap.cs`（1）：T-10
- `src/BotSrv/Player/RobotPlayer.cs`（1）：T-206
- `src/GameSrv/AppService.cs`（1）：(散文)
- `src/GameSrv/Maps/MapManager.cs`（1）：(散文)
- `src/GameSrv/Word/Threads/SystemProcessor.cs`（1）：T-127
- `src/GameSrv/Word/Threads/UserProcessor.cs`（1）：T-112
- `src/GameSrv/Word/WorldServer.Merchant.cs`（1）：T-64
- `src/LoginSrv/Services/ClientSession.cs`（1）：A-4
- `src/LoginSrv/Storage/AccountStorage.cs`（1）：A-7
- `src/M2Server/Actor/ActorEntity.cs`（1）：T-207
- `src/M2Server/Actor/BaseObject.ViewRange.cs`（1）：D-8
- `src/M2Server/Castle/CastleManager.cs`（1）：T-127
- `src/M2Server/Guild/GuildInfo.cs`（1）：T-39
- `src/M2Server/Guild/GuildManager.cs`（1）：T-97
- `src/M2Server/Monster/Monsters/ArcherGuard.cs`（1）：T-155
- `src/M2Server/Monster/Monsters/BoneKingMonster.cs`（1）：T-143
- `src/M2Server/Monster/Monsters/ElfMonster.cs`（1）：T-143
- `src/M2Server/Monster/Monsters/GuardUnit.cs`（1）：T-96
- `src/M2Server/Net/ChannelMessageHandler.cs`（1）：T-100
- `src/M2Server/Player/CharacterObject.cs`（1）：T-196
- `src/Modules/GameCommand/CommandAttribute.cs`（1）：T-212
- `src/Modules/GameCommand/Commands/AuthCancelCommand.cs`（1）：T-263
- `src/Modules/GameCommand/Commands/AuthCommand.cs`（1）：T-263
- `src/Modules/GameCommand/Commands/ChangeLevelCommand.cs`（1）：T-268
- `src/Modules/GameCommand/Commands/ClearBagItemCommand.cs`（1）：T-218
- `src/Modules/GameCommand/Commands/DelSkillCommand.cs`（1）：T-291
- `src/Modules/GameCommand/Commands/GroupRecallCommand.cs`（1）：(散文)
- `src/Modules/GameCommand/Commands/KickHumanCommand.cs`（1）：T-214
- `src/Modules/GameCommand/Commands/MobFireBurnCommand.cs`（1）：T-216
- `src/Modules/GameCommand/Commands/ReconnectionCommand.cs`（1）：(散文)
- `src/Modules/GameCommand/Commands/SbkDoorControlCommand.cs`（1）：(散文)
- `src/Modules/GameCommand/Commands/ShutupCommand.cs`（1）：T-209
- `src/Modules/GameCommand/Commands/ShutupReleaseCommand.cs`（1）：T-212
- `src/Modules/GameCommand/Commands/SmakeItemCommand.cs`（1）：T-217
- `src/Modules/GameCommand/GameCommand.cs`（1）：T-210
- `src/Modules/MarketSystem/Services/MarketService.cs`（1）：T-36
- `src/Modules/ScriptEngine/Consts/ConditionCode.cs`（1）：(散文)
- `src/Modules/ScriptEngine/ProcessingBase.cs`（1）：(散文)
- `src/Modules/SystemModule/Conf/GameSettingConf.cs`（1）：T-36
- `src/Modules/SystemModule/Conf/StringConf.cs`（1）：T-116
- `src/Modules/SystemModule/MessageSettings.cs`（1）：T-191
- `src/OpenMir2/Common/ConfigFile.cs`（1）：T-153
- `src/OpenMir2/Enums/PlayerJob.cs`（1）：T-52
- `src/SelGate/Conf/ConfigManager.cs`（1）：T-172
- `src/SelGate/GateShare.cs`（1）：T-172
- `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs`（1）：T-29

## 8. Phase 1/2 入口（**暂停**）
- Phase 1：只对 6 道场景锚门覆盖文件集开启严口径判红；**错误码 `SRC_DECL_DRIFT`**（与场景门 `ANCHOR_DRIFT` 区分），标注「本层=声明级」。
- Phase 2：**在底账被修正/裁决前不开启**；每条「已漂」先走**溯源四步**（锚基线 → 漂移提交 → 是否登记/正当 → 无二次漂移），确认新状态正确再移锚，不自动修。
- 常量类需补「全仓唯一定义」规则，否则恒落「松命中·非声明」。

## 9. 复现
```text
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py report-src-drift --tsv <out.tsv> --json <out.json>
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py report-selftest   # 合成夹具 + 反向牙
```
