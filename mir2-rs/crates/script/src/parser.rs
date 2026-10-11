//! `ScriptParsers` 的 1:1 移植（参照 `src/Modules/ScriptEngine/ScriptParsers.cs`）。
//!
//! 已确认的 C# 怪行为（全部保留，见各处注释）：
//! - `LoadScriptCallScript` 的 `findLab` 只影响首个匹配 label 行的跳过，
//!   **不**阻止 label 之前的行被加入（等效“从文件头加到第一个 `}` 行”）。
//! - `callList[i] = "#ACT"` 在 `i >= Count` 时抛 `ArgumentOutOfRangeException`；
//!   仅当此前已有成功的 `#CALL` 展开使列表变长时才不抛（此时会覆盖已展开的行）。
//! - 常量替换循环要求匹配位置 `> 0`（行首命中不替换），每个 define 每行最多 10 次。
//! - `scriptType == 1` 的 quest flag 分支在 C# 中不可达（`scriptType` 从不赋 1），保留为死分支。
//! - 未知 `#` 指令行被静默丢弃（不报错、不改变状态）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::codes::{CONDITION_CODES, EXECUTION_CODES};
use crate::hutil32::{
    arrest_string_ex_ref, compare_lstr, get_valid_str3, get_valid_str_cap, is_string_number,
    str_to_int, to_upper, ArrestWrite, CaptureStringPanic,
};
use crate::model::{
    Goods, MerchantFlags, QuestActionInfo, QuestConditionInfo, SayingProcedure, SayingRecord,
    ScriptInfo,
};

/// 分词分隔符（C# `TextSpitConst`）
const TEXT_SPLIT: &[char] = &[' ', '\t'];

/// 解析过程中的文件访问抽象：`SystemShare.GetEnvirFilePath` + `StringList.LoadFromFile`。
pub trait ScriptFs {
    /// `StringList.LoadFromFile`：GB2312(默认，带 BOM 嗅探) 解码 + ReadLine 切行。
    /// 文件不存在返回 `None`（对应 `File.Exists` == false）。
    fn load_string_list(&self, path: &Path) -> Option<Vec<String>>;
}

/// `SystemShare.GetEnvirFilePath(dirPath, filePath)`：
/// `filePath` 以 `..` 开头时去掉前 3 个字符（`"../"`）后只拼 Envir 根。
pub fn get_envir_file_path(envir_root: &Path, dir_path: &str, file_path: &str) -> PathBuf {
    if file_path.starts_with("..") {
        // C#: filePath[3..]（按 UTF-16 码元删 3 个字符；路径场景均为 ASCII）
        let stripped: String = file_path.chars().skip(3).collect();
        envir_root.join(stripped)
    } else {
        envir_root.join(dir_path).join(file_path)
    }
}

/// `GetCallScriptPath`（Windows 分支）：去掉开头的 `\\` 或 `\`。
fn get_call_script_path(path: &str) -> String {
    let mut s = path;
    if let Some(rest) = s.strip_prefix("\\\\") {
        s = rest;
    } else if let Some(rest) = s.strip_prefix('\\') {
        s = rest;
    }
    s.to_string()
}

/// 单条脚本错误（对应 C# `LogService.Error("脚本错误: ...")`）。
#[derive(Debug, Clone)]
pub struct ParseError {
    pub line: String,
    /// C# 错误消息中的“第 i 行”（stringList 下标，0-based）
    pub line_index: usize,
    pub kind: ParseErrorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParseErrorKind {
    /// 条件/动作行解析失败（`LoadScriptFileQuestCondition/Action` 返回 false）
    ScriptError,
    /// `#CALL` 目标加载失败
    CallLoadFail,
    /// `#INCLUDE` 文件不存在
    IncludeLoadFail,
}

/// 加载统计（验收口径：与 C# 侧逐项对拍）。
#[derive(Debug, Default, Clone)]
pub struct LoadStats {
    pub scripts: usize,
    pub records: usize,
    pub procedures: usize,
    pub conditions: usize,
    pub actions: usize,
    pub else_actions: usize,
    /// #DEFINE 常量替换次数（宏展开条数）
    pub define_substitutions: usize,
    /// #CALL 成功展开次数
    pub call_loads: usize,
    pub call_failures: usize,
    pub include_loads: usize,
    /// #INCLUDE 目标缺失次数（与 #CALL 失败同记 LogService.Error，C# 文案相同）
    pub include_failures: usize,
    pub goods: usize,
    pub price_rate_lines: usize,
    pub item_type_lines: usize,
    pub merchant_cmd_lines: usize,
    pub parse_errors: usize,
    /// 重名 label 触发随机改名的次数
    pub label_renames: usize,
}

/// 单个脚本文件的完整加载结果。
#[derive(Debug, Default)]
pub struct LoadOutcome {
    pub scripts: Vec<ScriptInfo>,
    /// 商人头信息（`boFlag == true` 时收集）
    pub price_rate: Option<i32>,
    pub item_type_list: Vec<i32>,
    pub merchant_flags: MerchantFlags,
    pub refill_goods: Vec<Goods>,
    /// `ProcessRefillIndex` 赋值的次数（`[goods]` 区出现次数）
    pub goods_sections: usize,
    pub errors: Vec<ParseError>,
    pub stats: LoadStats,
    /// `#DEFINE` 表（调试用）
    pub defines: Vec<(String, String)>,
    /// `#SETHOME` 返回值（缺省 `@main`）
    pub home_label: String,
}

/// C# `ArgumentOutOfRangeException` / `IndexOutOfRangeException` 穿透解析器的情形。
/// 解析器本体不兜底，与 C# 一致；调用方（工具/引擎）按文件捕获计数。
#[derive(Debug)]
pub enum LoadPanic {
    Capture(CaptureStringPanic),
    /// `callList[i] = "#ACT"` 越界
    CallListOutOfRange,
    /// `RecordList.Add` 重复键（随机改名后仍冲突）
    DuplicateLabel(String),
}

impl std::fmt::Display for LoadPanic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadPanic::Capture(e) => write!(f, "{e}"),
            LoadPanic::CallListOutOfRange => {
                f.write_str("ArgumentOutOfRangeException (callList[i])")
            }
            LoadPanic::DuplicateLabel(l) => write!(f, "ArgumentException duplicate label {l}"),
        }
    }
}

