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

/// `BaseObject.Animal`（`src/M2Server/Actor/BaseObject.cs:178`，构造初值 `false`）的**构造期**等价物。
///
/// oracle 只在少数怪物构造路径上把它置 `true`（按 race 分支；其余 ctor 显式写 `false` 与初值同）：
/// - 对象初始化器 `WorldServer.MonGen.cs:518-553`：`AnimalChicken(51)` / `AnimalDeer(52)` / `AnimalWolf(53)`；
/// - 派生类构造器：`SpitSpider(82)`（`Monster/Monsters/SpitSpider.cs:15`）、`Scorpion(84)`（`Scorpion.cs:7`）、
///   `StickMonster(85)`（`StickMonster.cs:23`）、`GasAttackMonster(90)`（`GasAttackMonster.cs:12`）。
///
/// `BaseObject.ReAliveEx` 的 `case 54: Animal = true`（`BaseObject.cs:2601-2603`）只在**复活**路径生效；
/// 本骨架未建模 `CanReAlive`/复活（⑨-3 登记）⇒ race 54 **不**计入（登记在案，勿按常识补）。
/// `MonsterObject.Animal` 只被 `DoSpell` 的 `SKILL_TAMMING` 支读（`MagicManager.cs:395`）。
#[must_use]
pub fn race_is_animal(race: u8) -> bool {
    matches!(race, 51 | 52 | 53 | 82 | 84 | 85 | 90)
}

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
/// `MyGuild`/`MasterList`（行会/师徒）。无组队注册表 ⇒ 等价于传空成员表（见
/// [`is_attack_target_player_grouped`]）。
pub fn is_attack_target_player(attacker: &Entity, target: &Entity) -> bool {
    is_attack_target_player_grouped(attacker, target, &[])
}

