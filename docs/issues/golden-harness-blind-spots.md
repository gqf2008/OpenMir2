# MagicGolden 装置盲区总表（2026-11-21，只读调查）

> **判据（先记住这句）：“装置看不见” ≠ “oracle 不发”。**
> 遇到「Rust 少了某帧」，**先查它是不是落在下表某个盲区里**，再判断是不是产品缺陷。
> 本文只登记**装置结构**导致的静默不可观测；不改装置代码。

## 1. 盲区表

| # | 装置结构 | 触发条件 | 静默方式 | 生产里会发生什么 | 装置里不可观测的面 |
| --- | --- | --- | --- | --- | --- |
| B1 | **替身怪未绑定世界**：`OfflineWorldEngine.RegenMonsterByName` 只设身份/HP/Race，**不设 `Envir`、不进 `ActorMgr`、不 `AddMapObject`** | 任何走 `WorldEngine.RegenMonsterByName` 的路径（`MakeSlave`：17/30/41） | 该怪的 `SendRefMsg` 首句 `if (Envir == null) return;` **早退**（`src/M2Server/Actor/BaseObject.Message.cs:383` 的 `SendRefMsg` 开头，`Envir == null` 早退） | `BaseObject.MakeSlave`（`src/M2Server/Actor/BaseObject.cs:1179`）在收尾调 `monObj.RefNameColor()`（`src/M2Server/Actor/BaseObject.cs:730`） ⇒ `RM_CHANGENAMECOLOR` 发给 ±12 格（含旁观者） | **656**（17/30/41 召唤的改名色帧）与任何"怪作为发送者"的 RefMsg；且该怪不在格子 `ObjList` ⇒ `GetMovingObject`/视野类看不见它 |
| B2 | **`RecordingMonster` 是被动壳、不 `Operate`**：只暴露 `DrainRaw`（`Program.cs` 的 `RecordingMonster` 类）；主排水只记录它的 RM 账本（同文件 `:2895` 的 `mo.DrainRaw()`），**从不处理** | 任何 `SendMsg/SendSelfDelayMsg` 发给**怪物**的消息 | 消息进 `MsgQueue` 后**永不出队处理** | 怪物自己的处理器会产生的帧/状态（例：`RM_MAKEHOLYSEIZEMODE`(60005) 的处理器） | **60005 的下游**（20 诱惑术在 `src/M2Server/Magic/MagicManager.cs` 的 `MagTamming` 内部对怪 `SendMsg`；以及 54 族对目标发的同名 RM）；怪物受击/中毒/状态类下游 |
| B3 | **克隆体 `FixedHideMode=true` 且不登录**：`PlayObject` ctor 置 `FixedHideMode=true`（`src/M2Server/Player/PlayObject.cs:993`），仅登录支复位（`:1442`）；`PlayCloneObject` ctor（`src/M2Server/Player/PlayCloneObject.cs:8`）`SendRefMsg(RM_TURN)` 时仍是隐藏态 | 46 分身术（`MagMakeClone` → `new PlayCloneObject`） | `SendRefMsg` 的 `if (ObMode || FixedHideMode) { SendMsg(…); return; }` 支 ⇒ **只发给克隆体自己**；而克隆体队列无人排空 | 旁观者收 `SM_TURN(10)`/`SM_FEATURECHANGED(41)` | **克隆体出生帧 10/41**；任何"克隆体作为发送者/接收者"的帧 |
| B4 | **`VisibleActors` 不自建**：生产由 `UpdateVisibleGay` 填充；装置只在**推挤批**显式回填（`Program.cs` 的 push-batch 段 的 push-batch 段） | 依赖 `VisibleActors` 的效果（现：8/37 `MagPushArround`；将来新增者） | 表为空 ⇒ 目标集合为空 ⇒ **零目标、零帧、零抽** | 按 AOI 可见集选择目标 | 未显式回填的支：全部"按可见集选目标"的效果面 |
| B5 | **只排水四个玩家**：`byWho = {caster,target,obs_in,obs_out}`（`Program.cs` 的 `byWho` 构造 与 另一处 `byWho` 构造） | 任何第三方 actor（怪/克隆/预置 slave/后续召唤物）的队列 | 队列不排 ⇒ 其到点 RM 永不处理 | 该 actor 的 `Run()` 会处理自身队列 | 所有"由第三方 actor 到点触发"的帧与状态 |
| B6 | **只加载一张图**：`env.LoadMapData(map/0.map)`；`OfflineMapSystem.FindMap` 只匹配 `env.MapName`，且 `MapName` 仅在 `EnableMaps` 支设置 | `MapRandomMove`/`HomeMap`/跨图/`FindMap(name)` 类路径 | `FindMap` 返回 null ⇒ **不移动、不抽、无帧** | 真图查找成功后移动/换图并出帧 | 21 瞬息移动的落点与 633/634/807 换图簇（除显式 `EnableMaps` 支） |
| B7 | **`StartPointList` 置成空表**（`Program.cs` 启动段的 `StartPointList` 初始化 起 `new List<StartPoint>()`，只为避免 NRE） | 取出生点/回城点/随机起点类路径 | 查表得 null/空 ⇒ **静默无动作** | 按 StartPoint.txt 选点 | 依赖出生点的位移/回城类效果 |
| B8 | **`ItemSystem` 最小**：仅 `ArmRingAmulet` 支临时塞一个护身符（`Program.cs` 的 `ArmRingAmulet` 支 `GameItemSystem` 的 `new GameItemSystem()`/`AddItem`） | 任何 `GetStdItem(index)` 查非该护身符的物品 | 返回 null ⇒ 依赖物品表的分支**跳过** | 按物品表取 StdMode/Shape/属性 | 非护身符物品驱动的效果（装备/药品/特戒）；`UseItems` 其余槽 |
| B9 | **全局子系统最小替身**：`CastleMgr` 空城（`Program.cs` 启动段的 `CastleMgr` 初始化）、`EventMgr` 空事件表（同文件 `:110`）、`WorldEngine` 只有 `RegenMonsterByName` 真实现（其余 throw；只读成员返回 0/null） | 城堡区/事件/怪物生成/查询类路径 | 城堡判定恒 false、事件表恒空、查询恒 0/null | 城战/事件帧、真实怪物表查询 | 城堡与事件产生的帧；`GetPlayObject`/`MonsterCount`/`FindMagic` 等**静默空集**（注意与"throw 成员"的 loud 失败相反） |
| B10 | **配置吃 ctor 缺省**：仅施毒批显式 `ServerConf.LoadConfig()`；其它支只 `ExpConf.LoadConfig()` | 任何读 `SystemShare.Config` 的阈值/开关（`MagTurnUndeadLevel`、`MabMabe*`、三个 AoE range、`Dragon*` 等） | 缺省值 ≠ 部署值时，**静默走另一支** | 部署值（`Server.conf [Setup]`） | 阈值边界附近的命中/未命中（例：32 圣言术、50 魔爆、23/33/24/47 半径）。**实例**：MG-48 `MabMabe*` 部署值恰等于 ctor 缺省 ⇒ "读配置"与"吃缺省"行为等价（该维度不可区分）；MG-44 `MagTurnUndeadLevel` 部署 44 ≠ 缺省 50 ⇒ 该维度**可区分**（"叶子吃的是哪份配置"本身是覆盖面） |
| B11 | **不跑 `Run()`**：装置只 `Operate` 队列消息，不推进 actor 的 `Run` | 依赖时间推进的效果（毒 DoT、`IncHealing` 回血、buff/状态到期、体力/耐力恢复） | 计时器不递减 ⇒ 窗口内**无后续帧/无 HP 变化** | 每 tick 的 `Run` 结算 | 毒伤/回血/状态到期的**过程面**（`TargetHp` 在窗口内常不变） |
| B12 | **RNG 账本只覆盖单例**：`RngSeam` 反射替换 `OpenMir2.RandomNumber` 的私有静态 `random`（`Program.cs` 的 `RngSeam`） | 任何绕开该单例的随机源（`new Random()`、外部库） | 抽数**不进账本** | — | 若某路径绕开单例：RNG 对比面缺项（当前全仓 `new Random(` 0 命中，风险为将来） |
| B13 | **`RecordingNetChannel` 只记两条入队口**：`AddGateBuffer`/`AddGameGateQueue`（`Program.cs` 的 `RecordingNetChannel` 入队口）；`Send/Close/Kick` 等为空实现（同文件 `:3353` 起） | 非 `SendSocket`/`SendDefMessage` 通道的帧 | 不进 `Buffers` ⇒ **不可见** | 经相应通道真正下行 | 走这些通道的任何帧（现网 magic 面不涉，属结构性限制） |
| B14 | **`ObMode`/`FixedHideMode` 需每支显式复位**：装置只在「正常玩家」处显式置 false（`Program.cs` 启动段的 `FixedHideMode=false` 与 `:2408`） | 新增 case 忘记复位（ctor 可能为 true） | 同样命中"只发自己"支 ⇒ **旁观者零帧** | 登录后复位 | 新支的旁观者帧（与 B3 同源，但成因是"忘记复位"） |
| B15 | **等级门判据的夹具灵敏度极低**（16 圣言术/`holyshield`）：`cast_holy_curtain_spell` 的等级门 `draw + (caster_level - 1) > obj.level`（`mir2-rs/crates/world-bevy/src/world.rs`） | 任何**小幅度**修改该门的变异 | 判据 `draw + (caster_level - 1) > obj.level`，`draw = Random(4) ∈ [0,3]`；本 case 怪 `level=200`、caster 30 ⇒ 最坏 `draw=0` 时需 `caster_level ≥ 202`（**至少 +172** 才可能翻转）⇒ +1/+50 之类的小幅度变异**结构性不可观测** | 等级门决定 `RM_MAKEHOLYSEIZEMODE` 是否下发 | **本 case 的真因就是等级门**（`master=0`、目标格 objects 非空——原“master 门/objects 为空”归因**错误**，由 `bd` 于 2026-11-21 复核纠正）；**决定性变异可红**：删等级比较 （`if obj.master.is_none()`）⇒ 驱动与比较器双层红（见 MG-23 scope）。**纪律**：判“不可观测”前先用公式算出最小翻转幅度；算不出/没试到就只能写“已试幅度与所需幅度” |

