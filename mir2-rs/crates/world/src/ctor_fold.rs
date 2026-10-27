//! **构造段抽数 fold**（`openmir2-ctor-draws-fold`）——把 oracle 启动期**非 `Merchant` 家族之外的
//! 构造点**按 oracle 次序补进同一本 RNG 账本（**只补抽、不建实体**）。
//!
//! ## 为什么需要本片
//!
//! §4.4（`tests/parity/m3/design-buff-and-tick-20261016.md`）的窗口判据要求**两侧账本从 `seq=0`
//! 起逐项相等**。oracle 的 `GameApp` 启动链在**任何** NPC Run 之前就抽掉一批构造数：
//!
//! | 段 | oracle 锚 | 构造点 | 每处抽数 |
//! | --- | --- | --- | --- |
//! | ① `GameApp` ctor | `GameApp.cs:58` → `RobotManage.cs:8-12/21-49`（`:48`） | `Robot.txt` **每有效行**一个 `new RobotObject()` | 4 |
//! | ① `GameApp` ctor | `GameApp.cs:85`（G1）/`:86`（G2）/`:87`（G3） | 三只**孤儿** `new Merchant()`（不入任何列表，被 `LocalDB` 覆盖） | 4×3 |
//! | ★屏障 A（**已补**） | `AppService.cs:87` → `GameShare.cs:90` → `ServerConf.cs:114` | `SystemShare.Config.ItemNumber += Random(10000)`（[`CtorFoldSite::BarrierAItemNumber`]） | **1** |
//! | ② `GameApp.Initialize` | `GameApp.cs:124` → `Maps.cs:137` → `:454` | `MapInfo` 每个 `CHECKQUEST(x)` 行一个 `new Merchant` | 4×行数 |
//! | ★屏障 B（已建模） | `GameApp.cs:155` `WorldEngine.InitializeMonster()` | 宿主 `mon_gen`（`WorldServer.MonGen.cs:66/94` 的 `Random(limit)`） | — |
//! | ③ `GameApp.Initialize` | `GameApp.cs:184` → `MapQuestManager.cs:40` | `MapQuest.txt` **每个有效行**一个 `new Merchant` | 4×有效行数 |
//! | ③ `GameApp.Initialize` | `GameApp.cs:184` 尾 → `LocalDB.cs:403/404/405` → `:268/:210/:312` | 三只隐藏 `new Merchant`（`QManage`→`Function`→`RobotManage`） | 4×存在数 |
//! | ★屏障 C（已建模） | `GameApp.cs:227`/`:229` | `LoadMerchant`/`LoadNpcs`（见 `merchant.rs`/`npc.rs`） | 4×条 |
//!
//! 每处 `new Merchant`/`new RobotObject` 都走同一条 ctor 公共链
//! （`BaseObject` `:439/441` + `AnimalObject` `:76/77`）⇒ **恒 4 抽**，ctor 链的唯一抽数点仍是
//! [`BaseAnimalCtorDraws::draw`]（不重复模数表）。**唯一的 1 抽例外**是屏障 A——它不是 ctor 链上的
//! 构造点，而是 `ServerConf.cs:114` 的 `Random(10000)`，由
//! [`World::fold_ctor_draw_item_number_barrier_a`] 单独抽（`arg = `[`CTOR_FOLD_BARRIER_A_MOD`]）。
//! 实体缺口（`RobotObject`/任务 NPC/隐藏 NPC 的世界实体）**另立条目**，不在本片。
//!
//! ## 四条阶段硬约束（次序即位置）
//!
//! fold 不是「一共补 N 抽」就够——**位置**决定账本能否逐项对齐。本模块按 oracle 的三条**屏障**
//! 切成四段调用点（屏障 A 自带 1 抽，不是「一段」而是「一处」），宿主必须按
//! [`World::fold_ctor_draws_game_app_ctor`] → [`World::fold_ctor_draw_item_number_barrier_a`] →
//! [`World::fold_ctor_draws_load_map_info`] → [`World::fold_ctor_draws_load_map_quest`] 的次序
//! 调用，且中间两处**别的东西**的抽数不许挪：
//!
//! - **屏障 A**（`AppService.cs:87` `GameShare.LoadConfig()` → `ServerConf.cs:114` 的
//!   `Random(10000)`，**恰 1 抽，已补**）：它在 `GameApp` ctor **之后**、
//!   `GameApp.Initialize`（⇒ M2 `:124`）**之前**——`new GameApp(...)` 由 DI 在
//!   `AppService` 构造时发生，`StartingAsync` 里才 `LoadConfig()`。⇒ 段①与段②之间**就是**它：
//!   [`World::fold_ctor_draw_item_number_barrier_a`] 必须在这两段**之间**、且**恰一次**。
//!   它**不**走 ctor 公共链（`arg = 10000`、**1 抽**），且抽出的值**不是丢弃**的——宿主把它
//!   **加到** `[Setup] ItemNumber` 初值上（oracle 是 `+=`；宿主
//!   `bins/game-svr/src/ctor_fold.rs::item_number_counter_init`）。
//! - **屏障 B**（`GameApp.cs:155` `InitializeMonster`）：段②与段③之间的抽数属宿主 `mon_gen`，
//!   **不许**把段③提到它前面（否则 M1/L1-L3 与 `mon_gen` 互换位置、整本账本错位）。
//! - **屏障 C**（`GameApp.cs:227`/`:229`）：段③**必须早于** `LoadMerchant`（L4）/`LoadNpcs`（L5）
//!   （oracle `:184` < `:227`/`:229`）。
//!
//! ## 登记（不可达 / 未做 / 已补）
//!
//! - **L6 `LocalDB.cs:1067`（`ReLoadMerchants`）= N/A（不可达）**：唯一调用者是
//!   `ReloadNpcCommand.Execute`（`src/Modules/GameCommand/Commands/ReloadNpcCommand.cs:14`），
//!   而该 `Execute` **体整段被注释**（`:24` 起的 `M2Share.LocalDb.ReLoadMerchants();` 在 `//` 内）
//!   ⇒ 启动期与当前发布命令集都到不了 ⇒ 构造段**抽 0**、不 fold。
//! - **X1 `MobNpcCommand.cs:36` = N/A**：`//var merchant = new Merchant();` 源码已注释。
//! - **实体缺口（本片只 fold 抽数、不建实体）**：`RobotObject` 世界实体 + `AutoRun` 驱动器；
//!   M1/M2 任务 NPC 进 `QuestNpcList`；
//!   ~~L1 `QFunction` 进 `MerchantList`（`AddMerchant`）~~ **已接**（2026-10-24，卡
//!   `openmir2-qfunction-hidden-merchant-world-wiring`：宿主 `bins/game-svr` 用本账本
//!   `CtorFoldSite::HiddenQFunction` 的第 1 抽当 `RunTick` 建实体，见
//!   `ctor_fold::hidden_qfunction_run_tick`）；
//!   L2/L3 进 `QuestNpcList` + `ActorId` 按序分配 —— 仍另立条目，不在本片。
//! - **构造段「真机账本从 `seq=0` 逐项对齐」的逐条状态**：
//!   ① `GameApp.cs:55-58` 与本片同段的 `GameCommandSystem`/`AuthenticationService`/`ScriptEngine`
//!   **已核实读不抽**（`CommandSystem.cs:19-22`/`AuthenticationService.cs:15-20`/
//!   `ScriptEngine.cs:30-36`，见 `bins/game-svr/src/ctor_fold.rs` 模块头）；
//!   ② `GameApp.cs:88-93` 的 `ItemSystem`/`GuildMgr`/`CastleMgr`/`MapMgr`/`EventMgr`/`WorldServer`
//!   六个 ctor **已核实读不抽**（同上；这与 §4.4.2 的「须一并核」清单逐条对上）；
//!   ③ **已补（屏障 A）**：`ServerConf.cs:114` 的 `Random(10000)`（1 抽）——宿主在段①之后、
//!   段②之前恰调一次 [`World::fold_ctor_draw_item_number_barrier_a`]，并由
//!   `item_number_counter_init` 把该抽**加到** `bins/game-svr/src/config.rs` 读到的
//!   `[Setup] ItemNumber` 初值上（消费点**已建模**：该计数器 → `M2Share.GetItemNumber()`
//!   (`M2Share.cs:547`) → 新角色新手物品 `MakeIndex`，`lib.rs::add_newbie_items`）。⇒ 段① 在部署件上
//!   是 **16 抽**（`Robot.txt` 1 有效行 × 4 + 孤儿 G1–G3 × 4），屏障 A 即**第 17 条**，其后段②/段③
//!   与 oracle **整段等距**。
//!   ④ 运行期路径（`M2Share.cs:279/:526`、`ScriptEngine.cs:156`）不在启动构造段内，不计。
//!   **⇒ 构造段已无阻塞项（可整段等距）**：上面 ①② 已核「读不抽」、③ 已补；余下的缺口只在
//!   **消费点/实体层**（「实体缺口」那条：世界实体、`QuestNpcList`、`MerchantList` 入表等），
//!   它们**不改变抽数的位置与条数**，故不影响账本逐项对齐。
//!
//! ## 实测计数与影响面（2026-10-17 复验订正，**行号/数字以此段为准**）
//!
//! 用生产解析器对部署件（`E:/MirServer/M2GameSvr/Envir`）实测：
//! `Robot.txt` 有效行 **1** ⇒ 4 抽；`CHECKQUEST` **9** ⇒ 36 抽；`MapQuest.txt` 有效行 **1** ⇒ 4 抽
//! （唯一行 `Q001` **未被 FindMap 门丢**——`MapInfo.txt:16 [Q001 勇斗寺殿]` 的 `map_file_name` 是
//! `Q001` 而非描述串，且 `Map/Q001.map` 存在 ⇒ oracle `FindMap("Q001")` 命中 ⇒ `CreateQuest` 建对象）；
//! 隐藏 NPC **3** ⇒ 12 抽；再加**屏障 A 的 1 抽** ⇒ **共 69 抽 / 18 站点**（原提交信息写的
//! 「MapQuest 0 行 / 64 抽 / 16 站点」系把这个计数算错，已订正；`openmir2-ctor-draws-fold` 的
//! 「68 抽 / 17 站点」是**未含**屏障 A 的值，本片补上后为 69 抽 / 18 站点）。
//! **⇒ 段②/段③ 与 oracle 整段等距。**
//!
//! **影响面订正**：`push_orphan_merchants()`（G1–G3）是**无条件**的 ⇒ 计划**永不为空**：任何到达
//! `sim_loop` 且 registry 非空的会话都会多抽至少 12 个（与语料里有没有 `Robot.txt`/`CHECKQUEST`/
//! `MapQuest.txt` 无关）。实测：既有进程级用例
//! `game-svr/tests/relay.rs::starts_without_worldmaps_when_envir_present`（合成 Envir 只有
//! `MapInfo.txt="[0 town]"` + `Map/0.map`）在父提交 `befd3ad7c` 上 RNG 流水 **0 条**抽数行、
//! `openmir2-ctor-draws-fold` 提交 **12 条**（`entity.rs` 的 construct 组）、本片（屏障 A 已补）
//! **13 条**——多出的第 13 条是屏障 A，记在**宿主调用点** `bins/game-svr/src/lib.rs`（= 调度表的
//! `startup` 组，见下节）。**套件仍全绿**只因为仓内没有用例/golden 钉住这类会话的 RNG 流水
//! —— 原提交信息的「既有用例/golden 0 条受影响 ⇒ 只在真机可见」**不成立**，已由本段订正；
//! R 级窗口的必要性据此**加强**（默认流水已变）。
//!
//! **另一处流水变化（本片新增）**：**无地图注册表**（或计划算不出）的会话原本**一条都不抽**
//! （空计划），本片改用 `minimal_plan()`（**只含屏障 A**）⇒ 这类会话的流水 **0 → 1 条**
//! （实测 `game-svr/tests/relay.rs::ids_notification_enables_login_certification`，同种子
//! `MIR2_RNG_LOG`：父提交 0 条、本片 1 条 = `lib.rs` 的 `Next(max) 10000`）。oracle 在该情形下
//! **照样抽**（`ServerConf.LoadConfig()` 与 `Envir` 无关）⇒ 方向与 oracle 一致；段①②③ 仍不落账
//! （`lib.rs` 既有登记：无注册表 ⇒ 算不出站点表），本片只把「与语料无关的那 1 抽」补上。
//!
//! ## 与 `rng_trace` 的关系
//!
//! 本模块的抽数走共享 `world.rng` ⇒ `MIR2_RNG_LOG` 流水里每处仍是
//! `crates/world/src/entity.rs:106-110`（`construct` 组，4 行/处）；**唯一例外**是屏障 A：
//! [`World::fold_ctor_draw_item_number_barrier_a`] 标了 `#[track_caller]` ⇒ 流水里记的是
//! **调用方**的站点（生产 = `bins/game-svr/src/lib.rs`，与 `consumer_groups.tsv` 既有的
//! `startup|rust|bins/game-svr/src/lib\.rs:` 行同组；oracle 侧同组 =
//! `ServerConf.LoadConfig@ServerConf.cs:114`）。⇒ 这一抽**不落 `other` 组**，结构一致性门的
//! `groupdiff --require-closed` 不会因本片新增站点而判红。[`CtorFoldLedger`] 是**同一批
//! 抽数的符号化投影**（site = oracle 锚），供构造级用例把「次序」也钉死——真机流水的 site 列
//! 两侧都区分不出这些同形构造点（oracle 侧同为 `BaseObject..ctor`/`AnimalObject..ctor`），
//! 只有本账本的 site 列能咬住次序错位。
//!
//! ## 登录批（卡 `openmir2-player-ctor-rng-order`）
//!
//! 启动期之外，**登录玩家的 `new PlayObject()`（`WorldServer.cs:234`）也会无条件消耗全局
//! RNG**——继承链 `PlayObject : CharacterObject : AnimalObject : BaseObject` 的 ctor 公共链
//! 共 **5 抽**：`BaseObject.cs:439` `Random(1500)`、`:441` `Random(2000)`、
//! `AnimalObject.cs:76-77` `Random(3000)` ×2、`PlayObject.cs:1047` `Random(999999)`（`RandomNo`）。
//! 派生 ctor 覆盖 `RunTick`/`SearchTime`/`AttackTick`，但 **`WalkTick` 与 `RandomNo` 存活**。
//!
//! 登录批用 [`World::fold_login_player_ctor_draws`] 抽、[`CtorFoldLedger::batch`] =
//! [`CtorDrawBatch::Login`] 与启动期账本显式分桶：前 4 抽复用同一抽数点
//! [`BaseAnimalCtorDraws::draw`]（trace site 与启动期相同，**不能只靠 site 区分**），第 5 抽
//! 单独追加。调用位置契约：**早于** `HomeMap` 出生点抽数（`WorldServer.cs:254-259`）与
//! `rewrite_enter_position`/`shift_enter_position`（`:306/:339/:346/:380`），且只在非重复登录
//! 分支（oracle `:503` 的 `ProcessHumansIsLogined` 门在 `ProcessHumansMakeNewHuman` 之前）。

