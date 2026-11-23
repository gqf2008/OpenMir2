# 跨区帧三条 follow-up 落码说明（2026-11-23）

> 卡：`openmir2-botsrv-recv-null-leftworld-gate` 之后由 Leader 派的 L2-5 follow-up（三条；FRAME_SRC
> per-frame class 暂缓）。分支 `card/cross-frame-followups`（自 master `5ead482cc`）。
> **本文件说明三条的实现口径、门控与「生产默认零参与」的实现方式**，供验收与后人对照。

## 0. 一句话

① 验收件旁路行加 `surface`（A13 定责）；② 条件抑制计数 `depart_frames_mirror_suppressed`（不污染 ⑤
分母）；③ `MIR2_ZONE_MIRROR_DIAG=<path>` 门控诊断（镜像滞留的直接状态证据）。三条都**不改变默认构建
的输出**，只有显式开 env / feature 才产出。

## 1. 旁路行加 `surface`（A13 定责）

**做什么**：`frame-src` 验收构建下，`MIR2_CROSS_FRAME_SRC_LOG` 指向的文件里**每条**交付行都带
`"surface"`，取值 `RefMsg` / `RunFrames` / `NpcMerchant` / `FeatureChanged` / `MountRide` /
`MerchantDisappear` / `DepartFrames`（即 ⑤ 的七面名；非镜像来源写 `-`）。

- 行格式（一行一帧）：`{"tick":…,"socket":…,"len":…,"sha256":"…","src":"CrossZone","surface":"RefMsg"}`
- 实现：`surface` 随 `CrossZoneFrame` 从**产出点**（`note_mirror_recipient`）带到**交付点**
  （`drain_zone`），交付点写行时一起落盘；`CrossRecipientSurface::name()` 是唯一的取值表。
- **A13（WALK 双发）用法**：同一 `(socket, sha256(body))` 在 `frames.ndjson` 里出现两次时，旁路文件里
  **带 `surface` 的那条**指明第一路是哪个 face；**没有旁路行的第二条**就是第二路（不在 ⑤ 上）——
  「哪条带标记、标记的是哪个 face」即可判定第二路是谁，不需要猜。
- **生产默认零参与**：整条旁路在 `#[cfg(feature = "frame-src")]` 下编译；默认构建**一行都不编译**，
  `surface` 字段也不存在（`#[cfg(feature = "frame-src")]` 的 struct 字段），内存与行为零变化。

## 2. `depart_frames_mirror_suppressed` 计数

**做什么**：`drain_depart_frames` 在「观察者是镜像且镜像开」处**条件抑制**（`continue`，理由见该函数
注释：收件区 AOI 的 removed-filter 自己会出同一 `SM_DISAPPEAR`）；每次抑制在 `continue` **之前** +1，
作为 perf TSV 新列 `depart_frames_mirror_suppressed`（本 tick 增量，紧邻 `depart_wrn_suppressed`）。

- **语义**：被条件抑制的帧数。被抑制的帧**不进** ⑤ 账 ⇒ `Delivered` 不因抑制虚增（这是牙的判据）。
- **牙**（仓内可跑）：`depart_frames_mirror_observer_suppressed_when_mirror_enabled` 断言
  镜像开时 `depart_frames_mirror_suppressed` 本 tick +1，且 `cross_recipient_depart_frames()`（⑤ 第七面
  投递）**不变**；另有 `perf_tsv_header_matches_values_with_new_column` 断言表头/值列数一致、且计数
  落在**同名列**上（防「表头加列、值没加/错位」复发）。
- **生产默认零参与**：计数只在 TSV 行里出现；TSV 仅在 `MIR2_PERF_COUNTERS` 设了才写，N=1 默认输出
  不含任何 perf 内容（stdout 逐字节判据不受影响）。

## 3. `MIR2_ZONE_MIRROR_DIAG=<path>` 门控诊断

