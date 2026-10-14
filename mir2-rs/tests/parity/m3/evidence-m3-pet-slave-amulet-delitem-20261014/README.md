# evidence-m3-pet-slave-amulet-delitem-20261014

卡：`openmir2-m3-pet-slave-amulet-delitem-frame`（M3⑨-2 召唤面 —— **护身符扣光删除帧面**）
作者：`openmir2-worker-20`（worktree `m3hp`，分支 `worktree-m3-pet-slave-amulet-delitem`）
base（净差量口径）：`refs/heads/master` = walgit/master = origin/master = `9edfc745d48ec7edafcbcb9da3d9a6a0c19073ad`
代码提交：本目录的父提交（`feat(m3-9-2): …`）；证据提交 = 本目录所在提交 = patch head。

> **本 worktree 本地 `refs/heads/master` 缓冲在 `ad0592b19`（= 共享主检出 HEAD，与 walgit/master 分叉）。**
> 按「不许改共享主检出」纪律**未移动**该 ref；净差量/边界一律按 `9edfc745d`（= walgit/master）计。
> 本分支**直接从 `9edfc745d` 分出**（不含本车道尚未合入的宝宝叛变帧面补丁 `86c78daee`；两者写集不相交）。

tier：**L**（纯帧编码 + 已建模状态支；不触 RNG 抽数序列 / 时序 / 落库 / 战斗数值）。
`window_required = false` ⇒ 审查者在钉住的 commit 上逐行对 oracle + 复跑判据即可 `approve`。
卡级真机 DoD（`run_hero_pet_pair.ps1` 护身符扣光 step 同序列 diff=0）归**窗口线**，本目录记 `not_verified`。

## 一句话结论

`MagicBase.UseAmulet` 的扣光支（`MagicBase.cs:121-126`）在 oracle 里发**立即** `SM_DELITEM(202)`
（`PlayObject.Base.cs:1648-1665`）。Rust 此前只落了状态（`Dura=0` + 清槽），帧面登记未接
（`bins/game-svr/src/slave.rs` 模块头 + `main.rs` 结算点注释）⇒ 客户端背包里的护身符**不会消失**。
本片补 `smframe::sm_delitem` + `slave::deleted_amulet_body`，并在 `CM_SPELL` 的护身符结算处
**清槽前**编发（oracle 先置 `Dura=0` 再 `SendDelItems` ⇒ 帧内 `Dura` 恒 0）。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 锚（`MagicBase.cs:109-126` 语句序；`PlayObject.Base.cs:1648-1665` 帧构造；`Messages.cs:222`；三个触发点 `MagicManager.cs:319-325/450-454/462-466`；`ClientItem.WritePacket`） |
| `02-change-summary.txt` | 变更清单（三个函数/调用点）+ 口径（与 `SM_ADDITEM` 的差异、`Dura=0` 由来）+ 不覆盖项 |
| `03-tests.txt` | 3 条判据读数 + 3 条改坏必红（rc=101）+ 四门禁读数 + 真机 DoD `not_verified` |
| `04-boundary.txt` | 边界 / 纪律 / 端口 / 残留物 |
| `commands.txt` | 复现命令（隔离 `CARGO_TARGET_DIR`） |
| `gates/` | 四门禁输出（`10-fmt` / `11-clippy` / `12-msgcodegen` / `13-workspace-test`） |
| `redchecks/` | RC1–RC3 改坏必红（rc=101）+ `restored-green`（GREEN rc=0）+ `mutate.py` |
| `SHA256SUMS.txt` | 仓库字节（LF / git blob）口径清单 |

## 口径

`SHA256SUMS.txt` 按「仓库字节」记（`*.txt`/`*.md`/`*.py` 在本仓 `text eol=lf` ⇒ 即 LF 归一）；自检：

```
python -I tools/evidence-hash/verify_sha256sums.py verify  mir2-rs/tests/parity/m3/evidence-m3-pet-slave-amulet-delitem-20261014 --check-extra
python -I tools/evidence-hash/verify_sha256sums.py eolcheck mir2-rs/tests/parity/m3/evidence-m3-pet-slave-amulet-delitem-20261014
```
