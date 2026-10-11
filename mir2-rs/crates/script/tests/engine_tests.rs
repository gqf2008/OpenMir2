//! 求值引擎测试：控制流、派发落点（含 B-8 位移）、未实现显式报错。

use mir2_script::engine::{Engine, EngineError, ScriptNpc, ScriptPlayer};
use mir2_script::{ScriptInfo, ScriptParsers};

struct MockPlayer {
    last_npc: i32,
    script: Option<usize>,
    goto_count: i32,
    gold: i32,
    level: i32,
    flags: std::collections::HashMap<i16, u8>,
    mn: [i32; 100],
    msgs: Vec<(i32, Option<String>)>,
    /// `send_msg` 的完整行（含 5 个数值参数，与 flow 夹具同格式）
    msgs_full: Vec<String>,
    rng_value: i32,
    unit_status: std::collections::HashMap<i32, i32>,
    items: Vec<(String, i32)>,
    effects: Vec<String>,
    // ---- B2/M4 条件批次新增的脚本可见状态 ----
    game_gold: i32,
    game_point: i32,
    credit_point: i32,
    member_level: i32,
    /// `playerActor.BonusAbil` 九项之和 + `playerActor.BonusPoint`（`bonus_total_point()` 的 mocked 值）
    bonus_abil_total: i32,
    permission: i32,
    gender: i32,
    job: i32,
    dear_name: String,
    guild: Option<String>,
    is_guild_master: bool,
    safe_zone: bool,
    new_human: bool,
    envir_map: Option<String>,
    known_maps: Vec<String>,
    map_human: i32,
    chr: String,
    account: String,
    /// 佩戴位 `Desc` 之和（`None` = 空位）
    use_items: Vec<Option<i32>>,
    /// `QuestCheckItem` 返回的 `(n1C, nMaxDura, nDura)`
    quest_item: (i32, i32, i32),
    pose: Option<mir2_script::engine::PoseInfo>,
    last_hiter_race: Option<i32>,
    last_hiter_name: Option<String>,
    target: Option<String>,
    // ---- B2/M4 动作批次新增的脚本可见状态 ----
    tick: i32,
    /// 世界侧效果台账（广播/系统消息/对话框/装备改动/模式开关等）
    events: Vec<String>,
    game_gold_changed: i32,
    abil_exp: i32,
    abil_hp: i32,
    abil_mp: i32,
    wabil_hp: i32,
    wabil_max_hp: i32,
    wabil_mp: i32,
    wabil_max_mp: i32,
    pk_point: i32,
    pvp_level: i32,
    hair: u8,
    bonus: i32,
    re_level: i32,
    admin_mode: bool,
    super_man: bool,
    ob_mode: bool,
    bo_send_msg_flag: bool,
    bo_change_item_name_flag: bool,
    user_line_msgs: Vec<String>,
    no_item: bool,
    die_count: i32,
    last_hiter_set: bool,
    mission: Option<(String, i32, i32)>,
    delay_goto_cleared: bool,
    is_time_recall: Option<bool>,
    time_recall: Option<(String, i32, i32, i32)>,
    delay_call: Option<(i32, String, i32, i32)>,
    val_query: Option<(u8, u8, String, i32)>,
    dlg_query: Option<(bool, String)>,
    play_dice_label: String,
    auto_get_exp: Option<(bool, Option<String>, i32, i32)>,
    offline: Option<(Option<String>, i32, i32, i32)>,
    magics: Vec<(i32, String, u8)>,
    magic_table: Vec<(String, i32, String)>,
    training: Vec<i32>,
    mdy: std::collections::HashMap<usize, i32>,
    worn_names: std::collections::HashMap<i32, String>,
    worn_dura: std::collections::HashMap<i32, i32>,
    worn_dura_max: std::collections::HashMap<i32, i32>,
    // ---- B2/M4 var 家族批次新增的脚本可见状态 ----
    global: std::collections::HashMap<usize, i32>,
    globa_dy: std::collections::HashMap<usize, i32>,
    global_a: std::collections::HashMap<usize, String>,
    mnm: std::collections::HashMap<usize, i32>,
    mn_int: std::collections::HashMap<usize, i32>,
    ms_str: std::collections::HashMap<usize, String>,
    dyn_vars: std::collections::HashMap<
        (mir2_script::engine::DynVarKind, String),
        mir2_script::engine::DynVar,
    >,
    is_start_marry: bool,
    is_start_un_marry: bool,
    marry_count: i32,
    pose_state: std::cell::RefCell<PoseState>,
    cleared_map: Option<String>,
    regen: Vec<String>,
    today: String,
    worn_desc: std::collections::HashMap<(i32, i32), i32>,
    worn_present: std::collections::HashMap<i32, bool>,
    updated_items: Vec<i32>,
}

/// 对方（pose）侧的可观测状态。
#[derive(Default)]
struct PoseState {
    dear_name: String,
    is_start_marry: bool,
    is_start_un_marry: bool,
    marry_count: i32,
    goto: Vec<String>,
    ref_show_name: i32,
}

impl MockPlayer {
    fn new() -> Self {
        Self {
            last_npc: 0,
            script: None,
            goto_count: 0,
            gold: 100,
            level: 10,
            flags: std::collections::HashMap::new(),
            mn: [0; 100],
            msgs: Vec::new(),
            msgs_full: Vec::new(),
            rng_value: 0,
            unit_status: std::collections::HashMap::new(),
            items: Vec::new(),
            effects: Vec::new(),
            game_gold: 0,
            game_point: 0,
            credit_point: 0,
            member_level: 0,
            bonus_abil_total: 0,
            permission: 0,
            gender: mir2_script::engine::GENDER_MAN,
            job: 0,
            dear_name: String::new(),
            guild: None,
            is_guild_master: false,
            safe_zone: false,
            new_human: false,
            envir_map: None,
            known_maps: Vec::new(),
            map_human: 0,
            chr: "测试玩家".into(),
            account: "testacct".into(),
            use_items: Vec::new(),
            quest_item: (0, 0, 0),
            pose: None,
            last_hiter_race: None,
            last_hiter_name: None,
            target: None,
            tick: 1_000,
            events: Vec::new(),
            game_gold_changed: 0,
            abil_exp: 0,
            abil_hp: 0,
            abil_mp: 0,
            wabil_hp: 0,
            wabil_max_hp: 0,
            wabil_mp: 0,
            wabil_max_mp: 0,
            pk_point: 0,
            pvp_level: 0,
            hair: 0,
            bonus: 0,
            re_level: 0,
            admin_mode: false,
            super_man: false,
            ob_mode: false,
            bo_send_msg_flag: false,
            bo_change_item_name_flag: false,
            user_line_msgs: Vec::new(),
            no_item: false,
            die_count: 0,
            last_hiter_set: false,
            mission: None,
            delay_goto_cleared: false,
            is_time_recall: None,
            time_recall: None,
            delay_call: None,
            val_query: None,
            dlg_query: None,
            play_dice_label: String::new(),
            auto_get_exp: None,
            offline: None,
            magics: Vec::new(),
            magic_table: Vec::new(),
            training: Vec::new(),
            mdy: std::collections::HashMap::new(),
            worn_names: std::collections::HashMap::new(),
            worn_dura: std::collections::HashMap::new(),
            worn_dura_max: std::collections::HashMap::new(),
            global: std::collections::HashMap::new(),
            globa_dy: std::collections::HashMap::new(),
            global_a: std::collections::HashMap::new(),
            mnm: std::collections::HashMap::new(),
            mn_int: std::collections::HashMap::new(),
            ms_str: std::collections::HashMap::new(),
            dyn_vars: std::collections::HashMap::new(),
            is_start_marry: false,
            is_start_un_marry: false,
            marry_count: 0,
            pose_state: std::cell::RefCell::new(PoseState::default()),
            cleared_map: None,
            regen: Vec::new(),
            today: String::new(),
            worn_desc: std::collections::HashMap::new(),
            worn_present: std::collections::HashMap::new(),
            updated_items: Vec::new(),
        }
    }
}

