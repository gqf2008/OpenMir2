//! .NET `Encoding.GetEncoding(936)`（cp936 / 代码页 936）的**唯一**编解码入口。
//!
//! 供 `crates/script`、`crates/data`、`crates/envir` 共用（B-105 / E-1 / E-2 / E-4）。oracle 是
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
//! - **BOM 模型有两套，按 oracle 调用点区分，不可混用**（`.NET` 实测；复现命令见下）：
//!   - [`decode_string_list`]：`StringList.LoadFromFile`（`Common/StringList.cs:276-360`）
//!     先 `GetEncoding` 给出默认编码，再 `new StreamReader(fileName, fs)`。对 `FF FE xx`
//!     （xx != `FF`）`GetEncoding` 返回 `Encoding.Unicode`，`StreamReader` 因此**不会**
//!     升级成 UTF-32LE；对 `FF FE FF` 虽保持 gb2312，`StreamReader` 的 BOM 探测仍选
//!     UTF-16LE。⇒ **`FF FE 00 00` 与 `FF FE FF FE` 都是 UTF-16LE**（后者解出
//!     `U+FEFF`），本路径**没有** UTF-32LE 分支。判定顺序：UTF-8 → UTF-16LE（`FF FE`）
//!     → UTF-16BE → UTF-32BE → cp936。
//!   - [`decode_config_text`] / [`decode_stream_reader`]：`ConfigFile.Load` 的裸
//!     `new StreamReader(stream, Encoding.GetEncoding("GB2312"))`（**无** `GetEncoding`
//!     预判）——`FF FE 00 00` 按 **UTF-32LE**，其余 `FF FE` 按 UTF-16LE。
//!     `crates/envir` 的配置文件（`Server.conf` 等）走这一套。
//!
//! UTF 分支非法序列给 `U+FFFD`；只有 cp936 分支给 `?`。
//!
//! 探针复现（BOM）：把上面两种构图各跑一遍 `StreamReader.ReadToEnd()`；实测
//! `StringList` 构图 `FF FE 00 00 41 00 00 00` → `U+0000 U+0041 U+0000`（utf-16），
//! 裸 `StreamReader(gb2312)` 同字节 → `U+0041`（utf-32）；`FF FE FF FE` 两种构图均 →
//! `U+FEFF`（utf-16）。.NET 探针源码见 `tests/parity/m3/audit-encoding-v2-20261021.md`
//! §1.2（`E:/tmp/enca-v2/encprobe-dotnet`）。
//!
//! # 编码表
//!
//! `cp936_encode.bin`：65,536 个 `u16`（大端），按 BMP 码点直接索引；
//! `0xFFFF` = 不可映射（含代理区）。表由 .NET 探针
//! `Encoding.GetEncoding(936).GetBytes(new[]{ (char)cp })` 对 0x0000..=0xFFFF
//! （跳过代理）逐点导出后机械生成，**不手抄**：
//!
//! ```text
//! dotnet run -c Release -- <bmp-out> <extra-out>   # E:/tmp/enca-v2/encprobe-dotnet（审计探针）
//! python3 crates/encoding/tools/gen_cp936.py        # 本仓可复现：crates/script/tests/data/cp936-encode-bmp.txt -> src/cp936_encode.bin
//! sha256(cp936_encode.bin) = a1ad4d6d44f62cd90cfa445ee759ba05a5226f3187af873704ef0498d16af867
//! ```
//! `gen_cp936.py` 只做机械转换（代理区补 `0xFFFF` 哨兵），不改表内容；脚本以
//! `crates/script/tests/data/cp936-encode-bmp.txt`（已入库、由 .NET 探针导出）为唯一输入。
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

/// `StringList.LoadFromFile` 的解码模型（`Common/StringList.cs:276-360`）。
///
/// **先** `GetEncoding` 按前几字节选默认编码，**再** `new StreamReader(fileName, fs)`
/// 做 BOM 探测。`StreamReader` 的 BOM 优先级更高，但选出的编码是 UTF-16LE 时不会再
/// 升级到 UTF-32LE（.NET 实测）：
/// 1. `EF BB BF` → UTF-8；
/// 2. `FF FE` → UTF-16LE（含 `FF FE 00 00`：`GetEncoding` 给 `Encoding.Unicode`，
///    `StreamReader` 保持 UTF-16LE ⇒ 首字符 `U+0000`；也含 `FF FE FF`/`FF FE FF FE`：
///    `GetEncoding` 保持 gb2312，但 `StreamReader` 的 BOM 探测仍选 UTF-16LE）；
/// 3. `FE FF` → UTF-16BE；
/// 4. `00 00 FE FF` → UTF-32BE（`GetEncoding` 保持 gb2312，BOM 探测升级）；
/// 5. 其余 → cp936（[`decode`]）。
///
/// 本路径**不产生** UTF-32LE：`FF FE 00 00` 是 UTF-16LE，不是 UTF-32LE。
#[must_use]
pub fn decode_string_list(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _) = encoding_rs::UTF_8.decode_without_bom_handling(&bytes[3..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _) = encoding_rs::UTF_16BE.decode_without_bom_handling(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return decode_utf32_be(&bytes[4..]);
    }
    decode(bytes)
}

/// `ConfigFile.Load` 的解码模型：裸 `new StreamReader(stream, Encoding.GetEncoding("GB2312"))`
/// （`src/OpenMir2/Common/ConfigFile.cs:263`），没有 `StringList` 的 `GetEncoding` 预判。
///
/// `StreamReader` 的 BOM 探测（`detectEncodingFromByteOrderMarks` 默认 true）顺序：
/// 1. `EF BB BF` → UTF-8；
/// 2. `FF FE 00 00` → **UTF-32LE**（必须排在 `FF FE` 前）；
/// 3. `FF FE` → UTF-16LE；
/// 4. `FE FF` → UTF-16BE；
/// 5. `00 00 FE FF` → UTF-32BE；
/// 6. 其余 → cp936（[`decode`]）。
///
/// 与 [`decode_string_list`] 的差异只在对 `FF FE 00 00`：这里是 UTF-32LE，那里是
/// UTF-16LE。配置文件（`Server.conf`/`SabukW.txt` 等）用本函数，`StringList` 文本用前者。
#[must_use]
pub fn decode_stream_reader(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (s, _) = encoding_rs::UTF_8.decode_without_bom_handling(&bytes[3..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFF, 0xFE, 0x00, 0x00]) {
        return decode_utf32_le(&bytes[4..]);
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, _) = encoding_rs::UTF_16LE.decode_without_bom_handling(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, _) = encoding_rs::UTF_16BE.decode_without_bom_handling(&bytes[2..]);
        return s.into_owned();
    }
    if bytes.starts_with(&[0x00, 0x00, 0xFE, 0xFF]) {
        return decode_utf32_be(&bytes[4..]);
    }
    decode(bytes)
}

/// `ConfigFile.Load` 解码口径的**规范名**（与 [`decode_stream_reader`] 完全等价）。
///
/// 调用点语义见 [`decode_stream_reader`]：裸 `StreamReader(stream, GB2312)` 的 BOM
/// 探测顺序（`FF FE 00 00` → UTF-32LE）。`crates/envir` 的 `decode_config_text`
/// 与 `crates/data` 的 `IniFile::from_bytes` 都走本函数。
#[must_use]
pub fn decode_config_text(bytes: &[u8]) -> String {
    decode_stream_reader(bytes)
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
