-- ============================================================================
-- T-277 马牌物品数据回滚（卡 openmir2-t282-mount-server）
--
-- 只删除「本迁移插入、且仍保持原形态」的行（Id + 名称 + StdMode/Shape 三重匹配），
-- 避免误删他人后来在 1001..1012 上创建的行。幂等（删不到是 no-op）。
--
-- 应用（开发期只连 mir2_test_db；**禁止写生产库**）：
--   mysql.exe -h127.0.0.1 -uroot -p mir2_test_db < sql/migrations/T-277-mount-plates-rollback.sql
--
-- 回滚后自检：应得 0 行。
-- ============================================================================

DELETE FROM stditems
WHERE (Id = 1001 AND Name = '马牌01' AND StdMode = 18 AND Shape = 1)
   OR (Id = 1002 AND Name = '马牌02' AND StdMode = 18 AND Shape = 2)
   OR (Id = 1003 AND Name = '马牌03' AND StdMode = 18 AND Shape = 3)
   OR (Id = 1004 AND Name = '马牌04' AND StdMode = 18 AND Shape = 4)
   OR (Id = 1005 AND Name = '马牌05' AND StdMode = 18 AND Shape = 5)
   OR (Id = 1006 AND Name = '马牌06' AND StdMode = 18 AND Shape = 6)
   OR (Id = 1007 AND Name = '马牌07' AND StdMode = 18 AND Shape = 7)
   OR (Id = 1008 AND Name = '马牌08' AND StdMode = 18 AND Shape = 8)
   OR (Id = 1009 AND Name = '马牌09' AND StdMode = 18 AND Shape = 9)
   OR (Id = 1010 AND Name = '马牌10' AND StdMode = 18 AND Shape = 10)
   OR (Id = 1011 AND Name = '马牌11' AND StdMode = 18 AND Shape = 11)
   OR (Id = 1012 AND Name = '马牌12' AND StdMode = 18 AND Shape = 12);

SELECT COUNT(*) AS remaining FROM stditems WHERE Id BETWEEN 1001 AND 1012;
