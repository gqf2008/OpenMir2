//! 魔号效果体的 **RNG 取数站点**（`MagPushArround` / `MagSaceMove` 的正面取数）。
//!
//! **为什么单独成文件（2026-10-17，魔号 8/21 接线卡）**：RNG 结构一致性门
//! （`tests/parity/m3/rng-trace-probe/consumer_groups.tsv` 的 `groups --require-closed`）
//! 按**站点字符串**归组，而 `rng_ledger_diff.py` 的 `RUST_ATTACK_RE` 把整个
//! `crates/world/src/magic\.rs:` 前缀视作**攻击路径**（历史语义漂移）。效果体的取数若留在
//! `magic.rs`，会被误归 `attack` 组 ⇒ 与 C# 侧 `MagicManager.MagPushArround` /
//! `MagSaceMove` 的独立站点不对称。把取数搬进本文件后：
//!
//! | 组（consumer_groups.tsv） | C# oracle | Rust |
//! | --- | --- | --- |
//! | `magic_effect` | `MagicManager.MagPushArround@MagicManager.cs:37/41`、`MagSaceMove@:879` | 本文件 |
//! | `merchant` | `BaseObject.MapRandomMove@BaseObject.cs:1235/1236`（经 `MagSaceMove` 调用） | `crate::merchant::map_random_move_xy` |
//! | `space_move` | `BaseObject.SpaceMoveGetRandXY@BaseObject.cs:1040` | `crate::space_move::space_move_get_rand_xy` |
//!
//! 取数式与次序（逐行锚 `src/M2Server/Magic/MagicManager.cs`）：
//! - `MagPushArround`（`:24-52`）：`:37` `Random(20) < 6 + nPushLevel*3 + levelgap`，
//!   仅**通过且** `IsProperTarget` 后 `:41` `Random(2)`（`push = 1 + MAX(0, level-1) + Random(2)`）。
//! - `MagSaceMove`（`:876-891`）：`:879` `Random(11) < nLevel*2 + 4`。
//!
//! 本模块只做**取数**；门/落点/位移在调用方（`crate::world` 与 `crate::magic` 的探针版）逐行实现。
//!
//! **2026-10-17（卡 `openmir2-magic-draws-move`，方案 C）再迁入**：`crates/world/src/magic.rs`
//! 里属于**魔号效果语义**的取数按函数**等价搬入**本文件（函数体/取数式/次序/门逐字不变）——
//! `MPow`/`GetPower`/`GetPower13`/`GetRPow` 四个威力公式、幽灵盾 `power_defence_area`、
//! 圣言术 `mag_turn_undead`、诱惑之光 `mag_tamming`。`crate::magic` 用 `pub use` 再导出，
//! 对外路径不变。攻击语义的两处（`power_uenhancer` / `group_melee_damage`）搬去
//! `crate::combat`，**不**在本文件。
//!
//! 因此本文件的站点一律归 `magic_effect` 组（`consumer_groups.tsv` 的
//! `crates/world/src/magic_effect\.rs:` 文件级行）；`magic.rs` 里**不再有任何取数**
//! （`magic_draws_rng_trace` 的静态断言钉住）。

use mir2_data::models::MagicInfo;
use mir2_shared::hutil;
use mir2_shared::rng::RandomSource;

use crate::combat::{atk_power, hi_byte, lo_byte, AttackPowerMode};

/// `MagPushArround` 的第一抽：`Random(20) < 6 + nPushLevel * 3 + levelgap`（`MagicManager.cs:37`）。
#[must_use]
pub fn push_chance(rng: &mut impl RandomSource, push_level: i32, level_gap: i32) -> bool {
    rng.random_below(20) < 6 + push_level * 3 + level_gap
}

/// `MagPushArround` 的第二抽：`(byte)(1 + HUtil32._MAX(0, nPushLevel - 1) + Random(2))`（`:41`）。
#[must_use]
pub fn push_steps(rng: &mut impl RandomSource, push_level: i32) -> u8 {
    (1 + 0.max(push_level - 1) + rng.random_below(2)) as u8
}

/// `MagSaceMove` 的施放门：`Random(11) < nLevel * 2 + 4`（`:879`）。
#[must_use]
pub fn sace_move_gate(rng: &mut impl RandomSource, level: i32) -> bool {
    rng.random_below(11) < level * 2 + 4
}

