# `Run` 缺口可执行批次规划（玩家 Run 散段 + 34 个专用怪 `Run` 覆写）

- 规划日期：2026-10-18；**订正版 rev2**（同日按复验 20 条 must_fix 逐条修订：P32 ushort 回绕、R22/R29 抽数序、R13/R08 双判、R43 次数、R07 3s 门、MoveToHome 缺失、R34/R35 融化效果、3 处已有原语复用、MB-4/MB-8 trace、4 处行号、PR-2/PR-5、R16 作用域、整数除法、PR-3 前置、§2.5 覆盖对账）。
- 代码基线：`706c95cae`（订正 worktree `/e/tmp/rev-plan`）。本件引用的 Rust 符号/文件均以该基线为准；oracle 行号以本仓 `src/**` 当前树为准。
- 输入：
  - `mir2-rs/tests/parity/m3/audit-baseobject-run-coverage-20261018.md`（139 行覆盖矩阵，下称「审计件」）；
  - `mir2-rs/tests/parity/m3/design-monster-run-overrides-20261017.md`（43 条 Run 清册/可达性/设计，下称「怪物设计件」）；
  - `mir2-rs/tests/parity/m3/design-player-run-cadence-evidence-20261018.md`（玩家 Run 节拍裁定设计，下称「节拍设计件」）。
- 性质：**只读规划件**。本件是唯一交付物；未改 `.rs` / `src/**` / `sql/**` / `MirClient/` / `crates/{protocol,storage}/**` / `E:/MirServer/**`；未起服务、未占端口、未写库。
- 引用纪律：oracle 行号**逐条 `sed -n` 实测**（首/末行核对），复现命令见 §7；Rust 侧只引符号/文件路径，不写 `:行号`（并行车道会位移）。
- 判级：按 `AGENTS.md`「判据分级」；判不准一律按 **R**。本件本身为 **L**（文档）。

---

## 0. 结论摘要

1. **本次范围**：玩家 `Run` 散段 8 项（P07/P21/P22/P25/P26/P27/P31/P32）→ 4 个玩家批次；34 个「审计判缺失」的专用怪 `Run` 覆写 → 31 个新批次项（13 个真机可达 + 18 个数据不可达）、3 个在飞项不重复排；另列 5 个跨批前置（PR-1..PR-5）。
2. **玩家侧三个实测异常（必须先判，否则会写错）**：
   - **P07 疑似 oracle 死代码**：`AutoTimerStatus` 除 ctor 置 0 外**无任何写入者**；`SETONTIMER` 只在 `ExecutionCode.cs:620-621` 声明，整个 `src/` 无 handler；部署 `Envir` **0 个文件**使用 `SETONTIMER`。⇒ 先 spike；确认后「等价实现 = 不实现」，不得凭空造一个每 100ms 触发脚本的定时器。
   - **P21 的每-Run 复检不是穿戴校验**：它只调 `PlayObject.Base.cs:2832-2882` 的 `CheckItemsNeed`，只处理 `Need ∈ {6,60,7,70,8,81,82}`；其余 Need 一律通过。Rust `bins/game-svr/src/bag.rs::check_take_on_items` 是**全量穿戴校验**（含 Need 0/等级/职业/负重），**不能整体复用**。
   - **P25 的 `INCHP` 支照抄了 `DECHP` 的字段**：`PlayObject.Message.cs:585-587` 用 `nDECHPPOINT` 判上限并加血（不是 `nINCHPPOINT`）。按「连 bug 一起搬」复刻，禁止修正。
3. **内容可达性（本次只读复核）**：部署 `Server.conf:396/398` 为 `HungerSystem=0` / `HungerDecHP=0`；`MapInfo.txt` **0 条** `DECHP`/`INCHP`/`DECGAMEGOLD`/`INCGAMEGOLD`；`Envir` 脚本 **0 处** `AUTOADDGAMEGOLD`/`AUTOSUBGAMEGOLD`/`KILLMONEXPRATE`/`POWERRATE`。⇒ P22/P25/P26/P27 当前生产不可达（实现价值是「内容出现即等价」），但按审计判级仍 **R**（一旦启用即改 HP/DB/数值）。P31 的机制可达但**当前内容不触发**：唯一脚本行 `Market_Def/QFunction-0.txt:25` 是 `SETAUTOGETEXP 1400 1 1`（无地图参数），oracle 在 `sMap` 为空且 `FindMap("")==null` 时走 `ScriptActionError` 并**不设置**字段（本次实测 `ExecutionProcessingSys.cs:2698-2705`）；P21/P32 机制可达，是否被内容触发取决于物品数据（P32 标 spike）。
4. **怪物侧**：43 个 `Run` 覆写的函数区间**全部重测**，与怪物设计件 §1.2 逐条一致；审计判「缺失」34 个、`部分` 7 个、`等价` 2 个。34 个缺失中 15 个 race 在部署 `MonGen` 可达（含 2 个在飞：`BeeQueen`/`SpiderHouse`），19 个数据不可达（含 `Clone` 在飞）。本件只排 `Run` 覆写，**不排** `Operate`/`AttackTarget` 覆写；但会在依赖列标明它们是否同批必须接。
5. **串行是硬约束**：所有怪物批次都改 `crates/world/src/monai.rs` 且消费同一本 `world.rng`，**不能并行**；玩家批次都改玩家 Run 段序（`crates/world/src/world.rs` / `bins/game-svr/src/main.rs`），**同样串行**。若要多 worker 并行，先做 MB-0（可选骨架）把 per-class 入口拆文件；否则按本件批序单线执行。
6. **审计覆盖对账**：本件范围外、审计件仍判 `缺失/部分` 的行，全部在 §2.5 逐条给出「归属批次 / 已落地 / 显式排除 / 另立卡」四类归宿，不再有静默未处置行。
7. **trace 影响**：每个新增取数点都必须在 `mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv` 登记 oracle 侧 `类.Run@文件.cs:行` 与 Rust 侧站点（`monai.rs` / `run_gap.rs` / `combat.rs` 等）。未登记 ⇒ `UNCLASSIFIED`，R 级窗口必红。已落地的 `player_run` 组只覆盖 `CheckDupObj`（`PlayObject.Message.cs:283`），其它玩家段多数不抽数，但 P21 的 `DropItemDown`（`StdMode==45` 的 `Random(Shape)`）必须单列。

---

## 1. 玩家 `Run` 散段（审计 P07/P21/P22/P25/P26/P27/P31/P32）

### 1.1 实测区间与 Rust 现状总表

> 审计件的区间与本次实测有 3 处尾差（只差一个闭合花括号/一行快照），下表以**实测**为准，并在备注写明。

| ID | 审计锚点 | oracle 实测（`PlayObject.Message.cs`） | 段语义 | Rust 现状（grep 证据，文件/符号级） | 主要阻塞 | tier |
|---|---|---|---|---|---|---|
| P07 | 审计件 §2.6 P07 | **143-157** | 20 个 `@OnTimer0..19` | 无 `AutoTimerStatus`/`AutoTimerTick`（`crates`/`bins` 零命中）；`crates/script/src/codes.rs` 只有 `SETONTIMER` CodeDef；`crates/script` 有 `goto_label`，无 `ManageNPC` 脚本宿主 | 管理 NPC 脚本宿主；**疑似死代码，需 spike** | R（若活）/ L（若死） |
| P21 | §2.6 P21 | **403-441**（审计写 403-440；441 是 for 的闭合） | 装备需求每-Run 复检 | `Entity.use_items` 有 13 槽；`bins/game-svr/src/bag.rs::check_take_on_items` 是**全量穿戴校验**、非本段；`bag::take_on`/`take_off`、`ground::drop_item`、`ability::recalc_abilitys` 已有；无 `check_items_need` | 新增 `CheckItemsNeed` 子集；`DropItemDown` 的 StdMode 45 looks 随机化未建模 | R |
| P22 | §2.6 P22 | **442-529**（审计写 443-529；442 是 `tObjCount = GameGold` 快照） | 元宝自动增减/地图旗标 | `SessionGround.game_gold`/`game_point` 存在；`bins/db-srv/src/playdata.rs` 存档；`crates/envir/src/mapinfo.rs::MapInfoFlag` 已解析 `bo_decgamegold/n_decgamegoldtime/...`；`crates/protocol` 有 `RM_GOLDCHANGED`；无 per-tick 字段/逻辑；`crates/script/src/engine/actions.rs` 无 192/193 分支 | 脚本动作 `AUTOADDGAMEGOLD`/`AUTOSUBGAMEGOLD`；`MoveToHome`（**Rust 不存在** ⇒ 缺失依赖；按 `PlayObject.Base.cs:2306-2330` 实现，见 §1.2 P22 依赖） | R |
| P25 | §2.6 P25 | **569-594** | 地图 `DECHP`/`INCHP` | `MapInfoFlag.bo_dechp/n_dechppoint/n_dechptime/bo_inchp/...` 已解析；无消费；`Entity.hp` 与 53 帧下行已有 | 无（只缺行为）；生产 `MapInfo.txt` 0 条 | R |
| P26 | §2.6 P26 | **596-635**（审计写 596-634；635 是 `if (HungerSystem)` 的闭合） | 饥饿递减/扣血 | `SessionGround.hunger_status` + DB 存档；`bins/game-svr/src/pickwire.rs::my_status` + `RM_MYSTATUS`；`Recovery.per_health`/`per_spell` 存在；无 `HungerSystem`/`HungerDecHP` 配置 | 配置键（部署均 0）；`HealthTick/SpellTick` 在 `stamina`/`Recovery` | R |
| P27 | §2.6 P27 | **636-657** | 经验/攻击倍率 1s 递减 | `Entity.power_rate` 已有并被 `combat` 使用；`CombatConfig.kill_mon_exp_rate` 是**全局配置**（`win_exp` 用它）；无 per-player `KillMonExpRate`/`KillMonExpRateTime`/`PowerRateTime`；`crates/script/src/engine/actions.rs` 无 197/198 分支 | per-player 字段 + `win_exp` 改读；脚本动作 | R |
| P31 | §2.6 P31 | **738-745** | 自动经验 | `crates/script/src/engine/actions.rs` 的 `ActionOfAutoGetExp` 已实现，但 `crates/script/src/engine/mod.rs::set_auto_get_exp` 默认空实现；`world::gain_exp`/`combat::win_exp` 已有 | 玩家脚本适配器（`bins/game-svr/src/npc.rs::NpcPlayer` 的 trait impl）；per-tick 结算；**当前唯一脚本行缺地图参数 ⇒ oracle 报错，内容不触发** | R |
| P32 | §2.6 P32 | **747-763** | 复活戒指 | 无 `Revival`/`RevivalTick`/`UnRevival`；无 `ItemDamageRevivalRing`；无 `ItemDamageRevivalMap`；无 `RevivalTime` 配置；`ability::recalc_abilitys` 未建模戒指 shape 标志 | 戒指 shape 标记 + 耐久/RM_DURACHANGE；死亡结算路径 | R |

### 1.2 逐项明细

#### P07 个人定时器（`PlayObject.Message.cs:143-157`）

- **实测**：首行 `for (int i = 0; i < 20; i++) //个人定时器`（143），末行 `}`（157）。
- **语义**：20 槽；`AutoTimerStatus[i] > 500` 且 `now - AutoTimerTick[i] > AutoTimerStatus[i]`；`ManageNPC != null` 时刷新 `AutoTimerTick[i]`、`ScriptGotoCount = 0`、`ManageNPC.GotoLable(this, "@OnTimer" + i, false)`。
- **Rust 现状**：无 `AutoTimerStatus`/`AutoTimerTick`；`crates/script/src/codes.rs` 只有 `SETONTIMER` 的 CodeDef（field 307）；`crates/script` 的 `goto_label` 是引擎内部跳转，不提供「管理 NPC 对玩家调用标签」的宿主入口。
- **必须记下的实测异常**：
  - 全仓 `AutoTimerStatus` 的写入点只有 `PlayObject.cs:1032-1033` 的 ctor 零初始化，`PlayObject.Message.cs:145/147` 只读；
  - `SETONTIMER` 在 `ExecutionCode.cs:620-621` 只有枚举声明，`src/` 无 `case`/handler；
  - 部署 `Envir` 中 `SETONTIMER` 使用文件数 = 0（`grep -a -ril` 只读）。
  ⇒ **P07 在当前 oracle 树与当前内容下不可达**。按「行为等价优先」，盲目实现会引入 oracle 没有的脚本触发。
- **依赖**：若 spike 找到写入者：管理 NPC 脚本宿主 + `GotoLable`；若没有：无。
- **tier**：L（确认死代码）；若发现写入者改 R（时序 + 脚本副作用）。
- **构造级判据**：
  1. **spike（必做）**：`grep -rn "AutoTimerStatus" src --include=*.cs` 必须列出 ctor 之外的写入；`grep -rn "SetOnTimer\|SETONTIMER" src --include=*.cs` 必须有 handler。两者都没有 ⇒ 死代码。
  2. 确认死代码 ⇒ 单测断言：注入 20 槽任意状态/时钟推进 10s，`@OnTimer` 调用计数恒 0；不新增字段、不新增 trace 站点。
  3. 若活 ⇒ 固定时钟；`status[0]=600`，推进 599ms 不调用、601ms 调用一次且 `AutoTimerTick[0]` 被刷新、`ScriptGotoCount=0`；`status<=500` 永不调用。

#### P21 装备需求复检（`PlayObject.Message.cs:403-441`）

- **实测**：首行 `for (int i = 0; i < UseItems.Length; i++)`（403），末行 `}`（441）。
- **语义**：对每个 `UseItems[i]`（Index>0）取 `StdItem`；`CheckItemsNeed(stdItem)` 为 false ⇒ 从装备位移除：`AddItemToBag` 成功则 `SendAddItem + WeightChanged`，失败则 `DropItemDown(item, MIN(Config.DropItemRage,3), false, 0, ActorId)`；一旦移出 ⇒ `SendDelItems`、`Index=0`、`RecalcAbilitys()`。`StdItem==null` ⇒ `Index=0`。
- **`CheckItemsNeed` 的精确子集**（`PlayObject.Base.cs:2832-2882` 实测）：
  - `Need=6`：无行会 ⇒ false；
  - `Need=60`：无行会或 `GuildRankNo != 1` ⇒ false；
  - `Need=7`：不是城堡成员 ⇒ false；
  - `Need=70`：非城堡成员或 `GuildRankNo != 1` ⇒ false；
  - `Need=8`：`MemberType == 0` ⇒ false；
  - `Need=81`：`MemberType != LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel)` ⇒ false；
  - `Need=82`：`MemberType < LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel)` ⇒ false；
  - **其它 Need 一律 true**。
- **Rust 现状**：`Entity.use_items` 已有；`bins/game-svr/src/bag.rs::check_take_on_items` 覆盖 Need 0-13/40-44/60/70/81/82 的**穿戴**校验（含性别/负重/等级），**与本段语义不同**；`TakeOnContext` 已有 `has_guild/guild_rank_no/is_castle_member/member_type/member_level` 一类输入；`bag::take_on/take_off`、`ground::drop_item`、`ability::recalc_abilitys` 可复用。
- **依赖**：新增 `check_items_need`（7 值子集）；`DropItemDown` 的 `StdMode==45` looks 随机化在 `bins/game-svr/src/ground.rs` 明确未建模（其模块「未做清单」）；背包满+掉落时才会触发。
- **tier**：R（装备属性/背包/DB）。
- **构造级判据**：
  1. 固定时钟；在武器格装 `Need=6` 物品、玩家无行会 ⇒ 一次玩家 Run 后：该槽 `Index=0`、物品进背包、`SendDelItems`/`SendAddItem` 各一帧、`RecalcAbilitys` 被调用一次。
  2. 装 `Need=0` 且等级不足的物品 ⇒ **不得**被弹出（证明只走 `CheckItemsNeed` 子集，而非全量穿戴校验）。
  3. `Need=81/82` 用 `MemberType/MemberLevel` 四组边界值验证 `LoWord/HiWord` 比较。
  4. 背包 46 格全满 ⇒ 走 `DropItemDown`（`MIN(DropItemRage,3)`），断言地面出现 `RM_ITEMSHOW`；若物品 `StdMode==45`，登记「looks 随机抽未建模」为已知差异（不判绿）。

