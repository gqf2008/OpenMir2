# whitelist 冻结面（`src/**`）锚漂移底账（Phase 0 报告，2026-11-21）

> 工具：`mir2-rs/tests/parity/m3/whitelist_anchor_gate.py report-src-drift`（**只报告、不判红**；默认 `check` 行为不变）。
> 口径：方案 **A（声明级窗口匹配）+ B（资格过滤）**；时间盒按 opc 要求——**本报告在半个工作日内产出，未超时**。
> 数据：`/UserTemp/qa-b2/src-anchor-drift.{json,tsv}`（本卡为底账；后续裁决以此为准，不依赖一次运行输出）。

## 1. 方法与口径（先读）

- **B 资格过滤**：带 `::`/`.` 限定、或目标文件内**唯一**、或 CamelCase≥2 词/长度≥6/全大写常量 ⇒ 参与判定；否则记「符号不合格」**不判红**。
- **A 声明级匹配**：±3 行窗口内找**声明形态**——C# `class/struct/enum/interface Sym`、方法声明（`Sym(` + 访问修饰/返回类型）、属性/字段 `Sym{=;`、枚举成员 `Sym,`；Rust `fn/struct/enum/const/static/type/trait/mod … Sym`。
- **松口径列**：只看子串是否在 ±3 行内（旧判据 3 的口径）；**严口径列**：声明形态命中（本方案新增）。
- **上界口径**：通用短名即便通过 B，也可能撞到别的成员 ⇒ 「已漂出」是**上界**；逐条核查后才可移锚。

## 2. 五类（+1）分布

| 桶 | 数量 | 松口径（±3 子串） | 严口径（声明） | 说明 |
| --- | ---: | :---: | :---: | --- |
| 严格命中 | 102 | 是（dist=0..3） | **是** | 窗口内见到声明形态 ⇒ 最可信 |
| 无相邻符号 | 345 | — | — | 门只核文件+行号；**无判据**（本方案不覆盖） |
| 已漂出 | 26（符号不存在 8 + 行号漂出 18） | 否 | 否 | 需要逐条裁决（见 §3） |
| 松命中·非声明 | 48 | 是 | 否 | 多为常量/调用点（声明在别处）；Phase 1 需加「常量可全仓定位」规则 |
| 符号不合格(通用名) | 4 | — | — | B 过滤；只 WARN |
| ±3 边缘 | 3 | 是（dist 1–3） | 否 | 一次插入即越界（见 §5） |

## 3. 已漂出（26：符号不存在 8 + 行号漂出 18）

### 3a. 符号在目标文件内不存在（8 条）——**多数是「符号/文件配错」，不是行号漂移**
- B-108 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2696` | `IndexOutOfRangeException` | dist=None
- T-47 | `src/M2Server/Actor/BaseObject.cs:1771` | `SM_WEIGHTCHANGED` | dist=None
- T-47 | `src/M2Server/Player/PlayObject.cs:1210/1245` | `WeightChanged` | dist=None
- T-97 | `src/M2Server/Guild/GuildManager.cs:78` | `ReloadAllGuild` | dist=None
- T-118 | `src/M2Server/Actor/BaseObject.Message.cs:154` | `NullReferenceException` | dist=None
- T-142 | `src/Modules/SystemModule/Conf/ServerConf.cs:108` | `config_game_gold_name` | dist=None
- T-142 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2033` | `NpcPlayer.frames` | dist=None
- T-263 | `src/Modules/GameCommand/Commands/AuthCancelCommand.cs:18/21` | `ClientGuildAlly` | dist=None

