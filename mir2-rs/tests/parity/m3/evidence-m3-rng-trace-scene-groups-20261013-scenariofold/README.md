# 证据包：scenario 声明的 folded 映射 / 组清单 与代码真源同口径（M-1 sibling 机判）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- base = master `9edfe4b72`；本片零 `.rs`，改动仅在
  `rng-trace-scene/{rng_scene_gate.py, run_rng_trace_pair.ps1, README.md}` + 本证据包 + 报告 md
  （夹具写集 `mir2-rs/tests/parity/m3/**`）
- 上片的离线 ①②③ + R-6 minor + M-1 收口均已双签合入 master（`15510a47` / `7a654200` / `65602bf86`）；
  本片是**独立新片**，不移动任何已合并的 `reviewed_head`。

## 这条 finding 是什么（M-1 的 sibling：doc-vs-code 漂移）

`rng_trace_scenario.json` 是对夹具的**声明**，其中 `trace.fold`（`{construct: spawn}`）与
`trace.groups_expected`（`["monster_ai","spawn","drop"]`）此前**只是文档**：

- 真正的 folded 映射真源在代码 `rng-trace-probe/rng_consumer_groups.py :: FOLD_RULES`；
  组名真源在同文件的 `KNOWN_GROUPS`；
- 门（`rng_scene_gate.py`）与比较器（`rng_ledger_diff.py groupdiff --fold`）只用**代码**的映射；
- ⇒ 若有人改了代码 `FOLD_RULES`（或 scenario 手抄错了），`scenario` 上写的与实跑用的**不是一回事**，
  而没有任何机判会红。这与 M-1（`trace.runner` 写了脚本未声明的 `-PinScene`）是**同一族 doc-vs-code
  漂移**——owner 已为 M-1 要求「机判 + 改坏必红」，本片把同一条牙延伸到 folded 映射 / 组清单声明。

## 本片做了什么

1. **门 / scenario 模式直接 import 代码真源比较**（不手抄常量）：
   `rng_scene_gate.py` 把 `rng-trace-probe/rng_consumer_groups.py` 加入 `sys.path` 后 `import`，新增 6 项检查——
   - `scenario-trace-present`：scenario 必须有 `trace` 对象；
   - `scenario-fold-code-valid`：代码侧 `FOLD_RULES` 自洽（已知组→已知组、源≠目标）；
   - `scenario-fold-declared`：`trace.fold` 必须**显式声明**（缺 ⇒ 红，不许静默绿）；
   - `scenario-fold-matches-code`：`trace.fold` **逐项等于** `rcg.FOLD_RULES`；
   - `scenario-groups-declared`：`trace.groups_expected` 必须显式声明；
   - `scenario-groups-known`：其中每个组名都属于 `rcg.KNOWN_GROUPS`（未知组 ⇒ 红并点名）。
2. **新增离线 `scenario` 子命令**（`rng_scene_gate.py scenario --scenario <path>`）：只校验 scenario
   件自身（canon/字段 + 上述 6 项），不需要两侧描述件 ⇒ 无栈自检可在**离线**上锁住已入库的
   `rng_trace_scenario.json`（退出码 0/1/2 与 `gate` 同口径）。
3. **无栈自检扩到 17 项**（`-SelfCheckNoStack`）：新增
   `scenario-fold-group-consistency-green` / `scenario-fold-drift-red` / `scenario-groups-unknown-red`；
   另把场景门 selftest 由 11/11 扩到 **16/16**（新增 scenario 模式正/负样本 + 折叠漂移 / 缺声明 / 未知组 3 条改坏必红）。
4. **合成夹具同步**：自检/场景门 selftest 里造的合成 scenario 一并补上 `trace.fold`/`groups_expected`，
   与真源同口径（否则新检查会把合成正向样本判红——这正是牙在咬）。

## 判据与读数