#### P22 元宝自动增减 / 地图旗标（`PlayObject.Message.cs:442-529`）

- **实测**：首行 `tObjCount = GameGold;`（442），末行 `}`（529）。审计写 443-529，差异是一行快照。
- **语义**：
  - `BoDecGameGold && now-DecGameGoldTick > DecGameGoldTime` ⇒ 扣 `DecGameGold`；不足则清零、`BoDecGameGold=false`、`MoveToHome()`；刷新 tick。
  - `BoIncGameGold && now-IncGameGoldTick > IncGameGoldTime` ⇒ 加 `IncGameGold`；`GameGold+Inc < 2,000,000` 才加，否则封顶 2,000,000 且 `BoIncGameGold=false`（注意 oracle 的 `nInteger = 2000000 - GameGold` 在已经赋值 2,000,000 之后计算 ⇒ 该日志值恒 0；日志已被注释，不影响帧）。
  - 地图旗标：`!BoDecGameGold && Envir.Flag.boDECGAMEGOLD` + `nDECGAMEGOLDTIME*1000`；`!BoIncGameGold && boINCGAMEGOLD` + `nINCGAMEGOLDTIME*1000`，上限 `<= 2,000,000`。
  - 末尾 `tObjCount != GameGold` ⇒ `SendUpdateMsg(RM_GOLDCHANGED, ...)`。
- **Rust 现状**：`SessionGround.game_gold`/`game_point`；DB 存档字段 `game_gold` 已在 `bins/db-srv/src/playdata.rs`；`MapInfoFlag` 的四个旗标与时间/点数已解析；`RM_GOLDCHANGED` 常量在 `crates/protocol`；**无** `BoDec/IncGameGold`、无 tick 字段、无 per-tick 分支。`crates/script/src/engine/actions.rs` 只在 `handlers.rs` 注册 192/193 名称，无实现分支。
- **可达性（本次只读）**：部署 `Envir` 中 `AUTOADDGAMEGOLD`/`AUTOSUBGAMEGOLD` 使用文件数 = 0；`MapInfo.txt` 中 `DECGAMEGOLD/INCGAMEGOLD` = 0 条 ⇒ 生产不可达。
- **依赖**：脚本动作 192/193（`AutoAddGameGold`/`AutoSubGameGold`）；**`MoveToHome` 在 Rust 不存在（缺失依赖）**，按 `PlayObject.Base.cs:2306-2330` 实现：`SendRefMsg(RM_SPACEMOVE_FIRE)` + `BaseObjectMove(HomeMap,HomeX,HomeY)`；`BaseObjectMove`：空图 ⇒ 当前图，`sX!=0 && sY!=0` ⇒ `SpaceMove`，否则 `MapRandomMove`，跨图且 `Race==Play` ⇒ 清 `IsTimeRecall`；`RM_GOLDCHANGED` 下行（`bins/game-svr/src/downlink.rs`/`main.rs`）。
- **tier**：R（DB 写入 + 状态）。
- **构造级判据**：`BoDecGameGold=true, DecGameGold=30, DecGameGoldTime=1000, GameGold=100` → 1001ms 后 70 且 tick 刷新；再推进 1001ms → 40；`GameGold=10` → 清零、flag=false、`MoveToHome` 被调用；`BoIncGameGold` 封顶 2,000,000；地图旗标支各一条；仅实际变化时发 `RM_GOLDCHANGED`；存档记录中的 `game_gold` 与内存一致。

#### P25 地图 HP 事件（`PlayObject.Message.cs:569-594`）

- **实测**：首行 `if (Envir.Flag.boDECHP && ...)`（569），末行 `}`（594）。
- **语义**：`boDECHP` + `nDECHPTIME*1000` ⇒ `HP -= nDECHPPOINT`，不足则 `HP=0`，`HealthSpellChanged()`；`boINCHP` + `nINCHPTIME*1000` ⇒ **oracle bug：用 `nDECHPPOINT` 判上限并加血**（`:585-587`），`HP+point < MaxHP` 才加，否则 `HP=MaxHP`，`HealthSpellChanged()`。
- **Rust 现状**：`MapInfoFlag` 的 `bo_dechp/n_dechppoint/n_dechptime/bo_inchp/n_inchppoint/n_inchptime` 已解析；无消费；`Entity.hp`/`settle_deaths` 与 53 帧已存在。
- **可达性（本次只读）**：`MapInfo.txt` 0 条 `DECHP/INCHP` ⇒ 生产不可达。
- **依赖**：无新增系统；只需 map flag 读取 + 玩家 Run 段。
- **tier**：R（HP 轨迹/死亡时点）。
- **构造级判据**：注入 `boDECHP=true, nDECHPPOINT=10, nDECHPTIME=1`，HP=100 → 1001ms 后 90；HP=10 → 0 并在下一 tick 死亡结算；`boINCHP` 支用 `nDECHPPOINT` 而非 `nINCHPPOINT` 的 bug 必须显式钉住（改坏必红）；每次变更一次 53。

#### P26 饥饿系统（`PlayObject.Message.cs:596-635`）

- **实测**：首行 `if (SystemShare.Config.HungerSystem)`（596），末行 `}`（635）。
- **语义**：1s 门；`HungerStatus > 0` ⇒ `tObjCount = GetMyStatus(); HungerStatus -= 1;` 若 `GetMyStatus()` 变化 ⇒ `RefMyStatus()`；`HungerStatus <= 0` 且 `HungerDecHP` ⇒ `HealthTick -= 60; SpellTick = MAX(0, SpellTick-10); PerHealth -= 1; PerSpell -= 1;` 若 `HP > HP/100` ⇒ `HP -= MAX(1, HP/100)`；否则（即 `HP <= HP/100`，正 HP 不可达）`HP <= 2 ⇒ HP=0`；`HealthSpellChanged()`。
- **Rust 现状**：`SessionGround.hunger_status`（`CM_EAT` 写入）+ DB 存档；`bins/game-svr/src/pickwire.rs::my_status` 与 `RM_MYSTATUS` 已有；`Recovery.per_health/per_spell` 已有；**无** `HungerSystem`/`HungerDecHP` 配置键（`bins/game-svr/src/config.rs`、`crates/envir/src/conf.rs` 零命中）。
- **可达性（本次只读）**：部署 `Server.conf:396 HungerSystem=0`、`:398 HungerDecHP=0` ⇒ 生产不可达。
- **依赖**：两个配置键；`RefMyStatus` 帧；`HealthTick/SpellTick` 的归属（`Stamina`/`Recovery`）。
- **tier**：R（HP/DB）。
- **构造级判据**：打开 `HungerSystem`；`HungerStatus=1001` → 1s 后 1000，`GetMyStatus` 仍为 1 ⇒ **不发**帧；再 1s 到 999 ⇒ `GetMyStatus` 由 1 变 0 ⇒ 一帧 `RM_MYSTATUS`；`HungerStatus=0` 且打开 `HungerDecHP` → `HealthTick-60`、`SpellTick=max(0,-10)`、`PerHealth/PerSpell` 各 -1、`HP -= max(1,HP/100)`（HP=100 → 99；HP=2 → 1——`HP > HP/100` 对正 HP 恒真，`HP<=2 ⇒ 0` 支对正 HP 不可达）、53；DB `HungerStatus` 与内存一致。

#### P27 经验/攻击倍率计时（`PlayObject.Message.cs:636-657`）

- **实测**：首行 `if ((HUtil32.GetTickCount() - ExpRateTick) > 1000)`（636），末行 `}`（657）。
- **语义**：1s 门；`KillMonExpRateTime > 0` ⇒ 减 1，归零时 `KillMonExpRate=100` + SysMsg「经验倍数恢复正常...」；`PowerRateTime > 0` ⇒ 减 1，归零时 `PowerRate=100` + SysMsg「攻击力倍数恢复正常...」。
- **Rust 现状**：`Entity.power_rate` 是 per-player 字段，`combat::get_attack_power_player` 已用它；`CombatConfig.kill_mon_exp_rate` 是**全局**，`combat::win_exp` 读它（`crates/world/src/combat.rs`）；**无** `KillMonExpRateTime`/`PowerRateTime`/per-player `KillMonExpRate`；`crates/script/src/engine/actions.rs` 无 `ActionOfKillMonExpRate`/`ActionOfPowerRate` 分支；oracle 的使用点是 `PlayObject.Base.cs:140`（`WinExp` 乘 `KillMonExpRate`）与 `PlayObject.cs:2938`（`PowerRate` 乘攻击力）。
- **可达性（本次只读）**：部署 `Envir` 中 `KILLMONEXPRATE`/`POWERRATE` 使用文件数 = 0 ⇒ 当前生产倍率恒 100、计时段不可达。
- **依赖**：per-player `KillMonExpRate`（替换 `CombatConfig` 的全局读取）+ 两个 Time 字段；脚本动作 197/198；两条 SysMsg 帧。
- **tier**：R（经验/伤害数值）。
- **构造级判据**：设 `KillMonExpRate=200, KillMonExpRateTime=2` → 1s 后 time=1/rate=200；2s 后 rate=100 + 一条 SysMsg；`PowerRate=150, PowerRateTime=1` 同理；构造一次 `win_exp`：到期前经验×2、到期后×1（证明 `win_exp` 已改读 per-player）；脚本动作 197/198 各一条设置用例。

#### P31 自动经验（`PlayObject.Message.cs:738-745`）

- **实测**：首行 `if (AutoGetExpPoint > 0 && ...)`（738），末行 `}`（745）。
- **语义**：`AutoGetExpPoint > 0` 且 (`AutoGetExpEnvir == null` 或 == 当前 `Envir`) 且 `now - AutoGetExpTick > AutoGetExpTime` ⇒ 刷新 tick；若 `!AutoGetExpInSafeZone` 或 `InSafeZone()` ⇒ `GetExp(AutoGetExpPoint)`。
- **Rust 现状**：`crates/script/src/engine/actions.rs` 的 `ActionOfAutoGetExp` 已实现（含 `nTime<=0 || nPoint<=0` 错误分支），最终调 `set_auto_get_exp`；`crates/script/src/engine/mod.rs` 该 trait 方法是**默认空实现**；`bins/game-svr/src/npc.rs::NpcPlayer` 的 `ScriptPlayer` impl 未覆盖它。`world::gain_exp` / `combat::win_exp` 已有。
- **可达性（本次只读）**：部署唯一脚本行 `Market_Def/QFunction-0.txt:25` 是 `SETAUTOGETEXP 1400 1 1`（无地图参数）。oracle `ExecutionProcessingSys.cs:2698-2705` 在 `sMap` 为空且 `FindMap("")==null` 时 `ScriptActionError` 并**不设置** `AutoGetExp*` 字段；Rust `ActionOfAutoGetExp` 用 `map_exists("")` 复刻同一结果。⇒ **当前内容不触发**；实现仍按 R 保等价（字段设置/结算一旦被有地图的脚本或 GM 注入触发）。
- **依赖**：玩家脚本适配器把四字段写进会话态；玩家 Run 段做 per-tick 结算；安全区判定（`InSafeZone` 已有？若无需接线，标 spike）。
- **tier**：R（经验值/帧）。
- **构造级判据**：以**带地图参数**的脚本注入（部署那行无地图、按 oracle 会报错，见上），使适配器收到 `(safe_zone=true, envir=None, time=1_400_000, point=1)`；推进 1_399_999ms 不加经验，1_400_001ms 加 1 并刷新 tick；`AutoGetExpInSafeZone=true` 且不在安全区 ⇒ 不加；`AutoGetExpEnvir=Some(mapA)` 且玩家在 mapB ⇒ 不加；每次加经验产生 `SM_WINEXP(44)`。

#### P32 复活戒指（`PlayObject.Message.cs:747-763`）

- **实测**：首行 `if (!Death)`（747），末行 `}`（763）。
- **语义**：`!Death && HP==0`；`LastHiter == null` 或 (`LastHiter.Race == Play && !UnRevival`)；且 `Race == Play && Revival && now-RevivalTick > Config.RevivalTime` ⇒ `RevivalTick=now`、`ItemDamageRevivalRing()`、`HP=MaxHP`、`HealthSpellChanged()`、SysMsg `RevivalRecoverMsg`（绿色 Hint）。
- **`ItemDamageRevivalRing` 精确语义**（`PlayObject.cs:5355-5392` 实测）：遍历 `UseItems`；`ItemDamageRevivalMap = {114,160,161,162}`（`M2Share.cs:186`），命中 `Shape`，或命中武器/右手位的 `AniCount` ⇒ **`ushort` 无符号回绕**：`ushort nDura = Dura; ushort tDura = (ushort)Round(Dura/1000.0); nDura -= 1000;` ⇒ `Dura=0 ⇒ 64536`、`Dura=999 ⇒ 65535`、`Dura=1000 ⇒ 0`；**仅 `Dura==1000`（回绕后 `nDura==0`）** ⇒ `SendDelItems`、`Index=0`、`RecalcAbilitys()`；`Dura<1000` 时写回 `Dura+64536`（0⇒64536、999⇒65535）且**仍留在装备位**。`RM_DURACHANGE` 条件是 **`tDura != Round(nDura/1000.0)`**，其中 `tDura = Round(Dura/1000.0)` 已是千位值（**不是** `Round(tDura/1000)`）；`Dura∈[1000,1499]` 等边界按此式判定。
- **Rust 现状**：无 `Revival`/`RevivalTick`/`UnRevival`；无 `ItemDamageRevivalRing`；无 `ItemDamageRevivalMap`；无 `RevivalTime` 配置；`ability::recalc_abilitys` 未建模戒指 shape 标志（`RING_REVIVAL_ITEM` 分支）。
- **可达性**：部署 `Server.conf:739 RevivalTime=60000`；戒指物品存在与否需 data 抽样（标 spike，不阻塞实现）。
- **依赖**：`RecalcAbilitys` 的戒指 shape 分支（`PlayObject.cs:2242`）；`ItemDamageRevivalRing` + `RM_DURACHANGE`；死亡结算路径（`settle_deaths`）与 `LastHiter`（`Entity.last_hiter` 已有）；`UnRevival` 玩家标志。
- **tier**：R（死亡/HP/耐久/DB）。
- **构造级判据**：装备 `Shape=114` 戒指后 `RecalcAbilitys` ⇒ `Revival=true`；HP=0、`!Death`、`LastHiter=null`、`RevivalTick=0` ⇒ 一 tick 后 `HP=MaxHP`、`RevivalTick=now`、耐久 -1000、千位变化时 `RM_DURACHANGE`；耐久边界必须按 ushort 回绕断言：`Dura=1000 ⇒ 0` 才移除 + recalc，`Dura=999 ⇒ 65535` 且留在装备位，`Dura=0 ⇒ 64536` 且留在装备位；`RM_DURACHANGE` 仅在 `tDura != Round(nDura/1000.0)` 时发（含 `Dura∈[1000,1499]` 边界用例）；60s 内第二次死亡不复活；`LastHiter` 为玩家且 `UnRevival=true` 不复活。

### 1.3 玩家批次

> **PR-3 是 PL-1..PL-4 的硬前置**：字段归属（`SessionGround` vs `Entity`）未定前不得开工。玩家侧所有段都插入**玩家 Run 段序**（当前 `World::tick` 中已有的顺序：死亡结算 → 宝宝自投递 → extra_abil → 事件 → 宝宝清表 → buff → 怪物 AI → NPC → 商人 → FireHit/TwinHit → CheckDupObj → VerifyMapTime）。新增段必须落在 oracle 对应位置；同文件/同函数 ⇒ 批次串行。

