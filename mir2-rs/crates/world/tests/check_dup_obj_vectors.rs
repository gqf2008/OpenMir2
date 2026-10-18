//! 卡 `openmir2-run-gap-checkdupobj`（R 级）构造级判据。
//!
//! oracle 逐句：`src/M2Server/Player/PlayObject.Message.cs:263-285`（`PlayObject.Run` 的重叠推挤段）。
//! Rust 等价物：`mir2_world::World::check_dup_obj`（`World::tick` 内、FireHit/TwinHit 到期段之后、
//! `base.Run` 的 VerifyMapTime 之前调用）。
//!
//! 判据 a–f 与任务单一一对应：
//! a) 3s 周期门严格 `>`（恰 3000ms 不检查）；b) 2 对象：10s 前不推、`>`10s 推一次；
//! c) 3 对象：`>`3s 推一次；d) `<20000` 上限之后不再推；e) 推挤方向 = 抽数结果（固定种子可复现）；
//! f) 阴性对照：单对象格不推、无抽数。

use mir2_shared::rng::RandomNumber;
use mir2_world::action::PushEvent;
use mir2_world::entity::{Entity, ACTOR_RACE_PLAY};
use mir2_world::magic::{back_dir, front_position};
use mir2_world::World;

fn player(id: i32, x: i32, y: i32) -> Entity {
    let mut e = Entity::new(id, format!("p{id}"), x, y);
    e.race = ACTOR_RACE_PLAY;
    e.hp = 100;
    e
}

/// `n` 个玩家**同格**（5,5）进图；`add_entity` 的 `CheckDupObjTick` 在 clock=0 落章
/// （`PlayObject.cs:913`），`tick()` 把时钟推到 200ms（200 <= 3000 ⇒ 不检查）。
fn rig(n: i32) -> World {
    let mut w = World::new(32, 32);
    for i in 1..=n {
        w.add_entity(player(i, 5, 5));
    }
    w.tick();
    w
}

fn check_tick(w: &World, id: i32) -> i64 {
    w.entities.get(id).expect("实体存在").check_dup_obj_tick
}

/// 判据 a（严格 `>`）+ `World::tick` 接线：15 个 tick 到 3000ms 不检查，第 16 个 tick（3200ms）才检查。
#[test]
fn three_second_gate_is_strict_greater_through_tick() {
    let mut w = rig(1);
    for _ in 0..14 {
        w.tick();
    }
    assert_eq!(w.clock_ms, 3000, "14 + 1 次 tick = 3000ms");
    assert_eq!(
        check_tick(&w, 1),
        0,
        "恰 3000ms（> 不成立）⇒ 不检查、不落章"
    );
    w.tick();
    assert_eq!(w.clock_ms, 3200);
    assert_eq!(check_tick(&w, 1), 3200, "3200ms（> 3000）⇒ 检查并落章");
}

/// 判据 a（直接入口的边界）：`now - CheckDupObjTick` 恰等于 3000 不检查，3001 才检查。
#[test]
fn three_second_gate_is_strict_greater_direct() {
    let mut w = rig(1);
    w.clock_ms = 3000;
    assert!(w.check_dup_obj().is_empty());
    assert_eq!(check_tick(&w, 1), 0, "恰 3000ms 不检查");
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty());
    assert_eq!(check_tick(&w, 1), 3001, "3001ms 检查");
}

/// 判据 b：2 个对象。首次 ≥2 记 `BoDuplication=true` / `DupStartTick=now`；`>`10s 前不推。
#[test]
fn two_objects_push_only_after_ten_seconds() {
    // 10s 前：从 3001ms 起按 3001ms 节奏检查（3001/6002/9003/12004），均不推（最大 elapsed=9003）。
    let mut w = rig(2);
    for t in [3001_i64, 6002, 9003, 12004] {
        w.clock_ms = t;
        let ev = w.check_dup_obj();
        assert!(ev.is_empty(), "{t}ms：elapsed <= 10s 不推");
    }
    let e = w.entities.get(1).unwrap();
    assert!(e.bo_duplication, "≥2 对象 ⇒ BoDuplication=true");
    assert_eq!(e.dup_start_tick, 3001, "首次 ≥2 的时刻");

    // 恰 10000ms（13001-3001）：检查门通过（diff=10000>3000），`>` 不成立 ⇒ 不推。
    let mut w = rig(2);
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty());
    w.clock_ms = 13001;
    assert!(w.check_dup_obj().is_empty(), "elapsed 恰 10000ms 不推");

    // 10001ms：推且只推一次。
    let mut w = rig(2);
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty());
    w.clock_ms = 13002;
    let ev = w.check_dup_obj();
    assert_eq!(ev.len(), 1, "2 对象 >10s ⇒ 推 1 次（nPushCount=1）");
}

