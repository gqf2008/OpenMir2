//! 脚本数据模型：1:1 对应 `src/Modules/SystemModule/ScriptInfo.cs`。

/// `ScriptQuestInfo`
#[derive(Debug, Clone, Copy, Default)]
pub struct ScriptQuestInfo {
    pub w_flag: i16,
    pub bt_value: u8,
    pub n_rand_rage: i32,
}

/// `ScriptInfo`
#[derive(Debug, Default)]
pub struct ScriptInfo {
    pub is_quest: bool,
    /// C# 侧为长度 10 的定长数组
    pub quest_info: [ScriptQuestInfo; 10],
    /// label（大小写不敏感）→ SayingRecord；保留插入序（C# Dictionary 此处仅按键查）
    pub record_list: Vec<SayingRecord>,
    pub quest_count: i32,
}

impl ScriptInfo {
    /// C# `RecordList.TryGetValue`（`StringComparer.OrdinalIgnoreCase`）
    pub fn find_record(&self, label: &str) -> Option<&SayingRecord> {
        self.record_list
            .iter()
            .find(|r| r.s_label.eq_ignore_ascii_case(label))
    }

    pub fn contains_label(&self, label: &str) -> bool {
        self.find_record(label).is_some()
    }
}

/// `SayingRecord`
#[derive(Debug, Clone)]
pub struct SayingRecord {
    pub s_label: String,
    pub procedure_list: Vec<SayingProcedure>,
    pub bo_ext_jmp: bool,
}

/// `SayingProcedure`
#[derive(Debug, Default, Clone)]
pub struct SayingProcedure {
    pub condition_list: Vec<QuestConditionInfo>,
    pub action_list: Vec<QuestActionInfo>,
    pub else_action_list: Vec<QuestActionInfo>,
    pub s_say_msg: String,
    pub s_else_say_msg: String,
}

/// `QuestActionInfo`
#[derive(Debug, Clone, Default)]
pub struct QuestActionInfo {
    pub n_cmd_code: i32,
    pub s_param1: String,
    pub n_param1: i32,
    pub s_param2: String,
    pub n_param2: i32,
    pub s_param3: String,
    pub n_param3: i32,
    pub s_param4: String,
    pub n_param4: i32,
    pub s_param5: String,
    pub n_param5: i32,
    pub s_param6: String,
    pub n_param6: i32,
    pub s_op_name: String,
    pub s_op_h_name: String,
}

/// `QuestConditionInfo`（注意 C# 侧比 Action 多一对 sParam7/nParam7 字段）
#[derive(Debug, Clone, Default)]
pub struct QuestConditionInfo {
    pub cmd_code: i32,
    pub s_param1: String,
    pub n_param1: i32,
    pub s_param2: String,
    pub n_param2: i32,
    pub s_param3: String,
    pub n_param3: i32,
    pub s_param4: String,
    pub n_param4: i32,
    pub s_param5: String,
    pub n_param5: i32,
    pub s_param6: String,
    pub n_param6: i32,
    pub s_param7: String,
    pub n_param7: i32,
    pub s_op_name: String,
    pub s_op_h_name: String,
}

/// 商人脚本头部 `(buy sell ...)` 可执行命令标志（`ScriptFlagConst` + `IMerchant` 布尔位）。
#[derive(Debug, Clone, Copy, Default)]
pub struct MerchantFlags {
    pub is_buy: bool,
    pub is_sell: bool,
    pub is_make_drug: bool,
    pub is_prices: bool,
    pub is_storage: bool,
    pub is_getback: bool,
    pub is_upgradenow: bool,
    pub is_get_backupgnow: bool,
    pub is_repair: bool,
    pub is_sup_repair: bool,
    pub is_send_msg: bool,
    pub is_use_item_name: bool,
    pub is_offline_msg: bool,
    pub is_yb_deal: bool,
}

/// `[goods]` 区商品条目（`Goods`）。
#[derive(Debug, Clone)]
pub struct Goods {
    pub item_name: String,
    pub count: i32,
    pub refill_time: i32,
}
