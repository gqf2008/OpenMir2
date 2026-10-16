//! **精灵状态系统** —— `src/M2Server/ActorBuffSystem.cs` 的 1:1 复刻（卡
//! `openmir2-world-tick-expansion` 车道 B1；红名单 v2 finding V4-03，tier=R）。
//!
//! 移植源逐条（含设计件 `tests/parity/m3/design-buff-and-tick-20261016.md` §3 的推导）：
//! - `BuffType`（`ActorBuffSystem.cs:10-48`，12 值）；
//! - `ActorBuffData`（`:339-350`）；
//! - `AddBuff`（`:100-159`）；
//! - `DoWork`（`:58-91`，含两处 `RemoveAt` off-by-one）；
//! - `Operate`（`:164-263`，递减段 + 毒伤 DoT 段，含 `break` 跳出 `for(j)` 的怪癖）；
//! - `GetBuff`（`:274-296`）/`GetBuffStatus`（`:282-296`）；
//! - `HasBuff`（`:319-337`）/`TryUpdate`（`:339-350`）；
//! - `BaseObject.GetCharStatus`（`BaseObject.cs:1499-1503`）；
//! - `BaseObject.DamageHealth`（`BaseObject.cs:775-818`，见 [`damage_health`]）。
//!
//! **三条必须 1:1 复刻的 oracle 细节（不许"修正"）**：
//!
//! 1. **`break` 跳出的是 `for(j)` 而非 `switch`**：`StateTick == 10` 且类型是
//!    `DefensePower`/`MagicDefensePower` 时，本 pass **不再递减该 actor 后续的 buff**
//!    （`ActorBuffSystem.cs:206/211`）。
//! 2. **递减节奏（离线模型推导，待真机窗口 A-R2 证实——不是已确立真值）**：
//!    `StatusArrTick` 从不初始化（恒 0），每 decrement 只 `+= 1000`，门是
//!    `(now - StatusArrTick) > 1000`。以 `now = T_c + 50k`、`StatusArrTick = 1000k` 代入，
//!    第 `k` 个 pass 门开当且仅当 `k < (T_c - 1000)/950`——`T_c` 是 buff 建立时的
//!    `GetTickCount()`（开机毫秒数，`HUtil32.cs:36-39`）。因合法 buff 的 `StateTick ≪ 60000`
//!    且 `T_c` 是**大数**（开机时长），门在前 `T_c/950` 个 pass 恒开 ⇒ **按本模型，`StateTick`
//!    每 50ms pass 减 1**。本仓照此模型实现：见 [`BUFF_EPOCH_MS`] 与 [`World::buff_tick`]。
//!    **这是「实现与模型一致」的口径，不是「模型 = oracle 真值」**：redlist2 V4-03 原文把同一段读作
//!    「每 1s 减 1」——两种读法孰真**只能由真机窗口 A-R2 裁定**，本仓不预断。
//! 3. **`if (actor.Race == Play)` 才进递减段** ⇒ **怪物/动物（race > 0）的 buff 永不到期递减**，
//!    只有毒伤段对它们生效（`ActorBuffSystem.cs:170`）。
//!
//! **索引口径**：buff 的位号/序号是 `ActorBuffData` 在该 actor 列表里的**插入序 `i`**，
//! **不是 `BuffType`**（`GetBuffStatus`/`GetBuff` 都按 `i` 取值）。

use std::collections::HashMap;

use crate::entity::ACTOR_RACE_PLAY;
use crate::world::World;

/// `Grobal2.MAX_STATUS_ATTRIBUTE`（`src/OpenMir2/Grobal2.cs:39`）。
pub const MAX_STATUS_ATTRIBUTE: u8 = 12;

/// `ActorBuffData.DestroyTick = GetTickCount() + 40000`（`ActorBuffSystem.cs:117/129/144`）。
pub const BUFF_DESTROY_TICK_MS: i64 = 40000;

/// `ActorBuffProcessor` 的 50ms 节拍（`ActorBuffProcessor.cs:7`，见设计件 §3.4）。
pub const BUFF_PASS_INTERVAL_MS: i64 = 50;

/// 递减段的时间门 `(now - StatusArrTick) > 1000`（`ActorBuffSystem.cs:198`），
/// 以及每次 decrement 对 `StatusArrTick` 的加量 `+= 1000`（`:200`）——二者同值。
pub const BUFF_STATE_TICK_GATE_MS: i64 = 1000;

/// 递减段的 `StateTick < 60000` 上界（`ActorBuffSystem.cs:196`）。
pub const BUFF_STATE_TICK_MAX: i32 = 60000;

/// oracle `GetTickCount()` 是**开机毫秒数**（`Environment.TickCount`）；Rust 虚拟时钟从 0 起。
///
/// 第 2 条节奏模型的门 `(now - StatusArrTick) > 1000` 只在 `now` 落在**大数基**上时才复刻出
/// oracle 的破门语义（`StatusArrTick` 恒 0 + 每 decrement `+= 1000`）。本常量把 buff 门的绝对
/// 时刻抬到"模拟开机时长"基上（1e9 ms ≈ 11.6 天）：门在前 `1e9/950 ≈ 1.05e6` 个 pass 恒开 ⇒
/// **每个 50ms pass 减 1**（与第 2 条模型一致）。
///
/// **这是 U-A2（sim 200ms vs oracle 50ms）的落地口径**：sim 每 tick 200ms ⇒
/// [`BUFF_PASSES_PER_TICK`] 个 50ms pass；`add_buff`/`make_posion` 与 pass 用**同一基**
/// （`clock_ms + BUFF_EPOCH_MS`）⇒ `DestroyTick` 的 40s 与 `StatusArrTick` 的门同轴。
pub const BUFF_EPOCH_MS: i64 = 1_000_000_000;

/// 每个 world tick（200ms）里的 buff pass 数（`TICK_INTERVAL_MS / BUFF_PASS_INTERVAL_MS`）。
pub const BUFF_PASSES_PER_TICK: i64 = 200 / BUFF_PASS_INTERVAL_MS;

