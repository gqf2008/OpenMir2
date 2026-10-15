//! `Envir/MakeItem.txt` 物品合成配方解析 —— 1:1 复刻 `src/GameSrv/DB/LocalDB.cs:149-190`（`LoadMakeItem`）。
//!
//! 交付卡：`openmir2-envir-redlist-batch`（R0-3 装载半）。
//!
//! oracle 行为逐条（行号按 `LocalDB.cs`）：
//! - `:154` 文件名 `M2Share.GetEnvirFilePath("MakeItem.txt")`；`:155` `File.Exists` 门；
//! - `:161-165` 逐行 `sLine = loadList[i].Trim()`；空行/首字符 `';'` ⇒ 跳过；
//! - `:166-174` `sLine` 以 `'['` 开头 ⇒ 分组界：若已有在攒的分组，先 `MakeItemList.Add(sItemName, list28)`；
//!   新建 `list28`；`ArrestStringEx(sLine, "[", "]", ref sItemName)` 取组名（`[...]` 内内容）——
//!   **`sItemName` 跨迭代持久**：`ArrestStringEx` 返回 `Unchanged`（无可闭合 `]`）时沿用**上一组的名字**；
//! - `:175-183` 否则（配方项行）：只在已进入某组（`list28 != null`）时收；
//!   `GetValidStr3(sLine, ref sSubName, TextSplitConst)`，`TextSplitConst = { ' ', '\t' }`（`LocalDB.cs:12`）；
//!   `nItemCount = StrToInt(sLine.Trim(), 1)`（**默认 1**）；`list28.Add({sSubName, nItemCount})`；
//! - `:185-188` 收尾：文件以配方项行结束 ⇒ 补一次 `MakeItemList.Add(sItemName, list28)`。
//!
//! **重复组名**：`MakeItemList` 是 `Dictionary<string, IList<MakeItem>>(StringComparer.OrdinalIgnoreCase)`
//! （`GameApp.cs:23`），`.Add` 在键已存在时**抛 `ArgumentException`**；`LoadMakeItem` **无 try/catch**
//! ⇒ oracle 异常穿透（`GameApp.cs:224` 的 `InitializeWorld` catch 吞掉、后续装载整段跳过）。
//! 本模块忠实建模为 [`EnvirError::MakeItemDuplicateGroup`]（`Err`），**不"顺手修"成先到保留**。
//! 消费点（**不在本片**，登记）：`M2Share.GetMakeItemInfo`（`M2Share.cs:832-840`）→
//! `Merchant.ClientMakeDrugCheckNeedItem`（`Merchant.cs:1432`），由入站 `CM_USERMAKEDRUGITEM(1034)`
//! 触发（R1 红名单，Rust 侧未实现）⇒ 本片只做「解析 + 注册表 + 宿主装载」。

use mir2_shared::hutil32::{arrest_string_ex_ref, get_valid_str3, str_to_int, ArrestWrite};

use crate::error::EnvirError;

/// `LocalDB.TextSplitConst`（`LocalDB.cs:12`）：`{ ' ', '\t' }`。
pub const MAKE_ITEM_TEXT_SPLIT: [char; 2] = [' ', '\t'];

/// 一条合成配方项（`OpenMir2/Data/MakeItem.cs:3-7` 的 `ItemName`/`ItemCount`）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MakeItem {
    pub item_name: String,
    pub item_count: i32,
}

/// `M2Share.MakeItemList`（`Dictionary<string, IList<MakeItem>>(OrdinalIgnoreCase)`）的等价物。
///
/// 保序（组按 `[...]` 出现序；组内项按行序）；查找大小写不敏感（ASCII）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct MakeItemRegistry {
    groups: Vec<(String, Vec<MakeItem>)>,
}

impl MakeItemRegistry {
    /// 组数（`MakeItemList.Count`）。
    #[must_use]
    pub fn len(&self) -> usize {
        self.groups.len()
    }

    /// 无任何组。
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.groups.is_empty()
    }

    /// `M2Share.GetMakeItemInfo(sItemName)`（`M2Share.cs:832-840`；OrdinalIgnoreCase）；
    /// 未命中 ⇒ `None`（oracle `null`）。
    #[must_use]
    pub fn get(&self, item_name: &str) -> Option<&[MakeItem]> {
        self.groups
            .iter()
            .find(|(name, _)| name.eq_ignore_ascii_case(item_name))
            .map(|(_, list)| list.as_slice())
    }

    /// 按入库顺序遍历 `(组名, 配方项)`。
    pub fn iter(&self) -> impl Iterator<Item = &(String, Vec<MakeItem>)> {
        self.groups.iter()
    }
}

