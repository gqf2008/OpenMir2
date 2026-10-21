-- G1 受控登录摆位（runbook §2.2；只写 mir2_test_db）
-- 角色 winplay0：安全格 (311,666)，满血满蓝；不走路、不打怪。
-- expect: characters => 0 | 311 | 666 | Level=1 | Job=1
-- expect: characters_ablity => Hp=MaxHP、Mp=MAxMP
SET @pid=(SELECT Id FROM mir2_test_db.characters WHERE ChrName='winplay0');

UPDATE mir2_test_db.characters
   SET MapName='0', CX=311, CY=666
 WHERE Id=@pid;

UPDATE mir2_test_db.characters_ablity
   SET Hp=MaxHP, Mp=MAxMP
 WHERE PlayerId=@pid;

SELECT MapName,CX,CY,Level,Job FROM mir2_test_db.characters WHERE Id=@pid;
SELECT PlayerId,Level,Hp,MaxHP,Mp,MAxMP FROM mir2_test_db.characters_ablity WHERE PlayerId=@pid;
