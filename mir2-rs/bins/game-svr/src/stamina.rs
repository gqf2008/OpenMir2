//! oracle `BaseObject.Run()`（`src/M2Server/Actor/BaseObject.Base.cs:26-109`）的
//! **体力/魔力点（`HealthTick`/`SpellTick`）段**。
//!
//! # 为什么有这一片
//!
//! 真机隔离两相 `scoped_diff --canonical` 的唯一残留是攻方 `characters_ablity.Hp`
//! （C# `20668` / Rust `20669`）。C# 服务端权威逐击 dump 显示攻方 HP 只在**第 86 击**
//! 从 `20669` 掉到 `20668`，Rust 全程 `20669`（证据包
//! `mir2-rs/tests/parity/m3/evidence-m3-scoped-diff-ability-20261013/17-*.txt`）。
//! 逐行读 oracle 后，该扣血来自 `BaseObject.Run()` 的**体力点段**（本模块），而 Rust
//! 侧此前只在 `attackwire`/`movewire`/`main.rs` 的注释里**登记**「`HealthTick`/`SpellTick`
//! 未移植」⇒ 攻方体力点只减不回收，永远到不了 `-HealthFillTime`，所以不掉这 1 HP。
//!
//! # oracle 逐行（`BaseObject.Base.cs:26-109`）
//!
//! ```text
//! if (!Death) {                                              // :37
//!   recoveryTick = (GetTickCount() - AutoRecoveryTick) / 20  // :39
//!   AutoRecoveryTick = GetTickCount()                        // :40
//!   HealthTick += recoveryTick; SpellTick += recoveryTick    // :41-42
//!   if (HP < MaxHP && HealthTick >= HealthFillTime)          // :44-56
//!       HP += MaxHP/75 + 1（夹 MaxHP）; HealthSpellChanged()  // :46-55（帧 :55）
//!   if (MP < MaxMP && SpellTick >= SpellFillTime)            // :57-69
//!       MP += MaxMP/18 + 1（夹 MaxMP）; HealthSpellChanged()  // :59-68（帧 :68）
//!   if (HP == 0) Die()                                       // :70-73（本段不重复死亡结算）
//!   if (HealthTick >= HealthFillTime) HealthTick = 0         // :74-77
//!   if (SpellTick  >= SpellFillTime ) SpellTick  = 0         // :78-81
//! }
//! if (HealthTick < -HealthFillTime && HP > 1) {              // :105-110
//!   HP -= 1; HealthTick += HealthFillTime; HealthSpellChanged()  // :107-109（帧 :109）
//! }
//! ```
//!
//! # `HealthSpellChanged` 帧口径（本模块的 `changes` 出参）
//!
//! **一次 `Run()` 的 HP 支（`:55`）、MP 支（`:68`）、负值扣血支（`:109`）全部落在同一个
//! `base.Run` 批次里**：三次 `SendUpdateMsg(RM_HEALTHSPELLCHANGED, …)` 都只 `TryPeek` 队首并
//! 删除同 ident 旧消息（`BaseObject.Message.cs:283-305`）⇒ 同一 `Run` 的 53 每次替换前一条，
//! 队列里**至多一条 pending**；下一 `Run()` 的消息段（`PlayObject.Message.cs:358-366`）才出队。
//! ⇒ **每个 50ms `Run()` 至多一条帧**，不是「HP 支一条 + MP 支一条」。
//!
//! 载荷不随消息保存：`PlayObject.Message.cs:1736-1739` 在**出队时**读当前 `WAbil`
//! （逐字段 int → 线上 u16，若出队时已到终态则取终态）⇒ 本模块的 `changes` 按
//! **Run 粒度**记录该 Run 结束时的 `(WAbil.HP, WAbil.MP)`；调用方（`drain_stamina`）按序
//! 一个 Run 发一条帧。跨 Run 的 pending 本应在下一 Run 的 drain 发出（若下一 Run 的 P1/P2
//! 恢复先产生 53 则被替换）——宿主按 world tick 批量重放，**登记**为「帧相对 oracle 早
//! 0–50ms，且跨 tick 替换未建模」（R 级，真机窗口项）。
//!
//! # 扣体力的站点（都在本模块之外，由调用方作用到 [`Stamina`]）
//!
//! - 攻击成功：`HealthTick -= 30; SpellTick -= 100; SpellTick = max(0, SpellTick)`
//!   （`PlayObject.Attack.cs:955-957`）；
//! - 走路/跑步/骑跑成功：`HealthTick -= 10` / `-= 60`（`PlayObject.Attack.cs:1424` / `:1352`
//!   / `:1041`，跑步/骑跑另 `SpellTick -= 10`、`PerHealth/PerSpell -= 1`，见 `:1352-1356`/`:1041-1045`）；
//! - 受击（`RM_STRUCK`）：`HealthTick = 0; SpellTick = 0`（`PlayObject.Message.cs:1587-1588`）
//!   —— 这解释了为什么**受害者**全程不回血：每次被打都把体力点清零，永远到不了
//!   `HealthFillTime`。
//!
//! # 节奏口径（关键）
//!
//! oracle 的 `Run()` 由 `UserProcessor : TimerScheduledService(50ms)` 驱动
//! （`src/GameSrv/Word/Threads/UserProcessor.cs:7`），即**每 50ms 一次**；
//! `recoveryTick = (now - AutoRecoveryTick)/20` 是**整数除法**，每次调用丢掉不足 20ms 的余量
//! ⇒ 每 50ms 恰好 `+2`，长期回收率 **40/秒**（不是 50/秒）。本宿主的 world tick 是 200ms
//! （`mir2_world::TICK_INTERVAL_MS`）≠ 50ms ⇒ 若直接按 200ms 算 `(200/20)=10` 会得到 50/秒，
//! 攻方体力下滑变慢、掉血点漂移。因此这里**按 50ms 边界逐步重放** oracle 的 `Run()`，
//! 令整数截断口径与 oracle 一致。
//!
//! **死亡时不推进 `AutoRecoveryTick`**：oracle 的 `AutoRecoveryTick = GetTickCount()`（`:40`）在
//! `if (!Death)` 块内 ⇒ 死亡期间不更新；复活后第一次 `Run()` 的 `recoveryTick` 覆盖整段死亡时长
//! （oracle 原样）。本模块照抄：只有 `!death` 才推进 `auto_recovery_tick` 与 `health_tick/spell_tick`。
//!
//! # 登记（未建模，不静默）
//!
//! - `SuperMan`（GM 无敌）分支（`:31-35`）：未建模（本宿主无 GM 态）。
//! - `if (HP == 0) Die()`（`:65`）：死亡由 `crates/world` 的攻击结算负责，本段不重复结算。
//! - 目标清理/`LastHiter` 清理（`:111-131`）：属 world 侧，另有卡。

