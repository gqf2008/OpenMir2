# M2 会话生命周期 · O-3（每网关各踢一个）—— 2026-10-12

- 卡：`openmir2-m2-session-lifecycle`（owner `openmir2-worker-11`）
- 裁决：owner `cb0c70fd4f51053accbdd8a52aeaa9664ac24678`（**不登记偏差，按 oracle 1:1 修**）
- 写集：`mir2-rs/bins/game-svr/src/main.rs`（不起栈、不占端口）

## 1. 改什么

oracle `TCPNetChannel.KickUser`（`src/M2Server/Net/TCP/TCPNetChannel.cs:226-284`，见本目录
`oracle-TCPNetChannel-KickUser.txt`）：**外层按 `_gameGates[i]` 逐网关**，内层扫该网关 `UserList`，
命中 `Account`（OrdinalIgnoreCase）或 `SessionID` 的第一项 ⇒ `DeleteHuman` + `SysMsg` + 置
`BoEmergencyClose/BoSoftClose` + `break`（内层）⇒ 语义 = **每个网关最多踢一个**。

Rust 原先 `online_find_for_kick` 取**全局首个**（`Option`）——只在「同一账号经两个网关各有一条会话」时分叉。
按 owner 裁决改为**按 `gate_idx` 分桶、每桶首个**（`Vec<OnlineEntry>`），`handle_kick_account` 逐条处置。

## 2. 判据与证据

```powershell
# 绿：两条用例（既有 SM_SYSMESSAGE + 新增每网关一个）都过
cargo test -p game-svr --offline account_kick      # ⇒ 2 passed / 0 failed
# 红检：把 online_find_for_kick 改回「全局首个」（targets.truncate(1)）⇒ 新用例必 FAILED
```

| 文件 | 内容 |
| --- | --- |
| `test-green.txt` | 绿跑原始输出：`account_kick_targets_every_gateway_once ... ok` / `2 passed; 0 failed` |
| `test-red.txt` | 红检原始输出：`account_kick_targets_every_gateway_once ... FAILED`（rc=101） |
| `oracle-TCPNetChannel-KickUser.txt` | 口径来源（oracle 源码 226–284 行逐行） |
| `SHA256SUMS.txt` | 上列文件 sha256 |

## 3. 新建模用例（`account_kick_targets_every_gateway_once`）

构造：同账号 `acct1` 在网关 0（`lb1a`/socket 901）与网关 1（`lb1b`/socket 902）各一条会话。
断言：两条**都**收到 `SM_SYSMESSAGE`、两条**都**进软关名单；`drain_soft_close` 后两条都
`WorldStage::Left`、在线登记清空。红检（全局首个）⇒ 网关 1 收不到 `SM_SYSMESSAGE` ⇒ FAILED。
