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

use crate::entity::Entity;
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

/// 世界侧会话管理（`World` 的一部分）。
#[derive(Clone, Debug, Default)]
pub struct SessionRegistry {
    sessions: Vec<Session>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        SessionRegistry {
            sessions: Vec::new(),
        }
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
        let id = self.entities.insert(entity);
        let (x, y) = {
            let e = self.entities.get(id).expect("刚插入");
            (e.x, e.y)
        };
        let now = self.clock_ms;
        if !self.maps[map_id].add_map_object(x, y, CellType::Play, id, now) {
            // 格子不可用（越界/不可走）：按 C# 语义进图失败
            self.entities.remove(id);
            return Err(SessionError::CellRejected);
        }
        if let Some(e) = self.entities.get_mut(id) {
            e.add_to_mapped = true;
        }
        let sess = self.sessions.get_mut(chr_name).expect("上面已确认存在");
        sess.actor_id = Some(id);
        sess.map_id = map_id;
        sess.stage = WorldStage::InWorld;
        Ok(id)
    }

    /// 切图（对应地图链接/传送的落地动作）：旧图摘除 → 新图登记 → 视野列表整体失效。
    pub fn switch_map(
        &mut self,
        chr_name: &str,
        map_id: usize,
        x: i32,
        y: i32,
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
        // 旧图摘除
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
            return Err(SessionError::CellRejected);
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = true;
        }
        let sess = self.sessions.get_mut(chr_name).expect("上面已确认存在");
        sess.map_id = map_id;
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

    fn leave_world_inner(&mut self, chr_name: &str, soft: bool) -> Result<(), SessionError> {
        let Some(sess) = self.sessions.get(chr_name) else {
            return Err(SessionError::NoSession);
        };
        if sess.stage != WorldStage::InWorld {
            return Err(SessionError::NotInWorld);
        }
        let actor_id = sess.actor_id.ok_or(SessionError::NotInWorld)?;
        let map_id = sess.map_id;
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
    fn remove_from_map(&mut self, actor_id: i32, map_id: usize) {
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
    /// 目标格不可用（越界或不可走）——对应 C# `AddMapObject` 返回 false
    CellRejected,
}
