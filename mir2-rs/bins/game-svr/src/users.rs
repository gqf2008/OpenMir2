//! 网关侧用户槽表 —— C# `GateInfo.UserList`（`ChannelMessageHandler.cs:154-198`）的对应物。
//!
//! 三条必须照抄的语义：
//! 1. **append-only**：`OpenNewUser` 是 `userList.Add(...)` 后返回 `Count-1`（`:442-444`），
//!    关闭只把槽位置空、**不回收下标** ⇒ 关掉再开，下标继续往后长。
//! 2. **对外索引 = 下标 + 1**：`:157` 传 `nUserIdx + 1` 给 `SendNewUserMsg`，`:458` 原样写进头的 `SessionIndex`
//!    ⇒ 网关侧 `SessionIndex >= 1` 才有效（`:172`）。
//! 3. 找用户：**先按 `SessionIndex - 1` 找、且必须 `socket` 相同**；否则**线性扫 socket**（`:170-198`）。
//! 4. `GM_CLOSE` 是按 **socket** 关（`:161`），不是按 SessionId。

/// 一个网关用户槽。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateUser {
    /// 客户端 socket 句柄（网关头里的 `Socket`；C# 用它做关断与兜底查找）。
    pub socket: i32,
    /// 网关分配的 SessionId（随机 4 位）。
    pub session_id: u16,
    /// `GM_OPEN` 体里的客户端 IP 字符串。
    pub ip: String,
    /// 对外索引（= 内部下标 + 1）。
    pub index: i32,
    /// 是否已完成认证（C# `SessionUser.Certification`；S2 才会置真）。
    pub certification: bool,
}

/// append-only 的槽表。
#[derive(Debug, Default)]
pub struct UserList {
    slots: Vec<Option<GateUser>>,
}

impl UserList {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `GM_OPEN`：登记新用户，返回**对外索引（下标+1）**。
    pub fn open(&mut self, socket: i32, session_id: u16, ip: String) -> i32 {
        let idx = i32::try_from(self.slots.len()).unwrap_or(i32::MAX);
        self.slots.push(Some(GateUser {
            socket,
            session_id,
            ip,
            index: idx + 1,
            certification: false,
        }));
        idx + 1
    }

    /// `GM_CLOSE`：按 socket 关（`ChannelMessageHandler.cs:161` → `CloseUser(socket)`）。
    pub fn close_by_socket(&mut self, socket: i32) -> Option<i32> {
        for slot in &mut self.slots {
            if let Some(u) = slot {
                if u.socket == socket {
                    let idx = u.index;
                    *slot = None;
                    return Some(idx);
                }
            }
        }
        None
    }

    /// `GM_DATA` 的收件人查找：先 `SessionIndex-1`（且 socket 必须一致），否则线性扫 socket。
    #[must_use]
    pub fn find(&self, session_index: i32, socket: i32) -> Option<&GateUser> {
        if session_index >= 1 {
            let idx = usize::try_from(session_index - 1).unwrap_or(usize::MAX);
            if let Some(Some(u)) = self.slots.get(idx) {
                if u.socket == socket {
                    return Some(u);
                }
            }
        }
        self.slots.iter().flatten().find(|u| u.socket == socket)
    }

    /// 槽位总数（含已关闭的空槽——照 append-only 事实，日志口径用它）。
    #[must_use]
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    /// 在线数。
    #[must_use]
    pub fn online(&self) -> usize {
        self.slots.iter().filter(|s| s.is_some()).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_is_one_based_and_append_only() {
        let mut l = UserList::new();
        assert_eq!(l.open(10, 111, "1.1.1.1".into()), 1);
        assert_eq!(l.open(11, 222, "1.1.1.2".into()), 2);
        assert_eq!(l.close_by_socket(11), Some(2));
        // 关掉再开：下标继续长（不回收）——照 oracle 的 `userList.Add`
        assert_eq!(l.open(12, 333, "1.1.1.3".into()), 3);
        assert_eq!(l.slots(), 3);
        assert_eq!(l.online(), 2);
    }

    #[test]
    fn find_prefers_session_index_then_falls_back_to_socket() {
        let mut l = UserList::new();
        l.open(10, 111, "a".into());
        l.open(11, 222, "b".into());
        assert_eq!(
            l.find(2, 11).unwrap().session_id,
            222,
            "SessionIndex-1 命中"
        );
        // SessionIndex 指向别的槽（socket 不一致）⇒ 退回 socket 扫
        assert_eq!(l.find(1, 11).unwrap().session_id, 222, "socket 兜底");
        assert_eq!(
            l.find(0, 11).unwrap().session_id,
            222,
            "SessionIndex<1 直接兜底"
        );
        assert!(l.find(9, 99).is_none());
    }
}
