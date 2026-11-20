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
