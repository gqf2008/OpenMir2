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
    python -I per_strike_requests.py table \
        --csharp <csharp-attack-stream.tsv> --rust <rust-global-trace.tsv>
    python -I per_strike_requests.py jointable \
        --csharp-stream <csharp-attack-stream.tsv> --rust-stream <rust-global-trace.tsv> \
        [--csharp-seq <attack_seq.jsonl>] [--rust-seq <attack_seq.jsonl>] \
        ( --csharp-state <state.jsonl> --rust-state <state.jsonl> \
          | --csharp-server-state <SERVER_STATE.tsv> --rust-server-state <SERVER_STATE.tsv> \
            --csharp-attacker-id <id> --rust-attacker-id <id> --victim-x <x> --victim-y <y> \
          | --derive-state --csharp-raw <attack_raw.jsonl> --rust-raw <attack_raw.jsonl> )
    python -I per_strike_requests.py selftest

## jointable（owner 2026-10-12 23:50 第 ① 条）

把**每击的取数请求序列**（本工具切出的玩家单元）与夹具的**逐击状态记录**
（`attacker`/`victim` 的 `CurrX/CurrY/Dir/Ghost/Death/Hp`；m3bag 夹具写集）接在**同一张表**上
（长表：一行 = 一侧的一击，列两侧完全同名），并**机判两侧**：

* 状态记录必须覆盖 RNG 流水切出的**每一个**玩家单元（击数），`k` 不得有洞 ⇒ 否则 RED；
* 状态记录缺字段 / 缺键 / 不是 JSON ⇒ **PREREQ 点名**（`k` + 字段），**绝不**静默丢；
* `victim` 允许为 `null`（该击目标格为空，正是要看的「(18,20) 为什么空」）。

**夹具状态记录规范（唯一真源；m3bag 按此产出）**：每侧一个 JSONL，一行一击，
`{"role":"state","k":<int>,"attacker":{"x","y","dir","ghost","death","hp"},
"victim":{...}|null}`（6 字段齐全；攻方别名 `attacker|atk`、受害别名 `victim|vic`；
同文件里 `arm/attack/struck/done` 等其它 role 自动跳过）。

`--derive-state` 是**夹具状态记录就位前的离线预览**：从既有 `attack_raw.jsonl` 反推可得子集
（攻方 `x/y/dir` ← `role=attack`；受害 `present/hp` ← `role=struck`；其余字段写 `-`，
列内 `state=derived` 明示），**不替代**权威夹具记录。

退出码：`0` = 表已出且两侧覆盖/计数/无洞全过；`1` = RED（覆盖不全 / 计数不等）；
`2` = PREREQ（流水缺失 / 状态记录缺失或格式坏）。

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
# 夹具「逐击状态记录」（owner 2026-10-12 23:50 第 ① 条）
# ---------------------------------------------------------------------------
# 规范（唯一真源，见模块 docstring / README）：每侧一个 JSONL，一行一击：
#   {"role":"state","k":1,"attacker":{"x":17,"y":20,"dir":2,"ghost":0,"death":0,"hp":80},
#    "victim":{"x":18,"y":20,"dir":6,"ghost":0,"death":0,"hp":18}}   # victim 可 null
STATE_FIELDS = ("x", "y", "dir", "ghost", "death", "hp")
STATE_ROLES = {"state", "strike_state", "perhit", "perhit_state", ""}
ATK_KEYS = ("attacker", "atk")
VIC_KEYS = ("victim", "vic")


class StateError(Exception):
    """夹具逐击状态记录缺失 / 格式坏（PREREQ，点名 k + 字段）。"""


class MissingError(Exception):
    """夹具逐击状态记录未提供（PREREQ）。"""


def _pick(d: dict, keys):
    for k in keys:
        if k in d:
            return k, d[k]
    return None, None


def _require_actor(obj, name: str, lineno: int, k: int, who: str) -> dict:
    if not isinstance(obj, dict):
        raise StateError("%s:%d k=%d %s 不是对象" % (name, lineno, k, who))
    missing = [f for f in STATE_FIELDS if f not in obj]
    if missing:
        raise StateError("%s:%d k=%d %s 缺字段 %s" % (name, lineno, k, who, ",".join(missing)))
    return {f: int(obj[f]) for f in STATE_FIELDS}


