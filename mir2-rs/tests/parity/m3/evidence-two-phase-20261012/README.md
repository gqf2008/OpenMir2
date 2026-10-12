# M3②③ 真机两相逐 k 复核 — 证据包（2026-10-12，m3cs / openmir2-worker-5）

卡：`openmir2-m3-combat-skills-align`（issue `a2ef74aa`）· worktree `m3cs` · branch
`worktree-m3-combat-skills` · base = master **`8d9d0601`**（= `894ee05e` + `a0d93bcf`；
`a0d93bcf` = m2-svr「无目标一击接 `World::attack_swing`」接线，approve reviewer-5 `3b0b4437`）。

判据（DoD②，**未放宽**）：`attack_diff.py diff <csharp seq> <rust seq>` **逐 k = 0**。
本片结果：**80 项 ≠ 0 ⇒ DoD② 未达成**，如实登记，未声明达成。

## 1. 窗口与命令（原始件在 `window/`）

```
# 0) 起栈前实拍九端口 + 冻结基线
(cd E:/MirServer && powershell -ExecutionPolicy Bypass -File port-status.ps1)      -> window/00-ports-before.txt

# 1) C# r1 栈（rng-hook，-Seed 42；默认落点 E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv）
(cd E:/tmp/m3cs-tick && powershell -ExecutionPolicy Bypass -File start-csharp-hook.ps1 -Seed 42)  -> window/01-...
# 20 冒烟（pwsh：本机 PS 5.1 缺 Get-FileHash，见 §5）
pwsh -NoProfile -ExecutionPolicy Bypass -File mir2-rs/tests/parity/m3/run_attack_pair.ps1 \
  -Side csharp -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3cs2pc20 \
  -CsharpSeedLog E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv -Seed 42                 -> window/02-csharp-smoke-20.txt
# 120 —— 本片的跨侧 C# peer
pwsh ... -Side csharp -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3cs2pc1 \
  -CsharpSeedLog E:/tmp/m3cs-tick/csharp-stack/rng-stream.tsv -Seed 42                 -> window/03-csharp-120.txt
# 同栈内再跑一次 120（同侧重跑对照）
pwsh ... -Prefix m3cs2c3 ...                                                          -> window/03b-csharp-120-rerun.txt
(cd E:/tmp/m3cs-tick && powershell -ExecutionPolicy Bypass -File start-csharp-hook.ps1 -Stop)     -> window/04-...

# 2) Rust 栈（-Seed 42；MIR2_RNG_LOG 经子进程环境继承；binary = E:/tmp/m3cs-r2-target/debug，从 HEAD 8d9d0601 重编）
MIR2_RNG_LOG=E:/tmp/m3cs-2phase/rust-rng-stream.tsv pwsh ... tools/m2-acceptance/start_rust_stack.ps1 \
  -RustBinDir E:/tmp/m3cs-r2-target/debug -Seed 42 -LogDir E:/tmp/m3cs-2phase/rust-stack   -> window/05-rust-stack-start.txt
pwsh ... -Side rust -Bots 2 -AttackCount 20 -AttackPinPair -Prefix m3cs2prs0 -Seed 42 \
  -RustSeedLog E:/tmp/m3cs-2phase/rust-stack/game-svr.out.log -RustStackLogDir ...          -> window/06-rust-smoke-20.txt
pwsh ... -Side rust -Bots 2 -AttackCount 120 -AttackPinPair -Prefix m3cs2pr1 \
  -PeerSession csharp-20261012-204722 -Seed 42 -RustSeedLog ... -RustStackLogDir ...         -> window/07-rust-120-crossside.txt
pwsh ... start_rust_stack.ps1 -Stop -LogDir E:/tmp/m3cs-2phase/rust-stack                   -> window/09-...

# 2b) C# r2 栈（fresh stack，用于「同侧 fresh-stack 复现」对照）
start-csharp-hook.ps1 -Seed 42 -> 20 冒烟(m3cs2r20) -> 120(m3cs2r1) -> -Stop
                                                                                     -> window/01b,02b,03c,04b

# 3) 逐 k 判据
python mir2-rs/tests/parity/m3/attack_diff.py diff \
  sessions/csharp-20261012-204722/attack_seq.jsonl sessions/rust-20261012-205252/attack_seq.jsonl \
  --max-diffs 200                                                                     -> window/11-crossside-diff.txt
python ... attack_diff.py diff sessions/csharp-20261012-204722/attack_seq.jsonl \
  sessions/csharp-20261012-205707/attack_seq.jsonl --max-diffs 200                     -> window/12-sameside-csharp-diff.txt

# 4) 收工十端口实拍
(cd E:/MirServer && powershell ... port-status.ps1)                                   -> window/15-ports-after.txt
```

