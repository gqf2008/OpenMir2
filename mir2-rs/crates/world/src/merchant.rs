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

use crate::castle_tax::MerchantCastleBinding;
use crate::entity::{BaseAnimalCtorDraws, Entity};
use crate::map::{CellType, MoveRequest};
use crate::world::World;

/// `ActorRace.Merchant`（`src/OpenMir2/Enums/Race.cs`：`Merchant = 50`，与 `Animal` 同值）。
///
/// ⚠ 这是**外观轴** `RaceImg` 的取值（`Merchant.cs:84` 的 `RaceImg = ActorRace.Merchant`），
/// **不是**商人的**行为轴** `Race` —— oracle 商人的 `Race` 继承 `NormNpc` ctor 的
/// `ActorRace.NPC`（10，`NormNpc.cs:42`）。两者在 `Merchant`/`Animal` 上同为 50，极易混用：
/// 把本值写进 [`Entity::race`] 会让 `PlayObject.IsAttackTarget` 的 `HAM_ALL` 门
/// （`target.Race < NPC(10) || > PeaceNpc(15)`）把商人当动物，玩家可选中/攻击商人（T-31 修正）。
///
/// `crates/world` 的怪物 AI（[`crate::monai::monster_ai_tick`]）按 [`Entity::merchant`] 是否存在
/// 显式排除商人实体，**不**依赖 race 数值；商人 tick 同理走 [`World::merchant_tick`]。
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
    /// `NormNpc.Castle`（`Merchant.Castle`）——`Initialize()` 时刻
    /// `CastleMgr.InCastleWarArea(this)`（`NormNpc.cs:1674-1678`）命中的城堡下标。
    ///
    /// **spawn 时刻固定**：oracle 的 `Merchant.Run → MapRandomMove` 之后不重算
    /// （`design-castle-tax-and-item-name-20261022.md` §1.2.5），故本字段只在
    /// [`World::insert_merchant`] 写一次（或由 [`World::set_merchant_castle_index`] 显式重绑）；
    /// `Some(i)` ⇔ oracle `Castle != null`，与 `castle_merchant` **互相独立**
    /// （位置落在攻城区/皇宫/密道/`EnvirList` 图内即命中）。
    pub castle_index: Option<usize>,
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
    /// `IMerchant.UpgradeWeaponList`（`Merchant.cs:54/92`）：在途武器升级记录。
    ///
    /// **只在内存**（oracle `SaveUpgradeWeaponRecord` 恒 `return -1`，`MySqlDB.cs:345-348`）
    /// ⇒ 重启即丢（照抄）。缺省空表，**不改 [`MerchantState::new`] 签名**（简报 §3.1）。
    pub upgrade_weapon_list: Vec<WeaponUpgradeInfo>,
    /// `Config.ClearExpireUpgradeWeaponDays`（`Merchant.cs:1085`）的商人侧快照。
    ///
    /// `Merchant.Run` 的 10min 清理门用；缺省 = oracle 缺省 8（`GameSvrConf.cs:1423`），
    /// 宿主装载后按部署值覆写（`[Setup] ClearExpireUpgradeWeaponDays`）。
    pub clear_expire_upgrade_days: u8,
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
            castle_index: None,
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
            upgrade_weapon_list: Vec::new(),
            clear_expire_upgrade_days: 8,
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
        // `NormNpc.Initialize()`：`Castle = SystemShare.CastleMgr.InCastleWarArea(this)`
        // （`NormNpc.cs:1674-1678`）。在出生点解析**一次**并固定（`Merchant.Run` 的移动不重算）；
        // 与 `CastleMerchant` 无关——只要位置落在攻城区/皇宫/密道/`EnvirList` 图内就命中。
        state.castle_index = self.castle_war_area_at(Some(map_id), x, y);
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
        // 行为轴：oracle 商人继承 `NormNpc` ctor 的 `Race = ActorRace.NPC`（10，
        // `NormNpc.cs:42`）；`Merchant` ctor **不覆写** `Race`，只置外观轴 `RaceImg`
        // （`Merchant.cs:84`）。所有按 `Race` 的判定（攻击目标选择/受击/掉落/目标清理）
        // 都读 10 ⇒ 玩家 `HAM_ALL` 的 `IsAttackTarget` 不会把商人当动物选中。
        e.race = crate::entity::ACTOR_RACE_NPC;
        // 外观轴：`RaceImg = ActorRace.Merchant`（50，`Merchant.cs:84`），供客户端可见帧
        // `GetFeature() = MakeMonsterFeature(RaceImg, MonsterWeapon, Appr)`（`BaseObject.Base.cs:911`）。
        e.race_img = ACTOR_RACE_MERCHANT;
        // `Merchant : NormNpc`（`Merchant.cs:16`）⇒ `NormNpc()` ctor `SuperMan = true`
        // （`NormNpc.cs:41`）：商人打不死（`BaseObject.Die():318-322`）且每次 `Run` 回满（`:32-36`）。
        // **不得按 race 置位**：`ActorRace.NPC(10)` 也是普通固定 NPC 的 race，`RaceImg` 50 又与
        // 鸡/鹿/狼等动物怪同值（`Race.cs:26/34/40`）⇒ 只能按创建点（类）置位。
        e.super_man = true;
        // `Entity::new` 是玩家 ctor 等价物（SearchTime=1000）⇒ 非玩家还原占位缺省值。
        e.ai.search_time = crate::entity::BASE_ANIMAL_SEARCH_TIME;
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

    /// 商人的 `castle_index`（`NormNpc.Castle` 的下标）；
    /// `None` = 该 id 不是商人**或**商人无城堡（`Castle == null`）。
    ///
    /// 需要区分这两种 `None` 时用 [`World::merchant_castle_binding`]。
    #[must_use]
    pub fn merchant_castle_index(&self, id: i32) -> Option<usize> {
        self.entities.get(id)?.merchant.as_ref()?.castle_index
    }

    /// [`MerchantCastleBinding`] 视图（`castle_merchant` + `castle_index`；`None` = 非商人）。
    #[must_use]
    pub fn merchant_castle_binding(&self, id: i32) -> Option<MerchantCastleBinding> {
        let st = self.entities.get(id)?.merchant.as_ref()?;
        Some(MerchantCastleBinding::new(
            st.castle_merchant,
            st.castle_index,
        ))
    }

    /// 覆写 `castle_index`。oracle 只在 `Initialize()` 写一次；本 setter 供构造级夹具
    /// （`CastleMgr` 晚于商人装载的场景）与 `GetCastle` 后的显式重绑使用。
    ///
    /// 返回 `false` = 该 id 不是商人（无 `merchant` 状态）。
    pub fn set_merchant_castle_index(&mut self, id: i32, castle_index: Option<usize>) -> bool {
        let Some(st) = self.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) else {
            return false;
        };
        st.castle_index = castle_index;
        true
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

// ─────────────────────── 武器升级 NPC（`@upgradenow` / `@getbackupgnow`） ───────────────────────
//
// oracle 锚点（逐行；卡 `openmir2-m3-upgrade-npc`，简报
// `tests/parity/m3/design-upgrade-npc-20261020.md`，以「评审员更正 A1-A9」为准）：
//
// | 本模块 | oracle |
// | --- | --- |
// | [`UpgradeWeaponConfig`] | `GameSvrConf.cs:1418-1432` + `ServerConf.cs:230-242` + `[Names] BlackStone` |
// | [`WeaponUpgradeInfo`] | `src/Modules/SystemModule/Data/WeaponUpgradeInfo.cs:8-20` |
// | [`upgrade_wapon_add_value`] | `Merchant.cs:373-525`（`UpgradeWaponAddValue`，**零 RNG**） |
// | [`upgrade_wapon_impl`] | `Merchant.cs:527-586`（`UpgradeWapon`，**零 RNG**） |
// | [`get_backupg_weapon_impl`] | `Merchant.cs:592-759`（`GetBackupgWeapon`，RNG 密集） |
// | [`clear_expired_upgrades`] | `Merchant.cs:1076-1088`（`ClearExpreUpgradeListData`） |
//
// **RNG 站点（本卡红线）**：取回路径的全部抽数都落本文件（`consumer_groups.tsv` 的
// `merchant` 组）；宿主 `bins/game-svr` 不得直接抽数。
//
// **持久化**：在途列表只在内存（oracle `SaveUpgradeWeaponRecord` 恒 `return -1`、
// `MySqlDB.cs:345-348` 无 SQL）⇒ 不实现 `upgradeinfo*` 三表，重启即丢（照抄，见简报 §2.5）。
//
// **照抄的 oracle 缺陷**（BUG-REPLICA，必须钉住）：
// - 取回成功只发 `SM_ADDITEM`，**不** `AddItemToBag`（`Merchant.cs:744` 被注释）⇒ 入参
//   `item_list` 长度不变，武器只是客户端死数据（宿主断言）。
// - SC 支的 `n90` 取的是 `upgradeInfo.Mc`（`Merchant.cs:721`），不是 `Sc`。
// - 提交端同名检查是 C# `==`（**区分大小写**）；取回端是 `OrdinalIgnoreCase` 且命中不 `break`。

/// `Config.UpgradeWeapon*` + `[Names] BlackStone`（部署值见简报 §0/§4；缺省 = `GameSvrConf.cs`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeWeaponConfig {
    /// `Config.UpgradeWeaponPrice`（缺省 10000）。
    pub price: i32,
    /// `Config.UPgradeWeaponGetBackTime`（毫秒，缺省 3_600_000）。
    pub get_back_time_ms: i64,
    /// `Config.ClearExpireUpgradeWeaponDays`（byte，缺省 8）。
    pub clear_expire_days: u8,
    /// `[Names] BlackStone`（缺省「黑铁矿」）。
    pub black_stone: String,
    /// `Config.UpgradeWeaponDCRate`（缺省 100；部署 50）。
    pub dc_rate: i32,
    /// `Config.UpgradeWeaponDCTwoPointRate`（缺省 30）。
    pub dc_two_point_rate: i32,
    /// `Config.UpgradeWeaponDCThreePointRate`（缺省 200）。
    pub dc_three_point_rate: i32,
    /// `Config.UpgradeWeaponMCRate`。
    pub mc_rate: i32,
    /// `Config.UpgradeWeaponMCTwoPointRate`。
    pub mc_two_point_rate: i32,
    /// `Config.UpgradeWeaponMCThreePointRate`。
    pub mc_three_point_rate: i32,
    /// `Config.UpgradeWeaponSCRate`。
    pub sc_rate: i32,
    /// `Config.UpgradeWeaponSCTwoPointRate`。
    pub sc_two_point_rate: i32,
    /// `Config.UpgradeWeaponSCThreePointRate`。
    pub sc_three_point_rate: i32,
}

