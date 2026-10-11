//! `flow-run`：Rust 侧脚本执行器（与 C# `script-parity-cs run` 同参数、同日志格式）。
//!
//! 用法：
//!   script-tool flow-run <脚本文件> <label> [--gold N] [--level N]
//!                        [--items 名:序号:重量,...] [--player-items 名:件数,...]
//!
//! 输出（stdout，一行一效果，供与 C# 侧 diff）：
//!   msg <ident> <wParam> <nParam1> <nParam2> <nParam3> <文本>
//!   gold <delta>
//!   additem <名> <件数>
//!   delitem <名> <件数>
//!   unitstatus <index> <value>
//!   err <错误文本>
//!   #gold=<值>
//!   #items=<名,名,...>

use std::collections::HashMap;
use std::path::Path;
use std::process::ExitCode;

use mir2_script::engine::{Engine, EngineError, ScriptNpc, ScriptPlayer};
use mir2_script::ScriptInfo;
use mir2_script::ScriptParsers;

/// 内存物品表（--items 夹具，两侧必须给同一份）。
struct ItemTable {
    by_name: HashMap<String, u16>,
    by_idx: HashMap<u16, String>,
}

struct FlowPlayer {
    gold: i32,
    level: i32,
    items: Vec<(String, i32)>,
    log: Vec<String>,
    unit_status: HashMap<i32, i32>,
    /// 任务标记位图（对应 C# `PlayObject.QuestFlag[]`）
    quest_flag: [u8; 512],
    last_npc: i32,
    script: Option<usize>,
    goto_count: i32,
}

impl ScriptPlayer for FlowPlayer {
    fn last_npc(&self) -> i32 {
        self.last_npc
    }
    fn set_last_npc(&mut self, actor_id: i32) {
        self.last_npc = actor_id;
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
    fn quest_flag_status(&self, _flag: i16) -> u8 {
        0
    }
    fn gold(&self) -> i32 {
        self.gold
    }
    fn dec_gold(&mut self, n: i32) {
        // 对齐 C# flow 夹具 `DecGold`：`GoldValue -= v; Log.Add($"gold {-v}")`
        self.gold -= n;
        self.log.push(format!("gold {}", -n));
    }
    fn inc_gold(&mut self, n: i32) {
        // 对齐 C# flow 夹具 `IncGold`：`GoldValue += v; Log.Add($"gold {v}")`
        self.gold += n;
        self.log.push(format!("gold {n}"));
    }
    fn actor_id(&self) -> i32 {
        4242
    }
    fn level(&self) -> i32 {
        self.level
    }
    fn random(&mut self, _value: i32) -> i32 {
        0
    }
    fn send_msg(&mut self, ident: i32, param: i32, tag: i32, series: i32, msg: Option<&str>) {
        let text = msg.unwrap_or("").replace('\r', "\\r").replace('\n', "\\n");
        self.log
            .push(format!("msg {ident} {param} {tag} {series} {text}"));
    }
    fn mn_val(&self, _n: usize) -> i32 {
        0
    }
    fn set_mn_val(&mut self, _n: usize, _v: i32) {}
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
    fn set_quest_flag_status(&mut self, flag: i32, value: i32) {
        self.log.push(format!("flag {flag} {value}"));
        let idx = flag - 1;
        if idx < 0 {
            return;
        }
        let byte_idx = (idx / 8) as usize;
        if byte_idx >= self.quest_flag.len() {
            return;
        }
        let bit = 128u8 >> (idx % 8);
        self.quest_flag[byte_idx] = if value == 0 {
            !bit & self.quest_flag[byte_idx]
        } else {
            bit | self.quest_flag[byte_idx]
        };
    }
    fn set_quest_unit_open_status(&mut self, index: i32, value: i32) {
        self.log.push(format!("unitopen {index} {value}"));
    }
    fn set_quest_unit_status(&mut self, index: i32, value: i32) {
        self.unit_status.insert(index, value);
        self.log.push(format!("unitstatus {index} {value}"));
    }
    fn slave_count(&self) -> i32 {
        0
    }
    fn bag_count(&self) -> i32 {
        self.items.iter().map(|(_, c)| *c).sum()
    }
    fn item_count(&self, name: &str) -> i32 {
        self.items
            .iter()
            .filter(|(n, _)| n.eq_ignore_ascii_case(name))
            .map(|(_, c)| *c)
            .sum()
    }
    fn has_worn(&self, _location: &str) -> bool {
        false
    }
    fn give_item(&mut self, name: &str, count: i32) {
        self.items.push((name.to_string(), count));
        self.log.push(format!("additem {name} {count}"));
    }
    fn remove_item(&mut self, name: &str, count: i32) -> i32 {
        let mut left = count;
        for (n, c) in self.items.iter_mut() {
            if left <= 0 {
                break;
            }
            if n.eq_ignore_ascii_case(name) {
                let take = (*c).min(left);
                *c -= take;
                left -= take;
                self.log.push(format!("delitem {n} {take}"));
            }
        }
        self.items.retain(|(_, c)| *c > 0);
        count - left
    }
}

struct FlowNpc {
    scripts: Vec<ScriptInfo>,
    says: Vec<String>,
    /// 与玩家共享的效果日志：说词按 C# 的 `GotoLableSendMerChantSayMsg` 记为玩家消息
    log: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
}

impl ScriptNpc for FlowNpc {
    fn actor_id(&self) -> i32 {
        7
    }
    fn chr_name(&self) -> String {
        "测试NPC".into()
    }
    fn map_name(&self) -> String {
        "0".into()
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
        self.says.push(msg.to_string());
        // C#: SendMsg(normNpc, RM_MERCHANTSAY, 0,0,0,0, ChrName + '/' + msg)（优先消息走 SendPriorityMsg）
        let text = format!("测试NPC/{msg}")
            .replace('\r', "\\r")
            .replace('\n', "\\n");
        let kind = if priority { "msgP" } else { "msg" };
        self.log
            .borrow_mut()
            .push(format!("{kind} 11009 0 0 0 0 {text}")); // RM_MERCHANTSAY = 11009
    }

