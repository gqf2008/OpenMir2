//! 条件处理器（`ConditionProcessingSys`）——按语料使用频次优先移植。

use crate::engine::{Engine, EngineError, VarLookup};
use crate::model::QuestConditionInfo;

/// `HUtil32.Round`：`(int)Math.Round(r, MidpointRounding.AwayFromZero)`。
pub(crate) fn round_away(r: f64) -> i32 {
    if r >= 0.0 {
        (r + 0.5).floor() as i32
    } else {
        (r - 0.5).ceil() as i32
    }
}

/// `s[0]`（C# 空串会抛 `IndexOutOfRangeException`；此处落 `'\0'` 走 default 分支，绝不 panic）。
fn first_char(s: &str) -> char {
    s.chars().next().unwrap_or('\0')
}

/// `switch (cMethod) { '=' … '>' … '<' … default(>=) }`（C# 各 `ConditionOfCheckXxx` 共用）。
fn method_cmp(method: char, lhs: i32, rhs: i32) -> bool {
    match method {
        '=' => lhs == rhs,
        '>' => lhs > rhs,
        '<' => lhs < rhs,
        _ => lhs >= rhs,
    }
}

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
            "ConditionOfCheckItem" => self.cond_check_item(info, result),
            "ConditionOfCheckBagGage" => self.cond_check_bag_gage(info, result),
            "ConditionOfCheckGameGold" => {
                self.cond_method_threshold(info, result, "CHECKGAMEGOLD", |p| p.game_gold())
            }
            "ConditionOfCheckCreditPoint" => {
                self.cond_method_threshold(info, result, "CHECKCREDITPOINT", |p| p.credit_point())
            }
            "ConditionOfCheckGamePoint" => {
                self.cond_method_threshold(info, result, "CHECKGAMEPOINT", |p| p.game_point())
            }
            "ConditionOfCheckLevelEx" => {
                self.cond_method_threshold(info, result, "CHECKLEVELEX", |p| p.level())
            }
            // C# `ConditionOfCheckReNewLevel` 的错误码写的是 `CHECKLEVELEX`（原文如此，照搬）
            "ConditionOfCheckReNewLevel" => {
                self.cond_method_threshold(info, result, "CHECKLEVELEX", |p| p.re_level())
            }
            "ConditionOfCheckMemBerLevel" => {
                self.cond_method_threshold(info, result, "CHECKMEMBERLEVEL", |p| p.member_level())
            }
            "ConditionOfCheckBonusPoint" => self.cond_check_bonus_point(info, result),
            "ConditionOfIssysop" => {
                *result = self.player.permission() >= 4;
            }
            "ConditionOfIsAdmin" => {
                *result = self.player.permission() >= 6;
            }
            "ConditionOfCheckMarry" => {
                *result = !self.player.dear_name().is_empty();
            }
            "ConditionOfCheckHaveGuild" => {
                *result = self.player.guild_name().is_some();
            }
            // C# 方法体整段被注释 ⇒ 恒假（`success = false;` 后无任何赋值）
            "ConditionOfCheckIsCastleaGuild" | "ConditionOfCheckIsCastleMaster" => {
                *result = false;
            }
            "ConditionOfCheckIsGuildMaster" => {
                *result = self.player.is_guild_master();
            }
            "ConditionOfCheckSafeZone" => {
                *result = self.player.in_safe_zone();
            }
            "ConditionOfCheckUseItem" => self.cond_check_use_item(info, result),
            "ConditionOfCheckJob" => self.cond_check_job(info, result),
            "ConditionOfGender" => self.cond_gender(info, result),
            "ConditionOfCheckItemAddValue" => self.cond_check_item_add_value(info, result),
            "ConditionOfCheckBagSize" => self.cond_check_bag_size(info, result),
            "ConditionOfCheckHum" => self.cond_check_hum(info, result),
            // C# 缺陷照搬：`success = false;` 之后只会写 false（从不置 true）⇒ 恒假
            "ConditionOfCheckOnLine" => {
                // `if ((s01 != "") && (s01[0] == '<') && (s01[1] == '$')) GetLineVariableText(…)
                //  else GetValValue(…, ref s01);`（C# 在长度 1 时会越界；此处按不命中处理）
                let mut s01 = info.s_param1.clone();
                if s01.starts_with("<$") {
                    s01 = self.player.line_variable_text(&info.s_param1);
                } else if let Some(v) = self.get_val_value_string(&info.s_param1) {
                    s01 = v;
                }
                if !s01.is_empty() {
                    let _ = self.player.is_player_online(&s01);
                }
                *result = false;
            }
            "ConditionOfIsDupMode" => {
                *result = matches!(self.player.xy_obj_count(), Some(n) if n > 1);
            }
            "ConditionOfCheckNameList" => {
                let name = self.player.chr_name();
                *result = self.check_string_list(&name, &info.s_param1);
            }
            "ConditionOfCheckAccountList" => {
                let acct = self.player.user_account();
                let rel = format!("{}{}", self.npc.path(), info.s_param1);
                *result = self.check_string_list(&acct, &rel);
            }
            "ConditionOfIosnMap" => self.cond_is_on_map(info, result),
            "ConditionOfCheckPoseGender" => self.cond_pose_gender(info, result),
            "ConditionOfCheckPoseDir" => self.cond_pose_dir(info, result),
            "ConditionOfCheckPoseLevel" => self.cond_pose_level(info, result),
            "ConditionOfCheckPoseMarry" => {
                *result = self.player.pose().is_some_and(|p| {
                    p.race == crate::engine::ACTOR_RACE_PLAY && !p.dear_name.is_empty()
                });
            }
            "ConditionOfIsNewHuman" => {
                *result = self.player.is_new_human();
            }
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

    /// `ConditionOfCheckGameGold` / `CheckCreditPoint` / `CheckGamePoint` / `CheckLevelEx` /
    /// `CheckReNewLevel` / `CheckMemBerLevel` 的统一形状：
    ///
    /// ```text
    /// success = false;
    /// n = StrToInt(sParam2, -1);
    /// if (n < 0) { GetVarValue(player, sParam2, ref n); if (n < 0) { ScriptConditionError(sCmd); return; } }
    /// if (method(sParam1[0], playerField, n)) success = true;
    /// ```
    fn cond_method_threshold(
        &mut self,
        info: &QuestConditionInfo,
        result: &mut bool,
        cmd_member: &str,
        field: impl Fn(&dyn crate::engine::ScriptPlayer) -> i32,
    ) {
        *result = false;
        let mut n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
        if n < 0 {
            if let Some(v) = self.get_val_value_int(&info.s_param2) {
                n = v;
            }
            if n < 0 {
                self.script_condition_error(cmd_member, info);
                return;
            }
        }
        let value = field(&*self.player);
        if method_cmp(first_char(&info.s_param1), value, n) {
            *result = true;
        }
    }

    /// `ConditionOfCheckBagGage`（code 24）：权重分支在 C# 里被注释掉 ⇒
    /// `success = IsEnoughBag() && sParam1 == ""`。
    fn cond_check_bag_gage(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        if !self.player.is_enough_bag() || !info.s_param1.is_empty() {
            *result = false;
        }
    }

    /// `ConditionOfCheckBonusPoint`：`BonusAbil` 九项和 + `BonusPoint` 与 `nParam2` 比较。
    /// 注意 C# **没有**错误分支：`sParam2` 解析失败时 `nCount = -1`（不报错、不查变量）。
    fn cond_check_bonus_point(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let total = self.player.bonus_total_point();
        let mut n = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
        if n < 0 {
            if let Some(v) = self.get_val_value_int(&info.s_param2) {
                n = v;
            }
        }
        if method_cmp(first_char(&info.s_param1), total, n) {
            *result = true;
        }
    }

    /// `ConditionOfCheckUseItem`：`sParam1` 是佩戴位下标（`0..=UseItems.GetUpperBound(0)`）。
    fn cond_check_use_item(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let mut n_where = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
        if n_where < 0 {
            if let Some(v) = self.get_val_value_int(&info.s_param1) {
                n_where = v;
            }
        }
        if n_where < 0 || n_where > self.player.use_items_upper_bound() {
            self.script_condition_error("CHECKUSEITEM", info);
            return;
        }
        if self.player.use_item_index(n_where) > 0 {
            *result = true;
        }
    }

    /// `ConditionOfCheckJob`（code 8）：`CompareLStr(sParam1, "Warrior"/"Wizard"/"Taoist")`。
    fn cond_check_job(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        let job = self.player.job();
        let p1 = info.s_param1.as_str();
        if mir2_shared::hutil32::compare_lstr(p1, "Warrior") && job != 0 {
            *result = false;
        }
        if mir2_shared::hutil32::compare_lstr(p1, "Wizard") && job != 1 {
            *result = false;
        }
        if mir2_shared::hutil32::compare_lstr(p1, "Taoist") && job != 2 {
            *result = false;
        }
    }

    /// `ConditionOfGender`（code 3）：`MAN`（忽略大小写）→ Man=0，其余 → WoMan=1。
    fn cond_gender(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        let want = if info.s_param1.eq_ignore_ascii_case("MAN") {
            crate::engine::GENDER_MAN
        } else {
            crate::engine::GENDER_WOMAN
        };
        if self.player.gender() != want {
            *result = false;
        }
    }

    /// `ConditionOfCheckItemAddValue`（code 130）：佩戴位 `Desc` 之和与 `sParam3` 比较。
    ///
    /// **C# 缺陷照搬**：比较方法字符取的是 `sParam1[0]`（佩戴位下标）而**不是** `sParam2[0]`；
    /// 且范围校验写的是 `nWhere <= UseItems.Length`（上界多 1，越界由 C# 抛异常）。
    fn cond_check_item_add_value(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let n_where = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
        let n_add_value = mir2_shared::hutil32::str_to_int(&info.s_param3, -1);
        if !(n_where >= 0 && n_where <= self.player.use_items_upper_bound() + 1) || n_add_value < 0
        {
            self.script_condition_error("CHECKITEMADDVALUE", info);
            return;
        }
        let Some(total) = self.player.worn_item_add_value(n_where) else {
            return;
        };
        if method_cmp(first_char(&info.s_param1), total, n_add_value) {
            *result = true;
        }
    }

    /// `ConditionOfCheckBagSize`（code 111）：`ItemList.Count + nSize <= MaxBagItem(46)`。
    fn cond_check_bag_size(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let mut n_size = mir2_shared::hutil32::str_to_int(&info.s_param1, -1);
        if n_size < 0 {
            if let Some(v) = self.get_val_value_int(&info.s_param1) {
                n_size = v;
            }
        }
        if n_size <= 0 || n_size > crate::engine::MAX_BAG_ITEM {
            self.script_condition_error("CHECKBAGSIZE", info);
            return;
        }
        if self.player.bag_count() + n_size <= crate::engine::MAX_BAG_ITEM {
            *result = true;
        }
    }

    /// `ConditionOfCheckHum`（code 23）：`WorldEngine.GetMapHuman(sParam1) >= sParam2`（`sParam2`
    /// 先试变量取值，未命中用 `nParam2`）。
    fn cond_check_hum(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        let n14 = self
            .get_val_value_int(&info.s_param2)
            .unwrap_or(info.n_param2);
        if self.player.map_human_count(&info.s_param1) < n14 {
            *result = false;
        }
    }

    /// `ConditionOfIosnMap`（code 190，`ISONMAP`）：地图名支持变量展开；地图不存在则报错。
    fn cond_is_on_map(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let name = self.player.line_variable_text(&info.s_param1);
        if !self.player.map_exists(&name) {
            self.script_condition_error("ISONMAP", info);
        }
        if self.player.envir_map_name().as_deref() == Some(name.as_str()) {
            *result = true;
        }
    }

    /// `ConditionOfCheckPoseGender`（code 82）：`MAN`/`男` → 0，`WOMAN`/`女` → 1，其余 → 0。
    fn cond_pose_gender(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let sex = match info.s_param1.as_str() {
            "WOMAN" | "女" => crate::engine::GENDER_WOMAN,
            _ => crate::engine::GENDER_MAN,
        };
        if let Some(p) = self.player.pose() {
            if p.gender == sex {
                *result = true;
            }
        }
    }

    /// `ConditionOfCheckPoseDir`（code 80）：要求面向对象也面向自己（`is_mutual`）；
    /// `nParam1 == 1` 要求同性、`== 2` 要求异性、其余不判性别。
    fn cond_pose_dir(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let Some(p) = self.player.pose() else { return };
        if !p.is_mutual {
            return;
        }
        match info.n_param1 {
            1 => {
                if p.gender == self.player.gender() {
                    *result = true;
                }
            }
            2 => {
                if p.gender != self.player.gender() {
                    *result = true;
                }
            }
            _ => *result = true,
        }
    }

    /// `ConditionOfCheckPoseLevel`（code 81）：面向对象等级比较。
    fn cond_pose_level(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = false;
        let mut n_level = mir2_shared::hutil32::str_to_int(&info.s_param2, -1);
        if n_level < 0 {
            if let Some(v) = self.get_val_value_int(&info.s_param2) {
                n_level = v;
            }
            if n_level < 0 {
                self.script_condition_error("CHECKPOSELEVEL", info);
                return;
            }
        }
        if let Some(p) = self.player.pose() {
            if method_cmp(first_char(&info.s_param1), p.level, n_level) {
                *result = true;
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

    /// `ConditionOfCheckItem`：`success = true;` 变量替换后
    /// `playerActor.QuestCheckItem(s01, ref n1C, ...)`，`n1C < nParam2 → false`。
    ///
    /// 待核对：C# `PlayObject.QuestCheckItem` 的计数范围（背包/佩戴位）与耐久统计——
    /// 本实现按背包件数（`item_count`）计；若某些脚本依赖佩戴位计数，flow-diff 会暴露差异。
    fn cond_check_item(&mut self, info: &QuestConditionInfo, result: &mut bool) {
        *result = true;
        // C#: GetVarValue(playerActor, sParam1, ref s01)（变量替换；无变量层时原样）
        let name = &info.s_param1;
        let count = self.player.item_count(name);
        if count < info.n_param2 {
            *result = false;
        }
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
