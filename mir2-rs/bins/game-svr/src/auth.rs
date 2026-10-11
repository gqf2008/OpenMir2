//! "准入会话表" —— C# `GameSrv/Services/AuthenticationService.cs` 的 `_sessionList` 对应物。
//!
//! 语义照抄 `:283-297`（`NewSession`）、`:299-327`（`DelSession`）、`:338-368`（`GetAdmission`）：
//! - `NewSession`：**追加**一条（`RefCount = 1`）；
//! - `DelSession`：按 `SessionId` 线性扫、**删第一个匹配**（`:306-316`）；
//! - `GetAdmission`：按 `SessionId == id && Account == account` 匹配（**不看 IP**，`:348`），
//!   并把 `PayMent` 映射成对外口径：`2→3 / 1→2 / 0→1 / 其它→0`（`:350-361`）。
//!   调用方要求 `nPayMent > 0` 才算通过（`ChannelMessageHandler.cs:315`）⇒ `PayMent ∉ {0,1,2}` 视为不通过。

/// 本切片用到的 `AccountSession` 字段（`GameSrv` 侧那条表）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountSession {
    pub account: String,
    pub ipaddr: String,
    pub session_id: i32,
    pub pay_ment: i32,
    pub pay_mode: i32,
    pub play_time: i64,
    /// C# `RefCount`（`NewSession` 置 1；本切片不增减，登记为 S2b 的对齐点之一）。
    pub ref_count: i32,
}

/// 匹配结果（`GetAdmission` 的 ref 参数）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Admission {
    pub session: AccountSession,
    /// 映射后的 `nPayMent`（`PayMent ∉ {0,1,2}` ⇒ 0）。
    pub pay_ment: i32,
    pub pay_mode: i32,
    pub play_time: i64,
}

impl Admission {
    /// `ChannelMessageHandler.cs:315` 的判定：`sessInfo != null && nPayMent > 0`。
    #[must_use]
    pub fn is_admitted(&self) -> bool {
        self.pay_ment > 0
    }
}

/// append-only 的列表 + 线性查找（照 oracle 的 `List<AccountSession>`）。
#[derive(Debug, Default)]
pub struct AdmissionStore {
    list: Vec<AccountSession>,
}

impl AdmissionStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `NewSession`（`:283-297`）。
    ///
    /// **S2b 的调用方**是登录链的通知（`AuthenticationService.Run` 的 `(...)`/`/` 分帧，见卡内登记）——
    /// 那段传输落地前，本方法只有单测在用，故显式允许"尚未接线"。
    #[allow(dead_code)]
    pub fn new_session(
        &mut self,
        account: &str,
        ipaddr: &str,
        session_id: i32,
        pay_ment: i32,
        pay_mode: i32,
        play_time: i64,
    ) {
        self.list.push(AccountSession {
            account: account.to_string(),
            ipaddr: ipaddr.to_string(),
            session_id,
            pay_ment,
            pay_mode,
            play_time,
            ref_count: 1,
        });
    }

    /// `DelSession(sessionId)`（`:299-327`）：删第一个 `SessionId` 匹配的（返回它，供调用方踢人）。
    #[allow(dead_code)]
    pub fn del_session(&mut self, session_id: i32) -> Option<AccountSession> {
        let pos = self.list.iter().position(|s| s.session_id == session_id)?;
        Some(self.list.remove(pos))
    }

    /// `ClearSession()`（`:329-335`）：整张表清空（oracle 在 IDS 断线时调它，见 `:427-436`）。
    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.list.clear();
    }

    /// `GetAdmission`（`:338-368`）。
    #[must_use]
    pub fn get_admission(&self, account: &str, session_id: i32) -> Option<Admission> {
        for s in &self.list {
            if s.session_id == session_id && s.account == account {
                let pay_ment = match s.pay_ment {
                    2 => 3,
                    1 => 2,
                    0 => 1,
                    _ => 0,
                };
                return Some(Admission {
                    session: s.clone(),
                    pay_ment,
                    pay_mode: s.pay_mode,
                    play_time: s.play_time,
                });
            }
        }
        None
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.list.len()
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.list.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn match_requires_session_id_and_account_and_ignores_ip() {
        let mut s = AdmissionStore::new();
        s.new_session("mir2test", "10.0.0.1", 77, 0, 1, 100);
        assert!(s.get_admission("mir2test", 77).is_some());
        assert!(
            s.get_admission("mir2test", 78).is_none(),
            "SessionId 必须相同"
        );
        assert!(s.get_admission("other", 77).is_none(), "Account 必须相同");
        // IP 不参与匹配（oracle `:348` 只看 SessionId + Account）
        let a = s.get_admission("mir2test", 77).unwrap();
        assert_eq!(a.session.ipaddr, "10.0.0.1");
    }

    #[test]
    fn pay_ment_mapping_and_admitted_rule() {
        let mut s = AdmissionStore::new();
        for (pay, want) in [(0, 1), (1, 2), (2, 3), (3, 0), (9, 0)] {
            s.new_session("a", "ip", 100 + pay, pay, 0, 0);
            let got = s.get_admission("a", 100 + pay).unwrap();
            assert_eq!(got.pay_ment, want, "PayMent={pay} 的映射");
            assert_eq!(got.is_admitted(), want > 0);
        }
    }

    #[test]
    fn del_session_removes_first_match_only() {
        let mut s = AdmissionStore::new();
        s.new_session("a", "ip1", 5, 0, 0, 0);
        s.new_session("b", "ip2", 5, 0, 0, 0);
        let removed = s.del_session(5).unwrap();
        assert_eq!(removed.account, "a", "删第一个匹配（`RemoveAt(i)`）");
        assert_eq!(s.len(), 1);
        assert!(s.del_session(404).is_none());
    }
}
