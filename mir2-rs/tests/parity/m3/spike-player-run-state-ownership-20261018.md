# PR-3 spike：玩家 `Run` 散段状态归属与 tick 驱动点

- spike 日期：2026-10-18；代码基线 `32c72c120`（detached worktree；订正件同样只读该基线）。
- 性质：**只读 spike / 纯文档**。未改 `.rs` / `src/**` / `sql/**` / `MirClient/` / `crates/{protocol,storage}/**` / `E:/MirServer/**`；未起服务、未占端口、未写库。部署内容只做 `grep -a` 只读核对（九游戏端口仍由外部 C# 栈占用，本件不触碰）。
- 上游问题：`mir2-rs/tests/parity/m3/plan-run-gap-batches-20261018.md` §3 `PR-3`——P21/P22/P25/P26/P27/P31/P32 与 P07 结论所需字段，归 `SessionGround`（宿主）还是 `Entity`（world），每段由谁在 tick 的哪个位置驱动。
- 行号纪律：oracle（本仓 `src/**` C#）行号逐条 `sed -n` 实测，§7 给复现命令；Rust 侧只写符号 + 文件路径，不写行号（并行车道会位移）。
- 判级：本件 L（文档）。其结论服务 R 级实现批次 PL-1..PL-4。

---

## 0. 结论摘要

1. **可以开工 PL-1..PL-4**，但 **PL-1 的第一个 commit 必须先落 `World` 的 tick 切分 + 玩家死亡结算后移**（见 §4.3）。原因是 P32 复活戒指的门必须在 oracle `base.Run()` 的 `HP == 0 ⇒ Die()` 之前、且在 P25/P26 可能把 HP 打到 0 之后执行；当前 Rust 把 `settle_deaths` 放在 tick 开头，会让 P32 永远来不及救。
2. **字段归属一句话**：会话/DB/脚本/帧面状态留 `SessionGround`（`game_gold`、`game_point`、`hunger_status`、完整 `use_items`、`Recovery`、`Stamina`、`AutoGetExp*`、P22/P25/P26/P32 的计时与布尔）；**被 world 的战斗/经验/死亡直接读取的数值进 `Entity`**（`hp/mp/max_hp/max_mp`、`power_rate`，新增 per-player `kill_mon_exp_rate` 与其 Time、`exp_rate_tick`（后者的时钟域仍是宿主 `clock::now_ms()`，放 Entity 只为与 rate 同段）；`revival/revival_tick` 本 spike 建议留宿主，见 §3.3）。跨层只允许**单向投影**，不允许两边各写各的。
3. **P07 已确认是死代码**（本 oracle 树 + 本部署内容）：`AutoTimerStatus` 除 ctor 零初始化外无写入者；`SETONTIMER` 只有枚举声明、无 handler；部署 `Envir` 使用文件数 = 0。⇒ 等价实现 = **不实现**，PL-2 实际只剩 P31。
4. **三个此前规划件没有写全的实测事实**（会直接改变实现写法）：
   - `AUTOADDGAMEGOLD` / `AUTOSUBGAMEGOLD` 两个 oracle 脚本动作**从不解析参数**（`nPoint`/`nTime` 恒 0）⇒ `START` 分支永远进不去，只能 `STOP`；因此 P22 的玩家级 `Bo{Inc,Dec}GameGold` 在当前 oracle 里**不可被脚本打开**，只剩地图 Flag 支可达（而部署 `MapInfo.txt` 0 条）。
   - oracle `RecalcAbilitys()` **不重置 `Revival`**（只重置 `RecoveryRing`/`AngryRing`/…）⇒ `Revival` 是**粘滞锁存**：装上复活戒指后，卸下、戒指打碎、死亡都不会把它清回 false，只有重新登录（新对象 + 重新 Recalc）才重算。P32 必须照抄这个 bug。
   - `characters.Gold` 一列在 oracle 同时被读成 `BaseObject.Gold` 与 `PlayObject.GameGold`，但写回只写 `@Gold = hd.Gold` ⇒ **GameGold 的改动不落库**；Rust `playdata.rs` 已照抄。P22 不得“顺手修好”这条。
5. **`tick_tail` 方案已裁决（orchestrator，2026-10-18，依据硬约束③行为等价）——接受**：玩家死亡结算从 tick 开头移到 `P32` 之后（`VerifyMapTime`/AOI 同步入 tail），与 oracle `PlayObject.Run`（P21→…→P32→`base.Run` 的 Die→VerifyMapTime）同序；替代方案「P32 延后一 tick」差 200ms 且改变救活语义，已否决。对既有 golden 的影响：现有 world golden 的 `death` 行全部是怪物（`combat*.txt` 的幽灵/僵尸），**没有玩家死亡场景 ⇒ 不改任何既有 golden**；仅玩家死亡场景的 `record.deaths`/退组/自投递相对位置按新 tail 序变化（详见 §4.3/§6.2）。

---

## 1. oracle 事实

### 1.1 `PlayObject.Run` 总序（实测）

`Run()` 本体：`src/M2Server/Player/PlayObject.Message.cs:19-766`。与本次相关的段：

| 序 | oracle 行（实测） | 段 | 本件范围 |
|---:|---|---|---|
| 1 | 54-64 | `FireHitSkill` 20s / `TwinHitSkill` 60s 到期 | 已落地（`expire_player_run_state`） |
| 2 | 70-142 | `IncHealth/IncSpell/IncHealing` 周期恢复 | 已落地（宿主 `recover`） |
| 3 | 143-157 | P07 个人定时器 `@OnTimer0..19` | **P07** |
| 4 | 158-227 | `ExtraAbil` 到期 + 状态位（含 `boNeedRecalc ⇒ HealthSpellChanged`） | 已落地（`expire_extra_abil`） |
| 5 | 257-262 | `DecLightItemDrugTime` / `CheckPkStatus` | 已落地（`check_pk_status`） |
| 6 | 263-285 | `CheckDupObj` 3s 重叠推挤 | 已落地（`check_dup_obj`） |
| 7 | 356-400 | `GetMessage` 消息泵 + `Operate` | world 命令 FIFO / 宿主收包 |
| 8 | 403-441 | **P21** 装备需求每 Run 复检 | PL-1 |
| 9 | 442-529 | **P22** `GameGold` 自动增减 + 地图 Flag | PL-4 |
| 10 | 530-547 | `Fight3Zone` 死亡计数 | 不在本批 |
| 11 | 548-568 | P24 `boINCGAMEPOINT` 游戏点 | 不在 PL-1..4（L 级，见 §3.2） |
| 12 | 569-594 | **P25** 地图 `DECHP/INCHP` | PL-3 |
| 13 | 596-635 | **P26** 饥饿系统 | PL-3 |
| 14 | 636-657 | **P27** 经验/攻击倍率 1s 递减 | PL-3 |
| 15 | 658-668 | `ReNewChangeColor` | 不在本批 |
| 16 | 669-676 | `WhisperHuman` 失效检测 | 不在本批 |
| 17 | 677-737 | `ClearInvalidObj`（30s：Dear/Master/Group/Deal） | 不在本批 |
| 18 | 738-745 | **P31** `SETAUTOGETEXP` 结算 | PL-2 |
| 19 | 747-763 | **P32** 复活戒指 | PL-1 |
| 20 | 765 | `base.Run()`（含 `WAbil.HP == 0 ⇒ Die()`、命中者清理、`VerifyMapTime`） | 拆在 `BaseObject.Base.cs:25-316` |

`base.Run()` 的关键子序（实测 `src/M2Server/Actor/BaseObject.Base.cs`）：
- `:70-73` `if (WAbil.HP == 0) Die();`；
- `:105-110` `HealthTick < -HealthFillTime && HP > 1` ⇒ 扣 1 HP；
- `:119-132` `LastHiter`（30s / 死亡 / Ghost）与 `ExpHitter`（6s / 死亡 / Ghost）清理；
- `:208-217` 宝宝列表清理；
- `:219-222` `ShowHp` 周期 `BreakOpenHealth()`；
- `:223-228` `Envir.VerifyMapTime(...)`；
- `:230-245` `WAbil.HP > MaxHP ⇒ MaxHP-1`、`MP > MaxMP ⇒ MaxMP-1` + `HealthSpellChanged()`（**Rust 的 `verify_map_time` 只建模了 `:223-228`，钳制段未建模，属既有缺口，PL-1 切 `tick_tail` 时应一并登记/接线**）。