/// 带组队注册表的 [`is_attack_target_player`]（M3⑦-实现 4）。
///
/// `attacker_group_members` = **攻方所在组团长**的成员表（`attacker.GroupOwner == 0` 时传空表）；
/// oracle `PlayObject.IsGroupMember(target)`（`PlayObject.cs:5158-5175`）：`GroupOwner == 0` ⇒
/// `false`，否则遍历团长 `GroupMembers` 逐个引用比对。`HAM_GROUP` 支（`PlayObject.Attack.cs:667-673`）：
/// 先按 `Race` 置 `result`，再 `if (target.Race == ActorRace.Play) if (IsGroupMember(target)) result = false;`
/// ⇒ **同组玩家被剔除**（不同组/无组照旧可打）。
pub fn is_attack_target_player_grouped(
    attacker: &Entity,
    target: &Entity,
    attacker_group_members: &[i32],
) -> bool {
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
            // `if (targetObject.Race == ActorRace.Play) { if (IsGroupMember(targetObject)) result = false; }`
            // （`PlayObject.Attack.cs:671-677`）——`IsGroupMember` 的第一步是 `GroupOwner == 0 ⇒ false`。
            if target.race == ACTOR_RACE_PLAY
                && attacker.group_owner != 0
                && attacker_group_members.contains(&target.id)
            {
                result = false;
            }
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

/// `ItemLocation.Ringl` / `Ringr`（`src/OpenMir2/Enums/ItemLocation.cs`）——戒指两格
/// （下标与 `bins/game-svr/src/ability.rs` 的 `SLOT_RING_L`/`SLOT_RING_R` 同源）。
pub const ITEM_LOCATION_RING_L: usize = 7;
pub const ITEM_LOCATION_RING_R: usize = 8;

/// `ItemShapeConst.RING_MAKESTONE_ITEM`（`src/M2Server/Actor/ActorConst.cs:60`）——麻痹戒指的 Shape。
/// oracle 是 `byte` ⇒ 与 `StdItem.Shape`（`crates/data/src/models.rs:13`，`u8`）同型。
pub const RING_MAKESTONE_ITEM: u8 = 113;

/// 攻方是否戴了麻痹戒指（`PlayObject.cs:2230-2239`：`Case ItemLocation.Ringl/Ringr` 的
/// `switch (stdItem.Shape)` 命中 `RING_MAKESTONE_ITEM` ⇒ `Paralysis = true`）。
///
/// 入参 = 两枚戒指的 `StdItem.Shape`（缺槽 / `Index == 0` / 模板缺 ⇒ `None`）；任一为
/// `RING_MAKESTONE_ITEM` 即 `true`。宿主 `bins/game-svr` 从会话 `UseItems[Ringl/Ringr]` 查模板得 Shape
/// （与 [`crate::world::World::set_attack_poison`] 同址灌入）。
#[must_use]
pub fn paralysis_from_ring_shapes(shapes: impl IntoIterator<Item = Option<u8>>) -> bool {
    shapes
        .into_iter()
        .flatten()
        .any(|s| s == RING_MAKESTONE_ITEM)
}

/// `PlayObject.IsProperIsFriend`（`PlayObject.cs:4202-4296`）——玩家对玩家的「友军」判定。
///
/// 逐 `AttatckMode`（`Entity::attack_mode`）分派；`HAM_MASTER` 的 `MasterList`（师徒）与
/// `HAM_GUILD` 的 `MyGuild`/`GuildWarArea`（行会）本骨架未建模 ⇒ 只保留 `self == cret`
/// 支（登记，与 [`is_attack_target_player_grouped`] 的行会/师徒登记同源）。
/// `is_group_member` 由调用方按 oracle `IsGroupMember` 解析（`GroupOwner != 0` 且成员表含 `cret`）。
///
/// **登记 F2（订正版，独立复验 2026-10-17，`openmir2-buff-producer-wiring`，未实现）**：
/// 原文把偏差说成「返回**上界**（比 oracle 宽松，A-5 的 `MagMakeDefenceArea` 可能**多加点**）」
/// ——**方向说反了**，本处更正。逐条核实：
///
/// ① **`PlayObject.Attack.cs:755-817` 那三段是玩家目标路径上的死代码**（省略**不产生偏差**）：
///    该 `IsProperFriend` override 先 `bool result = IsProperIsFriend(targetObject)`（`:759`），
///    其 `:757` 判 `targetObject.Race == ActorRace.Play`，紧接 **`:760 if (targetObject.Race <
///    ActorRace.Animal) return result;`**；而 `ActorRace.Play = 0`（`src/OpenMir2/Enums/Race.cs:8`）、
///    `Animal = 50`（同文件 `:24`）⇒ `0 < 50` **恒成立** ⇒ `:764-817` 的
///    **`target.Master` 链 / PK 值保护 / `MapMoveTick` 3 秒窗**在**玩家目标**路径上永不执行。
///    ⇒ 原登记的「三段未建模 ⇒ 上界」**不成立**：省略它们**不产生任何偏差**（无论方向）。
///
/// ② **真实偏差在另一侧、方向相反（收窄 ⇒ 漏加点）**：本函数对
///    `HAM_MASTER`（= `AttackMode.HAM_MASTER`，3）与 `HAM_GUILD`（5）只保留 `self == cret`
///    （见 `match caster.attack_mode`），而 oracle 的 `IsProperIsFriend`（`PlayObject.cs:4202-4296`）
///    在这两支还收**名单**：
///    · `HAM_MASTER` 走 `MasterList`（师徒）——`PlayObject.cs:4221-4248`（`IsMaster` / 对方 `IsMaster`
///      两向遍历 `MasterList`）；
///    · `HAM_GUILD` 走 `MyGuild.IsMember(cret.ChrName)` 与 `GuildWarArea && MyGuild.IsAllyGuild(…)`
///      ——`PlayObject.cs:4259-4278`。
///    ⇒ 本仓这两支**比 oracle 窄** ⇒ 在该模式下 14/15（`MagMakeDefenceArea` 的友军门）会
///    **漏加**友军 buff（**漏加点**，不是多加点）。
///
/// ③ **可达性 = 生产可达，不是理论面**：`attack_mode` 来自线上报文
///    （`bins/game-svr/src/lib.rs` `attack_mode: d.attatck_mode`）⇒ 模式 3/5 由客户端上报即可进入。
///    ⇒ 该缺口是**真实可触发的收窄**，登记为**未实现**（保守方向登记 = 收窄 = 漏加点）。
#[must_use]
pub fn is_proper_is_friend(caster: &Entity, cret: &Entity, is_group_member: bool) -> bool {
    if cret.race != ACTOR_RACE_PLAY {
        return false;
    }
    match caster.attack_mode {
        ATTACK_MODE_HAM_ALL | ATTACK_MODE_HAM_PEACE => true,
        ATTACK_MODE_HAM_DEAR => caster.id == cret.id || caster.dear_human == Some(cret.id),
        ATTACK_MODE_HAM_MASTER => caster.id == cret.id,
        ATTACK_MODE_HAM_GROUP => caster.id == cret.id || is_group_member,
        ATTACK_MODE_HAM_GUILD => caster.id == cret.id,
        ATTACK_MODE_HAM_PKATTACK => {
            if caster.id == cret.id {
                true
            } else {
                let caster_pvp = (caster.pk_point / 100) as u8;
                let target_pvp = (cret.pk_point / 100) as u8;
                if caster_pvp >= 2 {
                    target_pvp < 2
                } else {
                    target_pvp >= 2
                }
            }
        }
        _ => false,
    }
}

/// `BaseObject.IsProperFriend`（`BaseObject.Base.cs:588-606`）——**基版**（非玩家攻方，或玩家版回落）。
#[must_use]
pub fn base_is_proper_friend(caster: &Entity, target: &Entity) -> bool {
    if caster.race >= ACTOR_RACE_ANIMAL {
        let mut result = target.race >= ACTOR_RACE_ANIMAL;
        if target.master.is_some() {
            result = false;
        }
        return result;
    }
    false
}

/// `IsProperFriend` 虚调用分派（`BaseObject.Base.cs:588` + `PlayObject.Attack.cs:755`）。
///
/// 玩家施法者：目标是玩家 ⇒ [`is_proper_is_friend`]；否则回落基版（玩家施法者的基版恒 `false`
/// —— `Race(0) < Animal` ⇒ 只保护玩家友军）。`is_group_member` 仅在目标是玩家且 `HAM_GROUP` 时被消费。
#[must_use]
pub fn is_proper_friend(caster: &Entity, target: &Entity, is_group_member: bool) -> bool {
    if caster.race == ACTOR_RACE_PLAY && target.race == ACTOR_RACE_PLAY {
        return is_proper_is_friend(caster, target, is_group_member);
    }
    base_is_proper_friend(caster, target)
}

/// `PlayObject.IsProperTarget`（`PlayObject.cs:1929`）——**玩家攻方**的判定链。
/// 带组队注册表的 [`is_proper_target_play`]（M3⑦-实现 4）。
fn is_proper_target_play_grouped(
    attacker: &Entity,
    target: &Entity,
    attacker_group_members: &[i32],
) -> bool {
    let mut result = is_attack_target_player_grouped(attacker, target, attacker_group_members);
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
    is_proper_target_grouped(attacker, target, &[])
}

/// 带组队注册表的 [`is_proper_target`]（M3⑦-实现 4）：`attacker_group_members` 供
/// `HAM_GROUP` 的 `IsGroupMember` 剔除用（其它攻击模式忽略它）。
pub fn is_proper_target_grouped(
    attacker: &Entity,
    target: &Entity,
    attacker_group_members: &[i32],
) -> bool {
    if attacker.id == target.id {
        return false;
    }
    if target.death || target.ghost || target.invisible {
        return false;
    }
    if attacker.race == ACTOR_RACE_PLAY {
        return is_proper_target_play_grouped(attacker, target, attacker_group_members);
    }
    is_attack_target_base(attacker, target)
}

/// **P3-B5**：oracle `PlayObject.IsProperTarget`（`PlayObject.cs:1924-1965`）**不读 `Invisible`**
/// ——[`is_proper_target_grouped`] 顶部折入的 `target.invisible` 是既有攻击调用点（`_Attack` 的
/// `SearchTarget` 下游）的口径，不是本函数。
///
/// 9/10 的 `MagPassThroughMagic` 逐格对象（`GetMovingObject(...,true)` 已过滤 Ghost/Death）与
/// 22 的事件格对象（`GetBaseObjects(...,true)` 已过滤 Death）都走本函数。
/// **登记 T-B5-1**：与折入版的唯一差异就是「隐身目标是否可被范围魔法命中」；oracle 可命中。
#[must_use]
pub fn is_proper_target_grouped_direct(
    attacker: &Entity,
    target: &Entity,
    attacker_group_members: &[i32],
) -> bool {
    if attacker.id == target.id {
        return false;
    }
    // **不折入 `death/ghost/invisible`**：oracle `PlayObject.IsProperTarget`（`PlayObject.cs:1924`）
    // 只做 `ActorId` 相等门 + `IsAttackTarget`/`IsProtectTarget`；Death/Ghost 由枚举面
    // （`GetMovingObject(...,true)` 的 `!Ghost && !Death`、`GetBaseObjects(...,true)` 的 `!Death`）负责。
    // 折进来会让「枚举面漏掉 Death 过滤」的改坏不可见（B5-9-2 牙）。
    if attacker.race == ACTOR_RACE_PLAY {
        return is_proper_target_play_grouped(attacker, target, attacker_group_members);
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
///      才破零；破零后 `Index = 0`（`:2982`/`:3014`/`:3016`）+ `Dura = 0`（`:2985`/`:3017`）。
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

/// `BaseObject.GetMagStruckDamage(IActor baseObject, int nDamage)`
/// （`src/M2Server/Actor/BaseObject.Base.cs:812-821`）——`RM_MAGSTRUCK` 交付段的唯一骰。
///
/// 逐行：
/// ```text
/// :814  n14 = LoByte(WAbil.MAC) + Random(|HiByte(WAbil.MAC) - LoByte(WAbil.MAC)| + 1)
/// :815  nDamage = (ushort)MAX(0, nDamage - n14)
/// :816  if (LifeAttrib == LA_UNDEAD && baseObject != null) nDamage += AddAbil.UndeadPower
/// ```
///
/// **`baseObject` 条件**：`RM_MAGSTRUCK` 的处理器传的是 `null`
/// （`BaseObject.Base.cs:624` 的 `GetMagStruckDamage(null, processMsg.nParam1)`）⇒ undead 加项
/// **不生效**。`Some(power)` 只给将来会传真实攻方的调用面（`MonDelHptoExp` 支等）。
///
/// **F4（`attacktarget-overrides` 复验登记，不在该片改）**：oracle 的 undead 加项读的是
/// `this.AddAbil.UndeadPower`（**接收者**），且门是 `LifeAttrib == LA_UNDEAD`；本函数的
/// `undead_power` 由调用面自选，`monai` 的攻击路径（`spit_attack`/`elf_warrior_spit_attack`
/// 等）现传**攻方**的 `undead_power`，且函数本身未判 `LifeAttrib`。生产怪物
/// `undead_power` 恒 0 ⇒ 现网无差异；修正需同时（a）改取接收者、（b）补 `LA_UNDEAD` 门，
/// 单开后续卡，本片不动。
///
/// `MAC` 是 `WAbil.MAC` 的 `ushort`；`Random` 参数用**绝对值+1**（Hi<Lo 时不 panic）。
/// 返回值按 oracle 的 `int`（`BaseObject.GetMagStruckDamage` 返回 `int`，不是 `ushort`）。
#[must_use]
pub fn get_mag_struck_damage(
    rng: &mut impl RandomSource,
    mac: u16,
    n_power: i32,
    undead_power: Option<i32>,
) -> i32 {
    let lo = i32::from(lo_byte(i32::from(mac)));
    let hi = i32::from(hi_byte(i32::from(mac)));
    let n14 = lo + rng.random_below((hi - lo).abs() + 1);
    let mut damage = hutil::max_i32(0, n_power - n14);
    if let Some(power) = undead_power {
        damage += power;
    }
    damage
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
    if let Some(hitter) =
        last_hitter.filter(|h| target.race >= ACTOR_RACE_ANIMAL && h.race == ACTOR_RACE_PLAY)
    {
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
    /// **卡 `openmir2-rm-refmessage-delay-layer`**：`_Attack` 传给
    /// `SendStruckDelayMsg(RM_REFMESSAGE, nPower, …)`（`BaseObject.Attack.cs:57`）的
    /// `nPower` —— `GetHitStruckDamage` **之后**、`StruckDamage` 倍率**之前**的值。
    ///
    /// `damage` 是倍率之后的最终掉血；两者在默认配置（倍率 10/10）相等，但非默认
    /// `WarrMon/WizardMon/TaosMon/MonHum` 下必须分开（oracle 的延时/自投消息都带 `nPower`）。
    pub struck_power: u16,
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
///
/// 无组队注册表 ⇒ 等价于 [`attack_with_grouped`] 传空成员表。
pub fn attack_with(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
    power_mode: AttackPowerMode,
) -> AttackOutcome {
    attack_with_grouped(rng, attacker, target, last_hitter, config, power_mode, &[])
}

/// [`attack_with`] 的组队感知版本（M3⑦-实现 4）：`attacker_group_members` = 攻方所在组团长的成员表，
/// 供 `HAM_GROUP` 的 `IsGroupMember` 剔除用（见 [`is_proper_target_grouped`]）。
pub fn attack_with_grouped(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
    power_mode: AttackPowerMode,
    attacker_group_members: &[i32],
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

    // `_Attack`（`BaseObject.Attack.cs:37`）调**虚函数** `IsProperTarget`：怪物 = 基版
    // `IsAttackTarget`（`BaseObject.Base.cs:470-473`），玩家 = `PlayObject` override
    // （`PlayObject.cs:1924-1962`）——**都不查 Death/Ghost/Invisible**（审计 G09）。
    // 用直通版以恢复 `RandomByte(target.SpeedPoint)` 与后续 `GetHitStruckDamage` 抽数；
    // 折入版 [`is_proper_target_grouped`] 仅供枚举/显示型调用点使用。
    if is_proper_target_grouped_direct(attacker, target, attacker_group_members) {
        // `if (targetObject.HitPoint > 0) { if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0; }`
        if target.hit_point > 0 && attacker.hit_point < rng.random_byte(target.speed_point) {
            n_power = 0;
        } else {
            hit = true;
        }
    } else {
        n_power = 0;
    }

    let (damage, struck_power, killed) = if n_power > 0 {
        apply_struck_damage_detail(rng, attacker, target, n_power, last_hitter, config)
    } else {
        (0, 0, false)
    };
    // 注意：`_Attack` 里**没有** `LastHiter` 赋值（C# 原文如此）。命中者的登记走
    // `RM_STRUCK` 消息路径，由调用方在攻击结算之后调用 `Entity::set_last_hiter`。
    AttackOutcome {
        power,
        hit,
        damage,
        struck_power,
        killed,
        target_hp_after: target.hp,
    }
}

/// `StruckDamage` + `DamageHealth` 的伤害落体（`attack_with_grouped` 与 MB-3 的
/// `HitMagAttackTarget`/`ArcherGuard.AttackTarger` 共用）。
///
/// **取数次序（oracle 同序，勿改）**：`GetHitStruckDamage`（AC 骰 1~2 抽）→ `StruckDamage`
/// 倍率 → `PlayObject.StruckDamage` 的耐久磨损（玩家目标：`Random(10)` + 逐格 `Random(8)`）。
/// 返回 `(damage, killed)`；`damage == 0` ⇒ 调用方不得 `SendStruckDelayMsg`。
pub fn apply_struck_damage(
    rng: &mut impl RandomSource,
    attacker: &Entity,
    target: &mut Entity,
    n_power: i32,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
) -> (u16, bool) {
    let (damage, _struck_power, killed) =
        apply_struck_damage_detail(rng, attacker, target, n_power, last_hitter, config);
    (damage, killed)
}

/// [`apply_struck_damage`] 的**三段**版本：额外返回 `GetHitStruckDamage` 之后、倍率之前的
/// `nPower`（= `_Attack` 传给 `SendStruckDelayMsg` 的值，见
/// [`AttackOutcome::struck_power`]）。取数次序与两段版逐字相同。
pub fn apply_struck_damage_detail(
    rng: &mut impl RandomSource,
    attacker: &Entity,
    target: &mut Entity,
    n_power: i32,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
) -> (u16, u16, bool) {
    let reduced = get_hit_struck_damage(rng, target, attacker, n_power);
    if reduced == 0 {
        return (0, 0, false);
    }
    let (damage, killed) =
        struck_damage_apply(rng, target, i32::from(reduced), last_hitter, config);
    (damage, reduced, killed)
}

/// [`apply_struck_damage`] 的「已过 `GetHitStruckDamage`」半边：MB-3 的
/// `GasAttackMonster.GasAttack`（`GetMagStruckDamage` 之后 `StruckDamage`）与
/// `ScultureKingMonster.Attack`（两段之和）需要在**不同的减伤入口**之后复用同一段
/// `StruckDamage`/`DamageHealth`。
///
/// `reduced` = 已减伤后的伤害（oracle 调用点里就是 `GetHitStruckDamage`/
/// `GetMagStruckDamage` 的返回值之和）；`reduced <= 0` ⇒ 调用方不得调用（oracle 有 `> 0` 门）。
pub fn struck_damage_apply(
    rng: &mut impl RandomSource,
    target: &mut Entity,
    reduced: i32,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
) -> (u16, bool) {
    if reduced <= 0 {
        return (0, false);
    }
    // `StruckDamage`：职业对怪倍率 / 主从对人倍率，然后 `DamageHealth`
    let per10 = struck_damage_multiplier(
        target,
        last_hitter,
        config.warr_mon,
        config.wizard_mon,
        config.taos_mon,
        config.mon_hum,
    );
    let final_damage = (reduced * per10 / 10).max(0) as u16;
    if i32::from(target.hp) - i32::from(final_damage) > 0 {
        target.hp -= final_damage;
    } else {
        target.hp = 0;
    }
    let killed = target.hp == 0;
    // `PlayObject.StruckDamage`（`PlayObject.cs:2954`）在 `base.StruckDamage` 之后的作用：
    //   1. `:2957` **无条件**抽一次 `Random(10)`（护甲耐久磨损点数 `nDam = Random(10)+5`）；
    //   2. `:2965-2996` 衣服位（`UseItems[ItemLocation.Dress=0]`）特判——**不抽数**，只扣耐久，
    //      但破零会先清 `Index`（`:2982`），使逐格循环跳过该格的 `Random(8)`；
    //   3. `:2998-3039` 逐格循环：`Index > 0`（已装备）的每格**各抽一次 `Random(8)`**。
    // 玩家目标走这个 override，怪物走 `BaseObject.StruckDamage`（无随机数）。
    // 漏掉任一条都会让同种子跨侧取数账本从第一次命中起整体错位。
    struck_damage_durability(rng, target);
    (final_damage, killed)
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
/// 其余 `AttackSuccess` 取数都依赖运行态（`Paralysis` / `SuckupEnemyHealthRate` / `MagicArr`）。
/// **登记订正 1（独立复验 2026-10-17）**——原文写「`:221` … 未建模」与「`:254/:261`」两处**已陈旧**
/// （`:221` 本片已建模；`:261` 行号有漂）。按现况如实登记（见模块头），不做静默降级：
/// - **`:221` 麻痹门** `Random(targetObject.AntiPoison + Config.AttackPosionRate)`：**本片已建模**——
///   [`attack_dir_grouped_poison`] 在 `poison = Some(..)` 且 `!UnParalysis && Paralysis` 时插一次抽取
///   并回 `poisoned`；**`poison = None` ⇒ 不抽**。故 [`attack_dir_grouped`]（恒传 `None`）与
///   `attack_with` / `World::attack_once` 等**不传运行态的入口仍不抽**（与旧口径逐取数等价）。
/// - **`:246` / `:254` 技能熟练度** `CheckSkillProficiency(M2Share.RandomNumber.Random(3) + 1, …)`
///   （`SKILL_ILKWANG` 支 `:246`、`SKILL_YEDO` 支 `:254`；原注的 `:254/:261` 订正为 `:246`/`:254`）；
/// - **`:364` / `:375` / `:388` / `:1207`** 亦有 `TrainSkill(MagicArr[…], Random(3) + 1)`（`wHitMode`
///   尾与另一处命中尾）。
///   以上技能熟练度支都要求对应 `MagicArr` 槽非 null（本骨架无 `MagicArr` / `UserMagic[]` 面）
///   ⇒ **未建模**，维持登记。
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
    attack_dir_grouped(rng, attacker, target, last_hitter, config, &[])
}

/// [`attack_dir`] 的组队感知版本（M3⑦-实现 4）：`attacker_group_members` 供 `HAM_GROUP` 的
/// `IsGroupMember` 剔除用（`PlayObject.Attack.cs:667-673`）；其余口径与 [`attack_dir`] 相同。
pub fn attack_dir_grouped(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
    attacker_group_members: &[i32],
) -> AttackOutcome {
    attack_dir_grouped_poison(
        rng,
        attacker,
        target,
        last_hitter,
        config,
        attacker_group_members,
        None,
    )
    .0
}

/// `PlayObject.AttackSuccess`（`PlayObject.Attack.cs:215-225`）的**攻击命中中毒门**运行态：
/// `:221` 的 `!targetObject.UnParalysis && Paralysis` 两项（`target.AntiPoison` 与
/// `Config.AttackPosionRate` 由 [`attack_dir_grouped_poison`] 的其余入参给出）。
///
/// oracle 里 `Paralysis` 是 `PlayObject` 的属性（`PlayObject.cs:474`；`PlayObject.cs:2238` 由
/// `RING_MAKESTONE_ITEM` 戒指置位），`UnParalysis`/`AntiPoison` 是 `BaseObject` 的属性
/// （`BaseObject.cs:85/109`）。本片把这组**攻/守双方**的写实放进
/// [`crate::world::World::attack_poison`]（与 `World::buffs` 同为「按 actor id 挂世界侧」的表），
/// 由宿主 `bins/game-svr` 在攻击派发前灌入（见 [`crate::world::World::set_attack_poison`]）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttackPoisonState {
    /// `PlayObject.Paralysis`（攻方麻痹戒指）。
    pub paralysis: bool,
    /// `BaseObject.UnParalysis`（守方免疫石化）。
    pub un_paralysis: bool,
    /// `BaseObject.AntiPoison`（守方抗毒）。
    pub anti_poison: u16,
}

/// [`attack_dir_grouped`] 的**中毒门感知**版本（本片 `buff-producer-wiring` 新增）。
///
/// 与 [`attack_dir_grouped`] 的差别**只在末尾的取数插入位置**：oracle `AttackSuccess` 里
/// `:221-224` 的麻痹中毒门（`Random(target.AntiPoison + Config.AttackPosionRate) == 0` ⇒
/// `targetObject.MakePosion(PoisonState.STONE, Config.AttackPosionTime, 0)`）**先于** `:225` 的
/// `Random(5)`。本函数按此序插入一次抽取并把「是否中毒」回给调用方（用于 `World::make_posion`）。
///
/// **短路口径照 oracle**：`!targetObject.UnParalysis && Paralysis && (Random(...) == 0)`——
/// `UnParalysis` 或 `!Paralysis` 时**不抽**（C# 的 `&&` 短路）；只有两项都过才抽一次。`poison`
/// 传 `None`（buff 关 / 无该 actor 的运行态）⇒ 与 [`attack_dir_grouped`] **逐取数等价**（不抽）。
///
/// 只对**玩家攻方**（`AttackPowerMode::Player`）生效——`AttackSuccess` 是 `PlayObject` 的私有方法，
/// 怪物/召唤物的 `BaseObject` 攻击路径不调它（见 [`attack_dir`] 注释）。
pub fn attack_dir_grouped_poison(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    last_hitter: Option<LastHitter>,
    config: &CombatConfig,
    attacker_group_members: &[i32],
    poison: Option<AttackPoisonState>,
) -> (AttackOutcome, bool) {
    let mode = AttackPowerMode::for_attacker(attacker);
    let outcome = attack_with_grouped(
        rng,
        attacker,
        target,
        last_hitter,
        config,
        mode,
        attacker_group_members,
    );
    let mut poisoned = false;
    if matches!(mode, AttackPowerMode::Player { .. }) {
        // `if (!targetObject.UnParalysis && Paralysis && (Random(targetObject.AntiPoison +
        //  SystemShare.Config.AttackPosionRate) == 0))`（`PlayObject.Attack.cs:221`）——
        // `&&` 短路：两项门都过才抽。
        if let Some(p) = poison {
            if !p.un_paralysis
                && p.paralysis
                && rng.random_below(i32::from(p.anti_poison) + config.attack_poison_rate) == 0
            {
                poisoned = true;
            }
        }
        // `AttackSuccess(..., targetObject)`：目标非空 ⇒ 抽 `Random(5)`（`PlayObject.Attack.cs:225`）。
        let _ = attack_success_weapon_damage(rng);
    }
    (outcome, poisoned)
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
    /// `SystemShare.Config.AttackPosionRate`（`GameSvrConf.cs:796/1723` 缺省 5；
    /// `ServerConf.cs:501` 读 `[Setup] AttackPosionRate`）——玩家攻击命中中毒门的抽样上界
    /// （`PlayObject.Attack.cs:221` 的 `Random(target.AntiPoison + AttackPosionRate) == 0`）。
    pub attack_poison_rate: i32,
    /// `SystemShare.Config.AttackPosionTime`（`GameSvrConf.cs:800/1724` 缺省 5）——命中中毒时
    /// 石化毒的 `nTime`（`PlayObject.Attack.cs:223` 的 `MakePosion(PoisonState.STONE, …, 0)`）。
    pub attack_poison_time: i32,
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
            attack_poison_rate: 5, // GameSvrConf.cs:1723
            attack_poison_time: 5, // GameSvrConf.cs:1724
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
/// 返回 `(获得经验, 是否进入升级块, 升级后等级)`。
///
/// 第 2 项口径：oracle `GetExp`（`PlayObject.Base.cs:149-165`）里 `Abil.Level++` 受
/// `Abil.Level < MAXUPLEVEL(255)` 门控，但**只要** `Abil.Exp >= Abil.MaxExp` 就进该块，
/// 块内 `HasLevelUp`/`AddBodyLuck(100)`/`IncHealthSpell(2000,2000)` **无条件**执行 ⇒ 宿主用
/// 这一位决定是否出 `SM_LEVELUP(45)`/`SM_ABILITY(52)`/`SM_SUBABILITY(752)`/`SM_HEALTHSPELLCHANGED(53)`。
pub fn award_kill_exp(
    killer: &mut Entity,
    mon_exp: i32,
    need_exps: &[i32],
    config: &CombatConfig,
    map_exp_rate: Option<i32>,
) -> (i32, bool, u8) {
    // KillTargetTrigger: monsterExp = CalcGetExp(WAbil.Level, fightExp)
    let monster_exp = calc_get_exp(
        i32::from(killer.level),
        i32::from(killer.level), // 入参即自己等级（oracle 原样）
        mon_exp,
        config.high_level_kill_mon_fix_exp,
    );
    win_exp(killer, monster_exp, need_exps, config, map_exp_rate)
}

/// `PlayObject.WinExp(int dwExp)`（`PlayObject.Base.cs:129-147`）+ `GetExp`（`:149-165`）
/// 的乘法 + 升级段（M3⑦-实现 4 从 [`award_kill_exp`] 拆出，供组队分配逐成员调用）。
///
/// 入参 `dw_exp` = **乘倍率之前**的值（oracle `GainExp` 传给每个成员的
/// `HUtil32.Round(dwExp/n)` 就是原值；`KillTargetTrigger` 传给 `GainExp` 的是
/// `CalcGetExp` 的结果）。返回 `(乘完倍率后的实际加值, 是否进升级块, 升级后等级)`。
pub fn win_exp(
    killer: &mut Entity,
    dw_exp: i32,
    need_exps: &[i32],
    config: &CombatConfig,
    map_exp_rate: Option<i32>,
) -> (i32, bool, u8) {
    // WinExp：等级超过 LimitExpLevel 时给固定值（本骨架 LimitExpLevel=1000，实际不触发）
    let mut dw_exp = dw_exp;
    if dw_exp > 0 {
        dw_exp *= config.kill_mon_exp_multiple;
        dw_exp *= config.mn_kill_mon_exp_multiple;
        // **PL-3 / P27**：`KillMonExpRate` 是 **per-player** 字段（`PlayObject.cs:720`），
        // 不再是全局 `CombatConfig` 字段——组队时每个成员各读自己的值（oracle `GainExp`
        // 对每个成员调各自的 `WinExp`）。`rate == 100` 时与旧公式逐字节相同。
        dw_exp = hutil::round(f64::from(killer.kill_mon_exp_rate) / 100.0 * f64::from(dw_exp));
        // `Envir.Flag.boEXPRATE` ⇒ 地图级经验倍率（`PlayObject.Base.cs:143-146`）。
        // `map_exp_rate = Some(rate)` 对应 `boEXPRATE==true` 时的 `Envir.Flag.ExpRate`。
        if let Some(rate) = map_exp_rate {
            dw_exp = hutil::round(f64::from(rate) / 100.0 * f64::from(dw_exp));
        }
    }
    get_exp_raw(killer, dw_exp, need_exps)
}

/// `PlayObject.GetExp(int dwExp)`（`PlayObject.Base.cs:149-166`）的**裸加经验**段。
///
/// **P31（`SETAUTOGETEXP`）的唯一入口**：oracle 的 `PlayObject.Message.cs:743` 是
/// `GetExp(AutoGetExpPoint)`——**不经** `WinExp`，所以这里**不得**乘 `KillMonExpMultiple` /
/// `MNKillMonExpMultiple` / `KillMonExpRate` / 地图 `ExpRate`，也**不得**过 `CalcGetExp`
/// 或组队均分（那些都在 [`win_exp`] / [`award_kill_exp`] 里）。逐行：
///
/// ```text
/// Abil.Exp += dwExp;
/// AddBodyLuck(dwExp * 0.002);            // 世界层不建模，宿主按 exp_awards 补
/// SendMsg(RM_WINEXP, 0, dwExp, 0, 0);    // 宿主按 exp_awards 出 44
/// if (Abil.Exp >= Abil.MaxExp) {
///     Abil.Exp -= Abil.MaxExp;
///     if (Abil.Level < MAXUPLEVEL) Abil.Level++;
///     HasLevelUp(Abil.Level - 1);
///     AddBodyLuck(100);                  // 宿主补
///     IncHealthSpell(2000, 2000);        // 宿主升级链补
/// }
/// ```
///
/// 返回 `(本次加值, 是否进入升级块, 升级后等级)`——`leveled` 是**进块**而非「等级真的 +1」
/// （255 级时 `Level++` 被门挡住，但块内 `HasLevelUp`/`IncHealthSpell` 仍无条件执行）。
pub fn get_exp_raw(entity: &mut Entity, dw_exp: i32, need_exps: &[i32]) -> (i32, bool, u8) {
    entity.exp += dw_exp;
    let leveled = entity.exp >= entity.max_exp;
    if leveled {
        entity.exp -= entity.max_exp;
        // oracle：`if (Abil.Level < MessageSettings.MAXUPLEVEL) Abil.Level++;`
        // （`MAXUPLEVEL = byte.MaxValue = 255`，`MessageSettings.cs:7`）⇒ u8 饱和自增。
        entity.level = entity.level.saturating_add(1);
        entity.max_exp = get_level_exp(need_exps, 255, i32::from(entity.level));
    }
    (dw_exp, leveled, entity.level)
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
                 // `Entity::new` 是**玩家** ctor 等价物（`PlayObject.cs:899-901` 覆盖 `SearchTime=1000`）；
                 // 怪物这里还原非玩家缺省占位值，真实值由生成路径的 ctor 抽/`seed_from_spawn` 落定。
    e.ai.search_time = crate::entity::BASE_ANIMAL_SEARCH_TIME;
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

// ---- 魔法/技能里的**攻击语义**取数（2026-10-17 从 `magic.rs` 等价搬入）--------------------
//
// 搬迁只移动文件位置：函数体 / 取数式 / 次序 / 门逐字不变；`crate::magic` 侧用
// `pub use crate::combat::{group_melee_damage, power_uenhancer};` 保持既有对外路径。
//
// 站点归组（`tests/parity/m3/rng-trace-probe/consumer_groups.tsv`）：这两个函数里的
// `random_below` **不是**近战攻击路径——C# 对侧分别在 `MagicManager.cs:623`（无极真气
// `Random(Level)`）与 `:957`（`MagGroupDeDing` 的 `Random(SpeedPoint)`），均不匹配
// `CSHARP_ATTACK_RE`。因此按**精确行**归 `magic_effect`，**不**加进 `RUST_ATTACK_RE`
// （加进去会让 `groups --no-attack` 把它们静默剔除，重现历史语义漂移）。
use crate::magic_effect::power_defence_area;
use mir2_data::models::MagicInfo;

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

/// 攻方攻击力口径（`GetAttackPower`）：`DoSpell` 的施法者是 `PlayObject`，
/// 走 `PlayObject.GetAttackPower` override（`PlayObject.cs:2909`）——**取数次数与 Base 版不同**
/// （`Luck == 0` 时多一次 `Random(10)`），因此不能拿 [`AttackPowerMode::Base`] 代替，
/// 否则当威力之后还有取数（如火墙的 `time`）时序列会错位。
pub(crate) fn atk_power(
    rng: &mut impl RandomSource,
    base: i32,
    range: i32,
    atk: AttackPowerMode,
) -> i32 {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};
    use mir2_shared::rng::{RandomNumber, RandomSource};

    /// `Animal` 构造期置真的 race 集合（`MonsterInfo`/各 ctor 的 oracle 事实）。
    #[test]
    fn animal_races_match_oracle_constructors() {
        for r in [51_u8, 52, 53, 82, 84, 85, 90] {
            assert!(race_is_animal(r), "race {r} 应为 Animal");
        }
        // 常见非动物怪：骷髅/神兽/钉耙猫/多钩猫/毒蜘蛛/半兽人等。
        for r in [80_u8, 81, 83, 101, 100, 10, 50, 0, 54] {
            assert!(!race_is_animal(r), "race {r} 不应为 Animal");
        }
    }

    // ================= 片 A-5（buff 生产者接线） =================

    /// 计数随机源：委托同种子 [`RandomNumber`]，记录每次 `random_below` 的入参（取数**次序 + 参数**）。
    struct CountingRng {
        inner: RandomNumber,
        below: Vec<i32>,
    }

    impl CountingRng {
        fn new(seed: i32) -> Self {
            Self {
                inner: RandomNumber::with_seed(seed),
                below: Vec::new(),
            }
        }
    }

    impl RandomSource for CountingRng {
        fn random(&mut self) -> i32 {
            self.inner.random()
        }
        fn random_below(&mut self, value: i32) -> i32 {
            self.below.push(value);
            self.inner.random_below(value)
        }
        fn random_range(&mut self, min_value: i32, max_value: i32) -> i32 {
            self.inner.random_range(min_value, max_value)
        }
        fn next_double(&mut self) -> f64 {
            self.inner.next_double()
        }
    }

    /// **A-5**：麻痹戒指的 `StdItem.Shape` 判定（`PlayObject.cs:2230-2239`）。
    #[test]
    fn paralysis_from_ring_shapes_matches_ring_shape() {
        assert!(paralysis_from_ring_shapes([
            Some(RING_MAKESTONE_ITEM),
            None
        ]));
        assert!(paralysis_from_ring_shapes([
            None,
            Some(RING_MAKESTONE_ITEM)
        ]));
        assert!(!paralysis_from_ring_shapes([None, Some(0)]));
        assert!(!paralysis_from_ring_shapes([None, None]));
        // 红检（变异牙）：把 `s == RING_MAKESTONE_ITEM` 改成恒 `true` ⇒ 上面两条 `!` 断言必红。
    }

    /// **A-5**：中毒门的抽取**插在** `AttackSuccess` 的 `Random(5)` **之前**（oracle `:221` < `:225`），
    /// 且 `&&` 短路（`UnParalysis` / `!Paralysis` ⇒ 不抽）。
    #[test]
    fn poison_gate_draws_before_weapon_damage_and_shortcircuits() {
        // `attack_poison_rate = 1` ⇒ `random_below(1)` 恒 0 ⇒ 门开时必中毒。
        let cfg = CombatConfig {
            attack_poison_rate: 1,
            attack_poison_time: 5,
            ..CombatConfig::default()
        };
        let mk_attacker = || {
            let mut e = Entity::new(1, "a", 0, 0);
            e.race = ACTOR_RACE_PLAY;
            e.dc = 20;
            e.luck = 0;
            e.power_rate = 100;
            e
        };
        let mk_target = || {
            let mut e = Entity::new(2, "t", 0, 0);
            e.race = ACTOR_RACE_MONSTER;
            e.hit_point = 0; // 命中门不抽
            e.speed_point = 10;
            e.hp = 100;
            e.max_hp = 100;
            e
        };
        let open = AttackPoisonState {
            paralysis: true,
            un_paralysis: false,
            anti_poison: 0,
        };

        let mut rng_open = CountingRng::new(7);
        let (_, poisoned) = attack_dir_grouped_poison(
            &mut rng_open,
            &mut mk_attacker(),
            &mut mk_target(),
            None,
            &cfg,
            &[],
            Some(open),
        );
        assert!(poisoned, "rate=1 ⇒ 中毒门必中");

        let mut rng_none = CountingRng::new(7);
        let _ = attack_dir_grouped_poison(
            &mut rng_none,
            &mut mk_attacker(),
            &mut mk_target(),
            None,
            &cfg,
            &[],
            None,
        );
        // 门开 ⇒ 恰好多一次抽取，且该抽取紧邻末抽（`Random(5)`）之前。
        assert_eq!(
            rng_open.below.len(),
            rng_none.below.len() + 1,
            "门开恰好多一次 `Random(AntiPoison + rate)`"
        );
        let n = rng_open.below.len();
        assert_eq!(
            rng_open.below[n - 1],
            5,
            "末抽 = `AttackSuccess` 的 `Random(5)`"
        );
        assert_eq!(
            rng_open.below[n - 2],
            1,
            "中毒抽样紧接 `Random(5)` 之前（oracle `:221` 先于 `:225`）"
        );

        // `paralysis=false` ⇒ 短路，逐取数等价（与 `None` 完全一致）。
        let mut rng_shut = CountingRng::new(7);
        let (_, p2) = attack_dir_grouped_poison(
            &mut rng_shut,
            &mut mk_attacker(),
            &mut mk_target(),
            None,
            &cfg,
            &[],
            Some(AttackPoisonState {
                paralysis: false,
                ..open
            }),
        );
        assert!(!p2);
        assert_eq!(
            rng_shut.below, rng_none.below,
            "!Paralysis ⇒ 短路，逐取数等价"
        );

        // `un_paralysis=true`（守方免疫）⇒ 短路。
        let mut rng_imm = CountingRng::new(7);
        let (_, p3) = attack_dir_grouped_poison(
            &mut rng_imm,
            &mut mk_attacker(),
            &mut mk_target(),
            None,
            &cfg,
            &[],
            Some(AttackPoisonState {
                un_paralysis: true,
                ..open
            }),
        );
        assert!(!p3);
        assert_eq!(
            rng_imm.below, rng_none.below,
            "UnParalysis ⇒ 短路，逐取数等价"
        );
        // 红检（变异牙）：把中毒门的两条 `&&` 判断去掉、或把抽样挪到 `Random(5)` 之后 ⇒ 上面断言必红。
    }

    /// **A-5**：`IsProperFriend` 的基版（怪→怪）与玩家版（玩家→玩家按 `AttatckMode`）。
    #[test]
    fn is_proper_friend_base_and_player() {
        // 基版：怪施法者只认怪目标（且目标无 master）。
        let mut mon = Entity::new(1, "m", 0, 0);
        mon.race = ACTOR_RACE_MONSTER;
        let mut mon2 = Entity::new(2, "m2", 0, 0);
        mon2.race = ACTOR_RACE_MONSTER;
        assert!(is_proper_friend(&mon, &mon2, false));
        // 目标有 master ⇒ 剔除。
        mon2.master = Some(9);
        assert!(!is_proper_friend(&mon, &mon2, false));
        // 玩家施法者：基版回落 ⇒ 怪目标 false。
        let mut ply = Entity::new(3, "p", 0, 0);
        ply.race = ACTOR_RACE_PLAY;
        assert!(!is_proper_friend(&ply, &mon, false));
        // 玩家→玩家：HAM_ALL/HAM_PEACE ⇒ true；HAM_DEAR ⇒ 仅配偶/自己。
        let mut p2 = Entity::new(4, "p2", 0, 0);
        p2.race = ACTOR_RACE_PLAY;
        assert!(is_proper_friend(&ply, &p2, false), "HAM_ALL 默认 ⇒ 友军");
        ply.attack_mode = ATTACK_MODE_HAM_DEAR;
        assert!(
            !is_proper_friend(&ply, &p2, false),
            "HAM_DEAR 非配偶 ⇒ 非友军"
        );
        ply.dear_human = Some(4);
        assert!(is_proper_friend(&ply, &p2, false), "HAM_DEAR 配偶 ⇒ 友军");
    }

    /// 审计 G09 / MCS-A10：`_Attack`（`BaseObject.Attack.cs:37`）走虚函数 `IsProperTarget`
    /// ——oracle 的基版（`BaseObject.Base.cs:470-473`）与玩家版（`PlayObject.cs:1924-1962`）
    /// **都不查 `Death/Ghost/Invisible`**。折入版会提前把 `nPower = 0`，导致漏掉
    /// `RandomByte(target.SpeedPoint)` 与后续 `GetHitStruckDamage` 的伤害抽数。
    ///
    /// 改坏必红：把 `attack_with_grouped` 改回 [`is_proper_target_grouped`] ⇒ 三态
    /// `rng.below` 均为空、`damage == 0`。
    #[test]
    fn attack_path_uses_direct_proper_target_and_restores_draws() {
        let cfg = CombatConfig::default();
        for flag in ["death", "ghost", "invisible"] {
            let mut attacker = Entity::new(1, "a", 0, 0);
            attacker.race = ACTOR_RACE_MONSTER; // 怪物攻方走基版 IsAttackTarget
            attacker.dc = 0x0A0A; // Lo=Hi=10 ⇒ 攻击力抽 1 次（不记录在 below）
            attacker.hit_point = 100;
            let mut target = Entity::new(2, "t", 0, 0);
            target.race = ACTOR_RACE_PLAY;
            target.hp = 100;
            target.max_hp = 100;
            target.hit_point = 10; // 准确度非 0 ⇒ 命中骰照抽
            target.speed_point = 10;
            target.ac = 0;
            match flag {
                "death" => target.death = true,
                "ghost" => target.ghost = true,
                _ => target.invisible = true,
            }
            // 折入版（显示/枚举路径）仍拒；直通版（_Attack）照 true。
            assert!(
                !is_proper_target_grouped(&attacker, &target, &[]),
                "{flag}: 折入版应拒"
            );
            assert!(
                is_proper_target_grouped_direct(&attacker, &target, &[]),
                "{flag}: 直通版 = oracle IsProperTarget 应接受"
            );

            let mut rng = CountingRng::new(7);
            let out = attack_with_grouped(
                &mut rng,
                &mut attacker,
                &mut target,
                None,
                &cfg,
                AttackPowerMode::Base,
                &[],
            );
            assert!(out.hit, "{flag}: 命中骰必须被抽（hit=true）");
            assert!(out.damage > 0, "{flag}: 必须走伤害结算");
            assert_eq!(
                rng.below.first(),
                Some(&10),
                "{flag}: 第 1 次记录抽必须是 `RandomByte(SpeedPoint=10)`"
            );
            assert!(
                rng.below.len() >= 2,
                "{flag}: 后续 `GetHitStruckDamage`/耐久抽数必须恢复（实际 {:?}）",
                rng.below
            );
        }
    }

    /// 反向对照：仍被折入版判真的正常目标与三态目标的**取数流水完全一致**（除目标状态外
    /// 没有其它门），证明差异只来自 fold。
    #[test]
    fn attack_draw_ledger_is_identical_for_normal_and_folded_flags() {
        fn run(flag: Option<&str>) -> (Vec<i32>, i32, bool) {
            let cfg = CombatConfig::default();
            let mut attacker = Entity::new(1, "a", 0, 0);
            attacker.race = ACTOR_RACE_MONSTER;
            attacker.dc = 0x0A0A;
            attacker.hit_point = 100;
            let mut target = Entity::new(2, "t", 0, 0);
            target.race = ACTOR_RACE_PLAY;
            target.hp = 100;
            target.max_hp = 100;
            target.hit_point = 10;
            target.speed_point = 10;
            target.ac = 0;
            match flag {
                Some("death") => target.death = true,
                Some("ghost") => target.ghost = true,
                Some("invisible") => target.invisible = true,
                _ => {}
            }
            let mut rng = CountingRng::new(11);
            let out = attack_with_grouped(
                &mut rng,
                &mut attacker,
                &mut target,
                None,
                &cfg,
                AttackPowerMode::Base,
                &[],
            );
            (rng.below, i32::from(out.damage), out.hit)
        }
        let normal = run(None);
        for flag in ["death", "ghost", "invisible"] {
            assert_eq!(run(Some(flag)), normal, "{flag}: 取数流水应与正常目标一致");
        }
    }
    /// PL-3 / P27：`win_exp` 读 **per-player** `kill_mon_exp_rate`（全局 `CombatConfig` 字段已删）；
    /// `rate=100` 时与旧公式逐字节相同（既有 golden 回归门）。
    ///
    /// 牙：改回全局字段/忽略 `killer.kill_mon_exp_rate` ⇒ 200 与队伍用例必红。
    #[test]
    fn win_exp_reads_per_player_kill_mon_exp_rate() {
        let cfg = CombatConfig::default();
        let need = [0i32; 256];
        for dw in [0, 1, 3, 25, 99, 100, 101, 999, 12_345] {
            let mut e = Entity::new(1, "a", 0, 0);
            e.kill_mon_exp_rate = 100;
            let (got, _, _) = win_exp(&mut e, dw, &need, &cfg, None);
            assert_eq!(got, dw, "rate=100 ⇒ 旧公式 round(1.0*dw) = dw");
        }
        let mut e = Entity::new(1, "a", 0, 0);
        e.kill_mon_exp_rate = 200;
        assert_eq!(win_exp(&mut e, 123, &need, &cfg, None).0, 246);
        let mut e = Entity::new(1, "a", 0, 0);
        e.kill_mon_exp_rate = 0;
        assert_eq!(win_exp(&mut e, 123, &need, &cfg, None).0, 0);
        // 队伍 per-member：A=100 / B=200 ⇒ B 拿双倍（oracle 对每成员调各自的 WinExp）。
        let mut a = Entity::new(1, "a", 0, 0);
        a.kill_mon_exp_rate = 100;
        let mut b = Entity::new(2, "b", 0, 0);
        b.kill_mon_exp_rate = 200;
        let ea = win_exp(&mut a, 280, &need, &cfg, None).0;
        let eb = win_exp(&mut b, 280, &need, &cfg, None).0;
        assert_eq!((ea, eb), (280, 560));
    }
}
