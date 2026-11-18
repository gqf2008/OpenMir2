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
