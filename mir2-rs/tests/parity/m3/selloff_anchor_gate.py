#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""selloff_anchor_gate.py — M3⑩-3 元宝寄售夹具 `selloff_scenario.json`
`oracle_anchors` 锚一致性门（fail-closed）。

卡：`openmir2-m3-selloff-fixture-botsrv`（⑩-3 夹具残余 Minor 收口片：
reviewer-5 `7a575abe` 的 M-2「场景 §B 的 oracle 锚点本轮未行级核对」）。

## 为什么有这道门

⑩-3 夹具骨架的 reviewer-5 M-2 指出：`selloff_scenario.json` 的 `oracle_anchors`
**本轮没有逐条对冻结 `src/` 行级核对**。⑧-10 / ⑨-5 两族夹具随后各补了
`guild_castle_anchor_gate.py` / `hero_pet_anchor_gate.py`（同形 fail-closed 门 + 常驻
夹具链深检门），⑩ 缺失 ⇒ 场景锚可以静默漂移 / 复制错行号而不被发现。本片把 ⑩ 补齐：
**逐条复核 + 订正漂移 + 用本门锁死**。

口径（与 ⑧-6 / ⑧-10 / ⑨-5 三锚门同形）：
* **冻结 oracle**（`src/M2Server/**`、`src/GameSrv/**`、`src/OpenMir2/**`、
  `src/Modules/**`、`sql/**`）⇒ 锚**行号**入表机判（可复现、不会漂）。
* 本场景的锚全部落在冻结面，**没有** `src/BotSrv/**` 工具侧行号锚 ⇒ 不需要 ⑧-10 的
  `TOOL_ANCHORS` 名字锚分支。

## 判据（任一不满足 ⇒ rc=1）

1. **覆盖面**：`oracle_anchors` 里解析出的每个 `file:spec` 引用都必须在 `REGISTERED`
   表内（新增/漏登记的锚 ⇒ RED）；反向亦然（表里有、场景里没有 ⇒ RED）；场景锚里
   重复引用同一 `file:spec` ⇒ RED。
2. **写侧**：`REGISTERED` 的 `file:spec` 必须能在场景锚文本里按同形解析出来。
3. **读侧**：每条锚声明的每个 `(line, token)` 必须命中**冻结** `src/` 的对应行
   （改行号 / 改文件 / 越界 ⇒ RED）。token 取自冻结行本体，锁住锚的位置语义。
   `MySqlDB.cs` 读列名与 DDL 列名不符（oracle「恒加载 0 行」缺陷）的**缺陷本体**
   也被 token 锁死 ⇒ 谁顺手「修好」oracle 就 RED（行为等价优先，另开 `S-*`）。
4. **读侧 token 强度**：每条锚**整组**至少要有一个有辨识度 token（含字母/数字/
   下划线/CJK）；整组只剩纯标点/空白 ⇒ RED（防行号漂移时只钉在一段空壳行上假绿）。

5. **路径前缀**：锚文本里显式写了目录前缀（每个目录段含字母）时，该前缀必须是
   `PATHS` 里登记路径的**路径后缀**（错误前缀如 `src/Evil/Messages.cs:62` ⇒ RED）；
   只写基名仍按既有口径（不误伤）。

## 用法

    python -I selloff_anchor_gate.py check [--json out]
    python -I selloff_anchor_gate.py selftest
    python -I selloff_anchor_gate.py mutation [--out DIR]

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
SCENARIO = os.path.join(HERE, "selloff_scenario.json")

# 锚里出现的文件基名 → 冻结面相对路径（基名多义 ⇒ 必须点明，不用「找第一个」的猜测口径）。
PATHS = {
    "PlayObject.Message.cs": "src/M2Server/Player/PlayObject.Message.cs",
    "PlayObject.Sale.cs": "src/M2Server/Player/PlayObject.Sale.cs",
    "PlayObject.cs": "src/M2Server/Player/PlayObject.cs",
    "M2Share.cs": "src/M2Server/M2Share.cs",
    "MySqlDB.cs": "src/GameSrv/DB/MySqlDB.cs",
    "GameApp.cs": "src/GameSrv/GameApp.cs",
    "GameSvrConf.cs": "src/Modules/SystemModule/Conf/Model/GameSvrConf.cs",
    "Messages.cs": "src/OpenMir2/Messages.cs",
    "mir2_data.sql": "sql/mir2_data.sql",
}

