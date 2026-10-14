#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""guild_castle_anchor_gate.py — M3⑧-10 行会/攻城夹具 `guild_castle_scenario.json`
`oracle_anchors` 锚一致性门（fail-closed）。

卡：`openmir2-m3-guild-castle-fixture-botsrv`（⑧-10 夹具收口片：reviewer 残余 Minor「锚行号」）。

## 为什么有这道门

⑧-6 reviewer-5 O-1 收口片（patch `3d76cdb9`）把 `guild_castle_scenario.json` 的 M-1
「锚行号」登记为该夹具卡的收口项：`castle_run` 写 `UserCastle.cs:311-448`（实为 :311-446）、
`castle_door` 写 `UserCastle.cs:890-910`（实为 :890-909）；其余 ~40 条**未逐条核**。
本片按冻结 `src/` 逐条机判复核，并把每条锚的**起点/终点行**钉进 `REGISTERED` 表。

口径：
* **冻结 oracle**（`src/M2Server/**`、`src/OpenMir2/**`）⇒ 锚**行号**入 `REGISTERED` 表机判
  （可复现、不会漂）。
* **夹具工具侧**（`src/BotSrv/**`）⇒ 只锚**函数/字段名**（`TOOL_ANCHORS`）、**不给行号**
  （BotSrv 是 T-41..T-50 的在途工具面，行号随夹具演进合法漂移；行号入 fail-closed 门会让
  别人合法的 BotSrv 改动把门打红 —— ⑧-10 残余 Minor；口径同 `hero_pet_anchor_gate.py`）。

## 判据（任一不满足 ⇒ rc=1）

1. **覆盖面**：`oracle_anchors` 里解析出的每个 `File.cs:SPEC` 引用都必须在 `REGISTERED` 表内
   （新增/漏登记的锚 ⇒ RED，防「新锚不登记就假绿」）；反向亦然（表里有、场景里没有 ⇒ RED）。
2. **写侧**：`REGISTERED` 的 `file:spec` 必须能在场景锚文本里按同形解析出来（覆盖面已覆盖该点）。
3. **读侧**：每条锚声明的每个 `(line, token)` 必须命中**冻结** `src/` 的对应行
   （改行号 / 改文件 / 越界 ⇒ RED）。token 取自冻结行本体，锁住锚的位置语义。
4. **读侧 token 强度**（⑧-10 reviewer-6 残余 Minor「结束行只有 `}` 的 token 判据较弱」）：
   每条锚**整组**至少要有一个**有辨识度** token（含字母/数字/下划线/CJK）；整组只剩纯标点/空白
   ⇒ RED（防行号漂移时「只钉在一段空壳行上」假绿）。**O-1 收口（2026-10-14）**：旧字符类
   `^[{}();,\s]*$` 漏 `[]<>+=-*/|&!~%^?.#:` 等 ⇒ 整组只剩 `]);`/`[]`/`//{` 被判「有辨识度」
   而漏红（reviewer-6 f97370f3 构造验证：`]);` ⇒ 无 finding）；现改为「必含字母/数字/下划线/CJK」。
5. **工具侧名字锚**（`TOOL_ANCHORS`）：每条 `(符号, 文件)` 的符号必须**逐字**出现在该 BotSrv
   文件里（只名字、无行号）⇒ 符号被改名/删除 ⇒ RED。

5. **路径前缀**：锚文本里显式写了目录前缀（每个目录段含字母）时，该前缀必须是
   `PATHS` 里登记路径的**路径后缀**（错误前缀如 `src/Evil/Messages.cs:62` ⇒ RED）；
   只写基名仍按既有口径（不误伤）。

## 用法

    python -I guild_castle_anchor_gate.py check [--json out]
    python -I guild_castle_anchor_gate.py selftest
    python -I guild_castle_anchor_gate.py mutation [--out DIR]

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
SCENARIO = os.path.join(HERE, "guild_castle_scenario.json")

# 锚里出现的 C# 文件基名 → 冻结 src/ 相对路径（基名多义（如 RobotPlayer.cs）⇒ 必须点明，
# 不用「找第一个」的猜测口径）。
PATHS = {
    "GuildOfficial.cs": "src/GameSrv/Npc/GuildOfficial.cs",
    "CastleOfficial.cs": "src/GameSrv/Npc/CastleOfficial.cs",
    "Merchant.cs": "src/GameSrv/Npc/Merchant.cs",
    "WorldServer.cs": "src/GameSrv/Word/WorldServer.cs",
    "M2Share.cs": "src/M2Server/M2Share.cs",
    "GuildManager.cs": "src/M2Server/Guild/GuildManager.cs",
    "GuildInfo.cs": "src/M2Server/Guild/GuildInfo.cs",
    "PlayObject.Operate.cs": "src/M2Server/Player/PlayObject.Operate.cs",
    "PlayObject.Base.cs": "src/M2Server/Player/PlayObject.Base.cs",
    "PlayObject.Message.cs": "src/M2Server/Player/PlayObject.Message.cs",
    "UserCastle.cs": "src/M2Server/Castle/UserCastle.cs",
    "CastleDoor.cs": "src/M2Server/Monster/Monsters/CastleDoor.cs",
    "Messages.cs": "src/OpenMir2/Messages.cs",
    # 注：夹具工具侧（src/BotSrv/**）不入本表 —— 见文件头「口径」+ `TOOL_ANCHORS`。
}

