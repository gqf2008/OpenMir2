# M3②③ B-1/B-2 修复 + `0137` 真机同种子两相对拍 · 证据片（2026-10-12 15:51–16:07）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- 分支：`worktree-m3-combat-skills`；patch base = `refs/heads/master`（`e0f9adad`），head 见 patch 条
- oracle 栈：`E:/tmp/m3cs-tick/start-csharp-hook.ps1 -Seed 42`（`M2GameSvr-hook-m3cs16` + `m3/rng-hook`）
- Rust 二进制：`E:\tmp\m3cs-b1b2-target\debug`（含 B-1/B-2；`cargo build -p game-svr` 由本 head 产出）
- 触发：owner 裁决 `cc42a959`（B-1 修 / B-2 修 / R-1 用 `0137` 真机跑），执行件 = 本 head 的 `dedef0d2`

> **结论（如实登记，不声明 DoD② 达成）**：同侧同种子 `c1≡c3`、`r1≡r2` 逐 k 全等；
> **跨侧逐 k 非 0**：120 击里 **k=2** 一处不同（C# `hit dmg=2` / Rust `dodge`），其余 119 击逐 k 全等；
> 且两侧**非平凡门**未过（Rust `hits=0`，`sum=0`）。⇒ `attack_diff.py diff != 0`，判据**未达成**。
> k=2 的定位见 `perk/k2-localization.txt`：属「**攻方属性未装载**」（真机 Rust 实体无 `dc/…` ⇒ 攻击力恒 0），
> **不是**命中门公式差；修点在 `bins/game-svr/**`（§1 禁止的 m2-svr 写集），本片**只登记、不修**。

---

## 1. 种子与场景前提（F-C / R-1）

- 两侧都注入 **seed=42**：C# `INJECT_OK 42` + `SEED_PROBE 1434747710 …`；Rust `RUST_SEED_INJECTED=42`
  + 同一 `SEED_PROBE` 参照序列（见各 `sessions/*/attack_seed.json` 的 `matched` 串）。
- 目标图 **`0137`（竞技场）**：`MAP_HAS_NO_MONSTER ok map=0137 spawns=0` +
  `MAP_HAS_NO_AI_CONSUMERS ok map=0137 total_refs=0`（探针 `fixtures/mongen-map0137.tsv` /
  `ai-consumer-map0137.tsv`，`source_verified=True`）。owner 已独立核过该图无刷怪/NPC/守卫/任务。

## 2. 命令（逐条原始件）

| # | 命令（要点） | 原始件 |
|---|---|---|
| 0 | 起栈前端口实拍（`E:/MirServer/port-status.ps1`） | `commands/00-ports-before.txt` |
| 1 | C# 栈起（`start-csharp-hook.ps1 -Seed 42`） | `commands/01-csharp-stack-start.txt` |
| 2 | `run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3cb1sm`（**冒烟**） | `commands/02-smoke-csharp-20.txt` |
| 3 | `… -Side csharp … -AttackCount 120 … -Prefix m3cb1c1`（**c1**，跨侧基准） | `commands/03-csharp-120-c1.txt` |
| 4 | C# 栈停 | `commands/04-csharp-stack-stop.txt` |
| 5 | Rust 栈起（`start_rust_stack.ps1 -RustBinDir E:\tmp\m3cs-b1b2-target\debug -Seed 42`） | `commands/05-rust-stack-start.txt` |
| 6 | `… -Side rust … -AttackCount 20 … -Prefix m3cb1rm`（**冒烟**） | `commands/06-rust-smoke-20.txt` |
| 7 | `… -Side rust … -AttackCount 120 … -Prefix m3cb1r1 -PeerSession csharp-20261012-155255`（**r1**，跨侧） | `commands/07-rust-120-crossside.txt` |
| 8 | `… -Side rust … -AttackCount 120 … -Prefix m3cb1r2`（**r2**，同侧重跑） | `commands/08-rust-120-r2.txt` |
| 9 | Rust 栈停 | `commands/09-rust-stack-stop.txt` |
| 10 | C# 栈起（第 2 次） | `commands/10-csharp-stack-start2.txt` |
| 11 | `… -Side csharp … -AttackCount 120 … -Prefix m3cb1c2`（**c2**，同侧重跑） | `commands/11-csharp-120-c2.txt` |
| 12 | C# 栈起（第 3 次，**输出为空文件**；`rng-stream.tsv` 的单个 `INJECT_OK` + 仅 120 组 `GetAttackPower` 佐证 c3 为独立栈） | `commands/12-csharp-stack-start3.txt` |
| 13 | `… -Side csharp … -AttackCount 120 … -Prefix m3cb1c3`（**c3**，同侧重跑） | `commands/13-csharp-120-c3.txt` |
| 14 | C# 栈停 + 端口实拍 | `commands/14-csharp-stack-stop.txt` |
| 15 | 收工端口实拍 | `commands/15-ports-after.txt` |
| 16 | 门禁 `cargo clippy --workspace --all-targets -- -D warnings`（隔离 `CARGO_TARGET_DIR=E:\tmp\m3cs-b1b2-target`） | `commands/16-gates-clippy.txt` |

## 3. 会话 ↔ perk 对应

