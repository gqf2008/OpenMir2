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

use crate::combat::{atk_power, hi_byte, lo_byte, AttackPowerMode};
use crate::map::{CellType, MapGrid, MoveRequest};

// 2026-10-17（卡 openmir2-magic-draws-move，方案 C）：本文件里的取数已按语义**等价搬出**——
// 攻击语义（`power_uenhancer` / `group_melee_damage`）→ `crate::combat`；
// 魔号效果语义（`MPow`/`GetPower`/`GetPower13`/`GetRPow`、幽灵盾、圣言、诱惑）→
// `crate::magic_effect`。函数体/取数式/次序/门逐字不变，这里只做 `pub use` 再导出，
// 保持既有 `crate::magic::*` / `mir2_world::magic::*` 对外 API 不变。
pub use crate::combat::{group_melee_damage, power_uenhancer};
pub use crate::magic_effect::{
    get_power, get_power13, get_rpow, mag_tamming, mag_turn_undead, mpow, power_defence_area,
    TammingInput, TammingOutcome, TurnUndeadInput, TurnUndeadOutcome, LA_UNDEAD,
};

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

/// `MagicConst.SKILL_FIREWIND`（`src/Modules/SystemModule/Const/MagicConst.cs:36`）：抗拒火环。
///
/// `DoSpell` 的 `case SKILL_FIREWIND`（`MagicManager.cs:259-264`）⇒ `MagPushArround(...)`。
pub const SKILL_FIREWIND: u16 = 8;

/// `MagicConst.SKILL_ENERGYREPULSOR`（`src/Modules/SystemModule/Const/MagicConst.cs:183`）：气功波。
///
/// `DoSpell` 的 `case SKILL_ENERGYREPULSOR`（`MagicManager.cs:601-606`）与
/// `case SKILL_FIREWIND`（同文件 `:259-264`）**函数体逐字相同**：
/// `if (MagPushArround(playObject, userMagic.Level) > 0) { boTrain = true; } break;`
/// （`MagPushArround` 定义在 `:24-52`）。37 不在 `IsWarrSkill` 的 9 个魔号内（`:82-100`）
/// ⇒ 与 8 一样走 `DoSpell` 的非战士路径（`:151` 门通过），效果体/帧面完全一致。
pub const SKILL_ENERGYREPULSOR: u16 = 37;

/// `MagicConst.SKILL_SPACEMOVE`（`MagicConst.cs:88`）：瞬息移动。
///
/// `DoSpell` 的 `case SKILL_SPACEMOVE`（`MagicManager.cs:403-410`）：先发 `RM_MAGICFIRE`
/// （`boSpellFire = false`）⇒ `MagSaceMove(...)`。
pub const SKILL_SPACEMOVE: u16 = 21;

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

/// `MagicBase.UseAmulet`（`src/M2Server/Magic/MagicBase.cs:109-127`）的**纯**结果。
///
/// oracle 的副作用分两支：
/// - `Dura > nCount*100` ⇒ `Dura -= nCount*100` 并 `SendMsg(RM_DURACHANGE, idx, Dura, DuraMax, 0)`；
/// - 否则 ⇒ `Dura = 0` + `SendDelItems(item)` + `Index = 0`。
///
/// `duration` 在 oracle 侧是 `(ushort)(nCount * 100)`；`nCount <= 5` ⇒ 不溢出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AmuletUse {
    /// 使用后的 `UserItem.Dura`（`deleted` 时为 0）。
    pub dura_after: i32,
    /// 是否走「耐久不足 ⇒ 扣光并删除该护身符」支（oracle `:124-126`）。
    pub deleted: bool,
}

/// `MagicBase.UseAmulet`（`src/M2Server/Magic/MagicBase.cs:109-127`）的纯函数形式。
///
/// **调用前提**：oracle 只在 `CheckAmulet` 返回 true 后调用（⇒ `UseItems[idx] != null`）
/// ⇒ `dura` 是已确认为非空槽的 `Dura`。`deleted == false` 时调用方需发 `SM_DURACHANGE`，
/// `deleted == true` 时调用方需 `SendDelItems` 并清槽。
#[must_use]
pub fn use_amulet(dura: i32, count: u8) -> AmuletUse {
    // oracle `ushort duration = (ushort)(nCount * 100)`。
    let duration = i32::from(count) * 100;
    if dura > duration {
        AmuletUse {
            dura_after: dura - duration,
            deleted: false,
        }
    } else {
        AmuletUse {
            dura_after: 0,
            deleted: true,
        }
    }
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
    spell_in_range_i32(
        i32::from(cur_x),
        i32::from(cur_y),
        i32::from(target_x),
        i32::from(target_y),
        magic_attack_rage,
    )
}

