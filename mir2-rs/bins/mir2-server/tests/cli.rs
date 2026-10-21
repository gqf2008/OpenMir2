//! `mir2-server` 的进程级判据：
//! 1. `--list-roles` 恰六行、退出 0；
//! 2. 未知/缺省角色 ⇒ 用法进 stderr、退出 **2**；
//! 3. argv 透传：`mir2-server <role> <args...>` 与旧 exe 同参数的行为（退出码 + stderr）逐字一致；
//! 4. 六个旧 exe 的 `main.rs` 只是调各自 lib `run()` 的薄包装（源码检查）。

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use mir2_server::ROLES;

/// 旧 exe 与 `mir2-server` 都落在同一个 `target/<profile>` 目录。
fn exe_dir() -> PathBuf {
    let test_exe = std::env::current_exe().expect("current_exe");
    test_exe
        .parent()
        .and_then(Path::parent)
        .expect("target/<profile>")
        .to_path_buf()
}

fn sibling_exe(name: &str) -> PathBuf {
    exe_dir().join(format!("{name}{}", std::env::consts::EXE_SUFFIX))
}

fn run(exe: &Path, args: &[&str]) -> Output {
    Command::new(exe)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("启动 {} 失败：{e}", exe.display()))
}

#[test]
fn list_roles_prints_exactly_six_lines() {
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_mir2-server")),
        &["--list-roles"],
    );
    assert_eq!(out.status.code(), Some(0));
    let text = String::from_utf8(out.stdout).expect("角色名是 ASCII");
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        vec![
            "game-svr",
            "db-srv",
            "login-srv",
            "game-gate",
            "login-gate",
            "sel-gate"
        ]
    );
    assert_eq!(text.lines().count(), 6);
    assert!(out.stderr.is_empty(), "stderr 不该有内容");
}

#[test]
fn unknown_role_exits_2_with_usage() {
    let out = run(
        Path::new(env!("CARGO_BIN_EXE_mir2-server")),
        &["no-such-role"],
    );
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("未知角色：no-such-role"), "stderr: {err}");
    assert!(err.contains("用法："), "stderr: {err}");
    for role in ROLES {
        assert!(err.contains(role), "用法缺角色 {role}");
    }
}

#[test]
fn missing_role_exits_2() {
    let out = run(Path::new(env!("CARGO_BIN_EXE_mir2-server")), &[]);
    assert_eq!(out.status.code(), Some(2));
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("用法："), "stderr: {err}");
}

/// argv 透传：旧 `db-srv.exe <不存在的配置>` 与 `mir2-server db-srv <同一个配置>`
/// 必须**同退出码、同 stderr**（配置读取失败在任何端口/DB 动作之前发生）。
#[test]
fn role_argv_is_passed_through_verbatim() {
    // 不存在的配置路径：配置读取失败发生在绑定端口/连库之前。
    let missing = std::env::temp_dir()
        .join("mir2-single-binary-argv-probe-missing.conf")
        .display()
        .to_string();
    let new = run(
        Path::new(env!("CARGO_BIN_EXE_mir2-server")),
        &["db-srv", missing.as_str()],
    );
    assert_eq!(
        new.status.code(),
        Some(1),
        "缺配置应退出 1（与旧 exe 同码）"
    );
    let stderr = String::from_utf8_lossy(&new.stderr);
    // 若角色名没被剥掉，`env::args().nth(1)` 会拿到 "db-srv" 而不是这个路径。
    assert!(
        stderr.contains(missing.as_str()),
        "角色必须收到透传的配置文件路径（stderr: {stderr}）"
    );
    // 旧 exe 在同一 `target/<profile>` 目录时，逐字对照退出码 + stderr。
    let old_exe = sibling_exe("db-srv");
    if old_exe.exists() {
        let old = run(&old_exe, &[missing.as_str()]);
        assert_eq!(old.status.code(), Some(1), "旧 exe 应因缺配置退出 1");
        assert_eq!(
            String::from_utf8_lossy(&old.stderr),
            stderr,
            "stderr 必须与旧 exe 逐字一致"
        );
    }
}

const LEGACY_BINS: [(&str, &str); 6] = [
    ("game-svr", "game_svr"),
    ("db-srv", "db_srv"),
    ("login-srv", "login_srv"),
    ("game-gate", "game_gate"),
    ("login-gate", "login_gate"),
    ("sel-gate", "sel_gate"),
];

/// 旧六个 exe 的 `main.rs` 只能是三行薄包装：`fn main() { std::process::exit(<crate>::run()); }`。
///
/// 牙：把任一 `main.rs` 改回内联体（搬回启动逻辑）⇒ 本用例红。
#[test]
fn legacy_bins_are_thin_wrappers_around_lib_run() {
    let bins = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("bins/")
        .to_path_buf();
    for (pkg, krate) in LEGACY_BINS {
        let path = bins.join(pkg).join("src").join("main.rs");
        let src = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("读不到 {}：{e}", path.display()));
        let body: Vec<&str> = src
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .collect();
        let call = format!("std::process::exit({krate}::run());");
        assert_eq!(
            body,
            vec!["fn main() {", call.as_str(), "}"],
            "{} 的 main 必须只调 lib 的 run()（薄包装）",
            path.display()
        );
    }
}
