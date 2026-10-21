//! R1 入站派发余 10 条（交付卡 `openmir2-inbound-dispatch-red-batch`，片 76）的**决策逻辑**纯函数接缝。
//!
//! # 为什么有本模块
//!
//! oracle `src/M2Server/Player/PlayObject.Message.cs` 的 `Operate` 里对以下 10 条 `CM_*` 有 `case`，
//! 而 Rust `bins/game-svr/src/lib.rs` 原先**只记日志即丢**（红名单 R1；`docs/移植计划重审-20261015.md` §1-R1）。
//! 本模块把这 10 条的 oracle 决策逐条搬成**无 I/O、无全局状态**的纯函数（与 `ground.rs`/`shop.rs` 同型），
//! 由 `lib.rs` 的派发臂接线。判据 tier = **L**（纯逻辑分支，无 RNG / 无落库 / 无时序；变异牙见各 `#[test]`）。
//!
//! # oracle 锚（逐条）
//!
//! | Rust | oracle `case` | oracle 实现 |
//! | --- | --- | --- |
//! | [`client_open_door`] | `PlayObject.Message.cs:817` | `PlayObject.Operate.cs:363-374` + `WorldServer.cs:1445-1452` |
//! | [`client_butch`] | `PlayObject.Message.cs:829-872` | `PlayObject.Operate.cs:702-751` |
//! | [`client_drop_gold`] | `PlayObject.Message.cs:903-908` | `PlayObject.Operate.cs:222-249` + `BaseObject.cs:735-771` |
//! | [`client_test`] | `PlayObject.Message.cs:909-911` | 内联 `SendDefMessage(1,0,0,0,0)` |
//! | [`client_get_min_map`] | `PlayObject.Message.cs:969-971` | `PlayObject.Operate.cs:1204-1214` |
//! | [`client_make_drug_item`] | `PlayObject.Message.cs:972-974` | `PlayObject.Operate.cs:1216-1227` |
//! | [`client_1042`] | `PlayObject.Message.cs:996-998` | 内联 `LogService.Warn("[非法数据] "+ChrName)` |
//! | [`client_adjust_bonus`] | `PlayObject.Message.cs:999-1001` | `PlayObject.Base.cs:1156-1180` |
//! | [`client_sit_down_hit`] | `PlayObject.Message.cs:1284-1338` | `PlayObject.Operate.cs:343-360` |
//! | [`process_client_password`] | `PlayObject.Message.cs:1397-1399` | `PlayObject.Base.cs:3190-3298` |
//!
//! # BUG-REPLICA（照搬的 oracle 既有形态，不是本片引入）
//!
//! 1. `CM_OPENDOOR`：`ClientOpenDoor` 首行 `if (Envir.GetDoor(...)) return;` —— **门存在即早退**。
//!    故唯一会调 `WorldEngine.OpenDoor` 的路径是「目标格**无**门」，而 `OpenDoor` 内又要求
//!    `GetDoor(...) && !door.Status.Opened` ⇒ 恒不成立 ⇒ **CM_OPENDOOR 恒零出帧**（[`client_open_door`]）。
//! 2. `CM_DROPGOLD`：`DropGoldDown` 造出的 `MapItem.ItemId` 恒 0（全仓无 `MapItem.ItemId=` 赋值点，
//!    见 `ground.rs` 模块头同款登记），`Envir.AddItemToMap` 首行 `if (ItemId==0) return false`
//!    ⇒ DropGoldDown 恒 false ⇒ `Gold += nGold` 回滚 ⇒ **净金币不变**，但仍发 `SM_GOLDCHANGED`。
//! 3. `CM_ADJUST_BONUS`：`ClientAdjustBonus` 里 `NakedAbility bonusAbil = new NakedAbility()` **从不**由
//!    `sMsg` 灌值 ⇒ `nTotleUsePoint == 0`、`BonusAbil.* += 0` 全为 no-op ⇒ 实际门是 `nPoint == BonusPoint`
//!    （[`client_adjust_bonus`]）。
//! 4. `CM_SITDOWN`：`ClientSitDownHit` **不改任何姿态/属性字段**，只 `SendRefMsg(RM_POWERHIT)`（任务书
//!    「oracle 改了哪些属性」的答案：**一个都没改**）。
//! 5. `CM_PASSWORD` 的 `IsCheckOldPwd` 支用 `StoragePwd == sData`（**区分大小写**），而同函数
//!    `IsReConfigPwd`/`IsUnLockPwd` 支用 `OrdinalIgnoreCase`（[`process_client_password`]）。
//!
//! # 未做清单（登记；不是静默跳过）
//!
//! - **`GetGoodTick` 直发字节**（`M2Share.cs:205`，`SendSocket(string)`）：字符串源 `MessageSettings.sSTATUS_GOOD`
//!   未建模 ⇒ [`client_sit_down_hit`] 只产出「应直发 GoodTick」标志，字节留接线层登记。
//! - **`SM_ABILITY(52)`/`SM_SUBABILITY(752)` 尾**（`WAbil`/7 项能力）：复用入图路径的帧体
//!   （`enter.rs`），本模块只产出帧头与「应发」标志。
//! - **`RM_ITEMSHOW` 广播**（`DropGoldDown` 成功才发）：因 BUG-REPLICA 2 恒不发，本模块不产出。
//! - **各 `SysMsg` 的文案**（`MessageSettings.*`）：`String.conf` 无键，只记语义标签。
//! - **`CM_BUTCH` 的 `MeatQuality`/`Skeleton` RNG 效果体**：`M2Share.RandomNumber` 序列 + 动物实体
//!   未建模 ⇒ 只产出「是否进入屠宰体」「是否发 `RM_BUTCH`」标志。
//! - **`CM_USERMAKEDRUGITEM` 的 `Merchant.ClientMakeDrugItem` 体**（`Merchant.cs:1432`）：依赖
//!   `MakeItem` 配方 + 背包/元宝，归商店车道 ⇒ 本模块只产出商人与距离门。

use mir2_protocol::messages;

/// R1 余 10 条入站消息号（`lib.rs` 的派发臂按此表判定，见模块头）。
pub const INBOUND_IDENTS: [u16; 10] = [
    messages::CM_OPENDOOR,
    messages::CM_DROPGOLD,
    messages::CM_BUTCH,
    messages::CM_USERMAKEDRUGITEM,
    messages::CM_WANTMINIMAP,
    messages::CM_ADJUST_BONUS,
    messages::CM_1042,
    messages::CM_SITDOWN,
    messages::CM_PASSWORD,
    messages::CM_TEST,
];

/// 一条**线上帧头**（= `Messages.MakeMessage(wIdent, nRecog, nParam, nTag, nSeries)` 的字段）。
///
/// 与 [`crate::useitem::QueuedNotice`] 的区别：`QueuedNotice` 是 `SendMsg`（RM 空间，尾由 `pickwire`
/// 按 ident 重映射），本结构是 **RM→SM 转换之后的客户端可收帧空间**，即 `SendDefMessage` 口径。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DefFrame {
    /// `wIdent`（client ident）。
    pub ident: u16,
    /// `nRecog`。
    pub recog: i32,
    /// `nParam`。
    pub param: i32,
    /// `nTag`。
    pub tag: i32,
    /// `nSeries`。
    pub series: i32,
}