/// [`spell_in_range`] 的 **i32 域**版本（登记订正 4，独立复验 2026-10-17）。
///
/// 语义与 oracle **逐字同构**：C# 的 `Math.Abs(playObject.CurrX - nTargetX)` 里两个 `short`
/// 运算数先按 `int` 提升再相减/取绝对值 ⇒ 该门本来就在 `int` 域成立，不是 `short` 域。
/// 宿主侧 `cur_x`/`tx` 本就是 `i32`，直接走本函数可**免掉** `i16::try_from(..).unwrap_or(0)`：
/// 后者会把超 `i16` 范围的坐标**静默折成 0** 再参与判定（fail-open：两个超界坐标可能被折成同一个
/// 0 而误判「在界内」）。oracle 侧坐标是 `short`、超界不可达，故此差异只在**本仓的非法输入**上
/// 显形；本函数把它收敛为「按真实 `i32` 差值判定」，即越界 ⇒ 不施放（保守/非 fail-open 方向）。
#[must_use]
pub fn spell_in_range_i32(
    cur_x: i32,
    cur_y: i32,
    target_x: i32,
    target_y: i32,
    magic_attack_rage: i32,
) -> bool {
    !((cur_x - target_x).abs() > magic_attack_rage || (cur_y - target_y).abs() > magic_attack_rage)
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

// ---- DoSpell 共享尾 + 技能熟练度/升级（第九片真身向量）----------------------------------------
//
// 第五片把 108 行魔表按 `DoSpell` 控制流分成 6 类；其中两类**没有任何效果体**：
// - `no_effect_case`（6 行）：case 体是 `break`（43 `SKILL_43`）或只置 `boTrain = true`
//   （49 `SKILL_49` / 51 `SKILL_51` / 52 `SKILL_52` / 53 `SKILL_53` / 54 `SKILL_54`）；
// - `no_switch_case`（20 行）：`DoSpell` 的 `switch` 里根本没有这些魔号的 case。
// 它们**唯一的运行时可观测行为**都在 `DoSpell` 的**共享尾**（`MagicManager.cs:644-670`）：
//   ① `if (boSpellFail) return false;`（`:644-647`）
//   ② `if (boSpellFire) SendRefMsg(RM_MAGICFIRE, …)`（`:648-658`）
//   ③ `if (userMagic.Level < 3 && boTrain) { if (TrainLevel[Level] <= Abil.Level) { TrainSkill(…, Random(3)+1);
//        if (!CheckMagicLevelUp(…)) SendSelfDelayMsg(RM_MAGIC_LVEXP, …); } }`（`:659-669`）
//   ④ `return true;`（`:670`）
//
// 本片把这套尾逻辑与它调用的两个**独立可反射直调**的真身方法——`PlayObject.TrainSkill`
// （`PlayObject.cs:4393-4400`）与 `PlayObject.CheckMagicLevelUp`（`PlayObject.cs:5198-5226`）——
// 做成可逐字段对拍的实现；攻击路径的同类入口 `PlayObject.CheckSkillProficiency`
// （`PlayObject.Attack.cs:200-213`）也一并覆盖。向量来源 `mir2-rs/tests/parity/m3/spell-tail-probe`
// （真身反射直调，记录 `System.Random` 取数账本）。

/// `PlayObject.TrainSkill`（`src/M2Server/Player/PlayObject.cs:4393-4400`）：
/// `if (FastTrain) nTranPoint *= 3; userMagic.TranPoint += nTranPoint;`。
///
/// 返回**本次实际增加**的熟练点（`FastTrain` 时 ×3）；调用方负责累加到 `TranPoint`。
pub fn train_skill(fast_train: bool, n_tran_point: i32) -> i32 {
    if fast_train {
        n_tran_point * 3
    } else {
        n_tran_point
    }
}

/// `PlayObject.CheckMagicLevelUp`（`PlayObject.cs:5198-5226`）读/写的标量子集。
///
/// 字段对应 oracle 同名属性：`MagicList[i].Level` / `.TranPoint` / `.Magic.{MagicId,TrainLv,TrainLevel,MaxTrain}`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SkillTrainState {
    pub magic_id: u16,
    pub level: u8,
    pub tran_point: i32,
    pub train_lv: u8,
    /// `MagicInfo.TrainLevel[0..4]`（学该级所需角色等级）。
    pub train_level: [u8; 4],
    /// `MagicInfo.MaxTrain[0..4]`（该级升下一级所需熟练点）。
    pub max_train: [i32; 4],
}

