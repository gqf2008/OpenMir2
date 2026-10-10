//! 解析器单元测试：内存 FS 固定夹具，锁定 1:1 移植的关键行为（含 C# 缺陷行为）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mir2_script::parser::{get_envir_file_path, ScriptFs};
use mir2_script::ScriptParsers;

struct MemFs {
    files: HashMap<String, String>,
}

impl MemFs {
    fn new() -> Self {
        Self {
            files: HashMap::new(),
        }
    }

    fn with(mut self, path: &str, content: &str) -> Self {
        self.files
            .insert(path.replace('\\', "/").to_lowercase(), content.to_string());
        self
    }
}

impl ScriptFs for MemFs {
    fn load_string_list(&self, path: &Path) -> Option<Vec<String>> {
        let key = path.to_string_lossy().replace('\\', "/").to_lowercase();
        self.files.get(&key).map(|s| mir2_script::split_lines(s))
    }
}

fn envir() -> PathBuf {
    PathBuf::from("root")
}

fn load(fs: &MemFs, patch: &str, name: &str, bo_flag: bool) -> mir2_script::LoadOutcome {
    let parsers = ScriptParsers::new();
    let mut rand = mir2_shared::rng::RandomNumber::with_seed(42);
    let mut rename = move |min: i32, max: i32| rand.get_random_number(min, max);
    parsers
        .load_script_file(fs, &envir(), patch, name, bo_flag, &mut rename)
        .expect("解析异常")
        .expect("文件缺失")
}

#[test]
fn basic_if_act_say() {
    let fs = MemFs::new().with(
        "root/npc_def/比奇国王-0122.txt",
        "[@main]\n#IF\ncheckitem 祈福项链 1\n#SAY\n你好\n#ACT\ngoto @x\n#ELSESAY\n没有\n#ELSEACT\nbreak\n",
    );
    let out = load(&fs, "Npc_def", "比奇国王-0122", false);
    assert_eq!(out.stats.records, 1);
    assert_eq!(out.stats.procedures, 1);
    assert_eq!(out.stats.conditions, 1);
    assert_eq!(out.stats.actions, 1);
    assert_eq!(out.stats.else_actions, 1);
    assert_eq!(out.stats.parse_errors, 0);
    let rec = &out.scripts[0].record_list[0];
    assert_eq!(rec.s_label, "@main");
    let proc_ = &rec.procedure_list[0];
    assert_eq!(proc_.s_say_msg, "你好");
    assert_eq!(proc_.s_else_say_msg, "没有");
    // CHECKITEM 字段序号 10 → CmdCode 9（C# code-1 缺陷原样保留）
    assert_eq!(proc_.condition_list[0].cmd_code, 9);
    assert_eq!(proc_.condition_list[0].s_param1, "祈福项链");
    assert_eq!(proc_.condition_list[0].n_param2, 1);
}

#[test]
fn multi_if_creates_new_procedure_only_after_content() {
    // C#: 仅当当前过程已有条件或 say 文本时，#IF 才新开过程
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "[@main]\n#IF\n#IF\ncheckgold 1\n#ACT\n#IF\ncheckgold 2\n#ACT\n",
    );
    let out = load(&fs, "Npc_def", "t", false);
    // 过程1: 空（首 #IF 不新建）；#IF(2) 仍空 → 不新建；条件进入过程1；
    // 过程1 有条件后，第 3 个 #IF 新开过程2
    assert_eq!(out.stats.procedures, 2);
    assert_eq!(out.stats.conditions, 2);
}

#[test]
fn unknown_command_is_parse_error_not_silent() {
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "[@main]\n#IF\nNOSUCHCOND 1\n#ACT\nNOSUCHACT\n",
    );
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.parse_errors, 2);
    assert_eq!(out.errors.len(), 2);
}

#[test]
fn say_text_concatenates_without_separator() {
    let fs = MemFs::new().with("root/npc_def/t.txt", "[@main]\n#SAY\n第一行\\\n第二行\n");
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(
        out.scripts[0].record_list[0].procedure_list[0].s_say_msg,
        "第一行\\第二行"
    );
}

#[test]
fn merchant_header_and_goods() {
    let fs = MemFs::new().with(
        "root/market_def/shop.txt",
        "%160\n+1\n+30\n(@buy @sell @repair)\n[goods]\n木剑 1 60\n\"青铜 剑\" 2 30\n[@main]\n#SAY\n欢迎\n",
    );
    let out = load(&fs, "Market_Def", "shop", true);
    assert_eq!(out.price_rate, Some(160));
    assert_eq!(out.item_type_list, vec![1, 30]);
    assert!(
        out.merchant_flags.is_buy && out.merchant_flags.is_sell && out.merchant_flags.is_repair
    );
    assert_eq!(out.stats.goods, 2);
    assert_eq!(out.refill_goods[0].item_name, "木剑");
    assert_eq!(out.refill_goods[0].count, 1);
    assert_eq!(out.refill_goods[0].refill_time, 60);
}

