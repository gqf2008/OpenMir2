//! `SETMPAMODE`（脚本动作 `ActionOfSetMapMode`，`ExecutionProcessingSys.cs:4423-4881`）的
//! **分支表落地**与写队列。
//!
//! oracle 语义（逐行对照 4423-4881）：
//!
//! ```text
//! IEnvirnoment envir = SystemShare.MapMgr.FindMap(sMapName);
//! if (envir == null || string.IsNullOrEmpty(sMapMode)) ScriptActionError(...); return;   // 4429-4434
//! 按 sMapMode（OrdinalIgnoreCase）逐支 else-if，共 31 支：
//!   SAFE   → Flag.SafeArea      = !IsNullOrEmpty(sParam1)
//!   DARK   → Flag.boDarkness    = !IsNullOrEmpty(sParam1)
//!   FIGHT  → Flag.FightZone     = !IsNullOrEmpty(sParam1)
//!   FIGHT3 → Flag.Fight3Zone    = !IsNullOrEmpty(sParam1)
//!   DAY    → Flag.DayLight      = !IsNullOrEmpty(sParam1)
//!   QUIZ   → Flag.boQUIZ        = !IsNullOrEmpty(sParam1)
//!   NORECONNECT → p1 非空 ? (true, sNoReConnectMap = sParam1) : false
//!   MUSIC       → p1 非空 ? (true, MusicId = StrToInt(sParam1,-1)) : false
//!   EXPRATE     → p1 非空 ? (true, ExpRate = StrToInt(sParam1,-1)) : false
//!   PKWINLEVEL / PKWINEXP / PKLOSTLEVEL / PKLOSTEXP
//!               → p1 非空 ? (true, nPK* = StrToInt(sParam1,-1)) : false
//!   DECHP       → **sParam3 与 sParam4 都非空** ? (boDECHP=true,
//!                   nDECHPTIME = StrToInt(sParam3,-1), nDECHPPOINT = StrToInt(sParam4,-1))
//!                 : boDECHP = false                    // 4585-4596
//!   DECGAMEGOLD → 同上两参数门；nDECGAMEGOLDTIME = StrToInt(sParam3), nDECGAMEGOLD = StrToInt(sParam4)
//!   RUNHUMAN/RUNMON/NEEDHOLE/NORECALL/NOGUILDRECALL/NODEARRECALL/NOMASTERRECALL/
//!   NORANDOMMOVE/NODRUG/MINE/MINE2/NOTHROWITEM/NODROPITEM/NOPOSITIONMOVE/NOHORSE/NOCHAT
//!               → 各 Flag 布尔 = !IsNullOrEmpty(sParam1)
//! 未命中任何一支 ⇒ **静默无操作**（不是错误；`ScriptActionError` 只在 FindMap 落空/mode 空时）。
//! ```
//!
//! **不抽 RNG**、不产生任何下行帧、不读玩家状态——纯地图 Flag 写入。
//!
//! ## 宿主落地点
//!
//! `NpcPlayer::set_map_mode`（`npc.rs`）只**记录**一条 [`MapModeWrite`]（脚本执行期不能借
//! `&mut MapRegistry`）；宿主在一条玩家命令处理完之后、`drain_player_map_hp`（P25 DECHP/INCHP
//! 的消费点）之前，用 [`flush_map_mode_writes`] 一次性落库。oracle 是脚本动作当场写
//! `envir.Flag`——本宿主的推迟点与 oracle 可观测等价：脚本引擎全仓**没有任何读地图 Flag 的
//! 条件/动作**（`grep 'Envir.Flag' src/Modules/ScriptEngine/` 只命中本动作的写侧），且消费点
//! （P25）在命令处理之后才跑。

use mir2_envir::{MapInfoFlag, MapRegistry};
use mir2_shared::hutil32::str_to_int;

/// `HUtil32.StrToInt(sParam, -1)` 的缺省（oracle 全程用 `-1`）。
const PARAM_DEFAULT: i32 = -1;

/// 一条待落的 `SETMPAMODE` 写（脚本执行期收集，宿主在消费点之前落库）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MapModeWrite {
    /// `sParam1`（`MapMgr.FindMap` 的逻辑名；引擎已用 `map_exists` 过门）。
    pub map_name: String,
    /// `sParam2`（分支名，`OrdinalIgnoreCase`）。
    pub mode: String,
    /// `sParam3`。
    pub param1: String,
    /// `sParam4`。
    pub param2: String,
}

/// `string.Compare(a, b, StringComparison.OrdinalIgnoreCase) == 0` 的等价判定。
///
/// 分支名全是 ASCII，`.NET` OrdinalIgnoreCase 与 `eq_ignore_ascii_case` 在这些字面量上一致。
fn mode_eq(mode: &str, name: &str) -> bool {
    mode.eq_ignore_ascii_case(name)
}