impl std::error::Error for LoadPanic {}

impl From<CaptureStringPanic> for LoadPanic {
    fn from(e: CaptureStringPanic) -> Self {
        LoadPanic::Capture(e)
    }
}

/// `ScriptParsers`：命令查表在 C# 侧由反射建立（`Dictionary<string,int>`，OrdinalIgnoreCase，
/// 重名保留第一个）；Rust 侧改为对生成表做 `def_of` 线性查找——同名字、同“保留第一个”语义。
pub struct ScriptParsers;

impl Default for ScriptParsers {
    fn default() -> Self {
        Self::new()
    }
}

impl ScriptParsers {
    pub fn new() -> Self {
        Self
    }

    /// `LoadScript`：C# 入口包装——`sPatch` 为空时默认 `"Npc_def"`。
    #[allow(clippy::too_many_arguments)]
    pub fn load_script<FS: ScriptFs>(
        &self,
        fs: &FS,
        envir_root: &Path,
        npc_def_path: &str,
        script_name: &str,
        bo_flag: bool,
        rename_rand: &mut dyn FnMut(i32, i32) -> i32,
    ) -> Result<Option<LoadOutcome>, LoadPanic> {
        let patch = if npc_def_path.is_empty() {
            "Npc_def"
        } else {
            npc_def_path
        };
        self.load_script_file(fs, envir_root, patch, script_name, bo_flag, rename_rand)
    }

