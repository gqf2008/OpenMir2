//! 运行期共享状态 —— 1:1 对齐 C# `src/LoginSrv/LsShare.cs`
//! （`SessionManager` / `ClientManager` / `CertUser` / 会话与网关台账）。
//!
//! 并发模型对齐 C#：**网关消息由单一消费者串行处理**（`LoginServer._messageQueue` 的单
//! 消费者），状态用 `Mutex` 保护只为跨任务（5600 的数据服务 + 定时器）共享，不引入并行语义。

use std::collections::HashMap;
use std::sync::atomic::{AtomicI32, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use mir2_storage::AccountStore;
use tokio::sync::{mpsc, Mutex};

use crate::config::{Config, LimitServerUserInfo};

/// `HUtil32.GetTickCount()`：`Environment.TickCount` 语义（毫秒、`int` 回绕）。
#[must_use]
#[allow(clippy::cast_possible_truncation)] // 回绕即语义（镜像 int32 TickCount）
pub fn tick() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u32
}

/// `LsShare.VersionDate`（`CM_PROTOCOL` 的版本判据）。
pub const VERSION_DATE: i32 = 2001_1006;

/// C# `SessionConnInfo`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SessionConnInfo {
    /// 账号。
    pub account: String,
    /// 客户端 IP。
    pub ipaddr: String,
    /// 已选定的服务器名（选服后由 `UpdateSession` 写入）。
    pub server_name: String,
    /// 会话号。
    pub session_id: i32,
    /// 是否点卡/月卡付费。
    pub pay_cost: bool,
    /// 服务器计费模式（`PayModel > 0`）。
    pub is_payment: bool,
    /// 踢线时刻。
    pub kick_tick: u32,
    /// 会话建立时刻。
    pub start_tick: u32,
    /// 是否已下踢线指令。
    pub kicked: bool,
}

/// C# `CertUser`（点卡/月卡认证表条目）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CertUser {
    /// 账号。
    pub login_id: String,
    /// 地址。
    pub addr: String,
    /// 服务器名。
    pub server_name: String,
    /// 免费模式。
    pub free_mode: bool,
    /// 认证号。
    pub certification: i32,
    /// 开卡时刻。
    pub open_time: u32,
    /// 是否已关闭。
    pub closing: bool,
    /// 可用类型。
    pub available_type: u8,
    /// 按天。
    pub id_day: i64,
    /// 按小时。
    pub id_hour: i64,
    /// IP 天。
    pub ip_day: i64,
    /// IP 小时。
    pub ip_hour: i64,
}

/// C# `LoginSrv/LsShare.cs::UserInfo`（一个网关连接下的客户端会话）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserInfo {
    /// 账号（登录成功前为空串 —— C# 用它判"是否已登录"）。
    pub account: String,
    /// 客户端 IP。
    pub user_ipaddr: String,
    /// 网关 IP。
    pub gate_ipaddr: String,
    /// 会话索引（网关给的 `SocketId` 字符串）。
    pub sock_index: String,
    /// 会话号。
    pub session_id: i32,
    /// 付费账号。
    pub pay_cost: bool,
    /// 0 体验 / 1 收费 / 2 免费。
    pub pay_mode: u8,
    /// 认证号。
    pub certification: i32,
    /// 剩余游戏秒数。
    pub seconds: i64,
    /// 可用类型。
    pub available_type: u8,
    /// 是否已选服。
    pub sel_server: bool,
    /// 接入时刻。
    pub client_tick: u32,
    /// 上次建号时刻（限频）。
    pub last_create_account_tick: u32,
    /// 上次改密时刻。
    pub last_update_pwd_tick: u32,
    /// 上次找回密码时刻。
    pub last_get_back_pwd_tick: u32,
    /// 上次更新资料时刻。
    pub last_update_account_tick: u32,
}

/// C# `LoginGateInfo`（客户端面连接在本进程侧的台账）。
#[derive(Debug, Clone)]
pub struct LoginGateInfo {
    /// `LsShare.GetGatePublicAddr` 结果（对外网关地址）。
    pub sipaddr: String,
    /// 该网关下的客户端会话。
    pub user_list: Vec<UserInfo>,
    /// 心跳时刻。
    pub keepalive_tick: u32,
    /// 回写该网关连接的通道（C# 直接 `Socket.Send`）。
    pub tx: mpsc::UnboundedSender<Vec<u8>>,
}

