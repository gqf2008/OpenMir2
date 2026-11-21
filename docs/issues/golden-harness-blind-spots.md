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
| B18 | **committed golden 与生成器不同步（schema 漂移）**：当前 master 的 `MagicGolden/Program.cs` 对**任意** stem 的 `cases[].Fixture` 多吐 8 个键（`amulet_shape`/`amy_ounsul_point`/`target_anti_poison`/`you_poisoned`/`red_msg_f_color`/`red_msg_b_color`/`caster_name`/`slave_colors`），committed golden 多数没有 | 用当前 harness 重出任意 golden | schema 多键：`regen vs committed 逐字节（剥 delivery）`会**全 stem 假红**（hbfireball pilot：24 处 diff **全是**这些 Fixture 多键、0 处语义） | committed golden 与当前生成器**不同步** ⇒ ①“重出 golden”在本树**默认产生 schema 漂移**；②“committed golden 能被当前代码复现”这一隐含假设**不成立** | **量化（60 支）**：全 8 键有 **5 支**（`fireball-nonzero`、`lightflower-nonzero`、`lightning-11-nonzero`、`mabmabe-nondefault`、`tamming`）；**全缺 8 键 42 支**（`bigcloak`、`bighealing`、`cloak`、`dejiwonho`、`fire`、`fireball-5`、`fireball`、`fireboom`、`fireboom47`、`firecharm`、`groupdeding`、`grouplightening`、`hangmajinbub`、`hbfireball`、`healing`、`holyshield`、`lightflower`、`lightning-11`、`lightning-45`、`shield`、`showhp`、`sinsu`、`skeleton`、`skill10`、`skill49`、`skill51`、`skill52`、`skill53`、`skill54`、`snowwind`、`uenhancer`、`unamyounsul`、`warr-12`、`warr-25`、`warr-26`、`warr-27`、`warr-3`、`warr-34`、`warr-38`、`warr-4`、`warr-43`、`warr-7`）；**部分 13 支**（`amyounsul`、`amyounsul2`、`angel`、`clone46`、`energyrepulsor`、`firecross`、`firewind`、`groupamyounsul`、`groupamyounsul2`、`mabmabe`、`spacemove`、`turnundead`、`windtebo`；均缺 `caster_name/slave_colors`）。**风险判定**：这 8 键的 `golden_fixture_required` 读取**全部 gated 到拥有该键的 stem**（poison 4 支：`amulet_shape`/`target_anti_poison`/`amy_ounsul_point`/`red_msg_*`/`you_poisoned`；`firecross_parity_drive`：`red_msg_*`；`caster_name`/`slave_colors` **无 required 读**）⇒ **当前无驱动因缺键 panic**，55 支陈旧 golden 只是少记了未用输入。**处置**：需要 schema 干净的批次用**快照生成器**（如 B17 用 `c31580a57` 生成器 + 单点改动，避免“排水语义 + schema”两因混淆）；全量重生成是另一批决策。**后续动作（另立批，不并 B17）**：把 55 支 golden 全量对齐当前 schema = 独立批（一批只允许一个自变量，混合则 B17 后异常无法归因）；建议**按被读取性分组**——`caster_name`/`slave_colors`（完全未读）一批、被 gated 读的前 6 键一批。**虚拟钟后重同步注意**：重出的 `delivery` 与 `due_ms_rel` 都会与**旧 golden 不同**（旧的是墙钟抖动/`castTick` 偏移产物）⇒ diff 比本卡预估大，**须逐 stem 分类（schema 键 / due 值 / 语义）**，不能只看非零差异数。另：`mabmabe-nondefault` 是 **"Y 桶"里唯一由 B17 引起的**（另外两个 Y：`uenhancer`（**装置建模缺口 B21**：fixture 意图 `CasterSc=15` 被登录序 `RecalcAbilitys` 用 `Abil.SC=0` 覆盖，非"丢开关"）、`clone46`（**B20**：产品 `Clone.actor_id` 非确定）），其 31 帧 `tail` 的 `CharStatus` 位必随 B17 重出（bit 级解码见 `docs/issues/openmir2-mabmabe-success-arm-parity.md` §4）。  **（重同步·provenance）`config.repo_head` 记的是生成时 git HEAD ⇒ 跨提交重出必变、属**漂移产物**：**比对时剥离，但保留在 golden 里作 provenance（不据它判语义差异）**。**分类器必须**显式剥离** `repo_head`（否则每次跨提交重出必报差异——这是**操作要求、非仅知识**）。|
| B19 | **装置墙钟非确定性 ⇒ golden 不可逐字节复现**：步进排水 `DrainOnce` 用 `Thread.Sleep(step)` + **实时** `GetTickCount()` 判到期；sleep 过冲使「同刻 due 的多条 self-delay 消息本步是否到期」翻转 ⇒ 处理顺序翻转 | 同 cast 多条**同刻/近同刻**延迟消息（`mabmabe-nondefault` 600/600；施毒 4 支；多受害者） | 3 轮 pristine 独立 regen 即轮间抖动：`due_ms_rel` ±15/16ms（TickCount 粒度）+ **顺序级** `Rm[i].ident` 换位；**14 支命中**：`mabmabe-nondefault`/`amyounsul`/`amyounsul2`/`groupamyounsul`/`groupamyounsul2`/`clone46`/`fireboom47`/`firecross`/`holyshield`/`lightflower`/`lightflower-nonzero`/`skill10`/`skill53`/`windtebo` | 生产同样用 `.NET` 实时钟，但装置重放需**可复现**；注入虚拟时钟是正解 | **边界**：比较器靠 `Rm` **多重集**比较 + `due_ms_rel` **32ms 容差**吸收 ⇒ **既有对拍仍通过**；但「逐字节对比 golden」在这 14 支上不可靠 ⇒ **凡逐字节比 golden 必先噪声地板**。根因批 = 虚拟时钟注入（`openmir2-firecharm-nondeterminism` 的正解）。**修法落地（2026-11-21）**：生产 `HUtil32` 加**惰性时钟接缝** `_tickOverride`（私有静态、镜像 `RngSeam` 反射设、默认 null ⇒ 生产逐字节等价；理由 `Environment.TickCount` 无字段可反射故须加 seam）；装置经 `ClockSeam` 注入虚拟钟、`Thread.Sleep(step)`→`ClockSeam.Advance(step)`。**行为牙** `--clock-seam-selftest` 绿（忽略 override ⇒ 该牙红）。小范围证：`hbfireball`/`healing`/`firecharm` 连跑 3 次**逐字节相同（含 delivery）**。**风险边界（④）**：`_tickOverride` 私有但**反射可写**——任何代码（含生产）都能反射设它；**约定：仅 harness 的 `ClockSeam` 可设、生产代码禁止出现对该字段名的反射引用**；将来若要更强隔离再议 `InternalsVisibleTo`。**可行性与建议（⑤）**：虚拟钟后 `delivery` 也确定 ⇒ 归一化「剥 delivery」已**非必要**（3 次不剥也逐字节相同）；下一批可议「把 `delivery` 纳入比较面」以增强判据（本批不改）。**边界（补充）**：虚拟钟治的是**时钟源**；`clone46` 的 `Clone.actor_id`（产品 ActorMgr 分配）是**非时钟的独立非确定源**（见 B20），虚拟钟治不了 ⇒ clone 家族仍不可逐字节，验证止于「比较器绿」；**别把『虚拟钟后』误读成『全稳』**。**覆盖范围点名（2026-11-21，三路径）**：①`RunMagicCase`（绝大多数）**已接**——「13/14 稳定」**只是此路径**的结果；②`RunFireball`（`fireball`）原未接、**已补接**（fireball 3× 逐字节相同）；③`ClientSpellXYEntry.Run`（`warrskill`→10 支 `warr-*`）原未接、**已补接**（10 支各 3× 逐字节相同）。**凡写「XX 已确定」，必须写明在哪些路径上。** |

