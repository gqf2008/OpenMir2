//! replay —— M0 回放门禁（设计文档 §6 M0 验收①②③ + 红检三连）。
//!
//! 输入：JSONL，每行一条记录：
//! - `{"kind":"edcode","plain_hex":"..","encoded_hex":".."}` —— 纯编解码向量（C# oracle 产物）
//! - `{"kind":"frame","dir":"c2s"|"s2c","seq":N,"ts_ms":N,"raw_hex":"..",`
//!   `"ident":N,"recog":N,"param":N,"tag":N,"series":N,"body_len":N,"body_sha256":".."}` —— 帧向量
//!   （字段值是 C# 侧同一时刻观测值；raw_hex 是线上完整帧字节）
//! - s2c 帧建议带 `"hop"`（`login`/`sel`/`game`）：login 跳帧尾应为 `!$`（LoginSrv 构造），
//!   其余跳为 `!`；同在 7000 但由 LoginGate 自身产生的帧用 `"tail":"!"` 显式覆盖。
//!   缺这两个字段时帧尾不参与判据（仅自洽往返，改尾不可见）。
//! - 纯字符串帧（无 12 字节头，如 GameGate 首个上行 `**账号/角色/...`）在同一行加
//!   `"string_frame":true`：只校验 ① 逐字节回放与 body 长度/hash，不做头字段对拍、
//!   不计入包号覆盖（`--sabotage ident` 对这类帧不适用，其余帧仍会检验它）。
//!
//! 判据（全部满足才绿，exit 0）：
//! ① 每个 frame 记录 decode→encode 后与 raw_hex 100% 相同；
//! ② 每个 frame 记录的 (ident,recog,param,tag,series,body_len,body_sha256) 与记录值逐项一致；
//! ③ 包号集合覆盖八阶段（登录/选服/选角/建角/进世界/移动/攻击/小退）且未识别包号数 = 0
//!    （仅对金标准抓包强制执行；oracle 向量文件不强制覆盖，用 `--no-coverage` 跳过）。
//!
//! 红检（`--sabotage`）：破坏输入后期望门禁**变红**（exit 1）；
//! 若破坏后仍绿（exit 3）说明门禁失效。
//!   `key`      用 seed+1 的错误密钥解码 → ①② 必须红；
//!   `ident`    解码后把 ident+1 再比对  → ② 必须红；
//!   `truncate` 每帧 body 截 1 字节       → ① 必须红。
//!
//! 用法：`replay <vectors.jsonl> [--no-coverage] [--sabotage key|ident|truncate]`

// 文档逐字引用字段名/C# 标识符，不加反引号改造。
#![allow(clippy::doc_markdown)]

use anyhow::{bail, Context, Result};
use mir2_protocol::{edcode, frame, messages};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Deserialize)]
struct Record {
    kind: String,
    #[serde(default)]
    dir: Option<String>,
    #[serde(default)]
    seq: Option<u64>,
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
    // server-header 专有字段
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
    /// 纯字符串帧（无 12 字节头；GameGate 首个上行 `**账号/角色/...` 一类）。
    #[serde(default)]
    string_frame: Option<bool>,
    /// 该帧所属跳：`login`(7000) / `sel`(7100) / `game`(7200)。
    /// s2c 帧用它给出**独立的帧尾期望**（login ⇒ `!$`，sel/game ⇒ `!`），
    /// 否则帧尾只有自洽往返、改尾不可见（该盲区由红检实测发现）。
    #[serde(default)]
    hop: Option<String>,
    /// 显式帧尾期望，优先于 `hop` 推断（用于 LoginGate 自身产生的帧：同在 7000 但无 `$`）。
    #[serde(default)]
    tail: Option<String>,
}

/// 八阶段 → 覆盖该阶段的 ident 值集合（按 Messages.cs 语义枚举，值而非名，规避重名）。
const STAGES: &[(&str, &[u16])] = &[
    ("登录", &[2000, 2001, 5001, 5002, 502, 503, 529, 531]),
    ("选服", &[104, 530]),
    ("选角", &[100, 520, 527]),
    ("建角", &[101, 521, 522]),
    ("进世界", &[103, 525, 526, 50, 51, 52, 54]),
    ("移动", &[3010, 3011, 3013, 10, 11, 13, 28]),
    ("攻击", &[3014, 3017, 14, 17, 31]),
    ("小退", &[1009, 528, 802]),
];

