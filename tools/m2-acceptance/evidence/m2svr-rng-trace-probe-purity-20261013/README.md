# Rust `MIR2_RNG_LOG` 逐次流水纯度：`SEED_PROBE` 自证抽数不得混进账本

- 卡：`openmir2-m2-rng-trace-seed-probe-purity`（issue，owner 侧归档可指到 Claim B
  `openmir2-rng-consumer-structure-parity` 的子项）
- 车道：`m2svr`（`openmir2-worker-11`）／worktree `m2-svr`／分支 `worktree-m2-rngtrace-purity`
  （base = `refs/heads/master` @ 交付时 tip，见 patch 条）
- 判据：`tools/m2-acceptance/check_rng_trace_purity.ps1`（8 项），**改坏必红**见本 README §3
- 边界：仅 `mir2-rs/bins/game-svr/src/main.rs`（启动期自证行）+ `tools/m2-acceptance/**`（新判据脚本与本证据目录）；
  `crates/**`、`src/`、`sql/`、`MirClient` 零改动

## 0. 缺陷（真机账本实测暴露）

Claim B（全服 RNG 消费者结构一致性）的判据是「两侧 `site` 集合 + 逐段次序 diff → 0」。m3cs 交付的两侧
账本片（`mir2-rs/tests/parity/m3/evidence-rng-ledger-diff-20261012/`）里：

- `32-diff-full.txt`：**全流第一处分歧 = index 0**：
  `a=M2Server.Actor.BaseObject..ctor@BaseObject.cs:438`（C# 首个真实消费者）
  vs `b=bins/game-svr/src/main.rs:2856`（Rust 首个记录）；
- `ONLY_IN_B(extra_in_B)` 里 Rust 独有站点 `bins/game-svr/src/main.rs:2856` **× 8**。

那 8 条**不是**游戏消费者：`sim_loop` 在注入 `MIR2_RNG_SEED` 后要打一条 `SEED_PROBE` 自证行，
原实现取「注入后世界随机源的克隆」前 8 项 —— 途径 [`mir2_shared::rng::RandomNumber::random`]，
而 `crates/shared/src/rng_trace.rs` 正是把 trace 挂在 `RandomNumber` 的每个取数入口上
（`rng_trace.rs` 模块头：「落点：全局随机源本身……不在 `crates/world` 各调用点手工打点」）
⇒ 复制出来的 8 次自证取数也被记进流水。

C# 钩子（`rng-hook/StartupHook.cs`）与 `rng_trace::enable` 写流水头部时都用**独立实例**
（`new Random(seed).Next()`），**不进**逐次流水 —— Rust 的生产自证行与它自己的 trace 头部口径不一致，
这就是 index 0 分歧与那个 Rust 独有站点的来源。

## 1. 复现（先复现再改）

命令见 `00-commands.txt`。最短复现：临时 `Server.conf` + 空闲端口，起 `game-svr.exe` 并注入
`MIR2_RNG_SEED=42`、`MIR2_RNG_LOG=<tmp>/rng-stream.tsv`，几秒后杀进程看流水头部：

`01-repro-before-fix-tsv.txt`（改前）：头部两行后紧跟 8 条 `Next()`，站点全是
`bins\game-svr\src\main.rs:2856`，值 = 头部 `SEED_PROBE` 的 8 项。

## 2. 修法（行为等价优先）

把自证参照序列改为**独立实例且不经 `RandomNumber`**（新增 `fn seed_probe_text(seed)`：
`DotNetRandom::new(seed).next_value()` 取 8 项）：

- 与 C# 钩子、`rng_trace::enable` 流水头部**同口径**（都是独立实例）；
- **不消耗**游戏流（本来就是克隆/独立实例，行为不变）；
- **不进**逐次流水 ⇒ 去掉 Rust 独有站点；
- stdout 的 `SEED_PROBE` 行**逐字节不变**（`RandomNumber` 的内层就是 `DotNetRandom`，序列相同；
  单测 `seed_probe_text_matches_csharp_seed42_and_random_number` 把等价关系锁死）。
- `world.rng = RandomNumber::with_seed(seed)` 注入点**一字未动**（T-30 的生产路径语义不变）。

## 3. 判据与牙齿

`check_rng_trace_purity.ps1` 8 项（不起完整栈、不占真端口；临时目录 + 空闲端口起单个 game-svr）：

| # | 口径 | GREEN | 改坏必红 |
|---|---|---|---|
| ①a/b/c | 传种子+路径 ⇒ 建流水，头部 = `INJECT_OK 42 0` + `SEED_PROBE <8 项>` | OK | — |
| ② | 头部 `SEED_PROBE` = C# 钩子 seed=42 实测序列 | OK | 改种子/算法即红 |
| ③ | **记录行（`^<seq>\tNext`）不含 `bins\game-svr\src\main.rs` 站点** | `records=0 main_records=0` | **改回经 `RandomNumber` ⇒ `main_records=8` FAIL** |
| ③b | 门的**前提**：`SEED_PROBE` 自证确实发生（stdout 有该行） | OK | 自证行没打 ⇒ 红 |
| ④ | stdout 的 `SEED_PROBE` 与流水头部逐项相同（两处独立实例一致） | OK | — |
| ⑤a | 只传种子、不传路径 ⇒ 正常起、不建流水（与改动前一致） | OK | — |

- **GREEN**：`02-green-check.txt`（`SUMMARY total=8 ok=8 fail=0` / `RNG_TRACE_PURITY GREEN`，rc=0），
  流水 `03-green-tsv.txt` 只有两行头。
- **改坏必红**：把 `seed_probe_text` 改回 `RandomNumber::with_seed(seed)` + `.random()`
  ⇒ `04-red-check.txt`（`records=8 main_records=8 first=[1 Next() 0 1434747710 bins\game-svr\src\main.rs:2892]`、
  `RNG_TRACE_PURITY FAIL`，rc=1），流水 `05-red-tsv.txt`；还原后复跑 `06-green-after-restore.txt` GREEN。
  （红件里的 8 条记录同时是「trace 通道确实在工作」的正向对照。）
- **构造层**：cargo 单测 `seed_probe_text_matches_csharp_seed42_and_random_number`（1 passed）。

## 4. 四门禁（隔离 `CARGO_TARGET_DIR=E:/tmp/openmir2-m2svr-target`、`--offline`）

`07`-`10`：fmt rc=0；clippy `--workspace --all-targets -D warnings` rc=0；
`cargo test --workspace` **84 suites / 687 passed / 0 failed / 16 ignored**；msgcodegen `--check` 604 条一致。

## 4b. 干净检出复核

`git worktree add --detach E:/tmp/m2svr-rngtrace-clean <head>` ⇒
`python -I tools/evidence-hash/verify_sha256sums.py --check-extra tools/m2-acceptance/evidence/m2svr-rng-trace-probe-purity-20261013`
= `GREEN [repo 口径] OK 15/15`（`13-clean-checkout-verify.txt`）。

## 5. 残留物清单

- 仓外临时物：`E:/tmp/m2svr-rngtrace-purity-{green,red,restored}/`、`E:/tmp/m2svr-rngtrace-*.out`、
  `E:/tmp/m2svr-probe-repro/`（本目录 `01`/`03`/`05` 已把关键原始件入库）。
- 本片未起完整栈、未占真端口（判据只用临时目录 + 空闲端口）。
- 未 merge、未自签 review；未改 `crates/**`、`src/`、`sql/`、`MirClient`。
