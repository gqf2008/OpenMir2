//! mir2-planes — 跨服（Planes）线协议编解码（G11）。
//!
//! 与 C# `PlanesServer.DecodeSocStr` / `PlanesClient.DecodeSocStr` **逐字节等价**：
//! 帧是 `(ident/serverNum/body)`，字段分隔符是 `'/'`（C# 的 `HUtil32.Backslash`，
//! 注意它的名字叫 Backslash、值却是 `/`）。`body` 内部还可以再用 `/` 分字段，
//! 由上层用 [`get_valid_str3`] 逐段取。
//!
//! **本层只处理字节**：线上文本是 GB2312 字节流，这里不做任何 UTF-8 转换或补全。
//!
//! 与 C# 的两处**刻意保留**的畸形行为：
//! - 服务器端循环条件 `IndexOf(')') > 0`，客户端是 `> -1`（见 [`Decoder::next_frame`]）；
//! - 找不到 `(` 时 C# 会在兜底循环里越界抛异常、被 catch 吞成空串（本实现同样返回空）。
#![forbid(unsafe_code)]
#![warn(missing_docs)]

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

/// `int.TryParse` 的等价物：失败（含非 UTF-8 / 非数字 / 溢出）返回 `def`。
#[must_use]
pub fn str_to_int(bytes: &[u8], def: i32) -> i32 {
    match std::str::from_utf8(bytes) {
        Ok(s) => s.trim().parse::<i32>().unwrap_or(def),
        Err(_) => def,
    }
}

/// C# `HUtil32.GetValidStr3(source, ref dest, divider)` 的逐行移植。
///
/// 语义：按分隔符**最多切成 2 段**并 `RemoveEmptyEntries`，`dest` 取第一段（可能为空），
/// 返回第二段（可能为空）。前导空字段会被折叠——这与「连续分隔符 = 空字段」的直觉不同。
pub fn get_valid_str3(source: &[u8], dest: &mut Vec<u8>) -> Vec<u8> {
    let mut it = source.splitn(2, |&b| b == DIVIDER);
    let first = it.next().unwrap_or(&[]);
    let second = it.next().unwrap_or(&[]);
    let mut parts: [&[u8]; 2] = [first, second];
    let mut n = 0;
    for part in &mut parts {
        if !part.is_empty() {
            n += 1;
        }
    }
    dest.clear();
    match n {
        0 => Vec::new(),
        1 => {
            let only = if first.is_empty() { second } else { first };
            dest.extend_from_slice(only);
            Vec::new()
        }
        _ => {
            dest.extend_from_slice(first);
            second.to_vec()
        }
    }
}

/// C# `HUtil32.ArrestStringEx(source, searchAfter, arrestBefore, ref arrestStr)` 的移植。
///
/// 返回「闭合符号之后的剩余部分」；`arrest_str` 被置为两个符号之间的内容。
/// 找不到闭合符号时返回 `searchAfter + 剩余`（与 C# 一致）；**找不到开启符号时返回空**
/// （C# 在兜底分支里越界抛异常后被 catch 吞成空串）。
pub fn arrest_string_ex(
    source: &[u8],
    search: u8,
    arrest: u8,
    arrest_str: &mut Vec<u8>,
) -> Vec<u8> {
    arrest_str.clear();
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
        return Vec::new();
    }
    if let Some(n) = span.iter().position(|&b| b == arrest) {
        arrest_str.extend_from_slice(&span[..n]);
        span[n + 1..].to_vec()
    } else {
        let mut out = Vec::with_capacity(span.len() + 1);
        out.push(search);
        out.extend_from_slice(span);
        out
    }
}

/// 组装一帧（发送侧）。C# 侧是 `nCode + "/" + nServerIdx + "/" + sMsg`，外层补括号。
#[must_use]
pub fn encode_frame(ident: i32, server_num: i32, body: &[u8]) -> Vec<u8> {
    let head = format!("({ident}{}{server_num}{}", DIVIDER as char, DIVIDER as char);
    let mut out = Vec::with_capacity(head.len() + body.len() + 1);
    out.extend_from_slice(head.as_bytes());
    out.extend_from_slice(body);
    out.push(b')');
    out
}

