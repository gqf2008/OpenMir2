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