def parse_state(path: Path) -> dict:
    """读夹具逐击状态记录 JSONL → {k: {"attacker": {...}, "victim": {...}|None}}。

    **fail-closed**：缺 `k` / 缺攻方或受害键 / actor 缺字段 / 非法 JSON ⇒ StateError（点名）。
    同文件里非状态 role（arm/attack/struck/done）跳过；文件里一条状态记录都没有 ⇒ StateError。
    """
    out: dict = {}
    for lineno, raw in enumerate(path.read_text(encoding="utf-8-sig", errors="replace").splitlines(), 1):
        line = raw.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError as exc:
            raise StateError("%s:%d 非法 JSON: %s" % (path.name, lineno, exc))
        if not isinstance(d, dict):
            raise StateError("%s:%d 不是 JSON 对象" % (path.name, lineno))
        if str(d.get("role", "")) not in STATE_ROLES:
            continue
        if "k" not in d:
            raise StateError("%s:%d 状态记录缺 `k`" % (path.name, lineno))
        k = int(d["k"])
        _akey, atk = _pick(d, ATK_KEYS)
        vkey, vic = _pick(d, VIC_KEYS)
        if atk is None:
            raise StateError("%s:%d k=%d 缺攻方键（attacker|atk）" % (path.name, lineno, k))
        if vkey is None:
            raise StateError("%s:%d k=%d 缺受害键（victim|vic；无目标写 null）" % (path.name, lineno, k))
        out[k] = {"attacker": _require_actor(atk, path.name, lineno, k, "attacker"),
                  "victim": None if vic is None else _require_actor(vic, path.name, lineno, k, "victim")}
    if not out:
        raise StateError("%s 无任何状态记录（role=state）" % path.name)
    return out


def derive_state_from_raw(path: Path) -> dict:
    """夹具状态记录就位前的**离线预览**：从 `attack_raw.jsonl` 反推可得子集。

    攻方 `x/y/dir` ← `role=attack`；受害 `present/hp` ← `role=struck`（归到最近一次 `k`）。
    其余字段（ghost/death/受害 x/y/dir、攻方 hp）不可得 ⇒ `None`（列内渲染为 `-`）。
    产出与 `parse_state` 同形（`attacker`/`victim`），并标 `state=derived`——**不替代**夹具记录。
    """
    out: dict = {}
    cur = None
    for raw in path.read_text(encoding="utf-8-sig", errors="replace").splitlines():
        line = raw.strip()
        if not line:
            continue
        if line.startswith("M3ATTSEQ "):
            line = line[len("M3ATTSEQ "):]
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        role = d.get("role")
        if role == "attack":
            cur = int(d["k"])
            rec = out.setdefault(cur, {"attacker": None, "victim": None})
            rec["attacker"] = {"x": d.get("x"), "y": d.get("y"), "dir": d.get("dir"),
                               "ghost": None, "death": None, "hp": None}
        elif role == "struck" and cur is not None:
            rec = out.setdefault(cur, {"attacker": None, "victim": None})
            rec["victim"] = {"x": None, "y": None, "dir": None,
                             "ghost": None, "death": None, "hp": d.get("hp")}
    if not out:
        raise MissingError("%s 无 role=attack/struck 行" % path.name)
    return out


# ---------------------------------------------------------------------------
# 服务端权威状态 dump（SERVER_STATE）——owner 2026-10-13 01:05 第 ② 条
# ---------------------------------------------------------------------------
# 两侧同形（C# `rng-hook/StartupHook.cs::DumpActors`、Rust `bins/game-svr/src/state_log.rs`）：
#   每行 `SERVER_STATE <seq> <json 数组>`，一个**玩家攻击单元**一行，数组元素 = 全部玩家族对象
#   `{id,name,x,y,dir,ghost,death,hp,maxhp,map}`。
# 为什么 jointable 要吃它：客户端夹具记录（`attack_state.jsonl`）**看不到 victim 的服务端 Ghost**
# （m3bag 已定位：受害被 ghost 后观察者仍保留 actor）⇒ 目标可用性只能由服务端 dump 咬住。
SERVER_STATE_PREFIX = "SERVER_STATE "


