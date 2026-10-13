#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""hero_pet_anchor_gate.py — M3⑨-5 英雄/宠物夹具 `hero_pet_scenario.json`
`oracle_anchors` 锚一致性门（fail-closed）。

卡：`openmir2-m3-hero-pet-fixture-botsrv`（⑨-5 夹具收口片：reviewer-5 M-1 锚行号、
reviewer-5 M-2 clone46 用例、reviewer-6 M-1 peer baseline-invalid 自检）。

## 为什么有这道门

⑨-5 的 reviewer-5 M-1 指出 `oracle_anchors.send_spell` 写
`src/BotSrv/Player/RobotPlayer.cs:3968 SendSpellMsg`，而该行在被审 head 上早已是无关代码
（同族：⑧-10 的 `guild_castle_anchor_gate.py`）。本片按冻结 `src/` 逐条机判复核，
发现 **`frames` 整条把消息号当成了行号**（旧稿 `Messages.cs:17/31/32/44/45/53/656` 实为
`SM_SPELL(17)…` 的**值**，真实行号是 `177/197/198/205/206/211/318/379`），另
`PlayObject.Base.cs:360-370` 的 `ProcessSlaveGhost` 起点也漂了一位。两条都在本片订正。

口径：
* **冻结 oracle**（`src/M2Server/**`、`src/OpenMir2/**`）⇒ 锚**行号**入表机判（可复现、不会漂）。
* **夹具工具侧**（`src/BotSrv/**`）⇒ 只锚**函数/字段名**、不给行号（BotSrv 是 T-41..T-46 的
  在途工具面，行号随夹具演进漂移；这类锚不构成 oracle 判据，故不入行号表）。

## 判据（任一不满足 ⇒ rc=1）

1. **覆盖面**：`oracle_anchors` 里解析出的每个 `File.cs:SPEC` 引用都必须在 `REGISTERED` 表内
   （新增/漏登记的锚 ⇒ RED，防「新锚不登记就假绿」）；反向亦然（表里有、场景里没有 ⇒ RED）。
2. **写侧**：`REGISTERED` 的 `file:spec` 必须能在场景锚文本里按同形解析出来（覆盖面已覆盖该点）。
3. **读侧**：每条锚声明的每个 `(line, token)` 必须命中**冻结** `src/` 的对应行
   （改行号 / 改文件 / 越界 ⇒ RED）。token 取自冻结行本体，锁住锚的位置语义。

## 用法

    python -I hero_pet_anchor_gate.py check [--json out]
    python -I hero_pet_anchor_gate.py selftest
    python -I hero_pet_anchor_gate.py mutation [--out DIR]

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
SCENARIO = os.path.join(HERE, "hero_pet_scenario.json")

# 锚里出现的 C# 文件基名 → 冻结 src/ 相对路径（基名多义（如 MonsterObject.cs）⇒ 必须点明，
# 不用「找第一个」的猜测口径）。夹具工具侧（src/BotSrv/**）不入表，见文件头口径。
PATHS = {
    "MagicManager.cs": "src/M2Server/Magic/MagicManager.cs",
    "BaseObject.cs": "src/M2Server/Actor/BaseObject.cs",
    "BaseObject.Base.cs": "src/M2Server/Actor/BaseObject.Base.cs",
    "MonsterObject.cs": "src/M2Server/Monster/MonsterObject.cs",
    "PlayObject.Base.cs": "src/M2Server/Player/PlayObject.Base.cs",
    "Messages.cs": "src/OpenMir2/Messages.cs",
}

# `File.cs:A[-B][/C[-D]...]`；与 guild_castle_anchor_gate.py / castle_oracle_anchor_gate.py 同形。
FILE_RE = re.compile(r'([A-Za-z0-9_]+(?:\.[A-Za-z0-9_]+)*\.cs)')
SPEC_RE = re.compile(r'/?\s*:(\d+(?:-\d+)?(?:/\d+(?:-\d+)?)*)')