use crate::entity::{BaseAnimalCtorDraws, BASE_ANIMAL_CTOR_DRAW_MODS};
use crate::World;

/// 构造段 fold 的**符号化 oracle 站点**（[`CtorFoldLedger`] 的 `site` 列）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum CtorFoldSite {
    /// `RobotManage.cs:48` `new RobotObject()`（`Robot.txt` 每有效行一处）。
    RobotObject,
    /// `GameApp.cs:85` `SystemShare.ManageNPC = new Merchant()`（孤儿 G1）。
    OrphanManageNpc,
    /// `GameApp.cs:86` `SystemShare.RobotNPC = new Merchant()`（孤儿 G2）。
    OrphanRobotNpc,
    /// `GameApp.cs:87` `SystemShare.FunctionNPC = new Merchant()`（孤儿 G3）。
    OrphanFunctionNpc,
    /// `Maps.cs:454` `new Merchant`（`MapInfo` 每个 `CHECKQUEST(x)` 一行）。
    MapCheckQuest,
    /// `MapQuestManager.cs:40` `new Merchant`（`MapQuest.txt` 每个有效行一行）。
    MapQuestRow,
    /// `LocalDB.cs:268` `QMangeNpc()` 的 `new Merchant{ChrName="QManage"}`。
    HiddenQManage,
    /// `LocalDB.cs:210` `QFunctionNpc()` 的 `new Merchant{ChrName="QFunction"}`。
    HiddenQFunction,
    /// `LocalDB.cs:312` `RobotNpc()` 的 `new Merchant{ChrName="RobotManage"}`。
    HiddenRobotManage,
    /// **屏障 A**：`ServerConf.cs:114` `SystemShare.Config.ItemNumber +=
    /// SystemShare.RandomNumber.Random(10000)`（`AppService.cs:87` 的 `GameShare.LoadConfig()` 链）
    /// —— **恰 1 抽**，且抽数要 `+=` 到 `[Setup] ItemNumber` 上（不是 ctor 链的 4 抽、也不是丢弃）。
    BarrierAItemNumber,
    /// **登录批第 1 抽**：`WorldServer.cs:234` `new PlayObject()` ⇒ `BaseObject.cs:439`
    /// `RunTick = GetTickCount() + Random(1500)`（值随后被 `PlayObject.cs:898` 覆盖，仅占流位）。
    PlayerBaseRunTick,
    /// **登录批第 2 抽**：`BaseObject.cs:441` `SearchTime = Random(2000) + 2000`
    /// （值随后被 `PlayObject.cs:900` 覆盖为 1000，仅占流位）。
    PlayerBaseSearchTime,
    /// **登录批第 3 抽**：`AnimalObject.cs:76` `AttackTick = GetTickCount() - Random(3000)`
    /// （值随后被 `PlayObject.cs:918` 覆盖，仅占流位）。
    PlayerAnimalAttackTick,
    /// **登录批第 4 抽**：`AnimalObject.cs:77` `WalkTick = GetTickCount() - Random(3000)`
    /// （**存活**：`PlayObject` ctor 不覆盖 `WalkTick`）。
    PlayerAnimalWalkTick,
    /// **登录批第 5 抽**：`PlayObject.cs:1047` `RandomNo = Random(999999).ToString()`（**存活**）。
    PlayerRandomNo,
}

