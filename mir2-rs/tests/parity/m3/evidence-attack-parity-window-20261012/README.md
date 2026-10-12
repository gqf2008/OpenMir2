# M3②③ 真机同种子攻击对拍 · 取窗运行证据片（tick27）

- 卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）／车道 `m3cs`／principal `openmir2-worker-5`
- base：`refs/heads/master` = `8a0fcd23195a835e176a7137a38d6479d29934ba`
- 夹具：`openmir2-m3-attack-pair-fixture` 的**已复核 head** `c04f3593`（reviewer-5 review `19f7ee3a` approve；
  base `8a0fcd23`。**该 head 尚未合并**，本片以 detached worktree `E:/tmp/m3csfix` 运行其仪器，未改夹具一字节）
- 触发：owner 13:50 的**取窗双条件**都成立——
  ① 四条机判全中（T-30 closed；`gate_guards.ps1:546` = `c.IncHealth=0, c.IncSpell=0`；`:442`
  `$script:M3AttackVictimHp = 30000`；`tools/botload/run_bots.ps1` 有 `Get-M3SeqDoneCount`/`M3ATTSEQ role=done` 收尾）；
  ② 夹具卡上出现 m3bag 的**自证连绿（两次）+ 窗口已释放**声明（entry `7fabf281`，ts 1791785198；
  「真机同配置同种子连跑 2 次均 attacked=20 hits=1 done=1/1、无第三方 struck；窗口已释放」）。
- 起栈前/收工后端口实拍（`00-ports-before.txt` / `09-ports-after.txt`）：九端口全空，仅共享 3306 ⇒ 窗口可用、收工已释放。

> **本片结论：DoD①② 未达成，且被判据钉在 Rust 侧的 `SM_STRUCK` 一条都没发出。** 本片是**真机原始件 + 根因**
> 的可审入库（零 `.rs`、零仪器改动），不是「已达等价」的声明。

---

## 1. 跑的命令（逐条原始件）

| # | 命令（要点） | 原始件 | rc |
|---|---|---|---|
| 0 | 端口实拍（起栈前） | `00-ports-before.txt` | — |
| 1 | `E:/tmp/m3cs-tick/start-csharp-hook.ps1 -Seed 42`（C# 影子栈 + `m3/rng-hook`） | `01-csharp-stack-start.txt` | 0 |
| 2 | `run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3csm1416 -Seed 42`（**冒烟**） | `02-smoke-csharp-20.txt` | 0 |
| 3 | `run_attack_pair.ps1 -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3csm1418 -Seed 42` | `03-csharp-120.txt` | 0 |
| 4 | 停 C# 栈 | `04-csharp-stack-stop.txt` | — |
| 5 | `start_rust_stack.ps1 -RustBinDir E:\tmp\openmir2-m2svr-target\debug -Seed 42 -LogDir E:\tmp\m3cs-tick\rust-stack` | `05-rust-stack-start.txt` | 0 |
| 6 | `run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3csm1419 -PeerSession csharp-20261012-141650 -Seed 42` | `06-rust-120.txt` | **1** |
| 7 | `run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3csm1425 -Seed 42`（Rust 冒烟） | `07-rust-smoke-20.txt` | 0 |
| 8 | 停 Rust 栈 + 端口实拍（收工） | `08-rust-stack-stop.txt` / `09-ports-after.txt` | 0 |
| 9 | `run_attack_pair.ps1 -DiffOnly -CsharpSession csharp-20261012-141650 -RustSession rust-20261012-141909`（不碰端口重比） | `10-attack-diff-diffonly.txt` | **1** |

两侧会话原始件（`attack_raw.jsonl` / `attack_seq.jsonl` / `attack_seed.json` / `baseline.json` / `bot.log`）
逐会话入库在 `sessions/`：`csharp-20261012-141542`（冒烟 20）、`csharp-20261012-141650`（120）、
`rust-20261012-141909`（120）、`rust-20261012-142253`（冒烟 20）。

