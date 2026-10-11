//! DBSrv **5700 `MarketService` 面**（拍卖行）：GameSvr ↔ DBSrv 的寄售行链路
//! （C# `src/DBSrv/Services/Impl/MarketService.cs`）。
//!
//! 与 6000 `DataService` 同用 6B `ServerDataPacket` + MemoryPack `ServerRequestData`
//! 信封，但**校验码语义不同**（逐条照 C#，两个面在这处分叉）：
//! - 6000：`checkCode <= 0` 或 `Sign.Length <= 0` 时**照常处理**（`ProcessServerMsg`）；
//! - 5700：同样两种情形回 **`DBR_FAIL`(2000)**、不处理。
//!
//! 逐条对齐 C#（含既有怪行为与**不可达分支**）：
//! - `DB_LOADMARKET`(100)：`MarketRegisterMessage` 解出后 `Token`/`ServerName` 任一为空 ⇒ 只记日志；
//!   否则查库；查得空 ⇒ `DB_LOADMARKETFAIL`(106) `recog=0 param=1`；
//!   查得非空 ⇒ C# 构造 `MarketDataMessage` 时 `Serialize` **抛**
//!   （`MarketItem` 未注册 MemoryPack），异常被 `Received` 的 try/catch 吞掉 ⇒ **不回包**；
//! - `DB_SAVEMARKET`(101)：`Deserialize<MarketSaveDataItem>` 只要碰到 `Item` 成员就抛
//!   （`MarketItem not registered`）⇒ 恒不回包；
//! - `DB_SEARCHMARKET`(102)：`SearchMarketItems` 四个分支全是空桩 ⇒ 恒 `DB_SRARCHMARKETFAIL`(104)
//!   （`GroupId==0` 时 `param=0`，否则 `param=1`）；
//! - `DB_LOADUSERMARKET`(109)：`GroupId==0` ⇒ 104 `param=0`；否则 `QueryMarketItemsCount` 恒 0 ⇒
//!   `DB_LOADUSERMARKETSUCCESS`(110) + `MarkerUserLoadMessage`（查询号**恒为 1**，C# `SendSuccessMessage` 原样）；
//! - 其余 `ident`：`ProcessMarketPacket` 无 `default` ⇒ **不回包**。
//!
//! **已知不可达（C# oracle 如此，非本实现遗漏）**：C# 线上任何携带非空 `MarketItem` 的
//! `Serialize`/`Deserialize` 都会抛 `MemoryPackSerializationException`（真身探针已证，见
//! `tests/parity/evidence/M1/s4b-slice3-market-probe.txt`）。Rust 侧因此不实现 `MarketItem`
//! 的 MemoryPack 编解码：凡声称含 `Item` 的 `MarketSaveDataItem` 一律按「C# 会抛」处理 → 不回包。
//! 存储层 `MarketStoageService` 的 SQL 仍 1:1 实现（`marketdata.rs`），由单元测试直接驱动。

use std::sync::Arc;

use anyhow::{bail, Context, Result};
use mir2_protocol::edcode;
use mir2_protocol::frame::{ServerDataPacket, DEF_BLOCK_SIZE};
use mir2_protocol::messages::grobal2::PacketCode;
use mir2_protocol::messages::{
    DBR_FAIL, DB_LOADMARKET, DB_LOADMARKETFAIL, DB_LOADUSERMARKET, DB_LOADUSERMARKETSUCCESS,
    DB_SAVEMARKET, DB_SEARCHMARKET, DB_SRARCHMARKETFAIL,
};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::{mpsc, Mutex};

use crate::data::{low16, make_long, send_request_bytes, send_request_plain, ConnAction};
use crate::db::Db;
use crate::log;
use crate::marketdata::MarketItem;
use crate::server::Server;

/// C# 校验码里的 `queryId ^ 170` 常量（`170 = 0xAA`）。
const CHECK_XOR: i32 = 0xAA;

