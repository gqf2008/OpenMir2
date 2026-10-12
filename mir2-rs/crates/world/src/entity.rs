//! 实体存储 —— 对应 C# 的 `SystemShare.ActorMgr`（`ActorId` 稳定 id）+ `BaseObject` 的 AOI 相关字段。
//!
//! 存储形态：连续数组 + 空闲槽回收 + **代际号**（generation），防止"已删除 id 被复用"后
//! 旧引用误命中（C# 侧靠 `ActorMgr.Get(id) == null` 判断，但 id 会复用；这里把复用风险显式化，
//! 对外仍以 C# 风格的 `i32` id 暴露，代际只在内部校验）。
//!
//! 遍历顺序 = 槽位顺序（= 插入顺序 + 回收槽复用），**全库唯一**：
//! tick 的确定性（判据①）依赖"同一组操作产生同一槽位排布"。

/// `src/OpenMir2/Enums/Race.cs`：`ActorRace.Play = 0`、`ActorRace.Monster = 80`。
pub const ACTOR_RACE_PLAY: u8 = 0;
pub const ACTOR_RACE_MONSTER: u8 = 80;

/// `CharacterObject.UseItems` 的佩戴位数量（`CharacterObject.cs:89`：`UseItems = new UserItem[13]`）。
pub const USE_ITEM_SLOTS: usize = 13;

/// `UserItem` 的战斗子集（`src/OpenMir2/Packets/ClientPackets/ClientUserItem.cs:14`）。
///
/// 本骨架只用到 `PlayObject.StruckDamage`（`PlayObject.cs:3003-3039`）的耐久循环：
/// 每格 `Index > 0`（= 已装备）各抽一次 `Random(8)`。`Dura` 只做减法，无消息面。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UseItemSlot {
    /// `UserItem.Index`（0 = 空位）
    pub index: u16,
    /// `UserItem.Dura`
    pub dura: u16,
}

/// `src/Modules/SystemModule/Enums/VisibleFlag.cs`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum VisibleFlag {
    /// 隐藏（本轮未再看见）
    Hidden = 0,
    /// 不可见（已存在、本轮又看见）
    Invisible = 1,
    /// 可见（本轮新增）
    Show = 2,
}

/// `src/Modules/SystemModule/Data/VisibleBaseObject.cs`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VisibleBaseObject {
    pub target: i32,
    pub flag: VisibleFlag,
}

