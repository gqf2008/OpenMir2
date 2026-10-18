//! 会话进世界 —— 状态机与实体生命周期。
//!
//! 行为基准（C# 侧）：
//! - 阶段序列见设计文档 §4.4（未连接→已连接→登录中→已选服→选角中→进世界中→断线/小退）；
//!   本模块只覆盖**世界侧**能观测到的部分：进图 / 切图 / 掉线 / 小退（其余阶段归 M1 边界服务）。
//! - 进图：`Envirnoment.AddMapObject`（格子登记）+ `AddObject`（地图对象表）；
//! - 切图：`TransMap`/`MapRoute` 的落地动作 = 旧图 `DeleteFromMap` → 新图 `AddMapObject`，
//!   并把新坐标写回；**视野列表整体失效**（旧图对象一律不再可见，C# 侧 `Envir` 变化后
//!   `SearchViewRange` 会把它们全部清出）；
//! - 掉线：断线 = 离开世界（本骨架不做存档，落库语义归 M1/DBSrv）；
//! - 小退：`CM_SOFTCLOSE` 语义 = 离开世界并允许再次进入（同一角色名、新的 `ActorId`）。
//!
//! 与 C# 的差异（登记在报告里）：C# 的小退会**存档后再回选人界面**，本骨架只做世界侧
//! 的"离开 + 可再进"，存档/回选人的状态由 M1 持有。

use crate::entity::{Entity, ACTOR_RACE_PLAY};
use crate::map::{CellType, MapGrid};
use crate::world::World;

/// 世界侧会话阶段（世界能观测到的子集）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldStage {
    /// 尚未进图
    NotInWorld,
    /// 已在某张地图上
    InWorld,
    /// 已离开（掉线/小退）
    Left,
}

/// 会话在世界侧的登记项：角色名 ↔ 当前实体 id ↔ 当前地图。
#[derive(Clone, Debug)]
pub struct Session {
    pub chr_name: String,
    pub actor_id: Option<i32>,
    pub map_id: usize,
    pub stage: WorldStage,
    /// 小退计数（用"小退再进"的场景断言：再进会得到新的 actor_id）
    pub soft_close_count: u32,
}

impl Session {
    pub fn new(chr_name: impl Into<String>) -> Self {
        Session {
            chr_name: chr_name.into(),
            actor_id: None,
            map_id: 0,
            stage: WorldStage::NotInWorld,
            soft_close_count: 0,
        }
    }
}

/// **TransMap 簇出帧（接线批 A）**：一次成功的切图落地（oracle `BaseObject.SpaceMove`
/// `src/M2Server/Actor/BaseObject.cs:1130-1144` 与 `PlayObject.EnterAnotherMap`
/// `src/M2Server/Player/PlayObject.cs:3772-3778` 的共同落地动作）产生的事件。
///
/// `switch_map` 成功后记一条；宿主（`bins/game-svr`）在下一个下行段 drain 并连发
/// `SM_CLEAROBJECTS(633)` → `SM_CHANGEMAP(634)`（+ `SM_SPACEMOVE_SHOW(801)`/`SHOW2(807)`）。
///
/// - `space_move_show`：`None` = `EnterAnotherMap`（无 show 帧）；`Some(show2)` = `SpaceMove`
///   的随机落点支（`nInt == 1` ⇒ `RM_SPACEMOVE_SHOW2` → `show2 == true`，否则 `RM_SPACEMOVE_SHOW`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MapChange {
    /// 落地者 `ActorId`（`SM_CHANGEMAP` 的 `Recog`、`SM_SPACEMOVE_SHOW` 的 `Recog`）。
    pub actor_id: i32,
    /// 落地后的图（宿主据此查地图文件名 + `DayBright`）。
    pub map_id: usize,
    /// 落地后的坐标（`SM_CHANGEMAP` 的 `Param`/`Tag`）。
    pub x: i32,
    pub y: i32,
    pub space_move_show: Option<bool>,
}

