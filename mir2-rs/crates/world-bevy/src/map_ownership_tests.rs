//! **P2-3a**：`map_id` 归属模型（全局保序 + 每 World「不拥有」集合 + 单点 fail-closed）。
//!
//! 一个 World 的 `maps` 仍是**全局下标空间**：本 World 拥有的图是真图，其余是 1×1 stub。
//! 本模块逐入口钉住「非拥有图 ⇒ fail-closed」，并钉住 tick 期扫描与运行期不变式。
//!
//! 入口清单（file:line 审计在卡面 patch body；这里按**符号**组织用例）：
//! - `World::enter_map_at` / `SessionRegistry::switch_map_inner`（含 `switch_map` / `switch_map_space`）；
//! - `World::walk_route_check`（route_enter 的对外入口，含 `RouteHop.dst_map_id`）；
//! - `World::base_object_move_player`（home move / BaseObjectMove）；
//! - `space_move::monster_space_move`（怪物的 SpaceMove 同服支）；
//! - `World::spawn_monster_precomputed` / `World::enter_world`（落格入口）；
//! - `World::insert_merchant` / `World::insert_npc`（固定 NPC/商人落格）。
//!
//! 未单独覆盖的两处（结构性不可达，只保留防御检查）：`World::spawn_clone`（主人已在
//! 非拥有图上本身就是不变式违背）与 `mag_make_clone`（其唯一生产调用者即 `spawn_clone`）。

use crate::entity::{EntitySpawn, ACTOR_RACE_MONSTER};
use crate::map::{CellType, RouteHop};
use crate::merchant::{MerchantFlags, MerchantSpawn, MerchantState};
use crate::npc::{NpcSpawn, NpcState};
use crate::session::SessionError;
use crate::world::{HomeMoveTarget, RouteCheck, World};

/// 三图世界：0/1 拥有（8×8），2 是 stub（1×1，不拥有）。
fn world3() -> World {
    let mut w = World::with_maps(vec![(8, 8), (8, 8), (1, 1)]);
    w.set_map_ownership(&[true, true, false]);
    w
}

fn player(name: &str) -> EntitySpawn {
    EntitySpawn::new(0, name.to_string(), 2, 2)
}

fn place_player(world: &mut World, name: &str, x: i32, y: i32) -> i32 {
    world.open_session(name);
    world
        .enter_map(name, EntitySpawn::new(0, name.to_string(), x, y), 0)
        .expect("拥有图可进")
}

#[test]
fn ownership_defaults_to_all_owned_and_set_validates() {
    let mut w = World::with_maps(vec![(4, 4), (5, 5)]);
    assert!(w.is_map_owned(0) && w.is_map_owned(1), "缺省全拥有");
    assert_eq!(w.owned_map_ids(), vec![0, 1]);
    w.set_map_ownership(&[true, false]);
    assert!(w.is_map_owned(0));
    assert!(!w.is_map_owned(1));
    assert_eq!(w.owned_map_ids(), vec![0]);
    // `[]` = 恢复全拥有。
    w.set_map_ownership(&[]);
    assert!(w.is_map_owned(1));
}

#[test]
#[should_panic(expected = "禁止半套归属")]
fn ownership_length_mismatch_panics() {
    let mut w = World::with_maps(vec![(4, 4), (5, 5)]);
    w.set_map_ownership(&[true]);
}

#[test]
fn enter_map_rejects_unowned_with_distinct_error() {
    let mut w = world3();
    w.open_session("p");
    let err = w
        .enter_map("p", player("p"), 2)
        .expect_err("stub 进图必须拒绝");
    assert_eq!(err, SessionError::MapNotInShard, "必须是可区分错误");
    assert!(
        !w.entities.iter_ref().any(|e| e.name == "p"),
        "拒绝不得留实体"
    );
    // 对照组：拥有图可进。
    assert!(w.enter_map("p", player("p"), 0).is_ok());
}

#[test]
fn switch_map_rejects_unowned_and_keeps_old_map_cell() {
    let mut w = world3();
    let id = place_player(&mut w, "p", 2, 2);
    let err = w.switch_map("p", 2, 0, 0).expect_err("stub 换图必须拒绝");
    assert_eq!(err, SessionError::MapNotInShard);
    let e = w.entities.get(id).expect("实体仍在");
    assert_eq!((e.map_id, e.x, e.y), (0, 2, 2), "旧图旧格保持不变");
    assert_eq!(w.maps[0].cell(2, 2).count(), 1, "旧格登记保持");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0, "stub 格不得登记");
    assert!(w.sessions.map_changes().is_empty(), "拒绝不得记 MapChange");
}