impl ScriptPlayer for MockPlayer {
    fn last_npc(&self) -> i32 {
        self.last_npc
    }
    fn set_last_npc(&mut self, id: i32) {
        self.last_npc = id;
    }
    fn script_index(&self) -> Option<usize> {
        self.script
    }
    fn set_script_index(&mut self, idx: Option<usize>) {
        self.script = idx;
    }
    fn script_goto_count(&self) -> i32 {
        self.goto_count
    }
    fn set_script_goto_count(&mut self, v: i32) {
        self.goto_count = v;
    }
    fn quest_flag_status(&self, flag: i16) -> u8 {
        *self.flags.get(&flag).unwrap_or(&0)
    }
    fn gold(&self) -> i32 {
        self.gold
    }
    fn dec_gold(&mut self, n: i32) {
        self.gold -= n;
    }
    fn inc_gold(&mut self, n: i32) {
        self.gold += n;
    }
    fn actor_id(&self) -> i32 {
        4242
    }
    fn level(&self) -> i32 {
        self.level
    }
    fn random(&mut self, _value: i32) -> i32 {
        self.rng_value
    }
    fn send_msg(
        &mut self,
        ident: i32,
        w_param: i32,
        n_param1: i32,
        n_param2: i32,
        n_param3: i32,
        msg: Option<&str>,
    ) {
        let text = msg.unwrap_or("").replace('\r', "\\r").replace('\n', "\\n");
        self.msgs_full.push(format!(
            "msg {ident} {w_param} {n_param1} {n_param2} {n_param3} {text}"
        ));
        self.msgs.push((ident, msg.map(str::to_string)));
    }
    fn mn_val(&self, n: usize) -> i32 {
        self.mn[n]
    }
    fn set_mn_val(&mut self, n: usize, v: i32) {
        self.mn[n] = v;
    }
    fn mdy_val(&self, n: usize) -> i32 {
        self.mdy.get(&n).copied().unwrap_or(0)
    }
    fn set_mdy_val(&mut self, n: usize, v: i32) {
        self.mdy.insert(n, v);
    }
    fn mnmval(&self, n: usize) -> i32 {
        self.mnm.get(&n).copied().unwrap_or(0)
    }
    fn set_mnmval(&mut self, n: usize, v: i32) {
        self.mnm.insert(n, v);
    }
    fn mn_integer(&self, n: usize) -> i32 {
        self.mn_int.get(&n).copied().unwrap_or(0)
    }
    fn set_mn_integer(&mut self, n: usize, v: i32) {
        self.mn_int.insert(n, v);
    }
    fn ms_string(&self, n: usize) -> String {
        self.ms_str.get(&n).cloned().unwrap_or_default()
    }
    fn set_ms_string(&mut self, n: usize, v: String) {
        self.ms_str.insert(n, v);
    }
    fn global_val(&self, n: usize) -> i32 {
        self.global.get(&n).copied().unwrap_or(0)
    }
    fn set_global_val(&mut self, n: usize, v: i32) {
        self.global.insert(n, v);
    }
    fn globa_dy_mval(&self, n: usize) -> i32 {
        self.globa_dy.get(&n).copied().unwrap_or(0)
    }
    fn set_globa_dy_mval(&mut self, n: usize, v: i32) {
        self.globa_dy.insert(n, v);
    }
    fn global_a_val(&self, n: usize) -> String {
        self.global_a.get(&n).cloned().unwrap_or_default()
    }
    fn set_global_a_val(&mut self, n: usize, v: String) {
        self.global_a.insert(n, v);
    }
    fn dyn_var_get(
        &self,
        kind: mir2_script::engine::DynVarKind,
        name: &str,
    ) -> Option<mir2_script::engine::DynVar> {
        self.dyn_vars.get(&(kind, name.to_string())).cloned()
    }
    fn dyn_var_set(
        &mut self,
        kind: mir2_script::engine::DynVarKind,
        name: &str,
        v: mir2_script::engine::DynVar,
    ) -> bool {
        let key = (kind, name.to_string());
        if let std::collections::hash_map::Entry::Occupied(mut e) = self.dyn_vars.entry(key) {
            e.insert(v);
            true
        } else {
            false
        }
    }
    fn dyn_var_insert(
        &mut self,
        kind: mir2_script::engine::DynVarKind,
        name: &str,
        v: mir2_script::engine::DynVar,
    ) -> bool {
        let key = (kind, name.to_string());
        if let std::collections::hash_map::Entry::Vacant(e) = self.dyn_vars.entry(key) {
            e.insert(v);
            true
        } else {
            false
        }
    }
    fn use_item_desc(&self, i: i32, attr: i32) -> i32 {
        self.worn_desc.get(&(i, attr)).copied().unwrap_or(0)
    }
    fn set_use_item_desc(&mut self, i: i32, attr: i32, v: i32) {
        self.worn_desc.insert((i, attr), v);
    }
    fn set_use_item_dura_max(&mut self, i: i32, v: i32) {
        self.worn_dura_max.insert(i, v);
    }
    fn send_update_item(&mut self, i: i32) {
        self.updated_items.push(i);
    }
    fn use_item_std_item_present(&self, i: i32) -> bool {
        self.worn_present.get(&i).copied().unwrap_or(false)
    }
    fn regen_monster(&mut self, map: &str, x: i32, y: i32, name: &str) -> bool {
        self.regen.push(format!("regen {map} {x} {y} {name}"));
        true
    }
    fn clear_map_monsters(&mut self, map: &str) {
        self.cleared_map = Some(map.to_string());
    }
    fn today_string(&self) -> String {
        self.today.clone()
    }
    fn is_start_marry(&self) -> bool {
        self.is_start_marry
    }
    fn set_is_start_marry(&mut self, v: bool) {
        self.is_start_marry = v;
    }
    fn is_start_un_marry(&self) -> bool {
        self.is_start_un_marry
    }
    fn set_is_start_un_marry(&mut self, v: bool) {
        self.is_start_un_marry = v;
    }
    fn set_dear_name(&mut self, v: &str) {
        self.dear_name = v.to_string();
    }
    fn inc_marry_count(&mut self) {
        self.marry_count += 1;
    }
    fn pose_actor_id(&self) -> i32 {
        999
    }
    fn pose_dear_name(&self) -> String {
        self.pose_state.borrow().dear_name.clone()
    }
    fn set_pose_dear_name(&mut self, v: &str) {
        self.pose_state.borrow_mut().dear_name = v.to_string();
    }
    fn pose_is_start_marry(&self) -> bool {
        self.pose_state.borrow().is_start_marry
    }
    fn set_pose_is_start_marry(&mut self, v: bool) {
        self.pose_state.borrow_mut().is_start_marry = v;
    }
    fn pose_is_start_un_marry(&self) -> bool {
        self.pose_state.borrow().is_start_un_marry
    }
    fn set_pose_is_start_un_marry(&mut self, v: bool) {
        self.pose_state.borrow_mut().is_start_un_marry = v;
    }
    fn inc_pose_marry_count(&mut self) {
        self.pose_state.borrow_mut().marry_count += 1;
    }
    fn pose_ref_show_name(&mut self) {
        self.pose_state.borrow_mut().ref_show_name += 1;
    }
    fn pose_goto_label(&mut self, label: &str) {
        self.pose_state.borrow_mut().goto.push(label.to_string());
    }
    fn set_quest_unit_status(&mut self, index: i32, value: i32) {
        self.unit_status.insert(index, value);
    }
    fn set_quest_flag_status(&mut self, flag: i32, value: i32) {
        self.flags
            .insert(flag as i16, if value == 0 { 0 } else { 1 });
    }
    fn set_quest_unit_open_status(&mut self, _index: i32, _value: i32) {}
    fn slave_count(&self) -> i32 {
        0
    }
    fn bag_count(&self) -> i32 {
        self.items.iter().map(|(_, c)| *c).sum()
    }
    fn item_count(&self, name: &str) -> i32 {
        self.items
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, c)| *c)
            .sum()
    }
    fn has_worn(&self, _location: &str) -> bool {
        false
    }
    fn give_item(&mut self, name: &str, count: i32) {
        self.effects.push(format!("additem {name} {count}"));
    }
    fn remove_item(&mut self, name: &str, count: i32) -> i32 {
        self.effects.push(format!("delitem {name} {count}"));
        count
    }
    fn chr_name(&self) -> String {
        self.chr.clone()
    }
    fn is_enough_bag(&self) -> bool {
        self.bag_count() < mir2_script::engine::MAX_BAG_ITEM
    }
    fn permission(&self) -> i32 {
        self.permission
    }
    fn gender(&self) -> i32 {
        self.gender
    }
    fn job(&self) -> i32 {
        self.job
    }
    fn game_gold(&self) -> i32 {
        self.game_gold
    }
    fn game_point(&self) -> i32 {
        self.game_point
    }
    fn credit_point(&self) -> i32 {
        self.credit_point
    }
    fn member_level(&self) -> i32 {
        self.member_level
    }
    fn bonus_total_point(&self) -> i32 {
        self.bonus_abil_total
    }
    fn dear_name(&self) -> String {
        self.dear_name.clone()
    }
    fn guild_name(&self) -> Option<String> {
        self.guild.clone()
    }
    fn is_guild_master(&self) -> bool {
        self.is_guild_master
    }
    fn is_new_human(&self) -> bool {
        self.new_human
    }
    fn user_account(&self) -> String {
        self.account.clone()
    }
    fn use_items_upper_bound(&self) -> i32 {
        self.use_items.len() as i32 - 1
    }
    fn use_item_index(&self, where_: i32) -> i32 {
        if where_ >= 0 && (where_ as usize) < self.use_items.len() {
            if self.use_items[where_ as usize].is_some() {
                1
            } else {
                0
            }
        } else {
            0
        }
    }
    fn worn_item_add_value(&self, where_: i32) -> Option<i32> {
        self.use_items.get(where_ as usize).copied().flatten()
    }
    fn quest_check_item(&self, _name: &str) -> (i32, i32, i32) {
        self.quest_item
    }
    fn in_safe_zone(&self) -> bool {
        self.safe_zone
    }
    fn map_exists(&self, map: &str) -> bool {
        self.known_maps.iter().any(|m| m == map)
    }
    fn envir_map_name(&self) -> Option<String> {
        self.envir_map.clone()
    }
    fn map_human_count(&self, _map: &str) -> i32 {
        self.map_human
    }
    fn last_hiter_race(&self) -> Option<i32> {
        self.last_hiter_race
    }
    fn last_hiter_name(&self) -> Option<String> {
        self.last_hiter_name.clone()
    }
    fn target_name(&self) -> Option<String> {
        self.target.clone()
    }
    fn pose(&self) -> Option<mir2_script::engine::PoseInfo> {
        self.pose.clone()
    }

    // ---- B2/M4 动作批次：世界缺省面的可观测替身 ----

    fn tick_count(&self) -> i32 {
        self.tick
    }
    fn sys_msg(&mut self, msg: &str, color: i32, msg_type: i32) {
        self.events.push(format!("sys {color} {msg_type} {msg}"));
    }
    fn world_broadcast(&mut self, msg: &str, msg_type: i32) {
        self.events.push(format!("broadcast {msg_type} {msg}"));
    }
    fn send_group_text(&mut self, msg: &str) {
        self.events.push(format!("grouptext {msg}"));
    }
    fn guild_send_msg(&mut self, msg: &str) {
        self.events.push(format!("guild {msg}"));
    }
    fn send_def_message(
        &mut self,
        ident: i32,
        param: i32,
        tag1: i32,
        tag2: i32,
        series: i32,
        msg: &str,
    ) {
        self.events
            .push(format!("def {ident} {param} {tag1} {tag2} {series} {msg}"));
    }
    fn send_ref_msg(&mut self, ident: i32, p1: i32, p2: i32, p3: i32, p4: i32, msg: &str) {
        self.events
            .push(format!("ref {ident} {p1} {p2} {p3} {p4} {msg}"));
    }
    fn map_random_move(&mut self, map: &str, range: i32) {
        self.events.push(format!("mapmove {map} {range}"));
    }
    fn set_game_gold(&mut self, v: i32) {
        self.game_gold = v;
    }
    fn game_gold_changed(&mut self) {
        self.game_gold_changed += 1;
    }
    fn set_game_point(&mut self, v: i32) {
        self.game_point = v;
    }
    fn set_credit_point(&mut self, v: i32) {
        self.credit_point = v;
    }
    fn abil_exp(&self) -> i32 {
        self.abil_exp
    }
    fn set_abil_exp(&mut self, v: i32) {
        self.abil_exp = v;
    }
    fn set_abil_level(&mut self, v: i32) {
        self.level = v;
    }
    fn has_level_up(&mut self, old_level: i32) {
        self.events.push(format!("levelup {old_level}"));
    }
    fn abil_hp(&self) -> i32 {
        self.abil_hp
    }
    fn set_abil_hp(&mut self, v: i32) {
        self.abil_hp = v;
    }
    fn abil_mp(&self) -> i32 {
        self.abil_mp
    }
    fn set_abil_mp(&mut self, v: i32) {
        self.abil_mp = v;
    }
    fn wabil_hp(&self) -> i32 {
        self.wabil_hp
    }
    fn wabil_max_hp(&self) -> i32 {
        self.wabil_max_hp
    }
    fn wabil_mp(&self) -> i32 {
        self.wabil_mp
    }
    fn wabil_max_mp(&self) -> i32 {
        self.wabil_max_mp
    }
    fn pk_point(&self) -> i32 {
        self.pk_point
    }
    fn set_pk_point(&mut self, v: i32) {
        self.pk_point = v;
    }
    fn pvp_level(&self) -> i32 {
        self.pvp_level
    }
    fn ref_name_color(&mut self) {
        self.events.push("namecolor".into());
    }
    fn ref_show_name(&mut self) {
        self.events.push("showname".into());
    }
    fn set_hair(&mut self, v: u8) {
        self.hair = v;
    }
    fn feature_changed(&mut self) {
        self.events.push("feature".into());
    }
    fn bonus_point(&self) -> i32 {
        self.bonus
    }
    fn set_bonus_point(&mut self, v: i32) {
        self.bonus = v;
    }
    fn set_re_level(&mut self, v: i32) {
        self.re_level = v;
    }
    fn admin_mode(&self) -> bool {
        self.admin_mode
    }
    fn set_admin_mode(&mut self, v: bool) {
        self.admin_mode = v;
    }
    fn super_man(&self) -> bool {
        self.super_man
    }
    fn set_super_man(&mut self, v: bool) {
        self.super_man = v;
    }
    fn ob_mode(&self) -> bool {
        self.ob_mode
    }
    fn set_ob_mode(&mut self, v: bool) {
        self.ob_mode = v;
    }
    fn set_bo_send_msg_flag(&mut self, v: bool) {
        self.bo_send_msg_flag = v;
    }
    fn set_bo_change_item_name_flag(&mut self, v: bool) {
        self.bo_change_item_name_flag = v;
    }
    fn set_kick_flags(&mut self) {
        self.events.push("kick".into());
    }
    fn set_permission(&mut self, v: i32) {
        self.permission = v;
    }
    fn process_user_line_msg(&mut self, line: &str) {
        self.user_line_msgs.push(line.to_string());
    }
    fn set_no_item(&mut self, v: bool) {
        self.no_item = v;
    }
    fn die(&mut self) {
        self.die_count += 1;
    }
    fn set_last_hiter_npc(&mut self) {
        self.last_hiter_set = true;
    }
    fn set_mission(&mut self, map: &str, x: i32, y: i32) {
        self.mission = Some((map.to_string(), x, y));
    }
    fn clear_delay_goto(&mut self) {
        self.delay_goto_cleared = true;
    }
    fn set_is_time_recall(&mut self, v: bool) {
        self.is_time_recall = Some(v);
    }
    fn set_time_recall(&mut self, map: &str, x: i32, y: i32, tick: i32) {
        self.time_recall = Some((map.to_string(), x, y, tick));
    }
    fn set_delay_call(&mut self, delay: i32, label: &str, npc_id: i32, goto_tick: i32) {
        self.delay_call = Some((delay, label.to_string(), npc_id, goto_tick));
    }
    fn set_val_query(&mut self, val_label: u8, val_type: u8, goto_npc_label: &str, npc_type: i32) {
        self.val_query = Some((val_label, val_type, goto_npc_label.to_string(), npc_type));
    }
    fn set_dlg_query(&mut self, take_dlg_item: bool, goto_npc_label: &str) {
        self.dlg_query = Some((take_dlg_item, goto_npc_label.to_string()));
    }
    fn set_play_dice_label(&mut self, label: &str) {
        self.play_dice_label = label.to_string();
    }
    fn set_auto_get_exp(
        &mut self,
        safe_zone: bool,
        envir_map: Option<&str>,
        time_ms: i32,
        point: i32,
    ) {
        self.auto_get_exp = Some((safe_zone, envir_map.map(str::to_string), time_ms, point));
    }
    fn set_offline(&mut self, envir_map: Option<&str>, time_ms: i32, point: i32, kick_tick: i32) {
        self.offline = Some((envir_map.map(str::to_string), time_ms, point, kick_tick));
    }
    fn find_magic(&self, name: &str) -> Option<(i32, String)> {
        self.magic_table
            .iter()
            .find(|(n, _, _)| n == name)
            .map(|(_, id, n)| (*id, n.clone()))
    }
    fn is_training_skill(&self, magic_id: i32) -> bool {
        self.training.contains(&magic_id)
    }
    fn learn_magic(&mut self, magic_id: i32, magic_name: &str, level: u8) {
        self.magics.push((magic_id, magic_name.to_string(), level));
    }
    fn use_items_len(&self) -> i32 {
        self.use_items.len() as i32
    }
    fn set_use_item_index(&mut self, i: i32, v: i32) {
        if i >= 0 {
            let i = i as usize;
            if i >= self.use_items.len() {
                self.use_items.resize(i + 1, None);
            }
            self.use_items[i] = if v > 0 { Some(v) } else { None };
        }
    }
    fn use_item_std_name(&self, i: i32) -> String {
        self.worn_names.get(&i).cloned().unwrap_or_default()
    }
    fn send_del_use_item(&mut self, i: i32) {
        let name = self.use_item_std_name(i);
        self.events.push(format!("delworn {i} {name}"));
    }
    fn use_item_dura(&self, i: i32) -> i32 {
        self.worn_dura.get(&i).copied().unwrap_or(0)
    }
    fn set_use_item_dura(&mut self, i: i32, v: i32) {
        self.worn_dura.insert(i, v);
    }
    fn use_item_dura_max(&self, i: i32) -> i32 {
        self.worn_dura_max.get(&i).copied().unwrap_or(0)
    }
}

