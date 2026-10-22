//! 登录服务器（IDS）的**会话通知链路** —— C# `GameSrv/Services/AuthenticationService.cs` 的对应物。
//!
//! 拓扑（照 oracle）：GameSvr 是 **TcpClient**（`:11`），**主动连** `IDSAddr:IDSPort`
//! （`Initialize` `:22-34`；现网 `Server.conf` 的 `IDSAddr= 127.0.0.1` / `IDSPort=5600`）。
//! 收到字节 → 按 **GBK** 累加成文本（`DataSocketRead` `:56-68` 的 `HUtil32.GetString`）→
//! `Run()`（`:125-204`）按 `(...)` 分帧、把首段当**消息号**（`GetValidStr3(sData, ref sCode, '/' )`），
//! 再分派到各 handler。
//!
//! 本切片做（认证闭环需要的那两条）：
//! 1. `SS_OPENSESSION`(100) → `GetPasswdSuccess`（`:241-265`）→ **填准入表**（`NewSession`）；
//! 2. `SS_CLOSESESSION`(101) → `GetCancelAdmission`（`:267-281`）→ **删准入**（`DelSession`），
//!    并把被删的 `Account/SessionID` 作为 [`KickRequest`] 交给 sim 线程去**账号级顶号**
//!    （oracle `DelSession` 的下一句 `NetChannel.KickUser`，`AuthenticationService.cs:319`）。
//!
//! 断线：`DataSocketDisconnect`（`:427-436`）会 **`ClearSession()`（清空整张表）** 并记错误日志——照抄。
//!
//! **上报**：`SS_SERVERINFO`（`(103/名字/索引/在线数/付费模式)`）由两处产生（oracle 穷举只有这两处）：
//! ① 连接事件即报一次当前值（`DataSocketConnect` `:420-425`）；
//! ② `SystemProcessor.ShowOnline` 的独立 10s 门（`SystemProcessor.cs:63-67`，严格 `>`）。
//! oracle 这条链路**本身不重连**（它的 `SendSocket` 在 `Offline` 时直接 return、不排队）——
//! 本仓保留既有的 3s 重试循环，重连时只补发**当前值**一帧，积压的陈旧帧一律丢掉。
//!
//! **登记不做**（各需世界/配置，属后续切片）：`SS_KEEPALIVE`(104)/`SS_KICKUSER`(111)/`SS_SERVERLOAD`(113)/
//! `ISM_ACCOUNTEXPIRED`(114)/`ISM_QUERYPLAYTIME`(116)。

/// IDS 链路自己的日志宏（与 main 的 `glog!` 同形，便于两侧日志按前缀过滤）。
macro_rules! glog_ids {
    ($lvl:expr, $($arg:tt)*) => {{
        let now = ::std::time::SystemTime::now()
            .duration_since(::std::time::UNIX_EPOCH)
            .unwrap_or_default();
        let ms = now.subsec_millis();
        let secs = now.as_secs();
        println!(
            "{:02}:{:02}:{:02}.{ms:03} [{}] {}",
            (secs / 3600) % 24,
            (secs / 60) % 60,
            secs % 60,
            $lvl,
            format!($($arg)*)
        );
    }};
}

use std::sync::{Arc, Mutex};

use mir2_protocol::messages::{SS_CLOSESESSION, SS_OPENSESSION, SS_SERVERINFO};
use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, str_to_int, ArrestWrite};

use crate::auth::AdmissionStore;
use crate::GateEvent;

/// 一条**账号级顶号**请求（`SS_CLOSESESSION` 删掉的会话）。
///
/// oracle `DelSession`（`AuthenticationService.cs:299-327`）删掉 `AccountSession` 后调
/// `M2Share.NetChannel.KickUser(sAccount, sessionId, sessInfo?.PayMode ?? 0)`；
/// 注意 `sessInfo` 在 `RemoveAt` 后已被置 `null` ⇒ `payMode` **恒为 0**（永远发 `sKickUserMsg`）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KickRequest {
    pub account: String,
    pub session_id: i32,
}

/// 字段分隔符：`HUtil32.Backslash = '/'`（`HUtil32.cs:12`）。
const DIV: [char; 1] = ['/'];

