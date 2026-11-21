# 自延迟泵堆序：同一 cast 的多条 self-delay 消息被推迟交付

> 状态：**已实测定位，未修代码**。触发源 = 50 `MabMabe`（魔爆）非缺省对照臂成功臂
> （卡 `openmir2-mabmabe-success-arm-parity.md`）。同一轮的先一步实测已证"8103 分歧 = 台账装配（N→N，
> 产品侧逐条产出 2 次受击）"；本卡接着证"两条 `RM_DELAYMAGIC` 的时序 700/800（相对 cast）vs golden
> 600/600"的成因**不是**调度差异，而是**自延迟泵的堆序保真缺陷**。修复前必须先做 §5 的影响面扫描。

## 0. 定性

**产品缺陷（自延迟泵的堆序保真），不是“装置缺口”。** 理由：`SelfDelayQueue` 在**模拟 oracle 的
`ActorEntity.MsgQueue`**；oracle 两条 600 先出、中间无跳过，Rust 因堆根可能是更晚到期的消息而 `break`、
把**已到点**的消息压到下一步 ⇒ **可观测行为与 oracle 不一致 ⇒ 产品缺陷**。

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

## 4. 正确语义（oracle 对照）

oracle 的 `ActorEntity.MsgQueue` 在 golden 里两条 `RM_DELAYMAGIC` **都按 600 同刻出队、中间无跳过**。
⇒ 正确语义要求：**一步内取出所有已到期消息**，且**取数序不与到期时刻相悖**。Rust 现状不满足：
- **堆序**：需按 `(priority, deliver_at)`（或 priority 相等时按 `deliver_at`）有序，使"最早到期"在堆根；
- **一步内**：`deliver_self_delay_msgs` 在 `now` 时刻应把**所有 `deliver_at <= now`** 的消息取出，
  而不是在第一个未到点堆根处停泵。

（本卡不对"复刻 `.NET PriorityQueue` 某次具体堆布局"下结论——`.NET` 堆在等优先级下不稳定；
判据是**可观测行为**：两条 600 必须同刻出队、不被更晚到期消息挡住。）

## 5. 影响面（待扫描，先写问号）

**凡"同一 cast 产生 ≥2 条 self-delay 消息、且与该 actor 队列里其它更晚到期消息并存"的场景都可能受影响**
——尤其：延迟魔法多击（50 魔爆 `power/3` + `n_power` 两条 `RM_DELAYMAGIC`）、延迟魔法与
`RM_POISON`/`RM_MAGIC_LVEXP` 同 cast 入队、以及任何"一条近到期、一条远到期"的 self-delay 组合。
**具体命中哪些既有 `verified` 条目（golden 是否已暴露、是否被现有断言挡住）需要一次独立扫描**，
不在本卡猜测。

## 6. 修法建议与风险

改泵/堆序会**改变多支 golden 的时序对拍结果**（`due_ms_rel`、出队顺序、中间态 HP）。因此：
- 必须跑**全量门**，并**逐个点名复跑既有 stem**（不能只看 50 非缺省一支）；
- **不得**用 `#[ignore]`、放宽断言或删帧来吸收差异（这是行为修正，不是判据放松）；
- 改动后按常规 review 逐条重生成/复核受影响的 `golden/magic/*.json` 及对应 `Rm`/`Sent` 期望。
