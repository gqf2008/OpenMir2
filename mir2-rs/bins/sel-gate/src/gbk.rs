//! `HUtil32.GetString` / `HUtil32.GetBytes` 的 gb2312(cp936) 等价物。
//!
//! C# 侧一律走 `Encoding.GetEncoding("gb2312")`（= .NET cp936）：
//! - **编码**走 `mir2_script::cp936::encode_gb2312_dotnet`（与 .NET 逐字节相等，无近似）；
//! - **解码**走 `mir2_script::cp936::decode`（.NET cp936 状态机：前导字节吞任意后继、
//!   `0x80→U+20AC`、`0xFF→U+F8F5`、非法回退 `?`；B-105 的 7032 条覆盖表）。
//!   E-8 的构造帧（账号含 `0x81 0x20`）要求 `81 20` 解成**一个** `?`（不是 WHATWG 的
//!   `U+FFFD` + 空格两个字符），否则长度门与重编码后的 `PackLength` 都与 C# 不同。

/// 解码（对应 `HUtil32.GetString(bytes, 0, count)`）：.NET cp936，逐字节等价。
///
/// 现在本 crate 的转发路径是**原样字节**（`accountPacket.Data = userData.Body`），
/// 只有 `config.conf` 走 `decode_config_text` ⇒ 暂无生产调用者；但 E-8 的
/// ClientSession「decode → re-encode」路径落地时会用到，与 `encode` 同理由保留。
#[allow(dead_code)]
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    mir2_script::cp936::decode(bytes)
}

/// 编码（对应 `HUtil32.GetBytes(str)`）：**逐字节等于 .NET cp936**。
///
/// 走 `mir2_script::cp936::encode_gb2312_dotnet`：不可映射码点回退单个 `0x3F`，
/// 绝不输出 `&#N;`。现在只有测试在用；**切片 3**（客户端帧的解码→处理→**重编码**，
/// 见协议梳理 §2 ⚠️）会用到它，故此处保留并显式允许未使用，避免被当成死代码删掉。
#[allow(dead_code)]
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    mir2_script::cp936::encode_gb2312_dotnet(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网服务名（`AddrTable.txt` 的 GB2312 字节）必须原样往返——与 `bins/login-srv/src/gbk.rs`
    /// 的同名断言同源（同一份 cp936 近似口径）。
    #[test]
    fn deployed_server_name_roundtrips() {
        let raw = [0xC8u8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6];
        assert_eq!(decode(&raw), "热血传奇");
        assert_eq!(encode("热血传奇"), raw);
        assert_eq!(
            encode("loadbot1"),
            b"loadbot1".to_vec(),
            "账号是 ASCII，字节不变"
        );
    }

    /// 判据 E-1（WHATWG-only）：99 个 `.NET cp936` 不可映射、但 WHATWG GBK 有真实字节的
    /// 码点必须回退单个 `0x3F`。牙：改回 `encoding_rs::GBK.encode` 时 `U+01F9` 编成
    /// `A8 BF`、`U+2E81` 编成 `FE 50`，本断言必红。
    #[test]
    fn whatwg_only_codepoints_fall_back_to_question_mark() {
        assert_eq!(encode("\u{01F9}"), vec![0x3F]);
        assert_eq!(encode("\u{2E81}"), vec![0x3F]);
        // 前后的合法字符不受影响（逐码元编码，不是整串替换）。
        assert_eq!(encode("a\u{01F9}\u{8840}"), vec![b'a', 0x3F, 0xD1, 0xAA]);
    }

    /// 判据 E-1（非 BMP）：按 UTF-16 码元拆成两个代理码元，各回退一个 `0x3F`。
    /// 牙：`encoding_rs::GBK.encode` 对 `U+1F600` 输出 `&#128512;`（9 字节），本断言必红。
    #[test]
    fn non_bmp_char_falls_back_to_two_question_marks() {
        assert_eq!(encode("\u{1F600}"), vec![0x3F, 0x3F]);
        assert_eq!(encode("a\u{1F600}b"), vec![b'a', 0x3F, 0x3F, b'b']);
    }

    /// 判据 E-1（合法 GBK 逐字节不变）：现网名字域的常用汉字与 ASCII 必须与
    /// C# `HUtil32.GetBytes` 同字节；best-fit 项（`U+00A6 -> 0x7C`）同 .NET 表。
    #[test]
    fn legal_gbk_bytes_unchanged() {
        assert_eq!(encode("测试"), vec![0xB2, 0xE2, 0xCA, 0xD4]);
        assert_eq!(
            encode("热血传奇"),
            vec![0xC8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6]
        );
        assert_eq!(encode("loadhero/1234"), b"loadhero/1234".to_vec());
        assert_eq!(encode("\u{00A6}"), vec![0x7C]);
    }

    /// 判据 E-1（非法输入）：孤立代理在 .NET 下回退 `0x3F`；Rust `&str` 不能承载孤立
    /// 代理，等价输入是 UTF-16 lossy 后的 `U+FFFD`。同时断言绝不输出 `&#N;`。
    #[test]
    fn invalid_or_unmappable_input_falls_back_to_single_question_mark() {
        let lone_surrogate = String::from_utf16_lossy(&[0xD800]);
        assert_eq!(encode(&lone_surrogate), vec![0x3F]);
        assert_eq!(encode("\u{FFFD}"), vec![0x3F]);
        let emoji = encode("\u{1F600}");
        assert_eq!(emoji, vec![0x3F, 0x3F], "每个代理码元各回退一个 `?`");
        assert!(
            !emoji.windows(2).any(|w| w[0] == b'&' && w[1] == b'#'),
            "绝不能输出数字字符引用"
        );
    }
}
