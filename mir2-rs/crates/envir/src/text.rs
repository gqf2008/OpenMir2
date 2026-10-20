//! 文本解码 —— 两套 .NET BOM 模型，按 oracle 调用点区分，**不可混用**。
//!
//! - [`decode_text`]：`StringList.LoadFromFile`（`src/OpenMir2/Common/StringList.cs:276-360`）
//!   口径 —— Envir 脚本/名单等文本。oracle 先用 `GetEncoding` 选默认编码，再
//!   `new StreamReader(fileName, fs)`；对 `FF FE xx`（xx != `FF`）`GetEncoding` 给
//!   `Encoding.Unicode`，`StreamReader` 因此不会再升级到 UTF-32LE ⇒ **`FF FE 00 00`
//!   是 UTF-16LE**（首字符 `U+0000`）。实现：
//!   `mir2_encoding::decode_string_list`（B-105 / E-4）。
//! - [`decode_config_text`]：`ConfigFile.Load`（`Common/ConfigFile.cs:263`）的裸
//!   `new StreamReader(stream, Encoding.GetEncoding("GB2312"))` 口径 —— `Server.conf` /
//!   `SabukW.txt` 等配置文件。无 `GetEncoding` 预判 ⇒ **`FF FE 00 00` 是 UTF-32LE**。
//!   实现：`mir2_encoding::decode_config_text`（与 `decode_stream_reader` 等价）。
//!
//! 两套都无 BOM 时按 cp936（7032 条覆盖表，见 `crates/encoding/src/gbk_overrides.rs`）。

/// `StringList.LoadFromFile` 口径：读文件字节 → 文本（去 BOM、选编码；无 BOM 按 cp936）。
#[must_use]
pub fn decode_text(bytes: &[u8]) -> String {
    mir2_encoding::decode_string_list(bytes)
}

/// `ConfigFile.Load` 口径：`StreamReader(..., gb2312)` 的 BOM 探测（与 [`decode_text`]
/// 的差异只在 `FF FE 00 00`：这里是 UTF-32LE，那里是 UTF-16LE）。
#[must_use]
pub fn decode_config_text(bytes: &[u8]) -> String {
    mir2_encoding::decode_config_text(bytes)
}