/// `PlayObject.CheckMagicLevelUp`（`PlayObject.cs:5198-5226`）的返回值与副作用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagicLevelUpOutcome {
    /// oracle 的 `bool result`（是否发生了升级）。
    pub result: bool,
    /// `CheckSeeHealGauge`（`:5231-5240`）：`MagicId == 28 && Level >= 2` ⇒ `AbilSeeHealGauge = true`。
    pub see_heal_gauge: bool,
    /// 是否发出了 `RM_MAGIC_LVEXP` 升级帧（`:5216 UpdateDelayMsg`）。
    pub levelup_msg: bool,
}

/// `PlayObject.CheckMagicLevelUp`（`src/M2Server/Player/PlayObject.cs:5198-5226`）。
///
/// 逐行：
/// 1. `nLevel = (Level < 4 && TrainLv >= Level) ? Level : 0`（`:5202-5209`）；
/// 2. `if (TrainLv > Level && MaxTrain[nLevel] <= TranPoint)` ⇒
///    `TranPoint -= MaxTrain[nLevel]; Level++; UpdateDelayMsg(RM_MAGIC_LVEXP, …); CheckSeeHealGauge(…)`；`result = true`（`:5210-5224`）。
///
/// **死代码**：`:5212` 内层 `if (TrainLv > Level)` 在外层已成立时恒为真 ⇒ `:5219-5222` 的
/// `else { TranPoint = MaxTrain[nLevel]; }` **永不执行**（连 bug 一起搬：保留结构但结果不变）。
pub fn check_magic_level_up(state: &mut SkillTrainState) -> MagicLevelUpOutcome {
    let mut out = MagicLevelUpOutcome::default();
    let n_level = if state.level < 4 && state.train_lv >= state.level {
        state.level
    } else {
        0
    };
    if state.train_lv > state.level && state.max_train[n_level as usize] <= state.tran_point {
        // oracle :5214-5215：先扣点、再升级；`Level`/`TranPoint` 的读取都在此之后。
        state.tran_point -= state.max_train[n_level as usize];
        state.level += 1;
        out.levelup_msg = true;
        // oracle :5217 `CheckSeeHealGauge`（只对 28 心灵启示、且 Level >= 2 置真）。
        out.see_heal_gauge = state.magic_id == 28 && state.level >= 2;
        out.result = true;
    }
    out
}

/// `PlayObject.CheckSkillProficiency`（`src/M2Server/Player/PlayObject.Attack.cs:200-213`）的返回/副作用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SkillProficiencyOutcome {
    /// 是否真的调了 `TrainSkill`（= 通过 `Level < 3 && TrainLevel[Level] <= Abil.Level` 门）。
    pub trained: bool,
    /// `CheckMagicLevelUp` 的 `result`。
    pub level_up: bool,
    /// 是否发出了 `RM_MAGIC_LVEXP` 帧（成功走 `CheckMagicLevelUp` 内的 `UpdateDelayMsg`；
    /// 失败由调用方 `if (!result) SendSelfDelayMsg(…)` 补发，延迟不同但**内容都是调用后的 `Level`/`TranPoint`**）。
    pub levelup_msg: bool,
    pub see_heal_gauge: bool,
}

/// `PlayObject.CheckSkillProficiency`（`src/M2Server/Player/PlayObject.Attack.cs:200-213`）：
/// 攻击路径的熟练度入口（与 DoSpell 尾部同一段逻辑，差别在**调用方先抽 `Random(3)+1`**）。
///
/// `n_tran_point` 是调用方已抽好的熟练点（`AttackSuccess` 的 `Random(3)+1` 或常量 `1`）。
pub fn check_skill_proficiency(
    fast_train: bool,
    abil_level: u8,
    n_tran_point: i32,
    state: &mut SkillTrainState,
) -> SkillProficiencyOutcome {
    let mut out = SkillProficiencyOutcome::default();
    if state.level < 3 && state.train_level[state.level as usize] <= abil_level {
        state.tran_point += train_skill(fast_train, n_tran_point);
        out.trained = true;
        let up = check_magic_level_up(state);
        out.level_up = up.result;
        out.see_heal_gauge = up.see_heal_gauge;
        out.levelup_msg = up.levelup_msg || !up.result;
    }
    out
}

