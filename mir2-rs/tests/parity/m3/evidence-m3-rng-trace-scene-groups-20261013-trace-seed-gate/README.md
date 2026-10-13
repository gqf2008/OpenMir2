# 证据包：M3 RNG 场景「流水头同种子」制品级前置门（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- 报告：`../M3RNG场景流水头同种子制品级门收口报告-20261013.md`
- 夹具：`../rng-trace-scene/`（scenario + 前置门 + 驱动 + README）

## 这一片补的是什么

DoD① 要求「两侧 trace 必须来自同场景/同起点/**同种子**，不满足 ⇒ `groupdiff` fail-closed 拒比」。
此前门的 `seed` 检查只读**场景描述件**里的自报值 —— 描述件写 42、真实流水是别的种子时，门仍绿，
于是把**场景/种子差异**报成「实现差异」（正是 DoD① 要拦的误判）。

本片加**制品级**同种子门：给 `--a-stream`/`--b-stream` 时，核两份 trace **流水头**的
`INJECT_OK <seed>`（C# `rng-hook` 与 Rust `crates/shared/src/rng_trace.rs` 都写）：

- 缺该行 ⇒ 无法证明种子（fail-closed RED，点名 `stream-inject-ok:<side>`）；
- `!= scenario.seed` ⇒ RED（点名 `stream-seed-equal:<side>`）；
- 两侧都给出时再加 `stream-seed-equal:a-vs-b`；
- **未给流水 ⇒ 保持既有描述件级门行为**（向后兼容，历史命令与已入库证据仍可复算）。

`-SceneGroupDiff`（DoD① 单一入口）恒传两侧流水 ⇒ 制品级种子漂移在**拒比**阶段被拦（不跑 groupdiff、
不输出任何结论行）。`-GroupDiffOnly` 语义不变。

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令 |
| `01-scene-gate-selftest.txt` | `rng_scene_gate.py selftest` ⇒ **21/21 GREEN**（原 16 + 新 5） |
| `02-scenario-green.txt` | `rng_scene_gate.py scenario` ⇒ **VERDICT GREEN**（未改语义） |
| `03-gate-stream-seed-green.txt` | CLI：两侧流水头 `INJECT_OK 42` ⇒ **rc=0 GREEN**（含 `stream-seed-equal:a/b`） |
| `04-gate-stream-seed-mismatch-red.txt` | **改坏必红**：B 侧流水头 43、描述件自报 42 ⇒ **rc=1 RED**，点名 `stream-seed-equal:b` |
| `05-gate-no-stream-backward-compatible.txt` | 回归：不给流水 ⇒ rc=0，且输出无任何 `stream-*` 检查（向后兼容） |
| `06-nostack-selfcheck.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` ⇒ **27/27 GREEN**（原 23 + 新 4；LF 由脚本自身落盘） |
| `07-gate-only-stream-seed-green.txt` | `-GateOnly` 现场 CLI（带流水）⇒ rc=0 GREEN |
| `08-scene-groupdiff-stream-seed-refused.txt` | **DoD① 单一入口拒比**：制品级种子漂移下 `-SceneGroupDiff` ⇒ rc=1、`REFUSED`、且**无任何** `RNG_CONSUMER_GROUPDIFF` 结论行 |
| `09-regression.txt` | 回归：`rng_ledger_diff` 32/32、`attack_rng_ledger` 32/32、`fixture_matrix check` GREEN |
| `10-mutation-a-gate-ignores-stream-seed.txt` | **改坏必红 A**：`stream_inject_seed()` 恒返 42 ⇒ 门自检 **20/21**，`stream-seed-mismatch-red` FAIL |
| `11-mutation-b-core-not-passing-streams.txt` | **改坏必红 B**：单一入口不再把流水传给门 ⇒ 无栈自检 **26/27**，`scene-groupdiff-stream-seed-refused` FAIL（rc=1 但 `ranDiff=True`） |
| `12-boundary-yuan-rs.txt` | 边界：冻结面（`src/ sql/ MirClient/ mir2-rs/crates/ mir2-rs/bins/`）零改动 + 零 `.rs` |

两条改坏都在**仓外**临时树（`E:/tmp/m3bag-mut/parity-m3`，由仓库件复制）上做，仓库内文件未被污染。

## 复算

```sh
cd mir2-rs/tests/parity/m3
python -I -B rng-trace-scene/rng_scene_gate.py selftest          # 21/21
powershell -NoProfile -ExecutionPolicy Bypass -File rng-trace-scene/run_rng_trace_pair.ps1 -SelfCheckNoStack   # 27/27
python -I tools/evidence-hash/verify_sha256sums.py verify mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-trace-seed-gate/SHA256SUMS.txt --check-extra
```

## 残留物清单

- 仓外临时件：`E:/tmp/m3bag-gen-evidence.py`、`E:/tmp/m3bag-trace-seed-evidence/`（合成场景件/流水）、
  `E:/tmp/m3bag-mut/parity-m3/`（改坏临时树）—— 保留以备复算。
- worktree 内 8 个他线历史未跟踪目录 `sessions/csharp-20261012-23*`（非本片、未纳入、未清）。
- 本片未起栈、未占端口、未 merge、未自签 review、未动共享主检出。
