//! 加载错误。口径：C# `Envirnoment.LoadMapData` 在**任一异常**下 `result=false`（并吞掉异常只记日志），
//! 这里把失败原因显式上抛，方便集成点「找不到就报错，别静默用占位图」。

use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum EnvirError {
    /// 对应 `File.Exists(sMapFile) == false`（`LoadMapData` 此时 `result` 保持 false）。
    #[error("地图文件不存在: {0}")]
    FileNotFound(PathBuf),

    /// 对应 `fileStream.Length <= 52` 的提前返回 false。
    #[error("地图文件太小（<= 52 字节），无法加载：{size} 字节")]
    FileTooSmall { size: u64 },

    /// 对应 `Initialize` 只在 `nWidth > 1 && nHeight > 1` 时分配 `CellArray`：
    /// 尺寸 <=1 时后续 `CellArray[n24+nH]` 会 NRE，被外层 catch 吞掉 ⇒ `result=false`。
    #[error("地图尺寸非法（源码 Initialize 要求 W/H 均 > 1）：W={w}, H={h}")]
    InvalidSize { w: i32, h: i32 },

    /// 读取中途越界（C# `BinaryReader.ReadUInt16` 抛 `EndOfStreamException` ⇒ `result=false`）。
    #[error("地图数据截断：W={w}, H={h} 需要 {need} 字节（含 52 字节头），实际 {actual} 字节")]
    Truncated {
        w: i32,
        h: i32,
        need: u64,
        actual: u64,
    },

    /// `MapRegistry` 的名称查找失败（对应 `MapManager.FindMap` 返回 null）。
    #[error("地图名未找到: {0}")]
    MapNameNotFound(String),

    /// 读配置文件（MapInfo.txt 等）失败。
    #[error("读取 {path} 失败: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}
