//! 等级基础属性/负重公式，移植源 `src/M2Server/Player/PlayObject.cs:5254`
//! （`RecalcLevelAbilitys`，逐条照抄含 bug）。

use mir2_data::models::MAX_LEVEL;
use mir2_shared::hutil;

/// C# `PlayerJob`（`src/OpenMir2/Enums/PlayerJob.cs`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlayerJob {
    /// 战士 = 0
    Warrior,
    /// 法师 = 1
    Wizard,
    /// 道士 = 2
    Taoist,
}

/// `RecalcLevelAbilitys` 读到的 `SystemShare.Config` 字段
/// （默认值见 `GameSvrConf.cs:1888-1894`）。
#[derive(Clone, Copy, Debug)]
pub struct LevelValueConfig {
    pub taos_hp: i32,        // nLevelValueOfTaosHP = 6
    pub taos_hp_rate: f64,   // nLevelValueOfTaosHPRate = 2.5
    pub taos_mp: i32,        // nLevelValueOfTaosMP = 8
    pub wizard_hp: i32,      // nLevelValueOfWizardHP = 15
    pub wizard_hp_rate: f64, // nLevelValueOfWizardHPRate = 1.8
    pub warr_hp: i32,        // nLevelValueOfWarrHP = 4
    pub warr_hp_rate: f64,   // nLevelValueOfWarrHPRate = 4.5
}

impl Default for LevelValueConfig {
    fn default() -> Self {
        LevelValueConfig {
            taos_hp: 6,
            taos_hp_rate: 2.5,
            taos_mp: 8,
            wizard_hp: 15,
            wizard_hp_rate: 1.8,
            warr_hp: 4,
            warr_hp_rate: 4.5,
        }
    }
}

/// `RecalcLevelAbilitys` 写回的 `Abil` 子集。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LevelAbility {
    pub max_hp: u16,
    pub max_mp: u16,
    pub max_weight: u16,
    pub max_wear_weight: u8,
    pub max_hand_weight: u8,
    pub dc: u16,
    pub mc: u16,
    pub sc: u16,
    pub ac: u16,
    pub mac: u16,
}

/// C# `PlayObject.RecalcLevelAbilitys` 逐条移植。
///
/// `n_level` 对应 C# 的 `byte nLevel = Abil.Level`（0..=255）。
///
/// BUG-REPLICA（`PlayObject.cs:5265-5271`）：道士 MaxHandWeight 的**判断**
/// 用 `Abil.Level / 13.0`，而**赋值**用 `nLevel / 42.0`——两处除数不同
/// （且判断与赋值用的都是等级本身，判断错用了 13）。此处原样保留。
pub fn recalc_level_abilitys(job: PlayerJob, n_level: u8, cfg: &LevelValueConfig) -> LevelAbility {
    let mut abil = LevelAbility::default();
    let lv = i32::from(n_level);
    match job {
        PlayerJob::Taoist => {
            abil.max_hp = hutil::min_i32(
                i32::from(u16::MAX),
                14 + hutil::round(
                    (lv as f64 / f64::from(cfg.taos_hp) + cfg.taos_hp_rate) * lv as f64,
                ),
            ) as u16;
            abil.max_mp = hutil::min_i32(
                i32::from(u16::MAX),
                13 + hutil::round(lv as f64 / f64::from(cfg.taos_mp) * 2.2 * lv as f64),
            ) as u16;
            abil.max_weight = (50 + hutil::round(lv as f64 / 4.0 * lv as f64)) as u16;
            abil.max_wear_weight = hutil::min_i32(
                i32::from(u8::MAX),
                15 + hutil::round(lv as f64 / 50.0 * lv as f64),
            ) as u8;
            if 12 + hutil::round(lv as f64 / 13.0 * lv as f64) > 255 {
                abil.max_hand_weight = u8::MAX;
            } else {
                abil.max_hand_weight = (12 + hutil::round(lv as f64 / 42.0 * lv as f64)) as u8;
            }
            let n = lv / 7;
            abil.dc =
                hutil::make_word(hutil::max_i32(n - 1, 0) as u16, hutil::max_i32(1, n) as u16);
            abil.mc = 0;
            abil.sc =
                hutil::make_word(hutil::max_i32(n - 1, 0) as u16, hutil::max_i32(1, n) as u16);
            abil.ac = 0;
            let n = hutil::round(lv as f64 / 6.0);
            abil.mac = hutil::make_word((n / 2) as u16, (n + 1) as u16);
        }
        PlayerJob::Wizard => {
            abil.max_hp = hutil::min_i32(
                i32::from(u16::MAX),
                14 + hutil::round(
                    (lv as f64 / f64::from(cfg.wizard_hp) + cfg.wizard_hp_rate) * lv as f64,
                ),
            ) as u16;
            abil.max_mp = hutil::min_i32(
                i32::from(u16::MAX),
                13 + hutil::round((lv as f64 / 5.0 + 2.0) * 2.2 * lv as f64),
            ) as u16;
            abil.max_weight = (50 + hutil::round(lv as f64 / 5.0 * lv as f64)) as u16;
            abil.max_wear_weight = hutil::min_i32(
                i32::from(u8::MAX),
                15 + hutil::round(lv as f64 / 100.0 * lv as f64),
            ) as u8;
            abil.max_hand_weight = (12 + hutil::round(lv as f64 / 90.0 * lv as f64)) as u8;
            let n = lv / 7;
            abil.dc =
                hutil::make_word(hutil::max_i32(n - 1, 0) as u16, hutil::max_i32(1, n) as u16);
            abil.mc =
                hutil::make_word(hutil::max_i32(n - 1, 0) as u16, hutil::max_i32(1, n) as u16);
            abil.sc = 0;
            abil.ac = 0;
            abil.mac = 0;
        }
        PlayerJob::Warrior => {
            abil.max_hp = hutil::min_i32(
                i32::from(u16::MAX),
                14 + hutil::round(
                    (lv as f64 / f64::from(cfg.warr_hp) + cfg.warr_hp_rate + lv as f64 / 20.0)
                        * lv as f64,
                ),
            ) as u16;
            abil.max_mp =
                hutil::min_i32(i32::from(u16::MAX), 11 + hutil::round(lv as f64 * 3.5)) as u16;
            abil.max_weight = (50 + hutil::round(lv as f64 / 3.0 * lv as f64)) as u16;
            abil.max_wear_weight = hutil::min_i32(
                i32::from(u8::MAX),
                15 + hutil::round(lv as f64 / 20.0 * lv as f64),
            ) as u8;
            abil.max_hand_weight = (12 + hutil::round(lv as f64 / 13.0 * lv as f64)) as u8;
            abil.dc = hutil::make_word(
                hutil::max_i32(lv / 5 - 1, 1) as u16,
                hutil::max_i32(1, lv / 5) as u16,
            );
            abil.sc = 0;
            abil.mc = 0;
            abil.ac = hutil::make_word(0, (lv / 7) as u16);
            abil.mac = 0;
        }
    }
    abil
}

