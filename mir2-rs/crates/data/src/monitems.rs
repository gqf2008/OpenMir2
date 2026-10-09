//! `Envir/MonItems/*.txt` 解析，移植源 `src/GameSrv/DB/LocalDB.cs:596`
//! （`LoadMonitems`）。文件为 GBK 编码。

use std::path::Path;

use mir2_shared::hutil;

use crate::models::MonsterDropItem;

/// C# `LocalDB.MonsterSpitConst`（`LocalDB.cs:13`）。
const MONSTER_SPIT_CONST: [char; 3] = [' ', '/', '\t'];
/// C# `LocalDB.TextSplitConst`（`LocalDB.cs:12`）。
const TEXT_SPLIT_CONST: [char; 2] = [' ', '\t'];

/// 解析一个 MonItems 文件（GBK 字节）为掉落表。
///
/// 逐条对应 `LoadMonitems`：
/// - 跳过空行与 `;` 注释行；
/// - 前两个 token 按 `[' ', '/', '\t']` 切（`StrToInt(s, -1)`）；
/// - 第三个 token 按 `[' ', '\t']` 切，以 `"` 开头则取引号内文本为物品名；
/// - 第四个 token 为数量（`StrToInt(s, 1)`）；
/// - 仅当 `n18 > 0 && n1C > 0 && 物品名非空` 才收录；
/// - 收录时 `SelPoint = n18 - 1`、`MaxPoint = n1C`。
pub fn parse_monitems_gbk(bytes: &[u8]) -> Vec<MonsterDropItem> {
    let (text, _, _) = encoding_rs::GBK.decode(bytes);
    let mut list = Vec::new();
    for line in text.lines() {
        if line.is_empty() || line.starts_with(';') {
            continue;
        }
        let (tok, rest) = hutil::get_valid_str3(line, &MONSTER_SPIT_CONST);
        let n18 = hutil::str_to_int(&tok, -1);
        let (tok, rest) = hutil::get_valid_str3(&rest, &MONSTER_SPIT_CONST);
        let n1c = hutil::str_to_int(&tok, -1);
        let (mut item_name, rest) = hutil::get_valid_str3(&rest, &TEXT_SPLIT_CONST);
        if item_name.starts_with('"') {
            if let Some((name, _)) = hutil::arrest_string_ex_quoted(&item_name) {
                item_name = name;
            }
        }
        let (tok, _) = hutil::get_valid_str3(&rest, &TEXT_SPLIT_CONST);
        let item_count = hutil::str_to_int(&tok, 1);
        if n18 > 0 && n1c > 0 && !item_name.is_empty() {
            list.push(MonsterDropItem {
                sel_point: n18 - 1,
                max_point: n1c,
                item_name,
                count: item_count,
            });
        }
    }
    list
}

/// 从文件路径加载（GBK）。
pub fn load_monitems(path: &Path) -> std::io::Result<Vec<MonsterDropItem>> {
    let bytes = std::fs::read(path)?;
    Ok(parse_monitems_gbk(&bytes))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 夹具统一走 GBK 编码（MonItems 真实文件格式）。
    fn gbk(s: &str) -> Vec<u8> {
        encoding_rs::GBK.encode(s).0.into_owned()
    }

    #[test]
    fn parses_typical_lines() {
        let text = gbk("; 注释\n10/100     金币 300\n\n10/300    魔法药(中量)\n10/3000   海魂\n");
        let list = parse_monitems_gbk(&text);
        assert_eq!(list.len(), 3);
        assert_eq!(
            list[0],
            MonsterDropItem {
                sel_point: 9,
                max_point: 100,
                item_name: "金币".to_string(),
                count: 300,
            }
        );
        // 无数量列 → 默认 1
        assert_eq!(list[1].count, 1);
        assert_eq!(list[2].item_name, "海魂");
    }

    #[test]
    fn skips_invalid_rows() {
        // n18=0 / n1C=0 / 无物品名 都不收录
        let text = gbk("0/100 金币\n10/0 金币\n10/100\n");
        assert!(parse_monitems_gbk(&text).is_empty());
    }

    #[test]
    fn quoted_item_name() {
        let text = gbk("10/100 \"裁决之杖\" 1\n");
        let list = parse_monitems_gbk(&text);
        assert_eq!(list[0].item_name, "裁决之杖");
        assert_eq!(list[0].count, 1);
    }
}
