//! `openmir2-m3-attack-magic-wiring`（**第一片：攻击派发**）：客户端攻击命令
//! （`CM_HIT`/`CM_HEAVYHIT`/`CM_BIGHIT`/`CM_POWERHIT`/`CM_LONGHIT`/`CM_WIDEHIT`/`CM_FIREHIT`/
//! `CM_CRSHIT`/`CM_TWINHIT`）的**纯函数接缝**（不含 I/O / 世界状态）。
//!
//! 移植源（逐条照 oracle，不按常识重推）：
//!
//! - **入帧映射**（`WorldServer.cs:835-844`，与 `CM_TURN/WALK/RUN` 同段）：
//!   `SendMsg(playObject, Ident, Tag, LoWord(Recog), HiWord(Recog), 0)`
//!   ⇒ `nX = LoWord(Recog)`、`nY = HiWord(Recog)`、`nDir = (byte)Tag`。
//! - **派发**（`PlayObject.Message.cs:1221-1230`）：9 条 ident **归一个处理器**
//!   `ClientHitXY(wIdent, nParam1, nParam2, (byte)wParam, LateDelivery, ref delayTime)`；
//!   成功 ⇒ `ActionTick = GetTickCount(); SendSocket(GetGoodTick)`（`+GD` ack）；
//!   失败且 `delayTime == 0` ⇒ `SendRefMsg(RM_MOVEFAIL, …)`（本片未接，只记日志）。
//! - **命中模式表**（`ClientHitXY` 的 `switch (wIdent)`，`PlayObject.Attack.cs:907-939`）：
//!   `CM_HIT→0`、`CM_HEAVYHIT→1`、`CM_BIGHIT→2`、`CM_POWERHIT→3`、`CM_LONGHIT→4`、
//!   `CM_WIDEHIT→5`、`CM_FIREHIT→7`、`CM_CRSHIT→8`、`CM_TWINHIT→9`（无 6）。
//! - **动作帧 ident**（`PlayObject.Attack.cs:78-120` 的 `GetHitMode`）：把 `wHitMode`
//!   映成 `RM_*`，再由观察者的 `PlayObject.Message.cs:1433-1521` 映成 `SM_*`。
//! - **`nX == CurrX && nY == CurrY` 门**（`PlayObject.Attack.cs:884`）：**只有**点在自己所站格
//!   才 `result = true`（随后 `AttackDir(null, wHitMode, nDir)` 取**前方格**的目标）。
//! - **取目标**（`BaseObject.cs:1253-1261` 的 `GetPoseCreate` + `BaseObject.cs:977-1030` 的
//!   `GetFrontPosition`）：`Envir.GetMovingObject(frontX, frontY, true)` = 前方格里**首个**
//!   `!Ghost && !Death` 的对象。
//!
//! # 登记（未接，不静默）
//!
//! - **限速/超速**（`CheckActionStatus` + `AttackTick`/`HitIntervalTime`/`HitSpeed` +
//!   `AttackCount`/`OverSpeedCount`/`BoEmergencyClose`）与 `!boLateDelivery` 支：本片把
//!   `CloseSpeedHackCheck` 视作**生效时的直通**（等价于 `boLateDelivery`），**不实现限速**。
//!   影响面：oracle 在限速不命中时会改发 `RM_MOVEFAIL` 或延时重投，本片恒回 `+GD`（登记）。
//! - **技能/装备态**（`GetHitMode` 的 `PowerHit`/`FireHitSkill`/`MagicArr[SKILL_ERGUM|BANWOL|
//!   CROSSMOON]` 条件）：技能层（`UserMagic`/`MagicArr`）未移植 ⇒ 这些条件恒假，模式 3/4/5/7/8
//!   一律回落 `RM_HIT`（与「无技能角色」的 oracle 结果一致）。
//! - **`Success` 支的体力面**：`HealthTick -= 30` / `SpellTick -= 100` 属体力/魔法点数面，
//!   未移植（`movewire` 同源登记）；`PerHealth -= 2` / `PerSpell -= 2` 已由调用方作用于
//!   `recover::Recovery`。
//! - **`IsProperTarget`/`IsAttackTarget`/`InSafeZone`** 与 PK/红名判定：目标筛选沿用
//!   `GetMovingObject` 的 `!Ghost && !Death`，**不**做安全区/攻击模式过滤（登记）。
//! - **`RM_MOVEFAIL`**：编码器未接（`movewire` 模块头已登记）。

