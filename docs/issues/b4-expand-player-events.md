# B4 扩面（31 / 53 / 656 / 100）— **OPEN**：装置无 per-recipient 队列模型

> 结论先行：**装置 `MagicGolden` 记录的 SM `due_ms_rel` 是 `SendSocket` 的“生产时刻”，没有
> per-recipient 队列延迟模型**。657 pilot 能在装置 golden 上对上，是**间接**的——装置处理
> `RM_TRANSPARENT` 的步次恰好与我们的 per-viewer 延迟结果对齐；这不代表装置能验证
> 31/53/656/100 的“收件人下一回合才可见”。

## 1. 决定性证据（2026-11-21，只读）

- `fireball/edge`：装置重出 **与旧 committed golden 逐帧完全相同**（`Fixture` diff 0；
  caster `[(638,2001,101,0),(31,2002,209,600),(656,2001,47,600)]`），而 Rust 扩面实现给出
  `31@700 / 656@700`。⇒ 差异不是“装置 B17 升级”，是 **Rust 扩面本身**。
- `bigcloak`：装置重出与 657 pilot 一致（caster `1002@900`、`1001@900`；target/obs
  `1002@800`、`1001@900`），但装置侧 due 仍取“该 actor 处理 RM 的步次”，不是收件人队列
  消费时刻。
- 全量对表（60 支装置重出 vs Rust 台账）：90 个 case-label 一致、80 个不一致；其中
  - 内容不同 15（含 fire/s1 的 `638.param 105 vs 101` 等装置夹具/记录面差异）；
  - 仅顺序 15、仅 due 19、无对应台账 32（多为 stem 命名映射，待补）。
  ⇒ **不能把这 80 条归因于“装置升级伴生”**。

## 2. 本批处置（已撤出本批）

31 / 53 / 100 / 656 的 per-viewer 延迟（`push_addressed` 扩面、`drain_b4_self_deliveries_with_pending`、
`PendingDelayedFrames`、out-of-scope 计数器、tick 序契约）**全部撤出本批，不提交**；
**未重出、未提交任何 golden**（fire/skill10/grouplightening 的临时重出已还原）。

补充：tick 序契约（host 契约序）在现有 B3Rig 单测上**非中性**——怪物 producer（id=4）
不在 3 个具名 actor 的 order 中，按契约必须 panic（4 条 `spell_p3b3_*` 红），故本批一并不带。

## 3. 重做扩面的前置条件（OPEN）

1. **装置侧**先建 per-recipient 队列模型（SM 帧的“收件人 Run 时刻”），或
2. 提供**真机 oracle**证据（同场景下 31/53/656/100 的实际到达 tick）。

在此之前，任何“把 31/53/656/100 的 due 改到下一 tick”的 Rust 改动都**不能用现有 golden 判定**。

## 4. 参考

- `docs/issues/b4-per-actor-queue-blueprint-20261122.md` §12（施工偏离）与 §12.3（tick 序契约，由 `1e` 维护）；
- B24（跨 actor 顺序 = 驱动契约）。

## 5. 扩面设计需求清单（给重开卡，**只列需求，不含实现**）

> 用途：明天给设计会话（`1e`/`3b`）的输入。每条给"要回答的问题 / 可选方向 / 判定证据 / 验收"。
> 现有证据清单：装置 60 支重出在 `E:/UserTemp/b4-expand/device-all/`，Rust 台账在
> `<worktree>/mir2-rs/target/magic-parity/`（`b4-open-final` run），对表结果见本卡 §1。

### 5.1 装置 per-recipient 队列模型：建到哪一层？

- **问题**：要让"SM 帧在收件人下一 `Run` 才可见"可被装置表达，模型应建在 **RM 入队层**还是 **SM 帧层**？
- **方向 A（RM 入队层，贴近 oracle）**：`SendRefMsg` 产生的 `SendMessage` 进入**目标 actor 的 MsgQueue**；
  目标 actor 的 `Run` 才 `Operate` 并 `SendSocket`。需建模每 actor 队列（装置已有 `MsgQueue` 字段）+
  记录 `Operate` 时机，而不是在发送处直接记录。
  - 成本：改 `RecordingNetChannel` 的归属、`DrainActor` 的 flush 点；对 657/31/53/656/100 都统一。
