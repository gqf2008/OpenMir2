//! `HUtil32.GetString` / `HUtil32.GetBytes` 的 gb2312(cp936) 等价物。
//!
//! C# 侧一律走 `Encoding.GetEncoding("gb2312")`（= .NET cp936）：
//! - 读配置/脚本：`StringList`（BOM 识别后 gb2312）；
//! - 线上文本：`HUtil32.GetString(bytes)` / `GetBytes(str)`。
//!
//! 本模块用 `encoding_rs::GBK` 近似 cp936：**已登记的差异**见
//! `mir2-rs/tests/parity/whitelist.md` 的 A-14（PUA 少量字节对映射与
//! 非法字节回退字符 U+FFFD vs `?`；现网语料为 ASCII 账号 + 常用 GBK 汉字，判定相同）。

/// 解码（对应 `HUtil32.GetString(bytes, 0, count)`）。
#[must_use]
pub fn decode(bytes: &[u8]) -> String {
    let (s, _, _) = encoding_rs::GBK.decode(bytes);
    s.into_owned()
}

/// 编码（对应 `HUtil32.GetBytes(str)`）。
#[must_use]
pub fn encode(s: &str) -> Vec<u8> {
    let (bytes, _, _) = encoding_rs::GBK.encode(s);
    bytes.into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 现网服务名（`AddrTable.txt` 的 GB2312 字节）必须原样往返。
    #[test]
    fn deployed_server_name_roundtrips() {
        let raw = [0xC8u8, 0xC8, 0xD1, 0xAA, 0xB4, 0xAB, 0xC6, 0xE6];
        assert_eq!(decode(&raw), "热血传奇");
        assert_eq!(encode("热血传奇"), raw);
        assert_eq!(encode("127.0.0.1/7100/25"), b"127.0.0.1/7100/25");
    }
}
