# M3②③ 攻击夹具 · F-I + F-J 证据包（2026-10-12，m3bag / worker-4）

卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）。
来源：owner 2026-10-12 13:45 brief（m3cs 第三轮真机审计 F-H/F-I/F-J；本片收 **F-I** 与 **F-J 自检缺口**，
F-H 与 F-J 的本体已在本地 `263a0175`）。**零 `.rs`**；改 `src/BotSrv/**` = 0；
写集 = `mir2-rs/tests/parity/m3/{gate_guards,run_pair,run_attack_pair}.ps1` + 新 fixture
`mir2-rs/tests/parity/m3/fixtures/mongen-map0.tsv` + 本目录 + `whitelist.md`/`README.md` 文档。

## 1. F-I（Important）：攻击 pin 必须落在无怪干扰处

### 根因（真机实测）

旧缺省 pin = `map=0 attacker=(289,618) dir=2 victim=(290,618)`。把该点与**冻结内容**
`Envir/MonGen.txt` 的 map-0 刷怪圈做几何判定：

```
最近刷怪圈 = 0 256 572 ... 50
hypot(289-256, 618-572) = 56.6
净距 = 56.6 - 50 = 6.6 格
```

⇒ 怪可以刷到 attacker/victim 身上。旧 pin 的会话 `csharp-20261012-130755` 的 `attack_raw.jsonl`
里 29 条 `struck` 有 **27 条来自第三方 src `1330629693`（怪）打在 attacker 身上**，attacker
在序列中被怪持续打击；同种子两次跑 hit 数 **2 vs 7**（owner/m3cs 另有一次 **0/20**）。

### 修法（可判、可复现）

- 新缺省 pin = `map=0 attacker=(429,320) dir=2 victim=(430,320)`（比奇城东门外空地）。
  对 map-0 全部刷怪圈的**最近净距 = 99.2 格**（最近圈 `333 268 r=10`：`hypot(429-333,320-268)=109.5-10`）。
- 唯一来源 = `gate_guards.ps1` 的 `$script:M3AttackPinCX/CY`；`run_attack_pair.ps1` 经
  `-AttackPinCX/-AttackPinCY`（缺省 <0 ⇒ 取常量）透传给 `run_pair.ps1` 的 `-PinCX/-PinCY`。
- **机判守卫**：`gate_guards.ps1::Test-M3AttackPinNoMonster` 读只读抽取
  `fixtures/mongen-map0.tsv`（map-0 全部刷怪点 `x\ty\trange`，内容冻结），要求净距 ≥ 50。
  - `run_pair.ps1` 在 `-AttackPinPair` 启动时 **fail-closed**（`throw`，不等真机跑出随机红）；
  - `run_attack_pair.ps1 -SelfCheckNoStack` **⑯** 两向：生产落点必须绿、旧缺省 (289,618) 净距 6.6 必红。

### 真机验收（同配置同种子连跑 2 次，C# hook 栈 seed=42）

| 项 | run1 `csharp-20261012-135416` | run2 `csharp-20261012-135539` |
|---|---|---|
| pose | `pair map=0 attacker=429,320,dir=2 victim=430,320 level=1 vhp=30000` | 同左（逐字节相同） |
| 开关 | `on count=20 step_ms=500 start_delay_ms=3000 disable_autoplay=True` | 同左 |
| seed | 42（`INJECT_OK`） | 42（`INJECT_OK`） |
| `PIN_NO_MONSTER` | `pin=(429,320) clearance=99.2 nearest=333,268 r=10` | 同左 |
| 序列完整 | `attack_seq_done=1/1`、`ATTACKSEQ_OK markers=24` | 同左 |
| 判据 | `attacked=20 hits=1 dodges=19 sum=2 death_k=null min_hp=18`、`GREEN` rc=0 | `attacked=20 hits=1 dodges=19 sum=2 death_k=null min_hp=18`、`GREEN` rc=0 |
| 第三方 struck | **0**（仅 1 个 src = attacker） | **0**（仅 1 个 src = attacker） |

⇒ 两次都 **20/20 + done + 命中非零**，且**无怪干扰**（`18-...txt` 的 src 分布：各 run 仅 1 个 src）。

### 残留（如实登记，不粉饰）

同种子两次跑的**逐 k 序列并不相同**（run1 hit 落在 `k=2`，run2 落在 `k=4`；见 `17-...txt`）。
原因：攻击伤害/命中判定取的是 oracle 全局 `M2Share.RandomNumber`，而该流被**全图怪物的
`AnimalObject.Wondering` / `Merchant.Run` / `MonGetRandomItems` 等持续消耗**（hook 流水里
5.67M 次调用，site 直方图见 `whitelist.md` T-29）。⇒「同种子 ⇒ 逐 k 相同」这一**更强**前提
当前不成立；本片只保证 F-I 的症状（第三方打击导致 attacker/victim 被打死/打断 ⇒ hits 随机为 0）
已消除。跨侧 diff 的口径是否要落在**聚合量**上或按 §6.0-3 登记，交 owner/m3cs 裁决。

## 2. F-J（自检缺口）：`-SelfCheckNoStack ⑧` 不得吞掉待测函数返回值