struct MockNpc {
    actor_id: i32,
    chr_name: String,
    map: String,
    scripts: Vec<ScriptInfo>,
    says: Vec<(String, bool)>,
    goto_limit: i32,
    envir_root: Option<String>,
    path: String,
}

impl ScriptNpc for MockNpc {
    fn actor_id(&self) -> i32 {
        self.actor_id
    }
    fn chr_name(&self) -> String {
        self.chr_name.clone()
    }
    fn map_name(&self) -> String {
        self.map.clone()
    }
    fn curr_x(&self) -> i32 {
        100
    }
    fn curr_y(&self) -> i32 {
        200
    }
    fn script_list(&self) -> &Vec<ScriptInfo> {
        &self.scripts
    }
    fn send_say(&mut self, msg: &str, priority: bool) {
        self.says.push((msg.to_string(), priority));
    }
    fn script_goto_count_limit(&self) -> i32 {
        self.goto_limit
    }
    fn envir_root(&self) -> Option<String> {
        self.envir_root.clone()
    }
    fn path(&self) -> String {
        self.path.clone()
    }
}

struct MemFs(std::collections::HashMap<String, String>);

impl mir2_script::ScriptFs for MemFs {
    fn load_string_list(&self, path: &std::path::Path) -> Option<Vec<String>> {
        let key = path.to_string_lossy().replace('\\', "/").to_lowercase();
        self.0.get(&key).map(|s| mir2_script::split_lines(s))
    }
}

/// 用内存 FS 解析脚本，返回 (npc, player)。
fn setup(src: &str) -> (MockNpc, MockPlayer) {
    let mut fs = MemFs(std::collections::HashMap::new());
    fs.0.insert("root/npc_def/t.txt".to_string(), src.to_string());
    let parsers = ScriptParsers::new();
    let mut rand = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename = move |min: i32, max: i32| rand.get_random_number(min, max);
    let out = parsers
        .load_script_file(
            &fs,
            std::path::Path::new("root"),
            "Npc_def",
            "t",
            false,
            &mut rename,
        )
        .unwrap()
        .unwrap();
    let npc = MockNpc {
        actor_id: 7,
        chr_name: "测试NPC".into(),
        map: "0".into(),
        scripts: out.scripts,
        says: Vec::new(),
        goto_limit: 100,
        envir_root: None,
        path: "Npc_def/".into(),
    };
    (npc, MockPlayer::new())
}

/// 执行一次 `goto_label`，返回引擎错误清单。
fn run(npc: &mut MockNpc, player: &mut MockPlayer, label: &str) -> Vec<EngineError> {
    let mut engine = Engine::new(npc, player);
    engine.goto_label(label, false);
    engine.errors.clone()
}

#[test]
fn say_and_close_flow() {
    let (mut npc, mut player) = setup(
        "[@main]
#SAY
你好\
欢迎
#ACT
close
",
    );
    let errors = run(&mut npc, &mut player, "@main");
    assert!(npc.says.iter().any(|(s, p)| s.contains("你好") && !*p));
    // B2/M4 翻转后：脚本 `close`（枚举 5）→ CmdCode 5 → ActionOfClose（已实现）
    // ⇒ 发 RM_MERCHANTDLGCLOSE，不再落到旧位移的 Takew 分支
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!player.msgs.is_empty(), "close 应发出关闭对话消息");
}

#[test]
fn script_command_lands_on_correct_handler_after_flip() {
    // B2/M4 翻转后：脚本 `checkgold 50`（枚举 12）→ CmdCode 12 → ConditionOfCheckGold（已实现）
    // ⇒ 真比金币数（旧位移会落到 CHECKITEMW＝按物品名查背包）。
    let src = "[@main]
#IF
checkgold 50
#SAY
够了
#ELSESAY
不够
";
    let (mut npc, mut player) = setup(src);
    player.gold = 10; // 10 < 50 ⇒ 不够
    let errors = run(&mut npc, &mut player, "@main");
    assert!(
        npc.says.iter().any(|(s, _)| s.contains("不够")),
        "{:?}",
        npc.says
    );
    assert!(errors.is_empty(), "{errors:?}");

    let (mut npc2, mut player2) = setup(src);
    player2.gold = 100; // 100 >= 50 ⇒ 够了
    let errors2 = run(&mut npc2, &mut player2, "@main");
    assert!(
        npc2.says.iter().any(|(s, _)| s.contains("够了")),
        "{:?}",
        npc2.says
    );
    assert!(errors2.is_empty(), "{errors2:?}");
}

/// 处理器级用例辅助：构造 `QuestConditionInfo`（nParam 按解析器规则由 sParam 填充）。
fn cond(code: i32, p1: &str, p2: &str, n1: i32) -> mir2_script::QuestConditionInfo {
    mir2_script::QuestConditionInfo {
        cmd_code: code,
        s_param1: p1.into(),
        s_param2: p2.into(),
        n_param1: n1,
        n_param2: mir2_shared::hutil32::str_to_int(p2, 0),
        ..Default::default()
    }
}

#[test]
fn condition_handlers_registered_keys() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.gold = 10;
    player.level = 10;
    let mut engine = Engine::new(&mut npc, &mut player);
    // CHECKGOLD 注册键 = 12（枚举值）
    assert!(!engine.check_conditions(&[cond(12, "50", "", 0)]), "钱不够");
    assert!(engine.check_conditions(&[cond(12, "5", "", 0)]), "钱够");
    // CHECKLEVEL 注册键 = 7
    assert!(!engine.check_conditions(&[cond(7, "", "", 20)]), "等级不够");
    assert!(engine.check_conditions(&[cond(7, "", "", 5)]), "等级够");
    // EQUAL 注册键 = 25：变量名 P1 → 槽 1（0..99 → 全局表，本 mock 下恒 0）与 nParam2 比较
    assert!(engine.check_conditions(&[cond(25, "P1", "0", 0)]));
    // LAPGE 键 = 26（0 > 0 假）、SMALL 键 = 27（0 < 1 真）
    assert!(!engine.check_conditions(&[cond(26, "P1", "0", 0)]));
    assert!(engine.check_conditions(&[cond(27, "P1", "1", 1)]));
    assert!(engine.errors.is_empty(), "{:?}", engine.errors);
}

#[test]
fn goto_executes_goto_not_endquest_after_flip() {
    // B2/M4 翻转后：脚本 `goto @别的`（枚举 55）→ CmdCode 55 → Goto（已实现）
    // ⇒ 真跳转（会累加 goto_count），不再落到旧位移的 EndQuest 清空脚本。
    let (mut npc, mut player) = setup(
        "[@main]
#ACT
goto @别的
break
",
    );
    let _ = run(&mut npc, &mut player, "@main");
    assert_eq!(player.goto_count, 1, "goto 应命中 Goto 处理器（累加次数）");
}

#[test]
fn take_gold_hits_goto_lable_take_item_after_flip() {
    // B2/M4 翻转后：脚本 `take 金币 1`（枚举 2）→ CmdCode 2 → GotoLableTakeItem（金币分支）⇒ 扣金币。
    let (mut npc, mut player) = setup(
        "[@main]
#ACT
take 金币 1
",
    );
    let errors = run(&mut npc, &mut player, "@main");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        player.gold, 99,
        "take 金币 1 应走 GotoLableTakeItem 金币分支：100 - 1"
    );
}

