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

use crate::aoi::{search_view_range_death, search_view_range_with_updates, CELL_OBJECT_EXPIRE_MS};
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

/// **P3-B3** 六魔号（按 `MagicConst.cs` 的 ID 钉死；中文名见合同 §1.1）。
pub const SKILL_HOLYSHIELD: u16 = 16; // `MagicConst.cs:68` 捆魔咒（部署名「困魔咒」）
pub const SKILL_CLOAK: u16 = 18; // `MagicConst.cs:76` 隐身术
pub const SKILL_BIGCLOAK: u16 = 19; // `MagicConst.cs:80` 集体隐身术
pub const SKILL_SHOWHP: u16 = 28; // `MagicConst.cs:116` 心灵启示
pub const SKILL_KILLUNDEAD: u16 = 32; // `MagicConst.cs:132` 圣言术
pub const SKILL_UNAMYOUNSUL: u16 = 40; // `MagicConst.cs:140` 解毒术常量（部署行「十方斩」）

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

/// `PlayObject.Run` 状态到期事件（`PlayObject.Message.cs:54-64`）。
///
/// 事件按 oracle 的**发出顺序**记录：烈火到期 = `SysMsg(SpiritsGone)` → `SendSocket("+UFIR")`；
/// 双龙到期 = `SendSocket("+UTWN")`。`SendSocket(string)` 走短包（`'#' + 正文 + '!'`，
/// 见 `mir2_protocol::frame`）；帧面由宿主 `bins/game-svr` 按本记录出。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RunStateExpiry {
    /// `SysMsg(MessageSettings.SpiritsGone, MsgColor.Red, MsgType.Hint)`（`:56`）——
    /// `SpiritsGone = "召唤烈火结束!!!"`（`SystemModule/MessageSettings.cs:60`）。
    SpiritsGone,
    /// `SendSocket("+UFIR")`（`:58`）。
    FireHitSocket,
    /// `SendSocket("+UTWN")`（`:63`）。
    TwinHitSocket,
}

/// `DropUseItems` 的一条删除记录：`(装备槽, ItemName, MakeIndex)`。
///
/// `ItemName` 在第一段（`ItemDesc & 8`）恒空串（oracle `DeleteItem.ItemName` 缺省 null）。
pub type UseItemDeletion = (usize, String, i32);

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
    /// **`openmir2-player-drop-useitems`**：本 tick 玩家死亡 `DropUseItems`
    /// （`PlayObject.Message.cs:2152-2228`）的删除列表，按死亡结算序：
    /// `(玩家 id, [(装备槽, 物品名 GBK 字节, MakeIndex)])`。
    ///
    /// `ItemName` 只在第一段（`ItemDesc & 8`）为空串——oracle 那里只填 `MakeIndex`
    /// （`PlayObject.Message.cs:2170`），`SendDelItemList` 拼成 `"/MakeIndex/"`。
    pub death_use_item_drops: Vec<(i32, Vec<UseItemDeletion>)>,
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
    /// **P3-B5**：本 tick 的 `ExtraAbil` 过期/到期前 10s 事件（`PlayObject.Message.cs:163-227`）。
    pub extra_abil_expiries: Vec<ExtraAbilExpiry>,
    /// **出站接线片 W2**：本 tick 的 `RM_PUSH` 族（`SM_BACKSTEP(9)`）——`CharPushed` 每推进一格一条，保序。
    ///
    /// 两个来源共用本表（顺序 = 先命令 FIFO、后玩家 `Run` 的重叠推挤段）：
    /// ① `Command::Push`（`World::push_now`）；② `World::check_dup_obj`（`PlayObject.Message.cs:283`）。
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
    /// 本 tick `PlayObject.Run` 的烈火/双龙到期事件（`PlayObject.Message.cs:54-64`）：
    /// `(actor_id, 事件)`，按玩家槽位顺序、每个玩家内按 oracle 发出顺序（见 [`RunStateExpiry`]）。
    pub run_state_expiries: Vec<(i32, RunStateExpiry)>,
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

/// **P3-B1（单体直击/骰门族）**：`DoSpell` 单个目标效果体的返回值（供共享尾与用例观察）。
///
/// `target` = oracle `:160-163` 死亡归一 + case 内可能再次置 null 之后，共享尾
/// `RM_MAGICFIRE` 读的 `targetObject.ActorId`（`None ⇒ 0`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpellStrikeOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的 ActorId 来源（`None ⇒ 0`）。
    pub target: Option<i32>,
    /// `boTrain`（`true` 才可能在共享尾抽 `Random(3)+1`）。
    pub bo_train: bool,
    /// `RM_DELAYMAGIC` 的威力（宿主据此 `SendSelfDelayMsg`，入队零抽数）。
    pub power: i32,
    /// 是否排入 `RM_DELAYMAGIC`（`SendSelfDelayMsg`）。
    pub delayed: bool,
    /// `RM_DELAYMAGIC` 的延迟毫秒（600 / 13 的 1200）。
    pub delay_ms: i32,
    /// 44 的 `RM_DELAYPUSHED` 推挤格数（`push > 0` 时 oracle 才发）。
    pub push: Option<u8>,
}

/// **P3-B2**：一条已入目标队列、尚未处理的 `RM_MAGSTRUCK(8044)`（`MagicManager.cs:1111/1138`）。
///
/// oracle 的 `targetObject.SendMsg(baseObject, RM_MAGSTRUCK, 0, nPower, 0, 0)` 进的是**目标自己的**
/// `MsgQueue`（优先级 `(byte)8044 = 108`）；本仓记录后由宿主 `World::deliver_mag_struck` 按
/// **入队序**逐目标完整处理（构造级判据口径，见合同 §4.2）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PendingMagStruck {
    pub target_id: i32,
    pub caster_id: i32,
    /// `RM_MAGSTRUCK.nParam1` = 施法者算出的 `nPower`（24 的非不死目标已 `/10`）。
    pub power: i32,
    /// **P3-B5**：`true` = `RM_MAGSTRUCK_MINE`（22 火墙事件），`false` = `RM_MAGSTRUCK`
    /// （9/10 穿透延迟帧 / 11/45 / B4 延迟魔法）。
    ///
    /// 两者共用处理器（`BaseObject.Base.cs:618-686`），唯一差别 = `Random(1000)` 只对
    /// `RM_MAGSTRUCK` 生效（`:620` 的 `wIdent == RM_MAGSTRUCK` 门）。
    pub mine: bool,
    /// **宿主排水序**（消息创建序，`World::next_msg_order`）：oracle 的 `RM_MAGSTRUCK`
    /// 进的是**目标自己的** `MsgQueue`，由 `ProcessMonsters` 的目标顺序泵出；本仓用一条
    /// 批次列表替代，故把批内顺序**钉在创建序**（= 五批 RNG trace 基线的口径）。
    /// **不参与**任何队列/战斗语义。
    pub order: u64,
}

/// **P3-B2**：一条 `RM_MAGSTRUCK` 交付段处理后的观察值（`BaseObject.Base.cs:618-686`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MagStruckOutcome {
    pub target_id: i32,
    pub caster_id: i32,
    /// `StruckDamage` 后的实际扣血（`nDamage > 0` 才非 0）。
    pub damage: i32,
    pub hp_after: u16,
    pub max_hp: u16,
    /// 目标 `Race == ActorRace.Play`（决定 53 帧是否发）。
    pub is_player: bool,
    /// 受击怪物自消息链是否产生 `RM_CHANGENAMECOLOR`（`!ObMode && !FixedHideMode`）。
    pub name_color: bool,
    /// `GetChrColor`（656 的 Param）。
    pub chr_color: u8,
}

/// **P3-B2**：治疗族（2/29）效果体返回值。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HealingSpellOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的 ActorId 来源（2 的「目标为空/死亡 ⇒ 自己」已在此回落）。
    pub target: Option<i32>,
    /// 共享尾坐标（2 的死亡/空目标回落会把坐标改写为施法者坐标）。
    pub tx: i32,
    pub ty: i32,
    /// `boTrain`：2 = `IsProperFriend && HP<MaxHP`；29 = 至少一个 `HP<MaxHP` 的 proper friend。
    pub bo_train: bool,
    /// `nPower`（`ceil(nPower/5)` 条恢复 53 的来源）。
    pub power: i32,
    /// 2 的 `IsProperFriend` 门是否通过（false ⇒ 0 抽、无 case 帧）。
    pub allowed: bool,
    /// 收到 `RM_MAGHEALING`（延迟 800）的目标（枚举序）。
    pub healed: Vec<i32>,
    /// 收到 `RM_ABILSEEHEALGAUGE` 的目标（2 单目标 / 29 全部 proper friend，不看 HP；枚举序）。
    pub gauges: Vec<i32>,
}

/// **P3-B2**：AoE 族（23/24/33/47）效果体返回值。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AreaSpellOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的 ActorId 来源（死亡归一后的点击目标）。
    pub target: Option<i32>,
    /// 至少一个 `IsProperTarget`（与 `MSD` 后的伤害无关）。
    pub bo_train: bool,
    /// 施法者侧 `nPower`（24 的每目标 `/10` 在交付消息载荷里）。
    pub power: i32,
    /// `RM_MAGSTRUCK` 的目标（枚举序）。
    pub targets: Vec<i32>,
}

/// **P3-B4**：6/48 的单目标施毒结果（`MagicManager.cs:235-252` / `:914-932`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PoisonStrikeOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的 ActorId 来源（proper 目标）。
    pub target: Option<i32>,
    /// 骰门通过且形状为 1/2（`Race == Play || >= Animal`）。
    pub bo_train: bool,
    /// 是否投了 `RM_POISON`。
    pub poisoned: bool,
    /// `nPower`（= `RM_POISON` 的 `nParam1`/时间）。
    pub power: i32,
    /// `PoisonState`（0=DECHEALTH 绿毒、1=DAMAGEARMOR 红毒）。
    pub poison_type: u8,
    /// `RM_POISON.nParam1`（时长 = `nPower`）。
    pub time: i32,
    /// `RM_POISON.nParam3`（中毒点）。
    pub point: i32,
}

/// **P3-B4**：48 群体施毒的逐目标结果（枚举序）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupPoisonOutcome {
    pub bo_train: bool,
    pub targets: Vec<PoisonStrikeOutcome>,
}

/// **P3-B4**：50 `MabMabe` 的效果结果。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MabMabeSpellOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的 ActorId 来源。
    pub target: Option<i32>,
    /// `boTrain = MabMabe(...)`（完整成功臂才为 true）。
    pub bo_train: bool,
    /// case 级 4 抽算出的 `nPower`。
    pub power: i32,
    /// 成功臂 `GetMagStruckDamage` 后的 `nPower`（未成功臂为 0）。
    pub msd_power: i32,
    /// 玩家目标的 `SetPkFlag` ⇒ 656（由目标发，cast tick）。
    pub name_color: Option<crate::slave::NameColorDelivery>,
}

/// **P3-B4**：42 `MagGroupLightening` 的逐目标载荷。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GroupLighteningTarget {
    pub target_id: i32,
    /// `RM_DELAYMAGIC` 的 `wParam`（不死系已 ×1.5）。
    pub power: i32,
}

/// **P3-B4**：42 `MagGroupLightening` 的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupLighteningOutcome {
    /// helper `:987` 的 `RM_MAGICFIRE`（638）的 ActorId（= `targetObject.ActorId`）。
    pub fire_target: i32,
    /// 任一目标 `Race >= Animal`（`:1005-1008`）。
    pub bo_train: bool,
    /// 命中骰门并排入 `RM_DELAYMAGIC` 的目标（枚举序）。
    pub targets: Vec<GroupLighteningTarget>,
    /// `RM_10205` ⇒ `SM_716` 的坐标（每个非中心 proper 目标一条，枚举序）。
    pub icons_716: Vec<(i32, i32)>,
}

/// **P3-B3**：16 `MagMakeHolyCurtain` 的结果（`MagicManager.cs:1146-1207`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct HolyCurtainOutcome {
    /// 共享尾 `RM_MAGICFIRE` 的坐标（= snap 后的点击点）。
    pub tx: i32,
    pub ty: i32,
    /// 已求值的 `nPower`（`GetPower13(40) + GetRPow(SC)*3`；`*1000` 后作为 `wParam`）。
    pub power: i32,
    /// `Envir.CanWalk(tx,ty,true)` 是否通过（失败 ⇒ `result=0` 且零 helper 抽）。
    pub can_walk: bool,
    /// 收到 `RM_MAKEHOLYSEIZEMODE` 的对象（枚举序）。
    pub eligible: Vec<i32>,
    /// `result`（**最后一段连续合格对象**的数量；任一不合格对象会把已累计值归零）。
    pub result: i32,
}

/// **P3-B3**：18 `MagMakePrivateTransparent` 的结果（`MagicManager.cs:673-699`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PrivateTransparentOutcome {
    /// helper 是否走到收尾（`StatusTimeArr[8] > 0` ⇒ 早退 ⇒ **无 657**）。
    pub status_changed: bool,
    /// `GetCharStatus()`（657 的 `nParam1`）。
    pub status: i32,
    /// `StatusTimeArr[8] = (ushort)nHTime` 之后的值。
    pub status_time: u16,
}

/// **P3-B3**：19 `MagMakeGroupTransparent` 的结果（`MagicManager.cs:1209-1228`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GroupTransparentOutcome {
    pub n_h_time: i32,
    /// 收到 `RM_TRANSPARENT`（800ms）的友方（枚举序）。
    pub friends: Vec<i32>,
    /// helper 返回值（至少一个合格友方 ⇒ true）。
    pub result: bool,
}

/// **P3-B3**：28 `SKILL_SHOWHP` 的结果（`MagicManager.cs:431-442`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShowHpOutcome {
    /// `targetObject != null && !ShowHp` 门是否通过（false ⇒ 零抽、无 1100/640）。
    pub gate_passed: bool,
    /// `Random(6) <= Level + 3` 是否通过。
    pub roll_passed: bool,
    /// `boTrain`（门 + 骰都过才 true）。
    pub bo_train: bool,
}

/// **P3-B3**：32 `MagTurnUndead` 的结果（`MagicManager.cs:827-856`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnUndeadSpellOutcome {
    /// `(AnimalObject)target` 抛 `InvalidCastException`（target 是 `PlayObject` 且
    /// `LA_UNDEAD`）——oracle 被 `PlayObject.Base.cs:1525-1529` 吞掉 ⇒ `DoSpell` 返回 false
    /// ⇒ 恰一条 `SM_MAGICFIRE_FAIL(639)`、**无共享尾**。
    pub invalid_cast: bool,
    /// `boTrain`（`MagTurnUndead` 返回 true = `target.WAbil.HP = 0`）。
    pub bo_train: bool,
    /// `target.WAbil.HP == 0` 已被写入。
    pub hp_zeroed: bool,
}

/// **P3-B3**：40 常量名解毒术（部署行「十方斩」）的结果（`MagicManager.cs:495-523`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UnamyounsulOutcome {
    /// `IsProperFriend` 门是否通过（false ⇒ 零抽）。
    pub allowed: bool,
    /// `Random(7) - (Level + 1) < 0` 是否通过。
    pub roll_passed: bool,
    /// 任一计时器非 0 且置 1（`boTrain`）。
    pub bo_train: bool,
}

/// **P3-B5**：一次穿透命中（`MagPassThroughMagic`，`BaseObject.cs:896-931`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PassThroughHit {
    pub target_id: i32,
    /// 本次命中投递的 `magPwr`（10 的 `undeadAttack` 会**逐击复合** ×1.5）。
    pub power: i32,
}

/// **P3-B5**：9/10 穿透效果体结果（`MagicManager.cs:265-288`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PassThroughOutcome {
    /// `MagPassThroughMagic(...) > 0`（`boTrain`）。
    pub bo_train: bool,
    /// 命中序（延迟帧的入队序；每项进被击中者自己的队列）。
    pub hits: Vec<PassThroughHit>,
    /// `nTargetX/nTargetY` 的终点（步长 5 = 9 / 8 = 10；共享尾 638 用它）。
    pub end_x: i32,
    pub end_y: i32,
}

/// **P3-B5**：22 火墙效果体结果（`MagicManager.cs:412-417` + `:1064-1098`）。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct FireWallOutcome {
    /// `MagMakeFireCross(...)` 的返回值（安全区门失败 ⇒ false ⇒ 无训练抽）。
    pub bo_train: bool,
    /// 安全区门失败（`DisableInSafeZoneFireCross && InSafeZone`）——调用方据此发 `SM_SYSMESSAGE(100)`。
    pub safe_zone_blocked: bool,
}

/// **P3-B5**：36 无极真气效果体结果（`MagicManager.cs:610-630`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct UEnhancerOutcome {
    /// `IsProperFriend(target)` 门（false ⇒ 零抽、`boSpellFail` 保持 true ⇒ 恰一条 639）。
    pub allowed: bool,
    /// `:612-617` 归一后的目标（`targetObject == null ⇒ this`）。
    pub target: i32,
    /// `(ushort)(Level + 1 + Random(Level))`。
    pub n_power: u16,
    /// `GetAttackPower(GetPower13(60) + Lo(SC)*10, ΔSC+1)`。
    pub n_time: i32,
    /// `(IPlayerActor)targetObject` 的防御臂（当前玩家入口不可达）：oracle 抛
    /// `InvalidCastException` 被 `PlayObject.Base.cs:1525-1529` 吞掉 ⇒ 639、无尾；
    /// **但 `UseAmulet` 已消耗、抽数已发生**。
    pub invalid_cast: bool,
}

/// **P3-B5**：`ExtraAbil[i]` 过期的观察值（`PlayObject.Message.cs:163-227`）。
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExtraAbilEventKind {
    /// `now > ExtraAbilTimes[i]` ⇒ 清零 + `SysMsg("…恢复正常。")` + 循环后 `HealthSpellChanged()`。
    Restored(u8),
    /// `ExtraAbilFlag[i] == 0 && now > ExtraAbilTimes[i] - 10000` ⇒ 置 1 + `SysMsg("…10秒后恢复正常。")`。
    TenSecondsBefore(u8),
}

/// **P3-B5**：一个 actor 一批过期检查的结果。
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ExtraAbilExpiry {
    pub actor_id: i32,
    pub events: Vec<ExtraAbilEventKind>,
    /// 任一槽过期 ⇒ `boNeedRecalc` ⇒ 只 `HealthSpellChanged()`（**不 recalc、不发 52**）。
    pub health_changed: bool,
}

/// **P3-B5**：一格火墙事件（`MapEvent` + `FireBurnEvent` 的最小字段集）。
///
/// **BUG-REPLICA 1**：`MapEvent.Id` 恒 0（赋值行被注释）⇒ `CellObjectMgr.TryAdd(0, …)`
/// 第一条胜；`Envir.GetEvent` 返回管理器里的第一条，不是当前格对象。
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FireBurnEvent {
    /// `OwnBaseObject`（施法者）；`Ghost || Death` 后置 `None`（oracle `MapEvent.Run` 尾）。
    pub owner_id: Option<i32>,
    /// `MapEvent.Envirnoment` 的图下标（`AddMapEvent` 落在施法者当前图）。
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    /// `EventType`：`Grobal2.ET_FIRE = 1`。
    pub event_type: i32,
    pub damage: i32,
    pub open_start_ms: i64,
    pub continue_ms: i32,
    pub run_start_ms: i64,
    pub run_tick_ms: i64,
    /// `FireBurnEvent.FireRunTick`（初值 0 ⇒ uptime > 3000ms 时第一次 Run 就造成伤害）。
    pub fire_run_tick_ms: i64,
    pub closed: bool,
    pub visible: bool,
    pub active: bool,
    /// **P3-B5（must_fix）**：该事件是否成功进入格子的 `CellObjectMgr`（`CellType::Event` 格对象
    /// 从 `map.add_event_cell` 返回 true）。oracle 里只有**第一条**注册成功的事件能被
    /// `GetEvent` 取到；越界/不可走格子的事件只进全局表（BUG-REPLICA 3）⇒ 不能作为
    /// `event_at` 的返回值。字段名对应合同 §2.8 的 `first_registered_event`。
    pub registered: bool,
}