| 判据 | 命令（cwd = `mir2-rs/tests/parity/m3`） | 读数 | 件 |
|---|---|---|---|
| 已入库 scenario 与代码同口径 | `python -I -B rng-trace-scene/rng_scene_gate.py scenario --scenario rng-trace-scene/rng_trace_scenario.json` | `VERDICT GREEN`（checks=8 failed=0）rc=0 | `03-` |
| 场景门 selftest | `python -I -B rng-trace-scene/rng_scene_gate.py selftest` | `SELFTEST ok=16/16` rc=0 | `02-` |
| 无栈自检（runner 级） | `powershell -File rng-trace-scene/run_rng_trace_pair.ps1 -SelfCheckNoStack` | `SELFTEST ok=17/17 failures=` rc=0 | `01-` |
| 改坏必红①：fold 声明漂移 | `scenario --scenario <copies>/fold-drift.json` | `scenario-fold-matches-code FAIL` rc=1 | `04-` |
| 改坏必红②：groups 未知组 | `scenario --scenario <copies>/groups-unknown.json` | `scenario-groups-known FAIL (['no_such_group'])` rc=1 | `05-` |
| 改坏必红③：fold 缺声明 | `scenario --scenario <copies>/fold-missing.json` | `scenario-fold-declared FAIL` rc=1 | `06-` |
| 回归：R-6 folded 账本 | `python -I -B rng-trace-probe/rng_ledger_diff.py selftest` | 32/32 GREEN | `07-` |
| 回归：攻击账本 | `python -I -B attack_rng_ledger.py selftest` | 32/32 GREEN | `08-` |
| 四门禁 | fmt / clippy / test / msgcodegen | fmt0 clippy0 **95 suites 945 passed 0 failed 21 ignored** / 604 | `09-` |
| 边界 | `git diff --name-only 9edfe4b72 [-- src sql MirClient mir2-rs/crates mir2-rs/bins]` | 仅 `tests/parity/m3/**`；分类面**空** | `10-` |

**改坏必红原始件**（`04-`/`05-`/`06-`）用的是 `E:/tmp/m3bag-scenariofold/` 下的副本（只改被测字段），
被审 worktree 未被污染；runner 自检里的三条改坏必红（`01-`）是**内置**复现。

## 件清单

| 件 | 内容 |
|---|---|
| `00-commands.txt` | 全部命令 + 环境 + 复现脚本 |
| `01-selftest-nostack.txt` | runner 无栈自检 17/17（含 3 条新项 + 场景门嵌套 16/16） |
| `02-selftest-scene-gate.txt` | 场景门 selftest 16/16 |
| `03-scenario-mode-green.txt` | `scenario` 模式对已入库文件的 8 项 GREEN |
| `04-mutation-fold-drift-red.txt` | 外部改坏：`trace.fold` 漂移 ⇒ 必红点名 |
| `05-mutation-groups-unknown-red.txt` | 外部改坏：`groups_expected` 未知组 ⇒ 必红点名 |
| `06-mutation-fold-missing-red.txt` | 外部改坏：缺 `trace.fold` ⇒ fail-closed 红 |
| `07-selftest-rng-ledger-diff.txt` | R-6 folded 账本 selftest 32/32（回归） |
| `08-selftest-attack-rng-ledger.txt` | 攻击账本 selftest 32/32（回归） |
| `09-gates-four.txt` | 四门禁读数 |
| `10-boundary.txt` | base/head 边界（零生产码） |

## 残留物清单

- `E:/tmp/m3bag-scenariofold/`（改坏副本 + 生成的 3 个变异 scenario；可清）；
  `E:/tmp/m3bag-scenariofold-{clippy,test,msgcodegen}.log`（门禁原始日志；可清）；
  `E:/tmp/m3bag-target/`（隔离 CARGO_TARGET_DIR；可清）。
- worktree 内 8 个他线残留未跟踪目录 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-2323..2346`
  （非本片、未纳入、未清）。

## 未做 / 边界（如实登记）

旧会话的 8 个未跟踪目录与本片无关；本片**未起栈、未占端口**（仅共享 3306 在听）。
真机同场景含怪两相 trace + `groupdiff --fold --strict` 仍需 (a) owner 独占窗口 (b) 生产者驱动承接卡
（写集 `crates/world`/`bins/**`/`src/**` 超出本卡禁令）；本片不改变该状态，real-run 仍 fail-closed `exit 2`。
