# Operate 面 × Rust 投递点 覆盖审计（2026-10-18）

- 审计基线：`8dea847c55df15b90c27bfd9b16fd9663b654bce`（master，detach worktree `E:/tmp/impl-opaudit`）。
- **状态基线：审计于 `8dea847c5`，按 `7e8d47426` 复述**（PR-1 已把自投递泵改为 .NET 8 `PriorityQueue`
  同构语义 ⇒ G05 关闭；G01–G04 的生产者未变，结论保持）。
- 范围：oracle `Operate` 面（`BaseObject.Operate` + 全部 override）逐消息对照 Rust 的
  `deliver_*` / 直接效果调用点，找「消息能送到、但效果体没接或接错」的缺口。
- 纪律：只读审计；未改 `.rs`/`src`/`sql`；未起服务、未占九游戏端口（外部 C# 栈占用状况仅记录、不停）。
- 行号口径：**oracle 侧逐条实测行号**；**Rust 侧只给符号/文件路径，不给行号**（按任务纪律）。
- 反例来源：`CloneMonster.Operate` 的 `RM_POISON` 漏屏蔽即本审计方法此前发现的缺陷；本件复核该总门是否仍有旁路。

---

## 0. 结论摘要

- 矩阵 **38 行**（`(ident, 目标类别)` 对），其中状态 `已实现` 19 行、`部分` 12 行、`缺失` 5 行、`不适用` 2 行
  （矩阵按 ident 组织，G05 不单列 ⇒ 38 行与 19/12/5/2 计数不因 G05 关闭而变）。
- 缺口 **12 条**：R 级 5 条、L 级 7 条。按「真机可达 × 影响」排序见 §4。
  其中 **G05 已由 `7e8d47426` 关闭** ⇒ **未闭合 11 条（R 4 / L 7）**。
- **最严重两条（R）**：
  1. **G01**：`RM_DELAYMAGIC` 在**生产宿主** `bins/game-svr::handle_spell_cmd` 的 1/5/11/13/44/45 六个魔号上
     只登记了 `SpellStrikeOutcome::{power,push}`，**没有入自投递队列** ⇒ 这六个魔号不入队，故其投递永不到达，
     这六类法术的伤害不落地（`slave::deliver_delay_magic` 在 42/50 的入队点会运行；宿主注释已自认
     「延迟结算未建模（登记）」，但它同时是效果体缺口）。
  2. **G02**：`RM_DIEDROPITEM` 对**玩家**目标的 `PlayObject.DropUseItems` 未接。oracle 对每个装备槽
     `Random(nRate)`（`PlayObject.Message.cs:2184-2189`，**槽位为 null 也抽**）并把 `ItemDesc & 8` 的装备
     置 `Index=0`；Rust `World::settle_deaths` 只折叠了怪物掉落预算，玩家死亡不抽数、不清装备。
     这既是**抽数账**（R）也是**背包状态**缺口。
- Clone 类覆盖：Rust 已有 **4 条投递路径 + 1 条入队侧**的 Clone 总门（§5），`RM_POISON` 类漏屏蔽已闭环；
  但 `RM_SPELL` 的广播入口仍未接（G11，当前不可达）。
- 泵返回值语义：只有玩家 `PlayObject.Run` 的 turbo 泵观察 `Operate` 返回值（`PlayObject.Message.cs:356-366`），
  怪物/NPC/Clone 的 `BaseObject.Run`（`BaseObject.Base.cs:25-31`）**丢弃返回值**。自投递泵的 `GetMessage`
  语义（只看队首、未到点回队并只结束本次泵）已由 `7e8d47426` 实现（**G05 关闭**）；`Operate` 返回值的
  玩家泵等价物仍未建模（R38）。

---

## 1. oracle 侧清册

### 1.1 `BaseObject.Operate`（`src/M2Server/Actor/BaseObject.Base.cs:610-777`）

完整分支表（`switch (processMsg.wIdent)`，`:616`）：

| # | ident | 行号 | 效果体（逐行实测） | 返回/泵 |
|---|---|---|---|---|
| 1 | `RM_MAGSTRUCK` / `RM_MAGSTRUCK_MINE` | `:618-686` | `:620-623` 怪且 `!RushMode && Level<50` ⇒ `WalkTick += 800 + Random(1000)`；`:624` `GetMagStruckDamage(null, nParam1)`（1 抽）；`:625-629` `StruckDamage` + `HealthSpellChanged` + `SendRefMsg(RM_STRUCK_MAG)`；`:630-671` `MonDelHptoExp` 支（玩家/PlayClone 主人 `GainExp`）；`:672-679` `Race>Play` ⇒ `Animal.MeatQuality -= dmg*1000` + `SendMsg(RM_STRUCK)`；`:681-685` `FastParalysis` ⇒ `STONE=1` | `break`→`:776 return false` |
| 2 | `RM_STRUCKEFFECT` | `:687-698` | `SendRefMsg(ActorId,wParam,nParam1…Msg)`；`ActorId==RM_STRUCK` ⇒ 再 `SendMsg(this,…)`；`FastParalysis` | 同上 |
| 3 | `RM_REFMESSAGE` | `:699-710` | `SendRefMsg(...)`；`ActorId==RM_STRUCK && Race!=Play` ⇒ `SendMsg(RM_STRUCK,…)`（自入队 → `AnimalObject.Operate`）；`FastParalysis` | 同上 |
| 4 | `RM_DELAYMAGIC` | `:711-729` | 解 `wParam`/`nParam1(lo/hi)`/`nParam2(rage)`/`nParam3(target)`；`GetMagStruckDamage(this,nPower)`（1 抽，**非 null 攻方**）；命中且距离门 ⇒ `SetTargetCreat` + `Race>=Animal` 时 `nPower=Round(nPower/1.2)` + `target.SendMsg(this,RM_MAGSTRUCK,0,nPower,0,0)` | 同上 |
| 5 | `RM_RANDOMSPACEMOVE` | `:730-732` | `MapRandomMove(Msg, wParam)` | 同上 |
| 6 | `RM_DELAYPUSHED` | `:733-739` | 解 `nParam3` 目标；`CharPushed((byte)wParam,(byte)nParam2)`（`BaseObject.cs:854-894`） | 同上 |
| 7 | `RM_POISON` | `:740-759` | 解 `nParam2` 施毒者；`IsProperTarget` ⇒ `SetTargetCreat` + Play↔Play `SetPkFlag` + `SetLastHiter`；`MakePosion(wParam,(ushort)nParam1,nParam3)`（`BaseObject.cs:1941-1973`，零 RNG） | 同上 |
| 8 | `RM_TRANSPARENT` | `:760-762` | `MagicManager.MagMakePrivateTransparent(this,(ushort)nParam1)` | 同上 |
| 9 | `RM_DOOPENHEALTH` | `:763-765` | `MakeOpenHealth()`（`BaseObject.cs:944-950`） | 同上 |
| 10 | `RM_DIEDROPITEM` | `:766-768` | `DieDropItems(wParam)`（`BaseObject.cs:2450-2464`） | 同上 |
| — | `default` | `:769` | 无 | `return false`（`:776`） |