/// 世界侧会话管理（`World` 的一部分）。
#[derive(Clone, Debug, Default)]
pub struct SessionRegistry {
    sessions: Vec<Session>,
    /// **接线批 A**：本 tick 内成功的切图事件（`World::switch_map` 记，宿主 drain）。保序。
    map_changes: Vec<MapChange>,
    /// **orphan 接线第二批**：掉线/小退/顶号时「能看到离开者」的观察者事件
    /// `(观察者 id, 离开者 id)`（oracle `BaseObject.DisappearA` ⇒ `SendRefMsg(RM_DISAPPEAR)` 广播）。保序。
    departs: Vec<(i32, i32)>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: Vec::new(),
            map_changes: Vec::new(),
            departs: Vec::new(),
        }
    }

    /// **orphan 接线第二批**：取走本轮的「离世广播」事件（保序，取走即清）。
    pub fn drain_departs(&mut self) -> Vec<(i32, i32)> {
        std::mem::take(&mut self.departs)
    }

    /// **orphan 接线第二批**：只读查看待 drain 的离世广播（诊断/用例断言用；不消费）。
    pub fn departs(&self) -> &[(i32, i32)] {
        &self.departs
    }

    pub fn get(&self, chr_name: &str) -> Option<&Session> {
        self.sessions.iter().find(|s| s.chr_name == chr_name)
    }

    pub fn get_mut(&mut self, chr_name: &str) -> Option<&mut Session> {
        self.sessions.iter_mut().find(|s| s.chr_name == chr_name)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Session> {
        self.sessions.iter()
    }

    pub fn len(&self) -> usize {
        self.sessions.len()
    }

    pub fn is_empty(&self) -> bool {
        self.sessions.is_empty()
    }

    /// **接线批 A**：取走本 tick 已记的切图事件（保序，取走即清）。
    pub fn drain_map_changes(&mut self) -> Vec<MapChange> {
        std::mem::take(&mut self.map_changes)
    }

    /// **接线批 A**：只读查看待 drain 的切图事件（诊断/用例断言用；不消费）。
    pub fn map_changes(&self) -> &[MapChange] {
        &self.map_changes
    }

    /// **L1 TransMap 回滚**：把本轮 `map_changes` 截回 `len`（撤掉本次未落地的切图记录）。
    ///
    /// 用在 `step_with_route` 的 `EnterAnotherMap` 失败回滚支（oracle `Walk()` 返回 false ⇒ 宿主
    /// 不发帧 ⇒ 已记的 `633/634` 也必须撤掉，否则宿主会多发一组换图簇）。
    pub fn truncate_map_changes(&mut self, len: usize) {
        self.map_changes.truncate(len);
    }
}

impl World {
    /// 建会话（对应 M1 侧"角色选定、准备进世界"）；此时还没进图。
    pub fn open_session(&mut self, chr_name: &str) -> usize {
        if self.sessions.get(chr_name).is_none() {
            self.sessions.sessions.push(Session::new(chr_name));
        }
        self.sessions
            .sessions
            .iter()
            .position(|s| s.chr_name == chr_name)
            .expect("刚插入过")
    }

    pub fn session(&self, chr_name: &str) -> Option<&Session> {
        self.sessions.get(chr_name)
    }

