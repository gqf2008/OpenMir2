-- G2a R3（runbook §B5 R3）：战士 MagicId=3（基本剑术）+ 木剑 StdIndex=207（Slot1），无戒指
-- 期望登录入图：658,51,...,752,621,211,811,46（621 + 211 SM_ADDMAGIC）
-- MagicId=1 是火球术（法师），不能用；战士用 3（Job=0 / NeedL1=7）。
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');

DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
INSERT INTO mir2_test_db.characters_magic (PlayerId,MagicId,Level,UseKey,CurrTrain)
VALUES (@pid,3,3,'F1',0);

UPDATE mir2_test_db.characters_item
   SET MakeIndex=900006, StdIndex=207, Dura=4000, DuraMax=4000
 WHERE PlayerId=@pid AND Position=1;

UPDATE mir2_test_db.characters SET Level=30 WHERE ChrName='aaa';
UPDATE mir2_test_db.characters_ablity a JOIN mir2_test_db.characters c ON a.PlayerId=c.Id
   SET a.Level=30 WHERE c.ChrName='aaa';

-- expect: magic 1 行 MagicId=3；item Slot1 => StdIndex=207；两处 Level=30
SELECT PlayerId,MagicId,Level,UseKey,CurrTrain FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
SELECT Position,StdIndex,Dura,DuraMax FROM mir2_test_db.characters_item
 WHERE PlayerId=@pid AND Position=1;
SELECT c.Level AS chr_level, a.Level AS abil_level
  FROM mir2_test_db.characters c JOIN mir2_test_db.characters_ablity a ON a.PlayerId=c.Id
 WHERE c.ChrName='aaa';