/// 把累积文本按 `(...)` 分帧并分派；返回本次处理的消息条数。
///
/// 逐字照 `Run()`（`:149-199`）的循环：取 `(`…`)` 之间那一段；取不到就停；
/// 处理完再看剩余里还有没有 `)`，没有就停；最后把**剩余**放回缓冲（`:190-197`）。
/// 注意 `ArrestStringEx` 的怪癖：闭合标记之后的内容才是"返回的剩余"（见 `hutil` 的注释）。
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct IdsDrain {
    /// 本次处理的会话通知条数（`SS_OPENSESSION`/`SS_CLOSESESSION` 命中数）。
    pub handled: usize,
    /// 账号级顶号请求（`SS_CLOSESESSION` 删到会话时一条）。
    pub kicks: Vec<KickRequest>,
}

#[must_use]
pub fn drain_messages(buf: &mut String, store: &mut AdmissionStore) -> IdsDrain {
    let mut text = std::mem::take(buf);
    let mut out = IdsDrain::default();
    loop {
        // 三态照抄 `:153-157`：`ArrestStringEx(sSocketText, "(", ")", ref sData)` ——
        // `Value(空)`（找不到 `(` 的越界缺陷分支）与 `Unchanged`（无闭合 `)`）都让 `sData` 不是"新截出的非空内容"
        // ⇒ 循环中断（注意 `sData` 是 `Run()` 的局部量、只在命中时被写：未命中时它保持上一次的值，
        // 而紧随其后的 `IndexOf(')')` 检查会兜住这种情况，故这里等价地按"中断"处理）。
        let (write, rest) = arrest_string_ex_ref(&text, '(', ')');
        let arrested = match write {
            ArrestWrite::Value(v) if !v.is_empty() => v,
            _ => {
                text = rest;
                break;
            }
        };
        out.handled += usize::from(dispatch(&arrested, store, &mut out.kicks));
        text = rest;
        if !text.contains(')') {
            break;
        }
    }
    *buf = text;
    out
}

/// 单条消息的分派（`:158-184` 的 switch）。返回是否被本切片处理。
fn dispatch(arrested: &str, store: &mut AdmissionStore, kicks: &mut Vec<KickRequest>) -> bool {
    let (code_str, body) = get_valid_str3(arrested, &DIV);
    match u16::try_from(str_to_int(&code_str, 0)).unwrap_or(u16::MAX) {
        SS_OPENSESSION => {
            get_passwd_success(&body, store);
            true
        }
        SS_CLOSESESSION => {
            get_cancel_admission(&body, store, kicks);
            true
        }
        other => {
            glog_ids!("DBG", "IDS: 未处理的消息号 {other}（登记在切片清单里）");
            false
        }
    }
}

/// `GetPasswdSuccess`（`:241-265`）：`account/sessionId/payCost/payMode/ip/playTime`。
///
/// 字段顺序由发送端 `LoginSrv/Services/SessionServer.cs` 决定：
/// `Account + "/" + SessionID + "/" + (PayCost ? 1 : 0) + "/" + nPayMode + "/" + UserIPaddr + "/" + Seconds`。
fn get_passwd_success(body: &str, store: &mut AdmissionStore) {
    let (account, r) = get_valid_str3(body, &DIV);
    let (sid, r) = get_valid_str3(&r, &DIV);
    let (pay_cost, r) = get_valid_str3(&r, &DIV);
    let (pay_mode, r) = get_valid_str3(&r, &DIV);
    let (ipaddr, r) = get_valid_str3(&r, &DIV);
    let (play_time, _) = get_valid_str3(&r, &DIV);
    store.new_session(
        &account,
        &ipaddr,
        str_to_int(&sid, 0),
        str_to_int(&pay_cost, 0),
        str_to_int(&pay_mode, 0),
        i64::from(str_to_int(&play_time, 0)),
    );
    glog_ids!(
        "INF",
        "会话准入登记 Account:{account} SessionId:{} PayCost:{pay_cost} PayMode:{pay_mode} IP:{ipaddr}",
        str_to_int(&sid, 0)
    );
}

/// `GetCancelAdmission`（`:267-281`）：体是 `account/sessionId`，而 C# 取的是 `GetValidStr3` 的**返回值**
/// （= 第二段）当 sessionId（首段给 `sC` 后丢弃）。
///
/// 删到会话后还要接**账号级顶号**（`DelSession` 的下一句 `NetChannel.KickUser`，`:319`）：
/// 把被删的账号/会话号交给 sim 线程去发 `SM_SYSMESSAGE` + 软关旧会话（见 `lib.rs::handle_kick_account`）。
fn get_cancel_admission(body: &str, store: &mut AdmissionStore, kicks: &mut Vec<KickRequest>) {
    let (_account, session_id_str) = get_valid_str3(body, &DIV);
    let session_id = str_to_int(&session_id_str, 0);
    let removed = store.del_session(session_id);
    glog_ids!(
        "INF",
        "会话准入注销 SessionId:{session_id}（{}）",
        if removed.is_some() {
            "命中"
        } else {
            "不在表里"
        }
    );
    if let Some(sess) = removed {
        kicks.push(KickRequest {
            account: sess.account,
            session_id: sess.session_id,
        });
    }
}

