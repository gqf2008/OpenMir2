//! **L2-3 两阶段 tick 的区边界交换（构造级）**
//!
//! 两阶段驱动（L2-4 的生产接线）在每个 tick 这样做：
//!
//! ```text
//! 每区: tick_phase1() → 发布本区边界快照 → barrier1 → 读邻居快照 → tick_phase2() → barrier2
//! ```
//!
//! - **快照在 phase1 之后取** ⇒ 邻居读到的位置/HP 等是**本 tick 终态**；
//! - 快照槽按 `tick & 1` 双缓冲：barrier2 保证没人还在读上一槽，下一 tick 才能覆盖；
//! - 本模块只提供**数据与同步原语**；生产 `OutputAoi` 的跨区收件人/投递在 L2-4。
//!
//! 范围（L2-3）：只快照**可见性面**字段（id/位置/视野/名字）；帧字段在 L2-4 扩展。
//! 边界带 = 距区界 ≤ `halo_width`（全局 `view_range` 上界）的已落格实体。

use std::sync::{Mutex, PoisonError};

use mir2_world_bevy::World;

use crate::zone::{ZoneId, ZoneRect};

/// 一个边界实体的只读快照（可见性面字段；帧字段 L2-4 扩展）。
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HaloEntity {
    /// 业务 ActorId（跨区唯一，P2-3b Step 3 已按片分段）。
    pub actor_id: i32,
    /// 地图号（全局下标）。
    pub map_id: usize,
    /// 世界坐标（本 tick 终态）。
    pub x: i32,
    /// 世界坐标（本 tick 终态）。
    pub y: i32,
    /// 观察者视野半径（`PlayObject.cs:904` = 12；非玩家取缺省）。
    pub view_range: i32,
    /// 名字（帧面在 L2-4 需要；本步只用于快照自证）。
    pub name: String,
}

/// 某区某 tick 的边界带快照。
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct HaloSnapshot {
    /// 产生快照的 tick 序号（读端用它拒绝「上一 tick 的陈旧槽」）。
    pub tick: u64,
    /// 边界带实体（按 World 槽位顺序，确定性）。
    pub entities: Vec<HaloEntity>,
}

/// 从 World 抽某区（`map_id` + `rect`）的边界带快照。
///
/// 过滤：`add_to_mapped`（已落格）+ 地图匹配 + `rect.border_contains(width)`。
/// **必须在 phase1 之后调用**（此时实体状态是本 tick 终态）。
#[must_use]
pub fn snapshot_border(
    world: &World,
    map_id: usize,
    rect: ZoneRect,
    halo_width: i32,
    tick: u64,
) -> HaloSnapshot {
    let entities = world
        .entities
        .iter_ref()
        .filter(|e| {
            e.add_to_mapped && e.map_id == map_id && rect.border_contains(e.x, e.y, halo_width)
        })
        .map(|e| HaloEntity {
            actor_id: e.id,
            map_id: e.map_id,
            x: e.x,
            y: e.y,
            view_range: e.view_range,
            name: e.name.clone(),
        })
        .collect();
    HaloSnapshot { tick, entities }
}

/// 每区两个槽（`tick & 1` 双缓冲）的快照交换。
#[derive(Debug)]
pub struct HaloExchange {
    slots: Vec<Mutex<[Option<HaloSnapshot>; 2]>>,
}

impl HaloExchange {
    /// 按区数构造（每区两槽，启动期一次分配）。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            slots: (0..zone_count).map(|_| Mutex::new([None, None])).collect(),
        }
    }

    /// owner 区发布自己的本 tick 快照（只写自己的槽）。
    ///
    /// # Panics
    /// `zone` 越界 ⇒ panic（编程错误）。
    pub fn publish(&self, zone: ZoneId, snapshot: HaloSnapshot) {
        let slot = (snapshot.tick as usize) & 1;
        let mut slots = self.slots[zone.index()]
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slots[slot] = Some(snapshot);
    }

    /// 读某区**本 tick** 的快照；槽里是别的 tick（陈旧）⇒ `None`。
    ///
    /// 读端必须把 `None` 当作错误（邻居没发布/barrier 用错），不得静默当空快照——
    /// 否则「谁也不发」会被伪装成「没有可见实体」。
    #[must_use]
    pub fn read(&self, zone: ZoneId, tick: u64) -> Option<HaloSnapshot> {
        let slot = (tick as usize) & 1;
        let slots = self.slots[zone.index()]
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        slots[slot].clone().filter(|s| s.tick == tick)
    }
}

