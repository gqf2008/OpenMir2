//! 背包/装备动作（`CM_TAKEONITEM` 装备 / `CM_TAKEOFFITEM` 卸下）的**纯函数接缝**。
//!
//! # 这条接缝给谁用
//!
//! oracle 的 `CM_TAKEONITEM`/`CM_TAKEOFFITEM` 落在 `PlayObject.Message.cs:820/:823`，真实实现是
//! `PlayObject.Operate.cs:377`（`ClientTakeOnItems`）与 `:499`（`ClientTakeOffItems`）——它们
//! 把 `ItemList`（背包）与 `UseItems[13]`（身上）这两个集合按 oracle 的守卫链做一次迁移，并回
//! `SM_TAKEON_OK/FAIL`（615/616）、`SM_TAKEOFF_OK/FAIL`（619/620）。
//!
//! 卡 `openmir2-m3-bag-equip-alignment` 的 DoD 是「拾取/装备/卸下/丢弃/使用药品 两侧同序列、
//! 跨侧作用域内字段级 diff=0」。本片是这条 DoD 的**装备/卸下（2/5）切片**：
//! `bins/game-svr` 侧（接线归 m2-svr 车道，本片不碰 `main.rs`/`smframe.rs`）先有**可判据的
//! 决策与状态迁移核心**；拾取/丢弃/用药需要 `crates/world` 还没有的**地图地面物品模型**（`Envir.GetItem`）
//! 与 `Envir.Flag`（`boNODRUG`/`NoThrowItem`）⇒ 登记为下一片（见下「未做清单」）。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! ClientTakeOnItems(btWhere, nItemIdx, sItemName)      PlayObject.Operate.cs:377
//!   ├─ 在 ItemList 里按 (MakeIndex == nItemIdx) 找实例；stdItem = GetStdItem(Index)
//!   │    且 CustomItemSystem.GetItemName(userItem) 与 sItemName OrdinalIgnoreCase 相等
//!   ├─ M2Share.CheckUserItems(btWhere, stdItem)         M2Share.cs:376（槽位 ↔ StdMode 白名单）
//!   ├─ GetUpgradeStdItem → clientItem（本片 = 恒等拷贝，见「未做清单」）
//!   ├─ PlayObject.CheckTakeOnItems(btWhere, clientItem) PlayObject.Base.cs:1730
//!   │    （性别、腕力/负重、Need/NeedLevel 分支）+ CheckItemBindUse（本片恒 true）
//!   ├─ 已占槽的"不可卸"守卫（Desc[7] / ItemDesc&2 / ItemDesc&4 / InDisableTakeOffList）
//!   ├─ UseItems[btWhere]=userItem; DelBagItem(itemIndex); AddItemToBag(takeOffItem)
//!   ├─ RecalcAbilitys()（见 `ability::recalc_abilitys`）
//!   └─ SendDefMessage(SM_TAKEON_OK, GetFeatureToLong(), GetFeatureEx(), 0, 0)  ⇒ n18=1
//!      else  n18 ∈ {0, -1, -4} ⇒ SendDefMessage(SM_TAKEON_FAIL, n18, 0, 0, 0)
//!
//! ClientTakeOffItems(btWhere, nItemIdx, sItemName)     PlayObject.Operate.cs:499
//!   ├─ Dealing / btWhere<13 守卫；UseItems[btWhere].Index>0 守卫（否则 n10=-2）
//!   ├─ MakeIndex == nItemIdx；同样三条"不可卸"守卫（否则 n10=-4）
//!   ├─ CustomItemSystem.GetItemName(...) 与 sItemName 相等
//!   ├─ AddItemToBag(userItem)（背包满 46 ⇒ n10=-3）；UseItems[btWhere]=null；RecalcAbilitys()
//!   └─ SendDefMessage(SM_TAKEOFF_OK, GetFeatureToLong(), GetFeatureEx(), 0, 0)  ⇒ n10=1
//!      else  n10 ∈ {0, -1, -2, -3, -4} ⇒ SendDefMessage(SM_TAKEOFF_FAIL, n10, 0, 0, 0)
//! ```
//!
//! 几个**容易抄错、必须原样**的口径：
//!
//! 1. **`InDisableTakeOffList` 分支不设错误码**：`goto FailExit` 之前**没有** `n18 = -4`
//!    （`Operate.cs:437`）⇒ 回 `SM_TAKEON_FAIL` 时 `Recog = 0`；take-off 同形（`:525`）。
//! 2. **道具名不符 = 找不到**：`stdItem`/`userItem` 保持 null ⇒ `n18` 停在 0（`SM_TAKEON_FAIL` 的
//!    `Recog=0`），不是 -1。
//! 3. **换下的旧物品进背包是"追加到尾部"**：先 `DelBagItem(itemIndex)` 再 `AddItemToBag(takeOffItem)`
//!    （`Operate.cs:449/:452`）⇒ 顺序进 `item_list` 末尾。
//! 4. **`AddItemToBag` 的满包判定是 `>= Grobal2.MaxBagItem`(46)**（`BaseObject.cs:1243`），
//!    take-on 忽略其返回值、take-off 用返回值走 `n10=-3`（且**不清空槽位**，`:543` 的
//!    `UseItems[btWhere]=null` 在成功分支里）。
//! 5. **take-on 成功后会 `Desc[8]=0`**（仅当 `StdModeMap` 命中，`Operate.cs:444`）。
//! 6. **`GetFeatureEx()` 恒等于 `MakeWord(OnHorse?HorseType:0, DressEffType)`**，而 `DressEffType`
//!    在 oracle 里**没有任何赋值点**（`CharacterObject.cs:48` 声明，全仓无写入）⇒ 恒 0；
//!    `SM_TAKEON_OK/TAKEOFF_OK` 的 `Param` 因此在本项目里恒 0（`feature::feature_ex`）。
//!
//! # 与 oracle 逐字节/逐字段相同怎么保证
//!
//! 用例值由 `mir2-rs/tests/parity/m3/bag-probe`（引用 `src/M2Server` 真身、用反射调
//! **私有** `ClientTakeOnItems`/`ClientTakeOffItems`，并用替身 `INetChannel` **截获真实出帧**）
//! 实测产出，落 `mir2-rs/tests/parity/m3/fixtures/bag_vectors.json`；本模块用例回读该夹具
//! 逐字段核对（改坏任一映射 ⇒ 必红）。生成命令（**在仓库根运行**）：
//!
//! ```text
//! dotnet build mir2-rs/tests/parity/m3/bag-probe -c Release
//! dotnet mir2-rs/tests/parity/m3/bag-probe/bin/Release/net8.0/bag-probe.dll \
//!   mir2-rs/tests/parity/m3/fixtures/bag_vectors.json
//! cargo test -p game-svr --lib
//! ```
//!
//! # 未做清单（登记在卡 `openmir2-m3-bag-equip-alignment`；不是"静默跳过"）
//!
//! - **拾取 / 丢弃 / 使用药品（`CM_PICKUP`/`CM_DROPITEM`/`CM_EAT`）**：oracle 三条都依赖
//!   `Envir`（`GetItem`/`DeleteFromMap`/`AddItemToMap`/`Envir.Flag.boNODRUG`/`NoThrowItem`）
//!   与地面物品（`MapItem`）模型，Rust `crates/world` 目前**没有地面物品** ⇒ 需要先在
//!   `crates/world` 加"地图地面物品"最小 API（与 m2-svr 的 `enter.rs`/`main.rs` 有撞面风险，
//!   需在卡内声明分片）。本片据此**不实现**这三条。
//! - **有效物品 = 模板**：`GetUpgradeStdItem`（`GameItemSystem.cs:731`）的 Desc 升级差量与各
//!   `StdMode` 变换未实现 ⇒ 传入本模块的模板必须是"已升级的有效物品"（与 `ability.rs` 同口径）。
//! - **`CheckItemBindUse`**：`M2Share.ItemBindAccount`/`ItemBindIPaddr` 的账号/IP 绑定校验
//!   未纳入（探针里两张表为空 ⇒ oracle 恒 true，两侧同域）。
//! - **自定义物品名**（`UserItem.Desc[13] == 1` → `CustomItemMgr.GetCustomItemName`）：未实现
//!   ⇒ 该形态按空名处理（普通物品 `Desc[13]==0` 用模板名，与 oracle 逐字节相同）。
//! - **出帧字节**：本模块只产出 `SM_TAKEON_OK/FAIL`、`SM_TAKEOFF_OK/FAIL` 的**帧规格**
//!   （ident/recog/param/tag/series）；`SM_ADDITEM`(200)/`SM_UPDATEITEM`(203) 与队列帧
//!   （`RM_ABILITY`/`RM_SUBABILITY`/`RM_FEATURECHANGED`）由接线层（m2-svr）按
//!   `swapped_out`/`bag_added` 信号发；探针**已截获并登记**这些帧的存在性（`has_add_item`/
//!   `has_update_item`），本模块用 `swapped_out` 对账。
//! - **双栈同序列驱动**（DoD 正文的"两侧同序列 diff=0"）：`src/**` 冻结、BotSrv 的
//!   装备动作驱动属登记面 ⇒ 本片不含；判据在此前仍是 RED，卡不 closed。