/// `MagicManager.DoSpell` 共享尾（`src/M2Server/Magic/MagicManager.cs:644-670`）的返回/副作用。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DoSpellTailOutcome {
    /// oracle 的返回值：`boSpellFail` ⇒ `false`，否则 `true`。
    pub returned: bool,
    /// `boSpellFire` ⇒ 发一帧 `RM_MAGICFIRE`。
    pub magic_fire: bool,
    /// `boTrain` 门内的熟练度结算结果。
    pub train: SkillProficiencyOutcome,
}

/// `MagicManager.DoSpell` 共享尾（`src/M2Server/Magic/MagicManager.cs:644-670`）。
///
/// 取数顺序（**与攻击路径不同，勿混**）：先判 `:659` 的 `Level < 3 && boTrain`、再判
/// `:661` 的 `TrainLevel[Level] <= Abil.Level`，**两者都过才**在 `:663` 抽 `Random(3)+1`。
/// ⇒ 门未过时**不取数**。返回 `returned=false` 等价于 oracle `:646 return false`。
///
/// 本函数是**内联块**（不是独立方法）⇒ 只 source-cited（锚点见探针 `VerifyAnchors`），
/// 不冒充真身向量。
pub fn do_spell_tail(
    bo_spell_fail: bool,
    bo_spell_fire: bool,
    bo_train: bool,
    fast_train: bool,
    abil_level: u8,
    rng: &mut impl RandomSource,
    state: &mut SkillTrainState,
) -> DoSpellTailOutcome {
    let mut out = DoSpellTailOutcome::default();
    if bo_spell_fail {
        // oracle :644-647
        return out;
    }
    out.returned = true;
    // oracle :648-658
    out.magic_fire = bo_spell_fire;
    if bo_train && state.level < 3 && state.train_level[state.level as usize] <= abil_level {
        // oracle :663 `Random(3) + 1`（门内才抽）。取数站点在 `crate::magic_effect`
        // （留在 `magic.rs` 会被 `RUST_ATTACK_RE` 整文件误归 attack 组）。
        let n_tran_point = crate::magic_effect::spell_train_points(rng);
        state.tran_point += train_skill(fast_train, n_tran_point);
        out.train.trained = true;
        let up = check_magic_level_up(state);
        out.train.level_up = up.result;
        out.train.see_heal_gauge = up.see_heal_gauge;
        out.train.levelup_msg = up.levelup_msg || !up.result;
    }
    out
}

// ---- map-bound 技能效果（第十片真身向量）------------------------------------------------------
//
// 第七片把 7 行 `context_required` 登记为 `map_context`（需真 `Envir`/可见集/召唤）而 deferred；
// 第十片逐条判「能不能离线做」，把 (a) 类（只需真 `Envir`/地图数据）做成真身向量：
//   - 8 / 37 `MagPushArround`（`MagicManager.cs:24-52`）
//   - 21 `MagSaceMove`（`:876-891`）
//   - 46 `MagMakeClone`（`:1337-1341`）
// 向量来源 `mir2-rs/tests/parity/m3/map-effect-probe`（真身反射直调 + 合成真格式 `.map` 交给真身
// `Envirnoment.LoadMapData` + 记录 `System.Random` 取数账本）。
// 17/30/41（`MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`）需 `SystemShare.WorldEngine`
// 的**真召唤**（GameSrv 宿主实现，M2Server 探针不可达）⇒ 本片只登记、不冒充真身。

/// `src/OpenMir2/Enums/Direction.cs`。
pub const DIR_UP: u8 = 0;
pub const DIR_UP_RIGHT: u8 = 1;
pub const DIR_RIGHT: u8 = 2;
pub const DIR_DOWN_RIGHT: u8 = 3;
pub const DIR_DOWN: u8 = 4;
pub const DIR_DOWN_LEFT: u8 = 5;
pub const DIR_LEFT: u8 = 6;
pub const DIR_UP_LEFT: u8 = 7;