/// `ActorBuffSystem.cs:10-48` 的 `enum BuffType : byte` 1:1。
///
/// 与 `PoisonState`（`src/OpenMir2/Consts/PoisonState.cs`）**按数值对齐**（设计件 §3.1.9）：
/// `MakePosion(nType)` 用 `(BuffType)nType` 直接索引 ⇒ 索引即数值，勿按名字重排。
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuffType {
    /// 绿毒（`PoisonState.DECHEALTH=0`）
    GreenPoison = 0,
    /// 红毒（`PoisonState.DAMAGEARMOR=1`）
    RedPoison = 1,
    /// `PoisonState.LOCKSPELL=2`
    Defense = 2,
    /// 不能走动（`PoisonState.LOCKRUN=3`）
    CanNotRun = 3,
    /// 不能移动（`PoisonState.DONTMOVE=4`）
    CanNotMove = 4,
    /// `PoisonState.STONE=5`
    Paralysis = 5,
    /// `PoisonState.POISON_68` 无对应（`:222`）
    ReduceHP = 6,
    /// `PoisonState.FASTMOVE=7`
    Frozen = 7,
    /// 魔法隐身（`PoisonState.STATETRANSPARENT=8`）
    Transparent = 8,
    /// 防御力（`PoisonState.DefenceUP=9`）
    DefensePower = 9,
    /// 魔法防御力（`PoisonState.MagDefenceUP=10`）
    MagicDefensePower = 10,
    /// 魔法盾（`PoisonState.BubbleDefenceUP=11`）
    MagicShield = 11,
}

impl BuffType {
    /// 按 `PoisonState` 的数值（= `BuffType` 数值）取枚举；`>= MAX_STATUS_ATTRIBUTE` ⇒ `None`
    /// （照 `MakePosion` 的 `nType >= MAX_STATUS_ATTRIBUTE` 门）。
    #[must_use]
    pub fn from_index(n: u8) -> Option<Self> {
        Some(match n {
            0 => BuffType::GreenPoison,
            1 => BuffType::RedPoison,
            2 => BuffType::Defense,
            3 => BuffType::CanNotRun,
            4 => BuffType::CanNotMove,
            5 => BuffType::Paralysis,
            6 => BuffType::ReduceHP,
            7 => BuffType::Frozen,
            8 => BuffType::Transparent,
            9 => BuffType::DefensePower,
            10 => BuffType::MagicDefensePower,
            11 => BuffType::MagicShield,
            _ => return None,
        })
    }

    #[must_use]
    pub fn as_u8(self) -> u8 {
        self as u8
    }
}

/// `ActorBuffData`（`ActorBuffSystem.cs:339-350`）。`ActorId` 由 map 键承担，不入结构体。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ActorBuffData {
    pub buff_type: BuffType,
    /// `StateVal`（`MakePosion` 的 `nPoint` 等）。
    pub state_val: i32,
    /// `StateTick` —— 递减段剩余量（= 秒数；模型下单 位 = 50ms，见模块头第 2 条）。
    pub state_tick: i32,
    /// `StatusArrTick` —— **初值必须为 0**（`ActorBuffSystem.cs:85`，全仓唯一赋值是 `:182` 的
    /// `+= 1000`；新 buff 的 `StatusArrTick == 0`，这决定递减节奏，**不是笔误**）。
    pub status_arr_tick: i64,
    /// `DestroyTick` = 建立时 `GetTickCount() + 40000`。
    pub destroy_tick: i64,
}

impl ActorBuffData {
    fn new(buff_type: BuffType, state_val: i32, state_tick: i32, now_ms: i64) -> Self {
        ActorBuffData {
            buff_type,
            state_val,
            state_tick,
            status_arr_tick: 0,
            destroy_tick: now_ms + BUFF_DESTROY_TICK_MS,
        }
    }
}

/// `Operate` 递减段里对**玩家**发的 `SysMsg`（`ActorBuffSystem.cs:191-208`）。
///
/// 帧面（聊天/提示行 `SM_*`）归接线层；world 侧只记事件，供宿主出帧或测试断言。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuffSysMsg {
    /// `"防御力回复正常."`（`:191`）
    DefensePowerRecovered,
    /// `"魔法防御力回复正常."`（`:197`）
    MagicDefensePowerRecovered,
    /// `"防御力10秒后恢复正常。"`（`:206`）
    DefensePower10Sec,
    /// `"魔法防御力10秒后恢复正常。"`（`:211`）
    MagicDefensePower10Sec,
}

/// 一趟 `DoWork`（含 `Operate` + 销毁段）的可观测产出（供 `TickRecord`/宿主/单测）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BuffTickOutcome {
    /// `boChg` 触发的 `StatusChanged()`：`(actor_id, 新 CharStatus)`，保序。
    pub status_changes: Vec<(i32, i32)>,
    /// 递减段对玩家发的 `SysMsg`：`(actor_id, msg)`，保序（含 `break` 前的各条）。
    pub sysmsgs: Vec<(i32, BuffSysMsg)>,
    /// `boNeedRecalc` 触发的 `RecalcAbilitys()`：actor_id，保序。
    pub recalcs: Vec<i32>,
    /// 毒伤段每次 `HealthSpellChanged()`：`(actor_id, hp_after, mp_after)`，保序。
    pub health_spell_changes: Vec<(i32, u16, u16)>,
}

/// `ActorBuffMap`（`:55`）+ `ActorBuffs`（`:56`）——**两条秩序都要保留**：
/// - `BuffSystem` 的 `map` 的 `Vec` 是 `GetBuffStatus` 的**位序来源**（插入序 `i`）；
/// - [`BuffSystem::actors`] 是 `ActorBuffs` 的「首获 buff 序」，`Operate`/`DoWork` 按它遍历。
#[derive(Clone, Debug, Default)]
pub struct BuffSystem {
    map: HashMap<i32, Vec<ActorBuffData>>,
    actors: Vec<i32>,
}