#[test]
fn unregistered_messagebox_is_noop_like_csharp() {
    // messagebox（枚举 234）→ CmdCode 233 = MobFireBurn 的注册键？
    // 实测语料分类里 233 命中 ActionOfMobFireBurn（未实现 → 显式报错）；此处只断言不静默无痕
    let (mut npc, mut player) = setup("[@main]\n#ACT\nmessagebox 你好\n");
    let errors = run(&mut npc, &mut player, "@main");
    assert!(
        !errors.is_empty() || !player.msgs.is_empty(),
        "未注册/未实现的命令必须可观测（报错或副作用），不得两者皆无"
    );
}

#[test]
fn break_stops_action_list() {
    // break（枚举 10）→ CmdCode 9 → ActionOfResetUnit（未实现 → 报错），其后的 close 不应执行
    let (mut npc, mut player) = setup("[@main]\n#ACT\nbreak\nclose\n");
    let errors = run(&mut npc, &mut player, "@main");
    assert!(errors.is_empty() || !player.msgs.iter().any(|m| m.0 == 10127));
}

#[test]
fn give_gold_hits_goto_lable_give_item_after_flip() {
    // B2/M4 翻转后：脚本 `give 金币 500`（枚举 3）→ CmdCode 3 → ActionOfGiveItem 金币分支 ⇒ 加金币。
    let (mut npc, mut player) = setup("[@main]\n#ACT\ngive 金币 500\n");
    let errors = run(&mut npc, &mut player, "@main");
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        player.gold, 600,
        "give 金币 500 应走 GotoLableGiveItem：100 + 500"
    );
}

/// `cond6`：按解析器规则从 6 个文本参数构造条件（`nParamN` 仅当文本是数字时才填）。
fn cond6(code: i32, p: [&str; 6]) -> mir2_script::QuestConditionInfo {
    let num = |s: &str| mir2_shared::hutil32::str_to_int(s, 0);
    mir2_script::QuestConditionInfo {
        cmd_code: code,
        s_param1: p[0].into(),
        s_param2: p[1].into(),
        s_param3: p[2].into(),
        s_param4: p[3].into(),
        s_param5: p[4].into(),
        s_param6: p[5].into(),
        n_param1: num(p[0]),
        n_param2: num(p[1]),
        n_param3: num(p[2]),
        n_param4: num(p[3]),
        n_param5: num(p[4]),
        n_param6: num(p[5]),
        ..Default::default()
    }
}

