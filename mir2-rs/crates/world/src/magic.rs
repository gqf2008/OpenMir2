//! 技能子集 —— 移植源（逐条照抄，标注 C# 行号）：
//! - `src/M2Server/Magic/MagicBase.cs`：`MPow` / `GetPower` / `GetPower13` / `GetRPow`
//!   （与 `MagicManager.cs` 的同名私有实现同式；后者只多一层 `ushort` 截断，输入量级下等价）
//! - `src/M2Server/Player/PlayObject.Base.cs:1369`：`GetSpellPoint`（MP 消耗）
//! - `src/M2Server/Player/PlayObject.Base.cs:2170-2196`：`ReadBook` 的施法前置
//!   （`!IsTrainingSkill && (Job==99||Job==Job) && Abil.Level >= TrainLevel[0]`）
//! - `src/M2Server/Player/PlayObject.Attack.cs:1105`：`MagicAttackInterval = DelayTime + MagicHitIntervalTime`
//! - `src/M2Server/Magic/MagicManager.cs:145` `DoSpell` 各分支的**威力表达式**（只取纯数值部分）
//!
//! **本批范围（明确不做，报告里登记）**：`DoSpell` 的完整施法流程——地图 `MagCanHitTarget` /
//! `AntiMagic` 判定 / 护身符消耗 / `MagPassThroughMagic` / 召唤与状态挂载——不在本片；
//! 本片只把「前置 / MP / 威力公式 / 冷却」四件事做成可逐字段对拍。
//!
//! RNG 纪律：所有取数走调用方注入的 [`RandomSource`]（同种子路径），
//! **不用** Rust `rand` 默认算法（见 `docs/Rust重写设计文档.md` §6）。

use mir2_data::models::MagicInfo;
use mir2_shared::hutil;
use mir2_shared::rng::RandomSource;

use crate::combat::{get_attack_power, hi_byte, lo_byte};

/// `MagicBase.MPow`：`Power + Random(MaxPower - Power)`。
///
/// C# 返回 `ushort`（`MagicManager.MPow`）或 `int`（`MagicBase.MPow`）；输入量级下二者同值。
pub fn mpow(rng: &mut impl RandomSource, magic: &MagicInfo) -> i32 {
    i32::from(magic.power) + rng.random_below(i32::from(magic.max_power) - i32::from(magic.power))
}

/// `MagicBase.GetPower`：
/// `Round(nPower / (TrainLv + 1) * (Level + 1)) + DefPower + Random(DefMaxPower - DefPower)`。
pub fn get_power(rng: &mut impl RandomSource, magic: &MagicInfo, level: u8, n_power: i32) -> i32 {
    let scaled =
        hutil::round(f64::from(n_power) / f64::from(magic.train_lv + 1) * f64::from(level + 1));
    scaled
        + i32::from(magic.def_power)
        + rng.random_below(i32::from(magic.def_max_power) - i32::from(magic.def_power))
}

/// `MagicBase.GetPower13`：
/// `Round(d18 / (TrainLv + 1) * (Level + 1) + d10 + (DefPower + Random(DefMaxPower - DefPower)))`，
/// 其中 `d10 = nInt / 3.0`、`d18 = nInt - d10`。
pub fn get_power13(rng: &mut impl RandomSource, magic: &MagicInfo, level: u8, n_int: i32) -> i32 {
    let d10 = f64::from(n_int) / 3.0;
    let d18 = f64::from(n_int) - d10;
    let tail = i32::from(magic.def_power)
        + rng.random_below(i32::from(magic.def_max_power) - i32::from(magic.def_power));
    hutil::round(d18 / f64::from(magic.train_lv + 1) * f64::from(level + 1) + d10 + f64::from(tail))
}

/// `MagicBase.GetRPow`：`HiByte(wInt) > LoByte(wInt)` 时取 `Random(Hi-Lo+1)+Lo`，否则取 `Lo`。
///
/// 注意 `else` 分支**不取数**——这是 RNG 取数账本的一部分。
pub fn get_rpow(rng: &mut impl RandomSource, w_int: i32) -> u16 {
    let lo = i32::from(lo_byte(w_int));
    let hi = i32::from(hi_byte(w_int));
    if hi > lo {
        (rng.random_below(hi - lo + 1) + lo) as u16
    } else {
        lo as u16
    }
}

/// `PlayObject.GetSpellPoint`：`(ushort)(Round(Spell / 4.0 * (Level + 1)) + DefSpell)`。
pub fn get_spell_point(magic: &MagicInfo, level: u8) -> u16 {
    (hutil::round(f64::from(magic.spell) / 4.0 * f64::from(level + 1)) + i32::from(magic.def_spell))
        as u16
}

