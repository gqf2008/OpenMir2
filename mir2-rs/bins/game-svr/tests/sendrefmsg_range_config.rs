//! 卡 `openmir2-sendrefmsg-range-config`（tier=R）判据②：`[Setup] SendRefMsgRange` 的生效值经
//! **生产安装点**（`sim_loop` 启动处的 `chat::install_send_ref_msg_range(enter_cfg.ref_msg_range)`）
//! 进入 `chat::send_ref_msg_range()`，`SendRefMsg` 收件人集合随之变化。
//!
//! 为什么放集成测试（独立进程）：安装会写**进程全局**生效值，放 lib 单测会与并行线程串扰；
//! 本文件在自己的二进制里跑，进程全局只影响本用例。
//!
//! 判据：
//! ① 未安装 ⇒ oracle **源码缺省 12**（`src/Modules/SystemModule/Conf/Model/GameSvrConf.cs:1503`
//!    构造体 `SendRefMsgRange = 12;`），**不是 15** ⇒ 12 格内收、13 格起不收（今天的收件人集合）；
//! ② 安装 15（= `[Setup] SendRefMsgRange=15`）⇒ 12 格外、15 格内**能收到**，16 格不收。
//!
//! 牙：把 `chat::send_ref_msg_range()` 改回常量 `chat::SEND_REF_MSG_RANGE`（= 12）⇒ ② 在本文件必红。
//! （配置文件 → 15 的解析面由 `config::tests::setup_drop_pickup_keys_and_defaults` 钉住；
//!  缺键 ⇒ 12 的缺省面同在那里。）

use game_svr::chat;
use mir2_world::{Entity, World, ACTOR_RACE_PLAY};

fn put(world: &mut World, name: &str, x: i32, y: i32) -> i32 {
    let mut e = Entity::new(0, name.to_string(), x, y);
    e.race = ACTOR_RACE_PLAY;
    world.add_entity(e)
}

#[test]
fn configured_range_drives_send_ref_msg_recipients() {
    // ① 未安装（未走 `sim_loop`）⇒ oracle 源码缺省 12，而不是卡面误记的 15。
    assert_eq!(chat::send_ref_msg_range(), 12);
    assert_eq!(chat::send_ref_msg_range(), chat::SEND_REF_MSG_RANGE);

    let mut world = World::new(64, 64);
    let a = put(&mut world, "a", 10, 10);
    let d12 = put(&mut world, "d12", 22, 10); // dx=12
    let d13 = put(&mut world, "d13", 23, 10); // dx=13
    let d15 = put(&mut world, "d15", 25, 10); // dx=15
    let d16 = put(&mut world, "d16", 26, 10); // dx=16

    // 今天的收件人集合（部署值 12 ⇒ 12 格内，13 格起不收）：回归。
    assert_eq!(
        chat::hear_targets(&world, a, chat::send_ref_msg_range()),
        vec![a, d12],
        "① 缺省 12 ⇒ 与今天一致（13 格起不收）"
    );

    // ② 生产安装点：`[Setup] SendRefMsgRange=15`。
    chat::install_send_ref_msg_range(15);
    assert_eq!(chat::send_ref_msg_range(), 15, "安装后生效值必须是 15");
    let got = chat::hear_targets(&world, a, chat::send_ref_msg_range());
    assert_eq!(
        got,
        vec![a, d12, d13, d15],
        "② 12 格外、15 格内的玩家能收到（证明真的读了配置值）"
    );
    assert!(!got.contains(&d16), "16 格外不收");
}