### 1.2 全部 `override bool Operate`（全仓 11 个，逐条实测）

| # | 类 | 文件:行号区间 | 处理的 ident | 返回语义 | 对消息泵的影响 |
|---|---|---|---|---|---|
| O1 | `AnimalObject` | `src/M2Server/Actor/AnimalObject.cs:114-140` | `RM_STRUCK`（`:118-133`：`SetLastHiter`/`Struck`/`BreakHolySeizeMode`/主人 `SetPkFlag`/`MonsterSayMessage`）；`RM_MAKEHOLYSEIZEMODE`（`:134-136`：`OpenHolySeizeMode(wParam)`） | 这两条 `return true`；其余 `base.Operate`→false | 怪泵（`BaseObject.Run`）丢弃；只有 PlayClone/Robot 继承的玩家泵会观察 |
| O2 | `MonsterObject` | `src/M2Server/Monster/MonsterObject.cs:177-185` | `RM_UPDATEVIEWRANGE`（`:179-183`：`UpdateMonsterVisible(wParam)`） | `true`；其余 base→false | 丢弃 |
| O3 | `BeeQueen` | `src/M2Server/Monster/Monsters/BeeQueen.cs:33-45` | `RM_ZEN_BEE`（`:35-43`：`RegenMonsterByName(Config.Bee)` + `SetTargetCreat` + `BeeList.Add`） | 处理完仍 `return base.Operate`（`:44`）→false | 丢弃 |
| O4 | `SpiderHouseMonster` | `src/M2Server/Monster/Monsters/SpiderHouseMonster.cs:32-49` | `RM_ZEN_BEE`（`:34-46`：`CanWalk(x,y+1)` + `RegenMonsterByName(Config.Spider)` + 入 `_bbList`） | 同上 `:48`→base false | 丢弃 |
| O5 | `CloneMonster` | `src/M2Server/Monster/Monsters/Clone.cs:14-35` | **任意 ident**；仅 `RM_STRUCK`/`RM_MAGSTRUCK`/`RM_SPELL`（`:16`）在 `Master != null` 时：`Master.MP<=0 ⇒ 自 HP=0`、`RM_SPELL` 扣 `nParam3`、否则扣 `wParam` | **恒 `return false`，且从不调 base**（`:34`）⇒ 屏蔽 `BaseObject` 全部 10 支 + `AnimalObject` 两支 | 丢弃（怪泵）；但**效果体被抑制**才是关键 |
| O6 | `StickMonster` | `src/M2Server/Monster/Monsters/StickMonster.cs:96-99` | 无（纯转发） | `base.Operate`→false | 丢弃 |
| O7 | `SuperGuard` | `src/M2Server/Monster/Monsters/SuperGuard.cs:58-61` | 无（纯转发） | 同上 | 丢弃 |
| O8 | `PlayCloneObject` | `src/M2Server/Player/PlayCloneObject.cs:22-25` | 无（纯转发到 `PlayObject.Operate`） | = `PlayObject.Operate` 的结果（**不是恒 false**） | `PlayClone` 走玩家 turbo 泵，返回 true 时继续排空 |
| O9 | `Trainer`（练功师） | `src/GameSrv/Npc/Trainer.cs:20-38` | `RM_STRUCK`（`:23-32`：`ActorId==ActorId` 时累加 `AttackPower`/`AttackCount` + `ProcessSayMsg`）；`RM_MAGSTRUCK`（`:33-36`） | 恒 false；**`RM_STRUCK` 不调 base**（抑制 `AnimalObject` 的 Struck 效果），只有 `RM_MAGSTRUCK` 调 base（`:35`） | `NormNpc` 泵丢弃 |
| O10 | `PlayObject` | `src/M2Server/Player/PlayObject.Message.cs:768-2126` | 215 个 case 标签：`CM_*` 73 个（`:781-1394` 64 个 + `:1397`/`:1400` 2 个 + `:2057-2075` 7 个）、`RM_*` 142 个（`:1403-2121`），尾部 `default → base.Operate`（`:2122-2124`） | 默认 `true`；`CM_BUTCH`(`:865`)/`CM_TURN`(`:1053`)/`CM_WALK`(`:1112`)/`CM_HORSERUN`(`:1161`)/`CM_RUN`(`:1215`)/`CM_*HIT` 族(`:1278`)/`CM_SITDOWN`(`:1333`)/`CM_SPELL`(`:1388`) 在「延迟重投」支置 `false`；`default` 取 base=false（`:2123`） | **玩家 turbo 泵**（`:360-366`）遇 false **break**，剩余消息留到下个 tick |
| O11 | `RobotPlayer` | `src/Modules/RobotSystem/Services/RobotPlayer.cs:1134-1179` | `RM_STRUCK`（`:1139-1168`：`SetPkFlag`/`SetLastHiter`/`Struck`/城堡守卫犯罪标记/重置 `HealthTick`/`SpellTick`/`PerHealth`/`PerSpell`/`StruckTick`） | `true`（`:1167`）；其余 `base.Operate`（= `PlayObject.Operate`） | 机器人继承玩家 turbo 泵 |

> 计数核对：M2Server 内 9 个（O1–O5、O7、O8、O10 中 `PlayObject.Message.cs`、`PlayCloneObject`）+ `Trainer.cs` + `RobotPlayer.cs` = 11。

### 1.3 消息泵与返回值语义（实测）

```text
BaseObject.Run              (BaseObject.Base.cs:25-31)
    while (GetMessage(ref msg)) Operate(msg);      // 返回值丢弃

PlayObject.Run 的消息段      (PlayObject.Message.cs:356-366)
    while (... && GetMessage(GetMessageTick, ref msg))
        if (!Operate(msg)) break;                  // ← false 停泵，剩余留到下 tick

GetMessage(ref)             (BaseObject.Message.cs:334-355)
GetMessage(msgTick, ref)    (BaseObject.Message.cs:357-378)
    队首未到点 ⇒ 以 Lowest 优先级重新入队并 return false（阻塞本条消息段）
```

- 因此 `Operate` 的 `true/false` **只对玩家系（PlayObject/PlayClone/Robot）的 turbo 泵**有意义；
  `MonsterObject`/`AnimalObject`/`NormNpc`/`Clone` 走的 `BaseObject.Run` 丢弃返回值。
- `PlayObject.Operate` 的 `default → base`（`:2122-2124`）返回 false ⇒ **任何落到基类处理的 RM_***
  （`RM_MAGSTRUCK`/`RM_POISON`/`RM_DELAYMAGIC`/`RM_TRANSPARENT`/`RM_DOOPENHEALTH`/`RM_DELAYPUSHED`/
  `RM_RANDOMSPACEMOVE`/`RM_REFMESSAGE`/`RM_STRUCKEFFECT`/`RM_DIEDROPITEM`）都会**中断该玩家本 tick 的排空**。
