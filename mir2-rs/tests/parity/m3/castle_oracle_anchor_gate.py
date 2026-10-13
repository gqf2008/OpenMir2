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

3. **覆盖面**：`COVERED_MARKERS` 括出的区间（当前 = ⑧-7 攻城状态机模块头，castle.rs 顶到
   ⑧-8 段之前）内，**每个** `文件.cs:NNN` 锚都必须在 `ANCHORS` 表内（新增锚不登记 ⇒ RED），
   且区间内**不得**留裸锚（必须写成 `文件.cs:NNN`，防 `:NNN` 在不同文件间被误读）。

⑧-8 / ⑧-9 / ⑧-6② 的锚**尚未纳入覆盖区间**（历史锚多，逐条核另开片）；它们可以出现在表里
（写/读侧照检），只是不参与「未登记即红」的覆盖面判定。

## 用法

    python -I castle_oracle_anchor_gate.py check [--json out]
    python -I castle_oracle_anchor_gate.py selftest
    python -I castle_oracle_anchor_gate.py mutation [--out DIR]

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
]

# 覆盖面区间（内容标记 → 行区间）。区间内每个 `.cs:NNN` 锚都必须登记，且不得留裸 `:NNN`。
COVERED_MARKERS = [
    # ⑧-7 攻城状态机模块头 + 常量 + `CastleSiegeConfig`（castle.rs 顶到 ⑧-8 段之前）。
    ("//! 攻城状态机（", "`CastleDoor.cs` / `WallStructure.cs`，⑧-8"),
]

ANCHOR_RE = re.compile(
    r'([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.cs):(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')
BARE_ANCHOR_RE = re.compile(r'(?<![A-Za-z0-9_.]):(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')


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
        ei = next((i for i, line in enumerate(castle_lines) if end_marker in line), None)
        if si is None or ei is None or ei < si:
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

    sys.stdout.write("MUTATION %s (%s)\n" % ("GREEN" if ok else "RED", out))
    return 0 if ok else 1


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("check")
    p.add_argument("--json")
    p.set_defaults(func=cmd_check)
    p = sub.add_parser("selftest")
    p.set_defaults(func=cmd_selftest)
    p = sub.add_parser("mutation")
    p.add_argument("--out")
    p.set_defaults(func=cmd_mutation)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