    /// `LoadScriptFile`：加载并解析一个 NPC/商人脚本文件。
    ///
    /// - `npc_def_path`：C# 的 `sPatch`（`"Npc_def"` / `"Market_Def"` 等，可为空串=Envir 根）；
    /// - `script_name`：不带扩展名的脚本名（C# 内部拼 `.txt`）；
    /// - `bo_flag`：是否按商人脚本解析（`%` / `+` / `(...)` 头与 `[goods]` 区）；
    /// - `rename_rand`：重名 label 改名用的随机源（`RandomNumber.GetRandomNumber(1, 200)`，
    ///   返回值域 [1, 200]）。后缀取值不影响任何计数口径；引擎批将换为 System.Random 复刻。
    pub fn load_script_file<FS: ScriptFs>(
        &self,
        fs: &FS,
        envir_root: &Path,
        npc_def_path: &str,
        script_name: &str,
        bo_flag: bool,
        rename_rand: &mut dyn FnMut(i32, i32) -> i32,
    ) -> Result<Option<LoadOutcome>, LoadPanic> {
        let script_file_name =
            get_envir_file_path(envir_root, npc_def_path, &format!("{script_name}.txt"));
        let Some(mut lines) = fs.load_string_list(&script_file_name) else {
            // C#: LogService.Error("Script file not found: ...")，无解析产出
            return Ok(None);
        };
        let mut outcome = LoadOutcome::default();
        let mut call_script_dict: HashMap<String, String> = HashMap::new();
        // while (!success) LoadCallScript(...)
        loop {
            if self.load_call_script(
                fs,
                envir_root,
                &mut lines,
                &mut call_script_dict,
                &mut outcome,
            )? {
                break;
            }
        }
        // #SETHOME / #DEFINE / #INCLUDE
        let mut define_list: Vec<(String, String)> = Vec::new();
        let mut defline = String::new();
        self.load_script_define_info(
            fs,
            envir_root,
            &mut lines,
            &mut define_list,
            &mut defline,
            &mut outcome,
        );
        if defline.is_empty() {
            defline = "@main".to_string();
        }
        outcome.home_label = defline;
        define_list.push(("@HOME".to_string(), outcome.home_label.clone()));
        outcome.defines = define_list.clone();

        // 常量处理（#DEFINE 替换）
        let mut bo_define = false;
        #[allow(clippy::needless_range_loop)]
        for i in 0..lines.len() {
            let line = lines[i].trim().to_string();
            if line.is_empty() {
                continue;
            }
            if line.starts_with('[') {
                bo_define = false;
            } else if line.starts_with('#')
                && (compare_lstr(&line, "#IF")
                    || compare_lstr(&line, "#ACT")
                    || compare_lstr(&line, "#ELSEACT"))
            {
                bo_define = true;
            } else if bo_define {
                // 将 Define 好的常量换成指定值（C# 在 Trim 后的副本上替换，写回替换结果）
                for (name, text) in &define_list {
                    let mut n1c = 0;
                    loop {
                        let trimmed = lines[i].trim().to_string();
                        let upper = to_upper(&trimmed);
                        // C#: line.ToUpper().IndexOf(name, OrdinalIgnoreCase)；n24 <= 0 时退出
                        // （未命中或命中行首都退出——行首不替换）
                        let Some(n24) = find_from(&upper, name, 0) else {
                            break;
                        };
                        if n24 == 0 {
                            break;
                        }
                        let cs: Vec<char> = trimmed.chars().collect();
                        let name_len = name.chars().count();
                        let mut new_line: String = cs[..n24].iter().collect();
                        new_line.push_str(text);
                        let tail: String = cs[n24 + name_len..].iter().collect();
                        new_line.push_str(&tail);
                        lines[i] = new_line;
                        outcome.stats.define_substitutions += 1;
                        n1c += 1;
                        if n1c >= 10 {
                            break;
                        }
                    }
                }
            }
        }

        // 主解析循环（scriptType 状态机）
        let mut script_type: i32 = 0;
        let mut script_idx: Option<usize> = None;
        // C# 的 SayingRecord 变量与 Script 变量相互独立：`{Quest` 只换 Script，
        // 不改 SayingRecord ⇒ 新脚本段落之前的内容行仍写入**上一个**记录（可能属于旧脚本）。
        // 因此这里保存"记录所在位置"的完整坐标，而不是当前脚本内的下标。
        let mut record_loc: Option<(usize, usize)> = None;
        let mut quest_count: i32 = 0;
        let n_quest_idx = 0usize; // C# 死分支残留变量，保留
        let _ = n_quest_idx;
        // C# 方法级局部变量：跨行复用的 label 名（见 label 分支注释）
        let mut slab_name = String::new();

        let lines_snapshot = lines.clone();
        for (i, raw) in lines_snapshot.iter().enumerate() {
            let line = raw.trim().to_string();
            if line.is_empty() || line.starts_with(';') || line.starts_with('/') {
                continue;
            }
            if script_type == 0 && bo_flag {
                if let Some(rest) = line.strip_prefix('%') {
                    // 物品价格倍率
                    let n_price_rate = str_to_int(rest, -1);
                    if n_price_rate >= 55 {
                        outcome.price_rate = Some(n_price_rate);
                    }
                    outcome.stats.price_rate_lines += 1;
                    continue;
                }
                if let Some(rest) = line.strip_prefix('+') {
                    // 物品交易类型
                    let n_item_type = str_to_int(rest, -1);
                    if n_item_type >= 0 {
                        outcome.item_type_list.push(n_item_type);
                    }
                    outcome.stats.item_type_lines += 1;
                    continue;
                }
                if line.starts_with('(') {
                    // NPC 可执行命令设置：C# 为 ArrestStringEx(line, "(", ")", ref line)
                    // （返回值丢弃，只看 ref 写回；未命中定界符时按 C# 语义写回空串）
                    let mut inner = match arrest_string_ex_ref(&line, '(', ')').0 {
                        ArrestWrite::Value(v) => v,
                        ArrestWrite::Unchanged => line.clone(),
                    };
                    while !inner.is_empty() {
                        let (command, rest) = get_valid_str3(&inner, &[' ', ',', '\t']);
                        inner = rest;
                        set_merchant_flag(&mut outcome.merchant_flags, &command);
                    }
                    outcome.stats.merchant_cmd_lines += 1;
                    continue;
                }
            }
            if line.starts_with('{') {
                if compare_lstr(&line, "{Quest") {
                    let (first, s38) = get_valid_str3(&line, &[' ', '}', '\t']);
                    let _ = first;
                    let (quest_name, _) = get_valid_str3(&s38, &[' ', '}', '\t']);
                    let qc = str_to_int(&quest_name, 0);
                    let mut script = ScriptInfo {
                        quest_count: qc,
                        ..ScriptInfo::default()
                    };
                    script.quest_info = [crate::model::ScriptQuestInfo::default(); 10];
                    outcome.scripts.push(script);
                    script_idx = Some(outcome.scripts.len() - 1);
                    // record_loc 不变（C# 语义，见上）
                    outcome.stats.scripts += 1;
                    quest_count = qc + 1;
                    let _ = quest_count;
                }
                if compare_lstr(&line, "{~Quest") {
                    continue;
                }
            }
            // C# 死分支（ScriptParsers.cs:794）：scriptType 从不被赋 1，此分支不可达。
            // 与 C# 等价地保持为空操作（不 panic，避免 Rust 侧整进程中断）。
            #[allow(clippy::absurd_extreme_comparisons)]
            if script_type == 1 && script_idx.is_some() && line.starts_with('#') {
                // 不可达：C# 中同样不执行任何语句
            }
            if line.starts_with('[') {
                script_type = 10;
                if script_idx.is_none() {
                    outcome.scripts.push(ScriptInfo {
                        quest_count,
                        ..ScriptInfo::default()
                    });
                    script_idx = Some(outcome.scripts.len() - 1);
                    outcome.stats.scripts += 1;
                }
                if line.eq_ignore_ascii_case("[goods]") {
                    script_type = 20;
                    outcome.goods_sections += 1;
                    continue;
                }
                // C#: line = ArrestStringEx(line, "[", "]", ref slabName)；
                // slabName 在方法级声明、跨行复用（未命中定界符时保留上一行残值），此处照搬。
                let (write, rest) = arrest_string_ex_ref(&line, '[', ']');
                match write {
                    ArrestWrite::Value(v) => slab_name = v,
                    ArrestWrite::Unchanged => {}
                }
                let (token, _) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
                let bo_ext_jmp = token.eq_ignore_ascii_case("TRUE");
                let script = &mut outcome.scripts[script_idx.unwrap()];
                let mut label = slab_name.clone();
                if script.contains_label(&label) {
                    // C#: sLabel += RandomNumber.GetRandomNumber(1, 200)
                    label = format!("{}{}", label, rename_rand(1, 200));
                    outcome.stats.label_renames += 1;
                }
                if script.contains_label(&label) {
                    // C# Dictionary.Add 抛 ArgumentException
                    return Err(LoadPanic::DuplicateLabel(label));
                }
                script.record_list.push(SayingRecord {
                    s_label: label,
                    procedure_list: vec![SayingProcedure::default()],
                    bo_ext_jmp,
                });
                record_loc = Some((script_idx.unwrap(), script.record_list.len() - 1));
                outcome.stats.records += 1;
                outcome.stats.procedures += 1;
                continue;
            }
            if let Some((si, ri)) = record_loc {
                if line.starts_with('#') && (10..20).contains(&script_type) {
                    if line.eq_ignore_ascii_case("#IF") {
                        let rec = &mut outcome.scripts[si].record_list[ri];
                        let cur = rec.procedure_list.last().unwrap();
                        if !cur.condition_list.is_empty() || !cur.s_say_msg.is_empty() {
                            rec.procedure_list.push(SayingProcedure::default());
                            outcome.stats.procedures += 1;
                        }
                        script_type = 11;
                        continue;
                    }
                    if line.eq_ignore_ascii_case("#ACT") {
                        script_type = 12;
                        continue;
                    }
                    if line.eq_ignore_ascii_case("#SAY") {
                        script_type = 10;
                        continue;
                    }
                    if line.eq_ignore_ascii_case("#ELSEACT") {
                        script_type = 13;
                        continue;
                    }
                    if line.eq_ignore_ascii_case("#ELSESAY") {
                        script_type = 14;
                    }
                    // 未知 # 指令：静默丢弃
                    continue;
                }
                match script_type {
                    10 => {
                        outcome.scripts[si].record_list[ri]
                            .procedure_list
                            .last_mut()
                            .unwrap()
                            .s_say_msg
                            .push_str(&line);
                    }
                    11 => match self.load_quest_condition(&line) {
                        Ok(Some(info)) => {
                            outcome.scripts[si].record_list[ri]
                                .procedure_list
                                .last_mut()
                                .unwrap()
                                .condition_list
                                .push(info);
                            outcome.stats.conditions += 1;
                        }
                        Ok(None) => {
                            outcome.errors.push(ParseError {
                                line: line.clone(),
                                line_index: i,
                                kind: ParseErrorKind::ScriptError,
                            });
                            outcome.stats.parse_errors += 1;
                        }
                        Err(e) => return Err(e),
                    },
                    12 | 13 => match self.load_quest_action(&line) {
                        Ok(Some(info)) => {
                            let rec = &mut outcome.scripts[si].record_list[ri];
                            let proc_ = rec.procedure_list.last_mut().unwrap();
                            if script_type == 12 {
                                proc_.action_list.push(info);
                                outcome.stats.actions += 1;
                            } else {
                                proc_.else_action_list.push(info);
                                outcome.stats.else_actions += 1;
                            }
                        }
                        Ok(None) => {
                            outcome.errors.push(ParseError {
                                line: line.clone(),
                                line_index: i,
                                kind: ParseErrorKind::ScriptError,
                            });
                            outcome.stats.parse_errors += 1;
                        }
                        Err(e) => return Err(e),
                    },
                    14 => {
                        outcome.scripts[si].record_list[ri]
                            .procedure_list
                            .last_mut()
                            .unwrap()
                            .s_else_say_msg
                            .push_str(&line);
                    }
                    _ => {}
                }
            }
            if script_type == 20 && bo_flag {
                let (s_item_name, rest) = get_valid_str_cap(&line, TEXT_SPLIT)?;
                let (s_item_count, rest) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
                let (s_item_refill_time, _) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
                if !s_item_name.is_empty() && !s_item_refill_time.is_empty() {
                    let mut item_name = s_item_name;
                    if item_name.starts_with('"') {
                        // C#: ArrestStringEx(sItemName, "\"", "\"", ref sItemName)
                        let (write, _) = arrest_string_ex_ref(&item_name, '"', '"');
                        match write {
                            ArrestWrite::Value(v) => item_name = v,
                            ArrestWrite::Unchanged => {}
                        }
                    }
                    outcome.refill_goods.push(Goods {
                        item_name,
                        count: str_to_int(&s_item_count, 0),
                        refill_time: str_to_int(&s_item_refill_time, 0),
                    });
                    outcome.stats.goods += 1;
                }
            }
        }
        Ok(Some(outcome))
    }