    /// 进图（对应 `AddMapObject` + 会话绑定的组合）。
    ///
    /// 已进图时返回 `Err`（防重复进图把实体登记两次）。
    pub fn enter_map(
        &mut self,
        chr_name: &str,
        mut entity: Entity,
        map_id: usize,
    ) -> Result<i32, SessionError> {
        let Some(sess) = self.sessions.get_mut(chr_name) else {
            return Err(SessionError::NoSession);
        };
        if sess.stage == WorldStage::InWorld {
            return Err(SessionError::AlreadyInWorld);
        }
        if map_id >= self.maps.len() {
            return Err(SessionError::NoSuchMap);
        }
        if entity.id <= 0 {
            entity.id = self.entities.alloc_id();
        }
        entity.map_id = map_id;
        // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`：ctor 等价物拿不到 `World` 时钟
        // ⇒ 落图时刻重盖（同 `poisoning_tick` 的口径）。
        entity.verify_tick = self.clock_ms;
        // `BaseObject.cs:442` / `PlayObject.cs:901` ctor `SearchTick = GetTickCount()`：
        // 玩家 `SearchViewRange` 的 1000ms 节拍从**构造/进图时刻**起算（同 `verify_tick` 口径）。
        entity.ai.search_tick = self.clock_ms;
        // `PlayObject.cs:913` ctor `CheckDupObjTick = GetTickCount()`：重叠推挤的 3s 门从落图时刻起算。
        entity.check_dup_obj_tick = self.clock_ms;
        let id = self.entities.insert(entity);
        let (x, y) = {
            let e = self.entities.get(id).expect("刚插入");
            (e.x, e.y)
        };
        let now = self.clock_ms;
        // oracle `BaseObject.Base.cs:14-23` `Initialize()`：
        //   `AddtoMapSuccess = true; if (Envir.CanWalk(CurrX, CurrY, true) && AddToMap()) AddtoMapSuccess = false;`
        // 落格失败（目标格越界 / 非 `Valid`）时 **不摘除角色**：`AddtoMapSuccess` 保持 `true`
        //（该字段方向是反的——`true` = 未落图；`WorldServer.Merchant.cs:18` 正是拿它当失败日志）。
        // 角色只是**不进格表**（`Envirnoment.AddMapObject` 早退，`AddObject` 也不执行）。
        // 玩家侧调用点 `PlayObject.UserLogon`（`Player/PlayObject.cs:1152`）在 `Initialize()`（`:1175`）
        // 之后**无条件**继续（`:1176` 发 `RM_LOGON`；另见 `WorldServer.cs:659` 调 `UserLogon`）⇒
        // 会话照常进世界，仅 `entity.add_to_mapped == false`。
        // 故这里**绝不** `entities.remove` / 提前 `Err(CellRejected)`。
        // 红检：改回 `entities.remove(id) + Err(CellRejected)` ⇒
        // `enter_map_wall_cell_keeps_actor_unmapped` / `enter_map_out_of_bounds_keeps_actor_unmapped` 必红。
        // **2026-10-17 后续片 `openmir2-enter-shift-loop`**：宿主进图路径（`bins/game-svr` 读档 →
        // 本函数）已先按 oracle `WorldServer.cs:371-395` 跑**地形移位环**（墙格逐轮
        // `-3+Random(6)`、至多 5 轮；仍不可走 ⇒ 回落 `[Setup] HomeMap/HomeX/HomeY`，`mapwire.rs`
        // 的 `shift_enter_position`）⇒ 经宿主登录**不存在「墙格出生」**，「进世界但不入格表」在该
        // 路径**不可达**（oracle 同因，`ProcessHumansMakeNewHuman` 先移位再落图）。本函数的 oracle
        // 同形语义仍保留给直接调用方（测试/非宿主路径），不是死代码。
        if self.maps[map_id].add_map_object(x, y, CellType::Play, id, now) {
            if let Some(e) = self.entities.get_mut(id) {
                e.add_to_mapped = true;
            }
        }
        let sess = self.sessions.get_mut(chr_name).expect("上面已确认存在");
        sess.actor_id = Some(id);
        sess.map_id = map_id;
        sess.stage = WorldStage::InWorld;
        Ok(id)
    }

    /// 切图（对应地图链接/传送的落地动作）：旧图摘除 → 新图登记 → 视野列表整体失效。
    ///
    /// = `PlayObject.EnterAnotherMap` 的落地（无 `SM_SPACEMOVE_SHOW` 尾帧）。成功记一条
    /// [`MapChange`]（`space_move_show = None`）。
    ///
    /// **失败语义**（oracle `PlayObject.cs:3787-3790`）：目标格落格失败 ⇒
    /// **回滚旧图旧坐标并重新落旧格**，返回 [`SessionError::CellRejected`]（oracle 返回
    /// `false`）；角色不会出现「两张图都不在格表」的中间态。
    pub fn switch_map(
        &mut self,
        chr_name: &str,
        map_id: usize,
        x: i32,
        y: i32,
    ) -> Result<(), SessionError> {
        self.switch_map_inner(chr_name, map_id, x, y, None)
    }

    /// **接线批 A**：`BaseObject.SpaceMove`（随机落点切图）的落地。与 [`World::switch_map`]
    /// 同一落地动作，但成功后在 `SM_CHANGEMAP` 之后追加 `SM_SPACEMOVE_SHOW2`(`show2==true`)/
    /// `SM_SPACEMOVE_SHOW`(`false`)（oracle `BaseObject.cs:1137/1141` 的 `nInt` 分叉）。
    pub fn switch_map_space(
        &mut self,
        chr_name: &str,
        map_id: usize,
        x: i32,
        y: i32,
        show2: bool,
    ) -> Result<(), SessionError> {
        self.switch_map_inner(chr_name, map_id, x, y, Some(show2))
    }

