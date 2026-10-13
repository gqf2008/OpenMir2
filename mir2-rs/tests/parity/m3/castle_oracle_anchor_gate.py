#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""castle_oracle_anchor_gate.py — M3⑧-6 第②片（`UserCastle.Initialize` 实体挂图）
oracle 锚一致性门（fail-closed）。

卡：`openmir2-m3-castle-file-load`（reviewer-6 M-1/M-2/O-1 收口片）。

## 为什么有这道门

reviewer-6 在 approve（`4b568b85` @`62fb829134f9`）里给了 **M-1**：⑧-6 第②片新增块的
doc-comment/README 锚行号是按**另一版本**抄的，与**冻结** `src/` 实际行号差 1–13 行，
而同一 patch 的 `05-oracle-anchors.txt` 行号却是对的 ⇒ 自相矛盾。本卡家族已有多次
「行号漂移」前科（⑥-4 / ⑧-2 等），散文改完仍会再漂 ⇒ 用机判把锚钉死。

## 判据（任一不满足 ⇒ rc=1）

对表内**每条**锚 `(anchor, cs_relpath, [(line, token), ...])`：

1. **写侧**：`anchor` 文本必须出现在本片两个落点之一 ——
   `mir2-rs/crates/world/src/castle.rs` 或本证据目录 `README.md`
   （把 `:113` 改回 `:111` ⇒ 注册的 `UserCastle.cs:113` 消失 ⇒ RED）；
2. **读侧**：冻结 `src/` 对应 `line` 必须命中 `token`（改表/改行号 ⇒ RED）。

3. **覆盖面**：`COVERED_MARKERS` 括出的区间（⑧-6 M-2 收口后 = **整个 castle.rs**，
   文件首行 → EOF）内，**每个**`文件.cs:NNN` 锚都必须在 `ANCHORS` 表内（新增锚不登记 ⇒ RED），
   且区间内**不得**留裸锚（必须写成 `文件.cs:NNN`，防 `:NNN` 在不同文件间被误读）。
4. **读侧 token 强度**（⑧-6 reviewer-6 O-1 / ⑧-10 reviewer-6 残余 Minor「结束行只有 `}` 的
   token 判据较弱」）：每条锚**整组**至少要有一个**非纯标点** token；整组只剩 `}`/`;`/`(`
   这类无信息量字符 ⇒ RED（防行号漂移时「只钉在一段空壳行上」假绿）。

`audit` 子命令与覆盖面同源，用于复算/清点（`check` 是判据面，`audit` 是清单面）。

## 用法

    python -I castle_oracle_anchor_gate.py check [--json out]
    python -I castle_oracle_anchor_gate.py audit [--json out]
    python -I castle_oracle_anchor_gate.py selftest
    python -I castle_oracle_anchor_gate.py mutation [--out DIR]

`audit` 是 `check` 的**覆盖面清点器**（⑧-6 reviewer-5 M-1：旧证据 `10-anchor-audit.txt` 由
仓外临时脚本产出、不可独立复算）⇒ 现在同一份逻辑住在本文件内，命令可复跑：打印覆盖区间、
已登记锚数、区间内**未登记**锚清单、**裸锚**清单。GREEN ⇔ 两张清单都空。

