# 证据包：M3 RNG 站点组同场景两相 trace 夹具（2026-10-13）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`；owner 派单 comment `6f19eacb`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- 报告：`../M3RNG站点组同场景folded映射与夹具报告-20261013.md`
- 夹具：`../rng-trace-scene/`（scenario + 前置门 + 驱动 + README）；折叠映射：`../rng-trace-probe/rng_consumer_groups.py` + `rng_ledger_diff.py`

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令 |
| `01-selftest-rng-ledger-diff.txt` | `rng_ledger_diff.py selftest` → **32/32 GREEN**（含 R-6 folded） |
| `02-selftest-scene-gate.txt` | `rng_scene_gate.py selftest` → **11/11 GREEN** |
| `03-selftest-attack-rng-ledger.txt` | `attack_rng_ledger.py selftest` → **32/32 GREEN**（回归：未改语义） |
| `04-selftest-nostack.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` → **12/12 GREEN**（LF 转录，脚本自己落盘） |
| `05-gate-identical-green.txt` | 场景前置门：两侧同场景 ⇒ **VERDICT GREEN** rc=0 |
| `06-gate-monster-drift-red.txt` | **改坏必红**：Rust 侧多一只怪 ⇒ `monsters-equal*` FAIL + rc=1 |
| `07-gate-missing-prereq.txt` | 缺场景描述件 ⇒ **PREREQ rc=2**（fail-closed） |
| `08-folded-equivalent-green.txt` | `groupdiff --fold --strict`：C# `[construct,spawn]` 折叠 == Rust `[spawn,spawn]` ⇒ **IDENTICAL** rc=0 |
| `09-mutation-folded-count-red.txt` | **改坏必红**：折叠计数不等 ⇒ `FOLDED_COUNT_DIFF` rc=1 |
| `10-mutation-folded-order-red.txt` | **改坏必红**：折叠后次序不等 ⇒ `SHARED_REL_ORDER_DIFF` rc=1 |
| `11-mutation-fold-off-still-red.txt` | 不传 `--fold` ⇒ `construct` vs `spawn` 仍 `GROUP_SET_DIFF`（折叠是显式开关，不是全局豁免） |
| `12-mutation-fold-table-red.txt` | **改坏必红**：`FOLD_RULES` 目标改成未知组 ⇒ `validate_folds()` 点名 |
| `13-baseline-nofold.txt` | 不同场景基线 `--strict` ⇒ `GROUP_SET_DIFF+SHARED_REL_ORDER_DIFF` rc=1 |
| `14-baseline-fold.{txt,json}` | 同基线 `--fold --strict` ⇒ 追加 `FOLDED_COUNT_DIFF`（spawn 折叠后 a=146545 b=0）⇒ **折叠不是豁免** |

## 判据读数（摘要）

- **R-6 folded 映射**：`FOLD_RULES = {construct: spawn}`；`--fold` 折叠后重算组集合 + **折叠计数** +
  共享组相对次序。等价 ⇒ `VERDICT IDENTICAL`（`FOLDED_COUNT_EQUAL`）；折叠计数不等 ⇒ `FOLDED_COUNT_DIFF`；
  次序不等 ⇒ `SHARED_REL_ORDER_DIFF`。缺省不传 `--fold` 时行为与既有逐字节相同。
- **场景一致性前置门**：`map`/`seed`/`start_pose` + `MonGen` 冻结件逐行多重集 + `Monster` 集合两侧同口径；
  任一漂移 ⇒ rc=1 点名；缺件/坏 JSON ⇒ rc=2 fail-closed。
- **基线实拍（不同场景，仅作对照）**：C# 非攻击 5,090,542 draws / 9 组 vs Rust 14 / 2 组；
  折叠后 C# `spawn = 82840(construct)+63705(spawn) = 146545`，Rust `spawn = 0` ⇒ `FOLDED_COUNT_DIFF`。
  这证明折叠不会把「Rust 缺消费者」洗成绿。

## 口径

- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / git blob）**记；复核：
  `python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013`
  以及 `… eolcheck …`（盘上字节 vs blob 的 EOL 一致性）。
- 合成 fixture 由生成脚本在临时目录创建（转录里的 `…\Temp\rngscene-evid-*` 路径为当次运行产物，不随包入库）；
  仓外基线原始件按 sha256 引用（见 `00-commands.txt`），未入库。
- 真机两相 trace **未跑**（等 owner 发窗口）；本包为**离线**证据，未起栈、未占端口。