    /// `LoadCallScript`：处理 `#CALL [file] label`。返回 success（对应 C# out success）。
    fn load_call_script<FS: ScriptFs>(
        &self,
        fs: &FS,
        envir_root: &Path,
        lines: &mut Vec<String>,
        call_script_dict: &mut HashMap<String, String>,
        outcome: &mut LoadOutcome,
    ) -> Result<bool, LoadPanic> {
        // ScriptHelper.GetScriptCallCount(text)：全文本中 "#CALL" 大小写不敏感出现次数
        let mut call_count: usize = lines
            .iter()
            .map(|l| count_occurrences_ignore_case(l, "#CALL"))
            .sum();
        if call_count == 0 {
            return Ok(true);
        }
        let mut call_list: SizedStringList = SizedStringList::new();
        // C# 方法级局部变量：sLable 跨行复用（同 LoadCallScript，未命中定界符时保留残值）
        let mut s_lable = String::new();
        #[allow(clippy::needless_range_loop)]
        for i in 0..lines.len() {
            let sline = lines[i].trim().to_string();
            if !sline.is_empty() && sline.starts_with('#') && compare_lstr(&sline, "#CALL") {
                // C#: sLine = ArrestStringEx(sLine, "[", "]", ref sLable)
                let (write, rest) = arrest_string_ex_ref(&sline, '[', ']');
                match write {
                    ArrestWrite::Value(v) => s_lable = v,
                    ArrestWrite::Unchanged => {}
                }
                let call_script_file = get_call_script_path(s_lable.trim());
                let lab_name = rest.trim().to_string();
                let file_name = get_envir_file_path(envir_root, "QuestDiary", &call_script_file);
                // C# 字典为默认序数比较，键是原样路径字符串（大小写敏感、不归一分隔符）
                let file_key = file_name.to_string_lossy().into_owned();
                if call_script_dict.contains_key(&file_key) {
                    call_count -= 1;
                    // C#: callList[i] = "#ACT"（i >= Count 时抛 ArgumentOutOfRangeException）
                    call_list.set(i, "#ACT".to_string())?;
                    // C#: callList.InsertText(i + 1, "goto " + sLabName)
                    call_list.insert_text(i + 1, format!("goto {lab_name}"));
                    break;
                }
                match self.load_script_call_script(fs, &file_name, &lab_name, &mut call_list) {
                    true => {
                        call_count -= 1;
                        outcome.stats.call_loads += 1;
                        // C# 缺陷：ContainsKey 查的是 sLabName，Add 用的键是 sFileName
                        if !call_script_dict.contains_key(&lab_name) {
                            call_script_dict.insert(file_key, lab_name.clone());
                        }
                    }
                    false => {
                        outcome.errors.push(ParseError {
                            line: format!("{call_script_file}{lab_name}"),
                            line_index: i,
                            kind: ParseErrorKind::CallLoadFail,
                        });
                        outcome.stats.call_failures += 1;
                    }
                }
            } else {
                call_list.append_text(sline);
            }
        }
        *lines = call_list.into_vec();
        Ok(call_count == 0)
    }

