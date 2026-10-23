# GM 命令可达性登记（88 条未实现命令）——2026-10-23（返工版）

- 卡：`openmir2-gm-reachability-registry`；本版为返工，处理 reviewer-1 `ad13dc87` 的
  **B1 / B2 / I1 / I2 / I3 / M1 / M2**。
- 工具：`mir2-rs/tests/parity/coverage/gm_registry.py`；登记表：`gm_registry.json`（`format=gm-registry/2`）。
- 口径：`mir2-rs/tests/parity/coverage/README.md`「GM 命令可达性登记」节。

> **一句话结论**：88 条未实现命令里 **8 条 `script-reachable`（现网真的能跑到，必须开实现卡）**、
> **80 条 `unreachable`**、**0 条 `reachable`（perm==0）**、**0 条 `structural-unreachable`**（v1 的「28 条结构性
> 不可达」不成立，已按 reviewer B2 修正）。与 reviewer B1 的差异：**9 条里 `复活` 的唯一 cite 是死 label**，
> 见 §5「与 review 的差异」。

## 1. 汇总（一条命令可复算）

```
$ python -I -B mir2-rs/tests/parity/coverage/gm_registry.py summary
GM_REGISTRY total=88 reachable=0 script-reachable=8 unreachable=80 structural-unreachable=0 undetermined=0
verdict_breakdown: {"script-reachable": 8, "unreachable": 80}
table_total=185 imp_breakdown={"Implemented": 97, "Unimplemented": 88} third_state=21 complete=76
crosscheck: {"extra_in_model": [], "missing_in_model": [], "model_total": 185, "ok": true,
             "perm_drift": [], "rust_total": 185}
script_occurrences: {"label_dead": 15, "label_reachable": 86, "total": 101,
                     "unloaded_files_hit": ["Market_Def/QFunction-0 - 副本.txt"]}
unknown_script_commands: ["+jww", "-jww", "去", "改变忠诚"]
script_reachable_but_implemented: []
```

| 维度 | 数值 |
|---|---:|
| 整表（`gmcmd.rs` `COMMANDS`） | 185 |
| 　├ 已实现（具名 `Imp`） | 97 = 完整 **76** + 第三态 **21** |
| 　└ 未实现（本表覆盖） | **88** |
| `script-reachable`（脚本 GMEXECUTE 抬权可达） | **8** |
| `unreachable`（权限门挡住，无脚本通道） | 80 |
| `reachable`（`perm_min==0`） | **0** |
| `structural-unreachable`（无命令体） | **0** |
| `undetermined` | 0 |

> `table_total` 的 97/88 是**具名 `Imp` 口径**；第三态 21 条 = `docs/rewrite-remaining-inventory-20261022.md`
> §2.1 的 awk 口径（T-161 的 14 + T-163 的 7），本工具按同一名单复算并与表内 `imp` 对齐（`third_state=21`）。
> **88 条未实现命令的 `perm_min` 全部为 10**（不存在 `perm==0` 的漏判，见 §6 抽验）。

## 2. 部署面（I1，必须显式声明）

**oracle = `E:/MirServer/M2GameSvr`，Release 构建**，证据（工具每次 build 现算并落进 `deployment`）：

1. `!deployed-from.json`：`build_dir = …\src\GameSrv\bin\Release`，`deployed_at = 2026-10-22 22:09:19`；
2. `GameSvr.dll` / `CommandSystem.dll` **不含** UTF-16LE 串「当前运行调试模式」
   （`src/Modules/GameCommand/GameCommand.cs:33-36` 的 `#if DEBUG` 分支才会写这个串）；
3. 反例：`E:/MirServer/Mir200/GameSvr.dll` **含**该 UTF-16LE 串 ⇒ Debug 构建。

> **若以 Debug 构建为对拍面**：`GameCommand.cs:33-36` 会把**每个调用者**的 `Permission` 直接置 10
> ⇒ 所有 `perm_min ≤ 10` 的命令对普通玩家全部可达 ⇒ **本表 80 条 `unreachable` 立即全部失效（改判 `reachable`）**。
> 该句写进 `deployment.debug_caveat`，`gate` 要求非空。

## 3. `script-reachable` 逐条（8 条，必须开实现卡）