impl BuffSystem {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// `AddBuff`（`ActorBuffSystem.cs:100-159`）。
    ///
    /// 已在该 actor：**无同 `BuffType`** ⇒ 追加一项（`DestroyTick = now + 40000`，`StatusArrTick = 0`）；
    /// **有同 `BuffType`** ⇒ 遍历**每一项**同类型（不是 first）刷新 `StateVal`/`StateTick`/`DestroyTick`
    /// （`:124-132`）。不在 map ⇒ 建表（`:136-149`）。**无条件**：`actors` 不含该 actor 才追加
    /// （`:152-155`）⇒ `actors` 序 = 首次获得 buff 的时间序。
    pub fn add_buff(
        &mut self,
        actor_id: i32,
        buff_type: BuffType,
        dura: i32,
        stateval: i32,
        now_ms: i64,
    ) {
        match self.map.get_mut(&actor_id) {
            Some(list) => {
                if !list.iter().any(|d| d.buff_type == buff_type) {
                    list.push(ActorBuffData::new(buff_type, stateval, dura, now_ms));
                } else {
                    for d in list.iter_mut() {
                        if d.buff_type == buff_type {
                            d.state_val = stateval;
                            d.state_tick = dura;
                            d.destroy_tick = now_ms + BUFF_DESTROY_TICK_MS;
                        }
                    }
                }
            }
            None => {
                self.map.insert(
                    actor_id,
                    vec![ActorBuffData::new(buff_type, stateval, dura, now_ms)],
                );
            }
        }
        if !self.actors.contains(&actor_id) {
            self.actors.push(actor_id);
        }
    }

    /// `GetBuff`（`:274-296`）：返回**`i + 1`**（1-based 位序）；无 ⇒ 0。
    #[must_use]
    pub fn get_buff(&self, actor_id: i32, buff_type: BuffType) -> i32 {
        let Some(list) = self.map.get(&actor_id) else {
            return 0;
        };
        for (i, d) in list.iter().enumerate() {
            if d.buff_type == buff_type {
                return i as i32 + 1;
            }
        }
        0
    }

    /// `HasBuff`（`:319-337`）。
    #[must_use]
    pub fn has_buff(&self, actor_id: i32, buff_type: BuffType) -> bool {
        self.get_buff(actor_id, buff_type) != 0
    }

    /// `TryUpdate`（`:339-350`）：命中同类型项即刷 `StateTick`（**不刷 `DestroyTick`**）；无 ⇒ `false`。
    pub fn try_update(&mut self, actor_id: i32, buff_type: BuffType, dura: i32) -> bool {
        let Some(list) = self.map.get_mut(&actor_id) else {
            return false;
        };
        for d in list.iter_mut() {
            if d.buff_type == buff_type {
                d.state_tick = dura;
                return true;
            }
        }
        false
    }

    /// `GetBuffStatus`（`:282-296`）：只有 `StateTick > 0` 的 buff 置位；
    /// 位号 = 插入序 `i`（`0x80000000 >> i`，最高位起）。`i ≥ 32` 时按 C# `uint >> i` 的
    /// `i & 31` 回绕（oracle 未防，U-A4，照搬）。
    #[must_use]
    pub fn get_buff_status(&self, actor_id: i32) -> i32 {
        let mut status: u32 = 0;
        if let Some(list) = self.map.get(&actor_id) {
            for (i, d) in list.iter().enumerate() {
                if d.state_tick > 0 {
                    status |= 0x8000_0000u32 >> (i & 31);
                }
            }
        }
        status as i32
    }

    /// `BaseObject.GetCharStatus()` 的 **BuffSystem 侧**：`GetBuffStatus(this) | (CharStatusEx & 0xFFFF)`。
    ///
    /// **必须用本方法（而不是 `world.get_char_status`）在 `Operate` 内算 `boChg` 的 CharStatus**：
    /// oracle（`ActorBuffSystem.cs:221-222`）读的是**活的** `ActorBuffSystem`；而本仓
    /// `World::buff_do_work` 会先把 `world.buffs` `mem::take` 走（避免 `do_work` 借两次 `&mut self`），
    /// 此刻经 `world.get_char_status` 取到的是**空系统** ⇒ 会丢掉所有仍生效的 buff 位。
    #[must_use]
    pub fn char_status_with(&self, actor_id: i32, char_status_ex: i32) -> i32 {
        self.get_buff_status(actor_id) | (char_status_ex & 0x0000_FFFF)
    }

    /// 某 actor 当前的 buff 列表（只读；测试与消费者用）。
    #[must_use]
    pub fn list(&self, actor_id: i32) -> Option<&[ActorBuffData]> {
        self.map.get(&actor_id).map(Vec::as_slice)
    }

    /// `ActorBuffs` 的只读视图（`Operate` 的遍历序）。
    #[must_use]
    pub fn actors(&self) -> &[i32] {
        &self.actors
    }

    /// `DoWork`（`:58-91`）：`if (ActorBuffs.Any())` 才继续；**先** `Operate`，再销毁段
    /// （含 `for(j)` 的 `RemoveAt(j)` off-by-one 与 actor 级 off-by-one）。
    pub fn do_work(&mut self, world: &mut World, now_ms: i64) -> BuffTickOutcome {
        if self.actors.is_empty() {
            return BuffTickOutcome::default();
        }
        let out = self.operate(world, now_ms);
        self.reap_destroyed(now_ms);
        out
    }