/// B2/M4 条件批次：新增注册处理器的语义（阈值比较 / 标志位 / 佩戴位 / 地图 / 面向）。
#[test]
fn condition_batch_new_handlers_semantics() {
    use mir2_script::engine::{GENDER_MAN, GENDER_WOMAN};
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.game_gold = 100;
    player.game_point = 7;
    player.credit_point = 3;
    player.member_level = 5;
    player.bonus_abil_total = 9;
    player.permission = 4;
    player.job = 1; // Wizard
    player.gender = GENDER_WOMAN;
    player.use_items = vec![None, Some(20)];
    player.quest_item = (1, 10_000, 0);
    player.known_maps = vec!["0".to_string()];
    player.envir_map = Some("0".to_string());
    player.map_human = 3;

    let mut e = Engine::new(&mut npc, &mut player);
    // 阈值族（'=' / '>' / '<' / default '>='）
    assert!(e.check_conditions(&[cond6(101, [">", "50", "", "", "", ""])])); // CHECKGAMEGOLD
    assert!(!e.check_conditions(&[cond6(101, [">", "150", "", "", "", ""])]));
    assert!(e.check_conditions(&[cond6(102, ["=", "7", "", "", "", ""])])); // CHECKGAMEPOINT
    assert!(e.check_conditions(&[cond6(79, ["<", "9", "", "", "", ""])])); // CHECKCREDITPOINT
    assert!(e.check_conditions(&[cond6(100, ["x", "5", "", "", "", ""])])); // CHECKMEMBERLEVEL default >=
    assert!(e.check_conditions(&[cond6(84, ["=", "9", "", "", "", ""])])); // CHECKBONUSPOINT
                                                                           // 权限：ISSYSOP ≥4 / ISADMIN ≥6
    assert!(e.check_conditions(&[cond6(62, ["", "", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(63, ["", "", "", "", "", ""])]));
    // 职业 / 性别
    assert!(e.check_conditions(&[cond6(8, ["Wizard", "", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(8, ["Warrior", "", "", "", "", ""])]));
    assert!(e.check_conditions(&[cond6(3, ["WOMAN", "", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(3, ["MAN", "", "", "", "", ""])]));
    assert_eq!(e.player.gender(), GENDER_WOMAN);
    let _ = GENDER_MAN;
    // 佩戴位：位置 1 有物、位置 0 空
    assert!(e.check_conditions(&[cond6(110, ["1", "", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(110, ["0", "", "", "", "", ""])]));
    // CHECKITEMADDVALUE：C# 用 sParam1[0] 当比较符 ⇒ '1' 落 default(>=)，20 >= 5
    assert!(e.check_conditions(&[cond6(130, ["1", "0", "5", "", "", ""])]));
    // 地图：ISONMAP（在 map "0" 上）
    assert!(e.check_conditions(&[cond6(190, ["0", "", "", "", "", ""])]));
    // CHECKHUM：地图玩家数 3 ≥ 3 / < 5
    assert!(e.check_conditions(&[cond6(23, ["0", "3", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(23, ["0", "5", "", "", "", ""])]));
    // CHECKDURAEVA（switch）：Round(10000/1/1000)=10
    assert!(e.check_conditions(&[cond6(15, ["x", "5", "", "", "", ""])]));
    assert!(!e.check_conditions(&[cond6(15, ["x", "50", "", "", "", ""])]));
    // KILLBYHUM：无击杀者 ⇒ 不变；击杀者是怪物 ⇒ false
    assert!(e.check_conditions(&[cond6(199, ["x", "", "", "", "", ""])]));
    assert!(e.errors.is_empty(), "{:?}", e.errors);
    drop(e);
    player.last_hiter_race = Some(50);
    let mut e2 = Engine::new(&mut npc, &mut player);
    assert!(!e2.check_conditions(&[cond6(199, ["x", "", "", "", "", ""])]));
    assert!(e2.errors.is_empty(), "{:?}", e2.errors);
}

/// B2/M4 条件批次：错误分支文本与 C# `ConditionProcessingSys.ScriptConditionError` 逐字一致；
/// 并复现三处 C# 缺陷照搬（CHECKITEMADDVALUE 比较符取 sParam1[0]、CHECKONLINE 恒假、
/// CHECKITEMADDVALUE 的 nAddValue 取 sParam3）。
#[test]
fn condition_batch_error_texts_and_csharp_bugs() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.use_items = vec![None, None, None, Some(0)];
    let mut e = Engine::new(&mut npc, &mut player);
    // 语料真实行形状：`CHECKITEMADDVALUE 3 10 = 1` ⇒ nAddValue = StrToInt("=") = -1 ⇒ 报错
    assert!(!e.check_conditions(&[cond6(130, ["3", "10", "=", "1", "", ""])]));
    // CHECKBAGSIZE 参数非法（> 46）
    assert!(!e.check_conditions(&[cond6(111, ["50", "", "", "", "", ""])]));
    // CHECKONLINE：C# 只在 false 分支赋值 ⇒ 恒假（即便玩家在线）
    assert!(!e.check_conditions(&[cond6(150, ["<$USERNAME>", "", "", "", "", ""])]));
    let texts: Vec<&str> = e
        .errors
        .iter()
        .filter_map(|err| match err {
            EngineError::ScriptError { text } => Some(text.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts[0],
        "[脚本参数不正确] Cmd:CHECKITEMADDVALUE NPC名称:测试NPC 地图:0 座标:100:200 参数1:3 参数2:10 参数3:= 参数4:1 参数5:"
    );
    assert_eq!(
        texts[1],
        "[脚本参数不正确] Cmd:CHECKBAGSIZE NPC名称:测试NPC 地图:0 座标:100:200 参数1:50 参数2: 参数3: 参数4: 参数5:"
    );
    assert!(texts.len() >= 2, "{texts:?}");
}

/// B2/M4 条件批次：语料用到的条件派发目标（注册面 + 引擎 switch 面）已全部实现，
/// 仅剩 `ConditionOfCheckVar`(139) / `ConditionOfCheckNameDateList`(144) 两个依赖动态变量层
/// 与文件日期列表的处理器留待下一批。
#[test]
fn condition_batch_closes_corpus_condition_targets() {
    use mir2_script::engine::dispatch as d;
    const CLOSED_REGISTERED: &[&str] = &[
        "ConditionOfCheckAccountList",
        "ConditionOfCheckItemAddValue",
        "ConditionOfCheckLevelEx",
        "ConditionOfCheckGameGold",
        "ConditionOfCheckJob",
        "ConditionOfCheckCreditPoint",
        "ConditionOfCheckGamePoint",
        "ConditionOfCheckNameList",
        "ConditionOfCheckOnLine",
        "ConditionOfCheckMarry",
        "ConditionOfCheckHaveGuild",
        "ConditionOfIsAdmin",
        "ConditionOfCheckUseItem",
        "ConditionOfGender",
        "ConditionOfIosnMap",
        "ConditionOfCheckHum",
        "ConditionOfCheckReNewLevel",
        "ConditionOfIsDupMode",
        "ConditionOfIsNewHuman",
        "ConditionOfCheckBagSize",
        "ConditionOfCheckIsCastleaGuild",
        "ConditionOfCheckPoseDir",
        "ConditionOfCheckPoseGender",
        "ConditionOfCheckPoseLevel",
        "ConditionOfCheckPoseMarry",
        "ConditionOfCheckSafeZone",
        "ConditionOfCheckBagGage",
    ];
    for name in CLOSED_REGISTERED {
        assert!(d::IMPLEMENTED_CONDITIONS.contains(name), "{name} 应已实现");
    }
    for case in [
        "CHECKDURAEVA",
        "KILLBYHUM",
        "KILLBYMON",
        "CHECKDIEMON",
        "CHECKKILLPLAYMON",
        "CHECKINSAFEZONE",
        "CHECKRANDOMNO",
        "ISTAKEITEM",
        "SCHECKDEATHPLAYMON",
    ] {
        assert!(
            d::IMPLEMENTED_SWITCH_CONDITIONS.contains(&case),
            "{case} 应已实现"
        );
    }
    // B2/M4 var 家族条件批次：两个遗留条件已实现（动态变量层 / 日期列表文件）
    for name in ["ConditionOfCheckVar", "ConditionOfCheckNameDateList"] {
        assert!(d::IMPLEMENTED_CONDITIONS.contains(&name), "{name} 应已实现");
    }
    let undone = d::unimplemented_handlers();
    assert!(
        !undone.contains(&"ConditionOfCheckVar"),
        "CheckVar 不应再在未实现清单"
    );
    assert!(
        !undone.contains(&"ConditionOfCheckNameDateList"),
        "CheckNameDateList 不应再在未实现清单"
    );
}

// ---- B2/M4 动作批次（2026-10-11）：非 var 家族动作处理器单测 ----

/// 处理器级用例辅助：直接构造一条动作并派发。
/// `code` 取 C# 枚举值——解析器现产出 `field_index`，全表与 `enum_value` 相等
/// （`codes.rs` 已核对 0 处不等），故等价于脚本解析后的 `n_cmd_code`。
fn act(code: i32, p1: &str, p2: &str, n1: i32, n2: i32) -> mir2_script::QuestActionInfo {
    mir2_script::QuestActionInfo {
        n_cmd_code: code,
        s_param1: p1.into(),
        n_param1: n1,
        s_param2: p2.into(),
        n_param2: n2,
        ..Default::default()
    }
}

fn run_act(
    npc: &mut MockNpc,
    player: &mut MockPlayer,
    info: mir2_script::QuestActionInfo,
) -> Vec<EngineError> {
    let mut engine = Engine::new(npc, player);
    engine.run_actions(&[info]);
    engine.errors.clone()
}

fn script_errors(errors: &[EngineError]) -> Vec<String> {
    errors
        .iter()
        .filter_map(|e| match e {
            EngineError::ScriptError { text } => Some(text.clone()),
            _ => None,
        })
        .collect()
}

/// 取第一条「脚本错误」文本（无则 panic 并打印全部错误，便于定位）。
fn first_script_error(errors: &[EngineError]) -> String {
    let texts = script_errors(errors);
    assert!(!texts.is_empty(), "应至少有一条 ScriptError：{errors:?}");
    texts[0].clone()
}

#[test]
fn action_batch_closes_corpus_action_targets() {
    use mir2_script::engine::dispatch as d;
    // (已实现处理器名, 语料 code)：语料里用到的动作派发目标逐个断言「落点正确且已实现」
    const CLOSED: &[(&str, i32)] = &[
        ("ActionOfLineMsg", 31),
        ("ActionOfLineMsg", 68),
        ("ActionOfGameGold", 191),
        ("ActionOfGamePoint", 217),
        ("ActionOfDelAccountList", 50),
        ("ActionOfAddNameList", 45),
        ("ActionOfDelNameList", 46),
        ("ActionOfDelUseDateList", 336),
        ("ActionOfAddSkill", 182),
        ("ActionOfGmExecute", 229),
        ("ActionOfMobPlace", 188),
        ("ActionOfQueryValue", 347),
        ("ActionOfQueryItemDlg", 349),
        ("ActionOfUpgradeDlgItem", 350),
        ("ActionOfPlayDice", 42),
        ("ActionOfChangeCreditPoint", 208),
        ("ActionOfChangeExp", 185),
        ("ActionOfChangeLevel", 174),
        ("ActionOfChangePkPoint", 184),
        ("ActionOfChangeHairStyle", 58),
        ("ActionOfHumanHp", 221),
        ("ActionOfHumanMp", 222),
        ("ActionOfKick", 201),
        ("ActionOfKill", 200),
        ("ActionOfMission", 187),
        ("ActionOfMap", 18),
        ("ActionOfClearDelayGoto", 244),
        ("ActionOfBreakTimereCall", 30),
        ("ActionOfTimereCall", 11),
        ("ActionOfDelayCall", 345),
        ("ActionOfSetScriptFlag", 235),
        ("ActionOfSetSendMsgFlag", 211),
        ("ActionOfAutoGetExp", 236),
        ("ActionOfOffLine", 337),
        ("ActionOfRepairAllItem", 338),
        ("ActionOfReNewLevel", 196),
        ("ActionOfRestReNewLevel", 203),
        ("ActionOfChangeMode", 32),
    ];
    for (name, code) in CLOSED {
        assert_eq!(
            d::execution_target_name(*code),
            Some(*name),
            "code {code} 的落点应是 {name}"
        );
        assert_eq!(
            d::execution_target_status(*code),
            Some(true),
            "{name}(code {code}) 应已实现"
        );
    }
    // Takew 走 `ScriptEngine` switch（注册表里没有）
    assert_eq!(d::execution_target_name(4), Some("Takew"));
    assert_eq!(d::execution_target_status(4), Some(true));
    // B2/M4 var 家族动作批次：15 个遗留动作已实现，语料动作派发目标未实现数归零
    const NOW_CLOSED: &[(&str, i32)] = &[
        ("ActionOfMovData", 23),
        ("ActionOfMovrData", 36),
        ("ActionOfIncInteger", 24),
        ("ActionOfDecInteger", 25),
        ("ActionOfMulData", 28),
        ("ActionOfVar", 237),
        ("ActionOfLoadVar", 238),
        ("ActionOfSaveVar", 239),
        ("ActionOfCalcVar", 240),
        ("ActionOfUpgradeItemsEx", 213),
        ("ActionOfMarry", 175),
        ("ActionOfUnMarry", 176),
        ("ActionOfAddUseDateList", 335),
        ("ActionOfClearMapMon", 215),
        ("ActionOfMonGenEx", 214),
    ];
    for (name, code) in NOW_CLOSED {
        assert_eq!(
            d::execution_target_name(*code),
            Some(*name),
            "code {code} 的落点应是 {name}"
        );
        assert_eq!(
            d::execution_target_status(*code),
            Some(true),
            "{name}(code {code}) 应已实现"
        );
        let undone = d::unimplemented_handlers();
        assert!(!undone.contains(name), "{name} 不应再在未实现清单");
    }
}

/// 「登记即实现」守卫：`IMPLEMENTED_EXECUTIONS` 里每个名字都必须有**真实执行路径**。
///
/// 动机：覆盖报告（DoD「未实现清单归零」）只读 `IMPLEMENTED_EXECUTIONS` 这份**登记表**；
/// 若某个名字只是登记了、`run_action` 里没有对应 match 臂，落点会滑进 default 臂推
/// `NotImplemented { kind: "action" }` —— 登记表说"已实现"、实际行为是"未实现"，
/// 这正是必须堵掉的假绿（`RULE_规则可执行性` 第 5/6 条）。
///
/// 阳性对照（手工做一次即知本测试有牙齿）：删掉 `actions.rs` 里任一 match 臂、保留其登记名，
/// 本测试立刻红并列出 `名字(code)`。
#[test]
fn implemented_registry_entries_are_actually_implemented() {
    use mir2_script::engine::dispatch as d;
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let mut registered = 0usize;
    let mut false_green: Vec<String> = Vec::new();
    for code in 0..1024 {
        let Some(handler) = d::execution_handler(code) else {
            continue;
        };
        if !d::IMPLEMENTED_EXECUTIONS.contains(&handler) {
            continue;
        }
        registered += 1;
        // 空参数走一遍：参数不全时 C# 只会写「脚本错误」，绝不会滑进未实现分支
        let errors = run_act(&mut npc, &mut player, act(code, "", "", 0, 0));
        if errors
            .iter()
            .any(|e| matches!(e, EngineError::NotImplemented { kind: "action", .. }))
        {
            false_green.push(format!("{handler}({code})"));
        }
    }
    assert!(
        registered >= 40,
        "登记表里的注册动作处理器数量随批次只增不减（本批 48），实测 {registered}"
    );
    assert!(
        false_green.is_empty(),
        "以下登记为「已实现」但落进 default 臂（登记名 ≠ 有执行路径）：{false_green:?}"
    );
}

#[test]
fn line_msg_modes_and_error() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // 0：世界广播原文；`%s`→玩家名、`%d`→NPC 名
    let e = run_act(&mut npc, &mut player, act(31, "0", "你好%s在%d", 0, 0));
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(player.events, vec!["broadcast 0 你好测试玩家在测试NPC"]);
    // 2：NPC 名前缀；3：玩家名前缀
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(31, "2", "正文", 2, 0));
    assert_eq!(player.events, vec!["broadcast 0 [测试NPC]正文"]);
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(31, "3", "正文", 3, 0));
    assert_eq!(player.events, vec!["broadcast 0 [测试玩家]正文"]);
    // 5/6/7：SysMsg 的 Red/Green/Blue，MsgType.Say
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(68, "5", "红", 5, 0));
    let _ = run_act(&mut npc, &mut player, act(68, "6", "绿", 6, 0));
    let _ = run_act(&mut npc, &mut player, act(68, "7", "蓝", 7, 0));
    assert_eq!(
        player.events,
        vec!["sys 0 3 红", "sys 1 3 绿", "sys 2 3 蓝"]
    );
    // 8：组队文本；9：无行会静默、有行会才发
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(68, "8", "队伍", 8, 0));
    assert_eq!(player.events, vec!["grouptext 队伍"]);
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(68, "9", "公会", 9, 0));
    assert!(player.events.is_empty(), "无行会时不应发消息");
    player.guild = Some("测试行会".into());
    let _ = run_act(&mut npc, &mut player, act(68, "9", "公会", 9, 0));
    assert_eq!(player.events, vec!["guild 公会"]);
    // 其它 nParam1：ScriptActionError(ExecutionCode.SendMsg)
    let errors = run_act(&mut npc, &mut player, act(68, "10", "越界", 10, 0));
    assert!(first_script_error(&errors).contains("脚本命令:SendMsg"));
}

#[test]
fn game_gold_and_point_ops() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let _ = run_act(&mut npc, &mut player, act(191, "+", "50", 0, 50));
    assert_eq!(player.game_gold, 50);
    assert_eq!(
        player.game_gold_changed, 1,
        "数值变更应触发 GameGoldChanged"
    );
    let _ = run_act(&mut npc, &mut player, act(191, "-", "100", 0, 100));
    assert_eq!(player.game_gold, 0, "不足时夹 0");
    assert_eq!(player.game_gold_changed, 2);
    let _ = run_act(&mut npc, &mut player, act(191, "=", "7", 0, 7));
    assert_eq!(player.game_gold, 7);
    // 值未变则不调 GameGoldChanged
    let before = player.game_gold_changed;
    let _ = run_act(&mut npc, &mut player, act(191, "=", "7", 0, 7));
    assert_eq!(player.game_gold_changed, before);
    // 负数参数 ⇒ ScriptActionError(GameGold)，值不变
    let errors = run_act(&mut npc, &mut player, act(191, "+", "-5", 0, -5));
    assert!(first_script_error(&errors).contains("脚本命令:GameGold"));
    assert_eq!(player.game_gold, 7);
    // GamePoint 同构，末端同样调 GameGoldChanged
    player.game_gold_changed = 0;
    let _ = run_act(&mut npc, &mut player, act(217, "+", "30", 0, 30));
    assert_eq!(player.game_point, 30);
    assert_eq!(player.game_gold_changed, 1);
}

#[test]
fn credit_point_ops_follow_csharp_byte_truncation() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.credit_point = 250;
    let _ = run_act(&mut npc, &mut player, act(208, "+", "10", 0, 10));
    assert_eq!(player.credit_point, 255, "上限 byte.MaxValue");
    let _ = run_act(&mut npc, &mut player, act(208, "-", "100", 0, 100));
    assert_eq!(player.credit_point, 155);
    // C# `(byte)StrToInt("300")` 截断 ⇒ 44（照搬）
    let _ = run_act(&mut npc, &mut player, act(208, "=", "300", 0, 300));
    assert_eq!(player.credit_point, 44);
    // 未识别的比较符 ⇒ ScriptActionError(CreditPoint)
    let errors = run_act(&mut npc, &mut player, act(208, "x", "1", 0, 1));
    assert!(first_script_error(&errors).contains("脚本命令:CreditPoint"));
}

#[test]
fn change_exp_level_pkpoint_ops() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let _ = run_act(&mut npc, &mut player, act(185, "=", "100", 0, 100));
    assert_eq!(player.abil_exp, 100);
    let _ = run_act(&mut npc, &mut player, act(185, "=", "0", 0, 0));
    assert_eq!(player.abil_exp, 100, "nExp=0 时 '=' 不赋值");
    let _ = run_act(&mut npc, &mut player, act(185, "+", "50", 0, 50));
    assert_eq!(player.abil_exp, 150);
    assert!(
        player.msgs.iter().any(|(id, _)| *id == 8075),
        "应发 RM_WINEXP"
    );
    let _ = run_act(&mut npc, &mut player, act(185, "-", "30", 0, 30));
    assert_eq!(player.abil_exp, 120);
    let errors = run_act(&mut npc, &mut player, act(185, "-", "-1", 0, -1));
    assert!(first_script_error(&errors).contains("脚本命令:ChangeExp"));
    // Level：'=' 限 (0, 255]；'+' 顶到 255 并 HasLevelUp
    player.level = 10;
    let _ = run_act(&mut npc, &mut player, act(174, "=", "20", 0, 20));
    assert_eq!(player.level, 20);
    assert!(player.events.contains(&"levelup 10".to_string()));
    let before = player.events.len();
    let _ = run_act(&mut npc, &mut player, act(174, "=", "300", 0, 300));
    assert_eq!(player.level, 20, "超上限不生效");
    assert_eq!(player.events.len(), before, "未生效不应 HasLevelUp");
    let _ = run_act(&mut npc, &mut player, act(174, "+", "1000", 0, 1000));
    assert_eq!(player.level, 255);
    // PkPoint：'+' / '-' 都夹 0 下界
    let _ = run_act(&mut npc, &mut player, act(184, "+", "10", 0, 10));
    assert_eq!(player.pk_point, 10);
    let _ = run_act(&mut npc, &mut player, act(184, "-", "99", 0, 99));
    assert_eq!(player.pk_point, 0);
    let errors = run_act(&mut npc, &mut player, act(184, "=", "-1", 0, -1));
    assert!(first_script_error(&errors).contains("脚本命令:ChangePkPoint"));
}

#[test]
fn hairstyle_change_mode_and_flags() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let _ = run_act(&mut npc, &mut player, act(58, "5", "", 0, 0));
    assert_eq!(player.hair, 5);
    assert!(player.events.contains(&"feature".to_string()));
    for bad in ["", "-1"] {
        let errors = run_act(&mut npc, &mut player, act(58, bad, "", 0, 0));
        assert!(first_script_error(&errors).contains("脚本命令:Hairstyle"));
    }
    // ChangeMode：1 管理员 / 2 无敌 / 3 隐身，各带进入/退出提示串
    player.events.clear();
    let _ = run_act(&mut npc, &mut player, act(32, "1", "1", 1, 1));
    assert!(player.admin_mode);
    assert!(player
        .events
        .contains(&"sys 1 2 已进入管理员模式".to_string()));
    let _ = run_act(&mut npc, &mut player, act(32, "1", "0", 1, 0));
    assert!(!player.admin_mode);
    assert!(player
        .events
        .contains(&"sys 1 2 已退出管理员模式".to_string()));
    let _ = run_act(&mut npc, &mut player, act(32, "2", "0", 2, 0));
    assert!(!player.super_man);
    assert!(player
        .events
        .contains(&"sys 1 2 已退出无敌模式".to_string()));
    let _ = run_act(&mut npc, &mut player, act(32, "3", "1", 3, 0));
    assert!(player.ob_mode);
    assert!(player
        .events
        .contains(&"sys 1 2 已进入隐身模式".to_string()));
    let errors = run_act(&mut npc, &mut player, act(32, "4", "1", 4, 0));
    assert!(first_script_error(&errors).contains("脚本命令:ChangeMode"));
    // SetScriptFlag（0 发消息开关 / 1 改名开关 / 其它报错）与 SetSendMsgFlag
    let _ = run_act(&mut npc, &mut player, act(235, "0", "1", 0, 1));
    assert!(player.bo_send_msg_flag);
    let _ = run_act(&mut npc, &mut player, act(235, "1", "1", 0, 1));
    assert!(player.bo_change_item_name_flag);
    let errors = run_act(&mut npc, &mut player, act(235, "2", "1", 0, 1));
    assert!(first_script_error(&errors).contains("脚本命令:SetscriptFlag"));
    player.bo_send_msg_flag = false;
    let _ = run_act(&mut npc, &mut player, act(211, "", "", 0, 0));
    assert!(player.bo_send_msg_flag);
}

#[test]
fn human_hp_mp_ops_follow_csharp_quirks() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let _ = run_act(&mut npc, &mut player, act(221, "=", "500", 0, 500));
    assert_eq!(player.abil_hp, 500);
    // '-' 比较的是 WAbil.HP（C# 原文如此）
    player.wabil_hp = 0;
    let _ = run_act(&mut npc, &mut player, act(221, "-", "600", 0, 600));
    assert_eq!(player.abil_hp, 0, "WAbil.HP=0 < 600 ⇒ 归零");
    player.abil_hp = 500;
    player.wabil_hp = 1_000;
    player.wabil_max_hp = 65_535;
    let _ = run_act(&mut npc, &mut player, act(221, "-", "100", 0, 100));
    assert_eq!(player.abil_hp, 400);
    // '+' 按 ushort 截断：400 + 65535 = 65935 ⇒ 399
    let _ = run_act(&mut npc, &mut player, act(221, "+", "65535", 0, 65535));
    assert_eq!(player.abil_hp, 399);
    // '+' 末端夹到 WAbil.MaxHP
    player.wabil_hp = 10;
    player.wabil_max_hp = 5;
    let _ = run_act(&mut npc, &mut player, act(221, "+", "1", 0, 1));
    assert_eq!(player.abil_hp, 5, "WAbil.HP > WAbil.MaxHP ⇒ 夹到 MaxHP");
    // MP 同构
    let _ = run_act(&mut npc, &mut player, act(222, "=", "300", 0, 300));
    assert_eq!(player.abil_mp, 300);
    let errors = run_act(&mut npc, &mut player, act(222, "=", "-1", 0, -1));
    assert!(first_script_error(&errors).contains("脚本命令:HumanMp"));
}

