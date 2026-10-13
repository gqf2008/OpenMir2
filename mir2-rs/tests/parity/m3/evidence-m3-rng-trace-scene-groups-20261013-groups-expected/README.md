# evidence — M3 RNG 场景 DoD② 组清单机判（2026-10-13）

卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）。
状态：本片（DoD② 组清单机判 + reviewer-6 O-1 早退）已交 `patch` + `status: needs-review`；
base = merge-base(origin/master, HEAD) = `c127ae9a4`（前片 `409a0aa84` 已合入 master `c127ae9a4`）。
写集：`mir2-rs/tests/parity/m3/**`，零 `.rs`。

## 1. 缺口（可复现的假绿）

卡 DoD② 要求「`monster_ai`/`spawn`/`drop` 三组**两侧都出现**且共享组相对次序一致」。
`rng_trace_scenario.json` 也已声明 `trace.groups_expected` / `trace.min_monster_ai_draws`，但
**合并后的工具从未读这两项**：

- `rng_scene_gate.py::scenario_trace_checks` 只校验 `groups_expected` 里的**组名在册**（`scenario-groups-known`）；
- `rng_ledger_diff.py::cmd_groupdiff` 的 `GROUP_SET_DIFF` 只比**两侧彼此相等**，不比「声明里要求出现」；
- `min_monster_ai_draws` 在全仓**无任何引用**。

后果（`01`）：两侧流水都只含 `spawn` 时，`monster_ai` / `drop` **两侧都缺** ⇒ 组集合彼此相等 ⇒
`VERDICT IDENTICAL`、gate GREEN。DoD② 变成注解。

## 2. 修复（WIP）

`cmd_groupdiff --scenario <path>`：把 scenario 声明的组清单与**真实账本**对账，任一不成立 ⇒ fail-closed。

| 字段 | flag | 行为 |
|---|---|---|
| `trace.groups_expected` | `MISSING_EXPECTED_GROUPS`（点名组 + side） | rc=1 |
| `trace.min_monster_ai_draws`（或通用 `trace.min_group_draws`） | `MIN_GROUP_DRAWS`（`want`/`a`/`b`） | rc=1 |
| scenario 不存在 / 坏 JSON / 声明账本未知组 | — | PREREQ rc=2 |

- `run_rng_trace_pair.ps1 -SceneGroupDiff` 自动把同一 scenario 传给 `groupdiff`（单一入口同时覆盖
  DoD① 场景门与 DoD② 组清单）；`-GroupDiffOnly`（裸比较）**不传** ⇒ 缺省行为逐字节不变。
- scenario JSON 的 `trace.criterion` / `artifact.groupdiff` / `notes` 同步写明该机判。

## 3. 判据读数

- **修复前**（master `886f123b`）：`01` gate GREEN + `VERDICT IDENTICAL`（假绿）；`04` `--scenario` 选项不存在。
- **修复后**（WIP）：`02` `rc=1 VERDICT MISSING_EXPECTED_GROUPS+MIN_GROUP_DRAWS`（两侧都点名
  `monster_ai`/`drop`）；`03` 不带 `--scenario` 仍 `IDENTICAL`（向后兼容）。
- **自检**：`06` ledger **37/37**（+5）、`07` gate 22/22、`08` runner **29/29**（+2）、`09` attack 32/32、
  `10` scenario GREEN。
- **改坏必红**（`05`）：scratch 副本去掉两个 flag ⇒ ledger selftest **rc=1 RED**，点名
  `scenario-expected-missing-red` / `scenario-min-monster-ai-red`；同一假绿场景回到 `IDENTICAL`。
- **O-1（`11`，reviewer-6 carried Minor）**：半给流水 PREREQ 早退 —— 前 `checks=29 failed=1`（多跑一条
  `stream-seed-equal:a`），后 `checks=28 failed=1`（只留 `stream-pair-required`）；rc 始终 2（fail-closed）。
  gate selftest 的 `stream-pair-prereq-red` 同步加牙：半给流水的输出不得含 `stream-seed-equal:a`。

## 4. 牙齿（selftest 内置，非仅本证据）

- ledger：`scenario-expected-green` / `scenario-expected-missing-red` / `scenario-min-monster-ai-red` /
  `scenario-missing-prereq` / `scenario-absent-backward-compatible`。
- 无栈自检：`scene-groupdiff-expected-group-missing-red` / `scene-groupdiff-min-monster-ai-red`。

## 5. 复算

见 `00-commands.txt`；原始输出 `01`–`10`。合成场景/流水由 `E:/tmp/_m3bag_dod2_demo.py` 生成（仓外，
临时件，不进仓）。清单按仓库字节（LF 归一 / git blob）记。

## 6. 边界

`src/ sql/ MirClient/`、`mir2-rs/crates`、`mir2-rs/bins` **零改动**；本片只动
`rng-trace-probe/rng_ledger_diff.py`、`rng-trace-scene/{run_rng_trace_pair.ps1,README.md,rng_trace_scenario.json}`
与本证据目录。未起栈、未占端口。