# `File.cs:A[-B][/C[-D]...]`；与 castle_oracle_anchor_gate.py 同形（含裸 `/ :NNN` 续写）。
# 锚引用可接受的扩展名集合 —— 由 `PATHS` 登记路径推导（reviewer-6 O-2 / reviewer-5 O-B 收口）：
#   不再硬编码 `cs|sql`；未来 `PATHS` 增列其它扩展名（如 `.txt`）的冻结面文件时，
#   覆盖面解析（`FILE_RE`）与路径前缀判据（`_PATHREF_RE`）都自动覆盖，避免 fail-open。
def _anchor_exts(pather=PATHS):
    exts = set()
    for rel in pather.values():
        ext = os.path.splitext(rel)[1].lstrip(".")
        if ext:
            exts.add(ext)
    return sorted(exts)


def _ext_group(exts):
    return "(?:%s)" % "|".join(re.escape(e) for e in exts) if exts else r"(?!)"


def _file_re(pather=PATHS):
    return re.compile(r'([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.%s)'
                      % _ext_group(_anchor_exts(pather)))


def _pathref_re(pather=PATHS):
    return re.compile(
        r'((?:[A-Za-z0-9_.-]+/)*)([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.%s)(?=\s*:)'
        % _ext_group(_anchor_exts(pather)))


FILE_RE = _file_re(PATHS)
SPEC_RE = re.compile(r'/?\s*:(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')

# 读侧 token 强度（判据 4）：整组只剩纯标点/空白 ⇒ 弱锚 ⇒ RED。
# O-1 收口（2026-10-14）：旧类 `{}();,` 漏 `[]<>+=-*/|&!~%^?.#:` ⇒ 整组只剩 `]);`/`[]` 被误判
# 「有辨识度」（reviewer-6 f97370f3 构造验证）；改为「整串不含字母/数字/下划线/CJK」才算弱。
_WEAK_ONLY_TOKEN_RE = re.compile(r'^[^0-9A-Za-z_\u4e00-\u9fff]*$')

# 锚引用的**路径前缀**（reviewer-6 f98f56f3 O-1 收口）：此前 `FILE_RE` 只取文件基名，
#   带错误目录前缀的锚（如 `src/Evil/Messages.cs:62`）会解析成已登记的 `Messages.cs:62`
#   而通过覆盖面。本门另外校验：锚文本里若显式写了目录前缀，
#   该前缀必须命中 `PATHS[基名]` 的**路径后缀**；只写基名仍按既有口径，不误伤。
#
#   reviewer-6 d52d4a294 后记 O-1（非阻塞）收口：旧实现要求「每个目录段都含字母」以规避
#   `:3966-4105/File.cs` 的 spec 区间误报，副作用是「纯数字/点/横线构成的错误前缀」
#   （如 `1/2/Messages.cs:62`）整段被跳过 ⇒ fail-open。现改为**只剔除紧跟 `:` 且形如
#   `数字[-数字]` 的 spec 区间首段**，其余段一律按目录前缀判（不再要求含字母）。
#   reviewer-6 同批 O-2（非阻塞）收口：扩展名集合改由 `PATHS` 推导（`_pathref_re`），不再硬编码
#   `cs|sql`；`_path_prefix_findings` 按传入 `pather` 逐次构造，便于 selftest 用替身 PATHS 咬住。
_PATHREF_RE = _pathref_re(PATHS)

# spec 区间段（`:3966-4105/File.cs` 里的 `3966-4105`）：仅当该段紧跟在锚文本 `:` 之后时剔除。
_SPEC_RANGE_COMP_RE = re.compile(r'^\d+(?:-\d+)?$')


def _path_prefix_findings(anchor_text, pather=PATHS):
    findings = []
    for m in _pathref_re(pather).finditer(anchor_text):
        comps = [c for c in m.group(1).strip("/").split("/") if c]
        # 剔除紧跟 `:` 的 spec 区间首段（如 `:3966-4105/`）；`1/2/Messages.cs` 这类真错前缀不在其列。
        if (comps and m.start(1) > 0 and anchor_text[m.start(1) - 1] == ":"
                and _SPEC_RANGE_COMP_RE.match(comps[0])):
            comps = comps[1:]
        if not comps:
            continue
        declared = "/".join(comps)
        fname = m.group(2)
        rel = pather.get(fname)
        if rel is None:
            continue  # 基名未登记 ⇒ 由覆盖面判据报告，不在此重复
        want = declared + "/" + fname
        if not (rel == want or rel.endswith("/" + want)):
            findings.append(
                "锚路径前缀：%s 的目录前缀 %r 与登记路径 %r 不符（须为路径后缀）"
                % (m.group(0), declared, rel))
    return findings


