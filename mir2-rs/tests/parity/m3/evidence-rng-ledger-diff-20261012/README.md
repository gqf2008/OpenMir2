# M3②③ 真机两侧 RNG 账本 diff · 证据片（2026-10-12 18:28–19:00）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- 任务来源：brief 顶部 owner 2026-10-12 18:25（**Rust 真机活栈 trace + 两侧账本顺序 diff**）
- base = `refs/heads/master` `6eec1e2e`；head 见 patch 条
- 检索：本目录 `00-commands.txt`（**命令留档，先读它**）；报告 `../M3RNG两侧账本diff与活栈消费者缺口报告-20261012.md`

## 一、结论（如实登记，不声明 DoD② 达成）

1. **Rust 活栈账本从空白变有读数**：总取数 **160**、攻击路径 146、噪声 **14**（8.75%）、distinct site 9。
   - 攻击站点：`combat.rs:109/111/736` 各 **36**、`combat.rs:606/579` 各 **19**；非战斗只有 8+6 抽。
   - **同侧逐字节可复现**：r1 == r2（sha256 `4963e9f6…`）。
2. **C# 活栈账本**：总取数 **479,028**、攻击路径 267、噪声 **478,761（99.94%）**、distinct site 76
   （`AnimalObject.Wondering` 202k+、`MonGetRandomItems` 100k+、`Merchant.Run` 114k、各类 `..ctor`/`CreateMonster`/`RegenMonsters`…）。
3. **两侧顺序 diff**：
   - 全流第一处分歧 = index 0（C# `BaseObject..ctor` vs Rust `main.rs:2856`）；
   - 攻击路径第一处分歧 = 第 1 段第 3 位（C# `attack_success` vs Rust `hit_struck_damage`）；
   - 缺消费者：Rust 活栈**没有** C# 的 70 个非攻击站点（478,761 抽 = 99.94%）；
     Rust 攻击路径**不抽** `attack_success`（C# 4 次）与 `atk_power_base`（C# 5 次）。
4. **定性**：(i) 缺消费者（占绝大多数）＋ (ii) 攻击路径站点同但次序/次数不同 ＋ (iii) 起点不同，三类**并存**。
   ⇒ 单靠 (a) 重播种不足以对齐；两侧都确定性，根因是**消费者集合不同**。
5. **夹具发现**：C# pin 重试 12 轮从未到 `expected=2`（11 轮 4 / 1 轮 5）⇒ `attack_diff.py` 逐 k 护栏
   fail-closed（PREREQ）；本片**未执行逐 k diff、不声明 DoD② 达成**。Rust 活栈 84/120 击「前方格无目标」。
   两条都**只登记**，归属 m3bag / m2-svr 写集。

## 二、命令 → 原始件对应