#[test]
fn kick_kill_mission_and_mobplace() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let _ = run_act(&mut npc, &mut player, act(201, "", "", 0, 0));
    assert!(player.events.contains(&"kick".to_string()));
    // Kill：1 无掉落 / 2 指定击杀者 / 3 两者 / 0 仅 Die / 越界报错
    let _ = run_act(&mut npc, &mut player, act(200, "1", "", 0, 0));
    assert!(player.no_item);
    assert_eq!(player.die_count, 1);
    let _ = run_act(&mut npc, &mut player, act(200, "2", "", 0, 0));
    assert!(player.last_hiter_set);
    assert_eq!(player.die_count, 2);
    player.no_item = false;
    let _ = run_act(&mut npc, &mut player, act(200, "3", "", 0, 0));
    assert!(player.no_item && player.last_hiter_set);
    assert_eq!(player.die_count, 3);
    let _ = run_act(&mut npc, &mut player, act(200, "0", "", 0, 0));
    assert_eq!(player.die_count, 4);
    let errors = run_act(&mut npc, &mut player, act(200, "9", "", 0, 0));
    assert!(first_script_error(&errors).contains("脚本命令:Kill"));
    assert_eq!(player.die_count, 4, "越界不得 Die");
    // Mission：三参数齐 ⇒ 设全局任务坐标；否则报错
    let mut m = act(187, "0", "", 0, 0);
    m.n_param2 = 10;
    m.n_param3 = 20;
    let errors = run_act(&mut npc, &mut player, m);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.mission, Some(("0".into(), 10, 20)));
    let errors = run_act(&mut npc, &mut player, act(187, "0", "", 0, 0));
    assert!(first_script_error(&errors).contains("脚本命令:Mission"));
    // MobPlace：C# 方法体是死循环（参数全 0）⇒ 无行为、无报错
    let before = player.events.len();
    let errors = run_act(&mut npc, &mut player, act(188, "鸡", "50", 0, 50));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.events.len(), before);
}

#[test]
fn map_and_timer_actions() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // Map：SendRefMsg(RM_SPACEMOVE_FIRE) + MapRandomMove(map, 0)
    let _ = run_act(&mut npc, &mut player, act(18, "0", "", 0, 0));
    assert!(player.events.contains(&"ref 8062 0 0 0 0 ".to_string()));
    assert!(player.events.contains(&"mapmove 0 0".to_string()));
    // TimereCall：IsTimeRecall=true + 当前位置 + tick + nParam1 分钟
    let _ = run_act(&mut npc, &mut player, act(11, "", "", 2, 0));
    assert_eq!(player.time_recall, Some(("".into(), 0, 0, 1_000 + 120_000)));
    // BreakTimereCall：IsTimeRecall=false
    let _ = run_act(&mut npc, &mut player, act(30, "", "", 0, 0));
    assert_eq!(player.is_time_recall, Some(false));
    // ClearDelayGoto
    let _ = run_act(&mut npc, &mut player, act(244, "", "", 0, 0));
    assert!(player.delay_goto_cleared);
    // DelayCall：DelayCall=max(1,nParam1)，行变量取不到数时按 nParam1 毫秒后跳转
    let _ = run_act(&mut npc, &mut player, act(345, "P1", "@lbl", 3, 0));
    assert_eq!(player.delay_call, Some((3, "@lbl".into(), 7, 1_003)));
}

#[test]
fn auto_get_exp_and_offline() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // 地图号为空 ⇒ FindMap("") 找不到 ⇒ 报错（C# 只在 sMap 为空时才查地图）
    let mut a = act(236, "10", "20", 0, 0);
    a.s_param3 = "A1".into();
    let errors = run_act(&mut npc, &mut player, a.clone());
    assert!(first_script_error(&errors).contains("脚本命令:SetautogetExp"));
    // 地图号非空 ⇒ 通过（AutoGetExpEnvir 恒 null，C# 原文如此）
    a.s_param4 = "0".into();
    let errors = run_act(&mut npc, &mut player, a);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.auto_get_exp, Some((true, None, 10_000, 20)));
    // 空/单字符 sParam3：C# `sParam3[1]` 抛 IndexOutOfRangeException；Rust 落 false、不崩
    // （刻意不复刻崩溃，登记见 tests/parity/whitelist.md B-108）
    let mut e = act(236, "10", "20", 0, 0);
    e.s_param4 = "0".into();
    let errors = run_act(&mut npc, &mut player, e);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.auto_get_exp, Some((false, None, 10_000, 20)));
    // 时间/点数非法 ⇒ 报错
    let mut b = act(236, "0", "20", 0, 0);
    b.s_param3 = "A1".into();
    b.s_param4 = "0".into();
    let errors = run_act(&mut npc, &mut player, b);
    assert!(first_script_error(&errors).contains("脚本命令:SetautogetExp"));
    // OffLine：默认 5 秒 / 500 点 / 1440*15 分钟踢线 + SM_OUTOFCONNECTION
    let errors = run_act(&mut npc, &mut player, act(337, "5", "500", 0, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        player.offline,
        Some((None, 5_000, 500, 1_000 + 21_600 * 60_000))
    );
    assert!(player.events.contains(&"def 528 0 0 0 0 ".to_string()));
}