/// `MapEvent`（`src/Modules/SystemModule/MagicEvent/MapEvent.cs`）的最小等价物——
/// 只承载 `DigOutZombi.Turned` 的 `ET_DIGOUTZOMBI` 洞穴事件（`DigOutZombi.cs:59`）。
///
/// 消费点：`PlayObject.Walk` 的 NEEDHOLE 门
/// （`PlayObject.cs:1874`：`EventMgr.GetEvent(Envir, CurrX, CurrY, ET_DIGOUTZOMBI) != null`）。
/// `EventManager.GetEvent`（`src/M2Server/Event/EventManager.cs:29-42`）逆插入序找**首个**
/// 同 `EventType/Envir/nX/nY` 的事件；`EventProcessor.ExecuteInternal`（`:31-48`）每
/// `RunTick(500ms)` 跑一次 `MapEvent.Run`，`now - OpenStartTick > ContinueTime` 即移出
/// `Events`（`Closed` 后下一轮进 `ClosedEvents`，`GetEvent` 便再也取不到）。
#[derive(Clone, Copy, Debug)]
pub struct DigOutEvent {
    pub map_id: usize,
    pub x: i32,
    pub y: i32,
    /// `Grobal2.ET_DIGOUTZOMBI = 1`（`src/OpenMir2/Grobal2.cs:41`）。
    pub event_type: i32,
    pub open_start_ms: i64,
    pub continue_ms: i64,
    pub run_start_ms: i64,
    pub run_tick_ms: i64,
    pub closed: bool,
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
    /// `Config.DieDropUseItemRate`（`GameSvrConf.cs:768/1714` 缺省 30；`ServerConf.cs:492`
    /// 读 `[Setup] DieDropUseItemRate`，部署件 = 50）——玩家死亡 `DropUseItems` 的
    /// `PvpLevel() <= 2` 支逐槽骰 `Random(nRate)`（`PlayObject.Message.cs:2183`）。
    pub die_drop_use_item_rate: i32,
    /// `Config.DieRedDropUseItemRate`（`GameSvrConf.cs:772/1715` 缺省 15；部署件 = 15）——
    /// `PvpLevel() > 2` 支（红名）。
    pub die_red_drop_use_item_rate: i32,
    /// `Config.DropItemRage`（`GameSvrConf.cs:1754` 缺省 3，`Server.conf [Setup]`）——
    /// `DropUseItems` 的 `dropWide = MIN(DropItemRage, 3)`（`PlayObject.Message.cs:2194`）。
    pub drop_item_rage: i32,
    /// `M2Share.DisableTakeOffList` 的键集合（`M2Share.cs:1482`：`ContainsKey(Index - 1)`）——
    /// `DropUseItems` 的「禁止取下列表」门（`PlayObject.Message.cs:2191`）。
    ///
    /// **装载登记**：部署件 `Envir/DisableTakeOffList.txt` 为 **0 字节** ⇒ 真机恒空；
    /// 本仓未接文件装载（T-81），缺省空与部署件等价。
    pub disable_take_off_keys: Vec<u16>,
    /// `Config.KillByHumanDropUseItem`（`GameSvrConf.cs:780/1717` 缺省 `false`；
    /// `ServerConf.cs:495`，部署件 `=1`）——`KillTargetTrigger` 第二段
    /// （`PlayObject.cs:1785-1800`）对**玩家**击杀者的 `DropUseItems` 门。
    pub kill_by_human_drop_use_item: bool,
    /// `Config.KillByMonstDropUseItem`（`GameSvrConf.cs:784/1718` 缺省 `true`；
    /// `ServerConf.cs:496`，部署件 `=1`）——同上，对**非玩家**击杀者。
    pub kill_by_monst_drop_use_item: bool,
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
    /// **P3-B4**：`MabMabe` / 施毒术的配置项（`GameSvrConf.cs:1763/1778-1781`）。
    ///
    /// 缺省 = oracle 缺省（`hit_rand_rate=100`/`hit_min_lv_limit=10`/`hit_success_rate=21`/
    /// `hit_mabe_time_rate=20`/`amy_ounsul_point=10`）；宿主 `bins/game-svr` 用
    /// [`World::set_b4_config`] 从 `Server.conf` 的 `[Setup]` 灌入。
    pub b4_cfg: crate::magic_effect::MabMabeConfig,
    /// **P3-B5**：22/36 的配置项（`DisableInSafeZoneFireCross` + 两对 `SysMsg` 颜色）。
    /// 宿主 `bins/game-svr` 用 [`World::set_b5_config`] 从 `Server.conf` 灌入。
    pub b5_cfg: crate::magic_effect::B5Config,
    /// **P3 收尾残余**：`SystemShare.Config.MagTurnUndeadLevel`（`GameSvrConf.cs:1773` 缺省 `50`）。
    ///
    /// `MagicManager.cs:842-853` 的 `targetObject.Abil.Level < Config.MagTurnUndeadLevel` 门。
    /// 宿主 `bins/game-svr` 用 [`World::set_mag_turn_undead_level`] 从 `Server.conf` 灌入；
    /// 缺省 = oracle 源码缺省（既有 golden 路径不变）。
    pub mag_turn_undead_level: i32,
    /// **P3 收尾残余**：`SystemShare.Config.dwPKFlagTime`（`GameSvrConf.cs:1395` 缺省 `60 * 1000`）
    /// —— [`World::check_pk_status`] 的 `PvpFlag` 过期门（`PlayObject.cs:4359`）。
    pub pk_flag_time_ms: i64,
    /// `SystemShare.Config.DecLightItemDrugTime`（`GameSvrConf.cs:1398` 缺省 `500`）——
    /// oracle 里 `CheckPkStatus()` 挂在玩家 `Run()` 的该周期块内（`PlayObject.Message.cs:257-262`）。
    /// 本骨架按同一周期在 [`World::check_pk_status`] 内节流。
    pub dec_light_item_drug_time_ms: i64,
    /// 上一趟 `CheckPkStatus` 的 sim 时钟（oracle `DecLightItemDrugTick`）。
    ///
    /// **T-78 修复（惰性初始化）**：oracle 在玩家构造时用 `GetTickCount()` 初始化该游标
    /// （`PlayObject.cs:969`）；本仓的等价物是**首次调用** [`World::check_pk_status`] 的时刻——
    /// 首次调用把它钉到当时的 [`World::clock_ms`]，而不是构造时的 0。否则 `clock_ms` 被注入为
    /// 墙钟量级（`MIR2_CLOCK_MS ≈ 1.7e12`）时 `clock - 0 > 500` 恒真，节流退化为每 tick 一次。
    pub pk_check_tick: i64,
    /// [`World::pk_check_tick`] 是否已按首次调用时钟惰性初始化（T-78 修复的内部状态）。
    /// 直接写 `pk_check_tick`（宿主/测试强制提前检查）不改变该标志。
    pk_check_tick_initialized: bool,
    /// `SystemShare.Config.HealthFillTime` / `SpellFillTime`（`GameSvrConf.cs:1323-1324` 缺省
    /// `300` / `800`；`ServerConf.cs:152-153` 可覆盖）。
    ///
    /// `BaseObject.Run` 恢复段（`BaseObject.Base.cs:47/57`）的到点门；由接线层
    /// （`bins/game-svr` 的 sim 循环）用同一份配置灌入。合成 golden 场景不跑 [`World`] 的
    /// 怪物 AI（`ai_enabled=false`）⇒ 这两个值在既有对拍里不被消费。
    pub health_fill_time: i32,
    pub spell_fill_time: i32,
    /// `Config.MakeGhostTime`（`GameSvrConf.cs:431`，构造缺省 `:1486` = `3 * 60 * 1000`；
    /// `ServerConf.cs:567` 读 `[Setup] MakeGhostTime`；部署件 = 90000）。
    ///
    /// `BaseObject.Run` 死亡支（`BaseObject.Base.cs:85-101`）的两条门都读它。缺省取 oracle
    /// 构造值；宿主用 [`World::set_corpse_config`] 灌入 `Server.conf` 的值。
    pub make_ghost_time: i32,
    /// `GetMonstersZenTime`（`WorldServer.MonGen.cs:168-190`）用的 `Config.UserFull`。
    /// 与 [`crate::mongen::MonGenConfig::user_full`] 同源（宿主灌入）。
    pub mon_gen_user_full: i32,
    /// `GetMonstersZenTime` 用的 `Config.ZenFastStep`（同 [`Self::mon_gen_user_full`]）。
    pub mon_gen_zen_fast_step: i32,
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
    /// **卡 `openmir2-rm-refmessage-delay-layer`**：是否启用 7 参 `SendStruckDelayMsg`
    /// （`RM_REFMESSAGE` 延时层，`BaseObject.Message.cs:211-228`）。
    ///
    /// **缺省 `false`**：合成 golden 场景（`tests/parity/csharp/WorldGolden`）的 `_Attack`
    /// 桩是 oracle 的**取数子集**（`StubsCombat.cs:248-284` 只折叠即时 `RM_STRUCK`，
    /// 不含 `SendStruckDelayMsg`）⇒ 默认开启会把 200ms 后的 `Struck` 抽数插进既有
    /// golden 的 RNG 序列，破坏 `diff = 0`。生产宿主 `bins/game-svr` 在起世界时
    /// [`World::enable_refmessage_delay`] 打开（与 `enable_monster_ai` 同口径）。
    refmessage_delay_enabled: bool,
    /// **P3-B2**：待处理的 `RM_MAGSTRUCK`（目标自有队列的插入序等价物）。
    ///
    /// 由效果体（23/24/33/47）入队，宿主调 `World::deliver_mag_struck` 逐目标处理
    /// （`R(1000)` → `MSD` → `nDamage>0` 链）。队列为空时零行为变化。
    pub pending_mag_struck: Vec<PendingMagStruck>,
    /// **卡 `openmir2-run-gap-monster-specials`**：待执行的 `Operate(RM_ZEN_BEE)`
    /// （`BeeQueen`/`SpiderHouseMonster` 产仔），由 [`crate::slave::World::deliver_self_delay_msgs`]
    /// 在 500ms 自投递到点时入队；宿主用 [`World::flush_special_spawns`] 在同一条 `rng` 上造怪。
    pub pending_special_spawns: Vec<crate::special_monster::SpecialSpawnRequest>,
    /// 专用怪 `SendRefMsg(RM_HIT, …)` 的帧事件（宿主出 `SM_HIT`；保序）。
    pub pending_special_ref_msgs: Vec<crate::special_monster::SpecialRefMsg>,
    /// **MB-1**：怪物 `Run` 覆写里的 `SendRefMsg(...)` 帧事件（`StickMonster`/
    /// `DigOutZombi`/`ScultureMonster`/`BigHeartMonster`），宿主按 ident 出
    /// `SM_DIGUP`/`SM_DIGDOWN`/`SM_HIT`/`SM_716`（保序；见 [`crate::monai::MonsterRefMsg`]）。
    pub pending_monster_ref_msgs: Vec<crate::monai::MonsterRefMsg>,
    /// **卡 `openmir2-rm-refmessage-delay-layer`**：`Operate(RM_REFMESSAGE)`
    /// （`BaseObject.Base.cs:699-710`）的 `SendRefMsg(RM_STRUCK, …)` 广播事件（保序）。
    ///
    /// 入列时刻 = 延时消息**到点**（`World::deliver_self_delay_msgs`），因此宿主从这里出的
    /// `SM_STRUCK` 天然带 oracle 的 200/300/500ms 广播延时；载荷是发送时刻的
    /// HP/MaxHP 快照（`SendStruckDelayMsg` 的 `nParam1/nParam2`）。
    pub pending_refmessage_frames: Vec<crate::slave::RefMessageFrame>,
    /// `GameSvrConf.Bee`（`GameSvrConf.cs:1316` 缺省 `"蝙蝠"`；`ServerConf.cs:100` 读 `[Names] Bee`）。
    pub bee_name: String,
    /// `GameSvrConf.Spider`（`GameSvrConf.cs:1317` 缺省 `"爆裂蜘蛛"`；`ServerConf.cs:101` 读 `[Names] Spider`）。
    pub spider_name: String,
    /// **MB-3（R34）**：`GameSvrConf.Zuma[4]`（`GameSvrConf.cs:159/1315`；`ServerConf.cs:96-99`
    /// 读 `[Names] Zuma1..4`）——`ScultureKingMonster.CallSlave` 的
    /// `Config.Zuma[Random(4)]` 怪名表。缺省 = oracle 构造缺省（部署件同值）。
    pub zuma_names: [String; 4],
    /// **MB-3（R34）**：待执行的 `RegenMonsterByName` 请求（`ScultureKingMonster.CallSlave`）。
    /// 宿主用 [`crate::monai::World::flush_sculture_spawns`]（或同签名方法）在 `world.rng` 上排水。
    pub pending_sculture_spawns: Vec<crate::monai::ScultureSpawnRequest>,
    /// **P3-B5**：`SystemShare.EventMgr.Events` 的火墙事件表（插入序 = `EventMgr.AddEvent` 序）。
    ///
    /// `EventProcessor.ExecuteInternal`（`:31-48`）逆插入序遍历、每事件 `RunTick=500ms` 门；
    /// `Closed` 的事件留在表里但不再 `Run`（oracle 移到 `ClosedEvents`，语义等价）。
    /// 缺省空表 ⇒ 既有 golden 逐字节不变。
    pub fire_burn_events: Vec<FireBurnEvent>,
    /// **MB-1**：`EventMgr.Events` 里 `ET_DIGOUTZOMBI` 事件的最小表（插入序）。
    ///
    /// 只由 `DigOutZombi.Turned` 写入（5 分钟 `ContinueTime`），由 [`World::run_map_events`]
    /// 按 `RunTick=500ms` 门到期移除；`PlayObject.Walk` 的 NEEDHOLE 门读它。
    pub dig_out_events: Vec<DigOutEvent>,
    /// **P3-B5**：`CellObjectMgr` 里是否已有 `CellObjId = 0` 的事件（`TryAdd` 第一条胜）。
    ///
    /// `Envir.GetEvent(x,y)` 的判据 = 「该格有 `CellType::Event` 对象」**且**本标记为真
    /// （`:1344-1360` 先找格对象，再 `CellObjectMgr.Get<MapEvent>(0)`）。
    pub event_registered: bool,
    /// 自身延时消息队列（oracle `BaseObject.MsgQueue` 的已建模子集，**每 actor 一把**）。
    ///
    /// [`crate::slave::SelfDelayQueue`] 是**四叉堆（.NET 8 `Arity=4`）**，与
    /// `PriorityQueue<SendMessage, byte>`（`ActorEntity.cs:21`）逐操作等价：
    /// `Enqueue` sift-up（父 `(i-1) >> 2`）/ `Dequeue`「末元素移根 + sift-down」
    /// （首子 `(i << 2) + 1`，四子取首个严格最小）、比较为**严格小于**。
    /// 因此等优先级时堆是**不稳定**的，
    /// 未到点重入队（`GetMessage`，`BaseObject.Message.cs:334-378`）后下一次泵会先投递
    /// 其它已到点项——必须照搬，不能退回插入序/FIFO。
    ///
    /// 投递语义见 [`crate::slave::World::deliver_self_delay_msgs`]：泵 = `peek → 未到点则
    /// 出队并以 `Lowest` 重入队 + 结束本次调用`；**没有跨调用的 blocked 状态**；不同 actor
    /// 的队列互不阻塞。
    /// 优先级：`SendSelfDelayMsg` / `SendStruckDelayMsg` 一律 `Lowest`(255)；即时
    /// `SendMsg(int…)` 用 `(byte)wIdent`（本层只建模 `RM_MAKEHOLYSEIZEMODE` 的 `send_msg_now`）。
    /// 产者：`RM_MASTERDIEMUTINY`/`RM_MASTERDIEGHOST`（玩家 1000ms）、`RM_MAGHEALING` 800ms、
    /// `RM_TRANSPARENT` 800ms、`RM_DOOPENHEALTH` 1500ms、`RM_MAGIC_LVEXP` 800/1000ms、
    /// `RM_MAKEHOLYSEIZEMODE`（即时）、`RM_DELAYMAGIC`/`RM_POISON`、`RM_MAGSTRUCK` 600ms
    /// （8 参 `SendStruckDelayMsg`，目标队列）、`RM_ZEN_BEE` 500ms（BeeQueen/SpiderHouse）。
    /// R19 的真实生产路径是 `FireballMonster.cs:41` 的 **7 参** `SendStruckDelayMsg`
    /// （入怪**自己**的队列，ident = `RM_DELAYMAGIC`），生产者接线归 MB-9。
    pub self_delay_msgs: crate::slave::SelfDelayQueue,
    /// 消息创建序计数器（自投递入队与即时 `pending_mag_struck` 入列共用）。
    ///
    /// 只用于宿主按创建序排水 [`PendingMagStruck`]（见其 `order` 字段），不参与队列比较、
    /// 战斗数值或落库。
    pub(crate) msg_order: u64,
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

/// [`World::cast_space_move_spell`] 的可观测结果（魔号 21 `MagSaceMove` `MagicManager.cs:876-891`）。
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SpaceMoveCastOutcome {
    /// oracle `MagSaceMove` 的返回值（`Random(11)` 门通过即 `true`，与是否找到 HomeMap/是否落地无关）。
    pub result: bool,
    /// `MapRandomMove` 找到了 HomeMap（`MapMgr.FindMap(HomeMap) != null`）。
    pub map_found: bool,
    /// `SpaceMoveGetRandXY` 找到落点并 `AddMapObject` 成功。
    pub moved: bool,
    /// 落地后的坐标（未移动时 = 调用前坐标）。
    pub x: i32,
    pub y: i32,
    /// 落地后的 `map_id`（未移动时 = 调用前 `map_id`）。
    pub map_id: usize,
    /// oracle `SpaceMoved = true`（仅成功落地）。
    pub space_moved: bool,
    /// oracle `if (envir != playObject.Envir && Race == ActorRace.Play) IsTimeRecall = false;`
    /// —— 本骨架无 `IsTimeRecall` 字段（登记）；这里只回报「是否跨图」供宿主/测试判读。
    pub cross_map: bool,
}

/// `BaseObject.Run` 的 `VerifyTick` 30s 门（`BaseObject.Base.cs:223`：`> 30 * 1000`）。
///
/// 命中后 `Envir.VerifyMapTime` 刷新本格 `CellObject.AddTime`（`Envirnoment.cs:858-897`），
/// 静止 actor（卫士 / 固定 NPC / 停走怪）因此不会在 60s 后被 AOI 扫描当作过期项逐出。
pub const VERIFY_MAP_TIME_MS: i64 = 30 * 1000;

/// `BaseObject.CellType` 的等价物（`PlayObject.cs:1034` = Play、`AnimalObject.cs:86` = Monster、
/// `NormNpc.cs:50` = Merchant）；`verify_map_time` 重新登记时用它保持落格类型。
pub(crate) fn entity_cell_type(e: &Entity) -> CellType {
    if e.merchant.is_some() || e.npc.is_some() {
        CellType::Merchant
    } else if e.race == crate::entity::ACTOR_RACE_PLAY {
        CellType::Play
    } else {
        CellType::Monster
    }
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
            die_drop_use_item_rate: 30,
            die_red_drop_use_item_rate: 15,
            drop_item_rage: 3,
            disable_take_off_keys: Vec::new(),
            kill_by_human_drop_use_item: false,
            kill_by_monst_drop_use_item: true,
            item_catalog: Vec::new(),
            mon_random_add_value: 40,
            need_exps: vec![0; 1000],
            combat_config: CombatConfig::default(),
            party_cfg: crate::party::PartyConfig::default(),
            b4_cfg: crate::magic_effect::MabMabeConfig::default(),
            b5_cfg: crate::magic_effect::B5Config::default(),
            mag_turn_undead_level: 50,
            pk_flag_time_ms: 60 * 1000,
            dec_light_item_drug_time_ms: 500,
            pk_check_tick: 0,
            pk_check_tick_initialized: false,
            health_fill_time: 300,
            spell_fill_time: 800,
            make_ghost_time: 3 * 60 * 1000,
            mon_gen_user_full: 5000,
            mon_gen_zen_fast_step: 300,
            clear_drop_on_floor_item_time: 5 * 60 * 1000,
            map_exp_rate: vec![None; maps_len],
            spirit_mutiny_tick: 0,
            slave_self_delay_enabled: false,
            pending_mag_struck: Vec::new(),
            pending_special_spawns: Vec::new(),
            pending_special_ref_msgs: Vec::new(),
            pending_refmessage_frames: Vec::new(),
            refmessage_delay_enabled: false,
            pending_monster_ref_msgs: Vec::new(),
            bee_name: crate::special_monster::DEFAULT_BEE_NAME.to_string(),
            spider_name: crate::special_monster::DEFAULT_SPIDER_NAME.to_string(),
            zuma_names: [
                "祖玛卫士".to_string(),
                "祖玛雕像".to_string(),
                "祖玛弓箭手".to_string(),
                "楔蛾".to_string(),
            ],
            pending_sculture_spawns: Vec::new(),
            fire_burn_events: Vec::new(),
            dig_out_events: Vec::new(),
            event_registered: false,
            self_delay_msgs: crate::slave::SelfDelayQueue::new(),
            msg_order: 0,
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

    /// 灌入尸体/复活面配置（`Config.MakeGhostTime` + `GetMonstersZenTime` 的两项）。
    ///
    /// 生产宿主在起世界时用 `Server.conf` 的 `[Setup] MakeGhostTime` 与 `[Server]
    /// UserFull/ZenFastStep` 调用；合成 golden/单测不调 ⇒ 保持 oracle 构造缺省，
    /// 既有对拍逐字节不变（本组字段只在死亡支/复活路径被读）。
    pub fn set_corpse_config(
        &mut self,
        make_ghost_time: i32,
        mon_gen_user_full: i32,
        mon_gen_zen_fast_step: i32,
    ) {
        self.make_ghost_time = make_ghost_time;
        self.mon_gen_user_full = mon_gen_user_full;
        self.mon_gen_zen_fast_step = mon_gen_zen_fast_step;
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

    /// **P3-B4**：灌入 `MabMabe`/施毒术配置（`Server.conf` 的 `[Setup]`）。
    /// **P3-B5**：灌入 22/36 的配置项（缺省 = oracle 源码缺省；宿主从 `Server.conf` 覆盖）。
    pub fn set_b5_config(&mut self, cfg: crate::magic_effect::B5Config) {
        self.b5_cfg = cfg;
    }

    pub fn set_b4_config(&mut self, cfg: crate::magic_effect::MabMabeConfig) {
        self.b4_cfg = cfg;
    }

    /// **P3 收尾残余**：灌入 `SystemShare.Config.MagTurnUndeadLevel`（`Server.conf [Setup]`）。
    /// 缺省已经是 oracle 源码缺省 50；宿主仅在配置层读到键时调用。
    pub fn set_mag_turn_undead_level(&mut self, level: i32) {
        self.mag_turn_undead_level = level;
    }

    /// **P3 收尾残余**：灌入 `PKFlagTime` / `DecLightItemDrugTime`（`Server.conf [Setup]`）。
    ///
    /// `pk_flag_time_ms` = `Config.dwPKFlagTime`（缺省 60_000）；
    /// `dec_light_item_drug_time_ms` = `Config.DecLightItemDrugTime`（缺省 500，`CheckPkStatus`
    /// 的调用周期）。
    pub fn set_pk_flag_config(&mut self, pk_flag_time_ms: i64, dec_light_item_drug_time_ms: i64) {
        self.pk_flag_time_ms = pk_flag_time_ms;
        self.dec_light_item_drug_time_ms = dec_light_item_drug_time_ms;
    }

    /// `PlayObject.CheckPkStatus()`（`PlayObject.cs:4357-4364`）的**全世界**等价物。
    ///
    /// 调度口径：oracle 把它挂在玩家 `Run()` 的 `DecLightItemDrugTime` 周期块里
    /// （`PlayObject.Message.cs:257-262`，缺省 500ms，`DecLightItemDrugTick += 周期`），
    /// 本骨架在同一方法内按 [`World::dec_light_item_drug_time_ms`] 节流；宿主每 world tick
    /// 调一次即可（200ms 量化与 oracle 的真实墙钟 tick 差异登记）。
    ///
    /// ```text
    /// :4359 if (PvpFlag && ((GetTickCount() - PvpNameColorTick) > Config.dwPKFlagTime))
    /// :4361   PvpFlag = false;
    /// :4362   RefNameColor();
    /// ```
    ///
    /// 返回 `RefNameColor` 的观察值（`RM_CHANGENAMECOLOR` 的宿主侧载荷）：色 = 清位后
    /// `GetChrColor`（玩家 ⇒ `NameColor`，本骨架 ctor = 255）。
    pub fn check_pk_status(&mut self) -> Vec<crate::slave::NameColorDelivery> {
        // oracle 的 `DecLightItemDrugTick` 在玩家构造时 = `GetTickCount()`（`PlayObject.cs:969`），
        // 即从「创建时刻」起算；本仓的等价物是**首次调用**时刻。若沿用构造时的 0，注入墙钟
        // （`MIR2_CLOCK_MS ≈ 1.7e12`）后 `clock - 0 > 500` 恒真 ⇒ 节流失效（T-78）。
        if !self.pk_check_tick_initialized {
            self.pk_check_tick = self.clock_ms;
            self.pk_check_tick_initialized = true;
        }
        if self.clock_ms - self.pk_check_tick <= self.dec_light_item_drug_time_ms {
            return Vec::new();
        }
        self.pk_check_tick += self.dec_light_item_drug_time_ms;
        let now = self.clock_ms;
        let mut out = Vec::new();
        for id in self.entities.ids_sorted() {
            let Some(e) = self.entities.get(id) else {
                continue;
            };
            if e.race != crate::entity::ACTOR_RACE_PLAY {
                continue;
            }
            if !e.pvp_flag || now - e.pvp_name_color_tick <= self.pk_flag_time_ms {
                continue;
            }
            let Some(t) = self.entities.get_mut(id) else {
                continue;
            };
            t.pvp_flag = false;
            let color = crate::slave::get_chr_color(t, &[], 0);
            out.push(crate::slave::NameColorDelivery {
                target_id: id,
                color,
            });
        }
        out
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
        let group_members = self.attacker_group_members(&caster);
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
                    let in_group = group_members.contains(&id);
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

    /// `SKILL_FIREBALL(1)` / `SKILL_FIREBALL2(5)` 效果体（`MagicManager.cs:174-203`）。
    ///
    /// 逐行顺序（**取数序即 oracle 表达式序**）：
    /// ```text
    /// :176 MagCanHitTarget(CurrX, CurrY, target)                       纯门
    /// :178 IsProperTarget(target)                                      纯门
    /// :180 target.AntiMagic <= Random(10)                              ← 第 1 抽
    /// :180 |target.CurrX - nTargetX| <= 1 && |...Y| <= 1               纯门
    /// :182 nPower = GetAttackPower(GetPower(MPow) + Lo(MC), ΔMC+1)     ← 第 2..5 抽
    /// :183 SendSelfDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx,ty), 2, target.ActorId, "", 600)
    /// :184 if (target.Race >= Animal) boTrain = true
    /// else 各失败支 ⇒ targetObject = null（共享尾 ActorId=0）
    /// ```
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_fireball_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        target_x: i32,
        target_y: i32,
        anti_magic: i32,
        mc: i32,
    ) -> SpellStrikeOutcome {
        let Some((caster, t, tid)) = self.spell_strike_targets(caster_id, target) else {
            return SpellStrikeOutcome::default();
        };
        if !self.spell_can_hit(&caster, &t) {
            return SpellStrikeOutcome::default();
        }
        if !crate::combat::is_proper_target_grouped(
            &caster,
            &t,
            &self.attacker_group_members(&caster),
        ) {
            return SpellStrikeOutcome::default();
        }
        // `:180 targetObject.AntiMagic <= Random(10)`；失败 ⇒ `targetObject = null`。
        if !crate::magic_effect::anti_magic_roll(&mut self.rng, anti_magic) {
            return SpellStrikeOutcome::default();
        }
        if (t.x - target_x).abs() > 1 || (t.y - target_y).abs() > 1 {
            return SpellStrikeOutcome::default();
        }
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::single_hit_power_mc(&mut self.rng, magic, level, mc, atk);
        SpellStrikeOutcome {
            target: Some(tid),
            bo_train: t.race >= crate::entity::ACTOR_RACE_ANIMAL,
            power,
            delayed: true,
            delay_ms: 600,
            push: None,
        }
    }

    /// `SKILL_LIGHTENING(11)` / `SKILL_45(45)` 效果体（`MagicManager.cs:289-314` / `:563-588`）。
    ///
    /// 两个 case 的语句逐字相同（`:289-314` 与 `:563-588`）：
    /// ```text
    /// :291 IsProperTarget(target)                                      纯门
    /// :292 Random(10) >= target.AntiMagic                              ← 第 1 抽
    /// :294 nPower = GetAttackPower(GetPower(MPow) + Lo(MC), ΔMC+1)     ← 第 2..5 抽
    /// :296 if (target.LifeAttrib == LA_UNDEAD) nPower = Round(nPower * 1.5)
    /// :299 SendSelfDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx,ty), 2, target.ActorId, "", 600)
    /// :300 if (target.Race >= Animal) boTrain = true
    /// else 各失败支 ⇒ targetObject = null
    /// ```
    /// **注意**：11/45 **没有** `MagCanHitTarget` 门（与 1/5 不同）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_lightening_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        anti_magic: i32,
        mc: i32,
    ) -> SpellStrikeOutcome {
        let Some((caster, t, tid)) = self.spell_strike_targets(caster_id, target) else {
            return SpellStrikeOutcome::default();
        };
        if !crate::combat::is_proper_target_grouped(
            &caster,
            &t,
            &self.attacker_group_members(&caster),
        ) {
            return SpellStrikeOutcome::default();
        }
        if !crate::magic_effect::anti_magic_roll(&mut self.rng, anti_magic) {
            return SpellStrikeOutcome::default();
        }
        let atk = AttackPowerMode::for_attacker(&caster);
        let mut power =
            crate::magic_effect::single_hit_power_mc(&mut self.rng, magic, level, mc, atk);
        if t.life_attrib == crate::magic_effect::LA_UNDEAD {
            // `nPower = (ushort)HUtil32.Round(nPower * 1.5)`（`:296`）。
            power = mir2_shared::hutil::round(f64::from(power) * 1.5);
        }
        SpellStrikeOutcome {
            target: Some(tid),
            bo_train: t.race >= crate::entity::ACTOR_RACE_ANIMAL,
            power,
            delayed: true,
            delay_ms: 600,
            push: None,
        }
    }

    /// `SKILL_FIRECHARM(13)` 效果体（`MagicManager.cs:328-351`）。
    ///
    /// 护身符门（`:322-325` `CheckAmulet`/`UseAmulet`）由**调用方**（宿主会话层）完成；
    /// 门不过时 oracle `boSpellFail` 保持 true ⇒ 不调用本函数（无效果体、零抽、尾不发）。
    ///
    /// 逐行顺序：
    /// ```text
    /// :330 MagCanHitTarget(CurrX, CurrY, target)     纯门；**失败 ⇒ targetObject = null**
    /// :332 IsProperTarget(target)                    纯门；失败 ⇒ target 保持（不置 null）
    /// :334 Random(10) >= target.AntiMagic            ← 第 1 抽；失败 ⇒ target 保持
    /// :336 |target.CurrX - nTargetX| <= 1 && |...Y| <= 1   纯门；失败 ⇒ target 保持
    /// :337 nPower = GetAttackPower(GetPower(MPow) + Lo(SC), ΔSC+1)  ← 第 2..5 抽
    /// :338 SendSelfDelayMsg(RM_DELAYMAGIC, nPower, MakeLong(tx,ty), 2, target.ActorId, "", 1200)
    /// :339 if (target.Race >= Animal) boTrain = true
    /// ```
    /// ⇒ **只有 `MagCanHitTarget` 失败才把 target 置 null**；其余失败支目标保留，
    /// 共享尾 `RM_MAGICFIRE` 仍带该 ActorId（oracle 的非对称行为，必须照抄）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_firecharm_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        target_x: i32,
        target_y: i32,
        anti_magic: i32,
        sc: i32,
    ) -> SpellStrikeOutcome {
        let Some((caster, t, tid)) = self.spell_strike_targets(caster_id, target) else {
            return SpellStrikeOutcome::default();
        };
        if !self.spell_can_hit(&caster, &t) {
            return SpellStrikeOutcome::default();
        }
        // 以下失败支**保持** targetObject（`MagicManager.cs:332-341` 无 else）。
        let stayed = SpellStrikeOutcome {
            target: Some(tid),
            ..SpellStrikeOutcome::default()
        };
        if !crate::combat::is_proper_target_grouped(
            &caster,
            &t,
            &self.attacker_group_members(&caster),
        ) {
            return stayed;
        }
        if !crate::magic_effect::anti_magic_roll(&mut self.rng, anti_magic) {
            return stayed;
        }
        if (t.x - target_x).abs() > 1 || (t.y - target_y).abs() > 1 {
            return stayed;
        }
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::single_hit_power_sc(&mut self.rng, magic, level, sc, atk);
        SpellStrikeOutcome {
            target: Some(tid),
            bo_train: t.race >= crate::entity::ACTOR_RACE_ANIMAL,
            power,
            delayed: true,
            delay_ms: 1200,
            push: None,
        }
    }

