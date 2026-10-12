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
//! 第二批已实装技能（9/10/22/23/24/28/29/31/33/36/45/47/50）在同一口径下补入
//! `power_fire_wall` / `power_show_hp_interval` / `power_shield` / `power_uenhancer`，
//! 其余按 [`power_get_power_plus_mc`] / [`power_heal_get_power_plus_sc2`] / [`power_lightening`] 复用。
//!
//! RNG 纪律：所有取数走调用方注入的 [`RandomSource`]（同种子路径），
//! **不用** Rust `rand` 默认算法（见 `docs/Rust重写设计文档.md` §6）。

use mir2_data::models::MagicInfo;
use mir2_shared::hutil;
use mir2_shared::rng::RandomSource;

use crate::combat::{
    get_attack_power, get_attack_power_player, get_hit_struck_damage, hi_byte, lo_byte,
    AttackPowerMode,
};
use crate::entity::Entity;

/// 攻方攻击力口径（`GetAttackPower`）：`DoSpell` 的施法者是 `PlayObject`，
/// 走 `PlayObject.GetAttackPower` override（`PlayObject.cs:2909`）——**取数次数与 Base 版不同**
/// （`Luck == 0` 时多一次 `Random(10)`），因此不能拿 [`AttackPowerMode::Base`] 代替，
/// 否则当威力之后还有取数（如火墙的 `time`）时序列会错位。
fn atk_power(rng: &mut impl RandomSource, base: i32, range: i32, atk: AttackPowerMode) -> i32 {
    match atk {
        AttackPowerMode::Base => get_attack_power(rng, base, range),
        AttackPowerMode::Player {
            luck,
            power_rate,
            bo_power_item,
            power_item,
        } => get_attack_power_player(
            rng,
            base,
            range,
            luck,
            power_rate,
            bo_power_item,
            power_item,
        ),
    }
}

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

/// `WorldServer.FindMagic(int nMagIdx)`（`src/GameSrv/Word/WorldServer.cs:1430-1444`）：
/// 在**装表顺序**里返回第一个 `MagicId == magic_id` 的行；找不到返回 `None`。
///
/// 装表顺序 = `MySqlDB.LoadMagicDB`（`src/GameSrv/DB/MySqlDB.cs:105-160`）的
/// `select * from magics` 行序（InnoDB 按主键 `Idx` 返回）经 `MagicList.Add` 追加后的次序。
///
/// **真魔表存在同一 `MagID` 多行**（实测 108 行 / 95 个不同 MagID / 13 个重复 MagID）：
/// 例如 `Idx=38`（群体施毒术）与 `Idx=48`（气功波）都是 `MagID=48`。
/// `SkillGroupAmyounsul = 48` 的 `DoSpell` 分支只按魔号分派 ⇒ 运行时 `FindMagic(48)`
/// 恒取**先入表**的「群体施毒术」，「气功波」一行被遮蔽（shadowed）且不可按魔号到达。
/// Rust 侧必须复刻该「首匹配」语义，**不能**用 `HashMap`/`last-wins` 覆盖。
pub fn resolve_magic(magics: &[MagicInfo], magic_id: u16) -> Option<&MagicInfo> {
    magics.iter().find(|m| m.magic_id == magic_id)
}

// ---- DoSpell 各分支的威力表达式（只取纯数值部分）--------------------------------------------

/// 火球术/大火球：`GetAttackPower(GetPower(MPow) + Lo(MC), Hi(MC) - Lo(MC) + 1)`。
pub fn power_get_power_plus_mc(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
    atk: AttackPowerMode,
) -> i32 {
    // C# `GetPower(userMagic, MPow(userMagic))`：先求 MPow（取数 1 次）再进 GetPower。
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(mc));
    atk_power(
        rng,
        base,
        i32::from(hi_byte(mc)) - i32::from(lo_byte(mc)) + 1,
        atk,
    )
}

/// 灵魂火符：`GetAttackPower(GetPower(MPow) + Lo(SC), Hi(SC) - Lo(SC) + 1)`。
pub fn power_get_power_plus_sc(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
    atk: AttackPowerMode,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(sc));
    atk_power(
        rng,
        base,
        i32::from(hi_byte(sc)) - i32::from(lo_byte(sc)) + 1,
        atk,
    )
}

