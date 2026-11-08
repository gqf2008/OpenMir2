//! **卡 `openmir2-test-permission-seam`**：`MIR2_TEST_PERMISSION` 从环境到会话的接线。
//!
//! 为什么放在集成测试里：缝是**进程内一次性**（`OnceLock`），只有独立进程才能既验证
//! 「未安装 ⇒ 0」又验证「安装后 ⇒ 生效」。单元测试共用进程，装了就会污染其它用例。

#[test]
fn seam_defaults_off_and_wires_into_session() {
    // ① 未安装 ⇒ oracle 锁死（默认关）
    assert_eq!(
        game_svr::gmcmd::startup_permission(),
        0,
        "未安装必须为 0（生产行为不变）"
    );

    // ② 安装 10 ⇒ 会话构造拿到 10
    std::env::set_var("MIR2_TEST_PERMISSION", "10");
    // 走生产入口（`var_os` + `to_str`），而不是自己读 `var`：这条路径才是 `run_inner` 用的。
    let installed = game_svr::gmcmd::install_test_permission_from_env().expect("合法值");
    assert_eq!(installed, 10, "安装返回值 = 生效值（供启动日志用）");
    assert_eq!(game_svr::gmcmd::startup_permission(), 10);

    let sess = game_svr::gmcmd::GmSession::from_character("", 0, false, false, false, 0, 0, 0);
    assert_eq!(sess.permission, 10, "会话构造必须吃启动期权限");

    // ③ 重复安装 ⇒ 拒绝（运行中不可改）
    assert!(
        game_svr::gmcmd::install_test_permission(Some("0")).is_err(),
        "只能安装一次"
    );
    assert_eq!(
        game_svr::gmcmd::startup_permission(),
        10,
        "重复安装不得改值"
    );
}
