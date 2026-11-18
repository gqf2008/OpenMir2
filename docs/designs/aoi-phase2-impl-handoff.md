# AOI phase 2 增量实现：开工单（handoff）

> 卡 `openmir2-aoi-incremental`。骨架分支 `card/aoi-phase2`（合并 head 见 master 历史）只含**测试与文档**，
> 生产路径零改动。本单是下一块（`card/aoi-phase2-impl`）的开工依据——**照此实现，不要重新调查**。

## 一、实现要点（已核实）
1. **类型隔离**：`struct AoiIncremental { cell_viewers: BTreeMap<AoiCell, BTreeSet<i32>>, last_cell: AoiCellSnapshot }`；
   **legacy 路径不得持有它的引用**；开关只在 `output_aoi_system` 选调用哪条路径（**行为层开关**，
   不允许"两条都跑再挑结果"）。
2. **三类触发**：由已实现的 `derive_aoi_change_feed(prev, curr)` 产出 `entered / left / moved`；
   `moved` 只记**跨格**，格内微动不产生条目、不触发观察者更新。
3. **受影响观察者**：`cell_viewers[old_cell] ∪ cell_viewers[new_cell]`（`entered/left` 用其在图上的格）。
4. **单实体可见性更新**必须复用 legacy 的过滤链（顺序不可改）：`store.get` 存在性 →
   Ghost/Hidden/`FixedHideMode` → 窗口包含 `|dx|<=R && |dy|<=R`。
5. **插入位置按扫描序**：x 外层升序、y 内层升序，同格内按加入序 —— 与 `search_view_range_with_updates` 一致。
6. **`SearchTime` 门时点**：增量更新只在门的同一时点生效（"legacy 在 tick 内看到格子变化的确切时点"
   已由对齐牙钉住；未到期 tick 不得重算）。
7. **开关只在进程启动时读一次**（`OnceLock`）：**运行期不可切换，灰度靠重启**——同一进程内「跑双实现对比」只在测试构建里做（差分骨架），生产不跑双路径。
7. **仪器**：索引维护耗时单列 `idx_avg`（线性桶，桶宽 32 µs），**不计入** `ns_*`。

## 二、验收顺序（每一步给原文）
1. **矩阵 + fuzz 双实现全绿**：legacy vs incremental，含每 tick 每观察者的 **候选全集对齐**
   （`aligned_checks == snapshots`）与 **`aoi_order` 逐 tick**；
2. **六颗触发牙逐支删**（进格 / 出格 / 观察者移动 / 上下线换图 / 间隔门到期 / 未到期跳过），
   每颗给"删后红在哪一条"的原文；
3. **4k 两轮性能对照**（同构建两轮、`ns_avg` + 32µs 线性桶分位、`idx_avg` 单列、`in_world/failed` ±2%）；
4. **10k 评估**；
5. 合并前：`fmt` / `clippy -D warnings` / 全量 `cargo test --workspace` 三元组（rc 落文件）/ 工具门 TOTAL；
6. 合并后：夜间灰度 1 轮（增量开）。

## 三、已知风险（都有对应牙，但必须在实现里主动处理）
- **候选集**：Ghost 与"已死未清"（store 已移除、格子里还在）都属于 legacy 候选集 —— 快照必须同源收录；
- **`SearchTime` 门**：增量若在未到期 tick 生效，差分在慢速场景仍会绿、生产会花屏/漏帧；
- **扫描序插入**：可见集顺序影响出帧，增量插入位置必须与扫描序一致；
- **边界格**：`|dx| == R` 必须可见（闭区间）；四角同判。

## 四、可直接复用的测试资产（已在 master）
- `[AOI_DIFF]` 骨架：`TickSnapshot` / `compare_snapshots`（五类分歧分支，count 最先）/ 场景 `run_diff_scenario` /
  固定种子 fuzz `run_fuzz(0x5EED1234, 120)`；
- 七颗前置牙：确定性、集合、顺序、removed、view_updates、count、fuzz 同种子；
- 六类矩阵场景（`aoi_matrix_*`，逐 tick 对齐内建，`aligned_checks` 输出）；
- `aoi_order` 逐 tick 确定性（`aoi_order_snapshot_is_deterministic_across_runs`）；
- 派生馈送 `derive_aoi_change_feed` + 四类转换牙；
- 候选全集对齐：`aoi_universe_capture_set` / `aoi_last_scan_universe` / `aoi_snapshot_from_map`。

