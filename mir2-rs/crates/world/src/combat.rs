//! 战斗子集 —— 移植源（逐条照抄，含注释里标注的 C# 行号）：
//! - `src/M2Server/Actor/BaseObject.Attack.cs:25` `GetBaseAttackPoewr`
//! - `BaseObject.Attack.cs:30` `_Attack`
//! - `BaseObject.Base.cs:886` `GetAttackPower`
//! - `BaseObject.Base.cs:789` `GetHitStruckDamage`
//! - `BaseObject.Base.cs:827` `StruckDamage`（职业对怪倍率、人物下属对人倍率）
//! - `BaseObject.cs:775` `DamageHealth`（护盾分支保留结构）
//! - `src/M2Server/Player/PlayObject.Base.cs:129/149` `WinExp` / `GetExp`（含单步升级）
//! - `src/M2Server/Actor/BaseObject.cs:2429` `KillFunc`（把击杀死讯发给 **`ExpHitter`**，不是 `LastHiter`）
//! - `src/M2Server/Player/PlayObject.cs:1632` `KillTargetTrigger`（`CalcGetExp` → `GainExp`）
//! - `src/M2Server/Actor/BaseObject.cs:1753` `SetLastHiter`（`LastHiter` 刷新、`ExpHitter` 粘第一个）
//! - `src/GameSrv/Word/WorldServer.MonGen.cs:440` `MonGetRandomItems`（**掉落在地图生成怪物时预算**，
//!   死亡时只是把预算好的物品散到地上）——由 `crates/formula::drop` 提供（D 线已对拍 == C#）
//!
//! **`LastHiter` 的写入点**（易错，本批修正）：`_Attack` 自己**不**写 `LastHiter`。
//! 人打怪时由 `targetObject.SendMsg(targetObject, RM_STRUCK, …)`（`BaseObject.Attack.cs:61-64`，
//! 条件是 `targetObject.Race > ActorRace.Play`）走消息队列，在怪物下一次 `Run()` 里
//! `AnimalObject.Operate`（`AnimalObject.cs:118-124`）调用 `SetLastHiter(攻击方)`。
//! ⇒ 同一次攻击里 `StruckDamage` 看到的仍是**上一次**的 `LastHiter`；本骨架把这一步
//! 折叠为"`_Attack` 结算完成后再 `set_last_hiter`"（见 `world::World::attack_once`），
//! 对"单/多攻击者逐次攻击"与 C# 同序（详见 `tests/parity/world/README.md` 的顺序契约）。
//!
//! 本批**不做**的（报告里写明）：技能（`MagicManager` 面太大）、狂暴/组队/任务条件、
//! 护身符与毒/麻痹状态、地图 `CanWalk` 地形（骨架格子属性恒 Walk）。

use mir2_data::models::StdItem;
use mir2_formula::drop::{mon_get_random_items, ItemCatalog, ItemNumberCounter, UserItem};
use mir2_formula::exp::{calc_get_exp, get_level_exp};
use mir2_shared::hutil;
use mir2_shared::rng::RandomSource;

use crate::entity::{Entity, UseItemSlot, ACTOR_RACE_PLAY};

/// `HUtil32.LoByte(short)`：低 8 位。
pub fn lo_byte(w: i32) -> u8 {
    w as u8
}

/// `HUtil32.HiByte(short)`：`(byte)(w >> 8)`。
pub fn hi_byte(w: i32) -> u8 {
    (w >> 8) as u8
}

/// `M2Share.GetGoldShape`（`M2Share.cs:498`）：金币落地外观。
pub fn get_gold_shape(gold: i32) -> u16 {
    let mut result: u16 = 112;
    if gold >= 30 {
        result = 113;
    }
    if gold >= 70 {
        result = 114;
    }
    if gold >= 300 {
        result = 115;
    }
    if gold >= 1000 {
        result = 116;
    }
    result
}

/// `BaseObject.GetAttackPower`（`BaseObject.Base.cs:886`）。
///
/// 注意 `power < 0` 先归零；`AutoChangeColor/FixColor` 两个倍率分支未启用（本骨架无变色逻辑）。
pub fn get_attack_power(rng: &mut impl RandomSource, base_power: i32, power: i32) -> i32 {
    let power = if power < 0 { 0 } else { power };
    base_power + rng.random_range(0, power + 1)
}

/// `BaseObject.GetBaseAttackPoewr`（`BaseObject.Attack.cs:25`）：
/// `GetAttackPower(LoByte(DC), (sbyte)(HiByte(DC) - LoByte(DC)))`。
pub fn get_base_attack_power(rng: &mut impl RandomSource, dc: i32) -> i32 {
    let (lo, diff) = base_power_parts(dc);
    get_attack_power(rng, lo, diff)
}

/// `WAbil.DC` → `(LoByte(DC), (sbyte)(HiByte(DC) - LoByte(DC)))`（`BaseObject.Attack.cs:27`）。
fn base_power_parts(dc: i32) -> (i32, i32) {
    let lo = i32::from(lo_byte(dc));
    let hi = i32::from(hi_byte(dc));
    let diff = (hi - lo) as i8 as i32; // C# 的 (sbyte) 强转
    (lo, diff)
}

/// `PlayObject.GetAttackPower`（`PlayObject.cs:2911`）——**玩家攻方**的 override。
///
/// 与 [`get_attack_power`]（`BaseObject` 版）的差别：多了 `Luck` / `PowerRate` / `BoPowerItem`
/// 三段，导致**取数次数不同**（`Luck == 0` 时每次多抽一次 `Random(10)`），
/// 所以必须单独建模，不能拿 Base 版代替（否则同种子序列整体错位）。
pub fn get_attack_power_player(
    rng: &mut impl RandomSource,
    base_power: i32,
    power: i32,
    luck: i32,
    power_rate: i32,
    bo_power_item: bool,
    power_item: u8,
) -> i32 {
    let power = if power < 0 { 0 } else { power };
    let mut result;
    if luck > 0 {
        // `if (Random(10 - _MIN(9, Luck)) == 0) result = basePower + power; else result = basePower + Random(power+1);`
        if rng.random_below(10 - hutil::min_i32(9, luck)) == 0 {
            result = base_power + power;
        } else {
            result = base_power + rng.random_below(power + 1);
        }
    } else {
        result = base_power + rng.random_below(power + 1);
        // `if (Luck <= 0) if (Random(10 - _MAX(0, -Luck)) == 0) result = basePower;`
        if luck <= 0 && rng.random_below(10 - hutil::max_i32(0, -luck)) == 0 {
            result = base_power;
        }
    }
    result = hutil::round(f64::from(result) * (f64::from(power_rate) / 100.0));
    if bo_power_item {
        result = hutil::round(f64::from(power_item) * f64::from(result));
    }
    result
}