⇒ **P32 必须早于 `:70` 的 Die、晚于 P25/P26 可能的 HP=0；`VerifyMapTime` 晚于 Die。** 这三条是 §4.3 tick 切分的 oracle 依据。

### 1.2 逐段字段读写（oracle 行号逐条实测）

#### P21 `PlayObject.Message.cs:403-441`

- 读：`UseItems[0..12]`（`Index`）、`SystemShare.ItemSystem.GetStdItem`（`StdItem.*`）、`CheckItemsNeed`（`PlayObject.Base.cs:2832-2882`）读 `MyGuild`/`GuildRankNo`/`CastleMgr.IsCastleMember`/`MemberType`/`MemberLevel`/`NeedLevel`。
- 写：`ItemList`（`AddItemToBag`，`BaseObject.cs:1241-1251`）、`WAbil.Weight`（`WeightChanged`）、地面 `MapItem`（`DropItemDown`，`BaseObject.cs:529-583`）、`UseItems[i].Index = 0`、`RecalcAbilitys()`（`PlayObject.cs:2008-…`，改 `WAbil`/`AddAbil`/`SubAbility`/`HitSpeed` 以及 shape 锁存）。
- 帧：`SendAddItem`、`SendDelItems`、`WeightChanged`、52/752（Recalc 内）。
- `CheckItemsNeed` 的**精确子集**（`PlayObject.Base.cs:2832-2882` 实测）：`Need=6` 无行会 false；`60` 无行会或 rank≠1 false；`7` 非城堡成员 false；`70` 非城堡成员或 rank≠1 false；`8` `MemberType==0` false；`81` `MemberType != LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel)` false；`82` `MemberType < LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel)` false；**其余 Need 一律 true**。
- 注意：`StdItem.NeedLevel` 在 `StdItem` 里是 **byte**，`LoWord/HiWord(byte)` 经 int 提升后 `Hi` 恒 0（Rust `bag.rs` 的注释已实测）；不要按 `short` 处理。

#### P22 `PlayObject.Message.cs:442-529`

- 读/写 `GameGold`（`PlayObject.cs:763`，ctor `:964`）。
- 读/写 `BoDecGameGold`/`DecGameGold`/`DecGameGoldTime`/`DecGameGoldTick`（声明 `PlayObject.cs:767-770`；ctor `:965-968`）。
- 读/写 `BoIncGameGold`/`IncGameGold`/`IncGameGoldTime`/`IncGameGoldTick`（`:772-776`；ctor `:970-973`）。
- 读地图：`Envir.Flag.boDECGAMEGOLD`/`nDECGAMEGOLD`/`nDECGAMEGOLDTIME`、`boINCGAMEGOLD`/`nINCGAMEGOLD`/`nINCGAMEGOLDTIME`（`:482-525`）。
- 写：`MoveToHome()`（`PlayObject.Base.cs:2306-2310` → `BaseObjectMove` `:2312-2333`；`SpaceMove` 或 `MapRandomMove`，跨图且 `Race==Play` 清 `IsTimeRecall`）。
- 帧：`tObjCount != GameGold` ⇒ `SendUpdateMsg(RM_GOLDCHANGED,…)`（`:526-529`）。
- 日志：`M2Share.GameLogGameGold` 两条 `AddEventLog` 都已注释（`:458-461`/`:477-480`），无帧影响。
- 上界：`Inc` 支 `GameGold + IncGameGold < 2_000_000` 才加，否则封顶 `2_000_000` 且 `BoIncGameGold=false`；`nInteger = 2000000 - GameGold` 在赋值**之后**算 ⇒ 恒 0（日志已注释，不影响帧）。地图 `Inc` 支用 `<= 2_000_000`（`:510`）。
- **实测缺陷**：`ActionOfAutoAddGameGold`（`ExecutionProcessingSys.cs:2667-2688`）与 `ActionOfAutoSubGameGold`（`:2733-2754`）里 `nPoint`/`nTime` 初始化为 0 后**从未解析参数** ⇒ `if (nPoint > 0 && nTime > 0)` 恒 false，`START` 永远走 `ScriptActionError`；只有 `STOP` 生效。部署 `Envir` 两个动作的使用文件数均为 0。⇒ 玩家级 `BoInc/BoDecGameGold` 在当前 oracle + 内容下不可达；地图 Flag 支也因 `MapInfo.txt` 0 条而不可达。
- P24（`548-568`，`GamePoint`）不在 PL-4；但它与 P22 共用 `IncGamePointTick`/`GamePoint` 字段，插入点见 §4.2。

#### P25 `PlayObject.Message.cs:569-594`

- 读：`Envir.Flag.boDECHP`/`nDECHPTIME`/`nDECHPPOINT`、`boINCHP`/`nINCHPTIME`/`nINCHPPOINT`；`WAbil.HP`/`MaxHP`。
- 读/写：`DecHpTick`（声明 `PlayObject.cs:783`，ctor **未初始化** ⇒ 0）、`IncHpTick`（`:784`，同样 0）。
- 写：`WAbil.HP`。
- 帧：`HealthSpellChanged()` ⇒ `SendUpdateMsg(RM_HEALTHSPELLCHANGED)`（`BaseObject.cs:700-710`），仅玩家 + `ShowHp` 广播。
- 实测 bug：`INCHP` 支用 `nDECHPPOINT` 判上限并加血（`:585-587`），不是 `nINCHPPOINT`；照抄，禁止修正。

#### P26 `PlayObject.Message.cs:596-635`

- 读：`SystemShare.Config.HungerSystem`/`HungerDecHP`；`HungerStatus`；`GetMyStatus()`（`PlayObject.Base.cs:1183-1191`：`HungerStatus/1000` 上限 4）。
- 读/写：`DecHungerPointTick`（声明 `PlayObject.cs:296`，ctor `:887`）；`HealthTick`/`SpellTick`（`BaseObject.cs:287-288`）；`PerHealth`/`PerSpell`（`CharacterObject` 的 `byte`）。
- 写：`HungerStatus -= 1`；`RefMyStatus()`（`PlayObject.Base.cs:288-292`：**`RecalcAbilitys()` + `SendMsg(RM_MYSTATUS)`**）；`HealthTick -= 60`、`SpellTick = MAX(0, SpellTick-10)`、`PerHealth -= 1`、`PerSpell -= 1`、`WAbil.HP -= MAX(1, HP/100)`（正 HP 时 `HP > HP/100` 恒真；`HP <= 2 ⇒ 0` 支对正 HP 不可达）；`HealthSpellChanged()`。
- 注意：饥饿跨档（`GetMyStatus()` 变化）会触发**完整 RecalcAbilitys**，不只是发一条 8102；这条容易漏。

#### P27 `PlayObject.Message.cs:636-657`

- 读/写：`ExpRateTick`（声明 `PlayObject.cs:727`，ctor `:942`）；`KillMonExpRate`（`:720`，ctor `:941` = 100）；`KillMonExpRateTime`（`:725`，默认 0）；`PowerRate`（`:724`，ctor `:943` = 100）；`PowerRateTime`（`:726`，默认 0）。
- 写：Time 递减；归零时 `KillMonExpRate = 100` / `PowerRate = 100` + 两条 `SysMsg`（`MsgColor.Red, MsgType.Hint`）。
- 消费点：`KillMonExpRate` 在 `WinExp`（`PlayObject.Base.cs:129-147`，`:140` 乘 `KillMonExpRate/100.0`）；`PowerRate` 在攻方攻击力（`PlayObject.cs:2938`）。
- 写入点：脚本 `KILLMONEXPRATE`（`ExecutionProcessingSys.cs:5449-5464`，`:5458-5459` 写 rate/time）、`POWERRATE`（`:5539-5554`，`:5548-5549`）。

#### P31 `PlayObject.Message.cs:738-745`

- 读：`AutoGetExpPoint`/`AutoGetExpEnvir`/`AutoGetExpTime`/`AutoGetExpInSafeZone`（声明 `PlayObject.cs:804-808`；ctor `:1009-1012` 置 `tick=now`, `point=0`, `envir=null`）；`InSafeZone()`。
- 读/写：`AutoGetExpTick`。
- 写：`GetExp(AutoGetExpPoint)`（`PlayObject.Base.cs:149-166`：`Exp +=`、`AddBodyLuck`、`RM_WINEXP`、单步升级、`IncHealthSpell(2000,2000)`）。**口径写死**：直接 `Exp += n`，不乘任何倍率、不过 `CalcGetExp`、不分组队；Rust 侧**不得复用 `World::gain_exp`**（`CalcGetExp` / `KillMonExpMultiple` / `MNKillMonExpMultiple` / `KillMonExpRate` / 地图 `ExpRate` / 组队均分与加成全在它里面）。
- 脚本写入：`SETAUTOGETEXP`（`ExecutionProcessingSys.cs:2691-2711`；`:2698-2705` 空地图且 `FindMap("")==null` ⇒ `ScriptActionError`，不写字段；`:2707-2710` 正常写入）。
- 部署实测：唯一脚本行 `Envir/Market_Def/QFunction-0.txt:25` 是 `SETAUTOGETEXP 1400 1 1`（无第 4 参数）⇒ 走错误路径，字段保持 0。