def _token_strength_findings(rows):
    findings = []
    for anchor, _rel, points in rows:
        toks = [t.strip() for _l, t in points]
        if not any(not _WEAK_ONLY_TOKEN_RE.match(t) for t in toks):
            findings.append(
                "读侧 token 强度：锚 %s 整组无有辨识度 token（%r）——"
                "至少一条 token 须含实际内容，不能只靠 `}`/`;` 等纯标点钉位置" % (anchor, toks))
    return findings


# (anchor_text, src_relpath, [(line, expected_token), ...])
# 由本片逐条对冻结 src/ 复核给出；token 取冻结行本体。
REGISTERED = [
    ('GuildOfficial.cs:70-72/112-172/142-162/163-170', 'src/GameSrv/Npc/GuildOfficial.cs', [
        (70, 'if (string.Compare(sLabel, ScriptFlagConst.sBUILDGUILDNOW, StringComparison.OrdinalIgnoreCase) == 0)'),
        (72, 'ReQuestBuildGuild(PlayObject, sMsg);'),
        (112, 'private int ReQuestBuildGuild(IPlayerActor PlayObject, string sGuildName)'),
        (172, '}'),
        (142, 'if (SystemShare.GuildMgr.AddGuild(sGuildName, PlayObject.ChrName))'),
        (162, '}'),
        (163, 'if (result >= 0)'),
        (170, '}'),
    ]),
    ('M2Share.cs:529-545', 'src/M2Server/M2Share.cs', [
        (529, 'public static bool CheckGuildName(string sGuildName)'),
        (545, '}'),
    ]),
    ('GuildManager.cs:19-32', 'src/M2Server/Guild/GuildManager.cs', [
        (19, 'public bool AddGuild(string sGuildName, string sChief)'),
        (32, '}'),
    ]),
    ('PlayObject.Operate.cs:1229-1276', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1229, 'private void ClientOpenGuildDlg()'),
        (1276, '}'),
    ]),
    ('PlayObject.Operate.cs:1283-1305', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1283, 'private void ClientGuildMemberList()'),
        (1305, '}'),
    ]),
    ('PlayObject.Operate.cs:1307-1367', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1307, 'private void ClientGuildAddMember(string sHumName)'),
        (1367, '}'),
    ]),
    ('PlayObject.Operate.cs:1369-1424', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1369, 'private void ClientGuildDelMember(string sHumName)'),
        (1424, '}'),
    ]),
    ('PlayObject.Operate.cs:1426-1442', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1426, 'private void ClientGuildUpdateNotice(string sNotict)'),
        (1442, '}'),
    ]),
    ('PlayObject.Operate.cs:1444-1463', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1444, 'private void ClientGuildUpdateRankInfo(string sRankInfo)'),
        (1463, '}'),
    ]),
    ('GuildInfo.cs:708-993', 'src/M2Server/Guild/GuildInfo.cs', [
        (708, 'public int UpdateRank(string sRankData)'),
        (993, '}'),
    ]),
    ('PlayObject.Operate.cs:1465-1523', 'src/M2Server/Player/PlayObject.Operate.cs', [
        (1465, 'public void ClientGuildAlly()'),
        (1523, '}'),
    ]),
    ('GuildInfo.cs:1007-1018/129-159', 'src/M2Server/Guild/GuildInfo.cs', [
        (1007, 'public void AllyGuild(IGuild guild)'),
        (1018, '}'),
        (129, 'public bool DelAllyGuild(IGuild guild)'),
        (159, '}'),
    ]),
    ('GuildOfficial.cs:74-77/180-199', 'src/GameSrv/Npc/GuildOfficial.cs', [
        (74, 'else if (string.Compare(sLabel, ScriptFlagConst.sSCL_GUILDWAR, StringComparison.OrdinalIgnoreCase) == 0)'),
        (77, '}'),
        (180, 'private static void ReQuestGuildWar(IPlayerActor PlayObject, string sGuildName)'),
        (199, '}'),
    ]),
    ('PlayObject.Base.cs:3349-3385', 'src/M2Server/Player/PlayObject.Base.cs', [
        (3349, 'public void ReQuestGuildWar(string sGuildName)'),
        (3385, '}'),
    ]),
    ('GuildInfo.cs:1020-1056', 'src/M2Server/Guild/GuildInfo.cs', [
        (1020, 'public WarGuild AddWarGuild(IGuild guild)'),
        (1056, '}'),
    ]),
    ('GuildManager.cs:145-167', 'src/M2Server/Guild/GuildManager.cs', [
        (145, 'public void Run()'),
        (167, '}'),
    ]),
    ('PlayObject.Message.cs:975-1007', 'src/M2Server/Player/PlayObject.Message.cs', [
        (975, 'case Messages.CM_OPENGUILDDLG:'),
        (1007, 'break;'),
    ]),
    ('GuildOfficial.cs:206-233', 'src/GameSrv/Npc/GuildOfficial.cs', [
        (206, 'private void ReQuestCastleWar(IPlayerActor PlayObject, string sIndex)'),
        (233, '}'),
    ]),
    ('UserCastle.cs:998-1014', 'src/M2Server/Castle/UserCastle.cs', [
        (998, 'public bool AddAttackerInfo(GuildInfo Guild)'),
        (1014, '}'),
    ]),
    ('UserCastle.cs:242-309', 'src/M2Server/Castle/UserCastle.cs', [
        (242, 'private void LoadAttackSabukWall()'),
        (309, '}'),
    ]),
    ('UserCastle.cs:311-446', 'src/M2Server/Castle/UserCastle.cs', [
        (311, 'public void Run()'),
        (446, '}'),
    ]),
    ('CastleDoor.cs:9-116', 'src/M2Server/Monster/Monsters/CastleDoor.cs', [
        (9, 'public class CastleDoor : GuardUnit'),
        (116, '}'),
    ]),
    ('UserCastle.cs:890-909', 'src/M2Server/Castle/UserCastle.cs', [
        (890, 'public void MainDoorControl(bool boClose)'),
        (909, '}'),
    ]),
    ('UserCastle.cs:920-996', 'src/M2Server/Castle/UserCastle.cs', [
        (920, 'public bool RepairDoor()'),
        (996, '}'),
    ]),
    ('CastleOfficial.cs:52-67/199-226/358-406', 'src/GameSrv/Npc/CastleOfficial.cs', [
        (52, 'case "$CASTLEDOORSTATE":'),
        (67, 'sMsg = ReplaceVariableText(sMsg, "<$CASTLEDOORSTATE>", sText);'),
        (199, 'else if (string.Compare(sLabel, ScriptFlagConst.sOPENMAINDOOR, StringComparison.OrdinalIgnoreCase) == 0)'),
        (226, '}'),
        (358, 'private void RepairDoor(IPlayerActor PlayObject)'),
        (406, '}'),
    ]),
    ('UserCastle.cs:768-812', 'src/M2Server/Castle/UserCastle.cs', [
        (768, 'public void IncRateGold(int nGold)'),
        (812, '/// <param name="nGold"></param>'),
    ]),
    ('UserCastle.cs:814-889', 'src/M2Server/Castle/UserCastle.cs', [
        (814, 'public int WithDrawalGolds(PlayObject PlayObject, int nGold)'),
        (889, '/// <param name="boClose"></param>'),
    ]),
    ('Merchant.cs:544/1250/1385/1637', 'src/GameSrv/Npc/Merchant.cs', [
        (544, 'if (CastleMerchant || SystemShare.Config.GetAllNpcTax)'),
        (1250, 'if (CastleMerchant || SystemShare.Config.GetAllNpcTax)'),
        (1385, 'if (CastleMerchant || SystemShare.Config.GetAllNpcTax)'),
        (1637, 'if (CastleMerchant || SystemShare.Config.GetAllNpcTax)'),
    ]),
    ('UserCastle.cs:454-478', 'src/M2Server/Castle/UserCastle.cs', [
        (454, 'public bool InCastleWarArea(IEnvirnoment envir, int nX, int nY)'),
        (478, '}'),
    ]),
    ('WorldServer.cs:298-334', 'src/GameSrv/Word/WorldServer.cs', [
        (298, 'playObject.MyGuild = SystemShare.GuildMgr.MemberOfGuild(playObject.ChrName);'),
        (334, 'playObject.CurrY = userCastle.GetHomeY();'),
    ]),
    ('Messages.cs:364/367/368/369/370/371/372/373/374/375/376/379/380/381/382/383', 'src/OpenMir2/Messages.cs', [
        (364, 'public const int SM_CHANGEGUILDNAME = 750;'),
        (367, 'public const int SM_OPENGUILDDLG = 753;'),
        (368, 'public const int SM_OPENGUILDDLG_FAIL = 754;'),
        (369, 'public const int SM_SENDGUILDMEMBERLIST = 756;'),
        (370, 'public const int SM_GUILDADDMEMBER_OK = 757;'),
        (371, 'public const int SM_GUILDADDMEMBER_FAIL = 758;'),
        (372, 'public const int SM_GUILDDELMEMBER_OK = 759;'),
        (373, 'public const int SM_GUILDDELMEMBER_FAIL = 760;'),
        (374, 'public const int SM_GUILDRANKUPDATE_FAIL = 761;'),
        (375, 'public const int SM_BUILDGUILD_OK = 762;'),
        (376, 'public const int SM_BUILDGUILD_FAIL = 763;'),
        (379, 'public const int SM_MENU_OK = 767;'),
        (380, 'public const int SM_GUILDMAKEALLY_OK = 768;'),
        (381, 'public const int SM_GUILDMAKEALLY_FAIL = 769;'),
        (382, 'public const int SM_GUILDBREAKALLY_OK = 770;'),
        (383, 'public const int SM_GUILDBREAKALLY_FAIL = 771;'),
    ]),
]

