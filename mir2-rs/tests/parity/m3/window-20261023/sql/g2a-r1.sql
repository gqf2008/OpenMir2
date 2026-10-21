-- G2a R1（runbook §B5 R1）：无技能 + 火焰戒指 StdIndex=354 在 Slot7
-- 期望登录入图：658,210,51,...（210 SM_ADDMAGIC 在 658 之后、51 之前）
-- 每轮前先 restore base（runbook §2.4 / §B5「侧间还原」）。
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='aaa');

DELETE FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;

UPDATE mir2_test_db.characters_item
   SET MakeIndex=900005, StdIndex=354, Dura=5000, DuraMax=5000
 WHERE PlayerId=@pid AND Position=7;

-- expect: magic=0 行；item Slot7 => StdIndex=354 Dura>0
SELECT COUNT(*) AS magic_rows FROM mir2_test_db.characters_magic WHERE PlayerId=@pid;
SELECT Position,StdIndex,Dura,DuraMax FROM mir2_test_db.characters_item
 WHERE PlayerId=@pid AND Position=7;