    fn script_goto_count_limit(&self) -> i32 {
        100
    }
}

pub fn run(args: &[String]) -> ExitCode {
    if args.len() < 4 {
        eprintln!("用法: flow-run <脚本文件> <label> [--gold N] [--level N] [--items 名:序号:重量,...] [--player-items 名:件数,...]");
        return ExitCode::from(2);
    }
    // args[0]=程序名, args[1]="flow-run", args[2]=脚本, args[3]=label
    let script_path = &args[2];
    let label = &args[3];
    let mut gold = 1000i32;
    let mut level = 10i32;
    let mut table = ItemTable {
        by_name: HashMap::new(),
        by_idx: HashMap::new(),
    };
    let mut player_items: Vec<(String, i32)> = Vec::new();
    let mut i = 4;
    while i + 1 < args.len() {
        match args[i].as_str() {
            "--gold" => gold = args[i + 1].parse().unwrap_or(1000),
            "--level" => level = args[i + 1].parse().unwrap_or(10),
            "--items" => {
                for spec in args[i + 1].split(',').filter(|s| !s.is_empty()) {
                    let p: Vec<&str> = spec.split(':').collect();
                    if p.len() >= 2 {
                        let idx: u16 = p[1].parse().unwrap_or(0);
                        table.by_name.insert(p[0].to_string(), idx);
                        table.by_idx.insert(idx, p[0].to_string());
                    }
                }
            }
            "--player-items" => {
                for spec in args[i + 1].split(',').filter(|s| !s.is_empty()) {
                    let p: Vec<&str> = spec.split(':').collect();
                    player_items.push((
                        p[0].to_string(),
                        p.get(1).and_then(|c| c.parse().ok()).unwrap_or(1),
                    ));
                }
            }
            other => {
                eprintln!("未知参数: {other}");
                return ExitCode::from(2);
            }
        }
        i += 2;
    }

    let parsers = ScriptParsers::new();
    let fs = mir2_script::LocalFs;
    let script_file = Path::new(script_path);
    let patch = script_file
        .parent()
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_default();
    let stem = script_file
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_default();
    let mut rng = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename = move |min: i32, max: i32| rng.get_random_number(min, max);
    // 注意：C# 侧 run 用 Path.GetDirectoryName 作为 patch（绝对路径再拼 .txt）
    let outcome =
        match parsers.load_script_file(&fs, Path::new(""), &patch, &stem, true, &mut rename) {
            Ok(Some(o)) => o,
            Ok(None) => {
                eprintln!("脚本未找到: {script_path}");
                return ExitCode::FAILURE;
            }
            Err(e) => {
                eprintln!("脚本解析异常: {e}");
                return ExitCode::FAILURE;
            }
        };
    let shared_log = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut npc = FlowNpc {
        scripts: outcome.scripts,
        says: Vec::new(),
        log: shared_log.clone(),
    };
    let mut player = FlowPlayer {
        gold,
        level,
        items: player_items,
        log: Vec::new(),
        unit_status: HashMap::new(),
        quest_flag: [0; 512],
        last_npc: 0,
        script: None,
        goto_count: 0,
    };

    if std::env::var("MIR2_FLOW_DUMP").is_ok() {
        for (si, sc) in npc.scripts.iter().enumerate() {
            for rec in &sc.record_list {
                eprintln!(
                    "[{}] label={:?} procs={}",
                    si,
                    rec.s_label,
                    rec.procedure_list.len()
                );
                for (pi, p) in rec.procedure_list.iter().enumerate() {
                    eprintln!(
                        "  proc{pi}: conds={:?} acts={:?} elseacts={:?} say={:?} elsay={:?}",
                        p.condition_list
                            .iter()
                            .map(|c| c.cmd_code)
                            .collect::<Vec<_>>(),
                        p.action_list
                            .iter()
                            .map(|a| a.n_cmd_code)
                            .collect::<Vec<_>>(),
                        p.else_action_list
                            .iter()
                            .map(|a| a.n_cmd_code)
                            .collect::<Vec<_>>(),
                        p.s_say_msg,
                        p.s_else_say_msg
                    );
                }
            }
        }
    }

    let errors: Vec<EngineError>;
    {
        let mut engine = Engine::new(&mut npc, &mut player);
        engine.goto_label(label, false);
        errors = engine.errors.clone();
    }

    for line in &player.log {
        println!("{line}");
    }
    for line in shared_log.borrow().iter() {
        println!("{line}");
    }
    for e in &errors {
        match e {
            EngineError::ScriptError { text } => println!("err {text}"),
            other => println!("err(rust) {other:?}"),
        }
    }
    println!("#gold={}", player.gold);
    let names: Vec<String> = player
        .items
        .iter()
        .flat_map(|(n, c)| std::iter::repeat_n(n.clone(), (*c).max(0) as usize))
        .collect();
    println!("#items={}", names.join(","));
    ExitCode::SUCCESS
}
