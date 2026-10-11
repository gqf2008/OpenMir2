//! `HUtil32.GetString` / `HUtil32.GetBytes` 的 gb2312(cp936) 等价物。
//!
//! 与 `bins/login-srv/src/gbk.rs` 同源（同一份差异登记）：C# 侧一律走
//! `Encoding.GetEncoding("gb2312")`（= .NET cp936），Rust 用 `encoding_rs::GBK` 近似；
//! **已登记的差异**见 `mir2-rs/tests/parity/whitelist.md` 的 A-14（PUA 少量字节对映射与非法字节
//! 回退字符；现网语料为 ASCII 账号 + 常用 GBK 汉字，判定相同）。

/// 解码（对应 `HUtil32.GetString(bytes, 0, count)`）。
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    let (s, _, _) = encoding_rs::GBK.decode(bytes);
    s.into_owned()
}

/// 编码（对应 `HUtil32.GetBytes(str)`）。
///
/// 现在只有测试在用；**切片 3**（客户端帧的解码→处理→**重编码**，见协议梳理 §2 ⚠️）会用到它，
/// 故此处保留并显式允许未使用，避免被当成死代码删掉。
#[allow(dead_code)]
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    let (bytes, _, _) = encoding_rs::GBK.encode(s);
    bytes.into_owned()
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
}
