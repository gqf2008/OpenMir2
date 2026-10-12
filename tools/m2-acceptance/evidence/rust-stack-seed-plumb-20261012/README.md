# `rust-stack-seed-plumb-20261012` — `start_rust_stack.ps1` 种子管线证据

卡：**`openmir2-m3-rust-stack-seed-plumb`**（issue `09841324`）——②③ 真机同种子攻击对拍的 **operability 前置（Rust 半）**。

## 0. 一句话

`tools/m2-acceptance/start_rust_stack.ps1` 过去用 `Start-Process` 起 `game-svr.exe`、**只继承调用方环境**
（`grep -i seed` 零命中）⇒ 只有 operator 手工在起栈的 shell 里设过 `MIR2_RNG_SEED` 才同种子，否则
C# 半注入了、Rust 半没注入（T-30 是 fail-soft）⇒ 得到**不可比的伤害序列**且看不出原因。本片加
`-Seed <int>`（或回落读 `MIR2_RNG_SEED`），起 game-svr 时把该变量**注入子进程环境**，并打印一行
可判据的 `RUST_SEED_INJECTED=<n>`；不传种子时**不注入、不打该行**。

## 1. 改动面（边界 = `tools/m2-acceptance/**`，未动 `mir2-rs/**`）

| 文件 | 改动 |
| --- | --- |
| `tools/m2-acceptance/rust_seed_plumb.ps1` | **新增**：`Resolve-Mir2Seed` / `Get-SeedInjectedLine` / `Start-SeededProcess`（与判据脚本共用同一份） |
| `tools/m2-acceptance/start_rust_stack.ps1` | 加 `-Seed`；点源管线；`Start-One` 走 `Start-SeededProcess`；game-svr 传 `-Seed $seedInfo`；非法种子 fail-closed；打 `RUST_SEED_INJECTED=<n>` |
| `tools/m2-acceptance/check_seed_plumb.ps1` | **新增**：判红/判绿自检（15 条，不起栈、不占真端口） |
| `tools/m2-acceptance/README.md` | 新增 §0b.1（`-Seed` 用法与退出码） |

## 2. 判据（原始件）

### 2.1 构造性判据 GREEN — `criterion-green.txt`

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/check_seed_plumb.ps1 `
  -GameSvrExe E:\tmp\openmir2-m2svr-target\debug\game-svr.exe
# => SUMMARY total=15 ok=15 fail=0 seeded_line=RUST_SEED_INJECTED=42 / SEED_PLUMB GREEN (rc=0)
```

