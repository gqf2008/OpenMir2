# M3⑧-10 公会/攻城夹具 —— 解 whitelist.md 冲突重交（merge master）证据包

- 卡：`openmir2-m3-guild-castle-fixture-botsrv`（父卡 `openmir2-m3-guild-castle-align`；T-46）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-guild-castle-fixture-botsrv`
- base：`origin/master 67e9f6c3e097da834ef5769eff1c5415e97bb458`（合并车道 comment `520a9023` 时的 master）
- head：`7cad6d9ead8130c377a85ad5799bea1d70b97409`（merge 提交；取代旧 head `4ddf7df6`）
- 触发：合并检查点车道 comment `520a9023`（`git merge-tree master(0f9fe9997) 4ddf7df6` ⇒ CONFLICT
  `mir2-rs/tests/parity/whitelist.md`：master 侧 T-47 段与本片 T-46 段同插入位）。

## 本片做了什么

**只做解冲突 + 重跑**：`git merge origin/master`，唯一冲突 `whitelist.md` 按**并集**解
（T-47 段在前、T-46 段在后；无手工改内容），并在合并树上重跑本卡全部自检与四门禁。

## 判据与读数

| 判据 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| 比较器 selftest | `python -I mir2-rs/tests/parity/m3/guild_castle_diff.py selftest` | **36/36** rc=0 | `02-guildcastle-diff-selftest.txt` |
| 入口无栈自检 | `run_guild_castle_pair.ps1 -SelfCheckNoStack` | **19/19** rc=0（fail-closed/改坏必红/开关耦合/零端口） | `03-run-guildcastle-pair-selfcheck.txt` |
| BotSrv 生产函数 | `BotSrv.exe --selftest-guildcastle` | **GREEN** rc=0 | `04-botsrv-guildcastleseq-selftest.txt` |
| run_bots 收尾计数 | `tools/botload/run_bots.ps1 -SelfCheckNoStack` | **GREEN** rc=0 | `05-run-bots-selfcheck.txt` |
| 四门禁 | fmt / clippy / workspace test / msgcodegen | fmt0 / clippy0 / **96 suites 971 passed 0 failed 21 ignored** / msgcodegen **604** | `06..09-gate-*.txt` |
| 边界 | `git diff --name-only origin/master HEAD -- src sql MirClient` | 仅 `src/BotSrv/**`（T-46 登记例外）；`sql`/`MirClient` 空 | `01-merge-and-boundary.txt` |
| 边界（宿主面） | `-- mir2-rs/crates mir2-rs/bins mir2-rs/protocol` | **空** | `01-merge-and-boundary.txt` |

## 冲突解（原文）

`whitelist.md` 冲突区：master 侧 = `## M3⑤ 商店接线登记 … \| T-47 \|`；本片侧 = `## M3⑧ 登记 … \| T-46 \|`。
解 = 保留两侧，`T-47 段` 在前、`T-46 段` 在后（并集，无内容改写）。

## 文件

| 文件 | 内容 |
| --- | --- |
| `00-commands.txt` | 命令四件套 |
| `01-merge-and-boundary.txt` | merge 说明 + 边界 + 对 master 的 diff 清单 |
| `02..05-*.txt` | 比较器 selftest / 入口无栈自检 / BotSrv selftest / run_bots 自检 |
| `06..09-gate-*.txt` | 四门禁原始输出（合并树） |
| `SHA256SUMS.txt` | 仓库 LF/git-blob 口径校验清单 |

## 残留 / 承接（不变）

- 真机两侧 `run_guild_castle_pair.ps1` diff=0 要 ⑧-3..⑧-9 实现 + `openmir2-m3-clock-injection` + 独占窗口。
- 本片**不 merge、不自签 review**；零起栈、零占端口。