| B20 | **clone 家族 `Clone.actor_id` 非确定（连虚拟钟也治不了）**：克隆体 ActorId 由产品 `PlayCloneObject`/`ActorMgr` 分配（非 harness 可控的确定源，疑运行时/对象哈希/进程计数） | 生成/比对 `clone` 家族 golden（46 分身术）且触及 `Clone.actor_id`/`rel_to_caster` | 同一装置连跑，`Clone.actor_id` 每次不同（~9e8 量级差）；**虚拟钟（B19）治不了**（已实证 3 次仍不稳） | 生产 ActorId 亦由 `ActorMgr` 分配，但生产不「重放比对」故非问题 | clone 家族 golden **天生不可逐字节比对** ⇒ 验证**只能停在「比较器绿」这一层**；**别把『虚拟钟后』当成『全稳』**——actor-id 是非时钟的独立非确定源 |

| B21 | **夹具意图未物化（`CasterSc/CasterMc` 只写 `WAbil`、不写 `Abil`，被登录序 recalc 覆盖为 0）**：`RunMagicCase` 设 `caster.WAbil.SC=CasterSc`（缺省 15），登录序 `RecalcAbilitys()`（`WAbil=Abil.Clone()`）用 `Abil.SC=0` 覆盖 ⇒ 实际 SC=0，与 fixture 意图 15 不符 | 设 `CasterSc/CasterMc` 却未同步 `Abil.SC/MC` 的 stem（如 `uenhancer`）；golden 记录 `caster.WAbil.SC` | 记录切到「实际应用值」（`9c9782a30`）后**暴露**：`uenhancer` 的 `caster_sc` 15→0；**非 golden 旧、非丢开关**（`CasterScNoClamp` 仍在） | 生产登录序同样 `WAbil=Abil.Clone()`，但 fixture 意图应体现在 `Abil` | `uenhancer` 等 stem 的 SC/MC 意图未物化（实际 0）；**修法（行为性、后续批）**：`CasterSc/CasterMc` 同时写 `Abil.SC/MC` 使 recalc 后仍为意图值；**别在缺口上重出 golden** |

