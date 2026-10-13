//! NPC 脚本运行时宿主（M3⑥-实现 3 第①片：**离线核心**）——`openmir2-m3-quest-dialog-wiring`。
//!
//! 本模块把 `crates/script` 的引擎（M4 已交付）接到 GameSvr 的**世界宿主**这一侧：
//! 装载隐藏任务 NPC 的脚本、按 oracle 的 `NpcGotoLable` 口径进入 `GotoLable`，并把
//! `GotoLableSendMerChantSayMsg` 的说词收成可下发帧。**本片不碰 `main.rs` 的会话接线**
//! （`SessionGround`/`World` → [`NpcPlayer`] 的映射、socket 下发）——那是第②片。
//!
//! # oracle 锚（逐行）
//!
//! ```text
//! 进世界 @Login（PlayObject.cs:1440）：
//!   if (SystemShare.ManageNPC != null) SystemShare.ManageNPC.GotoLable(this, "@Login", false);
//!     └─ INormNpc.GotoLable（NormNpc.cs:1889）→ ScriptEngine.GotoLable（ScriptEngine.cs:38）
//!   NpcGotoLable（PlayObject.cs:4880）先把 ScriptGotoCount 清零再转发（本模块的 goto_count_limit 同源）
//!
//! 用物品 @StdModeFunc*（PlayObject.Base.cs:1072 UseStdModeFunItem）：
//!   if (SystemShare.FunctionNPC != null) { FunctionNPC.GotoLable(this, "@StdModeFunc" + stdItem.AniCount, false); }
//!     └─ FunctionNPC = QFunction（`Market_Def/QFunction-0.txt`，Merchant 装载）
//!
//! 说词帧（ScriptEngine.cs:987-998 GotoLableSendMerChantSayMsg）：
//!   sMsg = normNpc.GetLineVariableText(playerActor, sMsg);
//!   playerActor.GetScriptLabel(sMsg);
//!   boFlag ⇒ SendPriorityMsg(RM_MERCHANTSAY, 0,0,0,0, normNpc.ChrName + '/' + sMsg, High)
//!   否则   ⇒ SendMsg(normNpc, RM_MERCHANTSAY, 0,0,0,0, normNpc.ChrName + '/' + sMsg)
//! 下行（PlayObject.Message.cs:1702-1724）：
//!   SM_MERCHANTSAY(643) = MakeMessage(SM_MERCHANTSAY, ActorId, MakeWord(nParam1,nParam2), 0, 1)
//!                         尾 = EDCode.EncodeString(ChrName + '/' + sMsg)
//!
//! 脚本装载（NormNpc.cs:1591 LoadNPCScript / Merchant.cs:1094）：
//!   任务 NPC：ScriptParsers.LoadScript(this, FilePath, ChrName)  ⇒ boFlag = false（ScriptParsers.cs:65-72）
//!   QFunction：ScriptParsers.LoadScriptFile(this, "Market_Def", scriptPath, true) ⇒ boFlag = true
//! ```
//!
//! # 与 oracle 的边界（本片登记，不在本片范围）
//!
//! - `playerActor.GetScriptLabel(sMsg)`（`<$...>`/`@...` 链的登记表）未建模：本片只产说词行。
//! - [`NpcPlayer`] 的字段目前由**调用方**（第②片：`SessionGround`/`World` 适配）填充；
//!   本片只保证「引擎 → 适配器 → 说词」这条链的取数/次序与 oracle 一致。
//! - 隐藏 NPC 的 `ActorId`：oracle 里它们不经地图、`BaseObject.ActorId` 恒 0（本模块同）。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use mir2_envir::mapquest::QuestNpcRecord;
use mir2_script::engine::{Engine, ScriptNpc, ScriptPlayer};
use mir2_script::{LocalFs, ScriptInfo, ScriptParsers};

