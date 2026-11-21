# B22 装置表示层重构设计：Sent 帧逐帧 `due_ms_rel` 归属

> 性质：**设计卡，只出方案，不改代码 / 不跑构建 / 不碰 golden**（opc-19 2026-11-22 派）。批准后再排实现。
> 归口：B22（`docs/issues/golden-harness-blind-spots.md`）的「后续正式批：表示层重构」。

## 0. 结论速览

- **B22 的 9 支分歧不是一类**：
  - **A 型 7 支**（`fireball`/`fireball-5`/`firecharm`/`grouplightening`/`lightning-11`/`lightning-45`/`showhp`）＝**装置 due 归属缺口**（本卡要解的）。根因**不是「没设 `CurrentDueMs`」这么简单**，而是**帧在触发它的延迟消息 `OperatePublic` 窗口之外才 emit**，单一全局标量届时已归 0。⇒ **必须链路传播（设计 ii），补设置点不够**。
  - **B 型 2 支**（`bigcloak`/`groupdeding`）＝**顺序问题（B17 家族），不是 due 表示**。golden 与 rust 的 `recog` **多重集相等、仅位置互换**，`due 800 vs 900` 是同一互换的**伴随现象**（同帧换了受害者）。⇒ **本重构不修 B 型**；B 型由 B17 顺序对齐解决，顺序对了 due 自然对。
- **推荐方案 (ii) 帧归属传播（harness 侧 geneaology map）**；**(i) 显式改签名被否**——需动生产 `SendMessage`，违反「生产类零改动」。

## 1. 现状机制（symbol anchors，不写行号）

- `RecordingNetChannel`（harness，换掉 `M2Share.NetChannel`）持有**单一静态标量** `RecordingNetChannel.CurrentDueMs`。
- 每条下行帧在 `RecordingNetChannel.AddGateBuffer` / `AddGameGateQueue`（即 `SendMsg` 落到通道时）**随帧捕获当前 `CurrentDueMs`**；`ParseFrames` 把它写成 golden 的 `due_ms_rel`。
- 排水循环（`RunMagicCase` 的 `DrainOnce` 步进分支与其 `else` 一次性分支、`RunFireball` 的排水、`ClientSpellXYEntry.Run` 的排水）在调用 `obj.OperatePublic(m)` **前后** set/reset `CurrentDueMs`：
  `CurrentDueMs = m.LateDelivery ? max(0, DeliveryTime − castTick) : stepMs; OperatePublic(m); CurrentDueMs = 0;`
- `castTick = HUtil32.GetTickCount()` 取在 `DoSpell` **之前**（`RunMagicCase` 有；**`RunFireball` 没有**）。

**为什么"单一全局标量 + OperatePublic 前后 set/clear"对"帧在后续 pass emit"必然失效**：
标量只在**某条消息的 `OperatePublic` 调用期内**有效。C# 生产模型里，`OperatePublic(延迟消息)` 往往**不直接发帧**，而是把后续消息（`RM_MAGSTRUCK` 扇出、`SM_OPENHEALTH` 等）**再入队**到各 actor 的消息队列；真正 `AddGateBuffer` 发生在**后续排水 pass** 处理那条子消息时——此时 `CurrentDueMs` 已被上一条消息的 `OperatePublic` 尾部 reset 成 **0**。⇒ 帧与"发起它的延迟消息"的 `DeliveryTime` **失去关联**，记 0。

## 2. 两条失败路径各要什么

### A 型 7 支 —— 需链路传播（非补设置点）

**实证（读既有重出件，未改任何东西）**：
- `showhp` edge：caster `Sent[640] due=1000`（**对**，与 rust 同值）——在触发延迟消息 `10123`（`late, delivery=1001000`）的 `OperatePublic` 窗口内**同步** emit；而 target `Sent[1100] due=0`——它的触发 `8040`（`late, delivery=1001500`）**存在且到期**，但帧在后续 pass emit ⇒ 记 0。
- `fireball-5` edge：caster `Rm` 有 `8021 late delivery=1000600`，`Sent[31] due=0`——触发消息是 600ms 延迟，帧却记 0。

⇒ **同一支里有的帧对（同步 emit）、有的帧 0（后续 emit）**。证明根因是"帧逃出触发消息的 `OperatePublic` 窗口"，**不是"这条路径从不设 `CurrentDueMs`"**。
- 对 `fireball`（`RunFireball`）**额外**：`RunFireball` 连 `castTick` 都没取、排水里也不 set `CurrentDueMs` ⇒ 该支所有帧记 0（双重缺口）。
- ⇒ **A 型要的是"把 `DeliveryTime` 沿消息链路传给子帧"**（link propagation），**补一个 set 点只对"同步 emit 的帧"有效，治不了"后续 pass emit"的帧**。

### B 型 2 支 —— 是 B17 顺序，不是 due 表示

- `bigcloak`：golden `Sent[2].recog=1001, Sent[3].recog=1002`；rust `1002 / 1001`——**多重集 {1001,1002} 相等，仅位置互换**；`Sent[3].due_ms_rel golden=800 rust=900` 是**同一互换**（位置 3 在 golden 是受害者 A、在 rust 是受害者 B，A/B 的 due 不同）。
- ⇒ **顺序与 due 同一根因＝多受害者出帧次序**（B17 家族：`DrainOnce`/`Operate` 扇出次序 vs Rust 泵步进次序）。**顺序对齐后 due 自动对，无需表示层改动**。
- ⇒ **本重构不覆盖 B 型**；B 型留给 B17 顺序对齐（先对齐顺序，再把这两支的 due 纳入即绿）。

## 3. 设计方案（i vs ii）

