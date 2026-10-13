# M3⑨-5 英雄/宠物夹具 —— 解 whitelist.md 冲突重交（merge master）证据包

- 卡：`openmir2-m3-hero-pet-fixture-botsrv`（父卡 `openmir2-m3-hero-pet-align`；T-45）
- 车道：`openmir2-worker-5` / worktree `m3cs` / branch `worktree-m3-hero-pet-fixture-botsrv`
- base：`origin/master 67e9f6c3e097da834ef5769eff1c5415e97bb458`
- head：`7b999382aeeb3145b8e06b717a87e1de360ac3fb`（merge 提交；取代旧 head `ca640e32`）
- 触发：合并检查点车道 comment `cb7e3c9a`（`git merge-tree master(67e9f6c3e) ca640e32` ⇒ CONFLICT
  `mir2-rs/tests/parity/whitelist.md`：master 侧 T-47 段与本片 T-45 段同插入位）。

## 本片做了什么

**只做解冲突 + 重跑**：`git merge origin/master`，唯一冲突 `whitelist.md` 按**并集**解
（T-47 段在前、T-45 段在后；无手工改内容），并在合并树上重跑本卡全部自检与四门禁。

## 判据与读数

| 判据 | 命令 | 读数 | 原始件 |
| --- | --- | --- | --- |
| 比较器 selftest | `python -I mir2-rs/tests/parity/m3/hero_pet_diff.py selftest` | **36/36** rc=0 | `02-heropet-diff-selftest.txt` |
| 入口无栈自检 | `run_hero_pet_pair.ps1 -SelfCheckNoStack` | **19/19** rc=0 | `03-run-heropet-pair-selfcheck.txt` |
| BotSrv 生产函数 | `BotSrv.exe --selftest-heropet` | **GREEN** rc=0 | `04-botsrv-heropetseq-selftest.txt` |
| run_bots 收尾计数 | `tools/botload/run_bots.ps1 -SelfCheckNoStack` | **GREEN** rc=0 | `05-run-bots-selfcheck.txt` |
| 四门禁 | fmt / clippy / workspace test / msgcodegen | fmt0 / clippy0 / **96 suites 971 passed 0 failed 21 ignored** / msgcodegen **604** | `06..09-gate-*.txt` |
| 边界 | `git diff --name-only origin/master HEAD -- src sql MirClient` | 仅 `src/BotSrv/**`（T-45 登记例外）；`sql`/`MirClient` 空 | `01-merge-and-boundary.txt` |
| 边界（宿主面） | `-- mir2-rs/crates mir2-rs/bins mir2-rs/protocol` | **空** | `01-merge-and-boundary.txt` |

## 冲突解（原文）

`whitelist.md` 冲突区：master 侧 = `## M3⑤ 商店接线登记 … \| T-47 \|`；本片侧 = `## M3⑨ 登记 … \| T-45 \|`。
解 = 保留两侧，`T-47 段` 在前、`T-45 段` 在后（并集，无内容改写）。

## 残留 / 承接（不变）

- 真机两侧 `run_hero_pet_pair.ps1` diff=0 要 ⑨-2/⑨-3 落地 + 独占窗口。
- 本片**不 merge、不自签 review**；零起栈、零占端口。
