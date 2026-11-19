//! 「使用药品（`CM_EAT=1006`）」的**决策 + 状态迁移 + 回帧**纯函数接缝。
//!
//! # oracle 链（逐行对齐）
//!
//! ```text
//! PlayObject.Message.cs:826  case CM_EAT: ClientUseItems(nParam1, Msg)
//!   PlayObject.Operate.cs:601  ClientUseItems(int nItemIdx, string sItemName)
//!     ├─ if (BoCanUseItem)                          // false ⇒ SendMsg(ManageNPC, RM_MENU_OK, …, ActorId,…)
//!     │   └─ if (!Death)
//!     │       └─ for i in 0..ItemList.Count          // 首个 MakeIndex 命中即处理；**无论成败都 break**
//!     │           ├─ GetStdItem(userItem.Index) == null ⇒ 不处理，落 SM_EAT_FAIL
//!     │           └─ switch (stdItem.StdMode)
//!     │               case 0/1/2/3 ⇒ if (EatItems(stdItem, userItem)) { Dispose; RemoveAt(i); eatSuccess=true }
//!     │               case 4      ⇒ ReadBook（技能书）——未实现（T-17）
//!     │               case 31     ⇒ AniCount==0 ? 解包（UnbindList）: UseStdModeFunItem
//!     │                             ——`AniCount != 0` 已做（看 FunctionNPC 是否存在），解包已实现（曾登记未做，2026-11-19 核对更正为已实现）
//!     ├─ if (eatSuccess) { WeightChanged(); SendDefMessage(SM_EAT_OK,0,0,0,0) }
//!     └─ else            { SendDefMessage(SM_EAT_FAIL,0,0,0,0) }
//!
//! PlayObject.Base.cs:2047  EatItems(StdItem, UserItem) —— **本片做 boNODRUG 守卫 + StdMode 0/1/2**
//!   ├─ if (Envir.Flag.boNODRUG) { SysMsg(CanotUseDrugOnThisMap, Red, Hint); return false }  // 仅 0/1/2/3 进这里
//!   ├─ case 0: switch (stdItem.Shape)
//!        1 ⇒ IncHealthSpell(AC, MAC)
//!        2 ⇒ UserUnLockDurg = true
//!        3 ⇒ IncHealthSpell(Round(MaxHP/100.0*AC), Round(MaxMP/100.0*MAC))
//!        _ ⇒ if (AC>0) IncHealth += AC; if (MAC>0) IncSpell += MAC
//!        （四条分支最后都 result = true）
//!   ├─ case 1: nOldStatus = GetMyStatus(); HungerStatus += DuraMax/10; HungerStatus = _MIN(5000, HungerStatus);
//!   │          if (nOldStatus != GetMyStatus()) RefMyStatus();  result = true
//!   └─ case 2 ⇒ result = true（无任何状态变更）
//!
//! PlayObject.Base.cs:1072  UseStdModeFunItem(StdItem) —— **StdMode 31 / AniCount != 0**
//!   └─ if (SystemShare.FunctionNPC != null) { GotoLable("@StdModeFunc"+AniCount); return true }
//!       （脚本派发本身不在本接缝作用域；**是否吃掉**只看 FunctionNPC 是否存在）
//! ```
//!
//! # 两类下行（本接缝把它们**分开**产出，别混成一条流）
//!
//! 1. **即时帧** `immediate_frames`：`SendDefMessage` → `SendSocket` 直接写 gate buffer
//!    （`SM_EAT_OK=635` / `SM_EAT_FAIL=636`），恒 **1 条**。
//! 2. **延时通知** `notices`：`SendMsg` 只入 `MsgQueue`（`PriorityQueue<SendMessage,byte>`，
//!    优先级 = `(byte)wIdent`），由消息泵在**随后**的 tick 下发：
//!    - `IncHealthSpell` 尾部 `HealthSpellChanged()` ⇒ `RM_HEALTHSPELLCHANGED=10052`
//!      （`BaseObject.cs:952`；`Race == Play` ⇒ `SendUpdateMsg`，`SendHp` 关闭时不再发 Ref）；
//!    - 成功尾部 `WeightChanged()` ⇒ `RM_WEIGHTCHANGED=10115`（同时 `WAbil.Weight = RecalcBagWeight()`）；
//!    - `BoCanUseItem == false` ⇒ `RM_MENU_OK=10309`（`nParam1 = ActorId`）。
//!
//! ⚠️ 真实协议上「即时帧晚于通知、还是通知晚于即时帧」由消息泵决定；本接缝**不**合并两条流，
//! 只各自保序（oracle 探针也是分开截获的，见 `tests/parity/m3/eat-probe`）。
//!
//! # 与 oracle 逐字段相同怎么保证
//!
//! 用例由 `mir2-rs/tests/parity/m3/eat-probe`（引用 `src/M2Server` 真身、反射调**私有**
//! `ClientUseItems`，替身 `INetChannel` 截即时帧 + 反射读 `MsgQueue` 取出队通知）实测产出，
//! 落 `mir2-rs/tests/parity/m3/fixtures/eat_vectors.json`；本模块用例回读逐字段核对
//! （改坏任一映射 ⇒ 必红）。生成命令（**在仓库根运行**）：
//!
//! ```text
//! dotnet build mir2-rs/tests/parity/m3/eat-probe -c Release
//! dotnet mir2-rs/tests/parity/m3/eat-probe/bin/Release/net8.0/eat-probe.dll \
//!   mir2-rs/tests/parity/m3/fixtures/eat_vectors.json
//! ```
//!
//! # 未做清单（登记在卡 `openmir2-m3-bag-equip-alignment` 的 whitelist）
//!
//! - **StdMode 3/4**：`ExtraAbil` buff / `GetExp`（Shape 12/13）、`ReadBook`（技能书）——
//!   未实现；且 StdMode 3 的 `default` 还要走 `EatUseItems`（传送/祝福油等大地图副作用）。
//!   oracle 对这些 StdMode 有**成功**路径（StdMode 3 的 Shape 13 **无条件**返回 true；其余为条件成功），
//!   成功时吃掉物品 + `SM_EAT_OK`，而本接缝只处理 `std_mode==0|1|2` 与 **31 的 `AniCount != 0`**
//!   ⇒ 同一输入落 `SM_EAT_FAIL`，**这是已证的行为差异**（不是"一致地不吃"）。
//!   **`EatUseItems` 的 8 个 case/Shape（IT-09…IT-16）逐条落点**（均 oracle `PlayObject.Base.cs`）：
//!   - IT-09 Shape1 回城卷（`:2162→2163`、`:2233`）——未实现，T-17；
//!   - IT-10 Shape2 随机传送（`:2238`，受 `boNORANDOMMOVE`）——未实现，T-17；
//!   - IT-11 Shape3 地牢逃脱（`:2246`，`PvpLevel()<2` 回 Home，否则红名村）——未实现，T-17；
//!   - IT-12 Shape4 祝福油 `WeaptonMakeLuck()`（`:2258`）——未实现，T-17；
//!   - **IT-13 Shape5 行会回城**（`:2264`，`CastleMgr.IsCastleMember`）——未实现，T-17；
//!   - **IT-14 Shape9 修复油 `RepairWeapon()`**（`:2286`）——未实现，T-17；
//!   - **IT-15 Shape10 特修油 `SuperRepairWeapon()`**（`:2292`）——未实现，T-17；
//!   - **IT-16 Shape11 彩票 `WinLottery()`**（`:2298`；`Config.WinLotteryLevel1..6` 本仓已有该组字段）——未实现，T-17。
//!     （加粗四条为 2026-11-19 核对时发现「只被『等大地图副作用』概括」的登记缺口，本行补齐。）
//! - **StdMode 31 / AniCount == 0（解包）**：已实现（卡 `openmir2-unbindlist-consumer`）——
//!   `ItemList.Count + 6 - 1 <= Grobal2.MaxBagItem`（46）时移除本件并解包 6 件
//!   （`UnbindList` 按 `StdItem.Shape` 查名；未命中 ⇒ 空串 ⇒ 0 件）。`StdMode=5`（`case` 名单外）
//!   才是两侧一致地不吃。
//! - **StdMode 1 只补到 `HungerStatus` + `RefMyStatus` 的发帧**：oracle `RefMyStatus()` 里的
//!   `RecalcAbilitys()` **整链**（由装备/buff 算 `WAbil`）只用 `ability::recalc_abilitys` 的最小等价物
//!   顶上（`ctx.recalc`）；它**尚未**覆盖 `PlayObject.RecalcAbilitys` 的套装/`ExtraAbil`/`GetCharStatus`
//!   等尾段 ⇒ 有装备/buff 的跨档场景仍可能与 oracle 不同。已登记在 whitelist 的 **T-17**。
//!   接线前必须先把这些分支补齐。
//!   （注：`boNODRUG` 开关对 0/1/2/3 都在 `EatItems` 首部生效，故本接缝对 1/3 也做该守卫，
//!   但未禁药时 1/3 仍落 `SM_EAT_FAIL`。）
//! - **代码内登记指路**：`useitem.rs` 中段的「StdMode 3/4（ExtraAbil/GetExp、书）未实现」注释
//!   （`eaten = true` 之后那段）**只覆盖 StdMode 3/4**；`EatUseItems` 的 10 个 Shape 见本节
//!   「`StdMode 3` 的 `default`…」那一组条目（IT-09…IT-16）。
//! - **`GetUpgradeStdItem` 的 Desc 升级差量**：同 `bag.rs`/`ability.rs`，传入的模板视为「有效物品」。
//! - `ClientUseItems` 的 `sItemName` 形参在 oracle 里**未被使用**（只按 `MakeIndex` 找），已照搬。

use crate::ability::{recalc_abilitys, Ability, ItemCatalog, RecalcInputs};
use crate::bag::{BagItem, Frame, MAX_BAG_ITEM};
use crate::unbindlist::UnbindList;
// `ExtraAbil` 槽数用宿主/oracle 同名常量（`mir2_world_bevy::components::EXTRA_ABIL_SLOTS` = 7，
// 含 `EABIL_PWRRATE(6)`）；slice 1 曾误写 6（会静默丢掉 PWRRATE），此处对齐并加维度牙。
use mir2_world_bevy::components::EXTRA_ABIL_SLOTS;