/// C# `SessionServer.ServerSessionInfo`（5600 上连过来的 DBSrv / GameSvr）。
#[derive(Debug, Clone)]
pub struct ServerSessionInfo {
    /// 连接标识。
    pub socket_id: String,
    /// 下行写通道（替代 C# `Socket`）。
    pub tx: mpsc::UnboundedSender<Vec<u8>>,
    /// 对端地址（日志用）。
    pub endpoint: String,
    /// 服务器名（`SS_SERVERINFO` 上报）。
    pub server_name: String,
    /// 服务器序号（99 = DBSrv）。
    pub server_index: i32,
    /// 在线人数。
    pub online_count: i32,
    /// 心跳时刻。
    pub keepalive_tick: u32,
    /// 计费模式（0 免费 / 1 试玩 / 2 测试 / 3 付费）。
    pub payment_mode: i32,
    /// 该服务器上的会话（`IS_*` 用）。
    pub session_list: Vec<SessionConnInfo>,
    /// 粘包缓冲（C# `ReceiveMsg`）。
    pub receive_msg: String,
    /// 关闭通知（C# `Socket.Close()`）。
    pub handle: crate::session_server::ServerHandle,
}

/// C# `SessionManager`：`sessionMap`（按会话号）+ `sessionAccountMap`（按账号）。
///
/// C# 两张表存的是**同一个对象引用**（`UpdateSession` 改一处两处都变）⇒ 这里第二张表只存
/// 会话号（`账号 → 会话号`），读取时经 `sessionMap` 解析，避免"两份副本各改一半"。
#[derive(Debug, Default)]
pub struct Sessions {
    map: HashMap<i32, SessionConnInfo>,
    account_map: HashMap<String, i32>,
}

impl Sessions {
    /// `AddSession`：C# 两处 `Add` 遇重复键会抛（调用方保证不重复）。
    pub fn add(&mut self, session_id: i32, info: SessionConnInfo) {
        self.account_map.insert(info.account.clone(), session_id);
        self.map.insert(session_id, info);
    }

    /// `GetSession(account)`：C# 用索引器（缺键抛异常）⇒ 这里返回 `Option`。
    #[must_use]
    pub fn get_by_account(&self, account: &str) -> Option<&SessionConnInfo> {
        self.account_map
            .get(account)
            .and_then(|id| self.map.get(id))
    }

    /// `UpdateSession`。
    pub fn update(&mut self, session_id: i32, server_name: &str, is_payment: bool) {
        if let Some(c) = self.map.get_mut(&session_id) {
            c.server_name = server_name.to_string();
            c.is_payment = is_payment;
        }
    }

    /// `IsLogin(int sessionId)`。
    #[must_use]
    pub fn is_login_id(&self, session_id: i32) -> bool {
        self.map.contains_key(&session_id)
    }

    /// `IsLogin(string account)`。
    #[must_use]
    pub fn is_login_account(&self, account: &str) -> bool {
        self.account_map.contains_key(account)
    }

    /// `DeleteByAccount`：顶号时立即释放账号占用。
    pub fn delete_by_account(&mut self, account: &str) {
        if let Some(session_id) = self.account_map.remove(account) {
            self.map.remove(&session_id);
        }
    }

    /// `Delete(account, sessionId)`。
    pub fn delete(&mut self, account: &str, session_id: i32) {
        if self.map.remove(&session_id).is_some() {
            self.account_map.remove(account);
        }
    }

    /// `GetSessions()`（快照）。
    #[must_use]
    pub fn snapshot(&self) -> Vec<SessionConnInfo> {
        self.map.values().cloned().collect()
    }

    /// `SessionKick` 的写回：`connInfo.Kicked = true; connInfo.KickTick = tick;`
    pub fn mark_kicked(&mut self, account: &str, kick_tick: u32) {
        if let Some(session_id) = self.account_map.get(account).copied() {
            if let Some(m) = self.map.get_mut(&session_id) {
                m.kicked = true;
                m.kick_tick = kick_tick;
            }
        }
    }
}

/// LoginSrv 全局共享（C# 的 `LsShare` 静态面 + 各单例服务）。
pub struct Share {
    /// 只读配置。
    pub cfg: Config,
    /// `Config.sGateIPaddr`（每条网关消息后刷新）。
    pub gate_ipaddr: Mutex<String>,
    /// `GateRoute[i].nSelIdx`（轮询游标）。
    pub sel_idx: Mutex<Vec<usize>>,
    /// `UserLimit`（100 槽，尾部 `None` 复刻 NRE 语义）。
    pub user_limit: Mutex<Vec<Option<LimitServerUserInfo>>>,
    /// 5600 上连的服务器（共享：数据服务写、登录侧读）。
    pub servers: Mutex<Vec<ServerSessionInfo>>,
    /// `ClientManager._gateInfos`。
    pub gates: Mutex<HashMap<String, LoginGateInfo>>,
    /// `SessionManager`。
    pub sessions: Mutex<Sessions>,
    /// `LsShare.SessionIdx`。
    pub session_idx: AtomicI32,
    /// `LsShare.CertList`。
    pub cert_list: Mutex<Vec<CertUser>>,
    /// `LsShare.OnlineCountMin`。
    pub online_min: AtomicI32,
    /// `LsShare.OnlineCountMax`。
    pub online_max: AtomicI32,
    /// 账号存储（`AccountStorage`）。
    pub accounts: Mutex<AccountStore>,
}