- **方向 B（SM 帧层 shim）**：在记录通道上按 recipient 维护 pending 帧 + 当前 turn 上下文，
  flush 时按"收件人 turn 是否已过"决定发/留。与 Rust 当前 pilot 实现同构。
  - 成本：改动小；**风险**：把"oracle 的 RM 队列语义"近似为"SM 帧延迟"，可能掩盖其它 RM 行为。
- **必须回答**：`due_ms_rel` 的语义要改成什么？（生产 step vs 收件人消费 step；47 支已比 due）
- **与 657 的关系**：若改装置模型，657 现有 golden（`1002@900` 等）可能变化，需重出并重验 bigcloak 三支。
- **判定证据**：新装置在 bigcloak 产出 `caster {1002@900,1001@900}`、`target/obs {1002@800,1001@900}`；
  且在 fireball/hbfireball 的 31/656 上给出与"收件人下一 Run"一致的 due。
- **验收**：同一场景装置输出可作为扩面 Rust 的权威；否则不得改 Rust due。

### 5.2 怪物 tick 序：扩 `byWho` 还是"怪物不产帧"？

- **事实**：装置 `byWho` 恒 4 个具名 actor；怪物不 `Operate`、不在 `byWho`。Rust 侧怪物
  （如 fire/skill10 的 id `id_base+10`、holy shield 单测的 id=4）**会作为 producer 产生 31/53/656 帧**。
- **方向 M1（扩 byWho 到怪物）**：给怪物建 `Run`/`Operate` 最小支 + 帧记录（至少"作为发送者"面）。
  - 需定：怪物 turn 序来源（生成序/登记序/byWho 之后的固定序）；是否纳入所有 MonsterSpec stem。
- **方向 M2（怪物不产帧）**：Rust 侧抑制怪物发出的 31/53/656；需 **oracle 证据**证明这些帧在真实
  服务端不会发给玩家（`AnimalObject.Struck` / `RefNameColor` 是否受 `WantRefMsg`/可见性门约束）。
- **必须回答**：现有旧 golden 中怪物产生的帧（例如 fire/skill10 的 31）是真实 oracle 行为还是装置
  伪影？（对照 `src/M2Server/Actor/AnimalObject.cs` 的 `Struck`/`RefNameColor` 调用链）
- **验收**：含 `MonsterSpec` 的 stem 要么全绿，要么在 scope 里逐条 OPEN（不得静默当"未跑"）。

### 5.3 窗口尾遗留：`NotDue` 跨层映射

- **事实**：装置 `NotDue` = **RM 队列未 Operate 项**（实例 `firecharm edge/hit`：
  `ident 8090/8103, delivery 0, now=窗口末`）；Rust 本批的 pending 是 **SM 帧**（已生成未 flush）。
- **方向 N1（装置同时记录 SM 待发帧）**：若 5.1 选 A/B，装置自然拥有 SM pending；`NotDue` 可扩展
  为可同形比较（需定义字段：recipient/socket、ident、due/turn）。
- **方向 N2（Rust 建 RM 层队列模型）**：成本高，属 §12 (ii) 另一半；会把"对拍"变成"建模"。
- **方向 N3（本批 OPEN）**：NotDue 只比 RM 层；Rust SM pending 另字段且**不跨层比**；比较器显式
  skip + 日志，禁止静默置空。
- **必须回答**：`firecharm` 的 RM 层 `8090/8103` 与 Rust SM pending 的 2 条**是否同一现象的两层表示**？
  （若否，分别是什么；若是，写进卡作为跨层映射实例。）
- **验收**：两侧窗口尾条目要么同形可比，要么各自登记 OPEN 并有日志；不得用"清空 NotDue"过门。

### 5.4 交付顺序建议（给设计者）

1. 先定 5.1 的层次与 `due` 语义 → 2. 定 5.2 的怪物边界（会决定多少 stem 可入批）→
3. 定 5.3 的记录结构 → 4. 再改 Rust（本卡 §2 的撤出改动可作参考）。
