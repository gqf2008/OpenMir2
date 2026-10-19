//! PL-3 的三段玩家 `Run()` 周期状态（**纯逻辑**）：
//!
//! - **P25**：地图 `DECHP`/`INCHP`（`src/M2Server/Player/PlayObject.Message.cs:569-594`）；
//! - **P26**：饥饿系统（`:596-635`）；
//! - **P27**：经验/攻击倍率 1s 递减（`:636-657`）。
//!
//! 纯函数把「时间门 + 状态机」与宿主（`Entity`/`SessionGround`/出帧）分开，便于构造级判据；
//! 宿主接线见 `main.rs` 的 `drain_player_map_hp` / `drain_player_hunger` /
//! `drain_player_run_rates`。
//!
//! # 照抄的 oracle 形态（禁止“顺手修好”）
//!
//! - **P25 的 `INCHP` 用的是 `nDECHPPOINT`**（`:585` 判上限、`:587` 加血）：`nINCHPPOINT`
//!   全仓无消费者。⇒ INCHP-only 地图恒为 no-op，但每周期仍发一条 53。
//! - `DecHpTick` / `IncHpTick` 的 ctor **不赋值** ⇒ 初值 0 ⇒ 进图后第一次 `Run` 立即触发
//!   （`now - 0 > time*1000`）。
//! - 门是**严格 `>`**；`time * 1000` 是 C# `int` 乘法（unchecked）⇒ 用 `wrapping_mul`。
//! - HP 运算是 **`ushort` 回绕**（`(ushort)point` 先截断；`+=`/`-=` 都模 65536）；比较是
//!   **`int` 提升**（`i32::from(hp)`）。
//! - P25/P26 **都没有 `!Death` 门**：死玩家照跑。
//! - P26 的 `PerHealth/PerSpell` 是 `byte`：`-= 1` 在 0 处**回绕到 255**。
//! - P26 的 `HungerStatus` **无下限**（可无限负）；`GetMyStatus` 只有 `> 4 ⇒ 4` 的上限，
//!   `HungerStatus = -1001` ⇒ `-1`（向零截断）。
//! - P27 的 `ExpRateTick` **过门无条件刷新**；两个倒计时**独立**、每秒各减 1（`if` 非 `while`，
//!   不追帧）；`Time = 0` 是**永久**倍率。

use mir2_envir::MapInfoFlag;

/// `PlayObject.DecHpTick` / `IncHpTick`（`PlayObject.cs:783-784`；ctor 不赋值 ⇒ 0）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MapHpTicks {
    pub dec_hp_tick: i64,
    pub inc_hp_tick: i64,
}

/// P25：一次 `Run()` 的地图 `DECHP`/`INCHP` 段。
///
/// 返回本段调用 `HealthSpellChanged()` 的**次数**（0/1/2）——每次调用都要在 G-B 槽里
/// `SendUpdateMsg(53)`（头-only 去重⇒ 同 Run 两段最多一条 pending，但 `ShowHp` 的自发副本
/// 每次一份，故次数必须逐次上报）。
pub fn map_hp_tick(
    flag: &MapInfoFlag,
    hp: &mut u16,
    max_hp: u16,
    now: i64,
    ticks: &mut MapHpTicks,
) -> u8 {
    let mut calls = 0_u8;
    // `(Envir.Flag.nDECHPTIME * 1000)`：C# int 乘法 unchecked。
    if flag.bo_dechp && now - ticks.dec_hp_tick > i64::from(flag.n_dechptime.wrapping_mul(1000)) {
        ticks.dec_hp_tick = now;
        if i32::from(*hp) > flag.n_dechppoint {
            *hp = hp.wrapping_sub(flag.n_dechppoint as u16);
        } else {
            *hp = 0;
        }
        calls += 1;
    }
    if flag.bo_inchp && now - ticks.inc_hp_tick > i64::from(flag.n_inchptime.wrapping_mul(1000)) {
        ticks.inc_hp_tick = now;
        // **oracle bug 照抄**：判上限与加血都用 `nDECHPPOINT`（`:585/:587`），不是 `nINCHPPOINT`。
        if i32::from(*hp).wrapping_add(flag.n_dechppoint) < i32::from(max_hp) {
            *hp = hp.wrapping_add(flag.n_dechppoint as u16);
        } else {
            *hp = max_hp;
        }
        calls += 1;
    }
    calls
}