| B22 | **（2026-11-22 全量门复核修正）** 原判「oracle 不表达 per-frame 延迟量、装置给不出正确 `due`」**对部分路径成立**。committed **37/60 支有 `due_ms_rel`**（异构）；`(a)`「不 emit」前提错误已回滚（`d10de6523`）。回滚后 `rerun_magic_plane.sh` 实测（58 支、落 49）：**32 支一致＋`cloak`/`holyshield`/`unamyounsul` 与 Rust 同值；9 支路径特异分歧** —— A 型 harness 记 0（`CurrentDueMs` 在 one-shot/RunFireball 路径未设）：`fireball`/`fireball-5`/`firecharm`/`grouplightening`/`lightning-11`/`lightning-45`/`showhp`；B 型 recog 顺序互换＋due 800 vs 900：`bigcloak`/`groupdeding`（后者并触发比较器 640 减法溢出，另行修复）。**定论：B22 真实、待装置表示层重构；不得据此删字段或调断言**（9 支暂留旧 golden 无 due ⇒ 比较器不比 ⇒ 绿，不在缺口上盖新值）。详见下方 D=12 due 值表 | 延迟帧（31/53/共享尾）的逐帧定时对拍 | golden 无 due ⇒ 比较器不比（无逐帧覆盖）；**有 due 但错**同属 B22（A/B 型） | oracle 逐帧本有延迟量、比较器本可比 | **教训**：`due` 属比较面、不得当 T 剥离（曾致「57 支 Y=0」假绿）。**后续（正式批）**：表示层重构（`due` 沿链传播到下游帧）后再逐支落盘 |
| B23 | **对拍的时间节拍是假设值（100ms 未核实）**：装置 `MagicGolden/Program.cs::DrainOnce` 的步进 `StepDrainMs=100` 与 Rust 驱动都用 100ms；生产 actor `Run()` 由 `RunTime=250` 门 + 玩家 `UserProcessor` 50ms 周期 / 怪物 `ProcessMonsters` 每趟 `Thread.Sleep(20)` 驱动 ⇒ **≈250–300ms**，且 `BaseObject` ctor `RunTick = GetTickCount() + RandomNumber.Random(1500)` ⇒ **怪物首跑时刻按实例随机**（`PlayObject` ctor 覆盖为当前 tick） | 任何"延迟 N ms 后在第几步处理 / 谁先谁后"的判据：自延迟消息、`due_ms_rel`、延迟帧顺序、`DrainPasses` 计数 | 装置与 Rust 用**同一个** 100ms 假设 ⇒ 两栈互相一致、**共同偏离生产**；golden 与台账同错 ⇒ 断言恒绿 | actor 每 ≈250–300ms 才泵一次队列；到 t=800ms 玩家只发生 ≈2–3 次 `Run()`（不是 8 次）；"未到期根把已到点项压到下一步"的奇偶取决于真实拍数 | "第几步/第几次 Run"语义、跨步排序、`DrainPasses` 真实值；怪物首相位随机 ⇒ 顺序不可当确定量。**依据**：`docs/issues/production-tick-cadence.md`；B 型 2 支分歧（`bigcloak`/`groupdeding`）是第一个显形处，**不是唯一** |
| B24 | **跨 actor 处理顺序被当作生产事实（实际是驱动契约）**：生产里同线程按容器插入序，玩家/怪物/NPC/机器人跨 OS 线程**无全局序**，怪物→线程分配用 `M2Share.RandomNumber.Random`（底层 `new Random()` 时间种子）⇒ 启动随机（见 `docs/issues/production-tick-cadence.md` §9）；装置 `MagicGolden` 固定按 `caster→target→obs_in→obs_out`（`byWho`），Rust 实为 `SelfDelayQueue` **首入队序**（bigcloak 恰好吻合）；契约改为**首入队序**（装置待改） | 任何"跨 actor 帧序 / 同一 tick 谁先谁后 / 消息落到谁的下一 tick"的判据 | 生产顺序不可复刻；装置把契约序当成生产序 ⇒ 比较器绿只证**契约一致**，不证生产一致 | 生产由线程调度 + 容器插入序决定，**没有确定答案** | 跨 actor 帧序、跨 actor 延迟消息落点、旁观者帧先后。**处置**：不追"生产的正确答案"，改为钉死驱动契约并让两侧都按契约（见下方 B24 操作要求） |