use mir2_data::StdItem;

use crate::ability::{self, Ability, ItemCatalog, USE_ITEMS_LEN};
use crate::feature;

/// `HUtil32.LoByte(byte NeedLevel)`：`NeedLevel` 是 `byte`，隐式转 `int` 后 `(byte)w = w`。
const fn lo_byte_u8(v: u8) -> u8 {
    v
}

/// `HUtil32.HiByte(byte NeedLevel)`：`(byte)(w >> 8)` = 0（`NeedLevel` 只有 1 字节）。
const fn hi_byte_u8(_v: u8) -> u8 {
    0
}

/// `HUtil32.LoWord(byte NeedLevel)`：`NeedLevel` 提升为 `int` ⇒ 低 16 位 = 值本身。
const fn lo_word_byte(v: u8) -> u8 {
    v
}

/// `HUtil32.HiWord(byte NeedLevel)`：高 16 位 = 0。
const fn hi_word_byte(_v: u8) -> u8 {
    0
}

/// `HUtil32.HiByte(ushort)`（`WAbil.DC/MC/SC`）。
const fn hi_byte_u16(v: u16) -> u8 {
    ((v >> 8) & 0x00FF) as u8
}

/// `Messages.CM_TAKEONITEM`（`src/OpenMir2/Messages.cs:47`）。
///
/// 收包接线：`PlayObject.Message.cs:820` 的 `ClientTakeOnItems((byte)processMsg.nParam2,
/// processMsg.nParam1, processMsg.Msg)` ⇒ `slot = tag`（低字节）、`make_index = param`、
/// `item_name = 尾`。
pub const CM_TAKEONITEM: u16 = 1003;
/// `Messages.CM_TAKEOFFITEM`（`src/OpenMir2/Messages.cs:48`）。
///
/// 同 `CM_TAKEONITEM` 的字段口径（`PlayObject.Message.cs:823`）。
pub const CM_TAKEOFFITEM: u16 = 1004;

/// `Messages.SM_TAKEON_OK`（`src/OpenMir2/Messages.cs:284`）。
pub const SM_TAKEON_OK: u16 = 615;
/// `Messages.SM_TAKEON_FAIL`（`:285`）。
pub const SM_TAKEON_FAIL: u16 = 616;
/// `Messages.SM_TAKEOFF_OK`（`:286`）。
pub const SM_TAKEOFF_OK: u16 = 619;
/// `Messages.SM_TAKEOFF_FAIL`（`:287`）。
pub const SM_TAKEOFF_FAIL: u16 = 620;
/// `Messages.SM_ADDITEM`（`:220`）——换装时旧物品回背包的帧（接线层据此发，本片只对账存在性）。
pub const SM_ADDITEM: u16 = 200;
/// `Messages.SM_UPDATEITEM`（`:223`）——衣服槽换装后的更新帧。
pub const SM_UPDATEITEM: u16 = 203;

/// `Grobal2.MaxBagItem`（`src/OpenMir2/Grobal2.cs:62`）。
pub const MAX_BAG_ITEM: usize = 46;

/// `M2Share.StdModeMap`（`src/M2Server/M2Share.cs:188`）：`{15,19,20,21,22,23,24,26}`。
pub const STD_MODE_MAP: [u8; 8] = [15, 19, 20, 21, 22, 23, 24, 26];

/// `M2Share.StdModeMap.Contains(stdMode)`。
#[must_use]
pub fn std_mode_in_map(std_mode: u8) -> bool {
    STD_MODE_MAP.contains(&std_mode)
}

/// `UserItem` 的本片子集（oracle `UserItem`：`MakeIndex`/`Index`/`Dura`/`DuraMax` + 14 字节 `Desc`）。
///
/// `Desc[7]` = 锁定；`Desc[8]` = 可鉴定/形状；`Desc[13]` = 自定义名标记。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BagItem {
    pub make_index: i32,
    pub index: u16,
    pub dura: u16,
    pub dura_max: u16,
    pub desc: [u8; 14],
}

impl BagItem {
    /// 只带 `MakeIndex`/`Index` 的最小构造（其余为 0，= oracle `new UserItem()` 之后未赋值的形态）。
    #[must_use]
    pub fn new(make_index: i32, index: u16) -> Self {
        Self {
            make_index,
            index,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }
    }
}

/// 装卸动作作用的两个集合：`ItemList`（背包）+ `UseItems[13]`（身上）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipState {
    /// `PlayObject.UseItems`（长度 13；`None` = oracle 的 `null` 槽）。
    pub use_items: Vec<Option<BagItem>>,
    /// `PlayObject.ItemList`（背包，顺序敏感）。
    pub item_list: Vec<BagItem>,
}

impl Default for EquipState {
    fn default() -> Self {
        Self::new()
    }
}

impl EquipState {
    #[must_use]
    pub fn new() -> Self {
        Self {
            use_items: vec![None; USE_ITEMS_LEN],
            item_list: Vec::new(),
        }
    }

