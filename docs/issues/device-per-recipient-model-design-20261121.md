# 装置 per-recipient 队列模型设计（扩面卡点 §5.1，2026-11-21）

> 依据：`docs/issues/b4-expand-player-events.md` §5.1（master `e20117d3e`）+
> `docs/issues/b4-per-actor-queue-blueprint-20261122.md` §12.3.b。
> 核心事实：**装置没有 per-recipient 队列延迟模型**——它记的 SM `due` 是「生产/直发时刻」，
> 不是「该收件人下一次 `Run` 才见」的时刻；因此装置 golden 对 31/53/100/656 的 `due` **不是权威**，
> 657 对上属**间接**。
>
> 本文是**只读设计**：不跑构建、不改代码。
>
> **红线（本文件自用）**：任何改动方向必须能在 **C# oracle 源码**里找到对应行为并给出 `file:line`；
> 找不到的方向写「**无 oracle 依据、不建议**」。**不得**以「更像 Rust」为正确性理由。
>
> **位置写法**：`src/**` 是冻结的 oracle 面，按锚门规则**可以**写 `path:line`；
> 而 `tests/parity/csharp/**` 属高 churn 档，**不写行号锚**，只写文件 + 符号。

---

## 1. 事实基线

### 1.1 装置现状（读代码）

- 帧在 **`SendSocket` 被调用的那一刻**被 `RecordingNetChannel` 记下，`due` 取
  `RecordingNetChannel.CurrentDueMs`：`MagicGolden` 的 `DrainOnce` 里设为
  `m.LateDelivery ? DeliveryTime - castTick : stepMs`（`Program.cs` 的 `DrainOnce`）。
- 每个 100ms 步内，装置对 `byWho`（`caster → target → obs_in → obs_out`）**反复排水最多
  `MaxDrainPasses = 8` 趟，直到"本趟没有任何 actor 产生活动"**（`Program.cs` 的 `for (int pass…)` 循环）。
- ⇒ **同一 step 内**，若 target 的 `Operate` 给 caster 入了一条新 RM，**下一趟**就会由 caster 处理，
  该帧 `due = 本 step`。这正是"装置 due 记生产/直发时刻"的机制来源：
  **装置把"一趟排水"和"一次 tick"混为一谈**。

### 1.2 oracle 契约（`file:line`）

| 行为 | oracle 位置 |
| --- | --- |
| 即时消息入**收件人自己的** `MsgQueue`（`DeliveryTime = 0`，优先级 = `(byte)wIdent`） | `src/M2Server/Actor/BaseObject.Message.cs:34`（`SendMsg`；入队 `MsgQueue.Enqueue(sendMessage, (byte)wIdent)` 在 `:94`） |
| 延时消息入队：`DeliveryTime = GetTickCount() + dwDelay`，优先级 `Lowest` | `src/M2Server/Actor/BaseObject.Message.cs:188`（`SendSelfDelayMsg`；`DeliveryTime = GetTickCount() + dwDelay` 在 `:199`，入队 `Lowest` 在 `:204`） |
| 广播：`SendRefMsg` 对视野内每个 actor 调 `SendMsg` ⇒ **各入各的队列** | `src/M2Server/Actor/BaseObject.Message.cs:383` |
| 取消息：出队后若 `msgTick < DeliveryTime` 则**回炉并返回 false**（本 tick 不再处理） | `src/M2Server/Actor/BaseObject.Message.cs:357`（`GetMessage(int msgTick, …)`；回炉 `MsgQueue.Enqueue(…, Lowest)` 在 `:364`） |
| **玩家**每 tick 的消息泵：`while ((now - GetMessageTick) < HumanGetMsgTime) && GetMessage(GetMessageTick, …) → Operate` | `src/M2Server/Player/PlayObject.Message.cs:360` |
| **怪物/基类**每 tick 的消息泵：`while (GetMessage(ref msg)) Operate(msg);` | `src/M2Server/Actor/BaseObject.Base.cs:25`（`Run`；`while (GetMessage(ref processMessage))` 在 `:28`，`Operate(processMessage)` 在 `:30`） |
| `SendSocket` 发生在 **`Operate` 之内**（各 `RM_*` 分支里）——SM 帧离开的时刻 = **收件人 Operate 的时刻** | 例：`src/M2Server/Actor/AnimalObject.cs:114`、`src/M2Server/Actor/BaseObject.Base.cs:610`、`src/M2Server/Monster/MonsterObject.cs:177` 的 `Operate` 覆写/定义 |

**由此得到 oracle 的时间线（这就是本设计要建模的唯一语义）**：

```
tick T:   actor 依次 Run（每个 actor 一次；玩家受 HumanGetMsgTime 预算，怪物全排）
          Run 内：取若干 RM → Operate → 若该 RM 触发 SendRefMsg/SendMsg 给别的 actor，
                  则那条 RM 进入**对方**队列，**在本 tick 不会再被对方处理**（对方本轮 Run 已过）
tick T+1: 对方下一次 Run 才 Operate 它 ⇒ 此时才 SendSocket ⇒ 帧"可见"
```

