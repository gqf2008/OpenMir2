# L2-5 P1 · M0 交接（每区 host 循环骨架，2026-11-23）

> 状态：**M0 收口**——A1/A2/A3 三步全部过硬门（门 rc=0 + 最终树 N=1 与 golden 逐字节相等）；
> **区 k 仍未接线**（`spawn_minimal_zone` 未被改动，区 k 不参与生产调度，那是 M1）。
> 本文件是 A4 的交付物：**docs-only，不含任何代码改动**。
> 设计稿：[`l25-zone-loop-m0-design-20261123.md`](l25-zone-loop-m0-design-20261123.md)；
> 评估稿：[`l25-per-zone-host-loop-scope-20261123.md`](l25-per-zone-host-loop-scope-20261123.md)。

## 0. 一句话

`sim_loop` 的主循环体已抽成「每区可实例化」的 `HostLoop::tick`（区级状态 + 进程级服务借用 +
段级 `PhaseSet` 守卫 + 区 k 空实例构造 `for_zone`），**区 0 行为逐字节不变**；区 k 的生产接线
留给 M1。

## 1. 三步证据表

| 步骤 | master | 本地分支提交 | 门（opc-27 合并门） | N=1 order / multiset | 行 / 字节 |
| --- | --- | --- | --- | --- | --- |
| A1 结构（纯搬移） | `1629bba1abd4` | `eb4265dbf` | FMT/CLIPPY/TEST rc=0（4683 passed） | `b78156a7…` / `bd4b13ba…` | 86 / 11980 |
| A2 段级 `PhaseSet` 守卫 | `015a88c268b5` | `f4fc7e365` | 同上（4683 passed） | 同上 | 86 / 11980 |
| A3 `for_zone` + 空实例用例 + 补牙 | `6e26d776a333` | `e4251a012` + `3a5dbb230` | 同上（4685 passed） | 同上 | 86 / 11980 |

- **N=1 证据来源**：`a7` 在最终树的 `n1_stdout_equivalence` 运行（N1-A3 原文
  `RUN_START 07:45:25 hour=7 bucket=1`；两树各自与 golden 相等、首差行无；两树 sha256
  `7b581d3f…` / `4c61e048…`；`git status` 跑前跑后 0 行）。门 RunId 与产物目录以 opc-27 / a7
  的台账为准，本地不留副本。
- 端口探针修复（`2d1829786`，A1 合并门被 ToDesk 占 24670 阻塞后的修复）随 A1 一并合并。

## 2. 关键决定与它们的「临时性」

| 决定 | 现状 | 临时性 / 移除条件 |
| --- | --- | --- |
| **按值交还** `tick(self, services, io) -> TickOut` | 让 2,767 行循环体逐字不变；每 tick 一次 **17,760 B** 结构 move（无分配/clone/锁，≈3–5 µs） | **M1 方法化**改 `&mut self` 后消除 |
| **`#[rustfmt::skip]`**（`HostLoop::tick`） | 避免套 `if` 后整段重排、把纯搬移 review 变成 2,500 行不可读 diff | **M1 拆 `fn phase_*` 时移除该 skip 并统一格式化** |
| `#[allow(unused_mut)]`（`let mut rec = tick_world_head_with_specials(`） | 该字面量是 **3 处源码扫描牙**的顺序锚，不得改 | 牙改用例后一并去掉 |
| `#[allow(clippy::needless_borrow)]` | `&mut rec` / `&rec` 保持 A1 原调用形状（`rec` 现为 `&mut TickRecord`） | 同上 |
| **`PhaseSet(u32)`** 位集 | `const`、无分配/无锁；每段一次整型与运算 + 分支（16–17 次/tick，已记明为边界） | 长期保留（M1+ 区 k 的子集调度靠它） |
| **`rec → WORLD_HEAD` 断言** | 取 `scratch.rec` 前 `debug_assert!(phases.has(WORLD_HEAD), "…（phases=0x{:05x}）")` | **必须保留**并随新段扩展（M1） |
| **`PhaseSet::M0_MIN = WORLD_HEAD \| WORLD_TAIL`** | 区 k 空实例最小集；M0 只单测用（`#[allow(dead_code)]`） | M1 起为区 k 生产入口 |
| **`TickScratch` 只装两个跨段值** | `rec`（WorldHead→后续段）、`castle_broadcast_texts`（Periodic→PostHead） | 新增跨段值必须先给「谁产出/谁消费」清单 |

## 3. M1 硬边界（给下一位实施者）