/// 对**单个** `Env.Flag` 落地 oracle 4423-4881 的分支表。
///
/// 返回 `true` = 命中了某一支（可能写了也可能只把布尔置 `false`）；`false` = 未命中任何分支
/// （oracle 静默无操作）。
#[must_use]
pub fn apply_set_map_mode(flag: &mut MapInfoFlag, mode: &str, param1: &str, param2: &str) -> bool {
    // 简单的「p1 非空 ⇒ true」支（oracle 每支都是 `if (!IsNullOrEmpty(sParam1)) X = true; else X = false;`）。
    let on = !param1.is_empty();
    macro_rules! bool_branch {
        ($name:literal, $field:ident) => {
            if mode_eq(mode, $name) {
                flag.$field = on;
                return true;
            }
        };
    }
    // 「p1 非空 ⇒ (true, 数值 = StrToInt(p1,-1))」支。
    macro_rules! int_branch {
        ($name:literal, $bool_field:ident, $int_field:ident) => {
            if mode_eq(mode, $name) {
                flag.$bool_field = on;
                if on {
                    flag.$int_field = str_to_int(param1, PARAM_DEFAULT);
                }
                return true;
            }
        };
    }

    bool_branch!("SAFE", safe_area);
    bool_branch!("DARK", bo_darkness);
    bool_branch!("FIGHT", fight_zone);
    bool_branch!("FIGHT3", fight3_zone);
    bool_branch!("DAY", day_light);
    bool_branch!("QUIZ", bo_quiz);
    if mode_eq(mode, "NORECONNECT") {
        flag.bo_noreconnect = on;
        if on {
            flag.s_no_reconnect_map = param1.to_string();
        }
        return true;
    }
    int_branch!("MUSIC", music, music_id);
    int_branch!("EXPRATE", bo_exprate, exp_rate);
    int_branch!("PKWINLEVEL", bo_pkwinlevel, n_pkwinlevel);
    int_branch!("PKWINEXP", bo_pkwinexp, n_pkwinexp);
    int_branch!("PKLOSTLEVEL", bo_pklostlevel, n_pklostlevel);
    int_branch!("PKLOSTEXP", bo_pklostexp, n_pklostexp);
    // DECHP：**两个参数都非空**才置真并写 TIME/POINT；否则只把布尔置 `false`
    //（数值保持原值——oracle 亦不重置）。`nDECHPTIME = sParam3`、`nDECHPPOINT = sParam4`。
    if mode_eq(mode, "DECHP") {
        if !param1.is_empty() && !param2.is_empty() {
            flag.bo_dechp = true;
            flag.n_dechptime = str_to_int(param1, PARAM_DEFAULT);
            flag.n_dechppoint = str_to_int(param2, PARAM_DEFAULT);
        } else {
            flag.bo_dechp = false;
        }
        return true;
    }
    // DECGAMEGOLD：同形（`ExecutionProcessingSys.cs:4598-4610`）。
    if mode_eq(mode, "DECGAMEGOLD") {
        if !param1.is_empty() && !param2.is_empty() {
            flag.bo_decgamegold = true;
            flag.n_decgamegoldtime = str_to_int(param1, PARAM_DEFAULT);
            flag.n_decgamegold = str_to_int(param2, PARAM_DEFAULT);
        } else {
            flag.bo_decgamegold = false;
        }
        return true;
    }
    bool_branch!("RUNHUMAN", run_human);
    bool_branch!("RUNMON", run_mon);
    bool_branch!("NEEDHOLE", bo_needhole);
    bool_branch!("NORECALL", no_re_call);
    bool_branch!("NOGUILDRECALL", no_guild_re_call);
    bool_branch!("NODEARRECALL", bo_nodearrecall);
    bool_branch!("NOMASTERRECALL", master_re_call);
    bool_branch!("NORANDOMMOVE", bo_norandommove);
    bool_branch!("NODRUG", bo_nodrug);
    bool_branch!("MINE", mine);
    bool_branch!("MINE2", bo_mine2);
    bool_branch!("NOTHROWITEM", no_throw_item);
    bool_branch!("NODROPITEM", no_drop_item);
    bool_branch!("NOPOSITIONMOVE", bo_nopositionmove);
    bool_branch!("NOHORSE", no_horse);
    bool_branch!("NOCHAT", bo_nochat);
    false
}

