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
1) 帧对齐：**按序**对齐（不是按 seq 找键）。默认**没有** ignore 规则：10=SM_TURN、
   41=SM_FEATURECHANGED 是真实协议帧（downlink 明确「SM_TURN+SM_FEATURECHANGED 一组两帧」，
   有逐字节黄金断言），必须比。确需剔除只能用显式 `--ignore-idents`；规则表里的
   `action=ignore` 若 `approved_by` 仍是 PENDING，则按**每侧每规则计数 + 形态**判：
   两侧计数相等**且**形态（param/tag/series + body_sha256 的多重集；recog 是 ActorId
   性质，不参与）相等 ⇒ 记为
   「已计数、形态已比」放行（report `frames.ignored_rule_census` /
   `ignored_ident_rules_counted`）；计数或形态对不上 ⇒ 仍 UNDECIDABLE
   （`pending_ignore_rule`，点名 oracle/rust/delta）——**保护没有取消，只是升级成可比判据**
   （卡 openmir2-ignore-rule-count-compare）。被剔除的消息号在两侧先各自剔除再逐位对齐。
   `ident` 永远参与比较（即使该帧其余字段被掩码）。
   例外（输入层，非 ignore 规则）：`ident` 为空（null/缺键）的**控制帧**（如 GameGate
   `+GD/<rtime>` 时间应答，frame_class=server_ack_time）在 load_stream 剔除并逐帧留痕
   （report `frames.filtered_control_frames`），不算 ignore、不静默；`frame_class=protocol`
   却缺 ident ⇒ 仍 UNDECIDABLE（驱动器产物自相矛盾）。
   两侧剔除后可比较帧数都是 0 ⇒ UNDECIDABLE(kind=empty_run)，只有 `--allow-empty` 放行。
2) 头部字段（recog/param/tag/series，外加驱动器字段 phase/seq/t_ms/conn）：默认哪些比由规则决定。
   recog（=会话相关 ActorId）、t_ms（墙钟）、conn（驱动器会话标签）、seq（驱动器本地序号）默认掩码。
   每条掩码都带 reason/tier，并在报告 `whitelisted` / `coverage.global_masks` 里显式登记，绝不静默丢弃。
   两侧都缺同一头部字段 ⇒ 默认 UNDECIDABLE（`--allow-missing-fields` 才降级为登记后放行）；
   只有一侧给了（无论该字段是否被掩码）⇒ 永远 UNDECIDABLE。
3) body：默认逐字节比较；差异报告 **ident + 帧序号 + 字节偏移** + 首次不同字节。
   * `mask_fields`：只掩头部字段，body 仍逐字节比。
   * `mask_body_ranges`：body 的指定 [start,end) 区间不参与判定（区间内若真不同，仍作为
     whitelisted 显式登记「此处不同但被掩码」）。
   * `ability_layout`：52 SM_ABILITY 的 40B(oracle)/58B(client) 布局对，按语义字段逐项比较。
     布局表**从 tools/loginfixture/capture_login_compare.py 导入**（ABILITY_LAYOUTS /
     ABILITY_FIELDS / decode_ability），本工具不抄第二份。驱动器落盘的是**线缆字节**，
     所以先按 ABILITY_WIRE_LENS（54→40、78→58，encoded_len 关系）做 EDCode 归一：
     语义不一致 ⇒ 差异；同长度同布局退回逐字节比（归一过的在明文空间比）；
     未知长度对 ⇒ 无法判定（fail-closed）。
   * `multiset_by_tick`：同一 ident 且在原帧流里**连续**的一段（块 = 同 ident 连续段，
     被别的 ident 隔开就不合并）按 `(param,tag,series,body_hex,phase)` 多重集比。顺序
     不同但集合相同 ⇒ rc 0，并**显式登记 `masked_order`**（不是静默 PASS）；集合不同 ⇒
     rc 1，逐条给出缺失/多余的具体帧（ident + param/tag/series + body 前 16 字节）。
     `tick_tolerance_ms` 只用于在登记里标注两侧 tick 窗口（`oracle_windows`/`rust_windows`
     + `tick_split`）：`t_ms` 是全局掩码的墙钟字段，**不能当判定边界**（1ms 抖动会把
     同一帧集误判成缺帧）；缺 `t_ms` ⇒ rc 2。只允许用于无因果序的帧（当前仅 ident 100），
     绝不推广到出现帧对/攻击序列。
   * `time_text`（内置掩码，仅 ident=100）：@time 回显 body = EDCode(`当前日期时间: `
     + `YYYY-MM-DD HH:MM:SS`)，时间戳数字是发送侧墙钟（两次运行必然不同 ⇒ 环境噪声，
     卡 openmir2-compare-time-noise-mask）。多重集键与逐帧 body 比较都用「时间文本位
     替换为等长占位符」后的明文；前缀静态文案与其余字节照常逐字节比；解码失败或未命中
     ⇒ 完全退回原路径（fail-closed）。掩码命中进报告 `masked_ranges`
     (space=plain_time_text)，被掩差异进 `whitelisted`(kind=time_text_masked)——
     都不是静默 PASS。掩码不越 ident：同形时间文本在其他 ident 照常逐字节比。
4) 规则覆盖：某 ident 在两侧任何一帧里出现、规则表却没有它的条目 ⇒ **UNCLASSIFIED ⇒ 退出码 2**
   （显式 `--ignore-idents` 点名的 ident 视为已分类：剔除即处置）。
   只有 `--allow-unclassified 1,2,3` 显式点名才放行，且报告里仍然逐个登记。
5) 库对账：两份快照走 tools/db-parity/dbsnap_pair.py 的同一套口径（直接 import 调 run_pair）。
   `db_snapshot.json` 支持三种形态（见 README）：
     a) `{"snapshot_dir": "<dbsnap 快照目录>"}`
     b) 裸 dbsnap manifest：`{"tables": [{"table":..., "file":..., ...}]}`（file 相对 json 所在目录）
     c) 内联：`{"format":"dbsnap-inline/1","tables":[{"table":...,"columns":[...],"rows":[[...]]}]}`
   结构问题（缺表/列不符/快照被改/未知表）⇒ 无法判定（2）；单元格/行差异 ⇒ 差异（1）。
   两侧都没有库快照 ⇒ 显式登记 `db.state=absent_both`，默认不当作差异；`--require-db` 时
   ⇒ 无法判定（2）。只有一侧有 ⇒ 无法判定（2）。

退出码：0 = 全等（PASS）/ 1 = 有差异（FAIL）/ 2 = 无法判定（UNDECIDABLE，含 UNCLASSIFIED）。
**两者同时出现时取 2**（未完成的判据优先，绝不拿「有差异」掩盖「没判完」）。

自检（全离线，不连网、不连库、不起进程）：
    python -I -B tools/conformance/compare_runs.py selftest

