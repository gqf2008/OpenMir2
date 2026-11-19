# openmir2-aoi-incremental-default-on

> 状态：**实现完成（2026-11-19，分支 `card/aoi-incremental-default-on`，基于 M7=`67fa1cdc6`）**。
> 来源：Owner 拍板（经 opc-07 转达）：AOI 增量切为**默认路径**，保留一键回退。

## 为什么翻默认

- 4k 两轮 + 10k 两轮同构建对照：增量 `ns_avg` 比 legacy 低一个量级、tick 速率更高；
- 差分/矩阵/fuzz 三类等价牙全绿（逐 tick 可见集、离开集、`view_updates`、`aoi_order`）；
- 增量实现已合并进 master（M1=C3D3C271）并在窗口里实测，不是"待验证的新路径"。

## 改动

| 位置 | 改动 |
|---|---|
| `mir2-rs/crates/world-bevy/src/aoi.rs` `parse_search_mode` | 语义翻转：未设置 ⇒ 增量；**字面 `0`（可含空白）⇒ legacy（kill switch）**；其余取值一律增量 |
| 同文件 `AoiSearchMode` 文档 | `Legacy` 标为 kill switch 路径，`Incremental` 标为默认 |
| 同文件 `aoi_mode_labels` | 新增**纯函数**：`[AOI_STATS]` 的 `mode/cells_sem/cand_sem` 由实际生效模式单源派生（`report_line` 改用它） |
| `docs/designs/aoi-phase2-design.md` | 新增 6e 节（默认翻转依据、kill switch 语义、遥测如实要求、验证计划）；§7 回退条目更新 |
| `docs/designs/aoi-phase2-impl-handoff.md` | 「默认 legacy / 灰度开增量」两处改为「默认增量 / `=0` 回退」；验收顺序里的合并后灰度改为两轮窗口 |

### 语义细节（写死，避免歧义）

- `MIR2_AOI_INCREMENTAL` **未设置** ⇒ `Incremental`；
- `"0"` / `" 0 "` / `"0 "`（trim 后等于 `0`）⇒ `Legacy`；
- `"1"`（兼容旧配）、`"true"`、空串、任何无法识别的串 ⇒ `Incremental`。
  理由：默认已是增量，唯一有语义的取值是"关"；把非法值判成 legacy 等于让写错的变量值
  **偷偷降级**到慢路径，与"默认"自相矛盾。
- 运行期语义不变：`OnceLock` 启动时读一次，**切换/回退靠重启**。
- 其它环境变量（`MIR2_AOI_STATS`、`MIR2_AOI_STATS_TICKS`、`MIR2_AOI_DBG`）语义不变。

## 测试（默认值不再是任何用例的隐式前提）

- 差分/矩阵/fuzz 用例本来就**显式传 `AoiSearchMode`**（`run_search(mode, …)`、`run_fuzz_mode(seed, …)`），
  不读进程默认值 ⇒ 不受翻转影响，继续做等价验收；
- 仓库内**没有**依赖"默认 legacy"的用例：`output_aoi_system` 在测试里无直接调用，
  也没有任何用例断言旧 `mode=legacy` 字面量（`grep -rn "AOI_STATS" --include=*.rs` 核对）；
- 新增/改写三颗牙（都在 `aoi::tests`）：

| 用例 | 钉住什么 | 变异后红在哪 |
|---|---|---|
| `aoi_search_mode_parses_safely` | 默认 = 增量；`0` 才 legacy；其余取值增量 | 默认改回 legacy ⇒ 首条 `assert_eq` 红；kill switch 改成 `== "1"` ⇒ `kill switch 取值 "0" 必须回退 legacy` 红 |
| `aoi_mode_labels_follow_effective_mode` | 两种模式的 `(mode, cells_sem, cand_sem)` 各自如实 | 增量支标签改成 legacy ⇒ `增量行必须标 incremental/na/touched` 红 |
| `aoi_stats_line_reports_effective_mode_labels` | 汇总行取**生效模式**，不得写死 | `report_line` 写死 `("legacy","real","visited")` ⇒ 缺 `mode=incremental cells_sem=na cand_sem=touched` 红（原文见下） |

变异证据（每次变异后 `cp` 回备份并断言源文件 sha 相等：`RESTORE_SHA_OK`）：

```
[MUTATION-C] report_line 标签写死 legacy（首处）
thread 'aoi::tests::aoi_stats_line_reports_effective_mode_labels' panicked at ...:
汇总行必须如实标注生效模式（缺 mode=incremental cells_sem=na cand_sem=touched）：
[AOI_STATS] ... mode=legacy cells_sem=real cand_sem=visited
test result: FAILED. 0 passed; 1 failed
```

## 验收

1. **M8 全量门禁**：C# 工程 build + `dotnet test TestProject1`；Rust `fmt`/`clippy -D warnings`/
   `test --workspace` 三元组 + `msgcodegen --check`；anchor gate；工具门 **TOTAL 70**（`oid_gate` 单列 NEED）。
2. **真机窗口两轮（合并后，由 opc-07 开窗）**：
   - 第 1 轮**不设任何环境变量**：`[AOI_STATS]` 行 `mode=incremental`、`cells_sem=na`、
     `ns_avg` 在 3–5 万 ns 量级；`in_world/failed` 落在 4,224–4,436 / 640–820；
   - 第 2 轮 `MIR2_AOI_INCREMENTAL=0`：`mode=legacy`（kill switch 生效）；
   - 两轮都要 `enter=req=rep`、判停器零触发、`slow_requests=0`；网关四个计数照旧全收。

## 边界

- 只改**默认值**与遥测标签的派生方式；两条搜索路径的算法、顺序、过滤链一律不动；
- 不改协议/帧字节；不扩容、不改网关策略（默认更快会推高网关 hold 压力，观测面照旧）。
