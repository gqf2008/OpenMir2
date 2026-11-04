-- ============================================================================
-- T-277 马牌物品数据（卡 openmir2-t282-mount-server）
--
-- 作用：向 `stditems` 插入 12 个马牌模板。服务端 `@OnHorse` 从背包中找
--       `StdMode = 18 AND Shape ∈ 1..=12` 的第一块，取 `Shape` 作为 `mount_id`。
--
-- 列口径（书面理由）：
--   Id        = 1001..1012（现有数据 max(Id)=1000，AUTO_INCREMENT=1001 ⇒ 不撞既有行）
--   Name      = 马牌01..马牌12（varchar(20)，UTF-8）
--   StdMode   = 18（冻结客户端 ClFunc.pas 的 `18: Result := U_HORSE`；仅作身份标记，
--               本实现**不**把马牌装到 Position=15——C# oracle 读侧 HumItems[15] 会越界）
--   Shape     = mount_id = 1..12（Mount 库 = Shape-1 对应 Crystal.Assets/ClientData/Mount/00..11.Lib；
--               0 保留给「下马」，见 whitelist T-277）
--   Weight    = 1（占用背包重量；避免 0 重量的特殊判定）
--   DuraMax   = 0（马牌无耐久，不参与掉耐久路径）
--   Price     = 0 / Stock = 0（不出售、不进入商城/寄售；避免误触发经济系统）
--   ImgIndex  = 0（冻结 Delphi 客户端无马牌图标槽；Zig 客户端的坐骑渲染在 T286 用独立图层）
--
-- 幂等：每条 `INSERT ... SELECT ... WHERE NOT EXISTS (Id=...)`；重复执行是 no-op。
-- 不覆盖任何既有行（Id 冲突时整条跳过，不 UPDATE）。
--
-- 应用（开发期只连 mir2_test_db；**禁止写生产库**）：
--   mysql.exe -h127.0.0.1 -uroot -p mir2_test_db < sql/migrations/T-277-mount-plates.sql
--   mysql.exe -h127.0.0.1 -uroot -p mir2_test_db -e \
--     "SELECT Id,Name,StdMode,Shape FROM stditems WHERE Id BETWEEN 1001 AND 1012 ORDER BY Id;"
--
-- 回滚：见 sql/migrations/T-277-mount-plates-rollback.sql
-- ============================================================================

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1001, '马牌01', 18, 1, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1001);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1002, '马牌02', 18, 2, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1002);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1003, '马牌03', 18, 3, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1003);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1004, '马牌04', 18, 4, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1004);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1005, '马牌05', 18, 5, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1005);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1006, '马牌06', 18, 6, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1006);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1007, '马牌07', 18, 7, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1007);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1008, '马牌08', 18, 8, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1008);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1009, '马牌09', 18, 9, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1009);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1010, '马牌10', 18, 10, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1010);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1011, '马牌11', 18, 11, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1011);

INSERT INTO stditems (Id, Name, StdMode, Shape, Weight, AniCount, Source, Reserved, ImgIndex, DuraMax, Price, Stock)
SELECT 1012, '马牌12', 18, 12, 1, 0, 0, 0, 0, 0, 0, 0 FROM DUAL
WHERE NOT EXISTS (SELECT 1 FROM stditems WHERE Id = 1012);

-- 应用后自检：应得 12 行，Id=1001..1012，Shape=1..12。
SELECT Id, Name, StdMode, Shape FROM stditems
WHERE Id BETWEEN 1001 AND 1012 ORDER BY Id;
