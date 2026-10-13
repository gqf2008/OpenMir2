//! 商人（`Merchant`）世界实体 + 挂图 `Initialize` + `ProcessMerchants` tick —— M3⑤-实现 2 第①/②片。
//!
//! oracle 锚点（逐条复核于本片）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`MerchantState`] 字段 | `src/GameSrv/Npc/Merchant.cs:10-70`（`IMerchant`）+ `BaseObject` 的 `RunTick/RunTime`（`BaseObject.cs:439-440`） |
//! | [`World::spawn_merchant`] 的 `Initialize`/`AddtoMapSuccess` | `src/M2Server/Actor/BaseObject.Base.cs:14-21` |
//! | 挂图失败移除（`!IsHide` 例外） | `src/GameSvr/Word/WorldServer.Merchant.cs:8-25` |
//! | `FindMap(MapName) == null ⇒ 丢弃` | `WorldServer.Merchant.cs:24-31`（**门在宿主侧按 `MapRegistry::id_of` 施加**，见 [`crate::merchant`] 模块尾与 `bins/game-svr/src/merchant.rs`） |
//! | [`World::merchant_tick`] | `WorldServer.Merchant.cs:65-110` + `MerchantProcessor.cs:8`（200ms 定时） |
//! | [`merchant_run`] | `Merchant.cs:112-170`（RNG/显隐/`MapRandomMove`）+ `BaseObject.cs:1214-1237` |
//! | `refill_goods` 计时态 | `Merchant.cs:209-276`（库存/落库属 ⑤-5） |
//!
//! ## 为什么门在宿主侧
//! `SystemShare.MapMgr.FindMap(MapName)` 的部署态等价物是 `MapRegistry::id_of`（同 `MapInfo` +
//! `Map/*.map` 合并口径，见 `crates/envir`）。`crates/world` 只持有**下标式**地图表、不含逻辑名
//! ⇒ 名字→`map_id` 的解析归宿主；本模块只负责「拿到 `map_id` 后的挂图/落格」。
//!
//! ## 未建模（登记）
//! - `GoodsList`/`ItemPriceList` 的库存与 `SaveGoodRecord` 持久化 —— 接缝归 ⑤-5；
//!   `refill_goods` 因此恒走 `goods.Count > 0` 分支（`GetRefillList` 取不到库存）⇒ 一次 Run
//!   最多补一件（oracle quirk 照搬）。
//! - `ClearExpreUpgradeListData`（`Merchant.cs:1076`）：oracle 用墙钟 `DateTime.Now` + 升级表持久化
//!   （⑤-5）⇒ 本片只推进 `clear_expre_upgrade_tick` 计时态，无可观测效果。
//! - `ProcessRefillIndex`（`ScriptParsers.cs:838`）：⑤-1 未随记录暴露 ⇒ 置 0（补货延时门）。
//! - `NormNpc.Initialize` 的脚本/说词加载（`NormNpc.cs:1676`）—— ⑤-4/⑤-6 接缝；商人 `RM_TURN`/
//!   `RM_HIT`/`RM_DISAPPEAR`/`RM_SPACEMOVE_FIRE` 的**帧面**（`SM_*`）也归接线层（⑤-4）。
//! - `MerchantState.is_hide` 当前恒 `false` 且 [`MerchantSpawn`] 无该入参（reviewer-6 M-3）：
//!   oracle `MerchantInitialize` 在 `LoadMerchantScript` **之前**读 `IsHide`（`WorldServer.Merchant.cs:16`）
//!   ⇒ 此刻确为默认 `false`，当前 1:1；将来若要注入（脚本置位）必须点明消费方。
//! - `MerchantState.castle_under_war` 缺省 `false`（沙巴克攻城层属 ⑧；`Merchant.cs:143` 同「不在战中」支）。

use mir2_data::models::StdItem;
use mir2_shared::rng::RandomNumber;

use crate::entity::Entity;
use crate::map::{CellType, MoveRequest};
use crate::world::World;

/// `ActorRace.Merchant`（`src/OpenMir2/Enums/Race.cs`：`Merchant = 50`，与 `Animal` 同值）。
///
/// ⚠ 与 `Race == ActorRace.Play` 一样是**broadcast 口径的枚举值**；`crates/world` 的怪物 AI
/// （[`crate::monai::monster_ai_tick`]）按 [`Entity::merchant`] 是否存在显式排除商人实体，
/// **不**依赖 race 数值（race 50 与 Animal 同值，靠 race 会误跑 `monster_run`）。
pub const ACTOR_RACE_MERCHANT: u8 = 50;

/// `Merchant.Run` 的 `RunTime`（`BaseObject.cs:440`：`RunTime = 250`）。
pub const MERCHANT_RUN_TIME: i64 = 250;

/// `MerchantProcessor` 的 200ms 定时器（`MerchantProcessor.cs:8`
/// `TimerScheduledService(TimeSpan.FromMilliseconds(200))`）。
///
/// **本片口径（owner `eb3bf42f` 裁决）**：用**世界 sim 时钟**的整数毫秒替代墙钟定时器
/// ——每 200ms 跑一趟 `ProcessMerchants`（`World::tick` 步长恰为 200ms ⇒ 每 tick 一趟），
/// 同一 200ms 窗口内的第二次调用是 no-op（节奏门）。
pub const MERCHANT_PASS_INTERVAL_MS: i64 = 200;

/// `ProcessMerchants` 的 Ghost 过期门：`(GetTickCount() - GhostTick) > 60 * 1000`
/// （`WorldServer.Merchant.cs:84-91`）。用 sim 时钟整数毫秒（不是墙钟）。
pub const MERCHANT_GHOST_EXPIRE_MS: i64 = 60 * 1000;

/// `BaseObject` ctor 的 `RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`）。
pub const MERCHANT_RUN_DELAY_MAX: i32 = 1500;