impl CtorFoldSite {
    /// 该站点的 oracle 锚（`类@文件.cs:行`，与 `rng-trace-probe/consumer_groups.tsv` 同口径）。
    #[must_use]
    pub fn oracle_anchor(self) -> &'static str {
        match self {
            CtorFoldSite::RobotObject => "RobotObject..ctor@RobotManage.cs:48",
            CtorFoldSite::OrphanManageNpc => "Merchant..ctor@GameApp.cs:85",
            CtorFoldSite::OrphanRobotNpc => "Merchant..ctor@GameApp.cs:86",
            CtorFoldSite::OrphanFunctionNpc => "Merchant..ctor@GameApp.cs:87",
            CtorFoldSite::MapCheckQuest => "Merchant..ctor@Maps.cs:454",
            CtorFoldSite::MapQuestRow => "Merchant..ctor@MapQuestManager.cs:40",
            CtorFoldSite::HiddenQManage => "Merchant..ctor@LocalDB.cs:268",
            CtorFoldSite::HiddenQFunction => "Merchant..ctor@LocalDB.cs:210",
            CtorFoldSite::HiddenRobotManage => "Merchant..ctor@LocalDB.cs:312",
            CtorFoldSite::BarrierAItemNumber => "ServerConf.LoadConfig@ServerConf.cs:114",
            CtorFoldSite::PlayerBaseRunTick => {
                "PlayObject..ctor@WorldServer.cs:234->BaseObject.cs:439"
            }
            CtorFoldSite::PlayerBaseSearchTime => {
                "PlayObject..ctor@WorldServer.cs:234->BaseObject.cs:441"
            }
            CtorFoldSite::PlayerAnimalAttackTick => {
                "PlayObject..ctor@WorldServer.cs:234->AnimalObject.cs:76"
            }
            CtorFoldSite::PlayerAnimalWalkTick => {
                "PlayObject..ctor@WorldServer.cs:234->AnimalObject.cs:77"
            }
            CtorFoldSite::PlayerRandomNo => {
                "PlayObject..ctor@WorldServer.cs:234->PlayObject.cs:1047"
            }
        }
    }
}

/// 每处构造点的抽数（`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`，恒 4）。
pub const CTOR_FOLD_DRAWS_PER_SITE: usize = 4;

/// fold 抽数的 `kind` 列（与 `crates/shared/src/rng.rs::random_below` 的
/// `record("Next(max)", value, …)` 同口径；`bins/game-svr/tests/ctor_fold_rng_trace.rs` 与真流水互核）。
pub const CTOR_FOLD_DRAW_KIND: &str = "Next(max)";

/// **屏障 A**：oracle `AppService.cs:87` → `GameShare.cs:90` → `ServerConf.cs:114` 的
/// `Random(10000)`（`SystemShare.Config.ItemNumber += …`）——**恰 1 抽**（站点数 =
/// [`CtorFoldPlan::barrier_a`] 的长度，恒 1）。
///
/// 它在段①（`GameApp` ctor）之后、段②（`:124` `LoadMapInfo`）之前 ⇒
/// [`World::fold_ctor_draw_item_number_barrier_a`] 必须在这两段**之间**、且**恰一次**。
pub const CTOR_FOLD_BARRIER_A_DRAWS: usize = 1;

/// 屏障 A 的抽数上界（oracle `ServerConf.cs:114` 的 `Random(10000)`）。
///
/// `kind` 仍是 [`CTOR_FOLD_DRAW_KIND`]（`Next(max)`，与 `rng_trace` 同列）；与 ctor 公共链的
/// `1500/2000/3000/3000` **不同表**。
pub const CTOR_FOLD_BARRIER_A_MOD: i64 = 10_000;

/// **登录批第 5 抽**的上界（oracle `PlayObject.cs:1047` `Random(999999)`，`RandomNo`）。
///
/// 前 4 抽复用 [`BASE_ANIMAL_CTOR_DRAW_MODS`]（`1500/2000/3000/3000`）；本常量只给第 5 抽。
pub const PLAYER_CTOR_RANDOM_NO_MOD: i32 = 999_999;