通道：`ExecutionCode.cs:464` `[ScriptDefName("GMEXECUTE")]` → `ExecutionProcessingSys.cs:4925-4947`
`ActionOfGmExecute`：`playerActor.Permission = 10` → `ProcessUserLineMsg("@<cmd> …")` →
`CommandSystem.Execute` → `GameCommand.Handle`（`GameCommand.cs:37`，判门是 `Permission < PermissionMin`）。
⇒ `perm_min = 10` 的命令只要被**已加载**脚本在**可达 label** 里调用，Release 现网就真的会执行。

| 命令名 | CmdName 来源 | handler | perm | 证据（脚本 file:line / label） | 触发条件 |
|---|---|---|---:|---|---|
| `禁言`（ShutUp） | `Shutup`→`禁言` | `ShutupCommand.cs:11` | 10 | `QuestDiary/游戏登陆/登陆脚本.txt:16` `[@登陆设置]` | 登录：`PlayObject.cs:1440` `GotoLable("@Login")` → `QManage.txt:37` `#CALL … @登陆设置` → `#IF ISNEWHUMAN`（新角色登录即自动禁言 10000ms） |
| `取消禁言`（ShutupRelease） | `ReleaseShutup`→`取消禁言` | `ShutupReleaseCommand.cs:8` | 10 | `Market_Def/功能NPC/比武-GA2.txt:43/68/106` `[@AA]` | 玩家点「比武裁判」NPC → `[@main]` 的 `<给装备/@AA>` |
| `VV踢人`（Kick） | `Kick`→`VV踢人` | `KickHumanCommand.cs:10` | 10 | `Market_Def/功能NPC/桃源-N109.txt:49` `[@01]`；`QuestDiary/月卡/月卡系统.txt:28` `[@踢人]` | 桃源 NPC `[@main]` 的 `<封停人物/@01>`；月卡脚本经 `QManage:@Login` `#CALL` 载入 |
| `ShowEffect` | 类属性（未进 conf） | `ShowEffectCommand.cs:8` | 10 | 54 处；如 `Market_Def/QFunction-0.txt:104` `[@StdModeFunc14]`、`封魔谷/月老-M101.txt:118` `[@RequestMarry]` | 使用道具触发 `@StdModeFunc<AniCount>`（`PlayObject.Base.cs:1077`）；结婚请求/庆典等 NPC 对话 |
| `MobFireBurn` | 类属性（未进 conf） | `MobFireBurnCommand.cs:13` | 10 | `Market_Def/QFunction-0.txt:156/163` `[@StdModeFunc15]/[@StdModeFunc16]` | 使用对应 StdMode 道具 |
| `改属性`（Smake） | `SuperMake`→`改属性` | `SmakeItemCommand.cs:12` | 10 | `Npc_Def/比奇国王-0122.txt:71-126`（15 处）`[@QF]` | 玩家点「比奇国王」Quest NPC（`Npcs.txt:3` code=1）→ `@main` → `@rw02` → `@rw03` → `@QF` |
| `清理垃圾`（ClearBag） | `ClearBag`→`清理垃圾` | `ClearBagItemCommand.cs:9` | 10 | `Market_Def/LEGM2.CC假人NPC/假人上线NPC-jiaren.txt:19` `[@main]`；`功能NPC/桃源-N109.txt:87` `[@06]` | 点假人上线 NPC / 桃源 NPC 的 `<清空包裹/@06>` |
| `开始提问`（StartQuest） | `StartQuest`→`开始提问` | `StartQuestCommand.cs:6` | 10 | `QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt:40` `[@假人到指定地图A]` | `@main` → `[@假人设置页面]` → `[@下一页]` → `[@假人到指定地图]` 的 `QUERYVALUE … @假人到指定地图A`（玩家输入地图编号后跳转） |

> 上表 8 条在 Rust 侧**全部未实现**（`gmcmd.rs` 里都是 `Imp::Unimplemented`，这正是它们被登记在本表的原因）
> ⇒ 8 条都已开签名子卡 `openmir2-gm-impl-*`（tier=R，见 §9.1）。

> 每条命令的**全部**命中位置、label 调用链（`label_path`）、加载来源（`load_origin`）、脚本 sha256
> 都在 `gm_registry.json` 的 `commands[].script_evidence` / `sources.envir.script_sha256` 里，可逐条复算。

## 4. `unreachable` 80 条是怎么判的

