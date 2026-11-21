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
| B16 | **魔表口径：装置用零值 `MagicInfo`，部署魔表未覆盖**：C# `MagicGolden/Program.cs` 的 `new OpenMir2.Data.MagicInfo { MagicId, MagicName, Effect, EffectType }`（其余字段默认 0）；Rust `fireball_rig` 同 `..mir2_data::MagicInfo::default()` | 任何读 `TrainLv`/`DefPower`/`DefMaxPower` 的魔号（`GetPower13`/`GetPower` 等） | 装置侧 `TrainLv=0`、`DefPower=DefMaxPower=0`，与部署口径「`src/GameSrv/DB/MySqlDB.cs` 的 `LoadMagicDB` 恒置 `TrainLv=3`、`sql/mir2_data.sql` MagID 48 行 `DefPower=4/DefMaxPower=4`」**不同源**；同一参数代入：**装置口径 P(30)=80（golden 值）、部署口径 P(30)≈54** | 部署魔表实数下的取整/阈值/倍率分支 | **部署口径下的行为**：凡读这三个字段的魔号，「两侧一致」只在**零值口径**下成立 |
| B17 | **延迟消息排水粒度：装置全量逐条 vs 生产「遇未到期队首即停」**：装置 `MagicGolden/Program.cs` 的 `DrainOnce` 先 `DrainRaw()` 取**全部**消息 → 逐条判 `DeliveryTime <= now`（到期 `OperatePublic`、未到期收进 `later` 后 `PushBack` 回 `MsgQueue`）⇒ **一步内处理所有到点消息、不在首个未到期项处停**；生产对照 `ActorEntity.MsgQueue`=`PriorityQueue<SendMessage,byte>`（**只按 priority**），`BaseObject.Message.cs` 的 `GetMessage`：`TryDequeue` → 若 `DeliveryTime > now` ⇒ `Enqueue(Lowest)` + `return false`（**遇未到期队首即停**） | 同一 cast 产生 ≥2 条延迟 self-delay（如 50 魔爆 `power/3`+`n_power` 两条 `RM_DELAYMAGIC`@600）+ 同队列里一条**更晚到期**项（`RM_POISON`/`RM_MAGIC_LVEXP`）并存 | 装置不看「队首是否到期」，把**所有**到点消息同刻吐出 ⇒ golden 的 `due_ms_rel`/出队顺序按**装置语义**生成 | `GetMessage` 遇未到期队首即 `Lowest` 重入队 + 停 ⇒ 已到点但排在其后的消息被**推到下一 tick**（Rust 泵 `deliver_self_delay_msgs_impl` 同构、忠实复刻） | 生产的「遇未到期队首即停/延后」；多延迟场景 golden 的帧时序/顺序**不代表生产**。**实例**：50 魔爆成功臂（`openmir2-mabmabe-success-arm-parity`）golden 两条 600 同刻出队（装置全量语义），Rust 生产分 tick（700/800）⇒ 曾误判为「产品泵缺陷」，`89` 复核纠正为**装置缺口**（判据 `fixture-self-consistency-is-not-parity`：**装置内部自洽 ≠ 与生产一致**）。**处置**：以生产为准 ⇒ 改 `DrainOnce` 为「遇未到期队首即停」，重出受影响 golden，再复核受影响条目（`89` 列出 6 个 stem） |
| B18 | **committed golden 与生成器不同步（schema 漂移）**：当前 master 的 `MagicGolden/Program.cs` 对**任意** stem 的 `cases[].Fixture` 多吐 8 个键（`amulet_shape`/`amy_ounsul_point`/`target_anti_poison`/`you_poisoned`/`red_msg_f_color`/`red_msg_b_color`/`caster_name`/`slave_colors`），committed golden 多数没有 | 用当前 harness 重出任意 golden | schema 多键：`regen vs committed 逐字节（剥 delivery）`会**全 stem 假红**（hbfireball pilot：24 处 diff **全是**这些 Fixture 多键、0 处语义） | committed golden 与当前生成器**不同步** ⇒ ①“重出 golden”在本树**默认产生 schema 漂移**；②“committed golden 能被当前代码复现”这一隐含假设**不成立** | **量化（60 支）**：全 8 键有 **5 支**（`fireball-nonzero`、`lightflower-nonzero`、`lightning-11-nonzero`、`mabmabe-nondefault`、`tamming`）；**全缺 8 键 42 支**（`bigcloak`、`bighealing`、`cloak`、`dejiwonho`、`fire`、`fireball-5`、`fireball`、`fireboom`、`fireboom47`、`firecharm`、`groupdeding`、`grouplightening`、`hangmajinbub`、`hbfireball`、`healing`、`holyshield`、`lightflower`、`lightning-11`、`lightning-45`、`shield`、`showhp`、`sinsu`、`skeleton`、`skill10`、`skill49`、`skill51`、`skill52`、`skill53`、`skill54`、`snowwind`、`uenhancer`、`unamyounsul`、`warr-12`、`warr-25`、`warr-26`、`warr-27`、`warr-3`、`warr-34`、`warr-38`、`warr-4`、`warr-43`、`warr-7`）；**部分 13 支**（`amyounsul`、`amyounsul2`、`angel`、`clone46`、`energyrepulsor`、`firecross`、`firewind`、`groupamyounsul`、`groupamyounsul2`、`mabmabe`、`spacemove`、`turnundead`、`windtebo`；均缺 `caster_name/slave_colors`）。**风险判定**：这 8 键的 `golden_fixture_required` 读取**全部 gated 到拥有该键的 stem**（poison 4 支：`amulet_shape`/`target_anti_poison`/`amy_ounsul_point`/`red_msg_*`/`you_poisoned`；`firecross_parity_drive`：`red_msg_*`；`caster_name`/`slave_colors` **无 required 读**）⇒ **当前无驱动因缺键 panic**，55 支陈旧 golden 只是少记了未用输入。**处置**：需要 schema 干净的批次用**快照生成器**（如 B17 用 `c31580a57` 生成器 + 单点改动，避免“排水语义 + schema”两因混淆）；全量重生成是另一批决策。**后续动作（另立批，不并 B17）**：把 55 支 golden 全量对齐当前 schema = 独立批（一批只允许一个自变量，混合则 B17 后异常无法归因）；建议**按被读取性分组**——`caster_name`/`slave_colors`（完全未读）一批、被 gated 读的前 6 键一批。 |
| B19 | **装置墙钟非确定性 ⇒ golden 不可逐字节复现**：步进排水 `DrainOnce` 用 `Thread.Sleep(step)` + **实时** `GetTickCount()` 判到期；sleep 过冲使「同刻 due 的多条 self-delay 消息本步是否到期」翻转 ⇒ 处理顺序翻转 | 同 cast 多条**同刻/近同刻**延迟消息（`mabmabe-nondefault` 600/600；施毒 4 支；多受害者） | 3 轮 pristine 独立 regen 即轮间抖动：`due_ms_rel` ±15/16ms（TickCount 粒度）+ **顺序级** `Rm[i].ident` 换位；**14 支命中**：`mabmabe-nondefault`/`amyounsul`/`amyounsul2`/`groupamyounsul`/`groupamyounsul2`/`clone46`/`fireboom47`/`firecross`/`holyshield`/`lightflower`/`lightflower-nonzero`/`skill10`/`skill53`/`windtebo` | 生产同样用 `.NET` 实时钟，但装置重放需**可复现**；注入虚拟时钟是正解 | **边界**：比较器靠 `Rm` **多重集**比较 + `due_ms_rel` **32ms 容差**吸收 ⇒ **既有对拍仍通过**；但「逐字节对比 golden」在这 14 支上不可靠 ⇒ **凡逐字节比 golden 必先噪声地板**。根因批 = 虚拟时钟注入（`openmir2-firecharm-nondeterminism` 的正解）。**修法落地（2026-11-21）**：生产 `HUtil32` 加**惰性时钟接缝** `_tickOverride`（私有静态、镜像 `RngSeam` 反射设、默认 null ⇒ 生产逐字节等价；理由 `Environment.TickCount` 无字段可反射故须加 seam）；装置经 `ClockSeam` 注入虚拟钟、`Thread.Sleep(step)`→`ClockSeam.Advance(step)`。**行为牙** `--clock-seam-selftest` 绿（忽略 override ⇒ 该牙红）。小范围证：`hbfireball`/`healing` 连跑 3 次**逐字节相同（含 delivery）**。 |


