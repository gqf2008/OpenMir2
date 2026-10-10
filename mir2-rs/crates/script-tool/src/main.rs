//! `mir2-script-tool`：
//! - `gen-codes`：从 C# 参照源码生成 `crates/script/src/codes.rs`
//! - `parse-stats <Envir目录>`：全量加载脚本，输出统计/错误/未实现命令清单（JSON + 摘要）

use mir2_script_tool::flowrun;
use mir2_script_tool::gbkoverrides;
use mir2_script_tool::gencodes;
use mir2_script_tool::parsestats;

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("用法:");
        eprintln!("  script-tool gen-codes [--consts-dir <dir>] [--out <file>] [--check]");
        eprintln!("  script-tool parse-stats <Envir目录> [--json <out.json>]");
        return ExitCode::from(2);
    }
    let tool_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mir2rs_root = tool_dir.join("../.."); // mir2-rs/
    let repo_root = mir2rs_root.join(".."); // 仓库根
    match args[1].as_str() {
        "gen-codes" => {
            let mut consts_dir = repo_root.join("src/Modules/ScriptEngine/Consts");
            let mut out = mir2rs_root.join("crates/script/src/codes.rs");
            let mut check = false;
            let mut i = 2;
            while i < args.len() {
                match args[i].as_str() {
                    "--consts-dir" => {
                        consts_dir = PathBuf::from(&args[i + 1]);
                        i += 2;
                    }
                    "--out" => {
                        out = PathBuf::from(&args[i + 1]);
                        i += 2;
                    }
                    "--check" => {
                        check = true;
                        i += 1;
                    }
                    other => {
                        eprintln!("未知参数: {other}");
                        return ExitCode::from(2);
                    }
                }
            }
            let report = match gencodes::generate(&consts_dir) {
                Ok(r) => r,
                Err(e) => {
                    eprintln!("gen-codes 失败: {e}");
                    return ExitCode::FAILURE;
                }
            };
            for w in &report.warnings {
                eprintln!("警告: {w}");
            }
            let tables = match gencodes::generate_handlers(
                &repo_root.join("src/Modules/ScriptEngine"),
                &report.condition,
                &report.execution,
            ) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("gen-codes 失败（派发表）: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let handlers_text = gencodes::emit_handlers_rust(&tables);
            let handlers_out = mir2rs_root.join("crates/script/src/handlers.rs");
            let text = gencodes::emit_rust(&report);
            if check {
                let handlers_ok = std::fs::read_to_string(&handlers_out)
                    .map(|cur| cur == handlers_text)
                    .unwrap_or(false);
                if !handlers_ok {
                    eprintln!(
                        "gen-codes --check: handlers.rs 不一致，请重跑 gen-codes（{}）",
                        handlers_out.display()
                    );
                    return ExitCode::FAILURE;
                }
                return match std::fs::read_to_string(&out) {
                    Ok(cur) if cur == text => {
                        println!("gen-codes --check: 一致（{}）", out.display());
                        ExitCode::SUCCESS
                    }
                    _ => {
                        eprintln!(
                            "gen-codes --check: 不一致，请重跑 gen-codes（{}）",
                            out.display()
                        );
                        ExitCode::FAILURE
                    }
                };
            }
            if let Err(e) = std::fs::write(&handlers_out, &handlers_text) {
                eprintln!("写入 {} 失败: {e}", handlers_out.display());
                return ExitCode::FAILURE;
            }
            match std::fs::write(&out, &text) {
                Ok(()) => {
                    println!(
                        "生成派发表 {}: 条件处理器 {} / 动作处理器 {} / switch case {} + {}",
                        handlers_out.display(),
                        tables.condition_handlers.len(),
                        tables.execution_handlers.len(),
                        tables.engine_switch_conditions.len(),
                        tables.engine_switch_executions.len()
                    );
                    println!(
                        "生成 {}: 条件 {} 条 / 动作 {} 条 / 全局变量 {} 对",
                        out.display(),
                        report.condition.len(),
                        report.execution.len(),
                        report
                            .grobal_consts
                            .iter()
                            .filter(|(n, _)| n.starts_with("sVAR_"))
                            .count()
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("写入 {} 失败: {e}", out.display());
                    ExitCode::FAILURE
                }
            }
        }
        "gen-gbk-overrides" => {
            let probe = match args.iter().position(|a| a == "--probe") {
                Some(i) => PathBuf::from(&args[i + 1]),
                None => mir2rs_root.join("target/cp936-pairs.txt"),
            };
            let out = match args.iter().position(|a| a == "--out") {
                Some(i) => PathBuf::from(&args[i + 1]),
                None => mir2rs_root.join("crates/script/src/gbk_overrides.rs"),
            };
            let table = match gbkoverrides::parse_probe(&probe) {
                Ok(t) => t,
                Err(e) => {
                    eprintln!("gen-gbk-overrides 失败: {e}");
                    return ExitCode::FAILURE;
                }
            };
            let overrides = gbkoverrides::generate_overrides(&table);
            match std::fs::write(&out, gbkoverrides::emit_rust(&overrides)) {
                Ok(()) => {
                    println!("生成 {}: {} 条覆盖", out.display(), overrides.len());
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("写入失败: {e}");
                    ExitCode::FAILURE
                }
            }
        }
        "flow-run" => flowrun::run(&args),
        "parse-stats" => {
            if args.len() < 3 {
                eprintln!("parse-stats 需要 Envir 目录");
                return ExitCode::from(2);
            }
            let envir = PathBuf::from(&args[2]);
            let mut json_out: Option<PathBuf> = None;
            let mut i = 3;
            while i < args.len() {
                match args[i].as_str() {
                    "--json" => {
                        json_out = Some(PathBuf::from(&args[i + 1]));
                        i += 2;
                    }
                    other => {
                        eprintln!("未知参数: {other}");
                        return ExitCode::from(2);
                    }
                }
            }
            parsestats::run(&envir, json_out)
        }
        other => {
            eprintln!("未知子命令: {other}");
            ExitCode::from(2)
        }
    }
}