impl Default for UpgradeWeaponConfig {
    fn default() -> Self {
        Self {
            price: 10_000,
            get_back_time_ms: 3_600_000,
            clear_expire_days: 8,
            black_stone: "黑铁矿".to_string(),
            dc_rate: 100,
            dc_two_point_rate: 30,
            dc_three_point_rate: 200,
            mc_rate: 100,
            mc_two_point_rate: 30,
            mc_three_point_rate: 200,
            sc_rate: 100,
            sc_two_point_rate: 30,
            sc_three_point_rate: 200,
        }
    }
}

/// `WeaponUpgradeInfo`（`WeaponUpgradeInfo.cs:8-20`）：在途升级记录。
///
/// `get_back_tick`/`upgrade_time_ms` 用世界 **sim 时钟**（`World::clock_ms`）；
/// oracle 用 `GetTickCount()`/`DateTime.Now`——进程内等价（简报 §2.5/§6-3）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WeaponUpgradeInfo {
    /// `UserName`（`PlayObject.ChrName`）。
    pub user_name: String,
    /// `UserItem`（提交时的武器拷贝；取回时写 `desc[10]`）。
    pub user_item: MerchantItem,
    /// `Dc`（`UpgradeWaponAddValue` 的 `btDc`）。
    pub dc: u8,
    /// `Sc`。
    pub sc: u8,
    /// `Mc`。
    pub mc: u8,
    /// `Dura`（`btDura`；取回耐久分支的输入）。
    pub dura: u8,
    /// `UpgradeTime`（`DateTime.Now` ⇒ sim `clock_ms`；过期清理用）。
    pub upgrade_time_ms: i64,
    /// `GetBackTick`（`GetTickCount()` ⇒ sim `clock_ms`；时限门用）。
    pub get_back_tick: i64,
}

/// `UpgradeWapon` 成功的结果：删除列表 + 新记录。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpgradeWaponResult {
    /// `(ItemName, MakeIndex)`；顺序 = 背包**从后往前**扫描的命中序（oracle `delItemList`）。
    pub deleted: Vec<(String, i32)>,
    /// 新加入 `UpgradeWeaponList` 的记录。
    pub info: WeaponUpgradeInfo,
}

/// `Merchant.UpgradeWapon`（`Merchant.cs:527-586`）的**第一阶段**出口。
///
/// `Accepted` = 已扣费、已拷贝 `UserItem`，但**背包材料未吃**（oracle 在 `AddValue` 之前
/// `RecalcAbilitys`）⇒ 宿主必须先重算，再调
/// [`World::merchant_upgrade_wapon_finish`]。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeWaponBegin {
    /// 受理成功（材料仍在包里）。
    Accepted(WeaponUpgradeInfo),
    /// 已有同名记录（**精确 `==`**，区分大小写）⇒ `~@upgradenow_ing`。
    AlreadyUpgrading,
    /// 无武器 / 金币不足 / 无黑铁 ⇒ `~@upgradenow_fail`。
    Fail,
}

/// `Merchant.UpgradeWapon`（`Merchant.cs:527-586`）的三种出口。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeWaponOutcome {
    /// 受理成功 ⇒ `~@upgradenow_ok`。
    Ok(UpgradeWaponResult),
    /// 已有同名记录（**精确 `==`**，区分大小写）⇒ `~@upgradenow_ing`。
    AlreadyUpgrading,
    /// 无武器 / 金币不足 / 无黑铁 ⇒ `~@upgradenow_fail`。
    Fail,
}

/// `Merchant.GetBackupgWeapon`（`Merchant.cs:592-759`）的四种出口。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum GetBackupgWeaponOutcome {
    /// `!IsEnoughBag()` ⇒ `~@getbackupgnow_bagfull`（**先查包、后找记录**）。
    BagFull,
    /// `nFlag == 0`（无记录）⇒ `~@getbackupgnow_fail`。
    NoRecord,
    /// `nFlag == 1`（记录存在但未到时限）⇒ `~@getbackupgnow_ing`。
    NotDue,
    /// 取回成功（已从列表移除；`user_item.desc[10]` 已写）。
    Ok(WeaponUpgradeInfo),
}

/// `M2Share.IsAccessoryMap`（`M2Share.cs:187/207`）：`StdMode ∈ {19,20,21,22,23,24,26}`。
#[must_use]
pub fn is_accessory_std_mode(std_mode: u8) -> bool {
    matches!(std_mode, 19..=24 | 26)
}

/// `GetStdItem(index)`（1-based；`index == 0`/越界 ⇒ `None`）。
fn std_item_at(catalog: &[StdItem], index: u16) -> Option<&StdItem> {
    if index == 0 {
        return None;
    }
    catalog.get(usize::from(index) - 1)
}

/// `GetStdItemName(index)`（`None` 对应 C# `null`）。
fn item_name_at(catalog: &[StdItem], index: u16) -> Option<&str> {
    std_item_at(catalog, index).map(|t| t.name.as_str())
}

/// `HUtil32.HiWord`（`ushort` 高字节）。
fn hi_word(v: u16) -> i32 {
    i32::from(v >> 8)
}

/// `HUtil32.LoWord`（`ushort` 低字节）。
fn lo_word(v: u16) -> i32 {
    i32::from(v & 0xFF)
}

/// `Merchant.UpgradeWaponAddValue`（`Merchant.cs:373-525`）——**零 RNG**。
///
/// 倒序扫描背包：吃掉**全部**黑铁矿（名 **精确 `==`** `Config.BlackStone`）与**全部**首饰
/// （`IsAccessory`）。首饰按 `GetUpgradeStdItem` 的恒等口径（T-24 残余：desc 升级差量未建模）
/// 取模板 `DC/SC/MC` 的 `HiWord+LoWord`（StdMode 24/26 再 +1），维护逐项最大/次大。
///
/// 收尾是 oracle 的**畸形「整体反转」置换**（不是排序）：`for i { for j=len-1..i+1 { if
/// dura[j]>dura[j-1] { duras.Reverse() } } }`；`Dura` 只累置换后的**前 5 项**。
fn upgrade_wapon_add_value(
    catalog: &[StdItem],
    item_list: &mut Vec<MerchantItem>,
    black_stone: &str,
    info: &mut WeaponUpgradeInfo,
) -> Vec<(String, i32)> {
    let mut deleted: Vec<(String, i32)> = Vec::new();
    let mut dura_list: Vec<f64> = Vec::new();
    let (mut dc_min, mut dc_max) = (0i32, 0i32);
    let (mut sc_min, mut sc_max) = (0i32, 0i32);
    let (mut mc_min, mut mc_max) = (0i32, 0i32);
    // `for (int i = itemList.Count - 1; i >= 0; i--)`
    let mut i = item_list.len();
    while i > 0 {
        i -= 1;
        let (index, make_index, dura) = {
            let it = &item_list[i];
            (it.index, it.make_index, it.dura)
        };
        // `GetStdItemName(userItem.Index) == Config.BlackStone`：C# `==` 是**区分大小写**的序数比较。
        if item_name_at(catalog, index) == Some(black_stone) {
            // `Math.Round(userItem.Dura / 1.0e3)`：缺省 `MidpointRounding.ToEven`（银行家舍入）。
            dura_list.push((f64::from(dura) / 1000.0).round_ties_even());
            deleted.push((black_stone.to_string(), make_index));
            item_list.remove(i);
            continue;
        }
        let Some(tpl) = std_item_at(catalog, index) else {
            continue;
        };
        if !is_accessory_std_mode(tpl.std_mode) {
            continue;
        }
        // `GetUpgradeStdItem(stdItem, userItem, ref clientItem)` 的恒等口径（T-24 残余）：
        // 直接用模板的 DC/SC/MC；`ClientItem.Item.StdMode` = 模板 StdMode。
        let (mut n_dc, mut n_sc, mut n_mc) = (0i32, 0i32, 0i32);
        match tpl.std_mode {
            19..=23 => {
                n_dc = hi_word(tpl.dc) + lo_word(tpl.dc);
                n_sc = hi_word(tpl.sc) + lo_word(tpl.sc);
                n_mc = hi_word(tpl.mc) + lo_word(tpl.mc);
            }
            24 | 26 => {
                n_dc = hi_word(tpl.dc) + lo_word(tpl.dc) + 1;
                n_sc = hi_word(tpl.sc) + lo_word(tpl.sc) + 1;
                n_mc = hi_word(tpl.mc) + lo_word(tpl.mc) + 1;
            }
            _ => {}
        }
        // `if (nMin < n) { nMax = nMin; nMin = n; } else if (nMax < n) { nMax = n; }`（逐项独立）。
        if dc_min < n_dc {
            dc_max = dc_min;
            dc_min = n_dc;
        } else if dc_max < n_dc {
            dc_max = n_dc;
        }
        if sc_min < n_sc {
            sc_max = sc_min;
            sc_min = n_sc;
        } else if sc_max < n_sc {
            sc_max = n_sc;
        }
        if mc_min < n_mc {
            mc_max = mc_min;
            mc_min = n_mc;
        } else if mc_max < n_mc {
            mc_max = n_mc;
        }
        deleted.push((tpl.name.clone(), make_index));
        item_list.remove(i);
    }
    // 畸形置换：`List.Reverse()` 是**整体反转**，不是排序。
    for i in 0..dura_list.len() {
        for j in (i + 1..dura_list.len()).rev() {
            if dura_list[j] > dura_list[j - 1] {
                dura_list.reverse();
            }
        }
    }
    let mut n_dura = 0i32;
    let mut n_item_count = 0i32;
    for d in &dura_list {
        n_dura += *d as i32; // C# `(int)double` 截断
        n_item_count += 1;
        if n_item_count >= 5 {
            break;
        }
    }
    // **复验 F6（已登记 T-95⑧）**：oracle 在 `nItemCount == 0` 时 `nDura / nItemCount`
    // 抛 `DivideByZeroException`。本入口前置 `CheckItems(BlackStone)`（`OrdinalIgnoreCase`）
    // 与这里的精确 `==` 名单**理论上**可错位（忽略大小写命中而精确不命中），但冻结内容
    // `BlackStone=黑铁矿` 使两者同集 ⇒ 不可达。`.max(1)` 是防御性偏离（oracle 抛异常、
    // 本侧给 0 耐久），登记而不照搬异常。
    let n_item_count = n_item_count.max(1);
    let min5 = n_item_count.min(5);
    info.dura = (f64::from(min5) + f64::from(min5) * (f64::from(n_dura / n_item_count) / 5.0))
        .round() as u8;
    info.dc = (dc_min / 5 + dc_max / 3) as u8;
    info.sc = (sc_min / 5 + sc_max / 3) as u8;
    info.mc = (mc_min / 5 + mc_max / 3) as u8;
    deleted
}

