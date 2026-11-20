//! **IT-16 彩票**（StdMode 3 / Shape 11）：`WinLottery()` 的宿主级状态与纯决策。
//!
//! oracle：`PlayObject.Base.cs:2298` 的 `EatUseItems` **case 11** → `WinLottery()`
//! （`PlayObject.cs:3809`）。**物品恒被吃**（`case 11: WinLottery(); result=true;`），
//! 与中/不中无关（卡 §9.11「恒吃单列断言」）。
//!
//! # 作用域（卡 §9.11「架构决议」）
//!
//! `WinLotteryCount / NoWinLotteryCount / WinLotteryLevel1..6` 在 oracle 是
//! `SystemShare.Config` 上的**进程级可变静态**。本仓 `World` 是 per-World 对象
//! （分片设计「同进程多 World」，`MIR2_SHARDS` 默认 1）⇒ 挂 `World` 会在 N>1 时变成
//! 每片独立计数、偏离 oracle。⇒ 这组计数放在这里，经 [`WinLotteryShared`]
//! （`Arc<Mutex<…>>`）在 `run_inner` 创建、贯穿 `sim_loop` / `handle_eat_cmd`，**不挂 World**。
//! 它是设计件「World 间无共享可变状态」在**游戏状态层**的**唯一显式例外**；RNG 仍走 `world.rng`。
//! （gate 层另有进程级共享 `TOMBSTONED_GATE_SOCKETS`，基础设施、非游戏状态，见
//! `openmir2-world-keeps-dead-session`。）
//! **不用进程 static**：并行单测会互相污染（static 只适合启动后只读的配置）。
//!
//! # 分层
//!
//! [`win_lottery`] 只做「抽一次 + 6 段匹配 + 闸」，返回 [`WinLotteryOutcome`]；
//! 加锁、写金、发帧由宿主（`handle_eat_cmd`）负责。

use std::sync::{Arc, Mutex};

use mir2_shared::rng::RandomSource;

use crate::config::WinLotteryCfg;

/// oracle `SystemShare.Config` 上那 8 个**可变**计数（`WinLotteryCount` /
/// `NoWinLotteryCount` / `WinLotteryLevel1..6`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WinLotteryState {
    /// `Config.WinLotteryCount`。oracle **全仓无运行期写点**（只有 `[Setup]` 装载与启动清零）
    /// ⇒ 本结构保留字段只为与 oracle 同形与 GM 显示面，`win_lottery` 只读。
    pub count: i32,
    /// `Config.NoWinLotteryCount`：每次未中（含闸闭）`+= 500`。
    pub no_win: i32,
    /// `Config.WinLotteryLevel1..6`：`level[0]` = 一等奖 … `level[5]` = 六等奖。
    pub level: [i32; 6],
}

/// **进程级共享句柄**（`Arc<Mutex<WinLotteryState>>`）。
///
/// 创建点 = `run_inner`（与 `ids_online` / `world_start` 同层），`clone` 进各 sim 线程；
/// N=1（默认）时与 oracle 的进程级静态语义逐字节等价。
#[derive(Clone, Debug)]
pub struct WinLotteryShared(Arc<Mutex<WinLotteryState>>);

impl WinLotteryShared {
    /// 以启动初值（`[Setup]` 的 8 个状态键，见 [`WinLotteryCfg::init_state`]）创建。
    #[must_use]
    pub fn new(init: WinLotteryState) -> Self {
        Self(Arc::new(Mutex::new(init)))
    }

    /// 持锁执行 `f`。锁中毒（他人在持锁时 panic）按 `into_inner` 继续——
    /// 这是**场景状态**不是不变量检查点，并行单测下不该因别人的 panic 全挂。
    pub fn with<R>(&self, f: impl FnOnce(&mut WinLotteryState) -> R) -> R {
        let mut guard = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        f(&mut guard)
    }

    /// per-case 复位（探针/单测同口径；oracle 侧即「每个 lottery case 前显式置初值」）。
    #[cfg(test)]
    pub fn reset(&self, init: WinLotteryState) {
        self.with(|st| *st = init);
    }

    /// 当前快照（GM 显示面 / 证据用；生产消费者随 follow-up 卡 `openmir2-lottery-ticket-gm-realtime`）。
    #[cfg(test)]
    #[must_use]
    pub fn snapshot(&self) -> WinLotteryState {
        self.with(|st| *st)
    }
}

