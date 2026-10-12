# 卡 `openmir2-m3-rust-seed-injection` 证据（T-30：Rust game-svr 同种子注入）

**issue**：`02e67b0e`（`walgit collab thread openmir2-m3-rust-seed-injection`）
**patch**：`patch --base refs/heads/master --head refs/heads/worktree-m2-gamesvr-host`（head 提交记录在 walgit patch 条目里）
**base**：`refs/heads/master`，交付时已 merge 到 **`9456ed6f`**（M3 ②③ 第十一片合入后；master 已含周期存档 `e4a18ca8`）
**worktree**：`E:\Users\gxh\Documents\GitHub\OpenMir2\.claude\worktrees\m2-svr`（分支 `worktree-m2-gamesvr-host`）
**写集**：`mir2-rs/bins/game-svr/**` + 本证据目录（未动 `crates/**`、`src/**`、`MirClient`、表结构/脚本/数据格式、`mir2-rs/tests/parity/m3/**` 的夹具脚本）

## 0. 为什么是这条（T-30）

`openmir2-m3-attack-pair-fixture` 收卡时登记：C# 侧真机跑可用既有 `m3/rng-hook` 影子注入
`MIR2_RNG_SEED`，而 Rust `bins/game-svr` 的生产路径**不读任何种子环境变数**（`World::with_maps`
里写死 `RandomNumber::with_seed(42)`）⇒ 真机两侧的 `SM_STRUCK` 伤害数值**不可比** ⇒ M3②③ 的
DoD①②（固定种子 100 次攻击伤害序列逐项相同）跑不出结论。本片把 Rust 生产路径的注入口补上。

## 1. oracle 侧注入点（只读核对，原始件 `oracle-injection-point.txt`）

```text
变数名           MIR2_RNG_SEED                        rng-hook/StartupHook.cs:41   Environment.GetEnvironmentVariable
空/未设           string.IsNullOrEmpty(seedText) ⇒ return（不启用）  StartupHook.cs:42-44
格式             int.TryParse(seedText, out int seed)（NumberStyles.Integer：前后空白 + 前导正负号、十进制）
                                                      StartupHook.cs:46
失败处置         Fail(...) ⇒ Environment.Exit(97)      StartupHook.cs:48 / 158-166（**不许静默降级**）
注入时机         Initialize()（进程启动、任何游戏取数之前）换成 `new Random(seed)`  StartupHook.cs:64-104
自证             `new Random(seed).Next()` 取 8 项写 `SEED_PROBE` + `MIR2_RNG_LOG`  StartupHook.cs:88-99
实测参照序列     SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
                 RNG同种子与记录回放方案.md:50-53（MIR2_RNG_SEED=42）
```

## 2. Rust 侧实现（写集 = `mir2-rs/bins/game-svr/**`）

| 文件 | 内容 |
| --- | --- |
| `src/seed.rs`（新增） | `RNG_SEED_ENV` / `parse_seed` / `env_seed` / `probe_values`（测试用）。读取口径**逐条对齐**上表：`None` 或 `""` ⇒ `Ok(None)`（不启用）；`text.trim().parse::<i32>()` ⇒ `Ok(Some(seed))`；其余（含溢出、十六进制、小数、空白串）⇒ `SeedError`（对应 C# `Fail`）。 |
| `src/main.rs` | `main()` 在 `config::load` 之后立即 `let rng_seed = seed::env_seed()?;`（**fail-closed**：非法值 ⇒ `Err` ⇒ 进程非零退出，且在任何监听/线程之前）；经 `EnterConfig.rng_seed` 传进 `sim_loop`。`sim_loop` 里 `World::with_maps(...)` 之后**仅当** `Some(seed)` 时 `world.rng = mir2_shared::rng::RandomNumber::with_seed(seed)`，并打一条 `SEED_PROBE` 自证行；`None` ⇒ **什么都不做**。 |
| `tests/seed_inject.rs`（新增） | 3 条进程级用例（见 §3）。 |

**关键口径**：

- **默认行为一字不改**：未设变数 ⇒ `rng_seed = None` ⇒ 不进 `if`、不加日志 ⇒ `crates/world` 的既有随机源
  （`seed 42`）与现状**逐字节相同**。**没有**把「总是用某默认种子」写进生产路径（默认种子来自
  `crates/world`，本片不碰）。
- **不改 `crates/shared`**：`RandomNumber::with_seed` 已存在（`crates/shared/src/rng.rs`），直接复用；
  **不在 game-svr 里自造随机源**。
- **自证行取注入后世界随机源的克隆**：`let mut probe_rng = world.rng.clone();` 取 8 项 —— 比 C# 钩子的
  独立 `reference` 实例更强：它直接证明**被替换的游戏随机源**就是该序列（红检 B 钉住这一点，见 §3），
  且克隆取数**不消耗**游戏流。

## 3. 用例与红检（DoD ④）

**用例（`src/seed.rs` 单元 7 条）**：