    /// `LoadScriptCallScript`：把被 call 文件的内容并入列表。
    ///
    /// C# 行为（含缺陷）：从文件头开始，跳过空行与 `{` 开头的行，其余行全部加入，
    /// 直到遇到 `}` 开头的行（返回 true）或文件尾（返回 false，但已加的行不回滚）。
    /// label 匹配行本身被跳过，`findLab` 无其他作用。
    fn load_script_call_script<FS: ScriptFs>(
        &self,
        fs: &FS,
        file_name: &Path,
        label: &str,
        list: &mut SizedStringList,
    ) -> bool {
        let mut result = false;
        if let Some(call_lines) = fs.load_string_list(file_name) {
            let target = format!("[{label}]");
            let mut find_lab = false;
            for raw in &call_lines {
                let sline = raw.trim();
                if sline.is_empty() {
                    continue;
                }
                if !find_lab && sline.starts_with('[') && sline.eq_ignore_ascii_case(&target) {
                    find_lab = true;
                    continue;
                }
                if !sline.starts_with('{') {
                    if sline.starts_with('}') {
                        result = true;
                        break;
                    }
                    list.add(sline.to_string());
                }
            }
        }
        result
    }

    /// `LoadScriptDefineInfo`：处理 `#SETHOME` / `#DEFINE` / `#INCLUDE`（递归）。
    fn load_script_define_info<FS: ScriptFs>(
        &self,
        fs: &FS,
        envir_root: &Path,
        lines: &mut [String],
        define_list: &mut Vec<(String, String)>,
        defline: &mut String,
        outcome: &mut LoadOutcome,
    ) {
        #[allow(clippy::needless_range_loop)]
        for i in 0..lines.len() {
            let line = lines[i].trim().to_string();
            if line.is_empty() || !line.starts_with('#') {
                continue;
            }
            if compare_lstr(&line, "#SETHOME") {
                // C#: result = GetValidStr3(line, ref defFile, ...).Trim() —— 取整段剩余
                let (_, rest) = get_valid_str3(&line, TEXT_SPLIT);
                *defline = rest.trim().to_string();
                lines[i] = String::new();
                continue;
            }
            if compare_lstr(&line, "#DEFINE") {
                let (_, rest) = get_valid_str3(&line, TEXT_SPLIT);
                let (define_name, rest) = get_valid_str3(&rest, TEXT_SPLIT);
                let (def_text, _) = get_valid_str3(&rest, TEXT_SPLIT);
                define_list.push((to_upper(&define_name), def_text));
                lines[i] = String::new();
                continue;
            }
            if compare_lstr(&line, "#INCLUDE") {
                // C#: definesFile = GetValidStr3(line, ref defFile, ...).Trim() —— 取整段剩余
                let (_, rest) = get_valid_str3(&line, TEXT_SPLIT);
                let defines_file = rest.trim().to_string();
                let path = get_envir_file_path(envir_root, "Defines", &defines_file);
                if let Some(mut incl_lines) = fs.load_string_list(&path) {
                    self.load_script_define_info(
                        fs,
                        envir_root,
                        &mut incl_lines,
                        define_list,
                        defline,
                        outcome,
                    );
                    outcome.stats.include_loads += 1;
                } else {
                    outcome.stats.include_failures += 1;
                    outcome.errors.push(ParseError {
                        line: defines_file,
                        line_index: i,
                        kind: ParseErrorKind::IncludeLoadFail,
                    });
                }
                lines[i] = String::new();
                continue;
            }
        }
    }

