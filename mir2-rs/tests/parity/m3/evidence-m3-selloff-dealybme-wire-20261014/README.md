# evidence-m3-selloff-dealybme-wire-20261014

卡：`openmir2-m3-selloff-consignment-world`（M3⑩-1，**收口片 #2 / `@dealybme` 开窗接线**）
作者：`openmir2-worker-20`（worktree `m3hp`，分支 `worktree-m3-selloff-dealoffform-recog`）
base：`refs/heads/master` = `6dba8719d`（= origin/master = walgit/master）
代码提交：`0a1b83f7b`（本片）；本 patch 的 head 另**包含**第一张收口片的代码提交 `65d0b34f2`
（`SM_SENDDEALOFFFORM` 头字段口径修正），因两者改同一函数 `selloff::open_deal_off_form`。
**本 patch 取代旧 needs-review `b7cd2d3f`（= head `4a314ecce`）**，请按本 head 评审。
tier：**L**（纯 label 派发 + 帧字段口径，oracle 锚定；不触 RNG / 时序 / 落库 / 战斗数值）
⇒ `window_required = false`。卡级真机两侧 DoD 仍未跑 ⇒ 不据此收卡。

## 文件

| 文件 | 内容 |
| --- | --- |
| `01-oracle-anchors.txt` | oracle 锚（`Merchant.cs:930-934` label 链 + `:1038-1056` 三支 + `PlayObject.Message.cs:1722-1724/2046-2048` 出队映射 + `CommandMessage` 12B 布局） |
| `02-change-summary.txt` | 变更清单（selloff.rs 三支 + merchant_say；main.rs handle_shop_cmd 接线）+ 未做登记 |
| `03-tests.txt` | 3 条判据（2 selloff + 1 进程级）+ 读数 + 3 条改坏必红 |
| `04-boundary.txt` | 边界 / 纪律 / 端口 |
| `commands.txt` | 复现命令（隔离 `CARGO_TARGET_DIR`） |
| `gates/` | 四门禁输出（fmt / clippy / msgcodegen / workspace test） |
| `redchecks/` | RC1/RC2/RC3 改坏必红（mutated + restored） |
| `SHA256SUMS.txt` | 仓库字节（LF / git blob）口径清单 |

## 口径

`SHA256SUMS.txt` 按「仓库字节」记（`*.txt`/`*.md` 在本仓 `text eol=lf` ⇒ 即 LF 归一）；
自检：

```
python -I tools/evidence-hash/verify_sha256sums.py verify  <本目录> --check-extra
python -I tools/evidence-hash/verify_sha256sums.py eolcheck <本目录>
```
