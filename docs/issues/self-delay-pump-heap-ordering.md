# 自延迟泵堆序：同一 cast 的多条 self-delay 消息被推迟交付

> 状态：**已实测定位，未修代码**。触发源 = 50 `MabMabe`（魔爆）非缺省对照臂成功臂
> （卡 `openmir2-mabmabe-success-arm-parity.md`）。同一轮的先一步实测已证"8103 分歧 = 台账装配（N→N，
> 产品侧逐条产出 2 次受击）"；本卡接着证"两条 `RM_DELAYMAGIC` 的时序 700/800（相对 cast）vs golden
> 600/600"的成因**不是**调度差异；定性更正见 §0（**装置缺口 B17**，不是产品缺陷）。

## 0. 定性

**定性更正（2026-11-21）**：本条原判「产品缺陷（自延迟泵堆序）」**已撤回**。核对 C# 真机：`ActorEntity.MsgQueue` 为 `PriorityQueue<SendMessage, byte>`（只按 priority），`BaseObject.Message.cs::GetMessage` 遇未到期队首即 `Enqueue(Lowest)` + `return false` ⇒ **Rust 泵与生产同构、无缺陷**；偏离生产的是 golden 装置 `MagicGolden/Program.cs::DrainOnce`（`DrainRaw()` 全取、逐条判到期、未到期 `PushBack`）。⇒ 本条改判**装置缺口 B17**（见 `docs/issues/golden-harness-blind-spots.md`）。**探针事实全部保留**——它们现在是“装置偏离生产”的证据，不是“产品缺陷”的证据。

## 1. 现象（探针原文，`nondef_hit`，cast 时刻 `clock_ms=400`）

施法后、排水前（Q0）：caster 队列 4 条 self-delay 消息，**全部 `prio=255`**：

```text
[PROBE-Q0] clock_ms=400 qlen=4
[PROBE-Q0MSG] ident=8021 actor=1001 deliver_at=1000 prio=255 seq=0 w=5  n3=1002   ← RM_DELAYMAGIC #1
[PROBE-Q0MSG] ident=8021 actor=1001 deliver_at=1000 prio=255 seq=1 w=15 n3=1002   ← RM_DELAYMAGIC #2
[PROBE-Q0MSG] ident=10123 actor=1001 deliver_at=1400 prio=255 seq=3              ← RM_MAGIC_LVEXP
[PROBE-Q0MSG] ident=8037 actor=1002 deliver_at=1050 prio=255 seq=2 w=5           ← RM_POISON
```

**首个 due 时刻 `clock=1000`：两条 `8021` 都已到期，却一条都没取**（`outs_len=0`）：

```text
[PROBE-OUT] clock=1000 outs_len=0 idents=[]
[PROBE-QMSG-AFTER] clock=1000 ident=8021 deliver_at=1000 ...   ← 到期却仍在队列
[PROBE-QMSG-AFTER] clock=1000 ident=8021 deliver_at=1000 ...   ← 到期却仍在队列
[PROBE-QMSG-AFTER] clock=1000 ident=10123 deliver_at=1400 ...
[PROBE-QMSG-AFTER] clock=1000 ident=8037 deliver_at=1050 ...
```

**`clock=1100`：3 条到期（两个 `8021@1000` + `8037@1050`），只取 2 条，留下一条已到点的 `8021`**；
`clock=1200` 才取剩下那条：

```text
[PROBE-OUT] clock=1100 outs_len=2 idents=[8021, 8037]
[PROBE-QMSG-AFTER] clock=1100 ident=8021 deliver_at=1000 prio=255 seq=1   ← 到期却留在队列
[PROBE-QMSG-AFTER] clock=1100 ident=10123 deliver_at=1400 prio=255 seq=3
[PROBE-OUT] clock=1200 outs_len=1 idents=[8021]
```

下游效应：两条 `RM_DELAYMAGIC` 在 Rust 里分两步到点（`clock=1100` 与 `1200`），golden 两条都按
`due_ms_rel=600` 同刻出队 ⇒ 对照臂观测到相对 cast 的 700/800 与中间态 HP=95。

## 2. 到期时刻证据（排除"调度差异"）

两条 `RM_DELAYMAGIC` 的 `deliver_at_ms` **都是 1000**（= cast 时刻 `clock_ms=400` + `delay=600`），
**完全相同**。因此延迟**不是**"两条消息的到期时刻本来就不同（600/700）"造成的——**是泵的交付粒度**。

## 3. 根因（符号锚）

- `SelfDelayMsg.deliver_at_ms`：入队时 `= clock_ms + delay_ms`（落点 `send_self_delay_msg_full`）。
  两条 `RM_DELAYMAGIC` 在同一 cast 发送（`cast_mab_mabe_spell` 两次 `send_self_delay_msg_full(RM_DELAYMAGIC, …, 600)`）
  ⇒ 同 `deliver_at_ms`。
