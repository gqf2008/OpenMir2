//! **出站接线片 W2 端到端**：`crates/world` 的世界事件（`TickRecord::{pushes,rushes,butches}`）
//! → `game_svr::downlink` 的 `SM_*` 帧字节。
//!
//! 覆盖 5 条 B 类帧里的 4 条世界事件路径（`SM_BACKSTEP(9)`/`SM_RUSH(6)`/`SM_RUSHKUNG(7)`/
//! `SM_BUTCH(637)`）；`SM_DEATH(32)` 的事件面是 AOI 出现帧（宿主 `drain_aoi_downlink`），
//! 帧面在 lib 的 `downlink::appear_frames` 单测里覆盖。
//!
//! 断言口径：事件字段（world 侧）→ 帧头 12B 的 ident/param/tag/series 字节（downlink 侧），
//! 逐字段打通两 crate 的接缝（不是各自单测的重复）。

use game_svr::downlink;
use mir2_protocol::messages;
use mir2_world::map::CellAttribute;
use mir2_world::{Command, Entity, RushFrame, World};

fn display(dir: u8) -> downlink::ActorDisplay {
    downlink::ActorDisplay::new(dir, 7, 0x0102_0304, 0, 0x0506_0708, 255, b"hero".to_vec())
}

fn header_u16(body: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([body[at], body[at + 1]])
}

#[test]
fn push_event_from_tick_to_backstep_bytes() {
    let mut w = World::new(64, 64);
    let id = w.add_entity(Entity::new(0, "hero", 5, 5));
    w.tick(); // 跑 Enter 落图
    w.enqueue(Command::Push {
        actor_id: id,
        dir: mir2_world::magic::DIR_UP,
        count: 2,
    });
    let rec = w.tick();
    assert_eq!(rec.pushes.len(), 2, "推 2 格 ⇒ 2 条 RM_PUSH");

    let ev = rec.pushes[0];
    assert_eq!(
        (ev.actor_id, ev.dir, ev.x, ev.y),
        (id, 4, 5, 4),
        "dir=nBackDir(上→下=4)"
    );
    let body = downlink::backstep_frame(&ev, &display(ev.dir)).expect("SM_BACKSTEP 编码器");
    assert_eq!(i32::from_le_bytes(body[0..4].try_into().unwrap()), id);
    assert_eq!(header_u16(&body, 4), messages::SM_BACKSTEP);
    assert_eq!(header_u16(&body, 6), 5, "param = x");
    assert_eq!(header_u16(&body, 8), 4, "tag = y");
    assert_eq!(body[10], ev.dir, "series 低字节 = 事件 nBackDir");
    assert_eq!(body[11], 7, "series 高字节 = Light");
}

#[test]
fn rush_and_rushkung_events_from_tick_to_bytes() {
    let mut w = World::new(64, 64);
    let id = w.add_entity(Entity::new(0, "hero", 5, 5));
    w.tick();
    // (7,5) 设为高墙 ⇒ 冲刺到 (6,5) 后正前不可走 ⇒ 末尾一条 RUSHKUNG（落在正前格 (7,5)）。
    w.maps[0]
        .get_cell_info_mut(7, 5)
        .0
        .set_attribute(CellAttribute::HighWall);
    w.enqueue(Command::Rush {
        actor_id: id,
        dir: mir2_world::magic::DIR_RIGHT,
        steps: 4,
    });
    let rec = w.tick();
    assert_eq!(rec.rushes.len(), 2, "1 格 Rush + 1 条 RushKung");
    assert_eq!(rec.rushes[0].frame, RushFrame::Rush);
    assert_eq!(rec.rushes[1].frame, RushFrame::RushKung);

    let rush = downlink::rush_frame(&rec.rushes[0], &display(rec.rushes[0].dir)).unwrap();
    assert_eq!(header_u16(&rush, 4), messages::SM_RUSH);
    assert_eq!(header_u16(&rush, 6), 6, "Rush 落在 (6,5)");
    assert_eq!(rush[10], 2, "series 低字节 = nDir(RIGHT=2)");

    let kung = downlink::rush_frame(&rec.rushes[1], &display(rec.rushes[1].dir)).unwrap();
    assert_eq!(header_u16(&kung, 4), messages::SM_RUSHKUNG);
    assert_eq!(header_u16(&kung, 6), 7, "RushKung 落在正前格 (7,5)");
    assert_eq!(header_u16(&kung, 8), 5);
}

#[test]
fn butch_event_from_tick_to_bytes() {
    let mut w = World::new(64, 64);
    let id = w.add_entity(Entity::new(0, "hero", 7, 9));
    w.tick();
    w.enqueue(Command::Butch {
        actor_id: id,
        dir: 3,
    });
    let rec = w.tick();
    assert_eq!(rec.butches.len(), 1);
    let body = downlink::butch_frame(&rec.butches[0]);
    assert_eq!(i32::from_le_bytes(body[0..4].try_into().unwrap()), id);
    assert_eq!(header_u16(&body, 4), messages::SM_BUTCH);
    assert_eq!(header_u16(&body, 6), 7);
    assert_eq!(header_u16(&body, 8), 9);
    assert_eq!(header_u16(&body, 10), 3, "series = Dir，不叠 Light");
    assert_eq!(body.len(), 12, "SM_BUTCH 无尾");
}
