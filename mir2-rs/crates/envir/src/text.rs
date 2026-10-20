//! 文本解码 —— 对齐 `src/OpenMir2/Common/StringList.cs:276-360` 的 `GetEncoding` + `ReadLine`。
//!
//! `StringList.LoadFromFile` 默认用 **gb2312**（.NET 实为 cp936）；前 3~4 字节是 BOM 时
//! 切换为 UTF-8 / UTF-32LE / UTF-16LE / UTF-16BE / UTF-32BE（`StreamReader` 的 BOM 探测
//! 优先）。实现统一委托给 `mir2_script::cp936::decode_string_list`（B-105 / E-4），
//! 与 `crates/script` 共用同一状态机、7032 条覆盖表与 BOM 顺序。

/// 读文件字节 → 文本（自动去 BOM 并选编码；无 BOM 按 cp936）。
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    mir2_script::cp936::decode_string_list(bytes)
}
