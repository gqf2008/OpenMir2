# evidence-m3-chain-gate-residuals-20261014

M3 夹具链深检门**残余收口片 #2**（离线、零起栈、零端口、零 `.rs`）。卡：
`openmir2-m3-fixture-chain-gate`（夹具链深检门 umbrella）。本片对应 owner 队列
（2026-10-14 00:2x/00:3x）第 1 条「⑧ 行会/攻城夹具 + **共享管线链深检门的残余 Minor**」。

tier：**L**（纯夹具/工具：3 个 `.py`；不触 RNG 抽数序列、时序、持久化落库、战斗数值
⇒ 等价 + 抽样冒烟即放行；卡级真机 DoD 不因本片闭合）。

## 为什么有它（同根因第 2、3 例）

上一片（`evidence-m3-anchor-token-class-20261014`，已合入 master `768b2bb6c`）修掉了
「三锚门 token 强度谓词漏红」这一类**门牙不全**。本片再收两条**同族**的链深检残余：

1. **零调用方的 fail-closed 门**（本卡已合入的 markers 前缀门是第 1 例）：
   `m3_pet_slave_registry_gate.py`（⑨-2/⑨-3 reviewer-5 M-1 收口片）在 `origin/master`
   上**没有任何 `.ps1`/`.py` 调用方** —— 只在它自己的证据 README 里手工跑。
   ⇒ 缺登记行 / 撞号 / 锚点不符会**静默通过整条链**（与卡 `openmir2-m3-fixture-marker-prefix-bug`
   的 RED 逃逸同一根因）。本片把它接入 `fixture_chain_gate.py` 的 `SHARED_CHECKS`
   （`pet_slave_registry_selftest` + `pet_slave_registry_check`），不看矩阵状态、独立常跑，
   并给链门 selftest 加**防回退牙**（被反注册 ⇒ 链门 RED）。
2. **恒真断言**：`marker_prefix_gate.py` 的 `guards-guard-only-ok` 是恒真项
   —— reviewer-6 在 `49993d1f`（`openmir2-m3-fixture-marker-prefix-bug` 的 O-2）点名。
   本片按 reviewer 建议改成**可失败向**（`guards-missing-real-guard-red`）。

## 逐条改法

### ① 接入宝宝登记门（`fixture_chain_gate.py`）

```python
("pet_slave_registry_selftest", "mir2-rs/tests/parity/m3/m3_pet_slave_registry_gate.py", ["selftest"]),
("pet_slave_registry_check",   "mir2-rs/tests/parity/m3/m3_pet_slave_registry_gate.py", ["check"]),
```

配牙（防回退）：

```python
_pet_gate = [e for _n, rel, e in SHARED_CHECKS if rel.endswith("m3_pet_slave_registry_gate.py")]
expect("宝宝登记门已接入共享管线（防回退）", ["selftest"] in _pet_gate and ["check"] in _pet_gate)
expect("主机门（.py）登记项 ≥10（… + 宝宝登记 各 selftest/check）", … >= 10)
```

`m3_pet_slave_registry_gate.py` 的模块 docstring 同步加「## 接入」段，说明它已常驻链门。

### ② O-2 恒真断言 → 可失败向（`marker_prefix_gate.py`）

旧（`origin/master`）：

```python
check("guards-guard-only-ok",
      guards_wired([m for m, _ in COMPARATORS] + list(GUARD_ONLY_MARKERS)))
```

喂进去的集合恰等于 `guards_wired` 内部算出的 `expected`（`set(COMPARATORS)|set(GUARD_ONLY_MARKERS)`）
⇒ 永远 `True`、不可能失败（见 `02` 的直接复算）。新：

```python
check("guards-missing-real-guard-red",
      bool(guard_markers) and not guards_wired(guard_markers[1:]))
```

从「**少一个真实扫到的牙**」向咬合 —— 与既有 `guards-extra-unregistered-red`（从「多」向咬合）
互补，合起来把 `guards_wired` 的**等值判据双向**钉住（`==` 若被误改成 `<=` 子集判定 ⇒ 新牙必红，
见 `09` MUT-A）。

## 读数（本片实跑，原始件见同目录）

| 项 | 读数 | 原始件 |
| --- | --- | --- |
| 登记前固定面 | `origin/master` 上 `m3_pet_slave_registry_gate` 的 `.ps1`/`.py` 调用方 = **无命中**（只有证据 README 手工读数） | `01` |
| O-2 恒真性 | 导入真实现复算：`guards_wired(COMPARATORS ∪ GUARD_ONLY_MARKERS) = True`（同构造恒真）、`guards_wired(COMPARATORS) = False` | `02` |
| markers 前缀门 | selftest **26/26 GREEN**（含新牙 `guards-missing-real-guard-red` ok）；check **GREEN**（7 生产牙含 1 guard-only 例外 + 6 比较器） | `03`/`04` |
| 宝宝登记门 | check **PET_SLAVE_REGISTRY GREEN**（5 行登记齐 + 锚点齐 + B 段编号唯一）；selftest **GREEN 5/5** | `05`/`06` |
| 链深检门自检 | `SELFTEST GREEN: fixture_chain_gate selftest 33/33`（新增 2 牙 ⇒ 31→33） | `07` |
| 链深检门 check | **FIXTURE_CHAIN GREEN: runner_ok=10 prereq_or_skip=0 red=0**；共享管线含 `pet_slave_registry_selftest/check` 两条 rc=0 | `08` |
| 外部改坏必红 | MUT-A（`==`→`<=`）：旧牙 `guards-extra-unregistered-red` 仍 ok、新牙 **FAIL**、`selftest 25/26 rc=1`；MUT-B（反注册宝宝门）：链门 **RED 31/33 rc=1**；真身 sha256 前后一致（`9229b205…` / `28b126a5…`） | `09` |
| 四门禁 | fmt rc=0 / clippy rc=0(-D warnings) / 101 suites **1200 passed 0 failed** 22 ignored / msgcodegen 604 rc=0 | `10`–`13` |

## 边界

见 `14-boundary.txt`：base = `origin/master` `768b2bb6c4b81e774a80a2ad27fda0cccdb1a12c`；
净差量全在 `mir2-rs/tests/parity/m3/**`（3 个 gate 工具 + 本证据目录）；
冻结面 `src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage mir2-rs/bins` = **空**；
`*.rs` 计数 = **0**。

## 残留（如实登记，勿重复）

- 卡级真机 DoD（⑧-6/⑧-10/⑨-5 各 READY 夹具 `run_*.ps1` 两侧同序列 diff=0）仍需
  `openmir2-m3-clock-injection` + owner 独占窗口；本片不涉。
- `m3_whitelist_registry_audit.py`（纯审计、非 fail-closed 门）仍**零调用方** —— 其
  `duplicates=[42]`（T-42 撞号）是已路由 owner 的历史问题，本片**不碰**（同上一片口径）。
- `check_window_batch.py` / `compare_dedicated_streams.py` 等为窗口期辅助件，不在「在场却坏」
  的 fail-closed 门之列，未登记。