/// `GotoLableSendMerChantSayMsg`（`ScriptEngine.cs:991/996`）的说词行：`normNpc.ChrName + '/' + sMsg`。
#[must_use]
pub fn merchant_say_text(chr_name: &str, msg: &str) -> String {
    format!("{chr_name}/{msg}")
}

/// 说词行 → oracle 线上字节。协议层一律原始字节（GBK），本层**不**做 UTF-8 转换。
#[must_use]
pub fn merchant_say_wire(chr_name: &str, msg: &str) -> Vec<u8> {
    let text = merchant_say_text(chr_name, msg);
    encoding_rs::GBK.encode(&text).0.into_owned()
}

/// 一条待下发的 `RM_MERCHANTSAY`（`ScriptEngine.cs:991/996` 的两个入队点）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SayMsg {
    /// `normNpc.ChrName`（说词前缀）。
    pub npc_name: String,
    /// 引擎产出的**原始**说词（`GetLineVariableText` 之后；未加 `ChrName + '/'`）。
    pub raw_msg: String,
    /// 线上行 = `npc_name + '/' + raw_msg`（[`merchant_say_text`]）。
    pub text: String,
    /// `true` = `SendPriorityMsg(..., High)`（`:993`）；`false` = `SendMsg`（`:997`）。
    pub priority: bool,
}

/// oracle `MsgQueue` 的**出队序**（`BaseObject.Base.cs:25-32` 的 `while (GetMessage(...))` +
/// `BaseObject.Message.cs:334` 的 `TryDequeue`）：
///
/// - `SendPriorityMsg(..., MessagePriority.High)` 入队优先级 = `(byte)High = 0`（`BaseObject.Message.cs:30`）；
/// - `SendMsg(normNpc, ...)` 入队优先级 = `(byte)RM_MERCHANTSAY = 11009 & 0xFF = 1`（`:180`）。
///
/// ⇒ 出队是**优先级字节升序**：`High` 组（0）先、`SendMsg` 组（1）后。oracle 用的是
/// `PriorityQueue`（同优先级不保证稳定序），本函数只按两个**不同**优先级分组，组内取**入队序**
/// （即引擎调用序）。这正是 `②③` F-1 那条「入队序 ≠ 出队序」的同源规则（那里三段优先级互不相同）。
#[must_use]
pub fn dequeue_order(says: &[SayMsg]) -> Vec<&SayMsg> {
    let mut out: Vec<&SayMsg> = says.iter().filter(|s| s.priority).collect();
    out.extend(says.iter().filter(|s| !s.priority));
    out
}

/// 脚本引擎 `playerActor.SendMsg(...)` 一次调用的原始实参（供第②片按 `MsgQueue` 出队序落帧）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SendMsg {
    pub ident: i32,
    pub w_param: i32,
    pub n_param1: i32,
    pub n_param2: i32,
    pub n_param3: i32,
    pub msg: Option<String>,
}

/// 脚本文件装载失败（`LoadPanic`：C# 异常穿透，调用方登记；不静默吞）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcLoadFailure {
    pub chr_name: String,
    pub file_path: String,
    pub script_name: String,
    pub reason: String,
}

/// 一个 NPC 的**运行时**记录（身份 + 脚本）。
#[derive(Debug)]
pub struct NpcDef {
    /// `NormNpc.ChrName`。
    pub chr_name: String,
    /// `NormNpc.FilePath`（`MapQuest_def`/`Market_Def`/`Robot_def`）。
    pub file_path: String,
    /// `Merchant.ScriptName`（仅 `QFunction` 为 `Some("QFunction")`）。
    pub script_name: Option<String>,
    pub is_hide: bool,
    pub is_quest: bool,
    /// `NormNpc.ScriptList`（装载顺序 = 文件里出现的顺序）。
    pub scripts: Vec<ScriptInfo>,
    /// 脚本文件是否实际存在（oracle `File.Exists` 口径；缺 ⇒ 不解析、不报错、`ScriptList` 为空）。
    pub loaded: bool,
}

