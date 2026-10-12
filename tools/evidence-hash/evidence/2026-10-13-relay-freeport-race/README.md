# `2026-10-13-relay-freeport-race` — 门禁卫生：relay 的 `free_port()` TOCTOU 假红

- 卡：`openmir2-m2-flaky-relay-testports`（issue `3d6c83c1`）。
- 车道：`m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m2-relay-testports`（base = master `518a9ad9`）。
- 写集：`mir2-rs/bins/game-svr/tests/**` + `mir2-rs/bins/game-gate/tests/**`（共用 helper 放在
  `bins/game-svr/tests/support/mod.rs`，game-gate 用 `#[path]` 引入同一份）。
  **零 `.rs` 生产代码、零 `src/ sql/ MirClient`、未起任何服务端栈、未占 5000/7000/7100/7200。**
- 隔离 target：`CARGO_TARGET_DIR=E:/tmp/m3bag-relay-target`，全部 cargo 命令 `--offline`。

## 1. 问题（owner 实拍）

owner 在 master `f2eb43d6` 的**合并树**上跑 `cargo test --workspace --offline`：
`bins/game-gate/tests/relay.rs` 的 `relays_open_data_heartbeat_and_close` /
`bad_login_is_kicked_and_nothing_is_forwarded` **FAILED**（`os error 10048`），且 cargo fail-fast
让后续 target 根本没跑；单跑该 target 却 11/11 绿 ⇒ 并发端口竞态假红。

机制：旧 `free_port()` = `bind(:0)` → 取值 → `drop`（TOCTOU）。释放的端口回到 OS 临时端口池，
并行用例的假服务 `bind(:0)` 抢走它 ⇒ 网关子进程 bind 失败退出，而客户端 `connect_with_retry`
连到了**别人的假服务**上（两边断言形态都错）。详见 `01-root-cause-and-mechanism.txt`。

## 2. 改动（照已 approve 的 `openmir2-m2-flaky-relay-nowm@00af1c78` 样板）

| 文件 | 改动 |
| --- | --- |
| `bins/game-svr/tests/support/mod.rs`（新） | 共用「无竞态端口 / 临时目录 / 就绪门」：`next_port()`（进程内原子唯一 + 非临时段 `[20000,32000)` + pid 错开 + 有界重试）、`unique_temp_dir()` + `TempDirGuard`、`wait_log_contains()`、`wait_child_ready()`、`require_ready()`；`#[cfg(test)] mod teeth` 三条判据 |
| `bins/game-svr/tests/relay.rs` | 删本地 `free_port()` 与重复 helper；`Child::spawn*` 改 async + **就绪门**（等 `游戏服务[…:{port}]已启动…`，未就绪 fail-closed）；端口全改 `support::next_port()`；临时目录改 `support::unique_temp_dir` |
| `bins/game-gate/tests/relay.rs` | 删本地 `free_port()`；新增 `Gate` helper（conf 落唯一临时目录 + stdout 落盘 + 就绪门 `游戏网关[…:{port}]已启动…`）；10 处 spawn 改 `Gate::start`/`connect`/`shutdown` |
| `bins/game-svr/tests/seed_inject.rs` | 同 crate 同源：`write_conf` 的三处 `free_port()` 改 `support::next_port()`（否则它在并行下也会因网关端口被抢而假红） |

**为什么新口径真的无竞态**：候选端口段 `[20000,32000)`（`32000 < 32768`）与 Windows 动态段
`49152+`（本机 `netsh` 实测）及 Linux 默认段 `32768+` **都不相交** ⇒ 并行用例的 `bind(:0)`
不可能返回候选；原子计数器保证同进程内不重号，pid 错开避免多 test binary 同起点。
就绪门是最终契约：只连**本子进程**自己 bind 成功后打印的地址，杜绝「连到别人监听」。

## 3. 判据与读数

| 文件 | 判据 | 结果 |
| --- | --- | --- |
| `01-root-cause-and-mechanism.txt` | owner 实拍 + 机制 + 本机动态段抽样 | 定性成立（见文件） |
| `02-target-green-streaks.txt` | 两个 target **各 3 连跑**（默认线程）+ `--test-threads=32` 各 3 连跑 + 两个 test binary **并发** 6 轮（各 16 线程）+ `seed_inject` | 全 GREEN（gate 14/14、svr 16/16、seed_inject 6/6） |
| `03-workspace-gates.txt` | `cargo test --workspace` **×3** + 四门禁 | **86 suites / 744 passed / 0 failed** ×3；fmt/clippy/msgcodegen(604) rc=0 |
| `04-mutation-red.txt` | 改坏必红：`next_port()` 退回旧口径 ⇒ teeth 必红；还原后逐字节相同 | **RED 2/3**，还原 sha256 一致 |
| 本文件 | — | — |

`bins/game-svr/tests/support/mod.rs` 的 `mod teeth`（三条判据，均带改坏必红语义）：
1. `ports_live_outside_the_os_dynamic_range`：候选必须在 `[PORT_LO,PORT_HI)`；退回旧口径立即红；
2. `ports_are_unique_across_parallel_callers`：32 线程 ×8 次调用不重号（原子计数器）；
3. `legacy_probe_release_port_is_left_unprotected`：**确定性复现旧机制**——旧 `free_port()` 释放的
   端口未受保护，紧接着 `bind` 得 `AddrInUse`（Windows 10048，即子进程失败那步）。

## 4. 边界与登记

- 只动 `bins/*/tests/**` 与其共用 helper；`git diff --name-only master..HEAD -- src sql MirClient` 为空。
- **同源未修（登记）**：`bins/login-gate/tests/relay.rs`、`bins/sel-gate/tests/relay.rs`、
  `bins/login-srv/tests/login.rs`、`bins/db-srv/tests/live_*.rs` 也各有一份 `free_port()`
  （同族）。本卡写集只点名 game-svr/game-gate 两个 target（另加同 crate 的 `seed_inject`），
  故未越界扩大；建议 owner 需要时另开一片套用同一 helper。
- 未改任何被测算子的语义/断言（就绪门只是把「盲 connect」换成「等本进程 bind 成功」）；
  game-gate 子进程日志改重定向到临时目录，测试不再读到子进程 stdout——断言均基于 5000 侧帧，不受影响。

## 5. 残留物清单

- 临时物：`E:/tmp/m3bag-relay-target`（编译 target，按惯例保留）、`E:/tmp/m3bag-relay-ev/**`
  （原始日志/证据中间件）、`E:/tmp/leg-*.log`（mutation 压力日志）、`E:/tmp/svr-c*.log`/`gate-c*.log`。
  记忆里工作树那 8 个未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-*`（别的卡残留）**未动、未入库**。
- 端口：本轮**未起** game-svr/game-gate/MySQL 服务端栈；测试只起进程级子进程并随用例收尸。
- `git status`：tracked 树仅本片四个文件变更。