/// `DoSpell` 共享尾的训练点抽样：`Random(3) + 1`（`MagicManager.cs:663`）。
///
/// 站点必须落在本文件：留在 `magic.rs` 会被 `RUST_ATTACK_RE`（整文件 `magic\.rs:`）误归
/// `attack` 组，而 C# oracle 的该抽在 `MagicManager.DoSpell`（非攻击路径方法）。
#[must_use]
pub fn spell_train_points(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(3) + 1
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

/// **P3-B1（单体直击/骰门族）**：抗魔骰 `Random(10) >= AntiMagic` 的一抽。
///
/// oracle 三处同式：
/// - `SKILL_FIREBALL`/`SKILL_FIREBALL2`（`MagicManager.cs:180`：`AntiMagic <= Random(10)`）；
/// - `SKILL_LIGHTENING`（`:292`：`Random(10) >= AntiMagic`）；
/// - `SKILL_45`（`:566`：同 11）；
/// - `SKILL_FIRECHARM`（`:334`：`Random(10) >= AntiMagic`）。
///
/// 返回 `true` = 骰门**通过**（后续才取威力）；`false` = 失败（oracle 置 `targetObject = null`）。
/// **注意**：本函数只负责**这一抽**；`MagCanHitTarget`/`IsProperTarget` 是纯门、必须先过。
#[must_use]
pub fn anti_magic_roll(rng: &mut impl RandomSource, anti_magic: i32) -> bool {
    rng.random_below(10) >= anti_magic
}

/// 单体直击的 MC 口径威力：`GetAttackPower(GetPower(MPow(magic)) + Lo(MC), ΔMC + 1)`。
///
/// 取数序（逐项，按 oracle 表达式求值序）：
/// 1. `MPow`（`:104`）：`Random(MaxPower - Power)`；
/// 2. `GetPower`（`:110`）：`Random(DefMaxPower - DefPower)`；
/// 3. `GetAttackPower`（玩家 `PlayObject.cs:2909` override）：`Random(power + 1)`，
///    `Luck <= 0` 时再 `Random(10)`。
///
/// 对应 `SKILL_FIREBALL(1)`/`SKILL_FIREBALL2(5)`（`MagicManager.cs:182`）、
/// `SKILL_LIGHTENING(11)`（`:294`）、`SKILL_45(45)`（`:568`）。
pub fn single_hit_power_mc(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
    atk: AttackPowerMode,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(mc));
    atk_power(
        rng,
        base,
        i32::from(hi_byte(mc)) - i32::from(lo_byte(mc)) + 1,
        atk,
    )
}

