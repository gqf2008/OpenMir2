//! 卡 `openmir2-gateway-frame-criticality-queue`（总监 2026-11-15 裁决）：下行帧的**关键性分类**。
//!
//! 网关发送口按「丢了会不会把会话废掉」把帧分成两条通道：
//!
//! - **关键（Critical，默认）**：登录/会话建立、地图与实体存在性、背包/装备/经济、对话/交易、
//!   聊天/系统/行会/组队等**一次性状态**。丢一帧客户端就永久不同步（或会话不可用）⇒ 进有界
//!   队列（A），队列满按会话阶段分流（见 [`crate::gate_queue`] / `lib.rs::queue_or_reject`）。
//! - **非关键（NonCritical）**：**AOI 位置/动作/状态类**——移动、转身、攻击动作、外观/状态广播、
//!   伤害飘字、HP/MP 状态条。它们是**下一 tick / 下一次状态变化会被覆盖**的瞬时展示帧；丢一帧
//!   最多晚一次刷新，**绝不因此关会话** ⇒ 完全绕开队列：socket 能收就直发，收不了立即丢弃 + 计数。
//!
//! ## 判据来源（逐条 oracle，不拍脑袋）
//!
//! 分类只收「由 AOI 广播/动作中继产生、且可由后续帧覆盖」的 ident。oracle 的发送侧证据是
//! `src/M2Server/Player/PlayObject.Message.cs` 里 `RM_*` → `SM_*` 的映射，以及广播点
//! `src/M2Server/Actor/BaseObject.Message.cs::SendRefMsg`（±`SendRefMsgRange` 格内广播；见
//! `docs/M2-S3世界下行-AOI同屏-20261011.md` §2 的口径链）。
//!
//! | wire ident | SM_* | oracle 产生点 | 为什么可丢（覆盖来源） |
//! | --- | --- | --- | --- |
//! | 4 | `SM_41` | `RM_41` → `PlayObject.Message.cs:1503-1509` | AOI 动作中继；动作序列继续推进 |
//! | 5 | 线上 `SM_HORSERUN` | `RM_HORSERUN` → `:1413-1422`；线上 ident 见 D-15（`crate::movewire::SM_HORSERUN_CLIENT`） | 移动步；下一步/下一帧位置覆盖 |
//! | 6 | `SM_RUSH` | `RM_RUSH` → `:1526, 1536-1541` | 冲刺动作；下一步位置覆盖 |
//! | 7 | `SM_RUSHKUNG` | `RM_RUSHKUNG` → `:1527, 1538-1541` | 同上 |
//! | 8 | `SM_FIREHIT` | `RM_FIREHIT` → `:1489-1495` | 攻击动作动画 |
//! | 9 | `SM_BACKSTEP` | `RM_PUSH` → `:1525, 1533-1535` | 后退动作；下一步位置覆盖 |
//! | 10 | `SM_TURN` | `RM_TURN` → `:1524-1560` | 朝向；每次转身/AOI 重现帧重发 |
//! | 11 | `SM_WALK` | `RM_WALK` → `:1403-1412` | 移动步；下一步位置覆盖 |
//! | 13 | `SM_RUN` | `RM_RUN` → `:1423-1432` | 移动步；下一步位置覆盖 |
//! | 14 | `SM_HIT` | `RM_HIT` → `:1433-1439` | 攻击动作动画 |
//! | 15 | `SM_HEAVYHIT` | `RM_HEAVYHIT` → `:1440-1446` | 攻击动作动画 |
//! | 16 | `SM_BIGHIT` | `RM_BIGHIT` → `:1447-1453` | 攻击动作动画 |
//! | 17 | `SM_SPELL` | `RM_SPELL` → `:1454-1460` | 施法动作动画 |
//! | 18 | `SM_POWERHIT` | `RM_SPELL2` → `:1461-1467` | 攻击动作动画 |
//! | 19 | `SM_LONGHIT` | `RM_LONGHIT` → `:1475-1481` | 攻击动作动画 |
//! | 20 | `SM_DIGUP` | `RM_DIGUP` → `:1891-1898` | 动作动画 |
//! | 21 | `SM_DIGDOWN` | `RM_DIGDOWN` → `:1900-1903` | 动作动画 |
//! | 22 | `SM_FLYAXE` | `RM_FLYAXE` → `:1904-1915` | 飞斧动作动画 |
//! | 23 | `SM_LIGHTING` | `RM_LIGHTING` → `:1916-1934` | 闪电动作动画 |
//! | 24 | `SM_WIDEHIT` | `RM_WIDEHIT` → `:1482-1488` | 攻击动作动画 |
//! | 25 | `SM_CRSHIT` | `RM_CRSHIT` → `:1496-1502` | 攻击动作动画 |
//! | 26 | `SM_TWINHIT` | `RM_TWINHIT` → `:1510-1516` | 攻击动作动画 |
//! | 28 | `SM_MOVEFAIL` | `RM_MOVEFAIL` → `:1468-1474` | 位置纠正；下一次失败/成功帧覆盖 |
//! | 31 | `SM_STRUCK` | `RM_STRUCK` → `:1562-1620` | 受击飘字；HP 由 53 覆盖（见下） |
//! | 41 | `SM_FEATURECHANGED` | `RM_TURN` → `:1552-1560`（另 `RM_FEATURECHANGED` `:1762-1764`） | 外观；每次转身重发 |
//! | 43 | `SM_43` | `RM_43` → `:1517-1523` | AOI 动作中继 |
//! | 53 | `SM_HEALTHSPELLCHANGED` | `RM_HEALTHSPELLCHANGED` → `:1736-1739`；产生点 `BaseObject.cs:700-710`（`HealthSpellChanged`） | HP/MP 状态条；每次 HP/MP 变化重发，且 `SendUpdateMsg` 自身去重只留最新一条（`BaseObject.Message.cs:283-304`） |
//! | 657 | `SM_CHARSTATUSCHANGED` | `RM_CHARSTATUSCHANGED` → `:1845-1847`；产生点 `BaseObject.cs:1516` / `PlayObject.cs:2876` | 状态位广播；每次状态变化重发，`SendUpdateMsg` 去重 |
//!
//! **刻意不在非关键集合里**（虽同属 AOI 广播，但**丢了不会被覆盖**——是持久状态跃迁，进关键通道）：
//! `SM_ALIVE(27)`、`SM_HIDE(29)`、`SM_DISAPPEAR(30)`、`SM_DEATH(32)`、`SM_SKELETON(33)`、
//! `SM_NOWDEATH(34)`、`SM_USERNAME(42)`、`SM_ITEMSHOW(610)/SM_ITEMHIDE(611)`、`SM_BUTCH(637)`、
//! `SM_CHANGELIGHT(654)`、`SM_CHANGENAMECOLOR(656)`、`SM_SPACEMOVE_*`。理由同一句：它们是
//! 「出现/消亡/改名/地面物品存在性」，客户端没有第二次机会从后续帧自愈。
//!
//! ## 分类只对 `GM_DATA` 的**内层命令头**生效
//!
//! 外层 `ServerMessage.ident` 的 `GM_*` 管理号（`GM_CLOSE=2`、`GM_CHECKSERVER=3`、
//! `GM_CHECKCLIENT=4`、`GM_DATA=5`、`GM_SERVERUSERINDEX=6`、`GM_RECEIVE_OK=7`、`GM_STOP=8`）
//! 与 `SM_*` 的前几个值**数值相撞**（`messages.rs:714-722` vs `:727-741`）。因此调用方必须先由
//! [`crate::wire_identity`] 判定「这是 `GM_DATA` + `PackLength >= 12` 的命令帧」，否则一律按
//! **关键**处理（控制帧绝不能被当成 AOI 丢）。
//!
//! 未知 ident（含未来新增的 SM_*）默认关键：宁可排队/拒绝，也不静默丢一次性状态帧。

