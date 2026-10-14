# evidence-m3-pet-slave-mutiny-frames-20261014

卡：`openmir2-m3-pet-slave-masterdie-wire`（M3⑨ 主人死/幽灵 1s 自投递 —— **残余帧面**）
作者：`openmir2-worker-20`（worktree `m3hp`，分支 `worktree-m3-selloff-dealoffform-recog`）
base（净差量口径）：`refs/heads/master` = walgit/master = origin/master = `9edfc745d48ec7edafcbcb9da3d9a6a0c19073ad`
代码提交：`c55dff977`（feat）；证据提交 = 本目录所在提交 = patch head。
关联：本卡前片（状态层，已合入 master `41e61029e`）、`openmir2-m3-pet-slave-lifecycle`（⑨-3）。

> **本 worktree 本地 `refs/heads/master` 缓冲在 `ad0592b19`（= 共享主检出 HEAD，与 walgit/master 分叉）。**
> 按「不许改共享主检出」纪律**未移动**该 ref；净差量/边界一律按 `9edfc745d`（= walgit/master）计。

tier：**R**（帧面 + 观察者 `MsgQueue` 优先级出队序 + AOI 接收集；真机同序列对拍才能证逐字节）
=> `window_required = true`。卡级真机 DoD 未跑 => **不据此收卡**。

## 一句话结论

`TickRecord::slave_self_deliveries` 此前在宿主**零消费者**（`grep -rn slave_self_deliveries bins/` = 0）
⇒ 宝宝叛变后名字不变色/不刷新，客户端看不到。本片在 `bins/game-svr` 接上
`RefNameColor → SM_CHANGENAMECOLOR(656)` 与 `RefShowName → SM_USERNAME(42)`，
并按观察者 `MsgQueue` 的优先级字节（`8090&0xFF=154 < 8101&0xFF=165`）复刻
「**所有色帧先于所有名帧**出队、宝宝序 = `SlaveList` 末尾往前」的出队序。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 锚（触发 `PlayObject.Base.cs:331-359/351-352`；帧产者 `BaseObject.cs:730-733/1174-1177`；接收集 `BaseObject.Message.cs:383-475`；优先级 `:98/181` + `ActorEntity.cs:16`；观察者支 `PlayObject.Message.cs:1662-1664/1677-1679`；`GetShowName`/`GetChrColor`；消息号） |
| `02-change-summary.txt` | 变更清单（`smframe::sm_changenamecolor`；`main::drain_slave_mutiny_downlink` + 调用点；出队序推导）+ 5 条登记 + 不覆盖项 |
| `03-tests.txt` | 3 条判据读数 + 3 条改坏必红 + 四门禁读数 + 真机 DoD `not_verified` |
| `04-boundary.txt` | 边界 / 纪律 / 端口 / 残留物 |
| `commands.txt` | 复现命令（隔离 `CARGO_TARGET_DIR`） |
| `gates/` | 四门禁输出（`10-fmt` / `11-clippy` / `12-msgcodegen` / `13-workspace-test`） |
| `redchecks/` | RC1–RC3 改坏必红（rc=101）+ `restored-green`（3/3 GREEN rc=0）+ `mutate.py` |
| `SHA256SUMS.txt` | 仓库字节（LF / git blob）口径清单 |

## 口径

`SHA256SUMS.txt` 按「仓库字节」记（`*.txt`/`*.md`/`*.py` 在本仓 `text eol=lf` => 即 LF 归一）；自检：

```
python -I tools/evidence-hash/verify_sha256sums.py verify  <本目录> --check-extra
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <本目录>
```
