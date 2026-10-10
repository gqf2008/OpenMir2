//! mir2-protocol —— OpenMir2 线缆协议（M0 冻结接口）。
//!
//! 三个组成部分：
//! - [`messages`]：消息号表（由 `tools/msgcodegen` 从 `src/OpenMir2/Messages.cs` 机械生成）；
//! - [`frame`]：帧类型与编解码（`CommandMessage` / 客户端帧 `#1...!` / 服务端帧 `#...!` /
//!   内部 `ServerMessage`、`ServerDataPacket`）；
//! - [`edcode`]：EDCode 编解码（XOR 0xAC + 6-bit 分组，逐字节兼容 C# `EncryptUtil`）。
//!
//! **接口冻结**：本 crate 的公开类型自 M0 接口冻结 tag 起，改动须走评审（改接口 = 全线返工，
//! 见 `docs/Rust重写设计文档.md` §12.4）。
//!
//! 编码约定：本层只处理字节；线上文本是 GB2312 字节流，不做 UTF-8 转换。

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// 文档中逐字引用 C# 标识符/文件名（Messages.cs、ClientSession 等），不加反引号改造名。
#![allow(clippy::doc_markdown)]

pub mod client_only;
pub mod edcode;
pub mod frame;
pub mod internal;
pub mod messages;

pub use client_only::CM_QUERYDYNCODE;
pub use frame::{
    ClientMessage, CommandMessage, FrameError, FrameSplitter, ServerDataPacket, ServerFrameTail,
    ServerMessage, SplitOut,
};
pub use internal::{ServerDataMessage, ServerDataType};
