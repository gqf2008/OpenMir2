# M3②③ 攻击夹具 · pin 重试账本证据包（2026-10-12，m3bag / worker-4）

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。
来源：owner 2026-10-12 16:30 brief「新活：把「pin 重试次数」纳入夹具判据」。
**零 `.rs`**、`src/**` / `sql/**` / `MirClient` 改动 = 0、**不起栈/不占端口**（纯离线：既有 `sessions/`
与 m3cs 已入库的 stdout 原始件）。
写集 = `mir2-rs/tests/parity/m3/{gate_guards,run_pair,run_attack_pair,selftest_gate}.ps1` +
`mir2-rs/tests/parity/m3/attack_diff.py` + 本目录。

## 1. 问题（owner 定性 + 本片复算坐实）

**同一侧、同种子、同 pin 起点**的两次跑仍可能逐 k 不一致，根因是 `Invoke-M3PinSettleHold` 的
**重试次数不同**：每次重试都会多跑一轮 pin SQL + settle（异步掉线存档落库窗口）+ observe，
并推移攻击序列开始前的全局 RNG 位置 ⇒ 逐 k 错位。旧夹具不记账重试次数，于是把「重试造成的错位」
**误判成「实现差异」**。

m3cs 的 B-1/B-2 原始件（`evidence-b1b2-20261012/`）复算结果：

| 运行 | 侧 | 会话 | `PIN_HOLD attempt` | `extra_retries` | 判据 |
|---|---|---|---|---|---|
| c1 | C# | `csharp-20261012-155255` | 3 | **1** | 超期望 |
| c2 | C# | `csharp-20261012-160205` | 2 | 0 | 零额外重试 |
| c3 | C# | `csharp-20261012-160446` | 4 | **2** | 超期望 |
| r1 | Rust | `rust-20261012-155659` | 2 | 0 | 零额外重试 |
| r2 | Rust | `rust-20261012-155907` | 2 | 0 | 零额外重试 |

对应逐 k 比对原件：`c2-vs-c3-differs.txt`（不一致，c3 多 2 次重试）、`c1-vs-c3-identical.txt`
（碰巧一致）、`r1-vs-r2-identical.txt`（一致）。**新判据对 c2-vs-c3 直接 fail-closed，不再把它
当「同一序列」比**（见 §4）。

> 如实登记一处观察：`c1(3)` vs `c3(4)` 逐 k **恰好一致**，说明「每次重试必推移 RNG」尚未在
> 所有运行上单独证明；但重试次数不同的两次运行**没有可复现性保证**，判据保守地一律拒绝比对
> （fail-closed），这是 owner 要的口径。真机侧若确认「重试不影响 RNG」，再由 owner 裁决是否放宽
> （本片不放宽）。

## 2. 记账口径（唯一来源：`gate_guards.ps1`）

- **期望值 = 2**（`$script:M3PinExpectedAttempts` / `Get-M3PinExpectedAttempts`）。
  依据：`Get-M3PinHoldVerdict` 要求「**连续两轮** settle 后复验都 match」（`consecutive=2`）⇒
  第 1 轮的 `PreviousMatch` 必为 False，最快 hold 也只能出现在**第 2 次**尝试。
  故「零额外重试」= `attempts == 2`（= `settle_gate` ⑥ 断言的生产正常路径 `attempt=2`）。
  owner 原话「无重试即 attempts==1」按其括注「按你的实现口径写清期望值」收敛为本口径 —— **不是 1**：
  本仪器结构上不可能在 attempt=1 hold，`attempts==1` 已被判 `pin_attempts_under_expected`（仪器被削弱）。
- **结构化字段**：`run_pair.ps1` 把本轮 pin 记账写进 **`baseline.json.pin_attempts`**，形态
  `attempts=<n> expected=2 settle_rounds=<m> max_attempts=<k>`（`Format-M3PinAttempts`），
  并把原始 `PIN_HOLD`/`PIN_RETRY` 行落 **`<会话>/pin_hold.txt`**（自包含，便于事后复算，与字段互证）。
  未走 `-PinStartPose` 的运行保持 `(off)`（逐字节等价旧路径）。
- `-EmitOpsArgs` 增 `pin_attempts_expected`（供无栈自检断言「常量 ≡ 仪器 consecutive」）。

## 3. 判据 + fail-closed（拒绝，不是静默继续）

`gate_guards.ps1` 新增：

- `Get-M3SessionPinAttempts -Dir`：读 `baseline.json.pin_attempts`（兼容结构化字符串与嵌套对象两种形态）。
- `Get-M3PinAttemptsGuard -PlayerDir [-Expected]`：`attempts > expected` ⇒
  **`pin_attempts_retried`**；`< expected` ⇒ `pin_attempts_under_expected`；`expected` 漂移 ⇒
  `pin_attempts_expected_mismatch`；缺字段 ⇒ `pin_attempts_missing`；形态坏 ⇒ `pin_attempts_invalid`；
  baseline 坏 JSON ⇒ `pin_attempts_baseline_invalid`（结构化、不抛异常）。
- `Test-M3PinAttemptsComparable -Dirs`：两侧一起核，任一不达期望即 fail-closed。

接线（`run_attack_pair.ps1`）：

- `Invoke-AttackCrossSideDiff` 在 pose 护栏之后、align/逐 k diff 之前插 **①.5 pin 重试账本护栏**：
  两侧 `baseline.json.pin_attempts` 必须都 == 期望；否则 **`PREREQ_MISSING`（exit 2，fail-closed）**，
  输出 `PIN_ATTEMPTS_OK ...` 才继续。
