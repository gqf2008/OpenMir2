#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""compare_runs.py — 双栈无头对拍比较器（Rust 一次 vs C# oracle 一次）。

输入是**两条无头运行各自的产物目录**（与驱动器解耦：只吃文件/目录，不 import 驱动器的任何模块）：

    <DIR>/frames.ndjson     必需。每行一帧 s2c：
                            seq, t_ms, phase, conn, ident, recog, param, tag, series,
                            body_len, body_hex, body_sha256
    <DIR>/run.json          可选。argv / 起止墙钟 / 步骤与匹配结果；缺失只在报告里登记，不影响判定。
    <DIR>/db_snapshot.json  可选。库后态（见下「库快照」）。

输出：report.json（机器可读）+ 终端摘要 + 退出码。

判据
====
1) 帧对齐：**按序**对齐（不是按 seq 找键）。`action=ignore` 的消息号（默认心跳 10/41）在
   **两侧先各自剔除**再从剩余序列逐位对齐 —— 心跳帧数/时刻天然不稳，不能用它把后续帧顶偏。
   `ident` 永远参与比较（即使该帧其余字段被掩码）。
2) 头部字段（recog/param/tag/series，外加驱动器字段 phase/seq/t_ms/conn）：默认哪些比由规则决定。
   recog（=会话相关 ActorId）、t_ms（墙钟）、conn（驱动器会话标签）、seq（驱动器本地序号）默认掩码。
   每条掩码都带 reason/tier，并在报告 `whitelisted` / `coverage.global_masks` 里显式登记，绝不静默丢弃。
3) body：默认逐字节比较；差异报告 **ident + 帧序号 + 字节偏移** + 首次不同字节。
   * `mask_fields`：只掩头部字段，body 仍逐字节比。
   * `mask_body_ranges`：body 的指定 [start,end) 区间不参与判定（区间内若真不同，仍作为
     whitelisted 显式登记「此处不同但被掩码」）。
   * `ability_layout`：52 SM_ABILITY 的 40B(oracle)/58B(client) 布局对，按语义字段逐项比较。
     布局表**从 tools/loginfixture/capture_login_compare.py 导入**（ABILITY_LAYOUTS /
     ABILITY_FIELDS / decode_ability），本工具不抄第二份。语义不一致 ⇒ 差异；同长度同布局
     退回逐字节比；未知长度 ⇒ 无法判定（fail-closed）。
4) 规则覆盖：某 ident 在两侧任何一帧里出现、规则表却没有它的条目 ⇒ **UNCLASSIFIED ⇒ 退出码 2**。
   只有 `--allow-unclassified 1,2,3` 显式点名才放行，且报告里仍然逐个登记。
5) 库对账：两份快照走 tools/db-parity/dbsnap_pair.py 的同一套口径（直接 import 调 run_pair）。
   `db_snapshot.json` 支持三种形态（见 README）：
     a) `{"snapshot_dir": "<dbsnap 快照目录>"}`
     b) 裸 dbsnap manifest：`{"tables": [{"table":..., "file":..., ...}]}`（file 相对 json 所在目录）
     c) 内联：`{"format":"dbsnap-inline/1","tables":[{"table":...,"columns":[...],"rows":[[...]]}]}`
   结构问题（缺表/列不符/快照被改/未知表）⇒ 无法判定（2）；单元格/行差异 ⇒ 差异（1）。
   两侧都没有库快照 ⇒ 显式登记 `db.state=absent_both`，不当作差异；只有一侧有 ⇒ 无法判定（2）。

退出码：0 = 全等（PASS）/ 1 = 有差异（FAIL）/ 2 = 无法判定（UNDECIDABLE，含 UNCLASSIFIED）。
**两者同时出现时取 2**（未完成的判据优先，绝不拿「有差异」掩盖「没判完」）。

自检（全离线，不连网、不连库、不起进程）：
    python -I -B tools/conformance/compare_runs.py selftest

