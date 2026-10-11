//! 「装备物品 + 基础 `Abil` ⇒ `WAbil`(52) / `SubAbility`(752) / 外形 feature(50/41/213)」的纯函数接缝。
//!
//! # 这条接缝给谁用
//!
//! oracle 的 `52 SM_ABILITY` / `752 SM_SUBABILITY` 载荷来自 `PlayObject.RecalcAbilitys()`
//! （`src/M2Server/Player/PlayObject.cs:2008`）——它从**已装备物品 + buff + 技能**算出派生能力。
//! `openmir2-m2-enter-world-frames` 的入图帧序里有 `52/752`，接线前需要这一层的输入；
//! 同一份 `WAbil` 也用在装备/卸下的成功回帧（`SM_TAKEON_OK`/`SM_TAKEOFF_OK`，`PlayObject.Operate.cs:458/:547`）。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! PlayObject.RecalcAbilitys()                    PlayObject.cs:2008
//!   ├─ BaseObject.RecalcAbilitys()               BaseObject.Abil.cs:15
//!   │    ├─ WAbil = Abil.Clone(); HP/MP 保留旧值；Weight/WearWeight/HandWeight = 0
//!   │    ├─ AntiPoison = 0; AntiMagic = 1
//!   │    └─ RecalcHitSpeed()                     PlayObject.cs:3647  → HitPoint/SpeedPoint 基线
//!   ├─ for i in 0..13: UseItems[i]               称重 + ApplyItemParameters + ApplyItemParametersEx
//!   ├─ （套装/特殊 Shape 的巨型 else-if 块：**本片未做**，见下"未做清单"）
//!   ├─ WAbil.Weight = RecalcBagWeight()          BaseObject.cs:1356（背包物品重量和）
//!   └─ 尾部合成：SubAbility 7 项 + WAbil MaxHP/MaxMP/AC/MAC/DC/MC/SC
//! ```
//!
//! 尾部合成的口径（都会改变结果，别"顺手简化"）：
//!
//! 1. `AntiMagic = 1 + AddAbil.AntiMagic`，其余 6 项基线为 0（`BaseObject.Abil.cs:30-31`）。
//! 2. `HitPoint = RecalcHitSpeed 基线 + AddAbil.HIT`；`SpeedPoint = RecalcHitSpeed 基线 + AddAbil.SPEED`。
//!    `RecalcHitSpeed` 按 `Job`：战士/法师 `DEFHIT(5)+BonusAbil.Hit/tick.Hit`、`DEFSPEED(15)+BonusAbil.Speed/tick.Speed`；
//!    道士只设 `SpeedPoint = DEFSPEED + … + 3`（**不重置 HitPoint**）。
//! 3. `WAbil.MaxHP = Abil.MaxHP + AddAbil.HP`（`HP/MP` 是**调用前的当前值**，被保留）。
//! 4. `WAbil.AC/MAC/DC/MC/SC` 是 **word 相加**：低字节 = `Lo(AddAbil)+Lo(Abil)`（**不夹**），
//!    高字节 = `_MIN(255, Hi(AddAbil)+Hi(Abil))`，再 `MakeWord`（低字节溢出会串进高字节——oracle 原样）。
//!    ⚠️ 注意 `ApplyItemParameters` 内部的 `MakeWord((ushort)Lo+Lo, (ushort)Hi+Hi)` 用的是 **ushort 重载**
//!    （两侧都不夹、都不截断到 byte），与尾部合成的 byte 重载**不是一回事**。
//! 5. `WAbil.Weight = Σ GetStdItem(bag[i]).Weight`。
//! 6. `RecalcAdjusBonus()`（`PlayObject.Base.cs:1104`）在尾部合成**之前**改的是 `WAbil` 的
//!    DC/MC/SC/AC/MAC/MaxHP/MaxMP——随后被尾部合成**逐项覆盖** ⇒ 对最终 `WAbil` **无可观测影响**
//!    （本模块据此不实现它；这是 oracle 的既有形态，不是省事）。
//!
//! # 与 oracle 逐字段相同怎么保证
//!
//! 用例值由 `mir2-rs/tests/parity/m3/ability-probe`（引用 `src/M2Server` 真身、真跑
//! `PlayObject.RecalcAbilitys()`）实测产出，落 `mir2-rs/tests/parity/m3/fixtures/ability_vectors.json`；
//! 本模块的用例回读该夹具逐字段核对（改坏任一映射 ⇒ 必红）。生成命令（**在仓库根运行**）：
//!
//! ```text
//! dotnet build mir2-rs/tests/parity/m3/ability-probe -c Release
//! dotnet mir2-rs/tests/parity/m3/ability-probe/bin/Release/net8.0/ability-probe.dll \
//!   mir2-rs/tests/parity/m3/fixtures/ability_vectors.json
//! ```
//!
//! # 未做清单（本片作用域外，登记在卡 `openmir2-m3-bag-equip-alignment`）
//!
//! - **有效物品 = 模板**：`GetUpgradeStdItem`（`GameItemSystem.cs:731`）的 Desc 升级差量与各 `StdMode`
//!   变换未实现 ⇒ 传入本模块的 `StdItem` 必须是"已升级的有效物品"。探针 `GetUpgradeStdItem` 也是恒等拷贝，
//!   两侧同域（真正的升级差量由后续卡接）。
//! - **套装/特殊 Shape 的巨型 else-if 块**（`PlayObject.cs:2538-2849`）：未实现（普通物品不含这些 Shape）。
//! - **buff**（`M2Share.ActorBuffSystem`）、**技能对 HitPoint 的加成**（`MagicList`）：未实现。
//! - `ApplyItemParametersByJob`（`PlayObject.cs:4889`，只对龙装 Shape 生效）：未实现。
//! - `HitSpeed`/`Luck`/`Slowdown`/`Poison`/`WeaponStrong`/`UndeadPower`：不影响 `WAbil`/`SubAbility`，
//!   未纳入 `AddAbility`。

