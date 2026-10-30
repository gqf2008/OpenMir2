# GM 命令 `Imp::Unimplemented` 74 条对 oracle 钉性盘点（2026-10-30）

> **计数订正**：同日族 P（密码锁门 7）、族 T（会话开关/骑马 7）、族 Q（查询/展示 10）与
> 族 X1（禁言名单 5）已落地，现存 `Imp::Unimplemented` = **45**——见 §7；§1–§6 保留盘点时点值。

- 卡：`openmir2-gm-unimpl-inventory`（tier=L，worker=openmir2-dev-3，分支 `card/gm-unimpl-inventory-dev3`，基线 `walgit/master` `26f88f483`）。
- 写集：仅本文档。不改 `mir2-rs/**`、不实现任何命令、不动 `Command.conf`。
- oracle = C# 参照实现 `src/Modules/GameCommand/Commands/`（185 个命令类）；rust = `mir2-rs/bins/game-svr/src/gmcmd.rs` 的 `COMMANDS` 常量表（185 行）。

## 1. 口径与证据

- **提取**：对 `gmcmd.rs` 逐 `CommandDef` 块做括号配对解析（非跨块正则），得 186 个表项块、其中 `imp: Imp::Unimplemented` 恰 **74** 块、命令名互异（无重名）。已知样例清单（+-jy/+jy/-jy/ClearCopyItem/…/setunit）全覆盖。
- **rust 现状（74 条全同）**：`execute` 命中表项 → 权限门 → `Imp::Unimplemented` 臂 ⇒ 恒零帧、恒零消息，`log_note` = 「GM 命令体未实现」。参数串不参与派发（`params` 字段仅登记 oracle `Execute` 形参个数）。
- **判定口径**（按卡面三类）：
  - **缺口**：oracle `Execute` 过权限门后存在**可观察行为**（SysMsg / 状态写 / 下行帧 / 吞异常路径的帧差），rust 恒零帧 ⇒ 至少一条输入上有帧差；
  - **已对齐**：oracle 过权限门后**恒零帧**（体整段注释 / 空体 / 死分支 / 反射异常被吞），rust 零帧 = oracle 零帧；
  - **垃圾命令**：oracle 无对应注册类 —— 本轮 **0 条**（见 §3 证伪）。
- **名字 → oracle 类映射**：rust 表项名是部署 `Command.conf` 覆盖后的现网名；逐条映射沿用设计件 §4.3 全量表（`mir2-rs/tests/parity/m3/design-gm-commands-20261023.md`，185 行现网名/出厂名/权限/参数），并回源逐类核对 `[Command]` 名与命令体原文。
- **「已对齐」不是完工态**：沿用 `gmcmd.rs` 已有惯例（`ShowEffect` 族注释「oracle 命令体整段注释；此前这段是 `Imp::Unimplemented`，现在仍是零帧但已登记」）——后续落地时把表项从 `Imp::Unimplemented` 换成带 `log_note` 的 `no_frames`（登记文案）或真实现；本文档只钉性、不改表。

## 2. 结论摘要

| 类别 | 条数 | 说明 |
|---|---:|---|
| 真缺口 | **65** | oracle 有可观察行为，rust 恒零帧有帧差；按 9 族拆实现提案（§5，已在本线程逐族签 comment） |
| 已对齐 | **9** | oracle 恒零帧，rust 零帧即等价；落地时随所在族卡顺手换成 `no_frames` 登记 |
| 垃圾命令 | **0** | 卡面假设的「fdgfdg56 等 9 条垃圾」全部证伪，见 §3 |

## 3. 「垃圾命令」假设证伪

卡面样例里的 `fdgfdg56 / fdht7ed / gfh678dfg / ghjhg6 / hjhg556 / hjhgj67 / sadsa54 / sdasdert / s` 不是垃圾：它们都是 `Command.conf` `[Command]` 段设置的**混淆现网名**，各映射到一个真实 oracle 类并带完整权限门：

