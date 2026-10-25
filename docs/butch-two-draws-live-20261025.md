# butch 真机轮：账本恰两抽（卡 `openmir2-butch-two-draws-unblock`，tier R）

> 取数时点：2026-10-25 13:53–13:56 +08。证据根：`E:/tmp/window/butch-run/out`（本文件所有数字都从该根读出）。

## 1. 这一轮验什么

oracle `src/M2Server/Player/PlayObject.Operate.cs:702 ClientGetButchItem`：**成功 = RNG 账本恰两抽**

| 抽 | 位置 | 模数 | 值域 | 账本判定 |
|---|---|---|---|---|
| 1 | `Operate.cs:722` | `Random(16)` | `+5 ∈ [5,20]` | 屠宰函数内 `Next(max) max=16` **恰 1 条** |
| 2 | `Operate.cs:723` | `Random(201)` | `+100 ∈ [100,300]` | 屠宰函数内 `max=201` **恰 1 条**，且 `seq` 在 16 之后 |

`SM_BUTCH(637)`（`:749` 无条件发）**不是判据**，只作伴随证据；`SM_SKELETON(33)` 仅 `BodyLeathery<=0` 时发。

## 2. 怎么跑的

- 机器：`cargo/rustc/mir2-server` 全 0、仅 mysqld；测试端口全空；签 `stack-claim` → 跑 → `stack-release`。
- 基线：master `64f49e902`；窗口分支 `win/butch-run`；**Rust 侧二进制 = 本检出 `cargo build --release` 的产物**
  （profile 用 `-InnerArgs "-ServerExe <dev-1>/mir2-rs/target/release/mir2-server.exe"` 显式指定，避免用到别的车道的旧二进制）。
- 两侧同场景、同种子（`-Seed 42`）、同参数；C# 侧由 `start-csharp.ps1` 注入
  `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG` + `MIR2_RNG_SITE=1`。
- 场景：仓库版 `butch_two_draws.json` 仍带 `blocked`（card_window 见 `blocked` 即 rc=2），本轮用
  **去 `blocked` 的窗口副本** `E:/tmp/window/butch-run/scenario.json`（其余逐字一致，`--check-scenario` rc=0）。
- 命令：

  ```powershell
  CONFORMANCE_PASSWORD=... python -I -B tools/conformance/card_window.py `
    --card openmir2-butch-two-draws-unblock `
    --profile E:/tmp/window/butch-run/profile.json `
    --scenario E:/tmp/window/butch-run/scenario.json `
    --rng-log --timeout-ms 60000 --out E:/tmp/window/butch-run/out
  ```

## 3. 结果（四个数）

### ① 账本没有被裁（`rng_log_trim.trimmed`）

| 侧 | trimmed | 行数 | 字节 | sha256（前 16 位） | 运行窗（`run.json`） |
|---|---|---|---|---|---|
| C# | **false** | 135,724 | 10,562,020 | `8d44c935316b3daf` | 13:53:52 → 13:54:06 |
| Rust | **false** | 106,251 | 5,745,855 | `683b4b9e181b62c6` | 13:55:12 → 13:55:24 |

两本账的原始体量与 `kept_*` 完全相等 ⇒ **一份都没被裁**，`changes_on_mutation` 类「证据被裁」风险未发生。

### ② 两抽（`tools/window/butch_ledger.py`，两侧 `TWO_DRAWS_OK`）

| 侧 | `max=16` | site | `max=201` | site | 次序 | 扣减量（`n10` / `meat`） |
|---|---|---|---|---|---|---|
| C# | 1 条（seq 119187） | `M2Server.Player.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722` | 1 条（seq 119188） | `…@PlayObject.Operate.cs:723` | 16 在前 ✓ | 13 / 131 |
| Rust | 1 条（seq 89066） | `crates\world\src\world.rs:7756` | 1 条（seq 89067） | `crates\world\src\world.rs:7757` | 16 在前 ✓ | 11 / 165 |

- 两侧 `max=201` 的**其它**条目（C# 104 条、Rust 12 条）全部来自 `MonGen.cs:815` / `mongen.rs:899,900`
  ⇒ **按 site 过滤**是这条判据成立的前提（工具 `--selftest` 牙 d 咬这一点）。
- 跨侧扣减量不同（13/131 vs 11/165）**不是失败**：两侧 RNG 流的消费点不同，判据是「每侧恰两抽 + site 归属 + 次序 + 值域」。
- 账本侧结论文件：`out/csharp/two-draws.json`、`out/rust/two-draws.json`。

### ③ 637（仅记录，不作判据）

两侧各 **1 条**：C# `seq=157 / t=12999ms / recog=1101538188 / param=330 / tag=330`；
Rust `seq=110 / t=10949ms / recog=605 / param=330 / tag=330`。

### ④ 字段映射与击杀

| 侧 | kill 结果 | 目标格 | swings / retries / waited | 死亡帧 | `CM_BUTCH` 发送 |
|---|---|---|---|---|---|
| C# | `dead` | `(x=329, y=330)` | 3 / 0 / 0 | `ident=34`（`SM_NOWDEATH`） | `recog=1013013390`（ActorId）、`param=329`、`tag=330` |
| Rust | `dead` | `(x=329, y=330)` | 3 / 0 / 0 | `ident=34` | `recog=199`（ActorId）、`param=329`、`tag=330` |

- `recog` 是**目标 ActorId**、`param/tag` 是**尸体格 X/Y**——与 `PlayObject.Message.cs:830` 的
  `ClientGetButchItem(nParam1, nParam2, nParam3, wParam)` 一致；旧口径「recog=MakeLong(X,Y)」在这两行数据上被直接否证。
- `waited=0`：开局 32 只鸡里就有相邻目标，等待分支未触发（与 `openmir2-probe-track-target-adjacency` 的分析一致）。
- `SM_SKELETON(33)` 两侧 0 条（`BodyLeathery` 未归零），符合「33 只在 `BodyLeathery<=0` 时发」。

## 4. 帧面对比是 UNDECIDABLE（与账本判据无关，但必须写清）

`report.json`：`verdict=UNDECIDABLE`、`frames.compared=75`。原因两类：

1. `unclassified_ident`：`32 / 34 / 44 / 637 / 656` 在产物里出现但 `rules.json` 无条目；
2. `pending_ignore_rule`：`ident 14` 的 ignore 规则仍 PENDING，且本次剔除两侧对不上（oracle=2 / rust=1，形态不同）。

⇒ **帧面判据尚不具备**（这是规则表覆盖问题，需另卡），但它**不影响**本卡判据（账本两抽）。
不得把这条 UNDECIDABLE 写成 PASS，也不得写成 FAIL。

## 5. 残留与复现

- 证据根 `E:/tmp/window/butch-run/out`（两侧 frames / run.json / db 快照 / rng-stream.tsv / two-draws.json / report.json）。
- 窗口临时物：`profile.json`（含 `-ServerExe`）、`scenario.json`（去 blocked 的副本）——不入库，路径已登记。
- 参考栈 `E:/MirServer` 未触碰（pid 14040/6808/7732/5832/22248/16280 与开窗前一致）；测试栈已停，端口无 LISTEN。
- 复现判据（无需重跑，直接读证据根）：

  ```powershell
  python -I -B tools/window/butch_ledger.py E:/tmp/window/butch-run/out/csharp/rng-stream.tsv --side csharp
  python -I -B tools/window/butch_ledger.py E:/tmp/window/butch-run/out/rust/rng-stream.tsv   --side rust
  ```
