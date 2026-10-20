//! **PL-4 / P22**：元宝/游戏点自动增减（`src/M2Server/Player/PlayObject.Message.cs:442-529`）。
//!
//! 纯逻辑与宿主（`SessionGround`/`World`/出帧）分离，便于构造级判据；宿主接线见
//! `main.rs::drain_player_game_gold`。`MoveToHome` 的落地在 `crates/world`
//! （[`mir2_world::World::base_object_move_player`]），本模块只回报「本 pass 触发了几次」。
//!
//! # 照抄的 oracle 形态（禁止“顺手修好”）
//!
//! - **四支严格按序**：① 玩家级 `DEC` → ② 玩家级 `INC` → ③ 地图 `DECGAMEGOLD` →
//!   ④ 地图 `INCGAMEGOLD`（`Message.cs:443/463/482/505`）。
//! - 四条**共用** `DecGameGoldTick` / `IncGameGoldTick`（不是每支一个 tick）。
//! - 门是**严格 `>`**（等于不触发）；`time * 1000` 是 C# `int` 乘法（unchecked）⇒ `wrapping_mul`。
//! - 玩家级 `INC` 用 `< 2,000,000`（等于即封顶**并清 flag**）；地图 `INC` 用 `<= 2,000,000`
//!   （等于即加上、**且永不清 flag**）。
//! - 地图 `INC` 的 `nInteger` 在赋值**之前**算（`:517`），与玩家级相反——两条日志都被注释
//!   ⇒ **无可观测差异**，本实现不建模 `nInteger`。
//! - `point` 可以 ≤ 0（ctor = 1；地图解析失败缺省 **-1**）：`GameGold >= -1` 恒真 ⇒
//!   `GameGold -= -1` 就是每周期 **+1**。负 point 按字面照抄，不 clamp。
//! - ①② 的 tick 刷新发生在**额度判断之前**：余额不足那一 pass 也刷新 tick。
//! - **`MoveToHome` 是副作用**：①/③ 的 else 支会移动玩家；oracle 里移动**立即**改 `Envir`
//!   ⇒ 同一 pass 后面的 ③/④ 读到的是**新图**的 `Envir.Flag`。宿主因此按阶段调用
//!   [`P22GameGold::auto_tick_player_dec`] / [`P22GameGold::auto_tick_player_inc`] /
//!   [`P22GameGold::auto_tick_map_dec`] / [`P22GameGold::auto_tick_map_inc`]，
//!   在两次 MoveToHome 之间**重新取 flag**。[`P22GameGold::auto_tick`] 是单 flag 的便捷入口
//!   （flag 不因移动而变时与宿主逐位等价；构造级判据用它）。
//!
//! # 登记差异
//!
//! - oracle 的 tick/时间字段是 `int`（`Environment.TickCount`，≈24.8 天回绕，差值可为负）；
//!   本实现用 `i64` 不回绕（沿用既有「时钟回绕」登记口径，不模拟回绕）。
//! - `M2Share.GameLogGameGold` 的两条 `AddEventLog` 在 oracle 里已注释 ⇒ 不建模任何日志字段。
//! - 地图支在无 `MapRegistry`（S1 占位模式）时按「无 flag」处理（`None`），**不**当作有 flag。

use mir2_envir::MapInfoFlag;

/// `PlayObject.GameGold` 的硬上限（`Message.cs:466/473/510/518`）。
pub const GAME_GOLD_CAP: i32 = 2_000_000;

/// `PlayObject` 的 P22 字段（`PlayObject.cs:763-776`；ctor `:964-973`）。
///
/// 不含 `GameGold` 本身——它住在 `SessionGround`（`sg.game_gold`），与 oracle 的
/// `PlayObject.GameGold` 同源（载入自 `characters.Gold`，见设计件 §1.3/§3.1）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct P22GameGold {
    /// `BoDecGameGold`（ctor `false`）。
    pub bo_dec_game_gold: bool,
    /// `DecGameGold`（ctor `1`）：每周期扣点数；可 ≤ 0（照抄）。
    pub dec_game_gold: i32,
    /// `DecGameGoldTime`（ctor `60_000`，毫秒）。
    pub dec_game_gold_time: i64,
    /// `DecGameGoldTick`（ctor `GetTickCount()`；换图/登录都不复位）。
    pub dec_game_gold_tick: i64,
    /// `BoIncGameGold`（ctor `false`）。
    pub bo_inc_game_gold: bool,
    /// `IncGameGold`（ctor `1`）。
    pub inc_game_gold: i32,
    /// `IncGameGoldTime`（ctor `60_000`，毫秒）。
    pub inc_game_gold_time: i64,
    /// `IncGameGoldTick`（ctor `GetTickCount()`；`UserLogon`/`EnterAnotherMap` 复位 = now）。
    pub inc_game_gold_tick: i64,
}