判据不是「GM 不可达」一句话，而是三层机器判据同时成立：

1. **命令体存在**：按 `GameCmdConf` 字段键 + `[RegisterCommand]` 推导出 `[Command]` 处理类（88/88 全部有，
   ⇒ `structural-unreachable = 0`）；
2. **`perm_min = 10 > 0`**：现网 `StartPermission=0`（`Server.conf:365`）、`GetHumPermission` 被注释
   （`PlayObject.cs:1188-1194`）、`AdminList` 无有效 IP ⇒ 普通玩家 `Permission` 恒 0，过不了 `GameCommand.cs:37`；
3. **部署脚本里没有该命令的可达调用**：扫 11 个已加载脚本里的 101 处 `GMEXECUTE`，实参解析后
   **没有一个**落在这 80 条上（`+jww`/`-jww`/`去`/`改变忠诚` 在 oracle 里本身就是「未知命令」，见 §5.4）。

> 注意：这一桶的结论**依赖部署构型是 Release**（§2）。Debug 面下全部翻为 reachable。

## 5. 与 review（`ad13dc87`）的差异与订正

工具不采信任何 hand-written 结论：以上所有判定都由 `build()` 现算。复核 review 的逐条 cite 后发现
**两处 cite 指向死 label**，已在 `gm_registry.json.review_corrections` 落盘。

### 5.1 `复活`（ReAlive）：review 的唯一 cite 是死代码

review B1 写「`复活`（`QFunction-0.txt:63`）」。该行所在 label 是 `[@PlayDie]`：

- **没有任何代码派发 `@PlayDie`**：`src/` 里全部 `GotoLable(...)` 的标签已枚举（工具现算，
  `sources.engine_label_roots`），唯一的死亡事件派发是 `PlayObject.Message.cs:1638` 的 `"@OnDeath"`；
  `QFunction-0.txt` 既没有 `[@OnDeath]`，也没有任何文件引用 `@PlayDie`（全 Envir 递归扫描 0 处
  hyperlink / goto / DelayCall / QUERYVALUE / `#CALL`，`incoming=0`）。
- `ScriptFlagConst.cs:180` 的 `SPLAYDIE="@PLAYDIE"` **只有定义、全仓库无引用** ⇒ 没有任何代码能拼出 `@PlayDie`。

⇒ `复活` 在本表维持 **`unreachable`**，review 的这条 cite 需作废。该命令若将来要有别的可达通道，
必须给出与上面同等的证据（file:line + 调用链），不能凭「qfunction 里有」推断。

### 5.2 `取消禁言`：review 的 cite 也是死 label，但结论仍成立

review 写 `QFunction-0.txt:6` 的 `[@PlayLevelUp]` —— 同样是死代码（`PlayObject.cs:5250` 的
`GotoLable(this, "@LevelUp")` 被注释，且标签名也不同）。但 `取消禁言` 在
`Market_Def/功能NPC/比武-GA2.txt:43/68/106` 的 `[@AA]`（NPC 超链接，`@main` 根）有 3 处调用
⇒ **verdict 不变（`script-reachable`），依据换成比武-GA2**。

### 5.3 B2 采纳：结构性桶 = 0

v1 的「28 条结构性不可达」全部不成立。本工具：

- 从 `GameCmdConf.LoadConfig` 按**字段键**取 conf，不再拿 conf 键当 CmdName（`StorageLock`→字段 `Lock`→
  `SetPassWordCommand.cs:177` 的 `[Command("Lock")]`，最终名 `fdgfdg56`）；
- handler 由字段的 `[RegisterCommand(typeof(T))]` 定位 T 的 `[Command]` 行；
- 修正 `Map` / `MapInfo` 对调：`Map → MapInfoCommand.cs:8`、`MapInfo → ShowMapInfoCommand.cs:6`（牙 M 专测）；
- **自校验**：推导出的 185 条名字与 perm 与 `gmcmd.rs` 的 `COMMANDS` **完全一致**
  （`crosscheck.ok=true`，`missing_in_model=[]`、`extra_in_model=[]`、`perm_drift=[]`）。

### 5.4 脚本里 4 个「未知命令」：oracle 本来就是 no-op，不需要实现

