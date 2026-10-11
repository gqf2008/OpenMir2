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
//!     │                             ——`AniCount != 0` 已做（看 FunctionNPC 是否存在），解包未做（T-17）
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
//! - **StdMode 31 / AniCount == 0（解包）**：oracle 在 `ItemList.Count + 6 - 1 <= Grobal2.MaxBagItem`（46）时
//!   解包出 6 件（`UnbindList`）；本接缝未实现 ⇒ 容量够时是已证差异（T-17），容量不足时两侧一致地不吃。
//!   `StdMode=5`（`case` 名单外）才是两侧一致地不吃。
//! - **StdMode 1 只补到 `HungerStatus` + `RefMyStatus` 的发帧**：oracle `RefMyStatus()` 里的
//!   `RecalcAbilitys()` **整链**（由装备/buff 算 `WAbil`）只用 `ability::recalc_abilitys` 的最小等价物
//!   顶上（`ctx.recalc`）；它**尚未**覆盖 `PlayObject.RecalcAbilitys` 的套装/`ExtraAbil`/`GetCharStatus`
//!   等尾段 ⇒ 有装备/buff 的跨档场景仍可能与 oracle 不同。已登记在 whitelist 的 **T-17**。
//!   接线前必须先把这些分支补齐。
//!   （注：`boNODRUG` 开关对 0/1/2/3 都在 `EatItems` 首部生效，故本接缝对 1/3 也做该守卫，
//!   但未禁药时 1/3 仍落 `SM_EAT_FAIL`。）
//! - **`GetUpgradeStdItem` 的 Desc 升级差量**：同 `bag.rs`/`ability.rs`，传入的模板视为「有效物品」。
//! - `ClientUseItems` 的 `sItemName` 形参在 oracle 里**未被使用**（只按 `MakeIndex` 找），已照搬。

use crate::ability::{recalc_abilitys, Ability, ItemCatalog, RecalcInputs};
use crate::bag::{BagItem, Frame};

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
    /// StdMode 31 的 `AniCount != 0` 分支用它；`AniCount == 0` 的解包分支未实现（T-17）。
    pub function_npc_present: bool,
    /// `Config.RedMsgFColor`（禁药 `SysMsg(Red)` 的前景色）。
    pub red_msg_fcolor: u16,
    /// `Config.RedMsgBColor`（禁药 `SysMsg(Red)` 的背景色）。
    pub red_msg_bcolor: u16,
    /// `RefMyStatus()` 里 `RecalcAbilitys()` 的观测输入（`PlayObject.cs:2008`）。StdMode 1 跨档才用到；
    /// `None` 时**不**复算 `WAbil`（只发 `RM_CHARSTATUSCHANGED`/`RM_MYSTATUS`）——见模块「未做清单」T-17。
    pub recalc: Option<RecalcInputs<'a>>,
}

/// 本动作会原地改的**演员侧**状态（`PlayObject` 上分散的字段；接缝把它们打包传入传出）。
#[derive(Debug, Clone, PartialEq, Eq)]
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
}

/// `ClientUseItems`（`PlayObject.Operate.cs:601`）的纯函数等价物——**本片只覆盖 StdMode 0**。
///
/// `item_list` 原地变更（吃掉则移除命中的那一件），`state` 原地变更（`WAbil`/`IncHealth`/`IncSpell`/
/// `UserUnLockDurg`）。返回的 `immediate_frames`/`notices` 由接线层按各自通道下发。
pub fn use_item(
    ctx: &UseItemContext<'_>,
    state: &mut UseItemActorState,
    item_list: &mut Vec<BagItem>,
) -> UseItemOutcome {
    let mut eaten = false;
    let mut removed_index: Option<usize> = None;
    let mut notices: Vec<QueuedNotice> = Vec::new();

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
                            //   - `AniCount == 0` ⇒ 解包（`ClientUseItemsGetUnBindItems`，`UnbindList` 出 6 件）：
                            //     未实现；容量不足时两侧一致地不吃，容量足够时是已证差异（T-17）。
                            31 if tpl.ani_count != 0 && ctx.function_npc_present => {
                                eaten = true;
                            }
                            // StdMode 3/4（ExtraAbil/GetExp、书）未实现 ⇒ 与 oracle 在未禁药形态下的
                            // 成功/条件成功路径**不同**，已证差异，登记在 T-17。
                            _ => {}
                        }
                        if eaten {
                            item_list.remove(i);
                            removed_index = Some(i);
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
    notices.sort_by_key(|n| n.ident as u8);

    UseItemOutcome {
        eaten,
        removed_index,
        immediate_frames,
        notices,
    }
}

/// `BaseObject.IncHealthSpell(int nHp, int nMp)`（`BaseObject.cs:952`）。
///
/// 返回 `false` ⇔ oracle 在开头的 `nHp<0 || nMp<0` 处 `return`（此时**不**调 `HealthSpellChanged`）。
/// `HP/MP` 用 `ushort` 语义（oracle `(ushort)nHp` 强转），超上限则夹到 `MaxHP/MaxMP`。
fn inc_health_spell(state: &mut UseItemActorState, n_hp: i32, n_mp: i32) -> bool {
    if n_hp < 0 || n_mp < 0 {
        return false;
    }
    if state.wabil.hp as i32 + n_hp >= state.wabil.max_hp as i32 {
        state.wabil.hp = state.wabil.max_hp;
    } else {
        state.wabil.hp = state.wabil.hp.wrapping_add(n_hp as u16);
    }
    if state.wabil.mp as i32 + n_mp >= state.wabil.max_mp as i32 {
        state.wabil.mp = state.wabil.max_mp;
    } else {
        state.wabil.mp = state.wabil.mp.wrapping_add(n_mp as u16);
    }
    true
}