    /// `Operate`（`:164-263`）。递减段的 `boChg`/`boNeedRecalc` 是**跨 actor 粘滞**的
    /// （oracle 在 `for i` 外声明、永不复位）——一并复刻。
    fn operate(&mut self, world: &mut World, now_ms: i64) -> BuffTickOutcome {
        let mut out = BuffTickOutcome::default();
        let poison_dec_health_time = i64::from(world.poison_dec_health_time);
        let mut bo_chg = false;
        let mut bo_need_recalc = false;
        for i in 0..self.actors.len() {
            let actor_id = self.actors[i];
            let Some(race) = world.entities.get(actor_id).map(|e| e.race) else {
                continue;
            };
            // ---- (a) 递减段：仅 `Race == Play`（怪物 buff 永不到期，见模块头第 3 条）----
            if race == ACTOR_RACE_PLAY {
                if let Some(list) = self.map.get_mut(&actor_id) {
                    let mut j = 0;
                    while j < list.len() {
                        let d = &mut list[j];
                        if d.state_tick > 0
                            && d.state_tick < BUFF_STATE_TICK_MAX
                            && now_ms - d.status_arr_tick > BUFF_STATE_TICK_GATE_MS
                        {
                            d.state_tick -= 1;
                            d.status_arr_tick += BUFF_STATE_TICK_GATE_MS;
                            if d.state_tick == 0 {
                                bo_chg = true;
                                match d.buff_type {
                                    BuffType::DefensePower => {
                                        bo_need_recalc = true;
                                        out.sysmsgs
                                            .push((actor_id, BuffSysMsg::DefensePowerRecovered));
                                    }
                                    BuffType::MagicDefensePower => {
                                        bo_need_recalc = true;
                                        out.sysmsgs.push((
                                            actor_id,
                                            BuffSysMsg::MagicDefensePowerRecovered,
                                        ));
                                    }
                                    BuffType::Transparent => {
                                        if let Some(e) = world.entities.get_mut(actor_id) {
                                            e.hide_mode = false;
                                        }
                                    }
                                    _ => {}
                                }
                            } else if d.state_tick == 10 {
                                // ⚠ `break` 跳出的是本 `for(j)`（不是 switch）：本 pass 不再递减后续 buff。
                                if d.buff_type == BuffType::DefensePower {
                                    out.sysmsgs.push((actor_id, BuffSysMsg::DefensePower10Sec));
                                    break;
                                }
                                if d.buff_type == BuffType::MagicDefensePower {
                                    out.sysmsgs
                                        .push((actor_id, BuffSysMsg::MagicDefensePower10Sec));
                                    break;
                                }
                            }
                        }
                        j += 1;
                    }
                }
                if bo_chg {
                    // oracle `ActorBuffSystem.cs:221-222`：`actor.CharStatus = actor.GetCharStatus()` 读的是
                    // **此刻正在被操作的** `ActorBuffSystem`（本函数就是它）⇒ 必须用 `self`（`BuffSystem`）
                    // 的位算 CharStatus；**不能**经 `world.get_char_status`（那读 `world.buffs`，而
                    // `World::buff_do_work` 已把 `buffs` `mem::take` 走 ⇒ 拿到空系统、丢掉所有仍生效的 buff 位）。
                    let ex = world.entities.get(actor_id).map_or(0, |e| e.char_status_ex);
                    let status = self.char_status_with(actor_id, ex);
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        e.char_status = status;
                    }
                    out.status_changes.push((actor_id, status));
                }
                if bo_need_recalc {
                    out.recalcs.push(actor_id);
                }
            }

            // ---- (b) 毒伤段：对所有 `!Death` 的 actor（不分 race）----
            let Some(ent) = world.entities.get(actor_id) else {
                continue;
            };
            if ent.death {
                continue;
            }
            if now_ms - ent.poisoning_tick <= poison_dec_health_time {
                continue;
            }
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.poisoning_tick = now_ms;
            }
            let Some(list) = self.map.get(&actor_id) else {
                continue;
            };
            // 逐 `for(j)`：每命中一条 GreenPoison(StateTick>0) 就扣一次血（可多条）。
            let green_hits: usize = list
                .iter()
                .filter(|d| d.buff_type == BuffType::GreenPoison && d.state_tick > 0)
                .count();
            if green_hits == 0 {
                continue;
            }
            for _ in 0..green_hits {
                let (animal, point) = match world.entities.get(actor_id) {
                    Some(e) => (e.ai.animal, e.green_poisoning_point),
                    None => break,
                };
                if animal {
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        // C# `ushort -= 1000` 是 unchecked 回绕。
                        e.ai.meat_quality = e.ai.meat_quality.wrapping_sub(1000);
                    }
                }
                let last_hiter = world.entities.get(actor_id).and_then(|e| e.last_hiter);
                damage_health(world, actor_id, i32::from(point) + 1, last_hiter);
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.health_tick = 0;
                    e.spell_tick = 0;
                }
                let (hp, mp) = match world.entities.get(actor_id) {
                    Some(e) => (e.hp, e.mp),
                    None => break,
                };
                out.health_spell_changes.push((actor_id, hp, mp));
            }
        }
        out
    }

    /// `DoWork` 的销毁段（`ActorBuffSystem.cs:64-90`）——两处 off-by-one 原样搬迁：
    /// `for(j)` 里 `RemoveAt(j)` 后 `j++` 跳过紧邻的下一个到期项；actor 级同形。
    fn reap_destroyed(&mut self, now_ms: i64) {
        let mut idx = 0;
        let mut actor_count = self.actors.len();
        while idx < actor_count {
            let actor_id = self.actors[idx];
            let mut remove_actor = false;
            if let Some(list) = self.map.get_mut(&actor_id) {
                let mut j = 0;
                while j < list.len() {
                    if now_ms >= list[j].destroy_tick {
                        list.remove(j);
                        // 注意：**不** `continue`——oracle 的 `for(j)` 在 body 后仍 `j++`，
                        // 故移入 `j` 的那一项被跳过（off-by-one），留下到下一 pass。
                    }
                    j += 1;
                }
                remove_actor = list.is_empty();
            }
            if remove_actor {
                self.actors.remove(idx);
                actor_count -= 1;
            }
            idx += 1;
        }
    }
}