/// `AuthenticationService.SendOnlineHumCountMsg`（`:113-116`）的帧体：
/// `({SS_SERVERINFO}/{ServerName}/{ServerIndex}/{nCount}/{PayMentMode})`
/// （C# `string.Format("({0}/{1}/{2}/{3}/{4})", …)`，字段全部十进制原样）。
///
/// 第 3 段取 `M2Share.ServerIndex`（`src/M2Server/M2Share.cs:29` 的
/// `public static byte ServerIndex = 0;`）——**全仓无赋值点**（`ServerConf.cs:17` 写的是
/// `SystemShare.ServerIndex`，另一个字段）⇒ oracle 恒发 `0`。按「行为等价优先于修 bug」
/// 照搬恒 `0`：不用 `[Server] ServerIndex`（配非 0 会与 oracle 分叉，尤其 99——
/// login-srv 的在线判定带 `ServerIndex != 99` 门，会把 GameSvr 当 DBSrv 排除 ⇒ 529 退回 `0`）。
#[must_use]
pub fn serverinfo_frame(server_name: &str, online_count: i32, payment_mode: u8) -> String {
    const ORACLE_SERVER_INDEX: i32 = 0; // `M2Share.ServerIndex`，从不赋值
    format!("({SS_SERVERINFO}/{server_name}/{ORACLE_SERVER_INDEX}/{online_count}/{payment_mode})")
}

/// `SystemProcessor.ShowOnline` 的 `SendOnlineHumTime` 10s 门（`SystemProcessor.cs:63-67`）。
///
/// oracle 在 `Startup`（`SystemProcessor.cs:22-27`）用 `GetTickCount()` 起算 ⇒ 基准是
/// **启动时刻**、不是 0。本仓注入时钟（`MIR2_CLOCK_MS`）后 `world.clock_ms` 从 epoch 量级
/// 起算；基准若写死 0，首个 tick 就 `clock_ms - 0 > 10000` ⇒ **连上后立刻多发一帧**
/// （oracle 要等满 10s）。门本身是**严格 `>`**，间隔 10000ms。
#[derive(Debug, Clone, Copy)]
pub struct OnlineHumGate {
    last: i64,
}

impl OnlineHumGate {
    /// 起算基准 = `SystemProcessor.Startup` 的 `SendOnlineHumTime = GetTickCount()`
    /// ⇒ 传**注入之后的** `world.clock_ms`。
    #[must_use]
    pub fn new(start_clock_ms: i64) -> Self {
        Self {
            last: start_clock_ms,
        }
    }

    /// 到点返回 `true` 并推进基准（oracle `:65-66`：先赋值、再发）。
    pub fn tick(&mut self, clock_ms: i64) -> bool {
        if clock_ms - self.last > 10_000 {
            self.last = clock_ms;
            true
        } else {
            false
        }
    }
}

/// 丢掉断链期积压的上报帧：oracle `SendSocket` 在 `Offline` 时直接 return、**不排队**
/// （`AuthenticationService.cs:70-79`）⇒ 重连只补发**当前值**一帧。
fn drain_stale(outbox: &mut tokio::sync::mpsc::UnboundedReceiver<String>) {
    while outbox.try_recv().is_ok() {}
}

/// 按 **gb2312(cp936)** 写出一条 `(...)` 帧（oracle `SendSocket` → `HUtil32.GetBytes`
/// = `Encoding.GetEncoding("gb2312").GetBytes`，`HUtil32.cs:689-692`）。
/// 用 `cp936` 而非 `encoding_rs::GBK`：与 .NET 的不可映射码点回退逐字节一致（A-14）。
async fn write_text(half: &mut tokio::net::tcp::OwnedWriteHalf, text: &str) -> anyhow::Result<()> {
    use tokio::io::AsyncWriteExt;
    let bytes = mir2_script::cp936::encode_gb2312_dotnet(text);
    half.write_all(&bytes).await?;
    half.flush().await?;
    Ok(())
}

