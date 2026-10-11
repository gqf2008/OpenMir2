//! 派发表与覆盖率：把 C# 生成的注册表（`handlers.rs`）与 Rust 侧已实现集合对应起来。
//!
//! 验收③要求「未实现命令显式报错、清单收敛到 0」：
//! - [`condition_status`] / [`execution_status`] 返回
//!   `Some(true)` 已实现 / `Some(false)` C# 有注册但 Rust 未实现 / `None` 未注册；
//! - 未实现项由引擎计入 `EngineError::NotImplemented`；
//! - [`coverage_report`] 输出「语料用到的派发目标中尚未实现」的清单，作为门禁判据。

use crate::handlers::{
    CONDITION_HANDLERS, ENGINE_SWITCH_CONDITIONS, ENGINE_SWITCH_EXECUTIONS, EXECUTION_HANDLERS,
};

/// `ScriptEngine` 动作 switch 命中项（键 = CmdCode 直接等于 case 的枚举值）。
pub fn engine_switch_execution(code: i32) -> Option<&'static str> {
    ENGINE_SWITCH_EXECUTIONS
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, name)| *name)
}

/// `ScriptEngine` 条件 switch 命中项。
pub fn engine_switch_condition(code: i32) -> Option<&'static str> {
    ENGINE_SWITCH_CONDITIONS
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, name)| *name)
}

/// 已实现的**动作**处理器名（随移植推进追加；名字与 C# 方法名一致）。
pub static IMPLEMENTED_EXECUTIONS: &[&str] = &[
    "ActionOfSet",
    "ActionOfClose",
    "ActionOfMessageBox",
    "ActionOfMapMove",
    "ActionOfGiveItem",
    "ActionOfMobFireBurn",
    "ActionOfResetUnit",
    "ActionOfReSet",
    "ActionOfSetOpen",
    "ActionOfSetUnit",
    // ---- B2/M4 动作批次（2026-10-11）----
    "ActionOfLineMsg",
    "ActionOfGameGold",
    "ActionOfGamePoint",
    "ActionOfChangeCreditPoint",
    "ActionOfChangeExp",
    "ActionOfChangeLevel",
    "ActionOfChangePkPoint",
    "ActionOfChangeHairStyle",
    "ActionOfHumanHp",
    "ActionOfHumanMp",
    "ActionOfKick",
    "ActionOfKill",
    "ActionOfMission",
    "ActionOfMobPlace",
    "ActionOfMap",
    "ActionOfClearDelayGoto",
    "ActionOfBreakTimereCall",
    "ActionOfTimereCall",
    "ActionOfDelayCall",
    "ActionOfSetScriptFlag",
    "ActionOfSetSendMsgFlag",
    "ActionOfAutoGetExp",
    "ActionOfOffLine",
    "ActionOfGmExecute",
    "ActionOfRepairAllItem",
    "ActionOfRestReNewLevel",
    "ActionOfReNewLevel",
    "ActionOfChangeMode",
    "ActionOfQueryValue",
    "ActionOfQueryItemDlg",
    "ActionOfUpgradeDlgItem",
    "ActionOfPlayDice",
    "ActionOfAddSkill",
    "ActionOfAddNameList",
    "ActionOfDelNameList",
    "ActionOfDelUseDateList",
    "ActionOfDelAccountList",
];

/// 已实现的**条件**处理器名。
pub static IMPLEMENTED_CONDITIONS: &[&str] = &[
    "ConditionOfCheck",
    "ConditionOfCheckGold",
    "ConditionOfCheckLevel",
    "ConditionOfEqual",
    "ConditionOfLapge",
    "ConditionOfSmall",
    "ConditionOfRandom",
    "ConditionOfCheckSlaveListCount",
    "ConditionOfCheckItem",
    // ---- B2/M4 条件批次（2026-10-11）----
    "ConditionOfCheckBagGage",
    "ConditionOfCheckGameGold",
    "ConditionOfCheckCreditPoint",
    "ConditionOfCheckGamePoint",
    "ConditionOfCheckLevelEx",
    "ConditionOfCheckReNewLevel",
    "ConditionOfCheckMemBerLevel",
    "ConditionOfCheckBonusPoint",
    "ConditionOfIssysop",
    "ConditionOfIsAdmin",
    "ConditionOfCheckMarry",
    "ConditionOfCheckHaveGuild",
    "ConditionOfCheckIsCastleaGuild",
    "ConditionOfCheckIsCastleMaster",
    "ConditionOfCheckIsGuildMaster",
    "ConditionOfCheckSafeZone",
    "ConditionOfCheckUseItem",
    "ConditionOfCheckJob",
    "ConditionOfGender",
    "ConditionOfCheckItemAddValue",
    "ConditionOfCheckBagSize",
    "ConditionOfCheckHum",
    "ConditionOfCheckOnLine",
    "ConditionOfIsDupMode",
    "ConditionOfCheckNameList",
    "ConditionOfCheckAccountList",
    "ConditionOfIosnMap",
    "ConditionOfCheckPoseGender",
    "ConditionOfCheckPoseDir",
    "ConditionOfCheckPoseLevel",
    "ConditionOfCheckPoseMarry",
    "ConditionOfIsNewHuman",
];

