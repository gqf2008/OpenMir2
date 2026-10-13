# 证据包：无栈自检的 scenario drift 构造改走 JSON 字段改写（M-1 sibling 登记 Minor 收口）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- base = master `0f9fe9997`；本片零 `.rs`，改动仅在
  `rng-trace-scene/{run_rng_trace_pair.ps1, README.md}` + 本证据包（夹具写集 `mir2-rs/tests/parity/m3/**`）
- 离线 ①②③ + R-6 minor + M-1 收口 + M-1 sibling 均已合入 master；本片是**评审登记的后续 Minor**，
  不移动任何已合并的 `reviewed_head`。

## 这条 Minor 是什么

`run_rng_trace_pair.ps1 -SelfCheckNoStack` 里有三条「改坏必红」用例，需要由入库的
`rng_trace_scenario.json` 构造出**漂移样本**（runner 多一个参数 / `fold.construct` 改成别的组 /
`groups_expected` 塞一个未知组）。原实现是对 scenario 的**原始文本**做正则 `-replace`：

- `-replace '-PinScene', '-PinScene -NotARealParam'`
- `-replace '"construct"\s*:\s*"spawn"', '"construct": "monster_ai"'`
- `-replace '("groups_expected"\s*:\s*\[)', '$1"no_such_group", '`

评审（reviewer-5，M-1 sibling 登记：`3a462fc9`）指出：这类构造依赖 JSON 的**字面拼写**，
scenario 一旦重排格式 / 换引号风格，替换会空转 ⇒ drift-red 用例自曝 RED（fail-loud，不会假绿），
建议后续改用 JSON 解析构造。

## 本片改法（唯一逻辑变更）

新增 `New-RngDriftScenario`（`ConvertFrom-Json` 读入 → scriptblock 字段改写 → `ConvertTo-Json -Depth 20`
→ `Write-M3LfText` 以 LF 落盘），三条 drift 构造全部改走它 ⇒ **与文本格式无关**。

并新增一条牙 `scenario-drift-samples-json-native`：把三份 drift 样本**解析回对象**，断言
① runner 文本确含 `-NotARealParam`；② `fold.construct` 与源不同；③ `groups_expected` 确含 `no_such_group`。
构造一旦空转（样本 == 源、drift 用例其实没漂移）⇒ 该牙必红。

## 判据读数（原始件在本包）

| 项 | 命令 | 读数 | 件 |
|---|---|---|---|
| 无栈自检（runner 级） | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | `SELFTEST ok=18/18 failures=` rc=0 | `01-` |
| 改坏必红（构造空转） | 同上（注释 `& $Mutate $sc`） | `SELFTEST ok=14/18`，4 项 FAIL（`scenario-runner-param-drift-red` / `scenario-fold-drift-red` / `scenario-groups-unknown-red` / `scenario-drift-samples-json-native`）rc=1 | `02-` |
| 场景门 selftest（回归） | `rng_scene_gate.py selftest` | `SELFTEST ok=16/16` rc=0 | `03-` |
| scenario 件自洽 | `rng_scene_gate.py scenario --scenario rng_trace_scenario.json` | `VERDICT GREEN` checks=8 failed=0 rc=0 | `04-` |
| 账本 selftest（回归） | `rng_ledger_diff.py selftest` | `SELFTEST GREEN 32/32` rc=0 | `05-` |
| 攻击账本 selftest（回归） | `attack_rng_ledger.py selftest` | `SELFTEST GREEN 32/32` rc=0 | `06-` |
| 四门禁 | fmt / clippy / test / msgcodegen | fmt0 / clippy0 / 95 suites 966 passed 0 failed 21 ignored / 604 | `07-` |
| 边界 | `git diff --cached --name-status 0f9fe9997` | 仅 `mir2-rs/tests/parity/m3/**`；`src sql MirClient mir2-rs/crates mir2-rs/bins` 空 | `08-` |
| 源码差量 | `git diff --cached 0f9fe9997 -- .../rng-trace-scene` | 见件 | `09-` |

## 未做 / 边界

真机同场景含怪两相 `trace` + `groupdiff --fold --strict` 仍需 owner 独占窗口 + 生产者驱动承接卡
（real-run 对 `-Go`/`-PinScene` 一律 fail-closed `exit 2`）；本片纯离线、零端口、未起栈。
