# butch 真机轮：账本恰两抽（卡 `openmir2-butch-two-draws-unblock`，tier R）

> 取数时点：2026-10-25 13:53–13:56 +08。证据根：`E:/tmp/window/butch-run/out`（本文件所有数字都从该根读出）。

## 1. 这一轮验什么

oracle `src/M2Server/Player/PlayObject.Operate.cs:702 ClientGetButchItem`：**成功 = RNG 账本恰两抽**

| 抽 | 位置 | 模数 | 值域 | 账本判定 |
|---|---|---|---|---|
| 1 | `Operate.cs:722` | `Random(16)` | `+5 ∈ [5,20]` | 屠宰函数内 `Next(max) max=16` **恰 1 条** |
| 2 | `Operate.cs:723` | `Random(201)` | `+100 ∈ [100,300]` | 屠宰函数内 `max=201` **恰 1 条**，且 `seq` 在 16 之后 |

`SM_BUTCH(637)`（`:749` 无条件发）**不是判据**，只作伴随证据；`SM_SKELETON(33)` 仅 `BodyLeathery<=0` 时发。

## 2. 怎么跑的

- 机器：`cargo/rustc/mir2-server` 全 0、仅 mysqld；测试端口全空；签 `stack-claim` → 跑 → `stack-release`。
- 基线：master `64f49e902`；窗口分支 `win/butch-run`；**Rust 侧二进制 = 本检出 `cargo build --release` 的产物**
  （profile 用 `-InnerArgs "-ServerExe <dev-1>/mir2-rs/target/release/mir2-server.exe"` 显式指定，避免用到别的车道的旧二进制）。
- 两侧同场景、同种子（`-Seed 42`）、同参数；C# 侧由 `start-csharp.ps1` 注入
  `DOTNET_STARTUP_HOOKS` + `MIR2_RNG_SEED` + `MIR2_RNG_LOG` + `MIR2_RNG_SITE=1`。
- 场景：仓库版 `butch_two_draws.json` 仍带 `blocked`（card_window 见 `blocked` 即 rc=2），本轮用
  **仅删 `blocked` 的窗口副本** `E:/tmp/window/butch-run/scenario.json`
  （sha256 `127ea1b191dbcf184618d6f85695691f7cdee1149c1fe8a7836abd030de15055`；
  `steps` / `params` / `placement` 与仓库版逐字相同，**`description` 因带「已解」状态文字而与仓库版不同**；
  `--check-scenario` rc=0）。
- 命令：

  ```powershell
  CONFORMANCE_PASSWORD=... python -I -B tools/conformance/card_window.py `
    --card openmir2-butch-two-draws-unblock `
    --profile E:/tmp/window/butch-run/profile.json `
    --scenario E:/tmp/window/butch-run/scenario.json `
    --rng-log --timeout-ms 60000 --out E:/tmp/window/butch-run/out
  ```

## 3. 结果（四个数）

### ① 账本没有被裁（`rng_log_trim.trimmed`）

| 侧 | trimmed | 行数 | 字节 | sha256（前 16 位） | 运行窗（`run.json`） |
|---|---|---|---|---|---|
| C# | **false** | 135,724 | 10,562,020 | `8d44c935316b3daf` | 13:53:52 → 13:54:06 |
| Rust | **false** | 106,251 | 5,745,855 | `683b4b9e181b62c6` | 13:55:12 → 13:55:24 |

两本账的原始体量与 `kept_*` 完全相等 ⇒ **一份都没被裁**，`changes_on_mutation` 类「证据被裁」风险未发生。

### ② 两抽（`tools/window/butch_ledger.py`，两侧 `TWO_DRAWS_OK`）

| 侧 | `max=16` | site | `max=201` | site | 次序 | 扣减量（`n10` / `meat`） |
|---|---|---|---|---|---|---|
| C# | 1 条（seq 119187） | `M2Server.Player.PlayObject.ClientGetButchItem@PlayObject.Operate.cs:722` | 1 条（seq 119188） | `…@PlayObject.Operate.cs:723` | 16 在前 ✓ | 13 / 131 |
| Rust | 1 条（seq 89066） | `crates\world\src\world.rs:7756` | 1 条（seq 89067） | `crates\world\src\world.rs:7757` | 16 在前 ✓ | 11 / 165 |

- 两侧 `max=201` 的**其它**条目（C# 104 条、Rust 12 条）全部来自 `MonGen.cs:815` / `mongen.rs:899,900`
  ⇒ **按 site 过滤**是这条判据成立的前提（工具 `--selftest` 牙 d 咬这一点）。
- 跨侧扣减量不同（13/131 vs 11/165）**不是失败**：两侧 RNG 流的消费点不同，判据是「每侧恰两抽 + site 归属 + 次序 + 值域」。
- 账本侧结论文件：`out/csharp/two-draws.json`、`out/rust/two-draws.json`。

### ③ 637（仅记录，不作判据）

两侧各 **1 条**：C# `seq=157 / t=12999ms / recog=1101538188 / param=330 / tag=330`；
Rust `seq=110 / t=10949ms / recog=605 / param=330 / tag=330`。

### ④ 字段映射与击杀

