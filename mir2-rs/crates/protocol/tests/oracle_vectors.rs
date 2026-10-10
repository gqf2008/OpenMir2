//! oracle 向量对拍 —— M0 验收①②的持续门禁（`cargo test -p mir2-protocol` 即触发）。
//!
//! 向量由 `tests/parity/oracle`（C#，引用真身 OpenMir2.dll 的 EDCode/EncryptUtil/SerializerUtil）
//! 生成，文件：`tests/parity/vectors/oracle_vectors.jsonl`（随库提交，含 sha256 见
//! `tests/parity/evidence/M0/` 报告）。
//!
//! 金标准抓包（tests/golden/，C 线产出）到位后由 `tools/replay` 执行同一套判据；
//! 本测试是 oracle 层的静态等价物，**不是**抓包回放的替代品。

// 测试体逐记录校验，行数即判据数；文档逐字引用 C# 标识符。
#![allow(clippy::too_many_lines)]
#![allow(clippy::doc_markdown)]

use mir2_protocol::{
    edcode, frame, messages, CommandMessage, ServerDataMessage, ServerDataType, ServerMessage,
};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
struct Record {
    kind: String,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    plain_hex: Option<String>,
    #[serde(default)]
    encoded_hex: Option<String>,
    #[serde(default)]
    raw_hex: Option<String>,
    #[serde(default)]
    ident: Option<u16>,
    #[serde(default)]
    recog: Option<i32>,
    #[serde(default)]
    param: Option<u16>,
    #[serde(default)]
    tag: Option<u16>,
    #[serde(default)]
    series: Option<u16>,
    #[serde(default)]
    body_len: Option<usize>,
    #[serde(default)]
    body_sha256: Option<String>,
    #[serde(default)]
    packet_code: Option<u32>,
    #[serde(default)]
    socket: Option<i32>,
    #[serde(default)]
    session_id: Option<u16>,
    #[serde(default)]
    session_index: Option<i32>,
    #[serde(default)]
    pack_length: Option<i32>,
    /// 纯字符串帧（无 12 字节头）：只校验编解码往返 + body 长度/hash。
    #[serde(default)]
    string_frame: Option<bool>,
    /// 该帧所属跳（`login`/`sel`/`game`）：s2c 用它给出独立的帧尾期望。
    #[serde(default)]
    hop: Option<String>,
    // ---- kind:"internal"（MemoryPack 内部帧）专有字段 ----
    #[serde(default)]
    msg: Option<String>,
    #[serde(default, rename = "type")]
    type_: Option<u8>,
    #[serde(default)]
    socket_id_utf16_hex: Option<String>,
    #[serde(default)]
    data_hex: Option<String>,
    #[serde(default)]
    body_hex: Option<String>,
    #[serde(default)]
    wire_hex: Option<String>,
    #[serde(default)]
    data_len_field: Option<i16>,
}

fn vectors_path() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/parity/vectors/oracle_vectors.jsonl")
}

