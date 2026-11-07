//! mir2-planes — 跨服（Planes）线协议编解码（G11）。
//!
//! 与 C# `PlanesServer.DecodeSocStr` / `PlanesClient.DecodeSocStr` 的
//! **帧提取 / 字段切分 / 数值字段**逐字节等价：帧是 `(ident/serverNum/body)`，
//! 字段分隔符是 `'/'`（C# 的 `HUtil32.Backslash`，注意它的名字叫 Backslash、值却是 `/`）。
//!
//! 范围分三层，别混着读：
//!
//! 1. **本文件（`lib.rs`）**：帧提取 / 字段切分 / 数值字段——「同样的字节进 ⇒ 同样的
//!    `(ident, serverNum, body)` 出」；
//! 2. **[`message`]**：`ProcessData` 的 **分类**（41 个 case → `Effect`）与
//!    **唯一可观察失败**（211 返回 [`message::ProcessError`]），以及 3 条真实执行的
//!    `GetValidStr3` 解析；
//! 3. **仍不在本 crate**（归 A3 传输层）：211 导致**整批丢弃**的执行、客户端 `catch`
//!    里的 `LogService.Error`（服务器侧静默吞）、非 ASCII body 的 GB2312 解码。
//!
//! **本层只处理字节**：线上文本是 GB2312 字节流，这里不做任何 UTF-8 转换。
//!
//! 所有"怪异"行为都是**对着 C# 源实跑出来的**（探针复制 `HUtil32` 原函数 + 模拟两路
//! `DecodeSocStr`），不是读代码推断的：
//! - `GetValidStr3` 会**跳过空段**（`a//b` ⇒ dest `a`、返回 `b`），不是简单的 split-once；
//! - `ArrestStringEx` 在「找到 `(` 但找不到 `)`」时返回 `( + 剩余`，且**不改写**上一个
//!   `arrest_str` ⇒ `Str` 跨迭代复用会造成**上一帧被重复处理**；
//! - 空帧 `()` 会消耗掉一帧并把缓冲推进到其后；
//! - 完全找不到 `(` 时 C# 越界抛异常被 catch 吞掉 ⇒ 缓冲被**清空**；
//! - 服务器循环条件是 `IndexOf(')') > 0`，客户端是 `> -1`，两者**可观察地不同**。
#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod client;
pub mod message;
pub mod server;
pub mod service;

/// 字段分隔符。C# `HUtil32.Backslash` 的真实值就是 `/`（`src/OpenMir2/HUtil32.cs`）。
pub const DIVIDER: u8 = b'/';

/// 一帧跨服消息。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// 消息号（C# `ident`），解析失败时为 `0`（`StrToInt(head, 0)`）。
    pub ident: i32,
    /// 源服务器序号（C# `serverNum`），解析失败时为 `-1`（`StrToInt(sNumStr, -1)`）。
    pub server_num: i32,
    /// 原始字节，未做任何编码转换。
    pub body: Vec<u8>,
}

/// 解码侧的行为差异：服务器与客户端在循环条件上不同。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flavor {
    /// `PlanesServer.DecodeSocStr`：`while (buf.IndexOf(')') > 0)`。
    Server,
    /// `PlanesClient.DecodeSocStr`：`while (buf.IndexOf(')') > -1)`。
    Client,
}

/// `int.TryParse` 的等价物：失败（含非数字 / 溢出 / 非 UTF-8）返回 `def`。
///
/// **只 trim ASCII 空白**：空格与 `\t`..`\r`（**含 VT 0x0B**，对齐 .NET 的 `IsWhite`）——
/// `str::trim_ascii()` 的集合不含 VT，所以这里用显式集合，不能图省事。
/// .NET 的 `TryParse` 不接受 Unicode 空白
/// （如 U+3000、U+00A0），实跑 `StrToInt("42\u{3000}", 7)` 得 `7`。
#[must_use]
pub fn str_to_int(bytes: &[u8], def: i32) -> i32 {
    match std::str::from_utf8(bytes) {
        Ok(s) => s
            .trim_matches(|c: char| c == ' ' || ('\t'..='\r').contains(&c))
            .parse::<i32>()
            .unwrap_or(def),
        Err(_) => def,
    }
}

