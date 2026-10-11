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