#[test]
fn oracle_vectors_byte_exact() {
    let path = vectors_path();
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "read {}: {e}（先跑 tests/parity/oracle 生成向量）",
            path.display()
        )
    });

    let mut n_edcode = 0usize;
    let mut n_frame = 0usize;
    let mut n_header = 0usize;
    let mut n_string_frame = 0usize;
    let mut n_internal = 0usize;
    let mut n_s2c_bang_dollar = 0usize;
    let mut n_s2c_bang = 0usize;
    let mut mismatches: Vec<String> = Vec::new();

    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: Record = serde_json::from_str(line)
            .unwrap_or_else(|e| panic!("line {}: bad json: {e}", lineno + 1));
        match rec.kind.as_str() {
            "edcode" => {
                n_edcode += 1;
                let plain = hex::decode(rec.plain_hex.as_deref().unwrap()).unwrap();
                let encoded = hex::decode(rec.encoded_hex.as_deref().unwrap()).unwrap();
                // ① 双向都要逐字节一致
                if edcode::encode(&plain) != encoded {
                    mismatches.push(format!("line {}: encode mismatch", lineno + 1));
                }
                if edcode::decode(&encoded) != plain {
                    mismatches.push(format!("line {}: decode mismatch", lineno + 1));
                }
            }
            "frame" if rec.string_frame.unwrap_or(false) => {
                n_string_frame += 1;
                let raw = hex::decode(rec.raw_hex.as_deref().unwrap()).unwrap();
                let dir = rec.dir.as_deref().unwrap_or("c2s");
                let offset = if dir == "s2c" { 1 } else { 2 };
                assert_eq!(raw.first(), Some(&b'#'), "line {}", lineno + 1);
                assert_eq!(*raw.last().unwrap(), b'!', "line {}", lineno + 1);
                let body = edcode::decode(&raw[offset..raw.len() - 1]);
                let mut reenc = Vec::with_capacity(body.len() + 4);
                reenc.push(b'#');
                if dir != "s2c" {
                    reenc.push(b'1');
                }
                reenc.extend_from_slice(&edcode::encode(&body));
                reenc.push(b'!');
                if reenc != raw {
                    mismatches.push(format!("line {}: string frame round-trip", lineno + 1));
                }
                if Some(body.len()) != rec.body_len {
                    mismatches.push(format!("line {}: string frame body_len", lineno + 1));
                }
                let hash = hex::encode(Sha256::digest(&body));
                if Some(&hash) != rec.body_sha256.as_ref() {
                    mismatches.push(format!("line {}: string frame body sha256", lineno + 1));
                }
            }
            "frame" => {
                n_frame += 1;
                let raw = hex::decode(rec.raw_hex.as_deref().unwrap()).unwrap();
                let dir = rec.dir.as_deref().unwrap_or("c2s");
                let parsed = match dir {
                    "s2c" => frame::decode_server_payload(
                        &raw[1..raw.len() - if raw.last() == Some(&b'$') { 2 } else { 1 }],
                    )
                    .map(|(head, body)| frame::ClientMessage { head, body })
                    .ok_or(frame::FrameError::MissingTail),
                    _ => frame::decode_client_frame(&raw),
                };
                let msg = match parsed {
                    Ok(m) => m,
                    Err(e) => {
                        mismatches.push(format!("line {}: frame parse: {e}", lineno + 1));
                        continue;
                    }
                };
                // ② 字段对拍
                let want = CommandMessage {
                    recog: rec.recog.unwrap(),
                    ident: rec.ident.unwrap(),
                    param: rec.param.unwrap(),
                    tag: rec.tag.unwrap(),
                    series: rec.series.unwrap(),
                };
                if msg.head != want {
                    mismatches.push(format!(
                        "line {}: head {:?} != {:?}",
                        lineno + 1,
                        msg.head,
                        want
                    ));
                }
                if Some(msg.body.len()) != rec.body_len {
                    mismatches.push(format!("line {}: body_len", lineno + 1));
                }
                let hash = hex::encode(Sha256::digest(&msg.body));
                if Some(&hash) != rec.body_sha256.as_ref() {
                    mismatches.push(format!("line {}: body sha256", lineno + 1));
                }
                // ① decode→encode 与原字节一致
                let reenc = match dir {
                    // s2c 帧尾有两种形态（LoginSrv `!$` / 网关 `!`），按原样复现
                    "s2c" => frame::encode_server_frame_layout(
                        &msg.head,
                        &msg.body,
                        frame::decode_server_frame_ex(&raw).unwrap().1,
                    ),
                    _ => frame::encode_client_frame(&msg.head, &msg.body),
                };
                if reenc != raw {
                    mismatches.push(format!("line {}: byte round-trip", lineno + 1));
                }
                // 帧尾独立判据：hop 声明的形态必须与线上字节一致
                // （逐字节回放对帧尾自洽，改尾不会被 ① 发现）
                if dir == "s2c" {
                    let tail = frame::decode_server_frame_ex(&raw).unwrap().1;
                    match tail {
                        frame::ServerFrameTail::BangDollar => n_s2c_bang_dollar += 1,
                        frame::ServerFrameTail::Bang => n_s2c_bang += 1,
                    }
                    let expect = match rec.hop.as_deref() {
                        Some("login") => Some(frame::ServerFrameTail::BangDollar),
                        Some("sel" | "game") => Some(frame::ServerFrameTail::Bang),
                        _ => None,
                    };
                    if let Some(exp) = expect {
                        if tail != exp {
                            mismatches.push(format!(
                                "line {}: s2c 帧尾 {tail:?} 与 hop 期望 {exp:?} 不符",
                                lineno + 1
                            ));
                        }
                    }
                }
                // ③ 包号必须在消息号表内（或属客户端独有号）
                if messages::names_of(msg.head.ident).is_empty()
                    && !mir2_protocol::client_only::is_client_only(msg.head.ident)
                {
                    mismatches.push(format!(
                        "line {}: ident {} 不在消息号表",
                        lineno + 1,
                        msg.head.ident
                    ));
                }
            }
            "server-header" => {
                n_header += 1;
                let raw = hex::decode(rec.raw_hex.as_deref().unwrap()).unwrap();
                let h = ServerMessage::from_bytes(&raw).expect("server header parse");
                let want = ServerMessage {
                    packet_code: rec.packet_code.unwrap(),
                    socket: rec.socket.unwrap(),
                    session_id: rec.session_id.unwrap(),
                    ident: rec.ident.unwrap(),
                    session_index: rec.session_index.unwrap(),
                    pack_length: rec.pack_length.unwrap(),
                };
                if h != want {
                    mismatches.push(format!("line {}: server header fields", lineno + 1));
                }
                if h.to_bytes().as_slice() != raw.as_slice() {
                    mismatches.push(format!("line {}: server header round-trip", lineno + 1));
                }
            }
            "internal" => {
                n_internal += 1;
                let units: Vec<u16> = hex::decode(rec.socket_id_utf16_hex.as_deref().unwrap())
                    .unwrap()
                    .as_chunks::<2>()
                    .0
                    .iter()
                    .map(|c| u16::from_le_bytes(*c))
                    .collect();
                let socket_id = String::from_utf16(&units).unwrap();
                let data = hex::decode(rec.data_hex.as_deref().unwrap_or("")).unwrap();
                let kind = ServerDataType::from_u8(rec.type_.unwrap()).unwrap();
                let data_len = rec
                    .data_len_field
                    .unwrap_or_else(|| i16::try_from(data.len()).expect("向量载荷长度适配 i16"));
                let msg = ServerDataMessage {
                    kind,
                    socket_id,
                    data_len,
                    data,
                };
                let what = rec.msg.as_deref().unwrap_or("");
                let (got_hex, want_hex) = match what {
                    "ServerDataMessage" => (hex::encode(msg.to_bytes()), rec.body_hex.clone()),
                    "ServerDataWire" => (hex::encode(msg.to_wire()), rec.wire_hex.clone()),
                    other => panic!("line {}: unknown internal msg {other}", lineno + 1),
                };
                if Some(&got_hex) != want_hex.as_ref() {
                    mismatches.push(format!(
                        "line {}: internal {what} 编码不符: got {got_hex} want {want_hex:?}",
                        lineno + 1
                    ));
                }
                // 反向：解析真身字节，字段必须回来
                let raw = hex::decode(want_hex.as_deref().unwrap()).unwrap();
                let back = match what {
                    "ServerDataMessage" => ServerDataMessage::from_bytes(&raw),
                    _ => ServerDataMessage::from_wire(&raw),
                };
                match back {
                    Ok(b) if b == msg => {}
                    other => mismatches.push(format!(
                        "line {}: internal {what} 解析往返不符: {other:?}",
                        lineno + 1
                    )),
                }
            }
            other => panic!("line {}: unknown kind {other}", lineno + 1),
        }
    }

    // 规模锚点：向量集缩水（文件被截断/生成器漏产）也要能发现
    assert!(n_edcode >= 1072, "edcode vectors too few: {n_edcode}");
    assert!(n_frame >= 46, "frame vectors too few: {n_frame}");
    assert_eq!(n_header, 1, "server-header vectors");
    assert!(
        n_string_frame >= 3,
        "string-frame vectors too few: {n_string_frame}"
    );
    // 两种帧尾都必须有向量（否则改尾无从判起）
    assert!(
        n_s2c_bang_dollar >= 1,
        "缺少 `!$` 帧尾（LoginSrv 形态）向量"
    );
    assert!(n_s2c_bang >= 1, "缺少 `!` 帧尾（网关形态）向量");
    assert!(n_internal >= 12, "internal 向量太少: {n_internal}");
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches[..mismatches.len().min(10)].join("\n")
    );
    println!(
        "oracle vectors: {n_edcode} edcode + {n_frame} frame + {n_string_frame} string-frame + {n_header} header + s2c 帧尾 !$×{n_s2c_bang_dollar} / !×{n_s2c_bang} + internal {n_internal}, 0 mismatches"
    );
}

/// 已知坑锚定：SM_ATTACKMODE 恒为 213（服务端为准；客户端分支曾错占给 SM_HERODELMAGIC）。
/// 若有人改消息号表或 C# 源漂移，此测试与 msgcodegen --check 一起红。
#[test]
fn attackmode_is_213() {
    assert_eq!(messages::SM_ATTACKMODE, 213);
    assert!(messages::names_of(213).contains(&"SM_ATTACKMODE"));
}
