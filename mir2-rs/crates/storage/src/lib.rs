//! mir2-storage —— 存储层接口（M0 接口冻结版）。
//!
//! 镜像 C# `src/Storeages/DBSrv.Storage/` 的四个插件接口（`IPlayRecordStorage` /
//! `IPlayDataStorage` / `ICacheStorage` / `IMarketStorage`），供 DBSrv 与 GameSvr 侧使用。
//!
//! **冻结范围（Day-1）**：trait 集合与方法签名。记录类型（[`CharacterDataInfo`] 等）
//! 的字段已随 M1 S4a 落地（[`playerdata`]），并由 C# oracle 字节向量逐字节钉住
//! （`tests/playerdatainfo_vectors.rs`）——**字段现在也是冻结面**，改动须同时更新向量。
//! 改 trait 方法签名仍须评审。
//!
//! 实现侧（M1）：`sqlx` + MySQL，库表边界见设计文档 §5.1（表结构一字不改）。

#![forbid(unsafe_code)]
#![warn(missing_docs)]
// 文档中逐字引用 C# 标识符/文件名（Messages.cs、ClientSession 等），不加反引号改造名。
#![allow(clippy::doc_markdown)]
// 全 trait 方法统一返回 `Result<_, StorageError>`，错误语义在 [`StorageError`] 一处定义，
// 逐个方法重复 `# Errors` 只增噪声。
#![allow(clippy::missing_errors_doc)]

mod account;
mod error;
mod legacy_dsn;
pub mod playerdata;
mod traits;
mod types;

pub use account::{AccountQuick, AccountRecord, AccountStore};
pub use error::StorageError;
pub use legacy_dsn::LegacyDsn;
pub use playerdata::{
    Ability, CharacterData, CharacterDataInfo, LoadCharacterData, LoadPlayerDataPacket, MagicRcd,
    MemPack, NakedAbility, PlayerRecordData, RecordHeader, SaveCharacterData, ServerRequestData,
    ServerRequestMessage, ServerUserItem,
};
pub use traits::{CacheStorage, MarketStorage, PlayDataStorage, PlayRecordStorage};
pub use types::{MarketItem, PlayQuick, QueryChr};