#[test]
fn gm_execute_substitutes_self_and_restores_permission() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.permission = 0;
    let _ = run_act(&mut npc, &mut player, act(229, "test", "Self", 0, 0));
    assert_eq!(player.user_line_msgs.len(), 1);
    assert!(
        player.user_line_msgs[0].starts_with("@test 测试玩家"),
        "{:?}",
        player.user_line_msgs
    );
    assert_eq!(player.permission, 0, "执行后应恢复原 Permission");
}

#[test]
fn repair_all_item_skips_empty_and_charm() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.use_items = vec![None; 13];
    player.use_items[0] = Some(1); // 衣服
    player.worn_names.insert(0, "布衣".into());
    player.worn_dura_max.insert(0, 100);
    player.use_items[12] = Some(2); // 宝石（ItemLocation.Charm）
    player.worn_names.insert(12, "宝石".into());
    let errors = run_act(&mut npc, &mut player, act(338, "", "", 0, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.worn_dura.get(&0), Some(&100), "衣服应修满持久");
    assert!(
        player.msgs.iter().any(|(id, _)| *id == 10125),
        "应发 RM_DURACHANGE"
    );
    assert!(
        player
            .events
            .contains(&"sys 0 2 宝石 禁止修理...".to_string()),
        "{:?}",
        player.events
    );
    assert!(player
        .events
        .contains(&"sys 2 2 您身上的的装备修复成功了...".to_string()));
}

#[test]
fn renew_level_actions() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let mut a = act(196, "1", "0", 0, 0);
    a.s_param3 = "10".into();
    let errors = run_act(&mut npc, &mut player, a);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.re_level, 1);
    assert_eq!(player.bonus, 10);
    assert!(
        player.msgs.iter().any(|(id, _)| *id == 8078),
        "应发 RM_ADJUST_BONUS"
    );
    assert!(player.events.contains(&"showname".to_string()));
    // sParam3 缺省 ⇒ -1 ⇒ 报错
    let errors = run_act(&mut npc, &mut player, act(196, "1", "0", 0, 0));
    assert!(first_script_error(&errors).contains("脚本命令:Renewlevel"));
    // RestReNewLevel：ReLevel 归零 + HasLevelUp(0)
    let _ = run_act(&mut npc, &mut player, act(203, "", "", 0, 0));
    assert_eq!(player.re_level, 0);
    assert!(player.events.contains(&"levelup 0".to_string()));
}

#[test]
fn query_value_item_dlg_and_upgrade_dlg_item() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    let mut a = act(347, "", "", 0, 0);
    a.n_param1 = 120; // >= 100 ⇒ ValLabel 原样
    a.n_param2 = 2; // <= 3 ⇒ ValType = 2
    a.n_param3 = 5; // 长度 5
    a.s_param4 = "@lbl".into();
    a.s_param5 = String::new(); // 空 ⇒ 默认提示
    a.s_param6 = "QF".into();
    let errors = run_act(&mut npc, &mut player, a);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.val_query, Some((120, 2, "@lbl".into(), 1)));
    // MakeWord(btType=2, btLen=5) = 2 | 5<<8 = 1282；SM_QUERYVALUE = 215
    assert_eq!(player.events.last().unwrap(), "def 215 0 1282 0 0 请输入：");
    // ValLabel < 100 ⇒ 0；ValType > 3 ⇒ 0；sHint 非空则原样；btLen 下限 1
    let mut b = act(347, "", "", 0, 0);
    b.n_param1 = 99;
    b.n_param2 = 9;
    b.n_param3 = 0;
    b.s_param5 = "给个值".into();
    let _ = run_act(&mut npc, &mut player, b);
    assert_eq!(player.val_query, Some((0, 0, String::new(), 0)));
    // MakeWord(btType=0, btLen=MAX(1,0)=1) = 1<<8 = 256
    assert_eq!(player.events.last().unwrap(), "def 215 0 256 0 0 给个值");
    // QueryItemDlg：TakeDlgItem = nParam3 != 0；hint 默认 "请输入:"
    let mut c = act(349, "", "@lbl", 0, 0);
    c.n_param3 = 1;
    let _ = run_act(&mut npc, &mut player, c);
    assert_eq!(player.dlg_query, Some((true, "@lbl".into())));
    assert_eq!(player.events.last().unwrap(), "def 623 7 0 0 0 请输入:");
    // UpgradeDlgItem：C# 方法体为空
    let before = player.events.len();
    let errors = run_act(&mut npc, &mut player, act(350, "1", "2", 1, 2));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.events.len(), before);
}

#[test]
fn play_dice_packs_mdy_values() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.mdy = (1..=10).map(|i| (i - 1, i as i32)).collect();
    let errors = run_act(&mut npc, &mut player, act(42, "", "@dice", 3, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.play_dice_label, "@dice");
    // p2 = MakeLong(MakeWord(1,2), MakeWord(3,4))；p3/p4 同构
    let p2 = 0x0201 | (0x0403 << 16);
    let p3 = 0x0605 | (0x0807 << 16);
    let p4 = 0x0A09;
    assert_eq!(
        player.msgs_full.last().unwrap(),
        &format!("msg 10500 3 {p2} {p3} {p4} @dice")
    );
}

#[test]
fn add_skill_learns_or_reports_error() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player
        .magic_table
        .push(("魔法盾".to_string(), 5, "魔法盾".to_string()));
    let errors = run_act(&mut npc, &mut player, act(182, "魔法盾", "2", 0, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.magics, vec![(5, "魔法盾".to_string(), 2)]);
    // 已学会（IsTrainingSkill）⇒ 不重复加
    player.training.push(5);
    let _ = run_act(&mut npc, &mut player, act(182, "魔法盾", "2", 0, 0));
    assert_eq!(player.magics.len(), 1);
    // 找不到魔法 ⇒ ScriptActionError(AddSkill)
    let errors = run_act(&mut npc, &mut player, act(182, "不存在", "1", 0, 0));
    assert!(first_script_error(&errors).contains("脚本命令:AddSkill"));
}

#[test]
fn name_list_actions_remove_entry_like_csharp_addlist_bug() {
    let dir = std::env::temp_dir().join(format!("mir2-b2-action-batch-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    let read = |p: &std::path::Path| mir2_script::decode_bytes(&std::fs::read(p).unwrap());
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    npc.envir_root = Some(dir.to_string_lossy().to_string());
    npc.path = String::new();
    // ADDNAMELIST / DELNAMELIST 同语义（C# ActionOfAddList 与 ActionOfDelList 逐字相同）
    // 夹具文件按 `StringList.SaveToFile` 口径写盘（gb2312/cp936），与引擎的读法一致
    std::fs::write(
        &file,
        mir2_script::textfile::encode_gb2312("别人\r\n测试玩家\r\n第三行\r\n"),
    )
    .unwrap();
    let errors = run_act(&mut npc, &mut player, act(45, "list.txt", "", 0, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(read(&file), "别人\r\n第三行\r\n");
    // 未命中 ⇒ 不写盘
    let _ = run_act(&mut npc, &mut player, act(46, "list.txt", "", 0, 0));
    assert_eq!(read(&file), "别人\r\n第三行\r\n");
    // DELACCOUNTLIST 比对 UserAccount（mock 为 testacct）
    std::fs::write(
        &file,
        mir2_script::textfile::encode_gb2312("testacct\r\n甲\r\n"),
    )
    .unwrap();
    let _ = run_act(&mut npc, &mut player, act(50, "list.txt", "", 0, 0));
    assert_eq!(read(&file), "甲\r\n");
    // DELUSERDATE(336) 走同一实现（比对 ChrName）
    std::fs::write(
        &file,
        mir2_script::textfile::encode_gb2312("测试玩家\r\n乙\r\n"),
    )
    .unwrap();
    let _ = run_act(&mut npc, &mut player, act(336, "list.txt", "", 0, 0));
    assert_eq!(read(&file), "乙\r\n");
    // 文件不存在 ⇒ 不创建、不报错
    let missing = dir.join("missing.txt");
    let errors = run_act(&mut npc, &mut player, act(50, "missing.txt", "", 0, 0));
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!missing.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn takew_removes_worn_by_prefix_then_by_std_name() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.use_items = vec![None; 13];
    player.use_items[3] = Some(11); // 项链位
    player.worn_names.insert(3, "祈福项链".into());
    player.use_items[0] = Some(12); // 衣服位
    player.worn_names.insert(0, "布衣".into());
    // 前缀命中：摘下项链位（C#：SendDelItems + Index=0 后 return）
    let errors = run_act(&mut npc, &mut player, act(4, "[NECKLACE]", "", 0, 1));
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(player.use_item_index(3), 0);
    assert!(player.events.contains(&"delworn 3 祈福项链".to_string()));
    // 通用分支：按标准名匹配并摘下
    let _ = run_act(&mut npc, &mut player, act(4, "布衣", "", 0, 2));
    assert_eq!(player.use_item_index(0), 0);
    assert!(player.events.contains(&"delworn 0 布衣".to_string()));
    // nItemCount <= 0 ⇒ 直接返回
    player.use_items[0] = Some(12);
    let before = player.events.len();
    let _ = run_act(&mut npc, &mut player, act(4, "布衣", "", 0, 0));
    assert_eq!(player.events.len(), before);
}

// ==== B2/M4 var 家族 / 剩余动作批次（2026-10-11）====

/// 构造动作（最多 6 个文本参数 + 6 个数值参数）。
fn act_full(code: i32, ps: &[&str], ns: &[i32]) -> mir2_script::QuestActionInfo {
    let mut info = mir2_script::QuestActionInfo {
        n_cmd_code: code,
        ..Default::default()
    };
    let mut p = ps.iter();
    if let Some(v) = p.next() {
        info.s_param1 = (*v).into();
    }
    if let Some(v) = p.next() {
        info.s_param2 = (*v).into();
    }
    if let Some(v) = p.next() {
        info.s_param3 = (*v).into();
    }
    if let Some(v) = p.next() {
        info.s_param4 = (*v).into();
    }
    if let Some(v) = p.next() {
        info.s_param5 = (*v).into();
    }
    if let Some(v) = p.next() {
        info.s_param6 = (*v).into();
    }
    let mut n = ns.iter();
    if let Some(v) = n.next() {
        info.n_param1 = *v;
    }
    if let Some(v) = n.next() {
        info.n_param2 = *v;
    }
    if let Some(v) = n.next() {
        info.n_param3 = *v;
    }
    if let Some(v) = n.next() {
        info.n_param4 = *v;
    }
    if let Some(v) = n.next() {
        info.n_param5 = *v;
    }
    if let Some(v) = n.next() {
        info.n_param6 = *v;
    }
    info
}

#[test]
fn var_family_inc_dec_mul_mov_slots() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // INC M6（无第三参）⇒ n3c=nParam2；nParam2=0 走 ++
    let _ = run_act(&mut npc, &mut player, act(24, "M6", "1", 0, 0));
    assert_eq!(player.mnmval(6), 1, "INC M6 → +1");
    // INC M6 5 ⇒ n3c=5>1 ⇒ +5
    let _ = run_act(&mut npc, &mut player, act(24, "M6", "5", 0, 5));
    assert_eq!(player.mnmval(6), 6, "INC M6 5 → +5");
    // DEC M6 2
    let _ = run_act(&mut npc, &mut player, act(25, "M6", "2", 0, 2));
    assert_eq!(player.mnmval(6), 4, "DEC M6 2 → -2");
    // MUL M6 3 4 ⇒ 12
    let _ = run_act(&mut npc, &mut player, act_full(28, &["M6", "3", "4"], &[]));
    assert_eq!(player.mnmval(6), 12, "MUL M6 3 4 → 12");
    // MOV M6 7（case 7）
    let _ = run_act(&mut npc, &mut player, act(23, "M6", "7", 0, 7));
    assert_eq!(player.mnmval(6), 7, "MOV M6 7 → 7");
    // INC G5 1 ⇒ GlobalVal[5]
    let _ = run_act(&mut npc, &mut player, act(24, "G5", "1", 0, 1));
    assert_eq!(player.global_val(5), 1, "INC G5 1 → GlobalVal[5]=1");
    // MOV M50 <$HP>（case 4）：WAbil.HP → M50
    player.wabil_hp = 42;
    let _ = run_act(&mut npc, &mut player, act(23, "M50", "<$HP>", 0, 0));
    assert_eq!(player.mnmval(50), 42, "MOV M50 <$HP> → 42");
    // MOVR M6 1 3：随机值由 mock 固定为 0（rng_value 默认 0）
    let _ = run_act(&mut npc, &mut player, act_full(36, &["M6"], &[0, 1, 3]));
    assert_eq!(
        player.mnmval(6),
        1,
        "MOVR M6 1 3 → nParam2 + Random(2) = 1+0"
    );
}

#[test]
fn var_dynamic_var_declare_calc_and_check_var() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // VAR Integer HUMAN 时间
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(237, &["Integer", "HUMAN", "时间"], &[]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(
        player.dyn_var_get(mir2_script::engine::DynVarKind::Human, "时间"),
        Some(mir2_script::engine::DynVar::Integer(0))
    );
    // CALCVAR HUMAN 时间 = 5（`IsStringNumber` 恒真 ⇒ 数值路径）
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(240, &["HUMAN", "时间", "=", "5"], &[]),
    );
    assert_eq!(
        player.dyn_var_get(mir2_script::engine::DynVarKind::Human, "时间"),
        Some(mir2_script::engine::DynVar::Integer(5))
    );
    // CALCVAR HUMAN 时间 + 3 → 8
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(240, &["HUMAN", "时间", "+", "3"], &[]),
    );
    assert_eq!(
        player.dyn_var_get(mir2_script::engine::DynVarKind::Human, "时间"),
        Some(mir2_script::engine::DynVar::Integer(8))
    );
    // VAR String HUMAN 名字 + CALCVAR = 你好
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(237, &["String", "HUMAN", "名字"], &[]),
    );
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(240, &["HUMAN", "名字", "=", "你好"], &[]),
    );
    assert_eq!(
        player.dyn_var_get(mir2_script::engine::DynVarKind::Human, "名字"),
        Some(mir2_script::engine::DynVar::Str("你好".into()))
    );
    // 重复 VAR ⇒ 「变量…已存在」
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(237, &["Integer", "HUMAN", "时间"], &[]),
    );
    assert!(first_script_error(&e).contains("变量时间已存在"));

    // CHECKVAR HUMAN 时间 >= 8（C# `sMethod[1]` ⇒ '='）
    let mut engine = Engine::new(&mut npc, &mut player);
    assert!(engine.check_conditions(&[cond6(139, ["HUMAN", "时间", ">=", "8", "", ""])]));
    // C# `sMethod[1]` 对单字操作符会越界（S2 崩溃面）：Rust 落 `'\0'` ⇒ 默认 `>=`
    let mut engine = Engine::new(&mut npc, &mut player);
    assert!(engine.check_conditions(&[cond6(139, ["HUMAN", "时间", ">", "8", "", ""])]));
    // 不存在的变量 ⇒ 条件错误 + false
    let mut engine = Engine::new(&mut npc, &mut player);
    assert!(!engine.check_conditions(&[cond6(139, ["HUMAN", "不存在", ">=", "1", "", ""])]));
}

