# items IT-18 行为牙与防空转负例证据（2026-11-20）

本目录是 IT-18（`parity-gap-items.json`）`verified` 的**行为牙 + 防空转（Z）负例**落盘证据，
由 `capture.py` 在本机**单 crate 定点**产出：每条先变异源码/比较器 → 跑对应用例（应红，
原文落 `<name>_red.txt`）→ 还原（`useitem.rs` sha256 必须回到 `sha_before.txt`）→ 复跑
（应绿，落 `<name>_green.txt`）。`commands.txt` 汇总每条 rc 与判定；`final_green.txt` 为还原后
`useitem::tests` 全绿原文。

| 变异 | 目标 | 期望红在 |
|---|---|---|
| `M1_unbind_count` | 解包 `for _ in 0..6`→`0..5` | `unbind_stdmode31_success_matches_oracle_fixture`：解包 6 件 5≠6 |
| `M2_unbind_dura` | 解包件 `dura: tpl_unbind.dura_max`→`0` | 同上：解包件 dura 0≠7 |
| `Z4_gate_removed` | `z4_ok` 恒 `true` | `z4_golden_nonzero_rust_zero_red`：golden 非零而 Rust 全零未检出 |
| `Z5_layer_removed` | 两层判断恒 `true` | `z5_single_layer_red`：只 SM 一层被判合格 |
| `Z6_notice_equal_stubbed` | `notices_equal` 恒 `true` | `tooth_notice_priority_changed_red`：改 priority 未被检出 |
| `D_drain_cap_removed` | 测试 rig `drain_ok` 上限 `8→1` | `matches_oracle_eat_fixture`：抽空上限异常（`hit_cap=false max_passes=8` 被判不合格） |

## Z4/Z5 适用面（口径，非"每例强判"）

- **Z4**：由 golden 决定——该例 golden 有非零期望（帧/通知/状态/解包/RNG 任一）时，Rust 侧必须有非零锚；
  golden 全零（合法失败例，如只发 `SM_EAT_FAIL` 后无副作用）⇒ **该例不解析 Z4 断言**。
  `final_green.txt` 的 stderr 会打印"本批 N 例中 M 例适用 Z4、N−M 例因 golden 全零不适用"。
- **Z5**：夹具级——至少一例同时有「SM 帧>0」与「RM 通知>0 或状态有变」即合格；`final_green.txt` 打印"K 例带两层"。
  其余例**不该强判两层**：纯失败例只有 `SM_EAT_FAIL`、无 RM/状态变化是合法行为，逐例强判会误红。


## 口径

- 只用单 crate：`cargo test -p game-svr --lib <filter> -- --test-threads=1`（`CARGO_BUILD_JOBS=6`）。
- **不改产品行为**：M1/M2 是 `use_item` 解包实现的临时变异，Z4/Z5/Z6 是判定面辅助函数的临时变异；
  每轮结束 `useitem.rs` 字节还原（`sha_before.txt` 证明）。
- 本证据只证明"断言能被咬住"；`verified` 的对外覆盖率仍以**合并后 master** 的
  `E:/OPC/tools/coverage-report.ps1` 输出为准（合并前对外是 `物品 7/19`）。

复现：`python -I -B capture.py`（先确保 `mir2-rs` 已构建；会短暂修改 `useitem.rs` 后还原）。