/// `MerchantState::run_tick` 的「未抽」哨兵：构造抽需按 oracle 次序在**独立一步**落定
/// （`LocalDB.LoadMerchant` 正序构造抽 → `MerchantInitialize` 倒序挂图），
/// 故 [`MerchantState::new`] 置该值，[`World::spawn_merchant`] 见它才抽。
pub const MERCHANT_RUN_TICK_UNSET: i64 = i64::MIN;

/// `Merchant.Run` 的补货间隔基数：`5 * 60 * 1000` ms（`Merchant.cs:119`）。
pub const MERCHANT_REFILL_GOODS_INTERVAL_MS: i64 = 5 * 60 * 1000;

/// `Merchant.Run` 的武器升级清理间隔：`10 * 60 * 1000` ms（`Merchant.cs:126`）。
pub const MERCHANT_CLEAR_EXPRE_UPGRADE_INTERVAL_MS: i64 = 10 * 60 * 1000;

/// `Merchant.Run` 的转向门：`Random(50) == 0`（`Merchant.cs:132/138`）。
pub const MERCHANT_TURN_RANDOM_MOD: i32 = 50;

/// `Merchant.Run` 的转向方向：`RandomByte(8)`（`Merchant.cs:134`）。
pub const MERCHANT_TURN_DIR_MAX: u8 = 8;

/// `Merchant.Run` 的补货延时步长：`ProcessRefillIndex * 500` ms（`Merchant.cs:117`）。
pub const MERCHANT_REFILL_DELAY_STEP_MS: i64 = 500;

/// `Merchant` 脚本头 `(...)` 命令位（`ScriptFlagConst`；与 `crates/script::MerchantFlags` 同形）。
///
/// `crates/world` **不依赖** `crates/script`（模拟层不引脚本引擎）⇒ 本片定义同形结构，
/// 由宿主 `bins/game-svr` 在装载时逐字段搬入。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MerchantFlags {
    pub is_buy: bool,
    pub is_sell: bool,
    pub is_make_drug: bool,
    pub is_prices: bool,
    pub is_storage: bool,
    pub is_getback: bool,
    pub is_upgradenow: bool,
    pub is_get_backupgnow: bool,
    pub is_repair: bool,
    pub is_sup_repair: bool,
    pub is_send_msg: bool,
    pub is_use_item_name: bool,
    pub is_offline_msg: bool,
    pub is_yb_deal: bool,
}

/// `[goods]` 区补货条目（`Goods`，`crates/script::Goods` 同形）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MerchantGoods {
    pub item_name: String,
    pub count: i32,
    pub refill_time: i32,
    /// `Goods.RefillTick`（`src/Modules/SystemModule/Data/Goods.cs:8`；脚本解析置 0，
    /// `ScriptParsers.cs:965`）。`Merchant.Run` 的补货门按它逐条计时。
    pub refill_tick: i64,
}

/// 商人的世界侧状态（`IMerchant` 的字段子集 + `BaseObject` 计时器）。
#[derive(Clone, Debug)]
pub struct MerchantState {
    /// `IMerchant.ScriptName`（`Merchant.cs:16`）。
    pub script_name: String,
    /// `IMerchant.MapName`（`BaseObject.MapName`）。
    pub map_name: String,
    /// `IMerchant.NpcFlag`（`NormNpc`）。
    pub npc_flag: i16,
    /// `IMerchant.Appr`（`Merchant.cs` ctor 置 0，装载后覆盖）。
    pub appr: u16,
    /// 脚本头命令位（`IsBuy`…`IsYbDeal`，`Merchant.cs:47-67`）。
    pub flags: MerchantFlags,
    /// `IMerchant.PriceRate`（ctor 默认 100，`Merchant.cs:86`）。
    pub price_rate: i32,
    /// `IMerchant.ItemTypeList`（脚本 `+StdMode`）。
    pub item_type_list: Vec<i32>,
    /// `IMerchant.RefillGoodsList`（脚本 `[goods]`）。
    pub refill_goods: Vec<MerchantGoods>,
    /// `IMerchant.CastleMerchant`（`LocalDB.cs:461-464`）。
    pub castle_merchant: bool,
    /// `IMerchant.BoCanMove`（`LocalDB.cs:465-468`）。
    pub can_move: bool,
    /// `IMerchant.MoveTime`（秒，`LocalDB.cs:451`）。
    pub move_time: i32,
    /// `ProcessRefillIndex`（脚本 `[goods]` 的补货延时序号，`ScriptParsers.cs:838`）。
    pub process_refill_index: i32,
    /// `IsHide`（`NormNpc`；`MerchantInitialize` 的挂图失败移除分支按它例外）。
    pub is_hide: bool,
    /// `RefillGoodsTick`（`Merchant.cs:26`；ctor = `GetTickCount()`）。
    pub refill_goods_tick: i64,
    /// `ClearExpreUpgradeTick`（`Merchant.cs:30`；ctor = `GetTickCount()`）。
    pub clear_expre_upgrade_tick: i64,
    /// `MoveTick`（`Merchant.cs:42`；ctor = `GetTickCount()`）。
    pub move_tick: i64,
    /// `RunTick`（`BaseObject.cs:439`；ctor = `GetTickCount() + Random(1500)`）。
    ///
    /// 缺省 = [`MERCHANT_RUN_TICK_UNSET`]（未抽）；构造抽由 [`World::merchant_ctor_run_delay`]
    /// （宿主**正序**预抽，`LocalDB.LoadMerchant` 口径）或 [`World::spawn_merchant`]（无预抽时）
    /// 从共享 `world.rng` 落定（站点归 `merchant` 组）。
    pub run_tick: i64,
    /// `GhostTick`（`BaseObject`；`ProcessMerchants` 的 60s 过期门）。
    pub ghost_tick: i64,
    /// `Castle.UnderWar`（`Merchant.cs:143` 的 `CastleMerchant && Castle != null && Castle.UnderWar`）。
    ///
    /// 沙巴克攻城层属 ⑧ 家族，本片不建模城堡状态 ⇒ 缺省 `false`（与 oracle 「不在战中」同支）；
    /// 接线层在 ⑧ 落地后按城堡状态覆写。
    pub castle_under_war: bool,
}