impl Default for P22GameGold {
    /// oracle ctor（`PlayObject.cs:964-973`）的**静态部分**：两个 tick 用 0。
    ///
    /// 生产构造必须用 [`P22GameGold::new`]（tick = 当时的 `GetTickCount()`）。
    fn default() -> Self {
        P22GameGold {
            bo_dec_game_gold: false,
            dec_game_gold: 1,
            dec_game_gold_time: 60_000,
            dec_game_gold_tick: 0,
            bo_inc_game_gold: false,
            inc_game_gold: 1,
            inc_game_gold_time: 60_000,
            inc_game_gold_tick: 0,
        }
    }
}

impl P22GameGold {
    /// oracle `new PlayObject()` 的 ctor 等价物：两个 tick 都 = 当前 `GetTickCount()`。
    #[must_use]
    pub fn new(now: i64) -> Self {
        P22GameGold {
            dec_game_gold_tick: now,
            inc_game_gold_tick: now,
            ..Self::default()
        }
    }

    /// ① 玩家级 `DEC`（`Message.cs:443-462`）。
    ///
    /// `GameGold >= DecGameGold` ⇒ 扣点；否则 `GameGold = 0; BoDecGameGold = false; MoveToHome()`。
    pub fn auto_tick_player_dec(&mut self, game_gold: i32, now: i64) -> StageOutcome {
        let mut gg = game_gold;
        let mut move_to_home = false;
        if self.bo_dec_game_gold && now - self.dec_game_gold_tick > self.dec_game_gold_time {
            self.dec_game_gold_tick = now;
            if gg >= self.dec_game_gold {
                gg = gg.wrapping_sub(self.dec_game_gold);
            } else {
                gg = 0;
                self.bo_dec_game_gold = false;
                move_to_home = true;
            }
        }
        StageOutcome {
            game_gold: gg,
            move_to_home,
        }
    }

    /// ② 玩家级 `INC`（`Message.cs:463-481`）。
    ///
    /// `GameGold + IncGameGold < 2,000,000` ⇒ 加；否则 `GameGold = 2,000,000;
    /// BoIncGameGold = false`（**精确等于上限也封顶并清 flag**）。
    pub fn auto_tick_player_inc(&mut self, game_gold: i32, now: i64) -> StageOutcome {
        let mut gg = game_gold;
        if self.bo_inc_game_gold && now - self.inc_game_gold_tick > self.inc_game_gold_time {
            self.inc_game_gold_tick = now;
            if gg.wrapping_add(self.inc_game_gold) < GAME_GOLD_CAP {
                gg = gg.wrapping_add(self.inc_game_gold);
            } else {
                gg = GAME_GOLD_CAP;
                self.bo_inc_game_gold = false;
            }
        }
        StageOutcome {
            game_gold: gg,
            move_to_home: false,
        }
    }

    /// ③ 地图 `DECGAMEGOLD`（`Message.cs:482-504`）。
    ///
    /// 门：`!BoDecGameGold && Envir.Flag.boDECGAMEGOLD`，周期 `nDECGAMEGOLDTIME * 1000`。
    /// 与 ① 共用 `DecGameGoldTick`。
    pub fn auto_tick_map_dec(
        &mut self,
        game_gold: i32,
        flag: Option<&MapInfoFlag>,
        now: i64,
    ) -> StageOutcome {
        let mut gg = game_gold;
        let mut move_to_home = false;
        if !self.bo_dec_game_gold {
            if let Some(f) = flag {
                if f.bo_decgamegold
                    && now - self.dec_game_gold_tick
                        > i64::from(f.n_decgamegoldtime.wrapping_mul(1000))
                {
                    self.dec_game_gold_tick = now;
                    if gg >= f.n_decgamegold {
                        gg = gg.wrapping_sub(f.n_decgamegold);
                    } else {
                        gg = 0;
                        self.bo_dec_game_gold = false;
                        move_to_home = true;
                    }
                }
            }
        }
        StageOutcome {
            game_gold: gg,
            move_to_home,
        }
    }

