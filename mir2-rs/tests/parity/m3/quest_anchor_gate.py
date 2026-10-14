#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""quest_anchor_gate.py — M3⑥-5 任务夹具 `quest_scenario.json` `oracle_anchors`
锚一致性门（fail-closed）。

卡：`openmir2-m3-quest-fixture-botsrv`（⑥-5 夹具；锚门族覆盖面收口片）。

## 为什么有这道门

M3 十副动作序列夹具的场景锚门族当前是「谁被 reviewer 点名谁补」：
* ⑧-6 / ⑧-10 / ⑨-5 / ⑩-3 各有 `castle_oracle_anchor_gate.py` /
  `guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py` /
  `selloff_anchor_gate.py`，均已登记进 `fixture_chain_gate.py` 常跑；
* ⑥（任务）/ ⑦（组队交易）两个场景件同样带 `oracle_anchors`，但**从来没有行级门**
  ⇒ 锚可以静默漂移 / 复制错行号 / 写错文件基名（`Query.cs` 其实不在树里）而不被发现。
本片把 ⑥ 补齐（⑦ 见 `party_trade_anchor_gate.py`）：**逐条复核 + 订正漂移 + 用本门锁死**。

口径（与 ⑧-6 / ⑧-10 / ⑨-5 / ⑩-3 四锚门同形）：
* **冻结 oracle**（`src/M2Server/**`、`src/GameSrv/**`、`src/OpenMir2/**`、
  `src/Modules/**`、`src/Storeages/**`、`sql/**`）⇒ 锚**行号**入表机判（可复现、不会漂）。
* 本场景的锚全部落在冻结面，**没有** `src/BotSrv/**` 工具侧行号锚 ⇒ `TOOL_ANCHORS` 为空
  （机制保留，供将来加名字锚）。

## 本片订正（逐条对冻结 src/ 复核后）

1. `quest_bits` 的范围终点 `PlayObject.cs:3966-4110` 越过了最后一个任务位函数
   （`SetQuestUnitStatus` 收在 :4105），落到了下一函数 `CheckWeaponUpgrade` 的注释上
   ⇒ 订正为 `:3966-4105`。
2. `no_persist` 的两个文件基名在仓库里**不存在**：`Query.cs`（实为
   `PlayDataStorage.Query.cs`）、`Save.cs`（实为 `PlayDataStorage.Save.cs`）
   ⇒ 补全为真名（同锚里另一条本来就写 `PlayDataStorage.Save.cs:45`，自相矛盾）。
   其余 12 个引用的行号逐条命中冻结行，未漂移。

## 判据（任一不满足 ⇒ rc=1）

1. **覆盖面**：`oracle_anchors` 里解析出的每个 `file:spec` 引用都必须在 `REGISTERED`
   表内（新增/漏登记的锚 ⇒ RED）；反向亦然（表里有、场景里没有 ⇒ RED）；场景锚里
   重复引用同一 `file:spec` ⇒ RED。
2. **写侧**：`REGISTERED` 的 `file:spec` 必须能在场景锚文本里按同形解析出来。
3. **读侧**：每条锚声明的每个 `(line, token)` 必须命中**冻结** `src/` 的对应行
   （改行号 / 改文件 / 越界 ⇒ RED）。token 取自冻结行本体，锁住锚的位置语义
   （如 `characters_quest` 的 `DROP TABLE`、`SaveQuest` 空壳 ⇒ 谁顺手「补全」任务落库
   就 RED；行为等价优先，另开 `S-*`）。
4. **读侧 token 强度**：每条锚**整组**至少要有一个**有辨识度** token（含字母/数字/
   下划线/CJK）；整组只剩纯标点/空白 ⇒ RED（防行号漂移时「只钉在一段空壳行上」假绿）。

5. **路径前缀**：锚文本里显式写了目录前缀（每个目录段含字母）时，该前缀必须是
   `PATHS` 里登记路径的**路径后缀**（错误前缀如 `src/Evil/Messages.cs:62` ⇒ RED）；
   只写基名仍按既有口径（不误伤）。

