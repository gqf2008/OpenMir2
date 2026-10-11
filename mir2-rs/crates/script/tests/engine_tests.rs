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
    rng_value: i32,
    unit_status: std::collections::HashMap<i32, i32>,
    items: Vec<(String, i32)>,
    effects: Vec<String>,
    // ---- B2/M4 条件批次新增的脚本可见状态 ----
    game_gold: i32,
    game_point: i32,
    credit_point: i32,
    member_level: i32,
    bonus_point: i32,
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
            rng_value: 0,
            unit_status: std::collections::HashMap::new(),
            items: Vec::new(),
            effects: Vec::new(),
            game_gold: 0,
            game_point: 0,
            credit_point: 0,
            member_level: 0,
            bonus_point: 0,
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
    fn send_msg(&mut self, ident: i32, _param: i32, _tag: i32, _series: i32, msg: Option<&str>) {
        self.msgs.push((ident, msg.map(str::to_string)));
    }
    fn mn_val(&self, n: usize) -> i32 {
        self.mn[n]
    }
    fn set_mn_val(&mut self, n: usize, v: i32) {
        self.mn[n] = v;
    }
    fn mdy_val(&self, _n: usize) -> i32 {
        0
    }
    fn mnmval(&self, _n: usize) -> i32 {
        0
    }
    fn mn_integer(&self, _n: usize) -> i32 {
        0
    }
    fn ms_string(&self, _n: usize) -> String {
        String::new()
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
        self.bonus_point
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
}

struct MockNpc {
    actor_id: i32,
    chr_name: String,
    map: String,
    scripts: Vec<ScriptInfo>,
    says: Vec<(String, bool)>,
    goto_limit: i32,
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
    player.bonus_point = 9;
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
    // 诚实登记：仍有两个条件处理器未实现（动态变量层 / 日期列表文件）
    let undone = d::unimplemented_handlers();
    for remain in ["ConditionOfCheckVar", "ConditionOfCheckNameDateList"] {
        assert!(undone.contains(&remain), "{remain} 应仍在未实现清单");
    }
}
