//! 世界 tick 循环 —— 对齐 C# `GameSvr/Word/WorldServer.cs::ProcessHumans` 的 **200ms** 节拍
//!（`WorldServer.cs:487`：`if ((HUtil32.GetTickCount() - ProcessLoadPlayTick) > 200)`）。
//!
//! 本骨架的边界（与设计文档 §4.2 一致）：
//! - **单线程串行**：`World` 只在创建它的线程上推进；`tick()` 每次校验线程 id（判据③）；
//! - **确定性**：时钟是**虚拟时钟**（`clock_ms`，每 tick +200），模拟逻辑不读墙钟；
//!   实体处理顺序 = 槽位顺序（= 插入顺序），命令按 FIFO 处理；
//! - 不实现 M1 的边界服务（登录/网关/DBSrv），只做世界内的 tick/实体/地图/AOI/会话进出。

use std::collections::VecDeque;
use std::thread::ThreadId;

use crate::aoi::{search_view_range, search_view_range_death};
use crate::entity::{Entity, EntityStore};
use crate::map::MapGrid;

/// C# `ProcessHumans` 的节拍（毫秒）。
pub const TICK_INTERVAL_MS: i64 = 200;

/// 世界 tick 里排队执行的命令（本骨架只有"走一步"与"进出世界"）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// 请求走一步（对应客户端 `CM_WALK` 的落点；服务端做格子校验后更新坐标）
    Walk { actor_id: i32, dx: i32, dy: i32 },
    /// 进入世界（会话建立后）
    Enter { actor_id: i32 },
    /// 离开世界（小退/断线）
    Leave { actor_id: i32 },
}

/// 一个 tick 的处理记录（判据①：断言"谁、按什么顺序"被处理）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickRecord {
    pub tick_seq: u64,
    pub clock_ms: i64,
    /// 按处理顺序记录的 `(actor_id, 结果)`；`result` = 移动是否成功 / 进出是否成功
    pub processed: Vec<(i32, bool)>,
    /// 本 tick 执行了 AOI 刷新的实体（顺序 = 槽位顺序）
    pub aoi_order: Vec<i32>,
}

/// 对拍/输出用的实体快照：`(id, x, y, 视野列表[(目标 id, 标志)])`。
pub type EntitySnapshot = (i32, i32, i32, Vec<(i32, u8)>);

pub struct World {
    pub map: MapGrid,
    pub entities: EntityStore,
    /// 虚拟时钟（毫秒），每 tick 前进 `TICK_INTERVAL_MS`
    pub clock_ms: i64,
    pub tick_seq: u64,
    /// 帧内待处理命令（FIFO）
    pending: VecDeque<Command>,
    /// 判据③：tick 必须始终跑在同一个线程上
    owner_thread: Option<ThreadId>,
    /// C# `SystemShare.Config.ClearDropOnFloorItemTime`（地面物品清理时间）
    pub clear_drop_on_floor_item_time: i64,
    /// 最近一次 tick 的记录
    pub last_tick: TickRecord,
}

impl World {
    pub fn new(width: i32, height: i32) -> Self {
        World {
            map: MapGrid::new(width, height),
            entities: EntityStore::new(),
            clock_ms: 0,
            tick_seq: 0,
            pending: VecDeque::new(),
            owner_thread: None,
            clear_drop_on_floor_item_time: 5 * 60 * 1000,
            last_tick: TickRecord::default(),
        }
    }

    /// 入队一条命令（会话线程 → 世界线程的交接点；本骨架里同线程、仍走队列以保证顺序）。
    pub fn enqueue(&mut self, cmd: Command) {
        self.pending.push_back(cmd);
    }

    /// 会话进入世界：登记实体并放入地图格子。
    pub fn add_entity(&mut self, mut entity: Entity) -> i32 {
        if entity.id <= 0 {
            entity.id = self.entities.alloc_id();
        }
        let id = self.entities.insert(entity);
        self.enqueue(Command::Enter { actor_id: id });
        id
    }

    /// 走一步（入队，tick 时按序生效）。
    pub fn walk(&mut self, actor_id: i32, dx: i32, dy: i32) {
        self.enqueue(Command::Walk { actor_id, dx, dy });
    }

    /// 离开世界（入队）。
    pub fn leave(&mut self, actor_id: i32) {
        self.enqueue(Command::Leave { actor_id });
    }

