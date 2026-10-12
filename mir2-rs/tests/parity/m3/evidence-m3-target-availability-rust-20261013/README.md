# evidence-m3-target-availability-rust-20261013 —— 目标可用性门的 **Rust 半接线**（两侧都强制）

- 卡：`openmir2-m3-attack-pair-fixture`（issue `76cc08b8`）／车道 `m3bag`／principal `openmir2-worker-4`。
- 来源：owner **2026-10-13 00:20**（`brief-m3-combat-skills.md` 顶部「先取窗跑，再补 Rust 侧的面」）
  + **2026-10-12 23:50** 第 ③ 条（`brief-m3-bag-equip.md` 顶部「加 fail-closed 门」）。
  承接上一片 `evidence-m3-target-availability-20261012`（已合 master `0f4e141b`）里**如实登记的边界**：
  「Rust 侧没有同形 dump ⇒ 这道门目前只在 C# 侧强制（只保护一半）」。
- 写集：`mir2-rs/tests/parity/m3/{run_attack_pair.ps1, server_state_gate.py}`（**零 `.rs`**；
  未动 `crates/**`/`bins/**`/`src/**`/`sql/**`/`MirClient`）。
- 依赖（**不在本片、归 m2-svr 车道**）：`mir2-rs/bins/game-svr/src/state_log.rs` 与
  `tools/m2-acceptance/start_rust_stack.ps1 -StateLog`（待审片 `4ba36368` / `7a41aebc`；其 commit message
  明示「夹具（`run_attack_pair.ps1`）侧接线归 m3bag」）。

## 0. 一句话结论

`server_state_gate.py` 的 **Rust 半**已在夹具侧接上：`run_attack_pair.ps1` 新增 `-RustStateLog`
（缺省读 `$env:MIR2_RNG_STATE_LOG` / `<RustStackLogDir>\rng-server-state.tsv`），并把原「`-Side rust`
只打 NOTE 放行」的 `else` 分支删除 —— **两侧同一个生产函数 `Invoke-M3TargetAvailabilityGate`**，
**任一侧缺 dump ⇒ RED（fail-closed）**。此前的边界（「门只保护 C# 一半」）关闭。

## 1. 本片交付

| 件 | 位置 | 作用 |
| --- | --- | --- |
| Rust 半状态 dump 入口 | `run_attack_pair.ps1 -RustStateLog`（缺省 `$env:MIR2_RNG_STATE_LOG` / `<RustStackLogDir>\rng-server-state.tsv`） | 与 C# 半 `-CsharpStateLog` 对称；喂 `server_state_gate.py` 的是 Rust 服务端权威 dump |
| 唯一口径生产函数 | `run_attack_pair.ps1::Invoke-M3TargetAvailabilityGate` | 解析路径（显式 → 环境 → 侧默认）+ **缺文件 ⇒ rc=1 `missing_state_log`** + 跑门；**C#/Rust 同一函数**，不允许任一侧静默放行 |
| 侧运行门 | `run_attack_pair.ps1`（原 `if ($Side -eq 'csharp') {…} else { NOTE }` 已删） | 两侧都要求服务端 dump：取最后 `-AttackCount` 行，攻方前方格须有非攻方、`!ghost && !death` 的玩家族对象；否则 RED 点名 k；并把尾段落 `server_state_segment.tsv` |
| 自检 ⑦c | `run_attack_pair.ps1 -SelfCheckNoStack` | 直接跑**生产函数**四向：`rust-good rc=0` / `rust-ghost rc=1` / `missing rc=1(missing_state_log)` / `csharp-good rc=0`；返回形态断言为结构化对象 |
| 门口径文档 | `server_state_gate.py` docstring | 输入从「C# 影子 hook」改为**两侧同形**（C# `StartupHook.cs::DumpActors` / Rust `bins/game-svr/src/state_log.rs`），并注明夹具侧由 `Invoke-M3TargetAvailabilityGate` 两侧强制 |

## 2. 判据与证据