    fn switch_map_inner(
        &mut self,
        chr_name: &str,
        map_id: usize,
        x: i32,
        y: i32,
        space_move_show: Option<bool>,
    ) -> Result<(), SessionError> {
        let Some(sess) = self.sessions.get(chr_name) else {
            return Err(SessionError::NoSession);
        };
        if sess.stage != WorldStage::InWorld {
            return Err(SessionError::NotInWorld);
        }
        if map_id >= self.maps.len() {
            return Err(SessionError::NoSuchMap);
        }
        let actor_id = sess.actor_id.ok_or(SessionError::NotInWorld)?;
        let old_map = sess.map_id;
        // oracle `EnterAnotherMap` 先保存 `oldEnvir/nOldX/nOldY`（`PlayObject.cs:3756-3758`），
        // 失败回滚支（`:3787-3790`）要用；坐标必须在旧图摘除前取。
        let (old_x, old_y) = {
            let e = self
                .entities
                .get(actor_id)
                .ok_or(SessionError::NotInWorld)?;
            (e.x, e.y)
        };
        // 旧图摘除（oracle `DisappearA` → `Envir.DeleteFromMap`，`BaseObject.cs:1519-1522`；
        // `DeleteFromMap` 同时把 `AddToMaped` 置 false，`Envirnoment.cs:660-661`）。
        self.remove_from_map(actor_id, old_map);
        // 新图登记（先改坐标再登记，否则格子登记到旧坐标）
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.map_id = map_id;
            e.x = x;
            e.y = y;
            e.visible_actors.clear(); // 视野列表整体失效
            e.is_visible_active = false;
            e.add_to_mapped = false;
        }
        let now = self.clock_ms;
        if !self.maps[map_id].add_map_object(x, y, CellType::Play, actor_id, now) {
            // oracle `EnterAnotherMap` 失败支（`PlayObject.cs:3787-3791`）：
            //   `Envir = oldEnvir; CurrX = nOldX; CurrY = nOldY;
            //    Envir.AddMapObject(CurrX, CurrY, CellType, this.ActorId, this);`
            // ⇒ **回滚旧图旧坐标并重新落旧格**（`AddMapObject` 在旧格可用时置
            // `AddToMaped=true` 并 `AddObject`，`Envirnoment.cs:279-283`；该位已被
            // `DeleteFromMap` 于旧图摘除时清零）。不回滚的话角色既不在旧图也不在新图
            // （两张图的格表都没有它），对应 oracle 绝不出现的中间态。
            // 视野列表保持清空——oracle 同样只清不回填（`PlayObject.cs:3760-3771`）。
            if let Some(e) = self.entities.get_mut(actor_id) {
                e.map_id = old_map;
                e.x = old_x;
                e.y = old_y;
                e.add_to_mapped = false;
            }
            let relanded = self
                .maps
                .get_mut(old_map)
                .is_some_and(|m| m.add_map_object(old_x, old_y, CellType::Play, actor_id, now));
            if let Some(e) = self.entities.get_mut(actor_id) {
                e.add_to_mapped = relanded;
            }
            return Err(SessionError::CellRejected);
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = true;
        }
        let sess = self.sessions.get_mut(chr_name).expect("上面已确认存在");
        sess.map_id = map_id;
        // **接线批 A**：成功落地 ⇒ 记一条切图事件（宿主按 oracle 序连发 CLEAROBJECTS/CHANGEMAP/[SHOW]）。
        self.sessions.map_changes.push(MapChange {
            actor_id,
            map_id,
            x,
            y,
            space_move_show,
        });
        Ok(())
    }

    /// 掉线：离开世界（实体从格子与仓库移除，会话留在 `Left`）。
    pub fn disconnect(&mut self, chr_name: &str) -> Result<(), SessionError> {
        self.leave_world_inner(chr_name, false)
    }

    /// 小退（`CM_SOFTCLOSE`）：离开世界并计数，允许再进。
    pub fn soft_close(&mut self, chr_name: &str) -> Result<(), SessionError> {
        self.leave_world_inner(chr_name, true)
    }

    /// **orphan 接线第二批**：取走本轮的「离世广播」事件（`(观察者, 离开者)`，保序，取走即清）。
    ///
    /// 供宿主把 `SM_DISAPPEAR(30)` 按 `[GateIdx, Socket]` 路由回**观察者自己**
    /// （oracle `BaseObject.DisappearA` ⇒ `SendRefMsg(RM_DISAPPEAR)` 广播，`BaseObject.cs:1519-1524`）。
    pub fn drain_departs(&mut self) -> Vec<(i32, i32)> {
        self.sessions.drain_departs()
    }

    fn leave_world_inner(&mut self, chr_name: &str, soft: bool) -> Result<(), SessionError> {
        let Some(sess) = self.sessions.get(chr_name) else {
            return Err(SessionError::NoSession);
        };
        if sess.stage != WorldStage::InWorld {
            return Err(SessionError::NotInWorld);
        }
        let actor_id = sess.actor_id.ok_or(SessionError::NotInWorld)?;
        let map_id = sess.map_id;
        // **orphan 接线第二批**（oracle `BaseObject.DisappearA`，`BaseObject.cs:1519-1524`）：
        // 掉线/小退/顶号 ⇒ `PlayObject.MakeGhost`（`BaseObject.Base.cs:484-497`，玩家 `CanReAlive=false`
        // ⇒ `Ghost=true`）⇒ `DisappearA()` ⇒ `SendRefMsg(RM_DISAPPEAR, 0,0,0,0,"")`（`:1519-1524`）
        // ⇒ `SendRefMsg`（`BaseObject.Message.cs:385-482`）把 `RM_DISAPPEAR` **广播**给同图
        // ±`SendRefMsgRange`(=12) 格内 `Race==Play` 且 `!Ghost` 的可见玩家。
        //
        // 本骨架的收件人集合 = 「`visible_actors` 里含 `actor_id` 的 Play 实体」（谁当前看得见它；
        // 与 oracle 的 ±12 扫描集在 ViewRange≤12 时一致，差别只在观察者视野已失效的边界）。
        // `ObMode || FixedHideMode` 的离开者 oracle 只发自己（`:389-392`）⇒ 无广播。
        // **必须在 `entities.remove` 之前采集**，否则实体已消失、反查不到观察者。
        let self_only = self
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ob_mode || e.fixed_hide_mode);
        if !self_only {
            let observers: Vec<i32> = self
                .entities
                .iter()
                .filter(|e| e.id != actor_id && e.race == ACTOR_RACE_PLAY && !e.ghost)
                .filter(|e| e.visible_actors.iter().any(|v| v.target == actor_id))
                .map(|e| e.id)
                .collect();
            for o in observers {
                self.sessions.departs.push((o, actor_id));
            }
        }
        self.remove_from_map(actor_id, map_id);
        self.entities.remove(actor_id);
        let sess = self.sessions.get_mut(chr_name).expect("上面已确认存在");
        sess.actor_id = None;
        sess.stage = WorldStage::Left;
        if soft {
            sess.soft_close_count += 1;
        }
        Ok(())
    }

    /// 从某张图的格子里摘除实体（按 `ActorId` 匹配）。
    ///
    /// `pub(crate)`：`world.rs` 的 `rollback_move`（走前格回滚）也要复用。
    pub(crate) fn remove_from_map(&mut self, actor_id: i32, map_id: usize) {
        let Some(e) = self.entities.get(actor_id) else {
            return;
        };
        let (x, y) = (e.x, e.y);
        let Some(map): Option<&mut MapGrid> = self.maps.get_mut(map_id) else {
            return;
        };
        let (cell, success) = map.get_cell_info_mut(x, y);
        if !success {
            return;
        }
        cell.obj_list
            .retain(|o| !(o.actor_object && o.cell_obj_id == actor_id));
        if cell.count() == 0 {
            cell.clear();
        }
    }
}

/// 会话操作失败原因（显式枚举：世界侧不允许静默失败）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SessionError {
    NoSession,
    AlreadyInWorld,
    NotInWorld,
    NoSuchMap,
    /// 目标格不可用（越界或不可走）——对应 C# `EnterAnotherMap` 返回 `false`
    ///（`AddToMap` → `AddMapObject` 返回 false，`PlayObject.cs:3779-3791`）。
    ///
    /// **返回前已完成 oracle 的回滚**：旧图旧坐标已恢复、旧格已重新登记
    ///（`PlayObject.cs:3787-3790`）；调用方看到的只是「这次切图没成功」，世界状态仍是
    /// 切图前的样子。
    ///
    /// 仅由 `switch_map_inner`（切图）产生；`enter_map`（进图）**不再**产生它——
    /// oracle `Initialize`（`BaseObject.Base.cs:14-23`）落格失败只「不进格表」，不移除角色。
    CellRejected,
}