/// `PlayObject.GetBaseAttackPoewr` 的 override 版：`GetAttackPower_player(Lo(DC), (sbyte)Δ)`。
pub fn get_base_attack_power_player(
    rng: &mut impl RandomSource,
    dc: i32,
    luck: i32,
    power_rate: i32,
    bo_power_item: bool,
    power_item: u8,
) -> i32 {
    let (lo, diff) = base_power_parts(dc);
    get_attack_power_player(rng, lo, diff, luck, power_rate, bo_power_item, power_item)
}

// -------------------------------------------------------------------------------------------
// DoSpell 之外的战士近战技能路径（`PlayObject.Attack.cs` / `PlayObject.cs`）
//
// 真魔表里 `IsWarrSkill`（`MagicManager.cs:82-100`）命中的 8 行，`DoSpell` 前置就 `return false`
// （见 `skill-probe` 的 `warr_reject` 登记）；它们的「效果」实际由 `PlayObject` 的近战路径施加。
// 本段移植其中**可独立反射直调**的两个入口（真身向量见 `tests/parity/m3/attack-hit-probe`）：
//   - [`recalc_hit_speed`]      ← `PlayObject.cs:3659-3713` `RecalcHitSpeed`（被动技能效果）
//   - [`get_attack_power_hit`]  ← `PlayObject.Attack.cs:579-625` `GetAttackPowerHit`（主动倍率）
// 其余 4 行（12/25/27/34）尾调走 `Envir`/地图目标集，只登记可达性，不在本段。
// -------------------------------------------------------------------------------------------

/// `MessageSettings.DEFHIT`（`MessageSettings.cs:10`）。
pub const DEFHIT: u16 = 5;
/// `MessageSettings.DEFSPEED`（`MessageSettings.cs:11`）。
pub const DEFSPEED: i32 = 15;

/// `MagicConst.SKILL_ONESWORD`（基本剑术）。
pub const SKILL_ONESWORD: u16 = 3;
/// `MagicConst.SKILL_ILKWANG`（精神力战法）。
pub const SKILL_ILKWANG: u16 = 4;
/// `MagicConst.SKILL_YEDO`（攻杀剑术）。
pub const SKILL_YEDO: u16 = 7;
/// `MagicConst.SKILL_FIRESWORD`（烈火剑法）。
pub const SKILL_FIRESWORD: u16 = 26;

/// `PlayObject.RecalcHitSpeed`（`PlayObject.cs:3659-3713`）的输入。
///
/// `tick_hit` / `tick_speed` = `SystemShare.Config.BonusAbilof{Job}.Hit/Speed`（oracle 按 `Job` 选表）。
/// `hit_point` / `speed_point` 是**初始值**：oracle 只在 Warrior/Wizard 分支写 `HitPoint`，
/// 道士分支保持原值，故这里显式带进带出（不是「从 0 开始」的纯函数）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitSpeedInput {
    pub job: u8,
    pub bonus_hit: u8,
    pub bonus_speed: i32,
    pub tick_hit: u8,
    pub tick_speed: i32,
    pub hit_point: u8,
    pub speed_point: u8,
    /// `MagicList` 顺序的 `(MagIdx, Level)`（顺序是契约：同一魔号出现两次会被处理两次）。
    pub magics: Vec<(u16, u8)>,
}

/// `PlayObject.RecalcHitSpeed` 的可观测输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HitSpeedOutcome {
    pub hit_point: u8,
    pub speed_point: u8,
    pub hit_plus: u16,
    pub hit_double: u16,
    pub attack_skill_count: u8,
    pub attack_skill_point_count: u8,
}

/// `PlayObject.RecalcHitSpeed`（`PlayObject.cs:3659-3713`）逐行移植。
///
/// 口径提醒（照抄 oracle，含 `byte` 截断与 `byte/byte` 整除）：
/// - `HitPoint += HUtil32.Round(...)` 是 `byte += int` ⇒ 隐式 `(byte)` 截断；
/// - `HitPlus = (byte)(DEFHIT + Level)`、`HitDouble = (byte)(4 + Level*4)` 同样先过 `byte`；
/// - 攻杀剑术分支**无论 Level 是否为 0 都会** `RandomByte(AttackSkillCount)`（取数不能少）。
// oracle 里攻杀剑术那行是 `HUtil32.Round(3 / 3.0 * userMagic.Level)`（`3 / 3.0` 为常量 1.0），
// 为保持 1:1 转写照抄字面式；clippy 的 `eq_op` 对此是误报，故显式放行。
#[allow(clippy::eq_op)]
pub fn recalc_hit_speed(rng: &mut impl RandomSource, input: &HitSpeedInput) -> HitSpeedOutcome {
    const JOB_WARRIOR: u8 = 0;
    const JOB_WIZARD: u8 = 1;

    let mut hit_point = input.hit_point;
    let mut speed_point = input.speed_point;
    // `HitPlus = 0; HitDouble = 0;`
    let mut hit_plus: u16 = 0;
    let mut hit_double: u16 = 0;
    let mut attack_skill_count: u8 = 0;
    let mut attack_skill_point_count: u8 = 0;

    match input.job {
        JOB_WARRIOR | JOB_WIZARD => {
            // HitPoint = (byte)(DEFHIT + BonusAbil.Hit / bonusTick.Hit)
            hit_point =
                (i32::from(DEFHIT) + i32::from(input.bonus_hit) / i32::from(input.tick_hit)) as u8;
            // SpeedPoint = (byte)(DEFSPEED + BonusAbil.Speed / bonusTick.Speed)
            speed_point = (DEFSPEED + input.bonus_speed / input.tick_speed) as u8;
        }
        // 道士：只算 SpeedPoint（`+ 3`），`HitPoint` 保持原值
        2 => {
            speed_point = (DEFSPEED + input.bonus_speed / input.tick_speed + 3) as u8;
        }
        // `PlayerJob` 只有 0/1/2；未知值 oracle 的 switch 落空 ⇒ 两项都不改（不是“当道士算”）。
        _ => {}
    }

    for &(mag_idx, level) in &input.magics {
        match mag_idx {
            SKILL_ONESWORD => {
                if level > 0 {
                    // HitPoint += (byte)Round(9 / 3.0 * Level)
                    hit_point =
                        (i32::from(hit_point) + hutil::round(9.0 / 3.0 * f64::from(level))) as u8;
                }
            }
            SKILL_ILKWANG => {
                if level > 0 {
                    hit_point =
                        (i32::from(hit_point) + hutil::round(8.0 / 3.0 * f64::from(level))) as u8;
                }
            }
            SKILL_YEDO => {
                if level > 0 {
                    hit_point =
                        (i32::from(hit_point) + hutil::round(3.0 / 3.0 * f64::from(level))) as u8;
                }
                // HitPlus = (byte)(DEFHIT + Level)
                hit_plus = u16::from((i32::from(DEFHIT) + i32::from(level)) as u8);
                // AttackSkillCount = (byte)(7 - Level); 无论 Level 是否为 0 都取数
                attack_skill_count = (7i32 - i32::from(level)) as u8;
                attack_skill_point_count = rng.random_byte(attack_skill_count);
            }
            SKILL_FIRESWORD => {
                // HitDouble = (byte)(4 + Level * 4)
                hit_double = u16::from((4 + i32::from(level) * 4) as u8);
            }
            _ => {}
        }
    }

    HitSpeedOutcome {
        hit_point,
        speed_point,
        hit_plus,
        hit_double,
        attack_skill_count,
        attack_skill_point_count,
    }
}

