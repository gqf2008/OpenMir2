# M3②③ 活栈隔离口径 —— 两相真机实跑证据包（2026-10-12，m3cs / openmir2-worker-5）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` · branch `worktree-m3-combat-skills`
· lane `m3cs` / principal `openmir2-worker-5`。

## 0. 本片测的是什么（口径）

- **Rust 半**：`b9e37389`（已合入 master `50fc02bd`）——攻击路径专用种子流 `World.rng_attack` +
  `MIR2_RNG_ISOLATE_ATTACK` 开关（关时逐字节不变）。
- **C# 半**：`7d3fb333`（同卡 m3bag 交付、**待审未合**）——影子 hook 按调用点分流到独立
  `Random(attack_seed=MIR2_RNG_SEED+0x9E37)`；夹具 `-IsolateAttack` 透传。
- 两侧同 `MIR2_RNG_SEED=42` ⇒ `attack_seed=40545`（= 42 + 0x9E37）。
- 判据（DoD②，**未放宽**）：`attack_diff.py diff <csharp seq> <rust seq>` **逐 k = 0**。
- 场景：pin 图 `0137`（竞技场，零刷怪/零 AI 消费者，`source_verified=True`）、`-AttackPinPair`、
  `-Bots 2`、attacker `(17,20) dir=2`、victim `(18,20)`。

> ⚠ 本片在 **C# 半尚未合入 master** 时执行（owner 21:15 派活：Rust 半就绪后用 C# 半的夹具跑两相）。
> C# 半的代码身份以 `7d3fb333` 钉住；若其复审改动落地，本片结论需在改后 head 上复跑。

## 1. 命令（原始输出在 `window/`）

```powershell
# 0) 离线自检（含隔离口径 21；无栈）
pwsh -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -SelfCheckNoStack
#     -> window/00-selfcheck-nostack.txt（SELFTEST_RED GREEN）

# 1) C# stack1（影子 + 隔离；-Repo 指主检出以取冻结 C# 构建）
pwsh ... -File mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -Seed 42 -IsolateAttack \
  -LogDir E:/tmp/m3cs-iso2/csharp -Repo E:\Users\gxh\Documents\GitHub\OpenMir2 -SkipHookDeploy
#     -> window/03-csharp-stack-start.txt（栈绿；末段 ReadAllBytes 异常，见 §4-F1）
# 20 冒烟 -> window/04；120 c1 -> window/05；同栈再 120 c2 -> window/06
pwsh ... -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -Side csharp -Bots 2 \
  -AttackCount 20  -AttackPinPair -IsolateAttack -Prefix m3iso20 -Seed 42 \
  -CsharpSeedLog E:/tmp/m3cs-iso2/csharp/rng-stream.tsv
pwsh ... -AttackCount 120 -AttackPinPair -IsolateAttack -Prefix m3isoc1 -Seed 42 ...
pwsh ... -AttackCount 120 -AttackPinPair -IsolateAttack -Prefix m3isoc2 -Seed 42 ...
pwsh ... -File mir2-rs/tests/parity/m3/start-csharp-hook.ps1 -Stop -LogDir ...   # window/07

# 2) Rust stack1（同种子 + 隔离；env MIR2_RNG_ISOLATE_ATTACK=1 / MIR2_RNG_LOG=.../rng-stream.tsv）
$env:MIR2_RNG_ISOLATE_ATTACK='1'; $env:MIR2_RNG_LOG='E:/tmp/m3cs-iso2/rust/rng-stream.tsv'
pwsh ... -File tools/m2-acceptance/start_rust_stack.ps1 -RustBinDir E:/tmp/m3cs-r2-target/debug \
  -Seed 42 -LogDir E:/tmp/m3cs-iso2/rust                              # window/08
pwsh ... -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 -Side rust -Bots 2 -AttackCount 20 \
  -AttackPinPair -IsolateAttack -Prefix m3iso20r -Seed 42 -PeerSession ... # window/09
pwsh ... -Side rust -AttackCount 120 -AttackPinPair -IsolateAttack -Prefix m3isor1 -Seed 42 \
  -PeerSession csharp-20261012-214806 ...                              # window/10（跨侧 83 ≠ 0）
pwsh ... -File tools/m2-acceptance/start_rust_stack.ps1 -Stop -LogDir ...  # window/11

# 3) 干净对照（两侧都取「无冒烟的第一发 120」）
#    C# stack2 -> window/12,13,14；Rust stack2 -> window/15,16
#    cA vs rA -> window/17（跨侧 73 ≠ 0）
```

## 2. 会话 ↔ 用途

| 会话 | 侧/栈 | 用途 | attacked | hits | dodges | sum |
|---|---|---|---|---|---|---|
| `csharp-20261012-214703` | C# stack1 | 20 冒烟 | 20 | 2 | 18 | 2 |
| `csharp-20261012-214806` | C# stack1 | **120 c1（跨侧 peer）** | 120 | 1 | 119 | 1 |
| `csharp-20261012-214957` | C# stack1 | 120 c2（同栈第二次） | 120 | 2 | 118 | 2 |
| `csharp-20261012-215606` | C# stack2 | **120 cA（干净首跑）** | 120 | 2 | 118 | 2 |
| `rust-20261012-215215` | Rust stack1 | 20 冒烟 | 20 | 9 | 11 | 9 |
| `rust-20261012-215323` | Rust stack1 | **120 r1（跨侧 peer）** | 120 | 19 | 101 | 19 |
| `rust-20261012-215832` | Rust stack2 | **120 rA（干净首跑）** | 120 | 19 | 101 | 19 |

