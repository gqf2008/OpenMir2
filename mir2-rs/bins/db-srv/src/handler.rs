//! 命令处理：C# `UserService` 的**消息线程**（单消费者队列）+ `DeCodeUserMsg` 的四个命令。
//!
//! 对齐要点：
//! - 透传数据按**客户端会话**累积（`userInfo.sText`），一次 `Data` **最多**抠出一帧
//!   （C# `ProcessMessage` 用 `if` 而非 `while`）；抠帧前先累加，再把缓冲替换成 `!` 之后的剩余；
//! - 抠出的内容**丢掉首字符**（客户端帧的 `1`），前 16 个字符是编码头、其余是编码体；
//! - 四个命令的**限频与前置条件**逐条照搬：`QUERYCHR` 200ms、`NEWCHR`/`DELCHR` 1000ms、
//!   `SELCHR` 必须先查过角色，且 `NEWCHR`/`DELCHR`/`SELCHR` 都要过 LoginSrv 会话校验，
//!   不过就回 `SM_OUTOFCONNECTION`；
//! - 回包体是 `#` + 编码头 + **已编码的体** + `!`（`user::encode_user_frame`）。

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use anyhow::{bail, Result};
use mir2_protocol::edcode;
use mir2_protocol::frame::CommandMessage;
use mir2_protocol::messages::{
    CM_DELCHR, CM_NEWCHR, CM_QUERYCHR, CM_SELCHR, SM_DELCHR_FAIL, SM_DELCHR_SUCCESS,
    SM_NEWCHR_FAIL, SM_NEWCHR_SUCCESS, SM_OUTOFCONNECTION, SM_QUERYCHR, SM_QUERYCHR_FAIL,
    SM_STARTFAIL, SM_STARTPLAY,
};
use tokio::sync::mpsc::UnboundedReceiver;

use crate::db::Db;
use crate::gbk::{arrest, char_count, str_to_int, trim_name};
use crate::log;
use crate::server::{GateUser, Queued, Server};
use crate::user::{self, ClientPacket};

/// 进程启动以来的毫秒数（C# `HUtil32.GetTickCount()` 的等价物，只用于间隔判定）。
#[must_use]
pub fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    #[allow(clippy::cast_possible_truncation)]
    let ms = start.elapsed().as_millis() as u64;
    ms
}

/// `pick` 的 xorshift 状态。
static PICK_SEED: AtomicU64 = AtomicU64::new(0);

/// 单消费者队列：C# `Channel<UserGateMessage>` + `StartMessageThread`。
pub async fn queue_loop(server: Arc<Server>, mut rx: UnboundedReceiver<Queued>, mut db: Db) {
    while let Some(q) = rx.recv().await {
        if let Err(e) = process_data(&server, &mut db, q).await {
            // C# 在消息线程里 catch 并 `LogService.Error(e)`：该条命令**不回包**、不影响后续
            log::error(&format!("处理角色网关数据失败: {e:#}"));
        }
    }
}

/// 处理一条 `Data`（C# `ProcessMessage` + `DeCodeUserMsg`）。
async fn process_data(server: &Arc<Server>, db: &mut Db, q: Queued) -> Result<()> {
    // 取会话：C# 按 `SessionId == packet.SocketId` 在网关的用户表里找
    let (mut u, tx, remote_ip) = {
        let gates = server.gates.lock().await;
        let Some(Some(g)) = gates.get(q.slot) else {
            return Ok(());
        };
        let Some(u) = g.users.iter().find(|u| u.socket_id == q.msg.socket_id) else {
            return Ok(());
        };
        (u.clone(), g.tx.clone(), g.remote_ip.clone())
    };

    // C# `HUtil32.GetString(packet.Data, 0, packet.DataLen)`：DataLen 是独立字段
    let Some(n) = user::data_len(q.msg.data_len, q.msg.data.len()) else {
        log::debug("Data 报文 DataLen 越界（C# 会抛异常 ⇒ 丢弃该条）");
        return Ok(());
    };
    let text = q.msg.data[..n].to_vec();
    u.text.extend_from_slice(&text);
    // C# `if (sText.IndexOf("!") < 1) continue;`（-1 与 0 都跳过帧抠取，但缓冲已累加）
    let bang_ok = text.iter().position(|b| *b == b'!').is_some_and(|p| p >= 1);
    if bang_ok {
        if !u.text.contains(&b'!') {
            return Ok(()); // C# 此处是 `return`（整条消息结束）
        }
        let (frame, rest) = arrest(&u.text, b'#', b'!');
        u.text = rest;
        if frame.is_empty() {
            u.text.clear();
        } else if frame.len() > user::DEF_BLOCK_SIZE {
            let payload = &frame[1..];
            let packet = user::decode_client_packet(payload)
                .map_err(|len| anyhow::anyhow!("客户端消息头长度不足（{len}）"))?;
            dispatch(server, db, q.slot, &tx, &remote_ip, &mut u, &packet).await?;
        }
    }
    // 回写会话状态（会话已被摘掉就丢弃）
    let mut gates = server.gates.lock().await;
    if let Some(Some(g)) = gates.get_mut(q.slot) {
        if let Some(slot_user) = g.users.iter_mut().find(|x| x.socket_id == u.socket_id) {
            *slot_user = u;
        }
    }
    Ok(())
}