const fn def(ident: u16, recog: i32, param: i32, tag: i32, series: i32) -> DefFrame {
    DefFrame {
        ident,
        recog,
        param,
        tag,
        series,
    }
}

/// `HUtil32.MakeWord(low, high)`（`src/OpenMir2/HUtil32.cs:61-64`）：`(high<<8)|low`。
#[must_use]
pub const fn make_word(low: u8, high: u8) -> i32 {
    ((high as i32) << 8) | (low as i32)
}

/// `HUtil32.LoWord(value)`。
#[must_use]
pub const fn lo_word(value: i32) -> i32 {
    (value as u16) as i32
}

/// `HUtil32.HiWord(value)`。
#[must_use]
pub const fn hi_word(value: i32) -> i32 {
    ((value >> 16) as u16) as i32
}

// ──────────────────── CM_LOGINNOTICEOK(1018) / `ClientTick` ────────────────────

/// `CM_LOGINNOTICEOK(1018)` 的**唯一语义**：把线上 `Recog` 写进 `PlayObject.ClientTick`。
///
/// oracle 逐行（`src/M2Server/Player/PlayObject.cs:1106-1115` 的 `RunNotice`）：
/// ```text
/// :1109  if (msg.wIdent == Messages.CM_LOGINNOTICEOK)
/// :1111      LoginNoticeOk = true;
/// :1112      ClientTick = (short)msg.nParam1;   // nParam1 = Recog（WorldServer.cs:856-858 default）
/// :1113      SysMsg(ClientTick.ToString(), MsgColor.Red, MsgType.Notice);
/// ```
/// 本函数只复刻 `:1112` 的**取值/截断**（C# `(int → short)` 截断 = `as i16`）。
///
/// 为什么**不**发 `:1113` 的 `SysMsg`：oracle 自己的 `while (GetMessage(ref msg))`
/// （`:1106-1115`）会把刚入队的 `RM_SYSMESSAGE` 在下一轮取出并丢弃（`BaseObject.Message.cs:334-355`）
/// ⇒ 线上**不存在**该帧（三份 oracle 捕获均无 `Param=14591`/正文 `"0"`；设计件
/// `tests/parity/m3/design-login-notice-and-clienttick-20261017.md` §1.4/§1.8）。
/// 回帧（B2）是帧面偏离，已否决。
#[must_use]
pub fn client_login_notice_ok(recog: i32) -> i16 {
    // C# `(short)msg.nParam1`：截断低 16 位并保号（65541 → 5、-1 → -1）。
    recog as i16
}

// ───────────────────────────── CM_TEST(1017) ─────────────────────────────

/// `CM_TEST(1017)`：`PlayObject.Message.cs:909-911` → `SendDefMessage(1, 0, 0, 0, 0);`。
///
/// 无门、无副作用、无尾：恒一条 `ident=1` 的空 def 帧。
#[must_use]
pub fn client_test() -> DefFrame {
    def(1, 0, 0, 0, 0)
}

// ─────────────────────────── CM_1042(1042) ───────────────────────────

/// `CM_1042(1042)`：`PlayObject.Message.cs:996-998` → `LogService.Warn("[非法数据] " + ChrName);`。
///
/// **零出帧**、零副作用；只产出一条 Warn 日志（含说话者 `ChrName`）。
#[must_use]
pub fn client_1042(chr_name: &str) -> String {
    format!("[非法数据] {chr_name}")
}

// ───────────────────────── CM_WANTMINIMAP(1033) ─────────────────────────

/// `CM_WANTMINIMAP(1033)`：`PlayObject.Message.cs:969-971` → `ClientGetMinMap()`
/// （`PlayObject.Operate.cs:1204-1214`）。
///
/// `Envir.MinMap > 0` ⇒ `SendDefMessage(SM_READMINIMAP_OK, 0, Envir.MinMap, 0, 0)`；
/// 否则 `SendDefMessage(SM_READMINIMAP_FAIL, 0, 0, 0, 0)`。`Envir.MinMap` 是 `short`。
#[must_use]
pub fn client_get_min_map(min_map: i16) -> DefFrame {
    if min_map > 0 {
        def(messages::SM_READMINIMAP_OK, 0, i32::from(min_map), 0, 0)
    } else {
        def(messages::SM_READMINIMAP_FAIL, 0, 0, 0, 0)
    }
}

// ─────────────────────────── CM_OPENDOOR(1002) ───────────────────────────

/// `ClientOpenDoor`（`PlayObject.Operate.cs:363-374`）的决策结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OpenDoorDecision {
    /// `Envir.GetDoor(nX, nY)` 命中 ⇒ 首行**早退**（BUG-REPLICA 1）；无出帧。
    EarlyReturnDoorExists,
    /// 无门 ⇒ castle 门不成立 ⇒ 调 `WorldEngine.OpenDoor`；但 `OpenDoor` 内 `GetDoor && !Opened`
    /// 恒假（目标格无门）⇒ 仍**无出帧**。
    OpenDoorCallNoop,
}

/// `CM_OPENDOOR(1002)`：`PlayObject.Message.cs:817-819` → `ClientOpenDoor(processMsg.nParam2, nParam3)`。
///
/// 输入是 `Envir.GetDoor(nX, nY)` 是否命中（接线层传 `false`——`CM_OPENDOOR` 恒零出帧、不看门表）。
/// **无论走哪支都零出帧** —— 这是 oracle 两条早退合成出的既有形态（见模块头 BUG-REPLICA 1）。
#[must_use]
pub fn client_open_door(has_door_at_target: bool) -> (OpenDoorDecision, Vec<DefFrame>) {
    let decision = if has_door_at_target {
        OpenDoorDecision::EarlyReturnDoorExists
    } else {
        OpenDoorDecision::OpenDoorCallNoop
    };
    (decision, Vec::new())
}

// ───────────────────────── CM_ADJUST_BONUS(1043) ─────────────────────────

/// `ClientAdjustBonus`（`PlayObject.Base.cs:1156-1180`）的决策结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AdjustBonus {
    /// 是否命中 `nPoint + nTotleUsePoint == BonusPoint`（合法）分支。
    pub applied: bool,
    /// `applied` 时置 `BonusPoint = nPoint`。
    pub new_bonus_point: i32,
    /// `applied` 时调 `RecalcAbilitys()`（帧体复用入图路径；本模块只置标志）。
    pub recalc: bool,
    /// `applied` 时 `SendMsg(RM_ABILITY)` → `SM_ABILITY(52)`。
    pub send_ability: bool,
    /// `applied` 时 `SendMsg(RM_SUBABILITY)` → `SM_SUBABILITY(752)`。
    pub send_subability: bool,
    /// `BonusAbil.*` 逐项 `+= 0`（BUG-REPLICA 3）——恒不发生净变化，显式建模为 `false`（未变）。
    pub bonus_abil_changed: bool,
    /// 非法分支 `SysMsg("非法数据调整!!!", Red, Hint)`。
    pub sysmsg_invalid: bool,
}