#### P32 `PlayObject.Message.cs:747-763` + `PlayObject.cs:5355-5392`

- 门：`!Death`；`WAbil.HP == 0`；`LastHiter == null || (LastHiter.Race == ActorRace.Play && !UnRevival)`；`Race == ActorRace.Play && Revival && (now - RevivalTick) > Config.RevivalTime`。
- 写：`RevivalTick = now`；`ItemDamageRevivalRing()`；`WAbil.HP = WAbil.MaxHP`；`HealthSpellChanged()`；`SysMsg(RevivalRecoverMsg, Green, Hint)`。
- `ItemDamageRevivalRing`（`PlayObject.cs:5355-5392`）：
  - 逐槽 `UseItems[i]`（`Index > 0`），取 `StdItem`；
  - 命中条件：`M2Share.ItemDamageRevivalMap.Contains(Shape)`（`M2Share.cs:186` = `{114,160,161,162}`），或 `i ∈ {ItemLocation.Weapon, ItemLocation.RighThand}` 且 `ItemDamageRevivalMap.Contains(AniCount)`；
  - `ushort nDura = Dura; ushort tDura = Round(Dura/1000.0); nDura -= 1000;`
  - `nDura <= 0`（ushort，只有 `Dura == 1000` 会得到 0）⇒ `Dura=0`、`SendDelItems`（仅玩家）、`Index=0`、`RecalcAbilitys()`；否则 `Dura = nDura`（`Dura=0 ⇒ 64536`、`999 ⇒ 65535`，仍留装备位）；
  - `tDura != Round(nDura/1000.0)` ⇒ `SendMsg(RM_DURACHANGE, i, nDura, DuraMax, 0)`。
- 关键实测：`RecalcAbilitys()` 顶部（`PlayObject.cs:2008-2024`）只重置 `RecoveryRing/AngryRing/MagicShield/FastTrain/...`，**不重置 `Revival`**；`Revival = true` 只在 `:2242`（`RING_REVIVAL_ITEM`）写，`UnRevival` 在整棵 `src/` 树里**只有声明 `PlayObject.cs:506` 与读取 `Message.cs:751`，无任何写入者**。
  ⇒ `Revival` 是粘滞锁存（装过戒指后卸下/打碎/死亡仍为 true，直到重新登录重新 Recalc）；`UnRevival` 在当前树恒 false。两者都必须照抄。
- `Config.RevivalTime`：`GameSvrConf.cs:804` 声明、`:1725` 默认 60000；`ServerConf.cs:503` 读 `Setup/RevivalTime`；部署 `Server.conf:739` = 60000。

### 1.3 字段生命周期总表（登录建立 / 存档落库 / 死亡清 / 换图清）

> 「登录」= ctor 初值 + `WorldServer` 登录载入；「存档」= `WorldServer.MakeSaveRcd`（`:1231-1382`）+ DBSvr 写库；「死亡」= `PlayObject.Die()`（`:1964-1997`）/`BaseObject.Die()`（`BaseObject.Base.cs:318-…`）；「换图」= `EnterAnotherMap`（`:3718-…`）/`UserLogon`（`:1152-…`）里的显式复位。

| 字段 | 登录建立 | 存档落库 | 死亡 | 换图 |
|---|---|---|---|---|
| `GameGold` | ctor `:964` = 0；载入 `WorldServer.cs:1142`（源列 `characters.Gold`，`Query.cs:241`） | `MakeSaveRcd:1274`；但 DB `SaveRecord` 只绑 `@Gold = hd.Gold`（`Save.cs:102`，来自 `:1245` 的 `playerInfo.Gold`）⇒ **GameGold 不落库** | 不清 | 不清 |
| `GamePoint` | ctor `:974` = 0；载入 `:1143` | `MakeSaveRcd:1275`；DB `@GamePoint`（`Save.cs:103`） | 不清 | 值不清；`IncGamePointTick` 复位 `:3794` / `:1483` |
| `HungerStatus` | 声明 `PlayObject.cs:225`（默认 0）；载入 `WorldServer.cs:1155` | `MakeSaveRcd:1286`；DB `@HungerStatus`（`Save.cs:117`） | 不清 | 不清 |
| `UseItems[]` | `CharacterObject.cs:40/89`（13 槽）；载入 `WorldServer.cs:1164-1176` | `MakeSaveRcd:1299-1311`；DB `characters_item`（`Save.cs:203-204`） | 不清（Die 不碰装备） | 不清 |
| `Recovery.IncSpell/IncHealth/IncHealing` | ctor 归零（`recover`）；载入 `Query.cs:247-249` → `WorldServer.cs:1149-1151` | `MakeSaveRcd:1279-1281`；DB `@IncHealth/@IncSpell/@IncHealing`（`Save.cs:112-114`） | **清**：`PlayObject.Die()` 置 0（`PlayObject.cs:1970-1972`）；`PerHealth/PerSpell` **不清**（死亡不碰） | 不清 |
| `WAbil.HP/MP/MaxHP/MaxMP` | `BaseObject.cs:411` 新建；`AbilCopyToWAbil`；载入 `characters_ablity`（`Query.cs:268-296`，`GetAbilGetRecord`；HP/MP 读在 `:280-285`） | `MakeSaveRcd:1247-1259`；DB `characters_ablity`（`Save.cs:141`） | `Die()` 置 `Death`，HP 已是 0；不清 Max | 不清 |
| `PowerRate` | ctor `:943` = 100 | 不落库 | 不清 | 不清 |
| `PowerRateTime` | 默认 0（无 ctor 赋值） | 不落库 | 不清 | 不清 |
| `KillMonExpRate` | ctor `:941` = 100 | 不落库 | 不清 | 不清 |
| `KillMonExpRateTime` | 默认 0 | 不落库 | 不清 | 不清 |
| `ExpRateTick` | ctor `:942` = now | 不落库 | 不清 | 不清 |
| `Revival` | 默认 false；`RecalcAbilitys` 命中 shape 时 `:2242` 置 true；**不重置** | 不落库（登录重算） | 不清（粘滞） | 不清 |
| `RevivalTick` | 默认 0 | 不落库 | 不清 | 不清 |
| `UnRevival` | 默认 false；**无写入者** | 不落库 | 不清 | 不清 |
| `LastHiter`/`LastHiterTick` | `BaseObject.cs:451` = null | 不落库 | `BaseObject.Base.cs:326-330` 仅在 `Master != null` 时清；玩家自身死亡不清 | 不清 |
| `AutoGetExpPoint/Envir/Time/InSafeZone` | ctor `:1009-1012` | 不落库 | 不清 | 不清 |
| `AutoGetExpTick` | ctor `:1009` = now | 不落库 | 不清 | 复位 `:3796` / `:1485` |
| `AutoTimerStatus/Tick[20]` | ctor `:1032-1033` = 0 | 不落库 | 不清 | 不清 |
| `DecHpTick`/`IncHpTick` | 无 ctor 赋值 ⇒ 0（声明 `PlayObject.cs:783-784`） | 不落库 | 不清 | 不清 |
| `DecHungerPointTick` | ctor `:887` = now | 不落库 | 不清 | 不清 |
| `DecGameGoldTick` | ctor `:967` = now | 不落库 | 不清 | **不复位** |
| `IncGameGoldTick` | ctor `:972` = now | 不落库 | 不清 | 复位 `:3795` / `:1484` |
| `IncGamePointTick` | ctor `:975` = now | 不落库 | 不清 | 复位 `:3794` / `:1483` |

> `Query.cs` 行号实测：`:240` 是 `BonusPoint`、`:241` 是 `GameGold = GetInt32("Gold")`；`characters_ablity` 的 `GetAbilGetRecord` 起于 `:268`，字段块 `:279-296`，HP/MP 读在 `:280-285`（复验件记的 `:276-279` 实为 `using dr` / `if (dr.Read())` / `{` / `Level`，偏移 4 行）。

