//! **`openmir2-player-drop-useitems`**：玩家死亡 `DropUseItems`
//! （`src/M2Server/Player/PlayObject.Message.cs:2152-2228`）的逐槽取数与清槽。
//!
//! 调用链（oracle 实测）：
//! ```text
//! PlayObject.Die()                       PlayObject.cs:1964-1996
//!   base.Die()                           BaseObject.Base.cs:318-344
//!     KillFunc()                         BaseObject.cs:2429-2447
//!       两个**独立** if（不是 `||` 折叠）：
//!       if (ExpHitter.Master != null) SendMsg(RM_DIEDROPITEM, Master.ActorId, …)
//!       if (ExpHitter.Race == Play)   SendMsg(RM_DIEDROPITEM, ExpHitter.ActorId, …)
//!       两条件同时成立 ⇒ oracle 两次 DropUseItems；本骨架的 Play 实体不置 Master
//!       （`PlayCloneObject` ctor 亦不置）⇒ 实际互斥（T-81 登记）。
//! Operate(RM_DIEDROPITEM)                BaseObject.Base.cs:766-767
//!   DieDropItems(wParam)                 BaseObject.cs:2450-2464
//!     if (!FightZone && !Fight3Zone && !Animal)
//!       DropUseItems(actorId)            PlayObject.Message.cs:2152
//! ```
//!
//! `DropUseItems` 的两段（**顺序固定，取数不可合并**）：
//! 1. 逐槽（13）：`(stdItem.ItemDesc & 8) != 0` ⇒ 记 `DeleteItem{MakeIndex}` 并 **只清 `Index`**。
//!    本段**零 RNG**。
//! 2. `nRate = PvpLevel() > 2 ? DieRedDropUseItemRate : DieDropUseItemRate`；对**全部 13 槽**
//!    逐槽 `Random(nRate)`（**空槽也抽**）。`!= 0` ⇒ continue；否则若槽非空且不在
//!    `InDisableTakeOffList`，调 `DropItemDown(useItem, MIN(DropItemRage,3), true, baseObject, ActorId)`：
//!    - `stdMode == 40` ⇒ `Dura` 按 oracle 的 **u16 回绕**（`ushort idura = (ushort)(idura - 2000)`；
//!      `if (idura <= 0)` 对 `ushort` 只在 0 时成立 ⇒ 与回绕后值恒等）——零 RNG，且即使落地失败也生效；
//!    - `stdMode == 45` ⇒ `Looks = GetRandomLook(Looks, Shape)` = **`Random(Shape)` 一抽**；
//!    - `GetDropPosition` 纯格子扫描（零 RNG）；
//!    - `Envir.AddItemToMap` 恒失败（`MapItem.ItemId == 0`，oracle 无赋值点，T-19/T-25）
//!      ⇒ `DropItemDown` **恒 false** ⇒ 第二段**永不**清槽、永不写 `DeleteItem`。
//! 3. `dropItemList.Count > 0` ⇒ `SM_DELITEMS`（`PlayObject.Base.cs:1637-1646`），
//!    串 = `ItemName + '/' + MakeIndex + '/'`；第一段只填 `MakeIndex` ⇒ 线上是 `"/MakeIndex/"`。
//!
//! **落地/清空行为登记（T-82）**：由于 oracle 的 `AddItemToMap` 恒失败，玩家死亡**不会真正
//! 掉落**第二段的装备，也不会清它们的槽；唯一可达的清槽是 `ItemDesc & 8` 的绑定件。
//! 这与「玩家死亡掉装备」的直觉不同，但按硬约束③照抄。
use mir2_formula::drop::{ItemCatalog, VecItemCatalog};
use mir2_shared::hutil;

use crate::entity::USE_ITEM_SLOTS;
use crate::world::World;