/// 治愈术：`GetAttackPower(GetPower(MPow) + Lo(SC) * 2, (Hi(SC) - Lo(SC)) * 2 + 1)`。
pub fn power_heal_get_power_plus_sc2(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
    atk: AttackPowerMode,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(sc)) * 2;
    atk_power(
        rng,
        base,
        (i32::from(hi_byte(sc)) - i32::from(lo_byte(sc))) * 2 + 1,
        atk,
    )
}

/// 雷电术：同火球，另记不死系倍率（`DoSpell` 里 `nPower = Round(nPower * 1.5)`）。
pub fn power_lightening(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
    atk: AttackPowerMode,
) -> (i32, i32) {
    let power = power_get_power_plus_mc(rng, magic, level, mc, atk);
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
    atk: AttackPowerMode,
) -> i32 {
    let base = get_power13(rng, magic, level, 60) + i32::from(lo_byte(sc)) * 10;
    atk_power(
        rng,
        base,
        i32::from(hi_byte(sc)) - i32::from(lo_byte(sc)) + 1,
        atk,
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

// ---- 第二批已实装技能：DoSpell 的**效果表达式**（只取纯数值部分，施加承接后续批次）----------
//
// 下列分支在 `MagicManager.DoSpell` 里对**已落地**的魔号求值；表达式本身只调 oracle 真身子函数
// （`MPow`/`GetPower`/`GetPower13`/`GetRPow`/`GetAttackPower`），可直接产真身向量。
// 其**施加**仍依赖地图/目标态（`MagPassThroughMagic` / `MagMakeFireCross` / `MagBigExplosion` /
// `MagBubbleDefenceUp` 等），本片只对表达式取数，施加承接后续批次（报告『未实现』清单）。

/// 火墙（`SKILL_EARTHFIRE = 22`）：`MagicManager.cs:412-413` 的 `MagMakeFireCross` 两个入参。
///
/// `power = GetAttackPower(GetPower(MPow) + Lo(MC), ΔMC + 1)`；
/// `time  = GetPower(10) + (GetRPow(MC) >> 1)`。
/// 取数顺序（C# 实参从左到右）：power（MPow → GetPower → GetAttackPower）→ GetPower(10) → GetRPow(MC)。
pub fn power_fire_wall(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
    atk: AttackPowerMode,
) -> (i32, i32) {
    let power = power_get_power_plus_mc(rng, magic, level, mc, atk);
    let time = get_power(rng, magic, level, 10) + (i32::from(get_rpow(rng, mc)) >> 1);
    (power, time)
}

/// 心灵启示（`SKILL_SHOWHP = 28`）：`MagicManager.cs:437` 的 `ShowHpInterval`。
///
/// `interval = GetPower13(GetRPow(SC) * 2 + 30) * 1000`；取数顺序：`GetRPow(SC)` → `GetPower13`。
pub fn power_show_hp_interval(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
) -> i32 {
    let rp = get_rpow(rng, sc);
    get_power13(rng, magic, level, i32::from(rp) * 2 + 30) * 1000
}

/// 魔法盾（`SKILL_SHIELD = 31`）：`MagicManager.cs:475` 的 `MagBubbleDefenceUp` 威力入参。
///
/// `power = GetPower(GetRPow(MC) + 15)`；取数顺序：`GetRPow(MC)` → `GetPower`。
pub fn power_shield(rng: &mut impl RandomSource, magic: &MagicInfo, level: u8, mc: i32) -> i32 {
    let rp = get_rpow(rng, mc);
    get_power(rng, magic, level, i32::from(rp) + 15)
}

/// 无极真气分支（`SKILL_UENHANCER = 36`；数据行名为「火焰冰」——oracle 按魔号分派，本实现照搬）：
/// `MagicManager.cs:625-626`。
///
/// `power = (ushort)(Level + 1 + Random(Level))`；
/// `time  = GetAttackPower(GetPower13(60) + Lo(SC) * 10, ΔSC + 1)`（与 [`power_defence_area`] 同式）。
/// 取数顺序：`Random(Level)` → time。
pub fn power_uenhancer(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
    atk: AttackPowerMode,
) -> (i32, i32) {
    let power = i32::from(level) + 1 + rng.random_below(i32::from(level));
    let time = power_defence_area(rng, magic, level, sc, atk);
    (power, time)
}

/// `MagicManager.MagGroupDeDing`（函数起于 `MagicManager.cs:942`，逐目标结算体 `:956-963`）的**单目标**结算（39 `SKILL_GROUPDEDING`）。
///
/// 逐字照抄循环体（取数顺序勿改）：
/// 1. `nPower = playObject.GetAttackPower(LoByte(DC), HiByte(DC) - LoByte(DC))`
///    —— 注意与普攻 `GetBaseAttackPoewr` 不同：这里第二参**不做 sbyte 强转、也不 +1**；
/// 2. `if (Random(baseObject.SpeedPoint) >= playObject.HitPoint) nPower = 0;`
///    —— **无条件**抽一次 `Random(SpeedPoint)`（在攻击力之后）；
/// 3. `if (nPower > 0) nPower = baseObject.GetHitStruckDamage(playObject, nPower);`
///
/// 返回 `(闪避前的 power, 护甲减免后的 damage)`。
/// 遍历可见集 / `StruckDamage` 倍率 / `RM_DELAYMAGIC` 回帧由调用方承接（需地图与目标态）。
pub fn group_melee_damage(
    rng: &mut impl RandomSource,
    attacker: &Entity,
    target: &Entity,
    atk: AttackPowerMode,
) -> (i32, u16) {
    let dc_lo = i32::from(lo_byte(attacker.dc));
    let dc_diff = i32::from(hi_byte(attacker.dc)) - dc_lo;
    let mut power = atk_power(rng, dc_lo, dc_diff, atk);
    if rng.random_below(i32::from(target.speed_point)) >= i32::from(attacker.hit_point) {
        power = 0;
    }
    let damage = if power > 0 {
        get_hit_struck_damage(rng, target, attacker, power)
    } else {
        0
    };
    (power, damage)
}

// ---- 第三批已实装技能：群体变体（逐目标表达式与单目标同式）----------------------------------
//
// - 42（`SKILL_GROUPLIGHTENING` → `MagGroupLightening`，`MagicManager.cs:981-1018`）：逐目标
//   `GetAttackPower(GetPower(MPow)+Lo(MC), ΔMC+1)` + 不死 ×1.5 ⇒ 复用 [`power_lightening`]。
//   注：数据行名为「龙影剑法」（job 0），但 oracle 按**魔号**分派到法师的群体雷电术，本实现照搬。
// - 44（`SKILL_44` → `MagHbFireBall`，`MagicManager.cs:1020-1062`）：逐目标同
//   [`power_get_power_plus_mc`]。
// - 48（`SKILL_GROUPAMYOUNSUL` → `MagGroupAmyounsul`，`MagicManager.cs:893-940`）：逐目标同
//   [`power_poison`]（绿毒 40 / 红毒 30）。
// 三者的**遍历/施加**仍依赖地图可见集（`GetMapBaseObjects`）、目标态与 `AntiMagic` 骰，
// 本批只对表达式取数；施加承接攻击/魔法接线后的批次（报告『未实现』清单）。

// ---- 施法前置（DoSpell 顶层门 + 武器/护身符）--------------------------------------------------
//
// 本组把 DoD③「施法前置」里**可离线**的两面补齐：
// - `is_warr_skill` / `check_amulet`：oracle 真有入口（public static），向量由 `amulet-probe` 反射/直调产出。
// - `spell_in_range` / `spell_version_gate` / `amulet_requirement`：`DoSpell` 的**内联语句**或
//   `switch` case 标签，未单独成方法、无法反射直调 ⇒ **source-cited**（注释给出行号）。

/// `MagicManager.IsWarrSkill`（`src/M2Server/Magic/MagicManager.cs:82-100`）：9 个战士技能魔号。
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

// ---- context_required 效果：独立可调方法（第七片真身向量）------------------------------------
//
// 这两条是 `DoSpell` 的 `case` 体里**唯一**不需要地图/可见集/召唤的独立方法：
// `MagTurnUndead`（32 圣言术）与 `MagTamming`（20 诱惑之光）。其余 context_required 分支
// （`MagPushArround`/`MagSaceMove`/`MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`/
// `MagMakeClone`）体内部走 `Envir`/`GetMapBaseObjects`/`MakeSlave`，本片只登记可达性。
// 向量来源：`mir2-rs/tests/parity/m3/magic-effect-probe`（反射直调真身，记录 System.Random 取数账本）。

/// `Grobal2.LA_UNDEAD`（`src/OpenMir2/Grobal2.cs:66`）。
///
/// **source-cited**（常量，未单独成方法）。
pub const LA_UNDEAD: u8 = 1;

/// `MagicManager.MagTurnUndead`（`src/M2Server/Magic/MagicManager.cs:827-856`）的**探针口径**。
///
/// 逐字移植：
/// 1. `if (target.SuperMan || target.LifeAttrib != LA_UNDEAD) return false;`
/// 2. `((AnimalObject)target).Struck(playObject)` —— 本口径下目标进入时 `TargetCret == null`
///    且 `IsProperTarget(施法者)` 为真 ⇒ **不抽** `Random(6)`、`target.TargetCret = 施法者`；
///    `Animal` 为真时 `MeatQuality -= Random(300)`（oracle 无条件抽这一次）。
/// 3. `if (target.TargetCret == null) { RunAwayMode = true; … }` —— 因第 2 步已写 `TargetCret`，本口径**不进**。
/// 4. `playObject.SetTargetCreat(target)`。
/// 5. `if (Random(2) + (caster.Lv - 1) > target.Lv)` 且 `target.Lv < cfg`：
///    `level = caster.Lv - target.Lv`，再 `if (Random(100) < (magicLevel << 3) - magicLevel + 15 + level)`
///    ⇒ `target.WAbil.HP = 0; result = true;`。
///
/// 未覆盖（承接攻击/魔法接线批次，需地图与朝向）：`TargetCret != null` 时 `Struck` 的
/// `GetAttackDir`/`Random(6)` 分支、`RunAwayMode` 分支、`SetLastHiter` 的其它副作用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnUndeadOutcome {
    pub result: bool,
    pub target_hp_zeroed: bool,
    pub target_targeted_by_caster: bool,
    pub target_run_away: bool,
    pub caster_target_is_target: bool,
}

/// `MagicManager.MagTurnUndead`（`MagicManager.cs:827-856`）的入参（探针口径）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnUndeadInput {
    pub super_man: bool,
    pub life_attrib: u8,
    /// 目标 `Animal`（`Struck` 里决定是否抽 `Random(300)`）。
    pub animal: bool,
    pub caster_level: i32,
    pub target_level: i32,
    pub magic_level: u8,
    /// `SystemShare.Config.MagTurnUndeadLevel`。
    pub mag_turn_undead_level: i32,
}

