//! OpenMir2 Envir 脚本引擎（Rust 1:1 移植，B 线 / M4）。
//!
//! 硬约束：客户端冻结、内容冻结（脚本语法与文件格式不改）、行为等价优先于修 bug。
//! 参照实现：`src/Modules/ScriptEngine/`（ScriptParsers.cs / ScriptEngine.cs /
//! Consts/{ConditionCode,ExecutionCode,GrobalVarCode}.cs / Processings/*.cs）。
//!
//! 本批范围：词法/语法解析 + 加载统计对拍。条件/动作求值与内建命令见后续批次。

#![forbid(unsafe_code)]

pub mod codes;
pub mod digest;
pub mod gbk_overrides;
pub mod model;
pub mod parser;
pub mod textfile;

/// `HUtil32` 原语：全 workspace 唯一实现在 `mir2-shared`（B-106 收敛）。
pub use mir2_shared::hutil32;

pub use digest::{
    structure_digest, structure_digest_full, MerchantDigestInput, StructureCounts, StructureDigest,
};
pub use model::{
    Goods, MerchantFlags, QuestActionInfo, QuestConditionInfo, SayingProcedure, SayingRecord,
    ScriptInfo, ScriptQuestInfo,
};
pub use parser::{
    LoadOutcome, LoadPanic, LoadStats, ParseError, ParseErrorKind, ScriptFs, ScriptParsers,
};
pub use textfile::{decode_bytes, fnv1a64, split_lines, text_as_string_list_text, LocalFs};