/// `Merchant.UpgradeWapon`（`Merchant.cs:527-586`）**第一阶段**——检查/扣费/拷贝，**零 RNG**。
///
/// **不改背包**：oracle 在 `AddValue` 之前先 `RecalcAbilitys()`（`:570`），此时黑铁/首饰仍在
/// `ItemList` 里 ⇒ 52 帧 `WAbil.Weight` 含材料重量。宿主必须在 [`UpgradeWaponBegin::Accepted`]
/// 之后、[`World::merchant_upgrade_wapon_finish`] 之前做重算。
#[allow(clippy::too_many_arguments)]
fn upgrade_wapon_begin_impl(
    catalog: &[StdItem],
    list: &[WeaponUpgradeInfo],
    chr_name: &str,
    weapon: Option<&MerchantItem>,
    bag: &[MerchantItem],
    gold: &mut i32,
    cfg: &UpgradeWeaponConfig,
    now_ms: i64,
) -> UpgradeWaponBegin {
    // 1. `upgradeInfo.UserName == user.ChrName`（C# `==`：**区分大小写**）。
    if list.iter().any(|u| u.user_name == chr_name) {
        return UpgradeWaponBegin::AlreadyUpgrading;
    }
    // 2. `UseItems[Weapon] != null && Index != 0 && Gold >= price && CheckItems(BlackStone) != null`。
    let Some(weapon) = weapon.filter(|w| w.index != 0) else {
        return UpgradeWaponBegin::Fail;
    };
    if *gold < cfg.price {
        return UpgradeWaponBegin::Fail;
    }
    // `CheckItems`：名 `OrdinalIgnoreCase` 命中（`BaseObject.cs:2189-2204`）。
    let has_black_stone = bag.iter().any(|it| {
        item_name_at(catalog, it.index)
            .is_some_and(|n| equals_ordinal_ignore_case(n, &cfg.black_stone))
    });
    if !has_black_stone {
        return UpgradeWaponBegin::Fail;
    }
    // 3. `DecGold(price)`（`GoldChanged` 的 653 由调用方出）。
    *gold -= cfg.price;
    // 4. `new UserItem(UseItems[Weapon])`（C# 的 Desc 是浅拷贝；本侧 clone；取回时不回写空槽）。
    UpgradeWaponBegin::Accepted(WeaponUpgradeInfo {
        user_name: chr_name.to_string(),
        user_item: weapon.clone(),
        dc: 0,
        sc: 0,
        mc: 0,
        dura: 0,
        upgrade_time_ms: now_ms,
        get_back_tick: now_ms,
    })
}

/// 一支（DC/MC/SC）的判定 + `desc[10]` 写入值（`Merchant.cs:677-733` 三支同形）。
///
/// `n10 = min(85, (n90 << ((3 - n90) & 31)) + 10 + Desc[3] - Desc[4] + BodyLuckLevel)`；
/// C# 的 `<<` 在 unchecked 上下文按 32 位回绕 ⇒ Rust `wrapping_shl`（`n90=6` 恒失败）。
#[allow(clippy::too_many_arguments)]
fn upgrade_one_stat(
    rng: &mut RandomNumber,
    value: u8,
    rate: i32,
    two_point_rate: i32,
    three_point_rate: i32,
    desc: &[u8; 14],
    body_luck_level: i32,
    base_desc10: u8,
) -> u8 {
    let n90 = i32::from(value.min(11));
    let shift = ((3 - n90) & 31) as u32;
    let n10 = (n90.wrapping_shl(shift) + 10 + i32::from(desc[3]) - i32::from(desc[4])
        + body_luck_level)
        .min(85);
    if rng.random_below(rate) < n10 {
        // **覆盖赋值**而非累加：oracle 是 `Desc[...] = 10; if(..) Desc[...] = 11;
        // if(..) Desc[...] = 12;`（`Merchant.cs:680-692` / `:703-715` / `:726-738`）——
        // 两点/三点都命中时结果是 **12**（三点覆盖两点），不是 13。累加是行为偏差（tier=R）。
        let mut d = base_desc10;
        if n10 > 63 && rng.random_below(two_point_rate) == 0 {
            d = base_desc10 + 1;
        }
        if n10 > 79 && rng.random_below(three_point_rate) == 0 {
            d = base_desc10 + 2;
        }
        d
    } else {
        1
    }
}

/// `Merchant.GetBackupgWeapon`（`Merchant.cs:592-759`）本体——RNG 密集。
///
/// 抽数顺序**严格** A（耐久）→ B（`Dc==Mc==Sc` 时的 `Random(3)` 空转）→ C/D/E（DC/MC/SC）。
#[allow(clippy::too_many_arguments)]
fn get_backupg_weapon_impl(
    rng: &mut RandomNumber,
    list: &mut Vec<WeaponUpgradeInfo>,
    chr_name: &str,
    permission: u8,
    now_ms: i64,
    bag_len: usize,
    body_luck_level: i32,
    cfg: &UpgradeWeaponConfig,
) -> GetBackupgWeaponOutcome {
    // 1. `if (!user.IsEnoughBag())`（`ItemList.Count < 46`）—— **先于**找记录。
    if bag_len >= 46 {
        return GetBackupgWeaponOutcome::BagFull;
    }
    // 2. `OrdinalIgnoreCase` 命中；未到时限**不 break**（继续找下一条同名记录）。
    let mut n_flag = 0u8;
    let mut info: Option<WeaponUpgradeInfo> = None;
    let mut i = 0usize;
    while i < list.len() {
        if equals_ordinal_ignore_case(&list[i].user_name, chr_name) {
            n_flag = 1;
            if (now_ms - list[i].get_back_tick) > cfg.get_back_time_ms || permission >= 4 {
                info = Some(list.remove(i));
                n_flag = 2;
                break;
            }
        }
        i += 1;
    }
    let Some(mut info) = info else {
        return if n_flag == 0 {
            GetBackupgWeaponOutcome::NoRecord
        } else {
            GetBackupgWeaponOutcome::NotDue
        };
    };
    // 3. 耐久分支（`upgradeInfo.Dura` = 提交时算出的 `btDura`）。
    let dura = info.dura;
    if dura <= 8 {
        if info.user_item.dura_max > 3000 {
            info.user_item.dura_max -= 3000;
        } else {
            info.user_item.dura_max >>= 1;
        }
        if info.user_item.dura > info.user_item.dura_max {
            info.user_item.dura = info.user_item.dura_max;
        }
    } else if matches!(dura, 9..=15) {
        if rng.random_below(i32::from(dura)) < 6 {
            if info.user_item.dura_max > 1000 {
                info.user_item.dura_max -= 1000;
            }
            if info.user_item.dura > info.user_item.dura_max {
                info.user_item.dura = info.user_item.dura_max;
            }
        }
    } else if dura >= 18 {
        // `Random(Dura - 18)`；`Dura == 18` ⇒ `Random(0)`（**仍消耗 1 抽**）。
        let r = rng.random_below(i32::from(dura) - 18);
        if matches!(r, 1..=4) {
            info.user_item.dura_max = info.user_item.dura_max.wrapping_add(1000);
        } else if matches!(r, 5..=7) {
            info.user_item.dura_max = info.user_item.dura_max.wrapping_add(2000);
        } else if matches!(r, 8..=255) {
            info.user_item.dura_max = info.user_item.dura_max.wrapping_add(4000);
        }
    }
    // Dura ∈ {16, 17}：三档都不命中，耐久不变、**不抽**。
    let (dc, mc, sc) = (info.dc, info.mc, info.sc);
    // B：`Dc==Mc==Sc` ⇒ `Random(3)`（**n1C 从不影响分支选择** ⇒ 纯空转抽）。
    let n1c = if dc == mc && mc == sc {
        rng.random_below(3)
    } else {
        -1
    };
    // C：DC 支（`||` 优先级低于 `&&`）。
    if (dc >= mc && dc >= sc) || n1c == 0 {
        info.user_item.desc[10] = upgrade_one_stat(
            rng,
            dc,
            cfg.dc_rate,
            cfg.dc_two_point_rate,
            cfg.dc_three_point_rate,
            &info.user_item.desc,
            body_luck_level,
            10,
        );
    }
    // D：MC 支。
    if (mc >= dc && mc >= sc) || n1c == 1 {
        info.user_item.desc[10] = upgrade_one_stat(
            rng,
            mc,
            cfg.mc_rate,
            cfg.mc_two_point_rate,
            cfg.mc_three_point_rate,
            &info.user_item.desc,
            body_luck_level,
            20,
        );
    }
    // E：SC 支——**照抄 oracle bug**：`n90 = min(11, upgradeInfo.Mc)`（`Merchant.cs:721`）。
    if (sc >= mc && sc >= dc) || n1c == 2 {
        info.user_item.desc[10] = upgrade_one_stat(
            rng,
            mc,
            cfg.sc_rate,
            cfg.sc_two_point_rate,
            cfg.sc_three_point_rate,
            &info.user_item.desc,
            body_luck_level,
            30,
        );
    }
    GetBackupgWeaponOutcome::Ok(info)
}

/// `Merchant.ClearExpreUpgradeListData`（`Merchant.cs:1076-1088`）：倒序删除
/// `(now - UpgradeTime).TotalDays >= ClearExpireUpgradeWeaponDays` 的记录（`>=` 边界）。
pub fn clear_expired_upgrades(list: &mut Vec<WeaponUpgradeInfo>, now_ms: i64, days: u8) {
    let expire_ms = i64::from(days) * 24 * 60 * 60 * 1000;
    let mut i = list.len();
    while i > 0 {
        i -= 1;
        if now_ms - list[i].upgrade_time_ms >= expire_ms {
            list.remove(i);
        }
    }
}

