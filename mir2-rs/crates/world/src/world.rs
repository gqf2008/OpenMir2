//! 世界 tick 循环 —— 对齐 C# `GameSvr/Word/WorldServer.cs::ProcessHumans` 的 **200ms** 节拍
//!（`WorldServer.cs:487`：`if ((HUtil32.GetTickCount() - ProcessLoadPlayTick) > 200)`）。
//!
//! 本骨架的边界（与设计文档 §4.2 一致）：
//! - **单线程串行**：`World` 只在创建它的线程上推进；`tick()` 每次校验线程 id（判据③）；
//! - **确定性**：时钟是**虚拟时钟**（`clock_ms`，每 tick +200），模拟逻辑不读墙钟；
//!   实体处理顺序 = 槽位顺序（= 插入顺序），命令按 FIFO 处理；随机源是**同种子**的
//!   `RandomNumber`（与 C# 侧按同一调用顺序消费）；
//! - 不实现 M1 的边界服务（登录/网关/DBSrv），只做世界内的 tick/实体/地图/AOI/会话/战斗。

use std::collections::{HashMap, VecDeque};
use std::thread::ThreadId;

use mir2_data::models::{MonsterDropItem, StdItem, STRING_GOLD_NAME};
use mir2_formula::drop::{ItemNumberCounter, VecItemCatalog};
use mir2_shared::rng::RandomNumber;
use mir2_world_bevy::{advance_clock_system, TickClock, TickOwnerThread, TickRunner};

use crate::aoi::{search_view_range, search_view_range_death};
use crate::buff::{
    BuffSysMsg, BuffSystem, BuffTickOutcome, BuffType, BUFF_EPOCH_MS, BUFF_PASSES_PER_TICK,
    BUFF_PASS_INTERVAL_MS, MAX_STATUS_ATTRIBUTE,
};
use crate::combat::{
    attack_dir_grouped_poison, generate_drops, swing_power, win_exp, AttackOutcome,
    AttackPoisonState, AttackPowerMode, CombatConfig,
};
use crate::entity::{Entity, EntityStore};
use crate::map::{CellType, FloorItemKind, MapGrid, MapRouteSpec};
use crate::session::SessionRegistry;

/// C# `ProcessHumans` 的节拍（毫秒）。
pub const TICK_INTERVAL_MS: i64 = 200;

/// `MagicConst.SKILL_HANGMAJINBUB`（`src/Modules/SystemModule/Const/MagicConst.cs:60`）——幽灵盾。
/// 效果体走 `MagMakeDefenceArea(..., btState = 1)` ⇒ `MagDefenceUp`（`MagicManager.cs:352-357`）。
pub const SKILL_HANGMAJINBUB: u16 = 14;
/// `MagicConst.SKILL_DEJIWONHO`（`MagicConst.cs:64`）——神圣战甲术。
/// 效果体走 `MagMakeDefenceArea(..., btState = 0)` ⇒ `DefenceUp`（`MagicManager.cs:359-365`）。
pub const SKILL_DEJIWONHO: u16 = 15;
/// `MagicConst.SKILL_SHIELD`（`MagicConst.cs:128`）——魔法盾。
/// 效果体走 `MagBubbleDefenceUp` ⇒ `MagicShield`（`MagicManager.cs:472-478`）。
pub const SKILL_SHIELD: u16 = 31;

/// 世界 tick 里排队执行的命令。
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// 进入世界（会话建立后）
    Enter { actor_id: i32 },
    /// 离开世界（掉线/小退）
    Leave { actor_id: i32 },
    /// 走一步（服务端做格子校验后更新坐标）
    Walk { actor_id: i32, dx: i32, dy: i32 },
    /// 攻击（`_Attack` 一次结算 + 命中者登记）——`combat.json` 场景用它把 `_Attack` 的 RNG 面纳入 golden
    Attack { actor_id: i32, target_id: i32 },
    /// 脚本化击杀：直接把目标 HP 置 0（最小切片的击杀触发；战斗结算不在本切片）
    Kill { target_id: i32 },
    /// 生成怪物（掉落在地图生成时按 C# 时机预算）
    SpawnMonster {
        id: i32,
        name: String,
        x: i32,
        y: i32,
        map_id: usize,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        /// `HitPoint` / `SpeedPoint`（命中判定用；缺省照 `BaseObject` 构造 = 5 / 15）
        hit_point: u8,
        speed_point: u8,
        drop_list: Vec<MonsterDropItem>,
    },
    /// **出站接线片 W2**：被推（`BaseObject.CharPushed`，`src/M2Server/Actor/BaseObject.cs:854-901`）。
    /// `dir` = `nDir`（推进方向）、`count` = `nPushCount`。tick 里逐格推进并记 [`crate::action::PushEvent`]。
    Push { actor_id: i32, dir: u8, count: u8 },
    /// **出站接线片 W2**：野蛮冲撞（`PlayObject.DoMotaebo` 的**无正前目标**支，
    /// `src/M2Server/Player/PlayObject.Base.cs:1443-1467`）。`dir` = `nDir`、`steps` = `_MAX(2, level+1)`。
    Rush { actor_id: i32, dir: u8, steps: i32 },
    /// **出站接线片 W2**：宰杀（`PlayObject.ClientGetButchItem` 的转发，`PlayObject.Operate.cs:702-750`）。
    /// `dir` = 已解析的 `Dir`（目标 2 格内且有效 ⇒ `btDir`，否则当前朝向；解析在宿主 `CM_BUTCH` 门面）。
    Butch { actor_id: i32, dir: u8 },
}

/// 一个 tick 的处理记录（判据①：断言"谁、按什么顺序"被处理）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TickRecord {
    pub tick_seq: u64,
    pub clock_ms: i64,
    /// 按处理顺序记录的 `(actor_id, 结果)`；`result` = 移动是否成功 / 进出是否成功 / 攻击是否命中
    pub processed: Vec<(i32, bool)>,
    /// 本 tick 执行的 AOI 刷新顺序（按地图、再按槽位顺序）
    pub aoi_order: Vec<i32>,
    /// 本 tick 的攻击结算（对拍用）
    pub attacks: Vec<(i32, i32, AttackOutcome)>,
    /// 本 tick 的死亡结算：`(死者 id, 掉落物品数, 金币, 经验归属者, 获得经验)`
    /// —— 归属者取 C# `ExpHitter`（不是 `LastHiter`），见 `Entity::exp_hitter`
    pub deaths: Vec<(i32, usize, i32, Option<i32>, i32)>,
    /// 本 tick 的**经验归属**（oracle `BaseObject.cs:2429 KillFunc` → `RM_PLAYERKILLMONSTER`
    /// → `PlayObject.cs:1632 KillTargetTrigger`）：
    /// `(获得者 id, 本次获得, 是否进入升级块, 升级后等级)`。
    /// 与 `deaths` 的 5 元组不同，这里把 `ExpHitter.Master`（宠物/下属击杀给主人）与
    /// `ExpHitter` 本人**都**列出——oracle 对二者**各发一条** `RM_PLAYERKILLMONSTER`
    /// （`BaseObject.cs:2434-2444`）。供宿主出 `SM_WINEXP(44)`；第 3 位 = `GetExp`
    /// （`PlayObject.Base.cs:149-165`）是否进了 `Abil.Exp >= Abil.MaxExp` 块（满级也置位），
    /// 宿主据此出升级链（`SM_LEVELUP(45)`/`SM_ABILITY(52)`/`SM_SUBABILITY(752)`/`SM_HEALTHSPELLCHANGED(53)`）。
    pub exp_awards: Vec<(i32, i32, bool, u8)>,
    /// **M3⑤-实现 2 第②片**：本 tick 的商人 `ProcessMerchants` 结果。
    pub merchants_run: Vec<i32>,
    /// 本 tick `Merchant.Run` 的 `TurnTo`：`(merchant_id, dir)`（`RM_TURN`；帧面归 ⑤-4）。
    pub merchant_turns: Vec<(i32, u8)>,
    /// 本 tick 商人 `RM_HIT` 的 id。
    pub merchant_hits: Vec<i32>,
    /// 本 tick 商人 `MapRandomMove` 的 id。
    pub merchant_space_moves: Vec<i32>,
    /// 本 tick 被 Ghost 60s 门移除的商人 id。
    pub merchant_removed: Vec<i32>,
    /// **世界 tick 扩容首批（T3）**：本 tick 的固定 NPC `ProcessNpcs` 结果。
    pub npcs_run: Vec<i32>,
    /// 本 tick NPC `Run` 的 `TurnTo`：`(npc_id, dir)`（`RM_TURN`；帧面归 B-4）。
    pub npc_turns: Vec<(i32, u8)>,
    /// 本 tick NPC `RM_HIT` 的 id。
    pub npc_hits: Vec<i32>,
    /// 本 tick 被 Ghost 60s 门移除的 NPC id。
    pub npc_removed: Vec<i32>,
    /// **M3⑦-实现 2 F-1**：本 tick 死亡触发的组队帧。
    ///
    /// oracle `PlayObject.Die()`（`src/M2Server/Player/PlayObject.cs:1964-1978`）在置 `Death` 之后、
    /// `base.Die()` 之前判 `if (GroupOwner != 0) groupOwnerPlay.DelMember(this)`（`:1975-1977`）
    /// ⇒ 死亡者立即退组（团长死亡 ⇒ 整组散）。事件序 = [`crate::party::PartyEvent`] 的
    /// 「立即帧 → `MsgQueue` 出队序」，由宿主（`bins/game-svr`）逐条回帧。
    pub party_events: Vec<crate::party::PartyEvent>,
    /// **M3⑨ 主人死/幽灵自投递**：本 tick **到点投递**的自身延时消息（oracle `PlayObject.Operate`
    /// 的 `RM_MASTERDIEMUTINY` / `RM_MASTERDIEGHOST` 支）。
    ///
    /// 帧面（`RefNameColor` ⇒ `RM_CHANGENAMECOLOR` / `RefShowName` ⇒ `RM_USERNAME`）不在
    /// `crates/world` 层（见 `crate::slave` 模块头），宿主据本记录出帧或登记 `not_verified`。
    pub slave_self_deliveries: Vec<crate::slave::SelfDelayOutcome>,
    /// **出站接线片 W2**：本 tick 的 `RM_PUSH` 族（`SM_BACKSTEP(9)`）——`CharPushed` 每推进一格一条，保序。
    pub pushes: Vec<crate::action::PushEvent>,
    /// **出站接线片 W2**：本 tick 的 `RM_RUSH`/`RM_RUSHKUNG` 族（`SM_RUSH(6)`/`SM_RUSHKUNG(7)`）——
    /// `DoMotaebo` 每成功推进一格一条 `Rush`，末尾若有则追加一条 `RushKung`，保序。
    pub rushes: Vec<crate::action::RushEvent>,
    /// **出站接线片 W2**：本 tick 的 `RM_BUTCH` 族（`SM_BUTCH(637)`）——每次 `ClientGetButchItem` 一条。
    pub butches: Vec<crate::action::ButchEvent>,
    /// **orphan 编码器接线首批**：本 tick 各观察者「离开视野」的对象
    /// （`(观察者 id, 离开者 id)`，保序 = AOI 移除顺序）。
    ///
    /// 对应 oracle `PlayObject.Run()` 的 `VisibleFlag.Hidden` 支（`src/M2Server/Player/PlayObject.cs:3214-3222`）：
    /// 观察者 `SendMsg(baseObject, RM_DISAPPEAR, 0, 0, 0, 0)` 走的是 **6 参**重载
    /// `SendMsg(IActor baseObject, int wIdent, …)`（`BaseObject.Message.cs:98`）——它把
    /// `SendMessage.ActorId` 置为 **`baseObject.ActorId` = 离开者**的 ActorId（**不是** `this`）；
    /// 帧经**观察者自己**的消息队列发出 ⇒ 收件人 = 观察者，`Recog` = 离开者。
    /// 由 [`crate::aoi::search_view_range`] 的返回值填充；帧面由宿主（`bins/game-svr`）出。
    pub hides: Vec<(i32, i32)>,
    /// **车道 B1（buff 系统）**：本 tick `StatusChanged()` 的结果 `(actor_id, 新 CharStatus)`，保序。
    ///
    /// oracle `ActorBuffSystem.Operate` 的 `boChg` 支（`ActorBuffSystem.cs:220-224`）⇒
    /// `BaseObject.StatusChanged()`（`BaseObject.cs:1517-1520`）⇒ `RM_CHARSTATUSCHANGED(10139)`
    /// ⇒ 宿主出 `SM_CHARSTATUSCHANGED(657)`（`PlayObject.Message.cs:1845-1846`）。
    pub buff_status_changes: Vec<(i32, i32)>,
    /// 本 tick buff 递减段对玩家发的 `SysMsg`：`(actor_id, msg)`。帧面（聊天/提示行）归接线层。
    pub buff_sysmsgs: Vec<(i32, BuffSysMsg)>,
    /// 本 tick buff 的 `boNeedRecalc` 触发的 `RecalcAbilitys()`：actor_id
    /// （`RM_ABILITY` 帧面归能力层接线，deferred）。
    pub buff_recalcs: Vec<i32>,
    /// 本 tick buff 毒伤段每次 `HealthSpellChanged()`：`(actor_id, hp_after, mp_after)`，保序。
    pub buff_health_changes: Vec<(i32, u16, u16)>,
}

/// **L1 TransMap 触发**：`World::step_with_route` 的结果（供宿主排帧序）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StepOutcome {
    /// 位移是否被接受。`false` = 底层 [`World::try_step_now`] 失败**或**落点格命中 `MapRoute` 且
    /// `EnterAnotherMap` 门失败（已回滚到走前格，宿主不得发移动帧）。
    pub moved: bool,
    /// 是否发生换图（移动后落点格命中 `CellType::MapRoute` 且 `EnterAnotherMap` 门通过）。
    pub changed_map: bool,
}

/// [`World::route_enter`] 的结果：区分「无 route / 被门跳过」「成功换图」「`EnterAnotherMap` 失败」。
///
/// 对应 oracle `PlayObject.Walk` 覆写的 `result` 布尔（`PlayObject.cs:1851/1878`）：`Skipped`/`Switched`
/// ⇒ `result == true`（发移动帧）；`Failed` ⇒ `result == false`（上游回滚、不发移动帧）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RouteOutcome {
    /// 当前格无 `MapRoute`，或命中但被门跳过（非玩家 / `NEEDHOLE` 等）⇒ 无换图、不回滚。
    Skipped,
    /// 至少一条 `MapRoute` 的 `EnterAnotherMap` 成功 ⇒ 换图（终落点 = 最后一条成功者）。
    Switched,
    /// 至少一条 `MapRoute` 的 `EnterAnotherMap` 失败 ⇒ 调用方回滚、不发移动帧。
    Failed,
}

pub struct World {
    /// 地图表（下标 = `map_id`）；`World::new` 建一张 0 号图。
    pub maps: Vec<MapGrid>,
    pub entities: EntityStore,
    pub sessions: SessionRegistry,
    /// 虚拟时钟（毫秒），每 tick 前进 `TICK_INTERVAL_MS`
    pub clock_ms: i64,
    pub tick_seq: u64,
    /// 随机源（同种子 = 与 C# 可对拍）
    pub rng: RandomNumber,
    /// **M3②③ 活栈隔离口径（owner 2026-10-12 21:15）**：攻击路径**专用**随机流。
    ///
    /// 与 `rng` 同源但独立推进：种子 = [`mir2_shared::rng::attack_seed`]`(MIR2_RNG_SEED)`。
    /// 仅在 [`World::attack_isolation_enabled`] 时被攻击路径取数；关闭时（缺省）**一次都不会被取数**，
    /// 行为与改动前逐字节相同。
    pub rng_attack: RandomNumber,
    /// 是否把攻击路径取数路由到 [`World::rng_attack`]。缺省 `false`（生产/既有 golden 路径不变）。
    isolate_attack: bool,
    /// 是否在 [`World::tick`] 里驱动怪物 AI（[`crate::monai::monster_ai_tick`]）。
    ///
    /// **缺省 `false`**：M2 的 golden 场景（`tests/parity/world/scenarios/*.json`）是「直接驱动
    /// `_Attack`」的合成探针（`csharp/WorldGolden`），不含 oracle 的 `ProcessMonsters` 线程，
    /// 因此**不能**默认跑 AI（否则每次 `CreateMonster` 后的第一只怪就会多抽 `Wondering`，
    /// 把 golden 的 RNG 序列整体移位）。接线层（`bins/game-svr` 的 sim 循环）在起世界时显式
    /// [`World::enable_monster_ai`]；`openmir2-m3-monster-ai-tick` 的宿主接线为后续分片。
    ai_enabled: bool,
    /// **M3⑤-实现 2 第②片**：是否在 [`World::tick`] 里驱动商人 tick
    /// （[`crate::World::merchant_tick`]，oracle `MerchantProcessor` 的单线程等价物）。
    ///
    /// **缺省 `false`**：合成 golden 场景不含 oracle 的商人 200ms 线程；由 `bins/game-svr` 的 sim
    /// 循环在起世界时 `enable_merchant_tick()`（生产路径）。
    merchant_enabled: bool,
    /// `MerchantPosition`（`WorldServer.Merchant.cs:66`）：本 pass 的起始索引。
    ///
    /// 本片丢掉墙钟预算（`NpcLimit`）分支 ⇒ 一趟扫到底、扫完归 0；保留字段与循环起点以与 oracle 同形
    /// （将来若恢复预算断点续跑，语义不变）。
    pub merchant_position: usize,
    /// 上一趟 `ProcessMerchants` 的 sim 时钟（200ms 节奏门）。
    pub merchant_pass_tick: i64,
    /// **世界 tick 扩容首批（T3）**：是否在 [`World::tick`] 里驱动固定 NPC 的 `ProcessNpcs`
    /// （[`crate::World::npc_tick`]）。
    ///
    /// **缺省 `false`**：合成 golden 场景不含 oracle 的 `MerchantProcessor` 200ms 线程；由
    /// `bins/game-svr` 的 sim 循环在起世界时 `enable_npc_tick()`（生产路径）。
    /// oracle `MerchantProcessor.ExecuteInternal`（`MerchantProcessor.cs:28-32`）**先**
    /// `ProcessNpcs()` **后** `ProcessMerchants()` ⇒ 本字段驱动的 `npc_tick` 在
    /// [`World::tick`] 里排在 `merchant_tick` **之前**（RNG 消费者次序，见 `World::tick` 注释）。
    npc_enabled: bool,
    /// `NpcPosition`（`WorldServer.Merchant.cs:120`）：本 pass 的起始索引。
    ///
    /// 本片丢掉墙钟预算（`NpcLimit`）分支 ⇒ 一趟扫到底、扫完归 0（照 [`World::merchant_position`]）。
    pub npc_position: usize,
    /// 上一趟 `ProcessNpcs` 的 sim 时钟（200ms 节奏门）。
    pub npc_pass_tick: i64,
    /// oracle `M2Share.GameTime`（`M2Share.cs:182`）——**世界级单值**的世界时钟桶值
    /// （`1`=白天 `2`=日落 `0`=日出 `3`=夜晚）。
    ///
    /// 由宿主按 `SystemProcessor.GetGameTime`（`SystemProcessor.cs:98-134`）的**本地小时桶**
    /// 口径经 [`World::refresh_game_time`] 周期刷新（原缺省 `0` = oracle `M2Share.GameTime` 初值）。
    pub game_time: u8,
    /// **车道 B1（buff 系统）**：精灵状态系统（oracle `M2Share.ActorBuffSystem` 等价物，见
    /// [`crate::buff`]）。
    pub buffs: BuffSystem,
    /// 是否在 [`World::tick`] 里驱动 buff pass（oracle `ActorBuffProcessor` 50ms `DoWork` 的
    /// 单线程等价物）。
    ///
    /// **缺省 `false`** ⇒ 既有 golden 逐字节不变；宿主 `bins/game-svr` 的 sim 循环
    /// `enable_buff_tick()`（生产路径）。每 world tick（200ms）跑 [`BUFF_PASSES_PER_TICK`] 个
    /// 50ms pass（U-A2 口径，见 [`crate::buff`] 模块头）。
    buff_enabled: bool,
    /// 上一 buff pass 的 sim 时钟（`clock_ms + BUFF_EPOCH_MS`；诊断/测试用）。
    pub buff_pass_tick: i64,
    /// `Config.PosionDecHealthTime`（`GameSvrConf.cs:893/1766` 缺省 2500；`ServerConf.cs:544`
    /// 读 `[Setup] PosionDecHealthTime`）——毒伤 DoT 的周期门（`ActorBuffSystem.cs:235`）。
    /// 宿主未解析该键 ⇒ 缺省 2500（= oracle 缺省；接线待 Envir 侧解析，登记）。
    pub poison_dec_health_time: i32,
    /// 物品目录（掉落生成按名查 `StdItem`）
    pub item_catalog: Vec<StdItem>,
    /// 怪物极品掉落几率（`Config.MonRandomAddValue`）
    pub mon_random_add_value: i32,
    /// 经验表（`Exps.conf` 的 `NeedExps`）
    pub need_exps: Vec<i32>,
    pub combat_config: CombatConfig,
    /// **M3⑦-实现 2 F-1**：死亡退组（oracle `Die()` 的 `DelMember`）所用配置。
    ///
    /// 由宿主在起世界时用与 4 条 `CM_*` 组队派发**同一份** [`crate::party::PartyConfig`] 灌入
    /// （缺省 = 常量默认值，与 `Server.conf` 缺键口径一致）。
    pub party_cfg: crate::party::PartyConfig,
    /// `SystemShare.Config.HealthFillTime` / `SpellFillTime`（`GameSvrConf.cs:1323-1324` 缺省
    /// `300` / `800`；`ServerConf.cs:152-153` 可覆盖）。
    ///
    /// `BaseObject.Run` 恢复段（`BaseObject.Base.cs:47/57`）的到点门；由接线层
    /// （`bins/game-svr` 的 sim 循环）用同一份配置灌入。合成 golden 场景不跑 [`World`] 的
    /// 怪物 AI（`ai_enabled=false`）⇒ 这两个值在既有对拍里不被消费。
    pub health_fill_time: i32,
    pub spell_fill_time: i32,
    /// C# `Config.ClearDropOnFloorItemTime`
    pub clear_drop_on_floor_item_time: i64,
    /// 每张地图的 `Envir.Flag.boEXPRATE`/`ExpRate`（`PlayObject.Base.cs:143-146`）：
    /// `None` = 该图无 `EXPRATE` 行（不加倍）；`Some(rate)` = 杀怪经验 × `rate/100`。
    /// 由接线层按 `MapRegistry::flag_of(map_id)` 逐图灌入（缺省全 `None` ⇒ 行为与改动前逐字节相同）。
    pub map_exp_rate: Vec<Option<i32>>,
    /// C# `M2Share.SpiritMutinyTick`（`M2Share.cs:185`）：神之祈祷的全服窗口到期时刻。
    ///
    /// `ProcessSpiritSuite`（`PlayObject.Base.cs:326`）置 `GetTickCount() + Config.SpiritMutinyTime`；
    /// `MonsterObject.CheckRoyalty`（`MonsterObject.cs:370-372`）读它：窗口未过期且
    /// `SlaveExpLevel < 5` ⇒ `MasterRoyaltyTick = 0`（立即叛变）。归 `openmir2-m3-pet-slave-lifecycle`（⑨-3）。
    /// 缺省 `0`（= 无窗口）⇒ 既有 golden 路径一字不改。
    pub spirit_mutiny_tick: i64,
    /// **M3⑨ 主人死/幽灵自投递（卡 `openmir2-m3-pet-slave-masterdie-wire`）**：是否在
    /// [`World::tick`] 里驱动「自身延时消息」队列（oracle `BaseObject.MsgQueue` +
    /// `BaseObject.SendSelfDelayMsg`）。
    ///
    /// **缺省 `false`**：合成 golden 场景不含 oracle 的玩家 `Run()` 消息段；生产宿主
    /// （`bins/game-svr` 的 sim 循环）起世界时用 [`World::enable_slave_self_delay`] 打开。
    /// 关闭时 `settle_deaths` 不入队、`tick` 不投递 ⇒ 既有对拍逐字节不变。
    slave_self_delay_enabled: bool,
    /// 自身延时消息队列（oracle `BaseObject.MsgQueue` 的延时子集，**插入序**）。
    ///
    /// oracle 的完整 `MsgQueue` 是「优先级 + 插入序」的优先队列（`SendSelfDelayMsg` 一律
    /// `MessagePriority.Lowest`）；本骨架只承载这一族自投递（`RM_MASTERDIEMUTINY` /
    /// `RM_MASTERDIEGHOST`）⇒ 按插入序扫描（登记：多产者/其他优先级的交织未建模）。
    pub self_delay_msgs: Vec<crate::slave::SelfDelayMsg>,
    /// `Server.conf` 的宝宝配置（`ProcessSlaveMutiny` 的 `Config.MasterDieMutiny*` 输入）；
    /// 由宿主 [`World::enable_slave_self_delay`] 灌入。缺省 = [`PetSlaveConfig::default`]。
    pub pet_slave_cfg: mir2_data::pet_slave::PetSlaveConfig,
    /// 帧内待处理命令（FIFO）
    pending: VecDeque<Command>,
    /// 判据③：tick 必须始终跑在同一个线程上
    owner_thread: Option<ThreadId>,
    /// S1 tick 骨架：时钟推进 + 线程断言（单线程 `Schedule`）。
    clock_runner: TickRunner,
    /// 掉落物品的 `MakeIndex` 计数器（`M2Share.GetItemNumber`）
    pub(crate) item_number: ItemNumberCounter,
    /// 最近一次 tick 的记录
    pub last_tick: TickRecord,
    /// AOI 分组的复用缓冲（避免每 tick 分配；P99 回归即由那次分配量出）
    by_map_scratch: Vec<Vec<i32>>,
    /// **W4 怪物 AI 攻击支**：本 tick 由 [`crate::monai::attack_target`]（怪物 AI）
    /// 发起的攻击结算。`World::tick` 在 AI 段之后转抄进 [`TickRecord::attacks`]，
    /// 使既有的 `drain_combat_downlink` ①（`SM_STRUCK` 受击广播）**无需宿主改动**
    /// 即覆盖「怪打玩家」（oracle `AnimalObject.Attack` → `_Attack` 走同一条
    /// `SendStruckDelayMsg(RM_REFMESSAGE) → SendRefMsg(RM_STRUCK)` 路径）。
    ///
    /// 只在 `ai_enabled` 时被写入 ⇒ 关闭时恒空、既有 golden 一字不改。
    pub(crate) pending_ai_attacks: Vec<(i32, i32, AttackOutcome)>,
    /// **片 A-5（buff 生产者接线）**：攻击命中中毒门（`PlayObject.Attack.cs:221`）的**攻/守双方**
    /// 运行态（`Paralysis` / `UnParalysis` / `AntiPoison`），按 actor id 挂世界侧。
    ///
    /// 与 [`World::buffs`] 同体例（`Entity` 里没有这三个字段的本仓，先挂世界表；由宿主
    /// `bins/game-svr` 在攻击派发前用 [`World::set_attack_poison`] 灌入）。**缺省空表** ⇒ 无任何
    /// actor 有麻痹态 ⇒ [`World::attack_once`] 的毒门短路（不抽），既有 golden 逐字节不变。
    pub attack_poison: HashMap<i32, AttackPoisonState>,
}

