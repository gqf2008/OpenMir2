//! `openmir2-m3-move-wiring`：客户端移动命令（`CM_TURN`/`CM_WALK`/`CM_RUN`/`CM_HORSERUN`）的
//! **纯函数接缝**（不含 I/O / 世界状态）。
//!
//! 移植源（逐条照 oracle，不按常识重推）：
//!
//! - `WorldServer.ProcessUserMessage`（`src/GameSrv/Word/WorldServer.cs:831-838`）：四个 ident 共用
//!   一条入帧映射 `SendMsg(Ident, Tag, LoWord(Recog), HiWord(Recog), 0)` ⇒ 目标坐标
//!   `nX = LoWord(Recog)`、`nY = HiWord(Recog)`、方向 `nDir = (byte)Tag`。
//!   （whitelist **T-6** 已实测钉住：`Recog = MakeLong(X,Y)`、`Tag = 方向`、`Param = 0`、`Series = 0`；
//!   C3 金标准锚 `0x026B011F` = `X=287,Y=619`、`Tag=4`。）
//! - `PlayObject.Message.cs:1008-1210` 的四个 `case` 及其实现：
//!   `ClientChangeDir`（`PlayObject.Operate.cs:310-348`）/
//!   `ClientWalkXY`（`PlayObject.Attack.cs:1365-1435`）/
//!   `ClientRunXY`（`:1288-1345`）/
//!   `ClientHorseRunXY`（`:970-1040`）。
//! - `M2Share.GetNextDirection`（`src/M2Server/M2Share.cs:299-378`）：由 `(sx,sy) → (dx,dy)` 求 8 向。
//!
//! **本片未接（登记，别按常识补）**：限速/超速（`CheckActionStatus` + `MoveTick`/`WalkIntervalTime`/
//! `RunIntervalTime` + `MoveCount`/`OverSpeedCount`/`BoEmergencyClose`）、`ClientWalkXY` 的
//! `HealthTick -= 10` / `ClientRunXY` 的 `HealthTick -= 60`（体力/饥饿面未移植）、失败支的
//! `SendRefMsg(RM_MOVEFAIL)`（需 `sm_movement_fail` 编码器）、`IsCanWalk`/`IsCanRun` 的状态位面。

use mir2_protocol::messages;

/// `Direction`（`src/OpenMir2/Enums/Direction.cs:5-12`）。
pub const DIR_UP: u8 = 0;
/// `Direction.UpRight`
pub const DIR_UPRIGHT: u8 = 1;
/// `Direction.Right`
pub const DIR_RIGHT: u8 = 2;
/// `Direction.DownRight`
pub const DIR_DOWNRIGHT: u8 = 3;
/// `Direction.Down`
pub const DIR_DOWN: u8 = 4;
/// `Direction.DownLeft`
pub const DIR_DOWNLEFT: u8 = 5;
/// `Direction.Left`
pub const DIR_LEFT: u8 = 6;
/// `Direction.UpLeft`
pub const DIR_UPLEFT: u8 = 7;

/// `M2Share.GetNextDirection(short sx, short sy, short dx, short dy)`（`src/M2Server/M2Share.cs:299-378`）。
///
/// 逐行照抄：先按 `sx`/`sy` 与 `dx`/`dy` 的大小定 `flagx`/`flagy`（`-1/0/1`），
/// 再做两处「远距离时把横向/纵向压平」的修正（`|sy-dy|>2` 且 `sx` 落在 `dx±1` ⇒ `flagx=0`；
/// `|sx-dx|>2` 且 `sy` 落在 `dy-1..dy+1` ⇒ `flagy=0`），最后按 `(flagx,flagy)` 查 8 向。
/// 无匹配（不可能）时 oracle 返回 `Down` ⇒ 这里同样回落 `DIR_DOWN`。
#[must_use]
pub fn next_direction(sx: i32, sy: i32, dx: i32, dy: i32) -> u8 {
    let mut flagx = match sx.cmp(&dx) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => -1,
    };
    if (sy - dy).abs() > 2 && sx >= dx - 1 && sx <= dx + 1 {
        flagx = 0;
    }
    let mut flagy = match sy.cmp(&dy) {
        std::cmp::Ordering::Less => 1,
        std::cmp::Ordering::Equal => 0,
        std::cmp::Ordering::Greater => -1,
    };
    if (sx - dx).abs() > 2 && sy > dy - 1 && sy <= dy + 1 {
        flagy = 0;
    }
    match (flagx, flagy) {
        (0, -1) => DIR_UP,
        (1, -1) => DIR_UPRIGHT,
        (1, 0) => DIR_RIGHT,
        (1, 1) => DIR_DOWNRIGHT,
        (0, 1) => DIR_DOWN,
        (-1, 1) => DIR_DOWNLEFT,
        (-1, 0) => DIR_LEFT,
        (-1, -1) => DIR_UPLEFT,
        _ => DIR_DOWN,
    }
}

/// `Direction` → 单位步进 `(dx, dy)`（`BaseObject.WalkTo` / `PlayObject.RunTo` / `HorseRunTo` 的
/// `switch (btDir)`；`Dir=0` 上、顺时针 1..7）。
///
/// 非法 `dir`（`>7`）在 oracle 的 `switch` 里**无 default** ⇒ `newX/newY` 保持 0（落 `(0,0)` 的
/// 越界支）；本函数对非法值返回 `(0,0)`，`try_step_now` 会拒绝 `(0,0)`（等价于「不动」）。
#[must_use]
pub const fn direction_delta(dir: u8) -> (i32, i32) {
    match dir {
        DIR_UP => (0, -1),
        DIR_UPRIGHT => (1, -1),
        DIR_RIGHT => (1, 0),
        DIR_DOWNRIGHT => (1, 1),
        DIR_DOWN => (0, 1),
        DIR_DOWNLEFT => (-1, 1),
        DIR_LEFT => (-1, 0),
        DIR_UPLEFT => (-1, -1),
        _ => (0, 0),
    }
}