#### PL-1 装备复检 + 复活戒指（P21 + P32）——优先
- **为什么同批**：两者都改 `RecalcAbilitys` 的装备 shape/需求语义与死亡/HP 路径；共享 `bins/game-svr/src/ability.rs`、`bins/game-svr/src/bag.rs`、`crates/world/src/entity.rs`/`world.rs`。
- **写集（文件级）**：`crates/world/src/entity.rs`（复活/戒指状态）、`crates/world/src/world.rs`（玩家 Run 段、死亡前复活门）、`bins/game-svr/src/ability.rs`（recalc 戒指 shape）、`bins/game-svr/src/bag.rs`（`check_items_need` 子集）、`bins/game-svr/src/ground.rs`（掉落路径，looks 未建模登记）、`bins/game-svr/src/main.rs`（`SessionGround`/下行 53/`RM_DURACHANGE`）、`bins/game-svr/src/config.rs`/`crates/envir/src/conf.rs`（`RevivalTime`）。
- **串行约束**：与 PL-2/PL-3/PL-4 全部串行（共享玩家 Run 段序与 `main.rs`）；尤其不能与任何动 `ability::recalc_abilitys` 的线并行（写集冲突）。
- **验收判据**：§1.2 P21/P32 的构造级用例全绿；R 级真机窗口：`window_required: true`（装备需求回包帧 + 复活链 HP/耐久/DB 需真机对拍）。
- **完成定义**：构造级测试 + `cargo test --workspace` + 真机窗口（或 owner 对不可达分支的显式裁决）；trace：P21 的 `DropItemDown` looks 抽如实现则登记 `drop`/新组，否则在报告写明未建模且当前不可观测；P32 无新 RNG。

#### PL-2 脚本驱动（P31 + P07）
- **为什么同批**：都依赖脚本引擎的玩家适配器/标签调用；P07 先 spike，若死代码则本批只剩 P31。
- **写集（文件级）**：`crates/script/src/engine/mod.rs`（trait 方法语义）、`crates/script/src/engine/actions.rs`（P07 若活）、`bins/game-svr/src/npc.rs`（`ScriptPlayer` impl）、`bins/game-svr/src/main.rs`（会话态字段 + tick）、`crates/world/src/entity.rs`/`world.rs`（P31 结算）、`bins/game-svr/src/downlink.rs`（`SM_WINEXP`）。
- **串行约束**：与 PL-1/PL-3/PL-4 串行；与任何改 `crates/script` 的车道串行（同 crate 写集）。
- **验收判据**：P31 构造级（§1.2）+ 部署 `SETAUTOGETEXP 1400 1 1` 的「报错且字段不变」用例 + 有地图参数注入的「字段设置」用例；P07 spike 结论落档；R 级窗口 `window_required: true`（P31 经验/帧）；P07 若死代码则 L 收口。
- **完成定义**：spike 记录（grep 命令 + 输出）进报告；P31 适配器 + tick 用例（含「无地图参数 ⇒ ScriptActionError、字段不变」与「有地图参数 ⇒ 字段设置」两态）；P07 若不实现，在审计件状态回写「N/A（死代码）」并保留证据。

#### PL-3 1s 周期状态（P27 + P25 + P26）
- **为什么同批**：三者都是「每玩家 1s/定时门」的状态段，写集高度重叠（`Recovery`/`Entity`/配置/53 帧）。
- **写集（文件级）**：`crates/world/src/entity.rs`（rate/time/hp tick 状态）、`crates/world/src/world.rs`（玩家 Run 段）、`crates/world/src/combat.rs`（`win_exp` 改读 per-player rate）、`bins/game-svr/src/main.rs`（`SessionGround`/`Recovery`/下行 53/SystemMsg）、`bins/game-svr/src/config.rs` + `crates/envir/src/conf.rs`（Hunger 键）、`crates/script/src/engine/actions.rs`（197/198 动作）。
- **串行约束**：与 PL-1/PL-2/PL-4 串行；`world.rs` 段序冲突。
- **验收判据**：§1.2 P25/P26/P27 构造级；P25 的 `INCHP` bug 用例必须显式；P26 配置默认 0 ⇒ 生产零行为（回归门）；P27 `win_exp` 改读 per-player 后既有 golden 数字不变（当 rate=100）；R 级窗口 `window_required: true`（P27 数值；P25/P26 内容不可达 ⇒ owner 裁决替代证据）。
- **完成定义**：构造级 + 既有 golden 不漂移 + 报告写明「生产不可达」的证据（Server.conf/MapInfo.txt/脚本 grep）。

#### PL-4 元宝/游戏点自动增减（P22）
- **为什么单独**：唯一触及 DB 金币轨迹与 `MoveToHome` 的段；写集偏宿主/脚本。
- **写集（文件级）**：`bins/game-svr/src/main.rs`（`SessionGround` 的 Bo*/tick）、`bins/game-svr/src/downlink.rs`（`RM_GOLDCHANGED`）、`bins/game-svr/src/homemap.rs`（**新增** `MoveToHome`；Rust 现仅有 `parse_start_points`/`get_home_info`/`new_human_spawn*`）、`crates/script/src/engine/actions.rs`（192/193）、`bins/db-srv/src/playdata.rs`（已存档，无改动或仅测试）。
- **串行约束**：与 PL-1/PL-2/PL-3 串行；与任何改 `SessionGround`/`main.rs` 的线串行。
- **验收判据**：§1.2 P22 构造级 + DB 存档一致性；R 级窗口 `window_required: true`（金币轨迹/DB）；生产不可达证据入报告。
- **完成定义**：构造级 + 存档回读 + 帧序（`RM_GOLDCHANGED` 仅在变化时）。

---

## 2. 34 个专用怪 `Run` 覆写（审计 R02–R43）

> 本节只排 `Run` 覆写。`Operate`（O01–O11）与 `AttackTarget`/`Attack`/`Die` 等覆写**不单独排**；但凡批次依赖它们，会在「依赖」列点名（例如 R03/R37 的 `RM_ZEN_BEE` 消费端是 O03/O05，二者在飞；R43 依赖 `Die()` 覆写的抽数）。

### 2.1 实测清册（34 个缺失项）

> 全部区间用「函数签名行 + 花括号配对」脚本实测，与怪物设计件 §1.2 逐条一致。状态列来自审计件 §2.4（`缺失` = 本表全部；`部分`/`等价` 不在本表）。

| ID | 类（`CreateMonster` race） | oracle 实测 `Run` | 本件批次 |
|---|---|---|---|
| R02 | `ArcherGuard`（112；`ArcherPolice` 20 继承） | `ArcherGuard.cs:39-91` | MB-3 |
| R03 | `BeeQueen`（103） | `BeeQueen.cs:47-74` | **在飞 O03，不排** |
| R04 | `BigHeartMonster`（115） | `BigHeartMonster.cs:44-54` | MB-1 |
| R05 | `BoneKingMonster`（134） | `BoneKingMonster.cs:48-76` | MB-7 |
| R07 | `CentipedeKingMonster`（107；`: StickMonster`） | `CentipedeKingMonster.cs:91-143` | MB-3 |
| R08 | `ChickenDeer`（52） | `ChickenDeer.cs:13-63` | MB-2 |
| R09 | `CloneMonster`（201） | `Clone.cs:62-88` | **在飞 O04，不排** |
| R10 | `CowKingMonster`（92） | `CowKingMonster.cs:40-94` | MB-4 |
| R12 | `DigOutZombi`（95） | `DigOutZombi.cs:18-55` | MB-1 |
| R13 | `DualAxeMonster`（87；`ThornDarkMonster` 93、`ArcherMonster` 104 继承） | `DualAxeMonster.cs:84-137` | MB-2 |
| R14 | `ElectronicScolpionMon`（200） | `ElectronicScolpionMon.cs:33-70` | MB-6 |
| R15 | `ElfMonster`（113） | `ElfMonster.cs:43-86` | MB-7 |
| R16 | `ElfWarriorMonster`（114；`: SpitSpider`） | `ElfWarriorMonster.cs:112-166` | MB-7 |
| R17 | `ExplosionSpider`（117） | `ExplosionSpider.cs:79-90` | MB-6 |
| R18 | `FireMonster`（214） | `FireMonster.cs:23-82` | MB-8 |
| R19 | `FireballMonster`（215；`: MagicMonster`） | `FireballMonster.cs:14-62` | MB-9 |
| R20 | `FrostTiger`（210） | `FrostTiger.cs:14-36` | MB-5 |
| R21 | `GasMothMonster`（105；`: GasAttackMonster`） | `GasMothMonster.cs:24-35` | MB-3 |
| R22 | `GreenMonster`（208） | `GreenMonster.cs:13-36` | MB-5 |
| R23 | `Khazard`（206） | `Khazard.cs:13-59` | MB-5 |
| R24 | `LightingZombi`（94） | `LightingZombi.cs:32-58` | MB-2 |
| R25 | `MagicMonObject`（133） | `MagicMonObject.cs:23-60` | MB-6 |
| R29 | `RedMonster`（209） | `RedMonster.cs:13-36` | MB-5 |
| R31 | `RonObject`（131） | `RonObject.cs:35-51` | MB-6 |
| R32 | `RunAway`（死类，`CreateMonster` 无 case） | `RunAway.cs:12-64` | MB-8 |
| R33 | `SandMobObject`（132；`: StickMonster`） | `SandMobObject.cs:14-57` | MB-8 |
| R34 | `ScultureKingMonster`（102） | `ScultureKingMonster.cs:61-115` | MB-3 |
| R35 | `ScultureMonster`（101） | `ScultureMonster.cs:43-79` | MB-1 |
| R36 | `SoccerBall`（120） | `SoccerBall.cs:19-74` | MB-8 |
| R37 | `SpiderHouseMonster`（116） | `SpiderHouseMonster.cs:51-78` | **在飞 O05，不排** |
| R38 | `StickMonster`（85） | `StickMonster.cs:101-149` | MB-1 |
| R40 | `TeleMonster`（203） | `TeleMonster.cs:12-31` | MB-5 |
| R42 | `WhiteSkeleton`（100） | `WhiteSkeleton.cs:29-39` | MB-8 |
| R43 | `ZilKinZombi`（96；`: AtMonster`） | `ZilKinZombi.cs:35-47` | MB-2 |

> **实测核对**：43 个 `Run` 的区间均由「签名行 + 花括号配对」脚本输出；下表 34 项的首行均为 `public override void Run()`、末行均为 `}`（复现脚本见 §7.2）。R01/R06/R11/R26/R27/R28/R30/R39/R41 等「部分/等价」项不在本表，避免与在飞/已落地车道重复。

> **在飞项边界**：R03/R09/R37 的 `Run` 体与 O03/O04/O05 是同一实体的两面；在飞车道若只做 `Operate`，本件建议在其交付说明中显式登记「Run 体仍未覆盖」，不要用 `Operate` 的完成冒充 `Run` 完成。R03/R37 的 `Run` 依赖自延时队列（PR-1）与生成回调；R09 依赖 `Operate` 的 `return false` 消息泵语义。

### 2.2 可达性排序（真机可生成 + 真机可达）

**数据源**：怪物设计件 §2.4（2026-10-17 对部署 `MonGen.txt` + MySQL `mir2_data.monsters` 的只读实测）；本次在基线 `706c95cae` 上对 `MonGen.txt` 做 GBK 只读抽检（按名字行匹配），与设计件结论一致：`角蝇 98`、`幻影蜘蛛 8`、`食人花 5`、`千年树妖+赤月恶魔 4`、`祖玛教主 3`、`恶魔弓箭手 21`、`鹿+羊 14`、`祖玛卫士 176`（样本名）、`月魔蜘蛛 23`、`僵尸1 39`/`电僵王 8`、`僵尸2 42`/`石墓尸王 3`、`僵尸3 41`/`僵尸4 39`、`双头血魔 6`/`暗之双头血魔 3`、`半兽统领 1`/`掷斧骷髅 16`、`暗黑战士 16`/`暴牙蜘蛛 34`、`祖玛弓箭手 153`/`魔龙射手 1`。

**可达（13 个；下表按设计件条目数降序给出优先级依据，实际批次按依赖聚类，见 §2.3）**：

| 序 | race | 类 | 设计件条目数 | 说明 |
|---:|---:|---|---:|---|
| 1 | 101 | `ScultureMonster` | 338 | 全表最高；`Run` 无直接 RNG，只有 `MeltStoneAll`/搜索 |
| 2 | 105 | `GasMothMonster` | 279 | `Run` 无直接 RNG；命中路径 `GasAttack` 有 `Random(3)` |
| 3 | 96 | `ZilKinZombi` | 120 | `Run` 无直接 RNG；`Die()` 有 `Random(20)+4` |
| 4 | 87/93/104 | `DualAxeMonster` + 继承 | 18 + 50 + 169 | `Run` 直接 `Random(5)`；继承类共用同一 `Run` |
| 5 | 94 | `LightingZombi` | 47 | `Run` 直接 `Random(3)` + `LightingAttack` 的 `Random(DC)` |
| 6 | 95 | `DigOutZombi` | 45 | `Run` 无直接 RNG |
| 7 | 112 | `ArcherGuard` | 21（另有 57 条同名缺失记录，出生失败） | 攻击 helper `Random(DC)` |
| 8 | 107 | `CentipedeKingMonster` | 19 | `AttackTarget` helper `Random(4)/Random(3)` |
| 9 | 92 | `CowKingMonster` | 18 | `SpaceMove`/`MapRandomMove` 抽数；无直接 RNG |
| 10 | 52 | `ChickenDeer` | 13 | 无 RNG；含两处变量 bug |
| 11 | 85 | `StickMonster` | 5 | 无 RNG；是 107/132 的基类 |
| 12 | 115 | `BigHeartMonster` | 4 | 攻击 helper `Random(DC)` |
| 13 | 102 | `ScultureKingMonster` | 3 | `CallSlave` `Random(6)+6`/`Random(4)` |

**在飞（3 个，不排）**：103 `BeeQueen`（98）、116 `SpiderHouseMonster`（8）、201 `CloneMonster`（0）。

**数据不可达（18 个，按 `Run` 体依赖聚类）**：200 `ElectronicScolpionMon`、113 `ElfMonster`、114 `ElfWarriorMonster`、117 `ExplosionSpider`、214 `FireMonster`、215 `FireballMonster`、210 `FrostTiger`、208 `GreenMonster`、206 `Khazard`、133 `MagicMonObject`、209 `RedMonster`、131 `RonObject`、132 `SandMobObject`、120 `SoccerBall`、203 `TeleMonster`、134 `BoneKingMonster`、`RunAway`（死类）、100 `WhiteSkeleton`（race 100 条目 0）。110/111（城门/城墙）由城堡系统放置，不属本表 34 项。

> **可达性口径（2026-10-21 写死）**：本段「不可达」= **部署 `MonGen.txt` 无该 race 映射**，不等于无法创建——库内有 race 行 + `CreateMonster` 有 case 时仍可由 GM/脚本按名创建（例 race 120：`sql/mir2_data.sql:573` + `WorldServer.MonGen.cs:683`；本次未连库，见 audit B02 注）。

> **判级口径**：即使数据不可达，只要 `Run` 体有 RNG/时序/战斗数值，仍按 **R**；只有 R42（纯首次帧）按 **L**。不可达项的「真机窗口」没有对象，**不得**用「构造级绿」替代 R 级窗口结论；review 必须写 `window_required: true` 并说明「内容不可达，需 owner 裁决替代证据」。

### 2.3 批次明细

> **共同写集**：每个 MB 批次都改 `crates/world/src/monai.rs`（per-race `Run` 体 + 分发）与 `crates/world/src/entity.rs`（缺失字段）；`bins/game-svr/src/mongen.rs` 的 `draw_monster_construct` **已覆盖全部可达 race 的 ctor 抽数**（本次复核：race 85/95/96/101/102/105/107/112/115/87/93/104/92/52 均有对应分支），**批次不重抽**；只补非抽数常量中真正缺的项（`ComeOutValue`/`AttackRange`/`UseMagic`/`DangerLevel`/`SiegeLockCount`/`N550`/`GuardDirection`/CowKing Crazy 六字段；`stone_mode`/`fixed_hide_mode`/`view_range`/`animal` 已在 `Entity`，不重复落地）。
> **共同串行**：全部 MB 批次消费同一 `world.rng`，且都在 `monai.rs` 的同一 tick 路径上 ⇒ **严格串行**。批内顺序即 oracle 的类链顺序（例如 R38 先于 R07/R33，因为后者 `: StickMonster`）。