`Command.conf` 的命令键拼写是 `DenyCharNameLogon` / `DelDenyCharNameLogon` / `ShowDenyCharNameLogon`
（**Char**），而 `GameCmdConf.cs:327/333/339` 读的是 `DenyChrNameLogon` / `DelDenyChrNameLogon` /
`ShowDenyChrNameLogon`（**Chr**）⇒ 这 6 条 conf 改名是**死键**，最终命令名退回构造默认值
（`DenyChrNameLogon` 等）。所以脚本里的 `+jww` / `-jww` 在 oracle 里是「未知命令」；
`去` / `改变忠诚` 更不在 185 条表里。⇒ 登记为 `unknown_script_commands`，**不开实现卡**（Rust 什么都不做才等价）。

### 5.5 其余死 label（逐条在 `dead_gmexecute_labels`）

| 文件:label | 死因 | 牵连的 GMEXECUTE |
|---|---|---|
| `QFunction-0.txt [@PlayDie]` | 无引擎派发、无入边 | 复活 |
| `QFunction-0.txt [@PlayLevelUp]` | `HasLevelUp` 里的 GotoLable 被注释 | 取消禁言（另有活路） |
| `QFunction-0.txt [@OnLogout_OffLinePlaying]` | 无引擎派发、无入边 | VV管理模式（未知命令） |
| `QFunction-0.txt [@假人技能]` | 仅被上面的死 label 用 DelayCall 引用 | TestStatus（第三态、已实现；不影响本表） |
| `QFunction-0.txt [@UserCmd1..9]` | `UserCmdCommand.cs` 的派发整段被注释 | 本身不含 GMEXECUTE（故不出现在 `dead_gmexecute_labels`），但其 `#CALL`/超链接子树（`代练.txt`、`@OffLinePlay`、`@一/@二/@三`）里的 GMEXECUTE 全部计为不可达 |
| `代练.txt [@LQ01..03]` | 只由死 label `[@UserCmd5]` 的 `#CALL` 进入 | ShowEffect（另有活路） |

> 死 label 上的 GMEXECUTE **不算证据**，但**没有删除**——它们仍在 `script_occurrences` 里以
> `label_reachable=false` 全量列出，便于将来 oracle 补回事件派发时立刻重算。

## 6. 抽验（26 条，跨四种机制）

四种机制：① `script-reachable`；② conf 改名（字段键 ≠ CmdName）；③ 纯类属性（未进 conf）；④ 纯 `conf[Permission]`。
`reachable` / `structural-unreachable` 两桶为空，故无法从中抽样——它们的「空」由 §1 的
`crosscheck`（185 条全对上）与「88/88 都有 handler」两条机器判据保证，而不是抽样。