use mir2_protocol::messages;

/// `Direction` 常量（与 [`crate::movewire`] 同源，避免循环依赖只在测试里解引用）。
const DIR_UP: u8 = 0;
const DIR_UPRIGHT: u8 = 1;
const DIR_RIGHT: u8 = 2;
const DIR_DOWNRIGHT: u8 = 3;
const DIR_DOWN: u8 = 4;
const DIR_DOWNLEFT: u8 = 5;
const DIR_LEFT: u8 = 6;
const DIR_UPLEFT: u8 = 7;

/// 本片派发的 9 条攻击 ident（`PlayObject.Message.cs:1221-1230` 的 `case` 列）。
pub const HIT_IDENTS: [u16; 9] = [
    messages::CM_HIT,
    messages::CM_HEAVYHIT,
    messages::CM_BIGHIT,
    messages::CM_POWERHIT,
    messages::CM_LONGHIT,
    messages::CM_WIDEHIT,
    messages::CM_FIREHIT,
    messages::CM_CRSHIT,
    messages::CM_TWINHIT,
];

/// `ident` 是否属于 9 条攻击命令。
#[must_use]
pub fn is_hit_ident(ident: u16) -> bool {
    HIT_IDENTS.contains(&ident)
}

/// `ClientHitXY` 的 `switch (wIdent)`（`PlayObject.Attack.cs:907-939`）：`ident → wHitMode`。
///
/// 非攻击 ident ⇒ `None`（调用方不得猜）。
#[must_use]
pub fn hit_mode(ident: u16) -> Option<i16> {
    match ident {
        messages::CM_HIT => Some(0),
        messages::CM_HEAVYHIT => Some(1),
        messages::CM_BIGHIT => Some(2),
        messages::CM_POWERHIT => Some(3),
        messages::CM_LONGHIT => Some(4),
        messages::CM_WIDEHIT => Some(5),
        messages::CM_FIREHIT => Some(7),
        messages::CM_CRSHIT => Some(8),
        messages::CM_TWINHIT => Some(9),
        _ => None,
    }
}

/// `ClientHitXY` 的成功门（`PlayObject.Attack.cs:884`）：`nX == CurrX && nY == CurrY`。
#[must_use]
pub fn at_self(nx: i32, ny: i32, cur_x: i32, cur_y: i32) -> bool {
    nx == cur_x && ny == cur_y
}

/// `BaseObject.GetFrontPosition`（`BaseObject.cs:977-1030`）：按 `Dir` 取前方相邻格；越界时
/// oracle 的 `nX/nY` **保持 CurrX/CurrY**（每个 `case` 内的边界 `if` 不满足就不动）。
/// 非法 `Dir`（>7）无 `default` ⇒ 同样原地。
#[must_use]
pub fn front_position(dir: u8, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
    match dir {
        DIR_UP if y > 0 => (x, y - 1),
        DIR_UPRIGHT if x < width - 1 && y > 0 => (x + 1, y - 1),
        DIR_RIGHT if x < width - 1 => (x + 1, y),
        DIR_DOWNRIGHT if x < width - 1 && y < height - 1 => (x + 1, y + 1),
        DIR_DOWN if y < height - 1 => (x, y + 1),
        DIR_DOWNLEFT if x > 0 && y < height - 1 => (x - 1, y + 1),
        DIR_LEFT if x > 0 => (x - 1, y),
        DIR_UPLEFT if x > 0 && y > 0 => (x - 1, y - 1),
        _ => (x, y),
    }
}

/// `GetHitMode` 需要的技能/装备运行态（`PlayObject.Attack.cs:78-120` 的条件项）。
///
/// 技能层未移植 ⇒ 生产路径传全 `false`（登记，模块头）：条件不满足时 oracle **保留初值
/// `RM_HIT`**（`int wIdent = Messages.RM_HIT;`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct HitSkillState {
    /// `PlayObject.PowerHit`（攻杀剑术 `SKILL_YEDO` 触发）。
    pub power_hit: bool,
    /// `PlayObject.FireHitSkill`（烈火剑法 `SKILL_FIRESWORD`）。
    pub fire_hit_skill: bool,
    /// `MagicArr[SKILL_ERGUM] != null`（刺杀剑术）。
    pub ergum: bool,
    /// `MagicArr[SKILL_BANWOL] != null`（半月弯刀）。
    pub banwol: bool,
    /// `MagicArr[SKILL_CROSSMOON] != null`（逐日剑法）。
    pub crossmoon: bool,
}