/// `BaseObject` 中参与本骨架（tick / AOI / 移动）的字段子集。
///
/// S1 起 `Entity` 直接就是 bevy 的**组件**（每个实体一个组件），由
/// `mir2_world_bevy::EcsStore`（bevy `World` + 显式槽位索引）承载。
#[derive(Clone, Debug, bevy_ecs::prelude::Component)]
pub struct Entity {
    /// C# `ActorId`
    pub id: i32,
    pub name: String,
    pub x: i32,
    pub y: i32,
    /// C# `ViewRange`（`BaseObject` 默认 5）
    pub view_range: i32,
    /// C# `Race`
    pub race: u8,
    /// C# `Master`（宝宝的主人）
    pub master: Option<i32>,
    pub death: bool,
    pub invisible: bool,
    pub ghost: bool,
    pub fixed_hide_mode: bool,
    pub ob_mode: bool,
    /// C# `NastyMode`（PK 模式等）
    pub nasty_mode: bool,
    /// C# `PlayObject.AttatckMode`（`AttackMode` 枚举，`src/OpenMir2/Enums/AttackMode.cs`）：
    /// `HAM_ALL=0 / HAM_PEACE=1 / HAM_DEAR=2 / HAM_MASTER=3 / HAM_GROUP=4 / HAM_GUILD=5 / HAM_PKATTACK=6`。
    /// 新角色 `PlayObject.cs:908` 置 `0`（`HAM_ALL`）；`ChangeAttackModeCommand.cs:26` 按 Ctrl-H 循环。
    pub attack_mode: u8,
    /// C# `BaseObject.InSafeZone()`（`BaseObject.cs:1777`）的结果折叠。
    ///
    /// oracle 的 `IsProtectTarget`（`BaseObject.Base.cs:360`）用它做 PvP 门槛；本骨架未建模
    /// 地图 Flag/StartPoint 安全区，由接线层在入图/换图时按 `Envir` 算好后写入；默认 `false`
    /// （= 非安全区，写清了才允许 PvP，与真机 pin 到比奇城东空地一致）。
    pub in_safe_zone: bool,
    /// C# `BaseObject.AdminMode`（`BaseObject.IsAttackTarget` 首判：管理员不可被攻击）。
    pub admin_mode: bool,
    /// C# `BaseObject.StoneMode`（同上，石化不可被攻击）。
    pub stone_mode: bool,
    /// C# `PlayObject.DearHuman`（`PlayObject.IsAttackTarget` 的 `HAM_DEAR` 支：只保护配偶）。
    pub dear_human: Option<i32>,
    /// C# `PlayObject.PkPoint`——`PlayObject.PvpLevel()`（`PlayObject.cs:4352`）= `PkPoint / 100`。
    /// `HAM_PKATTACK` 支用它做红名/白名判定。
    pub pk_point: i32,
    /// C# `PlayObject.Luck`（`PlayObject.cs:62`，`RecalcAbilitys` 里 `Luck = 0 + AddAbil.Luck - AddAbil.UnLuck`）。
    /// 玩家攻方的 [`crate::combat::get_attack_power_player`] override 用它决定取数次数。
    pub luck: i32,
    /// C# `PlayObject.PowerRate`（`PlayObject.cs:724`；新角色 `PlayObject.cs:943` 置 `100`）。
    pub power_rate: i32,
    /// C# `PlayObject.BoPowerItem`（`PlayObject.cs:223`）。
    pub bo_power_item: bool,
    /// C# `PlayObject.PowerItem`（`PlayObject.cs:199`）。
    pub power_item: u8,
    /// C# `CharacterObject.UseItems`（`CharacterObject.cs:40`，13 格，`CharacterObject.cs:89` 初始化）。
    /// `PlayObject.StruckDamage` 的耐久循环按它取数。
    pub use_items: [UseItemSlot; USE_ITEM_SLOTS],
    /// C# `WantRefMsg`
    pub want_ref_msg: bool,
    /// C# `AddToMaped`（首次入图标记）
    pub add_to_mapped: bool,
    /// C# `VisibleActors`
    pub visible_actors: Vec<VisibleBaseObject>,
    /// C# `IsVisibleActive`
    pub is_visible_active: bool,
    /// 本 tick 是否已被处理（tick 顺序判据用）
    pub processed_tick: u64,
    /// 所在地图（多地图世界用；`World::new` 单图时为 0）
    pub map_id: usize,
    /// `WAbil.HP` / `WAbil.MaxHP`
    pub hp: u16,
    pub max_hp: u16,
    /// `WAbil.DC`（低字节=下限、高字节=上限，与 C# 一致）
    pub dc: i32,
    /// `WAbil.AC`
    pub ac: i32,
    /// `HitPoint` / `SpeedPoint`（命中/敏捷，命中判定用）
    pub hit_point: u8,
    pub speed_point: u8,
    /// `Job`（0 战 / 1 法 / 2 道）
    pub job: u8,
    /// `LifeAttrib`（1 = `LA_UNDEAD` 不死系）
    pub life_attrib: u8,
    /// `Abil.Level`
    pub level: u8,
    /// `MonsterInfo.Exp`（怪物经验，死亡时按 `CalcGetExp` 结算）
    pub mon_exp: i32,
    /// `Abil.Exp` / `Abil.MaxExp`
    pub exp: i32,
    pub max_exp: i32,
    /// `AddAbil.UndeadPower`（攻击方对不死系的加成）
    pub undead_power: i32,
    /// `LastHiter`（最后一击者 `ActorId`）
    pub last_hiter: Option<i32>,
    /// C# `ExpHitter`（经验归属者 `ActorId`）——由 `SetLastHiter` 设置：
    /// **第一个**命中者粘住不放（`BaseObject.cs:1753` 的 `if (ExpHitter == null)`），
    /// 死亡时 `KillFunc()`（`BaseObject.cs:2429`）把经验发给它，而不是 `LastHiter`。
    pub exp_hitter: Option<i32>,
    /// 掉落预算里的金币（`mon.Gold`）
    pub gold: i32,
    /// 掉落预算里的物品（`mon.ItemList`，生成时算好、死亡时落地）
    pub drop_items: Vec<crate::combat::DropItem>,
    /// 死亡结算是否已做（避免重复发经验/重复掉落）
    pub death_settled: bool,
}

