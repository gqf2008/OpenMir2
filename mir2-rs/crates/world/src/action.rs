//! **出站接线片 W2（B 类 5 帧）**：`RM_PUSH`/`RM_RUSH`/`RM_RUSHKUNG`/`RM_BUTCH`/`RM_DEATH(nParam3=0)`
//! 的**世界侧事件**词汇表。
//!
//! 本模块只定义「世界发生了什么」的**确定性事件**（谁、朝哪、在哪个格、哪一族），**帧面（编码）**
//! 在宿主 `bins/game-svr`（`smframe::sm_move`/`sm_action_hit`/`sm_death` + `downlink` 的
//! 事件→帧翻译）。`crates/world` 不碰 `SM_*` 字节（与 `aoi`/`party`/`slave` 同口径）。
//!
//! 五条 oracle 触发点（逐条**回读**，不采信门单方结论）：
//!
//! | 帧 | ident | oracle 发点 | 本模块事件 |
//! |----|-------|-------------|-----------|
//! | `SM_BACKSTEP` | 9 | `BaseObject.CharPushed` 的 `SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "")`（`src/M2Server/Actor/BaseObject.cs:871`） | [`PushEvent`] |
//! | `SM_RUSH` | 6 | `PlayObject.DoMotaebo` 的 `SendRefMsg(RM_RUSH, nDir, CurrX, CurrY, 0, "")`（`src/M2Server/Player/PlayObject.Base.cs:1435,1453`） | [`RushEvent`]（`RushFrame::Rush`） |
//! | `SM_RUSHKUNG` | 7 | `PlayObject.DoMotaebo` 的 `SendRefMsg(RM_RUSHKUNG, Dir, nX, nY, 0, "")`（`PlayObject.Base.cs:1488`） | [`RushEvent`]（`RushFrame::RushKung`） |
//! | `SM_BUTCH` | 637 | `PlayObject.ClientGetButchItem` 末尾**无条件** `SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")`（`src/M2Server/Player/PlayObject.Operate.cs:749`） | [`ButchEvent`] |
//! | `SM_DEATH` | 32 | `PlayObject.Run()` 的 `VisibleFlag.Show` 支（出现帧）里 `if (baseObject.Death) SendMsg(baseObject, RM_DEATH, Dir, CurrX, CurrY, 0)`（`src/M2Server/Player/PlayObject.cs:3235`） | 由 `bins/game-svr` 的 AOI 出现帧路径按 `Entity::death` 直接判（无独立事件类型） |
//!
//! **注意与 `RM_DEATH(nParam3=1)` 的区别**：`BaseObject.Die()` 发的是
//! `SendRefMsg(RM_DEATH, Dir, CurrX, CurrY, 1, "")`（`BaseObject.Base.cs:351`）⇒ 观察者收
//! `SM_NOWDEATH(34)`（本仓已有发射点）；本片的 `SM_DEATH(32)` 只来自**出现帧**支（`nParam3=0`），
//! 二者是**两条不同生产路径**，不可合并。

/// `RM_PUSH` → 观察者 `SM_BACKSTEP(9)`（`BaseObject.CharPushed`，`BaseObject.cs:871`）。
///
/// oracle 在 `CharPushed` 的 `for` 循环里**每成功推进一格**发一次
/// `SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "")` ⇒ 推 `k` 格产生 `k` 条（保序）。
/// 帧值：`dir = nBackDir`（`GetBackDir(nDir)`，**不是**推进方向）、`x`/`y` = 推进后的 `CurrX/CurrY`。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PushEvent {
    pub actor_id: i32,
    /// 帧里的 `wParam` = `nBackDir`（推进方向的反向）。
    pub dir: u8,
    pub x: i32,
    pub y: i32,
}

/// `RM_RUSH` / `RM_RUSHKUNG` 两族（共用一个 `SendRefMsg` 家族，靠 ident 区分）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RushFrame {
    /// `RM_RUSH` → `SM_RUSH(6)`（向前冲刺一格成功）。
    Rush,
    /// `RM_RUSHKUNG` → `SM_RUSHKUNG(7)`（冲刺被挡且正前不可走）。
    RushKung,
}

/// `RM_RUSH`/`RM_RUSHKUNG` → `SM_RUSH(6)`/`SM_RUSHKUNG(7)`（`PlayObject.DoMotaebo`，`PlayObject.Base.cs:1389-1490`）。
///
/// 两条帧值口径**不同**（逐条照 oracle，别混）：
/// - `Rush`：`SendRefMsg(RM_RUSH, nDir, CurrX, CurrY, 0, "")` ⇒ `dir = nDir`（冲刺方向）、位置 = 推进后格子；
/// - `RushKung`：`SendRefMsg(RM_RUSHKUNG, Dir, nX, nY, 0, "")` ⇒ `dir = Dir`（= `nDir`，DoMotaebo 顶部
///   已 `Dir = nDir`）、位置 = `GetFrontPosition()`（**被挡的正前格**，角色并**不在**该格上）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RushEvent {
    pub actor_id: i32,
    pub frame: RushFrame,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
}

impl RushEvent {
    /// 该事件对应的 `SM_*` ident（`RM_RUSH`→6、`RM_RUSHKUNG`→7）。
    #[must_use]
    pub const fn ident(self) -> u16 {
        match self.frame {
            RushFrame::Rush => 6,
            RushFrame::RushKung => 7,
        }
    }
}

/// `RM_BUTCH` → 观察者 `SM_BUTCH(637)`（`PlayObject.ClientGetButchItem`，`PlayObject.Operate.cs:749`）。
///
/// oracle 末尾**无条件**发 `SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")`；`Dir` 仅当目标在
/// 2 格内且 `IsValidObject` 有效（且有具肉动物尸）时才更新为 `btDir`。**方向解析在宿主**（客户端
/// `CM_BUTCH` 的入站门面，本片不含）；本事件承载**已解析的 `Dir`** 与当前位置。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ButchEvent {
    pub actor_id: i32,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
}
