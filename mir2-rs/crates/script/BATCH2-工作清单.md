# 批次 2 工作清单：条件/动作求值与内建命令

来源：`script-tool parse-stats` 对 `E:\MirServer\M2GameSvr\Envir`（639 个 txt）的派发分类。
口径：解析器产出的 CmdCode 对普通命令是「字段序号-1」（= 枚举值-1，见 whitelist B-8），
执行侧注册表以枚举值为键 ⇒ 语料里出现的 CmdCode 落到哪一项，就要实现哪一项。

**结论：语料共用到 93 个有 C# 行为的派发目标**（下表），另有 797 次调用落在未注册项——
C# 对这些同样静默忽略（`ScriptEngine` switch 未命中即无行为），Rust 侧按"无操作"复刻即可，
清单天然收敛（验收③）。

## 已注册处理器（按语料调用次数降序）

### 条件处理器（28 项）

| # | 名称 | 语料调用次数 |
| --- | --- | --- |
| 1 | `ConditionOfCheckSlaveListCount` | 354 |
| 2 | `ConditionOfCheckBagGage` | 296 |
| 3 | `ConditionOfCheckRangeMonCount` | 120 |
| 4 | `ConditionOfCheckPoseGender` | 99 |
| 5 | `ConditionOfCheck` | 94 |
| 6 | `ConditionOfEqual` | 61 |
| 7 | `ConditionOfCheckMemBerLevel` | 47 |
| 8 | `ConditionOfCheckMapMonCount` | 41 |
| 9 | `ConditionOfCheckItem` | 36 |
| 10 | `ConditionOfCheckLevel` | 31 |
| 11 | `ConditionOfCheckIpList` | 28 |
| 12 | `ConditionOfCheckGameGold` | 24 |
| 13 | `ConditionCheckUnit` | 20 |
| 14 | `ConditionOfLapge` | 20 |
| 15 | `ConditionOfCheCkContAinsTextList` | 14 |
| 16 | `ConditionOfCheckBonusPoint` | 10 |
| 17 | `ConditionOfIssysop` | 8 |
| 18 | `ConditionOfCheckDura` | 7 |
| 19 | `ConditionOfRandom` | 5 |
| 20 | `ConditionOfCheckGuildList` | 2 |
| 21 | `ConditionOfCheckIsCastleMaster` | 2 |
| 22 | `ConditionOfCheckSkill` | 2 |
| 23 | `ConditionOfCheckCreditPoint` | 1 |
| 24 | `ConditionOfCheckMapName` | 1 |
| 25 | `ConditionOfCheckMarry` | 1 |
| 26 | `ConditionOfCheckPoseDir` | 1 |
| 27 | `ConditionOfCheckPoseLevel` | 1 |
| 28 | `ConditionOfCheckUseItem` | 1 |


### 动作处理器（51 项）