#[derive(Default)]
struct Stats {
    total_frames: usize,
    total_edcode: usize,
    byte_mismatch: usize,
    field_mismatch: usize,
    unknown_idents: BTreeSet<u16>,
    seen_idents: BTreeSet<u16>,
    /// 纯字符串帧条数（只参与 ① 逐字节回放与 body 对拍，不参与包号覆盖）
    string_frames: usize,
    first_diffs: Vec<String>,
}

impl Stats {
    fn diff(&mut self, msg: String) {
        if self.first_diffs.len() < 10 {
            self.first_diffs.push(msg);
        }
    }
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

/// 用指定 seed 的解码（红检①专用：传入 SEED+1 模拟密钥错 1 字节）。
/// 与 [`edcode::decode`] 同算法、种子可换 —— 仅用于红检，不进冻结 API。
fn decode_with_seed(src: &[u8], seed: u8) -> Vec<u8> {
    let len = src.len();
    let cycles = len / 4;
    let bytes_left = len % 4;
    let mut dst = Vec::with_capacity(edcode::decoded_len(len));
    let base = edcode::BASE;
    for i in 0..cycles {
        let b = i * 4;
        let remainder = src[b + 3].wrapping_sub(base);
        let temp = src[b].wrapping_sub(base);
        dst.push((((temp << 2) & 0xF0) | (remainder & 0x0C) | (temp & 0x3)) ^ seed);
        let temp = src[b + 1].wrapping_sub(base);
        dst.push((((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3)) ^ seed);
        let temp = src[b + 2].wrapping_sub(base);
        dst.push((temp | ((remainder << 2) & 0xC0)) ^ seed);
    }
    if bytes_left == 2 {
        let remainder = src[len - 1].wrapping_sub(base);
        let temp = src[len - 2].wrapping_sub(base);
        dst.push((((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3)) ^ seed);
    } else if bytes_left == 3 {
        let remainder = src[len - 1].wrapping_sub(base);
        let temp = src[len - 3].wrapping_sub(base);
        dst.push((((temp << 2) & 0xF0) | (remainder & 0x0C) | (temp & 0x3)) ^ seed);
        let temp = src[len - 2].wrapping_sub(base);
        dst.push((((temp << 2) & 0xF0) | ((remainder << 2) & 0x0C) | (temp & 0x3)) ^ seed);
    }
    dst
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Sabotage {
    None,
    Key,
    Ident,
    Truncate,
}

fn parse_args() -> (String, bool, Sabotage) {
    let argv: Vec<String> = std::env::args().skip(1).collect();
    let Some(path) = argv.first().cloned() else {
        eprintln!("usage: replay <vectors.jsonl> [--no-coverage] [--sabotage key|ident|truncate]");
        std::process::exit(2);
    };
    let mut coverage = true;
    let mut sabotage = Sabotage::None;
    let mut i = 1;
    while i < argv.len() {
        match argv[i].as_str() {
            "--no-coverage" => coverage = false,
            "--sabotage" => {
                let Some(mode) = argv.get(i + 1) else {
                    eprintln!("--sabotage requires key|ident|truncate");
                    std::process::exit(2);
                };
                sabotage = match mode.as_str() {
                    "key" => Sabotage::Key,
                    "ident" => Sabotage::Ident,
                    "truncate" => Sabotage::Truncate,
                    other => {
                        eprintln!("unknown sabotage mode: {other}");
                        std::process::exit(2);
                    }
                };
                i += 1;
            }
            other => {
                eprintln!("unknown arg: {other}");
                std::process::exit(2);
            }
        }
        i += 1;
    }
    (path, coverage, sabotage)
}

fn process_edcode(rec: &Record, lineno: usize, st: &mut Stats) -> Result<()> {
    st.total_edcode += 1;
    let plain = hex::decode(rec.plain_hex.as_deref().unwrap_or(""))?;
    let expected_enc = hex::decode(rec.encoded_hex.as_deref().unwrap_or(""))?;
    let got_enc = edcode::encode(&plain);
    let got_dec = edcode::decode(&expected_enc);
    if got_enc != expected_enc || got_dec != plain {
        st.byte_mismatch += 1;
        st.diff(format!("line {}: edcode vector mismatch", lineno + 1));
    }
    Ok(())
}

/// 内部帧头 `ServerMessage`（20B）向量：序列化字节与字段逐项比对。
fn process_server_header(rec: &Record, lineno: usize, st: &mut Stats) -> Result<()> {
    st.total_frames += 1;
    let raw = hex::decode(rec.raw_hex.as_deref().unwrap_or(""))?;
    let Some(h) = frame::ServerMessage::from_bytes(&raw) else {
        st.byte_mismatch += 1;
        st.diff(format!("line {}: server header parse failed", lineno + 1));
        return Ok(());
    };
    // 序列化往返必须逐字节一致
    if h.to_bytes().as_slice() != raw.as_slice() {
        st.byte_mismatch += 1;
        st.diff(format!(
            "line {}: server header round-trip mismatch",
            lineno + 1
        ));
    }
    let want = (
        rec.packet_code,
        rec.socket,
        rec.session_id,
        rec.ident,
        rec.session_index,
        rec.pack_length,
    );
    if let (Some(pc), Some(sock), Some(sid), Some(ident), Some(sidx), Some(plen)) = want {
        if h.packet_code != pc
            || h.socket != sock
            || h.session_id != sid
            || h.ident != ident
            || h.session_index != sidx
            || h.pack_length != plen
        {
            st.field_mismatch += 1;
            st.diff(format!("line {}: server header field mismatch", lineno + 1));
        }
    }
    Ok(())
}

/// 纯字符串帧：解码出裸体（无 12 字节 `CommandMessage` 头）。
///
/// 参照路径：GameGate 收到首个上行时走
/// `EDCode.DeCodeString(destinationSpan[2..packetLen-1])`，载荷是编码后的登录串，
/// 窗口内没有头字段；LoginGate/SelGate 存在同类形态。
fn decode_string_frame(raw: &[u8], dir: &str, sabotage: Sabotage) -> Option<Vec<u8>> {
    let offset = if dir == "s2c" { 1 } else { 2 };
    if raw.first() != Some(&b'#') || raw.last() != Some(&b'!') || raw.len() <= offset + 1 {
        return None;
    }
    let payload = &raw[offset..raw.len() - 1];
    Some(if sabotage == Sabotage::Key {
        decode_with_seed(payload, edcode::SEED.wrapping_add(1))
    } else {
        edcode::decode(payload)
    })
}

/// 按方向与红检模式解析一帧。返回 None 表示解析失败（本身计入 ① 差异）。
fn parse_frame(raw: &[u8], dir: &str, sabotage: Sabotage) -> Option<frame::ClientMessage> {
    if sabotage == Sabotage::Key {
        // 红检①：用错误密钥解码，后续字段/字节比对自然变红
        let offset = if dir == "s2c" { 1 } else { 2 };
        if raw.len() <= offset + 1 {
            return None;
        }
        let decoded = decode_with_seed(&raw[offset..raw.len() - 1], edcode::SEED.wrapping_add(1));
        return frame::CommandMessage::from_bytes(&decoded).map(|head| frame::ClientMessage {
            head,
            body: decoded
                .get(frame::CommandMessage::SIZE..)
                .unwrap_or(&[])
                .to_vec(),
        });
    }
    match dir {
        // 登录跳（LoginSrv 构造、LoginGate 转发）帧尾是 `!$`，其余跳是 `!`
        // —— 两种都要接受，帧尾形态在 process_frame 里按原样复现。
        "s2c" => frame::decode_server_frame_ex(raw).ok().map(|(msg, _)| msg),
        _ => frame::decode_client_frame(raw).ok(),
    }
}

/// 纯字符串帧的校验：① 逐字节回放 + ② body 长度/hash（无头字段、不计包号覆盖）。
fn process_string_frame(
    rec: &Record,
    lineno: usize,
    sabotage: Sabotage,
    st: &mut Stats,
) -> Result<()> {
    st.total_frames += 1;
    let raw_orig = hex::decode(rec.raw_hex.as_deref().unwrap_or(""))?;
    let mut raw = raw_orig.clone();
    if sabotage == Sabotage::Truncate && raw.len() > 3 {
        raw.remove(raw.len() - 2);
    }
    let dir = rec.dir.as_deref().unwrap_or("c2s");
    let Some(body) = decode_string_frame(&raw, dir, sabotage) else {
        st.byte_mismatch += 1;
        st.diff(format!(
            "line {}: string frame parse failed (seq {:?})",
            lineno + 1,
            rec.seq
        ));
        return Ok(());
    };
    // ① 逐字节回放：`#`/`#1` + 编码体 + `!`
    let mut reenc = Vec::with_capacity(body.len() + 4);
    reenc.push(b'#');
    if dir != "s2c" {
        reenc.push(b'1');
    }
    reenc.extend_from_slice(&edcode::encode(&body));
    reenc.push(b'!');
    if reenc != raw_orig {
        st.byte_mismatch += 1;
        st.diff(format!(
            "line {}: string frame byte round-trip mismatch (seq {:?})",
            lineno + 1,
            rec.seq
        ));
    }
    // ② 只对拍 body（无头字段可比）
    if let Some(want_len) = rec.body_len {
        if body.len() != want_len {
            st.field_mismatch += 1;
            st.diff(format!(
                "line {}: string frame body_len mismatch (seq {:?}): got {} want {want_len}",
                lineno + 1,
                rec.seq,
                body.len()
            ));
        }
    }
    if let Some(want_hash) = &rec.body_sha256 {
        if sha256_hex(&body) != *want_hash {
            st.field_mismatch += 1;
            st.diff(format!(
                "line {}: string frame body sha256 mismatch (seq {:?})",
                lineno + 1,
                rec.seq
            ));
        }
    }
    st.string_frames += 1;
    Ok(())
}

/// s2c 帧尾的**独立**期望校验：`hop`/`tail` 声明时比对实际帧尾。
///
/// 逐字节回放对帧尾是自洽的（解出尾、再原样编码回去），单独改尾 ① 看不见，
/// 故帧尾必须由记录里的外部期望来判（该盲区由红检实测暴露，见 evidence/M0/redcheck4）。
fn check_s2c_tail_expectation(rec: &Record, lineno: usize, dir: &str, raw: &[u8], st: &mut Stats) {
    // ②事前：帧尾独立期望（`hop`/`tail` 声明时）。逐字节回放对帧尾是自洽的，
    // 单独改尾不会被 ① 发现，因此这里必须有独立判据。
    if dir == "s2c" {
        let expect = match rec.tail.as_deref() {
            Some("!") => Some(frame::ServerFrameTail::Bang),
            Some("!$") => Some(frame::ServerFrameTail::BangDollar),
            Some(other) => {
                st.field_mismatch += 1;
                st.diff(format!("line {}: bad tail field {other:?}", lineno + 1));
                None
            }
            None => match rec.hop.as_deref() {
                Some("login") => Some(frame::ServerFrameTail::BangDollar),
                Some("sel" | "game") => Some(frame::ServerFrameTail::Bang),
                _ => None,
            },
        };
        let got = frame::ServerFrameTail::from_last_byte(*raw.last().unwrap_or(&0));
        if let Some(exp) = expect {
            if got != Some(exp) {
                st.field_mismatch += 1;
                st.diff(format!(
                    "line {}: s2c 帧尾与 hop/tail 期望不符 (seq {:?}): got {:?} want {exp:?}",
                    lineno + 1,
                    rec.seq,
                    got
                ));
            }
        }
    }
}

fn process_frame(rec: &Record, lineno: usize, sabotage: Sabotage, st: &mut Stats) -> Result<()> {
    if rec.string_frame.unwrap_or(false) {
        return process_string_frame(rec, lineno, sabotage, st);
    }
    st.total_frames += 1;
    let raw_orig = hex::decode(rec.raw_hex.as_deref().unwrap_or(""))?;
    let mut raw = raw_orig.clone();
    if sabotage == Sabotage::Truncate && raw.len() > 3 {
        // 红检③：body 截 1 字节（截掉 '!' 前一字节）
        raw.remove(raw.len() - 2);
    }
    let dir = rec.dir.as_deref().unwrap_or("c2s");
    // s2c：帧尾形态必须原样复现（`!` 与 `!$` 在现网并存：LoginSrv 构造的帧带 `$`，
    // 网关自身产生的帧不带）。帧尾只看末字节，与解码密钥无关，故红检模式下同样可取。
    let s2c_tail = if dir == "s2c" {
        frame::decode_server_frame_ex(&raw)
            .ok()
            .map(|(_, tail)| tail)
    } else {
        None
    };
    check_s2c_tail_expectation(rec, lineno, dir, &raw, st);
    let Some(mut msg) = parse_frame(&raw, dir, sabotage) else {
        st.byte_mismatch += 1;
        st.diff(format!(
            "line {}: frame parse failed (seq {:?})（若载荷是裸字符串而非 12 字节头，请在记录里加 \"string_frame\":true）",
            lineno + 1,
            rec.seq
        ));
        return Ok(());
    };
    if sabotage == Sabotage::Ident {
        // 红检②：包号 +1，字段对拍必须红
        msg.head.ident = msg.head.ident.wrapping_add(1);
    }
    st.seen_idents.insert(msg.head.ident);
    if messages::names_of(msg.head.ident).is_empty() {
        st.unknown_idents.insert(msg.head.ident);
    }
    // ① 逐字节回放：重编码后与**原始记录**比对
    let reenc = match dir {
        "s2c" => frame::encode_server_frame_tail(
            &msg.head,
            &edcode::encode(&msg.body),
            s2c_tail.unwrap_or(frame::ServerFrameTail::Bang),
        ),
        _ => frame::encode_client_frame(&msg.head, &msg.body),
    };
    if reenc != raw_orig {
        st.byte_mismatch += 1;
        st.diff(format!(
            "line {}: byte round-trip mismatch (seq {:?}, ident {})",
            lineno + 1,
            rec.seq,
            msg.head.ident
        ));
    }
    // ② 字段对拍
    if let (Some(ident), Some(recog), Some(param), Some(tag), Some(series)) =
        (rec.ident, rec.recog, rec.param, rec.tag, rec.series)
    {
        let h = &msg.head;
        if h.ident != ident
            || h.recog != recog
            || h.param != param
            || h.tag != tag
            || h.series != series
        {
            st.field_mismatch += 1;
            st.diff(format!(
                "line {}: field mismatch (seq {:?}): got (ident={},recog={},param={},tag={},series={}) want ({ident},{recog},{param},{tag},{series})",
                lineno + 1, rec.seq, h.ident, h.recog, h.param, h.tag, h.series
            ));
        }
    }
    if let Some(want_len) = rec.body_len {
        if msg.body.len() != want_len {
            st.field_mismatch += 1;
            st.diff(format!(
                "line {}: body_len mismatch (seq {:?}): got {} want {want_len}",
                lineno + 1,
                rec.seq,
                msg.body.len()
            ));
        }
    }
    if let Some(want_hash) = &rec.body_sha256 {
        if sha256_hex(&msg.body) != *want_hash {
            st.field_mismatch += 1;
            st.diff(format!(
                "line {}: body sha256 mismatch (seq {:?})",
                lineno + 1,
                rec.seq
            ));
        }
    }
    Ok(())
}

fn report(path: &str, coverage: bool, sabotage: Sabotage, st: &Stats) -> bool {
    println!("== replay report: {path} ==");
    println!(
        "frames: {} (其中纯字符串帧 {}), edcode vectors: {}",
        st.total_frames, st.string_frames, st.total_edcode
    );
    println!("byte mismatches (①): {}", st.byte_mismatch);
    println!("field mismatches (②): {}", st.field_mismatch);
    println!("distinct idents: {}", st.seen_idents.len());
    println!("unknown idents (未实现): {}", st.unknown_idents.len());
    for id in &st.unknown_idents {
        println!("  unknown ident: {id}");
    }
    if !st.first_diffs.is_empty() {
        println!("-- first diffs --");
        for d in &st.first_diffs {
            println!("{d}");
        }
    }

    let mut stage_missing: Vec<&str> = Vec::new();
    if coverage {
        println!("stage coverage (③):");
        let mut stage_hits: BTreeMap<&str, usize> = BTreeMap::new();
        for (stage, idents) in STAGES {
            let hits = idents.iter().filter(|i| st.seen_idents.contains(i)).count();
            stage_hits.insert(stage, hits);
            if hits == 0 {
                stage_missing.push(stage);
            }
        }
        for (stage, hits) in &stage_hits {
            println!("  {stage}: {hits} idents hit");
        }
    }

    let green = st.byte_mismatch == 0
        && st.field_mismatch == 0
        && st.unknown_idents.is_empty()
        && (!coverage || stage_missing.is_empty());
    match (sabotage, green) {
        (Sabotage::None, true) => println!("RESULT: GREEN"),
        (Sabotage::None, false) => println!("RESULT: RED"),
        (mode, false) => println!("RESULT: RED (sabotage {mode:?} detected as required)"),
        (mode, true) => println!("RESULT: GREEN UNDER SABOTAGE {mode:?} — GATE BROKEN"),
    }
    green
}

fn main() -> Result<()> {
    let (path, coverage, sabotage) = parse_args();
    let text = std::fs::read_to_string(&path).with_context(|| format!("read {path}"))?;
    let mut st = Stats::default();

    for (lineno, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let rec: Record =
            serde_json::from_str(line).with_context(|| format!("line {}: bad json", lineno + 1))?;
        match rec.kind.as_str() {
            "edcode" => process_edcode(&rec, lineno, &mut st)?,
            "frame" => process_frame(&rec, lineno, sabotage, &mut st)?,
            "server-header" => process_server_header(&rec, lineno, &mut st)?,
            other => bail!("line {}: unknown kind {other}", lineno + 1),
        }
    }

    let green = report(&path, coverage, sabotage, &st);
    match (sabotage, green) {
        (Sabotage::None, true) => Ok(()),
        (_, true) => std::process::exit(3), // 红检未检出：门禁失效
        (_, false) => std::process::exit(1), // 红（含红检被检出的预期红）
    }
}