补充：`MyGuild`/`GuildRankNo`（P21 的 `CheckItemsNeed` 输入）在登录时建立（`PlayObject.cs:1391-1396`，`GuildMgr.MemberOfGuild` + `GetRankName`），不在存档列里；`IsCastleMember` 是 `CastleMgr.IsCastleMember`（`CastleManager.cs:115-125`）逐城堡 `castle.IsMember`，而 `UserCastle.IsMember`（`UserCastle.cs:609-612`）判的是「玩家行会 == 城主行会」（`:480-483` 同义）。`MemberType`/`MemberLevel` 由脚本动作写（`ExecutionProcessingSys.cs:4789-4855`），当前部署 0 使用。**类型差异登记**：oracle `MemberType` 是 `int`（`PlayObject.cs:750` / `IPlayerActor.cs:802`，脚本可写 65535），Rust `bag::TakeOnContext.member_type` 是 `u8`；P21 实现时按 oracle 类型落字段（当前部署 0 使用，结论不变）。

### 1.4 P07 spike 结论

实测（命令见 §7）：
- `grep -rn "AutoTimerStatus" src --include=*.cs` 只命中：声明 `PlayObject.cs:831`、接口声明 `IPlayerActor.cs:924`、ctor 零初始化 `PlayObject.cs:1033`、读取 `PlayObject.Message.cs:145/147`；
- `AutoTimerTick` 另有声明 `PlayObject.cs:827`、接口声明 `IPlayerActor.cs:919`、ctor `PlayObject.cs:1032`、P07 内刷新 `PlayObject.Message.cs:151`；除 P07 自身外无写入者；**接口 `IPlayerActor` 同样只有声明、无写入者**，结论不变；
- `grep -rn "SetOnTimer\|SETONTIMER" src --include=*.cs` 只命中 `ExecutionCode.cs:620-621` 的 `[ScriptDefName("SETONTIMER")] SetOnTimer` 枚举声明，**无 handler、无 `case`**；
- 部署 `E:/MirServer/M2GameSvr/Envir` 中 `SETONTIMER` 使用文件数 = **0**（只读 `grep -a -ril`）。

⇒ **P07 = 死代码**。等价实现 = 不新增字段、不新增 `@OnTimer` 调用路径、不新增 trace 站点；审计件 P07 状态可回写 `N/A（死代码）`。若未来 oracle 树/内容出现写入者，再按 R（时序 + 脚本副作用）重开。

---

## 2. Rust 现状（符号 + 文件路径）

### 2.1 宿主 `SessionGround`（`bins/game-svr/src/main.rs`）

已有：
- `SessionGround.state: ground::GroundState` —— `item_list: Vec<BagItem>`、`gold: i32`（= oracle `BaseObject.Gold`）、`wabil: ability::Ability`（HP/MP/MaxHP/MaxMP/Weight 的**存档/帧面投影**）。
- `SessionGround.game_gold: i32`、`game_point: i32`、`hunger_status: i32`。
- `SessionGround.use_items: Vec<Option<bag::BagItem>>`（13 槽，完整物品：index/dura/duramax/makeindex/…）。
- `SessionGround.recovery: recover::Recovery`（`inc_health/inc_spell/inc_healing/per_health/per_spell/per_healing/inc_health_spell_tick`）。
- `SessionGround.stamina: stamina::Stamina`（`health_tick/spell_tick/auto_recovery_tick/replay_cursor`）。
- `SessionGround.equip: EquipInputs`、`hit_speed`、`save_rcd_tick` 等。

没有：`kill_mon_exp_rate`、`*_rate_time`、`exp_rate_tick`、`revival/revival_tick/un_revival`、P22/P25/P26/P31 需要的计时/布尔、`member_type/member_level`、`auto_get_exp_*`、`auto_timer_*`、`bo_dec/inc_game_gold`。

### 2.2 world `Entity`（`crates/world/src/entity.rs`）

已有：
- `hp/mp/max_hp/max_mp`（战斗结算的权威值；`combat::attack` 直接写 `target.hp`）。
- `power_rate: i32`（`combat::get_attack_power_player` 读）。
- `last_hiter: Option<i32>` / `last_hiter_tick`；`death/death_tick/death_settled`。
- `use_items: [UseItemSlot; 13]`，`UseItemSlot { index, dura }` —— **战斗耐久投影**，由 `combat::struck_damage_durability` 读写。

没有：`revival/revival_tick/un_revival`、per-player `kill_mon_exp_rate(_time)`、`power_rate_time`、`exp_rate_tick`、`un_revival`。

关键现状：
- `Entity.use_items` **从未由宿主装备栏灌入**：全仓只有 `combat::struck_damage_durability` 写它；`SessionGround.use_items` 才是完整真源。⇒ 当前「受击扣装备耐久」既不回写宿主存档，也看不见玩家真实装备（既有缺口）。
- `apply_ability_to_entity`（`bins/game-svr/src/main.rs`）已把 `dc/ac/hit_point/speed_point/hp/max_hp/level/anti_magic/mac/hit_speed` 写进 Entity，**只缺 `mp/max_mp`**。真正的缺口在**调用点**：`handle_bag_cmd` 的 `out.recalc` 支与 `AttPowerUp` 重算支在 `recalc_session_ability` 之后都只写了 `e.hit_speed = sg.hit_speed`；登录/升级路径已调 `apply_ability_to_entity`。⇒ 换装/需求复检后的 `dc/ac/mac/level/hp` 等不会立即反映到战斗实体（已登记缺口）。修法见 §5.5#4。
- `combat::win_exp` 读 `CombatConfig.kill_mon_exp_rate`（全局），不是 per-player。
- `ground::drop_item` 明确未建模 `StdMode==45` 的 `GetRandomLook`（`M2Share.cs:524-527`，会抽 `Random(Shape)`）。
- `merchant::map_random_move` / `map_random_move_xy` / `space_move::space_move_get_rand_xy` 已存在（`MoveToHome` 的两个分支可直接复用）。
- `homemap.rs` 只有 `parse_start_points`/`get_home_info`/`new_human_spawn*`，**没有 `MoveToHome`/`BaseObjectMove`**。
- `crates/envir/src/mapinfo.rs::MapInfoFlag` 已解析 `bo_dechp/n_dechppoint/n_dechptime/bo_inchp/n_inchppoint/n_inchptime/bo_decgamegold/.../bo_incgamepoint`；运行期消费者为 0（只有 `crates/envir/tests/csharp_parity.rs` 对读）。
- `bins/game-svr/src/config.rs` / `crates/envir/src/conf.rs` 没有 `HungerSystem`/`HungerDecHP`/`RevivalTime`。
- `crates/script/src/engine/actions.rs::ActionOfAutoGetExp` 已实现（含错误分支）；`crates/script/src/engine/mod.rs` 的 `ScriptPlayer::set_auto_get_exp` 是默认空实现；`bins/game-svr/src/npc.rs::NpcPlayer` 未 override。`SetMemberType/SetMemberLevel` 动作在 Rust 引擎里**没有实现分支**（枚举存在）。
- `crates/world/src/world.rs` 的 tick 末尾已有 `expire_player_run_state` → `check_dup_obj` → `verify_players_map_time`；`settle_deaths` 在 tick 开头。`settle_deaths` 同时处理怪物死亡/经验/玩家退组/主人死自投递。
- `World::gain_exp` 是私有；`World::tick` 是唯一生产入口；宿主在 tick 之后跑 `drain_recovery`/`drain_run_downlinks`/`drain_periodic_save`。

### 2.3 存档/DB（`bins/db-srv/src/playdata.rs`、`crates/storage/src/playerdata.rs`）

- 读：`characters.Gold` → `gold` **且** `game_gold`（照抄 oracle 同列双读）；`GamePoint` → `game_point`；`HungerStatus` → `hunger_status`；`characters_ablity` → HP/MP（`LoWord/HiWord`）。
- 写：`save_characters` 绑 `@Gold = d.gold`（**不是** `d.game_gold`）、`@GamePoint`、`@HungerStatus`；`characters_ability` 绑 `d.abil.hp/mp`。与 oracle 的 `Save.cs` 一致。
- 风险点：`build_save_record` 只写 `rec.data.game_gold = sg.game_gold`，**没有写 `rec.data.game_point = sg.game_point`**；`game_point` 的运行时改动目前不会回写存档（P24 将来必须补；PL-4 不涉及）。

---

## 3. 归属建议

### 3.1 总原则