/// 起 IDS 链路（照 `Start()` `:36-54`：连不上**只记日志、不退出进程**）。
///
/// `ev_tx`：把账号级顶号请求（`SS_CLOSESESSION` 删到会话）交给 sim 线程
/// （`GateEvent::KickAccount` ⇒ `handle_kick_account`）。
#[allow(clippy::too_many_arguments)]
pub async fn run_ids_link(
    ids_addr: String,
    ids_port: u16,
    store: Arc<Mutex<AdmissionStore>>,
    ev_tx: std::sync::mpsc::Sender<GateEvent>,
    server_name: String,
    payment_mode: u8,
    online_count: Arc<std::sync::atomic::AtomicI32>,
    outbox: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) {
    loop {
        match tokio::net::TcpStream::connect((ids_addr.as_str(), ids_port)).await {
            Ok(sock) => {
                sock.set_nodelay(true).ok();
                glog_ids!("INF", "登录服务器[{ids_addr}:{ids_port}]连接成功...");
                let (mut read_half, mut write_half) = sock.into_split();
                // `DataSocketConnect`（`AuthenticationService.cs:420-425`）：**连上即报一次当前值**。
                // 先把断链期积压的陈旧帧丢掉（oracle `SendSocket` 在 Offline 时直接 return、
                // 不排队；oracle 这条链路本身也不重连）⇒ 重连只补发这一帧当前值，不 flush 旧账。
                drain_stale(outbox);
                let frame = serverinfo_frame(
                    &server_name,
                    online_count.load(std::sync::atomic::Ordering::SeqCst),
                    payment_mode,
                );
                if let Err(e) = write_text(&mut write_half, &frame).await {
                    glog_ids!("ERR", "登录服务器链路：上报 SS_SERVERINFO 失败（{e:#}）");
                }
                if let Err(e) =
                    read_loop(&mut read_half, &mut write_half, &store, &ev_tx, outbox).await
                {
                    glog_ids!("ERR", "登录服务器链路结束：{e:#}");
                }
                // `DataSocketDisconnect`（`:427-436`）：**清空整张准入表** + 记错误
                store.lock().expect("准入表锁被毒化").clear();
                glog_ids!("ERR", "登录服务器[{ids_addr}]断开连接...（已清空准入表）");
            }
            Err(e) => {
                glog_ids!("ERR", "链接登录服务器[{ids_addr}:{ids_port}]失败...（{e}）");
            }
        }
        tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    }
}