    /// ④ 地图 `INCGAMEGOLD`（`Message.cs:505-525`）。
    ///
    /// 门：`!BoIncGameGold && Envir.Flag.boINCGAMEGOLD`；`GameGold + point <= 2,000,000`
    /// 才加，否则置 2,000,000——**永不清 flag**（下一周期照进分支）。
    pub fn auto_tick_map_inc(
        &mut self,
        game_gold: i32,
        flag: Option<&MapInfoFlag>,
        now: i64,
    ) -> StageOutcome {
        let mut gg = game_gold;
        if !self.bo_inc_game_gold {
            if let Some(f) = flag {
                if f.bo_incgamegold
                    && now - self.inc_game_gold_tick
                        > i64::from(f.n_incgamegoldtime.wrapping_mul(1000))
                {
                    self.inc_game_gold_tick = now;
                    if gg.wrapping_add(f.n_incgamegold) <= GAME_GOLD_CAP {
                        gg = gg.wrapping_add(f.n_incgamegold);
                    } else {
                        gg = GAME_GOLD_CAP;
                    }
                }
            }
        }
        StageOutcome {
            game_gold: gg,
            move_to_home: false,
        }
    }

    /// 单 flag 便捷入口：四支按序跑一遍（`Message.cs:442-529`），返回**整段**结果。
    ///
    /// **只给构造级判据用**：宿主必须按阶段调用（见模块头），故生产路径不引用它。
    ///
    /// 用于「flag 不因 `MoveToHome` 而变」的构造级判据；宿主在有移动可能时用四个阶段函数
    /// 以便在两次移动之间重取 `Envir.Flag`（见模块头）。
    #[allow(dead_code)]
    pub fn auto_tick(
        &mut self,
        game_gold: i32,
        flag: Option<&MapInfoFlag>,
        now: i64,
    ) -> AutoTickOutcome {
        let t_obj_count = game_gold;
        let mut moved = 0_u8;
        let mut o = self.auto_tick_player_dec(game_gold, now);
        if o.move_to_home {
            moved += 1;
        }
        o = self.auto_tick_player_inc(o.game_gold, now);
        o = self.auto_tick_map_dec(o.game_gold, flag, now);
        if o.move_to_home {
            moved += 1;
        }
        o = self.auto_tick_map_inc(o.game_gold, flag, now);
        AutoTickOutcome {
            game_gold: o.game_gold,
            changed: t_obj_count != o.game_gold,
            move_to_home: moved,
        }
    }
}

/// 单个阶段的输出。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StageOutcome {
    pub game_gold: i32,
    /// 本阶段是否调用了 `MoveToHome()`（①/③ 的 else 支）。
    pub move_to_home: bool,
}

/// 整段（442-529）的输出（只由 [`P22GameGold::auto_tick`] 产生，生产路径不用）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[allow(dead_code)]
pub struct AutoTickOutcome {
    pub game_gold: i32,
    /// `tObjCount != GameGold` ⇒ 发一条 `RM_GOLDCHANGED`（→ `SM_GOLDCHANGED(653)`）。
    pub changed: bool,
    /// 本 pass `MoveToHome()` 的**次数**（0/1/2；2 只在「① 清 flag 后 ③ 的 `time<0`」病态组合）。
    pub move_to_home: u8,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag(dec: Option<(i32, i32)>, inc: Option<(i32, i32)>) -> MapInfoFlag {
        let mut f = MapInfoFlag::default();
        if let Some((point, time)) = dec {
            f.bo_decgamegold = true;
            f.n_decgamegold = point;
            f.n_decgamegoldtime = time;
        }
        if let Some((point, time)) = inc {
            f.bo_incgamegold = true;
            f.n_incgamegold = point;
            f.n_incgamegoldtime = time;
        }
        f
    }

