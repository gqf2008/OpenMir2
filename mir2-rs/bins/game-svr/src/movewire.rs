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
//! `RunIntervalTime` + `MoveCount`/`OverSpeedCount`/`BoEmergencyClose`）、失败支的
//! `SendRefMsg(RM_MOVEFAIL)`（需 `sm_movement_fail` 编码器）、`IsCanWalk`/`IsCanRun` 的状态位面、
//! `SpaceMoved`（地图路由换图）。
//!
//! **已接（卡 `openmir2-m3-scoped-diff-ability`）**：成功支的体力/魔力点消耗——
//! `ClientWalkXY` 的 `HealthTick -= 10`（`PlayObject.Attack.cs:1424`）由 `handle_move_cmd` 调
//! `stamina::Stamina::on_walk_success`；`ClientRunXY`/`ClientHorseRunXY` 的
//! `HealthTick -= 60; SpellTick -= 10; SpellTick = max(0,·); PerHealth -= 1; PerSpell -= 1`
//! （`:1352-1356`/`:1041-1045`）由 `on_run_success` + `recover::Recovery` 的 `per_*` 回绕减接管
//! （见 `bins/game-svr/src/stamina.rs`）。
//!
//! **`Death`/`STONE` 守卫**（`PlayObject.Operate.cs:312`、`PlayObject.Attack.cs:1373/1297/980`）：
//! `Death` 已接（`handle_move_cmd` 读 `mir2_world_bevy::Entity.death`，为真则整支拒绝）；
//! `STONE = StatusTimeArr[PoisonState.STONE] != 0` 属**麻痹/状态位面，未移植** ⇒ **未接登记**
//! （`mir2_world_bevy::Entity` 无该字段，不造一份假状态）。

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
/// **Grobal2 / Delphi 谱系的骑乘跑 ident**（D-* 协议兼容别名）。
///
/// oracle `src/OpenMir2/Messages.cs:545` 只定义 `CM_HORSERUN = 3035`；**3009 在 oracle 里未分配**，
/// 但 MirClientZig（按 Grobal2.pas）按 3009 发包 ⇒ 真机上服务端当未知消息丢弃，
/// 坐标不再更新、攻击随后被 `at_self` 拒绝（Owner「骑马不掉血」根因）。
/// 本别名只新增一条兼容路径，**不改变 3011/3013/3035/3010 的既有语义**。
pub const CM_HORSERUN_LEGACY: u16 = 3009;

/// 冻结客户端谱系的**线上** `SM_HORSERUN` ident（D-15：以冻结客户端为准）。
///
/// 冻结客户端 `Common_Grobal2.pas:328` 定义 `SM_HORSERUN = 5`，`ClMain.pas:9910` 把它与
/// `SM_WALK`/`SM_RUN` 放同一支解码（尾 `TCharDesc`、`series = dir+light`），`Actor2.pas:475`
/// 按 `SM_HORSERUN` 设 3 格。oracle `Messages.cs:436` 的 `SM_HORSERUN = 5010` **从未上线**
/// （oracle 侧 `HorseType` 无赋值点 ⇒ 该帧永不发射），且 C# 低号段与冻结客户端本就不同源
/// （C# `SM_THROW = 5` vs 客户端 `SM_THROW = 65069`）⇒ 线上必须发客户端认得的 5，
/// 否则帧被当未知 ident 丢弃（旁观者看不到骑乘跑）。
/// 登记见 `mir2-rs/tests/parity/whitelist.md` D-15；C# 常量表保持不变（msgcodegen 对拍）。
pub const SM_HORSERUN_CLIENT: u16 = 5;

/// 是否骑乘跑（oracle 3035 + Grobal2 兼容别名 3009）。
#[must_use]
pub const fn is_horse_run(ident: u16) -> bool {
    matches!(ident, messages::CM_HORSERUN | CM_HORSERUN_LEGACY)
}