// ==== MemoryPack 编解码（5700 市场消息；不复用 `mir2_storage::playerdata`，避免动接口冻结线） ====

/// 最小 MemoryPack 读取器（只覆盖 5700 市场消息用到的成员类型）。
struct Mr<'a> {
    b: &'a [u8],
    pos: usize,
}

impl<'a> Mr<'a> {
    fn new(b: &'a [u8]) -> Self {
        Self { b, pos: 0 }
    }
    fn take(&mut self, n: usize) -> Result<&'a [u8]> {
        let end = self
            .pos
            .checked_add(n)
            .context("memorypack: length overflow")?;
        if end > self.b.len() {
            bail!("memorypack: truncated");
        }
        let s = &self.b[self.pos..end];
        self.pos = end;
        Ok(s)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_le_bytes(self.take(2)?.try_into().unwrap()))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take(4)?.try_into().unwrap()))
    }
    /// 对象头：成员数必须等于 `want`。
    fn header_expect(&mut self, want: u8) -> Result<()> {
        let got = self.u8()?;
        if got == want {
            Ok(())
        } else {
            bail!("memorypack: member count {got} != {want}");
        }
    }
    /// UTF-16LE 长度前缀字符串；`-1`（null）折成空串（C# 侧只做 `IsNullOrEmpty` 判定）。
    fn string(&mut self) -> Result<String> {
        let n = self.i32()?;
        if n == -1 {
            return Ok(String::new());
        }
        let n = usize::try_from(n).context("memorypack: negative string len")?;
        let raw = self.take(n.saturating_mul(2))?;
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16(&units).context("memorypack: invalid utf16 string")
    }
    fn finish(&self) -> Result<()> {
        if self.pos == self.b.len() {
            Ok(())
        } else {
            bail!("memorypack: {} trailing bytes", self.b.len() - self.pos)
        }
    }
}

/// C# `MarketRegisterMessage`（`Packets/ServerPackets/MarketDataMessage.cs`）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketRegisterMessage {
    pub server_index: u8,
    pub server_name: String,
    pub group_id: u8,
    pub token: String,
}

impl MarketRegisterMessage {
    fn from_bytes(b: &[u8]) -> Result<Self> {
        let mut r = Mr::new(b);
        r.header_expect(4)?;
        let m = Self {
            server_index: r.u8()?,
            server_name: r.string()?,
            group_id: r.u8()?,
            token: r.string()?,
        };
        r.finish()?;
        Ok(m)
    }
}

/// C# `MarketSearchMessage`（搜索/个人查询共用）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketSearchMessage {
    pub server_index: u8,
    pub group_id: u8,
    pub user_name: String,
    pub market_name: String,
    pub search_who: String,
    pub search_item: String,
    pub item_type: i16,
    pub item_set: u8,
    pub user_mode: i32,
    pub market_npc: i32,
}

impl MarketSearchMessage {
    fn from_bytes(b: &[u8]) -> Result<Self> {
        let mut r = Mr::new(b);
        r.header_expect(10)?;
        let m = Self {
            server_index: r.u8()?,
            group_id: r.u8()?,
            user_name: r.string()?,
            market_name: r.string()?,
            search_who: r.string()?,
            search_item: r.string()?,
            item_type: r.i16()?,
            item_set: r.u8()?,
            user_mode: r.i32()?,
            market_npc: r.i32()?,
        };
        r.finish()?;
        Ok(m)
    }
}

/// C# `MarketSaveDataItem`：含 `MarketItem Item`（`[MemoryPackAllowSerialize]`）。
///
/// 真身探针：任意成员数 ≥ 1 的编码都抛
/// `MemoryPackSerializationException: OpenMir2.Data.MarketItem is not registered`；
/// 只有对象头 `0x00`（空对象）能解出全默认。故这里**只接受** `0x00`，`item` 恒 `None`。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MarketSaveDataItem {
    pub item: Option<MarketItem>,
    pub server_index: u8,
    pub server_name: String,
    pub group_id: u8,
}

