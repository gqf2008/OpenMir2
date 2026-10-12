//! oracle `PlayObject.Run()` 的**周期恢复段**（`src/M2Server/Player/PlayObject.Message.cs:70-141`）。
//!
//! 这是 T-28（卡 `openmir2-m3-recovery-tick`）的实现：吃下 `StdMode 0` 的药品后
//! `IncHealth += AC`（`PlayObject.Base.cs:2073-2076`），oracle 每 `Run()` 一次就按下面的节奏
//! 把 `IncHealth/IncSpell/IncHealing` 消费回 `WAbil.HP/MP`，并在消费后调
//! `BaseObject.IncHealthSpell`（`BaseObject.cs:952-975`）⇒ `HealthSpellChanged()`（`BaseObject.cs:700-710`）。
//!
//! ```text
//! PlayObject.Run()                                          PlayObject.Message.cs:70
//!   !Death && (IncSpell>0 || IncHealth>0 || IncHealing>0) ?
//!     dwInChsTime = 600 - _MIN(400, WAbil.Level*10)          :72
//!     if (GetTickCount()-IncHealthSpellTick) >= dwInChsTime && !Death   :73
//!       incHealthTick = _MIN(200, GetTickCount()-IncHealthSpellTick-dwInChsTime)  :75
//!       IncHealthSpellTick = GetTickCount() + incHealthTick  :76
//!       if (IncSpell>0 || IncHealth>0 || PerHealing>0)       :77
//!         Per{Health,Spell,Healing} <= 0 ⇒ = 1              :79-90
//!         nHP = IncHealth<PerHealth ? IncHealth(置 0) : PerHealth(扣减)      :92-101
//!         nMP = IncSpell<PerSpell   ? IncSpell (置 0) : PerSpell (扣减)      :102-111
//!         nHP += IncHealing<PerHealing ? IncHealing(置 0) : PerHealing(扣减) :113-122
//!         PerHealth = PerSpell = Level/10+5; PerHealing = 5  :123-125
//!         IncHealthSpell(nHP, nMP)                            :126  → HealthSpellChanged()
//!         HP==MaxHP ⇒ IncHealth=0, IncHealing=0               :127-131
//!         MP==MaxMP ⇒ IncSpell=0                              :133-135
//!   : else IncHealthSpellTick = GetTickCount()                :139-142
//! ```
//!
//! `PerHealth/PerSpell/PerHealing` 与 `IncHealthSpellTick` 的初值照
//! `CharacterObject` 构造函数（`CharacterObject.cs:83-86`：三项均 5、`tick = GetTickCount()`）。
//!
//! # 登记（未建模，不静默）
//!
//! - **`Death`**：由调用方从世界实体 [`mir2_world::Entity::death`] 读入（与 `movewire` 同源）。
//! - **`ShowHp`** 的下行广播（`HealthSpellChanged` 的 `if (ShowHp)` 支）在 oracle 里由
//!   `MakeOpenHealth()`（`BaseObject.cs:944`）置位，而它只被 `RM_DOOPENHEALTH` 驱动
//!   （`BaseObject.Base.cs:763`），发点是技能「心灵启示」`MagicManager.cs:438`
//!   ⇒ **技能系统未移植，`ShowHp` 在本宿主恒 false**。本模块照 oracle 保留该支（调用方传
//!   `show_hp`），生产路径上不可达；一旦技能层接入即可生效。
//! - 血石/魔血石（`CharacterObject.cs:160-230` 的 `IncHealth += stoneDura`）、
//!   `StateTimeArr`/buff、`AutoTimer` 等 `Run()` 的其它段不在本切片（各自另有卡）。

use crate::ability::Ability;

/// `CharacterObject` 上与周期恢复相关的计数器（`IncHealth/IncSpell/IncHealing` + `Per*` + tick）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Recovery {
    /// `CharacterObject.IncHealth`（`ushort`）。
    pub inc_health: u16,
    /// `CharacterObject.IncSpell`（`ushort`）。
    pub inc_spell: u16,
    /// `CharacterObject.IncHealing`（`ushort`）。
    pub inc_healing: u16,
    /// `CharacterObject.PerHealth`（`byte`）。
    pub per_health: u8,
    /// `CharacterObject.PerSpell`（`byte`）。
    pub per_spell: u8,
    /// `CharacterObject.PerHealing`（`byte`）。
    pub per_healing: u8,
    /// `CharacterObject.IncHealthSpellTick`（oracle `int`；本宿主用 `i64` 毫秒）。
    pub inc_health_spell_tick: i64,
}