/// `PlayObject.DecHungerPointTick`（`PlayObject.cs:296`；ctor `:887` =
/// `HUtil32.GetTickCount()`）——**不是 0**：进图后 1s 才第一次触发。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HungerTicks {
    pub dec_hunger_point_tick: i64,
}

impl HungerTicks {
    #[must_use]
    pub fn new(now: i64) -> Self {
        Self {
            dec_hunger_point_tick: now,
        }
    }
}

/// P26 一次门内命中哪一支（决定宿主要不要 `RefMyStatus` / `HealthSpellChanged`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HungerOutcome {
    /// 未过门，或 `HungerSystem = false`。
    None,
    /// `HungerStatus > 0` 且 `GetMyStatus()` 跨档 ⇒ `RefMyStatus()`（Recalc + `RM_MYSTATUS`）。
    StatusChanged,
    /// `HungerStatus <= 0 && HungerDecHP` ⇒ `HealthSpellChanged()`。
    DecHealth,
}

/// P26：一次 `Run()` 的饥饿段。
///
/// 参数对应 oracle：
/// - `hunger_system` = `SystemShare.Config.HungerSystem`（总开关，`:596`）；
/// - `hunger_dec_hp` = `SystemShare.Config.HungerDecHP`（`:612`）；
/// - `hunger_status` = `PlayObject.HungerStatus`；
/// - `health_tick`/`spell_tick` = `BaseObject.HealthTick/SpellTick`（`i32`）；
/// - `per_health`/`per_spell` = `CharacterObject.PerHealth/PerSpell`（`u8`，回绕）。
///
/// **无 `!Death` 门**（死玩家照跑）。
#[allow(clippy::too_many_arguments)]
pub fn hunger_tick(
    hunger_system: bool,
    hunger_dec_hp: bool,
    hunger_status: &mut i32,
    hp: &mut u16,
    health_tick: &mut i32,
    spell_tick: &mut i32,
    per_health: &mut u8,
    per_spell: &mut u8,
    now: i64,
    ticks: &mut HungerTicks,
) -> HungerOutcome {
    if !hunger_system {
        return HungerOutcome::None;
    }
    // 严格 `>` 1000；过门即刷新。
    if now - ticks.dec_hunger_point_tick <= 1000 {
        return HungerOutcome::None;
    }
    ticks.dec_hunger_point_tick = now;
    if *hunger_status > 0 {
        let old = my_status(*hunger_status);
        *hunger_status -= 1;
        if old != my_status(*hunger_status) {
            return HungerOutcome::StatusChanged;
        }
        return HungerOutcome::None;
    }
    if !hunger_dec_hp {
        return HungerOutcome::None;
    }
    // 减少涨 HP / MP（oracle `:614-619`）。
    *health_tick -= 60;
    *spell_tick -= 10;
    *spell_tick = (*spell_tick).max(0);
    // C# `byte` compound assignment：0 ⇒ 255（unchecked 回绕）。
    *per_health = per_health.wrapping_sub(1);
    *per_spell = per_spell.wrapping_sub(1);
    if i32::from(*hp) > i32::from(*hp) / 100 {
        let dec = std::cmp::max(1, i32::from(*hp) / 100);
        *hp = hp.wrapping_sub(dec as u16);
    } else if *hp <= 2 {
        // oracle 的 else 只在 `HP == 0` 进入（正 HP 时 `HP > HP/100` 恒真），且仍置 0。
        *hp = 0;
    }
    HungerOutcome::DecHealth
}

/// `PlayObject.GetMyStatus()`（`PlayObject.Base.cs:1183-1191`）：`HungerStatus / 1000`，
/// 只有 `> 4 ⇒ 4` 的上限；**负数原样返回**（C# 整数除法向零截断）。
#[must_use]
pub fn my_status(hunger_status: i32) -> i32 {
    let result = hunger_status / 1000;
    if result > 4 {
        4
    } else {
        result
    }
}