def parse_server_state(path: Path) -> list:
    """读 `SERVER_STATE <seq> <json 数组>` → [(seq, actors | None)]；坏 JSON ⇒ actors=None。

    非本格式行忽略；整文件一条都没有 ⇒ StateError（PREREQ，绝不静默出空表）。
    """
    rows = []
    for lineno, raw in enumerate(path.read_text(encoding="utf-8-sig", errors="replace").splitlines(), 1):
        line = raw.strip()
        if not line.startswith(SERVER_STATE_PREFIX):
            continue
        rest = line[len(SERVER_STATE_PREFIX):]
        sp = rest.find(" ")
        if sp < 0:
            rows.append((int(rest or 0), None))
            continue
        try:
            seq = int(rest[:sp])
        except ValueError:
            raise StateError("%s:%d SERVER_STATE seq 非法: %s" % (path.name, lineno, rest[:sp]))
        payload = rest[sp + 1:].strip()
        try:
            actors = json.loads(payload)
        except json.JSONDecodeError:
            actors = None
        rows.append((seq, actors))
    if not rows:
        raise StateError("%s 无任何 SERVER_STATE 行" % path.name)
    return rows


def _server_actor(obj: dict, name: str, k: int, who: str) -> dict:
    if not isinstance(obj, dict):
        raise StateError("%s: SERVER_STATE k=%d %s 不是对象" % (name, k, who))
    missing = [f for f in STATE_FIELDS if f not in obj]
    if missing:
        raise StateError("%s: SERVER_STATE k=%d %s 缺字段 %s" % (name, k, who, ",".join(missing)))
    return {f: int(obj[f]) for f in STATE_FIELDS}


def state_from_server_state(path: Path, attacker_id: int, vx: int, vy: int, nunits: int) -> dict:
    """服务端 dump → 与 `parse_state` 同形的 {k: {"attacker":…, "victim":…|None}}。

    * 取**最后 `nunits` 行**（与 `server_state_gate.py` 同口径：N = 本侧玩家攻击单元数）；
    * `attacker` = 该行里 `id == attacker_id` 的玩家族对象；找不到 ⇒ StateError（PREREQ 点名 k）；
    * `victim` = 该行里站在 `(vx,vy)` 的**另一个**玩家族对象（**不筛 ghost/death**——正是要被看见的那一列）；
    * 行数 < `nunits` / 行非法 JSON / 缺字段 ⇒ StateError。
    """
    rows = parse_server_state(path)
    if nunits and len(rows) < nunits:
        raise StateError("%s: SERVER_STATE 行数 %d < 玩家攻击单元数 %d（PREREQ）"
                         % (path.name, len(rows), nunits))
    seg = rows[-nunits:] if nunits else rows
    out: dict = {}
    for k, (seq, actors) in enumerate(seg, 1):
        if not isinstance(actors, list):
            raise StateError("%s: SERVER_STATE 第 %d 行（seq=%s）非法 JSON 数组" % (path.name, k, seq))
        atk = None
        vic = None
        for a in actors:
            if not isinstance(a, dict):
                continue
            if a.get("id") == attacker_id:
                atk = a
            elif vic is None and int(a.get("x", -1)) == vx and int(a.get("y", -1)) == vy:
                vic = a
        if atk is None:
            raise StateError("%s: SERVER_STATE 第 %d 行找不到 attacker id=%s（PREREQ）"
                             % (path.name, k, attacker_id))
        out[k] = {"attacker": _server_actor(atk, path.name, k, "attacker"),
                  "victim": None if vic is None else _server_actor(vic, path.name, k, "victim")}
    return out