/// 见 [`TurnUndeadInput`]/[`TurnUndeadOutcome`]（`MagicManager.cs:827-856`，探针口径）。
pub fn mag_turn_undead(rng: &mut impl RandomSource, inp: &TurnUndeadInput) -> TurnUndeadOutcome {
    let mut out = TurnUndeadOutcome::default();
    if inp.super_man || inp.life_attrib != LA_UNDEAD {
        return out;
    }
    // `((AnimalObject)target).Struck(playObject)` —— 探针口径（TargetCret 为空 ⇒ 短路）。
    if inp.animal {
        rng.random_below(300);
    }
    out.target_targeted_by_caster = true;
    // `playObject.SetTargetCreat(targetObject)`（无条件，在早退之后）。
    out.caster_target_is_target = true;
    if rng.random_below(2) + (inp.caster_level - 1) > inp.target_level
        && inp.target_level < inp.mag_turn_undead_level
    {
        let level = inp.caster_level - inp.target_level;
        let magic_level = i32::from(inp.magic_level);
        if rng.random_below(100) < (magic_level << 3) - magic_level + 15 + level {
            out.target_hp_zeroed = true;
            out.result = true;
        }
    }
    out
}

/// `MagicManager.MagTamming`（`src/M2Server/Magic/MagicManager.cs:704-825`）的入参。
///
/// 只取 oracle 该函数**读/写到的标量**；`target.WAbil.HP` 等以调用初值给出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TammingInput {
    pub magic_level: u8,
    pub caster_level: i32,
    /// `target.Race`（`> ACTOR_RACE_PLAY` 才可能被诱惑）。
    pub target_race: u8,
    pub target_level: i32,
    /// `target.Abil.MaxHP`（`n14 = MaxHP / MagTammingHPRate`）。
    pub target_abil_max_hp: i32,
    pub target_life_attrib: u8,
    pub target_no_tame: bool,
    /// 调用前 `target.Master == 施法者`。
    pub target_master_is_caster: bool,
    /// 调用前 `target.Master != null`（含 ≠ 施法者）——决定受奴时是否先 `HP = HP/10`。
    pub target_master_present: bool,
    /// 调用前 `target.WAbil.HP` 初值（受奴且已有别的 Master 时先 `/10`；只影响输出，不参与判定）。
    pub target_hp: i32,
    /// 调用前 `target.WalkSpeed` / `target.NextHitTime`（受奴时按 `1500/2000 - magicLevel*200` 封顶）。
    pub walk_speed: i32,
    pub next_hit_time: i32,
    pub caster_slave_count: i32,
    pub cfg_target_level: i32,
    pub cfg_tamming_level: i32,
    pub cfg_tamming_count: i32,
    pub cfg_hp_rate: i32,
}