impl Entity {
    pub fn new(id: i32, name: impl Into<String>, x: i32, y: i32) -> Self {
        Entity {
            id,
            name: name.into(),
            x,
            y,
            view_range: 5, // BaseObject.cs:383
            race: ACTOR_RACE_PLAY,
            master: None,
            death: false,
            invisible: false,
            ghost: false,
            fixed_hide_mode: false,
            ob_mode: false,
            nasty_mode: false,
            // `PlayObject.cs:908` 新角色 `AttatckMode = 0 = HAM_ALL`；未接 `CM_ATTACKMODE` 前恒为 0。
            attack_mode: 0,
            in_safe_zone: false,
            admin_mode: false,
            stone_mode: false,
            dear_human: None,
            pk_point: 0,
            // `PlayObject.cs:62/943`：新角色 `Luck` 由 `RecalcAbilitys` 置 0（无 `AddAbil`），
            // `PowerRate = 100`；`BoPowerItem=false` / `PowerItem=0`。
            luck: 0,
            power_rate: 100,
            bo_power_item: false,
            power_item: 0,
            // `CharacterObject.cs:89`：13 个空位（`Index = 0`）。
            use_items: [UseItemSlot::default(); USE_ITEM_SLOTS],
            want_ref_msg: false,
            add_to_mapped: false,
            visible_actors: Vec::new(),
            is_visible_active: false,
            processed_tick: 0,
            map_id: 0,
            // 玩家默认满血（WAbil.HP 的默认值不能是 0：0 在语义上等于「已死」，
            // 会让死亡结算把刚进图的玩家当成死者——实测踩过）
            hp: 100,
            max_hp: 100,
            dc: 0,
            ac: 0,
            hit_point: 0,
            speed_point: 0,
            job: 0,
            life_attrib: 0,
            level: 1,
            mon_exp: 0,
            exp: 0,
            max_exp: 0,
            undead_power: 0,
            last_hiter: None,
            exp_hitter: None,
            gold: 0,
            drop_items: Vec::new(),
            death_settled: false,
        }
    }

    /// C# `BaseObject.SetLastHiter`（`BaseObject.cs:1753`）。
    ///
    /// `LastHiter` 每次都刷新；`ExpHitter` **只在为空时**才认领（第一个命中者粘住），
    /// 之后只有同一个命中者再次命中才会续期（本骨架不做 6 秒过期，见 `world.rs` 说明）。
    pub fn set_last_hiter(&mut self, hitter: i32) {
        self.last_hiter = Some(hitter);
        if self.exp_hitter.is_none() {
            self.exp_hitter = Some(hitter);
        }
    }
}

