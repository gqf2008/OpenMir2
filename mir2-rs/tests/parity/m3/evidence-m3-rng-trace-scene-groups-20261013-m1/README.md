# 证据包：M-1 收口（scenario runner 参数名 doc-vs-code 漂移 + 机判）

- 卡：`openmir2-m3-rng-trace-scene-groups`（issue `d9d03f47`）
- 车道 `m3bag` / principal `openmir2-worker-4` / 分支 `worktree-m3-rng-trace-scene-groups`
- base = master `15510a47`；本片零 `.rs`，改动仅
  `rng-trace-scene/{rng_trace_scenario.json, run_rng_trace_pair.ps1}`（夹具写集 `mir2-rs/tests/parity/m3/**`）
- 上片（offline ①②③）已双签合入 master `15510a47`；本片 **不移动上片 `reviewed_head=87f033e9`**（那是已合并历史），
  是独立新片。

## 这条 finding 是什么（M-1）

作者在上一 burst 的只读复核里登记（thread comment `29750cdc`）：

> `rng-trace-scene/rng_trace_scenario.json` 的 `trace.runner` 写了
> `run_rng_trace_pair.ps1 -Go -Side <csharp|rust> -PinScene -PeerSession <session>（窗口期）`，
> 而 `run_rng_trace_pair.ps1` 的 `param()` **没有声明 `-PinScene`**（全仓仅该一处出现）。
> ⇒ 照文档字面执行会在 **PowerShell 参数绑定期**即报错，
> 永远到不了 `-Go` 的 fail-closed 分支。属 doc-vs-code drift（Minor，无当下影响）。

按 AGENTS §3「可判的技术问题自己判完，不许挂起」：本片**两条一起做**——
把文档写成真的（声明参数并给它真语义），并加一条**机判**防止同类漂移再进来。

## 本片做了什么

1. **`-PinScene` 声明 + 真语义（不是空开关）**：
   - 声明 `[switch]$PinScene`；real-run 分支**显式要求**它：
     只给 `-Go` 不给 `-PinScene` ⇒ `PREREQ_MISSING ... 需显式 -PinScene` **exit 2**（防止误跑未钉场景）；
   - `-Go -PinScene` 齐备 ⇒ 越过参数门，但驱动仍未接线 ⇒ 仍 fail-closed **exit 2**（诚实拒跑，不静默起栈）。
2. **机判 `scenario-runner-params-declared`**（无栈自检第 ⑫ 项）：
   `Test-RngScenarioRunnerParams` 读取 scenario 的 `trace.runner` + `artifact.runner`，
   抽出所有「前面不是标识符字符」的单横线参数名（⇒ 不会把路径 `rng-trace-scene` 误读成 `-trace`/`-scene`），
   断言**每个名字都是本脚本已声明参数**（`$MyInvocation.MyCommand.Parameters.Keys`，反射取真源）；
   名字集为空也判红（不许机判空转）。real-run 分支起栈前也跑同一函数，漂移即 `PREREQ_MISSING` exit 2。
3. **改坏必红（内置）+ 外部复现**：
   - 内置：自检把 scenario 的 runner 文本塞 `-NotARealParam` ⇒ 正向项 **FAIL 并点名**、漂移项 ok（`02-`）；
   - 外部：用同一改动造副本 scenario 跑整轮 ⇒ `SELFTEST ok=13/14 failures=scenario-runner-params-declared` **rc=1**。
4. **回归**：R-6 folded 账本 32/32、场景门 11/11、攻击账本 32/32、四门禁全绿（零 `.rs`，读数与 master 合并树一致）。

## 件清单

| 文件 | 内容 |
|---|---|
| `00-commands.txt` | 全部可复算命令 |
| `01-selftest-nostack.txt` | `run_rng_trace_pair.ps1 -SelfCheckNoStack` ⇒ **14/14 GREEN**（原 12 项 + 新增 ⑫ 正/负两项；脚本自写 LF） |
| `02-mutation-runner-param-drift-red.txt` | **改坏必红**：runner 塞 `-NotARealParam` ⇒ `scenario-runner-params-declared` FAIL 点名 + `SELFTEST ok=13/14` **rc=1** |
| `03-realfail-go-without-pinscene.txt` | real-run 只给 `-Go` ⇒ `需显式 -PinScene` **exit 2** |
| `04-realfail-go-pinscene-driver-unwired.txt` | `-Go -PinScene` ⇒ 参数门过、驱动未接线 **exit 2** |
| `05-selftest-scene-gate.txt` | 场景前置门回归 **11/11 GREEN** |
| `06-selftest-rng-ledger-diff.txt` | R-6 folded 账本回归 **32/32 GREEN** |
| `07-selftest-attack-rng-ledger.txt` | 攻击账本回归 **32/32 GREEN** |
| `08-gates-four.txt` | 四门禁：fmt rc=0 / clippy rc=0 / test **94 suites 878 passed 0 failed 21 ignored** / msgcodegen **604** |
| `09-boundary.txt` | 边界：改动仅 2 个 `rng-trace-scene/**` 文件；`src sql MirClient mir2-rs/{crates,bins}` = 空 |

## 为什么这不是「空开关」

`-PinScene` 的语义是「真机两相 trace 必须显式请求按 scenario 钉住含怪场景」。它现在就能改变行为：
**只给 `-Go` 会被拒**（`03-`）；给全了也**仍拒跑**（`04-`，因为驱动未接线）。
所以它不是为过文档而加的死参数，而是把「必须钉场景」这条口径变成 fail-closed。

## 与上片 / 真机半边的关系

- 本片**不含真机两相 trace**。本卡 DoD 的真机半边（同场景含怪两相 trace + `groupdiff --fold --strict`）
  仍等 owner 发独占窗口，与 ④ 真机 100 杀同一批次；窗口未发前不起栈、不占端口。
- 「两级驱动接线」（两侧场景描述件 + 全场景抽数流水）仍是本卡外的承接卡；
  本片只保证：承接卡接线后，文档里那条真机命令**参数名全部可绑定**，且同类漂移从此有机判咬住。

## 复核

```
python -I tools/evidence-hash/verify_sha256sums.py --check-extra \
  mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-m1
python -I tools/evidence-hash/verify_sha256sums.py eolcheck \
  mir2-rs/tests/parity/m3/evidence-m3-rng-trace-scene-groups-20261013-m1
```

清单按**仓库字节（LF / git blob）**口径记；本包文本一律 LF 写出。
