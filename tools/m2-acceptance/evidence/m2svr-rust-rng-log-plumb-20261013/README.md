# `MIR2_RNG_LOG` 逐次 rng 流水管线（m2-svr 半）——`start_rust_stack.ps1 -RngLog`

卡 `openmir2-m2-rust-rng-log-plumb`（worker-11 / worktree `m2-svr` / branch `worktree-m2-gamesvr-host`）。

## 1. 为什么需要这一片（缺口）

RNG 消费者账本（`mir2-rs/tests/parity/m3/attack_rng_ledger.py` / `rng_ledger_diff.py` 的 site/order
逐 k diff，Claim B）要 C# 与 Rust 两侧的**同名**逐次 rng 流水：

- **C# 半**：rng-hook（`mir2-rs/tests/parity/m3/rng-hook/StartupHook.cs`）读 `MIR2_RNG_LOG` 落
  `rng-stream.tsv`（`seq · kind · max/arg · result · site`，头两行 `INJECT_OK <seed>` + `SEED_PROBE`）；
- **Rust 半**：`crates/shared/src/rng_trace.rs` 的 `TRACE_ENV` = `MIR2_RNG_LOG`，由
  `RandomNumber::with_seed` → `enable_from_env()` 读同一个变量开流水（`crates/shared/src/rng.rs:249-252`；
  旁路仪器，每次取数只多一个 `AtomicBool` 读，值序列与改动前逐字节相同）。

但**启动器没有这条管线**：`tools/m2-acceptance/start_rust_stack.ps1` 起 `game-svr.exe` 只**继承调用方环境**
⇒ 只有 operator 手工在起栈 shell 里设过 `MIR2_RNG_LOG` 才有 Rust 流水，否则账本拿不到 Rust 半。
这与已收口的 `MIR2_RNG_SEED` 种子管线（卡 `openmir2-m3-rust-stack-seed-plumb`）和
`MIR2_RNG_STATE_LOG` 状态 dump 管线（卡 `openmir2-m2-rust-state-log-plumb`）是**同一缺陷类**
（继承可用但不可见，忘了就静默）。

**额外的陷阱（本片新增的交叉守卫）**：`rng_trace::enable_from_env` 在 `MIR2_RNG_SEED` 未设/为空时
**关闭流水**（只打 stderr）⇒ 若只给 `MIR2_RNG_LOG` 而没给种子，栈看起来正常却永远不落 `rng-stream`。
这正是本管线要消灭的"静默空转"，因此在起栈前 fail-closed。

## 2. 改了什么

| 文件 | 变更 |
| --- | --- |
| `tools/m2-acceptance/rust_seed_plumb.ps1` | 新增 `Resolve-Mir2RngLog`（显式 `-RngLog` > `$env:MIR2_RNG_LOG`；父目录不存在 ⇒ **抛错 fail-closed**）、`Get-RngLogInjectedLine`（`RUST_RNG_LOG_INJECTED=<path>`）、`Assert-Mir2RngLogSeed`（交叉守卫真值表，可单测）；`Start-SeededProcess` 增可选 `-RngLog`，与 `-Seed`/`-StateLog` 同一次 `Start-Process` 里注入/还原（未传 ⇒ 逐字节同旧行为） |
| `tools/m2-acceptance/start_rust_stack.ps1` | 新增 `-RngLog <path>` 参数；解析后打印 `RNG_LOG: …`；交叉守卫调用 `Assert-Mir2RngLogSeed`（RngLog 有而 Seed 无 ⇒ `RED` + exit 1）；`Start-One` 增 `-RngLog`，仅对 `game-svr` 传 `$rngLogInfo` 并打印 `RUST_RNG_LOG_INJECTED=<path>` |
| `tools/m2-acceptance/check_rng_log_plumb.ps1` | **新**：15 项可判红/绿自检（见 §3） |
| `tools/m2-acceptance/README.md` | 新增 §0b.3（`-RngLog` 口径、命令、两级 fail-closed、证据路径） |

**行为等价**：`-RngLog` 未给且 `$env:MIR2_RNG_LOG` 未设 ⇒ **不注入、不打 `RUST_RNG_LOG_INJECTED` 行、
不建流水文件**，与改动前逐字节一致（②c / ④a 证明）。`-Seed`/`-StateLog` 路径一字未动
（`check_seed_plumb` / `check_state_log_plumb` 回归各 15/15）。

## 3. 判据（`checks-green.txt` / `checks-table.txt`，rc=0）

`powershell -File tools/m2-acceptance/check_rng_log_plumb.ps1 -GameSvrExe <exe>` ⇒ **15/15 GREEN**：

- ① `Resolve-Mir2RngLog` 4 项（未设 / 合法 / 环境回落 / 父目录不存在抛错）；
- ② 注入到位 3 项（cmd 子进程读到该路径 / 起完还原 / 不传则不注入）；
- ③ **真 game-svr.exe**（空闲端口 + 临时 `Server.conf`）传 `-Seed 42` + `-RngLog` ⇒ 该路径建出流水，
  且含 `INJECT_OK 42` + `SEED_PROBE`（2 项）；