# (anchor_text, src_relpath, [(line, expected_token), ...])
# 由本片逐条对冻结 src/ 复核给出；token 取冻结行本体。
REGISTERED = [
    ('MagicManager.cs:373/397/455/467/590', 'src/M2Server/Magic/MagicManager.cs', [(373, 'if (MagMakeSlave(playObject, userMagic))'), (397, 'if (MagTamming(playObject, (MonsterObject)targetObject, nTargetX, nTargetY, userMagic.Level))'), (455, 'if (MagMakeSinSuSlave(playObject, userMagic))'), (467, 'if (MagMakeAngelSlave(playObject, userMagic))'), (590, 'if (MagMakeClone(playObject, userMagic))')]),
    ('BaseObject.cs:1179-1209', 'src/M2Server/Actor/BaseObject.cs', [(1179, 'public IActor MakeSlave(string sMonName, int nMakeLevel, int nExpLevel, int nMaxMob, int dwRoyaltySec)'), (1209, '}')]),
    ('BaseObject.cs:1183', 'src/M2Server/Actor/BaseObject.cs', [(1183, 'SlaveList = new List<IMonsterActor>();')]),
    ('BaseObject.cs:1185', 'src/M2Server/Actor/BaseObject.cs', [(1185, 'if (SlaveList.Count < nMaxMob)')]),
    ('BaseObject.cs:1190', 'src/M2Server/Actor/BaseObject.cs', [(1190, 'MonsterObject monObj = (MonsterObject)SystemShare.WorldEngine.RegenMonsterByName(Envir.MapName, nX, nY, sMonName);')]),
    ('BaseObject.cs:1979-1994', 'src/M2Server/Actor/BaseObject.cs', [(1979, 'public bool CheckServerMakeSlave()'), (1994, '}')]),
    ('BaseObject.cs:1850-1876', 'src/M2Server/Actor/BaseObject.cs', [(1850, 'public void RecallSlave(string sSlaveName)'), (1876, '}')]),
    ('MagicManager.cs:704-825', 'src/M2Server/Magic/MagicManager.cs', [(704, 'private static bool MagTamming(PlayObject playObject, MonsterObject targetObject, int nTargetX, int nTargetY, byte magicLevel)'), (825, '}')]),
    ('MagicManager.cs:725', 'src/M2Server/Magic/MagicManager.cs', [(725, 'if (!targetObject.NoTame && targetObject.LifeAttrib != Grobal2.LA_UNDEAD && targetObject.Abil.Level < SystemShare.Config.MagTammingLevel && playObject.SlaveList.Count < SystemShare.Config.MagTammingCount)')]),
    ('MagicManager.cs:1308-1335', 'src/M2Server/Magic/MagicManager.cs', [(1308, 'private static bool MagMakeSlave(BaseObject playObject, UserMagic userMagic)'), (1335, '}')]),
    ('MagicManager.cs:1269-1306', 'src/M2Server/Magic/MagicManager.cs', [(1269, 'private static bool MagMakeSinSuSlave(PlayObject playObject, UserMagic userMagic)'), (1306, '}')]),
    ('MagicManager.cs:1343-1352', 'src/M2Server/Magic/MagicManager.cs', [(1343, 'private static bool MagMakeAngelSlave(BaseObject playObject, UserMagic userMagic)'), (1352, '}')]),
    ('MagicManager.cs:22', 'src/M2Server/Magic/MagicManager.cs', [(22, 'private const int DwRoyaltySec = 10 * 24 * 60 * 60;')]),
    ('MagicManager.cs:1337-1341', 'src/M2Server/Magic/MagicManager.cs', [(1337, 'private static bool MagMakeClone(PlayObject playObject, UserMagic userMagic)'), (1341, '}')]),
    ('MonsterObject.cs:71-84', 'src/M2Server/Monster/MonsterObject.cs', [(71, 'private void GainSlaveExp(byte nLevel)'), (84, '}')]),
    ('MonsterObject.cs:86-97', 'src/M2Server/Monster/MonsterObject.cs', [(86, 'private int GainSlaveUpKillCount()'), (97, 'return (Abil.Level * SystemShare.Config.MonUpLvRate) - Abil.Level + SystemShare.Config.MonUpLvNeedKillBase + tCount;')]),
    ('BaseObject.Base.cs:869-881', 'src/M2Server/Actor/BaseObject.Base.cs', [(869, 'protected virtual byte GetChrColor(IActor baseObject)'), (881, '}')]),
    ('MonsterObject.cs:363-397', 'src/M2Server/Monster/MonsterObject.cs', [(363, 'private void CheckRoyalty()'), (397, '}')]),
    ('MonsterObject.cs:352', 'src/M2Server/Monster/MonsterObject.cs', [(352, 'CheckRoyalty();')]),
    ('PlayObject.Base.cs:333-357', 'src/M2Server/Player/PlayObject.Base.cs', [(333, '// 宝宝在主人死亡后死亡处理'), (357, '}')]),
    ('PlayObject.Base.cs:361-371', 'src/M2Server/Player/PlayObject.Base.cs', [(361, 'private void ProcessSlaveGhost()'), (371, '}')]),
    ('BaseObject.Base.cs:208-217', 'src/M2Server/Actor/BaseObject.Base.cs', [(208, '// 清除宝宝列表中已经死亡及叛变的宝宝信息'), (217, '}')]),
    ('BaseObject.cs:1753-1768', 'src/M2Server/Actor/BaseObject.cs', [(1753, 'public void SetLastHiter(IActor baseObject)'), (1768, '}')]),
    ('BaseObject.Base.cs:119-132', 'src/M2Server/Actor/BaseObject.Base.cs', [(119, 'if (LastHiter != null)'), (132, '}')]),
    ('BaseObject.cs:2429-2443', 'src/M2Server/Actor/BaseObject.cs', [(2429, 'private void KillFunc()'), (2443, '}')]),
    ('BaseObject.cs:2434-2437', 'src/M2Server/Actor/BaseObject.cs', [(2434, 'if (ExpHitter.Master != null) //如果是角色下属杀死对象'), (2437, 'SendMsg(Messages.RM_DIEDROPITEM, ExpHitter.Master.ActorId, 0, 0, 0);')]),
    ('Messages.cs:177/197/198/205/206/211/318/379', 'src/OpenMir2/Messages.cs', [(177, 'public const int SM_SPELL = 17;'), (197, 'public const int SM_STRUCK = 31;'), (198, 'public const int SM_DEATH = 32;'), (205, 'public const int SM_WINEXP = 44;'), (206, 'public const int SM_LEVELUP = 45;'), (211, 'public const int SM_HEALTHSPELLCHANGED = 53;'), (318, 'public const int SM_CHANGENAMECOLOR = 656;'), (379, 'public const int SM_MENU_OK = 767;')]),
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
    return findings


def cmd_check(args):
    findings = _evaluate()
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "anchors": len(REGISTERED),
                       "points": sum(len(p) for _, _, p in REGISTERED)},
                      fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: hero/pet 场景锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(REGISTERED)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: hero/pet 场景锚门通过（%d 锚 / %d 行点全部命中冻结 src/ 且覆盖面闭合）\n"
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

    good = [("MagicManager.cs:1337-1341", PATHS["MagicManager.cs"], [
        (1337, "private static bool MagMakeClone(PlayObject playObject, UserMagic userMagic)"),
        (1341, "}")])]
    bad = [("MagicManager.cs:1337-1341", PATHS["MagicManager.cs"], [
        (1337, "private static bool MagMakeClone(PlayObject playObject, UserMagic userMagic)"),
        (1341, "ThisTokenDoesNotExistXYZ")])]
    stub_scen = "MagicManager.cs:1337-1341 note"
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
    oob = [("MagicManager.cs:1337-1341", PATHS["MagicManager.cs"], [(999999, "x")])]
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

    sys.stdout.write("SELFTEST %s\n" % ("GREEN" if ok else "RED"))
    return 0 if ok else 1