⇒ 交付延迟的原子是 **RM 入队**，不是 SM 帧。这也解释了为什么 657 能"间接对上"：
装置在 800/900 步处理 `RM_CHARSTATUSCHANGED` 的时刻，恰好与 per-viewer 的下一步一致。

---

## 2. 两个候选方向

### 2.A RM 入队层（**推荐**，有 oracle 依据）

**改动面（全部在装置侧，产品/Rust 零改动）**

1. `MagicGolden/Program.cs` 的**每步排水结构**：把「每 step 多趟直到空」改为
   **每 step 每个 actor 恰好 Run 一次**（按 `byWho` 序）；跨 actor 产生的 RM 只入对方队列，
   本 step 不再处理（回到 §1.2 的时间线）。
2. `MagicGolden` 的 `DrainOnce`：保留"取到 `DeliveryTime` 未到 ⇒ 回炉"语义（对应 `GetMessage` 的回炉），
   但**每 actor 每 step 只调用一次**；`due` 继续用 `CurrentDueMs = stepMs`——此时 `stepMs`
   就是**收件人消费 step**，语义自动变正确。
3. `RecordingPlayObject` 的 `DrainRaw/PushBack/OperatePublic` 不变（已经是 per-actor 队列）。
4. 需要新增**审计侧车**（见 §5 A3）：每帧记 `(recipient, ident, enqueue_step, run_step)`。

**能判什么**
- 31/53/656/100/657 的 **due**（收件人消费时刻）与**跨 actor 顺序**；
- "同一 tick 内 A 的 Operate 给 B 入队 ⇒ B 下一 tick 才发"这一类判据。

**判不了什么**
- 怪物 tick 序（§12.3.a OPEN）：装置 `RecordingMonster` 仍不 `Operate`（§5.2 另案）；
- 玩家 `HumanGetMsgTime` 预算导致的"本 tick 排不完"（装置全排）；
- 真实服务端的 actor 迭代序（§4.2.2 明禁当契约）——装置用的是 rig 的 4-actor 固定序，**必须写进 scope**。

**自己建模的风险（必须承认）**
- **turn 序来源**：装置只有 4 个具名 actor 的固定序，不是生产 ECS 序。任何"依赖怪物序/多观察者序"的
  stem 都不能拿它当权威。
- 一旦改装置，**旧 golden 的 due 可能整体位移**（见 §3），必须逐支核差分，不允许静默覆盖。

### 2.B SM 帧层 shim（**无 oracle 依据、不建议作权威**）

**改动面**：在 `RecordingNetChannel` 上按收件人维护 pending 帧，flush 时按"收件人本步是否已过"决定发/留
（与 Rust `b4-ii-pilot` 的 `SpellFrameBuf::deferred` 同构）。

**能判什么**：只能判"Rust 与这份近似一致"——**不能判 oracle 行为**。

**为什么不建议**：oracle 里**没有 SM 层延迟**——`SendSocket` 是 `Operate` 内的直接动作
（`BaseObject.Message.cs` 的 `GetMessage`/`Operate` 链路），延迟只发生在 **RM 入队到 Operate 之间**。
shim 把延迟搬到 SM 层，等于**在装置里另造一个 oracle 没有的机制**；它能"让 657 继续绿"，
但对 31/53/656 的 due 给不出权威值。⇒ 按红线记为「**无 oracle 依据、不建议**」；
若只为过渡保留，必须在 scope 里写明"近似，不作权威"。

---

## 3. `due_ms_rel` 的语义

**定义（本设计采用）**：`due_ms_rel` = **收件人消费 step** —— 即该收件人在哪一步 `Run → Operate` 了
产生这条 SM 的 RM。理由：oracle 的 `SendSocket` 在 `Operate` 内（§1.2 表），帧"离开服务端"的时刻
就是该 actor 消费时刻；这就是"收件人可见"的时刻。

**与 `SendSelfDelayMsg` 的关系**：`DeliveryTime = now + dwDelay`（`src/M2Server/Actor/BaseObject.Message.cs:188`，`:199` 设值、`:204` 入队 `Lowest`）是
**RM 层**的延迟；在"到点之前 `GetMessage` 回炉"（`:359-380`）。因此
`due_ms_rel = max(收件人消费 step, DeliveryTime − castTick 四舍五入到 step)`；
装置现有的 `m.LateDelivery ? DeliveryTime - castTick : stepMs` **在语义不变的前提下仍适用**，
只是 `stepMs` 现在真的等于"消费 step"。

**对 47 支已比 `due` 的 golden 的影响**（能否不重出就兼容）：

| 帧类别 | 是否变化 | 判据 |
| --- | --- | --- |
| 收件人 = 产生者自身的 self 帧（如施法者自己的 640/657 自通知） | **不变** | 入队与 Operate 同一步 |
| `SendSelfDelayMsg` 到点帧（late = true） | **不变** | due 由 `DeliveryTime − castTick` 决定 |
| 跨 actor 即时帧，且**入队发生在收件人本 step Run 之后** | **可能 +1 step** | 这正是本设计要修正的那批 |
| 跨 actor 即时帧，入队发生在收件人本 step Run **之前** | **不变** | 同 step 内仍会消费 |

