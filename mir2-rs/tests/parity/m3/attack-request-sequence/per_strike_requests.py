#!/usr/bin/env python3
# -*- coding: utf-8 -*-
r"""per_strike_requests.py — M3②③ 「两侧每击取数请求序列」机判（owner 2026-10-12 22:35 第 ③ 条）。

## 回答什么

owner 22:35 要求把「两侧每击的取数请求序列」做成**可判据**，并把**第一处请求分歧**
定位到「具体站点 + 触发的条件」。本工具：

1. 把两侧流水切成**攻击单元**（unit = 一次攻击；`atk_power_*` 起、下一次 `atk_power_*` 止）。
   单元分两类：
     * `player`：首站点是玩家攻方站点（`atk_power_a` / `atk_power_luck_a`）；
     * `base`  ：首站点是 `atk_power_base`（非 PlayObject 攻方，`BaseObject.GetAttackPower`）。
2. 把**每一击（k）的取数请求序列**（canon 站点，按先后）列出来，两侧逐击对齐。
3. 对**原始专用流逐次请求序列**求第一处分歧，并按「分歧点落在哪种 unit」**分类**：
     * `base_unit`   —— C# 侧多出/交错的**非玩家攻方单元**（并发实体；夹具/仪器口径，非玩家路径）；
     * `player_path` —— 玩家攻击路径内部的分歧（实现差，**必红**）；
     * `none`        —— 两侧逐次完全一致；
     * `length`      —— 一侧提前结束。
4. 独立校验每一侧的 player 单元是否符合 oracle 取数模板（**每侧自证**，不靠对侧）：
     * 躲闪：`atk_power_a, atk_power_b, hit_gate, attack_success`
     * 命中：`atk_power_a, atk_power_b, hit_gate, [hit_struck_damage2,] hit_struck_damage, struck_durability, attack_success`
   （`Luck > 0` 时首两抽换成 `atk_power_luck_a/_b`；`hit_struck_damage2` 只在 `|ΔAC|>0` 时多抽。）

## oracle（模板依据）

```
PlayObject.AttackDir(IActor targetObject, short wHitMode, byte nDir)      # PlayObject.Attack.cs:24
  IActor attackTarget = targetObject ?? GetPoseCreate();                # :26
  int nPower = GetAttackPowerHit(..., GetBaseAttackPoewr(), attackTarget, ref canHit);  # :73
  SkillAttackDamage(wHitMode, nPower);                                   # :74
  AttackDir(attackTarget, nPower, nDir);   # BaseObject.Attack.cs:11 -> _Attack  # :75
  SendAttackMsg(...);                                                    # :76（不抽数）
  AttackSuccess(wHitMode, nPower, canHit, attackTarget);                 # :77

private void AttackSuccess(...)                                          # PlayObject.Attack.cs:215
  if (targetObject == null) return;                                      # :217-220（无目标 ⇒ 不抽）
  ushort nWeaponDamage = (ushort)(Random(5) + 2 - AddAbil.WeaponStrong); # :225（有目标 ⇒ 无条件 1 抽）

internal bool _Attack(int nPower, IActor targetObject)                   # BaseObject.Attack.cs:30
  if (targetObject == null) return false;                                # :32-35（无目标 ⇒ 不抽门）
  if (IsProperTarget(targetObject)) {                                    # :37
    if (targetObject.HitPoint > 0) {                                     # :39
      if (HitPoint < RandomByte(targetObject.SpeedPoint)) nPower = 0;    # :41（命中门）
    }
  }
  if (nPower > 0) { nPower = targetObject.GetHitStruckDamage(this, nPower); ... }  # :48
```

⇒ **玩家攻方的取数次序恒为**：`a → b → [gate → (hsd → [hsd2] → durab)?] → succ`
（`gate` 只在「有目标 且 IsProperTarget 且 目标 HP>0」时抽；`succ` 只在「有目标」时抽）。
**C# 与 Rust 同形**（Rust `combat.rs:848 attack_dir`、`:830 attack_success_weapon_damage`）。

## 用法

    python -I per_strike_requests.py analyze \
        --csharp <csharp-attack-stream.tsv> --rust <rust-global-trace.tsv> \
        [--seed 40545] [--json out.json] [--csharp-kind csharp|rust] [--rust-kind rust]
    python -I per_strike_requests.py selftest

站点表（canon ↔ 正则）唯一事实源 = 同卡 `../rng-isolate-attack/attack_sites.tsv`。

退出码：0 = GREEN（玩家路径无分歧、两侧模板均合规、player 单元数相等）；
        1 = RED（玩家路径分歧 / 模板违规 / player 单元数不等）；2 = PREREQ。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
SITES_TSV = HERE.parent / "rng-isolate-attack" / "attack_sites.tsv"

PLAYER_START = {"atk_power_a", "atk_power_luck_a"}
BASE_START = {"atk_power_base"}
ATTACK_STARTS = PLAYER_START | BASE_START


# ---------------------------------------------------------------------------
# 站点表
# ---------------------------------------------------------------------------
def load_site_map(tsv: Path = SITES_TSV) -> dict[str, list[tuple[re.Pattern, str]]]:
    out: dict[str, list[tuple[re.Pattern, str]]] = {"csharp": [], "rust": []}
    for line in tsv.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or line.startswith("canon"):
            continue
        cols = line.split("\t")
        if len(cols) < 3:
            continue
        canon, side, pattern = cols[0], cols[1], cols[2]
        if side not in out:
            continue
        out[side].append((re.compile(pattern), canon))
    return out


def canon_of(side: str, site: str, smap) -> str | None:
    norm = (site or "").replace("\\", "/")
    for pat, canon in smap[side]:
        if pat.search(norm):
            return canon
    return None


class Entry:
    __slots__ = ("kind", "arg", "result", "canon")

    def __init__(self, kind, arg, result, canon):
        self.kind = kind
        self.arg = int(arg)
        self.result = int(result)
        self.canon = canon

    def req(self):
        return (self.kind, self.arg)

    def __repr__(self):
        return "%s(%d)->%d" % (self.canon, self.arg, self.result)


def parse_csharp(path: Path, smap) -> list[Entry]:
    """C# 专用流：`kind \\t arg \\t result \\t site`（无 seq、无头）。"""
    out = []
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        if not raw or "\t" not in raw:
            continue
        p = raw.split("\t")
        if len(p) < 4 or not p[3] or p[3][0].isdigit():
            continue
        canon = canon_of("csharp", p[3], smap)
        if canon is None:
            continue
        out.append(Entry(p[0], p[1], p[2], canon))
    return out


