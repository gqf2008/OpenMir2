# M3 RNG 站点组：scenario 声明的 folded 映射 / 组清单与代码真源同口径（M-1 sibling 机判）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`；base = master `9edfe4b72`
- 证据包：`mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-scenariofold/`
- 写集：`mir2-rs/tests/parity/m3/**`；**零 `.rs` / 零 `crates` / 零 `bins` / 零 `src` / 零 `sql` / 零 `MirClient`**
- 纪律：未起栈、未占端口（仅共享 3306 在听）、未 merge、未自签。

## 1. 背景：这条是 M-1 的 sibling

M-1 的 finding（已由前片 `85919bcd` 收口）是：`rng_trace_scenario.json` 的 `trace.runner` 文档里写了
`-PinScene`，而 `run_rng_trace_pair.ps1` 的 `param()` 没声明它 ⇒ 照文档执行会在参数绑定阶段报错，
永远到不了 `-Go` 的 fail-closed 分支。那是 **doc-vs-code 漂移**，owner 要求「补机判 + 改坏必红」。

同一漂移面还有一处**未被覆盖**：`rng_trace_scenario.json` 的

```json
"trace": { "fold": { "construct": "spawn" }, "groups_expected": ["monster_ai", "spawn", "drop"] }
```

是人对夹具的**声明**；而真正的 folded 映射真源是代码
`rng-trace-probe/rng_consumer_groups.py :: FOLD_RULES = {"construct": "spawn"}`、组名真源是同文件的
`KNOWN_GROUPS`。门（`rng_scene_gate.py`）与比较器（`rng_ledger_diff.py groupdiff --fold`）之前**只读代码**，
**从不校验 scenario 的声明** ⇒ 一旦代码 `FOLD_RULES`/`KNOWN_GROUPS` 变化（或 scenario 手抄错），
`groupdiff --fold` 实跑的映射与 scenario 上写给人看的映射会**静默分叉**，没有任何机判会红。

这不是实现缺口，是**仪器/口径完整性**缺口（与 M-1 同族）。按 AGENTS §3「可判的技术问题自己判完」，
本片把它判掉。

## 2. 做了什么

1. **门直接比代码真源（不手抄常量）**：`rng_scene_gate.py` 把 `../rng-trace-probe` 加入 `sys.path` 后
   `import rng_consumer_groups as rcg`，新增 6 项 scenario 级检查：

   | 检查 | 口径 |
   |---|---|
   | `scenario-trace-present` | scenario 必须有 `trace` 对象 |
   | `scenario-fold-code-valid` | `rcg.validate_folds()` 为空（代码折叠表自洽：已知组→已知组、源≠目标） |
   | `scenario-fold-declared` | `trace.fold` 必须**显式声明**（缺 ⇒ 红，不静默绿） |
   | `scenario-fold-matches-code` | `trace.fold` **逐项等于** `dict(rcg.FOLD_RULES)` |
   | `scenario-groups-declared` | `trace.groups_expected` 必须显式声明 |
   | `scenario-groups-known` | 每个组名 ∈ `rcg.KNOWN_GROUPS`（未知组 ⇒ 红并点名） |

   这 6 项同时进 `gate`（窗口 `-GateOnly` 的流程）与新增的离线 `scenario` 模式。

2. **新增离线 `scenario` 子命令**：`python -I rng_scene_gate.py scenario --scenario <path>` 只校验
   scenario 件自身（canon/字段 + 上表 6 项），不需要两侧描述件 ⇒ **无栈自检能在离线锁住已入库的
   `rng_trace_scenario.json`**；退出码 0/1/2 与 `gate` 同口径。

3. **无栈自检 14 → 17 项**（runner）：
   `scenario-fold-group-consistency-green`（已入库 scenario 与代码同口径）、
   `scenario-fold-drift-red`（改 fold ⇒ 必红并点名 `scenario-fold-matches-code`）、
   `scenario-groups-unknown-red`（塞未知组 ⇒ 必红并点名 `scenario-groups-known`）；
   场景门 selftest 11 → **16/16**（新增 scenario 模式正/负样本 + 折叠漂移 / 缺声明 / 未知组三条改坏必红）。

4. **合成夹具同步**：runner 自检与场景门 selftest 里造的合成 scenario 都补上 `trace.fold`/`groups_expected`
   并与真源同口径。这保证「正向样本仍绿」不是靠新检查被跳过，而是声明本身合规。

5. **文档**：`rng-trace-scene/README.md` §2 检查表补 6 行、§3 补「scenario 声明的 fold 也被机判」、
   §5 自检项 12→17、§6 命令补 `scenario` 模式与 16/16。

## 3. 判据与读数（原始件见证据包）

| 判据 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 |
|---|---|---|
| 已入库 scenario 与代码同口径 | `rng_scene_gate.py scenario --scenario rng-trace-scene/rng_trace_scenario.json` | `VERDICT GREEN`（checks=8 failed=0）rc=0 |
| 场景门 selftest | `rng_scene_gate.py selftest` | `SELFTEST ok=16/16` rc=0 |
| runner 无栈自检 | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | `SELFTEST ok=17/17 failures=` rc=0 |
| 改坏① fold 漂移 | `scenario --scenario <copies>/fold-drift.json` | `scenario-fold-matches-code FAIL` rc=1 |
| 改坏② 未知组 | `scenario --scenario <copies>/groups-unknown.json` | `scenario-groups-known FAIL (['no_such_group'])` rc=1 |
| 改坏③ 缺 fold 声明 | `scenario --scenario <copies>/fold-missing.json` | `scenario-fold-declared FAIL` rc=1 |
| 回归 R-6 账本 | `rng_ledger_diff.py selftest` | 32/32 GREEN |
| 回归攻击账本 | `attack_rng_ledger.py selftest` | 32/32 GREEN |
| 四门禁 | fmt / clippy / test / msgcodegen | fmt0 clippy0 **95 suites 945 passed 0 failed 21 ignored** / 604 |

改坏副本生成在 `E:/tmp/m3bag-scenariofold/`（只改被测字段），被审 worktree 未被污染；
runner 自检内的三条改坏必红是内置复现。

## 4. 与 R-6 的关系（为什么这不只是文档洁癖）

owner 对 R-6 的裁决原文是「C# `construct` 折叠到 Rust `spawn` 后，**折叠计数与相对次序必须相等**；
**只写白名单不校验不算达成**」。本片把「校验」从**运行时数据**（`groupdiff --fold` 的计数/次序）
extend 到**声明层**（scenario 的 `trace.fold` 必须就是 `groupdiff` 将要用的那份映射）——
否则「机判等价」所依据的映射本身可能与文档分叉。这是 R-6「必须机判」的闭合，不是新口径。

## 5. 未做 / 边界（如实登记）

- 真机同场景（含怪）两相 trace + `groupdiff --fold --strict` 仍需 (a) owner 独占窗口
  (b) 生产者驱动承接卡（两侧 `<side>-scene.json` + `<side>-rng-stream.tsv`；写集
  `crates/world`/`bins/**`/`src/**` 超出本卡禁令）。本片**不改变**该状态：real-run 对
  `-Go`/`-PinScene` 仍 fail-closed `exit 2`。
- 本片未起栈、未占端口；四门禁用隔离 `CARGO_TARGET_DIR=E:/tmp/m3bag-target`、`--offline`。
