//! 商人（`Merchant`）世界实体 + 挂图 `Initialize` + `ProcessMerchants` tick —— M3⑤-实现 2 第①/②片。
//!
//! oracle 锚点（逐条复核于本片）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`MerchantState`] 字段 | `src/GameSrv/Npc/Merchant.cs:10-70`（`IMerchant`）+ `BaseObject` 的 `RunTick/RunTime`（`BaseObject.cs:439-440`） |
//! | [`World::spawn_merchant`] 的 `Initialize`/`AddtoMapSuccess` | `src/M2Server/Actor/BaseObject.Base.cs:14-21` |
//! | 两个**彼此独立**的次序：[`World::insert_merchant`] 的实体槽位序 = 文件正序（`MerchantList`/tick 序）；[`World::attach_merchant_to_map`] 的 `obj_list` 追加序 = 倒序 | `WorldServer.Merchant.cs:8`（倒序 `Initialize` **不**重排 `MerchantList`）+ `:68`（`ProcessMerchants` 正序）|
//! | 挂图失败移除（`!IsHide` 例外） | `src/GameSvr/Word/WorldServer.Merchant.cs:8-25` |
//! | `FindMap(MapName) == null ⇒ 丢弃` | `WorldServer.Merchant.cs:24-31`（**门在宿主侧按 `MapRegistry::id_of` 施加**，见 [`crate::merchant`] 模块尾与 `bins/game-svr/src/merchant.rs`） |
//! | [`World::merchant_tick`] | `WorldServer.Merchant.cs:65-110` + `MerchantProcessor.cs:8`（200ms 定时） |
//! | [`merchant_run`] | `Merchant.cs:112-170`（RNG/显隐/`MapRandomMove`）+ `BaseObject.cs:1214-1237` |
//! | `refill_goods` 计时态 | `Merchant.cs:209-276`（`GoodsList` 库存已建模，见下；`SaveGoodRecord` 在 oracle 是空壳） |
//!
//! ## 为什么门在宿主侧
//! `SystemShare.MapMgr.FindMap(MapName)` 的部署态等价物是 `MapRegistry::id_of`（同 `MapInfo` +
//! `Map/*.map` 合并口径，见 `crates/envir`）。`crates/world` 只持有**下标式**地图表、不含逻辑名
//! ⇒ 名字→`map_id` 的解析归宿主；本模块只负责「拿到 `map_id` 后的挂图/落格」。
//!
//! ## 未建模（登记）
//! - `GoodsList`（在售库存，`Merchant.cs:34`）**已建模**（⑤-4 第①片）：`RefillGoods` 按实际
//!   库存量补/删、`AddItemToGoodsList` 由玩家卖出回收。剩下的 `ItemPriceList` 覆盖价与
//!   `SaveGoodRecord/SaveGoodPriceRecord` 在 oracle 本身就是空壳（`LocalDB.cs:916/1111/954/1162`）
//!   ⇒ 无行为可搬，不另登记。
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

use crate::entity::{BaseAnimalCtorDraws, Entity};
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
/// 故 [`MerchantState::new`] 置该值，[`World::insert_merchant`]（及一步到位的
/// [`World::spawn_merchant`]）见它才抽。
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