impl WinLotteryCfg {
    /// **配置 → 运行时状态的唯一入口**：`[Setup]` 的 8 个状态键逐字段映射。
    ///
    /// oracle 口径：`GameApp.InitializeSystem()` 的清零在 `GameShare.LoadConfig()` **之前**，
    /// 故**有效初值 = 配置值**（部署全 0）。见卡 §9.11「启动初值来自配置」。
    #[must_use]
    pub fn init_state(&self) -> WinLotteryState {
        WinLotteryState {
            count: self.init_count,
            no_win: self.init_no_win,
            level: self.init_level,
        }
    }
}

/// `WinLottery()` 的决策结果（纯函数输出，不含帧/金币写入）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinLotteryOutcome {
    /// 中奖。oracle 判据是 **`nGold > 0`**（`PlayObject.cs:3866`），**不是** `nWinLevel > 0`
    /// ——配置把某档 `Gold` 写成 0 时，oracle 仍走「未中」分支。
    Won {
        /// 中奖档位 1..=6（与 `WinLotteryLevelN` 的 N 同义）。
        level: u8,
        /// `WinLotteryNGold`。
        gold: i32,
    },
    /// 未中**或闸闭**（`Count < NoWinLotteryCount` 取假）：`NoWinLotteryCount += 500`。
    Lost,
}

/// 中奖/未中时宿主应入队的通知（顺序 = oracle 消息泵序）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinLotteryNotice {
    /// `RM_GOLDCHANGED`（10136）：仅 `IncGold` 成功时入队。
    GoldChanged,
    /// `RM_SYSMESSAGE` + 绿色（`MsgColor.Green`）：报喜。
    SysMsgGreen,
    /// `RM_SYSMESSAGE` + 红色（`MsgColor.Red`）：未中/闸闭。
    SysMsgRed,
}

impl WinLotteryNotice {
    /// 对应的 `RM_*` ident（与 `useitem` 的常量同源）。
    #[must_use]
    pub fn ident(self) -> i32 {
        match self {
            Self::GoldChanged => crate::useitem::RM_GOLDCHANGED,
            Self::SysMsgGreen | Self::SysMsgRed => crate::useitem::RM_SYSMESSAGE,
        }
    }
}

/// oracle `PlayObject.cs:3867-3903` 的通知入队序——**宿主必须按此顺序 push**，
/// 因为消息泵按 `(byte)ident` 升序出队：`RM_GOLDCHANGED`(10136→152) 先于
/// `RM_SYSMESSAGE`(11004→252)。
///
/// **绿字在 `IncGold` 之前发**（oracle bug：先报喜、后加钱）⇒ `IncGold` 失败时
/// 仍只有绿字、没有 `SM_GOLDCHANGED`（卡 §9.11「oracle bug，为 parity 复制」）。
#[must_use]
pub fn win_notices(outcome: WinLotteryOutcome, inc_gold_ok: bool) -> Vec<WinLotteryNotice> {
    match outcome {
        WinLotteryOutcome::Won { .. } => {
            let mut v = Vec::with_capacity(2);
            if inc_gold_ok {
                v.push(WinLotteryNotice::GoldChanged);
            }
            v.push(WinLotteryNotice::SysMsgGreen);
            v
        }
        WinLotteryOutcome::Lost => vec![WinLotteryNotice::SysMsgRed],
    }
}