/// **生产派发门**：这条 CI 命令是否交给 `handle_move_cmd`（含 3009 别名）。
///
/// 卡 `openmir2-horserun-3009-not-routed`：上一批把 `is_horse_run` 只写在 `handle_move_cmd`
/// **内部**，而 `sim_loop` 的外层派发门仍是手写的 `matches!(ident, CM_TURN|CM_WALK|CM_RUN|
/// CM_HORSERUN)` ⇒ 3009 在派发层被吞，谓词单测全绿、线上却不处理。本函数是**唯一**的派发
/// 判据，`sim_loop` 必须调它（接线牙见 `move_dispatch_gate_uses_is_move_command_in_sim_loop`）。
#[must_use]
pub const fn is_move_command(ident: u16) -> bool {
    matches!(
        ident,
        messages::CM_TURN | messages::CM_WALK | messages::CM_RUN
    ) || is_horse_run(ident)
}

#[must_use]
pub const fn steps_for(ident: u16) -> i32 {
    match ident {
        messages::CM_RUN => 2,
        messages::CM_HORSERUN | CM_HORSERUN_LEGACY => 3,
        _ => 1,
    }
}

/// `CM_*` → 同义的 `SM_*` 下行帧 ident（`PlayObject.Message.cs:1403-1432`）。
#[must_use]
pub const fn sm_ident_for(ident: u16) -> u16 {
    match ident {
        messages::CM_RUN => messages::SM_RUN,
        messages::CM_HORSERUN | CM_HORSERUN_LEGACY => SM_HORSERUN_CLIENT,
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

/// `ClientWalkXY`/`ClientRunXY`/`ClientHorseRunXY` 的 **result 门**
/// （`PlayObject.Attack.cs:1420`/`:1348`/`:1037`）：`SpaceMoved || (CurrX == nX && CurrY == nY)`。
///
/// `px`/`py` = **移动后**坐标，`nx`/`ny` = 请求目标坐标。本仓库**没有** `SpaceMoved`（地图路由换图
/// `EnterAnotherMap`/`ChangeSpaceMove` 未移植）⇒ `SpaceMoved` 那一半**已登记**（模块头），本门
/// 退化为 `CurrX == nX && CurrY == nY`。
///
/// `result == false` 时 oracle **不回** `+GD`（`delayTime == 0` 时改发 `RM_MOVEFAIL`，未接）；
/// 但移动帧广播在 `WalkTo`/`RunTo` 内部的 `Walk(RM_WALK/RM_RUN)` 里**已发出**，与本门无关。
#[must_use]
pub const fn move_result_ok(px: i32, py: i32, nx: i32, ny: i32) -> bool {
    px == nx && py == ny
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
        assert_eq!(sm_ident_for(messages::CM_HORSERUN), SM_HORSERUN_CLIENT);
        assert_eq!(
            SM_HORSERUN_CLIENT, 5,
            "冻结客户端 Common_Grobal2.pas:328 只认 5"
        );
        assert_ne!(
            SM_HORSERUN_CLIENT,
            messages::SM_HORSERUN,
            "oracle 的 5010 客户端不认（D-15），不得上线"
        );
    }

    #[test]
    fn turn_ack_only_at_target() {
        assert!(turn_is_at_target(287, 619, 287, 619));
        assert!(!turn_is_at_target(288, 619, 287, 619));
        assert!(!turn_is_at_target(287, 620, 287, 619));
    }

    #[test]
    fn move_result_gate_is_at_target() {
        // 移动后坐标 == 请求目标 ⇒ result 真（回 `+GD`）
        assert!(move_result_ok(11, 10, 11, 10));
        // 请求目标非相邻（走 1 格后仍在半路）⇒ result 假（不回 `+GD`）
        assert!(!move_result_ok(12, 10, 14, 10));
        assert!(!move_result_ok(11, 10, 11, 13));
    }

    #[test]
    fn good_tick_payload_shape() {
        assert_eq!(good_tick_payload(1234), b"+GD/1234".to_vec());
    }
    /// **派发门牙（卡 `openmir2-horserun-3009-not-routed`）**：3009 必须能从生产派发门
    /// （`sim_loop` → `handle_move_cmd`）进入移动支；3010/3011/3013/3035 不变；非移动命令不误入。
    ///
    /// 红检：删掉 `is_move_command` 的 `is_horse_run` 半支 ⇒ 本用例红。
    #[test]
    fn move_command_gate_includes_legacy_horserun() {
        for ident in [
            messages::CM_TURN,
            messages::CM_WALK,
            messages::CM_RUN,
            messages::CM_HORSERUN,
            CM_HORSERUN_LEGACY,
        ] {
            assert!(
                is_move_command(ident),
                "ident={ident} 必须由 handle_move_cmd 处理"
            );
        }
        for ident in [messages::CM_HIT, messages::CM_SITDOWN, 0u16] {
            assert!(!is_move_command(ident), "ident={ident} 不得进移动支");
        }
    }

    /// **D-* 别名牙**：3009 必须按骑乘跑处理（步数 3），且与 3035 同判定；
    /// walk/run 不受影响。
    #[test]
    fn horserun_legacy_alias_steps_and_predicates() {
        assert!(is_horse_run(CM_HORSERUN_LEGACY), "3009 必须被识别为骑乘跑");
        assert!(is_horse_run(messages::CM_HORSERUN));
        assert!(!is_horse_run(messages::CM_WALK));
        assert!(!is_horse_run(messages::CM_RUN));
        assert_eq!(
            sm_ident_for(CM_HORSERUN_LEGACY),
            SM_HORSERUN_CLIENT,
            "3009 成功移动的广播帧必须是客户端认得的 SM_HORSERUN(5)，不能落到 SM_WALK、也不是 oracle 的 5010"
        );
        assert_eq!(steps_for(CM_HORSERUN_LEGACY), 3, "3009 每轴 ±3");
        assert_eq!(steps_for(messages::CM_HORSERUN), 3);
        assert_eq!(steps_for(messages::CM_RUN), 2);
        assert_eq!(steps_for(messages::CM_WALK), 1);
    }
    /// **MOVEFAIL 完整帧体牙**：12B 头 + **EDCode(CharDesc)**。
    /// 客户端 onMoveFail 会 setFeature(desc.feature)，body 缺 CharDesc ⇒ 外观被重置。
    #[test]
    fn movefail_body_includes_char_desc_and_layout() {
        let desc = game_svr::downlink::char_desc(1234, 77);
        let body = game_svr::smframe::movefail_body(42, 100, 200, 5, 1234, 77);
        let encoded_len = mir2_protocol::edcode::encode(&desc).len();
        assert_eq!(body.len(), 12 + encoded_len, "12B 头 + EDCode(CharDesc)");
        assert_eq!(i32::from_le_bytes(body[0..4].try_into().unwrap()), 42);
        assert_eq!(
            u16::from_le_bytes([body[4], body[5]]),
            mir2_protocol::messages::SM_MOVEFAIL
        );
        assert_eq!(u16::from_le_bytes([body[6], body[7]]), 100, "param=x");
        assert_eq!(u16::from_le_bytes([body[8], body[9]]), 200, "tag=y");
        assert_eq!(
            u16::from_le_bytes([body[10], body[11]]),
            5,
            "series = dir（高字节恒 0；oracle 第 5 参是 byte Dir，不带 light）"
        );
        // 尾必须与 downlink::char_desc(feature,status) 经 EDCode 编码后的结果一致
        assert_eq!(
            &body[12..],
            mir2_protocol::edcode::encode(&desc).as_slice(),
            "body 尾必须是 EDCode(CharDesc)，否则客户端外观被重置"
        );
    }
}