#### 2.3.0 非-`Run` 覆写依赖清单（不单独排，但同批必接）

> 本件只排 `Run`；以下覆写不是 `Run`，却是对应 `Run` 的**虚拟调用目标或状态写点**。漏接会让 `Run` 的抽数/帧/数值失真，因此标为「同批必接」，但仍不单独立批次。

| 批次 | 类 | 非-`Run` 覆写（实测） | 为什么必须同批 |
|---|---|---|---|
| MB-2 | `DualAxeMonster` 87（93/104 继承） | `AttackTarget` `:33-72`（`:52` `Random(5)`、`FlyAxeAttack`） | `MonsterObject.Run` 的 `AttackTarget()` 是虚调用；不接则 5 格内攻击路径的 RNG/帧全错 |
| MB-2 | `ZilKinZombi` 96 | `Die` `:24-33` | `ZilKillCount -= 1`、`ReAliveTick`、`ZilkinDieTick = (Random(20)+4)*1000`；不接则 `Run` 的复活门永不成立 |
| MB-3 | `CentipedeKingMonster` 107 | `AttackTarget` `:42-83`、`FindAttackTarget` `:85-89` | `Run` 的 `AttackTarget`/`FindAttackTarget` 调用目标；`Random(4)/Random(3)`、毒、HP 回满都在这里 |
| MB-3 | `ScultureKingMonster` 102 | `Attack` `:55-59` | base AI 的 `Attack` 虚调用；`GetAttackPower` + `HitMagAttackTarget` |
| MB-4 | `CowKingMonster` 92 | `Attack` `:27-31`、`Initialize` `:33-38` | `Initialize` 保存 `oldHitTime/oldWalkTime`；不接则 Crazy 到期恢复为 0 |
| MB-6 | `ExplosionSpider` 117 | `AttackTarget` `:47-77` | 邻接目标时也调 `DoSelfExplosion`；与 `Run` 的 60s 自爆共用同一 helper/RNG |
| MB-7 | `BoneKingMonster` 134 | `Attack` `:42-46` | base AI 攻击走 `HitMagAttackTarget`；不接则伤害/帧不同 |
| MB-7 | `ElfWarriorMonster` 114 | `AttackTarget` `:72-103`、`RecalcAbilitys` `:35` | 强化形态（`Appr 704/706/708`）用 `SpitAttack`；形态速度在 `RecalcAbilitys` |
| MB-7 | `ElfMonster` 113 | `RecalcAbilitys` `:31` | 形态速度/攻击间隔 |
| MB-8 | `SoccerBall` 120 | `Struck` `:76-86` | `Random(4)` + `N550` 写点；不接则 `Run` 永远走 `N550=0` 支 |
| MB-8 | `WhiteSkeleton` 100 | `RecalcAbilitys` `:21-27` | `NextHitTime = 3000 - SlaveMakeLevel*600`、`WalkSpeed = 1200 - SlaveMakeLevel*250`、`WalkTick=now+2000` |
| MB-9 | `FireballMonster` 215 | **`MagicMonster.Run`/`Think`/`AttackTarget`（R26，审计判部分）** | `Fireball` 的 `base.Run()` 会再跑一遍 `MagicMonster.Run`；两 pass、无 HolySeize 门的 `Think`、不伤害的 `AttackTarget` 必须一起建模 |
| MB-3 | `GasMothMonster` 105 | `GasAttack` `:14-22` | 命中路径的 `Random(3)`/透明状态；`Run` 只负责索敌 |

#### MB-1 出土/石像/近身（可达；R04 的 helper 有抽数）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R38 | `StickMonster` 85 | `StickMonster.cs:101-149` | `CanMove && (now-WalkTick)>WalkSpeed` ⇒ 刷新 WalkTick；`FixedHideMode` ⇒ `CheckComeOut()` 真则 `FindAttackTarget()`；否则 `NextHitTime` 门 ⇒ `SearchTarget`；目标为空或任一轴 > `AttackRange` ⇒ `ComeDown()`；否则 `AttackTarget()` 真 ⇒ `base.Run(); return`。 | ctor 常量 `ViewRange=7/ComeOutValue=4/AttackRange=4/FixedHideMode=true/Animal=true`（怪物设计件 §1.2 #38）；`CheckComeOut`/`FindAttackTarget`/`ComeDown` 未建模；AnimalObject 链（**无** MonsterObject AI） | R |
| R12 | `DigOutZombi` 95 | `DigOutZombi.cs:18-55` | `WalkTick >` 门；`FixedHideMode` ⇒ 扫 `VisibleActors`，proper 且 `!HideMode\|\|CoolEye` 且两轴 ≤3 ⇒ `Turned(); WalkTick = now + 1000; break`；非隐藏 ⇒ 8s/1s 索敌。 | ctor `ViewRange=7/SearchTime=2500+/FixedHideMode=true`（`:12-15`）；`Turned` 未建模 | R |
| R35 | `ScultureMonster` 101 | `ScultureMonster.cs:43-79` | `WalkTick >=` 门；`StoneMode` ⇒ 扫两轴 ≤2 的 proper 目标 ⇒ `MeltStoneAll(); break`；否则 8s/1s 索敌。 | ctor `ViewRange=7/StoneMode=true`（`:11-13`）；`MeltStoneAll` 未建模，须完整复刻 `ScultureMonster.cs:25-42`：先 `MeltStone()`（`CharStatusEx=0; CharStatus=GetCharStatus(); SendRefMsg(RM_DIGUP,Dir,CurrX,CurrY,0); StoneMode=false`），再 `GetMapBaseObjects(Envir,CurrX,CurrY,7)` 对每个 `StoneMode && is ScultureMonster` 的邻接对象调其 `MeltStone()`（7 格邻接融化） | R |
| R04 | `BigHeartMonster` 115 | `BigHeartMonster.cs:44-54` | `CanMove && VisibleActors.Count>0` ⇒ `AttackTarget()`；helper（`:15-42`）在 `NextHitTime` 门内抽 `Random(\|Hi(DC)-Lo(DC)\|+1)`，对每个 `ViewRange` 内 proper 目标发 `SendSelfDelayMsg(RM_DELAYMAGIC, …, 200)` + `RM_10205`。 | PR-1（自延时 200ms）；`RM_10205` 帧；ctor `ViewRange=16/Animal=false` | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`（`come_out_value`/`attack_range`/`find_attack_target` 状态）、`bins/game-svr/src/mongen.rs`（常量落到 `MonsterSpawnRequest`）、`bins/game-svr/src/smframe.rs`（`RM_DIGUP`/`RM_10205`）、`bins/game-svr/src/main.rs`（下行）。
- **验收判据（构造级）**：
  1. 固定时钟、spawn race 85/95/101/115；分别断言 `FixedHideMode`/`StoneMode` 初值与 ctor 常量。
  2. R38：`FixedHideMode=true` + 目标在 `ComeOutValue` 内 ⇒ 一 tick 后 `FindAttackTarget` 被调、`FixedHideMode=false`；目标超出 `AttackRange` ⇒ `ComeDown`；`AttackTarget` 命中 ⇒ 只跑一次 `base.Run`（早退）。
  3. R12：目标 ≤3 且可见 ⇒ `Turned` 一次、`WalkTick == now + 1000`（精确断言）；目标隐藏且 `CoolEye=false` ⇒ 不触发。
  4. R35：`StoneMode=true` + 目标 ≤2 ⇒ `MeltStoneAll`：自身 `StoneMode=false` + `RM_DIGUP`/`CharStatus` 重算；`GetMapBaseObjects(...,7)` 范围内每个 `StoneMode` 的 `ScultureMonster` 都被融化（7 格内/外各一只断言）；目标在 3 格 ⇒ 不触发、走索敌。
  5. R04：`VisibleActors>0` ⇒ helper 在 `NextHitTime` 前后各一次；断言 `Random(|Hi-Lo|+1)` 的参数与次数、`RM_DELAYMAGIC` 200ms 到点、`RM_10205` 收件人集合。
- **完成定义**：四类构造用例 + 既有 golden 不漂移（未开启新 race 场景时）+ trace 登记（R04 的 `BigHeartMonster.AttackTarget@BigHeartMonster.cs:22` 与 Rust `monai.rs` 站点）；R 级窗口：race 101/95 真机 ≥1 场景，R38 抽样。

#### MB-2 近身 RNG 回退 / 复生（可达）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R13 | `DualAxeMonster` 87（93/104 继承） | `DualAxeMonster.cs:84-137` | 5s 门扫 `VisibleActors` 最近 proper（`!HideMode\|\|CoolEye`）⇒ `SetTargetCreat`；`WalkTick >` 门且目标两轴 ≤4：≤2 时 `Random(5)==0` ⇒ `GetBackPosition`，否则（3-4 格）直接 `GetBackPosition`。**bug**：`:120/:122` 两处都判 `CurrX`，未判 `CurrY`。 | `GetBackPosition`（`BaseObject.cs:1878`）**未建模**；`monai::master_get_back_position` 是「主人身后格」另一语义，不能复用；**`AttackTarget` 覆写（`:33-72`，`:52` 的 `Random(5)` + `FlyAxeAttack`）是 base AI 的虚拟调用目标，必须同批接**；93/104 继承同一 `Run`/`AttackTarget` | R |
| R24 | `LightingZombi` 94 | `LightingZombi.cs:32-58` | 8s/1s 索敌；`WalkTick` 门；目标 ≤4：≤2 且 `Random(3)!=0` ⇒ `base.Run(); return`，否则 `GetBackPosition`；目标 <6 且 `NextHitTime` ⇒ `LightingAttack`：`Random(\|Hi(DC)-Lo(DC)\|+1)` + `MagPassThroughMagic(..., true)` + `BreakHolySeizeMode` + `RM_LIGHTING(1,...)`。 | `MagPassThroughMagic` 已有 `World::cast_pass_through_spell`；`RM_LIGHTING`/`SM_LIGHTING` 已有；`BreakHolySeizeMode` 已由 A01 落地；ctor `SearchTime=1500+` | R |
| R08 | `ChickenDeer` 52 | `ChickenDeer.cs:13-63` | `WalkTick >=` 门扫最近 proper（`!HideMode\|\|CoolEye`）⇒ `RunAwayMode=true/TargetCret`；否则清；逃跑且目标非空：**bug** `:55` 是 `|CurrX - baseObject.CurrX| <= 6` **两次**（`<=6`），`Δy` 从不参与；`baseObject` 是最近目标扫描循环的**残留变量**（循环结束时停在 `VisibleActors` 最后一项），**不一定等于 `TargetCret`**，因此判的是「自身与残留对象的 X 差」而非「与当前目标」；通过则 `GetNextDirection` + `GetNextPosition(TargetCret.CurrX, TargetCret.CurrY, dir, 5)`（起点是目标格）。 | 逃跑移动（`GetNextPosition` 已有）；ctor 由 `MonGen` 的 `Random(30)` 分支决定（已 fold）；无直接 RNG | R |
| R43 | `ZilKinZombi` 96 | `ZilKinZombi.cs:35-47` | 死亡且非 Ghost、非 Stone、`ZilKillCount>=0`、`VisibleActors.Count>0`、`now-ReAliveTick >= ZilkinDieTick` ⇒ `MaxHP >>= 1`、`FightExp /= 2`、`HP=MaxHP`、`ReAlive()`、`WalkTick=now+1000`。 | **`Die()` 覆写**（`:24-33`）在 `ZilKillCount>0` 时：`ReAliveTick = now; ZilkinDieTick = (Random(20)+4)*1000;`，随后**无条件** `ZilKillCount -= 1`；B08 的 MakeGhost/ReAlive 已落地；`ReAlive` 帧 `RM_ALIVE` | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/combat.rs`（如需 `LightingAttack` 的伤害入口）、`crates/world/src/world.rs`（`ReAlive`/死亡路径）、`bins/game-svr/src/smframe.rs`（`RM_ALIVE`）、`bins/game-svr/src/main.rs`。
- **验收判据（构造级）**：
  1. R13：`Run` 的 `:120/:122` 两处都只判 `|CurrX-TargetCret.CurrX|`，**`Δy` 完全不参与**。构造 `Δx=1, Δy=4`（≤2 支 + 抽 `Random(5)`）与 `Δx=3/4`（直接 `GetBackPosition`）两组；**`Δx=0, Δy=4` 通过 ≤2 门并抽 `Random(5)`**（不是「≤4 门」）；断言 `Random(5)` 只在 ≤2 支抽一次、参数 5；`GetBackPosition` 目标格正确。
  2. R24：`Random(3)!=0` 与 `==0` 两支；前者 `base.Run` 只跑一次并早退；后者 `GetBackPosition`；`LightingAttack` 的 `Random(DC)` 参数/次数、`MagPassThroughMagic` 的逐格伤害与 `RM_LIGHTING` 帧、`BreakHolySeizeMode` 清 `holy_seize`。
  3. R08：`:55` 两次都是 `<=6` ⇒ 用 `Δx=6` 证明**触发**、用 `Δx=7` 证明**不触发**（`Δy` 任意取值均不影响）；另构造「残留 `baseObject != TargetCret`」用例（可见表最后一项与 `TargetCret` 不同）证明取的是残留变量而不是 `TargetCret`；`GetNextPosition` 起点必须是 `TargetCret.CurrX/CurrY`。
  4. R43：`ZilKillCount=2`：第 1 次死亡 ⇒ `count=1`（仍 `>=0`，可复活）；第 2 次死亡 ⇒ `count=0`（**仍可再复活**）；第 3 次死亡 ⇒ `count=-1` 才停止。`Die` 在 `count>0` 时置 `ReAliveTick=now`、`ZilkinDieTick=(Random(20)+4)*1000`；到点前不复活，到点后 `MaxHP` 减半/`FightExp` 减半/`HP=MaxHP`/`RM_ALIVE`/`WalkTick=now+1000`；`STONE` 状态阻止复活。
- **完成定义**：构造用例 + `Die()` 抽数账本（oracle `ZilKinZombi.cs:30` 与 Rust 死亡路径站点）+ trace 登记；R 级窗口：race 87/93/104、94、96、52 各 ≥1 真机场景（优先 96、87/93/104）。

