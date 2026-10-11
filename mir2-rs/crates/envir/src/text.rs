//! 文本解码 —— 对齐 `src/OpenMir2/Common/StringList.cs:276-360` 的 `GetEncoding` + `ReadLine`。
//!
//! `StringList.LoadFromFile` 默认用 **gb2312**；仅当前 3~4 字节是 BOM 时切换为
//! UTF-8 / UTF-16LE / UTF-16BE。这里照抄该判定（GB2312 用 `encoding_rs::GBK` 解码，
//! GBK 是 GB2312 的超集，与本仓其余 GBK 文本加载同口径）。

/// 读文件字节 → 文本（自动去 BOM 并选编码）。
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        let (text, _, _) = encoding_rs::UTF_8.decode(&bytes[3..]);
        return text.into_owned();
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (text, _, _) = encoding_rs::UTF_16BE.decode(&bytes[2..]);
        return text.into_owned();
    }
    // C# 判定：0xFF 0xFE 且第 3 字节 != 0xFF ⇒ Unicode(LE)；0xFF 0xFE 0xFF 落到默认编码。
    if bytes.starts_with(&[0xFF, 0xFE]) && bytes.get(2) != Some(&0xFF) {
        let (text, _, _) = encoding_rs::UTF_16LE.decode(&bytes[2..]);
        return text.into_owned();
    }
    let (text, _, _) = encoding_rs::GBK.decode(bytes);
    text.into_owned()
}
