-- G2a R2（runbook §B5 R2）：无技能 + 无戒指（Slot7 清空）
-- 期望登录入图：658,212,51,...（212 SM_DELMAGIC）
-- 粘滞对照：oracle 实测会把不可用技能删掉并落库 => 小退后 characters_magic 仍应为 0 行。
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');

DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;

UPDATE mir2_test_db.characters_item
   SET MakeIndex=0, StdIndex=0, Dura=0, DuraMax=0
 WHERE PlayerId=@pid AND Position=7;

-- expect: magic=0 行；item Slot7 => StdIndex=0
SELECT COUNT(*) AS magic_rows FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
SELECT Position,StdIndex,Dura,DuraMax FROM mir2_test_db.characters_item
 WHERE PlayerId=@pid AND Position=7;