| 侧 | kill 结果 | 目标格 | swings / retries / waited | 死亡帧 | `CM_BUTCH` 发送 |
|---|---|---|---|---|---|
| C# | `dead` | `(x=329, y=330)` | 3 / 0 / 0 | `ident=34`（`SM_NOWDEATH`） | `recog=1013013390`（ActorId）、`param=329`、`tag=330` |
| Rust | `dead` | `(x=329, y=330)` | 3 / 0 / 0 | `ident=34` | `recog=199`（ActorId）、`param=329`、`tag=330` |

- `recog` 是**目标 ActorId**、`param/tag` 是**尸体格 X/Y**——与 `PlayObject.Message.cs:830` 的
  `ClientGetButchItem(nParam1, nParam2, nParam3, wParam)` 一致。
  **旧口径「recog=MakeLong(X,Y)」的否证依据**是：`run.json` 的发送记录里 `recog` = 目标 ActorId
  （C# 1013013390 / Rust 199），而 `MakeLong(329,330) = 21627209` **不等于任一 recog**；
  且两侧账本**两抽真的发生**（若 recog 塞的是坐标，`ActorMgr.Get` 必失败、两抽不会出现）。
  **不是**「账本那两行」——账本行里没有 recog 字段。
- `waited=0`：开局 32 只鸡里就有相邻目标，等待分支未触发（与 `openmir2-probe-track-target-adjacency` 的分析一致）。
- `SM_SKELETON(33)` 两侧 0 条（`BodyLeathery` 未归零），符合「33 只在 `BodyLeathery<=0` 时发」。

## 4. DB 面：两条差异，都给归属

`report.json` 的 `db.state = compared`，两侧快照有 **2 条**字段差异。**都不是本卡判据**，但必须记账。

### 4.1 `characters.Dir`：C# `0` / Rust `3`（PlayerId 6228）—— **新差异（存档面），跨场景复现**

| 事实 | 值 |
|---|---|
| 两侧 XY | `Map=0, CX=330, CY=330`（**都正确落库**，与实时位置一致） |
| C# `Dir` | **0** = `btDir`（`CM_BUTCH` 的 `series`；oracle `Operate.cs:736` 在位置支内无条件 `Dir = btDir`） |
| Rust `Dir` | **3** = **基线值**（基线 dump 里 winplay0 是 `B013 / (7,12) / Dir=3`） |
| 帧面 | Rust 的 `SM_BUTCH(637)` 携带 `Dir=0`，与 C# 相同 ⇒ **命令时刻方向是对的** |

判定：**不是「帧/实体未供给方向」**——帧上 `Dir` 两侧一致且为 0；差异只出现在**持久化**：
Rust 把**载入值**（3）写回了 `characters.Dir`，C# 写的是 butch 后的 `Dir`（0）。
因此**不能**归到 `openmir2-player-dir-supply`（那张保证的是 `dir_updated ⇒ e.ai.dir = bt_dir` 与帧/展示一致）。

**同一签名在别的场景也出现**：W1 的 `move_walk` 当时记过 `Dir 2 → 3`（本目录旧报告 `docs/window-evidence-backfill-20261025.md` §6.2 判为 flaky），
而基线同样是 `Dir=3` ⇒ 同一形态：**Rust 的 `characters.Dir` 落的是载入值**。
⇒ 归为**存档面通用差异**（不是 butch 专属），建议另开卡：定位需要带 `CM_BUTCH ... dir_updated=` 与服务端存档链路的日志，
候选机制两条——(a) `dir_updated` 在运行时未置位（但帧上 `Dir=0` 又提示置位过）、(b) 命令之后实体 `ai.dir` 被重置。

### 4.2 `characters_item.Dura`：C# `3993` / Rust `3989`（item 106）—— **已登记 T-266**

每刀磨损 `Random(5) + 2 - AddAbil.WeaponStrong`，两侧 RNG 流本就分叉；`T-266`
（卡 `openmir2-attack-weapon-wear-642`，tier R）**明写「真机窗口未采」**，本条即其真机样本。
按登记处理，**不重复判**。

## 5. 帧面对比是 UNDECIDABLE（与账本判据无关，但必须写清）

`report.json`：`verdict=UNDECIDABLE`、`frames.compared=75`。原因两类：

1. `unclassified_ident`：`32 / 34 / 44 / 637 / 656` 在产物里出现但 `rules.json` 无条目；
2. `pending_ignore_rule`：`ident 14` 的 ignore 规则仍 PENDING，且本次剔除两侧对不上（oracle=2 / rust=1，形态不同）。

⇒ **帧面判据尚不具备**（这是规则表覆盖问题，需另卡），但它**不影响**本卡判据（账本两抽）。
不得把这条 UNDECIDABLE 写成 PASS，也不得写成 FAIL。

## 6. 残留与复现

- 证据根 `E:/tmp/window/butch-run/out`（两侧 frames / run.json / db 快照 / rng-stream.tsv / two-draws.json / report.json）。
- 窗口临时物：`profile.json`（含 `-ServerExe`）、`scenario.json`（去 blocked 的副本）——不入库，路径已登记。
- 参考栈 `E:/MirServer` 未触碰（pid 14040/6808/7732/5832/22248/16280 与开窗前一致）；测试栈已停，端口无 LISTEN。
- 复现判据（无需重跑，直接读证据根）：

  ```powershell
  python -I -B tools/window/butch_ledger.py E:/tmp/window/butch-run/out/csharp/rng-stream.tsv --side csharp
  python -I -B tools/window/butch_ledger.py E:/tmp/window/butch-run/out/rust/rng-stream.tsv   --side rust
  ```
