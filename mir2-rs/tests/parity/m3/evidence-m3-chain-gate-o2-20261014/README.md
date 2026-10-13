# evidence-m3-chain-gate-o2-20261014

卡 `openmir2-m3-fixture-chain-gate`（夹具族共享管线 / 常跑门 umbrella）的**残余收口片**：
关闭 reviewer-6 对上一片（`patch f0b93518` / head `b797711cd`，已合入 master `c517ce4fc`）
留下的两条非阻塞 observation。**离线、零起栈、零端口、零 `.rs`**。

tier：**L**（纯夹具/工具/文档；不触 RNG 抽数序列/随机播种、不触时序、不落库、不改战斗数值
⇒ 等价 + 抽样冒烟即放行）。

## 为什么有它（reviewer-6 的两条 observation）

| id | 原文 | 本片处置 |
| --- | --- | --- |
| **O-2** | 新牙 `guards-missing-real-guard-red` 依赖 `guard_markers[1:]` 为非全集的真子集（由先行的 `real-guards-wired` 已绿保证）；当前 `scan_guards` 去重且确定，成立。若将来 scan 产出顺序/重复变化，需一并复核。 | **硬化**：改走 `_drop_one_guard()`（先 `set` 去重再排序，切一刀必为真子集）⇒ 同一 marker 在 `run_pair.ps1` 重复出现（合法）时**不再假红**。 |
| **O-1** | `m3_whitelist_registry_audit.py`（纯审计、非 fail-closed 门）仍**零调用方**；其 `T-42` 撞号是已路由 owner 的历史问题。 | **部分收口**：把该门接入 `fixture_chain_gate.SHARED_CHECKS` 的 **`selftest` 向** + 防回退牙（不再零调用方）。见下方「未接 `--strict`」。 |

## 逐条改法

### ① O-2：`marker_prefix_gate.py` 的「少一个真实牙」向不再对重复标记假红

旧实现（`patch f0b93518`）：

```python
check("guards-missing-real-guard-red",
      bool(guard_markers) and not guards_wired(guard_markers[1:]))
```

`guard_markers` 是 `scan_guards()` 的**出现序**列表；`[1:]` 只切掉**首个出现**。
若同一 marker 在 `run_pair.ps1` 出现两次（同一族的 `Assert-M3MarkerPrefix` 写在两处是合法的），
`set(markers[1:])` 仍等于全集 ⇒ 该牙**假红**，把合法改动打红。

本片新增纯函数：

```python
def _drop_one_guard(guard_markers):
    return sorted(set(guard_markers))[1:]
```

牙改为 `bool(_drop_one_guard(...)) and not guards_wired(_drop_one_guard(...))`，
并新增两颗 `selftest` 牙：

* `drop-one-guard-dedup-proper-subset`：`_drop_one_guard(["A","A","B","C"]) == ["B","C"]`（重复标记下仍恰少一个）；
* `naive-slice-not-proper-subset-under-dup`：**根因复现** —— `set(["A","A","B","C"][1:]) == set(["A","A","B","C"])` 为真。

`marker_prefix_gate` selftest：**26 → 28**。

### ② O-1（部分）：把零调用方的白名单登记号审计门接入共享管线

`m3_whitelist_registry_audit.py`（卡 `openmir2-m3-whitelist-registry-integrity` 交付）此前
**无任何调用方**（`04` 在 `origin/master` 上复现：命中全是自身 docstring / 兄弟门的注释），
只在证据 README 里手工跑。⇒ 与 markers 前缀门 / 宝宝登记门同类的「在场却坏无声」隐患。

本片把它接入 `fixture_chain_gate.py` 的 `SHARED_CHECKS`（`whitelist_registry_selftest`，
不看矩阵状态、独立常跑；在场却坏 ⇒ 恒 RED），并加一颗防回退牙 + 把「主机门（.py）登记项」
下界 `≥10` 收紧为 `≥11`。`fixture_chain_gate` selftest：**33 → 34**。

### ⚠ 未接 `audit --strict`（如实登记，不是遗漏）

`whitelist.md` 现存**两行 `| T-42 |`**（⑤-2 商人 tick vs ⑥-5 任务夹具）⇒ `audit --strict`
现为 **RED**（`13` 如实留档）。该撞号是 **owner 已路由**的跨车道登记号重编：
报告 `M3白名单登记号审计与T-40订正报告-20261013.md` §2 建议把 **⑤-2 那行编到 T-48**，
但 ⑤-2 的证据 README 在 `mir2-rs/tests/parity/world/**`（本车道 brief 明列禁动写集）
⇒ 本片**不单方面重编号**。等 owner 裁决重编后，把 `audit --strict` 补登记为链门的 `check`
（届时 `fixture_chain_gate.py` 该段注释一并更新）。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| 接入前零调用方 | `origin/master` 上无 `.ps1`/链门调用方（命中全为自身/兄弟门注释） | `04` |
| marker 门 | selftest **28/28 GREEN**；check **GREEN** | `10` / `11` |
| 白名单审计门 | selftest **GREEN 9/9**；`audit --strict` **RED 2**（T-42，owner 路由，如实留档） | `12` / `13` |
| 链深检门 | selftest **34/34 GREEN**；**FIXTURE_CHAIN GREEN runner_ok=10 prereq_or_skip=0 red=0**（共享管线含 `whitelist_registry_selftest` rc=0） | `14` / `15` |
| 外部改坏必红 | **MUT-A**（`_drop_one_guard` 退化回 `sorted(...)` 不去重）⇒ marker selftest **RED 27/28** rc=1；**MUT-B**（从 `SHARED_CHECKS` 反注册白名单审计门）⇒ 链门 **RED 32/34** rc=1 | `01` / `02` |
| 真身未被改坏 | `marker_prefix_gate.py` / `fixture_chain_gate.py` 改坏/还原前后 sha256 一致 | `00` / `03` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen **604** rc=0 | `20`–`23` |
| 边界 | 仅 3 个 `.py` + 本证据目录；`*.rs`/`*.toml`=0，冻结面空 | `30` |

## 边界

* 写集：`mir2-rs/tests/parity/m3/{marker_prefix_gate.py,fixture_chain_gate.py,m3_whitelist_registry_audit.py}` + 本证据目录。
* `src/`、`sql/`、`MirClient/`、`mir2-rs/crates/**`、`mir2-rs/bins/**`、`src/BotSrv/**` 零改动；零 `.rs`。
* **只读 / 零端口**：只跑各件自带的无栈自检；未起栈、未占端口。

## 残留（如实登记，勿重复）

1. **T-42 撞号**（⑤-2 vs ⑥-5）与其 `audit --strict` fail-closed 接线：**owner 已路由**，
   需 `mir2-rs/tests/parity/world/**` 写集（非本车道）⇒ 本片只接 `selftest` 向，未接 `--strict`。
2. 卡级真机 DoD（⑧-6/⑧-10/⑨-5 各 READY 夹具 `run_*.ps1` 两侧同序列 diff=0）仍需
   `openmir2-m3-clock-injection` + owner 独占窗口；本片不涉、不据此收卡。