def cmd_mutation(args):
    out = args.out or tempfile.mkdtemp(prefix="hp-anchor-mut-")
    os.makedirs(out, exist_ok=True)
    ok = True
    real = scenario_anchor_text()

    # MUT1：把 ProcessSlaveGhost 的 361-371 改回历史错值 360-370 ⇒ 覆盖面（未登记）⇒ RED。
    mut1 = real.replace(":361-371 ProcessSlaveGhost", ":360-370 ProcessSlaveGhost")
    if mut1 == real:
        sys.stdout.write("BUG: 变异 MUT1 找不到 :361-371 ProcessSlaveGhost\n")
        ok = False
    f1 = _evaluate(scenario_text=mut1)
    red1 = any("未登记 PlayObject.Base.cs:360-370" in f for f in f1)
    sys.stdout.write("MUT1 行漂移(:361-371→:360-370): %s（%d 项）\n"
                     % ("RED" if f1 else "GREEN", len(f1)))
    ok = ok and red1

    # MUT2：把 frames 改回「消息号当行号」的旧稿 ⇒ 覆盖面（未登记）+ 表过期 ⇒ RED。
    mut2 = real.replace(
        "Messages.cs:177/197/198/205/206/211/318/379",
        "Messages.cs:17/31/32/44/45/53/656")
    if mut2 == real:
        sys.stdout.write("BUG: 变异 MUT2 找不到 Messages.cs:177/197/198/205/206/211/318/379\n")
        ok = False
    f2 = _evaluate(scenario_text=mut2)
    red2 = any("未登记 Messages.cs:17/31/32/44/45/53/656" in f for f in f2)
    sys.stdout.write("MUT2 消息号冒充行号: %s（%d 项）\n" % ("RED" if f2 else "GREEN", len(f2)))
    ok = ok and red2

    # MUT3：把表内 token 改坏（MakeSlave → MakeSlaveXYZ 不存在）⇒ 读侧 RED。
    bad_rows = []
    hit = False
    for anchor, rel, pts in REGISTERED:
        if anchor == "BaseObject.cs:1179-1209":
            hit = True
            pts = [(1179, "public IActor MakeSlaveXYZNope("), (1209, "}")]
        bad_rows.append((anchor, rel, pts))
    if not hit:
        sys.stdout.write("BUG: 变异 MUT3 找不到 BaseObject.cs:1179-1209\n")
        ok = False
    f3 = _evaluate(rows=bad_rows, scenario_text=real)
    red3 = any("读侧不匹配" in f for f in f3)
    sys.stdout.write("MUT3 读侧 token 漂移: %s（%d 项）\n" % ("RED" if f3 else "GREEN", len(f3)))
    ok = ok and red3

    # MUT4（回归）：真身 check 必须 GREEN。
    f0 = _evaluate(scenario_text=real)
    sys.stdout.write("MUT4 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
    ok = ok and not f0

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