/// `CM_EAT`（`src/OpenMir2/Messages.cs` 的 `CM_EAT = 1006`；
/// `PlayObject.Message.cs:826` 的 `case CM_EAT: ClientUseItems(nParam1, Msg)`）。
pub const CM_EAT: u16 = 1006;
/// `SM_EAT_OK`（`src/OpenMir2/Messages.cs:297`）。
pub const SM_EAT_OK: u16 = 635;
/// `SM_EAT_FAIL`（`src/OpenMir2/Messages.cs:298`）。
pub const SM_EAT_FAIL: u16 = 636;
/// `RM_MENU_OK`（`src/OpenMir2/Messages.cs:625`）。
pub const RM_MENU_OK: i32 = 10309;
/// `RM_SYSMESSAGE`（`src/OpenMir2/Messages.cs:646`）。
pub const RM_SYSMESSAGE: i32 = 11004;
/// `RM_HEALTHSPELLCHANGED`（`src/OpenMir2/Messages.cs:576`）。
pub const RM_HEALTHSPELLCHANGED: i32 = 10052;
/// `RM_WEIGHTCHANGED`（`src/OpenMir2/Messages.cs:579`）。
pub const RM_WEIGHTCHANGED: i32 = 10115;
/// `RM_CHARSTATUSCHANGED`（`src/OpenMir2/Messages.cs:599`）——`RecalcAbilitys` 尾部 `StatusChanged()` +
/// `SendUpdateMsg(...)`（`PlayObject.cs:2875-2876`）。
pub const RM_CHARSTATUSCHANGED: i32 = 10139;
/// `RM_MYSTATUS`（`src/OpenMir2/Messages.cs:700`）——`RefMyStatus()` 在 `RecalcAbilitys()` 之后发（`PlayObject.Base.cs:288-292`）。
pub const RM_MYSTATUS: i32 = 8102;
/// `RM_WINEXP`（`src/OpenMir2/Messages.cs` 8075）——`GetExp` 的 `SendMsg(RM_WINEXP, 0, dwExp, 0, 0)`。
pub const RM_WINEXP: i32 = mir2_protocol::messages::RM_WINEXP as i32;
/// `PlayObject.HungerStatus` 的上限（`HUtil32._MIN(5000, HungerStatus)`，`PlayObject.Base.cs:2088`）。
pub const HUNGER_STATUS_MAX: i32 = 5000;

/// `MsgQueue` 里的一条延时 `SendMessage`（`src/Modules/SystemModule/Data/SendMessage.cs`）。
///
/// 只保留会进线缆头的五个字段（`ActorId`/`DeliveryTime`/`Buff` 属于投递层，不在本片判据）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QueuedNotice {
    pub ident: i32,
    pub w_param: i32,
    pub n_param1: i32,
    pub n_param2: i32,
    pub n_param3: i32,
}

impl QueuedNotice {
    /// `SendUpdateMsg(Messages.X, 0,0,0,0, "")` → `SendMsg(this, X, 0,0,0,0)`。
    const fn plain(ident: i32) -> Self {
        Self {
            ident,
            w_param: 0,
            n_param1: 0,
            n_param2: 0,
            n_param3: 0,
        }
    }

    /// `PlayObject.Operate.cs:685`：`SendMsg(SystemShare.ManageNPC, RM_MENU_OK, 0, ActorId, 0, 0, msg)`。
    const fn menu_ok(actor_id: i32) -> Self {
        Self {
            ident: RM_MENU_OK,
            w_param: 0,
            n_param1: actor_id,
            n_param2: 0,
            n_param3: 0,
        }
    }

    /// `BaseObject.SysMsg(CanotUseDrugOnThisMap, MsgColor.Red, MsgType.Hint)`
    /// （`BaseObject.cs:1656`）：`SendMsg(RM_SYSMESSAGE, 0, RedMsgFColor, RedMsgBColor, 0, msg)`。
    /// 文本本身（`MessageSettings.CanotUseDrugOnThisMap`）不在本接缝的观测面内。
    const fn sysmsg(fcolor: u16, bcolor: u16) -> Self {
        Self {
            ident: RM_SYSMESSAGE,
            w_param: 0,
            n_param1: fcolor as i32,
            n_param2: bcolor as i32,
            n_param3: 0,
        }
    }
}

/// [`use_item`] 的观测输入（`ClientUseItems` 读到的全部外部状态）。
#[derive(Clone, Copy)]
pub struct UseItemContext<'a> {
    /// `SystemShare.ItemSystem.GetStdItem`。
    pub catalog: &'a dyn ItemCatalog,
    /// `PlayObject.BoCanUseItem`。
    pub can_use_item: bool,
    /// `PlayObject.Death`。
    pub dead: bool,
    /// `PlayObject.ActorId`（`BoCanUseItem == false` 时的 `RM_MENU_OK.nParam1`）。
    pub actor_id: i32,
    /// `CM_EAT` 的 `nParam1`（要吃的物品的 `MakeIndex`）。
    pub make_index: i32,
    /// `PlayObject.Envir.Flag.boNODRUG`（地图禁药；`EatItems` 的首个守卫）。
    pub no_drug: bool,
    /// `SystemShare.FunctionNPC != null`（`PlayObject.Base.cs:1072` `UseStdModeFunItem` 的**唯一**判据）。
    /// StdMode 31 的 `AniCount != 0` 分支用它。
    pub function_npc_present: bool,
    /// `M2Share.UnbindList`（`M2Share.cs:85`）：`StdMode 31 / AniCount == 0` 解包时按
    /// `StdItem.Shape` 查物品名（`ClientUseItemsGetUnbindItemName`，`PlayObject.Operate.cs:578-580`；
    /// **未命中 = 空串**）。空表 = 未装载 ⇒ `CopyToUserItemFromName("")` 恒 false ⇒ 解包 0 件。
    pub unbind: &'a UnbindList,
    /// `Config.RedMsgFColor`（禁药 `SysMsg(Red)` 的前景色）。
    pub red_msg_fcolor: u16,
    /// `Config.RedMsgBColor`（禁药 `SysMsg(Red)` 的背景色）。
    pub red_msg_bcolor: u16,
    /// `RefMyStatus()` 里 `RecalcAbilitys()` 的观测输入（`PlayObject.cs:2008`）。StdMode 1 跨档才用到；
    /// `None` 时**不**复算 `WAbil`（只发 `RM_CHARSTATUSCHANGED`/`RM_MYSTATUS`）——见模块「未做清单」T-17。
    pub recalc: Option<RecalcInputs<'a>>,
}

/// 本动作会原地改的**演员侧**状态（`PlayObject` 上分散的字段；接缝把它们打包传入传出）。
/// items 线「额外状态」束（harness 状态扩展，`opc-07` 2026-11-20 定）：把 oracle `PlayObject` 上分散、
/// `UseItemActorState` 原先未承载的状态集中一处，供 IT-07..17 逐条 wired。**未 wire 的字段保持 `Default`、
/// 不参与判据**（各自按其宿主原语接入并加最小单测后再启用）。
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemsExtraState {
    /// `PlayObject.BodyLuck`（double，`PlayObject.cs:520`），`AddBodyLuck` 维护（IT-08 `GetExp`）。宿主已有
    /// 同源 `add_body_luck` 纯函数（`lib.rs`）。
    pub body_luck: f64,
    /// `PlayObject.BodyLuckLevel`（`clamp(BodyLuck/BODYLUCKUNIT, -10, 5)`）。
    pub body_luck_level: i32,
    /// `ExtraAbil[EXTRA_ABIL_SLOTS]`（EABIL_DCUP/MCUP/SCUP/HITSPEEDUP/HPUP/MPUP/PWRRATE；IT-07）。
    pub extra_abil: [u16; EXTRA_ABIL_SLOTS],
    /// `ExtraAbilFlag[EXTRA_ABIL_SLOTS]`（IT-07）。
    pub extra_abil_flag: [u8; EXTRA_ABIL_SLOTS],
    /// `ExtraAbilTimes[EXTRA_ABIL_SLOTS]`（宿主存 tick；墙钟派生 ⇒ 判据只核相对增量/单调，不核绝对值）。
    pub extra_abil_times: [i64; EXTRA_ABIL_SLOTS],
    /// 当前装备武器（`UseItems[Weapon]`）；`None` = 空手（IT-12 祝福油 / IT-14/15 修复油）。
    pub weapon: Option<BagItem>,
    /// `PlayObject.MagicList` 的 magic id 序列（IT-17 ReadBook 追加）。
    pub magic_list: Vec<i32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UseItemActorState {
    /// `PlayObject.WAbil`（读 `HP/MP/MaxHP/MaxMP`，写 `HP/MP/Weight`）。
    pub wabil: Ability,
    /// `CharacterObject.IncHealth`（default Shape 分支叠加；`ushort` 语义，溢出回绕）。
    pub inc_health: u16,
    /// `CharacterObject.IncSpell`。
    pub inc_spell: u16,
    /// `CharacterObject.UserUnLockDurg`（Shape 2 置 true）。
    pub user_unlock_durg: bool,
    /// `PlayObject.HungerStatus`（`PlayObject.cs:225`，`int`；StdMode 1 食物加 `DuraMax/10`、上限 5000）。
    pub hunger_status: i32,
    /// 额外状态束（BodyLuck / ExtraAbil / 武器 / MagicList；见 [`ItemsExtraState`]）。
    pub extra: ItemsExtraState,
}

impl Default for UseItemActorState {
    fn default() -> Self {
        // `Ability` 未实现 `Default`（成员语义各异，默认全 0 有歧义）⇒ 这里给零值构造。
        Self {
            wabil: Ability {
                level: 0,
                reserved1: 0,
                ac: 0,
                mac: 0,
                dc: 0,
                mc: 0,
                sc: 0,
                hp: 0,
                mp: 0,
                max_hp: 0,
                max_mp: 0,
                reserved2: 0,
                exp_count: 0,
                exp_max_count: 0,
                exp: 0,
                max_exp: 0,
                weight: 0,
                max_weight: 0,
                wear_weight: 0,
                max_wear_weight: 0,
                hand_weight: 0,
                max_hand_weight: 0,
            },
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
            extra: ItemsExtraState::default(),
        }
    }
}

/// [`use_item`] 的结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseItemOutcome {
    /// oracle `eatSuccess`。
    pub eaten: bool,
    /// 被吃掉的物品在 `ItemList` 里的下标（未吃 ⇒ `None`）。
    pub removed_index: Option<usize>,
    /// 立即下发的帧（恒 1 条：`SM_EAT_OK` 或 `SM_EAT_FAIL`）。
    pub immediate_frames: Vec<Frame>,
    /// 进 `MsgQueue` 的延时通知（保序）。
    pub notices: Vec<QueuedNotice>,
    /// `ClientUseItemsGetUnBindItems`（`PlayObject.Operate.cs:582-599`）新建的**解包物品**（生成序）。
    ///
    /// oracle 对每件调 `SendAddItem(userItem)`（`PlayObject.Base.cs:198-228`，**立即帧**
    /// `SM_ADDITEM(200)`，尾 = `EDCode.EncodeBuffer(ClientItem)`），发生在尾部的
    /// `SendDefMessage(SM_EAT_OK)` **之前** ⇒ 接线层必须先下发本列、再下发 `immediate_frames`。
    /// 空 = 本次没有解包（`StdMode != 31` / `AniCount != 0` / 容量门失败 / 空名或名字查不到模板）。
    pub added_items: Vec<BagItem>,
    /// IT-08：本次 `GetExp` 的经验增量（基础 `Abil.Exp` 侧）。宿主落 `sg.equip.abil.exp`（**不改 `WAbil.Exp`**——
    /// oracle `GetExp` 后 WAbil 陈旧，仅 `AbilCopyToWAbil` 在登录/重算时同步）。非 GetExp ⇒ `None`。
    pub exp_delta: Option<i32>,
}