#### MB-3 延迟消息 / 毒 / 守卫（可达；本批引入 PR-1 的主要消费点）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R07 | `CentipedeKingMonster` 107 | `CentipedeKingMonster.cs:91-143` | `WalkTick >`；`FixedHideMode` ⇒ 10s `_attackTick` 门扫两轴 < `ComeOutValue`(4) 的 proper 目标 ⇒ `FindAttackTarget()`（覆写把 HP 回满）；非隐藏支：**3s `_attackTick` 门（`:126`）** ⇒ `AttackTarget()`（helper `:42-83`：`Random(DC)`、`Random(4)==0`、`Random(3)`、`MakePosion(DECHEALTH,60,3)` 或 `STONE,5,0`、`SendSelfDelayMsg(RM_DELAYMAGIC,…,600)`）；`AttackTarget()` 未命中且 **10s `_attackTick` 门** ⇒ `ComeDown(); _attackTick=now`。 | PR-1（600ms 自延时）；poison（`World::make_posion` 已有）；`ComeOutValue=4`/`AttackRange=6`；`FindAttackTarget`/`ComeDown` 未建模 | R |
| R21 | `GasMothMonster` 105 | `GasMothMonster.cs:24-35` | `CanMove && WalkTick >=` 门；8s/1s 索敌门 ⇒ `GasMothAttack()`（最近 proper 目标）；`Run` **不抽数**。 | 同 tick 的 `GasAttack` override（`:14-22`）在命中路径抽 `Random(3)`，目标 `HideMode` 时置透明 1；该 override 不在本批 `Run` 范围，但**必须同批接线**，否则 `Run` 只是索敌、攻击路径不完整 | R |
| R34 | `ScultureKingMonster` 102 | `ScultureKingMonster.cs:61-115` | `WalkTick >=`；`StoneMode` ⇒ 两轴 ≤2 ⇒ `MeltStone(); break`；否则 8s/1s 索敌 + `_mNDangerLevel > HP/MaxHP*5 && >0` 机制 `CallSlave()`（**C# `int/int` 整数除法**：`HP<MaxHP ⇒ 0`，仅满血得 5；召唤频率依赖此怪癖）（`Random(6)+6`、`Zuma[Random(4)]`）；清 `_mSlaveObjectList`。 | ctor `ViewRange=8/StoneMode=true/_mNDangerLevel=5`；`CallSlave` 生成链（`RegenMonsterByName` 已有宿主入口）；`MeltStone` 未建模，须完整复刻 `ScultureKingMonster.cs:25-33`：`CharStatusEx=0; CharStatus=GetCharStatus(); SendRefMsg(RM_DIGUP,Dir,CurrX,CurrY,0); StoneMode=false; new MapEvent(Envir,CurrX,CurrY,6,5*60*1000,true) + EventMgr.AddEvent` | R |
| R02 | `ArcherGuard` 112（20 继承） | `ArcherGuard.cs:39-91` | `CanMove` 且 `WalkSpeed` 门扫 `VisibleActors` 取曼哈顿最近 proper ⇒ `SetTargetCreat`/`DelTargetCreat`；有目标且 `NextHitTime` ⇒ `AttackTarger`（`:21-37`：`Random(\|Hi(DC)-Lo(DC)\|+1)`、`GetHitStruckDamage`、`StruckDamage`、`SendStruckDelayMsg(RM_REFMESSAGE,…,≥600)`、`RM_FLYAXE`）；无目标且 `GuardDirection>0` ⇒ `TurnTo`。 | 守卫链 `NormNpc→AnimalObject→Base`（**无** MonsterObject AI）；`RM_FLYAXE`/`RM_REFMESSAGE` 帧；guard 不入 `MonsterObject.Run` | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/slave.rs`（PR-1 队列扩展）、`crates/world/src/world.rs`（`make_posion`/攻击结算/5min `MapEvent`/`CharStatus` 重算）、`crates/world/src/mongen.rs` + `bins/game-svr/src/mongen.rs`（`CallSlave` 生成）、`bins/game-svr/src/smframe.rs`/`main.rs`（`RM_FLYAXE`/`RM_LIGHTING`/`RM_DIGUP`）。
- **验收判据（构造级）**：
  1. R07：`FixedHideMode` 且目标 4 格内 ⇒ `FindAttackTarget` 且 HP 回满；非隐藏支：`_attackTick` 距今 3000ms 内 ⇒ `AttackTarget` **一次都不调用**（3s 门），超过 3000ms 才调用；`AttackTarget` 返回 false 且超过 10000ms ⇒ `ComeDown` 且 `_attackTick=now`；`AttackTarget` 的 `Random(4)`/`Random(3)` 参数与次数；`Random(4)!=0` ⇒ 不抽 `Random(3)`（`if (Random(4)==0) { if (Random(3)!=0) … }`）；`RM_DELAYMAGIC` 600ms 前不投递、到点投递一次；毒型/时长/点数精确。
  2. R21：`GasMothAttack` 选最近 proper；`GasAttack` 的 `Random(3)` 在 `baseObject != null` 后**先抽**再判 `HideMode`（oracle `:17` 的短路顺序），用例必须钉住取数顺序。
  3. R34：`StoneMode` + 目标 ≤2 ⇒ `MeltStone`（断言 `CharStatusEx=0`/`CharStatus` 重算/`RM_DIGUP`/`StoneMode=false`/5min `MapEvent` 已注册）；`CallSlave` 的 `Random(6)+6` 与每次 `Random(4)` 的参数/次数；`_mNDangerLevel` 整数除法边界（`HP=MaxHP ⇒ 5` 不召唤；`HP=MaxHP-1 ⇒ 0`，条件退化为 `>0`）与满血复位 5。
  4. R02：最近目标选择（曼哈顿距离）、`Random(DC)` 参数、`RM_FLYAXE` 帧、`RM_REFMESSAGE` 的延迟（`max(|Δx|,|Δy|)*50+600`）、无目标时 `GuardDirection` 转向。
- **完成定义**：构造用例 + PR-1 队列到点用例 + trace 登记（`CentipedeKingMonster.AttackTarget@...:66/68`、`GasMothMonster.GasAttack@...:17`、`ScultureKingMonster.CallSlave@...:39/47`、`ArcherGuard.AttackTarger@...:24`）；R 级窗口：race 107/105/102/112 各 ≥1 场景。

#### MB-4 位移/狂暴（可达尾项）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R10 | `CowKingMonster` 92 | `CowKingMonster.cs:40-94` | `!Death && !Ghost && 30s JumpTime`：刷新 JumpTime；有目标且 `SiegeLockCount()>=5` ⇒ `GetBackPosition` + `CanWalk` ⇒ `SpaceMove(map,x,y,0)` 或 `MapRandomMove`，均 `return`；否则 `CrazyCount = 7 - HP/(MaxHP/7)`，`>=2` 且变化 ⇒ `CrazyReadyMode`；8s 内 `NextHitTime=10000`，之后 `CrazyKingMode`（8s：`NextHitTime=500/WalkSpeed=400`），到期恢复 `oldHitTime/oldWalkTime`。 | `SiegeLockCount` 未建模；`SpaceMove`/`MapRandomMove` 已有（`space_move`/`merchant::map_random_move`）；Crazy 字段（`M11` 只有 `crazy_mode`，本类另有 `CrazyCount/CrazyReady/CrazyKing/CrazyTime/oldHitTime/oldWalkTime`） | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/space_move.rs`、`crates/world/src/world.rs`。
- **验收判据（构造级）**：目标 + `SiegeLockCount>=5` ⇒ `SpaceMove` 分支把目标格作为落点、失败转 `MapRandomMove` 且 `return` 跳过 Crazy 段；`CrazyCount` 边界（HP=MaxHP ⇒ 0；HP=MaxHP/7 ⇒ 6）；`>=2` 且值变化才进 Ready；8s 边界严格 `<`；King 段 `NextHitTime/WalkSpeed` 精确恢复为进入前的值。
- **完成定义**：构造用例 + Crazy 字段全部可观测 + trace 登记（`MapRandomMove` 的 `:1235/:1236` 两抽落 **`merchant`** 组；`SpaceMove@:1103` 的移位环落 **`space_move`** 组）；R 级窗口：race 92 ≥1 场景。

#### MB-5 毒 / 隐身 / 瞬移（数据不可达；`Run` 内直接 RNG）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R22 | `GreenMonster` 208 | `GreenMonster.cs:13-36` | `CanMove`；有目标 ⇒ `TargetX/Y=目标格`；两轴各 1 ⇒ **先抽** `Random(TargetCret.AntiPoison+7) <= 6`，**再判** `TargetCret.StatusTimeArr[DECHEALTH]==0` ⇒ 两者都成立才 `MakePosion(DECHEALTH,30,1)`（状态已存在时**仍抽数**，只是不施毒）；8s/1s 索敌。 | `World::make_posion` 已有；`AntiPoison`/状态时长在 `Entity`（需确认玩家/怪都可用）；caster `TargetCret.AntiPoison`（注意是**目标**的 AntiPoison） | R |
| R29 | `RedMonster` 209 | `RedMonster.cs:13-36` | 与 R22 逐句相同（同样**先抽 Random、再判 DECHEALTH**），唯一差异：`MakePosion(DAMAGEARMOR,30,1)`。 | 同上；建议与 R22 同函数参数化，防止漂移 | R |
| R23 | `Khazard` 206 | `Khazard.cs:13-59` | 有目标 ⇒ 目标格写入 `TargetX/Y`；两轴 ==2 时：`Random(2)`；==0 支 ⇒ `GetFrontPosition` + `TargetCret.SendRefMsg(RM_SPACEMOVE_FIRE)` + `TargetCret.SpaceMove(map,nX,nY,0)`，再 `Random(1)` + `Random(TargetCret.AntiPoison+7)` ⇒ `DECHEALTH,35,2` 并 return；==1 支 ⇒ 目标 HP≤半才 `GetFrontPosition`（否则 `nX/nY` 保持 0），同样 `SpaceMove` + 同序两抽；8s/1s 索敌。 | `SpaceMove`（`space_move` 已有，落点重抽）；`RM_SPACEMOVE_FIRE` 帧；`Random(1)` 恒 0 但**仍抽** | R |
| R20 | `FrostTiger` 210 | `FrostTiger.cs:14-36` | `CanMove`；无目标且 `StatusTimeArr[TRANSPARENT]==0` ⇒ `MagMakePrivateTransparent(this,180)`；有目标 ⇒ `TRANSPARENT=0`；8s/1s 索敌。 | `World::mag_make_private_transparent` **已存在**（`crates/world/src/world.rs`，`slave.rs` 已消费）⇒ **复用**；本项只补 `Run` 段与透明状态/AOI 联动 | R |
| R40 | `TeleMonster` 203 | `TeleMonster.cs:12-31` | `CanMove`；有目标且 `\|CurrX-TargetX\|>5` 或 `\|CurrY-TargetY\|>5`（**比较 TargetX/Y，不是 TargetCret.CurrX/Y**）⇒ `RM_SPACEMOVE_FIRE` + `SpaceMove(TargetCret.MapName, TargetCret.CurrX, TargetCret.CurrY, 0)`；8s/1s 索敌。 | `SpaceMove` 跨图/落点；`RM_SPACEMOVE_FIRE`；注意 TargetX/Y 与 TargetCret 的错位是 oracle 原样 | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`（透明/毒状态）、`crates/world/src/world.rs`（`make_posion`）、`crates/world/src/space_move.rs`、`bins/game-svr/src/smframe.rs`/`main.rs`（`RM_SPACEMOVE_FIRE`）。
- **验收判据（构造级）**：R22/R29：目标两轴 1 格 + `AntiPoison=0/7`；`Random(AntiPoison+7)` 参数、`<=6` 门；**先抽 Random、再判 `DECHEALTH==0`**——`DECHEALTH` 已存在时**仍抽数**（只不施毒）；反向对照：把两次判定换序 ⇒ 断言红；R29 毒型为 `DAMAGEARMOR`。R23：两轴 2 格；`Random(2)` 两支；`Random(1)` 每次必抽（即使恒 0）；`Random(AntiPoison+7)` 的参数；目标半血分支的 `GetFrontPosition` 只在 HP≤半时调用。R20：无目标首次置透明 180、有目标清 0、仅在 `TRANSPARENT==0` 时调用。R40：`TargetX/Y` 与 `TargetCret` 错位用例（目标格 ≠ 当前格）必须按 oracle 走 `CurrX-TargetX`。
- **完成定义**：构造用例 + trace 登记（Green/Red `:23`、Khazard `:25/30/44`）；真机窗口无对象 ⇒ `window_required: true` + owner 裁决替代证据（不得以构造级绿代替）。

#### MB-6 魔法 / 群攻 / 自爆（数据不可达；helper 抽数）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R14 | `ElectronicScolpionMon` 200 | `ElectronicScolpionMon.cs:33-70` | `CanMove`；`UseMagic = HP<MaxHP/2`；`>1000 && TargetCret==null` ⇒ 索敌；**`TargetCret==null ⇒ return`（跳过 `base.Run`）**；目标两轴 ≤2 且 (`UseMagic` 或 恰 2 格) 且 `NextHitTime` ⇒ `LightingAttack`（`:15-31`：`GetAttackPower(DC)` 抽数 + `GetMagStruckDamage` 抽 MAC + `LoByte(MP)` 吸血 + `StruckDamage` + `RM_REFMESSAGE` 200 + `RM_LIGHTING`）。 | `GetAttackPower`/`GetMagStruckDamage`/`StruckDamage` 已有；`RM_LIGHTING` 已有；**早退跳过 base.Run 的时序差必须复刻** | R |
| R25 | `MagicMonObject` 133 | `MagicMonObject.cs:23-60` | `CanMove`；`UseMagic = HP<MaxHP/2`；`>1000 && 无目标` ⇒ 索敌；**`Master==null ⇒ return`（跳过 `base.Run`）**；`\|Δx\|<=5 && \|Δy\|<=5` 且 (`UseMagic` 或 恰 5 格) 且 `NextHitTime` ⇒ `LightingAttack`——**该类的 `LightingAttack` 是空函数**（`:18-21`），只有目标选择/字段副作用。 | `Master` 跟随已由 M06 落地；空 helper 也必须保留；早退时序 | R |
| R31 | `RonObject` 131 | `RonObject.cs:35-51` | `CanMove`；8s/1s 索敌；目标两轴 <6 且 `NextHitTime` ⇒ `AroundAttack`（`:13-33`：`GetAttackDir` 定 `Dir`；`GetMapBaseObjects(1)` 取邻接列表；`GetBaseAttackPoewr()` 抽 DC；对每个对象 `_Attack` + 移除；`RM_HIT`）。 | 多目标 `_Attack`（玩家耐久抽数）；`RM_HIT` 帧；邻接枚举顺序 | R |
| R17 | `ExplosionSpider` 117 | `ExplosionSpider.cs:79-90` | `!Death && !Ghost && 60s` ⇒ `DoSelfExplosion`（`:19-45`：`HP=0`；`Random(\|Hi(DC)-Lo(DC)\|+1)`；对每个邻接 proper：`GetHitStruckDamage(nPower/2)` + `GetMagStruckDamage(nPower/2)` + `StruckDamage` + `SendStruckDelayMsg(RM_REFMESSAGE,…,700)`）。 | `GetMagStruckDamage`/MAC 已有；PR-1（700ms）；死亡结算（自爆置 HP=0 后由下一 tick 结算） | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/combat.rs`（`_Attack`/magic damage 入口）、`crates/world/src/world.rs`（自爆/死亡）、`bins/game-svr/src/smframe.rs`/`main.rs`（`RM_LIGHTING`/`RM_HIT`/`RM_REFMESSAGE`）。
- **验收判据（构造级）**：R14：无目标 `return` 使 `base.Run` 不被调用（用计数器钉住「AI pass 只有 1 次」）；`UseMagic` 半血边界严格 `<`；`nX==2||nY==2` 与 `UseMagic` 的或门；`GetAttackPower`/`GetMagStruckDamage` 抽数顺序与参数。R25：`Master==null` 早退跳过 `base.Run`；空 `LightingAttack` 不产生帧/抽数；5 格或门。R31：邻接对象顺序（`GetMapBaseObjects` 的 x/y 扫描序）与 `_Attack` 次数；`Random(DC)` 参数。R17：60s 边界 `>`；`HP=0` 后同 tick 不结算、下一 tick 死亡；邻接每目标两次减伤（物理半 + 魔法半）与 `RM_REFMESSAGE` 700ms。
- **完成定义**：构造用例 + trace 登记（`ElectronicScolpionMon.LightingAttack@...:18`、`RonObject.AroundAttack@...:22`、`ExplosionSpider.DoSelfExplosion@...:22`）；真机窗口无对象 ⇒ `window_required: true` + owner 裁决。