/// 判据 c：3 个对象，`>`3s 推一次；且在 3s 内（首检后立即复检）不推。
#[test]
fn three_objects_push_after_three_seconds() {
    let mut w = rig(3);
    // 首个检查（3001ms）：记 DupStartTick，elapsed=0 ⇒ 不推。
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty(), "首次 ≥2 ⇒ 只记起点");
    assert_eq!(w.entities.get(1).unwrap().dup_start_tick, 3001);

    // 3 对象分支的边界：手工给「上次检查」一个使 3s 检查门通过的旧值
    // （now-2999 ⇒ diff=3001 > 3000），`DupStartTick` 恰为 now-3000 ⇒ `> 3000` 不成立。
    w.entities.get_mut(1).unwrap().check_dup_obj_tick = 3000 - 1;
    w.entities.get_mut(1).unwrap().dup_start_tick = 3000;
    w.clock_ms = 6000;
    assert!(w.check_dup_obj().is_empty(), "elapsed 恰 3000ms 不推");

    // 3001ms ⇒ 推。
    w.entities.get_mut(1).unwrap().check_dup_obj_tick = 3000 - 1;
    w.entities.get_mut(1).unwrap().dup_start_tick = 2999;
    w.clock_ms = 6000;
    let ev = w.check_dup_obj();
    assert_eq!(ev.len(), 1, "3 对象 >3s ⇒ 推 1 次");
}

/// **跨卡判据（`openmir2-movetomovingobject-quirk`）**：3 人同格时推挤必须复刻 oracle 的格子
/// 簿记怪癖——`Envirnoment.cs:383-424` 的 `moveObject` 是循环变量，cert 在首/中时进目标格的是
/// **源格最后一个元素**，cert 自己从两格消失。
///
/// 牙：把 `MapGrid::move_object` 改回「合理加 cert」⇒ 本用例红（目标格会出现 id=1）。
#[test]
fn push_three_overlapping_replicates_move_to_moving_object_quirk() {
    let mut w = rig(3);
    // 源格 (5,5) 的对象序列应为 [1,2,3]（按插入序）。
    let src_before: Vec<i32> = w.maps[0]
        .get_cell_info(5, 5)
        .0
        .obj_list
        .iter()
        .filter(|o| o.actor_object)
        .map(|o| o.cell_obj_id)
        .collect();
    assert_eq!(src_before, vec![1, 2, 3], "源格插入序");

    // 与 `three_objects_push_after_three_seconds` 同序：先到 3001ms 让首个检查记下
    // `BoDuplication=true` 与 `DupStartTick`，再手工把 tick 拨到「3s 检查门 + 3s 推挤门」都过。
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty(), "首次 ≥2 ⇒ 只记起点");
    {
        let e = w.entities.get_mut(1).unwrap();
        e.check_dup_obj_tick = 3000 - 1;
        e.dup_start_tick = 2999;
    }
    w.clock_ms = 6000;
    let ev = w.check_dup_obj();
    assert_eq!(ev.len(), 1, "3 对象 >3s ⇒ 推 1 次");

    // 1 号必须从源格消失，且**不再出现在任何格**（怪癖：cert 被 Remove 后无人重新 Add）。
    // 推挤只涉及相邻格 ⇒ 扫 (5,5) ± 3 的窗口即可覆盖。
    for x in 2..=8 {
        for y in 2..=8 {
            let (cell, ok) = w.maps[0].get_cell_info(x, y);
            if !ok {
                continue;
            }
            assert!(
                !cell
                    .obj_list
                    .iter()
                    .any(|o| o.actor_object && o.cell_obj_id == 1),
                "怪癖：被推者不得出现在任何格子（cell ({x},{y})）"
            );
        }
    }
    // 源格剩 [2,3]；目标格拿到的是**源格最后一个元素 3**（不是 1）。
    let src_after: Vec<i32> = w.maps[0]
        .get_cell_info(5, 5)
        .0
        .obj_list
        .iter()
        .filter(|o| o.actor_object)
        .map(|o| o.cell_obj_id)
        .collect();
    assert_eq!(src_after, vec![2, 3], "源格删 1 后剩 [2,3]");
    let mut gained = Vec::new();
    for x in 2..=8 {
        for y in 2..=8 {
            if (x, y) == (5, 5) {
                continue;
            }
            let (cell, ok) = w.maps[0].get_cell_info(x, y);
            if !ok {
                continue;
            }
            let ids: Vec<i32> = cell
                .obj_list
                .iter()
                .filter(|o| o.actor_object)
                .map(|o| o.cell_obj_id)
                .collect();
            if ids.contains(&3) {
                gained.push(((x, y), ids));
            }
        }
    }
    assert_eq!(gained.len(), 1, "恰一个目标格拿到 3");
    assert_eq!(gained[0].1, vec![3], "目标格内容是源格最后一个元素");
}