    /// 判据 1（R）：玩家 DEC 的时钟与额度——严格 `>`、过门先刷 tick。
    ///
    /// 牙：把 `>` 改 `>=` ⇒ 1000ms 用例红；删 tick 刷新 ⇒ 第二次不扣红。
    #[test]
    fn p22_player_dec_strict_gate_and_tick_refresh() {
        let now = 10_000;
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 30;
        s.dec_game_gold_time = 1000;
        // `now - tick == time` ⇒ 不触发。
        let o = s.auto_tick_player_dec(100, now + 1000);
        assert_eq!(
            o,
            StageOutcome {
                game_gold: 100,
                move_to_home: false
            }
        );
        assert_eq!(s.dec_game_gold_tick, now);
        // 1001ms ⇒ 扣 30，tick = now。
        let o = s.auto_tick_player_dec(100, now + 1001);
        assert_eq!(o.game_gold, 70);
        assert_eq!(s.dec_game_gold_tick, now + 1001);
        // 再过 1001ms ⇒ 40。
        let o = s.auto_tick_player_dec(70, now + 2002);
        assert_eq!(o.game_gold, 40);
    }

    /// 判据 2（R）：玩家 DEC 不足 ⇒ 清零 + 清 flag + 恰好一次 MoveToHome。
    ///
    /// 牙：不清 flag ⇒ 第二次移动红；clamp 到 0 但保留 flag ⇒ 红。
    #[test]
    fn p22_player_dec_underflow_clears_flag_and_moves_once() {
        let now = 0;
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 30;
        s.dec_game_gold_time = 1000;
        let o = s.auto_tick_player_dec(10, now + 1001);
        assert_eq!(
            o,
            StageOutcome {
                game_gold: 0,
                move_to_home: true
            }
        );
        assert!(!s.bo_dec_game_gold);
        // 后续周期不再移动/不再扣。
        let o = s.auto_tick_player_dec(0, now + 20_000);
        assert_eq!(
            o,
            StageOutcome {
                game_gold: 0,
                move_to_home: false
            }
        );
    }

    /// 判据 3（R）：玩家 INC 封顶的严格边界（`< 2,000,000`，等于即封顶并清 flag）。
    ///
    /// 牙：`<` ↔ `<=` 互换 ⇒ 两个用例各红一个。
    #[test]
    fn p22_player_inc_cap_boundary() {
        let now = 0;
        // 恰好到上限 ⇒ 封顶 + 清 flag。
        let mut s = P22GameGold::new(now);
        s.bo_inc_game_gold = true;
        s.inc_game_gold = 100;
        s.inc_game_gold_time = 1000;
        let o = s.auto_tick_player_inc(1_999_900, now + 1001);
        assert_eq!(o.game_gold, GAME_GOLD_CAP);
        assert!(!s.bo_inc_game_gold);
        // 差 1 到上限 ⇒ 正常加，flag 保留。
        let mut s = P22GameGold::new(now);
        s.bo_inc_game_gold = true;
        s.inc_game_gold = 100;
        s.inc_game_gold_time = 1000;
        let o = s.auto_tick_player_inc(1_999_899, now + 1001);
        assert_eq!(o.game_gold, 1_999_999);
        assert!(s.bo_inc_game_gold);
    }

    /// 判据 4（R）：地图 DEC 与共享 tick——玩家级先跑并刷新 tick ⇒ 地图支同 pass 不再扣。
    ///
    /// 牙：给地图支独立 tick 字段 ⇒ 第二条红。
    #[test]
    fn p22_map_dec_shares_tick_with_player_dec() {
        let now = 10_000;
        let f = flag(Some((30, 1)), None);
        let mut s = P22GameGold::new(now);
        s.dec_game_gold_tick = now - 1001;
        let o = s.auto_tick_map_dec(100, Some(&f), now);
        assert_eq!(o.game_gold, 70);
        assert_eq!(s.dec_game_gold_tick, now);

        // 玩家级先跑（tick = now-5000）⇒ 扣 30 并把 tick 刷成 now；地图支不再扣。
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 30;
        s.dec_game_gold_time = 1000;
        s.dec_game_gold_tick = now - 5000;
        let o = s.auto_tick_player_dec(100, now);
        assert_eq!(o.game_gold, 70);
        let o = s.auto_tick_map_dec(70, Some(&f), now);
        assert_eq!(o.game_gold, 70, "共享 tick ⇒ 同 pass 地图支不再扣");
    }