/// `M2Share.GetNextDirection`（`src/M2Server/M2Share.cs:299-376`）逐字移植。
pub fn next_direction(sx: i32, sy: i32, dx: i32, dy: i32) -> u8 {
    let mut flagx = if sx < dx {
        1
    } else if sx == dx {
        0
    } else {
        -1
    };
    if (sy - dy).abs() > 2 && sx >= dx - 1 && sx <= dx + 1 {
        flagx = 0;
    }
    let mut flagy = if sy < dy {
        1
    } else if sy == dy {
        0
    } else {
        -1
    };
    if (sx - dx).abs() > 2 && sy > dy - 1 && sy <= dy + 1 {
        flagy = 0;
    }
    match (flagx, flagy) {
        (0, -1) => DIR_UP,
        (1, -1) => DIR_UP_RIGHT,
        (1, 0) => DIR_RIGHT,
        (1, 1) => DIR_DOWN_RIGHT,
        (0, 1) => DIR_DOWN,
        (-1, 1) => DIR_DOWN_LEFT,
        (-1, 0) => DIR_LEFT,
        (-1, -1) => DIR_UP_LEFT,
        _ => DIR_DOWN,
    }
}

/// `BaseObject.GetBackDir`（`src/M2Server/Actor/BaseObject.cs:820-853`）。
pub fn back_dir(n_dir: u8) -> u8 {
    match n_dir {
        DIR_UP => DIR_DOWN,
        DIR_DOWN => DIR_UP,
        DIR_LEFT => DIR_RIGHT,
        DIR_RIGHT => DIR_LEFT,
        DIR_UP_LEFT => DIR_DOWN_RIGHT,
        DIR_UP_RIGHT => DIR_DOWN_LEFT,
        DIR_DOWN_LEFT => DIR_UP_RIGHT,
        DIR_DOWN_RIGHT => DIR_UP_LEFT,
        _ => 0,
    }
}

/// `BaseObject.GetFrontPosition`（`src/M2Server/Actor/BaseObject.cs:977-1040`）：按 `dir` 取前一格，
/// **越界时原样保留**（oracle 是 clamp 而非返回 false）。
pub fn front_position(x: i32, y: i32, dir: u8, width: i32, height: i32) -> (i32, i32) {
    let (mut nx, mut ny) = (x, y);
    match dir {
        DIR_UP if ny > 0 => ny -= 1,
        DIR_UP_RIGHT if nx < width - 1 && ny > 0 => {
            nx += 1;
            ny -= 1;
        }
        DIR_RIGHT if nx < width - 1 => nx += 1,
        DIR_DOWN_RIGHT if nx < width - 1 && ny < height - 1 => {
            nx += 1;
            ny += 1;
        }
        DIR_DOWN if ny < height - 1 => ny += 1,
        DIR_DOWN_LEFT if nx > 0 && ny < height - 1 => {
            nx -= 1;
            ny += 1;
        }
        DIR_LEFT if nx > 0 => nx -= 1,
        DIR_UP_LEFT if nx > 0 && ny > 0 => {
            nx -= 1;
            ny -= 1;
        }
        _ => {}
    }
    (nx, ny)
}

/// `BaseObject.CharPushed`（`src/M2Server/Actor/BaseObject.cs:854-901`）的**格子簿记等价实现**。
///
/// 无 RNG。目标自身不在格上（探针场景），故 `MoveToMovingObject` 的源格摘除不命中；
/// 目标位置随成功步数前进；`Dir`：`result>0` 取反方向，否则回到初值。
pub fn char_pushed(
    map: &mut MapGrid,
    x: &mut i32,
    y: &mut i32,
    dir: &mut u8,
    n_dir: u8,
    push_count: u8,
    actor_id: i32,
) -> i32 {
    let old_dir = *dir;
    *dir = n_dir;
    let back = back_dir(n_dir);
    let mut result = 0;
    for _ in 0..push_count {
        let (nx, ny) = front_position(*x, *y, *dir, map.width, map.height);
        // oracle `CharPushed` 先 `CanWalk(nx, ny, false)`：不可走（越界/墙/活体）即 break。
        // 探针场景目标未注册进 `ActorMgr` ⇒ 活体分支恒不命中，此处 `cellSuccess` 即 CanWalk 结果。
        if !map.get_cell_info(nx, ny).1 {
            break;
        }
        let moved = map.move_object(
            MoveRequest {
                from_x: *x,
                from_y: *y,
                actor_id,
                to_x: nx,
                to_y: ny,
                ignore_block: false,
                add_time: 0,
            },
            |_| false,
        );
        if moved {
            *x = nx;
            *y = ny;
            result += 1;
        } else {
            break;
        }
    }
    *dir = if result == 0 { old_dir } else { back };
    result
}