use std::collections::HashMap;

use mir2_data::StdItem;

use crate::feature;

/// `Ability`（= oracle `WAbil` / 角色记录 `Abil`；class：22 个成员，成员顺序 = C# 声明顺序）。
///
/// 定义在 **lib** 侧（不是 `record.rs`）：`ability.rs` 要产出它，而 `record.rs` 是 bin 模块。
/// `record.rs` 用 `pub use game_svr::ability::Ability;` 复用同一类型（线缆编解码仍留在 bin，
/// 因为它依赖那里的 `Reader`/`Writer`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ability {
    pub level: u8,
    pub reserved1: u8,
    pub ac: u16,
    pub mac: u16,
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub hp: u16,
    pub mp: u16,
    pub max_hp: u16,
    pub max_mp: u16,
    pub reserved2: i16,
    pub exp_count: u8,
    pub exp_max_count: u8,
    pub exp: i32,
    pub max_exp: i32,
    pub weight: u16,
    pub max_weight: u16,
    pub wear_weight: u8,
    pub max_wear_weight: u8,
    pub hand_weight: u8,
    pub max_hand_weight: u8,
}

/// `NakedAbility`（class：10 个成员）——角色记录 `BonusAbil` / `Config.BonusAbilofXxx`。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NakedAbility {
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub ac: u16,
    pub mac: u16,
    pub hp: u16,
    pub mp: u16,
    pub hit: u8,
    pub speed: i32,
    pub reserved: u8,
}

/// `ItemLocation`（`src/OpenMir2/Enums/ItemLocation.cs`）——`UseItems` 的 13 个槽位。
pub const SLOT_DRESS: usize = 0;
pub const SLOT_WEAPON: usize = 1;
/// 右手（照明物）：与武器同属"腕力"槽。
pub const SLOT_RIGHTHAND: usize = 2;
pub const SLOT_NECKLACE: usize = 3;
pub const SLOT_HELMET: usize = 4;
pub const SLOT_ARMRING_L: usize = 5;
pub const SLOT_ARMRING_R: usize = 6;
pub const SLOT_RING_L: usize = 7;
pub const SLOT_RING_R: usize = 8;
pub const SLOT_BUJUK: usize = 9;
pub const SLOT_BELT: usize = 10;
pub const SLOT_BOOTS: usize = 11;
pub const SLOT_CHARM: usize = 12;
/// `UseItems` 数组长度（oracle `new UserItem[13]`）。
pub const USE_ITEMS_LEN: usize = 13;

/// `MessageSettings.DEFHIT`（`src/Modules/SystemModule/MessageSettings.cs:10`）。
pub const DEF_HIT: u16 = 5;
/// `MessageSettings.DEFSPEED`（同上 `:11`）。
pub const DEF_SPEED: u16 = 15;

/// `EfftypeConst.EFFTYPE_TWOHAND_WEHIGHT_ADD`（`src/M2Server/Actor/ActorConst.cs:37`）。
const EFFTYPE_TWOHAND_WEIGHT_ADD: u8 = 1;
/// `EfftypeConst.EFFTYPE_EQUIP_WHEIGHT_ADD`。
const EFFTYPE_EQUIP_WEIGHT_ADD: u8 = 2;
/// `EfftypeConst.EFFTYPE_BAG_WHIGHT_ADD`。
const EFFTYPE_BAG_WEIGHT_ADD: u8 = 4;
/// `EfftypeConst.EFFTYPE_HP_MP_ADD`。
const EFFTYPE_HP_MP_ADD: u8 = 5;

