//! 客户端独有消息号 —— 冻结客户端会发、但服务端消息号表（`Messages.cs`）里**没有**的号。
//!
//! 这些号由**冻结客户端**定义（唯一真源：`E:\Users\gxh\Documents\GitHub\MirClient\Source`），
//! C# 服务端对其**无处理器**：LoginSrv 的 `ProcessUserMsg` switch 落到 default ⇒ 静默忽略、不回包
//! （LoginGate 只是原样转发）。**行为等价要求同样忽略**，但号本身必须登记在册，
//! 否则回放门禁会把它们报成"未实现包号"（M0 验收③）。
//!
//! 来源：M0 金标准抓包第 1 帧（真实客户端 `#1…!`，ident=3501，体 8 字节 = `"password"`）
//! 首次暴露；对照客户端源码确认。

/// `CM_QUERYDYNCODE`（客户端 `Source/Common/Grobal2.pas:1264`）。
///
/// 客户端在 `g_ConnectionStep = cnsIntro` 时发送（`ClMain.pas:16542`），
/// 体是 `EncodeString(g_LoginKey)`；`g_LoginKey` 的默认值是字面量 `"password"`
/// （`Source/MirClient/MShare.pas:1407`），启动参数可改写。
///
/// 服务端侧：`src/OpenMir2/Messages.cs` 无此常量，C# 静默忽略 ⇒ Rust 侧同样忽略、不回包。
pub const CM_QUERYDYNCODE: u16 = 3501;

/// `CM_WANTVIEWRANGE`（客户端 `Grobal2.pas:1093`）：进世界后与 `CM_QUERYBAGITEMS`(81)
/// 同一串自动查询里发出；服务端无处理器 ⇒ 忽略。
pub const CM_WANTVIEWRANGE: u16 = 84;

/// `CM_HIDEDEATHBODY`（客户端 `Grobal2.pas:1211`）：隐藏尸体；服务端无处理器 ⇒ 忽略。
pub const CM_HIDEDEATHBODY: u16 = 1200;

/// `CM_HEROSIDESTEP`（客户端 `Grobal2.pas:1214`）：英雄侧步；服务端无处理器 ⇒ 忽略。
pub const CM_HEROSIDESTEP: u16 = 10431;

/// `CM_HEROSERIESSKILLCONFIG`（客户端 `Grobal2.pas:1216`）：英雄连击技能配置；
/// 服务端无处理器 ⇒ 忽略。
pub const CM_HEROSERIESSKILLCONFIG: u16 = 10433;

/// 该号是否为"客户端独有、服务端静默忽略"的消息号。
#[must_use]
pub fn is_client_only(ident: u16) -> bool {
    name_of(ident).is_some()
}

/// 客户端独有号的可读名（回放报告用）。
#[must_use]
pub fn name_of(ident: u16) -> Option<&'static str> {
    match ident {
        CM_QUERYDYNCODE => Some("CM_QUERYDYNCODE"),
        CM_WANTVIEWRANGE => Some("CM_WANTVIEWRANGE"),
        CM_HIDEDEATHBODY => Some("CM_HIDEDEATHBODY"),
        CM_HEROSIDESTEP => Some("CM_HEROSIDESTEP"),
        CM_HEROSERIESSKILLCONFIG => Some("CM_HEROSERIESSKILLCONFIG"),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::messages;

    /// 3501 是客户端独有号：服务端表里**必须**没有它（否则说明 C# 源长出了新号，
    /// 该走 msgcodegen 重新生成而不是留在这里）。
    #[test]
    fn querydyncode_is_client_only_not_server() {
        assert!(is_client_only(CM_QUERYDYNCODE));
        // C2 金标准带出的 4 个客户端独有号（服务端表里同样必须不存在）
        for id in [
            CM_WANTVIEWRANGE,
            CM_HIDEDEATHBODY,
            CM_HEROSIDESTEP,
            CM_HEROSERIESSKILLCONFIG,
        ] {
            assert!(is_client_only(id), "ident {id} 应登记为客户端独有");
            assert!(
                messages::names_of(id).is_empty(),
                "ident {id} 出现在服务端消息号表里，应走 msgcodegen 重新生成而不是 client_only"
            );
        }
        assert!(messages::names_of(CM_QUERYDYNCODE).is_empty());
        assert_eq!(name_of(CM_QUERYDYNCODE), Some("CM_QUERYDYNCODE"));
        // 邻近的服务端号仍在表内（锚定 3500 = CM_SPEEDHACKMSG）
        assert_eq!(messages::CM_SPEEDHACKMSG, 3500);
        assert!(!is_client_only(3500));
    }
}