/// C# `HUtil32.GetValidStr3(source, ref dest, divider)` 的逐行移植（实跑对拍）。
///
/// 语义是 `Split(divider, 2, RemoveEmptyEntries)`：**跳过空段**，`dest` 取第一个非空段，
/// 返回「该段之后、跳过紧随的分隔符」的剩余部分。
///
/// 实跑真值表（oracle）：`a//b` ⇒ (`a`, `b`)；`//body` ⇒ (`body`, ``)；
/// `/a/b` ⇒ (`a`, `b`)；`a//b/c` ⇒ (`a`, `b/c`)；`a/` ⇒ (`a`, ``)。
pub fn get_valid_str3(source: &[u8], dest: &mut Vec<u8>) -> Vec<u8> {
    dest.clear();
    let mut i = 0;
    while i < source.len() && source[i] == DIVIDER {
        i += 1;
    }
    if i >= source.len() {
        return Vec::new();
    }
    let start = i;
    while i < source.len() && source[i] != DIVIDER {
        i += 1;
    }
    dest.extend_from_slice(&source[start..i]);
    while i < source.len() && source[i] == DIVIDER {
        i += 1;
    }
    source[i..].to_vec()
}

/// C# `HUtil32.ArrestStringEx(source, searchAfter, arrestBefore, ref arrestStr)` 的移植。
///
/// 返回「闭合符号之后的剩余部分」，`arrest_str` 被置为两符号之间的内容。三条边界都按
/// oracle 实跑保留：
/// - source 为空 ⇒ 返回空，**不动** `arrest_str`；
/// - 找不到 `(` ⇒ C# 在兜底分支越界抛异常被 catch ⇒ **清空** `arrest_str` 并返回空；
/// - 找到 `(` 但找不到 `)` ⇒ 返回 `search + 剩余`，**不动** `arrest_str`
///   （跨迭代复用 `arrest_str` 时会因此重复处理上一帧）。
pub fn arrest_string_ex(
    source: &[u8],
    search: u8,
    arrest: u8,
    arrest_str: &mut Vec<u8>,
) -> Vec<u8> {
    if source.is_empty() {
        return Vec::new();
    }
    let mut span: &[u8] = source;
    let mut find_data = false;
    if source.len() >= 2 {
        if source[0] == search {
            span = &source[1..];
            find_data = true;
        } else if let Some(n) = source.iter().position(|&b| b == search) {
            if n > 0 {
                span = &source[n + 1..];
                find_data = true;
            }
        }
    }
    if !find_data {
        arrest_str.clear();
        return Vec::new();
    }
    if let Some(n) = span.iter().position(|&b| b == arrest) {
        arrest_str.clear();
        arrest_str.extend_from_slice(&span[..n]);
        span[n + 1..].to_vec()
    } else {
        let mut out = Vec::with_capacity(span.len() + 1);
        out.push(search);
        out.extend_from_slice(span);
        out
    }
}

/// 组装一帧（发送侧）。C# 是 `nCode + "/" + nServerIdx + "/" + sMsg`，外层补括号。
#[must_use]
pub fn encode_frame(ident: i32, server_num: i32, body: &[u8]) -> Vec<u8> {
    let head = format!("({ident}{}{server_num}{}", DIVIDER as char, DIVIDER as char);
    let mut out = Vec::with_capacity(head.len() + body.len() + 1);
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(body);
    out.push(b')');
    out
}

/// 流式解码器：`push` 累积字节，`decode` 跑**一次** C# 语义的解码过程。
///
/// 与 C# 一致：预检（`IndexOf(')') <= 0` ⇒ 什么都不做）**每次 decode 只做一次**；
/// 循环内用 [`Flavor`] 决定条件；`arrest_str` 在**同一次 decode 内跨迭代复用**。
#[derive(Debug)]
pub struct Decoder {
    buf: Vec<u8>,
    flavor: Flavor,
}

impl Decoder {
    /// 新建解码器。
    #[must_use]
    pub fn new(flavor: Flavor) -> Self {
        Self {
            buf: Vec::new(),
            flavor,
        }
    }

    /// 累积一段收到的字节（可含 0 到多帧）。**不解析**。
    pub fn push(&mut self, chunk: &[u8]) {
        self.buf.extend_from_slice(chunk);
    }

    /// 当前缓冲区（测试与排障用）。
    #[must_use]
    pub fn buffered(&self) -> &[u8] {
        &self.buf
    }

    /// 丢弃缓冲（oracle 的 catch 分支等价物：异常后 `SocData` 停在 `""`）。
    pub fn clear(&mut self) {
        self.buf.clear();
    }

    /// 跑一次解码：返回本次解出的帧，剩余字节留在缓冲里。
    pub fn decode(&mut self) -> Vec<Frame> {
        self.decode_raw().into_iter().map(|(_, f)| f).collect()
    }