    /// 从"槽位 → 物品"列表建状态（`None` 槽保持空）。
    #[must_use]
    pub fn from_slots(slots: Vec<Option<BagItem>>, item_list: Vec<BagItem>) -> Self {
        let mut use_items = vec![None; USE_ITEMS_LEN];
        for (i, it) in slots.into_iter().enumerate().take(USE_ITEMS_LEN) {
            use_items[i] = it;
        }
        Self {
            use_items,
            item_list,
        }
    }
}

/// 一条出帧规格（`Messages.MakeMessage` 之后进线上的 `CommandMessage`；`Recog` 是 int，其余按 ushort 截断）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Frame {
    pub ident: u16,
    pub recog: i32,
    pub param: u16,
    pub tag: u16,
    pub series: u16,
}

/// `PlayObject.CheckTakeOnItems` 的观测输入（`PlayObject.Base.cs:1730`）。
#[derive(Clone, Copy)]
pub struct TakeOnContext<'a> {
    /// `SystemShare.ItemSystem.GetStdItem`。
    pub catalog: &'a dyn ItemCatalog,
    /// `PlayerJob`（0 战 / 1 法 / 2 道；Need 10-13/40-43 的 `Job` 比较）。
    pub job: u8,
    /// `PlayerGender`（Man=0 / WoMan=1）。
    pub gender: u8,
    /// `Hair`（`GetFeatureToLong` 用）。
    pub hair: u8,
    /// `Abil.Level`（Need 0/10/40/70）。
    pub abil_level: u8,
    /// `ReLevel`（Need 4/40-44）。
    pub re_level: u8,
    /// `CreditPoint`（Need 5/44）。
    pub credit_point: u16,
    /// `MemberType`（Need 8/81/82）。
    pub member_type: u8,
    /// `MemberLevel`（Need 81/82）。
    pub member_level: u8,
    /// `MyGuild != null`（Need 6/7/60/70）。
    pub has_guild: bool,
    /// `GuildRankNo`（Need 60/70）。
    pub guild_rank_no: u8,
    /// `SystemShare.CastleMgr.IsCastleMember(this) != null`（Need 7/70）。
    pub is_castle_member: bool,
    /// `UserUnLockDurg`（解除锁定类药品生效时为 true）。
    pub user_unlock_durg: bool,
    /// `Dealing`（交易中）。
    pub dealing: bool,
    /// `DisableTakeOffList` 的**键集合**（oracle 查 `ContainsKey(Index - 1)`，`M2Share.cs:1482`）。
    pub disable_take_off_keys: &'a [u16],
    /// `OnHorse`。
    pub on_horse: bool,
    /// `HorseType`。
    pub horse_type: u8,
    /// `DressEffType`（oracle 全仓无赋值 ⇒ 恒 0）。
    pub dress_eff_type: u8,
    /// **调用前**的 `WAbil`（`CheckTakeOnItems` 读 `MaxHandWeight`/`MaxWearWeight`/`DC`/`MC`/`SC`）。
    pub wabil: &'a Ability,
}

impl TakeOnContext<'_> {
    fn feature_ex(&self) -> u16 {
        feature::feature_ex(self.on_horse, self.horse_type, self.dress_eff_type)
    }
}

/// `take_on`/`take_off` 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EquipOutcome {
    /// oracle 的错误码（take-on 的 `n18` / take-off 的 `n10`；`1` = 成功）。
    pub code: i32,
    /// 本动作**直接**发出的帧（`SM_TAKEON_OK/FAIL` 或 `SM_TAKEOFF_OK/FAIL`；恒 1 条）。
    pub frames: Vec<Frame>,
    /// 换下并回背包的旧物品（非空 ⇒ 接线层应发 `SM_ADDITEM`）。
    pub swapped_out: Option<BagItem>,
    /// 成功穿上的**衣服**（`StdMode 10/11`）⇒ 接线层应发 `SM_UPDATEITEM(203)`（G-2）。
    pub update_item: Option<BagItem>,
    /// 成功时需要重算能力（oracle 在成功分支调用 `RecalcAbilitys()`）。
    pub recalc: bool,
}

impl EquipOutcome {
    #[must_use]
    pub fn ok(&self) -> bool {
        self.frames
            .iter()
            .any(|f| f.ident == SM_TAKEON_OK || f.ident == SM_TAKEOFF_OK)
    }
}

/// `M2Share.CheckUserItems(btWhere, stdItem)`（`src/M2Server/M2Share.cs:376`）——槽位 ↔ `StdMode` 白名单。
#[must_use]
pub fn check_user_items(slot: u8, std_mode: u8) -> bool {
    match usize::from(slot) {
        ability::SLOT_DRESS => std_mode == 10 || std_mode == 11,
        ability::SLOT_WEAPON => std_mode == 5 || std_mode == 6,
        ability::SLOT_RIGHTHAND => std_mode == 28 || std_mode == 29 || std_mode == 30,
        ability::SLOT_NECKLACE => std_mode == 19 || std_mode == 20 || std_mode == 21,
        ability::SLOT_HELMET => std_mode == 15,
        ability::SLOT_ARMRING_L => std_mode == 24 || std_mode == 25 || std_mode == 26,
        ability::SLOT_ARMRING_R => std_mode == 24 || std_mode == 26,
        ability::SLOT_RING_L | ability::SLOT_RING_R => std_mode == 22 || std_mode == 23,
        ability::SLOT_BUJUK => std_mode == 25 || std_mode == 51,
        ability::SLOT_BELT => std_mode == 54 || std_mode == 64,
        ability::SLOT_BOOTS => std_mode == 52 || std_mode == 62,
        ability::SLOT_CHARM => std_mode == 53 || std_mode == 63,
        _ => false,
    }
}

/// `PlayObject.GetUserItemWeitht(nWhere)`（`PlayObject.Base.cs:2026`）——已装备物品重量（排除 `nWhere` 与 1/2 两腕槽）。
fn user_item_weight(ctx: &TakeOnContext<'_>, state: &EquipState, where_: i32) -> u32 {
    let mut sum = 0u32;
    for (i, slot) in state.use_items.iter().enumerate() {
        // oracle: `nWhere == -1 || (!(i == nWhere) && !(i == 1) && !(i == 2))`
        let skip = where_ != -1 && (i as i32 == where_ || i == 1 || i == 2);
        if skip {
            continue;
        }
        if let Some(it) = slot {
            if let Some(t) = ctx.catalog.get(it.index) {
                sum += u32::from(t.weight);
            }
        }
    }
    sum
}