| 会话 | 侧/前缀 | 击数 | summary（attacked/hits/dodges/sum） | 角色 |
|---|---|---|---|---|
| `sessions/csharp-20261012-155129` | csharp / `m3cb1sm` | 20 | 20/0/20/0 | C# 冒烟 |
| `sessions/csharp-20261012-155255` | csharp / `m3cb1c1` | 120 | 120/1/119/2 (`k=2 dmg=2`) | **c1**（跨侧基准） |
| `sessions/csharp-20261012-160205` | csharp / `m3cb1c2` | 120 | 120/8/112/14 | **c2**（同侧第 2 次） |
| `sessions/csharp-20261012-160446` | csharp / `m3cb1c3` | 120 | 120/1/119/2 (`k=2 dmg=2`) | **c3**（同侧第 3 次） |
| `sessions/rust-20261012-155537` | rust / `m3cb1rm` | 20 | 20/0/20/0 | Rust 冒烟 |
| `sessions/rust-20261012-155659` | rust / `m3cb1r1` | 120 | 120/0/120/0 | **r1**（跨侧） |
| `sessions/rust-20261012-155907` | rust / `m3cb1r2` | 120 | 120/0/120/0 | **r2**（同侧第 2 次） |

每个会话目录入库 5 件：`attack_raw.jsonl` / `attack_seq.jsonl` / `attack_seed.json` / `baseline.json` /
`bot.log.txt`（按 F-1 由 `.gitignore` 吞掉的 `bot.log` **改名**为 `.txt`）。

`perk/`（逐 k 对拍原始件）：

| 文件 | 对应会话对 | 结果 |
|---|---|---|
| `perk/c1-vs-c3-identical.txt` | c1(155255) ↔ c3(160446) | 逐 k **全等**（`attack_seq.jsonl` sha256 同为 `efac9876…`） |
| `perk/r1-vs-r2-identical.txt` | r1(155659) ↔ r2(155907) | 逐 k **全等**（sha256 `b2bf73e7…`） |
| `perk/c1-vs-r1-cross-side.txt` | c1(155255) ↔ r1(155659) | **k=2 不同**（C# hit / Rust dodge）+ 非平凡门 RED（Rust hits=0） |
| `perk/c2-vs-c3-differs.txt` | c2(160205) ↔ c3(160446) | 逐 k 不等（9 处）；夹具侧发现（PIN_RETRY 时序），**只登记不改夹具** |
| `perk/c1-vs-c2-same-side.txt` | c1(155255) ↔ c2(160205) | 同侧同种子逐 k 不等（9 处）⇒ 逐 k 非 seed+属性 的函数 |
| `perk/k2-localization.txt` | — | **k=2 定位结论**（属性装载缺口 / 复现命令 / 承接去向） |

## 4. 判据现状（**未达成，按 §6.0-3 登记**）

- 跨侧判据 `python mir2-rs/tests/parity/m3/attack_diff.py align … && … diff <c1> <r1>`：
  **`diff != 0`**（1 处：k=2）+ **非平凡门 RED**（Rust `hits=0`、`sum=0`）。原始输出在
  `commands/07-rust-120-crossside.txt` 与 `perk/c1-vs-r1-cross-side.txt`。
- DoD② 的真机同种子逐 k 部分 **未达成**；未降级口径（未改成集合/统计），未改 `attack_diff.py`。
- 同侧重跑性：`c1≡c3`、`r1≡r2` **成立**；但 `c2` 是**反例**（同代码/同种子/同 pin 属性，逐 k 不等）
  ⇒ R-1 的「同侧连跑两次逐 k 完全一致」**不是稳定前提**，需夹具层（流位置对齐）解决。

## 5. 夹具侧发现（**只登记，不改**；夹具属 m3bag 写集）

- c1/c3 各有 2/3 次 `PIN_RETRY`，c2 只有 1 次；但 `c1≡c3` ⇒ **PIN_RETRY 次数本身不移动 RNG 流**
  （与「多一次 PIN_RETRY ⇒ 流位置平移」的假设不符）——流位置差异另有来源（全局非 0137 图消费者 + 时序）。
- C# `rng-stream.tsv` 共 341,775 次抽数，攻击路径只占 ~120×2 次 `PlayObject.GetAttackPower`
  （+ ≤12 次 `_Attack` 命中门）；其余为 `AnimalObject.Wondering`（112,529）、
  `WorldServer.RegenMonsters`/`MonGetRandomItems`（数千）等**非 0137 图**的消费者。

## 6. 门禁

- `cargo clippy --workspace --all-targets -- -D warnings`：见 `commands/16-gates-clippy.txt`（rc=0）。
- head `dedef0d2` 提交信息记录：`fmt` 0 / `clippy` 0 / `test --workspace` 79 suites **656** passed / 0 failed /
  **604**（`msgcodegen --check`），隔离 `CARGO_TARGET_DIR` + `--offline`。reviewer 请在自己钉住的 commit 上重跑。

## 7. 残留物清单

- **已清**：工作树里的原始会话草稿目录 `mir2-rs/tests/parity/m3/sessions/{csharp,rust}-20261012-15*`
  （7 个）——入库前逐文件 `cmp` 与 `sessions/` 内 5 件**逐字节相同**后删除，只保留本证据包 `sessions/`。
- **保留（登记）**：`E:\tmp\m3cs-b1b2-target\`（cargo 隔离 target，热缓存，供 reviewer/后续门禁复用）；
  `E:\tmp\m3cs-b1b2\`（本 head 的 build/gate 日志 + WorldGolden 重基准期的扫描件）；
  `E:\tmp\m3cs-tick\csharp-stack\`（C# oracle 栈日志 + `rng-stream.tsv`，30MB 仓外输入，未入库）。
- **端口**：收工实拍 `commands/15-ports-after.txt` = 只余 mysqld:3306，5000/5100/5500/5600/5700/6000/7000/7100/7200 全 DOWN。
- **未动**：`src/**`（除已登记 `T-*`）、表结构/脚本语法/数据格式、`whitelist.md`、`run_pair.ps1`、`run_attack_pair.ps1`、`attack_diff.py`、`gate_guards.ps1`。