/// `PlayObject.GetHitMode(wHitMode)`（`PlayObject.Attack.cs:78-120`）：`wHitMode → RM_*`。
///
/// 条件不满足时保留初值 `RM_HIT`（**不**回落到同模式，见 oracle 的 `int wIdent = RM_HIT`）。
#[must_use]
pub fn rm_action_ident(mode: i16, skill: HitSkillState) -> u16 {
    match mode {
        1 => messages::RM_HEAVYHIT,
        2 => messages::RM_BIGHIT,
        3 if skill.power_hit => messages::RM_SPELL2,
        4 if skill.ergum => messages::RM_LONGHIT,
        5 if skill.banwol => messages::RM_WIDEHIT,
        7 if skill.fire_hit_skill => messages::RM_FIREHIT,
        8 if skill.crossmoon => messages::RM_CRSHIT,
        9 => messages::RM_TWINHIT,
        _ => messages::RM_HIT,
    }
}

/// 观察者的 `PlayObject.Message.cs:1433-1521`：`RM_* → SM_*`（`GetMessage` 的 `case` 表）。
///
/// 表内遗漏（如 `RM_POWERHIT`）**不猜**：回落 `SM_HIT`（oracle 侧不存在这条 `RM` 的处理器 ⇒
/// 收不到帧；这里只在显式已知的 9 条上调用，回落支仅作防御）。
#[must_use]
pub fn sm_ident_for_rm(rm: u16) -> u16 {
    match rm {
        messages::RM_HEAVYHIT => messages::SM_HEAVYHIT,
        messages::RM_BIGHIT => messages::SM_BIGHIT,
        messages::RM_SPELL2 => messages::SM_POWERHIT,
        messages::RM_LONGHIT => messages::SM_LONGHIT,
        messages::RM_WIDEHIT => messages::SM_WIDEHIT,
        messages::RM_FIREHIT => messages::SM_FIREHIT,
        messages::RM_CRSHIT => messages::SM_CRSHIT,
        messages::RM_TWINHIT => messages::SM_TWINHIT,
        _ => messages::SM_HIT,
    }
}