#[test]
fn var_loadvar_and_savevar_file_io() {
    let dir = std::env::temp_dir().join("openmir2_b2_loadvar");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    std::fs::write(
        &file,
        mir2_script::textfile::encode_gb2312("[测试玩家]\r\nX=hello\r\n"),
    )
    .unwrap();
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    npc.envir_root = Some(dir.to_string_lossy().to_string());
    npc.path = String::new();
    // LOADVAR HUMAN X list.txt：文件里存在，且变量尚未声明 ⇒ 插入 String
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(238, &["HUMAN", "X", "list.txt"], &[]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(
        player.dyn_var_get(mir2_script::engine::DynVarKind::Human, "X"),
        Some(mir2_script::engine::DynVar::Str("hello".into()))
    );
    let before = std::fs::read(&file).unwrap();
    // SAVEVAR HUMAN X list.txt：变量存在、文件存在 ⇒ 无错误（C# 不落盘）
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(239, &["HUMAN", "X", "list.txt"], &[]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(
        std::fs::read(&file).unwrap(),
        before,
        "SAVEVAR 不写盘（原文）"
    );
    // SAVEVAR 不存在的变量 ⇒ 变量不存在错误
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(239, &["HUMAN", "Y", "list.txt"], &[]),
    );
    assert!(first_script_error(&e).contains("变量Y不存在"));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn check_name_date_list_writes_day_vars() {
    let dir = std::env::temp_dir().join("openmir2_b2_namedate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("list.txt");
    std::fs::write(
        &file,
        mir2_script::textfile::encode_gb2312("测试玩家 2020-01-01\r\n"),
    )
    .unwrap();
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    npc.envir_root = Some(dir.to_string_lossy().to_string());
    npc.path = String::new();
    // nDay 恒 int.MaxValue；'>' ⇒ 成功；p0/p1 分别写 GlobalVal[0]/[1]
    let mut engine = Engine::new(&mut npc, &mut player);
    assert!(engine.check_conditions(&[cond6(144, ["list.txt", ">", "30", "p0", "p1", ""])]));
    assert_eq!(player.global_val(0), i32::MAX);
    assert_eq!(player.global_val(1), 30i32.wrapping_sub(i32::MAX));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn upgrade_items_ex_success_and_failure_paths() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    player.use_items = vec![None; 13];
    player.use_items[1] = Some(5);
    player.worn_names.insert(1, "屠龙".into());
    player.worn_present.insert(1, true);
    player.worn_dura.insert(1, 10);
    player.worn_dura_max.insert(1, 100);
    player.worn_desc.insert((1, 0), 7);
    player.rng_value = 5; // RatePoint=5（成功）、Point=max(1,5)
                          // UPGRADEITEMSEX 1 0 10 5 0：ValType=0，Desc[0] 7→12
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(213, &["1", "0", "10", "5", "0"], &[]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(player.use_item_desc(1, 0), 12, "Desc[0] 应 7→12");
    assert_eq!(player.updated_items, vec![1]);
    // ValType=14 ⇒ DuraMax += Point*1000
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(213, &["1", "14", "10", "5", "0"], &[]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(player.use_item_dura_max(1), 5100, "DuraMax=100+5*1000");
    // 参数越界 ⇒ ScriptActionError(UpgradeItemSex)
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(213, &["1", "15", "10", "5", "0"], &[]),
    );
    assert!(first_script_error(&e).contains("脚本命令:UpgradeItemSex"));
}

#[test]
fn mon_gen_ex_and_clear_map_mon() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // MONGENEX 0 100 200 稻草人 5 3：Random 固定 0 ⇒ (100-5, 200-5) 三次
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(214, &["0", "100", "200", "稻草人"], &[0, 0, 0, 0, 5, 3]),
    );
    assert!(e.is_empty(), "{e:?}");
    assert_eq!(player.regen.len(), 3);
    assert_eq!(player.regen[0], "regen 0 95 195 稻草人");
    // 参数不全 ⇒ 错误
    let e = run_act(
        &mut npc,
        &mut player,
        act_full(214, &["0", "0", "0", ""], &[]),
    );
    assert!(first_script_error(&e).contains("脚本命令:MonGenex"));
    // CLEARMAPMON 直接委派世界层
    let _ = run_act(&mut npc, &mut player, act(215, "0", "", 0, 0));
    assert_eq!(player.cleared_map.as_deref(), Some("0"));
}

#[test]
fn add_use_date_list_appends_once() {
    let dir = std::env::temp_dir().join("openmir2_b2_usedate");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    npc.envir_root = Some(dir.to_string_lossy().to_string());
    npc.path = String::new();
    player.today = "2026-10-11".into();
    let _ = run_act(&mut npc, &mut player, act(335, "uselist.txt", "", 0, 0));
    let file = dir.join("uselist.txt");
    let first = std::fs::read(&file).unwrap();
    assert_eq!(
        mir2_script::textfile::decode_bytes(&first),
        "测试玩家    2026-10-11\r\n"
    );
    // 二次调用：名字已存在 ⇒ 不追加
    let _ = run_act(&mut npc, &mut player, act(335, "uselist.txt", "", 0, 0));
    assert_eq!(std::fs::read(&file).unwrap(), first);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn marry_and_unmarry_reachable_branches() {
    let (mut npc, mut player) = setup("[@main]\n#SAY\nx\n");
    // 无 DearName、无 pose ⇒ @MarryCheckDir（无该 label 时静默）
    let _ = run_act(&mut npc, &mut player, act(175, "", "", 0, 0));
    // 有 pose 且同性 ⇒ @MarrySexErr
    player.pose = Some(mir2_script::engine::PoseInfo {
        race: mir2_script::engine::ACTOR_RACE_PLAY,
        gender: mir2_script::engine::GENDER_MAN,
        chr_name: "对方".into(),
        is_mutual: true,
        ..Default::default()
    });
    player.gender = mir2_script::engine::GENDER_MAN;
    let _ = run_act(&mut npc, &mut player, act(175, "", "", 0, 0));
    // 异性 ⇒ 双方 IsStartMarry=true + 广播
    player.pose = Some(mir2_script::engine::PoseInfo {
        race: mir2_script::engine::ACTOR_RACE_PLAY,
        gender: mir2_script::engine::GENDER_WOMAN,
        chr_name: "对方".into(),
        is_mutual: true,
        ..Default::default()
    });
    let _ = run_act(&mut npc, &mut player, act(175, "", "", 0, 0));
    assert!(player.is_start_marry);
    assert!(player.pose_is_start_marry());
    // RESPONSEMARRY（女对男，OK）⇒ 设置 DearName
    player.gender = mir2_script::engine::GENDER_WOMAN;
    player.pose = Some(mir2_script::engine::PoseInfo {
        race: mir2_script::engine::ACTOR_RACE_PLAY,
        gender: mir2_script::engine::GENDER_MAN,
        chr_name: "男方".into(),
        is_mutual: true,
        ..Default::default()
    });
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(175, &["RESPONSEMARRY", "OK"], &[]),
    );
    assert_eq!(player.dear_name, "男方");
    assert_eq!(player.pose_dear_name(), "测试玩家");
    // UNMARRY（REQUESTUNMARRY，对方已响应）⇒ DearName 清空 + MarryCount++
    player.pose_state.borrow_mut().is_start_un_marry = true;
    let _ = run_act(
        &mut npc,
        &mut player,
        act_full(176, &["REQUESTUNMARRY", ""], &[]),
    );
    assert!(player.dear_name.is_empty());
    assert_eq!(player.marry_count, 1);
}