/// `PlayObject.GetAttackPowerHit`（`PlayObject.Attack.cs:579-625`）用到的对象状态。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttackHitState {
    pub power_hit: bool,
    pub fire_hit_skill: bool,
    pub twin_hit_skill: bool,
    pub hit_plus: u16,
    pub hit_double: u16,
}

/// `nPower = (ushort)(nPower + HUtil32.Round(nPower / 100 * HitDouble * 10.0))`
/// （`PlayObject.Attack.cs:594` / `:600`）——注意 `nPower / 100` 是**整除**，末段 `(ushort)` 截断。
fn hit_double_power(power: i32, hit_double: u16) -> i32 {
    let inc = hutil::round((power / 100 * i32::from(hit_double)) as f64 * 10.0);
    i32::from((power + inc) as u16)
}

/// `PlayObject.GetAttackPowerHit`（`PlayObject.Attack.cs:579-625`）逐行移植。
///
/// 返回 `(nPower, canHit)`，并**原地**清 `power_hit`/`fire_hit_skill`/`twin_hit_skill`。
/// `has_target == false` 时，mode 7/9 只清标志、不改威力、不置 `canHit`（oracle 的 null 分支）。
pub fn get_attack_power_hit(
    hit_mode: i16,
    power: i32,
    has_target: bool,
    state: &mut AttackHitState,
) -> (i32, bool) {
    let mut can_hit = false;
    let mut power = power;
    if has_target {
        match hit_mode {
            3 if state.power_hit => {
                state.power_hit = false;
                power += i32::from(state.hit_plus);
                can_hit = true;
            }
            7 if state.fire_hit_skill => {
                state.fire_hit_skill = false;
                power = hit_double_power(power, state.hit_double);
                can_hit = true;
            }
            9 if state.twin_hit_skill => {
                state.twin_hit_skill = false;
                power = hit_double_power(power, state.hit_double);
                can_hit = true;
            }
            _ => {}
        }
    } else {
        match hit_mode {
            3 if state.power_hit => {
                state.power_hit = false;
                power += i32::from(state.hit_plus);
                can_hit = true;
            }
            7 if state.fire_hit_skill => {
                state.fire_hit_skill = false;
            }
            9 if state.twin_hit_skill => {
                state.twin_hit_skill = false;
            }
            _ => {}
        }
    }
    (power, can_hit)
}

/// 攻方攻击力的建模口径（`_Attack` 的调用方：`GetBaseAttackPoewr`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttackPowerMode {
    /// `BaseObject.GetAttackPower`（怪物/无 override 的对象）——M2 既有 golden 用这条。
    Base,
    /// `PlayObject.GetAttackPower` override（`PlayObject.cs:2911`）。
    Player {
        luck: i32,
        power_rate: i32,
        bo_power_item: bool,
        power_item: u8,
    },
}

impl AttackPowerMode {
    /// 按攻方 `Race` 分派（oracle：`GetBaseAttackPoewr` 调**虚函数** `GetAttackPower`
    /// （`BaseObject.Attack.cs:25` → `BaseObject.Base.cs:886` 基版；`PlayObject.cs:2909` 玩家 override））。
    ///
    /// `Race == ActorRace.Play` ⇒ [`AttackPowerMode::Player`]；其余（怪物/召唤物/非玩家）⇒
    /// [`AttackPowerMode::Base`]。两版**取数次数不同**（`Luck <= 0` 时 override 多一次
    /// `Random(10)`）⇒ 必须按对象分派，不能一律走 `Base`。
    #[must_use]
    pub fn for_attacker(attacker: &Entity) -> AttackPowerMode {
        if attacker.race == ACTOR_RACE_PLAY {
            AttackPowerMode::Player {
                luck: attacker.luck,
                power_rate: attacker.power_rate,
                bo_power_item: attacker.bo_power_item,
                power_item: attacker.power_item,
            }
        } else {
            AttackPowerMode::Base
        }
    }
}

// -------------------------------------------------------------------------------------------
// 攻击合法性（`IsProperTarget` 判定链）
//
// oracle 的 `_Attack`（`BaseObject.Attack.cs:37`）门是 **虚函数 `IsProperTarget`**，不是固定的
// race 规则。两条实现：
//   * 玩家攻方 → `PlayObject.IsProperTarget`（`PlayObject.cs:1929` override）：
//       先 `PlayObject.IsAttackTarget`（`PlayObject.Attack.cs:627` override，按 `AttatckMode` 分派）；
//       目标为 `Play` ⇒ 再 `IsProtectTarget`（安全区，`BaseObject.Base.cs:360`）；
//       目标为非玩召唤物 ⇒ 按主人/召唤者分支；否则回落 `BaseObject.IsAttackTarget`（`BaseObject.Base.cs:377`）。
//   * 非玩家攻方 → `BaseObject.IsProperTarget`（`BaseObject.Base.cs:470`）= `BaseObject.IsAttackTarget`。
// 两处 `IsAttackTarget` **不能混用**：玩家版是攻击模式 switch，基版是主人目标集。
//
// 折入的「目标可见性」门（单独注明，别混进 `IsProperTarget` 的等价断言）：
//   `Envir.GetMovingObject(x, y, true)`（`Envirnoment.cs:1203`）与接线层 `handle_hit_cmd` 的
//   `!Ghost && !Death` 目标筛选；本骨架把「已死/幽灵/隐身不可被选为攻击目标」折进本函数顶部，
//   与 oracle 的最终可攻击集合同解（oracle 把它放在取目标那一层）。
// -------------------------------------------------------------------------------------------