/// 收一个组（`MakeItemList.Add`）：重复键（OrdinalIgnoreCase）⇒ `Err`（照搬 `.Add` 抛异常）。
fn add_group(
    groups: &mut Vec<(String, Vec<MakeItem>)>,
    name: String,
    list: Vec<MakeItem>,
) -> Result<(), EnvirError> {
    if groups.iter().any(|(n, _)| n.eq_ignore_ascii_case(&name)) {
        return Err(EnvirError::MakeItemDuplicateGroup { name });
    }
    groups.push((name, list));
    Ok(())
}

/// 整文件解析（`LoadMakeItem` 的解析半）。
///
/// # Errors
/// 组名重复（`Dictionary.Add` 抛 `ArgumentException` 的对照）。
pub fn parse_make_item_lines(file_text: &str) -> Result<MakeItemRegistry, EnvirError> {
    let mut groups: Vec<(String, Vec<MakeItem>)> = Vec::new();
    // oracle 的 `sItemName`（跨迭代持久，供 `ArrestStringEx` 的 Unchanged 分支沿用）。
    let mut s_item_name = String::new();
    // oracle 的 `list28`（`null` = 尚未进入任何组）。
    let mut list28: Option<Vec<MakeItem>> = None;

    for line in file_text.split('\n') {
        // `:161` `sLine = loadList[i].Trim()`（CRLF 行尾在此一并吸收）。
        let s_line = line.trim();
        // `:162-165` 空行 / `;` 注释。
        if s_line.is_empty() || s_line.starts_with(';') {
            continue;
        }
        if s_line.starts_with('[') {
            // `:168-171` 先把在攒的组入表。
            if let Some(list) = list28.take() {
                add_group(&mut groups, s_item_name.clone(), list)?;
            }
            // `:173` `ArrestStringEx(sLine, "[", "]", ref sItemName)`。
            match arrest_string_ex_ref(s_line, '[', ']') {
                (ArrestWrite::Value(v), _) => s_item_name = v,
                // oracle 此处 `ref sItemName` **不被改写** ⇒ 沿用上一组的名字（登记：连坑一起搬）。
                (ArrestWrite::Unchanged, _) => {}
            }
            // `:172` `list28 = new List<MakeItem>()`。
            list28 = Some(Vec::new());
        } else if let Some(list) = list28.as_mut() {
            // `:179` `GetValidStr3(sLine, ref sSubName, TextSplitConst)`。
            let (sub_name, rest) = get_valid_str3(s_line, &MAKE_ITEM_TEXT_SPLIT);
            // `:180` `nItemCount = StrToInt(sLine.Trim(), 1)`（默认 1）。
            let item_count = str_to_int(rest.trim(), 1);
            // `:181` `list28.Add(...)`。
            list.push(MakeItem {
                item_name: sub_name,
                item_count,
            });
        }
    }
    // `:185-188` 收尾。
    if let Some(list) = list28.take() {
        add_group(&mut groups, s_item_name, list)?;
    }
    Ok(MakeItemRegistry { groups })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 部署件 `E:\MirServer\M2GameSvr\Envir\MakeItem.txt` 的真实形态（GBK 解码后）：
    /// `[赤血魔剑]` 组 + 7 条 `<名>\t1` 配方项。
    #[test]
    fn parses_deployed_shape() {
        let text = "[赤血魔剑]\r\n罗刹\t1\r\n攻击神水\t1\r\n魔力神水\t1\r\n道力神水\t1\r\n疾风神水\t1\r\nHP强化水\t1\r\nMP强化水\t1";
        let reg = parse_make_item_lines(text).unwrap();
        assert_eq!(reg.len(), 1);
        let items = reg.get("赤血魔剑").expect("命中组名");
        assert_eq!(items.len(), 7);
        assert_eq!(
            items[0],
            MakeItem {
                item_name: "罗刹".to_string(),
                item_count: 1
            }
        );
        assert_eq!(items[6].item_name, "MP强化水");
        // 组名大小写不敏感（配方面为中文，此处钉住查找口径）。
        assert!(reg.get("赤血魔剑").is_some());
    }

    /// `:180` 默认数量 1：无数量 / 非数字 / 空 ⇒ 1；显式数字照取。
    #[test]
    fn item_count_defaults_to_one() {
        let reg = parse_make_item_lines("[A]\n铜\nalt\t3\nb x\nc\t\n").unwrap();
        let items = reg.get("A").unwrap();
        assert_eq!(
            items.iter().map(|m| m.item_count).collect::<Vec<_>>(),
            vec![1, 3, 1, 1],
            "无数量/非数字/空 ⇒ 默认 1；3 照取"
        );
        assert_eq!(items[1].item_name, "alt");
        assert_eq!(items[3].item_name, "c");
    }

    /// 空格与制表符都是分隔符（`TextSplitConst`）；组内保序。
    #[test]
    fn splits_on_space_and_tab() {
        let reg = parse_make_item_lines("[G]\n甲 2\n乙\t3\n").unwrap();
        let items = reg.get("G").unwrap();
        assert_eq!(
            items,
            [
                MakeItem {
                    item_name: "甲".to_string(),
                    item_count: 2
                },
                MakeItem {
                    item_name: "乙".to_string(),
                    item_count: 3
                },
            ]
        );
    }

    /// 多组 + 注释/空行 + 组名前后空白（行被 Trim）。组按出现序、保序。
    #[test]
    fn multiple_groups_and_comments_keep_order() {
        let text = ";头\r\n[A]\r\n甲 1\r\n\r\n;组内注释\r\n乙 2\r\n[B]\r\n丙 5\r\n";
        let reg = parse_make_item_lines(text).unwrap();
        assert_eq!(reg.len(), 2);
        let names: Vec<&str> = reg.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, vec!["A", "B"]);
        assert_eq!(reg.get("A").unwrap().len(), 2);
        assert_eq!(reg.get("B").unwrap()[0].item_name, "丙");
    }

    /// 组界前的裸配方项行（`list28 == null`）⇒ 丢弃（`:177` 的 `if (list28 != null)`）。
    #[test]
    fn items_before_first_group_are_dropped() {
        let reg = parse_make_item_lines("孤儿 1\r\n[A]\r\n甲 1\r\n").unwrap();
        assert_eq!(reg.len(), 1);
        assert_eq!(reg.get("A").unwrap().len(), 1);
    }

    /// 空文件 / 全注释 ⇒ 空表（无文件时 oracle 走 `File.Exists` 门 ⇒ 同样空表）。
    #[test]
    fn empty_or_comment_only_is_empty() {
        assert!(parse_make_item_lines("").unwrap().is_empty());
        assert!(parse_make_item_lines(";just a comment\r\n")
            .unwrap()
            .is_empty());
    }

    /// `Dictionary.Add` 抛异常照搬：重复组名（**大小写不敏感**）⇒ `Err`。
    ///
    /// 红检：若把 `add_group` 改成「先到保留」（像 MiniMap）⇒ 本用例拿不到 `Err` ⇒ 必红。
    #[test]
    fn duplicate_group_name_is_error_like_add() {
        let err = parse_make_item_lines("[A]\n甲 1\n[A]\n乙 2\n").unwrap_err();
        assert!(matches!(err, EnvirError::MakeItemDuplicateGroup { .. }));
        // 大小写不同也算重复（OrdinalIgnoreCase）。
        let err2 = parse_make_item_lines("[Ab]\n甲 1\n[aB]\n乙 2\n").unwrap_err();
        assert!(matches!(err2, EnvirError::MakeItemDuplicateGroup { .. }));
    }

    /// `ArrestStringEx` 拿不到闭合 `]` ⇒ `sItemName` 保持（照搬）：第二组沿用第一组名 ⇒ 重复键 ⇒ Err。
    ///
    /// 红检：若把 `Unchanged` 分支改成「置空」⇒ 第二组名 ""（不与 "A" 冲突）⇒ 本用例拿不到 `Err` ⇒ 必红。
    #[test]
    fn unclosed_bracket_reuses_previous_group_name() {
        // 第二行 `[B`（无 `]`）：`arrest_string_ex_ref` 的 findData 为真但无闭合 ⇒ Unchanged ⇒ 组名仍 "A"。
        let err = parse_make_item_lines("[A]\n甲 1\n[B\n乙 2\n").unwrap_err();
        assert!(
            matches!(&err, EnvirError::MakeItemDuplicateGroup { name } if name == "A"),
            "无闭合 ] ⇒ 沿用上一组名 A ⇒ 与首组重复 ⇒ Err（拿到的 {err:?}）"
        );
    }

    /// 查找未命中 ⇒ `None`（oracle `GetMakeItemInfo` 返 null）。
    #[test]
    fn lookup_miss_is_none() {
        let reg = parse_make_item_lines("[A]\n甲 1\n").unwrap();
        assert_eq!(reg.get("不存在"), None);
    }
}