- `SelfDelayQueue`（4 叉堆，`.NET 8 PriorityQueue` Arity=4 的复刻）：**只按 `priority` 比较**。
  所有 self-delay 消息（`SendSelfDelayMsg` / `SendStruckDelayMsg`）一律 `MESSAGE_PRIORITY_LOWEST`(255)
  ⇒ 优先级全相等 ⇒ **堆根可以是 `deliver_at_ms` 更晚的消息**，而非"最早到期"的那条。
- `deliver_self_delay_msgs_impl` 的泵语义：
  `while let Some(head) = peek { if head.deliver_at_ms > now { dequeue + enqueue(Lowest) + break } else { deliver } }`
  ⇒ **一遇到"堆根未到点"就整泵 `break`**。因堆根可能是一条更晚到期的消息（如 `RM_POISON@1050`、
  `RM_MAGIC_LVEXP@1400`），已到点（`@1000`）的 `RM_DELAYMAGIC` 会被压到后续步。

## 4. 两侧语义对照（生产 vs 装置）

同一多延迟场景在两侧的可观测行为不同，**以生产语义为准**：
- **生产语义**（`ActorEntity.MsgQueue` 只按 priority；`BaseObject.Message.cs::GetMessage`）：`TryDequeue` 后若 `DeliveryTime > now` ⇒ `Enqueue(Lowest)` + `return false` ⇒ **遇未到期队首即停**，已到点但排在未到期队首之后的消息被推迟到下一 tick。**Rust 泵 `deliver_self_delay_msgs_impl` 与生产同构、正确复刻**。
- **装置语义**（`MagicGolden/Program.cs::DrainOnce`）：先 `DrainRaw()` 取**全部**消息，逐条判 `DeliveryTime <= now`（到期 `OperatePublic`、未到期收进 `later` 后 `PushBack` 回 `MsgQueue`）⇒ **一步内处理所有到点消息、不因未到期队首而停**。
⇒ 可观测差异：同一多延迟场景，装置会把「生产本应延后」的到期消息**提前同刻吐出**；因此 golden 的帧 `due_ms_rel`/出队顺序是**装置语义**，不代表生产。§1 的探针（生产分 tick 700/800）恰是「装置 golden 两条 600/600 同刻」反差的证据。

（`.NET PriorityQueue` 在等优先级下不稳定——判据是**可观测行为**，不钉某次具体堆布局。）

## 5. 影响面（待扫描，先写问号）

**凡"同一 cast 产生 ≥2 条 self-delay 消息、且与该 actor 队列里其它更晚到期消息并存"的场景都可能受影响**
——尤其：延迟魔法多击（50 魔爆 `power/3` + `n_power` 两条 `RM_DELAYMAGIC`）、延迟魔法与
`RM_POISON`/`RM_MAGIC_LVEXP` 同 cast 入队、以及任何"一条近到期、一条远到期"的 self-delay 组合。
**具体命中哪些既有 `verified` 条目（golden 是否已暴露、是否被现有断言挡住）需要一次独立扫描**，
不在本卡猜测。

## 6. 修法建议与风险（修装置，不修泵）

Rust 泵与生产同构、无缺陷（§0/§4），**不改泵**。处置方向是**修装置**：把 `MagicGolden/Program.cs::DrainOnce` 改为**遇未到期队首即停**（对齐生产 `GetMessage` 的 `Enqueue(Lowest)` + `return false`），而不是「全量逐条判到期 + `PushBack`」。
改装置会**改变受影响多延迟场景的 golden 时序**（`due_ms_rel`、出队顺序、中间态 HP），因此：
- 必须重出受影响 golden，并**逐个点名复跑既有 stem**（不能只看 50 非缺省一支；受影响条目见 `89` 的 6 stem 扫描）；
- 按常规 review 逐条复核受影响的 `Rm`/`Sent` 期望；
- **不得**用 `#[ignore]`、放宽断言或删帧来吸收差异（这是装置对齐修正，不是判据放松）。

## 7. 装置对齐修法（B17）与预登记预测（**改代码前落盘**）

**改法（仅 `MagicGolden/Program.cs::DrainOnce`）**：`RecordingPlayObject.DrainRaw()` = `while (MsgQueue.TryDequeue(...))` ⇒ `raw` 已是生产 `GetMessage` 的**堆根出队序**。故把内层从「全取 → 逐条判到期、到期处理、未到期收 `later`、末尾统一 `PushBack`」改为：**按 `raw` 顺序处理，遇第一条 `LateDelivery && DeliveryTime > now` 立即 `break`；已到点前缀照旧处理，把该未到期消息及其后所有未处理条目按原顺序 `PushBack`（该条还原 Lowest，其余按各自记录 prio）**。即生产 `GetMessage`「遇未到期队首即停、单条 `Enqueue(Lowest)` 归还」。**不按到期时间重排/排序**（那是另一种语义，不是生产）。