/// `PlayObject.Base.cs:2178-2180` 的 `ReadBook` 前置（不含 `FindMagic` 查表，由调用方给定魔表行）。
///
/// `is_training_skill` = 该技能已在角色 `MagicList` 中（`PlayObject.IsTrainingSkill`）。
pub fn can_read_book(magic: &MagicInfo, job: u8, char_level: u8, is_training_skill: bool) -> bool {
    !is_training_skill
        && (magic.job == 99 || magic.job == job)
        && char_level >= magic.train_level[0]
}

/// `PlayObject.Attack.cs:1105`：`MagicAttackInterval = UserMagic.Magic.DelayTime + Config.MagicHitIntervalTime`。
pub fn magic_attack_interval(delay_time: i32, magic_hit_interval_time: i32) -> i32 {
    delay_time + magic_hit_interval_time
}

// ---- DoSpell 各分支的威力表达式（只取纯数值部分）--------------------------------------------

/// 火球术/大火球：`GetAttackPower(GetPower(MPow) + Lo(MC), Hi(MC) - Lo(MC) + 1)`。
pub fn power_get_power_plus_mc(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
) -> i32 {
    // C# `GetPower(userMagic, MPow(userMagic))`：先求 MPow（取数 1 次）再进 GetPower。
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(mc));
    get_attack_power(
        rng,
        base,
        i32::from(hi_byte(mc)) - i32::from(lo_byte(mc)) + 1,
    )
}

/// 灵魂火符：`GetAttackPower(GetPower(MPow) + Lo(SC), Hi(SC) - Lo(SC) + 1)`。
pub fn power_get_power_plus_sc(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(sc));
    get_attack_power(
        rng,
        base,
        i32::from(hi_byte(sc)) - i32::from(lo_byte(sc)) + 1,
    )
}

/// 治愈术：`GetAttackPower(GetPower(MPow) + Lo(SC) * 2, (Hi(SC) - Lo(SC)) * 2 + 1)`。
pub fn power_heal_get_power_plus_sc2(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(sc)) * 2;
    get_attack_power(
        rng,
        base,
        (i32::from(hi_byte(sc)) - i32::from(lo_byte(sc))) * 2 + 1,
    )
}

/// 雷电术：同火球，另记不死系倍率（`DoSpell` 里 `nPower = Round(nPower * 1.5)`）。
pub fn power_lightening(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
) -> (i32, i32) {
    let power = power_get_power_plus_mc(rng, magic, level, mc);
    (power, hutil::round(f64::from(power) * 1.5))
}

/// 施毒术（绿毒/红毒各一次）：`GetPower13(nInt) + GetRPow(SC) * 2`。
///
/// oracle 按 `StdItem.Shape` 选 `nInt`（1 → 40 绿毒、2 → 30 红毒）。
pub fn power_poison(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
    n_int: i32,
) -> i32 {
    get_power13(rng, magic, level, n_int) + i32::from(get_rpow(rng, sc)) * 2
}

/// 幽灵盾/神圣战甲术：`GetAttackPower(GetPower13(60) + Lo(SC) * 10, Hi(SC) - Lo(SC) + 1)`。
pub fn power_defence_area(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    let base = get_power13(rng, magic, level, 60) + i32::from(lo_byte(sc)) * 10;
    get_attack_power(
        rng,
        base,
        i32::from(hi_byte(sc)) - i32::from(lo_byte(sc)) + 1,
    )
}

/// 困魔咒：`GetPower13(40) + GetRPow(SC) * 3`。
pub fn power_holy_shield(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    get_power13(rng, magic, level, 40) + i32::from(get_rpow(rng, sc)) * 3
}

/// 隐身术/集体隐身术：`GetPower13(30) + GetRPow(SC) * 3`。
pub fn power_transparent(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    get_power13(rng, magic, level, 30) + i32::from(get_rpow(rng, sc)) * 3
}

// ---- 施法前置（DoSpell 顶层门 + 武器/护身符）--------------------------------------------------
//
// 本组把 DoD③「施法前置」里**可离线**的两面补齐：
// - `is_warr_skill` / `check_amulet`：oracle 真有入口（public static），向量由 `amulet-probe` 反射/直调产出。
// - `spell_in_range` / `spell_version_gate` / `amulet_requirement`：`DoSpell` 的**内联语句**或
//   `switch` case 标签，未单独成方法、无法反射直调 ⇒ **source-cited**（注释给出行号）。

/// `MagicManager.IsWarrSkill`（`src/M2Server/Magic/MagicManager.cs:82-105`）：9 个战士技能魔号。
///
/// `DoSpell` 开头 `if (IsWarrSkill(userMagic.MagIdx)) return false;`（同文件 :151）——
/// 战士技能不走魔法施放流程。
pub fn is_warr_skill(mag_idx: i32) -> bool {
    matches!(mag_idx, 3 | 4 | 7 | 12 | 25 | 26 | 27 | 34 | 38)
}