impl MerchantState {
    /// 按 oracle ctor 的**非随机**初值构造（`Merchant.cs:70-106`）；随机项（`RunTick` 的
    /// `Random(1500)`）留作 [`MERCHANT_RUN_TICK_UNSET`]，由调用方按 oracle 次序落定
    /// （[`World::merchant_ctor_run_delay`] 正序，或 [`World::spawn_merchant`] 兜底）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        script_name: String,
        map_name: String,
        npc_flag: i16,
        appr: u16,
        price_rate: i32,
        castle_merchant: bool,
        can_move: bool,
        move_time: i32,
        flags: MerchantFlags,
        item_type_list: Vec<i32>,
        refill_goods: Vec<MerchantGoods>,
        process_refill_index: i32,
    ) -> Self {
        MerchantState {
            script_name,
            map_name,
            npc_flag,
            appr,
            flags,
            price_rate,
            item_type_list,
            refill_goods,
            castle_merchant,
            can_move,
            move_time,
            process_refill_index,
            is_hide: false,
            refill_goods_tick: 0,
            clear_expre_upgrade_tick: 0,
            move_tick: 0,
            run_tick: MERCHANT_RUN_TICK_UNSET,
            ghost_tick: 0,
            castle_under_war: false,
        }
    }
}

/// 一条待挂图的商人（宿主从 `MerchantRecord` + `map_id` 装配后的入参）。
#[derive(Clone, Debug)]
pub struct MerchantSpawn {
    /// `ChrName`。
    pub name: String,
    /// `CurrX`。
    pub x: i32,
    /// `CurrY`。
    pub y: i32,
    /// `FindMap(MapName)` 命中的地图下标（宿主已过门）。
    pub map_id: usize,
    pub state: MerchantState,
}

impl World {
    /// 把一条商人落到世界并执行 oracle `Initialize()` 的挂图段：
    /// `AddtoMapSuccess = !(CanWalk(CurrX,CurrY,true) && AddToMap())`
    /// （`BaseObject.Base.cs:14-21`）；为真且 `!IsHide` ⇒ 从列表移除（`WorldServer.Merchant.cs:16-22`）。
    ///
    /// 返回 `Some(id)` = 商人留在世界；`None` = 挂图失败被移除（oracle `RemoveAt`）。
    ///
    /// **构造抽**：按 oracle `BaseObject` ctor 的 `RunTick = now + Random(1500)`
    /// （`BaseObject.cs:439`）从**共享 `world.rng`** 取一次数（站点归 `merchant` 组）——
    /// **仅当** [`MerchantState::run_tick`] 仍为 [`MERCHANT_RUN_TICK_UNSET`]（宿主已用
    /// [`World::merchant_ctor_run_delay`] 按 `LocalDB.LoadMerchant` 正序预抽时不再重复抽）。
    /// `refill_goods_tick`/`clear_expre_upgrade_tick`/`move_tick` 置为本刻 `clock_ms`（oracle ctor
    /// 的 `GetTickCount()` 同口径；`Goods.RefillTick` 由脚本解析置 0，见 [`MerchantGoods`]）。
    pub fn spawn_merchant(&mut self, spawn: MerchantSpawn) -> Option<i32> {
        if spawn.map_id >= self.maps.len() {
            return None;
        }
        let now = self.clock_ms;
        let map_id = spawn.map_id;
        let (x, y) = (spawn.x, spawn.y);
        let mut state = spawn.state;
        if state.run_tick == MERCHANT_RUN_TICK_UNSET {
            state.run_tick = now + i64::from(self.rng.random_below(MERCHANT_RUN_DELAY_MAX));
        }
        state.refill_goods_tick = now;
        state.clear_expre_upgrade_tick = now;
        state.move_tick = now;
        state.ghost_tick = 0;
        let is_hide = state.is_hide;

        let mut e = Entity::new(0, spawn.name, x, y);
        // `Entity::new(0, …)` 的占位 id 必须换成**唯一** id（照 `World::add_entity` 的口径）：
        // 否则所有商人 `id == 0`、`EntityStore::insert` 的 `index.insert(0, …)` 互相覆盖，
        // `merchant_ids_sorted` 返回一串 0、`merchant_tick` 会反复 Run 同一个实体。
        e.id = self.entities.alloc_id();
        e.race = ACTOR_RACE_MERCHANT;
        e.map_id = map_id;
        e.merchant = Some(state);
        let id = self.entities.insert(e);

        // `Envir.CanWalk(CurrX, CurrY, true)`：`true` = 忽略活体阻挡，只看格子是否可走。
        // `MapGrid::cell_valid` 对越界返回 `true`（复刻 C# `CellValid` 方向），故这里必须
        // 先 `cell_match` 判界（越界 ⇒ CanWalk 失败 ⇒ `AddtoMapSuccess`）。
        let (can_walk, added) = {
            let map = &mut self.maps[map_id];
            let can_walk = map.cell_match(x, y) && map.cell_valid(x, y);
            let added = map.add_map_object(x, y, CellType::Merchant, id, now);
            (can_walk, added)
        };
        let add_to_map_success = !(can_walk && added);
        if add_to_map_success && !is_hide {
            // oracle `LogService.Warn("Merchant Initalize fail...") + RemoveAt(i)`
            self.entities.remove(id);
            return None;
        }
        if let Some(ent) = self.entities.get_mut(id) {
            ent.add_to_mapped = !add_to_map_success;
        }
        Some(id)
    }

    /// 按**槽位顺序**列出所有商人实体 id（`MerchantList` 的确定性等价物；供 ⑤-4 取列表）。
    ///
    /// 返回的 id 由 [`World::spawn_merchant`] 用 `EntityStore::alloc_id` 分配 ⇒ **唯一**
    /// （`merchant_tick` 直接以它为键；`merchant_ids_sorted` 不再返回重复占位 id）。
    #[must_use]
    pub fn merchant_ids_sorted(&self) -> Vec<i32> {
        self.entities
            .iter()
            .filter(|e| e.merchant.is_some())
            .map(|e| e.id)
            .collect()
    }