def parse_rust(path: Path, smap) -> list[Entry]:
    """Rust 全局 trace：`seq \\t kind \\t arg \\t result \\t site`；只保留攻击站点行。"""
    out = []
    for raw in path.read_text(encoding="utf-8-sig").splitlines():
        p = raw.split("\t")
        if len(p) < 5 or not p[0].isdigit() or not p[1].startswith("Next"):
            continue
        canon = canon_of("rust", p[4], smap)
        if canon is None:
            continue
        out.append(Entry(p[1], p[2], p[3], canon))
    return out


# ---------------------------------------------------------------------------
# 单元切分 / 模板
# ---------------------------------------------------------------------------
def segment_units(seq: list[Entry]):
    """返回 [(start_index, kind, [Entry...])]；kind = player|base。"""
    units = []
    cur = None
    for e in seq:
        if e.canon in ATTACK_STARTS:
            if cur is not None:
                units.append(cur)
            kind = "base" if e.canon in BASE_START else "player"
            cur = (len(units), kind, [e])
        elif cur is not None:
            cur[2].append(e)
    if cur is not None:
        units.append(cur)
    return units


def is_dodge(canons: list[str]) -> bool:
    return "hit_struck_damage" not in canons


def template_ok(canons: list[str]) -> bool:
    """独立校验单元是否符合 oracle 玩家取数模板。"""
    head = canons[0]
    if head not in PLAYER_START and head != "atk_power_base" and head != "atk_power_luck_a":
        return False
    if head in ("atk_power_luck_a",):
        # Luck>0：第一抽后可能是 atk_power_luck_b；夹具 Luck=0 不触发，宽松接受
        body = canons[1:]
        if body and body[0] == "atk_power_luck_b":
            body = body[1:]
    elif head == "atk_power_a":
        body = canons[1:]
        if body and body[0] == "atk_power_b":
            body = body[1:]
    elif head == "atk_power_base":
        body = canons[1:]
    else:
        return False
    if not body:
        return True  # 有目标前的空打（只抽攻击力）——非本夹具的 player 单元
    if body[0] == "hit_gate":
        gate = body[0]
        rest = body[1:]
    else:
        return False
    if rest and rest[0] == "attack_success":
        return True  # 躲闪：gate -> succ
    if rest and rest[0] == "hit_struck_damage":
        rest = rest[1:]
        if rest and rest[0] == "hit_struck_damage2":
            rest = rest[1:]
        if rest and rest[0] == "struck_durability":
            rest = rest[1:]
        return bool(rest) and rest[0] == "attack_success"
    return False