/// `CM_ADJUST_BONUS(1043)`：`PlayObject.Message.cs:999-1001` → `ClientAdjustBonus(nParam1, Msg)`。
///
/// **BUG-REPLICA 3**：oracle 里 `NakedAbility bonusAbil = new NakedAbility()` 从不被 `sMsg` 灌值
/// ⇒ `nTotleUsePoint = DC+MC+SC+AC+MAC+HP+MP+Hit+Speed+Reserved = 0`，`BonusAbil.* += 0` 全 no-op
/// ⇒ 门退化为 `nPoint == BonusPoint`。`sMsg` 在本支**完全不被消费**（不是忽略参数，是 oracle 就没读）。
#[must_use]
pub fn client_adjust_bonus(n_point: i32, bonus_point: i32) -> AdjustBonus {
    // `nTotleUsePoint` 恒 0（fresh `NakedAbility`）。
    let n_totle_use_point = 0;
    if n_point + n_totle_use_point == bonus_point {
        AdjustBonus {
            applied: true,
            new_bonus_point: n_point,
            recalc: true,
            send_ability: true,
            send_subability: true,
            bonus_abil_changed: false,
            sysmsg_invalid: false,
        }
    } else {
        AdjustBonus {
            applied: false,
            new_bonus_point: bonus_point,
            recalc: false,
            send_ability: false,
            send_subability: false,
            bonus_abil_changed: false,
            sysmsg_invalid: true,
        }
    }
}

// ──────────────────────────── CM_SITDOWN(3012) ────────────────────────────

/// `ClientSitDownHit`（`PlayObject.Operate.cs:343-360`）的输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SitDownInputs {
    /// `Death`。
    pub death: bool,
    /// `StatusTimeArr[PoisonState.STONE] != 0`（防麻）。
    pub stone_status_nonzero: bool,
    /// `SystemShare.Config.CloseSpeedHackCheck`（部署 `Server.conf` 无键 ⇒ 缺省 **true** ⇒ 跳过限速）。
    pub close_speed_hack_check: bool,
    /// `HUtil32.GetTickCount()`。
    pub now_tick: i32,
    /// `PlayObject.TurnTick`。
    pub turn_tick: i32,
    /// `Config.TurnIntervalTime`（部署 600）。
    pub turn_interval_time: i32,
}

/// `ClientSitDownHit` 的返回（含 `ref int dwDelayTime` 的出口值 / `TurnTick` 更新）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SitDownHit {
    /// 函数返回值（决定了消息分支是「成功」还是「失败」）。
    pub result: bool,
    /// `ref dwDelayTime` 的出口值（`0` = 无延迟）。
    pub delay_time: i32,
    /// 更新后的 `TurnTick`（未写入则等于入参）。
    pub turn_tick: i32,
    /// 是否发 `SendRefMsg(RM_POWERHIT, 0,0,0,0, "")`（= `result`）。
    pub power_hit: bool,
}

/// `CM_SITDOWN` 的消息级后续动作（`PlayObject.Message.cs:1284-1338` 的 `if/else`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SitDownAction {
    /// `result == true` ⇒ `ActionTick = GetTickCount()` + `SendSocket(M2Share.GetGoodTick)`。
    GoodTick,
    /// `result == false && delayTime == 0` ⇒ `SendRefMsg(RM_MOVEFAIL, 0,0,0,0, "")`（→ `SM_MOVEFAIL`）。
    MoveFail,
    /// `result == false && delayTime != 0` ⇒ 限速分支（`GetSiteDownMsgCount`/超速踢/`SendSelfDelayMsg`；登记）。
    Delayed { delay_time: i32 },
}

/// `ClientSitDownHit`（`PlayObject.Operate.cs:343-360`）。
///
/// **BUG-REPLICA 4**：本函数**不改任何姿态/属性字段**，只 `SendRefMsg(RM_POWERHIT)` 并返回 `true`
/// （默认配置 `CloseSpeedHackCheck=true` ⇒ 限速块被跳过，`delayTime` 保持 0）。
#[must_use]
pub fn client_sit_down_hit(i: &SitDownInputs) -> SitDownHit {
    // `if (Death || StatusTimeArr[PoisonState.STONE] != 0) return false;`（delayTime 保持入参 0）。
    if i.death || i.stone_status_nonzero {
        return SitDownHit {
            result: false,
            delay_time: 0,
            turn_tick: i.turn_tick,
            power_hit: false,
        };
    }
    let mut turn_tick = i.turn_tick;
    if !i.close_speed_hack_check {
        let dw_check_time = i.now_tick.wrapping_sub(i.turn_tick);
        if dw_check_time < i.turn_interval_time {
            return SitDownHit {
                result: false,
                delay_time: i.turn_interval_time - dw_check_time,
                turn_tick: i.turn_tick,
                power_hit: false,
            };
        }
        turn_tick = i.now_tick;
    }
    // SendRefMsg(RM_POWERHIT, 0, 0, 0, 0, ""); return true;
    SitDownHit {
        result: true,
        delay_time: 0,
        turn_tick,
        power_hit: true,
    }
}

/// `CM_SITDOWN` 消息级分支（`PlayObject.Message.cs:1284-1338`）。
#[must_use]
pub const fn sit_down_action(hit: &SitDownHit) -> SitDownAction {
    if hit.result {
        SitDownAction::GoodTick
    } else if hit.delay_time == 0 {
        SitDownAction::MoveFail
    } else {
        SitDownAction::Delayed {
            delay_time: hit.delay_time,
        }
    }
}

// ────────────────────────── CM_DROPGOLD(1016) ──────────────────────────

/// `ClientDropGold`（`PlayObject.Operate.cs:222-249`）的输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DropGoldInputs {
    /// `CM_DROPGOLD` 的 `nParam1`（要丢的金币数）。
    pub n_gold: i32,
    /// `BaseObject.Gold`。
    pub gold: i32,
    /// `BaseObject.GameGold`（`SM_GOLDCHANGED` 的 LoWord/HiWord）。
    pub game_gold: i32,
    /// `Config.InSafeDisableDrop`（部署 1）。
    pub in_safe_disable_drop: bool,
    /// `InSafeZone()`。
    pub in_safe_zone: bool,
    /// `Config.ControlDropItem`（部署 0）。
    pub control_drop_item: bool,
    /// `Config.CanDropGold`（部署 1000）。
    pub can_drop_gold: i32,
    /// `PlayObject.IsCanDrop`（进世界 `= !Config.LockDropAction`，缺省 true）。
    pub is_can_drop: bool,
    /// `Envir.Flag.NoThrowItem`。
    pub no_throw_item: bool,
    /// 说话者 `ActorId`（`RM_MENU_OK` 的 `nParam1`）。
    pub actor_id: i32,
}

/// `ClientDropGold` 的拒绝原因（决定用哪条 `SysMsg` 文案；文案未建模）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DropGoldReject {
    /// `InSafeDisableDrop && InSafeZone()` ⇒ `CanotDropInSafeZoneMsg`。
    SafeZone,
    /// `ControlDropItem && nGold < CanDropGold` ⇒ `CanotDropGoldMsg`。
    BelowCanDropGold,
    /// `!IsCanDrop || Envir.Flag.NoThrowItem` ⇒ `CanotDropItemMsg`。
    CannotDropItem,
}