#[test]
fn merchant_header_only_when_bo_flag() {
    let fs = MemFs::new().with("root/npc_def/t.txt", "%160\n+1\n[@main]\n#SAY\nx\n");
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.price_rate, None);
    assert!(out.item_type_list.is_empty());
}

#[test]
fn call_script_expands_with_csharp_quirks() {
    // C# LoadScriptCallScript：从文件头加行（不理会 label 之前的行也一并加入），
    // 遇 '}' 行终止；label 匹配行本身跳过。
    let fs = MemFs::new()
        .with(
            "root/questdiary/shared.txt",
            ";注释\n{@Quest}\n[@目标]\ngoto @a\ntake 金币 1\n}\n[@其他]\ngoto @b\n",
        )
        .with(
            "root/npc_def/t.txt",
            "[@main]\n#IF\ncheckgold 1\n#ACT\n#CALL [shared.txt] @目标\n",
        );
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.call_loads, 1);
    // 展开后动作：goto @a / take 金币 1（} 终止，之后的 [@其他] 段不进入）
    let proc_ = &out.scripts[0].record_list[0].procedure_list[0];
    let names: Vec<i32> = proc_.action_list.iter().map(|a| a.n_cmd_code).collect();
    assert_eq!(names.len(), 2, "call 展开的动作数应为 2: {names:?}");
    assert_eq!(out.stats.parse_errors, 0);
}

#[test]
fn duplicate_label_gets_random_suffix() {
    let fs = MemFs::new().with("root/npc_def/t.txt", "[@main]\n#SAY\na\n[@main]\n#SAY\nb\n");
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.label_renames, 1);
    assert_eq!(out.stats.records, 2);
}

#[test]
fn define_substitution_and_home() {
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "#SETHOME @start\n#DEFINE FEE 100\n[@start]\n#IF\ncheckgold FEE\n#ACT\ntake 金币 FEE\n",
    );
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.home_label, "@start");
    // FEE 在两条内容行中各替换 1 次（行首命中不替换，此处均不在行首）
    assert_eq!(out.stats.define_substitutions, 2);
    let proc_ = &out.scripts[0].record_list[0].procedure_list[0];
    assert_eq!(proc_.condition_list[0].n_param1, 100);
    assert_eq!(proc_.action_list[0].s_param2, "100");
}

#[test]
fn quest_creates_new_script_info() {
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "{Quest 3}\n[@main]\n#SAY\nq\n{Quest 5}\n[@other]\n#SAY\nx\n",
    );
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.scripts, 2);
    assert_eq!(out.scripts[0].quest_count, 3);
    assert_eq!(out.scripts[1].quest_count, 5);
}

#[test]
fn quest_block_keeps_writing_into_previous_record() {
    // C# 的 SayingRecord 与 Script 相互独立：`{Quest` 只换 Script，
    // 之后的动作行仍写入上一个记录（回归用例，对拍 C# 实测 脚本2/标签2/过程2/条件1/动作2）
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "[@main]
#IF
checkgold 1
#ACT
give 金币 1
{Quest 2}
give 金币 2
[@b]
#SAY
x
",
    );
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.scripts, 2);
    assert_eq!(out.stats.records, 2);
    assert_eq!(out.stats.conditions, 1);
    assert_eq!(out.stats.actions, 2, "第二个 give 应写入上一个记录");
    // 新脚本（quest_count=2）自身没有记录，记录仍在第一个脚本里
    assert_eq!(out.scripts[1].record_list.len(), 1); // [@b]
    assert_eq!(
        out.scripts[0].record_list[0].procedure_list[0]
            .action_list
            .len(),
        2
    );
}

#[test]
fn comment_and_empty_lines_skipped() {
    let fs = MemFs::new().with("root/npc_def/t.txt", ";注释\n/注释2\n\n[@main]\n#SAY\nx\n");
    let out = load(&fs, "Npc_def", "t", false);
    assert_eq!(out.stats.records, 1);
    assert_eq!(out.stats.parse_errors, 0);
}

#[test]
fn opname_dot_split() {
    let fs = MemFs::new().with(
        "root/npc_def/t.txt",
        "[@main]\n#IF\n玩家甲.CHECKLEVEL > 10\n#ACT\n",
    );
    let out = load(&fs, "Npc_def", "t", false);
    let cond = &out.scripts[0].record_list[0].procedure_list[0].condition_list[0];
    assert_eq!(cond.s_op_name, "玩家甲");
    // CHECKLEVEL 字段序号 7 → CmdCode 6
    assert_eq!(cond.cmd_code, 6);
}

#[test]
fn envir_path_dotdot() {
    let root = Path::new("E:/MirServer/M2GameSvr/Envir");
    assert_eq!(
        get_envir_file_path(root, "QuestDiary", "..\\QuestDiary\\x.txt"),
        root.join("QuestDiary/x.txt")
    );
    assert_eq!(
        get_envir_file_path(root, "Defines", "a.txt"),
        root.join("Defines").join("a.txt")
    );
}
