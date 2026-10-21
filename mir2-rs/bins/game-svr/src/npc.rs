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

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use mir2_envir::mapquest::QuestNpcRecord;
use mir2_script::engine::quest_state::QuestBitmap;
use mir2_script::engine::vars::get_val_name_no;
use mir2_script::engine::{Engine, ScriptNpc, ScriptPlayer};
use mir2_script::{LocalFs, ScriptInfo, ScriptParsers};
use mir2_shared::hutil32::{arrest_string_ex_ref, compare_lstr_prefix, ArrestWrite};

/// `GotoLableSendMerChantSayMsg`（`ScriptEngine.cs:991/996`）的说词行：`normNpc.ChrName + '/' + sMsg`。
#[must_use]
pub fn merchant_say_text(chr_name: &str, msg: &str) -> String {
    format!("{chr_name}/{msg}")
}

/// 说词行的 **GBK 原文**（`normNpc.ChrName + '/' + sMsg` 的字节编码）。
///
/// **F-2 订正**（reviewer-5 `b164fa48`）：这**不是** oracle 线上字节。线上尾 =
/// `SendSocket(ClientMsg, EDCode.EncodeString(processMsg.Msg))`（`PlayObject.Message.cs:1724`）
/// ⇒ `edcode::encode(self)`（6-bit 变换，`EncryptUtil.cs:108-140`）；本仓权威移植 =
/// `crates/protocol::edcode::encode`。本函数只产**未编码的 GBK 原文**，编码在
/// [`crate::smframe::sm_merchantsay`] 里做（与 `sm_hear` 同结构）。
#[must_use]
pub fn merchant_say_gbk(chr_name: &str, msg: &str) -> Vec<u8> {
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
    /// 说这句话的 NPC 的 `ActorId`（`baseObject.ActorId`，`BaseObject.Message.cs:154`）。
    /// 非 priority 支的 `SM_MERCHANTSAY.Recog` 用它（G-4：旧实现恒 0）。
    pub npc_actor_id: i32,
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
    /// `BaseObject.ActorId`（`ActorEntity()` 构造时的 `ActorMgr.GetNextIdentity()`）。
    ///
    /// 本宿主按装载序赋稳定正整数（1..），**不复刻** oracle 的全局 ActorMgr 流水号：
    /// 那需要整个世界（怪物/商人/玩家）的构造序；客户端只用它关联说话 NPC（`g_nCurMerchant`），
    /// 只要非 0 且稳定即可（G-4；精确数值登记为口径差）。
    pub actor_id: i32,
}

/// `NormNpc.Castle` 的只读快照（`$OWNERGUILD/$LORD/$CASTLENAME/...` 展开用）。
///
/// 城堡家族（⑧）尚未在宿主落地 ⇒ 生产恒 `None`（= oracle `Castle == null` 的 `"????"` 支）；
/// 本结构只为把 oracle 的**非 null 分支**也能被构造级用例钉住。
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct CastleVars {
    /// `Castle.sName`（`$CASTLENAME`）。
    pub name: String,
    /// `Castle.OwnGuild`（`$OWNERGUILD`；oracle 空串 ⇒ `"游戏管理"`）。
    pub own_guild: String,
    /// `Castle.MasterGuild.GetChiefName()`（`$LORD`；`None` = MasterGuild 为 null ⇒ `"管理员"`）。
    pub lord: Option<String>,
}

/// `NormNpc.GetVariableText` / `Merchant.GetVariableText` 需要的**非玩家**取值。
///
/// oracle 的变量表一半来自 `SystemShare.Config`（服务器名、货币名、升级费…），一半来自
/// NPC 自身（`PriceRate`、`Castle`）；本结构把这些在说词展开前一次性喂给 [`NpcPlayer`]。
/// 缺省值 = oracle `GameSvrConf` ctor（`GameSvrConf.cs:1233/1320/1321/1420`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NpcVars {
    /// `SystemShare.Config.ServerName`（缺省 `SKY引擎`）。
    pub server_name: String,
    /// `SystemShare.Config.GameGoldName`（缺省 `元宝`）。
    pub game_gold_name: String,
    /// `SystemShare.Config.GamePointName`（缺省 `游戏点`）。
    pub game_point_name: String,
    /// `SystemShare.Config.UpgradeWeaponPrice`（缺省 `10000`；`$UPGRADEWEAPONFEE`）。
    pub upgrade_weapon_price: i32,
    /// `Merchant.PriceRate`（`$PRICERATE`）。
    pub price_rate: i32,
    /// `NormNpc.Castle`（`$OWNERGUILD/$LORD/$CASTLENAME/...`）。
    pub castle: Option<CastleVars>,
    /// `UseItems[Weapon]` 的展示名（`$USERWEAPON`；`None`/空 = `无`）。
    pub user_weapon: Option<String>,
    /// `PlayObject.MyGuild?.GuildName`（`$GUILDNAME`）。
    pub guild_name: Option<String>,
    /// `SystemShare.Config.GlobalVal`（`int[500]`）。
    pub global_val: Vec<i32>,
    /// `SystemShare.Config.GlobaDyMval`（`int[500]`）。
    pub globa_dy_mval: Vec<i32>,
    /// `SystemShare.Config.GlobalAVal`（`string[500]`）。
    pub global_a_val: Vec<String>,
    /// `M2Share.High*Human` 的 `GetMyInfo()`；`None` = 无人 ⇒ `????`（`NormNpc.cs:362-435`）。
    pub high_level_info: Option<String>,
    pub high_pk_info: Option<String>,
    pub high_dc_info: Option<String>,
    pub high_mc_info: Option<String>,
    pub high_sc_info: Option<String>,
    pub high_online_info: Option<String>,
}

impl Default for NpcVars {
    fn default() -> Self {
        Self {
            server_name: "SKY引擎".to_string(),
            game_gold_name: "元宝".to_string(),
            game_point_name: "游戏点".to_string(),
            upgrade_weapon_price: 10_000,
            price_rate: 100,
            castle: None,
            user_weapon: None,
            guild_name: None,
            global_val: vec![0; 500],
            globa_dy_mval: vec![0; 500],
            global_a_val: vec![String::new(); 500],
            high_level_info: None,
            high_pk_info: None,
            high_dc_info: None,
            high_mc_info: None,
            high_sc_info: None,
            high_online_info: None,
        }
    }
}

/// 一个**商人**的运行时脚本条目（`Merchant.ScriptList` + 变量展开所需的 NPC 侧字段）。
#[derive(Debug)]
pub struct MerchantNpc {
    /// `Merchant.ScriptList` 与身份（`file_path` = `Market_Def`，`actor_id` = 世界实体 id）。
    pub def: NpcDef,
    /// `Merchant.PriceRate`（构造默认 100；脚本 `%` 行覆盖）。
    pub price_rate: i32,
    /// `NormNpc.Castle`（`Initialize` 的 `InCastleWarArea`；本宿主未建模 ⇒ `None`）。
    pub castle: Option<CastleVars>,
    pub map_name: String,
    pub curr_x: i32,
    pub curr_y: i32,
}

/// 商人脚本表（`ActorId → Merchant.ScriptList`）。
///
/// oracle 用 `SystemShare.ActorMgr.Get<INormNpc>(actorId)` 找脚本；本宿主以世界实体 id
/// （= `ActorId`）为键。`ScriptInfo` 不 Clone ⇒ 表只在装载时建一次，宿主按引用使用
/// （**禁止**每次 `GotoLable` 重新 `load_script_file`，见 `crates/envir::LoadedMerchant`）。
#[derive(Debug, Default)]
pub struct MerchantScriptBook {
    /// `SystemShare.Config.ScriptGotoCountLimit`。
    pub goto_count_limit: i32,
    by_actor: BTreeMap<i32, MerchantNpc>,
}