/// `ClientDropGold` 的决策结果。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DropGoldOutcome {
    /// `nGold <= 0` —— 消息级门（`if (processMsg.nParam1 > 0)`）不成立，什么都不做。
    NotPositive,
    /// 守卫拒绝 ⇒ `SendMsg(ManageNPC, RM_MENU_OK, 0, ActorId, 0, 0, msg)` → `SM_MENU_OK`（尾文案未建模）。
    Rejected(DropGoldReject, DefFrame),
    /// `nGold >= Gold` ⇒ 静默 return（零出帧）。
    NotEnoughGold,
    /// 过了所有门 ⇒ `Gold -= nGold`；`DropGoldDown` 恒失败（BUG-REPLICA 2）⇒ `Gold += nGold`；
    /// `GoldChanged()` ⇒ `SM_GOLDCHANGED`。**净金币不变**，但帧照发。
    Dropped { gold: i32, frame: DefFrame },
}

/// `SM_GOLDCHANGED`（`PlayObject.Message.cs:1833-1835`）：
/// `SendDefMessage(SM_GOLDCHANGED, Gold, LoWord(GameGold), HiWord(GameGold), 0)`。
#[must_use]
pub fn sm_goldchanged(gold: i32, game_gold: i32) -> DefFrame {
    def(
        messages::SM_GOLDCHANGED,
        gold,
        lo_word(game_gold),
        hi_word(game_gold),
        0,
    )
}

/// `RM_MENU_OK` → `SM_MENU_OK`（`PlayObject.Message.cs:1952-1954`）：
/// `SendDefMessage(SM_MENU_OK, processMsg.nParam1, 0, 0, 0, Msg)`。
#[must_use]
pub fn sm_menu_ok(n_param1: i32) -> DefFrame {
    def(messages::SM_MENU_OK, n_param1, 0, 0, 0)
}

/// `CM_DROPGOLD(1016)`：`PlayObject.Message.cs:903-908` → `if (nParam1 > 0) ClientDropGold(nParam1)`。
///
/// 消息级门 + `ClientDropGold` 三段守卫 + `nGold>=Gold` 门 + 净金币不变（BUG-REPLICA 2）。
#[must_use]
pub fn client_drop_gold(i: &DropGoldInputs) -> DropGoldOutcome {
    if i.n_gold <= 0 {
        return DropGoldOutcome::NotPositive;
    }
    if i.in_safe_disable_drop && i.in_safe_zone {
        return DropGoldOutcome::Rejected(DropGoldReject::SafeZone, sm_menu_ok(i.actor_id));
    }
    if i.control_drop_item && i.n_gold < i.can_drop_gold {
        return DropGoldOutcome::Rejected(DropGoldReject::BelowCanDropGold, sm_menu_ok(i.actor_id));
    }
    if !i.is_can_drop || i.no_throw_item {
        return DropGoldOutcome::Rejected(DropGoldReject::CannotDropItem, sm_menu_ok(i.actor_id));
    }
    if i.n_gold >= i.gold {
        return DropGoldOutcome::NotEnoughGold;
    }
    // Gold -= nGold; if (!DropGoldDown(...)) Gold += nGold; GoldChanged();
    // DropGoldDown 恒 false（BUG-REPLICA 2）⇒ 净不变。
    let gold = i.gold - i.n_gold + i.n_gold;
    DropGoldOutcome::Dropped {
        gold,
        frame: sm_goldchanged(gold, i.game_gold),
    }
}

// ──────────────────────────── CM_BUTCH(1007) ────────────────────────────

/// `ClientGetButchItem`（`PlayObject.Operate.cs:702-751`）的输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButchInputs {
    /// `nX`（`nParam1`）。
    pub n_x: i32,
    /// `nY`（`nParam2`）。
    pub n_y: i32,
    /// `btDir`（`(byte)wParam`）。
    pub bt_dir: u8,
    /// `CurrX`。
    pub curr_x: i32,
    /// `CurrY`。
    pub curr_y: i32,
    /// `Config.CloseSpeedHackCheck`（缺省 true）。
    pub close_speed_hack_check: bool,
    /// `HUtil32.GetTickCount()`。
    pub now_tick: i32,
    /// `PlayObject.TurnTick`。
    pub turn_tick: i32,
    /// `Config.TurnIntervalTime`（部署 600）。
    pub turn_interval_time: i32,
    /// `Envir.IsValidObject(nX, nY, 2, baseObject)`（2 格内有目标对象）。
    pub valid_object: bool,
    /// 目标对象的屠宰前置：`baseObject.Death && !Skeleton && Animal`。
    pub target_butcherable: bool,
}

/// `ClientGetButchItem` 的决策结果。
///
/// `result` **恒 false**（oracle 无一支出 `true`）⇒ 消息层恒走 `if (!ClientGetButchItem(...))`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButchOutcome {
    /// 函数返回值（恒 `false`）。
    pub result: bool,
    /// `ref dwDelayTime` 的出口值。
    pub delay_time: i32,
    /// 是否更新 `Dir = btDir`（仅在 `|nX-CurrX|<=2 && |nY-CurrY|<=2` 成立时）。
    pub dir_updated: bool,
    /// 是否进入屠宰效果体（`Death && !Skeleton && Animal`）——**RNG 效果体未建模（登记）**。
    pub butchered: bool,
    /// 是否发 `SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")`（过限速门后**恒发**）。
    pub send_butch: bool,
    /// 更新后的 `TurnTick`（未写入则等于入参）。
    pub turn_tick: i32,
}

/// `ClientGetButchItem`（`PlayObject.Operate.cs:702-751`）。
///
/// 顺序：`dwDelayTime=0` → 限速门 → `|Δ|<=2` → `IsValidObject` → 屠宰效果体 → `Dir=btDir` →
/// `SendRefMsg(RM_BUTCH)` → `return false`。
#[must_use]
pub fn client_butch(i: &ButchInputs) -> ButchOutcome {
    let mut delay_time = 0;
    let mut turn_tick = i.turn_tick;
    // `if (!Config.CloseSpeedHackCheck) { ... }`（缺省 true ⇒ 跳过）。
    if !i.close_speed_hack_check {
        let dw_check_time = i.now_tick.wrapping_sub(i.turn_tick);
        // `HUtil32._MAX(150, TurnIntervalTime - 150)`
        let threshold = (i.turn_interval_time - 150).max(150);
        if dw_check_time < threshold {
            delay_time = threshold - dw_check_time;
            return ButchOutcome {
                result: false,
                delay_time,
                dir_updated: false,
                butchered: false,
                send_butch: false,
                turn_tick: i.turn_tick,
            };
        }
        turn_tick = i.now_tick;
    }
    let mut dir_updated = false;
    let mut butchered = false;
    if (i.n_x - i.curr_x).abs() <= 2 && (i.n_y - i.curr_y).abs() <= 2 {
        if i.valid_object && i.target_butcherable {
            butchered = true;
        }
        dir_updated = true;
    }
    // SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, ""); return false;
    let _ = &mut delay_time;
    ButchOutcome {
        result: false,
        delay_time,
        dir_updated,
        butchered,
        send_butch: true,
        turn_tick,
    }
}