规则表：默认读本目录 rules.json（每条带 reason/tier/approved_by；初始全部 PENDING 复核），
`--rules FILE` 覆盖。
"""
import argparse
import hashlib
import importlib.util
import json
import os
import shutil
import sys
import tempfile

TOOL = "tools/conformance/compare_runs.py"
RULES_FORMAT = "conformance-rules/1"
DB_INLINE_FORMAT = "dbsnap-inline/1"

HEAD_FIELDS = ("recog", "param", "tag", "series")
STREAM_FIELDS = ("phase", "seq", "t_ms", "conn") + HEAD_FIELDS
ACTIONS = ("compare", "mask_fields", "mask_body_ranges", "ignore", "ability_layout")
TIERS = ("R", "L")

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
DEFAULT_RULES = os.path.join(HERE, "rules.json")
LOGIN_COMPARE = os.path.join(REPO_ROOT, "tools", "loginfixture", "capture_login_compare.py")
DBSNAP_PAIR = os.path.join(REPO_ROOT, "tools", "db-parity", "dbsnap_pair.py")

# dbsnap_pair 的差异记录里，哪些 kind 属于「结构/输入问题（无法判定）」而不是「两侧取值不同」。
DB_UNDECIDABLE_KINDS = frozenset({
    "unknown_table", "missing_table", "schema", "tamper", "row_count",
    "column_count", "duplicate_key", "input_error",
})


class InputError(Exception):
    """用法/规则/输入格式错误 ⇒ 退出码 2。"""


class Undecidable(Exception):
    """判据无法完成（缺产物/无法解码/未覆盖）⇒ 退出码 2。"""


# ---------------------------------------------------------------------------
# 小工具
# ---------------------------------------------------------------------------
def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(1 << 20), b""):
            h.update(chunk)
    return h.hexdigest()


def is_int(v):
    return isinstance(v, int) and not isinstance(v, bool)


def read_json(path, what):
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except OSError as e:
        raise InputError("读不了%s %s: %s" % (what, path, e))
    except ValueError as e:
        raise InputError("%s 不是合法 JSON：%s (%s)" % (what, path, e))


_module_cache = {}


def load_module(name, path):
    """按路径加载另一个工具模块（复用其实现，不抄第二份）。"""
    if name in _module_cache:
        return _module_cache[name]
    if not os.path.exists(path):
        raise Undecidable("依赖模块缺失：%s（本工具不复刻它的表，缺了就判不了）" % path)
    spec = importlib.util.spec_from_file_location("_conformance_" + name, path)
    if spec is None or spec.loader is None:
        raise Undecidable("无法加载依赖模块：%s" % path)
    mod = importlib.util.module_from_spec(spec)
    try:
        spec.loader.exec_module(mod)
    except Exception as e:  # noqa: BLE001 - 任何加载失败都必须 fail-closed
        raise Undecidable("依赖模块加载失败：%s (%s: %s)" % (path, type(e).__name__, e))
    _module_cache[name] = mod
    return mod


# ---------------------------------------------------------------------------
# 规则表
# ---------------------------------------------------------------------------
class RuleSet(object):
    def __init__(self, data, source, sha256):
        if not isinstance(data, dict):
            raise InputError("规则表顶层必须是对象：%s" % source)
        if data.get("format") != RULES_FORMAT:
            raise InputError("规则表 format 必须是 %r（实际 %r）：%s"
                             % (RULES_FORMAT, data.get("format"), source))
        self.source = source
        self.sha256 = sha256
        self.global_masks = {}      # field -> {reason,tier,approved_by,origin}
        self.by_ident = {}          # ident -> rule dict（已展开、已校验）
        self.db = {}
        self.pending_review = []

        self._parse_global(data.get("global") or [])
        self._parse_rules(data.get("rules") or [])
        self._parse_db(data.get("db") or {})
        self.pending_review = sorted(set(self.pending_review))

    # -- 解析/校验 --
    @staticmethod
    def _check_meta(rule, where):
        for key in ("reason", "tier", "approved_by"):
            v = rule.get(key)
            if not isinstance(v, str) or not v.strip():
                raise InputError("%s 缺 %s（每条规则都必须写清理由/等级/批准人）" % (where, key))
        if rule["tier"] not in TIERS:
            raise InputError("%s 的 tier 必须是 %s（实际 %r）"
                             % (where, "/".join(TIERS), rule["tier"]))

    @staticmethod
    def _fields_list(value, allowed, where):
        if isinstance(value, str):
            value = [value]
        if not isinstance(value, list) or not value:
            raise InputError("%s 的 fields 必须是非空数组" % where)
        out = []
        for f in value:
            if f not in allowed:
                raise InputError("%s 的字段 %r 不在允许集合 %s" % (where, f, list(allowed)))
            if f not in out:
                out.append(f)
        return out

    def _parse_global(self, entries):
        if not isinstance(entries, list):
            raise InputError("规则表 global 必须是数组")
        for i, rule in enumerate(entries):
            where = "global[%d]" % i
            if not isinstance(rule, dict):
                raise InputError("%s 必须是对象" % where)
            self._check_meta(rule, where)
            action = rule.get("action", "mask_fields")
            if action != "mask_fields":
                raise InputError("%s 只允许 action=mask_fields（全局规则只做字段掩码）" % where)
            fields = self._fields_list(rule.get("fields"), STREAM_FIELDS, where)
            if rule.get("ident") not in ("*", None):
                raise InputError("%s 带 ident 时应写进 rules[]，不要放 global" % where)
            if str(rule["approved_by"]).upper().startswith("PENDING"):
                self.pending_review.append("%s:%s" % (where, ",".join(fields)))
            for f in fields:
                if f in self.global_masks:
                    raise InputError("全局掩码重复声明字段 %r" % f)
                self.global_masks[f] = {"reason": rule["reason"], "tier": rule["tier"],
                                        "approved_by": rule["approved_by"],
                                        "origin": where}

    def _parse_rules(self, entries):
        if not isinstance(entries, list):
            raise InputError("规则表 rules 必须是数组")
        for i, rule in enumerate(entries):
            where = "rules[%d]" % i
            if not isinstance(rule, dict):
                raise InputError("%s 必须是对象" % where)
            self._check_meta(rule, where)
            action = rule.get("action")
            if action not in ACTIONS:
                raise InputError("%s 的 action 必须是 %s（实际 %r）"
                                 % (where, "/".join(ACTIONS), action))
            idents = rule.get("ident")
            if isinstance(idents, int) and not isinstance(idents, bool):
                idents = [idents]
            if not isinstance(idents, list) or not idents:
                raise InputError("%s 的 ident 必须是整数或非空整数数组" % where)
            if any((not is_int(x)) or x < 0 or x > 65535 for x in idents):
                raise InputError("%s 的 ident 必须是 0..65535 的整数（u16）" % where)
            expanded = dict(rule)
            if action == "mask_fields":
                expanded["fields"] = self._fields_list(rule.get("fields"), HEAD_FIELDS, where)
            elif action == "mask_body_ranges":
                ranges = rule.get("ranges")
                if not isinstance(ranges, list) or not ranges:
                    raise InputError("%s（mask_body_ranges）必须给非空 ranges" % where)
                norm = []
                for r in ranges:
                    if (not isinstance(r, list)) or len(r) != 2:
                        raise InputError("%s 的 range 必须是 [start,end]（end 可为 null）" % where)
                    start, end = r
                    if not is_int(start) or start < 0:
                        raise InputError("%s 的 range start 必须是非负整数" % where)
                    if end is not None and (not is_int(end) or end <= start):
                        raise InputError("%s 的 range end 必须是 > start 的整数或 null" % where)
                    norm.append([start, end])
                expanded["ranges"] = norm
            if "head_mask" in rule:
                expanded["head_mask"] = self._fields_list(rule["head_mask"], HEAD_FIELDS, where)
            if "head_compare" in rule:
                expanded["head_compare"] = self._fields_list(rule["head_compare"], HEAD_FIELDS, where)
            if str(rule["approved_by"]).upper().startswith("PENDING"):
                self.pending_review.append("%s:ident=%s" % (where, idents))
            for ident in idents:
                if ident in self.by_ident:
                    raise InputError("ident %d 在规则表里出现两次（歧义 ⇒ 拒绝）" % ident)
                self.by_ident[ident] = expanded

    def _parse_db(self, db):
        if not isinstance(db, dict):
            raise InputError("规则表 db 必须是对象")
        out = {"time_mode": "exact", "key_mode": "pk", "ignore_columns": [],
               "allow_unknown_table": False}
        if "time_mode" in db:
            if db["time_mode"] not in ("exact", "presence", "normalized"):
                raise InputError("db.time_mode 必须是 exact/presence/normalized")
            out["time_mode"] = db["time_mode"]
        if "key_mode" in db:
            if db["key_mode"] not in ("pk", "multiset"):
                raise InputError("db.key_mode 必须是 pk/multiset")
            out["key_mode"] = db["key_mode"]
        if "ignore_columns" in db:
            cols = db["ignore_columns"]
            if not isinstance(cols, list) or any(not isinstance(c, str) for c in cols):
                raise InputError("db.ignore_columns 必须是字符串数组")
            out["ignore_columns"] = [c for c in cols if c]
        if "allow_unknown_table" in db:
            if not isinstance(db["allow_unknown_table"], bool):
                raise InputError("db.allow_unknown_table 必须是布尔")
            out["allow_unknown_table"] = db["allow_unknown_table"]
        for key in ("reason", "tier", "approved_by"):
            if key in db and not isinstance(db[key], str):
                raise InputError("db.%s 必须是字符串" % key)
        if db.get("tier") and db["tier"] not in TIERS:
            raise InputError("db.tier 必须是 R/L")
        if "tier" in db and "reason" not in db:
            raise InputError("db 给了 tier 就必须给 reason")
        if str(db.get("approved_by", "")).upper().startswith("PENDING"):
            self.pending_review.append("db")
        out["meta"] = {k: db[k] for k in ("reason", "tier", "approved_by") if k in db}
        self.db = out

    # -- 查询 --
    def mask_for(self, ident):
        """返回该 ident 的头部掩码 {field: {reason,tier,approved_by,origin}}。"""
        masks = dict(self.global_masks)
        rule = self.by_ident.get(ident)
        if rule:
            head_mask = list(rule.get("head_mask", []))
            if rule.get("action") == "mask_fields":
                head_mask.extend(rule.get("fields", []))
            for f in head_mask:
                masks[f] = {"reason": rule.get("reason", ""), "tier": rule.get("tier", ""),
                            "approved_by": rule.get("approved_by", ""),
                            "origin": "ident=%d head_mask" % ident}
            for f in rule.get("head_compare", []):
                masks.pop(f, None)
        return masks


def load_rules(path):
    if not path:
        raise InputError("没有规则表路径")
    if not os.path.exists(path):
        raise InputError("规则表不存在：%s（默认应为 tools/conformance/rules.json）" % path)
    data = read_json(path, "规则表")
    return RuleSet(data, os.path.abspath(path), sha256_file(path))


# ---------------------------------------------------------------------------
# 帧流输入
# ---------------------------------------------------------------------------
class Stream(object):
    def __init__(self, label, dirpath, path, frames, problems):
        self.label = label
        self.dirpath = dirpath
        self.path = path
        self.frames = frames
        self.problems = problems


def _parse_body(rec, where):
    body_hex = rec.get("body_hex")
    if body_hex is None:
        return None
    if not isinstance(body_hex, str):
        raise Undecidable("%s 的 body_hex 不是字符串" % where)
    s = body_hex.strip()
    if s[:2].lower() == "0x":
        s = s[2:]
    try:
        return bytes.fromhex(s)
    except ValueError:
        raise Undecidable("%s 的 body_hex 不是合法十六进制" % where)


def load_stream(dirpath, label):
    if not os.path.isdir(dirpath):
        raise Undecidable("--%s 目录不存在：%s" % (label, dirpath))
    path = os.path.join(dirpath, "frames.ndjson")
    if not os.path.exists(path):
        raise Undecidable("%s 侧缺 frames.ndjson：%s" % (label, path))
    frames = []
    problems = []
    with open(path, "rb") as f:
        for lineno, raw in enumerate(f, 1):
            line = raw.strip()
            if not line:
                continue
            where = "%s frames.ndjson 第 %d 行" % (label, lineno)
            try:
                rec = json.loads(line.decode("utf-8"))
            except (UnicodeDecodeError, ValueError):
                raise Undecidable("%s 不是合法 UTF-8 JSON" % where)
            if not isinstance(rec, dict):
                raise Undecidable("%s 不是 JSON 对象" % where)
            ident = rec.get("ident")
            if not is_int(ident) or ident < 0 or ident > 65535:
                raise Undecidable("%s 缺合法 ident（u16）" % where)
            body = _parse_body(rec, where)
            declared_len = rec.get("body_len")
            if declared_len is not None:
                if not is_int(declared_len) or declared_len < 0:
                    raise Undecidable("%s 的 body_len 必须是非负整数" % where)
                if body is not None and declared_len != len(body):
                    problems.append({"kind": "input_inconsistent", "side": label,
                                     "frame_index": len(frames), "ident": ident,
                                     "field": "body_len", "declared": declared_len,
                                     "actual": len(body),
                                     "detail": "body_len 与 body_hex 实际长度不符（驱动器产物自相矛盾）"})
            declared_sha = rec.get("body_sha256")
            if declared_sha is not None:
                if not isinstance(declared_sha, str) or not declared_sha.strip():
                    raise Undecidable("%s 的 body_sha256 必须是非空字符串" % where)
                declared_sha = declared_sha.strip().lower()
                if body is not None and sha256_bytes(body) != declared_sha:
                    problems.append({"kind": "input_inconsistent", "side": label,
                                     "frame_index": len(frames), "ident": ident,
                                     "field": "body_sha256", "declared": declared_sha,
                                     "actual": sha256_bytes(body),
                                     "detail": "body_sha256 与 body_hex 不符（驱动器产物自相矛盾）"})
            fr = {"index": len(frames), "line": lineno, "ident": ident,
                  "body": body, "body_sha256": declared_sha, "raw": rec}
            for field in STREAM_FIELDS:
                fr[field] = rec.get(field)
            frames.append(fr)
    return Stream(label, os.path.abspath(dirpath), os.path.abspath(path), frames, problems)


def load_run_meta(dirpath):
    path = os.path.join(dirpath, "run.json")
    if not os.path.exists(path):
        return {"present": False, "path": None,
                "note": "run.json 缺失：只比帧流，不核对步骤/墙钟（已显式登记）"}
    data = read_json(path, "run.json")
    if not isinstance(data, dict):
        raise InputError("run.json 顶层必须是对象：%s" % path)
    out = dict(data)
    out["present"] = True
    out["path"] = os.path.abspath(path)
    missing = [k for k in ("argv", "started_at", "ended_at") if k not in data]
    if missing:
        out["missing_fields"] = missing
    return out


# ---------------------------------------------------------------------------
# 帧比较
# ---------------------------------------------------------------------------
def first_diff_offset(a, b):
    n = min(len(a), len(b))
    for i in range(n):
        if a[i] != b[i]:
            return i
    return n


def count_diff_bytes(a, b):
    return sum(1 for x, y in zip(a, b) if x != y) + abs(len(a) - len(b))


def _compare_ability(idx, ident, ob, rb, rule, diffs, whitelisted, undecidable):
    """52 的 40/58 布局对：按语义字段比。返回 True 表示已处理。"""
    try:
        mod = load_module("capture_login_compare", LOGIN_COMPARE)
        layouts = mod.ABILITY_LAYOUTS
        fields = mod.ABILITY_FIELDS
        decode = mod.decode_ability
    except Undecidable as e:
        undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "kind": "ability_prereq", "detail": str(e)})
        return True
    pair = (len(ob), len(rb))
    if pair in ((40, 58), (58, 40)):
        od = decode(ob)
        rd = decode(rb)
        if od is None or rd is None:
            undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "kind": "ability_decode",
                                "detail": "40/58 布局对解不出语义字段（body 被截断？）"})
            return True
        mism = []
        for name in fields:
            if od[name] != rd[name]:
                mism.append({"field": name, "oracle": od[name], "rust": rd[name],
                             "oracle_offset": layouts[len(ob)][name][1],
                             "rust_offset": layouts[len(rb)][name][1]})
        if mism:
            for m in mism:
                diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                              "field": "52_semantic:%s" % m["field"], "kind": "ability_semantic",
                              "oracle": m["oracle"], "rust": m["rust"],
                              "oracle_offset": m["oracle_offset"],
                              "rust_offset": m["rust_offset"],
                              "detail": "52 语义字段 %s 不同（oracle %sB / rust %sB 布局）"
                                        % (m["field"], len(ob), len(rb))})
        else:
            whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "field": "52_layout", "kind": "ability_layout",
                                "oracle_len": len(ob), "rust_len": len(rb),
                                "reason": "oracle40/client58 布局差；18 个语义字段逐项相等",
                                "tier": rule.get("tier", ""),
                                "approved_by": rule.get("approved_by", "")})
        return True
    if len(ob) == len(rb) and len(ob) in layouts:
        return False  # 同布局 ⇒ 逐字节比（更严，语义相等必然字节相等）
    undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                        "kind": "ability_layout_unknown",
                        "detail": "52 的 body 长度组合 %s 不在已知布局表 %s 里"
                                  % (pair, sorted(layouts))})
    return True


def compare_bodies(fr_o, fr_r, rule, diffs, whitelisted, undecidable, masked_ranges_note):
    """按 action 比 body；所有分支都必须给出明确结论。"""
    ident = fr_o["ident"]
    idx = fr_o["index"]
    ob, osha = fr_o["body"], fr_o["body_sha256"]
    rb, rsha = fr_r["body"], fr_r["body_sha256"]
    action = (rule or {}).get("action", "compare")

    if ob is None or rb is None:
        if ob is None and rb is None:
            if osha and rsha:
                if osha != rsha:
                    diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                                  "field": "body_sha256", "kind": "body_sha256",
                                  "oracle": osha, "rust": rsha,
                                  "detail": "两侧都只有 body_sha256：哈希不同（无法定位字节偏移）"})
                else:
                    whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                        "field": "body_sha256", "kind": "body_partial",
                                        "reason": "两侧都没有 body_hex，只比了 body_sha256",
                                        "tier": (rule or {}).get("tier", "R"),
                                        "approved_by": (rule or {}).get("approved_by", "")})
                return
            undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "kind": "body_unavailable",
                                "detail": "两侧都没有 body_hex / body_sha256，body 无法比较"})
            return
        # 只有一侧有 body：按 sha 互验（缺失侧若没 sha 则无法判定）
        if not (osha and rsha):
            undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "kind": "body_partial",
                                "detail": "只有一侧给了 body_hex，且另一侧没有 body_sha256"})
            return
        present = ob if ob is not None else rb
        if sha256_bytes(present) != osha or sha256_bytes(present) != rsha:
            diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                          "field": "body_sha256", "kind": "body_sha256",
                          "oracle": osha, "rust": rsha,
                          "detail": "一侧只有 body_sha256；哈希不同（无法定位字节偏移）"})
        else:
            whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "field": "body_sha256", "kind": "body_partial",
                                "reason": "只有一侧给了 body_hex；已用 body_sha256 互验相等，但未逐字节比",
                                "tier": (rule or {}).get("tier", "R"),
                                "approved_by": (rule or {}).get("approved_by", "")})
        return

    if action == "ability_layout":
        if _compare_ability(idx, ident, ob, rb, rule, diffs, whitelisted, undecidable):
            return

    if action == "mask_body_ranges" and len(ob) == len(rb):
        ranges = (rule or {}).get("ranges", [])
        masked = bytearray(len(ob))
        for start, end in ranges:
            stop = len(ob) if end is None else min(end, len(ob))
            for i in range(start, stop):
                masked[i] = 1
        first_unmasked = None
        first_masked = None
        unmasked_diffs = 0
        masked_diffs = 0
        for i in range(len(ob)):
            if ob[i] == rb[i]:
                continue
            if masked[i]:
                masked_diffs += 1
                if first_masked is None:
                    first_masked = i
            else:
                unmasked_diffs += 1
                if first_unmasked is None:
                    first_unmasked = i
        if unmasked_diffs:
            diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                          "field": "body", "kind": "body",
                          "offset": first_unmasked,
                          "oracle_byte": ob[first_unmasked], "rust_byte": rb[first_unmasked],
                          "oracle_len": len(ob), "rust_len": len(rb),
                          "diff_bytes": unmasked_diffs,
                          "detail": "body 在未掩码区间不同（首处 offset=%d）" % first_unmasked})
        if masked_diffs:
            whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "field": "body", "kind": "mask_body_ranges",
                                "diff_bytes": masked_diffs, "first_offset": first_masked,
                                "reason": "%s（区间内确有 %d 字节不同，已显式登记）"
                                          % (rule.get("reason", ""), masked_diffs),
                                "tier": rule.get("tier", ""),
                                "approved_by": rule.get("approved_by", "")})
        masked_ranges_note.append({"frame_index": idx, "ident": ident,
                                   "ranges": rule.get("ranges")})
        return

    # 默认 / compare / mask_fields / 区间比长度不同的情形
    if len(ob) != len(rb):
        off = first_diff_offset(ob, rb)
        diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                      "field": "body_len", "kind": "body_len",
                      "oracle": len(ob), "rust": len(rb), "offset": off,
                      "detail": "body 长度不同（无法逐字节对齐）"})
        return
    if ob != rb:
        off = first_diff_offset(ob, rb)
        diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                      "field": "body", "kind": "body",
                      "offset": off,
                      "oracle_byte": ob[off], "rust_byte": rb[off],
                      "oracle_len": len(ob), "rust_len": len(rb),
                      "diff_bytes": count_diff_bytes(ob, rb),
                      "detail": "body 逐字节不同（首处 offset=%d）" % off})


def compare_streams(o_stream, r_stream, rules):
    diffs = []
    whitelisted = []
    undecidable = []
    missing_fields = []
    masked_ranges_note = []

    def split(frames):
        kept, ignored = [], []
        for fr in frames:
            rule = rules.by_ident.get(fr["ident"])
            if rule and rule["action"] == "ignore":
                ignored.append(fr)
            else:
                kept.append(fr)
        return kept, ignored

    o_kept, o_ign = split(o_stream.frames)
    r_kept, r_ign = split(r_stream.frames)
    ignored = {"oracle": [fr["ident"] for fr in o_ign],
               "rust": [fr["ident"] for fr in r_ign]}

    # 覆盖检查：两侧出现过的 ident 都必须有规则（ignore 规则也算覆盖）。
    present = sorted({fr["ident"] for fr in o_stream.frames} |
                     {fr["ident"] for fr in r_stream.frames})
    unclassified = [i for i in present if i not in rules.by_ident]

    def head_compare(of, rf):
        ident = of["ident"]
        masks = rules.mask_for(ident)
        for field in STREAM_FIELDS:
            ov, rv = of.get(field), rf.get(field)
            mask = masks.get(field)
            if mask is not None:
                if ov is not None and rv is not None and ov != rv:
                    whitelisted.append({"source": "frame", "frame_index": of["index"],
                                        "ident": ident, "field": field,
                                        "kind": "masked_head", "oracle": ov, "rust": rv,
                                        "reason": mask["reason"], "tier": mask["tier"],
                                        "approved_by": mask["approved_by"],
                                        "origin": mask["origin"]})
                continue
            if ov is None and rv is None:
                missing_fields.append({"ident": ident, "field": field,
                                       "frame_index": of["index"],
                                       "detail": "两侧都缺该字段，无法比较（已显式登记）"})
                continue
            if ov is None or rv is None:
                undecidable.append({"source": "frame", "frame_index": of["index"],
                                    "ident": ident, "kind": "missing_field", "field": field,
                                    "oracle_present": ov is not None,
                                    "rust_present": rv is not None,
                                    "detail": "只有一侧给了该字段，无法比较"})
                continue
            if ov != rv:
                diffs.append({"source": "frame", "frame_index": of["index"], "ident": ident,
                              "field": field, "kind": "head",
                              "oracle": ov, "rust": rv,
                              "detail": "头部字段不同"})

    n = min(len(o_kept), len(r_kept))
    for i in range(n):
        of, rf = o_kept[i], r_kept[i]
        if of["ident"] != rf["ident"]:
            diffs.append({"source": "frame", "frame_index": i,
                          "ident": of["ident"],
                          "field": "ident", "kind": "ident",
                          "oracle": of["ident"], "rust": rf["ident"],
                          "detail": "按序对齐后 ident 不同（帧错位/一侧多帧）"})
            continue
        head_compare(of, rf)
        compare_bodies(of, rf, rules.by_ident.get(of["ident"]),
                       diffs, whitelisted, undecidable, masked_ranges_note)

    if len(o_kept) != len(r_kept):
        tail = o_kept[n:] if len(o_kept) > n else r_kept[n:]
        diffs.append({"source": "frame", "frame_index": n, "ident": None,
                      "field": "frame_count", "kind": "frame_count",
                      "oracle": len(o_kept), "rust": len(r_kept),
                      "detail": "可比较帧数不同（ignore 规则剔除后）；多出的前几个 ident：%s"
                                % [fr["ident"] for fr in tail[:10]]})

    return {
        "diffs": diffs,
        "whitelisted": whitelisted,
        "undecidable": undecidable,
        "missing_fields": missing_fields,
        "ignored": ignored,
        "present_idents": present,
        "unclassified": unclassified,
        "compared_frames": n,
        "masked_ranges": masked_ranges_note,
    }


# ---------------------------------------------------------------------------
# 库快照
# ---------------------------------------------------------------------------
def find_snapshot(dirpath):
    cand = os.path.join(dirpath, "db_snapshot.json")
    if os.path.exists(cand):
        return cand
    cand = os.path.join(dirpath, "db_snapshot")
    if os.path.exists(cand):
        return cand
    return None


def _materialize_inline(data, what, workdir):
    tables = data.get("tables")
    if not isinstance(tables, list) or not tables:
        raise Undecidable("%s 的 tables 必须是非空数组" % what)
    os.makedirs(workdir, exist_ok=True)
    manifest = {"format": "dbsnap/1", "source": what, "tables": []}
    for i, entry in enumerate(tables):
        where = "%s tables[%d]" % (what, i)
        if not isinstance(entry, dict):
            raise Undecidable("%s 必须是对象" % where)
        name = entry.get("table")
        cols = entry.get("columns")
        rows = entry.get("rows")
        if not isinstance(name, str) or "." not in name:
            raise Undecidable("%s 的 table 必须是 db.table 形式" % where)
        if not isinstance(cols, list) or not cols:
            raise Undecidable("%s 缺 columns" % where)
        col_names = []
        for c in cols:
            if isinstance(c, str):
                col_names.append(c)
            elif isinstance(c, dict) and isinstance(c.get("name"), str):
                col_names.append(c["name"])
            else:
                raise Undecidable("%s 的 columns 项必须是字符串或 {name:...}" % where)
        if not isinstance(rows, list):
            raise Undecidable("%s 缺 rows 数组" % where)
        lines = []
        for ri, row in enumerate(rows):
            if not isinstance(row, list):
                raise Undecidable("%s rows[%d] 必须是数组" % (where, ri))
            if len(row) != len(col_names):
                raise Undecidable("%s rows[%d] 列数 %d != columns %d"
                                  % (where, ri, len(row), len(col_names)))
            vals = []
            for v in row:
                if v is None:
                    vals.append("NULL")
                elif isinstance(v, bool):
                    raise Undecidable("%s rows[%d] 含布尔值（快照口径不接受）" % (where, ri))
                elif isinstance(v, (int, float)):
                    vals.append(repr(v) if isinstance(v, float) else str(v))
                elif isinstance(v, str):
                    if "\t" in v or "\n" in v or "\r" in v:
                        raise Undecidable("%s rows[%d] 含 TAB/LF/CR（dbsnap tsv 无法无损表示）"
                                          % (where, ri))
                    vals.append(v)
                else:
                    raise Undecidable("%s rows[%d] 含不支持的值类型 %s"
                                      % (where, ri, type(v).__name__))
            lines.append("\t".join(vals))
        payload = ("\n".join(lines) + ("\n" if lines else "")).encode("utf-8")
        fname = name.replace(".", "__") + ".tsv"
        fpath = os.path.join(workdir, fname)
        with open(fpath, "wb") as f:
            f.write(payload)
        manifest["tables"].append({"table": name, "file": fname, "rows": len(lines),
                                   "sha256": sha256_bytes(payload),
                                   "columns": [{"name": c} for c in col_names]})
    with open(os.path.join(workdir, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")
    return workdir


def prepare_snapshot(path, label, workdir):
    """把一个 db_snapshot.json / 目录 解析成可供 dbsnap_pair 读取的快照目录。"""
    if os.path.isdir(path):
        if not os.path.exists(os.path.join(path, "manifest.json")):
            raise Undecidable("%s 的库快照目录缺 manifest.json：%s" % (label, path))
        return path
    data = read_json(path, "%s 的库快照" % label)
    if not isinstance(data, dict):
        raise Undecidable("%s 的 db_snapshot.json 顶层必须是对象" % label)
    base = os.path.dirname(os.path.abspath(path))
    if "snapshot_dir" in data:
        target = data["snapshot_dir"]
        if not isinstance(target, str) or not target:
            raise Undecidable("%s 的 snapshot_dir 必须是非空字符串" % label)
        target = target if os.path.isabs(target) else os.path.normpath(os.path.join(base, target))
        if not os.path.exists(os.path.join(target, "manifest.json")):
            raise Undecidable("%s 的 snapshot_dir 里没有 manifest.json：%s" % (label, target))
        return target
    tables = data.get("tables")
    if not isinstance(tables, list) or not tables:
        raise Undecidable("%s 的 db_snapshot.json 既无 snapshot_dir 也无 tables" % label)
    if all(isinstance(t, dict) and "file" in t for t in tables):
        # 裸 dbsnap manifest：改写成绝对 file 路径（os.path.join 对绝对路径会直接采用）
        manifest = dict(data)
        entries = []
        for i, t in enumerate(tables):
            e = dict(t)
            f = e.get("file")
            if not isinstance(f, str) or not f:
                raise Undecidable("%s tables[%d].file 必须是非空字符串" % (label, i))
            e["file"] = f if os.path.isabs(f) else os.path.normpath(os.path.join(base, f))
            entries.append(e)
        manifest["tables"] = entries
        os.makedirs(workdir, exist_ok=True)
        with open(os.path.join(workdir, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
            json.dump(manifest, f, ensure_ascii=False, indent=2, sort_keys=True)
            f.write("\n")
        return workdir
    fmt = data.get("format", DB_INLINE_FORMAT)
    if fmt != DB_INLINE_FORMAT:
        raise Undecidable("%s 的库快照 format 只能是 %r（实际 %r）"
                          % (label, DB_INLINE_FORMAT, fmt))
    return _materialize_inline(data, "%s db_snapshot.json" % label, workdir)


def compare_db(o_dir, r_dir, db_opts, tmp_root, overrides):
    o_path = overrides.get("oracle") or find_snapshot(o_dir)
    r_path = overrides.get("rust") or find_snapshot(r_dir)
    info = {"oracle_snapshot": o_path, "rust_snapshot": r_path,
            "options": {"time_mode": db_opts["time_mode"], "key_mode": db_opts["key_mode"],
                        "ignore_columns": db_opts["ignore_columns"],
                        "allow_unknown_table": db_opts["allow_unknown_table"]},
            "diffs": [], "undecidable": [], "records": []}
    if o_path is None and r_path is None:
        info["state"] = "absent_both"
        info["note"] = "两侧都没有库快照：本次只判帧流；库后态未比较（显式登记，不是通过）"
        return info, []
    if o_path is None or r_path is None:
        info["state"] = "one_sided"
        info["undecidable"].append({"source": "db", "kind": "db_snapshot_missing",
                                    "detail": "只有%s侧有库快照，无法对账"
                                              % ("oracle" if o_path else "rust")})
        return info, info["undecidable"]
    try:
        o_snap = prepare_snapshot(o_path, "oracle", os.path.join(tmp_root, "oracle"))
        r_snap = prepare_snapshot(r_path, "rust", os.path.join(tmp_root, "rust"))
        mod = load_module("dbsnap_pair", DBSNAP_PAIR)
        rc, records, summary = mod.run_pair(
            o_snap, r_snap,
            key_mode=db_opts["key_mode"], time_mode=db_opts["time_mode"],
            null_token="NULL", ignore_columns=list(db_opts["ignore_columns"]),
            allow_unknown_table=bool(db_opts["allow_unknown_table"]), quiet=True)
    except (InputError, Undecidable):
        raise
    except Exception as e:  # dbsnap_pair.InputError 等：一律 fail-closed
        info["state"] = "error"
        info["undecidable"].append({"source": "db", "kind": "db_input_error",
                                    "detail": "%s: %s" % (type(e).__name__, e)})
        return info, info["undecidable"]
    info["state"] = "compared"
    info["summary"] = summary
    info["records"] = records
    und = []
    for rec in records:
        if rec.get("kind") in DB_UNDECIDABLE_KINDS:
            und.append({"source": "db", "kind": "db_" + str(rec.get("kind")), "record": rec})
        else:
            info["diffs"].append({"source": "db", "kind": str(rec.get("kind")), "record": rec})
    info["undecidable"] = und
    if summary.get("unrecognized_time_values"):
        info["note"] = ("normalized 下有 %d 个时间值无法识别格式，已按 exact 比较"
                        % summary["unrecognized_time_values"])
    return info, und


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def empty_report():
    return {
        "tool": TOOL,
        "rules": None,
        "inputs": {},
        "frames": {},
        "coverage": {},
        "diffs": [],
        "whitelisted": [],
        "undecidable": [],
        "missing_fields": [],
        "db": None,
        "verdict": "UNDECIDABLE",
        "exit_code": 2,
    }


def _parse_allow_unclassified(groups):
    allowed = set()
    for group in (groups or []):
        for tok in group.split(","):
            tok = tok.strip()
            if not tok:
                continue
            if not tok.isdigit():
                raise InputError("--allow-unclassified 只接受逗号分隔的数字 ident，实际 %r" % tok)
            allowed.add(int(tok))
    return allowed


def compare_main(args):
    report = empty_report()
    undecidable = []
    try:
        rules = load_rules(args.rules)
        report["rules"] = {"path": rules.source, "sha256": rules.sha256,
                           "format": RULES_FORMAT,
                           "ident_rule_count": len(rules.by_ident),
                           "global_masks": sorted(rules.global_masks),
                           "pending_review": rules.pending_review,
                           "rules_pending_review": bool(rules.pending_review)}
        o_stream = load_stream(args.oracle, "oracle")
        r_stream = load_stream(args.rust, "rust")
        o_meta = load_run_meta(args.oracle)
        r_meta = load_run_meta(args.rust)
        report["inputs"] = {
            "oracle": {"dir": o_stream.dirpath, "frames": o_stream.path, "run": o_meta},
            "rust": {"dir": r_stream.dirpath, "frames": r_stream.path, "run": r_meta},
        }
    except (InputError, Undecidable) as e:
        report["undecidable"].append({"source": "input", "kind": type(e).__name__,
                                      "detail": str(e)})
        report["verdict"] = "UNDECIDABLE"
        report["exit_code"] = 2
        return report

    cmp = compare_streams(o_stream, r_stream, rules)
    try:
        allowed_unclassified = _parse_allow_unclassified(args.allow_unclassified)
    except InputError as e:
        report["undecidable"].append({"source": "input", "kind": "bad_flag", "detail": str(e)})
        report["verdict"] = "UNDECIDABLE"
        report["exit_code"] = 2
        return report
    effective_unclassified = [i for i in cmp["unclassified"] if i not in allowed_unclassified]
    for ident in effective_unclassified:
        undecidable.append({"source": "coverage", "kind": "unclassified_ident",
                            "ident": ident,
                            "detail": "ident %d 在产物里出现但规则表没有条目（UNCLASSIFIED）" % ident})
    for ident in sorted(allowed_unclassified & set(cmp["unclassified"])):
        cmp["whitelisted"].append({"source": "coverage", "ident": ident, "field": "ident",
                                   "kind": "allowed_unclassified",
                                   "reason": "规则表未覆盖，但调用方用 --allow-unclassified 显式点名放行",
                                   "tier": "R", "approved_by": "CLI --allow-unclassified"})

    report["frames"] = {
        "oracle_total": len(o_stream.frames),
        "rust_total": len(r_stream.frames),
        "compared": cmp["compared_frames"],
        "ignored": {k: {"count": len(v), "idents": v} for k, v in cmp["ignored"].items()},
        "ignored_ident_rules": sorted(i for i, r in rules.by_ident.items()
                                      if r["action"] == "ignore"),
    }
    report["coverage"] = {
        "present_idents": cmp["present_idents"],
        "unclassified_idents": cmp["unclassified"],
        "allowed_unclassified": sorted(allowed_unclassified),
        "unresolved_unclassified": effective_unclassified,
        "ident_rules": [{"ident": i, "action": rules.by_ident[i]["action"],
                         "reason": rules.by_ident[i].get("reason"),
                         "tier": rules.by_ident[i].get("tier"),
                         "approved_by": rules.by_ident[i].get("approved_by")}
                        for i in sorted(rules.by_ident)],
        "global_masks": [dict(m, field=f)
                         for f, m in sorted(rules.global_masks.items())],
    }
    report["diffs"] = cmp["diffs"]
    report["whitelisted"] = cmp["whitelisted"]
    report["missing_fields"] = cmp["missing_fields"]
    report["masked_ranges"] = cmp["masked_ranges"]
    undecidable.extend(cmp["undecidable"])
    for p in o_stream.problems + r_stream.problems:
        undecidable.append({"source": "input", **p})
    report["undecidable"] = undecidable

    db_opts = dict(rules.db)
    if args.db_time_mode:
        db_opts["time_mode"] = args.db_time_mode
    if args.db_key_mode:
        db_opts["key_mode"] = args.db_key_mode
    if args.db_ignore_columns is not None:
        db_opts["ignore_columns"] = [c.strip() for c in args.db_ignore_columns.split(",") if c.strip()]
    if args.db_allow_unknown_table:
        db_opts["allow_unknown_table"] = True
    tmp_root = tempfile.mkdtemp(prefix="conformance-db-")
    try:
        db_info, db_und = compare_db(args.oracle, args.rust, db_opts, tmp_root,
                                     {"oracle": args.db_oracle, "rust": args.db_rust})
        report["db"] = db_info
        report["diffs"].extend(db_info.get("diffs") or [])
        report["undecidable"].extend(db_und)
    except (InputError, Undecidable) as e:
        report["db"] = {"state": "error", "detail": str(e), "diffs": [], "undecidable": []}
        report["undecidable"].append({"source": "db", "kind": "db_input_error",
                                      "detail": str(e)})
    finally:
        shutil.rmtree(tmp_root, ignore_errors=True)

    if report["undecidable"]:
        report["verdict"] = "UNDECIDABLE"
        report["exit_code"] = 2
    elif report["diffs"]:
        report["verdict"] = "FAIL"
        report["exit_code"] = 1
    else:
        report["verdict"] = "PASS"
        report["exit_code"] = 0
    return report


def human_summary(report):
    lines = ["== compare_runs =="]
    r = report.get("rules")
    if r:
        lines.append("rules: %s (sha256=%s, ident 规则 %d 条%s)"
                     % (r["path"], r["sha256"][:16], r["ident_rule_count"],
                        "，PENDING 复核" if r.get("rules_pending_review") else ""))
    fr = report.get("frames") or {}
    if fr:
        lines.append("frames: oracle=%d rust=%d compared=%d"
                     % (fr["oracle_total"], fr["rust_total"], fr["compared"]))
        ig = fr.get("ignored") or {}
        if any(v["count"] for v in ig.values()):
            lines.append("ignored(规则): " + ", ".join(
                "%s=%d" % (k, v["count"]) for k, v in sorted(ig.items())))
    cov = report.get("coverage") or {}
    if cov.get("present_idents"):
        lines.append("idents: %s" % cov["present_idents"])
    diffs = report.get("diffs") or []
    if diffs:
        lines.append("DIFFS: %d" % len(diffs))
        for d in diffs[:20]:
            if d.get("source") == "frame":
                lines.append("  [frame] idx=%s ident=%s field=%s offset=%s oracle=%s rust=%s"
                             % (d.get("frame_index"), d.get("ident"), d.get("field"),
                                d.get("offset"), str(d.get("oracle"))[:40],
                                str(d.get("rust"))[:40]))
            else:
                rec = d.get("record") or {}
                lines.append("  [db] %s" % json.dumps(rec, ensure_ascii=True,
                                                      sort_keys=True)[:200])
        if len(diffs) > 20:
            lines.append("  ... 其余 %d 条见 report.json" % (len(diffs) - 20))
    else:
        lines.append("DIFFS: 0")
    wl = report.get("whitelisted") or []
    if wl:
        agg = {}
        for w in wl:
            key = (w.get("kind"), w.get("field"), w.get("ident"))
            agg[key] = agg.get(key, 0) + 1
        lines.append("whitelisted: %d 条" % len(wl))
        for (kind, field, ident), n in sorted(agg.items(), key=lambda x: str(x[0])):
            lines.append("  %s ident=%s field=%s x%d" % (kind, ident, field, n))
    und = report.get("undecidable") or []
    if und:
        lines.append("UNDECIDABLE: %d" % len(und))
        for u in und[:20]:
            lines.append("  %s/%s: %s" % (u.get("source"), u.get("kind"),
                                          str(u.get("detail"))[:160]))
        if len(und) > 20:
            lines.append("  ... 其余 %d 条见 report.json" % (len(und) - 20))
    db = report.get("db") or {}
    if db:
        lines.append("db: state=%s%s" % (db.get("state"),
                                         (" " + db.get("note")) if db.get("note") else ""))
    mf = report.get("missing_fields") or []
    if mf:
        agg2 = {}
        for m in mf:
            key = (m["ident"], m["field"])
            agg2[key] = agg2.get(key, 0) + 1
        lines.append("missing(both sides): " + ", ".join(
            "ident %s/%s x%d" % (k[0], k[1], v)
            for k, v in sorted(agg2.items(), key=lambda x: str(x[0]))))
    lines.append("VERDICT: %s (exit %d)" % (report.get("verdict"), report.get("exit_code")))
    return "\n".join(lines)


def build_parser():
    p = argparse.ArgumentParser(
        description="双栈无头对拍比较器：帧流 + 数据库后态逐项 diff（PASS/FAIL/UNDECIDABLE）")
    p.add_argument("cmd", nargs="?", choices=["compare", "selftest"], default="compare")
    p.add_argument("--rust", help="Rust 侧产物目录（含 frames.ndjson）")
    p.add_argument("--oracle", help="C# oracle 侧产物目录（含 frames.ndjson）")
    p.add_argument("--rules", default=DEFAULT_RULES,
                   help="规则表 JSON（默认 tools/conformance/rules.json）")
    p.add_argument("--out", default="report.json",
                   help="报告输出路径（默认 report.json；- = stdout）")
    p.add_argument("--allow-unclassified", action="append", metavar="IDENTS",
                   help="显式放行未覆盖的 ident（逗号分隔，可重复）；仍会在报告里登记")
    p.add_argument("--db-oracle", help="oracle 库快照路径（默认 <oracle>/db_snapshot.json）")
    p.add_argument("--db-rust", help="rust 库快照路径（默认 <rust>/db_snapshot.json）")
    p.add_argument("--db-time-mode", choices=["exact", "presence", "normalized"],
                   help="覆盖规则表 db.time_mode")
    p.add_argument("--db-key-mode", choices=["pk", "multiset"], help="覆盖规则表 db.key_mode")
    p.add_argument("--db-ignore-columns", help="覆盖规则表 db.ignore_columns（逗号分隔）")
    p.add_argument("--db-allow-unknown-table", action="store_true",
                   help="覆盖规则表 db.allow_unknown_table")
    p.add_argument("--quiet", action="store_true", help="不打印终端摘要")
    return p


def write_report(report, out):
    if out in (None, ""):
        return
    if out == "-":
        print(json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True))
        return
    with open(out, "w", encoding="utf-8", newline="\n") as f:
        json.dump(report, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")


def cmd_selftest():
    here = os.path.dirname(os.path.abspath(__file__))
    me = sys.modules[__name__]
    mod = load_module("conformance_selftest_impl",
                      os.path.join(here, "selftest_impl.py"))
    return mod.run(me, here)


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.cmd == "selftest":
        return cmd_selftest()
    if not args.rust or not args.oracle:
        print("用法错误：compare 需要 --rust DIR 与 --oracle DIR", file=sys.stderr)
        return 2
    report = compare_main(args)
    write_report(report, args.out)
    if not args.quiet:
        print(human_summary(report))
    return report["exit_code"]


if __name__ == "__main__":
    sys.exit(main())