### 3b. 行号漂出（声明存在但距锚 >3 行，18 条）
- T-216 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4933` | `GetLineVariableText` | dist=720
- T-95 | `src/M2Server/Player/PlayObject.cs:2882/2885` | `RecalcAbilitys` | dist=147
- T-53 | `src/Modules/ScriptEngine/ProcessingBase.cs:1420/1432` | `ProcessingBase.GetValNameValue` | dist=84
- T-207 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2861` | `ActionOfChangeExp` | dist=55
- T-266 | `src/M2Server/Items/GameItemSystem.cs:225/231` | `GameItemSystem.UpgradeRandomWeapon` | dist=43
- T-24 | `src/M2Server/Player/PlayObject.Base.cs:225/227` | `SendAddItem` | dist=27
- T-294 | `src/M2Server/Player/PlayObject.cs:5378` | `ItemDamageRevivalRing` | dist=23
- T-148 | `src/M2Server/Monster/Monsters/FireballMonster.cs:39/40` | `FireballMonster.Run` | dist=21
- D-13 | `src/GameSrv/Npc/Merchant.cs:132` | `Merchant.Run` | dist=20
- T-203 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2346` | `ActionOfOpenSaleDeal` | dist=17
- T-207 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3945` | `ActionOfRepairAllItem` | dist=16
- T-207 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:5317/5323` | `ActionOfBonusPoint` | dist=14
- T-266 | `src/M2Server/Items/GameItemSystem.cs:744/750` | `GetUpgradeStdItem` | dist=13
- T-97 | `src/M2Server/Guild/GuildInfo.cs:444/448` | `GuildInfo.SendGuildMsg` | dist=10
- T-214 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4933` | `ActionOfGmExecute` | dist=8
- T-216 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:4933` | `ActionOfGmExecute` | dist=8
- T-86 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:2723/2726` | `ActionOfOffLine` | dist=7
- T-29 | `src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs:279` | `characters_ablity.Level` | dist=4