- `CloneMonster` 的 false 对怪泵无影响，它的意义是「**永不进 base 效果体**」。

### 1.4 关键 `ident` 的发送点（用于判定真机可达）

| ident | 发送点（oracle，实测） | 目标/载荷 |
|---|---|---|
| `RM_MAGSTRUCK` | `BaseObject.Base.cs:629/678/726`；`BaseObject.cs:912`（`SendStruckDelayMsg` 600ms）；`MagicManager.cs:1111/1138`（经 `RM_DELAYMAGIC`） | 受击者；`wParam` 物理伤害或 0，`nParam1` 威力 |
| `RM_MAGSTRUCK_MINE` | `BaseObject.cs:1562`、`PlayObject.cs:1904`、`FireBurnEvent.cs:36` | 火墙事件目标，3000ms 循环 |
| `RM_STRUCKEFFECT` | `ElfWarriorMonster.cs:65`（仅此一处，300ms） | 受击者；**发送方 race 114 属「数据不可达」集**（`plan-run-gap-batches-20261018.md` R16） |
| `RM_REFMESSAGE` | `BaseObject.Attack.cs:57/78`、`AnimalObject.cs:377`、`ArcherGuard.cs:34`、`DualAxeMonster.cs:27`、`ElectronicScolpionMon.cs:28`、`ExplosionSpider.cs:40`、`GasAttackMonster.cs:30`、`MagCowMonster.cs:27`、`SpitSpider.cs:43`、`DoubleCriticalMonster.cs:67` | 受击者；200/300/500/600/700ms |
| `RM_DELAYMAGIC` | `MagicManager.cs:183`（1/5，600ms）、`:299`（11/45，600ms）、`:338`（13，1200ms）、`:573`（45，600ms）、`:968`（200ms）、`:1004`（600ms）、`:1039`（44，600ms）、`:1239`（600ms）、`:1254`（600ms）；`BigHeartMonster.cs:34`、`CentipedeKingMonster.cs:65` | **施法者自己的队列**；`nParam3=target` |
| `RM_DELAYPUSHED` | `MagicManager.cs:1053`（仅此一处，600ms） | 施法者自己的队列；`wParam=dir`、`nParam2=push`、`nParam3=target` |
| `RM_RANDOMSPACEMOVE` | `ScriptEngine.cs:955` **整段被注释** | 不可达 |
| `RM_POISON` | `MagicManager.cs:241/245`（1000ms）、`:921/925`（1000ms）、`:1257`（650ms STONE） | 中毒目标自己的队列 |
| `RM_TRANSPARENT` | `MagicManager.cs:1221`（800ms） | 目标自己的队列 |
| `RM_DOOPENHEALTH` | `MagicManager.cs:438`（1500ms） | 目标自己的队列 |
| `RM_DIEDROPITEM` | `BaseObject.cs:2437/2442`（`KillFunc`，即时 `SendMsg` 给自己） | 死亡者自己的队列；`wParam=ExpHitter(.Master)` |
| `RM_STRUCK` | `BaseObject.Attack.cs` 的 `SendMsg(targetObject,…)`；`BaseObject.Base.cs:678`；`BaseObject.Base.cs:701-702`（`RM_REFMESSAGE` 自投递：`ActorId==RM_STRUCK && Race!=Play` 时 `SendMsg`；`AnimalObject.cs:118-133` 是消费端） | 怪物目标；`wParam=伤害` |
| `RM_MAKEHOLYSEIZEMODE` | `MagicManager.cs:712/812`（`SendMsg` 即时）、`:1168`（即时） | 目标为 `MonsterObject`/`Race>=Animal` |
| `RM_UPDATEVIEWRANGE` | `PlayObject.cs:3136`（`ActorMgr.SendMessage` 即时） | 附近怪 |
| `RM_ZEN_BEE` | `BeeQueen.cs:30`、`SpiderHouseMonster.cs:28`（`SendSelfDelayMsg` 500ms） | 产仔者自己 |
| `RM_MAGHEALING` | `MagicManager.cs:66/216`（800ms） | 目标自己的队列 |
| `RM_MASTERDIEMUTINY` | `PlayObject.cs:1996`（1000ms） | 玩家自己 |
| `RM_MASTERDIEGHOST` | `PlayObject.cs:3527`（`if (Ghost) SendSelfDelayMsg(...,1000)`，被 `BaseObject.Message.cs:190` 的 `!Ghost` 门恒拦，oracle 缺陷；`Ghost=true` 由 `PlayObject.cs:3524` 调 `BaseObject.Base.cs:492` 置位；`:2005` 是 `KickException()` 里的 `RM_MASTERDIEMUTINY`） | 玩家自己 |

---

## 2. Rust 侧投递点清册（只列符号/路径）

### 2.1 `deliver_*` 主体

| 符号 | 文件 | 处理的 ident | 生效实体 | Clone 屏蔽 |
|---|---|---|---|---|
| `World::deliver_self_delay_msgs` / `deliver_self_delay_msgs_with` | `crates/world/src/slave.rs` | `RM_MAGHEALING`、`RM_MASTERDIEMUTINY`、`RM_MASTERDIEGHOST`、`RM_TRANSPARENT`、`RM_DOOPENHEALTH`、`RM_MAGIC_LVEXP`、`RM_MAKEHOLYSEIZEMODE`、`RM_DELAYMAGIC`、`RM_POISON`、`RM_MAGSTRUCK`、`RM_ZEN_BEE`；其余 `_ => {}` **静默丢弃** | 所有入 `self_delay_msgs` 的 actor | ✅ 首判 `race == RACE_CLONE_MONSTER` ⇒ 只调 `special_monster::clone_operate` 后 `continue` |
| `World::deliver_delay_magic` | `crates/world/src/slave.rs` | `RM_DELAYMAGIC` 效果体（`:711-729`） | 施法者队列 → 目标 `pending_mag_struck` | 不在本层；由 `deliver_mag_struck` 总门兜底 |
| `World::deliver_poison` | `crates/world/src/slave.rs` | `RM_POISON` 效果体（`:740-759`） | 中毒目标 | 上游总门已拦 |
| `World::deliver_mag_struck` | `crates/world/src/world.rs` | `RM_MAGSTRUCK`/`RM_MAGSTRUCK_MINE`（`:618-686`） | `pending_mag_struck` 的目标 | ✅ 直接判 race 201 → `clone_operate` |
| `monai::base_object_run_monster` | `crates/world/src/monai.rs` | `pending_struck` → `RM_STRUCK`（`AnimalObject.cs:118-133`） | AI/守卫/NPC 实体 | ✅ 出队前判 race 201 → `clone_operate` |
| `monai::animal_struck` | `crates/world/src/monai.rs` | `AnimalObject.Struck`（`:336-359`） | 受击怪 | 由上游总门拦 |
| `special_monster::clone_operate` | `crates/world/src/special_monster.rs` | `Clone.Operate` 三 ident + 恒 false | CloneMonster | 本身即总门 |
| `special_monster::operate_special` | `crates/world/src/special_monster.rs` | `RM_ZEN_BEE`（Bee/Spider）+ Clone 分派 | 专用怪 | ✅ `SpecialKind::Clone ⇒ Some(clone_operate(..))` |
| `special_monster::special_spawn_request` / `adopt_special_child` | `crates/world/src/special_monster.rs` | `BeeQueen.cs:37-41` / `SpiderHouseMonster.cs:38-44` | 产仔者/子体 | 不适用 |
| `World::attack_once` | `crates/world/src/world.rs` | 命中结算 + `pending_struck` 入队（`BaseObject.Attack.cs:61-64`） | 受击者 | ✅ 命中者登记 `set_last_hiter` 对 race 201 跳过 |