def compare_client_server(fixture: dict, server: dict, side: str, problems: list) -> None:
    """夹具客户端记录 vs 服务端 dump 的 victim 一致性 → informational（两条都给了才有意义）。

    这是「客户端视图看不到 ghost」的可判据点：客户端 `victim` 在场且 `ghost=0`，而服务端同一 k
    `ghost=1` ⇒ 记 `info:client_vs_server_state_diff`（不判红；判红由夹具侧 `server_state_gate.py` 负责）。
    """
    diffs = []
    for k in sorted(set(fixture) & set(server)):
        fv = (fixture.get(k) or {}).get("victim")
        sv = (server.get(k) or {}).get("victim")
        if bool(fv) != bool(sv):
            diffs.append("%d:%s" % (k, "client_present_server_absent" if fv else "client_absent_server_present"))
        elif fv and sv:
            for f in ("ghost", "death"):
                if fv.get(f) is not None and int(fv[f]) != int(sv.get(f, 0)):
                    diffs.append("%d:%s(client=%s,server=%s)" % (k, f, fv[f], sv.get(f)))
    if diffs:
        problems.append("info:client_vs_server_state_diff side=%s ks=%s" % (side, ",".join(diffs[:20])))


def parse_attack_seq(path: Path) -> dict:
    """读 `attack_seq.jsonl` → {k: {"hit":bool,"dmg":int,...}}（summary 行忽略；缺文件 ⇒ {}）。"""
    out: dict = {}
    if not path or not Path(path).exists():
        return out
    for raw in Path(path).read_text(encoding="utf-8-sig", errors="replace").splitlines():
        line = raw.strip()
        if not line:
            continue
        try:
            d = json.loads(line)
        except json.JSONDecodeError:
            continue
        if isinstance(d, dict) and d.get("role") != "summary" and "k" in d:
            out[int(d["k"])] = d
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


# ---------------------------------------------------------------------------
# jointable：逐击请求序列 × 夹具逐击状态记录（两侧同表）
# ---------------------------------------------------------------------------
JOINT_COLS = (["side", "k", "state", "hit", "rng", "dmg", "reqs"]
              + ["atk_" + f for f in STATE_FIELDS]
              + ["vic"] + ["vic_" + f for f in STATE_FIELDS])


def _fmt(v):
    return "-" if v is None else str(v)


def _yn(v):
    if v is None:
        return "-"
    return "hit" if v else "dodge"


def _load_attack_units(side: str, stream: Path, smap):
    seq = parse_csharp(stream, smap) if side == "csharp" else parse_rust(stream, smap)
    return [u for _s, kind, u in segment_units(seq) if kind == "player"]


def _load_state_side(side, state_path, raw_path, derive, server_state="", attacker_id=None,
                     victim_x=None, victim_y=None, nunits=0):
    if server_state:
        if attacker_id is None or victim_x is None or victim_y is None:
            raise MissingError(
                "side=%s 提供 --%s-server-state 时必须同时给 --%s-attacker-id 与 --victim-x/--victim-y"
                "（服务端 dump 按 id + 格选 attacker/victim）" % (side, side, side))
        if not Path(server_state).exists():
            raise MissingError("side=%s 找不到服务端状态 dump %s" % (side, server_state))
        return state_from_server_state(Path(server_state), attacker_id, victim_x, victim_y, nunits), "server"
    if state_path:
        return parse_state(Path(state_path)), "fixture"
    if derive and raw_path:
        return derive_state_from_raw(Path(raw_path)), "derived"
    raise MissingError(
        "side=%s 缺夹具逐击状态记录（--%s-state 或 --%s-server-state）；"
        "或显式 --derive-state --%s-raw <attack_raw.jsonl> 用可得子集预览" % (side, side, side, side))


def _check_state_coverage(state, nunits, side, problems):
    ks = sorted(state)
    if ks and ks != list(range(1, ks[-1] + 1)):
        problems.append("state_k_gap side=%s ks=%s" % (side, ks))
    if len(ks) != nunits:
        problems.append("state_vs_stream_count_mismatch side=%s state=%d player_units=%d"
                        % (side, len(ks), nunits))


