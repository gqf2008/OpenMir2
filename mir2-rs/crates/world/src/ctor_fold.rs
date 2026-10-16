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
//! | ★屏障 A（**未建模**） | `AppService.cs:87` → `GameShare.cs:90` → `ServerConf.cs:114` | `SystemShare.Config.ItemNumber += Random(10000)` | **1** |
//! | ② `GameApp.Initialize` | `GameApp.cs:124` → `Maps.cs:137` → `:454` | `MapInfo` 每个 `CHECKQUEST(x)` 行一个 `new Merchant` | 4×行数 |
//! | ★屏障 B（已建模） | `GameApp.cs:155` `WorldEngine.InitializeMonster()` | 宿主 `mon_gen`（`WorldServer.MonGen.cs:66/94` 的 `Random(limit)`） | — |
//! | ③ `GameApp.Initialize` | `GameApp.cs:184` → `MapQuestManager.cs:40` | `MapQuest.txt` **每个有效行**一个 `new Merchant` | 4×有效行数 |
//! | ③ `GameApp.Initialize` | `GameApp.cs:184` 尾 → `LocalDB.cs:403/404/405` → `:268/:210/:312` | 三只隐藏 `new Merchant`（`QManage`→`Function`→`RobotManage`） | 4×存在数 |
//! | ★屏障 C（已建模） | `GameApp.cs:227`/`:229` | `LoadMerchant`/`LoadNpcs`（见 `merchant.rs`/`npc.rs`） | 4×条 |
//!
//! 每处 `new Merchant`/`new RobotObject` 都走同一条 ctor 公共链
//! （`BaseObject` `:439/441` + `AnimalObject` `:76/77`）⇒ **恒 4 抽**，唯一抽数点仍是
//! [`BaseAnimalCtorDraws::draw`]（本模块不新增抽数点、不重复模数表）。实体缺口（`RobotObject`/
//! 任务 NPC/隐藏 NPC 的世界实体）**另立条目**，不在本片。
//!
//! ## 三条阶段硬约束（次序即位置）
//!
//! fold 不是「一共补 N 抽」就够——**位置**决定账本能否逐项对齐。本模块按 oracle 的三条**屏障**
//! 切成三段，宿主必须按 [`World::fold_ctor_draws_game_app_ctor`] →
//! [`World::fold_ctor_draws_load_map_info`] → [`World::fold_ctor_draws_load_map_quest`] 的次序
//! 调用，且中间两处**别的东西**的抽数不许挪：
//!
//! - **屏障 A**（`AppService.cs:87` `GameShare.LoadConfig()` → `ServerConf.cs:114` 的
//!   `Random(10000)`，**1 抽，本片未建模**）：它在 `GameApp` ctor **之后**、
//!   `GameApp.Initialize`（⇒ M2 `:124`）**之前**——`new GameApp(...)` 由 DI 在
//!   `AppService` 构造时发生，`StartingAsync` 里才 `LoadConfig()`。⇒ 段①与段②之间**必须**留出
//!   这 1 抽的位置（本片留空 + 登记，见下「仍阻塞」）。
//! - **屏障 B**（`GameApp.cs:155` `InitializeMonster`）：段②与段③之间的抽数属宿主 `mon_gen`，
//!   **不许**把段③提到它前面（否则 M1/L1-L3 与 `mon_gen` 互换位置、整本账本错位）。
//! - **屏障 C**（`GameApp.cs:227`/`:229`）：段③**必须早于** `LoadMerchant`（L4）/`LoadNpcs`（L5）
//!   （oracle `:184` < `:227`/`:229`）。
//!
//! ## 登记（本片未做 / 不可达 / 仍阻塞）
//!
//! - **L6 `LocalDB.cs:1067`（`ReLoadMerchants`）= N/A（不可达）**：唯一调用者是
//!   `ReloadNpcCommand.Execute`（`src/Modules/GameCommand/Commands/ReloadNpcCommand.cs:14`），
//!   而该 `Execute` **体整段被注释**（`:24` 起的 `M2Share.LocalDb.ReLoadMerchants();` 在 `//` 内）
//!   ⇒ 启动期与当前发布命令集都到不了 ⇒ 构造段**抽 0**、不 fold。
//! - **X1 `MobNpcCommand.cs:36` = N/A**：`//var merchant = new Merchant();` 源码已注释。
//! - **实体缺口（本片只 fold 抽数、不建实体）**：`RobotObject` 世界实体 + `AutoRun` 驱动器；
//!   M1/M2 任务 NPC 进 `QuestNpcList`；L1 `QFunction` 进 `MerchantList`（`AddMerchant`）；
//!   L2/L3 进 `QuestNpcList` + `ActorId` 按序分配 —— 均另立条目，不在本片。
//! - **仍阻塞「真机账本从 `seq=0` 逐项对齐」的未核/未做项**：
//!   ① `GameApp.cs:55-58` 与本片同段的 `GameCommandSystem`/`AuthenticationService`/`ScriptEngine`
//!   **已核实读不抽**（`CommandSystem.cs:19-22`/`AuthenticationService.cs:15-20`/
//!   `ScriptEngine.cs:30-36`，见 `bins/game-svr/src/ctor_fold.rs` 模块头）；
//!   ② `GameApp.cs:88-93` 的 `ItemSystem`/`GuildMgr`/`CastleMgr`/`MapMgr`/`EventMgr`/`WorldServer`
//!   六个 ctor **已核实读不抽**（同上；这与 §4.4.2 的「须一并核」清单逐条对上）；
//!   ③ **未做（屏障 A）**：`ServerConf.cs:114` 的 `Random(10000)`（1 抽）——Rust 侧
//!   `bins/game-svr/src/config.rs` 读同一键但**不抽**。⇒ 真机账本自**第 13 条**
//!   （段① 12 抽之后）起即分歧；本片**只留位置、不复刻该抽** ⇒ 段②/段③ 的**绝对**位置仍
//!   差 1（但相对次序已与 oracle 一致，补上屏障 A 即整段对齐）。
//!   **⇒ 账本对齐在本片之后仍受此未做项阻塞（须另立条目）。**
//!   ④ 运行期路径（`M2Share.cs:279/:526`、`ScriptEngine.cs:156`）不在启动构造段内，不计。
//!
//! ## 与 `rng_trace` 的关系
//!
//! 本模块的抽数走共享 `world.rng` ⇒ `MIR2_RNG_LOG` 流水里每处仍是
//! `crates/world/src/entity.rs:106-110`（`construct` 组，4 行/处）。[`CtorFoldLedger`] 是**同一批
//! 抽数的符号化投影**（site = oracle 锚），供构造级用例把「次序」也钉死——真机流水的 site 列
//! 两侧都区分不出这些同形构造点（oracle 侧同为 `BaseObject..ctor`/`AnimalObject..ctor`），
//! 只有本账本的 site 列能咬住次序错位。

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
        }
    }
}