/// `IMerchant.GoodsList` 里的一件：oracle `UserItem`（`Item`/`MakeIndex`/`Dura`/`DuraMax` + `Desc`）。
///
/// 与 `crates/world` 已有的掉落/定价口径同型（`mir2_formula::drop::UserItem`）⇒ ⑤-4 的买/卖
/// 直接复用公式层（`mir2_formula::shop`）的取价函数，宿主只做 `BagItem` ↔ `MerchantItem` 互转。
pub use mir2_formula::drop::UserItem as MerchantItem;

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
    /// `IMerchant.GoodsList`（`IList<IList<UserItem>>`，`Merchant.cs:34`）——在售库存。
    ///
    /// 由 `Merchant.RefillGoods`（脚本 `[goods]` 补货）与 `Merchant.AddItemToGoodsList`
    /// （玩家卖出回收，`Merchant.cs:1417`）写入；`Merchant.ClientBuyItem` 从这里取件。
    /// **组内顺序敏感**：写入用 `Insert(0)`（新件在前），组序 = 首次出现序。
    pub goods_list: Vec<Vec<MerchantItem>>,
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
            goods_list: Vec::new(),
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
    /// **两段挂图（①）**：建实体并入 `EntityStore`（**不挂图**）。
    ///
    /// oracle 里 `MerchantList` 由 `LocalDB.LoadMerchant` **正序** `AddMerchant` 追加，且
    /// `MerchantInitialize` 的 `for (i = Count-1; i >= 0; i--)` **并不重排 `MerchantList`**
    /// （`WorldServer.Merchant.cs:8`，只决定 `AddToMap` 对 map cell `obj_list` 的追加序）；
    /// `ProcessMerchants`（同文件 `:68`）又从 `MerchantPosition` 起**正序**遍历 `MerchantList`
    /// 调 `Run()`。⇒ 宿主必须**正序**逐条调用本函数（实体槽位序 = 插入序 = `MerchantList` 序），
    /// 再**倒序** [`World::attach_merchant_to_map`] 落格；两个次序彼此独立，不可合并成
    /// 同一个 `rev()`（见 reviewer-5 `0f220cac` F-1）。
    ///
    /// 返回 `Some(id)` = 已入世界（槽位已分配）；`None` = `map_id` 越界（oracle 不可达，
    /// 宿主 `FindMap` 门已保证）。
    ///
    /// **构造抽**：按 oracle `new Merchant()` 的整条 ctor 公共链（`BaseObject`→`AnimalObject`）
    /// 从**共享 `world.rng`** 抽 **4 个数**（`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`，
    /// 唯一抽数点 [`BaseAnimalCtorDraws::draw`]；站点归 `merchant` 组）——**仅当**
    /// [`MerchantState::run_tick`] 仍为 [`MERCHANT_RUN_TICK_UNSET`]（宿主已用
    /// [`World::merchant_ctor_run_delay`] 按 `LocalDB.LoadMerchant` 正序预抽时不再重复抽）。
    /// `refill_goods_tick`/`clear_expre_upgrade_tick`/`move_tick` 置为本刻 `clock_ms`（oracle ctor
    /// 的 `GetTickCount()` 同口径；`Goods.RefillTick` 由脚本解析置 0，见 [`MerchantGoods`]）。
    pub fn insert_merchant(&mut self, spawn: MerchantSpawn) -> Option<i32> {
        if spawn.map_id >= self.maps.len() {
            return None;
        }
        let now = self.clock_ms;
        let map_id = spawn.map_id;
        let (x, y) = (spawn.x, spawn.y);
        let mut state = spawn.state;
        if state.run_tick == MERCHANT_RUN_TICK_UNSET {
            // 无预抽（[`World::spawn_merchant`] 一步到位）⇒ 在此抽整条公共链 4 抽，同
            // [`World::merchant_ctor_run_delay`]；只有 `RunTick` 分量被保留（见其注释）。
            let ctor = BaseAnimalCtorDraws::draw(&mut self.rng);
            state.run_tick = now + i64::from(ctor.run_tick_delta);
        }
        state.refill_goods_tick = now;
        state.clear_expre_upgrade_tick = now;
        state.move_tick = now;
        state.ghost_tick = 0;

        let mut e = Entity::new(0, spawn.name, x, y);
        // `Entity::new(0, …)` 的占位 id 必须换成**唯一** id（照 `World::add_entity` 的口径）：
        // 否则所有商人 `id == 0`、`EntityStore::insert` 的 `index.insert(0, …)` 互相覆盖，
        // `merchant_ids_sorted` 返回一串 0、`merchant_tick` 会反复 Run 同一个实体。
        e.id = self.entities.alloc_id();
        e.race = ACTOR_RACE_MERCHANT;
        e.map_id = map_id;
        // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`。
        e.verify_tick = now;
        e.merchant = Some(state);
        Some(self.entities.insert(e))
    }

    /// **两段挂图（②）**：执行 oracle `Initialize()` 的挂图段：
    /// `AddtoMapSuccess = !(CanWalk(CurrX,CurrY,true) && AddToMap())`
    /// （`BaseObject.Base.cs:14-21`）；为真且 `!IsHide` ⇒ 从世界移除
    /// （`WorldServer.Merchant.cs:16-22`）。
    ///
    /// 宿主按 `MerchantInitialize` 的**倒序**逐条调用（owner `b311a876b1` 裁决）⇒ map cell
    /// `obj_list` 的追加序 = 倒序。该序与 [`World::insert_merchant`] 决定的实体槽位序 /
    /// `MerchantList` 序**互相独立**（reviewer-5 `0f220cac` F-1）。
    ///
    /// 返回 `true` = 商人留在世界；`false` = 挂图失败被移除（oracle `RemoveAt`）。
    pub fn attach_merchant_to_map(&mut self, id: i32) -> bool {
        let Some(entity) = self.entities.get(id) else {
            return false;
        };
        let (map_id, x, y, is_hide) = (
            entity.map_id,
            entity.x,
            entity.y,
            entity.merchant.as_ref().is_some_and(|m| m.is_hide),
        );
        if map_id >= self.maps.len() {
            return false;
        }
        let now = self.clock_ms;
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
            return false;
        }
        if let Some(ent) = self.entities.get_mut(id) {
            ent.add_to_mapped = !add_to_map_success;
        }
        true
    }

    /// 一步到位：建实体（[`World::insert_merchant`]）+ 挂图（[`World::attach_merchant_to_map`]）。
    ///
    /// 生产装载按 oracle 两段次序**分两步**调用（正序 insert、倒序 attach，见宿主
    /// `spawn_merchants`）；本函数保留给单测 / 无预抽的直接挂图场景。
    ///
    /// 返回 `Some(id)` = 商人留在世界；`None` = `map_id` 越界或挂图失败被移除。
    pub fn spawn_merchant(&mut self, spawn: MerchantSpawn) -> Option<i32> {
        let id = self.insert_merchant(spawn)?;
        if self.attach_merchant_to_map(id) {
            Some(id)
        } else {
            None
        }
    }

    /// 按**槽位顺序**列出所有商人实体 id（`MerchantList` 的确定性等价物；供 ⑤-4 取列表）。
    ///
    /// 槽位序 = [`World::insert_merchant`] 的**插入序**；宿主按 `LocalDB.LoadMerchant` 文件正序
    /// 插入 ⇒ 本序列 = 文件序（即 `merchant_tick` 的 pass 内次序，见 [`World::merchant_tick`]）。
    /// ⚠ 与 `MerchantInitialize` 倒序造成的 map cell `obj_list` 追加序**是两回事**（见
    /// [`World::attach_merchant_to_map`]）；不要用槽位序去推断 `obj_list` 序，反之亦然
    /// （reviewer-5 `0f220cac` F-1）。
    ///
    /// 返回的 id 由 [`World::insert_merchant`] 用 `EntityStore::alloc_id` 分配 ⇒ **唯一**
    /// （`merchant_tick` 直接以它为键；`merchant_ids_sorted` 不再返回重复占位 id）。
    #[must_use]
    pub fn merchant_ids_sorted(&self) -> Vec<i32> {
        self.entities
            .iter()
            .filter(|e| e.merchant.is_some())
            .map(|e| e.id)
            .collect()
    }

    /// `Merchant.GetRefillList(int nIndex)`（`Merchant.cs:278`）：按 `goods[0].Index` 找同 `Index`
    /// 的库存组，返回其在 `GoodsList` 里的下标（无 ⇒ `None`）。
    #[must_use]
    pub fn merchant_goods_group(&self, id: i32, index: u16) -> Option<usize> {
        let st = self.entities.get(id)?.merchant.as_ref()?;
        st.goods_list
            .iter()
            .position(|g| g.first().is_some_and(|it| it.index == index))
    }

    /// `GoodsList` 的只读视图（`IList<IList<UserItem>>`）；供宿主出 `SM_SENDGOODSLIST` 与买/卖取件。
    ///
    /// 组序 = 首次出现序；组内序 = `Insert(0)` 后的新件在前（与 oracle 同，**顺序敏感**）。
    #[must_use]
    pub fn merchant_goods_groups(&self, id: i32) -> Option<&[Vec<MerchantItem>]> {
        Some(&self.entities.get(id)?.merchant.as_ref()?.goods_list)
    }

    /// [`World::merchant_goods_groups`] 的可变版（⑤-4 买/卖直接改在售库存）。
    #[must_use]
    pub fn merchant_goods_groups_mut(&mut self, id: i32) -> Option<&mut Vec<Vec<MerchantItem>>> {
        Some(&mut self.entities.get_mut(id)?.merchant.as_mut()?.goods_list)
    }

    /// `Merchant.AddItemToGoodsList`（`Merchant.cs:1417`）：`Dura <= 0` 不收；否则
    /// `GetRefillList(Index)` 命中就 `Insert(0)`，未命中就新建组（组序 = 首次出现序）。
    pub fn merchant_add_item_to_goods_list(&mut self, id: i32, item: MerchantItem) {
        // oracle `if (userItem.Dura <= 0) return;`（`Merchant.cs:1419`）；`Dura` 是 `ushort` ⇒ `<= 0` ≡ `== 0`。
        if item.dura == 0 {
            return;
        }
        let Some(st) = self.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) else {
            return;
        };
        match st
            .goods_list
            .iter_mut()
            .find(|g| g.first().is_some_and(|it| it.index == item.index))
        {
            Some(group) => group.insert(0, item),
            None => st.goods_list.push(vec![item]),
        }
    }

    /// `ClientBuyItem` 的取件：`list20.RemoveAt(j)`，空组再 `GoodsList.RemoveAt(i)`
    /// （`Merchant.cs:1269-1273`）。返回被取走的库存件；`group`/`item` 越界 ⇒ `None`。
    pub fn merchant_take_goods_item(
        &mut self,
        id: i32,
        group: usize,
        item: usize,
    ) -> Option<MerchantItem> {
        let st = self.entities.get_mut(id)?.merchant.as_mut()?;
        let g = st.goods_list.get_mut(group)?;
        if item >= g.len() {
            return None;
        }
        let taken = g.remove(item);
        if g.is_empty() {
            st.goods_list.remove(group);
        }
        Some(taken)
    }

    /// `GameItemSystem.CopyToUserItemFromName`（`GameItemSystem.cs:113`）的 `Merchant.RefillGoodsItems`
    /// 口径：按名（`OrdinalIgnoreCase`，同 [`World::std_item_idx_by_name`]）取模板首条 ⇒
    /// `Index = 下标+1`、`Dura = DuraMax = tpl.DuraMax`、`MakeIndex = M2Share.GetItemNumber()`。
    #[must_use]
    pub fn make_merchant_item_by_name(&mut self, name: &str) -> Option<MerchantItem> {
        let idx = self.std_item_idx_by_name(name);
        if idx == 0 {
            return None;
        }
        let tpl = self.std_item_by_idx(idx)?;
        let (dura, dura_max) = (tpl.dura_max, tpl.dura_max);
        let make_index = self.item_number.next_value();
        Some(MerchantItem {
            make_index,
            index: u16::try_from(idx).unwrap_or(u16::MAX),
            dura,
            dura_max,
            desc: [0; 14],
        })
    }

    /// `GetRefillList(nIndex)?.Count`（`Merchant.cs:286`）：同 `Index` 库存组的件数（无 ⇒ 0）。
    #[must_use]
    pub fn merchant_goods_count(&self, id: i32, index: u16) -> usize {
        self.merchant_goods_group(id, index)
            .and_then(|gi| self.merchant_goods_groups(id).map(|gs| gs[gi].len()))
            .unwrap_or(0)
    }

    /// `Merchant.RefillDelReFillItem`（`Merchant.cs:317`）：同 `Index` 组从**尾部**删 `n` 件。
    pub fn merchant_del_items_from_end(&mut self, id: i32, index: u16, n: usize) {
        let Some(st) = self.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) else {
            return;
        };
        let Some(gi) = st
            .goods_list
            .iter()
            .position(|g| g.first().is_some_and(|it| it.index == index))
        else {
            return;
        };
        let g = &mut st.goods_list[gi];
        let mut left = n;
        while left > 0 && !g.is_empty() {
            g.pop();
            left -= 1;
        }
        if g.is_empty() {
            st.goods_list.remove(gi);
        }
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

    /// oracle `new Merchant()` 的 ctor 公共链构造抽，单独成步以便宿主按 oracle 次序落定：
    /// `LocalDB.LoadMerchant` **正序**逐条抽（本函数），再到 `MerchantInitialize` **倒序**挂图
    /// （[`World::spawn_merchant`]）。
    ///
    /// **抽 4 个数**（整条 `BaseObject`→`AnimalObject` 链）：`RunTick = now + Random(1500)`、
    /// `SearchTime = Random(2000)+2000`、`AttackTick = now - Random(3000)`、`WalkTick = now - Random(3000)`
    /// （`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`）。唯一抽数点
    /// [`BaseAnimalCtorDraws::draw`]；原实现只抽第 1 个 ⇒ 每条商人少抽 3 次、共享 RNG 流错位
    /// （本卡订正）。商人**不消费**后三个（`Merchant.Run` 只读 `RunTick`；动物 AI 不驱动 NPC），
    /// 故只返回 `RunTick`，余 3 抽**只推进共享 RNG 流**以保流位置与 oracle 一致。
    ///
    /// 取数走共享 `world.rng`，站点落 `crates/world/src/merchant.rs`（`merchant` 组）。
    #[must_use]
    pub fn merchant_ctor_run_delay(&mut self) -> i64 {
        let ctor = BaseAnimalCtorDraws::draw(&mut self.rng);
        self.clock_ms + i64::from(ctor.run_tick_delta)
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
    /// 同 pass 内次序 = `MerchantList` 顺序 = 实体槽位插入序（[`World::insert_merchant`] 正序）；
    /// `MerchantInitialize` 的倒序只定 `obj_list` 追加序，**不**改变本次序（reviewer-5 `0f220cac` F-1）。
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
    // `Merchant.cs:170` 末尾 `base.Run()`：`BaseObject.Run` 的 VerifyMapTime 段
    // （`BaseObject.Base.cs:223-228`）——刷新本格 `CellObject.AddTime`，防 60s 过期。
    world.verify_map_time(actor_id);
    outcome
}