`Invoke-AttackAlign` 的本体修复（stdout 走 host、返回值只留 `[int]$LASTEXITCODE`）在 `263a0175`。
但旧 **⑧** 是 `& python $AttackDiff align $rawA --out $seqA | Out-Null` + 读 `$LASTEXITCODE`
⇒ 正好绕过「返回值被 stdout 污染」这个 bug。**修**：⑧ 改成把合成 raw 放进
`<tmp>/a/attack_raw.jsonl`、`<tmp>/b/attack_raw.jsonl`，**直接调用生产函数 `Invoke-AttackAlign`**，
断言两次返回值都是 `[int]` 且为 0；并在合成 attack 行补 `"my":101` 以真正走 src 过滤。

**改坏必红**（`08-mutation-fj-align-rc-red.txt`）：把函数体改回 `& python …; return $LASTEXITCODE`
⇒ 无栈自检 rc=1，同时 **⑧** `align_synthetic_rc_not_int=Object[]`、`align_synthetic_rc_b_not_int=Object[]`
与 **⑮** `attack_align_rc_not_int=Object[]` 三条并发报红。还原后 rc=0（`07-...txt`）。

## 3. 无栈自检 / 红检原始输出

| 件 | 命令 | 结果 |
|---|---|---|
| `07-selfcheck-fij-green.txt` | `run_attack_pair.ps1 -SelfCheckNoStack` | **rc=0**；⑥ pose=`429,320/430,320`；⑯ 生产净距 99.2 / 旧缺省必红；⑧ 绿 + 改坏 dmg/hit 必红；⑮ `type=Int32` |
| `08-mutation-fj-align-rc-red.txt` | 把 `Invoke-AttackAlign` 改回 stdout 泄漏后同上 | **rc=1**；`attack_align_rc_not_int=Object[], align_synthetic_rc_not_int=Object[], align_synthetic_rc_b_not_int=Object[]` |
| `09-mutation-fi-pin-near-monster-red.txt` | 把 `$script:M3AttackPinCX/CY` 改回 `289/618` 后同上 | **rc=1**；`emit_pair_rc=non-zero, attack_pin_near_monster=6.6`（`run_pair` F-I 守卫 fail-closed） |
| `10-selftest-gate-47of47.txt` | `selftest_gate.ps1` | **total=47 failed=0 GREEN** |

## 3b. 四门禁（本片零 `.rs`；隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-fij-target`，在 `mir2-rs/` 下跑）

| 件 | 命令 | 结果 |
|---|---|---|
| `19-gates-fmt.txt` | `cargo fmt --all -- --check` | rc=0 |
| `20-gates-clippy.txt` | `cargo clippy --workspace --all-targets --offline -- -D warnings` | rc=0 |
| `21-gates-test.txt` | `cargo test --workspace --offline` | **79 suites / 648 passed / 0 failed / 16 ignored**，rc=0 |
| `22-gates-msgcodegen.txt` | `cargo run -q -p msgcodegen --offline -- --check` | 604 条一致，rc=0 |

干净检出复核（F-1 同族）：`git clone --local` 到 `E:/tmp/m3bag-fij-clone` ⇒ 两个证据目录
`verify_sha256sums.py --check-extra` 均 GREEN（6/6、24/24），`git check-ignore` 命中 0。

## 4. 端口 / 窗口纪律（本片起了真机栈，已释放）

- `01-ports-before.txt`：起栈前实拍 —— 仅共享 3306。
- `02-csharp-stack-start.txt`：起 C# hook 栈（`E:/tmp/m3bag-fi/csharp-stack/start-csharp-hook.ps1 -Seed 42`），
  9 端口全 LISTEN + `CSHARP_STACK_GREEN`。
- `05-csharp-stack-stop.txt` / `06-ports-after.txt`：`-Stop` 后十端口实拍 —— **只剩 3306**（窗口已释放）。
- 两 run 均按「禁管道、重定向到文件」起（先例 `LESSON_PowerShell起长命服务会占住调用方stdout管道`）。

## 5. 复现命令

```powershell
# 0) 无栈自检 + 红检（不需端口）
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack   # rc=0
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/selftest_gate.ps1                        # 47/0
# 1) 真机（需窗口；MySQL 3306 共享 + 5000/7000/7100/7200/6000/5100/5700/5500/5600 独占）
powershell -ExecutionPolicy Bypass -File E:/tmp/m3bag-fi/csharp-stack/start-csharp-hook.ps1 -Seed 42      # 需自备脚本
powershell -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -Side csharp -Bots 2 `
  -AttackCount 20 -AttackPinPair -Prefix m3bfi1 -CsharpSeedLog E:/tmp/m3bag-fi/csharp-stack/rng-stream.tsv
# 换前缀 m3bfi2 再跑一次；收工
powershell -File E:/tmp/m3bag-fi/csharp-stack/start-csharp-hook.ps1 -Stop
```

## 6. 残留物清单

- 入库：本目录 + `fixtures/mongen-map0.tsv` + `gate_guards.ps1` / `run_pair.ps1` / `run_attack_pair.ps1`
  + `evidence-attack-pair-feh-20261012/**`（F-E/F-H）+ `whitelist.md` / `README.md`（m3）。
- 仓外临时物（可删）：`E:/tmp/m3bag-fi/**`（C# stack 日志、冒烟输出、`.bak`）、
  `E:/tmp/m3bag-fij-target`（隔离 cargo target）、`E:/tmp/m3bag-fij-clone`（干净 clone 复核）、
  `E:/tmp/m3bag-map0*.bmp/png`（选点用图）。
- 端口/进程：本片起的 C# 栈已 `-Stop`，`06-ports-after.txt` = 只剩 3306。
- source-mutation 已还原（`git status` 中 `gate_guards.ps1`/`run_attack_pair.ps1` 的改动即生产改动）。