## 用法

    python -I quest_anchor_gate.py check [--json out]
    python -I quest_anchor_gate.py selftest
    python -I quest_anchor_gate.py mutation [--out DIR]

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
SCENARIO = os.path.join(HERE, "quest_scenario.json")

# 锚里出现的文件基名 → 冻结面相对路径（基名多义 ⇒ 必须点明，不用「找第一个」的猜测口径）。
PATHS = {
    "Messages.cs": "src/OpenMir2/Messages.cs",
    "PlayObject.Base.cs": "src/M2Server/Player/PlayObject.Base.cs",
    "NormNpc.cs": "src/M2Server/Npc/NormNpc.cs",
    "ScriptEngine.cs": "src/Modules/ScriptEngine/ScriptEngine.cs",
    "PlayObject.Message.cs": "src/M2Server/Player/PlayObject.Message.cs",
    "PlayObject.cs": "src/M2Server/Player/PlayObject.cs",
    "WorldServer.cs": "src/GameSrv/Word/WorldServer.cs",
    "PlayDataStorage.Save.cs": "src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs",
    "PlayDataStorage.Query.cs": "src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs",
    "mir2_db.sql": "sql/mir2_db.sql",
}

# `File.cs:A[-B][/C[-D]...]`（.cs 与 .sql 两族）；与 ⑧/⑨/⑩ 四锚门同形。
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

# 读侧 token 强度（判据 4）：整组不含字母/数字/下划线/CJK ⇒ 弱锚 ⇒ RED。
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
    ('Messages.cs:62', 'src/OpenMir2/Messages.cs', [
        (62, 'public const int CM_CLICKNPC = 1010;'),
    ]),
    ('PlayObject.Base.cs:674', 'src/M2Server/Player/PlayObject.Base.cs', [
        (674, 'private void ClientClickNpc(int actorId)'),
    ]),
    ('NormNpc.cs:68', 'src/M2Server/Npc/NormNpc.cs', [
        (68, 'public virtual void Click(IPlayerActor PlayObject)'),
    ]),
    ('ScriptEngine.cs:987-998', 'src/Modules/ScriptEngine/ScriptEngine.cs', [
        (987, 'private void GotoLableSendMerChantSayMsg(INormNpc normNpc, IPlayerActor playerActor, string sMsg, bool boFlag)'),
        (998, '}'),
    ]),
    ('PlayObject.Message.cs:1702-1724', 'src/M2Server/Player/PlayObject.Message.cs', [
        (1702, 'case Messages.RM_MERCHANTSAY:'),
        (1724, 'ClientMsg = Messages.MakeMessage(Messages.SM_MERCHANTSAY, processMsg.ActorId'),
    ]),
    ('PlayObject.cs:3966-4105', 'src/M2Server/Player/PlayObject.cs', [
        (3966, 'public int GetQuestFalgStatus(int nFlag)'),
        (4105, '}'),
    ]),
    ('WorldServer.cs:1160-1162', 'src/GameSrv/Word/WorldServer.cs', [
        (1160, 'playObject.QuestUnitOpen = humData.QuestUnitOpen;'),
        (1162, 'playObject.QuestFlag = humData.QuestFlag;'),
    ]),
    ('WorldServer.cs:1291-1293', 'src/GameSrv/Word/WorldServer.cs', [
        (1291, 'playerInfo.QuestUnitOpen = playObject.QuestUnitOpen;'),
        (1293, 'playerInfo.QuestFlag = playObject.QuestFlag;'),
    ]),
    ('PlayDataStorage.Save.cs:45', 'src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs', [
        (45, 'private bool SaveRecord(int playerId, ref CharacterDataInfo humanRcd)'),
    ]),
    ('PlayDataStorage.Save.cs:84-86', 'src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs', [
        (84, 'StringBuilder strSql = new StringBuilder();'),
        (86, 'GamePoint = @GamePoint, HomeMap = @HomeMap'),
    ]),
    ('PlayDataStorage.Query.cs:192', 'src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs', [
        (192, 'private CharacterData GetChrRecord(int playerId, StorageContext context)'),
    ]),
    ('PlayDataStorage.Query.cs:194', 'src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Query.cs', [
        (194, 'SELECT * FROM characters WHERE ID=@ID'),
    ]),
    ('PlayDataStorage.Save.cs:493', 'src/Storeages/DBSvr.Storage.MySQL/PlayDataStorage.Save.cs', [
        (493, 'private void SaveQuest(StorageContext context, int id, CharacterDataInfo humanRcd)'),
    ]),
    ('mir2_db.sql:371936', 'sql/mir2_db.sql', [
        (371936, 'DROP TABLE IF EXISTS `characters_quest`;'),
    ]),
]