/// `RM_BUTCH` → `SM_BUTCH`（`PlayObject.Message.cs:1781-1788`）：
/// `if (processMsg.ActorId != 0) SendSocket(MakeMessage(SM_BUTCH, processMsg.ActorId, nParam1, nParam2, wParam))`。
///
/// 字段来源（oracle）：
/// 1. 发帧点 `PlayObject.Operate.cs:749`：`SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")`；
/// 2. `SendRefMsg`（`BaseObject.Message.cs:383`）形参序 `(wIdent, wParam, nParam1, nParam2, nParam3)`
///    ⇒ 收件人 `ProcessMessage` 得 `wParam=Dir, nParam1=CurrX, nParam2=CurrY`；
/// 3. `MakeMessage`（`Messages.cs:846`）形参序 `(msg, recog, param, tag, series)`
///    ⇒ **`recog=ActorId, param=CurrX, tag=CurrY, series=Dir`**。
#[must_use]
pub fn sm_butch(actor_id: i32, dir: i32, curr_x: i32, curr_y: i32) -> Option<DefFrame> {
    if actor_id != 0 {
        Some(DefFrame {
            ident: messages::SM_BUTCH,
            recog: actor_id,
            param: curr_x,
            tag: curr_y,
            series: dir,
        })
    } else {
        None
    }
}

// ────────────────────── CM_USERMAKEDRUGITEM(1034) ──────────────────────

/// `ClientMakeDrugItem`（`PlayObject.Operate.cs:1216-1227`）的输入。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MakeDrugInputs {
    /// `SystemShare.WorldEngine.FindMerchant(actorId)` 是否命中（`actorId = nParam1`）。
    pub merchant_found: bool,
    /// `merchant.IsMakeDrug`。
    pub merchant_is_make_drug: bool,
    /// `merchant.Envir == Envir`（同图）。
    pub same_envir: bool,
    /// `IsWithinSight(merchant)`。
    pub within_sight: bool,
}

/// `ClientMakeDrugItem` 的决策结果。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MakeDrugOutcome {
    /// `merchant == null || !merchant.IsMakeDrug` ⇒ return。
    NoMerchant,
    /// `merchant.Envir != Envir || !IsWithinSight(merchant)` ⇒ return。
    OutOfReach,
    /// 进入 `merchant.ClientMakeDrugItem(this, nItemName)`（体归商店车道，登记）。
    DelegatedToMerchant,
}

/// `CM_USERMAKEDRUGITEM(1034)`：`PlayObject.Message.cs:972-974` → `ClientMakeDrugItem(nParam1, Msg)`。
#[must_use]
pub fn client_make_drug_item(i: &MakeDrugInputs) -> MakeDrugOutcome {
    if !i.merchant_found || !i.merchant_is_make_drug {
        return MakeDrugOutcome::NoMerchant;
    }
    if !i.same_envir || !i.within_sight {
        return MakeDrugOutcome::OutOfReach;
    }
    MakeDrugOutcome::DelegatedToMerchant
}

// ─────────────────────────── CM_PASSWORD(2001) ───────────────────────────

/// `PlayObject` 的仓库/登录密码相关状态字段（`PlayObject.Base.cs:3190-3298` 读写的那些）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StoragePwdState {
    /// `PlayObject.StoragePwd`。
    pub storage_pwd: String,
    /// `PlayObject.MSTempPwd`。
    pub ms_temp_pwd: String,
    /// `PlayObject.IsSetStoragePwd`。
    pub is_set_storage_pwd: bool,
    /// `PlayObject.IsReConfigPwd`。
    pub is_re_config_pwd: bool,
    /// `PlayObject.IsUnLockPwd`。
    pub is_un_lock_pwd: bool,
    /// `PlayObject.IsUnLockStoragePwd`。
    pub is_un_lock_storage_pwd: bool,
    /// `PlayObject.IsCheckOldPwd`。
    pub is_check_old_pwd: bool,
    /// `PlayObject.IsPasswordLocked`。
    pub is_password_locked: bool,
    /// `PlayObject.PwdFailCount`。
    pub pwd_fail_count: i32,
}

/// `Config.Lock*Action`（`GameSvrConf.cs:1490-1500`，全缺省 false）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PasswordLockConfig {
    pub lock_deal_action: bool,
    pub lock_drop_action: bool,
    pub lock_walk_action: bool,
    pub lock_run_action: bool,
    pub lock_hit_action: bool,
    pub lock_spell_action: bool,
    pub lock_send_msg_action: bool,
    pub lock_user_item_action: bool,
    pub lock_in_ob_mode_action: bool,
    pub lock_get_back_item_action: bool,
}

/// 解锁动作使能（`IsCanDeal`/`IsCanDrop`/…）。随 `process_client_password` 原地改。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ActionLocks {
    pub is_can_deal: bool,
    pub is_can_drop: bool,
    pub is_can_walk: bool,
    pub is_can_run: bool,
    pub is_can_hit: bool,
    pub is_can_spell: bool,
    pub is_can_send_msg: bool,
    pub bo_can_use_item: bool,
    pub ob_mode: bool,
    pub admin_mode: bool,
    pub is_lock_logoned: bool,
    pub is_can_get_back_item: bool,
}

/// **接线批 A**：一个会话的 `CM_PASSWORD` 密码态（`StoragePwdState` + `ActionLocks`）。
///
/// `process_client_password` **原地改**这两组字段、跨多次 `CM_PASSWORD` 调用累积
/// （`IsSetStoragePwd`→`IsReConfigPwd`→… 的状态机）。宿主按 `chr_name` 持有本结构。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PwdSession {
    pub state: StoragePwdState,
    pub locks: ActionLocks,
}

/// 一条 `SysMsg` 的语义标签（文案 = `MessageSettings.*`，`String.conf` 无键 ⇒ 未建模）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PwdMsg {
    /// 绿：`ReSetPasswordMsg`「请重复输入一次仓库密码：」。
    ReSetPassword,
    /// 红：`PasswordOverLongMsg`「长度不正确（4-7）」。
    PasswordOverLong,
    /// 蓝：`ReSetPasswordOKMsg`。
    ReSetPasswordOk,
    /// 红：`ReSetPasswordNotMatchMsg`。
    ReSetPasswordNotMatch,
    /// 蓝：`PasswordUnLockOKMsg`。
    PasswordUnLockOk,
    /// 蓝：`StorageUnLockOKMsg`。
    StorageUnLockOk,
    /// 红：`UnLockPasswordFailMsg`。
    UnLockPasswordFail,
    /// 红：`StoragePasswordLockedMsg`。
    StoragePasswordLocked,
    /// 绿：`SetPasswordMsg`。
    SetPassword,
    /// 红：`OldPasswordIncorrectMsg`。
    OldPasswordIncorrect,
}

/// `ProcessClientPassword`（`PlayObject.Base.cs:3190-3298`）的输出。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PasswordOutcome {
    /// 产出的 `SysMsg`（按代码顺序）。
    pub msgs: Vec<PwdMsg>,
    /// 是否 `SendMsg(Messages.RM_PASSWORD, 0, 0, 0, 0)`（→ `SM_PASSWORD`，`PlayObject.Message.cs:2031`）。
    pub send_password_frame: bool,
}