| 用例 | 判据 |
| --- | --- |
| `parse_seed_unset_or_empty_is_disabled` | `None` / `""` ⇒ `Ok(None)`（对应 `IsNullOrEmpty` 提前 return） |
| `parse_seed_accepts_decimal_with_sign_and_whitespace` | `42`/` 42 `/`+7`/`-7`/`0`/`i32::MAX`/`i32::MIN` 均按 `int.TryParse` 语义解析 |
| `parse_seed_rejects_non_integer` | `abc`/`0x10`/`4.2`/溢出 `2147483648`/空白串 `"   "` ⇒ 全 `Err`（fail-closed） |
| `probe_matches_csharp_seed42` | Rust `with_seed(42)` 的 8 项 = C# 实测 `SEED_PROBE`（逐项相同） |
| `probe_is_deterministic_and_seed_sensitive` | 同种子可复现；不同种子必须不同（防「忽略种子」假绿） |
| `injected_world_rng_matches_csharp_probe` | 世界随机源被注入后，**从该实例**取出的序列 = C# `SEED_PROBE` |
| `unset_seed_keeps_existing_world_rng` | 未注入 ⇒ 世界随机源仍是既有 `seed 42`（与现状一致） |

**进程级用例（`tests/seed_inject.rs` 3 条）**：起真 `game-svr` 二进制 + 最小合法 `Server.conf`。

| 用例 | 判据 |
| --- | --- |
| `seed_env_injection_logs_csharp_probe` | 设 `MIR2_RNG_SEED=42` ⇒ 日志出现 `RNG 同种子注入：seed=42` 且 `SEED_PROBE` 8 项 = C# 实测 |
| `unset_seed_env_is_inert` | 不设 ⇒ 日志**没有**注入行（生产路径一字不改） |
| `invalid_seed_env_exits_nonzero` | 设 `not-an-int` ⇒ 进程**非零退出**且点名 `MIR2_RNG_SEED` |

**红检（改坏 ⇒ 必红；每步跑完立即 `git checkout` 还原）**：

| 文件 | 改法 | 原始输出 |
| --- | --- | --- |
| `red-ignore-env.txt` | `main.rs` 的 `let rng_seed = seed::env_seed()?;` → `let rng_seed: Option<i32> = None;`（**忽略环境变数**） | `cargo test … seed_env_injection_logs_csharp_probe`：exit 101，`FAILED`（日志无 `SEED_PROBE`），21.21s 超时后断言红 |
| `red-hardcoded-seed.txt` | `main.rs` 的 `world.rng = …with_seed(seed)` → `with_seed(7)`（**把种子常量写死**） | exit 101，`FAILED`；日志 `seed=42 … SEED_PROBE 822959691 1871007331 …`（**种子 7 的序列**）⇒ 证明自证行取自被注入的世界实例，写死常量跑不过 |
| `green-after-red.txt` | 两处改坏全部还原后复跑 `seed_inject` | `3 passed; 0 failed` |

## 4. 门禁（DoD ⑤，隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`，全 `--offline`）

**base = master `9456ed6f`（merge 后）**：

| 门禁 | 命令 | 结果 |
| --- | --- | --- |
| fmt | `cargo fmt --all -- --check` | rc=0（`gates-fmt.txt`，0 字节 = 无 diff） |
| clippy | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0（`gates-clippy.txt`） |
| test | `cargo test --workspace --offline` | **79 suites / 648 passed / 0 failed / 16 ignored**（`gates-test-workspace.txt`） |
| msgcodegen | `cargo run -p msgcodegen --offline -- --check` | rc=0，604 条（`gates-msgcodegen.txt`） |

新增/改动用例 **3 连跑**：`gamesvr-3run-1..3.txt`，每次 `242 passed / 0 failed`（bin unit 146 含本片 7 条 +
`seed_inject` 3 条 + `relay` 83 + 库用例；**同一命令三次同结论**）。

## 5. 真机 / 端到端——**本片不起栈、不占端口**（卡上明示判据在构造层）

- 开工实拍（`ports-preflight.txt`）：5000/5100/5500/5600/5700/6000/7000/7100/7200 **全 free**；
  3306 = 共享 MySQL（pid 26652）。本片**未起栈** ⇒ 无 `-Stop`，收工端口与开工一致（窗口未被本线占用）。
- 真机对拍（`run_attack_pair.ps1` 两侧同 `MIR2_RNG_SEED`）由 ②③ 在 owner 发的窗口里做，不在本片；
  本片交付的是它**能跑出结论**的前置（Rust 侧同种子注入）。

## 6. 边界与残留物

- 未动 `src/**`（C# oracle）、`crates/**`、`crates/storage`、`MirClient`、表结构/脚本语法/数据格式；
  未动 `mir2-rs/tests/parity/m3/**` 的夹具脚本（m3bag 写集）；未动别人的卡状态。
- 证据入库路径：`mir2-rs/tests/parity/m3/evidence/rust-seed-injection-20261012/`；
  `SHA256SUMS.txt` 按仓库口径（LF 归一 = Git blob 字节）记，`tools/evidence-hash/verify_sha256sums.py --check-extra`
  复核 GREEN，并在**干净 clone** 上复跑 GREEN（原始件 `clean-clone-verify.txt`）。
- 残留物：无（红检两处改坏均已还原；`git status` 仅本片源码 + 证据目录；未起栈/未起客户端）。