#[test]
fn switch_map_space_rejects_unowned() {
    let mut w = world3();
    let id = place_player(&mut w, "p", 2, 2);
    let err = w
        .switch_map_space("p", 2, 0, 0, false)
        .expect_err("stub SpaceMove 必须拒绝");
    assert_eq!(err, SessionError::MapNotInShard);
    assert_eq!(w.entities.get(id).expect("实体仍在").map_id, 0);
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn walk_route_rejects_unowned_dst_without_moving_or_map_change() {
    let mut w = world3();
    let id = place_player(&mut w, "p", 3, 3);
    // 在 (3,3) 放一条通往 stub（map 2）的连接点：局部下标即全局下标。
    assert!(w.maps[0].add_map_route(
        3,
        3,
        7,
        RouteHop {
            dst_map_id: 2,
            dst_x: 0,
            dst_y: 0,
        },
    ));
    let check = w.walk_route_check(id);
    assert_eq!(
        check,
        RouteCheck::NoMapChange,
        "非拥有目标 ⇒ 前置失败（无换图、不复位泡点）"
    );
    let e = w.entities.get(id).expect("实体仍在");
    assert_eq!((e.map_id, e.x, e.y), (0, 3, 3), "换图被拒 ⇒ 停在原图原格");
    assert!(w.sessions.map_changes().is_empty(), "不得产生 MapChange");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0, "stub 格不得登记");
}