/// `HUtil32.MakeWord(byte low, byte high)` = `high << 8 | low`。
const fn make_word_byte(low: u8, high: u8) -> u16 {
    ((high as u16) << 8) | (low as u16)
}

/// `HUtil32.MakeWord(ushort bLow, ushort bHigh)` = `bLow | bHigh << 8`（**两侧都不夹/不截断**）。
const fn make_word_u16(low: u16, high: u16) -> u16 {
    low | (high << 8)
}

/// `HUtil32.LoByte(ushort)`。
const fn lo_byte(v: u16) -> u8 {
    (v & 0x00FF) as u8
}

/// `HUtil32.HiByte(ushort)`。
const fn hi_byte(v: u16) -> u8 {
    ((v >> 8) & 0x00FF) as u8
}

/// `OpenMir2.Data.StdItem` 的 **Index → 模板**查表接缝（`SystemShare.ItemSystem.GetStdItem`）。
pub trait ItemCatalog {
    /// 物品 Index（>0）→ 模板；缺失返回 `None`（oracle `GetStdItem` 返回 null）。
    fn get(&self, index: u16) -> Option<&StdItem>;

    /// `StdItem.Shape`（`GetFeature`/套装判定用）。
    fn shape(&self, index: u16) -> Option<u8> {
        self.get(index).map(|t| t.shape)
    }

    /// `SystemShare.ItemSystem.GetStdItemName(index)`（= 模板名；`CustomItemSystem.GetItemName` 的非自定义分支）。
    fn name(&self, index: u16) -> Option<&str> {
        self.get(index).map(|t| t.name.as_str())
    }
}

/// `Vec<StdItem>` 的目录实现（下标 = Index，等价于 `ItemSystem` 的数组语义）。
#[derive(Debug, Clone, Default)]
pub struct StdItemCatalog {
    items: HashMap<u16, StdItem>,
}

impl StdItemCatalog {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// 从 `(index, StdItem)` 列表建目录。
    #[must_use]
    pub fn from_items(items: impl IntoIterator<Item = (u16, StdItem)>) -> Self {
        Self {
            items: items.into_iter().collect(),
        }
    }

    pub fn insert(&mut self, index: u16, item: StdItem) {
        self.items.insert(index, item);
    }
}

impl ItemCatalog for StdItemCatalog {
    fn get(&self, index: u16) -> Option<&StdItem> {
        self.items.get(&index)
    }
}

/// `UseItems[i]` 里的一个物品实例（本片只用到 `Index`/`Dura`；`Desc` 升级差量未做，见模块文档）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ItemInstance {
    pub index: u16,
    pub dura: u16,
}

/// `PlayObject.GetFeature(null)` 的**装备集取值口径**：把 `UseItems[Dress/Weapon]` 的
/// `Index` 查表成 `StdItem.Shape` 后交给 [`crate::feature::human_feature`]。
///
/// `Index == 0` / 模板缺失 / 槽为空 ⇒ 该槽按 `None`（oracle 逐行，见 `PlayObject.cs:3393-3420`）。
#[must_use]
pub fn equipped_feature(
    catalog: &dyn ItemCatalog,
    use_items: &[Option<ItemInstance>],
    gender: u8,
    hair: u8,
) -> i32 {
    let shape_of = |slot: usize| -> Option<u8> {
        let inst = use_items.get(slot).and_then(Option::as_ref)?;
        if inst.index == 0 {
            return None;
        }
        catalog.get(inst.index).map(|t| t.shape)
    };
    feature::human_feature(shape_of(SLOT_DRESS), shape_of(SLOT_WEAPON), gender, hair)
}

/// `OpenMir2.Data.AddAbility` 的**本片子集**：只保留会影响 `WAbil`/`SubAbility` 的字段。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AddAbility {
    pub hp: u16,
    pub mp: u16,
    /// `HIT`（准确）→ `HitPoint`。
    pub hit: u16,
    /// `SPEED`（敏捷）→ `SpeedPoint`。
    pub speed: u16,
    pub ac: u16,
    pub mac: u16,
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub anti_poison: u16,
    pub anti_magic: u16,
    pub poison_recover: u16,
    pub health_recover: u16,
    pub spell_recover: u16,
}

/// `752 SM_SUBABILITY` 的 7 个字段（类型照 oracle：`AntiMagic/PoisonRecover/HealthRecover/SpellRecover`
/// 是 `ushort`，`HitPoint/SpeedPoint/AntiPoison` 是 `byte`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SubAbility {
    pub anti_magic: u16,
    pub hit_point: u8,
    pub speed_point: u8,
    pub anti_poison: u8,
    pub poison_recover: u16,
    pub health_recover: u16,
    pub spell_recover: u16,
}