/// `MagPushArround`（`MagicManager.cs:24-52`）的一个可见目标（探针口径：只带该函数读/写的标量）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PushTarget {
    pub actor_id: i32,
    pub x: i32,
    pub y: i32,
    pub level: u8,
    pub death: bool,
    pub stick_mode: bool,
    /// 由调用方按 oracle `IsProperTarget` 判定的结果（Rust 侧 `crate::combat::is_proper_target` 可算）。
    pub proper_target: bool,
    pub dir: u8,
}

/// [`PushTarget`] 的可观测输出。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PushTargetOutcome {
    pub pushed: bool,
    pub push_count: i32,
    pub x: i32,
    pub y: i32,
    pub dir: u8,
}

/// [`mag_push_arround`] 的入参。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushAroundInput {
    pub caster_x: i32,
    pub caster_y: i32,
    pub caster_level: u8,
    /// `MagPushArround(playObject, nPushLevel)` 的 `nPushLevel`（= `userMagic.Level`）。
    pub push_level: i32,
    pub targets: Vec<PushTarget>,
}

/// [`mag_push_arround`] 的输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PushAroundOutcome {
    /// oracle 的 `result`（本函数返回的推进数，成功受推的目标计数）。
    pub result: i32,
    pub targets: Vec<PushTargetOutcome>,
}

/// 见 [`PushAroundInput`]/[`PushAroundOutcome`]（`MagicManager.cs:24-52`）。
pub fn mag_push_arround(
    rng: &mut impl RandomSource,
    map: &mut MapGrid,
    inp: &PushAroundInput,
) -> PushAroundOutcome {
    let mut result = 0;
    let mut outcomes = Vec::with_capacity(inp.targets.len());
    for t in &inp.targets {
        let mut oc = PushTargetOutcome {
            x: t.x,
            y: t.y,
            dir: t.dir,
            ..Default::default()
        };
        let adj = (inp.caster_x - t.x).abs() <= 1 && (inp.caster_y - t.y).abs() <= 1;
        if adj && !t.death && inp.caster_level > t.level && !t.stick_mode {
            let levelgap = i32::from(inp.caster_level) - i32::from(t.level);
            // 两抽的**次序**与 oracle 一致：`:37` 先抽 20；`IsProperTarget` 只作门（不抽数），
            // 通过后才在 `:41` 抽 2。取数站点在 `crate::magic_effect`（非 `magic.rs`，防被
            // `RUST_ATTACK_RE` 误归 attack 组）。
            if crate::magic_effect::push_chance(rng, inp.push_level, levelgap) && t.proper_target {
                let push = crate::magic_effect::push_steps(rng, inp.push_level);
                let n_dir = next_direction(inp.caster_x, inp.caster_y, t.x, t.y);
                let steps = char_pushed(
                    map,
                    &mut oc.x,
                    &mut oc.y,
                    &mut oc.dir,
                    n_dir,
                    push,
                    t.actor_id,
                );
                oc.push_count = steps;
                oc.pushed = true;
                result += 1;
            }
        }
        outcomes.push(oc);
    }
    PushAroundOutcome {
        result,
        targets: outcomes,
    }
}

/// `MagSaceMove`（`MagicManager.cs:876-891`）的入参（探针口径）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SaceMoveInput {
    /// `MagSaceMove(playObject, nLevel)` 的 `nLevel`（= `userMagic.Level`）。
    pub level: i32,
    /// `SystemShare.MapMgr.FindMap(HomeMap) != null`（探针替身只做这一步）。
    pub map_found: bool,
    /// `MapMgr.FindMap(HomeMap) == playObject.Envir`（同图同对象）。
    pub same_envir: bool,
    /// 目标地图尺寸（`MapRandomMove` 用目标 `envir.Width/Height`）。
    pub map_width: i32,
    pub map_height: i32,
    /// 调用前 `playObject.CurrX/CurrY` 与 `IsTimeRecall`。
    pub x: i32,
    pub y: i32,
    pub is_time_recall: bool,
}

/// [`mag_sace_move`] 的输出。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SaceMoveOutcome {
    pub result: bool,
    pub moved: bool,
    pub x: i32,
    pub y: i32,
    pub space_moved: bool,
    pub is_time_recall: bool,
}