/// `ClientUseItems`（`PlayObject.Operate.cs:601`）的纯函数等价物（StdMode 0/1/2 + 31 的两支）。
///
/// `item_list` 原地变更（吃掉则移除命中的那一件），`state` 原地变更（`WAbil`/`IncHealth`/`IncSpell`/
/// `UserUnLockDurg`）。返回的 `immediate_frames`/`notices` 由接线层按各自通道下发。
pub fn use_item(
    ctx: &UseItemContext<'_>,
    state: &mut UseItemActorState,
    item_list: &mut Vec<BagItem>,
    // `M2Share.GetItemNumber()`（`M2Share.cs:547`）：`CopyToUserItemFromName` 每建一件就取一次
    // 新 `MakeIndex`。与宿主 `GIVE`/新手物品**共用同一个会话计数器**。
    item_number: &mut i32,
) -> UseItemOutcome {
    let mut eaten = false;
    let mut exp_delta: Option<i32> = None;
    let mut removed_index: Option<usize> = None;
    let mut notices: Vec<QueuedNotice> = Vec::new();
    let mut added_items: Vec<BagItem> = Vec::new();

    if ctx.can_use_item {
        if !ctx.dead {
            // oracle：遍历背包，**首个** `MakeIndex == nItemIdx` 的项处理掉后无条件 `break`
            //（`PlayObject.Operate.cs:614` 的 `break` 在 `if` 块之外）⇒ 同 `MakeIndex` 的重复项只动第一个。
            let mut hit = None;
            for (i, item) in item_list.iter().enumerate() {
                if item.make_index == ctx.make_index {
                    hit = Some(i);
                    break;
                }
            }
            if let Some(i) = hit {
                let idx = item_list[i].index;
                // 模板缺失 ⇒ oracle 的 `if (stdItem != null)` 不成立，什么都不做（落 SM_EAT_FAIL）。
                if let Some(tpl) = ctx.catalog.get(idx) {
                    // `EatItems` 的首个守卫（`PlayObject.Base.cs:2048-2053`）：地图禁药 ⇒ `SysMsg(Red)`
                    // 入 `MsgQueue` 并 `return false`。仅 StdMode 0/1/2/3 会进 `EatItems`
                    //（4/31 走 `ReadBook`/`UseStdModeFunItem`，不受 `boNODRUG` 约束）。
                    if matches!(tpl.std_mode, 0..=3) && ctx.no_drug {
                        notices.push(QueuedNotice::sysmsg(ctx.red_msg_fcolor, ctx.red_msg_bcolor));
                    } else {
                        match tpl.std_mode {
                            0 => {
                                match tpl.shape {
                                    1 => {
                                        // 固定量：IncHealthSpell(AC, MAC)
                                        if inc_health_spell(
                                            state,
                                            i32::from(tpl.ac),
                                            i32::from(tpl.mac),
                                        ) {
                                            notices
                                                .push(QueuedNotice::plain(RM_HEALTHSPELLCHANGED));
                                        }
                                    }
                                    2 => {
                                        // 解锁药：只置 UserUnLockDurg，无即时帧/通知
                                        state.user_unlock_durg = true;
                                    }
                                    3 => {
                                        // 百分比：Round(MaxHP/100.0*AC) / Round(MaxMP/100.0*MAC)
                                        let hp = round_away(
                                            f64::from(state.wabil.max_hp) / 100.0
                                                * f64::from(tpl.ac),
                                        );
                                        let mp = round_away(
                                            f64::from(state.wabil.max_mp) / 100.0
                                                * f64::from(tpl.mac),
                                        );
                                        if inc_health_spell(state, hp, mp) {
                                            notices
                                                .push(QueuedNotice::plain(RM_HEALTHSPELLCHANGED));
                                        }
                                    }
                                    _ => {
                                        // default：AC/MAC 各自 `> 0` 才叠加到 IncHealth/IncSpell（ushort 回绕）
                                        if tpl.ac > 0 {
                                            state.inc_health =
                                                state.inc_health.wrapping_add(tpl.ac);
                                        }
                                        if tpl.mac > 0 {
                                            state.inc_spell = state.inc_spell.wrapping_add(tpl.mac);
                                        }
                                    }
                                }
                                eaten = true;
                            }
                            // StdMode 1（`PlayObject.Base.cs:2085-2092`）：食物。
                            //   nOldStatus = GetMyStatus();
                            //   HungerStatus += DuraMax/10; HungerStatus = _MIN(5000, HungerStatus);
                            //   if (nOldStatus != GetMyStatus()) RefMyStatus();  result = true;
                            1 => {
                                let old_status = my_status(state.hunger_status);
                                state.hunger_status += i32::from(tpl.dura_max) / 10;
                                state.hunger_status = state.hunger_status.min(HUNGER_STATUS_MAX);
                                if old_status != my_status(state.hunger_status) {
                                    // `RefMyStatus()`（`PlayObject.Base.cs:288-292`）= `RecalcAbilitys()` +
                                    // `SendMsg(RM_MYSTATUS)`；`Race==Play` 的 `RecalcAbilitys` 尾部会先发
                                    // `RM_CHARSTATUSCHANGED`（`PlayObject.cs:2875-2876`）。
                                    if let Some(inputs) = &ctx.recalc {
                                        state.wabil = recalc_abilitys(ctx.catalog, inputs).wabil;
                                    }
                                    notices.push(QueuedNotice::plain(RM_CHARSTATUSCHANGED));
                                    notices.push(QueuedNotice::plain(RM_MYSTATUS));
                                }
                                eaten = true;
                            }
                            // StdMode 2（`PlayObject.Base.cs:2095`）：`result = true`，无任何状态变更。
                            2 => {
                                eaten = true;
                            }
                            // StdMode 31（`PlayObject.Operate.cs:657-672`）：
                            //   - `AniCount != 0` ⇒ `UseStdModeFunItem(stdItem)`（`PlayObject.Base.cs:1072`）:
                            //     `if (SystemShare.FunctionNPC != null) { GotoLable("@StdModeFunc"+AniCount); return true; }`
                            //     ——`GotoLable` 的脚本派发本身不在本接缝作用域（登记 T-17），但**是否吃掉**
                            //     只取决于 `FunctionNPC` 是否存在，这条已 1:1。
                            //   - `AniCount == 0` ⇒ 解包（`:658-666`）：
                            //     `if (ItemList.Count + 6 - 1 <= Grobal2.MaxBagItem)` 才吃（容量门取**移除前**的 Count）；
                            //     `ClientUseItemsGetUnBindItems(ClientUseItemsGetUnbindItemName(stdItem.Shape), 6)`
                            //     逐件 `CopyToUserItemFromName`，**失败即 break**（未命中 Shape ⇒ 空名 ⇒ 0 件）。
                            31 if tpl.ani_count != 0 => {
                                // `UseStdModeFunItem`：只有 `FunctionNPC != null` 才吃掉。
                                eaten = ctx.function_npc_present;
                            }
                            // 容量门在**移除本件之前**取 `ItemList.Count`（`PlayObject.Operate.cs:659`）：
                            // `Count + 6 - 1 <= 46`；不满足 ⇒ 落到 `_ => {}`（不吃、`SM_EAT_FAIL`）。
                            31 if item_list.len() + 6 - 1 <= MAX_BAG_ITEM => {
                                // `ClientUseItemsGetUnbindItemName(stdItem.Shape)`：未命中 ⇒ 空串。
                                let name = ctx
                                    .unbind
                                    .get_unbind_item_name(i32::from(tpl.shape))
                                    .to_string();
                                for _ in 0..6 {
                                    // `CopyToUserItemFromName`（`GameItemSystem.cs:113-140`）：
                                    // 空名 ⇒ false；名字查不到模板 ⇒ false ⇒ break（不再继续建件）。
                                    let Some(index) = ctx.catalog.find_by_name(&name) else {
                                        break;
                                    };
                                    let Some(tpl_unbind) = ctx.catalog.get(index) else {
                                        break;
                                    };
                                    *item_number += 1;
                                    added_items.push(BagItem {
                                        make_index: *item_number,
                                        index,
                                        dura: tpl_unbind.dura_max,
                                        dura_max: tpl_unbind.dura_max,
                                        desc: [0; 14],
                                    });
                                }
                                eaten = true;
                            }
                            // StdMode 3：oracle `EatItems` 的 case 3（`PlayObject.Base.cs:2099`，受 `boNODRUG` 门，已在上方）。
                            // **IT-08**：`Shape 13 → GetExp(DuraMax)`，本片实现**非升级支**（Exp+= / AddBodyLuck(dwExp*0.002) /
                            // RM_WINEXP）；**升级支**（`Exp>=MaxExp → GetLevelExp/RecalcLevelAbilitys/SM_LEVELUP/…`）**未实现**，登记 T-17。
                            // IT-07（Shape12 ExtraAbil×6）与 default → EatUseItems（IT-09..16 move 组）亦未实现。
                            3 if tpl.shape == 13 => {
                                // `GetExp`（`PlayObject.Base.cs:149-166`）非升级支：**不改 `WAbil.Exp`**
                                //（oracle 后 WAbil 陈旧），经验增量经 `Outcome.exp_delta` 交宿主落基础 `sg.equip.abil.exp`。
                                exp_delta = Some(i32::from(tpl.dura_max));
                                crate::add_body_luck(
                                    &mut state.extra.body_luck,
                                    &mut state.extra.body_luck_level,
                                    f64::from(tpl.dura_max) * 0.002,
                                );
                                notices.push(QueuedNotice {
                                    ident: RM_WINEXP,
                                    w_param: 0,
                                    n_param1: i32::from(tpl.dura_max),
                                    n_param2: 0,
                                    n_param3: 0,
                                });
                                eaten = true;
                            }
                            // StdMode 4（书）与 StdMode 3 其余 Shape 未实现 ⇒ 已证差异，登记 T-17。
                            _ => {}
                        }
                        if eaten {
                            item_list.remove(i);
                            removed_index = Some(i);
                            // oracle `:661-663`：先 `Dispose(userItem) + ItemList.RemoveAt(i)`，
                            // 再由 `ClientUseItemsGetUnBindItems` 逐件 `ItemList.Add`。
                            // 追加放在移除**之后**，下标语义与重量重算（`WeightChanged()`，`:676`）同源。
                            item_list.extend(added_items.iter().cloned());
                        }
                    }
                }
            }
        }
    } else {
        notices.push(QueuedNotice::menu_ok(ctx.actor_id));
    }

    let mut immediate_frames = Vec::new();
    if eaten {
        // `WeightChanged()`：先按**移除后**的背包重算 `WAbil.Weight`，再排通知。
        state.wabil.weight = recalc_bag_weight(ctx.catalog, item_list);
        notices.push(QueuedNotice::plain(RM_WEIGHTCHANGED));
        immediate_frames.push(Frame {
            ident: SM_EAT_OK,
            recog: 0,
            param: 0,
            tag: 0,
            series: 0,
        });
    } else {
        immediate_frames.push(Frame {
            ident: SM_EAT_FAIL,
            recog: 0,
            param: 0,
            tag: 0,
            series: 0,
        });
    }

    // oracle `MsgQueue` 是 `PriorityQueue<SendMessage,byte>`（优先级 = `(byte)wIdent`）⇒ 消息泵按
    // **优先级升序**下发；夹具的 `notices` 就是出队序。这里按同一键排序，保证跨档时
    // （先入队的 `RM_CHARSTATUSCHANGED`/`RM_MYSTATUS` 反而晚于 `RM_WEIGHTCHANGED`）与 oracle 一致。
    //
    // **同 ident 多帧在本接缝不可达**（卡 `openmir2-frame-drain-order-audit` 的枚举证据）：本函数
    // 的 `notices` 只在五条互斥分支里产生 ——
    //   ① 禁药 `RM_SYSMESSAGE`(252)（`no_drug` 且 StdMode 0..=3；`eaten=false`，与成功支互斥）；
    //   ② `RM_HEALTHSPELLCHANGED`(68)（StdMode 0 的 shape 1 固定量 / shape 3 百分比，两支**互斥**，
    //      且各自 `IncHealthSpell` 成功才入队 ⇒ 至多一条）；
    //   ③ `RM_CHARSTATUSCHANGED`(155) + `RM_MYSTATUS`(166)（StdMode 1 的状态跳档支，成对出现，
    //      且该支不产 68）；
    //   ④ `RM_MENU_OK`(69)（`can_use_item==false` 的 else 支）；
    //   ⑤ `RM_WEIGHTCHANGED`(131)（`eaten` 支）。
    // 每个可达组合内优先级字节两两不同 ⇒ 稳定排序与四叉堆出序逐条相同。可达组合共五个：
    //   `{252}`（禁药）/ `{68,131}`（StdMode 0 shape 1 或 3 补血补魔成功）/ `{155,166,131}`
    //   （StdMode 1 状态跳档）/ **`{131}`**（纯 `WeightChanged`：StdMode 2、StdMode 31 解包成功、
    //   以及 StdMode 0 的 shape 非 1/3 的 default 支）/ `{69}`（`can_use_item==false`）。
    // 新增同 ident 的 `SendMsg` 前必须先改成 `NetPriorityQueue`。
    notices.sort_by_key(|n| n.ident as u8);

    UseItemOutcome {
        eaten,
        removed_index,
        immediate_frames,
        notices,
        added_items,
        exp_delta,
    }
}

