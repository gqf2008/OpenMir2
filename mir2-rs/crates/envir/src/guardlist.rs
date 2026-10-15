//! `Envir/GuardList.txt` 守卫配置解析 —— 1:1 复刻 `src/GameSrv/DB/LocalDB.cs:100-141`（`LoadGuardList`）。
//!
//! 交付卡：`openmir2-guardlist-guards-loading`（片70：解析半；放置半在宿主接线片）。
//!
//! oracle 行为逐条（行号按 `LocalDB.cs`）：
//! - `:113` 行过滤：空行与 `;` 注释行跳过；
//! - `:117` `GetValidStrCap(sLine, ref monName, ' ')` —— 分隔符**只有空格**（MonGen 用的是 `{ ' ', '\t' }`，
//!   这里逐字照抄单个空格）；首字符为 `"` 时 `ArrestStringEx(monName, "\"", "\"", ref monName)` 去引号；
//! - `:119` `GetValidStr3(sLine, ref mapName, ' ')`；
//! - `:120` `GetValidStr3(sLine, ref cX, { ' ', ',' })`；
//! - `:121` `GetValidStr3(sLine, ref cY, { ' ', ',', ':' })`；
//! - `:122` `GetValidStr3(sLine, ref direction, { ' ', ':' })`；
//! - `:123` 入库门：`monName`、`mapName`、`direction` 三者非空（**X/Y 允许缺省**：`StrToInt16(cX, 0)` 默认 0）；
//! - `:130` `RegenMonsterByName(mapName, StrToInt16(cX,0), StrToInt16(cY,0), monName)`（返 null ⇒ 只跳过置向）；
//! - `:134` `guard.Dir = (byte)StrToInt(direction, 0)`（**`(byte)` 截断**照搬）；
//! - `:139` 整个方法包 try/catch：解析异常被吞只留日志 ⇒ 本模块以逐行 `Result` 承载，宿主按
//!   「整文件尽力而为」语义消费（与 `mongen.rs` 的口径一致）；
//! - oracle **不**在解析层校验地图存在（`RegenMonsterByName` 内部 `FindMap` 失败返 null）⇒ 本模块同样不过滤。

use mir2_shared::hutil32::{
    arrest_string_ex_ref, get_valid_str3, get_valid_str_cap, str_to_int, str_to_int16, ArrestWrite,
};

use crate::error::EnvirError;

/// 一条守卫：行格式 `怪物名 地图 X, Y : 方向`。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardInfo {
    pub mon_name: String,
    pub map_name: String,
    pub x: i16,
    pub y: i16,
    pub dir: u8,
}

/// 解析一行（`LocalDB.cs:113-135`）。空行/注释行 ⇒ `Ok(None)`。
pub fn parse_guard_line(line: &str) -> Result<Option<GuardInfo>, EnvirError> {
    // `:113`：`if (!string.IsNullOrEmpty(sLine) && sLine[0] != ';')`
    if line.is_empty() || line.starts_with(';') {
        return Ok(None);
    }

    // `:117` MonName：`GetValidStrCap(sLine, ref monName, ' ')`（分隔符只有空格）。
    let (mut mon_name, rest) =
        get_valid_str_cap(line, &[' ']).map_err(|_| EnvirError::GuardListCaptureString {
            line: line.to_string(),
        })?;
    // `:117-120` 首字符为 `"` ⇒ `ArrestStringEx` 去引号（与 `mongen.rs` 同款三态写回）。
    if !mon_name.is_empty() && mon_name.starts_with('"') {
        if let (ArrestWrite::Value(v), _) = arrest_string_ex_ref(&mon_name, '"', '"') {
            mon_name = v;
        }
    }

    // `:119` MapName（`{ ' ' }`）。
    let (map_name, rest) = get_valid_str3(&rest, &[' ']);
    // `:120` X（`{ ' ', ',' }`；`StrToInt16(cX, 0)`）。
    let (c_x, rest) = get_valid_str3(&rest, &[' ', ',']);
    // `:121` Y（`{ ' ', ',', ':' }`；`StrToInt16(cY, 0)`）。
    let (c_y, rest) = get_valid_str3(&rest, &[' ', ',', ':']);
    // `:122` Direction（`{ ' ', ':' }`）。
    let (direction, _) = get_valid_str3(&rest, &[' ', ':']);

    // `:123` 入库门：monName/mapName/direction 三者非空（X/Y 可缺省）。
    if mon_name.is_empty() || map_name.is_empty() || direction.is_empty() {
        return Ok(None);
    }

    Ok(Some(GuardInfo {
        mon_name,
        map_name,
        // `:130` `(short)StrToInt16(cX/cY, 0)`。
        x: str_to_int16(&c_x, 0),
        y: str_to_int16(&c_y, 0),
        // `:134` `(byte)StrToInt(direction, 0)` —— byte 截断照搬。
        dir: str_to_int(&direction, 0) as u8,
    }))
}

