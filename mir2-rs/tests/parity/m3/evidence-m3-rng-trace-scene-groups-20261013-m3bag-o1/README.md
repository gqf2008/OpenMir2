# 证据包：无栈自检牙 O-1 收口（base 值牙内自解析 + 非空转对照）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- base = master **`6f0bca48e`**（上一片已合并）；本片零 `.rs`，改动仅在
  `rng-trace-scene/{run_rng_trace_pair.ps1, README.md}` + 本证据包（夹具写集 `mir2-rs/tests/parity/m3/**`）
- 本片是**评审登记的后续 Minor**，不移动任何已合并的 `reviewed_head`（上一片 `aa93dd9a0` 已由
  reviewer-6 `b53a7451` approve 并合并为 master `6f0bca48e`）。

## 这条 Minor 是什么（reviewer-6 `b53a7451`，O-1）

`run_rng_trace_pair.ps1 -SelfCheckNoStack` 的第 ⑰ 项牙 `scenario-drift-samples-json-native`
里，fold 半断言写的是：

```powershell
($driftBack.trace.fold.construct -ne $sc.trace.fold.construct)
```

`$sc` 是**同一 SelfCheckNoStack 块更早的场景校验项**（约 line 240）解析出来的对象。当前有定义 ⇒
18/18 有牙；但该块一旦重构/改名，`$sc` 会退化为 `$null` ⇒ `-ne` 恒真 ⇒ **fold 半断言空转**
（runner/groups 两半仍由字符串/包含断言咬住，所以当时判 Minor、非阻塞）。

## 本片改法（唯一逻辑变更）

1. **牙内自解析 base 值**：在 ⑰ 内 `$srcBack = (Get-Content -LiteralPath $ScenarioFile -Raw | ConvertFrom-Json)`，
   断言改为 `$driftBack.trace.fold.construct -ne $srcBack.trace.fold.construct`，并显式要求 `$null -ne $srcBack`
   ⇒ 去掉对外层 `$sc` 的跨块隐式依赖；base 解析失败也直接判红（不静默绿）。
2. **新增第 ⑱ 项非空转对照** `scenario-drift-tooth-not-vacuous`：用 `New-RngDriftScenario` 造一份
   **空转样本**（mutation 什么都不改），断言同一 fold 谓词对它为 **false** ⇒ 证明 ⑰ 不是恒真、drift
   构造一旦失效会被咬住（这正是 O-1 要防的「空转」）。

`README.md` §5 自检项数由 18 同步为 19。

## 判据读数（原始件在本包）

| 项 | 命令 | 读数 | 件 |
|---|---|---|---|
| 无栈自检（runner 级） | `run_rng_trace_pair.ps1 -SelfCheckNoStack` | `SELFTEST ok=19/19 failures=` rc=0 | `01-` |
| 改坏必红 A（O-1 回归：base 取不到） | base 解析改 `$srcBack = $null` | `ok=17/19`，2 项 FAIL（⑰ ⑱）rc=1 | `02-` |
| 改坏必红 B（drift 构造空转） | 注释 `& $Mutate $sc` | `ok=15/19`，4 项 FAIL（3 条 drift-red + ⑰）rc=1 | `03-` |
| 场景门 selftest（回归） | `rng_scene_gate.py selftest` | `SELFTEST ok=16/16` rc=0 | `04-` |
| scenario 件自洽 | `rng_scene_gate.py scenario --scenario rng_trace_scenario.json` | `VERDICT GREEN` rc=0 | `05-` |
| 账本 selftest（回归） | `rng_ledger_diff.py selftest` | `SELFTEST GREEN 32/32` rc=0 | `06-` |
| 攻击账本 selftest（回归） | `attack_rng_ledger.py selftest` | `SELFTEST GREEN 32/32` rc=0 | `07-` |
| 四门禁 | fmt / clippy / test / msgcodegen | fmt0 / clippy0 / 97 suites 987 passed 0 failed 21 ignored / 604 | `08-` |
| 边界 | `git diff --cached --name-status 6f0bca48e` | 仅 `mir2-rs/tests/parity/m3/**`；`src sql MirClient mir2-rs/crates mir2-rs/bins` 空 | `09-` |
| 源码差量 | `git diff --cached 6f0bca48e -- .../rng-trace-scene` | 见件 | `10-` |

## 未做 / 边界

真机同场景含怪两相 `trace` + `groupdiff --fold --strict` 仍需 owner 独占窗口 + 生产者驱动承接卡
（real-run 对 `-Go`/`-PinScene` 一律 fail-closed `exit 2`）；本片纯离线、零端口、未起栈、未 merge、未自签。