    /// `SKILL_44(44)` 效果体 —— `MagicManager.MagHbFireBall`（`MagicManager.cs:1020-1064`）
    /// 的调用点 `case SKILL_44`（`:557-562`）。
    ///
    /// 逐行顺序：
    /// ```text
    /// :1023 MagCanHitTarget(CurrX, CurrY, target)            纯门；失败 ⇒ target = null、return false
    /// :1028 IsProperTarget(target)                           纯门；失败 ⇒ target = null、return false
    /// :1032 target.AntiMagic > Random(10) || |Δx|>1 || |Δy|>1  ← 第 1 抽；失败 ⇒ target = null
    /// :1038 nPower = GetAttackPower(GetPower(MPow) + Lo(MC), ΔMC+1)  ← 第 2..5 抽
    /// :1039 SendSelfDelayMsg(RM_DELAYMAGIC, nPower, ..., target.ActorId, "", 600)
    /// :1040 if (target.Race >= Animal) result = true
    /// :1044 if (caster.Abil.Level > target.Abil.Level && !target.StickMode)
    /// :1047   if (Random(20) < 6 + Level*3 + levelgap)        ← 第 6 抽
    /// :1049     push = Random(Level) - 1                      ← 第 7 抽（Level=0 也抽）
    /// :1050     if (push > 0) SendSelfDelayMsg(RM_DELAYPUSHED, ...)
    /// ```
    /// `boTrain` = `result`（只看 `Race >= Animal`，与推挤是否命中无关）。
    /// `StickMode` 读目标实体的 `Entity.stick_mode`（普通目标缺省 false）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_hb_fireball_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        target_x: i32,
        target_y: i32,
        anti_magic: i32,
        mc: i32,
    ) -> SpellStrikeOutcome {
        let Some((caster, t, tid)) = self.spell_strike_targets(caster_id, target) else {
            return SpellStrikeOutcome::default();
        };
        if !self.spell_can_hit(&caster, &t) {
            return SpellStrikeOutcome::default();
        }
        if !crate::combat::is_proper_target_grouped(
            &caster,
            &t,
            &self.attacker_group_members(&caster),
        ) {
            return SpellStrikeOutcome::default();
        }
        // `:1032 targetObject.AntiMagic > Random(10) || ...`（短路口径与
        // `Random(10) >= AntiMagic` 失败等价；距离检查纯量）。
        if !crate::magic_effect::anti_magic_roll(&mut self.rng, anti_magic) {
            return SpellStrikeOutcome::default();
        }
        if (t.x - target_x).abs() > 1 || (t.y - target_y).abs() > 1 {
            return SpellStrikeOutcome::default();
        }
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::single_hit_power_mc(&mut self.rng, magic, level, mc, atk);
        let push = crate::magic_effect::hb_fireball_push(
            &mut self.rng,
            level,
            caster.level,
            t.level,
            t.stick_mode, // `BaseObject.StickMode`（`BeeQueen`/`SpiderHouseMonster` ctor 置 true）
        );
        SpellStrikeOutcome {
            target: Some(tid),
            bo_train: t.race >= crate::entity::ACTOR_RACE_ANIMAL,
            power,
            delayed: true,
            delay_ms: 600,
            push,
        }
    }

    // ====================== P3-B4（毒族 / 群体列表：6/48/50/42）======================

    /// `PlayObject.PvpLevel()`（`PlayObject.cs:4352`）：`(byte)(PkPoint / 100)`。
    #[must_use]
    pub fn pvp_level(e: &Entity) -> u8 {
        u8::try_from(e.pk_point / 100).unwrap_or(0)
    }

    /// `GameSvrConf.PKFlagNameColor`（`GameSvrConf.cs:1731` 缺省 `0x2F`）。
    pub const PK_FLAG_NAME_COLOR: u8 = 0x2F;

    /// `PlayObject.SetPkFlag(IActor)`（`PlayObject.cs:4326-4341`）。
    ///
    /// ```text
    /// :4328 if (baseObject.Race == Play)
    /// :4331   if (PvpLevel() < 2 && targetObject.PvpLevel() < 2
    ///            && !Envir.Flag.FightZone && !Envir.Flag.Fight3Zone && !PvpFlag)
    /// :4333     targetObject.PvpNameColorTick = GetTickCount();
    /// :4336     if (!targetObject.PvpFlag) { targetObject.PvpFlag = true; targetObject.RefNameColor(); }
    /// ```
    ///
    /// 本仓 `FightZone`/`Fight3Zone` 未建模 ⇒ 恒 `false`（不阻塞置位）。
    /// 返回 `RefNameColor` 的观察值（由 **`target_id`** 发出；颜色按接收侧
    /// `PlayObject.GetChrColor` 的 `PvpFlag` 支 = `PKFlagNameColor`）。
    pub fn set_pk_flag(
        &mut self,
        this_id: i32,
        target_id: i32,
    ) -> Option<crate::slave::NameColorDelivery> {
        // oracle 的判据全在 `this`/`target` 的只读字段上；先取值再释放借用（供下面写计时）。
        let (this_level, this_flagged, target_level, target_race, target_flagged) = {
            let this = self.entities.get(this_id)?;
            let target = self.entities.get(target_id)?;
            (
                Self::pvp_level(this),
                this.pvp_flag,
                Self::pvp_level(target),
                target.race,
                target.pvp_flag,
            )
        };
        // `baseObject.Race == ActorRace.Play`
        if target_race != crate::entity::ACTOR_RACE_PLAY {
            return None;
        }
        // `PvpLevel() < 2 && targetObject.PvpLevel() < 2 && !PvpFlag`
        if this_level >= 2 || target_level >= 2 || this_flagged {
            return None;
        }
        // `:4333` 计时**无条件刷新**（在 `if (!targetObject.PvpFlag)` 之前）：目标已带 flag 时
        // 再来一次 `SetPkFlag` 不产帧，但会把过期时刻推后（oracle 原样，不许「顺手」改成早退）。
        let now = self.clock_ms;
        if let Some(t) = self.entities.get_mut(target_id) {
            t.pvp_name_color_tick = now;
        }
        if target_flagged {
            return None;
        }
        if let Some(t) = self.entities.get_mut(target_id) {
            t.pvp_flag = true;
        }
        Some(crate::slave::NameColorDelivery {
            target_id,
            color: Self::PK_FLAG_NAME_COLOR,
        })
    }

    /// 目标的 `AntiPoison`（`BaseObject.cs:109`）。唯一注入面 = [`World::attack_poison`]；
    /// 未注入 ⇒ 0（普通 `MonGen` 怪与 oracle 普通生成路径同）。
    #[must_use]
    pub fn target_anti_poison(&self, target_id: i32) -> i32 {
        self.attack_poison
            .get(&target_id)
            .map_or(0, |s| i32::from(s.anti_poison))
    }

    /// 目标的 `UnParalysis`（`BaseObject.cs:85`）。注入面同 [`World::target_anti_poison`]；
    /// oracle 全仓无赋值点 ⇒ 恒 `false`。
    #[must_use]
    pub fn target_un_paralysis(&self, target_id: i32) -> bool {
        self.attack_poison
            .get(&target_id)
            .is_some_and(|s| s.un_paralysis)
    }

    /// `SKILL_AMYOUNSUL(6)` / `SKILL_GROUPAMYOUNSUL(48)` 的**单目标效果体**
    /// （`MagicManager.cs:235-252` / `:914-932`）。
    ///
    /// 护身符门（`CheckAmulet`/`UseAmulet`）与 `SetTargetCreat` 在宿主侧完成（需要会话 `UseItems`）；
    /// 本函数承接骰门之后的**取数与投毒**：
    /// ```text
    /// :235/:914 if (Random(antiPoison + 7) <= 6) {
    /// :237/:917   switch (stdItem.Shape)
    ///               case 1: nPower = GP13(40) + RP(SC)*2; RM_POISON(DECHEALTH, nPower, caster, point, 1000)
    ///               case 2: nPower = GP13(30) + RP(SC)*2; RM_POISON(DAMAGEARMOR, ...)
    /// :248/:928   if (Race == Play || Race >= Animal) boTrain = true
    /// ```
    /// `shape` 来自 `UseItems[nAmuletIdx].Index` 的 `StdItem.Shape`（同一确定性表；见合同 §2.5）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_poison_on_target(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target_id: i32,
        sc: i32,
        shape: u8,
        cfg: &crate::magic_effect::MabMabeConfig,
    ) -> PoisonStrikeOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return PoisonStrikeOutcome::default();
        };
        let Some(target) = self.entities.get(target_id).cloned() else {
            return PoisonStrikeOutcome::default();
        };
        let _ = caster;
        let anti_poison = self.target_anti_poison(target_id);
        let mut out = PoisonStrikeOutcome {
            target: Some(target_id),
            ..PoisonStrikeOutcome::default()
        };
        // `:235/:914 Random(AntiPoison + 7) <= 6`（失败 ⇒ 不抽 GP13/RP）。
        if !crate::magic_effect::poison_dice(&mut self.rng, anti_poison) {
            return out;
        }
        // `:237/:917 switch (stdItem.Shape)`——只有 1/2 两支可达（合同 §2.5）。
        let (n_int, poison_type) = match shape {
            1 => (40, 0u8), // DECHEALTH
            2 => (30, 1u8), // DAMAGEARMOR
            _ => return out,
        };
        let power = crate::magic::power_poison(&mut self.rng, magic, level, sc, n_int);
        let point = crate::magic_effect::poison_point(level, power, cfg.amy_ounsul_point);
        // `SendSelfDelayMsg(RM_POISON, type, nPower, caster, point, "", 1000)`
        // ⇒ wParam=type、nParam1(时间)=nPower、nParam2=caster、nParam3=point。
        self.send_self_delay_msg_full(
            target_id,
            crate::slave::RM_POISON,
            i32::from(poison_type),
            power,
            caster_id,
            point,
            1000,
        );
        out.poisoned = true;
        out.power = power;
        out.poison_type = poison_type;
        out.time = power;
        out.point = point;
        out.bo_train = target.race == crate::entity::ACTOR_RACE_PLAY
            || target.race >= crate::entity::ACTOR_RACE_ANIMAL;
        out
    }

    /// `SKILL_GROUPAMYOUNSUL(48)` 的**逐目标循环**（`MagicManager.cs:893-940`）。
    ///
    /// `use_amulet(target_id)` 由宿主实现：执行 `CheckAmulet` + `UseAmulet`（帧面 642/202 也由
    /// 宿主出），成功返回 `Some(StdItem.Shape)`，失败返回 `None`（⇒ 该目标**零抽**且
    /// **不** `SetTargetCreat`）。回调按枚举序（`x→y→格内 ObjList 插入序`）逐目标调用，
    /// 因此护身符消耗/帧面与 oracle 的循环体同序。
    ///
    /// 本函数返回 `boTrain`（任一目标命中 `Race == Play || >= Animal`）。
    #[allow(clippy::too_many_arguments)]
    pub fn cast_group_poison_spell<F>(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target_x: i32,
        target_y: i32,
        sc: i32,
        cfg: &crate::magic_effect::MabMabeConfig,
        mut use_amulet: F,
    ) -> GroupPoisonOutcome
    where
        F: FnMut(i32) -> Option<u8>,
    {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return GroupPoisonOutcome::default();
        };
        let range = i32::from(level).max(1);
        let mut out = GroupPoisonOutcome::default();
        for tid in self.mag_objects_in_range(caster.map_id, target_x, target_y, range) {
            if tid == caster_id {
                continue;
            }
            let Some(t) = self.entities.get(tid).cloned() else {
                continue;
            };
            if t.death || t.ghost {
                continue;
            }
            if !crate::combat::is_proper_target_grouped(
                &caster,
                &t,
                &self.attacker_group_members(&caster),
            ) {
                continue;
            }
            let Some(shape) = use_amulet(tid) else {
                continue;
            };
            // `:934 playObject.SetTargetCreat(baseObject);`（在 `if (CheckAmulet)` 内）。
            if let Some(c) = self.entities.get_mut(caster_id) {
                c.ai.target_creature = Some(tid);
            }
            let one = self.cast_poison_on_target(caster_id, magic, level, tid, sc, shape, cfg);
            out.bo_train |= one.bo_train;
            out.targets.push(one);
        }
        out
    }

    /// `SKILL_MABE(50)` 效果体 —— `case` `MagicManager.cs:530-536` + helper `:1230-1267`。
    ///
    /// **case 级的 4 抽无条件发生**（`targetObject == null` 也抽）：
    /// `nPower = GetAttackPower(GetPower(MPow) + Lo(MC), ΔMC + 1)`
    /// （`MPow` → `GetPower` → `GetAttackPower`）。
    ///
    /// helper 的门序（短路，逐抽见合同 §3.3）：
    /// `MagCanHitTarget` → `IsProperTarget` → `R(10)` 抗魔门 → 距离 ≤1 →
    /// **入队第一条 `RM_DELAYMAGIC(nPower/3, ..., 600)`** → `R(2)` 等级门 →
    /// `R(hit_rand_rate)` → `R(hit_success_rate)` → 成功臂 `GetMagStruckDamage`（第 9 抽）→
    /// 入队第二条 `RM_DELAYMAGIC(post-MSD nPower, ..., 600)` → 玩家且 `!UnParalysis` 再
    /// `R(nLevel)` 并投 `RM_POISON(STONE, ..., 650)`。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_mab_mabe_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        target_x: i32,
        target_y: i32,
        mc: i32,
        cfg: &crate::magic_effect::MabMabeConfig,
    ) -> MabMabeSpellOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return MabMabeSpellOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        // `:531` —— 4 抽无条件。
        let power = crate::magic::power_get_power_plus_mc(&mut self.rng, magic, level, mc, atk);
        let mut out = MabMabeSpellOutcome {
            target,
            power,
            ..MabMabeSpellOutcome::default()
        };
        let Some(tid) = target else {
            return out;
        };
        let Some(t) = self.entities.get(tid).cloned() else {
            return out;
        };
        // `:1233 MagCanHitTarget` / `:1235 IsProperTarget`（纯门、零抽）。
        if !self.spell_can_hit(&caster, &t)
            || !crate::combat::is_proper_target_grouped(
                &caster,
                &t,
                &self.attacker_group_members(&caster),
            )
        {
            return out;
        }
        // `:1237` 的 `&&` 里距离门与 R(10) 同一条件（距离失败 ⇒ 不抽 R(2)）。
        let in_range = (t.x - target_x).abs() <= 1 && (t.y - target_y).abs() <= 1;
        let gates = crate::magic_effect::mab_mabe_gates(
            &mut self.rng,
            i32::from(t.anti_magic),
            in_range,
            i32::from(caster.level),
            i32::from(t.level),
            level,
            cfg,
        );
        if !gates.anti_magic_pass || !in_range {
            return out;
        }
        // `:1239` 第一条 RM_DELAYMAGIC（**先于** `:1240` 的等级门）。
        self.send_self_delay_msg_full(
            caster_id,
            crate::slave::RM_DELAYMAGIC,
            power / 3,
            mir2_shared::hutil::make_long_unsigned(target_x as u16, target_y as u16),
            2,
            tid,
            600,
        );
        if !gates.level_gate_pass || !gates.rand_gate_pass || !gates.success_gate_pass {
            return out;
        }
        // `:1247-1251`：玩家目标 ⇒ `SetPkFlag`（⇒ 656，由 target 发）+ `SetTargetCreat`。
        if t.race == crate::entity::ACTOR_RACE_PLAY {
            out.name_color = self.set_pk_flag(caster_id, tid);
            if let Some(c) = self.entities.get_mut(caster_id) {
                c.ai.target_creature = Some(tid);
            }
        }
        // `:1252 targetObject.SetLastHiter(playObject)`
        let now = self.clock_ms;
        if let Some(tm) = self.entities.get_mut(tid) {
            tm.set_last_hiter(caster_id, now);
        }
        // `:1253 nPower = targetObject.GetMagStruckDamage(playObject, nPower)`（第 9 抽）。
        let n_power = crate::combat::get_mag_struck_damage(
            &mut self.rng,
            t.mac,
            power,
            Some(caster.undead_power),
        );
        out.msd_power = n_power;
        // `:1254` 第二条 RM_DELAYMAGIC。
        self.send_self_delay_msg_full(
            caster_id,
            crate::slave::RM_DELAYMAGIC,
            n_power,
            mir2_shared::hutil::make_long_unsigned(target_x as u16, target_y as u16),
            2,
            tid,
            600,
        );
        // `:1255-1258`：玩家且 `!UnParalysis` ⇒ `R(nLevel)` 后投 STONE（650ms）。
        if t.race == crate::entity::ACTOR_RACE_PLAY && !self.target_un_paralysis(tid) {
            let time = n_power / cfg.hit_mabe_time_rate + self.rng.random_below(i32::from(level));
            self.send_self_delay_msg_full(
                tid,
                crate::slave::RM_POISON,
                5, // PoisonState.STONE
                time,
                caster_id,
                i32::from(level),
                650,
            );
        }
        out.bo_train = true;
        out
    }

    /// `SKILL_GROUPLIGHTENING(42)` 效果体 —— `MagGroupLightening`（`MagicManager.cs:981-1018`）。
    ///
    /// **调用前提**：`targetObject != null`（helper 在 `:987` 直接读 `targetObject.ActorId`；
    /// null ⇒ NPE ⇒ 639 失败臂，由宿主处理，不调用本函数）。
    ///
    /// 逐目标（`x→y→格内 ObjList 插入序`）：
    /// ```text
    /// :995  IsProperTarget（纯门；失败 ⇒ 零抽）
    /// :997  Random(10) >= AntiMagic                   ← 1 抽
    /// :999  nPower = AP(GP(MPow)+Lo(MC), ΔMC+1)        ← 4 抽
    /// :1000 LA_UNDEAD ⇒ Round(*1.5)
    /// :1004 RM_DELAYMAGIC(nPower, MakeLong(obj.CurrX,obj.CurrY), 2, obj.ActorId, 600)
    /// :1005 Race >= Animal ⇒ result = true
    /// :1010 obj.CurrX/Y != nTargetX/Y ⇒ RM_10205（716，**骰门之外**）
    /// ```
    /// `boSpellFire = false` 由 `case` 的 `ref` 置（`:984`），宿主据返回值不再发共享尾 638。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_group_lightening_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target_id: i32,
        target_x: i32,
        target_y: i32,
        mc: i32,
    ) -> GroupLighteningOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return GroupLighteningOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        let range = i32::from(level).max(1);
        let mut out = GroupLighteningOutcome {
            fire_target: target_id,
            ..GroupLighteningOutcome::default()
        };
        for tid in self.mag_objects_in_range(caster.map_id, target_x, target_y, range) {
            if tid == caster_id {
                continue;
            }
            let Some(t) = self.entities.get(tid).cloned() else {
                continue;
            };
            if t.death || t.ghost {
                continue;
            }
            if !crate::combat::is_proper_target_grouped(
                &caster,
                &t,
                &self.attacker_group_members(&caster),
            ) {
                continue;
            }
            // `:997 Random(10) >= AntiMagic`
            if crate::magic_effect::anti_magic_roll(&mut self.rng, i32::from(t.anti_magic)) {
                // `:999`
                let mut power =
                    crate::magic::power_get_power_plus_mc(&mut self.rng, magic, level, mc, atk);
                // `:1000-1003`
                if t.life_attrib == crate::magic_effect::LA_UNDEAD {
                    power = mir2_shared::hutil::round(f64::from(power) * 1.5);
                }
                // `:1004`
                self.send_self_delay_msg_full(
                    caster_id,
                    crate::slave::RM_DELAYMAGIC,
                    power,
                    mir2_shared::hutil::make_long_unsigned(t.x as u16, t.y as u16),
                    2,
                    tid,
                    600,
                );
                // `:1005-1008 if (baseObject.Race >= ActorRace.Animal) result = true;`
                if t.race >= crate::entity::ACTOR_RACE_ANIMAL {
                    out.bo_train = true;
                }
                out.targets.push(GroupLighteningTarget {
                    target_id: tid,
                    power,
                });
            }
            // `:1010-1013`（骰门之外）。
            if t.x != target_x || t.y != target_y {
                out.icons_716.push((t.x, t.y));
            }
        }
        out
    }

    /// 取 `(施法者, 目标, 目标 id)`（均 clone ⇒ 后续可安全 `&mut self.rng`）。
    /// `target` 为 `None` / 实体不存在 ⇒ `None`（oracle 的 `targetObject == null`）。
    fn spell_strike_targets(
        &self,
        caster_id: i32,
        target: Option<i32>,
    ) -> Option<(Entity, Entity, i32)> {
        let caster = self.entities.get(caster_id)?.clone();
        let tid = target?;
        let t = self.entities.get(tid)?.clone();
        Some((caster, t, tid))
    }

    /// `BaseObject.MagCanHitTarget` 的世界层调用（图 = 施法者当前图；目标坐标 = 实体坐标）。
    fn spell_can_hit(&self, caster: &Entity, target: &Entity) -> bool {
        crate::magic::mag_can_hit_target(
            self.map(caster.map_id),
            caster.x,
            caster.y,
            target.x,
            target.y,
        )
    }

    // ====================== P3-B2（治疗/爆炸 AoE：2/29/23/24/33/47）======================

    /// `BaseObject.GetMapBaseObjects`（`BaseObject.cs:2808-2844`）的**有序**等价物（合同 §1.4）。
    ///
    /// ```text
    /// :2817  for (x = nX-range; x <= nX+range; x++)           外层升序
    /// :2819    for (y = nY-range; y <= nY+range; y++)         内层升序
    /// :2821      cellInfo = GetCellInfo(x, y, out cellSuccess)  只在 Valid(Walk) 时 success
    /// :2822      if (cellSuccess && cellInfo.IsAvailable)
    /// :2824        for (i = 0; i < cellInfo.ObjList.Count; i++) 格内 **ObjList 插入序**
    /// :2827          if (cellObject.CellObjId > 0 && cellObject.ActorObject)
    /// :2830            if (baseObject != null && !baseObject.Death && !baseObject.Ghost)
    /// ```
    ///
    /// **不得**用 `EntityStore::iter` 的槽位序（槽复用会换位）。
    #[must_use]
    pub fn mag_objects_in_range(&self, map_id: usize, n_x: i32, n_y: i32, range: i32) -> Vec<i32> {
        let Some(map) = self.maps.get(map_id) else {
            return Vec::new();
        };
        let mut out = Vec::new();
        for x in (n_x - range)..=(n_x + range) {
            for y in (n_y - range)..=(n_y + range) {
                let (cell, success) = map.get_cell_info(x, y);
                if !success || !cell.is_available() {
                    continue;
                }
                for obj in &cell.obj_list {
                    if obj.cell_obj_id <= 0 || !obj.actor_object {
                        continue;
                    }
                    if let Some(e) = self.entities.get(obj.cell_obj_id) {
                        if !e.death && !e.ghost {
                            out.push(e.id);
                        }
                    }
                }
            }
        }
        out
    }

    /// `SKILL_HEALLING(2)` 效果体（`MagicManager.cs:204-224`）。
    ///
    /// ```text
    /// :205-210  target == null => target = self, nTargetX/nTargetY = CurrX/CurrY
    /// :211      if (IsProperFriend(target)) {                       <- 失败 => 0 抽 / 0 case 帧
    /// :213        nPower = GetAttackPower(GetPower(MPow) + Lo(SC)*2, (Hi-Lo)*2+1)  <- 4 抽
    /// :214        if (target.HP < target.MaxHP) {
    /// :216          target.SendSelfDelayMsg(RM_MAGHEALING, 0, nPower, 0,0,"",800)
    /// :217          boTrain = true }
    /// :219-222    if (AbilSeeHealGauge) playObject.SendMsg(target, RM_ABILSEEHEALGAUGE, ...)
    /// ```
    ///
    /// 宿主负责 `AbilSeeHealGauge` 门与 1103 帧（本函数只返回 gauge 目标）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_healing_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        target_x: i32,
        target_y: i32,
        sc: i32,
    ) -> HealingSpellOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return HealingSpellOutcome::default();
        };
        // `:160-163` 死亡归一在宿主已完成；这里再兜一次（直调用例）。
        let target = target.filter(|id| self.entities.get(*id).is_some_and(|e| !e.death));
        let (tid, tx, ty) = match target {
            Some(id) => (id, target_x, target_y),
            // `:205-210` 目标为空 => 治疗自己并**改写尾帧坐标**。
            None => (caster.id, caster.x, caster.y),
        };
        let Some(t) = self.entities.get(tid).cloned() else {
            return HealingSpellOutcome::default();
        };
        let mut out = HealingSpellOutcome {
            target: Some(tid),
            tx,
            ty,
            ..HealingSpellOutcome::default()
        };
        let in_group = self.attacker_group_members(&caster).contains(&tid);
        // `:211` —— 门在**抽数之前**；失败 => `allowed=false`、`target` 保留（尾仍带该 id）。
        if !crate::combat::is_proper_friend(&caster, &t, in_group) {
            return out;
        }
        out.allowed = true;
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::power_healing(&mut self.rng, magic, level, sc, atk);
        out.power = power;
        if t.hp < t.max_hp {
            self.send_self_delay_msg_with(tid, crate::slave::RM_MAGHEALING, 0, power, 800);
            out.bo_train = true;
            out.healed.push(tid);
        }
        // `:219-222`：对**每个** proper friend 都发（不看 HP）。
        out.gauges.push(tid);
        out
    }

    /// `SKILL_BIGHEALLING(29)` 效果体 + `MagBigHealing`（`MagicManager.cs:443-449` / `:54-76`）。
    ///
    /// 施法者侧恒 4 抽（**无前置门**）；helper 内 0 抽。枚举 = `GetMapBaseObjects(..., 1, ...)`
    /// 的 3x3 有序集；只治 `IsProperFriend && HP<MaxHP` 的玩家；gauge 对所有 proper friend。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_big_healing_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        n_x: i32,
        n_y: i32,
        sc: i32,
    ) -> HealingSpellOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return HealingSpellOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::power_healing(&mut self.rng, magic, level, sc, atk);
        let mut out = HealingSpellOutcome {
            target,
            tx: n_x,
            ty: n_y,
            power,
            ..HealingSpellOutcome::default()
        };
        let group_members = self.attacker_group_members(&caster);
        for tid in self.mag_objects_in_range(caster.map_id, n_x, n_y, 1) {
            let Some(t) = self.entities.get(tid).cloned() else {
                continue;
            };
            let in_group = group_members.contains(&tid);
            if !crate::combat::is_proper_friend(&caster, &t, in_group) {
                continue;
            }
            // `:69-72` gauge **不看 HP**。
            out.gauges.push(tid);
            if t.hp < t.max_hp {
                self.send_self_delay_msg_with(tid, crate::slave::RM_MAGHEALING, 0, power, 800);
                out.bo_train = true;
                out.healed.push(tid);
            }
        }
        out
    }

    /// `MagicManager.MagBigExplosion`（`MagicManager.cs:1100-1117`）——
    /// `SKILL_FIREBOOM(23)` / `SKILL_SNOWWIND(33)` / `SKILL_47(47)` 共用。
    ///
    /// ```text
    /// :1104  GetMapBaseObjects(envir, nX, nY, nRage, ...)          有序枚举（合同 §1.4）
    /// :1108  baseObject.IsProperTarget(targetObject)               纯门（不取数）
    /// :1110  baseObject.SetTargetCreat(targetObject)               只改**施法者**的 TargetCret
    /// :1111  targetObject.SendMsg(baseObject, RM_MAGSTRUCK, 0, nPower, 0, 0)
    /// ```
    ///
    /// 施法者侧 4 抽先于枚举（oracle 是实参求值序）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_big_explosion_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        n_x: i32,
        n_y: i32,
        range: i32,
        mc: i32,
    ) -> AreaSpellOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return AreaSpellOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::power_magic_attack(&mut self.rng, magic, level, mc, atk);
        let mut out = AreaSpellOutcome {
            target,
            power,
            ..AreaSpellOutcome::default()
        };
        let group_members = self.attacker_group_members(&caster);
        let mut last_target = None;
        for tid in self.mag_objects_in_range(caster.map_id, n_x, n_y, range) {
            let Some(t) = self.entities.get(tid).cloned() else {
                continue;
            };
            if !crate::combat::is_proper_target_grouped(&caster, &t, &group_members) {
                continue;
            }
            last_target = Some(tid);
            out.targets.push(tid);
            out.bo_train = true;
            let order = self.msg_order;
            self.msg_order += 1;
            self.pending_mag_struck.push(PendingMagStruck {
                target_id: tid,
                caster_id,
                power,
                mine: false,
                order,
            });
        }
        // `SetTargetCreat` 逐目标覆盖；末值 = 最后一个 proper target（oracle 同）。
        if let (Some(tid), Some(e)) = (last_target, self.entities.get_mut(caster_id)) {
            e.ai.target_creature = Some(tid);
            e.ai.target_focus_tick = self.clock_ms;
        }
        out
    }

    /// `MagicManager.MagElecBlizzard`（`MagicManager.cs:1119-1144`）——`SKILL_LIGHTFLOWER(24)`。
    ///
    /// ```text
    /// :1123  GetMapBaseObjects(envir, baseObject.CurrX, baseObject.CurrY, ElecBlizzardRange)
    ///        —— 中心是**施法者坐标**，不是点击坐标（合同 §5-C5）
    /// :1128  if (target.LifeAttrib != LA_UNDEAD) nPowerPoint = nPower / 10  （整除）
    /// :1134  else                               nPowerPoint = nPower
    /// :1136  if (baseObject.IsProperTarget(targetObject))
    /// :1138    targetObject.SendMsg(baseObject, RM_MAGSTRUCK, 0, nPowerPoint, 0, 0)
    /// ```
    ///
    /// `nPowerPoint` 在 `IsProperTarget` **之前**算（无抽数）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_elec_blizzard_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        range: i32,
        mc: i32,
    ) -> AreaSpellOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return AreaSpellOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        let power = crate::magic_effect::power_magic_attack(&mut self.rng, magic, level, mc, atk);
        let mut out = AreaSpellOutcome {
            target,
            power,
            ..AreaSpellOutcome::default()
        };
        let group_members = self.attacker_group_members(&caster);
        for tid in self.mag_objects_in_range(caster.map_id, caster.x, caster.y, range) {
            let Some(t) = self.entities.get(tid).cloned() else {
                continue;
            };
            let power_point = if t.life_attrib != crate::magic_effect::LA_UNDEAD {
                power / 10
            } else {
                power
            };
            if !crate::combat::is_proper_target_grouped(&caster, &t, &group_members) {
                continue;
            }
            out.targets.push(tid);
            out.bo_train = true;
            let order = self.msg_order;
            self.msg_order += 1;
            self.pending_mag_struck.push(PendingMagStruck {
                target_id: tid,
                caster_id,
                power: power_point,
                mine: false,
                order,
            });
        }
        out
    }

    /// **P3-B2**：`RM_MAGSTRUCK` 交付处理器（`BaseObject.Base.cs:618-686`）的**有序**批量版。
    ///
    /// 逐目标（入队序）：
    /// ```text
    /// :620-623  if (Race >= Animal && !RushMode && Level < 50) WalkTick += 800 + Random(1000)
    /// :624      nDamage = GetMagStruckDamage(null, nParam1)          <- MSD 1 抽（undead 项不生效）
    /// :625      if (nDamage > 0) {
    /// :627        StruckDamage(nDamage)                               玩家：STD（R(10)+每格 R(8)）
    /// :628        HealthSpellChanged()                               仅玩家出 53（宿主）
    /// :629        SendRefMsg(RM_STRUCK_MAG, ...)                     31（宿主）
    /// :672-679    if (Race > Play) { MeatQuality -= nDamage*1000; SendMsg(RM_STRUCK) }
    ///             -- AnimalObject.Operate：SetLastHiter -> Struck（TargetCret!=null 且
    ///                GetAttackDir 失败抽 R(6)）-> Animal 抽 R(300) -> BreakHolySeizeMode（656）
    /// ```
    ///
    /// `RushMode` 本骨架无字段 => 恒 false（合同 §3.1 的夹具 pin；登记）。
    /// `MonDelHptoExp`/`MonSayMsg`/`Disable*Struck` 缺省 false（合同 §3.1），不建模其支。
    pub fn deliver_mag_struck(&mut self) -> Vec<MagStruckOutcome> {
        let mut msgs = std::mem::take(&mut self.pending_mag_struck);
        // 宿主排水序 = 消息创建序（见 `PendingMagStruck::order`）；稳定排序。
        msgs.sort_by_key(|m| m.order);
        let mut out = Vec::new();
        for msg in msgs {
            let Some(mut t) = self.entities.get(msg.target_id).cloned() else {
                continue;
            };
            // `CloneMonster.Operate`（`Clone.cs:14-30`）吞掉 `RM_MAGSTRUCK`（以及
            // `RM_MAGSTRUCK_MINE` 等一切消息）⇒ **不**跑 `BaseObject.Base.cs:618-686`
            // 的 `R(1000)`/MSD/`StruckDamage` 段；只做 `Master.MP -= (ushort)wParam`。
            // 已知的所有发送点 `wParam` 都是 0（`MagicManager.cs:1111/1138`、
            // `BaseObject.Base.cs:726`）⇒ 数值上 MP 不变，但 `MP <= 0 ⇒ HP = 0` 仍生效。
            if t.race == crate::special_monster::RACE_CLONE_MONSTER {
                let ident = if msg.mine {
                    crate::special_monster::RM_MAGSTRUCK_MINE
                } else {
                    crate::special_monster::RM_MAGSTRUCK
                };
                let clone_id = msg.target_id;
                crate::special_monster::clone_operate(self, clone_id, ident, 0, 0);
                let (hp_after, max_hp) = self
                    .entities
                    .get(clone_id)
                    .map_or((t.hp, t.max_hp), |e| (e.hp, e.max_hp));
                out.push(MagStruckOutcome {
                    target_id: clone_id,
                    caster_id: msg.caster_id,
                    damage: 0,
                    hp_after,
                    max_hp,
                    is_player: false,
                    name_color: false,
                    chr_color: 0,
                });
                continue;
            }
            let is_monster = t.race >= crate::entity::ACTOR_RACE_ANIMAL;
            // `:620-623` —— 只对怪物的 `RM_MAGSTRUCK`（**不是** `RM_MAGSTRUCK_MINE`）取
            // `Random(1000)`（与伤害无关）；`!RushMode` 是 P3-B5 补上的第三道门。
            if !msg.mine && is_monster && !t.rush_mode && t.level < 50 {
                let draw = crate::magic_effect::mag_struck_walk_draw(&mut self.rng);
                t.ai.walk_tick += 800 + i64::from(draw);
            }
            // `:624` — `GetMagStruckDamage(null, nPower)`：`baseObject == null` => 无 undead 加项。
            let damage =
                crate::combat::get_mag_struck_damage(&mut self.rng, t.mac, msg.power, None);
            let mut name_color = false;
            let mut chr_color = 0u8;
            if damage > 0 {
                // `base.StruckDamage`（`:827-851`）：本夹具配置倍率均缺省 10 => 数值不变；
                // `LastHiter` 缩放面未在本层复刻（合同 §3.1 pin `LastHiter=null`，登记）。
                t.hp = if damage >= i32::from(t.hp) {
                    0
                } else {
                    t.hp - u16::try_from(damage).unwrap_or(u16::MAX)
                };
                if is_monster {
                    // `:672-679` `SendMsg(RM_STRUCK)` -> `AnimalObject.Operate`（`AnimalObject.cs:118-133`）。
                    t.set_last_hiter(msg.caster_id, self.clock_ms);
                    // `Struck`（`AnimalObject.cs:336-358`）。
                    let mut set_target = false;
                    match t.ai.target_creature {
                        None => set_target = true, // 短路：不抽 `Random(6)`
                        Some(tc) => {
                            let dir_ok = self.entities.get(tc).is_some_and(|o| {
                                (t.x - o.x).abs() <= 1
                                    && (t.y - o.y).abs() <= 1
                                    && !(t.x == o.x && t.y == o.y)
                            });
                            if dir_ok
                                || crate::magic_effect::mag_struck_attack_dir_roll(&mut self.rng)
                            {
                                set_target = true;
                            }
                        }
                    }
                    if set_target {
                        if let Some(c) = self.entities.get(msg.caster_id) {
                            if crate::combat::is_proper_target(&t, c) {
                                t.ai.target_creature = Some(msg.caster_id);
                                t.ai.target_focus_tick = self.clock_ms;
                            }
                        }
                    }
                    if t.ai.animal {
                        let draw = crate::magic_effect::mag_struck_meat_roll(&mut self.rng);
                        t.ai.meat_quality = (i32::from(t.ai.meat_quality) - draw).max(0) as u16;
                    }
                    // `:672` `MeatQuality -= nDamage * 1000`（无抽数，值面）。
                    t.ai.meat_quality =
                        (i32::from(t.ai.meat_quality) - damage * 1000).max(0) as u16;
                    // `BreakHolySeizeMode`（`:125`）-> `RefNameColor`（`BaseObject.cs:730-733`）。
                    name_color = !t.ob_mode && !t.fixed_hide_mode;
                    chr_color = crate::slave::get_chr_color(&t, &[], 0);
                } else {
                    // `PlayObject.StruckDamage` 的 RNG 段（`PlayObject.cs:2957/3000`）。
                    crate::combat::struck_damage_durability(&mut self.rng, &mut t);
                }
            }
            let (hp_after, max_hp) = (t.hp, t.max_hp);
            let is_player = t.race == crate::entity::ACTOR_RACE_PLAY;
            if let Some(e) = self.entities.get_mut(msg.target_id) {
                *e = t;
            }
            out.push(MagStruckOutcome {
                target_id: msg.target_id,
                caster_id: msg.caster_id,
                damage,
                hp_after,
                max_hp,
                is_player,
                name_color,
                chr_color,
            });
        }
        out
    }

    /// **片 A-5（buff 生产者接线）**：技能 14/15（`SKILL_HANGMAJINBUB`/`SKILL_DEJIWONHO`）的生产入口
    /// （`MagicManager.cs:352-366` 的 `SKILL_HANGMAJINBUB`/`SKILL_DEJIWONHO` 支）。
    ///
    /// `nPower = GetAttackPower(GetPower13(userMagic, 60) + Lo(SC)*10, ΔSC + 1)`（取数顺序：
    /// `GetPower13` → `GetAttackPower`；玩家 `GetAttackPower` override 若 `Luck <= 0` 多抽一次
    /// `Random(10)`）；随后 `MagMakeDefenceArea(nTargetX, nTargetY, 3, nPower, btState)`
    /// （`btState == 0` ⇒ `DefenceUp`；否则 ⇒ `MagDefenceUp`）。返回 `result`（供 `boTrain` 的 `> 0` 门）。
    /// **P3-B3**：16 `MagMakeHolyCurtain`（`MagicManager.cs:1146-1207`）。
    ///
    /// 取数次序：先求实参 `GetPower13(40)`（1 抽 `R(0)`）→ `GetRPow(SC)`（0/1 抽），**再**进
    /// helper。helper 内：
    /// 1. `Envir.CanWalk(tx,ty,true)` 失败 ⇒ `result=0`（零 helper 抽）；
    /// 2. `GetMapBaseObjects(..., 1, ...)` 为空 ⇒ 0；
    /// 3. 逐对象：`Race >= Animal`（false ⇒ 不抽、直接 else）⇒ **必抽 `R(4)`** ⇒
    ///    `R(4)+caster.Level-1 > obj.Level` 且 `obj.Master == null` ⇒
    ///    `SendMsg(RM_MAKEHOLYSEIZEMODE, nPower*1000)`、`result++`；否则 `result = 0`。
    ///    ⇒ 最终 `result` = **最后一段连续合格对象**的数量（已发帧不回滚）。
    #[must_use]
    pub fn cast_holy_curtain_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        sc: i32,
        tx: i32,
        ty: i32,
    ) -> HolyCurtainOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return HolyCurtainOutcome::default();
        };
        // C# 实参求值先于调用：`GetPower13(userMagic, 40) + GetRPow(WAbil.SC) * 3`。
        let power = crate::magic_effect::get_power13(&mut self.rng, magic, level, 40)
            + i32::from(crate::magic_effect::get_rpow(&mut self.rng, sc)) * 3;
        let mut out = HolyCurtainOutcome {
            tx,
            ty,
            power,
            ..HolyCurtainOutcome::default()
        };
        // `baseObject.Envir.CanWalk(nX, nY, true)`（`Envirnoment.cs:442-451`）：
        // `cellSuccess && cellInfo.Valid`；`get_cell_info` 的 `success` 已同时含 Valid。
        let can_walk = self
            .maps
            .get(caster.map_id)
            .is_some_and(|m| m.get_cell_info(tx, ty).1);
        out.can_walk = can_walk;
        if !can_walk {
            return out;
        }
        let objects = self.mag_objects_in_range(caster.map_id, tx, ty, 1);
        if objects.is_empty() {
            return out;
        }
        let caster_level = i32::from(caster.level);
        let mut result = 0i32;
        for id in objects {
            let Some(obj) = self.entities.get(id).cloned() else {
                continue;
            };
            if obj.race >= crate::entity::ACTOR_RACE_ANIMAL {
                // C# `&&` 从左到右：Race 过了就**必抽** `Random(4)`（即使 Master 非空）。
                let draw = crate::magic_effect::holy_curtain_draw(&mut self.rng);
                if draw + (caster_level - 1) > i32::from(obj.level) && obj.master.is_none() {
                    self.send_msg_now(
                        id,
                        crate::slave::RM_MAKEHOLYSEIZEMODE,
                        power * 1000,
                        0,
                        0,
                        0,
                    );
                    out.eligible.push(id);
                    result += 1;
                } else {
                    result = 0;
                }
            } else {
                result = 0;
            }
        }
        out.result = result;
        out
    }

    /// **P3-B3**：`MagMakePrivateTransparent`（`MagicManager.cs:673-699`）的**投递时**执行体。
    ///
    /// 18 在施法 tick 直接调用；19 在 `RM_TRANSPARENT` 的 800ms 到点投递时调用（重读
    /// `StatusTimeArr[8]`）。helper 内零前置抽；只有「`Race>=Animal && TargetCret==目标` 且
    /// `|dx|<=1 && |dy|<=1`」的对象各抽一次 `R(2)`（坐标不合格 ⇒ 短路不抽）。
    pub fn mag_make_private_transparent(
        &mut self,
        target_id: i32,
        n_h_time: u16,
    ) -> crate::slave::TransparentDelivery {
        let mut d = crate::slave::TransparentDelivery {
            target_id,
            n_h_time,
            ..crate::slave::TransparentDelivery::default()
        };
        let Some(base) = self.entities.get(target_id).cloned() else {
            return d;
        };
        if base.status_time[crate::entity::POISON_STATETRANSPARENT] > 0 {
            return d;
        }
        let objects = self.mag_objects_in_range(base.map_id, base.x, base.y, 9);
        for id in objects {
            let Some(obj) = self.entities.get(id).cloned() else {
                continue;
            };
            if obj.race >= crate::entity::ACTOR_RACE_ANIMAL
                && obj.ai.target_creature == Some(target_id)
            {
                let dx = (obj.x - base.x).abs();
                let dy = (obj.y - base.y).abs();
                let clear = if dx > 1 || dy > 1 {
                    true
                } else {
                    crate::magic_effect::private_transparent_draw(&mut self.rng) == 0
                };
                if clear {
                    if let Some(e) = self.entities.get_mut(id) {
                        e.ai.target_creature = None;
                    }
                }
            }
        }
        let status = crate::buff::get_char_status(self, target_id);
        if let Some(e) = self.entities.get_mut(target_id) {
            e.status_time[crate::entity::POISON_STATETRANSPARENT] = n_h_time;
            e.char_status = status;
            e.hide_mode = true;
            e.transparent = true;
        }
        d.status_changed = true;
        d.status = status;
        d
    }

    /// **P3-B3**：18 `SKILL_CLOAK`（`MagicManager.cs:378-383`）。
    ///
    /// 实参 `(ushort)(GetPower13(30) + GetRPow(SC)*3)` **先求值**（即使 helper 因
    /// `StatusTimeArr[8] > 0` 早退，这两抽也已发生）。
    #[must_use]
    pub fn cast_private_transparent_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        sc: i32,
    ) -> PrivateTransparentOutcome {
        let n_h_time = (crate::magic_effect::get_power13(&mut self.rng, magic, level, 30)
            + i32::from(crate::magic_effect::get_rpow(&mut self.rng, sc)) * 3)
            as u16;
        let d = self.mag_make_private_transparent(caster_id, n_h_time);
        PrivateTransparentOutcome {
            status_changed: d.status_changed,
            status: d.status,
            status_time: n_h_time,
        }
    }

    /// **P3-B3**：19 `SKILL_BIGCLOAK`（`MagicManager.cs:384-389` + `:1209-1228`）。
    ///
    /// 实参先求值（`GetPower13(30) + GetRPow(SC)*3`，**int** 不 cast）；helper 内零 RNG：
    /// 3×3 有序枚举（中心 = snap 后的点击点），`IsProperFriend && StatusTimeArr[8] == 0`
    /// ⇒ `SendSelfDelayMsg(RM_TRANSPARENT, 0, nHTime, 0, 0, "", 800)`。
    #[must_use]
    pub fn cast_group_transparent_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        sc: i32,
        tx: i32,
        ty: i32,
    ) -> GroupTransparentOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return GroupTransparentOutcome::default();
        };
        let n_h_time = crate::magic_effect::get_power13(&mut self.rng, magic, level, 30)
            + i32::from(crate::magic_effect::get_rpow(&mut self.rng, sc)) * 3;
        let mut out = GroupTransparentOutcome {
            n_h_time,
            ..GroupTransparentOutcome::default()
        };
        for id in self.mag_objects_in_range(caster.map_id, tx, ty, 1) {
            let Some(obj) = self.entities.get(id).cloned() else {
                continue;
            };
            let in_group = self.attacker_group_members(&caster).contains(&id);
            if crate::combat::is_proper_friend(&caster, &obj, in_group)
                && obj.status_time[crate::entity::POISON_STATETRANSPARENT] == 0
            {
                self.send_self_delay_msg_full(
                    id,
                    crate::slave::RM_TRANSPARENT,
                    0,
                    n_h_time,
                    0,
                    0,
                    800,
                );
                out.friends.push(id);
                out.result = true;
            }
        }
        out
    }

    /// **P3-B3**：28 `SKILL_SHOWHP`（`MagicManager.cs:431-442`）。
    ///
    /// 门：`target != null && !ShowHp`（false ⇒ **零抽**）。`R(6) <= Level+3` 过门后：
    /// `ShowHpTick/ShowHpInterval` 落目标、`SendSelfDelayMsg(RM_DOOPENHEALTH, …, 1500)`、
    /// `boTrain = true`。取数顺序：`R(6)` → `GetRPow(SC)` → `GetPower13(RP*2+30)`。
    #[must_use]
    pub fn cast_show_hp_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        sc: i32,
        target: Option<i32>,
    ) -> ShowHpOutcome {
        let Some(tid) = target else {
            return ShowHpOutcome::default();
        };
        let Some(t) = self.entities.get(tid).cloned() else {
            return ShowHpOutcome::default();
        };
        let _ = caster_id;
        if t.show_hp {
            return ShowHpOutcome::default();
        }
        let mut out = ShowHpOutcome {
            gate_passed: true,
            ..ShowHpOutcome::default()
        };
        let roll = crate::magic_effect::show_hp_gate_draw(&mut self.rng);
        if roll > i32::from(level) + 3 {
            return out;
        }
        out.roll_passed = true;
        out.bo_train = true;
        let interval = crate::magic::power_show_hp_interval(&mut self.rng, magic, level, sc);
        if let Some(e) = self.entities.get_mut(tid) {
            e.show_hp_tick = self.clock_ms;
            e.show_hp_interval = interval;
        }
        // 立即把 1500ms 自投递排到**目标自己**的队列。
        self.send_self_delay_msg_full(tid, crate::slave::RM_DOOPENHEALTH, 0, 0, 0, 0, 1500);
        out
    }

    /// **P3-B3**：`MakeOpenHealth`（`BaseObject.cs:944-950`）——`RM_DOOPENHEALTH` 的到点投递。
    pub fn make_open_health(&mut self, target_id: i32) -> crate::slave::OpenHealthDelivery {
        let Some(e) = self.entities.get(target_id).cloned() else {
            return crate::slave::OpenHealthDelivery::default();
        };
        let status_ex = e.char_status_ex | crate::entity::POISON_OPENHEATH;
        let mut d = crate::slave::OpenHealthDelivery {
            target_id,
            hp: e.hp,
            max_hp: e.max_hp,
        };
        let status =
            crate::buff::get_char_status(self, target_id) | crate::entity::POISON_OPENHEATH;
        if let Some(e) = self.entities.get_mut(target_id) {
            e.show_hp = true;
            e.char_status_ex = status_ex;
            e.char_status = status;
        }
        d.hp = self.entities.get(target_id).map_or(d.hp, |e| e.hp);
        d.max_hp = self.entities.get(target_id).map_or(d.max_hp, |e| e.max_hp);
        d
    }

    /// **P3-B3**：`OpenHolySeizeMode(dwInterval)`（`AnimalObject.cs:322-328`）+ `RefNameColor`。
    ///
    /// **656 的颜色不是 `MonsterObject.GetChrColor` 的 `HolySeize ⇒ 0x7D` 覆写**：
    /// `RM_CHANGENAMECOLOR` 由**接收者**处理（`PlayObject.Message.cs:1677-1679`）——
    /// `PlayObject.GetChrColor(baseObject)`（`PlayObject.cs:3585-3656`）走
    /// `BaseObject.GetChrColor`（`BaseObject.Base.cs:869-883/904-907`）⇒ 非 NPC/非宝宝的
    /// 怪取 `GetNameColor()`，即构造置 255 的 `BaseObject.cs:385 name_color`。
    /// `MonsterObject.GetChrColor`（`MonsterObject.cs:482-492`）的 HolySeize 分支只在怪物
    /// **自己**计算颜色时可达，本处理器（接收者是玩家）不可达。
    pub fn open_holy_seize_mode(
        &mut self,
        target_id: i32,
        interval: i32,
    ) -> crate::slave::NameColorDelivery {
        let now = self.clock_ms;
        let Some(color) = self
            .entities
            .get(target_id)
            .map(|e| crate::slave::get_chr_color(e, &[], 0))
        else {
            return crate::slave::NameColorDelivery::default();
        };
        if let Some(e) = self.entities.get_mut(target_id) {
            e.ai.holy_seize = true;
            e.ai.holy_seize_tick = now;
            e.ai.holy_seize_interval = i64::from(interval);
        }
        crate::slave::NameColorDelivery { target_id, color }
    }

    /// **P3-B3**：32 `MagTurnUndead`（`MagicManager.cs:827-856`）。
    ///
    /// `IsProperTarget` 由宿主在死亡归一后判（false ⇒ 零抽）；本函数从
    /// `SuperMan || LifeAttrib != LA_UNDEAD` 开始。目标为玩家（`Race == Play`）时 oracle 的
    /// `(AnimalObject)target` 抛 `InvalidCastException` ⇒ [outcome.invalid_cast]（**无尾**）。
    #[must_use]
    pub fn cast_turn_undead_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        cfg_turn_undead_level: i32,
    ) -> TurnUndeadSpellOutcome {
        let _ = magic;
        let Some(tid) = target else {
            return TurnUndeadSpellOutcome::default();
        };
        let (Some(caster), Some(t)) = (
            self.entities.get(caster_id).cloned(),
            self.entities.get(tid).cloned(),
        ) else {
            return TurnUndeadSpellOutcome::default();
        };
        // `:830-833` `target.SuperMan || target.LifeAttrib != LA_UNDEAD` ⇒ false（零抽）。
        if t.super_man || t.life_attrib != crate::magic_effect::LA_UNDEAD {
            return TurnUndeadSpellOutcome::default();
        }
        // `:834` `((AnimalObject)targetObject).Struck(playObject)` —— 玩家不是 AnimalObject。
        if t.race == crate::entity::ACTOR_RACE_PLAY {
            return TurnUndeadSpellOutcome {
                invalid_cast: true,
                ..TurnUndeadSpellOutcome::default()
            };
        }
        let target_creature_present = t.ai.target_creature.is_some();
        let attack_dir_ok =
            t.ai.target_creature
                .is_some_and(|tc| crate::monai::get_attack_dir(self, tid, tc).is_some());
        let caster_is_proper_target = crate::combat::is_proper_target(&t, &caster);
        let inp = crate::magic_effect::TurnUndeadInput {
            super_man: t.super_man,
            life_attrib: t.life_attrib,
            animal: t.ai.animal,
            caster_level: i32::from(caster.level),
            target_level: i32::from(t.level),
            magic_level: level,
            mag_turn_undead_level: cfg_turn_undead_level,
            target_creature_present,
            attack_dir_ok,
            caster_is_proper_target,
            target_is_animal_object: true,
        };
        let out = crate::magic_effect::mag_turn_undead(&mut self.rng, &inp);
        let now = self.clock_ms;
        if let Some(e) = self.entities.get_mut(tid) {
            e.ai.struck_tick = now;
            if out.target_targeted_by_caster {
                e.ai.target_creature = Some(caster_id);
                e.ai.target_focus_tick = now;
            }
            if let Some(draw) = out.meat_draw {
                // BUG-REPLICA（合同 §3.4 B3-32-3）：`MeatQuality` 是 `ushort`，赋值先截断，
                // oracle 的 `if (MeatQuality < 0)` 恒 false ⇒ **按 16 位回绕**，不钳 0。
                e.ai.meat_quality =
                    e.ai.meat_quality
                        .wrapping_sub(u16::try_from(draw).unwrap_or(0));
            }
            e.ai.attack_tick += i64::from(out.attack_tick_delta);
        }
        if out.target_run_away {
            if let Some(e) = self.entities.get_mut(tid) {
                e.ai.run_away_mode = true;
                e.ai.run_away_start = now;
                e.ai.run_away_time = 10 * 1000;
            }
        }
        if out.result {
            // `targetObject.SetLastHiter(playObject); targetObject.WAbil.HP = 0;`
            if let Some(e) = self.entities.get_mut(tid) {
                e.set_last_hiter(caster_id, now);
                e.hp = 0;
            }
        }
        // `MagicManager.cs:841` `playObject.SetTargetCreat(targetObject)`（**无条件**，Struck 之后）：
        // 写**施法者**的 `TargetCret + TargetFocusTick`（`BaseObject.Base.cs:557-561`）。
        // 与 [`World::cast_big_explosion_spell`] 的「逐目标覆盖、末值胜」同口径（本处仅一个目标）。
        if out.caster_target_is_target {
            if let Some(e) = self.entities.get_mut(caster_id) {
                e.ai.target_creature = Some(tid);
                e.ai.target_focus_tick = now;
            }
        }
        TurnUndeadSpellOutcome {
            invalid_cast: false,
            bo_train: out.result,
            hp_zeroed: out.target_hp_zeroed,
        }
    }

    /// **P3-B3**：40 `SKILL_UNAMYOUNSUL`（常量名解毒术；部署行「十方斩」= `MagicManager.cs:495-523`）。
    ///
    /// `IsProperFriend` 失败 ⇒ 零抽。`R(7) - (Level+1) < 0` 过门后：三个计时器
    /// `DECHEALTH/DAMAGEARMOR/STONE` 各自 `!= 0 ⇒ 置 1` 并 `boTrain = true`（**不是清 0**）。
    #[must_use]
    pub fn cast_unamyounsul_spell(
        &mut self,
        caster_id: i32,
        level: u8,
        target: Option<i32>,
    ) -> UnamyounsulOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return UnamyounsulOutcome::default();
        };
        let tid = target.unwrap_or(caster_id);
        let Some(t) = self.entities.get(tid).cloned() else {
            return UnamyounsulOutcome::default();
        };
        let in_group = self.attacker_group_members(&caster).contains(&tid);
        let mut out = UnamyounsulOutcome::default();
        if !crate::combat::is_proper_friend(&caster, &t, in_group) {
            return out;
        }
        out.allowed = true;
        let roll = crate::magic_effect::unamyounsul_gate_draw(&mut self.rng);
        if roll - (i32::from(level) + 1) >= 0 {
            return out;
        }
        out.roll_passed = true;
        if let Some(e) = self.entities.get_mut(tid) {
            if e.status_time[crate::entity::POISON_DECHEALTH] != 0 {
                e.status_time[crate::entity::POISON_DECHEALTH] = 1;
                out.bo_train = true;
            }
            if e.status_time[crate::entity::POISON_DAMAGEARMOR] != 0 {
                e.status_time[crate::entity::POISON_DAMAGEARMOR] = 1;
                out.bo_train = true;
            }
            if e.status_time[crate::entity::POISON_STONE] != 0 {
                e.status_time[crate::entity::POISON_STONE] = 1;
                out.bo_train = true;
            }
        }

        out
    }
    #[allow(clippy::too_many_arguments)]
    #[must_use]
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

    // ======================================================================================
    // **P3-B5**（魔号 9 / 10 / 22 / 36）
    // ======================================================================================

    /// `Envirnoment.GetMovingObject(nX, nY, boFlag)`（`Envirnoment.cs:1203-1240`）逐行移植。
    ///
    /// 按格子 `ObjList` 的**插入序**找第一个 `CellObjId > 0 && ActorObject`、实体存在、
    /// `!Ghost`、且（`boFlag=true` 时）`!Death` 的对象。**不得**用实体表/`HashMap` 序。
    #[must_use]
    pub fn moving_object_at(
        &self,
        map_id: usize,
        x: i32,
        y: i32,
        exclude_dead: bool,
    ) -> Option<i32> {
        let map = self.maps.get(map_id)?;
        let (cell, success) = map.get_cell_info(x, y);
        if !success || !cell.is_available() {
            return None;
        }
        for o in &cell.obj_list {
            if o.cell_obj_id > 0 && o.actor_object {
                if let Some(e) = self.entities.get(o.cell_obj_id) {
                    if !e.ghost && (!exclude_dead || !e.death) {
                        return Some(o.cell_obj_id);
                    }
                }
            }
        }
        None
    }

    /// `Envirnoment.GetBaseObjects(nX, nY, boFlag=true, ref list)`（`Envirnoment.cs:1311-1342`）。
    ///
    /// `boFlag=true` 时判据按 C# 的 `(!Ghost && !boFlag) || !Death` 化简为 **`!Death`**
    /// （存活但 `Ghost` 的对象仍会被收集——oracle 的运算符优先级怪行为，照搬）。
    pub(crate) fn base_objects_at(&self, map_id: usize, x: i32, y: i32) -> Vec<i32> {
        let Some(map) = self.maps.get(map_id) else {
            return Vec::new();
        };
        let (cell, success) = map.get_cell_info(x, y);
        if !success || !cell.is_available() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for o in &cell.obj_list {
            if o.cell_obj_id > 0 && o.actor_object {
                if let Some(e) = self.entities.get(o.cell_obj_id) {
                    if !e.death {
                        out.push(o.cell_obj_id);
                    }
                }
            }
        }
        out
    }

    /// `Envirnoment.GetBaseObjects(nX, nY, boFlag=false, ref list)`（`Envirnoment.cs:1311-1342`）。
    ///
    /// `boFlag=false` 的 oracle 判据是 `(!Ghost && !boFlag) || !Death` = **`!Ghost || !Death`**
    /// （非城堡门支；`base_objects_at` 是 `boFlag=true` = `!Death` 口径）。差别只有一类：
    /// **已死但尚未 `Ghost`** 的同格对象——`HitMagAttackTarget`（`AnimalObject.cs:365`）用本口径。
    ///
    /// 登记（MB-3b F4）：本口径只修「枚举面」；命中判定仍走 [`crate::combat::is_proper_target`]，
    /// 该函数顶部折入 `death/ghost/invisible`（既有缺口 **G09**，见
    /// `tests/parity/m3/audit-attack-search-coverage-20261018.md`）⇒ 死对象此处仍会被拒绝，
    /// 与 oracle `IsAttackTarget`（`BaseObject.Base.cs:377-466`，不判 `Death/Ghost`）的**剩余**
    /// 差异归属 G09，不在本卡扩大修复范围。
    pub(crate) fn base_objects_at_including_dead(&self, map_id: usize, x: i32, y: i32) -> Vec<i32> {
        let Some(map) = self.maps.get(map_id) else {
            return Vec::new();
        };
        let (cell, success) = map.get_cell_info(x, y);
        if !success || !cell.is_available() {
            return Vec::new();
        }
        let mut out = Vec::new();
        for o in &cell.obj_list {
            if o.cell_obj_id > 0 && o.actor_object {
                if let Some(e) = self.entities.get(o.cell_obj_id) {
                    // `!Ghost || !Death`（oracle 的 `(!Ghost && !boFlag) || !Death`，boFlag=false）。
                    if !e.ghost || !e.death {
                        out.push(o.cell_obj_id);
                    }
                }
            }
        }
        out
    }

    /// `Envirnoment.GetEvent(nX, nY)`（`:1344-1360`）的格子 + 管理器双层门。
    ///
    /// **BUG-REPLICA 1**：`MapEvent.Id` 恒 0，`CellObjectMgr.Add` 是 `ConcurrentDictionary.TryAdd`
    /// ⇒ 管理器只保留**第一条**注册成功的事件；`GetEvent` 返回它，**不是**当前格的事件对象。
    /// 只放格对象而管理器无 id=0 时仍返回 `None`（该格会被重复新建）。
    ///
    /// **P3-B5（must_fix）**：返回**第一条已注册**的事件（`registered == true` 且属于本图），
    /// 不是全局表的 `first()`——首建事件格非法（`AddMapEvent` no-op）但后续事件注册成功时，
    /// `first()` 会错误地返回那条**未注册**的事件。`CellObjectMgr` 是 per-`Environment` 的，
    /// 故另加 `map_id` 过滤。
    #[must_use]
    pub fn event_at(&self, map_id: usize, x: i32, y: i32) -> Option<&FireBurnEvent> {
        let map = self.maps.get(map_id)?;
        if !map.has_event_cell(x, y) {
            return None;
        }
        self.fire_burn_events
            .iter()
            .find(|e| e.registered && e.map_id == map_id)
    }

    /// `MagMakeFireCross` 的 `new FireBurnEvent(...) + EventMgr.AddEvent(...)` 等价物。
    ///
    /// `pub` 供夹具按 oracle「ctor + AddMapEvent」预置事件（F22-C 的中心格占用）。
    pub fn add_fire_burn_event(
        &mut self,
        owner_id: i32,
        map_id: usize,
        x: i32,
        y: i32,
        damage: i32,
        continue_ms: i32,
    ) {
        let now = self.clock_ms;
        self.fire_burn_events.push(FireBurnEvent {
            owner_id: Some(owner_id),
            map_id,
            x,
            y,
            event_type: 1, // `Grobal2.ET_FIRE`
            damage,
            open_start_ms: now,
            continue_ms,
            run_start_ms: now,
            run_tick_ms: 500,
            fire_run_tick_ms: 0,
            closed: false,
            visible: true,
            active: true,
            registered: false,
        });
        // **BUG-REPLICA 3**：越界/不可走的格子 `AddMapEvent` no-op，但事件仍进全局表；
        // 只有格子真正追加成功时 `CellObjectMgr.Add(0, event)` 才执行（第一条胜）。
        let added = self
            .maps
            .get_mut(map_id)
            .is_some_and(|m| m.add_event_cell(x, y, now));
        if added {
            if let Some(evt) = self.fire_burn_events.last_mut() {
                evt.registered = true;
            }
            self.event_registered = true;
        }
    }

    /// **MB-1**：`new MapEvent(Envir, X, Y, ET_DIGOUTZOMBI, 5*60*1000, true) + EventMgr.AddEvent`
    /// （`DigOutZombi.cs:59-60`）。
    ///
    /// **建模差异（登记）**：oracle ctor 的 `Envir.AddMapEvent` 还会在该格追加一个
    /// `CellType.Event`、`CellObjId=0` 的格子对象；本仓的 `event_at`（火墙）与
    /// `CellObjectMgr` 是简化模型，混入 dig-out 格会让火墙判据串味 ⇒ 这里只维护
    /// `EventMgr.Events` 注册表（NEEDHOLE 门的唯一消费面），不落格子对象。
    pub fn add_dig_out_event(&mut self, map_id: usize, x: i32, y: i32) {
        self.add_map_event_at(map_id, x, y, 1, 5 * 60 * 1000); // ET_DIGOUTZOMBI
    }

    /// **MB-3**：`new MapEvent(Envir, X, Y, nType, dwETime, true) + EventMgr.AddEvent`
    /// （`MapEvent.cs:60-79` + `EventManager.cs`）的事件表落点。
    ///
    /// `event_type`：`Grobal2.ET_DIGOUTZOMBI=1`（`DigOutZombi.Turned`）、
    /// `ET_SCULPEICE=6`（`ScultureKingMonster.MeltStone`，`ScultureKingMonster.cs:31-32`）。
    /// 与 [`Self::add_dig_out_event`] 共用同一条 `EventMgr.Events` 表与 500ms `RunTick` 关闭门
    /// （`EventProcessor.ExecuteInternal`）；NEEDHOLE 门只读 `event_type == 1`，不会误命中。
    pub fn add_map_event_at(
        &mut self,
        map_id: usize,
        x: i32,
        y: i32,
        event_type: i32,
        continue_ms: i64,
    ) {
        let now = self.clock_ms;
        self.dig_out_events.push(DigOutEvent {
            map_id,
            x,
            y,
            event_type,
            open_start_ms: now,
            continue_ms,
            run_start_ms: now,
            run_tick_ms: 500,
            closed: false,
        });
    }

    /// `EventMgr.GetEvent(envir, nX, nY, ET_DIGOUTZOMBI) != null`（`EventManager.cs:29-42`：
    /// **逆插入序**找首个匹配；`Closed` 的事件已移出 `Events`）。
    #[must_use]
    pub fn dig_out_event_at(&self, map_id: usize, x: i32, y: i32) -> bool {
        self.dig_out_events
            .iter()
            .rev()
            .any(|e| !e.closed && e.event_type == 1 && e.map_id == map_id && e.x == x && e.y == y)
    }

    /// 取走本 tick 的怪物 `SendRefMsg` 帧事件（保序）。
    pub fn take_monster_ref_msgs(&mut self) -> Vec<crate::monai::MonsterRefMsg> {
        std::mem::take(&mut self.pending_monster_ref_msgs)
    }

    /// 取走本 tick **到点**的 `RM_REFMESSAGE` 广播事件（保序）。宿主据此出 `SM_STRUCK`。
    pub fn take_refmessage_frames(&mut self) -> Vec<crate::slave::RefMessageFrame> {
        std::mem::take(&mut self.pending_refmessage_frames)
    }

    /// **P3-B5**：9/10 穿透效果体（`MagicManager.cs:265-288` + `BaseObject.cs:896-931`）。
    ///
    /// 有序门与取数：
    /// 1. `GetNextPosition(CurrX,CurrY,nextDir,1)` 失败 ⇒ **零抽**、无效果体
    ///    （共享尾仍用点击坐标发 638）；
    /// 2. `GetNextPosition(CurrX,CurrY,nextDir,steps)`（5 = 9 / 8 = 10）求终点；
    /// 3. `nPower = GetAttackPower(GetPower(MPow)+Lo(MC), ΔMC+1)`（4 抽）；
    /// 4. 逐格（≤12 次）：`GetMovingObject(sx,sy,true)` → `IsProperTarget` → `R(10) >= AntiMagic`
    ///    →（10）`magPwr = Round(magPwr*1.5)` → `SendStruckDelayMsg(..., 600)`。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_pass_through_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        click_x: i32,
        click_y: i32,
        mc: i32,
        steps: i32,
        undead_attack: bool,
    ) -> PassThroughOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return PassThroughOutcome {
                end_x: click_x,
                end_y: click_y,
                ..PassThroughOutcome::default()
            };
        };
        let map_id = caster.map_id;
        let next_dir = crate::magic::next_direction(caster.x, caster.y, click_x, click_y);
        // `:267` `if (Envir.GetNextPosition(CurrX, CurrY, nextDir, 1, ref n14, ref n18))`
        let Some((mut sx, mut sy)) = self
            .maps
            .get(map_id)
            .and_then(|m| m.get_next_position(caster.x, caster.y, next_dir, 1))
        else {
            return PassThroughOutcome {
                end_x: click_x,
                end_y: click_y,
                ..PassThroughOutcome::default()
            };
        };
        // `:269/:281` 终点；`GetNextPosition` 先写 `snx=sx` ⇒ 未变时 = 施法者坐标。
        let (end_x, end_y) = self
            .maps
            .get(map_id)
            .and_then(|m| m.get_next_position(caster.x, caster.y, next_dir, steps))
            .unwrap_or((caster.x, caster.y));
        // `:270/:282` 威力（4 抽；按施法者分派 `GetAttackPower` override）。
        let atk = AttackPowerMode::for_attacker(&caster);
        let mut mag_pwr =
            crate::magic_effect::single_hit_power_mc(&mut self.rng, magic, level, mc, atk);
        let group = self.attacker_group_members(&caster);
        let mut hits = Vec::new();
        for _ in 0..12 {
            if let Some(obj_id) = self.moving_object_at(map_id, sx, sy, true) {
                let proper = self.entities.get(obj_id).is_some_and(|t| {
                    crate::combat::is_proper_target_grouped_direct(&caster, t, &group)
                });
                if proper {
                    let anti = self
                        .entities
                        .get(obj_id)
                        .map_or(0, |t| i32::from(t.anti_magic));
                    if crate::magic_effect::anti_magic_roll(&mut self.rng, anti) {
                        if undead_attack {
                            mag_pwr = crate::magic_effect::scale_undead_attack(mag_pwr);
                        }
                        self.send_struck_delay_msg(obj_id, caster_id, mag_pwr, 600);
                        hits.push(PassThroughHit {
                            target_id: obj_id,
                            power: mag_pwr,
                        });
                    }
                }
            }
            if sx == end_x && sy == end_y {
                break;
            }
            let nd = crate::magic::next_direction(sx, sy, end_x, end_y);
            let Some((nx, ny)) = self
                .maps
                .get(map_id)
                .and_then(|m| m.get_next_position(sx, sy, nd, 1))
            else {
                break;
            };
            sx = nx;
            sy = ny;
        }
        PassThroughOutcome {
            bo_train: !hits.is_empty(),
            hits,
            end_x,
            end_y,
        }
    }

    /// `BaseObject.MagPassThroughMagic(short sx, short sy, short tx, short ty, byte nDir,
    /// int magPwr, bool undeadAttack)`（`BaseObject.cs:896-925`）的**怪侧**入口。
    ///
    /// 与 [`World::cast_pass_through_spell`] 的区别：威力由调用方先算好
    /// （`LightingZombi.LightingAttack` 的 `Random(|Hi(DC)-Lo(DC)|+1)` 在**调用前**已抽），
    /// 且 `IsProperTarget` 走**怪**的基版（不带组队表）。逐句：
    ///
    /// ```text
    /// for (i = 0; i < 12; i++) {
    ///   baseObject = Envir.GetMovingObject(sx, sy, true);
    ///   if (baseObject != null && IsProperTarget(baseObject)
    ///       && Random(10) >= baseObject.AntiMagic) {
    ///       if (undeadAttack) magPwr = Round(magPwr * 1.5);          // 逐次复合
    ///       baseObject.SendStruckDelayMsg(this.ActorId, RM_MAGSTRUCK, 0, magPwr, 0, 0, "", 600);
    ///       tcount++;
    ///   }
    ///   if (!(sx == tx && sy == ty)) {
    ///       nDir = GetNextDirection(sx, sy, tx, ty);
    ///       if (!GetNextPosition(sx, sy, nDir, 1, ref sx, ref sy)) break;
    ///   } else break;
    /// }
    /// ```
    ///
    /// 取数：每格最多一次 `Random(10)`（`anti_magic_roll`），**只在**该格有合法目标时抽；
    /// `undeadAttack` 的 1.5 倍是**原地复合**（命中越多，后续越大）。
    #[allow(clippy::too_many_arguments)]
    pub fn mag_pass_through_magic(
        &mut self,
        caster_id: i32,
        sx: i32,
        sy: i32,
        tx: i32,
        ty: i32,
        _n_dir: u8,
        mag_pwr: i32,
        undead_attack: bool,
    ) -> i32 {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return 0;
        };
        let map_id = caster.map_id;
        // `nDir` 形参在 oracle 里只作为 `GetNextDirection` 的初值，而循环第一轮必先重算 ⇒ 不读。
        let (mut sx, mut sy, mut mag_pwr) = (sx, sy, mag_pwr);
        let mut tcount = 0;
        for _ in 0..12 {
            if let Some(obj_id) = self.moving_object_at(map_id, sx, sy, true) {
                let proper = self
                    .entities
                    .get(obj_id)
                    .is_some_and(|t| crate::combat::is_proper_target(&caster, t));
                if proper {
                    let anti = self
                        .entities
                        .get(obj_id)
                        .map_or(0, |t| i32::from(t.anti_magic));
                    if crate::magic_effect::anti_magic_roll(&mut self.rng, anti) {
                        if undead_attack {
                            mag_pwr = crate::magic_effect::scale_undead_attack(mag_pwr);
                        }
                        self.send_struck_delay_msg(obj_id, caster_id, mag_pwr, 600);
                        tcount += 1;
                    }
                }
            }
            if sx == tx && sy == ty {
                break;
            }
            let n_dir = crate::magic::next_direction(sx, sy, tx, ty);
            let Some((nx, ny)) = self
                .maps
                .get(map_id)
                .and_then(|m| m.get_next_position(sx, sy, n_dir, 1))
            else {
                break;
            };
            sx = nx;
            sy = ny;
        }
        tcount
    }

    /// **P3-B5**：22 火墙效果体（`MagicManager.cs:412-417` + `:1064-1098`）。
    ///
    /// **取数在安全区门之前**（C# 实参先求值）：`nDamage = GetAttackPower(GetPower(MPow)+Lo(MC), ΔMC+1)`
    /// （4 抽）→ `time = GetPower(10) + (GetRPow(MC) >> 1)`（1 + 0/1 抽）。
    /// 安全区门失败 ⇒ `SysMsg(..., Red, Notice)` + `return false`（调用方发 100），**事件零落地**。
    /// 否则按固定顺序 上/左/中/右/下 逐格 `GetEvent == null` 才建事件；`boTrain = true`
    /// （与是否真的新建了事件**无关**）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_fire_wall_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        click_x: i32,
        click_y: i32,
        mc: i32,
        disable_in_safe_zone: bool,
    ) -> FireWallOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return FireWallOutcome::default();
        };
        let atk = AttackPowerMode::for_attacker(&caster);
        let (damage, time) = crate::magic::power_fire_wall(&mut self.rng, magic, level, mc, atk);
        // `:1066-1070`：`DisableInSafeZoneFireCross && InSafeZone(Envir, nX, nY)`。
        // 夹具/生产用施法者实体的 `in_safe_zone`（= `Envir.Flag.SafeArea` 的第 1 步）。
        if disable_in_safe_zone && caster.in_safe_zone {
            return FireWallOutcome {
                bo_train: false,
                safe_zone_blocked: true,
            };
        }
        let map_id = caster.map_id;
        for (x, y) in [
            (click_x, click_y - 1),
            (click_x - 1, click_y),
            (click_x, click_y),
            (click_x + 1, click_y),
            (click_x, click_y + 1),
        ] {
            if self.event_at(map_id, x, y).is_none() {
                self.add_fire_burn_event(caster_id, map_id, x, y, damage, time * 1000);
            }
        }
        FireWallOutcome {
            bo_train: true,
            safe_zone_blocked: false,
        }
    }

    /// `EventProcessor.ExecuteInternal`（`EventProcessor.cs:31-48`）的单线程等价物：
    /// 逆插入序遍历，`Active && now - RunStart > RunTick(500ms)` 才 `RunStart = now; Run()`；
    /// `Run()` 造成的 `RM_MAGSTRUCK_MINE` 进入各目标自己的队列（`pending_mag_struck.mine=true`），
    /// 由宿主随后 [`World::deliver_mag_struck`] 处理（MSD 照抽、**无** `R(1000)`）。
    ///
    /// `FireBurnEvent.Run`（`FireBurnEvent.cs:23-43`）：`now - FireRunTick > 3000` 才结算；
    /// `MapEvent.Run` 尾：`now - OpenStartTick > ContinueTime`（严格大于）⇒ `Closed`。
    pub fn run_map_events(&mut self) {
        let now = self.clock_ms;
        let len = self.fire_burn_events.len();
        for i in (0..len).rev() {
            let ev = &self.fire_burn_events[i];
            if ev.closed || !ev.active || now - ev.run_start_ms <= ev.run_tick_ms {
                continue;
            }
            self.fire_burn_events[i].run_start_ms = now;
            let (map_id, x, y, damage, owner_id) = {
                let ev = &self.fire_burn_events[i];
                (ev.map_id, ev.x, ev.y, ev.damage, ev.owner_id)
            };
            if now - self.fire_burn_events[i].fire_run_tick_ms > 3000 {
                self.fire_burn_events[i].fire_run_tick_ms = now;
                let owner = owner_id
                    .and_then(|id| self.entities.get(id).cloned())
                    .filter(|e| !e.ghost && !e.death);
                if let Some(o) = owner {
                    let group = self.attacker_group_members(&o);
                    for tid in self.base_objects_at(map_id, x, y) {
                        let proper = self.entities.get(tid).is_some_and(|t| {
                            crate::combat::is_proper_target_grouped_direct(&o, t, &group)
                        });
                        if proper {
                            let order = self.msg_order;
                            self.msg_order += 1;
                            self.pending_mag_struck.push(PendingMagStruck {
                                target_id: tid,
                                caster_id: o.id,
                                power: damage,
                                mine: true,
                                order,
                            });
                        }
                    }
                }
            }
            // `base.Run()` 的关闭门（严格大于）与 `OwnBaseObject` 的 Ghost/Death 归一。
            let (open_start, continue_ms) = {
                let ev = &self.fire_burn_events[i];
                (ev.open_start_ms, i64::from(ev.continue_ms))
            };
            if now - open_start > continue_ms {
                self.fire_burn_events[i].closed = true;
                self.fire_burn_events[i].visible = false;
            }
            let still_alive = owner_id
                .is_some_and(|id| self.entities.get(id).is_some_and(|e| !e.ghost && !e.death));
            if !still_alive {
                self.fire_burn_events[i].owner_id = None;
            }
        }
        // **MB-1**：`EventProcessor` 对 `ET_DIGOUTZOMBI` 事件的 `RunTick=500ms` 门与
        // `MapEvent.Run` 的关闭门（`now - OpenStartTick > ContinueTime`，严格大于）。
        // `Closed` 后 `EventProcessor` 立即移出 `Events`（`EventProcessor.cs:40-44`）⇒ retain。
        if !self.dig_out_events.is_empty() {
            for ev in &mut self.dig_out_events {
                if ev.closed || now - ev.run_start_ms <= ev.run_tick_ms {
                    continue;
                }
                ev.run_start_ms = now;
                if now - ev.open_start_ms > ev.continue_ms {
                    ev.closed = true;
                }
            }
            self.dig_out_events.retain(|e| !e.closed);
        }
    }

    /// **P3-B5**：36 无极真气效果体（`MagicManager.cs:610-630` 的 `:612-625`）。
    ///
    /// 前置（`IsProperFriend` / `CheckAmulet` / `UseAmulet`）由调用方完成；本函数从
    /// 「目标归一 + 友方门」开始，到抽数为止（`AttPowerUp` 由宿主在 recalc 后落地）。
    ///
    /// - `targetObject == null ⇒ this`（`target = caster_id`）；
    /// - `IsProperFriend`：玩家施法者只放行 `Race == Play`（`PlayObject.Attack.cs:755-822`），
    ///   非玩家目标 ⇒ false（零抽；`boSpellFail` 保持 true ⇒ 恰一条 639、无尾）；
    /// - 抽数：`R(Level)`（`Level == 0` 也推进）→ `GP13` → `AP`；
    /// - 防御臂：目标非玩家（当前入口不可达）⇒ `invalid_cast = true`（抽数已发生）。
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn cast_uenhancer_spell(
        &mut self,
        caster_id: i32,
        magic: &mir2_data::MagicInfo,
        level: u8,
        target: Option<i32>,
        sc: i32,
    ) -> UEnhancerOutcome {
        let Some(caster) = self.entities.get(caster_id).cloned() else {
            return UEnhancerOutcome::default();
        };
        let tid = target.unwrap_or(caster_id);
        let Some(t) = self.entities.get(tid).cloned() else {
            return UEnhancerOutcome {
                target: tid,
                ..UEnhancerOutcome::default()
            };
        };
        let in_group = self.attacker_group_members(&caster).contains(&tid);
        if !crate::combat::is_proper_friend(&caster, &t, in_group) {
            return UEnhancerOutcome {
                allowed: false,
                target: tid,
                ..UEnhancerOutcome::default()
            };
        }
        let atk = AttackPowerMode::for_attacker(&caster);
        let (n_power, n_time) =
            crate::magic_effect::power_uenhancer(&mut self.rng, magic, level, sc, atk);
        UEnhancerOutcome {
            allowed: true,
            target: tid,
            n_power: n_power as u16,
            n_time,
            // `(IPlayerActor)targetObject`：`Race != Play` 时抛 `InvalidCastException`。
            invalid_cast: t.race != crate::entity::ACTOR_RACE_PLAY,
        }
    }

    /// **P3-B5**：`PlayObject.AttPowerUp`（`PlayObject.cs:5323-5330`）的状态面。
    ///
    /// 只写 `ExtraAbil[0]/ExtraAbilTimes[0]`；`SysMsg` / `RecalcAbilitys` / `RM_ABILITY`
    /// 由宿主完成（`RecalcAbilitys` 需要 `record.data` 与目录）。
    pub fn att_power_up(&mut self, target_id: i32, n_power: u16, n_time: i32) {
        let now = self.clock_ms;
        if let Some(e) = self.entities.get_mut(target_id) {
            e.extra_abil[0] = n_power;
            e.extra_abil_times[0] = now + i64::from(n_time) * 1000;
        }
    }

    /// **P3-B5**：`ExtraAbil` 过期/到期前 10s 检查（`PlayObject.Message.cs:163-227`）。
    ///
    /// 节拍：宿主在每个 world tick 调一次（oracle 在每个 `PlayObject.Run` 的消息泵里；
    /// 相位差登记 T-B5-2）。**过期不 recalc、不发 52**——只 `HealthSpellChanged()`（53）。
    #[must_use]
    pub fn expire_extra_abil(&mut self) -> Vec<ExtraAbilExpiry> {
        let now = self.clock_ms;
        let mut out = Vec::new();
        for id in self.entities.order_index() {
            let Some(e) = self.entities.get_mut(id) else {
                continue;
            };
            let mut expiry = ExtraAbilExpiry {
                actor_id: id,
                ..ExtraAbilExpiry::default()
            };
            for i in 0..7usize {
                if e.extra_abil[i] == 0 {
                    continue;
                }
                if now > e.extra_abil_times[i] {
                    e.extra_abil[i] = 0;
                    e.extra_abil_flag[i] = 0;
                    expiry.health_changed = true;
                    expiry
                        .events
                        .push(ExtraAbilEventKind::Restored(u8::try_from(i).unwrap_or(0)));
                } else if e.extra_abil_flag[i] == 0 && now > e.extra_abil_times[i] - 10_000 {
                    e.extra_abil_flag[i] = 1;
                    expiry.events.push(ExtraAbilEventKind::TenSecondsBefore(
                        u8::try_from(i).unwrap_or(0),
                    ));
                }
            }
            if !expiry.events.is_empty() {
                out.push(expiry);
            }
        }
        out
    }

    /// **魔号 8（`SKILL_FIREWIND` 抗拒火环）/ 37（`SKILL_ENERGYREPULSOR` 气功波）效果体**：
    /// `MagicManager.MagPushArround`（`src/M2Server/Magic/MagicManager.cs:24-52`）的生产入口。
    /// oracle 两处 `case` 的函数体逐字相同：`:259-264`（8）与 `:601-606`（37），
    /// 都是 `if (MagPushArround(playObject, userMagic.Level) > 0) boTrain = true;`。
    ///
    /// 逐行口径（`VisibleActors` 顺序 = 施法者 `visible_actors` 列表序，两侧同源）：
    /// ```text
    /// :27  for i in VisibleActors
    /// :30  |Cx-Tx| <= 1 && |Cy-Ty| <= 1
    /// :32  !Death && target != playObject
    /// :34  playObject.Abil.Level > target.Abil.Level && !target.StickMode
    /// :36  levelgap = ...
    /// :37  Random(20) < 6 + nPushLevel*3 + levelgap     <- magic_effect::push_chance
    /// :39  IsProperTarget(target)
    /// :41  push = 1 + MAX(0, nPushLevel-1) + Random(2)  <- magic_effect::push_steps
    /// :42  nDir = GetNextDirection(...)
    /// :43  target.CharPushed(nDir, push); result++     <- 本仓 push_now（逐格 RM_PUSH 事件）
    /// ```
    ///
    /// 返回 `(oracle result, 逐格推挤事件)`。**`result` 是「调用了 `CharPushed` 的目标数」**，
    /// 与 `CharPushed` 实际推进几格无关（oracle 不看其返回值）——训练尾 `boTrain` 的判据。
    ///
    /// **登记（本卡不实现）**：`CharPushed` 的
    /// `WalkTick += 800`（`Race >= Animal` 支）本骨架未建模；`IsProperTarget` 用
    /// [`crate::combat::is_proper_target_grouped`]（组队成员表 = `Entity::group_members`）。
    #[must_use]
    pub fn cast_firewind_spell(
        &mut self,
        actor_id: i32,
        push_level: i32,
    ) -> (i32, Vec<crate::action::PushEvent>) {
        let Some(caster) = self.entities.get(actor_id).cloned() else {
            return (0, Vec::new());
        };
        // oracle `VisibleActors` 的迭代序即列表序（本骨架 `Entity::visible_actors` 同源）。
        let visible: Vec<i32> = caster.visible_actors.iter().map(|v| v.target).collect();
        let group_members = self.attacker_group_members(&caster);
        let mut result = 0;
        let mut events = Vec::new();
        for target_id in visible {
            let Some(target) = self.entities.get(target_id).cloned() else {
                continue;
            };
            if (caster.x - target.x).abs() > 1 || (caster.y - target.y).abs() > 1 {
                continue;
            }
            if target.death || target_id == actor_id {
                continue;
            }
            if caster.level <= target.level || target.stick_mode {
                // `:34 playObject.Abil.Level > targetObject.Abil.Level && !targetObject.StickMode`
                // ——StickMode 目标**零抽**（`Random(20)` 在门内，漏掉此门会平移整条 RNG 流）。
                continue;
            }
            let level_gap = i32::from(caster.level) - i32::from(target.level);
            if !crate::magic_effect::push_chance(&mut self.rng, push_level, level_gap) {
                continue;
            }
            // `PlayObject.IsProperTarget`：组队成员按 `HAM_GROUP` 剔除（oracle 读内存里的
            // `GroupOwner/GroupMembers`；本骨架同源字段在 `Entity::{group_owner,group_members}`）。
            if !crate::combat::is_proper_target_grouped(&caster, &target, &group_members) {
                continue;
            }
            let push = crate::magic_effect::push_steps(&mut self.rng, push_level);
            let n_dir = crate::magic::next_direction(caster.x, caster.y, target.x, target.y);
            let pushed = self.push_now(target_id, n_dir, push);
            // oracle `CharPushed`：循环内 `Dir = nDir`，收尾 `Dir = nBackDir`（`result > 0`）或旧值
            // （`result == 0`）。本骨架 `push_now` 不改 `Dir` => 在此按 oracle 收尾语义补写。
            if !pushed.is_empty() {
                if let Some(e) = self.entities.get_mut(target_id) {
                    e.ai.dir = crate::magic::back_dir(n_dir);
                }
            }
            events.extend(pushed);
            // oracle `result++` 在 `CharPushed` 调用之后、**不看**其返回值。
            result += 1;
        }
        (result, events)
    }

    /// **魔号 21（`SKILL_SPACEMOVE` 瞬息移动）效果体**：`MagicManager.MagSaceMove`
    /// （`MagicManager.cs:876-891`）+ `BaseObject.MapRandomMove`（`:1214-1237`）+
    /// `BaseObject.SpaceMove`（`:1103-1168`，同服支）的生产入口。
    ///
    /// 取数次序（与 oracle 逐条同序）：
    /// 1. `Random(11) < nLevel*2 + 4`（`:879`，站点 `magic_effect`）；门不过 => **一个数都不多抽**；
    /// 2. `MapRandomMove` 两次初始取数（`:1235/1236`，站点 `merchant`）；**只有 `home_map` 解析成功才抽**；
    /// 3. `SpaceMoveGetRandXY` 的移位环（`BaseObject.cs:1040-1096`，站点 `space_move`）。
    ///
    /// `home_map` = 宿主用 `MapRegistry::id_of(CharacterData.HomeMap)` 解析的结果（`None` =>
    /// oracle `FindMap == null` => 不抽、不移动）。落地用 [`World::switch_map_space`]（记一条
    /// [`crate::session::MapChange`]，`space_move_show = Some(true)` = oracle `nInt == 1` 的
    /// `RM_SPACEMOVE_SHOW2`）。
    ///
    /// **失败支（oracle `if (!moveSuccess)` 回滚）**：本实现在**落地前**算完 `SpaceMoveGetRandXY`，
    /// 未找到落点 => 一个格子都不动（等价于 oracle 的「恢复旧 `Envir/CurrX/CurrY` + 重新
    /// `AddMapObject`」；oracle 那一步在可观测状态上恒等于「没动」）。
    ///
    /// **登记（本卡不实现）**：`IsTimeRecall = false`（无该字段，仅回报 `cross_map`）、
    /// `OnEnvirnomentChanged()`、`VisibleHumanList.Clear()`、`SpaceMoved` 字段。
    pub fn cast_space_move_spell(
        &mut self,
        chr_name: &str,
        actor_id: i32,
        level: i32,
        home_map: Option<usize>,
    ) -> SpaceMoveCastOutcome {
        self.cast_space_move_spell_with(chr_name, actor_id, level, home_map, |_| {})
    }

    /// [`World::cast_space_move_spell`] 的宿主挂钩版：`after_gate` 在 `Random(11)` 门**通过后、
    /// `MapRandomMove` 之前**调用（oracle `MagicManager.cs:881` 的
    /// `SendRefMsg(RM_SPACEMOVE_FIRE2, …)` 恰在这个位置——收件人必须按**施法前**的旧图/旧坐标算）。
    ///
    /// 不传钩子的调用方用 [`World::cast_space_move_spell`]（行为与取数次序逐字节不变）。
    pub fn cast_space_move_spell_with<F>(
        &mut self,
        chr_name: &str,
        actor_id: i32,
        level: i32,
        home_map: Option<usize>,
        after_gate: F,
    ) -> SpaceMoveCastOutcome
    where
        F: FnOnce(&World),
    {
        let mut out = SpaceMoveCastOutcome::default();
        let Some(e) = self.entities.get(actor_id) else {
            return out;
        };
        out.x = e.x;
        out.y = e.y;
        out.map_id = e.map_id;
        if !crate::magic_effect::sace_move_gate(&mut self.rng, level) {
            return out;
        }
        out.result = true;
        // oracle `:881 SendRefMsg(RM_SPACEMOVE_FIRE2, …)`：在 `MapRandomMove` **之前**。
        after_gate(&*self);
        let Some(dst) = home_map else {
            return out;
        };
        if dst >= self.maps.len() {
            return out;
        }
        out.map_found = true;
        let (w, h) = {
            let map = &self.maps[dst];
            (map.width, map.height)
        };
        // `MapRandomMove` 的两次初始取数（`merchant` 组）。
        let (nx, ny) = crate::merchant::map_random_move_xy(&mut self.rng, w, h);
        // `SpaceMove` 的移位环（`space_move` 组）；`can_walk` = `CanWalk(x,y,true)`（`cellSuccess && Valid`）。
        let found = {
            let rng = &mut self.rng;
            let map = &self.maps[dst];
            crate::space_move::space_move_get_rand_xy(rng, w, h, nx, ny, |x, y| {
                map.get_cell_info(x, y).1
            })
        };
        let Some((fx, fy)) = found else {
            return out;
        };
        // `BaseObject.SpaceMove` 同服支：旧图摘除 -> 新图登记 -> 记 `MapChange`（宿主出 633/634/SHOW2）。
        if self.switch_map_space(chr_name, dst, fx, fy, true).is_err() {
            return out;
        }
        out.moved = true;
        out.space_moved = true;
        out.cross_map = dst != out.map_id;
        out.x = fx;
        out.y = fy;
        out.map_id = dst;
        out
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

    /// 开启 7 参 `SendStruckDelayMsg`（`RM_REFMESSAGE`）延时层（缺省关闭；见
    /// [`World::refmessage_delay_enabled`]）。生产宿主起世界时调用；合成 golden 保持关闭。
    pub fn enable_refmessage_delay(&mut self) {
        self.refmessage_delay_enabled = true;
    }

    /// 7 参 `SendStruckDelayMsg`（`RM_REFMESSAGE`）延时层是否开启。
    #[must_use]
    pub fn refmessage_delay_enabled(&self) -> bool {
        self.refmessage_delay_enabled
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

    /// 设置某张地图的 `Envir.Flag.FightZone`/`Fight3Zone`（`DieDropItems` 的地图门，
    /// `BaseObject.cs:2452`）。越界 `map_id` 静默忽略。幂等。生产宿主在起世界时按注册表逐图灌入。
    pub fn set_map_fight_zone(&mut self, map_id: usize, fight_zone: bool, fight3_zone: bool) {
        if let Some(map) = self.maps.get_mut(map_id) {
            map.set_fight_zone(fight_zone, fight3_zone);
        }
    }

    /// **`openmir2-player-drop-useitems`**：灌入玩家死亡 `DropUseItems` 的配置
    /// （`Server.conf [Setup] DieDropUseItemRate`/`DieRedDropUseItemRate`/`DropItemRage`
    /// + `Envir/DisableTakeOffList.txt` 的键集合）。缺省 = oracle 缺省（30/15/3/空）。
    pub fn set_death_drop_config(
        &mut self,
        die_drop_use_item_rate: i32,
        die_red_drop_use_item_rate: i32,
        drop_item_rage: i32,
        disable_take_off_keys: Vec<u16>,
    ) {
        self.die_drop_use_item_rate = die_drop_use_item_rate;
        self.die_red_drop_use_item_rate = die_red_drop_use_item_rate;
        self.drop_item_rage = drop_item_rage;
        self.disable_take_off_keys = disable_take_off_keys;
    }

    /// **`openmir2-player-drop-useitems`**：`KillTargetTrigger` 第二段的两个配置门
    /// （`[Setup] KillByHumanDropUseItem` / `KillByMonstDropUseItem`）。
    pub fn set_death_drop_kill_config(
        &mut self,
        kill_by_human_drop_use_item: bool,
        kill_by_monst_drop_use_item: bool,
    ) {
        self.kill_by_human_drop_use_item = kill_by_human_drop_use_item;
        self.kill_by_monst_drop_use_item = kill_by_monst_drop_use_item;
    }

    /// **生产入口**：灌入 `stditems` 的**顺序表**作为物品目录。
    ///
    /// `items[i]` 对应 `Index = i + 1`（oracle `GameItemSystem` 的 1-based 口径，
    /// 同 `bins/game-svr::items::catalog_from`）。目录必须是**原始表顺序**（空名项也占位）：
    /// `VecItemCatalog::get_std_item` 按 `wrapping_sub(1)` 取下标、空名返回 `None`；
    /// 未灌入（空向量）时查表恒 `None`。
    ///
    /// 消费点：`death_drop::drop_use_items_on_death`、怪物掉落生成等所有走
    /// `VecItemCatalog` 的路径。宿主 `bins/game-svr` 在建世界时用
    /// `EnterConfig::world_item_catalog`（与帧/能力面的 `StdItemCatalog` 同源同表）喂入。
    pub fn set_item_catalog(&mut self, items: Vec<StdItem>) {
        self.item_catalog = items;
    }

    /// 设置某张地图的 `Envir.Flag.NoDropItem`（`KillTargetTrigger` 第二段的地图门）。
    pub fn set_map_no_drop_item(&mut self, map_id: usize, no_drop_item: bool) {
        if let Some(map) = self.maps.get_mut(map_id) {
            map.set_no_drop_item(no_drop_item);
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
        // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`：按落图时刻重盖。
        entity.verify_tick = self.clock_ms;
        // `BaseObject.cs:442` ctor `SearchTick = GetTickCount()`（玩家 ctor 又在
        // `PlayObject.cs:901` 置一次）：ctor 等价物拿不到世界时钟 ⇒ 按落图时刻重盖。
        // 之后 `World::tick` 的玩家 scanner 按 `now - SearchTick > SearchTime(=1000)` 节流。
        entity.ai.search_tick = self.clock_ms;
        // `PlayObject.cs:913` ctor `CheckDupObjTick = GetTickCount()`（重叠推挤的 3s 门从落图时刻起算）。
        entity.check_dup_obj_tick = self.clock_ms;
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
            death_use_item_drops: Vec::new(),
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
            extra_abil_expiries: Vec::new(),
            pushes: Vec::new(),
            rushes: Vec::new(),
            butches: Vec::new(),
            hides: Vec::new(),
            buff_status_changes: Vec::new(),
            buff_sysmsgs: Vec::new(),
            buff_recalcs: Vec::new(),
            buff_health_changes: Vec::new(),
            run_state_expiries: Vec::new(),
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
        // P3-B2：`RM_MAGHEALING`（治疗）不依赖该开关；主人死族在 `deliver_self_delay_msgs`
        // 内部仍按 `slave_self_delay_enabled` 过滤（关闭时保持入队）。
        record.slave_self_deliveries = self.deliver_self_delay_msgs();
        // **P3-B5**：`ExtraAbil` 过期检查（oracle 在 `PlayObject.Run` 的消息泵；
        // 本骨架每 world tick 一次，相位差登记 T-B5-2）与地图事件处理器
        // （`EventProcessor.ExecuteInternal` 的 20ms 周期 / 每事件 500ms `RunTick` 门）。
        record.extra_abil_expiries = self.expire_extra_abil();
        self.run_map_events();

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

        // **`PlayObject.Run` 的状态到期段**（`PlayObject.Message.cs:54-64`）：烈火 20s / 双龙 60s。
        // 在玩家 `base.Run`（VerifyMapTime 段）之前；两者都不抽随机数。oracle 的玩家 `Run`
        // 名义 300ms 一次、本骨架每个 world tick（200ms）一次——相位差与既有玩家散段同口径登记。
        record.run_state_expiries = self.expire_player_run_state();

        // **`PlayObject.Run` 的重叠推挤段**（`PlayObject.Message.cs:263-285`）：
        // 3s 周期门内 `GetXYObjCount`；`>=2` 记 `BoDuplication/DupStartTick`；
        // `>=3 且 >3s` / `==2 且 >10s` 且 `<20s` ⇒ `CharPushed(RandomByte(8), 1)`。
        // 位置照 oracle：在 FireHit/TwinHit 到期段之后、`base.Run` 的 VerifyMapTime 之前。
        // 只有三处门全部通过才取数（`crate::run_gap::dup_push_dir`）；事件进 `record.pushes`。
        record.pushes.extend(self.check_dup_obj());

        // **玩家 `BaseObject.Run` 的 VerifyMapTime 段**（oracle `WorldServer.cs:697` 每 ~200ms
        // 驱动一次 `playObject.Run()`，末尾无条件 `base.Run()`）——必须在 AOI 扫描之前刷新
        // 静止玩家的格子时间（`BaseObject.Base.cs:223-228`）。30s 门在 `verify_map_time` 内。
        self.verify_players_map_time();

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
                    // **本 tick 是否自扫（oracle 的调用点决定）**：
                    //
                    // 1. 玩家（`WorldServer.cs:663-667`）：
                    //    `if ((GetTickCount() - SearchTick) > SearchTime) { SearchTick = now;
                    //    SearchViewRange(); GameTimeChanged(); }` —— 严格大于，`SearchTime = 1000`
                    //    常量（`PlayObject.cs:900`）。200ms tick 下 1200ms 才命中。
                    //    Ghost 玩家已从 `PlayObjectList` 移除（`WorldServer.cs:708-711`）⇒ 不扫。
                    // 2. 非玩家只有 `IsSlave || Race in {Guard(11), ArcherGuard(112),
                    //    SlaveMonster(254)}`（`WorldServer.MonGen.cs:300-313`）在
                    //    `(currentTick - SearchTick) > SearchTime` 门后自扫；**其余怪物不主动全扫**
                    //    —— 被动视野由玩家扫描末尾的 `RM_UPDATEVIEWRANGE` 投递
                    //    （`PlayObject.cs:3130-3139` → `MonsterObject.cs:136-185`）。
                    //    Ghost 怪在 `ProcessMonsters` 里整只跳过（`WorldServer.MonGen.cs:320-333`）。
                    // 3. 固定 NPC / 商人：oracle 无 `SearchViewRange` 调用点 ⇒ 不自扫。
                    //
                    // 两个调用点都是「命中门 ⇒ 先 `SearchTick = now` 再扫」。
                    //
                    // `aoi_order` 仍记录每个实体的处理顺序（它是槽位顺序诊断，不是「实际扫了
                    // 几次」的计数）；被节流跳过/不自扫的实体也入列，保持既有诊断契约不变。
                    let scan = match self.entities.get(*id) {
                        Some(e) if e.race == crate::entity::ACTOR_RACE_PLAY => {
                            !e.ghost && self.clock_ms - e.ai.search_tick > e.ai.search_time
                        }
                        Some(e) if !e.ghost && crate::monai::monster_self_scans(e) => {
                            self.clock_ms - e.ai.search_tick > e.ai.search_time
                        }
                        Some(_) => false,
                        None => continue,
                    };
                    if !scan {
                        record.aoi_order.push(*id);
                        continue;
                    }
                    if let Some(e) = self.entities.get_mut(*id) {
                        e.ai.search_tick = self.clock_ms;
                    }
                    let (map, entities, clock) =
                        (&mut self.maps, &mut self.entities, self.clock_ms);
                    let outcome =
                        search_view_range_with_updates(&mut map[map_id], entities, *id, clock);
                    for target in outcome.removed {
                        // 观察者 `*id` 收「`target` 离开视野」⇒ `SM_DISAPPEAR(target)`（保序）。
                        record.hides.push((*id, target));
                    }
                    // 玩家扫描期间的 `RM_UPDATEVIEWRANGE` 投递（`PlayObject.cs:3136`）：
                    // 只有 oracle 有该 `Operate` 支的实体才入队（`MonsterObject`，排除
                    // `GuardUnit`/`CloneMonster`）；目标已消失 ⇒ 丢弃。
                    for (monster_id, player_id) in outcome.view_updates {
                        if entities
                            .get(monster_id)
                            .is_some_and(crate::monai::consumes_view_range_update)
                        {
                            if let Some(monster) = entities.get_mut(monster_id) {
                                monster.ai.pending_view_updates.push(player_id);
                            }
                        }
                    }
                }
                record.aoi_order.push(*id);
            }
        }
        self.by_map_scratch = by_map;

        self.last_tick = record.clone();
        record
    }

    /// `Envirnoment.VerifyMapTime`（`Envirnoment.cs:858-897`）的等价物 + `BaseObject.Run` 的
    /// 30s 门（`BaseObject.Base.cs:223-228`）。
    ///
    /// 语义逐条照抄：
    /// 1. `(now - VerifyTick) > 30*1000` 才动作（严格大于；命中先 `VerifyTick = now`）；
    /// 2. 在本格 `ObjList` 中按**下标升序**扫描：`now - AddTime >= 60s` 的项**先删除**；
    ///    C# 是 `for (i = 0; i < Count; i++)` 里的 `continue` ⇒ 删除后 `i++` **会跳过**被前移的
    ///    下一项（照抄该怪癖，不"修"）；
    /// 3. 未过期的项里若 `ActorObject && CellObjId == ActorId` ⇒ `AddTime = now` 并停止；
    /// 4. 循环结束仍未命中（`boVerify == false`）⇒ `AddMapObject` 重新登记（`AddMapObject`
    ///    同时置 `AddToMaped`/`AddObject`）。
    ///
    /// `DenyRefStatus` 本仓未建模（oracle ctor `:453` 恒 false、全仓无写点）⇒ 恒执行刷新。
    /// **不消费随机数**（纯状态演进），因此五批 RNG trace / golden 逐字节不变。
    ///
    /// 返回 `true` = 本次过了 30s 门并执行了刷新；`false` = 未到门 / actor 不存在。
    pub fn verify_map_time(&mut self, actor_id: i32) -> bool {
        let now = self.clock_ms;
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        // `BaseObject.Base.cs:223`：`(HUtil32.GetTickCount() - VerifyTick) > 30 * 1000`。
        if now - e.verify_tick <= VERIFY_MAP_TIME_MS {
            return false;
        }
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        // `baseObject.CellType`：`PlayObject` = Play、`AnimalObject` = Monster、`NormNpc` = Merchant。
        let cell_type = entity_cell_type(e);
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.verify_tick = now;
        }
        let Some(map) = self.maps.get_mut(map_id) else {
            return true;
        };
        let mut bo_verify = false;
        {
            let (cell, cell_success) = map.get_cell_info_mut(x, y);
            if cell_success && cell.is_available() {
                let mut i = 0usize;
                while i < cell.count() {
                    let obj = cell.obj_list[i];
                    if now - obj.add_time >= CELL_OBJECT_EXPIRE_MS {
                        // `boVerify = true; cellInfo.Remove(i);`
                        bo_verify = true;
                        cell.remove_at(i);
                        if cell.count() > 0 {
                            // C# `for` 的 `continue` 仍执行 `i++` ⇒ 跳过前移后的下一项。
                            i += 1;
                            continue;
                        }
                        cell.clear();
                        break;
                    }
                    if obj.actor_object && obj.cell_obj_id == actor_id {
                        // `cellInfo.Update(i, ref cellObject)` ⇒ `AddTime = GetTickCount()`。
                        cell.obj_list[i].add_time = now;
                        bo_verify = true;
                        break;
                    }
                    i += 1;
                }
            }
        }
        if !bo_verify {
            // `AddMapObject`（`Envirnoment.cs:252-296`）：成功时置 `AddToMaped`/`AddObject`。
            if map.add_map_object(x, y, cell_type, actor_id, now) {
                if let Some(e) = self.entities.get_mut(actor_id) {
                    e.add_to_mapped = true;
                }
            }
        }
        true
    }

    /// `PlayObject.Run` 的烈火/双龙到期段（`PlayObject.Message.cs:54-64`），按玩家槽位顺序。
    ///
    /// 逐行口径（**严格大于**）：
    /// - `FireHitSkill && (now - LatestFireHitTick) > 20*1000` ⇒ `FireHitSkill = false`；
    ///   `SysMsg(SpiritsGone, Red, Hint)`；`SendSocket("+UFIR")`；
    /// - `TwinHitSkill && (now - LatestTwinHitTick) > 60*1000` ⇒ `TwinHitSkill = false`；`SendSocket("+UTWN")`
    ///   （**无** `SysMsg`）。
    ///
    /// 不消费随机数。玩家选取 = `Race == ActorRace::Play && !Ghost`（与
    /// [`World::verify_players_map_time`] 同口径；oracle 的 `PlayObjectList` 不含幽灵）。
    /// 状态写入点（技能层）尚未移植 ⇒ 生产路径恒 `false`，本段零动作（登记见 `Entity::fire_hit_skill`）。
    pub fn expire_player_run_state(&mut self) -> Vec<(i32, RunStateExpiry)> {
        let now = self.clock_ms;
        let ids: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.race == crate::entity::ACTOR_RACE_PLAY && !e.ghost)
            .map(|e| e.id)
            .collect();
        let mut events = Vec::new();
        for id in ids {
            let Some(e) = self.entities.get_mut(id) else {
                continue;
            };
            // `PlayObject.Message.cs:54-58`：先 `SysMsg` 再 `SendSocket("+UFIR")`。
            if e.fire_hit_skill && (now - e.latest_fire_hit_tick) > 20 * 1000 {
                e.fire_hit_skill = false;
                events.push((id, RunStateExpiry::SpiritsGone));
                events.push((id, RunStateExpiry::FireHitSocket));
            }
            // `:60-64`：仅 `SendSocket("+UTWN")`。
            if e.twin_hit_skill && (now - e.latest_twin_hit_tick) > 60 * 1000 {
                e.twin_hit_skill = false;
                events.push((id, RunStateExpiry::TwinHitSocket));
            }
        }
        events
    }

    /// `Envirnoment.GetXYObjCount`（`src/M2Server/Maps/Envirnoment.cs:1062-1093`）的世界侧入口：
    /// 统计 `(x, y)` 格上 `!Ghost && !Death && !FixedHideMode && !ObMode` 的 actor 对象数。
    ///
    /// **建模差异（登记）**：`CastleDoor` 的 `HoldPlace` 支（门建筑只有在 `HoldPlace && !Ghost &&
    /// !Death && !FixedHideMode && !ObMode` 时计数）未建模——本仓的城门/城墙不在 `EntityStore`
    /// （见 `crate::castle` 的独立结构体），故走普通 actor 支。
    #[must_use]
    pub fn xy_obj_count(&self, map_id: usize, x: i32, y: i32) -> i32 {
        crate::monai::count_at_cell(self, map_id, x, y)
    }

    /// `PlayObject.Run` 的重叠推挤段（`PlayObject.Message.cs:263-285`），按玩家槽位顺序。
    ///
    /// 逐行口径（三处门都以 `>` 严格比较，照 oracle 原式）：
    ///
    /// ```text
    /// :263 if ((now - CheckDupObjTick) > 3000) {
    /// :265   CheckDupObjTick = now;
    /// :266   GetStartPoint();
    /// :267   tObjCount = Envir.GetXYObjCount(CurrX, CurrY);
    /// :268   if (tObjCount >= 2) { if (!BoDuplication) { BoDuplication = true; DupStartTick = now; } }
    /// :276   else { BoDuplication = false; }
    /// :280   if ((tObjCount >= 3 && now - DupStartTick > 3000 || tObjCount == 2 && now - DupStartTick > 10000)
    /// :281       && (now - DupStartTick < 20000))
    /// :283     CharPushed(RandomNumber.RandomByte(8), 1);
    /// ```
    ///
    /// **`GetStartPoint()` 处置（登记）**：`PlayObject.Base.cs:704-723` 的它只写
    /// `HomeMap/HomeX/HomeY`——`PvpLevel() >= 2` ⇒ `Config.RedHome*`，否则按当前 `Envir.MapName`
    /// 在 `M2Share.StartPointList` 里顺序查（`OrdinalIgnoreCase`）；**零取数**，且不参与
    /// `tObjCount`/推挤门。本仓的落点由**宿主**解析（`bins/game-svr/src/homemap.rs` +
    /// `MapRegistry::id_of(CharacterData.HomeMap)`，登录/回城时一次性给 `MapRandomMove`），
    /// 世界侧没有 per-entity `HomeMap` 状态 ⇒ 本段**不建模**（不影响本函数判据）。
    ///
    /// **取数**：只有三处门全部通过才调用 [`crate::run_gap::dup_push_dir`]（= C# `RandomByte(8)`，
    /// `random.Next(8)` **恰一次**、值域 `0..=7`）；随后 [`World::char_pushed_now`] 逐格发
    /// `RM_PUSH`。玩家选取 = `Race == Play && !Ghost`（oracle `PlayObjectList` 不含幽灵）。
    pub fn check_dup_obj(&mut self) -> Vec<crate::action::PushEvent> {
        let now = self.clock_ms;
        let ids: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.race == crate::entity::ACTOR_RACE_PLAY && !e.ghost)
            .map(|e| e.id)
            .collect();
        let mut pushes = Vec::new();
        for id in ids {
            let Some((map_id, x, y, last)) = self
                .entities
                .get(id)
                .map(|e| (e.map_id, e.x, e.y, e.check_dup_obj_tick))
            else {
                continue;
            };
            // `:263` 严格 `>`：恰 3000ms 不检查。
            if now - last <= 3000 {
                continue;
            }
            // `:265` `CheckDupObjTick = GetTickCount()`（本 tick 内同一 `now`）。
            if let Some(e) = self.entities.get_mut(id) {
                e.check_dup_obj_tick = now;
            }
            // `:266` `GetStartPoint()`：见本函数文档「处置（登记）」。
            // `:267` `tObjCount = Envir.GetXYObjCount(CurrX, CurrY)`（含自己）。
            let t_obj_count = self.xy_obj_count(map_id, x, y);
            // `:268-279`：`>=2` 首次置位；`<2` 清 `BoDuplication`（`DupStartTick` 不复位）。
            let elapsed = {
                let Some(e) = self.entities.get_mut(id) else {
                    continue;
                };
                if t_obj_count >= 2 {
                    if !e.bo_duplication {
                        e.bo_duplication = true;
                        e.dup_start_tick = now;
                    }
                } else {
                    e.bo_duplication = false;
                }
                now - e.dup_start_tick
            };
            // `:280-281`：C# 原式 `(A && B || C && D) && E`（`&&` 优先于 `||`）。
            let should_push = ((t_obj_count >= 3 && elapsed > 3000)
                || (t_obj_count == 2 && elapsed > 10000))
                && elapsed < 20000;
            if should_push {
                // `:283` 实参求值顺序：先取数，再 `CharPushed`。
                let dir = crate::run_gap::dup_push_dir(&mut self.rng);
                pushes.extend(self.char_pushed_now(id, dir, 1));
            }
        }
        pushes
    }

    /// 目标格是否可走（`Envirnoment.CanWalk(nX, nY, false)`）：**包含自身**——
    /// 与 [`World::cell_free`]（排除 `self_id`）不同：oracle `CanWalk` 不看 `cert`，同格 clamp 时
    /// 当前格上的自己也会挡住 `CharPushed`（`GetFrontPosition` 在边界原样返回当前格）。
    fn can_walk_cell(&self, map_id: usize, x: i32, y: i32) -> bool {
        let Some(map) = self.maps.get(map_id) else {
            return false;
        };
        if !map.cell_match(x, y) {
            return false;
        }
        let (cell, ok) = map.get_cell_info(x, y);
        // C# `if (cellSuccess && cellInfo.Valid)`；`get_cell_info` 的 `ok` 已含 `valid()`。
        if !ok {
            return false;
        }
        if !cell.is_available() {
            return true;
        }
        !cell.obj_list.iter().any(|o| {
            o.actor_object
                && self
                    .entities
                    .get(o.cell_obj_id)
                    .is_some_and(|e| !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode)
        })
    }

    /// `Envirnoment.CanFly(nsX, nsY, ndX, ndY)`（`Envirnoment.cs:1371-1393`）。
    ///
    /// **oracle 原样照抄（含两处怪行为）**：
    /// - `r30 = (ndY - ndX) / 10.0`——第二轴减的是 **ndX**（不是 `nsY`），照搬；
    /// - 循环体里 `nsX/nsY` **从不推进**，10 次迭代检查的是**同一格** ⇒ 首轮失败即失败，
    ///   首轮通过则后续 9 轮同样通过。判据只看 `CanWalk(flyX, flyY, true)`（忽略角色）。
    #[must_use]
    pub fn can_fly(&self, map_id: usize, ns_x: i32, ns_y: i32, nd_x: i32, nd_y: i32) -> bool {
        let r28 = f64::from(nd_x - ns_x) / 1.0e1;
        let r30 = f64::from(nd_y - nd_x) / 1.0e1;
        let fly_x = mir2_shared::hutil::round(f64::from(ns_x) + r28);
        let fly_y = mir2_shared::hutil::round(f64::from(ns_y) + r30);
        // `CanWalk(..., boFlag=true)`：`cellSuccess && cellInfo.Valid` ⇒ 忽略角色阻挡。
        self.can_walk_at(map_id, fly_x, fly_y, true)
    }

    /// `BaseObject.CharPushed(nDir, nPushCount)`（`src/M2Server/Actor/BaseObject.cs:854-897`）的
    /// 世界侧**逐句**实现（`Command::Push` 与重叠推挤共用）。
    ///
    /// 逐句：
    ///
    /// ```text
    /// :857 Dir = nDir;
    /// :858 nBackDir = GetBackDir(nDir);
    /// :859 for i in 0..nPushCount {
    /// :861   GetFrontPosition(ref nx, ref ny)                  // 用当前 Dir(= nDir) 与 CurrX/Y
    /// :862   if (Envir.CanWalk(nx, ny, false)) {
    /// :864     if (Envir.MoveToMovingObject(CurrX,CurrY,this,nx,ny,false)) {
    /// :866       CurrX = nx; CurrY = ny;
    /// :871       SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "");
    /// :873       result++;
    /// :874       if (Race >= ActorRace.Animal) WalkTick = WalkTick + 800;
    /// :878     } else break; :882 } else break;
    /// :890 Dir = nBackDir;
    /// :891 if (result == 0) Dir = olddir;
    /// ```
    ///
    /// `MoveToMovingObject` 的阻挡集合与 `CanWalk` 同源（`!Ghost && !Death && !FixedHideMode &&
    /// !ObMode`），故 `CanWalk` 通过后格子簿记必成功；这里仍走 `MapGrid::move_object`（源格摘除 +
    /// 目标格追加），**不**经 `try_step`（后者额外挡非玩家 `MapRoute`，而 oracle
    /// `MoveToMovingObject` 不挡——照抄）。
    ///
    /// **建模差异（登记）**：`CastleDoor.HoldPlace` 阻挡支不建模（城门不在 `EntityStore`）。
    pub(crate) fn char_pushed_now(
        &mut self,
        actor_id: i32,
        n_dir: u8,
        n_push_count: u8,
    ) -> Vec<crate::action::PushEvent> {
        let Some(e) = self.entities.get(actor_id) else {
            return Vec::new();
        };
        let old_dir = e.ai.dir;
        let (mut x, mut y, map_id, race) = (e.x, e.y, e.map_id, e.race);
        let Some((w, h)) = self.maps.get(map_id).map(|m| (m.width, m.height)) else {
            return Vec::new();
        };
        // `:857 Dir = nDir;`（先落，末尾按 result 决定是否复原）。
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.ai.dir = n_dir;
        }
        let back = crate::magic::back_dir(n_dir);
        let mut out = Vec::new();
        for _ in 0..n_push_count {
            let (nx, ny) = crate::magic::front_position(x, y, n_dir, w, h);
            // `:862 Envir.CanWalk(nx, ny, false)`（含自身；不可走即 break）。
            if !self.can_walk_cell(map_id, nx, ny) {
                break;
            }
            // `:864 Envir.MoveToMovingObject(CurrX, CurrY, this, nx, ny, false)`。
            let moved = {
                let (maps, entities) = (&mut self.maps, &self.entities);
                let Some(map) = maps.get_mut(map_id) else {
                    break;
                };
                map.move_object(
                    crate::map::MoveRequest {
                        from_x: x,
                        from_y: y,
                        actor_id,
                        to_x: nx,
                        to_y: ny,
                        ignore_block: false,
                        add_time: self.clock_ms,
                        // `CheckDupObj` 只对玩家生效 ⇒ cert 的 `CellType` 恒 `Play`。
                        cert_cell_type: crate::map::CellType::Play,
                    },
                    |cid| {
                        entities.get(cid).is_some_and(|o| {
                            !o.ghost && !o.death && !o.fixed_hide_mode && !o.ob_mode
                        })
                    },
                )
            };
            if !moved {
                break;
            }
            x = nx;
            y = ny;
            if let Some(e) = self.entities.get_mut(actor_id) {
                e.x = x;
                e.y = y;
                // `:874-877 if (Race >= ActorRace.Animal) WalkTick = WalkTick + 800;`
                if race >= crate::entity::ACTOR_RACE_ANIMAL {
                    e.ai.walk_tick += 800;
                }
            }
            // `:871 SendRefMsg(RM_PUSH, nBackDir, CurrX, CurrY, 0, "")` —— 逐格一条。
            out.push(crate::action::PushEvent {
                actor_id,
                dir: back,
                x,
                y,
            });
        }
        // `:890-895 Dir = nBackDir; if (result == 0) Dir = olddir;`
        let final_dir = if out.is_empty() { old_dir } else { back };
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.ai.dir = final_dir;
        }
        out
    }

    /// 玩家 `BaseObject.Run` 的等价路径：`PlayObject.Run()` 末尾无条件 `base.Run()`
    /// （`PlayObject.Message.cs:765`），其中的 `VerifyTick` 段
    /// （`BaseObject.Base.cs:223-228`）对每个在场玩家每 30s 调一次
    /// [`World::verify_map_time`]。
    ///
    /// oracle 的玩家 `Run` 由 `WorldServer.cs:697` 每 ~200ms 驱动；本骨架玩家没有独立的
    /// `Run` 方法 ⇒ 在 [`World::tick`] 的 AOI 段之前补这一趟（`race == Play && !Ghost`，
    /// `Ghost` 玩家已从 `PlayObjectList` 移除、不再 Run，见 `WorldServer.cs:708-711`）。
    /// 30s 门在 [`World::verify_map_time`] 内 ⇒ 常态每 tick 只是一次整数比较。
    fn verify_players_map_time(&mut self) {
        let ids: Vec<i32> = self
            .entities
            .iter()
            .filter(|e| e.race == crate::entity::ACTOR_RACE_PLAY && !e.ghost)
            .map(|e| e.id)
            .collect();
        for id in ids {
            self.verify_map_time(id);
        }
    }

    /// 进图：实体落进 `map_id` 的格子（对应 `Envirnoment.AddMapObject`）。
    pub fn enter_world(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        // **落格类型按类分派**：`BaseObject.Initialize()`（`BaseObject.Base.cs:18`）调
        // `AddToMap()`，而 `CellType` 是对象自己的属性（`AnimalObject.cs:86`
        // `CellType = CellType.Monster`；`PlayObject` = `Play`；`NormNpc/Merchant` = `Merchant`）。
        // oracle `CreateMonster` 也显式用 `CellType.Monster` 登记（`WorldServer.MonGen.cs:788`）。
        // 之前这里恒用 `Play` ⇒ 玩家 AOI 的 `baseObject.CellType == CellType.Monster`
        // 距离门（`PlayObject.cs:3131`）对刚落图的怪永不命中。
        let cell_type = entity_cell_type(e);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let now = self.clock_ms;
        if !map.add_map_object(x, y, cell_type, actor_id, now) {
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

    /// `BaseObject.DisappearA()` 的格子面（`BaseObject.cs:1519-1522`）：
    /// `Envir.DeleteFromMap(CurrX, CurrY, CellType, ActorId, this)`。
    ///
    /// 只摘格、**不删实体**（oracle 对象仍被 `MonGen.CertList` 持有，可复活）。帧面
    /// `SendRefMsg(RM_DISAPPEAR)` 由 AOI 的视野差集在下一 tick 表现（本函数不产生帧记录）。
    /// oracle 的 `DisappearA` 不改 `AddToMaped`；本仓用 `add_to_mapped` 表示「已落格」，
    /// 摘格后同步清位以保持不变量（登记：非逐字段等价，仅本仓内部状态位）。
    pub fn disappear_a(&mut self, actor_id: i32) -> bool {
        let Some(e) = self.entities.get(actor_id) else {
            return false;
        };
        let (x, y, map_id) = (e.x, e.y, e.map_id);
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        let (cell, success) = map.get_cell_info_mut(x, y);
        let mut removed = false;
        if success {
            let before = cell.count();
            cell.obj_list
                .retain(|o| !(o.actor_object && o.cell_obj_id == actor_id));
            if cell.count() == 0 {
                cell.clear();
            }
            removed = cell.count() != before;
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = false;
        }
        removed
    }

    /// Ghost 5 分钟门后释放实体（`WorldServer.MonGen.cs:320-333` 的 `CertList.RemoveAt`
    /// + 对象失引用）：摘格 + 从实体仓库移除。不消费随机数。
    pub fn release_entity(&mut self, actor_id: i32) -> bool {
        self.disappear_a(actor_id);
        self.entities.remove(actor_id).is_some()
    }

    /// 在指定图/格落位（先摘旧格）。`CellType` 取 [`entity_cell_type`]（怪 = `Monster`）。
    fn place_actor_at(&mut self, actor_id: i32, x: i32, y: i32, now: i64) -> bool {
        self.disappear_a(actor_id);
        let Some(e) = self.entities.get_mut(actor_id) else {
            return false;
        };
        let map_id = e.map_id;
        let cell_type = entity_cell_type(e);
        // oracle `CurrX/CurrY` 在 `AddMapObject` 之前就已写入（`BaseObject.cs:2674-2676`）。
        e.x = x;
        e.y = y;
        let Some(map) = self.maps.get_mut(map_id) else {
            return false;
        };
        if !map.add_map_object(x, y, cell_type, actor_id, now) {
            return false;
        }
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.add_to_mapped = true;
        }
        true
    }

    /// `Envirnoment.CanWalk(x, y, boFlag)` 的等价物。
    ///
    /// `ignore_actors = true`（oracle `boFlag` 支）：只判 `cellSuccess && Valid`；
    /// `false`：再排除格上的活体（`!Ghost && !Death && !FixedHideMode && !ObMode`）。
    pub(crate) fn can_walk_at(&self, map_id: usize, x: i32, y: i32, ignore_actors: bool) -> bool {
        let Some(map) = self.maps.get(map_id) else {
            return false;
        };
        let (cell, success) = map.get_cell_info(x, y);
        if !success {
            return false;
        }
        if ignore_actors {
            return true;
        }
        for obj in &cell.obj_list {
            if !obj.actor_object {
                continue;
            }
            if let Some(e) = self.entities.get(obj.cell_obj_id) {
                if !e.ghost && !e.death && !e.fixed_hide_mode && !e.ob_mode {
                    return false;
                }
            }
        }
        true
    }

    /// `BaseObject.ReAliveEx(MonGenInfo)`（`BaseObject.cs:2466-2750`）的已建模子集。
    ///
    /// 已建模：字段复位（`Gold=0`/`ShowHp=false`/`StoneMode=false`/`FixedHideMode=false`/
    /// `HP=MaxHP`/`MP=MaxMP`/`Death=false`/`Invisible=false`），`Animal` 的肉质重掷与
    /// race 51/52/53/95/96/97 的 switch 抽数、race 54 的 `Animal=true`，
    /// `monGen.X/Range` 位置重抽（2 抽）、`CanWalk(...,true)` + 回退循环（含
    /// `Random(Width/2)`/`Random(Height/2)` 抽数）、`AddToMap`/`AddMapObject`、
    /// `RM_TURN` 帧（[`crate::mongen::ReviveOutcome::frames`]）。
    ///
    /// **未建模（登记）**：`WAbil=Abil`/`RecalcAbilitys()`（本仓怪属性静态）、`NoItem`/
    /// `Skeleton`/各专用怪标志复位（`MagicMonster.DupMode`/`WallStructure.SetMapFlaged` 等）、
    /// `CharStatus/StatusChanged()`、`OnEnvirnomentChanged` 的换图清位（本仓无怪换图路径 ⇒
    /// 恒不触发）、`MonsterSayMessage(null, MonStatus.MonGen)`（oracle 对
    /// `monsterObject == null` 直接返回，`AnimalObject.cs:280-283` ⇒ 无帧）、
    /// `GetFeatureToLong()` 的怪物 `CharDesc`（展示层，本仓怪物帧面未建模）。
    ///
    /// 返回 `None`：实体不存在或地图不存在（oracle `if (Envir == null) return false;`）。
    pub fn revive_monster(
        &mut self,
        actor_id: i32,
        mon_gen_x: i16,
        mon_gen_y: i16,
        range: u8,
        now: i64,
    ) -> Option<crate::mongen::ReviveOutcome> {
        let (race, animal_now, map_id) = self.entities.get(actor_id).map(|e| {
            (
                e.race,
                e.ai.animal || crate::monai::is_animal_race(e.race),
                e.map_id,
            )
        })?;
        let (width, height) = {
            let map = self.maps.get(map_id)?;
            (map.width, map.height)
        };
        // 1) `if (Animal) { MeatQuality = Random(3500) + 3000; }`（`BaseObject.cs:2568-2570`）。
        let mut meat_quality = None;
        if animal_now {
            meat_quality = Some(self.rng.random_below(3500) + 3000);
        }
        // 2) `switch (Race)`（`BaseObject.cs:2572-2620`）的抽数；169/170 只改 StickMode（未建模，无抽）。
        match race {
            51 => meat_quality = Some(self.rng.random_below(3500) + 3000),
            52 => {
                meat_quality = Some(if self.rng.random_below(30) == 0 {
                    self.rng.random_below(20000) + 10000
                } else {
                    self.rng.random_below(8000) + 8000
                });
            }
            53 => meat_quality = Some(self.rng.random_below(8000) + 8000),
            95 => {
                self.rng.random_below(2);
            }
            96 => {
                self.rng.random_below(4);
            }
            97 => {
                self.rng.random_below(2);
            }
            _ => {}
        }

        // 3) 字段复位（`BaseObject.cs:2468-2479` + `:2678-2687`）。
        if let Some(e) = self.entities.get_mut(actor_id) {
            e.death = false;
            // `BaseObject.Die`（`BaseObject.Base.cs:318-345`）**每次**死亡都无条件置 `Death`/`DeathTick`，
            // 复活后 `CanReAlive` 仍为 true ⇒ oracle 允许「死→复活→再死」无限循环。本仓的死亡结算是
            // 一次性闩锁（`settle_deaths` 的 `!death_settled` 过滤），复活必须把它清掉，否则第二次死亡
            // 永不结算（复验 F1：复活即免死）。
            e.death_settled = false;
            e.invisible = false;
            e.ghost = false;
            e.hp = e.max_hp;
            e.mp = e.max_mp;
            e.gold = 0;
            e.show_hp = false;
            e.stone_mode = false;
            e.fixed_hide_mode = false;
            e.re_alive_tick = now;
            if race == 54 {
                // `case 54: Animal = true; break;`（`BaseObject.cs:2601-2603`）。
                e.ai.animal = true;
            }
            if let Some(q) = meat_quality {
                e.ai.meat_quality = u16::try_from(q).unwrap_or(u16::MAX);
            }
        }

        // 4) 位置重抽（`BaseObject.cs:2640-2641`）。
        let span = i32::from(range) * 2 + 1;
        let mut n_x = i32::from(mon_gen_x) - i32::from(range) + self.rng.random_below(span);
        let mut n_y = i32::from(mon_gen_y) - i32::from(range) + self.rng.random_below(span);
        let mut frames = Vec::new();
        let mut error_on_init = true;
        if self.can_walk_at(map_id, n_x, n_y, true) && self.place_actor_at(actor_id, n_x, n_y, now)
        {
            error_on_init = false;
            // `AddToMap()` 内部的 `RM_TURN`（`BaseObject.cs:1370-1378`；刚清 `FixedHideMode` ⇒ 恒发）。
            // 注：oracle **无论 `AddMapObject` 成败都发**该帧；本仓只在落位成功时记帧——因
            // `can_walk_at(...,true)` 成立即蕴含 `cell_match && Valid`（`map.rs`），`AddMapObject`
            // 失败在此分支不可达（复验 F3），故无行为差。
            if let Some(e) = self.entities.get(actor_id) {
                frames.push(crate::mongen::ReviveFrame {
                    dir: e.ai.dir,
                    x: e.x,
                    y: e.y,
                });
            }
        }
        // `nX2/nY2` 在首次落位尝试**之后**取（`BaseObject.cs:2684-2685`）。
        let x2 = self.entities.get(actor_id).map_or(0, |e| e.x);
        let y2 = self.entities.get(actor_id).map_or(0, |e| e.y);

        if error_on_init {
            let n_range = if width < 50 { 2 } else { 3 };
            let n_range2 = if height < 250 {
                if height < 30 {
                    2
                } else {
                    20
                }
            } else {
                50
            };
            let mut n_c = 0;
            let mut add_obj = false;
            loop {
                if !self.can_walk_at(map_id, n_x, n_y, false) {
                    if width - n_range2 - 1 > n_x {
                        n_x += n_range;
                    } else {
                        n_x = self.rng.random_below(width / 2) + n_range2;
                    }
                    if height - n_range2 - 1 > n_y {
                        n_y += n_range;
                    } else {
                        n_y = self.rng.random_below(height / 2) + n_range2;
                    }
                } else {
                    // 直接 `Envir.AddMapObject`（不经 `AddToMap` ⇒ 无 `RM_TURN`）。
                    add_obj = self.place_actor_at(actor_id, n_x, n_y, now);
                    break;
                }
                n_c += 1;
                if n_c > 46 {
                    break;
                }
            }
            if !add_obj {
                // `CurrX = nX2; CurrY = nY2; Envir.AddMapObject(...)`（直接调用 ⇒ 无 `RM_TURN`）。
                self.place_actor_at(actor_id, x2, y2, now);
            }
        }

        // 5) 尾部 `SendRefMsg(RM_TURN, Dir, CurrX, CurrY, GetFeatureToLong(), "")`
        //    （`BaseObject.cs:2747`）——无条件发一条。
        let e = self.entities.get(actor_id)?;
        frames.push(crate::mongen::ReviveFrame {
            dir: e.ai.dir,
            x: e.x,
            y: e.y,
        });
        Some(crate::mongen::ReviveOutcome { actor_id, frames })
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
        // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`。
        e.verify_tick = self.clock_ms;
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
        // 专用怪 ctor 的非抽数字段（`ViewRange`/`SearchTick`/`StickMode`；
        // `BeeQueen.cs:13-21`、`SpiderHouseMonster.cs:13-21`、`Clone.cs:9-12`）。
        crate::special_monster::apply_ctor_fields(&mut e, self.clock_ms);
        // `BaseObject` ctor `AutoRecoveryTick = GetTickCount()`（`BaseObject.cs:445`）：
        // 恢复段从实体落库那一刻起算。
        e.auto_recovery_tick = self.clock_ms;
        // `BaseObject.cs:443` ctor `PoisoningTick = GetTickCount()`：同上，按落图时刻重盖（buff 轴）。
        e.poisoning_tick = self.clock_ms + BUFF_EPOCH_MS;
        // `BaseObject.cs:444` ctor `VerifyTick = GetTickCount()`：同上。
        e.verify_tick = self.clock_ms;
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
            let (map_id, gold, mon_exp, exp_hitter, group_owner, is_play) = (
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
                // `BaseObject.cs:323`：`Death = true; DeathTick = HUtil32.GetTickCount();`
                e.death_tick = self.clock_ms;
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
            // **`openmir2-player-drop-useitems`**：玩家死亡 `DropUseItems`
            // （`PlayObject.Message.cs:2152-2228`）。oracle 的触发链是
            // `Die()` → `KillFunc()` → `SendMsg(RM_DIEDROPITEM)` → `Operate` →
            // `DieDropItems`（`BaseObject.cs:2437-2464`）；本骨架在死亡结算点执行
            // ——同点（死亡结算）、同次（每次死亡恰一次），相位差见 T-81。
            //
            // 三道**产生**门（缺一不抽）：
            // ① `KillFunc` 的 `ExpHitter != null && (ExpHitter.Master != null || ExpHitter.Race == Play)`；
            // ② `DieDropItems` 的 `!Envir.Flag.FightZone && !Envir.Flag.Fight3Zone`；
            // ③ `!Animal`（玩家恒 false）。
            if is_play {
                // `KillFunc`（`BaseObject.cs:2431-2445`）是两个**独立** if：
                // `ExpHitter.Master != null` 发一次、`ExpHitter.Race == Play` 再发一次。
                // 两条件同时成立时 oracle 会执行**两次** `DropUseItems`；本骨架的 Play 实体
                // 不置 `Master`（`PlayCloneObject` ctor 也不置）⇒ 实际互斥，单次调用与
                // oracle 等价（相位/触发点登记见 T-81）。
                let hitter_is_actor =
                    exp_hitter
                        .and_then(|h| self.entities.get(h))
                        .is_some_and(|h| {
                            h.master.is_some() || h.race == crate::entity::ACTOR_RACE_PLAY
                        });
                let fight_zone = self.maps.get(map_id).is_some_and(|m| m.is_fight_zone());
                let animal = self.entities.get(id).is_some_and(|x| x.ai.animal);
                if hitter_is_actor && !fight_zone && !animal {
                    let deleted = crate::death_drop::drop_use_items_on_death(self, id);
                    if !deleted.is_empty() {
                        record.death_use_item_drops.push((id, deleted));
                    }
                }
                // **第二段 `DropUseItems`（oracle `KillTargetTrigger`）**：
                // 击杀者（`ExpHitter` 或其主人）是玩家时，它处理
                // `RM_PLAYERKILLMONSTER`（`PlayObject.cs:1632`）的尾部会再调
                // `killObject.DropUseItems(0)`（`:1787-1798`）。部署件
                // `KillByHumanDropUseItem=1` ⇒ PvP 死亡**两次**逐槽抽数（第二次的删除列表
                // 恒空：第一段已把 `ItemDesc&8` 清空）。
                // `PlayObject.cs:1787-1793` 的前置与 `!NoItem || !Envir.Flag.NoDropItem`
                // 都取 **`this`**——即处理 `RM_PLAYERKILLMONSTER` 的 PlayObject：
                // `ExpHitter.Master` 存在时是主人（`KillFunc` 把消息发给 Master），
                // 否则是 `ExpHitter` 本人。**不是**死者（`killObject`）。
                let kill_trigger_id =
                    exp_hitter.and_then(|h| self.entities.get(h).map(|h| h.master.unwrap_or(h.id)));
                let kill_trigger = kill_trigger_id.and_then(|k| self.entities.get(k));
                let no_item = kill_trigger.is_some_and(|k| k.no_item);
                let killer_map_id = kill_trigger.map(|k| k.map_id);
                let killer_map = killer_map_id.and_then(|m| self.maps.get(m));
                let map_no_drop = killer_map.is_some_and(|m| m.no_drop_item());
                let killer_fight_zone = killer_map.is_some_and(|m| m.is_fight_zone());
                // `:1788-1793 AttackBaseObject = killObject.Master ?? killObject`——human/monster
                // 门由**死者侧**的 `AttackBaseObject.Race` 选（不是击杀者；死者是 Play 但
                // `Master` 非 Play 时仍走 `KillByMonstDropUseItem`）。
                let attack_base_race_play = match self.entities.get(id).and_then(|e| e.master) {
                    None => true,
                    Some(m) => self
                        .entities
                        .get(m)
                        .is_none_or(|e| e.race == crate::entity::ACTOR_RACE_PLAY),
                };
                let kill_gate = if attack_base_race_play {
                    self.kill_by_human_drop_use_item
                } else {
                    self.kill_by_monst_drop_use_item
                };
                if hitter_is_actor && !killer_fight_zone && (!no_item || !map_no_drop) && kill_gate
                {
                    let _ = crate::death_drop::drop_use_items_on_death(self, id);
                }
            }
            record.deaths.push((id, placed, gold, exp_hitter, gained));
            // **MB-2（R43）**：`ZilKinZombi.Die()` 覆写（`ZilKinZombi.cs:24-33`）——在 `base.Die()`
            // 之后、`if (ZilKillCount > 0)` 时抽 `(Random(20)+4)*1000`，随后**无条件** `-= 1`。
            // 抽数落在 `crates/world/src/monai.rs`（`monster_ai` 组），本处只是调用点。
            if !is_play {
                crate::monai::zil_kin_zombi_die(self, id);
            }
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
    /// `PlayObject.IsGroupMember`（`PlayObject.cs:5158-5175`）的注册表源：**团长**
    /// （`GroupOwner`）实体持有的 `GroupMembers`；`GroupOwner == 0` ⇒ 空表。
    ///
    /// 本仓只有团长 Entity 持成员表（`party.rs:542-544/624`；`JoinGroup` 只给加入者写
    /// `group_owner`，`party.rs:322`）⇒ **非团长施法者自身的 `group_members` 恒空**。
    /// `HAM_GROUP` 的 `IsProperTarget` 必须读这张表（`PlayObject.Attack.cs:686-697` 调
    /// `IsGroupMember`），不能读施法者自己的——否则同组玩家会被误判为合法目标
    /// （多抽、白扣护身符、638/642 而非恰 1 条 639）。
    #[must_use]
    pub fn attacker_group_members(&self, attacker: &Entity) -> Vec<i32> {
        if attacker.group_owner == 0 {
            return Vec::new();
        }
        self.entities
            .get(attacker.group_owner)
            .map(|e| e.group_members.clone())
            .unwrap_or_default()
    }

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
        let attacker_group_members = self.attacker_group_members(&attacker);
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
            // **G-06 耐久面回写（`openmir2-persist-useitems-wiring`）**：`attack_dir_grouped_poison`
            // 在 `target` 的副本上跑 `PlayObject.StruckDamage` 的耐久段（`PlayObject.cs:2954-3036`），
            // 只回写 `hp` 会把磨损/破零整段丢掉（实体槽永远停在受击前）。oracle 改的是会话
            // `UseItems` 对象本身 ⇒ 这里必须把 `use_items` 一并写回实体。
            e.use_items = target.use_items;
            // 命中者登记的**折叠**：C# 在 `_Attack` 里对 `Race > ActorRace.Play` 的目标
            // `SendMsg(RM_STRUCK)`（`BaseObject.Attack.cs:61-64`），由目标在自己的 `Run()` 里
            // `SetLastHiter(攻击方)`（`AnimalObject.cs:118-124`）。这里在结算之后立刻登记：
            // 本次 `_Attack` 期间的 `StruckDamage` 仍看到旧的 `LastHiter`，与 C# 同序。
            if target.race > crate::entity::ACTOR_RACE_PLAY {
                // `CloneMonster.Operate` 覆盖了 `RM_STRUCK` 的 `base.Operate`
                // （`Clone.cs:14-30` 恒 `return false`）⇒ `AnimalObject.Operate` 的
                // `SetLastHiter` 不会执行。普通怪照旧。
                if target.race != crate::special_monster::RACE_CLONE_MONSTER {
                    e.set_last_hiter(actor_id, now);
                }
                // `targetObject.SendMsg(targetObject, RM_STRUCK, nPower, …)`（`BaseObject.Attack.cs:61-64`）
                // 的**入队等价物**：`SendMsg` 只在 `!Ghost` 时入队（`BaseObject.Message.cs:152-183`）。
                // `wParam = nPower`（护甲减免后的实际伤害）随队携带，供 `CloneMonster` 扣主人 MP。
                // 出队处理在目标的 `base.Run`（[`crate::monai::base_object_run_monster`]）。
                if !e.ghost {
                    e.ai.pending_struck.push(crate::entity::StruckMsg {
                        hiter: actor_id,
                        // oracle `SendMsg(target, RM_STRUCK, nPower, …)`（`BaseObject.Attack.cs:61-64`）
                        // 的 `nPower` = `GetHitStruckDamage` 之后（`struck_power`），不是倍率后的
                        // `damage`；默认配置下两者相等。
                        w_param: i32::from(outcome.struck_power),
                    });
                }
            }
        }
        // **卡 `openmir2-rm-refmessage-delay-layer`**：`_Attack` 的
        // `targetObject.SendStruckDelayMsg(RM_REFMESSAGE, nPower, target.WAbil.HP,
        // target.WAbil.MaxHP, ActorId, "", 200)`（`BaseObject.Attack.cs:57`）。
        // `nPower` = `GetHitStruckDamage` 之后（`struck_power`），门为 `> 0`。
        // 入队零抽数；到点由 `deliver_self_delay_msgs` 的 `RM_REFMESSAGE` 支处理
        // （广播框架 + `Race != Play` 自投 `RM_STRUCK` → `Struck` 的 `Random(6)`/`Random(300)`）。
        if outcome.struck_power > 0 {
            self.send_struck_delay_msg_refmessage(
                target_id,
                actor_id,
                i32::from(outcome.struck_power),
                crate::monai::ATTACK_STRUCK_DELAY_MS,
            );
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
    /// （`PlayObject.cs:1911-1913`）。本方法据此返回 `moved == false` 并回滚，宿主因此不发 `SM_WALK`。
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
            // **MB-1**：`(!mapRoute.Envir.Flag.boNEEDHOLE) ||
            // EventMgr.GetEvent(Envir, CurrX, CurrY, ET_DIGOUTZOMBI) != null`——事件查的是
            // **玩家当前图/当前格**（`Envir`/`CurrX/CurrY`），不是目标图/目标格。
            if self.maps.get(hop.dst_map_id).is_some_and(|m| m.need_hole())
                && !self.dig_out_event_at(map_id, x, y)
            {
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
            // oracle `PlayObject.cs:1878-1881`：`if (!EnterAnotherMap(...)) { result = false; }`
            // ⇒ **任何** `EnterAnotherMap` 失败都必须置 `result=false`，不只看前置门。
            // 目标越界时 `CellValid` 返回 **true**（`Envirnoment.cs:307-314`）⇒ 前置门放行，
            // 由 `AddMapObject` 的 `CellMatch` 失败返回 false（`PlayObject.cs:3779-3791`）
            // ⇒ 本仓对应 `switch_map` 的 `Err(CellRejected)`（已在其中回滚旧图旧格，`df5a260ea`）。
            // `NoSuchMap`/`NotInWorld` 是 Rust 独有护栏（oracle `EnterAnotherMap` 无对应错误；
            // 目标图由 route 表校验、会话由宿主建立）⇒ 按「失败」保守处理，与 `result=false` 同支。
            match self.switch_map(&chr_name, hop.dst_map_id, hop.dst_x, hop.dst_y) {
                Ok(()) => switched = true,
                Err(_) => failed = true,
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
        let cert_cell_type = entity_cell_type(e);
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
                cert_cell_type,
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

    /// **出站接线片 W2**：`Command::Push` → `BaseObject.CharPushed`
    /// （`src/M2Server/Actor/BaseObject.cs:854-897`）——委托给逐句实现
    /// [`World::char_pushed_now`]（含 `Dir` 复原与 `WalkTick += 800`；帧面在宿主）。
    fn push_now(&mut self, actor_id: i32, n_dir: u8, count: u8) -> Vec<crate::action::PushEvent> {
        self.char_pushed_now(actor_id, n_dir, count)
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

    /// **卡 `openmir2-player-searchviewrange-cadence` 判据 (a) + (b) 的严格大于边界**。
    ///
    /// oracle 证据：
    /// - `WorldServer.cs:663-667`：`if ((GetTickCount() - playObject.SearchTick) > playObject.SearchTime)`
    ///   `{ playObject.SearchTick = GetTickCount(); playObject.SearchViewRange(); }`（严格大于）；
    /// - `PlayObject.cs:899-901`：ctor 里 `SearchTime = 1000; SearchTick = GetTickCount();`
    ///   （覆盖 `BaseObject.cs:441` 的 `Random(2000)+2000`）；
    /// - `BaseObject.cs:442`：ctor `SearchTick = GetTickCount()`。
    ///
    /// 世界 tick = 200ms ⇒ 1200ms 才命中（1000ms 不满足 `>`），周期 1200ms。
    /// 用「离开世界后 `visible_actors` 仍残留」观测**没有重扫**——若每 tick 都扫，
    /// 残留会在下一 tick 立即消失。
    #[test]
    fn player_search_view_range_throttled_and_strictly_greater() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(Entity::new(0, "p", 10, 10));
        let q = w.add_entity(Entity::new(0, "q", 11, 10));

        // 玩家 ctor 覆盖值：必须是常量 1000（tooth ③ 会打掉这条并在失败信息里打印实际值）。
        let st = w.entities.get(p).unwrap().ai.search_time;
        assert_eq!(
            st, 1000,
            "玩家 SearchTime 必须是 1000（PlayObject.cs:900），实际 {st}"
        );

        // (a) 200ms × 5 = 恰好 1000ms —— 严格大于 ⇒ 一次都不扫。
        for _ in 0..5 {
            w.tick();
            assert!(
                w.entities.get(p).unwrap().visible_actors.is_empty(),
                "clock={}ms：玩家不得扫描（SearchTime=1000，必须严格大于）",
                w.clock_ms
            );
            assert_eq!(w.entities.get(p).unwrap().ai.search_tick, 0);
        }
        // (b) 恰 1000ms：仍不扫。
        assert_eq!(w.clock_ms, 1000);
        assert!(
            w.entities.get(p).unwrap().visible_actors.is_empty(),
            "恰 1000ms 不满足 `> 1000` ⇒ 不得扫描"
        );

        // 第 6 tick（1200ms > 1000）⇒ 扫一次，q 进入视野。
        w.tick();
        assert_eq!(w.clock_ms, 1200);
        assert_eq!(
            w.entities.get(p).unwrap().ai.search_tick,
            1200,
            "命中后 SearchTick = now"
        );
        assert!(
            w.entities
                .get(p)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == q),
            "1200ms 必须扫到 q"
        );

        // q 离开世界：1400ms 的 tick 不得重扫 ⇒ p 的可见集仍残留 q。
        w.leave(q);
        w.tick(); // 1400ms
        assert_eq!(w.clock_ms, 1400);
        assert_eq!(w.entities.get(p).unwrap().ai.search_tick, 1200);
        assert!(
            w.entities
                .get(p)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == q),
            "1400ms 距上次扫描仅 200ms ⇒ 不得重扫"
        );
        // 直到 2200ms（= 1200 + 1000）都不满足 `>` ⇒ 仍不扫。
        for _ in 0..4 {
            w.tick();
            assert!(
                w.entities
                    .get(p)
                    .unwrap()
                    .visible_actors
                    .iter()
                    .any(|v| v.target == q),
                "clock={}ms：2200ms 之前不得重扫",
                w.clock_ms
            );
        }
        assert_eq!(w.clock_ms, 2200);
        // 2400ms（距上次 1200ms > 1000）⇒ 重扫并移除已离开世界的 q。
        w.tick();
        assert_eq!(w.clock_ms, 2400);
        assert_eq!(w.entities.get(p).unwrap().ai.search_tick, 2400);
        assert!(
            !w.entities
                .get(p)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == q),
            "2400ms 必须重扫（q 已离开世界 ⇒ 从视野移除）"
        );
    }

    /// 判据 (c)：同一 `SearchTime` 窗口内的连续 tick 不重复扫（`SearchTick` 只在命中时推进）。
    #[test]
    fn player_search_view_range_does_not_rescan_within_window() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(Entity::new(0, "p", 10, 10));
        w.add_entity(Entity::new(0, "q", 11, 10));

        for _ in 0..6 {
            w.tick(); // 1200ms 首次命中
        }
        assert_eq!(w.entities.get(p).unwrap().ai.search_tick, 1200);

        // 同窗口内重复推进（1400/1600/1800/2000/2200）：SearchTick 不动。
        for _ in 0..5 {
            w.tick();
            assert_eq!(
                w.entities.get(p).unwrap().ai.search_tick,
                1200,
                "clock={}ms：同一 SearchTime 窗口内不得重扫",
                w.clock_ms
            );
        }
        assert_eq!(w.clock_ms, 2200);
        // 窗口外的第一 tick（2400ms）才推进。
        w.tick();
        assert_eq!(w.entities.get(p).unwrap().ai.search_tick, 2400);
    }

    /// 判据 (d)：死亡玩家走 `search_view_range_death`（清空视野，不刷新 `SearchTick`），
    /// ghost 玩家不扫描（oracle `WorldServer.cs:708-711` 已把 Ghost 移出 `PlayObjectList`）。
    #[test]
    fn dead_and_ghost_players_do_not_scan() {
        let mut w = World::new(32, 32);
        let p = w.add_entity(Entity::new(0, "p", 10, 10));
        let q = w.add_entity(Entity::new(0, "q", 11, 10));
        for _ in 0..6 {
            w.tick(); // 1200ms 首次命中
        }
        assert!(w
            .entities
            .get(p)
            .unwrap()
            .visible_actors
            .iter()
            .any(|v| v.target == q));

        // 死亡：走既有 death 支（清空视野），不是 scan ⇒ SearchTick 保持 1200。
        w.entities.get_mut(p).unwrap().death = true;
        w.tick(); // 1400ms
        assert!(
            w.entities.get(p).unwrap().visible_actors.is_empty(),
            "死亡玩家的 search_view_range_death 必须清空 visible_actors（语义不变）"
        );
        assert_eq!(
            w.entities.get(p).unwrap().ai.search_tick,
            1200,
            "死亡清空不是扫描，不得刷新 SearchTick"
        );
        for _ in 0..10 {
            w.tick();
        }
        assert!(
            w.entities.get(p).unwrap().visible_actors.is_empty(),
            "死亡玩家不得扫描"
        );

        // ghost：从进图起推进 6 tick（>1000ms）也不得扫。
        let g = w.add_entity(Entity::new(0, "g", 20, 20));
        let q2 = w.add_entity(Entity::new(0, "q2", 21, 20));
        w.entities.get_mut(g).unwrap().ghost = true;
        let g_tick = w.entities.get(g).unwrap().ai.search_tick;
        for _ in 0..6 {
            w.tick();
        }
        assert!(
            w.entities.get(g).unwrap().visible_actors.is_empty(),
            "ghost 玩家不得扫描（clock={}ms）",
            w.clock_ms
        );
        assert_eq!(
            w.entities.get(g).unwrap().ai.search_tick,
            g_tick,
            "ghost 跳过不得刷新 SearchTick"
        );
        assert!(
            w.entities.get(q2).is_some(),
            "q2 仍在世界（仅是观察者不可见）"
        );
    }

    // ---------------------------------------------------------------------------------------
    // 卡 `openmir2-monster-view-throttle`（tier=R）：怪物视野由玩家 `RM_UPDATEVIEWRANGE`
    // 投递并按 `SearchTime` 节流；非玩家只有 `IsSlave || Race in {11,112,254}` 自扫。
    // oracle：`PlayObject.cs:3129-3139`、`MonsterObject.cs:136-185`、`WorldServer.MonGen.cs:300-313`。
    // ---------------------------------------------------------------------------------------

    /// **生产口径玩家**：`PlayObject` ctor `WantRefMsg = true`（`PlayObject.cs:889`）。
    /// `Entity::new` 的缺省 `false` 是构造等价物的空缺，宿主 `new_player_entity` 会置 true。
    fn mview_player(w: &mut World, x: i32, y: i32, view_range: i32) -> i32 {
        let mut e = Entity::new(0, "p", x, y);
        e.race = ACTOR_RACE_PLAY;
        e.want_ref_msg = true;
        e.view_range = view_range;
        w.add_entity(e)
    }

    /// **普通野怪（被动视野）**：`walk_speed = MAX` 钉住位置，避免 `Wondering` 移动干扰断言。
    fn mview_monster(w: &mut World, x: i32, y: i32, view_range: i32, search_time: i64) -> i32 {
        let mut e = Entity::new(0, "m", x, y);
        e.race = ACTOR_RACE_MONSTER;
        e.view_range = view_range;
        e.ai.search_time = search_time;
        e.ai.walk_speed = i64::MAX;
        w.add_entity(e)
    }

    fn mview_visible(w: &World, id: i32) -> Vec<(i32, u8)> {
        w.entities
            .get(id)
            .unwrap()
            .visible_actors
            .iter()
            .map(|v| (v.target, v.flag as u8))
            .collect()
    }

    /// **判据①：普通怪不再每 tick 全扫**（oracle `WorldServer.MonGen.cs:300-313`）。
    ///
    /// 玩家 `ViewRange(12) - 怪 ViewRange(12) = 0` ⇒ dx=1 不满足距离门 ⇒ 不投递；
    /// 若怪仍每 tick 全扫，`baseObject.Race == Play` 会让它在第一 tick 就看见玩家。
    /// 用 `SearchTick` 恒 0 + 可见集恒空断言“从未自扫”。
    ///
    /// 牙：恢复 AOI pass 的「所有非玩家每 tick 全扫」⇒ 可见集非空 ⇒ 必红。
    #[test]
    fn passive_monster_never_self_scans() {
        let mut w = World::new(32, 32);
        let p = mview_player(&mut w, 11, 10, 12);
        let m = mview_monster(&mut w, 10, 10, 12, 0);
        for _ in 0..10 {
            w.tick();
        }
        assert!(
            w.entities.get(m).unwrap().visible_actors.is_empty(),
            "普通怪不得自扫：{:?}",
            mview_visible(&w, m)
        );
        assert_eq!(
            w.entities.get(m).unwrap().ai.search_tick,
            0,
            "普通怪的自扫路径不得推进 SearchTick"
        );
        // 阴性对照：玩家自己的扫描确实跑了（看见怪）——否则本条不构成「怪不自扫」的证据。
        assert!(
            w.entities
                .get(p)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == m),
            "玩家扫描必须看见怪（want_ref=true）"
        );
    }

    /// **判据②+⑦：投递在怪物自己的 `base.Run()` 消费；`SearchTime` 严格大于门。**
    ///
    /// 时间线（200ms tick）：玩家 1200ms 首次扫描 ⇒ 入队；怪 1400ms 消费
    /// （`1400 - 0 > 1000`）⇒ 加入可见集并 `SearchTick = 1400`。玩家下次 2400ms 扫描
    /// ⇒ 入队；怪 2600ms 消费（`2600 - 1400 = 1200 > 1000`）⇒ 已存在项打 `Invisible`。
    #[test]
    fn monster_view_update_consumed_at_search_time_boundary() {
        let mut w = World::new(32, 32);
        w.enable_monster_ai();
        let p = mview_player(&mut w, 11, 10, 12);
        let m = mview_monster(&mut w, 10, 10, 5, 1000);

        for _ in 0..6 {
            w.tick(); // 1200ms：玩家扫描末尾投递
        }
        assert_eq!(
            w.entities.get(m).unwrap().ai.pending_view_updates,
            vec![p],
            "1200ms：玩家扫描必须投递 RM_UPDATEVIEWRANGE"
        );
        assert!(
            w.entities.get(m).unwrap().visible_actors.is_empty(),
            "1200ms：怪自己还没 Run，消息不得提前消费"
        );
        assert_eq!(w.entities.get(m).unwrap().ai.search_tick, 0);

        w.tick(); // 1400ms：monster_ai_tick 的 base.Run 消息段消费
        assert!(w
            .entities
            .get(m)
            .unwrap()
            .ai
            .pending_view_updates
            .is_empty());
        assert_eq!(mview_visible(&w, m), vec![(p, 2)], "Show = 2");
        assert_eq!(w.entities.get(m).unwrap().ai.search_tick, 1400);
        assert!(
            w.entities.get(m).unwrap().is_visible_active,
            "收到玩家 ⇒ IsVisibleActive = true"
        );

        for _ in 0..5 {
            w.tick(); // 2400ms：玩家第二次扫描（2400-1200>1000）
        }
        assert_eq!(
            w.entities.get(m).unwrap().ai.search_tick,
            1400,
            "2400ms：怪未到下一次 Run 消费，SearchTick 不动"
        );
        w.tick(); // 2600ms
        assert_eq!(w.entities.get(m).unwrap().ai.search_tick, 2600);
        assert_eq!(
            mview_visible(&w, m),
            vec![(p, 1)],
            "已在列表 ⇒ UpdateMonsterVisible 打 Invisible(1)，不重复追加"
        );
    }

    /// **判据①：节流窗口内的投递被丢弃；窗口外的投递生效。**
    ///
    /// `SearchTime = 2000`：1400ms 消费 `1400 - 0 <= 2000` ⇒ 丢弃（`SearchTick` 不推进）；
    /// 2600ms 消费 `2600 - 0 > 2000` ⇒ 生效。
    #[test]
    fn monster_view_update_dropped_within_search_time() {
        let mut w = World::new(32, 32);
        w.enable_monster_ai();
        let p = mview_player(&mut w, 11, 10, 12);
        let m = mview_monster(&mut w, 10, 10, 5, 2000);

        for _ in 0..6 {
            w.tick(); // 1200ms 投递
        }
        w.tick(); // 1400ms 消费：1400 <= 2000 ⇒ 丢弃
        assert!(
            w.entities
                .get(m)
                .unwrap()
                .ai
                .pending_view_updates
                .is_empty(),
            "消息已出队（oracle 先判门再更新）"
        );
        assert!(
            w.entities.get(m).unwrap().visible_actors.is_empty(),
            "节流窗口内不得更新可见集"
        );
        assert_eq!(
            w.entities.get(m).unwrap().ai.search_tick,
            0,
            "节流丢弃时 oracle 不写 SearchTick（return 在赋值之前）"
        );

        for _ in 0..5 {
            w.tick(); // 2400ms 玩家再次投递
        }
        w.tick(); // 2600ms：2600-0 > 2000 ⇒ 生效
        assert_eq!(mview_visible(&w, m), vec![(p, 2)]);
        assert_eq!(w.entities.get(m).unwrap().ai.search_tick, 2600);
    }

    /// **判据①：同一 tick 的第二个玩家投递被同一 `SearchTime` 窗口丢弃**（oracle 单怪单队列）。
    ///
    /// 两个玩家同 tick 扫描 ⇒ 队列 `[p1, p2]`；怪在自己的 Run 里按入队序消费：
    /// p1 通过并 `SearchTick = now`；p2 随即命中 `<= SearchTime` ⇒ 丢弃。
    #[test]
    fn second_player_update_dropped_in_same_tick() {
        let mut w = World::new(32, 32);
        w.enable_monster_ai();
        let p1 = mview_player(&mut w, 11, 10, 12);
        let p2 = mview_player(&mut w, 9, 10, 12);
        let m = mview_monster(&mut w, 10, 10, 5, 0);

        for _ in 0..6 {
            w.tick(); // 1200ms：p1、p2 按槽位序投递
        }
        assert_eq!(
            w.entities.get(m).unwrap().ai.pending_view_updates,
            vec![p1, p2]
        );
        w.tick(); // 1400ms：第一个通过，第二个被 SearchTime=0 丢弃
        assert_eq!(mview_visible(&w, m), vec![(p1, 2)], "只接受第一个投递");
        assert_eq!(w.entities.get(m).unwrap().ai.search_tick, 1400);
    }

    /// **判据③：距离门精确边界（差 1 格）通过侧**——`|Δx| = ViewRange(12) - ViewRange(5) = 7`。
    /// 失败侧（8 格）在 `aoi.rs::player_scan_view_update_distance_gate_exact_boundary`。
    #[test]
    fn player_update_reaches_monster_at_exact_view_range_span() {
        let mut w = World::new(32, 32);
        w.enable_monster_ai();
        let p = mview_player(&mut w, 10, 10, 12);
        let m = mview_monster(&mut w, 17, 10, 5, 0); // dx = 7 == 12 - 5
        for _ in 0..6 {
            w.tick();
        }
        assert_eq!(w.entities.get(m).unwrap().ai.pending_view_updates, vec![p]);
        w.tick(); // 1400ms
        assert_eq!(
            mview_visible(&w, m),
            vec![(p, 2)],
            "恰好 7 格必须通过距离门"
        );
    }

    /// **判据④：`IsVisibleActive` 置位条件**——玩家或宝宝（`Master != null`）触发；
    /// 普通怪/无主怪不触发（`MonsterObject.cs:145-149`）。
    ///
    /// 由 `aoi.rs::monster_view_update_sets_visible_active_for_player_and_baby` 直接钉住
    /// 消费函数；此处在世界路径上确认玩家投递 ⇒ `true`（见上一条）。
    ///
    /// **判据⑤：跨图不投递**——玩家 `SearchViewRange` 只扫自己地图的格子。
    #[test]
    fn player_update_does_not_cross_maps() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        let _p = mview_player(&mut w, 10, 10, 12);
        let mut e = Entity::new(0, "m", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.view_range = 5;
        e.ai.search_time = 0;
        e.ai.walk_speed = i64::MAX;
        e.map_id = 1;
        let m = w.add_entity(e);

        for _ in 0..8 {
            w.tick();
        }
        assert!(
            w.entities
                .get(m)
                .unwrap()
                .ai
                .pending_view_updates
                .is_empty(),
            "异图不得投递 RM_UPDATEVIEWRANGE"
        );
        assert!(w.entities.get(m).unwrap().visible_actors.is_empty());
        assert_eq!(
            w.entities.get(m).unwrap().ai.search_tick,
            0,
            "普通怪不自扫、无异图投递 ⇒ SearchTick 恒 0"
        );
    }

    /// **判据⑥：死亡目标 vs Ghost 目标**。
    ///
    /// oracle `PlayObject.SearchViewRange` **没有 `Death` 门**（`PlayObject.cs:3125-3129`）
    /// ⇒ 尸体仍可见、仍投递 `RM_UPDATEVIEWRANGE`（`:3130-3139`），且死者 `BaseObject.Run`
    /// 的消息段照常出队（`BaseObject.Base.cs:26-30`）。`Ghost` 则在过滤器里被排除
    /// （`!baseObject.Ghost`），且 `ProcessMonsters` 整只跳过 Ghost 怪。
    #[test]
    fn dead_monster_gets_update_but_ghost_monster_does_not() {
        // 死亡怪：投递 + 消费。
        let mut w = World::new(32, 32);
        w.enable_monster_ai();
        let p = mview_player(&mut w, 11, 10, 12);
        let m = mview_monster(&mut w, 10, 10, 5, 0);
        w.entities.get_mut(m).unwrap().death = true;
        for _ in 0..6 {
            w.tick();
        }
        assert_eq!(
            w.entities.get(m).unwrap().ai.pending_view_updates,
            vec![p],
            "死亡目标仍应收到 RM_UPDATEVIEWRANGE（PlayObject 无 Death 门）"
        );
        w.tick(); // 1400ms：死者 base.Run 消息段照常出队
                  // 消费证据用 `SearchTick`（`UpdateMonsterVisible` 唯一留下的可观测状态）：本仓死亡支
                  // 仍在同一 tick 末尾跑 `search_view_range_death`（清空 `visible_actors` 并复位
                  // `is_visible_active`，既有近似、本片不动）⇒ 列表/标志在 tick 结束时已被清掉。
        assert!(w
            .entities
            .get(m)
            .unwrap()
            .ai
            .pending_view_updates
            .is_empty());
        assert_eq!(
            w.entities.get(m).unwrap().ai.search_tick,
            1400,
            "死者收到并消费了 RM_UPDATEVIEWRANGE（SearchTime=0 门通过）"
        );

        // Ghost 怪：玩家过滤器 `!Ghost` 排除 ⇒ 不投递。
        let mut w2 = World::new(32, 32);
        mview_player(&mut w2, 11, 10, 12);
        let g = mview_monster(&mut w2, 10, 10, 5, 0);
        w2.entities.get_mut(g).unwrap().ghost = true;
        for _ in 0..6 {
            w2.tick();
        }
        assert!(
            w2.entities
                .get(g)
                .unwrap()
                .ai
                .pending_view_updates
                .is_empty(),
            "Ghost 目标不得进入玩家可见集/投递"
        );
        assert!(w2.entities.get(g).unwrap().visible_actors.is_empty());
    }

    /// **判据①：非玩家自扫集合 = `IsSlave || Race in {11,112,254}`**，且用同一
    /// `SearchTime` 严格大于门（`WorldServer.MonGen.cs:300-313`）。
    ///
    /// 卫士（race 11）`SearchTime = 1000`：1000ms 不扫、1200ms 扫；宝宝（`is_slave`）
    /// `SearchTime = 0`：200ms 即扫。
    #[test]
    fn guard_and_slave_self_scan_on_their_own_cadence() {
        let mut w = World::new(32, 32);
        let mut e = Entity::new(0, "guard", 10, 10);
        e.race = 11;
        e.view_range = 5;
        e.ai.search_time = 1000;
        e.ai.walk_speed = i64::MAX;
        let g = w.add_entity(e);
        let p = mview_player(&mut w, 11, 10, 12);

        for _ in 0..5 {
            w.tick(); // 1000ms
        }
        assert!(
            w.entities.get(g).unwrap().visible_actors.is_empty(),
            "1000ms：1000 - 0 > 1000 不成立 ⇒ 卫士不得自扫"
        );
        assert_eq!(w.entities.get(g).unwrap().ai.search_tick, 0);
        w.tick(); // 1200ms
        assert!(
            w.entities
                .get(g)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == p),
            "1200ms：1200 - 0 > 1000 ⇒ 卫士自扫看见玩家"
        );
        assert_eq!(w.entities.get(g).unwrap().ai.search_tick, 1200);

        // 宝宝（race 80 + is_slave）：200ms 即自扫（SearchTime = 0）。
        let mut w2 = World::new(32, 32);
        let mut e = Entity::new(0, "slave", 10, 10);
        e.race = ACTOR_RACE_MONSTER;
        e.is_slave = true;
        e.view_range = 5;
        e.ai.search_time = 0;
        e.ai.walk_speed = i64::MAX;
        let s = w2.add_entity(e);
        let p2 = mview_player(&mut w2, 11, 10, 12);
        w2.tick(); // 200ms
        assert!(
            w2.entities
                .get(s)
                .unwrap()
                .visible_actors
                .iter()
                .any(|v| v.target == p2),
            "宝宝（is_slave）必须在自己 SearchTime 门后自扫"
        );
        assert_eq!(w2.entities.get(s).unwrap().ai.search_tick, 200);
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
        // 玩家 `SearchTime=1000`（`PlayObject.cs:900`）⇒ 200ms tick 下首次扫描在第 6 tick。
        for _ in 0..6 {
            w.tick();
        }
        let vis = crate::aoi::visible_snapshot(&w.entities, 1);
        let targets: Vec<i32> = vis.iter().map(|(t, _)| *t).collect();
        assert!(targets.contains(&2), "dx=ViewRange 必须可见（闭区间）");
        assert!(!targets.contains(&3), "dx=ViewRange+1 不可见");
    }

    #[test]
    fn aoi_scan_order_is_x_outer_y_inner() {
        let mut w = world_with(&[(1, 30, 30), (10, 33, 32), (11, 34, 31)]);
        // 玩家 `SearchTime=1000` ⇒ 首次扫描在第 6 tick（1200ms，严格大于 1000）。
        for _ in 0..6 {
            w.tick();
        }
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

    /// **切图成功语义**（oracle `PlayObject.EnterAnotherMap` 的正向支，`PlayObject.cs:3772-3783`）：
    /// 目标格可走 ⇒ 旧图摘除、新图落格、`add_to_mapped` 保持 true、`Session.map_id` 更新、
    /// 记一条 `MapChange`（`space_move_show == None`）。
    ///
    /// 红检（变异牙）：把 `switch_map_inner` 的成功支改成不记 `MapChange` ⇒ 本测必红；
    /// 把旧图摘除删掉 ⇒ 旧格计数断言必红。
    #[test]
    fn switch_map_success_lands_new_map_and_reports_map_change() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        assert_eq!(w.map(0).cell(5, 4).count(), 1, "进图先落旧格");

        w.switch_map("hero", 1, 7, 9).expect("目标格可走 ⇒ 成功");

        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (1, 7, 9), "实体字段切到新图新坐标");
        assert!(e.add_to_mapped, "成功落格 ⇒ add_to_mapped = true");
        assert!(
            e.visible_actors.is_empty(),
            "切图后视野列表整体失效（oracle 清 Visible*）"
        );
        assert_eq!(w.map(0).cell(5, 4).count(), 0, "旧图格已摘除");
        assert!(
            w.map(1)
                .cell(7, 9)
                .obj_list
                .iter()
                .any(|o| o.actor_object && o.cell_obj_id == hero),
            "新图格登记了该玩家"
        );
        assert_eq!(w.session("hero").unwrap().map_id, 1, "会话 map_id 更新");
        let ch = w.sessions.map_changes();
        assert_eq!(ch.len(), 1, "成功恰记一条 MapChange");
        assert_eq!(
            (
                ch[0].actor_id,
                ch[0].map_id,
                ch[0].x,
                ch[0].y,
                ch[0].space_move_show
            ),
            (hero, 1, 7, 9, None)
        );
    }

    /// **切图失败语义 · 目标格墙**（oracle `PlayObject.EnterAnotherMap` 失败支，
    /// `PlayObject.cs:3787-3791`）：`AddToMap()` 返回 false ⇒
    /// `Envir = oldEnvir; CurrX = nOldX; CurrY = nOldY; Envir.AddMapObject(...)`。
    /// 断言重点是**旧图格登记真的恢复**（只把实体字段改回去不算数）：
    /// 实体仍在旧图旧坐标、`add_to_mapped == true`、旧格 `ObjList` 里能找回该玩家，
    /// 且不记 `MapChange`（本仓只在成功落地时记）。
    ///
    /// 红检（变异牙 ①）：把失败支改回直接 `return Err(CellRejected)`（不回滚）⇒
    /// `map_id/坐标` 与旧格登记断言必红；
    /// 红检（变异牙 ②）：只回滚实体字段、不调旧图 `add_map_object` ⇒ 旧格登记断言必红。
    #[test]
    fn switch_map_wall_cell_rolls_back_to_old_map_and_cell() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        // 目标图 (7,9) → LowWall（oracle CellValid 失败 ⇒ AddToMap 失败）。
        assert!(w.maps[1].set_map_xy_flag(7, 9, false));
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();
        assert_eq!(w.map(0).cell(5, 4).count(), 1);

        let err = w.switch_map("hero", 1, 7, 9).unwrap_err();
        assert_eq!(
            err,
            crate::session::SessionError::CellRejected,
            "oracle 返回 false；本仓用 CellRejected 表达（返回前已回滚）"
        );

        let e = w.entities.get(hero).unwrap();
        assert_eq!(
            (e.map_id, e.x, e.y),
            (0, 5, 4),
            "回滚旧图旧坐标（oracle :3788-3789）"
        );
        assert!(e.add_to_mapped, "旧格重新落格 ⇒ add_to_mapped 恢复 true");
        assert_eq!(w.session("hero").unwrap().map_id, 0, "会话 map_id 仍是旧图");
        assert!(
            w.map(0)
                .cell(5, 4)
                .obj_list
                .iter()
                .any(|o| o.actor_object && o.cell_obj_id == hero),
            "**旧图格登记必须恢复**（oracle `Envir.AddMapObject`，:3790）"
        );
        assert_eq!(
            w.map(0).cell(5, 4).count(),
            1,
            "旧格恰一个对象（无重复登记）"
        );
        assert!(
            !w.map(1)
                .cell(7, 9)
                .obj_list
                .iter()
                .any(|o| o.actor_object && o.cell_obj_id == hero),
            "目标墙格不得留下玩家登记"
        );
        assert!(w.sessions.map_changes().is_empty(), "失败不发换图簇");
    }

    /// 同上，目标坐标**越界**：oracle `CellValid` 对越界返回 `true`（`Envirnoment.cs:307-314`
    /// 的方向）⇒ `EnterAnotherMap` 的门放行，随后 `AddMapObject` 的 `CellMatch` 失败
    /// （`Envirnoment.cs:258-261`）⇒ 走**同一回滚支**（旧图旧坐标 + 重落旧格）。
    ///
    /// 红检（变异牙）：失败支不回滚 ⇒ 旧格登记与坐标断言必红。
    #[test]
    fn switch_map_out_of_bounds_cell_rolls_back_to_old_map_and_cell() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();

        let err = w.switch_map("hero", 1, 99, 99).unwrap_err();
        assert_eq!(
            err,
            crate::session::SessionError::CellRejected,
            "越界落格失败 = oracle AddMapObject false"
        );

        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "回滚旧图旧坐标");
        assert!(e.add_to_mapped, "重落旧格");
        assert!(
            w.map(0)
                .cell(5, 4)
                .obj_list
                .iter()
                .any(|o| o.actor_object && o.cell_obj_id == hero),
            "旧图格登记必须恢复"
        );
        assert!(w.sessions.map_changes().is_empty());
    }

    /// 目标**图**不存在：`Env` 模型里没有此分支（oracle 的 `envir` 恒有效），这是本仓护栏。
    /// 它在**任何状态变更之前**返回 `NoSuchMap` ⇒ 角色仍留在旧图旧格（旧格登记不丢）。
    ///
    /// 红检（变异牙）：把 `map_id >= maps.len()` 检查移到旧图摘除之后 ⇒ 本测旧格/坐标断言必红。
    #[test]
    fn switch_map_unknown_map_leaves_actor_in_old_cell() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
        w.open_session("hero");
        let hero = w
            .enter_map("hero", Entity::new(0, "hero", 5, 4), 0)
            .unwrap();

        let err = w.switch_map("hero", 2, 3, 3).unwrap_err();
        assert_eq!(err, crate::session::SessionError::NoSuchMap);

        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "未动旧图旧坐标");
        assert!(e.add_to_mapped);
        assert!(
            w.map(0)
                .cell(5, 4)
                .obj_list
                .iter()
                .any(|o| o.actor_object && o.cell_obj_id == hero),
            "旧图格登记保持不变"
        );
        assert!(w.sessions.map_changes().is_empty());
    }

    /// **卡 `openmir2-enter-map-cell-rejected-vs-oracle`·墙格进图**：进图落在**墙格**时，
    /// oracle `BaseObject.Base.cs:14-23` `Initialize()` 只「不进格表」而**不摘除角色**
    ///（`AddtoMapSuccess` 保持 `true` = 未落图；`PlayObject.UserLogon` 之后无条件继续）⇒
    /// `enter_map` 必须返回 `Ok`（**不是** `Err(CellRejected)`）、保留实体、`add_to_mapped == false`、
    /// 会话照常进世界。
    ///
    /// 红检：把 `enter_map` 的落格失败支改回 `entities.remove(id) + Err(CellRejected)` ⇒ 本用例必红。
    #[test]
    fn enter_map_wall_cell_keeps_actor_unmapped() {
        let mut w = World::with_maps(vec![(16, 16)]);
        // 只把 (3,3) 置成 `HighWall`（`set_cell_attributes` 列主序 `x*H+y`、fail-closed）。
        let mut attrs = vec![crate::map::CellAttribute::Walk; 16 * 16];
        attrs[3 * 16 + 3] = crate::map::CellAttribute::HighWall;
        assert_eq!(w.maps[0].set_cell_attributes(&attrs), 16 * 16);
        assert_eq!(
            w.maps[0].cell_attribute(3, 3),
            Some(crate::map::CellAttribute::HighWall)
        );

        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 3, 3);
        e.race = ACTOR_RACE_PLAY;
        let id = w
            .enter_map("hero", e, 0)
            .expect("墙格进图 = oracle Initialize 语义（保留角色，不进格表），不是 Err");

        assert!(
            w.entities.get(id).is_some(),
            "oracle Initialize 落格失败不移除角色 ⇒ 实体必须还在"
        );
        assert!(
            !w.entities.get(id).unwrap().add_to_mapped,
            "未进格表 ⇒ add_to_mapped 必须为 false"
        );
        assert_eq!(w.map(0).cell(3, 3).count(), 0, "墙格不得落对象");
        let sess = w.session("hero").unwrap();
        assert_eq!(sess.stage, crate::session::WorldStage::InWorld);
        assert_eq!(sess.actor_id, Some(id));
    }

    /// 同上，坐标**越界**：oracle `CanWalk(x,y,true)`（经 `GetCellInfo`）对越界同样返回 `false`
    /// ⇒ `Initialize()` 同样只「不落图」不移除角色 ⇒ `enter_map` 返回 `Ok`、实体保留、`add_to_mapped=false`。
    #[test]
    fn enter_map_out_of_bounds_keeps_actor_unmapped() {
        let mut w = World::with_maps(vec![(16, 16)]);
        w.open_session("hero");
        let mut e = Entity::new(0, "hero", 99, 99);
        e.race = ACTOR_RACE_PLAY;
        let id = w
            .enter_map("hero", e, 0)
            .expect("越界落格 = oracle Initialize 语义（保留角色），不是 Err");
        assert!(w.entities.get(id).is_some(), "越界落格不移除角色");
        assert!(!w.entities.get(id).unwrap().add_to_mapped, "越界不入格表");
        assert_eq!(
            w.session("hero").unwrap().stage,
            crate::session::WorldStage::InWorld
        );
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
            crate::map::CellType::Play,
        );

        assert_eq!(res, steps, "成功步数 = 事件数");
        assert_eq!((wx, wy), (x, y), "末端坐标一致");
        assert_eq!(dir, crate::magic::DIR_DOWN, "result>0 ⇒ Dir 落 nBackDir");
    }

    /// **StickMode 门（`MagicManager.cs:34`）**：`!target.StickMode` 在 `Random(20)` **之前**
    /// ⇒ StickMode 目标既不被推、也**零抽**（漏门会平移整条 RNG 流）。
    ///
    /// 红检：删掉 `cast_firewind_spell` 里的 `|| target.stick_mode` ⇒ `result=1`、有事件、
    /// RNG 流前进 ⇒ 三条断言全红。
    #[test]
    fn firewind_stick_mode_target_is_skipped_without_drawing() {
        let mut w = World::new(64, 64);
        let caster = w.add_entity(player(1, "caster", 10, 10, 60));
        let mut sticky = player(2, "sticky", 11, 10, 1);
        sticky.stick_mode = true;
        let sticky = w.add_entity(sticky);
        {
            let e = w.entities.get_mut(caster).unwrap();
            e.visible_actors.push(crate::entity::VisibleBaseObject {
                target: sticky,
                flag: crate::entity::VisibleFlag::Show,
            });
        }
        let before = format!("{:?}", w.rng);
        let (result, events) = w.cast_firewind_spell(caster, 3);
        assert_eq!(result, 0, "StickMode ⇒ 不调 CharPushed、result 不 ++");
        assert!(events.is_empty(), "StickMode ⇒ 无逐格 RM_PUSH");
        assert_eq!(
            format!("{:?}", w.rng),
            before,
            "StickMode ⇒ Random(20) 零抽"
        );
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
    /// （`BaseObject.cs:685-688`），因 `if (result)` 不成立而**不发移动帧**（`:1911-1913`）
    /// ⇒ 本方法返回 `moved == false`。
    ///
    /// **为何原先写反了**：旧断言写 `moved == true`（"走一格仍成功"），是把 `step_with_route`
    /// 当成"移动与路线检查互不影响"来看——漏了 oracle 的 `Walk()` 返回值是**整条走命令的结果**，
    /// 失败会连同位移一起回滚。验证者给的真实证据（`BaseObject.cs:666-673` 的回滚三步 + `:1911-1913`
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

    /// **`EnterAnotherMap` 对越界目标坐标的行为**：`Envirnoment.CellValid` 对**越界返回 true**
    /// （`Envirnoment.cs:307-314`）⇒ oracle 仍进 `EnterAnotherMap`，由 `AddMapObject` 的 `CellMatch`
    /// 失败返回 false（`PlayObject.cs:3779-3791`）⇒ `PlayObject.Walk` 的 `result=false`
    /// （`:1878-1881`）⇒ 上游 `WalkTo` 回滚坐标（`BaseObject.cs:685-688`）、不发移动帧
    /// （`:1911-1913`）。本仓对应 `switch_map` 的 `Err(CellRejected)`；本片修的是「该 `Err` 必须
    /// 置 `failed`」——否则会被误判成 `Skipped`（`moved=true`、不回滚），与 oracle 分叉。
    ///
    /// 红检（变异牙 ①）：把 `switch_map` 结果分支改回 `is_ok() { switched = true }`（`Err` 不置
    /// `failed`）⇒ 本测 `moved`/坐标/`MapChange` 断言必红（越界被当成正常走位）。
    #[test]
    fn step_with_route_out_of_bounds_dest_rolls_back() {
        let mut w = World::with_maps(vec![(16, 16), (16, 16)]);
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
                dst_y: 99, // 越界：cell_valid 返回 true，落地 CellMatch 失败
            },
        }]);
        assert!(
            w.map(1).cell_valid(7, 99),
            "oracle CellValid 对越界返回 true（Envirnoment.cs:307-314）"
        );
        let out = w.step_with_route(hero, 0, 1, 1);
        assert!(
            !out.moved,
            "EnterAnotherMap 失败 ⇒ Walk()=false ⇒ 位移回滚、不发移动帧"
        );
        assert!(!out.changed_map, "越界目标落格失败 ⇒ 不换图");
        let e = w.entities.get(hero).unwrap();
        assert_eq!((e.map_id, e.x, e.y), (0, 5, 4), "坐标回滚到走前格");
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
    /// **B2-1**：`mag_objects_in_range` 的顺序契约 = x 外层升序 → y 内层升序 → 格内
    /// **ObjList 插入序**（`BaseObject.cs:2817-2832`）；过滤 `Death`/`Ghost`，且只认
    /// `CellObjId>0 && ActorObject`。
    ///
    /// 红检：把内层/外层循环互换、或改用 `EntityStore::iter` 槽位序 ⇒ 断言必红。
    #[test]
    fn p3b2_mag_objects_in_range_order_is_x_then_y_then_objlist() {
        let mut w = World::new(32, 32);
        let mut mk = |name: &str, x: i32, y: i32, race: u8, ghost: bool, death: bool| {
            let mut e = Entity::new(0, name.to_string(), x, y);
            e.race = race;
            e.ghost = ghost;
            e.death = death;
            w.add_entity(e)
        };
        // 同格双对象：插入序必须可观测。
        let a = mk("a", 10, 10, ACTOR_RACE_PLAY, false, false);
        let b = mk("b", 10, 10, ACTOR_RACE_MONSTER, false, false);
        // y 更大的格子（x 相同时按 y 升序）。
        let c = mk("c", 10, 11, ACTOR_RACE_PLAY, false, false);
        // x 更小的格子（内层 y 走完才轮到下一个 x）。
        let d = mk("d", 9, 11, ACTOR_RACE_PLAY, false, false);
        let dead = mk("dead", 9, 9, ACTOR_RACE_MONSTER, false, true);
        let ghost = mk("ghost", 11, 9, ACTOR_RACE_MONSTER, true, false);
        w.tick(); // Enter 落格（cell.obj_list 插入序 = add_entity 序）
        let got = w.mag_objects_in_range(0, 10, 10, 1);
        assert_eq!(
            got,
            vec![d, a, b, c],
            "x=9 的 d 先于 x=10 的 a/b/c；同格 a 先于 b；dead/ghost 被过滤"
        );
        assert!(!got.contains(&dead) && !got.contains(&ghost));
    }

    /// **B2-2**：`get_mag_struck_damage` = `MAX(0, nPower - (Lo(MAC)+Random(Δ+1)))`；
    /// `baseObject == null`（`RM_MAGSTRUCK` 的真实调用）不加 undead 项；`Some` 才加。
    ///
    /// 红检：把 `Random(Δ+1)` 写成 `Random(Δ)`（Δ=0 时不抽）⇒ 对照流状态不等。
    #[test]
    fn p3b2_get_mag_struck_damage_mac_roll_and_null_undead() {
        use mir2_shared::rng::RandomNumber;
        let mac: u16 = 0x0A00; // Lo=0, Hi=10 ⇒ Random(11)
        let seed = 42;
        let mut rng = RandomNumber::with_seed(seed);
        let draw = rng.random_below(11);
        let want = (20 - draw).max(0);
        let mut rng2 = RandomNumber::with_seed(seed);
        assert_eq!(
            crate::combat::get_mag_struck_damage(&mut rng2, mac, 20, None),
            want
        );
        assert_eq!(
            format!("{rng2:?}"),
            format!("{:?}", {
                let mut r = RandomNumber::with_seed(seed);
                let _ = r.random_below(11);
                r
            }),
            "MSD 恰 1 抽 R(11)"
        );
        // undead 项只在 baseObject != null（Some）时生效。
        let mut rng3 = RandomNumber::with_seed(seed);
        assert_eq!(
            crate::combat::get_mag_struck_damage(&mut rng3, mac, 20, Some(7)),
            want + 7
        );
    }

    /// **B1**：`SKILL_LIGHTENING(11)` 的不死系 ×1.5（`MagicManager.cs:296`）。
    /// 期望值 = **手写推导**：`MPow` → `GetPower` → `GetAttackPower`（玩家 override）→ `Round(x*1.5)`。
    /// 同时钉住返回值口径（`bo_train = Race >= Animal`、`delay_ms = 600`、target 保留）。
    #[test]
    fn p3b1_lightening_undead_multiplier() {
        let mut w = World::new(32, 32);
        let mut c = Entity::new(0, "c".to_string(), 10, 10);
        c.race = ACTOR_RACE_PLAY;
        c.level = 5;
        let caster = w.add_entity(c);
        let mut t = Entity::new(0, "t".to_string(), 11, 10);
        t.race = ACTOR_RACE_MONSTER;
        t.level = 1;
        t.life_attrib = crate::magic_effect::LA_UNDEAD;
        let target = w.add_entity(t);
        w.tick();
        let magic = mir2_data::MagicInfo {
            power: 5,
            max_power: 10,
            def_power: 2,
            def_max_power: 8,
            train_lv: 3,
            ..Default::default()
        };
        // 骰门（AntiMagic=1）通过的固定种子。
        let seed = (1..1000)
            .find(|s| RandomNumber::with_seed(*s).random_below(10) >= 1)
            .unwrap();
        let mut control = RandomNumber::with_seed(seed);
        let _ = control.random_below(10);
        let mp_draw = control.random_below(5);
        let gp_draw = control.random_below(6);
        // `GetAttackPower(..., Hi-Lo+1)` 内部 `Random(nPower + 1)` ⇒ `Random(0+2)`。
        let atk_draw = control.random_below(2);
        let luck_draw = control.random_below(10);
        let mpow = 5 + mp_draw;
        let base = mir2_shared::hutil::round(f64::from(mpow) / 4.0 * 2.0) + 2 + gp_draw;
        // `Luck <= 0` 且 `Random(10)==0` ⇒ `result = nBasePower`（不再加 atk 抽）。
        let power = if luck_draw == 0 {
            base
        } else {
            base + atk_draw
        };
        let want = mir2_shared::hutil::round(f64::from(power) * 1.5);
        w.rng = RandomNumber::with_seed(seed);
        let out = w.cast_lightening_spell(caster, &magic, 1, Some(target), 1, 0);
        let mut probe = RandomNumber::with_seed(seed);
        let _ = probe.random_below(10);
        assert_eq!(
            out.power, want,
            "不死系威力 = Round(GetAttackPower(...) * 1.5)"
        );
        assert!(out.bo_train, "Race >= Animal ⇒ boTrain");
        assert_eq!(out.target, Some(target), "成功支保留目标");
        assert!(out.delayed);
        assert_eq!(out.delay_ms, 600);
    }

    /// **T-78**：构造一个 `PvpFlag = true` 的玩家；`pvp_name_color_tick = clock_ms - expired_by_ms`
    /// （`expired_by_ms > pk_flag_time_ms` ⇒ 一旦被检查就清位 + 一条 656 载荷）。
    fn pk_world(clock_ms: i64, expired_by_ms: i64) -> (World, i32) {
        let mut w = World::new(16, 16);
        w.clock_ms = clock_ms;
        let mut e = Entity::new(7, "p7", 10, 10);
        e.pvp_flag = true;
        e.pvp_name_color_tick = clock_ms - expired_by_ms;
        assert_eq!(w.add_entity(e), 7);
        (w, 7)
    }

    /// **T-78 修复**：`clock_ms` 被注入成墙钟量级（`MIR2_CLOCK_MS ≈ 1.7e12`）时，
    /// `pk_check_tick` 必须在**首次调用**时惰性对齐到 `clock_ms`（oracle
    /// `DecLightItemDrugTick = GetTickCount()`，`PlayObject.cs:969`）；否则
    /// `clock - 0 > 500` 恒真 ⇒ 过期检查退化为每 tick 一次。
    ///
    /// 夹具里的 `PvpFlag` 已过期：若首调就检查，会立刻清位并返回一条 656 载荷。
    ///
    /// 牙①：把惰性初始化改回「构造时 0」⇒ 首调即 `out.len() == 1`、flag 被清 ⇒ 红。
    #[test]
    fn pk_check_throttle_holds_with_injected_wall_clock() {
        const T0: i64 = 1_700_000_000_000;
        let (mut w, id) = pk_world(T0, 61_000);

        // 首调：只建立基线，不检查（过期者仍在）。
        assert!(w.check_pk_status().is_empty(), "首调不得检查");
        assert_eq!(
            w.pk_check_tick, T0,
            "游标必须惰性初始化到当前 clock_ms（不是 0）"
        );
        assert!(w.entities.get(id).unwrap().pvp_flag, "首调不得清位");

        // 500ms 内：不检查。
        w.clock_ms = T0 + 200;
        assert!(w.check_pk_status().is_empty());
        w.clock_ms = T0 + 500;
        assert!(w.check_pk_status().is_empty(), "500ms 内不得检查");
        assert!(w.entities.get(id).unwrap().pvp_flag);
        assert_eq!(w.pk_check_tick, T0);

        // 超过 500ms：检查一次 ⇒ 过期清位、恰一条 656 载荷（宿主 1:1 出帧）。
        w.clock_ms = T0 + 600;
        let out = w.check_pk_status();
        assert_eq!(out.len(), 1, "过期者恰一条 656");
        assert_eq!(out[0].target_id, id);
        assert!(!w.entities.get(id).unwrap().pvp_flag, "过期清位");
        assert_eq!(w.pk_check_tick, T0 + 500);
        // 同一时刻再调：游标已推进到 T0+500，100ms < 500ms ⇒ 不重复检查。
        assert!(w.check_pk_status().is_empty());
    }

    /// **T-78 回归**：未注入时钟（`clock_ms` 从 0 起）时，500ms 节流与修复前逐位一致
    /// （首调 0 时刻建基线；`clock - tick > 500` 才检查，严格 `>`）。
    #[test]
    fn pk_check_throttle_unchanged_when_clock_starts_at_zero() {
        let (mut w, id) = pk_world(0, 100_000); // pvp_name_color_tick = -100_000（已过期）
        assert!(w.check_pk_status().is_empty());
        assert_eq!(w.pk_check_tick, 0, "从 0 起的基线仍是 0");
        for t in [200_i64, 500] {
            w.clock_ms = t;
            assert!(w.check_pk_status().is_empty(), "t={t} 未过 500ms 不得检查");
            assert!(w.entities.get(id).unwrap().pvp_flag);
            assert_eq!(w.pk_check_tick, 0);
        }
        w.clock_ms = 600;
        let out = w.check_pk_status();
        assert_eq!(out.len(), 1, "t=600 首次检查 ⇒ 过期者恰一条 656");
        assert_eq!(out[0].target_id, id);
        assert!(!w.entities.get(id).unwrap().pvp_flag);
        assert_eq!(w.pk_check_tick, 500, "游标 += 周期（oracle 同式）");
    }

    /// **T-78 边界**：过期门是严格 `>`（oracle `:4359` `(now - PvpNameColorTick) > dwPKFlagTime`）
    /// ⇒ 恰 60s 不清、60s+1ms 清且**恰一条** 656；节流门同样严格 `>` ⇒ 恰 500ms 不检查。
    ///
    /// 牙②：把节流门改 `>=` ⇒ `T0+500` 就检查、游标被推进 ⇒ 本用例红。
    #[test]
    fn pk_check_expiry_boundary_strictly_greater_reports_once() {
        const T0: i64 = 1_700_000_000_000;
        let (mut w, id) = pk_world(T0, 0); // pvp_name_color_tick = T0
        assert!(w.check_pk_status().is_empty(), "首调只建基线");
        assert_eq!(w.pk_check_tick, T0);

        // 恰 500ms：节流门 `>` 严格 ⇒ 不检查、游标不动。
        w.clock_ms = T0 + 500;
        assert!(w.check_pk_status().is_empty());
        assert_eq!(w.pk_check_tick, T0, "恰 500ms 不得推进游标（牙②）");

        // 恰 60s：检查发生，但过期门 `>` 严格 ⇒ 不清位、零 656。
        w.clock_ms = T0 + 60_000;
        assert!(w.check_pk_status().is_empty(), "恰 60s 不得过期");
        assert!(w.entities.get(id).unwrap().pvp_flag);
        assert_eq!(w.pk_check_tick, T0 + 500, "周期门已过 ⇒ 游标推进");

        // 60s+1ms：过期 ⇒ 恰一条 656、清位。
        w.clock_ms = T0 + 60_001;
        let out = w.check_pk_status();
        assert_eq!(out.len(), 1, "恰在严格 > 后触发一次 656");
        assert_eq!(out[0].target_id, id);
        assert!(!w.entities.get(id).unwrap().pvp_flag);

        // 再调：不重复投递。
        assert!(w.check_pk_status().is_empty(), "656 不得重复");
    }

    /// `World::verify_map_time` 的 30s 门与刷新（`BaseObject.Base.cs:223-228` +
    /// `Envirnoment.cs:858-897`）。
    ///
    /// 红检：删掉 30s 门（无条件刷新）⇒ 第一次 `assert!(!w.verify_map_time(id))` 红；
    /// 删掉 `cell.obj_list[i].add_time = now` ⇒ 第二次后的 `add_time` 断言红。
    #[test]
    fn verify_map_time_gate_refreshes_cell_add_time() {
        let mut w = World::new(64, 64);
        let mut e = Entity::new(0, "mon", 10, 10);
        e.race = crate::entity::ACTOR_RACE_MONSTER;
        let id = w.add_entity(e);
        w.tick(); // clock=200，Enter 落格（add_time=200）；verify_tick 在 add_entity 时=0
        assert_eq!(w.clock_ms, 200);
        let cell_add = |w: &World| {
            w.maps[0]
                .cell(10, 10)
                .obj_list
                .iter()
                .find(|o| o.actor_object && o.cell_obj_id == id)
                .expect("格子项存在")
                .add_time
        };
        assert_eq!(cell_add(&w), 200);
        // 严格大于：`now - verify_tick == 30_000` 不动。
        w.clock_ms = 30_000;
        assert!(!w.verify_map_time(id), "等于 30s 不刷新（oracle 严格大于）");
        assert_eq!(cell_add(&w), 200);
        // > 30s ⇒ 刷新（`add_time = now`）。
        w.clock_ms = 30_200;
        assert!(w.verify_map_time(id));
        assert_eq!(cell_add(&w), 30_200);
        assert_eq!(w.entities.get(id).unwrap().verify_tick, 30_200);
        // 再过 200ms 仍不到下一个 30s 门。
        w.clock_ms = 30_400;
        assert!(!w.verify_map_time(id));
        assert_eq!(cell_add(&w), 30_200);
    }

    /// 找不到自己的 `CellObject` ⇒ `AddMapObject` 重新登记（`Envirnoment.cs:894`），
    /// 并按 `baseObject.CellType` 落格（`AnimalObject.cs:86` = Monster）。
    ///
    /// 红检：删掉 `if !bo_verify { map.add_map_object(...) }` 支 ⇒ 格子项数断言红。
    #[test]
    fn verify_map_time_reinserts_missing_cell_entry() {
        let mut w = World::new(64, 64);
        let mut e = Entity::new(0, "mon", 10, 10);
        e.race = crate::entity::ACTOR_RACE_MONSTER;
        let id = w.add_entity(e);
        w.tick();
        {
            let (cell, _) = w.maps[0].get_cell_info_mut(10, 10);
            cell.obj_list.clear();
        }
        w.entities.get_mut(id).unwrap().add_to_mapped = false;
        w.clock_ms = 30_200;
        assert!(w.verify_map_time(id));
        let cell = w.maps[0].cell(10, 10);
        assert_eq!(cell.count(), 1, "找不到自己 ⇒ 重新 AddMapObject");
        let obj = cell.obj_list[0];
        assert_eq!(obj.cell_obj_id, id);
        assert_eq!(obj.cell_type, crate::map::CellType::Monster);
        assert_eq!(obj.add_time, 30_200);
        assert!(w.entities.get(id).unwrap().add_to_mapped);
    }

    /// `Entity.verify_tick` 在落图/生成时 = 当时的 `clock_ms`（`BaseObject.cs:444`）。
    ///
    /// 红检：把 `entity.verify_tick = self.clock_ms;` 从 `add_entity` 删掉 ⇒ 恒 0 ⇒ 时钟
    /// 从 0 直接跳到 30_200 的用例会按「已过门」刷新；本用例钉住落图时刻值。
    #[test]
    fn entity_verify_tick_stamped_at_enter_time() {
        let mut w = World::new(32, 32);
        w.clock_ms = 5_000;
        let e = Entity::new(0, "p", 10, 10);
        let id = w.add_entity(e);
        assert_eq!(w.entities.get(id).unwrap().verify_tick, 5_000);
    }
}
