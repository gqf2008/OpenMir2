//! 漂移门禁：`codes.rs` 必须由 C# 参照源码机械生成且未漂移。
//! 红检方式：手改 `codes.rs` 或 C# 侧枚举/常量 → 本测试必须红。

use std::path::PathBuf;

use mir2_script_tool::gencodes;

#[test]
fn codes_rs_matches_csharp_source() {
    let tool_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = tool_dir.join("../../.."); // mir2-rs/crates/script-tool → 仓库根
    let consts_dir = repo_root.join("src/Modules/ScriptEngine/Consts");
    let codes_rs = tool_dir.join("../script/src/codes.rs");
    let report = gencodes::generate(&consts_dir).expect("生成码表失败");
    let expected = gencodes::emit_rust(&report);
    let actual = std::fs::read_to_string(&codes_rs).expect("读取 codes.rs 失败");
    assert!(
        actual == expected,
        "codes.rs 与 C# 参照源码不一致；重跑: cargo run -p mir2-script-tool -- gen-codes"
    );
    // 码表规模锚定（枚举增减必须显式感知）
    assert_eq!(report.condition.len(), 207, "ConditionCode 条数变化");
    assert_eq!(report.execution.len(), 352, "ExecutionCode 条数变化");
}

/// 字段序号语义锚定：GetFields() 下 `value__` 占 0 号位，成员序号 = 声明序 + 1，
/// 且本仓库两个枚举的字段序号与枚举值一致（首个成员显式从 1 开始、无中途显式赋值）。
#[test]
fn field_index_equals_enum_value_anchor() {
    let tool_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = tool_dir.join("../../..");
    let consts_dir = repo_root.join("src/Modules/ScriptEngine/Consts");
    let report = gencodes::generate(&consts_dir).unwrap();
    for d in report.condition.iter().chain(report.execution.iter()) {
        assert_eq!(
            d.field_index, d.enum_value,
            "{} 的字段序号与枚举值不再一致（C# 侧可能出现显式赋值）",
            d.member
        );
    }
    // 已知锚点：CHECK=1 / Set=1；解析器对普通命令存 field_index-1
    let check = report
        .condition
        .iter()
        .find(|d| d.member == "CHECK")
        .unwrap();
    assert_eq!((check.field_index, check.enum_value), (1, 1));
    let set = report.execution.iter().find(|d| d.member == "Set").unwrap();
    assert_eq!((set.field_index, set.enum_value), (1, 1));
}

/// 派发表漂移门禁：`handlers.rs` 必须与 C# 参照源码机械生成结果一致。
/// 红检方式：改 `handlers.rs` 任一行或改 C# 侧注册/switch → 本测试必红。
#[test]
fn handlers_rs_matches_csharp_source() {
    let tool_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = tool_dir.join("../../..");
    let script_engine = repo_root.join("src/Modules/ScriptEngine");
    let handlers_rs = tool_dir.join("../script/src/handlers.rs");
    let report = gencodes::generate(&script_engine.join("Consts")).expect("生成码表失败");
    let tables = gencodes::generate_handlers(&script_engine, &report.condition, &report.execution)
        .expect("生成派发表失败");
    let expected = gencodes::emit_handlers_rust(&tables);
    let actual = std::fs::read_to_string(&handlers_rs).expect("读取 handlers.rs 失败");
    assert!(
        actual == expected,
        "handlers.rs 与 C# 参照源码不一致；重跑: cargo run -p mir2-script-tool -- gen-codes"
    );
    // 规模锚定（与反射 dump 一致：conditionMap 111 / ProcessExecutionMessage 133）
    assert_eq!(tables.condition_handlers.len(), 111);
    assert_eq!(tables.execution_handlers.len(), 133);
}