## 4. 松命中·非声明（48 条）
- D-14 | `src/M2Server/Player/PlayObject.Message.cs:398/399` | `LogService.Error` | dist=0
- (散文) | `src/Modules/ScriptEngine/Consts/ExecutionCode.cs:210/213` | `REPAIRCASTLEDOOR` | dist=0
- (散文) | `src/M2Server/Player/PlayObject.Message.cs:1750/1755` | `RM_DOOROPEN` | dist=0
- (散文) | `src/BotSrv/Player/RobotPlayer.Operate.cs:911` | `SM_OPENDOOR_OK` | dist=0
- T-29 | `src/GameSrv/Word/WorldServer.cs:711/713` | `WorldServer.SaveHumanRcd` | dist=0
- T-36 | `src/OpenMir2/Packets/ClientPackets/ClientConf.cs:20` | `ShowMail` | dist=0
- T-47 | `src/GameSrv/Npc/Merchant.cs:1261/1396` | `CastleMgr.IncRateGold` | dist=0
- T-54 | `src/GameSrv/Npc/GuildOfficial.cs:186/187` | `GoldChanged` | dist=0
- T-54 | `src/M2Server/Player/PlayObject.Base.cs:3351` | `IsGuildMaster` | dist=0
- T-55 | `src/M2Server/Player/PlayObject.Operate.cs:806/843` | `SystemShare.FunctionNPC.GotoLable` | dist=0
- T-80 | `src/M2Server/Player/PlayObject.cs:3240` | `SendMsg` | dist=0
- T-80 | `src/M2Server/Player/PlayObject.Message.cs:1558` | `SM_FEATURECHANGED` | dist=0
- T-81 | `src/M2Server/Player/PlayObject.cs:1787/1798` | `killObject.DropUseItems` | dist=0
- T-94 | `src/M2Server/Player/PlayObject.cs:1199` | `GetStartPoint` | dist=0
- T-95 | `src/M2Server/Player/PlayObject.cs:2874/2876` | `StatusChanged` | dist=0
- T-99 | `src/GameSrv/Word/WorldServer.cs:648/653` | `RunNotice` | dist=0
- T-118 | `src/GameSrv/Word/WorldServer.cs:755/764` | `SendMsg` | dist=0
- T-118 | `src/M2Server/Player/PlayObject.Chat.cs:222/226` | `LogService.Error` | dist=0
- T-127 | `src/GameSrv/Word/Threads/SystemProcessor.cs:54/62` | `CastleMgr.Save` | dist=0
- T-127 | `src/GameSrv/GameApp.cs:246` | `LoadCastleList` | dist=0
- T-131 | `src/GameSrv/GameApp.cs:213` | `CastleMgr.Initialize` | dist=0
- T-131 | `src/GameSrv/GameApp.cs:245` | `GuildMgr.LoadGuildInfo` | dist=0
- T-137 | `src/M2Server/Npc/NormNpc.cs:68/73` | `GotoLable` | dist=0
- T-140 | `src/M2Server/Player/PlayObject.Operate.cs:114/128` | `UserSelect` | dist=0
- T-142 | `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs:3089/3092` | `GameGoldChanged` | dist=0
- T-143 | `src/M2Server/Monster/Monsters/ElfMonster.cs:70/83` | `KickException` | dist=0
- T-144 | `src/M2Server/Monster/Monsters/ElfMonster.cs:72` | `MakeClone` | dist=0
- T-149 | `src/OpenMir2/Common/ConfigFile.cs:263` | `Encoding.GetEncoding` | dist=0
- T-171 | `src/GameSrv/Maps/MapManager.cs:10` | `Dictionary` | dist=0
- T-196 | `src/M2Server/Actor/BaseObject.Base.cs:320` | `SuperMan` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeObModeCommand.cs:20` | `RM_DISAPPEAR` | dist=0
- T-196 | `src/Modules/GameCommand/Commands/ChangeObModeCommand.cs:20` | `RM_DISAPPEAR` | dist=0
- T-201 | `src/M2Server/Player/PlayObject.cs:2882/2885` | `RM_CHANGELIGHT` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Socket.cs:17` | `nRecog` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Message.cs:2015/2017` | `SendDefMessage` | dist=0
- T-199 | `src/M2Server/Player/PlayObject.Attack.cs:225/229` | `DoDamageWeapon` | dist=0
- T-204 | `src/M2Server/Actor/BaseObject.Message.cs:399/402` | `SystemShare.Config.SendRefMsgRange` | dist=0
- T-205 | `src/Modules/SystemModule/MagicEvent/MapEvent.cs:62` | `GetNextIdentity` | dist=0
- T-205 | `src/M2Server/CellObjectMgr.cs:10/15` | `TryAdd` | dist=0
- T-205 | `src/M2Server/Maps/Envirnoment.cs:666` | `CellObjectMgr.Remove` | dist=0
- T-205 | `src/M2Server/Player/PlayObject.cs:3278` | `RM_HIDEEVENT` | dist=0
- T-205 | `src/M2Server/Player/PlayObject.cs:3284` | `RM_SHOWEVENT` | dist=0
- (散文) | `src/M2Server/Player/PlayObject.Socket.cs:98` | `NetChannel.AddGateBuffer` | dist=0
- T-220 | `src/Modules/ScriptEngine/ProcessingBase.cs:1829` | `SystemShare.GetEnvirFilePath` | dist=0
- T-218 | `src/M2Server/Player/PlayObject.Message.cs:1870/1874` | `SendDelItemList` | dist=0
- T-224 | `src/GameSrv/Word/WorldServer.cs:1738/1765` | `ShowPreFixMsg` | dist=0
- T-224 | `src/M2Server/Player/PlayObject.cs:1969/1998` | `PlayObject.Die` | dist=0
- T-266 | `src/M2Server/Player/PlayObject.Attack.cs:31` | `CheckWeaponUpgrade` | dist=0

## 5. ±3 边缘（3 条，一次插入即越界）
- T-131 | `src/M2Server/Castle/UserCastle.cs:543` | `GetCastle` | dist=2
- T-206 | `src/BotSrv/Player/RobotPlayer.cs:4852` | `CM_BUTCH` | dist=1
- T-220 | `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs:642` | `CHECKNAMEDATELIST` | dist=3

## 6. 符号不合格（4 条，B 过滤；只 WARN）
- T-31 | `src/M2Server/Monster/Monsters/CastleDoor.cs:99/104` | `Dir` | dist=None
- T-94 | `src/M2Server/Player/PlayObject.Message.cs:266` | `Run` | dist=None
- T-169 | `src/M2Server/Player/PlayObject.cs:2235/2237` | `Teleport` | dist=None
- T-206 | `src/M2Server/Actor/AnimalObject.cs:42` | `ushort` | dist=None

## 7. 无相邻符号（345 条，按文件分组；本方案不覆盖）
- `src/M2Server/Player/PlayObject.Message.cs`（38）：D-9, D-9, D-10, D-13, A-8, (散文), T-26, T-28, T-28, T-29, T-57, T-76, T-79, T-82, T-100, T-101, T-134, T-135, T-142, T-142, T-142, T-147, T-148, T-148, T-203, (散文), T-206, T-206, T-207, T-207, T-207, T-207, T-207, T-214, T-224, T-224, T-270, T-292
- `src/M2Server/Player/PlayObject.cs`（29）：B-2, T-62, T-65, T-17, T-29, T-29, T-69, T-43, T-54, T-73, T-79, T-79, T-80, T-80, T-99, T-118, T-142, T-142, (散文), (散文), T-192, T-203, T-205, T-205, T-205, T-205, T-206, T-266, T-266
- `src/M2Server/Actor/BaseObject.cs`（19）：B-3, T-61, T-28, T-32, T-33, T-57, T-80, T-142, T-147, T-154, T-196, T-196, T-196, T-196, T-206, T-206, T-206, T-206, T-224
- `src/GameSrv/Word/WorldServer.cs`（18）：T-67, (散文), (散文), (散文), (散文), (散文), (散文), T-80, T-94, T-120, T-129, T-171, (散文), (散文), T-196, (散文), (散文), T-272
- `src/M2Server/Actor/BaseObject.Base.cs`（15）：B-115, B-115, T-31, T-32, T-49, T-77, T-154, T-154, T-154, T-154, T-177, T-196, T-196, T-196, T-224
- `src/M2Server/Player/PlayObject.Base.cs`（14）：B-112, B-113, B-113, B-118, T-61, T-63, T-66, T-20, T-28, T-43, T-142, T-142, T-142, T-211
- `src/M2Server/Player/PlayObject.Operate.cs`（12）：T-65, (散文), (散文), (散文), T-16, T-43, T-47, T-47, T-79, T-140, T-211, T-292
- `src/GameSrv/Npc/Merchant.cs`（10）：D-16, B-16, T-47, T-95, T-95, T-137, T-154, T-192, T-199, T-289
- `src/Modules/SystemModule/Conf/Model/GameSvrConf.cs`（9）：T-23, T-23, T-29, B-16, T-101, T-144, T-154, T-199, T-204
- `src/M2Server/Actor/BaseObject.Message.cs`（8）：D-13, T-80, T-83, T-192, T-196, T-199, T-206, T-224
- `src/GameSrv/DB/MySqlDB.cs`（7）：B-4, B-5, B-6, T-61, T-44, T-44, T-266
- `src/GameSrv/Word/WorldServer.MonGen.cs`（7）：B-1, T-79, T-95, T-155, T-206, T-206, T-269
- `src/Modules/ScriptEngine/ScriptParsers.cs`（7）：B-8, B-8, B-11, B-12, B-13, B-14, T-47
- `src/M2Server/Monster/MonsterObject.cs`（6）：B-114, B-114, T-49, T-143, T-143, T-196
- `src/GameSrv/DB/LocalDB.cs`（5）：T-56, T-140, T-193, T-212, T-219
- `src/M2Server/Maps/Envirnoment.cs`（5）：B-15, T-65, (散文), (散文), T-29
- `src/M2Server/Npc/NormNpc.cs`（5）：T-31, T-154, T-155, T-192, T-224
- `src/M2Server/Player/PlayObject.Chat.cs`（5）：T-113, T-210, T-210, T-212, T-216
- `src/Modules/SystemModule/Conf/ServerConf.cs`（5）：B-113, B-113, T-101, T-128, T-144
- `src/M2Server/Magic/MagicManager.cs`（4）：T-285, T-285, T-286, T-286
- `src/Modules/GameCommand/CommandHelp.cs`（4）：T-212, T-216, T-217, T-268
- `src/Modules/ScriptEngine/Processings/ConditionProcessingSys.cs`（4）：B-107, (散文), T-37, T-223
- `src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs`（4）：T-79, T-142, T-220, T-224
- `src/GameGate/Services/ClientSession.cs`（3）：A-1, A-2, T-206
- `src/GameSrv/GameApp.cs`（3）：D-21, (散文), (散文)
- `src/GameSrv/Npc/CastleOfficial.cs`（3）：(散文), T-40, T-142
- `src/M2Server/Castle/CastleConf.cs`（3）：T-127, T-152, T-152
- `src/M2Server/Monster/Monsters/ElfWarriorMonster.cs`（3）：T-79, T-143, T-143
- `src/M2Server/Player/PlayObject.Attack.cs`（3）：T-266, T-266, T-269
- `src/M2Server/Player/PlayObject.Socket.cs`（3）：T-100, T-142, T-202
- `src/Modules/SystemModule/Data/MapItem.cs`（3）：T-19, T-25, T-32
- `src/OpenMir2/HUtil32.cs`（3）：B-9, B-10, (散文)
- `src/OpenMir2/Messages.cs`（3）：D-15, T-44, T-196
- `src/BotSrv/Player/RobotPlayer.Operate.cs`（2）：T-29, T-199
- `src/GameSrv/Npc/GuildOfficial.cs`（2）：(散文), T-138
- `src/LoginGate/Services/ClientSession.cs`（2）：A-3, A-4
- `src/M2Server/Actor/AnimalObject.cs`（2）：T-31, T-96
- `src/M2Server/Castle/UserCastle.cs`（2）：T-38, T-153
- `src/M2Server/Items/GameItemSystem.cs`（2）：T-95, T-266
- `src/M2Server/M2Share.cs`（2）：T-64, T-42
- `src/M2Server/Monster/Monsters/CastleDoor.cs`（2）：B-15, (散文)
- `src/M2Server/Monster/Monsters/PetSuperGuard.cs`（2）：T-155, T-155
- `src/Modules/GameCommand/Commands/ForcedWallconquestWarCommand.cs`（2）：(散文), (散文)
- `src/Modules/GameCommand/Commands/ShowEffectCommand.cs`（2）：T-215, T-215
- `src/Modules/GameCommand/Commands/StartQuestCommand.cs`（2）：T-219, T-219
- `src/Modules/SystemModule/ActorManager.cs`（2）：T-137, T-140
- `src/Modules/SystemModule/SystemShare.cs`（2）：T-63, T-64
- `src/OpenMir2/Common/StringList.cs`（2）：T-150, T-153
- `src/OpenMir2/Grobal2.cs`（2）：B-115, B-115
- `src/OpenMir2/RandomNumber.cs`（2）：B-7, T-269
- `src/BotSrv/Maps/TMap.cs`（1）：T-10
- `src/GameSrv/AppService.cs`（1）：(散文)
- `src/GameSrv/Maps/MapManager.cs`（1）：(散文)
- `src/GameSrv/Word/Threads/SystemProcessor.cs`（1）：T-127
- `src/GameSrv/Word/Threads/UserProcessor.cs`（1）：T-112
- `src/LoginSrv/Services/ClientSession.cs`（1）：A-4
- `src/LoginSrv/Storage/AccountStorage.cs`（1）：A-7
- `src/M2Server/Actor/ActorEntity.cs`（1）：T-207
- `src/M2Server/Actor/BaseObject.ViewRange.cs`（1）：D-8
- `src/M2Server/Castle/CastleManager.cs`（1）：T-127
- `src/M2Server/Guild/GuildInfo.cs`（1）：T-39
- `src/M2Server/Monster/Monsters/ArcherGuard.cs`（1）：T-155
- `src/M2Server/Monster/Monsters/BoneKingMonster.cs`（1）：T-143
- `src/M2Server/Monster/Monsters/GuardUnit.cs`（1）：T-96
- `src/M2Server/Monster/Monsters/SuperGuard.cs`（1）：T-155
- `src/M2Server/Net/ChannelMessageHandler.cs`（1）：T-100
- `src/M2Server/Player/CharacterObject.cs`（1）：T-196
- `src/Modules/GameCommand/CommandAttribute.cs`（1）：T-212
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

## 8. Phase 1/2 入口（未开）
- Phase 1：只对 6 道场景锚门覆盖文件集开启严口径判红，**错误码 `SRC_DECL_DRIFT`**（与场景门 `ANCHOR_DRIFT` 区分），输出标注「本层=声明级」。
- Phase 2：逐条裁决本卡 §3/§5 后全量开启；每条「已漂」先走**改判据前溯源四步**（锚基线 → 漂移提交 → 是否登记/正当 → 无二次漂移），确认新状态正确再移锚，**不自动修**。
- 常量类（`RM_*`/`SM_*`/枚举）的严口径需补「全仓唯一定义」规则，否则恒落「松命中·非声明」。

## 9. 复现
```text
python -I mir2-rs/tests/parity/m3/whitelist_anchor_gate.py report-src-drift --tsv <out.tsv> --json <out.json>
```