def jointable(csharp_stream, rust_stream, csharp_seq="", rust_seq="",
              csharp_state="", rust_state="", csharp_raw="", rust_raw="",
              derive=False, csharp_server_state="", rust_server_state="",
              csharp_attacker_id=None, rust_attacker_id=None,
              victim_x=None, victim_y=None):
    """产出两侧同表的逐击行 → (rc, text, problems)。rc：0=GREEN / 1=RED / 2=PREREQ。

    state 来源三选一（每侧独立）：
      * `--<side>-server-state`（**服务端权威** `SERVER_STATE`，两侧同形）⇒ `state=server`；
      * `--<side>-state`（夹具客户端 JSONL）⇒ `state=fixture`；
      * `--derive-state --<side>-raw`（预览）⇒ `state=derived`。
    两侧都给 server-state+state ⇒ 额外记 `info:client_vs_server_state_diff`（客户端看不到 ghost 的据点）。
    """
    smap = load_site_map()
    sides = (("csharp", csharp_stream, csharp_seq, csharp_state, csharp_raw, csharp_server_state, csharp_attacker_id),
             ("rust", rust_stream, rust_seq, rust_state, rust_raw, rust_server_state, rust_attacker_id))
    data: dict = {}
    problems: list = []
    try:
        for side, stream, seqp, statep, rawp, srvp, atkid in sides:
            if not stream or not Path(stream).exists():
                return 2, "", ["PREREQ_MISSING side=%s RNG 流水 %s" % (side, stream)]
            units = _load_attack_units(side, Path(stream), smap)
            state, src = _load_state_side(side, statep, rawp, derive, srvp, atkid,
                                          victim_x, victim_y, len(units))
            if srvp and statep:
                try:
                    compare_client_server(parse_state(Path(statep)), state, side, problems)
                except StateError:
                    pass  # 客户端记录本身坏 ⇒ 只比服务端
            _check_state_coverage(state, len(units), side, problems)
            data[side] = {"units": units, "state": state, "src": src,
                          "seq": parse_attack_seq(Path(seqp)) if seqp else {}}
    except (StateError, MissingError) as exc:
        return 2, "", [str(exc)]
    lines = ["\t".join(JOINT_COLS)]
    mismatch: dict = {"csharp": [], "rust": []}
    for side in ("csharp", "rust"):
        d = data[side]
        ks = sorted(set(list(d["state"]) + list(range(1, len(d["units"]) + 1))))
        for k in ks:
            u = d["units"][k - 1] if k - 1 < len(d["units"]) else None
            s = d["seq"].get(k, {})
            rec = d["state"].get(k) or {}
            atk = rec.get("attacker")
            vic = rec.get("victim")
            canons = [e.canon for e in u] if u else []
            rng_hit = None if not canons else ("hit_struck_damage" in canons)
            if s.get("hit") is not None and rng_hit is not None and bool(s["hit"]) != rng_hit:
                mismatch[side].append(k)
            row = [side, k, d["src"], _yn(s.get("hit")), _yn(rng_hit), _fmt(s.get("dmg")),
                   ",".join(canons) if canons else "-"]
            row += [_fmt(atk.get(f)) if atk else "-" for f in STATE_FIELDS]
            row.append("present" if vic else "absent")
            row += [_fmt(vic.get(f)) if vic else "-" for f in STATE_FIELDS]
            lines.append("\t".join(str(x) for x in row))
    for side in ("csharp", "rust"):
        if mismatch[side]:
            problems.append("info:align_vs_rng_hit_mismatch side=%s ks=%s" % (side, mismatch[side]))
    blocking = [p for p in problems if not p.startswith("info:")]
    return (1 if blocking else 0), "\n".join(lines), problems


