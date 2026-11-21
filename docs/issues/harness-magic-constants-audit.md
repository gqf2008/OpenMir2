# 装置/驱动魔数常量出处审计（2026-11-21，只读）

> 目的：逐个问"这个值/行为是**生产的**，还是**我们发明的**"。今天的四个教训（100ms 节拍、堆比较键/arity 误判、actor 迭代序、AOI 非纯几何）共同形状就是"常识值没人问出处"。
> 范围：`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs`、`mir2-rs/bins/game-svr/src/lib.rs`（驱动 rig/断言）、`mir2-rs/tests/parity/tests/magic_fireball_parity.rs`（比较器）。
> 分类：**【P】有生产出处**；**【M】无出处但只是建模约定**（写明它约束了什么）；**【K】无出处且被写成生产事实**（下一批要契约化）。

## 1. 逐条表

| 常量 | 值 | 装置哪里用 | 生产有无出处（符号锚 or "无"） | 若生产不是这个值，会影响哪些判据 |
| --- | --- | --- | --- | --- |
| `StepDrainMs` | 100 ms | `Program.cs` 各 `MagicSpec.StepDrainMs`；`RunFireball` 的 `const StepDrainMs`/`WaitMs`；`RunMagicCase` 步进排水 | 【K】生产**无**此节拍：actor `Run()` 是 `RunTime=250` 门 + 玩家 50ms/怪物 20ms 循环（`docs/issues/production-tick-cadence.md` §0/§9）。装置注释称"与生产逐 tick 交付同序"——该句没有生产出处 | 所有延迟帧的 `due_ms_rel`、`DrainPasses`、同 step 内顺序；B23 第一实例 |
| `WaitMs` | 700（RunFireball）/ 1200（`MagicSpec.WaitMs` 缺省）/ 1600（tamming） | 排水窗口（`elapsed < WaitMs`；`Thread.Sleep` 已换 `ClockSeam.Advance`） | 【M】生产无"等待后一次排空"概念（生产按 tick 逐条）。**约束**：窗口必须覆盖被测延迟，超出即漏账 | 延迟 > WaitMs 的消息是否进账本；`NotDue` 列表 |
| `MaxDrainPasses` | 8 | `RunFireball` / `RunMagicCase` 单次排水循环上限；命中写 `DrainPassesHitCap` | 【M】生产无固定上限（`while(GetMessage)` 到未到期为止）。**约束**：防自发链无限增生；上界命中必须可见（已实现） | 超 8 趟的自发链被截断（显式标记，不静默） |
| `idBase` 段 | 1000/2000/3000（caster=+1、target=+2、obs=+3/+4；怪物 +10；预置 slave +50） | `Program.cs` 两处 scene 构造；`RunFireball`/`RunMagicCase` | 【M】生产 ActorId 由 `ActorMgr` 随机队列分配（`RandomNumber` 用 `new Random()` 时间种子）⇒ 值不可复现。**约束**：ActorId 进帧参数/尾串，必须与 Rust rig 同值 | 含 ActorId 的帧（如 `SM_MAGICFIRE` 尾串）与 Rust rig 对拍；生产上无意义 |
| 座位坐标 | caster(100,100)/target(101,100)/obs_in(105,100)/obs_out(140,100) | `Program.cs` 两处 scene 构造 | 【M】生产无固定摆位。**约束**：obs_in 在 ±12 AOI 内、obs_out 在 ±12 外（生产锚 `GameSvrConf.SendRefMsgRange=12`）、target 与 caster 相邻（push 的 `<=1`） | AOI 集合（obs_out 负对照）、`MagPushArround` 相邻性、走位/穿透线 |
| `CasterScNoClamp`/`caster_sc` | `MakeWord(5,10)`=15 | 多数 spec 的 `MagicSpec.CasterScNoClamp`；`Fixture.caster_sc` | 【M】生产 SC 来自人物属性/装备（`WAbil`/`Abil`），15 是夹具选择；且 B21：登录序 `RecalcAbilitys` 用 `Abil.SC=0` 覆盖（`uenhancer` 15→0） | SC 相关威力/命中公式；B21 意图未物化 |
| `CasterMcNoClamp`/`caster_mc` | `MakeWord(5,10)`=15 | 多数 spec；`Fixture.caster_mc` | 【M】生产 MC 来自人物属性；15 是夹具 | 伤害区间（`GetAttackPower`） |
| `HitSpeed` | 7 | `hangmajinbub`/`dejiwonho`/`shield`/`bigcloak`/施毒 4 支 | 【M】生产 HitSpeed 来自角色装备/属性（`CharacterObject.HitSpeed`）；7 是夹具 | `SM_CHARSTATUSCHANGED(657)` 的 series |
| `CasterLevel`/`MagicTrainLv`/`MagicMaxTrain0`/`UserMagicLevel` | 30 / 3 / 1000 / 各 spec | `MagicSpec` 各分支；`Fixture` | 【M】生产来自角色等级与魔法 DB；具体值（30/3/1000）是夹具，用来命中/避开等级与训练门 | 等级门、训练尾（640）是否发生 |
| `LoginOrderAntiMagic` | 1 | `Program.cs` 的 `const int LoginOrderAntiMagic` | 【P】生产：登录序 `RecalcAbilitys()` 把 `AntiMagic` 置 1 后加 `AddAbil.AntiMagic`（`PlayObject.UserLogon` 路径） | AntiMagic 骰门分支（miss/edge/hit） |
| `antiMagic`（FireballCase） | 3 | `RunFireball(..., antiMagic:3)` | 【M】3 是夹具选择（登录序下被覆盖为 1；生成器按生效值选种子） | 仅非登录序/历史 legacy 支 |
| 目标 HP/MaxHP | 100（legacy 合成）；登录序按公式 `MaxHP` | `RunFireball` 的 `target.WAbil.*=100`；`RunMagicCase` 默认 | 【M】生产 HP 由等级/职业公式（`RecalcLevelAbilitys`）；100 是合成夹具 | 伤害/HP 断言、`SM_HEALTHSPELLCHANGED(53)` 的 MaxHP |
| `SoftVersionDateEx`/`ClientTick` | 20240101 / 5 | 两处 scene 构造 | 【M】生产来自客户端登录包；值仅作夹具 | 版本门、客户端 tick 相关分支 |
| `seed` 常量 / `FindSeedForFirstDraw` | 各 stem 固定 seed；扫描 1..200000 | `Program.cs` 各 batch；`FindSeedForFirstDraw` | 【M】生产无"选种子"概念（真实客户端随机）。**约束**：确定性重放 | 分支覆盖（miss/edge/hit）与 RNG 抽数 |
| `probeDrainDraw` 额外抽 | `Random(1000)` 一次 | `RunFireball(..., probeDrainDraw:true)`（seam 自检） | 【M】装置自检专用，生产无 | 仅探针自检，不入 golden |
| `MAGICGOLDEN_CONF_DIR`/`MAGICGOLDEN_MAP_DIR` | 缺省 `E:/MirServer/M2GameSvr` 与其 `Map/` | `Program.cs` 的 `EnsureConfFiles`/`LoadMapData` | 【M】部署目录约定（不在生产源码里）；`EnsureConfFiles` 另核 conf sha 同源 | 配置十键与地图加载；缺源即 PREREQ |
| 单图 `0.map` | 一张 | `env.LoadMapData(path/0.map)` | 【M】生产是多图；装置只加载一张（B6） | 跨图/换图/`FindMap` 类路径（B6） |
| `frame_order_scope` | `multi_victim_same_tick`（fire/skill10 + 施毒 4 支） | golden `Fixture`；比较器白名单 | 【M】生产无"同刻多重集"概念，是放宽策略；白名单写死并配负例牙（`tooth_poison_marker_whitelist_rejects_foreign_stem`） | 跨受害者帧序（多重集比较） |
| `sort_sm_ident` / "tie 序不入判据" | ident=716 排序；注释称"泵序是 4 叉堆 artifact，不是语义序" | `magic_fireball_parity.rs` 的 `sort_sm_ident` | 【K】生产确有确定的堆序（.NET `PriorityQueue` + oracle `GetMessage`）；"不是语义序"是**我们的策略判断**，没有生产出处；B24 已定跨 actor 序为契约 | 施毒 716 等优先级帧的 tie 序 |
| `DUE_MS_TOLERANCE_MS` | 32 ms | `crates/shared`（比较器/驱动共用） | 【M】输入有生产出处（`Environment.TickCount` 15.6ms 粒度），"2×"是本仓选择（见 `production-tick-cadence.md` §11） | 仅 `ident==640` late 帧的 `due_ms_rel` |
| `SendRefMsgRange` | 12 | `Program.cs` 摆位依赖（未显式设 ⇒ 用生产值） | 【P】生产：`GameSvrConf.SendRefMsgRange=12`（`[Setup] SendRefMsgRange` 可配） | AOI 集合（obs_in/obs_out 的 ±12 边界） |
| `.NET PriorityQueue` arity / 优先级 | 4 叉堆；self-delay=255 | Rust `SelfDelayQueue` 复刻；装置 `MsgQueue` | 【P】生产库：`System.Collections.Generic.PriorityQueue`（.NET 8，Arity=4）+ `MessagePriority.Lowest`（`BaseObject.SendSelfDelayMsg` 入队参数）。**注**：arity 的锚在 .NET 库，不在本仓源码；本仓 `SelfDelayQueue` 注释与 `pr1_*` 对表是转述 | 同优先级 tie 序、延迟消息泵行为（B17/B23） |
| `SpellFrameRig` 世界/起点 | 默认 64×64、caster(10,10)；golden 对齐支另设 | `lib.rs::SpellFrameRig::new/new_layout` | 【M】生产真实地图；注释自述"合成图只需覆盖 golden 几何 + 行走步长"（行为输入） | 边界钳制/走位/穿透线 |
| `id_base` 约定（驱动） | caster=base+1、第 i 观察者=base+2+i；预置 slave=base+50 | `lib.rs::SpellFrameRig::new_layout` 注释与实现 | 【M】生产 ActorId 随机；**约束**：与 C# golden 分段同值（ActorId 进 `SM_MAGICFIRE` 尾串） | 含 ActorId 的帧与 golden 对拍 |
| `CLICK_NPC_TIME_MS` | 1000 ms | `lib.rs` 驱动点 NPC 路径 | 【P】生产：`GameSvrConf.ClickNpcTime = 1000`；消费点 `PlayObject.Base.cs::ClientClickNpc` | 点 NPC 频率/两帧间隔 |
| `SOC_CHECK_TIMEOUT_MS` | 50 ms | `lib.rs` 测试等待 | 【M】测试专用，生产无 | 仅测试时序 |
| `SEED` 常量（驱动） | 20261027/20261012/20261023/20_261_023 等 | `lib.rs` 各 parity 测试 | 【M】生产无固定种子；**约束**：确定性重放 | 分支/抽数 |
| AoE 半径缺省 | `fire_boom_rage`/`snow_wind_range`/`elec_blizzard_range` = oracle `GameSvrConf` 缺省（1770-1772） | `lib.rs::SpellFrameRig` 字段注释 | 【P】生产：`GameSvrConf` 对应三项 | AoE 命中范围 |
| `amulet_shape` 缺省 | 5（= nType=1 组） | `lib.rs::SpellFrameRig` 字段；施毒 6/48 的 `CheckAmulet(1,2)` 要求 ≤2 | 【M】生产形状来自 `StdItem.Shape`；5 是夹具缺省（施毒支由 golden `Fixture.amulet_shape` 覆盖） | `CheckAmulet` 分支 |
| socket/账户命名 | 911、`u{i}` 等 | `lib.rs::SpellFrameRig` 会话构造 | 【M】测试装置约定 | 帧归属/收件人 |
| `SITE_ALLOWLIST` | 8 条路径前缀 + 理由 | `magic_fireball_parity.rs` | 【M】治理约定（扩充须评审）；生产无 | RNG site 校验 |
| `keep = {17,638,642}` | sinsu 等 stem 的多重集白名单 | `magic_fireball_parity.rs` 的 `sent_filter`/`sent_filter_sinsu` | 【M】排除 640 有登记（RNG-payload 偏移：tag golden 2 vs rust 3） | 640 训练尾帧不在比较面 |
| 多重集排序比较 | 按 `(ident,param,tag,series,tail)` 排序 | `sent_filter*`、`real_ledger_verify_labels` | 【M】帧序不入判据（与 B24 契约配套）；生产帧序真实存在 | 跨帧顺序不可见 |