1. **会话/持久化/脚本/帧面**的状态留 `SessionGround`；world 不读它、不写它。
2. **world 的战斗/经验/死亡同 tick 直接读**的数值进 `Entity`；宿主只保留**投影**（`state.wabil`），投影方向固定为 Entity → SessionGround，且在存档前刷新。
3. 不允许两边都当权威。确有需要双写的字段，必须在表里写清唯一真源与同步时刻。
4. 不为了“看起来干净”搬字段：搬一个字段就会改动存档时序/clock 相位/既有 golden，收益必须由 world 的读点证明。

### 3.2 逐字段归属表

| 字段 | 归属 | 唯一真源 / 同步方向 | 判断依据 |
|---|---|---|---|
| `game_gold` | `SessionGround` | 宿主 | 只有 P22/P24、寄售/交易、帧面与登录载入读；world 战斗/死亡不读。DB 双读同列的 oracle bug 保留。 |
| `game_point` | `SessionGround` | 宿主 | 只有 P24、寄售/交易、帧面读。 |
| `hunger_status` | `SessionGround` | 宿主 | P26/`CM_EAT`/DB/`GetMyStatus` 全在宿主；world 不读。 |
| `use_items`（完整物品） | `SessionGround` | 宿主（P21/P32 直接改） | `AddItemToBag`/`SendDelItems`/`RecalcAbilitys`/存档都在宿主；world 只需要耐久投影。 |
| `Entity.use_items`（index+dura） | `Entity`（**投影**） | 宿主 → world（入图/换装/需求复检后整栏灌入）；world → 宿主（受击耐久后回写） | 战斗 `struck_damage_durability` 在 world tick 内必须同步改耐久；但完整物品仍以宿主为准。两条方向都必须显式化。 |
| `hp/mp/max_hp/max_mp` | `Entity`（战斗权威）+ `SessionGround.state.wabil`（投影） | Entity → 宿主（每次战斗/恢复/片段后刷新；存档前必须刷新） | `combat::attack`、死亡、`win_exp`、buff 毒伤都读 Entity；宿主 `wabil` 只服务帧面/存档。P25/P26/P32 的 HP 改动必须同时写 Entity 再回投影。 |
| `power_rate` | `Entity`（已有） | Entity | 攻方攻击力 `combat::get_attack_power_player` 直接读；P27 归零改它。 |
| `power_rate_time` | `Entity`（新增） | Entity | 与 `power_rate` 同段同 tick 递减，放一起避免双写。 |
| `kill_mon_exp_rate` | `Entity`（新增） | Entity | `combat::win_exp` 有 `&mut Entity killer`，改成读 `killer.kill_mon_exp_rate` 即可；留宿主会导致每个经验点都要传参/回写。 |
| `kill_mon_exp_rate_time` | `Entity`（新增） | Entity | 与 rate 同段递减。 |
| `exp_rate_tick` | `Entity`（新增） | 宿主段 P27 独占读写；**时钟域 = `clock::now_ms()`**（登录/进世界时初始化，比较也用同一来源） | 与 `kill_mon_exp_rate(_time)` 同段同 tick 递减，放一起避免双写。P27 必须留在宿主段：world 段只能在 `tick_head`（P21..P26 之前）或 `tick_tail`（P31/P32 之后）整体运行，二者都会破坏 oracle 的 P26→P27→P31 行序。 |
| `last_hiter`/`last_hiter_tick` | `Entity`（已有） | Entity | `SetLastHiter`/`StruckDamage`/P32 都读；world 已有。P32 需在 `expire_hitters` 之前取快照（见 §5.5）。 |
| `death/death_tick` | `Entity`（已有） | Entity | 死亡结算/战斗/P32 门都读。 |
| `revival` | `SessionGround`（新增） | 宿主（`recalc_session_ability` 写；**只置 true，不清 false**） | 唯一消费者是 P32，而 P32 由宿主驱动；`Revival` 来自装备 shape，宿主 `ability.rs` 本来就在遍历 `use_items`。放 Entity 会多一份需要同步的锁存状态，且没有 world 读点。 |
| `revival_tick` | `SessionGround`（新增） | 宿主 | 只在 P32 写/读。 |
| `un_revival` | 不建字段（或 `SessionGround`，默认 false） | — | 当前 oracle 树无写入者，恒 false；等出现写入者再加。 |
| `dec_hp_tick`/`inc_hp_tick` | `SessionGround`（新增） | 宿主 | 只在 P25；初值 0（照 oracle 未初始化）。 |
| `dec_hunger_point_tick` | `SessionGround`（新增） | 宿主 | 只在 P26；ctor = now。 |
| P22 `bo_dec/inc_game_gold`、`dec/inc_game_gold(_time/_tick)` | `SessionGround`（新增） | 宿主 | 只在 P22；与 `game_gold` 同生命周期。 |
| P31 `auto_get_exp_point/time/envir/in_safe_zone/tick` | `SessionGround`（新增） | 宿主（脚本适配器写，P31 读） | 脚本 trait 是宿主侧；world 不读。P31 结算通过新的 world 经验入口完成。 |
| P07 `auto_timer_status/tick[20]` | **不建** | — | 死代码（§1.4）。 |
| `member_type`/`member_level` | `SessionGround`（新增，仅在 P21 需要时） | 宿主 | 脚本动作写、P21 读；world 不读。当前部署 0 使用。 |
| `MyGuild`/`GuildRankNo`/`is_castle_member` | 不落字段；P21 时从 `GuildManager`/`CastleManager` **按需查询** | 宿主只读（两个管理器当前未挂进 `World`） | `Entity` 没有 guild 字段；`GuildManager.guilds[].ranks[].members`（`crates/world/src/guild.rs`）与 `CastleManager.castle_list[].own_guild`（`crates/world/src/castle.rs`/`castle_tax.rs`）足够算出 `has_guild/guild_rank_no/is_castle_member`。**注意：这两个管理器目前没有接进 `World`/`sim_loop`，P21 需要先补这条数据面接线（或在宿主侧持有同一份）**。 |

### 3.3 三个容易做错的点

1. **HP 不是“搬进 Entity 就完事”**。当前 `SessionGround.state.wabil` 是存档/帧面/恢复/体力点的读写对象，`Entity.hp` 是战斗权威。建议把 `Entity` 定为唯一真源，宿主 `wabil` 定为 projection；所有宿主侧 HP 改动（P25/P26/P32、恢复、体力点）通过一个 helper 先写 `Entity.hp` 再刷新 `wabil.hp`；所有 world 侧 HP 改动（攻击、毒伤）在宿主 drain 时刷新 `wabil`。**不能**出现一条路径只改一边。
2. **P32 的 `Revival` 留宿主，但 P32 的门读 Entity**。P32 在宿主玩家段里执行：读 `world.entities[actor].death/hp/max_hp/last_hiter`，读宿主 `revival/revival_tick/use_items`，写 `Entity.hp` + 宿主 HP 投影 + 宿主 `use_items`（耐久/移除）+ 宿主 `revival_tick`，再让 `tick_tail` 的玩家死亡结算读 `Entity.hp`。这样 `Revival` 不需要进 Entity。
3. **`use_items` 的双表示必须补同步**，否则 P32 的“戒指耐久”与 oracle 对不上：oracle 只有一个 `UseItems`，受击扣耐久与复活扣耐久改的是同一份；Rust 的受击只改 `Entity.use_items`，宿主永远看不到。PL-1 必须在 `struck_damage_durability` 之后把 index/dura 回写宿主（或把耐久权威整体移到宿主并在 world 用回调）。不补这条，P32 的构造级用例可以绿，但真机对拍会在“先被打再复活”的路径上差耐久。

---

## 4. 驱动点

### 4.1 现有 `World::tick` 线性化（实测）

顺序（`crates/world/src/world.rs::World::tick`）：
命令 FIFO → `expire_hitters` → `settle_deaths` → 宝宝自投递 → `expire_extra_abil` → 地图事件 → 宝宝清表 → buff → 怪物 AI → NPC → 商人 → `expire_player_run_state`（oracle 54-64）→ `check_dup_obj`（263-285）→ `verify_players_map_time`（base.Run 的 223-228）→ AOI。

宿主在 `world.tick()` 之后：`drain_run_state_expiries` → `drain_pk_flag_downlink` → `drain_party_death_events` → `drain_action_downlink` → `drain_buff_downlink` → … → `drain_recovery`（oracle 70-142）→ `drain_run_downlinks`（体力/魔力点 + AOI 出现帧）→ `drain_periodic_save`。

### 4.2 建议的切分与插入点