/// 登录批 `new PlayObject()`（`WorldServer.cs:234`）的 5 抽结果 + 账本。
///
/// 抽数次序 = oracle 继承链：`BaseObject`（`1500`、`2000`）+ `AnimalObject`
/// （`3000`、`3000`）+ `PlayObject`（`999999`）。派生 ctor 覆盖
/// `RunTick`/`SearchTime`/`AttackTick`，但 [`Self::walk_tick_delta`] 与
/// [`Self::random_no`] **存活**（消费面见 [`crate::Entity::random_no`] 与
/// `AnimalState::walk_tick`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LoginPlayerCtorDraws {
    /// 第 1 抽 `Random(1500)`（`RunTick` 增量；被 `PlayObject.cs:898` 覆盖，仅占流位）。
    pub run_tick_delta: i32,
    /// 第 2 抽 `Random(2000) + 2000`（`SearchTime`；被 `PlayObject.cs:900` 覆盖为 1000）。
    pub base_search_time: i32,
    /// 第 3 抽 `Random(3000)`（`AttackTick` 增量；被 `PlayObject.cs:918` 覆盖）。
    pub attack_tick_delta: i32,
    /// 第 4 抽 `Random(3000)`（`WalkTick` 增量；**存活**）。
    pub walk_tick_delta: i32,
    /// 第 5 抽 `Random(999999)`（`RandomNo`；**存活**）。
    pub random_no: i32,
    /// 5 条符号化账本（`batch = Login`，site 与 oracle 逐抽对应）。
    pub ledger: CtorFoldLedger,
}

impl LoginPlayerCtorDraws {
    /// `WalkTick = ctor_tick - Random(3000)`（oracle `AnimalObject.cs:77`，`GetTickCount()` 取
    /// 构造时刻；派生 `PlayObject` ctor 不覆盖 `WalkTick`）。
    #[must_use]
    pub fn walk_tick(&self, ctor_tick: i64) -> i64 {
        ctor_tick - i64::from(self.walk_tick_delta)
    }
}

/// **构造段 fold 计划**：按 oracle 次序排好的站点表，按三条屏障切成三段（见模块头）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtorFoldPlan {
    /// 段①（`GameApp` ctor）：`GameApp.cs:58`（RobotObject）+ `:85-87`（G1–G3）。
    /// **调用时机**：早于屏障 A（`ServerConf.cs:114` 的 1 抽）。
    pub game_app_ctor: Vec<CtorFoldSite>,
    /// **屏障 A**（0 或 1 个 [`CtorFoldSite::BarrierAItemNumber`]）：`AppService.cs:87`
    /// `GameShare.LoadConfig()` → `ServerConf.cs:114` 的 `Random(10000)`。
    /// **调用时机**：晚于段①、早于段②；**每站点 1 抽**（不是 ctor 公共链的 4 抽）。
    pub barrier_a: Vec<CtorFoldSite>,
    /// 段②（`GameApp.Initialize` 的 `:124`）：`MapInfo` 的 `CHECKQUEST`。
    /// **调用时机**：晚于屏障 A、早于屏障 B（`InitializeMonster` `:155`）。
    pub load_map_info: Vec<CtorFoldSite>,
    /// 段③（`GameApp.Initialize` 的 `:184`）：M1 + L2/L1/L3。
    /// **调用时机**：晚于屏障 B、早于屏障 C（`LoadMerchant`/`LoadNpcs` `:227`/`:229`）。
    pub load_map_quest: Vec<CtorFoldSite>,
}

impl CtorFoldPlan {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 段①：`Robot.txt` **有效行数** × `new RobotObject()`（`GameApp.cs:58`）。
    ///
    /// `lines` 必须是**有效行数**（两列非空，见 `mir2_envir::robot::parse_robot_lines`），
    /// 不是文件行数——注释行/单列行不建对象、不抽数。
    pub fn push_robot_objects(&mut self, lines: usize) {
        self.game_app_ctor
            .extend(std::iter::repeat_n(CtorFoldSite::RobotObject, lines));
    }

    /// 段①：G1–G3 三只**孤儿** `new Merchant()`（`GameApp.cs:85-87`，恒 3 处）。
    pub fn push_orphan_merchants(&mut self) {
        self.game_app_ctor.extend([
            CtorFoldSite::OrphanManageNpc,
            CtorFoldSite::OrphanRobotNpc,
            CtorFoldSite::OrphanFunctionNpc,
        ]);
    }

    /// **屏障 A**：`AppService.cs:87` → `ServerConf.cs:114` 的 `Random(10000)`（恒 1 站点）。
    ///
    /// 计划里**恒有**它（`build_plan` 无条件 push）；宿主**必须**在段①之后、段②之前**恰调一次**
    /// （漏掉 ⇒ 段②/段③ 的绝对位置差 1，账本自第 17 条起分歧）。
    pub fn push_barrier_a(&mut self) {
        self.barrier_a.push(CtorFoldSite::BarrierAItemNumber);
    }

    /// 段②：`MapInfo` 的 `CHECKQUEST` 行数 × `new Merchant`（`GameApp.cs:124` → `Maps.cs:454`）。
    pub fn push_map_check_quests(&mut self, lines: usize) {
        self.load_map_info
            .extend(std::iter::repeat_n(CtorFoldSite::MapCheckQuest, lines));
    }

    /// 段③：`MapQuest.txt` **有效行数** × `new Merchant`（`GameApp.cs:184` →
    /// `MapQuestManager.cs:40`；`LocalDB.CreateQuest` 返 true 才建对象 ⇒ 计数取
    /// `MapQuestLoad::quests.len()`）。
    pub fn push_map_quest_rows(&mut self, rows: usize) {
        self.load_map_quest
            .extend(std::iter::repeat_n(CtorFoldSite::MapQuestRow, rows));
    }

    /// 段③：一只隐藏脚本 NPC（`LocalDB.cs:403/404/405` 的 `QMangeNpc`/`QFunctionNpc`/`RobotNpc`）。
    pub fn push_hidden_npc(&mut self, site: CtorFoldSite) {
        debug_assert!(matches!(
            site,
            CtorFoldSite::HiddenQManage
                | CtorFoldSite::HiddenQFunction
                | CtorFoldSite::HiddenRobotManage
        ));
        self.load_map_quest.push(site);
    }

    /// ctor 公共链的抽数（三段的站点数 × 4；**不含**屏障 A 的 1 抽）。
    #[must_use]
    pub fn ctor_chain_draws(&self) -> usize {
        (self.game_app_ctor.len() + self.load_map_info.len() + self.load_map_quest.len())
            * CTOR_FOLD_DRAWS_PER_SITE
    }

    /// 计划总抽数（= ctor 公共链的 `站点数 × 4` + 屏障 A 的 1 抽）。
    #[must_use]
    pub fn total_draws(&self) -> usize {
        self.ctor_chain_draws() + self.barrier_a.len()
    }
}

/// 一条构造抽账目（与实际抽数一一对应）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CtorFoldEntry {
    /// 符号化 oracle 站点。
    pub site: CtorFoldSite,
    /// 抽数形态（恒 [`CTOR_FOLD_DRAW_KIND`]，与 `rng_trace` 同列同形）。
    pub kind: &'static str,
    /// 抽数上界（`Random(arg)` 的 `arg`；`BASE_ANIMAL_CTOR_DRAW_MODS` 的第 i 项）。
    pub arg: i64,
    /// **原始抽取结果**（`rng_trace` 的 `result` 列口径：`Random(2000)+2000` 只记 `Random(2000)`
    /// 的原值，不含 `+2000` 偏移）。
    pub result: i64,
}

