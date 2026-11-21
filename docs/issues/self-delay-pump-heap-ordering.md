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
