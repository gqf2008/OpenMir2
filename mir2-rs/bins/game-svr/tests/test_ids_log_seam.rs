//! **卡 `openmir2-ids-keepalive-timeout`**：`MIR2_LOG_IDS` 从环境到生效态的接线（独立进程）。

#[test]
fn ids_log_seam_defaults_off_and_wires_from_env() {
    assert!(
        !game_svr::test_seams::ids_log_enabled(),
        "未安装必须为关（不多打一行日志）"
    );

    std::env::set_var("MIR2_LOG_IDS", "1");
    let installed = game_svr::test_seams::install_log_ids_from_env().expect("合法值");
    assert!(installed, "返回生效值");
    assert!(game_svr::test_seams::ids_log_enabled());

    assert!(
        game_svr::test_seams::install_log_ids(Some("0")).is_err(),
        "重复安装必须报错（运行中不可改）"
    );
    assert!(game_svr::test_seams::ids_log_enabled());
}
