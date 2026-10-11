//! **进程级验收**（M1 S4b ②）：用「伪 LoginSrv + 伪 GameSvr」把 Rust db-srv 的 **6000 `DataService` 面**
//! 单独架起来跑，对真实 `mir2_db` 做一轮**取档 → 改字段 → 存档 → 复读库列**的闭环。
//!
//! 覆盖点（逐条对齐 C# `DataService` + `PlayDataStorage.{Query,Save}`）：
//! - 信封：6B `ServerDataPacket` + MemoryPack `ServerRequestData`（`Message`/`Packet`/`Sign` 都是 EDCode）；
//!   校验码 `MakeLong(queryId^170, Message.Length+Packet.Length+6)`，签名取低 16 位；
//! - 取档：先会话校验（`CheckSessionLoadRcd`），再主表 + 附属表读全，回 `DBR_LOADHUMANRCD`(1100) recog=1 series=1；
//! - 存档：回 `DBR_SAVEHUMANRCD`(1102) recog=1，并**按 C# 既有 bug 落库**：
//!   `characters.Job = Job`（**不更新职业**）、`characters.AllowGroupReCall` 收的是 `hd.AllowGroup`、
//!   `characters_ablity.Ac` 收的是 `Abil.Level`、`Hp` 只写 `Abil.HP`（不写 AC）。
//!
//! 端口一律临时分配、配置写在 temp 目录 —— 不动 `E:\MirServer`，不与并行 agent 的现网端口打架。
//!
//! 跑法（需本机 MySQL + `mir2_db`）：
//! ```powershell
//! cargo test -p db-srv --test live_data --offline -- --ignored --nocapture
//! ```

// 文档逐字引用 C# 标识符/文件名，不加反引号改造。
#![allow(clippy::doc_markdown)]

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use mir2_protocol::edcode;
use mir2_protocol::frame::ServerDataPacket;
use mir2_protocol::messages::grobal2::PacketCode;
use mir2_protocol::messages::{
    DBR_LOADHUMANRCD, DBR_SAVEHUMANRCD, DB_LOADHUMANRCD, DB_SAVEHUMANRCD,
};
use mir2_storage::playerdata::{
    Ability, CharacterDataInfo, LoadCharacterData, LoadPlayerDataPacket, MagicRcd, MemPack,
    SaveCharacterData, ServerRequestData, ServerRequestMessage, ServerUserItem,
};
use sqlx::{MySqlPool, Row};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};

/// 测试账号（伪 LoginSrv 推的会话账号必须与请求一致，账号比较大小写敏感）。
const ACCOUNT: &str = "mir2test6k";
/// 会话号（测试自定，不依赖任何登录流程）。
const SESSION: i32 = 6000;