impl SubAbility {
    /// 线上 7 个单字节通道（`smframe::sm_subability` 的入参；
    /// `MakeWord(x, 0)` 只取低字节 ⇒ ushort 项按低字节截断）。
    #[must_use]
    pub fn to_frame_bytes(self) -> [u8; 7] {
        [
            lo_byte(self.anti_magic),
            self.hit_point,
            self.speed_point,
            self.anti_poison,
            lo_byte(self.poison_recover),
            lo_byte(self.health_recover),
            lo_byte(self.spell_recover),
        ]
    }
}

/// [`recalc_abilitys`] 的输入（一次 `RecalcAbilitys()` 的全部观测输入）。
#[derive(Debug, Clone, Copy)]
pub struct RecalcInputs<'a> {
    /// `PlayerJob`：0 = 战士、1 = 法师、2 = 道士（其余走 `switch` 缺省分支）。
    pub job: u8,
    /// `BaseObject.Abil`（角色记录里的基础能力，`record::CharacterData.abil`）。
    pub abil: &'a Ability,
    /// `PlayObject.BonusAbil`（角色记录 `bonus_abil`）。
    pub bonus_abil: &'a NakedAbility,
    /// `Config.BonusAbilofXxx.Hit`（`RecalcHitSpeed` 的除数）。
    pub bonus_tick_hit: u8,
    /// `Config.BonusAbilofXxx.Speed`（`RecalcHitSpeed` 的除数）。
    pub bonus_tick_speed: i32,
    /// 调用前的 `WAbil.HP`（被保留）。
    pub cur_hp: u16,
    /// 调用前的 `WAbil.MP`（被保留）。
    pub cur_mp: u16,
    /// `UseItems[13]`（未装备的槽传 `None`；装备但 `Index == 0` 的槽按 oracle 忽略）。
    pub use_items: &'a [Option<ItemInstance>],
    /// 背包物品 Index 列表（`ItemList`，只用于 `WAbil.Weight`）。
    pub bag: &'a [u16],
}

/// [`recalc_abilitys`] 的输出。
#[derive(Debug, Clone, PartialEq)]
pub struct RecalcOutput {
    /// oracle `WAbil`（可直接喂 `record::encode_ability`）。
    pub wabil: Ability,
    /// oracle 752 的 7 个字段。
    pub sub: SubAbility,
    /// `AddAbil`（调试/红检用；不进载荷）。
    pub add: AddAbility,
}

/// `PlayObject.RecalcAbilitys()` 的纯函数等价物（本片作用域，见模块文档「未做清单」）。
#[must_use]
pub fn recalc_abilitys(catalog: &dyn ItemCatalog, inputs: &RecalcInputs<'_>) -> RecalcOutput {
    let mut add = AddAbility::default();
    // `WAbil = Abil.Clone()` + 三个重量清零（`BaseObject.Abil.cs:19-27`）。
    let mut max_weight = inputs.abil.max_weight;
    let mut max_wear_weight = inputs.abil.max_wear_weight;
    let mut max_hand_weight = inputs.abil.max_hand_weight;
    let mut wear_weight: u8 = 0;
    let mut hand_weight: u8 = 0;

    for (slot, cell) in inputs.use_items.iter().enumerate() {
        let Some(inst) = cell else { continue };
        if inst.index == 0 {
            continue;
        }
        let Some(tpl) = catalog.get(inst.index) else {
            continue;
        };
        if inst.dura == 0 {
            // `PlayObject.cs:2107-2120`：Dura==0 只计入重量，不参与属性累加。
            add_weight(slot, tpl.weight, &mut wear_weight, &mut hand_weight);
            continue;
        }
        apply_item_parameters(&mut add, tpl);
        apply_item_parameters_ex(
            tpl,
            &mut max_weight,
            &mut max_wear_weight,
            &mut max_hand_weight,
        );
        add_weight(slot, tpl.weight, &mut wear_weight, &mut hand_weight);
    }

    let (base_hit, base_speed) = hit_speed_base(inputs);
    let sub = SubAbility {
        anti_magic: 1u16.wrapping_add(add.anti_magic),
        hit_point: base_hit.wrapping_add(add.hit as u8),
        speed_point: base_speed.wrapping_add(add.speed as u8),
        anti_poison: add.anti_poison as u8,
        poison_recover: add.poison_recover,
        health_recover: add.health_recover,
        spell_recover: add.spell_recover,
    };

    let wabil = Ability {
        level: inputs.abil.level,
        reserved1: inputs.abil.reserved1,
        ac: word_add(add.ac, inputs.abil.ac),
        mac: word_add(add.mac, inputs.abil.mac),
        dc: word_add(add.dc, inputs.abil.dc),
        mc: word_add(add.mc, inputs.abil.mc),
        sc: word_add(add.sc, inputs.abil.sc),
        hp: inputs.cur_hp,
        mp: inputs.cur_mp,
        max_hp: inputs.abil.max_hp.wrapping_add(add.hp),
        max_mp: inputs.abil.max_mp.wrapping_add(add.mp),
        reserved2: inputs.abil.reserved2,
        exp_count: inputs.abil.exp_count,
        exp_max_count: inputs.abil.exp_max_count,
        exp: inputs.abil.exp,
        max_exp: inputs.abil.max_exp,
        weight: bag_weight(catalog, inputs.bag),
        max_weight,
        wear_weight,
        max_wear_weight,
        hand_weight,
        max_hand_weight,
    };

    RecalcOutput { wabil, sub, add }
}