impl Recovery {
    /// `CharacterObject()` 的初值（`CharacterObject.cs:83-86`）：三项 `Per* = 5`、tick = now。
    #[must_use]
    pub fn new(now: i64) -> Self {
        Self {
            inc_health: 0,
            inc_spell: 0,
            inc_healing: 0,
            per_health: 5,
            per_spell: 5,
            per_healing: 5,
            inc_health_spell_tick: now,
        }
    }
}

/// `BaseObject.IncHealthSpell(int nHp, int nMp)`（`BaseObject.cs:952-975`）。
///
/// 返回 `false` ⇔ oracle 开头 `nHp<0 || nMp<0` 提前 `return`（**不**调 `HealthSpellChanged`）。
/// `HP/MP` 是 `ushort` 语义（oracle `(ushort)nHp` 强转），达上限则夹到 `MaxHP/MaxMP`。
pub fn inc_health_spell(wabil: &mut Ability, n_hp: i32, n_mp: i32) -> bool {
    if n_hp < 0 || n_mp < 0 {
        return false;
    }
    if wabil.hp as i32 + n_hp >= wabil.max_hp as i32 {
        wabil.hp = wabil.max_hp;
    } else {
        wabil.hp = wabil.hp.wrapping_add(n_hp as u16);
    }
    if wabil.mp as i32 + n_mp >= wabil.max_mp as i32 {
        wabil.mp = wabil.max_mp;
    } else {
        wabil.mp = wabil.mp.wrapping_add(n_mp as u16);
    }
    true
}