def cmd_jointable(args) -> int:
    for p in (args.csharp_stream, args.rust_stream):
        if not p or not Path(p).exists():
            print("PREREQ_MISSING: 找不到 RNG 流水 %s" % p, file=sys.stderr)
            return 2
    rc, text, problems = jointable(args.csharp_stream, args.rust_stream,
                                   args.csharp_seq, args.rust_seq,
                                   args.csharp_state, args.rust_state,
                                   args.csharp_raw, args.rust_raw, args.derive_state,
                                   args.csharp_server_state, args.rust_server_state,
                                   args.csharp_attacker_id, args.rust_attacker_id,
                                   args.victim_x, args.victim_y)
    if rc == 2:
        print("PREREQ_MISSING: %s" % "; ".join(problems), file=sys.stderr)
        return 2
    print(text)
    if args.out:
        Path(args.out).write_text(text + "\n", encoding="utf-8", newline="\n")
        print("\n[out] %s" % args.out)
    blocking = [p for p in problems if not p.startswith("info:")]
    info = [p for p in problems if p.startswith("info:")]
    if blocking:
        print("\nJOINTABLE RED: %s" % "; ".join(blocking), file=sys.stderr)
        return 1
    for p in info:
        print("[info] %s" % p)
    if args.csharp_server_state or args.rust_server_state:
        src_tag = "server"
    elif args.csharp_state or args.rust_state:
        src_tag = "fixture"
    else:
        src_tag = "derived"
    print("\nJOINTABLE GREEN: 两侧逐击表已出（state 来源 = %s%s）" % (
        src_tag, "；%d 条 informational" % len(info) if info else ""))
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


def _mk_jsonl(path, rows):
    path.write_text("\n".join(json.dumps(r, ensure_ascii=False) for r in rows) + "\n",
                    encoding="utf-8", newline="\n")
    return path


def _mk_state(path, recs):
    return _mk_jsonl(path, [{"role": "state", "k": k, "attacker": a, "victim": v} for k, a, v in recs])


def _mk_raw(path, recs):
    rows = []
    for k, x, y, dr, hp in recs:
        rows.append({"role": "attack", "k": k, "op": "CM_HIT", "x": x, "y": y, "dir": dr, "my": 1, "id": "bot"})
        if hp is not None:
            rows.append({"role": "struck", "dmg": 1, "hp": hp, "maxhp": 19, "src": 1, "actor": 2, "my": 2})
    return _mk_jsonl(path, rows)


# 服务端 SERVER_STATE dump 夹具（与 server_state_gate.py 的 _mk_line 同形，含 id/name/map）
SRV_ATK = {"id": 1, "name": "a", "x": 17, "y": 20, "dir": 2, "ghost": 0, "death": 0, "hp": 80, "map": "0137"}
SRV_VIC = {"id": 2, "name": "v", "x": 18, "y": 20, "dir": 6, "ghost": 0, "death": 0, "hp": 18, "map": "0137"}


def _mk_server_state(path, vic_ghost_from=None, n=2):
    lines = []
    for k in range(1, n + 1):
        vic = dict(SRV_VIC)
        vic["ghost"] = 1 if (vic_ghost_from is not None and k >= vic_ghost_from) else 0
        lines.append("SERVER_STATE %d %s" % (k, json.dumps([dict(SRV_ATK), vic], ensure_ascii=False)))
    path.write_text("\n".join(lines) + "\n", encoding="utf-8", newline="\n")
    return path


