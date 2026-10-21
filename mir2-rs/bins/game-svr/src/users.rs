//! 网关侧用户槽表 —— C# `GateInfo.UserList`（`ChannelMessageHandler.cs:154-198`）的对应物。
//!
//! 三条必须照抄的语义：
//! 1. **最小空闲槽复用**：`OpenNewUser`（`:434-444`）先线性找第一个 `null` 槽并占用，找不到才 `Add`；
//!    关闭（`:404`）把槽置 `null` ⇒ 关掉再开**复用最小空闲下标**（不是往后长）。
//!    ⚠️ 这是 S1 交付后按 oracle 源码复核发现的偏差（原实现写成 append-only），本提交修正。
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
    /// 是否已完成认证（C# `SessionUser.Certification`；认证链在 `cert.rs`）。
    pub certification: bool,
    /// C# `SessionUser.Account`（认证通过后才有）。
    pub account: String,
    /// C# `SessionUser.ChrName`。
    pub chr_name: String,
    /// C# `SessionUser.SessionID`（认证链里的 `nSessionId`，与网关的 `session_id` 不是一回事）。
    pub game_session_id: i32,
    /// 是否**已进世界**（C# `SessionUser.PlayObject != null && WorldEngine != null` 的对应物）。
    /// 这是 `GM_DATA` 分支的**真正门**（`:201`）——不是 `Certification`（reviewer-2 的 F3）。
    pub in_world: bool,
}

/// 网关用户槽表（最小空闲槽复用；`open` 与 `OpenNewUser` 同构）。
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
    ///
    /// `OpenNewUser`（`:434-444`）：先找**最小空闲槽**复用，没有才追加。
    pub fn open(&mut self, socket: i32, session_id: u16, ip: String) -> i32 {
        let user = GateUser {
            socket,
            session_id,
            ip,
            index: 0, // 下面按落位填
            certification: false,
            account: String::new(),
            chr_name: String::new(),
            game_session_id: 0,
            in_world: false,
        };
        let slot = self.slots.iter().position(Option::is_none);
        let idx = match slot {
            Some(i) => {
                self.slots[i] = Some(GateUser {
                    index: i32::try_from(i).unwrap_or(i32::MAX) + 1,
                    ..user
                });
                i
            }
            None => {
                let i = self.slots.len();
                self.slots.push(Some(GateUser {
                    index: i32::try_from(i).unwrap_or(i32::MAX) + 1,
                    ..user
                }));
                i
            }
        };
        i32::try_from(idx).unwrap_or(i32::MAX) + 1
    }

    /// 认证通过后落字段（C# `DoClientCertification` `:317-322` 的四项：`Certification/Account/ChrName/SessionID`）。
    ///
    /// **Trim 口径照 oracle `:318-319`**：`gateUser.Account = sAccount.Trim()` / `ChrName = sChrName.Trim()`
    /// ⇒ 这里存的 `Account`/`ChrName` 是 **trim 过**的。
    /// 注意**不能**把同一口径搬到 `LoadDBInfo`：oracle `:325-326` 给 `LoadDBInfo.Account/ChrName` 的是
    /// `sAccount`/`sChrName` **原值**（未 trim）⇒ `lib.rs` 里发给 DBSvr 的取角色请求用未 trim 原值。
    ///
    /// 进世界（`in_world`）由 S2b-3 在取到角色后置真。
    ///
    /// # Panics
    /// 不会 panic。
    pub fn certify(
        &mut self,
        socket: i32,
        account: &str,
        chr_name: &str,
        game_session_id: i32,
    ) -> bool {
        for u in self.slots.iter_mut().flatten() {
            if u.socket == socket {
                u.certification = true;
                // `:318-319`：`gateUser.Account = sAccount.Trim()` / `ChrName = sChrName.Trim()`
                u.account = account.trim().to_string();
                u.chr_name = chr_name.trim().to_string();
                u.game_session_id = game_session_id;
                return true;
            }
        }
        false
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

    /// 关断前读该 socket 已认证的角色名（未认证/不存在 ⇒ 空串）。
    ///
    /// `GM_CLOSE` 分支里 `close_by_socket` 会把槽置空，所以角色名必须在关断**之前**读；
    /// 这个名字是 `openmir2-m2-session-lifecycle`「断线清世界」的输入。
    #[must_use]
    pub fn chr_name_by_socket(&self, socket: i32) -> String {
        self.slots
            .iter()
            .flatten()
            .find(|u| u.socket == socket)
            .map(|u| u.chr_name.clone())
            .unwrap_or_default()
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

    /// 进世界完成：把该 socket 的槽标成 `in_world`（oracle 的 `gateUser.PlayObject`/`WorldEngine` 挂上时）。
    pub fn mark_in_world(&mut self, socket: i32) -> bool {
        for u in self.slots.iter_mut().flatten() {
            if u.socket == socket {
                u.in_world = true;
                return true;
            }
        }
        false
    }

    /// 槽位总数（含已关闭的空槽——下标只增不缩，对外索引=下标+1）。
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
    fn index_is_one_based_and_reuses_smallest_free_slot() {
        let mut l = UserList::new();
        assert_eq!(l.open(10, 111, "1.1.1.1".into()), 1);
        assert_eq!(l.open(11, 222, "1.1.1.2".into()), 2);
        assert_eq!(l.close_by_socket(11), Some(2));
        // 关掉再开：**复用最小空闲槽**（oracle `:434-441`），不是往后长
        assert_eq!(l.open(12, 333, "1.1.1.3".into()), 2);
        assert_eq!(l.slots(), 2, "槽数不增长（复用了 0 号空槽）");
        assert_eq!(l.online(), 2);
        // 再关一个（socket=10 → 下标 0）后开：复用的还是最小空闲（下标 0）
        assert_eq!(l.close_by_socket(10), Some(1));
        assert_eq!(l.open(13, 444, "1.1.1.4".into()), 1);
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

    /// N2（reviewer-3）：`certify` 落 `Account`/`ChrName` 照 oracle `:318-319` **trim**。
    #[test]
    fn certify_trims_account_and_chr_name() {
        let mut l = UserList::new();
        l.open(10, 111, "a".into());
        assert!(l.certify(10, " loadbot1 ", "\tlb1a\n", 7));
        let u = l.find(1, 10).unwrap();
        assert_eq!(u.account, "loadbot1");
        assert_eq!(u.chr_name, "lb1a");
        assert!(u.certification);
        assert_eq!(u.game_session_id, 7);
        // 不存在的 socket ⇒ 不动任何槽
        assert!(!l.certify(99, "x", "y", 1));
    }
}