impl World {
    /// `Merchant.UpgradeWapon`（`Merchant.cs:527-586`）的宿主入口（**零 RNG**）。
    ///
    /// `weapon` = `UseItems[ItemLocation.Weapon]`（`None` = `null`；`index == 0` 由内部判 fail）。
    /// `bag`/`gold` 直接改（成功时扣费并吃掉材料）；`weapon` 槽的 `Index = 0` 由宿主执行
    /// （oracle 是「保留槽对象、只清 Index」⇒ 宿主解构后必须写回 `Some(BagItem{index:0, ..})`）。
    #[allow(clippy::too_many_arguments)]
    pub fn merchant_upgrade_wapon_begin(
        &mut self,
        merchant_id: i32,
        chr_name: &str,
        weapon: Option<&MerchantItem>,
        bag: &[MerchantItem],
        gold: &mut i32,
        cfg: &UpgradeWeaponConfig,
    ) -> UpgradeWaponBegin {
        let now = self.clock_ms;
        let catalog = std::mem::take(&mut self.item_catalog);
        let result = match self
            .entities
            .get_mut(merchant_id)
            .and_then(|e| e.merchant.as_mut())
        {
            Some(st) => upgrade_wapon_begin_impl(
                &catalog,
                &st.upgrade_weapon_list,
                chr_name,
                weapon,
                bag,
                gold,
                cfg,
                now,
            ),
            None => UpgradeWaponBegin::Fail,
        };
        self.item_catalog = catalog;
        result
    }

    /// `Merchant.UpgradeWapon` 的**第二阶段**：`UpgradeWaponAddValue`（吃材料、算 dc/sc/mc/dura）
    /// + `UpgradeWeaponList.Add`（`SaveUpgradingList` 是空桩）。
    ///
    /// 必须在宿主 `RecalcAbilitys` **之后**调用（oracle 顺序：`:570` Recalc → `:572` AddValue）。
    pub fn merchant_upgrade_wapon_finish(
        &mut self,
        merchant_id: i32,
        mut info: WeaponUpgradeInfo,
        bag: &mut Vec<MerchantItem>,
        cfg: &UpgradeWeaponConfig,
    ) -> UpgradeWaponResult {
        let catalog = std::mem::take(&mut self.item_catalog);
        let deleted = upgrade_wapon_add_value(&catalog, bag, &cfg.black_stone, &mut info);
        if let Some(st) = self
            .entities
            .get_mut(merchant_id)
            .and_then(|e| e.merchant.as_mut())
        {
            st.upgrade_weapon_list.push(info.clone());
        }
        self.item_catalog = catalog;
        UpgradeWaponResult { deleted, info }
    }

    /// 两步合一的便利入口（**只给 world 单测/无重算场景**；宿主生产路径必须分两步：
    /// `begin` → `RecalcAbilitys` → `finish`）。
    #[allow(clippy::too_many_arguments)]
    pub fn merchant_upgrade_wapon(
        &mut self,
        merchant_id: i32,
        chr_name: &str,
        weapon: Option<&MerchantItem>,
        bag: &mut Vec<MerchantItem>,
        gold: &mut i32,
        cfg: &UpgradeWeaponConfig,
    ) -> UpgradeWaponOutcome {
        match self.merchant_upgrade_wapon_begin(merchant_id, chr_name, weapon, bag, gold, cfg) {
            UpgradeWaponBegin::Accepted(info) => {
                let result = self.merchant_upgrade_wapon_finish(merchant_id, info, bag, cfg);
                UpgradeWaponOutcome::Ok(result)
            }
            UpgradeWaponBegin::AlreadyUpgrading => UpgradeWaponOutcome::AlreadyUpgrading,
            UpgradeWaponBegin::Fail => UpgradeWaponOutcome::Fail,
        }
    }