impl World {
    /// 单地图世界（与 M2 上半程的 `World::new` 语义一致）。
    pub fn new(width: i32, height: i32) -> Self {
        Self::with_maps(vec![(width, height)])
    }

    /// 多地图世界（切图用）；`map_id` = 下标。
    pub fn with_maps(sizes: Vec<(i32, i32)>) -> Self {
        let maps: Vec<MapGrid> = sizes.into_iter().map(|(w, h)| MapGrid::new(w, h)).collect();
        let maps_len = maps.len();
        // S1：实体存储是 bevy_ecs 内核；时钟/线程断言/顺序收集都走它的单线程骨架。
        let mut entities = EntityStore::new();
        entities
            .world_mut()
            .insert_resource(TickClock::new(TICK_INTERVAL_MS));
        entities
            .world_mut()
            .insert_resource(TickOwnerThread::default());
        let mut clock_runner = TickRunner::new();
        clock_runner.add_systems(advance_clock_system);
        let mut world = World {
            maps,
            entities,
            sessions: SessionRegistry::new(),
            clock_ms: 0,
            tick_seq: 0,
            rng: RandomNumber::with_seed(42),
            rng_attack: RandomNumber::with_seed(42),
            isolate_attack: false,
            ai_enabled: false,
            merchant_enabled: false,
            merchant_position: 0,
            merchant_pass_tick: i64::MIN,
            npc_enabled: false,
            npc_position: 0,
            npc_pass_tick: i64::MIN,
            game_time: 0,
            buffs: BuffSystem::new(),
            buff_enabled: false,
            buff_pass_tick: i64::MIN,
            poison_dec_health_time: 2500,
            item_catalog: Vec::new(),
            mon_random_add_value: 40,
            need_exps: vec![0; 1000],
            combat_config: CombatConfig::default(),
            party_cfg: crate::party::PartyConfig::default(),
            health_fill_time: 300,
            spell_fill_time: 800,
            clear_drop_on_floor_item_time: 5 * 60 * 1000,
            map_exp_rate: vec![None; maps_len],
            spirit_mutiny_tick: 0,
            slave_self_delay_enabled: false,
            self_delay_msgs: Vec::new(),
            pet_slave_cfg: mir2_data::pet_slave::PetSlaveConfig::default(),
            pending: VecDeque::new(),
            owner_thread: None,
            clock_runner,
            item_number: ItemNumberCounter::default(),
            last_tick: TickRecord::default(),
            by_map_scratch: Vec::new(),
            pending_ai_attacks: Vec::new(),
            attack_poison: HashMap::new(),
        };
        // **M3②③ 隔离口径**：`MIR2_RNG_ISOLATE_ATTACK` 开启时按 `MIR2_RNG_SEED` 派生专用流
        // （缺省未设 ⇒ `None`，`isolate_attack` 保持 `false`，一字不改）。读 env 的 fail-closed
        // 语义见 [`mir2_shared::rng::attack_isolation_seed_from_env`]。
        if let Some(seed) = mir2_shared::rng::attack_isolation_seed_from_env() {
            world.enable_attack_isolation(seed);
        }
        world
    }

    /// 显式开启攻击路径专用流（种子由调用方按两侧同口径给出）。
    ///
    /// 幂等：重复调用只重播专用流。测试与真机接线（经 [`World::with_maps`] 的环境入口）共用本方法。
    pub fn enable_attack_isolation(&mut self, attack_seed: i32) {
        self.rng_attack = RandomNumber::with_seed(attack_seed);
        self.isolate_attack = true;
    }

    /// 攻击路径专用流是否开启（隔离口径的真机自证）。
    #[must_use]
    pub fn attack_isolation_enabled(&self) -> bool {
        self.isolate_attack
    }

    /// 开启怪物 AI 驱动（缺省关闭；见 `World` 的 `ai_enabled` 字段说明）。
    ///
    /// 幂等：重复调用无副作用。生产路径由 `bins/game-svr` 的 sim 循环在起世界时调用；
    /// 合成 golden 场景保持关闭 ⇒ 既有 `tests/parity/world` 逐字节不变。
    pub fn enable_monster_ai(&mut self) {
        self.ai_enabled = true;
    }

