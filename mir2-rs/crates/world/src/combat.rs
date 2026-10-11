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