impl Share {
    /// 构造。
    #[must_use]
    pub fn new(cfg: Config, accounts: AccountStore) -> Self {
        let n = cfg.routes.len();
        let limits = cfg.user_limit.clone();
        Self {
            cfg,
            gate_ipaddr: Mutex::new(String::new()),
            sel_idx: Mutex::new(vec![0; n]),
            user_limit: Mutex::new(limits),
            servers: Mutex::new(Vec::new()),
            gates: Mutex::new(HashMap::new()),
            sessions: Mutex::new(Sessions::default()),
            session_idx: AtomicI32::new(1), // LsShare.Initialization()
            cert_list: Mutex::new(Vec::new()),
            online_min: AtomicI32::new(0),
            online_max: AtomicI32::new(0),
            accounts: Mutex::new(accounts),
        }
    }

    /// `LsShare.GetSessionId()`：先自增，`>= int.MaxValue` 回绕到 2。
    #[must_use]
    pub fn next_session_id(&self) -> i32 {
        let v = self.session_idx.fetch_add(1, Ordering::SeqCst) + 1;
        if v == i32::MAX {
            self.session_idx.store(2, Ordering::SeqCst);
            2
        } else {
            v
        }
    }
}

/// 构造共享状态。
#[must_use]
pub fn new_share(cfg: Config, accounts: AccountStore) -> Arc<Share> {
    Arc::new(Share::new(cfg, accounts))
}

/// `LsShare.VerifyAccountRule`（1:1，含 `i <= nLen` 的越界缺陷 ⇒ 以 `Err` 表达"C# 抛异常"）。
///
/// # Errors
/// 与 C# 相同输入的 `IndexOutOfRangeException` 路径（`i == nLen` 时读 `account[i]`）。
pub fn verify_account_rule(account: &str) -> Result<bool, IndexOutOfRange> {
    if account.is_empty() {
        return Ok(false);
    }
    let cs: Vec<char> = account.chars().collect();
    let n_len = cs.len();
    let mut result = true;
    let mut i = 0usize;
    loop {
        if i >= n_len {
            break;
        }
        if cs[i] < '0' || cs[i] > 'z' {
            result = false;
            if cs[i] >= '\u{b0}' && cs[i] <= '\u{e8}' {
                i += 1;
                if i <= n_len {
                    // C# `account[i]`：i == nLen 时越界抛 IndexOutOfRangeException
                    let c = *cs.get(i).ok_or(IndexOutOfRange)?;
                    if ('?'..='t').contains(&c) {
                        result = true;
                    }
                }
            }
            if !result {
                break;
            }
        }
        i += 1;
    }
    Ok(result)
}

/// C# `IndexOutOfRangeException` 的镜像（用于 `VerifyAccountRule` 的越界路径）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexOutOfRange;

impl std::fmt::Display for IndexOutOfRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("IndexOutOfRangeException")
    }
}

impl std::error::Error for IndexOutOfRange {}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网账号（ASCII）都必须通过；中文账号按 C# 语义拒绝。
    #[test]
    fn account_rule_matches_csharp_shape() {
        for ok in ["mir2test", "loadbot1", "a0", "0z", "test_1", "A"] {
            assert!(verify_account_rule(ok).unwrap(), "{ok} 应通过");
        }
        // '0'..'z' 之外的 ASCII（'.' = 0x2E、'{' = 0x7B）按 C# 语义拒绝
        assert!(!verify_account_rule("a.b").unwrap());
        assert!(!verify_account_rule("{").unwrap());
        assert!(!verify_account_rule("").unwrap());
        assert!(!verify_account_rule("中文账号").unwrap());
        assert!(verify_account_rule("z").unwrap());
    }

    /// 会话台账：Add/IsLogin/DeleteByAccount/Delete 的键口径（会话号 vs 账号）。
    #[test]
    fn session_manager_keys() {
        let mut s = Sessions::default();
        s.add(
            7,
            SessionConnInfo {
                account: "acc".into(),
                session_id: 7,
                ..SessionConnInfo::default()
            },
        );
        assert!(s.is_login_id(7));
        assert!(!s.is_login_id(8));
        assert!(s.is_login_account("acc"));
        assert!(!s.is_login_account("7"));
        s.update(7, "热血传奇", true);
        assert_eq!(s.get_by_account("acc").unwrap().server_name, "热血传奇");
        assert!(s.get_by_account("acc").unwrap().is_payment);
        s.delete_by_account("acc");
        assert!(!s.is_login_id(7));
        assert!(!s.is_login_account("acc"));
    }
}