/// 灵魂火符（`SKILL_FIRECHARM(13)`，`MagicManager.cs:337`）的 SC 口径威力：
/// `GetAttackPower(GetPower(MPow(magic)) + Lo(SC), ΔSC + 1)`。
///
/// 取数序与 [`single_hit_power_mc`] 相同，只把 MC 换成 SC。
pub fn single_hit_power_sc(
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

/// **P3-B5**：10 的 `undeadAttack=true` 复合放大（`BaseObject.cs:908-911`）：
/// `magPwr = HUtil32.Round(magPwr * 1.5)`。
///
/// **不是「只打不死」**：oracle 在 `MagPassThroughMagic` 内对**每个骰门通过的命中**无条件调用，
/// 且 `magPwr` 是同一个局部变量 ⇒ **逐击复合**（第一击 ×1.5，第二击在放大值上再 ×1.5）。
#[must_use]
pub fn scale_undead_attack(power: i32) -> i32 {
    hutil::round(f64::from(power) * 1.5)
}

/// **P3-B5**：36 `SKILL_UENHANCER` 的两抽（`MagicManager.cs:623-624`）。
///
/// ```text
/// nPower = (ushort)(Level + 1 + Random(Level));          // Level == 0 也推进一次 Random(0)
/// nTime  = GetAttackPower(GetPower13(userMagic, 60) + Lo(SC)*10, Δ(SC)+1);
/// ```
/// ⇒ 取数次序固定：`R(Level)` → `GetPower13`（其内 1 抽 `R(0)`）→ `GetAttackPower`
/// （玩家 `Luck <= 0` 时 `R(range+1)` + `R(10)`）。
#[must_use]
pub fn power_uenhancer(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    sc: i32,
    atk: AttackPowerMode,
) -> (i32, i32) {
    // `:623` `(ushort)(Level + 1 + M2Share.RandomNumber.Random(Level))`；`Random(0)` 仍推进。
    let n_power = i32::from(level) + 1 + rng.random_below(i32::from(level));
    // `:624` `GetPower13(userMagic, 60) + Lo(SC)*10` → `GetAttackPower(..., ΔSC+1)`。
    let base = get_power13(rng, magic, level, 60) + i32::from(lo_byte(sc)) * 10;
    let n_time = atk_power(
        rng,
        base,
        i32::from(hi_byte(sc)) - i32::from(lo_byte(sc)) + 1,
        atk,
    );
    (n_power, n_time)
}

/// `MagicManager.MagHbFireBall`（`MagicManager.cs:1020-1064`）的推挤两抽（魔号 44）。
///
/// oracle 顺序（**在威力之后**）：
/// ```text
/// :1048 if (playObject.Abil.Level > targetObject.Abil.Level && !targetObject.StickMode)
/// :1050   levelgap = playObject.Abil.Level - targetObject.Abil.Level
/// :1051   if (Random(20) < 6 + userMagic.Level * 3 + levelgap)
/// :1053     push = Random(userMagic.Level) - 1;
/// ```
/// `Random(0)`（`userMagic.Level == 0`）在 .NET 里仍调一次 `Sample()` ⇒ **照抽一次**再返回 0。
///
/// 返回 `Some(push)` 表示 oracle 会发 `RM_DELAYPUSHED`（`push > 0`）；`None` 不等于"没抽"。
/// `StickMode` 本骨架无字段 ⇒ 调用方恒传 `false`（登记）。
#[must_use]
pub fn hb_fireball_push(
    rng: &mut impl RandomSource,
    magic_level: u8,
    caster_level: u8,
    target_level: u8,
    target_stick_mode: bool,
) -> Option<u8> {
    if !(caster_level > target_level && !target_stick_mode) {
        return None;
    }
    let level_gap = i32::from(caster_level) - i32::from(target_level);
    if rng.random_below(20) < 6 + i32::from(magic_level) * 3 + level_gap {
        let push = rng.random_below(i32::from(magic_level)) - 1;
        if push > 0 {
            return Some(push as u8);
        }
    }
    None
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

/// **P3-B2（`RM_MAGSTRUCK` 交付段）**：`WalkTick += 800 + Random(1000)`
/// （`BaseObject.Base.cs:620-623`，只对 `Race>=Animal && !RushMode && Level<50`）。
#[must_use]
pub fn mag_struck_walk_draw(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(1000)
}

/// **P3-B2**：`AnimalObject.Struck` 的 `GetAttackDir` 失败骰 `Random(6) == 0`
/// （`AnimalObject.cs:342`；只在 `TargetCret != null` 且方向门失败时抽）。
#[must_use]
pub fn mag_struck_attack_dir_roll(rng: &mut impl RandomSource) -> bool {
    rng.random_below(6) == 0
}

/// **P3-B2**：`AnimalObject.Struck` 的 `Random(300)`（`AnimalObject.cs:352`，`Animal=true`）。
#[must_use]
pub fn mag_struck_meat_roll(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(300)
}

/// **P3-B2（治疗族 2/29）**：`GetAttackPower(GetPower(MPow) + Lo(SC) * 2, (Hi(SC)-Lo(SC))*2+1)`
/// （`MagicManager.cs:213` / `:444`）。
///
/// 取数次序（C# 实参从左到右）：`MPow` 的 `Random(MaxPower-Power)` → `GetPower` 的
/// `Random(DefMaxPower-DefPower)`（`R(0)` 也推进）→ `GetAttackPower` 内
/// `Random((Δ)*2+1 + 1)` → `R(10 - MAX(0,-Luck))`（玩家 Luck=0 时即 `R(10)`）。
pub fn power_healing(
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

/// **P3-B2（爆炸/雷光/冰咆哮/地狱烈焰 23/24/33/47）**：
/// `GetAttackPower(GetPower(MPow) + Lo(MC), Hi(MC)-Lo(MC)+1)`（`MagicManager.cs:419/425/490/596`）。
pub fn power_magic_attack(
    rng: &mut impl RandomSource,
    magic: &MagicInfo,
    level: u8,
    mc: i32,
    atk: AttackPowerMode,
) -> i32 {
    let mp = mpow(rng, magic);
    let base = get_power(rng, magic, level, mp) + i32::from(lo_byte(mc));
    atk_power(
        rng,
        base,
        i32::from(hi_byte(mc)) - i32::from(lo_byte(mc)) + 1,
        atk,
    )
}

// ---- context_required 效果：独立可调方法（第七片真身向量）------------------------------------
//
// 这两条是 `DoSpell` 的 `case` 体里**唯一**不需要地图/可见集/召唤的独立方法：
// `MagTurnUndead`（32 圣言术）与 `MagTamming`（20 诱惑之光）。其余 context_required 分支
// （`MagPushArround`/`MagSaceMove`/`MagMakeSlave`/`MagMakeSinSuSlave`/`MagMakeAngelSlave`/
// `MagMakeClone`）体内部走 `Envir`/`GetMapBaseObjects`/`MakeSlave`，本片只登记可达性。
// 向量来源：`mir2-rs/tests/parity/m3/magic-effect-probe`（反射直调真身，记录 System.Random 取数账本）。

// ---- P3-B3：状态/辅助/不死六魔号的取数站点 ------------------------------------------------
//
// 六个魔号的取数必须落在本文件（`consumer_groups.tsv` 的 `magic_effect` 组）；留在
// `crates/world/src/magic.rs` 会被 `RUST_ATTACK_RE` 整文件误归 `attack` 组。

/// `MagMakeHolyCurtain` 的逐对象抽：`Random(4)`（`MagicManager.cs:1166`）。
///
/// 只在 `targetObject.Race >= ActorRace.Animal` **之后**抽（C# `&&` 短路）：`Race` 不合格或
/// `Master != null` 的判定次序见 [`crate::world::World::cast_holy_curtain_spell`]。
#[must_use]
pub fn holy_curtain_draw(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(4)
}

/// `MagMakePrivateTransparent` 的逐对象抽：`Random(2)`（`MagicManager.cs:686`）。
///
/// 只在 `|dx| <= 1 && |dy| <= 1` 之后抽（短路次序：`|dx|>1` ⇒ 不抽，`|dy|>1` ⇒ 不抽）。
#[must_use]
pub fn private_transparent_draw(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(2)
}

/// 心灵启示（28）的门：`Random(6) <= userMagic.Level + 3`（`MagicManager.cs:434`）。
#[must_use]
pub fn show_hp_gate_draw(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(6)
}

/// 解毒术常量（40）：`Random(7) - (userMagic.Level + 1) < 0`（`MagicManager.cs:504`）。
#[must_use]
pub fn unamyounsul_gate_draw(rng: &mut impl RandomSource) -> i32 {
    rng.random_below(7)
}

// ---- P3-B4（群体列表 / 毒族：6/48/50/42）----------------------------------------------

/// 施毒术族的抗毒骰：`Random(target.AntiPoison + 7) <= 6`
/// （`MagicManager.cs:235`（6）/ `:914`（48））。
///
/// `n == 0` 也**照抽一次**（oracle `Random(0)` 走 `Next(0)`，推进 `Sample`）。
/// 返回 `true` = 骰门通过（后续才取 `GP13`/`RP` 并投毒）。
#[must_use]
pub fn poison_dice(rng: &mut impl RandomSource, anti_poison: i32) -> bool {
    rng.random_below(anti_poison + 7) <= 6
}

/// `GameSvrConf` 里 `MabMabe` / 施毒术用到的配置项（`GameSvrConf.cs:1778-1781` /
/// `:1763`）。缺省值即 oracle 缺省。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MabMabeConfig {
    pub hit_rand_rate: i32,
    pub hit_min_lv_limit: i32,
    pub hit_success_rate: i32,
    pub hit_mabe_time_rate: i32,
    /// `AmyOunsulPoint`（6/48 的毒 `point` 公式分母）。
    pub amy_ounsul_point: i32,
}

impl Default for MabMabeConfig {
    fn default() -> Self {
        Self {
            hit_rand_rate: 100,
            hit_min_lv_limit: 10,
            hit_success_rate: 21,
            hit_mabe_time_rate: 20,
            amy_ounsul_point: 10,
        }
    }
}

/// `MagicManager.MabMabe`（`MagicManager.cs:1237-1245`）的门与逐抽结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MabMabeGates {
    /// `target.AntiMagic <= Random(10)`（第 1 抽；失败 ⇒ 其余门都不执行、不再抽）。
    pub anti_magic_pass: bool,
    /// `|target.CurrX - nTargetX| <= 1 && |target.CurrY - nTargetY| <= 1`。
    /// 与 `R(10)` 在 oracle **同一个 `&&` 条件**里 ⇒ 距离失败时 `R(2)` 也不抽。
    pub in_range: bool,
    /// `Random(2) + (caster.Level - 1) > target.Level`（第 2 抽）。
    pub level_gate_pass: bool,
    /// `caster.Level - target.Level`（仅 `level_gate_pass` 时有意义）。
    pub n_lv: i32,
    /// `Random(MabMabeHitRandRate) < MAX(MabMabeHitMinLvLimit, nLevel*8 - nLevel + 15 + nLv)`。
    pub rand_gate_pass: bool,
    /// `Random(MabMabeHitSucessRate) < nLevel*2 + 4`。
    pub success_gate_pass: bool,
}

/// `MagicManager.MabMabe`（`MagicManager.cs:1233-1245`）的**纯骰门**部分。
///
/// `MagCanHitTarget` / `IsProperTarget` / 坐标距离门是调用方的纯门（零抽），必须在调用本函数
/// **之前**过完（oracle `:1233/:1235/:1237` 的 `&&` 短路次序）。
///
/// 逐抽次序（合同 §3.3）：
/// 1. `R(10)`（`AntiMagic <= Random(10)`）；距离门与它同条件（`in_range` 为假 ⇒ 到此为止）；
/// 2. `R(2) + (caster_level - 1) > target_level`；
/// 3. `R(hit_rand_rate) < MAX(hit_min_lv_limit, magic_level*8 - magic_level + 15 + nLv)`；
/// 4. `R(hit_success_rate) < magic_level*2 + 4`。
pub fn mab_mabe_gates(
    rng: &mut impl RandomSource,
    anti_magic: i32,
    in_range: bool,
    caster_level: i32,
    target_level: i32,
    magic_level: u8,
    cfg: &MabMabeConfig,
) -> MabMabeGates {
    // `:1237 targetObject.AntiMagic <= Random(10) && |Δx| <= 1 && |Δy| <= 1`
    let mut out = MabMabeGates {
        anti_magic_pass: anti_magic_roll(rng, anti_magic),
        in_range,
        ..MabMabeGates::default()
    };
    if !out.anti_magic_pass || !in_range {
        return out;
    }
    // `:1240 Random(2) + (playObject.Abil.Level - 1) > targetObject.Abil.Level`
    if rng.random_below(2) + (caster_level - 1) > target_level {
        out.level_gate_pass = true;
        let n_lv = caster_level - target_level;
        out.n_lv = n_lv;
        let level = i32::from(magic_level);
        // `:1243 Random(MabMabeHitRandRate) < MAX(MabMabeHitMinLvLimit, nLevel*8 - nLevel + 15 + nLv)`
        let threshold = hutil::max_i32(cfg.hit_min_lv_limit, level * 8 - level + 15 + n_lv);
        if rng.random_below(cfg.hit_rand_rate) < threshold {
            out.rand_gate_pass = true;
            // `:1245 Random(MabMabeHitSucessRate) < nLevel*2 + 4`
            if rng.random_below(cfg.hit_success_rate) < level * 2 + 4 {
                out.success_gate_pass = true;
            }
        }
    }
    out
}

/// 施毒术毒 `point`：`Round(userMagic.Level / 3.0 * (nPower / (double)AmyOunsulPoint))`
/// （`MagicManager.cs:241/245/921/925`）。
#[must_use]
pub fn poison_point(level: u8, n_power: i32, amy_ounsul_point: i32) -> i32 {
    hutil::round(f64::from(level) / 3.0 * (f64::from(n_power) / f64::from(amy_ounsul_point)))
}

/// `Grobal2.LA_UNDEAD`（`src/OpenMir2/Grobal2.cs:66`）。
///
/// **source-cited**（常量，未单独成方法）。
pub const LA_UNDEAD: u8 = 1;

/// **P3-B5**：22/36 的配置项（`GameSvrConf.cs:1764` 的 `DisableInSafeZoneFireCross` +
/// `SysMsg` 的颜色对；部署值取自 `Server.conf`，本仓由宿主灌入）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct B5Config {
    /// `[Setup] DisableInSafeZoneFireCross`（源码缺省 `false`；**现网 `Server.conf:854` = 1**）。
    pub disable_in_safe_zone_fire_cross: bool,
    /// `[Setup] RedMsgFColor` / `RedMsgBColor`（22 安全区提示，`MsgColor.Red` + `MsgType.Notice`）。
    pub red_msg_f_color: u8,
    pub red_msg_b_color: u8,
    /// `[Setup] GreenMsgFColor` / `GreenMsgBColor`（36 的 `AttPowerUp` 提示，`MsgColor.Green`）。
    pub green_msg_f_color: u8,
    pub green_msg_b_color: u8,
}

