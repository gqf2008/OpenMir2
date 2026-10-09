//! OpenMir2 Envir 脚本引擎（Rust 1:1 移植，B 线 / M4）。
//!
//! 硬约束：客户端冻结、内容冻结（脚本语法与文件格式不改）、行为等价优先于修 bug。
//! 参照实现：`src/Modules/ScriptEngine/`（ScriptParsers.cs / ScriptEngine.cs /
//! Consts/{ConditionCode,ExecutionCode,GrobalVarCode}.cs / Processings/*.cs）。
//!
//! 本批范围：词法/语法解析 + 加载统计对拍。条件/动作求值与内建命令见后续批次。

#![forbid(unsafe_code)]

pub mod codes;
pub mod hutil32;
pub mod model;
pub mod parser;
pub mod random;
pub mod textfile;

pub use model::{
    Goods, MerchantFlags, QuestActionInfo, QuestConditionInfo, SayingProcedure, SayingRecord,
    ScriptInfo, ScriptQuestInfo,
};
pub use parser::{
    LoadOutcome, LoadPanic, LoadStats, ParseError, ParseErrorKind, ScriptFs, ScriptParsers,
};
pub use textfile::{decode_bytes, split_lines, LocalFs};
