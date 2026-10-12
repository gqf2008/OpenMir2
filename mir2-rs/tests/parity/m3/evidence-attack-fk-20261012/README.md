# M3②③ F-K 修复 + R-1 构造级隔离 · 真机取窗证据片（tick，2026-10-12 14:30 owner 裁决 `466cf58d` 执行）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- 分支：`worktree-m3-combat-skills`；base = `refs/heads/master`（本次 head 见 patch 条）
- Rust 二进制：`E:\tmp\m3cs-fk-target\debug`（**含 F-K 修复**，`cargo build -p game-svr` 由当前树产出）
- C# oracle 栈：`E:/tmp/m3cs-tick/start-csharp-hook.ps1 -Seed 42`（`M2GameSvr-hook-m3cs16` + `m3/rng-hook`）
- 触发：owner 14:30 裁决 —— ① F-K 修（crates/world 写集）② R-1 先做「零 RNG 消费者」场景隔离、判据不降级 ③ 再取窗跑两相。

> **本片结论**：F-K **已修**并有真身向量 + 生产路径回归 + 红检；R-1 的**构造级**（零 RNG 消费者）逐 k 跨侧
> **= 0**；但**真机两相仍非 0**（Rust 140 击 0 条 `SM_STRUCK`）⇒ DoD①② 的真机部分**未达成**，
> 剩余 blocker 是「活图全局 RNG 被非被测消费者消耗」（见 §4 与报告 §C）。**未自签 review、未 merge。**

---

## 1. 跑的命令（逐条原始件）

| # | 命令（要点） | 原始件 | rc |
|---|---|---|---|
| 0 | 端口实拍（起栈前） | `00-ports-before.txt` | — |
| 1 | `E:/tmp/m3cs-tick/start-csharp-hook.ps1 -Seed 42` | `01-csharp-stack-start.txt` | 0 |
| 2 | `run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3cfk1500 -Seed 42 -CsharpSeedLog E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv`（**冒烟**） | `02-smoke-csharp-20.txt` | 0 |
| 3 | `run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3cfk1502 -Seed 42 -CsharpSeedLog …` | `03-csharp-120.txt` | 0 |
| 4 | 停 C# 栈（`start-csharp-hook.ps1 -Stop`） | `04-csharp-stack-stop.txt` | 0 |
| 5 | `tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:\tmp\m3cs-fk-target\debug -Seed 42 -LogDir E:\tmp\m3cs-fk\rust-stack`（**F-K 修复后的二进制**） | `05-rust-stack-start.txt` | 0 |
| 6 | `run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3cfk1505 -PeerSession csharp-20261012-150100 -Seed 42 -RustStackLogDir E:\tmp\m3cs-fk\rust-stack` | `06-rust-120.txt` | **1** |
| 7 | `run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3cfk1507 -Seed 42 -RustStackLogDir …`（Rust 冒烟） | `07-rust-smoke-20.txt` | 0 |
| 8 | 停 Rust 栈 + 端口实拍（收工） | `08-rust-stack-stop.txt` / `09-ports-after.txt` | 0 |
| 9 | `run_attack_pair.ps1 -DiffOnly -CsharpSession csharp-20261012-150100 -RustSession rust-20261012-150343`（不碰端口重比） | `10-attack-diff-diffonly.txt` | **1** |

两侧会话原始件逐会话入库在 `sessions/`：`csharp-20261012-145931`（冒烟 20）、`csharp-20261012-150100`（120）、
`rust-20261012-150343`（120）、`rust-20261012-150901`（冒烟 20）。

## 2. 种子前提（F-C 自证，成立）

`11-seed-injection-both-sides.txt`：C# `INJECT_OK 42` + `SEED_PROBE 1434747710 … 1101493307`；
Rust `RUST_SEED_INJECTED=42` + game-svr 同一 `SEED_PROBE` 参照序列。⇒ 起始 RNG 流一致。

## 3. 结果（判据 = 0？**否**）

`14-hit-count-analysis.txt` 的原始计数：