    /// 推进一个 tick（固定 200ms；不做 sleep —— 调度归调用方）。
    ///
    /// 顺序（全部确定性）：
    /// 1. 校验单线程（判据③）；
    /// 2. `clock_ms += 200`、`tick_seq += 1`；
    /// 3. 按 FIFO 处理本 tick 之前的全部命令；
    /// 4. 按**槽位顺序**对每个实体做 `search_view_range`（死者走 `search_view_range_death`）。
    pub fn tick(&mut self) -> TickRecord {
        let current = std::thread::current().id();
        match self.owner_thread {
            None => self.owner_thread = Some(current),
            Some(t) => assert_eq!(
                t, current,
                "世界 tick 必须始终在同一线程上串行执行（设计文档 §4.2：禁止并行）"
            ),
        }

        self.clock_ms += TICK_INTERVAL_MS;
        self.tick_seq += 1;
        let mut record = TickRecord {
            tick_seq: self.tick_seq,
            clock_ms: self.clock_ms,
            processed: Vec::new(),
            aoi_order: Vec::new(),
        };

        // 3. 命令 FIFO
        while let Some(cmd) = self.pending.pop_front() {
            let (actor_id, ok) = match cmd {
                Command::Enter { actor_id } => {
                    let ok = self
                        .entities
                        .enter_world(actor_id, &mut self.map, self.clock_ms);
                    (actor_id, ok)
                }
                Command::Leave { actor_id } => {
                    let ok = self.entities.leave_world(actor_id, &mut self.map);
                    if ok {
                        self.entities.remove(actor_id);
                    }
                    (actor_id, ok)
                }
                Command::Walk { actor_id, dx, dy } => {
                    let ok = self.try_step(actor_id, dx, dy);
                    (actor_id, ok)
                }
            };
            record.processed.push((actor_id, ok));
        }

        // 4. AOI 刷新（槽位顺序；先收集 id 再逐个刷新，避免遍历中改列表）
        let ids: Vec<i32> = self.entities.iter().map(|e| e.id).collect();
        for id in ids {
            let (death, _) = match self.entities.get(id) {
                Some(e) => (e.death, ()),
                None => continue,
            };
            if death {
                search_view_range_death(
                    &mut self.map,
                    &mut self.entities,
                    id,
                    self.clock_ms,
                    self.clear_drop_on_floor_item_time,
                );
            } else {
                search_view_range(&mut self.map, &mut self.entities, id, self.clock_ms);
            }
            record.aoi_order.push(id);
        }

        self.last_tick = record.clone();
        record
    }

    /// 单步移动：格子校验 → 更新坐标（对应 `BaseObject.Walk` 的服务端部分 +
    /// `Envirnoment.MoveToMovingObject`）。
    ///
    /// C# 顺序：先由 `CanWalk`/`MoveToMovingObject` 判阻挡，成功才改 `CurrX/CurrY`。
    fn try_step(&mut self, actor_id: i32, dx: i32, dy: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (fx, fy) = (e.x, e.y);
        let (tx, ty) = (fx + dx, fy + dy);
        if !self.map.cell_match(tx, ty) {
            return false;
        }
        // 目标格内的"阻挡者"（C#：!Ghost && !Death && !FixedHideMode && !ObMode）
        let blockers: Vec<i32> = {
            let (cell, _) = self.map.get_cell_info(tx, ty);
            cell.obj_list
                .iter()
                .filter(|o| o.actor_object)
                .map(|o| o.cell_obj_id)
                .collect()
        };
        let blocking: Vec<i32> = blockers
            .into_iter()
            .filter(|id| {
                self.entities
                    .get(*id)
                    .is_some_and(|o| !o.ghost && !o.death && !o.fixed_hide_mode && !o.ob_mode)
            })
            .collect();
        let moved = self.map.move_object(
            crate::map::MoveRequest {
                from_x: fx,
                from_y: fy,
                actor_id,
                to_x: tx,
                to_y: ty,
                ignore_block: false,
                add_time: self.clock_ms,
            },
            |cid| blocking.contains(&cid),
        );
        if moved {
            if let Some(e) = self.entities.get_mut(actor_id) {
                e.x = tx;
                e.y = ty;
            }
        }
        moved
    }