/// `PlayObject.DropUseItems`（`PlayObject.Message.cs:2152-2228`）的世界侧执行体。
///
/// 返回删除列表 `[(槽位, ItemName, MakeIndex)]`——`ItemName` 第一段恒空串（oracle 的
/// `DeleteItem.ItemName` 缺省 `null`，`SendDelItemList` 里等价空串）。
pub(crate) fn drop_use_items_on_death(
    world: &mut World,
    actor_id: i32,
) -> Vec<crate::world::UseItemDeletion> {
    let Some(e) = world.entities.get(actor_id) else {
        return Vec::new();
    };
    // `:2156 if (AngryRing || NoDropUseItem) return;`
    if e.angry_ring || e.no_drop_use_item {
        return Vec::new();
    }
    // `:2183 nRate = PvpLevel() > 2 ? DieRedDropUseItemRate : DieDropUseItemRate`。
    let pvp = World::pvp_level(e);
    let n_rate = if pvp > 2 {
        world.die_red_drop_use_item_rate
    } else {
        world.die_drop_use_item_rate
    };
    // `:2194 int dropWide = HUtil32._MIN(Config.DropItemRage, 3)`——只作为
    // `DropItemDown → GetDropPosition` 的散布半径；`AddItemToMap` 恒失败（T-82）⇒ 无消费者。
    // 保留计算以记录口径（`_` 前缀避免 dead-code 噪声）。
    let _drop_wide = hutil::min_i32(world.drop_item_rage, 3);
    let disable = world.disable_take_off_keys.clone();
    // 物品目录按 `Index`（1-based）查表；`mem::take` 避免与 `world.rng`/实体同时可变借用
    // （与 `World::spawn_monster_now` 的既有写法同口径）。
    let catalog_items = std::mem::take(&mut world.item_catalog);
    let catalog = VecItemCatalog {
        items: &catalog_items,
    };
    let mut deleted: Vec<crate::world::UseItemDeletion> = Vec::new();

    // ---- 第 1 段（`:2159-2179`，零 RNG）----
    for slot in 0..USE_ITEM_SLOTS {
        let Some(cell) = world.entities.get(actor_id).map(|e| e.use_items[slot]) else {
            break;
        };
        if cell.index == 0 {
            continue;
        }
        let Some(std_item) = catalog.get_std_item(cell.index) else {
            continue;
        };
        if std_item.item_desc & 8 != 0 {
            // `dropItemList.Add(new DeleteItem { MakeIndex = … })`（ItemName 缺省 null）。
            deleted.push((slot, String::new(), cell.make_index));
            if let Some(e) = world.entities.get_mut(actor_id) {
                // `UseItems[i].Index = 0`（MakeIndex/Dura/DuraMax/Desc 均保留）。
                e.use_items[slot].index = 0;
            }
        }
    }

    // ---- 第 2 段（`:2183-2214`）：逐槽 `Random(nRate)`，空槽也抽 ----
    for slot in 0..USE_ITEM_SLOTS {
        if world.rng.random_below(n_rate) != 0 {
            continue;
        }
        let Some(cell) = world.entities.get(actor_id).map(|e| e.use_items[slot]) else {
            break;
        };
        // `if (UseItems[i] != null && M2Share.InDisableTakeOffList(UseItems[i].Index)) continue;`
        // Rust 槽恒存在（`null` 等价 `index == 0`）；`InDisableTakeOffList(Index)` 查
        // `DisableTakeOffList.ContainsKey(Index - 1)`。
        if cell.index > 0
            && disable
                .iter()
                .any(|&k| u32::from(k) == u32::from(cell.index) - 1)
        {
            continue;
        }
        // `DropItemDown(UseItems[i], …)`：`null`（`index == 0`）⇒ 立即 false（零 RNG）。
        if cell.index == 0 {
            continue;
        }
        let Some(std_item) = catalog.get_std_item(cell.index) else {
            continue;
        };
        // `BaseObject.cs:541-548`：`ushort idura = (ushort)(userItem.Dura - 2000);`——
        // **u16 回绕**（1..1999 得 63537..65535），随后的 `if (idura <= 0) idura = 0;`
        // 对 `ushort` 只在恰 0 时成立、赋值 0 是无操作 ⇒ 等价 `wrapping_sub(2000)`。
        // **不得**用 `saturating_sub`（会把 1..1999 错误地压成 0）。
        if std_item.std_mode == 40 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                let dura = e.use_items[slot].dura.wrapping_sub(2000);
                // `if (idura <= 0) idura = 0;` 对 `ushort` 只在恰 0 时成立，且赋值 0 无操作
                // ⇒ 回绕值原样写回（`Dura == 2000` 得 0，与 oracle 相同）。
                e.use_items[slot].dura = dura;
            }
        }
        // `if (stdItem.StdMode == 45) mapItem.Looks = GetRandomLook(mapItem.Looks, stdItem.Shape);`
        // `M2Share.GetRandomLook`（`M2Share.cs:524-527`）= `nBaseLook + Random(nRage)`。
        if std_item.std_mode == 45 {
            let _looks = u32::from(std_item.looks)
                + (world.rng.random_below(i32::from(std_item.shape)) as u32);
        }
        // `GetDropPosition(CurrX, CurrY, nScatterRange, …)`：纯格子扫描，零 RNG（`BaseObject.cs:459`）。
        // `Envir.AddItemToMap` 恒 false（`MapItem.ItemId == 0`）⇒ 本槽不清、不记 DeleteItem。
    }

    world.item_catalog = catalog_items;
    deleted
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, UseItemSlot, ACTOR_RACE_ANIMAL, ACTOR_RACE_PLAY};
    use mir2_data::models::StdItem;
    use mir2_shared::rng::RandomNumber;

    fn player(id: i32, x: i32, y: i32) -> Entity {
        let mut e = Entity::new(id, format!("p{id}"), x, y);
        e.race = ACTOR_RACE_PLAY;
        e.hp = 100;
        e.max_hp = 100;
        e
    }

    fn slot(make_index: i32, index: u16, dura: u16, desc: [u8; 14]) -> UseItemSlot {
        UseItemSlot {
            make_index,
            index,
            dura,
            dura_max: 1000,
            desc,
        }
    }

    fn std_item(index: u8, std_mode: u8, shape: u8, looks: u16, item_desc: u8) -> StdItem {
        StdItem {
            name: format!("item{index}"),
            std_mode,
            shape,
            looks,
            item_desc,
            ..StdItem::default()
        }
    }

    fn draws_after(seed: i32, maxes: &[i32]) -> RandomNumber {
        let mut c = RandomNumber::with_seed(seed);
        for &m in maxes {
            let _ = c.random_below(m);
        }
        c
    }

    /// 13 槽逐槽 `Random(nRate)`（**空槽也抽**）；第一段只清 `ItemDesc & 8`，第二段因
    /// `AddItemToMap` 恒失败**不清槽**；`StdMode 45` 多一抽 `Random(Shape)`。
    #[test]
    fn drop_use_items_draws_thirteen_plus_shape_and_clears_only_item_desc_8() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        // index 1 = 绑定件（ItemDesc & 8），index 2 = StdMode 45，index 3 = StdMode 40。
        w.item_catalog = vec![
            std_item(1, 0, 0, 0, 8),
            std_item(2, 45, 4, 100, 0),
            std_item(3, 40, 0, 0, 0),
        ];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.use_items[0] = slot(11, 1, 500, [1; 14]);
            e.use_items[1] = slot(22, 2, 500, [2; 14]);
            e.use_items[2] = slot(33, 3, 2500, [3; 14]);
        }
        // nRate = 1 ⇒ 每槽 `Random(1) == 0` ⇒ 13 槽全部进 DropItemDown。
        w.set_death_drop_config(1, 1, 3, Vec::new());
        let seed = 12345;
        w.rng = RandomNumber::with_seed(seed);
        let deleted = drop_use_items_on_death(&mut w, p);
        // 13 抽 + StdMode45 的 `Random(Shape=4)`；StdMode40 零抽。
        let mut maxes = vec![1; 13];
        maxes.push(4);
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &maxes)),
            "取数序：13×Random(nRate) →（StdMode45）Random(Shape)"
        );
        // 第一段：绑定件清 Index、留 MakeIndex，并进删除列表（ItemName 空）。
        assert_eq!(deleted, vec![(0usize, String::new(), 11)]);
        let e = w.entities.get(p).unwrap();
        assert_eq!(e.use_items[0].index, 0, "ItemDesc&8 ⇒ Index=0");
        assert_eq!(e.use_items[0].make_index, 11, "MakeIndex 保留");
        assert_eq!(e.use_items[1].index, 2, "第二段不落地 ⇒ 不清槽（T-82）");
        assert_eq!(e.use_items[2].index, 3, "第二段不落地 ⇒ 不清槽（T-82）");
        assert_eq!(
            e.use_items[2].dura, 500,
            "StdMode40 ⇒ Dura -= 2000（2500→500）"
        );
    }

    /// 空槽也逐槽抽 13 次（`Random(nRate)` 在 `UseItems[i] == null` 之前）。
    #[test]
    fn drop_use_items_draws_for_empty_slots_too() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.item_catalog = vec![std_item(1, 0, 0, 0, 0)];
        w.set_death_drop_config(100, 100, 3, Vec::new());
        let seed = 424242;
        w.rng = RandomNumber::with_seed(seed);
        let deleted = drop_use_items_on_death(&mut w, p);
        assert!(deleted.is_empty());
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &[100; 13])),
            "空槽仍逐槽抽 Random(nRate)（13 次）"
        );
    }

    /// 首门：`AngryRing || NoDropUseItem` ⇒ **零抽**、零删除。
    #[test]
    fn drop_use_items_angry_ring_and_no_drop_gate_do_not_draw() {
        for (angry, no_drop) in [(true, false), (false, true)] {
            let mut w = World::new(32, 32);
            let p = w.add_entity(player(0, 10, 10));
            w.item_catalog = vec![std_item(1, 0, 0, 0, 8)];
            {
                let e = w.entities.get_mut(p).unwrap();
                e.angry_ring = angry;
                e.no_drop_use_item = no_drop;
                e.use_items[0] = slot(11, 1, 500, [0; 14]);
            }
            w.set_death_drop_config(1, 1, 3, Vec::new());
            let seed = 777;
            w.rng = RandomNumber::with_seed(seed);
            let deleted = drop_use_items_on_death(&mut w, p);
            assert!(deleted.is_empty(), "首门 ⇒ 无删除列表");
            assert_eq!(
                format!("{:?}", w.rng),
                format!("{:?}", RandomNumber::with_seed(seed)),
                "首门 ⇒ 零抽"
            );
            assert_eq!(w.entities.get(p).unwrap().use_items[0].index, 1);
        }
    }

    /// `InDisableTakeOffList(Index)` = `ContainsKey(Index - 1)`：命中后该槽不进
    /// `DropItemDown`（不再有 `StdMode45` 的 `Random(Shape)`），但 `Random(nRate)` 照抽。
    #[test]
    fn drop_use_items_disable_take_off_key_skips_drop_down_only() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.item_catalog = vec![std_item(1, 45, 9, 100, 0)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        // Index=1 ⇒ 键 0 命中；nRate=1 ⇒ 每槽必进 Random==0 分支。
        w.set_death_drop_config(1, 1, 3, vec![0]);
        let seed = 999;
        w.rng = RandomNumber::with_seed(seed);
        drop_use_items_on_death(&mut w, p);
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &[1; 13])),
            "禁止取下列表命中 ⇒ 无 Random(Shape)；Random(nRate) 仍 13 次"
        );
    }

    /// `PvpLevel() > 2` 用 `DieRedDropUseItemRate`（`PkPoint / 100 > 2`）。
    #[test]
    fn drop_use_items_red_rate_for_pvp_level_three() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.entities.get_mut(p).unwrap().pk_point = 300; // PvpLevel = 3
        w.item_catalog = vec![std_item(1, 0, 0, 0, 0)];
        w.set_death_drop_config(999, 1, 3, Vec::new());
        let seed = 31;
        w.rng = RandomNumber::with_seed(seed);
        drop_use_items_on_death(&mut w, p);
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &[1; 13])),
            "红名 ⇒ Random(DieRedDropUseItemRate=1)"
        );
    }

    /// **第二段 `DropUseItems`（`KillTargetTrigger`）**：部署件 `KillByHumanDropUseItem=1`
    /// ⇒ 玩家击杀者的 `RM_PLAYERKILLMONSTER` 尾再调一次 `killObject.DropUseItems(0)`
    /// （`PlayObject.cs:1787-1798`）⇒ 同一死亡**两轮** 13 抽（第二轮的删除列表为空）。
    #[test]
    fn settle_deaths_pvp_calls_drop_use_items_twice() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        let killer = w.add_entity(player(1, 11, 10));
        w.item_catalog = vec![std_item(1, 45, 4, 100, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.exp_hitter = Some(killer);
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        w.set_death_drop_kill_config(true, true);
        let seed = 555;
        w.rng = RandomNumber::with_seed(seed);
        let rec = w.tick();
        // `ItemDesc&8` 的 StdMode45 在第一段就被清 `Index` ⇒ 两轮都只有逐槽 `Random(1)`，
        // 没有 `Random(Shape)`（第二段 `GetStdItem(0)` 不可达）。
        let maxes = vec![1; 26];
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &maxes)),
            "PvP ⇒ 两轮逐槽 Random(nRate)（第二轮零 Shape）"
        );
        assert_eq!(rec.death_use_item_drops.len(), 1, "删除列表只由第一轮产生");
        assert_eq!(
            rec.death_use_item_drops[0].1,
            vec![(0usize, String::new(), 11)]
        );

        // 对照：`KillByHumanDropUseItem=false` ⇒ 只一轮。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        let killer = w.add_entity(player(1, 11, 10));
        w.item_catalog = vec![std_item(1, 45, 4, 100, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.exp_hitter = Some(killer);
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        w.set_death_drop_kill_config(false, true);
        w.rng = RandomNumber::with_seed(seed);
        let _ = w.tick();
        let maxes = vec![1; 13];
        assert_eq!(
            format!("{:?}", w.rng),
            format!("{:?}", draws_after(seed, &maxes)),
            "human 门关 ⇒ 只 DieDropItems 一轮"
        );
    }

    /// `settle_deaths` 的三道产生门：`ExpHitter` 归属 + `!FightZone && !Fight3Zone`。
    #[test]
    fn settle_deaths_player_drop_gates_match_die_drop_items() {
        // 无 ExpHitter ⇒ `KillFunc` 不发 RM_DIEDROPITEM ⇒ 零动作。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.item_catalog = vec![std_item(1, 0, 0, 0, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        let rec = w.tick();
        assert!(rec.death_use_item_drops.is_empty());
        assert_eq!(w.entities.get(p).unwrap().use_items[0].index, 1);

        // ExpHitter = 无主怪 ⇒ 不发。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        let killer = w.add_entity({
            let mut m = Entity::new(0, "mob".to_string(), 11, 10);
            m.race = ACTOR_RACE_ANIMAL;
            m
        });
        w.item_catalog = vec![std_item(1, 0, 0, 0, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.exp_hitter = Some(killer);
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        let rec = w.tick();
        assert!(
            rec.death_use_item_drops.is_empty(),
            "无主怪 ⇒ 不发 RM_DIEDROPITEM"
        );

        // ExpHitter = 玩家 ⇒ 发。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        let killer = w.add_entity(player(0, 11, 10));
        w.item_catalog = vec![std_item(1, 0, 0, 0, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.exp_hitter = Some(killer);
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        let rec = w.tick();
        assert_eq!(rec.death_use_item_drops.len(), 1);
        assert_eq!(rec.death_use_item_drops[0].0, p);
        assert_eq!(
            rec.death_use_item_drops[0].1,
            vec![(0usize, String::new(), 11)]
        );

        // FightZone：同构但地图门关。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        let killer = w.add_entity(player(0, 11, 10));
        w.item_catalog = vec![std_item(1, 0, 0, 0, 8)];
        {
            let e = w.entities.get_mut(p).unwrap();
            e.hp = 0;
            e.exp_hitter = Some(killer);
            e.use_items[0] = slot(11, 1, 500, [0; 14]);
        }
        w.set_map_fight_zone(0, false, true);
        w.set_death_drop_config(1, 1, 3, Vec::new());
        let rec = w.tick();
        assert!(rec.death_use_item_drops.is_empty(), "Fight3Zone ⇒ 不发");
    }

    /// **I2 判据（StdMode 40 耐久口径）**：oracle `BaseObject.cs:541-548` 是
    /// `ushort idura = (ushort)(Dura - 2000)`——**u16 回绕**，不是 `max(0, …)`。
    /// `Dura=1` ⇒ `1u16.wrapping_sub(2000) = 63537`；`Dura=2000` ⇒ 0。
    ///
    /// 红检：改回 `saturating_sub(2000)`（或 `max(0, Dura-2000)`）⇒ 63537 断言必红。
    #[test]
    fn drop_use_items_stdmode40_dura_wraps_u16() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.set_item_catalog(vec![std_item(1, 40, 0, 0, 0)]);
        {
            let e = w.entities.get_mut(p).unwrap();
            e.use_items[0] = slot(11, 1, 1, [0; 14]);
        }
        // nRate=1 ⇒ 每槽必进 `DropItemDown` 支（StdMode40 的耐久扣减照做）。
        w.set_death_drop_config(1, 1, 3, Vec::new());
        drop_use_items_on_death(&mut w, p);
        assert_eq!(
            w.entities.get(p).unwrap().use_items[0].dura,
            1u16.wrapping_sub(2000),
            "Dura=1 ⇒ u16 回绕 63537（oracle `(ushort)(1 - 2000)`）"
        );
        assert_eq!(
            w.entities.get(p).unwrap().use_items[0].dura,
            63537,
            "回绕的具体值"
        );

        // 恰 2000 ⇒ 0（`if (idura <= 0)` 的唯一成立点）。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(0, 10, 10));
        w.set_item_catalog(vec![std_item(1, 40, 0, 0, 0)]);
        {
            let e = w.entities.get_mut(p).unwrap();
            e.use_items[0] = slot(11, 1, 2000, [0; 14]);
        }
        w.set_death_drop_config(1, 1, 3, Vec::new());
        drop_use_items_on_death(&mut w, p);
        assert_eq!(w.entities.get(p).unwrap().use_items[0].dura, 0);
    }

    /// **M1 判据**：第二段的 `!NoItem || !Envir.Flag.NoDropItem` 取**击杀者**
    /// （`PlayObject.cs:1793` 的 `this`），不是死者。
    ///
    /// A：死者 `NoItem=true` + 图 `NoDropItem=true`、击杀者 `NoItem=false` ⇒ 门**开**
    /// （26 抽 = 两轮）；旧实现（读死者）会误判为门关（13 抽）⇒ 红。
    /// B：击杀者 `NoItem=true` + 图 `NoDropItem=true`、死者 `NoItem=false` ⇒ 门**关**
    /// （13 抽）；旧实现会误判为门开（26 抽）⇒ 红。
    #[test]
    fn settle_deaths_kill_trigger_no_item_gate_reads_killer() {
        let check = |dead_no_item: bool, killer_no_item: bool, maxes: Vec<i32>, why: &str| {
            let mut w = World::new(32, 32);
            let p = w.add_entity(player(0, 10, 10));
            // 注意：`player(1, …)` 会把 id=1 直接插进 store（覆盖死者）——两实体必须都用
            // id=0 走 `add_entity` 的 `alloc_id`，否则「死者/击杀者」是同一个实体。
            let killer = w.add_entity(player(0, 11, 10));
            assert_ne!(p, killer, "死者与击杀者必须是两个实体");
            {
                let e = w.entities.get_mut(p).unwrap();
                e.hp = 0;
                e.exp_hitter = Some(killer);
                e.no_item = dead_no_item;
            }
            w.entities.get_mut(killer).unwrap().no_item = killer_no_item;
            w.set_map_no_drop_item(0, true);
            w.set_death_drop_config(1, 1, 3, Vec::new());
            w.set_death_drop_kill_config(true, true);
            let seed = 901;
            w.rng = RandomNumber::with_seed(seed);
            let _ = w.tick();
            assert_eq!(
                format!("{:?}", w.rng),
                format!("{:?}", draws_after(seed, &maxes)),
                "{}",
                why
            );
        };
        check(
            true,
            false,
            vec![1; 26],
            "死者 NoItem 不参与：击杀者 NoItem=false ⇒ 第二段仍跑（两轮）",
        );
        check(
            false,
            true,
            vec![1; 13],
            "击杀者 NoItem=true + 图 NoDropItem=true ⇒ 第二段不跑（一轮）",
        );
    }
}