### B22 · D=12 `due` 值表（golden 新值 vs Rust 台账，2026-11-22 全量门实测）

| stem | golden `due` | Rust `due` | 判 |
|---|---|---|---|
| cloak | 1000 | 1000 | PASS |
| holyshield | 1000 | 1000 | PASS |
| unamyounsul | 1000 | 1000 | PASS |
| fireball | 0 | 600 | A |
| fireball-5 | 0 | 600 | A |
| firecharm | 0 | 1200 | A |
| grouplightening | 0 | 600 | A |
| lightning-11 | 0 | 600 | A |
| lightning-45 | 0 | 600 | A |
| showhp | 0 | 1500 | A |
| bigcloak | 800 | 900(+recog互换) | B |
| groupdeding | 800 | 900(+640溢出) | B |

### B23 · 操作要求（2026-11-21 裁定：保留 100ms 作为共享驱动节拍）

**裁定**：不重写两侧节拍。对拍的本质是"**同一驱动下，两份实现是否给出同一输出**"——harness 与 Rust 都用 100ms ⇒ 对比的是同一个世界，结论是"**在 100ms 共享驱动节拍下行为一致**"；它**不证明**真实生产（actor 拍 ≈250–300ms + 怪物首相位随机）也一致。依据：`docs/issues/production-tick-cadence.md`。

