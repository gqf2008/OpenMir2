# openmir2-golden-generator-drift — 既有 golden 与当前生成器 schema/墙钟漂移

**状态**：事实登记（只读查证）；**待决策（是否整批重生成）**，非紧急。
**查证 head**：master `2692d0a26`。

## 1. 现象
仓库既有 magic golden（如 `mir2-rs/tests/parity/golden/magic/skeleton.json`）是**旧 schema** 生成的；用当前 master 的 MagicGolden 生成器**重跑会出 diff**：
- **新增字段**（结构 diff）：`caster_actor_id` / `target_actor_id` / `recog` / `due_ms_rel` / `hit_speed` / `frame_order_scope`（均已在 master `Program.cs` 的 `FireballResult`/帧记录里，属 c6 的 recog-schema 变更：`:866/867/931/944/1000`）。
- **值变化**：`delivery`（`GetTickCount` **墙钟**）。

## 2. 影响
- **任何「重生成 == 仓库现状」的判据都不成立**（旧 golden 天然 stale）；用逐字节相等当门是错判据。
- golden 含**墙钟字段**（`delivery`）⇒ **天然不可逐字节复现**。§13 允许"行为等价 + 自报身份"，但**必须明说哪些字段不可复现**。

## 3. 要求（口径）
- 列「哪些 golden 已 stale、哪些字段不可复现」的清单口径：**不可复现 = 墙钟/GetTickCount 派生字段**（至少 `delivery`；`MasterRoyaltyTick` 已按 §36 不记不比）。
- 结论化的回归判据应改为「改动不引入**语义相关**变化」：**结构 diff 的新增键 ⊆ 已登记 schema 变更集，值变化 ⊆ 墙钟集**，逐条归类（附命令）。
- 后续是否**整批重生成**所有 magic golden（一次把 schema/墙钟对齐）——决策点，非今晚解决。

## 4. 证据（本卡生成时）
对 skeleton：结构 diff `ADDED ⊆ {schema}`、`CHANGED = {delivery}`、`SinSu/recall 相关 = 空`、`未归类 = 空`。

## 5. 整类根因：离线替身「造怪 0 抽」⇒ RNG 流偏移（不止 SINSU）
`OfflineWorldEngine.RegenMonsterByName` 造怪 **0 抽**，而 Rust `create_monster_oracle` 真抽（`bins/game-svr/src/mongen.rs:498/501` 等）⇒ **凡走 `RegenMonsterByName` 的叶子，RNG 流在两实现间偏移**，后果有二（同一根因）：
- **`Rng` 长度/逐值不同源**（SINSU 实测 golden 6 抽 vs Rust 8 抽：前 5 同、Rust 多 3 造怪抽）⇒ 这类 leaf 不宜逐值比 Rng（是**替身不忠实**，非产品错）。
- **帧面 RNG-payload 偏移**（例：SINSU 640 `SM_MAGIC_LVEXP` 的 `tag`=训练 `Random(3)+1`，golden=2 vs rust=3）⇒ 与 drain 窗口无关。
⇒ 凡 `# act` 召唤/造怪类叶子，评审时先问"这个值是否来自造怪/train 抽"；是则从比较面排除并注明此根因。