/// 收字节 → GBK 文本 → 累积 → 分帧分派。返回 `Err` 表示链路断了（或读失败）。
async fn read_loop(
    read_half: &mut tokio::net::tcp::OwnedReadHalf,
    write_half: &mut tokio::net::tcp::OwnedWriteHalf,
    store: &Arc<Mutex<AdmissionStore>>,
    ev_tx: &std::sync::mpsc::Sender<GateEvent>,
    outbox: &mut tokio::sync::mpsc::UnboundedReceiver<String>,
) -> anyhow::Result<()> {
    use tokio::io::AsyncReadExt;
    let mut buf = Vec::with_capacity(4096);
    let mut text = String::new();
    loop {
        buf.resize(4096, 0);
        // 读与「sim 线程投递的上报帧」二选一：上报不阻塞收包。
        let n = tokio::select! {
            r = read_half.read(&mut buf) => r?,
            frame = outbox.recv() => {
                match frame {
                    Some(frame) => write_text(write_half, &frame).await?,
                    // 投递端已关闭（sim 线程收尾）⇒ 正常结束链路；旧实现的 `continue`
                    // 会让 select 立刻再次就绪 ⇒ 忙等（N-1）。
                    None => return Ok(()),
                }
                continue;
            }
        };
        if n == 0 {
            return Ok(());
        }
        // `DataSocketRead`（`:56-68`）：`HUtil32.GetString(buffer, 0, len)` = GBK 解码
        let (chunk, _, _) = encoding_rs::GBK.decode(&buf[..n]);
        text.push_str(&chunk);
        let drained = drain_messages(&mut text, &mut store.lock().expect("准入表锁被毒化"));
        if drained.handled > 0 {
            glog_ids!("DBG", "IDS: 处理 {} 条会话通知", drained.handled);
        }
        for k in drained.kicks {
            // 送达 sim 线程；sends 不会阻塞（`std::sync::mpsc::channel` 无界）。
            if ev_tx
                .send(GateEvent::KickAccount {
                    account: k.account,
                    session_id: k.session_id,
                })
                .is_err()
            {
                glog_ids!("WRN", "IDS: sim 事件通道已关闭，顶号请求丢弃");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_session_message_fills_admission() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/127.0.0.1/3600)".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert!(buf.is_empty(), "整条被消费 ⇒ 缓冲清空");
        let a = store.get_admission("loadbot1", 2).expect("应已登记");
        assert_eq!(a.session.ipaddr, "127.0.0.1");
        assert_eq!(a.session.pay_ment, 0, "线上的 PayCost=0");
        assert_eq!(a.pay_ment, 1, "映射 0→1");
        assert!(a.is_admitted());
        assert_eq!(a.session.play_time, 3600);
        assert_eq!(a.session.pay_mode, 1);
    }

    /// `SS_CLOSESESSION` 除了删准入，还要产出**账号级顶号请求**（`DelSession` ⇒ `KickUser`）。
    #[test]
    fn close_session_message_removes_admission_and_emits_kick() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(101/loadbot1/2)".to_string();
        let d = drain_messages(&mut buf, &mut store);
        assert_eq!(d.handled, 2, "两条一起处理（100 无 kick、101 有 kick）");
        assert!(
            store.get_admission("loadbot1", 2).is_none(),
            "101 应删掉登记"
        );
        // 顶号请求带上被删会话的 account/session_id（顺序：账号/会话号，与 `DelSession` 一致）。
        assert_eq!(
            d.kicks,
            vec![KickRequest {
                account: "loadbot1".to_string(),
                session_id: 2
            }]
        );
        assert!(buf.is_empty());
    }

    /// 未命中的 `SS_CLOSESESSION`（表里没有该 sessionId）不得产出顶号请求。
    #[test]
    fn close_session_miss_emits_no_kick() {
        let mut store = AdmissionStore::new();
        let mut buf = "(101/nobody/99)".to_string();
        assert!(drain_messages(&mut buf, &mut store).kicks.is_empty());
    }

    #[test]
    fn partial_frame_stays_in_buffer() {
        let mut store = AdmissionStore::new();
        let mut buf = "(100/loadbot1/2/0/1/ip/0)(100/loadbo".to_string();
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert_eq!(buf, "(100/loadbo", "未闭合的那段必须留在缓冲里");
        // 补齐后能被处理
        buf.push_str("t2/3/1/0/ip2/60)");
        assert_eq!(drain_messages(&mut buf, &mut store).handled, 1);
        assert!(store.get_admission("loadbot2", 3).is_some());
        assert!(
            store.get_admission("loadbot2", 3).unwrap().is_admitted(),
            "PayCost=1 ⇒ 映射 1→2 ⇒ 通过"
        );
        assert!(buf.is_empty());
    }

    #[test]
    fn unknown_code_and_garbage_are_ignored() {
        let mut store = AdmissionStore::new();
        let mut buf = "(104/whatever)(1/2/3)".to_string();
        assert_eq!(
            drain_messages(&mut buf, &mut store).handled,
            0,
            "未登记的消息号不计入"
        );
        assert!(buf.is_empty());
        // 不含 `(` 的垃圾：照 oracle 的越界分支 ⇒ 整段被丢（`:527-545` 的 catch 语义）
        let mut garbage = "abc".to_string();
        assert_eq!(drain_messages(&mut garbage, &mut store).handled, 0);
        assert!(garbage.is_empty(), "照 oracle 会丢干净");
    }
}

#[cfg(test)]
mod serverinfo_tests {
    use super::{drain_stale, serverinfo_frame, OnlineHumGate};
    use std::sync::atomic::AtomicI32;
    use std::sync::{Arc, Mutex};
    use tokio::io::AsyncReadExt;

    /// 帧体逐字节钉住：`(103/热血传奇/0/3/1)`，与 oracle
    /// `string.Format("({0}/{1}/{2}/{3}/{4})", SS_SERVERINFO, ServerName, M2Share.ServerIndex,
    /// nCount, PayMentMode)` 同形（字段间 `/`、首尾圆括号、整数十进制）。
    ///
    /// 第 3 段恒 `0`（`M2Share.ServerIndex` 从不赋值）；第 5 段缺省 `1`
    /// （`GameSvrConf.cs:1253`）——现网三份 `Server.conf` 都没有该键 ⇒ oracle 发 `…/0/0/1`。
    #[test]
    fn serverinfo_frame_matches_oracle_format() {
        assert_eq!(serverinfo_frame("热血传奇", 0, 1), "(103/热血传奇/0/0/1)");
        assert_eq!(serverinfo_frame("SKY引擎", 3, 3), "(103/SKY引擎/0/3/3)");
    }