规则表：默认读本目录 rules.json（每条带 reason/tier/approved_by；初始全部 PENDING 复核），
`--rules FILE` 覆盖。
"""
import argparse
import datetime
import hashlib
from collections import Counter
import importlib.util
import json
import os
import re
import shutil
import sys
import tempfile

TOOL = "tools/conformance/compare_runs.py"
RULES_FORMAT = "conformance-rules/1"
DB_INLINE_FORMAT = "dbsnap-inline/1"

HEAD_FIELDS = ("recog", "param", "tag", "series")
STREAM_FIELDS = ("phase", "seq", "t_ms", "conn") + HEAD_FIELDS
#: `ignore` 规则的**形态**比较字段（卡 openmir2-ignore-rule-count-compare）。
#: **不含 `recog`**：`SM_HIT(14)` 的 recog 是攻击者 **ActorId**
#: （`PlayObject.Message.cs:1436` `MakeMessage(SM_HIT, processMsg.ActorId, …)`），
#: 两侧 ActorId 分配互不相关 ⇒ 比 recog 只会制造假红。残余风险已在
#: `docs/ignore-rule-count-compare-20261025.md` §4 登记（「同量不同攻击者」不算差异）。
#: **body 另算**（见 `_ignore_shape_key`）：帧体本来就在 `fr["body"]` /
#: `fr["body_sha256"]` 里（`load_stream`），不拿它比形态会漏掉「同量同头、体不同」。
IGNORE_SHAPE_FIELDS = ("param", "tag", "series")


def _ignore_shape_key(fr):
    """一帧的形态键：`param,tag,series|<body_sha256>`（多重集用，见 `ignore_rule_census`）。

    body 优先用驱动器声明的 `body_sha256`（`load_stream` 已校验与 `body_hex` 一致），
    缺失时按帧体现算——两者同源，取哪个都不改变结论。
    """
    body_sha = fr.get("body_sha256") or sha256_bytes(fr.get("body") or b"")
    return (",".join(str(fr.get(f)) for f in IGNORE_SHAPE_FIELDS) + "|" + body_sha)
ACTIONS = ("compare", "mask_fields", "mask_body_ranges", "ignore", "ability_layout",
           "multiset_by_tick", "bagitems_records")
#: SM_ADDITEM(200)/SM_BAGITEMS(201) 的包体 = 逐条 `ClientItem`（定长 124B）+ '/' 分隔。
#: 布局见 `src/OpenMir2/Packets/ClientPackets/ClientItem.cs` WritePacket：
#: Name@0（1B 长度+14B 定长）、StdMode@15、AniCount@18、MakeIndex@100(i32 LE)、Dura@104、DuraMax@106。
BAG_RECORD_LEN = 124
BAG_RECORD_FIELDS = {"MakeIndex": (100, 104)}
BAG_DEFAULT_MASKS = ("MakeIndex",)
TIERS = ("R", "L")

# ---------------------------------------------------------------------------
# ident=100（SM_SYSMESSAGE）「@time 回显」的时间文本掩码
# （卡 openmir2-compare-time-noise-mask，tier=L；比较器内置，rules.json 未改）
#
# 现象：gm_time / gm_batch 真机窗口（docs/window-run-1-20261024.md §3 第 3 条）里，
# 两侧 ident=100 的 @time 回显 body 等长但差 3 字节（线缆 offset 38/41/42 = 时间文本
# 的时分秒位）——GM 命令 @time 的回显是 `当前日期时间: YYYY-MM-DD HH:MM:SS`（GBK 明文
# 经 EDCode 编码），时间戳数字来自**发送侧墙钟**，两次独立运行必然不同 ⇒ 环境噪声，
# 非实现差异。
#
# 掩码口径（窄，且 fail-closed）：
#   * 只作用于 ident == 100 的 body；其他 ident 即使出现同形时间文本也不掩（selftest 牙）；
#   * body 必须能**严格** EDCode 解码（decode_bytes strict，越界字节即失败），解码失败
#     ⇒ 完全退回原逐字节路径，绝不掩；
#   * 明文必须命中「前缀 `当前日期时间: ` + `\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2}`」——
#     前缀是静态文案，**照常逐字节比**（文案写错照样红）；只把命中Match组 1（时间戳
#     数字段，明文区间）替换为**等长**占位符后再比；
#   * 掩码后相等但原字节不同 ⇒ whitelisted(kind=time_text_masked) 显式登记，不是静默 PASS；
#   * 掩码后仍不同 ⇒ diff（明文空间定位）——时间位之外的**真实差异绝不被掩**；
#   * 命中区间进报告 masked_ranges(space=plain_time_text)，逐帧可审计。
TIME_TEXT_IDENT = 100
TIME_TEXT_PREFIX = "当前日期时间: ".encode("gbk")
TIME_TEXT_RE = re.compile(
    re.escape(TIME_TEXT_PREFIX) + rb"(\d{4}-\d{2}-\d{2} \d{2}:\d{2}:\d{2})")
TIME_TEXT_PLACEHOLDER = b"YYYY-MM-DD HH:MM:SS"   # 与时间戳等长（19B），保持明文对齐

HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
DEFAULT_RULES = os.path.join(HERE, "rules.json")
LOGIN_COMPARE = os.path.join(REPO_ROOT, "tools", "loginfixture", "capture_login_compare.py")
MIR2FRAME = os.path.join(REPO_ROOT, "tools", "probe", "mir2frame.py")
DBSNAP_PAIR = os.path.join(REPO_ROOT, "tools", "db-parity", "dbsnap_pair.py")

# dbsnap_pair 的差异记录里，哪些 kind 属于「结构/输入问题（无法判定）」而不是「两侧取值不同」。
DB_UNDECIDABLE_KINDS = frozenset({
    "unknown_table", "missing_table", "schema", "tamper", "row_count",
    "column_count", "duplicate_key", "input_error",
    # 时间列见证（卡 openmir2-dbsrv-session-timezone）：未登记 / 无墙钟 / 值不可识别 ⇒ 判不了；
    # `time_witness_stale` 是**差异**（不进这个集合）。
    "time_witness_unregistered", "time_witness_unscoped", "time_witness_no_clock",
    "time_witness_unparsable",
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
            elif action == "bagitems_records":
                names = rule.get("record_mask", list(BAG_DEFAULT_MASKS))
                if not isinstance(names, list) or not names:
                    raise InputError("%s（bagitems_records）的 record_mask 必须是非空数组" % where)
                for nm in names:
                    if nm not in BAG_RECORD_FIELDS:
                        raise InputError("%s 的 record_mask 只允许 %s（实际 %r）"
                                         % (where, sorted(BAG_RECORD_FIELDS), nm))
                expanded["record_mask"] = list(names)
            elif action == "multiset_by_tick":
                tol = rule.get("tick_tolerance_ms", 0)
                if not is_int(tol) or tol < 0:
                    raise InputError("%s（multiset_by_tick）的 tick_tolerance_ms 必须是非负整数"
                                     "（实际 %r）" % (where, tol))
                expanded["tick_tolerance_ms"] = tol
            if action != "multiset_by_tick" and "tick_tolerance_ms" in rule:
                raise InputError("%s 的 tick_tolerance_ms 只允许用于 action=multiset_by_tick"
                                 % where)
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
        # 卡 openmir2-dbsrv-session-timezone ④：时间列**逐列登记**（presence 只比存在性，
        # 曾经把「两侧会话时区差 8h」藏了整整一轮）。形状非法一律加载期拒。
        # G1（review `df349949`，blocking）：**缺键不许静默关掉整层见证**。
        # `db.get(...)` 的默认值会让「没登记任何时间列」= 「不做任何见证」而 rc 0 PASS——
        # 与本日另外两次同源（`or 2`、`characters.ModifyTime` 不存在）：**看不见 ≠ 没问题**。
        if "time_columns" not in db:
            raise InputError(
                "db.time_columns 缺失：时间列见证层会被静默关掉（fail-open）。"
                "必须显式登记（可为空 dict，但那时被快照表的时间列一律判 unregistered）")
        tc = db["time_columns"]
        if not isinstance(tc, dict):
            raise InputError("db.time_columns 必须是 {表: {列: {mode, why}}}")
        norm = {}
        for table, cols in tc.items():
            if not isinstance(cols, dict):
                raise InputError("db.time_columns[%r] 必须是 {列: {mode, why}}" % (table,))
            norm[table] = {}
            for col, decl in cols.items():
                if not isinstance(decl, dict):
                    raise InputError("db.time_columns[%r][%r] 必须是对象" % (table, col))
                mode = decl.get("mode")
                if mode not in ("plausible", "presence"):
                    raise InputError("db.time_columns[%r][%r].mode 必须是 plausible/presence，得到 %r"
                                     % (table, col, mode))
                why = decl.get("why")
                if not isinstance(why, str) or not why.strip():
                    raise InputError("db.time_columns[%r][%r] 必须写 why（为什么这个判据）"
                                     % (table, col))
                norm[table][col] = {"mode": mode, "why": why.strip()}
        out["time_columns"] = norm
        # 已登记的**掩码列**（卡 `openmir2-db-makeindex-mask`）：必须带 reason + ref + 结构牙清单。
        mc = db.get("masked_columns", {})
        if not isinstance(mc, dict):
            raise InputError("db.masked_columns 必须是 {表: {列: {reason, ref, group_by, require}}}")
        norm_mc = {}
        allowed_checks = {"nonzero_both_sides", "distinct_per_player", "changes_on_mutation"}
        for table, cols_decl in mc.items():
            if not isinstance(cols_decl, dict):
                raise InputError("db.masked_columns[%r] 必须是 {列: …}" % (table,))
            norm_mc[table] = {}
            for col, decl in cols_decl.items():
                if not isinstance(decl, dict):
                    raise InputError("db.masked_columns[%r][%r] 必须是对象" % (table, col))
                reason = decl.get("reason")
                ref = decl.get("ref")
                if not isinstance(reason, str) or not reason.strip():
                    raise InputError("db.masked_columns[%r][%r] 必须写 reason" % (table, col))
                if not isinstance(ref, str) or not ref.strip():
                    raise InputError("db.masked_columns[%r][%r] 必须写 ref（登记编号 + 行号）"
                                     % (table, col))
                req = decl.get("require")
                if not isinstance(req, list) or not req:
                    raise InputError("db.masked_columns[%r][%r].require 必须是非空结构牙清单"
                                     "（掩掉一列必须带牙，缺了就是放水）" % (table, col))
                bad = sorted(set(req) - allowed_checks)
                if bad:
                    raise InputError("db.masked_columns[%r][%r].require 有未知检查 %s（允许 %s）"
                                     % (table, col, bad, sorted(allowed_checks)))
                norm_mc[table][col] = {"reason": reason.strip(), "ref": ref.strip(),
                                       "group_by": decl.get("group_by"),
                                       "require": list(req)}
        out["masked_columns"] = norm_mc
        tol = db.get("time_tolerance_s", 600)
        if not is_int(tol) or tol < 0 or tol > 86400:
            raise InputError("db.time_tolerance_s 必须是 0..86400 的整数")
        out["time_tolerance_s"] = int(tol)
        # 窗口**下界**的 slack 要小：`start - 300s` 的写法会让「上一次运行写的行」误绿（F1-C）。
        lead = db.get("time_lead_s", 30)
        if not is_int(lead) or lead < 0 or lead > 3600:
            raise InputError("db.time_lead_s 必须是 0..3600 的整数")
        out["time_lead_s"] = int(lead)
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
#: **控制帧过滤（compare 输入层；卡 openmir2-spell-cast-controlframe-filter，tier=L）**。
#: frames.ndjson 里 `ident` 为空（null / 缺键）的行**不是游戏协议帧**——协议帧必有 u16
#: 消息号；它们是网关/驱动的**控制帧**（实测：GameGate 对 `+GD` 时间请求的明文应答
#: `+GD/<rtime>`，frame_class=server_ack_time，分类见 tools/probe/mir2frame.py
#: classify_plain；window-run-1 spell_cast 两侧各 2 条）。本比较器按 u16 ident 对齐，
#: 控制帧没有 ident、也从不参与判定，留在流里只会把整轮判成 UNDECIDABLE（「缺合法
#: ident」fail-closed，window-run-1 spell_cast 实测 oracle 第 31 行）⇒ 在 load_stream
#: 输入层显式剔除。
#: **不是静默吞帧**：逐帧登记进 Stream.filtered_control（行号/frame_class/seq/t_ms/
#: body 摘要），report["frames"]["filtered_control_frames"] 与终端摘要都给计数，
#: 可与文件总行数对账（total = 协议帧 + filtered_control + 空行）。
#: 红线：只滤 ident **为空**的行；ident 存在但非法（负数 / >65535 / 类型错）仍然
#: fail-closed 判 UNDECIDABLE；frame_class=protocol 却缺 ident 是驱动器自相矛盾的
#: 产物，同样 fail-closed——过滤绝不扩大到任何声明为协议帧（或带合法 ident）的行。
CONTROL_FRAME_FILTER = "ident-null-control-frame"


class Stream(object):
    def __init__(self, label, dirpath, path, frames, problems,
                 filtered_control=None):
        self.label = label
        self.dirpath = dirpath
        self.path = path
        self.frames = frames
        self.problems = problems
        #: ident=null 的控制帧台账（CONTROL_FRAME_FILTER：被输入层剔除，逐帧留痕不静默）
        self.filtered_control = list(filtered_control or [])


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
    filtered_control = []
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
            if ident is None:
                # CONTROL_FRAME_FILTER：ident 为空的行是控制帧，显式剔除并逐帧留痕。
                if rec.get("frame_class") == "protocol":
                    raise Undecidable("%s 声明 frame_class=protocol 却缺 ident"
                                      "（驱动器产物自相矛盾，不按控制帧滤）" % where)
                filtered_control.append({
                    "line": lineno, "frame_class": rec.get("frame_class"),
                    "seq": rec.get("seq"), "t_ms": rec.get("t_ms"),
                    "body_len": rec.get("body_len"),
                    "body_sha256": rec.get("body_sha256")})
                continue
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
    return Stream(label, os.path.abspath(dirpath), os.path.abspath(path), frames,
                  problems, filtered_control)


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


def time_masked_plain(body):
    """ident=100 的 EDCode 线缆 body -> (时间文本掩码后的明文, 命中的明文区间列表)。

    掩码 = 把 `当前日期时间: ` 后面的 `YYYY-MM-DD HH:MM:SS` 数字段（Match 组 1）替换为
    等长占位符。只有**严格解码成功且命中该模式**才返回结果；否则返回 None
    （调用方必须退回原逐字节路径——掩码绝不放宽、绝不掩盖非时间差异）。
    """
    try:
        mod = load_module("mir2frame", MIR2FRAME)
        plain = mod.decode_bytes(body, strict=True)
    except Undecidable:
        return None             # 解码器模块不可用：不掩码（fail-closed）
    except Exception:           # FrameError 等：不是合法 EDCode 线缆字节 ⇒ 不掩码
        return None
    spans = [list(m.span(1)) for m in TIME_TEXT_RE.finditer(plain)]
    if not spans:
        return None
    out = bytearray()
    pos = 0
    for start, end in spans:
        out += plain[pos:start]
        out += TIME_TEXT_PLACEHOLDER
        pos = end
    out += plain[pos:]
    return bytes(out), spans


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


# ---------------------------------------------------------------------------
# multiset_by_tick：同 ident 连续段的帧按多重集比（不按位置）
# ---------------------------------------------------------------------------
# 语义：同一 ident、且在原帧流里**连续**的一段，按 (param,tag,series,body_hex,phase)
# 排序后逐项比（phase 默认参与比较 ⇒ 必须进键，reviewer-2 M-1）。
#   * 顺序不同但多重集相同 ⇒ rc 0，**显式登记 masked_order**（不是静默 PASS）；
#   * 多重集不同 ⇒ rc 1，逐条给出「缺失/多余」的帧（ident + 关键字段 + body 前 16B）。
# 边界（reviewer-2 I-1/I-3）：
#   * 块 = 同 ident 连续段：被别的 ident 隔开的两段**绝不合并**（因果序安全）；
#   * t_ms **不是**判定边界：它是全局掩码的墙钟字段，按它硬切块会把「两侧帧集完全相同、
#     只是差 1ms」误判成缺帧。它只在 masked_order 里标注 tick 窗口（tick_split）。
def _ord_token(v):
    """把头部取值映射成可全序比较的 token（int/str/None 混在一列也不会比较崩）。"""
    if v is None:
        return (0, 0, "")
    if is_int(v):
        return (1, v, "")
    if isinstance(v, str):
        return (2, 0, v)
    return (3, 0, repr(v))


def _multiset_body_token(fr):
    body = fr["body"]
    if body is not None and fr["ident"] == TIME_TEXT_IDENT:
        # ident=100：多重集身份用时间文本掩码后的明文——@time 回显的时间戳是墙钟噪声，
        # 不能让它把「同一文案、只差时分秒」的两帧判成缺失/多余；掩码未命中（非时间文本
        # 或解码失败）时退回原 wire hex token，行为与掩码引入前完全一致。
        masked = time_masked_plain(body)
        if masked is not None:
            return "plain-time-masked:" + masked[0].hex()
    if body is not None:
        return "hex:" + body.hex()
    if fr["body_sha256"]:
        return "sha256:" + fr["body_sha256"]
    return "absent:"


def multiset_key(fr):
    """多重集身份 = (param,tag,series,body,phase)。排序与计数都用它；**不得省略 body**。

    phase 默认参与头部比较（且不在全局掩码里）⇒ 必须进键：否则同 body、不同 phase
    的两帧顺序互换时会退化成 phase 差异，而不是「顺序不同、集合相同」（reviewer-2 M-1）。
    """
    return (_ord_token(fr.get("param")), _ord_token(fr.get("tag")),
            _ord_token(fr.get("series")), _multiset_body_token(fr),
            _ord_token(fr.get("phase")))


def _frame_brief(fr):
    body = fr["body"]
    return {"ident": fr["ident"], "frame_index": fr["index"], "seq": fr.get("seq"),
            "t_ms": fr.get("t_ms"),
            "param": fr.get("param"), "tag": fr.get("tag"), "series": fr.get("series"),
            "body_len": len(body) if body is not None else None,
            "body_sha256": fr.get("body_sha256"),
            "body_prefix_hex": body[:16].hex() if body is not None else None}


# 52 SM_ABILITY 的已知布局对：布局表（tools/loginfixture/capture_login_compare.py 的
# ABILITY_LAYOUTS）描述的是**明文** 40B/58B 结构；而本仓库驱动器
# （tools/probe/mir2_probe.py，frames.ndjson 的 body 字段）落盘的是**线缆字节**（EDCode 编码后）。
# 所以 52 先按「线缆长度 → 明文长度」归一，再落到布局表：
#   * 40B 明文 = C# Ability.cs 声明序（EncodeMessage(WAbil)），encoded_len(40) = 54；
#   * 58B 明文 = 冻结客户端 HIGHHP TAbility（record.rs::encode_ability_client；
#     client-e2e-enter-gaps-20261013/21-client-ability-layout.md），encoded_len(58) = 78。
# 首次双栈真机对拍实测就是 54↔40、78↔58（docs/conformance-first-dual-run-20261023.md §2.3）。
# **新增布局对必须附来源**（C#/Rust 源锚 + 实测样本），否则一律按未知长度判不了。
ABILITY_WIRE_LENS = {
    54: 40,  # encoded_len(40)：oracle EncodeMessage(WAbil) 40B 布局
    78: 58,  # encoded_len(58)：冻结客户端 HIGHHP TAbility 58B 布局
}


def _ability_plain(body, mod):
    """52 body → (明文字节, 是否做过线缆归一)。未知长度原样返回，由调用方判 UNKNOWN。"""
    if len(body) in mod.ABILITY_LAYOUTS:
        return body, False
    expect = ABILITY_WIRE_LENS.get(len(body))
    if expect is None:
        return body, False
    try:
        plain = mod.decode(body)
    except Exception as e:  # noqa: BLE001 - 解码失败必须 fail-closed
        raise Undecidable("52 线缆 body（%dB）EDCode 解码失败：%s: %s"
                          % (len(body), type(e).__name__, e))
    if len(plain) != expect:
        raise Undecidable("52 线缆 body（%dB）解码得 %dB，与登记的明文 %dB 不符"
                          % (len(body), len(plain), expect))
    return plain, True


def _compare_ability(idx, ident, ob, rb, rule, diffs, whitelisted, undecidable):
    """52 的 40/58 布局对：按语义字段比。返回 True 表示已处理。"""
    try:
        mod = load_module("capture_login_compare", LOGIN_COMPARE)
        layouts = mod.ABILITY_LAYOUTS
        fields = mod.ABILITY_FIELDS
        decode_ability = mod.decode_ability
    except Undecidable as e:
        undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "kind": "ability_prereq", "detail": str(e)})
        return True
    try:
        op, o_wire = _ability_plain(ob, mod)
        rp, r_wire = _ability_plain(rb, mod)
    except Undecidable as e:
        undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "kind": "ability_wire_layout", "detail": str(e)})
        return True
    pair = (len(op), len(rp))
    wire = {"oracle_wire_len": len(ob) if o_wire else None,
            "rust_wire_len": len(rb) if r_wire else None,
            "wire_normalized": bool(o_wire or r_wire)}
    if pair in ((40, 58), (58, 40)):
        od = decode_ability(op)
        rd = decode_ability(rp)
        if od is None or rd is None:
            undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "kind": "ability_decode",
                                "detail": "40/58 布局对解不出语义字段（body 被截断？）"})
            return True
        mism = []
        for name in fields:
            if od[name] != rd[name]:
                mism.append({"field": name, "oracle": od[name], "rust": rd[name],
                             "oracle_offset": layouts[len(op)][name][1],
                             "rust_offset": layouts[len(rp)][name][1]})
        if mism:
            for m in mism:
                diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                              "field": "52_semantic:%s" % m["field"], "kind": "ability_semantic",
                              "oracle": m["oracle"], "rust": m["rust"],
                              "oracle_offset": m["oracle_offset"],
                              "rust_offset": m["rust_offset"],
                              "space": "plain",
                              "oracle_body_len": len(op), "rust_body_len": len(rp),
                              "oracle_wire_len": wire["oracle_wire_len"],
                              "rust_wire_len": wire["rust_wire_len"],
                              "wire_normalized": wire["wire_normalized"],
                              "detail": "52 语义字段 %s 不同（oracle %sB / rust %sB 明文布局）"
                                        % (m["field"], len(op), len(rp))})
        else:
            whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "field": "52_layout", "kind": "ability_layout",
                                "oracle_len": len(op), "rust_len": len(rp),
                                "oracle_wire_len": wire["oracle_wire_len"],
                                "rust_wire_len": wire["rust_wire_len"],
                                "wire_normalized": wire["wire_normalized"],
                                "reason": "oracle40/client58 布局差；18 个语义字段逐项相等",
                                "tier": rule.get("tier", ""),
                                "approved_by": rule.get("approved_by", "")})
        return True
    if len(op) == len(rp) and len(op) in layouts:
        if not wire["wire_normalized"]:
            return False  # 两侧都是明文同布局 ⇒ 交给 compare_bodies 逐字节比
        # 至少一侧是线缆字节：长度已归一到明文，必须在明文空间比（原始线缆长度可以不同）
        if op == rp:
            whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                                "field": "52_body", "kind": "ability_layout_wire",
                                "oracle_len": len(op), "rust_len": len(rp),
                                "oracle_wire_len": wire["oracle_wire_len"],
                                "rust_wire_len": wire["rust_wire_len"],
                                "wire_normalized": True,
                                "reason": "52 线缆长度经 EDCode 归一为 %dB 明文布局后逐字节相等"
                                          % len(op),
                                "tier": rule.get("tier", ""),
                                "approved_by": rule.get("approved_by", "")})
            return True
        off = first_diff_offset(op, rp)
        diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                      "field": "body", "kind": "body", "offset": off,
                      "oracle_byte": op[off], "rust_byte": rp[off],
                      "oracle_len": len(op), "rust_len": len(rp),
                      "diff_bytes": count_diff_bytes(op, rp),
                      "space": "plain",
                      "oracle_wire_len": wire["oracle_wire_len"],
                      "rust_wire_len": wire["rust_wire_len"],
                      "wire_normalized": True,
                      "detail": "52 线缆归一成明文后逐字节不同（首处 offset=%d，明文空间）" % off})
        return True
    undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                        "kind": "ability_layout_unknown",
                        "detail": "52 的 body 长度组合（原始 %s，归一后 %s）不在已知布局表 %s 里；"
                                  "已知明文 40/58、线缆 54/78，新增长度对必须附来源后再登记"
                                  % ((len(ob), len(rb)), pair, sorted(layouts))})
    return True


def _decode_bag_records(body, mod):
    """`ClientItem` 包体 -> ([124B 记录], None) 或 (None, 原因)。坏数据一律判不了（不猜）。"""
    segs = body.split(b"/")
    if segs and segs[-1] == b"":
        segs.pop()                                  # oracle 每条记录后都追加 '/'
    records = []
    for i, seg in enumerate(segs):
        if not seg:
            return None, "空段（'/' 连写）@slot %d" % i
        try:
            raw = mod.decode_bytes(seg, strict=True)
        except Exception as ex:                     # noqa: BLE001 - FrameError 等一律 fail-closed
            return None, "slot %d EDCode 解码失败：%s" % (i, ex)
        if len(raw) != BAG_RECORD_LEN:
            return None, "slot %d 解码后 %d 字节，期望 %d" % (i, len(raw), BAG_RECORD_LEN)
        records.append(raw)
    return records, None


def _compare_bagitems(idx, ident, ob, rb, rule, diffs, whitelisted, undecidable):
    """客户端物品记录级比较（SM_ADDITEM(200)/SM_BAGITEMS(201)）。

    口径：**逐条 124B 记录**比；只有 `record_mask` 登记过的字段（默认 MakeIndex）允许不同，
    其余每个字节都必须相同。被掩的字段差异**显式登记**进 whitelisted（不是静默放行）；
    任何解码失败 ⇒ undecidable（绝不按通过处理）。
    """
    try:
        mod = load_module("mir2frame", MIR2FRAME)
    except Undecidable as ex:
        undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "kind": "bagitems_decoder_unavailable",
                            "detail": "EDCode 解码器不可用 ⇒ 物品记录判不了：%s" % ex})
        return
    o_records, o_err = _decode_bag_records(ob, mod)
    r_records, r_err = _decode_bag_records(rb, mod)
    if o_err or r_err:
        undecidable.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "kind": "bagitems_decode",
                            "oracle_error": o_err, "rust_error": r_err,
                            "detail": "物品记录无法解码 ⇒ 判不了（不按通过处理）"})
        return
    if len(o_records) != len(r_records):
        diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                      "field": "body", "kind": "bagitems_count",
                      "oracle": len(o_records), "rust": len(r_records),
                      "detail": "物品记录条数不同"})
        return
    masks = [(name, BAG_RECORD_FIELDS[name])
             for name in rule.get("record_mask", list(BAG_DEFAULT_MASKS))]
    masked_slots = 0
    first_masked = None
    for i, (o_rec, r_rec) in enumerate(zip(o_records, r_records)):
        if o_rec == r_rec:
            continue
        off = None
        for j in range(BAG_RECORD_LEN):
            if any(start <= j < end for _, (start, end) in masks):
                continue
            if o_rec[j] != r_rec[j]:
                off = j
                break
        if off is not None:
            diffs.append({"source": "frame", "frame_index": idx, "ident": ident,
                          "field": "body", "kind": "bagitems_record", "slot": i,
                          "offset": off,
                          "oracle_byte": o_rec[off], "rust_byte": r_rec[off],
                          "detail": "物品记录 slot=%d 在 offset=%d 不同（掩码字段之外）"
                                    % (i, off)})
            return
        masked_slots += 1
        for name, (start, end) in masks:
            if o_rec[start:end] != r_rec[start:end] and first_masked is None:
                first_masked = {"slot": i, "field": name,
                                "oracle": o_rec[start:end].hex(),
                                "rust": r_rec[start:end].hex()}
    if masked_slots:
        whitelisted.append({"source": "frame", "frame_index": idx, "ident": ident,
                            "field": "body", "kind": "bagitems_masked_field",
                            "slots": masked_slots,
                            "fields": sorted({name for name, _ in masks}),
                            "first": first_masked,
                            "reason": rule.get("reason", ""),
                            "tier": rule.get("tier", ""),
                            "approved_by": rule.get("approved_by", "")})


def _compare_time_masked_body(fr_o, fr_r, rule, diffs, whitelisted, masked_ranges_note):
    """ident=100 @time 回显的时间文本掩码比较。返回 True 表示 body 已处置（不再走默认逐字节）。

    * 任一侧解码失败，或两侧都没命中时间文本 ⇒ False（退回原路径，无任何掩码行为）；
    * 掩码后相等、原字节不同 ⇒ whitelisted(kind=time_text_masked) 显式登记（不是静默 PASS）；
    * 掩码后仍不同 ⇒ diff（明文空间定位）——时间位之外的真实差异绝不被掩。
    """
    ident = fr_o["ident"]
    idx = fr_o["index"]
    ob, rb = fr_o["body"], fr_r["body"]
    o_mask = time_masked_plain(ob)
    r_mask = time_masked_plain(rb)
    if o_mask is None or r_mask is None:
        return False
    om, o_spans = o_mask
    rm, r_spans = r_mask
    if not o_spans and not r_spans:
        return False
    masked_ranges_note.append({"frame_index": idx, "ident": ident,
                               "space": "plain_time_text",
                               "oracle_spans": o_spans, "rust_spans": r_spans})
    if om == rm:
        if ob != rb:
            whitelisted.append({
                "source": "frame", "frame_index": idx, "ident": ident,
                "field": "body", "kind": "time_text_masked",
                "oracle_time_spans": o_spans, "rust_time_spans": r_spans,
                "diff_bytes": count_diff_bytes(ob, rb),
                "oracle_wire_len": len(ob), "rust_wire_len": len(rb),
                "plain_len": len(om),
                "reason": "ident=100 @time 回显的时间戳数字段是发送侧墙钟"
                          "（跨运行不可复现，卡 openmir2-compare-time-noise-mask，"
                          "比较器内置掩码，rules.json 未改）；前缀静态文案与其余字节"
                          "仍逐字节比",
                "tier": "L",
                "approved_by": "卡 openmir2-compare-time-noise-mask（draft: "
                               "openmir2-worker-58）",
                "detail": "ident=100 body 仅时间文本位不同（线缆 %d 字节），掩码后逐字节相等"
                          % count_diff_bytes(ob, rb)})
        return True
    # 掩码后仍不同 ⇒ 真实差异（占位符两侧相同，任何差异都在时间位之外）
    if len(om) != len(rm):
        diffs.append({
            "source": "frame", "frame_index": idx, "ident": ident,
            "field": "body_len", "kind": "body_len", "space": "plain",
            "oracle": len(om), "rust": len(rm),
            "oracle_wire_len": len(ob), "rust_wire_len": len(rb),
            "oracle_time_spans": o_spans, "rust_time_spans": r_spans,
            "detail": "ident=100 body 掩掉时间文本位之后明文长度仍不同"
                      "（oracle %dB / rust %dB）——时间位之外的真实差异不被掩码" % (len(om), len(rm))})
        return True
    off = first_diff_offset(om, rm)
    diffs.append({
        "source": "frame", "frame_index": idx, "ident": ident,
        "field": "body", "kind": "body", "space": "plain",
        "offset": off,
        "oracle_byte": om[off], "rust_byte": rm[off],
        "oracle_plain_len": len(om), "rust_plain_len": len(rm),
        "oracle_wire_len": len(ob), "rust_wire_len": len(rb),
        "diff_bytes": count_diff_bytes(om, rm),
        "oracle_time_spans": o_spans, "rust_time_spans": r_spans,
        "detail": "ident=100 body 掩掉时间文本位之后仍有差异（明文首处 offset=%d）——"
                  "时间位之外的真实差异不被掩码" % off})
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

    if action == "bagitems_records":
        _compare_bagitems(idx, ident, ob, rb, rule, diffs, whitelisted, undecidable)
        return

    if ident == TIME_TEXT_IDENT and _compare_time_masked_body(
            fr_o, fr_r, rule, diffs, whitelisted, masked_ranges_note):
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


def _build_blocks(frames, rules):
    """把一侧（已按 ignore 规则剔除后的）帧流切成对齐块。

    * 普通 ident：每帧一个 single 块（与原逐帧对齐完全一致）；
    * action=multiset_by_tick 的 ident：**连续**同 ident 的帧合成一个 multiset 块。

    边界只由「ident 是否连续」决定：被别的 ident 隔开的两段绝不跨段合并（否则会抹掉
    与 10/41/14 等的因果序，reviewer-2 I-1）。t_ms **不参与切块**：它是全局掩码的墙钟
    字段，按它硬切会把「两侧同一帧集、只是差 1ms」误判成缺帧（reviewer-2 I-3）。
    """
    blocks = []
    i = 0
    n = len(frames)
    while i < n:
        fr = frames[i]
        rule = rules.by_ident.get(fr["ident"])
        if not rule or rule.get("action") != "multiset_by_tick":
            blocks.append({"kind": "single", "ident": fr["ident"], "frames": [fr],
                           "rule": None})
            i += 1
            continue
        run = [fr]
        j = i + 1
        while j < n and frames[j]["ident"] == fr["ident"]:
            run.append(frames[j])
            j += 1
        blocks.append({"kind": "multiset", "ident": fr["ident"], "frames": run,
                       "rule": rule})
        i = j
    return blocks


def _tick_windows(frames, tol):
    """把块内帧按 t_ms 切成 tick 窗口（greedy，锚点 = 窗口首帧）。

    **只作证据**：t_ms 是全局掩码字段（墙钟，不是协议内容），不参与判定。
    缺 t_ms ⇒ None（compare_streams 的预扫会先把这侧判成 rc 2）。
    """
    windows = []
    for f in frames:
        t = f.get("t_ms")
        if not is_int(t):
            return None
        if windows and abs(t - windows[-1][0]) <= tol:
            windows[-1][1].append(f)
        else:
            windows.append((t, [f]))
    return windows


def _window_sig(windows):
    if windows is None:
        return None
    return [tuple(sorted(multiset_key(f) for f in w[1])) for w in windows]


def _ser_windows(windows):
    if windows is None:
        return []
    return [{"start_ms": start, "frames": [_frame_brief(f) for f in fs]}
            for start, fs in windows]


def _compare_multiset_block(ob, rb, head_compare, diffs, whitelisted, undecidable,
                            masked_ranges_note, masked_order):
    """按多重集比一个 multiset 块（块 = 同 ident 连续段）。返回配成对比较的帧数。"""
    ident = ob["ident"]
    rule = ob["rule"] or {}
    ofr, rfr = ob["frames"], rb["frames"]
    o_keys = [multiset_key(f) for f in ofr]
    r_keys = [multiset_key(f) for f in rfr]
    o_cnt, r_cnt = Counter(o_keys), Counter(r_keys)
    o_first, r_first = {}, {}
    for f, k in zip(ofr, o_keys):
        o_first.setdefault(k, f)
    for f, k in zip(rfr, r_keys):
        r_first.setdefault(k, f)

    # 判定在**整块**上做：绝不用「块内位置/块数」冒充帧数，也不以 t_ms 切分判缺帧
    # （reviewer-2 I-3：两侧帧数相同却报缺帧是自相矛盾）。
    set_diff = 0
    for key in sorted(set(o_cnt) | set(r_cnt)):
        delta = o_cnt.get(key, 0) - r_cnt.get(key, 0)
        if delta > 0:
            brief = _frame_brief(o_first[key])
            diffs.append({"source": "frame", "frame_index": brief["frame_index"],
                          "ident": ident, "field": "multiset", "kind": "multiset_missing",
                          "side": "oracle", "absent_on": "rust", "count": delta,
                          "frame": brief,
                          "detail": "rust 侧缺失 %d 帧（同 ident 连续段的多重集）："
                                    "param=%r tag=%r series=%r phase=%r body_len=%r "
                                    "body_prefix=%s"
                                    % (delta, brief["param"], brief["tag"], brief["series"],
                                       brief.get("phase"), brief["body_len"],
                                       brief["body_prefix_hex"])})
            set_diff += delta
        elif delta < 0:
            brief = _frame_brief(r_first[key])
            diffs.append({"source": "frame", "frame_index": brief["frame_index"],
                          "ident": ident, "field": "multiset", "kind": "multiset_extra",
                          "side": "rust", "absent_on": "oracle", "count": -delta,
                          "frame": brief,
                          "detail": "rust 侧多余 %d 帧（同 ident 连续段的多重集）："
                                    "param=%r tag=%r series=%r phase=%r body_len=%r "
                                    "body_prefix=%s"
                                    % (-delta, brief["param"], brief["tag"], brief["series"],
                                       brief.get("phase"), brief["body_len"],
                                       brief["body_prefix_hex"])})
            set_diff += -delta

    # 集合相同、只是顺序不同 ⇒ 显式登记 masked_order（不是静默 PASS）。
    if set_diff == 0 and o_keys != r_keys:
        tol = rule.get("tick_tolerance_ms", 0)
        ow, rw = _tick_windows(ofr, tol), _tick_windows(rfr, tol)
        tick_split = None
        if ow is not None and rw is not None:
            tick_split = _window_sig(ow) != _window_sig(rw)
        detail = ("ident %d 同块（同 ident 连续段）的 %d 帧两侧顺序不同、多重集相同 ⇒ "
                  "按多重集判等（已登记，不是静默通过）" % (ident, len(ofr)))
        if tick_split:
            detail += ("；两侧 tick 窗口结构不同（t_ms 被全局掩码、1ms 抖动不改变判定，"
                       "tick_split 仅作证据）")
        masked_order.append({
            "source": "frame", "ident": ident, "kind": "masked_order",
            "oracle_frame_indices": [f["index"] for f in ofr],
            "rust_frame_indices": [f["index"] for f in rfr],
            "oracle_order": [_frame_brief(f) for f in ofr],
            "rust_order": [_frame_brief(f) for f in rfr],
            "tick_tolerance_ms": tol,
            "tick_split": tick_split,
            "oracle_windows": _ser_windows(ow),
            "rust_windows": _ser_windows(rw),
            "reason": rule.get("reason", ""), "tier": rule.get("tier", ""),
            "approved_by": rule.get("approved_by", ""),
            "detail": detail})

    # 多重集相同的配对帧仍逐帧走头部/body 比较（排序不得跳过任何判定）。
    osorted = sorted(ofr, key=multiset_key)
    rsorted = sorted(rfr, key=multiset_key)
    matched = 0
    i = j = 0
    while i < len(osorted) and j < len(rsorted):
        ok, rk = multiset_key(osorted[i]), multiset_key(rsorted[j])
        if ok == rk:
            head_compare(osorted[i], rsorted[j])
            compare_bodies(osorted[i], rsorted[j], rule, diffs, whitelisted, undecidable,
                           masked_ranges_note)
            matched += 1
            i += 1
            j += 1
        elif ok < rk:
            i += 1
        else:
            j += 1
    return matched


def ignore_rule_census(counts, shapes, rules):
    """`ignore` 规则的**每侧计数 + 形态**普查（卡 openmir2-ignore-rule-count-compare）。

    `counts` = {"oracle": {ident: n}, "rust": {...}}；`shapes` = 同形，值是
    {"param,tag,series|<body_sha256>": n}（见 `IGNORE_SHAPE_FIELDS` 与 `_ignore_shape_key`）。
    只统计**真的剔了帧**的规则
    （任一侧 hit > 0）；每条给两侧计数、差值、计数是否相等、形态是否相等，
    以及规则自身的 `pending`（`approved_by` 以 PENDING 开头）。

    判定语义（main 里用）：PENDING 规则只有在「计数相等 **且** 形态相等」时才不再
    整轮 UNDECIDABLE（记为「已计数、形态已比」）；否则仍点名差额（fail-closed）。
    非 PENDING 的 ignore 规则只做留痕，不影响判定。
    """
    rows = []
    for ident in sorted(set(counts["oracle"]) | set(counts["rust"])):
        rule = rules.by_ident.get(ident) or {}
        o_n = counts["oracle"].get(ident, 0)
        r_n = counts["rust"].get(ident, 0)
        o_s = shapes["oracle"].get(ident) or {}
        r_s = shapes["rust"].get(ident) or {}
        rows.append({
            "ident": ident,
            "action": rule.get("action"),
            "approved_by": rule.get("approved_by"),
            "pending": str(rule.get("approved_by", "")).upper().startswith("PENDING"),
            "oracle": o_n,
            "rust": r_n,
            "delta": o_n - r_n,
            "counts_equal": o_n == r_n,
            "shape_equal": o_s == r_s,
            # F7（review 后 follow-up）：形态**实际**还含 body_sha256——只报
            # param/tag/series 会让「体不同」的差额显示成「形态（param/tag/series）不同」，
            # 而这三项完全相同 ⇒ 误导排障的人。
            "shape_fields": list(IGNORE_SHAPE_FIELDS) + ["body_sha256"],
            "shapes": {"oracle": dict(sorted(o_s.items())),
                       "rust": dict(sorted(r_s.items()))},
        })
    return rows


def compare_streams(o_stream, r_stream, rules, cli_ignore_idents=frozenset()):
    diffs = []
    whitelisted = []
    undecidable = []
    missing_fields = []
    masked_ranges_note = []
    pending_ignore = set()
    # 每侧每条 ignore 规则剔了多少帧 + 这些帧的形态分布（卡 openmir2-ignore-rule-count-compare）。
    # 计数是硬判据；形态是加强项（见 IGNORE_SHAPE_FIELDS 为什么不含 recog）。
    ignore_counts = {"oracle": {}, "rust": {}}
    ignore_shapes = {"oracle": {}, "rust": {}}

    def split(side, frames):
        kept, ignored, ignored_cli = [], [], []
        for fr in frames:
            ident = fr["ident"]
            if ident in cli_ignore_idents:
                ignored_cli.append(ident)
                continue
            rule = rules.by_ident.get(ident)
            if rule and rule["action"] == "ignore":
                ignored.append(ident)
                ignore_counts[side][ident] = ignore_counts[side].get(ident, 0) + 1
                shape = _ignore_shape_key(fr)
                bucket = ignore_shapes[side].setdefault(ident, {})
                bucket[shape] = bucket.get(shape, 0) + 1
                if str(rule.get("approved_by", "")).upper().startswith("PENDING"):
                    pending_ignore.add(ident)
                continue
            kept.append(fr)
        return kept, ignored, ignored_cli

    o_kept, o_ign, o_ign_cli = split("oracle", o_stream.frames)
    r_kept, r_ign, r_ign_cli = split("rust", r_stream.frames)

    # multiset_by_tick 需要 t_ms 标注 tick 窗口：**任一侧任一帧**缺 t_ms 都判不了。
    # 必须预扫整条流，不能在块首判——缺的是运行中的后续帧时会被漏判，进而误报
    # 「多余帧」（reviewer-2 I-2）。
    for side_label, side_frames in (("oracle", o_kept), ("rust", r_kept)):
        for fr in side_frames:
            rule = rules.by_ident.get(fr["ident"])
            if not rule or rule.get("action") != "multiset_by_tick":
                continue
            if is_int(fr.get("t_ms")):
                continue
            undecidable.append({
                "source": "frame", "frame_index": fr["index"], "ident": fr["ident"],
                "kind": "multiset_tick_missing", "side": side_label, "seq": fr.get("seq"),
                "detail": "ident %d 的 multiset_by_tick 需要 t_ms 标注 tick 窗口，但 %s 侧"
                          " index=%d（seq=%r）缺 t_ms ⇒ 判不了（不按缺帧处理）"
                          % (fr["ident"], side_label, fr["index"], fr.get("seq"))})

    ignored = {"oracle": o_ign, "rust": r_ign}
    ignored_cli = {"oracle": o_ign_cli, "rust": r_ign_cli}

    # 覆盖检查：两侧出现过的 ident 都必须有规则（ignore 规则也算覆盖）。
    present = sorted({fr["ident"] for fr in o_stream.frames} |
                     {fr["ident"] for fr in r_stream.frames})
    unclassified = [i for i in present
                    if i not in rules.by_ident and i not in cli_ignore_idents]

    def head_compare(of, rf):
        ident = of["ident"]
        masks = rules.mask_for(ident)
        for field in STREAM_FIELDS:
            ov, rv = of.get(field), rf.get(field)
            mask = masks.get(field)
            # 先查缺失再走掩码：掩码只免「值不同」，不免「根本没给」（CH-4）。
            if ov is None or rv is None:
                if ov is None and rv is None:
                    missing_fields.append({"ident": ident, "field": field,
                                           "frame_index": of["index"],
                                           "masked": mask is not None,
                                           "detail": "两侧都缺该字段，无法比较（已显式登记）"})
                else:
                    undecidable.append({"source": "frame", "frame_index": of["index"],
                                        "ident": ident, "kind": "missing_field", "field": field,
                                        "masked": mask is not None,
                                        "oracle_present": ov is not None,
                                        "rust_present": rv is not None,
                                        "detail": "只有一侧给了该字段，无法比较"})
                continue
            if mask is not None:
                if ov != rv:
                    whitelisted.append({"source": "frame", "frame_index": of["index"],
                                        "ident": ident, "field": field,
                                        "kind": "masked_head", "oracle": ov, "rust": rv,
                                        "reason": mask["reason"], "tier": mask["tier"],
                                        "approved_by": mask["approved_by"],
                                        "origin": mask["origin"]})
                continue
            if ov != rv:
                diffs.append({"source": "frame", "frame_index": of["index"], "ident": ident,
                              "field": field, "kind": "head",
                              "oracle": ov, "rust": rv,
                              "detail": "头部字段不同"})

    o_blocks = _build_blocks(o_kept, rules)
    r_blocks = _build_blocks(r_kept, rules)
    masked_order = []
    compared_frames = 0
    n = min(len(o_blocks), len(r_blocks))
    for i in range(n):
        ob, rb = o_blocks[i], r_blocks[i]
        if (ob["kind"] == "multiset" and rb["kind"] == "multiset"
                and ob["ident"] == rb["ident"]):
            compared_frames += _compare_multiset_block(ob, rb, head_compare, diffs,
                                                       whitelisted, undecidable,
                                                       masked_ranges_note, masked_order)
            continue
        if ob["kind"] == "single" and rb["kind"] == "single":
            of, rf = ob["frames"][0], rb["frames"][0]
            if of["ident"] != rf["ident"]:
                diffs.append({"source": "frame", "frame_index": of["index"],
                              "ident": of["ident"],
                              "field": "ident", "kind": "ident",
                              "oracle": of["ident"], "rust": rf["ident"],
                              "detail": "按序对齐后 ident 不同（帧错位/一侧多帧）"})
                continue
            head_compare(of, rf)
            compare_bodies(of, rf, rules.by_ident.get(of["ident"]),
                           diffs, whitelisted, undecidable, masked_ranges_note)
            compared_frames += 1
            continue
        diffs.append({"source": "frame", "frame_index": ob["frames"][0]["index"],
                      "ident": ob["ident"], "field": "ident", "kind": "block_ident",
                      "oracle": ("multiset:%d" % ob["ident"])
                                if ob["kind"] == "multiset" else ob["ident"],
                      "rust": ("multiset:%d" % rb["ident"])
                              if rb["kind"] == "multiset" else rb["ident"],
                      "detail": "对齐块不一致（一侧是 multiset 块、另一侧不是，或 ident 不同）"})

    if len(o_blocks) != len(r_blocks):
        tail_blocks = o_blocks[n:] if len(o_blocks) > n else r_blocks[n:]
        tail = [fr for b in tail_blocks for fr in b["frames"]]
        # reviewer-2 I-3①：这里比的是**块数**，绝不再拿块数不等的条件输出
        # 「帧数 X vs Y 不等」（两侧帧数相同却报缺帧是自相矛盾）。帧数只作为事实附注。
        if len(o_kept) != len(r_kept):
            extra = "；两侧帧数 oracle=%d rust=%d" % (len(o_kept), len(r_kept))
        else:
            extra = ("；两侧帧数相同（oracle=%d），差异只在块边界/交错，不据此报缺帧"
                     % len(o_kept))
        diffs.append({"source": "frame", "frame_index": n, "ident": None,
                      "field": "block_count", "kind": "block_align",
                      "oracle_blocks": len(o_blocks), "rust_blocks": len(r_blocks),
                      "oracle_frames": len(o_kept), "rust_frames": len(r_kept),
                      "detail": "对齐块数不同（块 = ident 连续段，multiset 块可含多帧）；"
                                "尾部多出的 ident：%s%s"
                                % ([fr["ident"] for fr in tail[:10]], extra)})

    # F3（reviewer-20）：单侧命中时另一侧也必须**有键**（补 0），
    # 否则读 `ignored_counts["rust"]["658"]` 会 KeyError。census 里两侧计数也一并对齐。
    for _ident in sorted(set(ignore_counts["oracle"]) | set(ignore_counts["rust"])):
        for _side in ("oracle", "rust"):
            ignore_counts[_side].setdefault(_ident, 0)
            ignore_shapes[_side].setdefault(_ident, {})
    census = ignore_rule_census(ignore_counts, ignore_shapes, rules)
    return {
        "diffs": diffs,
        "whitelisted": whitelisted,
        "undecidable": undecidable,
        "missing_fields": missing_fields,
        "ignored": ignored,
        "ignored_cli": ignored_cli,
        "ignored_counts": ignore_counts,
        "ignored_shapes": ignore_shapes,
        "ignored_rule_census": census,
        "pending_ignore_hits": sorted(pending_ignore),
        "pending_ignore_counted": sorted(
            row["ident"] for row in census
            if row["pending"] and row["counts_equal"] and row["shape_equal"]),
        "pending_ignore_mismatch": sorted(
            row["ident"] for row in census
            if row["pending"] and not (row["counts_equal"] and row["shape_equal"])),
        "kept_frames": {"oracle": len(o_kept), "rust": len(r_kept)},
        "present_idents": present,
        "unclassified": unclassified,
        "compared_frames": compared_frames,
        "masked_ranges": masked_ranges_note,
        "masked_order": masked_order,
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


def _side_run_clock(o_dir, r_dir):
    """两侧 `run.json` 的运行窗口 → {"oracle": (start_epoch, end_epoch), ...}（缺失/非法则不收）。

    **字段名以 probe 实际产物为准**：`start_wall` / `end_wall`（ISO 带 `+0800`）——`started_at`/
    `ended_at` 是 selftest 合成 run.json 与旧口径的字段名，这里作为退路保留（review 实测四份
    真机 run.json 都只有 `*_wall`）。

    返回**窗口**而不是一个时刻：长场景（例如带 620s sleep 的场景）允许在运行途中写时间列，
    「距结束 ≤ 600s」会把合法值判成 stale。
    """
    out = {}
    for side, d in (("oracle", o_dir), ("rust", r_dir)):
        path = os.path.join(d, "run.json")
        if not os.path.exists(path):
            continue
        try:
            meta = read_json(path, "run.json")
        except (InputError, Undecidable):
            continue
        if not isinstance(meta, dict):
            continue

        def epoch(*names):
            for name in names:
                raw = meta.get(name)
                if isinstance(raw, str) and raw.strip():
                    try:
                        return datetime.datetime.fromisoformat(raw.strip()).timestamp()
                    except ValueError:
                        continue
            return None

        start = epoch("start_wall", "started_at")
        end = epoch("end_wall", "ended_at")
        if start is None or end is None:
            continue
        if end < start:
            start, end = end, start
        out[side] = (start, end)
    return out


def _side_char_name(o_dir, r_dir):
    """两侧 `run.json` argv 里的 `--char-name` → {"oracle": name, "rust": name}（缺失则不收）。

    见证限定到**本角色行**：`max(时间列)` 落在窗口内，也可能来自别人的写入（F1-A/C）——
    用本次运行实际登录的角色名收窄范围，才是「本次运行有没有写这一行」的等价判据。
    """
    out = {}
    for side, d in (("oracle", o_dir), ("rust", r_dir)):
        path = os.path.join(d, "run.json")
        if not os.path.exists(path):
            continue
        try:
            meta = read_json(path, "run.json")
        except (InputError, Undecidable):
            continue
        argv = meta.get("argv") if isinstance(meta, dict) else None
        if not isinstance(argv, list):
            continue
        for i, item in enumerate(argv):
            if item in ("--char-name", "--char_name") and i + 1 < len(argv):
                name = argv[i + 1]
                if isinstance(name, str) and name.strip():
                    out[side] = name.strip()
                break
    return out


def compare_db(o_dir, r_dir, db_opts, tmp_root, overrides, require_db=False):
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
        if require_db:
            info["note"] += "；--require-db 要求库后态 ⇒ 无法判定"
            info["undecidable"].append({
                "source": "db", "kind": "db_snapshot_absent",
                "detail": "两侧都没有库快照，而调用方要求 --require-db"})
            return info, info["undecidable"]
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
            allow_unknown_table=bool(db_opts["allow_unknown_table"]),
            masked_columns=db_opts.get("masked_columns"),
            time_witness=db_opts.get("time_columns"),
            time_ref=_side_run_clock(o_dir, r_dir),
            time_scope=_side_char_name(o_dir, r_dir),
            time_tolerance_s=db_opts.get("time_tolerance_s", 600),
            time_lead_s=db_opts.get("time_lead_s", 30), quiet=True)
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
        if rec.get("kind") == "masked_column_applied":
            # 掩码必须**显式**出现在报告的 whitelisted 里（卡 openmir2-db-makeindex-mask ①）。
            info.setdefault("masked_columns", []).append({
                "table": rec.get("table"), "column": rec.get("column"),
                "reason": rec.get("reason"), "ref": rec.get("ref"),
                "checks": rec.get("checks"), "group_by": rec.get("group_by"),
                "skipped": rec.get("skipped") or []})
            continue
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
        "masked_order": [],
        "db": None,
        "verdict": "UNDECIDABLE",
        "exit_code": 2,
    }


def _parse_ident_list(groups, flag):
    allowed = set()
    for group in (groups or []):
        for tok in group.split(","):
            tok = tok.strip()
            if not tok:
                continue
            if not tok.isdigit():
                raise InputError("%s 只接受逗号分隔的数字 ident，实际 %r" % (flag, tok))
            if int(tok) > 65535:
                raise InputError("%s 的 ident 必须在 0..65535，实际 %r" % (flag, tok))
            allowed.add(int(tok))
    return allowed


def _parse_allow_unclassified(groups):
    return _parse_ident_list(groups, "--allow-unclassified")


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

    try:
        allowed_unclassified = _parse_allow_unclassified(args.allow_unclassified)
        cli_ignore_idents = _parse_ident_list(args.ignore_idents, "--ignore-idents")
    except InputError as e:
        report["undecidable"].append({"source": "input", "kind": "bad_flag", "detail": str(e)})
        report["verdict"] = "UNDECIDABLE"
        report["exit_code"] = 2
        return report
    report["options"] = {
        "allow_unclassified": sorted(allowed_unclassified),
        "ignore_idents": sorted(cli_ignore_idents),
        "allow_empty": bool(args.allow_empty),
        "allow_missing_fields": bool(args.allow_missing_fields),
        "require_db": bool(args.require_db),
    }
    cmp = compare_streams(o_stream, r_stream, rules, cli_ignore_idents)
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
    for ident in sorted(set(cmp["ignored_cli"]["oracle"])
                        | set(cmp["ignored_cli"]["rust"])):
        cmp["whitelisted"].append({"source": "coverage", "ident": ident, "field": "ident",
                                   "kind": "cli_ignore",
                                   "reason": "调用方用 --ignore-idents 显式剔除该消息号（不参与对齐）",
                                   "tier": "R", "approved_by": "CLI --ignore-idents"})

    report["frames"] = {
        "oracle_total": len(o_stream.frames),
        "rust_total": len(r_stream.frames),
        "compared": cmp["compared_frames"],
        "filtered_control_frames": {
            "oracle": {"count": len(o_stream.filtered_control),
                       "frames": o_stream.filtered_control},
            "rust": {"count": len(r_stream.filtered_control),
                     "frames": r_stream.filtered_control},
        },
        "ignored": {k: {"count": len(v), "idents": v} for k, v in cmp["ignored"].items()},
        "ignored_cli": {k: {"count": len(v), "idents": v}
                        for k, v in cmp["ignored_cli"].items()},
        "ignored_ident_rules": sorted(i for i, r in rules.by_ident.items()
                                      if r["action"] == "ignore"),
        "ignored_ident_rules_pending": sorted(
            i for i, r in rules.by_ident.items()
            if r["action"] == "ignore"
            and str(r.get("approved_by", "")).upper().startswith("PENDING")),
        # 卡 openmir2-ignore-rule-count-compare：ignore 不能只留「剔了几帧」的总数，
        # 必须给**每侧每条规则**的计数与形态，并明确哪些 PENDING 规则已计数放行。
        "ignored_counts": {side: {str(i): n for i, n in sorted(v.items())}
                           for side, v in cmp["ignored_counts"].items()},
        "ignored_rule_census": cmp["ignored_rule_census"],
        "ignored_ident_rules_counted": cmp["pending_ignore_counted"],
        "ignored_ident_rules_mismatch": cmp["pending_ignore_mismatch"],
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
    report["masked_order"] = cmp["masked_order"]
    undecidable.extend(cmp["undecidable"])
    for p in o_stream.problems + r_stream.problems:
        undecidable.append({"source": "input", **p})
    report["undecidable"] = undecidable

    # CH-1：两侧都没比到帧（空运行）不是通过。
    if cmp["kept_frames"]["oracle"] == 0 and cmp["kept_frames"]["rust"] == 0:
        if args.allow_empty:
            report["whitelisted"].append({
                "source": "coverage", "field": "frames", "kind": "allowed_empty_run",
                "reason": "两侧可比较帧数都是 0，调用方用 --allow-empty 显式放行",
                "tier": "R", "approved_by": "CLI --allow-empty"})
        else:
            undecidable.append({"source": "coverage", "kind": "empty_run",
                                "detail": "两侧可比较帧数都是 0（ignore 剔除后）：没有任何帧被比过，"
                                          "不能判绿（确需空跑用 --allow-empty）"})

    # CH-3：两侧都缺同一头部字段，默认判不了。
    if report["missing_fields"]:
        agg = sorted({(m["ident"], m["field"]) for m in report["missing_fields"]})
        if args.allow_missing_fields:
            report["whitelisted"].append({
                "source": "coverage", "field": "missing_fields",
                "kind": "allowed_missing_fields",
                "idents_fields": [list(x) for x in agg],
                "reason": "两侧都缺同一头部字段，调用方用 --allow-missing-fields 显式放行（未比较）",
                "tier": "R", "approved_by": "CLI --allow-missing-fields"})
        else:
            undecidable.append({"source": "coverage", "kind": "missing_field_both_sides",
                                "idents_fields": [list(x) for x in agg],
                                "count": len(report["missing_fields"]),
                                "detail": "两侧都缺同一头部字段 ⇒ 该字段未比较，默认判不了"
                                          "（确需放行用 --allow-missing-fields）"})

    # CH-2：PENDING 的 ignore 规则真的剔了帧 ⇒ 不许直接判绿。
    # 卡 openmir2-ignore-rule-count-compare：保护**不从「整轮 UNDECIDABLE」减弱成「取消」**，
    # 而是升级判据 —— 只有「两侧计数相等 **且** 形态相等」才算「已计数、形态已比」放行；
    # 计数或形态对不上仍整轮 UNDECIDABLE，并点名差多少（fail-closed）。
    for row in cmp["ignored_rule_census"]:
        if not row["pending"]:
            continue
        if row["counts_equal"] and row["shape_equal"]:
            continue
        undecidable.append({
            "source": "coverage", "kind": "pending_ignore_rule", "ident": row["ident"],
            "oracle": row["oracle"], "rust": row["rust"], "delta": row["delta"],
            "counts_equal": row["counts_equal"], "shape_equal": row["shape_equal"],
            "shape_fields": row["shape_fields"], "shapes": row["shapes"],
            "detail": ("ident %d 的 ignore 规则 approved_by 仍为 PENDING，且本次剔除"
                       "两侧对不上：oracle=%d / rust=%d（差 %+d），形态（%s）%s ⇒ 判不了"
                       "（先复核该规则，或用显式 --ignore-idents 承担剔除）"
                       % (row["ident"], row["oracle"], row["rust"], row["delta"],
                          "/".join(row["shape_fields"]),
                          "相同" if row["shape_equal"] else "不同"))})

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
                                     {"oracle": args.db_oracle, "rust": args.db_rust},
                                     require_db=args.require_db)
        report["db"] = db_info
        report["diffs"].extend(db_info.get("diffs") or [])
        report["undecidable"].extend(db_und)
        # 掩码列必须**显式**进 whitelisted（卡 openmir2-db-makeindex-mask ①：不许静默丢弃）。
        for m in (db_info.get("masked_columns") or []):
            report["whitelisted"].append({
                "source": "db", "field": m["column"], "table": m["table"],
                "kind": "db_column_masked", "reason": m["reason"], "ref": m["ref"],
                "checks": m["checks"], "group_by": m.get("group_by"),
                "skipped": m.get("skipped") or [], "tier": "L", "approved_by": m["ref"]})
    except (InputError, Undecidable) as e:
        report["db"] = {"state": "error", "detail": str(e), "diffs": [], "undecidable": []}
        report["undecidable"].append({"source": "db", "kind": "db_input_error",
                                      "detail": str(e)})
    finally:
        shutil.rmtree(tmp_root, ignore_errors=True)

    # 卡 `openmir2-rules-pending-adjudication` ④（Leader 裁定 2026-10-25）：
    #   * `rules_pending_review` = **保守主标志**：规则文件里只要还有任何 PENDING
    #     （global/rules/db 任一，含本卡保留的 ident 10/14/50）就是 true。读者据此判断
    #     「这份报告的判据是否已全部审过」；宁可恒真，也不让读者误以为全审过。
    #   * `pending_review_used` = 附属证据：本次**实际涉及**的 PENDING 项
    #     （global 恒计入；db 仅在本次比了库时计入；ident 规则与两侧出现过的 ident 求交集）。
    #   * `pending_review` = 全量清单（审计用）。
    report["rules"]["pending_review_used"] = []
    if rules.pending_review:
        seen = {fr.get("ident") for fr in (o_stream.frames + r_stream.frames)}
        used = []
        for p in rules.pending_review:
            if str(p).startswith("global"):
                used.append(p)
            elif str(p) == "db":
                if report.get("db"):
                    used.append(p)
            else:
                mm = re.search(r"ident=\[([0-9, ]*)\]", str(p))
                if mm:
                    ids = {int(x) for x in mm.group(1).split(",") if x.strip().isdigit()}
                    if ids & seen:
                        used.append(p)
        # **保守主标志**（Leader 裁定 2026-10-25）：只要规则文件里还有 PENDING，就是
        # true——读者据此判断「这份报告的判据是否全部审过」。按需的「本次影响面」降级为
        # 附属证据 `pending_review_used`，**不能**替代主标志（否则一份 20 条 UNDECIDABLE
        # 的报告可能因为没碰到 10/14/50 而显示 false，误导读者）。
        report["rules"]["pending_review_used"] = used

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
        igc = fr.get("ignored_cli") or {}
        if any(v["count"] for v in igc.values()):
            lines.append("ignored(CLI): " + ", ".join(
                "%s=%d" % (k, v["count"]) for k, v in sorted(igc.items())))
        census = fr.get("ignored_rule_census") or []
        if census:
            # 卡 openmir2-ignore-rule-count-compare：每条 ignore 规则一行，两侧同量同形
            # 才算 counted；否则在 undecidable 里点名差额（这里也先给一眼可见的行）。
            parts = []
            for row in census:
                if row["counts_equal"] and row["shape_equal"]:
                    verdict = "counted" + ("(pending)" if row["pending"] else "")
                else:
                    verdict = "MISMATCH"
                parts.append("ident=%d oracle=%d rust=%d delta=%+d shape=%s => %s"
                             % (row["ident"], row["oracle"], row["rust"], row["delta"],
                                "same" if row["shape_equal"] else "diff", verdict))
            lines.append("ignore 规则普查（每侧计数/形态）: " + "; ".join(parts))
        fcf = fr.get("filtered_control_frames") or {}
        if any(v["count"] for v in fcf.values()):
            lines.append("filtered(控制帧 ident=null, 见 filtered_control_frames): "
                         + ", ".join("%s=%d" % (k, v["count"])
                                     for k, v in sorted(fcf.items())))
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
    mo = report.get("masked_order") or []
    if mo:
        lines.append("masked_order: %d 组（同 ident 连续段多重集：顺序不同、集合相同，已登记）"
                      % len(mo))
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
    p.add_argument("--ignore-idents", action="append", metavar="IDENTS",
                   help="显式剔除的消息号（逗号分隔，可重复）；默认规则表没有 ignore")
    p.add_argument("--allow-empty", action="store_true",
                   help="两侧可比较帧数都是 0 时仍按规则判定（默认 UNDECIDABLE empty_run）")
    p.add_argument("--allow-missing-fields", action="store_true",
                   help="两侧都缺同一头部字段时只登记不判死（默认 UNDECIDABLE）")
    p.add_argument("--require-db", action="store_true",
                   help="要求两侧都有库快照；absent_both 时判 UNDECIDABLE")
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
