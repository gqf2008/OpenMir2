# 证据包：M3 Claim B —— 全服 RNG 消费者结构一致性基线（2026-10-13）

- 卡：`openmir2-rng-consumer-structure-parity`（owner 派单 comment `d1135a73`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-consumers`
- 报告：`../M3全服RNG消费者结构一致性基线报告-20261013.md`

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令（自检 / 绿红对照 / 基线 / 复核） |
| `01-selftest-rng-ledger-diff.txt` | `rng_ledger_diff.py selftest` → **20/20 GREEN** |
| `02-selftest-attack-rng-ledger.txt` | `attack_rng_ledger.py selftest` → **31/31 GREEN** |
| `03-mutation-table-drift.txt` | **改坏必红**：删 `drop/csharp` 行 ⇒ `UNCLASSIFIED` + `--require-closed` rc=1 |
| `04-mutation-order.txt` | **改坏必红**：共享组换位 ⇒ `SHARED_REL_ORDER_DIFF` rc=1（index=1） |
| `05-mutation-extra-draw.txt` | **改坏必红**：同组多抽一次 ⇒ 共享组序列长度不等 rc=1 |
| `06-mutation-missing-group.txt` | **改坏必红**：只在 B 出现的组 ⇒ `GROUP_SET_DIFF` rc=1 |
| `07-selftest-groups-clean-green.txt` | 合成干净 trace + 完整表 ⇒ `GROUP_COVERAGE closed` rc=0 |
| `10-baseline-csharp-groups.{txt,json}` | C# 活栈（F-1 权威件）分组读数 |
| `11-baseline-rust-groups.{txt,json}` | Rust 活栈分组读数 |
| `12-baseline-groupdiff.{txt,json}` | 两侧组集合 + 相对次序 diff（当前基线） |
| `fixtures/syn-*.tsv` | 合成 trace（干净 / 未归类 / 换序 / 多抽 / 缺组） |
| `fixtures/consumer_groups.drop-removed.tsv` | 故意改坏的组表（删 `drop/csharp` 行） |

## 判据读数（摘要）

- 两侧自检：`20/20`、`31/31` GREEN；组表 `validate()` 空。
- 合成 fixture：干净 ⇒ 绿；四类改坏（表漂移 / 换序 / 多抽 / 缺组）⇒ 全红且定位。
- 基线（现存唯一一对同种子两侧活栈 trace）：C# 非攻击 **5,090,542** draws / 9 组 vs Rust **14** draws / 2 组；
  **7 组只在 C#**（`monster_ai`/`merchant`/`drop`/`construct`/`spawn`/`npc_ai`/`revive`，合计 5,090,528 draws）；
  共享组 `[homemap, startup]` 的相对次序 index=1 分歧。**两侧未归类均为 0**（表对现存 trace 全覆盖）。
- **次序判据的诚实边界**：两侧 trace 是**不同场景** ⇒ 组集合差可信、次序差不可作实现缺陷判据（详见报告 §2.2 警告）。

## 口径

- `SHA256SUMS.txt` 按**仓库字节（LF 归一 / git blob）**记；复核：
  `python -I tools/evidence-hash/verify_sha256sums.py --check-extra mir2-rs/tests/parity/m3/evidence-m3-rng-consumer-groups-20261013`。
- 仓外原始件（`E:/tmp/m3cs-*`）未入库，按 sha256 引用（报告 §2/§6）。