/// `BaseObject.DamageHealth(int nDamage)`（`BaseObject.cs:775-818`）的 1:1。
///
/// **首行 `LastHiter` 守卫**（`:777`）：`(LastHiter == null) || (LastHiter.Race == Play && !UnMagicShield)`
/// 为真才进 **魔法盾 MP 抵扣**；`LastHiter` 为玩家且已开 `UnMagicShield` 时**跳过** MP 抵扣、
/// 直接扣 HP。`last_hiter` = `world.entities[actor_id].last_hiter`（由调用方取，见 `BuffSystem::operate` 的毒伤段）。
///
/// MP 抵扣（`:779-791`）：`Race==Play && MagicShield && nDamage>0 && MP>0` ⇒
/// `nSpdam = Round(nDamage * 1.5)`（`Round` = `Math.Round(.., AwayFromZero)`）；
/// `MP >= nSpdam` ⇒ `MP -= nSpdam`、`nSpdam = 0`；否则 `nSpdam -= MP`、`MP = 0`；
/// 然后 `nDamage = Round(nSpdam / 1.5)`。抵扣后调 `HealthSpellChanged()`（帧面归接线层）。
///
/// HP 扣减（`:796-817`）：`nDamage > 0` ⇒ `HP-nDamage > 0 ? HP-=nDamage : HP=0`；
/// 否则（`nDamage <= 0`，含 0）⇒ `HP-nDamage < MaxHP ? HP-=nDamage : HP=MaxHP`（负值 ⇒ 回血，照搬）。
pub fn damage_health(world: &mut World, actor_id: i32, n_damage: i32, last_hiter: Option<i32>) {
    if world.entities.get(actor_id).is_none() {
        return;
    }
    let guard = match last_hiter.and_then(|h| world.entities.get(h)) {
        None => true,
        Some(h) => h.race == ACTOR_RACE_PLAY && !h.un_magic_shield,
    };
    let mut n_damage = n_damage;
    if guard {
        let shield = world.entities.get(actor_id).is_some_and(|e| {
            e.race == ACTOR_RACE_PLAY && e.magic_shield && n_damage > 0 && e.mp > 0
        });
        if shield {
            let e = world.entities.get_mut(actor_id).expect("actor exists");
            let mut n_spdam = mir2_shared::hutil::round(f64::from(n_damage) * 1.5);
            if i32::from(e.mp) >= n_spdam {
                e.mp = (i32::from(e.mp) - n_spdam) as u16;
                n_spdam = 0;
            } else {
                n_spdam -= i32::from(e.mp);
                e.mp = 0;
            }
            // oracle `BaseObject.cs:792`：`nDamage = (ushort)HUtil32.Round(nSpdam / 1.5)`——**带 `(ushort)` 截断**。
            // 照抄：当前不可达（毒伤 `GreenPoisoningPoint + 1 <= 256` ⇒ `Round(/1.5) <= 256 < 65536`），
            // 为忠实起见保留截断语义（毒伤上界若将来放宽即生效）。
            n_damage = i32::from(mir2_shared::hutil::round(f64::from(n_spdam) / 1.5) as u16);
            // `HealthSpellChanged()`（`BaseObject.cs:793`）：帧面归接线层（`SM_HEALTHSPELLCHANGED(53)`）。
            //
            // **登记（不可达差异，项⑤）**：oracle 在**盾支内**（本处）调一次 `HealthSpellChanged()`，
            // 毒伤段（`ActorBuffSystem.cs:255`）**再**调一次 ⇒ 「中毒 + 魔法盾」同时生效时**一 pass 两帧**；
            // Rust 只在毒伤段记一次 `health_spell_changes`（见 [`BuffSystem::operate`]）⇒ 只发一帧。
            // **当前不可达**：毒与魔法盾的生产源均 deferred（`make_posion`/`magic_shield_on` 无生产调用者）
            // ⇒ 运行期不会同时出现盾支与毒伤段 ⇒ 与 oracle 无观测差异。**不硬接**（等消费方接线批）。
        }
    }
    let e = world.entities.get_mut(actor_id).expect("actor exists");
    if n_damage > 0 {
        if i32::from(e.hp) - n_damage > 0 {
            e.hp = (i32::from(e.hp) - n_damage) as u16;
        } else {
            e.hp = 0;
        }
    } else if i32::from(e.hp) - n_damage < i32::from(e.max_hp) {
        e.hp = (i32::from(e.hp) - n_damage) as u16;
    } else {
        e.hp = e.max_hp;
    }
}

