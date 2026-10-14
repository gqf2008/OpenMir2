# evidence-m3-pet-slave-masterdie-wire-20261014

卡：`openmir2-m3-pet-slave-masterdie-wire`（M3⑨ 主人死/幽灵 1s 自投递驱动）
作者：`openmir2-worker-20`（worktree `m3hp`，分支 `worktree-m3-selloff-dealoffform-recog`）
base（净差量口径）：`refs/heads/master` = walgit/master = origin/master = `f0a9ab911ddc84846b69f0a3541390e81dba977a`
代码提交：`a6359cd78`（feat）；证据提交 = 本目录所在提交 = patch head。
关联：`openmir2-m3-pet-slave-lifecycle`（⑨-3，已合入 master，`slave.rs:44-46` 登记「自投递队列缺失」）、
`openmir2-m3-pet-slave-clone46`（review O-1 同题）。

> **本 worktree 本地 `refs/heads/master` 缓冲在 `ad0592b19`（= 共享主检出 HEAD，与 walgit/master 分叉）。**
> 按「不许改共享主检出」纪律**未移动**该 ref；净差量/边界一律按 `f0a9ab911`（= walgit/master）计。

tier：**R**（触 RNG 抽数序列 `Random(MasterDieMutinyRate)` + 时序 1000ms + 实体状态面）
=> `window_required = true`。卡级真机 DoD 未跑 => **不据此收卡**。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 锚（触发点 `PlayObject.cs:1995-1996/1999-2005/3422-3528`；队列 `BaseObject.Message.cs:188-206/334-352`；投递 `PlayObject.Message.cs:2116-2121` + `PlayObject.Base.cs:331-357/361-369`；次序 `PlayObject.Message.cs:360-364/765` + `BaseObject.Base.cs:208-217`） |
| `02-change-summary.txt` | 变更清单（world.rs 延时队列/触发点/开关；slave.rs 常量+API+3 判据；main.rs 生产接线）+ BUG-REPLICA 第 4 条 + 未做登记 |
| `03-tests.txt` | 3 条判据读数 + 4 条改坏必红 + 四门禁读数 + 真机 DoD 未跑登记 |
| `04-boundary.txt` | 边界 / 纪律 / 端口 / 残留物 |
| `commands.txt` | 复现命令（隔离 `CARGO_TARGET_DIR`） |
| `gates/` | 四门禁输出（`10-fmt` / `11-clippy` / `12-msgcodegen` / `13-workspace-test`） |
| `redchecks/` | RC1–RC4 改坏必红（mutated rc=101）+ restored-green（45 passed rc=0） |
| `SHA256SUMS.txt` | 仓库字节（LF / git blob）口径清单 |

## 口径

`SHA256SUMS.txt` 按「仓库字节」记（`*.txt`/`*.md` 在本仓 `text eol=lf` => 即 LF 归一）；自检：

```
python -I tools/evidence-hash/verify_sha256sums.py verify  <本目录> --check-extra
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <本目录>
```

## 一句话结论

oracle 的 `Die()`/`KickException()`/`MakeGhost()` 各自 `SendSelfDelayMsg(...,1000)`；本片把 ⑨-3 已实现但
**无生产调用点**的 `process_slave_mutiny`/`process_slave_ghost` 接上：`World` 增加 1000ms 自身延时消息队列，
`settle_deaths` 里对玩家死亡入队，`tick` 在清表前到点投递（`clock_ms >= deliver_at_ms`）。逐字保留
`!Ghost`/`!Death` 两道门 ⇒ 复刻 `RM_MASTERDIEGHOST` 恒不可达（BUG-REPLICA 第 4 条）。出帧归宿主，登记未做。
