//! `Envir/MonItems/*.txt` 解析，移植源 `src/GameSrv/DB/LocalDB.cs:596`
//! （`LoadMonitems`）。文件为 GBK 编码。

use std::path::Path;

use mir2_shared::hutil;

use crate::models::{MonsterDropItem, MonsterInfo};

/// C# `LocalDB.MonsterSpitConst`（`LocalDB.cs:13`）。
const MONSTER_SPIT_CONST: [char; 3] = [' ', '/', '\t'];
/// C# `LocalDB.TextSplitConst`（`LocalDB.cs:12`）。
const TEXT_SPLIT_CONST: [char; 2] = [' ', '\t'];
/// oracle `M2Share.GetEnvirFilePath("MonItems", ...)` 的固定子目录名（`M2Share.cs:1979-1986`）。
pub const MONITEMS_DIR: &str = "MonItems";

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

/// `LocalDB.LoadMonitems`（`LocalDB.cs:596`）的**存在性语义**：
///
/// - 文件不存在（或同名路径是目录）⇒ `Ok(None)`。oracle 的 `File.Exists(monFileName)`
///   为假时直接返回、**不改动调用方的 `ref itemList`**（保持 `null`）；
///   `File.Exists` 对目录同样为假，故这里用 [`Path::is_file`] 同口径。
/// - 文件存在 ⇒ `Ok(Some(解析结果))`，**解析出 0 条也给 `Some(vec![])`**
///   （oracle 会 `new List<MonsterDropItem>()`，与「不存在」的 `null` 是两种状态）。
/// - 文件存在但读取失败（权限/句柄/IO）⇒ `Err`。oracle 不吞异常：
///   `LoadFromFile` → `GetEncoding` 的 `new FileStream(fileName, FileMode.Open)` 会抛，
///   而 `LoadMonsterDB` 只有 `try/finally` 没有 `catch` ⇒ 启动期 fail-closed。
///
/// # Errors
/// `path` 是存在的文件但无法读取（权限、被独占句柄占用等）时返回 [`std::io::Error`]。
pub fn load_monitems_if_exists(path: &Path) -> std::io::Result<Option<Vec<MonsterDropItem>>> {
    if !path.is_file() {
        return Ok(None);
    }
    Ok(Some(load_monitems(path)?))
}

/// 挂接统计（启动日志 / 判据用）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AttachReport {
    /// 处理的怪物总数。
    pub total: usize,
    /// 命中 `Envir/MonItems/<name>.txt` 的怪物数（即 `item_list.is_some()`）。
    pub attached: usize,
    /// 命中的掉落条目总数（各表条目求和）。
    pub entries: usize,
}