/// `BaseObject.GetCharStatus()`（`BaseObject.cs:1499-1503`）：
/// `GetBuffStatus(this) | (CharStatusEx & 0x0000FFFF)`。低 16 位 = `CharStatusEx`
/// （`STONEMODE=0x1`/`OPENHEATH=0x2`），高 16 位 = buff 位。
#[must_use]
pub fn get_char_status(world: &World, actor_id: i32) -> i32 {
    let ex = world.entities.get(actor_id).map_or(0, |e| e.char_status_ex);
    world.buffs.char_status_with(actor_id, ex)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::Entity;
    use crate::world::{World, TICK_INTERVAL_MS};

    fn world_with_players(ids: &[i32]) -> World {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        for &id in ids {
            let e = Entity::new(id, format!("p{id}"), 10, 10);
            w.add_entity(e);
        }
        w
    }

    // ---------------- A-L1：表语义（无则加 / 有则刷；destroy_tick；status_arr_tick 初值 0） ----------------
    #[test]
    fn a_l1_add_buff_semantics() {
        let mut bs = BuffSystem::new();
        bs.add_buff(1, BuffType::GreenPoison, 30, 5, 1000);
        let l = bs.list(1).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(
            l[0].destroy_tick,
            1000 + BUFF_DESTROY_TICK_MS,
            "destroy_tick = now + 40000"
        );
        assert_eq!(l[0].status_arr_tick, 0, "StatusArrTick 初值必须为 0");
        assert_eq!(l[0].state_tick, 30);

        // 有同类型 ⇒ 刷新（不新增）；并验证同类型**逐项**刷新的多实例臂。
        bs.add_buff(1, BuffType::GreenPoison, 99, 7, 2000);
        let l = bs.list(1).unwrap();
        assert_eq!(l.len(), 1, "同 BuffType 不新增");
        assert_eq!(l[0].state_tick, 99);
        assert_eq!(l[0].state_val, 7);
        assert_eq!(l[0].destroy_tick, 2000 + BUFF_DESTROY_TICK_MS);

        // actors 仅追加一次。
        assert_eq!(bs.actors(), &[1], "重复 AddBuff 不重复入 actor 列");
        bs.add_buff(2, BuffType::DefensePower, 10, 10, 3000);
        assert_eq!(bs.actors(), &[1, 2], "新 actor 追加（首获序）");
    }

    // ---------------- A-L2：位序 = 插入序（不是 BuffType） ----------------
    #[test]
    fn a_l2_bit_is_insertion_order() {
        let mut bs = BuffSystem::new();
        bs.add_buff(1, BuffType::GreenPoison, 10, 0, 0); // i=0 → bit31
        assert_eq!(bs.get_buff_status(1), 0x8000_0000u32 as i32);
        bs.add_buff(1, BuffType::DefensePower, 10, 0, 0); // i=1 → bit30
        assert_eq!(bs.get_buff_status(1), 0xC000_0000u32 as i32);
        bs.add_buff(1, BuffType::Transparent, 10, 0, 0); // i=2 → bit29
        assert_eq!(bs.get_buff_status(1), 0xE000_0000u32 as i32);
        // get_buff 返回 i+1（1-based）。
        assert_eq!(bs.get_buff(1, BuffType::Transparent), 3);
        // 位号与 BuffType 数值无关：GreenPoison(=0) 在 i=0，DefensePower(=9) 在 i=1。
        assert_eq!(bs.get_buff(1, BuffType::DefensePower), 2);
    }

    #[test]
    fn a_l2b_only_state_tick_positive_sets_bit() {
        let mut bs = BuffSystem::new();
        bs.add_buff(1, BuffType::GreenPoison, 10, 0, 0);
        bs.add_buff(1, BuffType::DefensePower, 0, 0, 0); // StateTick=0 ⇒ 不置位
        assert_eq!(bs.get_buff_status(1), 0x8000_0000u32 as i32);
    }

    // ---------------- A-L3：off-by-one 复刻（同 pass 两个到期只删第一个） ----------------
    #[test]
    fn a_l3_reap_off_by_one_only_removes_first() {
        let mut bs = BuffSystem::new();
        // 两个 destroy_tick 同时到期，且相邻。
        bs.add_buff(1, BuffType::GreenPoison, 10, 0, 0);
        bs.add_buff(1, BuffType::RedPoison, 10, 0, 0);
        // 手工把两者 destroy_tick 设为同一到期点（也在 40s 内）。
        // （用 try_update 无法改 destroy_tick；直接构造：在 now=0 加早 buff 后，到期点相同。）
        let now = BUFF_DESTROY_TICK_MS; // 第一个 pass 判定 `now >= destroy_tick`（0+40000）
        bs.reap_destroyed(now);
        let l = bs.list(1).unwrap();
        assert_eq!(l.len(), 1, "同一 pass 两个到期 ⇒ 只删第一个（off-by-one）");
        assert_eq!(
            l[0].buff_type,
            BuffType::RedPoison,
            "留下的是第二个（被跳过）"
        );
        // 下一 pass 才删第二个。
        bs.reap_destroyed(now);
        assert_eq!(bs.list(1).unwrap().len(), 0);
        assert!(bs.actors().is_empty(), "列表空 ⇒ actor 被摘除");
    }

    // ---------------- A-L4：毒伤节奏（跨 PosionDecHealthTime 命中） ----------------
    #[test]
    fn a_l4_poison_dot_rhythm_and_meat_quality() {
        let mut w = world_with_players(&[1]);
        w.poison_dec_health_time = 2500;
        let base = w.clock_ms + BUFF_EPOCH_MS;
        {
            let e = w.entities.get_mut(1).unwrap();
            e.ai.animal = true;
            e.ai.meat_quality = 5000;
            e.hp = 100;
            e.mp = 0;
            e.green_poisoning_point = 3;
            // 模拟「已存活 3000ms」的 actor（>PosionDecHealthTime=2500）⇒ 首 pass 跨门触发 DoT。
            // （新落图 actor 的 `poisoning_tick` = 落图时刻 ⇒ 首 DoT 在 2500ms 后；见
            //  `poisoning_tick_initialised_to_creation_tick`。）
            e.poisoning_tick = base - 3000;
        }
        w.make_posion(1, BuffType::GreenPoison as u8, 30, 3);
        // 第一 pass：`now - poisoning_tick` = 3000 > 2500 ⇒ 触发一次 DoT。
        let mut now = base;
        let out1 = w.buff_do_work(now);
        assert_eq!(out1.health_spell_changes.len(), 1, "首 pass 命中一次 DoT");
        assert_eq!(out1.health_spell_changes[0].0, 1);
        // `DamageHealth(GreenPoisoningPoint + 1) = 4` 点 HP。
        assert_eq!(w.entities.get(1).unwrap().hp, 96, "HP -= (point+1)=4");
        assert_eq!(
            w.entities.get(1).unwrap().ai.meat_quality,
            4000,
            "Animal ⇒ MeatQuality -= 1000"
        );
        // 第二个 pass（+50ms）未跨 2500ms ⇒ 不再扣。
        now += BUFF_PASS_INTERVAL_MS;
        let out2 = w.buff_do_work(now);
        assert!(
            out2.health_spell_changes.is_empty(),
            "未跨 PosionDecHealthTime ⇒ 不扣"
        );
        // 跨过 2500ms。
        now += 2500;
        let out3 = w.buff_do_work(now);
        assert_eq!(out3.health_spell_changes.len(), 1, "跨门 ⇒ 再扣一次");
        assert_eq!(w.entities.get(1).unwrap().hp, 92);
    }

    // ---------------- A-L5：递减节奏（**模型口径** —— 每 50ms pass 减 1） ----------------
    //
    // ⚠ 口径声明（项②；设计件 §3.1.4）：本用例保护的是「**实现与离线模型一致**」，**不是**「模型 = oracle 真值」。
    //   「每 50ms pass 减 1」是设计件 §3.1.4 的**离线模型推导**（待真机窗口 A-R2 证实），与 redlist2
    //   V4-03 原文「每 1s 减 1」是**两种读法**——孰真**只能由窗口裁定**，本仓不预断。
    #[test]
    fn a_l5_state_tick_decrements_every_pass() {
        let mut w = world_with_players(&[1]);
        let mut now = w.clock_ms + BUFF_EPOCH_MS;
        w.defence_up(1, 30);
        // 模型口径：每 50ms pass 减 1 ⇒ 连续 5 个 pass ⇒ 30 → 25。
        for _ in 0..5 {
            w.buff_do_work(now);
            now += BUFF_PASS_INTERVAL_MS;
        }
        assert_eq!(
            w.buffs.list(1).unwrap()[0].state_tick,
            25,
            "实现与模型一致（模型口径：每 50ms pass 减 1；真值待窗口 A-R2 裁定）"
        );
        // 与「每 1s 减 1」读法的**差异对照**（不作定论）：5 pass = 250ms < 1s ⇒ 该读法下应仍为 30。
        // 本仓取模型口径 ⇒ 必 ≠ 30。两种读法孰真由窗口 A-R2 判；本断言只钉「实现 ≠ 1s 读法」。
        assert_ne!(
            w.buffs.list(1).unwrap()[0].state_tick,
            30,
            "实现取模型口径（每 50ms pass 减 1）；「每 1s 减 1」读法下应为 30——孰真待窗口"
        );
    }

    // ---------------- A-L6：break quirk（StateTick==10 的 DefensePower 后不再递减后续） ----------------
    #[test]
    fn a_l6_break_skips_subsequent_buffs() {
        let mut w = world_with_players(&[1]);
        let now = w.clock_ms + BUFF_EPOCH_MS;
        // 造 `[DefensePower(StateTick=11), Transparent(StateTick=5)]`；推进 1 个 pass ⇒
        // DefensePower 减到 10 触发 break ⇒ Transparent 不被递减。
        w.buffs.add_buff(1, BuffType::DefensePower, 11, 11, now);
        w.buffs.add_buff(1, BuffType::Transparent, 5, 5, now);
        let out = w.buff_do_work(now);
        assert_eq!(
            w.buffs.list(1).unwrap()[0].state_tick,
            10,
            "DefensePower 减到 10"
        );
        assert_eq!(
            w.buffs.list(1).unwrap()[1].state_tick,
            5,
            "Transparent 不被递减（break 跳出 for(j)）"
        );
        assert!(
            out.sysmsgs.contains(&(1, BuffSysMsg::DefensePower10Sec)),
            "打出 10 秒提示"
        );
    }

    #[test]
    fn a_l6b_break_absent_would_decrement_both() {
        // 对照：把 DefensePower 换成非 break 类型（GreenPoison 到 10 不 break）⇒ 两个都递减。
        let mut w = world_with_players(&[1]);
        let now = w.clock_ms + BUFF_EPOCH_MS;
        w.buffs.add_buff(1, BuffType::GreenPoison, 11, 11, now);
        w.buffs.add_buff(1, BuffType::Transparent, 5, 5, now);
        w.buff_do_work(now);
        assert_eq!(w.buffs.list(1).unwrap()[0].state_tick, 10);
        assert_eq!(
            w.buffs.list(1).unwrap()[1].state_tick,
            4,
            "非 break 类型 ⇒ 后续 buff 照减"
        );
    }

    // ---------------- 怪物 buff 永不到期（模块头第 3 条） ----------------
    #[test]
    fn monsters_buffs_never_tick_down() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        let mut mon = Entity::new(7, "mon", 10, 10);
        mon.race = crate::entity::ACTOR_RACE_MONSTER;
        w.add_entity(mon);
        let now = w.clock_ms + BUFF_EPOCH_MS;
        w.buffs.add_buff(7, BuffType::Frozen, 5, 5, now);
        for _ in 0..10 {
            w.buff_do_work(now);
        }
        assert_eq!(
            w.buffs.list(7).unwrap()[0].state_tick,
            5,
            "怪物（race>0）的 StateTick 不递减"
        );
    }

    // ---------------- GetCharStatus 派生 ----------------
    #[test]
    fn char_status_derivation() {
        let mut w = world_with_players(&[1]);
        w.entities.get_mut(1).unwrap().char_status_ex = 0x2; // OPENHEATH
        assert_eq!(
            w.get_char_status(1),
            0x2,
            "无 buff ⇒ 仅低 16 位 CharStatusEx"
        );
        w.defence_up(1, 10); // i=0 → bit31
        assert_eq!(
            w.get_char_status(1),
            (0x8000_0000u32 as i32) | 0x2,
            "buff 位或 CharStatusEx 低 16 位"
        );
    }

    // ---------------- DamageHealth：MagicShield MP 抵扣 + LastHiter 守卫 ----------------
    #[test]
    fn damage_health_magic_shield_deducts_mp_first() {
        let mut w = world_with_players(&[1]);
        {
            let e = w.entities.get_mut(1).unwrap();
            e.magic_shield = true;
            e.hp = 100;
            e.max_hp = 100;
            e.mp = 100;
        }
        // nDamage=10 ⇒ nSpdam = Round(15) = 15 ≤ MP ⇒ MP 100→85，HP 不变。
        damage_health(&mut w, 1, 10, None);
        assert_eq!(w.entities.get(1).unwrap().mp, 85);
        assert_eq!(w.entities.get(1).unwrap().hp, 100, "MP 足够 ⇒ 不扣 HP");

        // MP 不足：nDamage=100 ⇒ nSpdam=150 > MP 85 ⇒ nSpdam=65、MP=0、nDamage=Round(65/1.5)=43。
        damage_health(&mut w, 1, 100, None);
        assert_eq!(w.entities.get(1).unwrap().mp, 0);
        assert_eq!(
            w.entities.get(1).unwrap().hp,
            100 - 43,
            "MP 抵扣后余额扣 HP"
        );
    }

    #[test]
    fn damage_health_last_hiter_guard_and_un_magic_shield() {
        let mut w = world_with_players(&[1, 2]);
        {
            let e = w.entities.get_mut(1).unwrap();
            e.magic_shield = true;
            e.hp = 100;
            e.mp = 100;
        }
        // LastHiter 是玩家且已开 UnMagicShield ⇒ 跳过 MP 抵扣、直接扣 HP。
        w.entities.get_mut(2).unwrap().un_magic_shield = true;
        damage_health(&mut w, 1, 10, Some(2));
        assert_eq!(w.entities.get(1).unwrap().mp, 100, "守卫走 else ⇒ 不扣 MP");
        assert_eq!(w.entities.get(1).unwrap().hp, 90, "直接扣 HP");

        // LastHiter 是玩家但**未**开 UnMagicShield ⇒ 仍走 MP 抵扣。
        w.entities.get_mut(2).unwrap().un_magic_shield = false;
        damage_health(&mut w, 1, 10, Some(2));
        assert_eq!(
            w.entities.get(1).unwrap().mp,
            85,
            "未开 UnMagicShield ⇒ 走 MP 抵扣"
        );
    }

    // ---------------- make_posion：CharStatus/GreenPoisoningPoint + **返回值语义** ----------------
    #[test]
    fn make_posion_sets_status_and_dot_point() {
        let mut w = world_with_players(&[1]);
        // oracle `BaseObject.cs:1966`：合法 nType **无条件 `return true`**（返回值语义 = 「nType 合法」，
        // 与 `CharStatus` 是否变化无关）。见 `World::make_posion`。
        let changed = w.make_posion(1, BuffType::GreenPoison as u8, 30, 7);
        assert!(changed, "合法 nType ⇒ 无条件 true");
        assert_eq!(w.entities.get(1).unwrap().green_poisoning_point, 7);
        assert_eq!(
            w.entities.get(1).unwrap().char_status,
            w.get_char_status(1),
            "CharStatus 写入实体"
        );
        // 再次中毒（同类型刷新，CharStatus 位不变）⇒ 返回值仍为 true（oracle 与「位是否变化」无关）。
        let changed2 = w.make_posion(1, BuffType::GreenPoison as u8, 40, 9);
        assert!(
            changed2,
            "合法 nType 恒 true（与 CharStatus 位是否变化无关）"
        );
        assert_eq!(
            w.entities.get(1).unwrap().green_poisoning_point,
            9,
            "point 仍更新"
        );
        // nType >= MAX_STATUS_ATTRIBUTE ⇒ false（`:1943` 的首门；不建 buff）。
        assert!(!w.make_posion(1, 12, 10, 0));
    }

    // ---------------- A-L7：boChg 的 CharStatus 必须含**其余活跃 buff 的位**（项①） ----------------
    //
    // 红检（牙）：把 `operate` 的 boChg 支改回 `world.get_char_status(actor_id)`（经被 `mem::take`
    //   走、此刻为空的 `world.buffs` 取）⇒ `status` 只剩 `CharStatusEx`、bit30 丢失 ⇒ 本用例必红。
    #[test]
    fn a_l7_bo_chg_status_keeps_other_active_buff_bits() {
        let mut w = world_with_players(&[1]);
        let now = w.clock_ms + BUFF_EPOCH_MS;
        // i=0 GreenPoison StateTick=1（本 pass 到期 ⇒ boChg），
        // i=1 DefensePower StateTick=5（仍活跃 ⇒ 位 30 必须保留）。
        w.buffs.add_buff(1, BuffType::GreenPoison, 1, 1, now);
        w.buffs.add_buff(1, BuffType::DefensePower, 5, 5, now);
        let out = w.buff_do_work(now);
        assert_eq!(out.status_changes.len(), 1, "到期一个 ⇒ 一条 StatusChanged");
        let (id, status) = out.status_changes[0];
        assert_eq!(id, 1);
        let bit30: i32 = 0x4000_0000u32 as i32; // i=1 ⇒ bit30
        let bit31: i32 = 0x8000_0000u32 as i32; // i=0 ⇒ bit31
        assert_eq!(status & bit30, bit30, "仍活跃的 buff（i=1）位必须保留");
        assert_eq!(status & bit31, 0, "已到期 buff（i=0，StateTick=0）位应清零");
        assert_eq!(
            w.entities.get(1).unwrap().char_status,
            status,
            "实体 CharStatus 同步"
        );
        // 其余活跃 buff 若被丢弃（原 bug）⇒ `status == 0`（无 CharStatusEx）⇒ 上面断言必红。
    }

    // ---------------- A-L8：poisoning_tick 初值 = 落图时刻（oracle ctor `GetTickCount()`，项④） ----------------
    #[test]
    fn poisoning_tick_initialised_to_creation_tick() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.poison_dec_health_time = 2500;
        w.add_entity(Entity::new(1, "p", 10, 10));
        w.make_posion(1, BuffType::GreenPoison as u8, 30, 3);
        let now = w.clock_ms + BUFF_EPOCH_MS;
        assert_eq!(
            w.entities.get(1).unwrap().poisoning_tick,
            now,
            "落图即置 poisoning_tick = 落图时刻（buff 轴）"
        );
        // 仅存活 0ms ⇒ 首 pass 不跨 2500ms 门（oracle 同：ctor 那一刻起算）。
        let out = w.buff_do_work(now);
        assert!(
            out.health_spell_changes.is_empty(),
            "新落图 actor 的首个 pass 不扣血（未跨 PosionDecHealthTime）"
        );
        // 跨 2500ms ⇒ 触发一次。
        let out2 = w.buff_do_work(now + 2501);
        assert_eq!(out2.health_spell_changes.len(), 1, "跨门 ⇒ 扣血");
    }

    // ---------------- 关闭时零动作（golden 不变） ----------------
    #[test]
    fn disabled_buff_tick_is_noop() {
        let mut w = world_with_players(&[1]);
        w.defence_up(1, 5);
        // 未 enable ⇒ `World::tick` 不跑 buff pass；直接驱动一次 tick，buff 不递减。
        w.tick();
        assert_eq!(
            w.buffs.list(1).unwrap()[0].state_tick,
            5,
            "buff_enabled=false ⇒ 不递减"
        );
        // 启用后才递减。
        w.enable_buff_tick();
        for _ in 0..BUFF_PASSES_PER_TICK {
            w.tick();
        }
        assert!(
            w.buffs.list(1).unwrap()[0].state_tick < 5,
            "enable 后每 tick 4 个 pass ⇒ 递减"
        );
        assert_eq!(TICK_INTERVAL_MS, 200);
    }
}