## 2. 已知受影响的面（与具体 leaf 对号）

- **656（`RM_CHANGENAMECOLOR`）**：17/30/41 的召唤怪改名色 —— 落在 **B1**；20 的诱惑（`MagTamming` 内对怪发 60005）与 54 族的 60005 下游 —— 落在 **B2**。
- **克隆体出生帧 `10`/`41`（46）** —— 落在 **B3**（`FixedHideMode` 未复位 + 队列无人 drain）。
- **时间推进类**（毒 DoT/回血/状态到期）—— 落在 **B11**；`TargetHp` 不变不等于"oracle 不扣"。
- **按可见集选目标类**（8/37 之外的将来效果）—— 落在 **B4**。
- **按物品表/城堡/事件/配置阈值分叉的效果** —— 落在 **B8/B9/B10**。

## 3. 遇到疑似缺失帧时的排查顺序（**顺序很重要**）

1. **先查本卡 15 条盲区**：该帧的发送者/目标是否 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11？
2. **再查队列模型**：`docs/issues/openmir2-refmsg-queue-model.md`（oracle 全入队 vs Rust 直发 ⇒ 跨 actor 次序/可见面的系统性差异）。
3. **再查比较器是否把它滤掉了**：`keep` / `sent_filter` / `sent_filter_sinsu` 白名单、`frame_order_scope` 放宽、`due` 容差——**先确认这条帧在判据面内**。
4. **最后才判产品缺陷**（并按 §1 的 R/L 分级与 §3 的牙要求取证）。

