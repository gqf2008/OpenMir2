# DEV-SELECHR-ACQUIRE-SEMANTICS — 选角复用一条连接后的 acquire 失败语义

> 登记日：2026-11-26。来源：`card/selectchr-roundtrip`（整条选角/查角复用一条连接的改动）。
> **本文件不在 parity 台账（`mir2-rs/tests/parity/whitelist.md`）内**，也不进覆盖率机器清单
> `docs/issues/parity-gap-*.json`；是这两者之外的单独偏差记录。

## 差异

改动把选角（`CM_SELCHR`）与查角（`CM_QUERYCHR`）从「每条 SQL 各自 `pool.acquire()`」改为
「整条请求 `Db::acquire()` 一次，后续语句在同一连接上自动提交」。由此产生**两处**与旧实现
不同、但对外仍同形的失败语义：

1. **唯一 acquire 失败（含 15 s 超时）⇒ 一条语句都不执行。**
   旧实现逐条 acquire：第 k 条的 acquire 失败时，前 k−1 条可能**已提交**（两条 `SelectID`
   UPDATE 是自动提交），即失败时可能已**部分写库**；新实现在最前面失败，库里没有任何改动。
   对外（SelGate 侧）两边都是 Err、不回包。
2. **连接中途断开 ⇒ 后续语句不再各自重新 acquire。**
   旧实现每条语句重新 acquire，断连后**可能换到一条健康连接**继续；新实现整条共用一条连接，
   断连即整条失败。对外同样是不回包。

方向上是 fail-closed 更早、更干净，但**不是逐字节等价**。

## 可达性

- 差异 1 的“部分写”旧语义**可达但无法稳定构造**：可达条件 = 前一条 UPDATE 已提交、其后的
  acquire 超时/连接断开、最终 `query_name_and_map` 无从执行 ⇒ 走 Err 不回包但 `SelectID`
  已落库。注意：把池**在入口就关闭**并不能复现——旧实现的错误大多被吞（`record_get_by` /
  `record_update_by` 记日志后继续），第一条 SQL 的 acquire 就失败，同样一条语句都不执行。
- 差异 2 取决于网络/MySQL 异常，无法在测试里稳定注入。

## 护栏牙（已在 `select_roundtrip_tests`）

- `select_chr_falls_back_to_per_row_when_batch_query_fails`（池关闭）断言：① 无回包
  ② `characters_indexes.SelectID` 未被改动。
  **注释写明：此牙在新旧实现下都绿，是回归护栏、不是等价性红证。**

## 若将来要拿“部分写”的真红（本轮不做）

`connect_with(dsn, max_connections = 1, acquire_timeout = 短)` + 后台任务在两语句之间抢占唯一
连接，使**第二次 acquire** 超时；断言旧实现 `SelectID` 已变、新实现未变。或使用 sqlx
`before_acquire` 钩子做确定性注入。成本高、易 flaky，2026-11-26 裁决不做。