**目标序**（只动玩家 Run 相关，不动怪物 AI/NPC/商人的相对位置）：

```
world.tick_head()                       // 命令 FIFO…CheckDupObj（含现有 expire_player_run_state/check_dup_obj）
宿主既有 drain_*（帧面）
drain_recovery(...)                     // oracle 70-142
drain_player_run_segments(...)          // 每玩家按槽位序：P21 → P22 → [P24] → P25 → P26 → P27 → P31 → P32
world.tick_tail(&mut rec)               // 玩家死亡结算（base.Run:70-73）→ VerifyMapTime（:223-228）→ AOI
drain_run_downlinks(...)                // 体力/魔力点 + 经验帧 + AOI 出现帧
drain_periodic_save(...)
```

- `World::tick()` 保持为 `tick_head()` + `tick_tail()` 的组合，既有 golden/单测调用不变；宿主 `sim_loop` 改用三段式，把 `drain_player_run_segments` 插在中间。
- `tick_head` 负责：命令 FIFO、`expire_hitters`、**非玩家**死亡结算、宝宝自投递、extra_abil、事件、宝宝清表、buff、怪物 AI、NPC、商人、`expire_player_run_state`、`check_dup_obj`。
- `tick_tail` 负责：**玩家**死亡结算（`settle_deaths` 加 `race == PLAY` 过滤）→ `verify_players_map_time` → AOI 刷新。理由：oracle `base.Run()` 的 Die（`:70`）在 `VerifyMapTime`（`:223`）之前，且两者都在 P32 之后。注意 oracle 的 `:230-245` 还有 HP/MP 超上限钳制 + `HealthSpellChanged()`；Rust 的 `verify_map_time` 目前只建模 `:223-228`，切 `tick_tail` 时应把 `:230-245` 一并补上或显式登记。
- 为什么 `drain_recovery` 要放在玩家段之前：oracle 恢复（70-142）在 P21（403）之前；P21 的 `RecalcAbilitys` 会保留当前 HP，恢复与复检的顺序会改变同一 Run 的 HP 轨迹。把 `drain_recovery` 原位置（tick 后）保持，只把新段插在它之后，是最小改动。
- 为什么 P22 必须在 `VerifyMapTime` 之前：`MoveToHome` 会改位置/换图；oracle 的 `VerifyMapTime` 在 `base.Run` 里读的是移动后的 `CurrX/CurrY`。若保持当前 `verify_players_map_time` 在 tick 内，则 P22 之后要在新位置再刷一次；切到 `tick_tail` 后天然正确。

**逐段插入位置表**（相对现有段）：

| 段 | 插入位置 | oracle 对应 | 驱动者 | 备注 |
|---|---|---|---|---|
| P07 | 若未来复活：`tick_head` 中 `expire_extra_abil` 之前（即宝宝自投递/恢复之后、extra_abil 之前） | 143-157 | world（或宿主） | **当前不实现（死代码）**；注意 Rust `drain_recovery` 在 tick 后，若 P07 活会与 oracle 的「recovery→P07→extra_abil」有相位差，需另立卡。 |
| P21 | `drain_player_run_segments` 第 1 段（在 `check_dup_obj` 之后、`verify_players_map_time` 之前） | 403-441 | 宿主 | 每玩家按 Entity 槽位序；`CheckItemsNeed` 新子集；`RecalcAbilitys` 后同步 Entity 战斗数值。 |
| P22 | 同阶段，P21 之后 | 442-529 | 宿主 | 金币不足 → `MoveToHome`（复用 `merchant::map_random_move`）；`RM_GOLDCHANGED` 只在变化时发。 |
| P24（不在 PL） | P22 与 P25 之间 | 548-568 | 宿主 | 仅为将来留位；PL-4 不实现时该位置为空。 |
| P25 | P22 之后、P26 之前 | 569-594 | 宿主 | 改 HP 时同时写 Entity + 宿主投影；`INCHP` 用 `nDECHPPOINT` 的 bug 照抄。 |
| P26 | P25 之后、P27 之前 | 596-635 | 宿主 | 状态跨档 `RefMyStatus` = RecalcAbilitys + 8102；改 HP/HealthTick/SpellTick。 |
| P27 | P26 之后 | 636-657 | 宿主（状态在 Entity，**时钟域 = `clock::now_ms()`**） | 1s 门用 `clock::now_ms()` 与登录/进世界初始化的 `exp_rate_tick` 比较，**不得**用 `world.clock_ms`；归零改 `Entity.kill_mon_exp_rate/power_rate`；`win_exp` 已改读 per-player。 |
| P31 | P27 之后、P32 之前 | 738-745 | 宿主 | **口径写死**：P31 = `GetExp(AutoGetExpPoint)`（`PlayObject.Base.cs:149-166`）= `exp += n` + `AddBodyLuck(n*0.002)` + 单步升级 + `IncHealthSpell(2000,2000)`，**不乘任何倍率**。新增 world 入口只做这条语义并入 `rec.exp_awards`，由 `drain_run_downlinks` 发 `SM_WINEXP(44)`；**不得复用 `World::gain_exp`**（它经 `CalcGetExp`/`KillMonExpMultiple`/`MNKillMonExpMultiple`/`KillMonExpRate`/地图 `ExpRate`/组队均分与加成）。 |
| P32 | P31 之后、`tick_tail` 的玩家死亡结算之前 | 747-763 | 宿主 | 读 Entity 的 HP/Death/LastHiter 快照；改宿主 `use_items` + Entity HP；`Revival` 粘滞。 |
| 玩家死亡结算 | `tick_tail` 第 1 步 | `base.Run` 70-73 | world | `settle_deaths` 拆分；玩家死亡延后到 P32 之后。 |
| `VerifyMapTime` | `tick_tail` 第 2 步 | `base.Run` 223-228 | world | 在 P22 可能移动之后。 |

### 4.3 死亡结算顺序（PL-1 的前置）

当前 `settle_deaths` 在 tick 开头，会把 `death=true` 写在 P32 之前，P32 的 `!Death` 门永远失败；同时它会把 HP=0 的玩家直接 Die，而 oracle 是先跑 P32（可救活）再 `base.Run` 的 Die。因此：

1. `settle_deaths` 加过滤：`tick_head` 只结算 `race != PLAY`（怪物/宝宝/NPC 等）；玩家留到 `tick_tail`。
2. `tick_tail` 在 P21..P32 全部跑完后，再结算 `race == PLAY && hp == 0 && !death_settled`。
3. P32 读 `LastHiter` 时用 **`expire_hitters` 之前的快照**（oracle 的 `LastHiter` 清理在 `base.Run:119-132`，晚于 P32）。可在 `tick_head` 开头为每个玩家记下 `(last_hiter, last_hiter_tick)`，或在 P32 之前不清理玩家自己的 `last_hiter`。前者最小侵入。
4. 现有 golden 没有玩家死亡（`combat*.txt` 的 `death` 行全部是怪物），所以把玩家 Death 从 head 移到 tail **不改既有 golden**；只改玩家死亡场景的 `record.deaths`/退组/自投递相对位置。
5. 该切分已由 orchestrator 于 2026-10-18 裁决接受（§6.2），PL-1 的首个 commit 按本小节落地。

### 4.4 帧面与 RNG 次序

- P21/P22/P25/P26/P27/P31/P32 的帧（`SM_ADDITEM`/`SM_DELITEMS`/`SM_WEIGHTCHANGED`/52/752/`SM_GOLDCHANGED(10136)`/53/8102/`SM_SYSMESSAGE(100)`/`SM_WINEXP(44)`/`SM_DURACHANGE(642)`）都在宿主段内产生；段内按 oracle 行序逐条入各自连接的发送口。跨段的发送次序必须固定为 P21→P22→P25→P26→P27→P31→P32；不要按帧类型分组后再排序。
- 新段的 world 状态改动（Entity 的 rate/exp/HP/use_items 投影）必须发生在 `tick_tail`/`drain_run_downlinks` 之前，否则同一 tick 的体力/魔力点与经验帧会读到旧值。
- `drain_run_downlinks` 目前按帧优先级处理 53/44/45/52/752；P31 的 `GetExp` 若走 `rec.exp_awards`，天然复用这条链；P25/P26 的 53 建议也进同一记录（带段序），避免与恢复段的 53 乱序。

---

## 5. 风险

### 5.1 存档/DB