impl MarketSaveDataItem {
    fn from_bytes(b: &[u8]) -> Result<Self> {
        let header = b.first().copied().context("memorypack: empty payload")?;
        if header != 0 {
            bail!(
                "MarketSaveDataItem 含 MarketItem 成员（C# MemoryPack 未注册 MarketItem ⇒ 反序列化抛异常）"
            );
        }
        Ok(Self::default())
    }
}

/// C# `MarkerUserLoadMessage`（个人拍卖行数量应答）。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MarkerUserLoadMessage {
    pub sell_count: i32,
    pub market_npc: i32,
    pub is_busy: u8,
}

impl MarkerUserLoadMessage {
    /// C# `MarkerUserLoadMessage` 是**全非托管 struct**（`int,int,byte`）：MemoryPack 对这类
    /// 结构体按**原始内存布局**序列化 —— **无成员数对象头**，含 `Pack=1` 外的 3 字节尾部填充
    /// （真身探针：`{3,7,0}` ⇒ `030000000700000000000000`，12 字节）。
    fn to_bytes(self) -> Vec<u8> {
        let mut b = Vec::with_capacity(12);
        b.extend_from_slice(&self.sell_count.to_le_bytes());
        b.extend_from_slice(&self.market_npc.to_le_bytes());
        b.push(self.is_busy);
        b.extend_from_slice(&[0u8; 3]); // struct 尾部填充（内存布局）
        b
    }
}

// ==== 监听与连接 ====

/// 起 5700 监听并服务（`MarketService.Initialize` + `Start`）。
///
/// # Errors
/// 绑定失败（端口被占/地址非法）时返回错误（M1④：启动失败要明确报错并给退出码）。
pub async fn run(server: Arc<Server>, db: Arc<Mutex<Db>>) -> Result<()> {
    let addr = (server.cfg.market_addr.as_str(), server.cfg.market_port);
    let listener = TcpListener::bind(addr).await.with_context(|| {
        format!(
            "绑定 {}:{} 失败（端口被占？M1④）",
            server.cfg.market_addr, server.cfg.market_port
        )
    })?;
    log::info(&format!(
        "拍卖行数据库服务[{}:{}]已启动.等待链接...",
        server.cfg.market_addr, server.cfg.market_port
    ));
    loop {
        let (sock, peer) = listener.accept().await.context("accept 失败")?;
        sock.set_nodelay(true).ok();
        let server = Arc::clone(&server);
        let db = Arc::clone(&db);
        tokio::spawn(async move {
            if let Err(e) = conn_task(server, db, sock, peer.to_string()).await {
                log::warn(&format!("拍卖行客户端会话结束: {e:#}"));
            }
        });
    }
}

/// 单个 GameSvr 连接（C# `Connecting`/`Received`/`Disconnected`）。
async fn conn_task(
    server: Arc<Server>,
    db: Arc<Mutex<Db>>,
    sock: TcpStream,
    peer: String,
) -> Result<()> {
    let ip = peer
        .rsplit_once(':')
        .map_or(peer.as_str(), |(h, _)| h)
        .to_string();
    if !server.share.check_server_ip(&ip) {
        log::warn(&format!("非法服务器连接: {ip}"));
        return Ok(());
    }
    log::info(&format!("拍卖行客户端(GameSvr)连接 {peer}"));
    let (mut rd, mut wr) = sock.into_split();
    let (tx, mut rx) = mpsc::unbounded_channel::<Vec<u8>>();
    let writer = tokio::spawn(async move {
        while let Some(buf) = rx.recv().await {
            if wr.write_all(&buf).await.is_err() {
                break;
            }
            wr.flush().await.ok();
        }
    });
    let mut head = [0u8; ServerDataPacket::SIZE];
    loop {
        if rd.read_exact(&mut head).await.is_err() {
            break;
        }
        let Some(pkt) = ServerDataPacket::from_bytes(&head) else {
            log::error("解析寄售行封包出现异常封包...");
            break;
        };
        let mut body = vec![0u8; usize::from(pkt.packet_len)];
        if rd.read_exact(&mut body).await.is_err() {
            break;
        }
        if pkt.packet_code != PacketCode {
            log::error("解析寄售行封包出现异常封包...");
            continue;
        }
        // C# `Received` 把整段包在 try/catch 里：反序列化异常只记日志、连接保持。
        let Some(request) = decode_request(&body) else {
            continue;
        };
        match process(&db, &tx, &request).await {
            Ok(ConnAction::Continue) => {}
            Ok(ConnAction::Close) => break,
            Err(e) => log::error(&format!("处理寄售行封包失败: {e:#}")),
        }
    }
    writer.abort();
    log::info(&format!("拍卖行客户端(GameSvr)断开连接 {peer}"));
    Ok(())
}

