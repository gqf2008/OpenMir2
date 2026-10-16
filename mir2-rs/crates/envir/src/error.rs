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

    /// `Envir/MonGen.txt` 怪物名列走了 oracle `HUtil32.CaptureString` 的越界分支
    /// （C# 抛 `IndexOutOfRangeException`，`LocalDB.LoadMonGen` 无 try/catch 兜底）。
    #[error(
        "MonGen.txt 怪物名列解析越界（CaptureString，对应 C# IndexOutOfRangeException）：{line}"
    )]
    MonGenCaptureString { line: String },

    /// GuardList.txt 怪物名列解析越界（CaptureString，对应 C# IndexOutOfRangeException）；
    /// 交付卡 `openmir2-guardlist-guards-loading`（`LocalDB.cs:117` 的 GetValidStrCap）。
    #[error(
        "GuardList.txt 怪物名列解析越界（CaptureString，对应 C# IndexOutOfRangeException）：{line}"
    )]
    GuardListCaptureString { line: String },

    /// `Envir/MakeItem.txt` 组名重复（`Dictionary<string, IList<MakeItem>>.Add` 抛
    /// `ArgumentException`，`LocalDB.LoadMakeItem` 无 try/catch ⇒ 异常穿透）。
    /// 交付卡 `openmir2-envir-redlist-batch`（R0-3；`LocalDB.cs:170/187`）。
    #[error("MakeItem.txt 组名重复（对应 C# Dictionary.Add 抛 ArgumentException）：{name}")]
    MakeItemDuplicateGroup { name: String },

    /// Npcs.txt 角色名列解析越界（CaptureString，对应 C# IndexOutOfRangeException）；
    /// 交付卡 `openmir2-npcs-fixed-loading`（`LocalDB.cs:674` 的 GetValidStrCap）。
    #[error(
        "Npcs.txt 角色名列解析越界（CaptureString，对应 C# IndexOutOfRangeException）：{line}"
    )]
    NpcsCaptureString { line: String },

    /// 商人脚本（`Market_Def/<ScriptName>-<MapName>.txt`）解析抛出 `LoadPanic`
    /// （对应 C# `ScriptParsers.LoadScriptFile` 内异常的穿透，如重复 label 的
    /// `ArgumentException` / `CaptureString` 越界）。
    #[error("商人脚本解析失败（{script}）：{reason}")]
    MerchantScriptLoad { script: String, reason: String },

    /// `Envir/NoClearMonList.txt` 不存在时 oracle 的**异常穿透**（`M2Share.cs:1763`）：
    /// `File.Exists == false` ⇒ `LoadList.SaveToFile(sFileName)`，而 `StringList.SaveToFile`
    /// 首行 `GetEncoding(fileName, …)` → `new FileStream(fileName, FileMode.Open)`
    /// 抛 `FileNotFoundException`（`LoadNoClearMonList` 无 try/catch）。
    /// 交付卡 `openmir2-envir-redlist-batch`。**注意**：缺文件在 oracle 里不是「空名单」而是异常，
    /// 别"顺手修"成静默空表（§1 硬约束③：行为等价优先于修 bug）。
    #[error("NoClearMonList.txt 不存在（oracle 走 SaveToFile 分支抛 FileNotFoundException）：{0}")]
    NoClearMonListMissing(PathBuf),
}