/// `src/OpenMir2/Enums/Race.cs`。
pub const ACTOR_RACE_NPC: u8 = 10;
pub const ACTOR_RACE_PEACE_NPC: u8 = 15;
pub const ACTOR_RACE_ANIMAL: u8 = 50;

/// `src/OpenMir2/Enums/AttackMode.cs`。
pub const ATTACK_MODE_HAM_ALL: u8 = 0;
pub const ATTACK_MODE_HAM_PEACE: u8 = 1;
pub const ATTACK_MODE_HAM_DEAR: u8 = 2;
pub const ATTACK_MODE_HAM_MASTER: u8 = 3;
pub const ATTACK_MODE_HAM_GROUP: u8 = 4;
pub const ATTACK_MODE_HAM_GUILD: u8 = 5;
pub const ATTACK_MODE_HAM_PKATTACK: u8 = 6;

/// `BaseObject.IsProtectTarget`（`BaseObject.Base.cs:360`）：`true` = **允许**（双方都不在安全区）。
///
/// 名字带 `Protect` 但语义是「不被安全区保护」：oracle `if (InSafeZone() || target.InSafeZone()) return false; return true;`。
pub fn is_protect_target(attacker: &Entity, target: &Entity) -> bool {
    !(attacker.in_safe_zone || target.in_safe_zone)
}

/// `BaseObject.IsAttackTarget`（`BaseObject.Base.cs:377`）——**基版**（非玩家攻方，或玩家版回落）。
///
/// 未建模分支（报告登记）：`Race >= Animal` 且 `Master != null` 的主从目标集（需要取主人的
/// `LastHiter/ExpHitter/TargetCret` 实体，本函数只拿得到 `&Entity`）、`CrazyMode`（本骨架无狂暴）、
/// `HolySeize`/`SlaveRelax`。本骨架驱怪攻人的路径 `Master == null`，该分支逐行覆盖。
pub fn is_attack_target_base(attacker: &Entity, target: &Entity) -> bool {
    if attacker.id == target.id {
        return false;
    }
    if target.admin_mode || target.stone_mode {
        return false;
    }
    if attacker.race >= ACTOR_RACE_ANIMAL {
        let mut result = false;
        if attacker.master.is_none() {
            // `else` 支（`BaseObject.Base.cs:438-452`）：
            if target.race == ACTOR_RACE_PLAY {
                result = true; // 怪打人
            }
            if attacker.race > ACTOR_RACE_PEACE_NPC && attacker.race < ACTOR_RACE_ANIMAL {
                result = true;
            }
            if target.master.is_some() {
                result = true; // 可打对方召唤物
            }
        }
        // `CrazyMode` 未建模（本骨架无狂暴态）；`NastyMode` 已建模。
        if attacker.nasty_mode
            && (target.race < ACTOR_RACE_NPC || target.race > ACTOR_RACE_PEACE_NPC)
        {
            result = true;
        }
        return result;
    }
    // `Race < Animal` 的攻方：oracle 直接 `return true`（`BaseObject.Base.cs:463`）。
    true
}

/// `PlayObject.IsAttackTarget`（`PlayObject.Attack.cs:627`）——按 `AttatckMode` 分派的**玩家版**。
///
/// 未建模分支（报告登记）：`PveServer`（现网 `Server.conf NonPKServer=FALSE` ⇒ 默认不触发）、
/// `IsGroupMember`/`MyGuild`/`MasterList`（师徒）——本骨架世界无组队/行会/师徒，`result` 不被剔除。
pub fn is_attack_target_player(attacker: &Entity, target: &Entity) -> bool {
    let mut result = false;
    match attacker.attack_mode {
        ATTACK_MODE_HAM_ALL => {
            if target.race < ACTOR_RACE_NPC || target.race > ACTOR_RACE_PEACE_NPC {
                result = true;
            }
            // `if (PveServer) return true;` 登记未建模（默认 false）。
        }
        ATTACK_MODE_HAM_PEACE => {
            if target.race >= ACTOR_RACE_ANIMAL {
                return true;
            }
        }
        ATTACK_MODE_HAM_DEAR => {
            if Some(target.id) != attacker.dear_human {
                return true;
            }
        }
        ATTACK_MODE_HAM_MASTER => {
            if target.race == ACTOR_RACE_PLAY {
                result = true;
                // `MasterList`（师徒）为空 ⇒ 两处剔除都不发生。
            } else {
                result = true;
            }
        }
        ATTACK_MODE_HAM_GROUP => {
            if target.race < ACTOR_RACE_NPC || target.race > ACTOR_RACE_PEACE_NPC {
                result = true;
            }
            // `IsGroupMember` 未建模（无组队）⇒ 不剔除。
        }
        ATTACK_MODE_HAM_GUILD => {
            if target.race < ACTOR_RACE_NPC || target.race > ACTOR_RACE_PEACE_NPC {
                result = true;
            }
            // `MyGuild` 未建模（无行会）⇒ 不剔除。
        }
        ATTACK_MODE_HAM_PKATTACK => {
            if target.race < ACTOR_RACE_NPC || target.race > ACTOR_RACE_PEACE_NPC {
                result = true;
            }
            if target.race == ACTOR_RACE_PLAY {
                let atk_pvp = (attacker.pk_point / 100) as u8;
                let tgt_pvp = (target.pk_point / 100) as u8;
                if atk_pvp >= 2 {
                    result = tgt_pvp < 2;
                } else {
                    result = tgt_pvp >= 2;
                }
            }
        }
        _ => {}
    }
    result
}