**预登记预测（改前落盘；用于「预测 vs 实测」对照）**：
- **会变（只动延迟帧的出现时刻/顺序）—— 5 支**：`healing`(MG-12)、`bigcloak`(MG-33)、`bighealing`(MG-40)、`groupdeding`(MG-51)、`mabmabe-nondefault`(MG-48 非缺省)。
- **不变 —— `hbfireball`(MG-53) + 其余全部 stem**：剥绝对墙钟 `delivery` 后**逐字节相等**。
- **5 支的变必须只限**：延迟帧 `due_ms_rel`/出队顺序；**伤害值、HP、抽数、帧条数不得变**。
- **停下条件**（任一即停手报 opc-19，不自行调断言/补 golden）：①预期外 stem 变化；②5 支里连伤害值/HP/抽数/帧条数也变；③任何既有对拍由绿转红。

**验收口径（2026-11-21，opc-19 批准收窄）**：经「3 轮 pristine + 2 轮 fixed」噪声地板（见 B19）判定，**无歧义真变仅 `healing`/`bighealing`**——`cases[2].Receivers[0].Rm` 的 `8034(RM_MAGHEALING)↔10123(RM_MAGIC_LVEXP)` **顺序交换**（两轮 pristine 都不抖、两轮 fixed 都稳）；`hbfireball` IDENTICAL；`pr1_*` 未动；语义审查对齐 `GetMessage`。**多延迟/多受害者 stem（含 `mabmabe-nondefault`、施毒 4 支）因装置墙钟非确定（B19），本批无法逐字节证明——这是验收口径收窄，不是「没验」，更不是「已验证全部多延迟 stem」。** 本批只重出 `healing`/`bighealing`。

**预测口径更正（opc-19）**：预登记预测（5 支）＝「≥2 条延迟消息」，**漏了「多受害者」类**（施毒 4 支每条受害者各一条延迟/受击亦属多延迟）⇒ **下次预测按「队列里是否存在交错 due」判，不按「消息条数」判**。另：`89` 扫描「可能变」的 `bigcloak`/`groupdeding` 实测**没变** ⇒ **扫描是保守、实测更准**。

**虚拟钟后复核（2026-11-21，B19 修法落地）**：噪声地板里持续抖的 **14 支**在虚拟钟下重测——**13 支连跑 3 次逐字节相同（含 delivery）**：`mabmabe-nondefault`/`amyounsul`/`amyounsul2`/`groupamyounsul`/`groupamyounsul2`/`fireboom47`/`firecross`/`holyshield`/`lightflower`/`lightflower-nonzero`/`skill10`/`skill53`/`windtebo`。⇒ **B17 的多延迟 stem 现在可逐字节验**（本卡「口径收窄」升级为「可逐字节验」）。**唯一例外 `clone46`：虚拟钟后仍不稳** ⇒ 其非确定源是 `Clone.actor_id`（产品 `ActorMgr` 分配，**非时钟、独立于 B19**）——需另案，不影响 B17 多延迟面。

**clone46（独立）**：`Clone.actor_id`/`rel_to_caster` 两跑差 ~9e8 —— 克隆体 ActorId 由产品 `ActorMgr`/`PlayCloneObject` ctor 分配（非 harness 可控的确定源，疑运行时/对象哈希/进程计数），**与排水无关**；clone 家族 golden 的该字段天然不可逐字节比对。

**判据固定（opc-19 2026-11-21）**：B17 的「只 5 支变、其余零变化」**只能用 pristine-vs-fixed**（同码仅差排水语义）判；**不得 vs committed**（committed golden 有 schema 漂移，见 B18，会全 stem 假红）。

**两处装置近似（登记）**：
1. **`now` 采样粒度**：装置每步取一次 `GetTickCount()`；生产每次 `GetMessage` 现取。属**装置对生产的近似**，可接受（步长 100ms、窗口内无跨步 timing 敏感支）。**将来若有亚步长时序支，此近似会失效**。
2. **同优先级 tie 序**：装置 `MsgQueue` 与生产同为 `PriorityQueue<SendMessage, byte>`，tie 序应一致；但「未到期那条以 Lowest 重入队」会改变等优先级项间的相对次序（`PriorityQueue` tie 序非 FIFO）。**若某支因此产生与生产不同的输出，那是装置表示能力的限制，不是生产语义本身**——届时登记、不据此改产品。