use crate::ability::Ability;

/// `UserProcessor : TimerScheduledService(TimeSpan.FromMilliseconds(50))`
/// （`src/GameSrv/Word/Threads/UserProcessor.cs:7`）——oracle `BaseObject.Run()` 的调用节奏。
pub const RUN_INTERVAL_MS: i64 = 50;

/// `recoveryTick = (GetTickCount() - AutoRecoveryTick) / 20`（`BaseObject.Base.cs:39`）的除数。
const RECOVERY_DIVISOR: i64 = 20;

/// `BaseObject.HealthTick` / `SpellTick` / `AutoRecoveryTick`（`BaseObject.cs:287/289/306`）。
///
/// `HealthTick`/`SpellTick` 是 `int`（oracle），`AutoRecoveryTick` 是 `int`（`GetTickCount()` ms）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stamina {
    /// `BaseObject.HealthTick`（`BaseObject.cs:287`）。
    pub health_tick: i32,
    /// `BaseObject.SpellTick`（`BaseObject.cs:289`）。
    pub spell_tick: i32,
    /// `BaseObject.AutoRecoveryTick`（`BaseObject.cs:306`；oracle `int`，本宿主 `i64`）。
    pub auto_recovery_tick: i64,
    /// 已重放到的 50ms 边界（本模块内部；仅用于按 [`RUN_INTERVAL_MS`] 重放 `Run()` 节奏，
    /// 不映射 oracle 的某个字段）。
    pub replay_cursor: i64,
}

impl Stamina {
    /// `CharacterObject()`/`BaseObject` 初值：`HealthTick = 0`、`SpellTick = 0`、
    /// `AutoRecoveryTick = GetTickCount()`（`BaseObject.cs:445-448`）。
    #[must_use]
    pub fn new(now: i64) -> Self {
        Self {
            health_tick: 0,
            spell_tick: 0,
            auto_recovery_tick: now,
            replay_cursor: now,
        }
    }