/// 每处构造点的抽数（`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`，恒 4）。
pub const CTOR_FOLD_DRAWS_PER_SITE: usize = 4;

/// fold 抽数的 `kind` 列（与 `crates/shared/src/rng.rs::random_below` 的
/// `record("Next(max)", value, …)` 同口径；`bins/game-svr/tests/ctor_fold_rng_trace.rs` 与真流水互核）。
pub const CTOR_FOLD_DRAW_KIND: &str = "Next(max)";

/// **屏障 A**：oracle `AppService.cs:87` → `GameShare.cs:90` → `ServerConf.cs:114` 的
/// `Random(10000)`（`SystemShare.Config.ItemNumber += …`）——**1 抽，本片未建模**（登记见模块头）。
///
/// 它在段①（`GameApp` ctor）之后、段②（`:124` `LoadMapInfo`）之前 ⇒ 段①与段②之间要**留出这 1 抽**。
pub const CTOR_FOLD_BARRIER_A_DRAWS: usize = 1;

/// **构造段 fold 计划**：按 oracle 次序排好的站点表，按三条屏障切成三段（见模块头）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtorFoldPlan {
    /// 段①（`GameApp` ctor）：`GameApp.cs:58`（RobotObject）+ `:85-87`（G1–G3）。
    /// **调用时机**：早于屏障 A（`ServerConf.cs:114` 的 1 抽）。
    pub game_app_ctor: Vec<CtorFoldSite>,
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

    /// 计划总抽数（= 站点数 × 4；**不含**屏障 A 的 1 抽——那是未建模项）。
    #[must_use]
    pub fn total_draws(&self) -> usize {
        (self.game_app_ctor.len() + self.load_map_info.len() + self.load_map_quest.len())
            * CTOR_FOLD_DRAWS_PER_SITE
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

/// 构造段 fold 账本（按抽数次序）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CtorFoldLedger {
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

    /// 逐站点折叠成 `(site, 该处 4 抽的原值)` 的视图（判据用例的期望表按此写）。
    #[must_use]
    pub fn by_site(&self) -> Vec<(CtorFoldSite, [i64; CTOR_FOLD_DRAWS_PER_SITE])> {
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

    /// **三段 → 抽数与共享流位置**：三段合起来的 `result` 列**逐项等于**独立流按同一 `arg` 序列
    /// 抽出的原值；且三段之间**留出屏障 A 的 1 抽位置**（屏障 A 挪一抽即其后全部错位）。
    #[test]
    fn ledger_results_match_independent_stream_across_three_segments() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(2);
        plan.push_orphan_merchants();
        plan.push_map_check_quests(1);
        plan.push_map_quest_rows(3);
        plan.push_hidden_npc(CtorFoldSite::HiddenQManage);
        plan.push_hidden_npc(CtorFoldSite::HiddenQFunction);
        plan.push_hidden_npc(CtorFoldSite::HiddenRobotManage);

        let mut world = World::with_maps(vec![(8, 8)]);
        world.rng = RandomNumber::with_seed(20240229);
        let seg1 = world.fold_ctor_draws_game_app_ctor(&plan);
        // ★屏障 A 槽位：oracle `ServerConf.cs:114` 的 `Random(10000)`（未建模）——
        // 本用例按 oracle 位置把它**跳过去**（不抽），只验证「段②在段①之后、段③在段②之后」。
        let seg2 = world.fold_ctor_draws_load_map_info(&plan);
        let seg3 = world.fold_ctor_draws_load_map_quest(&plan);

        let mut exp = RandomNumber::with_seed(20240229);
        let expected: Vec<i64> = BASE_ANIMAL_CTOR_DRAW_MODS
            .iter()
            .cycle()
            .take(plan.total_draws())
            .map(|m| i64::from(exp.random_below(*m)))
            .collect();
        let got: Vec<i64> = seg1
            .entries
            .iter()
            .chain(seg2.entries.iter())
            .chain(seg3.entries.iter())
            .map(|e| e.result)
            .collect();
        assert_eq!(got, expected, "三段账本 result 列 = 独立流同 arg 序列");
        // `World::fold_*` 不碰屏障 A ⇒ 屏障 A 由宿主在段①之后自己抽（本片留空）。
        assert_eq!(
            world.rng.random(),
            exp.random(),
            "三段共推进 4×站点数 个抽数（屏障 A 的 1 抽由宿主负责）"
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

    /// **屏障 A 挪位必红**：把屏障 A 的 1 抽从段①之后挪到段①之前 ⇒ 段①的 `result` 列整体错位。
    #[test]
    fn barrier_a_position_matters() {
        let mut plan = CtorFoldPlan::new();
        plan.push_robot_objects(1);

        // 正确位置：段①（4 抽）→ 屏障 A（1 抽）→ 段②…
        let mut w1 = World::with_maps(vec![(8, 8)]);
        w1.rng = RandomNumber::with_seed(99);
        let seg1 = w1.fold_ctor_draws_game_app_ctor(&plan);
        let barrier_after = w1.rng.random_below(10_000);

        // 改坏位置：屏障 A 抽在段①之前。
        let mut w2 = World::with_maps(vec![(8, 8)]);
        w2.rng = RandomNumber::with_seed(99);
        let barrier_before = w2.rng.random_below(10_000);
        let seg1_bad = w2.fold_ctor_draws_game_app_ctor(&plan);

        // 同一独立流、屏障 A 在两个不同位置 ⇒ 段① 的 `result` 列必错位（且屏障自身的值也变）。
        let a: Vec<i64> = seg1.entries.iter().map(|e| e.result).collect();
        let b: Vec<i64> = seg1_bad.entries.iter().map(|e| e.result).collect();
        assert_ne!(a, b, "屏障 A 位置挪动 ⇒ 段① result 列错位");
        assert_ne!(
            barrier_after, barrier_before,
            "屏障 A 的值随位置变 ⇒ 位置可判"
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
}