### 2.2 效果体（直接调用点）

| 符号 | 文件 | 对应 oracle |
|---|---|---|
| `World::make_posion` | `crates/world/src/world.rs` | `BaseObject.MakePosion`（`BaseObject.cs:1941-1973`） |
| `World::mag_make_private_transparent` | `crates/world/src/world.rs` | `MagicManager.MagMakePrivateTransparent`（`MagicManager.cs:673-699`） |
| `World::make_open_health` | `crates/world/src/world.rs` | `BaseObject.MakeOpenHealth`（`BaseObject.cs:944-950`） |
| `World::open_holy_seize_mode` | `crates/world/src/world.rs` | `AnimalObject.OpenHolySeizeMode`（`AnimalObject.cs:322-328`） |
| `World::char_pushed_now` / `magic::char_pushed` | `crates/world/src/world.rs` / `crates/world/src/magic.rs` | `BaseObject.CharPushed`（`BaseObject.cs:854-894`） |
| `World::settle_deaths` | `crates/world/src/world.rs` | `DieDropItems` 的怪物侧折叠（`BaseObject.cs:2450-2464`） |
| `World::run_map_events` | `crates/world/src/world.rs` | 火墙 `RM_MAGSTRUCK_MINE` 生产（`FireBurnEvent.cs:36`） |
| `World::tick` 的 AOI 段 → `aoi::search_view_range` | `crates/world/src/world.rs` / `crates/world/src/aoi.rs` | `UpdateMonsterVisible`（`MonsterObject.cs:136-175`）的轮询等价 |
| `bins/game-svr::drain_combat_downlink` / `drain_b4_with_enter_cfg` / `drain_b5_self_deliveries` / `drain_b3_self_deliveries` / `drain_mag_healing_delivery` / `drain_slave_mutiny_downlink` | `bins/game-svr/src/main.rs` | 各 `SelfDelayOutcome`/`MagStruckOutcome` 的帧面 |

### 2.3 入队点（谁把消息放进队列）

| 符号 | 文件 | 入队的 ident |
|---|---|---|
| `World::send_self_delay_msg` / `send_self_delay_msg_full` / `send_self_delay_msg_with` | `crates/world/src/slave.rs` | 通用 |
| `World::send_msg_now` | `crates/world/src/slave.rs` | 即时 `SendMsg`（含 `RM_MAKEHOLYSEIZEMODE`） |
| `World::send_struck_delay_msg` | `crates/world/src/slave.rs` | `RM_MAGSTRUCK`（穿透 9/10，600ms） |
| `World::cast_poison_on_target` / `cast_mab_mabe_spell` / `cast_group_lightening_spell` | `crates/world/src/world.rs` | `RM_POISON`/`RM_DELAYMAGIC`（部分魔号） |
| `World::cast_healing_spell` / `cast_big_healing_spell` | `crates/world/src/world.rs` | `RM_MAGHEALING` |
| `World::cast_private_transparent_spell` / `cast_group_transparent_spell` | `crates/world/src/world.rs` | `RM_TRANSPARENT` |
| `World::cast_show_hp_spell` | `crates/world/src/world.rs` | `RM_DOOPENHEALTH` |
| `World::cast_fireball_spell` / `cast_lightening_spell` / `cast_firecharm_spell` / `cast_hb_fireball_spell` | `crates/world/src/world.rs` | **只返回 `SpellStrikeOutcome`，不入队**（G01/G04 根因） |
| `World::cast_big_explosion_spell` / `cast_elec_blizzard_spell` | `crates/world/src/world.rs` | 直接 `pending_mag_struck`（跳过 `RM_DELAYMAGIC` 的 600ms 层） |
| `World::cast_pass_through_spell` | `crates/world/src/world.rs` | `send_struck_delay_msg`（`RM_MAGSTRUCK`） |
| `World::cast_fire_wall_spell` + `run_map_events` | `crates/world/src/world.rs` | `pending_mag_struck { mine: true }` |

---

## 3. 矩阵（38 行）

> 状态口径：`已实现` = 效果体与投递路径均在且行为等价；`部分` = 主路径在但缺层/缺配置/缺类；`缺失` = 效果体或投递未接；`不适用` = 发送点当前不可达或对象未移植。
> tier：R = 需真机/独占窗口；L = 等价 + 抽样冒烟。