impl MerchantScriptBook {
    #[must_use]
    pub fn new(goto_count_limit: i32) -> Self {
        Self {
            goto_count_limit,
            by_actor: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, entry: MerchantNpc) {
        self.by_actor.insert(entry.def.actor_id, entry);
    }

    #[must_use]
    pub fn get(&self, actor_id: i32) -> Option<&MerchantNpc> {
        self.by_actor.get(&actor_id)
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.by_actor.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_actor.is_empty()
    }

    /// `ScriptEngine.GotoLable(player, actorId, label, boExtJmp)`（`ScriptEngine.cs:38-129`）
    /// 的商人侧入口：按实体 id 取 `ScriptList`，跑引擎并收说词。
    ///
    /// `None` = `ActorMgr.Get(actorId)` 为 null（未装载脚本/非商人）——oracle 的
    /// `if (normNpc == null) return;`（`ScriptEngine.cs:131-136`）。
    pub fn goto_label(
        &self,
        actor_id: i32,
        player: &mut NpcPlayer<'_>,
        label: &str,
        bo_ext_jmp: bool,
    ) -> Option<Vec<SayMsg>> {
        let entry = self.get(actor_id)?;
        // `Merchant.GetVariableText` 覆写在基类之后读 `PriceRate`；`NormNpc` 的 `Castle` 支读
        // `Initialize` 落下的城堡快照。两者都由本表在入口注入。
        player.vars.price_rate = entry.price_rate;
        player.vars.castle = entry.castle.clone();
        let (map_name, curr_x, curr_y) = (entry.map_name.clone(), entry.curr_x, entry.curr_y);
        Some(run_goto_label(
            &entry.def,
            actor_id,
            &map_name,
            curr_x,
            curr_y,
            self.goto_count_limit,
            player,
            label,
            bo_ext_jmp,
        ))
    }
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
        for (idx, record) in quest_npcs.iter().chain(merchants.iter()).enumerate() {
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
                // 稳定非 0（见字段文档；oracle 的精确流水号不复刻，登记）。
                actor_id: i32::try_from(idx + 1).unwrap_or(i32::MAX),
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
        // 隐藏 NPC 不经地图：`ActorId/Envir/CurrX/CurrY` 均为 0（`BaseObject` 默认）。
        Some(run_goto_label(
            def,
            0,
            "0",
            0,
            0,
            self.goto_count_limit,
            player,
            label,
            bo_ext_jmp,
        ))
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

/// `ScriptNpc` 适配：借 [`NpcDef`] 的脚本表 + 调用方给的实体身份，收集说词。
struct HostNpc<'a> {
    def: &'a NpcDef,
    says: Vec<SayMsg>,
    goto_count_limit: i32,
    /// `BaseObject.ActorId`（隐藏 NPC 0；商人 = 世界实体 id）。
    actor_id: i32,
    map_name: String,
    curr_x: i32,
    curr_y: i32,
}

/// 跑一次 `ScriptEngine.GotoLable` 并收说词（[`NpcHost`] 与 [`MerchantScriptBook`] 共用）。
#[allow(clippy::too_many_arguments)]
fn run_goto_label(
    def: &NpcDef,
    actor_id: i32,
    map_name: &str,
    curr_x: i32,
    curr_y: i32,
    goto_count_limit: i32,
    player: &mut NpcPlayer<'_>,
    label: &str,
    bo_ext_jmp: bool,
) -> Vec<SayMsg> {
    let mut npc = HostNpc {
        def,
        says: Vec::new(),
        goto_count_limit,
        actor_id,
        map_name: map_name.to_string(),
        curr_x,
        curr_y,
    };
    {
        let mut engine = Engine::new(&mut npc, player);
        engine.goto_label(label, bo_ext_jmp);
    }
    npc.says
}

impl ScriptNpc for HostNpc<'_> {
    fn actor_id(&self) -> i32 {
        self.actor_id
    }
    fn chr_name(&self) -> String {
        self.def.chr_name.clone()
    }
    fn map_name(&self) -> String {
        self.map_name.clone()
    }
    fn curr_x(&self) -> i32 {
        self.curr_x
    }
    fn curr_y(&self) -> i32 {
        self.curr_y
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
            npc_actor_id: self.def.actor_id,
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
    /// `playerActor.GameGold`（`$GAMEGOLD`；`OPENYBDEAL` 等动作的读源）。
    pub game_gold: i32,
    /// `playerActor.GamePoint`（`$GAMEPOINT`）。
    pub game_point: i32,
    /// `playerActor.CreditPoint`（`$CREDITPOINT`）。
    pub credit_point: i32,
    /// `playerActor.ReLevel`（`$RELEVEL`）。
    pub re_level: i32,
    /// 说词变量展开的非玩家取值（`NormNpc.GetVariableText` 的 `SystemShare.Config`/NPC 侧）。
    pub vars: NpcVars,
    /// `PlayObject.LastNpc`
    pub last_npc: i32,
    /// `playerActor.Script`（脚本下标）
    pub script_index: Option<usize>,
    /// `playerActor.ScriptGotoCount`
    pub script_goto_count: i32,
    /// `PlayObject.QuestFlag`（`PlayObject.cs:1040` 的 `QuestFlag = new byte[128]`）。
    ///
    /// **F-1 订正**：oracle 三组任务态是 `byte[128]` + **位运算**（`128 >> n`，MSB-first；`nFlag -= 1`），
    /// 不是按字节下标直存。本仓库的权威移植 = `crates/script::engine::quest_state::QuestBitmap`
    /// （`quest_state.rs`，含 `tests/parity/m3/quest-state-probe` 真身向量）——三类数组**直接复用它**。
    pub quest_flag: QuestBitmap,
    /// `playerActor.QuestUnit`（同 `QuestFlag` 位语义）。
    pub quest_unit: QuestBitmap,
    /// `playerActor.QuestUnitOpen`（同 `QuestFlag` 位语义）。
    pub quest_unit_open: QuestBitmap,
    /// `playerActor.ItemList`（名 → 件数）
    pub items: Vec<(String, i32)>,
    /// `playerActor.UseItems`（位置名 → 是否佩戴）
    pub worn: HashMap<String, bool>,
    /// `playerActor.SlaveList.Count`
    pub slaves: i32,
    /// `playerActor.GroupOwner`（0 = 无组）。§1.O `recall-ext` 读侧，由宿主快照填入。
    pub group_owner: i32,
    /// `playerActor.GroupMembers[i].Job`（`PlayerJob` 值：0/1/2）。
    pub group_member_jobs: Vec<i32>,
    pub mn_val: [i32; 100],
    pub mn_integer: [i32; 100],
    pub mdy_val: HashMap<usize, i32>,
    pub mnmval: HashMap<usize, i32>,
    pub ms_string: HashMap<usize, String>,
    /// `PlayObject.ServerStrVal[string[100]]`（`$STR` 1600..1699）。
    pub server_str_val: Vec<String>,
    /// `PlayObject.ServerIntVal[int[100]]`（`$STR` 1700..1799）。
    pub server_int_val: Vec<i32>,
    /// 引擎调用 `SendMsg` 的实参序列（保序）。
    pub frames: Vec<SendMsg>,
    /// `RandomNumber.Random` —— 必须**世界同一条流**（R-4/Claim B 的输入）。
    pub rng: &'r mut dyn FnMut(i32) -> i32,
    /// `PlayObject.KillMonExpRate`（`PlayObject.cs:720`；ctor 100）——P27 / 脚本 197 的写回源。
    pub kill_mon_exp_rate: i32,
    /// `PlayObject.KillMonExpRateTime`（`PlayObject.cs:725`；ctor 0）。
    pub kill_mon_exp_rate_time: i32,
    /// `PlayObject.PowerRate`（`PlayObject.cs:724`；ctor 100）——P27 / 脚本 198。
    pub power_rate: i32,
    /// `PlayObject.PowerRateTime`（`PlayObject.cs:726`；ctor 0）。
    pub power_rate_time: i32,
    /// `PlayObject.AutoGetExpTime`（`PlayObject.cs:805`；ctor 0；脚本 `SETAUTOGETEXP`
    /// 写 `nTime * 1000`）。**P31**：宿主 `SessionGround` 是唯一真源，脚本跑前 seed、跑后写回。
    pub auto_get_exp_time: i32,
    /// `PlayObject.AutoGetExpPoint`（`PlayObject.cs:806`；ctor 0）。
    pub auto_get_exp_point: i32,
    /// `PlayObject.AutoGetExpEnvir`（`PlayObject.cs:807`；ctor `null`）。C# 存的是
    /// `IEnvirnoment` 引用，Rust 这里存地图名（宿主再解析成 `map_id` 做同一性比较）。
    pub auto_get_exp_envir: Option<String>,
    /// `PlayObject.AutoGetExpInSafeZone`（`PlayObject.cs:808`；ctor false）。
    pub auto_get_exp_in_safe_zone: bool,
    /// `PlayObject.BoIncGameGold`（`PlayObject.cs:772`；ctor false）：脚本 192
    /// `AUTOADDGAMEGOLD` 的 STOP 写点（START 在当前 oracle 源码下不可达，见 `actions.rs`）。
    /// 宿主 `SessionGround.p22` 是唯一真源：脚本跑前 seed、跑后写回。
    pub bo_inc_game_gold: bool,
    /// `PlayObject.BoDecGameGold`（`PlayObject.cs:767`；ctor false）：脚本 193 的 STOP 写点。
    pub bo_dec_game_gold: bool,
    /// `PlayObject.SaleDeal`（`PlayObject.cs:856`，**未初始化自动属性 ⇒ 默认 false**；
    /// 全仓唯一赋值是 `ExecutionProcessingSys.cs:2346` 的 `= true`）——是否已开通元宝寄售。
    ///
    /// 读点：`ActionOfOpenSaleDeal` 前置（`ExecutionProcessingSys.cs:2334`）、
    /// `UserSelectOpenDealOffForm`（`Merchant.cs:1040`）、`GoldDealService.cs:18`。
    /// 唯一写点：`ActionOfOpenSaleDeal` 成功路径 `:2346`（**失败/已开通路径不写**）。
    /// 生产侧唯一真源 = 宿主 [`game_svr::selloff::SellOffState::sale_deal`]：脚本跑前 seed、
    /// 跑后写回（与 oracle 的同一个 `PlayObject` 字段同义）。
    pub sale_deal: bool,
    /// `PlayObject.PlayDiceLabel`（`PlayObject.cs:588`，**未初始化自动属性 ⇒ null**）。
    ///
    /// 写点：`ActionOfPlayDice`（`ExecutionProcessingSys.cs:2032`）写 `sParam2`；
    /// 读点/清点：`LableIsCanJmp` 第三支（`PlayObject.Base.cs:2824-2828`，命中即
    /// `PlayDiceLabel = string.Empty`，一次性）。`None` = C# `null`（永不匹配任何非 null 标签）。
    /// 生产侧跨调用保持于宿主 `ShopSession::play_dice_label`。
    pub play_dice_label: Option<String>,
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
            game_gold: 0,
            game_point: 0,
            credit_point: 0,
            re_level: 0,
            vars: NpcVars::default(),
            last_npc: 0,
            script_index: None,
            script_goto_count: 0,
            quest_flag: QuestBitmap::new(),
            quest_unit: QuestBitmap::new(),
            quest_unit_open: QuestBitmap::new(),
            items: Vec::new(),
            worn: HashMap::new(),
            slaves: 0,
            group_owner: 0,
            group_member_jobs: Vec::new(),
            mn_val: [0; 100],
            mn_integer: [0; 100],
            mdy_val: HashMap::new(),
            mnmval: HashMap::new(),
            ms_string: HashMap::new(),
            server_str_val: vec![String::new(); 100],
            server_int_val: vec![0; 100],
            frames: Vec::new(),
            rng,
            // `PlayObject.cs:941/943` 的 ctor 初值 + 两个 `Time` 的零初值。
            kill_mon_exp_rate: 100,
            kill_mon_exp_rate_time: 0,
            power_rate: 100,
            power_rate_time: 0,
            // P31：`PlayObject.cs:1009-1012` 的 ctor 初值（Tick 不在本适配器里建模，
            // 宿主 `SessionGround` 持有；这里只有脚本可写的四个字段）。
            auto_get_exp_time: 0,
            auto_get_exp_point: 0,
            auto_get_exp_envir: None,
            auto_get_exp_in_safe_zone: false,
            // `PlayObject.cs:965/970` 的 ctor 初值。
            bo_inc_game_gold: false,
            bo_dec_game_gold: false,
            // C# 未初始化自动属性的默认值：SaleDeal=false（`PlayObject.cs:856`）、
            // PlayDiceLabel=null（`:588`）——ctor 里没有对两者赋值。
            sale_deal: false,
            play_dice_label: None,
        }
    }
}