/// `PlayObject.IsProperTarget`（`PlayObject.cs:1929`）——**玩家攻方**的判定链。
fn is_proper_target_play(attacker: &Entity, target: &Entity) -> bool {
    let mut result = is_attack_target_player(attacker, target);
    if result {
        if target.race == ACTOR_RACE_PLAY {
            // `if (baseObject.Race == ActorRace.Play) return IsProtectTarget(baseObject);`
            return is_protect_target(attacker, target);
        }
        if let Some(master) = target.master {
            if master == attacker.id {
                // 自己的宝宝：非 `HAM_ALL` 直接拒绝；`HAM_ALL` 落到下面的 `if (result) return result`。
                if attacker.attack_mode != ATTACK_MODE_HAM_ALL {
                    return false;
                }
            } else {
                // 对方召唤者：`result = base.IsAttackTarget(...)`，再 `if (IsProtectTarget(...)) result = false;`
                // ——**照抄 oracle 的这段反直觉写法**（`IsProtectTarget` 为真时应置假）。
                result = is_attack_target_base(attacker, target);
                if is_protect_target(attacker, target) {
                    result = false;
                }
            }
        }
        if result {
            return result;
        }
        return is_attack_target_base(attacker, target);
    }
    false
}

/// 攻击合法性（`BaseObject.Attack.cs:37` 的 `IsProperTarget`，虚函数分派）。
///
/// 顶部折入「已死/幽灵/隐身不可被选为目标」（见本节注释）；其余逐行照 oracle。
pub fn is_proper_target(attacker: &Entity, target: &Entity) -> bool {
    if attacker.id == target.id {
        return false;
    }
    if target.death || target.ghost || target.invisible {
        return false;
    }
    if attacker.race == ACTOR_RACE_PLAY {
        return is_proper_target_play(attacker, target);
    }
    is_attack_target_base(attacker, target)
}

/// 兼容别名：`crates/world` 旧调用点与既有用例用的名字（现与 [`is_proper_target`] 同义）。
pub fn is_attack_target(attacker: &Entity, target: &Entity) -> bool {
    is_proper_target(attacker, target)
}

/// `ItemLocation.Dress`（`src/OpenMir2/Enums/ItemLocation.cs:7`）：`UseItems` 的衣服位。
pub const ITEM_LOCATION_DRESS: usize = 0;

/// `PlayObject.StruckDamage` 的单格耐久磨损（`PlayObject.cs:2965-2996` 衣服位特判 /
/// `:3005-3038` 逐格循环共用）。
///
/// 逐字复刻 oracle 的两点易错语义（**不是**「减到 0 就停」）：
///   1. `ushort nDura; nDura -= nDam;` 是 **unchecked ushort 回绕**（工程默认
///      `CheckForOverflowUnderflow=false`，`Directory.Build.targets` 未开启 checked）
///      ⇒ `Dura < nDam` 时结果是**大正数**，**不破零**；
///   2. `if (nDura <= 0)` 对 `ushort` 等价于 `nDura == 0` ⇒ 只有 `Dura` **恰好等于** `nDam`
///      才破零；破零后 `Index = 0`（`:2982`/`:3008`）+ `Dura = 0`（`:2985`/`:3012`）。
///
/// 为什么必须逐字：破零清 `Index` 会改变**同一击**里逐格循环是否给该格抽 `Random(8)`
/// （衣服位特判在循环之前跑），从而让全局 RNG 流错位。
fn wear_durability(slot: &mut UseItemSlot, n_dam: u16) {
    let next = slot.dura.wrapping_sub(n_dam);
    if next == 0 {
        slot.index = 0;
        slot.dura = 0;
    } else {
        slot.dura = next;
    }
}

/// `PlayObject.StruckDamage`（`PlayObject.cs:2954-3036`）在 `base.StruckDamage` 之后、
/// 只与**耐久与取数**有关的那一部分（`nDam` 抽取 + 衣服位特判 + 逐格循环）。
///
/// RNG 消耗（仅玩家目标）：`Random(10)` 一次（`:2957`），再对每个 `Index > 0` 的格子各抽
/// `Random(8)`（`:3000`）——衣服位在特判里破零清 `Index` 会**少抽一次**（见 [`wear_durability`]）。
///
/// 怪物目标走 `BaseObject.StruckDamage`（`BaseObject.Base.cs:827`，无随机数）⇒ 直接返回。
pub fn struck_damage_durability(rng: &mut impl RandomSource, target: &mut Entity) {
    if target.race != ACTOR_RACE_PLAY {
        return;
    }
    // `usort nDam = (ushort)(Random(10) + 5)`（`PlayObject.cs:2957`）。
    let n_dam = (rng.random_below(10) + 5) as u16;
    // 衣服位特判（`PlayObject.cs:2965-2996`）：**不抽数**，只扣耐久；破零先清 `Index`。
    let dress = &mut target.use_items[ITEM_LOCATION_DRESS];
    if dress.index > 0 {
        wear_durability(dress, n_dam);
    }
    // 逐格循环（`PlayObject.cs:2998-3039`）：每格 `Index > 0` **先抽一次 `Random(8)`**。
    for slot in target.use_items.iter_mut() {
        if slot.index > 0 && rng.random_below(8) == 0 {
            wear_durability(slot, n_dam);
        }
    }
}

/// `BaseObject.GetHitStruckDamage`（`BaseObject.Base.cs:789`）——目标侧护甲减免。
///
/// 逐字语义：`nRnd = LoByte(AC) + Random(|HiByte(AC)-LoByte(AC)| + 1)`；
/// `nRnd > 0` 时 `nArmor = LoByte(AC) + Random(nRnd)`，否则 `nArmor = LoByte(AC)`；
/// `damage = max(0, damage - nArmor)`；不死系再加攻击方的 `UndeadPower`。
pub fn get_hit_struck_damage(
    rng: &mut impl RandomSource,
    target: &Entity,
    attacker: &Entity,
    n_damage: i32,
) -> u16 {
    let ac_lo = i32::from(lo_byte(target.ac));
    let ac_hi = i32::from(hi_byte(target.ac));
    let n_rnd = ac_lo + rng.random_range(0, (ac_hi - ac_lo).abs() + 1);
    let n_armor = if n_rnd > 0 {
        ac_lo + rng.random_range(0, n_rnd)
    } else {
        ac_lo
    };
    let mut damage = hutil::max_i32(0, n_damage - n_armor);
    if damage > 0 && target.life_attrib == 1 {
        // `if ((LifeAttrib == Grobal2.LA_UNDEAD) && (target != null)) nDamage += target.AddAbil.UndeadPower;`
        // 注意：oracle **无条件**相加（没有 `> 0` 守卫），UndeadPower 为负时也照加。
        damage += attacker.undead_power;
    }
    damage as u16
}

