fn main() {
    // 卡 `openmir2-rust-build-rev-embedding`：`--version`/`-V` 一行机读自报构建身份，
    // 由 `build.rs`（`mir2-build-meta`）注入；无 git 元数据时是 `unknown`，不是空串。
    if std::env::args().any(|a| a == "--version" || a == "-V") {
        // 能力标记（部署闸门据此判断该件支持自报 rev；老件/夹具没有此串 ⇒ 跳过而不误判）。
        std::hint::black_box("MIR2_BUILD_SELF_REV=1");
        println!(
            "rev={} dirty={} built_at={}",
            option_env!("MIR2_BUILD_REV").unwrap_or("unknown"),
            option_env!("MIR2_BUILD_DIRTY").unwrap_or("unknown"),
            option_env!("MIR2_BUILD_AT").unwrap_or("unknown"),
        );
        std::process::exit(0);
    }
    std::process::exit(login_gate::run());
}