/// `PlayObject.WinLottery()`（`PlayObject.cs:3809-3905`）的纯决策半。
///
/// 1. `nRate = Random(Config.WinLotteryRate)`（值域 `[0, rate)`）；
/// 2. 按 **6→5→4→3→2→1** 的 if/else-if 链求值：档 6..2 是闭区间，**档 1 是「和式等值」**
///    （`WinLottery1Min + WinLottery1Max == nRate`，不是区间）；命中某档后**不再看后续档**
///    （闸闭也一样停住，不会落到别的档）；
/// 3. 命中且 `count < no_win` ⇒ 该档 `level += 1`、`nGold = gold`；否则 `nGold` 保持 0；
/// 4. `nGold > 0` ⇒ 中奖；否则 `no_win += 500`（未中/闸闭同一条路）。
#[must_use]
pub fn win_lottery(
    rng: &mut impl RandomSource,
    cfg: &WinLotteryCfg,
    st: &mut WinLotteryState,
) -> WinLotteryOutcome {
    let rate = rng.random_below(cfg.rate);
    let mut gold = 0i32;
    let mut level = 0u8;
    for lv in (1u8..=6).rev() {
        let i = usize::from(lv - 1);
        let hit = if lv == 1 {
            // oracle `:3859`：`WinLottery1Min + WinLottery1Max == nRate`（和式等值）。
            // int 加法在 oracle 是无符号检查的，这里用 `wrapping_add` 对齐（不 panic）。
            cfg.min[i].wrapping_add(cfg.max[i]) == rate
        } else {
            rate >= cfg.min[i] && rate <= cfg.max[i]
        };
        if hit {
            if st.count < st.no_win {
                gold = cfg.gold[i];
                level = lv;
                st.level[i] = st.level[i].wrapping_add(1);
            }
            break;
        }
    }
    if gold > 0 {
        WinLotteryOutcome::Won { level, gold }
    } else {
        st.no_win = st.no_win.wrapping_add(500);
        WinLotteryOutcome::Lost
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 固定取值的随机源：`random_below` 恒返回 `value`（其余入口不用）。
    struct FixedRng(i32);
    impl RandomSource for FixedRng {
        fn random(&mut self) -> i32 {
            self.0
        }
        fn random_below(&mut self, _value: i32) -> i32 {
            self.0
        }
        fn random_range(&mut self, _min_value: i32, _max_value: i32) -> i32 {
            self.0
        }
        fn next_double(&mut self) -> f64 {
            f64::from(self.0)
        }
    }

    /// 部署参数（`Server.conf` / `GameSvrConf.cs:1669-1688` 同值）。
    fn deploy_cfg() -> WinLotteryCfg {
        WinLotteryCfg::default()
    }

    /// 开闸的初值（`count=0 < no_win=500`）。
    fn open_state() -> WinLotteryState {
        WinLotteryState {
            count: 0,
            no_win: 500,
            level: [0; 6],
        }
    }

    #[test]
    fn deploy_defaults_match_oracle_ctor() {
        let c = deploy_cfg();
        assert_eq!(c.rate, 30_000);
        assert_eq!(c.min, [16_180, 16_170, 16_150, 16_000, 14_000, 1]);
        assert_eq!(c.max, [16_185, 16_179, 16_169, 16_149, 15_999, 4_999]);
        assert_eq!(c.gold, [1_000_000, 200_000, 100_000, 10_000, 1_000, 500]);
        assert_eq!(c.init_state(), WinLotteryState::default());
    }

    #[test]
    fn level1_is_sum_equality_not_range() {
        let c = deploy_cfg();
        // 部署下 1Min+1Max = 32365 ≥ rate=30000 ⇒ 恒不可达。
        let mut st = open_state();
        assert_eq!(
            win_lottery(&mut FixedRng(16_180), &c, &mut st),
            WinLotteryOutcome::Lost,
            "1 等奖不是区间：nRate=1Min 不应中奖"
        );
        // 覆盖成 5000/8000 ⇒ 和 13000：只有 13000 中 1 等奖。
        let mut c2 = c;
        c2.min[0] = 5_000;
        c2.max[0] = 8_000;
        let mut st = open_state();
        assert_eq!(
            win_lottery(&mut FixedRng(13_000), &c2, &mut st),
            WinLotteryOutcome::Won {
                level: 1,
                gold: 1_000_000
            }
        );
        assert_eq!(st.level, [1, 0, 0, 0, 0, 0]);
        let mut st = open_state();
        assert_eq!(
            win_lottery(&mut FixedRng(12_999), &c2, &mut st),
            WinLotteryOutcome::Lost
        );
        assert_eq!(
            win_lottery(&mut FixedRng(13_001), &c2, &mut st),
            WinLotteryOutcome::Lost
        );
    }

    #[test]
    fn bands_are_checked_six_first_then_down() {
        // 6 档 [1,4999] 与 5 档 [14000,15999] 不重叠；把 6 档扩到覆盖 15000 后，
        // 15000 应判 6 等（链首命中即停），而不是 5 等。
        let mut c = deploy_cfg();
        c.max[5] = 20_000;
        let mut st = open_state();
        assert_eq!(
            win_lottery(&mut FixedRng(15_000), &c, &mut st),
            WinLotteryOutcome::Won {
                level: 6,
                gold: 500
            }
        );
        assert_eq!(st.level, [0, 0, 0, 0, 0, 1]);
    }

    #[test]
    fn gate_closed_stops_at_matched_band_and_counts_as_lost() {
        // 档命中但 count >= no_win ⇒ 不落到别的档、不升 level，走「未中」：no_win += 500。
        let c = deploy_cfg();
        let mut st = WinLotteryState {
            count: 0,
            no_win: 0,
            level: [0; 6],
        };
        assert_eq!(
            win_lottery(&mut FixedRng(500), &c, &mut st),
            WinLotteryOutcome::Lost
        );
        assert_eq!(st.level, [0; 6]);
        assert_eq!(st.no_win, 500);
    }

    #[test]
    fn zero_gold_band_is_lost_not_won() {
        // oracle 判据是 `nGold > 0`：某一等奖金配成 0 时，即便档命中且过闸也走「未中」，
        // 但 `WinLotteryLevelN++` 已发生（递增在 IncGold 之前）。
        let mut c = deploy_cfg();
        c.max[5] = 20_000;
        c.gold[5] = 0;
        let mut st = open_state();
        assert_eq!(
            win_lottery(&mut FixedRng(15_000), &c, &mut st),
            WinLotteryOutcome::Lost
        );
        assert_eq!(st.level, [0, 0, 0, 0, 0, 1]);
        assert_eq!(st.no_win, 1_000);
    }

    #[test]
    fn init_state_maps_all_eight_keys_without_shift() {
        // 逐键映射牙：8 个键取互不相同的值；任何漏映/错位（尤其 1↔6 等奖交换）都会改变数组。
        let mut c = deploy_cfg();
        c.init_count = 11;
        c.init_no_win = 22;
        c.init_level = [1, 2, 3, 4, 5, 6];
        assert_eq!(
            c.init_state(),
            WinLotteryState {
                count: 11,
                no_win: 22,
                level: [1, 2, 3, 4, 5, 6],
            }
        );
    }

    #[test]
    fn pure_decision_reads_state_not_cfg_init() {
        // 「运行时不许读 cfg.init_*」牙：初值给极端值，运行时状态另给一套；
        // 闸判定/结果必须只随 state 变（实现改成读 cfg.init_* ⇒ 本用例红）。
        let mut c = deploy_cfg();
        c.init_count = 100;
        c.init_no_win = 0; // 若误读初值 ⇒ 闸闭
        let mut st = open_state(); // 真闸开
        assert_eq!(
            win_lottery(&mut FixedRng(500), &c, &mut st),
            WinLotteryOutcome::Won {
                level: 6,
                gold: 500
            }
        );
        c.init_count = 0;
        c.init_no_win = 1000;
        let mut st = WinLotteryState {
            count: 5,
            no_win: 5, // 真闸闭
            level: [0; 6],
        };
        assert_eq!(
            win_lottery(&mut FixedRng(500), &c, &mut st),
            WinLotteryOutcome::Lost
        );
    }

    #[test]
    fn shared_reset_and_snapshot_roundtrip() {
        // 进程级句柄的 per-case 复位面（宿主/探针共用；`reset` 是唯一复位入口）。
        let sh = WinLotteryShared::new(WinLotteryState::default());
        assert_eq!(sh.snapshot(), WinLotteryState::default());
        sh.reset(WinLotteryState {
            count: 1,
            no_win: 2,
            level: [3, 4, 5, 6, 7, 8],
        });
        assert_eq!(sh.snapshot().level, [3, 4, 5, 6, 7, 8]);
        assert_eq!(sh.snapshot().no_win, 2);
    }

    #[test]
    fn notice_order_is_pump_priority_ascending() {
        // 主断言（性质断言，不钉两元素字面量）：中奖且 IncGold 成功 ⇒ 金币帧先于红/绿字。
        let ids: Vec<i32> = win_notices(
            WinLotteryOutcome::Won {
                level: 6,
                gold: 500,
            },
            true,
        )
        .iter()
        .map(|n| n.ident())
        .collect();
        assert_eq!(ids, vec![10_136, 11_004], "中奖：GOLDCHANGED 后 SYSMESSAGE");
        let prio: Vec<u8> = ids.iter().map(|i| *i as u8).collect();
        assert!(
            prio.windows(2).all(|w| w[0] < w[1]),
            "泵按 (byte)ident 升序：{prio:?}"
        );
        // IncGold 失败 ⇒ 只有绿字（oracle bug 复制）。
        let ids: Vec<i32> = win_notices(
            WinLotteryOutcome::Won {
                level: 6,
                gold: 500,
            },
            false,
        )
        .iter()
        .map(|n| n.ident())
        .collect();
        assert_eq!(ids, vec![11_004], "IncGold 失败：只有绿字");
        // 未中/闸闭 ⇒ 只有红字。
        assert_eq!(
            win_notices(WinLotteryOutcome::Lost, false)
                .iter()
                .map(|n| n.ident())
                .collect::<Vec<_>>(),
            vec![11_004]
        );
    }

    #[test]
    fn boundary_is_inclusive_on_both_ends() {
        let mut c = deploy_cfg();
        c.min[5] = 100;
        c.max[5] = 200;
        for (rate, won) in [(99, false), (100, true), (200, true), (201, false)] {
            let mut st = open_state();
            let out = win_lottery(&mut FixedRng(rate), &c, &mut st);
            assert_eq!(
                matches!(out, WinLotteryOutcome::Won { level: 6, .. }),
                won,
                "nRate={rate} 的边界判定错"
            );
        }
    }
}
