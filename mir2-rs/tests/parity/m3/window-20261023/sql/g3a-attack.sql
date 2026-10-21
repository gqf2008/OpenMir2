-- G3a attack 摆位（runbook §B7）：(605,578)、Level 30、HP=MaxHP-2（两侧同值）
-- 不直接改 MaxHP（会被 RecalcAbilitys 覆盖）；MaxHP<3 的角色不得用本脚本。
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');

UPDATE mir2_test_db.characters SET MapName='0', CX=605, CY=578 WHERE ChrName='aaa';
UPDATE mir2_test_db.characters SET Level=30 WHERE ChrName='aaa';
UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
   SET a.Level=30 WHERE c.ChrName='aaa';
UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
   SET a.Hp=a.MaxHP-2 WHERE c.ChrName='aaa' AND a.MaxHP>=3;

-- expect: 0 | 605 | 578 | Level=30 | Hp=MaxHP-2
SELECT MapName,CX,CY,Level FROM mir2_test_db.characters WHERE ChrName='aaa';
SELECT PlayerId,Level,Hp,MaxHP FROM mir2_test_db.characters_ablity WHERE PlayerId=@pid;
