//! 怪物 / 动物 AI —— oracle `AnimalObject` / `MonsterObject` 的**确定性子集**（M3④-实现 C 第 ① 片）。
//!
//! oracle 锚点（逐函数，行号复核于本片）：
//!
//! | 本模块 | oracle |
//! | --- | --- |
//! | [`walk_to`] | `BaseObject.WalkTo`（`src/M2Server/Actor/BaseObject.cs:601-...`） |
//! | [`turn_to`] | `BaseObject.TurnTo`（`BaseObject.cs:1582`） |
//! | [`wondering`] | `AnimalObject.Wondering`（`src/M2Server/Actor/AnimalObject.cs:425-441`） |
//! | [`search_target`] | `AnimalObject.SearchTarget`（`AnimalObject.cs:388-423`） |
//! | [`goto_target_xy`] | `AnimalObject.GotoTargetXY`（`AnimalObject.cs:174-249`） |
//! | [`think`] | `MonsterObject.Think`（`src/M2Server/Monster/MonsterObject.cs:187-215`） |
//! | [`attack_target`] | `MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）+ `BaseObject.GetAttackDir`（`BaseObject.cs:1264`） |
//! | [`monster_run`] | `MonsterObject.Run`（`MonsterObject.cs:251-360`） |
//!
//! ## RNG 取数次序（Claim B 两侧 site/order 账本的输入；**不许为可测性改序**）
//!
//! - `Wondering`：先 1 次 `Random(20)`；≠0 直接返回（**不再抽**）。==0 时 1 次 `Random(4)`：
//!   ==1 ⇒ 1 次 `RandomByte(8)` 后 `TurnTo`；否则 `WalkTo(Dir)`（`WalkTo`/`TurnTo` 本身不抽）。
//! - `GotoTargetXY`：进入后（目标格与当前格不同）先 `WalkTo(nDir)`，随后**无条件** 1 次
//!   `Random(3)`（用于未移动时的 8 向重试循环；每次重试不再抽）。
//! - `Think`：仅当 `now - ThinkTick > 3000` 时刷新；`DupMode` 支 `WalkTo(RandomByte(8))`。
//! - `AttackTarget`：命中门槛通过且到时间才攻击；攻击取数走 [`crate::world::World::attack_once`]
//!   （= oracle `GetBaseAttackPoewr` + `_Attack` 的怪物路径）。
//!
//! ## 与 oracle 的建模差异（登记，不在本片修）
//!
//! - **线程模型**：oracle 的 `ProcessMonsters` 跑在独立线程（`WorldServer.MonGen.cs:192`），
//!   本 crate 单线程 tick 串行（设计文档 §4.2 硬约束①）。本片把「每 tick 对每只怪跑一次
//!   `Run()`」按**实体槽位顺序**串行执行（顺序来自 `EntityStore` 的显式槽位索引）。
//! - **`BaseObject.Run` 的公共段已分归两处**（`BaseObject.Base.cs:24-316`）：
//!   - [`base_object_run_monster`]：恢复 `HealthTick/SpellTick`（`:39-66`）、体力点段（`:105-110`）、
//!     `TargetCret` 清理（`:111-118`）——每只怪在 [`monster_run`] 之后跑（oracle `MonsterObject.Run`
//!     末尾的 `base.Run()`，`MonsterObject.cs:255/359`）；
//!   - [`crate::world::World`]：`HP==0 ⇒ Die()`（`:70-73`，由 `settle_deaths` 死亡结算等价）、
//!     `LastHiter` 30s / `ExpHiter` 6s 过期（`:119-132`，由 `expire_hitters`）。
//! - **已建模**：`SuperMan`（`:32-36`，见 [`base_object_run_monster`]；置位与 `Die()` 早退见
//!   卡片 `openmir2-superman-normnpc` 与 `World::settle_deaths`）。**仍未建模**（登记）：`BaseObject.Run` 的 Master 变色支
//!   （`:133-215`，与 `MonsterObject.Run` 的跟随支不同）、`ShowHp`/`VerifyTick` 的 `ShowHp` 支
//!   （`:217-256`；`VerifyTick` 已接）。死亡支
//!   `MakeGhost`（`:85-101`）已由本片接上（[`make_ghost`]，门见
//!   [`crate::mongen::monster_death_gate`]）。**⑨-3 已接**：`CheckRoyalty()`（`MonsterObject.cs:352`）
//!   在 [`monster_run`] 的两个提前返回口之外复刻调用；宝宝列表清表（`BaseObject.Base.cs:208-217`）
//!   在 [`crate::world::World::tick`] 里集中跑（见 [`crate::slave`]）。
//! - **构造期 RNG**：oracle 在 `new MonsterObject()` 时先抽 `BaseObject` 的 `Random(1500)`+
//!   `Random(2000)`、`AnimalObject` 的 `Random(3000)`×2、`MonsterObject` 的 `Random(2000)`
//!   （`BaseObject.cs:439/441`、`AnimalObject.cs:76/77`、`MonsterObject.cs:65`），**再**由
//!   `CreateMonster` 抽 `RandomByte(8)` 的 `Dir`（`WorldServer.MonGen.cs:740`）。该缺口已由
//!   卡 `openmir2-m3-mongen-load-spawn` 的 F-1 返工闭合：生成路径把构造抽存进
//!   `Entity.ctor_draws`（[`MonsterCtorDraws`]），AI 在宿主接线时只**消费**（
//!   [`AnimalState::seed_from_spawn`](crate::entity::AnimalState::seed_from_spawn)），**不重抽**。
//! - **专用怪子类**：MB-1..MB-9 的 34 个 `Run` 覆写已按批接上（见 [`monster_class_run`] 的
//!   race 分派表）；`RunAway` 是**死类**（`CreateMonster` 无 case），已按「内容出现即等价」
//!   实现并登记为 [`run_away_run`]。`Mission` 触发点仍未建模：默认 `Mission == false`，
//!   与「普通 `MonsterObject`、无任务」的 oracle 执行路径一致。
//! - **已接（卡 `openmir2-run-gap-monster-master-follow`）**：`MonsterObject.Run` 的
//!   `Master` 跟随段（`MonsterObject.cs:299-329`）、`Master.SlaveRelax` 早退（`:333-338`）
//!   与 `RunAwayTime` 到期（`:329-345`）。跟随段在 `TargetCret == null` 支内按 oracle 同点接入
//!   （[`master_get_back_position`] / `Envir.GetMovingObject` 占位回退 / `SpaceMove` 同服分支）。
//! - **`CrazyMode`（`MonsterObject.cs:49-57/353-356/465-480`）**：状态与到期已建模
//!   （[`open_crazy_mode`]/[`break_crazy_mode`]/[`crazy_mode_expiry`]）；生产写入点 =
//!   20 诱惑之光的 `OpenCrazyMode(Random(20)+10)`（`MagicManager.cs:797/805`，抽数先由
//!   [`crate::magic_effect::mag_tamming`] 按同序消费并把值随 `crazy_interval` 返回，
//!   宿主 `cast_tamming_spell` 落状态）。`GetChrColor` 的 `crazy_mode ⇒ 0xF9` 色支仍登记未实现。
//! - **`AnimalObject.Run` 的 HolySeize 到期**（`AnimalObject.cs:142-149`）已建模为
//!   [`animal_run`]（严格 `>`），由 [`monster_ai_tick`] 在 `monster_run` 之后、`base.Run` 之前调用
//!   （= oracle 的 `MonsterObject.Run` 尾 `base.Run()` → `AnimalObject.Run`）。`RefNameColor` 帧不在此层。

use crate::combat::{is_proper_target, is_proper_target_grouped_direct};
use crate::entity::{Entity, ACTOR_RACE_PLAY};
use crate::magic::{
    front_position, DIR_DOWN, DIR_DOWN_LEFT, DIR_DOWN_RIGHT, DIR_LEFT, DIR_RIGHT, DIR_UP,
    DIR_UP_LEFT, DIR_UP_RIGHT,
};
use crate::world::World;

/// `Messages.RM_TURN`（`src/OpenMir2/Messages.cs:555`）——转向帧（`SM_TURN` + `SM_FEATURECHANGED`）。
pub const RM_TURN: u16 = 10001;
/// `Messages.RM_HIT`（`src/OpenMir2/Messages.cs:554`）——`AnimalObject.Attack` 的攻击动作帧。
pub const RM_HIT: u16 = 10004;
/// `Messages.RM_DIGUP`（`src/OpenMir2/Messages.cs:614`）——出土帧（`SM_DIGUP`）。
pub const RM_DIGUP: u16 = 10200;
/// `Messages.RM_SPELL`（`src/OpenMir2/Messages.cs:562` = 10008）——`FireballMonster.Run` 的
/// 施法帧（`FireballMonster.cs:39`：`SendRefMsg(RM_SPELL, 48, tx, ty, 48, "")`）。
pub const RM_SPELL: u16 = 10008;
/// `Messages.RM_MAGICFIRE`（`src/OpenMir2/Messages.cs:582` = 10120）——`FireballMonster.Run`
/// 的魔法动画帧（`FireballMonster.cs:40`：`SendRefMsg(RM_MAGICFIRE, 0, MakeWord(2, 48),
/// MakeLong(tx, ty), targetId, "")`）。
pub const RM_MAGICFIRE: u16 = 10120;
/// `Messages.RM_DIGDOWN`（`src/OpenMir2/Messages.cs:615`）——入土帧（`SM_DIGDOWN`）。
pub const RM_DIGDOWN: u16 = 10201;
/// `Messages.RM_CHANGEFACE`（`src/OpenMir2/Messages.cs:640` = 10415）——
/// `ElfWarriorMonster.Run` 换形成功帧（`ElfWarriorMonster.cs:149`）。
pub const RM_CHANGEFACE: u16 = 10415;
/// `Messages.RM_10205`（`src/OpenMir2/Messages.cs:618`）——`SM_716` 帧。
pub const RM_10205: u16 = 10205;
/// `Messages.RM_FLYAXE`（`src/OpenMir2/Messages.cs:616`）——`DualAxeMonster.FlyAxeAttack` 的飞斧帧
/// （`DualAxeMonster.cs:29`；`SM_FLYAXE(22)`）。
pub const RM_FLYAXE: u16 = 10202;
/// `Messages.RM_LIGHTING`（`src/OpenMir2/Messages.cs:617`）——`LightingZombi.LightingAttack`
/// （`LightingZombi.cs:22`；`SM_LIGHTING(23)`）。
pub const RM_LIGHTING: u16 = 10204;
/// `Messages.RM_ALIVE`（`src/OpenMir2/Messages.cs:612`）——`BaseObject.ReAlive()`
/// （`BaseObject.Base.cs:354-358`；`ZilKinZombi.Run` 的复生；`SM_ALIVE(27)`）。
pub const RM_ALIVE: u16 = 10153;
/// `Messages.RM_CHARSTATUSCHANGED`（`src/OpenMir2/Messages.cs` = 10139）——`StatusChanged()`
/// 的状态帧（`BaseObject.cs` 的 `SendRefMsg(RM_CHARSTATUSCHANGED, 0, CharStatus, 0, 0, "")`；
/// 玩家侧 `PlayObject.Message.cs` 出 `SM_CHARSTATUSCHANGED(657)`）。
/// 生产来源见 [`frost_tiger_run`]（`MagMakePrivateTransparent` 收尾的 `StatusChanged()`）。
pub const RM_CHARSTATUSCHANGED: u16 = 10139;

/// `_Attack` 的 `SendStruckDelayMsg(RM_REFMESSAGE, …, 200)`（`BaseObject.Attack.cs:57`）。
/// `AttackDirect` 的 500ms（`:78`）尚无移植调用点（`AttackDirect` 未建模，登记）。
pub const ATTACK_STRUCK_DELAY_MS: i64 = 200;
/// `AnimalObject.HitMagAttackTarget` 的 `SendStruckDelayMsg(…, 200)`
/// （`AnimalObject.cs:379`；ScultureKing 走它）。
pub const HIT_MAG_STRUCK_DELAY_MS: i64 = 200;
/// `GasAttackMonster.GasAttack` 的 `SendStruckDelayMsg(…, 300)`（`GasAttackMonster.cs:30`）。
pub const GAS_STRUCK_DELAY_MS: i64 = 300;
/// `ArcherGuard.AttackTarger`（`ArcherGuard.cs:34`）/ `DualAxeMonster.FlyAxeAttack`
/// （`DualAxeMonster.cs:28`）的延时基数：`max(|Δx|,|Δy|)*50 + 600`。
pub const GUARD_STRUCK_DELAY_BASE_MS: i64 = 600;

/// `BaseObject.WalkTo` 的 `Dir → (dx, dy)`（`BaseObject.cs:601-...` 的 switch）。
///
/// 与 oracle 同构：越界由调用方（`World::try_step_now` 的格子校验）拒绝，本函数只给增量。
#[must_use]
pub fn dir_delta(dir: u8) -> (i32, i32) {
    match dir {
        DIR_UP => (0, -1),
        DIR_UP_RIGHT => (1, -1),
        DIR_RIGHT => (1, 0),
        DIR_DOWN_RIGHT => (1, 1),
        DIR_DOWN => (0, 1),
        DIR_DOWN_LEFT => (-1, 1),
        DIR_LEFT => (-1, 0),
        DIR_UP_LEFT => (-1, -1),
        _ => (0, 0),
    }
}

/// `BaseObject.TurnTo`（`BaseObject.cs:1582`）：只改 `Dir`（帧面 `RM_TURN` 由接线层发）。
pub fn turn_to(world: &mut World, actor_id: i32, dir: u8) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
}

/// `BaseObject.WalkTo(btDir, boFlag)`（`BaseObject.cs:601`）：**先置 `Dir`**（即使走不动也保留），
/// 再尝试单步移动。返回是否真的移动（oracle 的 `result`）。
///
/// `fearFire`（`BoFearFire`）在本片未建模（普通怪 false；火墙 `CanSafeWalk` 语义不在本片）。
pub fn walk_to(world: &mut World, actor_id: i32, dir: u8) -> bool {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let (dx, dy) = dir_delta(dir);
    if dx == 0 && dy == 0 {
        return false;
    }
    world.try_step_now(actor_id, dx, dy, 1)
}

/// `Envirnoment.GetXYObjCount`（`src/M2Server/Maps/Envirnoment.cs:1062-1093`）的 actor 计数子集：
/// 统计 `(x, y)` 格上 `!Ghost && !Death && !FixedHideMode && !ObMode` 的 actor 对象数。
pub(crate) fn count_at_cell(world: &World, map_id: usize, x: i32, y: i32) -> i32 {
    let Some(map) = world.maps.get(map_id) else {
        return 0;
    };
    let (cell, ok) = map.get_cell_info(x, y);
    if !ok || !cell.is_available() {
        return 0;
    }
    let mut count = 0;
    for o in &cell.obj_list {
        if !o.actor_object {
            continue;
        }
        if world
            .entities
            .get(o.cell_obj_id)
            .is_some_and(|e| !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode)
        {
            count += 1;
        }
    }
    count
}

/// `BaseObject.GetAttackDir`（`BaseObject.cs:1264-1313`）：目标在**八邻格**时给出朝向。
///
/// `None` = 不满足邻接（含同格）；`Some(dir)` = 命中（oracle 返回 true 且 `btDir` 有值）。
#[must_use]
pub fn get_attack_dir(world: &World, actor_id: i32, target_id: i32) -> Option<u8> {
    let a = world.entities.get(actor_id)?;
    let t = world.entities.get(target_id)?;
    let (ax, ay) = (a.x, a.y);
    let (tx, ty) = (t.x, t.y);
    if !(ax - 1 <= tx && ax + 1 >= tx && ay - 1 <= ty && ay + 1 >= ty) || (ax == tx && ay == ty) {
        return None;
    }
    // oracle 的判定顺序：Left, Right, Up, Down, UpLeft, UpRight, DownLeft, DownRight。
    if ax - 1 == tx && ay == ty {
        return Some(DIR_LEFT);
    }
    if ax + 1 == tx && ay == ty {
        return Some(DIR_RIGHT);
    }
    if ax == tx && ay - 1 == ty {
        return Some(DIR_UP);
    }
    if ax == tx && ay + 1 == ty {
        return Some(DIR_DOWN);
    }
    if ax - 1 == tx && ay - 1 == ty {
        return Some(DIR_UP_LEFT);
    }
    if ax + 1 == tx && ay - 1 == ty {
        return Some(DIR_UP_RIGHT);
    }
    if ax - 1 == tx && ay + 1 == ty {
        return Some(DIR_DOWN_LEFT);
    }
    if ax + 1 == tx && ay + 1 == ty {
        return Some(DIR_DOWN_RIGHT);
    }
    None
}

/// `AnimalObject.Wondering`（`AnimalObject.cs:425-441`）。
///
/// 取数：`Random(20)`；非 0 返回。否则 `Random(4)`：==1 ⇒ `RandomByte(8)` + `TurnTo`；
/// 否则 `WalkTo(Dir)`。
pub fn wondering(world: &mut World, actor_id: i32) {
    if world.rng.random_below(20) != 0 {
        return;
    }
    if world.rng.random_below(4) == 1 {
        let dir = world.rng.random_byte(8);
        turn_to(world, actor_id, dir);
    } else {
        let dir = world.entities.get(actor_id).map_or(0, |e| e.ai.dir);
        walk_to(world, actor_id, dir);
    }
}

/// `AnimalObject.SearchTarget`（`AnimalObject.cs:388-416`）。
///
/// 逐句复刻 oracle 的 `for` + `ClearTargetCreat`（`BaseObject.Base.cs:568-585`）：
/// 无效项分支先从可见集**删首个匹配项**再 `DelTargetCreat()`（`TargetCret = null`、
/// `TargetX/Y = -1`），然后 `continue` ⇒ C# `for` 尾的 `i++` **跳过**因 `RemoveAt`
/// 前移的下一项。该跳项是 oracle 的真实怪癖（硬约束③：照抄，不当 bug 修）：
/// 初始无目标、列表 `[无效, 合法]` ⇒ 合法项被跳过，收尾 `searchTarget == null` ⇒
/// **保持无目标**。末尾 `if (searchTarget != null) SetTargetCreat(searchTarget)`。
///
/// 目标有效性用**直通版** `IsProperTarget`（`BaseObject.Base.cs:470-473` = `IsAttackTarget`）：
/// 死亡/幽灵已由上面的无效项分支排除，`Invisible` oracle 不查（审计 G09），
/// 因此这里**不能**用折入 death/ghost/invisible 的 [`is_proper_target`]。
pub fn search_target(world: &mut World, actor_id: i32) {
    let Some((cx, cy, map_id, cool_eye, mut visible)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.map_id, e.ai.cool_eye, e.visible_actors.clone()))
    else {
        return;
    };
    // oracle `int n10 = 999;`（`AnimalObject.cs:390`）：全部候选距离都 ≥999 时**一个都不选**
    // （±15 窗口内曼哈顿距离上限 30，不可达，但按 1:1 保留该初值）。
    let mut n10 = 999;
    let mut best: Option<i32> = None;
    let mut cleared_target = false;
    // `for (int i = 0; i < VisibleActors.Count; i++)`：条件每轮重取（列表会被 `RemoveAt`
    // 缩短）；无效项分支的 `i += 1` 对应 C# 的 `continue` ⇒ `for` 尾 `i++`。
    let mut i = 0;
    while i < visible.len() {
        let target_id = visible[i].target;
        // `baseObject.Death || baseObject.Ghost || baseObject.Envir != Envir ||
        //  |Δx| > 15 || |Δy| > 15`
        let invalid = match world.entities.get(target_id) {
            None => true,
            Some(t) => {
                t.death
                    || t.ghost
                    || t.map_id != map_id
                    || (t.x - cx).abs() > 15
                    || (t.y - cy).abs() > 15
            }
        };
        if invalid {
            // `ClearTargetCreat(baseObject)`：`VisibleActors` 里删**首个** `BaseObject == 目标`
            // 的项（`VisibleActors.RemoveAt(i); break;`），随后 `DelTargetCreat()`。
            if let Some(pos) = visible.iter().position(|v| v.target == target_id) {
                visible.remove(pos);
            }
            cleared_target = true;
            i += 1;
            continue;
        }
        // oracle 在 `ClearTargetCreat` 之后还写了一遍 `if (!baseObject.Death)`（`AnimalObject.cs:400`）：
        // 上面的 `baseObject.Death` 已 `continue` ⇒ 该层不可达（照搬时省略，行为等价）。
        // 隐藏门读的是 `HideMode`（**不是** `FixedHideMode`，后者只用于 `GetXYObjCount`/`Run`）。
        if let (Some(a), Some(t)) = (world.entities.get(actor_id), world.entities.get(target_id)) {
            if is_proper_target_grouped_direct(a, t, &[]) && (!t.hide_mode || cool_eye) {
                let dist = (cx - t.x).abs() + (cy - t.y).abs();
                if dist < n10 {
                    n10 = dist;
                    best = Some(target_id);
                }
            }
        }
        i += 1;
    }
    if cleared_target {
        if let Some(e) = world.entities.get_mut(actor_id) {
            // `ClearTargetCreat` 的尾部 `DelTargetCreat()`（`AnimalObject.DelTargetCreat`
            // 额外把 `TargetX/TargetY` 置 -1）。多次无效项只影响最终态，逐次调用等价。
            e.ai.target_creature = None;
            e.ai.target_x = -1;
            e.ai.target_y = -1;
        }
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.visible_actors = visible;
    }
    if let Some(target_id) = best {
        set_target_creature(world, actor_id, target_id);
    }
}

/// `BaseObject.SetTargetCreat`：置 `TargetCret` 并刷新 `TargetFocusTick`。
pub fn set_target_creature(world: &mut World, actor_id: i32, target_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.target_creature = Some(target_id);
        e.ai.target_focus_tick = now;
    }
}

/// `BaseObject.DelTargetCreat`（`AnimalObject.cs:151` override 追加 `TargetX/Y = -1`）。
pub fn del_target_creature(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.target_creature = None;
        e.ai.target_x = -1;
        e.ai.target_y = -1;
    }
}

/// `BaseObject.ClearTargetCreat`（`BaseObject.Base.cs:568-585`）：先把目标从 `VisibleActors`
/// 里删掉（首个匹配，`VisibleActors[i].BaseObject == baseObject` ⇒ Rust 的 `v.target`），
/// 再 `DelTargetCreat()`。
///
/// 与 [`del_target_creature`] 分开：oracle 里 `ClearTargetCreat` 用于 `Run()` 的目标清理段
/// （`BaseObject.Base.cs:111-118`）、`DelTargetCreat` 用于其它路径，两者不同源。
pub fn clear_target_creature(world: &mut World, actor_id: i32, target_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        if let Some(i) = e.visible_actors.iter().position(|v| v.target == target_id) {
            e.visible_actors.remove(i);
        }
        e.ai.target_creature = None;
        e.ai.target_x = -1;
        e.ai.target_y = -1;
    }
}

/// `AnimalObject.GotoTargetXY`（`AnimalObject.cs:174-249`）。
///
/// 目标格 ≠ 当前格时：按 Δ 定主方向 → `WalkTo` → **无条件** `Random(3)` → 最多 8 次
/// （`Dir.Up..=Dir.UpLeft`）在「仍未移动」时按 `n20` 顺时针/逆时针微调方向再 `WalkTo`。
pub fn goto_target_xy(world: &mut World, actor_id: i32) {
    let Some((cx, cy, tx, ty)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.ai.target_x, e.ai.target_y))
    else {
        return;
    };
    if cx == tx && cy == ty {
        return;
    }
    let mut n_dir = DIR_DOWN;
    if tx > cx {
        n_dir = DIR_RIGHT;
        if ty > cy {
            n_dir = DIR_DOWN_RIGHT;
        }
        if ty < cy {
            n_dir = DIR_UP_RIGHT;
        }
    } else if tx < cx {
        n_dir = DIR_LEFT;
        if ty > cy {
            n_dir = DIR_DOWN_LEFT;
        }
        if ty < cy {
            n_dir = DIR_UP_LEFT;
        }
    } else if ty > cy {
        n_dir = DIR_DOWN;
    } else if ty < cy {
        n_dir = DIR_UP;
    }
    let (old_x, old_y) = (cx, cy);
    walk_to(world, actor_id, n_dir);
    let n20 = world.rng.random_below(3);
    for _ in DIR_UP..=DIR_UP_LEFT {
        let Some((nx, ny)) = world.entities.get(actor_id).map(|e| (e.x, e.y)) else {
            return;
        };
        if old_x == nx && old_y == ny {
            if n20 != 0 {
                n_dir += 1;
            } else if n_dir > 0 {
                n_dir -= 1;
            } else {
                n_dir = DIR_UP_LEFT;
            }
            if n_dir > DIR_UP_LEFT {
                n_dir = DIR_UP;
            }
            walk_to(world, actor_id, n_dir);
        }
    }
}

/// `AnimalObject.CanMove`（`AnimalObject.cs:157-160`）：`!Ghost && !Death && Stone == 0`。
///
/// `Stone` 读的是 `StatusTimeArr[PoisonState.STONE]`（`Entity::status_time[POISON_STONE]`），
/// **不是** `BaseObject.StoneMode`：`ScultureMonster` ctor 置 `StoneMode = true` 而
/// `StatusTimeArr[STONE]` 仍为 0 ⇒ `CanMove()` 为真（`ScultureMonster.cs:43` 的 `Run` 门）。
#[must_use]
pub fn can_move(e: &Entity) -> bool {
    !e.ghost && !e.death && e.status_time[crate::entity::POISON_STONE] == 0
}

/// `MonsterObject.Think`（`MonsterObject.cs:187-215`）：3s 一次的 `DupMode`/目标有效性刷新，
/// `DupMode` 时 `WalkTo(RandomByte(8))`，移动成功即清 `DupMode` 并返回 true。
pub fn think(world: &mut World, actor_id: i32) -> bool {
    think_refresh(world, actor_id);
    think_dup_move(world, actor_id, true)
}

/// `MagicMonster.Think`（`MagicMonster.cs:18-45`）——与 `MonsterObject.Think` 逐字相同，
/// **唯一差异**：`DupMode` 支**没有** `if (HolySeize) return false;` 早退（`MagicMonster.cs:33-43`）。
///
/// oracle 的两个 `Think` 都是 `private`（`MonsterObject.cs:187` / `MagicMonster.cs:18`）⇒
/// 静态绑定，**不是虚分派**：`MonsterObject.Run`（Pass 2）里的 `Think()` 恒落
/// `MonsterObject.Think`（**有**门）；`MagicMonster.Run`（Pass 1，`:90`）里的 `Think()` 恒落
/// 本函数（**无**门）。`MagicMonster.Run` 尾部 `base.Run()`（`:185`）进 `MonsterObject.Run`，因此 `FireballMonster(215)` 的两段 AI 是「Pass 1 无门 → Pass 2 有门」。
///
/// 本仓由调用点显式选择：Pass 1 = [`magic_monster_run`] 直调本函数，Pass 2 =
/// [`monster_run`] 直调 [`think`]；不再按 race 做「两个 pass 都走无门版」的错分派。
fn magic_monster_think(world: &mut World, actor_id: i32) -> bool {
    think_refresh(world, actor_id);
    think_dup_move(world, actor_id, false)
}

/// `Think` 的 3s 刷新段（`MonsterObject.cs:190-201` / `MagicMonster.cs:21-32` 逐字相同）。
fn think_refresh(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some((x, y, map_id)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.map_id)) else {
        return;
    };
    if now - world.entities.get(actor_id).map_or(0, |e| e.ai.think_tick) > 3 * 1000 {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.think_tick = now;
        }
        if count_at_cell(world, map_id, x, y) >= 2 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.dup_mode = true;
            }
        }
        let target = world
            .entities
            .get(actor_id)
            .and_then(|e| e.ai.target_creature);
        if let Some(t) = target {
            let proper = world
                .entities
                .get(actor_id)
                .zip(world.entities.get(t))
                .is_some_and(|(a, b)| is_proper_target_grouped_direct(a, b, &[]));
            if !proper {
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.target_creature = None;
                }
            }
        }
    }
}

/// `Think` 的 `DupMode` 支（`MonsterObject.cs:203-215` / `MagicMonster.cs:34-43`）。
///
/// `honor_holy_seize = true` ⇒ `MonsterObject.Think` 的 `if (HolySeize) return false;`
/// （`MonsterObject.cs:205-208`）；`false` ⇒ `MagicMonster.Think`（无该门）。
fn think_dup_move(world: &mut World, actor_id: i32, honor_holy_seize: bool) -> bool {
    let mut result = false;
    let dup = world.entities.get(actor_id).is_some_and(|e| e.ai.dup_mode);
    if dup {
        if honor_holy_seize
            && world
                .entities
                .get(actor_id)
                .is_some_and(|e| e.ai.holy_seize)
        {
            return false;
        }
        let (old_x, old_y) = match world.entities.get(actor_id) {
            Some(e) => (e.x, e.y),
            None => return false,
        };
        let dir = world.rng.random_byte(8);
        let bo_fear_fire = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.bo_fear_fire);
        let _ = bo_fear_fire; // `WalkTo(..., BoFearFire)` 的 fearFire 子集不在本片
        walk_to(world, actor_id, dir);
        if let Some(e) = world.entities.get(actor_id) {
            if old_x != e.x || old_y != e.y {
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.dup_mode = false;
                }
                result = true;
            }
        }
    }
    result
}

/// `MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）。
///
/// 邻接 ⇒ 到点（`now - AttackTick > NextHitTime`）则刷新 `AttackTick`/`TargetFocusTick`、
/// `TurnTo(btDir)`、`AttackDir`；不邻接 ⇒ 同图则 `SetTargetXy(目标格)`，异图则 `DelTargetCreat`。
pub fn attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
                e.ai.holy_seize = false;
            }
            turn_to(world, actor_id, bt_dir);
            // oracle `AnimalObject.Attack`：`GetBaseAttackPoewr()`（怪物 = `AttackPowerMode::Base`）
            // + `AttackDir(target, nPower, nDir)`。`World::attack_once` 的怪物路径逐取数同构
            // （`combat::attack_dir` 对非玩家攻方不加 `AttackSuccess`）。
            //
            // **W4 下行接线**：oracle `AnimalObject.Attack`（`AnimalObject.cs:165-170`）在
            // `AttackDir` 之后 `SendAttackMsg(RM_HIT, …)`；本次结算（`_Attack` 的伤害/命中）
            // 与玩家 `CM_HIT` 攻击走**同一条** `SM_STRUCK` 下行（受击者 `RM_REFMESSAGE` →
            // `SendRefMsg(RM_STRUCK)` → 视野内广播）。因此把结算记进 `pending_ai_attacks`
            // （宿主 `World::tick` 转抄进 `TickRecord::attacks`）⇒ 既有 `drain_combat_downlink`
            // 的 ① 段无需改动即出「怪打玩家」的 `SM_STRUCK` 帧。
            if let Some(outcome) = world.attack_once(actor_id, target_id) {
                world
                    .pending_ai_attacks
                    .push((actor_id, target_id, outcome));
            }
            // **Lane 1（`openmir2-monster-attack-smhit`）**：`AnimalObject.Attack`
            // （`AnimalObject.cs:167-171`）的第二句 `SendAttackMsg(RM_HIT, Dir, CurrX, CurrY)`
            // ——在 `AttackDir` **之后无条件**发（`_Attack` 的 `nPower==0` 支、被闪避、
            // 目标无效都照发；只有 `GetAttackDir` 不邻接时根本不进本分支）。
            // `AttackDir` 已把 `Dir` 置为 `nDir`（上面的 `turn_to`），坐标取**发帧时刻**的
            // `CurrX/CurrY`；`SendRefMsg` 的收件人语义由 `send_ref_msg` 在发帧时刻快照
            // （±12 格、仅 `Race==Play`、排除自己；`ObMode/FixedHideMode` ⇒ `self_only`）。
            if let Some((dir, x, y)) = world.entities.get(actor_id).map(|e| (e.ai.dir, e.x, e.y)) {
                send_ref_msg(world, actor_id, RM_HIT, i32::from(dir), x, y, 0);
            }
        }
        true
    } else {
        let (same_map, tx, ty) = match (
            world.entities.get(actor_id).map(|e| e.map_id),
            world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
        ) {
            (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
            _ => (false, 0, 0),
        };
        if same_map {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// `AnimalObject.Struck(hiter)`（`AnimalObject.cs:334-360`）——怪物**被命中**时的索敌与受击后果。
///
/// oracle 调用点 = `Operate(RM_STRUCK)`（`AnimalObject.cs:118-124`）：
/// `SetLastHiter(struckObject); Struck(struckObject); BreakHolySeizeMode(); …`。
/// 本骨架由 [`crate::world::World::attack_once`] 把命中者按入队序压进目标的
/// [`crate::entity::AnimalState::pending_struck`]（= oracle `SendMsg` 的 `MsgQueue.Enqueue`），
/// 再由 [`base_object_run_monster`]（= 目标自己的 `base.Run()` 的消息段）出队调本函数
/// ⇒ `Struck` 的取数落在「怪自己的 `base.Run`」这个 site（与 oracle 同）。
/// `SetLastHiter`/`BreakHolySeizeMode` 不消费随机数，仍折在 `attack_once`（见该处注释）。
///
/// 取数（oracle 逐行，`AnimalObject.cs:334-360`）：
/// - 仅当 `TargetCret != null && !GetAttackDir(TargetCret, btDir)` 时抽 **1 次 `Random(6)`**
///   （短路：无目标或已邻接 ⇒ **不抽**）；
/// - 命中门通过后 `IsProperTarget(hiter)` 为真 ⇒ `SetTargetCreat(hiter)`；
/// - `if (Animal)` ⇒ 再抽 **1 次 `Random(300)`** 扣 `MeatQuality`（本骨架 `Animal` 恒 `false`，登记）；
/// - 尾：`AttackTick += 150 - MIN(130, Abil.Level * 4)`（不抽数）。
///
/// **前置**：调用方保证 `hiter_id` 在场（oracle `Operate` 的 `struckObject != null` 门在出队处）。
pub fn animal_struck(world: &mut World, victim_id: i32, hiter_id: i32) {
    // **MB-8（R36）**：`SoccerBall.Struck`（`SoccerBall.cs:76-86`）**完全覆写**了
    // `AnimalObject.Struck`（无 `base.Struck()`）⇒ 不写 `StruckTick`、不选目标、不扣肉质、
    // 不加 `AttackTick`，只写 `Dir/N550/TargetX/Y`。
    if world.entities.get(victim_id).is_some_and(|e| e.race == 120) {
        soccer_ball_struck(world, victim_id, hiter_id);
        return;
    }
    let now = world.clock_ms;
    // `StruckTick = HUtil32.GetTickCount();`（`AnimalObject.cs:336`）
    if let Some(e) = world.entities.get_mut(victim_id) {
        e.ai.struck_tick = now;
    }
    let cur_target = world
        .entities
        .get(victim_id)
        .and_then(|e| e.ai.target_creature);
    // `if (TargetCret == null || GetAttackDir(TargetCret, ref btDir) || M2Share.RandomNumber.Random(6) == 0)`
    let take = match cur_target {
        None => true,
        Some(t) => get_attack_dir(world, victim_id, t).is_some() || world.rng.random_below(6) == 0,
    };
    if take {
        let proper = world
            .entities
            .get(victim_id)
            .zip(world.entities.get(hiter_id))
            .is_some_and(|(a, b)| is_proper_target_grouped_direct(a, b, &[]));
        if proper {
            set_target_creature(world, victim_id, hiter_id);
        }
    }
    // `if (Animal) { MeatQuality = (ushort)(MeatQuality - Random(300)); if (MeatQuality < 0) MeatQuality = 0; }`
    // `Animal` 由怪种分类决定（[`is_animal_race`]）；`e.ai.animal` 保留为显式覆盖位（恒 `false`）。
    let is_animal = world
        .entities
        .get(victim_id)
        .is_some_and(|e| e.ai.animal || is_animal_race(e.race));
    if is_animal {
        let dec = world.rng.random_below(300);
        if let Some(e) = world.entities.get_mut(victim_id) {
            let v = i32::from(e.ai.meat_quality) - dec;
            e.ai.meat_quality = if v < 0 { 0 } else { v as u16 };
        }
    }
    // `AttackTick = AttackTick + (150 - HUtil32._MIN(130, Abil.Level * 4));`
    let level = world.entities.get(victim_id).map_or(0, |e| e.level);
    let add = 150 - (i32::from(level) * 4).min(130);
    if let Some(e) = world.entities.get_mut(victim_id) {
        e.ai.attack_tick += i64::from(add);
    }
    // `GuardUnit.Struck`（`GuardUnit.cs:27-33`）：`Castle != null` ⇒ 置 `CrimeforCastle`。
    guard_unit_struck(world, victim_id, now);
}

/// `CreateMonster`（`WorldServer.MonGen.cs:507-728`）按 `monsters.Race` 选怪类；本函数给出
/// **`ATMonster` 家族中「未覆写 `Run`」** 的 race 集——这些类直接继承 `ATMonster.Run`
/// （`ATMonster.cs:13-24`），在 `base.Run()` 之前按时钟门调 `SearchTarget()`。
///
/// 由 `CreateMonster` 的 `race → 类` 映射 + 各派生类是否覆写 `Run` 推出：
/// - 本体：`53`（`AnimalWolf`=`new AtMonster`）、`81`/`86`/`88`/`89`（`new AtMonster`）；
/// - `83`（`new SlowAtMonster`，`SlowATMonster.cs:3` `: AtMonster` 无覆写）。
///
/// 审计 `audit-attack-search-coverage-20261018.md` G08：原集合 `{53,81,83,86,88,89}` 漏了
/// **继承 `ATMonster.Run` 但未覆写 `Run`** 的 9 个 race。逐 race 实读 `CreateMonster`
/// （`WorldServer.MonGen.cs:566-690`）+ 类继承链确认：
///
/// | race | 类 | 继承链 | 覆写 `Run`？ | 数据可达 |
/// | ---: | --- | --- | --- | --- |
/// | 82 | `SpitSpider` | `AtMonster` | 否 | 是（10 条） |
/// | 84 | `Scorpion` | `AtMonster` | 否 | 是 |
/// | 90 | `GasAttackMonster` | `AtMonster` | 否 | 是（25 条） |
/// | 91 | `MagCowMonster` | `AtMonster` | 否 | 是（18 条） |
/// | 97 | `CowMonster` | `AtMonster` | 否 | 是（2 条） |
/// | 106 | `GasDungMonster` | `GasAttackMonster` | 否 | 是（2 条） |
/// | 118 | `HighRiskSpider` | `SpitSpider` | 否 | **否**（部署无 MonGen/库行；仍补，数据不可达 ≠ 不补） |
/// | 119 | `BigPoisionSpider` | `SpitSpider` | 否 | 是（31 条） |
/// | 130 | `DoubleCriticalMonster` | `AtMonster` | 否 | **否**（部署无 MonGen/库行） |
///
/// 这些类的 `AttackTarget` 覆写尚未建模（另有卡），但**索敌节拍本身**与基类一致，必须进本集合。
///
/// **不在此列（登记）**：覆写 `Run` 的 `ATMonster` 派生（`92`/`96`/`100`/`105`/`107`/`114`/
/// `132` 等，见模块头「未建模」；**MB-3b F2**：105、**MB-4 F1**：92、**MB-8（R42）**：100 的
/// `base.Run()` 段已由 [`monster_run`] 单独补门 `|| race == 105` / `|| race == 92` /
/// `|| race == 100`，本集合仍只表示「未覆写 `Run` 的直派生」），以及非 `ATMonster`
/// 家族但自实现同样索敌门的类（`BoneKing`/`Clone`/`DigOutZombi`/`Sculture`/`LightingZombi`/…）。
///
/// 改坏必红：去掉本门 ⇒ `active_seek_race_*` 用例里 race 81/83 的怪不再发现玩家。
#[must_use]
pub fn is_active_seek_race(race: u8) -> bool {
    matches!(
        race,
        53 | 81 | 82 | 83 | 84 | 86 | 88 | 89 | 90 | 91 | 97 | 106 | 118 | 119 | 130
    )
}

/// `AnimalObject.Struck` 的 `if (Animal)` 门（`AnimalObject.cs:350-357`）所读 `BaseObject.Animal`
/// 的**运行时等价物**——`Animal` 是怪种分类的函数（构造期置位，之后不再变）：
/// - `WorldServer.MonGen.cs:518-553` 的对象初始化式：`AnimalChicken(51)`/`AnimalDeer(52)`/`AnimalWolf(53)`；
/// - 派生类 ctor：`SpitSpider(82)`（`SpitSpider.cs:15`）/`Scorpion(84)`（`Scorpion.cs:7`）/
///   `StickMonster(85)`（`StickMonster.cs:23`）/`GasAttackMonster(90)`（`GasAttackMonster.cs:12`）；
/// - **继承**上述 ctor 且不再覆写 `Animal` 的派生：`105`/`106`（`: GasAttackMonster`）、
///   `114`（`: SpitSpider`；`ElfWarriorMonster` ctor 不改 `Animal`）、`132`（`: StickMonster`）。
///   **显式 `Animal = false` 的派生一律排除**：`107`（`CentipedeKingMonster.cs:17`）、
///   `118`（`HighRiskSpider.cs:7`）、`119`（`BigPoisionSpider.cs:7`）。
///
/// `BaseObject.ReAliveEx` 的 `case 54`（`BaseObject.cs:2601-2603`）只在**复活**路径置位；本骨架未建模
/// 复活 ⇒ `54` **不计入**（与 [`crate::combat::race_is_animal`] 口径一致）。这里在
/// [`crate::combat::race_is_animal`]（服务 `SKILL_TAMMING` 门）之上补 ctor 继承带来的 race。
///
/// 改坏必红：去门 ⇒ `animal_struck_no_random300_for_non_animal_race` 的对照与
/// `animal_struck_no_random300_for_race_51_vs_80` 必红。
#[must_use]
pub fn is_animal_race(race: u8) -> bool {
    // 118/119 的 ctor 显式 `Animal = false`（`HighRiskSpider.cs:7`、`BigPoisionSpider.cs:7`），
    // 不得继承 `SpitSpider` 的 `Animal = true`。
    crate::combat::race_is_animal(race) || matches!(race, 105 | 106 | 114 | 132)
}

/// 🛡️ `GuardList.txt` 放置的守卫 race 集（`CreateMonster` 的守卫族分支，`WorldServer.MonGen.cs:509-516/659-661`）：
/// `Supreguard(11)`→`SuperGuard`、`Petsupreguard(12)`→`PetSuperGuard`、`ArcherPolice(20)`、`ArcherGuard(112)`。
///
/// 这些类经 `GuardUnit : NormNpc : AnimalObject`（`GuardUnit.cs:14`、`NormNpc.cs:18`），**不继承
/// `MonsterObject.Run`** ⇒ 不走 `Think`/`AttackTarget`/`GotoTargetXY`/`Wondering`（= [`monster_run`]）。
/// `NormNpc.Run`（`NormNpc.cs:1604-1611`）只做「清 master + `base.Run()`」⇒ 守卫**静止**（不游走）；
/// 仍需走 [`base_object_run_monster`]（= `BaseObject.Run` 的恢复/出队/清理段）。
///
/// **登记（本片不建模）**：`SuperGuard.Run`（`SuperGuard.cs:68-115`）/`ArcherGuard.Run`
/// （`ArcherGuard.cs:39-...`）各自另有一套「扫 `VisibleActors` 找 pvp 玩家/怪并攻击」的主动逻辑；
/// 本片只做「不游走/不索敌」的静止等价（安静场景下与 oracle 一致）。
///
/// 改坏必红：去掉本门 ⇒ `guard_race_stays_static_while_normal_monster_wanders` 必红。
#[must_use]
pub fn is_guard_race(race: u8) -> bool {
    matches!(race, 11 | 12 | 20 | 112)
}

/// `WorldServer.MonGen.cs:300` 的**非玩家自扫条件**：
/// `monster.IsSlave || (monster.Race is ActorRace.Guard or ActorRace.ArcherGuard or ActorRace.SlaveMonster)`
/// ⇒ 只有宝宝与 11/112/254 三类怪在自己的 `SearchTime` 门后调 `SearchViewRange()`；
/// **其余怪物不主动全扫**，被动视野由玩家的 `RM_UPDATEVIEWRANGE` 投递
/// （`PlayObject.cs:3130-3139` → `MonsterObject.cs:136-185`）。
#[must_use]
pub fn monster_self_scans(e: &Entity) -> bool {
    e.is_slave || matches!(e.race, 11 | 112 | 254)
}

/// `GuardUnit` 家族：`CreateMonster`（`WorldServer.MonGen.cs:510-518/660`）里映射到
/// `SuperGuard(11)` / `PetSuperGuard(12)` / `ArcherPolice(20)` / `CastleDoor(110)` /
/// `WallStructure(111)` / `ArcherGuard(112)` 的 race。
///
/// 它们覆写 `SearchViewRange`（`GuardUnit.cs:127`）且**不继承** `MonsterObject.Operate`
/// ⇒ 收到 `RM_UPDATEVIEWRANGE` 也不处理（`AnimalObject.Operate` → `BaseObject.Operate` 返回
/// false，消息被丢弃）。
#[must_use]
pub fn is_guard_unit_race(race: u8) -> bool {
    matches!(race, 11 | 12 | 20 | 110 | 111 | 112)
}

/// 该实体是否**消费** `RM_UPDATEVIEWRANGE`（= `MonsterObject.Operate` 的
/// `RM_UPDATEVIEWRANGE` 支存在，`MonsterObject.cs:177-185`）。
///
/// 排除：`GuardUnit` 家族（无该支）、`CloneMonster`（`Clone.cs:14-30` 覆写 `Operate` 且
/// 恒 `return false`，不调 base）、固定 NPC/商人（`entity_cell_type` 本就不会登记成
/// `CellType::Monster`，此处双保险）。
#[must_use]
pub fn consumes_view_range_update(e: &Entity) -> bool {
    e.merchant.is_none()
        && e.npc.is_none()
        && !is_guard_unit_race(e.race)
        && e.race != crate::special_monster::RACE_CLONE_MONSTER
}

/// `MonsterObject.Run` 之后 `base.Run()` 的落点（见 [`monster_class_run`]）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClassRunBase {
    /// `StickMonster`/`BigHeartMonster : AnimalObject` ⇒ `base.Run()` = `AnimalObject.Run`
    /// （HolySeize 到期）→ `BaseObject.Run`。
    Animal,
    /// **MB-8（R33）**：`SandMobObject.Run` 在 `AttackTarget()` 命中支里**先跑了一次**
    /// `base.Run()`（= `StickMonster.Run`；此时 `WalkTick` 刚被本函数刷新 ⇒ `StickMonster.Run`
    /// 的 `WalkTick` 门必然为假、只剩其尾 `base.Run()` = `AnimalObject.Run`），函数尾**再跑一次**
    /// `base.Run()`（同样早退成 `AnimalObject.Run`）⇒ 调用方要跑 **两次**
    /// `AnimalObject.Run` + `BaseObject.Run`（oracle 原样的双 pass）。
    AnimalTwice,
    /// **MB-9（R19）**：`FireballMonster : MagicMonster : MonsterObject`。
    /// 调用方顺序：类体（`fireball_monster_run`）→ [`magic_monster_run`]（`MagicMonster.Run`
    /// 自身的一段 AI）→ `MonsterObject.Run` → `AnimalObject.Run` → `BaseObject.Run`。
    /// **两段 AI pass**：`MagicMonster.Run` 尾的 `base.Run()` 是 `MonsterObject.Run`。
    MagicMonster,
    /// `DigOutZombi`/`ScultureMonster : MonsterObject` ⇒ `base.Run()` = `MonsterObject.Run`
    /// → `AnimalObject.Run` → `BaseObject.Run`。
    Monster,
    /// 类 `Run` 体内已 `return`（oracle 显式跳过 `base.Run()`）⇒ 调用方**不得**续跑
    /// `AnimalObject.Run`/`BaseObject.Run`。目前五处：
    /// ① `CowKingMonster.Run` 的 Siege 两支（`CowKingMonster.cs:53/56`）；
    /// ② `Khazard.Run` 的毒门命中支（`Khazard.cs:32/46`；见 [`khazard_run`] 返回 `true`）；
    /// ③ `ElectronicScolpionMon.Run` 的 `TargetCret == null` 支（`ElectronicScolpionMon.cs:50-53`）；
    /// ④ `MagicMonObject.Run` 的 `Master == null` 支（`MagicMonObject.cs:40-43`）；
    /// ⑤ `SandMobObject.Run` 的目标两轴均 `> 15` 支（`SandMobObject.cs:33-37`：`ComeDown(); return;`）。
    None,
}

/// `SendRefMsg(...)`（`BaseObject.Message.cs:383-445`）在怪物 AI 里产生的帧事件。
///
/// 只登记**需要出帧**的调用；收件人集合在**发帧时刻**快照（±12、仅 `Race==Play`，见
/// [`MonsterRefMsg::recipients`]），宿主排水时直接用快照、不再重算。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MonsterRefMsg {
    pub actor_id: i32,
    pub ident: u16,
    pub w_param: i32,
    pub n_param1: i32,
    pub n_param2: i32,
    pub n_param3: i32,
    /// `RM_FLYAXE`/`RM_LIGHTING` 的**目标坐标快照**（发帧时刻 `target.CurrX/CurrY`）。
    ///
    /// oracle 在收件人处理消息时读 `ActorMgr.Get(nParam3).CurrX/CurrY`；本仓在 `world.tick()`
    /// 之后排水，目标可能已移动 ⇒ 与收件人集合同口径，在**发帧时刻**快照。其它 ident 恒 0。
    pub target_x: i32,
    pub target_y: i32,
    pub dir: u8,
    pub x: i32,
    pub y: i32,
    pub map_id: usize,
    /// 发帧时刻的 `BaseObject.CharStatus`（`RM_DIGUP` 尾的 `Status` 字段读它）。
    pub status: i32,
    /// `ObMode || FixedHideMode`：oracle `SendRefMsg` 只 `SendMsg` 给自己（怪物无客户端 ⇒ 不产帧）。
    pub self_only: bool,
    /// **发帧时刻**的收件人快照（玩家 actor_id）。
    ///
    /// oracle `SendRefMsg` 在**调用时刻**就把消息投进各玩家的队列（`BaseObject.Message.cs:383-444`
    /// 的 ±12 格扫）；本仓宿主在 `world.tick()` 之后才排水，若那时再按实体状态重算，会漏掉
    /// 「发帧后立刻改状态」的帧（如 `StickMonster.ComeDown` 先发 `RM_DIGDOWN` 再置
    /// `FixedHideMode=true` ⇒ 排水时 sender 已隐藏、重算收件人为空 ⇒ 整帧丢失）。
    /// 因此在入队时就快照收件人（`SEND_REF_MSG_RANGE` 格扫、排除自己、`Race==Play && !Ghost`）。
    pub recipients: Vec<i32>,
}

/// 只读快照 `(dir, x, y, map_id, char_status, hidden)`。
fn ref_snapshot(world: &World, actor_id: i32) -> Option<(u8, i32, i32, usize, i32, bool)> {
    world.entities.get(actor_id).map(|e| {
        (
            e.ai.dir,
            e.x,
            e.y,
            e.map_id,
            e.char_status,
            e.ob_mode || e.fixed_hide_mode,
        )
    })
}

/// `send_ref_msg_targets_at` 的 `sender_id` 只用于排除「已 `Ghost` 的离开者」（`RM_DISAPPEAR`）。
/// 怪物 AI 的 `SendRefMsg` 无该语义，且 oracle 格扫**包含 sender 自己**（`Race == Play` 时）
/// ⇒ 传一个不可能命中的 id，sender 不被排除，格扫顺序仍按 oracle（X 外层、Y 内层）。
const NO_SENDER_EXCLUSION: i32 = i32::MIN;

/// `BaseObject.SendRefMsg(wIdent, wParam, nParam1, nParam2, nParam3, sMsg)`（`BaseObject.Message.cs:383`）。
fn send_ref_msg(
    world: &mut World,
    actor_id: i32,
    ident: u16,
    w_param: i32,
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
) {
    send_ref_msg_with_target(
        world, actor_id, ident, w_param, n_param1, n_param2, n_param3, None,
    );
}

/// [`send_ref_msg`] 的带目标版本：额外快照 `target.CurrX/CurrY`（`RM_FLYAXE`/`RM_LIGHTING` 的尾）。
#[allow(clippy::too_many_arguments)]
fn send_ref_msg_with_target(
    world: &mut World,
    actor_id: i32,
    ident: u16,
    w_param: i32,
    n_param1: i32,
    n_param2: i32,
    n_param3: i32,
    target: Option<i32>,
) {
    let Some((dir, x, y, map_id, status, self_only)) = ref_snapshot(world, actor_id) else {
        return;
    };
    let (target_x, target_y) = target
        .and_then(|id| world.entities.get(id))
        .map_or((0, 0), |t| (t.x, t.y));
    // 收件人**在发帧时刻**快照（oracle `SendRefMsg` 同点）；`self_only` 恒无收件人。
    //
    // **格扫包含发送者自己**：oracle `BaseObject.Message.cs` 的格扫对命中的
    // `Race == Play && !Ghost` 对象一律 `SendMsg(this, …)`，发送者自己的格子也在扫掠范围内
    // ⇒ 玩家作 sender 时（`Khazard` 的 `TargetCret.SendRefMsg(RM_SPACEMOVE_FIRE)`）收件人含本人。
    // `send_ref_msg_targets_at` 因 `RM_DISAPPEAR`（离开者已 `Ghost`）语义显式排除 `sender_id`；
    // 怪物 AI 的 `SendRefMsg` 没有「离开者」语义，故传一个不可能命中的 id，让格扫按 oracle
    // 原样（含玩家 sender、按 X 外层 → Y 内层的格序）产出收件人。
    let recipients = if self_only {
        Vec::new()
    } else {
        crate::session::send_ref_msg_targets_at(world, NO_SENDER_EXCLUSION, map_id, x, y, 12)
    };
    world.pending_monster_ref_msgs.push(MonsterRefMsg {
        actor_id,
        ident,
        w_param,
        n_param1,
        n_param2,
        n_param3,
        target_x,
        target_y,
        dir,
        x,
        y,
        map_id,
        status,
        self_only,
        recipients,
    });
}

/// `Messages.RM_SPACEMOVE_FIRE`（`src/OpenMir2/Messages.cs:676` = 8062）——瞬移前
/// `SendRefMsg(RM_SPACEMOVE_FIRE, 0, 0, 0, 0, "")`（`TeleMonster.cs:20`、`Khazard.cs:28/42`）。
/// 收件人支 `PlayObject.Message.cs:1957-1966` 发 `SM_SPACEMOVE_HIDE(800)`；本层只产出
/// [`MonsterRefMsg`]，宿主排水面归接线层（`bins/game-svr/src/main.rs` 的 drain 尚未列
/// 该 ident ⇒ 本车道登记为未接线项）。
pub const RM_SPACEMOVE_FIRE: u16 = 8062;

/// `Run` 覆写共用的 8s/1s 索敌门（逐句同一段，`GreenMonster.cs:29-33` 等五类相同）：
/// ```text
/// if ((GetTickCount() - SearchEnemyTick) > 8000
///     || (GetTickCount() - SearchEnemyTick) > 1000 && TargetCret == null)
/// { SearchEnemyTick = GetTickCount(); SearchTarget(); }
/// ```
/// `SearchTarget` 本身不抽数（见 [`search_target`] 文档）⇒ 本门只改目标态。
fn class_seek_gate(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let enemy_tick = e.ai.search_enemy_tick;
    let has_target = e.ai.target_creature.is_some();
    // C# `&&` 优先级高于 `||` ⇒ 第二项是 `(> 1000) && (TargetCret == null)`。
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
}

/// `GreenMonster.Run`（`GreenMonster.cs:13-36`）/ `RedMonster.Run`（`RedMonster.cs:13-36`）。
///
/// 两类逐句相同，唯一差异是 `MakePosion` 的毒型（`DECHEALTH` vs `DAMAGEARMOR`）⇒ 参数化
/// 防止漂移。`CanMove` 门内有目标即写 `TargetX/Y = TargetCret.CurrX/CurrY`；两轴各 1 格时：
/// **先抽** `Random(TargetCret.AntiPoison + 7)`，**再判** `TargetCret.StatusTimeArr[DECHEALTH] == 0`
/// ——oracle 的 `&&` 短路发生在**抽数之后**（`GreenMonster.cs:23`），因此状态已存在时
/// **仍抽数**、只是不施毒。尾段 `class_seek_gate` + `base.Run()`。
///
/// `AntiPoison` 取**目标**的（Rust 唯一注入面 = [`World::target_anti_poison`]）。
fn green_red_monster_run(world: &mut World, actor_id: i32, poison_type: u8) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let (cx, cy, target_id) = (e.x, e.y, e.ai.target_creature);
    if let Some(target_id) = target_id {
        if let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) {
            // oracle 无条件覆盖 `TargetX/TargetY`（不走 `SetTargetCreat`）。
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
            if (tx - cx).abs() == 1 && (ty - cy).abs() == 1 {
                let anti_poison = world.target_anti_poison(target_id);
                // **先抽**（即使 DECHEALTH 已存在，`&&` 的右支不求值但左支已抽）。
                let roll_passed = world.rng.random_below(anti_poison + 7) <= 6;
                let dec_health_free = world
                    .entities
                    .get(target_id)
                    .is_some_and(|t| t.status_time[crate::entity::POISON_DECHEALTH] == 0);
                if roll_passed && dec_health_free {
                    world.make_posion(target_id, poison_type, 30, 1);
                }
            }
        }
    }
    class_seek_gate(world, actor_id);
}

/// `Khazard.Run`（`Khazard.cs:13-59`）。
///
/// 有目标 ⇒ 目标格写 `TargetX/Y`；`|Δx| == 2 && |Δy| == 2` 时按 `Random(2)` 分两支：
/// - `==0`：`GetFrontPosition`（**自己**的朝向前格）→ `TargetCret.SendRefMsg(RM_SPACEMOVE_FIRE)`
///   → `TargetCret.SpaceMove(this.MapName, nX, nY, 0)`；
/// - `!=0`：目标 `HP <= MaxHP / 2` 才 `GetFrontPosition`（否则 `nX/nY` 保持 **0**，oracle 原样），
///   随后**无条件** `SendRefMsg + SpaceMove(…, 0, 0)`。
///
/// 两支之后同序两抽：`Random(1)`（恒 0 但**必抽**）+ `Random(TargetCret.AntiPoison + 7)`，
/// `<= 6` ⇒ `MakePosion(DECHEALTH, 35, 2)` 并 **`return`**（跳过索敌门与 `base.Run()`）。
/// 毒门不成立 ⇒ 落回索敌门，再 `base.Run()`（= `MonsterObject.Run`）。
///
/// 发帧者是 **`TargetCret`**（不是 Khazard）：`send_ref_msg(world, target_id, …)`。
/// 落点走 `space_move` 的移位环（起点可走 ⇒ 0 抽）。
fn khazard_run(world: &mut World, actor_id: i32) -> bool {
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    if !can_move(e) {
        return false;
    }
    let (cx, cy, target_id, map_id, dir) = (e.x, e.y, e.ai.target_creature, e.map_id, e.ai.dir);
    if let Some(target_id) = target_id {
        if let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
            if (tx - cx).abs() == 2 && (ty - cy).abs() == 2 {
                // oracle `short nX = 0; short nY = 0;`（两支持续共用；!=0 支可能不重算）。
                let mut n_x = 0;
                let mut n_y = 0;
                if world.rng.random_below(2) == 0 {
                    if let Some((fx, fy)) = front_of(world, actor_id, dir, map_id) {
                        n_x = fx;
                        n_y = fy;
                    }
                    // `TargetCret.SendRefMsg(...)`——发帧者 = 目标。
                    send_ref_msg(world, target_id, RM_SPACEMOVE_FIRE, 0, 0, 0, 0);
                    let _ =
                        crate::space_move::monster_space_move(world, target_id, map_id, n_x, n_y);
                    if world.rng.random_below(1) == 0
                        && world
                            .rng
                            .random_below(world.target_anti_poison(target_id) + 7)
                            <= 6
                    {
                        world.make_posion(target_id, crate::entity::POISON_DECHEALTH as u8, 35, 2);
                        return true;
                    }
                } else {
                    let half_or_below = world
                        .entities
                        .get(target_id)
                        .is_some_and(|t| i32::from(t.hp) <= i32::from(t.max_hp) / 2);
                    if half_or_below {
                        if let Some((fx, fy)) = front_of(world, actor_id, dir, map_id) {
                            n_x = fx;
                            n_y = fy;
                        }
                    }
                    send_ref_msg(world, target_id, RM_SPACEMOVE_FIRE, 0, 0, 0, 0);
                    let _ =
                        crate::space_move::monster_space_move(world, target_id, map_id, n_x, n_y);
                    if world.rng.random_below(1) == 0
                        && world
                            .rng
                            .random_below(world.target_anti_poison(target_id) + 7)
                            <= 6
                    {
                        world.make_posion(target_id, crate::entity::POISON_DECHEALTH as u8, 35, 2);
                        return true;
                    }
                }
            }
        }
    }
    class_seek_gate(world, actor_id);
    false
}

/// `FrostTiger.Run`（`FrostTiger.cs:14-36`）。
///
/// `CanMove` 门内：无目标且 `StatusTimeArr[STATETRANSPARENT] == 0` ⇒
/// `MagicManager.MagMakePrivateTransparent(this, 180)`（复用 [`World::mag_make_private_transparent`]，
/// 其内部按 `GetMapBaseObjects(9)` 逐目标 `Random(2)`）；helper 收尾 oracle 无条件
/// `StatusChanged()` ⇒ [`RM_CHARSTATUSCHANGED`]（`nParam1 = CharStatus`），本函数按返回的
/// [`crate::slave::TransparentDelivery::status_changed`] 入出帧队列，**不得丢弃**。
/// 有目标 ⇒ 只把 `StatusTimeArr[STATETRANSPARENT]` 置 **0**（oracle **不**在此清
/// `HideMode/Transparent`，那两个位由 `RecalcAbilitys` 收口）。尾段 `class_seek_gate` + `base.Run()`。
fn frost_tiger_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let has_target = e.ai.target_creature.is_some();
    let transparent_status = e.status_time[crate::entity::POISON_STATETRANSPARENT];
    if !has_target {
        if transparent_status == 0 {
            let d = world.mag_make_private_transparent(actor_id, 180);
            // oracle `MagicManager.MagMakePrivateTransparent` 收尾无条件 `baseObject.StatusChanged()`
            // ⇒ `SendRefMsg(RM_CHARSTATUSCHANGED, 0, CharStatus, 0, 0, "")`。helper 内早退
            // （计时器 > 0 / 实体消失）时 `status_changed == false` ⇒ 不产帧。
            // **投递结果不得丢弃**：为真 ⇒ 入 `pending_monster_ref_msgs`（与其它 `SendRefMsg`
            // 消费点同口径）。宿主 `drain_monster_ref_msgs` 尚未列该 ident ⇒ 出帧面登记 T-135。
            if d.status_changed {
                send_ref_msg(world, actor_id, RM_CHARSTATUSCHANGED, 0, d.status, 0, 0);
            }
        }
    } else if let Some(e) = world.entities.get_mut(actor_id) {
        e.status_time[crate::entity::POISON_STATETRANSPARENT] = 0;
    }
    class_seek_gate(world, actor_id);
}

/// `TeleMonster.Run`（`TeleMonster.cs:12-31`）。
///
/// `CanMove` 门内有目标、且 `|CurrX - TargetX| > 5 || |CurrY - TargetY| > 5`
/// （**比较的是 `TargetX/TargetY` 字段，不是 `TargetCret.CurrX/CurrY`**——目标刚被
/// `SetTargetCreat` 选中、`TargetX/Y` 还是 -1 时也会触发，oracle 原样）⇒
/// `this.SendRefMsg(RM_SPACEMOVE_FIRE)` + `SpaceMove(TargetCret.MapName, TargetCret.CurrX,
/// TargetCret.CurrY, 0)`。尾段 `class_seek_gate` + `base.Run()`。
fn tele_monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let (cx, cy, target_id, target_x, target_y) =
        (e.x, e.y, e.ai.target_creature, e.ai.target_x, e.ai.target_y);
    if let Some(target_id) = target_id {
        if (cx - target_x).abs() > 5 || (cy - target_y).abs() > 5 {
            send_ref_msg(world, actor_id, RM_SPACEMOVE_FIRE, 0, 0, 0, 0);
            let dest = world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y));
            if let Some((dest_map, dx, dy)) = dest {
                let _ = crate::space_move::monster_space_move(world, actor_id, dest_map, dx, dy);
            }
        }
    }
    class_seek_gate(world, actor_id);
}

// ---------------------------------------------------------------------------
// **MB-6**（R14/R25/R31；R17 已由 `attacktarget-overrides` 落地）——魔法 / 群攻
//
// oracle 逐句锚点（本片在 `b46805058` 上实读）：
//
// | 本片 | oracle |
// | --- | --- |
// | [`electronic_scolpion_run`] | `ElectronicScolpionMon.cs:33-70` |
// | [`electronic_scolpion_lighting_attack`] | `ElectronicScolpionMon.cs:15-31` |
// | [`magic_mon_object_run`] | `MagicMonObject.cs:23-60`（`LightingAttack` 是空函数 `:18-21`） |
// | [`ron_object_run`] | `RonObject.cs:35-51` |
// | [`ron_object_around_attack`] | `RonObject.cs:13-33` |
// | [`ron_attack`] | `BaseObject._Attack`（`BaseObject.Attack.cs:30-66`） |
// | [`map_base_objects_1`] | `BaseObject.GetMapBaseObjects`（`BaseObject.cs:2808-2837`） |
//
// 三类的 `base.Run()` 都是 `MonsterObject.Run`（`: MonsterObject`）⇒ [`ClassRunBase::Monster`]；
// 但 R14/R25 的 oracle 在 `CanMove()` 门内 `return` 时**跳过** `base.Run()` ⇒ 调用方必须拿
// [`ClassRunBase::None`]（与 CowKing/Khazard 同口径）。
//
// **建模差异（登记，不忽略）**：`GetMapBaseObjects` 的 `CellObjId > 0 && ActorObject`
// 在 Rust 由 `CellObject` 的 `cell_obj_id/actor_object` 两字段承载（逐字段相同）；`Envir.GetCellInfo`
// 的「越界/非法 ⇒ cellSuccess=false」由 `MapGrid::get_cell_info` 的 `cell_match + valid()` 承担。
// ---------------------------------------------------------------------------

/// `BaseObject.GetMapBaseObjects(envir, nX, nY, 1, ref objectList)`（`BaseObject.cs:2808-2837`）。
///
/// 3×3 窗口按 **x 外层升序、y 内层升序**扫描；每格按 `ObjList` 顺序追加
/// `CellObjId > 0 && ActorObject && !Death && !Ghost` 的对象。**自身也在列表里**
/// （同格对象，未死时），oracle 的 `_Attack(self)` 由 `IsProperTarget` 的
/// `ActorId` 相等门归零 —— 真机 `RonObject.AroundAttack` 因此会给自己投递一条
/// `RM_STRUCK(wParam=0)`，照抄。
#[must_use]
fn map_base_objects_1(world: &World, map_id: usize, nx: i32, ny: i32) -> Vec<i32> {
    let Some(map) = world.maps.get(map_id) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for x in (nx - 1)..=(nx + 1) {
        for y in (ny - 1)..=(ny + 1) {
            let (cell, success) = map.get_cell_info(x, y);
            if !success || !cell.is_available() {
                continue;
            }
            for o in &cell.obj_list {
                if o.cell_obj_id > 0 && o.actor_object {
                    if let Some(e) = world.entities.get(o.cell_obj_id) {
                        if !e.death && !e.ghost {
                            out.push(o.cell_obj_id);
                        }
                    }
                }
            }
        }
    }
    out
}

/// `ElectronicScolpionMon.Run`（`ElectronicScolpionMon.cs:33-70`）。
///
/// 返回 `true` = oracle 在 `CanMove()` 门内 `return`（`TargetCret == null`，`:50-53`）
/// ⇒ 调用方必须用 [`ClassRunBase::None`] 跳过 `base.Run()`。
///
/// 逐句：`UseMagic = HP < MaxHP / 2`（**严格 `<`**，`ushort` 整数除法）；`(now - SearchEnemyTick) > 1000
/// && TargetCret == null` ⇒ 刷新 tick + `SearchTarget()`；仍无目标 ⇒ `return`；目标两轴 `<= 2` 且
/// (`UseMagic || nX == 2 || nY == 2`) 且 `(now - AttackTick) > NextHitTime` ⇒ 刷新 `AttackTick` →
/// `GetNextDirection` → [`electronic_scolpion_lighting_attack`]。
fn electronic_scolpion_run(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    if !can_move(e) {
        return false;
    }
    // `WAbil.HP < WAbil.MaxHP / 2`（`:37`）——两侧都是 `ushort`，`/2` 为整数除法。
    let use_magic = e.hp < e.max_hp / 2;
    let (cx, cy) = (e.x, e.y);
    let (has_target, enemy_tick) = (e.ai.target_creature.is_some(), e.ai.search_enemy_tick);
    let (attack_tick, next_hit_time) = (e.ai.attack_tick, e.ai.next_hit_time);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.use_magic = use_magic;
    }
    // `if ((now - SearchEnemyTick) > 1000 && TargetCret == null)`（`:45`）。
    if now - enemy_tick > 1000 && !has_target {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
    // `if (TargetCret == null) return;`（`:50-53`）——**跳过 `base.Run()`**。
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return true;
    };
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return false;
    };
    let nx = (cx - tx).abs();
    let ny = (cy - ty).abs();
    if nx <= 2 && ny <= 2 && (use_magic || nx == 2 || ny == 2) && now - attack_tick > next_hit_time
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.attack_tick = now;
        }
        let dir = crate::magic::next_direction(cx, cy, tx, ty);
        electronic_scolpion_lighting_attack(world, actor_id, dir);
    }
    false
}

/// `ElectronicScolpionMon.LightingAttack(byte nDir)`（`ElectronicScolpionMon.cs:15-31`）。
///
/// 取数次序（勿改）：`GetAttackPower(Lo(DC), |Hi(DC)-Lo(DC)|)`（`:18`，**1 抽**）→
/// `TargetCret.GetMagStruckDamage(this, nPower)`（`BaseObject.Base.cs:812-821`，MAC **1 抽**；
/// 不死系再加**接收者**的 `UndeadPower`）→ `nDamage > 0` 才：吸血
/// `WAbil.HP += (ushort)(nDamage / LoByte(WAbil.MP))`（`:22-26`，`ushort` 回绕）→
/// `TargetCret.StruckDamage(nDamage)` → `SendStruckDelayMsg(RM_REFMESSAGE, nDamage, …, 200)`。
/// 尾帧 `SendRefMsg(RM_LIGHTING, 1, CurrX, CurrY, TargetCret.ActorId, "")`（`:30`）**无条件**。
fn electronic_scolpion_lighting_attack(world: &mut World, actor_id: i32, n_dir: u8) {
    let Some((cx, cy, target_id)) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature.map(|t| (e.x, e.y, t)))
    else {
        return;
    };
    let dc = world.entities.get(actor_id).map_or(0, |e| e.dc);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = n_dir;
    }
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    // `GetAttackPower(HUtil32.LoByte(WAbil.DC), Math.Abs(HUtil32.HiByte(WAbil.DC) - LoByte))`
    // （`:18`）：`power < 0` 归零后 `Random(power + 1)` ⇒ **恰 1 抽**。
    let n_power = crate::combat::get_attack_power(&mut world.rng, lo, (hi - lo).abs());
    // `GetMagStruckDamage` 的 undead 加项读**接收者的** `AddAbil.UndeadPower`，门 `LA_UNDEAD`。
    let (mac, undead): (u16, Option<i32>) = world.entities.get(target_id).map_or((0, None), |t| {
        (
            t.mac,
            if t.life_attrib == 1 {
                Some(t.undead_power)
            } else {
                None
            },
        )
    });
    let n_damage = crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_power, undead);
    if n_damage > 0 {
        let (mp, hp) = world
            .entities
            .get(actor_id)
            .map_or((0u16, 0u16), |e| (e.mp, e.hp));
        // `int btGetBackHp = HUtil32.LoByte(WAbil.MP); if (btGetBackHp != 0) WAbil.HP += (ushort)(...)`。
        let back = i32::from(crate::combat::lo_byte(i32::from(mp)));
        if back != 0 {
            let heal = (n_damage / back) as u16;
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.hp = hp.wrapping_add(heal);
            }
        }
        // `TargetCret.StruckDamage(nDamage)`：倍率 + `DamageHealth` + 玩家耐久（**不发 RM_STRUCK**）。
        let _ = class_struck_apply(world, actor_id, target_id, n_damage, n_power);
        // `TargetCret.SendStruckDelayMsg(RM_REFMESSAGE, nDamage, …, ActorId, "", 200)`（`:28`）。
        world.send_struck_delay_msg_refmessage(
            target_id,
            actor_id,
            n_damage,
            ATTACK_STRUCK_DELAY_MS,
        );
    }
    // `SendRefMsg(RM_LIGHTING, 1, CurrX, CurrY, TargetCret.ActorId, "")`（`:30`）——**无条件**。
    send_ref_msg_with_target(
        world,
        actor_id,
        RM_LIGHTING,
        1,
        cx,
        cy,
        target_id,
        Some(target_id),
    );
}

/// `MagicMonObject.Run`（`MagicMonObject.cs:23-60`）。
///
/// 返回 `true` = oracle 在 `CanMove()` 门内 `return`（`Master == null`，`:40-43`）⇒
/// [`ClassRunBase::None`]。
///
/// 与 R14 的差异逐句：目标判据是 **`Master`**（不是 `TargetCret`）；`(now - SearchEnemyTick) > 1000
/// && TargetCret == null` 仍按 `TargetCret` 索敌（照抄）；距离窗 `<= 5`、或门 `nX == 5 || nY == 5`；
/// 到点分支里的 `LightingAttack(nAttackDir)` 是**空函数**（`:18-21`）⇒ 只刷新 `AttackTick`，
/// 不抽数、不产帧、不写其它字段。
fn magic_mon_object_run(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    if !can_move(e) {
        return false;
    }
    let use_magic = e.hp < e.max_hp / 2;
    let (cx, cy) = (e.x, e.y);
    let (has_target, enemy_tick) = (e.ai.target_creature.is_some(), e.ai.search_enemy_tick);
    let (attack_tick, next_hit_time) = (e.ai.attack_tick, e.ai.next_hit_time);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.use_magic = use_magic;
    }
    if now - enemy_tick > 1000 && !has_target {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
    // `if (Master == null) return;`（`:40-43`）——**跳过 `base.Run()`**。
    let Some(master_id) = world.entities.get(actor_id).and_then(|e| e.master) else {
        return true;
    };
    let Some((mx, my)) = world.entities.get(master_id).map(|m| (m.x, m.y)) else {
        return true;
    };
    let nx = (cx - mx).abs();
    let ny = (cy - my).abs();
    if nx <= 5 && ny <= 5 && (use_magic || nx == 5 || ny == 5) && now - attack_tick > next_hit_time
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.attack_tick = now;
        }
        // `int nAttackDir = GetNextDirection(...); LightingAttack(nAttackDir);`（`:53-54`）——
        // `LightingAttack` 是**空函数**（`:18-21`）：方向计算无副作用、不抽数 ⇒ 不落任何状态。
    }
    false
}

/// `RonObject.Run`（`RonObject.cs:35-51`）。`base.Run()` = `MonsterObject.Run` ⇒ 恒
/// [`ClassRunBase::Monster`]。
///
/// 索敌门与 [`class_seek_gate`] 逐字相同（`> 8000 || (> 1000 && TargetCret == null)`）；
/// 有目标且两轴 **`< 6`**（严格）且 `(now - AttackTick) > NextHitTime` ⇒ 刷新 `AttackTick` →
/// [`ron_object_around_attack`]。
fn ron_object_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    class_seek_gate(world, actor_id);
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let Some(target_id) = e.ai.target_creature else {
        return;
    };
    let (cx, cy) = (e.x, e.y);
    let (attack_tick, next_hit_time) = (e.ai.attack_tick, e.ai.next_hit_time);
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    if (cx - tx).abs() < 6 && (cy - ty).abs() < 6 && now - attack_tick > next_hit_time {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.attack_tick = now;
        }
        ron_object_around_attack(world, actor_id);
    }
}

/// `RonObject.AroundAttack()`（`RonObject.cs:13-33`）——群攻。
///
/// 顺序（勿改）：`GetAttackDir(TargetCret, ref attDir)`（不邻接 ⇒ `attDir` 保持 **0**）→ `Dir = attDir`
/// → `GetMapBaseObjects(Envir, CurrX, CurrY, 1)`（3×3，含自身）→ **列表非空**才抽一次
/// `GetBaseAttackPoewr()` → 从尾向头逐个 [`ron_attack`]（= oracle 的倒序 `for` + `RemoveAt`）→
/// 尾帧 `SendRefMsg(RM_HIT, Dir, CurrX, CurrY, 0, "")`（**无条件**，列表空也发）。
fn ron_object_around_attack(world: &mut World, actor_id: i32) {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return;
    };
    let att_dir = get_attack_dir(world, actor_id, target_id).unwrap_or(0);
    let Some((cx, cy, map_id)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.map_id)) else {
        return;
    };
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = att_dir;
    }
    let list = map_base_objects_1(world, map_id, cx, cy);
    if !list.is_empty() {
        let dc = world.entities.get(actor_id).map_or(0, |e| e.dc);
        // `int nPower = GetBaseAttackPoewr();`（`BaseObject.Attack.cs:25-28`）——**一次**，全目标共用。
        let n_power = crate::combat::get_base_attack_power(&mut world.rng, dc);
        // `for (int i = objectList.Count - 1; i >= 0; i--) { _Attack(nPower, objectList[i]); RemoveAt(i); }`
        // ⇒ 倒序访问，每项恰一次。
        for tid in list.into_iter().rev() {
            ron_attack(world, actor_id, tid, n_power);
        }
    }
    let (dir, x, y) = world
        .entities
        .get(actor_id)
        .map_or((0u8, 0, 0), |e| (e.ai.dir, e.x, e.y));
    send_ref_msg(world, actor_id, RM_HIT, i32::from(dir), x, y, 0);
}

/// `BaseObject._Attack(int nPower, IActor targetObject)`（`BaseObject.Attack.cs:30-66`），
/// 攻方攻击力**由调用方预先抽好**（`RonObject` 一次 `GetBaseAttackPoewr` 打全列表）。
///
/// 取数与副作用次序（勿改）：
/// 1. `IsProperTarget(target)` 为假 ⇒ `nPower = 0`（不抽）；
/// 2. `target.HitPoint > 0` ⇒ `HitPoint < RandomByte(target.SpeedPoint)` 时 `nPower = 0`（**1 抽**）；
/// 3. `nPower > 0` ⇒ `GetHitStruckDamage`（AC 1~2 抽）→ `> 0` 时 `StruckDamage`（倍率 + 掉血 +
///    玩家目标耐久抽）→ `SendStruckDelayMsg(RM_REFMESSAGE, nPower, …, 200)`；
/// 4. `target.Race > Play` ⇒ `target.SendMsg(target, RM_STRUCK, nPower, HP, MaxHP, ActorId)`
///    （本骨架折叠为 `SetLastHiter` + `pending_struck` 入队，与 [`World::attack_once`] 同口径；
///    `CloneMonster` 的 `Operate` 覆写除外）。
fn ron_attack(world: &mut World, actor_id: i32, target_id: i32, n_power: i32) {
    let Some(attacker) = world.entities.get(actor_id).cloned() else {
        return;
    };
    let Some(mut target) = world.entities.get(target_id).cloned() else {
        return;
    };
    let power = n_power;
    let mut n_power = n_power;
    let mut hit = false;
    if is_proper_target_grouped_direct(&attacker, &target, &[]) {
        // `if (targetObject.HitPoint > 0) { if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0; }`
        if target.hit_point > 0 && attacker.hit_point < world.rng.random_byte(target.speed_point) {
            n_power = 0;
        } else {
            hit = true;
        }
    } else {
        n_power = 0;
    }
    let mut damage = 0u16;
    let mut struck_power = 0u16;
    let mut killed = false;
    if n_power > 0 {
        let reduced =
            crate::combat::get_hit_struck_damage(&mut world.rng, &target, &attacker, n_power);
        if reduced > 0 {
            let last_hitter = current_last_hitter(world, target_id);
            let cfg = world.combat_config;
            let (d, k) = crate::combat::struck_damage_apply(
                &mut world.rng,
                &mut target,
                i32::from(reduced),
                last_hitter,
                &cfg,
            );
            damage = d;
            killed = k;
            struck_power = reduced;
            // 目标 `StruckDamage` 在克隆上跑（玩家目标含耐久段）⇒ 回写 `hp`/`use_items`。
            if let Some(e) = world.entities.get_mut(target_id) {
                e.hp = target.hp;
                e.use_items = target.use_items;
            }
            // `targetObject.SendStruckDelayMsg(RM_REFMESSAGE, nPower, …, 200)`（`:57`）。
            world.send_struck_delay_msg_refmessage(
                target_id,
                actor_id,
                i32::from(reduced),
                ATTACK_STRUCK_DELAY_MS,
            );
        }
    }
    world.pending_ai_attacks.push((
        actor_id,
        target_id,
        crate::combat::AttackOutcome {
            power,
            hit,
            damage,
            struck_power,
            killed,
            target_hp_after: target.hp,
        },
    ));
    // `if (targetObject.Race > ActorRace.Play) targetObject.SendMsg(targetObject, RM_STRUCK, nPower, …)`
    // （`:61-64`）：目标自己的 `base.Run()` 出队（`AnimalObject.Operate`）。折叠口径同
    // [`World::attack_once`]：`CloneMonster` 不写 `LastHiter`（其 `Operate` 覆写恒 `return false`）。
    if target.race > ACTOR_RACE_PLAY {
        let now = world.clock_ms;
        let is_clone = target.race == crate::special_monster::RACE_CLONE_MONSTER;
        if let Some(e) = world.entities.get_mut(target_id) {
            if !is_clone {
                e.set_last_hiter(actor_id, now);
            }
            if !e.ghost {
                e.ai.pending_struck.push(crate::entity::StruckMsg {
                    hiter: actor_id,
                    w_param: i32::from(struck_power),
                });
            }
        }
    }
}

/// 专用怪 `Run` 覆写的分派（`CreateMonster` 的 race → 类映射）。
///
/// 已接：MB-1 的 `StickMonster`(85)/`DigOutZombi`(95)/`ScultureMonster`(101)/
/// `BigHeartMonster`(115)；MB-2 的 `ChickenDeer`(52)/`DualAxeMonster`(87，93/104 继承)/
/// `LightingZombi`(94)/`ZilKinZombi`(96)；MB-3 的 `CentipedeKing`(107)/`GasMoth`(105)/
/// `ScultureKing`(102)/`ExplosionSpider`(117)；MB-4 的 `CowKingMonster`(92)；
/// MB-5 的 `TeleMonster`(203)/`Khazard`(206)/`GreenMonster`(208)/`RedMonster`(209)/`FrostTiger`(210)；
/// MB-6 的 `ElectronicScolpionMon`(200)/`MagicMonObject`(133)/`RonObject`(131)；
/// MB-7 的 `BoneKingMonster`(134)/`ElfMonster`(113)/`ElfWarriorMonster`(114)；
/// MB-8 的 `WhiteSkeleton`(100)/`SoccerBall`(120)/`SandMobObject`(132)/`FireMonster`(214)；
/// MB-9 的 `FireballMonster`(215)。（`RunAway` 是死类：`CreateMonster` 无 case，见
/// [`run_away_run`]。）
///
/// 返回 `None` = 非上述类，调用方继续走通用 [`monster_run`]；否则按 [`ClassRunBase`]
/// 决定 oracle `base.Run()` 的落点。
pub fn monster_class_run(world: &mut World, actor_id: i32) -> Option<ClassRunBase> {
    let race = world.entities.get(actor_id)?.race;
    match race {
        // `StickMonster.Run`（`StickMonster.cs:101-149`）。
        85 => {
            stick_monster_run(world, actor_id);
            Some(ClassRunBase::Animal)
        }
        // `DigOutZombi.Run`（`DigOutZombi.cs:18-55`）。
        95 => {
            dig_out_zombi_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `ScultureMonster.Run`（`ScultureMonster.cs:43-79`）。
        101 => {
            sculture_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `BigHeartMonster.Run`（`BigHeartMonster.cs:44-54`）。
        115 => {
            big_heart_monster_run(world, actor_id);
            Some(ClassRunBase::Animal)
        }
        // **MB-2**：`ChickenDeer`（52）、`DualAxeMonster` 及 93/104 继承、
        // `LightingZombi`（94）、`ZilKinZombi`（96）。
        52 => {
            chicken_deer_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        87 | 93 | 104 => {
            dual_axe_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        94 => {
            lighting_zombi_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        96 => {
            zil_kin_zombi_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-3**：`CentipedeKingMonster`（`StickMonster` 派生；`base.Run` 的
        // `StickMonster.Run` 在 `WalkTick` 已刷新后必然早退 ⇒ 等价于 `Animal` 续跑）。
        107 => {
            centipede_king_run(world, actor_id);
            Some(ClassRunBase::Animal)
        }
        // `GasMothMonster`（`base.Run` = `ATMonster.Run` → `MonsterObject.Run`）。
        105 => {
            gas_moth_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `ScultureKingMonster`（`base.Run` = `MonsterObject.Run`）。
        102 => {
            sculture_king_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **`openmir2-attacktarget-overrides`**：`ExplosionSpider.Run`（60s 自爆段 +
        // `base.Run()` = `MonsterObject.Run`）。
        117 => {
            explosion_spider_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-4**：`CowKingMonster.Run`（`:40-94`）；`base.Run()` =
        // `ATMonster.Run`（race 92 不在索敌集合）→ `MonsterObject.Run`。
        // Siege 两支在 oracle 里 `return`，**跳过 `base.Run()`** ⇒ `ClassRunBase::None`。
        92 => {
            if cow_king_run(world, actor_id) {
                Some(ClassRunBase::None)
            } else {
                Some(ClassRunBase::Monster)
            }
        }
        // **MB-5**：毒 / 隐身 / 瞬移五类（全部 `: MonsterObject` ⇒
        // `base.Run()` = `MonsterObject.Run`）。
        // `TeleMonster`（203）：`TargetX/Y` 错位瞬移。
        203 => {
            tele_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `Khazard`（206）：目标 `return`（毒门命中）时 oracle **跳过 `base.Run()`**
        // ⇒ `ClassRunBase::None`（与 CowKing 的 Siege 两支同口径）。
        206 => {
            if khazard_run(world, actor_id) {
                Some(ClassRunBase::None)
            } else {
                Some(ClassRunBase::Monster)
            }
        }
        // `GreenMonster`（208）/ `RedMonster`（209）：只差毒型。
        208 => {
            green_red_monster_run(world, actor_id, crate::entity::POISON_DECHEALTH as u8);
            Some(ClassRunBase::Monster)
        }
        209 => {
            green_red_monster_run(world, actor_id, crate::entity::POISON_DAMAGEARMOR as u8);
            Some(ClassRunBase::Monster)
        }
        // `FrostTiger`（210）：透明 180s / 有目标清计时器。
        210 => {
            frost_tiger_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-6**：`ElectronicScolpionMon`（200）。`CanMove()` 门内 `TargetCret == null`
        // ⇒ oracle `return`（跳过 `base.Run()`）⇒ [`ClassRunBase::None`]。
        200 => {
            if electronic_scolpion_run(world, actor_id) {
                Some(ClassRunBase::None)
            } else {
                Some(ClassRunBase::Monster)
            }
        }
        // `MagicMonObject`（133）：`CanMove()` 门内 `Master == null` ⇒ oracle `return`。
        133 => {
            if magic_mon_object_run(world, actor_id) {
                Some(ClassRunBase::None)
            } else {
                Some(ClassRunBase::Monster)
            }
        }
        // `RonObject`（131）：`Run` 恒落到 `base.Run()`（`MonsterObject.Run`）。
        131 => {
            ron_object_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-7**：`BoneKingMonster.Run`（`BoneKingMonster.cs:48-76`）；`: MonsterObject`
        // ⇒ `base.Run()` = `MonsterObject.Run`。
        134 => {
            bone_king_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `ElfMonster.Run`（`ElfMonster.cs:43-86`）；`: MonsterObject` ⇒ `MonsterObject.Run`。
        113 => {
            elf_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // `ElfWarriorMonster.Run`（`ElfWarriorMonster.cs:112-166`）；`: SpitSpider : AtMonster`
        // ⇒ `base.Run()` = `ATMonster.Run` → `MonsterObject.Run`（索敌门见 [`monster_run`]）。
        114 => {
            elf_warrior_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-8（R18）**：`FireMonster.Run`（`FireMonster.cs:23-82`）；`: MonsterObject`
        // ⇒ `base.Run()` = `MonsterObject.Run`。
        214 => {
            fire_monster_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-8（R33）**：`SandMobObject.Run`（`SandMobObject.cs:14-57`）；`: StickMonster`。
        // 双 `base.Run()` 的 pass 数由本函数返回（`AnimalTwice`/`Animal`/`None`）。
        132 => Some(sand_mob_object_run(world, actor_id)),
        // **MB-8（R36）**：`SoccerBall.Run`（`SoccerBall.cs:19-74`）；`: AnimalObject`
        // ⇒ `base.Run()` = `AnimalObject.Run`。
        120 => {
            soccer_ball_run(world, actor_id);
            Some(ClassRunBase::Animal)
        }
        // **MB-8（R42）**：`WhiteSkeleton.Run`（`WhiteSkeleton.cs:29-39`）；`: AtMonster`
        // ⇒ `base.Run()` = `ATMonster.Run` → `MonsterObject.Run`（8s/1s 门见 [`monster_run`]）。
        100 => {
            white_skeleton_run(world, actor_id);
            Some(ClassRunBase::Monster)
        }
        // **MB-9（R19）**：`FireballMonster.Run`（`FireballMonster.cs:14-62`）；`: MagicMonster`。
        215 => {
            fireball_monster_run(world, actor_id);
            Some(ClassRunBase::MagicMonster)
        }
        _ => None,
    }
}

/// [`monster_class_run`] 的返回值 → oracle `base.Run()` 链的落地（四个调用点共用）。
fn run_class_base(world: &mut World, actor_id: i32, base: ClassRunBase) {
    match base {
        ClassRunBase::Animal => {
            animal_run(world, actor_id);
            base_object_run_monster(world, actor_id);
        }
        ClassRunBase::AnimalTwice => {
            // `SandMobObject.Run` 的两次 `base.Run()`（各早退成 `AnimalObject.Run`）。
            animal_run(world, actor_id);
            base_object_run_monster(world, actor_id);
            animal_run(world, actor_id);
            base_object_run_monster(world, actor_id);
        }
        ClassRunBase::Monster => {
            monster_run(world, actor_id);
            animal_run(world, actor_id);
            base_object_run_monster(world, actor_id);
        }
        ClassRunBase::MagicMonster => {
            magic_monster_run(world, actor_id);
            monster_run(world, actor_id);
            animal_run(world, actor_id);
            base_object_run_monster(world, actor_id);
        }
        ClassRunBase::None => {}
    }
}

/// `StickMonster.CheckComeOut`（`StickMonster.cs:84-104`）：严格 `<` 两轴、`Death` 跳过，
/// `IsProperTarget && (!HideMode || CoolEye)`。
fn stick_check_come_out(world: &World, actor_id: i32) -> bool {
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    let (cx, cy, come_out, cool_eye) = (e.x, e.y, e.come_out_value, e.ai.cool_eye);
    let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if proper
            && (!t.hide_mode || cool_eye)
            && (cx - t.x).abs() < come_out
            && (cy - t.y).abs() < come_out
        {
            return true;
        }
    }
    false
}

/// `StickMonster.FindAttackTarget`（`StickMonster.cs:54-58`）：
/// 先 `FixedHideMode = false`，再 `SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0, "")`。
fn stick_find_attack_target(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.fixed_hide_mode = false;
    }
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    // oracle `SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0, "")`（wParam = Dir）。
    send_ref_msg(world, actor_id, RM_DIGUP, dir as i32, x, y, 0);
}

/// `StickMonster.ComeDown`（`StickMonster.cs:60-68`）：`RM_DIGDOWN` → 清可见集 → `FixedHideMode=true`。
fn stick_come_down(world: &mut World, actor_id: i32) {
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    send_ref_msg(world, actor_id, RM_DIGDOWN, dir as i32, x, y, 0);
    if let Some(e) = world.entities.get_mut(actor_id) {
        // oracle `Dispose(VisibleActors[i])` 是 `obj = null` 的无副作用空操作。
        e.visible_actors.clear();
        e.fixed_hide_mode = true;
    }
}

/// `StickMonster.AttackTarget`（`StickMonster.cs:25-52`）。
///
/// 与 `MonsterObject.AttackTarget` 的差异：**不**调 `BreakHolySeizeMode()`。
fn stick_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
            }
            turn_to(world, actor_id, bt_dir);
            // `AnimalObject.Attack`：`GetBaseAttackPoewr()`（`World::attack_once` 等价）
            // → `AttackDir`（置 `Dir` + `_Attack`）→ `SendAttackMsg(RM_HIT, Dir, CurrX, CurrY)`。
            if let Some(outcome) = world.attack_once(actor_id, target_id) {
                world
                    .pending_ai_attacks
                    .push((actor_id, target_id, outcome));
            }
            let (dir, x, y) = match world.entities.get(actor_id) {
                Some(e) => (e.ai.dir, e.x, e.y),
                None => return true,
            };
            send_ref_msg(world, actor_id, RM_HIT, dir as i32, x, y, 0);
        }
        true
    } else {
        let same_map = world
            .entities
            .get(actor_id)
            .zip(world.entities.get(target_id))
            .is_some_and(|(a, t)| a.map_id == t.map_id);
        if same_map {
            let (tx, ty) = world.entities.get(target_id).map_or((0, 0), |t| (t.x, t.y));
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// `StickMonster.Run`（`StickMonster.cs:106-149`）。`base.Run()` 由调用方跑（= `AnimalObject.Run`）。
fn stick_monster_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && (now - e.ai.walk_tick) > e.ai.walk_speed) {
        return;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
    }
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.fixed_hide_mode)
    {
        if stick_check_come_out(world, actor_id) {
            stick_find_attack_target(world, actor_id);
        }
        return;
    }
    let attack_due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
    if attack_due {
        search_target(world, actor_id);
    }
    let bo05 = match (
        world.entities.get(actor_id),
        world
            .entities
            .get(actor_id)
            .and_then(|e| e.ai.target_creature)
            .and_then(|t| world.entities.get(t)),
    ) {
        (Some(a), Some(t)) => {
            (t.x - a.x).abs() > a.attack_range || (t.y - a.y).abs() > a.attack_range
        }
        _ => true,
    };
    if bo05 {
        stick_come_down(world, actor_id);
    } else {
        let _ = stick_attack_target(world, actor_id);
    }
}

/// `BigHeartMonster.AttackTarget`（`BigHeartMonster.cs:17-42`）。
///
/// 唯一取数点：`Random(Math.Abs(Hi(DC)-Lo(DC)) + 1)`（`:22`）——**先于**可见目标循环，
/// 即使没有收件人也抽一次。
pub fn big_heart_attack_target(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
    if !due {
        return false;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.attack_tick = now;
    }
    let (dir, x, y, dc, view_range) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y, e.dc, e.view_range),
        None => return false,
    };
    // `SendRefMsg(RM_HIT, Dir, CurrX, CurrY, 0, "")`（`:20`）——在取数之前。
    send_ref_msg(world, actor_id, RM_HIT, dir as i32, x, y, 0);
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_power = i32::max(0, lo + world.rng.random_below((hi - lo).abs() + 1));
    let visible: Vec<i32> = world
        .entities
        .get(actor_id)
        .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
        .unwrap_or_default();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if !(proper && (x - t.x).abs() <= view_range && (y - t.y).abs() <= view_range) {
            continue;
        }
        let (tx, ty) = (t.x, t.y);
        // `SendSelfDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(x, y), 1, ActorId, "", 200)`。
        world.send_self_delay_msg_full(
            actor_id,
            crate::slave::RM_DELAYMAGIC,
            n_power,
            make_long(tx, ty),
            1,
            target_id,
            200,
        );
        // `SendRefMsg(RM_10205, 0, baseObject.CurrX, baseObject.CurrY, 1, "")`。
        send_ref_msg(world, actor_id, RM_10205, 0, tx, ty, 1);
    }
    true
}

/// `BigHeartMonster.Run`（`BigHeartMonster.cs:44-54`）。
fn big_heart_monster_run(world: &mut World, actor_id: i32) {
    let ok = world
        .entities
        .get(actor_id)
        .is_some_and(|e| can_move(e) && !e.visible_actors.is_empty());
    if ok {
        let _ = big_heart_attack_target(world, actor_id);
    }
}

// ---------------------------------------------------------------------------
// **MB-2**（R13/R24/R08/R43）——近身 RNG 回退 / 复生。
// ---------------------------------------------------------------------------

/// `MonsterObject.Run` 里的**虚调用** `AttackTarget()`（`MonsterObject.cs:277`）分派。
///
/// 本批接 `DualAxeMonster.AttackTarget`（`DualAxeMonster.cs:33-72`，93/104 继承同一实现）；
/// 其余 race 仍走基版 [`attack_target`]。
fn monster_attack_target(world: &mut World, actor_id: i32) -> bool {
    match world.entities.get(actor_id).map(|e| e.race) {
        Some(87 | 93 | 104) => dual_axe_attack_target(world, actor_id),
        // **MB-3**：`GasAttackMonster.AttackTarget`（90/105/106 继承）。
        Some(90 | 105 | 106) => gas_attack_monster_attack_target(world, actor_id),
        // `CentipedeKingMonster.AttackTarget`（107）。
        Some(107) => centipede_king_attack_target(world, actor_id),
        // `ScultureKingMonster` 用的是基版 `MonsterObject.AttackTarget`，但虚调用 `Attack`
        // 被覆写为 `HitMagAttackTarget`（102）。
        Some(102) => sculture_king_attack_target(world, actor_id),
        // **MB-7**：`BoneKingMonster.Attack`（`BoneKingMonster.cs:42-46`）与
        // `ScultureKingMonster.Attack`（`:55-59`）**逐字相同** ⇒ 复用同一包装。
        Some(134) => sculture_king_attack_target(world, actor_id),
        // **MB-4**：`CowKingMonster.Attack`（`:27-31`：`GetAttackPower` + 半物理/半魔法
        // `HitMagAttackTarget`）。
        Some(92) => cow_king_attack_target(world, actor_id),
        // **`openmir2-attacktarget-overrides`**：余下 7 类（见上方分区注释）。
        Some(130) => double_critical_attack_target(world, actor_id),
        // `SpitSpider`（82）/`HighRiskSpider`（118）/`BigPoisionSpider`（119）继承同一实现，
        // 但 `UsePoison` 由各自 ctor 决定：82=true（`SpitSpider.cs:16`）、
        // 118=false（`HighRiskSpider.cs:8`）、119=true（`BigPoisionSpider.cs:8`）。
        Some(82) => spit_spider_attack_target(world, actor_id, true),
        Some(118) => spit_spider_attack_target(world, actor_id, false),
        Some(119) => spit_spider_attack_target(world, actor_id, true),
        Some(117) => explosion_spider_attack_target(world, actor_id),
        Some(91) => mag_cow_attack_target(world, actor_id),
        // `ElfWarriorMonster`（114）：`Appr` 形态门内再分派 base / 强化 spit。
        Some(114) => elf_warrior_attack_target(world, actor_id),
        // `FireballMonster`（215）继承 `MagicMonster.AttackTarget`。
        Some(215) => magic_monster_attack_target(world, actor_id),
        // `PercentMonster`（死类；ctor 自置 Race=80）：与基类逐行同构。
        Some(80) => percent_monster_attack_target(world, actor_id),
        _ => attack_target(world, actor_id),
    }
}

/// `MonsterObject.AttackTarget` 的 race 102 版本：与 [`attack_target`] 同门/同状态写点，
/// 仅把到点后的虚调用 `Attack(target, btDir)` 换成 [`sculture_king_attack`]。
fn sculture_king_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
                e.ai.holy_seize = false;
            }
            turn_to(world, actor_id, bt_dir);
            sculture_king_attack(world, actor_id, target_id);
        }
        true
    } else {
        let (same_map, tx, ty) = match (
            world.entities.get(actor_id).map(|e| e.map_id),
            world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
        ) {
            (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
            _ => (false, 0, 0),
        };
        if same_map {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// `DualAxeMonster.AttackMax` 的 race → 常量映射（`DualAxeMonster.cs:80` =2；
/// `ThornDarkMonster.cs:7` =3；`ArcherMonster.cs:10` =6）。类常量，不随实例变化。
fn dual_axe_attack_max(race: u8) -> i32 {
    match race {
        93 => 3,
        104 => 6,
        _ => 2,
    }
}

/// `DualAxeMonster.AttackTarget()`（`DualAxeMonster.cs:33-72`）。
///
/// **oracle bug 照搬**：`:42`/`:61` 两处都判 `|CurrX - TargetCret.CurrX|`（`CurrY` 不参与）。
/// `AttackTick` 在距离判定**之前**刷新（`:41`）；只有 `AttackMax - 1 > _mNAttackCount` 才飞斧，
/// 否则抽 `Random(5)`（`==0 ⇒ _mNAttackCount = 0`）。
// oracle `:42`/`:61` 两处都写 `Math.Abs(CurrX - TargetCret.CurrX)`（副本），照抄 ⇒ 显式放行 eq_op。
#[allow(clippy::eq_op)]
fn dual_axe_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    let due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
    if !due {
        return false;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.attack_tick = now;
    }
    let (cx, race) = match world.entities.get(actor_id) {
        Some(e) => (e.x, e.race),
        None => return false,
    };
    let Some((tx, ty, t_map, a_map)) = world
        .entities
        .get(target_id)
        .zip(world.entities.get(actor_id))
        .map(|(t, a)| (t.x, t.y, t.map_id, a.map_id))
    else {
        return false;
    };
    // `:42`：两处都是 `Math.Abs(CurrX - TargetCret.CurrX) <= 7`（照抄 bug）。
    let dx = (cx - tx).abs();
    if dx <= 7 && dx <= 7 {
        let attack_max = dual_axe_attack_max(race);
        let count = world
            .entities
            .get(actor_id)
            .map_or(0, |e| e.ai.m_n_attack_count);
        if attack_max - 1 > count {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.m_n_attack_count += 1;
                e.ai.target_focus_tick = now;
            }
            fly_axe_attack(world, actor_id, target_id);
        } else if world.rng.random_below(5) == 0 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.m_n_attack_count = 0;
            }
        }
        return true;
    }
    if t_map == a_map {
        // `:61`：两处都是 `|CurrX - TargetCret.CurrX| <= 11`（照抄 bug）。
        if dx <= 11 && dx <= 11 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        }
    } else {
        del_target_creature(world, actor_id);
    }
    false
}

/// `DualAxeMonster.FlyAxeAttack(IActor target)`（`DualAxeMonster.cs:14-31`）。
///
/// 取数序：`CanFly` 门失败 ⇒ **零抽**；通过 ⇒ `Random(|Hi(DC)-Lo(DC)|+1)`（`:19`）→
/// 目标 `GetHitStruckDamage`（AC 两抽）→ 目标 `StruckDamage`（玩家耐久抽）。
/// 末尾 `SendRefMsg(RM_FLYAXE, Dir, CurrX, CurrY, target.ActorId, "")`（`:29`）。
fn fly_axe_attack(world: &mut World, actor_id: i32, target_id: i32) {
    let Some((cx, cy, map_id)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.map_id)) else {
        return;
    };
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    if !world.can_fly(map_id, cx, cy, tx, ty) {
        return;
    }
    let dir = crate::magic::next_direction(cx, cy, tx, ty);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let dc = world.entities.get(actor_id).map_or(0, |e| e.dc);
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_damage = lo + world.rng.random_below((hi - lo).abs() + 1);
    if n_damage > 0 {
        let Some(attacker) = world.entities.get(actor_id).cloned() else {
            return;
        };
        let Some(target) = world.entities.get(target_id).cloned() else {
            return;
        };
        let reduced =
            crate::combat::get_hit_struck_damage(&mut world.rng, &target, &attacker, n_damage);
        if reduced > 0 {
            let last_hitter = target
                .last_hiter
                .and_then(|id| world.entities.get(id))
                .map(|h| crate::combat::LastHitter {
                    race: h.race,
                    job: h.job,
                    has_master: h.master.is_some(),
                });
            let cfg = world.combat_config;
            let per10 = crate::combat::struck_damage_multiplier(
                &target,
                last_hitter,
                cfg.warr_mon,
                cfg.wizard_mon,
                cfg.taos_mon,
                cfg.mon_hum,
            );
            let final_damage = (i32::from(reduced) * per10 / 10).max(0) as u16;
            let last_hiter_id = target.last_hiter;
            // `BaseObject.StruckDamage` → `DamageHealth`（`BaseObject.Base.cs:827-857`）。
            crate::buff::damage_health(world, target_id, i32::from(final_damage), last_hiter_id);
            // `PlayObject.StruckDamage` 在 `base.StruckDamage` **之后**的耐久段
            // （`PlayObject.cs:2957/2998-3039`）；函数自身对非玩家目标零动作。
            if let Some(t) = world.entities.get_mut(target_id) {
                crate::combat::struck_damage_durability(&mut world.rng, t);
            }
            let (hp, killed) = world
                .entities
                .get(target_id)
                .map_or((0, false), |e| (e.hp, e.hp == 0));
            world.pending_ai_attacks.push((
                actor_id,
                target_id,
                crate::combat::AttackOutcome {
                    power: n_damage,
                    hit: true,
                    damage: final_damage,
                    struck_power: reduced,
                    killed,
                    target_hp_after: hp,
                },
            ));
            // `target.SendStruckDelayMsg(RM_REFMESSAGE, nDamage, …,
            //   max(|Δx|,|Δy|)*50+600)`（`DualAxeMonster.cs:26-28`）——与 ArcherGuard 同形。
            let delay =
                i64::from((cx - tx).abs().max((cy - ty).abs()) * 50) + GUARD_STRUCK_DELAY_BASE_MS;
            world.send_struck_delay_msg_refmessage(target_id, actor_id, i32::from(reduced), delay);
        }
    }
    send_ref_msg_with_target(
        world,
        actor_id,
        RM_FLYAXE,
        i32::from(dir),
        cx,
        cy,
        target_id,
        Some(target_id),
    );
}

/// `DualAxeMonster.Run()`（`DualAxeMonster.cs:84-137`）。
///
/// 门：`CanMove()`；5s `SearchEnemyTick` 门扫最近 proper（`!HideMode || CoolEye`）⇒ `SetTargetCreat`；
/// `(now - WalkTick) > WalkSpeed && TargetCret != null`：
/// `|Δx| <= 4`（**四处都是 CurrX，照抄 bug**）⇒ `|Δx| <= 2` 时 `Random(5)==0` 才回退，
/// 3-4 格直接回退。`WalkTick` 刷新在 `base.Run()`（= `MonsterObject.Run`）里。
// oracle `:120/:122` 四处判定全部重复写 `Math.Abs(CurrX - TargetCret.CurrX)`，照抄 ⇒ 放行 eq_op。
#[allow(clippy::eq_op)]
fn dual_axe_monster_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let (cx, cy, cool_eye, walk_tick, walk_speed, enemy_tick) = (
        e.x,
        e.y,
        e.ai.cool_eye,
        e.ai.walk_tick,
        e.ai.walk_speed,
        e.ai.search_enemy_tick,
    );
    if now - enemy_tick >= 5000 {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        let visible: Vec<i32> = world
            .entities
            .get(actor_id)
            .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
            .unwrap_or_default();
        let mut n_rage = 9999;
        let mut best: Option<i32> = None;
        for target_id in visible {
            let Some(t) = world.entities.get(target_id) else {
                continue;
            };
            if t.death {
                continue;
            }
            let proper = world
                .entities
                .get(actor_id)
                .is_some_and(|a| is_proper_target(a, t));
            if proper && (!t.hide_mode || cool_eye) {
                let dist = (cx - t.x).abs() + (cy - t.y).abs();
                if dist < n_rage {
                    n_rage = dist;
                    best = Some(target_id);
                }
            }
        }
        if let Some(target_id) = best {
            set_target_creature(world, actor_id, target_id);
        }
    }
    let target = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature);
    let Some(target_id) = target else {
        return;
    };
    if now - walk_tick <= walk_speed {
        return;
    }
    let Some(tx) = world.entities.get(target_id).map(|t| t.x) else {
        return;
    };
    // `:120`/`:122`：四处判定全部是 `Math.Abs(CurrX - TargetCret.CurrX)`——CurrY 从不参与。
    let dx = (cx - tx).abs();
    if dx <= 4 && dx <= 4 {
        if dx <= 2 && dx <= 2 {
            if world.rng.random_below(5) == 0 {
                if let Some((bx, by)) = get_back_position(world, actor_id) {
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        e.ai.target_x = bx;
                        e.ai.target_y = by;
                    }
                }
            }
        } else if let Some((bx, by)) = get_back_position(world, actor_id) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = bx;
                e.ai.target_y = by;
            }
        }
    }
}

/// `LightingZombi.LightingAttack(byte nDir)`（`LightingZombi.cs:15-30`）。
///
/// `Dir = nDir` → `SendRefMsg(RM_LIGHTING, 1, CurrX, CurrY, TargetCret.ActorId, "")` →
/// `GetNextPosition(...,1)` 成功才：`GetNextPosition(...,9)` →
/// `Random(|Hi(DC)-Lo(DC)|+1)` → `MagPassThroughMagic(..., true)` → `BreakHolySeizeMode()`。
fn lighting_attack(world: &mut World, actor_id: i32, n_dir: u8) {
    let Some((cx, cy, map_id, target_id)) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature.map(|t| (e.x, e.y, e.map_id, t)))
    else {
        return;
    };
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = n_dir;
    }
    send_ref_msg_with_target(
        world,
        actor_id,
        RM_LIGHTING,
        1,
        cx,
        cy,
        target_id,
        Some(target_id),
    );
    let Some((sx, sy)) = world
        .maps
        .get(map_id)
        .and_then(|m| m.get_next_position(cx, cy, n_dir, 1))
    else {
        return;
    };
    let (tx, ty) = world
        .maps
        .get(map_id)
        .and_then(|m| m.get_next_position(cx, cy, n_dir, 9))
        .unwrap_or((cx, cy));
    let dc = world.entities.get(actor_id).map_or(0, |e| e.dc);
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_power = i32::max(0, lo + world.rng.random_below((hi - lo).abs() + 1));
    world.mag_pass_through_magic(actor_id, sx, sy, tx, ty, n_dir, n_power, true);
    break_holy_seize_mode(world, actor_id);
}

/// `LightingZombi.Run()`（`LightingZombi.cs:32-58`）。
///
/// 8s/1s 索敌；`|Δ|<=4` 且 `|Δ|<=2` 时 `Random(3)!=0` ⇒ **`base.Run(); return;`**
/// （跳过本类的 `LightingAttack` 段——调用方在本函数返回后照常跑 `base.Run()` 链）。
fn lighting_zombi_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && now - e.ai.search_enemy_tick > 8000) {
        return;
    }
    let enemy_tick = e.ai.search_enemy_tick;
    let has_target = e.ai.target_creature.is_some();
    if now - enemy_tick > 1000 && !has_target {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
    let Some((cx, cy, walk_tick, walk_speed, target_id)) = world.entities.get(actor_id).map(|e| {
        (
            e.x,
            e.y,
            e.ai.walk_tick,
            e.ai.walk_speed,
            e.ai.target_creature,
        )
    }) else {
        return;
    };
    let Some(target_id) = target_id else {
        return;
    };
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    let dx = (cx - tx).abs();
    let dy = (cy - ty).abs();
    if now - walk_tick > walk_speed && dx <= 4 && dy <= 4 {
        if dx <= 2 && dy <= 2 && world.rng.random_below(3) != 0 {
            // oracle `base.Run(); return;`——跳过下面的 LightningAttack 段。
            return;
        }
        if let Some((bx, by)) = get_back_position(world, actor_id) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = bx;
                e.ai.target_y = by;
            }
        }
    }
    let due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
    if dx < 6 && dy < 6 && due {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.attack_tick = now;
        }
        let dir = crate::magic::next_direction(cx, cy, tx, ty);
        lighting_attack(world, actor_id, dir);
    }
}

/// `ChickenDeer.Run()`（`ChickenDeer.cs:13-63`）。
///
/// 两处 oracle bug **照搬**：
/// - `:55` 两个条件都是 `|CurrX - baseObject.CurrX| <= 6`（`Δy` 从不参与）；
/// - `baseObject` 是扫描循环的**残留变量**（循环结束停在 `VisibleActors` 最后一项，
///   即使该项 `Death`/非目标），**不是** `TargetCret`。
///
/// 第二段与扫描段共用同一个 `(now - WalkTick) >= WalkSpeed` 门 ⇒ 第二段可达时扫描段必已执行，
/// 残留变量必为「可见表最后一项」（可见表为空时 `RunAwayMode` 刚被清、第二段不可达）。
// oracle `:55` 两个条件都写 `Math.Abs(CurrX - baseObject.CurrX)`，照抄 ⇒ 放行 eq_op。
#[allow(clippy::eq_op)]
fn chicken_deer_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let (cx, cy, cool_eye, walk_tick, walk_speed) =
        (e.x, e.y, e.ai.cool_eye, e.ai.walk_tick, e.ai.walk_speed);
    if now - walk_tick < walk_speed {
        return;
    }
    let visible: Vec<i32> = world
        .entities
        .get(actor_id)
        .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
        .unwrap_or_default();
    let mut n10 = 9999;
    let mut best: Option<i32> = None;
    let mut residual: Option<i32> = None;
    for target_id in visible {
        // `baseObject = VisibleActors[i].BaseObject;`——先赋值，再判 `Death`（残留变量语义）。
        residual = Some(target_id);
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if proper && (!t.hide_mode || cool_eye) {
            let dist = (cx - t.x).abs() + (cy - t.y).abs();
            if dist < n10 {
                n10 = dist;
                best = Some(target_id);
            }
        }
    }
    if let Some(target_id) = best {
        // oracle 直接赋 `RunAwayMode = true; TargetCret = ...`（不走 `SetTargetCreat`，
        // 因此**不刷新** `TargetFocusTick`）。
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.run_away_mode = true;
            e.ai.target_creature = Some(target_id);
        }
    } else if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.run_away_mode = false;
        e.ai.target_creature = None;
    }
    let (run_away, target_id) = world.entities.get(actor_id).map_or((false, None), |e| {
        (e.ai.run_away_mode, e.ai.target_creature)
    });
    let Some(target_id) = target_id else {
        return;
    };
    if !run_away {
        return;
    }
    let Some(residual_x) = residual.and_then(|id| world.entities.get(id)).map(|r| r.x) else {
        return;
    };
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    // `:55`：两个条件都是 `|CurrX - baseObject.CurrX| <= 6`（`Δy` 不参与，照抄 bug）。
    let dx = (cx - residual_x).abs();
    if dx <= 6 && dx <= 6 {
        let dir = crate::magic::next_direction(cx, cy, tx, ty);
        // `Envir.GetNextPosition(TargetCret.CurrX, TargetCret.CurrY, n14, 5, ...)`——**起点是目标格**。
        let (nx, ny) = world
            .maps
            .get(world.entities.get(actor_id).map_or(0, |e| e.map_id))
            .and_then(|m| m.get_next_position(tx, ty, dir, 5))
            .unwrap_or((tx, ty));
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.target_x = nx;
            e.ai.target_y = ny;
        }
    }
}

/// `ZilKinZombi.Die()`（`ZilKinZombi.cs:24-33`）的覆写段。
///
/// `ZilKillCount > 0` ⇒ `ReAliveTick = now; ZilkinDieTick = (Random(20)+4)*1000;`
/// 随后**无条件** `ZilKillCount -= 1`（`>0` 门只包住前两行）。
///
/// 调用点：死亡结算（[`crate::world::World`] 的 `settle_deaths`，等价 oracle `base.Run` 的
/// `HP==0 ⇒ Die()`）。抽数站点落在本文件（`monster_ai` 组）。
pub fn zil_kin_zombi_die(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some((race, count)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.race, e.ai.zil_kill_count))
    else {
        return;
    };
    if race != 96 {
        return;
    }
    if count > 0 {
        let die_tick = i64::from(world.rng.random_below(20) + 4) * 1000;
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.re_alive_tick = now;
            e.ai.zilkin_die_tick = die_tick;
        }
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.zil_kill_count -= 1;
    }
}

/// `BaseObject.ReAlive()`（`BaseObject.Base.cs:354-358`）：`Death = false` + `RM_ALIVE`。
fn monster_re_alive(world: &mut World, actor_id: i32) {
    let Some((dir, x, y)) = world.entities.get(actor_id).map(|e| (e.ai.dir, e.x, e.y)) else {
        return;
    };
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.death = false;
        // 本骨架的死亡结算闩锁（oracle 无此字段，等价物）：不清 ⇒ 第二次死亡永不结算。
        e.death_settled = false;
    }
    send_ref_msg(world, actor_id, RM_ALIVE, i32::from(dir), x, y, 0);
}

/// `ZilKinZombi.Run()`（`ZilKinZombi.cs:35-47`）。
///
/// 门：`Death && !Ghost && STONE==0 && ZilKillCount >= 0 && VisibleActors.Count > 0 &&
/// (now - ReAliveTick) >= ZilkinDieTick`。
/// 复生：`MaxHP >>= 1`、`FightExp /= 2`、`HP = MaxHP`、`ReAlive()`、`WalkTick = now + 1000`。
fn zil_kin_zombi_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let revive = e.death
        && !e.ghost
        && e.status_time[crate::entity::POISON_STONE] == 0
        && e.ai.zil_kill_count >= 0
        && !e.visible_actors.is_empty()
        && now - e.re_alive_tick >= e.ai.zilkin_die_tick;
    if !revive {
        return;
    }
    let max_hp = e.max_hp >> 1;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.max_hp = max_hp;
        // `FightExp = FightExp / 2`（整数除法）。
        e.mon_exp /= 2;
        e.hp = max_hp;
    }
    monster_re_alive(world, actor_id);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now + 1000;
    }
}

/// `HUtil32.MakeLong(short low, short high)`（`src/OpenMir2/HUtil32.cs:51-54`）。
#[must_use]
pub fn make_long(low: i32, high: i32) -> i32 {
    (low as u16 as i32) | (high << 16)
}

/// `DigOutZombi.Turned`（`DigOutZombi.cs:57-64`）：`MapEvent(ET_DIGOUTZOMBI, 5min)`
/// → `FixedHideMode = false` → `SendRefMsg(RM_DIGUP, Dir, X, Y, event.Id, "")`。
fn dig_out_zombi_turned(world: &mut World, actor_id: i32) {
    let (map_id, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.map_id, e.x, e.y),
        None => return,
    };
    world.add_dig_out_event(map_id, x, y);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.fixed_hide_mode = false;
    }
    // `MapEvent.Id` 恒 0（`readonly` 且赋值行被注释）⇒ `nParam3 = 0`；wParam = `Dir`。
    let dir = world.entities.get(actor_id).map_or(0, |e| e.ai.dir);
    send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
}

/// `DigOutZombi.Run`（`DigOutZombi.cs:18-55`）。
fn dig_out_zombi_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && (now - e.ai.walk_tick) > e.ai.walk_speed) {
        return;
    }
    if e.fixed_hide_mode {
        let (cx, cy, cool_eye) = (e.x, e.y, e.ai.cool_eye);
        let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
        for target_id in visible {
            let Some(t) = world.entities.get(target_id) else {
                continue;
            };
            if t.death {
                continue;
            }
            let proper = world
                .entities
                .get(actor_id)
                .is_some_and(|a| is_proper_target(a, t));
            if proper
                && (!t.hide_mode || cool_eye)
                && (cx - t.x).abs() <= 3
                && (cy - t.y).abs() <= 3
            {
                dig_out_zombi_turned(world, actor_id);
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.walk_tick = now + 1000;
                }
                break;
            }
        }
        return;
    }
    let enemy_tick = e.ai.search_enemy_tick;
    let has_target = e.ai.target_creature.is_some();
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
}

/// `ScultureMonster.MeltStone`（`ScultureMonster.cs:18-24`）：
/// `CharStatusEx = 0` → `CharStatus = GetCharStatus()` → `RM_DIGUP` → `StoneMode = false`。
fn sculture_melt_stone(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.char_status_ex = 0;
    }
    let status = world.get_char_status(actor_id);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.char_status = status;
    }
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    // oracle `SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0, "")`（wParam = Dir）。
    send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.stone_mode = false;
    }
}

/// `ScultureMonster.MeltStoneAll`（`ScultureMonster.cs:26-42`）：
/// 先融自己，再 `GetMapBaseObjects(Envir, X, Y, 7)`（x 外、y 内、格内 `ObjList` 序；
/// 过滤 `Death`/`Ghost`）对每个 `StoneMode && is ScultureMonster` 融化。
fn sculture_melt_stone_all(world: &mut World, actor_id: i32) {
    sculture_melt_stone(world, actor_id);
    let (map_id, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.map_id, e.x, e.y),
        None => return,
    };
    let mut object_list = Vec::new();
    if let Some(map) = world.maps.get(map_id) {
        for sx in (x - 7)..=(x + 7) {
            for sy in (y - 7)..=(y + 7) {
                let (cell, ok) = map.get_cell_info(sx, sy);
                if !(ok && cell.is_available()) {
                    continue;
                }
                for o in &cell.obj_list {
                    if o.cell_obj_id <= 0 || !o.actor_object {
                        continue;
                    }
                    if world
                        .entities
                        .get(o.cell_obj_id)
                        .is_some_and(|e| !e.death && !e.ghost)
                    {
                        object_list.push(o.cell_obj_id);
                    }
                }
            }
        }
    }
    for id in object_list {
        let hit = world
            .entities
            .get(id)
            .is_some_and(|e| e.stone_mode && e.race == 101);
        if hit {
            sculture_melt_stone(world, id);
        }
    }
}

/// `ScultureMonster.Run`（`ScultureMonster.cs:43-79`）。注意门是 `>=`（不是 `>`），
/// 且 `StoneMode` 支**不刷新** `WalkTick`。
fn sculture_monster_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && (now - e.ai.walk_tick) >= e.ai.walk_speed) {
        return;
    }
    if e.stone_mode {
        let (cx, cy, cool_eye) = (e.x, e.y, e.ai.cool_eye);
        let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
        for target_id in visible {
            let Some(t) = world.entities.get(target_id) else {
                continue;
            };
            if t.death {
                continue;
            }
            let proper = world
                .entities
                .get(actor_id)
                .is_some_and(|a| is_proper_target(a, t));
            if proper
                && (!t.hide_mode || cool_eye)
                && (cx - t.x).abs() <= 2
                && (cy - t.y).abs() <= 2
            {
                sculture_melt_stone_all(world, actor_id);
                break;
            }
        }
        return;
    }
    let enemy_tick = e.ai.search_enemy_tick;
    let has_target = e.ai.target_creature.is_some();
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
    }
}

// ---------------------------------------------------------------------------
// **MB-3**（R07/R21/R34/R02）——延迟消息 / 毒 / 守卫。
// ---------------------------------------------------------------------------

/// 召唤 / 复制类生成请求的**种类**（oracle 三个同步 `RegenMonsterByName` 调用点）。
///
/// - [`Self::Sculture`]：`ScultureKingMonster.CallSlave`（`ScultureKingMonster.cs:35-53`）；
/// - [`Self::BoneKing`]：`BoneKingMonster.CallSlave`（`BoneKingMonster.cs:21-40`）；
/// - [`Self::MakeClone`]：`MonsterObject.MakeClone`（`MonsterObject.cs:100-136`）——
///   `ElfMonster.Run` / `ElfWarriorMonster.Run` 的换形支。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClassSlaveSpawnKind {
    /// `Random(4)` 选 `Config.Zuma[idx]`（`ScultureKingMonster.cs:47`）。
    Sculture,
    /// `Random(3)` 选 `BoneCaptain/BoneArcher/BoneSpearman`（`BoneKingMonster.cs:22/34`）。
    BoneKing,
    /// `MakeClone(name, this)`：先造 `name`，再按 `MonsterObject.cs:100-136` 复制字段；
    /// `source_race` 决定 `Run` 收尾的帧/`AppearNow` 分派（113/114）。
    MakeClone { name: String, source_race: u8 },
}

/// 一条待执行的 `RegenMonsterByName` 生成批次。
///
/// `count` 在 `Run` 的调用点**同点同次**抽出（ScultureKing/BoneKing 的 `Random(6)+6`）；
/// `Random(3)`/`Random(4)` 与 `RegenMonsterByName` 的构造抽由宿主
/// [`World::flush_sculture_spawns`] 在**同一条** `world.rng` 上按 oracle 序执行
/// （这样 `>= 30` 的门与每次 `Random(n)` 的次数才与成功次数一致）。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScultureSpawnRequest {
    pub owner_id: i32,
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub count: i32,
    pub kind: ClassSlaveSpawnKind,
}

impl World {
    /// `GameSvrConf.Zuma[4]`（`[Names] Zuma1..4`）的接缝。
    pub fn set_zuma_names(&mut self, names: &[String; 4]) {
        self.zuma_names = names.clone();
    }

    /// 召唤 / 复制类请求的宿主排水（oracle 同步 `RegenMonsterByName` 的等价物）。
    ///
    /// 按 [`ClassSlaveSpawnKind`] 逐字执行：
    /// - `Sculture`：先判 `_mSlaveObjectList.Count >= 30` ⇒ break；再抽 `Random(4)` 选
    ///   `Config.Zuma[idx]`；`RegenMonsterByName(MapName, nX, nY, name)` 成功 ⇒ 入表；
    /// - `BoneKing`：同样先判 `SlaveObjectList.Count >= 30`，再抽 `Random(3)` 选
    ///   `BoneCaptain/BoneArcher/BoneSpearman`（`BoneKingMonster.cs:31-38`）；
    /// - `MakeClone`：在主人 `CurrX/CurrY` 造 `name`，成功后按 `MonsterObject.cs:100-136`
    ///   复制字段，并执行 `ElfMonster`/`ElfWarriorMonster` `Run` 的收尾
    ///   （帧 / `AppearNow` / `Master=null` / `KickException`）。
    ///
    /// **次序登记**（与 `flush_special_spawns` 同口径）：oracle 在各自 `Run()` 内同步造怪；
    /// 本仓在 `world.tick()` 之后排水 ⇒ 构造抽相对其它 AI 抽数的位置不同。无这些调用的 tick
    /// 零抽数（五批 trace 逐字节不变）；有调用的场景需真机窗口核次序（R 级余量）。
    /// `MakeClone` 的 `KickException` 也因此在 `base.Run()` **之后**落定（oracle 在之前），
    /// 三条 elf 路径数据不可达（规划件 MB-7），登记为已知次序差。
    pub fn flush_sculture_spawns<S: crate::special_monster::SpecialSpawner + ?Sized>(
        &mut self,
        spawner: &mut S,
    ) -> Vec<i32> {
        const BONE_KING_NAMES: [&str; 3] = ["BoneCaptain", "BoneArcher", "BoneSpearman"];
        let reqs = std::mem::take(&mut self.pending_sculture_spawns);
        let mut out = Vec::new();
        for req in reqs {
            match req.kind.clone() {
                ClassSlaveSpawnKind::Sculture => {
                    for _ in 0..req.count {
                        let list_len = self
                            .entities
                            .get(req.owner_id)
                            .map_or(0, |e| e.class_slave_list.len());
                        if list_len >= 30 {
                            break;
                        }
                        let idx = self.rng.random_below(4) as usize;
                        let name = self
                            .zuma_names
                            .get(idx)
                            .cloned()
                            .unwrap_or_else(|| self.zuma_names[0].clone());
                        if let Some(child) =
                            self.regen_one(spawner, req.map_id, &name, req.x, req.y)
                        {
                            out.push(child);
                            if let Some(e) = self.entities.get_mut(req.owner_id) {
                                e.class_slave_list.push(child);
                            }
                        }
                    }
                }
                // `BoneKingMonster.CallSlave`：`nC` 已在 `Run` 调用点抽好；
                // 每个 minion 先判 30 上限再抽 `Random(3)`（`BoneKingMonster.cs:33-34`）。
                ClassSlaveSpawnKind::BoneKing => {
                    for _ in 0..req.count {
                        let list_len = self
                            .entities
                            .get(req.owner_id)
                            .map_or(0, |e| e.class_slave_list.len());
                        if list_len >= 30 {
                            break;
                        }
                        let idx = self.rng.random_below(3) as usize;
                        let name = BONE_KING_NAMES[idx];
                        if let Some(child) = self.regen_one(spawner, req.map_id, name, req.x, req.y)
                        {
                            out.push(child);
                            if let Some(e) = self.entities.get_mut(req.owner_id) {
                                e.class_slave_list.push(child);
                            }
                        }
                    }
                }
                ClassSlaveSpawnKind::MakeClone { name, source_race } => {
                    if let Some(child) = self.regen_one(spawner, req.map_id, &name, req.x, req.y) {
                        apply_make_clone(self, req.owner_id, child);
                        match source_race {
                            // `ElfMonster.Run`（`ElfMonster.cs:70-83`）。
                            crate::entity::ACTOR_RACE_ELF_MONSTER => {
                                if self.entities.get(child).is_some_and(|e| {
                                    e.race == crate::entity::ACTOR_RACE_ELF_WARRIORMON
                                }) {
                                    elf_warrior_appear_now(self, child);
                                }
                                if let Some(e) = self.entities.get_mut(req.owner_id) {
                                    e.master = None;
                                }
                                kick_exception(self, req.owner_id);
                            }
                            // `ElfWarriorMonster.Run`（`ElfWarriorMonster.cs:146-157`）。
                            crate::entity::ACTOR_RACE_ELF_WARRIORMON => {
                                let (dir, x, y) = self
                                    .entities
                                    .get(req.owner_id)
                                    .map_or((0, 0, 0), |e| (e.ai.dir, e.x, e.y));
                                send_ref_msg(
                                    self,
                                    req.owner_id,
                                    RM_DIGDOWN,
                                    i32::from(dir),
                                    x,
                                    y,
                                    0,
                                );
                                send_ref_msg(
                                    self,
                                    req.owner_id,
                                    RM_CHANGEFACE,
                                    0,
                                    req.owner_id,
                                    child,
                                    0,
                                );
                                if self.entities.get(child).is_some_and(|e| {
                                    e.race == crate::entity::ACTOR_RACE_ELF_MONSTER
                                }) {
                                    elf_monster_appear_now(self, child);
                                }
                                if let Some(e) = self.entities.get_mut(req.owner_id) {
                                    e.master = None;
                                }
                                kick_exception(self, req.owner_id);
                            }
                            _ => {}
                        }
                        out.push(child);
                    }
                }
            }
        }
        out
    }

    /// 在 `world.rng` 上执行一次宿主 `RegenMonsterByName`（同一条 RNG 流）。
    fn regen_one<S: crate::special_monster::SpecialSpawner + ?Sized>(
        &mut self,
        spawner: &mut S,
        map_id: usize,
        name: &str,
        x: i32,
        y: i32,
    ) -> Option<i32> {
        let mut rng =
            std::mem::replace(&mut self.rng, mir2_shared::rng::RandomNumber::with_seed(0));
        let child = spawner.regen_monster_by_name(self, &mut rng, map_id, name, x as i16, y as i16);
        self.rng = rng;
        child
    }
}

/// `MonsterObject.MakeClone`（`MonsterObject.cs:100-136`）的**字段复制段**。
///
/// 逐句（`old` = 调用者 `OldMon`，`clone` = 刚造出的怪）：
/// 1. `if (OldMon.TargetCret == null) OldMon.TargetCret = Master.TargetCret ?? Master.LastHiter`
///    —— `Master == null` 时 oracle 会 NRE；本仓按「不可达」跳过继承（登记）。
/// 2. `clone.Master/MasterRoyaltyTick/SlaveMakeLevel/SlaveExpLevel` 复制；
/// 3. `clone.RecalcAbilitys()`：base 的 `ApplySlaveLevelAbilitys` + 派生 `ResetElfMon`
///    （`ElfMonster.cs:31-41` / `ElfWarriorMonster.cs:35-39/105-110`）；
/// 4. `clone.Abil = OldMon.Abil`（`MonsterObject.cs:115`）：复制 [`Entity::abil`] 快照
///    （`Abil.DC`/`Abil.MaxHP`）。**必须**发生在第 3 步 `RecalcAbilitys()` 之后（oracle
///    顺序）：第 3 步用克隆自己的 `Abil`，随后的 `AppearNow` 再用源怪 `Abil` 重算
///    （`f(Abil)` 而非 `f(f(初值))`）；
/// 5. `StatusTimeArr/TargetCret/TargetFocusTick/LastHiter/LastHiterTick/Dir/IsSlave` 复制；
/// 6. `if (OldMon.Master != null) Master.SlaveList.Add(clone)`。
///
/// `RefNameColor()` 只改展示色（未建模，登记）。
fn apply_make_clone(world: &mut World, owner_id: i32, clone_id: i32) {
    // 1. `TargetCret` 继承（先读 Master 的目标，再写回 OldMon）。
    let inherited_target = {
        let Some(old) = world.entities.get(owner_id) else {
            return;
        };
        if old.ai.target_creature.is_some() {
            None
        } else {
            old.master
                .and_then(|m| world.entities.get(m))
                .and_then(|m| m.ai.target_creature.or(m.last_hiter))
        }
    };
    if let Some(target) = inherited_target {
        if let Some(e) = world.entities.get_mut(owner_id) {
            e.ai.target_creature = Some(target);
        }
    }
    let Some(old) = world.entities.get(owner_id) else {
        return;
    };
    let (master, royalty, make_level, exp_level) = (
        old.master,
        old.master_royalty_tick,
        old.slave_make_level,
        old.slave_exp_level,
    );
    let (status_time, dir, target, focus, last_hiter, last_hiter_tick) = (
        old.status_time,
        old.ai.dir,
        old.ai.target_creature,
        old.ai.target_focus_tick,
        old.last_hiter,
        old.last_hiter_tick,
    );
    // 4. `clone.Abil = OldMon.Abil`（`MonsterObject.cs:115`）：先取源怪的 `Abil` 快照
    //    （未取过则按其当前 `dc`/`max_hp` 懒取——源怪未 `RecalcAbilitys` 过时二者相等）；
    //    等克隆自己的第一次 `RecalcAbilitys()` 落定后再写入 `clone.abil`（oracle 顺序）。
    let old_abil = world.entities.get_mut(owner_id).map(Entity::abil_snapshot);
    let now = world.clock_ms;
    let Some(clone) = world.entities.get_mut(clone_id) else {
        return;
    };
    clone.master = master;
    clone.master_royalty_tick = royalty;
    clone.slave_make_level = make_level;
    clone.slave_exp_level = exp_level;
    // 3. `RecalcAbilitys()`（base + 派生覆写），顺序与 oracle 一致。
    crate::slave::apply_slave_level_abilitys(clone);
    crate::slave::apply_derived_slave_recalc(clone, now);
    // 4. `ElfMon.Abil = OldMon.Abil`（`MonsterObject.cs:115`）——在第一次 Recalc 之后，
    //    供随后的 `AppearNow` 第二次 Recalc 从源怪 `Abil` 重算。
    if let Some(abil) = old_abil {
        clone.abil = Some(abil);
    }
    // 5. 其余字段复制。
    clone.status_time = status_time;
    clone.ai.target_creature = target;
    clone.ai.target_focus_tick = focus;
    clone.last_hiter = last_hiter;
    clone.last_hiter_tick = last_hiter_tick;
    clone.ai.dir = dir;
    clone.is_slave = true;
    // 6. `Master.SlaveList.Add(clone)`。
    if let Some(master_id) = master {
        if let Some(m) = world.entities.get_mut(master_id) {
            m.slave_list.push(clone_id);
        }
    }
}

/// `BaseObject.KickException`（`BaseObject.Base.cs:919-924`）：`Death=true` → `DeathTick=now`
/// → `MakeGhost()`。
fn kick_exception(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.death = true;
        e.death_tick = now;
    }
    make_ghost(world, actor_id);
}

/// `ElfMonster.AppearNow`（`ElfMonster.cs:24-29`）：
/// `FixedHideMode=false` → `RecalcAbilitys()` → `WalkTick += 800`。
fn elf_monster_appear_now(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.fixed_hide_mode = false;
        crate::slave::apply_slave_level_abilitys(e);
        crate::slave::apply_derived_slave_recalc(e, now);
        e.ai.walk_tick += 800;
    }
}

/// `ElfWarriorMonster.AppearNow`（`ElfWarriorMonster.cs:15-24`）：
/// `BoIsFirst=false` → `FixedHideMode=false` → `RM_DIGUP` → `RecalcAbilitys()` →
/// `WalkTick += 800` → `DigDownTick=now` → `Race=ElfWarriormon`。
fn elf_warrior_appear_now(world: &mut World, actor_id: i32) {
    let (dir, x, y) = world
        .entities
        .get(actor_id)
        .map_or((0, 0, 0), |e| (e.ai.dir, e.x, e.y));
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.bo_is_first = false;
        e.fixed_hide_mode = false;
    }
    send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        crate::slave::apply_slave_level_abilitys(e);
        crate::slave::apply_derived_slave_recalc(e, now);
        e.ai.walk_tick += 800;
        e.dig_down_tick = now;
        e.race = crate::entity::ACTOR_RACE_ELF_WARRIORMON;
    }
}

/// `CentipedeKingMonster.CheckAttackTarget`（`CentipedeKingMonster.cs:20-40`）：
/// 死怪跳过；`IsProperTarget` 且**两轴 `<= ViewRange`**。
fn centipede_king_check_attack_target(world: &World, actor_id: i32) -> bool {
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    let (cx, cy, view_range) = (e.x, e.y, e.view_range);
    let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if proper && (cx - t.x).abs() <= view_range && (cy - t.y).abs() <= view_range {
            return true;
        }
    }
    false
}

/// `CentipedeKingMonster.FindAttackTarget`（`:85-89`）：`StickMonster.FindAttackTarget` + HP 回满。
fn centipede_king_find_attack_target(world: &mut World, actor_id: i32) {
    stick_find_attack_target(world, actor_id);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.hp = e.max_hp;
    }
}

/// `CentipedeKingMonster.AttackTarget`（`:42-83`）。
///
/// 取数次序（勿改）：命中门 `(now - AttackTick) > NextHitTime` → `Random(|Hi(DC)-Lo(DC)|+1)`
/// （**无条件**）→ 逐可见目标 `< ViewRange`：`Random(4)`；`==0` 时再抽 `Random(3)`
/// （`!=0` ⇒ `DECHEALTH,60,3`；`==0` ⇒ `STONE,5,0`）并 `TargetCret = baseObject`。
fn centipede_king_attack_target(world: &mut World, actor_id: i32) -> bool {
    if !centipede_king_check_attack_target(world, actor_id) {
        return false;
    }
    let now = world.clock_ms;
    let due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
    if !due {
        return true;
    }
    let (dir, x, y, dc, view_range) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y, e.dc, e.view_range),
        None => return false,
    };
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.attack_tick = now;
    }
    // `SendAttackMsg(RM_HIT, Dir, CurrX, CurrY)`（`BaseObject.Attack.cs:173` ⇒ `SendRefMsg`）。
    send_ref_msg(world, actor_id, RM_HIT, dir as i32, x, y, 0);
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_power = i32::max(0, lo + world.rng.random_below((hi - lo).abs() + 1));
    let visible: Vec<i32> = world
        .entities
        .get(actor_id)
        .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
        .unwrap_or_default();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if !(proper && (x - t.x).abs() < view_range && (y - t.y).abs() < view_range) {
            continue;
        }
        let (tx, ty) = (t.x, t.y);
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.target_focus_tick = now;
        }
        // `SendSelfDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(x,y), 2, ActorId, "", 600)`。
        world.send_self_delay_msg_full(
            actor_id,
            crate::slave::RM_DELAYMAGIC,
            n_power,
            make_long(tx, ty),
            2,
            target_id,
            600,
        );
        if world.rng.random_below(4) == 0 {
            if world.rng.random_below(3) != 0 {
                world.make_posion(target_id, crate::entity::POISON_DECHEALTH as u8, 60, 3);
            } else {
                world.make_posion(target_id, crate::entity::POISON_STONE as u8, 5, 0);
            }
            set_target_creature(world, actor_id, target_id);
        }
    }
    true
}

/// `CentipedeKingMonster.Run`（`:91-143`）。
///
/// `base.Run()` 是 `StickMonster.Run`；其顶部门与本函数同为
/// `(now - WalkTick) > WalkSpeed` 且 `WalkTick` 已被刷新 ⇒ 该次 `base.Run` 在
/// `StickMonster.Run` 内必然早退，只落到 `AnimalObject.Run`+`BaseObject.Run`
/// ⇒ 调用方按 [`ClassRunBase::Animal`] 续跑即可（与 oracle 等价）。
fn centipede_king_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && (now - e.ai.walk_tick) > e.ai.walk_speed) {
        return;
    }
    let (cx, cy, cool_eye, fixed_hide) = (e.x, e.y, e.ai.cool_eye, e.fixed_hide_mode);
    let class_tick = e.class_attack_tick;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
    }
    if fixed_hide {
        if now - class_tick > 10_000 {
            let come_out = world.entities.get(actor_id).map_or(0, |e| e.come_out_value);
            let visible: Vec<i32> = world
                .entities
                .get(actor_id)
                .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
                .unwrap_or_default();
            for target_id in visible {
                let Some(t) = world.entities.get(target_id) else {
                    continue;
                };
                if t.death {
                    continue;
                }
                let proper = world
                    .entities
                    .get(actor_id)
                    .is_some_and(|a| is_proper_target(a, t));
                if proper
                    && (!t.hide_mode || cool_eye)
                    && (cx - t.x).abs() < come_out
                    && (cy - t.y).abs() < come_out
                {
                    centipede_king_find_attack_target(world, actor_id);
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        e.class_attack_tick = now;
                    }
                    break;
                }
            }
        }
        return;
    }
    if now - class_tick > 3_000 {
        if centipede_king_attack_target(world, actor_id) {
            return;
        }
        if now - class_tick > 10_000 {
            stick_come_down(world, actor_id);
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.class_attack_tick = now;
            }
        }
    }
}

/// `GasMothMonster.GasMothAttack`（`GasMothMonster.cs:37-66`）：
/// 曼哈顿最近 proper（**不**判隐藏/超距），有候选才 `SetTargetCreat`（否则不动原目标）。
fn gas_moth_attack(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let (cx, cy) = (e.x, e.y);
    let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
    let mut n10 = 999;
    let mut best = None;
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let proper = world
            .entities
            .get(actor_id)
            .is_some_and(|a| is_proper_target(a, t));
        if !proper {
            continue;
        }
        let n_c = (cx - t.x).abs() + (cy - t.y).abs();
        if n_c >= n10 {
            continue;
        }
        n10 = n_c;
        best = Some(target_id);
    }
    if let Some(target_id) = best {
        set_target_creature(world, actor_id, target_id);
    }
}

/// `GasMothMonster.Run`（`GasMothMonster.cs:24-35`）。`Run` 本身**零抽数**；
/// `base.Run()` = `ATMonster.Run`（同一索敌门已被刷新 ⇒ 早退）→ `MonsterObject.Run`。
fn gas_moth_monster_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && now - e.ai.walk_tick >= e.ai.walk_speed) {
        return;
    }
    let enemy_tick = e.ai.search_enemy_tick;
    let has_target = e.ai.target_creature.is_some();
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        gas_moth_attack(world, actor_id);
    }
}

/// `GasAttackMonster.GasAttack(bt05)`（`GasAttackMonster.cs:15-40`）。
///
/// 取数次序（勿改）：`Random(|Hi(DC)-Lo(DC)|+1)` →（`nPower > 0`）`GetPoseCreate` →
/// `Random(target.SpeedPoint)` → `GetMagStruckDamage`（1 抽）→（`nPower > 0`）
/// `Random(target.AntiPoison + 20)`。
///
/// `transparent_on_hit` = `GasMothMonster.GasAttack` override（`GasMothMonster.cs:14-22`）：
/// `baseObject != null` 后**先抽** `Random(3)`，`==0` **再判** `HideMode` ⇒ 置
/// `StatusTimeArr[STATETRANSPARENT] = 1`。
fn gas_attack(
    world: &mut World,
    actor_id: i32,
    bt_dir: u8,
    transparent_on_hit: bool,
) -> Option<i32> {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = bt_dir;
    }
    let e = world.entities.get(actor_id)?;
    let (x, y, dc, hit_point, map_id, undead) =
        (e.x, e.y, e.dc, e.hit_point, e.map_id, e.undead_power);
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let mut n_power = lo + world.rng.random_below((hi - lo).abs() + 1);
    let mut result = None;
    if n_power > 0 {
        send_ref_msg(world, actor_id, RM_HIT, i32::from(bt_dir), x, y, 0);
        let pose = front_of(world, actor_id, bt_dir, map_id)
            .and_then(|(fx, fy)| world.moving_object_at(map_id, fx, fy, true));
        if let Some(target_id) = pose {
            let proper = world
                .entities
                .get(actor_id)
                .zip(world.entities.get(target_id))
                .is_some_and(|(a, t)| is_proper_target(a, t));
            if proper {
                let speed_point = world
                    .entities
                    .get(target_id)
                    .map_or(0, |t| i32::from(t.speed_point));
                if world.rng.random_below(speed_point) < i32::from(hit_point) {
                    let mac = world.entities.get(target_id).map_or(0, |t| t.mac);
                    n_power = crate::combat::get_mag_struck_damage(
                        &mut world.rng,
                        mac,
                        n_power,
                        Some(undead),
                    );
                    if n_power > 0 {
                        let _ = class_struck_apply(world, actor_id, target_id, n_power, n_power);
                        // `baseObject.SendStruckDelayMsg(RM_REFMESSAGE, nPower, …,
                        //   ActorId, "", 300)`（`GasAttackMonster.cs:30`）——在
                        // `StruckDamage` 之后、`Random(AntiPoison+20)` 之前（oracle 同序）。
                        world.send_struck_delay_msg_refmessage(
                            target_id,
                            actor_id,
                            n_power,
                            GAS_STRUCK_DELAY_MS,
                        );
                        let anti_poison = world.target_anti_poison(target_id);
                        if world.rng.random_below(anti_poison + 20) == 0 {
                            world.make_posion(target_id, crate::entity::POISON_STONE as u8, 5, 0);
                        }
                        result = Some(target_id);
                    }
                }
            }
        }
    }
    if transparent_on_hit {
        if let Some(target_id) = result {
            if world.rng.random_below(3) == 0 {
                let hidden = world.entities.get(target_id).is_some_and(|t| t.hide_mode);
                if hidden {
                    if let Some(t) = world.entities.get_mut(target_id) {
                        t.status_time[crate::entity::POISON_STATETRANSPARENT] = 1;
                    }
                }
            }
        }
    }
    result
}

/// `GasAttackMonster.AttackTarget`（`GasAttackMonster.cs:42-73`）：
/// 邻接 ⇒ 到点（**严格 `>`**）则 `GasAttack(btDir)` + `BreakHolySeizeMode`，恒 `true`；
/// 不邻接 ⇒ 同图写 `TargetX/Y`，异图 `DelTargetCreat`，返回 `false`。
fn gas_attack_monster_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
            }
            let transparent = world.entities.get(actor_id).is_some_and(|e| e.race == 105);
            gas_attack(world, actor_id, bt_dir, transparent);
            break_holy_seize_mode(world, actor_id);
        }
        true
    } else {
        let (same_map, tx, ty) = match (
            world.entities.get(actor_id).map(|e| e.map_id),
            world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
        ) {
            (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
            _ => (false, 0, 0),
        };
        if same_map {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// 取目标当下的 `LastHiter` 子集（`StruckDamage` 倍率用；与 `World::attack_once` 同口径）。
fn current_last_hitter(world: &World, target_id: i32) -> Option<crate::combat::LastHitter> {
    world
        .entities
        .get(target_id)
        .and_then(|t| t.last_hiter)
        .and_then(|id| world.entities.get(id))
        .map(|h| crate::combat::LastHitter {
            race: h.race,
            job: h.job,
            has_master: h.master.is_some(),
        })
}

/// MB-3 三类攻击（`GasAttack`/`ScultureKing.Attack`/`ArcherGuard.AttackTarger`）共用的
/// `StruckDamage` + `DamageHealth` + 耐久段；返回 `None` = 目标不在场。
///
/// **不发 `RM_STRUCK`、不写 `LastHiter`**（MB-3b F1）：这三条 oracle 路径
/// （`GasAttackMonster.GasAttack` `:15-40`、`HitMagAttackTarget` `AnimalObject.cs:361-381`、
/// `ArcherGuard.AttackTarger` `:21-37`）都只调 `StruckDamage`，**没有** `_Attack`
/// （`BaseObject.Attack.cs:61-64`）的 `SendMsg(target, RM_STRUCK, …)`。折叠进这里会在目标
/// 自己的 `base.Run()` 多出 `Struck`（`Random(6)`/`Random(300)`）并改写 `TargetCret`/`ExpHitter`。
/// `ArcherGuard` 的同步 `SetLastHiter`/`ExpHitter=null` 由调用方在**本函数之前**落（oracle 顺序）。
fn class_struck_apply(
    world: &mut World,
    attacker_id: i32,
    target_id: i32,
    reduced: i32,
    power: i32,
) -> Option<crate::combat::AttackOutcome> {
    let target = world.entities.get(target_id)?.clone();
    let last_hitter = current_last_hitter(world, target_id);
    let cfg = world.combat_config;
    let per10 = crate::combat::struck_damage_multiplier(
        &target,
        last_hitter,
        cfg.warr_mon,
        cfg.wizard_mon,
        cfg.taos_mon,
        cfg.mon_hum,
    );
    let final_damage = (reduced * per10 / 10).max(0) as u16;
    let last_hiter_id = target.last_hiter;
    crate::buff::damage_health(world, target_id, i32::from(final_damage), last_hiter_id);
    if let Some(t) = world.entities.get_mut(target_id) {
        crate::combat::struck_damage_durability(&mut world.rng, t);
    }
    let (hp, killed) = world
        .entities
        .get(target_id)
        .map_or((0, false), |e| (e.hp, e.hp == 0));
    let outcome = crate::combat::AttackOutcome {
        power,
        hit: true,
        damage: final_damage,
        struck_power: reduced.max(0) as u16,
        killed,
        target_hp_after: hp,
    };
    world
        .pending_ai_attacks
        .push((attacker_id, target_id, outcome));
    Some(outcome)
}

/// `ScultureKingMonster.MeltStone`（`ScultureKingMonster.cs:25-33`）：
/// `ScultureMonster.MeltStone` 四步 + `MapEvent(..., ET_SCULPEICE=6, 5min, true)` 注册。
fn sculture_king_melt_stone(world: &mut World, actor_id: i32) {
    sculture_melt_stone(world, actor_id);
    let (map_id, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.map_id, e.x, e.y),
        None => return,
    };
    world.add_map_event_at(map_id, x, y, 6, 5 * 60 * 1000);
}

/// `ScultureKingMonster.CallSlave`（`:35-53`）的**抽点**：`Random(6) + 6` 与 `GetFrontPosition`。
/// `Random(4)` 与造怪在宿主排水 [`World::flush_sculture_spawns`] 内按 oracle 序执行。
fn sculture_king_call_slave(world: &mut World, actor_id: i32) {
    let Some((map_id, x, y, dir)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.map_id, e.x, e.y, e.ai.dir))
    else {
        return;
    };
    let count = world.rng.random_below(6) + 6;
    let (nx, ny) = front_of(world, actor_id, dir, map_id).unwrap_or((x, y));
    world.pending_sculture_spawns.push(ScultureSpawnRequest {
        owner_id: actor_id,
        map_id,
        x: nx,
        y: ny,
        count,
        kind: ClassSlaveSpawnKind::Sculture,
    });
}

// ---------------------------------------------------------------------------
// **MB-7**（R05/R15/R16）——`BoneKingMonster`(134) / `ElfMonster`(113) /
// `ElfWarriorMonster`(114)。
// ---------------------------------------------------------------------------

/// `GameSvrConf.Dragon1`（`GameSvrConf.cs:1759` 源码缺省 `"神兽1"`；部署
/// `E:/MirServer/M2GameSvr/Server.conf:130` 覆盖为 `"神兽"`，`ServerConf.cs:106` 读
/// `[Names] Dragon1`）——`ElfMonster.Run` 的 `MakeClone(Config.Dragon1, this)`
/// （`ElfMonster.cs:72`）。
///
/// **登记**：宿主配置通路不在本车道写集（`bins/game-svr/**` 属并行车道），此处取
/// **部署件值 `"神兽"`**（race 113）；若换了 `Server.conf` 的 `Dragon1`，克隆目标类会随
/// 之变化（`ElfMonster.cs:76` 的 `is ElfWarriorMonster` 分派）。
pub const DRAGON1_NAME: &str = "神兽";

/// `BoneKingMonster.CallSlave`（`BoneKingMonster.cs:21-40`）的**抽点**：
/// `Random(6) + 6` 与 `GetFrontPosition`；每个 minion 的 `Random(3)` 与造怪在
/// [`World::flush_sculture_spawns`] 内按 oracle 序执行。
fn bone_king_call_slave(world: &mut World, actor_id: i32) {
    let Some((map_id, x, y, dir)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.map_id, e.x, e.y, e.ai.dir))
    else {
        return;
    };
    let count = world.rng.random_below(6) + 6;
    let (nx, ny) = front_of(world, actor_id, dir, map_id).unwrap_or((x, y));
    world.pending_sculture_spawns.push(ScultureSpawnRequest {
        owner_id: actor_id,
        map_id,
        x: nx,
        y: ny,
        count,
        kind: ClassSlaveSpawnKind::BoneKing,
    });
}

/// `BoneKingMonster.Run`（`BoneKingMonster.cs:48-76`）。
///
/// ```csharp
/// if (CanMove() && GetTickCount() - WalkTick >= WalkSpeed) {
///   if ((now-SearchEnemyTick) > 8000 || (now-SearchEnemyTick) > 1000 && TargetCret == null) {
///     SearchEnemyTick = now; SearchTarget();
///     if (DangerLevel > WAbil.HP / WAbil.MaxHP * 5 && DangerLevel > 0) { DangerLevel -= 1; CallSlave(); }
///     if (WAbil.HP == WAbil.MaxHP) DangerLevel = 5;
///   }
///   for (i = SlaveObjectList.Count-1; i >= 0; i--) if (Death || Ghost) RemoveAt(i);
/// }
/// base.Run();
/// ```
///
/// **整数除法照抄**：`HP / MaxHP` 是 C# `int/int`（ushort 提升），`HP < MaxHP` ⇒ 商 0
/// ⇒ `DangerLevel > 0` 即触发；仅满血时阈值是 5。改坏必红：
/// `mb7_bone_king_danger_level_uses_integer_division`（HP=MaxHP-1 ⇒ 必须召唤）。
fn bone_king_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let now = world.clock_ms;
    if !(can_move(e) && now - e.ai.walk_tick >= e.ai.walk_speed) {
        return;
    }
    let (enemy_tick, has_target) = (e.ai.search_enemy_tick, e.ai.target_creature.is_some());
    // C# `&&` 优先于 `||`：`(>8000) || ((>1000) && TargetCret == null)`。
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
        // `DangerLevel > WAbil.HP / WAbil.MaxHP * 5 && DangerLevel > 0`。
        let (hp, max_hp, danger) = world
            .entities
            .get(actor_id)
            .map_or((0, 0, 0), |e| (e.hp, e.max_hp, e.danger_level));
        // `MaxHP == 0` 时 oracle 抛 DivideByZeroException；生产不变量 MaxHP>0（登记）。
        let threshold = if max_hp == 0 {
            0
        } else {
            (i32::from(hp) / i32::from(max_hp)) * 5
        };
        if danger > threshold && danger > 0 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.danger_level -= 1;
            }
            bone_king_call_slave(world, actor_id);
        }
        if hp == max_hp {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.danger_level = 5;
            }
        }
    }
    // `for (i = Count-1; i >= 0; i--) if (Death || Ghost) RemoveAt(i);`
    let dead_slaves: Vec<i32> = world
        .entities
        .get(actor_id)
        .map(|e| {
            e.class_slave_list
                .iter()
                .copied()
                .filter(|id| world.entities.get(*id).is_none_or(|s| s.death || s.ghost))
                .collect()
        })
        .unwrap_or_default();
    if !dead_slaves.is_empty() {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.class_slave_list.retain(|id| !dead_slaves.contains(id));
        }
    }
}

/// `ElfMonster.ResetElfMon`（`ElfMonster.cs:37-41`）：`WalkSpeed = 500 - SlaveMakeLevel*50`、
/// `WalkTick = now + 2000`。
fn elf_monster_reset(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_speed = 500 - i64::from(e.slave_make_level) * 50;
        e.ai.walk_tick = now + 2000;
    }
}

/// `ElfMonster.Run`（`ElfMonster.cs:43-86`）。
///
/// 首次出土 → 死亡 2s `MakeGhost` → 否则 `TargetCret`/主人目标 ⇒ `MakeClone(Dragon1)`。
/// 克隆的字段复制/收尾（`AppearNow`/`Master=null`/`KickException`）在宿主排水
/// [`World::flush_sculture_spawns`] 内落定（登记次序差，见该函数文档）。
fn elf_monster_run(world: &mut World, actor_id: i32) {
    let (bo_first, dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.bo_is_first, e.ai.dir, e.x, e.y),
        None => return,
    };
    if bo_first {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.bo_is_first = false;
            e.fixed_hide_mode = false;
        }
        send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
        elf_monster_reset(world, actor_id);
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let now = world.clock_ms;
    if e.death {
        if now - e.death_tick > 2 * 1000 {
            make_ghost(world, actor_id);
        }
        return;
    }
    let mut bo_change_face = e.ai.target_creature.is_some();
    if let Some(master_id) = e.master {
        if let Some(m) = world.entities.get(master_id) {
            if m.ai.target_creature.is_some() || m.last_hiter.is_some() {
                bo_change_face = true;
            }
        }
    }
    if bo_change_face {
        let (map_id, x, y) = match world.entities.get(actor_id) {
            Some(e) => (e.map_id, e.x, e.y),
            None => return,
        };
        world.pending_sculture_spawns.push(ScultureSpawnRequest {
            owner_id: actor_id,
            map_id,
            x,
            y,
            count: 1,
            kind: ClassSlaveSpawnKind::MakeClone {
                name: DRAGON1_NAME.to_string(),
                source_race: crate::entity::ACTOR_RACE_ELF_MONSTER,
            },
        });
    }
}

/// `ElfWarriorMonster.ResetElfMon`（`ElfWarriorMonster.cs:105-110`）：
/// `NextHitTime = 1500 - SlaveMakeLevel*100`、`WalkSpeed = 500 - SlaveMakeLevel*50`、
/// `WalkTick = now + 2000`。
fn elf_warrior_reset(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        let lvl = i64::from(e.slave_make_level);
        e.ai.next_hit_time = 1500 - lvl * 100;
        e.ai.walk_speed = 500 - lvl * 50;
        e.ai.walk_tick = now + 2000;
    }
}

/// `ElfWarriorMonster.Run`（`ElfWarriorMonster.cs:112-166`）。
///
/// 关键作用域：`else { DigDownTick = now; }` 属于 `if (boChangeFace)` 的 **else** ——
/// `boChangeFace=true` 但未到 60s / 名字尾位不是 `1` 时**不刷新** `DigDownTick`；
/// 只有 `boChangeFace=false`（有目标 / 主人有目标或 `LastHiter`）才刷新。
/// 改坏必红：`mb7_elf_warrior_dig_down_tick_not_refreshed_when_change_face_true`。
fn elf_warrior_run(world: &mut World, actor_id: i32) {
    let (bo_first, dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.bo_is_first, e.ai.dir, e.x, e.y),
        None => return,
    };
    if bo_first {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.bo_is_first = false;
            e.fixed_hide_mode = false;
        }
        send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
        elf_warrior_reset(world, actor_id);
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let now = world.clock_ms;
    if e.death {
        if now - e.death_tick > 2 * 1000 {
            make_ghost(world, actor_id);
        }
        return;
    }
    // `bool boChangeFace = TargetCret == null;`
    let mut bo_change_face = e.ai.target_creature.is_none();
    if let Some(master_id) = e.master {
        if let Some(m) = world.entities.get(master_id) {
            if m.ai.target_creature.is_some() || m.last_hiter.is_some() {
                bo_change_face = false;
            }
        }
    }
    if bo_change_face {
        let (dig_down_tick, name) = match world.entities.get(actor_id) {
            Some(e) => (e.dig_down_tick, e.name.clone()),
            None => return,
        };
        if now - dig_down_tick > 6 * 10 * 1000 {
            // `if (elfName[^1] == '1') { elfName = elfName[..^1]; elfMon = MakeClone(elfName, this); }`
            // C# `string` 的 `[^1]` 是 UTF-16 code unit；怪名均为 BMP ⇒ Rust `char` 等价。
            if let Some(trimmed) = name.strip_suffix('1') {
                let (map_id, x, y) = match world.entities.get(actor_id) {
                    Some(e) => (e.map_id, e.x, e.y),
                    None => return,
                };
                world.pending_sculture_spawns.push(ScultureSpawnRequest {
                    owner_id: actor_id,
                    map_id,
                    x,
                    y,
                    count: 1,
                    kind: ClassSlaveSpawnKind::MakeClone {
                        name: trimmed.to_string(),
                        source_race: crate::entity::ACTOR_RACE_ELF_WARRIORMON,
                    },
                });
            }
        }
    } else if let Some(e) = world.entities.get_mut(actor_id) {
        // `else { DigDownTick = GetTickCount(); }` —— **属于 `if (boChangeFace)` 的 else**。
        e.dig_down_tick = now;
    }
}

/// `ScultureKingMonster.Attack`（`:55-59`）：
/// `nPower = GetAttackPower(Lo(DC), |Hi(DC)-Lo(DC)|)`（1 抽）→ `HitMagAttackTarget(target, 0,
/// nPower, true)`：对**目标格**每个 proper 对象抽 `GetHitStruckDamage(this, 0)`（1~2 抽）与
/// `GetMagStruckDamage(this, nPower)`（1 抽），`sum > 0` ⇒ `StruckDamage(sum)` +
/// `SendStruckDelayMsg(RM_REFMESSAGE, …, 200)`；最后 `SendRefMsg(RM_HIT, …)`。
fn sculture_king_attack(world: &mut World, actor_id: i32, target_id: i32) {
    let Some((cx, cy, dc)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.dc)) else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_power = crate::combat::get_attack_power(&mut world.rng, lo, (hi - lo).abs());
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    // `Dir = GetNextDirection(CurrX, CurrY, target.CurrX, target.CurrY)`。
    let dir = crate::magic::next_direction(cx, cy, tx, ty);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let map_id = world.entities.get(actor_id).map_or(0, |e| e.map_id);
    let undead = world.entities.get(actor_id).map_or(0, |e| e.undead_power);
    // `Envir.GetBaseObjects(target.CurrX, target.CurrY, false, ...)`：**boFlag=false** 口径
    // （含「死但未 Ghost」；`base_objects_at` 是 true 口径）。MB-3b F4；与 G09 的关系见方法文档。
    for tid in world.base_objects_at_including_dead(map_id, tx, ty) {
        let proper = world
            .entities
            .get(actor_id)
            .zip(world.entities.get(tid))
            .is_some_and(|(a, t)| is_proper_target(a, t));
        if !proper {
            continue;
        }
        let mac = world.entities.get(tid).map_or(0, |t| t.mac);
        // `GetHitStruckDamage(this, 0)`：AC 骰照抽（`0 - nArmor` ⇒ 0），随后
        // `GetMagStruckDamage(this, nPower)`。
        let hit = {
            let target = world.entities.get(tid).expect("target exists");
            let attacker = world.entities.get(actor_id).expect("attacker exists");
            crate::combat::get_hit_struck_damage(&mut world.rng, target, attacker, 0)
        };
        let mag = crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_power, Some(undead));
        let n_damage = i32::from(hit) + mag;
        if n_damage > 0 {
            let _ = class_struck_apply(world, actor_id, tid, n_damage, n_power);
            // `baseObject.SendStruckDelayMsg(RM_REFMESSAGE, nDamage, …,
            //   ActorId, "", 200)`（`AnimalObject.cs:379`，ScultureKing 经
            //   `HitMagAttackTarget`）——每个 proper 格对象各一条。
            world.send_struck_delay_msg_refmessage(
                tid,
                actor_id,
                n_damage,
                HIT_MAG_STRUCK_DELAY_MS,
            );
        }
    }
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    send_ref_msg(world, actor_id, RM_HIT, i32::from(dir), x, y, 0);
}

/// `ScultureKingMonster.Run`（`:61-115`）。`base.Run()` = `MonsterObject.Run`。
fn sculture_king_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && now - e.ai.walk_tick >= e.ai.walk_speed) {
        return;
    }
    if e.stone_mode {
        let (cx, cy, cool_eye) = (e.x, e.y, e.ai.cool_eye);
        let visible: Vec<i32> = e.visible_actors.iter().map(|v| v.target).collect();
        for target_id in visible {
            let Some(t) = world.entities.get(target_id) else {
                continue;
            };
            if t.death {
                continue;
            }
            let proper = world
                .entities
                .get(actor_id)
                .is_some_and(|a| is_proper_target(a, t));
            if proper
                && (!t.hide_mode || cool_eye)
                && (cx - t.x).abs() <= 2
                && (cy - t.y).abs() <= 2
            {
                sculture_king_melt_stone(world, actor_id);
                break;
            }
        }
        // 融化之后 oracle 仍走 `_mSlaveObjectList` 清理（在 `if (StoneMode)` 之外）。
        cleanup_sculture_slaves(world, actor_id);
        return;
    }
    let (enemy_tick, has_target) = (e.ai.search_enemy_tick, e.ai.target_creature.is_some());
    if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.search_enemy_tick = now;
        }
        search_target(world, actor_id);
        // **C# `int / int` 整数除法**：`HP < MaxHP ⇒ HP/MaxHP = 0 ⇒ 阈值 0`（仅满血得 5）。
        let (danger, hp, max_hp) = world
            .entities
            .get(actor_id)
            .map_or((0, 0, 0), |e| (e.danger_level, e.hp, e.max_hp));
        let ratio = i32::from(hp) / i32::from(max_hp).max(1);
        if danger > ratio * 5 && danger > 0 {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.danger_level -= 1;
            }
            sculture_king_call_slave(world, actor_id);
        }
        let (hp_now, max_hp_now) = world
            .entities
            .get(actor_id)
            .map_or((0, 0), |e| (e.hp, e.max_hp));
        if hp_now == max_hp_now {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.danger_level = 5;
            }
        }
    }
    cleanup_sculture_slaves(world, actor_id);
}

/// `ScultureKingMonster.Run` 尾的 `_mSlaveObjectList` 倒序清理（`:105-112`）。
fn cleanup_sculture_slaves(world: &mut World, actor_id: i32) {
    let Some(mut list) = world
        .entities
        .get(actor_id)
        .map(|e| e.class_slave_list.clone())
    else {
        return;
    };
    if list.is_empty() {
        return;
    }
    for i in (0..list.len()).rev() {
        let remove = match world.entities.get(list[i]) {
            Some(child) => child.death || child.ghost,
            None => true,
        };
        if remove {
            list.remove(i);
        }
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.class_slave_list = list;
    }
}

/// `GuardUnit.IsProperTarget`（`GuardUnit.cs:35-125`）—— ArcherGuard 的 proper 判定。
///
/// `Castle != null` 分支（`GuardUnit.cs:37-100`）的等价物 = 实体带
/// [`crate::entity::CastleBinding`]（oracle `BaseObject.Castle != null`）：
/// 1. `LastHiter == target` ⇒ `true`；
/// 2. `Castle.UnderWar` ⇒ `true`（先于成员豁免/管理员/同城排除）；
/// 3. `Race == ActorRace.Guard(11)` 且 `CrimeforCastle` 未过期 ⇒ `true`；过期清位，
///    若该守卫自身 `Castle != null` ⇒ `false`；
/// 4. `Castle.MasterGuild != null` 时，目标（或其主人）是玩家且
///    `MasterGuild == MyGuild || MasterGuild.IsAllyGuild(MyGuild)` ⇒ 非 `LastHiter` 时 `false`；
/// 5. 管理员 / 石化 / `Race ∈ [NPC, Animal)` / 自己 / 同城 ⇒ `false`。
///
/// 之后落回尾段（`LastHiter`、弓箭守卫目标、红名玩家、石化）。`Castle == null` 的旧路径
/// 与改造前逐字节一致（判据 C5）。
///
/// 逐行实现见 [`guard_target_is_proper_target`]（本文件尾部）；本函数保持薄包装，
/// 以便 `check_anchors.py` 的 `monster_ai_tick` 行号锚不因新逻辑位移。
///
/// 判据：`guard_target_castle_tests` 的 C1..C8（每条带「改坏⇒红⇒sha256 还原」牙）。
///
///
/// # 参数
/// - `world`：世界（需要可变：到期时清 `CrimeforCastle`）；
/// - `actor_id`：发起判定的守卫（`ArcherGuard.Run` 的 `this`）；
/// - `target_id`：被判定为目标的实体。
fn guard_unit_is_proper_target(world: &mut World, actor_id: i32, target_id: i32) -> bool {
    guard_target_is_proper_target(world, actor_id, target_id)
}

/// `ArcherGuard.AttackTarger`（`ArcherGuard.cs:21-37`）。
fn archer_guard_attack_targer(world: &mut World, actor_id: i32, target_id: i32) {
    let (cx, cy, dc) = match world.entities.get(actor_id) {
        Some(e) => (e.x, e.y, e.dc),
        None => return,
    };
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    let dir = crate::magic::next_direction(cx, cy, tx, ty);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let mut n_damage = lo + world.rng.random_below((hi - lo).abs() + 1);
    if n_damage > 0 {
        let (Some(target), Some(attacker)) = (
            world.entities.get(target_id).cloned(),
            world.entities.get(actor_id).cloned(),
        ) else {
            return;
        };
        n_damage = i32::from(crate::combat::get_hit_struck_damage(
            &mut world.rng,
            &target,
            &attacker,
            n_damage,
        ));
    }
    if n_damage > 0 {
        // `targetObject.SetLastHiter(this); targetObject.ExpHitter = null;`（`ArcherGuard.cs:31-32`）
        // ——**在 `StruckDamage` 之前**（`:33`）：`StruckDamage` 的 `WarrMon`/`WizardMon`/`TaosMon`
        // 倍率读的是**当下**的 `LastHiter`，顺序反了会读到旧命中者（MB-3b F1b）。
        // 不发 `RM_STRUCK`（oracle 本路径没有 `_Attack` 的 `SendMsg`）。
        let now = world.clock_ms;
        if let Some(t) = world.entities.get_mut(target_id) {
            t.set_last_hiter(actor_id, now);
            t.exp_hitter = None;
        }
        let _ = class_struck_apply(world, actor_id, target_id, n_damage, n_damage);
        // `targetObject.SendStruckDelayMsg(RM_REFMESSAGE, nDamage, …,
        //   ActorId, "", max(|Δx|,|Δy|)*50 + 600)`（`ArcherGuard.cs:34`）。
        let delay =
            i64::from((cx - tx).abs().max((cy - ty).abs()) * 50) + GUARD_STRUCK_DELAY_BASE_MS;
        world.send_struck_delay_msg_refmessage(target_id, actor_id, n_damage, delay);
    }
    // `SendRefMsg(RM_FLYAXE, Dir, CurrX, CurrY, targetObject.ActorId, "")`。
    send_ref_msg_with_target(
        world,
        actor_id,
        RM_FLYAXE,
        i32::from(dir),
        cx,
        cy,
        target_id,
        Some(target_id),
    );
}

/// `ArcherGuard.Run`（`ArcherGuard.cs:39-91`）。`base.Run()` = `NormNpc.Run`
/// （清 Master）→ `AnimalObject.Run` → `BaseObject.Run`；调用方按
/// [`ClassRunBase::Animal`] 续跑，本函数在末尾清 Master。
fn archer_guard_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if can_move(e) && now - e.ai.walk_tick >= e.ai.walk_speed {
        let (cx, cy) = (e.x, e.y);
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.walk_tick = now;
        }
        let visible: Vec<i32> = world
            .entities
            .get(actor_id)
            .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
            .unwrap_or_default();
        let mut n_rage = 9999;
        let mut best = None;
        for target_id in visible {
            let Some((tx, ty, death)) = world.entities.get(target_id).map(|t| (t.x, t.y, t.death))
            else {
                continue;
            };
            if death {
                continue;
            }
            if !guard_unit_is_proper_target(world, actor_id, target_id) {
                continue;
            }
            let n_abs = (cx - tx).abs() + (cy - ty).abs();
            if n_abs < n_rage {
                n_rage = n_abs;
                best = Some(target_id);
            }
        }
        if let Some(target_id) = best {
            set_target_creature(world, actor_id, target_id);
        } else {
            del_target_creature(world, actor_id);
        }
    }
    // `ArcherGuard.Run` 的攻击/转向段仍在 `if (CanMove())` 内（只是不在 `WalkTick` 门内）。
    let can_move_now = world.entities.get(actor_id).is_some_and(can_move);
    let target = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature);
    if !can_move_now {
        if let Some(e) = world.entities.get_mut(actor_id) {
            if e.master.is_some() {
                e.master = None;
            }
        }
        return;
    }
    match target {
        Some(target_id) => {
            let due = world
                .entities
                .get(actor_id)
                .is_some_and(|e| now - e.ai.attack_tick >= e.ai.next_hit_time);
            if due {
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.attack_tick = now;
                }
                archer_guard_attack_targer(world, actor_id, target_id);
            }
        }
        None => {
            let guard_dir = world
                .entities
                .get(actor_id)
                .map_or(0, |e| e.guard_direction);
            let dir = world.entities.get(actor_id).map_or(0, |e| e.ai.dir);
            // `TurnTo` 的 `RM_TURN` 帧沿用本层 `turn_to` 口径（只落朝向，帧面登记未接）。
            if guard_dir > 0 && dir != guard_dir as u8 {
                turn_to(world, actor_id, guard_dir as u8);
            }
        }
    }
    // `NormNpc.Run`：`if (Master != null) Master = null;`（不进怪物 AI 的宝宝表）。
    if let Some(e) = world.entities.get_mut(actor_id) {
        if e.master.is_some() {
            e.master = None;
        }
    }
}

/// `MonsterObject.OpenCrazyMode(int nTime)`（`MonsterObject.cs:465-471`）——
/// `CrazyMode = true; CrazyModeTick = now; CrazyModeInterval = nTime * 1000; RefNameColor();`
/// （`RefNameColor` 是帧面，不在 world 层；`GetChrColor` 的 0xF9 支仍登记未实现）。
pub fn open_crazy_mode(world: &mut World, actor_id: i32, interval_seconds: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.crazy_mode = true;
        e.ai.crazy_mode_tick = now;
        // C# `int * int` 在 int 上计算；正常取值（`Random(20)+10`）远不溢出。
        e.ai.crazy_mode_interval = i64::from(interval_seconds) * 1000;
    }
}

/// `MonsterObject.BreakCrazyMode()`（`MonsterObject.cs:473-480`）：**只有** `CrazyMode` 为真才清
/// （`RefNameColor` 同属帧面，world 层不产帧）。
pub fn break_crazy_mode(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        if e.ai.crazy_mode {
            e.ai.crazy_mode = false;
        }
    }
}

/// `AnimalObject.BreakHolySeizeMode()`（`AnimalObject.cs:330-334`）：置 `HolySeize = false`
/// （`RefNameColor` 帧面不在 world 层；与既有 `attack_target`/`tame_adopt` 的写法同口径）。
pub fn break_holy_seize_mode(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.holy_seize = false;
    }
}

/// `MonsterObject.Run` 尾的 `CrazyMode` 到期段（`MonsterObject.cs:353-356`）。
///
/// **严格大于**：`(now - CrazyModeTick) > CrazyModeInterval`（等于不清）。只在 oracle 落到
/// `if (CanMove() && ...)` **之后**的路径上调用（`Think()`/`AttackTarget()` 的提前返回
/// 会 `base.Run(); return;` ⇒ 跳过本段，见 [`monster_run`]）。
fn crazy_mode_expiry(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let expired = world.entities.get(actor_id).is_some_and(|e| {
        e.ai.crazy_mode && (now - e.ai.crazy_mode_tick) > e.ai.crazy_mode_interval
    });
    if expired {
        break_crazy_mode(world, actor_id);
    }
}

/// `AnimalObject.Run()`（`AnimalObject.cs:142-149`）的到期段：
/// `if (HolySeize && (now - HolySeizeTick) > HolySeizeInterval) BreakHolySeizeMode();` 再 `base.Run()`。
///
/// **严格大于**（等于不清）。oracle 里 `MonsterObject.Run` 的每个出口都调 `base.Run()`
/// （= `AnimalObject.Run`），因此 [`monster_ai_tick`] 对每只怪在 [`monster_run`] 之后、
/// [`base_object_run_monster`] 之前调用本函数（顺序 = `CrazyMode` 到期 → HolySeize 到期 → `BaseObject.Run`）。
/// `NormNpc.Run`（守卫等）同样 `base.Run()` 进 `AnimalObject.Run` ⇒ 守卫分支也调用本函数。
pub fn animal_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let expired = world.entities.get(actor_id).is_some_and(|e| {
        e.ai.holy_seize && (now - e.ai.holy_seize_tick) > e.ai.holy_seize_interval
    });
    if expired {
        break_holy_seize_mode(world, actor_id);
    }
}

/// `BaseObject.GetBackPosition(ref nX, ref nY)`（`BaseObject.cs:1878-1927`）：按**主人**的
/// `Dir` 取身后一格，并在主人所在地图的 `Width/Height` 边界内夹取。
///
/// 方向语义照 `Direction`（`src/OpenMir2/Enums/Direction.cs`）：
/// `Up=0` ⇒ `nY++`、`Down=4` ⇒ `nY--`、`Left=6` ⇒ `nX++`、`Right=2` ⇒ `nX--`；
/// 对角方向**两轴同时**满足边界才一起动（oracle 的 `&&` 门，照抄）。
/// **无随机数、无移动**。主人不在实体表（悬空 id）⇒ `None`（等价 oracle 的 `Master == null`）。
fn master_get_back_position(world: &World, master_id: i32) -> Option<(i32, i32)> {
    let m = world.entities.get(master_id)?;
    Some(back_position_from(world, m.map_id, m.x, m.y, m.ai.dir))
}

/// `BaseObject.GetBackPosition(ref nX, ref nY)` 的**自身**版本（`BaseObject.cs:1878-1927`）：
/// 按自己的 `Dir` 取身后一格，并在自己所在地图的 `Width/Height` 内夹取。
///
/// 与 [`master_get_back_position`] 共用同一份方向/边界语义（后者只是「按主人的状态取」）。
/// MB-2 的 `DualAxeMonster`/`LightingZombi` 用它写 `TargetX/TargetY`（`DualAxeMonster.cs:126/131`、
/// `LightingZombi.cs:48`）。
fn get_back_position(world: &World, actor_id: i32) -> Option<(i32, i32)> {
    let e = world.entities.get(actor_id)?;
    Some(back_position_from(world, e.map_id, e.x, e.y, e.ai.dir))
}

/// `GetBackPosition` 的纯函数核心：给定地图/坐标/朝向，返回身后一格（含边界夹取）。
///
/// **无随机数、无移动**。对角方向两轴同时满足边界才一起动（oracle 的 `&&` 门，照抄）。
fn back_position_from(world: &World, map_id: usize, x: i32, y: i32, dir: u8) -> (i32, i32) {
    let (mut n_x, mut n_y) = (x, y);
    let (w, h) = world
        .maps
        .get(map_id)
        .map_or((0, 0), |map| (map.width, map.height));
    match dir {
        DIR_UP if n_y < h - 1 => n_y += 1,
        DIR_DOWN if n_y > 0 => n_y -= 1,
        DIR_LEFT if n_x < w - 1 => n_x += 1,
        DIR_RIGHT if n_x > 0 => n_x -= 1,
        DIR_UP_LEFT if n_x < w - 1 && n_y < h - 1 => {
            n_x += 1;
            n_y += 1;
        }
        DIR_UP_RIGHT if n_x < w - 1 && n_y > 0 => {
            n_x -= 1;
            n_y += 1;
        }
        DIR_DOWN_LEFT if n_x > 0 && n_y < h - 1 => {
            n_x += 1;
            n_y -= 1;
        }
        DIR_DOWN_RIGHT if n_x > 0 && n_y > 0 => {
            n_x -= 1;
            n_y -= 1;
        }
        _ => {}
    }
    (n_x, n_y)
}

// ---------------------------------------------------------------------------
// **MB-4**（R10）——`CowKingMonster`（race 92）：30s 跳跃/狂暴状态机 + `Attack` 覆写。
// ---------------------------------------------------------------------------

/// 30s 跳跃门（`CowKingMonster.cs:42`，严格 `>`）。
const COW_KING_JUMP_INTERVAL_MS: i64 = 30 * 1000;
/// `CrazyReady` 段长（`:70`，严格 `<`）。
const COW_KING_READY_MS: i64 = 8 * 1000;
/// `CrazyKing` 段长（`:81`，严格 `<`）。
const COW_KING_KING_MS: i64 = 8 * 1000;
/// `CrazyReadyMode` 期间的 `NextHitTime`（`:72`）。
const COW_KING_READY_HIT_TIME: i64 = 10_000;
/// `CrazyKingMode` 期间的 `NextHitTime`（`:83`）。
const COW_KING_KING_HIT_TIME: i64 = 500;
/// `CrazyKingMode` 期间的 `WalkSpeed`（`:84`）。
const COW_KING_KING_WALK_SPEED: i64 = 400;
/// `SiegeLockCount()` 的进入门（`:47`）。
const COW_KING_SIEGE_LOCK_THRESHOLD: i32 = 5;

/// `CowKingMonster.SiegeLockCount()`（`CowKingMonster.cs:96-118`）。
///
/// **oracle 无持久字段**：每次 `Run` 现算 3×3 邻域（`nC/n10 ∈ {-1,0,1}`）里
/// `Envir.CanWalk(CurrX+nC, CurrY+n10, false)` 为假的格数，**中心格不计**
/// （`:110` 的 `(nC != 0) || (n10 != 0)`）。`CanWalk(..., false)` 的含义见
/// [`World::can_walk_at`]：越界/墙格为假；格上 `!Ghost && !Death && !FixedHideMode && !ObMode`
/// 的活体也算阻挡。
///
/// 不抽随机数、不写状态。
fn cow_king_siege_lock_count(world: &World, actor_id: i32) -> i32 {
    let Some(e) = world.entities.get(actor_id) else {
        return 0;
    };
    let (cx, cy, map_id) = (e.x, e.y, e.map_id);
    let mut result = 0;
    let mut n_c = -1;
    while n_c != 2 {
        let mut n10 = -1;
        while n10 != 2 {
            if !world.can_walk_at(map_id, cx + n_c, cy + n10, false) && (n_c != 0 || n10 != 0) {
                result += 1;
            }
            n10 += 1;
        }
        n_c += 1;
    }
    result
}

/// `CowKingMonster.Run`（`CowKingMonster.cs:40-94`）。`base.Run()` 由调用方按
/// [`ClassRunBase::Monster`] 续跑。
///
/// 逐句：
/// 1. `!Death && !Ghost && (now - JumpTime) > 30s` —— 严格 `>`，进门先刷新 `JumpTime`；
/// 2. `TargetCret != null && SiegeLockCount() >= 5` ⇒ `TargetCret.GetBackPosition` 的格子
///    `CanWalk(...,false)` ⇒ `SpaceMove(Envir.MapName, x, y, 0)`，否则 `MapRandomMove`；
///    **两支都 `return`**（跳过 Crazy 段与 `base.Run()`）；
/// 3. `CrazyCount = 7 - HP / (MaxHP / 7)` —— **C# `int/int` 两次整除**；
/// 4. `CrazyCount >= 2 && CrazyCount != oldCrazyCount` ⇒ 进 `CrazyReadyMode`；
/// 5. Ready 段 `< 8s` ⇒ `NextHitTime = 10000`；到期 ⇒ 清 Ready、进 King、`CrazyTime = now`
///    （同一调用内随后立即跑 King 段 ⇒ 立刻写 `500/400`）；
/// 6. King 段 `< 8s` ⇒ `NextHitTime = 500`、`WalkSpeed = 400`；到期 ⇒ 清 King 并恢复
///    `oldHitTime/oldWalkTime`（`Initialize()` 保存的 **DB** 值）。
///
/// **取数**：本函数自身零抽。`SpaceMove` 在落点已由 `CanWalk` 验证可走时
/// `SpaceMoveGetRandXY` 首轮即成功 ⇒ 零抽；`MapRandomMove` 恰好 2 抽
/// （`merchant::map_random_move_xy`），落点不可走时再进 `space_move` 移位环。
///
/// 返回 `true` = oracle 在 `Run` 体内 `return`（Siege 两支）⇒ 调用方不得续跑 `base.Run()`。
fn cow_king_run(world: &mut World, actor_id: i32) -> bool {
    let now = world.clock_ms;
    let Some(e) = world.entities.get(actor_id) else {
        return false;
    };
    if e.death || e.ghost || now - e.jump_time <= COW_KING_JUMP_INTERVAL_MS {
        return false;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.jump_time = now;
    }
    // `TargetCret != null`：悬空 id 在本骨架等价于 `null`（oracle 的强引用不可能悬空）。
    let target_id = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
        .filter(|tid| world.entities.get(*tid).is_some());
    if let Some(tid) = target_id {
        if cow_king_siege_lock_count(world, actor_id) >= COW_KING_SIEGE_LOCK_THRESHOLD {
            // `TargetCret.GetBackPosition(ref nX, ref nY)`（边界按**目标**图夹取）。
            let (n_x, n_y) = get_back_position(world, tid).unwrap_or((0, 0));
            let map_id = world.entities.get(actor_id).map_or(0, |e| e.map_id);
            // `Envir.CanWalk(nX, nY, false)` —— `Envir` 是**自己**的地图。
            if world.can_walk_at(map_id, n_x, n_y, false) {
                crate::space_move::monster_space_move(world, actor_id, map_id, n_x, n_y);
                return true;
            }
            crate::merchant::map_random_move(world, actor_id);
            return true;
        }
    }
    let (hp, max_hp) = world
        .entities
        .get(actor_id)
        .map_or((0, 0), |e| (e.hp, e.max_hp));
    let old_crazy_count = world.entities.get(actor_id).map_or(0, |e| e.crazy_count);
    // **C# `int/int` 整除**：`MaxHP / 7` 先截断，再 `HP / (MaxHP / 7)`。
    // `MaxHP < 7` 时 oracle 会抛 `DivideByZeroException`（部署无此怪）；本实现用
    // `max(1)` 避免 panic（等价的 crash 不复刻，登记在报告）。
    let base = i32::from(max_hp) / 7;
    let crazy_count = 7 - i32::from(hp) / base.max(1);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.crazy_count = crazy_count;
    }
    if crazy_count >= 2 && crazy_count != old_crazy_count {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.crazy_ready_mode = true;
            e.crazy_ready = now;
        }
    }
    let (ready, ready_tick) = world
        .entities
        .get(actor_id)
        .map_or((false, 0), |e| (e.crazy_ready_mode, e.crazy_ready));
    if ready {
        if now - ready_tick < COW_KING_READY_MS {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.next_hit_time = COW_KING_READY_HIT_TIME;
            }
        } else {
            // `CrazyReadyMode = false; CrazyKingMode = true; CrazyTime = GetTickCount();`
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.crazy_ready_mode = false;
                e.crazy_king_mode = true;
                e.crazy_time = now;
            }
        }
    }
    let (king, king_tick) = world
        .entities
        .get(actor_id)
        .map_or((false, 0), |e| (e.crazy_king_mode, e.crazy_time));
    if king {
        if now - king_tick < COW_KING_KING_MS {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.next_hit_time = COW_KING_KING_HIT_TIME;
                e.ai.walk_speed = COW_KING_KING_WALK_SPEED;
            }
        } else {
            // `CrazyKingMode = false; NextHitTime = oldHitTime; WalkSpeed = oldWalkTime;`
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.crazy_king_mode = false;
                e.ai.next_hit_time = e.old_hit_time;
                e.ai.walk_speed = e.old_walk_time;
            }
        }
    }
    false
}

/// `MonsterObject.AttackTarget` 的 race 92 版本：门/状态写点与 [`attack_target`] 相同，
/// 只把到点后的虚调用 `Attack(target, btDir)` 换成 [`cow_king_attack`]
/// （`CowKingMonster.cs:27-31`）。
fn cow_king_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let now = world.clock_ms;
    if let Some(bt_dir) = get_attack_dir(world, actor_id, target_id) {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
                e.ai.holy_seize = false;
            }
            turn_to(world, actor_id, bt_dir);
            cow_king_attack(world, actor_id, target_id);
        }
        true
    } else {
        let (same_map, tx, ty) = match (
            world.entities.get(actor_id).map(|e| e.map_id),
            world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
        ) {
            (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
            _ => (false, 0, 0),
        };
        if same_map {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.target_x = tx;
                e.ai.target_y = ty;
            }
        } else {
            del_target_creature(world, actor_id);
        }
        false
    }
}

/// `CowKingMonster.Attack`（`CowKingMonster.cs:27-31`）：
/// `nPower = GetAttackPower(LoByte(DC), Math.Abs(HiByte(DC) - LoByte(DC)))`
/// （第二参是 **`Abs`**，不是 `(sbyte)` 强转）→ `HitMagAttackTarget(target, nPower/2, nPower/2, true)`
/// （`AnimalObject.cs:361-383`：目标格 `GetBaseObjects(..., false)` 逐对象
/// `GetHitStruckDamage(nPower/2) + GetMagStruckDamage(nPower/2)` → `StruckDamage` +
/// 200ms `RM_REFMESSAGE`；尾帧 `RM_HIT`）。
fn cow_king_attack(world: &mut World, actor_id: i32, target_id: i32) {
    let Some((cx, cy, dc)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.dc)) else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    // **1 抽**：`Random(|Hi-Lo| + 1)`。
    let n_power = crate::combat::get_attack_power(&mut world.rng, lo, (hi - lo).abs());
    let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) else {
        return;
    };
    // `Dir = M2Share.GetNextDirection(CurrX, CurrY, target.CurrX, target.CurrY)`。
    let dir = crate::magic::next_direction(cx, cy, tx, ty);
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let map_id = world.entities.get(actor_id).map_or(0, |e| e.map_id);
    let undead = world.entities.get(actor_id).map_or(0, |e| e.undead_power);
    // `Envir.GetBaseObjects(target.CurrX, target.CurrY, false, ...)`：boFlag=false 口径
    // （含「死但未 Ghost」）——复用与 ScultureKing 相同的格扫。
    for tid in world.base_objects_at_including_dead(map_id, tx, ty) {
        let proper = world
            .entities
            .get(actor_id)
            .zip(world.entities.get(tid))
            .is_some_and(|(a, t)| is_proper_target(a, t));
        if !proper {
            continue;
        }
        let mac = world.entities.get(tid).map_or(0, |t| t.mac);
        // `GetHitStruckDamage(this, nPower / 2)`：AC 骰照抽（即使 nPower/2 == 0）。
        let hit = {
            let target = world.entities.get(tid).expect("target exists");
            let attacker = world.entities.get(actor_id).expect("attacker exists");
            crate::combat::get_hit_struck_damage(&mut world.rng, target, attacker, n_power / 2)
        };
        // `GetMagStruckDamage(this, nPower / 2)`：MAC 骰照抽。
        let mag =
            crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_power / 2, Some(undead));
        let n_damage = i32::from(hit) + mag;
        if n_damage > 0 {
            let _ = class_struck_apply(world, actor_id, tid, n_damage, n_power);
            // `SendStruckDelayMsg(RM_REFMESSAGE, nDamage, HP, MaxHP, ActorId, "", 200)`。
            world.send_struck_delay_msg_refmessage(
                tid,
                actor_id,
                n_damage,
                HIT_MAG_STRUCK_DELAY_MS,
            );
        }
    }
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    send_ref_msg(world, actor_id, RM_HIT, i32::from(dir), x, y, 0);
}

/// `MonsterObject.Run`（`MonsterObject.cs:251-360`）的**普通怪路径**（任务/逃跑触发点未建模；
/// `Master` 跟随 / `SlaveRelax` 已接）。
///
/// 只做 AI；`BaseObject.Run` 的恢复/死亡/过期段由 `World` 统一处理（见模块头）。
pub fn monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    // `ATMonster.Run`（`ATMonster.cs:13-24`）：`is_active_seek_race` 的怪在 `base.Run()`
    // （= `MonsterObject.Run`，本函数余下部分）之前先按时钟门索敌。外层门是 `CanMove()`
    // （`AnimalObject.cs:162-165`，= [`can_move`]）；门未到 ⇒ 不搜不抽。
    // `SearchTarget` 本身**不消费随机数**（见其文档）⇒ 只改目标态、不移位 RNG 流水。
    //
    // **MB-3b F2**：`GasMothMonster`(105) `: GasAttackMonster : AtMonster` 覆写了 `Run`
    // （`GasMothMonster.cs:26-36`：自己的 `GasMothAttack` 后 `base.Run()`），故不能进
    // [`is_active_seek_race`] 集合（那代表「未覆写 `Run`、直接由本函数驱动」的怪），但它的
    // `base.Run()` = `ATMonster.Run`，**这道 8s/1s 索敌门同样存在**。外层 `GasMothMonster.Run`
    // 过门时会先把 `SearchEnemyTick` 刷成 now ⇒ 本段随后自然为假；只有 WalkSpeed 门挡住外层
    // 时才由本段补跑 `SearchTarget`（与 oracle 同点同次）。最小改法：条件里显式加 105。
    //
    // **MB-4 F1**：`CowKingMonster`(92) `: AtMonster` 同样覆写 `Run`（`CowKingMonster.cs:40-94`），
    // 末尾 `base.Run()`（`:93`）= `ATMonster.Run` ⇒ 同一道索敌门。两个 `return` 支（Siege 位移）
    // 走 [`ClassRunBase::None`]、根本不进本函数，故这里补门与 oracle 的「两次 return 之后才
    // base.Run()」逐点一致。`SearchTarget` 不消费随机数 ⇒ 五批 trace 不受影响。
    // **MB-7 F1**：`ElfWarriorMonster`(114) `: SpitSpider : AtMonster` 亦覆写 `Run`，
    // 末尾 `base.Run()` = `ATMonster.Run` ⇒ 同一道 8s/1s 索敌门（与 105/92 同法补门）。
    // **MB-8（R42）**：`WhiteSkeleton(100) : AtMonster` 覆写 `Run`，其 `base.Run()` =
    // `ATMonster.Run` ⇒ 同一道 8s/1s 索敌门（与 105/92/114 同法补门）。
    if (is_active_seek_race(e.race)
        || e.race == 105
        || e.race == 92
        || e.race == 114
        || e.race == 100)
        && can_move(e)
    {
        let now = world.clock_ms;
        let enemy_tick = e.ai.search_enemy_tick;
        let has_target = e.ai.target_creature.is_some();
        // `(now - SearchEnemyTick) > 8000 || (now - SearchEnemyTick) > 1000 && TargetCret == null`
        // ——C# 的 `&&` 优先级高于 `||`，故第二项是 `(>1000) && (TargetCret == null)`。
        if now - enemy_tick > 8000 || (now - enemy_tick > 1000 && !has_target) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.search_enemy_tick = now;
            }
            search_target(world, actor_id);
        }
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !(can_move(e) && !e.fixed_hide_mode && !e.stone_mode) {
        // oracle：外层 `if (CanMove() && ...)` 为假 ⇒ 落到函数尾的 `CheckRoyalty()`
        // （`MonsterObject.cs:352`）与 `CrazyMode` 到期（`:353-356`）。
        crate::slave::check_royalty(world, actor_id);
        crazy_mode_expiry(world, actor_id);
        return;
    }
    // `MonsterObject.Run`（Pass 2）的 `Think()` 静态绑定 `MonsterObject.Think`（**有**门）：
    // 无门版只属于 `MagicMonster.Run`（Pass 1，见 [`magic_monster_run`]）。
    if think(world, actor_id) {
        // oracle `:255-258`：`if (Think()) { base.Run(); return; }` ⇒ **跳过** `CheckRoyalty`。
        return;
    }
    let now = world.clock_ms;
    // `if (WalkWaitLocked) { if (now - WalkWaitTick) > WalkWait) WalkWaitLocked = false; }`
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.walk_wait_locked)
        && world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.walk_wait_tick > e.ai.walk_wait)
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.walk_wait_locked = false;
        }
    }
    let walk_due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| !e.ai.walk_wait_locked && (now - e.ai.walk_tick) > e.ai.walk_speed);
    if !walk_due {
        // oracle：`if (!WalkWaitLocked && (now - WalkTick) > WalkSpeed)` 为假 ⇒ 落到
        // `CheckRoyalty()` 与 `CrazyMode` 到期（`MonsterObject.cs:352-356`）。
        crate::slave::check_royalty(world, actor_id);
        crazy_mode_expiry(world, actor_id);
        return;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
        e.ai.walk_count += 1;
        if e.ai.walk_count > e.ai.walk_step {
            e.ai.walk_count = 0;
            e.ai.walk_wait_locked = true;
            e.ai.walk_wait_tick = now;
        }
    }
    if !world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.run_away_mode)
    {
        if !world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.no_attack_mode)
        {
            let target = world
                .entities
                .get(actor_id)
                .and_then(|e| e.ai.target_creature);
            match target {
                Some(_) => {
                    // `AttackTarget()` 是**虚调用**：MB-2 起按 race 分派
                    // （`DualAxeMonster` 87/93/104 覆写）。
                    if monster_attack_target(world, actor_id) {
                        return;
                    }
                }
                None => {
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        // `MonsterObject.cs:296-301`：无目标且 `Mission == false` ⇒ `TargetX = -1`。
                        e.ai.target_x = -1;
                    }
                }
            }
        }
        monster_follow_master(world, actor_id);
    } else {
        monster_run_away_expiry(world, actor_id, now);
    }
    // `MonsterObject.cs:333-338`：`Master != null && Master.SlaveRelax` ⇒ `base.Run(); return;`
    // —— **跳过** `GotoTargetXY`/`Wondering`、`CheckRoyalty()` 与 `CrazyMode` 到期。
    // 本骨架的 `base.Run()` 由 [`monster_ai_tick`] 在 [`monster_run`] 之后统一调用
    // （`animal_run` + `base_object_run_monster`），故这里只需提前返回。
    let slave_relax = world
        .entities
        .get(actor_id)
        .and_then(|e| e.master)
        .and_then(|m| world.entities.get(m))
        .is_some_and(|m| m.slave_relax);
    if slave_relax {
        return;
    }
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.target_x != -1)
    {
        goto_target_xy(world, actor_id);
    } else {
        let has_target = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.target_creature.is_some());
        if !has_target {
            wondering(world, actor_id);
        }
    }
    // `MonsterObject.Run` 尾的 `CheckRoyalty()`（`MonsterObject.cs:352`）。
    // 注：oracle 的 `AttackTarget()` 命中支（`:277-283`）与 `Master.SlaveRelax` 支（`:333-338`）
    // 会 `base.Run(); return;` 跳过本调用；两条支都已在上面提前返回（与 oracle 同）。
    crate::slave::check_royalty(world, actor_id);
    // `MonsterObject.cs:353-356`：CrazyMode 到期（在 `CheckRoyalty()` 之后、`base.Run()` 之前）。
    crazy_mode_expiry(world, actor_id);
}

/// `MonsterObject.Run` / `MagicMonster.Run` 共用的 `Master != null` 跟随段
/// （`MonsterObject.cs:299-329` 与 `MagicMonster.cs:110-148` **逐字相同**）。
///
/// oracle 逐句：
///   1. `TargetCret == null` ⇒ `Master.GetBackPosition(ref nX, ref nY)`；若
///      `|TargetX-nX| > 1 || |TargetY-nY| > 1` ⇒ `TargetX/TargetY = nX/nY`；
///      再若 `|CurrX-nX| <= 2 && |CurrY-nY| <= 2` 且该格 `GetMovingObject(nX,nY,true) != null`
///      ⇒ `TargetX/TargetY = CurrX/CurrY`（身后格被占时原地待命）。
///   2. `!Master.SlaveRelax && (Envir != Master.Envir || |Δx| > 20 || |Δy| > 20)`
///      ⇒ `SpaceMove(Master.Envir.MapName, TargetX, TargetY, 1)`。
///
/// `TargetCret` 可能在上一段 `AttackTarget()` 里被 `DelTargetCreat()` 清空 ⇒ **重新读**。
fn monster_follow_master(world: &mut World, actor_id: i32) {
    // `MonsterObject.cs:299-329`：`Master != null` 的跟随段（仍在 `!RunAwayMode` 内）。
    //
    // oracle 逐句：
    //   1. `TargetCret == null` ⇒ `Master.GetBackPosition(ref nX, ref nY)`；若
    //      `|TargetX-nX| > 1 || |TargetY-nY| > 1` ⇒ `TargetX/TargetY = nX/nY`；
    //      再若 `|CurrX-nX| <= 2 && |CurrY-nY| <= 2` 且该格 `GetMovingObject(nX,nY,true) != null`
    //      ⇒ `TargetX/TargetY = CurrX/CurrY`（身后格被占时原地待命）。
    //   2. `!Master.SlaveRelax && (Envir != Master.Envir || |Δx| > 20 || |Δy| > 20)`
    //      ⇒ `SpaceMove(Master.Envir.MapName, TargetX, TargetY, 1)`。
    // `TargetCret` 可能在上一段 `AttackTarget()` 里被 `DelTargetCreat()` 清空 ⇒ **重新读**。
    let master_id = world.entities.get(actor_id).and_then(|e| e.master);
    if let Some(master_id) = master_id {
        let has_target = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.target_creature.is_some());
        if !has_target {
            if let Some((n_x, n_y)) = master_get_back_position(world, master_id) {
                let (tx, ty) = world
                    .entities
                    .get(actor_id)
                    .map_or((-1, 0), |e| (e.ai.target_x, e.ai.target_y));
                if (tx - n_x).abs() > 1 || (ty - n_y).abs() > 1 {
                    let (cx, cy, map_id) = world
                        .entities
                        .get(actor_id)
                        .map_or((0, 0, 0), |e| (e.x, e.y, e.map_id));
                    let mut new_x = n_x;
                    let mut new_y = n_y;
                    if (cx - n_x).abs() <= 2 && (cy - n_y).abs() <= 2 {
                        // `Envir.GetMovingObject(nX, nY, true)`（`Envirnoment.cs:1203`）。
                        // 本仓 [`World::moving_object_at`] 是其逐行移植（含 `!Ghost`/`!Death`
                        // 门）；oracle 的 `CastleDoor.HoldPlace` 特例未建模（登记，见报告）。
                        if world.moving_object_at(map_id, n_x, n_y, true).is_some() {
                            new_x = cx;
                            new_y = cy;
                        }
                    }
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        e.ai.target_x = new_x;
                        e.ai.target_y = new_y;
                    }
                }
            }
        }
        // `SpaceMove` 的目标图 = `Master.Envir.MapName`（本仓以 `map_id` 表示图）。
        // oracle 的该门在 `if (TargetCret == null)` **之外** ⇒ 有目标时同样会追随。
        // `SlaveRelax` 为真时**不**追随（但上面的 TargetX/TargetY 仍照 oracle 更新）。
        let master = world
            .entities
            .get(master_id)
            .map(|m| (m.map_id, m.x, m.y, m.slave_relax));
        let own = world
            .entities
            .get(actor_id)
            .map(|e| (e.map_id, e.x, e.y, e.ai.target_x, e.ai.target_y));
        if let (Some((m_map, m_x, m_y, m_relax)), Some((s_map, s_x, s_y, tx, ty))) = (master, own) {
            if !m_relax && (s_map != m_map || (s_x - m_x).abs() > 20 || (s_y - m_y).abs() > 20) {
                // `SpaceMove(..., nInt=1)` ⇒ `RM_SPACEMOVE_SHOW2`（怪物无会话；出帧面登记）。
                let _ = crate::space_move::monster_space_move(world, actor_id, m_map, tx, ty);
            }
        }
    }
}

/// `MonsterObject.Run` / `MagicMonster.Run` 共用的 `RunAwayMode` 到期段
/// （`MonsterObject.cs:330-345` / `MagicMonster.cs:149-158` 逐字相同；**严格大于**）。
fn monster_run_away_expiry(world: &mut World, actor_id: i32, now: i64) {
    let expired = world.entities.get(actor_id).is_some_and(|e| {
        e.ai.run_away_time > 0 && (now - e.ai.run_away_start) > e.ai.run_away_time
    });
    if expired {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.run_away_mode = false;
            e.ai.run_away_time = 0;
        }
    }
}

/// `BaseObject.Run()`（`BaseObject.Base.cs:24-316`）的**怪侧公共段**：恢复 / 体力点 / `TargetCret` 清理。
///
/// oracle 里这三段在 `monster.Run()` 的**末尾**执行：`MonsterObject.Run` 的两个出口
/// （`Think()` 命中 `MonsterObject.cs:255-258` 与普通出口 `:359`）都调 `base.Run()`；
/// 因此 [`monster_ai_tick`] 对每只怪先跑 [`monster_run`]（AI）再跑本函数，与 oracle 同序。
///
/// 覆盖的段（行号 = `src/M2Server/Actor/BaseObject.Base.cs`）：
/// - 恢复段 `:39-66`：`(now - AutoRecoveryTick) / 20` 折成 `HealthTick/SpellTick`；
///   `HP < MaxHP` 且 `HealthTick >= HealthFillTime` ⇒ `HP += MaxHP / 75 + 1`（截断到 `MaxHP`）；
///   `MP < MaxMP` 且 `SpellTick >= SpellFillTime` ⇒ `MP += MaxMP / 18 + 1`；到点后两个 tick 归零。
///   怪物的 `HealthSpellChanged()` 只对 `Race == ActorRace.Play` 发帧（`BaseObject.cs:700-703`）
///   ⇒ **怪侧无帧面**（不产生任何 `SM_*`）。
/// - 体力点段 `:105-110`：`HealthTick < -HealthFillTime && HP > 1` ⇒ `HP -= 1; HealthTick += HealthFillTime`。
///   普通怪的 `HealthTick` 只增不减⇒该支对怪物不可达；照抄是为了同一份 `BaseObject.Run` 语义
///   （将来玩家/消费点接进来时不需要再改）。
/// - `TargetCret` 清理段 `:111-118`：`now - TargetFocusTick > 30000 || TargetCret.Death ||
///   TargetCret.Ghost || 异图 || |Δx| > 15 || |Δy| > 15` ⇒ `ClearTargetCreat`。
///
/// **本函数不消费随机数**（恢复/体力点/过期都是纯状态演进）。
///
/// **已建模**：`SuperMan`（`:32-36`，消息泵之后、`!Death` 之前回满；本函数覆盖
/// race 11/12/20/112/110/111 与 race 120，固定 NPC/商人在各自 `Run` 等价点回满；
/// 直接 Die / 伤害路径与 T-31 残余见 `World::settle_deaths`）；
/// **未建模（登记，见模块头/证据）**：`HP == 0 ⇒ Die()`（`:70-73`，
/// 由 [`crate::world::World`] 的 `settle_deaths` 在同一 tick 的死亡结算里等价处理；本段的恢复
/// 只增 HP、体力点段有 `HP > 1` 门⇒此处 `HP == 0` 不可达）、`MakeGhost`（`:86-101`，需
/// `MonGen.ZenTime`/`Config.MakeGhostTime`）、`BaseObject.Run` 的 Master 变色支（`:133-215`）、
/// `ShowHp`/`VerifyTick`（`:217-256`）。**⑨-3 已接**：宝宝列表清表（`:208-217`）。
///
/// 锚点复核于 2026-10-13。
/// 改坏必红：把 `HealthFillTime` 门（或 `HP > 1` 门）拆掉⇒`base_recovery_*`/
/// `health_tick_negative_cost_is_clamped` 必红（见 `tests/parity/world/monster-ai-host-20261013`）。
pub fn base_object_run_monster(world: &mut World, actor_id: i32) {
    // `BaseObject.Run` 的**消息段**（`BaseObject.Base.cs:26-30`）按 FIFO 出队。本骨架有两条队列：
    // `RM_UPDATEVIEWRANGE`（上一 tick 玩家扫描末尾投递）与 `RM_STRUCK`（本 tick 攻击入队）。
    // oracle 只有一条 `MsgQueue`；view update 的入队时刻严格早于本 tick 的战损消息 ⇒ 先消费
    // view update 与 oracle FIFO 同序。两者消费都不取随机数，也不读对方写的字段 ⇒ 即使交错也无状态差。
    let now_ms = world.clock_ms;
    let pending_views: Vec<i32> = match world.entities.get_mut(actor_id) {
        Some(e) => std::mem::take(&mut e.ai.pending_view_updates),
        None => return,
    };
    for viewer_id in pending_views {
        let _ =
            crate::aoi::update_monster_visible(&mut world.entities, actor_id, viewer_id, now_ms);
    }
    // 再把入队的 `RM_STRUCK` 出队，
    // 逐个 `Operate(RM_STRUCK) → Struck`（`AnimalObject.cs:118-124`）。本骨架用
    // [`crate::entity::AnimalState::pending_struck`] 承载队列（入队处 = `World::attack_once`）。
    let pending: Vec<crate::entity::StruckMsg> = match world.entities.get_mut(actor_id) {
        Some(e) => std::mem::take(&mut e.ai.pending_struck),
        None => return,
    };
    // `CloneMonster.Operate` 覆写了 `Operate(RM_STRUCK)`（`Clone.cs:14-30`）：
    // 恒 `return false` ⇒ `AnimalObject.Operate` 的 `SetLastHiter`/`Struck` 都不执行；
    // 只做 `Master.WAbil.MP -= (ushort)wParam`。普通怪照旧。
    let is_clone = world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.race == crate::special_monster::RACE_CLONE_MONSTER);
    for msg in pending {
        if is_clone {
            crate::special_monster::clone_operate(
                world,
                actor_id,
                crate::special_monster::RM_STRUCK,
                msg.w_param,
                0,
            );
            continue;
        }
        // oracle `if (processMsg.ActorId == ActorId && struckObject != null)`：命中者已不在场 ⇒ 跳过。
        if world.entities.get(msg.hiter).is_some() {
            animal_struck(world, actor_id, msg.hiter);
        }
    }
    // `BaseObject.Base.cs:32-36`：`if (SuperMan) { HP = MaxHP; MP = MaxMP; }`（在 `!Death` 之前）。
    // **置位口径**：`mongen::apply_monster_class_constants` 给 NormNpc 派生 race 置位
    // （守卫 11/12/20/112、城门 110、城墙 111 = `NormNpc.cs:41` 的 ctor），race 120 由
    // `SoccerBall` ctor 置位；固定 NPC（`npc::insert_npc`）与商人（`merchant::insert_merchant`）
    // 的 `NormNpc.Run → base.Run` 等价点各自调用 [`Entity::run_super_man_refill`]。
    // `Die()` 首行早退（`BaseObject.Base.cs:318-322`）的等价物在 `World::settle_deaths`。
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.run_super_man_refill();
    }
    let now = world.clock_ms;
    let state = world.entities.get(actor_id).map(|e| {
        (
            e.death,
            e.can_re_alive,
            e.mon_gen.is_some(),
            e.mon_gen_zen_time,
            e.death_tick,
        )
    });
    let Some((is_death, can_re_alive, has_mon_gen, zen_time, death_tick)) = state else {
        return;
    };
    if is_death {
        // 死亡分支（`BaseObject.Base.cs:85-101`）：
        //
        //   if (CanReAlive && MonGen != null) {
        //       int makeGhostTime = MAX(10s, GetMonstersZenTime(MonGen.ZenTime) - 20s);
        //       if (makeGhostTime > Config.MakeGhostTime) makeGhostTime = Config.MakeGhostTime;
        //       if (GetTickCount() - DeathTick > makeGhostTime) MakeGhost();
        //   } else {
        //       if ((GetTickCount() - DeathTick) > Config.MakeGhostTime) MakeGhost();
        //   }
        //
        // 两条门都是**严格 >**。`GetMonstersZenTime` 读全服 `PlayObjectCount`
        // （`WorldServer.MonGen.cs:168-190`）⇒ 这里数 `race == Play` 的实体（宿主同口径）。
        let play_object_count = world
            .entities
            .iter()
            .filter(|x| x.race == ACTOR_RACE_PLAY)
            .count() as i32;
        // `get_monsters_zen_time` 只读这两项；其余字段用 oracle 构造缺省（不参与计算）。
        let cfg = crate::mongen::MonGenConfig {
            user_full: world.mon_gen_user_full,
            zen_fast_step: world.mon_gen_zen_fast_step,
            ..crate::mongen::MonGenConfig::default()
        };
        let gate = crate::mongen::monster_death_gate(
            can_re_alive,
            has_mon_gen,
            zen_time,
            world.make_ghost_time,
            play_object_count,
            &cfg,
        );
        let now_i32 = i32::try_from(now).unwrap_or(i32::MAX);
        let death_i32 = i32::try_from(death_tick).unwrap_or(i32::MAX);
        if crate::mongen::monster_due_for_ghost(death_i32, gate, now_i32) {
            make_ghost(world, actor_id);
        }
        // 死亡实体不再回血（oracle 同行）。oracle 的 VerifyTick 段在 `if (!Death)` **之外** ⇒
        // 死者同样刷新格子时间（`BaseObject.Base.cs:220-228`）。
        world.verify_map_time(actor_id);
        return;
    }
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let health_fill_time = world.health_fill_time;
    let spell_fill_time = world.spell_fill_time;
    let recovery_tick = ((now - e.auto_recovery_tick) / 20) as i32;
    let mut health_tick = e.health_tick + recovery_tick;
    let mut spell_tick = e.spell_tick + recovery_tick;
    let mut hp = e.hp;
    let mut mp = e.mp;
    let (max_hp, max_mp) = (e.max_hp, e.max_mp);
    // `BaseObject.Base.cs:47-55`：`WAbil.MaxHP / 75 + 1`，`(HP + n) < MaxHP` 否则截断到 `MaxHP`。
    // 加/比在 `int` 上进行（oracle 的 `ushort + ushort` 被提升为 `int`，不会在 65536 处回绕）；
    // 落回 `u16` 时 `sum < max_hp <= 65535`，强转安全。
    if hp < max_hp && health_tick >= health_fill_time {
        let n = i32::from(max_hp) / 75 + 1;
        let sum = i32::from(hp) + n;
        hp = if sum < i32::from(max_hp) {
            sum as u16
        } else {
            max_hp
        };
    }
    // `BaseObject.Base.cs:57-65`：`WAbil.MaxMP / 18 + 1`（同 HP 支在 `int` 上计算）。
    if mp < max_mp && spell_tick >= spell_fill_time {
        let n = i32::from(max_mp) / 18 + 1;
        let sum = i32::from(mp) + n;
        mp = if sum < i32::from(max_mp) {
            sum as u16
        } else {
            max_mp
        };
    }
    // `BaseObject.Base.cs:66-69` 的归零两行（照 oracle 次序：先判恢复、再归零）。
    if health_tick >= health_fill_time {
        health_tick = 0;
    }
    if spell_tick >= spell_fill_time {
        spell_tick = 0;
    }
    // `BaseObject.Base.cs:105-110` 体力点段。
    if health_tick < -health_fill_time && hp > 1 {
        hp -= 1;
        health_tick += health_fill_time;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.health_tick = health_tick;
        e.spell_tick = spell_tick;
        e.hp = hp;
        e.mp = mp;
        e.auto_recovery_tick = now;
    }
    target_cret_cleanup(world, actor_id, now);
    // `BaseObject.Base.cs:223-228`：`(now - VerifyTick) > 30s` ⇒ `Envir.VerifyMapTime(...)`
    // 刷新本格 `CellObject.AddTime`（静止怪/卫士/NPC 不被 60s 过期逐出）。不抽随机数。
    world.verify_map_time(actor_id);
}

/// `BaseObject.Run()` 的 `TargetCret` 清理段（`BaseObject.Base.cs:111-118`）。
fn target_cret_cleanup(world: &mut World, actor_id: i32, now: i64) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let Some(target_id) = e.ai.target_creature else {
        return;
    };
    let (map_id, x, y, focus) = (e.map_id, e.x, e.y, e.ai.target_focus_tick);
    let invalid = match world.entities.get(target_id) {
        None => true,
        Some(t) => {
            (now - focus) > 30_000
                || t.death
                || t.ghost
                || t.map_id != map_id
                || (t.x - x).abs() > 15
                || (t.y - y).abs() > 15
        }
    };
    if invalid {
        clear_target_creature(world, actor_id, target_id);
    }
}

/// `BaseObject.MakeGhost()`（`BaseObject.Base.cs:484-496`）：
///
/// ```csharp
/// if (CanReAlive) { Invisible = true; } else { Ghost = true; }
/// GhostTick = HUtil32.GetTickCount();
/// DisappearA();
/// ```
///
/// `CanReAlive` 的怪**保留尸体**（`Invisible`，可被 `ReAliveEx` 复活）；其余置 `Ghost`
/// （5 分钟后由 [`crate::mongen::process_cert_lists`] 释放）。
/// `DisappearA()` 的格子面 = [`World::disappear_a`]；`RM_DISAPPEAR` 帧由 AOI 视野差集表现。
///
/// **oracle 怪癖照抄**：死亡支每跑一次 `base.Run` 且门仍成立就再调一次 `MakeGhost`
/// （`Invisible` 尸体仍满足 `now - DeathTick > makeGhostTime`）⇒ `GhostTick` 被反复刷新、
/// `DisappearA` 反复执行。本函数同样重复执行（幂等：已摘格时 `disappear_a` 返回 false）。
fn make_ghost(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    if let Some(e) = world.entities.get_mut(actor_id) {
        if e.can_re_alive {
            // `Invisible = true;`——尸体保留、仍可复活。
            e.invisible = true;
        } else {
            // `Ghost = true;`
            e.ghost = true;
        }
        e.ghost_tick = now;
    }
    world.disappear_a(actor_id);
}

/// 按**实体槽位顺序**对场上所有怪物跑一次 [`monster_run`]（`ProcessMonsters` 的单线程等价物）。
///
/// 顺序契约：来自 `EntityStore` 的显式槽位索引（`iter`），不迭代 Query。
/// 每只怪再跑一次 [`base_object_run_monster`]（`MonsterObject.Run` 末尾的 `base.Run()`）。
/// **Lane B**：race 110/111 的 `Run`（`CastleDoor.Run` / `WallStructure.Run`）+ 副作用落地。
///
/// oracle 调用链：`ProcessMonsters` → `cert.Run()` 多态分派到
/// `CastleDoor.Run`（`CastleDoor.cs:84-104`）/ `WallStructure.Run`（`WallStructure.cs:47-82`），
/// 末尾再 `base.Run()`（= `GuardUnit:NormNpc` → `AnimalObject.Run` → `BaseObject.Run`，由调用方
/// 按序跑 [`animal_run`] + [`base_object_run_monster`]）。
///
/// 事件落地：
/// - `RefDigUp/RefDigDown/RefTurn/RefAlive` ⇒ `SendRefMsg`（`MonsterRefMsg`，宿主排水出帧）；
///   `RM_TURN` 的收件人支在 `PlayObject.Message.cs:1524-1560` 发 `SM_TURN` + `SM_FEATURECHANGED`；
/// - `SetMapXyFlag` ⇒ 同步按序作用到 `World.maps[map_id]`，并保留原始序供判据。
///
/// **HP 权威 = `Entity.hp`**：oracle 的 `WAbil.HP` 只有一份；本骨架 `StruckDamage` 写实体，
/// 因此每 tick 入口把 `CastleUnit.hp` 同步成实体 HP，再跑门/墙状态机（C3b 的防漂移牙）。
fn castle_structure_run(world: &mut World, actor_id: i32, race: u8) {
    let now = world.clock_ms;
    let Some((castle_index, role, map_id, hp)) = world.entities.get(actor_id).and_then(|e| {
        e.castle_binding
            .map(|b| (b.castle_index, b.role, e.map_id, e.hp))
    }) else {
        return;
    };
    let mut events = Vec::new();
    if let Some(castle) = world.castles.get_mut(castle_index) {
        if let Some(unit) = castle.defense_unit_mut(role) {
            unit.hp = i32::from(hp);
            events = if race == 110 {
                // `CastleDoor.Run`：`Death && Castle != null` ⇒ 刷 `DeathTick`；
                // 未开门 ⇒ 按 `3 - Round(HP/MaxHP*3)` 刷 `Dir` + `RM_TURN`（`n08 < 3`）。
                unit.door_run(now, true)
            } else {
                // `WallStructure.Run`：死亡恢复格（B-15：`LowWall` 无法救回）+ `RM_DIGUP`。
                unit.wall_run(now)
            };
        }
    }
    for event in events {
        if let crate::castle::StructureEvent::RefDigUp { dir, .. }
        | crate::castle::StructureEvent::RefDigDown { dir, .. }
        | crate::castle::StructureEvent::RefTurn { dir, .. }
        | crate::castle::StructureEvent::RefAlive { dir, .. } = event
        {
            // `SendRefMsg` 的帧方向 = `BaseObject.Dir`（状态机刚写完）。
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.dir = dir;
            }
        }
        match event {
            crate::castle::StructureEvent::RefDigUp { dir, x, y } => {
                send_ref_msg(
                    world,
                    actor_id,
                    RM_DIGUP,
                    i32::from(dir),
                    i32::from(x),
                    i32::from(y),
                    0,
                );
            }
            crate::castle::StructureEvent::RefDigDown { dir, x, y } => {
                send_ref_msg(
                    world,
                    actor_id,
                    RM_DIGDOWN,
                    i32::from(dir),
                    i32::from(x),
                    i32::from(y),
                    0,
                );
            }
            crate::castle::StructureEvent::RefTurn { dir, x, y } => {
                send_ref_msg(
                    world,
                    actor_id,
                    RM_TURN,
                    i32::from(dir),
                    i32::from(x),
                    i32::from(y),
                    0,
                );
            }
            crate::castle::StructureEvent::RefAlive { dir, x, y } => {
                send_ref_msg(
                    world,
                    actor_id,
                    RM_ALIVE,
                    i32::from(dir),
                    i32::from(x),
                    i32::from(y),
                    0,
                );
            }
            crate::castle::StructureEvent::SetMapXyFlag { .. } => {}
        }
        world.record_castle_structure(castle_index, map_id, event);
    }
}

/// race 110/111 是否绑定到一座城堡（未绑定 ⇒ 不跑门/墙状态机，落回通用路径）。
fn has_castle_binding(world: &World, actor_id: i32) -> bool {
    world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.castle_binding.is_some())
}

pub fn monster_ai_tick(world: &mut World) {
    let ids: Vec<i32> = world
        .entities
        .iter()
        // `race > Play` 会把交易商人也命中（`ActorRace.Merchant == Animal == 50`）⇒
        // 显式排除 `merchant.is_some()` 的实体（M3⑤-实现 2：商人由自己的 tick 驱动）。
        // 固定管理 NPC（`Envir/Npcs.txt`，race = `ActorRace.NPC`）同样排除：oracle 由
        // `ProcessNpcs`（`WorldServer.Merchant.cs:118`）驱动，本仓已由 `World::npc_tick`
        // 建模（同一次序）；此处必须不跑怪物 AI（否则会用与 oracle 不同的取数路径污染
        // RNG 流，且与 `npc_tick` 双驱固定 NPC）。
        .filter(|e| {
            e.race > ACTOR_RACE_PLAY
                && e.merchant.is_none()
                && e.npc.is_none()
                // `Ghost` 怪在 oracle `ProcessMonsters` 里整只跳过（`:320-333` 只做 5min 清理，
                // 由 [`crate::mongen::process_cert_lists`] 承担）⇒ 本 tick 不再驱动。
                // **死亡但未 Ghost** 的怪仍要跑 `base.Run`（死亡支 `MakeGhost`，`:85-101`）⇒ 不过滤。
                && !e.ghost
        })
        .map(|e| e.id)
        .collect();
    for id in ids {
        // `CanMove()`（`AnimalObject.cs:162-165`）对 `Death` 恒 false ⇒ `MonsterObject.Run`
        // 的 AI 体（Think/Walk/Attack）整段跳过，只剩 `base.Run()` 的死亡支与 VerifyTick 段。
        if world.entities.get(id).is_some_and(|e| e.death) {
            // **Lane B**：死亡但未 Ghost 的门/墙仍要跑自己的 `Run`（CastleDoor.Run 刷
            // `DeathTick`；WallStructure.Run 恢复格），随后 `base.Run()`。
            let race = world.entities.get(id).map_or(0, |e| e.race);
            if matches!(race, 110 | 111) && has_castle_binding(world, id) {
                castle_structure_run(world, id, race);
                base_object_run_monster(world, id);
                continue;
            }
            // oracle `ProcessMonsters` 对 `!Ghost` 的怪**照常调 `Run()`**（`WorldServer.MonGen.cs:288`）
            // ——`ZilKinZombi.Run` 的复活门正是读 `Death`（`ZilKinZombi.cs:37`）。本批只对
            // race 96 走类覆写；其余死怪维持既有等价（`BaseObject.Run` 的死亡支）。
            // **MB-7**：`ElfMonster.Run` / `ElfWarriorMonster.Run` 的死亡支是
            // `(now - DeathTick) > 2s ⇒ MakeGhost()`（`ElfMonster.cs:53-58` /
            // `ElfWarriorMonster.cs:121-127`），**短于** `Config.MakeGhostTime` 的通用门
            // ⇒ 死后仍必须跑类 `Run`。
            // **MB-8/MB-9**：100/120/132/214/215 的类 `Run` 在 `Death` 下仍有可观测路径
            // （`SoccerBall.Run` 无 `Death` 门；`FireMonster`/`Fireball` 的 `CanMove()` 门后
            // 落到 `MonsterObject.Run` 的 `CheckRoyalty`/`CrazyMode`；`WhiteSkeleton` 的
            // `BoIsFirst` 只在首次 `Run`）。与 96/113/114 同口径：先跑类覆写，再按
            // [`run_class_base`] 跑 oracle 的 `base.Run()` 链。
            if world
                .entities
                .get(id)
                .is_some_and(|e| matches!(e.race, 96 | 113 | 114 | 100 | 120 | 132 | 214 | 215))
            {
                if let Some(base) = monster_class_run(world, id) {
                    run_class_base(world, id, base);
                    continue;
                }
            }
            base_object_run_monster(world, id);
            continue;
        }
        // **Lane B**：城堡门/墙（race 110/111）**不得**进入 `monster_run`（`GuardUnit:NormNpc`
        // 没有 `MonsterObject.Run` 的 Think/游走/索敌）。未绑定城堡时落回下面的静止守卫/通用支。
        let race_now = world.entities.get(id).map_or(0, |e| e.race);
        if matches!(race_now, 110 | 111) && has_castle_binding(world, id) {
            castle_structure_run(world, id, race_now);
            // `CastleDoor.Run`/`WallStructure.Run` 末尾的 `base.Run()`。
            animal_run(world, id);
            base_object_run_monster(world, id);
            continue;
        }
        // `GuardList` 放置的守卫（race 11/12/20/112）不继承 `MonsterObject.Run`
        // （`GuardUnit:NormNpc`，`NormNpc.Run` 只是「清 master + `base.Run()`」）⇒ **不游走/不索敌**，
        // 但仍要跑自己的 `base.Run()` 段（`BaseObject.Run` 的恢复/`RM_STRUCK` 出队/`TargetCret` 清理）。
        // **MB-3（R02）**：`ArcherGuard`(112) / `ArcherPolice`(20) 覆写 `Run`
        // （`ArcherGuard.cs:39-91`），虽然仍在 `GuardUnit : NormNpc : AnimalObject` 链上，
        // 但它有自己的索敌/射击逻辑，**不能**走下面「静止守卫」的等价支。
        if matches!(world.entities.get(id).map_or(0, |e| e.race), 20 | 112) {
            archer_guard_run(world, id);
            // `NormNpc.Run` 的 `base.Run()` = `AnimalObject.Run` → `BaseObject.Run`。
            animal_run(world, id);
            base_object_run_monster(world, id);
            continue;
        }
        if is_guard_race(world.entities.get(id).map_or(0, |e| e.race)) {
            // `NormNpc.Run`（`NormNpc.cs:1604-1611`）→ `base.Run()` = `AnimalObject.Run`
            // （HolySeize 到期）→ `BaseObject.Run`。
            animal_run(world, id);
            base_object_run_monster(world, id);
            continue;
        }
        // 专用怪 `BeeQueen`/`SpiderHouseMonster` 覆写 `Run`（`BeeQueen.cs:51-79` /
        // `SpiderHouseMonster.cs:45-73`）：只有 `CanMove`/`WalkTick`/`AttackTick` 三段 +
        // `base.Run()`，**不**走 `MonsterObject.Run` 的 Think/AttackTarget/GotoTargetXY/
        // Wondering/CheckRoyalty/CrazyMode 段。
        if crate::special_monster::special_monster_run(world, id) {
            animal_run(world, id);
            base_object_run_monster(world, id);
            continue;
        }
        // **MB-1**：四个专用怪的 `Run` 覆写（`StickMonster`/`DigOutZombi`/
        // `ScultureMonster`/`BigHeartMonster`）。先跑各自 `Run`，再按 [ClassRunBase]
        // 跑 oracle 的 `base.Run()` 链。
        // `base.Run()` 链（`Animal`/`AnimalTwice`/`Monster`/`MagicMonster`/`None`）见
        // [`run_class_base`]。
        if let Some(base) = monster_class_run(world, id) {
            run_class_base(world, id, base);
            continue;
        }
        monster_run(world, id);
        // `MonsterObject.Run` 的末尾 `base.Run()`（`MonsterObject.cs:255/359`）：
        // `AnimalObject.Run`（HolySeize 到期）→ `BaseObject.Run`。
        animal_run(world, id);
        base_object_run_monster(world, id);
    }
}

/// 便于测试/接线：当前朝向下的前一格（oracle `GetFrontPosition`，越界 clamp）。
#[must_use]
pub fn front_of(world: &World, actor_id: i32, dir: u8, map_id: usize) -> Option<(i32, i32)> {
    let e = world.entities.get(actor_id)?;
    let map = world.maps.get(map_id)?;
    Some(front_position(e.x, e.y, dir, map.width, map.height))
}

// ---------------------------------------------------------------------------
// **`openmir2-attacktarget-overrides`**（tier=R）——余下 `AttackTarget` 覆写族
//
// oracle 清册（`grep "override.*AttackTarget" src/` 的 10 条真覆写）：
//   DoubleCritical(130) / DualAxe(87，93/104 继承) / ElfWarrior(114) /
//   ExplosionSpider(117) / GasAttack(90/105/106) / MagCow(91) / MagicMonster(215) /
//   PercentMonster(80，死类) / SpitSpider(82，118/119 继承) / CentipedeKing(107)。
// 其中 DualAxe/GasAttack/CentipedeKing 已由 MB-2/MB-3 落地（本片复核后未重复实现），
// 百分号类见 `percent_monster_attack_target` 的逐行同构说明。
// ---------------------------------------------------------------------------

/// `Messages.RM_STRUCKEFFECT`（`src/OpenMir2/Messages.cs:658`；10102）——
/// `ElfWarriorMonster.SpitAttack` 的 `SendStruckDelayMsg` ident（`:65`）。
pub const RM_STRUCKEFFECT: u16 = 10102;

/// `SendStruckDelayMsg` 的**攻方 HP 快照**版本。
///
/// oracle 有三处（`DoubleCriticalMonster.DoubleAttack@:67`、`SpitSpider.SpitAttack@:43`、
/// `ElfWarriorMonster.SpitAttack@:65`）在**收件人**上调 `SendStruckDelayMsg`，但第 2/3 参写
/// 的是未限定的 `WAbil.HP` / `WAbil.MaxHP` = **攻方自己的** HP/MaxHP（其余调用点显式写
/// `baseObject.WAbil.*` 或 `targetObject.WAbil.*`）。此函数按该字面语义取攻方快照。
fn send_struck_delay_msg_attacker_hp(
    world: &mut World,
    receiver_id: i32,
    hitter_id: i32,
    ident: u16,
    damage: i32,
    delay_ms: i64,
) {
    if !world.refmessage_delay_enabled() {
        return;
    }
    let Some((hp, max_hp)) = world
        .entities
        .get(hitter_id)
        .map(|e| (i32::from(e.hp), i32::from(e.max_hp)))
    else {
        return;
    };
    world.send_struck_delay_msg_7(receiver_id, ident, damage, hp, max_hp, hitter_id, delay_ms);
}

/// `SystemShare.Config.SpitMap[dir, y, x]`（`GameSvrConf.cs:1332-1371`，8×5×5）。
///
/// 索引与 oracle 相同：`[btDir][i][k]`，其中 `i` 是行（y 偏移 `i-2`）、`k` 是列
/// （x 偏移 `k-2`）。**逐字节照抄**（含 `DR_UP` 两格在 `x=0`、`y=-2/-1` 的形状）。
const SPIT_MAP: [[[u8; 5]; 5]; 8] = [
    // DR_UP（0）
    [
        [0, 0, 1, 0, 0],
        [0, 0, 1, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ],
    // DR_UPRIGHT（1）
    [
        [0, 0, 0, 0, 1],
        [0, 0, 0, 1, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ],
    // DR_RIGHT（2）
    [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 1, 1],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ],
    // DR_DOWNRIGHT（3）
    [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 1, 0],
        [0, 0, 0, 0, 1],
    ],
    // DR_DOWN（4）
    [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 1, 0, 0],
        [0, 0, 1, 0, 0],
    ],
    // DR_DOWNLEFT（5）
    [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 1, 0, 0, 0],
        [1, 0, 0, 0, 0],
    ],
    // DR_LEFT（6）
    [
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [1, 1, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ],
    // DR_UPLEFT（7）
    [
        [1, 0, 0, 0, 0],
        [0, 1, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
        [0, 0, 0, 0, 0],
    ],
];

/// `BaseObject.TargetInSpitRange(target, ref btDir)`（`BaseObject.cs:1327-1351`）。
///
/// 返回 `Some(btDir)` = oracle `true`；`None` = `false`。
/// - `|Δx| <= 2 && |Δy| <= 2` 才进入；
/// - `|Δx| <= 1 && |Δy| <= 1` ⇒ 调 `GetAttackDir` 后**无条件 `return true`**
///   （即使 `GetAttackDir` 失败、`btDir` 保持 0）——照抄；
/// - 否则 `SpitMap[GetNextDirection(…), Δy+2, Δx+2] == 1`。
#[must_use]
pub fn target_in_spit_range(world: &World, actor_id: i32, target_id: i32) -> Option<u8> {
    let (cx, cy) = world.entities.get(actor_id).map(|e| (e.x, e.y))?;
    let (tx, ty) = world.entities.get(target_id).map(|t| (t.x, t.y))?;
    if (tx - cx).abs() > 2 || (ty - cy).abs() > 2 {
        return None;
    }
    let nx = tx - cx;
    let ny = ty - cy;
    if nx.abs() <= 1 && ny.abs() <= 1 {
        return Some(get_attack_dir(world, actor_id, target_id).unwrap_or(0));
    }
    let sx = nx + 2;
    let sy = ny + 2;
    if (0..=4).contains(&sx) && (0..=4).contains(&sy) {
        let dir = crate::magic::next_direction(cx, cy, tx, ty);
        if SPIT_MAP[dir as usize][sy as usize][sx as usize] == 1 {
            return Some(dir);
        }
    }
    None
}

/// `MonsterObject.GetLongAttackDirDis(baseObject, dis, ref dir)`（`MonsterObject.cs:399-463`）。
///
/// **oracle bug 照搬**：八支里只有 `Left` 支**不 break**，且该支之后块尾的 `dir = 0`
/// 会执行 ⇒ 目标恰在正左方时返回 `true` 但 `dir == 0`（左向永不返回 6）。
/// 其余七支命中即 `break`，不会被块尾覆盖。
#[must_use]
fn get_long_attack_dir_dis(world: &World, actor_id: i32, target_id: i32, dis: i32) -> Option<u8> {
    let (cx, cy) = world.entities.get(actor_id).map(|e| (e.x, e.y))?;
    let (tx, ty) = world.entities.get(target_id).map(|t| (t.x, t.y))?;
    let mut n_c = 0;
    let mut found = false;
    let mut dir = 0u8;
    loop {
        if cx - n_c <= tx
            && cx + n_c >= tx
            && cy - n_c <= ty
            && cy + n_c >= ty
            && (cx != tx || cy != ty)
        {
            if cx - n_c == tx && cy == ty {
                // `Left` 支无 break（照抄）：oracle 写 `dir = Direction.Left` 后继续往下，
                // 块尾 `dir = 0` 再覆盖 ⇒ 对外只留下 `result = true` / `dir = 0`。
                found = true;
            }
            if cx + n_c == tx && cy == ty {
                dir = DIR_RIGHT;
                found = true;
                break;
            }
            if cx == tx && cy - n_c == ty {
                dir = DIR_UP;
                found = true;
                break;
            }
            if cx == tx && cy + n_c == ty {
                dir = DIR_DOWN;
                found = true;
                break;
            }
            if cx - n_c == tx && cy - n_c == ty {
                dir = DIR_UP_LEFT;
                found = true;
                break;
            }
            if cx + n_c == tx && cy - n_c == ty {
                dir = DIR_UP_RIGHT;
                found = true;
                break;
            }
            if cx - n_c == tx && cy + n_c == ty {
                dir = DIR_DOWN_LEFT;
                found = true;
                break;
            }
            if cx + n_c == tx && cy + n_c == ty {
                dir = DIR_DOWN_RIGHT;
                found = true;
                break;
            }
            dir = 0;
        }
        n_c += 1;
        if n_c > dis {
            break;
        }
    }
    if found {
        Some(dir)
    } else {
        None
    }
}

/// 覆写族公共尾：不邻接时 `SetTargetXy(target.CurrX, target.CurrY)`，异图 `DelTargetCreat()`。
fn sync_target_xy_or_clear(world: &mut World, actor_id: i32, target_id: i32) {
    let (same_map, tx, ty) = match (
        world.entities.get(actor_id).map(|e| e.map_id),
        world.entities.get(target_id).map(|t| (t.map_id, t.x, t.y)),
    ) {
        (Some(am), Some((tm, tx, ty))) => (am == tm, tx, ty),
        _ => (false, 0, 0),
    };
    if same_map {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.target_x = tx;
            e.ai.target_y = ty;
        }
    } else {
        del_target_creature(world, actor_id);
    }
}

/// 覆写族的直通 proper 判定（oracle `IsProperTarget` = `IsAttackTarget`，
/// `BaseObject.Base.cs:470-473`；**不**折入 death/ghost/invisible，与 `GetMovingObject(...,true)`
/// 自身的 `!Ghost && !Death` 枚举门互补）。
fn proper_for_override(world: &World, actor_id: i32, target_id: i32) -> bool {
    world
        .entities
        .get(actor_id)
        .zip(world.entities.get(target_id))
        .is_some_and(|(a, t)| is_proper_target_grouped_direct(a, t, &[]))
}

/// `MonsterObject.AttackTarget` 风格的共享骨架：射程判定 → 到点 ⇒ 攻击 → `true`；
/// 不在射程 ⇒ 同图写 `TargetX/Y`、异图清目标。
fn override_attack_target_with<F>(
    world: &mut World,
    actor_id: i32,
    in_range: bool,
    bt_dir: u8,
    on_due: F,
) -> bool
where
    F: FnOnce(&mut World, u8),
{
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    if in_range {
        let now = world.clock_ms;
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
            }
            on_due(world, bt_dir);
        }
        true
    } else {
        sync_target_xy_or_clear(world, actor_id, target_id);
        false
    }
}

/// `DoubleCriticalMonster.AttackTarget`（`DoubleCriticalMonster.cs:14-41`）。
fn double_critical_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let spit = target_in_spit_range(world, actor_id, target_id);
    override_attack_target_with(
        world,
        actor_id,
        spit.is_some(),
        spit.unwrap_or(0),
        |w, dir| {
            double_attack(w, actor_id, dir);
            break_holy_seize_mode(w, actor_id);
        },
    )
}

/// `DoubleCriticalMonster.DoubleAttack(btDir)`（`DoubleCriticalMonster.cs:43-73`）。
///
/// 取数次序（勿改）：`Lo(DC)+Random(|Hi-Lo|+1)`（`:46`，`<=0` ⇒ 无帧/无后续）→
/// `SendRefMsg(RM_HIT)` → 4×4 `SpitMap` 逐格：`Random(target.SpeedPoint)`（`:61`）→
/// `GetHitStruckDamage`（物理，`nDamage` 跨格**累积改写**）→ `StruckDamage` → 300ms。
fn double_attack(world: &mut World, actor_id: i32, bt_dir: u8) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = bt_dir;
    }
    let Some((cx, cy, dc, hit_point, map_id)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.dc, e.hit_point, e.map_id))
    else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let mut n_damage = lo + world.rng.random_below((hi - lo).abs() + 1);
    if n_damage <= 0 {
        return;
    }
    send_ref_msg(world, actor_id, RM_HIT, i32::from(bt_dir), cx, cy, 0);
    // 4×4：只扫 `SpitMap[dir][0..4][0..4]`（`i`/`k` 上限 4），偏移 `-2..+1`。
    for (i, row) in SPIT_MAP[bt_dir as usize].iter().enumerate().take(4) {
        for (k, cell) in row.iter().enumerate().take(4) {
            if *cell != 1 {
                continue;
            }
            let nx = cx - 2 + k as i32;
            let ny = cy - 2 + i as i32;
            let Some(target_id) = world.moving_object_at(map_id, nx, ny, true) else {
                continue;
            };
            if target_id == actor_id || !proper_for_override(world, actor_id, target_id) {
                continue;
            }
            let speed_point = world
                .entities
                .get(target_id)
                .map_or(0, |t| i32::from(t.speed_point));
            if world.rng.random_below(speed_point) >= i32::from(hit_point) {
                continue;
            }
            let Some(target) = world.entities.get(target_id).cloned() else {
                continue;
            };
            let Some(attacker) = world.entities.get(actor_id).cloned() else {
                continue;
            };
            n_damage = i32::from(crate::combat::get_hit_struck_damage(
                &mut world.rng,
                &target,
                &attacker,
                n_damage,
            ));
            if n_damage > 0 {
                let _ = class_struck_apply(world, actor_id, target_id, n_damage, n_damage);
                send_struck_delay_msg_attacker_hp(
                    world,
                    target_id,
                    actor_id,
                    crate::slave::RM_REFMESSAGE,
                    n_damage,
                    300,
                );
            }
        }
    }
}

/// `SpitSpider.AttackTarget`（`SpitSpider.cs:58-85`；118/119 继承同一实现）。
///
/// `use_poison` = 该 race ctor 的 `UsePoison`（`SpitSpider.SpitAttack` 的 `:44` 门）：
/// 82=true、118=false、119=true；`ElfWarriorMonster`（114）的**非强化形态**也走本函数
/// （`base.AttackTarget()` ⇒ `SpitSpider` 版），其 ctor 置 `UsePoison=false`。
fn spit_spider_attack_target(world: &mut World, actor_id: i32, use_poison: bool) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let spit = target_in_spit_range(world, actor_id, target_id);
    override_attack_target_with(
        world,
        actor_id,
        spit.is_some(),
        spit.unwrap_or(0),
        |w, dir| {
            spit_attack(w, actor_id, dir, use_poison);
            break_holy_seize_mode(w, actor_id);
        },
    )
}

/// `SpitSpider.SpitAttack(btDir)`（`SpitSpider.cs:19-56`）。
///
/// 取数次序（勿改）：`Lo(DC)+Random(|Hi-Lo|+1)`（`:22`，`<=0` ⇒ 无帧/无后续）→
/// `SendRefMsg(RM_HIT)` → 5×5 `SpitMap` 逐格：`Random(target.SpeedPoint)`（`:37`）→
/// `GetMagStruckDamage`（魔法，**`nDamage` 跨格累积改写**）→ `StruckDamage` → 300ms →
/// `UsePoison` 且 `Random(攻方 AntiPoison + 20) == 0` ⇒ `MakePosion(DECHEALTH, 30, 1)`（`:44-49`）。
///
/// **注意**：`:46` 的 `AntiPoison` 是**攻方**自己的属性（不是 `baseObject.` 的；与
/// `GasAttackMonster.cs:31` 的 `baseObject.AntiPoison` 相反）。怪物 ctor 置 0 且不调
/// `RecalcAbilitys` ⇒ 恒 `Random(20)`。
fn spit_attack(world: &mut World, actor_id: i32, bt_dir: u8, use_poison: bool) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = bt_dir;
    }
    let Some((cx, cy, dc, hit_point, map_id)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.dc, e.hit_point, e.map_id))
    else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let mut n_damage = lo + world.rng.random_below((hi - lo).abs() + 1);
    if n_damage <= 0 {
        return;
    }
    send_ref_msg(world, actor_id, RM_HIT, i32::from(bt_dir), cx, cy, 0);
    for (i, row) in SPIT_MAP[bt_dir as usize].iter().enumerate() {
        for (k, cell) in row.iter().enumerate() {
            if *cell != 1 {
                continue;
            }
            let nx = cx - 2 + k as i32;
            let ny = cy - 2 + i as i32;
            let Some(target_id) = world.moving_object_at(map_id, nx, ny, true) else {
                continue;
            };
            if target_id == actor_id || !proper_for_override(world, actor_id, target_id) {
                continue;
            }
            let speed_point = world
                .entities
                .get(target_id)
                .map_or(0, |t| i32::from(t.speed_point));
            if world.rng.random_below(speed_point) >= i32::from(hit_point) {
                continue;
            }
            let Some(mac) = world.entities.get(target_id).map(|t| t.mac) else {
                continue;
            };
            let undead = world.entities.get(actor_id).map_or(0, |e| e.undead_power);
            n_damage =
                crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_damage, Some(undead));
            if n_damage > 0 {
                let _ = class_struck_apply(world, actor_id, target_id, n_damage, n_damage);
                send_struck_delay_msg_attacker_hp(
                    world,
                    target_id,
                    actor_id,
                    crate::slave::RM_REFMESSAGE,
                    n_damage,
                    300,
                );
                if use_poison {
                    // 攻方 `AntiPoison`：ctor 0、怪不调 `RecalcAbilitys` ⇒ 恒 0。
                    let anti_poison = 0;
                    if world.rng.random_below(anti_poison + 20) == 0 {
                        world.make_posion(target_id, crate::entity::POISON_DECHEALTH as u8, 30, 1);
                    }
                }
            }
        }
    }
}

/// `ExplosionSpider.AttackTarget`（`ExplosionSpider.cs:47-77`）。
///
/// 与基类差异：邻接且到点 ⇒ `DoSelfExplosion()`；**不调 `BreakHolySeizeMode`**。
fn explosion_spider_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let bt_dir = get_attack_dir(world, actor_id, target_id);
    override_attack_target_with(
        world,
        actor_id,
        bt_dir.is_some(),
        bt_dir.unwrap_or(0),
        |w, _| {
            do_self_explosion(w, actor_id);
        },
    )
}

/// `ExplosionSpider.DoSelfExplosion()`（`ExplosionSpider.cs:19-45`）。
///
/// 取数次序（勿改）：`HP=0` → `MAX(0, Lo(DC)+Random(|Hi-Lo|+1))`（`:22`）→
/// 逐 `VisibleActors`（`Death` 跳过、`IsProperTarget`、两轴 `<=1`）：
/// `GetHitStruckDamage(nPower/2)` → `GetMagStruckDamage(nPower/2)` → `StruckDamage(damage)` →
/// `RM_REFMESSAGE` 700ms。
fn do_self_explosion(world: &mut World, actor_id: i32) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.hp = 0;
    }
    let Some((cx, cy, dc)) = world.entities.get(actor_id).map(|e| (e.x, e.y, e.dc)) else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let n_power = i32::max(0, lo + world.rng.random_below((hi - lo).abs() + 1));
    let visible: Vec<i32> = world
        .entities
        .get(actor_id)
        .map(|e| e.visible_actors.iter().map(|v| v.target).collect())
        .unwrap_or_default();
    for target_id in visible {
        let Some(t) = world.entities.get(target_id) else {
            continue;
        };
        if t.death {
            continue;
        }
        let (tx, ty) = (t.x, t.y);
        if !proper_for_override(world, actor_id, target_id) {
            continue;
        }
        if (cx - tx).abs() > 1 || (cy - ty).abs() > 1 {
            continue;
        }
        let half = n_power / 2;
        let Some(target) = world.entities.get(target_id).cloned() else {
            continue;
        };
        let Some(attacker) = world.entities.get(actor_id).cloned() else {
            continue;
        };
        let mut damage = i32::from(crate::combat::get_hit_struck_damage(
            &mut world.rng,
            &target,
            &attacker,
            half,
        ));
        damage += crate::combat::get_mag_struck_damage(
            &mut world.rng,
            target.mac,
            half,
            Some(attacker.undead_power),
        );
        if damage > 0 {
            let _ = class_struck_apply(world, actor_id, target_id, damage, damage);
            world.send_struck_delay_msg_refmessage(target_id, actor_id, damage, 700);
        }
    }
}

/// `ExplosionSpider.Run()`（`ExplosionSpider.cs:79-90`）：`!Death && !Ghost` 且
/// `now - explosionMakeTime > 60000` ⇒ 刷新时刻并自爆；随后 `base.Run()`。
fn explosion_spider_run(world: &mut World, actor_id: i32) {
    let now = world.clock_ms;
    let fire = world
        .entities
        .get(actor_id)
        .is_some_and(|e| !e.death && !e.ghost && now - e.ai.explosion_make_time > 60 * 1000);
    if fire {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.explosion_make_time = now;
        }
        do_self_explosion(world, actor_id);
    }
}

/// `MagCowMonster.AttackTarget`（`MagCowMonster.cs:33-64`）。
fn mag_cow_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let bt_dir = get_attack_dir(world, actor_id, target_id);
    override_attack_target_with(
        world,
        actor_id,
        bt_dir.is_some(),
        bt_dir.unwrap_or(0),
        |w, dir| {
            magic_attack(w, actor_id, dir);
            break_holy_seize_mode(w, actor_id);
        },
    )
}

/// `MagCowMonster.MagicAttack(btDir)`（`MagCowMonster.cs:13-31`）。
///
/// 取数次序（勿改）：`Lo(DC)+Random(|Hi-Lo|+1)`（`:16`，`<=0` ⇒ 无帧/无后续）→
/// `SendRefMsg(RM_HIT)` → `GetPoseCreate()`（前格首个 `!Ghost && !Death`）→
/// `IsProperTarget` → **`AntiMagic >= 0`**（`ushort` 属性 ⇒ 恒真；不是命中率）→
/// `GetMagStruckDamage` → `StruckDamage` → 300ms。
fn magic_attack(world: &mut World, actor_id: i32, bt_dir: u8) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = bt_dir;
    }
    let Some((cx, cy, dc, map_id, undead)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.x, e.y, e.dc, e.map_id, e.undead_power))
    else {
        return;
    };
    let lo = i32::from(crate::combat::lo_byte(dc));
    let hi = i32::from(crate::combat::hi_byte(dc));
    let mut n_power = lo + world.rng.random_below((hi - lo).abs() + 1);
    if n_power <= 0 {
        return;
    }
    send_ref_msg(world, actor_id, RM_HIT, i32::from(bt_dir), cx, cy, 0);
    let pose = front_of(world, actor_id, bt_dir, map_id)
        .and_then(|(fx, fy)| world.moving_object_at(map_id, fx, fy, true));
    let Some(target_id) = pose else {
        return;
    };
    if target_id == actor_id || !proper_for_override(world, actor_id, target_id) {
        return;
    }
    // `AntiMagic >= 0`（`BaseObject.cs:113` 的 `ushort`）⇒ **恒真**（不是命中率门）。
    // 保留 oracle 的字面判据以钉住语义；`absurd_extreme_comparisons` 由局部 allow 放行。
    #[allow(unused_comparisons, clippy::absurd_extreme_comparisons)]
    let anti_magic_ok = world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.anti_magic >= 0);
    if !anti_magic_ok {
        return;
    }
    let Some(mac) = world.entities.get(target_id).map(|t| t.mac) else {
        return;
    };
    n_power = crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_power, Some(undead));
    if n_power > 0 {
        let _ = class_struck_apply(world, actor_id, target_id, n_power, n_power);
        world.send_struck_delay_msg_refmessage(target_id, actor_id, n_power, 300);
    }
}

/// `ElfWarriorMonster.AttackTarget`（`ElfWarriorMonster.cs:72-103`）。
///
/// 类链 `ElfWarriorMonster : SpitSpider : AtMonster : MonsterObject` ⇒ `:80` 的
/// `base.AttackTarget()` 解析到 **`SpitSpider.AttackTarget`**（`SpitSpider.cs:58-85`），
/// 不是 `MonsterObject` 的物理 `_Attack`。`Appr=171`（部署 `神兽1`，race=114）即是这一形态：
/// `TargetInSpitRange`（**无** `GetLongAttackDirDis`）→ `SpitSpider.SpitAttack`（**Byte** DC、
/// 5×5 `SpitMap`、`RM_REFMESSAGE` 300ms），`UsePoison=false`（`ElfWarriorMonster.cs:32`）。
/// 强化形态（`Appr` 704/706/708）：`TargetInSpitRange || GetLongAttackDirDis(target, 3)`，
/// 到点 ⇒ `ElfWarriorMonster.SpitAttack(btDir)`（Word DC、`RM_STRUCKEFFECT`）。
fn elf_warrior_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    let appr = world.entities.get(actor_id).map_or(0, |e| e.appr);
    if appr != 704 && appr != 706 && appr != 708 {
        // **非强化形态（部署常态）**：`base.AttackTarget()` = `SpitSpider.AttackTarget`。
        return spit_spider_attack_target(world, actor_id, false);
    }
    let dir = target_in_spit_range(world, actor_id, target_id)
        .or_else(|| get_long_attack_dir_dis(world, actor_id, target_id, 3));
    override_attack_target_with(world, actor_id, dir.is_some(), dir.unwrap_or(0), |w, d| {
        elf_warrior_spit_attack(w, actor_id, d);
        break_holy_seize_mode(w, actor_id);
    })
}

/// `ElfWarriorMonster.SpitAttack(dir)`（`ElfWarriorMonster.cs:41-70`）。
///
/// 取数次序（勿改）：`LoWord(DC)+Random(|HiWord(DC)-LoWord(DC)|+1)`（`:45`——**Word**
/// 而不是 Byte：`HiWord(ushort) == 0`、`LoWord(ushort) == DC` ⇒ `DC + Random(DC+1)`）→
/// `<=0` 无帧 → `SendRefMsg(RM_HIT)` → `nDamage = 1` → `while(true)`。
///
/// **G19（登记，不修）**：`while(true)` **无 break、`nDamage` 不递增**（`:54-69`）——
/// 每轮都用 `nDamage == 1` 取**同一前格**；只要进入本函数（无论前格有无目标/命中与否）
/// 就**永不返回**（有目标时还会无限重复 `Random(SpeedPoint)`/`GetMagStruckDamage` 抽数）。
/// 真机 oracle 的等价行为是「挂住线程」，任何可返回的实现都是登记偏离；本实现只执行
/// **首轮**（`GetNextPosition(...,1)`）后返回。race 114 数据不可达（无 MonGen/库行）。
fn elf_warrior_spit_attack(world: &mut World, actor_id: i32, dir: u8) {
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = dir;
    }
    let Some((dc, hit_point, map_id)) = world
        .entities
        .get(actor_id)
        .map(|e| (e.dc, e.hit_point, e.map_id))
    else {
        return;
    };
    // `HUtil32.HiWord(ushort)` → 0；`LoWord(ushort)` → DC（C# 重载解析选 `LoWord(int)`）。
    // `Entity.dc` 是 i32 载体，按 C# `ushort` 口径取低 16 位。
    let lo_word = dc & 0xFFFF;
    let hi_word = 0i32;
    let mut n_damage = lo_word + world.rng.random_below((hi_word - lo_word).abs() + 1);
    if n_damage <= 0 {
        return;
    }
    let (x, y) = world.entities.get(actor_id).map_or((0, 0), |e| (e.x, e.y));
    send_ref_msg(world, actor_id, RM_HIT, i32::from(dir), x, y, 0);
    n_damage = 1;
    // G19：只跑首轮（oracle 此处 while(true) 永不返回）。
    let Some((nx, ny)) = front_of(world, actor_id, dir, map_id) else {
        return;
    };
    let Some(target_id) = world.moving_object_at(map_id, nx, ny, true) else {
        return;
    };
    if target_id == actor_id || !proper_for_override(world, actor_id, target_id) {
        return;
    }
    let speed_point = world
        .entities
        .get(target_id)
        .map_or(0, |t| i32::from(t.speed_point));
    if world.rng.random_below(speed_point) >= i32::from(hit_point) {
        return;
    }
    let Some(mac) = world.entities.get(target_id).map(|t| t.mac) else {
        return;
    };
    let undead = world.entities.get(actor_id).map_or(0, |e| e.undead_power);
    n_damage = crate::combat::get_mag_struck_damage(&mut world.rng, mac, n_damage, Some(undead));
    if n_damage > 0 {
        let _ = class_struck_apply(world, actor_id, target_id, n_damage, n_damage);
        send_struck_delay_msg_attacker_hp(
            world,
            target_id,
            actor_id,
            RM_STRUCKEFFECT,
            n_damage,
            300,
        );
    }
}

/// `MagicMonster.AttackTarget`（`MagicMonster.cs:47-82`；215 `FireballMonster` 继承）。
///
/// 与基类差异：① `TargetCret == Master` ⇒ 直接置 `null`（不走 `DelTargetCreat`）并返回 false；
/// ② 邻接到点**只刷新 `AttackTick`/`TargetFocusTick`**——无伤害、无 `RM_HIT`、
/// **不调 `BreakHolySeizeMode`**。
fn magic_monster_attack_target(world: &mut World, actor_id: i32) -> bool {
    let Some(target_id) = world
        .entities
        .get(actor_id)
        .and_then(|e| e.ai.target_creature)
    else {
        return false;
    };
    if world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.master == Some(target_id))
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.target_creature = None;
        }
        return false;
    }
    let now = world.clock_ms;
    if get_attack_dir(world, actor_id, target_id).is_some() {
        let due = world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.attack_tick > e.ai.next_hit_time);
        if due {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.ai.attack_tick = now;
                e.ai.target_focus_tick = now;
            }
        }
        true
    } else {
        sync_target_xy_or_clear(world, actor_id, target_id);
        false
    }
}

/// `PercentMonster.AttackTarget`（`PercentMonster.cs:47-77`）。
///
/// **逐行同构**：`TargetCret == null ⇒ false`；`GetAttackDir` 命中 ⇒
/// `now - AttackTick > NextHitTime` 时刷新两 tick、`Attack(TargetCret, btDir)`、
/// `BreakHolySeizeMode()`，返回 `true`；不邻接 ⇒ 同图 `SetTargetXy`、异图 `DelTargetCreat`，
/// 返回 `false`。与 `MonsterObject.AttackTarget`（`MonsterObject.cs:220-249`）**完全相同**
/// （`Attack` 的虚调用也只被该基类使用）⇒ 直接复用 [`attack_target`]，不复制一份实现。
///
/// 该类是**死类**（全仓无 `new PercentMonster`，`CreateMonster` 亦无 race 80 分支；
/// 其 ctor 自置 `Race = 80`）⇒ 生产不可达；dispatch 仍显式登记以防未来接类。
fn percent_monster_attack_target(world: &mut World, actor_id: i32) -> bool {
    attack_target(world, actor_id)
}

/// `ActorRace.Guard = 11`（`Race.cs:16`）—— `GuardUnit.IsProperTarget` 的
/// `CrimeforCastle` 子分支只对 race 11 生效。
const ACTOR_RACE_GUARD: u8 = 11;
/// `GuardUnit.cs:52` 的 `2 * 60 * 1000`：`CrimeforCastle`（犯罪标记）的有效窗口。
const GUARD_CRIME_FOR_CASTLE_MS: i64 = 2 * 60 * 1000;

/// `GuardUnit.IsProperTarget` 只读的目标字段快照（避免与 `World` 的犯罪表写侧冲突）。
struct GuardTargetSnapshot {
    id: i32,
    race: u8,
    master: Option<i32>,
    admin_mode: bool,
    stone_mode: bool,
    castle: Option<crate::entity::CastleBinding>,
    target_creature: Option<i32>,
    pk_point: i32,
}

/// `GuardUnit.IsProperTarget`（`GuardUnit.cs:35-125`）—— ArcherGuard 的 proper 判定。
///
/// `Castle != null` 分支（`GuardUnit.cs:37-100`）的等价物 = 实体带
/// [`crate::entity::CastleBinding`]（oracle `BaseObject.Castle != null`；由
/// `UserCastle::Initialize` / [`World::bind_castle_defense`] 写入门/墙/弓手）：
///
/// 1. `LastHiter == target` ⇒ `true`；
/// 2. `Castle.UnderWar` ⇒ `true`（**先于**成员豁免与管理员/同城排除，故开战时全打）；
/// 3. `Race == ActorRace.Guard(11)` 且 `CrimeforCastle` 未过期（<2min）⇒ `true`；过期清位，
///    若该守卫自身 `Castle != null` ⇒ `false`；
/// 4. `Castle.MasterGuild != null` 时：目标（无主人则看本人；有主人则看主人）是玩家且
///    `MasterGuild == MyGuild || MasterGuild.IsAllyGuild(MyGuild)` ⇒ 非 `LastHiter` 时 `false`；
/// 5. 管理员 / 石化 / `Race ∈ [NPC, Animal)` / 自己 / 同城（`target.Castle == Castle`）⇒ `false`。
///
/// 之后落回尾段（`LastHiter`、弓箭守卫目标、红名玩家、石化）。`Castle == null` 的旧路径
/// 与改造前逐字节一致（判据 C5）。
fn guard_target_is_proper_target(world: &mut World, actor_id: i32, target_id: i32) -> bool {
    let (Some(a), Some(t)) = (world.entities.get(actor_id), world.entities.get(target_id)) else {
        return false;
    };
    if a.id == t.id {
        return false;
    }
    // 只复制标量/`Option`，使城堡分支能改 `World` 侧 `CrimeforCastle` 表
    // （oracle 在 `IsProperTarget` 里就地清 `guardObject.CrimeforCastle`）。
    let actor_castle = a.castle_binding;
    let last_hiter = a.last_hiter;
    let target = GuardTargetSnapshot {
        id: t.id,
        race: t.race,
        master: t.master,
        admin_mode: t.admin_mode,
        stone_mode: t.stone_mode,
        castle: t.castle_binding,
        target_creature: t.ai.target_creature,
        pk_point: t.pk_point,
    };
    // ---- `Castle != null` 分支（`GuardUnit.cs:37-100`）----
    if let Some(binding) = actor_castle {
        if let Some(castle) = world.castles.get(binding.castle_index) {
            let under_war = castle.under_war;
            let master_guild = castle.master_guild;
            // `if (LastHiter == baseObject) return true;`
            if last_hiter == Some(target.id) {
                return true;
            }
            // `if (Castle.UnderWar) return true;`
            if under_war {
                return true;
            }
            // `if (baseObject.Race == ActorRace.Guard) { GuardUnit guardObject = ... }`
            if target.race == ACTOR_RACE_GUARD {
                if let Some(crime_tick) = world.guard_crime_for_castle_tick(target.id) {
                    if world.clock_ms - crime_tick < GUARD_CRIME_FOR_CASTLE_MS {
                        return true;
                    }
                    // 过期：`guardObject.CrimeforCastle = false;`
                    world.clear_guard_crime_for_castle(target.id);
                    // `if (guardObject.Castle != null) { ...; return false; }`
                    if target.castle.is_some() {
                        return false;
                    }
                }
            }
            // `if (Castle.MasterGuild != null) { ... }`
            if let Some(master_guild) = master_guild {
                let (exempt, last_hiter_is_target) = match target.master {
                    // `baseObject.Master == null`：只有玩家本人有 `MyGuild`。
                    None => (
                        target.race == ACTOR_RACE_PLAY
                            && world
                                .castle_exempts_guild(master_guild, world.actor_guild(target.id)),
                        last_hiter == Some(target.id),
                    ),
                    // `baseObject.Master != null`：主人是玩家时读主人的 `MyGuild`。
                    Some(master_id) => {
                        let master_is_play = world
                            .entities
                            .get(master_id)
                            .is_some_and(|m| m.race == ACTOR_RACE_PLAY);
                        let exempt = master_is_play
                            && world
                                .castle_exempts_guild(master_guild, world.actor_guild(master_id));
                        (
                            exempt,
                            last_hiter == Some(master_id) || last_hiter == Some(target.id),
                        )
                    }
                };
                // `if (LastHiter != baseObject / != baseObject.Master) return false;`
                if exempt && !last_hiter_is_target {
                    return false;
                }
            }
            // `if (AdminMode || StoneMode || Race ∈ [NPC, Animal) || baseObject == this ||
            //      baseObject.Castle == Castle) return false;`
            if target.admin_mode
                || target.stone_mode
                || (target.race >= crate::entity::ACTOR_RACE_NPC
                    && target.race < crate::entity::ACTOR_RACE_ANIMAL)
                || target.id == actor_id
                || target.castle.map(|b| b.castle_index) == Some(binding.castle_index)
            {
                return false;
            }
        }
    }
    // ---- 尾段（`GuardUnit.cs:101-124`）----
    if last_hiter == Some(target.id) {
        return true;
    }
    if let Some(tc) = target.target_creature {
        if world.entities.get(tc).is_some_and(|x| x.race == 112) {
            return true;
        }
    }
    if target.race == ACTOR_RACE_PLAY {
        if target.admin_mode {
            return false;
        }
        if target.pk_point / 100 >= 2 {
            return true;
        }
    }
    if target.stone_mode {
        return false;
    }
    true
}

/// `GuardUnit.Struck`（`GuardUnit.cs:27-33`）：`Castle != null` ⇒
/// `CrimeforCastle = true; CrimeforCastleTime = GetTickCount();`。
///
/// 本骨架的 `Castle != null` = `Entity::castle_binding.is_some()`（弓手/门/墙由
/// `UserCastle::Initialize` 绑定；守卫表不绑，与 oracle 同）。
fn guard_unit_struck(world: &mut World, victim_id: i32, now: i64) {
    if world
        .entities
        .get(victim_id)
        .is_some_and(|e| e.castle_binding.is_some())
    {
        world.set_guard_crime_for_castle(victim_id, now);
    }
}

// ---------------------------------------------------------------------------
// **MB-8 / MB-9**（tier=R）——事件 / 出土 / 运动 + `FireballMonster` 全链
//
// oracle 逐句锚点（本片在 `ce61cdca4` 上实读；类链 = `CreateMonster` 的 race → 类映射）：
//
// | 本片 | oracle |
// | --- | --- |
// | [`white_skeleton_run`] | `WhiteSkeleton.cs:29-39`（ctor 常量见 `mongen::apply_monster_class_constants`） |
// | [`soccer_ball_run`] / [`soccer_ball_struck`] | `SoccerBall.cs:19-74` / `:76-86` |
// | [`sand_mob_object_run`] | `SandMobObject.cs:14-57` |
// | [`fire_monster_run`] | `FireMonster.cs:23-82` |
// | [`run_away_run`]（死类） | `RunAway.cs:12-64` |
// | [`fireball_monster_run`] | `FireballMonster.cs:14-62` |
// | [`magic_monster_run`] | `MagicMonster.cs:84-165`（Pass 1） |
// | [`magic_monster_think`] | `MagicMonster.cs:18-45`（**无** HolySeize 门） |
//
// ## 类链（`base.Run()` 的落点，逐类实读）
//
// - `WhiteSkeleton(100) : AtMonster : MonsterObject` ⇒ `ATMonster.Run`（8s/1s 索敌门）→
//   `MonsterObject.Run` ⇒ [`ClassRunBase::Monster`]（索敌门由 [`monster_run`] 的 race 100 分支补）；
// - `SoccerBall(120) : AnimalObject` ⇒ `AnimalObject.Run` ⇒ [`ClassRunBase::Animal`]；
// - `SandMobObject(132) : StickMonster : AnimalObject` ⇒ **每次** `base.Run()` 都因本函数刚刷新
//   `WalkTick` 而早退，只剩 `AnimalObject.Run`；`AttackTarget()` 命中支先跑一次 + 函数尾再跑一次
//   ⇒ [`ClassRunBase::AnimalTwice`] / `Animal` / `None`（目标两轴均 >15 的 `return`）；
// - `FireMonster(214) : MonsterObject` ⇒ `MonsterObject.Run` ⇒ [`ClassRunBase::Monster`]；
// - `FireballMonster(215) : MagicMonster : MonsterObject` ⇒ `MagicMonster.Run`（自带一段 AI）→
//   `MonsterObject.Run` ⇒ [`ClassRunBase::MagicMonster`]（**两段 AI pass**）。
//
// ## 事件 / 帧
//
// - `FireMonster.Run` 的 9 格火墙：`Envir.GetEvent(x,y) == null` 才
//   `new FireBurnEvent(this,x,y,ET_FIRE,FireTime=20000,FireDamage=10)` + `AddEvent`。本仓复用
//   [`World::event_at`] / [`World::add_fire_burn_event`]（**含 `MapEvent.Id == 0` 的
//   「管理器只留第一条」BUG-REPLICA**），不另建存储。
// - `FireballMonster.Run` 的三帧：`RM_SPELL(48,x,y,48)`、`RM_MAGICFIRE(0,MakeWord(2,48),
//   MakeLong(x,y),targetId)`、`SendStruckDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(x,y), 2,
//   targetId, "", 600)`（入**怪自己**的队列；到点语义见
//   [`crate::slave::World::deliver_self_delay_msgs`] 的 `RM_DELAYMAGIC` 支）。
// ---------------------------------------------------------------------------

/// `FireMonster.FireTime`（`FireMonster.cs:12`）= `20 * 1000`。
const FIRE_MONSTER_TIME_MS: i32 = 20 * 1000;
/// `FireMonster.FireDamage`（`FireMonster.cs:16`）= 10。
const FIRE_MONSTER_DAMAGE: i32 = 10;
/// `FireMonster.Run` 的 9 格检查顺序（`FireMonster.cs:31-64`，原文逐条照抄）。
const FIRE_MONSTER_CELLS: [(i32, i32); 9] = [
    (0, -1),
    (0, -2),
    (-1, 0),
    (-2, 0),
    (0, 0),
    (1, 0),
    (2, 0),
    (0, 1),
    (0, 2),
];

/// `WhiteSkeleton.Run`（`WhiteSkeleton.cs:29-39`）：首次 `Run` 只做
/// `BoIsFirst = false; Dir = 5; FixedHideMode = false; SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0)`。
/// **无 RNG、无 `CanMove`/`Death` 门**；`base.Run()` 由 [`monster_class_run`] 的
/// [`ClassRunBase::Monster`] 承接（= `ATMonster.Run` → `MonsterObject.Run`）。
pub fn white_skeleton_run(world: &mut World, actor_id: i32) {
    let first = world.entities.get(actor_id).is_some_and(|e| e.bo_is_first);
    if !first {
        return;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.bo_is_first = false;
        e.ai.dir = 5;
        e.fixed_hide_mode = false;
    }
    let (dir, x, y) = match world.entities.get(actor_id) {
        Some(e) => (e.ai.dir, e.x, e.y),
        None => return,
    };
    // `SendRefMsg(RM_DIGUP, Dir, CurrX, CurrY, 0, "")`（`:36`）。
    send_ref_msg(world, actor_id, RM_DIGUP, i32::from(dir), x, y, 0);
}

/// `SoccerBall.Run`（`SoccerBall.cs:19-74`）。`base.Run()` = `AnimalObject.Run`。
///
/// `N550 > 0`：前格（`GetNextPosition(...,1)`）可走（`CanWalk(...,false)`）⇒ 按反射表把 `Dir`
/// 取反后再 `GetNextPosition(..., N550)` 写 `TargetX/Y`；不可走 ⇒ 不改任何字段。
/// `N550 <= 0` ⇒ `TargetX = -1`。随后 `TargetX != -1` ⇒ `GotoTargetXY()`，到达则 `N550 = 0`。
pub fn soccer_ball_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    let (cx, cy, map_id, dir, n550) = (e.x, e.y, e.map_id, e.ai.dir, e.n550);
    if n550 > 0 {
        let one = world
            .maps
            .get(map_id)
            .and_then(|m| m.get_next_position(cx, cy, dir, 1));
        if let Some((nx, ny)) = one {
            if world.can_walk_at(map_id, nx, ny, false) {
                // `switch (Dir)` 的反射表：0↔4、1↔7、2↔6、3↔5（`SoccerBall.cs:31-56`）。
                let new_dir = soccer_ball_reflect_dir(dir);
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.dir = new_dir;
                }
                // `Envir.GetNextPosition(CurrX, CurrY, Dir, N550, ref TargetX, ref TargetY)`
                // ——`GetNextPosition` 先把 `snx/sny` 置为 `CurrX/CurrY`，故未移动时也要写回。
                let dest = world
                    .maps
                    .get(map_id)
                    .and_then(|m| m.get_next_position(cx, cy, new_dir, n550))
                    .unwrap_or((cx, cy));
                if let Some(e) = world.entities.get_mut(actor_id) {
                    e.ai.target_x = dest.0;
                    e.ai.target_y = dest.1;
                }
            }
        }
    } else if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.target_x = -1;
    }
    let (target_x, target_y) = world
        .entities
        .get(actor_id)
        .map_or((-1, 0), |e| (e.ai.target_x, e.ai.target_y));
    if target_x != -1 {
        goto_target_xy(world, actor_id);
        let (nx, ny) = world.entities.get(actor_id).map_or((0, 0), |e| (e.x, e.y));
        if target_x == nx && target_y == ny {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.n550 = 0;
            }
        }
    }
}

/// `SoccerBall.Run` 的 `Dir` 反射表（`SoccerBall.cs:31-56`，8 向）。
fn soccer_ball_reflect_dir(dir: u8) -> u8 {
    match dir {
        0 => 4,
        1 => 7,
        2 => 6,
        3 => 5,
        4 => 0,
        5 => 3,
        6 => 2,
        7 => 1,
        _ => dir,
    }
}

/// `SoccerBall.Struck`（`SoccerBall.cs:76-86`）——**完全覆写** `AnimalObject.Struck`。
///
/// `hiter == null ⇒ return`；`Dir = hiter.Dir`；`N550 = MIN(20, Random(4) + N550 + 4)`；
/// `GetNextPosition(CurrX, CurrY, Dir, N550, ref TargetX, ref TargetY)`。
/// 不写 `StruckTick`、不选目标、不扣肉质、不加 `AttackTick`（无 `base.Struck()`）。
/// 由 [`animal_struck`] 在 race 120 处转派。
pub fn soccer_ball_struck(world: &mut World, actor_id: i32, hiter_id: i32) {
    let Some(hiter_dir) = world.entities.get(hiter_id).map(|h| h.ai.dir) else {
        return;
    };
    let (cx, cy, map_id, n550) = match world.entities.get(actor_id) {
        Some(e) => (e.x, e.y, e.map_id, e.n550),
        None => return,
    };
    let n550 = world.rng.random_below(4) + n550 + 4;
    let n550 = n550.min(20);
    let dest = world
        .maps
        .get(map_id)
        .and_then(|m| m.get_next_position(cx, cy, hiter_dir, n550))
        .unwrap_or((cx, cy));
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.dir = hiter_dir;
        e.n550 = n550;
        e.ai.target_x = dest.0;
        e.ai.target_y = dest.1;
    }
}

/// `SandMobObject.Run`（`SandMobObject.cs:14-57`）。返回值 = oracle 本 tick 要跑的
/// `base.Run()`（= `StickMonster.Run`，因 `WalkTick` 刚刷新而必然早退成 `AnimalObject.Run`）次数：
///
/// - `AttackTarget()` 命中 ⇒ 体内先跑一次 + 函数尾再一次 ⇒ [`ClassRunBase::AnimalTwice`]；
/// - 目标两轴均 `> 15` 的 `ComeDown(); return;` ⇒ **0 次** ⇒ [`ClassRunBase::None`]；
/// - 其余路径（含 `!Death && !Ghost` 门/`WalkTick` 门为假）⇒ 函数尾一次 ⇒ [`ClassRunBase::Animal`]。
fn sand_mob_object_run(world: &mut World, actor_id: i32) -> ClassRunBase {
    let Some(e) = world.entities.get(actor_id) else {
        return ClassRunBase::Animal;
    };
    // `if (!Death && !Ghost)`（`:16`）：为假 ⇒ 只剩函数尾 `base.Run()`。
    if e.death || e.ghost {
        return ClassRunBase::Animal;
    }
    let now = world.clock_ms;
    // `if ((GetTickCount() - WalkTick) > WalkSpeed)`（`:18`）：**严格大于**。
    if now - e.ai.walk_tick <= e.ai.walk_speed {
        return ClassRunBase::Animal;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
    }
    let (hp, max_hp, fixed_hide, attack_tick, next_hit_time, appear_start, target_id) = {
        let e = match world.entities.get(actor_id) {
            Some(e) => e,
            None => return ClassRunBase::Animal,
        };
        (
            i32::from(e.hp),
            i32::from(e.max_hp),
            e.fixed_hide_mode,
            e.ai.attack_tick,
            e.ai.next_hit_time,
            e.appear_start_tick,
            e.ai.target_creature,
        )
    };
    if fixed_hide {
        // `if (WAbil.HP > WAbil.MaxHP / 20 && CheckComeOut()) _mDwAppearStart = now;`
        if hp > max_hp / 20 && stick_check_come_out(world, actor_id) {
            if let Some(e) = world.entities.get_mut(actor_id) {
                e.appear_start_tick = now;
            }
        }
    } else {
        // `if (HP > 0 && HP < MaxHP / 20 && (now - _mDwAppearStart) > 3000) ComeDown();`
        if hp > 0 && hp < max_hp / 20 && (now - appear_start) > 3000 {
            stick_come_down(world, actor_id);
        } else if let Some(target_id) = target_id {
            // `else if (TargetCret != null) { if (|Δx| > 15 && |Δy| > 15) { ComeDown(); return; } }`
            if let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) {
                let (cx, cy) = world.entities.get(actor_id).map_or((0, 0), |e| (e.x, e.y));
                if (cx - tx).abs() > 15 && (cy - ty).abs() > 15 {
                    stick_come_down(world, actor_id);
                    return ClassRunBase::None;
                }
            }
        }
        // `if ((now - AttackTick) > NextHitTime) SearchTarget();`（`:38-41`）
        if now - attack_tick > next_hit_time {
            search_target(world, actor_id);
        }
        // `if (!FixedHideMode) { if (AttackTarget()) base.Run(); }`（`:42-48`）
        let still_hidden = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.fixed_hide_mode);
        if !still_hidden && stick_attack_target(world, actor_id) {
            return ClassRunBase::AnimalTwice;
        }
    }
    ClassRunBase::Animal
}

/// `FireMonster.Run`（`FireMonster.cs:23-82`）。
///
/// `CanMove()` 门内按 `FIRE_MONSTER_CELLS` 的**固定顺序**检查 9 格：`Envir.GetEvent == null`
/// 才新建 `FireBurnEvent(ET_FIRE, FireTime=20000, FireDamage=10)` 并 `AddEvent`；随后 8s/1s
/// 索敌门。`base.Run()` = `MonsterObject.Run`（[`ClassRunBase::Monster`]）。
pub fn fire_monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if !can_move(e) {
        return;
    }
    let (x, y, map_id) = (e.x, e.y, e.map_id);
    for (dx, dy) in FIRE_MONSTER_CELLS {
        let (ex, ey) = (x + dx, y + dy);
        if world.event_at(map_id, ex, ey).is_none() {
            world.add_fire_burn_event(
                actor_id,
                map_id,
                ex,
                ey,
                FIRE_MONSTER_DAMAGE,
                FIRE_MONSTER_TIME_MS,
            );
        }
    }
    class_seek_gate(world, actor_id);
}

/// `RunAway.Run`（`RunAway.cs:12-64`）——**死类**：`CreateMonster`（`WorldServer.MonGen.cs:507-728`）
/// 无对应 case，ctor 也不自置 `Race` ⇒ 生产不可达；本函数为「内容出现即等价」保留并**登记**。
///
/// - `!Death && !Ghost`；
/// - 有目标 ⇒ 无条件把 `TargetX/TargetY` 写为目标格；
/// - `HP <= Round(MaxHP/2.0)` ⇒ `GetFrontPosition` + `RM_SPACEMOVE_FIRE` +
///   `SpaceMove(MapName, nx-2, ny-2, 0)`，`borunaway = true`；`HP >= Round(MaxHP/2.0)` ⇒ false；
/// - `borunaway` 支里的 `time1` 是**局部变量**、每次 `Run` 置 0 ⇒ `(now - 0) > 5000` 恒等于
///   `now > 5000`（**照抄 oracle 的 bug**，`RunAway.cs:44`/`:48` 的 `RandomByte(4/7)` 条件选择
///   依赖目标是否恰在斜邻格）；
/// - 尾部 8s/1s 索敌门；`base.Run()` = `MonsterObject.Run`。
///
/// `WalkTo(dir, true, BoFearFire)` 的 `boFlag`/`fearFire` 子集与 [`walk_to`] 一致（未建模，登记）。
pub fn run_away_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    if e.death || e.ghost {
        return;
    }
    let now = world.clock_ms;
    if let Some(target_id) = e.ai.target_creature {
        if let Some((tx, ty)) = world.entities.get(target_id).map(|t| (t.x, t.y)) {
            if let Some(a) = world.entities.get_mut(actor_id) {
                a.ai.target_x = tx;
                a.ai.target_y = ty;
            }
            let (hp, max_hp, map_id, dir) = match world.entities.get(actor_id) {
                Some(a) => (i32::from(a.hp), i32::from(a.max_hp), a.map_id, a.ai.dir),
                None => return,
            };
            let half = mir2_shared::hutil::round(f64::from(max_hp) / 2.0);
            if hp <= half {
                let (nx, ny) = front_of(world, actor_id, dir, map_id)
                    .unwrap_or_else(|| world.entities.get(actor_id).map_or((0, 0), |a| (a.x, a.y)));
                send_ref_msg(world, actor_id, RM_SPACEMOVE_FIRE, 0, 0, 0, 0);
                let _ =
                    crate::space_move::monster_space_move(world, actor_id, map_id, nx - 2, ny - 2);
                // `if ((GetTickCount() - time1) > 5000)`：`time1` 是局部 `int`，每次 Run 置 0。
                if now > 5000 {
                    let (tx2, ty2) = world
                        .entities
                        .get(actor_id)
                        .map_or((0, 0), |a| (a.ai.target_x, a.ai.target_y));
                    let (cx2, cy2) = world.entities.get(actor_id).map_or((0, 0), |a| (a.x, a.y));
                    let dir_to = if (tx2 - cx2).abs() == 1 && (ty2 - cy2).abs() == 1 {
                        world.rng.random_byte(4)
                    } else {
                        world.rng.random_byte(7)
                    };
                    walk_to(world, actor_id, dir_to);
                }
            }
        }
    }
    class_seek_gate(world, actor_id);
}

/// `BaseObject.GetPoseCreate()`（`BaseObject.cs:1253-1262`）：`GetFrontPosition` 后取该格
/// 首个 `GetMovingObject(nX, nY, true)`（`!Ghost && !Death`）。
fn get_pose_create(world: &World, actor_id: i32) -> Option<i32> {
    let e = world.entities.get(actor_id)?;
    let map = world.maps.get(e.map_id)?;
    let (nx, ny) = front_position(e.x, e.y, e.ai.dir, map.width, map.height);
    world.moving_object_at(e.map_id, nx, ny, true)
}

/// `FireballMonster.Run`（`FireballMonster.cs:14-62`）——类体（魔法块 + 索敌门）。
///
/// **必须逐条复刻的怪癖**（设计件 `design-monster-run-overrides-20261017.md` §4.1.2）：
/// 1. `MagCanHitTarget(TargetCret.CurrX, TargetCret.CurrY, TargetCret)`——起点 = **目标自身坐标**
///    （`n20 = 0`，第一轮 `GetNextDirection(x,y,x,y)` 兜底 `Down`）；
/// 2. 距离门读 **`TargetX/TargetY` 字段**（不是 `TargetCret.CurrX/CurrY`；初值 -1）；
/// 3. 伤害打 **`GetPoseCreate()`（前方一格）**，帧却引用 `TargetCret` 的坐标/id；
/// 4. `AntiMagic > 0` 是硬门（普通生成怪 `anti_magic = 0` ⇒ 只抽 `nPower` 不进伤害块）；
/// 5. `BreakHolySeizeMode()` 在**有目标**时无条件执行（伤害块全跳过也执行）；
/// 6. `SpellTick` 与 `BaseObject.Run` 的恢复累加器同名共用 ⇒ 到点门实际只挡一 tick。
///
/// `base.Run()` 链（`MagicMonster.Run` → `MonsterObject.Run`）由
/// [`monster_class_run`] 的 [`ClassRunBase::MagicMonster`] 承接。
pub fn fireball_monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    // `if (!Death && !Ghost)`（`:16`）。
    if e.death || e.ghost {
        return;
    }
    let (map_id, cx, cy, target_x, target_y, dc, anti_magic, spell_tick, next_hit_time) = (
        e.map_id,
        e.x,
        e.y,
        e.ai.target_x,
        e.ai.target_y,
        e.dc,
        e.anti_magic,
        e.spell_tick,
        e.ai.next_hit_time,
    );
    let target_id = e.ai.target_creature;
    if let Some(target_id) = target_id {
        // `if (MagCanHitTarget(TargetCret.CurrX, TargetCret.CurrY, TargetCret))`（`:20`）。
        let mag_ok = match world.entities.get(target_id) {
            Some(t) => world
                .maps
                .get(map_id)
                .is_some_and(|m| crate::magic::mag_can_hit_target(m, t.x, t.y, t.x, t.y)),
            None => false,
        };
        if mag_ok {
            // `if (IsProperTarget(TargetCret))`（`:22`）。
            let proper = world
                .entities
                .get(actor_id)
                .zip(world.entities.get(target_id))
                .is_some_and(|(a, t)| is_proper_target(a, t));
            if proper && (target_x - cx).abs() <= 8 && (target_y - cy).abs() <= 8 {
                // `nPower = LoByte(DC) + Random(|HiByte(DC) - LoByte(DC)| + 1)`（`:26`）。
                let lo = i32::from(crate::combat::lo_byte(dc));
                let hi = i32::from(crate::combat::hi_byte(dc));
                let n_power = lo + world.rng.random_below((hi - lo).abs() + 1);
                if n_power > 0 {
                    // `IActor baseObject = GetPoseCreate();`（`:29`）。
                    if let Some(pose_id) = get_pose_create(world, actor_id) {
                        let pose_proper = world
                            .entities
                            .get(actor_id)
                            .zip(world.entities.get(pose_id))
                            .is_some_and(|(a, t)| is_proper_target(a, t));
                        if pose_proper && anti_magic > 0 {
                            // `baseObject.GetMagStruckDamage(this, nPower)`（`:32`）在**受击者
                            // （pose）**上调用 ⇒ MAC / `LifeAttrib` / `AddAbil.UndeadPower` 都取
                            // pose（`:816` 的 `this` 是接收者；参数 `baseObject` 只是攻方，本函数
                            // 不读它的 `undead_power`）。生产怪 `undead_power` 恒 0，方向仍按 oracle。
                            let (mac, undead): (u16, Option<i32>) =
                                world.entities.get(pose_id).map_or((0, None), |p| {
                                    (
                                        p.mac,
                                        if p.life_attrib == 1 {
                                            Some(p.undead_power)
                                        } else {
                                            None
                                        },
                                    )
                                });
                            let reduced = crate::combat::get_mag_struck_damage(
                                &mut world.rng,
                                mac,
                                n_power,
                                undead,
                            );
                            if reduced > 0 {
                                // `baseObject.StruckDamage(nPower)`（`:35`）：倍率 + `DamageHealth`
                                // + 玩家耐久抽（[`class_struck_apply`] 逐行同构）。
                                let _ =
                                    class_struck_apply(world, actor_id, pose_id, reduced, reduced);
                                let now = world.clock_ms;
                                let (spell_tick_now, next_hit) = world
                                    .entities
                                    .get(actor_id)
                                    .map_or((spell_tick, next_hit_time), |a| {
                                        (a.spell_tick, a.ai.next_hit_time)
                                    });
                                if now - i64::from(spell_tick_now) > next_hit {
                                    if let Some(a) = world.entities.get_mut(actor_id) {
                                        a.spell_tick = i32::try_from(now).unwrap_or(i32::MAX);
                                    }
                                    let (tx, ty) = world
                                        .entities
                                        .get(target_id)
                                        .map_or((0, 0), |t| (t.x, t.y));
                                    // `SendRefMsg(RM_SPELL, 48, tx, ty, 48, "")`（`:39`）。
                                    send_ref_msg(world, actor_id, RM_SPELL, 48, tx, ty, 48);
                                    // `SendRefMsg(RM_MAGICFIRE, 0, MakeWord(2, 48),
                                    //   MakeLong(tx, ty), targetId, "")`（`:40`）。
                                    send_ref_msg(
                                        world,
                                        actor_id,
                                        RM_MAGICFIRE,
                                        0,
                                        i32::from(mir2_shared::hutil::make_word(2, 48)),
                                        make_long(tx, ty),
                                        target_id,
                                    );
                                    // `SendStruckDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx, ty),
                                    //   2, targetId, "", 600)`（`:41`）——7 参重载，入**怪自己**的队列。
                                    world.send_struck_delay_msg_7(
                                        actor_id,
                                        crate::slave::RM_DELAYMAGIC,
                                        reduced,
                                        make_long(tx, ty),
                                        2,
                                        target_id,
                                        600,
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }
        // `BreakHolySeizeMode()`（`:49`）：有目标即执行，**无条件**。
        break_holy_seize_mode(world, actor_id);
    }
    // `else { TargetCret = null; }`（`:52-55`）——字段本身已为 `None`，无写点（照抄空操作）。
    // 8s/1s 索敌门（`:56-60`）。
    class_seek_gate(world, actor_id);
}

/// `MagicMonster.Run`（`MagicMonster.cs:84-165`）——**Pass 1**（类体）。
///
/// 与 [`monster_run`]（= `MonsterObject.Run`，本类尾部 `base.Run()` 的 Pass 2）逐句相同的段
/// 直接复用；差异只有三处：
/// 1. **没有** `ATMonster.Run` 的 8s/1s 索敌门（`MagicMonster : MonsterObject`）；
/// 2. `Think()` 是 `MagicMonster.Think`（无 HolySeize 门）——本函数直调 [`magic_monster_think`]；
/// 3. 函数尾**不**跑 `CheckRoyalty()`/`CrazyMode` 到期（那是 Pass 2 =
///    [`monster_run`] 的尾部；oracle 由 `base.Run()` 进入）。
///
/// 所有出口都落 `base.Run()` = `MonsterObject.Run` ⇒ 调用方（[`run_class_base`]）在本函数之后
/// 无条件跑 `monster_run` + `animal_run` + `base_object_run_monster`。
fn magic_monster_run(world: &mut World, actor_id: i32) {
    let Some(e) = world.entities.get(actor_id) else {
        return;
    };
    // `if (CanMove() && !FixedHideMode && !StoneMode)`（`:86`）。
    if !(can_move(e) && !e.fixed_hide_mode && !e.stone_mode) {
        return;
    }
    // `if (Think()) { base.Run(); return; }`（`:88-92`）。
    if magic_monster_think(world, actor_id) {
        return;
    }
    let now = world.clock_ms;
    // `if (WalkWaitLocked) { if ((now - WalkWaitTick) > WalkWait) WalkWaitLocked = false; }`
    let walk_wait_locked = world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.walk_wait_locked);
    if walk_wait_locked
        && world
            .entities
            .get(actor_id)
            .is_some_and(|e| now - e.ai.walk_wait_tick > e.ai.walk_wait)
    {
        if let Some(e) = world.entities.get_mut(actor_id) {
            e.ai.walk_wait_locked = false;
        }
    }
    let walk_due = world
        .entities
        .get(actor_id)
        .is_some_and(|e| !e.ai.walk_wait_locked && (now - e.ai.walk_tick) > e.ai.walk_speed);
    if !walk_due {
        return;
    }
    if let Some(e) = world.entities.get_mut(actor_id) {
        e.ai.walk_tick = now;
        e.ai.walk_count += 1;
        if e.ai.walk_count > e.ai.walk_step {
            e.ai.walk_count = 0;
            e.ai.walk_wait_locked = true;
            e.ai.walk_wait_tick = now;
        }
    }
    let run_away_mode = world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.run_away_mode);
    if !run_away_mode {
        let no_attack_mode = world
            .entities
            .get(actor_id)
            .is_some_and(|e| e.ai.no_attack_mode);
        if !no_attack_mode {
            let target = world
                .entities
                .get(actor_id)
                .and_then(|e| e.ai.target_creature);
            match target {
                Some(_) => {
                    // 虚调用 `AttackTarget()` ⇒ race 215 走 `MagicMonster.AttackTarget`
                    // （[`magic_monster_attack_target`]：只刷 tick、不伤害）。
                    if monster_attack_target(world, actor_id) {
                        return;
                    }
                }
                None => {
                    // `MonsterObject.cs:296-301` 的 Mission 段（`Mission == false` ⇒ `TargetX = -1`）。
                    if let Some(e) = world.entities.get_mut(actor_id) {
                        e.ai.target_x = -1;
                    }
                }
            }
        }
        monster_follow_master(world, actor_id);
    } else {
        monster_run_away_expiry(world, actor_id, now);
    }
    // `if (Master != null && Master.SlaveRelax) { base.Run(); return; }`
    let slave_relax = world
        .entities
        .get(actor_id)
        .and_then(|e| e.master)
        .and_then(|m| world.entities.get(m))
        .is_some_and(|m| m.slave_relax);
    if slave_relax {
        return;
    }
    let target_x = world.entities.get(actor_id).map_or(-1, |e| e.ai.target_x);
    let has_target = world
        .entities
        .get(actor_id)
        .is_some_and(|e| e.ai.target_creature.is_some());
    if target_x != -1 {
        goto_target_xy(world, actor_id);
    } else if !has_target {
        wondering(world, actor_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{VisibleBaseObject, VisibleFlag, ACTOR_RACE_MONSTER};
    use crate::world::World;
    use mir2_shared::rng::RandomNumber;

    fn monster(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "稻草人", x, y);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 60;
        e.max_hp = 60;
        world.add_entity(e)
    }

    fn player(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "m2r", x, y);
        e.hp = 100;
        e.max_hp = 100;
        world.add_entity(e)
    }

    /// 找一个「首抽 `Random(20)` 为 `want_zero`、且（可选）次抽 `Random(4)` 为 `want4`」的种子。
    fn seed_with_gate(want_zero: bool, want4: Option<i32>) -> i32 {
        for seed in 0..100_000 {
            let mut r = RandomNumber::with_seed(seed);
            let first = r.random_below(20);
            if (first == 0) != want_zero {
                continue;
            }
            if let Some(w4) = want4 {
                if r.random_below(4) != w4 {
                    continue;
                }
            }
            return seed;
        }
        unreachable!("应能在 10 万种子里找到目标门取值");
    }

    /// oracle `Wondering`：`Random(20) != 0` ⇒ **只抽这一次**、不动不转（`AnimalObject.cs:425-428`）。
    #[test]
    fn wondering_gate_nonzero_consumes_exactly_one_draw_no_move() {
        let seed = seed_with_gate(false, None);
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let first = probe.random_below(20);
        assert_ne!(first, 0);
        let probe_next = probe.random();
        let before = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y, e.ai.dir)
        };
        wondering(&mut world, id);
        let after = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y, e.ai.dir)
        };
        assert_eq!(before, after, "非 0 支不应移动/转向");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非 0 支必须只消耗一次 Random(20)"
        );
    }

    /// oracle `Wondering`：`Random(20)==0 && Random(4)==1` ⇒ `RandomByte(8)` 后 `TurnTo`
    /// （`AnimalObject.cs:430-435`）。取数次序：Random(20) → Random(4) → RandomByte(8)。
    #[test]
    fn wondering_turn_branch_matches_oracle_draw_order() {
        let seed = seed_with_gate(true, Some(1));
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        assert_eq!(expect.random_below(20), 0);
        assert_eq!(expect.random_below(4), 1);
        let dir = expect.random_byte(8);
        let (x, y) = {
            let e = world.entities.get(id).unwrap();
            (e.x, e.y)
        };
        wondering(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.dir, dir, "应 TurnTo(RandomByte(8))");
        assert_eq!((e.x, e.y), (x, y), "TurnTo 分支不移动");
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "取数次序须为 Random(20)→Random(4)→RandomByte(8)"
        );
    }

    /// oracle `Wondering`：`Random(20)==0 && Random(4)!=1` ⇒ `WalkTo(Dir)`（`AnimalObject.cs:437`）。
    #[test]
    fn wondering_walk_branch_moves_and_keeps_dir() {
        let seed = seed_with_gate(true, Some(0));
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        assert_eq!(expect.random_below(20), 0);
        assert_eq!(expect.random_below(4), 0);
        let before_dir = world.entities.get(id).unwrap().ai.dir;
        wondering(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.dir, before_dir, "WalkTo(Dir) 不转向");
        // dir=0（Up）⇒ y 减一（地图空旷）。
        assert_eq!((e.x, e.y), (10, 9), "WalkTo(Up) 应向北走一格");
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "WalkTo 分支不再抽随机数"
        );
    }

    /// `SearchTarget`：可见集里按曼哈顿距离选最近、且被 `IsProperTarget` 承认的目标；
    /// 死亡/超距者从 `VisibleActors` 清除（`AnimalObject.cs:388-423`）。
    #[test]
    fn search_target_picks_nearest_and_clears_invalid() {
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let near = player(&mut world, 12, 10);
        let far = player(&mut world, 20, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        {
            let e = world.entities.get_mut(mid).unwrap();
            e.visible_actors = vec![
                VisibleBaseObject {
                    target: far,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: dead,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: far,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: near,
                    flag: VisibleFlag::Show,
                },
            ];
        }
        search_target(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(near),
            "应选曼哈顿距离最近的合法目标"
        );
        let vis = world.entities.get(mid).unwrap().visible_actors.clone();
        assert!(vis.iter().all(|v| v.target != dead), "死亡目标应被清除");
        assert_eq!(
            vis.iter().filter(|v| v.target == far).count(),
            2,
            "重复可见项保留"
        );
    }

    /// **reviewer-6 的 Important 收口（`AnimalObject.cs:402`）**：`SearchTarget` 的隐藏门读的是
    /// `HideMode`，**不是** `FixedHideMode`。
    ///
    /// - `hide_mode=true && cool_eye=false` ⇒ 不得被选为目标（oracle 的 `(!HideMode || CoolEye)` 不成立）；
    /// - `hide_mode=false && fixed_hide_mode=true` ⇒ **可以**被选为（oracle 该函数不看 `FixedHideMode`）；
    /// - `hide_mode=true && cool_eye=true` ⇒ 可以被选（`CoolEye` 破隐身）。
    ///
    /// 红检：把门改回 `fixed_hide_mode` ⇒ 第 1/3 条断言必红。
    #[test]
    fn search_target_hide_gate_reads_hide_mode_not_fixed_hide_mode() {
        fn run(hide_mode: bool, fixed: bool, cool_eye: bool) -> (i32, Option<i32>) {
            let mut world = World::new(60, 60);
            let mid = monster(&mut world, 10, 10);
            world.entities.get_mut(mid).unwrap().ai.cool_eye = cool_eye;
            let target = player(&mut world, 12, 10);
            {
                let t = world.entities.get_mut(target).unwrap();
                t.hide_mode = hide_mode;
                t.fixed_hide_mode = fixed;
            }
            world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
                target,
                flag: VisibleFlag::Show,
            }];
            search_target(&mut world, mid);
            (target, world.entities.get(mid).unwrap().ai.target_creature)
        }
        assert_eq!(run(true, true, false).1, None, "HideMode 挡住：不得选中");
        let (target, picked) = run(false, true, false);
        assert_eq!(
            picked,
            Some(target),
            "FixedHideMode 不属该门 ⇒ 照选（与 oracle 1:1）"
        );
        let (target, picked) = run(true, true, true);
        assert_eq!(picked, Some(target), "CoolEye 破 HideMode");
    }

    /// 审计 G05/G20 / MCS-A1+A2：`ClearTargetCreat` 的 `RemoveAt` + `continue`（⇒ `i++`）
    /// 跳项怪癖：四种组合 + 连续无效项 + 「跳过后选中更远者」。
    ///
    /// oracle `AnimalObject.SearchTarget`（`:392-416`）：无效项 ⇒ `ClearTargetCreat`
    /// （删**首个**匹配项 + `DelTargetCreat`）⇒ `continue` ⇒ C# `for` 尾 `i++` **跳过**
    /// 因删除而前移的下一项；收尾只有 `searchTarget != null` 才 `SetTargetCreat`。
    /// **照抄怪癖**（硬约束③）。
    ///
    /// 改坏必红：把删除改回 `retain`/`while` 重扫 ⇒「[无效, 合法] 保持 None」「连续无效项
    /// 第二个仍留在列表」「跳过后选中更远者」三组断言全红。
    #[test]
    fn search_target_clear_target_quirk_four_combinations() {
        // 返回 (最终目标, 剩余可见集, target_x, target_y, 合法者 id)。
        fn case(has_initial: bool, invalid_first: bool) -> (Option<i32>, Vec<i32>, i32, i32, i32) {
            let mut world = World::new(60, 60);
            let mid = monster(&mut world, 10, 10);
            let dead = player(&mut world, 11, 10);
            world.entities.get_mut(dead).unwrap().death = true;
            let legal = player(&mut world, 12, 10);
            let old = player(&mut world, 13, 10);
            let order: [i32; 2] = if invalid_first {
                [dead, legal]
            } else {
                [legal, dead]
            };
            world.entities.get_mut(mid).unwrap().visible_actors = order
                .iter()
                .map(|&target| VisibleBaseObject {
                    target,
                    flag: VisibleFlag::Show,
                })
                .collect();
            if has_initial {
                set_target_creature(&mut world, mid, old);
                let e = world.entities.get_mut(mid).unwrap();
                e.ai.target_x = 42;
                e.ai.target_y = 43;
            }
            search_target(&mut world, mid);
            let e = world.entities.get(mid).unwrap();
            (
                e.ai.target_creature,
                e.visible_actors.iter().map(|v| v.target).collect(),
                e.ai.target_x,
                e.ai.target_y,
                legal,
            )
        }

        // ① 初始无目标 + `[无效, 合法]` ⇒ 无效项删掉后 `i++` 跳过合法项 ⇒ 保持 None。
        let (target, vis, _, _, legal) = case(false, true);
        assert_eq!(
            target, None,
            "[无效, 合法] + 初始无目标 ⇒ oracle 跳项后保持无目标"
        );
        assert_eq!(vis, vec![legal], "无效项仍被 ClearTargetCreat 移出可见集");

        // ② 初始无目标 + `[合法, 无效]` ⇒ 合法者已入选，收尾 SetTargetCreat ⇒ Some(合法)。
        let (target, vis, tx, ty, legal) = case(false, false);
        assert_eq!(target, Some(legal), "[合法, 无效] ⇒ 收尾仍置合法目标");
        assert_eq!(vis, vec![legal]);
        assert_eq!((tx, ty), (-1, -1), "无效项触发过 DelTargetCreat ⇒ x/y=-1");

        // ③ 初始有目标 + `[无效, 合法]` ⇒ 无效项清掉旧目标，合法项被跳过 ⇒ None。
        let (target, _, tx, ty, _) = case(true, true);
        assert_eq!(
            target, None,
            "初始有目标也会被无效项的 DelTargetCreat 清掉，且合法项仍被跳过"
        );
        assert_eq!((tx, ty), (-1, -1));

        // ④ 初始有目标 + `[合法, 无效]` ⇒ 旧目标先被清、收尾置合法 ⇒ Some(合法)。
        let (target, _, tx, ty, legal) = case(true, false);
        assert_eq!(target, Some(legal));
        assert_eq!((tx, ty), (-1, -1));
    }

    /// G20：连续无效项时 `RemoveAt(pos)` 后 `i++` 只跳过一项 ⇒ 第二个无效项**留在**列表。
    /// 另证「被跳过的合法项不会被选中」：`[无效, 近合法, 远合法]` ⇒ 选远者。
    #[test]
    fn search_target_remove_at_skips_shifted_item() {
        // `[无效A, 无效B, 合法]` ⇒ A 删后跳过已前移的 B。
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let dead_a = player(&mut world, 11, 10);
        let dead_b = player(&mut world, 12, 10);
        let legal = player(&mut world, 13, 10);
        for id in [dead_a, dead_b] {
            world.entities.get_mut(id).unwrap().death = true;
        }
        world.entities.get_mut(mid).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: dead_a,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: dead_b,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: legal,
                flag: VisibleFlag::Show,
            },
        ];
        search_target(&mut world, mid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(e.ai.target_creature, Some(legal));
        assert_eq!(
            e.visible_actors
                .iter()
                .map(|v| v.target)
                .collect::<Vec<_>>(),
            vec![dead_b, legal],
            "删 index 0 后 i++ ⇒ 第二个无效项被跳过并留在列表（oracle 怪癖）"
        );

        // 同一个无效目标在列表中出现两次：`ClearTargetCreat` 的 `RemoveAt(i); break;`
        // **只删首个匹配**。`[死, 死, 合法]` ⇒ 删 index 0 后 i++ 跳过剩下的死项 ⇒
        // 选中合法者且列表仍留一个死项（`retain` 会全删 ⇒ target=None，必红）。
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        let legal = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: dead,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: dead,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: legal,
                flag: VisibleFlag::Show,
            },
        ];
        search_target(&mut world, mid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(e.ai.target_creature, Some(legal));
        assert_eq!(
            e.visible_actors
                .iter()
                .map(|v| v.target)
                .collect::<Vec<_>>(),
            vec![dead, legal],
            "RemoveAt 只删首个匹配 ⇒ 剩余一个死项"
        );

        // `[无效, 近合法, 远合法]` ⇒ 近合法被跳过，选远合法。
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        let near = player(&mut world, 12, 10);
        let far = player(&mut world, 20, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: dead,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: near,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: far,
                flag: VisibleFlag::Show,
            },
        ];
        search_target(&mut world, mid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.target_creature,
            Some(far),
            "删无效项后 i++ 跳过已前移的 near ⇒ 选中 far（照抄怪癖）"
        );
    }

    /// G09：索敌路径用**直通版** `IsProperTarget` —— 隐身（且非 HideMode）目标可被选中；
    /// 折入版 [`crate::combat::is_proper_target_grouped`] 仍保留给枚举/显示型调用点。
    #[test]
    fn search_target_uses_direct_proper_target_and_keeps_invisible() {
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let target = player(&mut world, 12, 10);
        world.entities.get_mut(target).unwrap().invisible = true;
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target,
            flag: VisibleFlag::Show,
        }];
        assert!(
            !crate::combat::is_proper_target_grouped(
                world.entities.get(mid).unwrap(),
                world.entities.get(target).unwrap(),
                &[]
            ),
            "折入版（显示路径）仍拒隐身目标"
        );
        search_target(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(target),
            "oracle SearchTarget → IsProperTarget 不查 Invisible ⇒ 必须选中"
        );
    }

    /// G09/G25：`MonsterObject.Think` 的目标有效性刷新用**直通版** —— 隐身目标不被清。
    #[test]
    fn think_keeps_invisible_target_like_oracle_is_proper_target() {
        let mut world = World::new(60, 60);
        let mid = monster(&mut world, 10, 10);
        let target = player(&mut world, 12, 10);
        world.entities.get_mut(target).unwrap().invisible = true;
        set_target_creature(&mut world, mid, target);
        world.clock_ms = 4000; // `now - ThinkTick(0) > 3000`
        let _ = think(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(target),
            "oracle Think → IsProperTarget 不查 Invisible ⇒ 不得清目标"
        );
    }

    /// G09：`AnimalObject.Struck` 的 `IsProperTarget(hiter)` 用**直通版** —— 隐身命中者
    /// 仍会被登记为目标（初始无目标 ⇒ `take` 短路为真）。
    #[test]
    fn animal_struck_accepts_invisible_hitter() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 80, 10, 10);
        let hiter = player(&mut world, 20, 20);
        world.entities.get_mut(hiter).unwrap().invisible = true;
        animal_struck(&mut world, mid, hiter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(hiter),
            "oracle Struck → IsProperTarget 不查 Invisible ⇒ 仍登记命中者"
        );
    }

    /// `GotoTargetXY`：进函数先 `WalkTo(主方向)`，**无条件**抽一次 `Random(3)`；
    /// 移动成功 ⇒ 不再走重试循环（`AnimalObject.cs:174-249`）。
    #[test]
    fn goto_target_xy_walks_and_consumes_one_random3() {
        let mut world = World::new(30, 30);
        let id = monster(&mut world, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.target_x = 10;
            e.ai.target_y = 15; // 正下方 ⇒ 主方向 Down(4)
        }
        let mut expect = RandomNumber::with_seed(1234);
        world.rng = RandomNumber::with_seed(1234);
        let _ = expect.random_below(3);
        goto_target_xy(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.x, e.y), (10, 11), "应由 Down 走到 (10,11)");
        assert_eq!(e.ai.dir, DIR_DOWN);
        // 移动成功后循环体不再抽（oracle 的 Random(3) 只在进函数时抽一次）。
        assert_eq!(world.rng.random(), expect.random());
    }

    /// `Think`：3s 门未到 ⇒ 不抽不判；到点且同格对象 ≥2 ⇒ `DupMode=true`，
    /// 且同一次调用里的 `DupMode` 支 `WalkTo(RandomByte(8))` 被边界拦住（不移动）⇒ 返回 false。
    #[test]
    fn think_sets_dup_mode_when_cell_has_two_objects() {
        // 1×1 地图：四周越界 ⇒ `WalkTo` 不可能成功，`DupMode` 保持。
        let mut world = World::new(1, 1);
        let id = monster(&mut world, 0, 0);
        let _other = player(&mut world, 0, 0);
        assert!(world.enter_world(id));
        assert!(world.enter_world(_other));
        world.entities.get_mut(id).unwrap().ai.think_tick = 0;
        world.clock_ms = 5000;
        assert!(
            !think(&mut world, id),
            "边界拦住的 DupMode 移动不应返回 true"
        );
        assert!(world.entities.get(id).unwrap().ai.dup_mode);
    }

    /// 顺序判据：`monster_ai_tick` 按实体槽位顺序处理，同种子两次世界状态逐字节一致。
    #[test]
    fn monster_ai_tick_is_slot_ordered_and_deterministic() {
        fn run() -> Vec<(i32, i32, i32, u8)> {
            let mut world = World::new(30, 30);
            let _p = player(&mut world, 5, 5);
            for (i, (x, y)) in [(10, 10), (12, 12), (20, 20)].into_iter().enumerate() {
                let mut e = Entity::new(0, format!("m{i}"), x, y);
                e.race = ACTOR_RACE_MONSTER;
                e.hp = 60;
                e.max_hp = 60;
                world.add_entity(e);
            }
            world.rng = RandomNumber::with_seed(99);
            for _ in 0..20 {
                world.clock_ms += 200;
                monster_ai_tick(&mut world);
            }
            let mut out: Vec<(i32, i32, i32, u8)> = world
                .entities
                .iter()
                .map(|e| (e.id, e.x, e.y, e.ai.dir))
                .collect();
            out.sort_unstable();
            out
        }
        assert_eq!(run(), run(), "同种子两次运行必须逐字节一致");
    }

    /// **缺省关闭**：`World::tick` 在未 `enable_monster_ai()` 时不得驱动 AI、不得消耗额外随机数；
    /// 显式开启后必须推进随机流（gate 是 load-bearing，也是既有 golden 不被移位的前提）。
    #[test]
    fn monster_ai_is_opt_in_and_off_by_default() {
        fn run(enable: bool) -> (Vec<(i32, i32, u8)>, i32) {
            let mut world = World::new(30, 30);
            let _id = monster(&mut world, 10, 10);
            world.rng = RandomNumber::with_seed(99);
            if enable {
                world.enable_monster_ai();
            }
            assert_eq!(world.monster_ai_enabled(), enable);
            for _ in 0..30 {
                world.tick();
            }
            let state = world
                .entities
                .iter()
                .map(|e| (e.x, e.y, e.ai.dir))
                .collect();
            (state, world.rng.random())
        }
        let (off_state, off_rng) = run(false);
        let (on_state, on_rng) = run(true);
        assert_eq!(off_state, vec![(10, 10, 0)], "默认关 ⇒ 怪物不被 AI 移动");
        assert_ne!(
            on_rng, off_rng,
            "开启 AI 必须消耗/推进随机流（否则 gate 形同虚设）"
        );
        let _ = on_state;
    }

    /// `BaseObject.Run` 恢复段（`BaseObject.Base.cs:39-66`）：`HealthTick` 到 `HealthFillTime`
    /// ⇒ `HP += MaxHP / 75 + 1`（截断到 `MaxHP`）并把 `HealthTick` 归零。
    /// 改坏必红：去掉 `health_tick >= health_fill_time` 门（无条件回血）⇒ 本用例第一个断言红。
    #[test]
    fn base_recovery_heals_hp_at_fill_time_and_resets_tick() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 10;
        e.max_hp = 75;
        e.health_tick = 0;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        // 未到点：`(5980 - 0) / 20 = 299` ⇒ `HealthTick = 299 < 300` ⇒ 不回血。
        world.clock_ms = 5_980;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 10, "未到 HealthFillTime 不得回血");
        assert_eq!(e.health_tick, 299, "HealthTick 仍按时间累加");
        // 到点：`(6000 - 5980) / 20 = 1` ⇒ `HealthTick = 300 >= 300` ⇒ `HP += 75/75+1 = 2` 后归零。
        world.clock_ms = 6_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 12, "回血量 = MaxHP/75 + 1");
        assert_eq!(e.health_tick, 0, "到点后 HealthTick 归零");
    }

    /// 恢复段截断：`(HP + n) < MaxHP` 不成立 ⇒ 跳到 `MaxHP`（`BaseObject.Base.cs:51-54`）。
    #[test]
    fn base_recovery_caps_at_max_hp() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 74;
        e.max_hp = 75;
        e.health_tick = 300;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id);
        assert_eq!(world.entities.get(id).unwrap().hp, 75, "HP 截断到 MaxHP");
    }

    /// 恢复段在 `u16` 边界上的加法必须按 oracle 的 `int` 语义（`BaseObject.Base.cs:47-55/57-65`：
    /// C# 的 `ushort + ushort` 提升为 `int` ⇒ 不会在 65536 处回绕/panic）。HP/MP 两支都盖。
    /// 改坏必红：把本段改回 `hp + n`（u16 加）⇒ 本用例 debug `attempt to add with overflow`
    /// panic / release 回绕为 0。
    #[test]
    fn base_recovery_high_max_hp_does_not_overflow_u16() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "魔龙教主", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.max_hp = 65_535;
        // n = 65535/75 + 1 = 874 ⇒ hp + n = 66374 > u16::MAX（旧 u16 写法在此回绕/panic）。
        e.hp = 65_500;
        e.health_tick = 300;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().hp,
            65_535,
            "溢出带内应截断到 MaxHP 而不是回绕为 0"
        );
        // MP 支同理：n = 65535/18 + 1 = 3641 ⇒ mp + n = 69141 > u16::MAX。
        let mut e = Entity::new(1, "魔龙教主", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.max_mp = 65_535;
        e.mp = 65_500;
        e.spell_tick = 800;
        e.auto_recovery_tick = 0;
        let id2 = world.add_entity(e);
        base_object_run_monster(&mut world, id2);
        assert_eq!(
            world.entities.get(id2).unwrap().mp,
            65_535,
            "MP 溢出带内应截断到 MaxMP"
        );
    }

    /// 恢复段 MP 支（`BaseObject.Base.cs:57-65`）：`MaxMP/18 + 1`。
    #[test]
    fn base_recovery_heals_mp_at_spell_fill_time() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 75;
        e.max_hp = 75;
        e.mp = 0;
        e.max_mp = 18;
        e.spell_tick = 795;
        e.auto_recovery_tick = 0;
        let id = world.add_entity(e);
        world.clock_ms = 100; // recovery = 5 ⇒ SpellTick = 800 >= 800
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.mp, 2, "回蓝量 = MaxMP/18 + 1");
        assert_eq!(e.spell_tick, 0, "到点后 SpellTick 归零");
    }

    /// 体力点段（`BaseObject.Base.cs:105-110`）：`HealthTick < -HealthFillTime && HP > 1`
    /// ⇒ `HP -= 1` 且 `HealthTick += HealthFillTime`；`HP == 1` 时被 `HP > 1` 门挡住。
    /// 改坏必红：拆掉 `hp > 1` 门 ⇒ 第二个断言红。
    #[test]
    fn health_tick_negative_costs_one_hp_and_is_gated() {
        let mut world = World::new(32, 32);
        world.set_recovery_fill_times(300, 800);
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 5;
        e.max_hp = 60;
        e.health_tick = -301;
        let id = world.add_entity(e);
        base_object_run_monster(&mut world, id); // clock 0 ⇒ recovery 0
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.hp, 4, "负体力点扣 1 点 HP");
        assert_eq!(e.health_tick, -1, "HealthTick 回填 HealthFillTime");
        // `HP == 1` ⇒ `HP > 1` 门挡住。
        let mut e = Entity::new(0, "稻草人", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.hp = 1;
        e.max_hp = 60;
        e.health_tick = -301;
        let id2 = world.add_entity(e);
        base_object_run_monster(&mut world, id2);
        let e = world.entities.get(id2).unwrap();
        assert_eq!(e.hp, 1, "HP == 1 时不得再扣");
        assert_eq!(e.health_tick, -301, "被门挡住 ⇒ HealthTick 不回填");
    }

    /// `BaseObject.Run` 的 `TargetCret` 清理段（`BaseObject.Base.cs:111-118`）四种失效条件。
    #[test]
    fn target_cret_cleanup_covers_age_death_map_and_distance() {
        let build = |clock: i64, tx: i32, ty: i32, kill: bool, tmap: usize| {
            let mut world = World::new(64, 64);
            world.clock_ms = 0;
            let id = monster(&mut world, 10, 10);
            let tid = player(&mut world, tx, ty);
            if tmap != 0 {
                world.entities.get_mut(tid).unwrap().map_id = tmap;
            }
            if kill {
                world.entities.get_mut(tid).unwrap().death = true;
            }
            set_target_creature(&mut world, id, tid);
            world.clock_ms = clock;
            base_object_run_monster(&mut world, id);
            world.entities.get(id).unwrap().ai.target_creature
        };
        assert!(
            build(0, 11, 10, false, 0).is_some(),
            "同图近距且未过期 ⇒ 保留"
        );
        assert!(build(30_001, 11, 10, false, 0).is_none(), "30s 过期 ⇒ 清理");
        assert!(build(0, 11, 10, true, 0).is_none(), "目标死亡 ⇒ 清理");
        assert!(build(0, 11, 10, false, 1).is_none(), "异图 ⇒ 清理");
        assert!(build(0, 26, 10, false, 0).is_none(), "|Δx| > 15 ⇒ 清理");
        assert!(build(0, 11, 26, false, 0).is_none(), "|Δy| > 15 ⇒ 清理");
    }

    /// M-1：清理段走 oracle 的 `ClearTargetCreat`（`BaseObject.Base.cs:568-585`）——
    /// 除 `DelTargetCreat` 外还要把目标从 `VisibleActors` 移除。
    /// 改坏必红：把 `clear_target_creature` 换回 `del_target_creature` ⇒ 第二个断言红。
    #[test]
    fn target_cret_cleanup_also_removes_visible_actor() {
        let mut world = World::new(64, 64);
        world.clock_ms = 0;
        let id = monster(&mut world, 10, 10);
        let tid = player(&mut world, 11, 10);
        world.entities.get_mut(id).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: tid,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: 999,
                flag: VisibleFlag::Show,
            },
        ];
        set_target_creature(&mut world, id, tid);
        world.clock_ms = 30_001; // 过期 ⇒ 清理
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.ai.target_creature.is_none(), "目标已清理");
        assert!(
            !e.visible_actors.iter().any(|v| v.target == tid),
            "目标应从 VisibleActors 移除（ClearTargetCreat）"
        );
        assert!(
            e.visible_actors.iter().any(|v| v.target == 999),
            "其它可见体不受影响"
        );
    }

    // ------------------------------------------------------------------
    // W4：怪物 → 玩家攻击判定链（AnimalObject.Struck / MonsterObject.AttackTarget）
    //     oracle：AnimalObject.cs:334-360、MonsterObject.cs:220-245、BaseObject.Attack.cs:37-64
    // ------------------------------------------------------------------

    /// 找一个「首抽 `Random(6)` 为 `want_zero`」的种子。
    fn seed_random6(want_zero: bool) -> i32 {
        for seed in 0..100_000 {
            let mut r = RandomNumber::with_seed(seed);
            if (r.random_below(6) == 0) == want_zero {
                return seed;
            }
        }
        unreachable!("应能在 10 万种子里找到 Random(6) 门取值");
    }

    /// W4-A：`AnimalObject.Struck`（`AnimalObject.cs:334-360`）的**索敌支**——
    /// `TargetCret == null` ⇒ 短路**不抽** `Random(6)`；`IsProperTarget(hiter)` 成立即认领；
    /// 尾 `AttackTick += 150 - MIN(130, Level*4)`。
    /// 改坏必红：删掉认领支（或 `base_object_run_monster` 的队列出队）⇒ 第一个断言红。
    #[test]
    fn animal_struck_acquires_hitter_without_target_and_bumps_attack_tick() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        world.entities.get_mut(mid).unwrap().level = 1;
        let pid = player(&mut world, 11, 10);
        world.rng = RandomNumber::with_seed(1234);
        let mut probe = RandomNumber::with_seed(1234);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.target_creature,
            Some(pid),
            "无目标 ⇒ 认领命中者（Struck 索敌支）"
        );
        assert_eq!(e.ai.attack_tick, 146, "AttackTick += 150 - min(130, 1*4)");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "TargetCret==null ⇒ 不得抽 Random(6)"
        );
    }

    /// W4-B：`TargetCret != null` 且**邻接** ⇒ `GetAttackDir` 真 ⇒ 短路**不抽** `Random(6)`。
    #[test]
    fn animal_struck_adjacent_target_takes_without_drawing() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let old = player(&mut world, 11, 10); // 邻接
        let hitter = player(&mut world, 12, 10);
        set_target_creature(&mut world, mid, old);
        world.rng = RandomNumber::with_seed(7);
        let mut probe = RandomNumber::with_seed(7);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(hitter)
        );
        assert_eq!(world.rng.random(), probe_next, "邻接支不得抽 Random(6)");
    }

    /// W4-C：`TargetCret != null` 且**不邻接** ⇒ 恰好抽 **1 次** `Random(6)`：
    /// `== 0` 认领命中者、`!= 0` 保持旧目标。取数次序/次数须与 oracle 逐次一致。
    /// 改坏必红：把 `get_attack_dir(...).is_some() || ...` 短路改反（无条件抽/不抽）⇒ 次数断言红。
    #[test]
    fn animal_struck_draws_random6_only_when_target_present_and_far() {
        // Random(6) == 0 ⇒ take=true ⇒ 目标换成命中者，恰好抽 1 次。
        let seed = seed_random6(true);
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let far = player(&mut world, 20, 20); // 非邻接
        let hitter = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, far);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_eq!(probe.random_below(6), 0);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(hitter),
            "Random(6)==0 ⇒ 认领命中者"
        );
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非邻接支恰好抽 1 次 Random(6)"
        );

        // Random(6) != 0 ⇒ take=false ⇒ 保持旧目标，仍恰好抽 1 次。
        let seed = seed_random6(false);
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let far = player(&mut world, 20, 20);
        let hitter = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, far);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_ne!(probe.random_below(6), 0);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, hitter);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(far),
            "Random(6)!=0 ⇒ 保持旧目标"
        );
        assert_eq!(
            world.rng.random(),
            probe_next,
            "非邻接支恰好抽 1 次 Random(6)"
        );
    }

    /// W4-D + G09：`IsProperTarget(hiter)` 门（`AnimalObject.cs:344-347`）——oracle
    /// `BaseObject.IsAttackTarget`（`BaseObject.Base.cs:382-385`）只拒 `AdminMode/StoneMode`，
    /// **不拒 `Death/Ghost/Invisible`**（审计 G09）。
    /// 改坏必红：删掉直通门 ⇒ AdminMode 断言红；改回折入版 ⇒ Death 断言红。
    #[test]
    fn animal_struck_proper_target_gate_matches_oracle() {
        // `AdminMode` 命中者 ⇒ `IsAttackTarget` false ⇒ 不认领（照旧）。
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let admin = player(&mut world, 11, 10);
        world.entities.get_mut(admin).unwrap().admin_mode = true;
        world.rng = RandomNumber::with_seed(3);
        let mut probe = RandomNumber::with_seed(3);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, admin);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "AdminMode 命中者不得被认领"
        );
        assert_eq!(world.rng.random(), probe_next, "TargetCret==null 支不抽数");

        // `Death` 命中者 ⇒ oracle `IsProperTarget` 仍为 true ⇒ **认领**（G09）。
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let dead = player(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        animal_struck(&mut world, mid, dead);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(dead),
            "oracle IsProperTarget 不查 Death ⇒ 仍认领（折入版会误拒）"
        );
    }

    /// W4-E：`RM_STRUCK` 队列（`AnimalState::pending_struck`）在怪物自己的 `base.Run()`
    /// （[`base_object_run_monster`]）里按**入队序**出队；已消失的命中者跳过。
    /// 改坏必红：删掉出队循环 ⇒ 第一个断言红。
    #[test]
    fn base_object_run_monster_drains_struck_queue_in_order() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        let gone = player(&mut world, 12, 10);
        let live = player(&mut world, 11, 10);
        world.entities.remove(gone);
        world.entities.get_mut(mid).unwrap().ai.pending_struck = vec![
            crate::entity::StruckMsg {
                hiter: gone,
                w_param: 0,
            },
            crate::entity::StruckMsg {
                hiter: live,
                w_param: 0,
            },
        ];
        base_object_run_monster(&mut world, mid);
        let e = world.entities.get(mid).unwrap();
        assert!(e.ai.pending_struck.is_empty(), "队列应被出队清空");
        assert_eq!(
            e.ai.target_creature,
            Some(live),
            "已消失的命中者跳过，活的命中者被认领"
        );
    }

    /// W4-F：`MonsterObject.AttackTarget`（`MonsterObject.cs:220-245`）——邻接且到点
    /// （`now - AttackTick > NextHitTime`）才对目标 `_Attack`；结算落进 `pending_ai_attacks`
    /// （宿主转抄 `TickRecord::attacks` ⇒ `drain_combat_downlink` ① 出 `SM_STRUCK`）。
    /// 改坏必红：去攻击间隔门 ⇒ 首个断言（未到点不得出记录）红；去记录支 ⇒ 末条断言红。
    #[test]
    fn monster_attack_target_gates_on_interval_and_records_attack() {
        let mut world = World::new(30, 30);
        let mid = monster(&mut world, 10, 10);
        {
            let m = world.entities.get_mut(mid).unwrap();
            m.dc = 10 | (10 << 8); // lo=10,hi=10 ⇒ power 10
            m.level = 1;
            m.ai.attack_tick = 0;
        }
        let pid = player(&mut world, 11, 10);
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 1000;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ 返回 true");
        assert!(
            world.pending_ai_attacks.is_empty(),
            "到点门未过（1000-0=1000 <= 2000）⇒ 不得出攻击记录"
        );
        world.clock_ms = 2200;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ 返回 true");
        let m = world.entities.get(mid).unwrap();
        assert_eq!(m.ai.dir, DIR_RIGHT, "先 TurnTo(GetAttackDir)");
        assert_eq!(m.ai.attack_tick, 2200, "到点后刷新 AttackTick");
        let rec = world.pending_ai_attacks.clone();
        assert_eq!(rec.len(), 1, "恰 1 条 AI 攻击记录");
        assert_eq!(
            (rec[0].0, rec[0].1),
            (mid, pid),
            "记录 (攻方=怪, 受击方=玩家)"
        );
        assert!(rec[0].2.hit, "怪对玩家的攻击应命中");
        assert!(rec[0].2.damage > 0, "护甲 0 ⇒ 伤害应 > 0（下游才出帧）");
    }

    // ------------------------------ Lane 1：怪普攻 `RM_HIT` ------------------------------

    /// **Lane 1**：`AnimalObject.Attack`（`AnimalObject.cs:167-171`）的第二句
    /// `SendAttackMsg(RM_HIT, Dir, CurrX, CurrY)` ⇒ world 层每次「邻接 + 到点」的攻击
    /// **恰产出一条** `RM_HIT` 事件，宿主 `drain_monster_ref_msgs` 据此出 `SM_HIT(14)`。
    ///
    /// 收件人按 oracle `SendRefMsg`（`BaseObject.Message.cs:383-445`）在**发帧时刻**快照：
    /// ±12 格、仅 `Race==Play`、排除自己（怪物观察者 `WantRefMsg` 只对
    /// STRUCK/HEAR/DEATH 放行 ⇒ 不收 `RM_HIT`）。
    ///
    /// 牙：删掉 `attack_target` 尾部的 `send_ref_msg` ⇒ 首条「恰 1 条」红；把 `wParam`
    /// 换成 0 或把 x/y 取错 ⇒ 字段断言红；把收件人改成全体实体 ⇒ 集合断言红。
    #[test]
    fn monster_attack_emits_one_rm_hit_with_oracle_recipients() {
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let victim = player(&mut world, 11, 10);
        let observer = player(&mut world, 10, 20); // Δ=10 ⇒ ±12 内
        let outsider = player(&mut world, 23, 10); // Δ=13 ⇒ 界外
        let other_mon = monster(&mut world, 10, 11); // 非 Play ⇒ 不收 RM_HIT
        world.tick(); // 处理 Enter 落格（收件人扫描读 map cell）
        {
            let m = world.entities.get_mut(mid).unwrap();
            m.dc = 10 | (10 << 8);
            m.level = 1;
            m.ai.attack_tick = 0;
        }
        set_target_creature(&mut world, mid, victim);
        world.clock_ms = 2_200;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ 返回 true");

        assert_eq!(
            world.pending_ai_attacks.len(),
            1,
            "恰 1 条攻击结算（SM_STRUCK 数据源）"
        );
        let refs = &world.pending_monster_ref_msgs;
        assert_eq!(refs.len(), 1, "一次攻击恰 1 条 RM_HIT");
        let r = &refs[0];
        assert_eq!(r.ident, RM_HIT, "ident = RM_HIT");
        assert_eq!(
            r.w_param,
            i32::from(DIR_RIGHT),
            "wParam = Dir（`AttackDir` 先置 `Dir = nDir`）"
        );
        assert_eq!(
            (r.n_param1, r.n_param2, r.n_param3),
            (10, 10, 0),
            "坐标 = 发帧时刻 CurrX/CurrY，尾 0"
        );
        assert_eq!((r.x, r.y, r.map_id), (10, 10, 0), "发帧快照 (x,y,map)");
        assert_eq!(r.dir, DIR_RIGHT, "方向快照 = AttackDir 后的 Dir");
        assert!(!r.self_only, "非 ObMode/FixedHideMode ⇒ 正常广播");
        let mut got = r.recipients.clone();
        got.sort_unstable();
        let mut want = vec![victim, observer];
        want.sort_unstable();
        assert_eq!(
            got, want,
            "收件人 = ±12 内的玩家（受击者 + 观察者）；排除自己/怪物观察者/界外"
        );
        assert!(!r.recipients.contains(&mid), "自己不在收件人内");
        assert!(!r.recipients.contains(&other_mon), "怪物观察者不发 RM_HIT");
        assert!(!r.recipients.contains(&outsider), "Δ>12 不发");
    }

    /// **Lane 1**：`RM_HIT` 的发送门严格照 oracle——`Attack` 只在
    /// `GetAttackDir` 命中（八邻格）且攻击间隔门通过时被调用；一旦被调用，
    /// `SendAttackMsg` **无条件**发（零伤害/被闪避也发）。
    ///
    /// 牙：把零伤害支改成「伤害 > 0 才发」⇒ A 段红；把 `RM_HIT` 挪到邻接判定之外
    /// ⇒ B/C 段红。
    #[test]
    fn monster_attack_hit_send_gate_matches_attack_invocation_gate() {
        // A) 零伤害（`_Attack` 的 `nPower == 0` 支）仍必须发 RM_HIT。
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let pid = player(&mut world, 11, 10);
        world.tick();
        world.entities.get_mut(mid).unwrap().dc = 0; // Lo=Hi=0 ⇒ nPower=0
        world.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 2_200;
        assert!(attack_target(&mut world, mid));
        assert_eq!(world.pending_ai_attacks.len(), 1, "结算记录照样产生");
        assert_eq!(world.pending_ai_attacks[0].2.damage, 0, "零伤害");
        assert_eq!(
            world.pending_monster_ref_msgs.len(),
            1,
            "零伤害/未命中仍发 RM_HIT（oracle 无条件）"
        );

        // B) 距离不足（非八邻格）⇒ 不进 `Attack` ⇒ 不发 RM_HIT。
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let pid = player(&mut world, 12, 10);
        world.tick();
        world.entities.get_mut(mid).unwrap().dc = 10 | (10 << 8);
        world.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 2_200;
        assert!(!attack_target(&mut world, mid), "不邻接 ⇒ false");
        assert!(
            world.pending_monster_ref_msgs.is_empty(),
            "距离不足不得发 RM_HIT"
        );
        assert!(world.pending_ai_attacks.is_empty());

        // C) 邻接但攻击间隔门未过 ⇒ 不进 `Attack` ⇒ 不发 RM_HIT（也不得多发）。
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let pid = player(&mut world, 11, 10);
        world.tick();
        world.entities.get_mut(mid).unwrap().dc = 10 | (10 << 8);
        {
            let m = world.entities.get_mut(mid).unwrap();
            m.ai.attack_tick = 1_200; // now 2_200 - 1_200 = 1_000 <= NextHitTime 2_000
            m.ai.next_hit_time = 2_000;
        }
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 2_200;
        assert!(attack_target(&mut world, mid), "邻接 ⇒ true（只是不打）");
        assert!(
            world.pending_monster_ref_msgs.is_empty(),
            "间隔门未过 ⇒ 不得发 RM_HIT"
        );
        assert!(world.pending_ai_attacks.is_empty());

        // D) `ObMode/FixedHideMode`：oracle `SendRefMsg` 只投给自己（怪物无客户端 ⇒ 不产帧）。
        let mut world = World::new(40, 40);
        let mid = monster(&mut world, 10, 10);
        let pid = player(&mut world, 11, 10);
        world.tick();
        {
            let m = world.entities.get_mut(mid).unwrap();
            m.dc = 10 | (10 << 8);
            m.ai.attack_tick = 0;
            m.fixed_hide_mode = true;
        }
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 2_200;
        assert!(attack_target(&mut world, mid));
        let refs = &world.pending_monster_ref_msgs;
        assert_eq!(refs.len(), 1, "事件仍产生（SendRefMsg 语义）");
        assert!(refs[0].self_only, "隐藏模式 ⇒ self_only");
        assert!(refs[0].recipients.is_empty(), "self_only ⇒ 无收件人");
    }

    /// **Lane 1 回归**：玩家攻击路径不受影响——`CM_HIT`/`Command::Attack` 的
    /// `SM_HIT` 由玩家攻击接线层出，**不得**流进怪物 AI 的 `pending_monster_ref_msgs`。
    ///
    /// 牙：把 `send_ref_msg(RM_HIT)` 误放进 `World::attack_once`（两条路径共用）⇒ 本用例红。
    #[test]
    fn player_attack_path_does_not_emit_monster_rm_hit() {
        let mut world = World::new(40, 40);
        let pid = player(&mut world, 10, 10);
        let mid = monster(&mut world, 11, 10);
        world.tick();
        world.entities.get_mut(pid).unwrap().dc = 10 | (10 << 8);
        world.attack_cmd(pid, mid);
        world.clock_ms = 2_200;
        let rec = world.tick();
        assert_eq!(rec.attacks.len(), 1, "玩家攻击照常结算");
        assert!(
            world.pending_monster_ref_msgs.is_empty(),
            "玩家攻击不得产生怪物 RM_HIT 事件"
        );
        assert!(world.pending_ai_attacks.is_empty());
    }

    /// W4-G（端到端）：玩家打怪 ⇒ 怪被 `RM_STRUCK` ⇒ 怪在后续 AI 里**反击玩家**，
    /// 且反击结算落进 `TickRecord::attacks`（= 宿主「怪打玩家」帧的数据源）。固定种子两次一致。
    /// 改坏必红：删掉 `attack_target` 的攻击支（或 `pending_struck` 的出队）⇒ 找不到记录。
    #[test]
    fn player_hit_triggers_monster_retaliation_via_tick_and_records_frame() {
        fn run() -> Option<(i32, i32)> {
            let mut world = World::new(40, 40);
            let mid = monster(&mut world, 10, 10);
            {
                let m = world.entities.get_mut(mid).unwrap();
                m.dc = 10 | (10 << 8);
                m.level = 1;
            }
            let pid = player(&mut world, 11, 10);
            world.rng = RandomNumber::with_seed(2024);
            world.enable_monster_ai();
            // 玩家攻击怪（= CM_HIT 的结算）⇒ 怪被命中 ⇒ 入队 `RM_STRUCK`。
            let o = world.attack_once(pid, mid).expect("玩家攻怪应结算");
            assert!(o.hit, "玩家应命中怪");
            let mut found = None;
            for _ in 0..40 {
                let rec = world.tick();
                for (a, t, _) in &rec.attacks {
                    if *a == mid && *t == pid {
                        found = Some((*a, *t));
                    }
                }
                if found.is_some() {
                    break;
                }
            }
            found
        }
        let a = run();
        assert!(a.is_some(), "怪应在被命中后的某 tick 反击玩家");
        assert_eq!(a, run(), "固定种子两次运行必须一致（确定性）");
    }

    // ==================================================================
    // W9：① 主动索敌（ATMonster.Run）・② Animal 肉质扣减・③ 守卫静止
    //     oracle：ATMonster.cs:13-24、AnimalObject.cs:334-360、GuardUnit.cs/NormNpc.cs:1604-1611
    // ==================================================================

    /// 造一只指定 race 的怪；`walk_speed` 顶高以把 `monster_run` 隔离在索敌段
    /// （`now - walk_tick > walk_speed` 恒假 ⇒ 不进游走/攻击段）。
    fn race_monster(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "m", x, y);
        e.race = race;
        e.hp = 60;
        e.max_hp = 60;
        e.ai.walk_speed = 1_000_000;
        world.add_entity(e)
    }

    /// ①-A：`ATMonster.Run`（`ATMonster.cs:17`）的时钟门四态——race 81/83 的怪只在
    /// `(now-SearchEnemyTick)>8000 || ((now-SearchEnemyTick)>1000 && TargetCret==null)` 时索敌。
    /// 改坏必红：去掉 `search_target` 调用 ⇒ 段 3/4（应认领却空）断言红。
    #[test]
    fn active_seek_race_searches_only_under_clock_gate() {
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 83, 10, 10); // SlowAtMonster：继承 ATMonster.Run
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        // 段1：`now - SearchEnemyTick <= 1000` ⇒ 不搜。
        world.clock_ms = 1000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "<=1000 不索敌"
        );
        // 段2：`>1000` 但已有目标 ⇒ 不搜（保持哨兵）。
        set_target_creature(&mut world, mid, pid);
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">1000 但有目标 ⇒ 不重搜"
        );
        // 段3：`>1000` 且无目标 ⇒ 搜（认领最近可见玩家）。
        world.entities.get_mut(mid).unwrap().ai.target_creature = None;
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">1000 且无目标 ⇒ 索敌"
        );
        // 段4：`>8000` 即使有目标也重搜（超距者被清、取最近合法者）。
        let far = player(&mut world, 40, 10);
        world
            .entities
            .get_mut(mid)
            .unwrap()
            .visible_actors
            .push(VisibleBaseObject {
                target: far,
                flag: VisibleFlag::Show,
            });
        set_target_creature(&mut world, mid, far);
        world.clock_ms = 9000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            ">8000 重搜 ⇒ 取更近者（far 超距被清）"
        );
    }

    /// ①-B：`SearchTarget` **不消费随机数**（RNG 流水不变）；race 80（非索敌族）即使有可见玩家也不索敌。
    /// 改坏必红：给 SearchTarget 加抽数 ⇒ 流水断言红；把门放开到全体 ⇒ race 80 断言红。
    #[test]
    fn active_seek_is_rng_free_and_non_seek_race_never_searches() {
        // race 81：门到点、索敌成功，但 RNG 流水不动。
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 81, 10, 10);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        world.rng = RandomNumber::with_seed(777);
        let mut probe = RandomNumber::with_seed(777);
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            Some(pid),
            "race 81 应索敌"
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "SearchTarget 不得消费随机数"
        );

        // race 80（MonsterObject 本体）：同样有可见玩家，但**不**自动索敌。
        let mut world = World::new(60, 60);
        let mid = race_monster(&mut world, 80, 10, 10);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.clock_ms = 5000;
        world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, mid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.target_creature,
            None,
            "race 80 不索敌"
        );
    }

    /// ①-C：`is_active_seek_race` 的分类（`AtMonster` 本体 + 全部「未覆写 `Run`」直/间派生；
    /// 排除覆写 `Run` 的派生）。审计 G08 的 9 个 race 必须全部在列（含数据不可达的 118/130）。
    #[test]
    fn active_seek_race_classification_matches_oracle() {
        for r in [
            53u8, 81, 82, 83, 84, 86, 88, 89, 90, 91, 97, 106, 118, 119, 130,
        ] {
            assert!(is_active_seek_race(r), "race {r} 应为主动索敌族");
        }
        // race 50/80（非 ATMonster.Run）与覆写 Run 的派生（85/87/92/94/95/96/100/101/
        // 105/107/114/115/132）不在本集合内。
        for r in [
            50u8, 80, 85, 87, 92, 94, 95, 96, 100, 101, 105, 107, 114, 115, 132,
        ] {
            assert!(!is_active_seek_race(r), "race {r} 不应为主动索敌族");
        }
    }

    /// 审计 G08 / MCS-A3：9 个「继承 `ATMonster.Run` 而未覆写 `Run`」的 race 在固定时钟
    /// `>8000ms` 门到点后必须认领 5 格内玩家。逐 race 各跑一个独立世界（互不污染）。
    ///
    /// 改坏必红：任一 race 漏出 [`is_active_seek_race`] ⇒ 该轮 `target_creature == None`。
    #[test]
    fn active_seek_race_inherits_at_monster_run_for_nine_races() {
        for r in [82u8, 84, 90, 91, 97, 106, 118, 119, 130] {
            let mut world = World::new(60, 60);
            let mid = race_monster(&mut world, r, 10, 10);
            let pid = player(&mut world, 15, 10);
            world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
                target: pid,
                flag: VisibleFlag::Show,
            }];
            world.clock_ms = 8001;
            world.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 0;
            monster_run(&mut world, mid);
            assert_eq!(
                world.entities.get(mid).unwrap().ai.target_creature,
                Some(pid),
                "race {r} 必须继承 ATMonster.Run 的 `(now - SearchEnemyTick) > 8000` 索敌门"
            );
        }
    }

    /// ②-A：`AnimalObject.Struck`（`AnimalObject.cs:350-357`）——`Animal` 为真 ⇒ 恰抽 1 次
    /// `Random(300)` 扣 `MeatQuality`。固定种子断言取数与流水位置（在索敌支之后）。
    /// 改坏必红：门恒 false（去掉 `is_animal_race`）⇒ 断言红。
    #[test]
    fn animal_struck_consumes_random300_and_reduces_meat_quality() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 52, 10, 10); // 鹿：AnimalDeer
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 5000;
        let pid = player(&mut world, 11, 10);
        world.clock_ms = 100;
        world.rng = RandomNumber::with_seed(4242);
        let mut probe = RandomNumber::with_seed(4242);
        // 无目标 ⇒ 索敌支短路**不抽** `Random(6)`；随后 `Animal` 支抽 1 次 `Random(300)`。
        let draw = probe.random_below(300);
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.meat_quality,
            5000 - draw as u16,
            "MeatQuality -= Random(300)"
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "恰抽 1 次 Random(300)，位置在索敌支之后"
        );
    }

    /// ②-B：`Animal` 为假（race 80）⇒ **不**抽 `Random(300)`、`MeatQuality` 不变。
    /// 改坏必红：门恒真 ⇒ 断言红。
    #[test]
    fn animal_struck_no_random300_for_non_animal_race() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 80, 10, 10);
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 5000;
        let pid = player(&mut world, 11, 10);
        world.clock_ms = 100;
        world.rng = RandomNumber::with_seed(4242);
        let mut probe = RandomNumber::with_seed(4242);
        let probe_next = probe.random();
        animal_struck(&mut world, mid, pid);
        let e = world.entities.get(mid).unwrap();
        assert_eq!(e.ai.meat_quality, 5000, "非 Animal 不扣肉质");
        assert_eq!(
            world.rng.random(),
            probe_next,
            "恰抽 0 次随机数（无目标支 + 非 Animal）"
        );
    }

    /// **F3**：`HighRiskSpider`/`BigPoisionSpider` ctor 显式 `Animal = false`
    /// （`HighRiskSpider.cs:7`、`BigPoisionSpider.cs:7`）⇒ 118/119 被击**不得**抽
    /// `Random(300)`；对照 82（`SpitSpider` ctor `Animal = true`）仍恰抽一次。
    ///
    /// **可达性**：119（`BigPoisionSpider`）有 31+ 条部署 `MonGen`，旧实现多抽一次 `Random(300)`
    /// 会污染共享 RNG 流水。
    ///
    /// 牙：把 119（或 118）加回 [`is_animal_race`] ⇒ 本用例红。
    #[test]
    fn animal_struck_race118_119_do_not_draw_random300() {
        let run = |race: u8, animal: bool| {
            let mut world = World::new(30, 30);
            let mid = race_monster(&mut world, race, 10, 10);
            world.entities.get_mut(mid).unwrap().ai.meat_quality = 5000;
            let pid = player(&mut world, 11, 10);
            world.clock_ms = 100;
            world.rng = RandomNumber::with_seed(4242);
            let mut probe = RandomNumber::with_seed(4242);
            if animal {
                let draw = probe.random_below(300);
                animal_struck(&mut world, mid, pid);
                assert_eq!(
                    world.entities.get(mid).unwrap().ai.meat_quality,
                    5000 - draw as u16,
                    "race {race} 应扣肉质"
                );
            } else {
                let next = probe.random();
                animal_struck(&mut world, mid, pid);
                assert_eq!(
                    world.entities.get(mid).unwrap().ai.meat_quality,
                    5000,
                    "race {race} 非 Animal 不扣肉质"
                );
                assert_eq!(
                    world.rng.random(),
                    next,
                    "race {race} 非 Animal 不得抽 Random(300)"
                );
            }
        };
        run(118, false);
        run(119, false);
        run(82, true);
    }

    /// ②-C：`MeatQuality` 下限 0（`AnimalObject.cs:353-356`）——`u16` 不得回绕。
    /// 改坏必红：去掉 `< 0 ⇒ 0` 夹取 ⇒ `0u16 - Random(300)` 回绕成 ~65236。
    #[test]
    fn animal_struck_clamps_meat_quality_at_zero() {
        let mut world = World::new(30, 30);
        let mid = race_monster(&mut world, 53, 10, 10); // 狼：AnimalWolf（AtMonster 系）
        world.entities.get_mut(mid).unwrap().ai.meat_quality = 0;
        let pid = player(&mut world, 11, 10);
        world.rng = RandomNumber::with_seed(5);
        animal_struck(&mut world, mid, pid);
        assert_eq!(
            world.entities.get(mid).unwrap().ai.meat_quality,
            0,
            "扣减后夹到 0（不回绕）"
        );
    }

    /// ②-D：`is_animal_race` 分类与 oracle 一致（含 ctor 继承与显式 `Animal=false` 的排除）。
    ///
    /// **F3 牙**：把 119（或 118）加回 `is_animal_race` ⇒ 本用例红。
    #[test]
    fn animal_race_classification_matches_oracle() {
        for r in [51u8, 52, 53, 82, 84, 85, 90, 105, 106, 114, 132] {
            assert!(is_animal_race(r), "race {r} 应为 Animal");
        }
        // 54（ReAliveEx 才置位，复活未建模）；107/118/119（ctor 显式 `Animal = false`）；非动物。
        for r in [54u8, 80, 81, 83, 86, 88, 89, 107, 118, 119, 11, 112] {
            assert!(!is_animal_race(r), "race {r} 不应为 Animal");
        }
    }

    /// 造一只有可见玩家的怪，跑 `ticks` 个 `monster_ai_tick`，返回末位置 `(x, y)`。
    fn run_ai_ticks(race: u8, ticks: u32) -> (i32, i32) {
        let mut world = World::new(60, 60);
        let mut m = Entity::new(0, "m", 20, 20);
        m.race = race;
        m.hp = 6000;
        m.max_hp = 6000;
        let mid = world.add_entity(m);
        let pid = player(&mut world, 22, 20);
        world.entities.get_mut(mid).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.rng = RandomNumber::with_seed(20261016);
        for _ in 0..ticks {
            world.clock_ms += 200;
            monster_ai_tick(&mut world);
        }
        let e = world.entities.get(mid).unwrap();
        (e.x, e.y)
    }

    /// ③-A：race 11（守卫）静止不动；race 80（普通怪）照常游走。
    /// 改坏必红：去掉 `monster_ai_tick` 的守卫跳过 ⇒ 守卫游走 ⇒ 首个断言红。
    #[test]
    fn guard_race_stays_static_while_normal_monster_wanders() {
        assert_eq!(
            run_ai_ticks(11, 800),
            (20, 20),
            "守卫（race 11）必须静止（不游走/不索敌）"
        );
        assert_ne!(
            run_ai_ticks(80, 800),
            (20, 20),
            "普通怪（race 80）应照常游走"
        );
    }

    /// ③-B：守卫虽跳过 `monster_run`，仍要跑 `base.Run()` 段——`RM_STRUCK` 队列照常出队。
    /// 改坏必红：把守卫改成 `continue` 前不出队（或整只跳过 AI）⇒ 队列不为空。
    #[test]
    fn guard_still_drains_struck_queue() {
        let mut world = World::new(30, 30);
        let gid = race_monster(&mut world, 11, 10, 10);
        let hitter = player(&mut world, 11, 10);
        world.entities.get_mut(gid).unwrap().ai.pending_struck = vec![crate::entity::StruckMsg {
            hiter: hitter,
            w_param: 0,
        }];
        world.clock_ms = 200;
        monster_ai_tick(&mut world);
        let e = world.entities.get(gid).unwrap();
        assert!(
            e.ai.pending_struck.is_empty(),
            "守卫的 RM_STRUCK 队列应被出队"
        );
        assert_eq!(
            e.ai.target_creature,
            Some(hitter),
            "守卫被击应认领命中者（GuardUnit.Struck 同 AnimalObject.Struck）"
        );
        assert_eq!((e.x, e.y), (10, 10), "处理受击也不移动");
    }

    /// ③-C：`is_guard_race` 分类（`CreateMonster` 的守卫族；不含普通怪）。
    #[test]
    fn guard_race_classification_matches_oracle() {
        for r in [11u8, 12, 20, 112] {
            assert!(is_guard_race(r), "race {r} 应为守卫族");
        }
        for r in [0u8, 10, 50, 80, 81, 83, 110, 111] {
            assert!(!is_guard_race(r), "race {r} 不应为守卫族");
        }
    }

    // ---- 尸体 / MakeGhost 门（卡 openmir2-run-gap-corpse-ghost-revive） ----------------

    /// 造一只已死怪（race 80 = 普通 `MonsterObject`，非 Animal）。
    fn corpse(
        world: &mut World,
        can_re_alive: bool,
        has_mon_gen: bool,
        zen_time: i32,
        death_tick: i64,
    ) -> i32 {
        let mut e = Entity::new(0, "尸体", 10, 10);
        e.race = 80;
        e.max_hp = 60;
        e.hp = 0;
        e.max_mp = 10;
        e.mp = 0;
        e.death = true;
        e.death_tick = death_tick;
        e.can_re_alive = can_re_alive;
        e.mon_gen = if has_mon_gen { Some(0) } else { None };
        e.mon_gen_zen_time = zen_time;
        world.add_entity(e)
    }

    /// 判据 a：`ZenTime` 足够大 ⇒ 门 = `ZenTime - 20s`（未触截断）：
    /// `DeathTick + gate`（等于）**不** Ghost，`+1` Ghost。两条门都是严格 `>`。
    ///
    /// **牙①**：去掉可复活支的 `- 20 * 1000` ⇒ 门变 120000，`+100001` 不 Ghost ⇒ 本用例红。
    #[test]
    fn make_ghost_gate_uses_zen_time_minus_20s() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(180_000, 5000, 300);
        // gate = max(10000, 120000 - 20000) = 100000（< MakeGhostTime ⇒ 不截断）。
        let id = corpse(&mut world, true, true, 120_000, 1_000);

        world.clock_ms = 1_000 + 99_999;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.invisible && !e.ghost, "gate-1 不得 MakeGhost");

        world.clock_ms = 1_000 + 100_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(
            !e.invisible && !e.ghost,
            "恰等于 gate 不得 MakeGhost（严格 >）"
        );

        world.clock_ms = 1_000 + 100_001;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.invisible, "gate+1 可复活怪 ⇒ Invisible");
        assert!(!e.ghost, "可复活怪保留尸体，不置 Ghost");
        assert_eq!(e.ghost_tick, world.clock_ms, "GhostTick = GetTickCount()");
    }

    /// 判据 b：`ZenTime` 小 ⇒ `MAX(10s)` 下限生效（`25000-20000=5000 ⇒ 10000`）。
    ///
    /// **牙②**：去掉 `MAX(10 * 1000, ...)` ⇒ 门变 5000，`+10001` 时仍 Ghost，但
    /// `+10000` 处就已经 Ghost ⇒ 第一条断言红。
    #[test]
    fn make_ghost_gate_floors_at_10s() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(180_000, 5000, 300);
        let id = corpse(&mut world, true, true, 25_000, 1_000);

        world.clock_ms = 1_000 + 10_000;
        base_object_run_monster(&mut world, id);
        assert!(
            !world.entities.get(id).unwrap().invisible,
            "MAX(10s) 下限：恰等 10000 不 Ghost"
        );
        world.clock_ms = 1_000 + 10_001;
        base_object_run_monster(&mut world, id);
        assert!(
            world.entities.get(id).unwrap().invisible,
            "MAX(10s) 下限：10001 必须 Ghost"
        );
    }

    /// 截断步：`ZenTime - 20s > Config.MakeGhostTime` ⇒ 取 `Config.MakeGhostTime`。
    #[test]
    fn make_ghost_gate_truncates_to_config_make_ghost_time() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(90_000, 5000, 300);
        // 2_000_000 - 20_000 = 1_980_000 > 90_000 ⇒ 截到 90_000。
        let id = corpse(&mut world, true, true, 2_000_000, 1_000);
        world.clock_ms = 1_000 + 90_000;
        base_object_run_monster(&mut world, id);
        assert!(
            !world.entities.get(id).unwrap().invisible,
            "恰等截断值不 Ghost"
        );
        world.clock_ms = 1_000 + 90_001;
        base_object_run_monster(&mut world, id);
        assert!(
            world.entities.get(id).unwrap().invisible,
            "截断值 +1 必须 Ghost"
        );
    }

    /// 判据 c：`CanReAlive=false` ⇒ 走普适支 `Config.MakeGhostTime`（90s）。
    ///
    /// **牙③**：把普适支的 `>` 改成 `>=` ⇒ 恰等 90000 时 Ghost ⇒ 第二条断言红。
    #[test]
    fn make_ghost_gate_non_revivable_is_config_value() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(90_000, 5000, 300);
        // 即使 `MonGen != null`，`CanReAlive == false` 也必须走普适支。
        let id = corpse(&mut world, false, true, 120_000, 1_000);

        world.clock_ms = 1_000 + 89_999;
        base_object_run_monster(&mut world, id);
        assert!(!world.entities.get(id).unwrap().invisible);

        world.clock_ms = 1_000 + 90_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(
            !e.invisible && !e.ghost,
            "普适支严格 >：恰等 90000 不 Ghost"
        );

        world.clock_ms = 1_000 + 90_001;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.ghost, "普适支：90001 ⇒ Ghost=true");
        assert!(!e.invisible, "普适支不得保留 Invisible 尸体");
    }

    /// 判据 f（阴性对照）：活怪不受尸体门影响；且死亡怪在门未到前也不改变状态。
    #[test]
    fn live_monster_is_not_affected_by_corpse_gates() {
        let mut world = World::new(64, 64);
        world.set_corpse_config(10_000, 5000, 300);
        let id = monster(&mut world, 10, 10);
        world.clock_ms = 10_000_000;
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.death && !e.ghost && !e.invisible);
        assert_eq!(e.ghost_tick, 0, "活怪不得写 GhostTick");

        // 刚死（门未到）的怪同样不动。
        let now = world.clock_ms;
        let dead = corpse(&mut world, false, false, 0, now);
        base_object_run_monster(&mut world, dead);
        let e = world.entities.get(dead).unwrap();
        assert!(!e.ghost && !e.invisible, "门未到不得 MakeGhost");
    }

    /// 死亡时刻由 `settle_deaths` 写 `death_tick`（`BaseObject.cs:323`）。
    #[test]
    fn death_settlement_records_death_tick() {
        let mut world = World::new(32, 32);
        let id = monster(&mut world, 10, 10);
        world.clock_ms = 12_345;
        world.entities.get_mut(id).unwrap().hp = 0;
        world.tick();
        let e = world.entities.get(id).unwrap();
        assert!(e.death);
        // `tick()` 先推进时钟（+200ms）再结算 ⇒ `DeathTick` = 结算时刻。
        assert_eq!(e.death_tick, world.clock_ms);
        assert_eq!(e.death_tick, 12_545);
    }
    // ==================================================================================
    // 卡 `openmir2-run-gap-monster-master-follow`（oracle `MonsterObject.cs:299-350`）
    // ==================================================================================

    /// 进入 `monster_run` 的公共前置：把「走路到点 / Think 到点」的计时器压到会通过的值，
    /// 保证测试观测到的是跟随/逃跑段而不是外层门。
    fn ready(world: &mut World, id: i32, now: i64) {
        let e = world.entities.get_mut(id).unwrap();
        e.ai.walk_tick = 0;
        e.ai.walk_speed = 1400;
        e.ai.think_tick = now;
        e.ai.walk_wait_locked = false;
        e.ai.walk_wait_tick = now;
    }

    /// 判据 1：`|Δ| > 20` ⇒ `SpaceMove` 到主人身后格；落点可走 ⇒ `SpaceMoveGetRandXY`
    /// **不抽数**，且后续 `GotoTargetXY` 因已在目标格而不抽。
    #[test]
    fn master_follow_space_move_when_distance_gt_20() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 30, 30);
        world.entities.get_mut(master).unwrap().ai.dir = DIR_UP; // 身后 = (30,31)
        assert!(world.enter_world(master));
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        world.entities.get_mut(id).unwrap().master = Some(master);
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 30, 31), ">20 ⇒ 飞到主人身后格");
        assert_eq!((e.ai.target_x, e.ai.target_y), (30, 31));
        assert_eq!(world.rng.random(), probe.random(), "可走落点不抽数");
    }

    /// **复验 F1 回归**：`RunAwayMode` 为真时**不得**跑 Master 跟随段（含 `SpaceMove`）。
    ///
    /// oracle `MonsterObject.cs:277` 的 `if (!RunAwayMode)` 包住整段跟随（`:299-324`）；
    /// 逃跑到点前宝宝必须留在原地（`MagicManager.cs:827-840` 可把无主亡灵宝宝置
    /// `RunAwayMode=true / RunAwayTime=10s`，玩家对其放圣言术即可命中）。
    ///
    /// **牙⑦**：把跟随段移回 `if !run_away_mode` 之外 ⇒ 本用例红（宝宝会 `SpaceMove` 到主人身后）。
    #[test]
    fn run_away_mode_skips_master_follow() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 30, 30);
        world.entities.get_mut(master).unwrap().ai.dir = DIR_UP;
        assert!(world.enter_world(master));
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        {
            let e = world.entities.get_mut(id).unwrap();
            e.master = Some(master);
            e.ai.run_away_mode = true;
            e.ai.run_away_start = 5_000;
            e.ai.run_away_time = 10_000;
        }
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert!(
            e.ai.run_away_mode,
            "未到点仍是逃跑态（严格 > 3000/10000 与本用例无关）"
        );
        assert_eq!(
            (e.map_id, e.x, e.y),
            (0, 5, 5),
            "RunAwayMode ⇒ 不跟随、不 SpaceMove"
        );
    }

    /// 判据 2：`|Δ| <= 20` 且同图 ⇒ **不** `SpaceMove`；`GotoTargetXY` 只走一格并恰抽 1 次
    /// `Random(3)`（oracle `AnimalObject.cs:222`）。
    #[test]
    fn master_follow_no_space_move_when_within_20() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 15, 15);
        world.entities.get_mut(master).unwrap().ai.dir = DIR_UP; // 身后 = (15,16)
        assert!(world.enter_world(master));
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        world.entities.get_mut(id).unwrap().master = Some(master);
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);
        let _ = probe.random_below(3);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!(
            (e.map_id, e.x, e.y),
            (0, 6, 6),
            "≤20 ⇒ 只按 GotoTargetXY 走一格"
        );
        assert_eq!((e.ai.target_x, e.ai.target_y), (15, 16));
        assert_eq!(world.rng.random(), probe.random(), "恰一次 Random(3)");
    }

    /// 判据 3：`Master.SlaveRelax` ⇒ 不 `SpaceMove`，且在 `GotoTargetXY` 之前
    /// `base.Run(); return;`（连 `CheckRoyalty`/`CrazyMode` 到期都跳过）。
    #[test]
    fn slave_relax_skips_space_move_and_early_return() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 30, 30);
        world.entities.get_mut(master).unwrap().ai.dir = DIR_UP;
        world.entities.get_mut(master).unwrap().slave_relax = true;
        assert!(world.enter_world(master));
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        {
            let e = world.entities.get_mut(id).unwrap();
            e.master = Some(master);
            // 若未早退，末尾的 CrazyMode 到期会清掉它。
            e.ai.crazy_mode = true;
            e.ai.crazy_mode_tick = 0;
            e.ai.crazy_mode_interval = 10;
        }
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 5), "SlaveRelax ⇒ 原地不动");
        assert_eq!(
            (e.ai.target_x, e.ai.target_y),
            (30, 31),
            "TargetX/Y 仍照 oracle 更新"
        );
        assert!(e.ai.crazy_mode, "早退必须跳过 CrazyMode 到期");
        assert_eq!(world.rng.random(), probe.random(), "早退路径不抽数");
    }

    /// 判据 4：`Envir != Master.Envir` ⇒ 即使距离 ≤20 也 `SpaceMove`；并复刻
    /// `OnEnvirnomentChanged` 的实体侧效果（`CanReAlive=false; MonGen=null`）。
    #[test]
    fn master_on_other_map_triggers_space_move() {
        let mut world = World::with_maps(vec![(60, 60), (60, 60)]);
        let master = player(&mut world, 10, 10);
        {
            let m = world.entities.get_mut(master).unwrap();
            m.map_id = 1;
            m.ai.dir = DIR_UP; // 身后 = (10,11)
        }
        assert!(world.enter_world(master));
        let id = monster(&mut world, 10, 10);
        assert!(world.enter_world(id));
        {
            let e = world.entities.get_mut(id).unwrap();
            e.master = Some(master);
            e.can_re_alive = true;
            e.mon_gen = Some(0);
        }
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (1, 10, 11), "异图 ⇒ 飞到主人身后格");
        assert!(
            !e.can_re_alive && e.mon_gen.is_none(),
            "OnEnvirnomentChanged 实体侧"
        );
        assert_eq!(world.rng.random(), probe.random(), "可走落点不抽数");
    }

    /// **`openmir2-player-dir-supply`**：宝宝的 `GetBackPosition` 读**主人实体**的
    /// `Entity.ai.dir`（玩家转身写入的唯一权威），展示表不参与。
    #[test]
    fn player_dir_supply_master_back_position_reads_entity_dir() {
        let mut world = World::new(10, 10);
        let master = player(&mut world, 5, 5);
        let slave = world.add_entity({
            let mut e = Entity::new(0, "pet".to_string(), 5, 4);
            e.master = Some(master);
            e
        });
        assert_eq!(world.entities.get(slave).unwrap().master, Some(master));
        for (dir, want) in [(DIR_RIGHT, (4, 5)), (DIR_LEFT, (6, 5)), (DIR_UP, (5, 6))] {
            world.entities.get_mut(master).unwrap().ai.dir = dir;
            assert_eq!(
                master_get_back_position(&world, master),
                Some(want),
                "玩家实体 Dir={dir} ⇒ 宝宝 GetBackPosition"
            );
        }
    }

    /// 判据 5：`GetBackPosition` 八方向 + 边界夹取（`BaseObject.cs:1878-1927`）。
    #[test]
    fn get_back_position_eight_dirs_and_boundary_clamp() {
        let mut world = World::new(10, 10);
        let id = player(&mut world, 5, 5);
        let cases = [
            (DIR_UP, (5, 6)),
            (DIR_UP_RIGHT, (4, 6)),
            (DIR_RIGHT, (4, 5)),
            (DIR_DOWN_RIGHT, (4, 4)),
            (DIR_DOWN, (5, 4)),
            (DIR_DOWN_LEFT, (6, 4)),
            (DIR_LEFT, (6, 5)),
            (DIR_UP_LEFT, (6, 6)),
        ];
        for (dir, want) in cases {
            world.entities.get_mut(id).unwrap().ai.dir = dir;
            assert_eq!(
                master_get_back_position(&world, id),
                Some(want),
                "dir={dir} 身后格"
            );
        }
        // 左上角夹取：两轴各自的门与对角 `&&` 门。
        {
            let e = world.entities.get_mut(id).unwrap();
            e.x = 0;
            e.y = 0;
        }
        let corner = [
            (DIR_UP, (0, 1)),
            (DIR_DOWN, (0, 0)),
            (DIR_LEFT, (1, 0)),
            (DIR_RIGHT, (0, 0)),
            (DIR_UP_LEFT, (1, 1)),
            (DIR_UP_RIGHT, (0, 0)),  // nY > 0 不成立
            (DIR_DOWN_LEFT, (0, 0)), // nX > 0 不成立
            (DIR_DOWN_RIGHT, (0, 0)),
        ];
        for (dir, want) in corner {
            world.entities.get_mut(id).unwrap().ai.dir = dir;
            assert_eq!(
                master_get_back_position(&world, id),
                Some(want),
                "(0,0) dir={dir} 夹取"
            );
        }
        // 右下角夹取。
        {
            let e = world.entities.get_mut(id).unwrap();
            e.x = 9;
            e.y = 9;
        }
        let corner2 = [
            (DIR_UP, (9, 9)),
            (DIR_DOWN, (9, 8)),
            (DIR_LEFT, (9, 9)),
            (DIR_RIGHT, (8, 9)),
            (DIR_UP_LEFT, (9, 9)),
            (DIR_UP_RIGHT, (9, 9)),
            (DIR_DOWN_LEFT, (9, 9)),
            (DIR_DOWN_RIGHT, (8, 8)),
        ];
        for (dir, want) in corner2 {
            world.entities.get_mut(id).unwrap().ai.dir = dir;
            assert_eq!(
                master_get_back_position(&world, id),
                Some(want),
                "(9,9) dir={dir} 夹取"
            );
        }
    }

    /// 判据 6：身后格已有 `GetMovingObject(...,true)` 的活体 ⇒ 目标回退 `CurrX/CurrY`。
    #[test]
    fn occupied_back_cell_falls_back_to_current_position() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 10, 10);
        world.entities.get_mut(master).unwrap().ai.dir = DIR_UP; // 身后 = (10,11)
        assert!(world.enter_world(master));
        let blocker = player(&mut world, 10, 11);
        assert!(world.enter_world(blocker));
        let id = monster(&mut world, 10, 9);
        assert!(world.enter_world(id));
        world.entities.get_mut(id).unwrap().master = Some(master);
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!(
            (e.ai.target_x, e.ai.target_y),
            (10, 9),
            "身后格被占 ⇒ 回退到当前格"
        );
        assert_eq!((e.x, e.y), (10, 9), "同图 ≤20 ⇒ 不传送");
        assert_eq!(world.rng.random(), probe.random(), "回退路径不抽数");
    }

    /// 判据 7：`RunAwayTime > 0 && (now - RunAwayStart) > RunAwayTime` —— **严格大于**。
    /// 相等不清、+1ms 才清（`MonsterObject.cs:329-345`）。
    #[test]
    fn run_away_expiry_is_strict_greater() {
        let mut world = World::new(60, 60);
        let id = monster(&mut world, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.run_away_mode = true;
            e.ai.run_away_start = 5_000;
            e.ai.run_away_time = 1_000;
        }
        // 恰好到期：`1000 > 1000` 为假 ⇒ 保持。
        world.clock_ms = 6_000;
        ready(&mut world, id, 6_000);
        monster_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.ai.run_away_mode, "相等不得清除（严格 >）");
        assert_eq!(e.ai.run_away_time, 1_000);
        // 超 1ms：清除。
        world.clock_ms = 6_001;
        ready(&mut world, id, 6_001);
        monster_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.ai.run_away_mode, "超期必须清除");
        assert_eq!(e.ai.run_away_time, 0);
    }

    /// 判据 8（阴性对照）：`Master == null` 的怪行为不变 —— 仍走 `TargetX = -1` + `Wondering`，
    /// 且只抽一次 `Random(20)`（非 0 支），不走任何跟随/SpaceMove 路径。
    #[test]
    fn no_master_monster_behavior_unchanged() {
        let seed = seed_with_gate(false, None);
        let mut world = World::new(60, 60);
        let id = monster(&mut world, 10, 10);
        assert!(world.enter_world(id));
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_ne!(probe.random_below(20), 0);
        let probe_next = probe.random();

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert!(e.master.is_none());
        assert_eq!((e.x, e.y), (10, 10), "Wondering 非 0 支不移动");
        assert_eq!(e.ai.target_x, -1, "无 master ⇒ 目标仍为 -1（无跟随段）");
        assert_eq!(world.rng.random(), probe_next, "只抽一次 Random(20)");
    }

    /// 判据 9：`SpaceMove` 的取数点/次数与 oracle 同点 —— 起点可走 0 抽；起点在第
    /// `Width-n1C-1` 边界之外才 `Random(Width)`（`BaseObject.cs:1067-1094`）。
    #[test]
    fn space_move_draw_point_matches_oracle_shift_loop() {
        // 证据钩子（仅显式设 `M2_MASTER_FOLLOW_TRACE_PATH` 时开启；未设 ⇒ 零行为差）。
        // 用于机判「本片 SpaceMove 的抽数站点仍归 `space_move` 组」。
        if let Ok(path) = std::env::var("M2_MASTER_FOLLOW_TRACE_PATH") {
            let _ = std::fs::remove_file(&path);
            mir2_shared::rng_trace::enable(std::path::Path::new(&path), 7)
                .expect("打开 master-follow trace");
        }
        let mut world = World::new(60, 60);
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        // n18 = 3（Width < 80）、n1C = 15（50 <= Height < 150）⇒ 阈值 60-15-1 = 44。
        assert!(world.maps[0].set_map_xy_flag(30, 31, false));
        world.rng = RandomNumber::with_seed(2026);
        let mut probe = RandomNumber::with_seed(2026);
        assert!(crate::space_move::monster_space_move(
            &mut world, id, 0, 30, 31
        ));
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.x, e.y), (33, 31), "不可走 ⇒ x += n18 后命中可走格");
        assert_eq!(world.rng.random(), probe.random(), "移位环此支不抽数");

        // 起点 x = 44（>= 阈值）且为墙 ⇒ 首次 `Random(Width)`，随后 y += n18。
        assert!(world.maps[0].set_map_xy_flag(44, 31, false));
        world.rng = RandomNumber::with_seed(7);
        let mut probe = RandomNumber::with_seed(7);
        let expected_x = probe.random_below(60);
        assert!(crate::space_move::monster_space_move(
            &mut world, id, 0, 44, 31
        ));
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.x, e.y), (expected_x, 34));
        assert_eq!(world.rng.random(), probe.random(), "恰一次 Random(60)");
    }

    /// 判据 10：201 轮仍不可走 ⇒ 回滚旧图/旧坐标并重新登记旧格（`BaseObject.cs:1147-1153`）。
    #[test]
    fn space_move_rolls_back_when_no_walkable_cell() {
        // 目标图全墙 ⇒ SpaceMoveGetRandXY 201 轮失败；旧图旧格可走 ⇒ 回滚重登记成功。
        let mut world = World::with_maps(vec![(10, 10), (10, 10)]);
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        for x in 0..10 {
            for y in 0..10 {
                assert!(world.maps[1].set_map_xy_flag(x, y, false));
            }
        }
        assert!(!crate::space_move::monster_space_move(
            &mut world, id, 1, 5, 5
        ));
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 5), "失败必须回滚到旧图旧格");
        assert!(e.add_to_mapped, "回滚后重新登记旧格");
        assert!(
            world.moving_object_at(0, 5, 5, true).is_some(),
            "旧格簿记恢复"
        );
    }
    /// 判据 11：oracle 的 `SpaceMove` 门在 `if (TargetCret == null)` **之外** ——
    /// 宝宝有攻击目标、但离主人 >20 时仍会飞到主人身边（落点用 `TargetX/TargetY`，
    /// 此例由 `AttackTarget()` 设为目标的格子）。
    #[test]
    fn master_follow_space_move_also_runs_when_target_present() {
        let mut world = World::new(60, 60);
        let master = player(&mut world, 30, 30);
        assert!(world.enter_world(master));
        let victim = player(&mut world, 8, 5);
        assert!(world.enter_world(victim));
        let id = monster(&mut world, 5, 5);
        assert!(world.enter_world(id));
        {
            let e = world.entities.get_mut(id).unwrap();
            e.master = Some(master);
            e.ai.target_creature = Some(victim);
        }
        world.clock_ms = 5_000;
        ready(&mut world, id, 5_000);

        monster_run(&mut world, id);

        let e = world.entities.get(id).unwrap();
        assert_eq!(
            (e.map_id, e.x, e.y),
            (0, 8, 5),
            "有目标时 SpaceMove 门仍生效（TargetX/Y = 目标格）"
        );
    }
    // ------------------------------------------------------------------
    // **MB-1**（R38/R12/R35/R04）——四个专用怪 `Run` 覆写的构造级用例。
    // ------------------------------------------------------------------

    /// 造一只本批专用怪（race 决定类；不抽数）。
    fn mb1_monster(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "mb1", x, y);
        e.race = race;
        e.hp = 200;
        e.max_hp = 200;
        e.ai.walk_speed = 0;
        e.ai.walk_tick = 0;
        e.ai.attack_tick = 0;
        e.ai.next_hit_time = 2000;
        crate::mongen::apply_monster_class_constants(&mut e, i16::from(race), 0);
        world.add_entity(e)
    }

    fn mb1_see(world: &mut World, viewer: i32, target: i32) {
        world.entities.get_mut(viewer).unwrap().visible_actors = vec![VisibleBaseObject {
            target,
            flag: VisibleFlag::Show,
        }];
    }

    fn mb1_players(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "P", x, y);
        e.hp = 500;
        e.max_hp = 500;
        world.add_entity(e)
    }

    /// 真正**落图**的玩家（`race == Play` + `enter_world`）：`send_ref_msg_targets_at` 是格扫，
    /// 只认格表里的 `actor_object` 玩家（oracle `SendRefMsg` 同口径）。
    fn mb1_real_player(world: &mut World, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "R", x, y);
        e.race = crate::entity::ACTOR_RACE_PLAY;
        e.hp = 500;
        e.max_hp = 500;
        let id = world.add_entity(e);
        assert!(world.enter_world(id), "玩家落图");
        id
    }

    /// R38 边界①：`CheckComeOut` 两轴**严格 `< ComeOutValue(4)`**，且 `HideMode` 需 `CoolEye`
    /// 破门；命中后 `FindAttackTarget` 置 `FixedHideMode=false` 并出 `RM_DIGUP`。
    ///
    /// 牙①：把 `<` 改成 `<=` ⇒ Δ=4 的用例红；去掉 `CoolEye` 门 ⇒ 隐藏目标用例红。
    #[test]
    fn mb1_stick_come_out_is_strict_and_honours_cool_eye() {
        let run = |hide_mode: bool, cool_eye: bool, dx: i32| {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 85, 10, 10);
            world.entities.get_mut(id).unwrap().ai.cool_eye = cool_eye;
            let p = mb1_players(&mut world, 10 + dx, 10);
            world.entities.get_mut(p).unwrap().hide_mode = hide_mode;
            mb1_see(&mut world, id, p);
            world.clock_ms = 100_000;
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Animal)
            );
            let e = world.entities.get(id).unwrap();
            (
                e.fixed_hide_mode,
                world.pending_monster_ref_msgs.clone(),
                e.ai.walk_tick,
            )
        };
        // Δx=3 < 4 ⇒ 出土。
        let (hidden, refs, walk_tick) = run(false, false, 3);
        assert!(!hidden, "命中 CheckComeOut ⇒ FixedHideMode=false");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_DIGUP);
        assert_eq!((refs[0].x, refs[0].y), (10, 10));
        assert_eq!(walk_tick, 100_000, "WalkTick 刷新");
        // Δx=4 **不** <4 ⇒ 不出土（严格 `<`）。
        let (hidden, refs, _) = run(false, false, 4);
        assert!(hidden && refs.is_empty(), "Δ=4 必须被严格 < 挡住");
        // 隐藏目标 + CoolEye=false ⇒ 不出土。
        let (hidden, refs, _) = run(true, false, 1);
        assert!(hidden && refs.is_empty(), "HideMode 需 CoolEye");
        // 隐藏目标 + CoolEye=true ⇒ 出土。
        let (hidden, refs, _) = run(true, true, 1);
        assert!(!hidden && refs.len() == 1, "CoolEye 破 HideMode");
    }

    /// R38 边界②：非隐藏支——目标为空/超 `AttackRange` ⇒ `ComeDown`（`RM_DIGDOWN` + 清可见集
    /// + `FixedHideMode=true`）；相邻目标 ⇒ `AttackTarget` 命中一次（`RM_HIT`）。
    ///
    /// 牙②：把 `bo05` 的任一轴判据删掉 ⇒ 超距用例红。
    #[test]
    fn mb1_stick_come_down_and_attack_target() {
        // 超距（Δx=5 > AttackRange=4）⇒ ComeDown。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 85, 10, 10);
        let p = mb1_players(&mut world, 15, 10);
        // 另放一个**真正落图**的玩家在 ±12 内，用于钉住「发帧时刻快照」。
        let rp = mb1_real_player(&mut world, 12, 10);
        mb1_see(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.target_creature = Some(p);
        }
        world.clock_ms = 100_000;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Animal)
        );
        let e = world.entities.get(id).unwrap();
        assert!(e.fixed_hide_mode, "ComeDown ⇒ FixedHideMode=true");
        assert!(e.visible_actors.is_empty(), "ComeDown 清 VisibleActors");
        let refs = &world.pending_monster_ref_msgs;
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_DIGDOWN);
        // **复验 MB-1 F1 回归**：收件人在**发帧时刻**快照（此时 sender 尚未隐藏）。
        // 若改成排水时现算，`ComeDown` 之后 `fixed_hide_mode=true` ⇒ 会丢帧。
        assert!(!refs[0].self_only, "发帧时未隐藏 ⇒ 不是 self_only");
        assert_eq!(
            refs[0].recipients,
            vec![rp],
            "±12 内落图的玩家必须被快照为收件人（否则客户端看不到缩回地下动画）"
        );

        // 相邻（Δx=1 ≤4）⇒ AttackTarget：受击 + RM_HIT + AttackTick 刷新。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 85, 10, 10);
        let p = mb1_players(&mut world, 11, 10);
        mb1_see(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.target_creature = Some(p);
            // 攻击力确定化：DC = MakeWord(10, 10)（`GetBaseAttackPoewr` = 10）。
            e.dc = (10 << 8) | 10;
            e.hit_point = 10;
            e.speed_point = 0;
        }
        world.entities.get_mut(p).unwrap().speed_point = 0;
        world.clock_ms = 100_000;
        let before = world.entities.get(p).unwrap().hp;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Animal)
        );
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.attack_tick, 100_000, "AttackTick 刷新");
        assert_eq!(e.ai.dir, DIR_RIGHT, "AttackDir 置朝向");
        assert!(
            world.entities.get(p).unwrap().hp < before,
            "相邻目标必须被命中"
        );
        assert_eq!(world.pending_monster_ref_msgs.len(), 1);
        assert_eq!(world.pending_monster_ref_msgs[0].ident, RM_HIT);
        assert_eq!(world.pending_ai_attacks.len(), 1, "仅一次攻击结算");
    }

    /// R12 边界：Δ≤3 且可见 ⇒ `Turned` 一次、`WalkTick == now + 1000`（精确）、
    /// `ET_DIGOUTZOMBI` 事件注册、`RM_DIGUP` 出帧；Δ=4 / 隐藏未破 ⇒ 不触发。
    ///
    /// 牙③：把 `<= 3` 改成 `< 3` ⇒ Δ=3 用例红；把 `now + 1000` 改成 `now` ⇒ 精确断言红。
    #[test]
    fn mb1_dig_out_zombi_turned_boundaries() {
        let run = |hide_mode: bool, cool_eye: bool, dx: i32| {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 95, 10, 10);
            world.entities.get_mut(id).unwrap().ai.cool_eye = cool_eye;
            let p = mb1_players(&mut world, 10 + dx, 10);
            world.entities.get_mut(p).unwrap().hide_mode = hide_mode;
            mb1_see(&mut world, id, p);
            world.clock_ms = 100_000;
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            let e = world.entities.get(id).unwrap();
            (
                e.fixed_hide_mode,
                e.ai.walk_tick,
                world.dig_out_event_at(0, 10, 10),
                world.pending_monster_ref_msgs.len(),
            )
        };
        let (hidden, walk_tick, event, refs) = run(false, false, 3);
        assert!(!hidden, "Turned ⇒ FixedHideMode=false");
        assert_eq!(walk_tick, 101_000, "WalkTick = now + 1000");
        assert!(event, "ET_DIGOUTZOMBI 事件注册");
        assert_eq!(refs, 1);
        let (hidden, _, event, refs) = run(false, false, 4);
        assert!(hidden && !event && refs == 0, "Δ=4 不触发");
        let (hidden, _, _, refs) = run(true, false, 1);
        assert!(hidden && refs == 0, "HideMode + CoolEye=false ⇒ 不触发");
        let (hidden, _, _, refs) = run(true, true, 1);
        assert!(!hidden && refs == 1, "CoolEye 破 HideMode");
    }

    /// R35 边界：`StoneMode` + 目标两轴 ≤2 ⇒ `MeltStoneAll`：
    /// 自身 `StoneMode=false`/`CharStatusEx=0`/`CharStatus` 重算/`RM_DIGUP`；
    /// `GetMapBaseObjects(...,7)` 内**每个** `StoneMode && ScultureMonster` 都融化，
    /// 7 格外与非 Sculture 不融化；Δ=3 不触发。
    ///
    /// 牙④：把扫描半径 7 改成 6 ⇒ Δ7 的邻接石像断言红；把 `<= 2` 改成 `< 2` ⇒ Δ=2 红。
    #[test]
    fn mb1_sculture_melts_self_and_neighbours_within_seven() {
        let mut world = World::new(40, 40);
        let id = mb1_monster(&mut world, 101, 10, 10);
        let near = mb1_monster(&mut world, 101, 17, 17); // Δ7（边界内）
        let far = mb1_monster(&mut world, 101, 18, 10); // Δ8（边界外）
        let other_stone = mb1_monster(&mut world, 80, 12, 12); // 非 Sculture
        let p = mb1_players(&mut world, 12, 10); // Δ2（边界）
        for e in [id, near, far, other_stone, p] {
            assert!(world.enter_world(e), "落格");
        }
        world.entities.get_mut(other_stone).unwrap().stone_mode = true;
        world.entities.get_mut(id).unwrap().char_status = 0x1234;
        mb1_see(&mut world, id, p);
        world.clock_ms = 100_000;

        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        let self_e = world.entities.get(id).unwrap();
        assert!(!self_e.stone_mode, "自身 StoneMode=false");
        assert_eq!(self_e.char_status_ex, 0, "CharStatusEx=0");
        assert_eq!(
            self_e.char_status,
            world.get_char_status(id),
            "CharStatus = GetCharStatus()"
        );
        assert!(!world.entities.get(near).unwrap().stone_mode, "Δ7 邻接融化");
        assert!(
            world.entities.get(far).unwrap().stone_mode,
            "Δ8 不融化（7 格边界）"
        );
        assert!(
            world.entities.get(other_stone).unwrap().stone_mode,
            "非 ScultureMonster 不融化"
        );
        let digups: Vec<i32> = world
            .pending_monster_ref_msgs
            .iter()
            .filter(|m| m.ident == RM_DIGUP)
            .map(|m| m.actor_id)
            .collect();
        assert!(digups.contains(&id) && digups.contains(&near));
        assert!(!digups.contains(&far) && !digups.contains(&other_stone));

        // Δ=3 ⇒ 不触发，走索敌（>1000ms 且无目标 ⇒ SearchTarget）。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 101, 10, 10);
        let p = mb1_players(&mut world, 13, 10);
        mb1_see(&mut world, id, p);
        world.clock_ms = 100_000;
        world.entities.get_mut(id).unwrap().ai.search_enemy_tick = 90_000;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        let e = world.entities.get(id).unwrap();
        assert!(e.stone_mode, "Δ=3 不得融化");
        // oracle `Run` 的 `else` 属于 `if (StoneMode)` ⇒ 石化态下**不**走索敌。
        assert_eq!(e.ai.target_creature, None, "StoneMode 支不索敌");
        // 融化后（`StoneMode=false`）同一时钟门 ⇒ `SearchTarget` 选目标。
        world.entities.get_mut(id).unwrap().stone_mode = false;
        world.entities.get_mut(id).unwrap().ai.search_enemy_tick = 90_000;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert_eq!(
            world.entities.get(id).unwrap().ai.target_creature,
            Some(p),
            "StoneMode=false ⇒ 8s/1s 索敌"
        );
    }

    /// R35 门是 `>=`（`ScultureMonster.cs:45`），恰等 `WalkSpeed` 就进函数体。
    /// 牙⑤：把 `>=` 改成 `>` ⇒ 本用例红。
    #[test]
    fn mb1_sculture_run_gate_is_inclusive() {
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 101, 10, 10);
        let p = mb1_players(&mut world, 10, 10 - 1);
        mb1_see(&mut world, id, p);
        world.clock_ms = 100_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.walk_speed = 500;
            e.ai.walk_tick = 99_500; // now - walk_tick == 500 == WalkSpeed
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert!(!world.entities.get(id).unwrap().stone_mode, ">= 门放行");
    }

    /// R04：`NextHitTime` 门；`Random(|Hi(DC)-Lo(DC)|+1)` **同点同次**；
    /// `RM_DELAYMAGIC` 200ms + `RM_10205`；`ViewRange` 边界；到点前不投递。
    ///
    /// 牙⑥：把 `Random(|Hi-Lo|+1)` 改成 `Random(Hi-Lo)` ⇒ 取数参数/载荷断言红；
    /// 把 200ms 改成 100ms ⇒ 到点断言红。
    #[test]
    fn mb1_big_heart_attack_target_draws_once_and_delivers_after_200ms() {
        // 门未到：零抽、零帧。
        let mut world = World::new(40, 40);
        let id = mb1_monster(&mut world, 115, 10, 10);
        let p = mb1_players(&mut world, 12, 10);
        mb1_see(&mut world, id, p);
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(777);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.attack_tick = 99_000; // now - attack_tick == 1000 <= NextHitTime(2000)
        }
        let mut probe = RandomNumber::with_seed(777);
        assert!(!big_heart_attack_target(&mut world, id));
        assert!(world.self_delay_msgs.is_empty(), "门未到 ⇒ 不入队");
        assert!(world.pending_monster_ref_msgs.is_empty(), "门未到 ⇒ 无帧");
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "门未到 ⇒ 零取数（随机流不前进）"
        );

        // 门到：恰一次 `Random(param)`，参数 = |Hi-Lo|+1。
        let (lo, hi) = (5i32, 9i32);
        let param = (hi - lo).abs() + 1;
        let seed = (0..100_000)
            .find(|candidate| {
                let v = RandomNumber::with_seed(*candidate).random_below(param);
                let v_lo = RandomNumber::with_seed(*candidate).random_below(param - 1);
                let v_hi = RandomNumber::with_seed(*candidate).random_below(param + 1);
                v != v_lo && v != v_hi
            })
            .expect("找不到能分辨参数漂移的种子");
        let mut world = World::new(40, 40);
        let id = mb1_monster(&mut world, 115, 10, 10);
        world.entities.get_mut(id).unwrap().dc = (hi << 8) | lo;
        let p = mb1_players(&mut world, 12, 10); // Δ2 ≤ ViewRange(16)
        let outside = mb1_players(&mut world, 10 + 17, 10); // Δ17 > 16
        for e in [id, p, outside] {
            assert!(world.enter_world(e));
        }
        {
            let e = world.entities.get_mut(id).unwrap();
            e.visible_actors = vec![
                VisibleBaseObject {
                    target: p,
                    flag: VisibleFlag::Show,
                },
                VisibleBaseObject {
                    target: outside,
                    flag: VisibleFlag::Show,
                },
            ];
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let draw = expect.random_below(param);
        assert!(big_heart_attack_target(&mut world, id));
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "恰一次取数（同点同次）"
        );
        let msg = world.self_delay_msgs.peek(id).expect("RM_DELAYMAGIC 入队");
        assert_eq!(msg.ident, crate::slave::RM_DELAYMAGIC);
        assert_eq!(
            msg.w_param,
            i32::max(0, lo + draw),
            "nPower = MAX(0, Lo+draw)"
        );
        assert_eq!(msg.n_param1, make_long(12, 10), "MakeLong(x, y)");
        assert_eq!(msg.n_param2, 1, "nRage=1");
        assert_eq!(msg.n_param3, p, "nParam3=目标 ActorId");
        assert_eq!(msg.deliver_at_ms, 100_200, "200ms 自延时");
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(
            refs.len(),
            2,
            "RM_HIT + 一条 RM_10205（Δ17 被 ViewRange 挡）"
        );
        assert_eq!(refs[0].ident, RM_HIT, "RM_HIT 先于循环");
        assert_eq!(refs[1].ident, RM_10205);
        assert_eq!((refs[1].n_param1, refs[1].n_param2), (12, 10));
        assert_eq!(refs[1].n_param3, 1);

        // 200ms 前不投递、到点投递一次。
        world.clock_ms = 100_199;
        assert!(world.deliver_self_delay_msgs().is_empty(), "199ms 不得到点");
        world.clock_ms = 100_200;
        let out = world.deliver_self_delay_msgs();
        assert_eq!(out.len(), 1, "200ms 到点投递一次");
        assert_eq!(out[0].ident, crate::slave::RM_DELAYMAGIC);
    }

    /// R04 边界：`VisibleActors` 为空 ⇒ 不调用 helper（Run 的门），且 `CanMove=false` 同样不调用。
    #[test]
    fn mb1_big_heart_run_gates() {
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 115, 10, 10);
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(9);
        let mut probe = RandomNumber::with_seed(9);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Animal)
        );
        assert!(world.self_delay_msgs.is_empty(), "无可见目标 ⇒ 不攻击");
        assert_eq!(world.rng.random(), probe.random(), "零抽");
        // 有目标但 CanMove=false（Ghost/Death/STONE）⇒ 不进 helper。
        let p = mb1_players(&mut world, 12, 10);
        mb1_see(&mut world, id, p);
        world.entities.get_mut(id).unwrap().ai.attack_tick = 0;
        world.entities.get_mut(id).unwrap().death = true;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Animal)
        );
        assert!(world.self_delay_msgs.is_empty(), "Death ⇒ 不攻击");
    }
    // ------------------------------------------------------------------
    // **MB-2**（R13/R24/R08/R43）——构造级用例。
    // ------------------------------------------------------------------

    /// 找一个满足 `Random(5)` 指定分支的种子。
    fn mb2_seed_mod5(want_zero: bool) -> i32 {
        (0..100_000)
            .find(|seed| (RandomNumber::with_seed(*seed).random_below(5) == 0) == want_zero)
            .expect("seed")
    }

    /// 找一个满足 `Random(3)` 指定分支的种子。
    fn mb2_seed_mod3(want_zero: bool) -> i32 {
        (0..100_000)
            .find(|seed| (RandomNumber::with_seed(*seed).random_below(3) == 0) == want_zero)
            .expect("seed")
    }

    /// R13（Run）：`:120/:122` 四处判定**只读 `CurrX`**（`Δy` 完全不参与，照抄 bug）；
    /// `Random(5)` 只在 `Δx<=2` 支抽一次、参数 5；3-4 格支**零抽**直接 `GetBackPosition`。
    #[test]
    fn mb2_r13_run_reads_only_currx_and_draws_random5_in_two_tile_branch() {
        let zero = mb2_seed_mod5(true);
        let nonzero = mb2_seed_mod5(false);
        let run = |seed: i32, tx: i32, ty: i32| -> (i32, i32, usize) {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 87, 10, 10);
            let p = mb1_players(&mut world, tx, ty);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.ai.search_enemy_tick = 100_000; // 跳过 5s 扫描
                e.ai.target_creature = Some(p);
            }
            world.clock_ms = 100_000;
            world.rng = RandomNumber::with_seed(seed);
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            let e = world.entities.get(id).unwrap();
            // 统计本函数消耗的取数次数（用独立探针复跑同一路径不现实；这里用
            // 「Random(5) 是否发生」的两支断言 + 下方零抽支的流对比）。
            let _ = e;
            (e.ai.target_x, e.ai.target_y, 0)
        };
        // Δx=1, Δy=4：`Δx<=2` 支（随机 0）⇒ GetBackPosition（Dir=Up ⇒ y+1）。
        let (tx, ty, _) = run(zero, 11, 14);
        assert_eq!((tx, ty), (10, 11), "Δy 不参与 ⇒ 必须进 ≤2 支并回退");
        // 随机非 0 ⇒ 不回退。
        let (tx, ty, _) = run(nonzero, 11, 14);
        assert_eq!((tx, ty), (-1, 0), "Random(5)!=0 ⇒ 不改 TargetX/Y");
        // **`Δx=0, Δy=4` 通过 ≤2 门并抽 Random(5)**（判据钉死）。
        let (tx, ty, _) = run(zero, 10, 14);
        assert_eq!((tx, ty), (10, 11), "Δx=0,Δy=4 必须进 ≤2 支");
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let p = mb1_players(&mut world, 10, 14);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.search_enemy_tick = 100_000;
            e.ai.target_creature = Some(p);
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(zero);
        let mut probe = RandomNumber::with_seed(zero);
        let _ = probe.random_below(5);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "Δx=0,Δy=4 必须抽一次 Random(5)（修成判 Δy 会走 3-4 支 ⇒ 红）"
        );
        // Δx=3（3-4 格支）：直接回退、**零抽**。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let p = mb1_players(&mut world, 13, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.search_enemy_tick = 100_000;
            e.ai.target_creature = Some(p);
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(424242);
        let mut probe = RandomNumber::with_seed(424242);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.ai.target_x, e.ai.target_y), (10, 11), "3-4 格直接回退");
        assert_eq!(world.rng.random(), probe.random(), "3-4 格支零抽");
        // Δx=5：外层 `<=4` 不通过 ⇒ 零抽、不改 TargetX/Y。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let p = mb1_players(&mut world, 15, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.search_enemy_tick = 100_000;
            e.ai.target_creature = Some(p);
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(424242);
        let mut probe = RandomNumber::with_seed(424242);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.ai.target_x, e.ai.target_y), (-1, 0), "Δx=5 不回退");
        assert_eq!(world.rng.random(), probe.random(), "Δx=5 零抽");
        // ≤2 支恰抽一次 `Random(5)`。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let p = mb1_players(&mut world, 11, 14);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.search_enemy_tick = 100_000;
            e.ai.target_creature = Some(p);
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(zero);
        let mut probe = RandomNumber::with_seed(zero);
        let _ = probe.random_below(5);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert_eq!(world.rng.random(), probe.random(), "恰一次 Random(5)");
    }

    /// R13（AttackTarget）：`:42` 两处都判 `CurrX`（`Δy` 不参与，照抄 bug）；
    /// `AttackMax-1 > _mNAttackCount` 才飞斧（`Random(|Hi(DC)-Lo(DC)|+1)` + AC 两抽），
    /// 否则只抽 `Random(5)`；`RM_FLYAXE` 帧带目标坐标快照。
    #[test]
    fn mb2_r13_attack_target_flyaxe_and_count_random5() {
        // 计数未耗尽（count=0, AttackMax=2）⇒ FlyAxe。
        let mut world = World::new(64, 64);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let t = monster(&mut world, 11, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.dc = (9 << 8) | 5; // Lo=5, Hi=9 ⇒ Random(5)
            e.ai.attack_tick = 0;
            e.ai.target_creature = Some(t);
            e.ai.m_n_attack_count = 0;
        }
        {
            let t = world.entities.get_mut(t).unwrap();
            t.ac = 0;
            t.hp = 60;
            t.max_hp = 60;
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(31415);
        let mut probe = RandomNumber::with_seed(31415);
        let draw = probe.random_below(5);
        let _ac = probe.random_range(0, 1);
        assert!(dual_axe_attack_target(&mut world, id));
        assert_eq!(world.rng.random(), probe.random(), "Random(5)+AC 两抽");
        assert_eq!(world.entities.get(id).unwrap().ai.m_n_attack_count, 1);
        assert_eq!(world.entities.get(id).unwrap().ai.attack_tick, 100_000);
        assert_eq!(world.entities.get(t).unwrap().hp, 60 - (5 + draw) as u16);
        assert_eq!(world.pending_ai_attacks.len(), 1);
        let frames = world.pending_monster_ref_msgs.clone();
        let fly = frames
            .iter()
            .find(|m| m.ident == RM_FLYAXE)
            .expect("RM_FLYAXE");
        assert_eq!((fly.n_param1, fly.n_param2), (10, 10), "头 = 施法者坐标");
        assert_eq!(fly.n_param3, t);
        assert_eq!((fly.target_x, fly.target_y), (11, 10), "尾 = 目标坐标快照");
        assert_eq!(fly.dir, DIR_RIGHT, "Dir = GetNextDirection");

        // 计数耗尽（count=1）⇒ 只抽 `Random(5)`，不飞斧。
        for want_zero in [true, false] {
            let seed = mb2_seed_mod5(want_zero);
            let mut world = World::new(64, 64);
            let id = mb1_monster(&mut world, 87, 10, 10);
            let t = monster(&mut world, 11, 10);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.ai.attack_tick = 0;
                e.ai.target_creature = Some(t);
                e.ai.m_n_attack_count = 1;
            }
            world.clock_ms = 100_000;
            world.rng = RandomNumber::with_seed(seed);
            let mut probe = RandomNumber::with_seed(seed);
            let d = probe.random_below(5);
            assert!(dual_axe_attack_target(&mut world, id));
            assert_eq!(
                world.rng.random(),
                probe.random(),
                "耗尽 ⇒ 恰一次 Random(5)"
            );
            assert_eq!(
                world.entities.get(id).unwrap().ai.m_n_attack_count,
                if d == 0 { 0 } else { 1 }
            );
            assert!(world.pending_ai_attacks.is_empty(), "不飞斧");
            assert!(world.pending_monster_ref_msgs.is_empty(), "无 RM_FLYAXE");
        }

        // **`Δy` 不参与**：Δx=1, Δy=8 ⇒ 仍进 `<=7` 支并飞斧（修成判两轴 ⇒ 本断言红）。
        let mut world = World::new(64, 64);
        let id = mb1_monster(&mut world, 87, 10, 10);
        let t = monster(&mut world, 11, 18);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.dc = (9 << 8) | 5;
            e.ai.attack_tick = 0;
            e.ai.target_creature = Some(t);
        }
        world.entities.get_mut(t).unwrap().ac = 0;
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(1);
        assert!(dual_axe_attack_target(&mut world, id), "Δy=8 不得挡住");
        assert_eq!(world.pending_ai_attacks.len(), 1, "Δy=8 仍飞斧");
    }

    /// R24：`Random(3)!=0 ⇒ base.Run(); return`（不 LightningAttack）；`==0 ⇒ GetBackPosition`；
    /// `LightingAttack` 的 `Random(|Hi-Lo|+1)` + 逐格 `Random(10)`、`RM_LIGHTING` 帧、
    /// 1.5 倍不死复合、`BreakHolySeizeMode`。
    #[test]
    fn mb2_r24_lighting_zombi_random3_and_lightning_attack() {
        let run = |seed: i32, attack_tick: i64| -> (i32, i32, bool, usize, usize) {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 94, 10, 10);
            let p = mb1_players(&mut world, 11, 11);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.ai.search_enemy_tick = 90_000;
                e.ai.target_creature = Some(p);
                e.ai.attack_tick = attack_tick;
            }
            world.clock_ms = 100_000;
            world.rng = RandomNumber::with_seed(seed);
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            let e = world.entities.get(id).unwrap();
            (
                e.ai.target_x,
                e.ai.target_y,
                world
                    .pending_monster_ref_msgs
                    .iter()
                    .any(|m| m.ident == RM_LIGHTING),
                world.self_delay_msgs.len(),
                world.pending_ai_attacks.len(),
            )
        };
        // `Random(3)!=0` ⇒ 跳过整段（不回退、不攻击）。
        let nonzero = mb2_seed_mod3(false);
        let (tx, ty, lit, delays, attacks) = run(nonzero, 100_000);
        assert_eq!((tx, ty), (-1, 0), "Random(3)!=0 ⇒ 不回退");
        assert!(!lit && delays == 0 && attacks == 0, "且不 LightningAttack");
        // `Random(3)==0` ⇒ GetBackPosition（Dir=Up ⇒ y+1）；攻击门未到 ⇒ 不攻击。
        let zero = mb2_seed_mod3(true);
        let (tx, ty, lit, delays, _) = run(zero, 100_000);
        assert_eq!((tx, ty), (10, 11), "Random(3)==0 ⇒ GetBackPosition");
        assert!(!lit && delays == 0, "NextHitTime 未到 ⇒ 不攻击");

        // 攻击支：Δx=5（≤4 门不通过）⇒ 直接 LightningAttack。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 94, 10, 10);
        let p = mb1_real_player(&mut world, 15, 10);
        world.entities.get_mut(p).unwrap().anti_magic = 0;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.dc = (9 << 8) | 5; // Lo=5, Hi=9 ⇒ Random(5)
            e.ai.search_enemy_tick = 90_000;
            e.ai.target_creature = Some(p);
            e.ai.attack_tick = 0;
            e.ai.holy_seize = true;
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(2718);
        let mut probe = RandomNumber::with_seed(2718);
        let draw = probe.random_below(5);
        let _anti = probe.random_below(10);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "Random(5) + 每格 Random(10)"
        );
        let n_power = i32::max(0, 5 + draw);
        let expected = crate::magic_effect::scale_undead_attack(n_power);
        let msg = world
            .self_delay_msgs
            .peek(p)
            .expect("RM_MAGSTRUCK 入队（被击中者自己的队列）");
        assert_eq!(msg.ident, crate::slave::RM_MAGSTRUCK);
        assert_eq!(msg.n_param1, expected, "MagPassThroughMagic 的 1.5 倍");
        assert_eq!(msg.actor_ref, id);
        assert_eq!(msg.deliver_at_ms, 100_600, "600ms");
        let lit = world
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_LIGHTING)
            .expect("RM_LIGHTING");
        assert_eq!(lit.w_param, 1);
        assert_eq!((lit.n_param1, lit.n_param2), (10, 10));
        assert_eq!(lit.n_param3, p);
        assert_eq!((lit.target_x, lit.target_y), (15, 10), "尾 = 目标坐标快照");
        assert_eq!(lit.dir, DIR_RIGHT);
        assert!(
            !world.entities.get(id).unwrap().ai.holy_seize,
            "BreakHolySeizeMode"
        );
        assert_eq!(world.entities.get(id).unwrap().ai.attack_tick, 100_000);
    }

    /// R08：`:55` 两次都是 `|CurrX - baseObject.CurrX| <= 6`（`Δy` 不参与，照抄 bug）；
    /// `baseObject` 是扫描循环的**残留变量**（可见表最后一项），不是 `TargetCret`；
    /// `GetNextPosition` 起点是**目标格**。
    #[test]
    fn mb2_r08_chicken_deer_x_only_and_residual_variable() {
        let run = |visible: Vec<(i32, i32)>| -> (i32, i32, Option<i32>) {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 52, 10, 10);
            let mut ids = Vec::new();
            for (x, y) in visible {
                ids.push(mb1_players(&mut world, x, y));
            }
            world.entities.get_mut(id).unwrap().visible_actors = ids
                .iter()
                .map(|target| VisibleBaseObject {
                    target: *target,
                    flag: VisibleFlag::Show,
                })
                .collect();
            world.clock_ms = 100_000;
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            let e = world.entities.get(id).unwrap();
            (e.ai.target_x, e.ai.target_y, e.ai.target_creature)
        };
        // Δx=6, Δy=12 ⇒ 触发；`GetNextPosition(16,22,DOWN_RIGHT(3),5) = (21,27)`。
        let (tx, ty, target) = run(vec![(16, 22)]);
        assert_eq!((tx, ty), (21, 27), "Δx=6 触发；起点是目标格");
        assert!(target.is_some());
        // Δx=7 ⇒ 不触发（`<=6`）。
        let (tx, ty, _) = run(vec![(17, 22)]);
        assert_eq!((tx, ty), (-1, 0), "Δx=7 不触发");
        // 残留变量：`TargetCret`=近目标(11,10)，最后一顶在(30,10) ⇒ 用残留的 x 判距 ⇒ 不触发。
        let (tx, ty, target) = run(vec![(11, 10), (30, 10)]);
        assert_eq!((tx, ty), (-1, 0), "判的是残留变量而不是 TargetCret");
        assert!(target.is_some(), "TargetCret 仍是最近的(11,10)");
    }

    /// R43：`Die()` 在 `ZilKillCount>0` 时置 `ReAliveTick` 并抽 `Random(20)+4`，随后**无条件** `-=1`；
    /// `Run` 门 `ZilKillCount >= 0`（2→1→0 仍可复活、-1 才停）；复生减半 MaxHP/FightExp、
    /// `HP=MaxHP`、`RM_ALIVE`、`WalkTick=now+1000`；STONE/Ghost 阻止复活。
    #[test]
    fn mb2_r43_zilkin_die_counts_and_revive_gate() {
        // 构造：ctor 抽数落字段（不重抽）。
        let mut ai = crate::entity::AnimalState::default();
        let ctor = crate::entity::MonsterCtorDraws {
            zil_kill_count: 2,
            ..crate::entity::MonsterCtorDraws::default()
        };
        ai.seed_from_spawn(0, false, &ctor, 1_000);
        assert_eq!(ai.zil_kill_count, 2);
        assert_eq!(ai.zilkin_die_tick, 0);

        // Die 三次：2→1（抽）、1→0（抽）、0→-1（**零抽**，且不复位计时器）。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 96, 10, 10);
        world.entities.get_mut(id).unwrap().ai.zil_kill_count = 2;
        world.clock_ms = 50_000;
        world.rng = RandomNumber::with_seed(777);
        let mut probe = RandomNumber::with_seed(777);
        let d1 = probe.random_below(20);
        zil_kin_zombi_die(&mut world, id);
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.ai.zil_kill_count, 1);
            assert_eq!(e.re_alive_tick, 50_000);
            assert_eq!(e.ai.zilkin_die_tick, i64::from(d1 + 4) * 1000);
        }
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "Die 恰抽一次 Random(20)"
        );
        world.clock_ms = 60_000;
        let d2 = probe.random_below(20);
        zil_kin_zombi_die(&mut world, id);
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.ai.zil_kill_count, 0);
            assert_eq!(e.re_alive_tick, 60_000);
            assert_eq!(e.ai.zilkin_die_tick, i64::from(d2 + 4) * 1000);
        }
        world.clock_ms = 70_000;
        zil_kin_zombi_die(&mut world, id);
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.ai.zil_kill_count, -1);
            assert_eq!(e.re_alive_tick, 60_000, "count=0 ⇒ 不重置");
            assert_eq!(e.ai.zilkin_die_tick, i64::from(d2 + 4) * 1000);
        }
        assert_eq!(world.rng.random(), probe.random(), "count=0 ⇒ 零抽");

        // `settle_deaths` 接线：hp=0 的 tick 会调用 Die 覆写。
        let mut world = World::new(32, 32);
        let id = mb1_monster(&mut world, 96, 10, 10);
        world.entities.get_mut(id).unwrap().ai.zil_kill_count = 1;
        world.entities.get_mut(id).unwrap().hp = 0;
        world.clock_ms = 10_000;
        world.rng = RandomNumber::with_seed(9);
        world.tick();
        // `World::tick` 每步推进 200ms（本骨架的 tick 相位），Death 时刻取推进后的时钟。
        let now_after_tick = world.clock_ms;
        assert_eq!(world.entities.get(id).unwrap().ai.zil_kill_count, 0);
        assert_eq!(
            world.entities.get(id).unwrap().re_alive_tick,
            now_after_tick
        );

        // 复生门（`>=` 恰等放行）。
        let setup = |stone: bool, ghost: bool| {
            let mut world = World::new(32, 32);
            let id = mb1_monster(&mut world, 96, 10, 10);
            let p = mb1_players(&mut world, 12, 10);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.death = true;
                e.ghost = ghost;
                e.status_time[crate::entity::POISON_STONE] = if stone { 1 } else { 0 };
                e.max_hp = 200;
                e.hp = 0;
                e.mon_exp = 100;
                e.re_alive_tick = 10_000;
                e.ai.zilkin_die_tick = 5_000;
                e.ai.zil_kill_count = 0;
                e.visible_actors = vec![VisibleBaseObject {
                    target: p,
                    flag: VisibleFlag::Show,
                }];
            }
            (world, id)
        };
        let (mut world, id) = setup(false, false);
        world.clock_ms = 14_999; // `now - re_alive_tick = 4999 < 5000`
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        assert!(world.entities.get(id).unwrap().death, "差 1ms 不复生");
        world.clock_ms = 15_000; // 恰等 ⇒ `>=` 放行
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert!(!e.death, "Death=false");
            assert_eq!(e.max_hp, 100, "MaxHP >>= 1");
            assert_eq!(e.hp, 100, "HP = MaxHP");
            assert_eq!(e.mon_exp, 50, "FightExp /= 2");
            assert_eq!(e.ai.walk_tick, 16_000, "WalkTick = now + 1000");
            assert!(!e.death_settled, "复活清死亡结算闩锁");
        }
        assert!(world
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_ALIVE));
        // STONE / Ghost 阻止复活。
        for (stone, ghost) in [(true, false), (false, true)] {
            let (mut world, id) = setup(stone, ghost);
            world.clock_ms = 20_000;
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            assert!(world.entities.get(id).unwrap().death, "STONE/Ghost 阻止");
        }
        // `monster_ai_tick` 的死怪分支必须仍调用 `ZilKinZombi.Run`。
        let (mut world, id) = setup(false, false);
        world.clock_ms = 20_000;
        world.enable_monster_ai();
        monster_ai_tick(&mut world);
        assert!(!world.entities.get(id).unwrap().death, "死怪分支驱动复生");
    }

    // ------------------------------------------------------------------
    // **MB-3**（R07/R21/R34/R02）构造级用例 + 改坏必红。
    // ------------------------------------------------------------------

    /// MB-3 专用怪：按 race 落 ctor 常量（`apply_monster_class_constants`）。
    fn class_monster(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "mb3", x, y);
        e.race = race;
        e.hp = 1000;
        e.max_hp = 1000;
        e.dc = 5 | (5 << 8); // Lo=5, Hi=5 ⇒ `Random(1)`（仍抽一次）
        crate::mongen::apply_monster_class_constants(&mut e, i16::from(race), world.clock_ms);
        world.add_entity(e)
    }

    fn link_visible(world: &mut World, viewer: i32, target: i32) {
        let vb = VisibleBaseObject {
            target,
            flag: VisibleFlag::Show,
        };
        world
            .entities
            .get_mut(viewer)
            .expect("viewer")
            .visible_actors
            .push(vb);
    }

    fn seed_where(mut pred: impl FnMut(&mut RandomNumber) -> bool) -> i32 {
        for seed in 0..200_000 {
            let mut r = RandomNumber::with_seed(seed);
            if pred(&mut r) {
                return seed;
            }
        }
        unreachable!("应能在 20 万种子里找到目标抽数序列");
    }

    /// R07 牙（门）：非隐藏支的 **3s 门**。2999ms 不调 `AttackTarget`（不抽数、`AttackTick` 不变）；
    /// 3001ms 才调。把 3000 改成 10000（或把 3s/10s 门互换）⇒ 本用例红。
    #[test]
    fn centipede_king_uses_3s_attack_gate_not_10s() {
        for (ago, should_fire) in [(2_999_i64, false), (3_001_i64, true)] {
            let mut world = World::new(30, 30);
            world.clock_ms = 100_000;
            let id = class_monster(&mut world, 107, 10, 10);
            let p = player(&mut world, 11, 10);
            link_visible(&mut world, id, p);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.fixed_hide_mode = false;
                e.ai.walk_speed = 0;
                e.ai.walk_tick = 99_999;
                e.ai.next_hit_time = 0;
                e.ai.attack_tick = 99_999;
                e.class_attack_tick = 100_000 - ago;
            }
            let seed = 7;
            world.rng = RandomNumber::with_seed(seed);
            centipede_king_run(&mut world, id);
            let e = world.entities.get(id).unwrap();
            if should_fire {
                assert_eq!(e.ai.attack_tick, 100_000, "3001ms ⇒ AttackTarget 被调用");
            } else {
                assert_eq!(
                    e.ai.attack_tick, 99_999,
                    "2999ms ⇒ 3s 门未到，不得调 AttackTarget"
                );
                assert_eq!(
                    world.rng.random(),
                    RandomNumber::with_seed(seed).random(),
                    "门未到 ⇒ 零抽数"
                );
            }
        }
    }

    /// R07 牙（抽数序）：`Random(4) != 0` ⇒ **不抽** `Random(3)`；
    /// `==0` 且 `Random(3) != 0` ⇒ `DECHEALTH,60,3`；`==0` 且 `Random(3)==0` ⇒ `STONE,5,0`。
    #[test]
    fn centipede_king_attack_target_draws_and_poison_branches() {
        // 分支 1：Random(4) != 0（2 抽）。
        let seed1 = seed_where(|r| {
            let _ = r.random_below(1);
            r.random_below(4) != 0
        });
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        let p = player(&mut world, 11, 10);
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.next_hit_time = 0;
            e.ai.attack_tick = 99_999;
        }
        world.rng = RandomNumber::with_seed(seed1);
        let mut expect = RandomNumber::with_seed(seed1);
        let _ = expect.random_below(1);
        let r4 = expect.random_below(4);
        assert_ne!(r4, 0);
        assert!(centipede_king_attack_target(&mut world, id));
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "Random(4)!=0 ⇒ 只抽 Random(DC)+Random(4)，不得抽 Random(3)"
        );
        assert_eq!(world.entities.get(id).unwrap().ai.target_creature, None);

        // 分支 2：Random(4)==0 且 Random(3)!=0 ⇒ 绿毒（3 抽）。
        let seed2 = seed_where(|r| {
            let _ = r.random_below(1);
            r.random_below(4) == 0 && r.random_below(3) != 0
        });
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        let p = player(&mut world, 11, 10);
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.next_hit_time = 0;
            e.ai.attack_tick = 99_999;
        }
        world.rng = RandomNumber::with_seed(seed2);
        let mut expect = RandomNumber::with_seed(seed2);
        let _ = expect.random_below(1);
        assert_eq!(expect.random_below(4), 0);
        assert_ne!(expect.random_below(3), 0);
        assert!(centipede_king_attack_target(&mut world, id));
        assert_eq!(world.rng.random(), expect.random(), "绿毒支 3 抽");
        assert!(world.buffs.list(p).is_some_and(|l| l
            .iter()
            .any(|b| b.buff_type == crate::buff::BuffType::GreenPoison)));
        assert_eq!(world.entities.get(id).unwrap().ai.target_creature, Some(p));

        // 分支 3：Random(4)==0 且 Random(3)==0 ⇒ 石化（3 抽）。
        let seed3 = seed_where(|r| {
            let _ = r.random_below(1);
            r.random_below(4) == 0 && r.random_below(3) == 0
        });
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        let p = player(&mut world, 11, 10);
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.next_hit_time = 0;
            e.ai.attack_tick = 99_999;
        }
        world.rng = RandomNumber::with_seed(seed3);
        let mut expect = RandomNumber::with_seed(seed3);
        let _ = expect.random_below(1);
        assert_eq!(expect.random_below(4), 0);
        assert_eq!(expect.random_below(3), 0);
        assert!(centipede_king_attack_target(&mut world, id));
        assert_eq!(world.rng.random(), expect.random(), "石化支 3 抽");
        assert!(world.buffs.list(p).is_some_and(|l| l
            .iter()
            .any(|b| b.buff_type == crate::buff::BuffType::Paralysis)));
    }

    /// R07 牙：隐藏支 10s 门 ⇒ `FindAttackTarget`（HP 回满）+ `_attackTick = now`；
    /// 非隐藏支未命中且 10s 门 ⇒ `ComeDown()` + `_attackTick = now`。
    #[test]
    fn centipede_king_find_attack_target_refills_hp_and_come_down_resets_tick() {
        // 隐藏支。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        let p = player(&mut world, 12, 12); // 两轴 < ComeOutValue(4)
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = true;
            e.hp = 1;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.class_attack_tick = 89_999; // 10s 门过
        }
        centipede_king_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.fixed_hide_mode, "FindAttackTarget ⇒ FixedHideMode=false");
        assert_eq!(e.hp, e.max_hp, "FindAttackTarget 覆写把 HP 回满");
        assert_eq!(e.class_attack_tick, 100_000, "_attackTick = now");

        // 非隐藏支：可见集为空 ⇒ AttackTarget=false；**10s 未到**（仅 3001ms）⇒ 不 ComeDown。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.class_attack_tick = 96_999; // 3001ms < 10000ms
        }
        centipede_king_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.fixed_hide_mode, "10s 未到不得 ComeDown");
        assert_eq!(e.class_attack_tick, 96_999, "10s 未到不得刷新 _attackTick");

        // 非隐藏支：可见集为空 ⇒ AttackTarget=false ⇒ 10s 门 ⇒ ComeDown。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 107, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.class_attack_tick = 89_999;
        }
        centipede_king_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(e.fixed_hide_mode, "ComeDown ⇒ FixedHideMode=true");
        assert_eq!(e.class_attack_tick, 100_000, "ComeDown 后 _attackTick=now");
        assert!(world
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_DIGDOWN));
    }

    /// R21 牙（取数序）：`GasAttack` 在 `baseObject != null` 后**先抽** `Random(3)`，
    /// `==0` 才判 `HideMode`；隐藏目标 ⇒ `StatusTimeArr[STATETRANSPARENT]=1`。
    #[test]
    fn gas_moth_gas_attack_draws_random3_before_hide_mode() {
        for (hide, want_status) in [(true, true), (false, false)] {
            let seed = seed_where(|r| {
                let _ = r.random_below(1); // DC
                let _ = r.random_below(0); // Random(SpeedPoint=0)
                let _ = r.random_below(1); // MAC
                let _ = r.random_below(10); // 玩家耐久
                let _ = r.random_below(20); // AntiPoison+20
                r.random_below(3) == 0
            });
            let mut world = World::new(30, 30);
            world.clock_ms = 100_000;
            let id = class_monster(&mut world, 105, 10, 10);
            let p = player(&mut world, 11, 10);
            link_visible(&mut world, id, p);
            // `GetPoseCreate` 走 `Envir.GetMovingObject`（格子 ObjList）⇒ 目标必须真正落图。
            assert!(world.enter_world(p));
            {
                let e = world.entities.get_mut(id).unwrap();
                e.ai.dir = crate::magic::DIR_RIGHT;
                e.hit_point = 1;
                e.ai.next_hit_time = 0;
                e.ai.attack_tick = 99_999;
                e.ai.target_creature = Some(p);
            }
            {
                let t = world.entities.get_mut(p).unwrap();
                t.speed_point = 0;
                t.hide_mode = hide;
            }
            world.rng = RandomNumber::with_seed(seed);
            let mut expect = RandomNumber::with_seed(seed);
            let _ = expect.random_below(1);
            let _ = expect.random_below(0);
            let _ = expect.random_below(1);
            let _ = expect.random_below(10);
            let _ = expect.random_below(20);
            assert_eq!(expect.random_below(3), 0);
            assert!(gas_attack_monster_attack_target(&mut world, id));
            assert_eq!(
                world.rng.random(),
                expect.random(),
                "GasAttack 取数序：DC→SpeedPoint→MAC→耐久→AntiPoison→Random(3)"
            );
            let t = world.entities.get(p).unwrap();
            assert_eq!(
                t.status_time[crate::entity::POISON_STATETRANSPARENT] == 1,
                want_status,
                "Random(3)==0 且 HideMode ⇒ 透明 1"
            );
            assert!(world.entities.get(p).unwrap().hp < 100, "命中扣血");
        }
    }

    /// R21 牙（Run）：`GasMothMonster.Run` 只索敌、**零抽数**；选曼哈顿最近 proper。
    #[test]
    fn gas_moth_run_selects_nearest_without_rng() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 105, 10, 10);
        let far = player(&mut world, 14, 10);
        let near = player(&mut world, 11, 12);
        link_visible(&mut world, id, far);
        link_visible(&mut world, id, near);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.ai.search_enemy_tick = 90_000;
        }
        let seed = 12345;
        world.rng = RandomNumber::with_seed(seed);
        gas_moth_monster_run(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().ai.target_creature,
            Some(near),
            "曼哈顿最近（|1|+|2|=3 < |4|+|0|=4）"
        );
        assert_eq!(
            world.rng.random(),
            RandomNumber::with_seed(seed).random(),
            "Run 只索敌、零抽数"
        );
    }

    /// R34 牙（整数除法）：`_mNDangerLevel > HP/MaxHP*5` 是 **C# `int/int`** ⇒
    /// `HP=MaxHP` 不召唤；`HP=MaxHP-1` 阈值为 0 ⇒ 召唤并递减。改成浮点 ⇒ `HP=MaxHP-1`
    /// 阈值为 4.995 ⇒ `5 > 4.995` 仍召唤（巧合通过），但 `HP=MaxHP*4/5` 会分叉；
    /// 本用例同时钉住「满血复位 5」。
    #[test]
    fn sculture_king_danger_level_uses_integer_division() {
        let seed = seed_where(|r| {
            let _ = r.random_below(6);
            true
        });
        // 满血：阈值为 5，5 > 5 不成立 ⇒ 不召唤。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.stone_mode = false;
            e.hp = 1000;
            e.danger_level = 5;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.ai.search_enemy_tick = 89_000; // >1000 且无目标 ⇒ 过门
            e.ai.target_creature = None;
        }
        world.rng = RandomNumber::with_seed(seed);
        sculture_king_run(&mut world, id);
        assert_eq!(world.entities.get(id).unwrap().danger_level, 5);
        assert!(world.pending_sculture_spawns.is_empty(), "满血不召唤");

        // HP=MaxHP-1：整数除法 ⇒ 阈值 0；5 > 0 ⇒ 召唤 + 递减到 4。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.stone_mode = false;
            e.hp = 999;
            e.danger_level = 5;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.ai.search_enemy_tick = 89_000;
            e.ai.target_creature = None;
        }
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let count = expect.random_below(6) + 6;
        sculture_king_run(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().danger_level,
            4,
            "整数除法阈值 0"
        );
        assert_eq!(world.pending_sculture_spawns.len(), 1);
        assert_eq!(world.pending_sculture_spawns[0].count, count, "Random(6)+6");
        assert_eq!(world.pending_sculture_spawns[0].x, 9);
        assert_eq!(world.pending_sculture_spawns[0].y, 11); // Dir=5(DownLeft) ⇒ 左下
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "CallSlave 抽数只此一次"
        );

        // **整数除法 vs 浮点的判别用例**：danger 已被递减到 4、HP=999 时，
        // `int/int` 阈值 = 0 ⇒ `4 > 0` **召唤**；浮点阈值 = 4.995 ⇒ `4 > 4.995` **不召唤**。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.stone_mode = false;
            e.hp = 999;
            e.danger_level = 4;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.ai.search_enemy_tick = 89_000;
            e.ai.target_creature = None;
        }
        world.rng = RandomNumber::with_seed(seed);
        sculture_king_run(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().danger_level,
            3,
            "整数除法的阈值 0：danger=4 > 0 ⇒ 召唤并递减（浮点实现会停在 4）"
        );
        assert_eq!(world.pending_sculture_spawns.len(), 1);

        // 满血复位：danger 已 4，HP==MaxHP ⇒ 回到 5。
        {
            let e = world.entities.get_mut(id).unwrap();
            e.hp = 1000;
            e.ai.search_enemy_tick = 89_000;
        }
        sculture_king_run(&mut world, id);
        assert_eq!(world.entities.get(id).unwrap().danger_level, 5);
    }

    /// R34 牙（MeltStone）：`StoneMode` + 目标 ≤2 ⇒ 融化：
    /// `CharStatusEx=0`、`StoneMode=false`、`RM_DIGUP`、**`ET_SCULPEICE` 5 分钟 MapEvent 注册**。
    #[test]
    fn sculture_king_melt_stone_registers_map_event() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        let p = player(&mut world, 12, 11); // 两轴 ≤2
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.stone_mode = true; // ctor 值
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.char_status_ex = 0x1;
        }
        sculture_king_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert!(!e.stone_mode, "MeltStone ⇒ StoneMode=false");
        assert_eq!(e.char_status_ex, 0, "CharStatusEx = 0");
        assert!(world
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_DIGUP));
        assert!(
            world.dig_out_events.iter().any(|ev| ev.event_type == 6
                && ev.x == 10
                && ev.y == 10
                && ev.continue_ms == 5 * 60 * 1000),
            "MeltStone 必须注册 ET_SCULPEICE 5min MapEvent（删掉必红）"
        );
    }

    /// R34 牙（生成链）：`flush_sculture_spawns` 按 `Random(4)` 逐次选 `Zuma[idx]` 造怪，
    /// 并在 `_mSlaveObjectList.Count >= 30` 时 **先判后抽**（不抽第 30+ 次）。
    #[test]
    fn sculture_king_flush_uses_zuma_names_and_count_gate() {
        struct RecordingSpawner {
            names: Vec<String>,
            draws: Vec<i32>,
        }
        impl crate::special_monster::SpecialSpawner for RecordingSpawner {
            fn regen_monster_by_name(
                &mut self,
                world: &mut World,
                rng: &mut RandomNumber,
                _map_id: usize,
                name: &str,
                x: i16,
                y: i16,
            ) -> Option<i32> {
                self.names.push(name.to_string());
                // 模拟 `CreateMonster` 的构造抽（记录与 Random(4) 的相对次序）。
                self.draws.push(rng.random_below(1));
                let e = Entity::new(0, name.to_string(), i32::from(x), i32::from(y));
                Some(world.add_entity(e))
            }
        }

        // 空表 + count=3 ⇒ 3 次 Random(4)，怪名逐个 = Zuma[抽值]。
        let seed = 4242;
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        world.rng = RandomNumber::with_seed(seed);
        world.pending_sculture_spawns.push(ScultureSpawnRequest {
            owner_id: id,
            map_id: 0,
            x: 10,
            y: 11,
            count: 3,
            kind: ClassSlaveSpawnKind::Sculture,
        });
        let mut spawner = RecordingSpawner {
            names: Vec::new(),
            draws: Vec::new(),
        };
        let mut expect = RandomNumber::with_seed(seed);
        let mut expected_names: Vec<String> = Vec::new();
        // `Random(4)` 与构造抽交错的次序：Random(4)→Random(1)→…
        for _ in 0..3 {
            expected_names.push(world.zuma_names[expect.random_below(4) as usize].clone());
            let _ = expect.random_below(1);
        }
        let spawned = world.flush_sculture_spawns(&mut spawner);
        assert_eq!(spawned.len(), 3);
        assert_eq!(spawner.names, expected_names, "每次 Random(4) 选 Zuma[idx]");
        assert_eq!(world.entities.get(id).unwrap().class_slave_list.len(), 3);
        assert_eq!(
            world.rng.random(),
            expect.random(),
            "Random(4) 与构造抽次序"
        );

        // 表内已有 29 只 ⇒ count=11 也只能再造 1 只（先判 `>=30` 再抽）。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.class_slave_list = (1..=29).collect();
        }
        world.rng = RandomNumber::with_seed(seed);
        world.pending_sculture_spawns.push(ScultureSpawnRequest {
            owner_id: id,
            map_id: 0,
            x: 10,
            y: 11,
            count: 11,
            kind: ClassSlaveSpawnKind::Sculture,
        });
        let mut spawner = RecordingSpawner {
            names: Vec::new(),
            draws: Vec::new(),
        };
        let spawned = world.flush_sculture_spawns(&mut spawner);
        assert_eq!(spawned.len(), 1, "29+1 ⇒ 到 30 即停");
        assert_eq!(spawner.names.len(), 1, "第 2 次起先判 >=30 不抽 Random(4)");
        assert_eq!(world.entities.get(id).unwrap().class_slave_list.len(), 30);
    }

    /// R02 牙：`ArcherGuard.Run` 曼哈顿最近 proper 选择 + 到点攻击（`Random(DC)` +
    /// `GetHitStruckDamage` + `RM_FLYAXE`）；无目标且 `GuardDirection>0` ⇒ 转向。
    #[test]
    fn archer_guard_run_picks_nearest_and_fires_flyaxe() {
        let seed = seed_where(|r| {
            let _ = r.random_below(1); // DC（Lo=Hi=5）
            true
        });
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 112, 10, 10);
        let far = player(&mut world, 14, 10); // 距离 4
        let near = player(&mut world, 11, 12); // 距离 3
        link_visible(&mut world, id, far);
        link_visible(&mut world, id, near);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
            e.ai.attack_tick = 90_000;
            e.ai.next_hit_time = 0;
        }
        world.rng = RandomNumber::with_seed(seed);
        archer_guard_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.target_creature, Some(near), "曼哈顿最近");
        // `AttackTarger`：Random(DC) → GetHitStruckDamage(AC 骰) → 玩家耐久 Random(10)
        let mut expect = RandomNumber::with_seed(seed);
        let _ = expect.random_below(1);
        let _ = expect.random_below(1);
        let _ = expect.random_below(10);
        assert_eq!(world.rng.random(), expect.random(), "AtackTarger 取数序");
        assert!(world
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_FLYAXE && m.n_param3 == near));
        assert!(world
            .pending_ai_attacks
            .iter()
            .any(|(a, t, _)| *a == id && *t == near));

        // 无目标 + GuardDirection>0 ⇒ TurnTo。
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 112, 10, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.dir = 0;
            e.guard_direction = 3;
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
        }
        archer_guard_run(&mut world, id);
        assert_eq!(
            world.entities.get(id).unwrap().ai.dir,
            3,
            "TurnTo(GuardDirection)"
        );
    }

    /// MB-3b F1 牙：`class_struck_apply` 的三条非 `_Attack` 路径**不发 `RM_STRUCK`、
    /// 不写 `LastHiter`**。把折叠块（`SetLastHiter` + `pending_struck.push`）恢复 ⇒ 红。
    #[test]
    fn mb3b_sculture_king_attack_does_not_emit_rm_struck() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        let master = player(&mut world, 20, 20);
        let mut target = Entity::new(0, "slave", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        target.hp = 1000;
        target.max_hp = 1000;
        target.ac = 0;
        target.mac = 0;
        target.master = Some(master); // `IsAttackTarget` 的 `target.Master != null` ⇒ proper
        let t = world.add_entity(target);
        assert!(
            world.enter_world(t),
            "目标必须落格（HitMagAttackTarget 枚举格对象）"
        );
        let before = world.entities.get(t).unwrap().hp;
        sculture_king_attack(&mut world, id, t);
        assert!(
            world.entities.get(t).unwrap().hp < before,
            "命中扣血（伤害 > 0）"
        );
        let t = world.entities.get(t).unwrap();
        assert!(t.last_hiter.is_none(), "HitMagAttackTarget 不 SetLastHiter");
        assert!(t.exp_hitter.is_none(), "ExpHitter 不被改写");
        assert!(
            t.ai.pending_struck.is_empty(),
            "不发 RM_STRUCK：恢复折叠块 ⇒ 目标 base.Run 多抽 Random(6)/Random(300)"
        );
    }

    /// MB-3b F1 牙（Gas 路径）：`GasAttackMonster.GasAttack` 同样只 `StruckDamage`。
    #[test]
    fn mb3b_gas_attack_does_not_emit_rm_struck() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 90, 10, 10);
        let master = player(&mut world, 20, 20);
        let mut target = Entity::new(0, "slave", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        target.hp = 1000;
        target.max_hp = 1000;
        target.ac = 0;
        target.mac = 0;
        target.speed_point = 0;
        target.master = Some(master);
        let t = world.add_entity(target);
        assert!(world.enter_world(t));
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.dir = crate::magic::DIR_RIGHT;
            e.hit_point = 1;
            e.ai.next_hit_time = 0;
            e.ai.attack_tick = 0;
            e.ai.target_creature = Some(t);
        }
        let before = world.entities.get(t).unwrap().hp;
        assert!(gas_attack_monster_attack_target(&mut world, id));
        let t = world.entities.get(t).unwrap();
        assert!(t.hp < before, "命中扣血");
        assert!(t.last_hiter.is_none(), "GasAttack 不 SetLastHiter");
        assert!(t.exp_hitter.is_none());
        assert!(t.ai.pending_struck.is_empty(), "GasAttack 不发 RM_STRUCK");
    }

    /// MB-3b F1b 牙：`ArcherGuard` 是**先 `SetLastHiter` 再 `StruckDamage`**（`ArcherGuard.cs:31-33`）。
    /// 顺序反了 ⇒ `StruckDamage` 的 `WarrMon` 倍率读到旧 LastHiter（本用例旧命中者 = 战士）。
    #[test]
    fn mb3b_archer_guard_sets_last_hiter_before_struck_damage() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        world.combat_config.warr_mon = 20;
        let id = class_monster(&mut world, 112, 10, 10); // DC=5/5 ⇒ Random(1)
        let old_hitter = player(&mut world, 20, 20);
        world.entities.get_mut(old_hitter).unwrap().job = 0; // Warrior
        let mut target = Entity::new(0, "target", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        target.hp = 100;
        target.max_hp = 100;
        target.ac = 0;
        target.mac = 0;
        target.last_hiter = Some(old_hitter);
        target.last_hiter_tick = 100_000;
        let t = world.add_entity(target);
        archer_guard_attack_targer(&mut world, id, t);
        let t = world.entities.get(t).unwrap();
        assert_eq!(
            t.hp, 95,
            "先 SetLastHiter ⇒ StruckDamage 读弓箭手（非旧战士）⇒ WarrMon 不生效；顺序反了扣 10"
        );
        assert_eq!(t.last_hiter, Some(id), "SetLastHiter(弓箭手)");
        assert!(t.exp_hitter.is_none(), "ExpHitter = null");
        assert!(t.ai.pending_struck.is_empty(), "ArcherGuard 不发 RM_STRUCK");
        assert!(world
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_FLYAXE));
    }

    /// MB-3b F2 牙：105（`GasMothMonster : GasAttackMonster : AtMonster`）的 `base.Run()` =
    /// `ATMonster.Run` 的 8s/1s 索敌门。外层 `GasMothMonster.Run` 的 WalkSpeed 门挡住时，
    /// 该门仍必须补跑 `SearchTarget`。去掉 `|| e.race == 105` ⇒ 本用例红。
    #[test]
    fn mb3b_gas_moth_base_run_keeps_at_monster_search_gate() {
        let mut world = World::new(30, 30);
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 105, 10, 10);
        let p = player(&mut world, 11, 10);
        link_visible(&mut world, id, p);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.walk_speed = 1000;
            e.ai.walk_tick = 99_999; // GasMothMonster.Run 外层门不过（1 < 1000）
            e.ai.search_enemy_tick = 90_000; // 10s > 8s ⇒ ATMonster 门过
            e.ai.think_tick = 100_000; // 跳过 Think 的额外路径
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        // 调用方对 `ClassRunBase::Monster` 的续跑 = oracle `base.Run()`。
        monster_run(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.ai.search_enemy_tick, 100_000, "ATMonster.Run 索敌门刷新");
        assert_eq!(e.ai.target_creature, Some(p), "SearchTarget 选中玩家");
    }

    /// MB-3b F4 牙：同格「死但未 Ghost」对象在 `boFlag=false` 枚举里（`base_objects_at` 的
    /// `boFlag=true` 口径漏掉它）。命中判定仍受 G09 折入版 `is_proper_target` 限制（登记）。
    #[test]
    fn mb3b_base_objects_bo_false_includes_dead_non_ghost() {
        let mut world = World::new(30, 30);
        let live = monster(&mut world, 11, 10);
        let dead = monster(&mut world, 11, 10);
        world.entities.get_mut(dead).unwrap().death = true;
        let ghost_dead = monster(&mut world, 11, 10);
        {
            let g = world.entities.get_mut(ghost_dead).unwrap();
            g.death = true;
            g.ghost = true;
        }
        assert!(world.enter_world(live));
        assert!(world.enter_world(dead));
        assert_eq!(
            world.base_objects_at_including_dead(0, 11, 10),
            vec![live, dead],
            "boFlag=false ⇒ !Ghost || !Death：含死但未 Ghost，排除 Ghost+Death"
        );
        assert_eq!(
            world.base_objects_at(0, 11, 10),
            vec![live],
            "既有 true 口径不含死对象"
        );
        let attacker = world.entities.get(live).cloned().unwrap();
        let dead_e = world.entities.get(dead).cloned().unwrap();
        assert!(
            !crate::combat::is_proper_target(&attacker, &dead_e),
            "G09 登记：死目标仍被折入版 is_proper_target 拒绝"
        );
    }

    // ═══════════════════════════════════════════════════════════════════════════════════════
    // 卡 `openmir2-rm-refmessage-delay-layer`（tier=R）：三条生产调用点的端到端。
    // 每条都钉：7 参载荷 / 延时值 / 到点广播帧 / 到点自投 RM_STRUCK 的效果。
    // ═══════════════════════════════════════════════════════════════════════════════════════

    /// 受击怪（带主人 ⇒ `IsProperTarget` 成立）准备：`ac=0/mac=0/speed_point=0`。
    fn refmsg_target(world: &mut World, x: i32, y: i32) -> i32 {
        let master = player(world, 20, 20);
        let mut target = Entity::new(0, "refmsg-target", x, y);
        target.race = ACTOR_RACE_MONSTER;
        target.hp = 1000;
        target.max_hp = 1000;
        target.ac = 0;
        target.mac = 0;
        target.speed_point = 0;
        target.master = Some(master);
        let t = world.add_entity(target);
        assert!(world.enter_world(t));
        t
    }

    /// **调用点 1**：`GasAttackMonster.GasAttack` ⇒ `SendStruckDelayMsg(…, 300)`
    /// （`GasAttackMonster.cs:30`）。
    #[test]
    fn refmsg_gas_attack_schedules_300ms() {
        let mut world = World::new(30, 30);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 90, 10, 10);
        let t = refmsg_target(&mut world, 11, 10);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.ai.dir = crate::magic::DIR_RIGHT;
            e.hit_point = 1;
            e.ai.next_hit_time = 0;
            e.ai.attack_tick = 0;
            e.ai.target_creature = Some(t);
        }
        assert!(gas_attack_monster_attack_target(&mut world, id));
        let m = world
            .self_delay_msgs
            .peek(t)
            .expect("GasAttack 必须入 7 参延时消息");
        assert_eq!(m.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(m.actor_ref, i32::from(crate::slave::RM_STRUCK));
        assert_eq!(m.w_param, 5, "wParam = GetMagStruckDamage 之后的值");
        assert_eq!(m.n_param2, 1000, "MaxHP 快照");
        assert_eq!(m.n_param3, id, "nParam3 = 命中者 ActorId");
        assert_eq!(m.deliver_at_ms, 100_300, "GasAttack 的延时 = 300ms");
        let hp_snapshot = world.entities.get(t).unwrap().hp;
        assert_eq!(m.n_param1, i32::from(hp_snapshot), "HP 快照 = 发送时刻");
        // 到点：广播 + 自投 RM_STRUCK（SetLastHiter + Struck）。
        assert!(world.entities.get(t).unwrap().last_hiter.is_none());
        world.clock_ms = 100_300;
        let outs = world.deliver_self_delay_msgs();
        assert_eq!(outs.iter().filter(|o| o.ref_message.is_some()).count(), 1);
        assert_eq!(
            world.pending_refmessage_frames.len(),
            1,
            "到点产生 1 条 SendRefMsg(RM_STRUCK) 广播"
        );
        let f = world.pending_refmessage_frames[0];
        assert_eq!(f.actor_id, t);
        assert_eq!(f.ident, crate::slave::RM_STRUCK);
        assert_eq!(f.w_param, 5);
        assert_eq!(f.n_param3, id);
        assert_eq!(
            world.entities.get(t).unwrap().last_hiter,
            Some(id),
            "自投 RM_STRUCK ⇒ SetLastHiter"
        );
    }

    /// **调用点 2**：`AnimalObject.HitMagAttackTarget` ⇒ `SendStruckDelayMsg(…, 200)`
    /// （`AnimalObject.cs:379`，ScultureKing 经它）。
    #[test]
    fn refmsg_hit_mag_attack_schedules_200ms() {
        let mut world = World::new(30, 30);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 102, 10, 10);
        let t = refmsg_target(&mut world, 11, 10);
        sculture_king_attack(&mut world, id, t);
        let m = world
            .self_delay_msgs
            .peek(t)
            .expect("HitMagAttackTarget 必须入 7 参延时消息");
        assert_eq!(m.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(m.actor_ref, i32::from(crate::slave::RM_STRUCK));
        assert_eq!(m.w_param, 5, "wParam = GetHit+GetMag 之和（倍率前）");
        assert_eq!(m.n_param2, 1000);
        assert_eq!(m.n_param3, id);
        assert_eq!(
            m.deliver_at_ms, 100_200,
            "HitMagAttackTarget 的延时 = 200ms"
        );
        world.clock_ms = 100_200;
        let outs = world.deliver_self_delay_msgs();
        assert_eq!(outs.iter().filter(|o| o.ref_message.is_some()).count(), 1);
        assert_eq!(
            world.entities.get(t).unwrap().last_hiter,
            Some(id),
            "自投 RM_STRUCK ⇒ SetLastHiter"
        );
    }

    /// **调用点 3**：`ArcherGuard.AttackTarger` ⇒ `max(|Δx|,|Δy|)*50 + 600`
    /// （`ArcherGuard.cs:34`）。
    #[test]
    fn refmsg_archer_guard_delay_is_chebyshev_times_50_plus_600() {
        let mut world = World::new(30, 30);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let id = class_monster(&mut world, 112, 10, 10);
        let t = refmsg_target(&mut world, 13, 10); // max(3,0)*50+600 = 750
        archer_guard_attack_targer(&mut world, id, t);
        let m = world
            .self_delay_msgs
            .peek(t)
            .expect("ArcherGuard 必须入 7 参延时消息");
        assert_eq!(m.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(m.actor_ref, i32::from(crate::slave::RM_STRUCK));
        assert_eq!(m.n_param3, id);
        assert_eq!(m.deliver_at_ms, 100_750, "max(|Δx|,|Δy|)*50+600");
        assert_eq!(m.w_param, 5);
        world.clock_ms = 100_750;
        let outs = world.deliver_self_delay_msgs();
        assert_eq!(outs.iter().filter(|o| o.ref_message.is_some()).count(), 1);
        assert_eq!(world.pending_refmessage_frames.len(), 1, "到点广播一次");
    }

    /// **判据 7（阴性对照）**：无命中（`struck_power == 0`）⇒ 不入延时消息、到点零抽数。
    #[test]
    fn refmsg_zero_damage_no_enqueue_and_zero_draws() {
        let mut world = World::new(30, 30);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let attacker = class_monster(&mut world, 90, 10, 10);
        {
            let e = world.entities.get_mut(attacker).unwrap();
            e.hit_point = 1;
            e.dc = 1 | (1 << 8); // Lo=Hi=1
        }
        let master = player(&mut world, 20, 20);
        let mut target = Entity::new(0, "armored", 11, 10);
        target.race = ACTOR_RACE_MONSTER;
        target.hp = 1000;
        target.max_hp = 1000;
        // AC 双字节 0xFFFF ⇒ `GetHitStruckDamage` 必为 0（`n_armor >= 255`）。
        target.ac = 0xFFFFu16 as i32;
        target.master = Some(master);
        let t = world.add_entity(target);
        let outcome = world.attack_once(attacker, t).expect("结算存在");
        assert_eq!(outcome.struck_power, 0, "护甲全免 ⇒ nPower = 0");
        assert!(
            world.self_delay_msgs.is_empty(),
            "nPower == 0 ⇒ 不得 SendStruckDelayMsg"
        );
        let before = world.rng.clone();
        world.clock_ms += 200;
        assert!(
            world.deliver_self_delay_msgs().is_empty(),
            "无延时消息 ⇒ 到点无投递"
        );
        assert_eq!(
            format!("{:?}", world.rng),
            format!("{before:?}"),
            "阴性对照：无命中零抽数"
        );
        assert!(world.pending_refmessage_frames.is_empty(), "无广播帧");
    }

    // ======================================================================================
    // **`openmir2-attacktarget-overrides`**（tier=R）——覆写族判据
    // ======================================================================================

    fn atk_monster(world: &mut World, race: u8, x: i32, y: i32) -> i32 {
        let mut e = Entity::new(0, "atk", x, y);
        e.race = race;
        e.hp = 1000;
        e.max_hp = 1000;
        e.dc = 5 | (9 << 8); // Lo=5, Hi=9 ⇒ `Random(5)`
        e.hit_point = 250;
        e.ai.next_hit_time = 2000;
        crate::mongen::apply_monster_class_constants(&mut e, i16::from(race), world.clock_ms);
        let id = world.add_entity(e);
        assert!(world.enter_world(id), "怪物落图");
        id
    }

    fn atk_player(world: &mut World, x: i32, y: i32, speed_point: u8) -> i32 {
        let mut e = Entity::new(0, "atkP", x, y);
        e.race = crate::entity::ACTOR_RACE_PLAY;
        e.hp = 500;
        e.max_hp = 500;
        e.speed_point = speed_point;
        e.ac = 0;
        e.mac = 0;
        let id = world.add_entity(e);
        assert!(world.enter_world(id), "玩家落图");
        id
    }

    fn atk_see(world: &mut World, viewer: i32, target: i32) {
        world.entities.get_mut(viewer).unwrap().visible_actors = vec![VisibleBaseObject {
            target,
            flag: VisibleFlag::Show,
        }];
    }

    /// 后续 3 个取值逐一相等 = 取数**次数与相对次序**一致（既有 trace 账本同口径）。
    fn assert_stream_eq(world: &mut World, expect: &mut RandomNumber, what: &str) {
        for i in 0..3 {
            assert_eq!(
                world.rng.random(),
                expect.random(),
                "{what}：后续第 {i} 个取值不同 ⇒ 抽数次数/次序漂移"
            );
        }
    }

    fn assert_stream_untouched(world: &mut World, before: &RandomNumber, what: &str) {
        assert_eq!(
            format!("{:?}", world.rng),
            format!("{before:?}"),
            "{what}：零抽数（随机流不得前进）"
        );
    }

    /// 找种子，使**前 `params.len()` 次** `random_below` 的最后一次为 0。
    fn seed_with_last_zero(params: &[i32]) -> i32 {
        for seed in 0..2_000_000 {
            let mut r = RandomNumber::with_seed(seed);
            let mut last = 1;
            for p in params {
                last = r.random_below(*p);
            }
            if last == 0 {
                return seed;
            }
        }
        unreachable!("应能找到末抽为 0 的种子");
    }

    // ------------------------------ SpitMap / TargetInSpitRange ------------------------------

    /// A03/G22：`TargetInSpitRange` 逐格复刻 `SpitMap`（8 向 × 5×5），含
    /// `|Δ|<=1` 的「GetAttackDir 失败也 return true」怪癖。
    ///
    /// 牙：把 `SPIT_MAP` 任一格改 0/1 ⇒ 本用例红。
    #[test]
    fn atk_target_in_spit_range_replicates_spit_map() {
        let mut world = World::new(30, 30);
        let m = atk_monster(&mut world, 130, 10, 10);
        let p = atk_player(&mut world, 0, 0, 10);
        for dy in -2i32..=2 {
            for dx in -2i32..=2 {
                if dx == 0 && dy == 0 {
                    continue;
                }
                {
                    let t = world.entities.get_mut(p).unwrap();
                    t.x = 10 + dx;
                    t.y = 10 + dy;
                }
                let got = target_in_spit_range(&world, m, p);
                // 表查找方向是 `GetNextDirection(…目标…)`（不是调用者的 btDir），
                // 与 oracle `TargetInSpitRange` 同点。
                let d = crate::magic::next_direction(10, 10, 10 + dx, 10 + dy);
                if dx.abs() <= 1 && dy.abs() <= 1 {
                    assert_eq!(got, Some(d), "邻接格 Δ=({dx},{dy})");
                } else {
                    let hit = SPIT_MAP[d as usize][(dy + 2) as usize][(dx + 2) as usize] == 1;
                    assert_eq!(got.is_some(), hit, "SpitMap[{d}][{}][{}]", dy + 2, dx + 2);
                    if hit {
                        assert_eq!(got, Some(d), "命中格方向应 = GetNextDirection");
                    }
                }
            }
        }
    }

    /// `|Δx|<=1 && |Δy|<=1` 支**无条件** `return true`：同格目标（`GetAttackDir` 必失败）也返回
    /// `Some(0)`（`btDir` 保持调用前值）。
    #[test]
    fn atk_target_in_spit_range_returns_true_for_same_cell() {
        let mut world = World::new(10, 10);
        let m = atk_monster(&mut world, 130, 5, 5);
        let p = atk_player(&mut world, 5, 5, 10);
        assert_eq!(target_in_spit_range(&world, m, p), Some(0));
        // 超 2 格 ⇒ false。
        let far = atk_player(&mut world, 8, 5, 10);
        assert_eq!(target_in_spit_range(&world, m, far), None);
    }

    /// A02/G21：`GetLongAttackDirDis` 的八格环扩张 + **Left 支不 break** 怪癖
    /// （正左方 `result=true` 但 `dir` 被块尾 `dir=0` 覆盖）。
    #[test]
    fn atk_get_long_attack_dir_dis_has_left_no_break_quirk() {
        let mut world = World::new(30, 30);
        let m = atk_monster(&mut world, 114, 10, 10);
        let at = |world: &mut World, dx: i32, dy: i32| atk_player(world, 10 + dx, 10 + dy, 10);
        let left3 = at(&mut world, -3, 0);
        assert_eq!(
            get_long_attack_dir_dis(&world, m, left3, 3),
            Some(0),
            "Left 无 break ⇒ dir=0"
        );
        let left1 = at(&mut world, -1, 0);
        assert_eq!(get_long_attack_dir_dis(&world, m, left1, 3), Some(0));
        let right3 = at(&mut world, 3, 0);
        assert_eq!(
            get_long_attack_dir_dis(&world, m, right3, 3),
            Some(DIR_RIGHT)
        );
        let upleft3 = at(&mut world, -3, -3);
        assert_eq!(
            get_long_attack_dir_dis(&world, m, upleft3, 3),
            Some(DIR_UP_LEFT)
        );
        let right4 = at(&mut world, 4, 0);
        assert_eq!(get_long_attack_dir_dis(&world, m, right4, 3), None);
        let same = at(&mut world, 0, 0);
        assert_eq!(get_long_attack_dir_dis(&world, m, same, 3), None);
    }

    // ------------------------------ DoubleCritical（130） ------------------------------

    /// E01：4×4 `SpitMap` 命中、物理减免、`nDamage` 跨格改写、300ms。
    ///
    /// 取数次序：`Random(|Hi-Lo|+1)` → `Random(SpeedPoint)` → `GetHitStruckDamage`
    /// （AC 骰）→ `StruckDamage`（玩家耐久 `Random(10)`）。
    #[test]
    fn atk_double_critical_hits_with_physical_damage_and_exact_draws() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 130, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 10, 9, 10); // 正上 1 格（SpitMap[UP][1][2]==1）
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = 424242;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let d0 = expect.random_below(5);
        let _speed = expect.random_below(10);
        let _ac = expect.random_range(0, 1);
        let _dur = expect.random_below(10);

        assert!(double_critical_attack_target(&mut world, m));
        assert_stream_eq(&mut world, &mut expect, "DoubleCritical 命中");
        assert_eq!(
            world.pending_ai_attacks.len(),
            1,
            "恰一次结算（4×4 里只有 UP 一支格命中）"
        );
        let outcome = world.pending_ai_attacks[0].2;
        assert_eq!(outcome.struck_power, (5 + d0) as u16, "nDamage = Lo+抽数");
        assert!(world.entities.get(p).unwrap().hp < 500, "StruckDamage 落血");
        assert_eq!(world.entities.get(m).unwrap().ai.attack_tick, 100_000);
        assert_eq!(world.entities.get(m).unwrap().ai.target_focus_tick, 100_000);
        assert!(
            !world.entities.get(m).unwrap().ai.holy_seize,
            "BreakHolySeize"
        );
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1, "仅 RM_HIT（SpitAttack 不发逐目标帧）");
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].w_param, i32::from(DIR_UP));
        let msg = world.self_delay_msgs.peek(p).expect("300ms 延时");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, 5 + d0);
        assert_eq!(msg.deliver_at_ms, 100_300);
        // oracle `DoubleCriticalMonster.cs:67` 写**未限定** `WAbil.HP/MaxHP` = 攻方快照。
        assert_eq!((msg.n_param1, msg.n_param2), (1000, 1000), "攻方 HP/MaxHP");
    }

    /// E01 未命中：2 格但不在 `SpitMap` 的正下方偏右 ⇒ `SetTargetXy`、false、零抽数。
    #[test]
    fn atk_double_critical_out_of_spit_map_sets_target_xy_without_draws() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 130, 10, 10);
        let p = atk_player(&mut world, 11, 12, 10); // Δ=(1,2) → DR_DOWNRIGHT[4][3]==0
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        let before = world.rng.clone();
        assert!(!double_critical_attack_target(&mut world, m));
        assert_stream_untouched(&mut world, &before, "不在 SpitMap");
        assert!(world.pending_monster_ref_msgs.is_empty());
        let e = world.entities.get(m).unwrap();
        assert_eq!((e.ai.target_x, e.ai.target_y), (11, 12));
    }

    // ------------------------------ SpitSpider（82/118/119） ------------------------------

    /// E09/G01：5×5 喷吐、魔法减免、300ms、攻方 `AntiPoison+20` 毒门。
    #[test]
    fn atk_spit_spider_hits_with_magic_damage_poison_and_exact_draws() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 82, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 10, 8, 10); // UP 2 格（SpitMap[UP][0][2]==1）
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = seed_with_last_zero(&[5, 10, 1, 10, 20]);
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let d0 = expect.random_below(5);
        let _speed = expect.random_below(10);
        let _mac = expect.random_below(1);
        let _dur = expect.random_below(10);
        let poison = expect.random_below(20);

        assert!(spit_spider_attack_target(&mut world, m, true));
        assert_stream_eq(&mut world, &mut expect, "SpitSpider 命中");
        assert_eq!(poison, 0, "测试种子须使毒门命中");
        assert_eq!(world.pending_ai_attacks.len(), 1);
        assert_eq!(
            world.pending_ai_attacks[0].2.struck_power,
            (5 + d0) as u16,
            "GetMagStruckDamage 后"
        );
        assert!(
            world.buffs.has_buff(p, crate::buff::BuffType::GreenPoison),
            "Random(20)==0 ⇒ MakePosion(DECHEALTH,30,1)"
        );
        assert_eq!(world.entities.get(p).unwrap().green_poisoning_point, 1);
        assert!(!world.entities.get(m).unwrap().ai.holy_seize);
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].w_param, i32::from(DIR_UP));
        let msg = world.self_delay_msgs.peek(p).expect("300ms");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, 5 + d0);
        // oracle `SpitSpider.cs:43` 同样写未限定的 `WAbil.HP/MaxHP` = 攻方快照。
        assert_eq!((msg.n_param1, msg.n_param2), (1000, 1000), "攻方 HP/MaxHP");
    }

    /// E09 未命中：3 格（超 `|Δ|<=2`）⇒ false + `SetTargetXy` + 零抽数。
    #[test]
    fn atk_spit_spider_out_of_range_sets_target_xy_without_draws() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 119, 10, 10);
        let p = atk_player(&mut world, 13, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        let before = world.rng.clone();
        assert!(!spit_spider_attack_target(&mut world, m, true));
        assert_stream_untouched(&mut world, &before, "超 2 格");
        assert!(world.pending_monster_ref_msgs.is_empty());
        assert_eq!(
            (
                world.entities.get(m).unwrap().ai.target_x,
                world.entities.get(m).unwrap().ai.target_y
            ),
            (13, 10)
        );
    }

    // ------------------------------ ExplosionSpider（117） ------------------------------

    /// E04/G11：邻接 + 到点 ⇒ `HP=0` + 半物理半魔法 AoE + 700ms；**不** `BreakHolySeize`。
    #[test]
    fn atk_explosion_spider_self_destructs_with_half_physical_half_magic() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 117, 10, 10);
        world.entities.get_mut(m).unwrap().dc = 2 | (7 << 8); // Random(6)
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 11, 10, 10);
        atk_see(&mut world, m, p);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = 90210;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let d0 = expect.random_below(6);
        let _ac = expect.random_range(0, 1);
        let _mac = expect.random_below(1);
        let _dur = expect.random_below(10);

        assert!(explosion_spider_attack_target(&mut world, m));
        assert_stream_eq(&mut world, &mut expect, "ExplosionSpider 自爆");
        assert_eq!(
            world.entities.get(m).unwrap().hp,
            0,
            "DoSelfExplosion 置 HP=0"
        );
        assert_eq!(world.entities.get(m).unwrap().ai.attack_tick, 100_000);
        assert_eq!(world.entities.get(m).unwrap().ai.target_focus_tick, 100_000);
        assert!(
            world.entities.get(m).unwrap().ai.holy_seize,
            "本覆写**不**调 BreakHolySeizeMode"
        );
        assert!(world.pending_monster_ref_msgs.is_empty(), "自爆不发 RM_HIT");
        let n_power = 2 + d0;
        let half = n_power / 2;
        assert_eq!(world.pending_ai_attacks.len(), 1);
        assert_eq!(
            world.pending_ai_attacks[0].2.struck_power,
            (half * 2) as u16,
            "GetHit(nPower/2) + GetMag(nPower/2)"
        );
        let msg = world.self_delay_msgs.peek(p).expect("700ms");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, half * 2);
        assert_eq!(msg.deliver_at_ms, 100_700);
        // `ExplosionSpider.cs:40` 显式 `baseObject.WAbil.HP/MaxHP` ⇒ 收件人**受击后**快照。
        assert_eq!(
            (msg.n_param1, msg.n_param2),
            (i32::from(world.entities.get(p).unwrap().hp), 500),
            "收件人 HP/MaxHP"
        );
    }

    /// E04 的 `Run` 覆写：`>60000` 严格门；到点一次后刷新，不重复。
    #[test]
    fn atk_explosion_spider_run_fires_once_per_60s_strictly() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 117, 10, 10);
        let p = atk_player(&mut world, 11, 10, 10);
        atk_see(&mut world, m, p);
        // 恰 60000 ⇒ 严格 `>` 不成立。
        world.entities.get_mut(m).unwrap().ai.explosion_make_time = 40_000;
        let before = world.rng.clone();
        explosion_spider_run(&mut world, m);
        assert_eq!(world.entities.get(m).unwrap().hp, 1000, "恰 60000 不自爆");
        assert_stream_untouched(&mut world, &before, "恰 60000");

        // 60001 ⇒ 自爆一次并刷新时刻。
        world.entities.get_mut(m).unwrap().ai.explosion_make_time = 39_999;
        explosion_spider_run(&mut world, m);
        assert_eq!(world.entities.get(m).unwrap().hp, 0);
        assert_eq!(
            world.entities.get(m).unwrap().ai.explosion_make_time,
            100_000
        );
        let draws_after_first = format!("{:?}", world.rng);
        let pending = world.pending_ai_attacks.len();
        explosion_spider_run(&mut world, m);
        assert_eq!(
            format!("{:?}", world.rng),
            draws_after_first,
            "同一时刻第二次 Run 不再自爆"
        );
        assert_eq!(world.pending_ai_attacks.len(), pending);
    }

    // ------------------------------ MagCow（91） ------------------------------

    /// E06/G04：`AntiMagic >= 0`（恒真）门 + 魔法减免 + 300ms；无命中率骰。
    #[test]
    fn atk_mag_cow_magic_attack_draws_and_frames() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 91, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 11, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = 777001;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let d0 = expect.random_below(5);
        let _mac = expect.random_below(1);
        let _dur = expect.random_below(10);

        assert!(mag_cow_attack_target(&mut world, m));
        assert_stream_eq(&mut world, &mut expect, "MagCow");
        assert_eq!(world.pending_ai_attacks.len(), 1);
        assert_eq!(world.pending_ai_attacks[0].2.struck_power, (5 + d0) as u16);
        assert!(!world.entities.get(m).unwrap().ai.holy_seize);
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].w_param, i32::from(DIR_RIGHT));
        let msg = world.self_delay_msgs.peek(p).expect("300ms");
        assert_eq!(msg.w_param, 5 + d0);
        assert_eq!(msg.deliver_at_ms, 100_300, "MagCow 300ms");
        // `MagCowMonster.cs:27` 显式 `baseObject.WAbil.HP/MaxHP` = **收件人**快照
        // （与 DoubleCritical/SpitSpider/ElfWarrior 的未限定写法相反）。
        assert_eq!(
            (msg.n_param1, msg.n_param2),
            (i32::from(world.entities.get(p).unwrap().hp), 500),
            "收件人 HP/MaxHP"
        );
    }

    /// E06：`nPower <= 0` ⇒ **不**发帧、不进 `GetPoseCreate`/减免；`AttackTarget` 仍返回 true。
    #[test]
    fn atk_mag_cow_zero_power_no_frame_no_further_draws() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 91, 10, 10);
        world.entities.get_mut(m).unwrap().dc = 0;
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        let p = atk_player(&mut world, 11, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        let before = world.rng.clone();
        assert!(mag_cow_attack_target(&mut world, m));
        // 只抽 `Random(1)`（Lo=0, Hi=0）一次。
        let mut expect = before.clone();
        let _ = expect.random_below(1);
        assert_stream_eq(&mut world, &mut expect, "nPower<=0");
        assert!(world.pending_monster_ref_msgs.is_empty());
        assert!(world.pending_ai_attacks.is_empty());
        assert_eq!(world.entities.get(p).unwrap().hp, 500);
        assert!(
            !world.entities.get(m).unwrap().ai.holy_seize,
            "仍 BreakHolySeize"
        );
    }

    // ------------------------------ MagicMonster（215） ------------------------------

    /// E07：邻接到点**只刷新两 tick**——零抽数、零帧、零伤害、**不** `BreakHolySeize`。
    #[test]
    fn atk_magic_monster_refreshes_ticks_without_attack() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 215, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 11, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        let before = world.rng.clone();
        assert!(magic_monster_attack_target(&mut world, m));
        assert_stream_untouched(&mut world, &before, "MagicMonster 无取数");
        assert!(world.pending_monster_ref_msgs.is_empty());
        assert!(world.pending_ai_attacks.is_empty());
        assert_eq!(world.entities.get(p).unwrap().hp, 500);
        assert_eq!(world.entities.get(m).unwrap().ai.attack_tick, 100_000);
        assert_eq!(world.entities.get(m).unwrap().ai.target_focus_tick, 100_000);
        assert!(
            world.entities.get(m).unwrap().ai.holy_seize,
            "不 BreakHolySeize"
        );
    }

    /// E07：`TargetCret == Master` ⇒ 直接清目标（不走 DelTargetCreat，`TargetX/Y` 不动），返回 false。
    #[test]
    fn atk_magic_monster_master_target_is_cleared_directly() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 215, 10, 10);
        let p = atk_player(&mut world, 11, 10, 10);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.master = Some(p);
            e.ai.target_creature = Some(p);
            e.ai.target_x = 99;
            e.ai.target_y = 98;
        }
        let before = world.rng.clone();
        assert!(!magic_monster_attack_target(&mut world, m));
        assert_stream_untouched(&mut world, &before, "Master 支零取数");
        let e = world.entities.get(m).unwrap();
        assert_eq!(e.ai.target_creature, None);
        assert_eq!(
            (e.ai.target_x, e.ai.target_y),
            (99, 98),
            "不走 DelTargetCreat"
        );
    }

    /// E07：不邻接同图 ⇒ `SetTargetXy`；异图 ⇒ `DelTargetCreat`（`TargetX/Y = -1`）。
    #[test]
    fn atk_magic_monster_non_adjacent_paths() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 215, 10, 10);
        let p = atk_player(&mut world, 12, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        let before = world.rng.clone();
        assert!(!magic_monster_attack_target(&mut world, m));
        assert_stream_untouched(&mut world, &before, "不邻接零取数");
        assert_eq!(
            (
                world.entities.get(m).unwrap().ai.target_x,
                world.entities.get(m).unwrap().ai.target_y
            ),
            (12, 10)
        );

        world.entities.get_mut(p).unwrap().map_id = 999;
        assert!(!magic_monster_attack_target(&mut world, m));
        let e = world.entities.get(m).unwrap();
        assert_eq!(e.ai.target_creature, None);
        assert_eq!((e.ai.target_x, e.ai.target_y), (-1, -1));
    }

    // ------------------------------ ElfWarrior（114） ------------------------------

    /// E03/G12：`Appr=704` ⇒ `SpitAttack` 的**Word** 口径取数（`DC + Random(DC+1)`）、
    /// `RM_STRUCKEFFECT` 300ms。
    #[test]
    fn atk_elf_warrior_enhanced_form_uses_word_dc_and_struckeffect() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 114, 10, 10);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.appr = 704;
            e.dc = 7; // Word 口径 ⇒ Random(|0-7|+1) = Random(8)
            e.ai.attack_tick = 90_000;
            e.ai.holy_seize = true;
        }
        let p = atk_player(&mut world, 11, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = 31415;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let _word = expect.random_below(8);
        let _speed = expect.random_below(10);
        let _mac = expect.random_below(1);
        let _dur = expect.random_below(10);

        assert!(elf_warrior_attack_target(&mut world, m));
        assert_stream_eq(&mut world, &mut expect, "ElfWarrior 强化形态");
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].w_param, i32::from(DIR_RIGHT));
        assert_eq!(world.pending_ai_attacks.len(), 1);
        assert_eq!(
            world.pending_ai_attacks[0].2.struck_power, 1,
            "nDamage 先被置 1，再 GetMagStruckDamage(mac=0) ⇒ 1"
        );
        assert!(!world.entities.get(m).unwrap().ai.holy_seize);
        let msg = world.self_delay_msgs.peek(p).expect("300ms");
        assert_eq!(
            msg.ident, RM_STRUCKEFFECT,
            "oracle 用 RM_STRUCKEFFECT 而非 RM_REFMESSAGE"
        );
        assert_eq!(msg.deliver_at_ms, 100_300);
        assert_eq!(msg.w_param, 1);
        // `ElfWarriorMonster.cs:65` 也是未限定 `WAbil.HP/MaxHP`（攻方快照）。
        assert_eq!((msg.n_param1, msg.n_param2), (1000, 1000), "攻方 HP/MaxHP");
    }

    /// E03/G12 + G19：`GetLongAttackDirDis(target,3)` 的 Left 怪癖决定 `SpitAttack` 的 `Dir=0`；
    /// 正右 3 格则为 `Dir=2`。两者都只抽一次 Word 骰（第二/三轮 G19 无限循环已登记截断）。
    #[test]
    fn atk_elf_warrior_long_attack_dir_left_quirk_controls_frame_dir() {
        let run = |dx: i32| {
            let mut world = World::new(40, 40);
            world.clock_ms = 100_000;
            let m = atk_monster(&mut world, 114, 10, 10);
            {
                let e = world.entities.get_mut(m).unwrap();
                e.appr = 706;
                e.dc = 7;
                e.ai.attack_tick = 90_000;
            }
            let p = atk_player(&mut world, 10 + dx, 10, 10);
            world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
            let seed = 2718;
            world.rng = RandomNumber::with_seed(seed);
            assert!(elf_warrior_attack_target(&mut world, m));
            let mut expect = RandomNumber::with_seed(seed);
            let _ = expect.random_below(8);
            assert_stream_eq(&mut world, &mut expect, "3 格长刺只抽 Word 骰");
            assert!(
                world.pending_ai_attacks.is_empty(),
                "前格无目标（G19 首轮）"
            );
            world.pending_monster_ref_msgs[0].w_param
        };
        assert_eq!(run(-3), 0, "Left 支不 break ⇒ 块尾 dir=0");
        assert_eq!(run(3), i32::from(DIR_RIGHT));
    }

    /// **F1（Critical）**：`ElfWarriorMonster : SpitSpider` ⇒ 非强化形态（部署 `神兽1`
    /// `Appr=171` 的常态）的 `base.AttackTarget()` 是 **`SpitSpider.AttackTarget`**，不是
    /// `MonsterObject` 的物理 `_Attack`。逐项断言 SpitSpider 语义：
    /// Byte DC 骰 → `RM_HIT` → 5×5 `SpitMap` 命中 → `GetMagStruckDamage` → `RM_REFMESSAGE`
    /// 300ms → `BreakHolySeizeMode`；`UsePoison=false`（ctor `:32`）⇒ **零** `Random(20)`、
    /// 无 `MakePosion`、**不**发 `RM_STRUCKEFFECT`。
    ///
    /// 牙：把 `:80` 回退改成 [`attack_target`]（旧实现）⇒ 本用例红（无 `RM_REFMESSAGE`/
    /// 取数序变物理版）。
    #[test]
    fn atk_elf_warrior_plain_form_uses_spit_spider_semantics() {
        let mut world = World::new(40, 40);
        world.enable_refmessage_delay();
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 114, 10, 10);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.appr = 171; // 部署 `神兽1` 的实际 Appr（非强化形态）
            e.ai.attack_tick = 90_000;
            e.ai.holy_seize = true;
        }
        let p = atk_player(&mut world, 11, 10, 10); // Δx=1 ⇒ TargetInSpitRange ⇒ DIR_RIGHT
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);

        let seed = 31415;
        world.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        // SpitSpider.SpitAttack（Byte 口径）：LoByte(DC)=5、HiByte(DC)=9 ⇒ Random(5)。
        let d0 = expect.random_below(5);
        let _speed = expect.random_below(10); // Random(SpeedPoint)
        let _mac = expect.random_below(1); // GetMagStruckDamage：MAC=0 ⇒ Random(1)
        let _dur = expect.random_below(10); // 玩家 StruckDamage 耐久

        // **UsePoison=false ⇒ 此处不得再抽 Random(20)。**

        assert!(elf_warrior_attack_target(&mut world, m));
        assert_stream_eq(&mut world, &mut expect, "ElfWarrior 非强化形态");

        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].w_param, i32::from(DIR_RIGHT));
        assert_eq!(world.pending_ai_attacks.len(), 1, "SpitSpider 魔法结算");
        assert_eq!(
            world.pending_ai_attacks[0].2.struck_power,
            (5 + d0) as u16,
            "Byte 口径：LoByte(DC)+Random(|Hi-Lo|+1)"
        );
        assert!(!world.entities.get(m).unwrap().ai.holy_seize);

        let msg = world.self_delay_msgs.peek(p).expect("300ms");
        assert_eq!(
            msg.ident,
            crate::slave::RM_REFMESSAGE,
            "SpitSpider 用 RM_REFMESSAGE（不是强化形态的 RM_STRUCKEFFECT）"
        );
        assert_eq!(msg.deliver_at_ms, 100_300);
        assert_eq!(msg.w_param, 5 + d0);
        assert!(
            !world.buffs.has_buff(p, crate::buff::BuffType::GreenPoison),
            "UsePoison=false ⇒ 无中毒"
        );
    }

    /// F2：`UsePoison` 按 race 传——118 命中后**零** `Random(20)`；119 恰一次。
    ///
    /// 牙：把 dispatch 的 118 臂改回 `true` ⇒ 118 轮断言红（多抽一次 Random(20)）。
    #[test]
    fn atk_spit_spider_use_poison_follows_race_ctor() {
        let run = |race: u8| {
            let mut world = World::new(40, 40);
            world.enable_refmessage_delay();
            world.clock_ms = 100_000;
            let m = atk_monster(&mut world, race, 10, 10);
            world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
            let p = atk_player(&mut world, 10, 8, 10); // UP 2 格（SpitMap[UP][0][2]==1）
            world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
            let seed = 4242;
            world.rng = RandomNumber::with_seed(seed);
            let mut expect = RandomNumber::with_seed(seed);
            let _d0 = expect.random_below(5);
            let _speed = expect.random_below(10);
            let _mac = expect.random_below(1);
            let _dur = expect.random_below(10);
            assert!(monster_attack_target(&mut world, m), "race {race} 命中");
            assert_eq!(world.pending_ai_attacks.len(), 1, "race {race} 命中一次");
            // 期望流位置：118 到此为止；119 还要抽 Random(20)。
            let poison_draw = if race == 119 {
                Some(expect.random_below(20))
            } else {
                None
            };
            assert_eq!(
                world.rng.random(),
                expect.random(),
                "race {race} 的 Random(20) 抽取次数不符合 UsePoison"
            );
            poison_draw
        };
        assert_eq!(run(118), None, "HighRiskSpider ctor UsePoison=false");
        assert!(run(119).is_some(), "BigPoisionSpider ctor UsePoison=true");
    }

    // ------------------------------ PercentMonster（80，死类） ------------------------------

    /// E08/G14：`PercentMonster.AttackTarget` 与基类逐行同构 ⇒ 走基版（物理 + BreakHolySeize）；
    /// dispatch 仍显式登记 race 80。
    #[test]
    fn atk_percent_monster_dispatches_to_base_identical_semantics() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 80, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        world.entities.get_mut(m).unwrap().ai.holy_seize = true;
        let p = atk_player(&mut world, 11, 10, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        assert!(monster_attack_target(&mut world, m));
        assert_eq!(world.pending_ai_attacks.len(), 1, "基版 Attack 结算");
        assert!(!world.entities.get(m).unwrap().ai.holy_seize);
        assert_eq!(world.entities.get(m).unwrap().ai.attack_tick, 100_000);
    }

    // ------------------------------ 分派表（E14/G18） ------------------------------

    /// E14：`monster_run` 的 `AttackTarget` 分派必须按 race 选到各覆写；
    /// 用「只有对应覆写才会发生的帧/伤害形态」逐族断言。
    ///
    /// 牙：删任一 `Some(<race>)` 臂 ⇒ 对应断言红（至少 130/117/215 三条可区分）。
    #[test]
    fn atk_dispatch_selects_override_per_race() {
        // 130：2 格喷吐命中（基版只 SetTargetXy，不攻击）。
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 130, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        let p = atk_player(&mut world, 10, 9, 10);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        assert!(monster_attack_target(&mut world, m));
        assert!(
            !world.pending_monster_ref_msgs.is_empty(),
            "130 喷吐发 RM_HIT"
        );

        // 117：邻接 ⇒ 自爆置 HP=0。
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 117, 10, 10);
        world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
        let p = atk_player(&mut world, 11, 10, 10);
        atk_see(&mut world, m, p);
        world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
        assert!(monster_attack_target(&mut world, m));
        assert_eq!(world.entities.get(m).unwrap().hp, 0);

        // 215：Master ⇒ 清目标且返回 false（基版会攻击）。
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = atk_monster(&mut world, 215, 10, 10);
        let p = atk_player(&mut world, 11, 10, 10);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.master = Some(p);
            e.ai.target_creature = Some(p);
            e.ai.attack_tick = 90_000;
        }
        assert!(!monster_attack_target(&mut world, m));
        assert_eq!(world.entities.get(m).unwrap().ai.target_creature, None);

        // 82：2 格喷吐；91：邻接魔法；114（默认 Appr=0）走基版。
        for race in [82u8, 91, 114] {
            let mut world = World::new(40, 40);
            world.clock_ms = 100_000;
            let m = atk_monster(&mut world, race, 10, 10);
            world.entities.get_mut(m).unwrap().ai.attack_tick = 90_000;
            let p = if race == 82 {
                atk_player(&mut world, 10, 8, 10)
            } else {
                atk_player(&mut world, 11, 10, 10)
            };
            world.entities.get_mut(m).unwrap().ai.target_creature = Some(p);
            assert!(monster_attack_target(&mut world, m), "race {race}");
            assert_eq!(world.pending_ai_attacks.len(), 1, "race {race} 结算");
        }
    }

    // ------------------------------ ctor 常量（117/215） ------------------------------

    /// `ExplosionSpider` ctor `ViewRange=5`/`SearchTick=0`；`MagicMonster` ctor `ViewRange=8`。
    #[test]
    fn atk_class_constants_for_117_and_215() {
        // `add_entity` 会把 `search_tick` 重盖成落图时刻（ctor 等价物拿不到时钟），
        // 故这里直接对构造等价物断言 ctor 显式值。
        let mut e = Entity::new(0, "x", 5, 5);
        e.ai.search_tick = 999;
        crate::mongen::apply_monster_class_constants(&mut e, 117, 12_345);
        assert_eq!(e.view_range, 5, "ExplosionSpider ctor ViewRange=5");
        assert_eq!(
            e.ai.search_tick, 0,
            "ExplosionSpider ctor 显式 SearchTick=0"
        );
        let mut e = Entity::new(0, "y", 8, 8);
        crate::mongen::apply_monster_class_constants(&mut e, 215, 12_345);
        assert_eq!(e.view_range, 8, "MagicMonster ctor ViewRange=8");
    }

    // ------------------------------------------------------------------
    // **MB-4**（R10）——`CowKingMonster`（race 92）构造级用例。
    // ------------------------------------------------------------------

    /// 造一只 race 92 的牛魔王（`jump_time` 由各用例按门设置）。
    fn mb4_cow(world: &mut World, x: i32, y: i32, hp: u16, max_hp: u16) -> i32 {
        let id = mb1_monster(world, 92, x, y);
        let e = world.entities.get_mut(id).unwrap();
        e.hp = hp;
        e.max_hp = max_hp;
        id
    }

    /// 把 map 0 的指定格置为墙（`Valid == false` ⇒ `CanWalk` 失败）。
    fn mb4_block(world: &mut World, cells: &[(i32, i32)]) {
        for &(x, y) in cells {
            let (cell, ok) = world.maps[0].get_cell_info_mut(x, y);
            assert!(ok, "({x},{y}) 应为合法格");
            cell.set_attribute(crate::map::CellAttribute::LowWall);
        }
    }

    /// 判据①：30s 门严格 `>`；进门先刷新 `JumpTime`；`Death`/`Ghost` 两支都跳过。
    ///
    /// 牙：删掉 30s 门（或把 `>` 改 `>=`）⇒ 本用例红。
    #[test]
    fn mb4_run_30s_gate_is_strict_greater_and_refreshes_jump_time() {
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        world.clock_ms = 100_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 70_000; // now - jump == 30_000：严格 > ⇒ 不得进门
            e.crazy_count = 5;
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.jump_time, 70_000, "恰好 30s 不刷新 JumpTime");
            assert_eq!(e.crazy_count, 5, "恰好 30s 不进 Crazy 段");
        }
        // 30_001 ⇒ 进门：刷新 JumpTime 并重算 CrazyCount（满血 ⇒ 0）。
        world.entities.get_mut(id).unwrap().jump_time = 69_999;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.jump_time, 100_000, "> 30s 刷新 JumpTime");
            assert_eq!(e.crazy_count, 0, "满血 ⇒ 7 - 700/(700/7) = 0");
        }
        // Death / Ghost：即使 30s 已过也整段跳过（只留 base.Run）。
        for ghost in [false, true] {
            let mut world = World::new(32, 32);
            let id = mb4_cow(&mut world, 10, 10, 700, 700);
            world.clock_ms = 100_000;
            {
                let e = world.entities.get_mut(id).unwrap();
                e.jump_time = 0;
                e.crazy_count = 5;
                if ghost {
                    e.ghost = true;
                } else {
                    e.death = true;
                }
            }
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.jump_time, 0, "Death/Ghost ⇒ 不刷新");
            assert_eq!(e.crazy_count, 5, "Death/Ghost ⇒ 不进 Crazy 段");
        }
    }

    /// 判据②：`SiegeLockCount()` 按 oracle **现算** 3×3 不可走格数（中心格不计）；
    /// 地形墙、越界格、活体（`!Ghost && !Death && !FixedHideMode && !ObMode`）都算，
    /// `Death`/`Ghost` 不算。
    #[test]
    fn mb4_siege_lock_count_is_3x3_terrain_and_actor_blockers_excluding_center() {
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        mb4_block(&mut world, &[(9, 9), (10, 9), (11, 9), (9, 10), (9, 11)]);
        assert_eq!(
            cow_king_siege_lock_count(&world, id),
            5,
            "3×3 地形墙计数（中心格不计）"
        );
        // **MB-4 F3（牙补）**：把牛魔王自己落图 ⇒ 中心格上有活体；若实现漏了
        // `(n_c != 0 || n10 != 0)`，计数会变 6 ⇒ 本断言红。
        assert!(world.enter_world(id), "牛魔王自身落图（中心格占位）");
        assert_eq!(
            cow_king_siege_lock_count(&world, id),
            5,
            "中心格不计（自身活体不得 +1）"
        );
        // 活体占位 +1；死亡/隐身不阻挡。
        let blocker = mb1_monster(&mut world, 80, 11, 10);
        assert!(world.enter_world(blocker), "活体落图");
        assert_eq!(
            cow_king_siege_lock_count(&world, id),
            6,
            "活体算阻挡（CanWalk(...,false)）"
        );
        world.entities.get_mut(blocker).unwrap().death = true;
        assert_eq!(cow_king_siege_lock_count(&world, id), 5, "Death ⇒ 不阻挡");
        world.entities.get_mut(blocker).unwrap().death = false;
        world.entities.get_mut(blocker).unwrap().ghost = true;
        assert_eq!(cow_king_siege_lock_count(&world, id), 5, "Ghost ⇒ 不阻挡");
        // 边界：越界格算不可走（(0,0) 的 5 个越界邻格）。
        let mut world = World::new(32, 32);
        let corner = mb4_cow(&mut world, 0, 0, 700, 700);
        assert_eq!(
            cow_king_siege_lock_count(&world, corner),
            5,
            "越界格算不可走"
        );
    }

    /// 判据③a：`TargetCret != null && SiegeLockCount() >= 5` 且目标身后格可走 ⇒
    /// `SpaceMove` 到目标身后格，**零抽**，并且 `return` 跳过 Crazy 段。
    ///
    /// 牙：把 `>= 5` 改成 `> 5`（恰好 5 格时不再进支）⇒ 本用例红。
    #[test]
    fn mb4_siege_ge5_spaces_to_target_back_and_skips_crazy() {
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        assert!(world.enter_world(id), "牛魔王落图");
        mb4_block(&mut world, &[(9, 9), (10, 9), (11, 9), (9, 10), (9, 11)]);
        // 目标 (12,10) 朝右 ⇒ `GetBackPosition` = (11,10)（可走）。
        let target = mb1_monster(&mut world, 80, 12, 10);
        world.entities.get_mut(target).unwrap().ai.dir = DIR_RIGHT;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.ai.target_creature = Some(target);
            e.crazy_count = 5;
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(1234);
        let mut probe = RandomNumber::with_seed(1234);
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::None),
            "Siege 支 return ⇒ 跳过 base.Run"
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!((e.x, e.y), (11, 10), "落点 = 目标身后格");
            assert_eq!(e.jump_time, 100_000, "进门先刷新 JumpTime");
            assert_eq!(e.crazy_count, 5, "Siege 支 return ⇒ 跳过 Crazy 段");
        }
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "落点已由 CanWalk 验证 ⇒ SpaceMoveGetRandXY 首轮成功 ⇒ 零抽"
        );
    }

    /// 判据③b：目标身后格不可走 ⇒ `MapRandomMove`（恰好 2 抽，落点可走时无移位环），
    /// 并且同样 `return` 跳过 Crazy 段。
    #[test]
    fn mb4_siege_failure_falls_back_to_map_random_move_with_two_draws() {
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        assert!(world.enter_world(id), "牛魔王落图");
        // 目标 (12,9) 朝右 ⇒ 身后 (11,9) 是墙 ⇒ 走 MapRandomMove。
        mb4_block(&mut world, &[(9, 9), (10, 9), (11, 9), (9, 10), (9, 11)]);
        let target = mb1_monster(&mut world, 80, 12, 9);
        world.entities.get_mut(target).unwrap().ai.dir = DIR_RIGHT;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.ai.target_creature = Some(target);
            e.crazy_count = 5;
        }
        world.clock_ms = 100_000;
        world.rng = RandomNumber::with_seed(777);
        let mut probe = RandomNumber::with_seed(777);
        // `MapRandomMove`（`BaseObject.cs:1214-1237`）：32×32 ⇒ nEgdey=20，
        // 两抽 `Random(32-20-1)=Random(11)`，落点 = 值 + 20。
        let want_x = probe.random_below(11) + 20;
        let want_y = probe.random_below(11) + 20;
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::None),
            "Siege 支 return ⇒ 跳过 base.Run"
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!((e.x, e.y), (want_x, want_y), "MapRandomMove 落点");
            assert_eq!(e.crazy_count, 5, "MapRandomMove 后 return ⇒ 跳过 Crazy 段");
        }
        assert_eq!(
            world.rng.random(),
            probe.random(),
            "恰两抽（落点可走 ⇒ 无移位环）"
        );
    }

    /// 判据④：`CrazyCount = 7 - HP / (MaxHP / 7)` 是 **C# `int/int` 两次整除**。
    ///
    /// 牙：把外层 `/` 改成浮点（`7 - (HP as f64 / (MaxHP as f64 / 7.0)) as i32`）⇒
    /// `(hp=99, max=100)` 用例红（整除 99/14=7 ⇒ 0；浮点 99/14.285=6.93 ⇒ 1）。
    #[test]
    fn mb4_crazy_count_uses_int_division_boundaries() {
        let run = |hp: u16, max_hp: u16, old: i32| {
            let mut world = World::new(32, 32);
            let id = mb4_cow(&mut world, 10, 10, hp, max_hp);
            world.clock_ms = 100_000;
            {
                let e = world.entities.get_mut(id).unwrap();
                e.jump_time = 0;
                e.crazy_count = old;
            }
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            world.entities.get(id).unwrap().crazy_count
        };
        assert_eq!(run(700, 700, 9), 0, "HP=MaxHP ⇒ 7-7=0");
        assert_eq!(run(100, 700, 9), 6, "HP=MaxHP/7 ⇒ 7-1=6");
        assert_eq!(run(699, 700, 9), 1, "699/100=6 ⇒ 1");
        assert_eq!(run(99, 100, 9), 0, "int/int：MaxHP/7=14，99/14=7 ⇒ 0");
        assert_eq!(run(6, 42, 9), 6, "42/7=6，6/6=1 ⇒ 6");
    }

    /// 判据⑤：`CrazyCount >= 2 && CrazyCount != oldCrazyCount` 才进 Ready；
    /// 同值不重进（`CrazyReady` 不被改写）；`<2` 不进。
    #[test]
    fn mb4_ready_requires_ge2_and_value_change() {
        // (old=0, new=6) ⇒ 进 Ready，写 Ready tick 并立刻 `NextHitTime=10000`。
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 100, 700);
        world.clock_ms = 100_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.crazy_count = 0;
            e.ai.next_hit_time = 2000;
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert!(e.crazy_ready_mode, "new=6 >= 2 且变化 ⇒ Ready");
            assert_eq!(e.crazy_ready, 100_000, "CrazyReady = now");
            assert_eq!(e.ai.next_hit_time, COW_KING_READY_HIT_TIME);
        }
        // (old=6, new=6) ⇒ 同值不重进：Ready tick 仍是首次的 100_000；
        // 此时已过 8s ⇒ 转 King 并写 500/400（同一调用）。
        world.clock_ms = 130_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 99_999; // 让 30s 门通过
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.crazy_ready, 100_000, "同值不得重写 CrazyReady");
            assert!(e.crazy_king_mode, "已过 8s ⇒ 转 King");
            assert_eq!(e.ai.next_hit_time, COW_KING_KING_HIT_TIME);
            assert_eq!(e.ai.walk_speed, COW_KING_KING_WALK_SPEED);
        }
        // (new=0 < 2) ⇒ 不进 Ready。
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        world.clock_ms = 100_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.crazy_count = 0;
            e.ai.next_hit_time = 2000;
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert!(!e.crazy_ready_mode, "0 < 2 ⇒ 不 Ready");
            assert_eq!(e.ai.next_hit_time, 2000, "不 Ready ⇒ 不改 NextHitTime");
        }
        // **MB-4 F2（牙补）**：`new == 2` 是 `>= 2` 的下边界，必须也进 Ready
        // （MAXHP=700 ⇒ 700/7=100；HP=500 ⇒ 7-500/100=2）。改成 `> 2` ⇒ 本段红。
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 500, 700);
        world.clock_ms = 100_000;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.crazy_count = 0;
            e.ai.next_hit_time = 2000;
        }
        assert_eq!(
            monster_class_run(&mut world, id),
            Some(ClassRunBase::Monster)
        );
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.crazy_count, 2, "7 - 500/(700/7) = 2");
            assert!(e.crazy_ready_mode, "new=2（下边界）⇒ Ready");
            assert_eq!(e.ai.next_hit_time, COW_KING_READY_HIT_TIME);
        }
    }

    /// **MB-4 F1（复验发现）**：`CowKingMonster : AtMonster`，`Run` 末尾 `base.Run()`
    /// （`CowKingMonster.cs:93`）= `ATMonster.Run`（`ATMonster.cs:13-24`）——同一道
    /// 8s/1s 索敌门。race 92 不在 `is_active_seek_race` 集合（它覆写了 `Run`），
    /// 故必须由 [`monster_run`] 显式补门。
    ///
    /// 牙：去掉 `|| e.race == 92` ⇒ 本用例红（`target_creature` 仍为 None）。
    #[test]
    fn mb4_cow_king_inherits_at_monster_run_seek_gate() {
        let mut world = World::new(60, 60);
        let cow = mb4_cow(&mut world, 10, 10, 700, 700);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(cow).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        // 段1：`> 8000` ⇒ 即使无目标也搜。
        world.clock_ms = 8001;
        world.entities.get_mut(cow).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, cow);
        assert_eq!(
            world.entities.get(cow).unwrap().ai.target_creature,
            Some(pid),
            "race 92 的 base.Run() = ATMonster.Run：>8000 必须索敌"
        );
        // 段2：`<= 1000` 且无目标 ⇒ 不搜（门是严格的，与其它 ATMonster 一致）。
        let mut world = World::new(60, 60);
        let cow = mb4_cow(&mut world, 10, 10, 700, 700);
        let pid = player(&mut world, 12, 10);
        world.entities.get_mut(cow).unwrap().visible_actors = vec![VisibleBaseObject {
            target: pid,
            flag: VisibleFlag::Show,
        }];
        world.clock_ms = 1000;
        world.entities.get_mut(cow).unwrap().ai.search_enemy_tick = 0;
        monster_run(&mut world, cow);
        assert_eq!(
            world.entities.get(cow).unwrap().ai.target_creature,
            None,
            "<=1000 不索敌"
        );
    }

    /// 判据⑥：Ready/King 两段 8s 边界都是严格 `<`；King 到期恢复
    /// `oldHitTime/oldWalkTime`（`Initialize()` 保存值）。
    ///
    /// 牙：去掉恢复赋值 ⇒ 到期用例红；把 `< 8000` 改 `<= 8000` ⇒ 边界用例红。
    #[test]
    fn mb4_ready_and_king_8s_boundaries_are_strict_and_restore_old_values() {
        let setup = |clock: i64, ready_ago: i64, king_ago: Option<i64>| {
            let mut world = World::new(32, 32);
            let id = mb4_cow(&mut world, 10, 10, 700, 700);
            world.clock_ms = clock;
            {
                let e = world.entities.get_mut(id).unwrap();
                e.jump_time = 0;
                e.old_hit_time = 2222;
                e.old_walk_time = 333;
                e.ai.next_hit_time = 2000;
                e.ai.walk_speed = 777;
                match king_ago {
                    None => {
                        e.crazy_ready_mode = true;
                        e.crazy_ready = clock - ready_ago;
                    }
                    Some(ago) => {
                        e.crazy_king_mode = true;
                        e.crazy_time = clock - ago;
                    }
                }
            }
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(ClassRunBase::Monster)
            );
            world.entities.get(id).unwrap().clone()
        };
        // Ready 7999ms：仍 Ready，NextHitTime=10000。
        let e = setup(100_000, 7_999, None);
        assert!(e.crazy_ready_mode, "7999 < 8000 ⇒ 仍 Ready");
        assert!(!e.crazy_king_mode);
        assert_eq!(e.ai.next_hit_time, COW_KING_READY_HIT_TIME);
        // Ready 8000ms：转 King 并在同一调用写 500/400。
        let e = setup(100_000, 8_000, None);
        assert!(!e.crazy_ready_mode, "8000 不再 < 8000 ⇒ 退出 Ready");
        assert!(e.crazy_king_mode, "进入 King");
        assert_eq!(e.crazy_time, 100_000, "CrazyTime = now");
        assert_eq!(e.ai.next_hit_time, COW_KING_KING_HIT_TIME);
        assert_eq!(e.ai.walk_speed, COW_KING_KING_WALK_SPEED);
        // King 7999ms：保持 500/400。
        let e = setup(100_000, 0, Some(7_999));
        assert!(e.crazy_king_mode, "7999 < 8000 ⇒ 仍 King");
        assert_eq!(e.ai.next_hit_time, COW_KING_KING_HIT_TIME);
        assert_eq!(e.ai.walk_speed, COW_KING_KING_WALK_SPEED);
        // King 8000ms：退出并恢复 Initialize 保存值。
        let e = setup(100_000, 0, Some(8_000));
        assert!(!e.crazy_king_mode, "8000 不再 < 8000 ⇒ 退出 King");
        assert_eq!(e.ai.next_hit_time, 2222, "恢复 oldHitTime");
        assert_eq!(e.ai.walk_speed, 333, "恢复 oldWalkTime");
    }

    /// 判据⑦：`Attack` 覆写的取数/帧面：
    /// `GetAttackPower(Lo, Abs(Hi-Lo))`（**Abs**，不是 sbyte 差）→
    /// `HitMagAttackTarget(target, nPower/2, nPower/2)`（两半各自整除）→
    /// 目标格每 proper 对象 `GetHitStruckDamage` + `GetMagStruckDamage` →
    /// 200ms `RM_REFMESSAGE`；尾帧 `RM_HIT` 且 `Dir = NextDirection`。
    ///
    /// 牙：把 `n_power / 2` 两处改成 `n_power` ⇒ 伤害断言红；
    /// 把 `(hi - lo).abs()` 改成 `(hi - lo) as i8 as i32` ⇒ 取数参数/伤害断言红。
    #[test]
    fn mb4_attack_uses_abs_dc_spread_and_half_physical_half_magic() {
        // lo=13、hi=5 ⇒ Abs 差 = 8（sbyte 差 = -8 ⇒ clamp 0）。
        // 选一个 `Random(9)` 抽到偶数且 >0 的种子：nPower = 13+d 为奇数，两半各截断。
        let seed = (0..100_000)
            .find(|s| {
                let d = RandomNumber::with_seed(*s).random_below(9);
                d >= 2 && d % 2 == 0
            })
            .expect("找不到偶数非零抽的种子");
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.dc = (5 << 8) | 13;
            e.ai.attack_tick = 0;
            e.ai.next_hit_time = 2000;
        }
        // 必须**落图**：`HitMagAttackTarget` 走 `Envir.GetBaseObjects(target.CurrX, target.CurrY)`。
        let target = mb1_real_player(&mut world, 11, 10); // race=Play、AC=MAC=0、HP=500
        world.entities.get_mut(id).unwrap().ai.target_creature = Some(target);
        world.clock_ms = 100_000;
        world.enable_refmessage_delay();
        world.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let draw = probe.random_below(9);
        let n_power = 13 + draw;
        assert!(n_power % 2 == 1, "构造：nPower 奇数");
        let want_damage = (n_power / 2) * 2;
        // 目标 AC=0 ⇒ `Random(1)`；MAC=0 ⇒ `Random(1)`；玩家耐久 ⇒ `Random(10)`。
        let _ = probe.random_below(1);
        let _ = probe.random_below(1);
        let _ = probe.random_below(10);
        let next_after = probe.random();

        assert!(monster_attack_target(&mut world, id), "邻接目标 ⇒ true");
        {
            let t = world.entities.get(target).unwrap();
            assert_eq!(
                t.hp,
                500 - u16::try_from(want_damage).unwrap(),
                "伤害 = nPower/2 + nPower/2（各自整除）"
            );
        }
        assert_eq!(
            world.entities.get(id).unwrap().ai.dir,
            crate::magic::next_direction(10, 10, 11, 10),
            "Dir = NextDirection(CurrX,CurrY,target.CurrX,CurrY)"
        );
        assert_eq!(
            world.rng.random(),
            next_after,
            "取数次序：AP(Random(9)) → AC 骰 → MAC 骰 → 玩家耐久 Random(10)"
        );
        let msg = world
            .self_delay_msgs
            .peek(target)
            .expect("RM_REFMESSAGE 入队");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, want_damage);
        assert_eq!(msg.n_param1, i32::from(500 - want_damage as u16), "HP 快照");
        assert_eq!(msg.n_param2, 500, "MaxHP 快照");
        assert_eq!(msg.n_param3, id, "ActorId");
        assert_eq!(msg.deliver_at_ms, 100_200, "200ms 延迟");
        let refs = world.pending_monster_ref_msgs.clone();
        assert_eq!(refs.len(), 1, "HitMagAttackTarget 尾帧一条 RM_HIT");
        assert_eq!(refs[0].ident, RM_HIT);
        assert_eq!(refs[0].actor_id, id);
        assert_eq!((refs[0].x, refs[0].y), (10, 10));
    }

    /// 判据③c：Siege 两支在 oracle 里 `return` ⇒ **跳过 `base.Run()`**
    /// （`BaseObject.Run` 的消息段不执行）。用 `pending_view_updates` 是否被排空钉住。
    #[test]
    fn mb4_siege_branches_skip_base_run_in_tick_driver() {
        let mut world = World::new(32, 32);
        let id = mb4_cow(&mut world, 10, 10, 700, 700);
        assert!(world.enter_world(id), "牛魔王落图");
        mb4_block(&mut world, &[(9, 9), (10, 9), (11, 9), (9, 10), (9, 11)]);
        let target = mb1_monster(&mut world, 80, 12, 10);
        world.entities.get_mut(target).unwrap().ai.dir = DIR_RIGHT;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.jump_time = 0;
            e.ai.target_creature = Some(target);
            // base.Run 的 `RM_UPDATEVIEWRANGE` 消息段若执行会 take 掉它。
            e.ai.pending_view_updates = vec![999];
        }
        world.clock_ms = 100_000;
        monster_ai_tick(&mut world);
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.x, e.y), (11, 10), "Siege 支仍执行 SpaceMove");
        assert_eq!(
            e.ai.pending_view_updates,
            vec![999],
            "oracle return ⇒ base.Run 的消息段不得执行"
        );
    }

    // ===================== MB-5：毒 / 隐身 / 瞬移（race 203/206/208/209/210） =====================
    //
    // 统一口径：固定 `clock_ms = 100_000`；把 `search_enemy_tick` 抬到 now 以关闭 8s/1s
    // 索敌门（`SearchTarget` 不抽数，但会改写目标态，非本段判据的噪声源）；目标用
    // `enter_world` 落格，使 `SpaceMove` 的摘格/登记真正发生。RNG 判据一律用「同种子
    // 探针按预期抽数序列复算，最后比较 `random()`」——少抽/多抽/换序都会使探针状态错位。

    fn find_seed(mut pred: impl FnMut(&mut RandomNumber) -> bool) -> i32 {
        for seed in 0..200_000 {
            let mut r = RandomNumber::with_seed(seed);
            if pred(&mut r) {
                return seed;
            }
        }
        unreachable!("20 万种子里应能找到目标取值");
    }

    fn set_race(world: &mut World, id: i32, race: u8) {
        world.entities.get_mut(id).unwrap().race = race;
    }

    fn set_anti_poison(world: &mut World, id: i32, anti_poison: u16) {
        world.set_attack_poison(
            id,
            crate::combat::AttackPoisonState {
                paralysis: false,
                un_paralysis: false,
                anti_poison,
            },
        );
    }

    fn quiet_seek(world: &mut World, id: i32) {
        world.entities.get_mut(id).unwrap().ai.search_enemy_tick = world.clock_ms;
    }

    /// R22：`Random(TargetCret.AntiPoison + 7) <= 6` 过门 ⇒ `MakePosion(DECHEALTH, 30, 1)`；
    /// 且 `TargetX/Y` 被写成目标当前格（`GreenMonster.cs:17-18`）。
    #[test]
    fn green_monster_poisons_adjacent_diagonal() {
        let seed = find_seed(|r| r.random_below(7) == 0);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 208);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 11, 11);
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_eq!(probe.random_below(7), 0);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(
            w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison),
            "两轴各 1 格且过门 ⇒ 绿毒"
        );
        let buff = &w.buffs.list(tid).unwrap()[0];
        assert_eq!(
            (buff.state_tick, buff.state_val),
            (30, 1),
            "MakePosion(DECHEALTH,30,1)"
        );
        let a = w.entities.get(mid).unwrap();
        assert_eq!(
            (a.ai.target_x, a.ai.target_y),
            (11, 11),
            "TargetX/Y 跟随目标当前格"
        );
        assert_eq!(w.rng.random(), probe.random(), "只抽一次 Random(7)");
    }

    /// R22 的**取数次序**（先抽后判）：`StatusTimeArr[DECHEALTH] != 0` 时仍消耗一次
    /// `Random(AntiPoison+7)`，只是不施毒（`GreenMonster.cs:23` 的 `&&`）。
    /// 改坏（先判状态再抽）⇒ 本用例的 `w.rng.random()` 比探针少一次 ⇒ 必红。
    #[test]
    fn green_monster_draws_before_dechealth_gate() {
        let seed = find_seed(|r| r.random_below(7) == 0);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 208);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 11, 11);
        assert!(w.enter_world(tid));
        w.entities.get_mut(tid).unwrap().status_time[crate::entity::POISON_DECHEALTH] = 9;
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let _ = probe.random_below(7);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(
            !w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison),
            "DECHEALTH 已存在 ⇒ 不施毒"
        );
        assert_eq!(
            w.rng.random(),
            probe.random(),
            "状态已存在时**仍抽数**（先抽 Random、再判 DECHEALTH）"
        );
    }

    /// R22：`Random` 的模数取**目标**的 `AntiPoison + 7`。目标 `AntiPoison=7` 且种子使
    /// `Random(14) > 6` ⇒ 不施毒；若误用 `Random(7)`（antipoison 恒 0）则必过门而施毒。
    #[test]
    fn green_monster_random_modulus_reads_target_anti_poison() {
        let seed = find_seed(|r| r.random_below(14) > 6);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 208);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 11, 11);
        assert!(w.enter_world(tid));
        set_anti_poison(&mut w, tid, 7);
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert!(probe.random_below(14) > 6);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(
            !w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison),
            "Random(AntiPoison+7) 的模数必须是 14，不是 7"
        );
        assert_eq!(w.rng.random(), probe.random(), "仍恰一次抽数");
    }

    /// R22 的邻接门：两轴必须**同时**为 1。`(1,0)` ⇒ 零抽、无副作用，但 `TargetX/Y` 仍刷新
    /// （`GreenMonster.cs:17-18` 在距离判定之前）。
    #[test]
    fn green_monster_diagonal_gate_requires_both_axes() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 208);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 11, 10);
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(42);
        let before = w.rng.random();
        w.rng = RandomNumber::with_seed(42);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(!w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison));
        let a = w.entities.get(mid).unwrap();
        assert_eq!(
            (a.ai.target_x, a.ai.target_y),
            (11, 10),
            "TargetX/Y 在门之前刷新"
        );
        assert_eq!(w.rng.random(), before, "非对角邻接 ⇒ 零抽");
    }

    /// R29：与 R22 逐句相同，唯一差异毒型 = `DAMAGEARMOR`（`RedMonster.cs:25`）。
    #[test]
    fn red_monster_poisons_with_damage_armor_type() {
        let seed = find_seed(|r| r.random_below(7) == 0);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 209);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 11, 11);
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let _ = probe.random_below(7);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(
            w.buffs.has_buff(tid, crate::buff::BuffType::RedPoison),
            "红毒 = DAMAGEARMOR"
        );
        assert!(!w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison));
        assert_eq!(w.rng.random(), probe.random());
    }

    /// R23 的 `Random(2)==0` 支：目标瞬移到 **Khazard 自己**的朝向前格，随后
    /// `Random(1)` + `Random(AntiPoison+7)` 两抽；过门 ⇒ `DECHEALTH,35,2` 并
    /// **`return`**（分派返回 `ClassRunBase::None` ⇒ 跳过 `base.Run()`）。
    #[test]
    fn khazard_branch_zero_teleports_target_then_poisons_and_returns() {
        let seed = find_seed(|r| r.random_below(2) == 0);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 206);
        w.entities.get_mut(mid).unwrap().ai.dir = DIR_RIGHT; // 前格 = (11,10)
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 12, 12);
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_eq!(probe.random_below(2), 0);
        let _ = probe.random_below(1);
        let _ = probe.random_below(7);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        let a = w.entities.get(mid).unwrap();
        assert_eq!(
            (a.ai.target_x, a.ai.target_y),
            (12, 12),
            "TargetX/Y 跟随目标当前格"
        );
        let t = w.entities.get(tid).unwrap();
        assert_eq!((t.x, t.y), (11, 10), "落点 = GetFrontPosition(Khazard)");
        assert!(
            w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison),
            "Khazard 的毒型是 DECHEALTH"
        );
        let buff = &w.buffs.list(tid).unwrap()[0];
        assert_eq!((buff.state_tick, buff.state_val), (35, 2));
        let frame = w
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_SPACEMOVE_FIRE)
            .expect("RM_SPACEMOVE_FIRE 帧");
        assert_eq!(
            frame.actor_id, tid,
            "发帧者是 TargetCret（TargetCret.SendRefMsg），不是 Khazard"
        );
        assert!(
            frame.recipients.contains(&tid),
            "oracle SendRefMsg 的格扫包含 sender 自己的格子（Race==Play）⇒ 目标玩家本人也收 RM_SPACEMOVE_FIRE"
        );
        assert_eq!(
            w.rng.random(),
            probe.random(),
            "抽数序：Random(2) → Random(1) → Random(AntiPoison+7)"
        );
    }

    /// R23 的 `Random(2)!=0` 支、目标 **HP > 半血**：`GetFrontPosition` 不被调用 ⇒
    /// `nX/nY` 保持 0，`SpaceMove(MapName, 0, 0, 0)` 落到 (0,0)（oracle 原样怪癖）。
    #[test]
    fn khazard_branch_one_above_half_hp_teleports_to_origin_corner() {
        let seed = find_seed(|r| r.random_below(2) == 1);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 206);
        w.entities.get_mut(mid).unwrap().ai.dir = DIR_RIGHT;
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 12, 12);
        w.entities.get_mut(tid).unwrap().hp = 100;
        w.entities.get_mut(tid).unwrap().max_hp = 100;
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        assert_eq!(probe.random_below(2), 1);
        let _ = probe.random_below(1);
        let _ = probe.random_below(7);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        let t = w.entities.get(tid).unwrap();
        assert_eq!(
            (t.x, t.y),
            (0, 0),
            "HP > MaxHP/2 ⇒ 不重算前格，SpaceMove 起点保持 (0,0)"
        );
        assert_eq!(w.rng.random(), probe.random(), "两支同序三抽");
    }

    /// R23 的 `Random(2)!=0` 支、目标 **HP <= 半血**：才调 `GetFrontPosition` ⇒ 落点 = 前格。
    #[test]
    fn khazard_branch_one_at_half_hp_uses_front_position() {
        let seed = find_seed(|r| r.random_below(2) == 1);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 206);
        w.entities.get_mut(mid).unwrap().ai.dir = DIR_RIGHT;
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 12, 12);
        w.entities.get_mut(tid).unwrap().hp = 50;
        w.entities.get_mut(tid).unwrap().max_hp = 100;
        assert!(w.enter_world(tid));
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        let t = w.entities.get(tid).unwrap();
        assert_eq!((t.x, t.y), (11, 10), "HP <= MaxHP/2 ⇒ 前格落点");
    }

    /// R23：毒门失败时 oracle **不 return** ⇒ 落回索敌门并继续 `base.Run()`
    /// （分派返回 `ClassRunBase::Monster`），但瞬移与两抽都已发生。
    #[test]
    fn khazard_failed_poison_gate_falls_through_to_base_run() {
        let seed = find_seed(|r| {
            r.random_below(2) == 0 && {
                let _ = r.random_below(1);
                r.random_below(14) > 6
            }
        });
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 206);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 12, 12);
        assert!(w.enter_world(tid));
        set_anti_poison(&mut w, tid, 7);
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, mid);
        w.rng = RandomNumber::with_seed(seed);

        assert_eq!(
            monster_class_run(&mut w, mid),
            Some(ClassRunBase::Monster),
            "毒门失败 ⇒ 不跳过 base.Run()"
        );
        assert!(!w.buffs.has_buff(tid, crate::buff::BuffType::GreenPoison));
        let t = w.entities.get(tid).unwrap();
        assert_eq!((t.x, t.y), (10, 9), "dir=Up 的前格 = (10,9)");
    }

    /// R20：无目标且透明计时器为 0 ⇒ `MagMakePrivateTransparent(this, 180)`；
    /// 邻格有怪以它为靶 ⇒ helper 内 `Random(2)==0` 清靶（一次抽数）。
    #[test]
    fn frost_tiger_becomes_transparent_and_clears_nearby_targets() {
        let seed = find_seed(|r| r.random_below(2) == 0);
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let tiger = monster(&mut w, 10, 10);
        set_race(&mut w, tiger, 210);
        assert!(w.enter_world(tiger));
        let viewer = monster(&mut w, 11, 10);
        assert!(w.enter_world(viewer));
        w.entities.get_mut(viewer).unwrap().ai.target_creature = Some(tiger);
        // 玩家观察者：helper 的 `Random(2)` 只对 `Race >= Animal` 抽，玩家不改变抽数序；
        // 用于钉住 `StatusChanged()` 的收件人集合（oracle 格扫仅 `Race == Play`）。
        let observer = player(&mut w, 12, 10);
        assert!(w.enter_world(observer));
        quiet_seek(&mut w, tiger);
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let _ = probe.random_below(2);

        assert_eq!(
            monster_class_run(&mut w, tiger),
            Some(ClassRunBase::Monster)
        );
        let e = w.entities.get(tiger).unwrap();
        assert_eq!(
            e.status_time[crate::entity::POISON_STATETRANSPARENT],
            180,
            "MagMakePrivateTransparent(this, 180)"
        );
        assert!(e.hide_mode && e.transparent);
        assert_eq!(
            w.entities.get(viewer).unwrap().ai.target_creature,
            None,
            "邻格 Random(2)==0 ⇒ 清靶"
        );
        // **牙**：helper 返回的投递结果必须排水（`StatusChanged()` ⇒ RM_CHARSTATUSCHANGED）。
        // 去掉 `frost_tiger_run` 里的 `send_ref_msg(...)` ⇒ 本断言红。
        let frame = w
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_CHARSTATUSCHANGED)
            .expect("MagMakePrivateTransparent 的 StatusChanged ⇒ RM_CHARSTATUSCHANGED 必须入队");
        assert_eq!(frame.actor_id, tiger, "StatusChanged 由 FrostTiger 自己发");
        assert_eq!(
            frame.w_param, 0,
            "SendRefMsg(RM_CHARSTATUSCHANGED, 0, CharStatus, …)"
        );
        assert_eq!(
            frame.n_param1, e.char_status,
            "nParam1 = 收尾后的 CharStatus"
        );
        assert_eq!(
            frame.recipients,
            vec![observer],
            "oracle 格扫只发 Race==Play（含 sender 自己；tiger/邻怪不发）"
        );
        assert_eq!(w.rng.random(), probe.random(), "helper 内恰一次 Random(2)");
    }

    /// R20：无目标但透明计时器 > 0 ⇒ **不调用** helper（零抽、计时器不变）。
    #[test]
    fn frost_tiger_skips_transparent_helper_while_timer_active() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let tiger = monster(&mut w, 10, 10);
        set_race(&mut w, tiger, 210);
        w.entities.get_mut(tiger).unwrap().status_time[crate::entity::POISON_STATETRANSPARENT] = 50;
        assert!(w.enter_world(tiger));
        quiet_seek(&mut w, tiger);
        w.rng = RandomNumber::with_seed(3);
        let before = w.rng.random();
        w.rng = RandomNumber::with_seed(3);

        assert_eq!(
            monster_class_run(&mut w, tiger),
            Some(ClassRunBase::Monster)
        );
        let e = w.entities.get(tiger).unwrap();
        assert_eq!(e.status_time[crate::entity::POISON_STATETRANSPARENT], 50);
        assert!(!e.hide_mode);
        assert_eq!(w.rng.random(), before, "计时器 > 0 ⇒ 零抽");
    }

    /// R20：有目标 ⇒ 只把 `StatusTimeArr[STATETRANSPARENT]` 置 0；oracle **不**在此清
    /// `HideMode/Transparent`（由 `RecalcAbilitys` 收口）⇒ 本测试把该怪癖钉死。
    #[test]
    fn frost_tiger_with_target_resets_timer_only() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let tiger = monster(&mut w, 10, 10);
        set_race(&mut w, tiger, 210);
        {
            let e = w.entities.get_mut(tiger).unwrap();
            e.status_time[crate::entity::POISON_STATETRANSPARENT] = 180;
            e.hide_mode = true;
            e.transparent = true;
        }
        assert!(w.enter_world(tiger));
        let tid = player(&mut w, 12, 10);
        assert!(w.enter_world(tid));
        w.entities.get_mut(tiger).unwrap().ai.target_creature = Some(tid);
        quiet_seek(&mut w, tiger);
        w.rng = RandomNumber::with_seed(3);
        let before = w.rng.random();
        w.rng = RandomNumber::with_seed(3);

        assert_eq!(
            monster_class_run(&mut w, tiger),
            Some(ClassRunBase::Monster)
        );
        let e = w.entities.get(tiger).unwrap();
        assert_eq!(e.status_time[crate::entity::POISON_STATETRANSPARENT], 0);
        assert!(
            e.hide_mode && e.transparent,
            "oracle 不在此清 HideMode/Transparent"
        );
        assert_eq!(w.rng.random(), before, "有目标支零抽");
    }

    /// R40：条件读 `TargetX/TargetY` 字段（不是 `TargetCret.CurrX/CurrY`）。
    /// 目标实际在 (20,20)、但字段 `TargetX/Y = 10/10` ⇒ **不瞬移**。
    #[test]
    fn tele_monster_compares_target_fields_not_target_current_cell() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 203);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 20, 20);
        assert!(w.enter_world(tid));
        {
            let a = &mut w.entities.get_mut(mid).unwrap().ai;
            a.target_creature = Some(tid);
            a.target_x = 10;
            a.target_y = 10;
        }
        quiet_seek(&mut w, mid);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        let e = w.entities.get(mid).unwrap();
        assert_eq!(
            (e.x, e.y),
            (10, 10),
            "字段距离 <= 5 ⇒ 不瞬移（目标实际距 10 格）"
        );
        assert!(!w
            .pending_monster_ref_msgs
            .iter()
            .any(|m| m.ident == RM_SPACEMOVE_FIRE));
    }

    /// R40：字段错位但触发 ⇒ 落点是 `TargetCret.MapName/CurrX/CurrY`（目标**当前**格），
    /// 不是 `TargetX/Y`；发帧者 = TeleMonster 自己。
    #[test]
    fn tele_monster_teleports_to_target_current_cell_on_field_mismatch() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 203);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 13, 10);
        assert!(w.enter_world(tid));
        {
            let a = &mut w.entities.get_mut(mid).unwrap().ai;
            a.target_creature = Some(tid);
            a.target_x = 0;
            a.target_y = 0;
        }
        quiet_seek(&mut w, mid);

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        // 本函数自身瞬移到目标当前格；落点可走 ⇒ SpaceMove 零抽。
        let e = w.entities.get(mid).unwrap();
        assert_eq!((e.x, e.y), (13, 10), "SpaceMove(TargetCret.CurrX/CurrY)");
        let frame = w
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_SPACEMOVE_FIRE)
            .expect("RM_SPACEMOVE_FIRE 帧");
        assert_eq!(
            frame.actor_id, mid,
            "this.SendRefMsg ⇒ 发帧者 = TeleMonster"
        );
    }

    /// 五类共用的 8s/1s 索敌门：>1000ms 且无目标 ⇒ `SearchTarget` 并刷新 `SearchEnemyTick`。
    #[test]
    fn mb5_class_run_shares_the_8s_1s_seek_gate() {
        let mut w = World::new(60, 60);
        w.clock_ms = 100_000;
        let mid = monster(&mut w, 10, 10);
        set_race(&mut w, mid, 203);
        assert!(w.enter_world(mid));
        let tid = player(&mut w, 12, 10);
        assert!(w.enter_world(tid));
        {
            let e = w.entities.get_mut(mid).unwrap();
            e.ai.search_enemy_tick = 100_000 - 1001;
            e.visible_actors = vec![crate::entity::VisibleBaseObject {
                target: tid,
                flag: crate::entity::VisibleFlag::Show,
            }];
        }

        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        let e = w.entities.get(mid).unwrap();
        assert_eq!(
            e.ai.search_enemy_tick, 100_000,
            "过门必刷新 SearchEnemyTick"
        );
        assert_eq!(e.ai.target_creature, Some(tid), "SearchTarget 选中玩家");
    }

    // ------------------------------------------------------------------
    // **MB-6**（R14/R25/R31）——构造级用例。
    // ------------------------------------------------------------------

    /// 找一个「先抽 `Random(power+1)`、再抽 `RandomByte(sp)` 指定是否非 0」的种子。
    fn mb6_seed_power_then_byte(power: i32, sp: u8, want_nonzero: bool) -> i32 {
        (0..200_000)
            .find(|seed| {
                let mut r = RandomNumber::with_seed(*seed);
                let _ = r.random_below(power + 1);
                (r.random_byte(sp) != 0) == want_nonzero
            })
            .expect("seed")
    }

    /// R14①：`CanMove()` 门内 `TargetCret == null` ⇒ oracle `return`（跳过 `base.Run()`）
    /// ⇒ [`ClassRunBase::None`]；`return` 的位置在 `SearchTarget()` **之后**（照抄）——
    /// 索敌认领到目标时**不得**再早退。
    ///
    /// 牙：把 `return true` 挪到索敌之前（或少写 `Some(ClassRunBase::None)`）⇒ 段 1 红；
    /// 把 `return true` 删掉 ⇒ 段 1/2 红。
    #[test]
    fn mb6_r14_no_target_returns_none_but_after_search() {
        // 段 1：无可视目标、>1000ms 门 ⇒ SearchTarget 空手 ⇒ None。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10);
        w.clock_ms = 100_000;
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000 - 1001;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.search_enemy_tick,
            100_000,
            ">1000ms 门刷新 SearchEnemyTick"
        );
        // 段 2：门未到（`> 1000` 严格）⇒ 不索敌、仍 None。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10);
        w.clock_ms = 100_000;
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000 - 1000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.search_enemy_tick,
            100_000 - 1000,
            "`> 1000` 严格 ⇒ 恰 1000ms 不刷新"
        );
        // 段 3：索敌认领到目标 ⇒ `TargetCret != null` ⇒ **不早退**（None→Monster）。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10);
        let p = mb1_players(&mut w, 20, 20); // 距离 >2 ⇒ 不攻击
        mb1_see(&mut w, mid, p);
        w.clock_ms = 100_000;
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000 - 1001;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.target_creature,
            Some(p),
            "SearchTarget 认领玩家"
        );
    }

    /// R14②：`UseMagic = HP < MaxHP / 2`（严格 `<`、`ushort` 整数除法）；攻击门 =
    /// 两轴 `<= 2` 且 (`UseMagic || nX == 2 || nY == 2`) 且 `(now - AttackTick) > NextHitTime`。
    /// `LightingAttack` 取数序 = `Random(|Hi(DC)-Lo(DC)|+1)`（`:18`）→ MAC `Random(|Hi(MAC)-Lo(MAC)|+1)`
    /// （`GetMagStruckDamage`）；吸血 = `nDamage / LoByte(MP)`（`ushort` 回绕）；尾帧 `RM_LIGHTING`
    /// **无条件**；`RM_REFMESSAGE` 200ms。
    ///
    /// 牙：`<` 改成 `<=` ⇒ 段 1 红；删 `nX == 2 || nY == 2` ⇒ 段 3 红；把吸血放到 `StruckDamage`
    /// 之后 ⇒ 吸血值不变（该值只依赖 nDamage/MP）但把 `btGetBackHp != 0` 门删掉会除零 panic ⇒ 段 3 红。
    #[test]
    fn mb6_r14_use_magic_threshold_and_lighting_attack_rng() {
        // 段 1：HP == MaxHP/2 ⇒ UseMagic=false；HP == MaxHP/2 - 1 ⇒ true。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10); // MaxHP = 200
        w.clock_ms = 100_000;
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().hp = 100;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        assert!(
            !w.entities.get(mid).unwrap().use_magic,
            "HP == Max/2 ⇒ 不用魔法"
        );
        w.entities.get_mut(mid).unwrap().hp = 99;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        assert!(
            w.entities.get(mid).unwrap().use_magic,
            "HP < Max/2 ⇒ 用魔法"
        );

        // 段 2：UseMagic=false 且两轴 (1,0) ⇒ 攻击门不成立 ⇒ 零抽、零帧。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 100; // UseMagic=false
        let t = monster(&mut w, 11, 10);
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        w.rng = RandomNumber::with_seed(424242);
        let mut probe = RandomNumber::with_seed(424242);
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(w.rng.random(), probe.random(), "两轴 (1,0) ⇒ 零抽");
        assert_eq!(
            w.entities.get(mid).unwrap().ai.attack_tick,
            0,
            "不刷新 AttackTick"
        );
        assert!(w.pending_monster_ref_msgs.is_empty(), "不产帧");
        assert!(w.pending_ai_attacks.is_empty(), "不结算");

        // 段 3：nX == 2（UseMagic=false）⇒ 攻击；抽数序与数值逐项钉住。
        let mut w = World::new(32, 32);
        w.enable_refmessage_delay();
        let mid = mb1_monster(&mut w, 200, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 100; // UseMagic=false
        w.entities.get_mut(mid).unwrap().dc = (9 << 8) | 5; // Lo=5, Hi=9 ⇒ Random(5)
        w.entities.get_mut(mid).unwrap().mp = 2; // LoByte(MP)=2
        let t = monster(&mut w, 12, 10);
        w.entities.get_mut(t).unwrap().hp = 500;
        w.entities.get_mut(t).unwrap().max_hp = 500;
        w.entities.get_mut(t).unwrap().mac = 0; // Random(|0-0|+1)
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        let seed = 20261020;
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let draw = probe.random_below(5);
        let _mac = probe.random_below(1);
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.rng.random(),
            probe.random(),
            "Random(5) → MAC Random(1)（恰好两次）"
        );
        let n_power = 5 + draw;
        assert_eq!(
            w.entities.get(t).unwrap().hp,
            500 - n_power as u16,
            "GetMagStruckDamage(MAC=0) ⇒ nDamage = nPower"
        );
        assert_eq!(
            w.entities.get(mid).unwrap().hp,
            100 + (n_power / 2) as u16,
            "吸血 = nDamage / LoByte(MP=2)"
        );
        let lit = w
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_LIGHTING)
            .expect("RM_LIGHTING 帧");
        assert_eq!(lit.w_param, 1);
        assert_eq!((lit.n_param1, lit.n_param2), (10, 10), "CurrX/CurrY");
        assert_eq!(lit.n_param3, t);
        assert_eq!((lit.target_x, lit.target_y), (12, 10), "尾 = 目标坐标快照");
        assert_eq!(lit.dir, DIR_RIGHT);
        assert_eq!(w.pending_ai_attacks.len(), 1);
        assert_eq!(w.pending_ai_attacks[0].2.power, n_power);
        assert_eq!(w.pending_ai_attacks[0].2.struck_power, n_power as u16);
        let msg = w
            .self_delay_msgs
            .peek(t)
            .expect("200ms RM_REFMESSAGE 入队（受击者队列）");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, n_power, "wParam = nDamage");
        assert_eq!(msg.deliver_at_ms, 100_200, "200ms");
        assert_eq!(msg.n_param3, mid, "nParam3 = 攻击者 ActorId");

        // 段 4：UseMagic=true（HP < Max/2）且两轴 (1,1) ⇒ 走魔法（或门左支）。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 200, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 99; // UseMagic=true
        w.entities.get_mut(mid).unwrap().dc = 0;
        let t = monster(&mut w, 11, 11);
        w.entities.get_mut(t).unwrap().hp = 500;
        w.entities.get_mut(t).unwrap().max_hp = 500;
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert!(
            w.pending_monster_ref_msgs
                .iter()
                .any(|m| m.ident == RM_LIGHTING),
            "UseMagic 支攻击"
        );
        assert_eq!(w.entities.get(mid).unwrap().ai.attack_tick, 100_000);
    }

    /// R25：`CanMove()` 门内 `Master == null` ⇒ `return`（跳过 `base.Run()`）⇒ `None`；
    /// **索敌先于 `Master` 门**（照抄 `:35-43`）。有 Master 时：距离窗 `<= 5`、或门 `nX == 5 ||
    /// nY == 5`、到点只刷新 `AttackTick`——`LightingAttack` 是**空函数**（`:18-21`）⇒
    /// 零抽、零帧、零结算。
    ///
    /// 牙：把 `Master == null` 的早退删掉 ⇒ 段 1 红；把距离窗 `5` 改 `4` ⇒ 段 3 红。
    #[test]
    fn mb6_r25_master_gate_skips_base_and_empty_lighting_attack() {
        // 段 1：无 Master；>1000ms 门索敌认领玩家后仍返回 None。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 133, 10, 10);
        let p = mb1_players(&mut w, 20, 20);
        mb1_see(&mut w, mid, p);
        w.clock_ms = 100_000;
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000 - 1001;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::None));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.target_creature,
            Some(p),
            "索敌在 Master 门之前（照抄 oracle 次序）"
        );

        // 段 2：Master 存在、两轴 (5,0)、UseMagic=false ⇒ 到点 ⇒ 只刷新 AttackTick。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 133, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 100; // UseMagic=false（Max=200）
        let master = monster(&mut w, 15, 10);
        w.entities.get_mut(mid).unwrap().master = Some(master);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        w.rng = RandomNumber::with_seed(7777);
        let mut probe = RandomNumber::with_seed(7777);
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.attack_tick,
            100_000,
            "nX == 5 或门 ⇒ 刷新 AttackTick"
        );
        assert_eq!(w.rng.random(), probe.random(), "空 LightingAttack 零抽");
        assert!(w.pending_monster_ref_msgs.is_empty(), "零帧");
        assert!(w.pending_ai_attacks.is_empty(), "零结算");
        assert_eq!(w.entities.get(mid).unwrap().ai.dir, 0, "空函数不写 Dir");

        // 段 3：Master 两轴 (4,0) 且 UseMagic=false ⇒ 门不成立 ⇒ AttackTick 不变。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 133, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 100;
        let master = monster(&mut w, 14, 10);
        w.entities.get_mut(mid).unwrap().master = Some(master);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(mid).unwrap().ai.attack_tick,
            0,
            "nX == 4 不触发"
        );

        // 段 4：UseMagic=true 且两轴 (1,0) ⇒ 触发（只刷新 AttackTick）。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 133, 10, 10);
        w.entities.get_mut(mid).unwrap().hp = 99;
        let master = monster(&mut w, 11, 10);
        w.entities.get_mut(mid).unwrap().master = Some(master);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(w.entities.get(mid).unwrap().ai.attack_tick, 100_000);
    }

    /// R31①：`GetMapBaseObjects(envir, x, y, 1)` = 3×3、**x 外层升序 / y 内层升序**、
    /// 逐格按 `ObjList` 序、含自身；`AroundAttack` 倒序逐个 `_Attack`；攻击力
    /// `GetBaseAttackPoewr()` **只抽一次**（全目标共用）；`RM_HIT` 尾帧无条件。
    ///
    /// 牙：把 `into_iter().rev()` 改成正序 ⇒ 目标顺序断言红；把攻击力抽数挪进循环 ⇒ 抽数账本红；
    /// 把 3×3 改成 2×2 ⇒ 对角目标不在列表 ⇒ 顺序/hp 断言红。
    #[test]
    fn mb6_r31_around_attack_order_single_power_draw_and_self_struck() {
        let mut w = World::new(32, 32);
        w.enable_refmessage_delay();
        let mid = mb1_monster(&mut w, 131, 10, 10);
        w.entities.get_mut(mid).unwrap().dc = (9 << 8) | 5; // Lo=5, Hi=9 ⇒ Random(5)
        assert!(w.enter_world(mid));
        let p1 = mb1_players(&mut w, 9, 9); // 对角：枚举序第 1
        let p2 = mb1_players(&mut w, 11, 11); // 对角：枚举序最后
        for t in [p1, p2] {
            w.entities.get_mut(t).unwrap().hp = 500;
            w.entities.get_mut(t).unwrap().max_hp = 500;
            assert!(w.enter_world(t));
        }
        {
            let e = w.entities.get_mut(mid).unwrap();
            e.ai.target_creature = Some(p1); // 邻接 ⇒ attDir = UP_LEFT
            e.ai.search_enemy_tick = 100_000;
            e.ai.attack_tick = 0;
        }
        w.clock_ms = 100_000;
        let seed = 31337;
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let draw = probe.random_below(5); // GetBaseAttackPoewr：一次
        for _ in 0..2 {
            // p1/p2 各一次 `GetHitStruckDamage`（AC=0 ⇒ `Random(1)`）+ 一次
            // `PlayObject.StruckDamage` 的耐久骰 `Random(10)`；自身 improper ⇒ 不抽。
            let _ = probe.random_below(1);
            let _ = probe.random_below(10);
        }
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.rng.random(),
            probe.random(),
            "1 次攻击力 + 2×(护甲 1 + 玩家耐久 1)；自身 improper ⇒ 不抽"
        );
        let n_power = 5 + draw;
        // 倒序：p2（x=11）→ 自身（x=10）→ p1（x=9）。
        let order: Vec<i32> = w.pending_ai_attacks.iter().map(|(_, t, _)| *t).collect();
        assert_eq!(order, vec![p2, mid, p1], "倒序逐个 _Attack");
        for (_, tid, outcome) in &w.pending_ai_attacks {
            assert_eq!(outcome.power, n_power, "攻击力全目标共用");
            if *tid == mid {
                assert!(!outcome.hit, "自身 ⇒ IsProperTarget false ⇒ 归零");
                assert_eq!(outcome.damage, 0);
            } else {
                assert!(outcome.hit);
                assert_eq!(outcome.damage, n_power as u16);
                assert_eq!(
                    w.entities.get(*tid).unwrap().hp,
                    500 - n_power as u16,
                    "掉血 = nPower（AC=0、倍率 10）"
                );
                assert_eq!(outcome.struck_power, n_power as u16);
                let msg = w.self_delay_msgs.peek(*tid).expect("200ms RM_REFMESSAGE");
                assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
                assert_eq!(msg.w_param, n_power);
                assert_eq!(msg.deliver_at_ms, 100_200);
            }
        }
        // 自身：`Race > Play` ⇒ `SendMsg(self, RM_STRUCK, 0)` ⇒ 折叠出 LastHiter=self + 队列项。
        {
            let e = w.entities.get(mid).unwrap();
            assert_eq!(e.last_hiter, Some(mid), "自身 RM_STRUCK 折叠");
            assert_eq!(
                e.ai.pending_struck,
                vec![crate::entity::StruckMsg {
                    hiter: mid,
                    w_param: 0,
                }]
            );
        }
        // 玩家目标 `Race == Play` ⇒ **不**走 `SendMsg(RM_STRUCK)`（oracle `:61` 的 `> Play` 门）。
        for t in [p1, p2] {
            assert!(w.entities.get(t).unwrap().ai.pending_struck.is_empty());
        }
        let hit = w
            .pending_monster_ref_msgs
            .iter()
            .find(|m| m.ident == RM_HIT)
            .expect("RM_HIT 尾帧");
        assert_eq!(hit.dir, DIR_UP_LEFT, "GetAttackDir(TargetCret)");
        assert_eq!((hit.n_param1, hit.n_param2), (10, 10));
    }

    /// R31②：`Run` 的两轴门是**严格 `< 6`**；`|Δx|=5` 触发（列表只有自身，攻击力仍抽一次）、
    /// `|Δx|=6` 不触发（零抽、零帧）。
    /// 牙：`< 6` 改 `<= 6` ⇒ Δx=6 段红；去掉 `<` 的 y 轴门 ⇒ Δy=6 段红。
    #[test]
    fn mb6_r31_run_gate_is_strict_six() {
        // 世界不落图 ⇒ `GetMapBaseObjects` 为空：`AroundAttack` 仍发 `RM_HIT`（无条件），
        // 但**不抽攻击力**（`objectList.Count > 0` 门）。这样正好把 Run 门与抽取门分开。
        let run = |dx: i32, dy: i32| -> (usize, usize, bool) {
            let mut w = World::new(40, 40);
            let mid = mb1_monster(&mut w, 131, 10, 10);
            w.entities.get_mut(mid).unwrap().dc = 0; // Random(1) 恒 0
            let t = monster(&mut w, 10 + dx, 10 + dy);
            w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
            w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
            w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
            w.clock_ms = 100_000;
            w.rng = RandomNumber::with_seed(4242);
            let mut probe = RandomNumber::with_seed(4242);
            assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
            (
                w.pending_ai_attacks.len(),
                w.pending_monster_ref_msgs
                    .iter()
                    .filter(|m| m.ident == RM_HIT)
                    .count(),
                w.rng.random() == probe.random(),
            )
        };
        assert_eq!(run(5, 0), (0, 1, true), "|Δx|=5 触发（空列表 ⇒ 零抽）");
        assert_eq!(run(0, 5), (0, 1, true), "|Δy|=5 触发");
        assert_eq!(run(6, 0), (0, 0, true), "|Δx|=6 不触发");
        assert_eq!(run(0, 6), (0, 0, true), "|Δy|=6 不触发");
    }

    /// R31③：`_Attack` 的 `RandomByte(target.SpeedPoint)` **只在 `target.HitPoint > 0` 时抽**，
    /// 且 `HitPoint < roll` ⇒ `nPower = 0`（不再走护甲/伤害/延时）。
    /// 牙：把 `target.hit_point > 0` 去掉 ⇒ `HitPoint=0` 的既有段多抽一次、账本红；
    /// 把 `<` 改成 `<=` ⇒ roll==HitPoint 的边界（本段用 HitPoint=0）红。
    #[test]
    fn mb6_r31_speed_point_draw_only_when_hit_point_positive() {
        // 命中点 0 ⇒ 不抽 RandomByte(SpeedPoint)；抽数 = 攻击力 1 + 护甲 1。
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 131, 10, 10);
        w.entities.get_mut(mid).unwrap().dc = (9 << 8) | 5;
        assert!(w.enter_world(mid));
        let t = mb1_players(&mut w, 11, 10); // Race == Play ⇒ proper
        assert!(w.enter_world(t));
        w.entities.get_mut(t).unwrap().hit_point = 0;
        w.entities.get_mut(t).unwrap().speed_point = 10;
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        let seed = 123;
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let _power = probe.random_below(5);
        let _armor = probe.random_below(1);
        let _durability = probe.random_below(10); // PlayObject.StruckDamage 的耐久骰
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(
            w.rng.random(),
            probe.random(),
            "HitPoint=0 ⇒ 不抽 RandomByte(SpeedPoint)（否则多一抽 ⇒ 红）"
        );

        // 命中点 10、SpeedPoint 10、roll != 0 ⇒ nPower 归零、不抽护甲。
        let seed = mb6_seed_power_then_byte(5, 10, true);
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 131, 10, 10);
        w.entities.get_mut(mid).unwrap().dc = (9 << 8) | 5;
        // `HitPoint < RandomByte(SpeedPoint)`：攻方 HitPoint=0、roll != 0 ⇒ 闪避。
        assert!(w.enter_world(mid));
        let t = mb1_players(&mut w, 11, 10); // Race == Play ⇒ proper
        assert!(w.enter_world(t));
        w.entities.get_mut(t).unwrap().hit_point = 10;
        w.entities.get_mut(t).unwrap().speed_point = 10;
        w.entities.get_mut(t).unwrap().hp = 500;
        w.entities.get_mut(t).unwrap().max_hp = 500;
        w.entities.get_mut(mid).unwrap().ai.target_creature = Some(t);
        w.entities.get_mut(mid).unwrap().ai.search_enemy_tick = 100_000;
        w.entities.get_mut(mid).unwrap().ai.attack_tick = 0;
        w.clock_ms = 100_000;
        w.rng = RandomNumber::with_seed(seed);
        let mut probe = RandomNumber::with_seed(seed);
        let _power = probe.random_below(5);
        let _roll = probe.random_byte(10);
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        assert_eq!(w.rng.random(), probe.random(), "roll != 0 ⇒ 归零、无护甲抽");
        assert_eq!(w.entities.get(t).unwrap().hp, 500, "闪避 → 零伤害");
        let outcome = w
            .pending_ai_attacks
            .iter()
            .find(|(_, tid, _)| *tid == t)
            .map(|(_, _, o)| *o)
            .expect("仍有结算记录（oracle `_Attack` 照跑）");
        assert!(!outcome.hit, "闪避 ⇒ hit=false（`nPower` 归零支）");
        assert_eq!(outcome.damage, 0);
        assert_eq!(outcome.struck_power, 0);
        assert!(
            w.self_delay_msgs.peek(t).is_none(),
            "零伤害不发 RM_REFMESSAGE"
        );
    }

    /// R31①：`GetMapBaseObjects` 的 `!Death && !Ghost` 过滤（`BaseObject.cs:2830`）。
    ///
    /// 同格表里放「活怪 / 已死怪 / Ghost」：只有活怪进列表（死/幽灵对象仍留在 `ObjList`，
    /// 过滤发生在 `!baseObject.Death && !baseObject.Ghost`，不是落格期）。
    ///
    /// 牙：删掉 `!e.death && !e.ghost` ⇒ 死/幽灵也进 `pending_ai_attacks` ⇒ 序列断言红。
    #[test]
    fn mb6_r31_map_base_objects_filters_death_and_ghost() {
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 131, 10, 10);
        assert!(w.enter_world(mid));
        let alive = mb1_monster(&mut w, ACTOR_RACE_MONSTER, 9, 10);
        assert!(w.enter_world(alive));
        let dead = mb1_monster(&mut w, ACTOR_RACE_MONSTER, 10, 11);
        assert!(w.enter_world(dead));
        let ghost = mb1_monster(&mut w, ACTOR_RACE_MONSTER, 11, 10);
        assert!(w.enter_world(ghost));
        // 落图后再置位：对象仍在 `ObjList` 里（oracle 的过滤点在枚举期）。
        w.entities.get_mut(dead).unwrap().death = true;
        w.entities.get_mut(ghost).unwrap().ghost = true;
        {
            let e = w.entities.get_mut(mid).unwrap();
            e.ai.target_creature = Some(alive);
            e.ai.search_enemy_tick = 100_000;
            e.ai.attack_tick = 0;
        }
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        let order: Vec<i32> = w.pending_ai_attacks.iter().map(|(_, t, _)| *t).collect();
        // 枚举序 = [alive(9,10), 自身(10,10), dead(10,11), ghost(11,10)]，过滤后 [alive, 自身]，
        // 倒序逐个 `_Attack` ⇒ [自身, alive]。
        assert_eq!(
            order,
            vec![mid, alive],
            "`!Death && !Ghost`：死/幽灵目标不得进 objectList"
        );
    }

    /// R31①：3×3 越界格（`CellMatch=false`）与非法格（`Valid=false`）都整格跳过；
    /// 越界**不得**回落到 `GetCellInfo` 的 `CellArray[0]`（自身格）。
    ///
    /// 边界放在 `(0,0)`：`x=-1` 与 `y=-1` 共 5 个越界位；若丢掉 `success` 检查，5 次失败会
    /// 各自回落 `CellArray[0]`（自身格）⇒ 自身被重复 `_Attack`。`(1,1)` 落图后再置
    /// `LowWall`（`set_map_xy_flag` 只改属性、不摘对象）⇒ 该格 `cellSuccess=false`。
    ///
    /// 牙：删 `!success` ⇒ 越界位回落自身格 ⇒ 序列多出自身；删 `is_available`/整格跳过 ⇒
    /// 非法格里的玩家重新入列 ⇒ 序列多出 p_bad。
    #[test]
    fn mb6_r31_around_attack_skips_out_of_range_and_invalid_cells() {
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 131, 0, 0);
        assert!(w.enter_world(mid));
        let p_right = mb1_real_player(&mut w, 1, 0);
        let p_down = mb1_real_player(&mut w, 0, 1);
        let p_bad = mb1_real_player(&mut w, 1, 1);
        assert!(
            w.maps[0].set_map_xy_flag(1, 1, false),
            "置 LowWall（非法格）"
        );
        {
            let e = w.entities.get_mut(mid).unwrap();
            e.ai.target_creature = Some(p_right);
            e.ai.search_enemy_tick = 100_000;
            e.ai.attack_tick = 0;
        }
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        let order: Vec<i32> = w.pending_ai_attacks.iter().map(|(_, t, _)| *t).collect();
        // 枚举序 = [自身(0,0), p_down(0,1), p_right(1,0)]，倒序 ⇒ [p_right, p_down, 自身]。
        assert_eq!(
            order,
            vec![p_right, p_down, mid],
            "越界位不得回落 CellArray[0]；非法格整格跳过（p_bad 不在列）"
        );
        assert!(
            !order.contains(&p_bad),
            "LowWall 格内的玩家不参与 AroundAttack"
        );
    }

    /// R31③：`RACE_CLONE_MONSTER` 的 `Operate` 覆写（恒 `return false`）**不写 `LastHiter`**，
    /// 但 `SendMsg(self, RM_STRUCK)` 的入队面照旧 ⇒ `pending_struck` 仍有该项。
    /// 普通怪（同一路径）两者都写。
    ///
    /// 牙：把 `is_clone` 门删掉 ⇒ clone 的 `last_hiter` 变 `Some(mid)`；把入队也挪进
    /// `!is_clone` ⇒ clone 的 `pending_struck` 为空。
    #[test]
    fn mb6_r31_clone_monster_struck_skips_last_hiter_but_enqueues() {
        let mut w = World::new(32, 32);
        let mid = mb1_monster(&mut w, 131, 10, 10);
        w.entities.get_mut(mid).unwrap().dc = (9 << 8) | 5; // Lo=5, Hi=9
        assert!(w.enter_world(mid));
        let master = monster(&mut w, 20, 20);
        let clone = mb1_monster(&mut w, crate::special_monster::RACE_CLONE_MONSTER, 11, 10);
        assert!(w.enter_world(clone));
        let normal = mb1_monster(&mut w, ACTOR_RACE_MONSTER, 9, 10);
        assert!(w.enter_world(normal));
        for t in [clone, normal] {
            let e = w.entities.get_mut(t).unwrap();
            e.master = Some(master); // `is_attack_target_base` 的 `target.master != null` 支
            e.hp = 500;
            e.max_hp = 500;
            e.ai.pending_struck.clear();
        }
        {
            let e = w.entities.get_mut(mid).unwrap();
            e.ai.target_creature = Some(clone);
            e.ai.search_enemy_tick = 100_000;
            e.ai.attack_tick = 0;
        }
        w.clock_ms = 100_000;
        assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
        // 枚举序 = [normal(9,10), 自身(10,10), clone(11,10)] ⇒ 两者都被 `_Attack`。
        assert!(
            w.pending_ai_attacks.iter().any(|(_, t, _)| *t == clone),
            "CloneMonster 在 objectList 内"
        );
        {
            let c = w.entities.get(clone).unwrap();
            assert_eq!(c.last_hiter, None, "`Operate` 覆写 ⇒ 不写 LastHiter");
            assert_eq!(c.ai.pending_struck.len(), 1, "仍入队 pending_struck");
            assert_eq!(c.ai.pending_struck[0].hiter, mid);
        }
        {
            let n = w.entities.get(normal).unwrap();
            assert_eq!(n.last_hiter, Some(mid), "普通怪照写 LastHiter");
            assert_eq!(n.ai.pending_struck.len(), 1, "普通怪入队 pending_struck");
        }
    }

    /// R31④：`Run` 到点门 `(now - AttackTick) > NextHitTime` 是**严格 `>`**：过门刷新
    /// `AttackTick` 并进 `AroundAttack`（空列表也发 `RM_HIT`）；恰等于 `NextHitTime` 不触发、
    /// 不刷新、不发帧。
    ///
    /// 牙：`>` 改 `>=` ⇒ 边界段红（`attack_tick` 被刷新 + 多一帧）；把 `AttackTick = now`
    /// 删掉 ⇒ 首段 `attack_tick` 仍为 0 ⇒ 红。
    #[test]
    fn mb6_r31_run_attack_tick_refresh_and_strict_next_hit_time() {
        let run = |clock_ms: i64, attack_tick: i64| -> (i64, usize) {
            let mut w = World::new(32, 32);
            let mid = mb1_monster(&mut w, 131, 10, 10); // next_hit_time = 2000
            let t = monster(&mut w, 11, 10);
            {
                let e = w.entities.get_mut(mid).unwrap();
                e.ai.target_creature = Some(t);
                e.ai.search_enemy_tick = clock_ms;
                e.ai.attack_tick = attack_tick;
            }
            w.clock_ms = clock_ms;
            assert_eq!(monster_class_run(&mut w, mid), Some(ClassRunBase::Monster));
            (
                w.entities.get(mid).unwrap().ai.attack_tick,
                w.pending_monster_ref_msgs
                    .iter()
                    .filter(|m| m.ident == RM_HIT)
                    .count(),
            )
        };
        assert_eq!(
            run(2001, 0),
            (2001, 1),
            "差值 2001 > 2000 ⇒ 刷新 AttackTick + AroundAttack（RM_HIT）"
        );
        assert_eq!(
            run(2000, 0),
            (0, 0),
            "差值恰 2000 ⇒ 严格 `>` 不成立：不刷新、不发帧"
        );
        assert_eq!(
            run(2001, 1),
            (1, 0),
            "差值恰 2000（now=2001/AttackTick=1）⇒ 不触发"
        );
    }

    // ------------------------------------------------------------------
    // **MB-7**（R05/R15/R16）——`BoneKingMonster`(134) / `ElfMonster`(113) /
    // `ElfWarriorMonster`(114) 构造级用例 + 改坏必红。
    // ------------------------------------------------------------------

    /// 记录型 `RegenMonsterByName`：按名映射 race，记录名与「构造抽」的次序。
    struct Mb7Spawner {
        race_of: Vec<(&'static str, u8)>,
        names: Vec<String>,
        next_id: i32,
    }

    impl Mb7Spawner {
        fn new(race_of: Vec<(&'static str, u8)>) -> Self {
            Self {
                race_of,
                names: Vec::new(),
                next_id: 9000,
            }
        }
    }

    impl crate::special_monster::SpecialSpawner for Mb7Spawner {
        fn regen_monster_by_name(
            &mut self,
            world: &mut World,
            rng: &mut RandomNumber,
            _map_id: usize,
            name: &str,
            x: i16,
            y: i16,
        ) -> Option<i32> {
            self.names.push(name.to_string());
            // 模拟 `CreateMonster` 的构造抽（记录与 `Random(3)` 的相对次序）。
            let _ = rng.random_below(1);
            let race = self
                .race_of
                .iter()
                .find(|(n, _)| *n == name)
                .map_or(80, |(_, r)| *r);
            let mut e = Entity::new(0, name.to_string(), i32::from(x), i32::from(y));
            e.race = race;
            e.hp = 500;
            e.max_hp = 500;
            crate::mongen::apply_monster_class_constants(&mut e, i16::from(race), 0);
            self.next_id += 1;
            Some(world.add_entity(e))
        }
    }

    /// R05：`DangerLevel > WAbil.HP / WAbil.MaxHP * 5 && DangerLevel > 0` 是 **C# 整数除法**：
    /// `HP = MaxHP - 1` ⇒ 商 0 ⇒ `DangerLevel > 0` 即触发召唤；仅满血时阈值 5。
    /// 召唤点同点抽 `Random(6)+6`；排水时每个 minion 先判 30 上限、再 `Random(3)`、再构造抽。
    ///
    /// 牙：把 `(hp / max_hp) * 5` 改成浮点/`hp * 5 / max_hp` ⇒ 段 1 不召唤 ⇒ 红；
    /// 把 `Random(3)` 提到 30 上限判断之前 ⇒ 段 2 的抽数不符 ⇒ 红；
    /// 把 `DangerLevel -= 1` 删掉 ⇒ 段 1 的 `danger_level` 断言红。
    #[test]
    fn mb7_r05_bone_king_integer_division_and_slave_draw_order() {
        // 段 1：HP=999/MaxHP=1000 ⇒ 阈值 0 ⇒ DangerLevel=1 触发（整数除法）。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 134, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.hp = 999;
            e.max_hp = 1000;
            e.danger_level = 1;
            e.ai.search_enemy_tick = 0;
            e.ai.walk_tick = 0;
            e.ai.walk_speed = 0;
            e.ai.dir = DIR_RIGHT;
        }
        w.rng = RandomNumber::with_seed(1234);
        assert_eq!(
            monster_class_run(&mut w, id),
            Some(ClassRunBase::Monster),
            "base.Run() = MonsterObject.Run"
        );
        assert_eq!(
            w.entities.get(id).unwrap().danger_level,
            0,
            "DangerLevel -= 1"
        );
        let req = w
            .pending_sculture_spawns
            .first()
            .expect("整数除法 ⇒ 应召唤")
            .clone();
        assert_eq!(req.kind, ClassSlaveSpawnKind::BoneKing);
        assert_eq!((req.x, req.y), (11, 10), "GetFrontPosition(Dir=Right)");
        assert!(
            (6..=11).contains(&req.count),
            "Random(6)+6 ⇒ 6..=11，实际 {}",
            req.count
        );

        // 排水：每个 minion = Random(3) → 构造抽；怪名取自固定三表。
        let seed = 777;
        w.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let mut sp = Mb7Spawner::new(vec![
            ("BoneCaptain", 80),
            ("BoneArcher", 80),
            ("BoneSpearman", 80),
        ]);
        const NAMES: [&str; 3] = ["BoneCaptain", "BoneArcher", "BoneSpearman"];
        let mut expected = Vec::new();
        for _ in 0..req.count {
            expected.push(NAMES[expect.random_below(3) as usize].to_string());
            let _ = expect.random_below(1);
        }
        let spawned = w.flush_sculture_spawns(&mut sp);
        assert_eq!(spawned.len(), req.count as usize);
        assert_eq!(sp.names, expected, "Random(3) 与构造抽交错次序");
        assert_eq!(w.rng.random(), expect.random(), "RNG 流水一致");
        assert_eq!(
            w.entities.get(id).unwrap().class_slave_list,
            spawned,
            "成功造出的 minion 入 SlaveObjectList"
        );

        // 段 2：表内已 29 只、count=11 ⇒ 只能再造 1 只：先判 >=30 再抽 Random(3)。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 134, 10, 10);
        w.entities.get_mut(id).unwrap().class_slave_list = (1..=29).collect();
        w.rng = RandomNumber::with_seed(seed);
        w.pending_sculture_spawns.push(ScultureSpawnRequest {
            owner_id: id,
            map_id: 0,
            x: 10,
            y: 11,
            count: 11,
            kind: ClassSlaveSpawnKind::BoneKing,
        });
        let mut sp = Mb7Spawner::new(Vec::new());
        let mut expect = RandomNumber::with_seed(seed);
        let _ = expect.random_below(3);
        let _ = expect.random_below(1); // 构造抽
        let spawned = w.flush_sculture_spawns(&mut sp);
        assert_eq!(spawned.len(), 1, "29+1 ⇒ 到 30 即停");
        assert_eq!(sp.names.len(), 1, "第 2 次起先判 >=30，不再抽 Random(3)");
        assert_eq!(
            w.rng.random(),
            expect.random(),
            ">=30 后**不得**再抽 Random(3)（先判后抽）"
        );
        assert_eq!(w.entities.get(id).unwrap().class_slave_list.len(), 30);

        // 段 3：满血 ⇒ 阈值 5 ⇒ `DangerLevel=5` 不触发，且回满为 5。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 134, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.hp = 1000;
            e.max_hp = 1000;
            e.danger_level = 5;
            e.ai.search_enemy_tick = 0;
            e.ai.walk_tick = 0;
            e.ai.walk_speed = 0;
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert!(w.pending_sculture_spawns.is_empty(), "满血不召唤");
        assert_eq!(w.entities.get(id).unwrap().danger_level, 5);

        // 段 4：死亡/幽灵的子怪在 Run 尾被移除（倒序语义等价于 retain）。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 134, 10, 10);
        let alive = mb1_players(&mut w, 20, 20);
        let dead = mb1_players(&mut w, 21, 20);
        w.entities.get_mut(dead).unwrap().death = true;
        {
            let e = w.entities.get_mut(id).unwrap();
            e.class_slave_list = vec![alive, dead];
            e.ai.search_enemy_tick = 100_000;
            e.ai.walk_tick = 0;
            e.ai.walk_speed = 0;
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(id).unwrap().class_slave_list,
            vec![alive],
            "Death||Ghost 移除"
        );
    }

    /// R05（Attack）：`BoneKingMonster.Attack`（`BoneKingMonster.cs:42-46`）走
    /// `GetAttackPower(Lo(DC), |Hi-Lo|)` → `HitMagAttackTarget(target, 0, nPower, true)`：
    /// 目标格每个 proper 对象先 `GetHitStruckDamage(0)` 再 `GetMagStruckDamage(nPower)`，
    /// **不写 `LastHiter`/不发 `RM_STRUCK`**，尾帧 `RM_HIT`，`RM_REFMESSAGE` 200ms。
    ///
    /// 牙：把 `Some(134)` 落回基版 `attack_target` ⇒ 取数序/帧/`LastHiter` 全变 ⇒ 红。
    #[test]
    fn mb7_r05_bone_king_attack_uses_hit_mag_attack_target() {
        let mut w = World::new(30, 30);
        w.enable_refmessage_delay();
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 134, 10, 10);
        let t = refmsg_target(&mut w, 11, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.dc = 5 | (9 << 8); // Lo=5, Hi=9 ⇒ Random(|9-5|+1)=Random(5)
            e.ai.attack_tick = 0;
            e.ai.next_hit_time = 2000;
            e.ai.target_creature = Some(t);
        }
        let seed = 4242;
        w.rng = RandomNumber::with_seed(seed);
        let mut expect = RandomNumber::with_seed(seed);
        let d0 = expect.random_below(5);
        let _ac = expect.random_below(1); // GetHitStruckDamage：AC=0
        let _mac = expect.random_below(1); // GetMagStruckDamage：MAC=0
        assert!(monster_attack_target(&mut w, id));
        assert_stream_eq(&mut w, &mut expect, "BoneKing.Attack 取数序");
        let refs = &w.pending_monster_ref_msgs;
        assert_eq!(
            refs.iter().filter(|m| m.ident == RM_HIT).count(),
            1,
            "HitMagAttackTarget 尾帧 RM_HIT"
        );
        {
            let target = w.entities.get(t).unwrap();
            assert!(target.hp < 1000, "命中扣血");
            assert!(
                target.last_hiter.is_none(),
                "HitMagAttackTarget 不 SetLastHiter"
            );
            assert!(target.ai.pending_struck.is_empty(), "不发 RM_STRUCK");
        }
        let msg = w.self_delay_msgs.peek(t).expect("RM_REFMESSAGE");
        assert_eq!(msg.ident, crate::slave::RM_REFMESSAGE);
        assert_eq!(msg.w_param, 5 + d0, "nPower = Lo + Random(|Hi-Lo|+1)");
        assert_eq!(msg.deliver_at_ms, 100_200, "200ms");
    }

    /// R15：`ElfMonster.Run` 的 `BoIsFirst` 只触发一次（出土帧 + `ResetElfMon`）；
    /// 死亡支 `(now-DeathTick) > 2000` 严格 `>` ⇒ 恰 2000ms 不 Ghost。
    ///
    /// 牙：把 `BoIsFirst` 的清位删掉 ⇒ 第二次 Run 多发一帧 ⇒ 段 2 红；
    /// 把 `>` 改成 `>=` ⇒ 段 3 红。
    #[test]
    fn mb7_r15_elf_monster_first_dig_and_two_second_ghost() {
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 113, 10, 10);
        {
            let e = w.entities.get(id).unwrap();
            assert!(e.bo_is_first, "ctor BoIsFirst=true");
            assert!(e.fixed_hide_mode, "ctor FixedHideMode=true");
            assert!(e.ai.no_attack_mode, "ctor NoAttackMode=true");
            assert_eq!(e.view_range, 6, "ctor ViewRange=6");
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        {
            let e = w.entities.get(id).unwrap();
            assert!(!e.bo_is_first);
            assert!(!e.fixed_hide_mode, "首 Run 出土");
            assert_eq!(e.ai.walk_speed, 500, "ResetElfMon: 500 - 0*50");
            assert_eq!(e.ai.walk_tick, 102_000, "ResetElfMon: now+2000");
        }
        assert_eq!(
            w.pending_monster_ref_msgs
                .iter()
                .filter(|m| m.ident == RM_DIGUP)
                .count(),
            1,
            "首 Run 发一次 RM_DIGUP"
        );
        // 段 2：第二次 Run 不再出土。
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert_eq!(
            w.pending_monster_ref_msgs
                .iter()
                .filter(|m| m.ident == RM_DIGUP)
                .count(),
            1,
            "BoIsFirst 只触发一次"
        );
        assert!(
            w.pending_sculture_spawns.is_empty(),
            "无目标/无主人联动 ⇒ 不 MakeClone"
        );

        // 段 3：死亡 2s 边界。
        let mut w = World::new(30, 30);
        w.clock_ms = 102_000;
        let id = class_monster(&mut w, 113, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.bo_is_first = false;
            e.death = true;
            e.death_tick = 100_000;
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert!(!w.entities.get(id).unwrap().ghost, "恰 2000ms 不 MakeGhost");
        w.clock_ms = 102_001;
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert!(w.entities.get(id).unwrap().ghost, "> 2000ms ⇒ MakeGhost");
    }

    /// R15（`monster_ai_tick` 死怪路由）：死亡 race 113/114 必须经类 `Run` 的 2s 严格 `>` 门
    /// `MakeGhost`，**不是** `Config.MakeGhostTime` 的通用门（缺省 180s）。
    ///
    /// 这条补的是 `monster_ai_tick` 死怪支的 `matches!(e.race, 96 | 113 | 114)`——此前只有
    /// `monster_class_run` 直调用例，没有任何用例证明**死**的 113/114 在 tick 里真的进类 `Run`。
    ///
    /// 牙：把 `matches!(e.race, 96 | 113 | 114)` 改回 `96`（写集外也要能证）⇒ 死怪只落
    /// `base_object_run_monster` 的 180s 门 ⇒ 2001ms 段不 Ghost ⇒ 本用例红。
    #[test]
    fn mb7_dead_elf_races_ghost_through_class_run_via_ai_tick() {
        for race in [113u8, 114u8] {
            let mut w = World::new(30, 30);
            w.clock_ms = 102_000;
            assert!(
                w.make_ghost_time > 2_001,
                "前提：通用 MakeGhostTime 远大于类 Run 的 2s 门（oracle 缺省 180s）"
            );
            let id = class_monster(&mut w, race, 10, 10);
            {
                let e = w.entities.get_mut(id).unwrap();
                e.bo_is_first = false;
                e.death = true;
                e.death_tick = 100_000;
            }
            // 恰 2000ms：类 `Run` 的严格 `>` 不成立；通用门（180s）也远未到 ⇒ 不 Ghost。
            monster_ai_tick(&mut w);
            assert!(
                !w.entities.get(id).unwrap().ghost,
                "race {race}: 恰 2000ms 不得 MakeGhost"
            );
            // 2001ms：类 `Run` 的 2s 门命中（通用门尚差约 178s）⇒ 必须 Ghost。
            w.clock_ms = 102_001;
            monster_ai_tick(&mut w);
            assert!(
                w.entities.get(id).unwrap().ghost,
                "race {race}: >2000ms 必须 MakeGhost（类 Run 门，非通用 MakeGhostTime 门）"
            );
        }
    }

    /// R15（生成链）：`MakeClone(Dragon1)` 的请求点、字段继承与收尾。
    ///
    /// 段 1：`Dragon1` 造出 113 ⇒ 不 `AppearNow`；字段复制 + `Master=null` + `KickException`。
    /// 段 2：宿主把 `Dragon1` 配成 `神兽1`（114）⇒ 克隆走 `ElfWarriorMonster.AppearNow`
    ///        （出土帧 + `DigDownTick=now` + `WalkTick=now+2000+800`）。
    ///
    /// 牙：删 `apply_make_clone` 的 `MasterRoyaltyTick` 复制 ⇒ 段 1 红；
    /// 把 `source_race` 分支写成无条件 `AppearNow` ⇒ 段 1 多一帧/`fixed_hide` 红。
    #[test]
    fn mb7_r15_elf_monster_make_clone_inheritance_and_appear_now() {
        let run = |dragon1_race: u8| {
            let mut w = World::new(30, 30);
            w.clock_ms = 100_000;
            let id = class_monster(&mut w, 113, 10, 10);
            w.entities.get_mut(id).unwrap().name = "神兽".into();
            let master = mb1_players(&mut w, 20, 20);
            let target = mb1_players(&mut w, 12, 10);
            {
                let e = w.entities.get_mut(id).unwrap();
                e.bo_is_first = false;
                e.fixed_hide_mode = false;
                e.hp = 300;
                e.max_hp = 500;
                e.dc = 5 | (9 << 8);
                e.master = Some(master);
                e.master_royalty_tick = 42;
                e.slave_make_level = 2;
                e.slave_exp_level = 1;
                e.ai.dir = 3;
                e.status_time[0] = 7;
                e.ai.target_creature = Some(target);
                e.ai.target_focus_tick = 555;
                e.last_hiter = Some(target);
                e.last_hiter_tick = 666;
            }
            assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
            let req = w.pending_sculture_spawns[0].clone();
            assert_eq!(
                req.kind,
                ClassSlaveSpawnKind::MakeClone {
                    name: "神兽".into(),
                    source_race: 113
                },
                "MakeClone(Config.Dragon1, this)"
            );
            // `ElfMonster.Run` 恒请求 `Config.Dragon1`；测试用 race_of 模拟配置指向的类。
            let mut sp = Mb7Spawner::new(vec![(DRAGON1_NAME, dragon1_race)]);
            let spawned = w.flush_sculture_spawns(&mut sp);
            assert_eq!(spawned.len(), 1);
            (w, id, master, target, spawned[0])
        };

        // 段 1：Dragon1 仍是部署值「神兽」(113)。
        let (w, id, master, target, clone) = run(113);
        {
            let c = w.entities.get(clone).unwrap();
            assert_eq!(c.race, 113);
            assert_eq!(c.master, Some(master), "Master 继承");
            assert_eq!(c.master_royalty_tick, 42);
            assert_eq!(c.slave_make_level, 2);
            assert_eq!(c.slave_exp_level, 1);
            assert_eq!(c.ai.dir, 3);
            assert_eq!(c.status_time[0], 7);
            assert_eq!(c.ai.target_creature, Some(target));
            assert_eq!(c.ai.target_focus_tick, 555);
            assert_eq!(c.last_hiter, Some(target));
            assert_eq!(c.last_hiter_tick, 666);
            assert!(c.is_slave);
            // 113 克隆的 ctor 仍是 `FixedHideMode=true`/`BoIsFirst=true`（`ElfMonster.cs:18/20`）；
            // 源 113 的收尾只对 `ElfWarriorMonster` 调 `AppearNow`。
            assert!(c.fixed_hide_mode, "113 克隆保留 ctor FixedHideMode");
            assert!(c.bo_is_first, "113 克隆保留 ctor BoIsFirst");
        }
        assert_eq!(
            w.entities.get(master).unwrap().slave_list,
            vec![clone],
            "MakeClone 尾：Master.SlaveList.Add"
        );
        {
            let old = w.entities.get(id).unwrap();
            assert!(old.death, "KickException ⇒ Death=true");
            assert!(old.ghost, "KickException ⇒ MakeGhost");
            assert_eq!(old.master, None, "成功换形后 Master=null");
        }

        // 段 2：宿主把 Dragon1 配成「神兽1」(114) ⇒ 克隆走 ElfWarrior.AppearNow。
        let (w, _id, _master, _target, clone) = run(114);
        {
            let c = w.entities.get(clone).unwrap();
            assert_eq!(c.race, 114, "AppearNow 再置 Race=ElfWarriormon");
            // **Abil 快照钉数**（oracle 两次 `RecalcAbilitys` 的复合门）：
            // 源怪 `Abil` = `dc=0x0905` / `MaxHP=500`（上面设的源 113 值），克隆自身
            // 生成值 = `dc=0` / `MaxHP=500`（`Mb7Spawner`）。
            // 第 1 次 Recalc（`MonsterObject.cs:113`）用克隆自身 Abil ⇒ `dc=0x0100`/`700`；
            // `:115` `clone.Abil = OldMon.Abil`；第 2 次（`ElfWarriorMonster.cs:20`）
            // 从源怪 Abil 重算 ⇒ `dc = 0x0905 高字节 9 + Round(3*(0.3+0.1*1)*1)=1 = 0x0A`、
            // `MaxHP = 500 + Round(500*0.4)*1 = 700`。
            // 旧实现把「当前 WAbil」当 Abil ⇒ 复合两次得 `dc=0x0200(512)`/`MaxHP=980`。
            assert_eq!(
                c.abil.map(|a| (a.dc, a.max_hp)),
                Some((0x0905, 500)),
                "MonsterObject.cs:115 clone.Abil = OldMon.Abil"
            );
            assert_eq!(c.dc, 0x0A05, "oracle 手算 dc=0x0A05(2565)，非复合值 0x0200");
            assert_eq!(c.max_hp, 700, "oracle 手算 MaxHP=700，非复合值 980");
            assert!(!c.bo_is_first);
            assert!(!c.fixed_hide_mode);
            assert_eq!(c.dig_down_tick, 100_000);
            assert_eq!(c.ai.walk_tick, 102_800, "Recalc: now+2000, 再 +800");
            assert_eq!(c.ai.walk_speed, 400, "500 - SlaveMakeLevel(2)*50");
            assert_eq!(c.ai.next_hit_time, 1300, "1500 - 2*100");
        }
        assert_eq!(
            w.pending_monster_ref_msgs
                .iter()
                .filter(|m| m.ident == RM_DIGUP && m.actor_id == clone)
                .count(),
            1,
            "ElfWarrior.AppearNow 发 RM_DIGUP"
        );
    }

    /// R16：`ElfWarriorMonster.Run` 的 `boChangeFace` 与 `DigDownTick` 作用域。
    ///
    /// - 有目标 / 主人有目标或 `LastHiter` ⇒ `boChangeFace=false` ⇒ **刷新** `DigDownTick`；
    /// - `boChangeFace=true` 且未到 60s / 名字尾位不是 `1` ⇒ **不刷新**（else 作用域）；
    /// - 60s 门严格 `>`，名字 `[^1]=='1'` 才去尾并 `MakeClone`；
    /// - 换形成功 ⇒ `RM_DIGDOWN` + `RM_CHANGEFACE` + 克隆 `AppearNow` + `Master=null` + `KickException`。
    ///
    /// 牙：把 `else { DigDownTick = now; }` 移到 `if (boChangeFace)` 内 ⇒ 段 2/3 红；
    /// 把 `> 60000` 改 `>=` ⇒ 段 4 红；名字判断改成 `ends_with("1")` 去多个 1 ⇒ 段 5 红。
    #[test]
    fn mb7_r16_elf_warrior_change_face_scope_and_clone_frames() {
        // 段 1：首次出土（BoIsFirst）→ ResetElfMon。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 114, 10, 10);
        w.entities.get_mut(id).unwrap().slave_make_level = 2;
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        {
            let e = w.entities.get(id).unwrap();
            assert!(!e.bo_is_first);
            assert!(!e.fixed_hide_mode);
            assert_eq!(e.ai.next_hit_time, 1300);
            assert_eq!(e.ai.walk_speed, 400);
            assert_eq!(e.ai.walk_tick, 102_000);
        }

        // 段 2：boChangeFace=false（主人有目标）⇒ 刷新 DigDownTick。
        let mut w = World::new(30, 30);
        w.clock_ms = 100_000;
        let id = class_monster(&mut w, 114, 10, 10);
        let master = mb1_players(&mut w, 20, 20);
        let mtarget = mb1_players(&mut w, 21, 20);
        w.entities.get_mut(mtarget).unwrap().death = false;
        {
            let e = w.entities.get_mut(id).unwrap();
            e.bo_is_first = false;
            e.name = "神兽1".into();
            e.dig_down_tick = 0;
            e.master = Some(master);
        }
        w.entities.get_mut(master).unwrap().ai.target_creature = Some(mtarget);
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(id).unwrap().dig_down_tick,
            100_000,
            "boChangeFace=false ⇒ else 支刷新"
        );
        assert!(w.pending_sculture_spawns.is_empty());

        // 段 3：boChangeFace=true、未到 60s ⇒ 不刷新、不克隆。
        let mut w = World::new(30, 30);
        w.clock_ms = 160_000;
        let id = class_monster(&mut w, 114, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.bo_is_first = false;
            e.name = "神兽1".into();
            e.dig_down_tick = 100_000; // 恰 60000ms
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert_eq!(
            w.entities.get(id).unwrap().dig_down_tick,
            100_000,
            "boChangeFace=true 且未到 60s ⇒ **不得**刷新"
        );
        assert!(w.pending_sculture_spawns.is_empty(), "严格 > 60000ms");

        // 段 4：名字尾位不是 '1' ⇒ boChangeFace=true 也不克隆、不刷新。
        let mut w = World::new(30, 30);
        w.clock_ms = 200_000;
        let id = class_monster(&mut w, 114, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.bo_is_first = false;
            e.name = "神兽".into();
            e.dig_down_tick = 0;
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        assert!(w.pending_sculture_spawns.is_empty(), "尾位非 '1' 不克隆");
        assert_eq!(
            w.entities.get(id).unwrap().dig_down_tick,
            0,
            "boChangeFace=true ⇒ 不刷新"
        );

        // 段 5：60s 过门 + 名字尾位 '1' ⇒ 去尾 MakeClone；排水出两帧 + AppearNow。
        let mut w = World::new(30, 30);
        w.clock_ms = 200_000;
        let id = class_monster(&mut w, 114, 10, 10);
        {
            let e = w.entities.get_mut(id).unwrap();
            e.bo_is_first = false;
            e.name = "神兽1".into();
            e.dig_down_tick = 100_000;
            e.slave_make_level = 1;
            e.ai.dir = 5;
        }
        assert_eq!(monster_class_run(&mut w, id), Some(ClassRunBase::Monster));
        let req = w.pending_sculture_spawns[0].clone();
        assert_eq!(
            req.kind,
            ClassSlaveSpawnKind::MakeClone {
                name: "神兽".into(),
                source_race: 114
            }
        );
        let mut sp = Mb7Spawner::new(vec![("神兽", 113)]);
        let spawned = w.flush_sculture_spawns(&mut sp);
        assert_eq!(spawned.len(), 1);
        let clone = spawned[0];
        let refs = &w.pending_monster_ref_msgs;
        assert_eq!(
            refs.iter()
                .filter(|m| m.actor_id == id && m.ident == RM_DIGDOWN)
                .count(),
            1,
            "换形成功 ⇒ RM_DIGDOWN"
        );
        let change = refs
            .iter()
            .find(|m| m.actor_id == id && m.ident == RM_CHANGEFACE)
            .expect("RM_CHANGEFACE");
        assert_eq!(
            (change.n_param1, change.n_param2),
            (id, clone),
            "RM_CHANGEFACE(0, ActorId, elfMon.ActorId, 0)"
        );
        {
            let c = w.entities.get(clone).unwrap();
            assert_eq!(c.race, 113);
            assert!(!c.fixed_hide_mode, "ElfMonster.AppearNow");
            // `ElfMonster.AppearNow`（`:24-29`）**不**清 `BoIsFirst` ⇒ 113 克隆首 Run 仍会出土。
            assert!(c.bo_is_first);
            assert_eq!(c.ai.walk_tick, 202_800, "now+2000 再 +800");
            assert_eq!(c.ai.walk_speed, 450, "500 - 1*50");
        }
        {
            let old = w.entities.get(id).unwrap();
            assert!(old.death);
            assert!(old.ghost);
            assert_eq!(old.master, None);
        }
    }
}

/// **卡 `guard-target`**：`GuardUnit.IsProperTarget` 的 `Castle != null` 分支
/// （oracle `src/M2Server/Monster/Monsters/GuardUnit.cs:35-125`，城堡分支 `37-100`）逐行判据。
///
/// 生产路径 = [`super::guard_unit_is_proper_target`]（`ArcherGuard.Run` 的索敌门）；
/// 每条用例只经 `World` 的公开接线口（`castle_binding` / `set_actor_guild` /
/// `set_guild_allies` / `animal_struck`）布置前置，不改生产分支。
#[cfg(test)]
mod guard_target_castle_tests {
    use super::*;
    use crate::castle::{CastleSiegeConfig, DefenseRole, GuildId, UserCastle};
    use crate::entity::{CastleBinding, VisibleBaseObject, VisibleFlag};

    const G_CASTLE: GuildId = GuildId(7);
    const G_ALLY: GuildId = GuildId(9);
    const G_OTHER: GuildId = GuildId(77);

    /// 一座城 + 一个已绑定的弓手（race 112，`Castle != null`）。
    fn world_with_archer(under_war: bool, master_guild: Option<GuildId>) -> (World, i32) {
        let mut w = World::new(20, 20);
        w.clock_ms = 100_000;
        let mut e = Entity::new(1, "弓手", 5, 5);
        e.race = 112;
        e.hp = 100;
        e.max_hp = 100;
        let archer = w.add_entity(e);
        let mut castle = UserCastle::new("沙巴克", CastleSiegeConfig::default());
        castle.under_war = under_war;
        castle.master_guild = master_guild;
        w.castles.push(castle);
        w.entities.get_mut(archer).unwrap().castle_binding = Some(CastleBinding {
            castle_index: 0,
            role: DefenseRole::Archer(0),
        });
        (w, archer)
    }

    fn add_player(w: &mut World, x: i32, y: i32, guild: Option<GuildId>) -> i32 {
        let mut e = Entity::new(0, "玩家", x, y);
        e.hp = 100;
        e.max_hp = 100;
        let id = w.add_entity(e);
        if let Some(g) = guild {
            w.set_actor_guild(id, g);
        }
        id
    }

    fn add_archer_at(w: &mut World, x: i32, y: i32, castle_index: Option<usize>) -> i32 {
        let mut e = Entity::new(0, "弓手", x, y);
        e.race = 112;
        e.hp = 100;
        e.max_hp = 100;
        let id = w.add_entity(e);
        if let Some(castle_index) = castle_index {
            w.entities.get_mut(id).unwrap().castle_binding = Some(CastleBinding {
                castle_index,
                role: DefenseRole::Archer(1),
            });
        }
        id
    }

    /// C1 `same_castle_units_do_not_attack_each_other`：`baseObject.Castle == Castle`
    /// （同 `castle_index` 的绑定实体）⇒ `false`；**另一座城**（`castle_index = 1`）的同 race
    /// 单位仍可打；未绑定城堡的 112 也不触发同城排除。
    ///
    /// 牙：删 `target.castle.map(..) == Some(binding.castle_index)` ⇒ 第一条红。
    #[test]
    fn c1_same_castle_units_do_not_attack_each_other() {
        let (mut w, archer) = world_with_archer(false, None);
        let mate = add_archer_at(&mut w, 7, 5, Some(0));
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, mate),
            "同城单位（target.Castle == Castle）必须互不攻击"
        );
        // 真的补第二座城（`castle_index = 1`）：同 race、不同 Castle ⇒ 不是同城。
        w.castles
            .push(UserCastle::new("沙巴克2", CastleSiegeConfig::default()));
        let foreign = add_archer_at(&mut w, 9, 5, Some(1));
        assert!(
            guard_unit_is_proper_target(&mut w, archer, foreign),
            "另一座城（castle_index = 1）的同 race 单位仍可打"
        );
        // 未绑定城堡的 112 ⇒ 也不触发同城排除（防「凡 112 都 false」的假实现）。
        let unbound = add_archer_at(&mut w, 10, 5, None);
        assert!(
            guard_unit_is_proper_target(&mut w, archer, unbound),
            "未绑定城堡的 112 不是同城 ⇒ 走尾段可打"
        );
    }

    /// C2 `master_guild_players_are_exempt`：无主人的玩家 `MyGuild == MasterGuild` ⇒ `false`；
    /// 非成员仍可打；`LastHiter == target` 时豁免让位于先手（⇒ `true`）。
    ///
    /// 牙：把 `castle_exempts_guild` 的命中分支去掉（恒 `false`）⇒ 第一条红。
    #[test]
    fn c2_master_guild_players_are_exempt() {
        let (mut w, archer) = world_with_archer(false, Some(G_CASTLE));
        let member = add_player(&mut w, 8, 5, Some(G_CASTLE));
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, member),
            "MasterGuild 成员豁免"
        );
        let outsider = add_player(&mut w, 9, 5, Some(G_OTHER));
        assert!(
            guard_unit_is_proper_target(&mut w, archer, outsider),
            "非盟友/非 MasterGuild 的玩家可打"
        );
        w.entities.get_mut(archer).unwrap().last_hiter = Some(member);
        assert!(
            guard_unit_is_proper_target(&mut w, archer, member),
            "LastHiter == target 先手优先于 MasterGuild 豁免（oracle `LastHiter != baseObject`）"
        );
    }

    /// C3 `ally_guild_players_are_exempt`：`MasterGuild.IsAllyGuild(MyGuild)` ⇒ `false`；
    /// 盟友表为空时同一条不豁免。
    ///
    /// 牙：`World::is_ally_guild` 恒 `false` ⇒ 第一条红。
    #[test]
    fn c3_ally_guild_players_are_exempt() {
        let (mut w, archer) = world_with_archer(false, Some(G_CASTLE));
        w.set_guild_allies(G_CASTLE, &[G_ALLY]);
        let ally = add_player(&mut w, 8, 5, Some(G_ALLY));
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, ally),
            "盟友行会成员豁免"
        );
        let (mut w2, archer2) = world_with_archer(false, Some(G_CASTLE));
        let ally2 = add_player(&mut w2, 8, 5, Some(G_ALLY));
        assert!(
            guard_unit_is_proper_target(&mut w2, archer2, ally2),
            "盟友表空 ⇒ 不豁免（缺省与 oracle 的恒空 GuildAllList 一致）"
        );
    }

    /// C4 `under_war_attacks_everything`：`Castle.UnderWar` 的 `true` **先于**成员豁免、
    /// 管理员/石化排除与同城排除 ⇒ MasterGuild 成员、管理员、同城单位全部可打。
    ///
    /// 牙：把 `UnderWar` 门改成 `under_war && !target.admin_mode`（即排到管理员门之后）
    /// ⇒ 第二条红；再改成仅在非成员时开战 ⇒ 第一条红。
    #[test]
    fn c4_under_war_attacks_everything() {
        let (mut w, archer) = world_with_archer(true, Some(G_CASTLE));
        let member = add_player(&mut w, 8, 5, Some(G_CASTLE));
        assert!(
            guard_unit_is_proper_target(&mut w, archer, member),
            "UnderWar 先于 MasterGuild 豁免（全员开打）"
        );
        w.entities.get_mut(member).unwrap().admin_mode = true;
        assert!(
            guard_unit_is_proper_target(&mut w, archer, member),
            "UnderWar 先于管理员排除（oracle 在 AdminMode 前 return true）"
        );
        let mate = add_archer_at(&mut w, 7, 5, Some(0));
        assert!(
            guard_unit_is_proper_target(&mut w, archer, mate),
            "UnderWar 先于同城排除"
        );
    }

    /// C5 `null_castle_path_is_unchanged`：弓手**未绑定** `Castle` 时，即使世界里有城、
    /// 目标恰是 MasterGuild 成员，也按旧路径返回 `true`；旧尾段的红名/石化/管理员语义不变。
    ///
    /// 牙：把分支门 `if let Some(binding) = actor_castle` 改成「世界有城就进」
    /// （`if let Some(castle) = world.castles.first()`）⇒ 第一条红。
    #[test]
    fn c5_null_castle_path_is_unchanged() {
        let mut w = World::new(20, 20);
        w.clock_ms = 100_000;
        let archer = add_archer_at(&mut w, 5, 5, None);
        let mut castle = UserCastle::new("沙巴克", CastleSiegeConfig::default());
        castle.master_guild = Some(G_CASTLE);
        w.castles.push(castle);
        let member = add_player(&mut w, 8, 5, Some(G_CASTLE));
        assert!(
            guard_unit_is_proper_target(&mut w, archer, member),
            "Castle == null ⇒ 不看 MasterGuild（旧路径）"
        );
        let red = add_player(&mut w, 9, 5, Some(G_OTHER));
        w.entities.get_mut(red).unwrap().pk_point = 250;
        assert!(
            guard_unit_is_proper_target(&mut w, archer, red),
            "旧尾段：PvpLevel >= 2 的玩家可打"
        );
        let stone = add_player(&mut w, 10, 5, None);
        w.entities.get_mut(stone).unwrap().stone_mode = true;
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, stone),
            "旧尾段：石化目标不可打"
        );
        let admin = add_player(&mut w, 11, 5, None);
        w.entities.get_mut(admin).unwrap().admin_mode = true;
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, admin),
            "旧尾段：管理员玩家不可打"
        );
    }

    /// C6 `player_master_guild_is_exempt`：`baseObject.Master` 是玩家且其 `MyGuild` 命中
    /// `MasterGuild` ⇒ 宝宝豁免；`LastHiter` 是主人或宝宝时仍打。
    ///
    /// 牙：把主人分支的 `actor_guild(master_id)` 改成 `actor_guild(target.id)`（宝宝无行会）
    /// ⇒ 第一条红。
    #[test]
    fn c6_player_master_guild_is_exempt() {
        let (mut w, archer) = world_with_archer(false, Some(G_CASTLE));
        let owner = add_player(&mut w, 9, 9, Some(G_CASTLE));
        let mut pet = Entity::new(0, "神兽", 8, 5);
        pet.race = 80;
        pet.hp = 100;
        pet.max_hp = 100;
        pet.master = Some(owner);
        let pet_id = w.add_entity(pet);
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, pet_id),
            "主人的 MyGuild == MasterGuild ⇒ 宝宝豁免"
        );
        w.entities.get_mut(archer).unwrap().last_hiter = Some(owner);
        assert!(
            guard_unit_is_proper_target(&mut w, archer, pet_id),
            "LastHiter == 主人 ⇒ 豁免让位（打）"
        );
        w.entities.get_mut(archer).unwrap().last_hiter = Some(pet_id);
        assert!(
            guard_unit_is_proper_target(&mut w, archer, pet_id),
            "LastHiter == 宝宝 ⇒ 豁免让位（打）"
        );
    }

    /// C7 `crime_for_castle_window`：`GuardUnit.Struck` 经 `animal_struck` 置
    /// `CrimeforCastle`；2 分钟内 race 11 守卫是合法目标，到期清位且自身 `Castle != null`
    /// ⇒ `false`。
    ///
    /// 牙：删 `animal_struck` 尾部的 `set_guard_crime_for_castle` ⇒ 第一条红；
    /// 把过期门 `<` 改成 `<=` 或直接 `return true` ⇒ 第二条红。
    #[test]
    fn c7_crime_for_castle_window() {
        let (mut w, archer) = world_with_archer(false, None);
        // oracle 的守卫表（`UserCastle.Initialize` 的 `Guards[]`）不赋 `Castle`；
        // 这里显式绑定以钉住 `CrimeforCastle` 子分支的语义。
        let mut guard = Entity::new(0, "守卫", 8, 5);
        guard.race = 11;
        guard.hp = 100;
        guard.max_hp = 100;
        let guard_id = w.add_entity(guard);
        w.entities.get_mut(guard_id).unwrap().castle_binding = Some(CastleBinding {
            castle_index: 0,
            role: DefenseRole::Guard(0),
        });
        animal_struck(&mut w, guard_id, archer);
        assert_eq!(
            w.guard_crime_for_castle_tick(guard_id),
            Some(w.clock_ms),
            "GuardUnit.Struck（Castle != null）必须置 CrimeforCastle + 时刻"
        );
        assert!(
            guard_unit_is_proper_target(&mut w, archer, guard_id),
            "< 2min 的犯罪守卫是合法目标"
        );
        w.clock_ms += GUARD_CRIME_FOR_CASTLE_MS;
        assert!(
            !guard_unit_is_proper_target(&mut w, archer, guard_id),
            "到期且守卫自身 Castle != null ⇒ false"
        );
        assert_eq!(
            w.guard_crime_for_castle_tick(guard_id),
            None,
            "到期必须清 CrimeforCastle"
        );
    }

    /// C8 `npc_race_guard_is_not_a_target`：`Race ∈ [NPC, Animal)` 排除子句
    /// （oracle `GuardUnit.cs:96`）经**生产索敌路径**（`ArcherGuard.Run`）生效。
    ///
    /// 场景：城堡弓手（`Castle != null`）的可见集里同时有「race 11 守卫（未绑定 Castle、
    /// 无 `CrimeforCastle`、距离更近）」与「玩家（更远）」⇒ race 11 守卫被该子句排除
    /// （race 11 ∈ [10, 50)），最近 proper 目标是玩家。改前旧尾段对 race 11 返回 `true`
    /// （不命中红名/石化/目标为 112 的任何分支），故删该子句会把守卫选成目标。
    ///
    /// 牙：把 `(target.race >= NPC && target.race < ANIMAL)` 替换成 `false` ⇒ 断言红
    /// （race 11 距离 3 < 玩家距离 7，会被选为 `target_creature`）。
    #[test]
    fn c8_npc_race_guard_is_not_a_target() {
        let (mut w, archer) = world_with_archer(false, None);
        // oracle 的 `UserCastle.Initialize` 守卫表不赋 `Castle` ⇒ 这里同样不绑。
        let mut guard = Entity::new(0, "守卫", 8, 5);
        guard.race = 11;
        guard.hp = 100;
        guard.max_hp = 100;
        let guard_id = w.add_entity(guard);
        // 无犯罪标记：race-11 子分支不提前 `return true`，必须落到 [NPC, Animal) 排除。
        assert_eq!(w.guard_crime_for_castle_tick(guard_id), None);
        let player_id = add_player(&mut w, 12, 5, None);
        // 守卫更近（距离 3），玩家更远（距离 7）。
        w.entities.get_mut(archer).unwrap().visible_actors = vec![
            VisibleBaseObject {
                target: guard_id,
                flag: VisibleFlag::Show,
            },
            VisibleBaseObject {
                target: player_id,
                flag: VisibleFlag::Show,
            },
        ];
        {
            let e = w.entities.get_mut(archer).unwrap();
            e.ai.walk_speed = 0;
            e.ai.walk_tick = 99_999;
        }
        archer_guard_run(&mut w, archer);
        assert_eq!(
            w.entities.get(archer).unwrap().ai.target_creature,
            Some(player_id),
            "Race ∈ [NPC, Animal) 的未犯罪守卫不是合法目标 ⇒ 跳过它选更远的玩家"
        );
    }
}

#[cfg(test)]
mod mb89_tests {
    use super::*;
    use crate::entity::{VisibleBaseObject, VisibleFlag};
    use crate::world::World;
    use mir2_shared::rng::RandomNumber;

    fn spawn_monster(world: &mut World, race: i16, x: i32, y: i32, hp: u16) -> i32 {
        let mut e = Entity::new(0, "mb89", x, y);
        e.race = race as u8;
        e.hp = hp;
        e.max_hp = hp;
        crate::mongen::apply_monster_class_constants(&mut e, race, world.clock_ms);
        let id = world.add_entity(e);
        assert!(world.enter_world(id), "怪物落图");
        id
    }

    fn spawn_player(world: &mut World, x: i32, y: i32, hp: u16) -> i32 {
        let mut e = Entity::new(0, "mb89p", x, y);
        e.hp = hp;
        e.max_hp = hp;
        let id = world.add_entity(e);
        assert!(world.enter_world(id), "玩家落图");
        id
    }

    fn stream_eq(world: &World, expect: &RandomNumber, what: &str) {
        assert_eq!(
            format!("{:?}", world.rng),
            format!("{expect:?}"),
            "{what}：随机流必须逐次一致"
        );
    }

    // ------------------------------ R42 WhiteSkeleton ------------------------------

    /// R42（L）：ctor 三常量 + 首次 `Run` 只发一次 `RM_DIGUP(Dir=5)` 并清 `FixedHideMode`。
    ///
    /// 牙：把 `BoIsFirst` 置位挪到发帧之后 / 去掉 `if !first return` / `Dir` 写错 ⇒ 红。
    #[test]
    fn white_skeleton_ctor_and_single_digup() {
        let mut world = World::new(30, 30);
        world.clock_ms = 1000;
        let id = spawn_monster(&mut world, 100, 10, 10, 100);
        {
            let e = world.entities.get(id).unwrap();
            assert!(e.bo_is_first, "ctor BoIsFirst=true");
            assert!(e.fixed_hide_mode, "ctor FixedHideMode=true");
            assert_eq!(e.view_range, 6, "ctor ViewRange=6");
        }
        white_skeleton_run(&mut world, id);
        {
            let e = world.entities.get(id).unwrap();
            assert!(!e.bo_is_first);
            assert_eq!(e.ai.dir, 5, "首次 Run 置 Dir=5");
            assert!(!e.fixed_hide_mode, "首次 Run 清 FixedHideMode");
        }
        assert_eq!(world.pending_monster_ref_msgs.len(), 1);
        let f = &world.pending_monster_ref_msgs[0];
        assert_eq!(f.ident, RM_DIGUP);
        assert_eq!(
            (f.w_param, f.n_param1, f.n_param2, f.n_param3),
            (5, 10, 10, 0),
            "RM_DIGUP(Dir, CurrX, CurrY, 0)"
        );
        white_skeleton_run(&mut world, id);
        assert_eq!(
            world.pending_monster_ref_msgs.len(),
            1,
            "BoIsFirst 只发一次出土帧"
        );
    }

    // ------------------------------ R36 SoccerBall ------------------------------

    /// R36：`SoccerBall.Run` 的 Dir 反射表（0↔4、1↔7、2↔6、3↔5）全 8 向。
    #[test]
    fn soccer_ball_reflect_table_is_oracle_involution() {
        for (dir, want) in [
            (0u8, 4u8),
            (1, 7),
            (2, 6),
            (3, 5),
            (4, 0),
            (5, 3),
            (6, 2),
            (7, 1),
        ] {
            assert_eq!(soccer_ball_reflect_dir(dir), want, "dir {dir}");
        }
    }

    /// R36：`N550 > 0` ⇒ 前格可走 ⇒ 反射 `Dir` 后按 `N550` 写 `TargetX/Y`；
    /// `N550 == 0` ⇒ `TargetX = -1`；到达目标格才清 `N550`。
    ///
    /// 牙：反射表方向写错 / 忘记 `GetNextPosition` 的 `nFlag=N550` / 未到达就清 `N550` ⇒ 红。
    #[test]
    fn soccer_ball_run_writes_reflected_target_from_n550() {
        let mut world = World::new(40, 40);
        world.clock_ms = 1000;
        let id = spawn_monster(&mut world, 120, 10, 10, 100);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.n550 = 3;
            e.ai.dir = DIR_UP; // 0 ⇒ 反射 4（DOWN）
            e.ai.target_x = -1;
            e.ai.target_y = 0;
        }
        soccer_ball_run(&mut world, id);
        {
            let e = world.entities.get(id).unwrap();
            assert_eq!(e.ai.dir, DIR_DOWN, "Dir 0 ⇒ 4");
            assert_eq!(
                (e.ai.target_x, e.ai.target_y),
                (10, 13),
                "GetNextPosition(CurrX, CurrY, Dir, N550=3)"
            );
            assert_eq!(e.n550, 3, "未到达 ⇒ 不清零");
        }

        let id2 = spawn_monster(&mut world, 120, 20, 20, 100);
        {
            let e = world.entities.get_mut(id2).unwrap();
            e.n550 = 0;
            e.ai.target_x = 99;
        }
        soccer_ball_run(&mut world, id2);
        assert_eq!(
            world.entities.get(id2).unwrap().ai.target_x,
            -1,
            "N550<=0 ⇒ TargetX=-1"
        );
    }

    /// R36：`SoccerBall.Struck` **完全覆写** `AnimalObject.Struck`：只抽一次 `Random(4)`、
    /// `N550 = MIN(20, N550+Random(4)+4)`、`Dir = hiter.Dir`、写 `TargetX/Y`；
    /// **不写** `StruckTick`、不加 `AttackTick`。经 [`animal_struck`] 的 race 120 转派验证。
    ///
    /// 牙：去掉 `animal_struck` 的 race 120 分支 ⇒ `StruckTick` 非 0 ⇒ 红；
    /// 去掉 `MIN(20, …)` / 抽数位置错 ⇒ 红。
    #[test]
    fn soccer_ball_struck_draws_random4_and_caps_n550() {
        let mut world = World::new(40, 40);
        world.clock_ms = 5000;
        let id = spawn_monster(&mut world, 120, 10, 10, 100);
        let hitter = spawn_player(&mut world, 11, 10, 100);
        world.entities.get_mut(hitter).unwrap().ai.dir = DIR_RIGHT;
        {
            let e = world.entities.get_mut(id).unwrap();
            e.n550 = 19;
            e.ai.attack_tick = 777;
            e.ai.struck_tick = 0;
        }
        world.rng = RandomNumber::with_seed(3);
        let mut expect = RandomNumber::with_seed(3);
        let _draw = expect.random_below(4);
        animal_struck(&mut world, id, hitter);
        stream_eq(&world, &expect, "只抽一次 Random(4)");
        let e = world.entities.get(id).unwrap();
        assert_eq!(e.n550, 20, "MIN(20, 19 + Random(4) + 4)");
        assert_eq!(e.ai.dir, DIR_RIGHT, "Dir = hiter.Dir");
        assert_eq!(
            (e.ai.target_x, e.ai.target_y),
            (30, 10),
            "GetNextPosition(..., N550=20) 向右 20 格"
        );
        assert_eq!(e.ai.struck_tick, 0, "覆写版不写 StruckTick");
        assert_eq!(e.ai.attack_tick, 777, "覆写版不加 AttackTick");
    }

    /// R36（复验 Minor 2）：`BaseObject.Run` 的 `SuperMan` 段（`BaseObject.Base.cs:32-36`）
    /// 在消息泵之后、`!Death` 之前回满 HP/MP。race 120 `SoccerBall` ctor 置 `SuperMan=true`
    /// （数据可达）⇒ 每 `Run` 回满；普通怪不走该支。
    ///
    /// 牙：删掉 `base_object_run_monster` 的 `SuperMan` 段 ⇒ 第一条断言红。
    #[test]
    fn soccer_ball_super_man_refills_hp_each_base_run() {
        let mut world = World::new(40, 40);
        world.clock_ms = 1000;
        let id = spawn_monster(&mut world, 120, 10, 10, 100);
        assert!(
            world.entities.get(id).unwrap().super_man,
            "ctor SuperMan=true"
        );
        {
            let e = world.entities.get_mut(id).unwrap();
            e.hp = 3;
            e.mp = 1;
        }
        base_object_run_monster(&mut world, id);
        let e = world.entities.get(id).unwrap();
        assert_eq!((e.hp, e.mp), (e.max_hp, e.max_mp), "SuperMan ⇒ HP/MP 回满");
        // 对照：普通怪（race 50）同样的低 HP 不进 SuperMan 支。
        let n = spawn_monster(&mut world, 50, 20, 20, 100);
        world.entities.get_mut(n).unwrap().hp = 3;
        base_object_run_monster(&mut world, n);
        assert_eq!(
            world.entities.get(n).unwrap().hp,
            3,
            "非 SuperMan ⇒ 恢复段未到点（HealthTick=50 < 300），不一次回满"
        );
    }

    // ------------------------------ R33 SandMobObject ------------------------------

    /// R33：`SandMobObject.Run` 的三种 `base.Run()` pass 数 + `_mDwAppearStart` 写点。
    ///
    /// - 非隐藏 + 邻接目标（`AttackTarget()` 命中）⇒ `AnimalTwice`（体内一次 + 函数尾一次）；
    /// - 目标两轴均 > 15 ⇒ `ComeDown(); return;` ⇒ `None`（0 次）且 `FixedHideMode=true`；
    /// - `WalkTick` 门未到 ⇒ `Animal`（1 次）；
    /// - `FixedHideMode` 且 `HP > MaxHP/20` 且 `CheckComeOut()` ⇒ `appear_start_tick = now`。
    ///
    /// 牙：命中支返回 `Animal`（少一次 pass）/ 目标远支返回 `Animal` / 写错阈值 ⇒ 红。
    #[test]
    fn sand_mob_run_reports_base_pass_count_and_appear_start() {
        // AnimalTwice
        let mut world = World::new(40, 40);
        world.clock_ms = 3000;
        let id = spawn_monster(&mut world, 132, 10, 10, 100);
        let target = spawn_player(&mut world, 11, 10, 100);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.walk_tick = 0;
            e.ai.attack_tick = 0;
            e.ai.next_hit_time = 2000;
            e.ai.target_creature = Some(target);
        }
        assert_eq!(
            sand_mob_object_run(&mut world, id),
            ClassRunBase::AnimalTwice,
            "AttackTarget() 命中 ⇒ 双 base pass"
        );

        // None（目标两轴均 >15）
        let mut world = World::new(60, 60);
        world.clock_ms = 3000;
        let id = spawn_monster(&mut world, 132, 10, 10, 100);
        let far = spawn_player(&mut world, 30, 30, 100);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = false;
            e.ai.walk_tick = 0;
            e.ai.target_creature = Some(far);
        }
        assert_eq!(
            sand_mob_object_run(&mut world, id),
            ClassRunBase::None,
            "ComeDown(); return; ⇒ 0 次 base pass"
        );
        assert!(
            world.entities.get(id).unwrap().fixed_hide_mode,
            "ComeDown 置 FixedHideMode=true"
        );

        // Animal（WalkTick 未到点）
        let mut world = World::new(40, 40);
        world.clock_ms = 1000;
        let id = spawn_monster(&mut world, 132, 10, 10, 100);
        world.entities.get_mut(id).unwrap().ai.walk_tick = 1000;
        assert_eq!(
            sand_mob_object_run(&mut world, id),
            ClassRunBase::Animal,
            "WalkTick 门未过 ⇒ 仅函数尾一次"
        );

        // appear_start
        let mut world = World::new(40, 40);
        world.clock_ms = 5000;
        let id = spawn_monster(&mut world, 132, 10, 10, 100);
        let near = spawn_player(&mut world, 12, 12, 100);
        {
            let e = world.entities.get_mut(id).unwrap();
            e.fixed_hide_mode = true;
            e.ai.walk_tick = 0;
            e.visible_actors = vec![VisibleBaseObject {
                target: near,
                flag: VisibleFlag::Show,
            }];
        }
        assert_eq!(sand_mob_object_run(&mut world, id), ClassRunBase::Animal);
        assert_eq!(
            world.entities.get(id).unwrap().appear_start_tick,
            5000,
            "HP>MaxHP/20 && CheckComeOut() ⇒ _mDwAppearStart = now"
        );
    }

    // ------------------------------ R18 FireMonster ------------------------------

    /// R18：空地图一次 `Run` 生成 **9** 个火墙事件，坐标顺序与 `FireMonster.cs:31-64` 逐条一致，
    /// `Damage=10`、`ContinueTime=20000`、全部注册成功；第二 tick 不重复生成。
    ///
    /// 牙：漏一格 / 顺序错 / 用别的 damage / 不检查 `GetEvent` ⇒ 红。
    #[test]
    fn fire_monster_creates_nine_events_in_oracle_order() {
        let mut world = World::new(40, 40);
        world.clock_ms = 1000;
        let id = spawn_monster(&mut world, 214, 10, 10, 100);
        fire_monster_run(&mut world, id);
        let coords: Vec<(i32, i32)> = world.fire_burn_events.iter().map(|e| (e.x, e.y)).collect();
        assert_eq!(
            coords,
            vec![
                (10, 9),
                (10, 8),
                (9, 10),
                (8, 10),
                (10, 10),
                (11, 10),
                (12, 10),
                (10, 11),
                (10, 12)
            ],
            "9 格固定顺序"
        );
        assert!(
            world
                .fire_burn_events
                .iter()
                .all(|e| e.damage == FIRE_MONSTER_DAMAGE && e.continue_ms == FIRE_MONSTER_TIME_MS),
            "Damage=10 / FireTime=20000"
        );
        assert_eq!(
            world
                .fire_burn_events
                .iter()
                .filter(|e| e.registered)
                .count(),
            9,
            "9 格全部注册成功"
        );
        fire_monster_run(&mut world, id);
        assert_eq!(
            world.fire_burn_events.len(),
            9,
            "已有事件的格不再新建（GetEvent != null）"
        );
    }

    /// R18：`CanMove()` 为假（Death）⇒ 不生成任何事件、不索敌。
    #[test]
    fn fire_monster_run_is_gated_by_can_move() {
        let mut world = World::new(40, 40);
        world.clock_ms = 20_000;
        let id = spawn_monster(&mut world, 214, 10, 10, 100);
        world.entities.get_mut(id).unwrap().death = true;
        fire_monster_run(&mut world, id);
        assert!(world.fire_burn_events.is_empty());
        assert_eq!(world.entities.get(id).unwrap().ai.search_enemy_tick, 0);
    }

    // ------------------------------ R32 RunAway（死类） ------------------------------

    /// R32：死类 `RunAway.Run` 的 `time1` 局部变量 bug（`now > 5000` 恒真）+ 两类 `RandomByte`。
    ///
    /// 造景：act (2,2) `Dir=RIGHT` ⇒ `GetFrontPosition`=(3,2) ⇒ `SpaceMove(...,1,0)`（空格可走 ⇒
    /// 0 抽数）；目标 `(2,1)` 在空间移动后与 `(1,0)` **两轴均差 1** ⇒ `RandomByte(4)`；
    /// 目标 `(2,3)` ⇒ `RandomByte(7)`；`now=4000 < 5000` ⇒ 不抽。
    ///
    /// 牙：把 `time1` 改成静态字段并真做 5s 节流 / 4 与 7 写反 ⇒ 红。
    #[test]
    fn run_away_dead_class_time1_bug_and_random_byte_choice() {
        let setup = |target_y: i32, clock: i64| {
            let mut world = World::new(30, 30);
            world.clock_ms = clock;
            let id = spawn_monster(&mut world, 50, 2, 2, 100);
            {
                let e = world.entities.get_mut(id).unwrap();
                e.hp = 50;
                e.max_hp = 100;
                e.ai.dir = DIR_RIGHT;
                e.ai.search_enemy_tick = clock;
            }
            let target = spawn_player(&mut world, 2, target_y, 100);
            world.entities.get_mut(id).unwrap().ai.target_creature = Some(target);
            world.rng = RandomNumber::with_seed(7);
            (world, id)
        };

        // 空间移动后 |Δx|=|Δy|=1 ⇒ RandomByte(4)
        let (mut world, id) = setup(1, 10_000);
        let mut expect = RandomNumber::with_seed(7);
        expect.random_byte(4);
        run_away_run(&mut world, id);
        stream_eq(&world, &expect, "两轴均 1 ⇒ RandomByte(4)");
        assert_eq!(
            world.entities.get(id).unwrap().ai.dir,
            RandomNumber::with_seed(7).random_byte(4),
            "Dir = WalkTo 的随机方向"
        );
        assert_eq!(
            (
                world.entities.get(id).unwrap().x,
                world.entities.get(id).unwrap().y
            ),
            (1, 0),
            "SpaceMove(MapName, nx-2, ny-2)"
        );
        assert!(
            world
                .pending_monster_ref_msgs
                .iter()
                .any(|f| f.ident == RM_SPACEMOVE_FIRE),
            "先发 RM_SPACEMOVE_FIRE"
        );

        // 非斜邻 ⇒ RandomByte(7)
        let (mut world, id) = setup(3, 10_000);
        let mut expect = RandomNumber::with_seed(7);
        expect.random_byte(7);
        run_away_run(&mut world, id);
        stream_eq(&world, &expect, "非两轴均 1 ⇒ RandomByte(7)");

        // now <= 5000 ⇒ 不抽（局部 time1=0 的 bug 语义）
        let (mut world, id) = setup(3, 4000);
        let before = world.rng.clone();
        run_away_run(&mut world, id);
        stream_eq(&world, &before, "now=4000 ⇒ 不出 WalkTo");
    }

    // ------------------------------ R19 FireballMonster ------------------------------

    struct FireballSetup {
        world: World,
        m: i32,
        target: i32,
        pose: i32,
    }

    /// 造景：怪 (10,10) `Dir=RIGHT`、`DC = 10|(10<<8)`、`SpellTick=0`、`NextHitTime=2000`；
    /// `TargetCret` = 玩家 (12,10)（`TargetX/Y` 同步），`GetPoseCreate()` = 前格玩家 (11,10)
    /// （`MAC = 5|(5<<8)`）。
    fn fireball_setup(clock: i64, anti_magic: u16) -> FireballSetup {
        let mut world = World::new(40, 40);
        world.clock_ms = clock;
        let m = spawn_monster(&mut world, 215, 10, 10, 1000);
        let target = spawn_player(&mut world, 12, 10, 500);
        let pose = spawn_player(&mut world, 11, 10, 500);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.dc = 10 | (10 << 8);
            e.anti_magic = anti_magic;
            e.spell_tick = 0;
            e.ai.next_hit_time = 2000;
            e.ai.target_creature = Some(target);
            // `SetTargetCreat` 等价：`TargetFocusTick = GetTickCount()`（`BaseObject.Base.cs:113`
            // 的 30s 过期门读它；直接写 `TargetCret` 时也必须同步，否则
            // `BaseObject.Run` 的清理段会把它清掉）。
            e.ai.target_focus_tick = clock;
            e.ai.target_x = 12;
            e.ai.target_y = 10;
            e.ai.dir = DIR_RIGHT;
        }
        world.entities.get_mut(pose).unwrap().mac = 5 | (5 << 8);
        FireballSetup {
            world,
            m,
            target,
            pose,
        }
    }

    /// R19-A5：`AntiMagic == 0` ⇒ **只抽** `Random(|Hi(DC)-Lo(DC)|+1)`（DC），
    /// 不进 `GetPoseCreate`/减免/伤害/帧块。
    ///
    /// 牙：删掉 `AntiMagic > 0` 硬门 ⇒ 多抽 + 伤害 + 帧 ⇒ 红。
    #[test]
    fn fireball_only_draws_dc_when_anti_magic_zero() {
        let mut s = fireball_setup(100_000, 0);
        let mut expect = s.world.rng.clone();
        expect.random_below(1); // DC 10..10
        fireball_monster_run(&mut s.world, s.m);
        stream_eq(&s.world, &expect, "仅 DC 一抽");
        assert!(
            s.world
                .pending_monster_ref_msgs
                .iter()
                .all(|f| f.ident != RM_SPELL && f.ident != RM_MAGICFIRE),
            "anti_magic=0 ⇒ 不出施法帧"
        );
        assert_eq!(s.world.entities.get(s.pose).unwrap().hp, 500, "无伤害");
    }

    /// R19-A1..A3：完整施法链——抽数次序 `DC → pose.MAC → pose.StruckDamage 耐久段`；
    /// pose 掉 5；两帧 `RM_SPELL(48,12,10,48)` / `RM_MAGICFIRE(0,0x3002,MakeLong,12,target)`；
    /// 600ms 自投递 `RM_DELAYMAGIC`（599ms 不投、600ms 投）。
    ///
    /// 牙：帧载荷/顺序错 / 抽数次序错 / 600ms 提前 ⇒ 红。
    #[test]
    fn fireball_full_cast_frames_damage_and_600ms_delay() {
        let mut s = fireball_setup(100_000, 1);
        let mut expect = s.world.rng.clone();
        expect.random_below(1); // DC
        expect.random_below(1); // pose MAC 5..5
        let mut pose_clone = s.world.entities.get(s.pose).unwrap().clone();
        crate::combat::struck_damage_durability(&mut expect, &mut pose_clone);
        fireball_monster_run(&mut s.world, s.m);
        stream_eq(&s.world, &expect, "DC → MAC → StruckDamage 耐久");
        assert_eq!(
            s.world.entities.get(s.pose).unwrap().hp,
            495,
            "10 - MAC 5 = 5 伤害"
        );
        let idents: Vec<u16> = s
            .world
            .pending_monster_ref_msgs
            .iter()
            .map(|f| f.ident)
            .collect();
        assert_eq!(idents, vec![RM_SPELL, RM_MAGICFIRE], "帧顺序");
        let sp = &s.world.pending_monster_ref_msgs[0];
        assert_eq!(
            (sp.w_param, sp.n_param1, sp.n_param2, sp.n_param3),
            (48, 12, 10, 48),
            "RM_SPELL(48, tx, ty, 48)"
        );
        let mf = &s.world.pending_monster_ref_msgs[1];
        assert_eq!(
            (mf.w_param, mf.n_param1, mf.n_param2, mf.n_param3),
            (0, 0x3002, make_long(12, 10), s.target),
            "RM_MAGICFIRE(0, MakeWord(2,48), MakeLong(tx,ty), targetId)"
        );

        assert!(
            s.world.deliver_self_delay_msgs().is_empty(),
            "100000 不得到点"
        );
        s.world.clock_ms += 599;
        assert!(
            s.world.deliver_self_delay_msgs().is_empty(),
            "599ms 不得到点"
        );
        s.world.clock_ms += 1;
        let outs = s.world.deliver_self_delay_msgs();
        assert_eq!(outs.len(), 1, "600ms 到点投递 RM_DELAYMAGIC");
        assert_eq!(outs[0].ident, crate::slave::RM_DELAYMAGIC);
        assert_eq!(outs[0].actor_id, s.m, "入的是怪自己的队列");
        assert_eq!(
            s.world.pending_mag_struck.len(),
            1,
            "到点后产生 RM_MAGSTRUCK"
        );
    }

    /// R19-A6：`MagCanHitTarget(TargetCret.CurrX, TargetCret.CurrY, TargetCret)` 的
    /// 「目标自身坐标」怪癖——目标在底边时 Down 步越界 ⇒ 整块跳过（**零抽数**）。
    ///
    /// 牙：把它「修成」攻方→目标视线 ⇒ 本用例红。
    #[test]
    fn fireball_mag_can_hit_uses_target_own_coords() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = spawn_monster(&mut world, 215, 10, 39, 1000);
        let target = spawn_player(&mut world, 12, 39, 500);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.dc = 10 | (10 << 8);
            e.anti_magic = 1;
            e.ai.target_creature = Some(target);
            e.ai.target_x = 12;
            e.ai.target_y = 39;
            e.ai.dir = DIR_RIGHT;
        }
        let before = world.rng.clone();
        fireball_monster_run(&mut world, m);
        stream_eq(
            &world,
            &before,
            "目标在底边 ⇒ MagCanHitTarget 为假 ⇒ 零抽数",
        );
        assert!(world.pending_monster_ref_msgs.is_empty());
    }

    /// R19-A7：距离门读的是 **`TargetX/TargetY` 字段**，不是 `TargetCret.CurrX/CurrY`。
    ///
    /// 牙：改成读目标实时坐标（12）⇒ 会抽 DC ⇒ 红。
    #[test]
    fn fireball_distance_gate_reads_target_xy_fields() {
        let mut s = fireball_setup(100_000, 1);
        s.world.entities.get_mut(s.m).unwrap().ai.target_x = 50;
        let before = s.world.rng.clone();
        fireball_monster_run(&mut s.world, s.m);
        stream_eq(&s.world, &before, "|TargetX-CurrX|=40 > 8 ⇒ 跳过");
        assert_eq!(s.world.entities.get(s.pose).unwrap().hp, 500);
    }

    /// R19-A4：`BreakHolySeizeMode()` 在**有目标**时无条件执行（伤害块全跳过也执行）。
    #[test]
    fn fireball_breaks_holy_seize_unconditionally() {
        let mut world = World::new(40, 40);
        world.clock_ms = 100_000;
        let m = spawn_monster(&mut world, 215, 10, 39, 1000);
        let target = spawn_player(&mut world, 12, 39, 500);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.ai.holy_seize = true;
            e.ai.target_creature = Some(target);
            e.ai.target_x = 12;
            e.ai.target_y = 39;
        }
        let before = world.rng.clone();
        fireball_monster_run(&mut world, m);
        stream_eq(&world, &before, "魔法块跳过 ⇒ 零抽数");
        assert!(
            !world.entities.get(m).unwrap().ai.holy_seize,
            "有目标 ⇒ BreakHolySeizeMode 无条件"
        );
    }

    /// R19-A8：`MagicMonster.Think` **没有** HolySeize 早退（`MonsterObject.Think` 有）。
    ///
    /// oracle 两个 `Think` 都是 `private` ⇒ 静态绑定：Pass 1（`MagicMonster.Run`）落
    /// `MagicMonster.Think`、Pass 2（尾部 `base.Run()` = `MonsterObject.Run`）落
    /// `MonsterObject.Think`。本用例只钉两个叶子函数本身；**分派**由
    /// `mb89_race215_think_dispatch_two_pass_matches_oracle` 走整链钉（牙：把 Pass 2 也接成
    /// 无门版 ⇒ 整链多抽一次 `RandomByte(8)` ⇒ 红）。
    #[test]
    fn magic_monster_think_has_no_holy_seize_gate() {
        // race 215：HolySeize 下仍进 DupMode 抽 RandomByte(8)。
        let mut world = World::new(40, 40);
        world.clock_ms = 10_000;
        let m = spawn_monster(&mut world, 215, 10, 10, 1000);
        {
            let e = world.entities.get_mut(m).unwrap();
            e.ai.holy_seize = true;
            e.ai.dup_mode = true;
            e.ai.think_tick = 0;
        }
        world.rng = RandomNumber::with_seed(11);
        let mut expect = RandomNumber::with_seed(11);
        expect.random_byte(8);
        let _ = magic_monster_think(&mut world, m);
        stream_eq(
            &world,
            &expect,
            "MagicMonster.Think 无 HolySeize 门 ⇒ 抽 RandomByte(8)",
        );

        // 对照：普通 MonsterObject.Think 在 HolySeize 下零抽数并返回 false。
        let mut world = World::new(40, 40);
        world.clock_ms = 10_000;
        let n = spawn_monster(&mut world, 50, 10, 10, 1000);
        {
            let e = world.entities.get_mut(n).unwrap();
            e.ai.holy_seize = true;
            e.ai.dup_mode = true;
            e.ai.think_tick = 0;
        }
        let before = world.rng.clone();
        assert!(!think(&mut world, n));
        stream_eq(&world, &before, "MonsterObject.Think 的 HolySeize 门");
    }

    /// R19-A8（**集成判据**）：race 215 经 `monster_ai_tick` 的整链（Pass 1 → Pass 2）分派。
    ///
    /// `FireballMonster.Run` → `MagicMonster.Run`（Pass 1，`MagicMonster.Think` **无**门）→
    /// 尾部 `base.Run()` = `MonsterObject.Run`（Pass 2，`MonsterObject.Think` **有**门）。
    /// 造景：HolySeize + DupMode + 八邻全封 ⇒ Pass 1 仍抽一次 `RandomByte(8)`（移动失败 ⇒
    /// `DupMode` 保持 true）；Pass 2 必须被 HolySeize 门挡住 ⇒ **总抽数恰好 1**。
    ///
    /// 牙：Pass 2 错接无门版（旧实现的 race 215 分派）⇒ 抽 2 次 ⇒ 红；
    /// Pass 1 错接有门版 ⇒ 抽 0 次 ⇒ 红；**删掉 Pass 2 的 `monster_run(...)` 整行** ⇒
    /// 尾部 `CrazyMode` 到期不被清除 ⇒ `crazy_mode` 断言红（该清除只由 Pass 2 产生：
    /// `MagicMonster.Run` 不调该段，`animal_run`/`base_object_run_monster` 也不调）。
    #[test]
    fn mb89_race215_think_dispatch_two_pass_matches_oracle() {
        let mut world = World::new(40, 40);
        world.clock_ms = 10_000;
        let now = world.clock_ms;
        let m = spawn_monster(&mut world, 215, 10, 10, 1000);
        // 八邻全封 ⇒ Pass 1 的 `WalkTo(RandomByte(8))` 必失败 ⇒ `DupMode` 保持 true。
        for (dx, dy) in [
            (-1, -1),
            (0, -1),
            (1, -1),
            (-1, 0),
            (1, 0),
            (-1, 1),
            (0, 1),
            (1, 1),
        ] {
            assert!(world.maps[0].set_map_xy_flag(10 + dx, 10 + dy, false));
        }
        {
            let e = world.entities.get_mut(m).unwrap();
            e.ai.holy_seize = true;
            e.ai.holy_seize_tick = now; // 不让尾段 `animal_run` 到期清除
            e.ai.dup_mode = true;
            e.ai.think_tick = 0; // 3s 刷新段照跑（不改变 DupMode）
            e.ai.walk_tick = now; // WalkSpeed 门未到 ⇒ Pass 1/2 都不进 `Wondering`

            // **只由 Pass 2 产生**的可观测：`MonsterObject.Run` 尾的 `CrazyMode` 到期
            // （`monster_run` 的两个早退支与函数尾各调一次 `crazy_mode_expiry`）。
            // `MagicMonster.Run`（Pass 1）不调本段，`animal_run`/`base_object_run_monster` 也不调。
            e.ai.crazy_mode = true;
            e.ai.crazy_mode_tick = 0;
            e.ai.crazy_mode_interval = 10; // now=10_000 ⇒ (now - 0) > 10 ⇒ 到期
        }
        world.rng = RandomNumber::with_seed(11);
        let mut expect = RandomNumber::with_seed(11);
        expect.random_byte(8); // Pass 1 的唯一一抽
        monster_ai_tick(&mut world);
        stream_eq(&world, &expect, "Pass 1 无门抽一次 / Pass 2 有门零抽");
        let e = world.entities.get(m).unwrap();
        assert!(e.ai.dup_mode, "Pass 1 移动失败 ⇒ DupMode 保持 true");
        assert!(e.ai.holy_seize, "HolySeize 未被清除");
        assert_eq!((e.x, e.y), (10, 10), "四面被墙 ⇒ 原地不动");
        assert!(
            !e.ai.crazy_mode,
            "Pass 2 尾段 CrazyMode 到期必须清除（该清除只由 Pass 2 产生）"
        );
    }

    /// R19-A9（复验 Minor 1）：`baseObject.GetMagStruckDamage(this, nPower)`（`:32`）的
    /// undead 加项读**接收者（pose）**的 `AddAbil.UndeadPower`，不是攻方（本怪）的。
    ///
    /// 造景同 `fireball_full_cast_*`，另置 pose `LA_UNDEAD` + `UndeadPower=7`、本怪
    /// `undead_power=100`：DC=10、MAC=5、undead=7 ⇒ 伤害 12（旧实现会加 100 ⇒ 105）。
    ///
    /// 牙：把 undead 取回攻方 ⇒ 本用例红。
    #[test]
    fn fireball_mag_struck_undead_reads_victim_not_attacker() {
        let mut s = fireball_setup(100_000, 1);
        {
            let p = s.world.entities.get_mut(s.pose).unwrap();
            p.life_attrib = 1; // `Grobal2.LA_UNDEAD`
            p.undead_power = 7;
        }
        s.world.entities.get_mut(s.m).unwrap().undead_power = 100;
        fireball_monster_run(&mut s.world, s.m);
        assert_eq!(
            s.world.entities.get(s.pose).unwrap().hp,
            500 - 12,
            "只加接收者（pose）的 UndeadPower=7；攻方的 100 不参与"
        );
    }

    /// R19 §4.1.2 第 6 条：`SpellTick` 与 `BaseObject.Run` 的恢复累加器同名共用 ⇒ 同 tick 的
    /// `base.Run()` 把它归零 ⇒ 下一 tick 必然到点（冷却实际只有一 tick）。
    ///
    /// 牙：把施法冷却改到独立字段 ⇒ 第二次不同 tick 不再出帧 ⇒ 红。
    #[test]
    fn spell_tick_shared_accumulator_makes_cast_due_every_tick() {
        let mut s = fireball_setup(100_000, 1);
        fireball_monster_run(&mut s.world, s.m);
        assert_eq!(s.world.entities.get(s.m).unwrap().spell_tick, 100_000);
        base_object_run_monster(&mut s.world, s.m);
        assert_eq!(
            s.world.entities.get(s.m).unwrap().spell_tick,
            0,
            "BaseObject.Run 的恢复段把 SpellTick 归零（与 ctor/施法同名共用）"
        );
        fireball_monster_run(&mut s.world, s.m);
        assert_eq!(
            s.world
                .pending_monster_ref_msgs
                .iter()
                .filter(|f| f.ident == RM_SPELL)
                .count(),
            2,
            "下一 tick 仍然到点"
        );
    }

    // ------------------------------ 派发表 ------------------------------

    /// MB-9：`FireballMonster` ctor 的 `SpellTick = GetTickCount()` 必须落到实体上
    /// （`FireballMonster.cs:10`；`BaseObject.cs:449` 的 0 被覆盖）——否则首个 tick 的
    /// `now - SpellTick > NextHitTime` 门会提前放行。
    ///
    /// 牙：把 215 的 ctor 常量改回缺省 0 ⇒ 红。
    #[test]
    fn fireball_ctor_sets_spell_tick_to_now() {
        let mut world = World::new(40, 40);
        world.clock_ms = 1234;
        let id = spawn_monster(&mut world, 215, 10, 10, 1000);
        assert_eq!(
            world.entities.get(id).unwrap().spell_tick,
            1234,
            "FireballMonster ctor SpellTick = GetTickCount()"
        );
    }

    /// MB-8/MB-9 的 race → `base.Run()` 链映射（MB-0 口径的分派表牙）。
    #[test]
    fn monster_class_run_dispatch_covers_mb8_mb9() {
        let mut world = World::new(60, 60);
        world.clock_ms = 1000;
        for (i, (race, want)) in [
            (100i16, ClassRunBase::Monster),
            (120, ClassRunBase::Animal),
            (132, ClassRunBase::Animal),
            (214, ClassRunBase::Monster),
            (215, ClassRunBase::MagicMonster),
        ]
        .into_iter()
        .enumerate()
        {
            let id = spawn_monster(&mut world, race, 5, 5 + i as i32 * 2, 100);
            assert_eq!(
                monster_class_run(&mut world, id),
                Some(want),
                "race {race} 的 base.Run 链"
            );
        }
    }
}
