# butch 真机链执行清单：账本恰两抽（卡 `openmir2-butch-two-draws-unblock`，tier R）

- 卡：`openmir2-butch-two-draws-unblock`（owner `openmir2-dev-1`；branch `card/butch-two-draws-unblock`）。
- 本文件是**离线准备件**：把真机那一轮要做什么、按什么顺序做、判据用哪条命令判，先写死；
  真机证据另存证据根，本文件不预填任何真机结论。
- 状态：真机轮**未跑**（等窗口）。本文件与 `tools/window/butch_ledger.py` 均不含真机数据。

## 1. 判据（唯一成功口径）

oracle：`src/M2Server/Player/PlayObject.Operate.cs:702 ClientGetButchItem`。成功 = **RNG 账本恰两抽**：

| 抽 | 位置 | 模数 | 值域 | 落到 | 账本判定 |
|---|---|---|---|---|---|
| 1 | `Operate.cs:722` | `Random(16)` | `+5 ∈ [5,20]` | `BodyLeathery -= n10` | `Next(max) max=16` **恰 1 条** |
| 2 | `Operate.cs:723` | `Random(201)` | `+100 ∈ [100,300]` | `MeatQuality -= meat` | `Next(max) max=201` **恰 1 条** |

- **次序**：第 1 抽的 `seq` 必须早于第 2 抽。
- **`SM_BUTCH(637)` 不是判据**：`Operate.cs:749` 在函数尾部**无条件**发（唯一早退是速度门 `:709-712`）
  ⇒ 637 到达只证「请求被受理」，**只作伴随证据**（卡 `openmir2-butch-637-criterion-fix` 已改判据）。
- **`SM_SKELETON(33)` 不是必发**：仅 `BodyLeathery <= 0` 时发（`:731-738`），首刀远到不了 0，作加强项。
- 门清单（解释「为什么两抽可能没发生但 637 照样到」）：`:716` 每轴 ≤2 格 → `:718 IsValidObject(nX,nY,2)`
  → `:720 Death && !Skeleton && Animal`，**都是嵌套 if、不早退**，门不过只跳过两抽。

## 2. 前置（开工前逐条打勾）

1. **机器归属**：`powershell -File E:\MirServer\port-status.ps1` + 查 `cargo/rustc`；有主就**不开窗**。
   开窗前签 `stack-claim`（目的 + ETA），跑完 `-Stop` 后签 `stack-release`。
2. **夹具**：`tools/window/profiles/butch_monsters.json` 提供 8 格 ×4 = **32 只鸡**（map 0），
   `fixtures.inventory_replace` 槽 40 = 木剑（`make_index=5001`，`std_mode=5`，`ani_count=1`= Shape）。
3. **场景**：`tools/probe/scenarios/butch_two_draws.json`（7 步；`max_retries=6`、`interval_ms=1000`；
   `send CM_BUTCH recog=@last_kill_actor param=@last_kill_x tag=@last_kill_y`）。
4. **C# RNG 接线**：`tools/window/start-csharp.ps1` 在给 `-RngLog` 时注入
   `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG` + `MIR2_RNG_SITE=1`（只给 GameSvr 子进程）。
   **不接就采不到账本 ⇒ 判据不可判，不要写成 PASS/FAIL**。
5. **两侧同种子**：Rust 起栈脚本 `-Seed 42`；C# 用 `-Seed 42`。种子不等 ⇒ 账本不可比。
6. **构建**：Rust 侧二进制必须与当前分支同源（`require_pid_binaries` 会按 profile 的
   `binaries` 校验 pid 的可执行路径；路径对但**内容旧**不会报错——开窗前先 `cargo build --release`）。

## 3. 跑法（一条命令 + 两个必看字段）

```powershell
python -I -B tools/conformance/card_window.py --card openmir2-butch-two-draws-unblock `
  --profile tools/window/profiles/butch_monsters.json `
  --scenario tools/probe/scenarios/butch_two_draws.json `
  --rng-log --timeout-ms 60000 --out E:/tmp/window/butch-<轮次>