/// 把队列里的写落进注册表（`Envirnoment.Flag` 的运行时真值）。
///
/// `FindMap` 落空的写**丢弃**（oracle 在引擎层已用 `map_exists` 过门；此处的二次缺失只可能是
/// 注册表在脚本执行后被换掉，按 oracle「FindMap == null ⇒ 不写」同口径处理）。
/// 返回真正落库的条数。
pub fn flush_map_mode_writes(writes: &[MapModeWrite], registry: &mut MapRegistry) -> usize {
    let mut applied = 0usize;
    for w in writes {
        let Some(map_id) = registry.id_of(&w.map_name) else {
            continue;
        };
        let Some(mut flag) = registry.flag_of(map_id).cloned() else {
            continue;
        };
        if apply_set_map_mode(&mut flag, &w.mode, &w.param1, &w.param2) {
            registry.set_flag(map_id, flag);
            applied += 1;
        }
    }
    applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flag() -> MapInfoFlag {
        MapInfoFlag::default()
    }

    /// `DECHP`：两参数都非空 ⇒ `boDECHP=true`、`TIME=StrToInt(p1)`、`POINT=StrToInt(p2)`
    /// （`ExecutionProcessingSys.cs:4585-4596`）。
    #[test]
    fn dechp_sets_all_three_fields() {
        let mut f = flag();
        assert!(apply_set_map_mode(&mut f, "DECHP", "5", "10"));
        assert!(f.bo_dechp);
        assert_eq!(f.n_dechptime, 5);
        assert_eq!(f.n_dechppoint, 10);
    }

    /// `DECHP`：任一参数为空 ⇒ **只**把 `boDECHP` 置 `false`，数值保持原值。
    #[test]
    fn dechp_clears_boolean_only_when_a_param_is_missing() {
        let mut f = flag();
        f.bo_dechp = true;
        f.n_dechptime = 7;
        f.n_dechppoint = 9;
        assert!(apply_set_map_mode(&mut f, "dechp", "5", ""));
        assert!(!f.bo_dechp);
        assert_eq!((f.n_dechptime, f.n_dechppoint), (7, 9));
    }

    /// `StrToInt(_, -1)`：非数字参数落 `-1`（oracle 同形，不报错）。
    #[test]
    fn dechp_non_numeric_params_fall_back_to_minus_one() {
        let mut f = flag();
        assert!(apply_set_map_mode(&mut f, "DECHP", "abc", "xyz"));
        assert!(f.bo_dechp);
        assert_eq!(f.n_dechptime, -1);
        assert_eq!(f.n_dechppoint, -1);
    }

    /// 分支名 `OrdinalIgnoreCase`：大小写混写命中。
    #[test]
    fn mode_is_case_insensitive() {
        let mut f = flag();
        assert!(apply_set_map_mode(&mut f, "Safe", "1", ""));
        assert!(f.safe_area);
        assert!(apply_set_map_mode(&mut f, "nOdRoPiTeM", "1", ""));
        assert!(f.no_drop_item);
    }

    /// 简单支：`p1` 空 ⇒ 置 `false`（图上一行 `SAFE` 关掉安全区）。
    #[test]
    fn simple_branches_follow_param1_emptiness() {
        for mode in [
            "SAFE",
            "DARK",
            "FIGHT",
            "FIGHT3",
            "DAY",
            "QUIZ",
            "RUNHUMAN",
            "RUNMON",
            "NEEDHOLE",
            "NORECALL",
            "NOGUILDRECALL",
            "NODEARRECALL",
            "NOMASTERRECALL",
            "NORANDOMMOVE",
            "NODRUG",
            "MINE",
            "MINE2",
            "NOTHROWITEM",
            "NODROPITEM",
            "NOPOSITIONMOVE",
            "NOHORSE",
            "NOCHAT",
        ] {
            let mut on = flag();
            assert!(apply_set_map_mode(&mut on, mode, "1", ""), "{mode} 必命中");
            let mut off = flag();
            assert!(apply_set_map_mode(&mut off, mode, "", ""), "{mode} 必命中");
            // 每个简单支都至少有一个字段被置位/复位：用「on != off」钉住实际写了字段。
            assert_ne!(on, off, "{mode} 必须写 Flag（p1 空/非空不同）");
        }
    }

    /// 带数值的支（`NORECONNECT/MUSIC/EXPRATE/PK*`）写对应字段。
    #[test]
    fn value_branches_write_their_fields() {
        let mut f = flag();
        assert!(apply_set_map_mode(&mut f, "NORECONNECT", "3", ""));
        assert!(f.bo_noreconnect);
        assert_eq!(f.s_no_reconnect_map, "3");
        assert!(apply_set_map_mode(&mut f, "MUSIC", "77", ""));
        assert!(f.music);
        assert_eq!(f.music_id, 77);
        assert!(apply_set_map_mode(&mut f, "EXPRATE", "200", ""));
        assert!(f.bo_exprate);
        assert_eq!(f.exp_rate, 200);
        assert!(apply_set_map_mode(&mut f, "PKWINLEVEL", "5", ""));
        assert_eq!(f.n_pkwinlevel, 5);
        assert!(apply_set_map_mode(&mut f, "PKWINEXP", "6", ""));
        assert_eq!(f.n_pkwinexp, 6);
        assert!(apply_set_map_mode(&mut f, "PKLOSTLEVEL", "7", ""));
        assert_eq!(f.n_pklostlevel, 7);
        assert!(apply_set_map_mode(&mut f, "PKLOSTEXP", "8", ""));
        assert_eq!(f.n_pklostexp, 8);
        assert!(apply_set_map_mode(&mut f, "DECGAMEGOLD", "9", "10"));
        assert!(f.bo_decgamegold);
        assert_eq!((f.n_decgamegoldtime, f.n_decgamegold), (9, 10));
    }

    /// 未命中任何分支 ⇒ 返回 `false`、Flag 一字不改（oracle 静默无操作，不是错误）。
    #[test]
    fn unknown_mode_is_a_silent_no_op() {
        let mut f = flag();
        f.bo_dechp = true;
        let before = f.clone();
        assert!(!apply_set_map_mode(&mut f, "NOPE", "1", "2"));
        assert_eq!(f, before);
    }
}
