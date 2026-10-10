//! 打印常见脚本命令的派发落点（诊断用：
//! `cargo test -p mir2-script --test dispatch_map -- --nocapture`）。

use mir2_script::codes::{CONDITION_CODES, EXECUTION_CODES};
use mir2_script::engine::dispatch;

#[test]
fn print_dispatch_map() {
    println!("条件命令（脚本名 → 字段序号 → CmdCode → 落点）:");
    for d in CONDITION_CODES {
        // 特判命令（CHECK/CHECKOPEN/CHECKUNIT）保留字段序号；其余 -1（与解析器一致）
        let code = if matches!(d.member, "CHECK" | "CHECKOPEN" | "CHECKUNIT") {
            d.field_index
        } else {
            d.field_index - 1
        };
        let hit = match dispatch::condition_status(code) {
            Some(true) => format!("handler:{}", dispatch::condition_handler(code).unwrap()),
            Some(false) => format!(
                "handler(未实现):{}",
                dispatch::condition_handler(code).unwrap()
            ),
            None => match dispatch::engine_switch_condition(code) {
                Some(c) => format!("switch:{c}"),
                None => "ignored".into(),
            },
        };
        println!(
            "  {:<26} {:>4} → {:>4} → {hit}",
            d.name, d.field_index, code
        );
    }
    println!("动作命令:");
    for d in EXECUTION_CODES {
        let code = if matches!(
            d.member,
            "Set" | "ReSet" | "SetOpen" | "SetUnit" | "ResetUnit"
        ) {
            d.field_index
        } else {
            d.field_index - 1
        };
        let hit = match dispatch::execution_handler(code) {
            Some(h) if dispatch::execution_implemented(code) => format!("handler:{h}"),
            Some(h) => format!("handler(未实现):{h}"),
            None => match dispatch::engine_switch_execution(code) {
                Some(c) => format!("switch:{c}"),
                None => "ignored".into(),
            },
        };
        println!(
            "  {:<26} {:>4} → {:>4} → {hit}",
            d.name, d.field_index, code
        );
    }
}