/// `BaseObject.IncHealthSpell(int nHp, int nMp)`（`BaseObject.cs:952`）。
///
/// 返回 `false` ⇔ oracle 在开头的 `nHp<0 || nMp<0` 处 `return`（此时**不**调 `HealthSpellChanged`）。
/// `HP/MP` 用 `ushort` 语义（oracle `(ushort)nHp` 强转），超上限则夹到 `MaxHP/MaxMP`。
///
/// 实现只有一份：委托 [`crate::recover::inc_health_spell`]（周期恢复 tick 也用它）。
fn inc_health_spell(state: &mut UseItemActorState, n_hp: i32, n_mp: i32) -> bool {
    crate::recover::inc_health_spell(&mut state.wabil, n_hp, n_mp)
}

/// `PlayObject.GetMyStatus()`（`PlayObject.Base.cs:1183-1191`）：`min(HungerStatus/1000, 4)`（整数除）。
#[must_use]
pub fn my_status(hunger_status: i32) -> i32 {
    let result = hunger_status / 1000;
    if result > 4 {
        4
    } else {
        result
    }
}

/// `HUtil32.Round(double)` = `(int)Math.Round(r, MidpointRounding.AwayFromZero)`（`HUtil32.cs:131`）。
/// Rust `f64::round` 同为「半数远离零」，逐值相同。
fn round_away(r: f64) -> i32 {
    r.round() as i32
}

