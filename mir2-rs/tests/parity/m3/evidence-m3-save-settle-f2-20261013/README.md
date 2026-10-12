# 证据包 — M3 夹具：AFTER 快照前「等断线存档落库」的确定性等待（F-2 竞态修）

- 卡：`openmir2-m3-drop-exp-align`（登记 reviewer-6 的夹具 Minor；owner 2026-10-13 04:05 明确「改完在本卡或本卡上登记，**别开新卡**」）
- 车道：`m3bag`（principal `openmir2-worker-4`）／分支 `worktree-m3-drop-exp`；base = `e2b94234`（当前 `origin/master`）
- base 刷新：开工时 `origin/master=62fffb2e`；工作期间 master 前进一片 `e2b94234`（⑩ 邮件/拍卖**收口登记**，只动 `docs`/`whitelist`/证据、**未碰本片 4 个夹具文件**）⇒ 交付前已 `git rebase origin/master` 消除回退假差量（`git diff --name-only e2b94234..HEAD` 仅本片 16 件）
- 写集：`mir2-rs/tests/parity/m3/**`。**零 `.rs`**、**零 `src`/`sql`/`MirClient`**、**未起栈、未占端口**
- 上游归因（决定性）：`evidence-m3-scoped-diff-ability-20261013/README.md` §6 + `15/16-*.txt`

## 1. 结论（TL;DR）

`run_pair.ps1` 在取 AFTER 快照前用**固定** `Start-Sleep 3`，但 oracle 的断线存档是**异步**的
（`run_bots` 收工杀 BotSrv ⇒ GameSvr 异步 `SaveHumanRcd` ⇒ DBSrv 落库）：**C# 实测落库滞后 +3.7s / +7.7s**，
Rust 亚秒级。⇒ C# 侧 `snap_after` 会**间歇性早于落库**，把「存档还没写」误判成「实现差异」——
F-2 登记的 7 处差异里 **6 处同因**，T-14 家族（`scoped_diff` 9 表）持续假红。

本片把它换成**等状态变化**的确定性等待：轮询「本前缀角色的存档是否在 BEFORE 快照的 DB 时钟之后落库」，
轮询到**全部**落库且**连续 2 轮**保持才继续；`30s` 内未达成 ⇒ **fail-closed**（点名 RED，绝不静默继续）。
只调大 sleep 仍是竞态，故未采用。

## 2. 改了什么（4 个夹具文件，零 `.rs`）

| 文件 | 改动 |
| --- | --- |
| `gate_guards.ps1` | 新增唯一口径：`Get-M3SaveSettleProbeSql`（探针 SQL）、`Invoke-M3SaveRcdSettle`（轮询引擎，`Expected`/`TimeoutSeconds`/`PollMilliseconds`/`QuietPolls`，返回 `Settled/Polls/ElapsedMs/Reason/Samples`；`Expected<1`、`QuietPolls<2` 直接 throw）、`Format-M3SaveSettleInstrument`（仪器形态）+ 3 个常量 `30s/250ms/2` |
| `run_pair.ps1` | ① BEFORE 快照前取 DB 参考时刻 `SAVE_SETTLE_REF`（`SELECT DATE_FORMAT(NOW(),...)`）② AFTER 快照前用 `Invoke-M3SaveRcdSettle` 替换固定 `Start-Sleep 3`；超时 `throw`（RED）③ `-EmitOpsArgs` 增 `save_settle_instrument` ④ `baseline.json` 增结构化 `save_settle` 字段 ⑤ 仅 `-BagEquipSequence/-AttackSequence` 且非 `-SkipBots` 生效 |
| `run_bag_equip_pair.ps1` | `-SelfCheckNoStack` 新增 **⑫a–d**：晚落库必等到（>=13 轮）、永不落库 fail-closed、单次观测不得 settled、生产 `save_settle_instrument` 机判 |
| `run_attack_pair.ps1` | `-SelfCheckNoStack` 新增 **24**：攻击路径 `-EmitOpsArgs -AttackSequence` 必带同一 `save_settle_instrument`（引擎牙齿在 ⑫，两侧共用同一函数） |

**探针为什么用 `ModifyTime > 参考时刻` 而不是 `IS NOT NULL`**：oracle 的
`characters_ablity` INSERT（`src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Add.cs:162`）**不写 `ModifyTime`**
（保持 NULL），首次 `SaveAblity`（`PlayDataStorage.Save.cs:141`）才 `ModifyTime=now()`；而 Rust 侧创建时就
置值（`bins/db-srv/src/playdata.rs:851` `create_ablity_status` → `save_ablity`）⇒ 只有「晚于参考时刻」这一
口径对**两侧**都成立，否则 Rust 侧会开局即 satisfied（假 settle）。

## 3. 判据原始读数

| 判据 | 文件 | 读数 |
| --- | --- | --- |
| ① 无栈自检（背包/装备，含 ⑫a–d） | `01-bag-selfcheck-GREEN.txt` | **`SELFTEST_RED GREEN`** rc=0；⑫a `Settled=True polls=13`、⑫b `False/save_rcd_settle_timeout`、⑫c `False`、⑫d `mode=save_rcd timeout_s=30 poll_ms=250 quiet_polls=2` |
| ② 无栈自检（攻击，含 24） | `02-attack-selfcheck-GREEN.txt` | **`SELFTEST_RED GREEN`** rc=0；24 `save_settle_instrument='mode=save_rcd timeout_s=30 poll_ms=250 quiet_polls=2'` |
| ③ 改坏必红 A（去掉生产仪器接线） | `03-mutation-a-no-instrument-RED.txt` | **RED**：`save_settle_instrument_missing`（+ missing_timeout/poll/quiet） |
| ④ 改坏必红 B（超时也判 settled，削弱 fail-closed） | `04-mutation-b-settle-weakened-RED.txt` | **RED**：`save_settle_never_landed_false_green, save_settle_timeout_reason=save_rcd_settled, save_settle_single_observation_false_green` |
| ⑤ 还原逐字节一致 | `05-restore-sha256.txt` | 变异前后 sha256 相同（`fc75d576…` / `3ca8f5d8…`） |
| ⑥ 四门禁 | `06`/`07`/`08` | fmt rc=0；clippy `-D warnings` rc=0；test **86 suites / 759 passed / 0 failed / 18 ignored**；msgcodegen **604** rc=0 |
| ⑦ 边界 | `09-code-diff.txt` | 仅 4 个 `.ps1`；`git diff --name-only <base> -- src sql MirClient` 为空 |

## 4. EOL / 证据口径

本目录所有件**盘上即 LF**（写出时显式归一），`SHA256SUMS.txt` 按**仓库字节（Git blob）**记。
复核：`verify_sha256sums.py <dir> --check-extra` 与 `eolcheck <dir>` 均 GREEN（见 §3 命令、`00-commands.txt`）。

## 5. 残留物清单

- 临时物：`E:/tmp/m3bag-savesettle/**`（备份 + 原始自检/变异/门禁输出）、`E:/tmp/m3bag-savesettle-target`（编译 target，按惯例保留）。均未入库。
- **未动**：worktree 里 8 个未跟踪 `mir2-rs/tests/parity/m3/sessions/csharp-20261012-*`（别的卡残留，未混进本片）。
- 端口：本片**未起任何服务端栈**；自检不 bind/connect 任何游戏端口（仅无栈逻辑与既有只读 `3306` 探测路径不触发）。
- 真机验证：本片是**夹具修**，其真机效果要等 **T-14 / ②③ 的下一次窗口复跑**（届时 `snap_after` 应在存档落库之后）。
