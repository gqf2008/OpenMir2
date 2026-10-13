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

本门只覆盖**⑧-6 第②片新块**声明的锚；未登记的新锚由维护者在新增时补进 `ANCHORS`
（新增锚不登记不会假绿——本门不做「未登记即红」的全量扫描，避免把 ⑧-7/⑧-8 的
历史锚一并扫进来而漂移口径）。

## 用法

    python -I castle_oracle_anchor_gate.py check [--json out]
    python -I castle_oracle_anchor_gate.py selftest
    python -I castle_oracle_anchor_gate.py mutation [--out DIR]

退出码：0 = GREEN；1 = RED。
"""
import argparse
import json
import os
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
]


def _read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace") as fh:
        return fh.read()


def _cached_src(src_root, rel):
    path = os.path.join(src_root, rel)
    return _read_text(path).splitlines()


def _evaluate(anchors=None, castle_text=None, readme_text=None, src_root=None):
    """返回 findings 列表（空 = GREEN）。"""
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
    return findings


def cmd_check(args):
    findings = _evaluate()
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"findings": findings, "anchors": len(ANCHORS)},
                      fh, ensure_ascii=False, indent=2)
    if findings:
        sys.stdout.write("RED: castle oracle 锚门失败（%d 项 / 共 %d 锚）\n"
                         % (len(findings), len(ANCHORS)))
        for f in findings:
            sys.stdout.write("  - %s\n" % f)
        return 1
    sys.stdout.write("GREEN: castle oracle 锚门通过（%d 锚全部命中冻结 src/ 且写侧在位）\n"
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
    f0 = _evaluate(castle_text=castle_text, readme_text=readme_text)
    sys.stdout.write("MUT3 真身 check: %s（%d 项）\n" % ("GREEN" if not f0 else "RED", len(f0)))
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