/// `Merchant.RefillGoods`（`Merchant.cs:209-276`）。
///
/// 完整对齐 oracle：补货门 `(now - RefillTick) > RefillTime*60*1000` ⇒ 置 `RefillTick`，
/// `GetStdItemIdx(name) > 0` 时按 `GetRefillList` 的实际库存量走
/// `goodCout > nRefillCount`（`RefillGoodsItems` 新建 `delta` 件 + oracle `return`）或
/// `goodCout < nRefillCount`（`RefillDelReFillItem` 从尾删）分支；`GoodsList` 超 1000 的收缩
/// （脚本 `[goods]` 内保留 5000，否则 1000）。`LocalDb.SaveGoodRecord/SaveGoodPriceRecord`
/// 在 oracle 是**空壳**（`LocalDB.cs:916/1111`）⇒ 不落库。
///
/// 返回本趟是否命中 `>` 分支（oracle 的 `return`）。
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
                let index = u16::try_from(n_index).unwrap_or(u16::MAX);
                let n_refill_count = world.merchant_goods_count(actor_id, index);
                let target = usize::try_from(count).unwrap_or(0);
                if target > n_refill_count {
                    // `CheckItemPrice(nIndex)`（`ItemPriceList` 空 ⇒ no-op）。
                    for _ in 0..(target - n_refill_count) {
                        if let Some(item) = world.make_merchant_item_by_name(&item_name) {
                            world.merchant_add_item_to_goods_list(actor_id, item);
                        }
                    }
                    // oracle `return`（一次 Run 最多处理一条 `[goods]`）。
                    return true;
                }
                if target < n_refill_count {
                    world.merchant_del_items_from_end(actor_id, index, n_refill_count - target);
                }
            }
        }
    }
    // `GoodsList` 上限收缩（`Merchant.cs:248-274`）。
    let group_count = world
        .merchant_goods_groups(actor_id)
        .map_or(0, <[Vec<MerchantItem>]>::len);
    for i in 0..group_count {
        let Some((index, g_len)) = world
            .merchant_goods_groups(actor_id)
            .and_then(|gs| gs.get(i))
            .and_then(|g| g.first().map(|it| (it.index, g.len())))
        else {
            continue;
        };
        if g_len > 1000 {
            let in_refill = world
                .entities
                .get(actor_id)
                .and_then(|e| e.merchant.as_ref())
                .is_some_and(|st| {
                    st.refill_goods.iter().any(|g| {
                        let n = world.std_item_idx_by_name(&g.item_name);
                        n > 0 && u16::try_from(n).ok() == Some(index)
                    })
                });
            let keep = if in_refill { 5000 } else { 1000 };
            if g_len > keep {
                world.merchant_del_items_from_end(actor_id, index, g_len - keep);
            }
        }
    }
    false
}