/// 整文件解析（`LoadGuardList` 的解析半）。
///
/// 行尾 CR 吸收（C# `StringList.LoadFromFile` 语义）；任何一行的 `GetValidStrCap` panic
/// 形态都以 `Err` 冒泡给调用方（oracle 的 catch 只是不让异常逃出方法，宿主消费口径见卡）。
pub fn load_guard_list(file_text: &str) -> Result<Vec<GuardInfo>, EnvirError> {
    let mut out = Vec::new();
    for line in file_text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(g) = parse_guard_line(line)? {
            out.push(g);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `E:\MirServer\M2GameSvr\Envir\GuardList.txt` 的真实行形态
    /// （第 28 行，多空格 + `X, Y` + `: dir`；GBK 解码后的中文名）。
    #[test]
    fn parses_deployed_shape_line28() {
        let g = parse_guard_line("卫士   0        281, 661    : 5")
            .unwrap()
            .unwrap();
        assert_eq!(
            g,
            GuardInfo {
                mon_name: "卫士".to_string(),
                map_name: "0".to_string(),
                x: 281,
                y: 661,
                dir: 5,
            }
        );
    }

    /// 第 1 行形态（`: 7` 方向）与无逗号空格变体。
    #[test]
    fn parses_more_deployed_shapes() {
        let g = parse_guard_line("卫士   0        275, 212    : 7")
            .unwrap()
            .unwrap();
        assert_eq!(g.x, 275);
        assert_eq!(g.y, 212);
        assert_eq!(g.dir, 7);
        // 无空格变体：`X,Y:dir`（`GetValidStr3` 的 RemoveEmptyEntries 语义吸收空 token）。
        let g2 = parse_guard_line("G 0 300,400:3").unwrap().unwrap();
        assert_eq!(g2.x, 300);
        assert_eq!(g2.y, 400);
        assert_eq!(g2.dir, 3);
    }

    #[test]
    fn skips_comment_and_empty_lines() {
        assert_eq!(parse_guard_line(";comment").unwrap(), None);
        assert_eq!(parse_guard_line("").unwrap(), None);
        // 只有空白 ⇒ 首段切出空 monName ⇒ 入库门拒绝。
        assert_eq!(parse_guard_line("   ").unwrap(), None);
    }

    /// `:123` 入库门：direction 缺失 ⇒ None（X/Y 缺省反而合法）。
    #[test]
    fn gate_requires_name_map_direction_but_not_xy() {
        assert_eq!(parse_guard_line("G 0 281, 661").unwrap(), None); // 无方向
        assert_eq!(parse_guard_line("G 281, 661 : 5").unwrap(), None); // 无地图
                                                                       // 注意：`G 0 : 5` 在 oracle 里也**被拒**——cX 的分隔符是 { ' ', ',' }（不含 ':'），
                                                                       // `: 5` 的首 token 切成 `:`，direction 落空 ⇒ 入库门拒绝。X/Y 实际不可缺省。
        assert_eq!(parse_guard_line("G 0 : 5").unwrap(), None);
    }

    /// `:134` `(byte)` 截断照搬：260 → 4。
    #[test]
    fn dir_byte_truncation_like_oracle() {
        let g = parse_guard_line("G 0 1, 2 : 260").unwrap().unwrap();
        assert_eq!(g.dir, 4);
    }

    /// 引号名（BUG-REPLICA，登记不修）：`GetValidStrCap` 的引号捕获走 C# `CaptureString` 的
    /// c=1 扫描坑（LESSON B-10：dest 含开引号、遇首个空格截断）⇒ `"Big Guard" ...` 切成 `"Bi`；
    /// 随后 `ArrestStringEx(\"Bi\", '"', '"')` 无闭引号 ⇒ Unchanged ⇒ mon_name 保持 `"Bi`。
    /// 部署件 `GuardList.txt` 全部为裸中文名（无引号）⇒ 本形态实际不可达，钉住防"顺手修"。
    #[test]
    fn quoted_mon_name_replicates_capture_string_quirk() {
        let g = parse_guard_line("\"Big Guard\" 0 10, 20 : 6")
            .unwrap()
            .unwrap();
        assert_eq!(g.mon_name, "\"Bi");
        // 连锁错位照搬：名字吞掉 "Big 后，余下 Guard" 0 10, 20 : 6 依次落到 map/cX/cY/direction。
        assert_eq!((g.x, g.y, g.dir), (0, 10, 20));
    }

    /// 整文件：注释/空行混入 + CRLF 行尾吸收。
    #[test]
    fn load_guard_list_filters_and_handles_crlf() {
        let text = ";头\r\n卫士   0        281, 661    : 5\r\n\r\nG 0 1, 2 : 3\r\n";
        let list = load_guard_list(text).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].dir, 5);
        assert_eq!(list[1].dir, 3);
    }
}