#### MB-7 召唤 / 复制（数据不可达；生成链抽数）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R05 | `BoneKingMonster` 134 | `BoneKingMonster.cs:48-76` | `CanMove && WalkTick>=`；8s/1s 索敌；`DangerLevel > HP/MaxHP*5 && >0`（**C# `int/int` 整数除法**：`HP<MaxHP ⇒ HP/MaxHP*5 == 0`，条件退化为 `DangerLevel>0`；仅满血时为 5），满足 ⇒ `DangerLevel-=1; CallSlave()`；满血 ⇒ `DangerLevel=5`；清 `SlaveObjectList`。`CallSlave`（`:21-40`）：`Random(6)+6`，`GetFrontPosition`，最多 30 个；每个 `Random(3)` 选 `BoneCaptain/BoneArcher/BoneSpearman` 并 `RegenMonsterByName`。 | `RegenMonsterByName` 宿主入口已有；`GetFrontPosition`；生成构造链抽数已在 `bins/game-svr/src/mongen.rs::draw_monster_construct`（PR-2 只补非抽常量）；`DangerLevel` 字段 | R |
| R15 | `ElfMonster` 113 | `ElfMonster.cs:43-86` | `BoIsFirst` ⇒ 清 `FixedHideMode`、`RM_DIGUP`、`ResetElfMon()`；`Death && now-DeathTick>2s` ⇒ `MakeGhost()`；否则若有目标，或主人有目标/LastHiter ⇒ `MakeClone(Dragon1,this)`；clone 成功 ⇒ 继承 `AutoChangeColor`、若是 `ElfWarriorMonster` 则 `AppearNow()`、`Master=null`、`KickException()`。 | `MakeClone` 生成链抽数；`AppearNow`/`ResetElfMon`/`KickException` 未建模；B08 `MakeGhost` 已落地；主人联动（M06 已落地） | R |
| R16 | `ElfWarriorMonster` 114 | `ElfWarriorMonster.cs:112-166` | 同上首次出土/死亡 2s `MakeGhost`；`boChangeFace = TargetCret==null`，主人有目标/LastHiter ⇒ false；`boChangeFace` 且 `now-DigDownTick > 60s` ⇒ 名字末位为 `1` 时去尾并 `MakeClone`；成功 ⇒ `RM_DIGDOWN` + `RM_CHANGEFACE` + 继承颜色 + `AppearNow` + `Master=null` + `KickException`；**`else { DigDownTick=now; }` 属于 `if (boChangeFace)` 的 else**：`boChangeFace=true` 但未到 60s 或名字末位不是 `1` 时 **不刷新** `DigDownTick`，只有 `boChangeFace=false`（有目标 / 主人有目标或 LastHiter）才刷新。 | `MakeClone`/`AppearNow`；`RM_DIGDOWN`/`RM_CHANGEFACE` 帧；名字尾位 `1` 的字符串语义（UTF-8/GBK 字节层要按 oracle 的 C# `string` 末字符行为核对，**需 spike**） | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/mongen.rs` + `bins/game-svr/src/mongen.rs`（`RegenMonsterByName`/`MakeClone` 生成回调）、`crates/world/src/slave.rs`（主人/KickException）、`bins/game-svr/src/main.rs`（帧）。
- **验收判据（构造级）**：R05：`Random(6)+6` 的参数/次数、每个 minion 的 `Random(3)`、30 上限、`DangerLevel` 边界（**整数除法**：`HP=MaxHP-1 ⇒ HP/MaxHP*5=0 ⇒ DangerLevel>0 即触发；仅满血 ⇒ 5**）；R15/R16：`BoIsFirst` 只触发一次；死亡 2s 边界；`MakeClone` 的生成抽数；R16 的 60s 边界、名字尾位 `1` 与非 `1` 两支、`RM_DIGDOWN`/`RM_CHANGEFACE` 收件人；`boChangeFace=true` 且未到 60s（或名字尾位非 `1`）时 `DigDownTick` **不得刷新**（作用域回归）。
- **完成定义**：构造用例 + 生成链账本（`RegenMonsterByName`/`MakeClone` 的抽数必须与 oracle 同序）+ trace 登记；真机窗口无对象 ⇒ `window_required: true` + owner 裁决。

#### MB-8 事件 / 出土 / 运动（数据不可达；R42 为 L）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R18 | `FireMonster` 214 | `FireMonster.cs:23-82` | `CanMove` ⇒ 按固定顺序检查 9 格：`(nx,ny-1)`、`(nx,ny-2)`、`(nx-1,ny)`、`(nx-2,ny)`、`(nx,ny)`、`(nx+1,ny)`、`(nx+2,ny)`、`(nx,ny+1)`、`(nx,ny+2)`；每格 `GetEvent==null` 才 `new FireBurnEvent(this,x,y,ET_FIRE,FireTime,FireDamage)` + `AddEvent`；然后 8s/1s 索敌。 | `FireBurnEvent`/`fire_burn_events`/`event_at`/`add_fire_burn_event` **已存在**（`crates/world/src/world.rs`；`event_at` 已复刻 `MapEvent.Id=0` 第一条 quirk）⇒ **复用**，不新增 `map.rs` 事件存储；`ET_FIRE` 帧 | R |
| R33 | `SandMobObject` 132 | `SandMobObject.cs:14-57` | `!Death && !Ghost && WalkTick >`；刷新 WalkTick；`FixedHideMode`：HP>Max/20 且 `CheckComeOut()` ⇒ `_mDwAppearStart=now`；否则 HP∈(0,Max/20) 且 `now-_mDwAppearStart>3s` ⇒ `ComeDown()`；目标两轴均 >15 ⇒ `ComeDown(); return`；`NextHitTime` 门 ⇒ `SearchTarget`；非隐藏 ⇒ `AttackTarget()` 真 ⇒ **`base.Run()`（函数体）**；函数尾再 `base.Run()`（**两次 base pass**，oracle 原样）。 | `CheckComeOut`/`ComeDown`/`AttackTarget`；`ComeOutValue=8`；StickMonster 链（AnimalObject，**无** MonsterObject AI）；双 base pass 必须复刻 | R |
| R36 | `SoccerBall` 120 | `SoccerBall.cs:19-74` | `N550>0`：前格可走 ⇒ 按 `Dir` 反射表（0↔4、1↔7、2↔6、3↔5）改向，再 `GetNextPosition(...,N550)` 写 `TargetX/Y`；否则 `TargetX=-1`；`TargetX!=-1` ⇒ `GotoTargetXY()`，到达 ⇒ `N550=0`。 | 反射表、`GetNextPosition`、`GotoTargetXY`；**`Struck` 覆写（`:76-86`，`:83` 抽 `Random(4)`）写 `N550/Dir/TargetX/Y`，必须同批接并登记 trace**，否则 `N550` 恒 0；ctor 无额外抽（`AnimalObject` 直派生） | R |
| R32 | `RunAway`（死类） | `RunAway.cs:12-64` | 目标格写 `TargetX/Y`；HP≤Round(MaxHP/2) ⇒ `GetFrontPosition` + `RM_SPACEMOVE_FIRE` + `SpaceMove(map,nx-2,ny-2)`，`borunaway=true`；`borunaway` 且 `now-time1>5000`（**局部 `time1=0` ⇒ 恒真**）⇒ 目标两轴均 1 格时 `WalkTo(RandomByte(4))`，否则 `WalkTo(RandomByte(7))`；8s/1s 索敌。 | `SpaceMove`/`WalkTo` 已有；死类不实例化（`CreateMonster` 无 case），只能在构造级/注入测试 | R |
| R42 | `WhiteSkeleton` 100 | `WhiteSkeleton.cs:29-39` | `BoIsFirst` ⇒ `BoIsFirst=false; Dir=5; FixedHideMode=false; RM_DIGUP(Dir,CurrX,CurrY,0)`。无 RNG。 | `RM_DIGUP` 帧；race 100 在部署 `MonGen` 条目 0 | **L** |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/world.rs`（复用 `event_at`/`add_fire_burn_event` 的事件 tick/`GotoTargetXY`）、`bins/game-svr/src/smframe.rs`/`main.rs`（`RM_DIGUP` 等帧）。
- **验收判据（构造级）**：R18：空地图一次 Run 生成 9 个事件（坐标顺序精确）；已有事件格不重复生成；非空/越界格的行为照 `GetEvent`。R33：`FixedHideMode` + HP 阈值 + `CheckComeOut`；`AttackTarget` 命中时 `base.Run` 计数 = 2（双 pass），未命中时 = 1。R36：反射表全 8 向；`N550=0` 时 `TargetX=-1`；到达清 0。R32：`time1` 恒 0 的 bug ⇒ 每次逃跑都 `WalkTo`；两类 `RandomByte` 参数。R42：`BoIsFirst` 只发一次 `RM_DIGUP`，`Dir=5`，`FixedHideMode=false`。
- **完成定义**：构造用例 + R42 的 L 级抽样冒烟（1 个场景）+ trace 登记（R32 的 `RandomByte(4/7)`）；其余不可达项 `window_required: true` + owner 裁决。

#### MB-9 `FireballMonster` 全链（数据不可达；P1 设计已存在）

| ID | 类/race | 实测 | `Run` 体差异（逐句） | 依赖 | tier |
|---|---|---|---|---|---|
| R19 | `FireballMonster` 215 | `FireballMonster.cs:14-62` | `!Death && !Ghost`；有目标 ⇒ `MagCanHitTarget(target 自身坐标)` + `IsProperTarget` + 两轴 ≤8；否则清目标；索敌门；命中 ⇒ `Random(\|Hi(DC)-Lo(DC)\|+1)` 取 `nPower`，`GetMagStruckDamage`（MAC 抽）、`StruckDamage`、`RM_SPELL(48,x,y,48)`、`RM_MAGICFIRE(0,MakeWord(2,48),MakeLong(x,y),targetId)`、`RM_DELAYMAGIC`（对**目标**的 600ms 延时，`SendStruckDelayMsg`）、`BreakHolySeizeMode`（`RM_CHANGENAMECOLOR`）。 | 怪物设计件 §4.1 已有完整设计与 8 条机判（A1–A8）；`Entity.mac/anti_magic`、`get_mag_struck_damage`、`sm_spell`/`sm_magicfire`/`sm_changenamecolor` 已有；`magic::mag_can_hit_target` **已存在**（oracle 调用点传 `TargetCret.CurrX/CurrY` **目标自身坐标**作为起点的怪癖必须复刻）；缺自延时泵（PR-1）、`MagicMonster.Run` 两 pass（audit R26 部分） | R |

- **写集**：`crates/world/src/monai.rs`、`crates/world/src/entity.rs`、`crates/world/src/combat.rs`、`crates/world/src/magic_effect.rs`/`magic.rs`、`crates/world/src/slave.rs`（PR-1）、`crates/world/src/space_move.rs`（`SpaceMoveGetRandXY`）、`bins/game-svr/src/smframe.rs`/`main.rs`。
- **验收判据（构造级）**：直接采用怪物设计件 §5 的 A1–A8（`fireball_power_draw_arg_and_value`、`get_mag_struck_damage_draws_mac_once_and_adds_defender_undead_power`、`fireball_cast_draw_order_dc_mac_struck_durability`、`delayed_magic_fires_only_at_600ms_and_draws_mac_twice`、`anti_magic_zero_skips_damage_but_still_draws_dc_and_breaks_holy_seize`、`mag_can_hit_target_reproduces_target_coords_and_is_valid_cell_quirks`、`magic_monster_runs_second_pass_after_think_true`、`fireball_casts_every_tick_because_spell_tick_is_reset`），另加「无目标时 `base.Run` 只跑一次 / `Think` 真时两 pass」的计数断言。
- **完成定义**：P1 设计的 8 条机判全绿 + PR-1 队列语义 + trace 登记（`FireballMonster.Run@...:26/32/35/39/40/41`）；真机窗口无对象（race 215 在部署 `MonGen`/库均 0 行）⇒ `window_required: true` + owner 裁决；在报告中明确「生产行为改变 = 0」。

### 2.4 可选前置 MB-0（Run 分发骨架，L）

- **目的**：把 `monai.rs` 的「一坨 per-race if」拆成「dispatch + per-class 入口」；不改变任何现有 race 的行为（ATMonster 索敌门、守卫只跑 base、普通怪 `monster_run` 全部保持）。
- **写集**：`crates/world/src/monai.rs`（或新增 `crates/world/src/monster_run/` 模块）+ `crates/world/src/entity.rs`（缺失字段）。
- **判据**：43 个 race 的 dispatch 表枚举测试；既有 golden 逐字节不变；RNG trace 站点集合不变。
- **收益**：后续 MB-1..MB-9 若拆成 per-class 文件，**编辑**可并行；但**合并与 RNG 账本**仍需串行。不做 MB-0 也能按单线执行，不能并行。
- **风险**：dispatch 本身是行为面（每个 race 走哪条链），必须用「43 race → 期望基类链」表钉住，否则会把「共用门」误当「所有 race 都有」（怪物设计件 §3.1 的 `is_active_seek_race` 缺口就是前车之鉴）。

### 2.5 审计件其余「缺失/部分」行的归属/排除对账

> 本件交付范围 = 审计 §2.6 的 8 个玩家 `Run` 散段（P07/P21/P22/P25/P26/P27/P31/P32）+ 34 个「缺失」专用怪 `Run` 覆写（R02–R43 去掉在飞 R03/R09/R37）。审计件其余 `缺失/部分` 行**既不是本件交付，也不允许静默消失**：下表逐条给出「归属批次 / 已落地 / 显式排除 / 另立卡」四类归宿。`另立卡` 只是建议线程名，不在本件创建、不抢本件写集；`已落地` 行同时说明审计件正文是时点文档、其状态列已 stale。