**操作要求（判据，不是知识）**：
- 凡 `scope` / 卡面写"延迟 N ms 后…"的时间类结论，**必须带限定** `@100ms 共享驱动`（例：`640 在 +1000ms 后处理 @100ms 共享驱动`）。
- 未带限定的"已验证"**不得**被读成"真实时间下也对"；审卡时按缺限定判红（补限定即可，不改结论）。
- 该要求同时登入 `docs/issues/parity-assertion-selfcheck.md` 判据 9。

**B 型 2 支（不翻转，churn 与节拍无关）**：装置 `DrainOnce` 仍按生产 `GetMessage` 的**逐条 churn**修（100ms 下生产泵也是逐条）。解冻由 `opc-19` 宣布；解冻后顺序：`bigcloak` → `groupdeding` → 全量逐支点名、**不得翻转**。

### B24 · 操作要求（2026-11-21 裁定：跨 actor 顺序 = 驱动契约，不是生产事实）

**事实**：生产里"谁的下一 tick 处理"由**线程调度 + 容器插入序**决定——玩家 `PlayObjectList` 是登录完成序、怪物线程分配用 `Random()`（时间种子）、四类 actor 跨 OS 线程并发（`docs/issues/production-tick-cadence.md` §9）⇒ **不存在可复刻的"正确答案"**。

**契约（两侧都按）= 首入队序**：跨 actor 处理序 = **首条自延迟消息入队序**（Rust `SelfDelayQueue.order`；caster 通常先入队，但**不保证**）。装置须按此复刻（当前装置 `MagicGolden` 仍固定 `byWho` 循环 caster→target→obs_in→obs_out，**待改**——见 B24 装置修法/harness-unverified-assumptions ③）。两侧比较仍有效——它证的是"**同一契约下**行为一致"。

**操作要求（判据，不是知识）**：
- 凡 `scope`/卡面写"跨 actor 帧序 / 谁先谁后 / 消息落到谁的下一 tick"的结论，**必须带限定** `@首入队序 契约`（与 `@100ms 共享驱动` 并列）。
- 未带限定**不得**读成"生产里也如此"；审卡按缺限定判红（补限定即可、不改结论）。
- 该要求同时登入 `docs/issues/parity-assertion-selfcheck.md` 判据 10。

**B 型 2 支**：契约固定后两侧应能对齐；**若仍对不齐，那才是真缺陷**（值得查）。重出解冻口径与 B23 相同（`opc-19` 宣布；顺序 `bigcloak` → `groupdeding` → 全量逐支点名、**不得翻转**）。

## 2. 已知受影响的面（与具体 leaf 对号）

- **656（`RM_CHANGENAMECOLOR`）**：17/30/41 的召唤怪改名色 —— 落在 **B1**；20 的诱惑（`MagTamming` 内对怪发 60005）与 54 族的 60005 下游 —— 落在 **B2**。
- **克隆体出生帧 `10`/`41`（46）** —— 落在 **B3**（`FixedHideMode` 未复位 + 队列无人 drain）。
- **时间推进类**（毒 DoT/回血/状态到期）—— 落在 **B11**；`TargetHp` 不变不等于"oracle 不扣"。
- **按可见集选目标类**（8/37 之外的将来效果）—— 落在 **B4**。
- **按物品表/城堡/事件/配置阈值分叉的效果** —— 落在 **B8/B9/B10**。
- **多延迟同 cast 的帧时序/顺序（实例：50 魔爆成功臂）** —— 落在 **B17**。
- **committed golden 可复现性（生成器 schema 漂移）** —— 落在 **B18**。
- **装置墙钟非确定性（golden 不可逐字节复现）** —— 落在 **B19**；根因批=`openmir2-firecharm-nondeterminism`。
- **clone 家族 actor_id 非确定（虚拟钟也治不了）** —— 落在 **B20**。
- **夹具意图未物化（CasterSc/Mc 只写 WAbil 被 recalc 覆盖）** —— 落在 **B21**（暴露者 `9c9782a30`）。
- **per-frame due（时间维度逐帧覆盖）** —— 归 **B22**（**真实**：9 支路径特异分歧，见上）；本轮落 49 支、9 支暂留旧 golden 无 due（不盖缺口）；表示层重构后再补。
- **对拍的时间节拍是假设值（100ms vs 生产 ≈250–300ms + 怪物随机首相位）** —— 落在 **B23**（见 `docs/issues/production-tick-cadence.md`）；B 型 2 支分歧是第一个显形处，不是唯一。
- **跨 actor 顺序被当成生产事实** —— 落在 **B24**：生产无全局序（线程调度 + 容器插入序 + 怪物线程随机分配），装置/Rust 的**首入队序**（装置当前固定 byWho，待改）是**驱动契约**；跨 actor 帧序类结论必须带 `@首入队序 契约`。