#[test]
fn home_move_rejects_unowned_target() {
    let mut w = world3();
    let id = place_player(&mut w, "p", 2, 2);
    let out = w.base_object_move_player(
        "p",
        id,
        Some(HomeMoveTarget::Fixed {
            map_id: 2,
            x: 0,
            y: 0,
        }),
    );
    assert!(!out.map_found, "非拥有图视同 FindMap == null");
    assert!(!out.moved);
    assert_eq!(w.entities.get(id).expect("实体仍在").map_id, 0);
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn monster_space_move_rejects_unowned_dest() {
    let mut w = world3();
    let mut m = EntitySpawn::new(0, "m".to_string(), 2, 2);
    m.race = ACTOR_RACE_MONSTER;
    let id = w.add_entity(m);
    assert!(w.enter_world(id), "怪物先落在拥有图");
    let moved = crate::space_move::monster_space_move(&mut w, id, 2, 0, 0);
    assert!(!moved, "stub 目标必须拒绝");
    let e = w.entities.get(id).expect("实体仍在");
    assert_eq!((e.map_id, e.x, e.y), (0, 2, 2), "位置不变");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn enter_world_refuses_stub_placement() {
    let mut w = world3();
    let mut m = EntitySpawn::new(0, "m".to_string(), 0, 0);
    m.race = ACTOR_RACE_MONSTER;
    m.map_id = 2;
    let id = w.add_entity(m);
    assert!(!w.enter_world(id), "stub 上禁止实体");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn spawn_monster_precomputed_refuses_stub() {
    let mut w = world3();
    let out = w.spawn_monster_precomputed(
        "m",
        0,
        0,
        2,
        ACTOR_RACE_MONSTER,
        1,
        0,
        10,
        0,
        0,
        0,
        0,
        0,
        0,
        Vec::new(),
        crate::entity::MonsterCtorDraws::default(),
        0,
        false,
    );
    assert!(out.is_none(), "stub 上禁止生成怪物");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn insert_merchant_and_npc_refuse_stub() {
    let mut w = world3();
    let merchant = MerchantSpawn {
        name: "m".to_string(),
        x: 0,
        y: 0,
        map_id: 2,
        state: MerchantState::new(
            String::new(),
            String::new(),
            0,
            0,
            100,
            false,
            false,
            0,
            MerchantFlags::default(),
            Vec::new(),
            Vec::new(),
            0,
        ),
    };
    assert!(w.insert_merchant(merchant).is_none(), "stub 上禁止商人");
    let npc = NpcSpawn {
        name: "n".to_string(),
        x: 0,
        y: 0,
        map_id: 2,
        state: NpcState::default(),
    };
    assert!(w.insert_npc(npc).is_none(), "stub 上禁止固定 NPC");
    assert_eq!(w.maps[2].cell(0, 0).count(), 0);
}

#[test]
fn clone_spawn_stays_on_casters_owned_map() {
    // 主人只能在拥有图上 ⇒ 克隆体落点也是拥有图（stub 分支是防御性的，不可达）。
    let mut w = world3();
    let id = place_player(&mut w, "p", 2, 2);
    let clone = crate::slave::spawn_clone(&mut w, id, 0).expect("拥有图可生成克隆");
    let e = w.entities.get(clone.clone_id).expect("克隆实体已入世界");
    assert_eq!(e.map_id, 0);
    assert!(w.is_map_owned(e.map_id));
}

/// **不变式扫描牙**：伪造「已落格实体在 stub」的非法状态 ⇒ 断言 panic。
///
/// 红检：让 `assert_no_entity_on_unowned_map` 跳过非拥有图（或删掉 `add_to_mapped` 检查的
/// 反向）⇒ 本用例不再 panic，红。
#[test]
#[should_panic(expected = "P2-3a 不变式破坏")]
fn invariant_panics_when_placed_entity_is_on_stub() {
    let mut w = world3();
    let mut m = EntitySpawn::new(0, "bad".to_string(), 0, 0);
    m.race = ACTOR_RACE_MONSTER;
    m.map_id = 2;
    let id = w.add_entity(m);
    // 伪造「已落格」的非法状态（模拟某个入口漏了归属检查的后果）。
    w.entities
        .map_binding_mut(id)
        .expect("刚插入")
        .add_to_mapped = true;
    // **release 可观测**：报告（非 cfg 门）必须列出该实体；debug 每 tick 断言才会 panic。
    assert_eq!(w.unowned_map_entity_ids(), vec![(id, 2)]);
    w.assert_no_entity_on_unowned_map();
}

/// **release 可观测**：入口拒绝次数单调累计（非 cfg 门），单片恒 0。
///
/// 红检：删任一处 `note_unowned_refusal()` ⇒ 对应用例计数少 1，红。
#[test]
fn unowned_refusals_are_counted_in_all_profiles() {
    let mut w = world3();
    assert_eq!(w.unowned_refusal_count(), 0, "未拒绝时恒 0");
    w.open_session("p");
    assert!(matches!(
        w.enter_map("p", player("p"), 2),
        Err(SessionError::MapNotInShard)
    ));
    assert_eq!(w.unowned_refusal_count(), 1);
    place_player(&mut w, "q", 2, 2);
    assert!(matches!(
        w.switch_map("q", 2, 0, 0),
        Err(SessionError::MapNotInShard)
    ));
    assert_eq!(w.unowned_refusal_count(), 2);
    let mut m = EntitySpawn::new(0, "m".to_string(), 2, 2);
    m.race = ACTOR_RACE_MONSTER;
    let mid = w.add_entity(m);
    assert!(w.enter_world(mid));
    assert!(!crate::space_move::monster_space_move(&mut w, mid, 2, 0, 0));
    assert_eq!(w.unowned_refusal_count(), 3);
    assert!(w.unowned_map_entity_ids().is_empty());
    // 单片：全拥有 ⇒ 计数恒 0（`MIR2_SHARDS=1` 等价）。
    let mut single = World::with_maps(vec![(4, 4)]);
    single.open_session("p");
    assert!(single.enter_map("p", player("p"), 0).is_ok());
    assert_eq!(single.unowned_refusal_count(), 0);
}

/// **接线牙**：不变式必须由 `tick_tail` 在每 tick 末尾调用（删掉该调用 ⇒ 本用例红）。
#[test]
fn tick_tail_calls_the_ownership_invariant() {
    let src = include_str!("world.rs");
    let tail = src.find("pub fn tick_tail(").expect("tick_tail 定义行");
    let next_fn = src[tail..]
        .find("\n    pub(crate) fn assert_no_entity_on_unowned_map(")
        .expect("不变式函数定义");
    let body = &src[tail..tail + next_fn];
    assert!(
        body.contains("self.assert_no_entity_on_unowned_map();"),
        "tick_tail 必须调用归属不变式"
    );
}

/// **扫描牙**：tick 期逐图扫描（掉落实体清单）只遍历拥有图。
///
/// 红检：把 `floor_items_*` 改回 `self.maps.iter()`（遍历全部 id）⇒ stub 上的物品会被列出，红。
#[test]
fn tick_scans_skip_unowned_stub_maps() {
    let mut w = world3();
    assert!(w.maps[1].add_map_object(1, 1, CellType::Item, 11, 0));
    assert!(w.maps[2].add_map_object(0, 0, CellType::Item, 22, 0));
    let items = w.floor_items_sorted();
    assert!(items.iter().any(|(map, _, _, id)| *map == 1 && *id == 11));
    assert!(
        !items.iter().any(|(map, _, _, _)| *map == 2),
        "stub 不得被扫描当成真图：{items:?}"
    );
}

/// **单片等价**：缺省全拥有时，逐图扫描覆盖全部图（`MIR2_SHARDS=1` 零行为变化）。
#[test]
fn single_shard_scan_is_identity() {
    let mut w = World::with_maps(vec![(4, 4), (4, 4)]);
    assert_eq!(w.owned_map_ids(), vec![0, 1]);
    assert!(w.maps[0].add_map_object(1, 1, CellType::Item, 10, 0));
    assert!(w.maps[1].add_map_object(2, 2, CellType::Item, 20, 0));
    let maps: Vec<usize> = w
        .floor_items_sorted()
        .iter()
        .map(|(m, _, _, _)| *m)
        .collect();
    assert_eq!(maps, vec![0, 1], "单片 = 全图（与改动前同序）");
}

#[test]
fn single_shard_enter_and_switch_are_unchanged() {
    let mut w = World::with_maps(vec![(8, 8), (8, 8)]);
    let id = place_player(&mut w, "p", 2, 2);
    assert!(w.switch_map("p", 1, 3, 3).is_ok(), "单片全拥有 ⇒ 换图照旧");
    assert_eq!(w.entities.get(id).expect("实体在").map_id, 1);
}