    /// 同 [`Self::decode`]，但同时返回每帧的**原始内层字节**（两个括号之间，不含括号）。
    ///
    /// 跨服主服的中继（C# `DecodeSocStr_SendOtherServer`）转发的就是这段内容——
    /// 它把 `Str` 原样交给 `SendSocket`，后者再补回一对括号发出。
    pub fn decode_raw(&mut self) -> Vec<(Vec<u8>, Frame)> {
        let mut frames = Vec::new();
        if self.buf.is_empty() {
            return frames;
        }
        // C# 预检：`IndexOf(')') <= 0` ⇒ 直接 return（位置 0 也不处理），缓冲不动。
        match self.buf.iter().position(|&b| b == b')') {
            Some(i) if i > 0 => {}
            _ => return frames,
        }
        let mut buf_str = std::mem::take(&mut self.buf);
        let mut arrest_str: Vec<u8> = Vec::new();
        loop {
            let cond = match self.flavor {
                Flavor::Server => buf_str
                    .iter()
                    .position(|&b| b == b')')
                    .is_some_and(|i| i > 0),
                Flavor::Client => buf_str.contains(&b')'),
            };
            if !cond {
                break;
            }
            buf_str = arrest_string_ex(&buf_str, b'(', b')', &mut arrest_str);
            if arrest_str.is_empty() {
                break;
            }
            frames.push((arrest_str.clone(), parse_frame(&arrest_str)));
        }
        self.buf = buf_str;
        frames
    }
}

