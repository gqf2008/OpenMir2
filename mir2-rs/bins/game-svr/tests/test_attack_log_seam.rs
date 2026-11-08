//! **卡 `openmir2-test-attack-log`**：`MIR2_LOG_ATTACKS` 从环境到生效态的接线（独立进程）。

#[test]
fn attack_log_seam_defaults_off_and_wires_from_env() {
    // ① 未安装 ⇒ 关
    assert!(
        !game_svr::test_seams::attack_log_enabled(),
        "未安装必须为关（不多打一行日志）"
    );

    // ② 注入 1 ⇒ 开
    std::env::set_var("MIR2_LOG_ATTACKS", "1");
    let installed = game_svr::test_seams::install_log_attacks_from_env().expect("合法值");
    assert!(installed, "返回生效值");
    assert!(game_svr::test_seams::attack_log_enabled());

    // ③ 只能装一次（运行中不可改）
    assert!(
        game_svr::test_seams::install_log_attacks(Some("0")).is_err(),
        "重复安装必须报错"
    );
    assert!(
        game_svr::test_seams::attack_log_enabled(),
        "重复安装不得改值"
    );
}