/// C# `SerializerUtil.Deserialize<ServerRequestData>(fixedHeader.Message)`；失败只记日志。
fn decode_request(body: &[u8]) -> Option<mir2_storage::playerdata::ServerRequestData> {
    use mir2_storage::playerdata::{MemPack, ServerRequestData};
    match ServerRequestData::from_bytes(body) {
        Ok(v) => Some(v),
        Err(e) => {
            log::error(&format!("解析寄售行封包出现异常封包... {e:#}"));
            None
        }
    }
}

/// C# `ProcessMessagePacket`（5700 版：校验码/签名不合法一律回 `DBR_FAIL`）。
async fn process(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    request: &mir2_storage::playerdata::ServerRequestData,
) -> Result<ConnAction> {
    use mir2_storage::playerdata::{MemPack, ServerRequestMessage};
    let n_query_id = request.query_id;
    // C# 求值顺序：先 `Deserialize(EDCode.DecodeBuff(requestData.Message))`（Message null ⇒ NPE），
    // 再 `requestData.Message.Length + requestData.Packet.Length + 6`（Packet null ⇒ NPE）。
    let Some(message_enc) = request.message.as_ref() else {
        log::error("寄售行封包缺少 Message 字段（C# 同场景 NPE）");
        return Ok(ConnAction::Continue);
    };
    let Ok(request_message) = ServerRequestMessage::from_bytes(&edcode::decode(message_enc)) else {
        log::error("解析寄售行消息头失败");
        return Ok(ConnAction::Continue);
    };
    let Some(packet_enc) = request.packet.as_ref() else {
        log::error("寄售行封包缺少 Packet 字段（C# 同场景 NPE）");
        return Ok(ConnAction::Continue);
    };
    let Some(sign) = request.sign.as_ref() else {
        send_fail(tx, n_query_id, DBR_FAIL, 0);
        return Ok(ConnAction::Continue);
    };
    let packet_len = message_enc
        .len()
        .saturating_add(packet_enc.len())
        .saturating_add(ServerDataPacket::SIZE);
    if packet_len >= DEF_BLOCK_SIZE && n_query_id > 0 {
        let check_code = make_long(
            low16(n_query_id ^ CHECK_XOR),
            u16::try_from(packet_len).unwrap_or(u16::MAX),
        );
        if check_code <= 0 {
            send_fail(tx, n_query_id, DBR_FAIL, 0);
            return Ok(ConnAction::Continue);
        }
        if sign.is_empty() {
            send_fail(tx, n_query_id, DBR_FAIL, 0);
            return Ok(ConnAction::Continue);
        }
        let signature_id = low16(check_code);
        let s_data = edcode::decode(packet_enc);
        let sign_dec = edcode::decode(sign);
        // C# `BitConverter.ToInt16(signBuff)` 在 `signBuff.Length < 2` 时抛 `ArgumentException`
        // （被 `Received` 吞掉、连接保持）；Rust 取不到 2 字节按 0（共享登记 S4B-3）。
        let sign_id = if sign_dec.len() >= 2 {
            u16::from_le_bytes([sign_dec[0], sign_dec[1]])
        } else {
            0
        };
        if sign_id == signature_id {
            process_market_packet(
                db,
                tx,
                n_query_id,
                request_message.recog,
                &request_message,
                &s_data,
            )
            .await;
            return Ok(ConnAction::Continue);
        }
        log::error(&format!("关闭错误的任务{n_query_id}查询请求."));
        return Ok(ConnAction::Close);
    }
    send_fail(tx, n_query_id, DBR_FAIL, 0);
    Ok(ConnAction::Continue)
}