/// **抽数批次**：同一 oracle 构造链在启动期与登录期各出现一次。前 4 抽的 `rng_trace`
/// site 相同（都在 [`BaseAnimalCtorDraws::draw`] 内）⇒ 账本必须显式分桶，
/// **不能只按 site/流水文件行区分**（卡 `openmir2-player-ctor-rng-order` A3）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CtorDrawBatch {
    /// 启动期 `ctor_fold`（段①/屏障 A/段②/段③）。
    #[default]
    Startup,
    /// 登录期 `new PlayObject()`（`WorldServer.cs:234`，每次登录 5 抽）。
    Login,
}

/// 构造段 fold 账本（按抽数次序）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtorFoldLedger {
    /// 抽数批次（startup vs login）。[`Self::by_site`] 只对 startup 的 4 抽/站点账本成立。
    pub batch: CtorDrawBatch,
    pub entries: Vec<CtorFoldEntry>,
}

impl CtorFoldLedger {
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// ctor 公共链账本逐站点折叠成 `(site, 该处 4 抽的原值)` 的视图（判据用例的期望表按此写）。
    ///
    /// **只对 4 抽/站点的 startup ctor 链账本成立**：尾余数（屏障 A 的 1 条）被 `as_chunks`
    /// 丢掉 ⇒ 不要对 [`World::fold_ctor_draw_item_number_barrier_a`] 的返回值用它（直接读
    /// `entries`）；登录批是 **5 抽/次** 且 `batch = Login` ⇒ 也不得用它。
    #[must_use]
    pub fn by_site(&self) -> Vec<(CtorFoldSite, [i64; CTOR_FOLD_DRAWS_PER_SITE])> {
        debug_assert_eq!(
            self.batch,
            CtorDrawBatch::Startup,
            "by_site 只适用于 startup 的 4 抽/站点账本"
        );
        self.entries
            .as_chunks::<CTOR_FOLD_DRAWS_PER_SITE>()
            .0
            .iter()
            .map(|c| {
                (
                    c[0].site,
                    [c[0].result, c[1].result, c[2].result, c[3].result],
                )
            })
            .collect()
    }
}

impl World {
    /// **段①**：`GameApp` ctor 的构造抽（`GameApp.cs:58` RobotObject / `:85-87` G1–G3）。
    ///
    /// **必须早于屏障 A**（`AppService.cs:87` → `ServerConf.cs:114` 的 `Random(10000)`，未建模）。
    ///
    /// 只抽数、不建实体（fold）：抽出的 4 个分量**全部丢弃**，仅推进共享 `world.rng` 位置。
    pub fn fold_ctor_draws_game_app_ctor(&mut self, plan: &CtorFoldPlan) -> CtorFoldLedger {
        Self::fold_sites(self, &plan.game_app_ctor)
    }

    /// **屏障 A**：oracle `AppService.cs:87` → `GameShare.cs:90` → `ServerConf.cs:114` 的
    /// `SystemShare.Config.ItemNumber += SystemShare.RandomNumber.Random(10000)`（**恰 1 抽**）。
    ///
    /// **必须晚于段①（[`Self::fold_ctor_draws_game_app_ctor`]）、早于段②
    /// （[`Self::fold_ctor_draws_load_map_info`]）**。`plan.barrier_a` 为空 ⇒ 不抽、返回空账本
    /// （**宿主侧的 fail-closed 在** `bins/game-svr/src/ctor_fold.rs::item_number_counter_init`：
    /// 账本不恰 1 条即报错，绝不静默按 0 加到 `ItemNumber` 上）。
    ///
    /// 与另三段两点不同：① 每站点 **1 抽**（不是 ctor 公共链的 4 抽）、`arg =
    /// `[`CTOR_FOLD_BARRIER_A_MOD`]；② 抽出的值**不是丢弃**的——宿主把它**加到** `[Setup]
    /// ItemNumber` 上（oracle 是 `+=`，`ServerConf.cs:114`）。
    ///
    /// `#[track_caller]`：本方法**故意**让 `rng_trace`（`MIR2_RNG_LOG`）把这一抽记在**调用方**的
    /// 站点（生产 = `bins/game-svr/src/lib.rs`）——`consumer_groups.tsv` 已把 Rust 侧启动期抽数
    /// 归因到 `startup|rust|bins/game-svr/src/lib\.rs:`（oracle 侧同组 =
    /// `ServerConf.LoadConfig@ServerConf.cs:114`）；不标则记成 `crates/world/src/ctor_fold.rs:`，
    /// 在结构一致性门里落 `other` 组 ⇒ `groupdiff --require-closed` 判红。
    #[track_caller]
    pub fn fold_ctor_draw_item_number_barrier_a(&mut self, plan: &CtorFoldPlan) -> CtorFoldLedger {
        let mut ledger = CtorFoldLedger {
            batch: CtorDrawBatch::Startup,
            entries: Vec::with_capacity(plan.barrier_a.len()),
        };
        for &site in &plan.barrier_a {
            debug_assert_eq!(
                site,
                CtorFoldSite::BarrierAItemNumber,
                "屏障 A 段只装 BarrierAItemNumber 站点"
            );
            ledger.entries.push(CtorFoldEntry {
                site,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: CTOR_FOLD_BARRIER_A_MOD,
                result: i64::from(self.rng.random_below(CTOR_FOLD_BARRIER_A_MOD as i32)),
            });
        }
        ledger
    }

    /// **段②**：`GameApp.cs:124` `Map.LoadMapInfo()` 的 `CHECKQUEST` 构造抽（`Maps.cs:454`）。
    ///
    /// **必须晚于屏障 A、早于屏障 B**（`InitializeMonster`，`GameApp.cs:155` =
    /// 宿主 `MonGenHost::start` 的 `Random(limit)`）。
    pub fn fold_ctor_draws_load_map_info(&mut self, plan: &CtorFoldPlan) -> CtorFoldLedger {
        Self::fold_sites(self, &plan.load_map_info)
    }

    /// **段③**：`GameApp.cs:184` `LocalDB.LoadMapQuest()` 的构造抽（M1 + 尾 L2/L1/L3）。
    ///
    /// **必须晚于屏障 B、早于屏障 C**（`LoadMerchant` `:227` / `LoadNpcs` `:229`，见
    /// `merchant.rs`/`npc.rs`）。
    pub fn fold_ctor_draws_load_map_quest(&mut self, plan: &CtorFoldPlan) -> CtorFoldLedger {
        Self::fold_sites(self, &plan.load_map_quest)
    }

    /// fold 本体：逐站点走**同一条** ctor 公共链（唯一抽数点 [`BaseAnimalCtorDraws::draw`]）。
    fn fold_sites(&mut self, sites: &[CtorFoldSite]) -> CtorFoldLedger {
        let mut ledger = CtorFoldLedger {
            batch: CtorDrawBatch::Startup,
            entries: Vec::with_capacity(sites.len() * CTOR_FOLD_DRAWS_PER_SITE),
        };
        for &site in sites {
            let d = BaseAnimalCtorDraws::draw(&mut self.rng);
            // `result` 与原值互逆：第 2 项 oracle 是 `Random(2000) + 2000`（`BaseObject.cs:441`），
            // 而 `rng_trace` 记的是 `Random(2000)` 的**原值** ⇒ 这里减回偏移，保持两本账同列同值。
            let results = [
                i64::from(d.run_tick_delta),
                i64::from(d.base_search_time - BASE_ANIMAL_CTOR_DRAW_MODS[1]),
                i64::from(d.attack_tick_delta),
                i64::from(d.walk_tick_delta),
            ];
            for (i, result) in results.into_iter().enumerate() {
                ledger.entries.push(CtorFoldEntry {
                    site,
                    kind: CTOR_FOLD_DRAW_KIND,
                    arg: i64::from(BASE_ANIMAL_CTOR_DRAW_MODS[i]),
                    result,
                });
            }
        }
        ledger
    }

