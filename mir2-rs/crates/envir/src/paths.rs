//! 取图路径口径 —— 1:1 复刻 `M2Share.GetEnvirFilePath` 与 `Path.Combine` 的 .NET 语义。
//!
//! 关键事实（`src/M2Server/M2Share.cs:1970-1986`、`src/GameSrv/Maps/MapManager.cs:96`）：
//! - 地图文件 = `Path.Combine(BasePath, Config.MapDir, 文件名 + ".map")`，`BasePath = AppContext.BaseDirectory`（绝对）；
//! - `MapManager` 把**绝对路径**传给 `LoadMapData`，而 `LoadMapData` 里的点位表路径是
//!   `GetEnvirFilePath("Point", $"{sMapFile}.txt")`；
//! - .NET `Path.Combine` 遇到**后一个参数是 rooted path** 时会**丢弃**之前的参数
//!   ⇒ 真机下的点位表路径其实是 `<那个 .map 的绝对路径>.txt`（不是 `Envir/Point/`）。
//!   这里按源码口径实现，别"顺手修"成直觉写法（§1 硬约束③：行为等价优先于修 bug）。
//!
//! `..` 前缀特例照抄 `M2Share.GetEnvirFilePath`：以 `..` 开头时取 `BasePath/EnvirDir/<去掉前 3 个字符>`。

use std::path::{Path, PathBuf};

/// `GameSvrConf.EnvirDir` 默认值（`GameSvrConf.cs:1295`）。
pub const DEFAULT_ENVIR_DIR: &str = "Envir";
/// `GameSvrConf.MapDir` 默认值（`GameSvrConf.cs:1296`）。
pub const DEFAULT_MAP_DIR: &str = "Map";
/// 点位表子目录（`LoadMapData` 的 `GetEnvirFilePath("Point", …)`）。
pub const POINT_DIR: &str = "Point";

/// `System.IO.Path.Combine`：从左到右拼接；参数为空跳过；参数是 rooted path 时**丢弃**之前结果。
#[must_use]
pub fn path_combine(parts: &[&Path]) -> PathBuf {
    let mut result = PathBuf::new();
    for part in parts {
        if part.as_os_str().is_empty() {
            continue;
        }
        if part.is_absolute() || result.as_os_str().is_empty() {
            result = (*part).to_path_buf();
        } else {
            result.push(part);
        }
    }
    result
}

/// `M2Share.GetEnvirFilePath(dirPath, filePath)`（两参数重载）。
#[must_use]
pub fn envir_file_path(
    base_path: &Path,
    envir_dir: &str,
    dir_path: &str,
    file_path: &str,
) -> PathBuf {
    if let Some(rest) = file_path.strip_prefix("..") {
        // C#：`filePath[3..]`（把 ".." 及其后 1 个字符一起去掉）。
        let trimmed: String = rest.chars().skip(1).collect();
        return path_combine(&[base_path, Path::new(envir_dir), Path::new(&trimmed)]);
    }
    path_combine(&[
        base_path,
        Path::new(envir_dir),
        Path::new(dir_path),
        Path::new(file_path),
    ])
}

/// `MapManager.AddMapInfo` 传给 `LoadMapData` 的绝对/相对路径。
#[must_use]
pub fn map_file_path(base_path: &Path, map_dir: &str, file_name: &str) -> PathBuf {
    path_combine(&[
        base_path,
        Path::new(map_dir),
        Path::new(&format!("{file_name}.map")),
    ])
}

/// `LoadMapData` 里的 `<map_file>.txt`（`GetEnvirFilePath("Point", …)`）。
#[must_use]
pub fn point_file_path(base_path: &Path, envir_dir: &str, map_file: &Path) -> PathBuf {
    let file = format!("{}.txt", map_file.display());
    envir_file_path(base_path, envir_dir, POINT_DIR, &file)
}