/// `PlayObject.CheckTakeOnItems(nWhere, ref clientItem)`（`PlayObject.Base.cs:1730`）。
///
/// 本片口径：`clientItem.Item` = **有效物品模板**（`GetUpgradeStdItem` 恒等拷贝，见模块文档）。
/// `SysMsg` 的失败提示不进判据（本函数只回 bool + 不产生载荷）。
#[must_use]
pub fn check_take_on_items(
    ctx: &TakeOnContext<'_>,
    state: &EquipState,
    slot: u8,
    item: &StdItem,
) -> bool {
    // 性别：StdMode 10 = 男衣、11 = 女衣（`CharacterObject.CheckTakeOnItems` 顶部 switch）。
    match item.std_mode {
        10 if ctx.gender != 0 => return false,
        11 if ctx.gender != 1 => return false,
        _ => {}
    }
    if slot == ability::SLOT_WEAPON as u8 || slot == ability::SLOT_RIGHTHAND as u8 {
        if u32::from(item.weight) > u32::from(ctx.wabil.max_hand_weight) {
            return false;
        }
    } else if u32::from(item.weight) + user_item_weight(ctx, state, i32::from(slot))
        > u32::from(ctx.wabil.max_wear_weight)
    {
        return false;
    }
    // ⚠️ `StdItem.NeedLevel` 是 **byte**：`HUtil32.LoByte/HiByte(byte)` 经 int 隐式转换后
    // `Lo = NeedLevel`、`Hi = 0` ⇒ 10-13/40-44/81/82 分支里的 Hi 比较恒真（oracle 原样）。
    let lo = lo_byte_u8(item.need_level);
    let hi = hi_byte_u8(item.need_level);
    match item.need {
        0 => ctx.abil_level >= item.need_level,
        1 => hi_byte_u16(ctx.wabil.dc) >= item.need_level,
        2 => hi_byte_u16(ctx.wabil.mc) >= item.need_level,
        3 => hi_byte_u16(ctx.wabil.sc) >= item.need_level,
        4 => ctx.re_level >= item.need_level,
        5 => ctx.credit_point >= u16::from(item.need_level),
        6 => ctx.has_guild,
        7 => ctx.has_guild && ctx.is_castle_member,
        8 => ctx.member_type != 0,
        10 => ctx.job == lo && ctx.abil_level >= hi,
        11 => ctx.job == lo && hi_byte_u16(ctx.wabil.dc) >= hi,
        12 => ctx.job == lo && hi_byte_u16(ctx.wabil.mc) >= hi,
        13 => ctx.job == lo && hi_byte_u16(ctx.wabil.sc) >= hi,
        40 => ctx.re_level >= lo && ctx.abil_level >= hi,
        41 => ctx.re_level >= lo && hi_byte_u16(ctx.wabil.dc) >= hi,
        42 => ctx.re_level >= lo && hi_byte_u16(ctx.wabil.mc) >= hi,
        43 => ctx.re_level >= lo && hi_byte_u16(ctx.wabil.sc) >= hi,
        44 => ctx.re_level >= lo && ctx.credit_point >= u16::from(hi),
        60 => ctx.has_guild && ctx.guild_rank_no == 1,
        70 => {
            ctx.has_guild
                && ctx.is_castle_member
                && ctx.guild_rank_no == 1
                && ctx.abil_level >= item.need_level
        }
        81 => ctx.member_type == lo && ctx.member_level >= hi,
        82 => ctx.member_type >= lo && ctx.member_level >= hi,
        _ => false,
    }
}

/// `PlayObject.CheckItemsNeed(StdItem)`（`PlayObject.Base.cs:2832-2882`）的**7 值子集**。
///
/// **不是** [`check_take_on_items`] 的别名：后者是 `CM_TAKEONITEM` 的**全量穿戴校验**
/// （性别/等级/职业/负重/Need 0-13/40-44/…）。P21（`PlayObject.Message.cs:403-441`）的
/// 每-Run 复检只调本函数——**Need ∉ {6,60,7,70,8,81,82} ⇒ 一律 true**（含 Need=0 的等级不足）。
///
/// 逐分支照 oracle（`CheckItemsNeed` 的 `switch (stdItem.Need)`）：
/// - 6：`MyGuild == null ⇒ false`；
/// - 60：`MyGuild == null || GuildRankNo != 1 ⇒ false`；
/// - 7：`CastleMgr.IsCastleMember(this) == null ⇒ false`（**不看** `MyGuild`）；
/// - 70：`castle == null || GuildRankNo != 1 ⇒ false`（**不看** `MyGuild`）；
/// - 8：`MemberType == 0 ⇒ false`；
/// - 81：`MemberType != LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel) ⇒ false`；
/// - 82：`MemberType < LoWord(NeedLevel) || MemberLevel < HiWord(NeedLevel) ⇒ false`；
/// - 其它：true。
///
/// `NeedLevel` 是 **byte**（`OpenMir2.Data.StdItem`），传给 `HUtil32.LoWord/HiWord(int)` 时提升为
/// `int` ⇒ `LoWord = NeedLevel`、`HiWord = 0`；因此 81/82 的 `MemberLevel < HiWord` 分支
/// **对 u8 恒 false**（oracle 原样，不是省事）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ItemsNeedContext {
    /// `MyGuild != null`。
    pub has_guild: bool,
    /// `PlayObject.GuildRankNo`（**`short`**，`PlayObject.cs:250`）。
    pub guild_rank_no: i16,
    /// `SystemShare.CastleMgr.IsCastleMember(this) != null`。
    pub is_castle_member: bool,
    /// `PlayObject.MemberType`（**`int`**，`PlayObject.cs:750`）。
    pub member_type: i32,
    /// `Player.MemberLevel`。
    pub member_level: u8,
}

/// `PlayObject.CheckItemsNeed(StdItem)`——见 [`ItemsNeedContext`] 的口径表。
#[must_use]
pub fn check_items_need(ctx: &ItemsNeedContext, need: u8, need_level: u8) -> bool {
    match need {
        6 => ctx.has_guild,
        60 => ctx.has_guild && ctx.guild_rank_no == 1,
        7 => ctx.is_castle_member,
        70 => ctx.is_castle_member && ctx.guild_rank_no == 1,
        8 => ctx.member_type != 0,
        // `LoWord(byte)` = 值、`HiWord(byte)` = 0 ⇒ `member_level < 0` 恒 false（只剩前一项）。
        81 => {
            let hi = u16::from(hi_word_byte(need_level));
            ctx.member_type == i32::from(lo_word_byte(need_level))
                && u16::from(ctx.member_level) >= hi
        }
        82 => {
            let hi = u16::from(hi_word_byte(need_level));
            ctx.member_type >= i32::from(lo_word_byte(need_level))
                && u16::from(ctx.member_level) >= hi
        }
        _ => true,
    }
}

/// `CustomItemSystem.GetItemName(UserItem)`（`Items/CustomItemSystem.cs:122`）的本片子集：
/// `Desc[13] == 1` 走自定义名（**未实现**，返回空串，见模块文档）；否则取模板名。
#[must_use]
pub fn item_display_name(catalog: &dyn ItemCatalog, item: &BagItem) -> String {
    if item.desc[13] == 1 {
        return String::new();
    }
    catalog.name(item.index).unwrap_or("").to_string()
}

/// `GetFeatureToLong()`（复用 `ability::equipped_feature` 的 `UseItems[Dress/Weapon]` 口径）。
fn feature_to_long(ctx: &TakeOnContext<'_>, state: &EquipState) -> i32 {
    let use_items: Vec<Option<ability::ItemInstance>> = state
        .use_items
        .iter()
        .map(|s| {
            s.as_ref().map(|it| ability::ItemInstance {
                index: it.index,
                dura: it.dura,
            })
        })
        .collect();
    ability::equipped_feature(ctx.catalog, &use_items, ctx.gender, ctx.hair)
}