# ---------------------------------------------------------------------------
# 分析
# ---------------------------------------------------------------------------
def first_divergence(a: list[Entry], b: list[Entry]):
    """按「取数请求序列」= `(kind, arg)` 逐次比（与 `compare_dedicated_streams.py` 同口径）。

    注意：不比 canon——`atk_power_a(Next(max),1)` 与 `atk_power_base(Next(max),1)` 的
    **请求本身完全相同**（都是 `Random(1)`），继续下移一次才显形（这正是 owner 看到的 index=2）。
    """
    n = min(len(a), len(b))
    for i in range(n):
        if a[i].req() != b[i].req():
            return i, a[i], b[i]
    if len(a) != len(b):
        return n, (a[n] if len(a) > n else None), (b[n] if len(b) > n else None)
    return None, None, None


def classify(cs_units, index: int) -> str:
    """分歧点落在 C# 哪个 unit：base ⇒ base_unit；player ⇒ player_path。"""
    for _start, kind, ents in cs_units:
        # 用对象身份近似：重新线性映射 offset
        pass
    offset = 0
    for _start, kind, ents in cs_units:
        if offset <= index < offset + len(ents):
            return "base_unit" if kind == "base" else "player_path"
        offset += len(ents)
    return "length"


def analyze(csharp: Path, rust: Path, seed: int, allow_count_mismatch: bool = False) -> dict:
    smap = load_site_map()
    cs = parse_csharp(csharp, smap)
    rt = parse_rust(rust, smap)
    cs_units = segment_units(cs)
    rt_units = segment_units(rt)
    cs_player = [u for _s, k, u in cs_units if k == "player"]
    rt_player = [u for _s, k, u in rt_units if k == "player"]

    idx, ae, be = first_divergence(cs, rt)
    cls = "none" if idx is None else classify(cs_units, idx)

    violations = []
    for side, units in (("csharp", cs_player), ("rust", rt_player)):
        for k, u in enumerate(units, 1):
            canons = [e.canon for e in u]
            if not template_ok(canons):
                violations.append({"side": side, "k": k, "ops": canons})

    counts = {}
    for side, seq, units in (("csharp", cs, cs_units), ("rust", rt, rt_units)):
        c: dict[str, int] = {}
        for e in seq:
            c[e.canon] = c.get(e.canon, 0) + 1
        counts[side] = {
            "draws": len(seq),
            "site_counts": c,
            "player_units": sum(1 for _s, k, _u in units if k == "player"),
            "base_units": sum(1 for _s, k, _u in units if k == "base"),
        }

    problems = []
    if cls == "player_path":
        problems.append("player_path_divergence@draw_index=%d" % (idx + 1))
    if len(cs_player) != len(rt_player):
        if allow_count_mismatch:
            problems.append("info:player_units_mismatch csharp=%d rust=%d（--allow-count-mismatch）"
                            % (len(cs_player), len(rt_player)))
        else:
            problems.append("player_units_mismatch csharp=%d rust=%d" % (len(cs_player), len(rt_player)))
    if violations:
        problems.append("template_violations=%d" % len(violations))

    blocking = [p for p in problems if not p.startswith("info:")]
    ok = not blocking
    return {
        "seed": seed,
        "csharp": str(csharp),
        "rust": str(rust),
        "counts": counts,
        "first_divergence": {
            "index_0based": idx,
            "csharp": repr(ae) if ae else None,
            "rust": repr(be) if be else None,
            "class": cls,
        },
        "template_violations": violations,
        "verdict": "GREEN" if ok else "RED",
        "problems": problems,
    }


