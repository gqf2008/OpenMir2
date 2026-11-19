# items IT-18 行为牙与防空转负例证据（2026-11-20）

本目录是 IT-18（`parity-gap-items.json`）`verified` 的**行为牙 + 防空转（Z）+ 抽空上限（D）**证据。
两个脚本产出，均在**单 crate 定点**下运行，stdout/stderr **分段**并带时间戳（避免 cargo 编译行与
test 输出混排）：

- `capture.py`：M1/M2（`use_item` 实现变异）、Z4/Z5/Z6（判定面辅助函数变异）。每条：变异 `useitem.rs`
  → 跑对应用例（应红）→ 还原（sha256 回 `sha_before.txt`）→ 复跑（应绿）。
- `capture_drain_cap.py`：D——变异 **eat-probe** 的抽空上限 `MaxDrainPasses` 8→1（并让记录 `max_passes`
  跟随）→ 重出夹具 → 跑**主用例** ⇒ 红 → 还原源码与夹具（sha256 回 `D_sha_before.txt`）→ 复绿。

| 变异 | 目标文件 | 目标 | 期望红在 |
|---|---|---|---|
| `M1_unbind_count` | useitem.rs | 解包 `for _ in 0..6`→`0..5` | `unbind_stdmode31_success_matches_oracle_fixture`：解包 6 件 5≠6 |
| `M2_unbind_dura` | useitem.rs | 解包件 `dura: tpl_unbind.dura_max`→`0` | 同上：解包件 dura 0≠7 |
| `Z4_gate_removed` | useitem.rs | `z4_ok` 恒 `true` | `z4_golden_nonzero_rust_zero_red`：golden 非零而 Rust 全零未检出 |
| `Z5_layer_removed` | useitem.rs | `fixture_covers_two_layers` 恒 `true` | `z5_single_layer_red`：只 SM 一层被判合格 |
| `Z6_notice_equal_stubbed` | useitem.rs | `notices_equal` 恒 `true` | `tooth_notice_priority_changed_red`：改 priority 未被检出 |
| `D_drain_cap` | eat-probe/Program.cs | `MaxDrainPasses` 8→1，记录 `max_passes` 8→1 | `matches_oracle_eat_fixture`：`抽空上限异常 ... max_passes=1` |
| `D2_hit_cap_forced` | useitem.rs | 把记录的 `hit_cap` 强制读成 `true`（模拟"记录面命中上限"） | `matches_oracle_eat_fixture`：`抽空上限异常 ... hit_cap=true` |
| `D3`（drain-selftest） | eat-probe/Program.cs | `DrainPassLoop` 触顶返回 `hitCap` `true→false` | `eat-probe.dll --drain-selftest`：`always_full` 跑到 `MaxDrainPasses` 后 `ok=False` |

## D（抽空上限）说明

- **变异点在 eat-probe 而非 useitem.rs**：`max_passes` 是**记录面**（eat-probe 写进夹具）的值；
  比较器（useitem.rs）只读它并断言 `==8`。只改 useitem.rs 的常量不改变夹具记录值，红原文仍打印夹具旧值
  （`false/8`），不是"上限被改"的证据——这是上一版证据的错误，已废弃。
- **红原文为何是 `max_passes=1` 而非 `hit_cap=true`**：物品线用例只跑 **1 趟**排空（通知只入队、探针不
  `Operate` 重入），`MaxDrainPasses ≥ 1` 时**不会触顶**（`passes=1` 即空）。故真实的"上限被改"信号是
  记录面 `max_passes=1`；主用例在 `drain_ok`（**上限检查**）断言处红：`抽空上限异常 hit_cap=false max_passes=1`。
  失败点是上限检查本身，不是帧/通知/状态。
- **`hit_cap=true` 的字面牙（`D2_hit_cap_forced`，useitem.rs 变异）**：把主用例记录的 `hit_cap` 强制读成
  `true`（模拟"记录面命中上限"），主用例在 `drain_ok` 处红 `抽空上限异常 hit_cap=true max_passes=8` ——
  直接证明"`hit_cap=true` ⇒ 红"。文件 `D2_hit_cap_forced_{red,green}.txt`。
- **D3 自然命中单测（`capture_drain_selftest.py`）**：把排空趟循环抽成纯函数 `DrainPassLoop(maxPasses, hasWork,
  drainOnePass)`（`Drain` 复用之，行为不变：起于空 ⇒ 0 趟、一趟即空 ⇒ 1 趟）。自测用**假回调 `()=>true`**
  （每趟重满）跑到 `MaxDrainPasses` ⇒ 辅助函数自己返回 `hitCap=True`；`one_pass` ⇒ passes=1/hitCap=False；
  `none` ⇒ passes=0。`eat-probe.dll --drain-selftest` 打印 `ok=True`。变异"触顶返回 `true→false`" ⇒ `ok=False`（红），
  还原 Program.cs sha 后复绿。文件 `D3_drain_selftest_{green,red,restored_green}.txt` / `D3_probe_diff.txt` /
  `D3_sha_before.txt` / `D3_commands.txt`。**D3 是 dotnet 自测**（真实排空辅助函数在 C# 探针里；
  用 Rust 重写一份来测即为"转写"，不采用）。主用例 gate 拒绝 `hit_cap=true` 由 D2 证明。
- 文件：`D_probe_diff.txt`（Program.cs `8→1` 的 git diff）、`D_fixture_diff.txt`（夹具 `max_passes 8→1`）、
  `D_fixture_max_passes.txt`（变异后各例 `max_passes` 唯一值 = `[1]`）、`D_drain_cap_red.txt`（主用例红）、
  `D_drain_cap_green.txt`（还原后绿）、`D_sha_before.txt`、`D_build.txt`/`D_regen.txt`、`D_commands.txt`。

## Z4/Z5 适用面（口径，非"每例强判"）

- **Z4**：由 golden 决定——golden 有非零期望时 Rust 必须有非零锚；golden 全零（合法失败例）⇒ 不解析。
  `final_green.txt` 打印"23 例中 23 例适用、0 例因 golden 全零不适用"。
- **Z5**：夹具级——至少一例同时有「SM 帧>0」与「RM 通知>0 或状态有变」；`final_green.txt` 打印"17 例带两层"。
  其余例**不该强判两层**：纯失败例只有 `SM_EAT_FAIL`、无副作用是合法行为，逐例强判会误红。

## 口径
- 只用单 crate；**不改产品行为**（每轮字节还原，sha 自证）。
- 对外覆盖率仍以**合并后 master** 的 `coverage-report.ps1` 为准（合并前对外 `物品 7/19`）。

复现：先确保 `mir2-rs` 已构建。
- `python -I -B capture.py`：M/Z（会临时改 `useitem.rs` 后字节还原）。
- `python -I -B capture_drain_cap.py`：D（需 `dotnet`；临时改 `eat-probe/Program.cs` 并重出夹具，结束字节还原）。