/// `DeCodeUserMsg` 的四个命令（其余消息号 C# 侧也是 default 静默忽略）。
async fn dispatch(
    server: &Arc<Server>,
    db: &mut Db,
    slot: usize,
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    remote_ip: &str,
    u: &mut GateUser,
    packet: &ClientPacket,
) -> Result<()> {
    let now = now_ms();
    match packet.head.ident {
        x if x == CM_QUERYCHR => {
            if !u.chr_queryed || now.saturating_sub(u.chr_tick) > 200 {
                u.chr_tick = now;
                // C# `UserService.cs:356-361`：`QueryChr` 成功才置 `boChrQueryed`，
                // 否则后续 `CM_SELCHR` 恒被 `if (userInfo.boChrQueryed)` 拒掉（本卡修的缺陷）。
                if query_chr(server, db, slot, tx, u, &packet.body).await? {
                    u.chr_queryed = true;
                }
            } else {
                log::warn(&format!(
                    "[Hacker Attack] QueryChr:{}",
                    String::from_utf8_lossy(&u.user_ip)
                ));
            }
        }
        x if x == CM_NEWCHR => {
            if now.saturating_sub(u.chr_tick) > 1000 {
                u.chr_tick = now;
                if !u.account.is_empty()
                    && server
                        .login
                        .lock()
                        .await
                        .check_session(&u.account, u.session_no)
                {
                    if new_chr(server, db, tx, u, &packet.body).await? {
                        u.chr_queryed = false;
                    }
                } else {
                    out_of_connect(tx, u);
                }
            } else {
                log::warn(&format!(
                    "[Hacker Attack] NEWCHR {}/{}",
                    String::from_utf8_lossy(&u.account),
                    String::from_utf8_lossy(&u.user_ip)
                ));
            }
        }
        x if x == CM_DELCHR => {
            if now.saturating_sub(u.chr_tick) > 1000 {
                u.chr_tick = now;
                if !u.account.is_empty()
                    && server
                        .login
                        .lock()
                        .await
                        .check_session(&u.account, u.session_no)
                {
                    delete_chr(server, db, tx, u, &packet.body).await?;
                    u.chr_queryed = false;
                } else {
                    out_of_connect(tx, u);
                }
            } else {
                log::warn(&format!(
                    "[Hacker Attack] DELCHR {}/{}",
                    String::from_utf8_lossy(&u.account),
                    String::from_utf8_lossy(&u.user_ip)
                ));
            }
        }
        x if x == CM_SELCHR => {
            if u.chr_queryed {
                if !u.account.is_empty()
                    && server
                        .login
                        .lock()
                        .await
                        .check_session(&u.account, u.session_no)
                {
                    if select_chr(server, db, remote_ip, tx, u, &packet.body).await? {
                        u.chr_selected = true;
                    }
                } else {
                    out_of_connect(tx, u);
                }
            } else {
                log::warn(&format!(
                    "Double send SELCHR {}/{}",
                    String::from_utf8_lossy(&u.account),
                    String::from_utf8_lossy(&u.user_ip)
                ));
            }
        }
        _ => {}
    }
    Ok(())
}