### (i) 显式传递 —— **建议否决**
把 due 作为参数沿 `OperatePublic → SendMsg → 下游帧` 传下去。
- **否决理由**：`SendMessage` / `SendSelfDelayMsg` / `OperatePublic` 是**生产类**；加字段/改签名＝动生产代码，违反本装置「**生产类零改动**，只在 harness 换 `NetChannel`/接缝」的立轴（同 `RecordingNetChannel`/`RngSeam`/`ClockSeam` 的既有做法）。侵入面广且破坏"harness 与被测产品解耦"。

### (ii) 帧归属传播（harness 侧 geneaology map）—— **推荐**
纯 harness，不碰生产类型：
1. 每个 runner 建 `Dictionary<SendMessage, long> dueByMsg`（**按消息对象引用**为键）与 `long castTick`（`RunFireball` 补上）。
2. `DrainOnce`（及等价排水）处理消息 `m` 时：
   - `long due = dueByMsg.TryGetValue(m, out var d) ? d : (m.LateDelivery ? max(0, DeliveryTime−castTick) : stepMs);`
   - `CurrentDueMs = due; (snapshot 该 actor 队列的现有消息集) ; obj.OperatePublic(m);`
   - **inherit**：`OperatePublic(m)` 期间**新入队**的子消息（按引用差集）→ `dueByMsg[child] = due`（延迟消息自身仍用其 `DeliveryTime−castTick` 覆盖）。⇒ due 沿消息 genealogy **传递**到后续 pass 才 emit 的帧。
   - reset `CurrentDueMs=0`。
3. 队列可见性：`RecordingPlayObject` 已包 `DrainRaw`/`PushBack`/`OperatePublic`；补一个"入队消息引用快照"helper 即可（仍纯 harness）。

**取舍**：范围可控（只 harness 排水层），不破坏既有 32/49 支（它们同步/即时帧本就对）；代价=需按引用维护 map（C# 队列持引用，可行），且要覆盖所有排水入口（`RunMagicCase` 两分支 + `RunFireball` + `ClientSpellXYEntry`）。

**影响面**：`mir2-rs/tests/parity/csharp/MagicGolden/Program.cs` 单文件——
`RecordingPlayObject`（入队快照 helper + `dueByMsg` 由调用方持有）、`RunMagicCase::DrainOnce` 与 one-shot 分支、`RunFireball` 排水、`ClientSpellXYEntry.Run` 排水——约 **1 文件 / 6 处**；`RunFireball` 另加 `castTick`。**不改生产、不改比较器、不改 golden**（重出在实现后单独批）。

## 4. 验收设计（证"逐帧成立"，非"凑巧相等"）

1. **A 型 7 支逐支点名**（`MAGIC_PLANE_ONLY` 单跑）：每支 dump `golden.due` vs `rust.due` **逐帧并列**表（非聚合），differ 帧数必须 0（640 走 ±32ms、非 640 精确）。
2. **回归 49+32 支**：全 plane 复跑，确保**已绿的支 due 不翻**（逐支点名，RED_SET 空），且 byte 稳定 3×。
3. **反"凑巧相等"负例牙**（必须）：合成夹具里把**某一帧** due 改 +1（或把 inherit 故意接到错误父）⇒ 比较器**必须红**，`left/right` 原文进卡。证明 due 是**真被比较**，不是恒等巧合。
4. **同步/后续两态都覆盖**：A 型每支至少各有 1 帧"触发消息 `OperatePublic` 窗口内同步 emit"（如 showhp 640）与 1 帧"后续 pass emit"（如 showhp 1100），两者改后都等于 rust 值——分别证 set 点与 inherit 都生效。
5. **B 型不入本验收**：B 型由 B17 顺序对齐单独验收（顺序对齐后 due 自动对）。

## 5. 风险与止损线

- **止损 1（伪解）**：若某帧即使做了 genealogy inherit 仍记 0——说明该帧是 C# **真正在施法时即时 emit**、而 Rust 建模为延迟（**产品分歧**，非装置）。那时 doit 停 harness 重构，转判"是否 Rust 侧错/或真不可对角"。
- **止损 2（不可捕获）**：若生产入队**拷贝**消息（非引用）导致引用 map 失配，或队列未暴露子项——(ii) 不可行，退回 (i) 需生产改动（须 Owner/Leader 另批），否则**退回"这些帧永不比较 due"**（＝现状欠账，文档化，不硬凑）。
- **止损 3（回归面）**：若重构导致 **已绿 49/32 支**出现 due 翻转（哪怕 1 支）——视为"表示层改动不稳"，**回滚**，维持现状欠账；不在 destabilize 49 支的前提下去救 9 支。
- **止损 4（B 型误并）**：若把 B 型的 `800/900` 当成"due 表示"去改 scalar——错。B 型是顺序；**本重构不碰**，否则会把 B17 拖成"装置 due 问题"再误修。
- **永远可退的现状**：当前 9 支"暂留旧 golden 无 due、不比较"是**已知、文档化欠账**（B22/B17），不是绿；重构走不通就维持它，**不得在缺口上盖 golden 充绿**。

## 6. 建议下一步（实现排序，非本卡执行）

1. **先做一个小 probe（只读）**：对 7 支各 dump「每帧触发消息的 `LateDelivery`/`DeliveryTime` + 帧当前 due」，逐帧标注「同步 emit / 后续 emit」，确认 A 型全部落在"后续 emit"。这决定 inherit 覆盖范围。
2. B 型另起 B17 顺序对齐任务（与本卡并行、不耦合）。
3. A 型按 (ii) 实现 → 第 §4 验收 → 重出 7 支 → 全量门。