| 审计行 | 审计状态 | 本件处置 | 归宿或理由 |
|---|---|---|---|
| P05 | 部分 | 另立卡 | `IsTimeRecall` 延时回城属魔法/会话面，本件 8 段不含；建议 `openmir2-player-time-recall-run` |
| P07 | 缺失 | **PL-2** | 本件玩家段（含 spike） |
| P08 | 缺失 | 另立卡 | 透明结束清 `AbilMagBubbleDefence`，与 R20 透明状态联动的独立小卡 |
| P10 | 缺失（L） | 另立卡 | `PlaySuperRock` 挂机状态机 |
| P11 | 部分（L） | 另立卡 | `IsTimeGoto` 到点触发属脚本适配器线，PL-2 之外 |
| P12 | 部分（L） | 另立卡 | `OffLineFlag` 挂机踢线属连接生命周期线 |
| P13 | 部分（L） | 另立卡 | `IsDelayCall` 到点回调属脚本线 |
| P14 | 缺失（L） | 另立卡 | 2min PK 值衰减（§4 特殊行同项） |
| P15 | 部分（L） | 另立卡 | `UseLamp` 灯耐久（§4 特殊行同项） |
| P16 | 缺失 | **已落地 `706c95cae`** | 审计件正文 stale；本件不重排 |
| P17 | 部分（L） | 另立卡 | 攻城区 `ChangePkStatus(true)` 属 castle 线 |
| P18 | 部分（L） | 另立卡 | 1s 安全区/攻城/半价聚合门属 castle + 玩家节拍线 |
| P19 | 部分（R） | 另立卡 | 玩家 200ms 消息泵预算/优先级，随 D02/PR-1 的玩家侧卡 |
| P20 | 部分（L） | 另立卡 | 关/踢时序属连接生命周期线 |
| P21 | 缺失 | **PL-1** | 本件玩家段 |
| P22 | 缺失 | **PL-4** | 本件玩家段 |
| P23 | 缺失（L） | 另立卡 | 3v3 战场计分 |
| P24 | 缺失（L） | 另立卡 | 地图游戏点 `boINCGAMEPOINT` |
| P25 | 缺失 | **PL-3** | 本件玩家段 |
| P26 | 缺失 | **PL-3** | 本件玩家段 |
| P27 | 缺失 | **PL-3** | 本件玩家段 |
| P28 | 缺失（L） | 另立卡 | 转生变色 |
| P29 | 部分（L） | 另立卡 | `WhisperHuman` 清理 |
| P30 | 部分（L） | 另立卡 | 30s 失效清理聚合门 |
| P31 | 部分 | **PL-2** | 本件玩家段 |
| P32 | 缺失 | **PL-1** | 本件玩家段 |
| P33 | 部分 | 显式排除 | `base.Run()` 公共段已由审计 B01–B17 对账，本件不重复排 |
| P34 | 缺失 | 显式排除 | 玩家 `Run` 节拍 = D02，独立卡 `openmir2-run-gap-player-run-quantification` |
| P35 | 部分（L） | 另立卡 | 登录加载/周期存档节拍 |
| P37 | 缺失（L） | 另立卡 | 地面物/事件可见表与 `RM_ITEMHIDE` |
| P38 | 部分（R） | 显式排除 | `RM_UPDATEVIEWRANGE`/降频 = D05，独立卡 `openmir2-player-searchviewrange-cadence` |
| D01 | 部分 | 显式排除 | 玩家 50ms/恢复重放量化 = 玩家节拍卡（`openmir2-run-gap-player-run-quantification`） |
| D02 | 缺失 | 显式排除 | 同上（玩家逐人 `RunTime` 门） |
| D03 | 缺失 | 显式排除 | `openmir2-player-searchviewrange-cadence` |
| D04 | 缺失 | 显式排除 | 怪物每 actor `RunTime` 门，建议 `openmir2-monster-run-cadence` |
| D05 | 缺失 | 显式排除 | 可见性降频，随 D04 卡 |
| D08 | 部分 | **PR-1** | 仅「延时头阻塞整泵」部分纳入 PR-1；其余消息泵面见 P19 |
| D09 | 部分 | 显式排除 | 单线程设计硬约束，跨线程交错不可复现（已登记） |
| B01 | 部分 | **PR-1** | 仅延时头部分；玩家消息优先级不在本件 |
| B06 | 部分 | 另立卡 | 死亡结算时点；MB-2 只消费 `ZilKinZombi.Die()` 的抽数 |
| B08 | 缺失 | **已落地 `b12b59ca8`** | MakeGhost 双门 + 复活链 + Ghost 5min 清理 |
| B10 | 部分（L） | 另立卡 | 玩家 `TargetCret` 清理，随 D02 玩家节拍卡 |
| B13 | 缺失（L） | 另立卡 | `Master` 支变色轮换，L |
| B15 | 部分（L） | 另立卡 | `ShowHp` 已接线、只缺 Run 周期段（§4 特殊行同项） |
| A02 | 部分 | 显式排除 | `Operate` 覆写；本件 §2 已声明不排 `Operate`/`AttackTarget` |
| M05 | 部分（L） | 另立卡 | `Mission`/任务点，L |
| M06 | 缺失 | **已落地 `209023120`** | Master 跟随 / 20 格 SpaceMove / SlaveRelax 早退 |
| M08 | 缺失 | **已落地 `209023120`** | 同上 |
| M13 | 缺失 | 显式排除 | = D04/D05 节拍卡 |
| R01 | 部分 | 显式排除 | `ATMonster` 索敌门；§6.9 的 `is_active_seek_race` 家族，本件不扩大集合（会改所有 race 频率），另立卡 |
| R02 | 缺失 | **MB-3** | 本件专用怪 |
| R03 | 缺失 | 在飞 O03 | 不重复排 |
| R04 | 缺失 | **MB-1** | 本件专用怪 |
| R05 | 缺失 | **MB-7** | 本件专用怪 |
| R06 | 部分 | 显式排除 | 110/111 城堡放置，不属本件 34 项 |
| R07 | 缺失 | **MB-3** | 本件专用怪 |
| R08 | 缺失 | **MB-2** | 本件专用怪 |
| R09 | 缺失 | 在飞 O04 | 不重复排 |
| R10 | 缺失 | **MB-4** | 本件专用怪 |
| R12 | 缺失 | **MB-1** | 本件专用怪 |
| R13 | 缺失 | **MB-2** | 本件专用怪 |
| R14 | 缺失 | **MB-6** | 本件专用怪 |
| R15 | 缺失 | **MB-7** | 本件专用怪 |
| R16 | 缺失 | **MB-7** | 本件专用怪 |
| R17 | 缺失 | **MB-6** | 本件专用怪 |
| R18 | 缺失 | **MB-8** | 本件专用怪 |
| R19 | 缺失 | **MB-9** | 本件专用怪 |
| R20 | 缺失 | **MB-5** | 本件专用怪 |
| R21 | 缺失 | **MB-3** | 本件专用怪 |
| R22 | 缺失 | **MB-5** | 本件专用怪 |
| R23 | 缺失 | **MB-5** | 本件专用怪 |
| R24 | 缺失 | **MB-2** | 本件专用怪 |
| R25 | 缺失 | **MB-6** | 本件专用怪 |
| R26 | 部分 | **MB-9 依赖** | `MagicMonster.Run` 两 pass / 无 HolySeize 门的 `Think` / 不伤害的 `AttackTarget` |
| R27 | 部分 | 显式排除 | `Run` 体=父类骨架；§6.9 seek-gate 家族，另立卡 |
| R28 | 部分 | 显式排除 | 同 R27（父类骨架 + 百分比语义在 `AttackTarget`，不在 `Run`） |
| R29 | 缺失 | **MB-5** | 本件专用怪 |
| R31 | 缺失 | **MB-6** | 本件专用怪 |
| R32 | 缺失 | **MB-8** | 本件专用怪 |
| R33 | 缺失 | **MB-8** | 本件专用怪 |
| R34 | 缺失 | **MB-3** | 本件专用怪 |
| R35 | 缺失 | **MB-1** | 本件专用怪 |
| R36 | 缺失 | **MB-8** | 本件专用怪 |
| R37 | 缺失 | 在飞 O05 | 不重复排 |
| R38 | 缺失 | **MB-1** | 本件专用怪 |
| R39 | 部分 | 另立卡 | `SuperGuard` 主动攻击 PvP；守卫链，建议 `openmir2-superguard-run` |
| R40 | 缺失 | **MB-5** | 本件专用怪 |
| R41 | 部分 | 显式排除 | 110/111 城堡放置，不属本件 34 项 |
| R42 | 缺失（L） | **MB-8** | 本件专用怪（L） |
| R43 | 缺失 | **MB-2** | 本件专用怪 |
| O02 | 部分 | 显式排除 | `RM_UPDATEVIEWRANGE` 降频 = D05，随 D04/D05 卡 |
| O03 | 缺失 | 在飞 O03 | BeeQueen `RM_ZEN_BEE` |
| O04 | 缺失 | 在飞 O04 | Clone 受击扣主人 MP / 恒 `return false` |
| O05 | 缺失 | 在飞 O05 | SpiderHouse `RM_ZEN_BEE` |
| O08 | 部分（L） | 另立卡 | `PlayCloneObject.Operate` 受击转发，L |
| O09 | 部分（R） | 另立卡 | 玩家巨型消息分派；随 B01/D08 消息泵卡 |
| O10 | 缺失（L） | 另立卡 | 练功师受击统计，L |
| O11 | 缺失（L） | 另立卡 | 假人受击 PK 标记，L |
| N01 | 部分 | 另立卡 | NPC `base.Run` 恢复段（与 B03–B05 同族），建议 `openmir2-npc-base-run-recovery` |
| N02 | 部分（L） | 另立卡 | 商人攻城 `FixedHideMode` 切换与帧面，L |
| N04 | 部分（L） | 显式排除 | `NpcLimit` 预算断点续跑已登记 T-42 |
| N05 | 部分（L） | 显式排除 | 同 N04（已登记 T-42） |
| N06 | 部分 | 另立卡 | 同 N01（NPC/商人恢复段） |
| §4 `DecLightItemDrugTick` | = P15 | 见 P15 | 重复行 |
| §4 `DecPkPointTick` | = P14 | 见 P14 | 重复行 |
| §4 `CheckDupObjTick` | = P16 | **已落地 `706c95cae`** | 重复行 |
| §4 `ShowHpTick` | = B15 | 见 B15 | 重复行 |

> **对账结论**：本表共 107 行（含 §4 重复项）——**39 行归属本件批次**（8 个玩家段 + 31 个新怪批次项）、**1 行**（R26）归 MB-9 依赖、**2 行**（D08/B01）归 PR-1、**5 行已落地**（P16=`706c95cae`、M06/M08=`209023120`、B08=`b12b59ca8`，以及 §4 的 `CheckDupObjTick` 重复行）、**6 行在飞**（R03/R09/R37=O03/O04/O05，成对计）、**19 行显式排除**、**32 行另立卡**、**3 行**指向 P14/P15/B15 重复项；**0 条无归宿**。任何「另立卡」都不得与本件的 `monai.rs`/`world.rs`/`session` 写集并行；owner 若要扩本件范围，应新开线程而不是把其写集塞进本件。

---

## 3. 跨批前置（PR-1..PR-5）

### PR-1 怪物自延时消息队列与 `GetMessage` 头阻塞（R）
- **现状**：`crates/world/src/slave.rs::send_self_delay_msg` / `send_self_delay_msg_with` / `deliver_self_delay_msgs` 已存在，服务玩家/宝宝自投递；`crates/world/src/world.rs` 在 tick 中调用 `deliver_self_delay_msgs`。审计 D08 指出：oracle `GetMessage` 只检查**队首**，队首未到点 ⇒ 重新入队并 `return false`，**阻塞整泵**；Rust 当前扫描全部到点项，语义不同。
- **本规划需要**：怪物 `Run` 内的 `SendSelfDelayMsg`（R04 200ms、R07 600ms、R17 700ms；R03/R37 的 500ms 在飞）与 R19 的**目标延时** `SendStruckDelayMsg(...,600)`；以及「头项未到点阻塞其后的同优先级延时消息」。R04/R07 的 200/600ms 消息若被提前投递，会改变战斗时序和后续 RNG。
- **写集**：`crates/world/src/slave.rs`、`crates/world/src/world.rs`、`crates/world/src/entity.rs`、`crates/world/src/monai.rs`。
- **判据**：MCS-12（审计件 §5）——头项 `deliver_at=now+1000`、次项 `deliver_at=now`，调一次泵断言次项未处理；再断言到点后处理。怪物延迟消息的收件人/帧面按各自批次用例。
- **串行**：PR-1 必须先于 **MB-1/MB-3/MB-6/MB-9**；与任何改 `slave.rs` 的线串行。

### PR-2 类常量落到实体（L，行为由各 MB 承担）
- **现状**：`bins/game-svr/src/mongen.rs::draw_monster_construct` 已把**抽数**按 race 落进 `MonsterCtorDraws`（`meat_quality`/`bo_fear_fire`/`zil_kill_count`/`search_time` 等）；但 **非抽数常量**中，`Entity` 已有 `stone_mode`/`fixed_hide_mode`/`view_range`/`animal`（`crates/world/src/entity.rs`）**不得重复落地**；真正缺的是 `ComeOutValue`、`AttackRange`、`UseMagic`、`DangerLevel`、`SiegeLockCount`、`N550`、`GuardDirection` 与 CowKing 的 Crazy 六字段（`CrazyCount/CrazyReady/CrazyKing/CrazyTime/oldHitTime/oldWalkTime`），它们没有在 `MonsterSpawnRequest`/`Entity` 里表达。
- **本规划需要**：把上述常量按 race 落实体；不得把它们做成「全局默认」而让所有怪共享。
- **写集**：`crates/world/src/entity.rs`、`crates/world/src/mongen.rs`、`bins/game-svr/src/mongen.rs`。
- **判据**：spawn 每个 race 后断言常量表；RNG 账本不变（不新增抽数）。
- **串行**：先于所有 MB；与 `mongen` 车道的写集冲突 ⇒ 串行。

### PR-3 玩家状态归属 spike（L）
- **问题**：P22/P26/P31 的状态在宿主 `SessionGround`（`game_gold`/`hunger_status`/`use_items`），P25/P27/P32 需要 world 侧 `Entity`（HP/`power_rate`/死亡/`LastHiter`）。若把全部字段搬进 `Entity`，会与 DB 存档/会话生命周期重复；若全部留宿主，world 的死亡结算/RNG 无法直接读写。
- **spike 输出**：一张字段 → 归属表（`SessionGround` / `Entity` / 双写点）与「谁在 tick 中驱动」；建议：`game_gold`/`game_point`/`hunger_status` 留宿主，由宿主在 world.tick 前后跑对应段；`hp/power_rate/kill_mon_exp_rate/revival/revival_tick` 进 `Entity`（因为死亡/战斗/经验都在 world 侧）。**本件不替 owner 做决定**，spike 结论写入实现报告。
- **判据**：无代码判据；spike 交付 = 字段表 + 调用点图。

### PR-4 RNG trace 站点登记（L，但窗口前必须）
- **现状**：`mir2-rs/tests/parity/m3/rng-trace-probe/consumer_groups.tsv` 的 `monster_ai` 只按 `crates/world/src/monai.rs` 文件级归组；oracle 侧只有 `MonsterObject.Think`/`Wondering`/`GotoTargetXY` 等通用模式。新实现的每个类 `Run` 的抽数在 oracle trace 里会显示为 `类.Run@类.cs:行`，若不登记就是 `UNCLASSIFIED`。
- **本规划需要**：每批实现时追加「oracle 类名/行」行与 Rust 站点行；`player_run` 只覆盖 P16，其它玩家段如新增抽数（P21 的 looks）要单列。
- **判据**：`rng_consumer_groups.py` 的 `--require-closed` 无 `UNCLASSIFIED`；`groupdiff --fold` 在折叠后组集合/计数/次序一致。
- **串行**：与实现同批提交；不改 `crates/protocol`/`storage`。

### PR-5 已存在原语核对（不改代码，只作为依赖事实）
- **已存在**：`Entity.mac`/`anti_magic`、`combat::get_mag_struck_damage`、`World::make_posion`、`World::mag_make_private_transparent`、`world::FireBurnEvent`/`fire_burn_events`/`event_at`/`add_fire_burn_event`、`magic::mag_can_hit_target`、`space_move::space_move_get_rand_xy`、`merchant::map_random_move`、`bins/game-svr/src/guardlist.rs::regen_monster_by_name`、`map::get_next_position`、`monai::make_ghost`、`World::cast_pass_through_spell`、`smframe::sm_spell`/`sm_magicfire`/`sm_changenamecolor`/`sm_lighting`。
- **明确缺**（已剔除 3 项误报：`MagCanHitTarget`、`FireBurnEvent`、`EventMgr/GetEvent` 均已存在）：`MakeClone`/`AppearNow`/`ResetElfMon`/`KickException`、`CheckComeOut`/`FindAttackTarget`/`ComeDown`/`Turned`/`MeltStone`/`MeltStoneAll`、`GetBackPosition`（`BaseObject.cs:1878`）未建模（`monai::master_get_back_position` 不是同一语义）、`ItemDamageRevivalRing`、戒指 shape 标志、`SiegeLockCount`。
- **结论**：PR-5 不新增批次；但 review 时不得把「已存在原语」当作「Run 体已实现」。

---

## 4. trace 影响与验收口径

### 4.1 每批新增取数点（oracle 实测行）