## 2. 已知受影响的面（与具体 leaf 对号）

- **656（`RM_CHANGENAMECOLOR`）**：17/30/41 的召唤怪改名色 —— 落在 **B1**；20 的诱惑（`MagTamming` 内对怪发 60005）与 54 族的 60005 下游 —— 落在 **B2**。
- **克隆体出生帧 `10`/`41`（46）** —— 落在 **B3**（`FixedHideMode` 未复位 + 队列无人 drain）。
- **时间推进类**（毒 DoT/回血/状态到期）—— 落在 **B11**；`TargetHp` 不变不等于"oracle 不扣"。
- **按可见集选目标类**（8/37 之外的将来效果）—— 落在 **B4**。
- **按物品表/城堡/事件/配置阈值分叉的效果** —— 落在 **B8/B9/B10**。
- **多延迟同 cast 的帧时序/顺序（实例：50 魔爆成功臂）** —— 落在 **B17**。
- **committed golden 可复现性（生成器 schema 漂移）** —— 落在 **B18**。
- **装置墙钟非确定性（golden 不可逐字节复现）** —— 落在 **B19**；根因批=`openmir2-firecharm-nondeterminism`。

## 3. 遇到疑似缺失帧时的排查顺序（**顺序很重要**）

1. **先查本卡 19 条盲区**：该帧的发送者/目标是否 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11/B16/B17/B18/B19？
2. **再查队列模型**：`docs/issues/openmir2-refmsg-queue-model.md`（oracle 全入队 vs Rust 直发 ⇒ 跨 actor 次序/可见面的系统性差异）。
3. **再查比较器是否把它滤掉了**：`keep` / `sent_filter` / `sent_filter_sinsu` 白名单、`frame_order_scope` 放宽、`due` 容差——**先确认这条帧在判据面内**。
4. **最后才判产品缺陷**（并按 §1 的 R/L 分级与 §3 的牙要求取证）。

