//! .NET `Encoding.GetEncoding(936)`（cp936 / 代码页 936）的**唯一**编解码入口。
//!
//! 供 `crates/script` 与 `crates/envir` 共用（B-105 / E-1 / E-2）。oracle 是
//! `src/OpenMir2/HUtil32.cs:183-205,689-692` 的 `Encoding.GetEncoding("gb2312")`，
//! 实测编码器类型为 `InternalEncoderBestFitFallback`（.NET 9 + CodePages 8.0.0）。
//!
//! # 语义（逐条对应 .NET 实测）
//!
//! - **编码**：逐 UTF-16 码元查 cp936 表。命中 → 1/2 字节；未命中（含代理码元、
//!   99 个 WHATWG-only、38,907 个双不可映射）→ **单个 `0x3F`**。绝不输出
//!   `&#NNNN;` 数字字符引用，绝不 panic。非 BMP 字符按 UTF-16 码元拆成两个
//!   `0x3F`（`U+1F600` → `3F 3F`），与 .NET 一致。
//! - **解码**：`0x00..=0x7F` 原样；`0x80` → `U+20AC`；`0xFF` → `U+F8F5`；
//!   `0x81..=0xFE` 后**只要有后继字节就成对消耗**（后继任意，含 `0x00/0x0A/0x20/`
//!   `0x2F/0x7F/0xFF`），表内命中用覆盖值，否则用 WHATWG 逐对解码并把 `U+FFFD`
//!   归一成 `?`；末位孤立引导字节 → `?`。覆盖表见 [`crate::gbk_overrides`]（7032 条，
//!   B-105）。
//! - **StringList/BOM**：[`decode_string_list`] 按 `StreamReader` 实测顺序判定
//!   UTF-8 / **UTF-32LE（必须先于 UTF-16LE）** / UTF-16LE / UTF-16BE / UTF-32BE，
//!   其余走 cp936。UTF 分支非法序列给 `U+FFFD`；只有 cp936 分支给 `?`。
//!
//! # 编码表
//!
//! `cp936_encode.bin`：65,536 个 `u16`（大端），按 BMP 码点直接索引；
//! `0xFFFF` = 不可映射（含代理区）。表由 .NET 探针
//! `Encoding.GetEncoding(936).GetBytes(new[]{ (char)cp })` 对 0x0000..=0xFFFF
//! （跳过代理）逐点导出后机械生成，**不手抄**：
//!
//! ```text
//! dotnet run -c Release -- <bmp-out> <extra-out>   # E:/tmp/enca-v2/encprobe-dotnet
//! python3 gen.py                                    # 本仓离线生成脚本
//! sha256(cp936_encode.bin) = a1ad4d6d44f62cd90cfa445ee759ba05a5226f3187af873704ef0498d16af867
//! ```
//!
//! 一张 char → 字节 的值若 `<= 0xFF` 表示单字节结果（cp936 的双字节前导恒
//! `>= 0x81`，不会与单字节混淆），否则高/低字节各一。全表判据见
//! `tests/cp936_encode.rs::full_bmp_table_matches_dotnet`。

/// 全 BMP 编码表（65,536 × `u16` 大端，`0xFFFF` = 不可映射）。
static ENCODE_BMP_BE: &[u8; 65_536 * 2] = include_bytes!("cp936_encode.bin");

/// 查一个 UTF-16 码元的 cp936 编码；`None` = 不可映射（oracle 回退 `0x3F`）。
fn encode_unit(unit: u16) -> Option<u16> {
    let off = usize::from(unit) * 2;
    let v = u16::from_be_bytes([ENCODE_BMP_BE[off], ENCODE_BMP_BE[off + 1]]);
    if v == 0xFFFF {
        None
    } else {
        Some(v)
    }
}

/// .NET cp936 编码入口（全 workspace 唯一的 cp936 编码实现）。
///
/// 逐 UTF-16 码元查表：不可映射 → `0x3F`。绝不产生 `&#N;`，绝不 panic。
#[must_use]
pub fn encode_gb2312_dotnet(text: &str) -> Vec<u8> {
    // 每码元最多 2 字节；非 BMP 字符的每个代理码元各回退 1 个 `?`。
    let mut out = Vec::with_capacity(text.len());
    for unit in text.encode_utf16() {
        match encode_unit(unit) {
            Some(v) if v <= 0xFF => out.push(v as u8),
            Some(v) => {
                out.push((v >> 8) as u8);
                out.push((v & 0xFF) as u8);
            }
            None => out.push(0x3F),
        }
    }
    out
}

/// .NET cp936 解码（`HUtil32.GetString`）。
///
/// 字节语义见模块文档；`0x81..=0xFE` 后有成对后继就消耗 2 字节（不论后继值），
/// 这在字符数 / `Split` 边界上与 .NET 一致。
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b < 0x80 {
            out.push(b as char);
            i += 1;
            continue;
        }
        if b == 0x80 {
            out.push('\u{20AC}');
            i += 1;
            continue;
        }
        if b == 0xFF {
            out.push('\u{F8F5}');
            i += 1;
            continue;
        }
        if i + 1 >= bytes.len() {
            out.push('?');
            i += 1;
            continue;
        }
        let key = (u16::from(b) << 8) | u16::from(bytes[i + 1]);
        if let Some((_, c)) = crate::gbk_overrides::GBK_OVERRIDES
            .iter()
            .find(|(k, _)| *k == key)
        {
            out.push(*c);
            i += 2;
            continue;
        }
        let (s, _, _) = encoding_rs::GBK.decode(&bytes[i..i + 2]);
        for c in s.chars() {
            out.push(if c == '\u{FFFD}' { '?' } else { c });
        }
        i += 2;
    }
    out
}

/// `StringList.LoadFromFile` 的解码模型（`Common/StringList.cs:276-291` + `StreamReader`
/// 的 BOM 探测优先级）。
///
/// 判定顺序（.NET 实测，audit-encoding-v2 §2.2）：
/// 1. `EF BB BF` → UTF-8；
/// 2. `FF FE 00 00` → **UTF-32LE**（必须排在 `FF FE` 前）；
/// 3. `FF FE` → UTF-16LE；
/// 4. `FE FF` → UTF-16BE；
/// 5. `00 00 FE FF` → UTF-32BE；
/// 6. 其余 → cp936（[`decode`]）。
#[must_use]
pub fn decode_string_list(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _, _) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return decode_utf32_le(&bytes[4..]);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return decode_utf32_be(&bytes[4..]);
    }
    decode(bytes)
}

/// UTF-32LE 解码（.NET `Encoding.UTF32` little-endian 分支）：4 字节一组；
/// 非法码点/代理 → `U+FFFD`；尾部不足 4 字节 → 单个 `U+FFFD`。
fn decode_utf32_le(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (groups, rest) = bytes.as_chunks::<4>();
    for chunk in groups {
        let cp = u32::from_le_bytes(*chunk);
        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
    }
    if !rest.is_empty() {
        out.push('\u{FFFD}');
    }
    out
}

/// UTF-32BE 解码（.NET `Encoding.UTF32` big-endian 分支）：4 字节一组；
/// 非法码点/代理 → `U+FFFD`；尾部不足 4 字节 → 单个 `U+FFFD`。
fn decode_utf32_be(bytes: &[u8]) -> String {
    let mut out = String::new();
    let (groups, rest) = bytes.as_chunks::<4>();
    for chunk in groups {
        let cp = u32::from_be_bytes(*chunk);
        out.push(char::from_u32(cp).unwrap_or('\u{FFFD}'));
    }
    if !rest.is_empty() {
        out.push('\u{FFFD}');
    }
    out
}