| 文件 | 内容 | 结果 |
| --- | --- | --- |
| `01-baseline-selfcheck-nostack.txt` | 改前 `-SelfCheckNoStack` | `SELFTEST_RED GREEN`（无 ⑦c）——确认本片不引入既有项回归 |
| `02-selfcheck-nostack-green.txt` | 改后 `-SelfCheckNoStack` | `SELFTEST_RED GREEN`；`SELFTEST ⑦c … rust-good rc=0 / rust-ghost rc=1 / missing rc=1(missing_state_log) / csharp-good rc=0` |
| `03-redcheck-missing-not-failclosed.txt` | 改坏①：缺 dump 降级成放行 | `SELFTEST_RED RED: target_avail_missing_not_failclosed=0/missing_state_log`（**必红**） |
| `04-redcheck-rust-bypass.txt` | 改坏②：Rust 侧绕过门 | `SELFTEST_RED RED: target_avail_rust_ghost_rc=0`（**必红**） |
| `05-gate-on-real-rust-dump.txt` | m2-svr 真 Rust 产出喂本夹具门 | good ⇒ rc=0 `GREEN 5/5`；ghost-from-k4 ⇒ rc=1 点名 `#4(seq=4), #5(seq=5)` |
| `inputs-rust-good.txt` / `inputs-rust-ghost-from-k4.txt` | 上项输入（m2-svr `4ba36368` 证据的拷贝，LF 归一） | 5 行 `SERVER_STATE`，含 `id/name/x/y/dir/ghost/death/hp/maxhp/map` |
| `06-gate-fmt.txt` … `09-gate-msgcodegen.txt` | 四门禁 | 全 rc=0（见 `10-gates-summary.txt`） |

## 3. 与前一参数口径的关系（不降级）

- 门的**唯一判据实现**仍是 `server_state_gate.py`（本片只改其 docstring）；本片只补**接线**与**自检**。
- 上一片的 C# 行为**逐字节保持**：显式 `-CsharpStateLog` → 环境 → `E:\tmp\m3-csharp-hook\rng-server-state.tsv`，
  改由 `Invoke-M3TargetAvailabilityGate` 统一解析；缺 dump 仍 RED（消息改为同时点名两侧入口）。
- 容差仍是「允许 1 击起跑竞态」（`--tolerance 1`），与 `server_state_gate.py` 的 fail-closed 默认一致；
  **未放宽**任何判据。

## 4. 改坏必红（原始件）

| 改坏 | 期望 | 实测 |
| --- | --- | --- |
| 缺 dump 从 rc=1 改成 rc=0（旧「NOTE 放行」语义） | ⑦c 红 | `03`：`RED: target_avail_missing_not_failclosed=0/missing_state_log` |
| Rust 侧 `$rc=0`，只让 csharp 真跑 gate | ⑦c 红 | `04`：`RED: target_avail_rust_ghost_rc=0` |

两次改坏均在测后即时还原（`run_attack_pair.ps1` 恢复为绿态；`02` 的 GREEN 即还原后实跑）。

## 5. 边界 / 未做（如实登记）

- **真机两相复跑**（`-Side rust -RustStateLog <真 dump>` 的端到端）不在本片：本片纯离线。
  待 m2-svr 的 `4ba36368`/`7a41aebc` 合入 master 后，由 **m3cs** 在 L255 pin 的隔离两相里以
  `start_rust_stack.ps1 -StateLog <path>` + `run_attack_pair.ps1 -RustStateLog <path>` 实跑，届时这道门**两侧都咬**。
- 「触发 ghost 的精确 oracle 分支」仍由 m3cs/m2-svr 面推进；本片不含。
- 本片未起栈、未占端口（十端口纪律见 `E:/tmp/openmir2-watch/last-m3bag.md`）。

## 6. 残留物清单

- `mir2-rs/tests/parity/m3/sessions/csharp-20261012-23*/`（8 个上一实例的真机会话目录，未跟踪保留在盘，未入本包）。
- 本片未新建临时目录/进程；证据 hash 按仓库口径（LF 归一 / git blob）生成并用
  `tools/evidence-hash/verify_sha256sums.py --check-extra` 复核。