/// NPC 脚本运行时宿主。
#[derive(Debug, Default)]
pub struct NpcHost {
    /// Envir 根（`ScriptParsers.LoadScriptFile` 的 `envir_root`）。
    pub envir_root: PathBuf,
    /// 已装载的 NPC（顺序 = `add_quest_npc` 先、`add_merchant` 后）。
    pub defs: Vec<NpcDef>,
    /// 装载失败的脚本（`LoadPanic`）。
    pub failures: Vec<NpcLoadFailure>,
    /// `SystemShare.Config.ScriptGotoCountLimit`（`ScriptNpc::script_goto_count_limit`）。
    pub goto_count_limit: i32,
}

impl NpcHost {
    /// 按 oracle 的装载口径建宿主：任务 NPC 走 `LoadScript`（`boFlag=false`），
    /// `QFunction` 走 `Merchant.LoadScriptFile(..., true)`（`boFlag=true`）。
    ///
    /// `rename_rand` 是解析器重名 label 改名用的随机源（oracle `RandomNumber.GetRandomNumber(1,200)`；
    /// 生产路径必须传**世界同一条** RNG，见 R-4/Claim B）。
    pub fn load(
        envir_root: &Path,
        quest_npcs: &[QuestNpcRecord],
        merchants: &[QuestNpcRecord],
        goto_count_limit: i32,
        rename_rand: &mut dyn FnMut(i32, i32) -> i32,
    ) -> Self {
        let parsers = ScriptParsers::new();
        let fs = LocalFs;
        let mut defs: Vec<NpcDef> = Vec::new();
        let mut failures: Vec<NpcLoadFailure> = Vec::new();
        // oracle 装载序：`MapQuestManager` 的 `AddQuestNpc` 记录先，`AddMerchant`（QFunction）后。
        for record in quest_npcs.iter().chain(merchants.iter()) {
            // `Merchant`（ScriptName 非空）走 `LoadScriptFile(..., boFlag=true)`；其余 `LoadScript` ⇒ false。
            let bo_flag = record.script_name.is_some();
            let script_name = record
                .script_name
                .clone()
                .unwrap_or_else(|| record.chr_name.clone());
            let (scripts, loaded) = match parsers.load_script_file(
                &fs,
                envir_root,
                &record.file_path,
                &script_name,
                bo_flag,
                rename_rand,
            ) {
                Ok(Some(outcome)) => (outcome.scripts, true),
                Ok(None) => (Vec::new(), false),
                Err(panic) => {
                    failures.push(NpcLoadFailure {
                        chr_name: record.chr_name.clone(),
                        file_path: record.file_path.clone(),
                        script_name: script_name.clone(),
                        reason: panic.to_string(),
                    });
                    (Vec::new(), false)
                }
            };
            defs.push(NpcDef {
                chr_name: record.chr_name.clone(),
                file_path: record.file_path.clone(),
                script_name: record.script_name.clone(),
                is_hide: record.is_hide,
                is_quest: record.is_quest,
                scripts,
                loaded,
            });
        }
        NpcHost {
            envir_root: envir_root.to_path_buf(),
            defs,
            failures,
            goto_count_limit,
        }
    }

    /// `QuestNpcList`/`MerchantList` 的 `FindNPC`/`GetNpc` 口径（`ChrName`，大小写不敏感）。
    #[must_use]
    pub fn find(&self, chr_name: &str) -> Option<usize> {
        self.defs
            .iter()
            .position(|d| d.chr_name.eq_ignore_ascii_case(chr_name))
    }

    /// `SystemShare.ManageNPC != null`（`WorldServer` 装载 `QManage` 后置位）：
    /// `@Login` 的守卫（`PlayObject.cs:1440` 的 `if (SystemShare.ManageNPC != null)`）。
    #[must_use]
    pub fn has_manage_npc(&self) -> bool {
        self.find("QManage").is_some_and(|i| self.defs[i].loaded)
    }