| # | 命令（要点） | 原始件 |
|---|---|---|
| 0 | 起栈前九端口实拍（`E:/MirServer/port-status.ps1`） | `commands/00-ports-before.txt` |
| 1 | C# 栈起（`E:/tmp/m3cs-tick/start-csharp-hook.ps1 -Seed 42`） | `commands/01-csharp-stack-start.txt` |
| 2 | C# 冒烟 20（**失败**：`attempts=5 > expected=2` ⇒ 夹具 pin 重试 finding） | `commands/02-csharp-smoke-20.txt` |
| 2b | C# 120（12 轮重跑，每轮新前缀；权威=`m3csr2c12` / 会话 `csharp-20261012-185128`） | `commands/03-csharp-120.txt`（其余轮次在仓外 `E:/tmp/m3cs-r2/csharp-120-attempt*.txt`） |
| 3 | C# 栈停 | `commands/04-csharp-stack-stop.txt` |
| 4 | Rust 栈起（`start_rust_stack.ps1 -RustBinDir E:/tmp/m3cs-r2-target/debug -Seed 42`；`MIR2_RNG_LOG` 经环境继承） | `commands/05-rust-stack-start.txt` |
| 5 | Rust 冒烟 20（会话 `rust-20261012-185442`） | `commands/06-rust-smoke-20.txt` |
| 6 | Rust 权威 120（r1，`-PeerSession csharp-20261012-185128`，会话 `rust-20261012-185617`） | `commands/07-rust-120-crossside.txt` |
| 7 | Rust 同侧重跑 120（r2，会话 `rust-20261012-185955`） | `commands/07b-rust-120-r2.txt` |
| 8 | Rust 栈停（r1/r2 各一次）+ 十端口实拍 | `commands/09-rust-stack-stop.txt`、`commands/09-rust-stack-stop-r1.txt`、`commands/08-rust-stack-stop-smoke.txt`、`commands/15-ports-after.txt` |
| 9 | 账本机判工具 selftest（5/5） | `20-rng-ledger-diff-selftest.txt` |
| 10 | Rust / C# 账本 summary（站点 + canonical 计数） | `21-rust-summary.{txt,json}`、`22-csharp-summary.{txt,json}` |
| 11 | 攻击路径 canonical 序列（逐行） | `23-csharp-attack-order.txt`、`24-rust-attack-order.txt` |
| 12 | 攻击路径逐位 diff / 全流 diff | `25-diff-attack-only.{txt,json}`、`32-diff-full.{txt,json}` |
| 13 | 逐段（每击）形状 | `26-csharp-segments.{txt,json}`、`27-rust-segments.{txt,json}`、`28-segdiff.{txt,json}` |
| 14 | Rust 活栈同侧可复现（r1==r2 逐字节） | `29-rust-r1-r2-identical.txt` |
| 15 | 夹具读数（pin 重试 12 轮 / 84 次「无目标」） | `33-pin-attempts.txt`、`31-rust-live-84-no-target.txt` |
| 16 | **判决性实验**（owner `bbd1390c`）：3 次 C# 基线（各自新起栈 + 独立 trace） | `41-pin-experiment.txt`、`42-exp-runs.txt`、`experiment/*` |
| 17 | Q3：用当前 align 重算旧 c2-vs-c3（先 rebuild_attack_my 补 `my`） | `40-q3-c2-vs-c3-current-align.txt` |

> **判决性实验要点**：3 次 C# 基线 attempts=4/5/5；attempts 不同（4↔5）与**相同**（5↔5）时
> (a) `attack_seq` 与 (b) 攻击路径取数值序列**都不同**，攻击路径取数条数也变（275 vs 273）
> ⇒ pin_attempts 门**既不充分也不必要**；根因是全局 RNG 被并发噪声消费者共享 ⇒ C# 侧同侧本就不可复现
> （本窗口 3/3 各不同）。旧 c2-vs-c3 在当前 align 下仍差 39 项 ⇒ 旧归因非纯 align 假象。
> 本片**不改门**，请 owner 依此裁决。

## 三、会话（`sessions/`）

| 会话 | 侧 | 击数 | pin attempts | 备注 |
|---|---|---|---|---|
| `csharp-20261012-185128` | csharp | 120 | 4（>expected 2） | 跨侧基准（`m3csr2c12`） |
| `rust-20261012-185617` | rust | 120 | 3（>expected 2） | **r1**，账本权威（trace 见 `29`） |
| `rust-20261012-185955` | rust | 120 | **2**（=expected） | r2，同侧重跑，trace 与 r1 逐字节同 |

每会话入库 5 件：`attack_raw.jsonl` / `attack_seq.jsonl` / `attack_seed.json` / `baseline.json` / `bot.log.txt`。

## 四、口径与边界

- 账本 = C# `MIR2_RNG_LOG`（`MIR2_RNG_SITE=1`，逐次 `seq kind arg result site`）与
  Rust `crates/shared/src/rng_trace.rs`（**同形**）；两侧 canonical 映射见 `rng_ledger_diff.py::CANON_RULES`。
- 机判工具 = `mir2-rs/tests/parity/m3/rng-trace-probe/rng_ledger_diff.py`（本片新增；不依赖 m3bag 写集）。
- 判据现状：DoD② 仍 = `attack_diff.py diff` 逐 k=0，**未达成、未放宽**；本片判定 = 账本 site + 顺序 diff。
- 未动：`src/**`、`sql/**`、`MirClient`、`crates/**`、m3bag 的 `attack_rng_ledger.py` / `run_attack_pair.ps1` / `gate_guards.ps1`。
- hash 口径：`SHA256SUMS.txt` 按仓库字节（LF 归一）；复核 `python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-rng-ledger-diff-20261012`。
- 本片**未 merge、未自签 review**。
