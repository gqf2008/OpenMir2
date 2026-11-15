# load_gate 产物哈希与退出码语义（卡 `openmir2-loadgate-filehash-rc`）

> 结论先行：`load_gate.ps1` 的 manifest 步骤改用 **.NET `SHA256.Create()`**（不再依赖
> `Get-FileHash`）；产物清单格式保持 `{generated_ms, files:[{file,sha256}]}`（小写 hex，
> SHA-256）。退出码分开：**判据 RED ⇒ 1；环境/工具问题 ⇒ 2（不可判）；全绿 ⇒ 0**。
> 环境问题不再伪装成「测试失败」。

## 1. 背景（实测）

本机 Windows PowerShell 的 `Get-FileHash` 不可用：`load_gate.ps1` 的 manifest 步骤
（原第 888 行）在 tier 结果、`report.json`、`load_stats.ndjson` 都写完之后抛错，
脚本终止 ⇒ **整轮 rc=1，且不写 `artifacts.sha256.json` / `load_gate_summary.json`**。
2026-11-15 的 200/500 窗口就是此形态（tier 报告完整、GREEN；gate rc=1）。

## 2. 实现

### 2.1 哈希

```powershell
function Get-Sha256Hex([string]$Path) {
    # 生产：System.Security.Cryptography.SHA256；FileShare.ReadWrite 读正在写的日志
    # 返回小写 hex（与原 Get-FileHash...Hash.ToLower() 一致）
}
```

`Write-ArtifactManifest($Dir)`：按原 patterns（`report.json`/`load_stats.ndjson`/`mem.ndjson`/
`hostload.ndjson`/`login_conns.ndjson`/`bots.err.log`/`bots_runtime.json`）递归收集；
每个文件 `Get-Sha256Hex`；全部成功才写 `artifacts.sha256.json`。

### 2.2 失败语义

任何哈希失败（缺工具/坏文件/非 64-hex）：
- **不写半份清单**（避免把未哈希的文件伪装成“已核对”）；
- 写 `artifacts.sha256.inconclusive.json`：`{status:"inconclusive", error, generated_ms, files:[]}`；
- 打印 `ARTIFACT_HASH_INCONCLUSIVE: ... —— 不可判（环境/工具问题，不是判据 RED）`；
- `load_gate_summary.json` 仍写：`artifacts_hash_status:"inconclusive"` + `artifacts_hash_error`，
  并保留各 tier 的判据结果。

### 2.3 退出码

```powershell
function Get-GateExitCode([bool]$CriteriaRed, [bool]$ArtifactInconclusive) {
    if ($ArtifactInconclusive) { return 2 }   # 环境/工具问题：不可判
    if ($CriteriaRed) { return 1 }            # 判据 RED
    return 0                                  # 全绿
}
```

最终退出前先判 `ArtifactHashInconclusive` ⇒ **rc=2**；否则按原逻辑 1/0。
环境问题**不会**被算成 RED；两者同时出现时以 2 为准，summary 里仍保留判据结果。

## 3. 自测（无栈，`-SelfTestArtifactHash`）

`powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestArtifactHash` → **rc=0**：

| 用例 | 结果 |
| --- | --- |
| 正常 sha | `alpha`/`beta` 与黄金字面量 `8ed3f6ad…`/`f44e64e7…` 一致 |
| 实际 manifest 正常目录 | `artifacts.sha256.json` 生成、`status=ok` |
| 强制缺哈希工具（`LOADGATE_TEST_HASH_FAIL=1`） | `artifacts.sha256.inconclusive.json` 生成、`inconclusive=true`、`Get-GateExitCode`=**2** |
| 坏文件（不存在路径） | `Get-Sha256Hex` 抛错（→ manifest 层「不可判」） |
| rc 三态 | normal 0 / criteria RED 1 / inconclusive 2 |

兼容性：对正常 manifest 跑 `python tools/botload/verify_artifacts.py <manifest>` → **1/1 匹配，rc=0**。

## 4. 既有归档复算（A4）

`powershell -File tools/botload/load_gate.ps1 -RecomputeArtifacts E:\UserFleet\archive\botsrv-census-verify-20261115`
→ **`ARTIFACT_RECOMPUTE PASS: 14 个文件与既有清单一致`，rc=0**（逐文件 `file`/`sha256` 对比）。

## 5. 对明天 360s 窗口的影响

- 修复后 gate 会在 tier 循环后正常写 `artifacts.sha256.json` 与 `load_gate_summary.json`；
- rc：全绿 0 / 判据 RED 1 / 环境问题 2（并写 inconclusive 标记）；
- 判据阈值、tier 逻辑、报告字段均未改；只改哈希实现与退出码语义。

## 6. 复现

```sh
# 无栈自测
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestArtifactHash
# 既有归档复算
powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -RecomputeArtifacts E:\UserFleet\archive\botsrv-census-verify-20261115
# 正常路径（明天窗口）：-Tiers ... -HoldSec 360 -SkipBuild ... ⇒ 产物清单生成、rc 三态
```