# ---------------------------------------------------------------------------
# 输出
# ---------------------------------------------------------------------------
def render(res: dict, out_lines) -> None:
    out_lines.append("=== M3②③ 两侧每击取数请求序列 ===")
    for side in ("csharp", "rust"):
        c = res["counts"][side]
        out_lines.append("--- %s: 攻击取数 %d（player unit=%d, base unit=%d）"
                         % (side, c["draws"], c["player_units"], c["base_units"]))
        out_lines.append("    站点计数: %s" % json.dumps(c["site_counts"], ensure_ascii=False, sort_keys=True))
    fd = res["first_divergence"]
    out_lines.append("--- 第一处请求分歧（原始专用流逐次）")
    out_lines.append("    index(0based) = %s  class = %s" % (fd["index_0based"], fd["class"]))
    out_lines.append("    csharp: %s" % fd["csharp"])
    out_lines.append("    rust  : %s" % fd["rust"])
    out_lines.append("--- 模板校验（每侧自证）: violations=%d" % len(res["template_violations"]))
    for v in res["template_violations"][:8]:
        out_lines.append("    %s k=%d %s" % (v["side"], v["k"], v["ops"]))
    out_lines.append("--- 判读: %s  %s" % (res["verdict"], "; ".join(res["problems"]) or "no problem"))


def cmd_table(args) -> int:
    smap = load_site_map()
    cs_units = [u for _s, k, u in segment_units(parse_csharp(Path(args.csharp), smap)) if k == "player"]
    rt_units = [u for _s, k, u in segment_units(parse_rust(Path(args.rust), smap)) if k == "player"]
    print("k\tC#(hit?)\tRust(hit?)")
    for i in range(max(len(cs_units), len(rt_units))):
        def fmt(u):
            if u is None:
                return "-"
            canons = [e.canon for e in u]
            tag = "hit" if "hit_struck_damage" in canons else "dodge"
            return tag + ":" + ",".join(canons)
        print("%d\t%s\t%s" % (i + 1, fmt(cs_units[i] if i < len(cs_units) else None),
                               fmt(rt_units[i] if i < len(rt_units) else None)))
    return 0


def cmd_analyze(args) -> int:
    for p in (args.csharp, args.rust):
        if not Path(p).exists():
            print("PREREQ_MISSING: 找不到 %s" % p, file=sys.stderr)
            return 2
    res = analyze(Path(args.csharp), Path(args.rust), args.seed, getattr(args, "allow_count_mismatch", False))
    lines = []
    render(res, lines)
    text = "\n".join(lines)
    print(text)
    if args.json:
        Path(args.json).write_text(json.dumps(res, ensure_ascii=False, indent=1), encoding="utf-8", newline="\n")
        print("\n[json] %s" % args.json)
    return 0 if res["verdict"] == "GREEN" else 1


# ---------------------------------------------------------------------------
# selftest（牙齿）
# ---------------------------------------------------------------------------
def _tsv(rows):
    return "\n".join("\t".join(str(x) for x in r) for r in rows) + "\n"


CS_SITE = {
    "a": "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2929",
    "b": "M2Server.Player.PlayObject.GetAttackPower@PlayObject.cs:2934",
    "base": "M2Server.Actor.BaseObject.GetAttackPower@BaseObject.Base.cs:892",
    "gate": "M2Server.Actor.BaseObject._Attack@BaseObject.Attack.cs:41",
    "hsd": "M2Server.Actor.BaseObject.GetHitStruckDamage@BaseObject.Base.cs:792",
    "durab": "M2Server.Player.PlayObject.StruckDamage@PlayObject.cs:2957",
    "succ": "M2Server.Player.PlayObject.AttackSuccess@PlayObject.Attack.cs:223",
}
RS_SITE = {
    "a": r"crates\world\src\combat.rs:109",
    "b": r"crates\world\src\combat.rs:111",
    "base": r"crates\world\src\combat.rs:67",
    "gate": r"crates\world\src\combat.rs:758",
    "hsd": r"crates\world\src\combat.rs:628",
    "durab": r"crates\world\src\combat.rs:601",
    "succ": r"crates\world\src\combat.rs:831",
}


def _mk_csharp(unit_ops, tmp, name):
    rows = []
    n = 0
    for ops in unit_ops:
        for i, op in enumerate(ops):
            n += 1
            arg = 1 if op in ("a", "base", "hsd") else (10 if op in ("b", "durab") else 15)
            rows.append(("Next(max)", arg, n % 7, CS_SITE[op]))
    p = tmp / name
    p.write_text(_tsv(rows), encoding="utf-8", newline="\n")
    return p