- **不要搬 `game_gold` 的存储位置**：oracle 的 `characters.Gold` 同列双读、只写 `Gold` 字段；Rust `playdata.rs` 已照抄。P22 只改内存 `game_gold`，存档仍写 `sg.state.gold`；若“顺手”把 `game_gold` 写进 `@Gold`，会让金币列变成元宝，和 oracle 行为分叉。
- **`game_point` 目前不回写**：`build_save_record` 没写 `rec.data.game_point`。PL-4（P22）不碰它；将来接 P24 时必须补，否则地图 / `GAMEPOINT` 命令的改动不落库。
- **HP 存档读 `sg.state.wabil`**：若把 HP 真源改成 Entity，必须在 `drain_periodic_save` 之前把 Entity → `wabil` 刷新；否则存档读到旧 HP（尤其 P25/P26/P32 刚改过时）。
- **`UseItems` 耐久**：宿主存档读 `SessionGround.use_items`；受击耐久只改 `Entity.use_items`，当前不落库。P32 的复活扣耐久若只改宿主，真机上会被下一次受击覆盖；必须补 world → 宿主耐久回写（§3.3）。

### 5.2 时钟/回放

- 宿主段必须用与 `recover`/`stamina` 同源的 `now_ms()`（`clock::now_ms`，支持注入），不要混用 `world.clock_ms`（默认从 0 起、每 tick +200）。混用会让 P22/P25/P26/P27/P31 的门在真机与注入回放里差一个 epoch。**`exp_rate_tick` 虽存放在 `Entity`，但它属于宿主时钟域**：登录/进世界初始化与 P27 的 1s 比较都必须用 `clock::now_ms()`，world 段（`tick_head`/`tick_tail`）不读也不写该字段；绝不能拿 `clock::now_ms()` 去比 `Entity::new` 的 0 或 `world.clock_ms`（真机会差一个 epoch，P27 首秒永不触发或立即触发）。
- `DecHpTick`/`IncHpTick` 在 oracle 里 **0 初始化**；若 Rust 用 `now` 初始化，第一次地图 HP 事件会晚一个周期。照抄 0。
- 换图复位：P22/P31 的 `IncGameGoldTick`/`IncGamePointTick`/`AutoGetExpTick` 要在宿主换图路径（`SpaceMove`/`EnterAnotherMap` 等价物）复位；`DecGameGoldTick` **不复位**。Rust 当前没有这些字段，必须在 PL-4/PL-2 一并接。

### 5.3 对拍/golden

- world golden 走 `World::tick()`；`tick()` = head + tail，玩家死亡只在 tail 结算。现有 `golden/combat*.txt` 的 death 都是怪物（实测），不受影响。
- 新段默认只在宿主生产路径运行；world golden 不调用宿主段，因此 `golden/*.txt` 不因新增段变化。构造级测试必须显式调用宿主段。
- P21 的 `RecalcAbilitys` 会改变 `WAbil`；修法是**给 `apply_ability_to_entity` 补 `mp/max_mp`，并让 `handle_bag_cmd` 的 `out.recalc` 支与 `AttPowerUp` 重算支改调它**（该 helper 已是全字段、只缺 mp；不是“把只同步 `hit_speed` 扩成另一个新 helper”）。既有“只同步 hit_speed”的测试断言可能要更新（这是修既有缺口，不是 golden 漂移）。

### 5.4 RNG trace（五批 B1..B5）

- **P21 新增抽数**：背包满 + `StdMode==45` 时，`DropItemDown` 在 `AddItemToMap` 之前调 `M2Share.GetRandomLook`（`M2Share.cs:524-527`）⇒ `Random(Shape)` 一次。即使 `AddItemToMap` 因 `ItemId==0` 恒失败、物品不落地，这次抽数在 oracle 里**已经发生**。Rust `ground::drop_item` 当前未建模。实现时必须登记：oracle `M2Share.GetRandomLook@M2Share.cs:526`（调用者 `BaseObject.DropItemDown@BaseObject.cs:559`），Rust 站点建议放在 `crates/world/src/run_gap.rs` 或 `bins/game-svr/src/ground.rs` 并归 `player_run`/`drop` 组（当前 `consumer_groups.tsv` **没有** `GetRandomLook` 行 ⇒ 不登记会 `UNCLASSIFIED`）。
- **P22 可能新增抽数**：金币不足 → `MoveToHome` → `MapRandomMove` 两抽（`BaseObject.cs:1235-1236`）+ `SpaceMoveGetRandXY` 移位环（`:1040`）。这两组在 `consumer_groups.tsv` 已存在（`merchant`/`space_move`），Rust `merchant::map_random_move` 已实现；但调用点从玩家 Run 进入时，五批 trace 的**相对次序**会变（新增在 tick 末尾）。当前内容不可达（金币不足 + 地图 Flag 0 条），B1..B5 不会触发；一旦构造场景触发，必须重跑 `rng_consumer_groups.py --require-closed` 与 `rng_ledger_diff.py`。
- **P25/P26/P27/P31/P32**：均无 RNG 抽数（`RecalcAbilitys`/`GetExp`/`ItemDamageRevivalRing`/`HealthSpellChanged` 都不抽）；`GetExp` 的 `AddBodyLuck` 也不抽。⇒ 不改变 B1..B5 的组集合/相对次序。
- P07（若未来复活）不抽；`AutoTimer` 无随机。

### 5.5 既有缺陷/缺口的交互

1. **`expire_hitters` 早于 P32**：oracle 的 `LastHiter` 清理在 `base.Run:119-132`，晚于 P32；Rust 在 tick 开头。对“被怪物打死后 30s 内仍保留 LastHiter”的场景，oracle P32 会因 `LastHiter.Race != Play` 拒绝复活，Rust 若已清空会错误复活。→ P32 用快照（§4.3）。
2. **`settle_deaths` 早于 P32**：见 §4.3，必须拆。
3. **`Entity.use_items` 从未灌入**：受击耐久循环看不到玩家装备，P32 的戒指耐久也看不到战斗损耗。→ PL-1 补双向同步。
4. **重算后的实体同步缺口在调用点**：`apply_ability_to_entity` 已写全字段（除 `mp/max_mp`），但 `handle_bag_cmd` 的 `out.recalc` 支与 `AttPowerUp` 重算支在 `recalc_session_ability` 后只写了 `e.hit_speed = sg.hit_speed` ⇒ P21 复检后的 `max_hp/dc/ac/mac/level/hp` 不会进 Entity，战斗会用旧值。→ PL-1：**给 helper 补 `mp/max_mp`，并让上述两个重算支改调 `apply_ability_to_entity(e, &sg.state.wabil, &sg.sub, sg.hit_speed)`**。
5. **P21 的 guild/castle/member 输入未接线**：`TakeOnContext` 在唯一调用点（`bins/game-svr/src/main.rs` 的 `handle_bag_cmd`）把 `has_guild/guild_rank_no/is_castle_member/member_type/member_level` 全填常量。P21 必须从宿主侧的 `GuildManager`/`CastleManager`（`crates/world/src/guild.rs`、`crates/world/src/castle_tax.rs`，当前未挂进 `World`/`sim_loop`）查真实值，并新增宿主 `member_type/member_level`（脚本写入；当前部署 0 使用）。`bag::check_take_on_items` 的 7/70 分支还多了 `has_guild` 条件（与 `CheckItemsNeed` 不同），**不能复用**。
6. **P22 的脚本动作不可用**：`AUTOADDGAMEGOLD`/`AUTOSUBGAMEGOLD` 在 oracle 里不解析参数（`ExecutionProcessingSys.cs:2667-2688`/`:2733-2754`）⇒ 等价实现应保持“START 恒报错、STOP 清 flag”；不要为了“能用”去解析 `sParam2/sParam3`。
7. **P31 部署行报错**：`SETAUTOGETEXP 1400 1 1` 无地图参数 ⇒ oracle `ScriptActionError`，字段不变。Rust `ActionOfAutoGetExp` 已用 `map_exists("")` 复刻；玩家段必须仍以字段为 0 时零动作为判据。
8. **P32 粘滞 `Revival` + `UnRevival` 恒 false**：见 §1.2；测试必须覆盖“装戒→卸戒→仍 Revival=true”和“LastHiter 为怪物→不复活”。
9. **死亡路径未清 `Recovery` 组**：oracle `PlayObject.Die()`（`PlayObject.cs:1970-1972`）清零 `IncSpell/IncHealth/IncHealing`（`PerHealth/PerSpell` 不清）；Rust 宿主死亡路径当前不清 `SessionGround.recovery.inc_*`（`recover.rs` 只在恢复分支零化）。该组进 §1.3 生命周期表，并在 PL-3 随 P26 一并补（或显式登记为 PL-3 必做项）。