    /// 攻击成功（`PlayObject.Attack.cs:955-957`）：`HealthTick -= 30`、
    /// `SpellTick -= 100`、`SpellTick = max(0, SpellTick)`。
    pub fn on_attack_success(&mut self) {
        self.health_tick -= 30;
        self.spell_tick = (self.spell_tick - 100).max(0);
    }

    /// 走路成功（`PlayObject.Attack.cs:1424`）：`HealthTick -= 10`。
    pub fn on_walk_success(&mut self) {
        self.health_tick -= 10;
    }

    /// 跑步/骑跑成功（`PlayObject.Attack.cs:1352-1353` / `:1041-1042`）：
    /// `HealthTick -= 60`、`SpellTick -= 10`、`SpellTick = max(0, SpellTick)`。
    pub fn on_run_success(&mut self) {
        self.health_tick -= 60;
        self.spell_tick = (self.spell_tick - 10).max(0);
    }

    /// 受击（`PlayObject.Message.cs:1587-1588`）：`HealthTick = 0; SpellTick = 0`。
    pub fn on_struck(&mut self) {
        self.health_tick = 0;
        self.spell_tick = 0;
    }
}

/// `BaseObject.Run()`（`:26-109`）的体力/魔力点段，**按 [`RUN_INTERVAL_MS`] 重放**。
///
/// `changes` 会被**清空后重填**：**每个命中过 `HealthSpellChanged()` 的 `Run()` 压一条**
/// `(WAbil.HP, WAbil.MP)` 终态快照（同一 Run 内的 HP 支/MP 支/负值支共享一条 pending，
/// 见模块头）。调用方按序一 Run 发一帧 `SM_HEALTHSPELLCHANGED`。
/// 返回 = `changes.len()`（有 pending 的 `Run()` 数）。
/// `now` = 当轮 `HUtil32.GetTickCount()`（本宿主 [`super::now_ms`]，只有差值参与判定）。
#[must_use]
pub fn stamina_tick(
    st: &mut Stamina,
    wabil: &mut Ability,
    death: bool,
    now: i64,
    health_fill_time: i32,
    spell_fill_time: i32,
    changes: &mut Vec<(u16, u16)>,
) -> u32 {
    changes.clear();
    // 重放 oracle 的 `Run()` 调用：`UserProcessor` 每 50ms 一次；每次
    // `recoveryTick = (cursor - AutoRecoveryTick)/20`，随后 `AutoRecoveryTick = cursor`。
    // 整数截断按 50ms/次 ⇒ 每次 `+2`。world tick 200ms 时即 4 次 `Run()` ⇒ `+8`（40/秒）。
    while st.replay_cursor + RUN_INTERVAL_MS <= now {
        st.replay_cursor += RUN_INTERVAL_MS;
        // 本 `Run()` 是否调用过 `HealthSpellChanged()`（HP 支/MP 支/负值支共享一条 pending）。
        let mut run_changed = false;
        if !death {
            // `AutoRecoveryTick` 只在 `if (!Death)` 里更新（`:39-40`）⇒ 死亡期间冻结。
            let recovery_tick = (st.replay_cursor - st.auto_recovery_tick) / RECOVERY_DIVISOR;
            st.auto_recovery_tick = st.replay_cursor;
            let delta = clamp_i32(recovery_tick);
            st.health_tick = st.health_tick.wrapping_add(delta);
            st.spell_tick = st.spell_tick.wrapping_add(delta);
            // `if ((WAbil.HP < WAbil.MaxHP) && (HealthTick >= HealthFillTime))`（`:44`）
            if wabil.hp < wabil.max_hp && st.health_tick >= health_fill_time {
                let n18 = wabil.max_hp / 75 + 1; // `(ushort)((MaxHP / 75) + 1)`
                wabil.hp = if wabil.hp.saturating_add(n18) < wabil.max_hp {
                    wabil.hp + n18
                } else {
                    wabil.max_hp
                };
                run_changed = true; // `HealthSpellChanged()`（`:55`）
            }
            // `if ((WAbil.MP < WAbil.MaxMP) && (SpellTick >= SpellFillTime))`（`:52`）
            if wabil.mp < wabil.max_mp && st.spell_tick >= spell_fill_time {
                let n18 = wabil.max_mp / 18 + 1; // `(ushort)((MaxMP / 18) + 1)`
                wabil.mp = if wabil.mp.saturating_add(n18) < wabil.max_mp {
                    wabil.mp + n18
                } else {
                    wabil.max_mp
                };
                run_changed = true; // `HealthSpellChanged()`（`:68`）
            }
            // `:74-81`：到量清零（**在**负值扣血之前，照 oracle 次序）。
            if st.health_tick >= health_fill_time {
                st.health_tick = 0;
            }
            if st.spell_tick >= spell_fill_time {
                st.spell_tick = 0;
            }
        }
        // `if ((HealthTick < -HealthFillTime) && (WAbil.HP > 1))`（`:105-109`）
        if st.health_tick < -health_fill_time && wabil.hp > 1 {
            wabil.hp -= 1;
            st.health_tick += health_fill_time;
            run_changed = true; // `HealthSpellChanged()`（`:109`）
        }
        if run_changed {
            // oracle 在**下一次** drain 出队时才读 `WAbil` ⇒ 该 Run 的 pending 载荷 = 本 Run 终态。
            changes.push((wabil.hp, wabil.mp));
        }
    }
    u32::try_from(changes.len()).unwrap_or(u32::MAX)
}