/// 尾部合成的 word 相加（byte 重载：两侧都夹 255）。
const fn word_add(a: u16, b: u16) -> u16 {
    let low = clamp_u8((lo_byte(a) as u16) + (lo_byte(b) as u16));
    let high = clamp_u8((hi_byte(a) as u16) + (hi_byte(b) as u16));
    make_word_byte(low, high)
}

const fn clamp_u8(v: u16) -> u8 {
    if v > 255 {
        255
    } else {
        v as u8
    }
}

/// `ApplyItemParameters` 内部的 word 相加（ushort 重载：两侧都不夹/不截断）。
const fn word_add_raw(a: u16, b: u16) -> u16 {
    make_word_u16(
        (lo_byte(a) as u16) + (lo_byte(b) as u16),
        (hi_byte(a) as u16) + (hi_byte(b) as u16),
    )
}

fn add_weight(slot: usize, weight: u8, wear: &mut u8, hand: &mut u8) {
    if slot == SLOT_WEAPON || slot == SLOT_RIGHTHAND {
        *hand = hand.wrapping_add(weight);
    } else {
        *wear = wear.wrapping_add(weight);
    }
}

/// `RecalcHitSpeed`（`PlayObject.cs:3647-3679`）的基线：`HitPoint`/`SpeedPoint`。
fn hit_speed_base(inputs: &RecalcInputs<'_>) -> (u8, u8) {
    // `MessageSettings.DEFHIT/DEFSPEED` 在构造里已写进 HitPoint/SpeedPoint（`BaseObject.cs:388-389`）。
    let mut hit = DEF_HIT as u8;
    let mut speed = DEF_SPEED as u8;
    match inputs.job {
        0 | 1 => {
            hit = (u32::from(DEF_HIT)
                + u32::from(inputs.bonus_abil.hit) / u32::from(inputs.bonus_tick_hit))
                as u8;
            speed = (u32::from(DEF_SPEED)
                + (inputs.bonus_abil.speed / inputs.bonus_tick_speed) as u32)
                as u8;
        }
        2 => {
            speed = (u32::from(DEF_SPEED)
                + (inputs.bonus_abil.speed / inputs.bonus_tick_speed) as u32
                + 3) as u8;
        }
        _ => {}
    }
    (hit, speed)
}

fn bag_weight(catalog: &dyn ItemCatalog, bag: &[u16]) -> u16 {
    let mut result: u16 = 0;
    for &index in bag {
        if let Some(tpl) = catalog.get(index) {
            result = result.wrapping_add(u16::from(tpl.weight));
        }
    }
    result
}