- ④ 只传种子、不传路径 ⇒ 正常启动且**不建**流水文件（1 项）；
- ⑤ `Get-RngLogInjectedLine` 2 项；
- ⑥ 结构接线 + 交叉守卫真值表 3 项（`start_rust_stack` 点源管线并传 `-RngLog $rngLogInfo` / 管线含注入行 /
  `Assert-Mir2RngLogSeed` 真值表 `TRUE,TRUE,FALSE,FALSE` 且被 `start_rust_stack` 调用）。

全部**不占真端口、不起完整栈**（真 game-svr 用空闲端口 + 临时 conf）。

**启动器两级 fail-closed 实跑**（均 rc=1、`START` 计数 0、端口全空）：

- `launcher-failclosed.txt`：`-Force -Seed 42 -RngLog E:\no-such-dir-xyz-rng\rng.tsv`
  ⇒ `RED: MIR2_RNG_LOG/-RngLog 的父目录不存在: E:\no-such-dir-xyz-rng`；
- `launcher-crossguard.txt`：`-Force -RngLog <有效路径>` 但**不给种子**（用 dummy exe 目录避免起真栈）
  ⇒ `RED: 已给 MIR2_RNG_LOG/-RngLog 但无 MIR2_RNG_SEED/-Seed …`。

## 4. 改坏必红（`redcheck-injection-removed.txt`，rc=1）

把 `rust_seed_plumb.ps1` 的注入行 `if ($injectRng) { $env:MIR2_RNG_LOG = [string]$RngLog.Path }`
删掉（改注释）⇒ 自检 **②a / ③a / ③b / ⑥b FAIL，total=15 ok=11 fail=4，RNG_LOG_PLUMB FAIL，rc=1**。
随后从备份还原，`cmp` 与改前**逐字节相同**，复跑 15/15 GREEN（`checks-green-after-restore.txt`）。

## 5. 回归

- `check_seed_plumb.ps1` ⇒ **15/15 SEED_PLUMB GREEN**（`reg-seed.txt`）；
- `check_state_log_plumb.ps1` ⇒ **15/15 STATE_LOG_PLUMB GREEN**（`reg-state.txt`）；
- `tools/m2-acceptance/selftest.ps1`（M2 手册口径自检）⇒ **47/47 GREEN**（`harness-selftest.txt`）。

## 6. 四门禁（`gate-*.txt`，cwd `mir2-rs/`，`CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`）

| 门 | 读数 |
| --- | --- |
| `cargo fmt --all -- --check` | rc=0（输出空） |
| `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `cargo test --workspace --offline` | rc=0；**84 suites / 686 passed / 0 failed / 16 ignored** |
| `cargo run -p msgcodegen --offline -- --check` | rc=0；**604** 条常量一致 |

## 7. 边界 / 归属

- 写集：仅 `tools/m2-acceptance/**`（管线 + 自检 + README + 本证据目录）。**未动** `mir2-rs/**`、
  `crates/**`、`src/**`、客户端、表结构、脚本语法、数据格式、夹具（`tests/parity/m3/**`）。
- **夹具接线余量（属 m3bag 写集）**：`run_attack_pair.ps1` 的 `-RustRngLog`（`:111/1059-1061`，缺省
  `<RustStackLogDir>\rng-stream.tsv`）由 operator 起栈时用 `-RngLog` 提供；本片只保证「Rust 栈起得来流水、
  且 `RUST_RNG_LOG_INJECTED` 可判」。
- **依赖（叠片）**：本提交位于仍处【待审】的 `openmir2-m2-rust-server-state-dump`（head `4ba36368`）与
  `openmir2-m2-rust-state-log-plumb`（head `7a41aebc`）之上；`patch --base refs/heads/master` 的差量因此
  含前两片。独立复审可只看本片新提交；owner 可顺序合并三片（或 FF 到本片 head）。
- 本 tick **未起完整栈、未占真端口**：自检用空闲端口 + 临时目录；落片后 `netstat` 实拍
  5000/7000/7100/7200 全空（仅共享 MySQL 3306，归他车道窗口）。
- **未放宽判据**：Claim B / ②③ 的逐 k=0 仍是 m3cs/m3bag 取窗跑对拍的事。

## 8. 干净检出复核

在 detached worktree（`autocrlf` ⇒ 重新物化）上：`verify_sha256sums.py --check-extra <本目录>` ⇒ **GREEN**；
`check_rng_log_plumb.ps1` ⇒ **15/15 GREEN**（`clean-checkout-rng-log.txt`）；`check_seed_plumb.ps1` ⇒ **15/15 GREEN**
（`clean-checkout-seed.txt`）。验证器**自身输出不入清单**（同 `tools/evidence-hash/README.md` 口径）；
本片新产出的两个 `clean-checkout-*.txt` **入清单**（它们是判据输出，不是验证器输出）。