# 夹具工具侧（`src/BotSrv/**`）名字锚：只机判**符号存在于文件**，不钉行号。
# 背景（⑧-10 残余 Minor / 本片实测）：BotSrv 行号随 T-41..T-50 各夹具片合法漂移
# （④ 掉落/经验片的 T-50 改动把 `SendGuildDlg` 从 4708 推到 4916），钉行号会让
# `fixture_chain_gate.py` 常跑的 `guild_castle_anchor_check` 在 master 上无端 RED。
TOOL_ANCHORS = [
    ("RobotPlayer.SendGuildDlg", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildDlg()"),
    ("RobotPlayer.SendGuildHome", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildHome()"),
    ("RobotPlayer.SendGuildMemberList", "src/BotSrv/Player/RobotPlayer.cs", "private void SendGuildMemberList()"),
    ("RobotPlayer.SendGuildAddMem", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildAddMem(string who)"),
    ("RobotPlayer.SendGuildDelMem", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildDelMem(string who)"),
    ("RobotPlayer.SendGuildUpdateNotice", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildUpdateNotice(string notices)"),
    ("RobotPlayer.SendGuildUpdateGrade", "src/BotSrv/Player/RobotPlayer.cs", "public void SendGuildUpdateGrade(string rankinfo)"),
    ("RobotPlayer.SendMerchantDlgSelect", "src/BotSrv/Player/RobotPlayer.cs", "private void SendMerchantDlgSelect("),
    ("Operate.SM_OPENGUILDDLG", "src/BotSrv/Player/RobotPlayer.Operate.cs", "case Messages.SM_OPENGUILDDLG:"),
    ("Operate.SM_BUILDGUILD_OK", "src/BotSrv/Player/RobotPlayer.Operate.cs", "case Messages.SM_BUILDGUILD_OK:"),
    ("Operate.SM_GUILDDELMEMBER_OK", "src/BotSrv/Player/RobotPlayer.Operate.cs", "case Messages.SM_GUILDDELMEMBER_OK:"),
]


def _tool_symbol_findings(tools=None, src_root=None):
    """工具侧名字锚（判据 5）：符号必须逐字出现在文件里（无行号）。"""
    findings = []
    if tools is None:
        tools = TOOL_ANCHORS
    if src_root is None:
        src_root = REPO_ROOT
    cache = {}
    for name, rel, sym in tools:
        lines = cache.get(rel)
        if lines is None:
            try:
                cache[rel] = lines = _cached_src(src_root, rel)
            except OSError as exc:
                findings.append("工具侧缺件 %s（%s）: %s" % (name, rel, exc))
                continue
        if not any(sym in ln for ln in lines):
            findings.append("工具侧锚 %s：符号 %r 不在 %s（只锚名字；行号漂移不算）"
                            % (name, sym, rel))
    return findings


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _cached_src(src_root, rel):
    return _read_text(os.path.join(src_root, rel)).splitlines()


def parse_refs(anchor_text, pather=PATHS):
    """把一段锚文本解析成有序的 `file:spec` 引用列表（file 用基名）。"""
    out = []
    parts = _file_re(pather).split(anchor_text)
    i = 1
    while i < len(parts):
        fname = parts[i]
        content = parts[i + 1] if i + 1 < len(parts) else ''
        for m in SPEC_RE.finditer(content):
            out.append("%s:%s" % (fname, m.group(1)))
        i += 2
    return out


def scenario_anchor_text(scenario_path=SCENARIO):
    data = json.loads(_read_text(scenario_path))
    anchors = data.get("oracle_anchors")
    if not isinstance(anchors, dict) or not anchors:
        raise ValueError("scenario 缺 oracle_anchors / 非对象")
    return "\n".join("%s = %s" % (k, v) for k, v in anchors.items())


def _evaluate(rows=None, scenario_text=None, src_root=None):
    """返回 findings 列表（空 = GREEN）。"""
    if rows is None:
        rows = REGISTERED
    if src_root is None:
        src_root = REPO_ROOT
    findings = []
    if scenario_text is None:
        scenario_text = scenario_anchor_text()

    parsed = parse_refs(scenario_text)
    parsed_set = set(parsed)
    registered_set = set(anchor for anchor, _rel, _pts in rows)
    for ref in sorted(parsed_set - registered_set):
        findings.append("覆盖面：场景锚未登记 %s（新增锚必须补进 REGISTERED）" % ref)
    for ref in sorted(registered_set - parsed_set):
        findings.append("覆盖面：REGISTERED 有而场景锚里没有 %s（锚被删/改形 ⇒ 表过期）" % ref)
    if len(parsed) != len(parsed_set):
        findings.append("覆盖面：场景锚里有重复引用（%d 条 / %d 个唯一）"
                        % (len(parsed), len(parsed_set)))

    cache = {}
    for anchor, rel, points in rows:
        fname = anchor.split(":", 1)[0]
        if PATHS.get(fname) != rel:
            findings.append("表内路径与 PATHS 不一致 %s：%r vs %r" % (anchor, rel, PATHS.get(fname)))
            continue
        lines = cache.get(rel)
        if lines is None:
            try:
                lines = cache[rel] = _cached_src(src_root, rel)
            except OSError as exc:
                findings.append("读侧缺件 %s: %s" % (rel, exc))
                continue
        for line, token in points:
            if token.startswith("<OOB"):
                findings.append("读侧越界登记 %s:%d（表内 token=%r）" % (anchor, line, token))
                continue
            if not token:
                findings.append("读侧空 token %s:%d（无法机判）" % (anchor, line))
                continue
            if line < 1 or line > len(lines):
                findings.append("读侧越界 %s:%d（文件 %d 行）" % (anchor, line, len(lines)))
                continue
            if token not in lines[line - 1]:
                findings.append("读侧不匹配 %s:%d 期望含 %r，实际 %r"
                                % (anchor, line, token, lines[line - 1].strip()))
    findings.extend(_token_strength_findings(rows))
    findings.extend(_path_prefix_findings(scenario_text))
    findings.extend(_tool_symbol_findings(src_root=src_root))
    return findings


def cmd_check(args):
    findings = _evaluate()
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "anchors": len(REGISTERED),
                       "points": sum(len(p) for _, _, p in REGISTERED)},
                      fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: guild/castle 场景锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(REGISTERED)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: guild/castle 场景锚门通过（%d 锚 / %d 行点全部命中冻结 src/ 且覆盖面闭合）\n"
                     % (len(REGISTERED), sum(len(p) for _, _, p in REGISTERED)))
    return 0


def cmd_selftest(args):
    ok = True
    # 1) 表结构自检。
    for anchor, rel, points in REGISTERED:
        if not anchor or not rel or not points:
            sys.stdout.write("RED: 表项不完整 %r\n" % (anchor,))
            ok = False
        for line, token in points:
            if line < 1 or not token:
                sys.stdout.write("RED: 表项 %s 的 (line, token) 非法 %r\n" % (anchor, (line, token)))
                ok = False
    sys.stdout.write("selftest 表结构: %s (%d 锚)\n" % ("GREEN" if ok else "RED", len(REGISTERED)))

    good = [("UserCastle.cs:311-446", PATHS["UserCastle.cs"], [
        (311, "public void Run()"), (446, "}")])]
    bad = [("UserCastle.cs:311-446", PATHS["UserCastle.cs"], [
        (311, "public void Run()"), (446, "ThisTokenDoesNotExistXYZ")])]
    stub_scen = "UserCastle.cs:311-446 note"
    g = _evaluate(rows=good, scenario_text=stub_scen)
    b = _evaluate(rows=bad, scenario_text=stub_scen)
    t1 = (g == [] and len(b) == 1 and "读侧不匹配" in b[0])
    sys.stdout.write("selftest 读侧 token 牙齿: %s (good=%r bad=%d)\n"
                     % ("GREEN" if t1 else "RED", g, len(b)))
    ok = ok and t1

    # 2) 覆盖面牙：场景里多一个未登记锚 ⇒ RED。
    extra = stub_scen + "\nGhost.cs:1-2"
    e = _evaluate(rows=good, scenario_text=extra)
    t2 = any("未登记 Ghost.cs:1-2" in f for f in e)
    sys.stdout.write("selftest 覆盖面未登记牙齿: %s\n" % ("GREEN" if t2 else "RED"))
    ok = ok and t2

    # 3) 覆盖面牙：表里有而场景没有 ⇒ RED。
    m = _evaluate(rows=REGISTERED, scenario_text=stub_scen)
    t3 = any("REGISTERED 有而场景锚里没有" in f for f in m)
    sys.stdout.write("selftest 覆盖面表过期牙齿: %s (%d 项)\n" % ("GREEN" if t3 else "RED", len(m)))
    ok = ok and t3

    # 4) 越界牙：登记一个越界行 ⇒ RED。
    oob = [("UserCastle.cs:311-446", PATHS["UserCastle.cs"], [(999999, "x")])]
    o = _evaluate(rows=oob, scenario_text=stub_scen)
    t4 = any("读侧越界" in f for f in o)
    sys.stdout.write("selftest 越界牙齿: %s\n" % ("GREEN" if t4 else "RED"))
    ok = ok and t4

    # 5) 真身 check 必须 GREEN。
    cov_real = _evaluate(rows=REGISTERED)
    t5 = cov_real == []
    sys.stdout.write("selftest 真身 check: %s (%d 项)\n" % ("GREEN" if t5 else "RED", len(cov_real)))
    for f in cov_real:
        sys.stdout.write("  - %s\n" % f)
    ok = ok and t5

    # 6) token 强度牙（⑧-10 残余 Minor）：整组只剩结束行 `}` ⇒ RED。
    weak_only = [("UserCastle.cs:311-446", PATHS["UserCastle.cs"], [(446, "}")])]
    w = _evaluate(rows=weak_only, scenario_text=stub_scen)
    t6a = len(w) == 1 and "token 强度" in w[0]
    # 6b) O-1 收口（2026-10-14）：纯标点整组 `]);`/`[]`/`*/`/`//{` 旧类漏红 ⇒ 新谓词必须咬；
    #     含内容 token 的整组不误报。
    o1_rows = [
        ("X.cs:1-2", PATHS["UserCastle.cs"], [(2, "]);")]),
        ("Y.cs:1-2", PATHS["UserCastle.cs"], [(2, "[]")]),
        ("Z.cs:1-2", PATHS["UserCastle.cs"], [(2, "*/")]),
        ("W.cs:1-2", PATHS["UserCastle.cs"], [(2, "//{")]),
        ("V.cs:1-2", PATHS["UserCastle.cs"], [(2, "//{"), (3, "foo")]),
    ]
    o1 = _token_strength_findings(o1_rows)
    t6b = len(o1) == 4 and all("token 强度" in f for f in o1)
    t6 = t6a and t6b
    sys.stdout.write("selftest token 强度牙齿: %s (weak=%r; O-1 pure-punct=%d/4)\n"
                     % ("GREEN" if t6 else "RED", w, len(o1)))
    ok = ok and t6

    # 7) 工具侧名字锚牙（⑧-10 残余 Minor）：符号不存在 ⇒ RED；真身 TOOL_ANCHORS 全绿。
    tool_bad = [("X.fake", "src/BotSrv/Player/RobotPlayer.cs", "public void ThisSymbolDoesNotExistXYZ(")]
    tb = _tool_symbol_findings(tools=tool_bad)
    t7 = len(tb) == 1 and "工具侧锚" in tb[0]
    sys.stdout.write("selftest 工具侧名字锚牙齿: %s (bad=%r)\n" % ("GREEN" if t7 else "RED", tb))
    ok = ok and t7

    # 7) 扩展名推导牙（reviewer-6 O-2 / reviewer-5 O-B 收口）：锚引用的扩展名集合由 `PATHS`
    #    推导 ⇒ 未来 `PATHS` 增列非 `.cs`/`.sql` 的冻结面文件时，覆盖面解析与路径前缀判据
    #    都自动覆盖（旧实现硬编码 `cs|sql` ⇒ 该锚的错前缀被整段跳过 = fail-open）。
    fake_paths = {"Foo.gd": "src/Evil2/Foo.gd"}
    t7 = (parse_refs("Foo.gd:1", fake_paths) == ["Foo.gd:1"]
          and _path_prefix_findings("Bad/Dir/Foo.gd:1", fake_paths) != []
          and _path_prefix_findings("src/Evil2/Foo.gd:1", fake_paths) == []
          and _path_prefix_findings("Foo.gd:1", fake_paths) == [])
    sys.stdout.write("selftest 扩展名推导牙齿(O-2): %s\n" % ("GREEN" if t7 else "RED"))
    ok = ok and t7

    sys.stdout.write("SELFTEST %s\n" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def cmd_mutation(args):
    out = args.out or tempfile.mkdtemp(prefix="gc-anchor-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    real = scenario_anchor_text()

    # MUT1：把 castle_run 的范围改回历史错值 311-448 ⇒ 覆盖面（未登记）+ 读侧（446 不再是表内）⇒ RED。
    mut1 = real.replace("UserCastle.cs:311-446", "UserCastle.cs:311-448")
    if mut1 == real:
        sys.stdout.write("BUG: 变异 MUT1 找不到 UserCastle.cs:311-446\n")
        ok = False
    f1 = _evaluate(scenario_text=mut1)
    red1 = any("未登记 UserCastle.cs:311-448" in f for f in f1)
    sys.stdout.write("MUT1 范围漂移(:311-446→:311-448): %s（%d 项）\n"
                     % ("RED" if f1 else "GREEN", len(f1)))
    ok = ok and red1

    # MUT2：把 castle_door 的 890-909 改回 890-910 ⇒ RED。
    mut2 = real.replace("UserCastle.cs:890-909", "UserCastle.cs:890-910")
    if mut2 == real:
        sys.stdout.write("BUG: 变异 MUT2 找不到 UserCastle.cs:890-909\n")
        ok = False
    f2 = _evaluate(scenario_text=mut2)
    red2 = any("未登记 UserCastle.cs:890-910" in f for f in f2)
    sys.stdout.write("MUT2 范围漂移(:890-909→:890-910): %s（%d 项）\n"
                     % ("RED" if f2 else "GREEN", len(f2)))
    ok = ok and red2

    # MUT3：把 GuildManager.cs:19-32 的表内 token 改坏（AddGuild → AddGuildXYZ 不存在）⇒ 读侧 RED。
    bad_rows = []
    hit = False
    for anchor, rel, pts in REGISTERED:
        if anchor == "GuildManager.cs:19-32":
            hit = True
            pts = [(19, "public bool AddGuildXYZNope("), (32, "}")]
        bad_rows.append((anchor, rel, pts))
    if not hit:
        sys.stdout.write("BUG: 变异 MUT3 找不到 GuildManager.cs:19-32\n")
        ok = False
    f3 = _evaluate(rows=bad_rows, scenario_text=real)
    red3 = any("读侧不匹配" in f for f in f3)
    sys.stdout.write("MUT3 读侧 token 漂移: %s（%d 项）\n" % ("RED" if f3 else "GREEN", len(f3)))
    ok = ok and red3

    # MUT4（回归）：真身 check 必须 GREEN。
    f0 = _evaluate(scenario_text=real)
    sys.stdout.write("MUT4 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
    ok = ok and not f0

    # MUT5（token 强度，⑧-10 残余 Minor）：把 `UserCastle.cs:311-446` 整组退化成只看结束行 `}`
    # （写侧/读侧仍成立）⇒ 强度牙 RED。
    weak5 = [("UserCastle.cs:311-446", PATHS["UserCastle.cs"], [(446, "}")])
             if a[0] == "UserCastle.cs:311-446" else a for a in REGISTERED]
    f5 = _evaluate(rows=weak5, scenario_text=real)
    red5 = any("token 强度" in f for f in f5)
    sys.stdout.write("MUT5 token 强度(整组只剩 `}`): %s（%d 项）\n"
                     % ("RED" if red5 else "GREEN", len(f5)))
    ok = ok and red5

    # MUT6（工具侧名字锚，⑧-10 残余 Minor）：把一条工具侧符号改成不存在的名字 ⇒ RED；
    # 证明名字锚不是恒绿，且不再拿行号钉 BotSrv（行号漂移不会让它红）。
    tb6 = _tool_symbol_findings(tools=[("RobotPlayer.SendGuildDlg", "src/BotSrv/Player/RobotPlayer.cs",
                                        "public void SendGuildDlgXYZNope(")])
    red6 = len(tb6) == 1 and "工具侧锚" in tb6[0]
    sys.stdout.write("MUT6 工具侧名字锚(改坏符号): %s（%d 项）\n"
                     % ("RED" if red6 else "GREEN", len(tb6)))
    ok = ok and red6

    # MUT7（回归）：真身 TOOL_ANCHORS 符号全在 ⇒ GREEN。
    tb7 = _tool_symbol_findings()
    sys.stdout.write("MUT7 工具侧真身名字锚: %s（%d 项）\n"
                     % ("GREEN" if not tb7 else "RED", len(tb7)))
    ok = ok and not tb7

    # MUT8（O-1 收口，2026-10-14）：O-1 点名的纯标点整组 `]);`（旧类判「有辨识度」⇒ 漏红）与
    # `]`/`//{` 必须被新谓词咬住。证明强度牙对 O-1 边界不再恒绿。
    o1a = _token_strength_findings([("UserCastle.cs:311-446", PATHS["UserCastle.cs"],
                                     [(446, "]);")])])
    o1b = _token_strength_findings([("X.cs:1-2", PATHS["UserCastle.cs"], [(2, "]")]),
                                    ("Y.cs:1-2", PATHS["UserCastle.cs"], [(2, "//{")])])
    red8 = len(o1a) == 1 and "token 强度" in o1a[0] and len(o1b) == 2
    sys.stdout.write("MUT8 token 强度(O-1 纯标点 `]);`/`]`/`//{`): %s（%d+%d 项）\n"
                     % ("RED" if red8 else "GREEN", len(o1a), len(o1b)))
    ok = ok and red8

    # MUT-O1（路径前缀，reviewer-6 f98f56f3 O-1 收口）：锚文本显式写了目录前缀时，
    #   该前缀必须是登记路径的**路径后缀**；错误前缀（如 `src/Evil/Messages.cs:62`）
    #   不再被「只取基名」放过 ⇒ RED；正确前缀 / 只写基名 ⇒ 不产生 finding。
    pf_key = next(k for k, v in PATHS.items() if "/" in v)
    pf_bad = _path_prefix_findings("src/Evil/" + pf_key + ":1")
    pf_num = _path_prefix_findings("1/2/" + pf_key + ":1")
    pf_spec = _path_prefix_findings(":3966-4105/" + pf_key + ":1")
    pf_ok = _path_prefix_findings(PATHS[pf_key] + ":1")
    pf_bare = _path_prefix_findings(pf_key + ":1")
    red_o1 = (len(pf_bad) == 1 and "锚路径前缀" in pf_bad[0]
              and len(pf_num) == 1 and "锚路径前缀" in pf_num[0]
              and not pf_spec and not pf_ok and not pf_bare)
    sys.stdout.write("MUT-O1 路径前缀(错前缀/纯非字母前缀必红；spec 区间·正确·裸基名不误伤): %s（%d+%d 项）\n"
                     % ("RED" if red_o1 else "GREEN", len(pf_bad), len(pf_num)))
    ok = ok and red_o1

    # MUT-O2（扩展名推导，reviewer-6 O-2 / reviewer-5 O-B 收口）：把扩展名集合退化成旧的
    #   硬编码 `cs|sql` ⇒ 非 `.cs`/`.sql` 冻结面锚的错前缀不再被咬（fail-open 复现）⇒ 牙可失败向。
    _saved_ext_group = globals()["_ext_group"]
    globals()["_ext_group"] = lambda _exts: r"(?:cs|sql)"
    try:
        o2_fake = {"Foo.gd": "src/Evil2/Foo.gd"}
        mut_o2 = (parse_refs("Foo.gd:1", o2_fake) == []
                  and _path_prefix_findings("Bad/Dir/Foo.gd:1", o2_fake) == [])
    finally:
        globals()["_ext_group"] = _saved_ext_group
    red_o2 = mut_o2
    sys.stdout.write("MUT-O2 扩展名硬编码退化(非 .cs/.sql 错前缀漏判): %s（漏判=%s）\n"
                     % ("RED" if red_o2 else "GREEN", "是" if mut_o2 else "否"))
    ok = ok and red_o2

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