/// `ItemLocation.ArmRingl` / `ItemLocation.Bujuk`（`src/OpenMir2/Enums/ItemLocation.cs:28,44`）。
pub const ITEM_LOCATION_ARM_RINGL: u8 = 5;
pub const ITEM_LOCATION_BUJUK: u8 = 9;

/// 护身符槽位（oracle 侧 = `UserItem{Index,Dura}` + `StdItem{StdMode,Shape}` 两件套；
/// `index == 0` 表示空槽，`None` 表示 `UseItems[..] == null`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmuletSlot {
    pub index: i32,
    pub std_mode: u8,
    pub shape: u8,
    pub dura: i32,
}

/// `MagicBase.CheckAmulet`（`src/M2Server/Magic/MagicBase.cs:47-105`）：先查 `ArmRingl`(5) 再查
/// `Bujuk`(9)；命中返回被选中的 `ItemLocation` 值，否则 `None`。
///
/// 命中条件：`Index > 0`、`StdMode == 25`、`Round(Dura / 100.0) >= count`，且形状匹配
/// `amulet_type`（1 ⇒ `Shape == 5`；2 ⇒ `Shape <= 2`；其余不匹配）。
/// 注意：`ArmRingl` 不匹配时**继续查** `Bujuk`（oracle 无 early-return）。
pub fn check_amulet(
    armringl: Option<AmuletSlot>,
    bujuk: Option<AmuletSlot>,
    count: u8,
    amulet_type: u8,
) -> Option<u8> {
    for (loc, slot) in [
        (ITEM_LOCATION_ARM_RINGL, armringl),
        (ITEM_LOCATION_BUJUK, bujuk),
    ] {
        let Some(s) = slot else { continue };
        if s.index <= 0 || s.std_mode != 25 {
            continue;
        }
        let shape_ok = match amulet_type {
            1 => s.shape == 5,
            2 => s.shape <= 2,
            _ => false,
        };
        if shape_ok && hutil::round(f64::from(s.dura) / 100.0) >= i32::from(count) {
            return Some(loc);
        }
    }
    None
}

/// `DoSpell` 距离门（`src/M2Server/Magic/MagicManager.cs:155`）：
/// `|CurrX - nTargetX| > MagicAttackRage || |CurrY - nTargetY| > MagicAttackRage` ⇒ 直接 `return false`。
///
/// 返回 `true` 表示**通过**该门（可继续施法）。`magic_attack_rage` 取 oracle 配置
/// `SystemShare.Config.MagicAttackRage`（探针读出默认 `8`）。**source-cited**（内联语句）。
pub fn spell_in_range(
    cur_x: i16,
    cur_y: i16,
    target_x: i16,
    target_y: i16,
    magic_attack_rage: i32,
) -> bool {
    let dx = (i32::from(cur_x) - i32::from(target_x)).abs();
    let dy = (i32::from(cur_y) - i32::from(target_y)).abs();
    !(dx > magic_attack_rage || dy > magic_attack_rage)
}

/// `DoSpell` 版本/魔号门（`src/M2Server/Magic/MagicManager.cs:167`）：
/// `SoftVersionDateEx == 0 && ClientTick == 0 || MagicId > 80` ⇒ 直接 `return false`。
///
/// 返回 `true` 表示**通过**该门。**source-cited**（内联语句）。
/// `SoftVersionDateEx`/`ClientTick` 在 oracle 侧是 `int`/`short`。
pub fn spell_version_gate(soft_version_date_ex: i32, client_tick: i16, magic_id: i32) -> bool {
    !((soft_version_date_ex == 0 && client_tick == 0) || magic_id > 80)
}

/// `DoSpell` 里每个需要**护身符**的魔号：`(需求数量, 护身符类型)`。
///
/// 逐条来自 `MagicManager.cs` 的 `CheckAmulet` 调用点（各 `case` 的 `CheckAmulet(playObject, n, t, …)`）：
/// - `:229` 施毒术(6)：`(1, 2)`
/// - `:323` 灵魂火符/幽灵盾/神圣战甲术/困魔咒/召唤骷髅/隐身术/集体隐身术(13..=19)：`(1, 1)`
/// - `:452` 召唤神兽(30)：`(5, 1)`
/// - `:464` 天使(41)：`(2, 1)`
/// - `:620` 无极真气(36)：`(1, 1)`
/// - `:908` 群体施毒术(48)：`(1, 2)`
///
/// **source-cited**（`switch` case 标签，未单独成方法）。
pub fn amulet_requirement(magic_id: i32) -> Option<(u8, u8)> {
    match magic_id {
        6 => Some((1, 2)),
        13..=19 => Some((1, 1)),
        30 => Some((5, 1)),
        41 => Some((2, 1)),
        36 => Some((1, 1)),
        48 => Some((1, 2)),
        _ => None,
    }
}