/// `PlayObject.ApplyItemParameters`（`PlayObject.cs:4407`）的本片子集：把有效物品的属性累加进 `AddAbil`。
///
/// 只累加影响 `WAbil`/`SubAbility` 的字段（`HitSpeed`/`Luck`/`Slowdown`/`Poison`/`WeaponStrong` 略）。
pub fn apply_item_parameters(add: &mut AddAbility, item: &StdItem) {
    match item.std_mode {
        5 | 6 => {
            add.hit = add.hit.wrapping_add(u16::from(hi_byte(item.ac)));
        }
        10 | 11 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.speed = add.speed.wrapping_add(u16::from(item.agility));
            add.anti_magic = add.anti_magic.wrapping_add(u16::from(item.mg_avoid));
            add.anti_poison = add.anti_poison.wrapping_add(u16::from(item.tox_avoid));
            add.hp = add.hp.wrapping_add(item.hp_add as u16);
            add.mp = add.mp.wrapping_add(item.mp_add as u16);
            apply_eff_hp_mp(add, item.eff_type1, item.eff_rate1, item.eff_value1);
            apply_eff_hp_mp(add, item.eff_type2, item.eff_rate2, item.eff_value2);
        }
        15 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
            add.anti_magic = add.anti_magic.wrapping_add(u16::from(item.mg_avoid));
            add.anti_poison = add.anti_poison.wrapping_add(u16::from(item.tox_avoid));
        }
        19 => {
            add.anti_magic = add.anti_magic.wrapping_add(u16::from(hi_byte(item.ac)));
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
        }
        20 => {
            add.hit = add.hit.wrapping_add(u16::from(hi_byte(item.ac)));
            add.speed = add.speed.wrapping_add(u16::from(hi_byte(item.mac)));
            add.anti_magic = add.anti_magic.wrapping_add(u16::from(item.mg_avoid));
        }
        21 => {
            add.health_recover = add.health_recover.wrapping_add(u16::from(hi_byte(item.ac)));
            add.spell_recover = add.spell_recover.wrapping_add(u16::from(hi_byte(item.mac)));
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
            add.anti_magic = add.anti_magic.wrapping_add(u16::from(item.mg_avoid));
        }
        22 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
            add.hp = add.hp.wrapping_add(item.hp_add as u16);
        }
        23 => {
            add.anti_poison = add.anti_poison.wrapping_add(u16::from(hi_byte(item.ac)));
            add.poison_recover = add
                .poison_recover
                .wrapping_add(u16::from(hi_byte(item.mac)));
        }
        24 => {
            add.hit = add.hit.wrapping_add(u16::from(hi_byte(item.ac)));
            add.speed = add.speed.wrapping_add(u16::from(hi_byte(item.mac)));
        }
        26 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
            add.speed = add.speed.wrapping_add(u16::from(item.agility));
            add.mp = add.mp.wrapping_add(item.mp_add as u16);
        }
        52 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.speed = add.speed.wrapping_add(u16::from(item.agility));
        }
        53 => {
            add.hp = add.hp.wrapping_add(item.hp_add as u16);
            add.mp = add.mp.wrapping_add(item.mp_add as u16);
        }
        54 => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
            add.hit = add.hit.wrapping_add(u16::from(item.accurate));
            add.speed = add.speed.wrapping_add(u16::from(item.agility));
            add.anti_poison = add.anti_poison.wrapping_add(u16::from(item.tox_avoid));
        }
        _ => {
            add.ac = word_add_raw(add.ac, item.ac);
            add.mac = word_add_raw(add.mac, item.mac);
        }
    }
    // 无条件（`PlayObject.cs:4599-4601`）：DC/MC/SC 的高字节夹 255、低字节不夹。
    add.dc = add_word_capped(add.dc, item.dc);
    add.mc = add_word_capped(add.mc, item.mc);
    add.sc = add_word_capped(add.sc, item.sc);
}

/// `ApplyItemParameters` 尾部的 DC/MC/SC 相加：高字节 `_MIN(255, …)`。
const fn add_word_capped(a: u16, b: u16) -> u16 {
    make_word_u16(
        (lo_byte(a) as u16) + (lo_byte(b) as u16),
        clamp_u8((hi_byte(a) as u16) + (hi_byte(b) as u16)) as u16,
    )
}

/// `EfftypeConst.EFFTYPE_HP_MP_ADD` 分支（`PlayObject.cs:4440-4478`）：`HealthRecover += EffRate`、
/// `SpellRecover += EffValue`，各自夹到 65000。
fn apply_eff_hp_mp(add: &mut AddAbility, eff_type: u8, eff_rate: u8, eff_value: u8) {
    if eff_type != EFFTYPE_HP_MP_ADD {
        return;
    }
    add.health_recover = add
        .health_recover
        .saturating_add(u16::from(eff_rate))
        .min(65000);
    add.spell_recover = add
        .spell_recover
        .saturating_add(u16::from(eff_value))
        .min(65000);
}

/// `PlayObject.ApplyItemParametersEx`（`PlayObject.cs:4610`）的本片子集：mode 52/54 对最大重量的加成。
fn apply_item_parameters_ex(
    item: &StdItem,
    max_weight: &mut u16,
    max_wear_weight: &mut u8,
    max_hand_weight: &mut u8,
) {
    match item.std_mode {
        52 => {
            apply_hand_weight_eff(max_hand_weight, item.eff_type1, item.eff_value1);
            apply_hand_weight_eff(max_hand_weight, item.eff_type2, item.eff_value2);
            apply_wear_weight_eff(max_wear_weight, item.eff_type1, item.eff_value1);
            apply_wear_weight_eff(max_wear_weight, item.eff_type2, item.eff_value2);
        }
        54 => {
            apply_bag_weight_eff(max_weight, item.eff_type1, item.eff_value1);
            apply_bag_weight_eff(max_weight, item.eff_type2, item.eff_value2);
        }
        _ => {}
    }
}