/// 目标实体当下存着的 `LastHiter` 子集（`StruckDamage` 的倍率只看它，不看本次攻击者）。
///
/// 为什么要有这个类型：`_Attack` **不写** `LastHiter`，命中者登记走 `RM_STRUCK` 消息、
/// 由目标自己的 `Run()` 调 `SetLastHiter`（见 `attack()` 末尾与 `world::attack_once`）。
/// ⇒ 本次攻击的 `StruckDamage` 看到的 `LastHiter` 是**上一次**的命中者——这是 oracle 的真实语义，
/// 倍率必须按它取，不能按本次攻击者取。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LastHitter {
    /// `LastHiter.Race`
    pub race: u8,
    /// `LastHiter.Job`
    pub job: u8,
    /// `LastHiter.Master != null`
    pub has_master: bool,
}

/// `BaseObject.StruckDamage`（`BaseObject.Base.cs:827`）的职业/主从倍率部分。
///
/// 配置项取 `GameSvrConf` 默认值（`WarrMon/WizardMon/TaosMon/MonHum` 都是 10 ⇒ 倍率 1.0，
/// 与现网 Server.conf 未改动时一致；如需别的值由调用方传入）。
///
/// **取谁的 job**：oracle 用 `LastHiter`（`:833-846` 的 `LastHiter.Race == Play` + `((IPlayerActor)LastHiter).Job`），
/// 不是本次攻击者；默认配置下两者都归到倍率 1.0，只有非默认倍率或同 tick 混职业时才有数值差异。
pub fn struck_damage_multiplier(
    target: &Entity,
    last_hitter: Option<LastHitter>,
    warr_mon: i32,
    wizard_mon: i32,
    taos_mon: i32,
    mon_hum: i32,
) -> i32 {
    // 返回"十分之几"的倍率（C# 里是 `伤害 * per10 / 10`）
    if let Some(hitter) = last_hitter.filter(|h| target.race >= 50 && h.race == ACTOR_RACE_PLAY) {
        // `(Race >= ActorRace.Animal) && (LastHiter != null) && (LastHiter.Race == ActorRace.Play)`
        match hitter.job {
            0 => warr_mon,   // Warrior
            1 => wizard_mon, // Wizard
            _ => taos_mon,   // Taoist
        }
    } else if last_hitter.is_some_and(|h| target.race == ACTOR_RACE_PLAY && h.has_master) {
        // `(Race == ActorRace.Play) && (LastHiter != null) && (LastHiter.Master != null)`
        mon_hum
    } else {
        10 // 无倍率
    }
}

/// `_Attack` 的一次结算结果（供对拍与断言）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AttackOutcome {
    /// 攻击方原始攻击力（`GetBaseAttackPoewr` 的结果）
    pub power: i32,
    /// 命中判定的结果：`false` = 未命中（`nPower` 被归零）
    pub hit: bool,
    /// 护甲减免后的实际伤害（`GetHitStruckDamage` 之后）
    pub damage: u16,
    /// 目标 HP 是否降到 0（本 tick 触发死亡）
    pub killed: bool,
    /// 结算后目标的 `WAbil.HP`（对拍逐行输出用）
    pub target_hp_after: u16,
}

/// `_Attack`（`BaseObject.Attack.cs:30`）——一次攻击的完整结算（攻方走 [`AttackPowerMode::Base`]）。
///
/// **RNG 消耗顺序**（与 C# 一致，勿改）：
/// 1. 攻击力：`GetAttackPower` → `Random(power+1)`（1 次）
/// 2. 命中判定：`RandomByte(target.SpeedPoint)` → `Random(SpeedPoint)`（1 次）
/// 3. 护甲：`Random(|ΔAC|+1)`，若 `nRnd>0` 再 `Random(nRnd)`（1~2 次）
pub fn attack(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
) -> AttackOutcome {
    attack_with(
        rng,
        attacker,
        target,
        last_hitter,
        config,
        AttackPowerMode::Base,
    )
}

/// [`attack`] 的带攻方口径版本（M3 战斗线：玩家攻方走 `PlayObject.GetAttackPower` override）。
pub fn attack_with(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
    power_mode: AttackPowerMode,
) -> AttackOutcome {
    let power = match power_mode {
        AttackPowerMode::Base => get_base_attack_power(rng, attacker.dc),
        AttackPowerMode::Player {
            luck,
            power_rate,
            bo_power_item,
            power_item,
        } => get_base_attack_power_player(
            rng,
            attacker.dc,
            luck,
            power_rate,
            bo_power_item,
            power_item,
        ),
    };
    let mut n_power = power;
    let mut hit = false;

    if is_proper_target(attacker, target) {
        // `if (targetObject.HitPoint > 0) { if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0; }`
        if target.hit_point > 0 && attacker.hit_point < rng.random_byte(target.speed_point) {
            n_power = 0;
        } else {
            hit = true;
        }
    } else {
        n_power = 0;
    }

    let mut damage: u16 = 0;
    let mut killed = false;
    if n_power > 0 {
        let reduced = get_hit_struck_damage(rng, target, attacker, n_power);
        if reduced > 0 {
            // `StruckDamage`：职业对怪倍率 / 主从对人倍率，然后 `DamageHealth`
            let per10 = struck_damage_multiplier(
                target,
                last_hitter,
                config.warr_mon,
                config.wizard_mon,
                config.taos_mon,
                config.mon_hum,
            );
            let final_damage = (i32::from(reduced) * per10 / 10).max(0) as u16;
            damage = final_damage;
            // `DamageHealth`：HP 扣减（护盾分支需要 MP 状态，本骨架未启用）
            if i32::from(target.hp) - i32::from(final_damage) > 0 {
                target.hp -= final_damage;
            } else {
                target.hp = 0;
            }
            if target.hp == 0 {
                killed = true;
            }
            // `PlayObject.StruckDamage`（`PlayObject.cs:2954`）在 `base.StruckDamage` 之后的作用：
            //   1. `:2957` **无条件**抽一次 `Random(10)`（护甲耐久磨损点数 `nDam = Random(10)+5`）；
            //   2. `:2965-2996` 衣服位（`UseItems[ItemLocation.Dress=0]`）特判——**不抽数**，只扣耐久，
            //      但破零会先清 `Index`（`:2982`），使逐格循环跳过该格的 `Random(8)`；
            //   3. `:2998-3039` 逐格循环：`Index > 0`（已装备）的每格**各抽一次 `Random(8)`**。
            // 玩家目标走这个 override，怪物走 `BaseObject.StruckDamage`（无随机数）。
            // 漏掉任一条都会让同种子跨侧取数账本从第一次命中起整体错位。
            struck_damage_durability(rng, target);
        }
    }
    // 注意：`_Attack` 里**没有** `LastHiter` 赋值（C# 原文如此）。命中者的登记走
    // `RM_STRUCK` 消息路径，由调用方在攻击结算之后调用 `Entity::set_last_hiter`。
    AttackOutcome {
        power,
        hit,
        damage,
        killed,
        target_hp_after: target.hp,
    }
}