/// 三条"已占槽不可卸"守卫（`Desc[7]` / `ItemDesc&2` / `ItemDesc&4` / `InDisableTakeOffList`）。
///
/// 返回 `Some(code)` 表示被拦下：`-4` 是前三条的码；`InDisableTakeOffList` 命中回 `Some(0)`
/// （oracle 在该分支 **不设错误码**，`Operate.cs:437/:525`）。
fn occupied_slot_guard(ctx: &TakeOnContext<'_>, existing: &BagItem) -> Option<i32> {
    let std = ctx.catalog.get(existing.index);
    if let Some(t) = std {
        if std_mode_in_map(t.std_mode) && !ctx.user_unlock_durg && existing.desc[7] != 0 {
            return Some(-4);
        }
        // oracle 对 stdItem20 == null 会 NPE（`:425`）；本实现按"模板缺失 ⇒ 跳过该分支"，
        // 与"服务端不持有索引缺失的装备"这一可达形态等价（登记在卡）。
        if !ctx.user_unlock_durg && (t.item_desc & 2) != 0 {
            return Some(-4);
        }
        if (t.item_desc & 4) != 0 {
            return Some(-4);
        }
    }
    if ctx
        .disable_take_off_keys
        .contains(&existing.index.wrapping_sub(1))
    {
        return Some(0);
    }
    None
}

/// `PlayObject.ClientTakeOnItems(btWhere, nItemIdx, sItemName)` 的纯函数等价物。
///
/// 失败码（oracle `n18`）：`0` = 未找到/名称不符/`InDisableTakeOffList`；`-1` = `CheckUserItems`
/// 或 `CheckTakeOnItems`/`CheckItemBindUse` 失败；`-4` = 已占槽的不可卸守卫；`1` = 成功。
#[must_use]
pub fn take_on(
    state: &mut EquipState,
    ctx: &TakeOnContext<'_>,
    slot: u8,
    make_index: i32,
    item_name: &str,
) -> EquipOutcome {
    let mut n18: i32 = 0;
    let mut swapped_out = None;

    // 在 ItemList 里按 (MakeIndex, 名称) 找实例（oracle 的 for 循环 + break）。
    let found = state.item_list.iter().position(|it| {
        if it.make_index != make_index {
            return false;
        }
        // oracle 要求 `GetStdItem(...) != null` 才做名称比较（模板缺失则该实例不被选中）。
        if ctx.catalog.get(it.index).is_none() {
            return false;
        }
        item_display_name(ctx.catalog, it).eq_ignore_ascii_case(item_name)
    });

    if let Some(item_index) = found {
        let user_item = state.item_list[item_index].clone();
        let std_item = ctx.catalog.get(user_item.index).cloned();
        if let Some(std) = std_item {
            if check_user_items(slot, std.std_mode) {
                let client_item = effective_item(&std, &user_item);
                if check_take_on_items(ctx, state, slot, &client_item) {
                    // CheckItemBindUse 恒 true（本片作用域，见模块文档）。
                    if usize::from(slot) <= 12 {
                        let slot_idx = usize::from(slot);
                        // 已占槽守卫（oracle：btWhere <= 12；合法槽位恒真）。
                        if let Some(existing) = state.use_items[slot_idx].clone() {
                            if existing.index > 0 {
                                if let Some(code) = occupied_slot_guard(ctx, &existing) {
                                    return fail(SM_TAKEON_FAIL, code);
                                }
                                swapped_out = Some(existing);
                            }
                        }
                        // 装卸迁移。
                        let mut user_item = user_item;
                        if std_mode_in_map(std.std_mode) && user_item.desc[8] != 0 {
                            user_item.desc[8] = 0;
                        }
                        state.item_list.remove(item_index);
                        if let Some(out) = swapped_out.clone() {
                            if state.item_list.len() < MAX_BAG_ITEM {
                                state.item_list.push(out);
                            }
                        }
                        // `Operate.cs:460-478`：只有衣服（`StdMode 10/11`）在成功后发
                        // `SM_UPDATEITEM`（按 `Shape` 选 plain/WithLevel/ByJob）。
                        let update_item = if std.std_mode == 10 || std.std_mode == 11 {
                            Some(user_item.clone())
                        } else {
                            None
                        };
                        state.use_items[slot_idx] = Some(user_item);
                        n18 = 1;
                        let f = feature_to_long(ctx, state);
                        return EquipOutcome {
                            code: n18,
                            frames: vec![Frame {
                                ident: SM_TAKEON_OK,
                                recog: f,
                                param: ctx.feature_ex(),
                                tag: 0,
                                series: 0,
                            }],
                            swapped_out,
                            update_item,
                            recalc: true,
                        };
                    }
                } else {
                    n18 = -1;
                }
            } else {
                n18 = -1;
            }
        }
    }
    fail(SM_TAKEON_FAIL, n18)
}

/// `PlayObject.ClientTakeOffItems(btWhere, nItemIdx, sItemName)` 的纯函数等价物。
///
/// 失败码（oracle `n10`）：`-1` = `Dealing` 或 `btWhere >= 13`；`-2` = 槽为空（`Index == 0`）；
/// `0` = `MakeIndex`/名称不符或 `InDisableTakeOffList`；`-3` = 背包满（46）；`-4` = 不可卸守卫；
/// `1` = 成功。
#[must_use]
pub fn take_off(
    state: &mut EquipState,
    ctx: &TakeOnContext<'_>,
    slot: u8,
    make_index: i32,
    item_name: &str,
) -> EquipOutcome {
    let n10: i32 = 0;
    if ctx.dealing || usize::from(slot) >= USE_ITEMS_LEN {
        return fail(SM_TAKEOFF_FAIL, -1);
    }
    let slot_idx = usize::from(slot);
    // oracle 直接 `UseItems[btWhere].Index > 0`；`null` 槽在线上不可达（客户端只在已占槽发卸下），
    // 本实现按"空槽 ⇒ n10=-2"处理（不 panic；登记在卡）。
    let Some(existing) = state.use_items[slot_idx].clone() else {
        return fail(SM_TAKEOFF_FAIL, -2);
    };
    if existing.index == 0 {
        return fail(SM_TAKEOFF_FAIL, -2);
    }
    if existing.make_index == make_index {
        if let Some(code) = occupied_slot_guard(ctx, &existing) {
            return fail(SM_TAKEOFF_FAIL, code);
        }
        if item_display_name(ctx.catalog, &existing).eq_ignore_ascii_case(item_name) {
            if state.item_list.len() < MAX_BAG_ITEM {
                state.item_list.push(existing.clone());
                state.use_items[slot_idx] = None;
                let f = feature_to_long(ctx, state);
                // ⚠️ oracle 的成功分支**没有** `n10 = 1`（`PlayObject.Operate.cs:540-556`）⇒ 控制流
                // 落回 `FailExit:` 再发一条 `SM_TAKEOFF_FAIL(Recog=0)`。**必须原样复刻**（这是 oracle
                // 的既有形态，不是本片引入的）：成功 = OK 帧 + FAIL(0) 帧，`code` 停在 0。
                return EquipOutcome {
                    code: 0,
                    frames: vec![
                        Frame {
                            ident: SM_TAKEOFF_OK,
                            recog: f,
                            param: ctx.feature_ex(),
                            tag: 0,
                            series: 0,
                        },
                        Frame {
                            ident: SM_TAKEOFF_FAIL,
                            recog: 0,
                            param: 0,
                            tag: 0,
                            series: 0,
                        },
                    ],
                    swapped_out: Some(existing),
                    update_item: None,
                    recalc: true,
                };
            }
            return fail(SM_TAKEOFF_FAIL, -3);
        }
    }
    fail(SM_TAKEOFF_FAIL, n10)
}

