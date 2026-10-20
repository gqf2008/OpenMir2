//! OpenMir2 文本编码核心（.NET cp936 / 两套 BOM 模型）。
//!
//! 全 workspace **唯一的** cp936 编解码实现。`crates/script` 的 `cp936` 模块与
//! `crates/envir` 的 `text` 模块都委托到这里；`crates/shared` 的「零依赖」硬契约
//! 不允许把编码表塞进去，因此单独成 crate（依赖方向：`encoding ← script`、
//! `encoding ← data`、`encoding ← envir`，无环）。
//!
//! - 编码：逐 UTF-16 码元查 .NET `Encoding.GetEncoding(936)` 全表；不可映射 →
//!   单个 `0x3F`（非 BMP 每代理码元一个 `?`）。绝不输出 `&#N;`。
//! - 解码：cp936 状态机 + 7032 条覆盖表（B-105）。
//! - BOM 两套模型：`decode_string_list`（`StringList.LoadFromFile`）与
//!   `decode_config_text` / `decode_stream_reader`（`ConfigFile.Load` 的裸
//!   `StreamReader(..., gb2312)`）。
//!
//! 详见 [`cp936`] 模块文档；权威审计 `tests/parity/m3/audit-encoding-v2-20261021.md`。

#![forbid(unsafe_code)]

pub mod cp936;
pub mod gbk_overrides;

pub use cp936::{
    decode, decode_config_text, decode_stream_reader, decode_string_list, encode_gb2312_dotnet,
};
pub use gbk_overrides::GBK_OVERRIDES;