| # | 名称 | 语料调用次数 |
| --- | --- | --- |
| 1 | `ActionOfResetUnit` | 1425 |
| 2 | `ActionOfBreakTimereCall` | 1370 |
| 3 | `ActionOfSetMemberLevel` | 672 |
| 4 | `ActionOfAddAccountList` | 606 |
| 5 | `ActionOfMobFireBurn` | 585 |
| 6 | `ActionOfSet` | 531 |
| 7 | `ActionOfMovData` | 338 |
| 8 | `ActionOfLoadVar` | 228 |
| 9 | `ActionOfSaveVar` | 228 |
| 10 | `ActionOfDelSkill` | 205 |
| 11 | `ActionOfSetRankLevelName` | 166 |
| 12 | `ActionOfExeaction` | 139 |
| 13 | `ActionOfMission` | 111 |
| 14 | `ActionOfMonClear` | 105 |
| 15 | `ActionOfAutoGetExp` | 102 |
| 16 | `ActionOfVar` | 102 |
| 17 | `ActionOfRecallmob` | 85 |
| 18 | `ActionOfSetMapMode` | 82 |
| 19 | `ActionOfDelayCall` | 72 |
| 20 | `ActionOfGiveItem` | 71 |
| 21 | `ActionOfQueryItemDlg` | 59 |
| 22 | `ActionOfUnMaster` | 40 |
| 23 | `ActionOfUpgradeItems` | 34 |
| 24 | `ActionOfDivData` | 32 |
| 25 | `ActionOfMapMove` | 25 |
| 26 | `ActionOfChangePkPoint` | 18 |
| 27 | `ActionOfKill` | 13 |
| 28 | `ActionOfOffLine` | 12 |
| 29 | `ActionOfIncInteger` | 9 |
| 30 | `ActionOfHumanHp` | 6 |
| 31 | `ActionOfLineMsg` | 6 |
| 32 | `ActionOfSkillLevel` | 6 |
| 33 | `ActionOfTakeCastleGold` | 6 |
| 34 | `ActionOfKillSlaveName` | 5 |
| 35 | `ActionOfQueryTrustDeal` | 5 |
| 36 | `ActionOfChangeLevel` | 4 |
| 37 | `ActionOfDelUseDateList` | 4 |
| 38 | `ActionOfMarry` | 4 |
| 39 | `ActionOfChangeJob` | 3 |
| 40 | `ActionOfTimereCall` | 3 |
| 41 | `ActionOfAddNameList` | 2 |
| 42 | `ActionOfAddUseDateList` | 2 |
| 43 | `ActionOfChangePerMission` | 2 |
| 44 | `ActionOfClearMakeItems` | 2 |
| 45 | `ActionOfMessageBox` | 2 |
| 46 | `ActionOfMonGenEx` | 2 |
| 47 | `ActionOfUpgradeItemsEx` | 2 |
| 48 | `ActionOfBonusPoint` | 1 |
| 49 | `ActionOfClearPassword` | 1 |
| 50 | `ActionOfPercentData` | 1 |
| 51 | `ActionOfSetScriptFlag` | 1 |


## `ScriptEngine` switch 分支（无注册、由引擎控制流处理）

### 条件 switch（3 项）

| # | 名称 | 语料调用次数 |
| --- | --- | --- |
| 1 | `CHECKITEMW` | 102 |
| 2 | `CHECKINSAFEZONE` | 1 |
| 3 | `CHECKRANDOMNO` | 1 |


### 动作 switch（11 项）

| # | 名称 | 语料调用次数 |
| --- | --- | --- |
| 1 | `EndQuest` | 884 |
| 2 | `Take` | 586 |
| 3 | `AddBatch` | 252 |
| 4 | `BatchDelay` | 252 |
| 5 | `Takew` | 168 |
| 6 | `BatchMove` | 42 |
| 7 | `Param1` | 3 |
| 8 | `Param2` | 3 |
| 9 | `Param3` | 3 |
| 10 | `CheckUserDate` | 2 |
| 11 | `Break` | 1 |


## 未注册项（C# 无行为，复刻为无操作）

未注册（C# 静默忽略）合计: 797 次；条件 distinct code 40 / 动作 distinct code 67

（细节见每次运行的 stdout：`cargo run -p mir2-script-tool --release -- parse-stats <Envir>`。）

## 验收映射

| 验收项 | 判据 | 状态 |
| --- | --- | --- |
| ③ 未实现命令显式报错、清单收敛到 0 | 上表 93 项全部实现（或不适用时按 C# 语义复刻）；对拍时任何未实现项必须显式报错并列清单 | 待做 |
| ④ 商店/传送/修理/仓库/行会/任务用例 | 六类 NPC 流程各 ≥1 用例，副作用与 C# 一致（对拍脚本 + 双侧状态快照） | 待做 |
| ② 求值对拍 | 同脚本 + 同玩家初始状态下，两侧条件判定/动作副作用逐项一致 | 待做 |

## 实现顺序（按验收④的六类流程优先）

1. 控制流：`ScriptEngine.GotoLable` / 条件循环 / 动作循环 / `break` / `goto`（含位移语义）；
2. `ProcessingBase` 变量层（`$VAR` 解析、移动数据变量、动态变量、Set/Get）；
3. 六类流程用到的处理器（商店=Give/Take/MessageBox/Close、传送=MapMove/Map、修理=Repair 系、仓库=Storage 系、行会=Guild 系、任务=Mission/Quest 系）；
4. 其余处理器按语料次数降序补齐；
5. 每次补齐后跑一次全量对拍 + 六类用例，保持 ③ 清单为 0。