| # | ident | 目标类别 | oracle 效果（行号） | Rust 落点（符号/路径） | 状态 | 可观测后果 | tier | 可构造级判据 |
|---|---|---|---|---|---|---|---|---|
| R01 | `RM_MAGSTRUCK` | 全 BaseObject（Clone 除外） | `BaseObject.Base.cs:618-686` | `world::deliver_mag_struck` | 已实现 | HP/走 tick/肉/目标/帧 | R | `mac=0`、`power=10` ⇒ HP-10；怪 `walk_tick ∈ [800,1800)` |
| R02 | `RM_MAGSTRUCK_MINE` | 全（Clone 除外） | `BaseObject.Base.cs:619`（与 `RM_MAGSTRUCK` 同支） | `world::deliver_mag_struck`（`mine=true`） | 已实现 | 同上但**不抽** `R(1000)` | R | `mine=true` ⇒ `walk_tick` 不变 |
| R03 | `RM_STRUCKEFFECT` | 全 BaseObject | `BaseObject.Base.cs:687-698` | 无 | 缺失（发送点不可达） | 只差帧；无状态 | L | 发送方 `ElfWarriorMonster`(114) 属数据不可达（plan R16） |
| R04 | `RM_REFMESSAGE` | Play（观察者/受击者） | `BaseObject.Base.cs:699-705` | `world::attack_once` → `pending_struck` + `bins/game-svr::drain_combat_downlink` | 部分（延时层缺失） | `SM_STRUCK` 即时而非 200/700ms | R | 记录「200ms 内不得出现 `SM_STRUCK`」 |
| R05 | `RM_REFMESSAGE` | AnimalObject 树 | `BaseObject.Base.cs:701-705` | 同上 + `monai::base_object_run_monster` | 部分（延时层缺失） | 怪 `Struck` 提前 | R | 同 R04（目标为怪） |
| R06 | `RM_DELAYMAGIC` | 任意目标 | `BaseObject.Base.cs:711-729` | `slave::deliver_delay_magic` | 部分（6 魔号未入队） | 1/5/11/13/44/45 伤害不落地 | R | 魔号 1 命中目标 ⇒ 600ms 后 HP 应减 |
| R07 | `RM_RANDOMSPACEMOVE` | 任意 | `BaseObject.Base.cs:730-732` | `merchant::map_random_move`（无消息层） | 不适用 | 无（发送点被注释） | L | `ScriptEngine.cs:955` 为注释 |
| R08 | `RM_DELAYPUSHED` | 任意 | `BaseObject.Base.cs:733-739` | `world::char_pushed_now` / `magic::char_pushed` | 缺失 | 44 的推挤不落地/无 `SM_BACKSTEP` | R | `push>0` ⇒ 目标坐标应移动 |
| R09 | `RM_POISON` | 任意（目标自身） | `BaseObject.Base.cs:740-759` | `slave::deliver_poison` | 已实现 | 毒 buff/`CharStatus`/657/100/656 | R | 投 8037 后 `status_time[DECHEALTH]` 变 |
| R10 | `RM_TRANSPARENT` | 任意 | `BaseObject.Base.cs:760-762` | `deliver_self_delay_msgs` + `world::mag_make_private_transparent` | 已实现 | 隐身/目标清除/状态帧 | R | 投 8045 后 `hide_mode=true` |
| R11 | `RM_DOOPENHEALTH` | 任意 | `BaseObject.Base.cs:763-765` | `deliver_self_delay_msgs` + `world::make_open_health` | 已实现 | `show_hp`/`OPENHEATH`/帧 | R | 投 8040 后 `show_hp=true` |
| R12 | `RM_DIEDROPITEM` | Monster/Animal | `BaseObject.Base.cs:766-768`；`BaseObject.cs:2450-2464` | `world::settle_deaths` | 已实现（折叠；落地恒 0） | 掉落预算/金币 | R | `drop_items`/`gold` 预算与 oracle 同；`placed=0` |
| R13 | `RM_DIEDROPITEM` | Play | `BaseObject.cs:2454` → `PlayObject.Message.cs:2152-2220` | `world::settle_deaths`（无 `DropUseItems`） | 缺失 | 装备清空 + 每槽 `Random(nRate)` 不抽 | R | 玩家死亡前后 RNG 序列与装备槽 |
| R14 | （default） | 任意 | `BaseObject.Base.cs:769-776` | — | 已实现 | 无效果 | L | 任意未列 ident 不改状态 |
| R15 | `RM_STRUCK` | AnimalObject 树（普通怪/守卫/NPC/商人） | `AnimalObject.cs:118-133` | `monai::base_object_run_monster` + `animal_struck` | 已实现（部分：`MonsterSayMessage` 未建模） | 目标/肉/WalkTick | R | 入 `pending_struck` 后出队 ⇒ `AttackTick += 150-…` |
| R16 | `RM_STRUCK` | CloneMonster | `Clone.cs:14-35`（经 `AnimalObject.cs:118`） | `monai::base_object_run_monster` 的 race 201 门 | 已实现 | 主人 MP 扣/HP 归零 | R | `clone_operate(RM_STRUCK, wParam)` ⇒ `master.mp -= wParam` |
| R17 | `RM_MAKEHOLYSEIZEMODE` | AnimalObject 树 | `AnimalObject.cs:134-136` | `deliver_self_delay_msgs` + `world::open_holy_seize_mode` | 已实现（无类别门） | `HolySeize`/656 | R | 投 60005 后 `ai.holy_seize=true` |
| R18 | `RM_MAKEHOLYSEIZEMODE` | Play/PlayClone/Robot | 基类无此支（`BaseObject.Base.cs:769` default） | 同上（未加 `Race>=Animal` 门） | 部分（构造级偏差） | 玩家会被置 `holy_seize`（当前发送点不可达） | L | 直接 `send_msg_now` 给玩家 ⇒ oracle 无变化/Rust 有变化 |
| R19 | `RM_UPDATEVIEWRANGE` | MonsterObject 树 | `MonsterObject.cs:136-175,177-185` | `World::tick` AOI 段 → `aoi::search_view_range` | 已实现（轮询等价） | 视野表/`is_visible_active` | L | 怪视野表在下一 tick 含新玩家 |
| R20 | `RM_ZEN_BEE` | BeeQueen | `BeeQueen.cs:33-45` | `special_monster::operate_special` + `SpecialSpawner` | 已实现 | 产仔/抽数/`RM_HIT` | R | 投 8020 ⇒ 子体出现且继承目标 |
| R21 | `RM_ZEN_BEE` | SpiderHouseMonster | `SpiderHouseMonster.cs:32-49` | 同上 | 已实现 | 产仔（`CanWalk(y+1)` 门） | R | 墙格 ⇒ 不产仔 |
| R22 | `RM_SPELL` | CloneMonster | `Clone.cs:16-31` | `special_monster::clone_operate`（广播入口未接） | 部分 | 主人 MP | L | 广播面未建模；`clone_operate` 单测已覆盖 |
| R23 | 任意其他 ident（吞 base） | CloneMonster | `Clone.cs:34` | `deliver_self_delay_msgs` 首门 + `clone_operate` | 已实现 | 全部 base 效果不生效 | R | 6 类投递逐一投 Clone ⇒ 状态不变 |
| R24 | 全部 ident（纯转发） | Stick/SuperGuard/PlayCloneObject | `StickMonster.cs:96-99`、`SuperGuard.cs:58-61`、`PlayCloneObject.cs:22-25` | 守卫/NPC 走通用 `monster_ai_tick`；PlayClone 为静态实体 | 部分（O08） | PlayClone 不处理消息 | L | `T-51` 登记 |
| R25 | `RM_STRUCK` | Trainer | `Trainer.cs:23-32` | 无 Trainer 实体 | 缺失（O10） | 无「破坏力」播报 | L | 练功师受击应 `ProcessSayMsg` |
| R26 | `RM_MAGSTRUCK` | Trainer | `Trainer.cs:33-36` | 无 | 缺失（O10） | 同上 + base 效果 | L | 同上 |
| R27 | `RM_STRUCK` | RobotPlayer | `RobotPlayer.cs:1139-1168` | 无机器人系统 | 不适用（O11） | — | L | RobotSystem 未移植 |
| R28 | `RM_STRUCK`/`RM_STRUCK_MAG` | Play | `PlayObject.Message.cs:1562-1618` | `bins/game-svr::drain_combat_downlink`（`SM_STRUCK`） | 部分（O09/P19） | 客户端受击帧/`SetPkFlag`/tick 重置 | R | 受击后 `SM_STRUCK` 字段逐一比对 |
| R29 | `RM_MAGHEALING` | Play | `PlayObject.Message.cs:1619-1629` | `deliver_self_delay_msgs` + `drain_mag_healing_delivery` | 已实现 | `IncHealing`/`PerHealing` | R | 投 8034(100) ⇒ `inc_healing=100` |
| R30 | `RM_MASTERDIEMUTINY` | Play | `PlayObject.Message.cs:2116-2118` | `deliver_self_delay_msgs` + `slave::process_slave_mutiny` | 已实现 | 宝宝叛变 | R | 主人死 1000ms 后 `mutinied` 非空 |
| R31 | `RM_MASTERDIEGHOST` | Play | `PlayObject.Message.cs:2119-2121` | `deliver_self_delay_msgs` + `slave::process_slave_ghost` | 已实现（oracle 缺陷：`!Ghost` 恒拦，永不入队） | 无 | R | `send_master_die_ghost` 后队列仍空 |
| R32 | `RM_PLAYERKILLMONSTER` | Play | `PlayObject.Message.cs:2110-2112` | `world::settle_deaths` 经验结算 | 已实现 | 经验/升级 | R | 击杀后 `exp_awards` 与 oracle 同 |
| R33 | `RM_SPIRITSUITE` | Play | `PlayObject.Message.cs:2113-2115` | `bins/game-svr::drain_run_state_expiries` | 已实现 | 烈火/双龙到期 | L | 20s/60s 边界 |
| R34 | `RM_DEATH`/`RM_DISAPPEAR`/`RM_SKELETON` | Play | `PlayObject.Message.cs:1630-1665` | `bins/game-svr` AOI/死亡帧 | 部分 | 客户端帧 | R | 死亡/消失帧计数 |
| R35 | `RM_*` 移动/攻击回显族（`:1403-1561` 共 23 个 `case Messages.RM_` 标签：外层 20 ident + `:1532/:1535/:1538` 嵌套 3 个） | Play | `PlayObject.Message.cs:1403-1561` | `bins/game-svr` action downlink / `Command` | 部分 | 动作帧 | R | 逐帧 `SM_*` 对比 |
| R36 | `RM_ALIVE` 等状态/UI 族（`:1662-2029` 共 95 个 RM 标签） | Play | `PlayObject.Message.cs:1662-2029` | `bins/game-svr` 帧路径 | 部分 | UI 帧 | L | 抽样 |
| R37 | `CM_*`（73 个 case） | Play（网关→会话） | `PlayObject.Message.cs:781-1394`（64 个）+ `:1397`/`:1400`（2 个）+ `:2057-2075`（7 个） | `bins/game-svr` 命令处理（**非** `deliver_*` 面） | 部分（O09/P19） | 客户端操作 | R | 按 O09/P19 卡 |
| R38 | `default → base` | Play | `PlayObject.Message.cs:2122-2126` | 无泵等价物 | 部分（返回值语义未建模） | 本 tick 剩余消息被推迟 | R | 同一 tick 两条消息的顺序/条数 |