> 今天三次「少了帧」：**两次是盲区（B1/B2/B3 族）、一次是队列模型**，**没有一次是产品缺陷**。顺序反过来就会白改产品。

## 4. 使用法

1. 看到「Rust 少帧/少抽」时，先按上表逐行问："该帧的发送者/目标是不是 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11/B16/B17/B18/B19？"
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
| B16 魔表零值口径 | 无（本卡新立）；影响面见 `docs/issues/parity-gap-magic.json` 的 MG-65 `scope` |
| B17 延迟消息排水粒度（装置全量逐条 vs 生产遇未到期队首即停） | 本卡新立；实例见 `docs/issues/openmir2-mabmabe-success-arm-parity.md` 与 `89` 的 6 stem 复核 |
| B18 committed golden / 生成器 schema 漂移 | 与 `docs/issues/openmir2-golden-generator-drift.md` 同源（本卡给量化与风险判定）；B17 的快照生成器口径即为此处的隔离手段 |

## 6. 关联

- `docs/issues/openmir2-golden-generator-drift.md`（生成器世代漂移，另一类"看不见"）
- `docs/issues/groupamyounsul-nondegenerate-golden.md`（退化守卫：效果体零次执行的具名红）
- 卡 `openmir2-golden-harness-mapmgr`（`OfflineMapSystem`/`MapMgr` 的建立与边界）