# `File.cs:A[-B][/C[-D]...]`（.cs 与 .sql 两族）；与 ⑧/⑨ 三锚门同形。
FILE_RE = re.compile(r'([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.(?:cs|sql))')
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
_PATHREF_RE = re.compile(
    r'((?:[A-Za-z0-9_.-]+/)*)([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.(?:cs|sql))(?=\s*:)')

# spec 区间段（`:3966-4105/File.cs` 里的 `3966-4105`）：仅当该段紧跟在锚文本 `:` 之后时剔除。
_SPEC_RANGE_COMP_RE = re.compile(r'^\d+(?:-\d+)?$')


def _path_prefix_findings(anchor_text, pather=PATHS):
    findings = []
    for m in _PATHREF_RE.finditer(anchor_text):
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
#
# 订正记录（本片 = ⑩-3 reviewer-5 M-2「本轮未行级核对」的补做）：
#   * `MySqlDB.cs:251-291` ⇒ `251-288`（291 是函数后的空行，LoadSellOffItemList 实际
#     止于 288）。`MySqlDB.cs:293-330` ⇒ `295-337`（293 是注释、330 是 catch 的 `{`，
#     SaveSellOffItemList 实际 295..337）。
#   * `mir2_data.sql:24-36` ⇒ `24-33`（36 是 `-- Records of goldsales` 注释，
#     CREATE TABLE 实际止于 33 `) ENGINE = InnoDB ...`）。
#   * `Messages.cs:769-828` ⇒ `765-816`（769 起于 CM_SELLOFFADDITEM，已漏掉块首
#     RM_SENDDEALOFFFORM=23000；828 实为无关的 RM_PLAYERKILLMONSTER=60001，
#     寄售消息块实际 765..816 止于 RM_SELLOFFBUY_OK=23028）。
REGISTERED = [
    ('PlayObject.Message.cs:2046-2108', 'src/M2Server/Player/PlayObject.Message.cs', [
        (2046, 'case Messages.RM_SENDDEALOFFFORM:'),
        (2057, 'case Messages.CM_SELLOFFADDITEM:'),
        (2063, 'case Messages.CM_SELLOFFCANCEL:'),
        (2075, 'case Messages.CM_SELLOFFBUY:'),
        (2106, 'case Messages.RM_SELLOFFBUY_OK:'),
        (2108, 'SendSocket(ClientMsg, processMsg.Msg);')]),
    ('PlayObject.Sale.cs:60', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (60, 'private void ClientAddSellOffItem(int nItemIdx, string sItemName)')]),
    ('PlayObject.Sale.cs:127', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (127, 'private void ClientDelSellOffItem(int nItemIdx, string sItemName)')]),
    ('PlayObject.Sale.cs:516', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (516, 'private void ClientSellOffEnd(string sBuyChrName, int nSellGold, int nGameDiamond, int nCode)')]),
    ('PlayObject.Sale.cs:431', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (431, 'private void ClientCancelSellOff()')]),
    ('PlayObject.Sale.cs:593', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (593, 'private void SellOffCancel()')]),
    ('PlayObject.Sale.cs:185', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (185, 'private void ClientCancelSellOffIng()')]),
    ('PlayObject.Sale.cs:254', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (254, 'private void ClientBuyCancelSellOff(string dealChrName)')]),
    ('PlayObject.Sale.cs:281', 'src/M2Server/Player/PlayObject.Sale.cs', [
        (281, 'private void ClientBuySellOffItme(string dealChrName)')]),
    ('M2Share.cs:79', 'src/M2Server/M2Share.cs', [
        (79, 'public static IList<DealOffInfo> SellOffItemList = null;')]),
    ('PlayObject.cs:856', 'src/M2Server/Player/PlayObject.cs', [
        (856, 'public bool SaleDeal { get; set; }')]),
    ('PlayObject.cs:864', 'src/M2Server/Player/PlayObject.cs', [
        (864, 'public IList<UserItem> SellOffItemList { get; set; }')]),
    ('MySqlDB.cs:251-288', 'src/GameSrv/DB/MySqlDB.cs', [
        (251, 'public void LoadSellOffItemList()'),
        # 缺陷本体：oracle 读列名 "DealChrName"/"BuyChrName" 与 DDL 的 DealCharName/BuyCharName 不符
        # ⇒ 恒加载 0 行（异常上抛）。token 锁死缺陷，谁"修好"就 RED。
        (264, 'string sDealChrName = dr.GetString("DealChrName");'),
        (288, '}')]),
    ('GameApp.cs:181-182', 'src/GameSrv/GameApp.cs', [
        (181, 'LogService.Info("加载物品寄售系统...");'),
        (182, 'GameShare.DataSource.LoadSellOffItemList();')]),
    ('MySqlDB.cs:295-337', 'src/GameSrv/DB/MySqlDB.cs', [
        (295, 'public void SaveSellOffItemList()'),
        (337, '}')]),
    ('PlayObject.Sale.cs:240/332/577', 'src/M2Server/Player/PlayObject.Sale.cs', [
        # 三处调用点全部被注释 ⇒ oracle「寄售永不落库」。
        (240, 'M2Share.CommonDb.SaveSellOffItemList();'),
        (332, 'M2Share.CommonDb.SaveSellOffItemList();'),
        (577, 'M2Share.CommonDb.SaveSellOffItemList();')]),
    ('mir2_data.sql:24-33', 'sql/mir2_data.sql', [
        (24, 'CREATE TABLE `goldsales`'),
        (26, '`DealCharName` varchar(50)'),
        (31, '`UseItems` json NULL,'),
        (33, ') ENGINE = InnoDB AUTO_INCREMENT = 1')]),
    ('GameSvrConf.cs:1217/1919', 'src/Modules/SystemModule/Conf/Model/GameSvrConf.cs', [
        (1217, 'public int DecUserGameGold;'),
        (1919, 'DecUserGameGold = 10000;')]),
    ('Messages.cs:765-816', 'src/OpenMir2/Messages.cs', [
        (765, 'public const int RM_SENDDEALOFFFORM = 23000;'),
        (769, 'public const int CM_SELLOFFADDITEM = 23002;'),
        (816, 'public const int RM_SELLOFFBUY_OK = 23028;')]),
]


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _cached_src(src_root, rel):
    return _read_text(os.path.join(src_root, rel)).splitlines()