/// `PlayObject.Run()` 的周期恢复段（`PlayObject.Message.cs:70-141`），一个 world tick 调一次。
///
/// 返回值 = 本轮是否调用了 `IncHealthSpell`（即 oracle 是否走到 `HealthSpellChanged()`：
/// 自见恒发 `RM_HEALTHSPELLCHANGED`，`ShowHp` 时再 `SendRefMsg`）。
/// `now` = `HUtil32.GetTickCount()` 的当轮值（毫秒；只有差值参与判定）。
#[must_use]
pub fn recovery_tick(rec: &mut Recovery, wabil: &mut Ability, death: bool, now: i64) -> bool {
    if !death && (rec.inc_spell > 0 || rec.inc_health > 0 || rec.inc_healing > 0) {
        // `WAbil.Level * 10`（oracle `int` 算术）。
        let level_scaled = i64::from(wabil.level) * 10;
        let dw_in_chs_time = 600 - level_scaled.min(400);
        // 外层已判 `!death`；内层照 oracle 原样保留 `&& !Death`。
        if now - rec.inc_health_spell_tick >= dw_in_chs_time && !death {
            let inc_health_tick = (now - rec.inc_health_spell_tick - dw_in_chs_time).min(200);
            rec.inc_health_spell_tick = now + inc_health_tick;
            if rec.inc_spell > 0 || rec.inc_health > 0 || rec.per_healing > 0 {
                if rec.per_health == 0 {
                    rec.per_health = 1;
                }
                if rec.per_spell == 0 {
                    rec.per_spell = 1;
                }
                if rec.per_healing == 0 {
                    rec.per_healing = 1;
                }
                let mut n_hp;
                if rec.inc_health < u16::from(rec.per_health) {
                    n_hp = i32::from(rec.inc_health);
                    rec.inc_health = 0;
                } else {
                    n_hp = i32::from(rec.per_health);
                    rec.inc_health -= u16::from(rec.per_health);
                }
                let n_mp;
                if rec.inc_spell < u16::from(rec.per_spell) {
                    n_mp = i32::from(rec.inc_spell);
                    rec.inc_spell = 0;
                } else {
                    n_mp = i32::from(rec.per_spell);
                    rec.inc_spell -= u16::from(rec.per_spell);
                }
                if rec.inc_healing < u16::from(rec.per_healing) {
                    n_hp += i32::from(rec.inc_healing);
                    rec.inc_healing = 0;
                } else {
                    n_hp += i32::from(rec.per_healing);
                    rec.inc_healing -= u16::from(rec.per_healing);
                }
                rec.per_health = wabil.level / 10 + 5;
                rec.per_spell = wabil.level / 10 + 5;
                rec.per_healing = 5;
                inc_health_spell(wabil, n_hp, n_mp);
                if wabil.hp == wabil.max_hp {
                    rec.inc_health = 0;
                    rec.inc_healing = 0;
                }
                if wabil.mp == wabil.max_mp {
                    rec.inc_spell = 0;
                }
                return true;
            }
        }
    } else {
        rec.inc_health_spell_tick = now;
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `Ability` 零值构造（`level` 由用例给）。
    fn ability(level: u8, hp: u16, mp: u16, max_hp: u16, max_mp: u16) -> Ability {
        Ability {
            level,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            hp,
            mp,
            max_hp,
            max_mp,
            reserved2: 0,
            exp_count: 0,
            exp_max_count: 0,
            exp: 0,
            max_exp: 0,
            weight: 0,
            max_weight: 0,
            wear_weight: 0,
            max_wear_weight: 0,
            hand_weight: 0,
            max_hand_weight: 0,
        }
    }

    /// **本卡的判据（构造向量）**：1 级角色 `dwInChsTime = 600 - 10 = 590`，
    /// `PerHealth/PerSpell/PerHealing` 初值 5 ⇒ 每 590ms 消费 5 点，逐轮钉住
    /// `IncHealth/HP/PerHealth/IncHealthSpellTick` 的整条消费序列。
    ///
    /// 红检①：删掉 `IncHealth < PerHealth` 分支里的 `rec.inc_health = 0` ⇒
    /// [`partial_pool_is_zeroed_and_not_reconsumed`] 必红（本用例 `IncHealth == PerHealth` 走 else 支，
    /// 对那处置零不敏感）。
    /// 红检②：把 `dwInChsTime` 改成常数（如 590 硬编码）不影响 1 级，故红检改在 20 级用例
    /// （[`dw_in_chs_time_shrinks_with_level`]）上钉住。
    #[test]
    fn pooled_drug_is_consumed_over_ticks_when_not_at_full_hp() {
        let mut wabil = ability(1, 0, 0, 19, 15);
        // `金创药(小量)` StdMode 0：`IncHealth += AC = 20`；tick 初值 0。
        let mut rec = Recovery {
            inc_health: 20,
            ..Recovery::new(0)
        };
        // t<590：未到点，不动。
        assert!(!recovery_tick(&mut rec, &mut wabil, false, 589));
        assert_eq!((rec.inc_health, wabil.hp), (20, 0));
        assert_eq!(rec.inc_health_spell_tick, 0);

        // t=590：nHP = min(20, PerHealth=5) = 5 ⇒ HP=5，IncHealth=15；Per* 保持 5。
        assert!(recovery_tick(&mut rec, &mut wabil, false, 590));
        assert_eq!((wabil.hp, wabil.mp), (5, 0));
        assert_eq!(rec.inc_health, 15);
        assert_eq!(rec.per_health, 5);
        // 到期精确：incHealthTick = min(200, 590-0-590)=0 ⇒ tick=590。
        assert_eq!(rec.inc_health_spell_tick, 590);

        // t=1180：+5 ⇒ HP=10，IncHealth=10。
        assert!(recovery_tick(&mut rec, &mut wabil, false, 1180));
        assert_eq!((wabil.hp, rec.inc_health), (10, 10));

        // t=1770：+5 ⇒ HP=15，IncHealth=5。
        assert!(recovery_tick(&mut rec, &mut wabil, false, 1770));
        assert_eq!((wabil.hp, rec.inc_health), (15, 5));

        // t=2360：IncHealth(5) !< PerHealth(5) ⇒ nHP=5、IncHealth=0（红检①在此必红）。
        assert!(recovery_tick(&mut rec, &mut wabil, false, 2360));
        assert_eq!((wabil.hp, rec.inc_health), (19, 0), "HP 夹到 MaxHP=19");

        // t=2950：Inc 全 0 ⇒ else 支重置 tick，不发帧。
        assert!(!recovery_tick(&mut rec, &mut wabil, false, 2950));
        assert_eq!(rec.inc_health_spell_tick, 2950);
    }

    /// **红检①的靶子**：`IncHealth < PerHealth` 支必须把 `IncHealth` 置 0（`PlayObject.Message.cs:95`），
    /// 否则池不会被清、下一轮还会再消费一次（双计）。
    #[test]
    fn partial_pool_is_zeroed_and_not_reconsumed() {
        let mut wabil = ability(1, 0, 0, 100, 100);
        let mut rec = Recovery {
            inc_health: 3,
            ..Recovery::new(0)
        };
        // t=590：IncHealth(3) < PerHealth(5) ⇒ nHP=3、IncHealth=0。
        assert!(recovery_tick(&mut rec, &mut wabil, false, 590));
        assert_eq!((wabil.hp, rec.inc_health), (3, 0));
        // t=1180：池已空 ⇒ 不发帧、HP 不涨（删掉那处置零 ⇒ 这里 HP=6 且返回 true，必红）。
        assert!(!recovery_tick(&mut rec, &mut wabil, false, 1180));
        assert_eq!(wabil.hp, 3, "空池不得被二次消费");
    }

    /// `dwInChsTime = 600 - _MIN(400, Level*10)`（`:72`）随等级收缩；20 级 ⇒ 600-200=400。
    ///
    /// 红检②：把 `dwInChsTime` 改成常数 590 ⇒ 400ms 处不会触发 ⇒ 本用例必红。
    #[test]
    fn dw_in_chs_time_shrinks_with_level() {
        let mut wabil = ability(20, 0, 0, 100, 100);
        let mut rec = Recovery {
            inc_health: 10,
            ..Recovery::new(0)
        };
        assert!(
            !recovery_tick(&mut rec, &mut wabil, false, 399),
            "20 级阈值 400ms"
        );
        assert!(recovery_tick(&mut rec, &mut wabil, false, 400));
        // 首轮消费 PerHealth=5 ⇒ HP=5、IncHealth=5（20 级时 PerHealth 仍是初值 5）。
        assert_eq!((wabil.hp, rec.inc_health), (5, 5));
    }

    /// `IncSpell`/`IncHealing` 同构消费；`nMP` 与 `nHP` 分别进 `MP`/`HP`。
    #[test]
    fn inc_spell_and_inc_healing_are_consumed_into_mp_and_hp() {
        let mut wabil = ability(1, 0, 0, 19, 19);
        let mut rec = Recovery {
            inc_spell: 8,
            inc_healing: 3,
            ..Recovery::new(0)
        };
        assert!(recovery_tick(&mut rec, &mut wabil, false, 590));
        // nMP = min(8,5)=5 ⇒ MP=5、IncSpell=3；nHP = 0 + min(3,5)=3 ⇒ HP=3、IncHealing=0。
        assert_eq!((wabil.hp, rec.inc_healing), (3, 0));
        assert_eq!((wabil.mp, rec.inc_spell), (5, 3));
    }

    /// **HP 打满清池**（`:127-135`）：HP==MaxHP ⇒ `IncHealth=0, IncHealing=0`；
    /// MP==MaxMP ⇒ `IncSpell=0`。这是 T-14 `characters.IncHealth` 归零的主路径
    /// （`characters_ablity.Hp=MaxHP=19`）。
    #[test]
    fn full_hp_clears_the_health_pool() {
        let mut wabil = ability(1, 19, 15, 19, 15);
        let mut rec = Recovery {
            inc_health: 20,
            inc_healing: 7,
            ..Recovery::new(0)
        };
        assert!(recovery_tick(&mut rec, &mut wabil, false, 590));
        assert_eq!(wabil.hp, 19, "已满则夹回 MaxHP");
        assert_eq!(rec.inc_health, 0, "HP 打满 ⇒ IncHealth=0");
        assert_eq!(rec.inc_healing, 0, "HP 打满 ⇒ IncHealing=0");
    }

    /// `Death` 为真 ⇒ 走 else 支重置 tick、不消费（oracle `:70/:139`）。
    #[test]
    fn death_resets_tick_without_consuming() {
        let mut wabil = ability(1, 0, 0, 19, 15);
        let mut rec = Recovery {
            inc_health: 20,
            ..Recovery::new(0)
        };
        assert!(!recovery_tick(&mut rec, &mut wabil, true, 5000));
        assert_eq!((wabil.hp, rec.inc_health), (0, 20));
        assert_eq!(rec.inc_health_spell_tick, 5000);
    }

    /// 超时折回 `_MIN(200, …)`（`:75-76`）：积压 1000ms 只在 tick 上折回 200ms。
    #[test]
    fn large_gap_folds_back_at_most_200ms() {
        let mut wabil = ability(1, 0, 0, 1000, 1000);
        let mut rec = Recovery {
            inc_health: 100,
            ..Recovery::new(0)
        };
        assert!(recovery_tick(&mut rec, &mut wabil, false, 10_000));
        assert_eq!(
            rec.inc_health_spell_tick,
            10_000 + 200,
            "incHealthTick = _MIN(200, 10000-0-590)"
        );
    }
}