⇒ **不能默认兼容**。执行口径：先重出装置 golden，再用"逐帧 `(recipient, ident, due)` 差分"与本批
47 支旧 golden 对表；**差异必须全部落在第 3 行那一类**，否则说明模型或 turn 序有问题，**不得**为了过门而改期望。

---

## 4. 验收断言（改完后怎么证）

- **A1（bigcloak 三支）**：装置输出与契约一致——
  `caster {657,recog=1002 @900, 657,recog=1001 @900}`、`target`/`obs_in` `{1002@800, 1001@900}`；
  s1/s5/s13 三支同形，actor id 用 `base+1/base+2`（**不得**写死 1001/1002，见 `qa-b4pilot-diag`）。
- **A2（fireball / hbfireball 的 31/656）**：对每帧断言
  `run_step(f) >= enqueue_step(f)`，且
  `run_step(f) == 第一个满足「收件人在该 step 有 Run 且本 RM 在该 Run 之前已入队」的 step`
  （收件人本步已跑过 ⇒ 必须落到下一步）。这是**逐帧机判**，不是"看起来对"。
- **A3（审计侧车，fail-closed）**：装置额外落 `frames-audit.jsonl`，每帧
  `{who, ident, recog, enqueue_step, run_step, due}`；断言 `due == run_step`、`run_step >= enqueue_step`；
  **缺字段即契约失败**（rc=2 不落盘），与装置现有 `FixtureContract` 的 fail-closed 同口径。
- **A4（与 Rust 对齐）**：比较器逐收件人比 `(ident, recog, param, tag, series, due_ms_rel)`，
  与现有 `real_rust_ledger_*` 形态一致；Rust 侧按 §1.2 的时间线实现（不得反过来把装置改成像 Rust）。

---

## 5. 红证（这个模型"能判红"的构造）

全部在**装置侧**构造（产品/Rust 不动），每条都必须点出一个可读的失败位置：

| # | 变异（装置符号） | 期望红 |
| --- | --- | --- |
| R1 | 把"每 actor 每 step 只 Run 一次"改回**多趟直到空**（即当前 `MaxDrainPasses` 行为） | bigcloak `caster 1002` 变 `800` ⇒ A1 红（`left [800] right [900]`） |
| R2 | 忽略 turn 序、按**入队序**全排所有 actor | caster/target 的 31/53 先后与被观测 due 变化 ⇒ A2 红 |
| R3 | `due` 记**生产 step**（`enqueue_step`）而非 `run_step` | A3 审计断言红（`due != run_step`），且 A2 红 |
| R4 | `SendSelfDelayMsg` 的 `DeliveryTime` 不参与回炉（到点前也 Operate） | late 帧 due 提前 ⇒ A2 红 |

**判据**：R1–R4 每一条都要先跑出红（装置重出 golden 时即红），再还原复绿；红证必须带
`enqueue_step/run_step/due` 三列原文，不接受"总数变了"。

---

## 6. 与 657 已有 golden 的关系

- 现 657 golden（`caster {1002@900,1001@900}`、`target/obs {1002@800,1001@900}`）是**间接**对上的：
  装置恰好按"下一步处理被延迟的 RM"得到了正确值。
- 换成 RM 入队层后，机制变成**直接**：target 的 Run（turn 1）给 caster 入 `RM_CHARSTATUSCHANGED`，
  caster 本 step 已跑过 ⇒ caster 下一步 Run 才 Operate ⇒ `900`。
- ⇒ **657 golden 不重出**：它们是新模型的**回归验证**——如果新模型给出别的值，先怀疑模型，而不是改期望。
  只有在审计侧车证明"入队/消费 step 与旧 golden 的成因不同、但值相同"时，才在卡里记录成因变化。

---

## 7. 前置缺口（改之前要知道的）

1. **turn 序**：装置只有 `byWho` 的 4-actor 固定序；怪物序 OPEN（§12.3.a）。含 `MonsterSpec` 的 stem
   不得拿本模型当权威。
2. **玩家消息泵预算**：oracle 有 `HumanGetMsgTime`（`src/M2Server/Player/PlayObject.Message.cs:360`）；装置"全排"。
   若某 stem 出现"同 tick 内队列未排完"，装置与生产不等价——需在 scope 里 OPEN。
3. **怪物 Operate**：装置 `RecordingMonster` 被动不 `Operate`（§5.2），本设计不解决。

---

## 8. 结论

**选方向 A（RM 入队层）**：它逐条对得上 `src/M2Server` 的 `SendMsg`/`SendSelfDelayMsg`/`GetMessage`/`Run`/`Operate`
链路，改动集中在装置的"每步排水结构"与一条审计侧车；方向 B 无 oracle 依据，只能作过渡近似、不作权威。
`due_ms_rel` 统一改成**收件人消费 step**；旧 47 支 due 需逐帧差分核过（差异必须只落在"跨 actor 且收件人本步
已跑过"这一类）；657 golden 不重出，作为新模型的回归验证。