    /// 物品名 → 1-based 序号（oracle `GameItemSystem.GetStdItemIdx`，`GameItemSystem.cs:93-111`：
    /// `Name.Equals(name, OrdinalIgnoreCase)`，首个命中，1-based）。
    ///
    /// `crates/world` 的物品目录是 `Vec<StdItem>`、无 `Id` 列，与 `mir2-formula` 的
    /// `VecItemCatalog` 同口径（下标 + 1；`drop.rs:78-89`）。
    #[must_use]
    pub fn std_item_idx_by_name(&self, name: &str) -> usize {
        if name.is_empty() {
            return 0;
        }
        for (i, item) in self.item_catalog.iter().enumerate() {
            if equals_ordinal_ignore_case(&item.name, name) {
                return i + 1;
            }
        }
        0
    }

    /// 1-based 序号取模板（oracle `GetStdItem(ushort)`，`GameItemSystem`：`Idx -= 1` 回绕后越界 ⇒ null）。
    #[must_use]
    pub fn std_item_by_idx(&self, idx: usize) -> Option<&StdItem> {
        if idx == 0 {
            return None;
        }
        let i = idx - 1;
        self.item_catalog.get(i)
    }

    /// `BaseObject` ctor 的构造抽（`RunTick = now + Random(1500)`，`BaseObject.cs:439`），
    /// 单独成步以便宿主按 oracle 次序落定：`LocalDB.LoadMerchant` **正序**逐条抽（本函数），
    /// 再到 `MerchantInitialize` **倒序**挂图（[`World::spawn_merchant`]）。
    ///
    /// 取数走共享 `world.rng`，站点落 `crates/world/src/merchant.rs`（`merchant` 组）。
    #[must_use]
    pub fn merchant_ctor_run_delay(&mut self) -> i64 {
        self.clock_ms + i64::from(self.rng.random_below(MERCHANT_RUN_DELAY_MAX))
    }

    /// **M3⑤-实现 2 第②片**：`ProcessMerchants`（`WorldServer.Merchant.cs:65-110`）的单线程等价物。
    ///
    /// oracle 是 **200ms 定时线程**（`MerchantProcessor.cs:8`）+ 持久 `MerchantPosition` 索引 +
    /// 逐商人 `(now - RunTick) > RunTime` 门 + 墙钟预算 `(now - dwRunTick) > M2Share.NpcLimit`
    /// 断点续跑 + Ghost 超 60s 移除。
    ///
    /// **本片口径（owner `eb3bf42f` 裁决）**：
    /// 1. 节奏 200ms 用 **sim 时钟整数毫秒**（不是墙钟；同一窗口的第二次调用 no-op）；
    /// 2. `RunTick/RunTime` 门**逐字照搬**；
    /// 3. **丢掉墙钟预算分支**（`NpcLimit`，见 `whitelist.md` T-42）⇒ 一趟扫到底、扫完
    ///    [`World::merchant_position`] 归 0；
    /// 4. Ghost 用 sim 时钟的同单位整数（不是墙钟）。
    ///
    /// 跨 pass 交错（NpcLimit 预算决定某个该跑的商人落在哪一 pass）**不在判据内**；
    /// 同 pass 内次序 = `MerchantList` 顺序 = 实体槽位顺序。
    pub fn merchant_tick(&mut self) -> MerchantTickOutcome {
        let now = self.clock_ms;
        let mut out = MerchantTickOutcome::default();
        // 节奏门：同一 200ms 窗口内只跑一趟（`MerchantProcessor` 的 200ms 定时器，sim 时钟）。
        if now.saturating_sub(self.merchant_pass_tick) < MERCHANT_PASS_INTERVAL_MS {
            return out;
        }
        self.merchant_pass_tick = now;
        let ids = self.merchant_ids_sorted();
        let mut i = self.merchant_position;
        while i < ids.len() {
            let id = ids[i];
            let is_ghost = match self.entities.get(id) {
                Some(e) => e.ghost,
                None => {
                    i += 1;
                    continue;
                }
            };
            let (run_tick, ghost_tick) =
                match self.entities.get(id).and_then(|e| e.merchant.as_ref()) {
                    Some(st) => (st.run_tick, st.ghost_tick),
                    None => {
                        i += 1;
                        continue;
                    }
                };
            if !is_ghost {
                if (now - run_tick) > MERCHANT_RUN_TIME {
                    // oracle 先写 `RunTick = now` 再 `Run()`。
                    if let Some(st) = self.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) {
                        st.run_tick = now;
                    }
                    let r = merchant_run(self, id);
                    out.ran.push(id);
                    if let Some(dir) = r.turned {
                        out.turns.push((id, dir));
                    }
                    if r.hit {
                        out.hits.push(id);
                    }
                    if r.space_moved {
                        out.space_moves.push(id);
                    }
                }
            } else if (now - ghost_tick) > MERCHANT_GHOST_EXPIRE_MS {
                // oracle `MerchantList.RemoveAt(i); break;`（移除后本 pass 不再跑后面的）。
                self.entities.remove(id);
                out.removed.push(id);
                break;
            }
            i += 1;
        }
        // oracle `if (!boProcessLimit) MerchantPosition = 0;` —— 本片无预算分支 ⇒ 每 pass 归 0。
        self.merchant_position = 0;
        out
    }
}