ACT6 = {"x": 17, "y": 20, "dir": 2, "ghost": 0, "death": 0, "hp": 80}
VIC18 = {"x": 18, "y": 20, "dir": 6, "ghost": 0, "death": 0, "hp": 18}


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

    # jointable：夹具逐击状态记录（两侧同表）——owner 23:50 第 ① 条的权威路径
    st_cs = _mk_state(tmp / "state-cs.jsonl", [(1, dict(ACT6), None), (2, dict(ACT6), dict(VIC18))])
    st_rt = _mk_state(tmp / "state-rt.jsonl", [(1, dict(ACT6), dict(VIC18)), (2, dict(ACT6), None)])
    seq_cs = _mk_jsonl(tmp / "seq-cs.jsonl", [{"k": 1, "hit": False, "dmg": 0},
                                               {"k": 2, "hit": True, "dmg": 1}])
    seq_rt = _mk_jsonl(tmp / "seq-rt.jsonl", [{"k": 1, "hit": True, "dmg": 1},
                                               {"k": 2, "hit": False, "dmg": 0}])
    rc, text, probs = jointable(cs, rt, seq_cs, seq_rt, st_cs, st_rt)
    tl = text.splitlines()
    check("jointable-fixture-green", rc == 0, str(probs))
    check("jointable-both-sides-one-table",
          any(l.startswith("csharp\t") for l in tl) and any(l.startswith("rust\t") for l in tl)
          and "atk_hp" in text and "vic_x" in text)
    check("jointable-victim-present-absent", "present" in text and "absent" in text)

    # 缺状态记录 ⇒ PREREQ（rc=2），绝不静默出表
    rc, _t, probs = jointable(cs, rt)
    check("jointable-missing-state-prereq", rc == 2, str(probs))

    # 状态记录缺字段（victim 缺 ghost）⇒ PREREQ 点名
    bad = dict(VIC18)
    bad.pop("ghost")
    st_bad = _mk_state(tmp / "state-bad.jsonl", [(1, dict(ACT6), None), (2, dict(ACT6), bad)])
    rc, _t, probs = jointable(cs, rt, "", "", st_bad, st_rt)
    check("jointable-missing-field-prereq", rc == 2 and any("ghost" in p for p in probs), str(probs))

    # k 有洞（1,3）⇒ RED
    st_gap = _mk_state(tmp / "state-gap.jsonl", [(1, dict(ACT6), None), (3, dict(ACT6), None)])
    rc, _t, probs = jointable(cs, rt, "", "", st_gap, st_rt)
    check("jointable-k-gap-red", rc == 1 and any("state_k_gap" in p for p in probs), str(probs))

    # 状态记录数与 RNG 玩家单元数不等 ⇒ RED
    st_few = _mk_state(tmp / "state-few.jsonl", [(1, dict(ACT6), None)])
    rc, _t, probs = jointable(cs, rt, "", "", st_few, st_rt)
    check("jointable-count-mismatch-red", rc == 1 and any("count_mismatch" in p for p in probs), str(probs))

    # --derive-state：夹具记录就位前的预览，可从 attack_raw 反推（明示 derived）
    raw_cs = _mk_raw(tmp / "raw-cs.jsonl", [(1, 17, 20, 2, None), (2, 17, 20, 2, 18)])
    raw_rt = _mk_raw(tmp / "raw-rt.jsonl", [(1, 17, 20, 2, 18), (2, 17, 20, 2, None)])
    rc, text, probs = jointable(cs, rt, seq_cs, seq_rt, "", "", raw_cs, raw_rt, derive=True)
    check("jointable-derived-green", rc == 0 and "derived" in text, str(probs))

    # ---- owner 2026-10-13 01:05 第 ② 条：jointable 直接吃两侧 SERVER_STATE 服务端 dump ----
    ss_cs = _mk_server_state(tmp / "ss-cs.tsv", vic_ghost_from=None)
    ss_rt = _mk_server_state(tmp / "ss-rt.tsv", vic_ghost_from=None)
    rc, text, probs = jointable(cs, rt, seq_cs, seq_rt, "", "", "", "", derive=False,
                                csharp_server_state=ss_cs, rust_server_state=ss_rt,
                                csharp_attacker_id=1, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-server-state-green", rc == 0 and "server" in text, str(probs))
    check("jointable-server-state-both-sides",
          any(l.startswith("csharp\t") for l in text.splitlines())
          and any(l.startswith("rust\t") for l in text.splitlines())
          and "vic_ghost" in text and "atk_hp" in text)

    # 服务端 victim ghost=1（从 k=2 起）⇒ 不入列地丢为 present/absent，而是 vic_ghost 列出 1
    ss_ghost = _mk_server_state(tmp / "ss-ghost.tsv", vic_ghost_from=2)
    rc, text, probs = jointable(cs, rt, "", "", "", "", "", "", derive=False,
                                csharp_server_state=ss_ghost, rust_server_state=ss_rt,
                                csharp_attacker_id=1, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-server-state-ghost-column", rc == 0 and "\t1\t" in text and "present" in text, str(probs))

    # 客户端 fixture 全 ghost=0 vs 服务端 k=2 ghost=1 ⇒ info（不判红）：客户端看不到 ghost
    st_cs2 = _mk_state(tmp / "state-cs2.jsonl", [(1, dict(ACT6), dict(VIC18)), (2, dict(ACT6), dict(VIC18))])
    rc, text, probs = jointable(cs, rt, seq_cs, seq_rt, st_cs2, st_cs2, "", "", derive=False,
                                csharp_server_state=ss_ghost, rust_server_state=ss_rt,
                                csharp_attacker_id=1, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-client-vs-server-info",
          rc == 0 and any(p.startswith("info:client_vs_server_state_diff") and "2:ghost" in p for p in probs),
          str(probs))

    # 服务端 dump 行数 < 玩家单元数 ⇒ PREREQ（rc=2）
    ss_short = _mk_server_state(tmp / "ss-short.tsv", n=1)
    rc, _t, probs = jointable(cs, rt, "", "", "", "", "", "", derive=False,
                              csharp_server_state=ss_short, rust_server_state=ss_rt,
                              csharp_attacker_id=1, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-server-state-short-prereq", rc == 2 and any("行数" in p for p in probs), str(probs))

    # 找不到 attacker id ⇒ PREREQ（点名）
    rc, _t, probs = jointable(cs, rt, "", "", "", "", "", "", derive=False,
                              csharp_server_state=ss_cs, rust_server_state=ss_rt,
                              csharp_attacker_id=99, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-server-state-missing-attacker-prereq",
          rc == 2 and any("attacker id=99" in p for p in probs), str(probs))

    # 非法 JSON 行 ⇒ PREREQ
    (tmp / "ss-bad.tsv").write_text("SERVER_STATE 1 [oops\nSERVER_STATE 2 []\n", encoding="utf-8", newline="\n")
    rc, _t, probs = jointable(cs, rt, "", "", "", "", "", "", derive=False,
                              csharp_server_state=tmp / "ss-bad.tsv", rust_server_state=ss_rt,
                              csharp_attacker_id=1, rust_attacker_id=1, victim_x=18, victim_y=20)
    check("jointable-server-state-badjson-prereq", rc == 2 and any("非法 JSON" in p for p in probs), str(probs))

    # 给了 server-state 却没给选择参数 ⇒ PREREQ（fail-closed，不猜）
    rc, _t, probs = jointable(cs, rt, "", "", "", "", "", "", derive=False,
                              csharp_server_state=ss_cs, rust_server_state=ss_rt)
    check("jointable-server-state-needs-selector-prereq",
          rc == 2 and any("attacker-id" in p for p in probs), str(probs))

    for f in tmp.iterdir():
        f.unlink()
    tmp.rmdir()
    if failures:
        print("SELFTEST RED: %d/%d 未过: %s" % (len(failures), len(checks), ", ".join(failures)))
        return 1
    print("SELFTEST GREEN: per_strike_requests %d/%d（同形态绿 + base 单元分类 + 玩家路径多抽/次序/计数必红"
          " + jointable 夹具状态/缺字段/缺口/计数/derived/SERVER_STATE 两侧/ghost 列/客户端 vs 服务端/缺选择器必红）"
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
    j = sub.add_parser("jointable")
    j.add_argument("--csharp-stream", required=True)
    j.add_argument("--rust-stream", required=True)
    j.add_argument("--csharp-seq", default="")
    j.add_argument("--rust-seq", default="")
    j.add_argument("--csharp-state", default="")
    j.add_argument("--rust-state", default="")
    j.add_argument("--csharp-server-state", default="",
                   help="服务端权威 SERVER_STATE dump（C# StartupHook.DumpActors / Rust state_log.rs 同形）")
    j.add_argument("--rust-server-state", default="")
    j.add_argument("--csharp-attacker-id", type=int, default=None,
                   help="--csharp-server-state 时：攻方 ActorId（attack_raw.jsonl role=attack.my）")
    j.add_argument("--rust-attacker-id", type=int, default=None)
    j.add_argument("--victim-x", type=int, default=None, help="受害应在的格 X（= 攻方前方格；两侧共用一个 pin）")
    j.add_argument("--victim-y", type=int, default=None)
    j.add_argument("--csharp-raw", default="")
    j.add_argument("--rust-raw", default="")
    j.add_argument("--derive-state", action="store_true",
                   help="无权威夹具状态记录时，用 attack_raw.jsonl 反推可得子集（明示 state=derived）")
    j.add_argument("--out", default="")
    j.set_defaults(func=cmd_jointable)
    s = sub.add_parser("selftest")
    s.set_defaults(func=cmd_selftest)
    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