/// 流式解码器：`push` 累积字节，`next_frame` 取出一帧。
///
/// 复刻 C# 的「接收只累积、解码另外驱动」分离：`push` 不做任何解析。
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

    /// 取下一帧；不足一帧时返回 `None` 并保留缓冲。
    pub fn next_frame(&mut self) -> Option<Frame> {
        // C# 预检：`IndexOf(')') <= 0` ⇒ 直接 return（位置 0 也不处理）。
        let first_close = self.buf.iter().position(|&b| b == b')')?;
        if first_close == 0 {
            return None;
        }
        if !self.loop_condition() {
            return None;
        }
        let mut inner = Vec::new();
        let rest = arrest_string_ex(&self.buf, b'(', b')', &mut inner);
        if inner.is_empty() {
            return None;
        }
        self.buf = rest;
        let mut head = Vec::new();
        let remainder = get_valid_str3(&inner, &mut head);
        let mut s_num = Vec::new();
        let body = get_valid_str3(&remainder, &mut s_num);
        Some(Frame {
            ident: str_to_int(&head, 0),
            server_num: str_to_int(&s_num, -1),
            body,
        })
    }

    /// 服务器 `> 0`；客户端 `> -1`（即「存在 `)` 即可」）。
    fn loop_condition(&self) -> bool {
        match self.flavor {
            Flavor::Server => self
                .buf
                .iter()
                .position(|&b| b == b')')
                .is_some_and(|i| i > 0),
            Flavor::Client => self.buf.contains(&b')'),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decode_all(flavor: Flavor, chunks: &[&[u8]]) -> Vec<Frame> {
        let mut d = Decoder::new(flavor);
        let mut out = Vec::new();
        for c in chunks {
            d.push(c);
            while let Some(f) = d.next_frame() {
                out.push(f);
            }
        }
        out
    }

    #[test]
    fn encode_uses_slash_divider() {
        assert_eq!(encode_frame(201, 3, b"alice"), b"(201/3/alice)");
    }

    #[test]
    fn roundtrip_single_frame() {
        let f = decode_all(Flavor::Server, &[&encode_frame(205, 2, b"guild")]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].ident, 205);
        assert_eq!(f[0].server_num, 2);
        assert_eq!(f[0].body, b"guild");
    }

    #[test]
    fn multiple_frames_in_one_chunk() {
        let mut buf = encode_frame(100, 0, b"");
        buf.extend_from_slice(&encode_frame(211, 1, b"x/y"));
        let f = decode_all(Flavor::Server, &[&buf]);
        assert_eq!(f.len(), 2);
        assert_eq!((f[0].ident, f[0].server_num), (100, 0));
        assert_eq!((f[1].ident, f[1].server_num), (211, 1));
        assert_eq!(f[1].body, b"x/y", "body 内的分隔符原样保留");
    }

    #[test]
    fn partial_frame_is_buffered_until_close() {
        let mut d = Decoder::new(Flavor::Server);
        d.push(b"(20");
        assert!(d.next_frame().is_none());
        assert_eq!(d.buffered(), b"(20");
        d.push(b"1/9/al");
        assert!(d.next_frame().is_none());
        d.push(b"ice)");
        let f = d.next_frame().expect("frame");
        assert_eq!((f.ident, f.server_num), (201, 9));
        assert_eq!(f.body, b"alice");
        assert_eq!(d.buffered(), []);
    }

    #[test]
    fn no_close_paren_returns_none_and_keeps_buffer() {
        let mut d = Decoder::new(Flavor::Server);
        d.push(b"(201/9/x");
        assert!(d.next_frame().is_none());
        assert_eq!(d.buffered(), b"(201/9/x");
    }

    #[test]
    fn close_at_index_zero_is_ignored() {
        let mut d = Decoder::new(Flavor::Server);
        d.push(b")rest");
        assert!(d.next_frame().is_none());
        assert_eq!(d.buffered(), b")rest");
    }

    #[test]
    fn leading_junk_before_open_paren_is_skipped() {
        let f = decode_all(Flavor::Server, &[b"junk(201/1/ok)"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].body, b"ok");
    }

    #[test]
    fn missing_open_paren_yields_nothing() {
        let f = decode_all(Flavor::Server, &[b"201/1/ok)"]);
        assert!(f.is_empty(), "C# 在找不到 '(' 时抛异常被吞 → 空");
    }

    #[test]
    fn empty_fields_fold_like_remove_empty_entries() {
        // "(201//body)"：C# 的 Split(.., 2, RemoveEmptyEntries) 会把**空段折掉**，
        // 于是 "body" 被当成分隔符后的第二段 → 它成了 serverNum，body 反而为空。
        // 这是 oracle 的真实语义（不是我们想要的语义），必须原样保留。
        let f = decode_all(Flavor::Server, &[b"(201//body)"]);
        assert_eq!(f.len(), 1);
        assert_eq!(f[0].ident, 201);
        assert_eq!(
            f[0].server_num, -1,
            "serverNum 段是 \"body\" ⇒ 解析失败 ⇒ 默认 -1"
        );
        assert!(f[0].body.is_empty(), "body 段被折叠掉了");
    }

    #[test]
    fn bad_ident_falls_back_to_zero() {
        let f = decode_all(Flavor::Server, &[b"(abc/1/x)"]);
        assert_eq!(f[0].ident, 0);
    }

    #[test]
    fn str_to_int_matches_try_parse() {
        assert_eq!(str_to_int(b" 42 ", 7), 42, "TryParse 允许首尾空白");
        assert_eq!(str_to_int(b"4x", 7), 7);
        assert_eq!(str_to_int(b"", 7), 7);
        assert_eq!(str_to_int(b"-5", 7), -5);
    }

    #[test]
    fn server_and_client_differ_on_loop_condition() {
        // 预检只在 ')' 位于 0 时短路；这里构造「第一帧之后剩余以 ')' 开头」的形状，
        // 服务器（> 0）停止，客户端（> -1）继续。
        let mut d_s = Decoder::new(Flavor::Server);
        d_s.push(b"(1/2/a))");
        let first = d_s.next_frame().expect("first");
        assert_eq!(first.body, b"a");
        assert!(
            d_s.next_frame().is_none(),
            "服务器：剩余 \")\" 的 ')' 在位置 0 ⇒ 停"
        );

        let mut d_c = Decoder::new(Flavor::Client);
        d_c.push(b"(1/2/a))");
        assert!(d_c.next_frame().is_some());
        // 客户端循环条件 > -1：剩余 ")" 仍满足，但 arrest 找不到 '(' ⇒ 返回空 ⇒ 停止
        assert!(d_c.next_frame().is_none());
    }
}
