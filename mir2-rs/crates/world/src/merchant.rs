//! 商人（`Merchant`）世界实体 + 挂图 `Initialize` —— M3⑤-实现 2 的**第①片**。
//!
//! oracle 锚点（逐条复核于本片）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`MerchantState`] 字段 | `src/GameSrv/Npc/Merchant.cs:10-70`（`IMerchant`）+ `BaseObject` 的 `RunTick/RunTime`（`BaseObject.cs:439-440`） |
//! | [`World::spawn_merchant`] 的 `Initialize`/`AddtoMapSuccess` | `src/M2Server/Actor/BaseObject.Base.cs:14-21` |
//! | 挂图失败移除（`!IsHide` 例外） | `src/GameSvr/Word/WorldServer.Merchant.cs:8-25` |
//! | `FindMap(MapName) == null ⇒ 丢弃` | `WorldServer.Merchant.cs:24-31`（**门在宿主侧按 `MapRegistry::id_of` 施加**，见 [`crate::merchant`] 模块尾与 `bins/game-svr/src/merchant.rs`） |
//!
//! ## 为什么门在宿主侧
//! `SystemShare.MapMgr.FindMap(MapName)` 的部署态等价物是 `MapRegistry::id_of`（同 `MapInfo` +
//! `Map/*.map` 合并口径，见 `crates/envir`）。`crates/world` 只持有**下标式**地图表、不含逻辑名
//! ⇒ 名字→`map_id` 的解析归宿主；本模块只负责「拿到 `map_id` 后的挂图/落格」。
//!
//! ## 未建模（登记，留给本卡后续分片）
//! - `Merchant.Run`（`Merchant.cs:112-170`）、`RefillGoods`（`:209-276`）、
//!   `ProcessMerchants` 分片 tick（`WorldServer.Merchant.cs:65-110`）：本片**不驱动**。
//!   分片预算（`M2Share.NpcLimit = 5ms`）在单线程确定性 tick 下的等价物需 owner 拍板
//!   （其直接影响 Claim B 的 site/order，见卡 `openmir2-m3-merchant-entity-tick` comment）。
//! - `BaseObject` ctor 的 `RunTick = GetTickCount() + Random(1500)`（`BaseObject.cs:439`）本片
//!   **不抽随机数**（避免把构造抽落在错的分片；与 tick 分片一起按 oracle 次序接）。
//! - `GoodsList`/`ItemPriceList` 的库存与 `SaveGoodRecord` 持久化 —— 接缝归 ⑤-5。
//! - `NormNpc.Initialize` 的脚本/说词加载（`NormNpc.cs:1676`）—— ⑤-4/⑤-6 接缝。

use mir2_data::models::StdItem;

use crate::entity::Entity;
use crate::map::CellType;
use crate::world::World;

/// `ActorRace.Merchant`（`src/OpenMir2/Enums/Race.cs`：`Merchant = 50`，与 `Animal` 同值）。
///
/// ⚠ 与 `Race == ActorRace.Play` 一样是**broadcast 口径的枚举值**；`crates/world` 的怪物 AI
/// （[`crate::monai::monster_ai_tick`]）按 [`Entity::merchant`] 是否存在显式排除商人实体，
/// **不**依赖 race 数值（race 50 与 Animal 同值，靠 race 会误跑 `monster_run`）。
pub const ACTOR_RACE_MERCHANT: u8 = 50;

/// `Merchant.Run` 的 `RunTime`（`BaseObject.cs:440`：`RunTime = 250`）。
pub const MERCHANT_RUN_TIME: i64 = 250;

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
    /// 本片不抽构造随机数 ⇒ 由 [`World::spawn_merchant`] 置为 spawn 时刻（登记）。
    pub run_tick: i64,
    /// `GhostTick`（`BaseObject`；`ProcessMerchants` 的 60s 过期门）。
    pub ghost_tick: i64,
}

impl MerchantState {
    /// 按 oracle ctor 的**非随机**初值构造（`Merchant.cs:70-106`）；随机项（`RunTick` 的
    /// `Random(1500)`）由调用方在挂图时按 oracle 次序填入（本片置 spawn 时刻）。
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
            run_tick: 0,
            ghost_tick: 0,
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
    /// **不抽随机数**（构造抽登记留给 tick 分片）：`run_tick`/`refill_goods_tick`/
    /// `clear_expre_upgrade_tick`/`move_tick` 都置为本刻 `clock_ms`。
    pub fn spawn_merchant(&mut self, spawn: MerchantSpawn) -> Option<i32> {
        if spawn.map_id >= self.maps.len() {
            return None;
        }
        let now = self.clock_ms;
        let map_id = spawn.map_id;
        let (x, y) = (spawn.x, spawn.y);
        let mut state = spawn.state;
        state.run_tick = now;
        state.refill_goods_tick = now;
        state.clear_expre_upgrade_tick = now;
        state.move_tick = now;
        state.ghost_tick = 0;
        let is_hide = state.is_hide;

        let mut e = Entity::new(0, spawn.name, x, y);
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
}