pub fn condition_handler(code: i32) -> Option<&'static str> {
    CONDITION_HANDLERS
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, h)| *h)
}

pub fn execution_handler(code: i32) -> Option<&'static str> {
    EXECUTION_HANDLERS
        .iter()
        .find(|(k, _)| *k == code)
        .map(|(_, h)| *h)
}

/// C# 侧是否注册了该动作（不看 Rust 是否实现）。
pub fn execution_registered(code: i32) -> bool {
    execution_handler(code).is_some()
}

pub fn execution_implemented(code: i32) -> bool {
    execution_handler(code).is_some_and(|h| IMPLEMENTED_EXECUTIONS.contains(&h))
}

/// 条件三态：`Some(true)` 已实现 / `Some(false)` 注册但未实现 / `None` 未注册。
pub fn condition_status(code: i32) -> Option<bool> {
    condition_handler(code).map(|h| IMPLEMENTED_CONDITIONS.contains(&h))
}

/// 未实现的派发目标清单（handler 名去重、升序）。
pub fn unimplemented_handlers() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = CONDITION_HANDLERS
        .iter()
        .chain(EXECUTION_HANDLERS.iter())
        .map(|(_, h)| *h)
        .filter(|h| !IMPLEMENTED_CONDITIONS.contains(h) && !IMPLEMENTED_EXECUTIONS.contains(h))
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}

/// Rust 已实现的 `ScriptEngine` **条件** switch 分支（`run_unregistered_condition` 的 case）。
/// 这些 code 不在 `CONDITION_HANDLERS` 注册表里，由引擎控制流直接处理。
pub static IMPLEMENTED_SWITCH_CONDITIONS: &[&str] = &[
    "CHECKITEMW",
    "CHECKDIEMON",
    "CHECKKILLPLAYMON",
    "CHECKRANDOMNO",
    "CHECKDURAEVA",
    "KILLBYHUM",
    "KILLBYMON",
    "CHECKINSAFEZONE",
    "ISTAKEITEM",
    "SCHECKDEATHPLAYMON",
];

/// Rust 已实现的 `ScriptEngine` **动作** switch 分支（`run_switch_action` 的 case）。
pub static IMPLEMENTED_SWITCH_EXECUTIONS: &[&str] = &[
    "Break",
    "Param1",
    "Param2",
    "Param3",
    "Param4",
    "Map",
    "MapMove",
    "PlayDice",
    "AddBatch",
    "BatchDelay",
    "BatchMove",
    "CheckUserDate",
    "GetDlgItemValue",
    "TakeDlgItem",
    "GoQuest",
    "EndQuest",
    "Goto",
    "Take",
    "Takew",
];

/// 条件 switch case 的实现三态：`Some(true)` 已实现 / `Some(false)` 未实现 / `None` 不是 switch case。
pub fn switch_condition_status(code: i32) -> Option<bool> {
    engine_switch_condition(code).map(|name| IMPLEMENTED_SWITCH_CONDITIONS.contains(&name))
}

/// 动作 switch case 的实现三态。
pub fn switch_execution_status(code: i32) -> Option<bool> {
    engine_switch_execution(code).map(|name| IMPLEMENTED_SWITCH_EXECUTIONS.contains(&name))
}

/// 语料派发目标的实现三态（覆盖报告用）：
/// - `Some(true)` 已实现（注册处理器或已覆盖的 switch 分支）
/// - `Some(false)` 有 C# 行为但 Rust 未实现
/// - `None` C# 无注册、亦非 switch case ⇒ 静默忽略（无需实现）
pub fn condition_target_status(code: i32) -> Option<bool> {
    if let Some(v) = condition_status(code) {
        return Some(v);
    }
    switch_condition_status(code)
}

/// 动作侧同 [`condition_target_status`]。
pub fn execution_target_status(code: i32) -> Option<bool> {
    if let Some(handler) = execution_handler(code) {
        return Some(IMPLEMENTED_EXECUTIONS.contains(&handler));
    }
    switch_execution_status(code)
}

/// 派发目标名（错误清单/覆盖报告用）：注册处理器名优先，其次 switch case 名。
pub fn condition_target_name(code: i32) -> Option<&'static str> {
    condition_handler(code).or_else(|| engine_switch_condition(code))
}

pub fn execution_target_name(code: i32) -> Option<&'static str> {
    execution_handler(code).or_else(|| engine_switch_execution(code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_matches_csharp_tables() {
        // 与 C# 反射 dump 一致的规模锚点（handlers.rs 由生成器产出）
        assert_eq!(CONDITION_HANDLERS.len(), 111);
        assert_eq!(EXECUTION_HANDLERS.len(), 133);
    }

    #[test]
    fn implemented_set_is_a_subset_of_registry() {
        for h in IMPLEMENTED_CONDITIONS {
            assert!(
                CONDITION_HANDLERS.iter().any(|(_, x)| x == h),
                "已实现条件处理器 {h} 不在 C# 注册表中"
            );
        }
        for h in IMPLEMENTED_EXECUTIONS {
            assert!(
                EXECUTION_HANDLERS.iter().any(|(_, x)| x == h),
                "已实现动作处理器 {h} 不在 C# 注册表中"
            );
        }
    }
}
