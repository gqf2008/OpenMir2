//! 存储错误类型。

use thiserror::Error;

/// 存储层错误。
#[derive(Debug, Error)]
pub enum StorageError {
    /// 后端连接/查询失败（MySQL 等）。
    #[error("backend: {0}")]
    Backend(String),
    /// 记录不存在（C# 侧对应 `ref bool success = false` / 返回 -1 的路径）。
    #[error("not found: {0}")]
    NotFound(String),
    /// 数据损坏或编码不合法。
    #[error("corrupt data: {0}")]
    Corrupt(String),
}
