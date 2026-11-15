# load_gate 产物哈希与退出码语义（卡 `openmir2-loadgate-filehash-rc`）

> 结论先行：`load_gate.ps1` 的 manifest 步骤改用 **.NET `SHA256.Create()`**（不再依赖
> `Get-FileHash`）；产物清单格式保持 `{generated_ms, files:[{file,sha256}]}`（小写 hex，
> SHA-256）。退出码分开：**判据 RED ⇒ 1；环境/工具问题 ⇒ 2（不可判）；全绿 ⇒ 0**。
> 环境问题不再伪装成「测试失败」。

## 1. 背景（实测）

本机 Windows PowerShell 的 `Get-FileHash` 不可用：`load_gate.ps1` 的产物清单步骤
（`Write-ArtifactManifest`；原实现是 `Get-FileHash` 调用）在 tier 结果、`report.json`、
`load_stats.ndjson` 都写完之后抛错，
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
- **半份清单牙（P2-2）**：`-SelfTestArtifactHash` 在清单**已开始写之后**（第 3 个文件）强制失败，
  断言不产生 `artifacts.sha256.json`、只产生 inconclusive 文件。

### 2.3 退出码

```powershell
function Get-GateExitCode([bool]$CriteriaRed, [bool]$ArtifactInconclusive) {
    if ($ArtifactInconclusive) { return 2 }   # 环境/工具问题：不可判
    if ($CriteriaRed) { return 1 }            # 判据 RED
    return 0                                  # 全绿
}
```

生产最终退出统一走 `Exit-WithGateResult`；最终退出前先判 `ArtifactHashInconclusive` ⇒ **rc=2**；
否则按原逻辑 1/0。环境问题**不会**被算成 RED；两者同时出现时以 2 为准，summary 里仍保留判据结果。

**生产 rc 路径牙（P2-1）**：自测通过子进程 `-SelfTestProductionRc inconclusive|red` 走
`Exit-WithGateResult`（与真实窗口同一条路径），断言 rc=2/1；不是只调 `Get-GateExitCode` 的孤立函数。

## 3. 自测（无栈，`-SelfTestArtifactHash`）

`powershell -ExecutionPolicy Bypass -File tools/botload/load_gate.ps1 -SelfTestArtifactHash` → **rc=0**：

| 用例 | 结果 |
| --- | --- |
| 正常 sha | `alpha`/`beta` 与黄金字面量 `8ed3f6ad…`/`f44e64e7…` 一致 |
| 实际 manifest 正常目录 | `artifacts.sha256.json` 生成、`status=ok` |
| 强制缺哈希工具（`LOADGATE_TEST_HASH_FAIL=1`） | `artifacts.sha256.inconclusive.json` 生成、`inconclusive=true`、`Get-GateExitCode`=**2** |
| 坏文件（不存在路径） | `Get-Sha256Hex` 抛错（→ manifest 层「不可判」） |
| rc 三态 | normal 0 / criteria RED 1 / inconclusive 2 |
| 生产路径（子进程 `-SelfTestProductionRc`） | inconclusive ⇒ **rc=2**；red ⇒ **rc=1**（走 `Exit-WithGateResult`） |
| 部分清单失败（第 3 个文件） | 不产生 `artifacts.sha256.json`，只产生 inconclusive 文件 |
| `-RecomputeArtifacts` 缺目录 | **rc=2**（不可判；不再是 1） |
| 环境残留 `LOADGATE_TEST_HASH_FAIL=1` | 生产 recompute 忽略（P3-1 守卫），PASS rc=0 |
| 自测默认 `-OutDir` | 临时目录；`tests/loadgate/` 无残留（P3-4） |

兼容性：对正常 manifest 跑 `python tools/botload/verify_artifacts.py <manifest>` → **1/1 匹配，rc=0**。

### 3.1 变异（真跑；复审 P2 指出的两处）

| 变异 | 结果 |
| --- | --- |
| M-P2-1：生产 `Exit-WithGateResult` 的 `exit 2` 改 `exit 1` | `ARTIFACT_HASH SELFTEST FAIL: production inconclusive rc=1 want 2` |
| M-P2-2：catch 里先写半份 `artifacts.sha256.json` | `ARTIFACT_HASH SELFTEST FAIL: partial manifest was written` |
| 恢复 | 脚本 sha256 `34cf4cb83079740d7fd75e4486a4705a64e605e4f1d563aed56b79d4c2a02061`；自测 **rc=0** |

## 4. 既有归档复算（A4）

`powershell -File tools/botload/load_gate.ps1 -RecomputeArtifacts E:\UserFleet\archive\botsrv-census-verify-20261115`
→ **`ARTIFACT_RECOMPUTE PASS: 14 个文件与既有清单一致`，rc=0**（逐文件 `file`/`sha256` 对比）。
不存在目录 ⇒ `ARTIFACT_RECOMPUTE INCONCLUSIVE ...`，**rc=2**（P3-2；同一错误码不再与判据 RED 混淆）。

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