/// P27 的逐玩家计时/倍率状态（`PlayObject.cs:720/724-727`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RateTimers {
    /// `KillMonExpRate`（ctor = 100）。
    pub kill_mon_exp_rate: i32,
    /// `KillMonExpRateTime`（无 ctor ⇒ 0）。
    pub kill_mon_exp_rate_time: i32,
    /// `PowerRate`（ctor = 100）。
    pub power_rate: i32,
    /// `PowerRateTime`（无 ctor ⇒ 0）。
    pub power_rate_time: i32,
    /// `ExpRateTick`（ctor = `GetTickCount()`；宿主用 `now_ms()` 初始化）。
    pub exp_rate_tick: i64,
}

impl RateTimers {
    /// oracle `PlayObject.cs:941-943` 的 ctor 初值（`ExpRateTick` 取构造时刻）。
    #[must_use]
    pub fn new(now: i64) -> Self {
        Self {
            kill_mon_exp_rate: 100,
            kill_mon_exp_rate_time: 0,
            power_rate: 100,
            power_rate_time: 0,
            exp_rate_tick: now,
        }
    }
}

/// P27 一次门内命中的恢复事件（帧序：经验 → 攻击，两条都发）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RateEvents {
    pub exp_recovered: bool,
    pub power_recovered: bool,
}

/// P27：一次 `Run()` 的倍率 1s 递减。
///
/// - 门严格 `> 1000`；
/// - **过门无条件刷新** `ExpRateTick`（即使两个 `Time` 都是 0）；
/// - 两个倒计时独立、每次最多减 1（`if` 非 `while`，不追帧）；
/// - `Time = 0` 是**永久倍率**。
pub fn rate_tick(now: i64, t: &mut RateTimers) -> RateEvents {
    let mut events = RateEvents::default();
    if now - t.exp_rate_tick <= 1000 {
        return events;
    }
    t.exp_rate_tick = now;
    if t.kill_mon_exp_rate_time > 0 {
        t.kill_mon_exp_rate_time -= 1;
        if t.kill_mon_exp_rate_time == 0 {
            t.kill_mon_exp_rate = 100;
            events.exp_recovered = true;
        }
    }
    if t.power_rate_time > 0 {
        t.power_rate_time -= 1;
        if t.power_rate_time == 0 {
            t.power_rate = 100;
            events.power_recovered = true;
        }
    }
    events
}

#[cfg(test)]
mod tests {
    use super::*;

    /// P25 `DECHP(1/10)`（**MapInfo 口径**：`(time/point)` ⇒ time=1、point=10）。
    fn dechp_flag(time: i32, point: i32) -> MapInfoFlag {
        MapInfoFlag {
            bo_dechp: true,
            n_dechptime: time,
            n_dechppoint: point,
            ..MapInfoFlag::default()
        }
    }

    fn inchp_flag(time: i32, point: i32) -> MapInfoFlag {
        MapInfoFlag {
            bo_inchp: true,
            n_inchptime: time,
            n_inchppoint: point,
            ..MapInfoFlag::default()
        }
    }