/// `Merchant.Run` 的一次结果（供宿主出帧 / 证据；帧面接线属 ⑤-4）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MerchantRunOutcome {
    /// `TurnTo(RandomByte(8))` 抽到的朝向（`RM_TURN`）。
    pub turned: Option<u8>,
    /// 发 `RM_HIT`（`Random(50)==0` 支，或沙巴克战后恢复显形支）。
    pub hit: bool,
    /// 发 `RM_DISAPPEAR`（沙巴克战中进入 `FixedHideMode`）。
    pub disappear: bool,
    /// `BoCanMove` 到点 ⇒ `MapRandomMove`（`RM_SPACEMOVE_FIRE`）。
    pub space_moved: bool,
    /// 本趟触发了补货（`RefillGoods` 有 due 条目且命中 `>` 分支）。
    pub refilled: bool,
    /// 本趟补货门的 `dwDelayTick`（`ProcessRefillIndex * 500`；证据用）。
    pub refill_delay_ms: i64,
}

/// `ProcessMerchants` 一趟的结果（`World::tick` 写进 [`crate::world::TickRecord`]）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MerchantTickOutcome {
    /// 本 pass 执行了 `Run()` 的商人 id（按 `MerchantList` 顺序）。
    pub ran: Vec<i32>,
    /// `(id, dir)`：本 pass `TurnTo` 的结果（`RM_TURN`）。
    pub turns: Vec<(i32, u8)>,
    /// 本 pass 发 `RM_HIT` 的商人 id。
    pub hits: Vec<i32>,
    /// 本 pass `MapRandomMove` 的商人 id。
    pub space_moves: Vec<i32>,
    /// 本 pass 被 Ghost 60s 门移除的商人 id（oracle `RemoveAt`）。
    pub removed: Vec<i32>,
}

/// O-2 `rename_rand`（`ScriptParsers.cs:857` 的 `SystemShare.RandomNumber.GetRandomNumber(1, 200)`）。
///
/// **站点归组**：本函数在 `crates/world/src/merchant.rs` 里调 `get_random_number` ⇒
/// `#[track_caller]` 记到的站点是本文件 ⇒ Claim B 的 `merchant` 组（`consumer_groups.tsv` 的
/// rust 行）能认出它。宿主的闭包只转发到本函数，**不得**在 `bins/**` 里直接取数
/// （那会记成未归类站点 ⇒ `--require-closed` 判红）。
#[must_use]
pub fn merchant_rename_rand(rng: &mut RandomNumber, min: i32, max: i32) -> i32 {
    rng.get_random_number(min, max)
}

/// `Merchant.Run`（`Merchant.cs:112-170`）。
///
/// RNG 站点（全部落在本文件 ⇒ `merchant` 组）：
/// `Random(50)` → （命中时）`RandomByte(8)` → 未命中时再 `Random(50)`；
/// `BoCanMove` 到点时的 `MapRandomMove` 内部 2 次 `Random`（`BaseObject.cs:1214-1237`）。
///
/// 补货门/清理门/沙巴克显隐与 oracle 同构；库存与落库（`GoodsList`/`SaveGoodRecord`）
/// 是 ⑤-5 接缝（本片只推进计时态，见 [`refill_goods`]）。
pub fn merchant_run(world: &mut World, actor_id: i32) -> MerchantRunOutcome {
    let now = world.clock_ms;
    let mut outcome = MerchantRunOutcome::default();
    let Some(st) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.merchant.as_ref())
    else {
        return outcome;
    };
    let process_refill_index = st.process_refill_index;
    let refill_goods_tick = st.refill_goods_tick;
    let clear_expre_upgrade_tick = st.clear_expre_upgrade_tick;
    let castle_merchant = st.castle_merchant;
    let castle_under_war = st.castle_under_war;
    let can_move = st.can_move;
    let move_time = st.move_time;
    let move_tick = st.move_tick;

    // `Merchant.cs:117-121`：`dwDelayTick = ProcessRefillIndex * 500`；`now < dwDelayTick ⇒ 0`。
    let mut delay = i64::from(process_refill_index) * MERCHANT_REFILL_DELAY_STEP_MS;
    if now < delay {
        delay = 0;
    }
    if (now - refill_goods_tick) > MERCHANT_REFILL_GOODS_INTERVAL_MS + delay {
        if let Some(st) = world
            .entities
            .get_mut(actor_id)
            .and_then(|e| e.merchant.as_mut())
        {
            st.refill_goods_tick = now + delay;
        }
        outcome.refilled = refill_goods(world, actor_id);
        outcome.refill_delay_ms = delay;
    }
    // `Merchant.cs:126-130`：清理门（`ClearExpreUpgradeListData` 用墙钟 `DateTime.Now`，
    // 且升级表持久化属 ⑤-5 ⇒ 本片只推进计时态，无可观测效果）。
    if (now - clear_expre_upgrade_tick) > MERCHANT_CLEAR_EXPRE_UPGRADE_INTERVAL_MS {
        if let Some(st) = world
            .entities
            .get_mut(actor_id)
            .and_then(|e| e.merchant.as_mut())
        {
            st.clear_expre_upgrade_tick = now;
        }
    }
    // `Merchant.cs:132-141`：`Random(50)==0` ⇒ `TurnTo(RandomByte(8))`；否则 `Random(50)==0` ⇒ `RM_HIT`。
    if world.rng.random_below(MERCHANT_TURN_RANDOM_MOD) == 0 {
        let dir = world.rng.random_byte(MERCHANT_TURN_DIR_MAX);
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.dir = dir;
        }
        outcome.turned = Some(dir);
    } else if world.rng.random_below(MERCHANT_TURN_RANDOM_MOD) == 0 {
        outcome.hit = true;
    }
    // `Merchant.cs:143-160`：沙巴克战中 ⇒ 进 `FixedHideMode`（`RM_DISAPPEAR`）；否则恢复显形（`RM_HIT`）。
    if castle_merchant && castle_under_war {
        if world
            .entities
            .get(actor_id)
            .is_some_and(|e| !e.fixed_hide_mode)
        {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.fixed_hide_mode = true;
            }
            outcome.disappear = true;
        }
    } else if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.fixed_hide_mode)
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.fixed_hide_mode = false;
        }
        outcome.hit = true;
    }
    // `Merchant.cs:161-166`：`BoCanMove && (now - MoveTick) > MoveTime*1000` ⇒ `MapRandomMove`。
    if can_move && (now - move_tick) > i64::from(move_time) * 1000 {
        if let Some(st) = world
            .entities
            .get_mut(actor_id)
            .and_then(|e| e.merchant.as_mut())
        {
            st.move_tick = now;
        }
        map_random_move(world, actor_id);
        outcome.space_moved = true;
    }
    outcome
}

