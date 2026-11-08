//! **卡 `openmir2-test-permission-seam`**：缝**默认关**——环境变量未设时权限恒 0。
//!
//! 独立进程（`OnceLock` 一次性）：与 `test_permission_seam.rs`（设 10）互不干扰。

#[test]
fn seam_off_by_default_keeps_oracle_zero_permission() {
    std::env::remove_var("MIR2_TEST_PERMISSION");
    let installed = game_svr::gmcmd::install_test_permission_from_env().expect("未设是合法输入");
    assert_eq!(installed, 0, "未设 ⇒ 0");
    assert_eq!(game_svr::gmcmd::startup_permission(), 0, "oracle 锁死");

    let sess = game_svr::gmcmd::GmSession::from_character("", 0, false, false, false, 0, 0, 0);
    assert_eq!(sess.permission, 0, "会话构造同样是 0");
}