    /// **登录批**：oracle `WorldServer.cs:234` `new PlayObject()` 的 **5 抽**（继承链顺序）。
    ///
    /// 次序 = ① `BaseObject.cs:439` `Random(1500)` → ② `:441` `Random(2000)` →
    /// ③ `AnimalObject.cs:76` `Random(3000)` → ④ `:77` `Random(3000)` →
    /// ⑤ `PlayObject.cs:1047` `Random(999999)`。
    ///
    /// 前 4 抽**复用同一抽数点** [`BaseAnimalCtorDraws::draw`]（一处定义，防「少抽 2 次」复发），
    /// 第 5 抽单独追加；账本 `batch = Login`，与启动期显式分桶（前 4 抽的 `rng_trace` site 与
    /// 启动期相同，**不能只靠 site 区分**）。
    ///
    /// 调用位置契约（宿主 `bins/game-svr/src/lib.rs`）：
    /// - **只在非重复登录分支**（oracle `:503` 的 `ProcessHumansIsLogined` 门在
    ///   `ProcessHumansMakeNewHuman` 之前 ⇒ 重复登录不构造、不抽）；
    /// - **早于** `HomeMap` 出生点抽数（`:254-259`）与 `rewrite_enter_position`/
    ///   `shift_enter_position`（`:306/:339/:346/:380`）；
    /// - **不得**放进 `new_player_entity`（它是落图前的字段装配，晚于改写抽数）。
    #[must_use]
    pub fn fold_login_player_ctor_draws(&mut self) -> LoginPlayerCtorDraws {
        let d = BaseAnimalCtorDraws::draw(&mut self.rng);
        let random_no = self.rng.random_below(PLAYER_CTOR_RANDOM_NO_MOD);
        let entries = vec![
            CtorFoldEntry {
                site: CtorFoldSite::PlayerBaseRunTick,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: i64::from(BASE_ANIMAL_CTOR_DRAW_MODS[0]),
                result: i64::from(d.run_tick_delta),
            },
            CtorFoldEntry {
                site: CtorFoldSite::PlayerBaseSearchTime,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: i64::from(BASE_ANIMAL_CTOR_DRAW_MODS[1]),
                // 账本记 `Random(2000)` 的**原值**（不含 `+2000`），与启动期账本同列同值。
                result: i64::from(d.base_search_time - BASE_ANIMAL_CTOR_DRAW_MODS[1]),
            },
            CtorFoldEntry {
                site: CtorFoldSite::PlayerAnimalAttackTick,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: i64::from(BASE_ANIMAL_CTOR_DRAW_MODS[2]),
                result: i64::from(d.attack_tick_delta),
            },
            CtorFoldEntry {
                site: CtorFoldSite::PlayerAnimalWalkTick,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: i64::from(BASE_ANIMAL_CTOR_DRAW_MODS[3]),
                result: i64::from(d.walk_tick_delta),
            },
            CtorFoldEntry {
                site: CtorFoldSite::PlayerRandomNo,
                kind: CTOR_FOLD_DRAW_KIND,
                arg: i64::from(PLAYER_CTOR_RANDOM_NO_MOD),
                result: i64::from(random_no),
            },
        ];
        LoginPlayerCtorDraws {
            run_tick_delta: d.run_tick_delta,
            base_search_time: d.base_search_time,
            attack_tick_delta: d.attack_tick_delta,
            walk_tick_delta: d.walk_tick_delta,
            random_no,
            ledger: CtorFoldLedger {
                batch: CtorDrawBatch::Login,
                entries,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    /// 计划 → 抽数次序：**每站点恒 4 抽、次序 = 计划序**（`arg` 列 = 模数表）。
    #[test]
    fn plan_draws_four_per_site_in_plan_order() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_orphan_merchants();
        plan.push_map_check_quests(0);
        plan.push_map_quest_rows(0);
        plan.push_hidden_npc(CtorFoldSite::HiddenQManage);
        assert_eq!(plan.game_app_ctor.len(), 4);
        assert_eq!(plan.load_map_info.len(), 0);
        assert_eq!(plan.load_map_quest.len(), 1);

        let mut world = World::with_maps(vec![(8, 8)]);
        world.rng = RandomNumber::with_seed(20240229);
        let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
        let seg2 = world.fold_ctor_draws_load_map_info(&plan);
        let seg3 = world.fold_ctor_draws_load_map_quest(&plan);

        assert_eq!(seg1.len(), 16, "段① 4 站点 × 4 抽");
        assert!(seg2.is_empty(), "段② 无站点（0 个 CHECKQUEST）");
        assert_eq!(seg3.len(), 4, "段③ 1 站点 × 4 抽");
        let sites: Vec<CtorFoldSite> = seg1
            .by_site()
            .into_iter()
            .map(|(s, _)| s)
            .chain(seg3.by_site().into_iter().map(|(s, _)| s))
            .collect();
        assert_eq!(
            sites,
            vec![
                CtorFoldSite::RobotObject,
                CtorFoldSite::OrphanManageNpc,
                CtorFoldSite::OrphanRobotNpc,
                CtorFoldSite::OrphanFunctionNpc,
                CtorFoldSite::HiddenQManage,
            ]
        );
        // `arg` 列恒为模数表（`1500/2000/3000/3000`，次序固定）。
        for c in seg1.entries.as_chunks::<4>().0 {
            assert_eq!(
                c.iter().map(|e| e.arg).collect::<Vec<_>>(),
                vec![1500, 2000, 3000, 3000]
            );
            assert!(c.iter().all(|e| e.kind == CTOR_FOLD_DRAW_KIND));
        }
    }

    /// **四段 → 抽数与共享流位置**：段①→屏障 A→段②→段③ 合起来的 `result` 列**逐项等于**独立流
    /// 按同一 `arg` 序列抽出的原值（屏障 A 的 `arg = 10000`，恰夹在段①与段②之间）。
    #[test]
    fn ledger_results_match_independent_stream_across_three_segments() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(2);
        plan.push_orphan_merchants();
        plan.push_barrier_a(); // ★屏障 A：`ServerConf.cs:114` 的 `Random(10000)`（1 抽）
        plan.push_map_check_quests(1);
        plan.push_map_quest_rows(3);
        plan.push_hidden_npc(CtorFoldSite::HiddenQManage);
        plan.push_hidden_npc(CtorFoldSite::HiddenQFunction);
        plan.push_hidden_npc(CtorFoldSite::HiddenRobotManage);

        let mut world = World::with_maps(vec![(8, 8)]);
        world.rng = RandomNumber::with_seed(20240229);
        let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
        let barrier_a = world.fold_ctor_draw_item_number_barrier_a(&plan);
        let seg2 = world.fold_ctor_draws_load_map_info(&plan);
        let seg3 = world.fold_ctor_draws_load_map_quest(&plan);

        // 独立流按**同一位置**重建期望：段①（`game_app_ctor` 站点 × 4）→ 屏障 A（1）→ 段②+段③。
        let mut exp = RandomNumber::with_seed(20240229);
        let seg1_draws = plan.game_app_ctor.len() * CTOR_FOLD_DRAWS_PER_SITE;
        let mut expected: Vec<i64> = BASE_ANIMAL_CTOR_DRAW_MODS
            .iter()
            .cycle()
            .take(seg1_draws)
            .map(|m| i64::from(exp.random_below(*m)))
            .collect();
        let expected_barrier_a = i64::from(exp.random_below(CTOR_FOLD_BARRIER_A_MOD as i32));
        expected.push(expected_barrier_a);
        expected.extend(
            BASE_ANIMAL_CTOR_DRAW_MODS
                .iter()
                .cycle()
                .take(plan.ctor_chain_draws() - seg1_draws)
                .map(|m| i64::from(exp.random_below(*m))),
        );

        let got: Vec<i64> = seg1
            .entries
            .iter()
            .chain(barrier_a.entries.iter())
            .chain(seg2.entries.iter())
            .chain(seg3.entries.iter())
            .map(|e| e.result)
            .collect();
        assert_eq!(
            got, expected,
            "四段账本 result 列 = 独立流同 arg 序列（段①→屏障 A→段②→段③）"
        );
        assert_eq!(
            barrier_a.entries[0].result, expected_barrier_a,
            "屏障 A = 流上段①之后的第 {seg1_draws} 条（0-based）"
        );
        assert_eq!(
            plan.total_draws(),
            plan.ctor_chain_draws() + 1,
            "总抽数 = ctor 链 + 屏障 A 1 抽"
        );
        assert_eq!(
            world.rng.random(),
            exp.random(),
            "四段共推进 ctor 链 + 屏障 A 1 抽"
        );
        assert_eq!(CTOR_FOLD_BARRIER_A_DRAWS, 1, "屏障 A 是 1 抽（ServerConf）");
    }

    /// **改坏必红（次序）**：把 G1–G3 提到 RobotObject 之前 ⇒ `by_site()` 的站点序必变
    /// （`result` 列也随之错位 ⇒ 期望表对不上）。
    #[test]
    fn swapped_order_changes_site_sequence() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_orphan_merchants();
        let mut swapped = CtorFoldPlan::new();
        swapped.push_orphan_merchants(); // ← 次序调换
        swapped.push_robot_objects(1);

        let run = |p: &CtorFoldPlan| {
            let mut w = World::with_maps(vec![(8, 8)]);
            w.rng = RandomNumber::with_seed(7);
            w.fold_ctor_draws_game_app_ctor(p)
                .by_site()
                .into_iter()
                .map(|(s, _)| s)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            run(&plan),
            vec![
                CtorFoldSite::RobotObject,
                CtorFoldSite::OrphanManageNpc,
                CtorFoldSite::OrphanRobotNpc,
                CtorFoldSite::OrphanFunctionNpc
            ]
        );
        assert_ne!(run(&plan), run(&swapped), "调换次序必须被站点序咬住");
    }