---

## 4. 缺口清单（按「真机可达 × 影响」排序）

### G01（R）`RM_DELAYMAGIC`：六个魔号在生产宿主未入队
- **缺口**：`bins/game-svr::handle_spell_cmd` 对魔号 1/5/11/13/44/45 只调用
  `World::cast_fireball_spell`/`cast_lightening_spell`/`cast_firecharm_spell`/`cast_hb_fireball_spell`
  并取 `bo_train/target`，`SpellStrikeOutcome::power` **没有任何消费者**；`deliver_delay_magic` 虽已实现
  （且它在 42/50 的入队点会运行），但这六个魔号不入队，故其投递永不到达
  ⇒ 伤害不落地、`RM_MAGSTRUCK` 链不触发。
- **真机可达**：每次施放 1/5/11/13/44/45 且门通过。
- **影响**：战斗数值（伤害/HP/死亡/掉落）+ 帧面（31/53/656）。
- **最小修复面**：`bins/game-svr::handle_spell_cmd` 在 `outcome.target.is_some()` 时按 oracle 延时
  （1/5/11/45=600ms、13=1200ms、44=600ms）调 `World::send_self_delay_msg_full(caster, RM_DELAYMAGIC,
  power, MakeLong(tx,ty), 2, target, delay)`；`slave::deliver_delay_magic` 与
  `drain_b4_with_enter_cfg` 已就绪，无需改。
- **tier**：R。证据：宿主注释明写「延迟结算（`GetMagStruckDamage`/`RM_MAGSTRUCK`/`CharPushed`）未建模（登记）」。

### G02（R）`RM_DIEDROPITEM`：玩家 `DropUseItems` 未接
- **缺口**：oracle `DieDropItems`（`BaseObject.cs:2454`）对玩家调 `PlayObject.DropUseItems`
  （`PlayObject.Message.cs:2152-2220`）：每个装备槽 `Random(nRate)`（`:2184-2189`，**含 null 槽**）、
  `ItemDesc & 8` 的装备置 `Index=0`、末尾 `RM_SENDDELITEMLIST`。Rust `World::settle_deaths`
  只算怪物掉落预算，玩家死亡不抽数、不清装备。
- **真机可达**：任意玩家在非 `FightZone/Fight3Zone` 地图死亡（默认地图）。
- **影响**：RNG 序列（每槽一次 `Random(DieDropUseItemRate)`，随后所有世界抽数错位）+ 背包/装备状态 + UI 帧。
- **最小修复面**：新增 `World::drop_use_items_on_death(actor_id, slots, rate)`（或宿主侧
  `bins/game-svr` 用 `world.rng` 逐槽抽），由 `World::settle_deaths` 的玩家分支调用；
  帧面复用 `RM_SENDDELITEMLIST` 的宿主下行。
- **tier**：R。

### G03（R）`RM_REFMESSAGE`：200/700ms 延时层缺失
- **缺口**：oracle 受击者队列先收 `RM_REFMESSAGE`（`BaseObject.Attack.cs:57` 200ms、各怪 300/500/600/700ms），
  到点才 `SendRefMsg(RM_STRUCK)` 并（`Race != Play`）自入队 `RM_STRUCK`。Rust `World::attack_once`
  直接压 `pending_struck`，`drain_combat_downlink` 即时发 `SM_STRUCK`。
- **真机可达**：每次近战/怪物特殊攻击命中。
- **影响**：帧时序（客户端受击提前 200ms+）+ 怪 `Struck`/目标切换时序。
- **最小修复面**：`world::attack_once` 改为 `send_self_delay_msg_with(target, RM_REFMESSAGE, damage, hp,
  maxhp, hitter, 200..700)`；`slave::deliver_self_delay_msgs_with` 增加 `RM_REFMESSAGE` 支
  （产生 `pending_struck` + 帧事件），宿主 `drain_combat_downlink` 改读投递结果。
- **tier**：R。已由 `tests/parity/world/client-e2e-enter-gaps-20261013/155-*` 登记为偏差。