```

（`CONFORMANCE_PASSWORD` 由环境给；`--rng-log` 让每侧落 `<out>/<side>/rng-stream.tsv`。）

跑完**先看两个字段**（在 `<out>/<side>/run.json` 与 `card-window.json` 的 `sides.<side>`）：

- `rng_log`：账本路径存在且非空；
- **`rng_log_trim.trimmed` 必须为 `false`** —— 见 §4 的裁剪陷阱。

## 4. 裁剪陷阱（本轮新发现，必须先看）

`card_window.trim_rng_ledger` 超限时**只保留前 `head_lines` 行**（默认 `RNG_LEDGER_MAX_BYTES = 64 MiB`、
`RNG_LEDGER_HEAD_LINES = 200_000`），而**两抽发生在运行的后半段** ⇒ 一旦触发裁剪，两抽多半被切掉，
账本看起来「没有两抽」——那是**证据被裁没了**，不是行为不符。

实测速率（`E:/tmp/window/butch-run2/csharp/rng-stream.tsv`）：**151,877 行 / 11.9 MB / 21 s**
≈ **7.2k 行/s** ⇒ 行数上限先到，C# 侧运行 **超过约 28 s** 就有被裁风险。

处置：

1. 场景本身很短（kill + `sleep 500` + 收尾）；`max_retries=6` 最坏多 6 s。若**重试多**或**登录/settle 慢**，
   宁可按 16 只鸡（8 格 ×2）的小档重跑，也不要拿被裁的账本当证据；
2. 跑完立刻 `python -I -B tools/window/butch_ledger.py <账本> --side csharp --json <out>/csharp/two-draws.json`
   （见 §5），**在裁剪之前**跑；
3. `trimmed=true` ⇒ 该侧证据**作废**，在报告里写明「账本被裁，需重跑」，不得判 FAIL。

## 5. 判据命令（`tools/window/butch_ledger.py`，含 5 颗自检牙）

```powershell
python -I -B tools/window/butch_ledger.py <out>/csharp/rng-stream.tsv --side csharp --json <out>/csharp/two-draws.json
python -I -B tools/window/butch_ledger.py <out>/rust/rng-stream.tsv   --side rust   --json <out>/rust/two-draws.json
python -I -B tools/window/butch_ledger.py --selftest     # 5/5 GREEN
```

输出要点：`verdict`（`TWO_DRAWS_OK` / `TWO_DRAWS_FAIL`）、`draws.16.in_butch_fn`、`draws.201.in_butch_fn`、
`values.n10`/`values.meat`、`order_ok`、`sites`。

**必须按 site 过滤**：`max=201` 在真机账本里有 **206 条来自 `MonGen.cs:815`**（run2 实测），
只看 max 会把刷怪算成屠宰。Rust 侧对应站点今天是 `crates\world\src\world.rs:7756`（`max=16`）与 `:7757`
（`max=201`）——**行号会随重构漂**，`EXPECT_LINE` 只是参考值，判据以 `site` 命中屠宰函数为准。

自检牙（`--selftest`）：a 两抽成立（含 MonGen 干扰）、b 次序反 ⇒ FAIL、c `max=16` 两条 ⇒ FAIL、
d 只有 MonGen ⇒ FAIL、e Rust 侧同口径。**变异**：把 site 过滤去掉 ⇒ a/d/e 三条同时红
（说明「按 site 过滤」是承重逻辑，不是装饰）。

## 6. 这一轮要记进证据根的字段

| 字段 | 来源 | 用途 |
|---|---|---|
| `start_wall` / `end_wall` | `<side>/run.json` | 与 DB 时间列见证、账本时间段互相对齐 |
| `seed` | 两侧启动参数（`-Seed 42`） | 证明两侧同种子 |
| 场景 sha256 | `tools/probe/scenarios/butch_two_draws.json` | 证明判据文本没被改 |
| 账本 `lines` / `sha256` / `trimmed` | `card-window.json` 的 `sides.<side>.rng_log_trim` | 证「没被裁」且可复核 |
| `two-draws.json`（两侧） | §5 命令 | 判据的机器可读结论 |
| 637 伴随证据 | `frames.ndjson` 里 `ident=637` 至少 1 条 | 只作伴随，不作判据 |

## 7. 已知旧口径（别再照抄）

- 「637 不来 = 屠宰被拒」「`ClientGetButchItem` false ⇒ 无尸体/距离/速度门」——**已作废**（唯一早退是速度门）。
- 「`recog = MakeLong(尸体X, 尸体Y)`」——**已作废**（`PlayObject.Message.cs:830` 传 `(nParam1,nParam2,nParam3,wParam)`
  ⇒ `recog=charId`、`param=X`、`tag=Y`；场景用 `@last_kill_actor/@last_kill_x/@last_kill_y`）。
- `docs/window-exec-sheet-20261025.md:46` 仍写「缺场景击杀步 / ③ 待补：场景击杀步」——**该行已过期**
  （kill 步已落地并在实验轮跑通：`kill result=dead`、`matched_ident=34`）。该文件不在本卡写集，
  已在卡上向编排方登记，待其归口修改。

## 8. 未做

- **真机整轮未跑**（等窗口）；本文件与 `butch_ledger.py` 只保证「到那时不会因为解析器/裁剪/口径问题返工」。
- `rng_log_trim` 的 head-only 策略未改（改成「保留头 + 屠宰段」属 harness 改动，需另开卡）。
