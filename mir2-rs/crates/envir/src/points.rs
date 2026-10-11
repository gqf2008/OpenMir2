//! `Point/*.txt` 点位表 —— 1:1 复刻 `src/M2Server/Maps/Envirnoment.cs:1004-1027`。
//!
//! 源码逻辑：
//! ```text
//! line = GetValidStr3(line, ref sX, { ',', '\t' });   // sX = 第一段，line = 剩余
//! line = GetValidStr3(line, ref sY, { ',', '\t' });   // sY = 第二段
//! nX = StrToInt16(sX, -1); nY = StrToInt16(sY, -1);
//! if (nX >= 0 && nY >= 0 && nX < Width && nY < Height) PointList.Add(new PointInfo(nX, nY));
//! ```
//! - 空行与**首字符为 `;`** 的行跳过（不 trim 后再判）；
//! - 分隔符只有 `,` 与 `\t`（**空格不是分隔符**）；
//! - 解析失败按 -1，负值或越界一律**丢弃**（不报错）。

use mir2_shared::hutil32::get_valid_str3;

/// `LoadMapData` 点位表用的分隔符（`new[] { ',', '\t' }`）。
pub const POINT_SEPARATORS: [char; 2] = [',', '\t'];

/// 一个点位（`src/Modules/SystemModule/Data/Pointinfo.cs`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PointInfo {
    pub x: i16,
    pub y: i16,
}

/// 解析点位表文本；`width`/`height` 是地图尺寸（用于越界丢弃）。
#[must_use]
pub fn parse_points(text: &str, width: i32, height: i32) -> Vec<PointInfo> {
    let mut out = Vec::new();
    // `StreamReader.ReadLine` 按 \r\n / \n / \r 分行；这里覆盖最常见的 \n 与 \r\n。
    for raw in text.split('\n') {
        let line = raw.strip_suffix('\r').unwrap_or(raw);
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (sx, rest) = get_valid_str3(line, &POINT_SEPARATORS);
        let (sy, _) = get_valid_str3(&rest, &POINT_SEPARATORS);
        let nx = str_to_i16(&sx, -1);
        let ny = str_to_i16(&sy, -1);
        if nx >= 0 && ny >= 0 && i32::from(nx) < width && i32::from(ny) < height {
            out.push(PointInfo { x: nx, y: ny });
        }
    }
    out
}

/// `HUtil32.StrToInt16(str, def)`：`short.TryParse` 失败取 `def`。
fn str_to_i16(s: &str, def: i16) -> i16 {
    s.trim().parse::<i16>().unwrap_or(def)
}
