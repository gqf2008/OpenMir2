# 证据：M3⑤-4 收口 —— `ClientUserBuyItem` 的 `Dealing` 前置门

卡：`openmir2-m3-shop-dealing-gate`（T-47 ⑦ 收口）
分支：`worktree-m3-shop-dealing-gate`
base / head：`338a8996e` → `cba56414f`（base = ⑤-4 第②片 rebase 后 head，见同批 ⑤-4 交付）
tier：L（纯分支/状态门补齐，无 RNG/时序/落库/数值改动）· window_required=false

## 做了什么（一行）

oracle `PlayObject.Operate.cs:196-199` 的 `if (Dealing) { return; }` 在 Rust `handle_shop_cmd`
缺席；⑦-3/⑦-4 落地后 `Dealing` 不再恒 `false` ⇒「交易中买货/拉详情」是可达行为差异。
本片在 `FindMerchant` 之前、只对 `CM_USERBUYITEM | CM_USERGETDETAILITEM` 补该门
（`Message.cs:897-902` 两条共用同一 oracle 方法体），其余五条商店命令不加（oracle 无）。

## 文件

| 文件 | 内容 |
|---|---|
| `00-oracle-anchors.txt` | oracle 锚（`Message.cs:897-902` / `Operate.cs:196-200` / 对照 `:114`/`:162`） |
| `01-scope-and-writeset.txt` | base/head、净差量、写集与证据目录口径说明 |
| `02-newtest.txt` | 新用例 `shop_buy_and_detail_blocked_while_dealing` 的 GREEN 读数与断言 |
| `03-redcheck-dealing-gate-mutated.txt` | 改坏必红：删门 ⇒ FAILED rc=101（首条断言红） |
| `03-redcheck-dealing-gate-restored.txt` | 还原 ⇒ GREEN，`git status` 空 |
| `03-redchecks.txt` | 红检口径与可回滚点 |
| `04-gates.txt` + `04a..04d` | 四门禁（fmt / clippy / workspace test / msgcodegen）原始件 |
| `05-boundary.txt` | 冻结面零改动 |
| `SHA256SUMS.txt` | 仓库字节口径清单（`tools/evidence-hash/verify_sha256sums.py`） |

## 判据（机器可判）

1. `trade_sessions[chr].dealing=true` ⇒ `CM_USERBUYITEM` 与 `CM_USERGETDETAILITEM` 均**零帧**、
   玩家 `gold`/`item_list` 与商人库存**全不变**（oracle 零帧零副作用）；
2. 正对照 `dealing=false` ⇒ 买入成功（回帧 + 扣金 + 加背包 + 库存减）；
3. 删门 ⇒ 用例必红（rc=101）；还原 ⇒ GREEN。