/// 发一条下行帧（C# `SendUserSocket`：`Data = "#" + 编码头 + 已编码体 + "!"`）。
fn send_user(
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    socket_id: &str,
    head: &CommandMessage,
    encoded_body: &[u8],
) {
    let frame = user::encode_user_frame(head, encoded_body);
    let msg = mir2_protocol::internal::ServerDataMessage::new(
        mir2_protocol::internal::ServerDataType::Data,
        socket_id,
        frame,
    );
    let _ = tx.send(msg.to_wire());
}

/// C# `OutOfConnect`：回 `SM_OUTOFCONNECTION`（无体）。
fn out_of_connect(tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>, u: &GateUser) {
    let head = CommandMessage::make(i32::from(SM_OUTOFCONNECTION), 0, 0, 0, 0);
    send_user(tx, &u.socket_id, &head, &[]);
}

/// C# `CloseUser(userInfo.SessionId, …)`：摘掉会话（并可能通知 LoginSrv 软下线）。
pub async fn close_user_by_socket(server: &Arc<Server>, slot: usize, socket_id: &str) {
    let mut gates = server.gates.lock().await;
    let Some(Some(g)) = gates.get_mut(slot) else {
        return;
    };
    let Some(idx) = g.users.iter().position(|u| u.socket_id == socket_id) else {
        return;
    };
    let u = g.users[idx].clone();
    log::debug(&format!(
        "会话关闭 socket_id={} 存活={}ms",
        u.socket_id,
        now_ms().saturating_sub(u.tick34)
    ));
    let mut login = server.login.lock().await;
    if !login.get_play(u.session_no) {
        login.send_soft_out(&u.account, u.session_no);
        login.close_session(&u.account, u.session_no);
    }
    g.users.remove(idx);
}

/// 名字的 GBK 字节 → 尽量可读的文本（日志用；非 ASCII 名字按替换字符显示）。
fn show(b: &[u8]) -> String {
    String::from_utf8_lossy(b).into_owned()
}

/// C# `QueryChr`：查角色列表（会话校验 → 逐角色读库 → `SM_QUERYCHR[_FAIL]`）。
async fn query_chr(
    server: &Arc<Server>,
    db: &mut Db,
    slot: usize,
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    u: &mut GateUser,
    body: &[u8],
) -> Result<bool> {
    let (account, session) = user::query_chr_request(body);
    let mut nchr = 0i32;
    let mut items: Vec<(Vec<u8>, u8, u8, u16, u8, bool)> = Vec::new();
    let found = {
        let mut login = server.login.lock().await;
        let ok = login.check_session(&account, session);
        if ok {
            login.set_no_play(session);
        }
        ok
    };
    if found {
        u.account.clone_from(&account);
        u.session_no = session;
        for (i, q) in db.find_by_account(&account).into_iter().enumerate() {
            log::info(&format!(
                "[QueryChr] 候选 account={} i={i} name=[{}] quickIndex={}",
                show(&account),
                q.name_text,
                q.index
            ));
            let rec = db.record_get_by(q.index).await?;
            // 日志口径照抄 C# 的 `$"...result={result}"`（.NET 布尔渲染 True/False），
            // 便于把两侧日志按行 diff 当证据用。
            log::info(&format!(
                "[QueryChr]   GetBy -> result={}",
                if rec.is_some() { "True" } else { "False" }
            ));
            let Some(rec) = rec else { continue };
            if rec.deleted {
                continue;
            }
            let data_index = db.data_index(&q.name_gbk);
            log::info(&format!(
                "[QueryChr]   dataIndex={} nChrCount={nchr}",
                data_index.map_or(-1, |v| i32::try_from(v).unwrap_or(i32::MAX))
            ));
            let Some(index) = data_index else { continue };
            if nchr >= 2 {
                continue;
            }
            let Some(ch) = db.get_query_char(index).await? else {
                // C# 此处 `chrRecord` 为 null ⇒ NRE（被上层吞掉、**不回包**）
                bail!("角色[{index}]在 characters 表取不到行（C# 同场景为 NRE，不回包）");
            };
            log::info("[QueryChr]   GetQryChar -> True");
            items.push((
                q.name_gbk.clone(),
                ch.job,
                ch.hair,
                ch.level,
                ch.sex,
                rec.selected == 1,
            ));
            nchr += 1;
        }
        let head = CommandMessage::make(i32::from(SM_QUERYCHR), nchr, 0, 1, 0);
        let encoded = edcode::encode(&user::query_chr_body(&items));
        send_user(tx, &u.socket_id, &head, &encoded);
        log::debug(&format!("[QueryChr]:{} nChrCount={nchr}", show(&account)));
        Ok(true)
    } else {
        let head = CommandMessage::make(i32::from(SM_QUERYCHR_FAIL), nchr, 0, 1, 0);
        send_user(tx, &u.socket_id, &head, &[]);
        log::debug(&format!("[QueryChr]:{} 会话校验失败", show(&account)));
        close_user_by_socket(server, slot, &u.socket_id).await;
        Ok(false)
    }
}