| 命令名 | GameCmdConf 的 CmdName 来源 | handler `file:line` | PermissionMin | 判定 | 证据 |
|---|---|---|---:|---|---|
| `MobFireBurn` | 类属性 `MobFireBurn`（未进 conf） | `src/Modules/GameCommand/Commands/MobFireBurnCommand.cs:13` | 10（cs[Command] 属性） | **script-reachable** | `Market_Def/QFunction-0.txt:156` `@stdmodefunc15` |
| `ShowEffect` | 类属性 `ShowEffect`（未进 conf） | `src/Modules/GameCommand/Commands/ShowEffectCommand.cs:8` | 10（cs[Command] 属性） | **script-reachable** | `Market_Def/QFunction-0.txt:104` `@stdmodefunc14` |
| `VV踢人` | `Kick`→`VV踢人` | `src/Modules/GameCommand/Commands/KickHumanCommand.cs:10` | 10（conf[Permission].Kick） | **script-reachable** | `Market_Def/功能NPC/桃源-N109.txt:49` `@01` |
| `取消禁言` | `ReleaseShutup`→`取消禁言` | `src/Modules/GameCommand/Commands/ShutupReleaseCommand.cs:8` | 10（conf[Permission].ReleaseShutup） | **script-reachable** | `Market_Def/功能NPC/比武-GA2.txt:43` `@aa` |
| `开始提问` | `StartQuest`→`开始提问` | `src/Modules/GameCommand/Commands/StartQuestCommand.cs:6` | 10（conf[Permission].StartQuest） | **script-reachable** | `QuestDiary/LEGM2.CC假人系统/假人登录脚本.txt:40` `@假人到指定地图a` |
| `改属性` | `SuperMake`→`改属性` | `src/Modules/GameCommand/Commands/SmakeItemCommand.cs:12` | 10（conf[Permission].SuperMake） | **script-reachable** | `Npc_Def/比奇国王-0122.txt:71` `@qf` |
| `清理垃圾` | `ClearBag`→`清理垃圾` | `src/Modules/GameCommand/Commands/ClearBagItemCommand.cs:9` | 10（conf[Permission].ClearBag） | **script-reachable** | `Market_Def/LEGM2.CC假人NPC/假人上线NPC-jiaren.txt:19` `@main` |
| `禁言` | `Shutup`→`禁言` | `src/Modules/GameCommand/Commands/ShutupCommand.cs:11` | 10（conf[Permission].Shutup） | **script-reachable** | `QuestDiary/游戏登陆/登陆脚本.txt:16` `@登陆设置` |
| `fdgfdg56` | `StorageLock`→`fdgfdg56` | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs:177` | 10（GameCommands.cs 构造默认） | unreachable | — |
| `Map` | `Map`→`Map` | `src/Modules/GameCommand/Commands/MapInfoCommand.cs:8` | 10（conf[Permission].Map） | unreachable | — |
| `MapInfo` | `MapInfo`→`MapInfo` | `src/Modules/GameCommand/Commands/ShowMapInfoCommand.cs:6` | 10（conf[Permission].MapInfo） | unreachable | — |
| `IP` | `HumanLocal`→`IP` | `src/Modules/GameCommand/Commands/HumanLocalCommand.cs:10` | 10（conf[Permission].HumanLocal） | unreachable | — |
| `who` | `Who`→`who` | `src/Modules/GameCommand/Commands/WhoCommand.cs:8` | 10（conf[Permission].Who） | unreachable | — |
| `setflag` | `SetFlag`→`setflag` | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs:212` | 10（conf[Permission].SetFlag） | unreachable | — |
| `fdht7ed` | `Master`→`fdht7ed` | `src/Modules/GameCommand/Commands/SearchMasterCommand.cs:10` | 10（GameCommands.cs 构造默认） | unreachable | — |
| `hjhgj67` | `StorageSetPassword`→`hjhgj67` | `src/Modules/GameCommand/Commands/SetPassWordCommand.cs:11` | 10（GameCommands.cs 构造默认） | unreachable | — |
| `Announcement` | `Announcement`→`Announcement` | `src/Modules/GameCommand/Commands/AnnouncementCommand.cs:8` | 10（conf[Permission].Announcement） | unreachable | — |
| `DelNpc` | `DelNpc`→`DelNpc` | `src/Modules/GameCommand/Commands/DelNpcCommand.cs:9` | 10（conf[Permission].DelNpc） | unreachable | — |
| `ContestPoint` | `ContestPoint`→`ContestPoint` | `src/Modules/GameCommand/Commands/ContestPointCommand.cs:10` | 10（conf[Permission].ContestPoint） | unreachable | — |
| `ClearCopyItem` | 类属性 `ClearCopyItem`（未进 conf） | `src/Modules/GameCommand/Commands/ClearCopyItemCommand.cs:11` | 10（cs[Command] 属性） | unreachable | — |
| `GiveMine` | 类属性 `GiveMine`（未进 conf） | `src/Modules/GameCommand/Commands/GiveMineCommand.cs:12` | 10（cs[Command] 属性） | unreachable | — |
| `vv杀死` | `Kill`→`vv杀死` | `src/Modules/GameCommand/Commands/KillCommand.cs:10` | 10（conf[Permission].Kill） | unreachable | — |
| `MobPlace` | `MobPlace`→`MobPlace` | `src/Modules/GameCommand/Commands/MobPlaceCommand.cs:11` | 10（conf[Permission].MobPlace） | unreachable | — |
| `清除怪物` | `ClearMon`→`清除怪物` | `src/Modules/GameCommand/Commands/ClearMapMonsterCommand.cs:10` | 10（conf[Permission].ClearMon） | unreachable | — |
| `是我` | `MemberFunc`→`是我` | `src/Modules/GameCommand/Commands/MemberFunctionCommand.cs:6` | 10（GameCommands.cs 构造默认） | unreachable | — |
| `删除技能` | `DeleteSkill`→`删除技能` | `src/Modules/GameCommand/Commands/DelSkillCommand.cs:11` | 10（conf[Permission].DeleteSkill） | unreachable | — |