/// C# `ProcessMarketPacket`（无 `default` ⇒ 未知 ident 不回包）。
async fn process_market_packet(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    n_query_id: i32,
    actor_id: i32,
    packet: &mir2_storage::playerdata::ServerRequestMessage,
    s_data: &[u8],
) {
    match packet.ident {
        x if x == u16::from(DB_LOADMARKET) => {
            load_market_list(db, tx, n_query_id, s_data).await;
        }
        x if x == u16::from(DB_SAVEMARKET) => {
            save_market_item(db, tx, n_query_id, actor_id, s_data).await;
        }
        x if x == u16::from(DB_SEARCHMARKET) => {
            search_market_item(db, tx, n_query_id, s_data).await;
        }
        x if x == u16::from(DB_LOADUSERMARKET) => {
            query_market_user_load(db, tx, n_query_id, actor_id, s_data).await;
        }
        _ => {}
    }
}

/// C# `MarketService.LoadMarketList`。
async fn load_market_list(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    n_query_id: i32,
    s_data: &[u8],
) {
    let Ok(register) = MarketRegisterMessage::from_bytes(s_data) else {
        log::error("解析 MarketRegisterMessage 失败");
        return;
    };
    if register.token.is_empty() || register.server_name.is_empty() {
        log::warn(&format!("QueryId:[{n_query_id}] 非法获取拍卖行数据..."));
        return;
    }
    match db.lock().await.market_query_items(register.group_id).await {
        Err(e) => {
            // C# `QueryMarketItems` 无 try/catch ⇒ 异常冒到 `Received` ⇒ 只记日志、不回包。
            log::error(&format!("读取拍卖行数据失败: {e:#}"));
        }
        Ok(items) if items.is_empty() => {
            send_fail(tx, n_query_id, u16::from(DB_LOADMARKETFAIL), 1);
            log::info(&format!(
                "当前服务器组[{}]拍卖行数据为空,读取拍卖行数据失败.",
                register.group_id
            ));
        }
        Ok(items) => {
            // C# 此处构造 MarketDataMessage 后 `Serialize` 抛（MarketItem 未注册）⇒ 不回包。
            log::error(&format!(
                "拍卖行数据序列化失败（C# MarketItem 未注册 MemoryPack ⇒ 同场景抛异常、不回包）: 条数={}",
                items.len()
            ));
        }
    }
}

/// C# `MarketService.SaveMarketItem`。
async fn save_market_item(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    n_query_id: i32,
    actor_id: i32,
    s_data: &[u8],
) {
    let save = match MarketSaveDataItem::from_bytes(s_data) {
        Ok(v) => v,
        Err(e) => {
            log::error(&format!("解析 MarketSaveDataItem 失败: {e:#}"));
            return;
        }
    };
    if save.group_id == 0 || save.server_name.is_empty() {
        log::warn(&format!("任务[{n_query_id}]非法获取拍卖行数据..."));
        return;
    }
    // 运行时不可达：非空 `Item` 在 `from_bytes` 就按「C# 抛异常」处理了。
    // 保留 C# 的存储 + 成功回包形状，避免将来接上 MarketItem 编码时漏分支。
    let Some(item) = save.item.as_ref() else {
        log::error("MarketSaveDataItem.Item 为 null（C# 此处 NPE，被 Received 吞掉、不回包）");
        return;
    };
    match db
        .lock()
        .await
        .market_save_item(item, save.group_id, save.server_index)
        .await
    {
        Err(e) => log::error(&format!("保存拍卖行数据失败: {e:#}")),
        Ok(false) => log::info("当前服务器分组拍卖行数据为空,推送拍卖行数据失败."),
        Ok(true) => {
            let _ = db.lock().await.market_query_items(save.group_id).await;
            // C# `SendSuccessMessage(Serialize(saveMessage))` 抛（含 MarketItem）⇒ 不回包。
            log::error(&format!(
                "DB_SAVEMARKETSUCCESS 序列化失败（C# MarketSaveDataItem 含 MarketItem ⇒ 同场景抛异常、不回包）: recog={actor_id}"
            ));
        }
    }
    let _ = tx;
}

