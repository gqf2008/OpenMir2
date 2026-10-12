# evidence-prewarm-marker-20261012

reviewer-6 在 approve `d803a760`（被审 head `2f0dade4`）时留的 **M-1**（非阻塞）：
`gate_guards.ps1::Test-M3PrewarmNoWorldEntry` 对「**日志存在但没有任何 marker**」返回 `ok=True`
——即「预热相没跑到建角、只剩噪声/空日志」会被误判成合格。本片把它补成 **fail-closed 的正向断言**。

## 1. 本片改了什么（零 `.rs`、零 `src/**`、不起栈）

- `mir2-rs/tests/parity/m3/gate_guards.ps1`
  - `Test-M3PrewarmNoWorldEntry` 增**正证** marker 检测：`创建角色`（`SelectChrScene.SelectChrCreateNewChr` 落）
    或 `预热建角完成`（`-StopAfterCharCreated` 短路点 `ClientGetReceiveChrs` 落），任一即算「确实建了角」。
  - **日志存在但零 marker ⇒ `prewarm_marker_missing` fail-closed**（新增）。反证（`进入游戏`/`成功进入游戏`/
    `准备进入游戏`）与日志缺失/不可读的 fail-closed 口径不变。
  - 返回对象新增 `positive` 字段（`markers` 仍只记进世界标记）。
- `mir2-rs/tests/parity/m3/selftest_gate.ps1`
  - ⑩ 增第 4 向用例「日志存在但零 marker=红」（`reason=prewarm_marker_missing`）；绿用例改为同时断言
    `ok=True` 且 `positive` 非空。`total` 63 → **64**。
- `mir2-rs/tests/parity/m3/run_pair.ps1`
  - 预热相门失败时按 `reason` 分派抛错：`prewarm_entered_world` 走「进世界违规」文案；其余（日志缺失/不可读/
    零 marker）走「无合格证据」文案，不再把 `log_missing`/`prewarm_marker_missing` 也说成「出现进世界标记」。
- `mir2-rs/tests/parity/m3/run_attack_pair.ps1`
  - 文档串（§3.1 说明）补「必须有建角正证；零 marker 也 fail-closed（M-1）」。
- `mir2-rs/tests/parity/whitelist.md`
  - T-29 **第八轮**登记（同族夹具扩展；本片不动 `src/**`）。

## 2. 证据清单

| 文件 | 内容 | 结果 |
|---|---|---|
| `01-selftest-gate.txt` | `selftest_gate.ps1`（⑩ 四向：无标记+有正证=绿 / 有进世界标记=红 / 日志缺失=红 / 零 marker=红） | `total=64 failed=0` GREEN |
| `02-prewarm-gate-before-after.txt` | 改前（base `fa456481`）vs 改后：`empty.log` 改前 `ok=True`、改后 `ok=False prewarm_marker_missing` | 改前/改后 |
| `03-mutation-teeth.txt` | 改坏（删正向断言）⇒ `empty.log` 退回绿；把改坏件放回工作树跑 selftest ⇒ ⑩ 新用例 FAIL、rc=1；还原后逐字节相同 | 牙齿 |
| `04-commands.txt` | 复现命令（含 PSModulePath 注记） | — |
| `05-gates-four.txt` | fmt/clippy/test/msgcodegen | 全 rc=0；82 suites / 670 passed / 0 failed |
| `06-attack-pair-selftest.txt` | `run_attack_pair.ps1 -SelfCheckNoStack`（确认 run_pair/gate 改动未回归） | `SELFTEST_RED GREEN` rc=0 |
| `SHA256SUMS.txt` | 本目录文件 hash（**仓库字节 / LF 归一**口径，`verify_sha256sums.py`） | 干净 clone 可复现 |

## 3. 边界 / 残留（如实登记）

- 本片**未起真栈**：owner 19:20 明记「离线可判的别起栈」；预热相进世界的真机复跑由 m3cs 取窗执行
  （`attempts` 是否回到 2 也归它）。本片**零端口占用**。
- 正证 marker 取 `创建角色`/`预热建角完成`：前者是 `run_bots -RequireCharCreated` 自己的收尾判据（必然在
  日志里），后者只在「角色已存在」分支落；两者任一即可。若将来预热相改判据，需同步本门（属 T-29 同族）。
- 「日志存在但零 marker」在真机上还应由 `run_pair` 的 `SELECT COUNT(*) FROM characters` 复核兜底；
  本门把它提前到「日志层」fail-closed，二者不冲突。