    /// 怪物 AI 驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn monster_ai_enabled(&self) -> bool {
        self.ai_enabled
    }

    /// 设置 `BaseObject.Run` 恢复段的到点门（`SystemShare.Config.HealthFillTime`/`SpellFillTime`）。
    ///
    /// 生产宿主在起世界时用 `[Setup] HealthFillTime/SpellFillTime`（缺省 300/800）灌入；
    /// 合成 golden/单测不调 ⇒ 保持缺省，行为与改动前逐字节相同。
    pub fn set_recovery_fill_times(&mut self, health_fill_time: i32, spell_fill_time: i32) {
        self.health_fill_time = health_fill_time;
        self.spell_fill_time = spell_fill_time;
    }

    /// `ProcessSpiritSuite`（`PlayObject.Base.cs:322-330`）的尾：把全服宝宝叛变窗口
    /// `M2Share.SpiritMutinyTick` 置为 `now + Config.SpiritMutinyTime`。
    ///
    /// oracle 的完整 `ProcessSpiritSuite` 还要校验 `IsSpirit` + 拆卸祈祷套装物品
    /// （`UseItems` 的 `SpiritItem1..4`）——那部分归 `CM_SPELL`/物品面接线片；本函数只落
    /// 被 `CheckRoyalty` 消费的那个全局量。
    pub fn activate_spirit_mutiny(&mut self, duration_ms: i32) {
        self.spirit_mutiny_tick = self.clock_ms + i64::from(duration_ms);
    }

    /// 开启商人 tick 驱动（缺省关闭；见 `merchant_enabled` 字段说明）。
    ///
    /// 幂等：生产路径由 `bins/game-svr` 的 sim 循环在起世界时调用；合成 golden 场景保持关闭
    /// ⇒ 既有 `tests/parity/world` 逐字节不变。
    pub fn enable_merchant_tick(&mut self) {
        self.merchant_enabled = true;
    }

    /// 商人 tick 驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn merchant_tick_enabled(&self) -> bool {
        self.merchant_enabled
    }

    /// 开启固定 NPC tick 驱动（缺省关闭；见 `npc_enabled` 字段说明）。
    ///
    /// 幂等：生产路径由 `bins/game-svr` 的 sim 循环在起世界时调用；合成 golden 场景保持关闭
    /// ⇒ 既有 `tests/parity/world` 逐字节不变。
    pub fn enable_npc_tick(&mut self) {
        self.npc_enabled = true;
    }

    /// 固定 NPC tick 驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn npc_tick_enabled(&self) -> bool {
        self.npc_enabled
    }

    /// 开启 buff tick 驱动（缺省关闭；见 `buff_enabled` 字段说明）。
    ///
    /// 幂等：生产路径由 `bins/game-svr` 的 sim 循环在起世界时调用；合成 golden 场景保持关闭
    /// ⇒ 既有 `tests/parity/world` 逐字节不变。
    pub fn enable_buff_tick(&mut self) {
        self.buff_enabled = true;
    }

    /// buff tick 驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn buff_tick_enabled(&self) -> bool {
        self.buff_enabled
    }

    /// buff 门的「当前时刻」= `clock_ms + BUFF_EPOCH_MS`（与 [`World::buff_tick`] 的 pass 同基）。
    #[must_use]
    pub fn buff_now(&self) -> i64 {
        self.clock_ms + BUFF_EPOCH_MS
    }

    /// 一趟 buff 进：跑 [`BUFF_PASSES_PER_TICK`] 个 50ms pass（每个 pass 一个 `DoWork`）。
    ///
    /// pass 的 `now` = `clock_ms + BUFF_EPOCH_MS + i*50`（与 `add_buff`/`make_posion` 同一基）。
    /// 返回合并后的可观测产出（供 `TickRecord`）。oracle 的 `ActorBuffProcessor` 是独立 50ms
    /// 线程；本骨架在 200ms 的世界 tick 内串行跑 4 个 pass（U-A2 口径，见 [`crate::buff`] 模块头）。
    pub fn buff_tick(&mut self) -> BuffTickOutcome {
        let base = self.clock_ms + BUFF_EPOCH_MS;
        let mut out = BuffTickOutcome::default();
        for i in 0..BUFF_PASSES_PER_TICK {
            let now = base + i * BUFF_PASS_INTERVAL_MS;
            let pass = self.buff_do_work(now);
            out.status_changes.extend(pass.status_changes);
            out.sysmsgs.extend(pass.sysmsgs);
            out.recalcs.extend(pass.recalcs);
            out.health_spell_changes.extend(pass.health_spell_changes);
        }
        out
    }

    /// 单 pass 的 buff `DoWork`（供单测与 [`World::buff_tick`]）。`now_ms` 必须与
    /// `add_buff`/`make_posion` 的基一致（`clock_ms + BUFF_EPOCH_MS + …`）。
    pub fn buff_do_work(&mut self, now_ms: i64) -> BuffTickOutcome {
        self.buff_pass_tick = now_ms;
        // 把 `BuffSystem` 从 `World` 里取出，避免 `do_work` 同时借 `&mut self`（两条秩序都在
        // `do_work` 里改，且它要 `&mut World` 调用伤害/实体写）。
        let mut buffs = std::mem::take(&mut self.buffs);
        let out = buffs.do_work(self, now_ms);
        self.buffs = buffs;
        out
    }

    /// `BaseObject.GetCharStatus()`（`BaseObject.cs:1499-1503`）：
    /// `GetBuffStatus(this) | (CharStatusEx & 0x0000FFFF)`。见 [`crate::buff::get_char_status`]。
    #[must_use]
    pub fn get_char_status(&self, actor_id: i32) -> i32 {
        crate::buff::get_char_status(self, actor_id)
    }

    /// `BaseObject.MakePosion`（`BaseObject.cs:1941-1968`）：加毒 buff + 写 `CharStatus`/
    /// `GreenPoisoningPoint`。**返回值语义照 oracle**：`nType >= MAX_STATUS_ATTRIBUTE` ⇒ `false`
    /// （`:1943` 的首门）；否则**无条件 `return true`**（`:1966`）——与 `CharStatus` 是否变化**无关**。
    ///
    /// **登记（帧面 deferred，A-5）**：oracle 尾 `if (nOldCharStatus != CharStatus) StatusChanged();`
    /// （⇒ `RM_CHARSTATUSCHANGED`）与 `Race==Play` 的 `SysMsg` 未接（帧面归接线层）。
    ///
    /// **消费方（A-5 接线后的口径确认，独立复验 2026-10-17）**：本函数在本仓**唯一可达的生产入口 =
    /// 玩家攻击路径**——`PlayObject.Attack.cs:223` 的麻痹戒指命中（宿主 `bins/game-svr` 的
    /// `handle_hit_cmd` → `World::attack_once` 的 `attack_dir_grouped_poison` 那一次
    /// `make_posion(STONE, AttackPosionTime, 0)`）。
    /// **6 个怪物种类的 `MakePosion` 调用点在本仓不可接线**：`monai.rs` 只建模「通用族」AI、
    /// `mongen.rs` 不按怪物名分派 ⇒ 没有可挂的按名调用点。⇒ **原任务文本「怪物命中玩家施毒」
    /// 与 oracle 的接线口径不符**（oracle 那 6 处是**按怪物类**写的），需编排侧确认后再决定
    /// 是否为这 6 处单开片；本片**未引入、亦未修**。
    ///
    /// **登记订正 3（独立复验 2026-10-17）**：上面「oracle 入口」的枚举**低估**了——除本函数的
    /// 玩家攻击路径（`PlayObject.Attack.cs:223`）与 6 个怪物类（`CentipedeKingMonster.cs:70/74`、
    /// `GasAttackMonster.cs:33`、`GreenMonster.cs:25`、`Khazard.cs:32/46`、`RedMonster.cs:25`、
    /// `SpitSpider.cs:48`）外，还有**延迟消息路径**：`MagicManager.cs:241/245`
    /// （施毒术 `RM_POISON` DECHEALTH/DAMAGEARMOR）、`:921/:925`、`:1257`（麻痹 STONE）经
    /// `SendSelfDelayMsg(Messages.RM_POISON, …)` 落到 `BaseObject.Base.cs:740-758` 的
    /// `case Messages.RM_POISON:` ⇒ `MakePosion(processMsg.wParam, (ushort)nParam1, nParam3)`
    /// （另见 `BaseObject.Message.cs:44/108` 的同名 case 的延迟投递路径）。
    /// 本仓这些路径**都没有**（`SendSelfDelayMsg` 定时消息面未移植）⇒ 「本仓唯一可达生产入口 =
    /// 玩家攻击路径」这句**仍成立**，但 oracle 侧入口枚举按此**如实放宽**（保守方向）。
    ///
    /// 前置：`actor_id` 须是已落图的实体（oracle 里 `this` 恒有效）；`nType` 的 `from_index` 在
    /// `n_type < MAX_STATUS_ATTRIBUTE` 后恒 `Some`（12 值全覆盖）。
    pub fn make_posion(&mut self, actor_id: i32, n_type: u8, n_time: i32, n_point: i32) -> bool {
        if n_type >= MAX_STATUS_ATTRIBUTE {
            return false;
        }
        let Some(buff_type) = BuffType::from_index(n_type) else {
            return false;
        };
        let now = self.buff_now();
        self.buffs
            .add_buff(actor_id, buff_type, n_time, n_point, now);
        let status = self.get_char_status(actor_id);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.char_status = status;
            e.green_poisoning_point = n_point as u8;
        }
        true
    }

    /// `BaseObject.DefenceUp`（`BaseObject.cs:2137-2165`）：有 ⇒ `TryUpdate`，无 ⇒ `AddBuff`。
    /// （oracle 尾还有 `SysMsg`/`RecalcAbilitys`/`RM_ABILITY`，帧面/能力面 deferred，登记。）
    pub fn defence_up(&mut self, actor_id: i32, n_sec: i32) {
        let now = self.buff_now();
        if self.buffs.has_buff(actor_id, BuffType::DefensePower) {
            self.buffs
                .try_update(actor_id, BuffType::DefensePower, n_sec);
        } else {
            self.buffs
                .add_buff(actor_id, BuffType::DefensePower, n_sec, n_sec, now);
        }
        let status = self.get_char_status(actor_id);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.char_status = status;
        }
    }

    /// `BaseObject.MagDefenceUp`（`BaseObject.cs:2167-2185`）：无条件 `AddBuff`。
    pub fn mag_defence_up(&mut self, actor_id: i32, n_sec: i32) {
        let now = self.buff_now();
        self.buffs
            .add_buff(actor_id, BuffType::MagicDefensePower, n_sec, n_sec, now);
        let status = self.get_char_status(actor_id);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.char_status = status;
        }
    }

    /// `CharacterObject.MagicShield(nSec)`（`CharacterObject.cs:103`）：加魔法盾 buff + 置
    /// `MagicShield`（`DamageHealth` 的 MP 抵扣门）。
    pub fn magic_shield_on(&mut self, actor_id: i32, n_sec: i32) {
        let now = self.buff_now();
        self.buffs
            .add_buff(actor_id, BuffType::MagicShield, n_sec, n_sec, now);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.magic_shield = true;
        }
    }

    /// **片 A-5（buff 生产者接线）**：写入某 actor 的攻击命中中毒门运行态
    /// （[`crate::combat::AttackPoisonState`]）。宿主 `bins/game-svr` 在攻击派发前灌入；幂等（覆盖）。
    ///
    /// 与 [`World::attack_poison`] 表、[`crate::combat::attack_dir_grouped_poison`] 同源。
    pub fn set_attack_poison(&mut self, actor_id: i32, state: crate::combat::AttackPoisonState) {
        self.attack_poison.insert(actor_id, state);
    }

    /// 清某 actor 的攻击中毒门运行态（离场/卸麻痹戒指）。幂等。
    pub fn clear_attack_poison(&mut self, actor_id: i32) {
        self.attack_poison.remove(&actor_id);
    }

    /// **片 A-5（buff 生产者接线）**：`BaseObject.MagMakeDefenceArea`
    /// （`BaseObject.cs:2094-2132`）——技能 14/15 的 `DefenceUp`/`MagDefenceUp` 生产入口。
    ///
    /// **1:1 复刻 oracle 的一处 bug（不许"修正"）**：oracle 的双层循环虽有 `cX`/`cY`，但
    /// `ref Envir.GetCellInfo(nX, nY, out …)` 传的是**函数入参** `nX/nY`（`BaseObject.cs:2108`），
    /// **不是** `cX/cY` ⇒ `(2*range+1)^2` 个迭代**全部读中心格 `(nX,nY)`** ⇒ 效果等同
    /// 「中心格的每个友军各加 `(2*range+1)^2` 次 buff，周围格一个不加」。
    ///
    /// `bt_state == 0` ⇒ `baseObject.DefenceUp(nSec)`；否则 ⇒ `baseObject.MagDefenceUp(nSec)`
    /// （`BaseObject.cs:2117-2125`）。友军门 = [`crate::combat::is_proper_friend`]（`IsProperFriend`）；
    /// `Ghost` 目标跳过（oracle `!baseObject.Ghost`）。返回 `result`（命中次数求和，供 `boTrain` 的 `> 0` 门）。
    ///
    /// **登记**：oracle 的格 `ObjList` 只含**地图落格对象**（本仓 actors 不落格）⇒ 这里按
    /// 「世界内坐标 == `(nX,nY)` 且 `Race` 属 `Play`/`Monster`」的实体（槽位序）代替格列表；
    /// `cellSuccess`/`IsAvailable` 两支由「格内无实体 ⇒ 无命中」自然覆盖（登记）。
    pub fn mag_make_defence_area(
        &mut self,
        caster_id: i32,
        n_x: i32,
        n_y: i32,
        n_range: i32,
        n_sec: i32,
        bt_state: u8,
    ) -> i32 {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return 0;
        };
        let map_id = caster.map_id;
        let group_members: Vec<i32> = if caster.group_owner != 0 {
            self.entities
                .get(caster.group_owner)
                .map(|e| e.group_members.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let mut result = 0;
        for _c_x in (n_x - n_range)..=(n_x + n_range) {
            for _c_y in (n_y - n_range)..=(n_y + n_range) {
                // oracle bug：读的是中心格 `(nX,nY)`（见方法文档）。
                let ids: Vec<i32> = self
                    .entities
                    .iter()
                    .filter(|e| {
                        e.map_id == map_id
                            && e.x == n_x
                            && e.y == n_y
                            && (e.race == crate::entity::ACTOR_RACE_PLAY
                                || e.race >= crate::entity::ACTOR_RACE_ANIMAL)
                    })
                    .map(|e| e.id)
                    .collect();
                for id in ids {
                    let Some(obj) = self.entities.get(id).cloned() else {
                        continue;
                    };
                    if obj.ghost {
                        continue;
                    }
                    let in_group = caster.group_owner != 0 && group_members.contains(&id);
                    if !crate::combat::is_proper_friend(&caster, &obj, in_group) {
                        continue;
                    }
                    if bt_state == 0 {
                        self.defence_up(id, n_sec);
                    } else {
                        self.mag_defence_up(id, n_sec);
                    }
                    result += 1;
                }
            }
        }
        result
    }

    /// **片 A-5（buff 生产者接线）**：技能 31（`SKILL_SHIELD` 魔法盾）的生产入口
    /// （`MagicManager.cs:472-478` 的 `SKILL_SHIELD` 支 → `CharacterObject.MagBubbleDefenceUp`
    /// `CharacterObject.cs:96-110`）。
    ///
    /// `nSec = GetPower(GetRPow(MC) + 15)`（取数顺序：`GetRPow(MC)` → `GetPower`，见
    /// [`crate::magic::power_shield`]）；随后 `AddBuff(MagicShield, nSec, nSec)` + 置 `MagicShield`。
    /// 返回 `nSec`（供 `boTrain`/测试）。`MagBubbleDefenceUp` 的 `StatusTimeArr[BubbleDefenceUP] != 0`
    /// 早退门读的是**旧机制**（本骨架未建模 ⇒ 恒 0 ⇒ 不早退，登记）。
    pub fn cast_shield_spell(
        &mut self,
        actor_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        mc: i32,
    ) -> i32 {
        let n_sec = crate::magic::power_shield(&mut self.rng, magic, level, mc);
        self.magic_shield_on(actor_id, n_sec);
        n_sec
    }

    /// **片 A-5（buff 生产者接线）**：技能 14/15（`SKILL_HANGMAJINBUB`/`SKILL_DEJIWONHO`）的生产入口
    /// （`MagicManager.cs:352-366` 的 `SKILL_HANGMAJINBUB`/`SKILL_DEJIWONHO` 支）。
    ///
    /// `nPower = GetAttackPower(GetPower13(userMagic, 60) + Lo(SC)*10, ΔSC + 1)`（取数顺序：
    /// `GetPower13` → `GetAttackPower`；玩家 `GetAttackPower` override 若 `Luck <= 0` 多抽一次
    /// `Random(10)`）；随后 `MagMakeDefenceArea(nTargetX, nTargetY, 3, nPower, btState)`
    /// （`btState == 0` ⇒ `DefenceUp`；否则 ⇒ `MagDefenceUp`）。返回 `result`（供 `boTrain` 的 `> 0` 门）。
    #[allow(clippy::too_many_arguments)]
    pub fn cast_defence_area_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        sc: i32,
        n_target_x: i32,
        n_target_y: i32,
        bt_state: u8,
    ) -> i32 {
        let atk = self
            .entities
            .get(caster_id)
            .map_or(AttackPowerMode::Base, AttackPowerMode::for_attacker);
        let n_power = crate::magic::power_defence_area(&mut self.rng, magic, level, sc, atk);
        self.mag_make_defence_area(caster_id, n_target_x, n_target_y, 3, n_power, bt_state)
    }

    /// oracle `SystemProcessor.GetGameTime`（`SystemProcessor.cs:48/98-134`）的周期刷新等价物。
    ///
    /// 入参 `bucket` = 宿主按本地小时桶算出的桶值（`1`/`2`/`0`/`3`；生产路径走
    /// `enter::game_time_from_hour(chrono::Local::now().hour())`，**同一份换算**，不复制第二份）。
    /// 返回**是否发生变化**——oracle `PlayObject.Base.cs:824-830` 的
    /// `if (Bright != M2Share.GameTime)` 门；宿主据此对**每个**在线会话（各自缓存 `Bright`）
    /// 重发 `RM_DAYCHANGING`（`SM_DAYCHANGING(46)`）。
    ///
    /// **节奏口径**：oracle 每 50ms 重算一次；本骨架世界 tick 固定 200ms（`TICK_INTERVAL_MS`）
    /// ⇒ 宿主每 tick 调一次（U-A2/U-C3 的 50ms vs 200ms 适配，登记 `whitelist`）。因桶值只随
    /// **真实本地小时**翻页，200ms 与 50ms 的重算在**小时桶上等价**（错过的只是一个桶内无变化的
    /// 重算，不影响任何出帧时刻）。
    pub fn refresh_game_time(&mut self, bucket: u8) -> bool {
        let changed = bucket != self.game_time;
        self.game_time = bucket;
        changed
    }

    /// **M3⑨ 主人死/幽灵自投递**（卡 `openmir2-m3-pet-slave-masterdie-wire`）：开启
    /// 「自身延时消息」驱动并灌入宝宝配置。
    ///
    /// 生产宿主（`bins/game-svr` 的 sim 循环）在起世界时用 `Server.conf` 的
    /// [`mir2_data::pet_slave::PetSlaveConfig`] 调一次（oracle `SystemShare.Config.MasterDieMutiny*`）。
    /// 缺省关闭（见 `World` 的 `slave_self_delay_enabled` 字段说明）⇒ 合成 golden 场景逐字节不变。
    /// 幂等：重复调用只覆盖配置。
    pub fn enable_slave_self_delay(&mut self, cfg: mir2_data::pet_slave::PetSlaveConfig) {
        self.pet_slave_cfg = cfg;
        self.slave_self_delay_enabled = true;
    }

    /// 自身延时消息驱动是否开启（自证/测试用）。
    #[must_use]
    pub fn slave_self_delay_enabled(&self) -> bool {
        self.slave_self_delay_enabled
    }

    /// 设置某张地图的 `Envir.Flag.boEXPRATE`/`ExpRate`（see [`World::map_exp_rate`]）。
    /// 越界 `map_id` 静默忽略（接线层的图下标来自同一注册表）。幂等。
    pub fn set_map_exp_rate(&mut self, map_id: usize, rate: Option<i32>) {
        if let Some(slot) = self.map_exp_rate.get_mut(map_id) {
            *slot = rate;
        }
    }

    /// 设置某张地图的 `Envir.Flag.boNEEDHOLE`（route 落地门的输入，`PlayObject.cs:1874`）。
    /// 越界 `map_id` 静默忽略。幂等。生产宿主在起世界时按注册表逐图灌入；缺省 `false`。
    pub fn set_map_need_hole(&mut self, map_id: usize, need_hole: bool) {
        if let Some(map) = self.maps.get_mut(map_id) {
            map.set_need_hole(need_hole);
        }
    }

    /// 把注册表 `MapFile.doors`/`door_status` 搬进某图（`Envirnoment.cs:934-991` 加载即建 `DoorList`）。
    /// 越界 `map_id` 静默忽略。幂等（重复调用覆盖）。缺省空 ⇒ `arround_door_opened` 恒 `true`
    /// （改动前行为逐字节不变）。`crates/world` 不依赖 `crates/envir` ⇒ 用本中立结构
    /// [`crate::map::MapDoor`]/[`crate::map::DoorStatus`]，由宿主做映射。
    pub fn set_map_doors(
        &mut self,
        map_id: usize,
        doors: Vec<crate::map::MapDoor>,
        status: Vec<crate::map::DoorStatus>,
    ) {
        if let Some(map) = self.maps.get_mut(map_id) {
            map.set_doors(doors, status);
        }
    }

    /// 攻击路径取数的统一入口：按隔离开关选流。
    ///
    /// **只把攻击路径的取数**（[`crate::combat::attack_dir`] / [`crate::combat::swing_power`] 及其
    /// 全部下取：`atk_power_a/b`、`hit_gate`、`hit_struck_damage`、`struck_durability`、
    /// `attack_success`）路由到专用流；其余消费者仍走全局 `rng`。
    fn attack_rng(&mut self) -> &mut RandomNumber {
        if self.isolate_attack {
            &mut self.rng_attack
        } else {
            &mut self.rng
        }
    }

    pub fn map(&self, map_id: usize) -> &MapGrid {
        &self.maps[map_id]
    }

    /// 入队一条命令（会话线程 → 世界线程的交接点；本骨架里同线程、仍走队列以保证顺序）。
    pub fn enqueue(&mut self, cmd: Command) {
        self.pending.push_back(cmd);
    }

    /// 会话进入世界并落图。
    pub fn add_entity(&mut self, mut entity: Entity) -> i32 {
        if entity.id <= 0 {
            entity.id = self.entities.alloc_id();
        }
        // `BaseObject` ctor `PoisoningTick = GetTickCount()`（`BaseObject.cs:443`）：毒伤 DoT 的周期门
        // 从**实体落图那一刻**起算（buff 轴 = `clock_ms + BUFF_EPOCH_MS`，与 `buff_tick`/`make_posion` 同基）。
        // `Entity::new`（ctor 等价物）拿不到 `World` 时钟 ⇒ 在此按落图时刻精确重盖（同 `auto_recovery_tick`）。
        entity.poisoning_tick = self.clock_ms + BUFF_EPOCH_MS;
        let id = self.entities.insert(entity);
        self.enqueue(Command::Enter { actor_id: id });
        id
    }

    /// 走一步（入队，tick 时按序生效）。
    pub fn walk(&mut self, actor_id: i32, dx: i32, dy: i32) {
        self.enqueue(Command::Walk { actor_id, dx, dy });
    }

    /// 攻击（入队）。
    pub fn attack_cmd(&mut self, actor_id: i32, target_id: i32) {
        self.enqueue(Command::Attack {
            actor_id,
            target_id,
        });
    }

    /// **无目标挥砍**（`PlayObject.AttackDir(targetObject = null)`，`PlayObject.Attack.cs:25`）。
    ///
    /// oracle 在**取目标之前**就调 `GetAttackPowerHit(wHitMode, GetBaseAttackPoewr(), …)` ⇒
    /// 即使前方格无目标（`GetPoseCreate()` 返回 null），**攻击力取数照抽**（玩家 `Luck <= 0` 时
    /// `Random(power+1)` + `Random(10)` 共 2 抽）；之后 `_Attack(null)` 立刻 `return false`、
    /// `AttackSuccess(…, null)` 也在开头 `return` ⇒ **不再有别的取数**。
    ///
    /// 返回本次挥砍抽出的攻击力（`GetBaseAttackPoewr` 的结果）；actor 不存在 ⇒ `None`。
    ///
    /// **接线（`bins/game-svr`，属 m2-svr 写集）**：`handle_hit_cmd` 在“前方格无目标”支目前
    /// 只记日志（`main.rs` 的「只播动作帧（oracle `_Attack(null)` 返回 false）」支）⇒ 必须改调本方法；
    /// 否则 C# 侧那 115/120「有攻击命令、无目标」的 `atk_power_a/b` 两抽在 Rust 侧**整段缺失**
    /// （本片 .rs 侧归因，见 `M3RNG攻击路径三差归因报告-20261012.md`）。本 crate 不改 `bins/**`。
    #[must_use]
    pub fn attack_swing(&mut self, actor_id: i32) -> Option<i32> {
        let (dc, mode) = {
            let attacker = self.entities.get(actor_id)?;
            (attacker.dc, AttackPowerMode::for_attacker(attacker))
        };
        let power = swing_power(self.attack_rng(), dc, mode);
        Some(power)
    }

    /// 脚本化击杀（入队）。
    pub fn kill(&mut self, target_id: i32) {
        self.enqueue(Command::Kill { target_id });
    }

    /// 离开世界（入队）。
    pub fn leave(&mut self, actor_id: i32) {
        self.enqueue(Command::Leave { actor_id });
    }

    /// 推进一个 tick（固定 200ms；不做 sleep —— 调度归调用方）。
    pub fn tick(&mut self) -> TickRecord {
        let current = std::thread::current().id();
        match self.owner_thread {
            None => self.owner_thread = Some(current),
            Some(t) => assert_eq!(
                t, current,
                "世界 tick 必须始终在同一线程上串行执行（设计文档 §4.2：禁止并行）"
            ),
        }

        // S1 tick 骨架：时钟推进（+ 线程断言）在单线程 Schedule 里跑。
        // facade 仍是事实源（`clock_ms`/`tick_seq` 是对外 pub 字段）：先把值写进资源，
        // 跑完读回；数值语义与原 `clock_ms += 200; tick_seq += 1` 逐 tick 相同。
        {
            let mut clock = self.entities.world_mut().resource_mut::<TickClock>();
            clock.clock_ms = self.clock_ms;
            clock.tick_seq = self.tick_seq;
        }
        self.clock_runner.run(self.entities.world_mut());
        {
            let clock = self.entities.world().resource::<TickClock>();
            self.clock_ms = clock.clock_ms;
            self.tick_seq = clock.tick_seq;
        }
        let mut record = TickRecord {
            tick_seq: self.tick_seq,
            clock_ms: self.clock_ms,
            processed: Vec::new(),
            aoi_order: Vec::new(),
            attacks: Vec::new(),
            deaths: Vec::new(),
            exp_awards: Vec::new(),
            merchants_run: Vec::new(),
            merchant_turns: Vec::new(),
            merchant_hits: Vec::new(),
            merchant_space_moves: Vec::new(),
            merchant_removed: Vec::new(),
            npcs_run: Vec::new(),
            npc_turns: Vec::new(),
            npc_hits: Vec::new(),
            npc_removed: Vec::new(),
            party_events: Vec::new(),
            slave_self_deliveries: Vec::new(),
            pushes: Vec::new(),
            rushes: Vec::new(),
            butches: Vec::new(),
            hides: Vec::new(),
            buff_status_changes: Vec::new(),
            buff_sysmsgs: Vec::new(),
            buff_recalcs: Vec::new(),
            buff_health_changes: Vec::new(),
        };

        // 命令 FIFO
        while let Some(cmd) = self.pending.pop_front() {
            match cmd {
                Command::Enter { actor_id } => {
                    let ok = self.enter_world(actor_id);
                    record.processed.push((actor_id, ok));
                }
                Command::Leave { actor_id } => {
                    let ok = self.leave_world(actor_id);
                    record.processed.push((actor_id, ok));
                }
                Command::Walk { actor_id, dx, dy } => {
                    let ok = self.try_step(actor_id, dx, dy);
                    record.processed.push((actor_id, ok));
                }
                Command::Kill { target_id } => {
                    let ok = match self.entities.get_mut(target_id) {
                        Some(e) => {
                            e.hp = 0;
                            true
                        }
                        None => false,
                    };
                    record.processed.push((target_id, ok));
                }
                Command::Attack {
                    actor_id,
                    target_id,
                } => {
                    let outcome = self.attack_once(actor_id, target_id);
                    match outcome {
                        Some(o) => {
                            record.processed.push((actor_id, o.hit));
                            record.attacks.push((actor_id, target_id, o));
                        }
                        None => record.processed.push((actor_id, false)),
                    }
                }
                Command::SpawnMonster {
                    id,
                    name,
                    x,
                    y,
                    map_id,
                    level,
                    exp,
                    hp,
                    dc,
                    ac,
                    undead,
                    hit_point,
                    speed_point,
                    drop_list,
                } => {
                    let ok = self.spawn_monster_now(
                        id,
                        &name,
                        x,
                        y,
                        map_id,
                        level,
                        exp,
                        hp,
                        dc,
                        ac,
                        undead,
                        hit_point,
                        speed_point,
                        &drop_list,
                    );
                    record.processed.push((id, ok));
                }
                Command::Push {
                    actor_id,
                    dir,
                    count,
                } => {
                    let events = self.push_now(actor_id, dir, count);
                    record.processed.push((actor_id, !events.is_empty()));
                    record.pushes.extend(events);
                }
                Command::Rush {
                    actor_id,
                    dir,
                    steps,
                } => {
                    let events = self.rush_now(actor_id, dir, steps);
                    record.processed.push((actor_id, !events.is_empty()));
                    record.rushes.extend(events);
                }
                Command::Butch { actor_id, dir } => {
                    let event = self.butch_now(actor_id, dir);
                    record.processed.push((actor_id, event.is_some()));
                    if let Some(event) = event {
                        record.butches.push(event);
                    }
                }
            }
        }

        // 命中者过期清理（照 `BaseObject.Base.cs:119-132` 的结构；墙钟那两项不在本骨架）
        self.expire_hitters();
        // 死亡结算（C# 在实体下一次 `Run()` 里发现 `HP == 0` 才 `Die()`；
        // 这里同一 tick 内先跑命令、再统一结算死亡，语义等价且顺序确定）
        self.settle_deaths(&mut record);

        // **M3⑨ 主人死/幽灵自投递**（卡 `openmir2-m3-pet-slave-masterdie-wire`）：oracle
        // `PlayObject.Run()`（`PlayObject.Message.cs:360-364`）在 `base.Run()`（`:765`）之前
        // 先跑 `while (GetMessage(GetMessageTick, ...)) Operate(...)` 消息段 ⇒ 自投递的
        // `ProcessSlaveMutiny` / `ProcessSlaveGhost`（`PlayObject.Message.cs:2116-2121`）
        // **先于** `BaseObject.Run` 的宝宝清表（`BaseObject.Base.cs:208-217`）。同序放置。
        if self.slave_self_delay_enabled {
            record.slave_self_deliveries = self.deliver_self_delay_msgs();
        }

        // **M3⑨-实现 3**：`BaseObject.Run` 的宝宝列表清理（`BaseObject.Base.cs:208-217`）。
        // oracle 里这跑在每个主人（玩家）自己的 `Run()` 里；本骨架玩家无 `Run` ⇒ 在死亡结算后
        // 按槽位顺序对每个**持有宝宝**的实体跑一次。空表 ⇒ 零动作（golden 逐字节不变）。
        crate::slave::slave_list_cleanup_all(self);

        // **车道 B1（buff 系统）**：buff pass（oracle `ActorBuffProcessor` 的 50ms `DoWork`
        // 单线程等价物，见 [`crate::buff`]）。**缺省关闭**（`buff_enabled=false`）⇒ 既有 golden
        // 逐字节不变；宿主 `bins/game-svr` 的 sim 循环 `enable_buff_tick()`（生产路径）。
        // 位置：死亡结算之后、怪物 AI 之前（设计件 §3.3）。每 world tick（200ms）串行跑
        // [`BUFF_PASSES_PER_TICK`] 个 50ms pass（U-A2 口径）。buff 段**不抽 RNG** ⇒ 对
        // RNG 账本零影响（消费者次序不变）。
        if self.buff_enabled {
            let out = self.buff_tick();
            record.buff_status_changes = out.status_changes;
            record.buff_sysmsgs = out.sysmsgs;
            record.buff_recalcs = out.recalcs;
            record.buff_health_changes = out.health_spell_changes;
        }

        // M3④-实现 C：怪物 AI 一步（oracle `ProcessMonsters` → 每只 `monster.Run()`）。
        // **缺省关闭**：合成 golden 场景不含 oracle 的怪物线程，开 AI 会把 RNG 序列整体移位；
        // 由 `bins/game-svr` 的 sim 循环显式 `enable_monster_ai()`（见字段说明）。
        // 单线程串行：按实体槽位顺序（`EntityStore` 显式索引），在死亡结算之后、AOI 之前。
        if self.ai_enabled {
            crate::monai::monster_ai_tick(self);
        }
        // **W4**：把 AI（怪物）本 tick 的攻击结算转抄进 `record.attacks`——与玩家 `CM_HIT`
        // 命令的攻击**同一份**记录，宿主 `drain_combat_downlink` ① 据它出 `SM_STRUCK`。
        // 关闭 AI 时 `pending_ai_attacks` 恒空 ⇒ 既有 golden 逐字节不变。
        record.attacks.append(&mut self.pending_ai_attacks);

        // **世界 tick 扩容首批（T3）· ① NPC Run**：固定 NPC tick（oracle `MerchantProcessor` 的
        // `ProcessNpcs()`，200ms 线程单线程等价物）。
        //
        // **消费者次序（RNG 账本合同，必须固定）**：oracle `MerchantProcessor.ExecuteInternal`
        // （`MerchantProcessor.cs:28-32`）**先** `ProcessNpcs()` **后** `ProcessMerchants()`
        // ⇒ 本 `npc_tick` 排在 `merchant_tick` **之前**（U-B6 裁决：照 oracle 先后）。
        // `npc_run` 每次都从全服共享 `world.rng` 抽数 ⇒ 该次序决定共享 RNG 流的逐次序列
        // （B-R1 的「同窗口批次前提」）。整条线性化消费者序：
        //   命令 FIFO → 命中者过期 → 死亡结算 → 宝宝自投递 → 宝宝清表 → monster_ai_tick
        //   → **npc_tick** → merchant_tick → AOI。
        // 缺省 `npc_enabled=false`（合成 golden 不含 oracle 的 NPC 线程）⇒ 既有对拍逐字节不变。
        if self.npc_enabled {
            let n = self.npc_tick();
            record.npcs_run = n.ran;
            record.npc_turns = n.turns;
            record.npc_hits = n.hits;
            record.npc_removed = n.removed;
        }

        // **M3⑤-实现 2 第②片**：商人 tick（oracle `MerchantProcessor` 的 200ms 线程单线程等价物）。
        // 单线程串行：在怪物 AI 之后、AOI 刷新之前（与 oracle 同 tick 内的显式定序；oracle 里是两个
        // 独立 200ms 线程，跨系统交错不可复现 ⇒ 由 owner 裁决不在判据内）。
        if self.merchant_enabled {
            let m = self.merchant_tick();
            record.merchants_run = m.ran;
            record.merchant_turns = m.turns;
            record.merchant_hits = m.hits;
            record.merchant_space_moves = m.space_moves;
            record.merchant_removed = m.removed;
        }

        // AOI 刷新：按地图、地图内按槽位顺序（顺序来自 ECS 内核的**显式槽位索引**，
        // `EcsStore::iter` 不迭代 `Query` 求顺序）；AOI 的扫描/过滤语义仍归 S2。
        let ids: Vec<(i32, usize)> = self.entities.iter().map(|e| (e.id, e.map_id)).collect();
        // 复用缓冲（每 tick 不重新分配 —— 之前每次 vec![Vec::new(); maps] 让 P99 涨了约 15~35%）
        self.by_map_scratch.clear();
        self.by_map_scratch.resize_with(self.maps.len(), Vec::new);
        for v in self.by_map_scratch.iter_mut() {
            v.clear();
        }
        for (id, map_id) in ids {
            if let Some(v) = self.by_map_scratch.get_mut(map_id) {
                v.push(id);
            }
        }
        let by_map = std::mem::take(&mut self.by_map_scratch);
        for (map_id, ids) in by_map.iter().enumerate() {
            for id in ids {
                let death = match self.entities.get(*id) {
                    Some(e) => e.death,
                    None => continue,
                };
                if death {
                    let (map, entities) = (&mut self.maps, &mut self.entities);
                    search_view_range_death(
                        &mut map[map_id],
                        entities,
                        *id,
                        self.clock_ms,
                        self.clear_drop_on_floor_item_time,
                    );
                } else {
                    let (map, entities, clock) =
                        (&mut self.maps, &mut self.entities, self.clock_ms);
                    for target in search_view_range(&mut map[map_id], entities, *id, clock) {
                        // 观察者 `*id` 收「`target` 离开视野」⇒ `SM_DISAPPEAR(target)`（保序）。
                        record.hides.push((*id, target));
                    }
                }
                record.aoi_order.push(*id);
            }
        }
        self.by_map_scratch = by_map;

        self.last_tick = record.clone();
        record
    }

    /// 进图：实体落进 `map_id` 的格子（对应 `Envirnoment.AddMapObject`）。
    pub fn enter_world(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let now = self.clock_ms;
        if !map.add_map_object(x, y, CellType::Play, actor_id, now) {
            return false;
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = true;
        }
        true
    }

    /// 离开世界：从格子摘除 + 从仓库移除。
    pub fn leave_world(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let (cell, success) = map.get_cell_info_mut(x, y);
        if success {
            let before = cell.count();
            cell.obj_list
                .retain(|o| !(o.actor_object && o.cell_obj_id == actor_id));
            if cell.count() == 0 {
                cell.clear();
            }
            if cell.count() == before {
                return false;
            }
        }
        self.entities.remove(actor_id);
        true
    }

    /// 生成怪物：入队后由 tick 处理（掉落按 C# 时机在**生成时**预算）。
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_monster(&mut self, cmd: Command) -> bool {
        let Command::SpawnMonster { .. } = &cmd else {
            return false;
        };
        self.enqueue(cmd);
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn spawn_monster_now(
        &mut self,
        id: i32,
        name: &str,
        x: i32,
        y: i32,
        map_id: usize,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        hit_point: u8,
        speed_point: u8,
        drop_list: &[MonsterDropItem],
    ) -> bool {
        if map_id >= self.maps.len() {
            return false;
        }
        // 掉落预算（`WorldServer.MonGen.cs:440` 的时机）：RNG 在这里消耗
        let catalog_items = std::mem::take(&mut self.item_catalog);
        let (gold, items) = {
            let catalog = VecItemCatalog {
                items: &catalog_items,
            };
            generate_drops(
                &mut self.rng,
                drop_list,
                &catalog,
                self.mon_random_add_value,
                STRING_GOLD_NAME,
                &mut self.item_number,
            )
        };
        self.item_catalog = catalog_items;

        let mut e = crate::combat::monster_from_row(crate::combat::MonsterRow {
            id,
            name,
            x,
            y,
            level,
            exp,
            hp,
            dc,
            ac,
            undead,
            hit_point,
            speed_point,
        });
        e.id = id;
        e.map_id = map_id;
        e.gold = gold;
        e.drop_items = items;
        // `BaseObject.cs:443` ctor `PoisoningTick = GetTickCount()`：按落图时刻重盖（buff 轴）。
        e.poisoning_tick = self.clock_ms + BUFF_EPOCH_MS;
        let id = self.entities.insert(e);
        self.enter_world(id)
    }

    /// **生成期的掉落预算**（`MonGetRandomItems` 的时机）——用**调用方给定的**随机流。
    ///
    /// oracle `WorldServer.MonGen.cs:441-488` 的 `MonGetRandomItems` 在 `CreateMonster`
    /// 内部、即 **Dir 抽之后、CoolEye 抽之前** 发生在同一 `M2Share.RandomNumber` 上。
    /// 接线层的 `MonsterCreator` 回调（[`crate::mongen::regen_monsters`]）需要在「两次位置抽
    /// 之间」把这半段插入正确位置；本方法让回调不必碰 `World` 私有字段（`item_catalog` /
    /// `item_number` / `mon_random_add_value`）就能在**同一条**流上预算。
    pub fn budget_monster_drops(
        &mut self,
        rng: &mut RandomNumber,
        drop_list: &[MonsterDropItem],
    ) -> (i32, Vec<crate::combat::DropItem>) {
        let catalog_items = std::mem::take(&mut self.item_catalog);
        let (gold, items) = {
            let catalog = VecItemCatalog {
                items: &catalog_items,
            };
            generate_drops(
                rng,
                drop_list,
                &catalog,
                self.mon_random_add_value,
                STRING_GOLD_NAME,
                &mut self.item_number,
            )
        };
        self.item_catalog = catalog_items;
        (gold, items)
    }

    /// 生成**已预算好掉落/朝向**的怪物：**不再取任何随机数**（掉落预算由调用方在
    /// [`World::budget_monster_drops`] 里按 oracle 次序完成；构造函数链的取数由调用方的
    /// `CreateMonster` 回调完成）。
    ///
    /// 与 [`World::spawn_monster_now`] 的唯一差别是「不自己抽掉落」——用于
    /// `RegenMonsters` 的 `CreateMonster` 回调（拖到 tick 里再抽会与 oracle 的 interleaving 不同）。
    /// `ctor` = 构造函数链已抽出的值（[`crate::entity::MonsterCtorDraws`]），随生成结果落实体；
    /// `dir`/`cool_eye` = `CreateMonster` 的 `cert.Dir = RandomByte(8)`（`WorldServer.MonGen.cs:740`）
    /// 与 `CoolEye`（`:745-747`）——与本函数**同时**作为生成期取数落进 `Entity.ai` 的初始态
    /// （[`crate::entity::AnimalState::seed_from_spawn`]），供 `World::tick` 的 AI 消费。
    #[allow(clippy::too_many_arguments)]
    pub fn spawn_monster_precomputed(
        &mut self,
        name: &str,
        x: i32,
        y: i32,
        map_id: usize,
        race: u8,
        level: u8,
        exp: i32,
        hp: u16,
        dc: u16,
        ac: u16,
        undead: u8,
        hit_point: u8,
        speed_point: u8,
        gold: i32,
        drop_items: Vec<crate::combat::DropItem>,
        ctor: crate::entity::MonsterCtorDraws,
        dir: u8,
        cool_eye: bool,
    ) -> Option<i32> {
        if map_id >= self.maps.len() {
            return None;
        }
        let id = self.entities.alloc_id();
        let mut e = crate::combat::monster_from_row(crate::combat::MonsterRow {
            id,
            name,
            x,
            y,
            level,
            exp,
            hp,
            dc,
            ac,
            undead,
            hit_point,
            speed_point,
        });
        e.race = race;
        e.id = id;
        e.map_id = map_id;
        e.gold = gold;
        e.drop_items = drop_items;
        e.ctor_draws = ctor;
        // 生成期取数已全部完成（本函数不再抽随机数）；把结果写进 AI 初始态。
        e.ai.seed_from_spawn(dir, cool_eye, &ctor, self.clock_ms);
        // `BaseObject` ctor `AutoRecoveryTick = GetTickCount()`（`BaseObject.cs:445`）：
        // 恢复段从实体落库那一刻起算。
        e.auto_recovery_tick = self.clock_ms;
        // `BaseObject.cs:443` ctor `PoisoningTick = GetTickCount()`：同上，按落图时刻重盖（buff 轴）。
        e.poisoning_tick = self.clock_ms + BUFF_EPOCH_MS;
        let id = self.entities.insert(e);
        if !self.enter_world(id) {
            self.entities.remove(id);
            return None;
        }
        Some(id)
    }

    /// 死亡结算：把预算好的掉落散到地上 + 给击杀者发经验。
    ///
    /// 命中者过期（`BaseObject.Base.cs:119-132`）：`LastHiter` 30s 或命中者死亡/隐身，
    /// `ExpHitter` 6s 或命中者死亡/隐身 —— 两条件**逐字照搬**（`||` 短路，先判时间再判存殓）。
    ///
    /// ⚠ oracle 是在**命中者自己的** `Run()` 里清，本骨架在每 tick 的命令之后统一清（结构等价，
    /// 见 `settle_deaths` 的说明）。本函数使用 **sim 时钟**（`clock_ms`），不消费 RNG。
    ///
    /// 先前版本只做「命中者已死/已隐身」的结构清理（30s/6s 墙钟两项标为未建模，whitelist T-32）；
    /// 本片补上两只墙钟门并收窄 T-32。
    fn expire_hitters(&mut self) {
        let now = self.clock_ms;
        let gone: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.death || e.ghost)
            .map(|e| e.id)
            .collect();
        for e in self.entities.iter_mut() {
            if let Some(h) = e.last_hiter {
                if (now - e.last_hiter_tick) > 30_000 || gone.contains(&h) {
                    e.last_hiter = None;
                }
            }
            if let Some(h) = e.exp_hitter {
                if (now - e.exp_hitter_tick) > 6_000 || gone.contains(&h) {
                    e.exp_hitter = None;
                }
            }
        }
    }

    /// oracle `PlayObject.GainExp(int dwExp)`（`PlayObject.Base.cs:764-821`）。
    ///
    /// 在**经验获得者**（`KillTargetTrigger` 的 `this`）上调用：
    ///
    /// - `GroupOwner != 0` 但**团长对象不在注册表**（`ActorMgr.Get` 取不到，775 行）⇒ `GroupMembers`
    ///   空引用抛异常、被 `catch`（818-821）吞掉 ⇒ **本次零经验、零授奖**（不会退回单人满经验）；
    /// - `GroupOwner != 0` ⇒ 遍历**团长** `GroupMembers`，统计「`!Death` + 同图 +
    ///   `|CurrX - x| <= 12` + `|CurrX - x| <= 12`」的 `sumlv`/`n` —— **两处都比 `CurrX`、
    ///   都漏了 `CurrY`，是 oracle 的既有缺陷，照搬**（§1 硬约束③），并在判据里带该缺陷向量；
    /// - `sumlv > 0 && n > 1` ⇒ `dwExp = Round(dwExp * bonus[n])`（`n <= GroupMax` 门），再逐成员
    ///   `WinExp(Round(dwExp / n))`（`HighLevelKillMonFixExp = true`）或
    ///   `WinExp(Round(dwExp / sumlv * level))`（`false`）；
    /// - 否则 ⇒ 对获得者本人 `WinExp(dwExp)`。
    ///
    /// 返回**获得者本人**本次获得的经验（= [`TickRecord::deaths`] 5 元组的第 5 项），
    /// 并把每个进了 `WinExp` 的成员的 `(id, amount, leveled, level)` 按 oracle 的成员遍历序
    /// 追加进 `record.exp_awards`（宿主据此逐人出 `SM_WINEXP(44)`）。
    fn gain_exp(&mut self, recipient: i32, mon_exp: i32, record: &mut TickRecord) -> i32 {
        let cfg = self.combat_config;
        let need_exps = std::mem::take(&mut self.need_exps);
        // `KillTargetTrigger`：`monsterExp = CalcGetExp(WAbil.Level, fightExp)`（入参即自己等级）。
        let (monster_exp, group_owner, rx, rmap) = match self.entities.get(recipient) {
            Some(e) => (
                mir2_formula::exp::calc_get_exp(
                    i32::from(e.level),
                    i32::from(e.level),
                    mon_exp,
                    cfg.high_level_kill_mon_fix_exp,
                ),
                e.group_owner,
                e.x,
                e.map_id,
            ),
            None => {
                self.need_exps = need_exps;
                return 0;
            }
        };
        let map_rate = self.map_exp_rate.get(rmap).copied().flatten();
        let mut my_gain = 0;
        if group_owner != 0 {
            // oracle `PlayObject.Base.cs:771-776`：`GroupOwner != 0` 时先
            // `(IPlayerActor)SystemShare.ActorMgr.Get(GroupOwner)` 取团长对象；取不到
            // ⇒ `groupOwnerPlay.GroupMembers` 空引用抛异常，被 818-821 的 `catch` 吞掉
            // ⇒ 本次**零经验、零授奖**（不会退回单人满经验）。可达成因由：`leave_world`
            // 移除实体时不清同组成员的 `group_owner`（见 `leave_world`）。
            let members = match self.entities.get(group_owner) {
                Some(owner) => owner.group_members.clone(),
                None => {
                    self.need_exps = need_exps;
                    return 0;
                }
            };
            // 条件 = 团长成员表 ∩「同图 + !Death + |CurrX-x|<=12 + |CurrX-x|<=12（漏 CurrY，oracle 照搬）」。
            let in_range: Vec<i32> = members
                .iter()
                .copied()
                .filter(|&m| {
                    self.entities.get(m).is_some_and(|me| {
                        !me.death
                            && me.map_id == rmap
                            && (me.x - rx).abs() <= 12
                            && (me.x - rx).abs() <= 12
                    })
                })
                .collect();
            let sumlv: i32 = in_range
                .iter()
                .filter_map(|&m| self.entities.get(m))
                .map(|me| i32::from(me.level))
                .sum();
            let n = in_range.len();
            if sumlv > 0 && n > 1 {
                let mut dw_exp = monster_exp;
                // oracle：`if (n >= 0 && n <= Grobal2.GroupMax) dwExp = Round(dwExp * bonus[n]);`
                if n <= crate::party::PARTY_GROUP_MAX {
                    dw_exp = mir2_shared::hutil::round(
                        f64::from(dw_exp) * crate::party::PARTY_EXP_BONUS[n],
                    );
                }
                for &m in &in_range {
                    let level = self.entities.get(m).map_or(0, |me| i32::from(me.level));
                    let amount = if cfg.high_level_kill_mon_fix_exp {
                        mir2_shared::hutil::round(f64::from(dw_exp) / n as f64)
                    } else {
                        mir2_shared::hutil::round(
                            f64::from(dw_exp) / f64::from(sumlv) * f64::from(level),
                        )
                    };
                    let Some(k) = self.entities.get_mut(m) else {
                        continue;
                    };
                    let (g, l, lvl) = win_exp(k, amount, &need_exps, &cfg, map_rate);
                    if m == recipient {
                        my_gain = g;
                    }
                    record.exp_awards.push((m, g, l, lvl));
                }
            } else {
                let (g, l, lvl) = match self.entities.get_mut(recipient) {
                    Some(k) => win_exp(k, monster_exp, &need_exps, &cfg, map_rate),
                    None => {
                        self.need_exps = need_exps;
                        return 0;
                    }
                };
                my_gain = g;
                record.exp_awards.push((recipient, g, l, lvl));
            }
        } else {
            let (g, l, lvl) = match self.entities.get_mut(recipient) {
                Some(k) => win_exp(k, monster_exp, &need_exps, &cfg, map_rate),
                None => {
                    self.need_exps = need_exps;
                    return 0;
                }
            };
            my_gain = g;
            record.exp_awards.push((recipient, g, l, lvl));
        }
        self.need_exps = need_exps;
        my_gain
    }

    fn settle_deaths(&mut self, record: &mut TickRecord) {
        let dead: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.hp == 0 && !e.death_settled)
            .map(|e| e.id)
            .collect();
        for id in dead {
            let Some(e) = self.entities.get(id) else {
                continue;
            };
            let (_map_id, gold, mon_exp, exp_hitter, group_owner, is_play) = (
                e.map_id,
                e.gold,
                e.mon_exp,
                e.exp_hitter,
                e.group_owner,
                e.race == crate::entity::ACTOR_RACE_PLAY,
            );
            // 死亡标记（C# `Die()` 的 `Death = true`）
            if let Some(e) = self.entities.get_mut(id) {
                e.death = true;
                e.death_settled = true;
            }
            // **M3⑦-实现 2 F-1**：oracle `Die()`（`PlayObject.cs:1964-1978`）在置 `Death` 之后、
            // `base.Die()`（掉落）之前判 `if (GroupOwner != 0) groupOwnerPlay.DelMember(this)`
            // （`:1975-1977`）⇒ 死亡者立即退组；团长死亡 ⇒ 逐成员 `LeaveGroup` 后整组散。
            // 与断线路径（`Disappear()`）的差别：死亡者**仍在线**，它自己的回帧照发（由宿主决定）。
            if group_owner != 0 {
                let cfg = self.party_cfg.clone();
                let events = self.party_del_member(group_owner, id, &cfg);
                record.party_events.extend(events);
            }
            // 掉落结算（oracle `DieDropItems`，`BaseObject.cs:2450-2464`）：
            // `ScatterBagItems`（`BaseObject.Base.cs:501-550`，**倒序**遍历 `ItemList`）
            // + `ScatterGolds`（`:1711`）逐件调 `DropItemDown`（`BaseObject.cs:522-580`）/
            // `DropGoldDown`（`:730-768`）：先 `GetDropPosition`（`:459-524`，**纯格子扫描、不消费
            // RNG**），再 `Envir.AddItemToMap`（`Envirnoment.cs:88-130`）——而后者的**首行**是
            // `if (mapItem.ItemId == 0) return false;`。
            // oracle 全仓**无** `MapItem.ItemId` 赋值点（`MapItem.cs:41` 被注释；`rg 'ItemId\s*=' src`
            // 其余 4 处全是 `ItemId == 0` 守卫）⇒ **死亡掉落（物品 + 金币）恒不落地**，
            // `DropItemDown`/`DropGoldDown` 成功支的 `SendRefMsg(RM_ITEMSHOW,…)`（`:570`/`:752`）
            // 与客户端 `SM_ITEMSHOW(610)` **恒不执行**（与 M3① 的 T-19/T-25 同根；owner 已在
            // T-25 裁决「无地面物」为唯一可达形态）。
            // ⇒ Rust 照抄：**不再**把 `e.drop_items`/`Gold` 写进地图（`placed` 恒 0）。
            //    掉落**生成**（`MonGetRandomItems` 的预算）不受影响，仍是本卡判据要对比的「掉落列表」。
            let placed = 0usize;
            // 经验：`KillFunc`（`BaseObject.cs:2429`）认的是 `ExpHitter`，不是 `LastHiter`
            // —— `ExpHitter` 是**第一个**命中者（`SetLastHiter` 里 `if (ExpHitter == null)`）。
            // "命中者死亡/隐身后不再认领"由**每 tick 的 `expire_hitters()`** 按 oracle 的结构清理
            // （`BaseObject.Base.cs:119-132` 在命中者自己的 `Run()` 里清），故这里只判"归属者是玩家本人"
            // —— 与 `KillFunc` 的形状一致。
            // 子集口径：`ExpHitter.Master != null`（宝宝击杀）不在本骨架；30s/6s 的**墙钟**过期也不做。
            let mut gained = 0;
            if let Some(hitter_id) = exp_hitter {
                let hitter_master = self.entities.get(hitter_id).and_then(|k| k.master);
                let hitter_is_player = self
                    .entities
                    .get(hitter_id)
                    .is_some_and(|k| k.race == crate::entity::ACTOR_RACE_PLAY);
                // oracle `KillFunc`（`BaseObject.cs:2434-2444`）：`ExpHitter.Master != null` ⇒
                // 给 master；`ExpHitter.Race == Play` ⇒ 再给 `ExpHitter` 本人（两支照抄，
                // 真实实体上互斥）。`deaths` 的 5 元组口径仍取「主获得者」= master 优先。
                let mut winners: Vec<i32> = Vec::new();
                if let Some(m) = hitter_master {
                    winners.push(m);
                }
                if hitter_is_player {
                    winners.push(hitter_id);
                }
                let primary = hitter_master.unwrap_or(hitter_id);
                for winner in winners {
                    // oracle `KillTargetTrigger` → `GainExp`：每名 `RM_PLAYERKILLMONSTER`
                    // 获得者独立跑一次 `CalcGetExp` + `GainExp`（组队分配见 [`World::gain_exp`]）。
                    let amount = self.gain_exp(winner, mon_exp, record);
                    if winner == primary {
                        gained = amount;
                    }
                }
            }
            record.deaths.push((id, placed, gold, exp_hitter, gained));
            // **M3⑨ 主人死/幽灵自投递**：oracle `PlayObject.Die()` 尾
            // （`src/M2Server/Player/PlayObject.cs:1993-1996`）在 `base.Die()`（掉落）**之后**
            // `SendSelfDelayMsg(RM_MASTERDIEMUTINY, 0,0,0,0,"",1000)`（玩家 `Race == Play` 的
            // `Die()` override；怪物走 `BaseObject.Die()` 不发）。1s 后由本 tick 的自投递段
            // 触发 `ProcessSlaveMutiny`。
            if is_play && self.slave_self_delay_enabled {
                self.send_master_die_mutiny(id);
            }
        }
    }

    /// 一次攻击结算（`_Attack`）。
    pub(crate) fn attack_once(&mut self, actor_id: i32, target_id: i32) -> Option<AttackOutcome> {
        if self.entities.get(actor_id).is_none() || self.entities.get(target_id).is_none() {
            return None;
        }
        // 取出双方属性做结算（避免同时可变借用），再写回
        let mut attacker = self.entities.get(actor_id)?.clone();
        let mut target = self.entities.get(target_id)?.clone();
        // `StruckDamage` 的倍率取目标**当下存着**的 `LastHiter`（= 上一次命中者：本次命中者要等
        // 目标下一轮 `Run()` 处理 `RM_STRUCK` 才登记）。默认倍率 10 时数值不可分辨，非默认配置下
        // 才有差异 ⇒ 必须照 oracle 取，别改成本次攻击者。
        let last_hitter = target
            .last_hiter
            .and_then(|id| self.entities.get(id))
            .map(|h| crate::combat::LastHitter {
                race: h.race,
                job: h.job,
                has_master: h.master.is_some(),
            });
        let cfg = self.combat_config;
        // 攻方攻击力口径：oracle 的 `GetBaseAttackPoewr` 走**虚函数** `GetAttackPower`
        //（`BaseObject.Attack.cs:25` → `BaseObject.Base.cs:886` 基版；`PlayObject.cs:2909` 玩家 override）。
        // 玩家（`Race == ActorRace.Play`）走 override（`Luck`/`PowerRate`/`BoPowerItem`）；
        // 怪物等无 override 的对象走基版。两版**取数次数不同**（`Luck <= 0` 时 override 多一次
        // `Random(10)`）⇒ 必须按对象分派，不能一律走 `Base`（否则跨侧序列从第一次命中起错位）。
        //
        // 玩家攻方还要走完整的 `PlayObject.AttackDir`：`_Attack` 之后 `AttackSuccess` 再抽一次
        // `Random(5)`（`PlayObject.Attack.cs:225`）—— 由 [`crate::combat::attack_dir`] 统一补上；
        // 怪物攻方无这条（`AttackSuccess` 是 `PlayObject` 私有方法）。
        // `HAM_GROUP` 的 `IsGroupMember(target)`（`PlayObject.Attack.cs:671-677`）需要攻方所在
        // 组团长的成员表；`GroupOwner == 0` ⇒ 空表（`IsGroupMember` 第一步即 `false`）。
        let attacker_group_members: Vec<i32> = if attacker.group_owner != 0 {
            self.entities
                .get(attacker.group_owner)
                .map(|e| e.group_members.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        // **片 A-5（buff 生产者接线）**：攻击命中中毒门（`PlayObject.Attack.cs:221-224`）的运行态。
        // 每个 actor 的表项存**自身**的三个属性（`Paralysis` 作攻方 + `UnParalysis`/`AntiPoison`
        // 作守方）；门取**攻方**的 `Paralysis` 与**守方**的另两项。buff 关闭时传 `None` ⇒ 与
        // [`attack_dir_grouped`] 逐取数等价（既有 golden 不变）；开启且攻方有麻痹态时，在
        // `Random(5)` **之前**插一次 `Random(AntiPoison + AttackPosionRate)`（oracle 取数次序）。
        let poison = if self.buff_enabled {
            self.attack_poison
                .get(&actor_id)
                .map(|a| AttackPoisonState {
                    paralysis: a.paralysis,
                    un_paralysis: self
                        .attack_poison
                        .get(&target_id)
                        .is_some_and(|t| t.un_paralysis),
                    anti_poison: self
                        .attack_poison
                        .get(&target_id)
                        .map_or(0, |t| t.anti_poison),
                })
        } else {
            None
        };
        let (outcome, poisoned) = attack_dir_grouped_poison(
            self.attack_rng(),
            &mut attacker,
            &mut target,
            last_hitter,
            &cfg,
            &attacker_group_members,
            poison,
        );
        let now = self.clock_ms;
        if let Some(e) = self.entities.get_mut(target_id) {
            e.hp = target.hp;
            // 命中者登记的**折叠**：C# 在 `_Attack` 里对 `Race > ActorRace.Play` 的目标
            // `SendMsg(RM_STRUCK)`（`BaseObject.Attack.cs:61-64`），由目标在自己的 `Run()` 里
            // `SetLastHiter(攻击方)`（`AnimalObject.cs:118-124`）。这里在结算之后立刻登记：
            // 本次 `_Attack` 期间的 `StruckDamage` 仍看到旧的 `LastHiter`，与 C# 同序。
            if target.race > crate::entity::ACTOR_RACE_PLAY {
                e.set_last_hiter(actor_id, now);
                // `targetObject.SendMsg(targetObject, RM_STRUCK, …)`（`BaseObject.Attack.cs:61-64`）
                // 的**入队等价物**：`SendMsg` 只在 `!Ghost` 时入队（`BaseObject.Message.cs:152-183`）。
                // 出队处理在目标的 `base.Run`（[`crate::monai::base_object_run_monster`]）。
                if !e.ghost {
                    e.ai.pending_struck.push(actor_id);
                }
            }
        }
        // `targetObject.MakePosion(PoisonState.STONE, SystemShare.Config.AttackPosionTime, 0)`
        // （`PlayObject.Attack.cs:223`）：命中中毒门过了 ⇒ 给**目标**上石化毒 buff。
        // `STONE = 5` 与 `BuffType::Paralysis` 数值对齐（见 [`crate::buff`] 的索引表）。
        // `MakePosion` 不抽数 ⇒ 取数账本不受影响（那一次抽取已在
        // [`attack_dir_grouped_poison`] 里按 oracle 的位置、`&&` 短路口径发生）。
        if poisoned {
            self.make_posion(
                target_id,
                BuffType::Paralysis as u8,
                cfg.attack_poison_time,
                0,
            );
        }
        Some(outcome)
    }

    /// 把注册表解析出的连接点灌进对应 `MapGrid`（oracle `Maps.LoadMapInfo` 加载即
    /// `Envirnoment.AddMapRoute` 落格）。返回**实际落格**的条数。
    ///
    /// `map_id` 越界 ⇒ 该条跳过（不 panic）。无注册表时宿主不调用本方法 ⇒ 无 route，行为一字不变。
    pub fn add_map_routes<I>(&mut self, routes: I) -> usize
    where
        I: IntoIterator<Item = MapRouteSpec>,
    {
        let mut added = 0;
        for spec in routes {
            let Some(map) = self.maps.get_mut(spec.map_id) else {
                continue;
            };
            if map.add_map_route(spec.x, spec.y, spec.route_id, spec.hop) {
                added += 1;
            }
        }
        added
    }

    /// **L1 TransMap 触发（核心）**：先按 [`World::try_step_now`] 移动；**移动成功后**读落点格
    /// （`PlayObject.Walk` 覆写读 `CurrX/CurrY` 格，`PlayObject.cs:1868`），命中 `CellType::MapRoute`
    /// 且是玩家时走 `EnterAnotherMap` 门并 `switch_map`。
    ///
    /// **失败契约（照 oracle）**：落点格是 `MapRoute` 但 `EnterAnotherMap` 门失败（目标格非
    /// `CellValid`）⇒ oracle `Walk()` 返回 `false`（`PlayObject.cs:1878`）⇒ 上游 `BaseObject.WalkTo`
    /// 把坐标**回滚到走前格**（`BaseObject.cs:685-688`）⇒ 因 `if (result)` 不成立而**不发移动帧**
    /// （`PlayObject.cs:1909-1911`）。本方法据此返回 `moved == false` 并回滚，宿主因此不发 `SM_WALK`。
    ///
    /// 只对**玩家**触发（`PlayObject.Walk` 覆写专属）；怪物 AI 的 `monai::walk_to` 也走 `try_step_now`，
    /// 但非玩家在 `try_step_now` 即被**非玩家 `MapRoute` 阻挡**拦下（`BaseObject.cs:1548` 的
    /// `result = false` 配 `WalkTo` 回滚 `:683-688`——**只挡路、不换图**，坐标不变）⇒ 根本到不了本函数；
    /// 即便到了，`route_enter` 对非玩家也直接返回 [`RouteOutcome::Skipped`]（不换图）。
    #[must_use]
    pub fn step_with_route(&mut self, actor_id: i32, dx: i32, dy: i32, steps: i32) -> StepOutcome {
        let Some(e) = self.entities.get(actor_id) else {
            return StepOutcome {
                moved: false,
                changed_map: false,
            };
        };
        // 走前格（回滚落点；oracle `WalkTo` 的 `oldX/oldY`/`oldEnvir`）。
        let (old_x, old_y, old_map) = (e.x, e.y, e.map_id);
        if !self.try_step_now(actor_id, dx, dy, steps) {
            return StepOutcome {
                moved: false,
                changed_map: false,
            };
        }
        // 记下本轮起点，失败回滚时把误记的 `MapChange` 撤掉（否则宿主多发一组换图簇）。
        let mc_len = self.sessions.map_changes().len();
        match self.route_enter(actor_id) {
            RouteOutcome::Failed => {
                self.sessions.truncate_map_changes(mc_len);
                self.rollback_move(actor_id, old_x, old_y, old_map);
                StepOutcome {
                    moved: false,
                    changed_map: false,
                }
            }
            RouteOutcome::Switched => StepOutcome {
                moved: true,
                changed_map: true,
            },
            RouteOutcome::Skipped => StepOutcome {
                moved: true,
                changed_map: false,
            },
        }
    }

    /// **L1 TransMap 触发**：actor 在**当前格**做一次路线检查（走/跑/骑跑成功移动之后，或 `CM_TURN`
    /// 原地转身）。返回 `true` = 发生了换图。
    ///
    /// 调用点：① [`World::step_with_route`]（走）；② `CM_TURN` 原地转身（oracle `ClientChangeDir`
    /// 也调 `Walk(RM_TURN)`，`PlayObject.Operate.cs:334`）。转身路径无「走前格」可回滚，故 `Failed`
    /// 时只撤掉误记的 `MapChange` 并返回 `false`（oracle 转身时 `Walk()` 返回 false ⇒ 不发转身帧而回
    /// `RM_MOVEFAIL`；**该帧本宿主未接，登记**）。
    pub fn walk_route_check(&mut self, actor_id: i32) -> bool {
        let mc_len = self.sessions.map_changes().len();
        match self.route_enter(actor_id) {
            RouteOutcome::Switched => true,
            RouteOutcome::Failed => {
                self.sessions.truncate_map_changes(mc_len);
                false
            }
            RouteOutcome::Skipped => false,
        }
    }

    /// **`PlayObject.Walk` 覆写（`PlayObject.cs:1850-1918`）的路线检查本体**：读 actor 的当前格，
    /// **顺序遍历该格 `ObjList` 里的全部 `CellType::MapRoute`**（oracle 的 `for` **不 break**，
    /// `PlayObject.cs:1862-1884`），逐个走门 + `EnterAnotherMap`（复用 [`World::switch_map`] 落地）。
    ///
    /// 逐项对照 oracle 门（`PlayObject.cs:1869-1882` 外套 `EnterAnotherMap`，`:3718-3757`）：
    /// - `Envir.CellMatch(CurrX,CurrY)` 失败 ⇒ `return true`（无换图、不回滚）；
    /// - `cellSuccess && cellInfo.IsAvailable` 失败 ⇒ 不进 `for`（无换图）；
    /// - `mapRoute.Envir != null`：本仓 hop 表命中即非空；
    /// - `Envir.ArroundDoorOpened(CurrX,CurrY)`：**已建模**——读本图门表（`MapGrid::arround_door_opened`，
    ///   宿主按注册表灌入）。±1 邻域内有门且该门 `!Opened` ⇒ 整支跳过、**不换图**（oracle 门结构性
    ///   恒未开 ⇒ 门相邻的 route 格恒被挡；无门表 ⇒ 恒 true，合成场景行为不变）；
    /// - `(!mapRoute.Envir.Flag.boNEEDHOLE) || EventMgr.GetEvent(Envir,CurrX,CurrY,ET_DIGOUTZOMBI) != null`：
    ///   **本仓无 `MapEvent` 模型 ⇒ 事件恒不存在** ⇒ 落到 Rust 就是「**目标图 `boNEEDHOLE` 命中即挡**」
    ///   （`PlayObject.cs:1874`）。这是一个**真门**：真实 `MapInfo.txt` 有 `Q004/R001/H007/H106` 四段带
    ///   `NEEDHOLE`，共 **7** 条真实 route 以它们为目标图（见下方 `route_enter` 的登记块）⇒ 这些 route
    ///   在 oracle **不换图**。**将来若建事件模型**：把此判据换回
    ///   `!dst.need_hole() || self.event_at(dst, hop.dst_x, hop.dst_y, ET_DIGOUTZOMBI).is_some()`
    ///   （接口位留在此处，`event_at` 待增）。
    /// - `M2Share.ServerIndex == mapRoute.Envir.ServerIndex`：**单区单 World ⇒ 恒同**（登记；异支
    ///   `ChangeSpaceMove` 跨服搬家**未建**）；
    /// - `EnterAnotherMap` 的其余门（`:3721-3757`）：等级门 `Abil.Level < envir.EnterLevel`（oracle
    ///   `EnterLevel` 是恒 0 的 get-only 属性 ⇒ 照搬"死门"恒过，登记）、`NeedSetonFlag` 门（本仓无任务
    ///   标志 ⇒ 恒过，登记）、`!envir.CellValid(nDMapX,nDMapY)`（**实门**）、castle 门（无城堡注册表 ⇒
    ///   恒过，登记）、`NoHorse`（无骑马字段 ⇒ 不建模，登记）。
    ///
    /// **多 route 终落点**：oracle 顺序处理、**不 break** ⇒ 终落点 = **最后一条**成功换图的 route 目标
    /// （真实数据有重复源格：如 `D5069(90,76)` 同时 `→D5062(8,39)` 与 `→D5071(8,10)`，终落点 = `D5071`）。
    fn route_enter(&mut self, actor_id: i32) -> RouteOutcome {
        let Some(e) = self.entities.get(actor_id) else {
            return RouteOutcome::Skipped;
        };
        // `PlayObject.Walk` 覆写专属 ⇒ 只有玩家（`Race == ActorRace.Play`）触发；非玩家走
        // `BaseObject.Walk` 的 `CellType.MapRoute => result = false`（只挡路、不换图）。
        if e.race != crate::entity::ACTOR_RACE_PLAY {
            return RouteOutcome::Skipped;
        }
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        // 顺序收集该格全部 `MapRoute`（oracle 的 `for` 不 break）——**在换图前**收集，复刻
        // oracle「`cellInfo` 在 `EnterAnotherMap` 改 `Envir` 前已捕获」的语义。
        let route_ids: Vec<i32> = {
            let Some(map) = self.maps.get(map_id) else {
                return RouteOutcome::Skipped;
            };
            // `if (!Envir.CellMatch(CurrX, CurrY)) return true;`
            if !map.cell_match(x, y) {
                return RouteOutcome::Skipped;
            }
            // `if (cellSuccess && cellInfo.IsAvailable)`。
            let (cell, cell_success) = map.get_cell_info(x, y);
            if !(cell_success && cell.is_available()) {
                return RouteOutcome::Skipped;
            }
            cell.obj_list
                .iter()
                .filter(|o| o.cell_type == CellType::MapRoute)
                .map(|o| o.cell_obj_id)
                .collect()
        };
        if route_ids.is_empty() {
            return RouteOutcome::Skipped;
        }
        let mut switched = false;
        let mut failed = false;
        for route_id in route_ids {
            // `M2Share.CellObjectMgr.Get<MapRouteItem>(CellObjId)`；`mapRoute.Envir != null`。
            let Some(hop) = self
                .maps
                .get(map_id)
                .and_then(|m| m.route_hop(route_id).copied())
            else {
                continue;
            };
            // `if (Envir.ArroundDoorOpened(CurrX, CurrY))`（`PlayObject.cs:1872`；
            // `Envirnoment.cs:1185-1203`）：±1 邻域内有门且该门 `!Opened` ⇒ 整支跳过（不换图；
            // `result` 保持 true ⇒ 走位照常、不回滚——与 NEEDHOLE 门同型）。门数据由宿主
            // （`bins/game-svr`）按注册表逐图 `set_doors` 灌入；无门表 ⇒ 恒 true（改动前行为不变）。
            // oracle 门恒未开（`CM_OPENDOOR` 结构性 no-op，见 `design-door-and-mapevent-20261016.md`
            // §2.2）⇒ 与门相邻（±1）的 route 格**恒被挡**。
            let door_ok = self
                .maps
                .get(map_id)
                .is_some_and(|m| m.arround_door_opened(x, y));
            if !door_ok {
                continue;
            }
            // `(!mapRoute.Envir.Flag.boNEEDHOLE) || EventMgr.GetEvent(...) != null`：
            // 无 `MapEvent` 模型 ⇒ 事件恒 null ⇒ 目标图 `boNEEDHOLE` 命中即**挡**（门跳过：result 保持
            // true，不换图、不回滚）。**登记：`Q004/R001/H007/H106` 四段带 `NEEDHOLE`，共 7 条真实
            // route 以它们为目标**（`D402(138,107|138,108|137,107|137,108)→Q004(14,14)`、
            // `D717(80,28)→R001(162,169)`、`H006(83,25)→H007(161,172)`、`h006(80,28)→H106(163,169)`）
            // ⇒ 踩这些源格在 oracle **不换图**（moved 正常、changed_map=false）。
            if self.maps.get(hop.dst_map_id).is_some_and(|m| m.need_hole()) {
                continue;
            }
            // `!envir.CellValid(nDMapX, nDMapY)` ⇒ `EnterAnotherMap` 返回 false。
            let dst_valid = self
                .maps
                .get(hop.dst_map_id)
                .is_some_and(|m| m.cell_valid(hop.dst_x, hop.dst_y));
            if !dst_valid {
                failed = true;
                continue;
            }
            // 复用既有 `switch_map` 落地（旧图摘除 → 新图登记 → 记一条 `MapChange`）。
            let Some(chr_name) = self
                .sessions
                .iter()
                .find(|s| s.actor_id == Some(actor_id))
                .map(|s| s.chr_name.clone())
            else {
                continue;
            };
            if self
                .switch_map(&chr_name, hop.dst_map_id, hop.dst_x, hop.dst_y)
                .is_ok()
            {
                switched = true;
            }
        }
        // oracle `if (result)`：任一 `EnterAnotherMap` 失败 ⇒ result=false（即便另一条成功过，
        // oracle 的 `result` 也被置 false；该「部分成功 + 部分失败」病态混合在真实数据不可达，本仓
        // 按"任一失败 ⇒ 回滚到走前格"处理并登记）。
        if failed {
            RouteOutcome::Failed
        } else if switched {
            RouteOutcome::Switched
        } else {
            RouteOutcome::Skipped
        }
    }

    /// **走前格回滚**（oracle `BaseObject.WalkTo:685-688`）：`Envir.DeleteFromMap(CurrX,CurrY)`
    /// → `CurrX=oldX; CurrY=oldY` → `Envir.AddMapObject(oldX,oldY)`。**不查阻挡**（照 oracle）。
    ///
    /// 用于 [`World::step_with_route`] 在 `EnterAnotherMap` 失败后的坐标回滚。注意 oracle 在
    /// 「先成功换图、后有 route 失败」这种病态混合下会把玩家登记到**目标图**的**源坐标**上（现实
    /// 不可达）；本仓统一按"回到源图走前格"处理（已登记）。
    fn rollback_move(&mut self, actor_id: i32, fx: i32, fy: i32, map_id: usize) {
        let Some(e) = self.entities.get(actor_id) else {
            return;
        };
        let cur_map = e.map_id;
        // 从当前格摘除（可能是目标图，也可能是同图的目标格）。
        self.remove_from_map(actor_id, cur_map);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.map_id = map_id;
            e.x = fx;
            e.y = fy;
        }
        let now = self.clock_ms;
        self.maps[map_id].add_map_object(fx, fy, CellType::Play, actor_id, now);
    }

    /// **同步移动**（`openmir2-m3-move-wiring` 的接线入口）：`steps` = 走 / 跑 / 骑跑的格数（1/2/3）。
    ///
    /// oracle 的 `ClientWalkXY`/`ClientRunXY`/`ClientHorseRunXY` 在**消息处理线程上立即**改坐标
    /// （`WalkTo`/`RunTo`/`HorseRunTo` → `Envir.MoveToMovingObject`），不等 tick；本方法对应
    /// `Command::Walk` 的**同一套** `try_step` 判据（阻挡/格子校验逐字节同构），只是不入队。
    ///
    /// `steps > 1` 时照 oracle 的写法**先逐格判可走、再一次性移动**（`RunTo` 的
    /// `CanWalkEx(cur+1) && CanWalkEx(cur+2) && MoveToMovingObject(cur, cur+2)`；
    /// `HorseRunTo` 为 3 格）：中途任一格不可走 ⇒ 一格都不动（不是走一段再失败）。
    ///
    /// **非玩家 `MapRoute` 阻挡**（`BaseObject.Walk` 的非玩家支）：落点格含 `CellType::MapRoute`
    /// 且 actor `race != ACTOR_RACE_PLAY` ⇒ 拒绝该步（坐标不变、返回 `false`），语义等价 oracle
    /// `BaseObject.Walk` 的 `case CellType.MapRoute: result = false`（`BaseObject.cs:1548`）与**上游
    /// `WalkTo` 回滚**（`DeleteFromMap` → `CurrX=oldX;CurrY=oldY` → `AddMapObject`，`:683-688`）。
    /// **只挡非玩家**——玩家走 `PlayObject.Walk` 覆写（对 MapRoute 走 `EnterAnotherMap` **换图**、
    /// 不是阻断），由 [`World::step_with_route`] 负责（`PlayObject.cs:1868-1892`）。
    ///
    /// 红检（变异牙）：删掉本函数里的 `race != ACTOR_RACE_PLAY` 落点检查 ⇒
    /// `try_step_now` 会被怪物走上 route 格、`step_with_route_does_not_switch_for_monster` 必红。
    #[must_use]
    pub fn try_step_now(&mut self, actor_id: i32, dx: i32, dy: i32, steps: i32) -> bool {
        if steps <= 0 || (dx == 0 && dy == 0) {
            return false;
        }
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (fx, fy, map_id) = (e.x, e.y, e.map_id);
        let race = e.race;
        for k in 1..=steps {
            if !self.cell_free(map_id, fx + dx * k, fy + dy * k, actor_id) {
                return false;
            }
        }
        // 落点格（走完的 `CurrX/CurrY`）含 MapRoute 且非玩家 ⇒ 挡（oracle `BaseObject.cs:1548`
        // `result = false` + `WalkTo` 回滚 `:683-688`）。玩家由 `step_with_route` 的换图负责。
        if race != crate::entity::ACTOR_RACE_PLAY
            && self
                .maps
                .get(map_id)
                .is_some_and(|m| m.has_map_route(fx + dx * steps, fy + dy * steps))
        {
            return false;
        }
        self.try_step(actor_id, dx * steps, dy * steps)
    }

    /// 目标格是否可站（与 `try_step` 的阻挡口径同源：`cell_match` + `valid()/is_available()` +
    /// 阻挡 actor 的 `!Ghost && !Death && !FixedHideMode && !ObMode`；`self_id` 自己不算阻挡）。
    fn cell_free(&self, map_id: usize, tx: i32, ty: i32, self_id: i32) -> bool {
        let Some(map) = self.maps.get(map_id) else {
            return false;
        };
        if !map.cell_match(tx, ty) {
            return false;
        }
        let (cell, ok) = map.get_cell_info(tx, ty);
        if !ok {
            return false;
        }
        if !(cell.valid() && cell.is_available()) {
            return true;
        }
        !cell.obj_list.iter().any(|o| {
            o.actor_object
                && o.cell_obj_id != self_id
                && self
                    .entities
                    .get(o.cell_obj_id)
                    .is_some_and(|e| !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode)
        })
    }

    /// 单步移动：格子校验 → 更新坐标。
    fn try_step(&mut self, actor_id: i32, dx: i32, dy: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (fx, fy, map_id) = (e.x, e.y, e.map_id);
        let (tx, ty) = (fx + dx, fy + dy);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        if !map.cell_match(tx, ty) {
            return false;
        }
        let blockers: Vec<i32> = {
            let (cell, _) = map.get_cell_info(tx, ty);
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
        let now = self.clock_ms;
        let moved = map.move_object(
            crate::map::MoveRequest {
                from_x: fx,
                from_y: fy,
                actor_id,
                to_x: tx,
                to_y: ty,
                ignore_block: false,
                add_time: now,
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

    /// **出站接线片 W2**：`BaseObject.CharPushed`（`src/M2Server/Actor/BaseObject.cs:854-901`）的
    /// **世界侧**转发（帧面在宿主）。
    ///
    /// 逐格照 oracle 的 `for` 循环：`GetFrontPosition(Dir = nDir)` → `CanWalk(nx, ny, false)`
    /// （= [`World::cell_free`]，含越界/墙/活体阻挡）→ 走一格 → 记一条
    /// [`crate::action::PushEvent`]（`dir = nBackDir`、位置 = 走**后**的 `CurrX/CurrY`）。
    /// 任一步不可走即 `break`（oracle 的 `break`）。返回的事件表**逐格一条、保序**，
    /// `len` = oracle `CharPushed` 的返回值 `result`。
    ///
    /// **不建模**（登记）：`if (Race >= ActorRace.Animal) WalkTick += 800`（`WalkTick` 不在本骨架）、
    /// 以及 `Dir` 的落盘（`CharPushed` 会把 `Dir` 改成 `nBackDir`/`olddir`——本骨架 `Dir` 属宿主展示表，
    /// 帧值用的是 `nBackDir` 本身，故不影响帧字节）。
    fn push_now(&mut self, actor_id: i32, n_dir: u8, count: u8) -> Vec<crate::action::PushEvent> {
        let Some(e) = self.entities.get(actor_id) else {
            return Vec::new();
        };
        let (mut x, mut y, map_id) = (e.x, e.y, e.map_id);
        let Some((w, h)) = self.maps.get(map_id).map(|m| (m.width, m.height)) else {
            return Vec::new();
        };
        let back = crate::magic::back_dir(n_dir);
        let mut out = Vec::new();
        for _ in 0..count {
            let (nx, ny) = crate::magic::front_position(x, y, n_dir, w, h);
            // oracle `CanWalk(nx, ny, false)`：越界/墙/活体皆不可走 ⇒ break。
            if !self.cell_free(map_id, nx, ny, actor_id) {
                break;
            }
            if !self.try_step(actor_id, nx - x, ny - y) {
                break;
            }
            x = nx;
            y = ny;
            out.push(crate::action::PushEvent {
                actor_id,
                dir: back,
                x,
                y,
            });
        }
        out
    }

    /// `Envirnoment.CanWalk(nx, ny, true)` 的 `cellSuccess` 子集（`Envirnoment.cs:442-486` 的
    /// **boFlag=true** 支：只判 `cellSuccess && Valid`，不查格上对象）——`DoMotaebo` 的
    /// `if (Envir.CanWalk(nX, nY, true)) n28 = 0;`（`src/M2Server/Player/PlayObject.Base.cs:1461`）。
    fn can_walk_ignore_objects(&self, map_id: usize, x: i32, y: i32) -> bool {
        self.maps
            .get(map_id)
            .is_some_and(|m| m.get_cell_info(x, y).1)
    }

    /// **出站接线片 W2**：`PlayObject.DoMotaebo`（`src/M2Server/Player/PlayObject.Base.cs:1389-1490`）
    /// 的**无正前目标**支（`else`，`:1443-1467`）的世界侧转发。
    ///
    /// 向前冲刺至多 `steps` 格：每成功一格发 `SendRefMsg(RM_RUSH, nDir, CurrX, CurrY, 0, "")`
    /// ⇒ 记 [`crate::action::RushEvent`]（`frame = Rush`、`dir = nDir`、位置 = 走**后**格）；
    /// 被挡时若正前**不可走**（`CanWalk(nX, nY, true)` 为假）⇒ `bo35`，循环末尾 `GetFrontPosition`
    /// 后再发 `SendRefMsg(RM_RUSHKUNG, Dir, nX, nY, 0, "")` ⇒ 追加一条 `frame = RushKung` 事件
    /// （位置 = **被挡的正前格**，角色不在该格上）。正前可走但被活体挡 ⇒ 照 oracle 继续循环
    /// （`n28 = 0` 只影响伤害，非本例建模面）。
    ///
    /// **不在本片**（登记）：有正前目标支（`:1404-1440`：`CharPushed` 目标 + 进占其旧格 + 伤害）、
    /// `bo35 == false` 时的 `RM_STRUCK` 伤害段（`:1469-1486`）。
    fn rush_now(&mut self, actor_id: i32, n_dir: u8, steps: i32) -> Vec<crate::action::RushEvent> {
        if steps <= 0 {
            return Vec::new();
        }
        let Some(e) = self.entities.get(actor_id) else {
            return Vec::new();
        };
        let (mut x, mut y, map_id) = (e.x, e.y, e.map_id);
        let Some((w, h)) = self.maps.get(map_id).map(|m| (m.width, m.height)) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        let mut blocked_front = false;
        for _ in 0..steps {
            let (nx, ny) = crate::magic::front_position(x, y, n_dir, w, h);
            if self.try_step(actor_id, nx - x, ny - y) {
                x = nx;
                y = ny;
                out.push(crate::action::RushEvent {
                    actor_id,
                    frame: crate::action::RushFrame::Rush,
                    dir: n_dir,
                    x,
                    y,
                });
            } else if !self.can_walk_ignore_objects(map_id, nx, ny) {
                // `CanWalk(nX, nY, true)` 为假 ⇒ `bo35 = true; break;`
                blocked_front = true;
                break;
            }
        }
        if blocked_front {
            let (nx, ny) = crate::magic::front_position(x, y, n_dir, w, h);
            out.push(crate::action::RushEvent {
                actor_id,
                frame: crate::action::RushFrame::RushKung,
                dir: n_dir,
                x: nx,
                y: ny,
            });
        }
        out
    }

    /// **出站接线片 W2**：`RM_BUTCH`（`src/M2Server/Player/PlayObject.Operate.cs:749`）的世界侧事件。
    ///
    /// oracle 末尾**无条件** `SendRefMsg(RM_BUTCH, Dir, CurrX, CurrY, 0, "")`；`Dir` 由宿主
    /// （`CM_BUTCH` 门面）按 oracle 的「2 格 + `IsValidObject` + 具肉动物尸」门解析后传入——本片
    /// 不含该入站门面，故事件只承载 `(dir, CurrX, CurrY)`。actor 不在世界 ⇒ 无事件。
    fn butch_now(&self, actor_id: i32, dir: u8) -> Option<crate::action::ButchEvent> {
        let e = self.entities.get(actor_id)?;
        Some(crate::action::ButchEvent {
            actor_id,
            dir,
            x: e.x,
            y: e.y,
        })
    }

    /// 对拍用地面物品清单：`(map_id, x, y, obj_id)`，按 `(map, x, y, obj_id)` 升序。
    ///
    /// 对应 C# 侧驱动从各 `EnvirnomentStub.CellArray` 扫出来的 `CellType.Item` 项。
    pub fn floor_items_sorted(&self) -> Vec<(usize, i32, i32, i32)> {
        let mut out = Vec::new();
        for (map_id, map) in self.maps.iter().enumerate() {
            for x in 0..map.width {
                for y in 0..map.height {
                    let (cell, ok) = map.get_cell_info(x, y);
                    if !ok {
                        continue;
                    }
                    for o in &cell.obj_list {
                        if o.cell_type == CellType::Item {
                            out.push((map_id, x, y, o.cell_obj_id));
                        }
                    }
                }
            }
        }
        out.sort_unstable();
        out
    }

    /// 地面物品的**最小身份视图**（world 侧只读分片，owner §25.1 裁决 (A)）：
    /// 每个 `CellType::Item` 格子对象一条，按 `(map, x, y, obj_id, item_id)` 升序。
    ///
    /// 字段全部来自 world 在**掉落落地时**的记账（`FloorItemMeta`），不派生、不改模拟语义；
    /// 接线层拿它 + item_catalog 装配 `ground::GroundItem`，不需要反推金币 `count` 或归属。
    /// `can_pick_up_tick` = 落地时间（`CellObject.add_time` 的虚拟时钟转 `i32`）。
    pub fn floor_items_view_sorted(&self) -> Vec<FloorItemView> {
        let mut out = Vec::new();
        for (map_id, map) in self.maps.iter().enumerate() {
            for x in 0..map.width {
                for y in 0..map.height {
                    let (cell, ok) = map.get_cell_info(x, y);
                    if !ok {
                        continue;
                    }
                    for o in &cell.obj_list {
                        if o.cell_type != CellType::Item {
                            continue;
                        }
                        let Some(meta) = o.item else {
                            continue;
                        };
                        out.push(FloorItemView {
                            map_id,
                            x,
                            y,
                            obj_id: o.cell_obj_id,
                            kind: meta.kind,
                            index: meta.index,
                            count: meta.count,
                            item_id: meta.item_id,
                            of_base_object: meta.of_base_object,
                            can_pick_up_tick: o.add_time as i32,
                        });
                    }
                }
            }
        }
        out.sort_unstable_by_key(|v| (v.map_id, v.x, v.y, v.obj_id, v.item_id));
        out
    }

    /// 对拍用快照：全部实体的 `(id, map, x, y, hp, level, exp, 视野列表)`，**按 id 升序**。
    pub fn snapshot_sorted(&self) -> Vec<EntitySnapshot> {
        let mut ids = self.entities.ids_sorted();
        ids.sort_unstable();
        ids.into_iter()
            .filter_map(|id| {
                self.entities.get(id).map(|e| {
                    (
                        e.id,
                        e.map_id,
                        e.x,
                        e.y,
                        e.hp,
                        e.level,
                        e.exp,
                        crate::aoi::visible_snapshot(&self.entities, id),
                    )
                })
            })
            .collect()
    }
}

/// 对拍/输出用的实体快照：`(id, map_id, x, y, hp, level, exp, 视野列表[(目标 id, 标志)])`。
pub type EntitySnapshot = (i32, usize, i32, i32, u16, u8, i32, Vec<(i32, u8)>);

/// 地面物品的最小身份视图（[`World::floor_items_view_sorted`] 的元素）。
///
/// `obj_id` = `CellObject.cell_obj_id`（**与 `floor_items_sorted` 同口径**，不变）；
/// `item_id` 是拾取/删除格子的**唯一键**（物品 = `UserItem.MakeIndex`，金币 = 落主 `ActorId`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FloorItemView {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    pub obj_id: i32,
    pub kind: FloorItemKind,
    /// `StdItem` 序号（1-based）；金币 = 0。
    pub index: u16,
    /// 金币数 / 物品数（物品恒 1）。
    pub count: i32,
    /// 唯一 id。
    pub item_id: i32,
    /// 掉落归属者（0 = 任何人可拾）。
    pub of_base_object: i32,
    /// 可拾起时刻（落地时的虚拟时钟）。
    pub can_pick_up_tick: i32,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::{Entity, MonsterCtorDraws, ACTOR_RACE_MONSTER, ACTOR_RACE_PLAY};

    /// **世界 tick 扩容首批（T3）· ② 世界时钟（C-L2）**：`refresh_game_time` 只在桶值**变化**时
    /// 返回 `true`（oracle `PlayObject.Base.cs:826` 的 `Bright != M2Share.GameTime` 门）；
    /// 同值重复调用 ⇒ `false`（不发帧）。改坏必红：把 `!=` 写成 `==`、或恒返回 `true` ⇒ 红。
    #[test]
    fn refresh_game_time_reports_change_only_on_difference() {
        let mut w = World::new(8, 8);
        // 初值 = oracle `M2Share.GameTime = 0`。
        assert_eq!(w.game_time, 0);
        // 桶值 1（白天）：0→1 变化。
        assert!(w.refresh_game_time(1), "0→1 应报变化");
        assert_eq!(w.game_time, 1);
        // 同值：不变。
        assert!(!w.refresh_game_time(1), "同值不得报变化");
        assert_eq!(w.game_time, 1);
        // 跨桶：1→2（日落）。
        assert!(w.refresh_game_time(2), "1→2 应报变化");
        assert_eq!(w.game_time, 2);
        // 回到初值桶 0：变化（oracle 同样会发）。
        assert!(w.refresh_game_time(0), "2→0 应报变化");
        assert_eq!(w.game_time, 0);
    }

    /// **C-L3（跨桶）**：把一个「小时」映射为桶值（复用 `enter::game_time_from_hour` 的同口径
    /// 公式在此以字面量表达）后断言跨小时边界 `10:59 → 11:00` 使桶 `1 → 2`、`refresh_game_time`
    /// 报变化（⇒ 宿主出 1 帧 46）。这是 oracle `GetGameTime` 的桶切换。
    ///
    /// 注：本测试只钉 `refresh_game_time` 的**变化检测**；`hour→bucket` 映射的 24 值全覆盖在
    /// `bins/game-svr/src/enter.rs` 的 `C-L1`（唯一真源，不在此复制）。
    #[test]
    fn refresh_game_time_crosses_hour_bucket() {
        let mut w = World::new(8, 8);
        // 10 点 = 白天(1)（`GetGameTime`：`5..=10 | 16..=19 | 21 ⇒ 1`）。
        assert!(w.refresh_game_time(1));
        // 11 点 = 日落(2)（`11 | 20 | 23 ⇒ 2`）⇒ 跨小时边界翻桶。
        assert!(w.refresh_game_time(2), "10:59→11:00 应翻桶");
        // 11 点内不再变。
        assert!(!w.refresh_game_time(2));
    }

    /// **T3·①（本批最重要的判据）：`world.tick` 内 `npc_tick` 必先于 `merchant_tick`
    /// 消费共享 RNG**（oracle `MerchantProcessor.ExecuteInternal` 先 `ProcessNpcs()` 后
    /// `ProcessMerchants()`，`MerchantProcessor.cs:28-32`）。
    ///
    /// 同一 tick 内让一个固定 NPC（`GuildOfficial`）与一个商人（`Merchant`）**都过 Run 门**，
    /// 再以一个同种子独立流按**字面量 oracle 参数**复算合并后的抽数序列：先 NPC 体
    /// （`Random(40)` → [`RandomByte(8)` | `Random(30)`]），后商人体
    /// （`Random(50)` → [`RandomByte(8)` | `Random(50)`]）。
    ///
    /// **牙**：把 `world.rs` 里 `if self.npc_enabled {…}` 与 `if self.merchant_enabled {…}`
    /// 两块**对调**（RNG 消费序反转）⇒ 期望序列错位 ⇒ 必红。扫 **20000** 种子
    /// （`40/30/50` 模数 ±1 的区分窗口 ~1/1600/seed ⇒ 须大样本才咬得住）。
    #[test]
    fn world_tick_consumes_rng_npc_before_merchant() {
        use crate::merchant::{MerchantFlags, MerchantSpawn, MerchantState};
        use crate::npc::{NpcSpawn, NpcState, NPC_TYPE_GUILD_OFFICIAL};

        // 世界/两实体建一次；每轮重播种共享流并把两块 tick 的门都拨到「必跑」。
        let mut w = World::with_maps(vec![(20, 20)]);
        w.enable_npc_tick();
        w.enable_merchant_tick();
        let npc = w
            .spawn_npc(NpcSpawn {
                name: "比奇国王".to_string(),
                x: 3,
                y: 3,
                map_id: 0,
                state: NpcState {
                    npc_type: NPC_TYPE_GUILD_OFFICIAL,
                    ..NpcState::default()
                },
            })
            .expect("NPC 挂图成功");
        let mer = w
            .spawn_merchant(MerchantSpawn {
                name: "老人".to_string(),
                x: 4,
                y: 4,
                map_id: 0,
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
            })
            .expect("商人挂图成功");

        for seed in 0..20_000i32 {
            // 重播种共享流（无需复算 ctor 抽数：重播种抹掉 ctor 已消费的那几抽）。
            w.rng = RandomNumber::with_seed(seed);
            let mut exp = RandomNumber::with_seed(seed);
            if let Some(n) = w.entities.get_mut(npc).and_then(|e| e.npc.as_mut()) {
                n.run_tick = 0;
            }
            if let Some(s) = w.entities.get_mut(mer).and_then(|e| e.merchant.as_mut()) {
                s.run_tick = 0;
            }
            w.npc_pass_tick = i64::MIN;
            w.merchant_pass_tick = i64::MIN;
            w.clock_ms = 10_000;
            let rec = w.tick();

            // 期望（**字面量 oracle 参数**）：先 NPC 体 40→[8|30]，后商人体 50→[8|50]。
            let npc_expect = if exp.random_below(40) == 0 {
                (Some(exp.random_byte(8)), false)
            } else {
                (None, exp.random_below(30) == 0)
            };
            let mer_expect = if exp.random_below(50) == 0 {
                (Some(exp.random_byte(8)), false)
            } else {
                (None, exp.random_below(50) == 0)
            };

            assert_eq!(rec.npcs_run, vec![npc], "seed={seed}: NPC 先过 Run 门");
            assert_eq!(rec.merchants_run, vec![mer], "seed={seed}: 商人后过 Run 门");
            assert_eq!(
                (
                    rec.npc_turns.first().map(|t| t.1),
                    rec.npc_hits.contains(&npc)
                ),
                npc_expect,
                "seed={seed}: NPC 体先抽（40→[8|30]）"
            );
            assert_eq!(
                (
                    rec.merchant_turns.first().map(|t| t.1),
                    rec.merchant_hits.contains(&mer)
                ),
                mer_expect,
                "seed={seed}: 商人体后抽（50→[8|50]）"
            );
            assert_eq!(
                w.rng.random(),
                exp.random(),
                "seed={seed}: 合并抽数次序/次数"
            );
        }
    }

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

    #[test]
    fn tick_order_is_deterministic_and_documented() {
        let mut w = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w.walk(2, 1, 0);
        w.walk(1, 0, 1);
        let r = w.tick();
        assert_eq!(
            r.processed,
            vec![(1, true), (2, true), (3, true), (2, true), (1, true)],
            "命令必须按 FIFO 处理"
        );
        assert_eq!(r.aoi_order, vec![1, 2, 3]);

        let mut w2 = world_with(&[(1, 10, 10), (2, 12, 10), (3, 20, 20)]);
        w2.walk(2, 1, 0);
        w2.walk(1, 0, 1);
        let r2 = w2.tick();
        assert_eq!(r.processed, r2.processed);
        assert_eq!(r.aoi_order, r2.aoi_order);
    }

    /// `openmir2-m3-move-wiring`：同步移动（`ClientWalkXY`/`RunTo`/`HorseRunTo` 口径）。
    /// - 1/2/3 格的 `steps` 语义；
    /// - `steps > 1` 时**任一中间格不可走 ⇒ 一格都不动**（oracle `RunTo` 先 `CanWalkEx(cur+1) &&
    ///   CanWalkEx(cur+2)` 再 `MoveToMovingObject`，不是走一段再失败）；
    /// - `(0,0)` / `steps <= 0` 拒绝。
    #[test]
    fn try_step_now_is_synchronous_and_atomic() {
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick(); // Enter 落格
        assert!(w.try_step_now(1, 1, 0, 1));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (6, 5),
            "走 1 格"
        );
        assert!(w.try_step_now(1, 1, 0, 2));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (8, 5),
            "跑 2 格"
        );
        assert!(w.try_step_now(1, 0, 1, 3));
        assert_eq!(
            (w.entities.get(1).unwrap().x, w.entities.get(1).unwrap().y),
            (8, 8),
            "骑跑 3 格"
        );
        assert!(!w.try_step_now(1, 0, 0, 1), "(0,0) 拒绝");
        assert!(!w.try_step_now(1, 1, 0, 0), "steps<=0 拒绝");

        // 中间格被另一个 actor 占住 ⇒ 2 格跑一格都不动（整段原子）。
        let mut w2 = world_with(&[(1, 5, 5), (2, 6, 5)]);
        w2.tick();
        assert!(!w2.try_step_now(1, 1, 0, 2), "第 1 格被占 ⇒ 整段不动");
        assert_eq!(
            (w2.entities.get(1).unwrap().x, w2.entities.get(1).unwrap().y),
            (5, 5)
        );
    }

    #[test]
    fn aoi_boundary_at_view_range() {
        let mut w = world_with(&[(1, 30, 30), (2, 35, 30), (3, 36, 30)]);
        w.tick();
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let targets: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert!(targets.contains(&2), "dx=ViewRange 必须可见（闭区间）");
        assert!(!targets.contains(&3), "dx=ViewRange+1 不可见");
    }

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

    #[test]
    fn session_enter_leave_world() {
        let mut w = World::new(16, 16);
        let id = w.add_entity(Entity::new(0, "p", 5, 5));
        w.tick();
        assert!(w.entities.get(id).unwrap().add_to_mapped);
        assert_eq!(w.map(0).cell(5, 5).count(), 1);
        w.leave(id);
        w.tick();
        assert!(w.entities.get(id).is_none(), "离开世界后实体应从仓库移除");
        assert_eq!(w.map(0).cell(5, 5).count(), 0);
    }

    fn player(id: i32, name: &str, x: i32, y: i32, level: u8) -> Entity {
        let mut e = Entity::new(id, name, x, y);
        e.race = ACTOR_RACE_PLAY;
        e.level = level;
        e.hp = 200;
        e.max_hp = 200;
        e.dc = 0x0A_14; // LoByte=20, HiByte=10 → 攻击力 20+(0..0)（差为负 → 归零）
        e.ac = 0x00_00;
        e.hit_point = 10;
        e.speed_point = 0;
        e.max_exp = 1000;
        e
    }

    /// 战斗子集：固定种子下攻击结算可复现（判据①「可证伪」）。
    #[test]
    fn attack_outcome_is_deterministic() {
        let build = || {
            let mut w = World::new(32, 32);
            let p = w.add_entity(player(1, "hero", 10, 10, 10));
            w.spawn_monster(Command::SpawnMonster {
                id: 2,
                name: "鸡".into(),
                x: 11,
                y: 10,
                map_id: 0,
                level: 1,
                exp: 10,
                hp: 5,
                dc: 1,
                ac: 0,
                undead: 0,
                hit_point: 5,
                speed_point: 15,
                drop_list: vec![],
            });
            w.tick();
            w.attack_cmd(p, 2);
            let rec = w.tick();
            rec.attacks[0].2
        };
        let a = build();
        let b = build();
        assert_eq!(a, b, "同种子同输入的攻击结算必须一致");
        assert!(
            a.power >= 20,
            "攻击力下限 = LoByte(DC) = 20，实际 {}",
            a.power
        );
        assert!(a.damage > 0, "护甲 0 时伤害应大于 0");
    }

    /// 死亡 → 掉落**生成** + 击杀者得经验；再 tick 不重复结算。
    /// oracle 语义下掉落恒不落地（`AddItemToMap` 的 `MapItem.ItemId == 0` 门，同 T-25）。
    #[test]
    fn monster_death_drops_items_and_awards_exp() {
        let mut w = World::new(32, 32);
        // 经验表：1 级需 1000，2 级需 2000（自造，只为验证升级路径）
        w.need_exps[1] = 1000;
        w.need_exps[2] = 2000;
        w.mon_random_add_value = 1; // 必出极品（消耗 RNG 的分支也要覆盖）
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "鸡".into(),
            x: 11,
            y: 10,
            map_id: 0,
            level: 1,
            exp: 500,
            hp: 3,
            dc: 1,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            drop_list: vec![mir2_data::models::MonsterDropItem {
                sel_point: 9,
                max_point: 10,
                item_name: "金币".into(),
                count: 100,
            }],
        });
        w.tick(); // 生成（掉落预算在这里消耗 RNG）
        let exp_before = w.entities.get(p).unwrap().exp;
        w.attack_cmd(p, 2);
        let rec = w.tick(); // 攻击 → HP 归零；同 tick 内死亡结算
        assert_eq!(rec.deaths.len(), 1, "应有 1 次死亡结算: {rec:?}");
        let (dead_id, placed, gold, killer, gained) = rec.deaths[0];
        assert_eq!(dead_id, 2);
        assert!(gold > 0, "金币掉落应大于 0");
        assert_eq!(placed, 0, "oracle 语义 ⇒ 掉落恒不落地（T-19/T-25 同根）");
        assert_eq!(killer, Some(p));
        assert!(gained > 0, "击杀者应获得经验");
        assert_eq!(w.entities.get(p).unwrap().exp, exp_before + gained);
        assert!(w.entities.get(2).unwrap().death);
        // 再 tick 不重复结算
        let rec2 = w.tick();
        assert!(rec2.deaths.is_empty(), "死亡只能结算一次: {rec2:?}");
    }

    /// `KillFunc` 的 `ExpHitter.Master != null` 支（`BaseObject.cs:2434-2437`）：宠物/下属击杀 ⇒
    /// 经验给**主人**；`TickRecord::exp_awards` 列出获得者（供宿主出 `SM_WINEXP(44)`）。
    /// 改坏必红：删掉 master 支（只给 `ExpHitter` 本人）⇒ 主人 `exp` 不变、`exp_awards` 空。
    #[test]
    fn pet_kill_awards_exp_to_master() {
        let mut w = World::new(32, 32);
        w.need_exps[1] = 1000;
        let master = w.add_entity(player(1, "hero", 10, 10, 5));
        let pet = {
            let mut e = Entity::new(0, "pet", 11, 10);
            e.race = ACTOR_RACE_MONSTER; // 非 Play ⇒ `KillFunc` 只走 master 支
            e.master = Some(master);
            w.add_entity(e)
        };
        // 直接造一只 `hp=0`、`ExpHitter = 宠物` 的怪（本用例只测经验归属，不走攻击路径）。
        let mon = {
            let mut e = Entity::new(0, "鸡", 12, 10);
            e.race = ACTOR_RACE_MONSTER;
            e.mon_exp = 400;
            e.exp_hitter = Some(pet);
            w.add_entity(e)
        };
        w.entities.get_mut(mon).unwrap().hp = 0;
        let exp_before = w.entities.get(master).unwrap().exp;
        let pet_exp_before = w.entities.get(pet).unwrap().exp;
        let rec = w.tick();
        assert_eq!(rec.exp_awards.len(), 1, "只有 master 一支: {rec:?}");
        let (winner, amount, _leveled, _level) = rec.exp_awards[0];
        assert_eq!(winner, master, "经验归属者 = ExpHitter.Master");
        assert!(amount > 0, "master 应获得经验");
        assert_eq!(w.entities.get(master).unwrap().exp, exp_before + amount);
        assert_eq!(
            w.entities.get(pet).unwrap().exp,
            pet_exp_before,
            "宠物本人（非 Play race）不获得经验"
        );
        assert_eq!(rec.deaths[0].4, amount, "deaths 的 5 元组口径 = 主获得者");
    }

    /// `Envir.Flag.boEXPRATE`/`ExpRate`（`PlayObject.Base.cs:143-146`）：地图倍率在
    /// `KillMonExpRate` 之后、`GetExp` 之前施加。
    /// 改坏必红：不乘地图倍率（或乘错次序）⇒ 本用例的 `rate_amt` 断言必红。
    #[test]
    fn map_exp_rate_multiplies_kill_exp() {
        let build = |rate: Option<i32>| {
            let mut w = World::new(32, 32);
            w.need_exps[1] = 1_000_000; // 不升级，只看单次经验
            w.set_map_exp_rate(0, rate);
            let p = w.add_entity(player(1, "hero", 10, 10, 5));
            let mon = {
                let mut e = Entity::new(0, "鸡", 11, 10);
                e.race = ACTOR_RACE_MONSTER;
                e.mon_exp = 400;
                e.exp_hitter = Some(p);
                w.add_entity(e)
            };
            w.entities.get_mut(mon).unwrap().hp = 0;
            let rec = w.tick();
            (rec.exp_awards[0].1, w.entities.get(p).unwrap().exp)
        };
        let (base_amt, base_total) = build(None);
        let (rate_amt, rate_total) = build(Some(200));
        assert_eq!(
            base_amt, 400,
            "无 EXPRATE 时 = mon_exp（KillMonExpRate=100）"
        );
        assert_eq!(rate_amt, 800, "EXPRATE=200 ⇒ 单次经验 ×2");
        assert_eq!(rate_total, base_total * 2, "累计经验同样 ×2");
    }

    /// `GetExp`（`PlayObject.Base.cs:149-165`）的升级块触发口径：`exp_awards` 的第 3 位 = **进了**
    /// `Abil.Exp >= Abil.MaxExp` 块（不是「等级真的 +1」）——`Abil.Level++` 才有 `< MAXUPLEVEL(255)`
    /// 门，而块内 `HasLevelUp`/`IncHealthSpell(2000,2000)` **无条件**发生 ⇒ 满级仍要置位（宿主据此出
    /// `SM_LEVELUP(45)`/`SM_ABILITY(52)`/`SM_SUBABILITY(752)`/`SM_HEALTHSPELLCHANGED(53)`）。
    ///
    /// 改坏必红：把 `leveled` 退回「仅 `level < 255` 时置位」⇒ 满级用例的 `leveled` 断言必红。
    #[test]
    fn kill_exp_marks_levelup_block_even_at_max_level() {
        let build = |level: u8, exp: i32, need255: i32| {
            let mut w = World::new(32, 32);
            w.need_exps[1] = 1000;
            w.need_exps[255] = need255;
            let mut p = player(1, "hero", 10, 10, level);
            p.exp = exp;
            p.max_exp = if level == 255 { need255 } else { 1000 };
            let p = w.add_entity(p);
            let mon = {
                let mut e = Entity::new(0, "鸡", 11, 10);
                e.race = ACTOR_RACE_MONSTER;
                e.mon_exp = 100;
                e.exp_hitter = Some(p);
                w.add_entity(e)
            };
            w.entities.get_mut(mon).unwrap().hp = 0;
            let rec = w.tick();
            (rec.exp_awards[0].2, w.entities.get(p).unwrap().level)
        };
        // 1 级：exp 990 + 100 ⇒ 进升级块 + 等级 +1
        let (leveled, level) = build(1, 990, 0);
        assert!(leveled, "exp >= max_exp ⇒ leveled");
        assert_eq!(level, 2);
        // 255 级：同样进块（`HasLevelUp`/回血要发生），但 `Level++` 被 MAXUPLEVEL 门挡住
        let (leveled, level) = build(255, 1000, 1000);
        assert!(leveled, "满级仍进升级块（oracle 的 if 而非 while）");
        assert_eq!(level, 255, "MAXUPLEVEL 门 ⇒ 等级不变");
    }

    /// **oracle 忠实语义（M3④-D 掉落侧）**：死亡掉落（物品 + 金币）**恒不落地**。
    ///
    /// oracle `DieDropItems`（`BaseObject.cs:2450-2464`）逐件走 `DropItemDown`/`DropGoldDown`，
    /// 而 `Envir.AddItemToMap` 的**首行**是 `if (mapItem.ItemId == 0) return false;`
    /// （`Envirnoment.cs:88-92`）——oracle 全仓无 `MapItem.ItemId` 赋值点（`MapItem.cs:41` 被注释）
    /// ⇒ 掉落物加不进地图、`RM_ITEMSHOW`/`SM_ITEMSHOW(610)` 恒不发（同 M3① T-19/T-25 根因）。
    ///
    /// 本用例同时钉住「掉落**生成**不受影响」（金币预算仍 > 0）与「只读分片机制仍可工作」
    /// （显式落格后视图与格子状态逐项一致）。
    /// **红检**：把 `settle_deaths` 改回「落地」（重新写 `add_floor_item`）⇒ 本用例必红。
    #[test]
    fn monster_death_drops_do_not_land_matches_oracle_add_item_to_map_gate() {
        let mut w = World::new(32, 32);
        // 1-based 查表：下标 0 → `index = 1`（与 `combat.json` 的 `金创药(小量)` 同口径）
        w.item_catalog = vec![StdItem {
            name: "金创药(小量)".into(),
            dura_max: 1,
            ..Default::default()
        }];
        w.mon_random_add_value = 1; // 必出极品（消耗 RNG 的分支也覆盖）
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "僵尸".into(),
            x: 11,
            y: 10,
            map_id: 0,
            level: 1,
            exp: 500,
            hp: 3,
            dc: 1,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            // 金币 + 普通物品双掉落（与 `world/scenarios/combat.json` 的 `僵尸` 同形）
            drop_list: vec![
                MonsterDropItem {
                    sel_point: 9,
                    max_point: 10,
                    item_name: "金币".into(),
                    count: 300,
                },
                MonsterDropItem {
                    sel_point: 9,
                    max_point: 10,
                    item_name: "金创药(小量)".into(),
                    count: 1,
                },
            ],
        });
        w.tick(); // 生成（掉落预算在这里消耗 RNG）
        w.attack_cmd(p, 2);
        let rec = w.tick(); // 攻击 → 死亡结算
        assert_eq!(rec.deaths.len(), 1, "应有 1 次死亡结算: {rec:?}");
        // 掉落**生成**仍在（金币预算 > 0），但 **oracle 语义下恒不落地**（`placed` 恒 0）。
        assert!(rec.deaths[0].2 > 0, "本用例需金币掉落预算: {rec:?}");
        assert_eq!(rec.deaths[0].1, 0, "oracle `AddItemToMap` 门 ⇒ 落地数恒 0");
        assert!(
            w.floor_items_sorted().is_empty(),
            "oracle 语义 ⇒ 地面物表恒空: {:?}",
            w.floor_items_sorted()
        );
        assert!(
            w.floor_items_view_sorted().is_empty(),
            "oracle 语义 ⇒ 地面物视图恒空"
        );

        // 只读分片机制本身仍须正确：显式落格（非 0 唯一键）后视图与格子状态逐项一致。
        use crate::map::FloorItemMeta;
        assert!(w.maps[0].add_floor_item(
            12,
            11,
            1,
            FloorItemMeta {
                kind: FloorItemKind::Item,
                index: 1,
                count: 1,
                item_id: 77,
                of_base_object: 5,
            },
            0,
        ));
        assert!(w.maps[0].add_floor_item(
            13,
            11,
            2,
            FloorItemMeta {
                kind: FloorItemKind::Gold,
                index: 0,
                count: 300,
                item_id: 2,
                of_base_object: 5,
            },
            0,
        ));
        let view = w.floor_items_view_sorted();
        // 与既有对拍口径一致：`floor_items_sorted` 的 `(map,x,y,obj_id)` 必须与视图逐项相同
        let legacy: Vec<(usize, i32, i32, i32)> = view
            .iter()
            .map(|v| (v.map_id, v.x, v.y, v.obj_id))
            .collect();
        assert_eq!(
            legacy,
            w.floor_items_sorted(),
            "视图的 obj_id 口径必须与 floor_items_sorted 相同"
        );

        let gold = view
            .iter()
            .find(|v| v.kind == FloorItemKind::Gold)
            .expect("应有金币地面物");
        assert_eq!(
            (gold.index, gold.count, gold.item_id, gold.of_base_object),
            (0, 300, 2, 5)
        );
        let item = view
            .iter()
            .find(|v| v.kind == FloorItemKind::Item)
            .expect("应有普通物品地面物");
        assert_eq!(
            (item.index, item.count, item.item_id, item.of_base_object),
            (1, 1, 77, 5)
        );
        // 可拾起时刻 = 落地时的虚拟时钟（非 None/占位 0 的构造口径）
        assert!(view.iter().all(|v| v.can_pick_up_tick >= 0));
    }

    /// 攻击目标判定（`is_proper_target`）：F-K 修复后按 oracle `IsProperTarget` 忠实建模。
    ///
    /// 关键变化（原 `is_attack_target` 子集 ⇒ oracle）：**`HAM_ALL`（新角色默认 `AttatckMode=0`）
    /// 下允许 PvP**（`PlayObject.Attack.cs:632-640` 对 `Race=Play` 目标 `result=true`，再经
    /// `IsProtectTarget` 安全区门）；自己宝宝在 `HAM_ALL` 下也被允许（`PlayObject.cs:1944-1948`），
    /// 非 `HAM_ALL` 才拒。
    #[test]
    fn attack_target_rules_follow_oracle_proper_target() {
        use crate::combat::{is_proper_target, ATTACK_MODE_HAM_ALL, ATTACK_MODE_HAM_PEACE};

        let mut attacker = player(1, "hero", 10, 10, 10);
        let other = player(2, "rival", 11, 10, 10);
        // 新角色 `PlayObject.cs:908`：HAM_ALL。
        assert_eq!(attacker.attack_mode, ATTACK_MODE_HAM_ALL);
        // PvP：HAM_ALL 且非安全区 ⇒ 允许（旧子集恒 false，F-K 的真实缺口）。
        assert!(
            is_proper_target(&attacker, &other),
            "HAM_ALL 非安全区应允许 PvP"
        );
        // 自己：恒拒。
        assert!(!is_proper_target(&attacker, &attacker));
        // 安全区：PvP 拒；怪仍可打（安全区只挡 Play 目标）。
        let mut safe = attacker.clone();
        safe.in_safe_zone = true;
        assert!(!is_proper_target(&safe, &other), "任一方在安全区应拒绝 PvP");

        // 自己宝宝（非玩、Master==自己）：HAM_ALL 允许，HAM_PEACE 拒。
        let mut pet = player(3, "pet", 11, 10, 1);
        pet.race = 80;
        pet.master = Some(attacker.id);
        assert!(
            is_proper_target(&attacker, &pet),
            "HAM_ALL 下自己宝宝允许（oracle）"
        );
        attacker.attack_mode = ATTACK_MODE_HAM_PEACE;
        assert!(
            !is_proper_target(&attacker, &pet),
            "非 HAM_ALL 下自己宝宝拒绝"
        );
        attacker.attack_mode = ATTACK_MODE_HAM_ALL;

        // 已死/幽灵/隐身：不可被选为目标（折入的目标可见性门）。
        let mut dead = other.clone();
        dead.death = true;
        assert!(!is_proper_target(&attacker, &dead));
        let mut ghost = other.clone();
        ghost.ghost = true;
        assert!(!is_proper_target(&attacker, &ghost));

        // 世界路径仍能把拒绝转成「无伤害」（`World::attack_cmd` 走同一函数）。
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(1, "hero", 10, 10, 10));
        w.tick();
        w.attack_cmd(p, p); // 自己
        let rec = w.tick();
        assert!(
            rec.attacks.iter().all(|(_, _, o)| o.damage == 0),
            "打自己不应造成伤害: {:?}",
            rec.attacks
        );
        assert!(rec.processed.iter().any(|(_, ok)| !*ok));
    }

    /// F-K 真机根因的正向回归：**两个玩家**在 `World::attack_cmd` 生产路径上能真的造成伤害
    /// （旧 `is_attack_target` 对人打人恒 false ⇒ 恒无 `SM_STRUCK`）。
    #[test]
    fn pvp_attack_lands_damage_in_world_path() {
        let mut w = World::new(32, 32);
        let a = w.add_entity(player(1, "hero", 10, 10, 10));
        let b = w.add_entity(player(2, "rival", 11, 10, 10));
        // `player()` 的 HitPoint=10 / SpeedPoint=0 ⇒ 命中判定必中（`10 < RandomByte(0)=0` 为假）。
        w.tick();
        w.attack_cmd(a, b);
        let rec = w.tick();
        let out = rec
            .attacks
            .iter()
            .find(|(atk, tgt, _)| *atk == a && *tgt == b)
            .map(|(_, _, o)| *o)
            .expect("PvP 攻击必须产生结算记录");
        assert!(out.damage > 0, "HAM_ALL 下 PvP 应造成伤害，实际 {out:?}");
        assert!(w.entities.get(b).unwrap().hp < 200, "目标 HP 应下降");
    }

    /// 掉线：离开世界、会话留在 `Left`（与"小退"的区别：不计数 softclose、语义上不可再进同一会话实例）。
    #[test]
    fn session_disconnect_leaves_world_without_softclose_count() {
        let mut w = World::with_maps(vec![(16, 16)]);
        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 3, 3);
        e.race = ACTOR_RACE_PLAY;
        let id = w.enter_map("hero", e, 0).unwrap();
        assert_eq!(w.map(0).cell(3, 3).count(), 1);

        w.disconnect("hero").unwrap();
        assert!(w.entities.get(id).is_none(), "掉线后实体应从世界移除");
        assert_eq!(w.map(0).cell(3, 3).count(), 0, "掉线必须清掉格子登记");
        let sess = w.session("hero").unwrap();
        assert_eq!(sess.stage, crate::session::WorldStage::Left);
        assert_eq!(sess.actor_id, None);
        assert_eq!(sess.soft_close_count, 0, "掉线不计入小退次数");

        // 掉线后再掉线是非法转移（显式报错，不静默）
        assert_eq!(
            w.disconnect("hero").unwrap_err(),
            crate::session::SessionError::NotInWorld
        );
    }

    /// 会话状态机：进图 → 切图 → 小退 → 再进（新的 actor_id），并且视野列表整体失效。
    #[test]
    fn session_state_machine_enter_switch_softclose_reenter() {
        let mut w = World::with_maps(vec![(32, 32), (32, 32)]);
        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 5, 5);
        e.race = ACTOR_RACE_PLAY;
        let first = w.enter_map("hero", e, 0).unwrap();
        assert_eq!(w.session("hero").unwrap().actor_id, Some(first));
        assert_eq!(w.map(0).cell(5, 5).count(), 1);

        // 切图：旧图摘除、新图登记、视野清空
        w.switch_map("hero", 1, 7, 9).unwrap();
        assert_eq!(w.map(0).cell(5, 5).count(), 0, "旧图必须摘除");
        assert_eq!(w.map(1).cell(7, 9).count(), 1, "新图必须登记");
        assert_eq!(w.entities.get(first).unwrap().map_id, 1);
        assert!(w.entities.get(first).unwrap().visible_actors.is_empty());

        // 小退：离开世界但会话保留（可再进）
        w.soft_close("hero").unwrap();
        assert!(w.entities.get(first).is_none());
        assert_eq!(w.session("hero").unwrap().soft_close_count, 1);
        assert_eq!(w.map(1).cell(7, 9).count(), 0);

        // 再进：新的 actor_id（C# 侧重新建 PlayObject）
        let mut e2 = Entity::new(0, "hero", 7, 9);
        e2.race = ACTOR_RACE_PLAY;
        let second = w.enter_map("hero", e2, 1).unwrap();
        assert_ne!(first, second, "小退再进必须得到新的 ActorId");
        assert_eq!(w.map(1).cell(7, 9).count(), 1);
    }

    /// `spawn_monster_precomputed`（M3④-实现 B 第 ③ 片的新 API）：掉落/朝向/构造抽已在回调里
    /// 按 oracle 次序预算完，本方法**不再取任何随机数**；构造抽结果随实体落库。
    ///
    /// **C 第 ② 片追加**：`dir`/`cool_eye`/构造抽同时写进 `Entity.ai` 的初始态
    /// （`AnimalState::seed_from_spawn`）。
    #[test]
    fn spawn_monster_precomputed_consumes_no_rng() {
        let mut w = World::new(16, 16);
        w.clock_ms = 50_000;
        let mut fresh = w.rng.clone();
        let ctor = MonsterCtorDraws {
            run_tick_delta: 11,
            base_search_time: 2222,
            attack_tick_delta: 33,
            walk_tick_delta: 44,
            search_time: 3333,
            meat_quality: 555,
            bo_fear_fire: true,
            zil_kill_count: 6,
        };
        let id = w
            .spawn_monster_precomputed(
                "鸡",
                3,
                3,
                0,
                80,
                2,
                30,
                40,
                0x0A_14,
                5,
                0,
                6,
                7,
                99,
                vec![],
                ctor,
                5,
                true,
            )
            .expect("生成成功");
        let e = w.entities.get(id).expect("怪物在仓库");
        assert_eq!(e.name, "鸡");
        assert_eq!(e.race, 80);
        assert_eq!(e.hp, 40);
        assert_eq!(e.max_hp, 40);
        assert_eq!(e.dc, 0x0A_14);
        assert_eq!(e.ac, 5);
        assert_eq!(e.life_attrib, 0);
        assert_eq!(e.hit_point, 6);
        assert_eq!(e.speed_point, 7);
        assert_eq!(e.mon_exp, 30);
        assert_eq!(e.gold, 99, "预置金币必须原样落库");
        assert!(e.drop_items.is_empty());
        assert_eq!(e.ctor_draws, ctor, "构造抽结果必须原样落实体");
        assert!(e.add_to_mapped, "生成后必须落图");
        // 生成期取数落进 AI 初始态（`CreateMonster` 的 `cert.Dir`/`CoolEye` + 构造链计时器）。
        assert_eq!(e.ai.dir, 5, "dir 必须是 CreateMonster 的 cert.Dir");
        assert!(e.ai.cool_eye, "cool_eye 必须随生成结果落库");
        assert!(e.ai.bo_fear_fire, "bo_fear_fire 来自构造抽");
        assert_eq!(e.ai.attack_tick, 50_000 - 33);
        assert_eq!(e.ai.walk_tick, 50_000 - 44);
        assert_eq!(e.ai.search_time, 3333);
        assert_eq!(e.ai.search_tick, 50_000);
        assert_eq!(e.ai.think_tick, 50_000);
        assert_eq!(e.ai.walk_wait_tick, 50_000);
        // `world.rng` 必须一步未动。
        assert_eq!(w.rng.random(), fresh.random());
        // 越界地图 ⇒ None（不落库）。
        assert!(w
            .spawn_monster_precomputed(
                "鸡",
                3,
                3,
                9,
                80,
                0,
                0,
                1,
                0,
                0,
                0,
                0,
                0,
                0,
                vec![],
                MonsterCtorDraws::default(),
                0,
                false,
            )
            .is_none());
    }

    /// `budget_monster_drops`（新 API）的掉落预算与 `spawn_monster_now`（既有路径）逐字节相同。
    #[test]
    fn budget_monster_drops_matches_spawn_monster_now_drop_budget() {
        let drop_list = vec![MonsterDropItem {
            max_point: 100,
            sel_point: 100, // 恒命中
            item_name: STRING_GOLD_NAME.to_string(),
            count: 1000,
        }];
        let mut a = World::new(16, 16);
        let mut rng = RandomNumber::with_seed(5);
        let (gold, items) = a.budget_monster_drops(&mut rng, &drop_list);

        let mut b = World::new(16, 16);
        b.rng = RandomNumber::with_seed(5);
        b.spawn_monster(Command::SpawnMonster {
            id: 2,
            name: "鸡".into(),
            x: 1,
            y: 1,
            map_id: 0,
            level: 1,
            exp: 0,
            hp: 5,
            dc: 0,
            ac: 0,
            undead: 0,
            hit_point: 5,
            speed_point: 15,
            drop_list: drop_list.clone(),
        });
        b.tick();
        let e = b.entities.get(2).expect("spawned");
        assert_eq!(e.gold, gold, "预算金币必须与既有生成路径一致");
        assert_eq!(e.drop_items, items, "预算物品必须一致");
    }

    /// `BaseObject.Run` 的过期段（`BaseObject.Base.cs:119-132`）：`LastHiter` 30s、`ExpHitter` 6s。
    ///
    /// `SetLastHiter`（`BaseObject.cs:1753-1770`）每次命中刷新 `LastHiterTick`；`ExpHitter` 只在为空时
    /// 认领、同一命中者再次命中才续 `ExpHitterTick`。
    /// 改坏必红：把 `expire_hitters` 的 30s/6s 门拆回「只看命中者死/隐身」（旧版）⇒ 本用例前两段红。
    #[test]
    fn expire_hitters_uses_wall_clock_last_hiter_30s_exp_hitter_6s() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(player(1, "hero", 9, 9, 1));
        let mon = {
            let mut e = Entity::new(0, "鸡", 10, 10);
            e.race = ACTOR_RACE_MONSTER;
            e.hp = 100;
            e.max_hp = 100;
            w.add_entity(e)
        };
        // 命中登记：两者都置。
        w.entities.get_mut(mon).unwrap().set_last_hiter(p, 0);
        // 5s：两个都没过期。
        w.clock_ms = 5_000;
        w.tick();
        let e = w.entities.get(mon).unwrap();
        assert_eq!(e.last_hiter, Some(p), "5s < 30s ⇒ LastHiter 未过期");
        assert_eq!(e.exp_hitter, Some(p), "5s < 6s ⇒ ExpHitter 未过期");
        // 7s：`ExpHitter` 过期（6s），`LastHiter` 仍在（7s < 30s）。
        w.clock_ms = 7_000;
        w.tick();
        let e = w.entities.get(mon).unwrap();
        assert_eq!(e.exp_hitter, None, "7s > 6s ⇒ ExpHitter 过期");
        assert_eq!(e.last_hiter, Some(p), "7s < 30s ⇒ LastHiter 未过期");
        // 31s：`LastHiter` 过期。
        w.clock_ms = 31_000;
        w.tick();
        assert_eq!(
            w.entities.get(mon).unwrap().last_hiter,
            None,
            "31s > 30s ⇒ 过期"
        );
        // 同一命中者再次命中 ⇒ `ExpHitter` 重新认领并刷新 tick（清空后重新登记）。
        w.entities.get_mut(mon).unwrap().set_last_hiter(p, 31_000);
        w.clock_ms = 36_000; // 距上次登记 5s ⇒ 未过期
        w.tick();
        assert_eq!(
            w.entities.get(mon).unwrap().exp_hitter,
            Some(p),
            "重新登记后 5s 内未过期"
        );
        // 命中者死亡 ⇒ 无论时间都清（oracle 的 `||` 右支）。
        w.entities.get_mut(p).unwrap().death = true;
        w.entities.get_mut(mon).unwrap().set_last_hiter(p, 36_000);
        w.clock_ms = 36_100;
        w.tick();
        let e = w.entities.get(mon).unwrap();
        assert_eq!(e.last_hiter, None, "命中者死亡 ⇒ LastHiter 清");
        assert_eq!(e.exp_hitter, None, "命中者死亡 ⇒ ExpHitter 清");
    }

    // ---- 出站接线片 W2：RM_PUSH / RM_RUSH / RM_RUSHKUNG / RM_BUTCH 的 tick 产出 ------------------

    /// `Command::Push` → `BaseObject.CharPushed`（`BaseObject.cs:854-901`）的逐格 `RM_PUSH`：
    /// **每成功推一格一条**事件、`dir = nBackDir`、位置 = 走**后**格；被挡即停。
    ///
    /// 红检：把 `push_now` 里的 `back` 改成 `n_dir`（正推方向）或把事件位置改成走**前**格 ⇒ 必红。
    #[test]
    fn push_command_emits_backstep_per_step() {
        use crate::action::PushEvent;
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick(); // 先跑 Enter 落图（`add_map_object`），否则格子簿记无源格可摘。
        w.enqueue(Command::Push {
            actor_id: 1,
            dir: crate::magic::DIR_UP,
            count: 3,
        });
        let r = w.tick();
        assert_eq!(
            r.pushes,
            vec![
                PushEvent {
                    actor_id: 1,
                    dir: crate::magic::DIR_DOWN,
                    x: 5,
                    y: 4
                },
                PushEvent {
                    actor_id: 1,
                    dir: crate::magic::DIR_DOWN,
                    x: 5,
                    y: 3
                },
                PushEvent {
                    actor_id: 1,
                    dir: crate::magic::DIR_DOWN,
                    x: 5,
                    y: 2
                },
            ],
            "每推一格一条，dir = nBackDir(DIR_DOWN)，位置 = 走后格"
        );
        let e = w.entities.get(1).unwrap();
        assert_eq!((e.x, e.y), (5, 2), "世界权威坐标随推进更新");
    }

    /// `Command::Push` 与既有**已验收**的 [`crate::magic::char_pushed`] 末端状态必须一致
    /// （同一图、同一初值 ⇒ 同 `result`、同 `(x, y)`、同终态 `dir`）——防逐格重写引入漂移。
    #[test]
    fn push_now_final_state_matches_magic_char_pushed() {
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick();
        w.enqueue(Command::Push {
            actor_id: 1,
            dir: crate::magic::DIR_UP,
            count: 4,
        });
        let r = w.tick();
        let steps = i32::try_from(r.pushes.len()).unwrap();
        let (wx, wy) = {
            let e = w.entities.get(1).unwrap();
            (e.x, e.y)
        };

        let mut map = crate::map::MapGrid::new(64, 64);
        let (mut x, mut y, mut dir) = (5_i32, 5_i32, 0_u8);
        let res = crate::magic::char_pushed(
            &mut map,
            &mut x,
            &mut y,
            &mut dir,
            crate::magic::DIR_UP,
            4,
            1,
        );

        assert_eq!(res, steps, "成功步数 = 事件数");
        assert_eq!((wx, wy), (x, y), "末端坐标一致");
        assert_eq!(dir, crate::magic::DIR_DOWN, "result>0 ⇒ Dir 落 nBackDir");
    }

    /// `Command::Rush`（`DoMotaebo` 无正前目标支，`PlayObject.Base.cs:1443-1467`）：向前冲刺，
    /// 每格一条 `Rush`（`dir = nDir`、位置 = 走后格），不产 `RushKung`。
    ///
    /// 红检：把 `rush_now` 的 `Rush` 事件 ident 换成 `RushKung`（或方向改 `back_dir`）⇒ 必红。
    #[test]
    fn rush_command_emits_rush_per_step() {
        use crate::action::{RushEvent, RushFrame};
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick();
        w.enqueue(Command::Rush {
            actor_id: 1,
            dir: crate::magic::DIR_RIGHT,
            steps: 3,
        });
        let r = w.tick();
        assert_eq!(
            r.rushes,
            vec![
                RushEvent {
                    actor_id: 1,
                    frame: RushFrame::Rush,
                    dir: crate::magic::DIR_RIGHT,
                    x: 6,
                    y: 5
                },
                RushEvent {
                    actor_id: 1,
                    frame: RushFrame::Rush,
                    dir: crate::magic::DIR_RIGHT,
                    x: 7,
                    y: 5
                },
                RushEvent {
                    actor_id: 1,
                    frame: RushFrame::Rush,
                    dir: crate::magic::DIR_RIGHT,
                    x: 8,
                    y: 5
                },
            ]
        );
        let e = w.entities.get(1).unwrap();
        assert_eq!((e.x, e.y), (8, 5));
    }

    /// `Command::Rush` 被**不可走**格挡住 ⇒ `bo35` ⇒ 末尾一条 `RushKung`，且其坐标是**正前格**
    /// （角色不在该格上），`dir = Dir`（= `nDir`）。
    ///
    /// 红检：把 `RushKung` 的位置改成角色当前格（`x, y`）而非 `front_position` ⇒ 必红。
    #[test]
    fn rush_blocked_emits_rushkung_at_front_cell() {
        use crate::action::{RushEvent, RushFrame};
        use crate::map::CellAttribute;
        let mut w = world_with(&[(1, 5, 5)]);
        w.tick();
        w.maps[0]
            .get_cell_info_mut(7, 5)
            .0
            .set_attribute(CellAttribute::HighWall);
        w.enqueue(Command::Rush {
            actor_id: 1,
            dir: crate::magic::DIR_RIGHT,
            steps: 4,
        });
        let r = w.tick();
        assert_eq!(
            r.rushes,
            vec![
                RushEvent {
                    actor_id: 1,
                    frame: RushFrame::Rush,
                    dir: crate::magic::DIR_RIGHT,
                    x: 6,
                    y: 5
                },
                RushEvent {
                    actor_id: 1,
                    frame: RushFrame::RushKung,
                    dir: crate::magic::DIR_RIGHT,
                    x: 7,
                    y: 5
                },
            ],
            "走到 (6,5) 后正前 (7,5) 是墙 ⇒ RushKung 落在正前格"
        );
        let e = w.entities.get(1).unwrap();
        assert_eq!((e.x, e.y), (6, 5), "角色停在墙前，不入正前格");
    }

    /// `Command::Butch`（`PlayObject.Operate.cs:749`）：一条事件，位置 = 当前 `CurrX/CurrY`、
    /// `dir` = 传入的已解析 `Dir`。
    #[test]
    fn butch_command_emits_event_at_current_cell() {
        use crate::action::ButchEvent;
        let mut w = world_with(&[(1, 7, 9)]);
        w.tick();
        w.enqueue(Command::Butch {
            actor_id: 1,
            dir: 3,
        });
        let r = w.tick();
        assert_eq!(
            r.butches,
            vec![ButchEvent {
                actor_id: 1,
                dir: 3,
                x: 7,
                y: 9
            }]
        );
    }

    /// **L1 TransMap 触发（核心）**：玩家走一格踩中 `MapRoute` ⇒ `EnterAnotherMap` 门通过 ⇒
    /// `switch_map`，实体换图、旧图摘除/新图登记、`SessionRegistry` 记一条 `MapChange`（无 show）。
    ///
    /// 红检（变异牙）：把 `step_with_route` 里 `walk_route_check` 调用删掉 ⇒ `changed_map` 断言必红；
    /// 把 `walk_route_check` 的 `race != Play` 门去掉 ⇒ `step_with_route_does_not_switch_for_monster` 必红。
    #[test]
    fn step_with_route_triggers_enter_another_map_for_player() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        let n = w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        assert_eq!(n, 1);

        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(out.moved, "走一格被接受");
        assert!(out.changed_map, "踩中 route 应换图");

        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (1, 7, 9));
        assert_eq!(w.map(0).cell(5, 4).count(), 0, "旧格已摘除");
        assert_eq!(
            w.map(0).cell(5, 5).count(),
            1,
            "旧图 (5,5) 只剩 route cell（玩家已摘除）"
        );
        assert_eq!(w.map(1).cell(7, 9).count(), 1, "新图已登记");

        let ch = w.sessions.map_changes();
        assert_eq!(ch.len(), 1, "恰一条 MapChange");
        assert_eq!(ch[0].actor_id, hero);
        assert_eq!((ch[0].map_id, ch[0].x, ch[0].y), (1, 7, 9));
        assert_eq!(
            ch[0].space_move_show, None,
            "EnterAnotherMap 无 SM_SPACEMOVE_SHOW"
        );
    }

    /// **负向对照**：怪物踩同一 route 格 ⇒ **被挡在走前格**（oracle `BaseObject.Walk:1548` 对 `MapRoute`
    /// 只 `result = false`，上游 `WalkTo` 回滚坐标 `:683-688`；换图是 `PlayObject.Walk` 覆写专属）。
    ///
    /// **原断言与 oracle 相反、已改正**（`openmir2-worker-5`，片 R1 `openmir2-maptrigger-monster-and-door`，
    /// 2026-10-17）：旧版写 `assert!(out.moved, "怪物走一格仍成功")`——把「怪物不换图」误当成「怪物能走上
    /// route 格」，漏了 oracle 非玩家 `MapRoute` 支的 `result = false` + `WalkTo` 回滚 ⇒ 实测怪物会走上
    /// route 格（probe：`moved=true, pos=(5,5)`）。现按 oracle 改为 `!out.moved` + 坐标不动。
    ///
    /// 红检（变异牙）：删掉 `try_step_now` 的 `race != ACTOR_RACE_PLAY` 落点检查 ⇒ `out.moved` 变 true、
    /// 坐标变 (5,5) ⇒ 本测必红。
    #[test]
    fn step_with_route_does_not_switch_for_monster() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        // 走 `enter_map` 建会话——否则「无会话 ⇒ 不换图」会掩盖 `race != Play` 门的作用。
        w.open_session("mon");
        let mon = w
            .enter_map(
                "mon",
                {
                    let mut e = Entity::new(0, "mon", 5, 4);
                    e.race = ACTOR_RACE_MONSTER;
                    e
                },
                0,
            )
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        let out = w.step_with_route(mon, 0, 1, 1);
        assert!(
            !out.moved,
            "非玩家被 MapRoute 挡在走前格（oracle BaseObject.cs:1548 result=false + :683-688 回滚）"
        );
        assert!(!out.changed_map, "怪物不得触发换图");
        let e = w.entities.get(mon).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "坐标回到走前格、仍留原图");
        assert!(w.sessions.map_changes().is_empty());
    }

    /// **非玩家 `MapRoute` 阻挡（生产怪物路径）**：`monai::walk_to` → `try_step_now` 对非玩家在落点
    /// 格含 `MapRoute` 时**拒绝该步**（坐标不变）——oracle `BaseObject.Walk` 的非玩家支
    /// （`BaseObject.cs:1548` `result = false` + `:683-688` 回滚）。
    ///
    /// 红检（变异牙）：把 `try_step_now` 的落点检查删掉 ⇒ 本测 `moved`/坐标断言必红。
    #[test]
    fn try_step_now_blocks_non_player_on_map_route_cell() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("mon");
        let mon = w
            .enter_map(
                "mon",
                {
                    let mut e = Entity::new(0, "mon", 5, 4);
                    e.race = ACTOR_RACE_MONSTER;
                    e
                },
                0,
            )
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        assert!(
            !w.try_step_now(mon, 0, 1, 1),
            "非玩家踩 route 格 ⇒ 拒绝该步（oracle result=false）"
        );
        let e = w.entities.get(mon).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "坐标不变（留在走前格）");
        assert!(w.sessions.map_changes().is_empty());
    }

    /// `EnterAnotherMap` 的 `CellValid` 门：目标格不可走（墙）⇒ **换图失败** ⇒ 照 oracle
    /// `PlayObject.Walk()==false`（`PlayObject.cs:1878`）上游 `WalkTo` **回滚坐标到走前格**
    /// （`BaseObject.cs:685-688`），因 `if (result)` 不成立而**不发移动帧**（`:1909-1911`）
    /// ⇒ 本方法返回 `moved == false`。
    ///
    /// **为何原先写反了**：旧断言写 `moved == true`（"走一格仍成功"），是把 `step_with_route`
    /// 当成"移动与路线检查互不影响"来看——漏了 oracle 的 `Walk()` 返回值是**整条走命令的结果**，
    /// 失败会连同位移一起回滚。验证者给的真实证据（`BaseObject.cs:666-673` 的回滚三步 + `:1909-1911`
    /// 的 `if (result)` 才发帧）推翻了旧断言。
    ///
    /// 红检（变异牙）：把 `step_with_route` 的 `RouteOutcome::Failed` 支里的 `rollback_move` 删掉
    /// （或不再返回 `moved == false`）⇒ 下面 `moved`/坐标/旧格断言必红。
    #[test]
    fn step_with_route_blocked_when_dest_cell_is_wall() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.maps[1].set_map_xy_flag(7, 9, false); // 目标格 → LowWall
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(!out.moved, "EnterAnotherMap 失败 ⇒ Walk()=false ⇒ 位移回滚");
        assert!(!out.changed_map, "目标格不可走 ⇒ EnterAnotherMap 失败");
        // 坐标回滚到走前格 (5,4)，仍留原图。
        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "坐标回滚到走前格");
        // 格子簿记也回滚：源格 (5,4) 重新登记玩家、目标格 (5,5) 只剩 route cell。
        assert_eq!(w.map(0).cell(5, 4).count(), 1, "源格重新登记玩家");
        assert_eq!(
            w.map(0).cell(5, 5).count(),
            1,
            "目标格 (5,5) 只剩 route cell"
        );
        assert!(w.sessions.map_changes().is_empty(), "失败不发换图簇");
    }

    /// **must_fix #1 阳性对照**：目标图带 `Flag.boNEEDHOLE`（无 `ET_DIGOUTZOMBI` 事件模型）⇒
    /// oracle `PlayObject.cs:1874` 的门**挡**下 ⇒ **不换图**，但走本身成功（`moved == true`、
    /// 坐标落在源图的 route 格上、无回滚、无 `MapChange`）。这正是真实数据里
    /// `D402→Q004` / `D717→R001` / `H006→H007` / `h006→H106` 等 7 条 route 的行为。
    ///
    /// 红检（变异牙）：把 `route_enter` 里 `m.need_hole()` 的门删掉（恒不挡）⇒ 本测 `changed_map`
    /// 断言必红（会误换图）。
    #[test]
    fn step_with_route_needhole_dest_blocks_map_change() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.set_map_need_hole(1, true); // 目标图带 NEEDHOLE（Q004/R001/H007/H106 同型）
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 14,
                dst_y: 14,
            },
        }]);
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(out.moved, "NEEDHOLE 门只挡换图，不挡走（Walk() 仍 true）");
        assert!(!out.changed_map, "目标图 NEEDHOLE ⇒ 不换图");
        // 落在源图的 route 格上（未回滚），仍在原图。
        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 5), "未换图、未回滚");
        assert!(w.sessions.map_changes().is_empty(), "无换图簇");
    }

    /// **门控①（`ArroundDoorOpened`）**：route 落点格与**未开门相邻（±1）** ⇒ oracle
    /// `PlayObject.cs:1872` `Envir.ArroundDoorOpened(CurrX,CurrY)` 返回 false ⇒ **整支跳过、不换图**，
    /// 但走位照常（`moved == true`、坐标落在源图 route 格、无回滚、无 `MapChange`）——与 NEEDHOLE 门
    /// 同型。oracle 门恒未开（`CM_OPENDOOR` 结构性 no-op）⇒ 门相邻 route 格**恒被挡**
    /// （`tests/parity/m3/design-door-and-mapevent-20261016.md` §2.2）。
    ///
    /// 红检（变异牙）：把 `route_enter` 里的 `arround_door_opened` 门删掉 ⇒ 本测 `changed_map` 断言必红
    /// （会误换图）。
    #[test]
    fn step_with_route_blocked_when_door_adjacent_unopened() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        // 源图 (5,4) 放一条未开门：route 落点 (5,5) 与门 Chebyshev 距离 1 ⇒ 挡。
        w.set_map_doors(
            0,
            vec![crate::map::MapDoor {
                x: 5,
                y: 4,
                n08: 1,
                door_id: 0,
                status_id: 0,
            }],
            vec![crate::map::DoorStatus {
                opened: false,
                open_tick: 0,
            }],
        );
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(out.moved, "门控只挡换图，不挡走（Walk() 仍 true）");
        assert!(!out.changed_map, "与未开门相邻 ⇒ 不换图");
        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 5), "未换图、未回滚");
        assert!(w.sessions.map_changes().is_empty(), "无换图簇");
    }

    /// **门控②（阳性对照）**：同一门 `Opened=true`（测试注入）⇒ `arround_door_opened` 返回 true ⇒
    /// **照常换图**（证明门控读的是门状态、不是恒挡）。
    #[test]
    fn step_with_route_proceeds_when_door_adjacent_opened() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.set_map_doors(
            0,
            vec![crate::map::MapDoor {
                x: 5,
                y: 4,
                n08: 1,
                door_id: 0,
                status_id: 0,
            }],
            vec![crate::map::DoorStatus {
                opened: true,
                open_tick: 0,
            }],
        );
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 1,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 7,
                dst_y: 9,
            },
        }]);
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(out.moved, "走位照常");
        assert!(
            out.changed_map,
            "门开 ⇒ arround_door_opened=true ⇒ 照常换图"
        );
        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (1, 7, 9), "换到目标图目标格");
    }

    /// **must_fix #3**：同一源格有**多个** `MapRoute` 对象 ⇒ oracle `for` **不 break**，逐个
    /// `EnterAnotherMap` ⇒ **终落点 = 最后一条**（`PlayObject.cs:1862-1884`）。真实数据：
    /// `D5069(90,76)` 同时 `→D5062(8,39)` 与 `→D5071(8,10)`，终落点 = `D5071`。
    ///
    /// 红检（变异牙）：把 `route_enter` 收集改回 `find_map` 取**第一条**（`find_map(...)`）⇒
    /// 终落点断言必红（会停在第一条目标）。
    #[test]
    fn step_with_route_multiple_routes_last_wins() {
        let mut w = World::with_maps(vec![(64, 64), (64, 64), (64, 64)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        // 同一源格 (5,5) 落两条 route：第一条 → map1(8,39)，第二条 → map2(8,10)。
        w.add_map_routes(vec![
            MapRouteSpec {
                map_id: 0,
                x: 5,
                y: 5,
                route_id: 1,
                hop: crate::map::RouteHop {
                    dst_map_id: 1,
                    dst_x: 8,
                    dst_y: 39,
                },
            },
            MapRouteSpec {
                map_id: 0,
                x: 5,
                y: 5,
                route_id: 2,
                hop: crate::map::RouteHop {
                    dst_map_id: 2,
                    dst_x: 8,
                    dst_y: 10,
                },
            },
        ]);
        assert_eq!(w.map(0).cell(5, 5).count(), 2, "同格两条 route cell");
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(out.moved && out.changed_map, "命中 route ⇒ 换图");
        let e = w.entities.get(hero).unwrap();
        assert_eq!(
            (e.map_id, e.x, e.y),
            (2, 8, 10),
            "终落点 = 最后一条 route（D5071 同型）"
        );
        // 两条 route 各记一条 MapChange（oracle 会发两组 633/634），终落点为最后一条。
        let ch = w.sessions.map_changes();
        assert_eq!(ch.len(), 2, "两条 route ⇒ 两组换图簇");
        assert_eq!((ch[1].map_id, ch[1].x, ch[1].y), (2, 8, 10));
    }

    /// 原地转身（`CM_TURN`）：actor 已站在 route 格上 ⇒ `walk_route_check` 直接触发换图。
    #[test]
    fn walk_route_check_triggers_on_current_cell() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 5), 0)
            .unwrap();
        w.add_map_routes(vec![MapRouteSpec {
            map_id: 0,
            x: 5,
            y: 5,
            route_id: 3,
            hop: crate::map::RouteHop {
                dst_map_id: 1,
                dst_x: 2,
                dst_y: 2,
            },
        }]);
        assert!(w.walk_route_check(hero), "当前格命中 route ⇒ 换图");
        assert_eq!(
            (
                w.entities.get(hero).unwrap().map_id,
                w.entities.get(hero).unwrap().x
            ),
            (1, 2)
        );
        assert_eq!(w.sessions.map_changes().len(), 1);
    }

    // ================= 片 A-5（buff 生产者接线）: 攻击中毒门 =================

    fn ent(id: i32, name: &str, x: i32, y: i32, race: u8) -> Entity {
        let mut e = Entity::new(id, name, x, y);
        e.race = race;
        e
    }

    /// **A-5 端到端（攻击 → `make_posion` → 657）**：玩家戴麻痹戒指攻击一个玩家目标，中毒门命中
    /// （`Random(AntiPoison + AttackPosionRate) == 0`）⇒ `World::attack_once` 调
    /// `make_posion(STONE, AttackPosionTime, 0)` ⇒ 目标获得 `Paralysis` buff（`CharStatus` 置位）；
    /// buff 递减到 0 时 `Operate` 的 `boChg` 支 ⇒ `TickRecord.buff_status_changes` 非空
    /// （宿主 `drain_buff_downlink` 出 `SM_CHARSTATUSCHANGED(657)`）。
    #[test]
    fn attack_poison_end_to_end_yields_char_status_change() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        // `Random(rate) == 0` 必中：把 rate 压到 1（`random_below(1)` 恒 0）。
        w.combat_config.attack_poison_rate = 1;
        w.combat_config.attack_poison_time = 5;
        w.add_entity(ent(1, "hero", 10, 10, ACTOR_RACE_PLAY));
        w.add_entity(ent(2, "foe", 11, 10, ACTOR_RACE_PLAY));
        // 攻方持麻痹态（`set_attack_poison` = 宿主灌入 `PlayObject.Paralysis`）。
        w.set_attack_poison(
            1,
            AttackPoisonState {
                paralysis: true,
                un_paralysis: false,
                anti_poison: 0,
            },
        );
        w.attack_cmd(1, 2);
        w.tick();
        assert!(
            w.buffs.has_buff(2, BuffType::Paralysis),
            "中毒门命中 ⇒ 目标上石化毒 buff（`STONE = 5`）"
        );
        assert_ne!(
            w.entities.get(2).unwrap().char_status,
            0,
            "`MakePosion` 写 `CharStatus`（含 Paralysis 位）"
        );
        // 推进到 buff 过期（state_tick 5，每 tick 4 pass ⇒ 2 tick 内到 0）。
        let mut saw_change = false;
        for _ in 0..4 {
            w.tick();
            if w.last_tick
                .buff_status_changes
                .iter()
                .any(|(id, _)| *id == 2)
            {
                saw_change = true;
                break;
            }
        }
        assert!(
            saw_change,
            "buff 过期 ⇒ buff_status_changes 记目标（宿主出 657）"
        );
        // 红检（变异牙）：① 去掉 `attack_once` 里的 `make_posion` 调用 ⇒ 首个 `has_buff` 断言必红；
        // ② 不接线（`poisoned` 恒 false）⇒ 同上；③ 把 `if self.buff_enabled` 去掉成恒读表 ⇒
        // `attack_poison_caller_absent_when_buff_disabled` 必红。
    }

    /// **A-5 关闭语义**：`buff_enabled == false` 时**不接线**——即使攻方有麻痹态且 rate=1，也不上毒
    /// （既有 golden 逐字节不变）。
    #[test]
    fn attack_poison_caller_absent_when_buff_disabled() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        // 不 `enable_buff_tick`。
        w.combat_config.attack_poison_rate = 1;
        w.add_entity(ent(1, "hero", 10, 10, ACTOR_RACE_PLAY));
        w.add_entity(ent(2, "foe", 11, 10, ACTOR_RACE_PLAY));
        w.set_attack_poison(
            1,
            AttackPoisonState {
                paralysis: true,
                un_paralysis: false,
                anti_poison: 0,
            },
        );
        w.attack_cmd(1, 2);
        w.tick();
        assert!(
            !w.buffs.has_buff(2, BuffType::Paralysis),
            "buff 关闭 ⇒ 不接线、不上毒"
        );
    }

    /// **A-5 攻方无麻痹态 ⇒ 不抽不上毒**（oracle `PlayObject.Attack.cs:221` 的 `&&` 短路 + `Paralysis` 门）。
    #[test]
    fn attack_poison_requires_paralysis_flag() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        w.combat_config.attack_poison_rate = 1;
        w.add_entity(ent(1, "hero", 10, 10, ACTOR_RACE_PLAY));
        w.add_entity(ent(2, "foe", 11, 10, ACTOR_RACE_PLAY));
        w.set_attack_poison(
            1,
            AttackPoisonState {
                paralysis: false,
                un_paralysis: false,
                anti_poison: 0,
            },
        );
        w.attack_cmd(1, 2);
        w.tick();
        assert!(
            !w.buffs.has_buff(2, BuffType::Paralysis),
            "无麻痹态 ⇒ 不上毒"
        );
    }

    // ================= 片 A-5: 技能生产链（14/15/31） =================

    fn magic(magic_id: u16) -> mir2_data::MagicInfo {
        mir2_data::MagicInfo {
            magic_id,
            def_max_power: 1, // `GetPower` 的 `random_below(def_max_power - def_power)` 分界 > 0
            ..Default::default()
        }
    }

    /// **A-5 技能 15（`SKILL_DEJIWONHO`）→ `defence_up` + oracle 的中心格 bug**：
    /// `MagMakeDefenceArea` 的 `(2*3+1)^2 = 49` 个迭代全读**中心格** ⇒ 中心格的唯一友军被
    /// `DefenceUp` 49 次（`hits == 49`），邻格友军一次都不加。
    #[test]
    fn skill_dejiwonho_defence_up_follows_oracle_center_cell_bug() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        w.add_entity(ent(1, "caster", 10, 10, ACTOR_RACE_PLAY));
        w.add_entity(ent(2, "friend", 12, 12, ACTOR_RACE_PLAY));
        w.add_entity(ent(3, "bystander", 11, 10, ACTOR_RACE_PLAY));
        let m = magic(SKILL_DEJIWONHO);
        let hits = w.cast_defence_area_spell(1, &m, 0, 0, 12, 12, 0);
        assert_eq!(
            hits, 49,
            "中心格友军各 49 次（oracle `GetCellInfo(nX,nY)` 未用循环变量的 bug）"
        );
        assert!(
            w.buffs.has_buff(2, BuffType::DefensePower),
            "中心格友军获 `DefencePower`"
        );
        assert!(
            !w.buffs.has_buff(3, BuffType::DefensePower),
            "邻格（非中心）一次都不加"
        );
        // 红检（变异牙）：把 `GetCellInfo(nX,nY)` 的 bug「修正」成按 `(cX,cY)` 遍历 ⇒ `hits` 变 **3**
        // （**订正 F3**，独立复验 2026-10-17：车道自述里的「49→1」**不实**——按 `(cX,cY)` 遍历时，
        // 中心格 `(12,12)` 的友军 1 次，邻格 `(11,10)` 的旁观者 1 次，**外加施法者自己 `(10,10)`**
        // （落在这 7×7 窗内，且缺省 `HAM_ALL` 下也是自己的友军）1 次 ⇒ **恰 3**）⇒ 上面的
        // `hits == 49` 断言（以及 `bystander` 的 `!has_buff` 断言）必红。
    }

    /// **A-5 技能 14（`SKILL_HANGMAJINBUB`）→ `mag_defence_up`**：同一中心格 bug，加的是
    /// `MagicDefensePower`（`btState == 1`）。
    #[test]
    fn skill_hangmajinbub_mag_defence_up() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        w.add_entity(ent(1, "caster", 10, 10, ACTOR_RACE_PLAY));
        w.add_entity(ent(2, "friend", 12, 12, ACTOR_RACE_PLAY));
        let m = magic(SKILL_HANGMAJINBUB);
        let hits = w.cast_defence_area_spell(1, &m, 0, 0, 12, 12, 1);
        assert_eq!(hits, 49, "中心格友军各 49 次");
        assert!(w.buffs.has_buff(2, BuffType::MagicDefensePower));
        assert!(
            !w.buffs.has_buff(2, BuffType::DefensePower),
            "btState=1 ⇒ 走 MagDefenceUp"
        );
    }

    /// **A-5 技能 31（`SKILL_SHIELD`）→ `magic_shield_on`**：加 `MagicShield` buff + 置实体
    /// `magic_shield`（`DamageHealth` 的 MP 抵扣门，`BaseObject.cs:779`）。
    #[test]
    fn skill_shield_adds_magic_shield_buff_and_flag() {
        let mut w = World::new(8, 8);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        w.add_entity(ent(1, "hero", 5, 5, ACTOR_RACE_PLAY));
        let m = magic(SKILL_SHIELD);
        let n_sec = w.cast_shield_spell(1, &m, 0, 0);
        assert!(n_sec >= 0);
        assert!(
            w.buffs.has_buff(1, BuffType::MagicShield),
            "魔法盾 buff 已加"
        );
        assert!(
            w.entities.get(1).unwrap().magic_shield,
            "`MagicShield` 置位（DamageHealth 抵扣门）"
        );
        // 红检（变异牙）：去掉 `mag_make_defence_area` 的 `self.defence_up/mag_defence_up` 调用、
        // 或去掉 `magic_shield_on` 的 `add_buff` ⇒ 上述 `has_buff` 断言必红。
    }

    /// **A-5 友军门**：`HAM_DEAR`（只保护配偶）下，非配偶友军**不**被加点 ⇒ `hits == 0`。
    #[test]
    fn skill_defence_area_respects_attack_mode_friend_gate() {
        let mut w = World::new(32, 32);
        w.clock_ms = 100_000;
        w.enable_buff_tick();
        let mut caster = ent(1, "caster", 10, 10, ACTOR_RACE_PLAY);
        caster.attack_mode = crate::combat::ATTACK_MODE_HAM_DEAR;
        caster.dear_human = Some(99); // 配偶不是 (12,12) 的那位
        w.add_entity(caster);
        w.add_entity(ent(2, "stranger", 12, 12, ACTOR_RACE_PLAY));
        let m = magic(SKILL_DEJIWONHO);
        let hits = w.cast_defence_area_spell(1, &m, 0, 0, 12, 12, 0);
        assert_eq!(hits, 0, "HAM_DEAR ⇒ 非配偶友军不被加点");
        assert!(!w.buffs.has_buff(2, BuffType::DefensePower));
    }
}