/// C# 尾部钳制：`HP > MaxHP → HP = MaxHP`（MP 同理）。
/// 独立成函数，调用方传入当前血魔。
pub fn clamp_hp_mp(abil: &LevelAbility, hp: u16, mp: u16) -> (u16, u16) {
    (hp.min(abil.max_hp), mp.min(abil.max_mp))
}

/// 最大等级常量转发（C# `Grobal2.MaxLevel`），便于调用方做 0..=255 遍历。
pub const MAX_PLAYER_LEVEL: i32 = MAX_LEVEL;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn warrior_level1() {
        let a = recalc_level_abilitys(PlayerJob::Warrior, 1, &LevelValueConfig::default());
        // MaxHP = 14 + Round((1/4 + 4.5 + 1/20) * 1) = 14 + Round(4.8) = 19
        assert_eq!(a.max_hp, 19);
        // MaxMP = 11 + Round(3.5) = 15（AwayFromZero 3.5 → 4）
        assert_eq!(a.max_mp, 15);
        // MaxWeight = 50 + Round(1/3) = 50
        assert_eq!(a.max_weight, 50);
        // DC = MakeWord(max(0-1,1)=1... lv/5=0 → max(0-1,1)=1? lv/5-1 = -1 → max(-1,1)=1；hi=max(1,0)=1
        assert_eq!(a.dc, hutil::make_word(1, 1));
    }

    #[test]
    fn taoist_hand_weight_bug_replica() {
        // BUG-REPLICA：判断用 /13，赋值用 /42。lv=57 时判断 12+Round(57²/13)=12+250=262>255
        // → 走 u8::MAX；若按赋值式 12+Round(57²/42)=12+77=89 ≠ 255，两式分歧证明 bug 生效。
        let a = recalc_level_abilitys(PlayerJob::Taoist, 57, &LevelValueConfig::default());
        assert_eq!(a.max_hand_weight, u8::MAX);
        let lv40 = recalc_level_abilitys(PlayerJob::Taoist, 40, &LevelValueConfig::default());
        // lv=40：判断 12+Round(1600/13)=12+123=135 ≤255 → 赋值 12+Round(1600/42)=12+38=50
        assert_eq!(lv40.max_hand_weight, 50);
    }

    #[test]
    fn wizard_level50() {
        let a = recalc_level_abilitys(PlayerJob::Wizard, 50, &LevelValueConfig::default());
        // MaxHP = 14 + Round((50/15 + 1.8) * 50) = 14 + Round(256.667) = 271
        assert_eq!(a.max_hp, 271);
        // MaxMP = 13 + Round((10 + 2) * 2.2 * 50) = 13 + 1320 = 1333
        assert_eq!(a.max_mp, 1333);
    }
}