/// C# `MarketService.SearchMarketItem`。
async fn search_market_item(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    n_query_id: i32,
    s_data: &[u8],
) {
    let Ok(search) = MarketSearchMessage::from_bytes(s_data) else {
        log::error("解析 MarketSearchMessage 失败");
        return;
    };
    if search.group_id == 0 {
        send_fail(tx, n_query_id, u16::from(DB_SRARCHMARKETFAIL), 0);
        log::info("服务器组[0]拍卖行数据为空,搜索拍卖行数据失败.");
        return;
    }
    // C# 调用把 `SearchItem`/`SearchWho` 传反了（见 `marketdata.rs` 模块头），四个分支恒空。
    let items = db.lock().await.market_search_items(
        search.group_id,
        &search.market_name,
        &search.search_item, // C# 实参位置 = sellWho
        &search.search_who,  // C# 实参位置 = itemName
        search.item_type,
        search.item_set,
    );
    if items.is_empty() {
        send_fail(tx, n_query_id, u16::from(DB_SRARCHMARKETFAIL), 1);
        log::info(&format!(
            "服务器组[{}]拍卖行数据为空,搜索拍卖行数据失败.",
            search.group_id
        ));
        return;
    }
    // C# 构造 MarketDataMessage 后抛 ⇒ 不回包。
    log::error("搜索拍卖行数据序列化失败（C# MarketItem 未注册 MemoryPack ⇒ 不回包）");
}

/// C# `MarketService.QueryMarketUserLoad`。
async fn query_market_user_load(
    db: &Arc<Mutex<Db>>,
    tx: &mpsc::UnboundedSender<Vec<u8>>,
    n_query_id: i32,
    actor_id: i32,
    s_data: &[u8],
) {
    let Ok(user_market) = MarketSearchMessage::from_bytes(s_data) else {
        log::error("解析 MarketSearchMessage 失败");
        return;
    };
    if user_market.group_id == 0 {
        send_fail(tx, n_query_id, u16::from(DB_SRARCHMARKETFAIL), 0);
        log::info("服务器组[0]拍卖行数据为空,搜索拍卖行数据失败.");
        return;
    }
    let count = db
        .lock()
        .await
        .market_query_items_count(user_market.group_id, &user_market.search_who);
    let body = MarkerUserLoadMessage {
        sell_count: count,
        market_npc: user_market.market_npc,
        is_busy: 0,
    };
    send_success(
        tx,
        actor_id,
        u16::from(DB_LOADUSERMARKETSUCCESS),
        &body.to_bytes(),
    );
    log::info(&format!(
        "获取服务器组[{}] 用户[{}]个人拍卖行数据...",
        user_market.group_id, user_market.search_who
    ));
}

// ==== 应答编解码（C# `SendSuccessMessage`/`SendFailMessage`） ====