/// 各移动 ident 的**格数**（`WalkTo` 走 1、`RunTo` 跑 2、`HorseRunTo` 骑跑 3）。
#[must_use]
pub const fn steps_for(ident: u16) -> i32 {
    match ident {
        messages::CM_RUN => 2,
        messages::CM_HORSERUN => 3,
        _ => 1,
    }
}

/// `CM_*` → 同义的 `SM_*` 下行帧 ident（`PlayObject.Message.cs:1403-1432`）。
#[must_use]
pub const fn sm_ident_for(ident: u16) -> u16 {
    match ident {
        messages::CM_RUN => messages::SM_RUN,
        messages::CM_HORSERUN => messages::SM_HORSERUN,
        _ => messages::SM_WALK,
    }
}

/// `ClientChangeDir` 的成功条件（`PlayObject.Operate.cs:330-347`）：**只有** `nX == CurrX && nY == CurrY`
/// 才 `Dir = nDir` + `Walk(RM_TURN)` + 回 `+GD`；否则返回 false（调用方在 `delayTime == 0` 时发
/// `RM_MOVEFAIL`）。
#[must_use]
pub const fn turn_is_at_target(nx: i32, ny: i32, cur_x: i32, cur_y: i32) -> bool {
    nx == cur_x && ny == cur_y
}

/// `M2Share.GetGoodTick`（`src/M2Server/M2Share.cs:205`）= `string.Format(MessageSettings.sSTATUS_GOOD,
/// GetTickCount())`，而 `sSTATUS_GOOD = "+GD/{0}"`（`src/Modules/SystemModule/MessageSettings.cs:6`）
/// ⇒ 被接受的客户端动作回的**短包**正文（`SendSocket(string)`：`PackLength = -len`，网关端变成
/// `'#' + 正文 + '!'`，即真机观测到的 `#+GD/<tick>!`）。
#[must_use]
pub fn good_tick_payload(tick_ms: i64) -> Vec<u8> {
    format!("+GD/{tick_ms}").into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn next_direction_eight_ways_and_flattening() {
        // 8 个正方向（相邻格）
        assert_eq!(next_direction(10, 10, 10, 9), DIR_UP);
        assert_eq!(next_direction(10, 10, 11, 9), DIR_UPRIGHT);
        assert_eq!(next_direction(10, 10, 11, 10), DIR_RIGHT);
        assert_eq!(next_direction(10, 10, 11, 11), DIR_DOWNRIGHT);
        assert_eq!(next_direction(10, 10, 10, 11), DIR_DOWN);
        assert_eq!(next_direction(10, 10, 9, 11), DIR_DOWNLEFT);
        assert_eq!(next_direction(10, 10, 9, 10), DIR_LEFT);
        assert_eq!(next_direction(10, 10, 9, 9), DIR_UPLEFT);
        // `|sy-dy| > 2` 且 `sx ∈ dx±1` ⇒ flagx 压平（横向不再偏）
        assert_eq!(next_direction(10, 10, 11, 14), DIR_DOWN);
        // `|sx-dx| > 2` 且 `sy ∈ (dy-1, dy+1]` ⇒ flagy 压平（oracle 这里是非对称的 `> dy-1`）
        assert_eq!(next_direction(10, 12, 5, 11), DIR_LEFT);
        // `sy = dy-1` 不命中（照抄 oracle 的 `>`）——连这个边界一起搬
        assert_eq!(next_direction(10, 10, 5, 11), DIR_DOWNLEFT);
        // 同格 ⇒ flagx=flagy=0 ⇒ 落 `Down`（无匹配支）
        assert_eq!(next_direction(10, 10, 10, 10), DIR_DOWN);
    }

    #[test]
    fn direction_delta_is_unit_clockwise() {
        assert_eq!(direction_delta(DIR_UP), (0, -1));
        assert_eq!(direction_delta(DIR_UPRIGHT), (1, -1));
        assert_eq!(direction_delta(DIR_RIGHT), (1, 0));
        assert_eq!(direction_delta(DIR_DOWNRIGHT), (1, 1));
        assert_eq!(direction_delta(DIR_DOWN), (0, 1));
        assert_eq!(direction_delta(DIR_DOWNLEFT), (-1, 1));
        assert_eq!(direction_delta(DIR_LEFT), (-1, 0));
        assert_eq!(direction_delta(DIR_UPLEFT), (-1, -1));
        assert_eq!(direction_delta(8), (0, 0), "非法方向不猜");
    }

    #[test]
    fn steps_and_sm_ident_map() {
        assert_eq!(steps_for(messages::CM_WALK), 1);
        assert_eq!(steps_for(messages::CM_RUN), 2);
        assert_eq!(steps_for(messages::CM_HORSERUN), 3);
        assert_eq!(sm_ident_for(messages::CM_WALK), messages::SM_WALK);
        assert_eq!(sm_ident_for(messages::CM_RUN), messages::SM_RUN);
        assert_eq!(sm_ident_for(messages::CM_HORSERUN), messages::SM_HORSERUN);
    }

    #[test]
    fn turn_ack_only_at_target() {
        assert!(turn_is_at_target(287, 619, 287, 619));
        assert!(!turn_is_at_target(288, 619, 287, 619));
        assert!(!turn_is_at_target(287, 620, 287, 619));
    }

    #[test]
    fn good_tick_payload_shape() {
        assert_eq!(good_tick_payload(1234), b"+GD/1234".to_vec());
    }
}