/// 每个 tick 的两道屏障（phase1 之后 / phase2 之后）。
#[derive(Debug)]
pub struct TickBarriers {
    after_phase1: std::sync::Barrier,
    after_phase2: std::sync::Barrier,
}

impl TickBarriers {
    /// 按参与区数构造。
    #[must_use]
    pub fn new(zone_count: usize) -> Self {
        Self {
            after_phase1: std::sync::Barrier::new(zone_count),
            after_phase2: std::sync::Barrier::new(zone_count),
        }
    }

    /// phase1 之后的屏障：所有区发布完自己的快照后才返回（随后可安全读邻居）。
    pub fn wait_after_phase1(&self) {
        self.after_phase1.wait();
    }

    /// phase2 之后的屏障：保证没人还在读上一槽，下一 tick 才能覆盖。
    pub fn wait_after_phase2(&self) {
        self.after_phase2.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mir2_world_bevy::{EntitySpawn, World};

    fn entity(id: i32, name: &str, x: i32, y: i32) -> EntitySpawn {
        let mut e = EntitySpawn::new(id, name.to_string(), x, y);
        e.race = 80;
        e
    }

    /// **快照牙**：边界带只含「已落格 + 正确地图 + 距边 ≤ width」的实体；坐标是本 tick 值。
    #[test]
    fn snapshot_only_includes_this_zone_border_band() {
        let mut world = World::new(20, 20);
        world.add_entity(entity(1, "edge", 9, 5)); // 带内（距右边界 0）
        world.add_entity(entity(2, "deep", 4, 5)); // 距边 5 > width 3 ⇒ 带外
        world.add_entity(entity(3, "outside", 10, 5)); // 矩形外（rect 到 9）
                                                       // `add_entity` 只入队 `Command::Enter`；跑一个 tick 才落图（`add_to_mapped=true`）。
        world.tick();
        let rect = ZoneRect::new(0, 0, 9, 19);
        let snap = snapshot_border(&world, 0, rect, 3, 7);
        assert_eq!(snap.tick, 7);
        assert_eq!(
            snap.entities.iter().map(|e| e.actor_id).collect::<Vec<_>>(),
            vec![1],
            "只含带内已落格实体"
        );
    }

    /// **交换牙**：按 tick 双缓冲；读别的 tick ⇒ `None`（不得把陈旧槽伪装成空快照）。
    #[test]
    fn exchange_is_double_buffered_and_rejects_stale_ticks() {
        let ex = HaloExchange::new(2);
        let zone = ZoneId::new(1);
        ex.publish(
            zone,
            HaloSnapshot {
                tick: 4,
                entities: vec![HaloEntity {
                    actor_id: 9,
                    map_id: 0,
                    x: 1,
                    y: 2,
                    view_range: 12,
                    name: "m".to_string(),
                }],
            },
        );
        assert!(ex.read(zone, 4).is_some(), "本 tick 可读");
        assert!(ex.read(zone, 5).is_none(), "其它 tick 的槽必须拒绝");
        assert!(ex.read(ZoneId::new(0), 4).is_none(), "未发布区 ⇒ None");
        // tick 5 发布会写另一个槽；tick 4 的槽仍在（双缓冲）。
        ex.publish(
            zone,
            HaloSnapshot {
                tick: 5,
                entities: vec![],
            },
        );
        assert!(ex.read(zone, 4).is_some(), "tick 4 槽未被 tick 5 覆盖");
        assert_eq!(
            ex.read(zone, 5),
            Some(HaloSnapshot {
                tick: 5,
                entities: vec![]
            })
        );
    }

    /// **屏障牙**：2 区内两道屏障各放行 2 个参与者（不死锁）。
    #[test]
    fn barriers_release_all_participants_twice() {
        let barriers = std::sync::Arc::new(TickBarriers::new(2));
        let mut handles = Vec::new();
        for _ in 0..2 {
            let b = std::sync::Arc::clone(&barriers);
            handles.push(std::thread::spawn(move || {
                b.wait_after_phase1();
                b.wait_after_phase2();
            }));
        }
        for h in handles {
            h.join().expect("屏障线程");
        }
    }
}