1. **源码扫描窗口（window-bound 用例）**：多条用例用
   `body = lines[start .. next-fn]` 作扫描范围——起点是 `l.trim() == "fn sim_loop("`，
   终点是**其后第一条 `l.starts_with("fn ")`（列 0 顶层 fn）**。
   ⇒ **在 `fn sim_loop` 与该目标之间新增「列 0 的顶层 `fn`」⇒ 窗口被截断 ⇒ 计数 0 ⇒ 假红**。
   - **已知 window-bound 用例**：`p31_production_wiring_anchors`、
     `drain_close_health_residual_call_is_wired_inside_drain_stamina`、
     `shutdown_save_is_only_reachable_from_shutdown_branch`、
     `closed_arm_calls_close_cleanup_residue_once_in_decommented_production_view`，以及所有用到
     `count_seq` 的窗口用例（本轮实测：见证若用顶层 fn 实现，前两条与 `closed_arm…` 立即假红）。
   - **重要区分（0c 2026-11-23 复核）**：`impl HostLoop { fn phase_* }` 的方法行带缩进
     （`    fn phase_*`），**不匹配** `starts_with("fn ")` ⇒ **M1 的「方法化」本身不会截断窗口**；
     真正的风险只有「新增**顶层** `fn`」。
   - **order-only 用例**（`find("fn sim_loop(")`/`position(trim == "fn sim_loop(")` 再比大小，
     十余处 + 3 处 `find`）：新增 fn **不破序**（只要目标仍位于 `fn sim_loop` 之后），风险只在
     window-bound 那一类。
   - **但方法化会改「逐字行」**：这批用例按字面匹配（如 `world.tick_tail(&mut rec);`、
     `drain_pk_flag_downlink(&mut world, &online, &gates);`、
     `let guild_run_tick = world.clock_ms as i32;`）——M1 方法化若把 `&mut world` 改成
     `self.world`，必须同步改用例字面量（或保持绑定名不变），**不可静默放宽**。
2. **`rec → WORLD_HEAD` 的 `debug_assert` 必须保留**，M1 新增消费 `rec` 的段时同步扩展
   （取 `scratch.rec` 前断言；负例 `zone_tick_without_world_head_panics` 会盯住）。
3. **`spawn_minimal_zone` 在 M0 仍未接线**：区 k 不参与生产调度、不出帧。M1 才接。
4. 段集见证 `RAN_SEGMENTS`（`#[cfg(test)]` thread-local）+ 空实例用例的
   `assert_eq!(ran, M0_MIN.0)` 与「`M0_MIN` 组成写死」两条断言，M1 改段集时须同步更新。

## 4. 证据与口径（延续约定）

- **端口探针修复的回归保护是缺位的**：`tests/support/mod.rs` 的探针已改为与子进程同形的
  `0.0.0.0`（`port_bindable_for_child`），但 **Windows 上造不出「127 可绑 / 0.0.0.0 冲突」的
  判别夹具**（`SO_EXCLUSIVEADDRUSE` 占用得到的是相反形状）⇒ **没有合成牙**；验收依赖真实环境
  （ToDesk 占 24670 时 `session_face_accepts_loopback_despite_whitelist_miss` 通过）+ 一次性
  进程内判别式（`old_127_probe_ok=true / new_0_probe_ok=false`）。
- **门统计口径**：从测试输出的 `test result:` 行累加 `passed/failed/ignored`；**不取后台任务
  退出码**（`cmd; echo $?` 会被 `echo` 掩盖成 0——本轮实际踩过一次，日志里 FAILED 2 条）。
- **`wg` 的 status 写缺陷**（卡 `wg-collab-entry-parent-loadfailed`）：在本仓对**某些既有线程**
  `kind=status` 稳定 `LoadFailed`（连最小 `{"status":"done"}` 都失败），而 `kind=comment` 同线程
  正常 ⇒ **卡面内容以 comment 落库**是临时做法；状态列由卡 owner / Leader 改。

## 5. 未做的与不做的（各一句理由）

- **`World.OutputAoi` / `AoiMonsterWalk` 算法**：属 AOI 增量/算法重构，分区后按区摊开已够 10k
  ⇒ 不在 M0/M1 最短路径（评估稿 §12.5）。
- **`collect+sort` 顺序契约改造**：需 Owner/验收层面的顺序契约结论（已另立卡），且分区后其成本
  按区摊开 ⇒ 不在本项范围。
- **open-loop 负载生成器**：与「服务端分区」正交，**不阻塞 M0/M1 的代码路径**；但 **10k 的可测性
  仍受 C4（BotSrv 进程内 OOM）限制**——当前已验证的可测档位是 **1k / 2k（3 线）**，**4k 只有工具
  可用性证据**（BotSrv 自设 4GiB 堆上限 OOM、8GiB 下也只到 `world_ok 3739/4000`，卡
  `capacity-4k-botsrv-oom`），**10k 需 BotSrv 分片后才能测**。open-loop 只是「未来若要测真实容量
  上限」的另一条路，不是现在的必需品。

## 6. 交付清单

| 项 | 位置 |
| --- | --- |
| 设计稿（含 §3.0 A2 落地、§3.1 gap 归属、§3.2 见证与扫描窗口坑、附录端口探针） | `docs/issues/l25-zone-loop-m0-design-20261123.md` |
| 评估稿 | `docs/issues/l25-per-zone-host-loop-scope-20261123.md` |
| 实现（结构/守卫/区 k 构造） | `mir2-rs/bins/game-svr/src/lib.rs`（`HostLoop`/`HostServices`/`TickIo`/`PhaseSet`/`TickScratch`/`impl HostLoop`） |
| 测试端口探针 | 五个 `bins/*/tests/support/mod.rs`（同文本） |
| 卡 | `zone-loop-m0-p1`；缺陷卡 `wg-collab-entry-parent-loadfailed` |