## 2. 种子前提（F-C 自证，成立）

`11-seed-injection-both-sides.txt`：C# `INJECT_OK 42` + `SEED_PROBE 1434747710 … 1101493307`；
Rust 启动日志同一行参照序列（启动器 `RUST_SEED_INJECTED=42`）。
⇒ **起始 RNG 流一致**，两侧**不是**「没设种子的不可比红」。

## 3. 结果（判据 = 0？**否**）

- **冒烟（C# 20）**：`02-smoke-csharp-20.txt` ⇒ `ATTACKSEQ_OK markers=26`、
  `ALIGN_OK … attacked=20 hits=2 dodges=18 sum=4`、`GREEN: 侧运行与同序列判据通过`。
- **C# 120**：`03-csharp-120.txt` ⇒ `ALIGN_OK … attacked=120 hits=2 dodges=118 sum=2`；侧运行 rc=0。
- **Rust 120**：`06-rust-120.txt` ⇒ `ATTACKSEQ_OK markers=122` + `ATTACK_POSE_OK`（与 peer 逐字节同 pose）+
  `ALIGN_OK … attacked=120 hits=0 dodges=120 sum=0`，随后
  `NONTRIVIAL b=False(hits=0（一次都没命中；无法区分实现与空序列）)` ⇒
  **`RED: 非平凡门未过`** + **`RED: 跨侧攻击序列 diff != 0`**。
- **Rust 冒烟 20**：`07-rust-smoke-20.txt` ⇒ `attacked=20 hits=0 dodges=20 sum=0`。
- **DiffOnly 复比**：`10-attack-diff-diffonly.txt` 与第 6 步同结论（不碰端口也可复现）。

`14-hit-count-analysis.txt` 的原始计数：

| 侧运行 | raw roles | SM_STRUCK 条数 | align hits | hit k |
|---|---|---|---|---|
| C# smoke20 | arm 1 / attack 20 / **struck 4** / done 1 | 4 | 2 | 2,3 |
| C# 120 | arm 1 / attack 120 / **struck 2** / done 1 | 2 | 2 | 2,3 |
| Rust 120 | arm 1 / attack 120 / **struck 0** / done 1 | **0** | **0** | — |
| Rust smoke20 | arm 1 / attack 20 / **struck 0** / done 1 | **0** | **0** | — |

⇒ 同一夹具、同一 pin `(429,320)`、同一 seed=42：C# 侧共 4 次受击（k=2,3），
**Rust 侧 120+20=140 击一次 `SM_STRUCK` 都没发出**。

## 4. Findings

### F-K（Important，**阻塞 DoD②**）：Rust 侧玩家打玩家被 `is_attack_target` 结构性归零 ⇒ 恒无 `SM_STRUCK`

- 位置：`mir2-rs/crates/world/src/combat.rs:355-372`（`is_attack_target`）+ `:515-522`（`else { n_power = 0 }`）
  + `mir2-rs/bins/game-svr/src/main.rs:2157-2159`（`if outcome.damage == 0 { continue; }`）。
- 事实：`is_attack_target` 只放行「人打怪 / 怪打人」，**人打人两个分支都不命中 ⇒ 末尾 `return false`**
  （`12-rootcause-is-attack-target.txt`）。攻击结算据此把 `n_power` 归零；`drain_combat_downlink` 见
  `damage==0` 直接跳过受击帧 ⇒ 玩家目标一条 `SM_STRUCK` 都不发。真机 raw 坐实 `struck=0`（见 §3）。
- oracle 依据（`13-oracle-isproper-target.txt`）：`BaseObject.Attack.cs:37` 的命中门是 **`IsProperTarget`**（虚函数），
  不是固定 race 规则；`PlayObject.cs:908` 新角色 `AttatckMode = 0` = `AttackMode.HAM_ALL`（`src/OpenMir2/Enums/AttackMode.cs`），
  `PlayObject.Attack.cs:627-641` 的 `HAM_ALL` 支对同为 Play 的目标 `result = true` ⇒ 允许命中。
  真机 C# 侧实测确实打出 4 次 `SM_STRUCK` ⇒ oracle 允许。