/// `recoveryTick` 是 C# `int`（`BaseObject.Base.cs:39`）；本宿主用 `i64` 算差值，转回 `i32` 时**夹**
/// 到边界（正常 `Run()` 节奏下差值恒为 2，只有复活/长暂停才可能很大；夹取避免 debug 溢出 panic）。
fn clamp_i32(v: i64) -> i32 {
    i32::try_from(v).unwrap_or(if v > 0 { i32::MAX } else { i32::MIN })
}

#[cfg(test)]
mod tests {
    use super::{stamina_tick, Stamina, RUN_INTERVAL_MS};
    use crate::ability::Ability;

    fn wabil(hp: u16, mp: u16) -> Ability {
        Ability {
            level: 51,
            reserved1: 0,
            ac: 0,
            mac: 0,
            dc: 0,
            mc: 0,
            sc: 0,
            hp,
            mp,
            max_hp: 20669,
            max_mp: 900,
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

    /// 回归口径（本片判据）：**攻方体力点只减不回收是缺陷**——按 oracle 每 50ms `+2`（40/秒），
    /// 攻击扣 30。真机夹具（`-AttackPinPair`）攻方不移动、不受击，逐击走这条线：
    /// 累计到 `HealthTick < -HealthFillTime(300)` ⇒ `HP -= 1` 且 `HealthTick += 300`。
    ///
    /// 这里用「每击间隔 660ms、共 120 击」的构造。**660ms 是由证据反解出来的有效节奏**：
    /// C# 服务端权威逐击 dump（`evidence-m3-scoped-diff-ability-20261013/17-*.txt`）显示攻方 HP
    /// 只在**第 86 击**掉 1 点，而 oracle 每击固定扣 30 ⇒ 单次掉血、掉在 k=86 把
    /// `40*Δ - 30` 的净速率钉在 `≈ -3.5/击` ⇒ `Δ ≈ 662ms`。夹具的 `-AttackStepMs` 写的是 500ms，
    /// 但假人主循环是「每轮 `RobotPlayer.Run()`（客户端场景处理）+ `Thread.Sleep(50)`」⇒
    /// 实际两次 `CM_HIT` 的间隔 = 500ms 上界对齐到该轮边界（c2 侧实测约 660ms）。
    /// 本判据只依赖**「掉一次、终值 20668」**两个与节奏无关的硬结论 + 节奏区间，
    /// 真机两相复跑（要窗口）再做最终确认。
    #[test]
    fn attacker_loses_one_hp_when_health_tick_drops_below_fill_time() {
        let start = 3_000; // `-AttackStartDelayMs`
        let mut w = wabil(20669, 900);
        // 满血满蓝时 `stamina_tick` 不会回血（`HP < MaxHP` 为假），只会累积/扣减体力点。
        let mut st = Stamina::new(start);
        let step = 660; // 真机 bot 的有效步距（见上）
        let mut changes: Vec<(u16, u16)> = Vec::new();
        let mut drops = 0;
        for k in 0..120 {
            // 这一击前，把 elapsed 重放到该击时刻（模拟 world tick 逐轮推进）。
            let now = start + k * step;
            drops += stamina_tick(&mut st, &mut w, false, now, 300, 800, &mut changes);
            // 攻击成功：`HealthTick -= 30`（`PlayObject.Attack.cs:955`）。
            st.on_attack_success();
        }
        assert_eq!(
            drops, 1,
            "660ms 间隔 ⇒ 120 击内只触发一次 HealthTick<-300 的扣血"
        );
        assert_eq!(w.hp, 20668, "攻方终值 20669-1=20668，与真机 C# 行一致");
    }

    /// 回收口径钉死：**每 50ms `+2`（40/秒）**，不是 world tick 的 200ms `+10`（50/秒）。
    #[test]
    fn health_tick_recovers_two_per_fifty_ms() {
        let mut w = wabil(20669, 900);
        let mut st = Stamina::new(1_000);
        let mut changes = Vec::new();
        let _ = stamina_tick(&mut st, &mut w, false, 1_050, 300, 800, &mut changes);
        assert_eq!(
            st.health_tick, 2,
            "50ms 一次 Run() ⇒ +2（整数除法 floor(50/20)）"
        );
        let _ = stamina_tick(&mut st, &mut w, false, 1_200, 300, 800, &mut changes);
        assert_eq!(st.health_tick, 8, "150ms ⇒ 3 次 Run() ⇒ +6（累计 8）");
    }

    /// 改坏必红（牙齿）：删掉 `:105-109` 的负值扣血 ⇒ 低于 `-HealthFillTime` 也不再掉 HP。
    /// 这条用例锁的就是「回归口径」的下一步——把 `-1` 分支去掉必须让上面那条红。
    #[test]
    fn removing_the_negative_tick_branch_must_break_the_regression() {
        let mut w = wabil(20669, 900);
        let mut st = Stamina::new(0);
        // 手工把体力点压到 `-HealthFillTime` 以下（考虑本步 +2 回收后仍 < -300），再跑一次 Run()：oracle 应 `HP -= 1`。
        st.health_tick = -303;
        st.replay_cursor = 1_000;
        st.auto_recovery_tick = 1_000;
        let before = w.hp;
        let mut changes = Vec::new();
        let changed = stamina_tick(&mut st, &mut w, false, 1_050, 300, 800, &mut changes);
        assert_eq!(w.hp, before - 1, "HealthTick < -300 且 HP>1 ⇒ 扣 1 HP");
        assert_eq!(
            st.health_tick, -1,
            "随后 HealthTick += 300（-303+2+300=-1）"
        );
        assert_eq!(changed, 1, "该扣血调用 HealthSpellChanged() 一次");
        assert_eq!(
            changes,
            vec![(20668u16, 900u16)],
            "帧快照 = (扣血后 HP, 当前 MP)"
        );
    }

    /// 受害者口径（`:1587` 受击清零）：受击后 `HealthTick = 0`，回到满体力点前不会回血。
    #[test]
    fn struck_resets_stamina() {
        let mut st = Stamina::new(0);
        st.health_tick = 123;
        st.spell_tick = 45;
        st.on_struck();
        assert_eq!((st.health_tick, st.spell_tick), (0, 0));
    }

    /// 回收口径的**上界**：若把回收写成 world tick 200ms 的 `(200/20)=10`（50/秒），
    /// 660ms 间隔净 `-30+33=+3`/击 ⇒ 攻方**永不掉血**，与真机 20668 不等。
    /// 这条用「300ms 应累积 12（50ms×6 步 ×2 = 40/秒）」把两种口径钉开。
    #[test]
    fn recovery_is_fifty_ms_based_not_world_tick_based() {
        let mut w = wabil(20669, 900);
        let mut st = Stamina::new(0);
        let mut changes = Vec::new();
        let _ = stamina_tick(&mut st, &mut w, false, 300, 300, 800, &mut changes);
        assert_eq!(st.health_tick, 12, "300ms ⇒ 6 次 Run() × +2 = 12（40/秒）");
    }

    /// **C1**：一次 `Run()` 内 HP 支与 MP 支同时命中 ⇒ oracle 的两次 `SendUpdateMsg` 只保留
    /// 一条 pending（后到者覆盖）⇒ **恰 1 条帧**，载荷 = 该 Run 终态 `(HP+fill, MP+fill)`。
    ///
    /// 旧期望（「HP 支中间态 + MP 支终态 = 2 条」）把 `SendUpdateMsg` 的去重方向理解反了。
    ///
    /// 牙（改坏必红）：改回逐分支各发一条 ⇒ `n==2`（或 `changes.len()==2`）断言红。
    #[test]
    fn c1_one_run_with_hp_and_mp_branches_emits_one_terminal_snapshot() {
        let mut w = wabil(10, 10); // HP<MaxHP、MP<MaxMP ⇒ 两支都可命中
        w.max_hp = 100;
        w.max_mp = 100;
        let mut st = Stamina::new(0);
        st.health_tick = 300; // 恰好到 HP 阈值
        st.spell_tick = 800; // 恰好到 MP 阈值
        st.replay_cursor = 0;
        // `now=50` ⇒ 重放 1 次 Run()：`recovery_tick=(50-0)/20=2` ⇒ HT=302、ST=802，两支都命中。
        let mut changes = Vec::new();
        let n = stamina_tick(&mut st, &mut w, false, 50, 300, 800, &mut changes);
        assert_eq!(n, 1, "同一 Run 的 HP/MP 支共享一条 pending");
        // HP 支：HP += MaxHP/75+1 = 2 ⇒ 12；MP 支：MP += MaxMP/18+1 = 6 ⇒ 16（终态）。
        assert_eq!(changes, vec![(12u16, 16u16)], "载荷取 Run 终态");
        assert_eq!((w.hp, w.mp), (12, 16));
    }

    /// **C2**：一个 world tick（200ms）重放 **4 次** `Run()`，每次两支都命中 ⇒ 恰 **4 条**
    /// （每 Run 一条），载荷逐 Run 取终态、依次递增。
    ///
    /// 构造前提（**必须显式给**）：`fill_time <= 2`，否则 `:74-81` 在第一次命中后把
    /// `HealthTick/SpellTick` 清零，后续 Run 到不了阈值（构造不成立）。这里取 2。
    ///
    /// 牙（改坏必红）：把 4 个 Run 合并成 1 条 ⇒ 计数断言红；逐分支发 ⇒ 8 条断言红。
    #[test]
    fn c2_four_runs_emit_four_terminal_snapshots() {
        let mut w = wabil(10, 10);
        w.max_hp = 100;
        w.max_mp = 100;
        let mut st = Stamina::new(0);
        st.replay_cursor = 0;
        let mut changes = Vec::new();
        let n = stamina_tick(&mut st, &mut w, false, 200, 2, 2, &mut changes);
        assert_eq!(n, 4, "200ms ⇒ 4 次 Run()，每 Run 一条 pending");
        // HP 每 Run +2；MP 每 Run +6（100/18+1）。
        assert_eq!(
            changes,
            vec![(12, 16), (14, 22), (16, 28), (18, 34)],
            "逐 Run 终态（HP 支后 MP 支后的 WAbil）"
        );
        assert_eq!((w.hp, w.mp), (18, 34));
    }

    /// `if (!Death)` 内不更新 `AutoRecoveryTick`（`BaseObject.Base.cs:39-40`）：死亡期间冻结、
    /// `HealthTick/SpellTick` 不累积；复活后第一次 `Run()` 的 `recoveryTick` 覆盖整段死亡时长。
    #[test]
    fn death_freezes_auto_recovery_tick() {
        let mut w = wabil(50, 50);
        w.max_hp = 100;
        w.max_mp = 100;
        let mut st = Stamina::new(0);
        let mut changes = Vec::new();
        // 死亡 500ms（10 次 Run()）：`auto_recovery_tick` 不动、体力点不动。
        let _ = stamina_tick(&mut st, &mut w, true, 500, 300, 800, &mut changes);
        assert_eq!((st.auto_recovery_tick, st.health_tick), (0, 0));
        // 复活：第一次 Run() 的 recoveryTick = (550-0)/20 = 27 ⇒ HT=27（oracle 原样，整段补回）。
        let _ = stamina_tick(&mut st, &mut w, false, 550, 300, 800, &mut changes);
        assert_eq!(st.auto_recovery_tick, 550);
        assert_eq!(st.health_tick, 27, "复活后一次补回死亡期间 27 点");
    }

    #[test]
    fn run_interval_is_fifty_ms() {
        assert_eq!(RUN_INTERVAL_MS, 50);
    }
}