impl Default for B5Config {
    fn default() -> Self {
        // 源码缺省 `DisableInSafeZoneFireCross=false`（`GameSvrConf.cs:1764`）、颜色缺省
        // `0xFF/0x38/0xDB/0xFF`（`GameSvrConf.cs:1536-1540`）。
        Self {
            disable_in_safe_zone_fire_cross: false,
            red_msg_f_color: 0xFF,
            red_msg_b_color: 0x38,
            green_msg_f_color: 0xDB,
            green_msg_b_color: 0xFF,
        }
    }
}

/// `MagicManager.MagTurnUndead`（`src/M2Server/Magic/MagicManager.cs:827-856`）的完整口径。
///
/// 逐字移植（`:834` 的 `Struck` 见 [`TurnUndeadInput::target_creature_present`]）：
/// 1. `if (target.SuperMan || target.LifeAttrib != LA_UNDEAD) return false;`
/// 2. `((AnimalObject)target).Struck(playObject)`（`AnimalObject.cs:336-359`）：
///    - `TargetCret == null` ⇒ **不抽** `Random(6)`（短路第一支）；否则 `GetAttackDir(TargetCret)`
///      为真也**不抽**（短路第二支）；否则抽 `Random(6)`，`==0` 才进内层；
///    - 进内层且 `IsProperTarget(hiter)` ⇒ `SetTargetCreat(hiter)`；
///    - `Animal` ⇒ **无条件**抽 `Random(300)`（`MeatQuality` 的 `ushort` 回绕由调用方落）；
///    - `AttackTick += 150 - MIN(130, Abil.Level*4)`。
/// 3. `if (target.TargetCret == null) { RunAwayMode = true; … }`（读第 2 步之后的值）。
/// 4. `playObject.SetTargetCreat(target)`。
/// 5. `if (Random(2) + (caster.Lv - 1) > target.Lv)` 且 `target.Lv < cfg`：
///    `level = caster.Lv - target.Lv`，再 `if (Random(100) < (magicLevel << 3) - magicLevel + 15 + level)`
///    ⇒ `target.WAbil.HP = 0; result = true;`。
///
/// 类型面（`(AnimalObject)target` 的 `InvalidCastException`）在调用方判：
/// [`TurnUndeadInput::target_is_animal_object`] 为假 ⇒ 本函数**不得**被调用（oracle 会抛异常）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnUndeadOutcome {
    pub result: bool,
    pub target_hp_zeroed: bool,
    pub target_targeted_by_caster: bool,
    pub target_run_away: bool,
    pub caster_target_is_target: bool,
    /// `Struck` 的 `Random(300)` 结果（`Animal` 为真时恰一次；否则 `None`）。
    pub meat_draw: Option<i32>,
    /// `Struck` 尾 `AttackTick += 150 - MIN(130, Abil.Level * 4)`（目标 `Abil.Level` 决定）。
    pub attack_tick_delta: i32,
}