> 今天三次「少了帧」：**两次是盲区（B1/B2/B3 族）、一次是队列模型**，**没有一次是产品缺陷**。顺序反过来就会白改产品。

## 4. 使用法

1. 看到「Rust 少帧/少抽」时，先按上表逐行问："该帧的发送者/目标是不是 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11？"
2. 命中盲区 ⇒ 记为**装置限制**（进 scope 或登记卡），**不要**当成产品缺陷去改实现。
3. 需要把盲区变可见时，改装置的方向（另开卡）：替身怪绑定 `Envir`+`ActorMgr`+`AddMapObject`；对被动怪实现最小 `Operate`；克隆体复位 `FixedHideMode` 并把其队列纳入排水；对 `VisibleActors` 按 AOI 回填；显式装载 `ServerConf`；跑必要 tick 的 `Run`。

## 5. 与既有登记的对照（避免同一件事两处记）

| 盲区 | 已在别的卡登记？ |
| --- | --- |
| B1 替身怪未绑世界（`MakeSlave` → `RefNameColor`） | `docs/issues/mg42-angel-stub-impl.md`（41 支的替身/改名色面）；本卡补 17/30 的同源面 |
| B2 `RecordingMonster` 不 `Operate` | `docs/issues/device-gap-premise-audit.md`、`docs/issues/magic-observe-surface-plan.md` |
| B3 克隆体 `FixedHideMode` | `docs/issues/device-gap-premise-audit.md` §1.3、`docs/issues/magic-mgs-34-36-55-batch-impl.md`（MG-55）、`docs/issues/magic-observe-surface-plan.md` |
| B4 `VisibleActors` 未回填 | `docs/issues/device-gap-premise-audit.md` §1.2、`docs/issues/magic-push-batch-impl.md` |
| B5 只排水四名玩家 | `docs/issues/magic-observe-surface-plan.md` |
| B6 单地图 / `FindMap` | `docs/issues/openmir2-golden-harness-mapmgr.md` |
| B7 `StartPointList` 空表 | `docs/issues/magic-mgs-34-36-55-batch-impl.md`（MG-36） |
| B8 `ItemSystem` 最小 | `docs/issues/openmir2-items-parity.md`、`docs/issues/openmir2-magic-parity.md` |
| B9 子系统最小替身 | `docs/issues/openmir2-magic-parity.md`、`docs/issues/magic-observe-surface-plan.md`、`docs/issues/openmir2-changemap-missing-serverconfig.md`（`GetPlayObject`） |
| B10 配置吃缺省 | `docs/issues/magic-serverconf-batch-plan.md` / `-impl.md`（11029）、`docs/issues/device-gap-premise-audit.md` |
| B11 不跑 `Run()` | `docs/issues/magic-observe-surface-plan.md` |
| B12 RNG 单例覆盖 | `docs/issues/openmir2-golden-fixture-drift.md`、`docs/issues/openmir2-magic-parity.md` |
| B13 `RecordingNetChannel` 覆盖 | `docs/issues/openmir2-magic-parity.md`、`docs/issues/magic-52-clientxy-explicit-case.md` |
| B14 `ObMode`/`FixedHideMode` 待复位 | `docs/issues/notes-comments-that-lie-20261121.md`、`docs/issues/openmir2-hidden-npc-world-entities.md` |

## 6. 关联

- `docs/issues/openmir2-golden-generator-drift.md`（生成器世代漂移，另一类"看不见"）
- `docs/issues/groupamyounsul-nondegenerate-golden.md`（退化守卫：效果体零次执行的具名红）
- 卡 `openmir2-golden-harness-mapmgr`（`OfflineMapSystem`/`MapMgr` 的建立与边界）
