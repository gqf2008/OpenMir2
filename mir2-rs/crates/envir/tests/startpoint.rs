//! `Envir/StartPoint.txt` 真机部署件判据（**刻意 `#[ignore]`，不进常驻测试**）：读仓外 `E:\MirServer`。
//!
//! 跑法：
//!
//! ```text
//! $env:MIR2_ENVIR_DIR='E:\MirServer\M2GameSvr\Envir'
//! cargo test -p mir2-envir --test startpoint -- --ignored --nocapture
//! ```
//!
//! 部署件事实（2026-10-20 只读实测，GBK + CRLF）：
//! - 10 行 = 1 行注释 + 8 条数据 + 1 空行；
//! - 8 条数据**全部 3 列**（`MapName X Y`）⇒ 其余 5 列走默认：`NotAllowSay=false`、
//!   `Range/Type/PkZone/PkFire = 0`；
//! - 首两条 `0 289 618` / `0 650 631` 正是 `StartPointSize=2` 的抽样池
//!   （`WorldServer.cs:1409-1417`）。

use std::path::PathBuf;

use mir2_envir::{load_start_points_if_exists, StartPoint};

fn envir_dir() -> PathBuf {
    std::env::var("MIR2_ENVIR_DIR").map_or_else(
        |_| PathBuf::from(r"E:\MirServer\M2GameSvr\Envir"),
        PathBuf::from,
    )
}

/// 真机 `StartPoint.txt`：8 条、8 个字段逐个断言（后 5 列必须显式为默认值）。
#[test]
#[ignore = "读真机 E:\\MirServer（仓外）；MIR2_ENVIR_DIR 显式跑"]
fn deployment_start_point_file_matches_recorded_facts() {
    let path = envir_dir().join("StartPoint.txt");
    assert!(path.is_file(), "真机 Envir 目录不存在：{}", path.display());
    let list = load_start_points_if_exists(&path)
        .expect("读取 StartPoint.txt")
        .expect("文件存在 ⇒ Some");
    assert_eq!(list.len(), 8, "部署件 8 条数据行");
    assert_eq!(
        list[0],
        StartPoint {
            map_name: "0".to_string(),
            x: 289,
            y: 618,
            not_allow_say: false,
            range: 0,
            r#type: 0,
            pk_zone: 0,
            pk_fire: 0,
            shape: 0,
        }
    );
    assert_eq!(
        (list[1].map_name.as_str(), list[1].x, list[1].y),
        ("0", 650, 631)
    );
    // 后 7 条同样只有 3 列 ⇒ 五列全默认（逐条断言，防「只测首行」的假绿）。
    for (i, p) in list.iter().enumerate().skip(2) {
        assert!(!p.not_allow_say, "第 {i} 条 NotAllowSay 应为 false");
        assert_eq!(
            (p.range, p.r#type, p.pk_zone, p.pk_fire),
            (0, 0, 0, 0),
            "第 {i} 条五列应为默认值"
        );
        assert_eq!(p.shape, 0, "Shape 恒 0（oracle 无赋值点）");
    }
}