/// `MagicManager.MagTurnUndead`（`MagicManager.cs:827-856`）的入参。
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
    /// 进入 `Struck` 时 `target.TargetCret != null`（决定是否可能抽 `Random(6)`）。
    pub target_creature_present: bool,
    /// `GetAttackDir(TargetCret, ref dir)` 为真（目标当前目标在其相邻 8 格内）——
    /// 仅在 `target_creature_present` 为真时有意义；为真 ⇒ **不抽** `Random(6)`。
    pub attack_dir_ok: bool,
    /// `IsProperTarget(hiter)`（目标视角的施法者）；决定短路命中后是否 `SetTargetCreat`。
    pub caster_is_proper_target: bool,
    /// 目标是否 `AnimalObject`（`Race != ActorRace.Play`）。为假 ⇒ oracle 抛 `InvalidCastException`，
    /// 调用方不得调用本函数（`TurnUndeadOutcome` 全默认）。
    pub target_is_animal_object: bool,
}

/// 见 [`TurnUndeadInput`]/[`TurnUndeadOutcome`]（`MagicManager.cs:827-856`）。
pub fn mag_turn_undead(rng: &mut impl RandomSource, inp: &TurnUndeadInput) -> TurnUndeadOutcome {
    let mut out = TurnUndeadOutcome::default();
    if inp.super_man || inp.life_attrib != LA_UNDEAD || !inp.target_is_animal_object {
        return out;
    }
    // `:834` `((AnimalObject)target).Struck(playObject)`（`AnimalObject.cs:336-359`）。
    // `:342` `TargetCret == null || GetAttackDir(...) || Random(6) == 0` —— C# `||` 短路。
    let struck_branch = if !inp.target_creature_present || inp.attack_dir_ok {
        true
    } else {
        rng.random_below(6) == 0
    };
    if struck_branch && inp.caster_is_proper_target {
        // `:344-347` `SetTargetCreat(hiter)`。
        out.target_targeted_by_caster = true;
    }
    // `:350-356` `if (Animal) MeatQuality = (ushort)(MeatQuality - Random(300))`（无条件抽）。
    if inp.animal {
        out.meat_draw = Some(rng.random_below(300));
    }
    // `:358` `AttackTick = AttackTick + (150 - MIN(130, Abil.Level * 4))`。
    out.attack_tick_delta = 150 - 130.min(inp.target_level * 4);
    // `:835-840` Struck 之后重读 `TargetCret == null`（短路未命中且未写目标 ⇒ 逃跑）。
    let target_present_after = inp.target_creature_present || out.target_targeted_by_caster;
    out.target_run_away = !target_present_after;
    // `:841` `playObject.SetTargetCreat(targetObject)`（无条件）。
    out.caster_target_is_target = true;
    // `:842-853`。
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
    /// 受奴成功时 oracle 抽的 `Random(caster.Lv * 2)`（`MagicManager.cs:770`）——
    /// `MasterRoyaltyTick = (draw + (magicLevel<<2)*5 + 20)*60_000 + GetTickCount()`。
    /// 未受奴时为 0（不抽数）。调用方把它交给 `World::tame_adopt` / `tame_royalty_tick`。
    pub royalty_draw: i32,
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
                                                        // 表面积化：把抽到的值随结果返回（调用方算到期 tick）。
                                out.royalty_draw = rng.random_below(inp.caster_level * 2);
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

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_shared::rng::RandomNumber;

    /// 取数式逐值钉死（seed 固定；`Next(10)` 在多个索引同为 1 ⇒ 用**连续三抽**，不用单抽）。
    #[test]
    fn draw_formulas_and_order_source_cited() {
        let mut a = RandomNumber::with_seed(20261017);
        let mut b = RandomNumber::with_seed(20261017);
        // 同一条流：chance 先抽 20，再 steps 抽 2。期望表 = 独立复算（同 seed 的另一实例）。
        let chance = push_chance(&mut a, 3, 2);
        let steps = push_steps(&mut a, 3);
        let want_chance = b.random_below(20) < 6 + 3 * 3 + 2;
        let want_steps = (1 + 2 + b.random_below(2)) as u8;
        assert_eq!(chance, want_chance);
        assert_eq!(steps, want_steps);
        // 随后连续三抽也应逐项一致（证明前面恰好消费 2 个样本、次序未错位）。
        for _ in 0..3 {
            assert_eq!(a.random_below(10), b.random_below(10));
        }

        let mut c = RandomNumber::with_seed(20261017);
        let mut d = RandomNumber::with_seed(20261017);
        let got = sace_move_gate(&mut c, 2);
        let want = d.random_below(11) < 2 * 2 + 4;
        assert_eq!(got, want);
        for _ in 0..3 {
            assert_eq!(c.random_below(10), d.random_below(10));
        }
    }

    /// 红检（牙）：把公式改坏 ⇒ 随机取样的用例必红。
    #[test]
    fn teeth_wrong_threshold_must_fail() {
        // `6 + 0*3 + 0` = 6：seed 下 `Random(20)` 的样本会落在两侧。
        let mut bad = RandomNumber::with_seed(7);
        let mut good = RandomNumber::with_seed(7);
        let mut diff = false;
        for _ in 0..64 {
            let g = push_chance(&mut good, 0, 0);
            // 故意漏掉 `+6`（错式）。
            let b = bad.random_below(20) < 0;
            if g != b {
                diff = true;
                break;
            }
        }
        assert!(diff, "错式必须与正确式出现分歧（牙不成立）");
    }
    /// **B1**：抗魔骰 = `Random(10) >= AntiMagic`（一抽；与 `AntiMagic <= Random(10)` 同式）。
    #[test]
    fn anti_magic_roll_is_one_draw_and_matches_oracle_comparison() {
        for seed in [1, 7, 42, 20261017] {
            let mut a = RandomNumber::with_seed(seed);
            let mut b = RandomNumber::with_seed(seed);
            let want = b.random_below(10) >= 1;
            assert_eq!(anti_magic_roll(&mut a, 1), want);
            // 恰好消费 1 个样本。
            assert_eq!(a.random(), b.random());
        }
        // `AntiMagic >= 10` ⇒ 恒失败（`Random(10) ∈ [0,9]`）。
        let mut c = RandomNumber::with_seed(9);
        assert!(!anti_magic_roll(&mut c, 10));
        // `AntiMagic <= 0` ⇒ 恒通过。
        let mut d = RandomNumber::with_seed(9);
        assert!(anti_magic_roll(&mut d, 0));
    }

    /// **B1（44）**：`MagHbFireBall` 推挤门 —— 门关**零抽**；门开恰抽 `Random(20)`，
    /// 通过后再抽 `Random(Level)`（`Level=0` 也抽，.NET `Next(0)` 仍调 `Sample()`）。
    #[test]
    fn hb_fireball_push_draw_order_and_gates() {
        // 门关（caster 等级不高于目标）：零抽。
        let mut a = RandomNumber::with_seed(5);
        let mut b = RandomNumber::with_seed(5);
        assert_eq!(hb_fireball_push(&mut a, 1, 1, 1, false), None);
        assert_eq!(a.random(), b.random(), "门关不得抽任何数");

        // 门开：先 Random(20)，再 Random(Level=1)。
        let mut c = RandomNumber::with_seed(11);
        let mut d = RandomNumber::with_seed(11);
        let got = hb_fireball_push(&mut c, 1, 5, 1, false);
        let p20 = d.random_below(20);
        if p20 < 6 + 3 + 4 {
            let plevel = d.random_below(1);
            let push = plevel - 1;
            assert_eq!(got, if push > 0 { Some(push as u8) } else { None });
        } else {
            assert_eq!(got, None, "Random(20) 未过门 ⇒ 不再抽 Random(Level)");
        }
        assert_eq!(format!("{c:?}"), format!("{d:?}"), "两抽次序固定");

        // `StickMode = true` ⇒ 门关（本骨架无该字段，保留参数口径）。
        let mut e = RandomNumber::with_seed(13);
        let mut f = RandomNumber::with_seed(13);
        assert_eq!(hb_fireball_push(&mut e, 1, 5, 1, true), None);
        assert_eq!(e.random(), f.random());
    }
}