/// `Merchant.RefillGoods`（`Merchant.cs:209-276`）的**计时态**部分。
///
/// `GoodsList`（在售库存，`IList<IList<UserItem>>`）与 `LocalDb.SaveGoodRecord`/
/// `SaveGoodPriceRecord` 落库是 ⑤-5 接缝 ⇒ 本片 `GetRefillList` 取不到库存
/// （`nRefillCount = 0`），于是 oracle 的 `goods.Count > nRefillCount` 分支命中：
/// 置 `RefillTick` + （oracle 的 `return`）⇒ **一次 Run 最多补一件**（oracle quirk 照搬）。
///
/// 返回本趟是否命中补货分支。
#[must_use]
fn refill_goods(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let len = world
        .entities
        .get(actor_id)
        .and_then(|e| e.merchant.as_ref())
        .map_or(0, |st| st.refill_goods.len());
    for i in 0..len {
        let Some((item_name, count, refill_time, refill_tick)) = world
            .entities
            .get(actor_id)
            .and_then(|e| e.merchant.as_ref())
            .map(|st| {
                let g = &st.refill_goods[i];
                (g.item_name.clone(), g.count, g.refill_time, g.refill_tick)
            })
        else {
            break;
        };
        if (now - refill_tick) > i64::from(refill_time) * 60 * 1000 {
            if let Some(st) = world
                .entities
                .get_mut(actor_id)
                .and_then(|e| e.merchant.as_mut())
            {
                st.refill_goods[i].refill_tick = now;
            }
            let n_index = world.std_item_idx_by_name(&item_name);
            if n_index > 0 {
                // `GetRefillList(nIndex)`：`GoodsList` 未建模 ⇒ 库存恒 0（登记 ⑤-5）。
                let n_refill_count = 0usize;
                if count as usize > n_refill_count {
                    // oracle `CheckItemPrice + RefillGoodsItems + SaveGoodRecord + return`。
                    return true;
                }
                if (count as usize) < n_refill_count {
                    // oracle `RefillDelReFillItem + SaveGoodRecord`（无 return，继续循环）。
                }
            }
        }
    }
    false
}

/// `BaseObject.MapRandomMove`（`BaseObject.cs:1214-1237`）：`Random(Width-nEgdey-1)+nEgdey`
/// 两次取数（站点归 `merchant` 组），然后落点。
///
/// `SpaceMove` 不查阻挡（`ignore_block = true`）；不改 `Dir`。
fn map_random_move(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let (map_id, from_x, from_y) = (e.map_id, e.x, e.y);
    let Some(map) = world.maps.get(map_id) else {
        return;
    };
    // `BaseObject.cs:1218-1229`：`Height < 150` ⇒（`< 30` ⇒ 2，否则 20），否则 50。
    let n_edge_y = if map.height < 150 {
        if map.height < 30 {
            2
        } else {
            20
        }
    } else {
        50
    };
    let (w, h) = (map.width, map.height);
    let nx = world.rng.random_below(w - n_edge_y - 1) + n_edge_y;
    let ny = world.rng.random_below(h - n_edge_y - 1) + n_edge_y;
    let now = world.clock_ms;
    let moved = world.maps[map_id].move_object(
        MoveRequest {
            from_x,
            from_y,
            actor_id,
            to_x: nx,
            to_y: ny,
            ignore_block: true,
            add_time: now,
        },
        |_| false,
    );
    if moved {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.x = nx;
            e.y = ny;
        }
    }
}

