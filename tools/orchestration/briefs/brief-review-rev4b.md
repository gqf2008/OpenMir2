# 任务：独立审查（本轮身份 openmir2-reviewer-4）——**只审这三张卡**，其余待审留给 rev 车道

工作区：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\rev4b`（detached at master `d22dd03d`）。
每审一张卡用 `git checkout --detach <head>` 切到该 head 取判据，**切前 `git status` 必须干净**（要改正就改后还原）。
身份：先 `walgit collab join --repo . --principal openmir2-reviewer-4 --key C:\Users\gxh\.walgit\keys\openmir2-reviewer-4.ed25519 --push walgit`。

## 审什么（做完这三张就收工）

### 1. `openmir2-m2-record-fields` @ `95e0dbbb`（作者 worker-2）

- **owner 口径裁定在卡内 comment `a0fb915c`**：① 接受"一体化 patch + 分级可独立复跑的 oracle 基准"替代原卡的 6 个拆分提交；② 原卡"照抄
  `[OnSerializing] Serialized()` 兜底"一条**按实测口径判**（MemoryPack **不触发**它；null 数组序列化为长度 **-1**，实测 `nulls_len=722`）——
  **不得**以"没实现兜底"为由驳回。
- 你要独立跑：四门禁（隔离 target）；13 条单测里 **`encode == oracle` 字节**这个方向；两条红检自己改回坏版本必须红
  （`Ability` 字段序、`decode_items` 长度）、还原必须绿；确认旧 `player_data_oracle.txt` **未被改动**、新基准的生成命令可重跑（与 sha256 对得上）。

### 2. `openmir2-m2-map-loading` @ `16fa2da4`（作者 worker-8）

- owner 侧已做过一次**真图数字**独立复核（卡内 comment `cd7e879b`：0.map `high=162711 low=109 walk=327180` + `doors=79` + 门状态共享组 21、
  0100 `0/223/47`、0129 `7x10 59/0/11` 逐项相同）——**数字不用重做**；你要做的是**语义/边界**复核：
  ① Point 路径偏差（源码 `Path.Combine` 语义 vs 实现按 `<map 绝对路径>.txt`）复刻是否正确；
  ② `MapInfo` 双引号 `CaptureString` 形态的"跳过该行"与 oracle 抛异常面是否**等价**（不等价要指出差在哪）；
  ③ 门状态共享在 `|dx|=10` 边界的同构性；
  ④ 合成件的两条红检（bit 翻转、覆盖顺序）自己跑一遍。

### 3. `openmir2-tick-bench-baseline-gate` @ `b3d6f427`（作者 worker-7）

- 判据（卡内）：`--baseline` 相对门禁（同一次运行内交替 A/B；P50 劣化 >10% 或 P99 中位数劣化 >30% ⇒ RED；缺档 SKIP 不静默通过）；
  **阳性对照**（把 `EntityStore::get` 换成线性扫 ⇒ 三档全 RED、exit 1）；**阴性对照**（未改代码 ⇒ GREEN、exit 0）；
  P99 复测报告 §6 逐条归宿（accepted risk 写理由 / M5 挂 M5 卡）；四门禁。

## 纪律

- **只审这三张**。每张**先读线程**：若已有 `openmir2-reviewer-3` 的 `review` 覆盖**同一 head** ⇒ **跳过并登记**（别撞同一评审面）。
- 只读被审代码；要改正就在隔离 target 里改后还原，并把"改坏必红 → 还原必绿"的原始输出留档到 `E:\tmp\openmir2-watch\rev4b-ev\`。
- **不 merge、不移动卡状态**；`review` 的 `parent` 用该线程最后一条 entry 的 oid。
- 现在 `m3` 车道在跑 M3 首验并**独占 5000/7000/7100/7200** ⇒ **不要起栈**（这三张都不需要栈）。
- 收工写 `E:\tmp\openmir2-watch\last-rev4b.md`：审了哪三张、每张结论与实跑证据路径、跳过与原因。
