# 三张取档表补 `PlayerId` 索引（2026-11-17）

## 0. 结论（一句话）

DBSrv 每次进世界要跑 7 条 SQL，其中 3 张表**没有 `PlayerId` 索引** ⇒ 全表扫描，合计 **≈68 ms**；
加索引后 **≈0.6 ms（≈110×）**。这一项就是「进世界速率」的主项：串行取档下吞吐上界 = 1000ms ÷ 68ms ≈ **14.7/s**，
与 C# BotSrv 基线 **12.81/s** 同量级；botfleet 1000 档（20 s 阶段超时）封顶 **373 ≈ 20000ms ÷ 68ms** 的 294–373 区间。

## 1. 实测（同一库、同一数据量）

| 项 | 数值 |
| --- | --- |
| 表规模 | `characters_bagitem` 331,016 行 / `characters_storageitem` 359,800 行 / `characters_item` 93,548 行 / `characters` 6,995 行 |
| 加索引前（3 条 SQL 合计，5 轮） | 0.0672 / 0.0676 / 0.0681 / 0.0682 / 0.0694 s ⇒ **p50 ≈ 68 ms**，EXPLAIN 三表 `type=ALL` |
| 加索引后（同 5 轮） | 0.0006 / 0.0006 / 0.0006 / 0.0007 / 0.0010 s ⇒ **p50 ≈ 0.6 ms**，EXPLAIN 三表走 `IX_characters_*_PlayerId` |

## 2. 迁移文件

`sql/migrations/D-20261117-playerid-indexes.sql`：三条**非唯一**索引 `IX_characters_{bagitem,storageitem,item}_PlayerId`；
**不改列/表结构/数据语义**；文件末尾注释保留 `DROP INDEX` 回滚段。

## 3. 目标库与授权边界（重要）

- 本卡**只在测试库 `mir2_bf_db` 应用**（已应用，三个索引在位）。
- **主栈库 `mir2_db` 不在本卡授权范围**：那是生产数据所在库，任何 DDL 须单独提出并按授权口径处理。
- **口径差说明**：核查里引用的 `runtime\DBSrv\dbsrv.conf:21` 是**主检出 runtime 的影子配置**（其连接串写 `database=mir2_db`）；
  而**实测与索引都发生在 m1-stack 的 `mir2_bf_db`**。两者不要混读——`runtime` 是构建产物副本，不是本卡的执行目标。

## 4. 不是墙的那一项：连接池

`src/Storeages/DBSvr.Storage.MySQL/DBContext.cs:23` 每查询新建 `MySqlConnection`；连接串无池参数 ⇒ MySqlConnector 缺省
`Pooling=true / Max Pool Size=100 / Connection Timeout=15s`。但 `src/DBSrv/Services/Impl/DataService.cs:53-68` 在**收包回调里同步**
处理、game-svr 到 6000 面是**单连接** ⇒ **在飞取档恒为 1**，池深度用不到。实测佐证：DBSrv 日志本轮 `GetQryChar -> True` 恰 **1000** 条，
无超时、无非法请求。⇒ **第二道墙不是池，是「串行收包 × 单次取档耗时」**（索引后约 1.3ms/次；若不够，再评估 6000 面并发化——独立卡）。

## 5. 门禁口径

本改动**只有 SQL 迁移文件与本文档**，**不涉 cargo / dotnet**；按 SQL/文档类门禁执行（`git diff --check`、文档锚门）。