**做什么**：每个切片区在 `before_phase2` 的镜像刷新（`apply_mirrors`）**之后**，把该区的镜像 id 集合
（`world.mirror_ids()`，槽位序 ⇒ 确定性）写成一行；**集合变化时**才写。

- 文件格式：首行 `zone,tick,mirror_ids`；其后每行 `<zone>,<tick>,<id;id;…>`（空集合 = 空串）。
- 用途：把「离开后镜像是否滞留」从**行为推断**（看帧/看 WRN）变成**直接状态证据**——某区在某 tick
  的镜像集合里仍出现过期 id 就是滞留，不需要从帧面反推。
- **生产默认零参与（实现方式）**：env 未设 ⇒ 首次调用只做一次 `std::env::var_os` 判断，之后
  `enabled()` 只读一个 `Relaxed` 原子；**不调 `world.mirror_ids()`、不分配、不加锁、不写文件**。
  env 设了但文件打不开 ⇒ 打一条 WRN 并保持禁用（诊断不得影响游戏路径）。**不新开 cargo feature**。
- **可核凭据（实测）**：`mirror_ids()` 的唯一调用入口是 `maybe_write_world`；单测
  `mirror_ids_not_called_when_disabled` 用 `#[cfg(test)]` 计数断言「env 未设 ⇒ 调用次数 = 0；
  env 设了 ⇒ 恰好 1 次」——**不是靠代码阅读**。变异：删掉 `enabled()` 守卫 ⇒ 禁用态计数变 1 ⇒ 红。
- 调用点：`BarrierOps::before_phase2`（切片路径才有 `BarrierOps`；N=1 零调用）。

## 3.1 TSV 新列的消费者审计（跨卡共享面，不能只修一个读法）

`perf_counters.tsv` 的列名/表头消费者逐个核过：

| 消费者 | 读法 | 结论 |
| --- | --- | --- |
| `tools/botload/perf_tsv_summary.py` | 按列名 `perf[name]`（缺列 ⇒ 显示 `absent`/`None`） | 已加 `depart_frames_mirror_suppressed`（汇总 + `cross diag:` 行）；`--selftest` rc=0 |
| `tools/hygiene/tsv_recompute_legacy.py` | `detect(header)` 只按**列存在性**判版本；非错位列一律 `header.index(name)`；固定下标只用于**旧产物**的错位段 | 不假设列数；新增列不影响；`--selftest` rc=0 |
| `tools/botload/run_l25_slice.ps1` | 只消费 `perf_tsv_summary.py` 的 `summary.json` 键，不直接读 TSV 列 | 不受影响 |
| `tools/hygiene/tool_selftest_gate.py` | 登记两工具的 `--selftest` | 两自检已实跑 rc=0 |
| `docs/issues/l2-5-step5-cross-recipient-frames-design-20261122.md`、`…-depart-frames-design-20261123.md` | 设计文档里提到列名（记录性） | 不改历史记录；本文件是新列的权威说明 |

## 4. 验收计划（窗口开时按此跑）
1. 仓内门（默认构建）：`cargo fmt --all -- --check`、`cargo clippy --workspace --all-targets -- -D warnings`、
   `cargo test --workspace`、`cargo run -p msgcodegen -- --check`。
2. `frame-src` 构建：`cargo test -p game-svr --lib --features frame-src`（旁路行 `surface` 断言）。
3. N=1 两树逐字节（**Leader 跑**）：默认构建零行为。
4. 一次短两区运行（窗口）：`MIR2_PERF_COUNTERS=…`（看新列）与 `MIR2_ZONE_MIRROR_DIAG=…`（看镜像集合）；
   `frame-src` 验收件仅在需要 A13 定责时单独构建。

## 5. 未做

- **FRAME_SRC per-frame class**：暂缓（等 A13 是否需要）。
- 任何协议/帧面改动：没有；三条都只是观测与计数。