/// 一条下行帧所属的通道。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Channel {
    /// 一次性状态/控制帧：进有界队列，溢出按会话阶段分流。
    Critical,
    /// AOI 位置/动作/状态帧：绕开队列，收不了立即丢（有计数）。
    NonCritical,
}

/// 非关键 ident 集合（**升序**，[`is_non_critical_ident`] 用二分）。
///
/// 只收 AOI 瞬时展示帧，逐条依据见模块文档表。新增/删除条目必须同步改文档证据表与
/// `gate_criticality::tests::non_critical_set_is_the_aoi_transient_family`。
pub(crate) const NON_CRITICAL_IDENTS: &[u16] = &[
    4,   // SM_41（RM_41）
    5,   // 线上 SM_HORSERUN（RM_HORSERUN；D-15）
    6,   // SM_RUSH
    7,   // SM_RUSHKUNG
    8,   // SM_FIREHIT
    9,   // SM_BACKSTEP（RM_PUSH）
    10,  // SM_TURN
    11,  // SM_WALK
    13,  // SM_RUN
    14,  // SM_HIT
    15,  // SM_HEAVYHIT
    16,  // SM_BIGHIT
    17,  // SM_SPELL
    18,  // SM_POWERHIT（RM_SPELL2）
    19,  // SM_LONGHIT
    20,  // SM_DIGUP
    21,  // SM_DIGDOWN
    22,  // SM_FLYAXE
    23,  // SM_LIGHTING
    24,  // SM_WIDEHIT
    25,  // SM_CRSHIT
    26,  // SM_TWINHIT
    28,  // SM_MOVEFAIL
    31,  // SM_STRUCK
    41,  // SM_FEATURECHANGED
    43,  // SM_43
    53,  // SM_HEALTHSPELLCHANGED
    657, // SM_CHARSTATUSCHANGED
];