    /// `SystemShare.FunctionNPC != null`（`QFunction` 装载后置位）：
    /// `UseStdModeFunItem` 的守卫（`PlayObject.Base.cs:1072`）。
    #[must_use]
    pub fn has_function_npc(&self) -> bool {
        self.find("QFunction").is_some_and(|i| self.defs[i].loaded)
    }

    /// `PlayObject.NpcGotoLable` → `INormNpc.GotoLable` 的等价物（`bo_ext_jmp` 恒 `false` 的两处入口）。
    ///
    /// 返回该 NPC 本次产出的说词；`None` = 找不到该 `ChrName`（oracle `actor != null` 的守卫）。
    pub fn goto_label(
        &self,
        chr_name: &str,
        player: &mut NpcPlayer<'_>,
        label: &str,
        bo_ext_jmp: bool,
    ) -> Option<Vec<SayMsg>> {
        let idx = self.find(chr_name)?;
        let def = &self.defs[idx];
        let mut npc = HostNpc {
            def,
            says: Vec::new(),
            goto_count_limit: self.goto_count_limit,
        };
        {
            let mut engine = Engine::new(&mut npc, player);
            engine.goto_label(label, bo_ext_jmp);
        }
        Some(npc.says)
    }

    /// `@Login`（`PlayObject.cs:1440` 用 `SystemShare.ManageNPC`，即 `QManage`）。
    pub fn run_login(&self, player: &mut NpcPlayer<'_>) -> Option<Vec<SayMsg>> {
        self.goto_label("QManage", player, "@Login", false)
    }

    /// `@StdModeFunc<N>`（`PlayObject.Base.cs:1075` 用 `SystemShare.FunctionNPC`，即 `QFunction`）。
    pub fn run_std_mode_func(
        &self,
        ani_count: i32,
        player: &mut NpcPlayer<'_>,
    ) -> Option<Vec<SayMsg>> {
        self.goto_label(
            "QFunction",
            player,
            &format!("@StdModeFunc{ani_count}"),
            false,
        )
    }
}

/// `ScriptNpc` 适配：借 [`NpcDef`] 的脚本表，收集说词。
struct HostNpc<'a> {
    def: &'a NpcDef,
    says: Vec<SayMsg>,
    goto_count_limit: i32,
}

impl ScriptNpc for HostNpc<'_> {
    fn actor_id(&self) -> i32 {
        // 隐藏 NPC 不经地图，`BaseObject.ActorId` 恒 0。
        0
    }
    fn chr_name(&self) -> String {
        self.def.chr_name.clone()
    }
    fn map_name(&self) -> String {
        "0".to_string()
    }
    fn curr_x(&self) -> i32 {
        0
    }
    fn curr_y(&self) -> i32 {
        0
    }
    fn script_list(&self) -> &Vec<ScriptInfo> {
        &self.def.scripts
    }
    fn send_say(&mut self, msg: &str, priority: bool) {
        // `ScriptEngine.cs:989-998`：`normNpc.ChrName + '/' + sMsg`。
        self.says.push(SayMsg {
            npc_name: self.def.chr_name.clone(),
            raw_msg: msg.to_string(),
            text: merchant_say_text(&self.def.chr_name, msg),
            priority,
        });
    }
    fn script_goto_count_limit(&self) -> i32 {
        self.goto_count_limit
    }
    fn path(&self) -> String {
        self.def.file_path.clone()
    }
}

