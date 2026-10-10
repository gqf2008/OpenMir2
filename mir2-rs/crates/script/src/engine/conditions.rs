//! 条件处理器（`ConditionProcessingSys`）——按语料使用频次优先移植。

use crate::engine::{Engine, EngineError, VarLookup};
use crate::model::QuestConditionInfo;

/// 条件处理器实现体。
impl Engine<'_, '_> {
    /// 已注册条件的派发（C# `ConditionScript.Execute`）。
    pub(crate) fn run_condition(
        &mut self,
        code: i32,
        info: &QuestConditionInfo,
        result: &mut bool,
    ) {
        let handler = crate::engine::dispatch::condition_handler(code).unwrap_or("");
        match handler {
            "ConditionOfCheck" => self.cond_check(info, result),
            "ConditionOfCheckGold" => self.cond_check_gold(info, result),
            "ConditionOfCheckLevel" => self.cond_check_level(info, result),
            "ConditionOfEqual" => self.cond_var_cmp(info, result, std::cmp::Ordering::Equal),
            "ConditionOfLapge" => self.cond_var_cmp(info, result, std::cmp::Ordering::Greater),
            "ConditionOfSmall" => self.cond_var_cmp(info, result, std::cmp::Ordering::Less),
            "ConditionOfRandom" => self.cond_random(info, result),
            "ConditionOfCheckSlaveListCount" => self.cond_slave_list_count(info, result),
            other => {
                // 不应发生：dispatch 表判定为已实现却没有分支
                self.errors.push(EngineError::NotImplemented {
                    kind: "condition",
                    code,
                    handler: leak_name(other),
                });
            }
        }
    }

    /// `ConditionOfCheck`：任务标记状态比较。
    fn cond_check(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        let n14 = mir2_shared::hutil32::str_to_int(&info.s_param1, 0);
        let n18 = mir2_shared::hutil32::str_to_int(&info.s_param2, 0);
        let status = self.player.quest_flag_status(n14 as i16);
        if status == 0 {
            if n18 != 0 {
                *result = false;
            }
        } else if n18 == 0 {
            *result = false;
        }
    }

    /// `ConditionOfCheckGold`。
    fn cond_check_gold(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        let mut n14 = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
        if n14 < 0 {
            // C#: GetVarValue(playerActor, sParam1, ref n14) —— 变量取值层
            match self.lookup_var_number(&info.s_param1, &info.s_param2) {
                Some(v) => n14 = v,
                None => {
                    self.errors.push(EngineError::UnsupportedVariable {
                        text: info.s_param1.clone(),
                    });
                }
            }
        }
        if self.player.gold() < n14 {
            *result = false;
        }
    }

    /// `ConditionOfCheckLevel`。
    fn cond_check_level(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        if self.player.level() < info.n_param1 {
            *result = false;
        }
    }

    /// `ConditionOfEqual` / `ConditionOfLapge` / `ConditionOfSmall` 共用
    /// `CheckVarNameNo` 取值后按序比较（C# 用 `nParam2` 作比较目标）。
    fn cond_var_cmp(
        &mut self,
        info: &QuestConditionInfo,
        result: &mut bool,
        want: std::cmp::Ordering,
    ) {
        let lhs = self.lookup_var_number(&info.s_param1, &info.s_param2);
        let rhs = info.n_param2;
        *result = match lhs {
            Some(v) => v.cmp(&rhs) == want,
            None => false,
        };
    }

    /// `ConditionOfCheckSlaveListCount`（C# 缺陷照搬）：
    /// `success = false; if (SlaveList.Count < nParam1) success = false;` ——
    /// 初始即 false 且**从不置 true** ⇒ 该条件恒为假（宝宝数量只被读取、不影响结果）。
    fn cond_slave_list_count(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        let _ = self.player.slave_count();
        *result = false;
        let _ = info;
    }

    /// `ConditionOfRandom`：`RandomNumber.Random(nParam1) != 0 → false`。
    fn cond_random(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        if self.player.random(info.n_param1) != 0 {
            *result = false;
        }
    }

    /// `CheckVarNameNo`：按变量名取数值（字符串型变量命中时按 C# 返回 0）。
    fn lookup_var_number(&mut self, name: &str, target_text: &str) -> Option<i32> {
        let player = &*self.player;
        let out = crate::engine::vars::check_var_name_no(
            name,
            target_text,
            &|_| 0,
            &|_| 0,
            &|_| String::new(),
            &|n| player.mn_val(n),
            &|n| player.mdy_val(n),
            &|n| player.mnmval(n),
            &|n| player.mn_integer(n),
            &|n| player.ms_string(n),
        );
        match out {
            Some(VarLookup::Number(v, _)) => Some(v),
            Some(VarLookup::StringHit) => Some(0),
            None => None,
        }
    }
}

/// 把 `&str` 提升为 `&'static str`（错误清单里用；名称来自生成表，生命周期本就是静态）。
fn leak_name(name: &str) -> &'static str {
    crate::handlers::CONDITION_HANDLERS
        .iter()
        .chain(crate::handlers::EXECUTION_HANDLERS.iter())
        .find(|(_, h)| *h == name)
        .map(|(_, h)| *h)
        .unwrap_or("(unknown)")
}