fn free_port() -> u16 {
    let l = std::net::TcpListener::bind("127.0.0.1:0").expect("取空闲端口失败");
    l.local_addr().expect("取本地地址失败").port()
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::new();
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

fn fixture_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("a2-dbsrv-6000-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&d).expect("建 temp 目录失败");
    std::fs::write(d.join("AddrTable.txt"), "127.0.0.1\n").expect("写 AddrTable.txt 失败");
    std::fs::write(d.join("ServerInfo.txt"), "127.0.0.1 127.0.0.1 7200\n")
        .expect("写 ServerInfo.txt 失败");
    d
}

fn write_conf(dir: &Path, gate_port: u16, server_port: u16, ids_port: u16) {
    let body = format!(
        "[Setup]\nShowDebug=1\nServerAddr=127.0.0.1\nServerPort={server_port}\nGateAddr=0.0.0.0\nGatePort={gate_port}\n\
         ServerName=LegendOfMir2\nDenyChrName=1\nDELMaxLevel=30\nDynamicIPMode=0\n\n\
         [Server]\nIDSAddr=127.0.0.1\nIDSPort={ids_port}\nMarketAddr=127.0.0.1\nMarketPort={}\n\n\
         [DataBase]\nStoreage=MySQL\nConnctionString=server=127.0.0.1;uid=root;pwd=;database=mir2_db;\n\n\
         [DBClear]\nInterval=3000\n",
        free_port()
    );
    std::fs::write(dir.join("dbsvr.conf"), body).expect("写 dbsvr.conf 失败");
}

fn log_file(dir: &Path, name: &str) -> std::fs::File {
    std::fs::File::create(dir.join(name)).expect("建日志文件失败")
}

struct Proc(Option<Child>);

impl Drop for Proc {
    fn drop(&mut self) {
        if let Some(mut c) = self.0.take() {
            c.kill().ok();
            c.wait().ok();
        }
    }
}

fn start_rust_dbsrv(dir: &Path) -> Proc {
    let exe = env!("CARGO_BIN_EXE_db-srv");
    let child = Command::new(exe)
        .arg(dir.join("dbsvr.conf"))
        .current_dir(dir)
        .stdout(Stdio::from(log_file(dir, "dbsrv.out.log")))
        .stderr(Stdio::from(log_file(dir, "dbsrv.err.log")))
        .spawn()
        .expect("起 Rust db-srv 失败");
    eprintln!(
        "[fixture] Rust db-srv pid={} dir={}",
        child.id(),
        dir.display()
    );
    Proc(Some(child))
}

async fn fake_login_srv() -> (u16, Arc<AtomicBool>) {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("伪 LoginSrv 绑定失败");
    let port = listener.local_addr().unwrap().port();
    let accepted = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&accepted);
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = listener.accept().await else {
                return;
            };
            let flag = Arc::clone(&flag);
            tokio::spawn(async move {
                let msg = format!("(100/{ACCOUNT}/{SESSION}/0/0/127.0.0.1/0)");
                let mut tick = tokio::time::interval(Duration::from_millis(300));
                let mut buf = [0u8; 256];
                loop {
                    tokio::select! {
                        _ = tick.tick() => {
                            if sock.write_all(msg.as_bytes()).await.is_err() {
                                break;
                            }
                            sock.flush().await.ok();
                            flag.store(true, Ordering::SeqCst);
                        }
                        r = sock.read(&mut buf) => {
                            if !matches!(r, Ok(n) if n > 0) {
                                break;
                            }
                        }
                    }
                }
            });
        }
    });
    (port, accepted)
}