    /// `Merchant.GetBackupgWeapon`（`Merchant.cs:592-759`）的宿主入口。
    ///
    /// `bag_len` = `ItemList.Count`（包满门）；`permission` 生产恒 0（简报 §2.3）。
    /// 成功时记录已从列表移除、`user_item.desc[10]` 已写；**不改 `ItemList`**
    /// （照抄 `AddItemToBag` 被注释的 oracle 缺陷）。
    pub fn merchant_get_backupg_weapon(
        &mut self,
        merchant_id: i32,
        chr_name: &str,
        permission: u8,
        bag_len: usize,
        body_luck_level: i32,
        cfg: &UpgradeWeaponConfig,
    ) -> GetBackupgWeaponOutcome {
        let now = self.clock_ms;
        let World { entities, rng, .. } = self;
        match entities
            .get_mut(merchant_id)
            .and_then(|e| e.merchant.as_mut())
        {
            Some(st) => get_backupg_weapon_impl(
                rng,
                &mut st.upgrade_weapon_list,
                chr_name,
                permission,
                now,
                bag_len,
                body_luck_level,
                cfg,
            ),
            // 宿主已在 `handle_shop_cmd` 做 `FindMerchant` 门 ⇒ 此处不可达；按「无记录」兜底。
            None => GetBackupgWeaponOutcome::NoRecord,
        }
    }
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
    // `Merchant.cs:126-131`：清理门（`ClearExpreUpgradeListData`）。
    //
    // oracle 用墙钟 `DateTime.Now` 与 `WeaponUpgradeInfo.UpgradeTime` 比；本侧两者都取
    // **sim 时钟** `world.clock_ms`（提交时写入、清理时比较）⇒ 进程内等价（登记）。
    if (now - clear_expre_upgrade_tick) > MERCHANT_CLEAR_EXPRE_UPGRADE_INTERVAL_MS {
        let days = world
            .entities
            .get(actor_id)
            .and_then(|e| e.merchant.as_ref())
            .map_or(0, |st| st.clear_expire_upgrade_days);
        if let Some(st) = world
            .entities
            .get_mut(actor_id)
            .and_then(|e| e.merchant.as_mut())
        {
            st.clear_expre_upgrade_tick = now;
            clear_expired_upgrades(&mut st.upgrade_weapon_list, now, days);
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
    // `Merchant.cs:170` 末尾 `base.Run()`：先 SuperMan 回满（`BaseObject.Base.cs:32-36`），
    // 再 VerifyMapTime 段（`:223-228`）——刷新本格 `CellObject.AddTime`，防 60s 过期。
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.run_super_man_refill();
    }
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
    let cert_cell_type = crate::world::entity_cell_type(e);
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
            cert_cell_type,
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
    use crate::castle::{CastleSiegeConfig, UserCastle};
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

    /// reviewer-6 **M-2**：商人识别**必须走 [`Entity::merchant`] 身份**，不能走 race ——
    /// `monster_ai_tick` 的 `merchant.is_none()` 过滤是行为关键：商人不得被 `monster_run`
    /// 当怪跑（置 `target_x = -1` / 消耗 AI 取数）。删掉该守卫 ⇒ 必红。
    ///
    /// 2026-10-21（`openmir2-merchant-race-bias`）后商人 `race` 已从 50 改为 10，但
    /// `race > Play(0)` 仍为真 ⇒ 本守卫依旧被需要；把身份门换成 `race == 50` ⇒ 必红。
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
            "商人不得被 monster_run 当怪跑（身份门 merchant.is_none）"
        );
        assert_eq!(e.ai.walk_tick, 0, "商人不得被 AI 推进 walk_tick");
        let mut expect = rng_before;
        assert_eq!(w.rng.random(), expect.random(), "跳过商人 ⇒ 不消耗 AI 取数");
    }

    fn player_entity(id: i32, name: &str, x: i32, y: i32) -> Entity {
        let mut e = Entity::new(id, name, x, y);
        e.race = crate::entity::ACTOR_RACE_PLAY;
        e.hp = 100;
        e.max_hp = 100;
        e.dc = 0x0A_14; // LoByte=20、HiByte=10 ⇒ 攻击力 20（差为负 ⇒ 0..0）
        e.hit_point = 10;
        e.speed_point = 0;
        e
    }

    fn race_entity(id: i32, name: &str, x: i32, y: i32, race: u8) -> Entity {
        let mut e = Entity::new(id, name, x, y);
        e.race = race;
        e
    }

    /// `openmir2-merchant-race-bias`：oracle 商人**行为轴** `Race = ActorRace.NPC(10)`
    /// （`NormNpc.cs:42`；`Merchant` ctor 不覆写 `Race`），**外观轴**
    /// `RaceImg = ActorRace.Merchant(50)`（`Merchant.cs:84`）。`crates/world` 用独立
    /// [`Entity::race_img`] 承载外观轴，`race` 保持 10 供所有攻击/受击判定读取。
    ///
    /// 改坏必红：把 `insert_merchant` 的 `e.race` 改回 `ACTOR_RACE_MERCHANT(50)` ⇒ race 断言红；
    /// 删 `e.race_img = ACTOR_RACE_MERCHANT` ⇒ race_img 断言红。
    #[test]
    fn merchant_race_is_npc_and_race_img_is_merchant() {
        let mut w = World::new(20, 20);
        let id = w.spawn_merchant(spawn_at(3, 4)).expect("挂图成功");
        let e = w.entities.get(id).expect("实体在");
        assert_eq!(
            e.race,
            crate::entity::ACTOR_RACE_NPC,
            "行为轴 = NormNpc ctor 的 Race"
        );
        assert_eq!(e.race, 10);
        assert_eq!(
            e.race_img, ACTOR_RACE_MERCHANT,
            "外观轴 = Merchant ctor 的 RaceImg"
        );
        assert_eq!(e.race_img, 50);
        // 身份仍按 `merchant` 标记，不依赖 race；CellType 同源（`NormNpc.cs:50`）。
        assert!(e.merchant.is_some());
        assert_eq!(w.merchant_ids_sorted(), vec![id]);
        assert_eq!(crate::world::entity_cell_type(e), CellType::Merchant);
    }

    /// 判据 1（玩家攻击路径）：**race 窗模式**（HAM_ALL/PEACE/GROUP/GUILD/PKATTACK）下
    /// 商人不可被选中/击中，攻击不产生伤害与受击帧；`HAM_MASTER`/`HAM_DEAR` 对非玩家目标
    /// 恒真（`PlayObject.Attack.cs:689-698`），属 oracle 既有分支，不在本判据范围。
    ///
    /// 走生产结算入口 [`World::attack_once`]（`Command::Attack` 的唯一结算点，玩家/怪物共用；
    /// 等价 oracle `_Attack` + `AttackSuccess`）。受击帧 = `RM_REFMESSAGE` 到点后的
    /// `SM_STRUCK` 广播（`pending_refmessage_frames`）。
    ///
    /// 改坏必红：把 `insert_merchant` 的 `e.race` 改回 50 ⇒ `HAM_ALL` 变可攻击、
    /// `damage > 0`、受击帧非空。
    #[test]
    fn player_cannot_target_or_damage_merchant() {
        use crate::combat::{
            is_attack_target_base, is_attack_target_player_grouped, is_proper_target_grouped,
            ATTACK_MODE_HAM_ALL, ATTACK_MODE_HAM_GROUP, ATTACK_MODE_HAM_GUILD,
            ATTACK_MODE_HAM_PEACE, ATTACK_MODE_HAM_PKATTACK,
        };

        let mut w = World::new(32, 32);
        w.enable_refmessage_delay();
        w.clock_ms = 1_000;
        let mid = w.spawn_merchant(spawn_at(5, 5)).expect("挂图成功");
        let pid = w.add_entity(player_entity(0, "hero", 6, 5));
        let player = w.entities.get(pid).cloned().expect("玩家在");
        let merchant = w.entities.get(mid).cloned().expect("商人在");

        // 玩家攻方：所有按 race 窗口分派的攻击模式都把 race 10 判成**不可攻击**
        // （`HAM_DEAR` / `HAM_MASTER` 对「非 dear / 非师徒」恒真，不属 race 窗口，故不列）。
        for mode in [
            ATTACK_MODE_HAM_ALL,
            ATTACK_MODE_HAM_PEACE,
            ATTACK_MODE_HAM_GROUP,
            ATTACK_MODE_HAM_GUILD,
            ATTACK_MODE_HAM_PKATTACK,
        ] {
            let mut p = player.clone();
            p.attack_mode = mode;
            assert!(
                !is_attack_target_player_grouped(&p, &merchant, &[]),
                "攻击模式 {mode} 不得把商人当目标"
            );
        }
        let mut p = player.clone();
        p.attack_mode = ATTACK_MODE_HAM_ALL;
        assert!(!is_proper_target_grouped(&p, &merchant, &[]));

        // 基版目标规则（怪物/NPC 攻方）：普通怪不攻击商人；`NastyMode` 的 race 窗
        // `[NPC(10), PeaceNpc(15)]` 也排除 race 10（race 50 时会被误判为可攻击）。
        let mut plain_mon = race_entity(0, "plain", 7, 5, crate::entity::ACTOR_RACE_MONSTER);
        plain_mon.nasty_mode = false;
        assert!(!is_attack_target_base(&plain_mon, &merchant));
        let mut nasty_mon = plain_mon.clone();
        nasty_mon.nasty_mode = true;
        assert!(
            !is_attack_target_base(&nasty_mon, &merchant),
            "NastyMode 的 race 窗排除商人"
        );

        // 生产结算入口：不命中、不掉血、不排 `SendStruckDelayMsg`（受击帧门）。
        let hp_before = merchant.hp;
        let outcome = w.attack_once(pid, mid).expect("攻击应结算");
        assert!(!outcome.hit, "不可攻击目标不得命中");
        assert_eq!(outcome.damage, 0, "商人不得掉血");
        assert_eq!(outcome.struck_power, 0, "不得入队 RM_REFMESSAGE");
        assert_eq!(w.entities.get(mid).unwrap().hp, hp_before);
        assert!(
            w.entities.get(mid).unwrap().merchant.is_some(),
            "商人仍在 merchant 身份位（不得从列表消失）"
        );
        // 到点投递：没有 RM_REFMESSAGE ⇒ 没有任何受击帧。
        w.clock_ms += 1_000;
        w.deliver_self_delay_msgs();
        assert!(
            w.take_refmessage_frames().is_empty(),
            "对商人攻击不得产生受击帧"
        );
    }

    /// 判据 2（回归）：race 50 的目标**仍可被玩家攻击**，伤害与受击帧路径不变。
    ///
    /// 覆盖 oracle 的「动物 race 50」语义（`ActorRace.Animal = Merchant = 50`，`Race.cs`）；
    /// 商人 race 改成 10 不得把 race 50 一并排除。
    ///
    /// 改坏必红：把 `is_attack_target_player_grouped` 的 `HAM_ALL` 上界从
    /// `ACTOR_RACE_PEACE_NPC(15)` 放宽到 `ACTOR_RACE_ANIMAL(50)` ⇒ race 50 被判不可攻击 ⇒ 红。
    #[test]
    fn race_50_stays_attackable_for_players() {
        use crate::combat::{is_attack_target_player_grouped, ATTACK_MODE_HAM_ALL};
        use crate::entity::ACTOR_RACE_ANIMAL;

        let mut w = World::new(32, 32);
        w.enable_refmessage_delay();
        w.clock_ms = 1_000;
        let pid = w.add_entity(player_entity(0, "hero", 6, 5));
        let aid = w.add_entity(race_entity(0, "animal", 5, 5, ACTOR_RACE_ANIMAL));
        let player = w.entities.get(pid).cloned().expect("玩家在");
        let animal = w.entities.get(aid).cloned().expect("动物在");
        let mut p = player.clone();
        p.attack_mode = ATTACK_MODE_HAM_ALL;
        assert!(
            is_attack_target_player_grouped(&p, &animal, &[]),
            "race 50 仍属可攻击集"
        );
        let hp_before = animal.hp;
        let outcome = w.attack_once(pid, aid).expect("攻击应结算");
        assert!(outcome.hit, "race 50 目标应命中");
        assert!(outcome.damage > 0, "race 50 目标应掉血");
        assert_eq!(
            w.entities.get(aid).unwrap().hp,
            hp_before - outcome.damage,
            "伤害路径不变（HP 扣减与结算一致）"
        );
        w.clock_ms += 1_000;
        w.deliver_self_delay_msgs();
        assert!(
            !w.take_refmessage_frames().is_empty(),
            "race 50 受击仍产生受击帧"
        );
    }

    /// 判据 3（身份面）：race 从 50→10 不得让商人从 `merchant_ids` / `ProcessMerchants`
    /// 列表消失；tick 与 CellType / 视野更新门都按身份（`merchant.is_some()`）判定。
    ///
    /// 改坏必红：把 `merchant_ids_sorted` 的 `e.merchant.is_some()` 换成
    /// `e.race == ACTOR_RACE_MERCHANT` ⇒ 商人列表为空、tick 不跑 ⇒ 红。
    #[test]
    fn merchant_tick_and_identity_do_not_depend_on_race() {
        let mut w = World::new(20, 20);
        w.clock_ms = 10_000;
        let id = w.spawn_merchant(spawn_at(2, 2)).expect("挂图成功");
        // 令 `RunTick` 早已到点（`ProcessMerchants` 的 `now - RunTick > RunTime` 门）。
        if let Some(st) = w.entities.get_mut(id).and_then(|e| e.merchant.as_mut()) {
            st.run_tick = 0;
        }
        w.clock_ms += MERCHANT_RUN_TIME + 1;
        let out = w.merchant_tick();
        assert_eq!(w.merchant_ids_sorted(), vec![id], "身份列表不依赖 race");
        assert!(out.ran.contains(&id), "商人仍由 ProcessMerchants 驱动");
        let e = w.entities.get(id).unwrap();
        assert_eq!(crate::world::entity_cell_type(e), CellType::Merchant);
        assert!(
            !crate::monai::consumes_view_range_update(e),
            "商人仍不消费 RM_UPDATEVIEWRANGE"
        );
        assert!(e.merchant.is_some(), "点击/对话的宿主身份位仍为真");
        assert_eq!(e.race, crate::entity::ACTOR_RACE_NPC);
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
    // ───────────────────── 武器升级 NPC 判据（C1-C7/C9/C10，简报 §4 + 更正 A1-A9） ─────────────────────

    fn upgrade_catalog() -> Vec<StdItem> {
        vec![
            // Index 1：黑铁矿（StdMode 43，DuraMax 5000，`sql/mir2_data.sql:1058`）。
            StdItem {
                name: "黑铁矿".to_string(),
                std_mode: 43,
                dura_max: 5000,
                ..StdItem::default()
            },
            // Index 2：首饰 StdMode 22（`DC = MakeWord(1, 2)` ⇒ Hi+Lo = 3）。
            StdItem {
                name: "项链".to_string(),
                std_mode: 22,
                dc: 0x0201,
                ..StdItem::default()
            },
            // Index 3：首饰 StdMode 24（`DC = MakeWord(2, 1)` ⇒ 3 + 1 = 4；`SC = 0x0200` ⇒ 2 + 1 = 3）。
            StdItem {
                name: "戒指".to_string(),
                std_mode: 24,
                dc: 0x0102,
                sc: 0x0200,
                ..StdItem::default()
            },
            // Index 4/5：武器。
            StdItem {
                name: "炼狱".to_string(),
                std_mode: 5,
                dura_max: 20000,
                ..StdItem::default()
            },
            StdItem {
                name: "屠龙".to_string(),
                std_mode: 5,
                dura_max: 20000,
                ..StdItem::default()
            },
        ]
    }

    fn upgrade_world(seed: i32) -> (World, i32) {
        let mut w = World::new(32, 32);
        w.rng = RandomNumber::with_seed(seed);
        w.item_catalog = upgrade_catalog();
        w.clock_ms = 1_000;
        let mid = w
            .spawn_merchant(MerchantSpawn {
                name: "铁匠".to_string(),
                x: 5,
                y: 5,
                map_id: 0,
                state: MerchantState::new(
                    "铁匠".to_string(),
                    "3".to_string(),
                    0,
                    0,
                    100,
                    false,
                    false,
                    0,
                    MerchantFlags {
                        is_upgradenow: true,
                        is_get_backupgnow: true,
                        ..MerchantFlags::default()
                    },
                    Vec::new(),
                    Vec::new(),
                    0,
                ),
            })
            .expect("挂图成功");
        // 构造抽（4 抽）已发生；重播种让取回路径的流水从已知起点开始（用例镜像用）。
        w.rng = RandomNumber::with_seed(seed);
        (w, mid)
    }

    /// 部署值（`Server.conf`）：取回时限 600000ms、三率 50/30/200。
    fn deploy_cfg() -> UpgradeWeaponConfig {
        UpgradeWeaponConfig {
            get_back_time_ms: 600_000,
            dc_rate: 50,
            mc_rate: 50,
            sc_rate: 50,
            ..UpgradeWeaponConfig::default()
        }
    }

    fn weapon_at(index: u16) -> MerchantItem {
        MerchantItem {
            make_index: 900 + i32::from(index),
            index,
            dura: 5000,
            dura_max: 20000,
            desc: [0; 14],
        }
    }

    fn black_stone(dura: u16, make_index: i32) -> MerchantItem {
        MerchantItem {
            make_index,
            index: 1,
            dura,
            dura_max: 5000,
            desc: [0; 14],
        }
    }

    fn accessory(index: u16, make_index: i32) -> MerchantItem {
        MerchantItem {
            make_index,
            index,
            dura: 1000,
            dura_max: 1000,
            desc: [0; 14],
        }
    }

    fn info_with(dc: u8, mc: u8, sc: u8, dura: u8, get_back_tick: i64) -> WeaponUpgradeInfo {
        WeaponUpgradeInfo {
            user_name: "hero".to_string(),
            user_item: MerchantItem {
                make_index: 77,
                index: 4,
                dura: 5000,
                dura_max: 20000,
                desc: [0; 14],
            },
            dc,
            sc,
            mc,
            dura,
            upgrade_time_ms: get_back_tick,
            get_back_tick,
        }
    }

    fn push_info(w: &mut World, mid: i32, info: WeaponUpgradeInfo) {
        w.entities
            .get_mut(mid)
            .and_then(|e| e.merchant.as_mut())
            .expect("商人在")
            .upgrade_weapon_list
            .push(info);
    }

    fn list_len(w: &World, mid: i32) -> usize {
        w.entities
            .get(mid)
            .and_then(|e| e.merchant.as_ref())
            .expect("商人在")
            .upgrade_weapon_list
            .len()
    }

    /// **C1**：提交成功——扣 10000、吃掉全部黑铁/首饰、`dc/sc/mc/dura` 手算值、
    /// `info.user_item` 四字段拷贝、`get_back_tick == now`、列表 +1。
    #[test]
    fn c1_upgrade_wapon_success_state_and_fee() {
        let (mut w, mid) = upgrade_world(1);
        let weapon = weapon_at(4);
        let mut bag = vec![
            black_stone(5000, 10),
            black_stone(5000, 11),
            black_stone(5000, 12),
            black_stone(5000, 13),
            black_stone(5000, 14),
            accessory(2, 20),
            accessory(3, 21),
        ];
        let mut gold = 20_000;
        let r = w.merchant_upgrade_wapon(
            mid,
            "hero",
            Some(&weapon),
            &mut bag,
            &mut gold,
            &UpgradeWeaponConfig::default(),
        );
        let UpgradeWaponOutcome::Ok(res) = r else {
            panic!("应受理成功：{r:?}");
        };
        assert_eq!(gold, 10_000, "扣 UpgradeWeaponPrice");
        assert!(bag.is_empty(), "全部黑铁与首饰被吃");
        // 倒序扫描：戒指(21) → 项链(20) → 黑铁(14..10)。
        assert_eq!(res.deleted.len(), 7);
        assert_eq!(res.deleted[0], ("戒指".to_string(), 21));
        assert_eq!(res.deleted[1], ("项链".to_string(), 20));
        let blacks: Vec<(String, i32)> =
            (10..=14).rev().map(|m| ("黑铁矿".to_string(), m)).collect();
        assert_eq!(&res.deleted[2..], &blacks[..], "黑铁按背包倒序");
        // 手算：`Dc = nDcMin/5 + nDcMax/3 = 4/5 + 3/3 = 1`；`Sc = 3/5 + 0/3 = 0`；
        // 5×黑铁 5000 ⇒ 每项 5，`Dura = round(5 + 5*(25/5/5.0)) = 10`。
        let info = &res.info;
        assert_eq!((info.dc, info.sc, info.mc, info.dura), (1, 0, 0, 10));
        assert_eq!(info.user_item, weapon, "UserItem 四字段拷贝");
        assert_eq!(info.user_name, "hero");
        assert_eq!(info.get_back_tick, w.clock_ms);
        assert_eq!(info.upgrade_time_ms, w.clock_ms);
        assert_eq!(list_len(&w, mid), 1, "UpgradeWeaponList +1");
        assert_eq!(
            &w.entities
                .get(mid)
                .and_then(|e| e.merchant.as_ref())
                .unwrap()
                .upgrade_weapon_list[0],
            info
        );
    }

    /// **C2**：6 件黑铁 [3,1,4,1,5,9]×1000 —— oracle 的**畸形整体反转置换**（不是排序）
    /// ⇒ `nDura=14` / `n=5` / `Dura=7`；真降序排序会得 9。全部 6 件都被吃。
    #[test]
    fn c2_black_stone_count_and_malformed_reverse() {
        let (mut w, mid) = upgrade_world(2);
        let weapon = weapon_at(4);
        let mut bag: Vec<MerchantItem> = [3u16, 1, 4, 1, 5, 9]
            .into_iter()
            .enumerate()
            .map(|(i, k)| black_stone(k * 1000, 30 + i as i32))
            .collect();
        let mut gold = 10_000;
        let UpgradeWaponOutcome::Ok(res) = w.merchant_upgrade_wapon(
            mid,
            "hero",
            Some(&weapon),
            &mut bag,
            &mut gold,
            &UpgradeWeaponConfig::default(),
        ) else {
            panic!("应受理成功");
        };
        assert_eq!(
            res.info.dura, 7,
            "畸形置换后前 5 项和 14 ⇒ 7（真排序会得 9）"
        );
        assert_eq!(res.deleted.len(), 6, "全部黑铁被吃（不只 5 件）");
        assert!(bag.is_empty());
    }

    /// **C3**：提交门控顺序——
    /// ① 同名检查**区分大小写**（A1：大小写不同 ⇒ **继续升级并再加一条**）；
    /// ② 武器 `Index == 0` ⇒ fail；③ 金币不足 ⇒ fail 且金币/材料不动；
    /// ④ 金币够但无黑铁 ⇒ fail 且金币不动。
    #[test]
    fn c3_upgrade_gate_order_and_case_sensitive_duplicate() {
        let (mut w, mid) = upgrade_world(3);
        let weapon = weapon_at(4);
        let cfg = UpgradeWeaponConfig::default();
        // ① 记录 "HERO"（大写）已在表 ⇒ 用 "hero" 提交**不是** `_ing`（A1）。
        push_info(&mut w, mid, info_with(1, 0, 0, 16, 0));
        w.entities
            .get_mut(mid)
            .and_then(|e| e.merchant.as_mut())
            .unwrap()
            .upgrade_weapon_list[0]
            .user_name = "HERO".to_string();
        let mut bag = vec![black_stone(5000, 1)];
        let mut gold = 10_000;
        let r = w.merchant_upgrade_wapon(mid, "hero", Some(&weapon), &mut bag, &mut gold, &cfg);
        assert!(
            matches!(r, UpgradeWaponOutcome::Ok(_)),
            "大小写不同 ⇒ 继续升级"
        );
        assert_eq!(list_len(&w, mid), 2, "再加一条记录（A1）");
        assert_eq!(gold, 0, "扣费发生");
        // 完全同名（精确 ==）⇒ `_ing`，且一切不动。
        let mut bag2 = vec![black_stone(5000, 2)];
        let mut gold2 = 10_000;
        let r = w.merchant_upgrade_wapon(mid, "hero", Some(&weapon), &mut bag2, &mut gold2, &cfg);
        assert!(matches!(r, UpgradeWaponOutcome::AlreadyUpgrading));
        assert_eq!(gold2, 10_000, "重复检查先于扣费");
        assert_eq!(bag2.len(), 1, "重复检查先于吃材料");
        // ② 武器 Index == 0 ⇒ fail。
        let mut zero = weapon.clone();
        zero.index = 0;
        let mut gold3 = 10_000;
        let mut bag3 = vec![black_stone(5000, 3)];
        assert!(matches!(
            w.merchant_upgrade_wapon(mid, "p2", Some(&zero), &mut bag3, &mut gold3, &cfg),
            UpgradeWaponOutcome::Fail
        ));
        // ③ gold = 9999 ⇒ fail，金币/材料不动。
        let mut gold4 = 9_999;
        let mut bag4 = vec![black_stone(5000, 4)];
        assert!(matches!(
            w.merchant_upgrade_wapon(mid, "p3", Some(&weapon), &mut bag4, &mut gold4, &cfg),
            UpgradeWaponOutcome::Fail
        ));
        assert_eq!((gold4, bag4.len()), (9_999, 1), "金币不足不扣、不吃材料");
        // ④ gold 够但无黑铁 ⇒ fail，金币不动。
        let mut gold5 = 10_000;
        let mut bag5 = vec![accessory(2, 5)];
        assert!(matches!(
            w.merchant_upgrade_wapon(mid, "p4", Some(&weapon), &mut bag5, &mut gold5, &cfg),
            UpgradeWaponOutcome::Fail
        ));
        assert_eq!((gold5, bag5.len()), (10_000, 1), "无黑铁不扣金");
    }

    /// **C4**：取回时限三态 + 包满；`>` 严格边界；包满门**先于**找记录；A1 的
    /// 「OrdinalIgnoreCase 找到不 break」可取到后一条。
    #[test]
    fn c4_get_back_time_limit_and_bag_full() {
        let (mut w, mid) = upgrade_world(4);
        let cfg = deploy_cfg();
        push_info(&mut w, mid, info_with(1, 0, 0, 16, 0));
        // now - tick == 600000 ⇒ 不 > ⇒ `_ing`。
        w.clock_ms = 600_000;
        assert!(matches!(
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &cfg),
            GetBackupgWeaponOutcome::NotDue
        ));
        assert_eq!(list_len(&w, mid), 1, "未到期不移除");
        // 600001 ⇒ ok 且移除。
        w.clock_ms = 600_001;
        assert!(matches!(
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &cfg),
            GetBackupgWeaponOutcome::Ok(_)
        ));
        assert_eq!(list_len(&w, mid), 0);
        // 无记录 ⇒ fail。
        assert!(matches!(
            w.merchant_get_backupg_weapon(mid, "nobody", 0, 0, 0, &cfg),
            GetBackupgWeaponOutcome::NoRecord
        ));
        // 包满 46 ⇒ bagfull，记录仍在（门先于找记录）。
        push_info(&mut w, mid, info_with(1, 0, 0, 16, 0));
        assert!(matches!(
            w.merchant_get_backupg_weapon(mid, "hero", 0, 46, 0, &cfg),
            GetBackupgWeaponOutcome::BagFull
        ));
        assert_eq!(list_len(&w, mid), 1, "包满不删记录");
        // A1：单条记录名 "HERO"、请求名 "hero" ⇒ `OrdinalIgnoreCase` 命中并取回。
        let mut w2 = w;
        {
            let list = w2
                .entities
                .get_mut(mid)
                .and_then(|e| e.merchant.as_mut())
                .unwrap();
            list.upgrade_weapon_list.clear();
            list.upgrade_weapon_list.push({
                let mut i = info_with(9, 0, 0, 16, 0);
                i.user_name = "HERO".to_string();
                i
            });
        }
        let r = w2.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &cfg);
        assert!(
            matches!(r, GetBackupgWeaponOutcome::Ok(_)),
            "OrdinalIgnoreCase 大小写不同也命中：{r:?}"
        );
        // A1：第一条 "HERO" 未到期 ⇒ **不 break**，第二条 "hero" 到期 ⇒ 取走后一条。
        let list = w2
            .entities
            .get_mut(mid)
            .and_then(|e| e.merchant.as_mut())
            .unwrap();
        list.upgrade_weapon_list.clear();
        list.upgrade_weapon_list.push({
            let mut i = info_with(1, 0, 0, 16, w2.clock_ms);
            i.user_name = "HERO".to_string();
            i
        });
        list.upgrade_weapon_list.push(info_with(2, 0, 0, 16, 0));
        let r = w2.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &cfg);
        let GetBackupgWeaponOutcome::Ok(info) = r else {
            panic!("应取到后一条：{r:?}");
        };
        assert_eq!(info.dc, 2, "取到的是第二条（未 break 语义）");
        assert_eq!(list_len(&w2, mid), 1, "第一条（未到期）保留");
    }

    /// **C5**：耐久分支的抽数与边界——
    /// `[0,8]` 不抽、`[9,15]` 抽 `Random(Dura)`、`[18,255]` 抽 `Random(Dura-18)`
    /// （`Dura==18 ⇒ Random(0)` **仍消耗 1 抽**）、`{16,17}` 不改耐久且**不抽**。
    ///
    /// 用同一流的下一次 `random()` 与镜像流比较（抽数错 ⇒ 后续流偏移）。
    #[test]
    fn c5_durability_branch_draws_and_boundaries() {
        // (dura, dura_max, expected_dura_max after, durability_draw: Option<i32>)
        // (dura, dura_max, 期望 DuraMax, 期望 Dura, 耐久分支抽: Option<upper>)
        let cases = [
            (5u8, 5000u16, 2000u16, 2000u16, None),
            (8, 2000, 1000, 1000, None),
            (16, 5000, 5000, 5000, None),
            (17, 5000, 5000, 5000, None),
            (18, 5000, 5000, 5000, Some(0)),
        ];
        for (dura, dura_max, exp_max, exp_dura, draw) in cases {
            let (mut w, mid) = upgrade_world(77);
            let mut info = info_with(1, 0, 0, dura, 0);
            info.user_item.dura_max = dura_max;
            info.user_item.dura = dura_max;
            push_info(&mut w, mid, info);
            w.clock_ms = 600_001;
            let mut exp = RandomNumber::with_seed(77);
            if let Some(v) = draw {
                assert_eq!(exp.random_below(v), 0, "Random(0)/Random(0) 恒 0");
            }
            // 镜像 DC/MC/SC 三支（dc=1,mc=sc=0 ⇒ 仅 DC 支成立；无 n1C）。
            let _ = exp.random_below(50);
            let r = w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg());
            let GetBackupgWeaponOutcome::Ok(out) = r else {
                panic!("应取回：{r:?}");
            };
            assert_eq!(out.user_item.dura_max, exp_max, "Dura={dura} 的 DuraMax");
            assert_eq!(out.user_item.dura, exp_dura, "Dura={dura} 的 Dura（clamp）");
            assert_eq!(w.rng.random(), exp.random(), "Dura={dura} 后续流同源");
        }
        // Dura=12：Random(12) < 6 ⇒ DuraMax -= 1000（找不到种子则跳过——但种子空间足够大，
        // 这里显式搜索一个 `< 6` 的种子，并把 `>= 6` 的对照也跑一遍）。
        let seed_lt = (1..=1000)
            .find(|&s| {
                let mut r = RandomNumber::with_seed(s);
                r.random_below(12) < 6
            })
            .expect("应有 Random(12)<6 的种子");
        for seed in [seed_lt, seed_lt + 1] {
            let (mut w, mid) = upgrade_world(seed);
            let mut info = info_with(1, 0, 0, 12, 0);
            info.user_item.dura_max = 5000;
            info.user_item.dura = 5000;
            push_info(&mut w, mid, info);
            w.clock_ms = 600_001;
            let mut exp = RandomNumber::with_seed(seed);
            let roll = exp.random_below(12);
            let exp_max = if roll < 6 { 4000 } else { 5000 };
            let _ = exp.random_below(50);
            let GetBackupgWeaponOutcome::Ok(out) =
                w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
            else {
                panic!("应取回");
            };
            assert_eq!(out.user_item.dura_max, exp_max, "Dura=12 / roll={roll}");
            assert_eq!(w.rng.random(), exp.random(), "后续流同源");
            if roll < 6 {
                break;
            }
        }
    }

    /// **C6**：升级判定 RNG 顺序（核心）——
    /// 例 A `Dc=Mc=Sc=0`：`Random(3)`（空转）+ DC/MC/SC 各 `Random(50)`；
    /// 例 B `Dc=7`：DC 支 `Random(50)` 必成 + `Random(30)` + `Random(200)`；
    /// 例 C `Dc=6`：n10 为负 ⇒ `Desc[10]=1`（仍抽 `Random(50)`）。
    ///
    /// 用例都用 `Dura=16`（耐久分支**不抽**）以隔离 A7 的分支。
    #[test]
    fn c6_upgrade_decision_rng_order() {
        const RATE: i32 = 50;
        // 例 A：三值全等 ⇒ 4 抽；最后一支（SC）赢。
        let seed = 4242;
        let (mut w, mid) = upgrade_world(seed);
        push_info(&mut w, mid, info_with(0, 0, 0, 16, 0));
        w.clock_ms = 600_001;
        let mut exp = RandomNumber::with_seed(seed);
        let _n1c = exp.random_below(3);
        let mut last = 0u8;
        for base in [10u8, 20, 30] {
            let success = exp.random_below(RATE) < 10;
            last = if success { base } else { 1 };
        }
        let GetBackupgWeaponOutcome::Ok(out) =
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
        else {
            panic!("应取回");
        };
        assert_eq!(out.user_item.desc[10], last, "三支全成 ⇒ 最后一支（SC）赢");
        assert_eq!(w.rng.random(), exp.random(), "例 A 恰 4 抽");
        // 例 B：Dc=7 / Mc=Sc=0 ⇒ 3 抽（无 n1C）。
        let seed = 7;
        let (mut w, mid) = upgrade_world(seed);
        push_info(&mut w, mid, info_with(7, 0, 0, 16, 0));
        w.clock_ms = 600_001;
        let mut exp = RandomNumber::with_seed(seed);
        assert!(exp.random_below(RATE) < 85, "n90=7 ⇒ n10=85 必成");
        let _two = exp.random_below(30);
        let _three = exp.random_below(200);
        let GetBackupgWeaponOutcome::Ok(out) =
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
        else {
            panic!("应取回");
        };
        assert!(
            matches!(out.user_item.desc[10], 10..=12),
            "成功支 Desc[10] ∈ {{10,11,12}}，实际 {}",
            out.user_item.desc[10]
        );
        assert_eq!(w.rng.random(), exp.random(), "例 B 恰 3 抽");
        // 例 C：Dc=6 ⇒ n10 为负（`6 << 29` 回绕）⇒ 必败，仍抽 1 次 Random(50)。
        let seed = 6;
        let (mut w, mid) = upgrade_world(seed);
        push_info(&mut w, mid, info_with(6, 0, 0, 16, 0));
        w.clock_ms = 600_001;
        let mut exp = RandomNumber::with_seed(seed);
        let _ = exp.random_below(RATE);
        let GetBackupgWeaponOutcome::Ok(out) =
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
        else {
            panic!("应取回");
        };
        assert_eq!(out.user_item.desc[10], 1, "n90=6 恒失败");
        assert_eq!(w.rng.random(), exp.random(), "例 C 恰 1 抽");
    }

    /// **C7**：SC 支**照抄 oracle bug**（`n90 = min(11, upgradeInfo.Mc)`，`Merchant.cs:721`）。
    ///
    /// `Dc=0, Mc=3, Sc=6`：只有 SC 支成立；oracle 取 `Mc=3` ⇒ `n10 = 4 + 10 - 1 = 13`
    /// （`3 << 0 = 3`，descluck 0）⇒ 成功时 `Desc[10]=30`（**不会出现 31/32**，A2）。
    /// 修正成 `Sc=6` ⇒ `n90=6` ⇒ n10 负 ⇒ 恒失败（1）。
    #[test]
    fn c7_sc_branch_uses_mc_like_oracle() {
        let seed = (1..=10_000)
            .find(|&s| {
                let mut r = RandomNumber::with_seed(s);
                r.random_below(50) < 13
            })
            .expect("应能找到 Random(50)<13 的种子");
        let (mut w, mid) = upgrade_world(seed);
        push_info(&mut w, mid, info_with(0, 3, 6, 16, 0));
        w.clock_ms = 600_001;
        let GetBackupgWeaponOutcome::Ok(out) =
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
        else {
            panic!("应取回");
        };
        assert_eq!(
            out.user_item.desc[10], 30,
            "oracle 的 SC 支用 Mc ⇒ n10=13 ⇒ 30"
        );
        // 修正版（用 Sc）对照：同一流位置恒失败。
        let mut rng = RandomNumber::with_seed(seed);
        assert_eq!(
            upgrade_one_stat(&mut rng, 6, 50, 30, 200, &[0; 14], 0, 30),
            1,
            "若把 SC 的 Mc 改成 Sc ⇒ n90=6 ⇒ 恒失败"
        );
    }

    /// **C8**：两点/三点判定是**覆盖赋值**，不是累加（`Merchant.cs:680-692`/`:703-715`/`:726-738`）。
    ///
    /// oracle 逐行：`Desc[..] = 10; if(n10>63 && Random(Two)==0) Desc[..] = 11;
    /// if(n10>79 && Random(Three)==0) Desc[..] = 12;` ⇒ 两点、三点**都命中**时结果是 **12**
    /// （三点覆盖两点），不是 13；MC/SC 支同形 ⇒ 32，不是 33。
    ///
    /// 种子 16271：`n90=7 ⇒ n10=85`（必进两个判定），本种子两判都命中 ⇒ 旧实现（`d += 1; d += 2;`）
    /// 返回 13/33，oracle 返回 12/32。
    /// 牙：把 `d = base_desc10 + 2;` 改回 `d += 2;` ⇒ 本用例红（13 ≠ 12）。
    #[test]
    fn c8_two_and_three_point_assign_not_accumulate() {
        let seed = 16271;
        // 先用独立重放确认本种子「两判都命中」——否则累加与覆盖同值，用例无牙。
        let mut r = RandomNumber::with_seed(seed);
        assert!(r.random_below(50) < 85, "n90=7 ⇒ n10=85 必成功");
        assert_eq!(r.random_below(30), 0, "两点判定命中（本种子的牙所在）");
        assert_eq!(r.random_below(200), 0, "三点判定命中");

        for (base, want) in [(10u8, 12u8), (20, 22), (30, 32)] {
            let mut rng = RandomNumber::with_seed(seed);
            assert_eq!(
                upgrade_one_stat(&mut rng, 7, 50, 30, 200, &[0; 14], 0, base),
                want,
                "base={base}：三点覆盖两点 ⇒ {want}（累加会给 {}）",
                base + 3
            );
        }

        // 生产路径同断言（不是只测 helper）：Dc=7 ⇒ DC 支必成，desc[10] 必须是 12。
        let (mut w, mid) = upgrade_world(seed);
        push_info(&mut w, mid, info_with(7, 0, 0, 16, 0));
        w.clock_ms = 600_001;
        let GetBackupgWeaponOutcome::Ok(out) =
            w.merchant_get_backupg_weapon(mid, "hero", 0, 0, 0, &deploy_cfg())
        else {
            panic!("应取回");
        };
        assert_eq!(
            out.user_item.desc[10], 12,
            "生产路径：两点+三点都命中 ⇒ oracle 覆盖成 12"
        );
        // 抽数次数不变（覆盖只改写入值）：Random(50)+Random(30)+Random(200) = 3 抽。
        let mut exp = RandomNumber::with_seed(seed);
        let _ = exp.random_below(50);
        let _ = exp.random_below(30);
        let _ = exp.random_below(200);
        assert_eq!(w.rng.random(), exp.random(), "覆盖语义不改抽数序");
    }

    /// **C10**：过期清理边界——`>=` 删除、`<` 保留，且只影响过期记录。
    #[test]
    fn c10_clear_expired_upgrades_boundary() {
        let day = 24 * 60 * 60 * 1000i64;
        let now = 10 * day;
        let mut list = vec![
            {
                let mut i = info_with(1, 0, 0, 16, 0);
                i.user_name = "exact3".to_string();
                i.upgrade_time_ms = now - 3 * day;
                i
            },
            {
                let mut i = info_with(1, 0, 0, 16, 0);
                i.user_name = "justunder".to_string();
                i.upgrade_time_ms = now - 3 * day + 1;
                i
            },
            {
                let mut i = info_with(1, 0, 0, 16, 0);
                i.user_name = "old".to_string();
                i.upgrade_time_ms = 0;
                i
            },
        ];
        clear_expired_upgrades(&mut list, now, 3);
        let names: Vec<&str> = list.iter().map(|i| i.user_name.as_str()).collect();
        assert_eq!(names, vec!["justunder"], "3 天整删、差 1ms 留、旧记录删");
        // `Merchant.Run` 的 10min 门真的会调清理（days 从商人状态取）。
        let (mut w, mid) = upgrade_world(10);
        w.clock_ms = now;
        {
            let st = w
                .entities
                .get_mut(mid)
                .and_then(|e| e.merchant.as_mut())
                .unwrap();
            st.clear_expre_upgrade_tick = 0;
            st.clear_expire_upgrade_days = 3;
            let mut old = info_with(1, 0, 0, 16, 0);
            old.upgrade_time_ms = 0;
            st.upgrade_weapon_list.push(old);
        }
        let _ = merchant_run(&mut w, mid);
        assert_eq!(list_len(&w, mid), 0, "10min 门到点清理过期记录");
    }

    /// 攻城区型城堡：`castle_map_id` 命中 + `|home - pos| < range`（严格 `<`）。
    fn castle_at(map_id: usize, home: (i16, i16), range: (i32, i32)) -> UserCastle {
        let mut c = UserCastle::new("沙巴克", CastleSiegeConfig::default());
        c.map_name = Some(map_id.to_string());
        c.castle_map_id = Some(map_id);
        c.home_x = home.0;
        c.home_y = home.1;
        c.war_range_x = range.0;
        c.war_range_y = range.1;
        c
    }

    /// G-T1 `merchant_castle_index_is_fixed_at_spawn`：spawn 时按
    /// `CastleMgr.InCastleWarArea(this)`（`NormNpc.cs:1674-1678`）解析一次，之后移动**不重算**。
    ///
    /// 牙：把 `merchant_castle_index` 改成“按当前坐标实时重算” ⇒ 移动后断言 `Some(0)` 得 `None`（红）；
    /// 把 `insert_merchant` 的解析删掉（恒 `None`）⇒ 首条断言红。
    #[test]
    fn merchant_castle_index_is_fixed_at_spawn() {
        let mut w = World::with_maps(vec![(120, 120); 10]);
        w.castles.push(castle_at(3, (100, 50), (100, 100)));

        let mut in_area = spawn_at(1, 50); // dx = |100-1| = 99 < 100
        in_area.map_id = 3;
        in_area.state.castle_merchant = false; // 与 Castle 命中无关（§1.2.2）
        let id = w.spawn_merchant(in_area).expect("map 3 界内可走");
        assert_eq!(w.merchant_castle_index(id), Some(0));
        let binding = w.merchant_castle_binding(id).expect("商人");
        assert!(!binding.castle_merchant, "MerChant 第 8 列缺省 0");
        assert_eq!(
            binding.castle_index,
            Some(0),
            "CastleMerchant=false 但位置在攻城区仍命中 Castle"
        );

        // oracle：Initialize 后移动不重算（`Merchant.Run → MapRandomMove` 不刷 Castle）。
        // 移到 dx = 100（严格 `<` 不成立）——若重算必须变 None。
        w.entities.get_mut(id).unwrap().x = 0;
        assert_eq!(
            w.merchant_castle_index(id),
            Some(0),
            "spawn 后移动不得重算（若重算应为 None）"
        );

        // 同一组城堡，出生点就在界外 ⇒ None（证明解析真的按位置发生，不是恒 Some）。
        let mut out_of_area = spawn_at(1, 50);
        out_of_area.map_id = 3;
        out_of_area.state.castle_merchant = true;
        w.castles[0].war_range_x = 99; // dx=99 ⇒ 严格 `<` 不成立
        let far_id = w.spawn_merchant(out_of_area).expect("界内");
        assert_eq!(
            w.merchant_castle_index(far_id),
            None,
            "dx=99 不满足严格 `<`（range=99）"
        );
    }

    /// `InCastleWarArea` 三支（攻城区严格 `<` / 皇宫 / 密道 / `EnvirList`）都参与 spawn 解析；
    /// 多座城按 `CastleList` 顺序取首个命中。
    ///
    /// 牙：把解析改成 `castle_member_at`（按城主行会）⇒ 所有行都 `None`（红）；把严格 `<`
    /// 改成 `<=` ⇒ dx=100 行得 `Some(0)`（红，需同步改 `castle.rs` 生产路径）。
    #[test]
    fn merchant_castle_index_uses_in_castle_war_area_branches() {
        let mut w = World::with_maps(vec![(120, 120); 10]);
        w.castles.push(castle_at(3, (100, 50), (100, 100)));
        let mut palace = UserCastle::new("城B", CastleSiegeConfig::default());
        palace.palace_map_id = Some(8);
        palace.envir_map_ids = vec![7];
        w.castles.push(palace);

        let mut spawn_on = |map_id: usize, x: i32, y: i32| -> Option<usize> {
            let mut sp = spawn_at(x, y);
            sp.map_id = map_id;
            let id = w.spawn_merchant(sp).expect("界内可走");
            w.merchant_castle_index(id)
        };

        // dx=99 ⇒ 命中城 A；dx=100 ⇒ 严格 `<` 不成立。
        assert_eq!(spawn_on(3, 1, 50), Some(0), "dx=99");
        assert_eq!(spawn_on(3, 0, 50), None, "dx=100 严格 `<` 为 false");
        // 皇宫 / EnvirList 支。
        assert_eq!(spawn_on(8, 0, 0), Some(1), "PalaceEnvir");
        assert_eq!(spawn_on(7, 0, 0), Some(1), "EnvirList");
        // 无命中。
        assert_eq!(spawn_on(9, 0, 0), None);
    }

    /// G-T1 变体：`castle_index` 的 setter/accessor 往返（构造级夹具与 `GetCastle` 重绑）。
    ///
    /// 牙：setter 改成只对 `castle_merchant=true` 的商人生效 ⇒ 缺省商人往返断言红。
    #[test]
    fn merchant_castle_index_setter_round_trip() {
        let mut w = World::with_maps(vec![(20, 20); 4]);
        let id = w.spawn_merchant(spawn_at(3, 4)).expect("map 0");
        assert_eq!(w.merchant_castle_index(id), None, "无城堡 ⇒ Castle=null");

        assert!(w.set_merchant_castle_index(id, Some(2)));
        assert_eq!(w.merchant_castle_index(id), Some(2));
        assert_eq!(
            w.merchant_castle_binding(id),
            Some(MerchantCastleBinding::new(false, Some(2)))
        );

        assert!(w.set_merchant_castle_index(id, None));
        assert_eq!(w.merchant_castle_index(id), None);

        // 非商人：setter false、getter None。
        assert!(!w.set_merchant_castle_index(9999, Some(0)));
        assert_eq!(w.merchant_castle_index(9999), None);
        assert!(w.merchant_castle_binding(9999).is_none());
    }
}