/// `NormNpc.ReplaceVariableText`（`NormNpc.cs:1618-1633`）：`IndexOf(sStr, OrdinalIgnoreCase)`
/// 命中**第一处**后用 `sText` 替换（只替换一次）。
fn replace_variable_text(msg: &str, token: &str, text: &str) -> String {
    if token.is_empty() || token.len() > msg.len() {
        return msg.to_string();
    }
    let Some(pos) = msg
        .as_bytes()
        .windows(token.len())
        .position(|w| w.eq_ignore_ascii_case(token.as_bytes()))
    else {
        return msg.to_string();
    };
    let mut out = String::with_capacity(msg.len() + text.len());
    out.push_str(&msg[..pos]);
    out.push_str(text);
    out.push_str(&msg[pos + token.len()..]);
    out
}

impl NpcPlayer<'_> {
    /// `NormNpc.GetVariableText` + `Merchant.GetVariableText` 的 1:1 移植（说词侧）。
    ///
    /// 口径（逐条对应 oracle）：
    /// - `switch (sVariable)` 是 **C# 字符串 switch ⇒ 大小写敏感**（`$username` 不命中）；
    /// - 替换用 `ReplaceVariableText` ⇒ **只替换第一处**、且 token 查找 `OrdinalIgnoreCase`；
    /// - `$GUILDNAME` 无行会时 oracle **把整条说词置为 `"无"`**（不是替换 token）；
    /// - `Castle == null` 时 `$OWNERGUILD/$LORD/$CASTLENAME/...` 展开为 `"????"`
    ///   （`NormNpc.cs:749-894`）；
    /// - `$STR(...)` 按 `GetValNameNo` 的 12 段槽位映射（说词侧：800..1199 偏移 **-700**）；
    /// - oracle 没有 `$GAMEGIRD`/`$GAMEDIAMOND` 分支 ⇒ **原样保留**（部署件 23/20 次，照搬）。
    pub fn get_variable_text(&mut self, s_variable: &str, s_msg: &mut String) {
        let token = |name: &str| format!("<{name}>");
        match s_variable {
            "$SERVERNAME" => {
                *s_msg =
                    replace_variable_text(s_msg, &token("$SERVERNAME"), &self.vars.server_name);
            }
            "$GAMEGOLDNAME" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$GAMEGOLDNAME"),
                    &self.vars.game_gold_name,
                );
            }
            "$GAMEPOINTNAME" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$GAMEPOINTNAME"),
                    &self.vars.game_point_name,
                );
            }
            "$USERNAME" => {
                *s_msg = replace_variable_text(s_msg, &token("$USERNAME"), &self.chr_name);
            }
            "$GAMEGOLD" => {
                *s_msg =
                    replace_variable_text(s_msg, &token("$GAMEGOLD"), &self.game_gold.to_string());
            }
            "$GAMEPOINT" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$GAMEPOINT"),
                    &self.game_point.to_string(),
                );
            }
            "$CREDITPOINT" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$CREDITPOINT"),
                    &self.credit_point.to_string(),
                );
            }
            "$RELEVEL" => {
                *s_msg =
                    replace_variable_text(s_msg, &token("$RELEVEL"), &self.re_level.to_string());
            }
            "$LEVEL" => {
                *s_msg = replace_variable_text(s_msg, &token("$LEVEL"), &self.level.to_string());
            }
            "$GUILDNAME" => {
                // `NormNpc.cs:486-497`：**无行会时整条说词 = "无"**（怪异点，照搬）。
                if let Some(g) = &self.vars.guild_name {
                    *s_msg = replace_variable_text(s_msg, &token("$GUILDNAME"), g);
                } else {
                    *s_msg = "无".to_string();
                }
            }
            "$OWNERGUILD" => {
                let text = match &self.vars.castle {
                    Some(c) if !c.own_guild.is_empty() => c.own_guild.clone(),
                    Some(_) => "游戏管理".to_string(),
                    None => "????".to_string(),
                };
                *s_msg = replace_variable_text(s_msg, &token("$OWNERGUILD"), &text);
            }
            "$LORD" => {
                let text = match &self.vars.castle {
                    Some(c) => c.lord.clone().unwrap_or_else(|| "管理员".to_string()),
                    None => "????".to_string(),
                };
                *s_msg = replace_variable_text(s_msg, &token("$LORD"), &text);
            }
            "$CASTLENAME" => {
                let text = match &self.vars.castle {
                    Some(c) => c.name.clone(),
                    None => "????".to_string(),
                };
                *s_msg = replace_variable_text(s_msg, &token("$CASTLENAME"), &text);
            }
            "$USERWEAPON" => {
                let text = self
                    .vars
                    .user_weapon
                    .as_deref()
                    .filter(|s| !s.is_empty())
                    .unwrap_or("无")
                    .to_string();
                *s_msg = replace_variable_text(s_msg, &token("$USERWEAPON"), &text);
            }
            "$PRICERATE" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$PRICERATE"),
                    &self.vars.price_rate.to_string(),
                );
            }
            "$UPGRADEWEAPONFEE" => {
                *s_msg = replace_variable_text(
                    s_msg,
                    &token("$UPGRADEWEAPONFEE"),
                    &self.vars.upgrade_weapon_price.to_string(),
                );
            }
            "$HIGHLEVELINFO" => {
                let t = self
                    .vars
                    .high_level_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHLEVELINFO"), &t);
            }
            "$HIGHPKINFO" => {
                let t = self
                    .vars
                    .high_pk_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHPKINFO"), &t);
            }
            "$HIGHDCINFO" => {
                let t = self
                    .vars
                    .high_dc_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHDCINFO"), &t);
            }
            "$HIGHMCINFO" => {
                let t = self
                    .vars
                    .high_mc_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHMCINFO"), &t);
            }
            "$HIGHSCINFO" => {
                let t = self
                    .vars
                    .high_sc_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHSCINFO"), &t);
            }
            "$HIGHONLINEINFO" => {
                let t = self
                    .vars
                    .high_online_info
                    .clone()
                    .unwrap_or_else(|| "????".into());
                *s_msg = replace_variable_text(s_msg, &token("$HIGHONLINEINFO"), &t);
            }
            _ => {
                // `$STR(...)`（`NormNpc.cs:1532-1587`）。`CompareLStr` 只看前 5 字符。
                if compare_lstr_prefix(s_variable, "$STR(", 5) {
                    let (ArrestWrite::Value(inner), _) = arrest_string_ex_ref(s_variable, '(', ')')
                    else {
                        return;
                    };
                    let n = get_val_name_no(&inner);
                    let text = match n {
                        0..=99 => self.mn_val(n as usize).to_string(),
                        100..=199 => self
                            .vars
                            .global_val
                            .get((n - 100) as usize)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        200..=299 => self.mdy_val((n - 200) as usize).to_string(),
                        300..=399 => self.mnmval((n - 300) as usize).to_string(),
                        400..=499 => self
                            .vars
                            .globa_dy_mval
                            .get((n - 400) as usize)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        500..=599 => self.mn_integer((n - 500) as usize).to_string(),
                        600..=699 => self.ms_string((n - 600) as usize),
                        700..=799 => self
                            .vars
                            .global_a_val
                            .get((n - 700) as usize)
                            .cloned()
                            .unwrap_or_default(),
                        // 说词侧偏移是 **-700**（不是 -800；`NormNpc.cs:1565-1568`）。
                        800..=1199 => self
                            .vars
                            .global_val
                            .get((n - 700) as usize)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        1200..=1599 => self
                            .vars
                            .global_a_val
                            .get((n - 1100) as usize)
                            .cloned()
                            .unwrap_or_default(),
                        1600..=1699 => self
                            .server_str_val
                            .get((n - 1600) as usize)
                            .cloned()
                            .unwrap_or_default(),
                        1700..=1799 => self
                            .server_int_val
                            .get((n - 1700) as usize)
                            .copied()
                            .unwrap_or(0)
                            .to_string(),
                        // 槽位号 `< 0`（或越界）⇒ **不替换**（原样保留，`NormNpc.cs:1534`）。
                        _ => return,
                    };
                    *s_msg = replace_variable_text(s_msg, &format!("<{s_variable}>"), &text);
                }
            }
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
        // `GetQuestFalgStatus`（`PlayObject.cs:3966`）：`nFlag -= 1` + `128 >> n` 位提取。
        u8::try_from(self.quest_flag.get(i32::from(flag))).unwrap_or(0)
    }
    fn quest_unit_status(&self, flag: i16) -> u8 {
        // `GetQuestUnitStatus`（`PlayObject.cs:4060`），与 QuestFlag 同形。
        u8::try_from(self.quest_unit.get(i32::from(flag))).unwrap_or(0)
    }
    fn quest_unit_open_status(&self, flag: i16) -> u8 {
        // `GetQuestUnitOpenStatus`（`PlayObject.cs:4013`），与 QuestFlag 同形。
        u8::try_from(self.quest_unit_open.get(i32::from(flag))).unwrap_or(0)
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
    fn chr_name(&self) -> String {
        self.chr_name.clone()
    }
    /// `playerActor.GameGold = v`（`BaseObject.GameGold`）：唯一生产写点
    /// `ActionOfOpenSaleDeal` 成功扣费（`ExecutionProcessingSys.cs:2345`）；`GAMEGOLD`
    /// 动作（`ActionOfGameGold`，`:3058-3083`，写点 `:3073/3078/3082`）同源。
    /// 写回 `SessionGround.game_gold`（存档 `build_save_record` 已落 `rec.data.game_gold`）。
    fn set_game_gold(&mut self, v: i32) {
        self.game_gold = v;
    }
    fn game_gold(&self) -> i32 {
        self.game_gold
    }
    /// `playerActor.SaleDeal`（`PlayObject.cs:856`，未初始化自动属性 ⇒ false）。
    fn sale_deal(&self) -> bool {
        self.sale_deal
    }
    /// `playerActor.SaleDeal = v`（唯一写点 `ExecutionProcessingSys.cs:2346`：OPENYBDEAL 成功）。
    fn set_sale_deal(&mut self, v: bool) {
        self.sale_deal = v;
    }
    /// `SystemShare.Config.GameGoldName`（`ServerConf.cs:108` 读 `[Names]/GameGold`；缺省
    /// "元宝" `GameSvrConf.cs:1320`）：宿主经 [`NpcVars::game_gold_name`] 注入部署值。
    fn config_game_gold_name(&self) -> String {
        self.vars.game_gold_name.clone()
    }
    fn game_point(&self) -> i32 {
        self.game_point
    }
    fn credit_point(&self) -> i32 {
        self.credit_point
    }
    fn re_level(&self) -> i32 {
        self.re_level
    }
    fn guild_name(&self) -> Option<String> {
        self.vars.guild_name.clone()
    }
    /// `playerActor.PlayDiceLabel = v`（`ActionOfPlayDice`，`ExecutionProcessingSys.cs:2032`；
    /// `sParam2` 是客户端随后回跳的标签）。
    fn set_play_dice_label(&mut self, label: &str) {
        self.play_dice_label = Some(label.to_string());
    }
    /// `playerActor.PlayDiceLabel`（`LableIsCanJmp` 第三支取数口，`PlayObject.Base.cs:2824`）。
    fn play_dice_label(&self) -> Option<String> {
        self.play_dice_label.clone()
    }
    /// `NormNpc.GetLineVariableText`（`NormNpc.cs:245-273`）：
    /// 循环条件 `sMsg.IndexOf('>') >= 1`（**门：没有 `>` 或 `>` 在位置 0 ⇒ 立即中止**）；
    /// `ArrestStringEx(sMsg,"<",">",ref sVariable)` 取 `<...>`；仅 `sVariable[0]=='$'` 才展开；
    /// **最多 101 次**（`nCount >= 101` ⇒ break）。`ArrestStringEx` 失败时 C# 不覆盖
    /// `sVariable`（保留上一次的值）——本实现照搬（`ArrestWrite::Unchanged`）。
    fn line_variable_text(&mut self, msg: &str) -> String {
        let mut s = msg.to_string();
        let mut variable = String::new();
        let mut count = 0i32;
        loop {
            if s.find('>').is_none_or(|p| p < 1) {
                break;
            }
            match mir2_shared::hutil32::arrest_string_ex_ref(&s, '<', '>') {
                (mir2_shared::hutil32::ArrestWrite::Value(v), _) => variable = v,
                (mir2_shared::hutil32::ArrestWrite::Unchanged, _) => {}
            }
            if !variable.is_empty() && variable.starts_with('$') {
                self.get_variable_text(&variable.clone(), &mut s);
            }
            count += 1;
            if count >= 101 {
                break;
            }
        }
        s
    }
    fn group_owner(&self) -> i32 {
        self.group_owner
    }
    fn group_member_jobs(&self) -> Vec<i32> {
        self.group_member_jobs.clone()
    }
    fn level(&self) -> i32 {
        self.level
    }
    fn random(&mut self, value: i32) -> i32 {
        (self.rng)(value)
    }
    fn set_kill_mon_exp_rate(&mut self, rate: i32, time: i32) {
        self.kill_mon_exp_rate = rate;
        self.kill_mon_exp_rate_time = time;
    }
    fn set_power_rate(&mut self, rate: i32, time: i32) {
        self.power_rate = rate;
        self.power_rate_time = time;
    }
    fn set_bo_inc_game_gold(&mut self, v: bool) {
        self.bo_inc_game_gold = v;
    }
    fn set_bo_dec_game_gold(&mut self, v: bool) {
        self.bo_dec_game_gold = v;
    }
    fn set_auto_get_exp(
        &mut self,
        safe_zone: bool,
        envir_map: Option<&str>,
        time_ms: i32,
        point: i32,
    ) {
        // `ExecutionProcessingSys.cs:2707-2710`：四字段按 oracle 次序写入
        //（InSafeZone → Envir → Time → Point）。`envir_map = None` 对应 oracle 的
        // `AutoGetExpEnvir = null`（`FindMap` 返回 null 或 `sMap` 非空 —— 后者是
        // `:2698` 的条件写反导致的既有缺陷，照搬）。
        self.auto_get_exp_in_safe_zone = safe_zone;
        self.auto_get_exp_envir = envir_map.map(str::to_string);
        self.auto_get_exp_time = time_ms;
        self.auto_get_exp_point = point;
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
    fn global_val(&self, n: usize) -> i32 {
        self.vars.global_val.get(n).copied().unwrap_or(0)
    }
    fn globa_dy_mval(&self, n: usize) -> i32 {
        self.vars.globa_dy_mval.get(n).copied().unwrap_or(0)
    }
    fn global_a_val(&self, n: usize) -> String {
        self.vars.global_a_val.get(n).cloned().unwrap_or_default()
    }
    fn server_int_val(&self, n: usize) -> i32 {
        self.server_int_val.get(n).copied().unwrap_or(0)
    }
    fn server_str_val(&self, n: usize) -> String {
        self.server_str_val.get(n).cloned().unwrap_or_default()
    }
    // ---- `$STR` 槽位写侧（`NormNpc.cs:1532-1587` 的读侧对面）----
    //
    // 复验 Important 2：这些 setter 是脚本 `MOV/INC/DEC/SET` 的唯一落点
    // （`crates/script/src/engine/{actions,mod}.rs`），默认实现为空 ⇒ 200..1799 槽
    // 在生产路径恒 0/""，说词 `<$STR(...)>` 全部展开成 0/空。这里按槽位写进与
    // [`Self::get_variable_text`] 读取的**同一存储**（全局槽在 `vars`，玩家槽在自身字段）。
    fn set_mdy_val(&mut self, n: usize, v: i32) {
        self.mdy_val.insert(n, v);
    }
    fn set_mnmval(&mut self, n: usize, v: i32) {
        self.mnmval.insert(n, v);
    }
    fn set_mn_integer(&mut self, n: usize, v: i32) {
        if let Some(slot) = self.mn_integer.get_mut(n) {
            *slot = v;
        }
    }
    fn set_ms_string(&mut self, n: usize, v: String) {
        self.ms_string.insert(n, v);
    }
    fn set_global_val(&mut self, n: usize, v: i32) {
        if let Some(slot) = self.vars.global_val.get_mut(n) {
            *slot = v;
        }
    }
    fn set_globa_dy_mval(&mut self, n: usize, v: i32) {
        if let Some(slot) = self.vars.globa_dy_mval.get_mut(n) {
            *slot = v;
        }
    }
    fn set_global_a_val(&mut self, n: usize, v: String) {
        if let Some(slot) = self.vars.global_a_val.get_mut(n) {
            *slot = v;
        }
    }
    fn set_server_int_val(&mut self, n: usize, v: i32) {
        if let Some(slot) = self.server_int_val.get_mut(n) {
            *slot = v;
        }
    }
    fn set_server_str_val(&mut self, n: usize, v: String) {
        if let Some(slot) = self.server_str_val.get_mut(n) {
            *slot = v;
        }
    }
    fn set_quest_unit_status(&mut self, index: i32, value: i32) {
        // `SetQuestUnitStatus`（`PlayObject.cs:4084`）：位语义在实现侧。
        self.quest_unit.set(index, value);
    }
    fn set_quest_flag_status(&mut self, flag: i32, value: i32) {
        // `SetQuestFlagStatus`（`PlayObject.cs:3990`）：`nFlag -= 1` + `128 >> n` 位写入。
        self.quest_flag.set(flag, value);
    }
    fn set_quest_unit_open_status(&mut self, index: i32, value: i32) {
        // `SetQuestUnitOpenStatus`（`PlayObject.cs:4037`）：位语义在实现侧。
        self.quest_unit_open.set(index, value);
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
            actor_id: 1,
        }
    }

    fn no_rng() -> impl FnMut(i32) -> i32 {
        |_v| 0
    }

    /// `GotoLableSendMerChantSayMsg` 的行格式：`ChrName + '/' + sMsg`（`ScriptEngine.cs:991/996`）。
    ///
    /// **F-2 订正**：本用例只锁**GBK 原文**；线上尾是它的 EDCode 编码
    /// （`edcode::encode`），由 `smframe::tests::sm_merchantsay_header_and_tail` 锁。
    #[test]
    fn merchant_say_text_and_gbk_bytes() {
        assert_eq!(merchant_say_text("QManage", "欢迎"), "QManage/欢迎");
        // ASCII 段在 GBK 下逐字节不变；中文按 GBK 双字节编。
        let gbk = merchant_say_gbk("QManage", "hi");
        assert_eq!(gbk, b"QManage/hi");
        let cn = merchant_say_gbk("QManage", "欢迎");
        assert_eq!(&cn[..8], b"QManage/");
        assert_eq!(&cn[8..], &[0xBB, 0xB6, 0xD3, 0xAD], "「欢迎」的 GBK 字节");
        // 明确不是线上字节：线上尾 = edcode::encode(gbk)（6-bit 变换会改变字节）。
        assert_ne!(
            mir2_protocol::edcode::encode(&cn),
            cn,
            "线上尾不是裸 GBK（EDCode 会变换）"
        );
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
            npc_actor_id: 1,
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

    /// **F-1 返工**（reviewer-5 `b164fa48`）：`NpcPlayer` 的三组任务位图必须走 oracle **位语义**
    /// （`nFlag -= 1` + `128 >> n`，MSB-first），并复用本仓已合入的
    /// `crates/script::engine::quest_state::QuestBitmap`（真身向量 `tests/parity/m3/quest-state-probe`）。
    ///
    /// 红检：把 `set_quest_flag_status` 改回 `quest_flag[index] = value`（去掉位运算）⇒ 本用例必红。
    #[test]
    fn quest_flag_and_unit_use_oracle_bit_semantics() {
        let mut rng = no_rng();
        let mut p = NpcPlayer::empty(&mut rng);
        // flag 8 ⇒ nFlag=7 ⇒ 字节 0、位 `128>>7 = 1`（`PlayObject.cs:3990`）。
        p.set_quest_flag_status(8, 1);
        assert_eq!(p.quest_flag_status(8), 1);
        assert_eq!(
            p.quest_flag.as_bytes()[0],
            0x01,
            "flag 8 落在字节 0 的最低位"
        );
        p.set_quest_flag_status(8, 0);
        assert_eq!(p.quest_flag_status(8), 0);
        // `nFlag < 0` / 越界 ⇒ no-op / 返回 0（oracle `quest_state.rs` 顶部表）。
        p.set_quest_flag_status(0, 1);
        p.set_quest_flag_status(-1, 1);
        assert_eq!(p.quest_flag_status(0), 0);
        assert_eq!(p.quest_flag_status(-1), 0);
        // `QuestUnit` / `QuestUnitOpen` 与 `QuestFlag` 同形（reviewer 点名的 `quest_unit_status` 恒 0 缺口）。
        p.set_quest_unit_status(8, 1);
        p.set_quest_unit_open_status(8, 1);
        assert_eq!(p.quest_unit_status(8), 1);
        assert_eq!(p.quest_unit_open_status(8), 1);
        assert_eq!(p.quest_unit.as_bytes()[0], 0x01);
        assert_eq!(p.quest_unit_open.as_bytes()[0], 0x01);
    }

    /// **PL-2 / P31**：`ScriptPlayer::set_auto_get_exp`（oracle `ExecutionProcessingSys.cs:2707-2710`）
    /// 写四字段；`None` 地图形参 = `AutoGetExpEnvir null`。
    #[test]
    fn npc_player_set_auto_get_exp_writes_four_fields() {
        let mut rng = |_v: i32| 0;
        let mut p = NpcPlayer::empty(&mut rng);
        // ctor 缺省（`PlayObject.cs:1009-1012`）。
        assert_eq!(p.auto_get_exp_time, 0);
        assert_eq!(p.auto_get_exp_point, 0);
        assert!(p.auto_get_exp_envir.is_none());
        assert!(!p.auto_get_exp_in_safe_zone);
        p.set_auto_get_exp(true, None, 1_400_000, 1);
        assert_eq!(
            (
                p.auto_get_exp_in_safe_zone,
                p.auto_get_exp_envir.clone(),
                p.auto_get_exp_time,
                p.auto_get_exp_point
            ),
            (true, None, 1_400_000, 1)
        );
        p.set_auto_get_exp(false, Some("A"), 5_000, 20);
        assert_eq!(
            (
                p.auto_get_exp_in_safe_zone,
                p.auto_get_exp_envir.clone(),
                p.auto_get_exp_time,
                p.auto_get_exp_point
            ),
            (false, Some("A".to_string()), 5_000, 20)
        );
    }

    // ---- lane marketdef：商人脚本表 / GotoLable 说词 / GetVariableText ----

    fn merchant_script(pairs: &[(&str, &str)]) -> ScriptInfo {
        ScriptInfo {
            is_quest: false,
            quest_info: Default::default(),
            record_list: pairs
                .iter()
                .map(|(label, say)| SayingRecord {
                    s_label: (*label).to_string(),
                    procedure_list: vec![SayingProcedure {
                        condition_list: Vec::new(),
                        action_list: Vec::new(),
                        else_action_list: Vec::new(),
                        s_say_msg: (*say).to_string(),
                        s_else_say_msg: String::new(),
                    }],
                    bo_ext_jmp: false,
                })
                .collect(),
            quest_count: 0,
        }
    }

    fn merchant_book(
        actor_id: i32,
        chr_name: &str,
        price_rate: i32,
        pairs: &[(&str, &str)],
    ) -> MerchantScriptBook {
        let mut book = MerchantScriptBook::new(30);
        book.insert(MerchantNpc {
            def: NpcDef {
                chr_name: chr_name.to_string(),
                file_path: "Market_Def".to_string(),
                script_name: Some("卫家店-0103".to_string()),
                is_hide: false,
                is_quest: false,
                scripts: vec![merchant_script(pairs)],
                loaded: true,
                actor_id,
            },
            price_rate,
            castle: None,
            map_name: "0103".to_string(),
            curr_x: 9,
            curr_y: 12,
        });
        book
    }

    /// **C2 `merchant_click_main_say`**：`GotoLable("@main")` 的说词经 `GetLineVariableText`
    /// 展开后收进 [`SayMsg`]：前缀 `NPC 名 + '/'`、`npc_actor_id` = 真实实体 id、
    /// `priority=false`（`ScriptEngine.cs:997` 的 `SendMsg(normNpc)` 支）。
    ///
    /// 牙：不种 `price_rate`（`$PRICERATE` 不展开）/ 不实现变量展开 / 用 0 当 NPC id ⇒ 红。
    #[test]
    fn merchant_click_main_say_expands_player_and_npc_vars() {
        let book = merchant_book(
            42,
            "卫家店",
            120,
            &[(
                "@main",
                "你好<$USERNAME>！元宝<$GAMEGOLD> 价格比例<$PRICERATE> \\ <返回/@Main>",
            )],
        );
        let mut rng = no_rng();
        let mut p = NpcPlayer::empty(&mut rng);
        p.chr_name = "张三".to_string();
        p.game_gold = 7;
        let says = book
            .goto_label(42, &mut p, "@main", false)
            .expect("商人已装载");
        assert_eq!(says.len(), 1);
        assert_eq!(
            says[0].raw_msg,
            "你好张三！元宝7 价格比例120 \\ <返回/@Main>"
        );
        assert_eq!(says[0].npc_name, "卫家店");
        assert_eq!(
            says[0].text,
            "卫家店/你好张三！元宝7 价格比例120 \\ <返回/@Main>"
        );
        assert!(!says[0].priority, "最终说词走 SendMsg（非 High）");
        assert_eq!(says[0].npc_actor_id, 42, "Recog 必须是商人实体 id（G-4）");
        // `GotoLable` 选脚本/LastNpc（`ScriptEngine.cs:41-70`）。
        assert_eq!(p.script_index, Some(0));
        assert_eq!(p.last_npc, 42);
    }

    /// **C4 `merchant_dialog_label_gate`**：`GetScriptLabel` 从说词重建的标签表使
    /// `<修理/@repair>` 可跳；冷标签（`@upgradenow`）仍被拒（`PlayObject.Base.cs:2814-2826`）。
    ///
    /// 牙：不重建 / 删掉 `@main` 特判 / 大小写比较改成敏感 ⇒ 红。
    #[test]
    fn merchant_dialog_label_gate() {
        let labels = crate::shop::get_script_label("欢迎 <购买/@buy> <修理/@repair> <离开/@exit>");
        assert_eq!(labels, vec!["@buy", "@repair", "@exit"]);
        let mut dice = None;
        assert!(crate::shop::lable_is_can_jmp(&labels, &mut dice, "@main"));
        assert!(crate::shop::lable_is_can_jmp(&labels, &mut dice, "@BUY"));
        assert!(!crate::shop::lable_is_can_jmp(
            &labels,
            &mut dice,
            "@upgradenow"
        ));
        // 空的表只放行 `@main`（未说词前的冷直发）。
        assert!(!crate::shop::lable_is_can_jmp(&[], &mut dice, "@repair"));
    }

    /// 商人的 `ScriptInfo`：每个 label 带一条动作（`QuestActionInfo`，`n_cmd_code` =
    /// `codes.rs` 的 `field_index`，与 C# 枚举值相等）。
    fn merchant_script_with_actions(
        pairs: &[(&str, &str, Vec<mir2_script::QuestActionInfo>)],
    ) -> ScriptInfo {
        ScriptInfo {
            is_quest: false,
            quest_info: Default::default(),
            record_list: pairs
                .iter()
                .map(|(label, say, actions)| SayingRecord {
                    s_label: (*label).to_string(),
                    procedure_list: vec![SayingProcedure {
                        condition_list: Vec::new(),
                        action_list: actions.clone(),
                        else_action_list: Vec::new(),
                        s_say_msg: (*say).to_string(),
                        s_else_say_msg: String::new(),
                    }],
                    bo_ext_jmp: false,
                })
                .collect(),
            quest_count: 0,
        }
    }

    fn book_with_actions(
        actor_id: i32,
        chr_name: &str,
        pairs: &[(&str, &str, Vec<mir2_script::QuestActionInfo>)],
    ) -> MerchantScriptBook {
        let mut book = MerchantScriptBook::new(30);
        book.insert(MerchantNpc {
            def: NpcDef {
                chr_name: chr_name.to_string(),
                file_path: "Market_Def".to_string(),
                script_name: Some("寄售商-0103".to_string()),
                is_hide: false,
                is_quest: false,
                scripts: vec![merchant_script_with_actions(pairs)],
                loaded: true,
                actor_id,
            },
            price_rate: 100,
            castle: None,
            map_name: "0103".to_string(),
            curr_x: 9,
            curr_y: 12,
        });
        book
    }

    fn act(code: i32, p1: &str, p2: &str) -> mir2_script::QuestActionInfo {
        mir2_script::QuestActionInfo {
            n_cmd_code: code,
            s_param1: p1.into(),
            s_param2: p2.into(),
            ..Default::default()
        }
    }

    /// **T-142 宿主接线（牙 a）**：`PLAYDICE`（`ActionOfPlayDice`，
    /// `ExecutionProcessingSys.cs:2030-2034`）把 `sParam2` 写进 `PlayObject.PlayDiceLabel`
    /// （`PlayObject.cs:588`）并下发 `RM_PLAYDICE`；客户端随后回跳该标签时
    /// `LableIsCanJmp` 第三支（`PlayObject.Base.cs:2824-2828`）放行**一次**。
    ///
    /// 牙：删掉 `NpcPlayer::set_play_dice_label` 覆写（回落 trait 默认 no-op）⇒
    /// 第一条断言红（永远 `None`）。
    #[test]
    fn t142_play_dice_label_lands_in_host_and_is_consumed_once() {
        let book = book_with_actions(
            9,
            "掷骰商",
            &[("@main", r"掷骰子\ <返回/@main>", vec![act(42, "", "@dice")])],
        );
        let mut rng = no_rng();
        let mut p = NpcPlayer::empty(&mut rng);
        p.chr_name = "张三".to_string();
        book.goto_label(9, &mut p, "@main", false)
            .expect("商人已装载");
        assert_eq!(
            p.play_dice_label(),
            Some("@dice".to_string()),
            "PLAYDICE 的 sParam2 必须落到宿主（trait 默认 no-op 会永远 None）"
        );
        // 同一动作还下发 `RM_PLAYDICE(10500)`（`ExecutionProcessingSys.cs:2033`）。
        assert!(
            p.frames.iter().any(|f| f.ident == 10500),
            "RM_PLAYDICE 帧必须下发：{:?}",
            p.frames
        );
        // 客户端把 `<.../@dice>` 发回：第三支命中并**一次性消费**（清成 string.Empty）。
        let mut label = p.play_dice_label();
        assert!(crate::shop::lable_is_can_jmp(&[], &mut label, "@DICE"));
        assert_eq!(label, Some(String::new()), "命中即清 `string.Empty`");
        assert!(
            !crate::shop::lable_is_can_jmp(&[], &mut label, "@dice"),
            "PlayDiceLabel 只能用一次"
        );
    }

    /// **T-142 宿主接线（牙 c）**：`OPENYBDEAL`（`ActionOfOpenSaleDeal`，
    /// `ExecutionProcessingSys.cs:2329-2357`）成功路径扣 `GameGold`（`:2345`）并置
    /// `SaleDeal = true`（`:2346`）；已开通（`:2334-2338`）与不足（`:2349-2353`）两路
    /// **不动** `SaleDeal`/`GameGold`。不足提示用宿主注入的 `GameGoldName`（`ServerConf.cs:108`）。
    ///
    /// 牙：删掉 `NpcPlayer::set_sale_deal` 的 `self.sale_deal = v` ⇒ 成功路径断言红。
    #[test]
    fn t142_openybdeal_sale_deal_and_game_gold_side_effects() {
        let book = book_with_actions(
            11,
            "寄售商",
            &[(
                "@open",
                r"开通结果\ <返回/@main>",
                vec![act(342, "100", "")],
            )],
        );
        let mut rng = no_rng();
        // 成功：150 >= 100 ⇒ 扣 100 + SaleDeal=true。
        let mut p = NpcPlayer::empty(&mut rng);
        p.chr_name = "张三".to_string();
        p.game_gold = 150;
        book.goto_label(11, &mut p, "@open", false)
            .expect("商人已装载");
        assert!(p.sale_deal(), "成功路径必须置 SaleDeal（`:2346`）");
        assert_eq!(p.game_gold, 50, "成功路径扣 nGameGold（`:2345`）");
        // 已开通：不扣费、不重写。
        book.goto_label(11, &mut p, "@open", false).unwrap();
        assert!(p.sale_deal());
        assert_eq!(p.game_gold, 50, "已开通路径不得再扣费（`:2334-2338`）");
        // 不足：SaleDeal 保持 false、GameGold 不动，提示用配置的 GameGoldName。
        let mut p2 = NpcPlayer::empty(&mut rng);
        p2.chr_name = "李四".to_string();
        p2.game_gold = 99;
        p2.vars.game_gold_name = "灵符".to_string();
        book.goto_label(11, &mut p2, "@open", false).unwrap();
        assert!(!p2.sale_deal(), "不足路径不得置 SaleDeal（`:2349-2353`）");
        assert_eq!(p2.game_gold, 99);
        assert!(
            p2.frames
                .iter()
                .any(|f| f.msg.as_deref().is_some_and(|m| m.contains("灵符"))),
            "不足提示必须用宿主注入的 GameGoldName（默认只给 \"元宝\"）：{:?}",
            p2.frames
        );
    }

    /// **C6 `merchant_repair_ok_label`**：修理成功后 `GotoLable("~@repair")` / `"~@s_repair"`
    /// 的标签与说词（`Merchant.cs:1652/1659`）。脚本无该标签时**无说词但不 panic**；
    /// 未装载（actor 不在表里）⇒ `None`（oracle `normNpc == null ⇒ return`）。
    ///
    /// 牙：把 `~@repair` 写成 `@repair`（前缀错）/ 未装载时返回 Some ⇒ 红。
    #[test]
    fn merchant_repair_ok_label_say_and_degrade() {
        let book = merchant_book(
            7,
            "恶魔布衣",
            100,
            &[
                ("~@repair", "修得还算不错。\\ \\ \\ <返 回/@Main>"),
                ("~@s_repair", "特殊修理完成。\\ \\ \\ <返 回/@Main>"),
            ],
        );
        let mut rng = no_rng();
        let mut p = NpcPlayer::empty(&mut rng);
        let says = book.goto_label(7, &mut p, "~@repair", false).unwrap();
        assert_eq!(says.len(), 1);
        assert_eq!(says[0].raw_msg, "修得还算不错。\\ \\ \\ <返 回/@Main>");
        let says = book.goto_label(7, &mut p, "~@s_repair", false).unwrap();
        assert_eq!(says[0].raw_msg, "特殊修理完成。\\ \\ \\ <返 回/@Main>");
        // 未装载脚本（actor 不在表里）：oracle `ActorMgr.Get` 为 null ⇒ 直接 return。
        assert!(book.goto_label(999, &mut p, "~@repair", false).is_none());
        // 已装载但脚本无该标签：`RecordList.TryGetValue` 失败 ⇒ 无说词（不是 None）。
        let empty_book = merchant_book(8, "空店", 100, &[("@main", "hi")]);
        assert_eq!(
            empty_book.goto_label(8, &mut p, "~@repair", false).unwrap(),
            Vec::new()
        );
    }

    /// **C7 `merchant_variable_text_vectors`**：`NormNpc.GetVariableText` 的边界口径。
    ///
    /// 牙（任一破坏 ⇒ 红）：
    /// - switch **大小写敏感**（`$username` 不展开）；
    /// - `ReplaceVariableText` **只替换第一处**；
    /// - 未知 token（`$GAMEGIRD/$GAMEDIAMOND`，oracle 无分支）**原样保留**；
    /// - `Castle == null` ⇒ `????`；`$GUILDNAME` 无行会 ⇒ **整条 = "无"**；
    /// - `$STR` 12 段槽位（含 800..1199 的 -700 偏移与 `G12` 的 C# 长度怪点）；
    /// - `IndexOf('>') < 1` 的**门**（无 `>` / 位置 0 ⇒ 中止）与 101 次上限。
    #[test]
    fn merchant_variable_text_vectors() {
        let mut rng = no_rng();
        let mut p = NpcPlayer::empty(&mut rng);
        p.chr_name = "张三".to_string();
        // 大小写敏感：C# 字符串 switch。
        assert_eq!(p.line_variable_text("<$username>"), "<$username>");
        // 未知 token 原样（oracle 没有这两个分支；部署件各出现 23/20 次）。
        assert_eq!(
            p.line_variable_text("<$GAMEGIRD>/<$GAMEDIAMOND>"),
            "<$GAMEGIRD>/<$GAMEDIAMOND>"
        );
        // 首次替换（直接调 GetVariableText：同一条里有两次只换第一处）。
        let mut msg = "x<$USERNAME>y<$USERNAME>".to_string();
        p.get_variable_text("$USERNAME", &mut msg);
        assert_eq!(msg, "x张三y<$USERNAME>");
        // `GetLineVariableText` 循环会把第二个也换掉。
        assert_eq!(
            p.line_variable_text("x<$USERNAME>y<$USERNAME>"),
            "x张三y张三"
        );
        // 门：没有 `>` ⇒ 中止；`>` 在位置 0 ⇒ 中止。
        assert_eq!(
            p.line_variable_text("没有闭合 <$USERNAME"),
            "没有闭合 <$USERNAME"
        );
        assert_eq!(p.line_variable_text(">abc"), ">abc");
        // 101 次上限：`>` 存在但 ArrestStringEx 失败（无 `<` 在 `>` 前）⇒ 不挂死、原样返回。
        assert_eq!(p.line_variable_text("a>b"), "a>b");
        // Castle == null ⇒ "????"（`NormNpc.cs:749-894`）。
        assert_eq!(
            p.line_variable_text("<$OWNERGUILD>|<$LORD>|<$CASTLENAME>"),
            "????|????|????"
        );
        // Castle 非 null 的三支。
        p.vars.castle = Some(CastleVars {
            name: "沙巴克".to_string(),
            own_guild: String::new(),
            lord: None,
        });
        assert_eq!(
            p.line_variable_text("<$OWNERGUILD>|<$LORD>|<$CASTLENAME>"),
            "游戏管理|管理员|沙巴克"
        );
        p.vars.castle = Some(CastleVars {
            name: "沙巴克".to_string(),
            own_guild: "王者".to_string(),
            lord: Some("老大".to_string()),
        });
        assert_eq!(p.line_variable_text("<$OWNERGUILD>|<$LORD>"), "王者|老大");
        // `$GUILDNAME` 无行会 ⇒ **整条说词 = "无"**（怪异点，照搬）。
        assert_eq!(p.line_variable_text("前缀<$GUILDNAME>后缀"), "无");
        p.vars.guild_name = Some("行会甲".to_string());
        assert_eq!(
            p.line_variable_text("前缀<$GUILDNAME>后缀"),
            "前缀行会甲后缀"
        );
        // 商人覆写：`$USERWEAPON` / `$UPGRADEWEAPONFEE`。
        assert_eq!(p.line_variable_text("<$USERWEAPON>"), "无");
        p.vars.user_weapon = Some("炼狱".to_string());
        assert_eq!(p.line_variable_text("<$USERWEAPON>"), "炼狱");
        p.vars.upgrade_weapon_price = 25_000;
        assert_eq!(p.line_variable_text("<$UPGRADEWEAPONFEE>"), "25000");
        // `$STR` 12 段（说词侧，`NormNpc.cs:1532-1587`）。
        // 复验 Important 2：只经 `ScriptPlayer` 的 setter 写（脚本动作的唯一落点），
        // **不直接写字段**——否则测的是读侧，掩掉写侧空默认的缺陷。
        p.set_mn_val(9, 11);
        p.set_mdy_val(4, 22);
        p.set_mnmval(9, 33);
        p.set_ms_string(9, "S9值".to_string());
        p.set_mn_integer(5, 66);
        p.set_global_val(5, 55);
        p.set_global_val(1, 99);
        p.set_globa_dy_mval(3, 44);
        p.set_global_a_val(5, "A5值".to_string());
        p.set_server_str_val(7, "E7值".to_string());
        p.set_server_int_val(7, 77);
        assert_eq!(
            p.line_variable_text("<$STR(M9)>"),
            "33",
            "300..399 → MNMval"
        );
        assert_eq!(
            p.line_variable_text("<$STR(D4)>"),
            "22",
            "200..299 → MDyVal"
        );
        assert_eq!(
            p.line_variable_text("<$STR(S9)>"),
            "S9值",
            "600..699 → MSString"
        );
        assert_eq!(
            p.line_variable_text("<$STR(N5)>"),
            "66",
            "500..599 → MNInteger"
        );
        assert_eq!(
            p.line_variable_text("<$STR(G5)>"),
            "55",
            "100..199 → GlobalVal"
        );
        assert_eq!(
            p.line_variable_text("<$STR(I3)>"),
            "44",
            "400..499 → GlobaDyMval"
        );
        assert_eq!(
            p.line_variable_text("<$STR(A5)>"),
            "A5值",
            "700..799 → GlobalAVal"
        );
        assert_eq!(
            p.line_variable_text("<$STR(E7)>"),
            "E7值",
            "1600..1699 → ServerStrVal"
        );
        assert_eq!(
            p.line_variable_text("<$STR(W7)>"),
            "77",
            "1700..1799 → ServerIntVal"
        );
        // **BUG-REPLICA**：`G12` 长度 3 ⇒ 槽 112（GlobalVal[12]），不是 812。
        p.set_global_val(12, 12);
        assert_eq!(p.line_variable_text("<$STR(G12)>"), "12");
        // 槽位号 < 0 ⇒ 不替换。
        assert_eq!(p.line_variable_text("<$STR(Z1)>"), "<$STR(Z1)>");
    }

    /// **复验 Important 2（生产路径）**：脚本 `#ACT` 的 `MOV/INC/DEC` 经引擎落到
    /// `ScriptPlayer::set_*`，随后 `#SAY` 的 `<$STR(...)>` 必须展开成写入值。
    ///
    /// 走「文件解析 → `Engine::goto_label` → setter → `GetLineVariableText`」，**不碰字段**。
    /// oracle `GotoLableQuestActionProcess` 对**已注册动作**执行一条即 return
    /// （`ScriptEngine.cs:889-893`）⇒ 每条动作各放一个 `#IF/#ACT/#SAY` procedure。
    /// 红检：删任一 `NpcPlayer::set_*` 覆写 ⇒ 对应槽回落到空默认（`0`/`""`）⇒ 本用例红。
    #[test]
    fn str_slots_set_by_script_actions_reach_say_text() {
        let root = std::env::temp_dir().join(format!("gs-npc-str-{}", std::process::id()));
        let dir = root.join("Envir").join("MapQuest_def");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("QManage.txt"),
            concat!(
                "[@Login]\r\n",
                "#ACT\r\n",
                "MOV D4 22\r\n",
                "#SAY\r\n",
                "D4=<$STR(D4)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV M9 33\r\n",
                "#SAY\r\n",
                "M9=<$STR(M9)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "INC M9 1\r\n",
                "#SAY\r\n",
                "M9b=<$STR(M9)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV N5 66\r\n",
                "#SAY\r\n",
                "N5=<$STR(N5)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV S9 S9v\r\n",
                "#SAY\r\n",
                "S9=<$STR(S9)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV G5 55\r\n",
                "#SAY\r\n",
                "G5=<$STR(G5)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "DEC G5 5\r\n",
                "#SAY\r\n",
                "G5b=<$STR(G5)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV I3 44\r\n",
                "#SAY\r\n",
                "I3=<$STR(I3)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV A5 A5v\r\n",
                "#SAY\r\n",
                "A5=<$STR(A5)>|\r\n",
                "#IF\r\n",
                "#ACT\r\n",
                "MOV G12 12\r\n",
                "#SAY\r\n",
                "G12=<$STR(G12)>\r\n",
            ),
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
        let mut rename = move |_min: i32, _max: i32| 1;
        let host = NpcHost::load(&root.join("Envir"), &[record], &[], 40, &mut rename);
        assert!(host.failures.is_empty(), "{:?}", host.failures);
        let mut rng = no_rng();
        let mut player = NpcPlayer::empty(&mut rng);
        let says = host.run_login(&mut player).expect("QManage 必须可加载");
        assert_eq!(says.len(), 1);
        assert_eq!(
            says[0].raw_msg,
            concat!(
                "D4=22|M9=34|M9b=34|N5=66|S9=S9v|G5=50|G5b=50|",
                "I3=44|A5=A5v|G12=12"
            ),
            "MOV/INC/DEC 写入的 $STR 槽必须在 #SAY 里取到"
        );
        std::fs::remove_dir_all(&root).ok();
    }
}