async fn wait_listen(port: u16) {
    for _ in 0..100 {
        if TcpStream::connect(("127.0.0.1", port)).await.is_ok() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("等端口 {port} 监听超时");
}

async fn wait_session_link(flag: &AtomicBool) {
    for _ in 0..150 {
        if flag.load(Ordering::SeqCst) {
            tokio::time::sleep(Duration::from_millis(400)).await;
            return;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    panic!("DBSrv 未连上伪 LoginSrv");
}

// ==== 6000 信封 ====

fn low16(v: i32) -> u16 {
    u16::try_from(v & 0xFFFF).unwrap_or(0)
}

fn make_long(low: u16, high: u16) -> i32 {
    (u32::from(low) | (u32::from(high) << 16)).cast_signed()
}

/// 造一条 `ServerRequestData` 请求（C# GameSvr `SendRequest` 的形态）。
fn build_request(query_id: i32, ident: u16, body: &[u8]) -> Vec<u8> {
    let message = edcode::encode(&ServerRequestMessage::new(ident, 0, 0, 0, 0).to_bytes());
    let packet = edcode::encode(body);
    let len = message.len() + packet.len() + ServerDataPacket::SIZE;
    let check = make_long(low16(query_id ^ 0xAA), u16::try_from(len).unwrap());
    let sign = edcode::encode(&check.to_le_bytes());
    let request = ServerRequestData {
        query_id,
        message: Some(message),
        packet: Some(packet),
        sign: Some(sign),
    };
    let body_bytes = request.to_bytes();
    let header = ServerDataPacket {
        packet_code: PacketCode,
        packet_len: u16::try_from(body_bytes.len()).unwrap(),
    };
    let mut wire = header.to_bytes().to_vec();
    wire.extend_from_slice(&body_bytes);
    wire
}

/// 发一条请求并读回一条应答，返回 `(ServerRequestMessage, packet 体)`。
async fn roundtrip(
    sock: &mut TcpStream,
    query_id: i32,
    ident: u16,
    body: &[u8],
) -> (ServerRequestMessage, Option<Vec<u8>>) {
    sock.write_all(&build_request(query_id, ident, body))
        .await
        .expect("发 6000 请求失败");
    sock.flush().await.ok();
    let mut head = [0u8; ServerDataPacket::SIZE];
    tokio::time::timeout(Duration::from_secs(10), sock.read_exact(&mut head))
        .await
        .expect("等 6000 应答超时")
        .expect("读 6000 应答头失败");
    let pkt = ServerDataPacket::from_bytes(&head).expect("6000 应答头非法");
    assert_eq!(pkt.packet_code, PacketCode);
    let mut body = vec![0u8; usize::from(pkt.packet_len)];
    sock.read_exact(&mut body)
        .await
        .expect("读 6000 应答体失败");
    let resp = ServerRequestData::from_bytes(&body).expect("6000 应答信封非法");
    let msg = ServerRequestMessage::from_bytes(&edcode::decode(resp.message.as_ref().unwrap()))
        .expect("6000 应答消息头非法");
    let decoded = resp.packet.as_ref().map(|p| edcode::decode(p));
    (msg, decoded)
}

/// `mysql://…`（用 `LegacyDsn` 与 db-srv 完全同口径解析现网连接串）。
async fn connect_db() -> MySqlPool {
    let dsn = mir2_storage::LegacyDsn::parse("server=127.0.0.1;uid=root;pwd=;database=mir2_db;")
        .expect("解析 DSN 失败");
    MySqlPool::connect(&dsn.to_sqlx_url())
        .await
        .expect("连 MySQL 失败（需要本机 mir2_db）")
}

/// 插入一份带区分度的夹具角色，返回 `(name, characters.Id)`。
async fn seed_character(pool: &MySqlPool, name: &str) -> i64 {
    cleanup(pool, name).await;
    let res = sqlx::query(
        "INSERT INTO characters (LoginID, ChrName, MapName, CX, CY, Level, Dir, Hair, Sex, Job, Gold, GamePoint, \
         HomeMap, HomeX, HomeY, PkPoint, ReLevel, AttatckMode, FightZoneDieCount, BodyLuck, IncHealth, IncSpell, \
         IncHealing, CreditPoint, BonusPoint, HungerStatus, PayMentPoint, LockLogon, MarryCount, AllowGroup, \
         AllowGroupReCall, GroupRcallTime, AllowGuildReCall, IsMaster, MasterName, DearName, StoragePwd, Deleted, \
         CREATEDATE, LASTUPDATE) VALUES \
         (?, ?, '0', 111, 222, 7, 3, 4, 1, 2, 5555, 66, '1', 5, 6, 77, 8, 9, 10, 1.25, 11, 12, 13, 14, 15, 16, 17, 1, 18, 19, 20, 21, 1, 1, 'mstr', 'dear', 'pw', 0, now(), now())",
    )
    .bind(ACCOUNT)
    .bind(name)
    .execute(pool)
    .await
    .expect("插入 characters 失败");
    let id = i64::try_from(res.last_insert_id()).unwrap();
    // ablity：`Hp`/`Mp` 是 **smallint**（≤32767）——C# `GetInt32("HP")` 再 `LoWord/HiWord` 取
    // HP/AC，但 smallint 装不下高 16 位 ⇒ **AC 读出来恒为 0**（C# 既有 bug，这里断言它）。
    sqlx::query(
        "INSERT INTO characters_ablity (PlayerId, Level, Ac, Mac, Dc, Mc, Sc, Hp, Mp, MaxHP, MAxMP, Exp, MaxExp, \
         Weight, MaxWeight, WearWeight, MaxWearWeight, HandWeight, MaxHandWeight) VALUES (?, 7, 41, 42, 1, 2, 3, 7, 8, 101, 102, 100, 1000, 10, 100, 11, 110, 12, 120)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_ablity 失败");
    sqlx::query(
        "INSERT INTO characters_status (PlayerId, Status0, Status1, Status2, Status3, Status4, Status5, Status6, Status7, Status8, Status9, Status10, Status11, Status12, Status13, Status14, Status15) VALUES (?, 111, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 222, 0)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_status 失败");
    sqlx::query(
        "INSERT INTO characters_bonusability (PLAYERID, AC, MAC, DC, MC, SC, HP, MP, HIT, SPEED, RESERVED) VALUES (?, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_bonusability 失败");
    sqlx::query(
        "INSERT INTO characters_magic (PlayerId, MagicId, Level, UseKey, CurrTrain) VALUES (?, 17, 1, '8', 44)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_magic 失败");
    sqlx::query(
        "INSERT INTO characters_item (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) VALUES (?, 0, 0, 0, 0, 0)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_item 槽 0 失败");
    sqlx::query(
        "INSERT INTO characters_item (PlayerId, Position, MakeIndex, StdIndex, Dura, DuraMax) VALUES (?, 1, 123, 45, 6, 7)",
    )
    .bind(id)
    .execute(pool)
    .await
    .expect("插入 characters_item 失败");
    id
}

async fn cleanup(pool: &MySqlPool, name: &str) {
    if let Ok(Some(row)) = sqlx::query("SELECT Id FROM characters WHERE ChrName=?")
        .bind(name)
        .fetch_optional(pool)
        .await
    {
        let id: i64 = row.try_get("Id").unwrap_or(0);
        for table in [
            "characters_ablity",
            "characters_status",
            "characters_bonusability",
            "characters_magic",
            "characters_item",
            "characters_bagitem",
            "characters_storageitem",
            "characters_item_attr",
        ] {
            let sql = format!("DELETE FROM {table} WHERE PlayerId=?");
            sqlx::query(&sql).bind(id).execute(pool).await.ok();
        }
    }
    sqlx::query("DELETE FROM characters WHERE ChrName=?")
        .bind(name)
        .execute(pool)
        .await
        .ok();
    sqlx::query("DELETE FROM characters_indexes WHERE ChrName=?")
        .bind(name)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
#[ignore = "需要本机 MySQL(mir2_db)，会起真正的 db-srv 进程 + 写临时角色行"]
#[allow(clippy::too_many_lines)]
async fn rust_dbsrv_6000_load_save_roundtrip() {
    let dir = fixture_dir("data");
    let gate_port = free_port();
    let server_port = free_port();
    let (ids_port, accepted) = fake_login_srv().await;
    write_conf(&dir, gate_port, server_port, ids_port);

    // 1) 先建夹具角色（DBSrv 启动时载入内存索引，所以必须在起进程之前写库）。
    let pool = connect_db().await;
    let name = format!("zz6k{}", std::process::id());
    let id = seed_character(&pool, &name).await;
    println!("[fixture] characters.Id={id} name={name}");

    // 2) 起 Rust db-srv 并等 6000 监听 + LoginSrv 会话到位。
    let dbsrv = start_rust_dbsrv(&dir);
    wait_listen(server_port).await;
    wait_session_link(&accepted).await;

    let mut sock = TcpStream::connect(("127.0.0.1", server_port))
        .await
        .expect("连 6000 面失败");
    sock.set_nodelay(true).ok();

    // 3) 取档：`DB_LOADHUMANRCD`(100) → `DBR_LOADHUMANRCD`(1100) recog=1 series=1 + `LoadPlayerDataPacket`。
    let load = LoadCharacterData {
        account: Some(ACCOUNT.into()),
        chr_name: Some(name.clone()),
        user_addr: Some("127.0.0.1".into()),
        session_id: SESSION,
    };
    let (msg, packet) = roundtrip(&mut sock, 1, DB_LOADHUMANRCD, &load.to_bytes()).await;
    println!("[load] reply={msg:?}");
    assert_eq!(msg.ident, DBR_LOADHUMANRCD);
    assert_eq!((msg.recog, msg.param, msg.tag, msg.series), (1, 0, 0, 1));
    let packet = packet.expect("取档应答缺业务体");
    println!(
        "[load] packet sha256 prefix={}",
        hex(&packet[..16.min(packet.len())])
    );
    let reply = LoadPlayerDataPacket::from_bytes(&packet).expect("LoadPlayerDataPacket 解析失败");
    // 回包的 ChrName 是 `EDCode.EncodeString(name)`（GBK→EDCode）——先解回 GBK 字节再比。
    let enc_name = reply.chr_name.clone().expect("缺 ChrName");
    assert_eq!(
        edcode::decode(enc_name.as_bytes()),
        name.as_bytes().to_vec(),
        "回包 ChrName 应是 EDCode(GBK(name))"
    );
    let data = reply.hum_data_info.expect("缺 HumDataInfo").data.unwrap();
    // 字段级断言（与夹具插入值一致；`PKPoint` C# 不读 ⇒ 0）。
    assert_eq!(data.cur_x, 111);
    assert_eq!(data.cur_y, 222);
    assert_eq!(data.dir, 3);
    assert_eq!(data.hair, 4);
    assert_eq!(data.sex, 1);
    assert_eq!(data.job, 2);
    assert_eq!(data.gold, 5555);
    assert_eq!(data.game_gold, 5555, "GameGold 与 Gold 同列");
    assert_eq!(data.game_point, 66);
    assert_eq!(data.re_level, 8);
    assert_eq!(data.pk_point, 0, "C# GetChrRecord 不读 PkPoint");
    assert_eq!(data.allow_group, 19);
    assert!(data.allow_group_re_call, "AllowGroupReCall=20 非 0");
    assert!(data.allow_guild_re_call);
    assert_eq!(data.master_name.as_deref(), Some("mstr"));
    assert_eq!(data.dear_name.as_deref(), Some("dear"));
    assert_eq!(data.storage_pwd.as_deref(), Some("pw"));
    let abil = data.abil.clone().unwrap();
    assert_eq!(abil.level, 7);
    assert_eq!(abil.hp, 7);
    assert_eq!(
        abil.ac, 0,
        "C# 从 HiWord(smallint Hp) 取 AC ⇒ 恒 0（既有 bug）"
    );
    assert_eq!(abil.mp, 8);
    assert_eq!(abil.mac, 0, "同上：HiWord(Mp) 恒 0");
    assert_eq!(abil.dc, 1);
    assert_eq!(abil.weight, 10);
    let bonus = data.bonus_abil.clone().unwrap();
    assert_eq!(
        (bonus.ac, bonus.hit, bonus.speed, bonus.reserved),
        (1, 8, 9, 10)
    );
    assert_eq!(data.status_time_arr.as_ref().unwrap()[0], 111);
    assert_eq!(data.status_time_arr.as_ref().unwrap()[14], 222);
    let magic = data.magic.as_ref().unwrap();
    let m0 = magic[0].as_ref().expect("技能槽 0");
    assert_eq!(m0.mag_idx, 17);
    assert_eq!(m0.level, 1);
    assert_eq!(m0.magic_key, u16::from(b'8'));
    assert_eq!(m0.tran_point, 44);
    let hum = data.hum_items.as_ref().unwrap();
    let item1 = hum[1].as_ref().expect("穿戴槽 1");
    assert_eq!(item1.make_index, 123);
    assert_eq!(item1.index, 45);
    assert_eq!(item1.dura, 6);
    assert_eq!(item1.dura_max, 7);

    // 4) 存档：改一批字段（含 C# 既有 bug 的验证点）。
    let mut save_data = data.clone();
    save_data.gold = 7777;
    save_data.game_point = 123;
    save_data.job = 9; // 期望：characters.Job 不更新（SQL 是 `Job = Job`）
    save_data.allow_group = 33; // 期望：写进 characters.AllowGroupReCall
    save_data.cur_x = 333;
    save_data.abil = Some(Ability {
        level: 42,
        hp: 99,
        mac: 4,
        dc: 5,
        ..Ability::default()
    });
    save_data.status_time_arr.as_mut().unwrap()[0] = 555;
    save_data.magic.as_mut().unwrap()[0] = Some(MagicRcd {
        mag_idx: 17,
        level: 2,
        magic_key: u16::from(b'9'),
        tran_point: 55,
    });
    // 穿戴槽 0：新物品（chg）；穿戴槽 1：清空（`MakeIndex=0` 而旧值 >0 ⇒ del）。
    save_data.hum_items.as_mut().unwrap()[0] = Some(ServerUserItem {
        make_index: 999,
        index: 45,
        dura: 6,
        dura_max: 7,
        desc: Some({
            let mut d = vec![0u8; 14];
            d[0] = 7;
            d[13] = 8;
            d
        }),
        ..ServerUserItem::csharp_new()
    });
    save_data.hum_items.as_mut().unwrap()[1] = Some(ServerUserItem::csharp_new());
    let save = SaveCharacterData {
        account: Some(ACCOUNT.into()),
        chr_name: Some(name.clone()),
        character_data: Some(CharacterDataInfo {
            header: {
                let mut h = mir2_storage::playerdata::RecordHeader::default();
                h.set_name(Some(name.clone()));
                h
            },
            data: Some(save_data),
        }),
    };
    let (msg, _) = roundtrip(&mut sock, 2, DB_SAVEHUMANRCD, &save.to_bytes()).await;
    println!("[save] reply={msg:?}");
    assert_eq!(msg.ident, DBR_SAVEHUMANRCD);
    assert_eq!(msg.recog, 1);

    // 5) 复读库列，验证「按 C# 既有 bug 落库」。
    let row = sqlx::query(
        "SELECT Gold, Job, AllowGroup, AllowGroupReCall, CX, MapName FROM characters WHERE ID=?",
    )
    .bind(id)
    .fetch_one(&pool)
    .await
    .expect("复读 characters 失败");
    assert_eq!(row.get::<i32, _>("Gold"), 7777);
    assert_eq!(row.get::<i8, _>("Job"), 2, "C# `Job = Job`：职业不更新");
    assert_eq!(
        row.get::<i32, _>("AllowGroup"),
        19,
        "AllowGroup 列不被存档改写"
    );
    assert_eq!(
        row.get::<i8, _>("AllowGroupReCall"),
        33,
        "C# 把 hd.AllowGroup 写进 AllowGroupReCall"
    );
    assert_eq!(row.get::<i32, _>("CX"), 333);
    assert_eq!(row.get::<String, _>("MapName"), "0");
    let ab = sqlx::query("SELECT Level, Ac, Hp, Mac FROM characters_ablity WHERE PlayerId=?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("复读 characters_ablity 失败");
    assert_eq!(
        ab.get::<u8, _>("Level"),
        42,
        "tinyint(1) UNSIGNED ⇒ sqlx BOOLEAN，按 u8 读（与 db-srv int_col 阶梯一致）"
    );
    assert_eq!(ab.get::<i16, _>("Ac"), 42, "C# `Ac = @Ac` 绑的是 Level");
    assert_eq!(ab.get::<i16, _>("Hp"), 99, "C# 只写 HP，不把 AC 塞回高位");
    assert_eq!(ab.get::<i16, _>("Mac"), 4);
    let st = sqlx::query("SELECT Status0 FROM characters_status WHERE PlayerId=?")
        .bind(id)
        .fetch_one(&pool)
        .await
        .expect("复读 characters_status 失败");
    assert_eq!(st.get::<i32, _>("Status0"), 555);
    let mg = sqlx::query("SELECT MagicId, Level, CurrTrain FROM characters_magic WHERE PlayerId=?")
        .bind(id)
        .fetch_all(&pool)
        .await
        .expect("复读 characters_magic 失败");
    assert_eq!(mg.len(), 1, "旧技能行被 DELETE 后只插新行");
    assert_eq!(mg[0].get::<i32, _>("MagicId"), 17);
    assert_eq!(mg[0].get::<i8, _>("Level"), 2);
    assert_eq!(mg[0].get::<i32, _>("CurrTrain"), 55);
    let it = sqlx::query("SELECT Position, MakeIndex, StdIndex FROM characters_item WHERE PlayerId=? ORDER BY Position")
        .bind(id)
        .fetch_all(&pool)
        .await
        .expect("复读 characters_item 失败");
    let mut positions: Vec<(i32, i32)> = it
        .iter()
        .map(|r| (r.get::<i32, _>("Position"), r.get::<i32, _>("MakeIndex")))
        .collect();
    positions.sort_unstable();
    assert_eq!(
        positions,
        vec![(0, 999), (1, 0)],
        "槽 0 换物（chg）、槽 1 清空（del）"
    );

    // 6) 收尾：停进程、删夹具行、删 temp 目录。
    drop(dbsrv);
    cleanup(&pool, &name).await;
    std::fs::remove_dir_all(&dir).ok();
}
