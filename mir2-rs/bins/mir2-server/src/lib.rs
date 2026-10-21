//! `mir2-server`：**单二进制、多角色**入口。
//!
//! 六个角色仍然是**各自独立的进程**（端口、日志前缀、配置读取、启动参数口径一字不改），
//! 只是共用同一份可执行文件：`mir2-server <role> [role argv...]` ⇒ 分派到对应角色 crate 的
//! [`run_with_args`]。旧六个 exe 保留为薄包装（`main` 只调各自 lib 的 `run()`），
//! 两者的 argv 语义都是"程序名之后的参数"，完全一致。
//!
//! 角色名固定为 [`ROLES`]；未知/缺省角色打印用法到 stderr 并以 **2** 退出。

use std::io::Write as _;

/// 六个角色名（顺序即 `--list-roles` 的输出顺序）。
pub const ROLES: [&str; 6] = [
    "game-svr",
    "db-srv",
    "login-srv",
    "game-gate",
    "login-gate",
    "sel-gate",
];

/// 角色入口签名：参数是**角色名之后**的剩余 argv；返回进程退出码。
pub type RoleEntry = fn(&[String]) -> i32;

/// 角色分派表（纯函数）：已知角色返回其入口，未知角色返回 `None`。
///
/// 入口签名 `fn(&[String]) -> i32`：参数是**角色名之后**的剩余 argv，
/// 与旧二进制的 `env::args().skip(1)` 等长同序；返回进程退出码。
#[must_use]
pub fn role_runner(role: &str) -> Option<RoleEntry> {
    match role {
        "game-svr" => Some(game_svr::run_with_args),
        "db-srv" => Some(db_srv::run_with_args),
        "login-srv" => Some(login_srv::run_with_args),
        "game-gate" => Some(game_gate::run_with_args),
        "login-gate" => Some(login_gate::run_with_args),
        "sel-gate" => Some(sel_gate::run_with_args),
        _ => None,
    }
}

/// 用法文本（未知/缺省角色时写到 stderr）。
#[must_use]
pub fn usage(program: &str) -> String {
    format!(
        "用法：{program} <role> [args...]\n角色：{}\n查看角色列表：{program} --list-roles",
        ROLES.join("|")
    )
}

/// 分派 argv（**不含程序名**）。
///
/// 返回进程退出码：`0` = 角色正常结束或 `--list-roles`；`2` = 未知/缺省角色；
/// 其余为角色自身的退出码（原样返回，不吞不改）。
#[must_use]
pub fn run_main(program: &str, args: &[String]) -> i32 {
    let Some((role, rest)) = args.split_first() else {
        let _ = writeln!(std::io::stderr(), "{}", usage(program));
        return 2;
    };
    if role == "--list-roles" {
        let mut out = std::io::stdout().lock();
        for name in ROLES {
            let _ = writeln!(out, "{name}");
        }
        return 0;
    }
    match role_runner(role) {
        Some(run) => run(rest),
        None => {
            let _ = writeln!(std::io::stderr(), "未知角色：{role}\n{}", usage(program));
            2
        }
    }
}

/// 进程入口：取 `env::args()`（跳过程序名）、分派、刷新输出后返回退出码。
///
/// `main` 只做 `std::process::exit(<本函数>())`；这里显式刷新 stdout/stderr，
/// 因为 `process::exit` 不跑运行期退出清理。
#[must_use]
pub fn run() -> i32 {
    let argv: Vec<String> = std::env::args().collect();
    let program = argv
        .first()
        .map_or_else(|| "mir2-server".to_string(), Clone::clone);
    let args = argv.get(1..).unwrap_or(&[]);
    let code = run_main(&program, args);
    let _ = std::io::stdout().flush();
    let _ = std::io::stderr().flush();
    code
}

#[cfg(test)]
mod tests {
    use super::{role_runner, usage, RoleEntry, ROLES};

    /// 六个角色都能分派到入口，未知角色返回 `None`。
    #[test]
    fn role_table_covers_exactly_six_roles() {
        assert_eq!(ROLES.len(), 6);
        for role in ROLES {
            assert!(role_runner(role).is_some(), "缺少角色 {role}");
        }
        assert!(role_runner("").is_none());
        assert!(role_runner("GameSvr").is_none());
        assert!(role_runner("--list-roles").is_none());
    }

    /// 角色名唯一（`--list-roles` 恰好六行）。
    #[test]
    fn role_names_are_unique() {
        let mut seen: Vec<&str> = ROLES.to_vec();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), ROLES.len());
    }

    /// 分派映射逐条钉死：改错/改漏一个臂，本用例即红。
    #[test]
    fn role_table_maps_each_role_to_its_own_entry() {
        fn addr(f: RoleEntry) -> RoleEntry {
            f
        }
        let expected: [(&str, RoleEntry); 6] = [
            ("game-svr", addr(game_svr::run_with_args)),
            ("db-srv", addr(db_srv::run_with_args)),
            ("login-srv", addr(login_srv::run_with_args)),
            ("game-gate", addr(game_gate::run_with_args)),
            ("login-gate", addr(login_gate::run_with_args)),
            ("sel-gate", addr(sel_gate::run_with_args)),
        ];
        for (role, want) in expected {
            let got = role_runner(role).expect("角色必须在表里");
            assert!(
                std::ptr::fn_addr_eq(got, want),
                "角色 {role} 分派到了错误的入口"
            );
        }
    }

    /// 用法文本列出全部六个角色（stderr 提示可机判）。
    #[test]
    fn usage_lists_all_roles() {
        let text = usage("mir2-server");
        for role in ROLES {
            assert!(text.contains(role), "用法缺角色 {role}");
        }
    }

    /// 旧六个 exe 的 `run()` 必须存在且签名不变（编译期即证明，bin 侧只调它）。
    #[test]
    fn legacy_runs_have_stable_signature() {
        let runs: [fn() -> i32; 6] = [
            game_svr::run,
            db_srv::run,
            login_srv::run,
            game_gate::run,
            login_gate::run,
            sel_gate::run,
        ];
        assert_eq!(runs.len(), ROLES.len());
    }
}