    /// `LoadScriptFileQuestCondition`：解析条件行。Ok(None) = 解析失败（C# 记“脚本错误”）。
    fn load_quest_condition(&self, s_text: &str) -> Result<Option<QuestConditionInfo>, LoadPanic> {
        let mut rest = s_text.to_string();
        let mut params: [String; 6] = Default::default();
        let (cmd, r) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
        rest = r;
        let mut s_cmd = cmd;
        for p in &mut params {
            let (v, r) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
            *p = v;
            rest = r;
        }
        let mut info = QuestConditionInfo::default();
        // 支持脚本变量：CMD.<名字>[.H]
        if s_cmd.contains('.') {
            let (act_name, r) = get_valid_str_cap(&s_cmd, &['.'])?;
            s_cmd = r;
            if !act_name.is_empty() {
                info.s_op_name = act_name;
                // C# 缺陷：'.".IndexOf(sCmd)' —— sCmd 为 "" 或 "." 时才 > -1
                if s_cmd.is_empty() || s_cmd == "." {
                    let (act_name2, r2) = get_valid_str_cap(&s_cmd, &['.'])?;
                    s_cmd = r2;
                    if act_name2.eq_ignore_ascii_case("H") {
                        info.s_op_h_name = "H".to_string();
                    }
                }
            }
        }
        s_cmd = to_upper(&s_cmd);

        let mut n_cmd_code = 0i32;
        if let Some(def) = def_of(CONDITION_CODES, &s_cmd) {
            // C# 三个特判分支（CHECK / CHECKOPEN / CHECKUNIT）体相同：不做 code-1，
            // 且对 sParam1 做 [...] 剥壳；IsStringNumber 恒 true，其后的 nCMDCode 清零判断是死代码。
            // C# 比的是**枚举值** `(int)ConditionCode.CHECK`，而 map 存的是字段序号——
            // 这里按枚举值比较，杜绝枚举重编号后静默错判（当前两列相等，见 codegen_drift 锚点测试）。
            if def.enum_value == enum_value_of(CONDITION_CODES, "CHECK")
                || def.enum_value == enum_value_of(CONDITION_CODES, "CHECKOPEN")
                || def.enum_value == enum_value_of(CONDITION_CODES, "CHECKUNIT")
            {
                n_cmd_code = def.field_index;
                // C#: ArrestStringEx(sParam1, "[", "]", ref sParam1)（写回三态照搬）
                let (write, _) = arrest_string_ex_ref(&params[0], '[', ']');
                match write {
                    ArrestWrite::Value(v) => params[0] = v,
                    ArrestWrite::Unchanged => {}
                }
            } else {
                // 同批翻转（B2/M4，2026-10-11）：crates/script 曾按 T-4 默认"复刻旧位移"（字段序号减一）。
                // S3 已在 C# 侧把 `code - 1` 改成 `code`（whitelist B-8 / T-4），继续复刻等于长期维护一个刻意偏差；
                // 协调者已裁定 Rust 同批翻转 ⇒ 非特判分支同样直接取字段序号。
                n_cmd_code = def.field_index;
            }
        }

        if n_cmd_code > 0 {
            info.cmd_code = n_cmd_code;
            for p in &mut params {
                if p.starts_with('"') {
                    // C#: ArrestStringEx(p, "\"", "\"", ref p)
                    let (write, _) = arrest_string_ex_ref(p, '"', '"');
                    match write {
                        ArrestWrite::Value(v) => *p = v,
                        ArrestWrite::Unchanged => {}
                    }
                }
            }
            info.s_param1 = params[0].clone();
            info.s_param2 = params[1].clone();
            info.s_param3 = params[2].clone();
            info.s_param4 = params[3].clone();
            info.s_param5 = params[4].clone();
            info.s_param6 = params[5].clone();
            if is_string_number(&info.s_param1) {
                info.n_param1 = str_to_int(&info.s_param1, 0);
            }
            if is_string_number(&info.s_param2) {
                info.n_param2 = str_to_int(&info.s_param2, 0);
            }
            if is_string_number(&info.s_param3) {
                info.n_param3 = str_to_int(&info.s_param3, 0);
            }
            if is_string_number(&info.s_param4) {
                info.n_param4 = str_to_int(&info.s_param4, 0);
            }
            if is_string_number(&info.s_param5) {
                info.n_param5 = str_to_int(&info.s_param5, 0);
            }
            if is_string_number(&info.s_param6) {
                info.n_param6 = str_to_int(&info.s_param6, 0);
            }
            return Ok(Some(info));
        }
        Ok(None)
    }