## 2. 会话 ↔ 用途

| 会话 | 侧 | 用途 | 汇总 |
|---|---|---|---|
| `csharp-20261012-204618` | C# r1 | 20 冒烟 | attacked=20 hits=2 dodges=18 sum=2 |
| `csharp-20261012-204722` | C# r1 | **120 · 跨侧 C# peer** | attacked=120 hits=2 dodges=118 sum=2 |
| `csharp-20261012-204927` | C# r1 | 120 同栈重跑 | attacked=120 hits=2 dodges=118 sum=2（逐 k 与 204722 **不同**，见 §3） |
| `csharp-20261012-205603` | C# r2 | 20 冒烟（fresh stack） | attacked=20 hits=1 dodges=19 sum=1 |
| `csharp-20261012-205707` | C# r2 | **120 · fresh-stack 复现** | attacked=120 hits=1 dodges=119 sum=1 |
| `rust-20261012-205150` | Rust | 20 冒烟 | attacked=20 hits=13 dodges=7 sum=13 |
| `rust-20261012-205252` | Rust | **120 · 跨侧 Rust** | attacked=120 hits=19 dodges=101 sum=19 death_k=38 |

`sessions/<会话>/{attack_seq.jsonl,attack_raw.jsonl,baseline.json,attack_seed.json,bot.log.txt}` 为本片入档副本；
两侧 RNG 权威流水的「路径 + sha256 + 字节数」见 `30-rng-stream-provenance.txt`（原件 100 MB 级，不入库）。

两侧均：`SEED_INJECT_OK`（seed=42）、`PIN_ATTEMPTS_INFO attempts=2/expected=2 retried=False`、
`ATTACKSEQ_OK raw=True`、`MAP_HAS_NO_MONSTER/NO_AI_CONSUMERS source_verified=True`（地图 `0137`）、
无 `PREREQ_MISSING`。

## 3. 判据结果（DoD② = 逐 k 0）

- **跨侧 204722 vs 205252 = 80 项差异**（`window/11-crossside-diff.txt`）。
  代表性：k=1 C# miss / Rust hit；k=5,6,7,8 起命中模式整体错位；统计
  `sum 2 vs 19`、`dodges 118 vs 101`、`death_k None vs 38`（Rust 侧 victim 在 k=38 归零）。
- **同侧 fresh-stack C# 复现 204722 vs 205707 = 15 项差异**（`window/12-sameside-csharp-diff.txt`）。
  ⇒ C# 侧「同种子同配置、fresh stack」**并非逐 k 可复现**；同栈内连续两次 120（204722 vs 204927）
  同样不同。此现象与 brief 顶部 owner 19:40 的注记一致（「C# 不可自复现已被判决实验证实」）。

## 4. 归因（给证据，不下判决）

1. **攻击路径取数结构已对齐（本片证实接线生效）**：Rust rng 账本 140 击 = 140 段，逐段形状
   `atk_power_a → atk_power_b → hit_gate → [hit_struck_damage → struck_durability] → attack_success`
   （`window/27-rust-segments.txt`），与 oracle 顺序同形；C# 侧同族站点齐备
   （`atk_power_a/b`、`hit_gate`、`attack_success`、`hit_struck_damage`、`atk_power_base`、`struck_durability`）。
