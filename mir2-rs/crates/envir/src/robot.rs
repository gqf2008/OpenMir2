//! `Envir/Robot.txt` 机器人配置解析 —— 1:1 复刻 `src/GameSrv/Robots/RobotManage.cs:21-49`（`LoadRobot`）。
//!
//! 交付卡：`openmir2-ctor-draws-fold`（构造段抽数 fold；范围裁决见卡 `openmir2-npc-ctor-draws` 的 comment）。
//!
//! oracle 行为逐条（行号按 `RobotManage.cs`）：
//! - `:23` 路径 = `M2Share.GetEnvirFilePath("Robot.txt")`（**单参数重载** `M2Share.cs:1970-1977`
//!   = `Path.Combine(BasePath, EnvirDir, "Robot.txt")`，即宿主 `envir_file_path(base, envir_dir, "", …)`）；
//! - `:24-27` **`File.Exists` 门**：文件不存在 ⇒ 直接 `return` ⇒ **0 个对象、0 抽数**；
//! - `:31-33` 行过滤：空行与 `;` 注释行跳过（`;` 判据看**原始行首字符**，未 trim）；
//! - `:35-36` 两次 `GetValidStr3(row, ref x, { ' ', '/', '\t' })` ⇒ 机器人名 + 脚本文件名；
//! - `:37-40` **入库门**：两者都非空才 `new RobotObject()`（`RobotObject.cs:13`）——
//!   只有 1 个 token 的行**不算有效行**；
//! - `:41-44` `ChrName`/`ScriptFileName` 赋值 + `LoadScript()`（`RobotObject.cs:202-…`
//!   **实读不抽 RNG**：只读 `Robot_def/{ScriptFileName}.txt` 建 `AutoRunInfo` 列表）；
//! - `:48` `new RobotObject()` 走 `RobotObject : PlayObject : CharacterObject : AnimalObject : BaseObject`
//!   的 ctor 公共链 —— **每行恰抽 4 个数**（`BaseObject.cs:439/441` + `AnimalObject.cs:76/77`；
//!   `RobotObject.cs:13-18`/`CharacterObject.cs:81-89`/`PlayObject.cs:870-…` 三个 ctor 体**自身不抽**）。
//!
//! ## 建模差异（登记）
//!
//! 本模块只做**解析**（行过滤 + 两列分词），产出记录供宿主**计数**用；`RobotObject` **世界实体**
//! **未建模**（无 `RobotCharacter`/`AutoRun` 驱动器）⇒ 宿主按 **fold** 口径处理：**只按行数补抽
//! 4×行数、不建实体**（见 `crates/world/src/ctor_fold.rs` 的 `CtorFoldSite::RobotObject`）。
//! 实体缺口另见 `tests/parity/m3/design-buff-and-tick-20261016.md` §4.4.2 与 whitelist 登记。

use mir2_shared::hutil32::get_valid_str3;

/// 一行有效机器人配置（`RobotManage.cs:35-40` 的两列）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RobotRecord {
    /// `ChrName`（`RobotObject.cs:42`；oracle `sRobotName`）。
    pub name: String,
    /// `ScriptFileName`（`RobotObject.cs:43`；oracle `sScriptFileName`，对应 `Robot_def/{x}.txt`）。
    pub script_file_name: String,
}

/// `Robot.txt` 的分词分隔符（`RobotManage.cs:35-36` 的 `new[] { ' ', '/', '\t' }`）。
pub const ROBOT_TEXT_SPLIT: [char; 3] = [' ', '/', '\t'];

/// 解析一行（`RobotManage.cs:31-40`）。空行 / `;` 注释行 / 只有 1 列 ⇒ `None`（= oracle `continue`）。
#[must_use]
pub fn parse_robot_line(line: &str) -> Option<RobotRecord> {
    // `:31-33` `if (string.IsNullOrEmpty(sLineText) || sLineText[0] == ';') continue;`
    if line.is_empty() || line.starts_with(';') {
        return None;
    }
    // `:35-36` 两次 `GetValidStr3`（先名前脚本名，顺序不可换）。
    let (name, rest) = get_valid_str3(line, &ROBOT_TEXT_SPLIT);
    let (script_file_name, _rest) = get_valid_str3(&rest, &ROBOT_TEXT_SPLIT);
    // `:37-40` 入库门：两列都非空。
    if name.is_empty() || script_file_name.is_empty() {
        return None;
    }
    Some(RobotRecord {
        name,
        script_file_name,
    })
}

/// 整文件解析（`RobotManage.cs:29-49` 的循环半）。
///
/// 行尾 CR 吸收（C# `StringList.LoadFromFile` 语义，与 `guardlist::load_guard_list` 同款）。
/// **调用方负责 `File.Exists` 门**（文件不存在 = 0 记录，见 [`parse_robot_line`] 的模块头）。
#[must_use]
pub fn parse_robot_lines(file_text: &str) -> Vec<RobotRecord> {
    let mut out = Vec::new();
    for line in file_text.split('\n') {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if let Some(rec) = parse_robot_line(line) {
            out.push(rec);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **部署件逐字节复刻**：`E:/MirServer/M2GameSvr/Envir/Robot.txt`（43 字节，GBK）实读为
    /// 「`;机器人名称 脚本名称` + CRLF + `系统控制 AutoRunRobot`」 —— 第 1 行是注释
    /// （**不是有效行**）、第 2 行才是有效行 ⇒ **有效行数 = 1** ⇒ 构造账本头 **4 抽**（不是 8）。
    #[test]
    fn deployed_file_has_exactly_one_valid_line() {
        let text = ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot";
        let recs = parse_robot_lines(text);
        assert_eq!(recs.len(), 1, "注释行不入册 ⇒ 有效行数 = 1");
        assert_eq!(recs[0].name, "系统控制");
        assert_eq!(recs[0].script_file_name, "AutoRunRobot");
    }

    /// **行数 × 4（不是常量 4）**：加一行有效行 ⇒ 记录数 +1（宿主据此多抽 4 个）。
    #[test]
    fn extra_valid_line_adds_one_record() {
        let base = ";机器人名称 脚本名称\r\n系统控制 AutoRunRobot\r\n";
        let one = parse_robot_lines(base);
        let two = parse_robot_lines(&format!("{base}机器人2 AutoRunRobot2\r\n"));
        assert_eq!(one.len(), 1);
        assert_eq!(two.len(), 2, "有效行数 = 行数，不是常量");
    }

    /// **只有 1 列不算有效行**（`RobotManage.cs:37-40` 的两列非空门）。
    #[test]
    fn single_token_line_is_not_valid() {
        assert_eq!(parse_robot_line("机器人1"), None);
        assert_eq!(parse_robot_line("机器人1 "), None, "尾随空格不产生第二列");
        assert_eq!(parse_robot_line(""), None);
        assert_eq!(parse_robot_line(";注释 行"), None);
    }

    /// 三个分隔符（`' '`/`'/'`/`'\t'`）都算分隔；连排分隔符只切一次（`GetValidStr3` 语义）。
    #[test]
    fn splits_on_space_slash_tab() {
        assert_eq!(
            parse_robot_line("机器人1/测试脚本"),
            Some(RobotRecord {
                name: "机器人1".to_string(),
                script_file_name: "测试脚本".to_string()
            })
        );
        assert_eq!(
            parse_robot_line("\t机器人1\t测试脚本\t"),
            Some(RobotRecord {
                name: "机器人1".to_string(),
                script_file_name: "测试脚本".to_string()
            })
        );
    }
}