---

## 6. 结论与开工判定

### 6.1 开工判定

- **PL-1（P21 + P32）**：可直接开工，但**第 1 个 commit 必须是 tick 切分 + 玩家死亡后移 + LastHiter 快照**（§4.3），否则 P32 无法落地。随后：`check_items_need` 子集、`apply_ability_to_entity` 补 `mp/max_mp` + 两处重算支改调它、复活戒指 shape 锁存 + `ItemDamageRevivalRing`、`use_items` 双向同步。
- **PL-2（P31 + P07）**：可直接开工。P07 已判定死代码 ⇒ 不实现，只把证据落档；P31 的脚本适配器 + 1s 结算 + `SM_WINEXP(44)` 按 §4.2 插入 P27 之后、P32 之前；经验口径固定为 `GetExp(AutoGetExpPoint)` 直加（`PlayObject.Base.cs:149-166`），**不复用 `World::gain_exp`**。
- **PL-3（P27 + P25 + P26）**：可直接开工。P27 需要 `Entity.kill_mon_exp_rate(_time)/power_rate_time/exp_rate_tick` 与 `win_exp` 改读 per-player（时钟域见 §5.2）；P25/P26 全宿主，注意 HP 双写与 `RefMyStatus` 的 Recalc。另补死亡清：`Die()` 的 `IncSpell/IncHealth/IncHealing`（§5.5#9）。
- **PL-4（P22）**：可直接开工。全宿主；`MoveToHome` 新增于 `homemap.rs`，复用 `merchant::map_random_move`；`RM_GOLDCHANGED` 只在 `tObjCount != GameGold` 时发；`game_gold` 不落库的 oracle bug 保留。
- **串行**：PL-1/2/3/4 全部改 `bins/game-svr/src/main.rs` 的同一宿主段序与 `SessionGround`，必须串行；PL-1 另改 `crates/world/src/world.rs`/`entity.rs`，与任何动 tick/Entity 的车道串行。

### 6.2 决断点（已全部裁决 / 无需再等 owner）

1. **`tick_tail` 方案：已裁决（orchestrator，2026-10-18，依据硬约束③行为等价）——接受。** 玩家死亡结算从 tick 开头移到 P32 之后，并顺带把 `VerifyMapTime`/AOI 放进 tail；与 oracle `PlayObject.Run`（P21→…→P32→`base.Run` 的 Die→VerifyMapTime）同序。对既有 golden 的影响：现有 world golden 的 `death` 行全部是怪物（`combat*.txt` 的幽灵/僵尸），**没有玩家死亡场景 ⇒ 不改任何既有 golden**；仅玩家死亡场景的 `record.deaths`/退组/自投递相对位置按新 tail 序变化（§4.3）。替代方案「P32 延后一 tick」差 200ms 且改变救活语义，已否决。
2. 其余（P07 不实现、P22 脚本动作保持坏、P32 粘滞 Revival、`game_gold` 落库 bug、P21 guild/castle 输入接线、`use_items` 双向同步、死亡清 `Recovery`）均按“行为等价优先”有唯一技术答案，不需要 owner 裁决。

---

## 7. 复现命令（行号逐条实测）

以下命令在基线 `32c72c120` 的任意只读 worktree 根目录执行；输出与 §1 的行号一一对应。

```sh
# Run 总序 / 各段边界
sed -n '143,157p' src/M2Server/Player/PlayObject.Message.cs
sed -n '403,441p' src/M2Server/Player/PlayObject.Message.cs
sed -n '442,529p' src/M2Server/Player/PlayObject.Message.cs
sed -n '548,568p' src/M2Server/Player/PlayObject.Message.cs
sed -n '569,594p' src/M2Server/Player/PlayObject.Message.cs
sed -n '596,635p' src/M2Server/Player/PlayObject.Message.cs
sed -n '636,657p' src/M2Server/Player/PlayObject.Message.cs
sed -n '738,745p' src/M2Server/Player/PlayObject.Message.cs
sed -n '747,763p' src/M2Server/Player/PlayObject.Message.cs

# 字段声明 / ctor / 生命周期
grep -n "public int HungerStatus\|public int RevivalTick\|public bool Revival\|public bool UnRevival\|public int KillMonExpRate\|public int PowerRate\|public int KillMonExpRateTime\|public int PowerRateTime\|public int GameGold\|public int GamePoint" src/M2Server/Player/PlayObject.cs
sed -n '875,945p' src/M2Server/Player/PlayObject.cs
sed -n '960,1015p' src/M2Server/Player/PlayObject.cs
sed -n '1030,1034p' src/M2Server/Player/PlayObject.cs
sed -n '1478,1488p' src/M2Server/Player/PlayObject.cs
sed -n '3788,3798p' src/M2Server/Player/PlayObject.cs

# 死亡 / LastHiter / VerifyMapTime
sed -n '60,75p' src/M2Server/Actor/BaseObject.Base.cs
sed -n '105,135p' src/M2Server/Actor/BaseObject.Base.cs
sed -n '208,230p' src/M2Server/Actor/BaseObject.Base.cs
sed -n '318,335p' src/M2Server/Actor/BaseObject.Base.cs
sed -n '1964,1997p' src/M2Server/Player/PlayObject.cs
sed -n '1748,1762p' src/M2Server/Actor/BaseObject.cs

# 复活戒指 / Recalc 锁存
sed -n '2008,2025p' src/M2Server/Player/PlayObject.cs
sed -n '2230,2250p' src/M2Server/Player/PlayObject.cs
sed -n '5355,5392p' src/M2Server/Player/PlayObject.cs
sed -n '180,190p' src/M2Server/M2Share.cs

# 脚本动作
sed -n '2667,2688p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs
sed -n '2691,2711p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs
sed -n '2733,2754p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs
sed -n '5449,5464p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs
sed -n '5539,5554p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs
sed -n '4789,4855p' src/Modules/ScriptEngine/Processings/ExecutionProcessingSys.cs

# 登录载入 / 存档
sed -n '1105,1180p' src/GameSrv/Word/WorldServer.cs
sed -n '1231,1382p' src/GameSrv/Word/WorldServer.cs
sed -n '235,300p' src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs  # :240/:241 = BonusPoint/GameGold；:268-296 = characters_ablity
sed -n '80,120p' src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs

# P07 死代码
grep -rn "AutoTimerStatus\|AutoTimerTick" src --include=*.cs
grep -rn "SetOnTimer\|SETONTIMER" src --include=*.cs

# 部署内容（只读；不启停服务）
for k in SETONTIMER AUTOADDGAMEGOLD AUTOSUBGAMEGOLD KILLMONEXPRATE POWERRATE SETAUTOGETEXP SETMEMBERTYPE SETMEMBERLEVEL; do echo "$k $(grep -a -ril "$k" /e/MirServer/M2GameSvr/Envir | wc -l)"; done
grep -a -ric "DECHP\|INCHP\|DECGAMEGOLD\|INCGAMEGOLD\|INCGAMEPOINT" /e/MirServer/M2GameSvr/Envir/MapInfo.txt
grep -a -in "HungerSystem\|HungerDecHP\|RevivalTime" /e/MirServer/M2GameSvr/Server.conf
grep -a -rin "SETAUTOGETEXP" /e/MirServer/M2GameSvr/Envir
```

实测结果（本次）：
- P07：`AutoTimerStatus` 只有声明/ctor/读取；`SETONTIMER` 只有 `ExecutionCode.cs:620-621`；部署文件数 0。
- `SETONTIMER/AUTOADDGAMEGOLD/AUTOSUBGAMEGOLD/KILLMONEXPRATE/POWERRATE/SETMEMBERTYPE/SETMEMBERLEVEL` 部署文件数均为 0；`SETAUTOGETEXP` 1 个文件 1 行（`Envir/Market_Def/QFunction-0.txt:25`，无地图参数）。
- `MapInfo.txt` 的 `DECHP/INCHP/DECGAMEGOLD/INCGAMEGOLD/INCGAMEPOINT` 命中 0 条。
- `Server.conf`：`396 HungerSystem=0`、`398 HungerDecHP=0`、`739 RevivalTime=60000`。

---

## 8. 残留物清单

- 本 spike 只新增本文件；未生成临时库/抓包/日志/脚本。
- 未起服务；`E:/MirServer` 九游戏端口状态未触碰（仍由外部 C# 栈占用）。
- 本 spike 的只读 worktree 在交付后由编排者按 §14.3 清理；本件不做清理动作。