/// `ident` 是否属于非关键（AOI 瞬时）帧。**不看帧形状**；调用方必须先用
/// [`crate::wire_identity`] 的 `is_command` 守住「外层不是 `GM_DATA` 控制帧」再调用。
#[must_use]
pub(crate) fn is_non_critical_ident(ident: u16) -> bool {
    NON_CRITICAL_IDENTS.binary_search(&ident).is_ok()
}

/// 分类入口：`inner_command` 为假（控制帧/短包/非法帧）⇒ 恒关键。
///
/// 未知 ident ⇒ 关键（默认安全）。
#[must_use]
pub(crate) fn channel_of(ident: u16, inner_command: bool) -> Channel {
    if inner_command && is_non_critical_ident(ident) {
        Channel::NonCritical
    } else {
        Channel::Critical
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_protocol::messages;

    /// 集合**逐值**钉住：多一个/少一个/顺序错都红。
    ///
    /// 为什么用整表相等而不是逐条 `contains`：逐条只证明「列出的都在」，删掉一条仍然绿
    /// （reviewer 的「牙不能回答空输入」口径）。整表相等同时钉住排序（二分前提）。
    #[test]
    fn non_critical_set_is_the_aoi_transient_family() {
        let expected: &[u16] = &[
            messages::SM_41,
            crate::movewire::SM_HORSERUN_CLIENT,
            messages::SM_RUSH,
            messages::SM_RUSHKUNG,
            messages::SM_FIREHIT,
            messages::SM_BACKSTEP,
            messages::SM_TURN,
            messages::SM_WALK,
            messages::SM_RUN,
            messages::SM_HIT,
            messages::SM_HEAVYHIT,
            messages::SM_BIGHIT,
            messages::SM_SPELL,
            messages::SM_POWERHIT,
            messages::SM_LONGHIT,
            messages::SM_DIGUP,
            messages::SM_DIGDOWN,
            messages::SM_FLYAXE,
            messages::SM_LIGHTING,
            messages::SM_WIDEHIT,
            messages::SM_CRSHIT,
            messages::SM_TWINHIT,
            messages::SM_MOVEFAIL,
            messages::SM_STRUCK,
            messages::SM_FEATURECHANGED,
            messages::SM_43,
            messages::SM_HEALTHSPELLCHANGED,
            messages::SM_CHARSTATUSCHANGED,
        ];
        assert_eq!(NON_CRITICAL_IDENTS, expected, "非关键集合必须逐值一致");
        let mut sorted = expected.to_vec();
        sorted.sort_unstable();
        assert_eq!(
            NON_CRITICAL_IDENTS,
            sorted.as_slice(),
            "集合必须升序（二分前提）"
        );
    }

    /// 持久状态跃迁 / 登录控制 / 聊天经济 **必须**关键（丢了不会被覆盖）。
    #[test]
    fn persistent_and_login_frames_stay_critical() {
        for ident in [
            0u16, 27,  // SM_ALIVE
            29,  // SM_HIDE
            30,  // SM_DISAPPEAR
            32,  // SM_DEATH
            33,  // SM_SKELETON
            34,  // SM_NOWDEATH
            42,  // SM_USERNAME
            50,  // SM_LOGON
            51,  // SM_NEWMAP
            52,  // SM_ABILITY
            100, // SM_SYSMESSAGE
            200, // SM_ADDITEM
            201, // SM_BAGITEMS
            202, // SM_DELITEM
            203, // SM_UPDATEITEM
            500, // SM_CERTIFICATION_SUCCESS
            633, // SM_CLEAROBJECTS
            634, // SM_CHANGEMAP
            637, // SM_BUTCH
            642, // SM_DURACHANGE
            654, // SM_CHANGELIGHT
            656, // SM_CHANGENAMECOLOR
            610, // SM_ITEMSHOW
            611, // SM_ITEMHIDE
            800, // SM_SPACEMOVE_HIDE
            801, // SM_SPACEMOVE_SHOW
            9999,
        ] {
            assert_eq!(
                channel_of(ident, true),
                Channel::Critical,
                "ident {ident} 必须走关键通道"
            );
        }
    }

    /// 外层 `GM_*` 管理号与 `SM_*` 数值相撞：**非命令帧恒关键**，否则 `GM_SERVERUSERINDEX(6)`
    /// 会被当成 `SM_RUSH` 丢掉（控制帧静默消失）。
    #[test]
    fn control_frames_are_never_classified_non_critical() {
        for outer in [
            messages::grobal2::GM_CLOSE,           // 2
            messages::grobal2::GM_CHECKSERVER,     // 3
            messages::grobal2::GM_CHECKCLIENT,     // 4
            messages::grobal2::GM_SERVERUSERINDEX, // 6
            messages::grobal2::GM_RECEIVE_OK,      // 7
            messages::grobal2::GM_STOP,            // 8
        ] {
            assert_eq!(
                channel_of(u16::from(outer), false),
                Channel::Critical,
                "外层 GM_{outer} 不是命令帧，不得进非关键通道"
            );
        }
        // 同一数值：命令帧（内层 SM_*）才是非关键。
        assert_eq!(
            channel_of(u16::from(messages::grobal2::GM_SERVERUSERINDEX), true),
            Channel::NonCritical,
            "内层 ident=6 是 SM_RUSH，命令帧按非关键"
        );
    }

    /// D-15：线上骑乘跑 ident 是 5（冻结客户端 `Common_Grobal2.pas:328`），C# 的 5010 不上线。
    /// Rust 生产侧从不发 C# 的 `SM_THROW`（`messages::SM_THROW == 5` 只是常量撞号），故 5 归非关键。
    #[test]
    fn wire_ident_five_is_horserun_per_d15() {
        assert_eq!(messages::SM_THROW, 5);
        assert_eq!(crate::movewire::SM_HORSERUN_CLIENT, 5);
        assert_eq!(channel_of(5, true), Channel::NonCritical);
        // C# 参照常量 5010 不在线上帧里；若真出现，默认按关键（未知一律保守）。
        assert_eq!(messages::SM_HORSERUN, 5010);
        assert_eq!(channel_of(5010, true), Channel::Critical);
    }
}