/// `PlayObject.AttackSuccess`（`PlayObject.Attack.cs:215`）里与**取数**有关的那一部分。
///
/// oracle 的 `PlayObject.AttackDir` 在 `_Attack`（+`SendAttackMsg`，无取数）之后调
/// `AttackSuccess(wHitMode, nPower, canHit, attackTarget)`（`:77`）；只要 `attackTarget != null`
/// 就**无条件**抽一次 `Random(5)`（`:225` `nWeaponDamage = (ushort)(Random(5) + 2 - AddAbil.WeaponStrong)`）
/// —— 记录站点是 `PlayObject.AttackSuccess@PlayObject.Attack.cs:223`。
///
/// **命中/躲闪都抽**（`AttackSuccess` 在 `_Attack` 返回后无条件调用，不看 `canHit`/`nPower`）。
/// 漏掉这一次会让跨侧取数账本从**第一击**起整体错位（C# 每次有目标的攻击都多 1 抽）。
///
/// 其余 `AttackSuccess` 取数（`:221` 麻痹 `Random(AntiPoison + AttackPosionRate)`、
/// `:254/:261` 技能熟练度 `Random(3)+1`、`TrainCurrentSkill` 的 `Random(3)+1`）都依赖运行态
/// （`Paralysis` / `SuckupEnemyHealthRate` / `MagicArr`），本骨架未建模 ⇒ **登记**（见模块头），
/// 不做静默降级。
///
/// 返回值 = `nWeaponDamage`（本骨架无武器耐久面，值不使用；只保取数次数与顺序）。
#[must_use]
pub fn attack_success_weapon_damage(rng: &mut impl RandomSource) -> u16 {
    (rng.random_below(5) + 2) as u16
}

/// `PlayObject.AttackDir(targetObject, wHitMode, nDir)`（`PlayObject.Attack.cs:25`）的**玩家版**
/// 完整取数口径：`GetAttackPowerHit(GetBaseAttackPoewr(), ...)` → `AttackDir(..., nPower)`（= `_Attack`）
/// → `SendAttackMsg`（无取数）→ `AttackSuccess`。
///
/// 与 [`attack_with`] 的区别**只在末尾**多一次 [`attack_success_weapon_damage`]（`AttackSuccess` 的
/// `Random(5)`；`Race == Play` 攻方有目标时必有）。
///
/// **不能**把它并进 [`attack_with`]：`combat-probe` 的真身向量是直接调 `GetBaseAttackPoewr` +
/// `_Attack`（**不含** `AttackSuccess`）产出的，[`attack_with`] 必须与那份向量逐字段对得上；
/// 这里按 oracle 的结构把「攻击入口」补全。
///
/// 按攻方 `Race` 自动选 `AttackPowerMode`（oracle 的虚调用分派，见 [`AttackPowerMode::for_attacker`]）；
/// `AttackSuccess` 只在玩家攻方（`Race == Play`）追加——oracle 里它是 `PlayObject` 的私有方法，
/// 怪物/召唤物的 `BaseObject` 攻击路径不调它。
pub fn attack_dir(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
) -> AttackOutcome {
    let mode = AttackPowerMode::for_attacker(attacker);
    let outcome = attack_with(rng, attacker, target, last_hitter, config, mode);
    if matches!(mode, AttackPowerMode::Player { .. }) {
        // `AttackSuccess(..., targetObject)`：目标非空 ⇒ 抽 `Random(5)`（`PlayObject.Attack.cs:225`）。
        let _ = attack_success_weapon_damage(rng);
    }
    outcome
}

/// 无目标挥砍（`PlayObject.AttackDir(targetObject = null)`）的**唯一**取数：攻击力。
///
/// oracle 在取目标之前先算 `GetBaseAttackPoewr()`（`PlayObject.Attack.cs:73`），所以即使
/// 前方格无目标也照抽（玩家 `Luck <= 0` 时 2 抽）。之后 `_Attack(null)` 与
/// `AttackSuccess(…, null)` 都立刻返回 ⇒ 无其它取数。供 [`crate::world::World::attack_swing`] 与
/// 单测（`CountingRng`）共用。
pub fn swing_power(rng: &mut impl RandomSource, dc: i32, mode: AttackPowerMode) -> i32 {
    match mode {
        AttackPowerMode::Base => get_base_attack_power(rng, dc),
        AttackPowerMode::Player {
            luck,
            power_rate,
            bo_power_item,
            power_item,
        } => get_base_attack_power_player(rng, dc, luck, power_rate, bo_power_item, power_item),
    }
}

/// 战斗相关配置（默认值 = `GameSvrConf` 初值；现网 Server.conf 未改动时即为默认）。
#[derive(Clone, Copy, Debug)]
pub struct CombatConfig {
    pub warr_mon: i32,
    pub wizard_mon: i32,
    pub taos_mon: i32,
    pub mon_hum: i32,
    pub high_level_kill_mon_fix_exp: bool,
    pub kill_mon_exp_multiple: i32,
    pub mn_kill_mon_exp_multiple: i32,
    pub kill_mon_exp_rate: i32,
}

impl Default for CombatConfig {
    fn default() -> Self {
        CombatConfig {
            warr_mon: 10,
            wizard_mon: 10,
            taos_mon: 10,
            mon_hum: 10,
            high_level_kill_mon_fix_exp: true, // 现网 Exps.conf: HighLevelKillMonFixExp=1
            kill_mon_exp_multiple: 1,
            mn_kill_mon_exp_multiple: 1,
            kill_mon_exp_rate: 100,
        }
    }
}