/// C# `string.Equals(a, b, OrdinalIgnoreCase)` 的近似（与 `mir2-formula::drop` 同口径：
/// 逐字符 Unicode 大写折叠后按序比较）。
fn equals_ordinal_ignore_case(a: &str, b: &str) -> bool {
    a.chars()
        .flat_map(char::to_uppercase)
        .eq(b.chars().flat_map(char::to_uppercase))
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_data::models::StdItem;

    fn sample_state() -> MerchantState {
        MerchantState::new(
            "封魔谷/恶魔修理师".to_string(),
            "0139".to_string(),
            0,
            9,
            100,
            false,
            false,
            0,
            MerchantFlags::default(),
            vec![1, 2],
            vec![MerchantGoods {
                item_name: "金创药(小量)".to_string(),
                count: 5,
                refill_time: 5,
                refill_tick: 0,
            }],
            0,
        )
    }

    fn spawn_at(x: i32, y: i32) -> MerchantSpawn {
        MerchantSpawn {
            name: "卫家店".to_string(),
            x,
            y,
            map_id: 0,
            state: sample_state(),
        }
    }

    #[test]
    fn merchant_attaches_to_walkable_cell_and_is_listed() {
        let mut w = World::new(20, 20);
        let id = w.spawn_merchant(spawn_at(3, 4)).expect("可走格应挂图成功");
        let e = w.entities.get(id).expect("实体在");
        assert_eq!((e.x, e.y), (3, 4));
        assert!(e.add_to_mapped, "挂图成功 ⇒ AddToMaped=true");
        assert!(e.merchant.is_some());
        assert_eq!(w.merchant_ids_sorted(), vec![id]);
        // `CellType::Merchant` 落格（`Merchant.cs:433 CellType = CellType.Merchant` 的等价物）。
        let (cell, ok) = w.map(0).get_cell_info(3, 4);
        assert!(ok);
        assert_eq!(cell.count(), 1);
        assert_eq!(cell.obj_list[0].cell_type, CellType::Merchant);
    }

    #[test]
    fn merchant_dropped_when_cell_not_walkable() {
        // 越界格 ⇒ `CanWalk` 失败 ⇒ `AddtoMapSuccess = true` 且 `!IsHide` ⇒ 移除（oracle RemoveAt）。
        let mut w = World::new(8, 8);
        assert!(
            w.spawn_merchant(spawn_at(99, 99)).is_none(),
            "越界应挂图失败被移除"
        );
        assert!(w.merchant_ids_sorted().is_empty());
        assert_eq!(
            w.entities.len(),
            0,
            "oracle 从 MerchantList 移除 ⇒ 不留实体"
        );
    }

    #[test]
    fn std_item_lookup_is_one_based_and_case_insensitive() {
        let mut w = World::new(4, 4);
        w.item_catalog = vec![
            StdItem {
                name: "金创药(小量)".to_string(),
                price: 100,
                ..StdItem::default()
            },
            StdItem {
                name: "WoodenSword".to_string(),
                price: 30,
                ..StdItem::default()
            },
        ];
        assert_eq!(w.std_item_idx_by_name("金创药(小量)"), 1);
        assert_eq!(
            w.std_item_idx_by_name("woodensword"),
            2,
            "OrdinalIgnoreCase"
        );
        assert_eq!(w.std_item_idx_by_name("不存在"), 0);
        assert_eq!(w.std_item_idx_by_name(""), 0);
        assert_eq!(w.std_item_by_idx(2).map(|i| i.price), Some(30));
        assert!(w.std_item_by_idx(0).is_none());
        assert!(w.std_item_by_idx(9).is_none());
    }

    fn set_run_tick_stale(w: &mut World, id: i32) {
        if let Some(st) = w.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) {
            st.run_tick = -1000;
        }
    }

    /// 构造抽：`RunTick = now + Random(1500)`（`BaseObject.cs:439`），且走共享 `world.rng`
    /// （站点归 `merchant` 组）。
    #[test]
    fn ctor_draws_run_delay_from_shared_rng() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(1234);
        w.clock_ms = 5_000;
        let id = w.spawn_merchant(spawn_at(3, 4)).expect("挂图成功");
        let mut exp = RandomNumber::with_seed(1234);
        let delay = exp.random_below(MERCHANT_RUN_DELAY_MAX);
        let st = w.entities.get(id).unwrap().merchant.as_ref().unwrap();
        assert_eq!(st.run_tick, 5_000 + i64::from(delay));
        // 取数与独立流对齐（同一个 `world.rng`，无私有流）。
        assert_eq!(w.rng.random(), exp.random());
    }

    /// 节奏门（owner `eb3bf42f` ①）：同一 200ms 窗口内第二趟是 no-op —— 即使有人为把
    /// `run_tick` 拨旧（模拟「窗口内有到期商人」），也不得再跑。
    #[test]
    fn merchant_pass_cadence_blocks_second_pass_in_same_window() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(7);
        w.clock_ms = 0;
        let a = w.spawn_merchant(spawn_at(1, 1)).expect("挂图成功");
        set_run_tick_stale(&mut w, a);
        let first = w.merchant_tick();
        assert_eq!(first.ran, vec![a], "首趟应跑");
        // 人为拨旧（现实中可由「窗口中途新挂的商人 + 旧 RunTick」触发）⇒ 节奏门必须挡住第二趟。
        set_run_tick_stale(&mut w, a);
        let second = w.merchant_tick();
        assert!(second.ran.is_empty(), "同一 200ms 窗口内不得重复 Run");
        assert!(second.turns.is_empty() && second.hits.is_empty());
    }

    /// `RunTime` 门（owner `eb3bf42f` ②）：`(now - RunTick) > 250` 才 Run
    /// （`WorldServer.Merchant.cs:74`、`BaseObject.cs:440`）。
    #[test]
    fn merchant_run_time_gate_not_due_then_due() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(9);
        w.clock_ms = 0;
        let a = w.spawn_merchant(spawn_at(2, 2)).expect("挂图成功");
        if let Some(st) = w.entities.get_mut(a).and_then(|e| e.merchant.as_mut()) {
            st.run_tick = 0;
        }
        w.clock_ms = MERCHANT_RUN_TIME; // 250: 250-0 不 > 250
        assert!(w.merchant_tick().ran.is_empty(), "未到期不得 Run");
        w.clock_ms = MERCHANT_RUN_TIME + MERCHANT_PASS_INTERVAL_MS; // 450: 450-0 > 250
        assert_eq!(w.merchant_tick().ran, vec![a], "到期才 Run");
    }

    /// Ghost 60s 过期门（`WorldServer.Merchant.cs:84-91`）用 **sim 时钟**（不是墙钟）。
    #[test]
    fn merchant_ghost_expires_after_60s_sim_clock() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(11);
        w.clock_ms = 0;
        let a = w.spawn_merchant(spawn_at(4, 4)).expect("挂图成功");
        if let Some(e) = w.entities.get_mut(a) {
            e.ghost = true;
        }
        if let Some(st) = w.entities.get_mut(a).and_then(|e| e.merchant.as_mut()) {
            st.ghost_tick = 0;
        }
        w.clock_ms = MERCHANT_GHOST_EXPIRE_MS; // 60000: 不 > 60000
        assert!(w.merchant_tick().removed.is_empty(), "未满 60s 不移除");
        w.clock_ms = MERCHANT_GHOST_EXPIRE_MS + MERCHANT_PASS_INTERVAL_MS + 1; // 60201: > 60000
        assert_eq!(
            w.merchant_tick().removed,
            vec![a],
            "满 60s 移除（RemoveAt）"
        );
        assert!(w.merchant_ids_sorted().is_empty());
    }

    /// `Merchant.Run` 的 RNG 次序（`Merchant.cs:132-141`）：`Random(50)` →（命中）`RandomByte(8)`，
    /// 未命中再 `Random(50)`；用第二个同种子流逐抽复算。
    #[test]
    fn merchant_run_rng_stream_matches_oracle_order() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(4321);
        w.clock_ms = 100_000; // 让补货门也满足（RNG-free，不影响取数）
        let a = w.spawn_merchant(spawn_at(5, 5)).expect("挂图成功");
        set_run_tick_stale(&mut w, a);
        let mut exp = RandomNumber::with_seed(4321);
        let _ctor = exp.random_below(MERCHANT_RUN_DELAY_MAX);
        let out = merchant_run(&mut w, a);
        if exp.random_below(MERCHANT_TURN_RANDOM_MOD) == 0 {
            let d = exp.random_byte(MERCHANT_TURN_DIR_MAX);
            assert_eq!(out.turned, Some(d), "转向支取数次序");
            assert_eq!(w.entities.get(a).unwrap().ai.dir, d);
        } else if exp.random_below(MERCHANT_TURN_RANDOM_MOD) == 0 {
            assert!(out.hit, "RM_HIT 支取数次序");
        }
        assert_eq!(w.rng.random(), exp.random(), "Run 后两流仍同源");
    }

    /// `RefillGoods` 计时：到期命中 `>` 分支 ⇒ 置 `RefillTick` 并（oracle `return`）
    /// **一次 Run 最多补一件**；紧接着的第二次 Run 不再补。
    #[test]
    fn merchant_refill_once_per_run_and_advances_tick() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(3);
        w.item_catalog = vec![StdItem {
            name: "金创药(小量)".to_string(),
            price: 100,
            ..StdItem::default()
        }];
        w.clock_ms = 0;
        let a = w.spawn_merchant(spawn_at(6, 6)).expect("挂图成功");
        w.clock_ms = MERCHANT_REFILL_GOODS_INTERVAL_MS + 1; // 300001 > 300000
        set_run_tick_stale(&mut w, a);
        let out = merchant_run(&mut w, a);
        assert!(out.refilled, "到期应命中补货");
        let st = w.entities.get(a).unwrap().merchant.as_ref().unwrap();
        assert_eq!(st.refill_goods[0].refill_tick, w.clock_ms, "补货门推进");
        // 同一时刻再来一次：补货门不满足 ⇒ 不补（oracle 口径）。
        set_run_tick_stale(&mut w, a);
        assert!(!merchant_run(&mut w, a).refilled, "同一 tick 不重复补货");
    }

    /// `BoCanMove` 到点 ⇒ `MapRandomMove` 消耗 2 次 `Random`（`BaseObject.cs:1214-1237`）
    /// 并按地图高定 `nEgdey`（150/30 两档）。
    #[test]
    fn merchant_can_move_uses_two_map_random_draws() {
        let mut w = World::new(200, 200);
        w.rng = RandomNumber::with_seed(99);
        w.clock_ms = 1_000_000;
        let mut spawn = spawn_at(10, 10);
        spawn.state.can_move = true;
        spawn.state.move_time = 0; // 到点条件：now - move_tick > 0（move_tick 置 now 后下次不满足）
        let a = w.spawn_merchant(spawn).expect("挂图成功");
        set_run_tick_stale(&mut w, a);
        if let Some(st) = w.entities.get_mut(a).and_then(|e| e.merchant.as_mut()) {
            st.move_tick = -1000; // 到点（`now - MoveTick > MoveTime*1000`）
        }
        let mut exp = RandomNumber::with_seed(99);
        let _ctor = exp.random_below(MERCHANT_RUN_DELAY_MAX);
        let out = merchant_run(&mut w, a);
        // 复算 turn/hit 两段取数。
        if exp.random_below(MERCHANT_TURN_RANDOM_MOD) == 0 {
            let _d = exp.random_byte(MERCHANT_TURN_DIR_MAX);
        } else {
            let _h = exp.random_below(MERCHANT_TURN_RANDOM_MOD);
        }
        // `MapRandomMove`：nEdgey=50（height 200 >= 150）⇒ Random(200-50-1)=Random(149) 两次。
        let nx = exp.random_below(200 - 50 - 1) + 50;
        let ny = exp.random_below(200 - 50 - 1) + 50;
        assert!(out.space_moved, "到点应 SpaceMove");
        let e = w.entities.get(a).unwrap();
        assert_eq!((e.x, e.y), (nx, ny), "落点坐标 = oracle 两次取数");
        assert_eq!(w.rng.random(), exp.random(), "两组取数后两流仍同源");
    }

    /// reviewer-6 **M-2**：`ActorRace.Merchant == Animal == 50` ⇒ `monster_ai_tick` 的
    /// `merchant.is_none()` 过滤（`crates/world/src/monai.rs:549`）是行为关键：商人不得被
    /// `monster_run` 当怪跑（置 `target_x = -1` / 消耗 AI 取数）。删掉该守卫 ⇒ 必红。
    #[test]
    fn monster_ai_tick_skips_merchants_race_is_animal() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(5);
        w.clock_ms = 10_000;
        let a = w.spawn_merchant(spawn_at(7, 7)).expect("挂图成功");
        // 让「若被当怪跑」必然产生可观测副作用：walk 到点 + 无目标 ⇒ `monster_run` 置 `target_x=-1`
        // 并在无目标分支调 `wondering`（消耗 RNG）。
        if let Some(e) = w.entities.get_mut(a) {
            e.ai.walk_speed = -1;
            e.ai.walk_tick = 0;
            e.ai.target_creature = None;
            e.ai.target_x = 42; // 哨兵
        }
        let rng_before = w.rng.clone();
        crate::monai::monster_ai_tick(&mut w);
        let e = w.entities.get(a).unwrap();
        assert_eq!(
            e.ai.target_x, 42,
            "商人不得被 monster_run 当怪跑（race 50 == Animal 50）"
        );
        assert_eq!(e.ai.walk_tick, 0, "商人不得被 AI 推进 walk_tick");
        let mut expect = rng_before;
        assert_eq!(w.rng.random(), expect.random(), "跳过商人 ⇒ 不消耗 AI 取数");
    }
}