    /// **改坏必红（少一处 4 抽）**：站点数 -1 ⇒ 抽数 -4、其后所有 `result` 错位。
    #[test]
    fn dropping_one_site_shifts_everything_after() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_orphan_merchants();
        let mut minus = CtorFoldPlan::new();
        minus.push_robot_objects(1);
        // ← 少 push 三只孤儿商人

        assert_eq!(plan.total_draws(), 16);
        assert_eq!(minus.total_draws(), 4);

        let run = |p: &CtorFoldPlan| {
            let mut w = World::with_maps(vec![(8, 8)]);
            w.rng = RandomNumber::with_seed(11);
            let mut v: Vec<i64> = w
                .fold_ctor_draws_game_app_ctor(p)
                .entries
                .iter()
                .map(|e| e.result)
                .collect();
            // 其后接一次「下一个消费者」的抽数（真机账本里就是 `mon_gen`/L4 段）。
            v.push(i64::from(w.rng.random_below(1500)));
            v
        };
        assert_ne!(run(&plan), run(&minus), "少一处 4 抽 ⇒ 其后全部错位");
    }

    /// **屏障 A 挪位必红**：把屏障 A 的 1 抽从段①之后挪到段①之前 ⇒ 段① 与屏障 A 的 `result`
    /// 都变（⇒ 挪位置必被账本咬住）。
    #[test]
    fn barrier_a_position_matters() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_barrier_a();

        // 正确位置：段①（4 抽）→ 屏障 A（1 抽）→ 段②…
        let mut w1 = World::with_maps(vec![(8, 8)]);
        w1.rng = RandomNumber::with_seed(99);
        let seg1 = w1.fold_ctor_draws_game_app_ctor(&plan);
        let barrier_after = w1.fold_ctor_draw_item_number_barrier_a(&plan);

        // 改坏位置：屏障 A 抽在段①之前。
        let mut w2 = World::with_maps(vec![(8, 8)]);
        w2.rng = RandomNumber::with_seed(99);
        let barrier_before = w2.fold_ctor_draw_item_number_barrier_a(&plan);
        let seg1_bad = w2.fold_ctor_draws_game_app_ctor(&plan);

        // 同一独立流、屏障 A 在两个不同位置 ⇒ 段① 的 `result` 列必错位（且屏障自身的值也变）。
        let a: Vec<i64> = seg1.entries.iter().map(|e| e.result).collect();
        let b: Vec<i64> = seg1_bad.entries.iter().map(|e| e.result).collect();
        assert_ne!(a, b, "屏障 A 位置挪动 ⇒ 段① result 列错位");
        assert_ne!(
            barrier_after.entries[0].result, barrier_before.entries[0].result,
            "屏障 A 的值随位置变 ⇒ 位置可判"
        );
    }

    /// **屏障 A 的落账形态**（卡 `openmir2-ctor-barrier-a-draw` 判据 ①）：计划里**恒 1 站点** ⇒
    /// 恰 **1 抽**、`arg = 10000`、`kind = Next(max)`、站点 = [`CtorFoldSite::BarrierAItemNumber`]
    /// （锚 = `ServerConf.LoadConfig@ServerConf.cs:114`）；计划里**没有**该站点 ⇒ **不抽**。
    #[test]
    fn barrier_a_is_exactly_one_draw_with_arg_10000() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_barrier_a();

        let mut w = World::with_maps(vec![(8, 8)]);
        w.rng = RandomNumber::with_seed(4242);
        assert!(
            w.fold_ctor_draw_item_number_barrier_a(&CtorFoldPlan::new())
                .is_empty(),
            "计划里没有屏障 A 站点 ⇒ 不抽（宿主侧 `item_number_counter_init` 会 fail-closed）"
        );

        let ledger = w.fold_ctor_draw_item_number_barrier_a(&plan);
        assert_eq!(ledger.len(), 1, "屏障 A 恰 1 抽（**不是** ctor 链的 4 抽）");
        let e = ledger.entries[0];
        assert_eq!(e.site, CtorFoldSite::BarrierAItemNumber);
        assert_eq!(
            e.kind, CTOR_FOLD_DRAW_KIND,
            "`RandomNumber.Random` ⇒ `Next(max)`"
        );
        assert_eq!(e.arg, CTOR_FOLD_BARRIER_A_MOD);
        assert_eq!(
            e.arg, 10_000,
            "oracle `ServerConf.cs:114` 的 `Random(10000)`"
        );
        assert!(
            (0..CTOR_FOLD_BARRIER_A_MOD).contains(&e.result),
            "result 落在 [0,10000)：{}",
            e.result
        );
        assert_eq!(
            CtorFoldSite::BarrierAItemNumber.oracle_anchor(),
            "ServerConf.LoadConfig@ServerConf.cs:114"
        );
        assert_eq!(CTOR_FOLD_BARRIER_A_DRAWS, 1, "屏障 A 是 1 抽");
        // 屏障 A 只在 `barrier_a` 段 ⇒ 不进 ctor 链计数。
        assert_eq!(plan.barrier_a, vec![CtorFoldSite::BarrierAItemNumber]);
        assert_eq!(plan.ctor_chain_draws(), 4);
        assert_eq!(plan.total_draws(), 5);
    }

    /// **改坏必红（漏抽屏障 A）**：计划里不 push 屏障 A ⇒ 其后的抽数（段② 首抽 = `Random(1500)`）
    /// 与 oracle 位置差 1（同种子下两值不同）⇒ 账本自第 17 条起分歧（部署件段① = 16 抽）。
    #[test]
    fn dropping_barrier_a_shifts_the_following_draws() {
        let mut with = CtorFoldPlan::new();
        with.push_robot_objects(1);
        with.push_orphan_merchants();
        with.push_barrier_a();
        let mut without = with.clone();
        without.barrier_a.clear();

        let run = |p: &CtorFoldPlan| {
            let mut w = World::with_maps(vec![(8, 8)]);
            w.rng = RandomNumber::with_seed(20240229);
            let seg1 = w.fold_ctor_draws_game_app_ctor(p);
            let barrier = w.fold_ctor_draw_item_number_barrier_a(p);
            let seg2_first = w.rng.random_below(1500); // 段② 的首抽（`BaseObject..ctor`）
            (seg1.len(), barrier.len(), seg2_first)
        };
        let (n1, b1, first_with) = run(&with);
        let (n2, b2, first_without) = run(&without);
        assert_eq!((n1, n2), (16, 16), "段① 不受影响（4 站点 × 4 抽）");
        assert_eq!((b1, b2), (1, 0), "屏障 A 的条数随计划变");
        assert_ne!(
            first_with, first_without,
            "漏抽屏障 A ⇒ 其后的抽数整体错位（段① 16 抽之后的第 17 条起分歧）"
        );
    }

    /// **行数 × 4 而非常量**：`Robot.txt` 1 行 → 2 行 ⇒ 头部 4 抽变 8 抽。
    #[test]
    fn robot_line_count_is_multiplied_by_four() {
        let mut p1 = CtorFoldPlan::new();
        p1.push_robot_objects(1);
        let mut p2 = CtorFoldPlan::new();
        p2.push_robot_objects(2);
        assert_eq!(p1.total_draws(), 4);
        assert_eq!(p2.total_draws(), 8, "行数 × 4，不是常量 4");
    }

    /// **登录批恰 5 抽、次序 = oracle 继承链**（卡 `openmir2-player-ctor-rng-order` A4）：
    /// ① `BaseObject` `1500` → ② `2000` → ③ `AnimalObject` `3000` → ④ `3000` →
    /// ⑤ `PlayObject` `999999`。缺任一抽/换序/错模数 ⇒ 本用例必红。
    #[test]
    fn login_player_ctor_draws_follow_oracle_order_and_count() {
        let mut w = World::with_maps(vec![(8, 8)]);
        w.rng = RandomNumber::with_seed(1234);
        let d = w.fold_login_player_ctor_draws();

        assert_eq!(d.ledger.batch, CtorDrawBatch::Login);
        assert_eq!(d.ledger.len(), 5, "登录 PlayObject 构造恒 5 抽");
        let sites: Vec<CtorFoldSite> = d.ledger.entries.iter().map(|e| e.site).collect();
        assert_eq!(
            sites,
            vec![
                CtorFoldSite::PlayerBaseRunTick,
                CtorFoldSite::PlayerBaseSearchTime,
                CtorFoldSite::PlayerAnimalAttackTick,
                CtorFoldSite::PlayerAnimalWalkTick,
                CtorFoldSite::PlayerRandomNo,
            ],
            "次序 = BaseObject(2) → AnimalObject(2) → PlayObject.RandomNo"
        );
        let args: Vec<i64> = d.ledger.entries.iter().map(|e| e.arg).collect();
        assert_eq!(args, vec![1500, 2000, 3000, 3000, 999_999]);
        assert!(
            d.ledger
                .entries
                .iter()
                .all(|e| e.kind == CTOR_FOLD_DRAW_KIND),
            "kind 必须与 rng_trace 同列 `Next(max)`"
        );

        // 结果逐项 = 同种子参考流按同序 5 抽。
        let mut refr = RandomNumber::with_seed(1234);
        let rd = BaseAnimalCtorDraws::draw(&mut refr);
        let expected = vec![
            i64::from(rd.run_tick_delta),
            i64::from(rd.base_search_time - BASE_ANIMAL_CTOR_DRAW_MODS[1]),
            i64::from(rd.attack_tick_delta),
            i64::from(rd.walk_tick_delta),
            i64::from(refr.random_below(PLAYER_CTOR_RANDOM_NO_MOD)),
        ];
        let got: Vec<i64> = d.ledger.entries.iter().map(|e| e.result).collect();
        assert_eq!(got, expected);
        assert_eq!(i64::from(d.random_no), expected[4]);
        // 流位恰好前移 5（第 6 个值两侧一致）。
        assert_eq!(w.rng.random_below(1_000_000), refr.random_below(1_000_000));
    }

    /// **缺第 5 抽（`RandomNo`）⇒ 其后共享流错位**（A4 负例）。
    #[test]
    fn login_missing_fifth_draw_shifts_stream() {
        let mut with5 = World::with_maps(vec![(8, 8)]);
        with5.rng = RandomNumber::with_seed(77);
        let _ = with5.fold_login_player_ctor_draws();
        let after5 = with5.rng.random_below(1_000_000);

        let mut only4 = World::with_maps(vec![(8, 8)]);
        only4.rng = RandomNumber::with_seed(77);
        let _ = BaseAnimalCtorDraws::draw(&mut only4.rng);
        let after4 = only4.rng.random_below(1_000_000);
        assert_ne!(after5, after4, "缺第 5 抽 ⇒ 其后所有抽数错位");
    }

    /// **startup / login 两批显式分桶、分别计数**（A3）：同一 `site` 文件行不能区分批次
    /// （前 4 抽的 trace site 相同）⇒ 靠 `batch` 字段；两批在同一条 `world.rng` 上首尾相接。
    #[test]
    fn startup_and_login_batches_are_separate() {
        let mut w = World::with_maps(vec![(8, 8)]);
        w.rng = RandomNumber::with_seed(9);
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);
        plan.push_orphan_merchants();
        let startup = w.fold_ctor_draws_game_app_ctor(&plan);
        assert_eq!(startup.batch, CtorDrawBatch::Startup);
        assert_eq!(startup.len(), 16, "段① = 4 站点 × 4 抽 = 16 条");

        let login1 = w.fold_login_player_ctor_draws();
        let login2 = w.fold_login_player_ctor_draws();
        assert_eq!(login1.ledger.batch, CtorDrawBatch::Login);
        assert_eq!(login2.ledger.batch, CtorDrawBatch::Login);
        assert_eq!(login1.ledger.len(), 5);
        assert_eq!(login2.ledger.len(), 5);

        // 同种子回放：`draw` 每次 = 1 站点（4 抽）⇒ 段① 4 次 draw，随后 login1/login2 各 4+1 抽。
        let sites = plan.game_app_ctor.len();
        assert_eq!(sites, 4);
        let mut refr = RandomNumber::with_seed(9);
        for _ in 0..sites {
            let _ = BaseAnimalCtorDraws::draw(&mut refr);
        }
        let rd1 = BaseAnimalCtorDraws::draw(&mut refr);
        assert_eq!(login1.run_tick_delta, rd1.run_tick_delta);
        let _ = refr.random_below(PLAYER_CTOR_RANDOM_NO_MOD); // login1 第 5 抽
        let rd2 = BaseAnimalCtorDraws::draw(&mut refr);
        assert_eq!(login2.run_tick_delta, rd2.run_tick_delta);
    }
}