- 侧运行结束后（`ATTACKSEQ_OK` 之后）对本侧自己的会话再核一遍：本侧超期望 ⇒ `PREREQ_MISSING`（exit 2），
  不让「被重试污染的基线」流到跨侧比对（`-Side csharp` 也立即卡住，省一轮窗口）。
- `-DiffOnly` 走同一 `Invoke-AttackCrossSideDiff`，同样覆盖。

## 4. 历史 pair 判据（真实数据，真跑生产函数）

见 `05-mutation-redcheck.txt`：

```
EXPECTED_ATTEMPTS 2
PAIR c2-vs-c3           => PREREQ_MISSING fail-closed failed=[pin_attempts_retried]
PAIR c1-vs-c3           => PREREQ_MISSING fail-closed failed=[pin_attempts_retried,pin_attempts_retried]
PAIR r1-vs-r2           => PIN_ATTEMPTS_OK 可比 failed=[]
```

⇒ 正是 `c2-vs-c3-differs.txt` 那对（c3 有额外重试）被拦；`r1-vs-r2`（两次都零额外重试）放行。

## 5. 改坏必红（真跑后还原）

- 无栈自检 **⑱**（`01-...txt`）：合成会话 `attempts=2` ⇒ 绿；**人为把 `pin_attempts` 改成 `attempts=3`
  （= 多一次重试）⇒ 必红 `pin_attempts_retried`**；还原 ⇒ 绿。另含四向构造输入
  （missing / invalid / expected 漂移 / attempts<expected）与嵌套对象兼容。
- 独立 harness（`05-...txt`，`E:/tmp/m3bag-pin-retry-redcheck.ps1`）对同一会话跑
  `2 → 3 → 还原 2`，打印 `MUTATION_REDCHECK GREEN(改坏必红 + 还原必绿)`。
- `attack_diff.py pin_attempts` 复算子命令 selftest 3 向（`03-...txt`）：干净日志=`attempts=2` 绿 /
  多一次重试=红 / 缺 `PIN_HOLD`=`PREREQ`（rc=2）。

## 6. 证据件

| 件 | 命令 | 结果 |
|---|---|---|
| `01-selfcheck-nostack-green.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` | **rc=0 / SELFTEST_RED GREEN**；⑱ 期望=2、改坏必红真跑还原；⑯/⑰ 仍绿 |
| `02-selftest-gate.txt` | `selftest_gate.ps1` | **total=57 failed=0 GREEN**（新增 ⑨ `Get-M3PinAttemptsGuard` 10 条） |
| `03-attack-diff-selftest.txt` | `attack_diff.py selftest` | **19/19 GREEN**（新增 (k) pin_attempts 复算 3 条） |
| `04-pin-attempts-recompute.txt` | `attack_diff.py pin_attempts <m3cs stdout>` | c1=3/c2=2/c3=4/r1=2/r2=2（rc 1/0/1/0/0） |
| `05-mutation-redcheck.txt` | `E:/tmp/m3bag-pin-retry-redcheck.ps1` | 历史 pair 判据 + `2→3→还原` 改坏必红 |
| `10-gates-fmt.txt` | `cargo fmt --all -- --check` | rc=0 |
| `11-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `12-gates-test.txt` | `cargo test --workspace --offline` | **79 suites / 654 passed / 0 failed / 16 ignored**，rc=0 |
| `13-gates-msgcodegen.txt` | `cargo run -q -p msgcodegen --offline -- --check` | 604 条一致，rc=0 |

## 7. 复现命令（离线，零端口）

```powershell
# ① 无栈自检（含 ⑱ 改坏必红真跑还原）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=0
# ② 构造输入护栏自检
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1                        # total=57 failed=0
# ③ 对账半段自检
python mir2-rs/tests/parity/m3/attack_diff.py selftest                                                     # 19/19
# ④ 从任意一次侧运行的 stdout 复算 pin 重试次数（历史件也可）
python mir2-rs/tests/parity/m3/attack_diff.py pin_attempts <stdout.txt> [--expected 2]                     # 超期望 rc=1
# ⑤ 四门禁（隔离 target）
cd mir2-rs; $env:CARGO_TARGET_DIR='E:/tmp/m3bag-fij-target'
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --offline -- -D warnings
cargo test --workspace --offline
cargo run -q -p msgcodegen --offline -- --check
```

**环境提示（复现用，非本片产物）**：若 Windows PowerShell 5.1 报
`Get-FileHash is not recognized`，是调用方 shell 的 `PSModulePath` 被 PS7 模块路径占先所致
（`run_pair` 的 `Test-M3MonGenEmpty -VerifySource` 用到 `Get-FileHash`）；清掉该环境变量即可：
`env -u PSModulePath powershell -ExecutionPolicy Bypass -File ...`。

## 8. 残留物清单

- 入库：本目录 + 上述 5 个源文件（`gate_guards.ps1` / `run_pair.ps1` / `run_attack_pair.ps1` /
  `selftest_gate.ps1` / `attack_diff.py`）。
- 仓外临时物（可删）：`E:/tmp/m3bag-pin-retry-redcheck.ps1`（harness）、`E:/tmp/m3bag-fij-target`
  （复用隔离 target，非本片新造）、`E:/tmp/m3bag-tick4-*.txt`（中间捕获）。
- 端口/进程：本片**未起栈/未占端口**（离线可判）；共享 3306 不属本线。
- source-mutation 已还原（工作树改动即生产改动，`git status` 无 `.bak`/临时件）。