    /// 牙：把分隔符 `/` 改成别的、或漏掉括号，本用例立刻红。
    #[test]
    fn serverinfo_frame_has_brackets_and_slashes() {
        let f = serverinfo_frame("a", 2, 3);
        assert!(f.starts_with('(') && f.ends_with(')'));
        assert_eq!(f.matches('/').count(), 4);
    }

    /// **I-2①** 生产入口：`run_ids_link` 连上本地 listener 后，**第一帧**就是 gb2312(cp936)
    /// 的 `(103/热血传奇/0/3/1)`（在线数取 sim 线程原子量、付费模式取配置）。
    ///
    /// 牙：① 删掉「连上即报」整段 ⇒ 本用例超时；② 把 `write_text` 的 cp936 换成 UTF-8
    /// ⇒ 下面的**原始字节**断言必红（UTF-8 编码是 8 个三字节序列）。
    #[tokio::test]
    async fn ids_link_reports_serverinfo_on_connect() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("绑定本地 listener");
        let port = listener.local_addr().expect("local_addr").port();
        let store = Arc::new(Mutex::new(crate::auth::AdmissionStore::new()));
        let (ev_tx, _ev_rx) = std::sync::mpsc::channel();
        let (_out_tx, mut outbox) = tokio::sync::mpsc::unbounded_channel::<String>();
        let online = Arc::new(AtomicI32::new(3));
        let link = tokio::spawn(async move {
            super::run_ids_link(
                "127.0.0.1".to_string(),
                port,
                store,
                ev_tx,
                "热血传奇".to_string(),
                1,
                online,
                &mut outbox,
            )
            .await;
        });
        let (mut sock, _peer) = listener.accept().await.expect("接受连接");
        let mut buf = [0u8; 64];
        let n = tokio::time::timeout(std::time::Duration::from_secs(5), sock.read(&mut buf))
            .await
            .expect("连上后 5s 内必须收到首帧")
            .expect("读取首帧");
        assert_eq!(
            &buf[..n],
            b"(103/\xC8\xC8\xD1\xAA\xB4\xAB\xC6\xE6/0/3/1)",
            "首帧必须是 gb2312 的 (103/热血传奇/0/3/1)"
        );
        link.abort();
    }

    /// **I-2②** `SystemProcessor.ShowOnline` 的 10s 门（`SystemProcessor.cs:63-67`）：
    /// 严格 `>`，基准取**启动时刻**（oracle `Startup` 的 `GetTickCount()`）。
    /// `MIR2_CLOCK_MS` 注入后 `world.clock_ms` 从 epoch 量级起算 ⇒ 基准必须同步到该起点，
    /// 否则首个 tick 就 `clock_ms - 0 > 10000` ⇒ **提前首发**（首条断言即红）。
    ///
    /// 牙：基准改回 0、或把 `>` 改成 `>=` ⇒ 前三条断言必红。
    #[test]
    fn online_hum_gate_is_strict_10s_from_startup_baseline() {
        let origin = 1_700_000_000_000i64; // 注入时钟后的起点量级
        let mut gate = OnlineHumGate::new(origin);
        assert!(!gate.tick(origin), "起算同一刻不发（注入下不得提前首发）");
        assert!(!gate.tick(origin + 10_000), "严格 >：正好 10000ms 不发");
        assert!(gate.tick(origin + 10_001), "超过 10000ms 发一次");
        assert!(
            !gate.tick(origin + 10_002),
            "发完推进基准（oracle :65 先赋值）"
        );
        assert!(gate.tick(origin + 20_002), "再等满 10s 后发下一帧");
    }

    /// **M-1**：断链期积压的陈旧帧在重连前必须丢干净（oracle `SendSocket` 在 `Offline`
    /// 时直接 return、不排队 ⇒ 重连只补发当前值一帧）。
    ///
    /// 牙：删掉 `drain_stale` 调用 ⇒ `try_recv` 仍能取到旧帧 ⇒ 必红。
    #[test]
    fn stale_frames_are_dropped_before_reporting_current_value() {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        tx.send("(103/a/0/1/1)".to_string()).expect("入队");
        tx.send("(103/a/0/2/1)".to_string()).expect("入队");
        drain_stale(&mut rx);
        assert!(
            rx.try_recv().is_err(),
            "断链期积压必须丢干净（oracle Offline 不排队）"
        );
    }
}