| 侧运行 | raw roles | SM_STRUCK 条数 | align hits | hit k |
|---|---|---|---|---|
| C# smoke20 | arm 1 / attack 20 / **struck 2** / done 1 | 2 | 2 | 5,6 |
| C# 120 | arm 1 / attack 120 / **struck 2** / done 1 | 2 | 2 | 1,2 |
| Rust 120 | arm 1 / attack 120 / **struck 0** / done 1 | **0** | **0** | — |
| Rust smoke20 | arm 1 / attack 20 / **struck 0** / done 1 | **0** | **0** | — |

⇒ F-K 修复后 Rust 侧 120+20=140 击仍 0 条 `SM_STRUCK`：**非平凡门 RED + 跨侧逐 k diff RED**。

## 4. Findings

### F-K（**已修**）：`is_attack_target` 对人打人恒 false ⇒ 用 `is_proper_target` 忠实建模

- 旧根因原始件：`../evidence-attack-parity-window-20261012/12-rootcause-is-attack-target.txt`。
- 修复：`crates/world/src/combat.rs` 新增 `is_proper_target` / `is_proper_target_play` /
  `is_attack_target_player` / `is_attack_target_base` / `is_protect_target`；`entity.rs` 新增
  `attack_mode`（默认 0=HAM_ALL）/`in_safe_zone`/`admin_mode`/`stone_mode`/`dear_human`/`pk_point`。
- 真身向量：`../proper-target-probe`（直调 `PlayObject.IsProperTarget`）24 条边界，Rust
  `proper_target_matches_csharp_on_boundaries` 逐条全等；红检 `redcheck_ham_all_pvp_boundary_must_fail` /
  `redcheck_own_pet_boundary_must_fail`；生产回归 `world::tests::pvp_attack_lands_damage_in_world_path`。
- 另补一处真实取数缺口：`PlayObject.StruckDamage`（`PlayObject.cs:2954`）对玩家目标无条件多抽一次
  `Random(10)`，`combat.rs` 已照抄（否则构造级 PvP 逐 k 从第一次命中起错位）。

### R-1（真机仍成立）：活图全局 RNG 被非被测消费者消耗 ⇒ 同种子不保证逐 k 相同

- **构造级隔离已证明逐 k 可比**：`../proper-target-probe` 直接构造两个真 `PlayObject`（无地图/无怪 AI/无 socket），
  注入 seed=20261012 跑 100 次 `_Attack` ⇒ `pvp_ham_all_100` 的 `(power,damage,hp,hit)` 与 oracle **全等**；
  同侧连跑两次 `proper_target_vectors.{json,tsv}` **逐字节相同**（sha256 `323d101b…`）。
- **真机仍 0/140**：C# 两次命中在 k=5,6 / k=1,2（本片与上一片都不同）⇒ 逐 k 无仲裁力；Rust 恒 0 与 oracle 的
  低命中率（~1.7%）在 RNG 流分岔下不可比。⇒ owner 的 R-1 解法（场景除攻击路径外零 RNG 消费者）在**真机**
  上需要「无 MonGen 空图」等夹具级输入，属 `m3bag` 写集，本卡只能登记（见报告 §C）。

## 5. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs-fk-target`，工作目录 `mir2-rs/`）

- `15-gates-fmt.txt`：`cargo fmt --all -- --check` rc=0
- `16-gates-clippy.txt`：`cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `17-gates-test.txt`：`cargo test --workspace` **79 suites / 654 passed / 0 failed / 16 ignored**，rc=0
- `18-gates-msgcodegen.txt`：`cargo run -p msgcodegen -- --check` **604** 条一致，rc=0

## 6. 窗口与残留物

- 起栈前/收工端口实拍：`00-ports-before.txt` / `09-ports-after.txt`（九端口空，仅共享 3306）⇒ **窗口已释放**。
- 临时物：`E:/tmp/m3cs-fk/rust-stack/**`（本片 Rust 栈日志）、`E:/tmp/m3cs-fk-target/**`（隔离 target，
  含 F-K 二进制）、`E:/tmp/m3cs-tick/csharp-stack/**`（C# 栈日志，脚本复用）。
- 本仓库内新增 = 本目录 + `../proper-target-probe/**` + `crates/world` 修复；零 `src/**` / 零 `sql/**`。