fn apply_hand_weight_eff(target: &mut u8, eff_type: u8, eff_value: u8) {
    if eff_type == EFFTYPE_TWOHAND_WEIGHT_ADD {
        // `_MIN(255, …)` 对 u8 就是饱和加法。
        *target = target.saturating_add(eff_value);
    }
}

fn apply_wear_weight_eff(target: &mut u8, eff_type: u8, eff_value: u8) {
    if eff_type == EFFTYPE_EQUIP_WEIGHT_ADD {
        *target = target.saturating_add(eff_value);
    }
}

fn apply_bag_weight_eff(target: &mut u16, eff_type: u8, eff_value: u8) {
    if eff_type == EFFTYPE_BAG_WEIGHT_ADD {
        *target = target.saturating_add(u16::from(eff_value)).min(65000);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/ability_vectors.json"
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
    fn i16v(v: &Value) -> i16 {
        v.as_i64().unwrap() as i16
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
            ani_count: u8v(&v["ani_count"]),
            special_pwr: v["special_pwr"].as_i64().unwrap() as i8,
            item_desc: 0,
            looks: 0,
            dura_max: 0,
            ac: u16v(&v["ac"]),
            mac: u16v(&v["mac"]),
            dc: u16v(&v["dc"]),
            mc: u16v(&v["mc"]),
            sc: u16v(&v["sc"]),
            need: 0,
            need_level: 0,
            need_identify: 0,
            price: 0,
            stock: 0,
            atk_spd: u8v(&v["atk_spd"]),
            agility: u8v(&v["agility"]),
            accurate: u8v(&v["accurate"]),
            mg_avoid: u8v(&v["mg_avoid"]),
            strong: 0,
            undead: 0,
            hp_add: i32v(&v["hp_add"]),
            mp_add: i32v(&v["mp_add"]),
            exp_add: 0,
            eff_type1: u8v(&v["eff_type1"]),
            eff_rate1: u8v(&v["eff_rate1"]),
            eff_value1: u8v(&v["eff_value1"]),
            eff_type2: u8v(&v["eff_type2"]),
            eff_rate2: u8v(&v["eff_rate2"]),
            eff_value2: u8v(&v["eff_value2"]),
            slowdown: u8v(&v["slowdown"]),
            tox: u8v(&v["tox"]),
            tox_avoid: u8v(&v["tox_avoid"]),
            unique_item: 0,
            overlap_item: 0,
            light: u8v(&v["light"]),
            item_type: 0,
            item_set: 0,
            reference: String::new(),
        }
    }

    fn catalog(doc: &Value) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
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
            reserved2: i16v(&v["reserved2"]),
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

    fn bonus_tick(doc: &Value, job: u8) -> (u8, i32) {
        let key = match job {
            1 => "bonus_tick_wizard",
            2 => "bonus_tick_taos",
            _ => "bonus_tick_warr",
        };
        let t = &doc["defaults"][key];
        (u8v(&t["hit"]), i32v(&t["speed"]))
    }

    /// A. 夹具里每条用例的 `WAbil`(22 字段) + `SubAbility`(7 字段) + `feature` 都必须逐字段相同。
    #[test]
    fn matches_oracle_ability_fixture() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = doc["cases"].as_array().expect("cases 数组");
        assert!(cases.len() >= 20, "夹具用例数异常：{}", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let job = u8v(&case["job"]);
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
            let (tick_hit, tick_speed) = bonus_tick(&doc, job);
            let mut use_items = vec![None; USE_ITEMS_LEN];
            for it in case["items"].as_array().unwrap() {
                let slot = it["slot"].as_u64().unwrap() as usize;
                use_items[slot] = Some(ItemInstance {
                    index: u16v(&it["index"]),
                    dura: u16v(&it["dura"]),
                });
            }
            let bag: Vec<u16> = case["bag"].as_array().unwrap().iter().map(u16v).collect();
            let inputs = RecalcInputs {
                job,
                abil: &abil,
                bonus_abil: &bonus,
                bonus_tick_hit: tick_hit,
                bonus_tick_speed: tick_speed,
                cur_hp: u16v(&case["cur_hp"]),
                cur_mp: u16v(&case["cur_mp"]),
                use_items: &use_items,
                bag: &bag,
            };
            let got = recalc_abilitys(&cat, &inputs);
            let expect = &case["expect_wabil"];
            assert_eq!(got.wabil, ability(expect), "{name}: WAbil 与 oracle 不符");

            let es = &case["expect_sub"];
            assert_eq!(
                got.sub.anti_magic,
                u16v(&es["anti_magic"]),
                "{name}: AntiMagic"
            );
            assert_eq!(got.sub.hit_point, u8v(&es["hit_point"]), "{name}: HitPoint");
            assert_eq!(
                got.sub.speed_point,
                u8v(&es["speed_point"]),
                "{name}: SpeedPoint"
            );
            assert_eq!(
                got.sub.anti_poison,
                u8v(&es["anti_poison"]),
                "{name}: AntiPoison"
            );
            assert_eq!(
                got.sub.poison_recover,
                u16v(&es["poison_recover"]),
                "{name}: PoisonRecover"
            );
            assert_eq!(
                got.sub.health_recover,
                u16v(&es["health_recover"]),
                "{name}: HealthRecover"
            );
            assert_eq!(
                got.sub.spell_recover,
                u16v(&es["spell_recover"]),
                "{name}: SpellRecover"
            );

            let feature =
                equipped_feature(&cat, &use_items, u8v(&case["gender"]), u8v(&case["hair"]));
            assert_eq!(
                i64::from(feature),
                case["expect_feature"].as_i64().unwrap(),
                "{name}: feature 与 oracle 不符"
            );
        }
    }

    /// B. 金标准锚点：`naked_warr_golden` 必须逐字段等于 M0 抓包 ident=52/752
    ///（`tests/golden/session-20261010-c2-hook`）——把"接缝 ↔ 真机帧"钉在一起。
    #[test]
    fn naked_case_reproduces_m0_golden() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "naked_warr_golden")
            .expect("夹具必须含 naked_warr_golden");
        let abil = ability(&case["abil"]);
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        let use_items = vec![None; USE_ITEMS_LEN];
        let bag = vec![1201u16; 14];
        let inputs = RecalcInputs {
            job: 0,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 20,
            bonus_tick_speed: 35,
            cur_hp: 15,
            cur_mp: 15,
            use_items: &use_items,
            bag: &bag,
        };
        let got = recalc_abilitys(&cat, &inputs);
        // 金标准 52（`SM_ABILITY` 体 decode）：level=1 / ac=0 / mac=0 / dc=0x0101 / mc=0 / sc=0 /
        // hp=15 / mp=15 / max_hp=19 / max_mp=15 / exp=0 / max_exp=10 / weight=14 / max_weight=50 /
        // max_wear_weight=15 / max_hand_weight=12。
        assert_eq!(got.wabil.level, 1);
        assert_eq!((got.wabil.ac, got.wabil.mac, got.wabil.dc), (0, 0, 0x0101));
        assert_eq!((got.wabil.mc, got.wabil.sc), (0, 0));
        assert_eq!((got.wabil.hp, got.wabil.mp), (15, 15));
        assert_eq!((got.wabil.max_hp, got.wabil.max_mp), (19, 15));
        assert_eq!((got.wabil.weight, got.wabil.max_weight), (14, 50));
        assert_eq!(
            (got.wabil.max_wear_weight, got.wabil.max_hand_weight),
            (15, 12)
        );
        assert_eq!(
            got.sub.to_frame_bytes(),
            [1, 5, 15, 0, 0, 0, 0],
            "金标准 752：AntiMagic=1 / HitPoint=5 / SpeedPoint=15 / 其余 0"
        );
    }

    /// C. 位/字节原语锚点：`MakeWord` 的两个重载语义不同（改坏必红）。
    #[test]
    fn make_word_overloads_are_distinct() {
        // byte 重载：两者都已是 byte，不会有溢出。
        assert_eq!(make_word_byte(0x12, 0x34), 0x3412);
        // ushort 重载：低字节 0x190 的第 8 位会串进高字节（oracle 的原样行为）。
        assert_eq!(make_word_u16(0x0190, 0x0002), 0x0390);
        assert_ne!(
            word_add_raw(0x00C8, 0x00C8),
            word_add(0x00C8, 0x00C8),
            "ApplyItemParameters 的 ushort 相加与尾部合成的 byte 相加必须不同"
        );
    }

    /// D. `AntiMagic` 基线必须来自 `AddAbil`（基线 1）——把 1 改成 0 必红。
    #[test]
    fn anti_magic_baseline_is_one() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "naked_wizard")
            .unwrap();
        let abil = ability(&case["abil"]);
        let bonus = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        let use_items = vec![None; USE_ITEMS_LEN];
        let inputs = RecalcInputs {
            job: 1,
            abil: &abil,
            bonus_abil: &bonus,
            bonus_tick_hit: 25,
            bonus_tick_speed: 35,
            cur_hp: abil.hp,
            cur_mp: abil.mp,
            use_items: &use_items,
            bag: &[],
        };
        assert_eq!(recalc_abilitys(&cat, &inputs).sub.anti_magic, 1);
    }
}
