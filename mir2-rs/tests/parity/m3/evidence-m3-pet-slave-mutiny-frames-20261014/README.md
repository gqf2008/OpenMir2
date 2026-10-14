# evidence-m3-pet-slave-mutiny-frames-20261014

卡：`openmir2-m3-pet-slave-masterdie-wire`（M3⑨ 主人死/幽灵 1s 自投递 —— **残余帧面**）
作者：`openmir2-worker-20`（worktree `m3hp`，分支 `worktree-m3-selloff-dealoffform-recog`）
base（净差量口径）：`refs/heads/master` = walgit/master = origin/master = `3db79cae05261d5fcdb90a9fdd6afb8e0c78179f`
代码提交：`ecd455765`（rework fix）+ `a2c5a2bad`（首次 merge origin/master）+ `5e932b4eb`（本轮 head 前移，见下）；证据提交 = 本目录所在提交 = patch head。
关联：本卡前片（世界侧，已合入 master `16b208343`）、`openmir2-m3-pet-slave-lifecycle`（⑨-3）。

> **本 worktree 本地 `refs/heads/master` 缓冲在 `ad0592b19`（= 共享主检出 HEAD，与 walgit/master 分叉）。**
> 按「不许改共享主检出」纪律**未移动**该 ref；净差量/边界一律按 `3db79cae0`（= walgit/master）计。
>
> **head 前移（2026-10-14 tick59）**：按 merge 车道 `last-merge.md` §4 的建议，把本片从旧 base
> `f43364c45` 前移到当前 master `3db79cae0`（`git merge --no-ff origin/master` 自动合并零冲突；
> 与本片同改 `bins/game-svr/src/{main,slave,smframe}.rs` 的护身符片 `060e73f20` 已在 master）。
> 前移后 vs master 净差量**逐字不变**（4 .rs +604/-0 + 证据 21 件），rework 逻辑未改；四门禁 /
> 聚焦判据 / RC1–RC5 已在新 head 复跑（见 `gates/`、`redchecks/`）。head 变化须重审。

tier：**R**（帧面 + 观察者 `MsgQueue` 同优先级堆出队序 + AOI 接收集；真机同序列对拍才能证逐字节）
=> `window_required = true`。卡级真机 DoD 未跑 => **不据此收卡**。

**rework 说明（reviewer-6 F-1，`a34cf65b` request_changes）**：原实现把观察者 `MsgQueue`
同优先级出队序当成入队序（「两组内各按 mutinied 序」）。但 oracle 的 `PriorityQueue<SendMessage,byte>`
（`ActorEntity.cs:16`）在 .NET 8 是 **4 叉最小堆**（`Arity=4/Log2Arity=2`），同优先级出队序 ≠ 入队序
⇒ 名帧序与 oracle **相反**。本片逐操作复刻该 4 叉堆（`msgqueue::NetPriorityQueue`），并用
net8.0.26 实跑 `dotnet` 逐位对照（n=1..12）。

## 一句话结论

`TickRecord::slave_self_deliveries` 此前在宿主**零消费者** ⇒ 宝宝叛变后名字不变色/不刷新，
客户端看不到。本片在 `bins/game-svr` 接上
`RefNameColor → SM_CHANGENAMECOLOR(656)` 与 `RefShowName → SM_USERNAME(42)`，
并按观察者 `MsgQueue` 的**堆出队序**（.NET 8 4 叉堆；优先级字节 `8090&0xFF=154 < 8101&0xFF=165`）
复刻出帧序：`色(s2),色(s1),名(s1),名(s2)`（2 宝宝，mutinied `[s2,s1]`）。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 锚（触发 `PlayObject.Base.cs:331-359/351-352`；帧产者 `BaseObject.cs:730-733/1174-1177`；接收集 `BaseObject.Message.cs:383-475`；优先级 `:98/181` + `ActorEntity.cs:16`；观察者支 `PlayObject.Message.cs:1662-1664/1677-1679`；`GetShowName`/`GetChrColor`；消息号；**I) .NET 8 4 叉堆**） |
| `02-change-summary.txt` | 返工清单（`msgqueue.rs` 新增；drain 两循环→堆；用例改名/改期望）+ 6 条登记 + 不覆盖项 |
| `03-tests.txt` | 5 条判据读数 + RC1–RC5 改坏必红 + 四门禁读数 + 真机 DoD `not_verified` |
| `04-boundary.txt` | 边界 / 纪律 / 端口 / 残留物 |
| `commands.txt` | 复现命令（隔离 `CARGO_TARGET_DIR`） |
| `msgqueue-dotnet-ref.txt` | **net8.0.26 实跑 `.NET PriorityQueue` 出队序**（程序 + 输出，n=1..12） |
| `dotnet-pq8-source-excerpt.txt` | .NET 8 BCL `PriorityQueue.cs` 源摘（`Arity=4/Log2Arity=2` / Enqueue / RemoveRootNode / MoveUp / MoveDown） |
| `gates/` | 四门禁输出（`10-fmt` / `11-clippy` / `12-msgcodegen` / `13-workspace-test`） |
| `redchecks/` | RC1–RC5 改坏必红（rc=101）+ `restored-green` + `mutate.py` |
| `SHA256SUMS.txt` | 仓库字节（LF / git blob）口径清单（20 件 + 本清单） |

## 口径

`SHA256SUMS.txt` 按「仓库字节」记（`*.txt`/`*.md`/`*.py` 在本仓 `text eol=lf` => 即 LF 归一）；自检：

```
python -I tools/evidence-hash/verify_sha256sums.py verify  <本目录> --check-extra
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <本目录>
```