退出码：0 = GREEN；1 = RED。
"""
import argparse
import json
import os
import re
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.normpath(os.path.join(HERE, "..", "..", "..", ".."))
CASTLE_RS = os.path.join(HERE, "..", "..", "..", "crates", "world", "src", "castle.rs")
README_MD = os.path.join(
    HERE, "evidence-m3-castle-file-load-f2-20261013", "README.md")

UC = "src/M2Server/Castle/UserCastle.cs"
CC = "src/M2Server/Castle/CastleConf.cs"
AU = "src/Modules/SystemModule/Data/ArcherUnit.cs"
GC = "src/Modules/SystemModule/Conf/Model/GameSvrConf.cs"
# ⑧-7 模块头新纳入的 oracle 面（横向体检片）。
CO = "src/GameSrv/Npc/CastleOfficial.cs"
GO = "src/GameSrv/Npc/GuildOfficial.cs"
CSL = "src/Modules/GameCommand/Commands/ChangeSabukLordCommand.cs"
MS = "src/M2Server/M2Share.cs"
MSG = "src/OpenMir2/Messages.cs"
SC = "src/Modules/SystemModule/Conf/ServerConf.cs"
CCONST = "src/M2Server/Castle/CastleConst.cs"
PM = "src/M2Server/Player/PlayObject.Message.cs"
PO = "src/M2Server/Player/PlayObject.Operate.cs"

# (anchor_text_in_docs, cs_relpath, [(line, expected_substring), ...])
#
# `anchor_text` 必须逐字节出现在 castle.rs 或 README.md。
# `(line, token)` 逐条对冻结 src 的该行做 `token in line` 校验。
ANCHORS = [
    # ---- CastleConf.LoadConfig 的 [Defense] 段（entity specs 输入面）----
    ("CastleConf.cs:39", CC, [(39, 'ReadWriteString("Defense", "CastleMap", "3")')]),
    ("CastleConf.cs:45", CC, [(45, 'ReadWriteString("Defense", "CastlePlaceMap"')]),
    ("CastleConf.cs:46", CC, [(46, 'ReadWriteString("Defense", "CastleSecretMap"')]),
    ("CastleConf.cs:49-55", CC, [
        (49, "userCastle.MainDoor = new ArcherUnit();"),
        (53, 'ReadWriteBool("Defense", "MainDoorOpen", true)'),
        (55, "userCastle.MainDoor.BaseObject = null;")]),
    ("CastleConf.cs:56-61", CC, [
        (56, "userCastle.LeftWall = new ArcherUnit();"),
        (61, "userCastle.LeftWall.BaseObject = null;")]),
    ("CastleConf.cs:62-67", CC, [
        (62, "userCastle.CenterWall = new ArcherUnit();"),
        (67, "userCastle.CenterWall.BaseObject = null;")]),
    ("CastleConf.cs:68-73", CC, [
        (68, "userCastle.RightWall = new ArcherUnit();"),
        (73, "userCastle.RightWall.BaseObject = null;")]),
    ("CastleConf.cs:74-83", CC, [
        (74, "for (int i = 0; i < userCastle.Archers.Length; i++)"),
        (82, "userCastle.Archers[i] = objUnit;")]),
    ("CastleConf.cs:84-93", CC, [
        (84, "for (int i = 0; i < userCastle.Guards.Length; i++)"),
        (92, "userCastle.Guards[i] = objUnit;")]),
    ("CastleConf.cs:53", CC, [(53, '"MainDoorOpen"')]),
    # `LoadConfig` 的 [Defense] 段整体引用（DefenseUnitSpec / 测试夹具）。
    ("CastleConf.cs:39-93", CC, [(39, '"CastleMap"')]),
    # ---- UserCastle.Initialize 108-226 逐段 ----
    ("UserCastle.cs:108-226", UC, [(108, "public void Initialize()")]),
    ("UserCastle.cs:113", UC, [(113, "GetMapOfServerIndex(MapName) == M2Share.ServerIndex")]),
    ("UserCastle.cs:115-125", UC, [
        (115, "SystemShare.MapMgr.FindMap(PalaceMap)"),
        (121, "SystemShare.MapMgr.FindMap(SecretMap)")]),
    ("UserCastle.cs:127-128", UC, [
        (127, "CastleEnvir = SystemShare.MapMgr.FindMap(MapName)"),
        (128, "if (CastleEnvir != null)")]),
    ("UserCastle.cs:130-138", UC, [
        (130, "MainDoor.BaseObject = SystemShare.WorldEngine.RegenMonsterByName"),
        (135, "if (MainDoor.nStatus)"),
        (137, "((CastleDoor)MainDoor.BaseObject).Open();")]),
    ("UserCastle.cs:135-138", UC, [
        (135, "if (MainDoor.nStatus)"),
        (137, "((CastleDoor)MainDoor.BaseObject).Open();")]),
    ("UserCastle.cs:133-211", UC, [
        (133, "MainDoor.BaseObject.WAbil.HP = MainDoor.nHP;"),
        (205, "ObjUnit.BaseObject.WAbil.HP = Guards[i].nHP;")]),
    ("UserCastle.cs:142", UC, [(142, 'MainDoor.UnitObj = nil')]),
    ("UserCastle.cs:144-173", UC, [
        (144, "LeftWall.BaseObject = SystemShare.WorldEngine.RegenMonsterByName"),
        (154, "CenterWall.BaseObject = SystemShare.WorldEngine.RegenMonsterByName"),
        (164, "RightWall.BaseObject = SystemShare.WorldEngine.RegenMonsterByName")]),
    ("UserCastle.cs:152/162/172", UC, [
        (152, "MainDoor.sName"),
        (162, "LeftWall.sName"),
        (172, "CenterWall.sName")]),
    ("UserCastle.cs:144-153", UC, [(144, "LeftWall.BaseObject = SystemShare.WorldEngine")]),
    ("UserCastle.cs:154-163", UC, [(154, "CenterWall.BaseObject = SystemShare.WorldEngine")]),
    ("UserCastle.cs:164-173", UC, [(164, "RightWall.BaseObject = SystemShare.WorldEngine")]),
    ("UserCastle.cs:174-193", UC, [
        (174, "for (int i = 0; i < Archers.Length; i++)"),
        (177, "if (ObjUnit.nHP <= 0)"),
        (187, "((GuardUnit)ObjUnit.BaseObject).GuardDirection = 3;")]),
    ("UserCastle.cs:177/197", UC, [
        (177, "if (ObjUnit.nHP <= 0)"),
        (197, "if (ObjUnit.nHP <= 0)")]),
    ("UserCastle.cs:186", UC, [(186, "ObjUnit.BaseObject.Castle = this;")]),
    ("UserCastle.cs:187", UC, [(187, "GuardDirection = 3;")]),
    ("UserCastle.cs:194-211", UC, [
        (194, "for (int i = 0; i < Guards.Length; i++)"),
        (205, "ObjUnit.BaseObject.WAbil.HP = Guards[i].nHP;")]),
    ("UserCastle.cs:134/148/158/168/186", UC, [
        (134, "MainDoor.BaseObject.Castle = this;"),
        (148, "LeftWall.BaseObject.Castle = this;"),
        (158, "CenterWall.BaseObject.Castle = this;"),
        (168, "RightWall.BaseObject.Castle = this;"),
        (186, "ObjUnit.BaseObject.Castle = this;")]),
    ("UserCastle.cs:202-206", UC, [
        (202, "ObjUnit.BaseObject = SystemShare.WorldEngine.RegenMonsterByName")]),
    ("UserCastle.cs:228-232", UC, [
        (228, "private void LoadConfig()"),
        (230, "castleConf.LoadConfig(this);")]),
    ("UserCastle.cs:328-330", UC, [(328, "DateTime.Now.Year")]),
    # ---- 其它被本片引用的冻结锚 ----
    ("ArcherUnit.cs:8-16", AU, [(8, "public class ArcherUnit")]),
    ("GameSvrConf.cs:1786-1787", GC, [
        (1786, "CastleWarRangeX = 100"),
        (1787, "CastleWarRangeY = 100")]),
    # ---- ⑧-7 攻城状态机模块头（castle.rs 1-174，纳入覆盖面）----
    # 横向体检（2026-10-13）订正：Run/Start/Stop/CheckInPalace 范围原多算 1-3 行；
    # CastleConst.cs:11→:10；N1 被吞调用点 Message.cs 三处 330/328/326→329/327/325。
    ("UserCastle.cs:311-446", UC, [
        (311, "public void Run()"),
        (373, "MainDoorControl(true);"),
        (446, "}")]),
    ("UserCastle.cs:328-339", UC, [
        (328, "int Year = DateTime.Now.Year;"),
        (336, "TodayIncome = 0;"),
        (339, "}")]),
    ("UserCastle.cs:340-376", UC, [
        (340, "if (!IsStartWar && !UnderWar)"),
        (343, "SystemShare.Config.StartCastlewarTime"),
        (376, "}")]),
    ("UserCastle.cs:377-389", UC, [
        (377, "for (int i = 0; i < Guards.Length; i++)"),
        (384, "for (int i = 0; i < Archers.Length; i++)"),
        (389, "}")]),
    ("UserCastle.cs:391-440", UC, [
        (391, "if (UnderWar)"),
        (440, "}")]),
    ("UserCastle.cs:564-572", UC, [
        (564, "public void StartWallconquestWar()"),
        (567, "GetMapRageHuman(PalaceEnvir"),
        (570, "playObjectList[i].RefShowName();"),
        (572, "}")]),
    ("UserCastle.cs:577-597", UC, [
        (577, "public void StopWallconquestWar()"),
        (580, "UnderWar = false;"),
        (581, "AttackGuildList.Clear();"),
        (597, "}")]),
    ("UserCastle.cs:890-909", UC, [
        (890, "public void MainDoorControl(bool boClose)"),
        (905, "((CastleDoor)MainDoor.BaseObject).Open();"),
        (909, "}")]),
    ("UserCastle.cs:998-1014", UC, [
        (998, "public bool AddAttackerInfo(GuildInfo Guild)"),
        (1011, "Messages.SS_212"),
        (1014, "}")]),
    ("UserCastle.cs:639-642", UC, [
        (639, "public bool IsMasterGuild(IGuild guild)"),
        (641, "MasterGuild != null && MasterGuild == guild")]),
    ("UserCastle.cs:480-483", UC, [
        (480, "public bool IsMember(PlayObject member)"),
        (482, "IsMasterGuild(member.MyGuild)")]),
    ("UserCastle.cs:604-607", UC, [
        (604, "public int InPalaceGuildCount()"),
        (606, "return AttackGuildList.Count;")]),
    ("UserCastle.cs:680-705", UC, [
        (680, "public bool CheckInPalace(int nX, int nY)"),
        (704, "return result;")]),
    # N1 抛出点 + 被吞调用点（⑧-7 模块头 N1 表）。
    ("UserCastle.cs:601", UC, [(601, "throw new NotImplementedException();")]),
    ("CastleOfficial.cs:157", CO, [(157, "Castle.WithDrawalGolds(PlayObject")]),
    ("UserCastle.cs:646", UC, [(646, "throw new NotImplementedException();")]),
    ("PlayObject.Message.cs:329", PM, [(329, "castle.GetCastle(MyGuild);")]),
    ("ChangeSabukLordCommand.cs:39", CSL, [(39, "castle.GetCastle(guild);")]),
    ("UserCastle.cs:672", UC, [(672, "throw new NotImplementedException();")]),
    ("GuildOfficial.cs:219", GO, [(219, "Castle.AddAttackerInfo(PlayObject.MyGuild)")]),
    ("UserCastle.cs:677", UC, [(677, "throw new NotImplementedException();")]),
    ("PlayObject.Message.cs:327", PM, [(327, "castle.CanGetCastle(MyGuild)")]),
    ("UserCastle.cs:709", UC, [(709, "throw new NotImplementedException();")]),
    ("PlayObject.Operate.cs:371", PO, [(371, "castle.CheckInPalace(CurrX, CurrY, this)")]),
    ("UserCastle.cs:913", UC, [(913, "throw new NotImplementedException();")]),
    ("CastleOfficial.cs:179", CO, [(179, "Castle.ReceiptGolds(PlayObject")]),
    ("UserCastle.cs:1042", UC, [(1042, "throw new NotImplementedException();")]),
    ("UserCastle.cs:1047", UC, [(1047, "throw new NotImplementedException();")]),
    ("PlayObject.Message.cs:325", PM, [(325, "castle.IsAttackGuild(MyGuild)")]),
    ("UserCastle.cs:1052", UC, [(1052, "throw new NotImplementedException();")]),
    ("UserCastle.cs:1057", UC, [(1057, "throw new NotImplementedException();")]),
    ("PlayObject.Message.cs:324-334", PM, [
        (325, "castle.IsAttackGuild(MyGuild)"),
        (329, "castle.GetCastle(MyGuild);"),
        (333, "castle.StopWallconquestWar();")]),
    # 照搬的 oracle 怪行为 + 常量 + 配置（⑧-7 模块头）。
    ("M2Share.cs:458-493", MS, [
        (458, "public static DateTime AddDateTimeOfDay"),
        (460, "DateTime result = DateTime.Now;")]),
    ("UserCastle.cs:582-584", UC, [
        (582, "IList<PlayObject> ListC = new List<PlayObject>();"),
        (583, "GetMapOfRangeHumanCount(CastleEnvir, HomeX, HomeY, 100);"),
        (584, "for (int i = 0; i < ListC.Count; i++)")]),
    ("UserCastle.cs:48", UC, [(48, "Archers = new ArcherUnit[12];")]),
    ("CastleConst.cs:10", CCONST, [(10, "public const int MaxCastleArcher = 12;")]),
    ("UserCastle.cs:49", UC, [(49, "Guards = new ArcherUnit[12];")]),
    ("GameSvrConf.cs:1782", GC, [(1782, 'CastleName = "沙巴克";')]),
    ("Messages.cs:453", MSG, [(453, "public const int SS_204 = 204;")]),
    ("Messages.cs:461", MSG, [(461, "public const int SS_212 = 212;")]),
    # ⑧-2 配置锚（⑧-7 `CastleSiegeConfig` 的读侧输入）。
    ("ServerConf.cs:639", SC, [(639, 'StartCastlewarTime = ReadWriteInteger("Setup", "StartCastlewarTime"')]),
    ("ServerConf.cs:642", SC, [(642, 'CastleWarTime = ReadWriteInteger("Setup", "CastleWarTime"')]),
    ("ServerConf.cs:640", SC, [(640, 'ShowCastleWarEndMsgTime = ReadWriteInteger("Setup", "ShowCastleWarEndMsgTime"')]),
    ("ServerConf.cs:643", SC, [(643, 'GetCastleTime = ReadWriteInteger("Setup", "GetCastleTime"')]),
    ("ServerConf.cs:638", SC, [(638, 'StartCastleWarDays = ReadWriteByte("Setup", "StartCastleWarDays"')]),
    ("ServerConf.cs:253", SC, [(253, 'CastleName = ReadWriteString("Setup", "CastleName"')]),
    ("GameSvrConf.cs:1442-1446", GC, [
        (1442, "StartCastleWarDays = 4;"),
        (1443, "StartCastlewarTime = 20;"),
        (1446, "GetCastleTime = 10 * 60 * 1000;")]),
    # ---- ⑧-6 M-2 收口：castle.rs 全文件覆盖面（新增登记锚；每条对冻结 src/ 的 (line, token) 机判）----
    ("CastleDoor.cs:12/57/75", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(12, 'public bool HoldPlace;'), (57, 'HoldPlace = false;'), (75, 'HoldPlace = true;')]),
    ("WallStructure.cs:7/53-66", "src/M2Server/Monster/Monsters/WallStructure.cs", [(7, 'internal bool SetMapFlaged;'), (53, 'if (SetMapFlaged)'), (65, 'SetMapFlaged = true;')]),
    ("CastleDoor.cs:66/96/108", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(66, 'Dir = (byte)(3 - HUtil32.Round(WAbil.HP / (double)WAbil.MaxHP * 3.0));'), (96, 'int n08 = 3 - HUtil32.Round(WAbil.HP / (double)WAbil.MaxHP * 3.0);'), (108, 'int n08 = 3 - HUtil32.Round(WAbil.HP / (double)WAbil.MaxHP * 3.0);')]),
    ("WallStructure.cs:28/70", "src/M2Server/Monster/Monsters/WallStructure.cs", [(28, 'n08 = (byte)(3 - HUtil32.Round(this.WAbil.HP / this.WAbil.MaxHP * 3.0));'), (70, 'n08 = 3 - HUtil32.Round(this.WAbil.HP / this.WAbil.MaxHP * 3.0);')]),
    ("CastleDoor.cs:46-58", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(46, 'public void Open()'), (57, 'HoldPlace = false;')]),
    ("CastleDoor.cs:60-76", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(60, 'public void Close()'), (75, 'HoldPlace = true;')]),
    ("CastleDoor.cs:78-82", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(78, 'public override void Die()'), (81, 'SetMapXyFlag(2);')]),
    ("CastleDoor.cs:84-104", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(84, 'public override void Run()'), (103, 'base.Run();')]),
    ("CastleDoor.cs:106-115", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(106, 'public void RefStatus()'), (114, 'SendRefMsg(Messages.RM_ALIVE, Dir, CurrX, CurrY, 0, "");')]),
    ("WallStructure.cs:17-21", "src/M2Server/Monster/Monsters/WallStructure.cs", [(17, 'public override void Initialize()'), (20, 'base.Initialize();')]),
    ("WallStructure.cs:23-40", "src/M2Server/Monster/Monsters/WallStructure.cs", [(23, 'public void RefStatus()'), (39, 'this.SendRefMsg(Messages.RM_ALIVE, this.Dir, this.CurrX, this.CurrY, 0, "");')]),
    ("WallStructure.cs:47-82", "src/M2Server/Monster/Monsters/WallStructure.cs", [(47, 'public override void Run()'), (81, 'base.Run();')]),
    ("HUtil32.cs:131-134", "src/OpenMir2/HUtil32.cs", [(131, 'public static int Round(double r)'), (133, 'return (int)Math.Round(r, MidpointRounding.AwayFromZero);')]),
    ("CastleDoor.cs:23-44", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(23, 'private void SetMapXyFlag(int nFlag)'), (42, 'Envir.SetMapXyFlag(CurrX + 1, CurrY - 2, false);')]),
    ("CastleDoor.cs:25-27", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(25, 'Envir.SetMapXyFlag(CurrX, CurrY - 2, true);'), (27, 'Envir.SetMapXyFlag(CurrX + 1, CurrY - 2, true);')]),
    ("CastleDoor.cs:28", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(28, 'bool boFlag = nFlag != 1;')]),
    ("CastleDoor.cs:29-37", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(29, 'Envir.SetMapXyFlag(CurrX, CurrY, boFlag);'), (37, 'Envir.SetMapXyFlag(CurrX - 1, CurrY + 1, boFlag);')]),
    ("CastleDoor.cs:38-43", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(38, 'if (nFlag == 0)'), (42, 'Envir.SetMapXyFlag(CurrX + 1, CurrY - 2, false);')]),
    ("CastleOfficial.cs:52-67", "src/GameSrv/Npc/CastleOfficial.cs", [(52, 'case "$CASTLEDOORSTATE":'), (67, 'sMsg = ReplaceVariableText(sMsg, "<$CASTLEDOORSTATE>", sText);')]),
    ("HUtil32.cs:550-576", "src/OpenMir2/HUtil32.cs", [(550, 'public static bool CompareLStr(string src, string targ)'), (575, 'return false;')]),
    ("CastleOfficial.cs:199-238", "src/GameSrv/Npc/CastleOfficial.cs", [(199, 'else if (string.Compare(sLabel, ScriptFlagConst.sOPENMAINDOOR, StringComparison.OrdinalIgnoreCase) == 0)'), (237, 'PlayObject.SendMsg(this, Messages.RM_MENU_OK, 0, ActorId, 0, 0);')]),
    ("CastleOfficial.cs:229/235", "src/GameSrv/Npc/CastleOfficial.cs", [(229, 's20 = sLabel.Substring(ScriptFlagConst.sHIREGUARDNOW.Length, sLabel.Length);'), (235, 's20 = sLabel.Substring(ScriptFlagConst.sHIREARCHERNOW.Length, sLabel.Length);')]),
    ("CastleOfficial.cs:358-381", "src/GameSrv/Npc/CastleOfficial.cs", [(358, 'private void RepairDoor(IPlayerActor PlayObject)'), (379, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);')]),
    ("CastleOfficial.cs:383-406", "src/GameSrv/Npc/CastleOfficial.cs", [(383, 'private void RepairWallNow(int nWallIndex, IPlayerActor PlayObject)'), (404, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);')]),
    ("UserCastle.cs:115/121/127", "src/M2Server/Castle/UserCastle.cs", [(115, 'PalaceEnvir = SystemShare.MapMgr.FindMap(PalaceMap);'), (121, 'SecretEnvir = SystemShare.MapMgr.FindMap(SecretMap);'), (127, 'CastleEnvir = SystemShare.MapMgr.FindMap(MapName);')]),
    ("UserCastle.cs:920/956", "src/M2Server/Castle/UserCastle.cs", [(920, 'public bool RepairDoor()'), (956, 'public bool RepairWall(int nWallIndex)')]),
    ("ScriptFlagConst.cs:75", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(75, 'public const string sOPENMAINDOOR = "@openmaindoor";')]),
    ("ScriptFlagConst.cs:76", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(76, 'public const string sCLOSEMAINDOOR = "@closemaindoor";')]),
    ("ScriptFlagConst.cs:77", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(77, 'public const string sREPAIRDOORNOW = "@repairdoornow";')]),
    ("ScriptFlagConst.cs:78", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(78, 'public const string sREPAIRWALLNOW1 = "@repairwallnow1";')]),
    ("ScriptFlagConst.cs:79", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(79, 'public const string sREPAIRWALLNOW2 = "@repairwallnow2";')]),
    ("ScriptFlagConst.cs:80", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(80, 'public const string sREPAIRWALLNOW3 = "@repairwallnow3";')]),
    ("ScriptFlagConst.cs:81", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(81, 'public const string sHIREARCHERNOW = "@hirearchernow";')]),
    ("ScriptFlagConst.cs:82", "src/Modules/SystemModule/Const/ScriptFlagConst.cs", [(82, 'public const string sHIREGUARDNOW = "@hireguardnow";')]),
    ("CastleOfficial.cs:59-61", "src/GameSrv/Npc/CastleOfficial.cs", [(59, 'else if (castleDoor.IsOpened)'), (61, 'sText = "opened";')]),
    ("CastleOfficial.cs:63-65", "src/GameSrv/Npc/CastleOfficial.cs", [(63, 'else'), (65, 'sText = "closed";')]),
    ("CastleOfficial.cs:358-406", "src/GameSrv/Npc/CastleOfficial.cs", [(358, 'private void RepairDoor(IPlayerActor PlayObject)'), (404, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);')]),
    ("CastleOfficial.cs:374/399", "src/GameSrv/Npc/CastleOfficial.cs", [(374, 'PlayObject.SysMsg("城门不需要修理!!!", MsgColor.Green, MsgType.Hint);'), (399, 'PlayObject.SysMsg("城门不需要修理!!!", MsgColor.Green, MsgType.Hint);')]),
    ("CastleOfficial.cs:379/404", "src/GameSrv/Npc/CastleOfficial.cs", [(379, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);'), (404, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);')]),
    ("UserCastle.cs:73", "src/M2Server/Castle/UserCastle.cs", [(73, 'public int SaveTick { get; set; }')]),
    ("UserCastle.cs:23-59", "src/M2Server/Castle/UserCastle.cs", [(23, 'private int _techLevel;'), (58, 'public int TechLevel')]),
    ("UserCastle.cs:318-321", "src/M2Server/Castle/UserCastle.cs", [(318, 'if (string.IsNullOrEmpty(MapName))'), (320, 'return;')]),
    ("UserCastle.cs:323-326", "src/M2Server/Castle/UserCastle.cs", [(323, 'if (M2Share.ServerIndex != SystemShare.MapMgr.GetMapOfServerIndex(MapName))'), (325, 'return;')]),
    ("UserCastle.cs:347-362", "src/M2Server/Castle/UserCastle.cs", [(347, 'for (int i = AttackWarList.Count - 1; i >= 0; i--)'), (360, 'AttackWarList.RemoveAt(i);')]),
    ("UserCastle.cs:358", "src/M2Server/Castle/UserCastle.cs", [(358, 'StartCastleWarTick = HUtil32.GetTickCount();')]),
    ("UserCastle.cs:365", "src/M2Server/Castle/UserCastle.cs", [(365, 'AttackGuildList.Add(MasterGuild);')]),
    ("UserCastle.cs:366", "src/M2Server/Castle/UserCastle.cs", [(366, 'StartWallconquestWar();')]),
    ("UserCastle.cs:367", "src/M2Server/Castle/UserCastle.cs", [(367, 'SaveAttackSabukWall();')]),
    ("UserCastle.cs:368", "src/M2Server/Castle/UserCastle.cs", [(368, 'SystemShare.WorldEngine.SendServerGroupMsg(Messages.SS_212, M2Share.ServerIndex, "");')]),
    ("UserCastle.cs:369-371", "src/M2Server/Castle/UserCastle.cs", [(369, 'string s20 = string.Format(sWarStartMsg, sName);'), (371, 'SystemShare.WorldEngine.SendServerGroupMsg(Messages.SS_204, M2Share.ServerIndex, s20);')]),
    ("UserCastle.cs:373", "src/M2Server/Castle/UserCastle.cs", [(373, 'MainDoorControl(true);')]),
    ("UserCastle.cs:393-406", "src/M2Server/Castle/UserCastle.cs", [(393, 'if (LeftWall.BaseObject != null)'), (405, 'RightWall.BaseObject.StoneMode = false;')]),
    ("UserCastle.cs:408-418", "src/M2Server/Castle/UserCastle.cs", [(408, 'if (!ShowOverMsg)'), (416, 'LogService.Warn(s20);')]),
    ("UserCastle.cs:419-422", "src/M2Server/Castle/UserCastle.cs", [(419, 'if ((HUtil32.GetTickCount() - StartCastleWarTick) > SystemShare.Config.CastleWarTime)'), (421, 'StopWallconquestWar();')]),
    ("UserCastle.cs:424-440", "src/M2Server/Castle/UserCastle.cs", [(424, 'else'), (438, 'RightWall.BaseObject.StoneMode = true;')]),
    ("UserCastle.cs:584-593", "src/M2Server/Castle/UserCastle.cs", [(584, 'for (int i = 0; i < ListC.Count; i++)'), (593, 'string stopMessage = string.Format(sWallWarStop, sName);')]),
    ("UserCastle.cs:920-955", "src/M2Server/Castle/UserCastle.cs", [(920, 'public bool RepairDoor()'), (948, 'return result;')]),
    ("UserCastle.cs:956-997", "src/M2Server/Castle/UserCastle.cs", [(956, 'public bool RepairWall(int nWallIndex)'), (995, 'return result;')]),
    ("UserCastle.cs:1016-1030", "src/M2Server/Castle/UserCastle.cs", [(1016, 'private bool InAttackerList(GuildInfo Guild)'), (1030, 'private void SetPower(int nPower)')]),
    ("UserCastle.cs:609-612", "src/M2Server/Castle/UserCastle.cs", [(609, 'public bool IsMember(IPlayerActor member)'), (611, 'return IsMasterGuild(member.MyGuild);')]),
    ("UserCastle.cs:366-373", "src/M2Server/Castle/UserCastle.cs", [(366, 'StartWallconquestWar();'), (373, 'MainDoorControl(true);')]),
    ("UserCastle.cs:345", "src/M2Server/Castle/UserCastle.cs", [(345, 'IsStartWar = true;')]),
    ("UserCastle.cs:391-406", "src/M2Server/Castle/UserCastle.cs", [(391, 'if (UnderWar)'), (405, 'RightWall.BaseObject.StoneMode = false;')]),
    ("UserCastle.cs:421", "src/M2Server/Castle/UserCastle.cs", [(421, 'StopWallconquestWar();')]),
    ("CastleDoor.cs:66-70", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(66, 'Dir = (byte)(3 - HUtil32.Round(WAbil.HP / (double)WAbil.MaxHP * 3.0));'), (69, 'Dir = 0;')]),
    ("CastleDoor.cs:86-93", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(86, 'if (Death && Castle != null)'), (92, 'HealthTick = 0;')]),
    ("CastleDoor.cs:94-102", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(94, 'if (!IsOpened)'), (100, 'SendRefMsg(Messages.RM_TURN, Dir, CurrX, CurrY, 0, "");')]),
    ("Envirnoment.cs:1362-1369", "src/M2Server/Maps/Envirnoment.cs", [(1362, 'public void SetMapXyFlag(int nX, int nY, bool boFlag)'), (1367, 'cellInfo.SetAttribute(boFlag ? CellAttribute.Walk : CellAttribute.LowWall);')]),
    ("CastleDoor.cs:25-43", "src/M2Server/Monster/Monsters/CastleDoor.cs", [(25, 'Envir.SetMapXyFlag(CurrX, CurrY - 2, true);'), (42, 'Envir.SetMapXyFlag(CurrX + 1, CurrY - 2, false);')]),
    ("UserCastle.cs:920-927", "src/M2Server/Castle/UserCastle.cs", [(920, 'public bool RepairDoor()'), (926, 'return false;')]),
    ("UserCastle.cs:930", "src/M2Server/Castle/UserCastle.cs", [(930, 'if ((HUtil32.GetTickCount() - castleDoor.BaseObject.StruckTick) > 60 * 1000)')]),
    ("UserCastle.cs:939", "src/M2Server/Castle/UserCastle.cs", [(939, 'if ((HUtil32.GetTickCount() - castleDoor.BaseObject.StruckTick) > 60 * 1000)')]),
    ("UserCastle.cs:924", "src/M2Server/Castle/UserCastle.cs", [(924, 'if (castleDoor.BaseObject == null || UnderWar || castleDoor.BaseObject.WAbil.HP >= castleDoor.BaseObject.WAbil')]),
    ("UserCastle.cs:958-992", "src/M2Server/Castle/UserCastle.cs", [(958, 'bool result = false;'), (992, 'result = true;')]),
    ("CastleOfficial.cs:365-380", "src/GameSrv/Npc/CastleOfficial.cs", [(365, 'if (Castle.TotalGold >= SystemShare.Config.RepairDoorPrice)'), (379, 'PlayObject.SysMsg("城内资金不足!!!", MsgColor.Red, MsgType.Hint);')]),
    ("CastleOfficial.cs:365", "src/GameSrv/Npc/CastleOfficial.cs", [(365, 'if (Castle.TotalGold >= SystemShare.Config.RepairDoorPrice)')]),
    ("UserCastle.cs:128-220", "src/M2Server/Castle/UserCastle.cs", [(128, 'if (CastleEnvir != null)'), (209, 'LogService.Warn("[错误信息] 城堡初始化守卫失败(检查怪物数据库里有没守卫怪物)");')]),
    # COUNT 76
]

# 覆盖面区间（内容标记 → 行区间）。区间内每个 `.cs:NNN` 锚都必须登记，且不得留裸 `:NNN`。
# ⑧-6 M-2 收口（reviewer-5 `8276f7bb`）：覆盖面从「⑧-7 模块头」扩到**整个 castle.rs**
# （起标记 = 文件首行；止标记 = `None` ⇒ 到 EOF）。区间内每个 `.cs:NNN` 锚都必须登记，
# 且不得留裸 `:NNN`（防行号漂移 + 防 `:NNN` 跨文件误读）。
COVERED_MARKERS = [
    ("//! 攻城状态机（", None),
]

ANCHOR_RE = re.compile(
    r'([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.cs):(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')
BARE_ANCHOR_RE = re.compile(r'(?<![A-Za-z0-9_.]):(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')

# 读侧 token 强度（判据 4）：整组只剩纯标点/空白 ⇒ 弱锚 ⇒ RED。
# 逐条语义级核对（行内容与散文「声称构造」一一对应）仍是抽样人核，见证据 README 残留①。
_WEAK_ONLY_TOKEN_RE = re.compile(r'^[{}();,\s]*$')


def _token_strength_findings(anchors):
    findings = []
    for anchor, _rel, points in anchors:
        toks = [t.strip() for _l, t in points]
        if not any(not _WEAK_ONLY_TOKEN_RE.match(t) for t in toks):
            findings.append(
                "读侧 token 强度：锚 %s 整组无有辨识度 token（%r）——"
                "至少一条 token 须含实际内容，不能只靠 `}`/`;` 等纯标点钉位置" % (anchor, toks))
    return findings


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _cached_src(src_root, rel):
    path = os.path.join(src_root, rel)
    return _read_text(path).splitlines()


def _covered_spans(castle_lines):
    """由 `COVERED_MARKERS` 的起/止内容标记解析出 1-based 行区间（含止）。"""
    spans = []
    for start_marker, end_marker in COVERED_MARKERS:
        si = next((i for i, line in enumerate(castle_lines) if start_marker in line), None)
        if si is None:
            continue
        if end_marker is None:
            # `None` ⇒ 覆盖到 EOF（⑧-6 M-2：整个 castle.rs）。
            spans.append((si + 1, len(castle_lines)))
            continue
        ei = next((i for i, line in enumerate(castle_lines) if end_marker in line), None)
        if ei is None or ei < si:
            continue
        spans.append((si + 1, ei + 1))
    return spans


def _coverage_findings(anchors, castle_text, castle_lines=None):
    """覆盖面口径：覆盖区间内每个 `.cs:NNN` 锚都已登记，且不留裸 `:NNN`。

    返回 findings 列表（空 = GREEN）。与写/读侧独立：不改 `ANCHORS` 也能单独自检。
    """
    if castle_lines is None:
        castle_lines = castle_text.splitlines()
    spans = _covered_spans(castle_lines)
    registered = set(anchor for anchor, _rel, _pts in anchors)
    findings = []
    if not spans:
        findings.append("覆盖面：未解析出任何覆盖区间（COVERED_MARKERS 标记缺失/顺序错）")
    for start, end in spans:
        seg = "\n".join(castle_lines[start - 1:end])
        for m in ANCHOR_RE.finditer(seg):
            if m.group(0) not in registered:
                findings.append("覆盖面：未登记锚 %s（覆盖区间 castle.rs:%d-%d）"
                                % (m.group(0), start, end))
        stripped = ANCHOR_RE.sub("", seg)
        for m in BARE_ANCHOR_RE.finditer(stripped):
            findings.append("覆盖面：裸锚 `:%s`（覆盖区间 castle.rs:%d-%d）须写成 `文件.cs:NNN`"
                            % (m.group(1), start, end))
    return findings


def _coverage_inventory(anchors, castle_text, castle_lines=None):
    """覆盖面清点（`audit` 面）：区间内锚清单 = 已登记 / 未登记 / 裸锚。

    与 `_coverage_findings` 同源（同一 `_covered_spans` + 同一正则），但返回**去重后的清单**
    供人/机复算（旧 `10-anchor-audit.txt` 的仓外脚本已废）。
    """
    if castle_lines is None:
        castle_lines = castle_text.splitlines()
    spans = _covered_spans(castle_lines)
    registered = set(anchor for anchor, _rel, _pts in anchors)
    present, bare = [], []
    for start, end in spans:
        seg = "\n".join(castle_lines[start - 1:end])
        for m in ANCHOR_RE.finditer(seg):
            present.append(m.group(0))
        stripped = ANCHOR_RE.sub("", seg)
        for m in BARE_ANCHOR_RE.finditer(stripped):
            bare.append(m.group(1))

    def _dedupe(xs):
        seen, out = set(), []
        for x in xs:
            if x not in seen:
                seen.add(x)
                out.append(x)
        return out

    present = _dedupe(present)
    bare = _dedupe(bare)
    uncovered = [a for a in present if a not in registered]
    return {"covered_spans": spans, "registered": len(registered),
            "present": present, "uncovered": uncovered, "bare": bare}


def _evaluate(anchors=None, castle_text=None, readme_text=None, src_root=None,
              coverage=False):
    """返回 findings 列表（空 = GREEN）。`coverage=True` 时附带覆盖面判定。"""
    anchors = ANCHORS if anchors is None else anchors
    if castle_text is None:
        castle_text = _read_text(CASTLE_RS)
    if readme_text is None:
        readme_text = _read_text(README_MD)
    if src_root is None:
        src_root = REPO_ROOT
    docs = castle_text + "\n" + readme_text
    findings = []
    for anchor, rel, points in anchors:
        if anchor not in docs:
            findings.append("写侧缺锚 %s（不在 castle.rs / README.md）" % anchor)
        lines = None
        for line, token in points:
            if lines is None:
                try:
                    lines = _cached_src(src_root, rel)
                except OSError as exc:
                    findings.append("读侧缺件 %s: %s" % (rel, exc))
                    break
            if line < 1 or line > len(lines):
                findings.append("读侧越界 %s:%d（文件 %d 行）" % (rel, line, len(lines)))
                continue
            actual = lines[line - 1]
            if token not in actual:
                findings.append("读侧不匹配 %s:%d 期望含 %r，实际 %r"
                                % (rel, line, token, actual.strip()))
    findings.extend(_token_strength_findings(anchors))
    if coverage:
        findings.extend(_coverage_findings(anchors, castle_text))
    return findings


def cmd_check(args):
    findings = _evaluate(coverage=True)
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "anchors": len(ANCHORS),
                       "covered_spans": _covered_spans(_read_text(CASTLE_RS).splitlines())},
                      fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: castle oracle 锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(ANCHORS)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: castle oracle 锚门通过（%d 锚命中冻结 src/ + 写侧在位 + 覆盖区间无未登记/裸锚）\n"
                     % len(ANCHORS))
    return 0


def cmd_audit(args):
    inv = _coverage_inventory(ANCHORS, _read_text(CASTLE_RS))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(inv, fh, ensure_ascii=False, indent=2)
    sys.stdout.write("覆盖面区间: %s\n" % (inv["covered_spans"],))
    sys.stdout.write("已登记锚（ANCHORS 表）: %d\n" % inv["registered"])
    sys.stdout.write("区间内锚（去重）: %d\n" % len(inv["present"]))
    sys.stdout.write("区间内未登记锚: %d\n" % len(inv["uncovered"]))
    for a in inv["uncovered"]:
        sys.stdout.write("  - %s\n" % a)
    sys.stdout.write("区间内裸锚: %d\n" % len(inv["bare"]))
    for a in inv["bare"]:
        sys.stdout.write("  - `:%s`\n" % a)
    ok = not inv["uncovered"] and not inv["bare"]
    sys.stdout.write("AUDIT %s\n" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def cmd_selftest(args):
    ok = True
    # 1) 表结构自检。
    for anchor, rel, points in ANCHORS:
        if not anchor or not rel or not points:
            sys.stdout.write("RED: 表项不完整 %r\n" % (anchor,))
            ok = False
        for line, token in points:
            if line < 1 or not token:
                sys.stdout.write("RED: 表项 %s 的 (line, token) 非法 %r\n" % (anchor, (line, token)))
                ok = False
    sys.stdout.write("selftest 表结构: %s (%d 锚)\n" % ("GREEN" if ok else "RED", len(ANCHORS)))

    # 2) 牙齿：读侧 token 错 ⇒ RED；对 ⇒ GREEN。
    castle_text = _read_text(CASTLE_RS)
    readme_text = _read_text(README_MD)
    good = [("UserCastle.cs:113", UC, [(113, "GetMapOfServerIndex")])]
    bad = [("UserCastle.cs:113", UC, [(113, "ThisTokenDoesNotExistXYZ")])]
    g = _evaluate(anchors=good, castle_text=castle_text, readme_text=readme_text)
    b = _evaluate(anchors=bad, castle_text=castle_text, readme_text=readme_text)
    t1 = (g == [] and len(b) == 1)
    sys.stdout.write("selftest 读侧 token 牙齿: %s (good=%r bad=%d)\n"
                     % ("GREEN" if t1 else "RED", g, len(b)))
    ok = ok and t1

    # 3) 牙齿：写侧缺锚 ⇒ RED。
    noanchor = [("UserCastle.cs:9999", UC, [(113, "GetMapOfServerIndex")])]
    n = _evaluate(anchors=noanchor, castle_text=castle_text, readme_text=readme_text)
    t2 = (len(n) == 1 and "写侧缺锚" in n[0])
    sys.stdout.write("selftest 写侧缺锚牙齿: %s (%r)\n" % ("GREEN" if t2 else "RED", n))
    ok = ok and t2

    # 4) 覆盖面牙：合成覆盖区间（用真身 COVERED_MARKERS）。
    syn = [
        "//! 攻城状态机（合成）",
        "//! `A.cs:1` 与裸 `:9`",
        "//! `B.cs:2`",
        "/// `CastleDoor.cs` / `WallStructure.cs`，⑧-8",
        "tail",
    ]
    cov = _coverage_findings([("A.cs:1", "A.cs", [(1, "a")])], "\n".join(syn), syn)
    t3a = any("未登记锚 B.cs:2" in f for f in cov)
    t3b = any("裸锚 `:9`" in f for f in cov)
    t3c = not any("A.cs:1" in f for f in cov)
    t3 = t3a and t3b and t3c
    sys.stdout.write("selftest 覆盖面牙齿: %s (cov=%r)\n" % ("GREEN" if t3 else "RED", cov))
    ok = ok and t3

    # 5) 覆盖面牙：标记缺失 ⇒ RED（防覆盖区间惄惄失效）。
    cov_none = _coverage_findings(ANCHORS, "no markers here")
    t4 = any("未解析出任何覆盖区间" in f for f in cov_none)
    sys.stdout.write("selftest 覆盖区间标记缺失牙齿: %s\n" % ("GREEN" if t4 else "RED"))
    ok = ok and t4

    # 6) 覆盖面正向：当前树覆盖面必须 GREEN。
    cov_real = _coverage_findings(ANCHORS, castle_text)
    t5 = cov_real == []
    sys.stdout.write("selftest 真身覆盖面: %s (%d 项)\n" % ("GREEN" if t5 else "RED", len(cov_real)))
    for f in cov_real:
        sys.stdout.write("  - %s\n" % f)
    ok = ok and t5

    # 7) ⑧-6 M-2 牙齿：覆盖面必须已扩到整个 castle.rs（首行→EOF），防回退到只盖模块头。
    castle_lines = castle_text.splitlines()
    spans = _covered_spans(castle_lines)
    t6 = spans == [(1, len(castle_lines))]
    sys.stdout.write("selftest 全文件覆盖面: %s (%s，castle.rs %d 行)\n"
                     % ("GREEN" if t6 else "RED", spans, len(castle_lines)))
    ok = ok and t6

    # 8) M-1 牙齿：`audit` 清点器与覆盖面同源 ——
    #    合成文本里未登记锚/裸锚各点名一条，已登记锚不点名；真身 audit 两张清单都空。
    inv_syn = _coverage_inventory([("A.cs:1", "A.cs", [(1, "a")])], "\n".join(syn), syn)
    t7a = inv_syn["uncovered"] == ["B.cs:2"] and inv_syn["bare"] == ["9"]
    inv_real = _coverage_inventory(ANCHORS, castle_text)
    t7b = inv_real["uncovered"] == [] and inv_real["bare"] == []
    t7 = t7a and t7b
    sys.stdout.write("selftest audit 清点器: %s (syn uncovered=%r bare=%r; real uncovered=%d bare=%d)\n"
                     % ("GREEN" if t7 else "RED", inv_syn["uncovered"], inv_syn["bare"],
                        len(inv_real["uncovered"]), len(inv_real["bare"])))
    ok = ok and t7

    # 9) token 强度牙（⑧-6 O-1 / ⑧-10 残余 Minor）：整组只剩结束行 `}` ⇒ RED；
    #    含一条有辨识度 token 的整组不因该项红。
    weak_only = [("UserCastle.cs:311-446", UC, [(446, "}")])]
    w = _evaluate(anchors=weak_only, castle_text=castle_text, readme_text=readme_text)
    t8 = (not any("token 强度" in f for f in g)) and len(w) == 1 and "token 强度" in w[0]
    sys.stdout.write("selftest token 强度牙齿: %s (good=%d weak=%r)\n"
                     % ("GREEN" if t8 else "RED", len(g), w))
    ok = ok and t8

    sys.stdout.write("SELFTEST %s\n" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def cmd_mutation(args):
    out = args.out or tempfile.mkdtemp(prefix="castle-anchor-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    castle_text = _read_text(CASTLE_RS)
    readme_text = _read_text(README_MD)

    # MUT1（写侧）：把正确的 `UserCastle.cs:113` 在两个落点都改回历史错值 `:111` ⇒ 注册锚消失 ⇒ RED。
    wrong_doc = castle_text.replace("UserCastle.cs:113", "UserCastle.cs:111")
    wrong_readme = readme_text.replace("UserCastle.cs:113", "UserCastle.cs:111")
    if wrong_doc == castle_text or wrong_readme == readme_text:
        sys.stdout.write("BUG: 变异 MUT1 找不到 UserCastle.cs:113\n")
        ok = False
    f1 = _evaluate(castle_text=wrong_doc, readme_text=wrong_readme)
    red1 = any("写侧缺锚 UserCastle.cs:113" in f for f in f1)
    sys.stdout.write("MUT1 写侧漂移(:113→:111): %s（%d 项；%s）\n"
                     % ("RED" if f1 else "GREEN", len(f1), out))
    ok = ok and red1

    # MUT2（读侧）：把表里 `CastleConf.cs:39` 期望改到 36（历史错值）⇒ 冻结行 token 不匹配 ⇒ RED。
    bad_table = [("CastleConf.cs:39", CC, [(36, '"CastleMap"')])]
    f2 = _evaluate(anchors=bad_table, castle_text=castle_text, readme_text=readme_text)
    red2 = any("读侧不匹配" in f for f in f2)
    sys.stdout.write("MUT2 读侧漂移(表 39→36): %s（%d 项）\n"
                     % ("RED" if f2 else "GREEN", len(f2)))
    for f in f2:
        sys.stdout.write("  - %s\n" % f)
    ok = ok and red2

    # MUT3（回归）：真实 check 必须 GREEN（变异体不影响真身）。
    f0 = _evaluate(castle_text=castle_text, readme_text=readme_text, coverage=True)
    sys.stdout.write("MUT3 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
    ok = ok and not f0

    # MUT4（覆盖面）：在覆盖区间内注入未登记锚 `Ghost.cs:1` ⇒ 未登记 ⇒ RED。
    mut_cov = castle_text.replace("//! ## N1：", "//! `Ghost.cs:1`\n//! ## N1：", 1)
    if mut_cov == castle_text:
        sys.stdout.write("BUG: 变异 MUT4 找不到 N1 标题\n")
        ok = False
    f4 = _evaluate(castle_text=mut_cov, readme_text=readme_text, coverage=True)
    red4 = any("未登记锚 Ghost.cs:1" in f for f in f4)
    sys.stdout.write("MUT4 覆盖面未登记锚: %s（%d 项）\n" % ("RED" if f4 else "GREEN", len(f4)))
    ok = ok and red4

    # MUT5（⑧-7 横向体检）：把 `UserCastle.cs:311-446` 改回历史多算值 `:311-448`
    # ⇒ 写侧缺锚 + 覆盖面未登记（两条独立牙）。
    mut5 = castle_text.replace("UserCastle.cs:311-446", "UserCastle.cs:311-448")
    if mut5 == castle_text:
        sys.stdout.write("BUG: 变异 MUT5 找不到 UserCastle.cs:311-446\n")
        ok = False
    f5 = _evaluate(castle_text=mut5, readme_text=readme_text, coverage=True)
    red5 = any("写侧缺锚 UserCastle.cs:311-446" in f for f in f5) and \
        any("未登记锚 UserCastle.cs:311-448" in f for f in f5)
    sys.stdout.write("MUT5 ⑧-7 范围漂移(:311-446→:311-448): %s（%d 项）\n"
                     % ("RED" if f5 else "GREEN", len(f5)))
    ok = ok and red5

    # MUT6（裸锚）：把 `UserCastle.cs:480-483` 改回裸 `:480-483` ⇒ 写侧缺锚 + 覆盖面裸锚。
    mut6 = castle_text.replace("`UserCastle.cs:480-483`", "`:480-483`")
    if mut6 == castle_text:
        sys.stdout.write("BUG: 变异 MUT6 找不到 UserCastle.cs:480-483\n")
        ok = False
    f6 = _evaluate(castle_text=mut6, readme_text=readme_text, coverage=True)
    red6 = any("写侧缺锚 UserCastle.cs:480-483" in f for f in f6) and \
        any("裸锚 `:480-483`" in f for f in f6)
    sys.stdout.write("MUT6 ⑧-7 裸锚回归(:480-483): %s（%d 项）\n"
                     % ("RED" if f6 else "GREEN", len(f6)))
    ok = ok and red6

    # MUT7（⑧-6 M-2：覆盖面已扩到全文）：在**文件尾部**（旧「⑧-7 模块头」区间之外）注入
    # 未登记锚 `Ghost2.cs:1` ⇒ 未登记 = RED。覆盖面若退回只盖模块头，此变异体不会红。
    tail_anchor = "        // `Ghost2.cs:1`\n        assert!(c.main_door.is_none());"
    mut7 = castle_text.replace("        assert!(c.main_door.is_none());", tail_anchor, 1)
    if mut7 == castle_text:
        sys.stdout.write("BUG: 变异 MUT7 找不到尾部断言\n")
        ok = False
    f7 = _evaluate(castle_text=mut7, readme_text=readme_text, coverage=True)
    red7 = any("未登记锚 Ghost2.cs:1" in f for f in f7)
    sys.stdout.write("MUT7 全文覆盖面(尾部未登记锚): %s（%d 项）\n"
                     % ("RED" if f7 else "GREEN", len(f7)))
    ok = ok and red7

    # MUT8（⑧-6 M-1 audit 清点器）：从表里删 `UserCastle.cs:930` ⇒ audit 未登记 = RED
    # （证明清点器与 check 覆盖面同源、不是恒绿）。
    inv8 = _coverage_inventory([a for a in ANCHORS if a[0] != "UserCastle.cs:930"], castle_text)
    red8 = "UserCastle.cs:930" in inv8["uncovered"]
    sys.stdout.write("MUT8 audit 清点器(删表项 UserCastle.cs:930): %s（uncovered=%d）\n"
                     % ("RED" if red8 else "GREEN", len(inv8["uncovered"])))
    ok = ok and red8

    # MUT9（token 强度，⑧-6 O-1 / ⑧-10 残余 Minor）：把 `UserCastle.cs:311-446` 整组退化成
    # 只看结束行 `}`（写侧/读侧仍成立）⇒ 强度牙 RED。证明该牙不是恒绿。
    weak9 = [("UserCastle.cs:311-446", UC, [(446, "}")]) if a[0] == "UserCastle.cs:311-446" else a
             for a in ANCHORS]
    f9 = _evaluate(anchors=weak9, castle_text=castle_text, readme_text=readme_text)
    red9 = any("token 强度" in f for f in f9)
    sys.stdout.write("MUT9 token 强度(整组只剩 `}`): %s（%d 项）\n"
                     % ("RED" if red9 else "GREEN", len(f9)))
    ok = ok and red9

    sys.stdout.write("MUTATION %s (%s)\n" % ("GREEN" if ok else "RED", out))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("check")
    p.add_argument("--json")
    p.set_defaults(func=cmd_check)
    p = sub.add_parser("audit")
    p.add_argument("--json")
    p.set_defaults(func=cmd_audit)
    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)
    p = sub.add_parser("mutation")
    p.add_argument("--out")
    p.set_defaults(func=cmd_mutation)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