/// C# `SendFailMessage`：`ServerRequestMessage(ident, 0, param, 0, 0)`，查询号回显请求值。
fn send_fail(tx: &mpsc::UnboundedSender<Vec<u8>>, query_id: i32, ident: u16, param: u16) {
    use mir2_storage::playerdata::ServerRequestMessage;
    send_request_plain(
        tx,
        query_id,
        ServerRequestMessage::new(ident, 0, param, 0, 0),
    );
}

/// C# `SendSuccessMessage`：查询号**恒为 1**，`recog=actorId`。
fn send_success(tx: &mpsc::UnboundedSender<Vec<u8>>, actor_id: i32, ident: u16, body: &[u8]) {
    use mir2_storage::playerdata::ServerRequestMessage;
    send_request_bytes(
        tx,
        1,
        ServerRequestMessage::new(ident, actor_id, 0, 0, 0),
        body,
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    fn vectors_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../tests/parity/vectors/oracle_vectors.jsonl")
    }

    fn unhex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
            .collect()
    }

    /// 取 `kind:"market"` 且 `name` 匹配的向量 hex。
    fn vector(name: &str) -> Vec<u8> {
        let text = std::fs::read_to_string(vectors_path()).expect("read oracle_vectors.jsonl");
        for line in text.lines() {
            if !line.contains("\"kind\":\"market\"") {
                continue;
            }
            let name_key = format!("\"name\":\"{name}\"");
            if !line.contains(&name_key) {
                continue;
            }
            let start = line.find("\"hex\":\"").expect("hex key") + 7;
            let end = line[start..].find('"').expect("hex end") + start;
            return unhex(&line[start..end]);
        }
        panic!("vector {name} not found in {}", vectors_path().display());
    }

    /// C# 真身向量逐条对拍：`MarketRegisterMessage`（4 成员声明序）解出全字段。
    #[test]
    fn market_register_message_true_bytes() {
        let raw = vector("market_register_message");
        let m = MarketRegisterMessage::from_bytes(&raw).unwrap();
        assert_eq!(m.server_index, 1);
        assert_eq!(m.server_name, "LegendOfMir2");
        assert_eq!(m.group_id, 2);
        assert_eq!(m.token, "tok");
    }

    /// `MarketSearchMessage`（10 成员声明序，含 `ItemType=-1` 的 `ffff`）解出全字段。
    #[test]
    fn market_search_message_true_bytes() {
        let raw = vector("market_search_message");
        let m = MarketSearchMessage::from_bytes(&raw).unwrap();
        assert_eq!(m.server_index, 1);
        assert_eq!(m.group_id, 2);
        assert_eq!(m.user_name, "u");
        assert_eq!(m.market_name, "Market");
        assert_eq!(m.search_who, "w");
        assert_eq!(m.search_item, "i");
        assert_eq!(m.item_type, -1);
        assert_eq!(m.item_set, 0);
        assert_eq!(m.user_mode, 1);
        assert_eq!(m.market_npc, 2);
    }

    /// `MarkerUserLoadMessage` 真身向量：`SellCount, MarketNPC, IsBusy` 声明序。
    #[test]
    fn marker_user_load_true_bytes() {
        let msg = MarkerUserLoadMessage {
            sell_count: 3,
            market_npc: 7,
            is_busy: 0,
        };
        assert_eq!(msg.to_bytes(), vector("marker_user_load_message"));
    }

    /// `MarketSaveDataItem`：C# 只要碰到 `Item` 成员就抛 ⇒ 只有空对象头可解。
    #[test]
    fn save_data_item_only_empty_object_decodes() {
        assert!(MarketSaveDataItem::from_bytes(&[0x00]).is_ok());
        // 4 成员 + Item=null（0xFF）：C# 真身探针实测就此抛。
        assert!(MarketSaveDataItem::from_bytes(&unhex("04ff01")).is_err());
        // 4 成员 + 空 Item 对象头（0x00）。
        assert!(MarketSaveDataItem::from_bytes(&unhex("040001")).is_err());
        // 截断/空。
        assert!(MarketSaveDataItem::from_bytes(&[]).is_err());
    }
}
