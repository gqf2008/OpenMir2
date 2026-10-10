//! 战斗子集 —— 移植源（逐条照抄，含注释里标注的 C# 行号）：
//! - `src/M2Server/Actor/BaseObject.Attack.cs:25` `GetBaseAttackPoewr`
//! - `BaseObject.Attack.cs:30` `_Attack`
//! - `BaseObject.Base.cs:886` `GetAttackPower`
//! - `BaseObject.Base.cs:789` `GetHitStruckDamage`
//! - `BaseObject.Base.cs:827` `StruckDamage`（职业对怪倍率、人物下属对人倍率）
//! - `BaseObject.cs:775` `DamageHealth`（护盾分支保留结构）
//! - `src/M2Server/Player/PlayObject.Base.cs:129/149` `WinExp` / `GetExp`（含单步升级）
//! - `src/M2Server/Player/PlayObject.cs:1638` `KillTargetTrigger`（`CalcGetExp` → `GainExp`）
//! - `src/GameSrv/Word/WorldServer.MonGen.cs:440` `MonGetRandomItems`（**掉落在地图生成怪物时预算**，
//!   死亡时只是把预算好的物品散到地上）——由 `crates/formula::drop` 提供（D 线已对拍 == C#）
//!
//! 本批**不做**的（报告里写明）：技能（`MagicManager` 面太大）、狂暴/组队/任务条件、
//! 护身符与毒/麻痹状态、地图 `CanWalk` 地形（骨架格子属性恒 Walk）。

use mir2_data::models::StdItem;
use mir2_formula::drop::{mon_get_random_items, ItemCatalog, ItemNumberCounter, UserItem};
use mir2_formula::exp::{calc_get_exp, get_level_exp};
use mir2_shared::hutil;
use mir2_shared::rng::RandomSource;

use crate::entity::{Entity, ACTOR_RACE_PLAY};

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
    let lo = i32::from(lo_byte(dc));
    let hi = i32::from(hi_byte(dc));
    let diff = (hi - lo) as i8 as i32; // C# 的 (sbyte) 强转
    get_attack_power(rng, lo, diff)
}

/// 攻击合法性（`BaseObject.IsProperTarget` → `IsAttackTarget` 的子集）。
///
/// 子集口径（报告里写明未覆盖的分支）：玩家↔怪物的常规敌对判定；
/// 不打自己/自己宝宝、不打已死或已隐身（Ghost）对象。
pub fn is_attack_target(attacker: &Entity, target: &Entity) -> bool {
    if attacker.id == target.id {
        return false;
    }
    if target.death || target.ghost || target.invisible {
        return false;
    }
    if target.master == Some(attacker.id) {
        return false; // 自己的宝宝
    }
    if attacker.race == ACTOR_RACE_PLAY && target.race != ACTOR_RACE_PLAY {
        return true; // 人打怪
    }
    if attacker.race != ACTOR_RACE_PLAY && target.race == ACTOR_RACE_PLAY {
        return true; // 怪打人
    }
    false
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
    if damage > 0 && target.life_attrib == 1 && attacker.undead_power > 0 {
        // LifeAttrib == LA_UNDEAD(1) 时加攻击方的 UndeadPower
        damage += attacker.undead_power;
    }
    damage as u16
}

/// `BaseObject.StruckDamage`（`BaseObject.Base.cs:827`）的职业/主从倍率部分。
///
/// 配置项取 `GameSvrConf` 默认值（`WarrMon/WizardMon/TaosMon/MonHum` 都是 10 ⇒ 倍率 1.0，
/// 与现网 Server.conf 未改动时一致；如需别的值由调用方传入）。
pub fn struck_damage_multiplier(
    target: &Entity,
    attacker: &Entity,
    warr_mon: i32,
    wizard_mon: i32,
    taos_mon: i32,
    mon_hum: i32,
) -> i32 {
    // 返回"十分之几"的倍率（C# 里是 `伤害 * per10 / 10`）
    if target.race >= 50 && attacker.race == ACTOR_RACE_PLAY {
        // Race >= ActorRace.Animal(50) 且最后一击者是人
        match attacker.job {
            0 => warr_mon,   // Warrior
            1 => wizard_mon, // Wizard
            _ => taos_mon,   // Taoist
        }
    } else if target.race == ACTOR_RACE_PLAY && attacker.master.is_some() {
        // 人物下属怪物攻击人
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
}

/// `_Attack`（`BaseObject.Attack.cs:30`）——一次攻击的完整结算。
///
/// **RNG 消耗顺序**（与 C# 一致，勿改）：
/// 1. 攻击力：`GetAttackPower` → `Random(power+1)`（1 次）
/// 2. 命中判定：`RandomByte(target.SpeedPoint)` → `Random(SpeedPoint)`（1 次）
/// 3. 护甲：`Random(|ΔAC|+1)`，若 `nRnd>0` 再 `Random(nRnd)`（1~2 次）
pub fn attack(
    rng: &mut impl RandomSource,
    attacker: &mut Entity,
    target: &mut Entity,
    config: &CombatConfig,
) -> AttackOutcome {
    let power = get_base_attack_power(rng, attacker.dc);
    let mut n_power = power;
    let mut hit = false;

    if is_attack_target(attacker, target) {
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
                attacker,
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
            target.last_hiter = Some(attacker.id);
            if target.hp == 0 {
                killed = true;
            }
        }
    }
    AttackOutcome {
        power,
        hit,
        damage,
        killed,
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

/// 死亡时的经验结算（`PlayObject.cs:1638 KillTargetTrigger` +
/// `PlayObject.Base.cs:129 WinExp` + `:149 GetExp` 的子集）。
///
/// 返回 `(获得经验, 是否升级, 升级后等级)`。
pub fn award_kill_exp(
    killer: &mut Entity,
    mon_level: i32,
    mon_exp: i32,
    need_exps: &[i32],
    config: &CombatConfig,
) -> (i32, bool, u8) {
    // KillTargetTrigger: monsterExp = CalcGetExp(自己等级, 怪物经验)
    let monster_exp = calc_get_exp(
        i32::from(killer.level),
        mon_level,
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