/// `BaseObject.MapRandomMove`（`BaseObject.cs:1235-1236`）的**两次初始取数**：
/// `Random(Width-nEgdey-1)+nEgdey`、`Random(Height-nEgdey-1)+nEgdey`。
///
/// `nEgdey` 只看**目标图高**（`BaseObject.cs:1218-1229`）：`Height < 150` ⇒（`< 30` ⇒ 2，否则 20），
/// 否则 50。
///
/// **站点归 `merchant` 组**：C# 侧这两抽的 site 恒为 `BaseObject.MapRandomMove@BaseObject.cs:…`
/// （无论调用者是 `Merchant.Run` 还是魔号 21 的 `MagSaceMove`）⇒ `consumer_groups.tsv` 把该 site
/// 归 `merchant`。Rust 侧唯一抽数点放在本文件，保证两侧**组 vocabulary 对称**（魔号 21 接线卡）。
#[must_use]
pub(crate) fn map_random_move_xy(
    rng: &mut impl mir2_shared::rng::RandomSource,
    width: i32,
    height: i32,
) -> (i32, i32) {
    let n_edge_y = if height < 150 {
        if height < 30 {
            2
        } else {
            20
        }
    } else {
        50
    };
    let nx = rng.random_below(width - n_edge_y - 1) + n_edge_y;
    let ny = rng.random_below(height - n_edge_y - 1) + n_edge_y;
    (nx, ny)
}