    /// `LoadScriptFileQuestAction`：解析动作行。
    fn load_quest_action(&self, s_text: &str) -> Result<Option<QuestActionInfo>, LoadPanic> {
        let mut rest = s_text.to_string();
        let mut params: [String; 6] = Default::default();
        let (cmd, r) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
        rest = r;
        let mut s_cmd = cmd;
        for p in &mut params {
            let (v, r) = get_valid_str_cap(&rest, TEXT_SPLIT)?;
            *p = v;
            rest = r;
        }
        let mut info = QuestActionInfo::default();
        if s_cmd.contains('.') {
            let (act_name, r) = get_valid_str_cap(&s_cmd, &['.'])?;
            s_cmd = r;
            if !act_name.is_empty() {
                info.s_op_name = act_name;
                // 动作版是正常检查（与条件版的缺陷不对称，ScriptParsers.cs:420）
                if s_cmd.contains('.') {
                    let (act_name2, r2) = get_valid_str_cap(&s_cmd, &['.'])?;
                    s_cmd = r2;
                    if act_name2.eq_ignore_ascii_case("H") {
                        info.s_op_h_name = "H".to_string();
                    }
                }
            }
        }
        s_cmd = to_upper(&s_cmd);

        let mut n_cmd_code = 0i32;
        if let Some(def) = def_of(EXECUTION_CODES, &s_cmd) {
            // C# 特判集合（Set/ReSet/SetOpen/SetUnit/ResetUnit）按**枚举值**比较
            let specials = ["Set", "ReSet", "SetOpen", "SetUnit", "ResetUnit"]
                .map(|m| enum_value_of(EXECUTION_CODES, m));
            if specials.contains(&def.enum_value) {
                n_cmd_code = def.field_index;
                // C#: ArrestStringEx(sParam1, "[", "]", ref sParam1)
                let (write, _) = arrest_string_ex_ref(&params[0], '[', ']');
                match write {
                    ArrestWrite::Value(v) => params[0] = v,
                    ArrestWrite::Unchanged => {}
                }
            } else {
                // 同批翻转（B2/M4）：与条件版一致，取字段序号而非减一（见上方说明）。
                n_cmd_code = def.field_index;
            }
        }

        if n_cmd_code > 0 {
            info.n_cmd_code = n_cmd_code;
            for p in &mut params {
                if p.starts_with('"') {
                    // C#: ArrestStringEx(p, "\"", "\"", ref p)
                    let (write, _) = arrest_string_ex_ref(p, '"', '"');
                    match write {
                        ArrestWrite::Value(v) => *p = v,
                        ArrestWrite::Unchanged => {}
                    }
                }
            }
            info.s_param1 = params[0].clone();
            info.s_param2 = params[1].clone();
            info.s_param3 = params[2].clone();
            info.s_param4 = params[3].clone();
            info.s_param5 = params[4].clone();
            info.s_param6 = params[5].clone();
            // C# 默认值：nParam1=0, nParam2=1, nParam3=1, nParam4..6=0
            if is_string_number(&info.s_param1) {
                info.n_param1 = str_to_int(&info.s_param1, 0);
            }
            if is_string_number(&info.s_param2) {
                info.n_param2 = str_to_int(&info.s_param2, 1);
            }
            if is_string_number(&info.s_param3) {
                info.n_param3 = str_to_int(&info.s_param3, 1);
            }
            if is_string_number(&info.s_param4) {
                info.n_param4 = str_to_int(&info.s_param4, 0);
            }
            if is_string_number(&info.s_param5) {
                info.n_param5 = str_to_int(&info.s_param5, 0);
            }
            if is_string_number(&info.s_param6) {
                info.n_param6 = str_to_int(&info.s_param6, 0);
            }
            return Ok(Some(info));
        }
        Ok(None)
    }
}