- 与接线登记的张力：`bins/game-svr/src/attackwire.rs` 模块头**已登记**「目标筛选沿用 `GetMovingObject` 的
  `!Ghost && !Death`，**不**做安全区/攻击模式过滤」——即接线层决定「不做 `IsProperTarget` 过滤」；
  但 `combat::attack` 内部又用 `is_attack_target` 把玩家目标归零，**两层口径互相矛盾**，效果是 PvP 永不命中。
- 为什么本片**不自行修**：忠实复刻需要把 `IsProperTarget`（攻击模式 / `IsProtectTarget` / 安全区 / 主人-召唤者）
  建模，属语义扩展，不是一行改；凭「让夹具变绿」的最小改法（比如删掉人打人分支）会在别处引入新分歧。
  按仓库纪律（`docs/M2验收真机报告-20261012-4.md`：「白名单条目要 owner 签字」）⇒ **交 owner 裁决**。

### R-1（Residual，已在夹具卡 review `19f7ee3a` 提出，本片复现）

同一种子**不保证逐 k 相同**：C# 两次运行命中的 k 位不同（夹具卡记录 hit@k=2 vs @k=4；本片两次 C# 都落在 k=2,3，
但 R-1 的「更强前提不成立」成立——攻击判定取全局 `M2Share.RandomNumber`，被全图怪物 AI 等持续消耗）。
⇒ 即便 F-K 修好，`attack_diff.py diff` 的**逐 k** 比对仍无仲裁力，需要 owner 对 DoD①② 口径裁决
（例如改为集合/统计口径，或给夹具做 RNG 隔离）。

## 5. 登记去向（本片不自行登记白名单）

- 判据 `RED`（非平凡门 + 跨侧 diff），差异**未修到 0**，也**未**自签 whitelist——按仓库纪律，
  白名单条目要 **owner 签字**；本片只提 finding + 真机原件，请 owner 决定：
  ① 另开卡修 `IsProperTarget`（属 `crates/world` + 战斗接线写集）；② 或裁决 DoD①② 口径为统计式；
  ③ 或裁定夹具需 RNG 隔离。
- 本片**零 `.rs` 改动**（`git diff --name-only master <head>` 非证据件 = 0），故不改变任何生产行为。

## 6. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/m3cs12-target`，工作目录 `mir2-rs/`）

- `15-gates-fmt.txt`：`cargo fmt --all -- --check` rc=0
- `16-gates-clippy.txt`：`cargo clippy --workspace --all-targets -- -D warnings` rc=0
- `17-gates-test.txt`：`cargo test --workspace` **79 suites / 648 passed / 0 failed / 16 ignored**，rc=0
- `18-gates-msgcodegen.txt`：`cargo run -p msgcodegen -- --check` **604** 条一致，rc=0

## 7. 窗口与残留物

- 起栈前/收工端口实拍：`00-ports-before.txt`（九端口空）/ `09-ports-after.txt`（九端口空，仅 3306）⇒
  **窗口已释放**；本片未与其它车道并行起栈。
- 临时物：`E:/tmp/m3cs-tick/**`（本片原始 stdout/日志）、`E:/tmp/m3csfix`（detached 夹具 worktree，跑完已 `git worktree remove`）；
  `E:/tmp/m3cs16-evidence/csharp-stack/{csharp-stack.pid.json,rng-stream.tsv,GameSvr.out.log}` 被本次 C# 栈复用/覆盖（脚本按 m3cs16 的 LogDir 起停，已在 `01`/`04` 留痕）。
- 本仓库内新增 = 本目录（`evidence-attack-parity-window-20261012/**`），零 `.rs` / 零 `.ps1` / 零 `src|sql|MirClient`。