> 说明：`hjhgj67` 这类「conf 键与 CmdName 完全不同」的条目是 B2 的典型陷阱——
> `StorageSetPassword` 是**字段键**，最终命令名是 `hjhgj67`，handler 是 `SetPassWordCommand.cs:11` 的
> `[Command("SetPassWord")]`（被 `RegisterCommandGroups` 改名）。v1 正是在这里把命令判成「无命令体」。

## 7. 门禁与牙

```
$ python -I -B mir2-rs/tests/parity/coverage/gm_registry.py gate
GREEN: gm_registry 88 条与源码逐字段一致（handler/perm/verdict/脚本证据全部复算通过）   (rc=0)

$ python -I -B mir2-rs/tests/parity/coverage/gm_registry.py selftest
  [ok] A 在界指错命令 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] B perm 10→8 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] C 改 cmd 名 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] D register_command=true + handler=null ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] E perm=null + unreachable ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] F perm_min=0 却判 unreachable ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] G handler 行号越界 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] H script-reachable 无证据 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] I 伪造脚本证据（该行不是 GMEXECUTE）⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] J 把 conf 键当 CmdName（B2 原缺陷）⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] K 证据指向未加载的副本脚本 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
  [ok] L StartPermission=1/10 重算：结构性行不被翻、perm=10 不被 sp=1 翻  [sha256 366fa4109e79b3c0 未变]
  [ok] M Map / MapInfo handler 未对调  [sha256 366fa4109e79b3c0 未变]
  [ok] N script-reachable 未链接子卡 ⇒ gate 红  [sha256 366fa4109e79b3c0 未变]
gm_registry selftest: OK (14 teeth; registry sha256=366fa4109e79b3c0 未变)
```

- **A–E 五颗牙正是 review I2 自造的五种失败模式**（N 是新加的「script-reachable 必须链接子卡」）（自洽却能穿过 v1 门禁），本版逐个变红。
- 每颗牙执行后都校验登记表 sha256 未变；`selftest` 末尾再断言 `render(base) == 磁盘字节` ⇒ **逐字节还原**。
- **M1**：`build` 以 utf-8 + LF 写出，`gate` 额外校验「登记表是本工具的规范序列化」⇒ build 后工作区不再有 diff。

## 8. 仓库门禁（本卡零 `.rs` 改动）

见交付 patch body 与 §10 的实测输出（`cargo fmt` / `clippy -D warnings` / `msgcodegen --check` /
anchor gate / registry audit / T 预约表 gate）。

## 9. 结论

### 9.1 开出的子卡（script-reachable 且 Rust 未实现 ⇒ 每条一张）

| 命令 | 子卡 id |
|---|---|
| `禁言` | `openmir2-gm-impl-shutup` |
| `取消禁言` | `openmir2-gm-impl-shutup-release` |
| `VV踢人` | `openmir2-gm-impl-kick` |
| `ShowEffect` | `openmir2-gm-impl-showeffect` |
| `MobFireBurn` | `openmir2-gm-impl-mobfireburn` |
| `改属性` | `openmir2-gm-impl-smake` |
| `清理垃圾` | `openmir2-gm-impl-clearbag` |
| `开始提问` | `openmir2-gm-impl-startquest` |


1. **8 条命令在 Release 现网真的可达**（脚本 GMEXECUTE 抬权）⇒ 已逐条开子卡，不能再用「GM 不可达」打包。
2. **80 条不可达**有逐条静态证据：有命令体、`perm=10`、部署脚本 101 处 GMEXECUTE 无一条命中。
3. **v1 的「结构性不可达 28 条」撤回**：结构性桶 = 0，`Map`/`MapInfo` handler 已修正。
4. **review B1 的 9 条里 `复活` 需降级为不可达**（cite 是死 label），`取消禁言` 依据换为比武-GA2；
   其余 8 条成立。

## 10. 残留物清单

- `gm_registry.json` 由 `build` 生成，**入库**（登记表本身是交付物）。
- 临时脚本 `E:/tmp/w58/proto.py`、`E:/tmp/w58/scriptproto.py` 为返工期间的探索脚本，**未入库**，随本批结束删除。
- 未起栈、未占端口、未连库。
