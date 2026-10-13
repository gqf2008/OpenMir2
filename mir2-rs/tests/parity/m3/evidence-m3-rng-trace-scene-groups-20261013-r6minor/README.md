# 证据包：R-6 folded Minor 收口（reviewer-6 residual Minor）

- **卡**：`openmir2-m3-rng-trace-scene-groups`
- **base**：`master` = `7a654200`（本卡离线片 `b826065c` 已合入该 master）
- **片性质**：**不改判据语义**的小收口——把 `rng_consumer_groups.folded_counts()` 从
  「未被调用的死辅助函数」变成 `cmd_groupdiff --fold` 的**唯一折叠计数路径**。
- **不是新判据**：本片**不新增 selftest 用例**，`rng_ledger_diff.py selftest` 仍 **32/32**；
  既有 `folded-count-red` / `folded-order-red` 用例即覆盖该辅助函数（见 `02-` 的改坏必红）。

## 背景（reviewer-6 residual Minor）

reviewer-6 在 `b66547ad4c63`（独立第二意见，`covered_by` reviewer-5 的 approve `c9e4b11a`）留档：

> `rng_consumer_groups.folded_counts()` 在本片 `groupdiff` 里**未被调用**
> （`cmd_groupdiff` 直接对 `folded(a)/folded(b)` 取 `Counter`）⇒ 死辅助函数；无害，
> 建议随下一片删除或补一调用点。

本片取**补一调用点**：`cmd_groupdiff` 的 `--fold` 分支改为

```
ca = rcg.folded_counts(collections.Counter(raw_a))
cb = rcg.folded_counts(collections.Counter(raw_b))
```

（`raw_a/raw_b` = 折叠前的组序列）。折叠后的**序列**仍保留给相对次序判据；
`--fold` 缺省关时走原分支，行为逐字节不变。

## 判据读数

| 项 | 命令 | 读数 | 件 |
|---|---|---|---|
| 工具自检（wiring 后不变） | `rng_ledger_diff.py selftest` | **32/32 GREEN** | `01-` |
| **改坏必红**（辅助函数恒等化 ⇒ 必红） | 运行时 `rcg.folded_counts := identity` 后跑 selftest | **RED rc=1，2/32 未过**（`folded-equivalent-green` / `folded-order-red`） | `02-` |
| 场景门回归 | `rng_scene_gate.py selftest` | **11/11 GREEN** | `03-` |
| 攻击账本回归 | `attack_rng_ledger.py selftest` | **32/32 GREEN** | `04-` |
| 无栈自检（零端口） | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | **12/12 GREEN** | `05-` |
| 边界 | `git diff 7a654200` | 非证据改动仅 `rng_ledger_diff.py`（+8/−1）；`src/sql/MirClient/crates/bins` 空 | `06-` |

**改坏必红的含义**：把 `folded_counts()` 换成恒等（不再做 `construct→spawn` 折叠计数）后，
`folded-equivalent-green` 用例直接翻红（`construct` 留在 A 侧 ⇒ `GROUP_SET_DIFF`；
`FOLDED_COUNT_DIFF {construct:{a:1,b:0}, spawn:{a:1,b:2}}`）⇒ 证明本片的 `groupdiff` 折叠计数
**确实走该辅助函数**，而不是就地 `Counter`。该 monkeypatch 只存在于运行时，**不落盘**，
跑完即恢复（`02-` 的完整输出可复现）。

## 与主窗口片的关系

本片**不含真机两相 trace**。本卡 DoD 的真机半边（同场景含怪两相 trace + `groupdiff --fold --strict`）
仍等 owner 发独占窗口，与 ④ 真机 100 杀同一批次；窗口未发前不起栈、不占端口。

## 复核

```
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-r6minor
python -I tools/evidence-hash/verify_sha256sums.py eolcheck \
  mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-r6minor
```

清单按**仓库字节（LF / git blob）**口径记；本包文本一律 LF 写出。