/// `IPlayerActor` 的宿主适配（第①片：字段由调用方填充；默认值与 C# flow 夹具一致）。
///
/// 生产路径（第②片）在进世界时用 `SessionGround`/`World` 的快照填这些字段，运行后把
/// 变更（金币/物品/任务位/`LastNpc`/`Script`/`ScriptGotoCount`）写回；红检/用例直接用字段。
pub struct NpcPlayer<'r> {
    pub actor_id: i32,
    pub chr_name: String,
    pub map_name: String,
    pub level: i32,
    /// `playerActor.Gold`
    pub gold: i32,
    /// `playerActor.LastNpc`
    pub last_npc: i32,
    /// `playerActor.Script`（脚本下标）
    pub script_index: Option<usize>,
    /// `playerActor.ScriptGotoCount`
    pub script_goto_count: i32,
    /// `PlayObject.QuestFlag`（`PlayObject.cs:1040` 的 `QuestFlag = new byte[128]`）
    pub quest_flag: [u8; 128],
    /// `playerActor.QuestUnit`（index → 位值）
    pub quest_unit: HashMap<i32, i32>,
    /// `playerActor.QuestUnitOpen`
    pub quest_unit_open: HashMap<i32, i32>,
    /// `playerActor.ItemList`（名 → 件数）
    pub items: Vec<(String, i32)>,
    /// `playerActor.UseItems`（位置名 → 是否佩戴）
    pub worn: HashMap<String, bool>,
    /// `playerActor.SlaveList.Count`
    pub slaves: i32,
    pub mn_val: [i32; 100],
    pub mn_integer: [i32; 100],
    pub mdy_val: HashMap<usize, i32>,
    pub mnmval: HashMap<usize, i32>,
    pub ms_string: HashMap<usize, String>,
    /// 引擎调用 `SendMsg` 的实参序列（保序）。
    pub frames: Vec<SendMsg>,
    /// `RandomNumber.Random` —— 必须**世界同一条流**（R-4/Claim B 的输入）。
    pub rng: &'r mut dyn FnMut(i32) -> i32,
}

impl NpcPlayer<'_> {
    /// 空会话（用例/第②片快照的起点）。
    pub fn empty<'r>(rng: &'r mut dyn FnMut(i32) -> i32) -> NpcPlayer<'r> {
        NpcPlayer {
            actor_id: 0,
            chr_name: String::new(),
            map_name: String::new(),
            level: 1,
            gold: 0,
            last_npc: 0,
            script_index: None,
            script_goto_count: 0,
            quest_flag: [0u8; 128],
            quest_unit: HashMap::new(),
            quest_unit_open: HashMap::new(),
            items: Vec::new(),
            worn: HashMap::new(),
            slaves: 0,
            mn_val: [0; 100],
            mn_integer: [0; 100],
            mdy_val: HashMap::new(),
            mnmval: HashMap::new(),
            ms_string: HashMap::new(),
            frames: Vec::new(),
            rng,
        }
    }
}