15 条覆盖：`Resolve-Mir2Seed`（未设/`42`/` +7 `/环境回落/**非法 ⇒ 抛错**）、`Start-SeededProcess`
注入到位（`cmd /c echo %MIR2_RNG_SEED%` ⇒ `42`）与起完还原、**真 game-svr** 传种子 ⇒ `SEED_PROBE` 与
C# 逐项一致（`SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307`）、
不传种子 ⇒ 无 `SEED_PROBE`、`Get-SeedInjectedLine`、以及 `start_rust_stack.ps1` 的结构接线。

### 2.2 改坏必红 — `criterion-red-injection-removed.txt`

把 `rust_seed_plumb.ps1` 的注入行 `if ($inject) { $env:MIR2_RNG_SEED = [string]$Seed.Text }`
改成 `if ($false -and $inject) { ... }`（删掉注入）后重跑同一判据：

```
SUMMARY total=15 ok=11 fail=4 seeded_line=RUST_SEED_INJECTED=42
SEED_PLUMB FAIL (rc=1)
  [FAIL] ②a 传种子 ⇒ 子进程环境 MIR2_RNG_SEED=42
  [FAIL] ③a 传种子 ⇒ 真 game-svr 打出 SEED_PROBE
  [FAIL] ③b SEED_PROBE 与 C# 同种子逐项一致
  [FAIL] ③c 有同种子注入行（seed=42）
```

（`⑤a Get-SeedInjectedLine` 仍绿是**预期的**：它只测「该不该打这行」的纯逻辑，与「注入到没到」正交；
真正会红的是 ②a/③a/③b/③c 这四条端到端口径。）改动已还原，还原后判据复绿（见 §2.1）。

### 2.3 实栈端到端（`-Seed 42`）— `stack-seeded-script-stdout.txt` / `stack-seeded-gamesvr-out.txt`

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1 -Seed 42
# 脚本 stdout: SEED: 将向 game-svr 注入 MIR2_RNG_SEED=42 … START game-svr pid=… … RUST_SEED_INJECTED=42 … RUST_STACK_OK
# game-svr.out.log: RNG 同种子注入：seed=42 参照序列=SEED_PROBE 1434747710 302596119 269548474 1122627734 361709742 563913476 1555655117 1101493307
```

⇒ 「脚本打的行」与「game-svr 自证的 `SEED_PROBE`」**对得上**，且与 C# 钩子同种子序列**逐项一致**。

### 2.4 不传种子 ⇒ 零行为差 — `stack-noseed-script-stdout.txt` / `stack-noseed-gamesvr-out.txt`

```
powershell -NoProfile -ExecutionPolicy Bypass -File tools/m2-acceptance/start_rust_stack.ps1
# 脚本 stdout: SEED: 未指定种子（不注入，与改动前行为一致） … START game-svr … RUST_STACK_OK
# grep -c RUST_SEED_INJECTED script-stdout  = 0
# grep -c SEED_PROBE        game-svr.out.log = 0
```

### 2.5 非法种子 fail-closed — `stack-invalid-seed.txt`

```
start_rust_stack.ps1 -Seed abc                 ⇒ rc=1, RED: MIR2_RNG_SEED/-Seed 不是合法 int: abc —— 拒绝在非法种子下起栈（fail-closed）
MIR2_RNG_SEED=not-an-int start_rust_stack.ps1  ⇒ rc=1, RED: MIR2_RNG_SEED/-Seed 不是合法 int: not-an-int —— 拒绝在非法种子下起栈（fail-closed）
```

两条都**没有起栈**（端口保持 FREE；见 `ports-after-stop.txt`）。

## 3. 四门禁（隔离 `CARGO_TARGET_DIR=E:\tmp\openmir2-m2svr-target`）

| 门禁 | 结果 | 原始件 |
| --- | --- | --- |
| `cargo fmt --all -- --check` | rc=0 | `gates-fmt.txt` |
| `cargo clippy --workspace --all-targets -- -D warnings` | rc=0 | `gates-clippy.txt` |
| `cargo test --workspace --offline` | rc=0，**79 suites / 648 passed / 0 failed / 16 ignored** | `gates-test-workspace.txt` |
| `cargo run -p msgcodegen -- --check` | rc=0，604 条常量 | `gates-msgcodegen.txt` |

## 4. 窗口 / 进程卫生

- `ports-after-stop.txt`：起栈取证后已 `-Stop`，5000/5100/5500/5600/5700/6000/7000/7100/7200 **全 FREE**
  （仅余共享 MySQL `3306`）。
- 未起客户端、未占用真客户端口做长时间跑；两次实栈起停均用脚本自带 pid 文件收干净。

## 5. 残留物 / 已知边界

- 管线只覆盖 `start_rust_stack.ps1` 起的 **Rust** game-svr；C# 半的 `MIR2_RNG_SEED` 由 `m3bag` 的夹具
  `run_attack_pair.ps1` 负责（`INJECT_OK 42`，同卡族 `openmir2-m3-attack-pair-fixture`）。
- `run_acceptance.ps1` **不自己起栈**，故无需透传种子（其入口只对「已跑着的栈/已采好的产物」取判据）。
- 本片不改 `bins/game-svr` 的种子口径（那是 T-30，已合并 `7dd42245`）；只补启动器管线。

## 6. hash 口径

`SHA256SUMS.txt` 一律按**仓库字节**（`SHA-256(Git blob 内容)`，即 CRLF→LF 归一后）记；
复核：`python -I tools/evidence-hash/verify_sha256sums.py verify tools/m2-acceptance/evidence/rust-stack-seed-plumb-20261012 --check-extra`。