/// `MagTamming` 的可观测输出（`MasterRoyaltyTick` 含 `GetTickCount()` 非确定，不在此列）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TammingOutcome {
    pub result: bool,
    /// `target.TargetCret = null`（进入分支时无条件清）。
    pub target_creature_cleared: bool,
    pub target_master_is_caster: bool,
    pub target_hp: i32,
    pub walk_speed: i32,
    pub next_hit_time: i32,
    pub crazy_mode: bool,
}

/// 见 [`TammingInput`]/[`TammingOutcome`]（`MagicManager.cs:704-825`）。
///
/// 逐字移植 oracle 控制流；`SendMsg`/`BreakCrazyMode`/`RefShowName` 等无 RNG、无返回值
/// 的副作用不建模（oracle 里它们不改变取数账本与上面这些输出）。
pub fn mag_tamming(rng: &mut impl RandomSource, inp: &TammingInput) -> TammingOutcome {
    let magic_level = i32::from(inp.magic_level);
    let mut out = TammingOutcome {
        target_hp: inp.target_hp,
        walk_speed: inp.walk_speed,
        next_hit_time: inp.next_hit_time,
        ..Default::default()
    };
    if inp.target_race > crate::entity::ACTOR_RACE_PLAY && rng.random_below(4 - magic_level) == 0 {
        out.target_creature_cleared = true;
        if inp.target_master_is_caster {
            out.target_master_is_caster = true;
            out.result = true;
        } else {
            if rng.random_below(2) == 0 && inp.target_level <= inp.caster_level + 2 {
                if rng.random_below(3) == 0 {
                    let roll = rng.random_below(inp.caster_level + 20 + magic_level * 5);
                    if roll > inp.target_level + inp.cfg_target_level {
                        if !inp.target_no_tame
                            && inp.target_life_attrib != LA_UNDEAD
                            && inp.target_level < inp.cfg_tamming_level
                            && inp.caster_slave_count < inp.cfg_tamming_count
                        {
                            let mut n14 = inp.target_abil_max_hp / inp.cfg_hp_rate;
                            if n14 <= 2 {
                                n14 = 2;
                            } else {
                                n14 += n14;
                            }
                            // 此处 target.Master != 施法者 恒成立（已在上面 else 分支）。
                            if rng.random_below(n14) == 0 {
                                if inp.target_master_present {
                                    out.target_hp /= 10;
                                }
                                out.target_master_is_caster = true;
                                out.crazy_mode = false; // BreakCrazyMode()
                                                        // `MasterRoyaltyTick = (Random(caster.Lv * 2) + …) * 60_000 + GetTickCount()`：
                                                        // GetTickCount 非确定，但 Random(caster.Lv * 2) 是取数账本的一部分。
                                rng.random_below(inp.caster_level * 2);
                                out.walk_speed = out.walk_speed.min(1500 - magic_level * 200);
                                out.next_hit_time = out.next_hit_time.min(2000 - magic_level * 200);
                            } else if rng.random_below(14) == 0 {
                                out.target_hp = 0;
                            }
                        } else if inp.target_life_attrib == LA_UNDEAD && rng.random_below(20) == 0 {
                            out.target_hp = 0;
                        }
                    } else if inp.target_life_attrib != LA_UNDEAD && rng.random_below(20) == 0 {
                        rng.random_below(20);
                        out.crazy_mode = true; // OpenCrazyMode(Random(20) + 10)
                    }
                } else if inp.target_life_attrib != LA_UNDEAD {
                    rng.random_below(20);
                    out.crazy_mode = true; // OpenCrazyMode(Random(20) + 10)
                }
            }
            out.result = true;
        }
    } else if rng.random_below(2) == 0 {
        out.result = true;
    }
    out
}
