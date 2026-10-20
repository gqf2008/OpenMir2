//! `.NET Encoding.GetEncoding(936)` 编解码入口（**兼容 re-export**）。
//!
//! 实现已下沉到独立 crate [`mir2_encoding::cp936`]（审计 v2 §5「统一编码模块」）：
//! `crates/shared` 有「零依赖」硬契约，`crates/data`/`crates/envir` 也不能依赖
//! `mir2-script`，因此编码核心单独成 crate，依赖方向 `encoding ← script`。
//!
//! 本模块只做转发，**不复制实现**：`mir2_script::cp936::*` 的全部公开 API
//! （`encode_gb2312_dotnet` / `decode` / `decode_string_list` /
//! `decode_stream_reader` / `decode_config_text`）与原实现逐字节一致。
//! B-105 的穷举门禁仍由 `crates/script/tests/gbk_decode.rs` 与
//! `crates/script/tests/cp936_encode.rs` 覆盖。

pub use mir2_encoding::cp936::*;