# 夹具工具侧（`src/BotSrv/**`）名字锚：本场景没有工具侧锚 ⇒ 空表（机制保留）。
TOOL_ANCHORS = []


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
        sys.stdout.write("RED: quest 场景锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(REGISTERED)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: quest 场景锚门通过（%d 锚 / %d 行点全部命中冻结 src/ 且覆盖面闭合）\n"
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

    good = [("PlayObject.cs:3966-4105", PATHS["PlayObject.cs"], [
        (3966, "public int GetQuestFalgStatus(int nFlag)"), (4105, "}")])]
    bad = [("PlayObject.cs:3966-4105", PATHS["PlayObject.cs"], [
        (3966, "public int GetQuestFalgStatus(int nFlag)"), (4105, "ThisTokenDoesNotExistXYZ")])]
    stub_scen = "PlayObject.cs:3966-4105 note"
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
    oob = [("PlayObject.cs:3966-4105", PATHS["PlayObject.cs"], [(999999, "x")])]
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

    # 6) token 强度牙：整组只剩结束行 `}` ⇒ RED；纯标点整组（`]);`/`[]`/`//{`）⇒ RED；
    #    含内容 token 的整组不误报。
    weak_only = [("PlayObject.cs:3966-4105", PATHS["PlayObject.cs"], [(4105, "}")])]
    w = _evaluate(rows=weak_only, scenario_text=stub_scen)
    t6a = len(w) == 1 and "token 强度" in w[0]
    o1_rows = [
        ("X.cs:1-2", PATHS["PlayObject.cs"], [(2, "]);")]),
        ("Y.cs:1-2", PATHS["PlayObject.cs"], [(2, "[]")]),
        ("Z.cs:1-2", PATHS["PlayObject.cs"], [(2, "*/")]),
        ("W.cs:1-2", PATHS["PlayObject.cs"], [(2, "//{")]),
        ("V.cs:1-2", PATHS["PlayObject.cs"], [(2, "//{"), (3, "foo")]),
    ]
    o1 = _token_strength_findings(o1_rows)
    t6b = len(o1) == 4 and all("token 强度" in f for f in o1)
    t6 = t6a and t6b
    sys.stdout.write("selftest token 强度牙齿: %s (weak=%r; pure-punct=%d/4)\n"
                     % ("GREEN" if t6 else "RED", w, len(o1)))
    ok = ok and t6

    # 7) 工具侧名字锚牙：符号不存在 ⇒ RED（本场景 TOOL_ANCHORS 为空，用构造表验证机制）。
    tool_bad = [("X.fake", "src/OpenMir2/Messages.cs", "public void ThisSymbolDoesNotExistXYZ(")]
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
    out = args.out or tempfile.mkdtemp(prefix="quest-anchor-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    real = scenario_anchor_text()

    # MUT1：把 quest_bits 的范围改回历史错值 3966-4110 ⇒ 覆盖面（未登记）⇒ RED。
    mut1 = real.replace("PlayObject.cs:3966-4105", "PlayObject.cs:3966-4110")
    if mut1 == real:
        sys.stdout.write("BUG: 变异 MUT1 找不到 PlayObject.cs:3966-4105\n")
        ok = False
    f1 = _evaluate(scenario_text=mut1)
    red1 = any("未登记 PlayObject.cs:3966-4110" in f for f in f1)
    sys.stdout.write("MUT1 范围漂移(:3966-4105→:3966-4110): %s（%d 项）\n"
                     % ("RED" if f1 else "GREEN", len(f1)))
    ok = ok and red1

    # MUT2：把补全的真名改回不存在的基名 Query.cs ⇒ 覆盖面（未登记 Query.cs:192）⇒ RED。
    mut2 = real.replace("PlayDataStorage.Query.cs:192", "Query.cs:192")
    if mut2 == real:
        sys.stdout.write("BUG: 变异 MUT2 找不到 PlayDataStorage.Query.cs:192\n")
        ok = False
    f2 = _evaluate(scenario_text=mut2)
    red2 = any("未登记 Query.cs:192" in f for f in f2)
    sys.stdout.write("MUT2 基名漂移(PlayDataStorage.Query.cs→Query.cs): %s（%d 项）\n"
                     % ("RED" if f2 else "GREEN", len(f2)))
    ok = ok and red2

    # MUT3：把 mir2_db.sql 的表内 token 改坏 ⇒ 读侧 RED。
    bad_rows = []
    hit = False
    for anchor, rel, pts in REGISTERED:
        if anchor == "mir2_db.sql:371936":
            hit = True
            pts = [(371936, "DROP TABLE IF EXISTS `characters_quest_XYZ`;")]
        bad_rows.append((anchor, rel, pts))
    if not hit:
        sys.stdout.write("BUG: 变异 MUT3 找不到 mir2_db.sql:371936\n")
        ok = False
    f3 = _evaluate(rows=bad_rows, scenario_text=real)
    red3 = any("读侧不匹配" in f for f in f3)
    sys.stdout.write("MUT3 读侧 token 漂移: %s（%d 项）\n" % ("RED" if f3 else "GREEN", len(f3)))
    ok = ok and red3

    # MUT4（回归）：真身 check 必须 GREEN。
    f0 = _evaluate(scenario_text=real)
    sys.stdout.write("MUT4 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
    ok = ok and not f0

    # MUT5（token 强度）：整组退化成只看 `}` ⇒ RED。
    weak5 = [("PlayObject.cs:3966-4105", PATHS["PlayObject.cs"], [(4105, "}")])
             if a[0] == "PlayObject.cs:3966-4105" else a for a in REGISTERED]
    f5 = _evaluate(rows=weak5, scenario_text=real)
    red5 = any("token 强度" in f for f in f5)
    sys.stdout.write("MUT5 token 强度(整组只剩 `}`): %s（%d 项）\n"
                     % ("RED" if red5 else "GREEN", len(f5)))
    ok = ok and red5

    # MUT6：纯标点整组（`]);`/`]`/`//{`）必须被强度牙咬住。
    o1a = _token_strength_findings([("X.cs:1-2", PATHS["PlayObject.cs"], [(2, "]);")])])
    o1b = _token_strength_findings([("X.cs:1-2", PATHS["PlayObject.cs"], [(2, "]")]),
                                    ("Y.cs:1-2", PATHS["PlayObject.cs"], [(2, "//{")])])
    red6 = len(o1a) == 1 and "token 强度" in o1a[0] and len(o1b) == 2
    sys.stdout.write("MUT6 token 强度(纯标点 `]);`/`]`/`//{`): %s（%d+%d 项）\n"
                     % ("RED" if red6 else "GREEN", len(o1a), len(o1b)))
    ok = ok and red6

    with open(os.path.join(out, "quest_mutation.txt"), "w", encoding="utf-8", newline="\n") as fh:
        fh.write("MUTATION %s\n" % ("GREEN" if ok else "RED"))
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