两侧 `attack_isolation.json`：`isolate_attack=true`、`attack_seed=40545`；C# 流水头
`ISOLATE_ATTACK 40545 isolate_sites=...`、Rust 流水头 `ISOLATE_ATTACK 40545`。
两侧 `PIN_ATTEMPTS_INFO attempts=2/expected=2 retried=False`、`ATTACKSEQ_OK raw=True`、
`MAP_HAS_NO_MONSTER/NO_AI_CONSUMERS source_verified=True`。

## 3. 逐 k 判据矩阵（`window/17..26`）

| 对比 | 侧 | 差异项 | 结论 |
|---|---|---|---|
| cA vs rA | C# stack2 首 120 ↔ Rust stack2 首 120 | **73** | 干净跨侧 **≠ 0** |
| c1 vs r1 | C# stack1 ↔ Rust stack1（协议含冒烟） | **83** | 跨侧 **≠ 0** |
| cA vs r1 | C# stack2 首 120 ↔ Rust stack1 | 80 | 跨侧 ≠ 0 |
| c1 vs c2 | C# **同栈** 两次 120 | 7 | 同栈**不可复现**（hits 1 vs 2） |
| c1 vs cA | C# 两栈各一次 | 7 | — |
| cA vs c2 | C# 两栈 | **0** | 巧合全等 |
| rA vs r1 | Rust 两栈（clean vs 含冒烟） | **77** | Rust 侧对起点极敏感 |

⇒ **DoD②（隔离口径逐 k=0）未达成**；本片**不宣布达成、不放宽判据**。

## 4. 分流站点取数次数（隔离口径的「逐段定位」，`window/20-site-counts.txt`）

120 击（各侧无冒烟首跑）下，攻击路径分流站点取数次数：

| canon | C# 站点 | C# 次数 | Rust 站点 | Rust 次数 |
|---|---|---|---|---|
| atk_power_a | `PlayObject.cs:2929` | 120 | `combat.rs:109` | 120 |
| atk_power_b | `PlayObject.cs:2934` | 120 | `combat.rs:111` | 120 |
| atk_power_base | `BaseObject.Base.cs:892` | **16** | `combat.rs:67` | **0** |
| hit_gate | `BaseObject.Attack.cs:41` | **19** | `combat.rs:758` | **38** |
| hit_struck_damage | `BaseObject.Base.cs:792` | 17 | `combat.rs:628` | 19 |
| hit_struck_damage2 | `BaseObject.Base.cs:793` | **4** | `combat.rs:630` | **0** |
| struck_durability | `PlayObject.cs:2957` | **1** | `combat.rs:601` | **19** |
| struck_durability_slot | `PlayObject.cs:3000` | 0 | `combat.rs:609` | 0 |
| attack_success | `PlayObject.Attack.cs:223` | **3** | `combat.rs:831` | **38** |
| **合计** | | **300** | | **354** |

⇒ 隔离口径把「两侧攻击路径**取数结构**不一致」暴露出来：即使两条专用流同种子、同一公式，
每击的取数条数/位置不同 ⇒ 流从第 1 击起就分叉 ⇒ 逐 k ≠ 0。这是**结构差**（非统计/集合问题），
本片给证据、**不下判决、不改判据**。

## 5. 夹具 finding（C# 半 `7d3fb333`，m3bag 写集；作者侧只登记）

- **F1（Important，真机才触发）**：`start-csharp-hook.ps1:51` 的 `Get-Sha256Hex` 与
  `gate_guards.ps1:45` 的 `Get-M3Sha256Hex` 都用 `[System.IO.File]::ReadAllBytes`（FileShare.Read），
  而 hook 以 `FileShare.ReadWrite` 持有 `rng-attack-stream.tsv`；Windows 共享模式下新 open 的
  share 必须包含既有 handle 的 access ⇒ 读被拒。
  实测：`start-csharp-hook.ps1 -IsolateAttack` 在写完 `ISOLATION_RECORDED` 前抛
  `The process cannot access the file ... because it is being used by another process`（栈本身已起、端口已就绪；
  见 `window/03`、`window/12`）。
  同一路径：`run_attack_pair.ps1` 在 `-AttackStreamLog`/`$env:MIR2_RNG_ATTACK_LOG` 被设置时，
  会把 `attack_stream_sha256` 计算成 `ReadAllBytes` ⇒ 同样会崩。
  本片规避：不设 `MIR2_RNG_ATTACK_LOG`（`attack_isolation.json.attack_stream_sha256=""`；
  跨侧护栏只比 `isolate_attack`/`attack_seed`，不受影响）。专用流 sha256 由本包
  `provenance/00-provenance.txt` 在栈停后补记。
  ⇒ 建议 m3bag 把两处 `ReadAllBytes` 改成 `FileStream(FileShare.ReadWrite)`（与它本片
  `Get-FileHash→.NET SHA256` 同族收口）。
- **F2（Minor）**：`attack_rng_ledger.py` 的 `RUST_ATTACK_RE` 仍是陈旧行号 ⇒ 同一条 Rust 流水
  `attack=2/段`（漏 758/628/601/831）而实际 6/段；本片定位改用直接站点计数（`window/20`），未依赖该正则。

## 6. 窗口 / 清理

- 起栈前实拍：`window/02-ports-before.txt`（5000..7200 全 free，仅共享 3306）。
- 收工：`window/07/11/14/18`（-Stop）+ `window/19-ports-after.txt` ⇒ 十端口全 free，仅共享 3306
  ⇒ **窗口已释放**。
- 大流水（112MB/31MB）不入库；sha256+字节数见 `provenance/00-provenance.txt`，原件在
  `E:/tmp/m3cs-iso2/provenance/`。