## 3. 遇到疑似缺失帧时的排查顺序（**顺序很重要**）

1. **先查本卡 24 条盲区**：该帧的发送者/目标是否 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11/B16/B17/B18/B19/B20/B21/B22/B23/B24？
2. **再查队列模型**：`docs/issues/openmir2-refmsg-queue-model.md`（oracle 全入队 vs Rust 直发 ⇒ 跨 actor 次序/可见面的系统性差异）。
3. **再查比较器是否把它滤掉了**：`keep` / `sent_filter` / `sent_filter_sinsu` 白名单、`frame_order_scope` 放宽、`due` 容差——**先确认这条帧在判据面内**。
4. **最后才判产品缺陷**（并按 §1 的 R/L 分级与 §3 的牙要求取证）。

> 今天三次「少了帧」：**两次是盲区（B1/B2/B3 族）、一次是队列模型**，**没有一次是产品缺陷**。顺序反过来就会白改产品。

## 4. 使用法

1. 看到「Rust 少帧/少抽」时，先按上表逐行问："该帧的发送者/目标是不是 B1/B2/B3/B5 的对象？是否依赖 B4/B6/B7/B8/B9/B10/B11/B16/B17/B18/B19/B20/B21/B22/B23/B24？"
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
| B20 clone 家族 actor_id 非确定 | 本卡新立；`clone46` 已实证虚拟钟 3 次仍不稳；与 B19（墙钟源）互补：B19 解墙钟、actor-id 是非时钟独立源 |
| B21 夹具意图未物化（CasterSc/Mc→Abil） | 本卡新立；`uenhancer` 15→0；暴露于 `9c9782a30`；修法=CasterSc/Mc 同步写 Abil.SC/MC（行为性，后续批） |
| B23 时间节拍是假设值（100ms 未核实） | 本卡新立；生产 `RunTime=250` + 50/20ms 循环 + `Random(1500)` 首相位，见 `docs/issues/production-tick-cadence.md`；B 型 2 支分歧（`bigcloak`/`groupdeding`）为第一个显形处；改法由总控裁定前暂停 |
| B24 跨 actor 顺序 = 驱动契约 | 本卡新立；生产无全局序（线程调度 + 容器插入序 + 怪物线程随机分配，见 `production-tick-cadence.md` §9）；装置/Rust 按**首入队序**契约（装置当前固定 byWho，待改）；跨 actor 帧序 scope 需带 `@首入队序 契约`；B 型 2 支契约固定后重出，仍不齐才是真缺陷 |

## 6. 关联

- `docs/issues/openmir2-golden-generator-drift.md`（生成器世代漂移，另一类"看不见"）
- `docs/issues/groupamyounsul-nondegenerate-golden.md`（退化守卫：效果体零次执行的具名红）
- 卡 `openmir2-golden-harness-mapmgr`（`OfflineMapSystem`/`MapMgr` 的建立与边界）