/// 判据 d：`< 20000` 上限。elapsed=19999 推；恰 20000 / 20001 不推。
#[test]
fn twenty_second_cap_is_strict_less_than() {
    // 2 对象、起点 0（`DupStartTick` 初值）：now=19999 ⇒ 推；20000/20001 ⇒ 不推。
    for (now, want_push) in [(19_999_i64, true), (20_000, false), (20_001, false)] {
        let mut w = rig(2);
        {
            let e = w.entities.get_mut(1).unwrap();
            e.bo_duplication = true;
            e.dup_start_tick = 0;
            e.check_dup_obj_tick = 0; // now - 0 > 3000，检查门通过
        }
        w.clock_ms = now;
        let ev = w.check_dup_obj();
        assert_eq!(
            ev.len(),
            usize::from(want_push),
            "elapsed={now}ms：`< 20000` 门 ⇒ push={want_push}"
        );
    }
}

/// 判据 e：推挤方向 = `RandomByte(8)` 的抽数结果（固定种子可复现），且 `Dir` 落 `nBackDir`。
#[test]
fn push_direction_is_the_drawn_byte() {
    let mut w = rig(2);
    // 先走到推挤门（记起点）：
    w.clock_ms = 3001;
    assert!(w.check_dup_obj().is_empty());
    // 再越过 10s 门：取数一次（world rng 种子 42，此前零取数）：
    let expected = RandomNumber::with_seed(42).random_byte(8);
    w.clock_ms = 13002;
    let ev = w.check_dup_obj();
    assert_eq!(ev.len(), 1, "一抽一推");
    let back = back_dir(expected);
    let (nx, ny) = front_position(5, 5, expected, 32, 32);
    assert_eq!(
        ev[0],
        PushEvent {
            actor_id: 1,
            dir: back,
            x: nx,
            y: ny
        },
        "RM_PUSH 帧面：dir=nBackDir(抽数)，位置=推进后的格"
    );
    let e = w.entities.get(1).unwrap();
    assert_eq!((e.x, e.y), (nx, ny), "CurrX/CurrY = 推进后的格");
    assert_eq!(e.dir(), back, "result>0 ⇒ Dir = nBackDir");
    assert_eq!(
        e.ai.walk_tick, 0,
        "玩家 Race=Play(0) < Animal ⇒ 不执行 WalkTick += 800"
    );
}

/// 判据 f：单对象格不推、**不抽数**（阴性对照）。
#[test]
fn single_object_never_pushes_and_never_draws() {
    let mut w = rig(1);
    let mut expect = w.rng.clone();
    for t in [3001_i64, 6002, 9003, 12004, 20005, 60000] {
        w.clock_ms = t;
        assert!(w.check_dup_obj().is_empty(), "{t}ms 单对象格不推");
    }
    assert!(!w.entities.get(1).unwrap().bo_duplication);
    assert_eq!(
        w.rng.random(),
        expect.random(),
        "未过门 ⇒ 全局 RNG 流一次都没动（RandomByte(8) 未抽）"
    );
}

/// 判据 b/c 的状态复位语义：对象数掉回 `<2` 只清 `BoDuplication`，**不**复位 `DupStartTick`；
/// 再次 `>=2` 时重新以当时时刻置 `DupStartTick`（照 `PlayObject.Message.cs:268-279`）。
#[test]
fn below_two_clears_flag_without_resetting_dup_start() {
    let mut w = rig(2);
    w.clock_ms = 3001;
    let _ = w.check_dup_obj();
    assert_eq!(w.entities.get(1).unwrap().dup_start_tick, 3001);

    // 把 2 号挪走（换格），本格只剩 1 人。
    {
        let (maps, entities) = (&mut w.maps, &mut w.entities);
        let e = entities.get_mut(2).unwrap();
        let (fx, fy) = (e.x, e.y);
        e.x = 9;
        e.y = 9;
        maps[0].move_object(
            mir2_world::map::MoveRequest {
                from_x: fx,
                from_y: fy,
                actor_id: 2,
                to_x: 9,
                to_y: 9,
                ignore_block: false,
                add_time: 3001,
                cert_cell_type: mir2_world::map::CellType::Play,
            },
            |_| false,
        );
    }
    w.clock_ms = 6002;
    let _ = w.check_dup_obj();
    let e = w.entities.get(1).unwrap();
    assert!(!e.bo_duplication, "<2 ⇒ BoDuplication=false");
    assert_eq!(e.dup_start_tick, 3001, "DupStartTick 不复位");
}