### G04（R）`RM_DELAYPUSHED`：44 的推挤不结算
- **缺口**：`MagicManager.cs:1053` 的 `RM_DELAYPUSHED`（600ms）无 Rust 入队点；
  `cast_hb_fireball_spell` 只把 `push` 放进 `SpellStrikeOutcome`，`handle_spell_cmd` 不消费。
  `BaseObject.Operate` 的该支（`:733-739`）在 Rust 无消息臂。
- **真机可达**：44 施放成功且 `push>0`。
- **影响**：目标坐标/朝向/`WalkTick`、`SM_BACKSTEP` 帧。
- **最小修复面**：`World::cast_hb_fireball_spell` 在 `push > 0` 时
  `send_self_delay_msg_full(caster, RM_DELAYPUSHED, dir, MakeLong(tx,ty), push, target, 600)`；
  `deliver_self_delay_msgs_with` 增加 `RM_DELAYPUSHED` 支调 `World::char_pushed_now`；宿主补 `SM_BACKSTEP` 下行。
- **tier**：R。

### G05（R）自投递泵的「队首阻塞」语义 —— 已实现（`7e8d47426`）
- **原缺口**：oracle `GetMessage(msgTick, ref)` 只看队首，未到点 ⇒ 重新以 `Lowest` 入队并 `return false`
  （整段消息停止）；审计基线 `8dea847c5` 的 Rust `deliver_self_delay_msgs_with` **跳过未到点项继续扫描后续项**。
- **现状（`7e8d47426`）**：`SelfDelayQueue` 改为 .NET 8 `PriorityQueue` 同构的四叉堆；泵只在队首到点时投递，
  队首未到点 ⇒ 出队、以 `Lowest` 重入队并 `break`（只结束本次泵，无跨调用阻塞），与 `BaseObject.Message.cs:334-378`
  的 `TryDequeue` / 重入队 / `return false` 逐条对应。
- **真机可达**：同一 actor 同时有多条不同到点时刻的自投递（如 `RM_MAGHEALING` + `RM_MAGIC_LVEXP`）。
- **影响（已关闭）**：tick 内投递次序/条数。
- **证据**：`7e8d47426` 记录与 net8.0.26 `PriorityQueue` 的 500 场景差分（不一致 0）、13 条 `pr1_*` 判据与 3 颗牙。
- **tier**：R（真机窗口仍待）。

### G06（L）`RM_MAKEHOLYSEIZEMODE` 缺 `AnimalObject` 类别门
- **缺口**：oracle 只有 `AnimalObject.Operate` 处理该 ident；`PlayObject` 落 default（无效果）。
  Rust `deliver_self_delay_msgs_with` 对任意实体调 `open_holy_seize_mode`。
- **真机可达**：当前三处发送点目标均为 `MonsterObject`/`Race>=Animal` ⇒ 生产不可观测；构造级可达。
- **影响**：玩家 `holy_seize`/改名色（若未来发送点放宽）。
- **最小修复面**：该臂加 `race >= ACTOR_RACE_ANIMAL`（或专用 `is_animal_object` 位）。
- **tier**：L。

### G07（L）Trainer（练功师）`Operate` 未接
- **缺口**：`Trainer.cs:20-38` 的受击统计/播报（`RM_STRUCK`/`RM_MAGSTRUCK`）无 Rust 对象；且
  `RM_STRUCK` 不调 base（抑制 `AnimalObject` 效果）。plan `O10` 已单列。
- **真机可达**：练功师 NPC 受击（正式区存在）。
- **影响**：聊天文本；状态无。
- **最小修复面**：实体加 `trainer` 标志 + `World::trainer_operate`（累计/3s 播报）；宿主下行 `RM_HEAR`。
- **tier**：L。

### G08（L）`PlayCloneObject.Operate` / 玩家克隆体消息处理
- **缺口**：oracle `PlayCloneObject` 是完整 `PlayObject`（继承玩家泵与全部 case）；Rust
  `slave::spawn_clone` 生成静态实体（`T-51`，不跑 tick/消息）。plan `O08` 已单列。
- **真机可达**：`MagMakeClone`（魔号 40 等）施放后。
- **影响**：克隆体的受击/动作/帧面。
- **最小修复面**：为克隆体接 `PlayObject` 消息面（或在 `T-51` 白名单内保持现状）。
- **tier**：L。

### G09（L）RobotPlayer `Operate` 未接
- **缺口**：`RobotPlayer.cs:1134-1179` 的 `RM_STRUCK` 专用体无 Rust 对象；plan `O11` 已单列。
- **真机可达**：仅当启用 RobotSystem（Rust 未移植）。
- **影响**：机器人 PK 标记/受击 tick。
- **最小修复面**：机器人系统移植时按 oracle 落 `robot_struck`。
- **tier**：L。

### G10（L）`RM_STRUCKEFFECT` 无效果体
- **缺口**：`BaseObject.Base.cs:687-698` 的帧中继 + 条件 `RM_STRUCK` 自入队，Rust 无对应 arm；
  `deliver_self_delay_msgs_with` 的 `_ => {}` 会静默吞掉。
- **真机可达**：**当前不可达**（唯一发送方 `ElfWarriorMonster.cs:65` 的 race 114 在 plan R16「数据不可达」集中）。
- **影响**：解禁 R16 后才会出现帧/受击差异。
- **最小修复面**：`deliver_self_delay_msgs_with` 增加 `RM_STRUCKEFFECT` 臂（`SendRefMsg` → 宿主帧；
  `ActorId==RM_STRUCK` 时压 `pending_struck`）。
- **tier**：L。

### G11（L）Clone 的 `RM_SPELL` 入口未接
- **缺口**：`clone_operate` 已实现 `RM_SPELL` 的扣 MP 分支，但 `RM_SPELL` 是 `SendRefMsg` 广播消息
  （`Clone.cs:16` 收到它才扣 `Master.MP`）；Rust 的广播面（`WantRefMsg`/收件人集）未把 `RM_SPELL`
  派发到 `clone_operate`。
- **真机可达**：clone 需在 `SendRefMsg` 收件人集内（oracle 只发 `Race==Play` 或 `WantRefMsg`；
  Clone race 201 通常不在内）⇒ 当前基本不可达。
- **影响**：主人 MP。
- **最小修复面**：`World` 的 `RM_SPELL` 广播路径按 oracle 收件人集调 `special_monster::clone_operate`。
- **tier**：L。

### G12（L）`deliver_self_delay_msgs_with` 对未知 ident 静默丢弃
- **缺口**：`_ => {}` 使「已入队但未建模」的 `RM_*` 无任何告警；G04/G10 因此不会被单测发现。
- **真机可达**：当前仅潜在（无生产入队点）。
- **影响**：缺陷可见性。
- **最小修复面**：为已知 `BaseObject` 投递集加显式 arm（至少 `debug_assert!` 或 `SelfDelayOutcome`
  的 `unmodeled` 标记），让宿主日志点名。
- **tier**：L。

---

## 5. 专项：Clone「恒 return false」的覆盖面（以及其他改变泵语义的类）