/// `ClientHitXY` 成功支发出的**动作帧** ident（`GetHitMode` + 观察者映射的合成）。
#[must_use]
pub fn action_sm_ident(mode: i16, skill: HitSkillState) -> u16 {
    sm_ident_for_rm(rm_action_ident(mode, skill))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nine_hit_idents_and_modes() {
        let want = [
            (messages::CM_HIT, 0),
            (messages::CM_HEAVYHIT, 1),
            (messages::CM_BIGHIT, 2),
            (messages::CM_POWERHIT, 3),
            (messages::CM_LONGHIT, 4),
            (messages::CM_WIDEHIT, 5),
            (messages::CM_FIREHIT, 7),
            (messages::CM_CRSHIT, 8),
            (messages::CM_TWINHIT, 9),
        ];
        assert_eq!(HIT_IDENTS.len(), want.len(), "9 条 ident");
        for (ident, mode) in want {
            assert!(is_hit_ident(ident), "ident={ident} 应属攻击");
            assert_eq!(hit_mode(ident), Some(mode), "ident={ident} 的模式");
        }
        // 非攻击 ident 不猜
        assert!(!is_hit_ident(messages::CM_WALK));
        assert_eq!(hit_mode(messages::CM_WALK), None);
        assert_eq!(hit_mode(0), None);
    }

    #[test]
    fn hit_mode_conditions_fall_back_to_rm_hit() {
        let none = HitSkillState::default();
        // 无条件支
        assert_eq!(rm_action_ident(0, none), messages::RM_HIT);
        assert_eq!(rm_action_ident(1, none), messages::RM_HEAVYHIT);
        assert_eq!(rm_action_ident(2, none), messages::RM_BIGHIT);
        assert_eq!(rm_action_ident(9, none), messages::RM_TWINHIT);
        // 条件支：无技能/无 buff ⇒ 回落 RM_HIT（oracle 的初值）
        for mode in [3, 4, 5, 7, 8] {
            assert_eq!(
                rm_action_ident(mode, none),
                messages::RM_HIT,
                "mode={mode} 条件不满足 ⇒ RM_HIT"
            );
        }
        // 条件满足 ⇒ 对应 RM
        let all = HitSkillState {
            power_hit: true,
            fire_hit_skill: true,
            ergum: true,
            banwol: true,
            crossmoon: true,
        };
        assert_eq!(rm_action_ident(3, all), messages::RM_SPELL2);
        assert_eq!(rm_action_ident(4, all), messages::RM_LONGHIT);
        assert_eq!(rm_action_ident(5, all), messages::RM_WIDEHIT);
        assert_eq!(rm_action_ident(7, all), messages::RM_FIREHIT);
        assert_eq!(rm_action_ident(8, all), messages::RM_CRSHIT);
    }

    #[test]
    fn action_sm_ident_matches_observer_table() {
        let none = HitSkillState::default();
        assert_eq!(action_sm_ident(0, none), messages::SM_HIT);
        assert_eq!(action_sm_ident(1, none), messages::SM_HEAVYHIT);
        assert_eq!(action_sm_ident(2, none), messages::SM_BIGHIT);
        assert_eq!(action_sm_ident(9, none), messages::SM_TWINHIT);
        assert_eq!(
            action_sm_ident(5, none),
            messages::SM_HIT,
            "无半月 ⇒ SM_HIT"
        );
        let all = HitSkillState {
            power_hit: true,
            fire_hit_skill: true,
            ergum: true,
            banwol: true,
            crossmoon: true,
        };
        assert_eq!(action_sm_ident(3, all), messages::SM_POWERHIT, "RM_SPELL2");
        assert_eq!(action_sm_ident(5, all), messages::SM_WIDEHIT);
        assert_eq!(action_sm_ident(8, all), messages::SM_CRSHIT);
    }

    #[test]
    fn front_position_clamps_at_border_like_oracle() {
        // 8 向（图 10×10，中心 (5,5)）
        assert_eq!(front_position(DIR_UP, 5, 5, 10, 10), (5, 4));
        assert_eq!(front_position(DIR_UPRIGHT, 5, 5, 10, 10), (6, 4));
        assert_eq!(front_position(DIR_RIGHT, 5, 5, 10, 10), (6, 5));
        assert_eq!(front_position(DIR_DOWNRIGHT, 5, 5, 10, 10), (6, 6));
        assert_eq!(front_position(DIR_DOWN, 5, 5, 10, 10), (5, 6));
        assert_eq!(front_position(DIR_DOWNLEFT, 5, 5, 10, 10), (4, 6));
        assert_eq!(front_position(DIR_LEFT, 5, 5, 10, 10), (4, 5));
        assert_eq!(front_position(DIR_UPLEFT, 5, 5, 10, 10), (4, 4));
        // 边界：原地（oracle 的 `if` 不满足 ⇒ nX/nY 保持 CurrX/CurrY）
        assert_eq!(front_position(DIR_UP, 5, 0, 10, 10), (5, 0));
        assert_eq!(front_position(DIR_LEFT, 0, 5, 10, 10), (0, 5));
        assert_eq!(front_position(DIR_RIGHT, 9, 5, 10, 10), (9, 5));
        assert_eq!(front_position(DIR_DOWN, 5, 9, 10, 10), (5, 9));
        // 对角：任一条件不满足即原地
        assert_eq!(front_position(DIR_UPRIGHT, 9, 5, 10, 10), (9, 5));
        assert_eq!(front_position(DIR_UPLEFT, 0, 5, 10, 10), (0, 5));
        // 非法方向：原地
        assert_eq!(front_position(8, 5, 5, 10, 10), (5, 5));
    }

    #[test]
    fn at_self_only_when_same_tile() {
        assert!(at_self(287, 619, 287, 619));
        assert!(!at_self(288, 619, 287, 619));
        assert!(!at_self(287, 618, 287, 619));
    }
}