/// `BaseObject.RecalcBagWeight()`（`BaseObject.cs:1356`）：Σ `GetStdItem(bag[i].Index).Weight`，`ushort` 加。
fn recalc_bag_weight(catalog: &dyn ItemCatalog, item_list: &[BagItem]) -> u16 {
    let mut result: u16 = 0;
    for item in item_list {
        if let Some(tpl) = catalog.get(item.index) {
            result = result.wrapping_add(u16::from(tpl.weight));
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ability::{NakedAbility, StdItemCatalog, USE_ITEMS_LEN};
    use mir2_data::StdItem;
    use serde_json::Value;

    fn fixture() -> Value {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../tests/parity/m3/fixtures/eat_vectors.json"
        );
        let raw = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("oracle 向量夹具缺失/不可读：{path}（{e}）—— 不许静默跳过"));
        serde_json::from_str(&raw).unwrap_or_else(|e| panic!("{path} 不是合法 JSON：{e}"))
    }

    fn u8v(v: &Value) -> u8 {
        v.as_u64().unwrap() as u8
    }
    fn u16v(v: &Value) -> u16 {
        v.as_u64().unwrap() as u16
    }
    fn i32v(v: &Value) -> i32 {
        v.as_i64().unwrap() as i32
    }

    fn template(v: &Value) -> StdItem {
        StdItem {
            name: v["name"].as_str().unwrap().to_string(),
            std_mode: u8v(&v["std_mode"]),
            shape: u8v(&v["shape"]),
            weight: u8v(&v["weight"]),
            ac: u16v(&v["ac"]),
            mac: u16v(&v["mac"]),
            dura_max: u16v(&v["dura_max"]),
            ani_count: u8v(&v["ani_count"]),
            ..StdItem::default()
        }
    }

    fn catalog(doc: &Value) -> StdItemCatalog {
        let mut c = StdItemCatalog::new();
        for (k, v) in doc["templates"].as_object().unwrap() {
            c.insert(k.parse::<u16>().unwrap(), template(v));
        }
        c
    }

    fn ability(v: &Value) -> Ability {
        Ability {
            level: u8v(&v["level"]),
            reserved1: u8v(&v["reserved1"]),
            ac: u16v(&v["ac"]),
            mac: u16v(&v["mac"]),
            dc: u16v(&v["dc"]),
            mc: u16v(&v["mc"]),
            sc: u16v(&v["sc"]),
            hp: u16v(&v["hp"]),
            mp: u16v(&v["mp"]),
            max_hp: u16v(&v["max_hp"]),
            max_mp: u16v(&v["max_mp"]),
            reserved2: v["reserved2"].as_i64().unwrap() as i16,
            exp_count: u8v(&v["exp_count"]),
            exp_max_count: u8v(&v["exp_max_count"]),
            exp: i32v(&v["exp"]),
            max_exp: i32v(&v["max_exp"]),
            weight: u16v(&v["weight"]),
            max_weight: u16v(&v["max_weight"]),
            wear_weight: u8v(&v["wear_weight"]),
            max_wear_weight: u8v(&v["max_wear_weight"]),
            hand_weight: u8v(&v["hand_weight"]),
            max_hand_weight: u8v(&v["max_hand_weight"]),
        }
    }

    fn bag_item(v: &Value) -> BagItem {
        let mut desc = [0u8; 14];
        for (i, d) in v["desc"].as_array().unwrap().iter().enumerate() {
            desc[i] = u8v(d);
        }
        BagItem {
            make_index: i32v(&v["make_index"]),
            index: u16v(&v["index"]),
            dura: u16v(&v["dura"]),
            dura_max: u16v(&v["dura_max"]),
            desc,
        }
    }

    fn bag_items(v: &Value) -> Vec<BagItem> {
        v.as_array().unwrap().iter().map(bag_item).collect()
    }

    fn frame(v: &Value) -> Frame {
        Frame {
            ident: u16v(&v["ident"]),
            recog: i32v(&v["recog"]),
            param: u16v(&v["param"]),
            tag: u16v(&v["tag"]),
            series: u16v(&v["series"]),
        }
    }

    fn frames(v: &Value) -> Vec<Frame> {
        v.as_array().unwrap().iter().map(frame).collect()
    }

    /// 构造一条与探针同口径的 `RecalcAbilitys` 输入：`Abil` = 用例的 `wabil_before`（探针里
    /// `po.Abil = wabilPre`）、无装备、`bag` = 当前背包 `Index` 列表、无 buff。
    /// 测试用**空解包表**（`M2Share.UnbindList` 空 = 未装载 / 无该 Shape）。
    ///
    /// 夹具 `eat_vectors.json` 的 StdMode 31 用例只有两条：`AniCount != 0`（走 FunctionNPC 门）
    /// 与 `eat_fail_stdmode31_unbind_bag_full`（42 件 ⇒ 容量门 `42+6-1 > 46` 不通过）——
    /// 两者都不会读表，空表与 oracle 探针（未装载 UnbindList）同口径。
    fn no_unbind() -> UnbindList {
        UnbindList::default()
    }

    /// 夹具记录的解包表（`unbind`：`Shape 字符串 → 解包物名`）；缺失 = 空表
    /// （oracle `M2Share.UnbindList` 未命中 ⇒ 空名 ⇒ `CopyToUserItemFromName` 恒 false ⇒ 0 件）。
    fn unbind_from(doc: &Value) -> UnbindList {
        let mut map = std::collections::BTreeMap::new();
        if let Some(o) = doc.get("unbind").and_then(Value::as_object) {
            for (k, v) in o {
                map.insert(k.parse::<i32>().unwrap(), v.as_str().unwrap().to_string());
            }
        }
        UnbindList { map }
    }

    /// 防空转 Z1/Z3：`cases` 缺字段 / 非数组 / 为空 / < 已知例数 ⇒ `Err`（零用例不得判绿）。
    fn cases_or_red(doc: &Value) -> Result<&Vec<Value>, String> {
        let cases = doc
            .get("cases")
            .and_then(Value::as_array)
            .ok_or_else(|| "防空转：夹具缺 `cases` 数组".to_string())?;
        if cases.len() < 12 {
            return Err(format!("防空转：夹具用例数异常（{}）", cases.len()));
        }
        Ok(cases)
    }

    /// 通知七字段元组 `(priority, ident, wParam, p1, p2, p3, late)`（排序序列比较与负例牙共用）。
    type NoticeTuple = (i32, i32, i32, i32, i32, i32, bool);
    fn expect_notice_tuples(case: &Value) -> Vec<NoticeTuple> {
        case["expect"]["notices"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| {
                (
                    i32v(&v["priority"]),
                    i32v(&v["ident"]),
                    i32v(&v["w_param"]),
                    i32v(&v["n_param1"]),
                    i32v(&v["n_param2"]),
                    i32v(&v["n_param3"]),
                    v["late"].as_u64().unwrap_or(0) != 0,
                )
            })
            .collect()
    }

    /// 排序后多重集相等（重复条目计数）。
    fn notices_equal(mut a: Vec<NoticeTuple>, mut b: Vec<NoticeTuple>) -> bool {
        a.sort();
        b.sort();
        a == b
    }

    /// D1/D2：抽空未命中上界且 `MaxDrainPasses=8` 才算合格（`hit_cap=true` ⇒ false ⇒ 比较器红）。
    fn drain_ok(hit_cap: bool, max_passes: u64) -> bool {
        !hit_cap && max_passes == 8
    }

    fn success_case(doc: &Value) -> Value {
        doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_ok_stdmode31_unbind_success")
            .expect("夹具必须含 eat_ok_stdmode31_unbind_success")
            .clone()
    }

    /// Z1/Z3 牙：空 `cases` / 缺 `cases` ⇒ `cases_or_red` 红。
    #[test]
    fn vacuous_guard_empty_or_missing_cases_red() {
        assert!(cases_or_red(&serde_json::json!({"cases": []})).is_err());
        assert!(cases_or_red(&serde_json::json!({})).is_err());
        assert!(cases_or_red(&fixture()).is_ok(), "真实夹具必须过防空转");
    }

    /// L2 牙①：把一条通知的 `priority` 改掉 ⇒ 排序序列不等（优先级进比较面）。
    #[test]
    fn tooth_notice_priority_changed_red() {
        let case = success_case(&fixture());
        let want = expect_notice_tuples(&case);
        let mut got = want.clone();
        got[0].0 += 1;
        assert!(!notices_equal(got, want), "改 priority 必须红");
    }

    /// L2 牙②：重复条目必须计数（多一条全等 ⇒ 不等）。
    #[test]
    fn tooth_notice_duplicate_count_red() {
        let case = success_case(&fixture());
        let want = expect_notice_tuples(&case);
        let mut got = want.clone();
        got.push(want[0]);
        assert!(!notices_equal(got, want), "重复条目未计数 ⇒ 假绿");
    }

    /// D2 牙：`hit_cap=true` ⇒ `drain_ok` false（比较器据此判红）；`max_passes!=8` 亦 false。
    #[test]
    fn tooth_drain_hit_cap_red() {
        assert!(!drain_ok(true, 8), "命中上限必须不合格");
        assert!(!drain_ok(false, 7), "上限不是 8 必须不合格");
        assert!(drain_ok(false, 8));
    }

    /// 状态差（Z4/Z5 共用）：任一 `*_before` 与终态不同，或解包件非空。
    fn case_state_changed(case: &Value) -> bool {
        let ex = &case["expect"];
        case["wabil_before"] != ex["wabil"]
            || case["item_list_before"] != ex["item_list"]
            || case["inc_health_before"] != ex["inc_health"]
            || case["inc_spell_before"] != ex["inc_spell"]
            || case["hunger_status_before"] != ex["hunger_status"]
            || case["user_unlock_durg_before"] != ex["user_unlock_durg"]
            || ex["added_items"]
                .as_array()
                .map(|a| !a.is_empty())
                .unwrap_or(false)
    }

    /// 非零锚（Z4）：帧(ident!=0) / 通知 / RNG / 状态差，任一非空即非「全零账本」。
    fn has_nonzero_anchor(case: &Value) -> bool {
        let ex = &case["expect"];
        let frames_nonzero = ex["immediate_frames"]
            .as_array()
            .map(|a| a.iter().any(|f| f["ident"].as_u64().unwrap_or(0) != 0))
            .unwrap_or(false);
        let notices_nonzero = ex["notices"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let rng_nonzero = ex["rng"].as_array().map(|a| !a.is_empty()).unwrap_or(false);
        frames_nonzero || notices_nonzero || rng_nonzero || case_state_changed(case)
    }

    /// 单例两层（Z5）：`SM 帧>0` 且（`RM 通知>0` 或 `状态有变`）。
    fn case_two_layer(c: &Value) -> bool {
        let sm = c["expect"]["immediate_frames"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        let rm = c["expect"]["notices"]
            .as_array()
            .map(|a| !a.is_empty())
            .unwrap_or(false);
        sm && (rm || case_state_changed(c))
    }

    /// 夹具级（Z5）：至少一例两层。
    fn fixture_covers_two_layers(doc: &Value) -> bool {
        doc["cases"]
            .as_array()
            .map(|cs| cs.iter().any(case_two_layer))
            .unwrap_or(false)
    }

    /// Z4 不适用（golden 全零）的用例名单。**当前为空**（23 例均有 635/636 帧）。
    /// 夹具一变（新增/改名/某例变全零）⇒ 名单 `assert_eq!` 红，须与用例同一次提交更新并写进提交信息。
    const EXPECTED_Z4_EXEMPT: &[&str] = &[];

    /// Z5 只需一层（纯失败：仅 `SM_EAT_FAIL`、无 RM/状态变化）的用例名单——合法「只发失败帧」，
    /// 故豁免两层要求（强判会误红）。夹具一变须同步更新本名单（`assert_eq!` 会咬）。
    const EXPECTED_ONE_LAYER: &[&str] = &[
        "eat_fail_not_found",
        "eat_fail_stdmode_other",
        "eat_fail_template_missing",
        "eat_fail_dead",
        "eat_fail_stdmode31_no_functionnpc",
        "eat_fail_stdmode31_unbind_bag_full",
    ];

    /// 状态扩展（slice 1）：`ItemsExtraState::default()` 为零值束；`UseItemActorState::default().extra` 同零。
    /// 保证新增状态字段的初值可控（未 wire 的字段不影响既有判据）。
    #[test]
    fn items_extra_state_default_is_zero() {
        let d = ItemsExtraState::default();
        assert_eq!(d.body_luck, 0.0);
        assert_eq!(d.body_luck_level, 0);
        // 维度牙（编译器抓不到的"少一格"）：必须 == 宿主/oracle `EXTRA_ABIL_SLOTS`（=7，含 EABIL_PWRRATE(6)）。
        assert_eq!(
            EXTRA_ABIL_SLOTS, 7,
            "= oracle EABIL 槽数（0..=EABIL_PWRRATE(6)）"
        );
        assert_eq!(d.extra_abil.len(), EXTRA_ABIL_SLOTS);
        assert_eq!(d.extra_abil, [0u16; EXTRA_ABIL_SLOTS]);
        assert_eq!(d.extra_abil_flag, [0u8; EXTRA_ABIL_SLOTS]);
        assert_eq!(d.extra_abil_times, [0i64; EXTRA_ABIL_SLOTS]);
        assert!(d.weapon.is_none());
        assert!(d.magic_list.is_empty());
        assert_eq!(
            UseItemActorState::default().extra,
            ItemsExtraState::default()
        );
    }

    /// **IT-08 非升级支**（StdMode3/Shape13 `GetExp(DuraMax)`）：吃掉、`WAbil.Exp += DuraMax`、
    /// `AddBodyLuck(DuraMax*0.002)`、`RM_WINEXP(nParam1=DuraMax)`、`RM_WEIGHTCHANGED`、背包 -1。
    /// **升级支未覆盖**（`Exp>=MaxExp → HasLevelUp 链`，登记 T-17）⇒ 本用例取 `Exp+DuraMax < MaxExp`。
    #[test]
    fn it08_stdmode3_shape13_getexp_nonlevelup() {
        let mut catalog = StdItemCatalog::new();
        catalog.insert(
            70,
            StdItem {
                name: "经验卷".into(),
                std_mode: 3,
                shape: 13,
                weight: 1,
                dura_max: 100,
                ..Default::default()
            },
        );
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &catalog,
            can_use_item: true,
            dead: false,
            actor_id: 9,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut state = UseItemActorState::default();
        state.wabil.exp = 0;
        state.wabil.max_exp = 10000;
        state.wabil.level = 5;
        let mut item_list = vec![BagItem {
            make_index: 1,
            index: 70,
            dura: 0,
            dura_max: 100,
            desc: [0; 14],
        }];
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten, "经验卷吃掉");
        assert_eq!(
            out.exp_delta,
            Some(100),
            "GetExp: exp_delta = DuraMax（交宿主落基础 Abil.Exp）"
        );
        assert_eq!(
            state.wabil.exp, 0,
            "use_item **不改 WAbil.Exp**（oracle GetExp 后 WAbil 陈旧，仅 AbilCopyToWAbil 同步）"
        );
        assert_eq!(state.wabil.level, 5, "非升级支：等级不变");
        assert!(
            (state.extra.body_luck - 0.2).abs() < 1e-9,
            "AddBodyLuck(DuraMax*0.002)"
        );
        assert_eq!(state.extra.body_luck_level, 0);
        let win = out
            .notices
            .iter()
            .find(|n| n.ident == RM_WINEXP)
            .expect("RM_WINEXP");
        assert_eq!(win.n_param1, 100, "RM_WINEXP.nParam1 = dwExp = DuraMax");
        assert!(out.notices.iter().any(|n| n.ident == RM_WEIGHTCHANGED));
        assert!(item_list.is_empty(), "经验卷被吃掉");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
    }

    /// Z4 gate：golden 非零 ⇒ Rust 必须非零；golden 全零 ⇒ 不适用（合法全零失败例豁免）。
    fn z4_ok(golden_nonzero: bool, rust_nonzero: bool) -> bool {
        !golden_nonzero || rust_nonzero
    }

    /// Z4 专用负例：**golden 非零而 Rust 全零 ⇒ 红**；golden 全零 ⇒ 不适用（豁免）。
    #[test]
    fn z4_golden_nonzero_rust_zero_red() {
        assert!(!z4_ok(true, false), "golden 非零而 Rust 全零必须红");
        assert!(z4_ok(true, true), "两侧非零 ⇒ 合格");
        assert!(z4_ok(false, false), "golden 全零 ⇒ 不适用（合法失败例）");
        assert!(has_nonzero_anchor(&success_case(&fixture())));
    }

    /// Z5 专用负例：只比一层（SM 帧有、RM/状态都无）不得判绿；真实夹具两层都有。
    #[test]
    fn z5_single_layer_red() {
        let single = serde_json::json!({
            "cases": [{
                "name": "sm_only",
                "wabil_before": {"hp": 1},
                "item_list_before": [],
                "inc_health_before": 0,
                "inc_spell_before": 0,
                "hunger_status_before": 0,
                "user_unlock_durg_before": false,
                "expect": {
                    "immediate_frames": [{"ident": 636}],
                    "notices": [], "added_items": [], "rng": [],
                    "wabil": {"hp": 1}, "item_list": [],
                    "inc_health": 0, "inc_spell": 0,
                    "hunger_status": 0, "user_unlock_durg": false
                }
            }]
        });
        assert!(
            !fixture_covers_two_layers(&single),
            "只有 SM 一层必须不合格"
        );
        assert!(
            fixture_covers_two_layers(&fixture()),
            "真实夹具必须两层都有"
        );
    }

    /// Z6 牙③：SM 帧改字段必须红（帧面也进比较面）。
    #[test]
    fn tooth_immediate_frame_changed_red() {
        let case = success_case(&fixture());
        let want = frames(&case["expect"]["immediate_frames"]);
        let mut got = want.clone();
        got[0].ident += 1;
        assert_ne!(got, want, "SM 帧改字段必须红");
    }

    fn recalc_inputs_default<'a>(abil: &'a Ability, bag: &'a [u16]) -> RecalcInputs<'a> {
        const BONUS: NakedAbility = NakedAbility {
            dc: 0,
            mc: 0,
            sc: 0,
            ac: 0,
            mac: 0,
            hp: 0,
            mp: 0,
            hit: 0,
            speed: 0,
            reserved: 0,
        };
        RecalcInputs {
            job: 0,
            abil,
            bonus_abil: &BONUS,
            bonus_tick_hit: 1,
            bonus_tick_speed: 1,
            cur_hp: abil.hp,
            cur_mp: abil.mp,
            use_items: &[None; USE_ITEMS_LEN],
            bag,
        }
    }

    /// 每条用例：即时帧、延时通知、`ItemList`、`WAbil`、`IncHealth`/`IncSpell`/`UserUnLockDurg`
    /// 都必须与 oracle 逐字段相同。
    #[test]
    fn matches_oracle_eat_fixture() {
        let doc = fixture();
        let cat = catalog(&doc);
        let cases = cases_or_red(&doc).unwrap_or_else(|e| panic!("{e}"));
        // 夹具记录的解包表（Shape → 解包物名）；无该表 = 空表。
        let unbind = unbind_from(&doc);
        assert!(
            fixture_covers_two_layers(&doc),
            "Z5：夹具必须至少一例同时覆盖 SM 帧与 RM/状态两层"
        );
        let mut z4_applicable = 0usize;
        let mut z4_exempt_names: Vec<String> = Vec::new();
        let mut one_layer_names: Vec<String> = Vec::new();
        let mut host_skipped = 0usize;
        for case in cases {
            let name = case["name"].as_str().unwrap();
            // IT-08 升级支：`host_levelup` 用例由宿主级用例（真实 `Operate` 下行对拍 pumped SM 帧）覆盖，
            // 纯 `use_item` 比较器跳过；但必须显式——被跳用例须 host_levelup 且 pumped 非空（有 oracle SM 帧）。
            if case["expect"]["host_levelup"].as_bool().unwrap_or(false) {
                assert!(
                    case["expect"]["pumped"]
                        .as_array()
                        .map(|a| !a.is_empty())
                        .unwrap_or(false),
                    "{name}: host_levelup 用例必须有 pumped oracle SM 帧"
                );
                host_skipped += 1;
                continue;
            }
            let c = &case["ctx"];
            let mut item_list = bag_items(&case["item_list_before"]);
            let item_list_before = item_list.clone();
            let abil = ability(&case["wabil_before"]);
            let bag: Vec<u16> = item_list.iter().map(|it| it.index).collect();
            let recalc = recalc_inputs_default(&abil, &bag);
            let ctx = UseItemContext {
                catalog: &cat,
                can_use_item: c["can_use_item"].as_bool().unwrap(),
                dead: c["dead"].as_bool().unwrap(),
                actor_id: i32v(&c["actor_id"]),
                make_index: i32v(&case["request"]["make_index"]),
                no_drug: c["no_drug"].as_bool().unwrap(),
                function_npc_present: c["function_npc_present"].as_bool().unwrap(),
                unbind: &unbind,
                red_msg_fcolor: u16v(&c["red_msg_fcolor"]),
                red_msg_bcolor: u16v(&c["red_msg_bcolor"]),
                recalc: Some(recalc),
            };
            let mut state = UseItemActorState {
                extra: Default::default(),
                wabil: abil.clone(),
                inc_health: u16v(&case["inc_health_before"]),
                inc_spell: u16v(&case["inc_spell_before"]),
                user_unlock_durg: case["user_unlock_durg_before"].as_bool().unwrap(),
                hunger_status: i32v(&case["hunger_status_before"]),
            };

            let mut item_number = 0;
            state.extra.body_luck = case["expect"]["body_luck_before"].as_f64().unwrap_or(0.0);
            state.extra.body_luck_level = case["expect"]["body_luck_level_before"]
                .as_i64()
                .unwrap_or(0) as i32;
            let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
            let expect = &case["expect"];

            // IT-08：基础 `Abil.Exp` 增量（fixture `abil_exp` vs 顶层 `abil_exp_before`）与 `BodyLuck` 终值（追加字段）。
            if let (Some(ab), Some(aa)) = (
                case["abil_exp_before"].as_i64(),
                expect["abil_exp"].as_i64(),
            ) {
                if aa != ab {
                    assert_eq!(
                        out.exp_delta,
                        Some((aa - ab) as i32),
                        "{name}: exp_delta = 基础 Abil.Exp 增量"
                    );
                } else {
                    assert_eq!(out.exp_delta, None, "{name}: 非 GetExp ⇒ exp_delta=None");
                }
            }
            if let Some(bl) = expect["body_luck"].as_f64() {
                assert!(
                    (state.extra.body_luck - bl).abs() < 1e-9,
                    "{name}: BodyLuck 终值与 oracle 不符"
                );
            }
            if let Some(bll) = expect["body_luck_level"].as_i64() {
                assert_eq!(
                    i64::from(state.extra.body_luck_level),
                    bll,
                    "{name}: BodyLuckLevel 终值与 oracle 不符"
                );
            }

            // Z4：golden 有非零期望 ⇒ Rust 必须有非零锚；golden 全零（合法失败例）⇒ 不适用。
            let golden_nonzero = has_nonzero_anchor(case);
            let rust_nonzero = !out.immediate_frames.is_empty()
                || !out.notices.is_empty()
                || !out.added_items.is_empty()
                || state.wabil != abil
                || item_list != item_list_before;
            if golden_nonzero {
                z4_applicable += 1;
            } else {
                z4_exempt_names.push(name.to_string());
            }
            if !case_two_layer(case) {
                one_layer_names.push(name.to_string());
            }
            assert!(
                z4_ok(golden_nonzero, rust_nonzero),
                "{name}: Z4 golden 非零但 Rust 全零"
            );

            // 即时帧 = 解包产物逐件 `SM_ADDITEM(200)`（`MakeMessage(SM_ADDITEM, ActorId, 0, 0, 1)`）
            // 在前，再 `SM_EAT_OK/636`。Rust 把解包件放在 `added_items`，这里按同一顺序拼回帧序。
            let mut rust_frames: Vec<Frame> = out
                .added_items
                .iter()
                .map(|_| Frame {
                    ident: 200,
                    recog: i32v(&c["actor_id"]),
                    param: 0,
                    tag: 0,
                    series: 1,
                })
                .collect();
            rust_frames.extend(out.immediate_frames.iter().cloned());
            assert_eq!(
                rust_frames,
                frames(&expect["immediate_frames"]),
                "{name}: 即时帧(含 SM_ADDITEM)与 oracle 不符"
            );
            // 通知：七字段 (ident,wParam,p1,p2,p3,priority,late) 按 (priority,ident,params,late) 排序后逐位比；
            // items 线的通知都走 `SendMsg`（`SendUpdateMsg` 亦汇到它）⇒ priority=(byte)ident、late=false，
            // Rust 据此派生；夹具记录 oracle 的入队键与 `LateDelivery`。`Vec` 相等即多重集（重复计数）。
            let mut got: Vec<(i32, i32, i32, i32, i32, i32, bool)> = out
                .notices
                .iter()
                .map(|n| {
                    (
                        n.ident & 0xFF,
                        n.ident,
                        n.w_param,
                        n.n_param1,
                        n.n_param2,
                        n.n_param3,
                        false,
                    )
                })
                .collect();
            let mut want: Vec<(i32, i32, i32, i32, i32, i32, bool)> = expect["notices"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| {
                    (
                        i32v(&v["priority"]),
                        i32v(&v["ident"]),
                        i32v(&v["w_param"]),
                        i32v(&v["n_param1"]),
                        i32v(&v["n_param2"]),
                        i32v(&v["n_param3"]),
                        v["late"].as_u64().unwrap_or(0) != 0,
                    )
                })
                .collect();
            got.sort();
            want.sort();
            assert_eq!(got, want, "{name}: 延时通知(七字段排序序列)与 oracle 不符");
            // 报告同 priority 内是否多 ident（供裁定是否需要单列同优先级内序）。
            let mut by_prio: std::collections::BTreeMap<i32, std::collections::BTreeSet<i32>> =
                std::collections::BTreeMap::new();
            for (prio, ident, ..) in &got {
                by_prio.entry(*prio).or_default().insert(*ident);
            }
            for (prio, idents) in &by_prio {
                if idents.len() > 1 {
                    eprintln!("[{name}] 同 priority={prio} 内多 ident: {idents:?}");
                }
            }
            // D1/D2/D3：抽空未命中上界且 MaxDrainPasses=8；`hit_cap=true` ⇒ 必红（不得当完整账本）。
            let hit_cap = expect["drain"]["hit_cap"].as_bool().unwrap_or(false);
            let max_passes = expect["drain"]["max_passes"].as_u64().unwrap_or(0);
            assert!(
                drain_ok(hit_cap, max_passes),
                "{name}: 抽空上限异常 hit_cap={hit_cap} max_passes={max_passes}"
            );
            // 解包新建物品：结构化逐字段（`make_index` 含 `Environment.TickCount` 墙钟 ⇒ 只核递增不核绝对值）。
            let exp_added = expect
                .get("added_items")
                .and_then(Value::as_array)
                .cloned()
                .unwrap_or_default();
            assert_eq!(
                out.added_items.len(),
                exp_added.len(),
                "{name}: added_items 条数与 oracle 不符"
            );
            let mut last_mi = i32::MIN;
            for (a, e) in out.added_items.iter().zip(exp_added.iter()) {
                assert_eq!(a.index, u16v(&e["index"]), "{name}: added.index");
                assert_eq!(a.dura, u16v(&e["dura"]), "{name}: added.dura");
                assert_eq!(a.dura_max, u16v(&e["dura_max"]), "{name}: added.dura_max");
                let e_desc: Vec<u8> = e["desc"].as_array().unwrap().iter().map(u8v).collect();
                assert_eq!(a.desc.to_vec(), e_desc, "{name}: added.desc");
                assert!(a.make_index > last_mi, "{name}: added.make_index 必须递增");
                last_mi = a.make_index;
            }
            assert_eq!(
                out.eaten,
                expect["immediate_frames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| u16v(&f["ident"]) == SM_EAT_OK),
                "{name}: eaten ↔ SM_EAT_OK 存在性"
            );
            // 背包终态：逐件比。解包追加的尾部件 `make_index` 是墙钟派生 ⇒ 只比 (index,dura,dura_max,desc)。
            let exp_items = bag_items(&expect["item_list"]);
            assert_eq!(item_list.len(), exp_items.len(), "{name}: ItemList 长度");
            let k = out.added_items.len();
            let split = item_list.len() - k;
            assert_eq!(
                &item_list[..split],
                &exp_items[..split],
                "{name}: ItemList 前段与 oracle 不符"
            );
            for (a, e) in item_list[split..].iter().zip(exp_items[split..].iter()) {
                assert_eq!(
                    (a.index, a.dura, a.dura_max, a.desc),
                    (e.index, e.dura, e.dura_max, e.desc),
                    "{name}: 解包件字段与 oracle 不符（make_index 墙钟排除）"
                );
            }
            assert_eq!(
                state.wabil,
                ability(&expect["wabil"]),
                "{name}: WAbil 与 oracle 不符"
            );
            assert_eq!(
                i32::from(state.inc_health),
                i32v(&expect["inc_health"]),
                "{name}: IncHealth"
            );
            assert_eq!(
                i32::from(state.inc_spell),
                i32v(&expect["inc_spell"]),
                "{name}: IncSpell"
            );
            assert_eq!(
                state.user_unlock_durg,
                expect["user_unlock_durg"].as_bool().unwrap(),
                "{name}: UserUnLockDurg"
            );
            assert_eq!(
                state.hunger_status,
                i32v(&expect["hunger_status"]),
                "{name}: HungerStatus"
            );
        }
        let total = cases.len();
        assert!(
            host_skipped >= 1,
            "至少一个 host_levelup 用例应由宿主级用例覆盖（纯比较器当前跳过 {host_skipped} 例）"
        );
        let compared = total - host_skipped;
        // Z4 计数式 + 不适用名单（断言，非打印）。
        assert_eq!(
            z4_applicable + z4_exempt_names.len(),
            compared,
            "Z4 计数式：适用 + 不适用 == 被比较例数（排除 host_levelup）"
        );
        let mut z4_ex: Vec<&str> = z4_exempt_names.iter().map(String::as_str).collect();
        z4_ex.sort_unstable();
        let mut z4_expect: Vec<&str> = EXPECTED_Z4_EXEMPT.to_vec();
        z4_expect.sort_unstable();
        assert_eq!(
            z4_ex.as_slice(),
            z4_expect.as_slice(),
            "Z4 不适用名单漂移：须随夹具变更在同一次提交更新并写进提交信息"
        );
        // Z5 计数式 + 豁免名单（断言，非打印）。
        let two_layer = compared - one_layer_names.len();
        assert_eq!(
            two_layer + one_layer_names.len(),
            compared,
            "Z5 计数式：两层 + 豁免 == 被比较例数（排除 host_levelup）"
        );
        let mut ol: Vec<&str> = one_layer_names.iter().map(String::as_str).collect();
        ol.sort_unstable();
        let mut ol_expect: Vec<&str> = EXPECTED_ONE_LAYER.to_vec();
        ol_expect.sort_unstable();
        assert_eq!(
            ol.as_slice(),
            ol_expect.as_slice(),
            "Z5 豁免名单漂移：须随夹具变更在同一次提交更新并写进提交信息"
        );
        eprintln!(
            "Z4：本批 {total} 例中 {z4_applicable} 例适用、{} 例不适用；Z5：本批 {two_layer} 例带两层、{} 例豁免（计数式与名单已断言）",
            z4_exempt_names.len(),
            one_layer_names.len()
        );
    }

    /// Shape 3 的百分比是 `Round(...AwayFromZero)`；夹具的 `max_hp=37/ac=5`（1.85→2）与
    /// `max_mp=25/mac=10`（**2.5→3**，银行家舍入得 2）钉住「半数远离零」这一口径
    /// （改截断/银行家舍入必红；旧值 5.5 两种舍入都得 6，无区分力）。
    #[test]
    fn shape3_percent_rounds_away_from_zero() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_ok_shape3_percent")
            .expect("夹具必须含 eat_ok_shape3_percent");
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten);
        assert_eq!(state.wabil.hp, 12, "37/100*5 = 1.85 ⇒ +2");
        assert_eq!(
            state.wabil.mp, 13,
            "25/100*10 = 2.5 ⇒ +3（半数远离零；银行家舍入=2）"
        );
    }

    /// `BoCanUseItem == false`：只排一条 `RM_MENU_OK`（`nParam1 = ActorId`）并落 `SM_EAT_FAIL`，
    /// **不**动背包（改坏守卫必红）。
    #[test]
    fn can_not_use_item_still_menus_then_fails() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_fail_can_use_item_false")
            .expect("夹具必须含 eat_fail_can_use_item_false");
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: false,
            dead: false,
            actor_id: 0x1234,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(!out.eaten);
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert_eq!(out.notices.len(), 1);
        assert_eq!(out.notices[0].ident, RM_MENU_OK);
        assert_eq!(
            out.notices[0].n_param1, 0x1234,
            "RM_MENU_OK.nParam1 = ActorId"
        );
        assert_eq!(item_list, before, "guard 拒绝时不许动背包");
    }

    /// `StdMode=5`（`ClientUseItems` 的 `case 0/1/2/3/4/31` 名单**外**）：两侧一致地不吃
    /// （落 `SM_EAT_FAIL`、无通知、背包不变）。
    ///
    /// ⚠️ 本用例**只**锤定"switch 外"这一形态；`StdMode 3/4/31` 是**已证差异**
    /// （oracle 在 StdMode 3 Shape 13 上**无条件**吃掉 + `SM_EAT_OK`，本接缝 `SM_EAT_FAIL`），
    /// 登记在 whitelist 的 T-17。
    #[test]
    fn unimplemented_stdmode_does_not_eat() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_fail_stdmode_other")
            .expect("夹具必须含 eat_fail_stdmode_other");
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(!out.eaten);
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert!(out.notices.is_empty());
        assert_eq!(item_list, before);
    }

    /// `StdMode 2`（`PlayObject.Base.cs:2095`）：oracle `result = true` ⇒ 吃掉物品 + `SM_EAT_OK` +
    /// `RM_WEIGHTCHANGED`，**没有任何属性/用药状态变更**（改回「不吃」⇒ 必红）。
    #[test]
    fn stdmode2_eats_without_effect() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_ok_stdmode2_noop")
            .expect("夹具必须含 eat_ok_stdmode2_noop");
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 3,
            inc_spell: 4,
            user_unlock_durg: true,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten, "StdMode 2 必吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
        assert_eq!(out.notices, vec![QueuedNotice::plain(RM_WEIGHTCHANGED)]);
        assert!(item_list.is_empty(), "StdMode 2 吃掉后背包为空");
        assert_eq!(state.inc_health, 3, "StdMode 2 不许动 IncHealth");
        assert_eq!(state.inc_spell, 4, "StdMode 2 不许动 IncSpell");
        assert!(state.user_unlock_durg, "StdMode 2 不许动 UserUnLockDurg");
    }

    /// `Envir.Flag.boNODRUG == true`：`SysMsg(Red)` 入 MsgQueue（`RM_SYSMESSAGE`，颜色取自
    /// `Config.RedMsgFColor/BColor`）、物品不消耗、无 `SM_EAT_OK`（只落 `SM_EAT_FAIL`）。
    /// 改坏颜色对（f/b 互换）或去掉守卫（照吃）⇒ 必红。
    #[test]
    fn nodrug_guard_does_not_consume() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_fail_nodrug_no_consume")
            .expect("夹具必须含 eat_fail_nodrug_no_consume");
        let c = &case["ctx"];
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: c["actor_id"].as_i64().unwrap() as i32,
            make_index: 1,
            no_drug: c["no_drug"].as_bool().unwrap(),
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: u16v(&c["red_msg_fcolor"]),
            red_msg_bcolor: u16v(&c["red_msg_bcolor"]),
            recalc: None,
        };
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(!out.eaten, "禁药地图不许吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert_eq!(out.notices.len(), 1, "禁药只排一条 SysMsg");
        assert_eq!(out.notices[0].ident, RM_SYSMESSAGE);
        assert_eq!(out.notices[0].n_param1, 255, "前景色 = RedMsgFColor");
        assert_eq!(out.notices[0].n_param2, 56, "背景色 = RedMsgBColor");
        assert_eq!(item_list, before, "禁药不许消耗物品");
    }

    /// `StdMode 31`（`PlayObject.Operate.cs:657-672`）：`AniCount != 0` 时是否吃掉**只**取决于
    /// `SystemShare.FunctionNPC != null`（`UseStdModeFunItem`，`PlayObject.Base.cs:1072`）。
    /// 同一用例构造两次：存在 ⇒ `SM_EAT_OK` + `RM_WEIGHTCHANGED` + 背包移除；缺失 ⇒ `SM_EAT_FAIL`
    /// + 背包不动。把 `&& ctx.function_npc_present` 删掉（或无脑 `eaten = true`）⇒ 后半必红。
    #[test]
    fn stdmode31_funitem_eats_only_when_function_npc_present() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_ok_stdmode31_funitem")
            .expect("夹具必须含 eat_ok_stdmode31_funitem");
        let abil = ability(&case["wabil_before"]);

        // 存在 FunctionNPC ⇒ 吃。
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: abil.clone(),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let unbind = no_unbind();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: true,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten, "FunctionNPC 存在 ⇒ StdMode 31/AniCount!=0 必吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
        assert_eq!(out.notices, vec![QueuedNotice::plain(RM_WEIGHTCHANGED)]);
        assert!(item_list.is_empty(), "吃掉后背包为空");

        // 缺失 FunctionNPC ⇒ 不吃（`UseStdModeFunItem` 返回 false）。
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: abil.clone(),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let unbind = no_unbind();
        let ctx = UseItemContext {
            function_npc_present: false,
            unbind: &unbind,
            ..ctx
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(!out.eaten, "FunctionNPC 缺失 ⇒ 不吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert!(out.notices.is_empty());
        assert_eq!(item_list, before, "拒绝时不许动背包");
    }

    /// **卡 `openmir2-unbindlist-consumer`（StdMode 31 / AniCount == 0 解包）**：
    /// `UnbindList` 命中 ⇒ 吃掉捆装包并按 `CopyToUserItemFromName` 生成 **6 件**
    /// （`PlayObject.Operate.cs:658-666` / `:582-599`）：`MakeIndex` 逐件取自会话计数器、
    /// `Dura = DuraMax = tpl.DuraMax`；重量按**解包后**的背包重算（`WeightChanged()`，`:676`）。
    ///
    /// 牙：删 `31 =>` 解包臂（回落 `_ => {}`）⇒ `eaten` 为 false、背包不变 ⇒ 必红。
    #[test]
    fn unbind_stdmode31_creates_six_items_and_reweights() {
        let mut catalog = StdItemCatalog::new();
        catalog.insert(
            50,
            StdItem {
                name: "捆装包".into(),
                std_mode: 31,
                shape: 5,
                weight: 1,
                ani_count: 0,
                ..Default::default()
            },
        );
        catalog.insert(
            60,
            StdItem {
                name: "解包物".into(),
                std_mode: 0,
                weight: 2,
                dura_max: 7,
                ..Default::default()
            },
        );
        let mut unbind = UnbindList::default();
        unbind.map.insert(5, "解包物".to_string());
        let ctx = UseItemContext {
            catalog: &catalog,
            can_use_item: true,
            dead: false,
            actor_id: 7,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut state = UseItemActorState::default();
        let mut item_list = vec![BagItem {
            make_index: 1,
            index: 50,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }];
        let mut item_number = 100;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten, "容量门通过 ⇒ 吃掉捆装包");
        assert_eq!(out.removed_index, Some(0));
        assert_eq!(
            out.added_items.len(),
            6,
            "`ClientUseItemsGetUnBindItems(name, 6)`"
        );
        assert_eq!(
            out.added_items
                .iter()
                .map(|i| i.make_index)
                .collect::<Vec<_>>(),
            vec![101, 102, 103, 104, 105, 106],
            "`CopyToUserItemFromName` 每件取一次 `GetItemNumber()`"
        );
        assert!(
            out.added_items
                .iter()
                .all(|i| i.index == 60 && i.dura == 7 && i.dura_max == 7),
            "`Index = 表内位置+1`、`Dura = DuraMax = tpl.DuraMax`"
        );
        assert_eq!(item_number, 106, "会话计数器被 6 件各消费一次");
        assert_eq!(item_list.len(), 6, "移除 1 件 + 生成 6 件");
        assert!(item_list.iter().all(|i| i.index == 60));
        assert_eq!(state.wabil.weight, 12, "重量按解包后的背包重算（6 × 2）");
        assert_eq!(
            out.immediate_frames,
            vec![Frame {
                ident: SM_EAT_OK,
                recog: 0,
                param: 0,
                tag: 0,
                series: 0,
            }]
        );
        assert_eq!(out.notices, vec![QueuedNotice::plain(RM_WEIGHTCHANGED)]);
    }

    /// **IT-18 对拍样板**：夹具 `eat_ok_stdmode31_unbind_success`（oracle 实测）逐字段复现——
    /// 容量门通过 ⇒ 吃掉捆装包、按 `UnbindList[Shape]` 建 6 件、6 条 `SM_ADDITEM(200)` 在前再
    /// `SM_EAT_OK`、`RM_WEIGHTCHANGED`、重量按解包后背包重算。`make_index` 墙钟派生 ⇒ 只核递增。
    ///
    /// 牙（改实现关键点必红）：①`for _ in 0..6` 改 `0..5` ⇒ 件数断言红；②`dura: tpl_unbind.dura_max`
    /// 改 `0` ⇒ 字段断言红。
    #[test]
    fn unbind_stdmode31_success_matches_oracle_fixture() {
        let doc = fixture();
        let cat = catalog(&doc);
        let case = doc["cases"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["name"] == "eat_ok_stdmode31_unbind_success")
            .expect("夹具必须含 eat_ok_stdmode31_unbind_success（IT-18 成功路径）");
        let unbind = unbind_from(&doc);
        let mut state = UseItemActorState {
            extra: Default::default(),
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: i32v(&case["ctx"]["actor_id"]),
            make_index: i32v(&case["request"]["make_index"]),
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        let expect = &case["expect"];
        assert!(out.eaten, "容量门通过 ⇒ 吃掉捆装包");
        let exp_added = expect["added_items"].as_array().unwrap();
        assert_eq!(out.added_items.len(), 6, "解包 6 件");
        let mut last_mi = i32::MIN;
        for (a, e) in out.added_items.iter().zip(exp_added.iter()) {
            assert_eq!(a.index, u16v(&e["index"]), "解包件 index");
            assert_eq!(a.dura, u16v(&e["dura"]), "解包件 dura = tpl.DuraMax");
            assert_eq!(a.dura_max, u16v(&e["dura_max"]), "解包件 dura_max");
            assert!(a.make_index > last_mi, "make_index 递增");
            last_mi = a.make_index;
        }
        // 即时帧：解包产物逐件 SM_ADDITEM(200) 在前，再 SM_EAT_OK。
        let mut frames_seq: Vec<u16> = std::iter::repeat_n(200u16, out.added_items.len()).collect();
        frames_seq.extend(out.immediate_frames.iter().map(|f| f.ident));
        let exp_seq: Vec<u16> = frames(&expect["immediate_frames"])
            .iter()
            .map(|f| f.ident)
            .collect();
        assert_eq!(frames_seq, exp_seq, "SM_ADDITEM×6 必须排在 SM_EAT_OK 之前");
        assert_eq!(out.notices, vec![QueuedNotice::plain(RM_WEIGHTCHANGED)]);
        assert_eq!(
            state.wabil.weight,
            ability(&expect["wabil"]).weight,
            "重量按解包后背包重算"
        );
        assert_eq!(item_list.len(), 6, "移除 1 件 + 生成 6 件");
    }

    /// **未命中 Shape ⇒ 空串、0 件**（`ClientUseItemsGetUnbindItemName` 返回 `string.Empty`，
    /// `CopyToUserItemFromName("")` 恒 false ⇒ break）：**捆装包照吃、`SM_EAT_OK` 照发**，
    /// 背包净少 1 件、无新增（不是「什么都不发生」——oracle 的 `eatSuccess = true` 在解包调用之后）。
    ///
    /// 牙：把 `eaten = true` 挪进「added > 0」的条件里 ⇒ 本用例 `eaten` 断言红。
    #[test]
    fn unbind_missing_shape_eats_pack_but_creates_nothing() {
        let mut catalog = StdItemCatalog::new();
        catalog.insert(
            50,
            StdItem {
                name: "捆装包".into(),
                std_mode: 31,
                shape: 5,
                weight: 1,
                ani_count: 0,
                ..Default::default()
            },
        );
        let unbind = UnbindList::default();
        let ctx = UseItemContext {
            catalog: &catalog,
            can_use_item: true,
            dead: false,
            actor_id: 7,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut state = UseItemActorState::default();
        let mut item_list = vec![BagItem {
            make_index: 1,
            index: 50,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }];
        let mut item_number = 0;
        let out = use_item(&ctx, &mut state, &mut item_list, &mut item_number);
        assert!(out.eaten, "解包调用不改变 eatSuccess（Operate.cs:665）");
        assert!(out.added_items.is_empty(), "空名 ⇒ 0 件");
        assert!(item_list.is_empty(), "捆装包被吃掉");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
        assert_eq!(item_number, 0, "没建件 ⇒ 计数器不动");

        // 另一半：Shape 命中但**名字查不到模板** ⇒ 同样 0 件、照吃。
        let mut unbind2 = UnbindList::default();
        unbind2.map.insert(5, "不存在的物品".to_string());
        let ctx2 = UseItemContext {
            unbind: &unbind2,
            ..ctx
        };
        let mut item_list2 = vec![BagItem {
            make_index: 1,
            index: 50,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        }];
        let out2 = use_item(&ctx2, &mut state, &mut item_list2, &mut item_number);
        assert!(out2.eaten);
        assert!(out2.added_items.is_empty());
        assert!(item_list2.is_empty());
    }

    /// **容量门取「移除前」的 `ItemList.Count`**（`PlayObject.Operate.cs:659`）：
    /// `Count + 6 - 1 <= 46` ⇔ `Count <= 41`。42 件 ⇒ 不吃（`SM_EAT_FAIL`，背包不动）；
    /// 41 件 ⇒ 吃，移除 1 + 新增 6。
    ///
    /// 牙：把容量门改成 `item_list.len() + 6 <= MAX_BAG_ITEM`（少减 1）⇒ 41 件那半变不吃 ⇒ 红。
    #[test]
    fn unbind_capacity_gate_uses_pre_removal_count() {
        let mut catalog = StdItemCatalog::new();
        catalog.insert(
            50,
            StdItem {
                name: "捆装包".into(),
                std_mode: 31,
                shape: 5,
                weight: 1,
                ani_count: 0,
                ..Default::default()
            },
        );
        catalog.insert(
            60,
            StdItem {
                name: "解包物".into(),
                std_mode: 0,
                weight: 1,
                dura_max: 7,
                ..Default::default()
            },
        );
        catalog.insert(
            70,
            StdItem {
                name: "填充".into(),
                std_mode: 0,
                weight: 1,
                ..Default::default()
            },
        );
        let mut unbind = UnbindList::default();
        unbind.map.insert(5, "解包物".to_string());
        let ctx = UseItemContext {
            catalog: &catalog,
            can_use_item: true,
            dead: false,
            actor_id: 7,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            unbind: &unbind,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let pack = BagItem {
            make_index: 1,
            index: 50,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        };
        let filler = |i: i32| BagItem {
            make_index: 100 + i,
            index: 70,
            dura: 0,
            dura_max: 0,
            desc: [0; 14],
        };
        let mut state = UseItemActorState::default();
        let mut item_number = 0;

        // 42 件：42 + 6 - 1 = 47 > 46 ⇒ 容量门不通过。
        let mut full: Vec<BagItem> = vec![pack.clone()];
        full.extend((0..41).map(filler));
        let before = full.clone();
        let out = use_item(&ctx, &mut state, &mut full, &mut item_number);
        assert!(
            !out.eaten,
            "42 件超容量 ⇒ 不吃（与夹具 unbind_bag_full 同口径）"
        );
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert!(out.added_items.is_empty());
        assert_eq!(full, before, "拒绝时不许动背包");
        assert_eq!(item_number, 0);

        // 41 件：41 + 6 - 1 = 46 ≤ 46 ⇒ 通过。
        let mut fits: Vec<BagItem> = vec![pack];
        fits.extend((0..40).map(filler));
        let out = use_item(&ctx, &mut state, &mut fits, &mut item_number);
        assert!(out.eaten, "41 件正好 46 ≤ MaxBagItem ⇒ 解包");
        assert_eq!(out.added_items.len(), 6);
        assert_eq!(fits.len(), 46, "移除 1 + 新增 6 ⇒ 46 件");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
    }
}