impl ScriptPlayer for NpcPlayer<'_> {
    fn last_npc(&self) -> i32 {
        self.last_npc
    }
    fn set_last_npc(&mut self, actor_id: i32) {
        self.last_npc = actor_id;
    }
    fn script_index(&self) -> Option<usize> {
        self.script_index
    }
    fn set_script_index(&mut self, idx: Option<usize>) {
        self.script_index = idx;
    }
    fn script_goto_count(&self) -> i32 {
        self.script_goto_count
    }
    fn set_script_goto_count(&mut self, v: i32) {
        self.script_goto_count = v;
    }
    fn quest_flag_status(&self, flag: i16) -> u8 {
        let i = flag.max(0) as usize;
        self.quest_flag.get(i).copied().unwrap_or(0)
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
        self.actor_id
    }
    fn level(&self) -> i32 {
        self.level
    }
    fn random(&mut self, value: i32) -> i32 {
        (self.rng)(value)
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
        self.frames.push(SendMsg {
            ident,
            w_param,
            n_param1,
            n_param2,
            n_param3,
            msg: msg.map(str::to_string),
        });
    }
    fn mn_val(&self, n: usize) -> i32 {
        self.mn_val.get(n).copied().unwrap_or(0)
    }
    fn set_mn_val(&mut self, n: usize, v: i32) {
        if let Some(slot) = self.mn_val.get_mut(n) {
            *slot = v;
        }
    }
    fn mdy_val(&self, n: usize) -> i32 {
        self.mdy_val.get(&n).copied().unwrap_or(0)
    }
    fn mnmval(&self, n: usize) -> i32 {
        self.mnmval.get(&n).copied().unwrap_or(0)
    }
    fn mn_integer(&self, n: usize) -> i32 {
        self.mn_integer.get(n).copied().unwrap_or(0)
    }
    fn ms_string(&self, n: usize) -> String {
        self.ms_string.get(&n).cloned().unwrap_or_default()
    }
    fn set_quest_unit_status(&mut self, index: i32, value: i32) {
        self.quest_unit.insert(index, value);
    }
    fn set_quest_flag_status(&mut self, flag: i32, value: i32) {
        let i = flag.max(0) as usize;
        if let Some(slot) = self.quest_flag.get_mut(i) {
            *slot = value as u8;
        }
    }
    fn set_quest_unit_open_status(&mut self, index: i32, value: i32) {
        self.quest_unit_open.insert(index, value);
    }
    fn item_count(&self, name: &str) -> i32 {
        self.items
            .iter()
            .filter(|(n, _)| n == name)
            .map(|(_, c)| *c)
            .sum()
    }
    fn slave_count(&self) -> i32 {
        self.slaves
    }
    fn bag_count(&self) -> i32 {
        i32::try_from(self.items.len()).unwrap_or(i32::MAX)
    }
    fn has_worn(&self, location: &str) -> bool {
        self.worn.get(location).copied().unwrap_or(false)
    }
    fn give_item(&mut self, name: &str, count: i32) {
        self.items.push((name.to_string(), count));
    }
    fn remove_item(&mut self, name: &str, count: i32) -> i32 {
        let mut removed = 0;
        let mut i = 0;
        while i < self.items.len() && removed < count {
            if self.items[i].0 == name {
                let take = self.items[i].1.min(count - removed);
                self.items[i].1 -= take;
                removed += take;
                if self.items[i].1 == 0 {
                    self.items.remove(i);
                    continue;
                }
            }
            i += 1;
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_script::{SayingProcedure, SayingRecord};

    fn script_with(label: &str, say: &str) -> ScriptInfo {
        ScriptInfo {
            is_quest: false,
            quest_info: Default::default(),
            record_list: vec![SayingRecord {
                s_label: label.to_string(),
                procedure_list: vec![SayingProcedure {
                    condition_list: Vec::new(),
                    action_list: Vec::new(),
                    else_action_list: Vec::new(),
                    s_say_msg: say.to_string(),
                    s_else_say_msg: String::new(),
                }],
                bo_ext_jmp: false,
            }],
            quest_count: 0,
        }
    }

    fn host_with(defs: Vec<NpcDef>) -> NpcHost {
        NpcHost {
            envir_root: PathBuf::new(),
            defs,
            failures: Vec::new(),
            goto_count_limit: 40,
        }
    }

    fn def(chr_name: &str, label: &str, say: &str) -> NpcDef {
        NpcDef {
            chr_name: chr_name.to_string(),
            file_path: "MapQuest_def".to_string(),
            script_name: None,
            is_hide: true,
            is_quest: false,
            scripts: vec![script_with(label, say)],
            loaded: true,
        }
    }

    fn no_rng() -> impl FnMut(i32) -> i32 {
        |_v| 0
    }

    /// `GotoLableSendMerChantSayMsg` 的行格式：`ChrName + '/' + sMsg`（`ScriptEngine.cs:991/996`）。
    #[test]
    fn merchant_say_text_and_wire() {
        assert_eq!(merchant_say_text("QManage", "欢迎"), "QManage/欢迎");
        // ASCII 段在 GBK 下逐字节不变；中文按 GBK 双字节编。
        let wire = merchant_say_wire("QManage", "hi");
        assert_eq!(wire, b"QManage/hi");
        let cn = merchant_say_wire("QManage", "欢迎");
        assert_eq!(&cn[..8], b"QManage/");
        assert_eq!(&cn[8..], &[0xBB, 0xB6, 0xD3, 0xAD], "「欢迎」的 GBK 字节");
    }

    /// `@Login` 经 `QManage` 产出说词，前缀 = NPC 名（`PlayObject.cs:1440` + `ScriptEngine.cs:991`）。
    #[test]
    fn run_login_emits_say_with_chrname_prefix() {
        let host = host_with(vec![def("QManage", "@Login", "登录脚本运行成功")]);
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        player.chr_name = "hero".to_string();
        let says = host.run_login(&mut player).expect("QManage 必须可解析");
        assert_eq!(says.len(), 1);
        assert_eq!(says[0].npc_name, "QManage");
        assert_eq!(says[0].raw_msg, "登录脚本运行成功");
        assert_eq!(says[0].text, "QManage/登录脚本运行成功");
        assert!(!says[0].priority, "`@Login` 走 SendMsg（非优先）");
        // 引擎把 LastNpc 指向该 NPC（`ScriptEngine.GotoLable` 的 `SetLastNpc`）。
        assert_eq!(player.last_npc, 0, "隐藏 NPC ActorId 恒 0");
    }

    /// `@StdModeFunc<N>` 经 `QFunction`（`PlayObject.Base.cs:1072`）。
    #[test]
    fn run_std_mode_func_uses_numeric_label() {
        let host = host_with(vec![def("QFunction", "@StdModeFunc7", "功能脚本")]);
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        let says = host
            .run_std_mode_func(7, &mut player)
            .expect("QFunction 必须可解析");
        assert_eq!(says.len(), 1);
        assert_eq!(says[0].text, "QFunction/功能脚本");
        // 未定义的 AniCount ⇒ 该 label 不存在 ⇒ 无说词（oracle 找不到 record 直接 return）。
        let mut rng2 = no_rng();
        let mut p2 = NpcPlayer::empty(&mut rng2);
        assert!(host.run_std_mode_func(9, &mut p2).unwrap().is_empty());
    }

    /// `send_msg` 实参保序（第②片据此进入 `MsgQueue` 并按出队序落帧）。
    #[test]
    fn send_msg_args_are_recorded_in_order() {
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        player.send_msg(44, 1, 2, 3, 4, Some("x"));
        player.send_msg(45, 0, 0, 0, 0, None);
        assert_eq!(player.frames.len(), 2);
        assert_eq!(player.frames[0].ident, 44);
        assert_eq!(player.frames[0].msg.as_deref(), Some("x"));
        assert_eq!(player.frames[1].ident, 45);
    }

    /// 从磁盘装载隐藏 NPC（`NormNpc.LoadNPCScript` 的文件名口径）并运行 `@Login`。
    #[test]
    fn loads_from_disk_and_runs_login() {
        let root = std::env::temp_dir().join(format!("gs-npc-{}", std::process::id()));
        let dir = root.join("Envir").join("MapQuest_def");
        std::fs::create_dir_all(&dir).unwrap();
        // 脚本语法：`[@Login]` 段 + `#say` 行（parser 会把 say 文本收进 s_say_msg）。
        std::fs::write(
            dir.join("QManage.txt"),
            "[@Login]\r\n#say\r\nhello from QManage\r\n",
        )
        .unwrap();
        let record = QuestNpcRecord {
            map_name: "0".to_string(),
            curr_x: 0,
            curr_y: 0,
            chr_name: "QManage".to_string(),
            npc_flag: 0,
            appr: 0,
            file_path: "MapQuest_def".to_string(),
            script_name: None,
            is_hide: true,
            is_quest: false,
        };
        let mut rng = no_rng();
        let mut rename = move |_min: i32, _max: i32| 1;
        let host = NpcHost::load(&root.join("Envir"), &[record], &[], 40, &mut rename);
        assert!(host.failures.is_empty(), "{:?}", host.failures);
        assert_eq!(host.defs.len(), 1);
        assert!(host.defs[0].loaded, "QManage.txt 必须被解析");
        let mut player = NpcPlayer::empty(&mut rng);
        let says = host.run_login(&mut player).expect("QManage 可解析");
        assert!(
            says.iter().any(|s| s.text.contains("hello from QManage")),
            "说词应含脚本文本：{:?}",
            says
        );
        std::fs::remove_dir_all(&root).ok();
    }

    /// 缺脚本文件 ⇒ `loaded=false` 且**不报错**（oracle `File.Exists` 早退），`@Login` 无说词。
    #[test]
    fn missing_script_is_not_an_error() {
        let root = std::env::temp_dir().join(format!("gs-npc-miss-{}", std::process::id()));
        std::fs::create_dir_all(root.join("Envir")).unwrap();
        let record = QuestNpcRecord {
            map_name: "0".to_string(),
            curr_x: 0,
            curr_y: 0,
            chr_name: "QManage".to_string(),
            npc_flag: 0,
            appr: 0,
            file_path: "MapQuest_def".to_string(),
            script_name: None,
            is_hide: true,
            is_quest: false,
        };
        let mut rename = move |_min: i32, _max: i32| 1;
        let host = NpcHost::load(&root.join("Envir"), &[record], &[], 40, &mut rename);
        assert!(host.failures.is_empty());
        assert!(!host.defs[0].loaded);
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        assert!(host.run_login(&mut player).unwrap().is_empty());
        std::fs::remove_dir_all(&root).ok();
    }

    /// **牙齿**：说词行必须带 `ChrName + '/'` 前缀（去掉前缀 ⇒ 本用例必红）。
    #[test]
    fn say_text_must_carry_chrname_prefix() {
        let host = host_with(vec![def("QManage", "@Login", "x")]);
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        let says = host.run_login(&mut player).unwrap();
        assert!(says[0].text.starts_with("QManage/"));
        assert_ne!(says[0].text, says[0].raw_msg, "前缀不能被吞");
    }

    /// **M3⑥-实现 3 第②片**：oracle `MsgQueue` 出队序——`SendPriorityMsg(..., High)` 的入队
    /// 优先级 = 0（`BaseObject.Message.cs:30`），`SendMsg` 的 = `(byte)RM_MERCHANTSAY = 1`
    /// （`:180`）⇒ **High 组先出、SendMsg 组后出**；即使调用序与出队序相反也要分组。
    ///
    /// 红检：把 `dequeue_order` 改成直接返回入队序 ⇒ 本用例必红。
    #[test]
    fn dequeue_order_puts_priority_high_first() {
        let say = |m: &str, priority| SayMsg {
            npc_name: "QManage".to_string(),
            raw_msg: m.to_string(),
            text: merchant_say_text("QManage", m),
            priority,
        };
        let says = vec![
            say("a", false),
            say("b", true),
            say("c", false),
            say("d", true),
        ];
        let order: Vec<&str> = dequeue_order(&says)
            .iter()
            .map(|s| s.raw_msg.as_str())
            .collect();
        assert_eq!(
            order,
            vec!["b", "d", "a", "c"],
            "High(0) 组先、组内入队序，SendMsg(1) 组后"
        );
    }

    /// `has_manage_npc`/`has_function_npc` = oracle `SystemShare.ManageNPC/FunctionNPC != null`
    /// 的宿主等价物：**未装载**（`loaded=false`，脚本文件缺）不算 `!= null`。
    ///
    /// 红检：把 `has_manage_npc` 的 `loaded` 守卫删掉（只看 `find`）⇒ 本用例必红。
    #[test]
    fn manage_and_function_npc_gates_require_loaded() {
        let mut qm = def("QManage", "@Login", "x");
        qm.loaded = false;
        let host = host_with(vec![qm, def("QFunction", "@StdModeFunc7", "y")]);
        assert!(!host.has_manage_npc(), "QManage 未装载 ⇒ ManageNPC == null");
        assert!(
            host.has_function_npc(),
            "QFunction 已装载 ⇒ FunctionNPC != null"
        );
    }
}