| 现网名 | oracle 类 | 类所在文件 |
|---|---|---|
| `fdgfdg56` | `LockCommand`（锁定密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `fdht7ed` | `SearchMasterCommand`（查询师徒位置） | `src/Modules/GameCommand/Commands/SearchMasterCommand.cs` |
| `gfh678dfg` | `UnLockCommand`（解除密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `ghjhg6` | `ChgpassWordCommand`（修改登录密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `hjhg556` | `UnlockStorageCommand`（解除仓库密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `hjhgj67` | `SetPassWordCommand`（设置登录密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `sadsa54` | `UnPasswWordCommand`（解除登录密码） | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs` |
| `sdasdert` | `SearchDearCommand`（查询配偶位置） | `src/Modules/GameCommand/Commands/SearchDearCommand.cs` |
| `s` | `MakeItemCommond`（造物 `Make`） | `src/Modules/GameCommand/Commands/MakeItemCommand.cs` |

这 9 条里除 `fdht7ed`/`sdasdert` 是完整查询实现、`s` 是带 RNG 的完整造物实现外，其余 6 条全部落在**密码锁门族**（部署 `PasswordLockSystem=0` ⇒ 命令体第一句恒回红字「游戏密码保护系统还没有启用!!!」，`MessageSettings.NoPasswordLockSystemMsg`）。

## 4. 主表（74 条）

oracle 状态缩写：**缺口·全实现** = 命令体整段活跃（含副作用）；**缺口·部分** = 校验/回执活跃、核心被注释（有帧差面）；**已对齐** = 过权限门后恒零帧。族号见 §5。

### 4.1 真缺口（65 条）

| 现网名 | oracle 符号 | oracle 状态 | 族 | 行为要点 / 必须照抄的怪癖 |
|---|---|---|---|---|
| `+-jy` | `DisableSendMsgListCommand.Execute`（`src/Modules/GameCommand/Commands/DisableSendMsgListCommand.cs`） | 缺口·部分 | X1 | 空列表红字「禁言列表为空!!!」；非空蓝字「禁言列表:」+ 循环体整段注释（不逐条列出） |
| `+jy` | `DisableSendMsgCommand.Execute`（`src/Modules/GameCommand/Commands/DisableSendMsgCommand.cs`） | 缺口·全实现 | X1 | 目标在线置 `FilterSendMsg`；无条件加 `DisableSendMsgList` + `SaveDisableSendMsgList()`；绿字「已加入禁言列表。」；缺名红字帮助 |
| `-jy` | `EnableSendMsgCommand.Execute`（`src/Modules/GameCommand/Commands/EnableSendMsgCommand.cs`） | 缺口·部分 | X1 | 循环体整段注释 ⇒ 恒红字「没有被禁言!!!」；缺名红字帮助 |
| `ClearCopyItem` | `ClearCopyItemCommand.Execute`（`src/Modules/GameCommand/Commands/ClearCopyItemCommand.cs`） | 缺口·全实现 | X2 | 背包/仓库去重删除；怪癖：内层删除写的是 `PlayerActor.ItemList`（调用者）而非目标 ⇒ 1:1 照抄；离线红字 |
| `ContestPoint` | `ContestPointCommand.Execute`（`src/Modules/GameCommand/Commands/ContestPointCommand.cs`） | 缺口·部分 | C | 行会不存在绿字「行会: X 不存在!!!」；存在 ⇒ 得分 SysMsg 被注释 ⇒ 静默；缺参/'?' 双红字 |
| `DelNpc` | `DelNpcCommand.Execute`（`src/Modules/GameCommand/Commands/DelNpcCommand.cs`） | 缺口·部分 | X2 | 删除循环整段注释 ⇒ 无论面对什么恒红字 `CommandHelp.GameCommandDelNpcMsg`（「命令使用方法不正确，必须与NPC面对面…」） |
| `DisableFilter` | `DisableFilterCommand.Execute`（`src/Modules/GameCommand/Commands/DisableFilterCommand.cs`） | 缺口·全实现 | T | 翻转全局 `FilterWord` + 绿字二选一 |
| `MemberFuncEx` | `MemberFunctionExCommand.Execute`（`src/Modules/GameCommand/Commands/MemberFunctionExCommand.cs`） | 缺口·全实现 | X2 | `FunctionNPC.GotoLable(player, "@Member")` 脚本跳线（宿主 NPC 接缝） |
| `IP` | `HumanLocalCommand.Execute`（`src/Modules/GameCommand/Commands/HumanLocalCommand.cs`） | 缺口·部分 | Q | 怪癖：判空写反成 `IsNullOrEmpty(s) \|\| !IsNullOrEmpty(s)` 恒真 ⇒ **恒**红字帮助（永不查在线） |
| `LotteryTicket` | `LotteryTicketCommandL.Execute`（`src/Modules/GameCommand/Commands/LotteryTicketCommand.cs`，类名带尾 L） | 缺口·全实现 | Q | 一条绿字：`GameCommandLotteryTicketMsg` 格式串 + 8 个彩票配置值 |
| `Map` | `MapInfoCommand.Execute`（`src/Modules/GameCommand/Commands/MapInfoCommand.cs`） | 缺口·部分 | Q | 怪癖：直取 `@params[2]` ⇒ 实参 <3 抛越界被吞 ⇒ 零帧；足参 ⇒ 取格子属性/对象数绿字或红字失败 |
| `MapInfo` | `ShowMapInfoCommand.Execute`（`src/Modules/GameCommand/Commands/ShowMapInfoCommand.cs`） | 缺口·全实现 | Q | 两条绿字：当前地图 `GameCommandMapInfoMsg` + 尺寸 `GameCommandMapInfoSizeMsg` |
| `Mission` | `MissionCommand.Execute`（`src/Modules/GameCommand/Commands/MissionCommand.cs`） | 缺口·全实现 | X2 | 写全局 `BoMission/MissionMap/MissionX/MissionY` + 绿字确认（MobPlace 的前置） |
| `MobCountwa` | `MobCountCommand.Execute`（`src/Modules/GameCommand/Commands/MobCountCommand.cs`） | 缺口·全实现 | Q | 地图不存在红字 `GameCommandMobCountMapNotFound`；否则绿字怪物数 |
| `HumanCountwa` | `HumanCountCommand.Execute`（`src/Modules/GameCommand/Commands/HumanCountCommand.cs`） | 缺口·全实现 | Q | 地图不存在红字；否则绿字玩家数（复用 `GameCommandMobCountMonsterCount` 文案） |
| `MobNpc` | `MobNpcCommand.Execute`（`src/Modules/GameCommand/Commands/MobNpcCommand.cs`） | 缺口·部分 | M | 缺参/'?' ⇒ 红字帮助（活跃面）；足参 ⇒ 建 NPC 体整段注释 ⇒ 静默 |
| `MobPlace` | `MobPlaceCommand.Execute`（`src/Modules/GameCommand/Commands/MobPlaceCommand.cs`） | 缺口·全实现 | M | 依赖全局 Mission 目标；未设双红字；按数量 `RegenMonsterByName` + 置 Mission 标记；绿字集中确认 |
| `OffHorse` | `TakeOffHorseCommand.Execute`（`src/Modules/GameCommand/Commands/TakeOffHorseCommand.cs`） | 缺口·全实现 | T | 未骑马静默；骑马 ⇒ `OnHorse=false` + `FeatureChanged()` 外观帧 |
| `OnHorse` | `TakeOnHorseCommand.Execute`（`src/Modules/GameCommand/Commands/TakeOnHorseCommand.cs`） | 缺口·全实现 | T | `HorseType=0` 红字「骑马必须先戴上马牌!!!」；上马 + 外观帧 + `FunctionNPC.GotoLable("@OnHorse")`（宿主接缝，FunctionNPC 为空时 NRE 被吞） |
| `PasswordLock` | `PasswordLockCommand.Execute`（`src/Modules/GameCommand/Commands/PasswordLockCommand.cs`） | 缺口·部分 | P | 部署 `PasswordLockSystem=0` ⇒ 第一句恒红字「游戏密码保护系统还没有启用!!!」（`NoPasswordLockSystemMsg`）；启用分支（RM_PASSWORD 状态机）现网不可达 |
| `ReCallMobEx` | `ReCallMobExCommand.Execute`（`src/Modules/GameCommand/Commands/ReCallMobExCommand.cs`） | 缺口·全实现 | M | 怪癖：`params[1..3]` 的取参守卫全写成 `Length > 0` ⇒ 缺参越界被吞 ⇒ 零帧；足参 ⇒ `RegenMonsterByName` 召宝宝 + `SlaveExpLevel=1` + 名字颜色 |
| `Reconnection` | `ReconnectionCommand.Execute`（`src/Modules/GameCommand/Commands/ReconnectionCommand.cs`） | 缺口·全实现 | X2 | 体首 `Permission < 10` 静默 return（外门已 10 ⇒ 死分支照抄）；'?' 蓝字说明；缺参红字帮助；足参发 `RM_RECONNECTION` |
| `s` | `MakeItemCommond.Execute`（`src/Modules/GameCommand/Commands/MakeItemCommand.cs`） | 缺口·全实现 | M | 造物：perm<PermissionMax 四门（允许表/攻城区/安全区/数量=1）；`Random(MakeRandomAddValue)` **抽数调用在**（升级逻辑注释）；高价物 perm<5 且非 TestServer 拒；`SendAddItem` 背包帧；名字不存在红字 |
| `SabukWallGold` | `ShowSbkGoldCommand.Execute`（`src/Modules/GameCommand/Commands/ShowSbkGoldCommand.cs`） | 缺口·全实现 | C | 无参列全部城堡金库绿字；有参改 `TotalGold`（'=' 设值/'-' 恒减 1/'+' 加值）；怪癖：`char ctr = sCtr[1]` ⇒ 第二参长度 <2 抛越界被吞 ⇒ 零帧 |
| `sadsa54` | `UnPasswWordCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 同密码锁门：部署恒红字「游戏密码保护系统还没有启用!!!」 |
| `sdasdert` | `SearchDearCommand.Execute`（`src/Modules/GameCommand/Commands/SearchDearCommand.cs`） | 缺口·全实现 | X2 | 配偶定位：未结婚红字/离线按性别红字/在线互发定位绿字（含给对方两条） |
| `fdht7ed` | `SearchMasterCommand.Execute`（`src/Modules/GameCommand/Commands/SearchMasterCommand.cs`） | 缺口·全实现 | X2 | 师徒定位：为师遍历 `MasterList` 互发，为徒查 `MasterHuman`；无师徒红字 |
| `setflag` | `SetFlagCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·全实现 | F | 怪癖：`SetQuestFlagStatus` 写在**调用者**身上（非 targetObject）；对 target 回绿字 `[n]=ON/OFF`；target 不在线 ⇒ 对 null 调 SysMsg 抛 NRE 被吞 ⇒ 零帧 |
| `setopen` | `SetOpenCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·全实现 | F | 同 setflag（写 `QuestUnitOpenStatus`，同样写调用者） |
| `setunit` | `SetUnitCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·全实现 | F | 同 setflag（写 `QuestUnitStatus`，同样写调用者） |
| `showflag` | `ShowHumanFlagCommand.Execute`（`src/Modules/GameCommand/Commands/ShowHumanFlagCommand.cs`） | 缺口·全实现 | Q | 查目标任务标志红/绿回执；离线红字 |
| `ShowMapMode` | `ShowMapModeCommand.Execute`（`src/Modules/GameCommand/Commands/ShowMapModeCommand.cs`） | 缺口·全实现 | Q | 地图不存在红字「X 不存在!!!」；否则蓝字「地图模式: …」（`GetEnvirInfo`） |
| `Showuseiteminfo` | `ShowUseItemInfoCommand.Execute`（`src/Modules/GameCommand/Commands/ShowUseItemInfoCommand.cs`） | 缺口·全实现 | Q | 怪癖：判空用 `sHumanName[1]` ⇒ 单字符名越界被吞 ⇒ 零帧；逐穿戴位蓝字——格式串是 `%s[%s]IDX[%d]…`，.NET `string.Format` 无 `{n}` 占位 ⇒ **原样字面输出** |
| `StartContest` | `StartContestCommand.Execute`（`src/Modules/GameCommand/Commands/StartContestCommand.cs`） | 缺口·全实现 | C | `Fight3Zone` 门红字；红/绿 SysMsg 各一 + 两次 `CryCry` 广播；范围内玩家 `FightZoneDieCount=0`；怪癖：`!ObMode \|\| !AdminMode`（或而非与）照抄；行会聚合 add 被注释 |
| `vv杀死` | `KillCommand.Execute`（`src/Modules/GameCommand/Commands/KillCommand.cs`） | 缺口·全实现 | X2 | 带名 ⇒ 目标 `Die()`（真杀死）；无名 ⇒ 面对面目标 `Die()`；查无此人/未面对面红字 |
| `VV给予权限` | `SetPermissionCommand.Execute`（`src/Modules/GameCommand/Commands/SetPermissionCommand.cs`） | 缺口·全实现 | X2 | 校验 0-10 + 在线 ⇒ 写目标运行时 `Permission`（L3 唯一运行时写点之一）+ 红字回执；`ShowMakeItemMsg` 开时记日志 |
| `交易` | `LetTradeCommand.Execute`（`src/Modules/GameCommand/Commands/LetTradeCommand.cs`） | 缺口·全实现 | T | 翻转 `AllowDeal` + 绿字 `[允许交易]`/`[禁止交易]` |
| `信息` | `HumanInfoCommand.Execute`（`src/Modules/GameCommand/Commands/HumanInfoCommand.cs`） | 缺口·部分 | Q | 校验红字 + 离线红字活跃；目标在线 ⇒ `GetBaseObjectInfo` SysMsg 被注释 ⇒ 静默 |
| `停止叛变` | `SpirtStopCommand.Execute`（`src/Modules/GameCommand/Commands/SpirtStopCommand.cs`） | 缺口·全实现 | X2 | 全局 `SpiritMutinyTick=0` + 绿字「祈祷叛变已停止。」 |
| `允许天地合一` | `AllowGroupReCallCommand.Execute`（`src/Modules/GameCommand/Commands/AllowGroupReCallCommand.cs`） | 缺口·全实现 | W | 翻转 `AllowGroupReCall`（**入库**列 `characters.AllowGroupReCall`，载入/存回见设计件 §4.1.5）+ 绿字 |
| `创建行会` | `AddGuildCommand.Execute`（`src/Modules/GameCommand/Commands/AddGuildCommand.cs`） | 缺口·全实现 | W | `ServerIndex≠0` 红字；掌门在线且无行会 ⇒ `GuildMgr.AddGuild` + `SS_205` 跨服通知 + 绿字；掌门身上的行会刷新段被注释 |
| `删除技能` | `DelSkillCommand.Execute`（`src/Modules/GameCommand/Commands/DelSkillCommand.cs`） | 缺口·全实现 | X2 | `All` 删全部（不发帧）；按名删 ⇒ `SendDelMagic` + 双方绿字 + `RecalcAbilitys` |
| `删除行会` | `DelGuildCommand.Execute`（`src/Modules/GameCommand/Commands/DelGuildCommand.cs`） | 缺口·全实现 | W | `ServerIndex≠0` 红字；`GuildMgr.DelGuild` 成功发 `SS_206`，失败红字「没找到…」 |
| `加入门派` | `LetGuildCommand.Execute`（`src/Modules/GameCommand/Commands/LetGuildCommand.cs`） | 缺口·全实现 | T | 翻转 `AllowGuild` + 绿字 `[允许加入行会]`/`[禁止加入行会]` |
| `复活` | `ReAliveCommand.Execute`（`src/Modules/GameCommand/Commands/ReAliveCommand.cs`） | 缺口·全实现 | X2 | 目标 `ReAlive()` + `HP=MaxHP` + `RM_ABILITY` 帧 + 双绿字；离线红字 |
| `开始攻城` | `ForcedWallconquestWarCommand.Execute`（`src/Modules/GameCommand/Commands/ForcedWallconquestWarCommand.cs`） | 缺口·全实现 | C | 翻转 `UnderWar`；开战置 `WarDate/ShowOverMsg=false` + `StartWallconquestWar` + `SS_212` + 全服广播 + `SS_204` + `MainDoorControl(true)`；停战 `StopWallconquestWar`；城堡不存在红字 |
| `怪` | `MobCommand.Execute`（`src/Modules/GameCommand/Commands/MobCommand.cs`） | 缺口·全实现 | M | 上限 64（`_MIN(64,nCount)` 再转 byte）；`GetFrontPosition` 取面前坐标；逐只 `RegenMonsterByName` + `SlaveExpLevel=nLevel`；失败红字 break |
| `拒绝组队聊天` | `LetShoutCommand.Execute`（`src/Modules/GameCommand/Commands/LetShoutCommand.cs`） | 缺口·全实现 | T | 翻转 `BanShout` + 绿字 |
| `探测` | `SearchHumanCommand.Execute`（`src/Modules/GameCommand/Commands/SearchHumanCommand.cs`） | 缺口·全实现 | X2 | `ProbeNecklace \|\| Permission>=6` 门（外门已 10 ⇒ 恒过，照抄）；10 秒节流（perm>=3 免）+ `ProbeTick` 写；在线蓝字定位（怪癖：坐标尾拼的是**调用者** `CurrY`）、离线红字 |
| `是我` | `MemberFunctionCommand.Execute`（`src/Modules/GameCommand/Commands/MemberFunctionCommand.cs`） | 缺口·全实现 | X2 | `ScriptGotoCount=0` + `ManageNPC.GotoLable("@Member")`（判空守卫在，宿主接缝） |
| `查看禁言` | `ShutupListCommand.Execute`（`src/Modules/GameCommand/Commands/ShutupListCommand.cs`） | 缺口·部分 | X1 | 列表空 ⇒ 绿字「禁言列表为空!!!」（`GameCommandShutupListIsNullMsg`）；非空 ⇒ 遍历整段注释 ⇒ 静默；`DenySayMsgList` 全仓无活跃写点 ⇒ 现网恒空 ⇒ 恒绿字 |
| `查看私聊` | `ViewWhisperCommand.Execute`（`src/Modules/GameCommand/Commands/ViewWhisperCommand.cs`） | 缺口·全实现 | X1 | 怪癖：判空用 `sChrName[1]` ⇒ 单字符名越界被吞 ⇒ 零帧；toggle 目标 `WhisperHuman` + 双向绿字/离线红字 |
| `沙城` | `ChangeSabukLordCommand.Execute`（`src/Modules/GameCommand/Commands/ChangeSabukLordCommand.cs`） | 缺口·全实现 | C | 怪癖：`bool.Parse(@params[2])` 缺第三参抛越界/非 True/False 抛异常 ⇒ 被吞 ⇒ 零帧；足参 ⇒ `castle.GetCastle(guild)`（改归属）+ `boFlag` 时 `SS_211` + 绿字；行会未建红字 |
| `清除仓库密码` | `ClearHumanPasswordCommand.Execute`（`src/Modules/GameCommand/Commands/ClearHumanPasswordCommand.cs`） | 缺口·全实现 | X2 | **不查** `PasswordLockSystem` 门；目标在线 ⇒ 清 `IsPasswordLocked/IsUnLockStoragePwd/StoragePwd`（随存档路径可见）+ 双绿字；离线静默 return |
| `清除任务` | `ClearMissionCommand.Execute`（`src/Modules/GameCommand/Commands/ClearMissionCommand.cs`） | 缺口·部分 | X2 | 全部校验活跃（'?' 蓝字说明/离线红字）；在线 ⇒ **只**发绿字「任务标志已经全部清零。」——清零代码根本不存在（非注释，是没写） ⇒ 1:1 只回执 |
| `清除怪物` | `ClearMapMonsterCommand.Execute`（`src/Modules/GameCommand/Commands/ClearMapMonsterCommand.cs`） | 缺口·部分 | X2 | 杀怪循环整段注释 ⇒ `envir` 恒 null ⇒ 足参恒红字「输入的地图不存在!!!」（「已清除怪物数: 0」在其后不可达）；缺参红字帮助 |
| `祈祷生效` | `SpirtStartCommand.Execute`（`src/Modules/GameCommand/Commands/SpirtStartCommand.cs`） | 缺口·全实现 | X2 | `SpiritMutinyTick = now + dwTime`（参>0 取参×1000，否则 `SpiritMutinyTime`）+ 绿字持续秒数 |
| `禁止行会信息` | `BanGuildChatCommand.Execute`（`src/Modules/GameCommand/Commands/BanGuildChatCommand.cs`） | 缺口·全实现 | T | 翻转 `BanGuildChat` + 绿字 |
| `退出门派` | `EndGuildCommand.Execute`（`src/Modules/GameCommand/Commands/EndGuildCommand.cs`） | 缺口·全实现 | W | 掌门（`GuildRankNo<=1`）红字拒绝；成员 ⇒ `DelMember` + `SS_207` + 清行会标记/名字刷新 + 绿字；无行会红字 |
| `fdgfdg56` | `LockCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 密码锁门：部署恒红字「游戏密码保护系统还没有启用!!!」；启用分支（锁定/已锁提示）现网不可达 |
| `gfh678dfg` | `UnLockCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 同上（启用分支走 RM_PASSWORD 解锁状态机） |
| `ghjhg6` | `ChgpassWordCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 同上（启用分支 `PwdFailCount>3` 锁定判定 + 改密状态机） |
| `hjhg556` | `UnlockStorageCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 同上（启用分支 `PasswordErrorCountLock` 门 + 解仓库锁状态机） |
| `hjhgj67` | `SetPassWordCommand.Execute`（`src/Modules/GameCommand/Commands/SetPassWordCommand.cs`） | 缺口·部分 | P | 同上（启用分支 RM_PASSWORD 设密状态机） |
| `GiveMine` | `GiveMineCommand.Execute`（`src/Modules/GameCommand/Commands/GiveMineCommand.cs`） | 缺口·全实现 | M | 怪癖：`nMineCount/nDura` 取参守卫也写成 `Length > 0` ⇒ 单参越界被吞 ⇒ 零帧；纯度未填 ⇒ **RNG** `Random(18)+3`；逐件 `CopyToUserItemFromName` + `StdMode=43` + 负重门 + `SendAddItem`；`NeedIdentify` 日志被注释 |


### 4.2 已对齐（9 条，oracle 过权限门后恒零帧）

| 现网名 | oracle 符号 | 恒零帧原因 |
|---|---|---|
| `ClearItemMap` | `ClearItemMapCommand.Execute`（`src/Modules/GameCommand/Commands/ClearItemMapCommand.cs`） | 校验不过静默 return（SysMsg 被注释）；校验过后清地物核心整段注释、`if (sItemName == "ALL") {}` 空支 |
| `EndContest` | `EndContestCommand.Execute`（`src/Modules/GameCommand/Commands/EndContestCommand.cs`） | 方法体整段注释（含声明） |
| `ServerInfo` | `ServerInfoCommand.Execute`（`src/Modules/GameCommand/Commands/ServerInfoCommand.cs`） | 方法体整段注释（StringBuilder 空转） |
| `SetMapMode` | `SetMapModeCommamd.Execute`（`src/Modules/GameCommand/Commands/SetMapModeCommamd.cs`） | 活跃面只有取参与 `Permission<6` 内门（外门已 10 ⇒ 不可达）；全部地图模式分支与回执整段注释 |
| `showopen` | `ShowHumanUnitOpenCommand.Execute`（`src/Modules/GameCommand/Commands/ShowHumanUnitOpenCommand.cs`） | 空方法体 |
| `showunit` | `ShowUnitCommand.Execute`（`src/Modules/GameCommand/Commands/ShowUnitCommand.cs`） | 空体且 `Execute()` 零形参 ⇒ `GameCommand.Handle` 走 default 支以 2 实参反射调用 ⇒ `TargetParameterCountException` 被 `ProcessUserLineMsg` 吞 ⇒ 零帧（同 `@commands` 活标本，设计件 §1.3） |
| `Total` | `TotalCommand.Execute`（`src/Modules/GameCommand/Commands/TotalCommand.cs`） | 唯一行（HearMsg 总在线数）被注释 |
| `who` | `WhoCommand.Execute`（`src/Modules/GameCommand/Commands/WhoCommand.cs`） | 唯一行（HearMsg 本服在线数）被注释 |
| `查看周围` | `MobLevelCommand.Execute`（`src/Modules/GameCommand/Commands/MobLevelCommand.cs`） | 只做 `GetRangeBaseObject` 收集 + 列表清空；逐对象 SysMsg 被注释 ⇒ 无可观察行为 |

### 4.3 缺口分族索引（65 条）

| 族 | 条数 | 成员（现网名） | tier 提案 |
|---|---:|---|---|
| P 密码锁门 | 7 | hjhgj67 / sadsa54 / ghjhg6 / hjhg556 / gfh678dfg / fdgfdg56 / PasswordLock | L |
| Q 查询/展示 | 10 | HumanCountwa / MobCountwa / Map / MapInfo / ShowMapMode / showflag / IP / 信息 / Showuseiteminfo / LotteryTicket | L |
| T 会话开关/骑马 | 7 | 交易 / 加入门派 / 拒绝组队聊天 / 禁止行会信息 / OffHorse / OnHorse / DisableFilter | L |
| F 脚本标志 | 3 | setflag / setopen / setunit | R（标志写随存档落库；文本面可按 L 审） |
| M 刷怪/造物 | 6 | MobNpc / MobPlace / ReCallMobEx / 怪 / GiveMine / s | R |
| C 城堡/赛事 | 5 | ContestPoint / SabukWallGold / 沙城 / 开始攻城 / StartContest | R |
| W 行会 | 4 | 创建行会 / 删除行会 / 退出门派 / 允许天地合一 | R |
| X1 禁言名单 | 5 | +jy / +-jy / -jy / 查看禁言 / 查看私聊 | L |
| X2 目标玩家控制/杂项 | 18 | ClearCopyItem / DelNpc / Mission / 删除技能 / Reconnection / vv杀死 / 复活 / VV给予权限 / fdht7ed / sdasdert / 探测 / MemberFuncEx / 是我 / 清除仓库密码 / 清除任务 / 清除怪物 / 祈祷生效 / 停止叛变 | R（其中 DelNpc/清除任务/清除怪物/Mission/祈祷生效/停止叛变为纯文本或内存全局，可在卡内按 L 子项先行） |

## 5. 缺口族拆卡提案（已逐族在本线程签 `comment`）

每族一条 issue 提案 comment（字段：goal / write_set / oracle / acceptance 牙 / forbidden / tier / members），待 owner/Leader 裁决后再开正式卡。提案要点汇总：

- **族 P（7，L）**：write_set 仅 `gmcmd.rs`。oracle：`SetPassWordCommand.cs`（6 类同文件）+ `PasswordLockCommand.cs`。验收牙：每条固定红字文本「游戏密码保护系统还没有启用!!!」+ 零副作用断言（删配置门变异必红）。forbidden：不实现 RM_PASSWORD 密码状态机（部署门恒 0 ⇒ 不可达面）。
- **族 Q（10，L）**：write_set 仅 `gmcmd.rs`（地图只读视图走既有 `GmWorld`）。oracle：各查询类 .cs。验收牙：每条红/绿回执文本 + 怪癖支（IP 恒红帮助；信息在线静默；Map 足参/缺参两支；Showuseiteminfo `%s` 字面输出与单字符名越界零帧）。forbidden：不做任何状态写。
- **族 T（7，L）**：write_set `gmcmd.rs` + 会话开关字段（`AllowDeal/AllowGuild/BanShout/BanGuildChat/OnHorse` 等会话内布尔）。oracle：`LetTradeCommand.cs` 等。验收牙：toggle 双绿字往返 + OffHorse/OnHorse 的 `FeatureChanged` 外观帧到位（宿主已接则断言帧，未接登记）；OnHorse 的马牌红字。forbidden：不接 `@OnHorse` NPC 脚本跳线的实现面（登记接缝，脚本行为归脚本卡）。
- **族 F（3，R）**：write_set `gmcmd.rs` + quest flag 存取（宿主已有 quest flag 面则只加命令体）。oracle：`SetPassWordCommand.cs` 内 `SetFlagCommand/SetOpenCommand/SetUnitCommand`。验收牙：标志写/读回执绿字 ON/OFF + 「写的是调用者」怪癖断言 + target 离线零帧（NRE 被吞）。tier=R 理由：标志位随存档落库（字段级 diff 面）。
- **族 M（6，R）**：write_set `gmcmd.rs` + 世界刷怪/背包通道（`RegenMonsterByName`、`CopyToUserItemFromName`/`SendAddItem` 宿主已有能力）。oracle：`MobNpcCommand.cs/MobPlaceCommand.cs/ReCallMobExCommand.cs/MobCommand.cs/GiveMineCommand.cs/MakeItemCommand.cs`。验收牙：抽数序列（GiveMine 纯度 `Random(18)`、Make `Random(MakeRandomAddValue)`）、上限 64、四门逐支、缺参越界零帧怪癖（ReCallMobEx/GiveMine 的 `Length>0` 守卫）。forbidden：不「修」oracle 注释掉的升级/日志面。
- **族 C（5，R）**：write_set `gmcmd.rs` + 城堡通道（`castle.rs` 已有 UnderWar/TotalGold 面）。oracle：`ContestPointCommand.cs/ShowSbkGoldCommand.cs/ChangeSabukLordCommand.cs/ForcedWallconquestWarCommand.cs/StartContestCommand.cs`。验收牙：城堡不存在红字、TotalGold 三操作符（'-' 恒减 1）、越界怪癖（`sCtr[1]`/`bool.Parse`）零帧、广播帧序。SS_204/211/212 单服登记不实现（沿用设计件 G5 口径）。
- **族 W（4，R）**：write_set `gmcmd.rs` + 行会通道（`guild.rs` add/del/del_member 已有）+ `AllowGroupReCall` 落库列（`playerdata.rs` 已有）。oracle：`AddGuildCommand.cs/DelGuildCommand.cs/EndGuildCommand.cs/AllowGroupReCallCommand.cs`。验收牙：ServerIndex 门红字、SS_205/206/207 登记、掌门拒退红字、toggle 双绿字 + 落库字段 diff（窗口项）。
- **族 X1（5，L）**：write_set `gmcmd.rs` + 全局禁言名单容器（内存面）+ `WhisperHuman` 监听位。oracle：`DisableSendMsgCommand.cs/DisableSendMsgListCommand.cs/EnableSendMsgCommand.cs/ShutupListCommand.cs/ViewWhisperCommand.cs`。验收牙：`-jy` 恒红「没有被禁言!!!」、`+-jy` 空列表红/非空蓝头、`查看禁言` 恒绿「禁言列表为空!!!」、`查看私聊` 单字符名越界零帧。`SaveDisableSendMsgList` 落盘面登记（宿主未接 ⇒ log_note）。
- **族 X2（18，R）**：write_set `gmcmd.rs` + 目标玩家状态通道（Die/ReAlive/Permission/魔法列表/密码字段/全局 Mission/SpiritMutinyTick）。oracle：见主表逐行。验收牙：每条回执文本 + 真副作用面（`vv杀死` 目标死亡、`复活` HP=Max + RM_ABILITY、`VV给予权限` 运行时权限写、`删除技能` SendDelMagic + RecalcAbilitys）；纯文本子项（DelNpc/清除任务/清除怪物）恒字断言。forbidden：不给 `清除任务` 补真清零（oracle 就没有）。

## 6. 统计

- 解析：`CommandDef` 表项 186；`Imp::Unimplemented` **74**（名字互异；**盘点时点值**，落地进度见 §7）。
- 处置：**真缺口 65**（P7 + Q10 + T7 + F3 + M6 + C5 + W4 + X1 5 + X2 18）；**已对齐 9**；**垃圾命令 0**。
- tier 提案分布：L 族 4 个（P/Q/T/X1，共 29 条）；R 族 5 个（F/M/C/W/X2，共 36 条）。
- 证伪记录：卡面「垃圾命令 9 条」假设不成立——9 条混淆名全部映射到真实 oracle 类（§3），其中 6 条归密码锁门族、2 条师徒/配偶查询、1 条造物。

## 7. 计数订正（2026-10-30 落地进度）

本文 §1–§6 是**盘点时点**（基线 `26f88f483`）的快照，数字不随落地改写；本节记落地后的现存计数。

- **族 P 7 条已落地**：卡 `openmir2-gm-family-p-passwordlock`（worker-85，commit `6791fe991`，
  merge `e16572145`）——`Imp::PasswordLockGate`，部署 `PasswordLockSystem=0` 恒红字 + 零副作用牙。
- **族 T 7 条已落地**：卡 `openmir2-gm-family-t-toggle-horse`（worker-85，merge `0bdf71ad4`）——四条 toggle
  绿字往返 + `OffHorse`/`OnHorse` 的 `FeatureChanged` 外观帧（`SM_FEATURECHANGED(41)` 宿主落地）+
  `DisableFilter` 全局 `FilterWord` 翻转（`Cell` 全局单元注入）。
- **族 Q 10 条已落地**：卡 `openmir2-gm-family-q-query-display`（worker-85，本卡）——四怪癖逐条照抄
  （IP 判空恒真 / 信息在线零帧 / Map 越界零帧+图不存在与格子无效静默 / Showuseiteminfo 单字符名
  越界零帧+%s 格式串字面输出）+ LotteryTicket 8 配置值（部署全 0，格式串尾随空格照抄）+ 其余
  红绿回执逐字节；`showflag` 的目标 QuestFlag 位阵在宿主未建模 ⇒ 恒零值模型（seam
  `target_quest_flag_status`，在线恒走 OFF 支；族 F 落地时接真位阵后 ON 支可达）。
- **族 X1 5 条已落地**：卡 `openmir2-gm-family-x1-denywhisper`（worker-85，本卡）——`+jy`
  无条件入 `DisableSendMsgList` 内存面 + 绿字（FilterSendMsg 位与 SaveDisableSendMsgList 落盘面
  登记）；`+-jy` 空/非空两支恒字（非空只出蓝头=循环体注释）；`-jy` 恒红「没有被禁言!!!」；
  `查看禁言` 恒绿「禁言列表为空!!!」（DenySayMsgList 全仓无写点 ⇒ 恒空模型）；`查看私聊`
  单字符名越界零帧 + `listening_to` 镜像 toggle 双绿字。
- **现存 `Imp::Unimplemented` = 74 − 29 = 45**（= 机器台账 `gmcmd.rs::tests::batch2_table_wires_
  thirty_command_bodies` 的 140 已接 / 45 未接断言；改坏任一计数 ⇒ 该测试红）。
- 剩余缺口族：F3 / M6 / C5 / W4 / X2 18（§4.3 分族索引不变）。