/// 实体仓库（`ActorMgr` 等价物）——S1 起是 **bevy_ecs 内核的薄适配层**。
///
/// 存储与遍历顺序都在 [`mir2_world_bevy::EcsStore`]（bevy `World` + 显式槽位索引）：
/// - 槽位顺序 = 插入顺序 + 空闲槽 LIFO 复用，`iter`/`iter_mut`/`ids_sorted`/`order_index` 全按它；
/// - `get`/`get_mut` 仍是 O(1) 查表；代际号在内核里维护（对外不可见）。
///
/// **顺序契约**：本类型**不迭代 Query 求顺序**——`iter`/`iter_mut` 走内核的显式槽位索引，
/// `Query`（内核里的 `world.query`）只用于「按句柄安全拿到互斥引用」，之后仍按槽位排序。
#[derive(Debug)]
pub struct EntityStore {
    inner: mir2_world_bevy::EcsStore<Entity>,
}

impl Default for EntityStore {
    fn default() -> Self {
        Self::new()
    }
}

impl EntityStore {
    pub fn new() -> Self {
        let mut inner = mir2_world_bevy::EcsStore::new();
        // 内核资源：显式处理顺序索引 + 本 tick 顺序（tick 骨架的系统按它们处理）。
        inner
            .world_mut()
            .insert_resource(mir2_world_bevy::SlotOrder::default());
        inner
            .world_mut()
            .insert_resource(mir2_world_bevy::TickOrder::default());
        EntityStore { inner }
    }

    /// 分配一个 id（不插入实体；C# `ActorMgr` 先 `Add` 后填字段）。
    pub fn alloc_id(&mut self) -> i32 {
        self.inner.alloc_id()
    }

    /// 插入实体，返回其 id（复用本实体的 `id`）。
    pub fn insert(&mut self, entity: Entity) -> i32 {
        self.inner.insert(entity.id, entity)
    }

    pub fn remove(&mut self, id: i32) -> Option<Entity> {
        self.inner.remove(id)
    }

    /// O(1) 查表（C# `ActorMgr.Get`）。
    pub fn get(&self, id: i32) -> Option<&Entity> {
        self.inner.get(id)
    }

    pub fn get_mut(&mut self, id: i32) -> Option<&mut Entity> {
        self.inner.get_mut(id)
    }

    /// 按槽位顺序遍历（tick 的确定性顺序）。
    pub fn iter(&self) -> impl Iterator<Item = &Entity> + '_ {
        self.inner.iter()
    }

    pub fn iter_mut(&mut self) -> impl Iterator<Item = &mut Entity> + '_ {
        self.inner.iter_mut()
    }

    pub fn len(&self) -> usize {
        self.inner.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// 按 id 升序收集（用于输出/对拍的稳定顺序）。
    pub fn ids_sorted(&self) -> Vec<i32> {
        self.inner.ids_sorted()
    }

    /// **显式处理顺序索引**：槽位顺序的业务 id（`crates/world` 的处理顺序契约）。
    pub fn order_index(&self) -> Vec<i32> {
        self.inner.order_index()
    }

    /// 把当前槽位顺序写入 ECS 资源 [`mir2_world_bevy::SlotOrder`]（tick 骨架处理前调用）。
    pub fn sync_slot_order(&mut self) {
        self.inner.sync_slot_order();
    }

    /// 暴露内核的 bevy `World`（`TickRunner` 跑单线程 Schedule 用）。
    pub fn world(&self) -> &bevy_ecs::world::World {
        self.inner.world()
    }

    pub fn world_mut(&mut self) -> &mut bevy_ecs::world::World {
        self.inner.world_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_stable_and_order_is_slot_order() {
        let mut s = EntityStore::new();
        let a = s.alloc_id();
        let b = s.alloc_id();
        s.insert(Entity::new(a, "a", 1, 1));
        s.insert(Entity::new(b, "b", 2, 2));
        assert_eq!(
            s.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
        s.remove(a);
        let c = s.alloc_id();
        s.insert(Entity::new(c, "c", 3, 3));
        // 回收槽复用：c 落在 a 的槽位（顺序 = 槽位顺序，判据①钉住这一点）
        assert_eq!(
            s.iter().map(|e| e.name.clone()).collect::<Vec<_>>(),
            vec!["c", "b"]
        );
    }
}