/// 见 [`SaceMoveInput`]/[`SaceMoveOutcome`]（`MagicManager.cs:876-891` + `MapRandomMove` + `SpaceMove`）。
pub fn mag_sace_move(
    rng: &mut impl RandomSource,
    map: &MapGrid,
    inp: &SaceMoveInput,
) -> SaceMoveOutcome {
    let mut out = SaceMoveOutcome {
        x: inp.x,
        y: inp.y,
        is_time_recall: inp.is_time_recall,
        ..Default::default()
    };
    if crate::magic_effect::sace_move_gate(rng, inp.level) {
        out.result = true;
        if inp.map_found {
            // `BaseObject.MapRandomMove`（`BaseObject.cs:1214-1241`）：两次初始取数（站点 `merchant`）。
            let (nx0, ny0) =
                crate::merchant::map_random_move_xy(rng, inp.map_width, inp.map_height);
            // `BaseObject.SpaceMove`（`BaseObject.cs:1103-1168`）同服分支：目标图尺寸即本图尺寸。
            // 移位环取数**复用** `crate::space_move::space_move_get_rand_xy`（站点 `space_move`），
            // 不在此保留私有副本（魔号 21 接线卡硬要求）。
            if let Some((nx, ny)) = crate::space_move::space_move_get_rand_xy(
                rng,
                map.width,
                map.height,
                nx0,
                ny0,
                |x, y| map.get_cell_info(x, y).1,
            ) {
                out.x = nx;
                out.y = ny;
                out.space_moved = true;
                out.moved = true;
                if !inp.same_envir {
                    out.is_time_recall = false;
                }
            }
        }
    }
    out
}

/// `MagMakeClone`（`MagicManager.cs:1337-1341`）的入参：`new PlayCloneObject(playObject)` 的全部效果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MakeCloneInput {
    pub x: i32,
    pub y: i32,
    pub dir: u8,
    /// clone 的 `ActorId`（探针由 `Envir.AddMapObject` 的 `CellObjId` 记账）。
    pub clone_id: i32,
}

/// [`mag_make_clone`] 的输出。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MakeCloneOutcome {
    pub created: bool,
    pub x: i32,
    pub y: i32,
    pub dir: u8,
}

/// `MagMakeClone`（`MagicManager.cs:1337-1341`）：`new PlayCloneObject(playObject)` 只拷
/// `CurrX/CurrY`、`Dir=GetBackDir(playObject.Dir)`、`Envir`，并 `Envir.AddMapObject(...)`；
/// oracle 返回恒 `true`。名字固定 `"Clone"`（`PlayCloneObject.cs:9-18`）。
pub fn mag_make_clone(map: &mut MapGrid, inp: &MakeCloneInput) -> MakeCloneOutcome {
    let dir = back_dir(inp.dir);
    let created = map.add_map_object(inp.x, inp.y, CellType::Play, inp.clone_id, 0);
    MakeCloneOutcome {
        created,
        x: inp.x,
        y: inp.y,
        dir,
    }
}

#[cfg(test)]
mod amulet_use_tests {
    use super::use_amulet;

    /// `MagicBase.UseAmulet`（`MagicBase.cs:109-127`）：`Dura > nCount*100` ⇒ 扣；
    /// 否则（含 `==`）`Dura = 0; Index = 0`（清槽）。边界逐点钉死。
    #[test]
    fn use_amulet_deducts_when_enough_and_zeroes_when_not() {
        let a = use_amulet(450, 1); // 450 > 100 ⇒ 扣 100
        assert_eq!(a.dura_after, 350);
        assert!(!a.deleted, "耐久够 ⇒ 不删");
        let b = use_amulet(100, 1); // 100 == 100 ⇒ 不满足 `>` ⇒ 清槽
        assert_eq!(b.dura_after, 0);
        assert!(b.deleted, "边界相等 ⇒ 清槽（oracle 严格大于）");
        let c = use_amulet(499, 5); // 499 < 500 ⇒ 清槽
        assert!(c.deleted);
        let d = use_amulet(500, 5); // 500 == 500 ⇒ 清槽
        assert!(d.deleted);
        let e = use_amulet(501, 5); // 501 > 500 ⇒ 扣
        assert_eq!(e.dura_after, 1);
        assert!(!e.deleted);
    }
}