/// C# `NewChr`：建角（前置校验 → 插索引 → 插存档 → `SM_NEWCHR_SUCCESS[_FAIL(code)]`）。
async fn new_chr(
    server: &Arc<Server>,
    db: &mut Db,
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    u: &mut GateUser,
    body: &[u8],
) -> Result<bool> {
    let req = user::new_chr_request(body);
    let account = show(&req.account);
    let mut ncode: i32 = -1;
    if !trim_name(&req.rest).is_empty() {
        ncode = 0;
    }
    let name = trim_name(&req.name);
    if char_count(&name) < 3 {
        ncode = 0;
    }
    if server.cfg.english_names && !user::is_english_str(&name) {
        ncode = 0;
    }
    if !server.share.check_deny_chr_name(&name) {
        ncode = 2;
    }
    if ncode == -1 {
        if db.data_index(&name).is_some() {
            ncode = 2;
        } else if db.chr_count_of_account(&req.account).await? >= 2 {
            ncode = 3;
        } else {
            // C# `Add` 绑的 `@SelectID` 是 `PlayerRecordData.Selected`（此处恒 0），
            // `Header.SelectID`（= 网关 ID）只用于内存索引 —— 如实照搬。
            if db
                .record_add(&name, &account, u.sel_gate_id)
                .await?
                .is_none()
            {
                ncode = 2;
            } else {
                let sex = byte_cast(str_to_int(&req.sex, 0));
                let job = byte_cast(str_to_int(&req.job, 0));
                let hair = byte_cast(str_to_int(&req.hair, 0));
                if db
                    .data_add(&name, &account, sex, job, hair)
                    .await?
                    .is_some()
                {
                    ncode = 1;
                } else {
                    // C#：`_playRecordStorage.Delete(sChrName)` 回滚索引（软删 IsDeleted=2）
                    db.record_soft_delete(&name).await?;
                    ncode = 4;
                }
            }
        }
    }
    let head = if ncode == 1 {
        CommandMessage::make(i32::from(SM_NEWCHR_SUCCESS), 0, 0, 0, 0)
    } else {
        CommandMessage::make(i32::from(SM_NEWCHR_FAIL), ncode, 0, 0, 0)
    };
    send_user(tx, &u.socket_id, &head, &[]);
    log::info(&format!(
        "创建角色:{} 结果:{}",
        show(&name),
        if ncode == 1 { "成功" } else { "失败" }
    ));
    Ok(ncode == 1)
}

/// C# `DeleteChr`：删角（软删 `IsDeleted=1`）。
async fn delete_chr(
    server: &Arc<Server>,
    db: &mut Db,
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    u: &mut GateUser,
    body: &[u8],
) -> Result<()> {
    let name = edcode::decode(body);
    let mut bocheck = false;
    if let Some(nindex) = db.record_index(&name) {
        if let Some(rec) = db.record_get_by(nindex).await? {
            bocheck = true; // C# `Get(nIndex, ref boCheck)` 读到行即置 true
            if rec.account_text.as_bytes() == u.account.as_slice() {
                let level = del_chr_sname_to_level(db, &name).await?;
                if i32::from(level) < server.cfg.delete_min_level {
                    let mut row = rec;
                    row.deleted = true;
                    bocheck = db.record_update(&row).await?;
                }
            }
        }
    }
    let head = if bocheck {
        CommandMessage::make(i32::from(SM_DELCHR_SUCCESS), 0, 0, 0, 0)
    } else {
        CommandMessage::make(i32::from(SM_DELCHR_FAIL), 0, 0, 0, 0)
    };
    send_user(tx, &u.socket_id, &head, &[]);
    log::info(&format!(
        "删除角色[{}] account={} 结果:{}",
        show(&name),
        show(&u.account),
        if bocheck { "成功" } else { "失败" }
    ));
    Ok(())
}