/// C# `MySqlDB.LoadMonsterDB`（`MySqlDB.cs:226`）循环体里 `ItemList` 的挂接段，逐行移植：
///
/// ```text
/// monster.ItemList = null;
/// GameShare.LocalDb.LoadMonitems(monster.Name, ref monster.ItemList);
/// ```
///
/// 路径口径 = `M2Share.GetEnvirFilePath("MonItems", $"{monName}.txt")`
/// （`M2Share.cs:1979-1986`）= `Path.Combine(BasePath, EnvirDir, "MonItems", name + ".txt")`，
/// 所以调用方传的 `envir_dir` 是 **`Envir` 目录本身**（例如 `E:/MirServer/M2GameSvr/Envir`）。
///
/// `load_monster_db` 只读 DB、不读 Envir（见其文档），本函数是把 oracle 同一个
/// `LoadMonsterDB` 里的另一半补上；两者组合才等于 oracle 的 `LoadMonsterDB`。
///
/// # Errors
/// 某个怪物的 `MonItems/<name>.txt` 存在但读取失败时立即返回 `Err`（fail-closed，
/// 不静默把坏表当成「没有掉落」）。
pub fn attach_monitems(
    monsters: &mut [MonsterInfo],
    envir_dir: &Path,
) -> std::io::Result<AttachReport> {
    let dir = envir_dir.join(MONITEMS_DIR);
    let mut report = AttachReport {
        total: monsters.len(),
        ..AttachReport::default()
    };
    for monster in monsters.iter_mut() {
        // oracle 的次序：先 `monster.ItemList = null;`，再 `LoadMonitems(name, ref ItemList)`
        // （`LoadMonitems` 在文件不存在时**不动** itemList，保持 null）。
        // 保持同样次序的理由：**读表抛异常时 itemList 停在 null**（oracle 的异常也会把它留在 null），
        // 不会把调用方预置的旧值留在原地。
        monster.item_list = None;
        let list = load_monitems_if_exists(&dir.join(format!("{}.txt", monster.name)))?;
        if let Some(list) = &list {
            report.attached += 1;
            report.entries += list.len();
        }
        monster.item_list = list;
    }
    Ok(report)
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

    /// 测试专用临时目录（跟随仓库既有写法：`std::env::temp_dir` + 进程号，无外部依赖）。
    fn temp_envir(tag: &str) -> std::path::PathBuf {
        let dir =
            std::env::temp_dir().join(format!("mir2-monitems-attach-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(MONITEMS_DIR)).unwrap();
        dir
    }

    /// 构造一只怪（只关心名字与 `item_list`）。
    fn monster(name: &str) -> MonsterInfo {
        MonsterInfo {
            name: name.to_string(),
            ..MonsterInfo::default()
        }
    }

    /// oracle `LoadMonsterDB`（`MySqlDB.cs:226`）+ `LoadMonitems`（`LocalDB.cs:596`）
    /// 的挂接语义：缺文件 ⇒ `None`（且先置空）、空文件 ⇒ `Some(vec![])`、有表 ⇒ 内容逐字段一致。
    #[test]
    fn attach_resets_then_matches_file_existence() {
        let envir = temp_envir("attach");
        let mitems = envir.join(MONITEMS_DIR);
        std::fs::write(
            mitems.join("alpha.txt"),
            gbk("10/100 金币 300\n10/3000 海魂\n"),
        )
        .unwrap();
        // 空文件：oracle 会 new 出空 List（与「文件不存在」的 null 不同）。
        std::fs::write(mitems.join("gamma.txt"), b"").unwrap();
        // 同名目录：oracle `File.Exists` 为假 ⇒ 视同不存在。
        std::fs::create_dir_all(mitems.join("delta.txt")).unwrap();

        // 预置脏 ItemList，验证 oracle 的 `ItemList = null` 先置空语义。
        let stale = vec![MonsterDropItem {
            sel_point: 0,
            max_point: 1,
            item_name: "脏数据".to_string(),
            count: 1,
        }];
        let mut monsters = vec![
            monster("alpha"),
            monster("beta"),
            monster("gamma"),
            monster("delta"),
        ];
        monsters[0].item_list = Some(stale.clone());
        monsters[1].item_list = Some(stale.clone());
        monsters[2].item_list = Some(stale.clone());
        monsters[3].item_list = Some(stale);

        let report = attach_monitems(&mut monsters, &envir).unwrap();

        assert_eq!(
            report,
            AttachReport {
                total: 4,
                attached: 2,
                entries: 2
            }
        );
        assert_eq!(monsters[0].item_list.as_ref().unwrap().len(), 2);
        assert_eq!(monsters[0].item_list.as_ref().unwrap()[0].sel_point, 9);
        assert_eq!(monsters[0].item_list.as_ref().unwrap()[1].item_name, "海魂");
        assert_eq!(monsters[1].item_list, None, "无表 ⇒ None（不能被脏值残留）");
        assert_eq!(
            monsters[2].item_list,
            Some(Vec::new()),
            "空文件 ⇒ Some(空列表)"
        );
        assert_eq!(monsters[3].item_list, None, "同名目录 ⇒ 视同不存在");

        std::fs::remove_dir_all(&envir).unwrap();
    }

    /// 路径口径 = `envir_dir/MonItems/<name>.txt`（oracle `GetEnvirFilePath("MonItems", ...)`）。
    #[test]
    fn attach_reads_from_monitems_subdir() {
        let envir = temp_envir("subdir");
        // 故意把表放在 `envir/MonItems` 之外：不应被读到。
        std::fs::write(envir.join("outside.txt"), gbk("10/100 金币 1\n")).unwrap();
        let mut monsters = vec![monster("outside")];
        let report = attach_monitems(&mut monsters, &envir).unwrap();
        assert_eq!(report.attached, 0);
        assert_eq!(monsters[0].item_list, None);
        // 放对位置后应命中。
        std::fs::write(
            envir.join(MONITEMS_DIR).join("outside.txt"),
            gbk("10/100 金币 1\n"),
        )
        .unwrap();
        let report = attach_monitems(&mut monsters, &envir).unwrap();
        assert_eq!(
            report,
            AttachReport {
                total: 1,
                attached: 1,
                entries: 1
            }
        );
        std::fs::remove_dir_all(&envir).unwrap();
    }

    /// 存在但读不了的坏表 ⇒ `Err`（fail-closed），不静默当「无掉落」。
    ///
    /// Windows 上用一个 `share_mode(0)`（FILE_SHARE_NONE）的独占句柄制造确定性 IO 错误；
    /// 非 Windows 平台无等价保障，跳过（本项目运行环境为 Windows）。
    #[cfg(windows)]
    #[test]
    fn attach_fails_closed_on_unreadable_file() {
        use std::os::windows::fs::OpenOptionsExt;

        let envir = temp_envir("badfile");
        let path = envir.join(MONITEMS_DIR).join("locked.txt");
        std::fs::write(&path, gbk("10/100 金币 1\n")).unwrap();
        // 0 = 不共享读/写/删 ⇒ 任何后继 open 得 `ERROR_SHARING_VIOLATION`。
        let held = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&path)
            .unwrap();

        let mut monsters = vec![monster("locked")];
        let err = attach_monitems(&mut monsters, &envir).unwrap_err();
        // 必须是**真的 IO 错误**（Windows 共享冲突 → OS 错误码），不是被静默成 None。
        assert!(
            err.raw_os_error().is_some(),
            "期望 OS 级 IO 错误，得 {err:?}"
        );
        // 非静默：坏表时 item_list 停在被置空的 None，绝不伪造成 `Some(空)`。
        assert_eq!(monsters[0].item_list, None);

        drop(held);
        std::fs::remove_dir_all(&envir).unwrap();
    }
}