/// `PlayObject.GetMyStatus()`（`PlayObject.Base.cs:1183-1191`）：`min(HungerStatus/1000, 4)`（整数除）。
fn my_status(hunger_status: i32) -> i32 {
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

    fn notice(v: &Value) -> QueuedNotice {
        QueuedNotice {
            ident: i32v(&v["ident"]),
            w_param: i32v(&v["w_param"]),
            n_param1: i32v(&v["n_param1"]),
            n_param2: i32v(&v["n_param2"]),
            n_param3: i32v(&v["n_param3"]),
        }
    }

    fn notices(v: &Value) -> Vec<QueuedNotice> {
        v.as_array().unwrap().iter().map(notice).collect()
    }

    /// 构造一条与探针同口径的 `RecalcAbilitys` 输入：`Abil` = 用例的 `wabil_before`（探针里
    /// `po.Abil = wabilPre`）、无装备、`bag` = 当前背包 `Index` 列表、无 buff。
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
        let cases = doc["cases"].as_array().expect("cases 数组");
        assert!(cases.len() >= 12, "夹具用例数异常：{}", cases.len());
        for case in cases {
            let name = case["name"].as_str().unwrap();
            let c = &case["ctx"];
            let mut item_list = bag_items(&case["item_list_before"]);
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
                red_msg_fcolor: u16v(&c["red_msg_fcolor"]),
                red_msg_bcolor: u16v(&c["red_msg_bcolor"]),
                recalc: Some(recalc),
            };
            let mut state = UseItemActorState {
                wabil: abil.clone(),
                inc_health: u16v(&case["inc_health_before"]),
                inc_spell: u16v(&case["inc_spell_before"]),
                user_unlock_durg: case["user_unlock_durg_before"].as_bool().unwrap(),
                hunger_status: i32v(&case["hunger_status_before"]),
            };

            let out = use_item(&ctx, &mut state, &mut item_list);
            let expect = &case["expect"];

            assert_eq!(
                out.immediate_frames,
                frames(&expect["immediate_frames"]),
                "{name}: 即时帧与 oracle 不符"
            );
            assert_eq!(
                out.notices,
                notices(&expect["notices"]),
                "{name}: 延时通知与 oracle 不符"
            );
            assert_eq!(
                out.eaten,
                expect["immediate_frames"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|f| u16v(&f["ident"]) == SM_EAT_OK),
                "{name}: eaten ↔ SM_EAT_OK 存在性"
            );
            assert_eq!(
                item_list,
                bag_items(&expect["item_list"]),
                "{name}: ItemList 与 oracle 不符"
            );
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
    }

    /// Shape 3 的百分比是 `Round(...AwayFromZero)`；夹具的 `max_hp=101/ac=5`（5.05→5）与
    /// `max_mp=55/mac=10`（5.5→**6**）钉住「半数进位」这一口径（改成截断/银行家舍入必红）。
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
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let out = use_item(&ctx, &mut state, &mut item_list);
        assert!(out.eaten);
        assert_eq!(state.wabil.hp, 55, "101/100*5 = 5.05 ⇒ +5");
        assert_eq!(state.wabil.mp, 46, "55/100*10 = 5.5 ⇒ +6（半数远离零）");
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
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: false,
            dead: false,
            actor_id: 0x1234,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let out = use_item(&ctx, &mut state, &mut item_list);
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
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let out = use_item(&ctx, &mut state, &mut item_list);
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
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: false,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let mut state = UseItemActorState {
            wabil: ability(&case["wabil_before"]),
            inc_health: 3,
            inc_spell: 4,
            user_unlock_durg: true,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let out = use_item(&ctx, &mut state, &mut item_list);
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
        let ctx = UseItemContext {
            catalog: &cat,
            can_use_item: true,
            dead: false,
            actor_id: c["actor_id"].as_i64().unwrap() as i32,
            make_index: 1,
            no_drug: c["no_drug"].as_bool().unwrap(),
            function_npc_present: false,
            red_msg_fcolor: u16v(&c["red_msg_fcolor"]),
            red_msg_bcolor: u16v(&c["red_msg_bcolor"]),
            recalc: None,
        };
        let mut state = UseItemActorState {
            wabil: ability(&case["wabil_before"]),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let out = use_item(&ctx, &mut state, &mut item_list);
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
            wabil: abil.clone(),
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
            actor_id: 0,
            make_index: 1,
            no_drug: false,
            function_npc_present: true,
            red_msg_fcolor: 255,
            red_msg_bcolor: 56,
            recalc: None,
        };
        let out = use_item(&ctx, &mut state, &mut item_list);
        assert!(out.eaten, "FunctionNPC 存在 ⇒ StdMode 31/AniCount!=0 必吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_OK);
        assert_eq!(out.notices, vec![QueuedNotice::plain(RM_WEIGHTCHANGED)]);
        assert!(item_list.is_empty(), "吃掉后背包为空");

        // 缺失 FunctionNPC ⇒ 不吃（`UseStdModeFunItem` 返回 false）。
        let mut state = UseItemActorState {
            wabil: abil.clone(),
            inc_health: 0,
            inc_spell: 0,
            user_unlock_durg: false,
            hunger_status: 0,
        };
        let mut item_list = bag_items(&case["item_list_before"]);
        let before = item_list.clone();
        let ctx = UseItemContext {
            function_npc_present: false,
            ..ctx
        };
        let out = use_item(&ctx, &mut state, &mut item_list);
        assert!(!out.eaten, "FunctionNPC 缺失 ⇒ 不吃");
        assert_eq!(out.immediate_frames[0].ident, SM_EAT_FAIL);
        assert!(out.notices.is_empty());
        assert_eq!(item_list, before, "拒绝时不许动背包");
    }
}