| 批次 | 新增 oracle 取数点（实测行） | Rust 落点（建议） | 组 |
|---|---|---|---|
| MB-1 | `BigHeartMonster.AttackTarget@BigHeartMonster.cs:22`（`Random(DC)`） | `crates/world/src/monai.rs`（`big_heart_attack_target`） | `monster_ai` |
| MB-2 | `DualAxeMonster.Run@DualAxeMonster.cs:124`（`Random(5)`）；`LightingZombi.Run@LightingZombi.cs:43`（`Random(3)`）、`LightingZombi.LightingAttack@...:26`（`Random(DC)`）；`ZilKinZombi.Die@...:30`（`Random(20)+4`） | `crates/world/src/monai.rs`；死亡抽数落 `crates/world/src/world.rs`（`settle_deaths`） | `monster_ai` / `revive` |
| MB-3 | `CentipedeKingMonster.AttackTarget@...:52/66/68`；`GasMothMonster.GasAttack@...:17`（`Random(3)`）；`ScultureKingMonster.CallSlave@...:39/47`；`ArcherGuard.AttackTarger@...:24` | `crates/world/src/monai.rs`；生成抽数落 `bins/game-svr/src/mongen.rs`/`crates/world/src/mongen.rs` | `monster_ai` / `spawn` |
| MB-4 | `MapRandomMove` 的**两次初始抽**在 `BaseObject.cs:1214`（抽点 `:1235`/`:1236`）⇒ **`merchant`** 组（Rust `merchant.rs::map_random_move_xy`）；`SpaceMove@BaseObject.cs:1103` 的**移位环**（`SpaceMoveGetRandXY@:1040`）⇒ **`space_move`** 组（Rust `space_move.rs`） | `crates/world/src/merchant.rs`（初始两抽）/`crates/world/src/space_move.rs`（移位环） | `merchant` + `space_move` |
| MB-5 | `GreenMonster.Run@...:23`、`RedMonster.Run@...:23`（`Random(AntiPoison+7)`）；`Khazard.Run@...:25/30/44`（`Random(2)/Random(1)/Random(AntiPoison+7)`）；`SpaceMove` | `crates/world/src/monai.rs`；`space_move.rs` | `monster_ai` / `space_move` |
| MB-6 | `ElectronicScolpionMon.LightingAttack@...:18`（`GetAttackPower`）；`RonObject.AroundAttack@...:22`（`GetBaseAttackPoewr`）；`ExplosionSpider.DoSelfExplosion@...:22`（`Random(DC)`）；`GetMagStruckDamage` | `crates/world/src/monai.rs`/`combat.rs` | `monster_ai` / `magic_effect` |
| MB-7 | `BoneKingMonster.CallSlave@...:26/34`；`ElfMonster/ElfWarriorMonster` 的 `MakeClone` 生成链 | `crates/world/src/monai.rs`；`mongen` | `spawn` |
| MB-8 | `RunAway.Run@...:44/48`（`RandomByte(4/7)`）；**`SoccerBall.Struck@SoccerBall.cs:83`（`Random(4)`）**；`SoccerBall.Run`/`FireMonster`/`SandMob` 无直接抽 | `crates/world/src/monai.rs` | `monster_ai` |
| MB-9 | `FireballMonster.Run@...:26`（`Random(DC)`）、`:32`（`GetMagStruckDamage` 的 MAC）、`:35`（`StruckDamage`）、`StruckDamage` 的玩家耐久抽、延迟链（怪物设计件 §4.1） | `monai.rs`/`combat.rs`/`slave.rs` | `monster_ai`/`magic_effect`/`player_run` |

### 4.2 验收口径（每批通用）
- **构造级**：固定 `world.clock_ms=100_000`（避开 `now <= NextHitTime` 启动窗）；用固定种子 `RandomNumber` + `CountingRng` 记录每次 `random_below` 的入参；断言抽数**次数、参数、相对顺序**，不是只看结果值。
- **帧面**：以 `TickRecord` 的新字段/宿主下行断言帧的 ident、收件人集合、顺序；`SendRefMsg` 的收件人语义沿用 `bins/game-svr/src/main.rs::broadcast_ref_msg` 的 ±12/`Race==Play`/非 Ghost 口径（注意 oracle 500ms 缓存未建模的已知差异）。
- **R 级窗口**：可达 race 必须真机全量对拍（`window_required: true`）；不可达 race **不能**用构造级替代，review 写 `window_required: true` + 缺证据 + owner 裁决。L 级（R42）按「等价 + 1 条抽样冒烟」。
- **改坏必红**：每个门/抽数顺序都要有「反向对照」用例（把门去掉/改序 ⇒ 断言红），至少覆盖 PR-1 头阻塞、`CheckItemsNeed` 子集、`INCHP` bug、ChickenDeer/DualAxe 的 `CurrX` 双判、Khazard 的 `Random(1)` 必抽、ZilKin `Die` 抽数。

---

## 5. 批次总表（写集 / 串行 / 判级）

| 批次 | 项 | 可达性 | 判级 | 主要写集（文件级） | 串行约束 |
|---|---|---|---|---|---|
| PR-1 | 自延时队列 + 头阻塞 | — | R | `crates/world/src/slave.rs`、`world.rs`、`entity.rs`、`monai.rs` | 先于 MB-1/3/6/9；与 slave 车道串行 |
| PR-2 | 类常量落实体 | — | L | `crates/world/src/entity.rs`、`crates/world/src/mongen.rs`、`bins/game-svr/src/mongen.rs` | 先于所有 MB；与 mongen 车道串行 |
| PR-3 | 玩家状态归属 spike | — | L | 无代码写集（spike：字段 → `SessionGround`/`Entity` 归属表 + tick 驱动点） | **先于 PL-1..PL-4**；spike 未出结论 ⇒ PL 不得开工 |
| MB-0（可选） | Run 分发骨架 | — | L | `crates/world/src/monai.rs`（或 `monster_run/`）、`entity.rs` | 先于 MB-1..9；不改 RNG |
| PL-1 | P21 + P32 | 可达 | R | `entity.rs`、`world.rs`、`ability.rs`、`bag.rs`、`ground.rs`、`main.rs`、`config.rs`/`envir/conf.rs` | **先完成 PR-3**；与 PL-2/3/4 串行；与 recalc 线串行 |
| PL-2 | P31 + P07 | P31 机制可达/内容不触发 / P07 待 spike | R / L | `script/engine/mod.rs`、`script/engine/actions.rs`、`npc.rs`、`main.rs`、`entity.rs`/`world.rs` | **先完成 PR-3**；与 PL-1/3/4 串行；与 script 车道串行 |
| PL-3 | P27 + P25 + P26 | 生产不可达 | R | `entity.rs`、`world.rs`、`combat.rs`、`main.rs`、`config.rs`/`envir/conf.rs`、`script/actions.rs` | **先完成 PR-3**；与 PL-1/2/4 串行 |
| PL-4 | P22 | 生产不可达 | R | `main.rs`、`downlink.rs`、`homemap.rs`、`script/actions.rs` | **先完成 PR-3**；与 PL-1/2/3 串行 |
| MB-1 | R38/R12/R35/R04 | 可达 | R | `monai.rs`、`entity.rs`、`bins/game-svr/src/mongen.rs`、`smframe.rs`、`main.rs` | 全部 MB 串行（`monai.rs` + 共享 RNG） |
| MB-2 | R13/R24/R08/R43 | 可达 | R | `monai.rs`、`entity.rs`、`combat.rs`、`world.rs`、`smframe.rs`、`main.rs` | 同上 |
| MB-3 | R07/R21/R34/R02 | 可达 | R | `monai.rs`、`entity.rs`、`slave.rs`、`world.rs`、`mongen.rs`、`smframe.rs`、`main.rs` | 同上；PR-1 先做 |
| MB-4 | R10 | 可达 | R | `monai.rs`、`entity.rs`、`space_move.rs`、`world.rs` | 同上 |
| MB-5 | R22/R29/R23/R20/R40 | 不可达 | R | `monai.rs`、`entity.rs`、`world.rs`、`space_move.rs`、`smframe.rs`、`main.rs` | 同上 |
| MB-6 | R14/R25/R31/R17 | 不可达 | R | `monai.rs`、`entity.rs`、`combat.rs`、`world.rs`、`smframe.rs`、`main.rs` | 同上；PR-1（R17 700ms） |
| MB-7 | R05/R15/R16 | 不可达 | R | `monai.rs`、`entity.rs`、`mongen.rs`、`slave.rs`、`main.rs` | 同上；与 MB-3 的 `CallSlave` 写集冲突，串行 |
| MB-8 | R18/R33/R36/R32/R42 | 不可达（R42 L） | R（R42 L） | `monai.rs`、`entity.rs`、`world.rs`、`smframe.rs`、`main.rs` | 同上 |
| MB-9 | R19 | 不可达 | R | `monai.rs`、`entity.rs`、`combat.rs`、`magic_effect.rs`/`magic.rs`、`slave.rs`、`space_move.rs`、`smframe.rs`、`main.rs` | 同上；PR-1 先做；依赖 R26 两 pass |

> **总结**：玩家 4 批 + 怪物 9 批 = 13 个新批次（另有 PR-1/PR-2/PR-3 三个跨批前置与可选 MB-0）；31 个怪物项 + 8 个玩家项。所有批次**单线串行**；任何并行都必须先做 MB-0 的模块化且仍按 RNG 账本合并顺序收口。

---

## 6. 未决项 / 需 spike（不得臆造）

1. **P07 死代码裁决**：`AutoTimerStatus` 无写入者、`SETONTIMER` 无 handler、部署脚本 0 使用。spike 命令与输出必须入报告；确认后把审计件 P07 改为 `N/A`，不要实现。若未来内容/源码出现写入者，按 R 重开。
2. **P31 的 `set_auto_get_exp` 现状**：`crates/script/src/engine/actions.rs` 的 `let envir = s_map.is_empty() && self.player.map_exists(&s_map);` 在 `s_map` 为空时查的是空串，与 oracle「仅当 sMap 为空才 FindMap」的注释口径需要 spike 对齐（现有测试注释称「AutoGetExpEnvir 恒 null」）。本件不修，只要求在 PL-2 前确认。
3. **P32 戒指数据**：部署物品库是否有 `Shape ∈ {114,160,161,162}` 的戒指（以及 `AniCount` 命中武器/右手位的情况）需只读抽样；`RevivalTick` 在 oracle 无 ctor 初始化（默认 0）⇒ 首次死亡可立即复活，这个怪癖必须保留。
4. **P21 与 `check_take_on_items` 的边界**：本段只认 7 个 Need；任何「复用全量穿戴校验」的实现都会把 `Need=0` 的等级不足物品错误弹出。用例必须覆盖。
5. **R19 的 `MagicMonster` 两 pass**：`FireballMonster.Run` 的 `base.Run()` 会再跑一遍 `MagicMonster.Run`（audit R26 判「部分」）；MB-9 必须显式建模 `MagicMonster.Think`（无 HolySeize 门）与 `AttackTarget`（不伤害、只刷 tick），不能只写 Fireball 的类体。
6. **R16 的 `elfName[^1]`**：C# 的 `string` 末字符 vs Rust/GBK 字节层的末字节；在 `MakeClone` 前必须 spike，避免把多字节汉字尾字节当 `'1'`。
7. **`MakeClone`/`RegenMonsterByName` 的生成抽数**：它们会在 `Run` 的 AI pass 内同步生成实体并抽构造链；必须复用同一 `world.rng` 与同一 `draw_monster_construct` 次序，不能在宿主侧用另一条 RNG。生成回调的 re-entrancy/顺序需在 MB-3/MB-7 的用例里钉住。
8. **`MagCanHitTarget`/`FireBurnEvent`/`event_at`**：三者在基线均已存在（`magic::mag_can_hit_target`、`world.rs` 的 `FireBurnEvent`/`fire_burn_events`/`event_at`/`add_fire_burn_event`）⇒ R19/R18 **直接复用**，不得另建第二套；MB-9 的 P1 设计已假设 `MagCanHitTarget` 的「目标自身坐标」怪癖（见怪物设计件 §6.3），实现时不得修正。
9. **`is_active_seek_race` 的历史缺口（不在本 34 项内）**：怪物设计件 §3.1 指出 `82/84/90/91/92/96/97/100/105/106/119` 中仍有 race 不在索敌集。本规划通过 R43/R21/R10 的 `Run` 体覆盖 92/96/105；但 `82/84/90/91/97/100/106/119` 的「无 Run 覆写 + 不索敌」仍是独立缺口，**不得**在本批顺手扩大 `is_active_seek_race` 而不立卡（会改变所有 race 的评估频率/RNG 流）。
10. **真机窗口**：九游戏端口被外部 C# 栈占用（本件只记录、未停）；R 级窗口需 owner 排期。不可达 race 的 R 级结论只能由 owner 裁决「构造级 + 逐行/抽数 + 内容可达性」替代，review 必须显式写 `window_required: true`。

---

## 7. 实测复现（本件所有行号）

### 7.1 玩家段（`PlayObject.Message.cs`）

```bash
F=src/M2Server/Player/PlayObject.Message.cs
for r in "143 157" "403 441" "442 529" "569 594" "596 635" "636 657" "738 745" "747 763"; do
  set -- $r
  echo "== $1-$2 =="
  sed -n "${1}p;${2}p" "$F"
done
```

实测首/末行（与正文一致）：
- 143 `for (int i = 0; i < 20; i++) //个人定时器` / 157 `}`
- 403 `for (int i = 0; i < UseItems.Length; i++)` / 441 `}`
- 442 `tObjCount = GameGold;` / 529 `}`
- 569 `if (Envir.Flag.boDECHP && ...` / 594 `}`
- 596 `if (SystemShare.Config.HungerSystem)` / 635 `}`
- 636 `if ((HUtil32.GetTickCount() - ExpRateTick) > 1000)` / 657 `}`
- 738 `if (AutoGetExpPoint > 0 && ...` / 745 `}`
- 747 `if (!Death)` / 763 `}`

### 7.2 怪物 `Run` 区间（43 条）

```bash
# 签名行
grep -n "public override void Run()" src/M2Server/Monster/Monsters/*.cs
```

花括号配对实测（本件使用的脚本；任取文件重放）：
```bash
python -I - <<'PY'
import glob,os
for p in sorted(glob.glob('src/M2Server/Monster/Monsters/*.cs')):
    L=open(p,encoding='utf-8-sig',errors='replace').read().split('\n')
    for i,l in enumerate(L):
        if 'override void Run()' in l:
            j=i;d=0;s=False;e=None
            while j<len(L):
                for ch in L[j]:
                    if ch=='{': d+=1;s=True
                    elif ch=='}':
                        d-=1
                        if s and d==0: e=j+1;break
                if e: break
                j+=1
            print(f'{os.path.basename(p)}:{i+1}-{e}')
            break
PY
```

### 7.3 可达性 / 内容（只读）

```bash
grep -a -n "HungerSystem\|HungerDecHP\|RevivalTime" /e/MirServer/M2GameSvr/Server.conf
grep -a -c "DECHP\|INCHP\|DECGAMEGOLD\|INCGAMEGOLD" /e/MirServer/M2GameSvr/Envir/MapInfo.txt
for c in SETAUTOGETEXP AUTOADDGAMEGOLD AUTOSUBGAMEGOLD KILLMONEXPRATE POWERRATE SETONTIMER; do
  echo -n "$c="; grep -a -ril "$c" /e/MirServer/M2GameSvr/Envir --include=*.txt | wc -l
done
```

本次结果：`HungerSystem=0`、`HungerDecHP=0`、`RevivalTime=60000`；`MapInfo.txt` 计数 0；脚本计数 `SETAUTOGETEXP=1`、其余 0（`SETONTIMER=0`）。注意 `SETAUTOGETEXP` 的 1 处调用**无地图参数**，按 oracle 会报错（见 §1.2 P31），所以「脚本出现」不等于「字段被设置」。

---

## 8. 残留物清单（§14.3）

- **仓内新增**：仅本件 `mir2-rs/tests/parity/m3/plan-run-gap-batches-20261018.md`（一个 md）。
- **仓内修改**：无。未动 `src/**`、`sql/**`、`MirClient/`、`crates/{protocol,storage}/**`；未动 `E:/MirServer/**`。
- **worktree**：`/e/tmp/rev-plan`（detached `706c95cae`，应用原 patch 后订正），仅用于写本件与生成 patch；订正提交不 push，`git format-patch -1 --stdout` 输出到 `/e/tmp/plan-rev.patch`。
- **仓外临时物**：`/e/tmp/plan.patch`（原 patch，sha256 `a0725165...614fb9`）、`/e/tmp/plan-rev.patch`（本次订正 patch）、`/e/tmp/revplan_edit.py`（订正替换脚本，逐条 count 断言）、`/e/tmp/coverage.md`（§2.5 草稿）；`/e/tmp/planscripts/runranges.py`、`/e/tmp/planscripts/rundeps.py`（oracle 区间/依赖抽取脚本）；`/e/tmp/test-hd.txt`（heredoc 测试残留，可删）。均不属于仓库。
- **进程/端口**：未起服务、未占端口、未连接 MySQL、未写库；`E:/MirServer` 仅只读 grep。
- **未做（诚实声明）**：未实现任何代码、未跑 `cargo fmt/clippy/test`（本件是纯规划）；未做真机对拍；可达性基于怪物设计件 §2.4 的 2026-10-17 只读实测 + 本次 `MonGen.txt` 抽样复核，未重新查询 MySQL 物品/怪物表；P32 的戒指物品存在性、P21 的 `Need` 物品分布均标 spike，未在本件下结论。