/// `BaseObject.MapRandomMove`（`BaseObject.cs:1214-1237`）：`Random(Width-nEgdey-1)+nEgdey`
/// 两次取数（站点归 `merchant` 组），然后 `SpaceMove`（`BaseObject.cs:1103-1172`）——
/// **落点不可走时进 `SpaceMoveGetRandXY` 移位环继续取数**（至多 201 轮）。
///
/// 逐行对照（本片 2026-10-17）：
///
/// | oracle | 本仓 |
/// | --- | --- |
/// | `MapRandomMove` 首两抽 `Random(Width-nEgdey-1)` / `Random(Height-nEgdey-1)`（`:1235-1236`） | 同（`nEdgeY` 分段见下） |
/// | `SpaceMove` 同服分支：删旧格 → `CurrX/CurrY = nX/nY` → `SpaceMoveGetRandXY`（`:1128`） | [`crate::slave::space_move_get_rand_xy`]（与 `RecallSlave` **同一** oracle 函数，不复制第二份） |
/// | `CanWalk(x, y, true)`（`Envirnoment.cs:442-485`）：只判 `cellSuccess && Valid`，忽略格上活体 | `get_cell_info(x, y).1` |
/// | 不可走：`x < Width-n1C-1 ⇒ x += n18`，否则 `x = Random(Width)`；`y` 同段推进 / `Random(Height)`；`n14++ >= 201` 放弃（`:1067-1094`） | 复用的函数逐句同构 |
/// | 201 轮仍不可走 ⇒ 回滚旧坐标（`:1147-1153`） | 直接返回（坐标不动；取数流水已推进）——见下「未建模」 |
/// | 落点可走 ⇒ `AddMapObject`（`ignore_block`，且 `SpaceMoveGetRandXY` 已保证 `Valid`） | [`crate::map::MapGrid::move_object`]（`ignore_block = true`） |
///
/// 不改 `Dir`。
///
/// **取数站点归组**：首两抽在 `map_random_move_xy`（本文件 ⇒ `merchant` 组，与 C# 的
/// `BaseObject.MapRandomMove@BaseObject.cs:` 对齐）；环内 `Random(Width)`/`Random(Height)` 落在
/// `crates/world/src/space_move.rs`（**`space_move` 组**，与 C# 的 `BaseObject.SpaceMoveGetRandXY@BaseObject.cs:`
/// 对齐）——**刻意不加 `#[track_caller]`**：加了会让 Rust 侧归 `merchant` 而 C# 侧归 `space_move`，破坏对称。
///
/// **可达性（收窄）**：只有 `merchant_run`（DB `CanMove != 0 && MoveTime > 0`）可达。
/// `npc_run` 的该支**不可达**——固定 NPC 经 `Npcs.txt` 装载 `BoCanMove` 恒 `false`
/// （`npc.rs` 已登记）⇒ 不改 NPC 面。
///
/// **未建模（登记）**：201 轮失败时 oracle 先 `DeleteFromMap` 再回滚 `AddMapObject`（`BaseObject.cs:1114/1150`）。
/// 本仓不预删，故失败时**坐标/取数/RNG 流水完全一致**，仅当旧格 `Valid` 且同格还有其它对象时
/// oracle 会把本对象挪到 `obj_list` 末尾（格内顺序）——位置与可达行为无差。
/// **`pub(crate)`**：`Merchant.Run`（`merchant.rs`）与固定 NPC 的 `Merchant` 类 Run
/// （`npc.rs::merchant_body_run`，`CastleOfficial : Merchant`）**共用**本函数。
pub(crate) fn map_random_move(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let (map_id, from_x, from_y) = (e.map_id, e.x, e.y);
    let Some(map) = world.maps.get(map_id) else {
        return;
    };
    let (w, h) = (map.width, map.height);
    // 首两抽（oracle `BaseObject.cs:1235-1236`）——与魔号 21 共用同一抽数点。
    let (nx, ny) = map_random_move_xy(&mut world.rng, w, h);
    // `SpaceMove`（`BaseObject.cs:1128`）：以 `(nX, nY)` 为起点找可走格；起点即墙时
    // `SpaceMoveGetRandXY` 进移位环继续抽数（201 轮上限）。
    let found = {
        let map = &world.maps[map_id];
        crate::slave::space_move_get_rand_xy(&mut world.rng, w, h, nx, ny, |x, y| {
            map.get_cell_info(x, y).1
        })
    };
    let Some((nx, ny)) = found else {
        // oracle 回滚到旧坐标（`BaseObject.cs:1147-1153`）：坐标不动。
        return;
    };
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
    use crate::map::CellAttribute;
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

    /// 复算 **整条** ctor 公共链 4 抽（`BaseObject` `:439/441` + `AnimalObject` `:76/77`，
    /// 字面 oracle 参数），返回第 0 抽（`RunTick` 的 `Random(1500)`）。
    ///
    /// **订正缘由**：原测试只复算 1 抽 ⇒ 与 oracle 相反（少 3 抽）；现按 4 抽字面复算。
    fn ctor_chain_first_draw(exp: &mut RandomNumber) -> i32 {
        let d = exp.random_below(MERCHANT_RUN_DELAY_MAX); // #0 BaseObject.RunTick = Random(1500)
        let _ = exp.random_below(2000); // #1 BaseObject.SearchTime = Random(2000)+2000
        let _ = exp.random_below(3000); // #2 AnimalObject.AttackTick = Random(3000)
        let _ = exp.random_below(3000); // #3 AnimalObject.WalkTick = Random(3000)
        d
    }

    /// 构造抽：**整条** ctor 公共链 4 抽（`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`），
    /// `RunTick = now + Random(1500)`（`BaseObject.cs:439`）＝ 公共链第 0 抽；走共享 `world.rng`
    /// （站点归 `merchant` 组）。
    ///
    /// **订正缘由**：原断言只抽 1 次（只看 `Random(1500)`）⇒ 每条商人少抽 3 次、与 oracle 相反；
    /// 现按 4 抽复算（少抽即「同源」断言必红）。
    #[test]
    fn ctor_draws_run_delay_from_shared_rng() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(1234);
        w.clock_ms = 5_000;
        let id = w.spawn_merchant(spawn_at(3, 4)).expect("挂图成功");
        let mut exp = RandomNumber::with_seed(1234);
        let delay = ctor_chain_first_draw(&mut exp);
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
        let _ctor = ctor_chain_first_draw(&mut exp);
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
        let _ctor = ctor_chain_first_draw(&mut exp);
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

    /// owner `eb3bf42f` ① 的规模版：**256 条商人**在同一 pass 里各跑一次（不重复 Run），
    /// 且同一 200ms 窗口的第二趟全部 no-op。
    #[test]
    fn merchant_pass_runs_each_of_many_merchants_once() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(21);
        w.clock_ms = 0;
        let mut ids = Vec::new();
        for i in 0..256 {
            let mut spawn = spawn_at(1 + (i % 30), 1 + (i / 30));
            spawn.name = format!("m{i}");
            ids.push(w.spawn_merchant(spawn).expect("挂图成功"));
        }
        let make_due = |w: &mut World| {
            for id in &ids {
                if let Some(st) = w.entities.get_mut(*id).and_then(|e| e.merchant.as_mut()) {
                    st.run_tick = -1000;
                }
            }
        };
        w.clock_ms = 10_000;
        make_due(&mut w);
        let out = w.merchant_tick();
        assert_eq!(out.ran.len(), ids.len(), "256 条全部应 Run");
        let mut unique = out.ran.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(unique.len(), ids.len(), "同一 pass 内不得重复 Run");
        // 同一 200ms 窗口第二趟：即便把 RunTick 拨旧，节奏门也得挡住全部 256 条。
        make_due(&mut w);
        assert!(
            w.merchant_tick().ran.is_empty(),
            "同一 200ms 窗口内 256 条都不得重复 Run"
        );
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

    fn item(index: u16, make_index: i32, dura: u16) -> MerchantItem {
        MerchantItem {
            make_index,
            index,
            dura,
            dura_max: 10,
            desc: [0; 14],
        }
    }

    /// `Merchant.AddItemToGoodsList`（`Merchant.cs:1417`）：同 `Index` 组 `Insert(0)`、
    /// `Dura <= 0` 不收、无组则新建（组序 = 首次出现序）。红检：改成 `push` ⇒ 顺序断言必红。
    #[test]
    fn merchant_add_item_to_goods_list_inserts_front() {
        let mut w = World::new(16, 16);
        w.clock_ms = 0;
        let id = w.spawn_merchant(spawn_at(1, 1)).expect("挂图成功");
        w.merchant_add_item_to_goods_list(id, item(1, 10, 5));
        w.merchant_add_item_to_goods_list(id, item(2, 20, 5));
        w.merchant_add_item_to_goods_list(id, item(1, 11, 5));
        // `Dura <= 0` 不收。
        w.merchant_add_item_to_goods_list(id, item(1, 12, 0));
        let groups = w.merchant_goods_groups(id).expect("商人还在");
        assert_eq!(groups.len(), 2, "两个 Index ⇒ 两组");
        assert_eq!(groups[0][0].make_index, 11, "Insert(0) ⇒ 后加的在前");
        assert_eq!(groups[0][1].make_index, 10);
        assert_eq!(groups[1][0].index, 2, "组序 = 首次出现序");
        assert_eq!(w.merchant_goods_count(id, 1), 2);
        assert_eq!(w.merchant_goods_count(id, 9), 0);
    }

    /// `ClientBuyItem` 的取件：`RemoveAt(j)`，空组 `GoodsList.RemoveAt(i)`（`Merchant.cs:1269-1273`）。
    #[test]
    fn merchant_take_goods_item_removes_empty_group() {
        let mut w = World::new(16, 16);
        w.clock_ms = 0;
        let id = w.spawn_merchant(spawn_at(1, 1)).expect("挂图成功");
        w.merchant_add_item_to_goods_list(id, item(1, 10, 5));
        let taken = w.merchant_take_goods_item(id, 0, 0).expect("取件");
        assert_eq!(taken.make_index, 10);
        assert!(w.merchant_goods_groups(id).unwrap().is_empty(), "空组移除");
        assert!(w.merchant_take_goods_item(id, 0, 0).is_none());
    }

    /// `RefillGoods`（`Merchant.cs:209`）：到期后按 `[goods]` 目标件数 **建出对应数量** 的实例
    /// （`CopyToUserItemFromName`：`Index` 1-based、`Dura = DuraMax = 模板 DuraMax`、
    /// `MakeIndex = M2Share.GetItemNumber()`），并置 `RefillTick`（oracle `return`）。
    #[test]
    fn merchant_refill_creates_goods_items_from_script_entries() {
        let mut w = World::new(16, 16);
        w.rng = RandomNumber::with_seed(13);
        w.clock_ms = 0;
        w.item_catalog = vec![StdItem {
            name: "金创药(小量)".to_string(),
            price: 100,
            dura_max: 10,
            ..StdItem::default()
        }];
        let id = w.spawn_merchant(spawn_at(2, 2)).expect("挂图成功");
        w.clock_ms = MERCHANT_REFILL_GOODS_INTERVAL_MS + 1;
        set_run_tick_stale(&mut w, id);
        let out = merchant_run(&mut w, id);
        assert!(out.refilled, "到期应补货");
        let groups = w.merchant_goods_groups(id).expect("商人还在");
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 5, "[goods] Count=5 ⇒ 建 5 件");
        assert_eq!(groups[0][0].index, 1);
        assert_eq!(groups[0][0].dura, 10);
        assert_eq!(groups[0][0].dura_max, 10);
        assert!(groups[0][0].make_index > 0, "MakeIndex 由计数器分配");
    }

    /// 红检牙齿：`RefillGoods` 的 `goodCout < nRefillCount` 支（库存多于 `[goods]` 目标）
    /// 从**尾部**删到目标数（`RefillDelReFillItem`，`Merchant.cs:317`）。
    #[test]
    fn merchant_refill_trims_excess_from_end() {
        let mut w = World::new(16, 16);
        w.rng = RandomNumber::with_seed(17);
        w.clock_ms = 0;
        w.item_catalog = vec![StdItem {
            name: "金创药(小量)".to_string(),
            dura_max: 10,
            ..StdItem::default()
        }];
        let id = w.spawn_merchant(spawn_at(3, 3)).expect("挂图成功");
        // 目标 5，库存已 7 ⇒ 删 2（尾部）。
        for i in 0..7 {
            w.merchant_add_item_to_goods_list(id, item(1, 100 + i, 5));
        }
        w.clock_ms = MERCHANT_REFILL_GOODS_INTERVAL_MS + 1;
        set_run_tick_stale(&mut w, id);
        // oracle `<` 支不 `return`（继续循环）⇒ `refilled` 为 false，库存被删到目标 5。
        let _ = merchant_run(&mut w, id);
        assert_eq!(w.merchant_goods_count(id, 1), 5, "删到目标 5 件");
    }

    // ---------------------------------------------------------------------------------
    // `MapRandomMove` 移位环（卡 `openmir2-map-random-move-shift-loop`，tier=R）
    //
    // 期望表的**独立复算**：下面自写 .NET Framework legacy `System.Random`（Knuth 减法）
    // 与 oracle `MapRandomMove` / `SpaceMoveGetRandXY` 环体；**不复用**本仓的
    // `space_move_get_rand_xy` / `DotNetRandom`（否则期望与实现同源，改坏必绿）。
    // ---------------------------------------------------------------------------------

    /// .NET `System.Random` 常量（`MBIG`/`MSEED`）。
    const ORACLE_MBIG: i32 = i32::MAX;
    const ORACLE_MSEED: i32 = 161_803_398;

    /// .NET Framework legacy `System.Random`（Knuth 减法滞后斐波那契）的独立复刻。
    struct OracleRandom {
        seed_array: [i32; 56],
        inext: usize,
        inextp: usize,
    }

    impl OracleRandom {
        fn new(seed: i32) -> Self {
            let mut seed_array = [0i32; 56];
            // `Random(int Seed)`：`subtraction = Seed == Int32.MinValue ? Int32.MaxValue : Abs(Seed)`。
            let subtraction = if seed == i32::MIN {
                i32::MAX
            } else {
                seed.abs()
            };
            let mut mj = ORACLE_MSEED - subtraction;
            seed_array[55] = mj;
            let mut mk = 1i32;
            for i in 1..55usize {
                let ii = (21 * i) % 55;
                seed_array[ii] = mk;
                mk = mj - mk;
                if mk < 0 {
                    mk += ORACLE_MBIG;
                }
                mj = seed_array[ii];
            }
            for _ in 1..5 {
                for i in 1..56usize {
                    seed_array[i] -= seed_array[1 + (i + 30) % 55];
                    if seed_array[i] < 0 {
                        seed_array[i] += ORACLE_MBIG;
                    }
                }
            }
            OracleRandom {
                seed_array,
                inext: 0,
                inextp: 21,
            }
        }

        fn internal_sample(&mut self) -> i32 {
            let mut loc_inext = self.inext + 1;
            let mut loc_inextp = self.inextp + 1;
            if loc_inext >= 56 {
                loc_inext = 1;
            }
            if loc_inextp >= 56 {
                loc_inextp = 1;
            }
            let mut ret_val = self.seed_array[loc_inext] - self.seed_array[loc_inextp];
            if ret_val == ORACLE_MBIG {
                ret_val -= 1;
            }
            if ret_val < 0 {
                ret_val += ORACLE_MBIG;
            }
            self.seed_array[loc_inext] = ret_val;
            self.inext = loc_inext;
            self.inextp = loc_inextp;
            ret_val
        }

        /// `Next()`（后续流水比对用）。
        fn next_value(&mut self) -> i32 {
            self.internal_sample()
        }

        /// `Next(int maxValue)` = `(int)(Sample() * maxValue)`，`Sample() = InternalSample()/MBIG`。
        fn next_max(&mut self, max_value: i32) -> i32 {
            assert!(max_value >= 0);
            (self.internal_sample() as f64 * (1.0 / f64::from(ORACLE_MBIG)) * f64::from(max_value))
                as i32
        }
    }

    /// oracle `SpaceMoveGetRandXY`（`BaseObject.cs:1040-1101`）的独立环体复刻。
    /// 返回 `(成功?, 终点x, 终点y, 逐抽 (参数, 结果), 迭代数)`。
    fn oracle_space_move_get_rand_xy<F>(
        rng: &mut OracleRandom,
        width: i32,
        height: i32,
        mut x: i32,
        mut y: i32,
        mut can_walk: F,
    ) -> (bool, i32, i32, Vec<(i32, i32)>, i32)
    where
        F: FnMut(i32, i32) -> bool,
    {
        let n18 = if width < 80 { 3 } else { 10 };
        let n1c = if height < 150 {
            if height < 50 {
                2
            } else {
                15
            }
        } else {
            50
        };
        let mut draws = Vec::new();
        let mut n14 = 0;
        loop {
            if can_walk(x, y) {
                return (true, x, y, draws, n14);
            }
            if x < (width - n1c - 1) {
                x += n18;
            } else {
                x = rng.next_max(width);
                draws.push((width, x));
                if y < (height - n1c - 1) {
                    y += n18;
                } else {
                    y = rng.next_max(height);
                    draws.push((height, y));
                }
            }
            n14 += 1;
            if n14 >= 201 {
                break;
            }
        }
        (false, x, y, draws, n14)
    }

    /// oracle `MapRandomMove`（`BaseObject.cs:1214-1237`）+ `SpaceMove` 的独立复刻。
    /// 返回 `(SpaceMoveGetRandXY 成功?, 首两抽落点, 终点, 逐抽 (参数, 结果), 移位环迭代数)`。
    #[allow(clippy::type_complexity)]
    fn oracle_map_random_move<F>(
        rng: &mut OracleRandom,
        width: i32,
        height: i32,
        can_walk: F,
    ) -> (bool, (i32, i32), (i32, i32), Vec<(i32, i32)>, i32)
    where
        F: FnMut(i32, i32) -> bool,
    {
        let n_edge_y = if height < 150 {
            if height < 30 {
                2
            } else {
                20
            }
        } else {
            50
        };
        let mut draws = Vec::new();
        let nx = rng.next_max(width - n_edge_y - 1) + n_edge_y;
        draws.push((width - n_edge_y - 1, nx - n_edge_y));
        let ny = rng.next_max(height - n_edge_y - 1) + n_edge_y;
        draws.push((height - n_edge_y - 1, ny - n_edge_y));
        let (ok, fx, fy, shift, iters) =
            oracle_space_move_get_rand_xy(rng, width, height, nx, ny, can_walk);
        draws.extend(shift);
        (ok, (nx, ny), (fx, fy), draws, iters)
    }

    /// 把 map 0 上满足 `pred` 的格置为 `LowWall`（`Valid == false`）。
    fn block_cells(w: &mut World, pred: impl Fn(i32, i32) -> bool) {
        let map = &mut w.maps[0];
        let (width, height) = (map.width, map.height);
        for x in 0..width {
            for y in 0..height {
                if pred(x, y) {
                    let (cell, ok) = map.get_cell_info_mut(x, y);
                    assert!(ok, "({x},{y}) 应为合法格");
                    cell.set_attribute(CellAttribute::LowWall);
                }
            }
        }
    }

    /// `spawn_merchant` 走 [`BaseAnimalCtorDraws::draw`] 的 4 抽（`entity.rs:80/104-112`）；
    /// 独立流必须同样前移，才能从 `MapRandomMove` 的**首抽**开始对表。
    fn oracle_after_ctor(seed: i32) -> OracleRandom {
        let mut rng = OracleRandom::new(seed);
        for m in crate::entity::BASE_ANIMAL_CTOR_DRAW_MODS {
            let _ = rng.next_max(m);
        }
        rng
    }

    /// 逐项核对期望表：同一 seed 的 `world.rng` 类型逐抽产出表中值
    /// （参数/结果任一项不符即红；前 4 抽为商人 ctor 公共链）。
    fn assert_draw_table_replay(seed: i32, table: &[(i32, i32)]) {
        let mut probe = RandomNumber::with_seed(seed);
        for m in crate::entity::BASE_ANIMAL_CTOR_DRAW_MODS {
            let _ = probe.random_below(m);
        }
        for (i, &(arg, want)) in table.iter().enumerate() {
            assert_eq!(probe.random_below(arg), want, "第 {i} 抽（Random({arg})）");
        }
    }

    /// **阴性对照**：落点可走 ⇒ **恰好 2 抽**、落点 = 首次抽到的格。
    ///
    /// 「恰好 2 抽」用**随后连续三抽**断言（单抽不具判别力：`Next(10)` 在多索引上常同值）。
    #[test]
    fn map_random_move_walkable_landing_draws_exactly_twice() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(12_345);
        let id = w.spawn_merchant(spawn_at(10, 10)).expect("挂图成功");
        let mut oracle = oracle_after_ctor(12_345);
        let (ok, init, final_xy, table, iters) =
            oracle_map_random_move(&mut oracle, 64, 64, |_, _| true);
        assert!(ok);
        assert_eq!(init, (54, 55), "独立复算：首两抽落点");
        assert_eq!(final_xy, (54, 55), "独立复算：落点可走 ⇒ 不自移位");
        assert_eq!(table, vec![(43, 34), (43, 35)], "独立复算：恰好 2 抽");
        assert_eq!(iters, 0, "未进入移位环");
        assert_draw_table_replay(12_345, &table);

        map_random_move(&mut w, id);
        let e = w.entities.get(id).expect("实体在");
        assert_eq!((e.x, e.y), final_xy, "落点 = 首次抽到的格");
        // 随后连续三抽：与只消费了 2 抽的独立流逐项一致 ⇒ 实现没有多抽。
        for i in 0..3 {
            assert_eq!(
                w.rng.random(),
                oracle.next_value(),
                "随后第 {i} 抽：落点可走时不得有多余取数"
            );
        }
    }

    /// **主判据**：随机落点为墙 ⇒ 进移位环继续抽数，逐抽与独立 oracle 期望一致，
    /// 终点也一致（64×64：`n18=3`、`n1C=15`，`x≥48` 后走 `Random(Width)`/`Random(Height)`）。
    #[test]
    fn map_random_move_wall_landing_enters_shift_loop_and_matches_oracle() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(5);
        let id = w.spawn_merchant(spawn_at(10, 10)).expect("挂图成功");
        // 最小语料：只有最右两列/最下两行可走。
        block_cells(&mut w, |x, y| !(x >= 60 || y >= 60));

        let mut oracle = oracle_after_ctor(5);
        let (ok, init, final_xy, table, iters) =
            oracle_map_random_move(&mut oracle, 64, 64, |x, y| x >= 60 || y >= 60);
        assert!(ok);
        assert_eq!(init, (39, 59));
        assert!(
            !(init.0 >= 60 || init.1 >= 60),
            "语料前提：首抽落点必须是墙"
        );
        assert_eq!(
            table,
            vec![(43, 19), (43, 39), (64, 9), (64, 60)],
            "移位环逐抽独立表（含 2 次随机跳点）"
        );
        assert_eq!(iters, 4, "3 次 `x += 3` 后进随机跳点支");
        assert_eq!(final_xy, (9, 60), "独立复算终点");
        assert_draw_table_replay(5, &table);

        map_random_move(&mut w, id);
        let e = w.entities.get(id).expect("实体在");
        assert_eq!((e.x, e.y), final_xy, "落点与 oracle 一致");
        for i in 0..3 {
            assert_eq!(
                w.rng.random(),
                oracle.next_value(),
                "移位环后第 {i} 个后续抽"
            );
        }
    }

    /// **上限判据**：201 轮全不可走 ⇒ 放弃并回滚旧坐标；取数在水位处截断。
    ///
    /// 语料：整图 `LowWall`。独立流共 35 抽（表 31 项 = 2 初始 + 29 环内；含 ctor 4 抽），末抽发生在**第 201 轮**（`n14 == 200`）
    /// ⇒ 环上限改成 200 时该抽消失、随后流水必错（本用例红）。
    #[test]
    fn map_random_move_shift_loop_stops_at_201_and_rolls_back() {
        let mut w = World::new(64, 64);
        w.rng = RandomNumber::with_seed(3);
        let id = w.spawn_merchant(spawn_at(10, 10)).expect("挂图成功");
        block_cells(&mut w, |_, _| true);

        let mut oracle = oracle_after_ctor(3);
        let (ok, init, _last_try, table, iters) =
            oracle_map_random_move(&mut oracle, 64, 64, |_, _| false);
        assert!(!ok, "全墙 ⇒ 201 轮后放弃");
        assert_eq!(init, (44, 27));
        assert_eq!(iters, 201, "环上限 201（自增后判）");
        assert_eq!(
            table,
            vec![
                (43, 24),
                (43, 7),
                (64, 16),
                (64, 60),
                (64, 22),
                (64, 24),
                (64, 32),
                (64, 51),
                (64, 44),
                (64, 1),
                (64, 5),
                (64, 57),
                (64, 7),
                (64, 54),
                (64, 48),
                (64, 30),
                (64, 13),
                (64, 37),
                (64, 31),
                (64, 5),
                (64, 63),
                (64, 29),
                (64, 32),
                (64, 34),
                (64, 24),
                (64, 31),
                (64, 18),
                (64, 15),
                (64, 32),
                (64, 4),
                (64, 44),
            ],
            "201 轮内逐抽独立表（末项 = 第 201 轮的 Random(64)）"
        );
        assert_draw_table_replay(3, &table);

        map_random_move(&mut w, id);
        let e = w.entities.get(id).expect("实体在");
        assert_eq!((e.x, e.y), (10, 10), "放弃后回滚旧坐标");
        for i in 0..3 {
            assert_eq!(w.rng.random(), oracle.next_value(), "第 {i} 个后续抽");
        }
    }

    /// **分段判据**（大图）：`Width >= 80 ⇒ n18 = 10`、`Height >= 150 ⇒ n1C = 50`，
    /// 且 `x`/`y` 两条随机跳点支都在集成路径上被走到。
    #[test]
    fn map_random_move_large_map_uses_width80_height150_segments() {
        let mut w = World::new(200, 300);
        w.rng = RandomNumber::with_seed(7);
        let id = w.spawn_merchant(spawn_at(10, 10)).expect("挂图成功");
        block_cells(&mut w, |x, y| !(x >= 190 || y >= 290));

        let mut oracle = oracle_after_ctor(7);
        let (ok, init, final_xy, table, iters) =
            oracle_map_random_move(&mut oracle, 200, 300, |x, y| x >= 190 || y >= 290);
        assert!(ok);
        assert_eq!(init, (104, 218));
        assert_eq!(
            table,
            vec![(149, 54), (249, 168), (200, 8), (200, 190)],
            "n18=10 步进到 `Width-n1C-1` 后：Random(200)"
        );
        assert_eq!(iters, 22);
        assert_eq!(final_xy, (190, 238));
        assert_draw_table_replay(7, &table);

        map_random_move(&mut w, id);
        let e = w.entities.get(id).expect("实体在");
        assert_eq!((e.x, e.y), final_xy);
        for i in 0..3 {
            assert_eq!(w.rng.random(), oracle.next_value(), "第 {i} 个后续抽");
        }
    }
}