## 五、边界
- 生产路径在骨架阶段**零改动**（所有新增项在 `#[cfg(test)]` 下）；判据：release `game-svr.exe` 中
  `CANDIDATE_SET_MISMATCH` / `AOI_ALIGN` / `AOI_MATRIX` 出现 0 次，同源测试二进制为 2/1/2。

## 六、执行步骤 / 每步绿判据 / 风险清单（opc-07 要求，压缩前固化）

### 第 1 步：`entered` / `left` 两类触发的可见性增删（含 `idx_avg` 首版）
- **改哪些行**：`aoi.rs` 新增 `struct AoiIncremental { cell_viewers: BTreeMap<AoiCell, BTreeSet<i32>>, last_cell: AoiCellSnapshot }`；
  `search_view_range_incremental` 改为：由 `derive_aoi_change_feed(&last_cell, &aoi_snapshot_from_map(map))` 取 feed，
  只处理 `entered/left`（`moved` 暂按"旧格+新格都受影响"保守处理并标注 TODO）；
  可见性判定**复用** legacy 的过滤链与扫描序插入；`idx_avg`（线性桶，32 µs）计时覆盖"快照重建 + feed 派生 + 索引更新"。
- **绿判据**：`[AOI_DIFF] DETERMINISM_OK`（不变）+ 六条 `[AOI_MATRIX] … aligned_checks=N nonzero_visible=M`（数值与 legacy 一致）
  + `[AOI_INC] STAGE1_EQ` 换成真正的双实现比较行（`legacy` vs `incremental` 逐 tick 相等）+ `[AOI_ORDER] OK` 不变。

### 第 2 步：加 `moved`（跨格）
- **改哪些行**：把第 1 步的"moved 保守处理"换成：受影响观察者 = `cell_viewers[old] ∪ cell_viewers[new]`，
  对每个受影响观察者只更新该实体（移除/插入按扫描序）。
- **绿判据**：同第 1 步全部；另加 `[AOI_MATRIX] viewer_moves` 的 `nonzero_visible` 保持 12/12。

### 第 3 步：受影响观察者精确定位
- **改哪些行**：`cell_viewers` 的维护点收敛到"观察者搜索/移动/上下线时才更新"，并给 `idx_avg` 分 bucket 报告。
- **绿判据**：同前 + 4k 两轮（`ns_avg`/`idx_avg` 分列）。


### 第 1 步的前置设计决定（已核实，执行时先做这个）
**先把可见性谓词抽成两侧共用的函数**，否则增量与 legacy 的过滤链必然分叉：
- 现有谓词（`search_view_range_with_updates` 循环体中部）：
  `!other.invisible && !other.ghost && !other.fixed_hide_mode && !other.ob_mode`
  `&& (viewer_is_play || !other.death)`
  `&& (base_pass || (other.master.is_some() && |dx|<=3 && |dy|<=3) || other.race == ACTOR_RACE_PLAY)`，
  其中 `base_pass = viewer_is_play || viewer_master.is_some() || viewer_nasty || viewer_want_ref`；
- 另有两处**随扫描发生副作用**的行为必须保留：非玩家观察者对过期对象做 `cell.remove_at`（`CELL_OBJECT_EXPIRE_MS`）、
  玩家观察者对可见怪物投递 `view_updates.push((other_id, viewer_id))`；
- 抽函数时 legacy 必须**改为调用它**（行为不变的纯重构，先用现有测试证明全绿），
  增量路径再复用同一个函数 —— 这是"两侧解同一个问题"的结构性保证，比差分兜底更强。
- 顺序：**先做这次纯重构并提交**（绿证 = 现有 28 条 AOI 测试全绿），再做 `entered/left`。

### 每步必须出现的输出行
`[AOI_DIFF] DETERMINISM_OK snapshots=…`、`[AOI_DIFF] FUZZ_OK seed=0x5EED1234 steps=120 snapshots=240`、
六条 `[AOI_MATRIX] <name> OK snapshots=… aligned_checks=… nonzero_visible=…`、`[AOI_ORDER] OK ticks=12`、
以及**双实现比较行**（`legacy` 与 `incremental` 逐 tick 相等）。

### 风险清单（每步都要盯）
- **候选集**：Ghost 与"已死未清"必须进快照（对齐牙已覆盖）；
- **`SearchTime` 门时点**：增量更新不得在未到期 tick 生效；
- **扫描序插入**：x 外层升序、y 内层升序、同格按加入序；
- **`idx_avg` 口径**：只含索引维护（快照重建 + feed 派生 + 索引更新），不含搜索段；
- **开关**：默认 legacy，运行期不可切换（灰度靠重启）。