## 2. 三类统计

- **【P】有生产出处：5 条**——`LoginOrderAntiMagic`、`SendRefMsgRange`、.NET `PriorityQueue`/`Lowest`、`CLICK_NPC_TIME_MS`、AoE 半径缺省。
- **【M】无出处但只是建模约定：26 条**——每条都在表内写明了"约束什么"（窗口、上界、几何、夹具值、白名单等）。
- **【K】无出处且被写成生产事实：2 条**——见 §3，下一批处理。

## 3. K 类（下一批要契约化/改写）

1. **`StepDrainMs = 100`**：装置注释称"与生产逐 tick 交付同序"，但生产 actor 拍 ≈250–300ms（`production-tick-cadence.md` §0/§9）⇒ 该句没有出处。**处置方向已定（B23）**：保留 100ms 作为共享驱动节拍，但所有时间类结论必须带 `@100ms 共享驱动`。
2. **`sort_sm_ident` / "等优先级 tie 序是 artifact、不是语义序"**：生产有确定的堆序（.NET `PriorityQueue` + `GetMessage`），"不是语义序"是**我们的策略判断**。**处置方向（B24）**：把跨 actor/同优先级序写成 `@byWho 驱动契约`，并在契约里写死 `VisibleActors` 迭代序；不要再把它说成"生产如此"。

## 4. 边界与备注

- `DUE_MS_TOLERANCE_MS=32` 不再重做，引用 `production-tick-cadence.md` §11（含 16–48ms 亚格点盲区与"容差内 = 没问题"是错的）。
- `SendRefMsgRange=12` 的生产锚见 `GameSvrConf`；AOI 非纯几何的缓存/边界口径见 **B25**。
- 本表只覆盖"看起来像约定"的常量；**夹具值（等级/HP/SC/MC/HitSpeed）的出处是"夹具选择"**，不是生产数据——若某条目把它们当生产事实写进 scope，属于 K 类，审卡时要改。
