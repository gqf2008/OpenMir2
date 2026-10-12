# `MIR2_RNG_STATE_LOG` 状态 dump 管线（m2-svr 半）——`start_rust_stack.ps1 -StateLog`

卡 `openmir2-m2-rust-state-log-plumb`（worker-11 / worktree `m2-svr` / branch `worktree-m2-gamesvr-host`）。

## 1. 为什么需要这一片（缺口）

M3②③ 隔离两相的目标可用性门 `mir2-rs/tests/parity/m3/server_state_gate.py` 要**两侧都强制**：

- **C# 半**：`start-csharp-hook.ps1 -StateLog` 把 `MIR2_RNG_STATE_LOG` 写进影子 GameSvr 的环境，
  StartupHook 在每个玩家攻击起点落一行 `SERVER_STATE <seq> <json[]>`；
- **Rust 半**：`bins/game-svr/src/state_log.rs` 读**同一个**环境变量落同形一行
  （已由卡 `openmir2-m2-rust-server-state-dump` 交付，reviewer-6 approve）。

但**启动器没有这条管线**：`tools/m2-acceptance/start_rust_stack.ps1` 起 `game-svr.exe` 只**继承调用方环境**
⇒ 只有 operator 手工在起栈 shell 里设过 `MIR2_RNG_STATE_LOG` 才落 dump，否则 Rust 半静默没有产出，
`run_attack_pair.ps1:1164` 的 NOTE（「-Side rust 无服务端状态 dump」）就永远成立、门只保护 C# 一半。
这与已收口的 `MIR2_RNG_SEED` 种子管线（卡 `openmir2-m3-rust-stack-seed-plumb`）是同一缺陷类。

## 2. 改了什么

| 文件 | 变更 |
| --- | --- |
| `tools/m2-acceptance/rust_seed_plumb.ps1` | 新增 `Resolve-Mir2StateLog`（显式 `-StateLog` > `$env:MIR2_RNG_STATE_LOG`；父目录不存在 ⇒ **抛错 fail-closed**）、`Get-StateLogInjectedLine`（`RUST_STATE_LOG_INJECTED=<path>`）；`Start-SeededProcess` 增可选 `-StateLog`，与 `-Seed` 同一次 `Start-Process` 里注入/还原（未传 ⇒ 逐字节同旧行为） |
| `tools/m2-acceptance/start_rust_stack.ps1` | 新增 `-StateLog <path>` 参数；解析后打印 `STATE_LOG: …`；`Start-One` 增 `-StateLog`，仅对 `game-svr` 传 `$stateLogInfo` 并打印 `RUST_STATE_LOG_INJECTED=<path>` |
| `tools/m2-acceptance/check_state_log_plumb.ps1` | **新**：15 项可判红/绿自检（见 §4） |
| `tools/m2-acceptance/README.md` | 新增 §0b.2（`-StateLog` 口径、命令、fail-closed、证据路径、夹具接线余量） |

**行为等价**：`-StateLog` 未给且 `$env:MIR2_RNG_STATE_LOG` 未设 ⇒ **不注入、不打 `RUST_STATE_LOG_INJECTED` 行、
不建 dump 文件**，与改动前逐字节一致（④a/④b 证明）。`-Seed` 路径一字未动（`check_seed_plumb` 回归 15/15）。

## 3. 判据（`checks-green.txt` / `checks-table.txt`，rc=0）

`powershell -File tools/m2-acceptance/check_state_log_plumb.ps1 -GameSvrExe <exe>` ⇒ **15/15 GREEN**。
其中 ③ 用**真 game-svr.exe**（空闲端口 + 临时 `Server.conf`）证明：传路径 ⇒ 日志出现
「服务端逐击状态 dump 已开启」且该路径被建出文件；不传 ⇒ 无该行、不建文件。全部**不占真端口、不起完整栈**。
另：启动器本体的 fail-closed 也实跑过——`start_rust_stack.ps1 -Force -StateLog E:\no-such-dir-xyz\rng.tsv`
⇒ 打印 `RED: … 父目录不存在: E:\no-such-dir-xyz`、**rc=1、未 START 任何进程**（`launcher-failclosed.txt`）。

## 4. 改坏必红（`redcheck-injection-removed.txt`，rc=1）

把 `rust_seed_plumb.ps1` 的注入行 `if ($injectState) { $env:MIR2_RNG_STATE_LOG = [string]$StateLog.Path }`
删掉（改注释）⇒ 自检 **②a / ③a / ③b FAIL，total=15 ok=12 fail=3，STATE_LOG_PLUMB FAIL，rc=1**。
随后从备份还原，`cmp` 与改前**逐字节相同**，复跑 15/15 GREEN（`checks-green-after-restore.txt`）。

## 5. 回归

- `check_seed_plumb.ps1` ⇒ **15/15 SEED_PLUMB GREEN**（共享 `rust_seed_plumb.ps1` 的扩展未伤 `-Seed`）。
- `tools/m2-acceptance/selftest.ps1`（M2 手册口径自检）⇒ **47/47 GREEN**。

## 6. 四门禁（`gate-*.txt`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0（输出空） |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `cargo test --workspace --offline` | rc=0；**84 suites / 686 passed / 0 failed / 16 ignored** |
| `cargo run -p msgcodegen --offline -- --check` | rc=0；**604** 条常量一致 |

## 7. 边界 / 归属

- 写集：仅 `tools/m2-acceptance/**`（管线 + 自检 + README + 本证据目录）。**未动** `mir2-rs/**`、
  `crates/**`、`src/**`、客户端、表结构、脚本语法、数据格式、夹具（`tests/parity/m3/**`）。
- **夹具接线余量（属 m3bag 写集）**：`run_attack_pair.ps1 -Side rust` 读 Rust 侧 dump 并调同一 `server_state_gate.py`
  （`:1164` 的 NOTE 尚未去掉）。本片只保证「Rust 栈起得来 dump、且 `RUST_STATE_LOG_INJECTED` 可判」。
- **依赖（叠片）**：本提交位于仍处【待审】的上一片 `openmir2-m2-rust-server-state-dump`（head `4ba36368`，
  reviewer-6 `9795e068` approve、未 merge）之上；`patch --base refs/heads/master` 的差量因此含那一片。
  独立复审可只看本片的新提交（`state dump 管线`）。owner 可顺序合并两片。
- 本 tick **未起完整栈、未占真端口**：自检用空闲端口 + 临时目录；落片时 `netstat` 实拍 5000/7000/7100/7200 归他车道窗口。
- **未声明 DoD② 达成、未放宽判据**：两侧逐 k=0 仍是 m3cs 取窗跑隔离两相的事。

## 8. 干净检出复核

在 detached worktree（autocrlf ⇒ 重新物化）上：`verify_sha256sums.py --check-extra <本目录>` ⇒ **GREEN 15/15**；
`check_state_log_plumb.ps1` ⇒ **15/15 GREEN**（`clean-checkout-state-log.txt`）；`check_seed_plumb.ps1` ⇒ **15/15 GREEN**
（`clean-checkout-seed.txt`）。验证器**自身输出不入清单**（同 `tools/evidence-hash/README.md` 口径）。
