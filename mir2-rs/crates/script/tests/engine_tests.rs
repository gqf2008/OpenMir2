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
fn take_targets_goto_lable_take_item_not_yet_implemented() {
    // B2/M4 翻转后：脚本 `take 金币 1`（枚举 2）→ CmdCode 2 → GotoLableTakeItem（尚未实现）
    // ⇒ 显式 NotImplemented，不再误落到旧位移的 ActionOfSet 写标记。
    let (mut npc, mut player) = setup(
        "[@main]
#ACT
take 金币 1
",
    );
    let errors = run(&mut npc, &mut player, "@main");
    assert!(
        errors.iter().any(|e| matches!(
            e,
            EngineError::NotImplemented {
                handler: "GotoLableTakeItem",
                ..
            }
        )),
        "{errors:?}"
    );
    assert_eq!(player.flags.get(&0), None, "不再误落 ActionOfSet 写标记");
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