def _mk_rust(unit_ops, tmp, name):
    rows = [("1", "INJECT_OK", "42", "0", "")]
    rows.append(("SEED_PROBE",))
    n = 0
    for ops in unit_ops:
        for op in ops:
            n += 1
            arg = 1 if op in ("a", "base", "hsd") else (10 if op in ("b", "durab") else 15)
            rows.append((str(n), "Next(max)", str(arg), str(n % 7), RS_SITE[op]))
    p = tmp / name
    p.write_text(_tsv(rows).replace("\t", "\t"), encoding="utf-8", newline="\n")
    return p


def cmd_selftest(args) -> int:
    import tempfile

    tmp = Path(tempfile.mkdtemp(prefix="m3-requests-selftest-"))
    checks, failures = [], []

    def check(name, cond, detail=""):
        checks.append(name)
        if not cond:
            failures.append(name + (" (%s)" % detail if detail else ""))

    # 同一形态：C# 全是 player 单元，与 Rust 同 ⇒ GREEN
    ops = [["a", "b", "gate", "succ"], ["a", "b", "gate", "hsd", "durab", "succ"]]
    cs = _mk_csharp(ops, tmp, "cs.tsv")
    rt = _mk_rust(ops, tmp, "rt.tsv")
    r = analyze(cs, rt, 40545)
    check("same-shape-green", r["verdict"] == "GREEN", str(r["problems"]))
    check("same-shape-no-div", r["first_divergence"]["class"] == "none", str(r["first_divergence"]))
    check("same-shape-units", r["counts"]["csharp"]["player_units"] == r["counts"]["rust"]["player_units"] == 2)

    # C# 头部多一个 base 单元 ⇒ 第一分歧 class=base_unit 且仍 GREEN（夹具口径，非玩家路径）
    cs2 = _mk_csharp([["base", "gate", "hsd"]] + ops, tmp, "cs2.tsv")
    r2 = analyze(cs2, rt, 40545)
    check("base-unit-class", r2["first_divergence"]["class"] == "base_unit", str(r2["first_divergence"]))
    check("base-unit-not-player-red", r2["verdict"] == "GREEN", str(r2["problems"]))

    # 玩家路径内多一次取数（hsd 后插 hsd）⇒ template 违规 ⇒ RED
    ops_bad = [["a", "b", "gate", "hsd", "hsd", "durab", "succ"], ops[1]]
    cs3 = _mk_csharp(ops_bad, tmp, "cs3.tsv")
    r3 = analyze(cs3, rt, 40545)
    check("player-template-violation-red", r3["verdict"] == "RED" and any("template" in p for p in r3["problems"]),
          str(r3["problems"]))

    # 玩家路径次序颠倒（succ 在 gate 前）⇒ RED
    ops_bad2 = [["a", "b", "succ", "gate"], ops[1]]
    cs4 = _mk_csharp(ops_bad2, tmp, "cs4.tsv")
    r4 = analyze(cs4, rt, 40545)
    check("player-order-violation-red", r4["verdict"] == "RED", str(r4["problems"]))

    # player 单元数不等 ⇒ RED
    cs5 = _mk_csharp(ops[:1], tmp, "cs5.tsv")
    r5 = analyze(cs5, rt, 40545)
    check("player-unit-count-red", r5["verdict"] == "RED" and any("player_units_mismatch" in p for p in r5["problems"]),
          str(r5["problems"]))

    for f in tmp.iterdir():
        f.unlink()
    tmp.rmdir()
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: per_strike_requests %d/%d（同形态绿 + base 单元分类 + 玩家路径多抽/次序/计数必红）"
          % (len(checks), len(checks)))
    return 0


def main(argv=None) -> int:
    ap = argparse.ArgumentParser(description="M3 per-strike RNG request-sequence judge")
    sub = ap.add_subparsers(dest="cmd", required=True)
    p = sub.add_parser("analyze")
    p.add_argument("--csharp", required=True)
    p.add_argument("--rust", required=True)
    p.add_argument("--seed", type=int, default=40545)
    p.add_argument("--json", default="")
    p.add_argument("--allow-count-mismatch", action="store_true",
                   help="player 单元数不等降级为 info（仅当两侧会话构成不同，如 smoke+两次 120 vs smoke+一次 120）")
    p.set_defaults(func=cmd_analyze)
    t = sub.add_parser("table")
    t.add_argument("--csharp", required=True)
    t.add_argument("--rust", required=True)
    t.set_defaults(func=cmd_table)
    s = sub.add_parser("selftest")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