/// 码表中按命令名（大小写不敏感，对应 C# OrdinalIgnoreCase 字典）取定义。
fn def_of(
    defs: &'static [crate::codes::CodeDef],
    name: &str,
) -> Option<&'static crate::codes::CodeDef> {
    defs.iter().find(|d| d.name.eq_ignore_ascii_case(name))
}

/// 码表中某成员的枚举值。
fn enum_value_of(defs: &'static [crate::codes::CodeDef], member: &str) -> i32 {
    defs.iter()
        .find(|d| d.member == member)
        .map(|d| d.enum_value)
        .unwrap_or(-1)
}

/// 大小写不敏感计数 needle 在 haystack 中的出现次数（ScriptHelper.GetScriptCallCount）。
fn count_occurrences_ignore_case(haystack: &str, needle: &str) -> usize {
    let h = to_upper(haystack);
    let n = to_upper(needle);
    let hc: Vec<char> = h.chars().collect();
    let nc: Vec<char> = n.chars().collect();
    if nc.is_empty() || hc.len() < nc.len() {
        return 0;
    }
    let mut count = 0;
    let mut i = 0;
    while i + nc.len() <= hc.len() {
        if hc[i..i + nc.len()] == nc[..] {
            count += 1;
            i += 1; // 允许重叠（正则从左到右不重叠？RightToLeft 计数——重叠场景无实际差异）
        } else {
            i += 1;
        }
    }
    count
}

/// 在 `hay` 中从 `from` 起查找 `needle`（前置假设：hay 已大写、needle 已大写）。
fn find_from(hay: &str, needle: &str, from: usize) -> Option<usize> {
    let hc: Vec<char> = hay.chars().collect();
    let nc: Vec<char> = needle.chars().collect();
    if nc.is_empty() || hc.len() < nc.len() + from {
        return None;
    }
    let mut i = from;
    while i + nc.len() <= hc.len() {
        if hc[i..i + nc.len()] == nc[..] {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// `IMerchant` 可执行命令标志设置（`(...)` 头行）。
fn set_merchant_flag(flags: &mut MerchantFlags, command: &str) {
    if command.eq_ignore_ascii_case("@buy") {
        flags.is_buy = true;
    } else if command.eq_ignore_ascii_case("@sell") {
        flags.is_sell = true;
    } else if command.eq_ignore_ascii_case("@makedrug") {
        flags.is_make_drug = true;
    } else if command.eq_ignore_ascii_case("@prices") {
        flags.is_prices = true;
    } else if command.eq_ignore_ascii_case("@storage") {
        flags.is_storage = true;
    } else if command.eq_ignore_ascii_case("@getback") {
        flags.is_getback = true;
    } else if command.eq_ignore_ascii_case("@upgradenow") {
        flags.is_upgradenow = true;
    } else if command.eq_ignore_ascii_case("@getbackupgnow") {
        flags.is_get_backupgnow = true;
    } else if command.eq_ignore_ascii_case("@repair") {
        flags.is_repair = true;
    } else if command.eq_ignore_ascii_case("@s_repair") {
        flags.is_sup_repair = true;
    } else if command.eq_ignore_ascii_case("@@sendmsg") {
        flags.is_send_msg = true;
    } else if command.eq_ignore_ascii_case("@@useitemname") {
        flags.is_use_item_name = true;
    } else if command.eq_ignore_ascii_case("@@offlinemsg") {
        flags.is_offline_msg = true;
    } else if command.eq_ignore_ascii_case("@ybdeal") {
        flags.is_yb_deal = true;
    }
}

/// 带 C# `StringList` 索引语义的行列表（`LoadCallScript` 专用）。
///
/// - `set(i, v)`：`i >= len` 时对应 C# 索引器抛 `ArgumentOutOfRangeException`；
/// - `insert_text(i, v)`：`i < len` 移位插入；`i == len` 追加；
///   `i > len` 为 C# 退化路径（内容不可见、仅 size+1），以空串占位近似并留注释。
struct SizedStringList {
    inner: Vec<String>,
}

impl SizedStringList {
    fn new() -> Self {
        Self {
            inner: Vec::with_capacity(1024),
        }
    }

    fn add(&mut self, v: String) {
        self.inner.push(v);
    }

    fn append_text(&mut self, v: String) {
        self.inner.push(v);
    }

    fn set(&mut self, index: usize, v: String) -> Result<(), LoadPanic> {
        if index >= self.inner.len() {
            return Err(LoadPanic::CallListOutOfRange);
        }
        self.inner[index] = v;
        Ok(())
    }

    fn insert_text(&mut self, index: usize, v: String) {
        if index < self.inner.len() {
            self.inner.insert(index, v);
        } else if index == self.inner.len() {
            self.inner.push(v);
        } else {
            // C# InsertText 退化路径：写入位置超出可读范围，仅 size 增加（内容出现空洞）。
            // 语料若触发此路径，C# 侧行为本身已损坏；用空串占位保持长度语义一致。
            self.inner.push(String::new());
        }
    }

    fn into_vec(self) -> Vec<String> {
        self.inner
    }
}
