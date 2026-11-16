-- 卡：openmir2-playerid-indexes（D-* 偏差卡，Authorized 2026-11-17）
-- 目的：给 DBSrv 取档（每次进世界 7 条查询里的 3 张表）加 PlayerId 非唯一索引，
--       把「全表扫描」降为索引查找。**不改列/表结构/数据语义**，可 DROP 回滚。
--
-- 依据（2026-11-17 实测，mir2_bf_db）：单次取档 7 条 SQL 合计 ≈ 69.6 ms，
--   其中 characters_bagitem 331,016 行 28.6 ms、characters_storageitem 359,800 行 32.2 ms、
--   characters_item 93,548 行 8.3 ms（EXPLAIN 均为 type=ALL）；其余 4 条（含主键查找）合计 < 0.7 ms。
-- 回滚：见文件末尾 DROP INDEX 段（注释形式保留）。
--
-- 注意：本文件只建索引，不建表；部署时对目标库执行一次即可（幂等：IF NOT EXISTS 由 MySQL 8/MariaDB 支持）。

CREATE INDEX `IX_characters_bagitem_PlayerId`     ON `characters_bagitem` (`PlayerId`);
CREATE INDEX `IX_characters_storageitem_PlayerId` ON `characters_storageitem` (`PlayerId`);
CREATE INDEX `IX_characters_item_PlayerId`        ON `characters_item` (`PlayerId`);

-- 回滚（需要时手工执行）：
-- DROP INDEX `IX_characters_bagitem_PlayerId`     ON `characters_bagitem`;
-- DROP INDEX `IX_characters_storageitem_PlayerId` ON `characters_storageitem`;
-- DROP INDEX `IX_characters_item_PlayerId`        ON `characters_item`;