/// C# `DelChrSnameToLevel`。
async fn del_chr_sname_to_level(db: &Db, name: &[u8]) -> Result<u16> {
    let Some(index) = db.data_index(name) else {
        return Ok(0);
    };
    match db.get_query_char(index).await? {
        Some(q) => Ok(q.level),
        None => bail!("角色[{index}]在 characters 表取不到行（C# 同场景为 NRE，不回包）"),
    }
}

/// C# `SelectChr`：写 `SelectID` 标记 → 取地图 → 回 `SM_STARTPLAY[_FAIL]`。
async fn select_chr(
    server: &Arc<Server>,
    db: &mut Db,
    remote_ip: &str,
    tx: &tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    u: &mut GateUser,
    body: &[u8],
) -> Result<bool> {
    let (account, name) = user::account_and_name(body);
    let mut data_ok = false;
    let mut cur_map = String::new();
    if u.account.eq_ignore_ascii_case(&account) {
        for q in db.find_by_account(&account) {
            let Some(mut rec) = db.record_get_by(q.index).await? else {
                continue;
            };
            if rec.name_gbk == name {
                rec.selected = 1;
                db.record_update_by(&rec).await?;
            } else if rec.selected == 1 {
                rec.selected = 0;
                db.record_update_by(&rec).await?;
            }
        }
        if let Some(index) = db.data_index(&name) {
            match db.query_name_and_map(index).await? {
                None => bail!("角色[{index}]在 characters 表取不到行（C# 同场景为 NRE，不回包）"),
                Some((chr_name, map)) => {
                    if !chr_name.is_empty() {
                        cur_map = map;
                        data_ok = true;
                    }
                }
            }
        }
    }
    if data_ok {
        // 地图编号：C# `DBShare.GetMapIndex(sCurMap)`（本部署 `[Setup] MapFile` 缺省为空 ⇒ 恒 0）
        let map_index = server.share.get_map_index(&cur_map);
        let (route_ip, port) = server
            .share
            .gate_route(remote_ip.as_bytes(), pick(usize::MAX));
        let route_ip = if server.cfg.dynamic_ip_mode {
            u.gate_ip.clone()
        } else {
            route_ip
        };
        let mut route = route_ip;
        route.push(b'/');
        route.extend_from_slice((port + map_index).to_string().as_bytes());
        let head = CommandMessage::make(i32::from(SM_STARTPLAY), 0, 0, 0, 0);
        let encoded = edcode::encode(&route);
        send_user(tx, &u.socket_id, &head, &encoded);
        server.login.lock().await.set_play(u.session_no);
        log::debug(&format!(
            "玩家获取游戏网关信息 RunGame:{} Port:{}",
            show(&route),
            port + map_index
        ));
        Ok(true)
    } else {
        let head = CommandMessage::make(i32::from(SM_STARTFAIL), 0, 0, 0, 0);
        send_user(tx, &u.socket_id, &head, &[]);
        Ok(false)
    }
}

/// C# `(byte)int` 强转：**按 8 位截断**（既有行为，不是校验）。
fn byte_cast(v: i32) -> u8 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let b = v as u8;
    b
}

/// C# `RandomNumber.Random(n)`：`n <= 1` 恒 0；`n >= 2` 取 `[0, n)`。
///
/// `.NET Random` 的无参序列不可复刻（whitelist D-3 同族），且"选哪台游戏网关"本身就是随机的；
/// 这里用进程内 xorshift，`GateCount == 1`（现网）时与 C# 完全一致地恒取 0。
fn pick(count: usize) -> usize {
    if count <= 1 {
        return 0;
    }
    let mut s = PICK_SEED.load(Ordering::Relaxed);
    if s == 0 {
        s = now_ms() | 1;
    }
    s ^= s << 13;
    s ^= s >> 7;
    s ^= s << 17;
    PICK_SEED.store(s, Ordering::Relaxed);
    #[allow(clippy::cast_possible_truncation)]
    let v = s as usize;
    v % count
}
