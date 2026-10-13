# evidence —— M3④ 夹具 DoD②：`run_drop_exp_pair.ps1` real-run 接线（2026-10-14）

> 卡：`openmir2-m3-drop-exp-fixture-botsrv`（M3④ 真机对拍夹具）。
> 本片 = **DoD②**（real-run 接线）：`run_drop_exp_pair.ps1 -Go` → `run_pair.ps1 -DropExpSequence`
> → `tools/botload/run_bots.ps1` DropExp* 配置透传 → BotSrv 固定掉落/经验序列。
> 前一片（DoD①）证据在 [`../evidence-drop-exp-fixture-botsrv-20261014/`](../evidence-drop-exp-fixture-botsrv-20261014/)。
> base = `origin/master` = `1cf88f6ad1d7d300ab31072bdfa025585c140bd3`；上一片 head = `9dcfb6f34`。

## 改了什么（逐文件）

| 文件 | 改动 |
| --- | --- |
| `tools/botload/run_bots.ps1` | 新增 `-DropExpSequence/-DropExpKills/-DropExpStepMs/-DropExpStartDelayMs/-DropExpDir/-DropExpRole/-DropExpMonster/-DropExpOut`；写进 `AppSetting.json`；`Get-M3SeqDoneCount` 认 `M3DROPEXPSEQ phase=complete`；预期 done = **1**（只有 attacker）；等待/收尾/GREEN/RED + 离线自检（done 1/0、sent 不算 done、预期=1、完备性两向） |
| `mir2-rs/tests/parity/m3/run_pair.ps1` | 新增 `-DropExpSequence` 家族；`Get-M3BotOpPlan` 透传（含 `-DisableAutoPlay` 耦合 + 真实跑追加 `-DropExpOut <session>/drop_exp_raw.jsonl`）；`-EmitOpsArgs` 出 `drop_exp_sequence`；读 bot.log `M3DROPEXPSEQ` 判完整（`complete` + `kills >= -DropExpKills` + 原始件存在且非空）；`baseline.json` 落 `drop_exp_sequence/drop_exp_markers/drop_exp_raw`；save_settle 仪器纳入 DropExp |
| `mir2-rs/tests/parity/m3/gate_guards.ps1` | 新增 `Format-M3DropExpScope` / `ConvertTo-M3DropExpScope`（canonical，缺 `disable_autoplay=` 按 False 还原）/ `Get-M3PeerDropExpScopeGuard`（peer baseline 缺失/半写/形态坏/开关不一致 ⇒ fail-closed） |
| `mir2-rs/tests/parity/m3/drop-exp/run_drop_exp_pair.ps1` | real-run 分支：`-Go` 后读场景件（单一真源，`-Kills` 必须与 `scenario.kills` 一致）→ MySQL 预检 → `run_pair.ps1` 两相 pin + `-DropExpSequence` → 会话发现 → `baseline.drop_exp_sequence` → `-Side rust -PeerSession` 时 peer 起点/开关护栏 + `drop_exp_diff.py diff`。**未给 `-Go` 仍 fail-closed rc=2**。 |
| `mir2-rs/tests/parity/m3/drop-exp/README.md` | 计数 16→19；现状说明（DoD② 已落地、单只怪驱动仍缺） |

## 判据 / 牙齿（原始件）

| 文件 | 内容 | rc |
| --- | --- | --- |
| `00-selfcheck-nostack.txt` | `-SelfCheckNoStack -SelfCheckOut`（脚本自落 **LF** 转录）：比较器 12/12 + 纯度门 + LF + 零端口 + `no-go-realrun-prereq` + `emit-dropexp-passthrough`/`-default-off` + `botsrv-dropexp-config-on`/`-default-off` + `emit-dropexp-pin-args` + BotSrv selftest ⇒ **ok=20/20** | 0 |
| `01-botsrv-selftest-dropseq.txt` | `dotnet build -c Release` 0 错 + `BotSrv.exe --selftest-dropseq` GREEN（26 断言） | 0 |
| `02-redcheck-runpair-passthrough.txt` | **改坏必红①**：停用 `run_pair` 的 DropExp 计划分支 ⇒ `emit-dropexp-passthrough` FAIL、ok=18/19 | 1 |
| `03-redcheck-botsrv-config-passthrough.txt` | **改坏必红②**：写错 `run_bots` 的 DropExp 配置键 ⇒ `botsrv-dropexp-config-on`/`-default-off` 双 FAIL、ok=17/19 | 1 |
| `04-gates.txt` | 四门禁（`cwd=mir2-rs`，`CARGO_TARGET_DIR=E:/tmp/m2svr-target`）：fmt 0 / clippy 0（-D warnings）/ test **101 suites 1200 passed 0 failed 22 ignored** / msgcodegen **604** | 0 |
| `05-boundary.txt` | 冻结面（`src sql MirClient mir2-rs/crates/protocol mir2-rs/crates/storage`，排除已登记 `src/BotSrv/**` 例外）为空；净差量 = 5 件（4 `.ps1`/`.md` + 本证据目录） | — |

- 两次改坏均**按字节还原**（sha256 前后一致，见 `05-boundary.txt` §恢复证明）。
- 新增/修改的 `.ps1` 均为 UTF-8/LF；证据 `.txt` 用 `Write-M3LfText` / `WriteAllText(UTF8 no BOM)` 以 LF 落盘。

## 未接线 / 不静默

- **单只 pin 怪驱动仍缺**（真实跑前必须落地）：场景件 `isolation.comment` 与
  [`rng-trace-scene/DRIVER-CONTRACT.md`](../../rng-trace-scene/DRIVER-CONTRACT.md) §7 指出的
  「0137 上运行时 pin 出**单只固定怪**」（C# 侧 GM/脚本 spawn + Rust 侧宿主 AI 模式开关），
  归 shared 承接卡 `openmir2-m3-rng-scene-driver`（**尚未开卡**）。
  单只怪未落地时，本接线的 `run_pair` DropExp 分支会在「0 杀 < `-DropExpKills`」处 RED（fail-closed，不静默绿）。
- **真机 100 杀两侧同序列 diff=0 + 纯度门实测**仍为卡级 R 项，须 owner 独占窗口（-Go），归下一片。

## 窗口纪律

本片**零起栈、零占端口**（无服务启动；仅离线 `dotnet build` + `cargo` 门禁 + PS/python 自检）。
保留物（仓外，可删）：`E:/tmp/m2svr-dexp2/`（门禁/自检/红检转录 + `.bak`）。