    /// 对拍用快照：全部实体的 `(id, x, y, 视野列表)`，**按 id 升序**（与两侧顺序无关）。
    pub fn snapshot_sorted(&self) -> Vec<EntitySnapshot> {
        let mut ids = self.entities.ids_sorted();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| {
                self.entities.get(id).map(|e| {
                    (
                        e.id,
                        e.x,
                        e.y,
                        crate::aoi::visible_snapshot(&self.entities, id),
                    )
                })
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world_with(entities: &[(i32, i32, i32)]) -> World {
        let mut w = World::new(64, 64);
        for (id, x, y) in entities {
            let mut e = Entity::new(*id, format!("e{id}"), *x, *y);
            e.id = *id;
            w.add_entity(e);
        }
        w
    }

    #[test]
    fn tick_advances_clock_by_200ms() {
        let mut w = World::new(16, 16);
        assert_eq!(w.clock_ms, 0);
        let r1 = w.tick();
        assert_eq!(r1.clock_ms, 200);
        assert_eq!(r1.tick_seq, 1);
        let r2 = w.tick();
        assert_eq!(r2.clock_ms, 400);
    }

    /// 判据①：tick 的处理顺序 = 命令 FIFO + 实体槽位顺序，且可复现。
    #[test]
    fn tick_order_is_deterministic_and_documented() {
        let mut w = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w.walk(2, 1, 0);
        w.walk(1, 0, 1);
        let r = w.tick();
        // 命令 FIFO：add_entity 各入队一条 Enter（1、2、3），随后才是两条 Walk（2 在前、1 在后）
        assert_eq!(
            r.processed,
            vec![(1, true), (2, true), (3, true), (2, true), (1, true)],
            "命令必须按 FIFO 处理"
        );
        // AOI 刷新按槽位顺序（= 插入顺序）
        assert_eq!(r.aoi_order, vec![1, 2, 3]);

        // 同一组操作 → 同一顺序（可复现）
        let mut w2 = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w2.walk(2, 1, 0);
        w2.walk(1, 0, 1);
        let r2 = w2.tick();
        assert_eq!(r.processed, r2.processed);
        assert_eq!(r.aoi_order, r2.aoi_order);
    }

    /// 判据①：AOI 边界——恰好 ViewRange 处可见、再远一格不可见。
    #[test]
    fn aoi_boundary_at_view_range() {
        // ViewRange 默认 5：dx=5 可见；dx=6 不可见
        let mut w = world_with(&[(1, 30, 30), (2, 35, 30), (3, 36, 30)]);
        w.tick();
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let targets: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert!(targets.contains(&2), "dx=ViewRange 必须可见（闭区间）");
        assert!(!targets.contains(&3), "dx=ViewRange+1 不可见");
    }

    /// 判据①：X 外层/Y 内层的扫描顺序 —— 用"先看到的先追加"体现：
    /// 观察者自己（30,30）先被扫到，(x=33,y=32) 在 (x=34,y=31) 之前被追加。
    ///
    /// 注意：C# 的 AOI 过滤器**没有排除观察者自己**（`BaseObject.ViewRange.cs:108-116`
    /// 没有 `baseObject == this` 判断），所以自己的 id 也会进 `VisibleActors`。
    /// 这是真实行为（行为等价优先于修 bug），由 C# 金标准确认；此处按同样语义断言。
    #[test]
    fn aoi_scan_order_is_x_outer_y_inner() {
        let mut w = world_with(&[(1, 30, 30), (10, 33, 32), (11, 34, 31)]);
        w.tick();
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let order: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert_eq!(
            order,
            vec![1, 10, 11],
            "扫描顺序：自己先入列（无自排除），随后 X 升序外层、Y 升序内层"
        );
    }

    /// 判据③：tick 在不同线程上执行必须 panic（拒绝并行）。
    #[test]
    fn tick_refuses_to_run_on_another_thread() {
        let mut w = World::new(8, 8);
        w.tick(); // 绑定当前线程
        let handle = std::thread::spawn(move || {
            let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| w.tick()));
            r.is_err()
        });
        assert!(
            handle.join().unwrap(),
            "tick 在别的线程上必须 panic（世界只能单线程串行）"
        );
    }

    /// 会话进出世界：进图后有格子、离开后格子空且实体移除。
    #[test]
    fn session_enter_leave_world() {
        let mut w = World::new(16, 16);
        let id = w.add_entity(Entity::new(0, "p", 5, 5));
        w.tick();
        assert!(w.entities.get(id).unwrap().add_to_mapped);
        assert_eq!(w.map.cell(5, 5).count(), 1);
        w.leave(id);
        w.tick();
        assert!(w.entities.get(id).is_none(), "离开世界后实体应从仓库移除");
        assert_eq!(w.map.cell(5, 5).count(), 0);
    }
}