/// 掉落预算里的一件物品（`mon.ItemList` 的元素；`crates/formula` 的 `UserItem` 别名）。
pub type DropItem = UserItem;

/// 在**怪物生成时**预算掉落（`WorldServer.MonGen.cs:440` 的时机）——RNG 在这里消耗。
///
/// 返回 `(gold, items)`：gold 对应 `mon.Gold`，items 对应 `mon.ItemList`。
pub fn generate_drops<C: ItemCatalog>(
    rng: &mut impl RandomSource,
    drop_list: &[mir2_data::models::MonsterDropItem],
    catalog: &C,
    mon_random_add_value: i32,
    gold_name: &str,
    item_number: &mut ItemNumberCounter,
) -> (i32, Vec<UserItem>) {
    let out = mon_get_random_items(
        drop_list,
        catalog,
        rng,
        mon_random_add_value,
        gold_name,
        item_number,
    );
    (out.gold, out.items)
}

/// 死亡时的经验结算（`BaseObject.cs:2429 KillFunc` → `PlayObject.cs:1632 KillTargetTrigger` +
/// `PlayObject.Base.cs:129 WinExp` + `:149 GetExp` 的子集）。
///
/// **调用口径（注意复刻 oracle 的原样调用）**：`KillTargetTrigger` 里是
/// `CalcGetExp(WAbil.Level /*玩家自己的等级*/, fightExp)`，而 `CalcGetExp` 内部又拿
/// `WAbil.Level` 与入参 `nLevel` 比——入参就是自己等级 ⇒ `L < L + 10` 恒真 ⇒
/// **击杀经验不按等级差衰减**（哪怕 `HighLevelKillMonFixExp = false`）。
/// 所以这里 `n_level` 传的是**击杀者自己的等级**，不是怪物等级（照抄 oracle，包括这个无效分支）。
///
/// 返回 `(获得经验, 是否升级, 升级后等级)`。
pub fn award_kill_exp(
    killer: &mut Entity,
    mon_exp: i32,
    need_exps: &[i32],
    config: &CombatConfig,
) -> (i32, bool, u8) {
    // KillTargetTrigger: monsterExp = CalcGetExp(WAbil.Level, fightExp)
    let monster_exp = calc_get_exp(
        i32::from(killer.level),
        i32::from(killer.level), // 入参即自己等级（oracle 原样）
        mon_exp,
        config.high_level_kill_mon_fix_exp,
    );
    // WinExp：等级超过 LimitExpLevel 时给固定值（本骨架 LimitExpLevel=1000，实际不触发）
    let mut dw_exp = monster_exp;
    if dw_exp > 0 {
        dw_exp *= config.kill_mon_exp_multiple;
        dw_exp *= config.mn_kill_mon_exp_multiple;
        dw_exp = hutil::round(f64::from(config.kill_mon_exp_rate) / 100.0 * f64::from(dw_exp));
    }
    // GetExp：加经验；`if (Abil.Exp >= Abil.MaxExp)` 单步升级（C# 就是 if 而非 while）
    killer.exp += dw_exp;
    let mut leveled = false;
    if killer.exp >= killer.max_exp {
        killer.exp -= killer.max_exp;
        if killer.level < 255 {
            killer.level += 1;
            leveled = true;
        }
        killer.max_exp = get_level_exp(need_exps, 255, i32::from(killer.level));
    }
    (dw_exp, leveled, killer.level)
}

/// 生成怪物所需的行数据（`monsters` 表子集 + 位置）。
#[derive(Clone, Copy, Debug)]
pub struct MonsterRow<'a> {
    pub id: i32,
    pub name: &'a str,
    pub x: i32,
    pub y: i32,
    pub level: u8,
    pub exp: i32,
    pub hp: u16,
    pub dc: u16,
    pub ac: u16,
    pub undead: u8,
    /// `HitPoint`（准确，命中判定的攻方一侧）——`BaseObject` 构造默认 5（`BaseObject.cs:386`）
    pub hit_point: u8,
    /// `SpeedPoint`（敏捷，命中判定的守方一侧）——`BaseObject` 构造默认 15（`BaseObject.cs:387`）
    pub speed_point: u8,
}

/// 由行数据构造怪物实体（数值语义与 `GameSvr/DB/MySqlDB.cs::LoadMonsterDB` 的产物一致）。
pub fn monster_from_row(row: MonsterRow<'_>) -> Entity {
    let mut e = Entity::new(row.id, row.name, row.x, row.y);
    e.race = 80; // ActorRace.Monster
    e.level = row.level;
    e.mon_exp = row.exp;
    e.hp = row.hp;
    e.max_hp = row.hp;
    e.dc = i32::from(row.dc);
    e.ac = i32::from(row.ac);
    e.life_attrib = row.undead;
    e.hit_point = row.hit_point;
    e.speed_point = row.speed_point;
    e
}

/// 便于测试/对拍：把掉落物品落到地上的位置（`BaseObject.GetDropPosition` 的扫描顺序子集）。
///
/// C# 的扫描是 `for i in 0..nRange { for ii in -i..=i { for iii in -i..=i { (orgX+iii+1, orgY+ii+1) } } }`，
/// 选第一个"格内物品数 == 0"的格子；若都不空则退回"物品数最少"的格子（`n24/n28/n2C` 分支）。
/// 本骨架只实现前者（地图够空时与 C# 同解），后者留待 `Envir.ChFlag` 语义明确后补。
pub fn drop_position(
    is_free: impl Fn(i32, i32) -> bool,
    org_x: i32,
    org_y: i32,
    range: i32,
) -> Option<(i32, i32)> {
    for i in 0..range {
        for ii in -i..=i {
            for iii in -i..=i {
                let px = org_x + iii + 1;
                let py = org_y + ii + 1;
                if is_free(px, py) {
                    return Some((px, py));
                }
            }
        }
    }
    None
}

/// 物品目录（`ItemSystem` 子集）：掉落生成时需要按名查 `StdItem`。
pub use mir2_formula::drop::VecItemCatalog as ItemCatalogImpl;

/// 便捷：把 `Vec<StdItem>` 包成目录。
pub fn catalog(items: &[StdItem]) -> ItemCatalogImpl<'_> {
    ItemCatalogImpl { items }
}