def parse_refs(anchor_text):
    """把一段锚文本解析成有序的 `file:spec` 引用列表（file 用基名）。"""
    out = []
    parts = FILE_RE.split(anchor_text)
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
    return findings


def cmd_check(args):
    findings = _evaluate()
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "anchors": len(REGISTERED),
                       "points": sum(len(p) for _, _, p in REGISTERED)},
                      fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: 元宝寄售场景锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(REGISTERED)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: 元宝寄售场景锚门通过（%d 锚 / %d 行点全部命中冻结面且覆盖面闭合）\n"
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

    good = [("PlayObject.Sale.cs:593", PATHS["PlayObject.Sale.cs"], [
        (593, "private void SellOffCancel()")])]
    bad = [("PlayObject.Sale.cs:593", PATHS["PlayObject.Sale.cs"], [
        (593, "private void SellOffCancelXYZNope()")])]
    stub_scen = "PlayObject.Sale.cs:593 note"
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
    oob = [("PlayObject.Sale.cs:593", PATHS["PlayObject.Sale.cs"], [(999999, "x")])]
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

    # 6) token 强度牙：整组只剩结束行 `}` ⇒ RED；O-1 纯标点整组 `]);`/`[]`/`*/`/`//{` 必须咬，
    #    含内容 token 的整组不误报。
    stub_weak = "MySqlDB.cs:251-288 note"
    weak_only = [("MySqlDB.cs:251-288", PATHS["MySqlDB.cs"], [(288, "}")])]
    w = _evaluate(rows=weak_only, scenario_text=stub_weak)
    t6a = len(w) == 1 and "token 强度" in w[0]
    o1_rows = [
        ("X.cs:1-2", PATHS["PlayObject.Sale.cs"], [(2, "]);")]),
        ("Y.cs:1-2", PATHS["PlayObject.Sale.cs"], [(2, "[]")]),
        ("Z.cs:1-2", PATHS["PlayObject.Sale.cs"], [(2, "*/")]),
        ("W.cs:1-2", PATHS["PlayObject.Sale.cs"], [(2, "//{")]),
        ("V.cs:1-2", PATHS["PlayObject.Sale.cs"], [(2, "//{"), (3, "foo")]),
    ]
    o1 = _token_strength_findings(o1_rows)
    t6b = len(o1) == 4 and all("token 强度" in f for f in o1)
    t6 = t6a and t6b
    sys.stdout.write("selftest token 强度牙齿: %s (weak=%r; pure-punct=%d/4)\n"
                     % ("GREEN" if t6 else "RED", w, len(o1)))
    ok = ok and t6

    sys.stdout.write("SELFTEST %s\n" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def cmd_mutation(args):
    out = args.out or tempfile.mkdtemp(prefix="so-anchor-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    real = scenario_anchor_text()

    # MUT1：把 Messages 锚退回订正前的 `769-828`（漏块首 RM_SENDDEALOFFFORM + 拖着无关的
    #   RM_PLAYERKILLMONSTER）⇒ 覆盖面（未登记 + 表过期）⇒ RED。
    mut1 = real.replace("Messages.cs:765-816", "Messages.cs:769-828")
    if mut1 == real:
        sys.stdout.write("BUG: 变异 MUT1 找不到 Messages.cs:765-816\n")
        ok = False
    f1 = _evaluate(scenario_text=mut1)
    red1 = any("未登记 Messages.cs:769-828" in f for f in f1)
    sys.stdout.write("MUT1 Messages 行漂移(:765-816→:769-828): %s（%d 项）\n"
                     % ("RED" if f1 else "GREEN", len(f1)))
    ok = ok and red1

    # MUT2：把 MySqlDB 读/写两锚退回订正前的 `251-291` / `293-330`（端点在空行/注释/`{` 上）
    #   ⇒ 覆盖面 RED。
    mut2 = (real.replace("MySqlDB.cs:251-288", "MySqlDB.cs:251-291")
                .replace("MySqlDB.cs:295-337", "MySqlDB.cs:293-330"))
    if mut2 == real:
        sys.stdout.write("BUG: 变异 MUT2 找不到 MySqlDB 订正后锚\n")
        ok = False
    f2 = _evaluate(scenario_text=mut2)
    red2 = (any("未登记 MySqlDB.cs:251-291" in f for f in f2)
            and any("未登记 MySqlDB.cs:293-330" in f for f in f2))
    sys.stdout.write("MUT2 MySqlDB 端点漂移: %s（%d 项）\n" % ("RED" if f2 else "GREEN", len(f2)))
    ok = ok and red2

    # MUT3：把表内 token 改坏（SellOffCancel → 不存在）⇒ 读侧 RED。
    bad_rows = []
    hit = False
    for anchor, rel, pts in REGISTERED:
        if anchor == "PlayObject.Sale.cs:593":
            hit = True
            pts = [(593, "private void SellOffCancelXYZNope()")]
        bad_rows.append((anchor, rel, pts))
    if not hit:
        sys.stdout.write("BUG: 变异 MUT3 找不到 PlayObject.Sale.cs:593\n")
        ok = False
    f3 = _evaluate(rows=bad_rows, scenario_text=real)
    red3 = any("读侧不匹配" in f for f in f3)
    sys.stdout.write("MUT3 读侧 token 漂移: %s（%d 项）\n" % ("RED" if f3 else "GREEN", len(f3)))
    ok = ok and red3

    # MUT4（回归）：真身 check 必须 GREEN。
    f0 = _evaluate(scenario_text=real)
    sys.stdout.write("MUT4 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
    ok = ok and not f0

    # MUT5（token 强度）：把 `MySqlDB.cs:251-288` 整组退化成只看结束行 `}`（写侧/读侧仍成立）
    #   ⇒ 强度牙 RED。
    weak5 = [("MySqlDB.cs:251-288", PATHS["MySqlDB.cs"], [(288, "}")])
             if a[0] == "MySqlDB.cs:251-288" else a for a in REGISTERED]
    f5 = _evaluate(rows=weak5, scenario_text=real)
    red5 = any("token 强度" in f for f in f5)
    sys.stdout.write("MUT5 token 强度(整组只剩 `}`): %s（%d 项）\n"
                     % ("RED" if red5 else "GREEN", len(f5)))
    ok = ok and red5

    # MUT6（缺陷本体锁死）：把 DDL 的 `DealCharName` 改回与读侧一致的 `DealChrName`
    #   （即「顺手修好 oracle 列名不符」）⇒ 读侧 token `\`DealCharName\` varchar(50)` 不匹配 ⇒ RED。
    #   用注入 src_root 的替身目录复现（不改工作树）。
    mut6_dir = os.path.join(out, "mut6")
    rel = PATHS["mir2_data.sql"]
    dst = os.path.join(mut6_dir, rel)
    os.makedirs(os.path.dirname(dst), exist_ok=True)
    with open(dst, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(_read_text(os.path.join(REPO_ROOT, rel)).replace(
            "`DealCharName` varchar(50)", "`DealChrName` varchar(50)"))
    f6 = _evaluate(rows=REGISTERED, scenario_text=real, src_root=mut6_dir)
    red6 = any("读侧不匹配 mir2_data.sql:24-33:26" in f for f in f6)
    sys.stdout.write("MUT6 缺陷本体(DDL 列名被'修好'): %s（%d 项）\n"
                     % ("RED" if red6 else "GREEN", len(f6)))
    ok = ok and red6

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