/// `ProcessClientPassword`（`PlayObject.Base.cs:3190-3298`）。**原地改** `state` 与 `locks`。
///
/// 分支顺序（每个 `return` 都在 oracle 里存在，最后一个 `IsCheckOldPwd` 到函数尾）：
/// `wParam==0` → `IsSetStoragePwd` → `IsReConfigPwd` → `IsUnLockPwd||IsUnLockStoragePwd` → `IsCheckOldPwd`。
#[must_use]
pub fn process_client_password(
    state: &mut StoragePwdState,
    locks: &mut ActionLocks,
    cfg: &PasswordLockConfig,
    w_param: u16,
    msg: &str,
) -> PasswordOutcome {
    // `if (processMsg.wParam == 0) return;`（连 `sData` 都不取）。
    if w_param == 0 {
        return PasswordOutcome {
            msgs: Vec::new(),
            send_password_frame: false,
        };
    }
    let s_data = msg;
    let n_len = s_data.chars().count();
    let mut msgs = Vec::new();
    let mut send_password_frame = false;

    if state.is_set_storage_pwd {
        state.is_set_storage_pwd = false;
        if n_len > 3 && n_len < 8 {
            state.ms_temp_pwd = s_data.to_string();
            state.is_re_config_pwd = true;
            msgs.push(PwdMsg::ReSetPassword);
            send_password_frame = true;
        } else {
            msgs.push(PwdMsg::PasswordOverLong);
        }
        return PasswordOutcome {
            msgs,
            send_password_frame,
        };
    }
    if state.is_re_config_pwd {
        state.is_re_config_pwd = false;
        if state.ms_temp_pwd.eq_ignore_ascii_case(s_data) {
            state.storage_pwd = s_data.to_string();
            state.is_password_locked = true;
            msgs.push(PwdMsg::ReSetPasswordOk);
        } else {
            msgs.push(PwdMsg::ReSetPasswordNotMatch);
        }
        return PasswordOutcome {
            msgs,
            send_password_frame,
        };
    }
    if state.is_un_lock_pwd || state.is_un_lock_storage_pwd {
        if state.storage_pwd.eq_ignore_ascii_case(s_data) {
            state.is_password_locked = false;
            if state.is_un_lock_pwd {
                if cfg.lock_deal_action {
                    locks.is_can_deal = true;
                }
                if cfg.lock_drop_action {
                    locks.is_can_drop = true;
                }
                if cfg.lock_walk_action {
                    locks.is_can_walk = true;
                }
                if cfg.lock_run_action {
                    locks.is_can_run = true;
                }
                if cfg.lock_hit_action {
                    locks.is_can_hit = true;
                }
                if cfg.lock_spell_action {
                    locks.is_can_spell = true;
                }
                if cfg.lock_send_msg_action {
                    locks.is_can_send_msg = true;
                }
                if cfg.lock_user_item_action {
                    locks.bo_can_use_item = true;
                }
                if cfg.lock_in_ob_mode_action {
                    locks.ob_mode = false;
                    locks.admin_mode = false;
                }
                locks.is_lock_logoned = true;
                msgs.push(PwdMsg::PasswordUnLockOk);
            }
            if state.is_un_lock_storage_pwd {
                if cfg.lock_get_back_item_action {
                    locks.is_can_get_back_item = true;
                }
                msgs.push(PwdMsg::StorageUnLockOk);
            }
        } else {
            state.pwd_fail_count += 1;
            msgs.push(PwdMsg::UnLockPasswordFail);
            if state.pwd_fail_count > 3 {
                msgs.push(PwdMsg::StoragePasswordLocked);
            }
        }
        state.is_un_lock_pwd = false;
        state.is_un_lock_storage_pwd = false;
        return PasswordOutcome {
            msgs,
            send_password_frame,
        };
    }
    if state.is_check_old_pwd {
        state.is_check_old_pwd = false;
        // BUG-REPLICA 5：此处是 `==`（区分大小写），非 `OrdinalIgnoreCase`。
        if state.storage_pwd == s_data {
            send_password_frame = true;
            msgs.push(PwdMsg::SetPassword);
            state.is_set_storage_pwd = true;
        } else {
            state.pwd_fail_count += 1;
            msgs.push(PwdMsg::OldPasswordIncorrect);
            if state.pwd_fail_count > 3 {
                msgs.push(PwdMsg::StoragePasswordLocked);
                state.is_password_locked = true;
            }
        }
    }
    PasswordOutcome {
        msgs,
        send_password_frame,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── CM_LOGINNOTICEOK(1018) ──
    /// `ClientTick = (short)nParam1`：C# 截断语义（正数/负数/超 16 位）。
    #[test]
    fn cm_login_notice_ok_truncates_to_short() {
        assert_eq!(client_login_notice_ok(1234), 1234);
        assert_eq!(client_login_notice_ok(0), 0);
        assert_eq!(client_login_notice_ok(32767), 32767);
        // 0x0001_0005 = 65541 ⇒ (short)5。
        assert_eq!(client_login_notice_ok(65541), 5);
        // 0x0000_8000 = 32768 ⇒ (short)-32768。
        assert_eq!(client_login_notice_ok(32768), i16::MIN);
        assert_eq!(client_login_notice_ok(-1), -1);
        // 线上消息号就是 1018（`Messages.cs`；常量由 msgcodegen 门禁守）。
        assert_eq!(messages::CM_LOGINNOTICEOK, 1018);
    }

    // ── CM_TEST ──
    #[test]
    fn cm_test_emits_ident_one_frame() {
        let f = client_test();
        // 改坏 `ident:1` 或任一参 ⇒ 必红。
        assert_eq!(f, def(1, 0, 0, 0, 0));
    }

    // ── CM_1042 ──
    #[test]
    fn cm_1042_logs_and_has_no_frame() {
        assert_eq!(client_1042("张三"), "[非法数据] 张三");
    }

    // ── CM_WANTMINIMAP ──
    #[test]
    fn cm_wantminimap_ok_when_minmap_positive() {
        // `Envir.MinMap > 0` ⇒ SM_READMINIMAP_OK(710, recog 0, param MinMap)。
        assert_eq!(client_get_min_map(3), def(710, 0, 3, 0, 0));
        // 边界：`MinMap == 0` 走 FAIL。
        assert_eq!(client_get_min_map(0), def(711, 0, 0, 0, 0));
        assert_eq!(client_get_min_map(-1), def(711, 0, 0, 0, 0));
    }

    // ── CM_OPENDOOR：BUG-REPLICA 1 ──
    #[test]
    fn cm_opendoor_is_always_zero_frame_bug_replica() {
        // 门存在 ⇒ 早退；门不存在 ⇒ OpenDoor 也是 no-op。两支都零出帧。
        let (d1, f1) = client_open_door(true);
        assert_eq!(d1, OpenDoorDecision::EarlyReturnDoorExists);
        assert!(f1.is_empty(), "门存在应早退，零出帧");
        let (d2, f2) = client_open_door(false);
        assert_eq!(d2, OpenDoorDecision::OpenDoorCallNoop);
        assert!(f2.is_empty(), "无门时 OpenDoor 也是 no-op，零出帧");
    }

    // ── CM_ADJUST_BONUS：BUG-REPLICA 3 ──
    #[test]
    fn cm_adjust_bonus_gate_is_npoint_eq_bonuspoint() {
        // nPoint == BonusPoint ⇒ 合法：set BonusPoint + RecalcAbilitys + RM_ABILITY/RM_SUBABILITY。
        let ok = client_adjust_bonus(5, 5);
        assert!(ok.applied && ok.recalc && ok.send_ability && ok.send_subability);
        assert_eq!(ok.new_bonus_point, 5);
        assert!(!ok.sysmsg_invalid && !ok.bonus_abil_changed);
        // nPoint != BonusPoint ⇒ SysMsg 非法，零出帧。
        let bad = client_adjust_bonus(4, 5);
        assert!(!bad.applied && bad.sysmsg_invalid);
        assert_eq!(bad.new_bonus_point, 5, "非法分支不改 BonusPoint");
        assert!(!bad.send_ability && !bad.send_subability);
    }

    // ── CM_SITDOWN：BUG-REPLICA 4 ──
    fn sit_defaults() -> SitDownInputs {
        SitDownInputs {
            death: false,
            stone_status_nonzero: false,
            close_speed_hack_check: true, // 部署缺省 true
            now_tick: 10_000,
            turn_tick: 0,
            turn_interval_time: 600,
        }
    }

    #[test]
    fn cm_sitdown_default_succeeds_and_sends_powerhit() {
        let hit = client_sit_down_hit(&sit_defaults());
        assert!(hit.result && hit.power_hit, "默认配置 ⇒ 成功 + RM_POWERHIT");
        assert_eq!(sit_down_action(&hit), SitDownAction::GoodTick);
    }

    #[test]
    fn cm_sitdown_death_or_stone_returns_movefail() {
        let i = SitDownInputs {
            death: true,
            ..sit_defaults()
        };
        let hit = client_sit_down_hit(&i);
        assert!(!hit.result && hit.delay_time == 0 && !hit.power_hit);
        assert_eq!(sit_down_action(&hit), SitDownAction::MoveFail);

        let j = SitDownInputs {
            stone_status_nonzero: true,
            ..sit_defaults()
        };
        let hit2 = client_sit_down_hit(&j);
        assert!(!hit2.result);
        assert_eq!(sit_down_action(&hit2), SitDownAction::MoveFail);
    }

    #[test]
    fn cm_sitdown_speed_check_trips_when_enabled() {
        // 打开限速且 `now - turn_tick < TurnIntervalTime` ⇒ 失败 + delayTime = 差。
        let i = SitDownInputs {
            close_speed_hack_check: false,
            now_tick: 100,
            turn_tick: 0,
            turn_interval_time: 600,
            ..sit_defaults()
        };
        let hit = client_sit_down_hit(&i);
        assert!(!hit.result && !hit.power_hit);
        assert_eq!(hit.delay_time, 500);
        assert_eq!(
            sit_down_action(&hit),
            SitDownAction::Delayed { delay_time: 500 }
        );
    }

    // ── CM_DROPGOLD：BUG-REPLICA 2 ──
    fn drop_gold_defaults() -> DropGoldInputs {
        DropGoldInputs {
            n_gold: 100,
            gold: 1000,
            game_gold: 0,
            in_safe_disable_drop: true, // 部署 1
            in_safe_zone: false,
            control_drop_item: false, // 部署 0
            can_drop_gold: 1000,
            is_can_drop: true,
            no_throw_item: false,
            actor_id: 77,
        }
    }

    #[test]
    fn cm_dropgold_net_gold_unchanged_bug_replica() {
        let out = client_drop_gold(&drop_gold_defaults());
        match out {
            DropGoldOutcome::Dropped { gold, frame } => {
                // 净不变（DropGoldDown 恒失败 ⇒ 回滚）。
                assert_eq!(gold, 1000, "净金币不变（BUG-REPLICA 2）");
                assert_eq!(frame, def(653, 1000, 0, 0, 0));
            }
            other => panic!("应走 Dropped，实得 {other:?}"),
        }
    }

    #[test]
    fn cm_dropgold_nonpositive_is_noop() {
        let i = DropGoldInputs {
            n_gold: 0,
            ..drop_gold_defaults()
        };
        assert_eq!(client_drop_gold(&i), DropGoldOutcome::NotPositive);
    }

    #[test]
    fn cm_dropgold_gates() {
        // 安全区禁用丢弃。
        let i = DropGoldInputs {
            in_safe_zone: true,
            ..drop_gold_defaults()
        };
        assert!(matches!(
            client_drop_gold(&i),
            DropGoldOutcome::Rejected(DropGoldReject::SafeZone, _)
        ));
        // 控制丢弃 + nGold < CanDropGold。
        let j = DropGoldInputs {
            control_drop_item: true,
            can_drop_gold: 1000,
            n_gold: 100,
            ..drop_gold_defaults()
        };
        assert!(matches!(
            client_drop_gold(&j),
            DropGoldOutcome::Rejected(DropGoldReject::BelowCanDropGold, _)
        ));
        // !IsCanDrop。
        let k = DropGoldInputs {
            is_can_drop: false,
            ..drop_gold_defaults()
        };
        assert!(matches!(
            client_drop_gold(&k),
            DropGoldOutcome::Rejected(DropGoldReject::CannotDropItem, _)
        ));
        // nGold >= Gold。
        let l = DropGoldInputs {
            gold: 100,
            ..drop_gold_defaults()
        };
        assert_eq!(client_drop_gold(&l), DropGoldOutcome::NotEnoughGold);
    }

    // ── CM_BUTCH ──
    fn butch_defaults() -> ButchInputs {
        ButchInputs {
            n_x: 10,
            n_y: 10,
            bt_dir: 3,
            curr_x: 10,
            curr_y: 10,
            close_speed_hack_check: true,
            now_tick: 0,
            turn_tick: 0,
            turn_interval_time: 600,
            valid_object: true,
            target_butcherable: true,
        }
    }

    #[test]
    fn cm_butch_always_false_and_sends_butch() {
        let out = client_butch(&butch_defaults());
        assert!(
            !out.result,
            "ClientGetButchItem 恒 false（oracle 无 true 支）"
        );
        assert_eq!(out.delay_time, 0, "默认配置跳过限速 ⇒ delayTime 0");
        assert!(out.dir_updated, "同格（|Δ|=0 ≤ 2）⇒ Dir = btDir");
        assert!(out.butchered, "Death&&!Skeleton&&Animal ⇒ 进屠宰体");
        assert!(out.send_butch, "过限速门后恒发 RM_BUTCH");
    }

    #[test]
    fn cm_butch_out_of_range_updates_no_dir() {
        let i = ButchInputs {
            n_x: 20,
            ..butch_defaults()
        };
        let out = client_butch(&i);
        assert!(!out.dir_updated);
        assert!(!out.butchered);
        assert!(out.send_butch, "即便超距，仍发 RM_BUTCH");
    }

    #[test]
    fn cm_butch_speed_gate_blocks_butch_broadcast() {
        let i = ButchInputs {
            close_speed_hack_check: false,
            now_tick: 0,
            turn_tick: 0,
            turn_interval_time: 600,
            ..butch_defaults()
        };
        let out = client_butch(&i);
        assert!(
            !out.send_butch,
            "限速命中 ⇒ 提前 return false，不发 RM_BUTCH"
        );
        // threshold = MAX(150, 600-150) = 450；dwCheckTime=0 ⇒ delayTime=450。
        assert_eq!(out.delay_time, 450);
    }

    #[test]
    fn sm_butch_field_mapping_and_zero_actor_suppressed() {
        // oracle `if (processMsg.ActorId != 0)`：ActorId 为 0 ⇒ 抑制（不发帧）。
        assert!(sm_butch(0, 0, 0, 0).is_none());
        // oracle：recog=ActorId, param=nParam1=CurrX, tag=nParam2=CurrY, series=wParam=Dir。
        // 四值互异 ⇒ 任一字段错位/取错源都会与期望元组不等 ⇒ 红。
        assert_eq!(
            sm_butch(5, 3, 11, 12),
            Some(def(637, 5, 11, 12, 3)),
            "sm_butch 应映射 recog=ActorId, param=CurrX, tag=CurrY, series=Dir"
        );
    }

    // ── CM_USERMAKEDRUGITEM ──
    #[test]
    fn cm_make_drug_gates() {
        assert_eq!(
            client_make_drug_item(&MakeDrugInputs {
                merchant_found: false,
                merchant_is_make_drug: false,
                same_envir: true,
                within_sight: true,
            }),
            MakeDrugOutcome::NoMerchant
        );
        assert_eq!(
            client_make_drug_item(&MakeDrugInputs {
                merchant_found: true,
                merchant_is_make_drug: true,
                same_envir: true,
                within_sight: false,
            }),
            MakeDrugOutcome::OutOfReach
        );
        assert_eq!(
            client_make_drug_item(&MakeDrugInputs {
                merchant_found: true,
                merchant_is_make_drug: true,
                same_envir: true,
                within_sight: true,
            }),
            MakeDrugOutcome::DelegatedToMerchant
        );
    }

    // ── CM_PASSWORD：BUG-REPLICA 5 + 状态机 ──
    #[test]
    fn cm_password_wparam_zero_is_noop() {
        let mut s = StoragePwdState::default();
        let mut l = ActionLocks::default();
        let out =
            process_client_password(&mut s, &mut l, &PasswordLockConfig::default(), 0, "1234");
        assert!(out.msgs.is_empty() && !out.send_password_frame);
        assert_eq!(s, StoragePwdState::default(), "零 wParam 不碰任何状态");
    }

    #[test]
    fn cm_password_set_then_reconfig() {
        let mut s = StoragePwdState {
            is_set_storage_pwd: true,
            ..Default::default()
        };
        let mut l = ActionLocks::default();
        let out =
            process_client_password(&mut s, &mut l, &PasswordLockConfig::default(), 1, "abcd");
        assert_eq!(out.msgs, vec![PwdMsg::ReSetPassword]);
        assert!(out.send_password_frame, "合法长度 ⇒ 回 SM_PASSWORD");
        assert!(s.is_re_config_pwd && s.ms_temp_pwd == "abcd" && !s.is_set_storage_pwd);

        // 第二次重复输入（大小写不敏感）⇒ 设成功 + 上锁。
        let out2 =
            process_client_password(&mut s, &mut l, &PasswordLockConfig::default(), 1, "AbCd");
        assert_eq!(out2.msgs, vec![PwdMsg::ReSetPasswordOk]);
        assert_eq!(s.storage_pwd, "AbCd");
        assert!(s.is_password_locked);
    }

    #[test]
    fn cm_password_overlong_rejected() {
        let mut s = StoragePwdState {
            is_set_storage_pwd: true,
            ..Default::default()
        };
        let mut l = ActionLocks::default();
        // 长度 8 不满足 `nLen < 8`。
        let out = process_client_password(
            &mut s,
            &mut l,
            &PasswordLockConfig::default(),
            1,
            "12345678",
        );
        assert_eq!(out.msgs, vec![PwdMsg::PasswordOverLong]);
        assert!(!out.send_password_frame);
        assert!(!s.is_re_config_pwd);
    }

    #[test]
    fn cm_password_check_old_is_case_sensitive_bug_replica() {
        let mut s = StoragePwdState {
            storage_pwd: "Secret".to_string(),
            is_check_old_pwd: true,
            ..Default::default()
        };
        let mut l = ActionLocks::default();
        // 大小写不同 ⇒ `==` 不成立（BUG-REPLICA 5：此处是区分大小写的 `==`）。
        let out =
            process_client_password(&mut s, &mut l, &PasswordLockConfig::default(), 1, "secret");
        assert_eq!(out.msgs, vec![PwdMsg::OldPasswordIncorrect]);
        assert!(!s.is_set_storage_pwd);

        // 精确相等 ⇒ 回 SM_PASSWORD + 置 IsSetStoragePwd。
        let mut s2 = StoragePwdState {
            storage_pwd: "Secret".to_string(),
            is_check_old_pwd: true,
            ..Default::default()
        };
        let out2 =
            process_client_password(&mut s2, &mut l, &PasswordLockConfig::default(), 1, "Secret");
        assert!(out2.send_password_frame && s2.is_set_storage_pwd);
        assert_eq!(out2.msgs, vec![PwdMsg::SetPassword]);
    }

    #[test]
    fn cm_password_unlock_is_case_insensitive_and_applies_locks() {
        let mut s = StoragePwdState {
            storage_pwd: "Secret".to_string(),
            is_un_lock_pwd: true,
            is_password_locked: true,
            ..Default::default()
        };
        // 全锁开 ⇒ 逐项使能。
        let cfg = PasswordLockConfig {
            lock_deal_action: true,
            lock_drop_action: true,
            lock_walk_action: true,
            lock_in_ob_mode_action: true,
            ..Default::default()
        };
        let mut l = ActionLocks {
            ob_mode: true,
            admin_mode: true,
            ..Default::default()
        };
        // 大小写不同也命中（OrdinalIgnoreCase）。
        let out = process_client_password(&mut s, &mut l, &cfg, 1, "secret");
        assert_eq!(out.msgs, vec![PwdMsg::PasswordUnLockOk]);
        assert!(!s.is_password_locked && !s.is_un_lock_pwd);
        assert!(l.is_can_deal && l.is_can_drop && l.is_can_walk && !l.ob_mode && !l.admin_mode);
        assert!(l.is_lock_logoned);
        // 只开的部分不误置。
        assert!(!l.is_can_run && !l.is_can_hit);
    }

    #[test]
    fn cm_password_unlock_fail_counts_and_locks_after_three() {
        let mut s = StoragePwdState {
            storage_pwd: "Secret".to_string(),
            is_un_lock_pwd: true,
            pwd_fail_count: 3,
            ..Default::default()
        };
        let mut l = ActionLocks::default();
        let out =
            process_client_password(&mut s, &mut l, &PasswordLockConfig::default(), 1, "wrong");
        assert_eq!(s.pwd_fail_count, 4);
        // 第 4 次失败 ⇒ 附 StoragePasswordLockedMsg。
        assert_eq!(
            out.msgs,
            vec![PwdMsg::UnLockPasswordFail, PwdMsg::StoragePasswordLocked]
        );
        assert!(!s.is_un_lock_pwd);
    }

    // ── 派发臂存在性断言 ──
    #[test]
    fn inbound_idents_are_exactly_the_ten_targets() {
        use std::collections::BTreeSet;
        let set: BTreeSet<u16> = INBOUND_IDENTS.iter().copied().collect();
        assert_eq!(set.len(), 10, "10 条入站消息号不得重复");
        for id in [
            1002u16, 1007, 1016, 1017, 1033, 1034, 1042, 1043, 2001, 3012,
        ] {
            assert!(set.contains(&id), "派发臂应含 CM_{id}");
        }
    }
}