### 5.1 oracle 侧分类

| 类 | `Operate` 返回 | 是否抑制 base 效果 | 泵影响（实测） |
|---|---|---|---|
| `BaseObject` | **恒 false**（`:776`） | n/a | 只在玩家系 `default → base` 路径上让玩家泵 break |
| `AnimalObject` | 两条 true，其余 base(false) | 否 | 怪泵丢弃 |
| `MonsterObject` | 一条 true，其余 base(false) | 否 | 怪泵丢弃 |
| `BeeQueen`/`SpiderHouseMonster` | 处理完仍走 base(false) | 否 | 怪泵丢弃 |
| `CloneMonster` | **恒 false 且从不 base** | **是（全部 10 支 + Animal 2 支）** | 怪泵丢弃；关键在效果屏蔽 |
| `StickMonster`/`SuperGuard` | base(false) | 否 | 怪泵丢弃 |
| `PlayCloneObject` | = `PlayObject` 的返回值（**不是**恒 false） | 否 | 玩家 turbo 泵，true 继续排空 |
| `Trainer` | 恒 false；`RM_STRUCK` 不 base | **是（仅 `RM_STRUCK` 抑制 Animal 支）** | NormNpc 泵丢弃 |
| `RobotPlayer` | `RM_STRUCK` true，其余 `PlayObject` 结果 | 否 | 玩家 turbo 泵 |

### 5.2 Rust 的 Clone 总门覆盖（逐路径）

| 投递路径 | Rust 符号 | 是否拦 Clone |
|---|---|---|
| 自投递/即时 `SendMsg`（全部 ident） | `slave::deliver_self_delay_msgs_with` 首门 | ✅ |
| `RM_MAGSTRUCK`/`MINE` | `world::deliver_mag_struck` | ✅ |
| `RM_STRUCK`（`pending_struck` 出队） | `monai::base_object_run_monster` | ✅ |
| 专用怪分派接缝 | `special_monster::operate_special` | ✅ |
| 入队侧 `set_last_hiter` | `world::attack_once` | ✅（只跳过登记；队列项仍入，供扣 MP） |

**结论**：`RM_POISON` 类漏屏蔽的旁路已被堵住；四条投递路径 + 入队侧均有 race 201 判定，
且有构造级用例 `clone_operate_swallows_every_modeled_delivery_class` 钉住。
**未覆盖**：`RM_SPELL` 的广播入口（G11）；`Trainer` 的 `RM_STRUCK` 抑制（G07）；
`PlayCloneObject`/`RobotPlayer` 的玩家泵语义（G08/G09）。

---

## 6. 复现命令与证据

```sh
# 工作单元
git worktree add /e/tmp/impl-opaudit 8dea847c5 --detach
cd /e/tmp/impl-opaudit

# oracle 清册
grep -rn "override bool Operate" --include=*.cs src/          # 11 条
sed -n '610,777p' src/M2Server/Actor/BaseObject.Base.cs        # base 分支表
sed -n '768,2126p' src/M2Server/Player/PlayObject.Message.cs   # 215 个 case
sed -n '356,366p' src/M2Server/Player/PlayObject.Message.cs    # turbo 泵
sed -n '25,31p'   src/M2Server/Actor/BaseObject.Base.cs        # 怪泵（丢弃返回值）

# Rust 投递点
grep -rn "fn deliver_\|fn animal_struck" crates/
grep -rn "RACE_CLONE_MONSTER" crates/world/src
grep -rn "send_self_delay_msg\|send_struck_delay_msg" crates/world/src

# 只读核对（本审计未运行服务、未占端口）
```

- 本件未执行 `cargo test`/`cargo clippy`（只读审计，未改代码）；矩阵中的「可构造级判据」是**建议判据**，
  不是已跑结果。
- 九游戏端口状态未采（任务要求「只记录不停」）；本 worktree 未起任何进程。

---

## 7. 边界与未覆盖声明

1. 本件只排 `Operate` 的**投递/效果面**；`Run`/`AttackTarget`/`Die` 等覆写不在范围（见
   `plan-run-gap-batches-20261018.md` §2）。
2. `PlayObject.Operate` 的 215 个 case 中，`CM_*` 73 个属网关/会话层，不在 `deliver_*` 面；
   矩阵 R37 只给聚合行，细目归 O09/P19。
3. 配置门 `MonDelHptoExp`/`MonSayMsg`/`DisableSelfStruck`/`DisableStruck` 在 Rust 侧缺省 false，
   按合同 §3.1 pin；开启时的分支未建模（R01/R15 的「部分」注记）。
4. `FastParalysis` 在 oracle 全仓只有构造器置 false（`BaseObject.cs:461`）与消费点，**无置 true 点**
   （`grep -rn FastParalysis src`）⇒ 三段 `if (FastParalysis)` 为死代码，Rust 未建模不构成缺口。
5. 数据不可达集（plan §2：18 个 race）中的发送点（如 `RM_STRUCKEFFECT` 的 ElfWarrior 114）按「解禁后生效」
   记 L，不计入 R 级缺口。

---

## 附录 A：`PlayObject.Operate` case 分块（`PlayObject.Message.cs:768-2126`）

| 行号区间 | 内容 | 条数 |
|---|---|---|
| `:781-1394` | `CM_*` 客户端操作（查询/物品/商店/组队/交易/行会/移动/攻击/施法/说话） | 64 |
| `:1397`、`:1400` | `CM_PASSWORD`、`CM_QUERYVAL`（散点 CM） | 2 |
| `:1403-1561` | `RM_*` 移动/攻击/推挤回显族（`RM_WALK`…`RM_RUSHKUNG`） | 23 |
| `:1562-1618` | `RM_STRUCK`/`RM_STRUCK_MAG` 玩家受击帧 | 2 |
| `:1619-1629` | `RM_MAGHEALING` | 1 |
| `:1630-2029` | 死亡/消失/属性/物品/商店/行会/UI 帧族 | 98 |
| `:2031-2121` | 密码/骰子/寄售/击杀/精灵/主人死族（18 RM + 7 CM） | 25 |
| `:2122-2126` | `default → base.Operate` | 1 |

## 附录 B：计数

- oracle override：**11**（M2Server 9 + Trainer + RobotPlayer）。
- `BaseObject.Operate` 分支：**10 个 ident 组**（含 `RM_MAGSTRUCK`/`MINE` 同支）。
- `PlayObject.Operate` case 标签：**215**（`CM_*` 73 / `RM_*` 142；这是**含嵌套标签**的原始标签计数）。
- Operate 内共 **3 个 switch**（1 外层 + 2 嵌套）；嵌套 case 标签 **11 个**：`:1532`/`:1535`/`:1538` 3 个 + `:1705-1726` 8 个。
- 矩阵：**38 行**。
- 缺口：**12 条**（R 5 / L 7）；G05 已由 `7e8d47426` 关闭 ⇒ 未闭合 **11 条（R 4 / L 7）**。