/// `GetUpgradeStdItem` 的**本片恒等拷贝**（`CopyItemToClientItem` 的字段搬运；见模块文档）。
fn effective_item(std: &StdItem, _user_item: &BagItem) -> StdItem {
    std.clone()
}

fn fail(ident: u16, code: i32) -> EquipOutcome {
    EquipOutcome {
        code,
        frames: vec![Frame {
            ident,
            recog: code,
            param: 0,
            tag: 0,
            series: 0,
        }],
        swapped_out: None,
        update_item: None,
        recalc: false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::{ItemInstance, NakedAbility};
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/bag_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn u8v(v: &Value) -> u8 {
        v.as_u64().unwrap() as u8
    }
    fn u16v(v: &Value) -> u16 {
        v.as_u64().unwrap() as u16
    }
    fn i32v(v: &Value) -> i32 {
        v.as_i64().unwrap() as i32
    }

    fn template(v: &Value) -> StdItem {
        StdItem {
            name: v["name"].as_str().unwrap().to_string(),
            std_mode: u8v(&v["std_mode"]),
            shape: u8v(&v["shape"]),
            weight: u8v(&v["weight"]),
            ani_count: 0,
            special_pwr: 0,
            item_desc: u8v(&v["item_desc"]),
            looks: 0,
            dura_max: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            need: u8v(&v["need"]),
            need_level: u8v(&v["need_level"]),
            need_identify: 0,
            price: 0,
            stock: 0,
            atk_spd: 0,
            agility: 0,
            accurate: 0,
            mg_avoid: 0,
            strong: 0,
            undead: 0,
            hp_add: 0,
            mp_add: 0,
            exp_add: 0,
            eff_type1: 0,
            eff_rate1: 0,
            eff_value1: 0,
            eff_type2: 0,
            eff_rate2: 0,
            eff_value2: 0,
            slowdown: 0,
            tox: 0,
            tox_avoid: 0,
            unique_item: 0,
            overlap_item: 0,
            light: 0,
            item_type: 0,
            item_set: 0,
            reference: String::new(),
        }
    }

    fn catalog(doc: &Value) -> ability::StdItemCatalog {
        let mut c = ability::StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().unwrap() {
            c.insert(k.parse::<u16>().unwrap(), template(v));
        }
        c
    }

    fn ability(v: &Value) -> Ability {
        Ability {
            level: u8v(&v["level"]),
            reserved1: u8v(&v["reserved1"]),
            ac: u16v(&v["ac"]),
            mac: u16v(&v["mac"]),
            dc: u16v(&v["dc"]),
            mc: u16v(&v["mc"]),
            sc: u16v(&v["sc"]),
            hp: u16v(&v["hp"]),
            mp: u16v(&v["mp"]),
            max_hp: u16v(&v["max_hp"]),
            max_mp: u16v(&v["max_mp"]),
            reserved2: v["reserved2"].as_i64().unwrap() as i16,
            exp_count: u8v(&v["exp_count"]),
            exp_max_count: u8v(&v["exp_max_count"]),
            exp: i32v(&v["exp"]),
            max_exp: i32v(&v["max_exp"]),
            weight: u16v(&v["weight"]),
            max_weight: u16v(&v["max_weight"]),
            wear_weight: u8v(&v["wear_weight"]),
            max_wear_weight: u8v(&v["max_wear_weight"]),
            hand_weight: u8v(&v["hand_weight"]),
            max_hand_weight: u8v(&v["max_hand_weight"]),
        }
    }

    fn bag_item(v: &Value) -> BagItem {
        let mut desc = [0u8; 14];
        for (i, d) in v["desc"].as_array().unwrap().iter().enumerate() {
            desc[i] = u8v(d);
        }
        BagItem {
            make_index: i32v(&v["make_index"]),
            index: u16v(&v["index"]),
            dura: u16v(&v["dura"]),
            dura_max: u16v(&v["dura_max"]),
            desc,
        }
    }

    fn bag_items(v: &Value) -> Vec<BagItem> {
        v.as_array().unwrap().iter().map(bag_item).collect()
    }

    fn use_items(v: &Value) -> Vec<Option<BagItem>> {
        v.as_array()
            .unwrap()
            .iter()
            .map(|x| if x.is_null() { None } else { Some(bag_item(x)) })
            .collect()
    }

    fn frame(v: &Value) -> Frame {
        Frame {
            ident: u16v(&v["ident"]),
            recog: i32v(&v["recog"]),
            param: u16v(&v["param"]),
            tag: u16v(&v["tag"]),
            series: u16v(&v["series"]),
        }
    }

    fn frames(v: &Value) -> Vec<Frame> {
        v.as_array().unwrap().iter().map(frame).collect()
    }

    fn make_ctx<'a>(
        doc: &'a Value,
        case: &'a Value,
        cat: &'a ability::StdItemCatalog,
        wabil: &'a Ability,
    ) -> TakeOnContext<'a> {
        let c = &case["ctx"];
        let keys: Vec<u16> = c["disable_take_off_keys"]
            .as_array()
            .map(|a| a.iter().map(u16v).collect())
            .unwrap_or_default();
        // 借用：把 disable keys 存进 case 的临时 Vec 不现实；这里改用泄漏到进程生命周期的
        // Vec（测试进程一次性，够用）。
        let leaked: &'static [u16] = Box::leak(keys.into_boxed_slice());
        let _ = doc;
        TakeOnContext {
            catalog: cat,
            job: u8v(&c["job"]),
            gender: u8v(&c["gender"]),
            hair: u8v(&c["hair"]),
            abil_level: u8v(&c["abil_level"]),
            re_level: u8v(&c["re_level"]),
            credit_point: u16v(&c["credit_point"]),
            member_type: u8v(&c["member_type"]),
            member_level: u8v(&c["member_level"]),
            has_guild: c["has_guild"].as_bool().unwrap(),
            guild_rank_no: u8v(&c["guild_rank_no"]),
            is_castle_member: c["is_castle_member"].as_bool().unwrap(),
            user_unlock_durg: c["user_unlock_durg"].as_bool().unwrap(),
            dealing: c["dealing"].as_bool().unwrap(),
            disable_take_off_keys: leaked,
            on_horse: c["on_horse"].as_bool().unwrap(),
            horse_type: u8v(&c["horse_type"]),
            dress_eff_type: u8v(&c["dress_eff_type"]),
            wabil,
        }
    }

    /// 夹具里 `expect.frames` 含探针截获的**全部**出帧（含 `SM_ADDITEM`/`SM_UPDATEITEM` 等
    /// 接线层帧）；本模块只产出 `SM_TAKEON_OK/FAIL`(615/616) 与 `SM_TAKEOFF_OK/FAIL`(619/620)。
    fn expected_frames(case: &Value) -> Vec<Frame> {
        frames(&case["expect"]["frames"])
            .into_iter()
            .filter(|f| {
                matches!(
                    f.ident,
                    SM_TAKEON_OK | SM_TAKEON_FAIL | SM_TAKEOFF_OK | SM_TAKEOFF_FAIL
                )
            })
            .collect()
    }

    /// 夹具每条用例：状态（UseItems/ItemList）、出帧（ident/recog/param/tag/series）、
    /// 换下物品是否存在（`swapped_out` ↔ 探针截获的 `SM_ADDITEM`）、成功时重算后的 `WAbil`
    /// 都必须与 oracle 逐字段相同。
    #[test]
    fn matches_oracle_bag_fixture() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = doc["cases"].as_array().expect("cases 数组");
        assert!(cases.len() >= 12, "夹具用例数异常：{}", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let wabil_pre = ability(&case["wabil"]);
            let ctx = make_ctx(&doc, case, &cat, &wabil_pre);
            let mut state = EquipState::from_slots(
                use_items(&case["use_items"]),
                bag_items(&case["item_list"]),
            );
            let req = &case["request"];
            let slot = u8v(&req["slot"]);
            let make_index = i32v(&req["make_index"]);
            let item_name = req["name"].as_str().unwrap();

            let out = if case["action"] == "take_on" {
                take_on(&mut state, &ctx, slot, make_index, item_name)
            } else {
                take_off(&mut state, &ctx, slot, make_index, item_name)
            };

            assert_eq!(
                out.frames,
                expected_frames(case),
                "{name}: 出帧与 oracle 不符"
            );
            assert_eq!(out.code, i32v(&case["expect"]["code"]), "{name}: 错误码");
            assert_eq!(
                out.swapped_out.is_some(),
                case["expect"]["has_add_item"].as_bool().unwrap(),
                "{name}: 换下物品（SM_ADDITEM 存在性）"
            );
            assert_eq!(
                state.use_items,
                use_items(&case["expect"]["use_items"]),
                "{name}: UseItems 与 oracle 不符"
            );
            assert_eq!(
                state.item_list,
                bag_items(&case["expect"]["item_list"]),
                "{name}: ItemList 与 oracle 不符"
            );

            // 成功分支 oracle 会 `RecalcAbilitys()`；本模块据此复算并与探针实测的 WAbil 对账。
            if out.recalc {
                let bag: Vec<u16> = state.item_list.iter().map(|i| i.index).collect();
                let mut use_inst: Vec<Option<ItemInstance>> = vec![None; USE_ITEMS_LEN];
                for (i, s) in state.use_items.iter().enumerate() {
                    if let Some(it) = s {
                        use_inst[i] = Some(ItemInstance {
                            index: it.index,
                            dura: it.dura,
                        });
                    }
                }
                let abil = ability(&case["abil"]);
                let ba = &case["bonus_abil"];
                let bonus = NakedAbility {
                    dc: u16v(&ba["dc"]),
                    mc: u16v(&ba["mc"]),
                    sc: u16v(&ba["sc"]),
                    ac: u16v(&ba["ac"]),
                    mac: u16v(&ba["mac"]),
                    hp: u16v(&ba["hp"]),
                    mp: u16v(&ba["mp"]),
                    hit: u8v(&ba["hit"]),
                    speed: i32v(&ba["speed"]),
                    reserved: u8v(&ba["reserved"]),
                };
                let job = u8v(&case["ctx"]["job"]);
                let key = match job {
                    1 => "bonus_tick_wizard",
                    2 => "bonus_tick_taos",
                    _ => "bonus_tick_warr",
                };
                let tick = &doc["defaults"][key];
                let inputs = ability::RecalcInputs {
                    job,
                    abil: &abil,
                    bonus_abil: &bonus,
                    bonus_tick_hit: u8v(&tick["hit"]),
                    bonus_tick_speed: i32v(&tick["speed"]),
                    cur_hp: wabil_pre.hp,
                    cur_mp: wabil_pre.mp,
                    use_items: &use_inst,
                    bag: &bag,
                };
                let got = ability::recalc_abilitys(&cat, &inputs);
                assert_eq!(
                    got.wabil,
                    ability(&case["expect"]["wabil"]),
                    "{name}: 重算后的 WAbil 与 oracle 不符"
                );
            }
        }
    }

    /// `InDisableTakeOffList` 命中时 oracle **不设错误码** ⇒ FAIL 帧的 `Recog` 必须是
    /// **0 而不是 -4**（改坏这一支必红；这是最容易抄错的口径之一）。
    #[test]
    fn disable_take_off_branch_keeps_code_zero() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "takeon_fail_disable_takeoff")
            .expect("夹具必须含 takeon_fail_disable_takeoff");
        let wabil = ability(&case["wabil"]);
        let ctx = make_ctx(&doc, case, &cat, &wabil);
        let mut state =
            EquipState::from_slots(use_items(&case["use_items"]), bag_items(&case["item_list"]));
        let out = take_on(
            &mut state,
            &ctx,
            1,
            i32v(&case["request"]["make_index"]),
            "w",
        );
        assert_eq!(out.code, 0, "InDisableTakeOffList 分支必须保持 n18=0");
        assert_eq!(out.frames[0].ident, SM_TAKEON_FAIL);
        assert_eq!(out.frames[0].recog, 0, "FAIL 帧 Recog 必须是 0（不是 -4）");
    }

    /// `CheckUserItems` 的槽位 ↔ StdMode 白名单（改坏任一分支必红）。
    #[test]
    fn check_user_items_whitelist() {
        assert!(check_user_items(0, 10));
        assert!(check_user_items(0, 11));
        assert!(!check_user_items(0, 5));
        assert!(check_user_items(1, 5));
        assert!(check_user_items(1, 6));
        assert!(!check_user_items(1, 10));
        assert!(check_user_items(7, 22));
        assert!(check_user_items(8, 23));
        assert!(!check_user_items(13, 10));
    }

    /// **G-2**：穿上衣服（`StdMode 10/11`）后 `EquipOutcome.update_item` 必须非空（接线层据此发
    /// `SM_UPDATEITEM(203)`）；非衣服槽（头盔 `15`）必须为空。
    #[test]
    fn take_on_dress_sets_update_item_only_for_dress_std_modes() {
        let mut cat = crate::ability::StdItemCatalog::new();
        cat.insert(
            100,
            mir2_data::StdItem {
                name: "dress".into(),
                std_mode: 10,
                shape: 0,
                weight: 1,
                ..mir2_data::StdItem::default()
            },
        );
        cat.insert(
            101,
            mir2_data::StdItem {
                name: "helmet".into(),
                std_mode: 15,
                shape: 0,
                weight: 1,
                ..mir2_data::StdItem::default()
            },
        );
        let wabil = Ability {
            max_hand_weight: 100,
            max_wear_weight: 100,
            ..Ability::default()
        };
        let ctx = TakeOnContext {
            catalog: &cat,
            job: 0,
            gender: 0,
            hair: 0,
            abil_level: 10,
            re_level: 0,
            credit_point: 0,
            member_type: 0,
            member_level: 0,
            has_guild: false,
            guild_rank_no: 0,
            is_castle_member: false,
            user_unlock_durg: false,
            dealing: false,
            disable_take_off_keys: &[],
            on_horse: false,
            horse_type: 0,
            dress_eff_type: 0,
            wabil: &wabil,
        };
        let mut state = EquipState {
            use_items: vec![None; USE_ITEMS_LEN],
            item_list: vec![BagItem {
                make_index: 7,
                index: 100,
                dura: 1,
                dura_max: 2,
                desc: [0; 14],
            }],
        };
        let out = take_on(&mut state, &ctx, 0, 7, "dress");
        assert_eq!(out.code, 1, "衣服穿上成功");
        assert_eq!(
            out.update_item.as_ref().map(|i| i.make_index),
            Some(7),
            "衣服 ⇒ SM_UPDATEITEM 输入"
        );

        let mut state2 = EquipState {
            use_items: vec![None; USE_ITEMS_LEN],
            item_list: vec![BagItem {
                make_index: 8,
                index: 101,
                dura: 1,
                dura_max: 2,
                desc: [0; 14],
            }],
        };
        let out2 = take_on(&mut state2, &ctx, 4, 8, "helmet");
        assert_eq!(out2.code, 1, "头盔穿上成功");
        assert!(
            out2.update_item.is_none(),
            "非衣服（StdMode 15）不产 SM_UPDATEITEM"
        );
    }

    /// **PL-1 复验 F8**：`ItemsNeedContext` 的字段宽度必须与 oracle 一致
    /// （`PlayObject.MemberType:int`、`GuildRankNo:short`）。窄化成 u8 在负值/超 255 的
    /// 行会/会员接线输入上会与 oracle 分岔。
    ///
    /// 牙：`MemberType=-1`（Need=82 应 false、Need=8 应 true）、`MemberType=300`
    /// （Need=81 的 LoWord 是 byte ⇒ 恒不等）、`GuildRankNo=300`（i16 可表示、≠1）。
    #[test]
    fn items_need_context_field_widths_match_oracle() {
        let negative = ItemsNeedContext {
            member_type: -1,
            ..ItemsNeedContext::default()
        };
        assert!(
            !check_items_need(&negative, 82, 0),
            "MemberType=-1 < LoWord(0) ⇒ Need=82 false（u8 会读成 255 ⇒ 假 true）"
        );
        assert!(
            check_items_need(&negative, 8, 0),
            "MemberType=-1 != 0 ⇒ Need=8 true（oracle 只判 ==0）"
        );

        let over_byte = ItemsNeedContext {
            member_type: 300,
            ..ItemsNeedContext::default()
        };
        assert!(
            !check_items_need(&over_byte, 81, 255),
            "MemberType=300 不可能等于 LoWord(byte<256) ⇒ Need=81 false"
        );

        let rank = ItemsNeedContext {
            has_guild: true,
            guild_rank_no: 300,
            ..ItemsNeedContext::default()
        };
        assert!(
            !check_items_need(&rank, 60, 0),
            "GuildRankNo=300（short 可表示）!= 1 ⇒ Need=60 false"
        );
    }

    /// **PL-1 / P21**：`CheckItemsNeed` 的 7 值子集逐分支（`PlayObject.Base.cs:2832-2882`）。
    ///
    /// 红检：把 `_ => true` 改成 `_ => false` ⇒ `other_needs_always_true` 与
    /// `need_zero_low_level_is_not_checked` 必红；把 8 的 `member_type != 0` 改成 `== 0`
    /// ⇒ `need_8` 必红。
    #[test]
    fn check_items_need_is_the_seven_value_subset() {
        let none = ItemsNeedContext::default();
        assert!(!check_items_need(&none, 6, 0), "Need=6 无行会 ⇒ false");
        assert!(!check_items_need(&none, 60, 0), "Need=60 无行会 ⇒ false");
        assert!(!check_items_need(&none, 7, 0), "Need=7 非城堡成员 ⇒ false");
        assert!(
            !check_items_need(&none, 70, 0),
            "Need=70 非城堡成员 ⇒ false"
        );
        assert!(
            !check_items_need(&none, 8, 0),
            "Need=8 MemberType==0 ⇒ false"
        );
        // 81：MemberType 必须**等于** LoWord(NeedLevel)；HiWord=0 ⇒ MemberLevel 不参与。
        let ctx = ItemsNeedContext {
            member_type: 3,
            member_level: 0,
            ..ItemsNeedContext::default()
        };
        assert!(
            check_items_need(&ctx, 81, 3),
            "Need=81 MemberType==3 ⇒ true"
        );
        assert!(
            !check_items_need(&ctx, 81, 2),
            "Need=81 MemberType!=LoWord ⇒ false"
        );
        assert!(
            check_items_need(&ctx, 81, 3),
            "Need=81 MemberLevel=0 不挡（HiWord(byte)=0，`< 0` 恒 false）"
        );
        // 82：MemberType >= LoWord；HiWord=0 同上。
        assert!(check_items_need(&ctx, 82, 3), "Need=82 ==LoWord ⇒ true");
        assert!(check_items_need(&ctx, 82, 2), "Need=82 >LoWord ⇒ true");
        assert!(!check_items_need(&ctx, 82, 4), "Need=82 <LoWord ⇒ false");
        // 行会/城堡两支的正例。
        let g = ItemsNeedContext {
            has_guild: true,
            guild_rank_no: 1,
            is_castle_member: true,
            ..ItemsNeedContext::default()
        };
        assert!(check_items_need(&g, 6, 0));
        assert!(check_items_need(&g, 60, 0));
        assert!(check_items_need(&g, 7, 0), "Need=7 只看城堡成员");
        assert!(check_items_need(&g, 70, 0));
        assert!(
            !check_items_need(
                &ItemsNeedContext {
                    guild_rank_no: 2,
                    ..g
                },
                60,
                0
            ),
            "Need=60 非会长 ⇒ false"
        );
        assert!(
            check_items_need(
                &ItemsNeedContext {
                    has_guild: false,
                    ..g
                },
                7,
                0
            ),
            "Need=7 不看 MyGuild（只有城堡成员门）"
        );
        assert!(!check_items_need(&g, 8, 0), "Need=8 仍看 MemberType");
        // 其它 Need 一律 true（含 Need=0 的等级不足）。
        assert!(check_items_need(&none, 0, 200), "Need=0 一律 true");
        assert!(check_items_need(&none, 1, 200));
        assert!(check_items_need(&none, 44, 200));
        assert!(check_items_need(&none, 99, 200));
    }
}