2. **全局随机源消费者数量级差**：C# 流水 `noise_ratio=0.9995`（1346972/1347599），Rust `0.0171`（8/468）
   （`window/20-rust-ledger-summary.txt` + `30-rng-stream-provenance.txt`）⇒ 攻击窗口的 RNG **起点**
   两侧不可比 ⇒ 逐 k 不齐属**流位置类**（Claim B），非攻击路径结构差。
3. **但仍不能仅凭 ①②排除实现差**：Rust 命中率系统性高于 C#（烟 13/20、120 命中 19/120 vs
   C# 烟 2/20、120 命中 2/120；命中门 = `attacker.HitPoint < RandomByte(target.SpeedPoint)`，
   见 `BaseObject.Attack.cs:37-44` / `combat.rs:752-760`）⇒ 建议下一片做**判别实验**：分别注入
   两侧同值 `HitPoint/SpeedPoint`（或把攻方 `HitPoint` 打到远超/远低于 victim `SpeedPoint`）
   看命中率是否随属性走；若随属性走 ⇒ 攻方属性输入差（m2-svr `bins/game-svr` 装载路径）；
   若只随种子走 ⇒ 纯流位置差（Claim B）。
4. `-AttackVictimHp 30000` 未生效（两侧 seq 的 victim `maxhp` 均为 19，Rust 在 k=38 死亡）⇒
   120 击的尾部落在「无目标/目标已死」区，按 oracle `Envir.GetMovingObject(...,true)` 的
   `!Ghost && !Death` 不再进入攻击判定。此点与 m2-svr 早先「84/120」归因同源。

**DoD② 结论：未达成。** 请 owner 裁决收口口径（§6.0-3 登记残留 / 授权「攻击窗口前两侧同步重播种」/
或先做 §4.3 的属性判别实验）。本片**未**自行改判据、未改门、未改白名单。

## 5. 夹具发现（不在本片写集，只登记）

1. **`powershell`(PS 5.1) 本机缺 `Get-FileHash`**（`Get-Command Get-FileHash` ⇒ CommandNotFound；
   `pwsh` 7.6.6 正常）。m3 夹具 `gate_guards.ps1:555,694` 与 `rng-baseline.ps1:45` 仍直接调
   `Get-FileHash`，而 README 的示例用 `powershell` 起 `run_attack_pair.ps1` ⇒ 按 README 照抄会
   在 `Test-M3MonGenEmpty -VerifySource` 处 `CommandNotFoundException` 崩溃（本片首跑即复现，
   见 `window/02-csharp-smoke-20.txt` 之前的手动失败；改 `pwsh` 后绿）。
   与 owner 19:45 派给 m2-svr 的 `Get-FileHash` 同族；本处属 m3bag 写集，建议统一为 .NET `SHA256`。
2. `-AttackVictimHp 30000` 不改变 victim `maxhp`（= 19），victim 会在约 k=36~38 归零；
   若「120 击全程有目标」是判据前提，则 pin 需修（m3bag 写集）。
3. 前缀长度上限 10（含序号）：`-Prefix m3cs2pc20b`(11) 直接 `RED: 账号名最长 11 字符 > 上限 10`；
   属护栏正常工作，仅记。

## 6. 窗口 / 清理

- 起栈前实拍：`window/00-ports-before.txt`（5000..7200 全缺，仅共享 3306）。
- 收工：`window/04-*`、`04b-*`、`09-rust-stack-stop.txt` + `window/15-ports-after.txt`
  ⇒ **十端口全空（仅余共享 MySQL 3306）**，`tasklist` 无 game-svr/GameSrv/BotSrv ⇒ **窗口已释放**。
- 本片 `src/ sql/ MirClient` 与 `mir2-rs/crates|bins` **零改动**（零 `.rs`）。