/// 把一帧的内层字节（`ident/serverNum/body`）解析成 [`Frame`]。
fn parse_frame(inner: &[u8]) -> Frame {
    let mut head = Vec::new();
    let remainder = get_valid_str3(inner, &mut head);
    let mut s_num = Vec::new();
    let body = get_valid_str3(&remainder, &mut s_num);
    Frame {
        ident: str_to_int(&head, 0),
        server_num: str_to_int(&s_num, -1),
        body,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn run(flavor: Flavor, input: &[u8]) -> (Vec<Frame>, Vec<u8>) {
        let mut d = Decoder::new(flavor);
        d.push(input);
        let frames = d.decode();
        (frames, d.buffered().to_vec())
    }

    fn fr(ident: i32, server_num: i32, body: &[u8]) -> Frame {
        Frame {
            ident,
            server_num,
            body: body.to_vec(),
        }
    }

    // ---- 以下期望值全部来自 C# 探针实跑（复制 HUtil32 原函数 + 模拟两路 DecodeSocStr）----

    #[test]
    fn encode_uses_slash_divider() {
        assert_eq!(encode_frame(201, 3, b"alice"), b"(201/3/alice)");
    }

    /// oracle 帧表的行类型：(输入, server 帧, server 剩余, client 帧, client 剩余)
    type Case = (
        &'static [u8],
        Vec<Frame>,
        &'static [u8],
        Vec<Frame>,
        &'static [u8],
    );

    /// 数据表本身比 100 行长 —— 这是刻意的：每条都是 C# 探针实跑出来的期望值。
    #[test]
    #[allow(clippy::too_many_lines)]
    fn oracle_frame_table() {
        let cases: &[Case] = &[
            (
                b"(201/9/alice)",
                vec![fr(201, 9, b"alice")],
                b"",
                vec![fr(201, 9, b"alice")],
                b"",
            ),
            (
                b"(201/9/a)(202/1/b)",
                vec![fr(201, 9, b"a"), fr(202, 1, b"b")],
                b"",
                vec![fr(201, 9, b"a"), fr(202, 1, b"b")],
                b"",
            ),
            (
                b"(201//body)",
                vec![fr(201, -1, b"")],
                b"",
                vec![fr(201, -1, b"")],
                b"",
            ),
            (
                b"(1//a/b)",
                vec![fr(1, -1, b"b")],
                b"",
                vec![fr(1, -1, b"b")],
                b"",
            ),
            (
                b"(1/2//b)",
                vec![fr(1, 2, b"b")],
                b"",
                vec![fr(1, 2, b"b")],
                b"",
            ),
            (
                b"(/2/body)",
                vec![fr(2, -1, b"")],
                b"",
                vec![fr(2, -1, b"")],
                b"",
            ),
            (b"()(1/2/x)", vec![], b"(1/2/x)", vec![], b"(1/2/x)"),
            (b"201/1/ok)", vec![], b"", vec![], b""),
            (
                b"(1/2/a)b)c(d",
                vec![fr(1, 2, b"a"), fr(1, 2, b"a")],
                b"(d",
                vec![fr(1, 2, b"a"), fr(1, 2, b"a")],
                b"(d",
            ),
            (
                b"(1/2/a))(3/4/y)",
                vec![fr(1, 2, b"a")],
                b")(3/4/y)",
                vec![fr(1, 2, b"a"), fr(3, 4, b"y")],
                b"",
            ),
            (
                b"(1/2/a))",
                vec![fr(1, 2, b"a")],
                b")",
                vec![fr(1, 2, b"a")],
                b"",
            ),
            (
                b"junk(201/1/ok)",
                vec![fr(201, 1, b"ok")],
                b"",
                vec![fr(201, 1, b"ok")],
                b"",
            ),
            (b"(201/9/x", vec![], b"(201/9/x", vec![], b"(201/9/x"),
            (b")rest", vec![], b")rest", vec![], b")rest"),
            (
                b"(abc/1/x)",
                vec![fr(0, 1, b"x")],
                b"",
                vec![fr(0, 1, b"x")],
                b"",
            ),
            (
                b"((1/2/x)",
                vec![fr(0, 2, b"x")],
                b"",
                vec![fr(0, 2, b"x")],
                b"",
            ),
            (
                b"(1/2/a/b/c)",
                vec![fr(1, 2, b"a/b/c")],
                b"",
                vec![fr(1, 2, b"a/b/c")],
                b"",
            ),
            (
                b"  (1/2/x)",
                vec![fr(1, 2, b"x")],
                b"",
                vec![fr(1, 2, b"x")],
                b"",
            ),
            (b"", vec![], b"", vec![], b""),
        ];
        for (input, sf, sr, cf, cr) in cases {
            let (got_f, got_r) = run(Flavor::Server, input);
            assert_eq!(
                &got_f,
                sf,
                "server frames for {:?}",
                String::from_utf8_lossy(input)
            );
            assert_eq!(
                &got_r,
                sr,
                "server rest for {:?}",
                String::from_utf8_lossy(input)
            );
            let (got_f, got_r) = run(Flavor::Client, input);
            assert_eq!(
                &got_f,
                cf,
                "client frames for {:?}",
                String::from_utf8_lossy(input)
            );
            assert_eq!(
                &got_r,
                cr,
                "client rest for {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn partial_frame_is_buffered_until_close() {
        let mut d = Decoder::new(Flavor::Server);
        d.push(b"(20");
        assert_eq!(d.decode(), Vec::new());
        assert_eq!(d.buffered(), b"(20");
        d.push(b"1/9/al");
        assert_eq!(d.decode(), Vec::new());
        d.push(b"ice)");
        let f = d.decode();
        assert_eq!(f, vec![fr(201, 9, b"alice")]);
        assert_eq!(d.buffered(), b"");
    }

    #[test]
    fn get_valid_str3_oracle_table() {
        let cases: &[(&[u8], &[u8], &[u8])] = &[
            (b"a//b", b"a", b"b"),
            (b"//body", b"body", b""),
            (b"/a/b", b"a", b"b"),
            (b"a//b/c", b"a", b"b/c"),
            (b"abc", b"abc", b""),
            (b"", b"", b""),
            (b"/", b"", b""),
            (b"a/", b"a", b""),
        ];
        for (input, want_dest, want_ret) in cases {
            let mut dest = Vec::new();
            let ret = get_valid_str3(input, &mut dest);
            assert_eq!(
                &dest,
                want_dest,
                "dest for {:?}",
                String::from_utf8_lossy(input)
            );
            assert_eq!(
                &ret,
                want_ret,
                "ret for {:?}",
                String::from_utf8_lossy(input)
            );
        }
    }

    #[test]
    fn str_to_int_oracle_table() {
        assert_eq!(str_to_int(b" 42 ", 7), 42);
        assert_eq!(str_to_int(b"4x", 7), 7);
        assert_eq!(str_to_int(b"", 7), 7);
        assert_eq!(str_to_int(b"-5", 7), -5);
        assert_eq!(str_to_int(b" 42\t", 7), 42);
        // VT(0x0B)：.NET 的 IsWhite 覆盖 \t..\r ⇒ 42；Rust 的 trim_ascii 不含 VT，必须显式覆盖
        assert_eq!(str_to_int(b"42\x0b", 7), 42);
        assert_eq!(str_to_int(b"\x0b42", 7), 42);
        // Unicode 空白：C# TryParse 不认 ⇒ 默认值；我们的字节层同样失败
        assert_eq!(str_to_int("\u{3000}42".as_bytes(), 7), 7);
    }
}