    /// P25-1：严格 `>` 门 + tick 刷新。
    #[test]
    fn p25_dechp_strict_gate_and_refresh() {
        let f = dechp_flag(1, 10);
        let mut hp = 100_u16;
        let mut t = MapHpTicks::default();
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 1001, &mut t), 1);
        assert_eq!(hp, 90);
        assert_eq!(t.dec_hp_tick, 1001);
        // elapsed == 1000 ⇒ 不触发（严格 >）。
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 2001, &mut t), 0);
        assert_eq!(hp, 90);
        assert_eq!(t.dec_hp_tick, 1001);
        // 1002 ⇒ 触发。
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 2002, &mut t), 1);
        assert_eq!(hp, 80);
    }

    /// P25-2/3：`HP <= point` ⇒ 清零（不是 `0-point` 回绕）。
    #[test]
    fn p25_dechp_clamps_to_zero_at_or_below_point() {
        let f = dechp_flag(1, 10);
        for hp0 in [10_u16, 5, 1] {
            let mut hp = hp0;
            let mut t = MapHpTicks::default();
            assert_eq!(map_hp_tick(&f, &mut hp, 100, 1001, &mut t), 1);
            assert_eq!(hp, 0, "HP={hp0} ⇒ 0");
        }
    }

    /// P25-4：ctor 初值 0 ⇒ 进图后的第一次调用（now ≫ 0）**立即**触发。
    #[test]
    fn p25_ticks_start_at_zero_so_first_run_fires() {
        let f = dechp_flag(1, 1);
        let mut hp = 50_u16;
        let mut t = MapHpTicks::default();
        assert_eq!(map_hp_tick(&f, &mut hp, 50, 5_000, &mut t), 1);
        assert_eq!(hp, 49);
    }

    /// **P25-6 核心牙（INCHP bug）**：`DECHP(1/10)+INCHP(1/7)`、HP=50/Max=100、两门都过
    /// ⇒ HP = **50**（40 + 10），不是 47（若“修好”成 `nINCHPPOINT`）。
    ///
    /// 牙：把 `map_hp_tick` 的 INCHP 支改成 `n_inchppoint` ⇒ 本用例必红（得 47）。
    #[test]
    fn p25_inchp_uses_dechp_point_bug() {
        let mut f = dechp_flag(1, 10);
        f.bo_inchp = true;
        f.n_inchptime = 1;
        f.n_inchppoint = 7;
        let mut hp = 50_u16;
        let mut t = MapHpTicks::default();
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 1001, &mut t), 2);
        assert_eq!(hp, 50, "DECHP 后 40，INCHP 用 nDECHPPOINT=10 回到 50");
    }

    /// P25-7：INCHP-only（`nDECHPPOINT` 默认 0）⇒ HP 不变，但**仍**上报一次
    /// `HealthSpellChanged`（无条件调用）。
    #[test]
    fn p25_inchp_only_is_noop_but_still_emits() {
        let f = inchp_flag(1, 7);
        let mut hp = 50_u16;
        let mut t = MapHpTicks::default();
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 1001, &mut t), 1);
        assert_eq!(hp, 50, "加 0 点");
        // 下一周期仍上报、HP 仍不变。
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 2002, &mut t), 1);
        assert_eq!(hp, 50);
    }

    /// P25-8：死玩家（HP=0）也可被 INCHP 抬到 MaxHP；本纯函数不看 `Death`
    /// （宿主层同样不拦；`Death` 的清除在 P32/`tick_tail`，不在本段）。
    #[test]
    fn p25_inchp_heals_dead_player_without_touching_death() {
        let f = inchp_flag(1, 7);
        let mut hp = 0_u16;
        let mut t = MapHpTicks::default();
        // INCHP 支的判据用 `nDECHPPOINT`（默认 0）⇒ 0 + 0 < 100 ⇒ 加 0；要抬到 Max 需
        // 同时有 DECHP 的 point（oracle 的实际数据形态）。这里显式构造双旗标：
        let mut f2 = f.clone();
        f2.bo_dechp = true;
        f2.n_dechptime = 1;
        f2.n_dechppoint = 100;
        assert_eq!(map_hp_tick(&f2, &mut hp, 100, 1001, &mut t), 2);
        // DECHP：0 > 100? 否 ⇒ HP=0；INCHP：0 + 100 < 100? 否 ⇒ HP=MaxHP。
        assert_eq!(hp, 100);
    }

    /// P25：`time < 0`（解析失败默认 -1）⇒ `time*1000 = -1000` ⇒ 门恒真（每个 tick 都触发）。
    #[test]
    fn p25_negative_time_makes_gate_always_true() {
        let f = dechp_flag(-1, 3);
        let mut hp = 100_u16;
        let mut t = MapHpTicks::default();
        for now in [1_i64, 2, 3] {
            assert_eq!(map_hp_tick(&f, &mut hp, 100, now, &mut t), 1, "now={now}");
        }
        assert_eq!(hp, 91);
    }

    /// P25：`ushort` 回绕的 point 截断（`(ushort)point`）照抄。
    ///
    /// `point = -1` ⇒ `(ushort)(-1) = 65535`；`int` 比较 `10 > -1` 为真 ⇒
    /// `hp.wrapping_sub(65535)`：10 - 65535 (mod 65536) = **11**（不是 10 - (-1) = 11 的巧合，
    /// 是截断后按模运算）。
    #[test]
    fn p25_point_cast_truncates_to_ushort() {
        let f = dechp_flag(1, -1);
        let mut hp = 10_u16;
        let mut t = MapHpTicks::default();
        assert_eq!(map_hp_tick(&f, &mut hp, 100, 1001, &mut t), 1);
        assert_eq!(hp, 11);
    }

    /// P26-1：`HungerStatus=1001` → 1000，状态不变 ⇒ 零事件。
    #[test]
    fn p26_status_decrement_without_band_change_is_silent() {
        let mut status = 1001;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                false,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!(status, 1000);
        assert_eq!((hp, ht, st, ph, ps), (100, 0, 0, 5, 5));
    }

    /// P26-2：`HungerStatus=1000` → 999：状态 1 → 0 ⇒ `StatusChanged`（RefMyStatus）。
    #[test]
    fn p26_band_crossing_reports_status_changed() {
        let mut status = 1000;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                false,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::StatusChanged
        );
        assert_eq!(status, 999);
        assert_eq!(my_status(status), 0);
    }

    /// P26-3：`HungerSystem=false` ⇒ 推进 10s 全不变。
    #[test]
    fn p26_disabled_is_full_noop() {
        let mut status = 4500;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (0_u8, 0_u8);
        let mut t = HungerTicks::new(0);
        for now in [1001_i64, 5001, 10_001] {
            assert_eq!(
                hunger_tick(
                    false,
                    true,
                    &mut status,
                    &mut hp,
                    &mut ht,
                    &mut st,
                    &mut ph,
                    &mut ps,
                    now,
                    &mut t
                ),
                HungerOutcome::None
            );
        }
        assert_eq!((status, hp, ht, st, ph, ps), (4500, 100, 0, 0, 0, 0));
        assert_eq!(t.dec_hunger_point_tick, 0, "未过门不刷新");
    }

    /// P26-4：`HungerSystem=1, HungerDecHP=0, HungerStatus=0` ⇒ 只跑门，无任何改动。
    #[test]
    fn p26_no_dec_hp_leaves_state_untouched() {
        let mut status = 0;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (7_u8, 8_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                false,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!((status, hp, ht, st, ph, ps), (0, 100, 0, 0, 7, 8));
        assert_eq!(t.dec_hunger_point_tick, 1001, "过门即刷新（即使无行为）");
    }

    /// P26-5：扣血支的逐字段。
    #[test]
    fn p26_dec_hp_branch_matches_oracle() {
        let mut status = 0;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 5_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                true,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::DecHealth
        );
        assert_eq!(hp, 99, "HP -= max(1, 100/100)");
        assert_eq!(ht, -60);
        assert_eq!(st, 0, "5-10 ⇒ max(0, -5)");
        assert_eq!(ph, 4);
        assert_eq!(ps, 4);
    }

    /// P26-6 牙：`PerHealth/PerSpell = 0` ⇒ `u8` 回绕到 255。
    ///
    /// 牙：用 `saturating_sub` ⇒ 断言红。
    #[test]
    fn p26_per_health_and_spell_wrap_at_zero() {
        let mut status = 0;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (0_u8, 0_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                true,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::DecHealth
        );
        assert_eq!((ph, ps), (255, 255));
    }

    /// P26-7 牙：死玩家（HP=0）照跑：体力点/Per* 照减，HP 保持 0（无 `!Death` 门）。
    #[test]
    fn p26_dead_player_still_decays() {
        let mut status = 0;
        let mut hp = 0_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                true,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::DecHealth
        );
        assert_eq!(hp, 0);
        assert_eq!(ht, -60);
        assert_eq!((ph, ps), (4, 4));
    }

    /// P26-8 牙：`HungerStatus = -1001` ⇒ `GetMyStatus() = -1`（向零截断，只有 >4 上限）；
    /// `HungerStatus` 无下限（1 → 0 → -1）。
    #[test]
    fn p26_negative_status_and_no_lower_bound() {
        assert_eq!(my_status(-1001), -1);
        assert_eq!(my_status(-999), 0);
        assert_eq!(my_status(4500), 4);
        assert_eq!(my_status(99_999), 4);

        let mut status = 1;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        // 1 → 0：两档都是 `GetMyStatus()=0`（1/1000 与 0/1000 都截断到 0）⇒ **不**跨档。
        assert_eq!(
            hunger_tick(
                true,
                false,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!(status, 0);
        // 下一周期：status 不再 > 0，走 else（dec_hp=false ⇒ 无行为），但不再减 status。
        assert_eq!(
            hunger_tick(
                true,
                false,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                2002,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!(status, 0);
    }

    /// P26-9：门严格 `>` 1000；`DecHungerPointTick` 只在触发时刷新。
    #[test]
    fn p26_gate_is_strictly_greater_than_1000() {
        let mut status = 5;
        let mut hp = 100_u16;
        let (mut ht, mut st) = (0_i32, 0_i32);
        let (mut ph, mut ps) = (5_u8, 5_u8);
        let mut t = HungerTicks::new(0);
        assert_eq!(
            hunger_tick(
                true,
                true,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1000,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!(status, 5);
        assert_eq!(t.dec_hunger_point_tick, 0);
        assert_eq!(
            hunger_tick(
                true,
                true,
                &mut status,
                &mut hp,
                &mut ht,
                &mut st,
                &mut ph,
                &mut ps,
                1001,
                &mut t
            ),
            HungerOutcome::None
        );
        assert_eq!(status, 4);
        assert_eq!(t.dec_hunger_point_tick, 1001);
    }

    /// P27-1：两段独立递减；到期各发一次事件，经验在前。
    #[test]
    fn p27_two_timers_decay_independently_exp_first() {
        let mut t = RateTimers::new(0);
        t.kill_mon_exp_rate = 200;
        t.kill_mon_exp_rate_time = 2;
        t.power_rate = 150;
        t.power_rate_time = 1;
        let e1 = rate_tick(1001, &mut t);
        assert_eq!(
            e1,
            RateEvents {
                exp_recovered: false,
                power_recovered: true
            }
        );
        assert_eq!((t.kill_mon_exp_rate, t.kill_mon_exp_rate_time), (200, 1));
        assert_eq!((t.power_rate, t.power_rate_time), (100, 0));
        let e2 = rate_tick(2002, &mut t);
        assert_eq!(
            e2,
            RateEvents {
                exp_recovered: true,
                power_recovered: false
            }
        );
        assert_eq!((t.kill_mon_exp_rate, t.kill_mon_exp_rate_time), (100, 0));
    }

    /// P27-3 牙：`Time = 0` ⇒ 永久（推进 10s 不变）。
    #[test]
    fn p27_time_zero_is_permanent() {
        let mut t = RateTimers::new(0);
        t.kill_mon_exp_rate = 200;
        t.power_rate = 300;
        for now in [1001_i64, 5001, 10_001] {
            assert_eq!(rate_tick(now, &mut t), RateEvents::default());
        }
        assert_eq!((t.kill_mon_exp_rate, t.kill_mon_exp_rate_time), (200, 0));
        assert_eq!((t.power_rate, t.power_rate_time), (300, 0));
    }

    /// P27-4：门严格 `>` 且 `ExpRateTick` **过门无条件刷新**（即使两个 Time 都是 0）。
    #[test]
    fn p27_gate_strict_and_tick_always_refreshed() {
        let mut t = RateTimers::new(0);
        assert_eq!(rate_tick(1000, &mut t), RateEvents::default());
        assert_eq!(t.exp_rate_tick, 0, "未过门不刷新");
        assert_eq!(rate_tick(1001, &mut t), RateEvents::default());
        assert_eq!(t.exp_rate_tick, 1001, "过门即刷新（无事件也刷新）");
    }

    /// P27-5 牙：一次跳 5001ms ⇒ 只减 1（`if` 非 `while`，不追帧）。
    #[test]
    fn p27_single_step_no_catch_up() {
        let mut t = RateTimers::new(0);
        t.kill_mon_exp_rate = 200;
        t.kill_mon_exp_rate_time = 10;
        assert_eq!(
            rate_tick(5001, &mut t),
            RateEvents {
                exp_recovered: false,
                power_recovered: false
            }
        );
        assert_eq!(t.kill_mon_exp_rate_time, 9);
    }
}