    /// 判据 5（R）：地图 INC 不清 flag + 负 point 可观测。
    ///
    /// 牙：照抄玩家级清 flag ⇒ 负 point 只减一次，红。
    #[test]
    fn p22_map_inc_never_clears_flag_and_negative_point() {
        let now = 0;
        let f = flag(None, Some((-1, 1)));
        let mut s = P22GameGold::new(now - 1001);
        let mut gg = 100;
        for expected in [99, 98, 97] {
            let o = s.auto_tick_map_inc(gg, Some(&f), s.inc_game_gold_tick + 1001);
            gg = o.game_gold;
            assert_eq!(gg, expected, "负 point 每周期都重跑");
            assert!(!s.bo_inc_game_gold);
        }
        // 封顶支：`<= 2,000,000` 时照加；超限置上限，flag 仍 false（下一周期照进）。
        let f = flag(None, Some((200, 1)));
        let mut s = P22GameGold::new(now);
        s.inc_game_gold_tick = now - 1001;
        let o = s.auto_tick_map_inc(1_999_900, Some(&f), now);
        assert_eq!(o.game_gold, GAME_GOLD_CAP);
        // 上限后下一周期：`2,000,000 + 200 > 2,000,000` ⇒ 仍置上限、tick 刷新（值不变）。
        let o = s.auto_tick_map_inc(GAME_GOLD_CAP, Some(&f), now + 1001);
        assert_eq!(o.game_gold, GAME_GOLD_CAP);
        assert_eq!(s.inc_game_gold_tick, now + 1001);
    }

    /// 判据 12（R）：ctor/换图 tick 相位。
    ///
    /// `new(now)` ⇒ 两个 tick 都 = now；`EnterAnotherMap` 只复位 `inc_tick`（由宿主调用点写）；
    /// `SpaceMove` 两者都不复位。
    #[test]
    fn p22_ctor_ticks() {
        let s = P22GameGold::new(1234);
        assert_eq!(s.dec_game_gold_tick, 1234);
        assert_eq!(s.inc_game_gold_tick, 1234);
        assert!(!s.bo_dec_game_gold);
        assert!(!s.bo_inc_game_gold);
        assert_eq!((s.dec_game_gold, s.dec_game_gold_time), (1, 60_000));
        assert_eq!((s.inc_game_gold, s.inc_game_gold_time), (1, 60_000));
        // Default 的 tick 是 0（仅测试字面量用）。
        assert_eq!(P22GameGold::default().dec_game_gold_tick, 0);
    }

    /// 判据 2/4 补充：地图 DEC 的 `time < 0` ⇒ ① 清 flag 后同 pass ③ 再次 MoveToHome
    /// （病态组合按字面允许；①② 的 flag 语义要照抄——① 成功时 `BoDec` 仍真，③ 根本进不去）。
    #[test]
    fn p22_two_move_to_home_in_one_pass() {
        let now = 10_000;
        let f = flag(Some((30, -1)), None);
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 30;
        s.dec_game_gold_time = 1000;
        s.dec_game_gold_tick = now - 5000;
        let o = s.auto_tick(100, Some(&f), now);
        // ① 扣 30 并刷 tick=now，但**不清 flag** ⇒ ③ 的 `!BoDecGameGold` 门为假 ⇒ 地图支不进。
        assert_eq!(o.game_gold, 70);
        assert_eq!(o.move_to_home, 0, "① 成功 ⇒ bo_dec 仍真 ⇒ ③ 不进入");
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 200;
        s.dec_game_gold_time = 1000;
        s.dec_game_gold_tick = now - 5000;
        let o = s.auto_tick(100, Some(&f), now);
        // ① 余额不足 ⇒ 0 + 清 flag + MoveToHome；③ 门 `!BoDec` 成立、time<0 ⇒ 再次 MoveToHome。
        assert_eq!(o.game_gold, 0);
        assert_eq!(o.move_to_home, 2);
    }

    /// 无 flag（S1 占位/地图未登记）⇒ 地图两支恒不动，玩家级照跑。
    #[test]
    fn p22_no_flag_only_player_branches() {
        let now = 0;
        let mut s = P22GameGold::new(now);
        s.bo_dec_game_gold = true;
        s.dec_game_gold = 10;
        s.dec_game_gold_time = 1000;
        let o = s.auto_tick(100, None, now + 1001);
        assert_eq!(o.game_gold, 90);
        assert!(o.changed);
        assert_eq!(o.move_to_home, 0);
    }
}
