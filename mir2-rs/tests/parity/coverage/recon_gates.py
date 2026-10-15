#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""recon_gates.py — OpenMir2→Rust 两道常驻对账门（完成定义 §4-1 / §4-2）。

卡：`openmir2-coverage-recon-gates`（第 0 层最优先）。
依据：`docs/移植计划重审-20261015.md` §1（红名单）/ §4-1（内容对账门）/ §4-2（派发对账门）。

子命令
------
  content   内容加载对账门：oracle `GameApp.cs` 全部 `Load*` 调用点 × 部署 Envir 文件 × Rust 实现（三方对照表）。
  dispatch  协议派发对账门：oracle `PlayObject.Message.cs` case 全集 vs `bins/game-svr` 派发臂（双向：
            入站 `CM_*` 派发 + 出站 `RM_*→SM_*` 编码器）。
  selftest  内置夹具自测（含「构造缺口必红」牙）。
  mutate    变异实测：对真实输入做三处自设计变异（注入缺口 / 撤卡 / 空语料），门必须转红。

判红口径（三者缺一即不红）
--------------------------
  content : oracle 有该 Load 调用 ∧ 部署有该内容文件 ∧ Rust 无实现 ∧ 无卡  ⇒ RED
  dispatch: oracle 有 case ∧ Rust 无派发臂/编码器 ∧ 无卡                  ⇒ RED

「无卡」= `coverage/cards.json` 的 `assignments` 未给该缺口指派一个存在于 `known_cards` 的卡 id。
「Rust 实现」= 在 Rust 语料（去注释/去字符串字面量后）里出现该标识符/文件名字面量。

退出码：0 = 无 RED（缺口全部「已实现或已卡」）；1 = 存在 RED；2 = 用法/环境错误或前置缺失
（PREREQ_RED：部署目录不存在/不可读/不是目录 ⇒ 印 `PREREQ_MISSING` + `RESULT: PREREQ_RED`，**拒跑而不
静默放行**；判不了缺口时绝不报「无缺口」假绿。诊断需要看表可加 `--allow-missing-deploy` 显式降级为 WARN，
但此时结论不可作数）。
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

BS = chr(92)   # backslash
NL = chr(10)   # newline
QT = chr(34)   # double quote

SCRIPT_DIR = Path(__file__).resolve().parent
# <repo>/mir2-rs/tests/parity/coverage/recon_gates.py → SCRIPT_DIR.parents[3] = <repo>
REPO_DEFAULT = SCRIPT_DIR.parents[3]

DEFAULT_ENVIR_DIR = Path("E:/MirServer/M2GameSvr/Envir")
DEFAULT_NOTICE_DIR = Path("E:/MirServer/M2GameSvr/Notice")
DEFAULT_CARDS = SCRIPT_DIR / "cards.json"

ORACLE_CONTENT = "src/GameSrv/GameApp.cs"
ORACLE_DISPATCH = "src/M2Server/Player/PlayObject.Message.cs"


def rust_source_dirs(repo: Path, exclude=()):
    """工作区全部 `*/src` 目录（crates/* 与 bins/*）。exclude 为后缀匹配的排除项。"""
    dirs = []
    for base in ("mir2-rs/crates", "mir2-rs/bins"):
        b = repo / base
        if not b.is_dir():
            continue
        for sub in sorted(b.iterdir()):
            s = sub / "src"
            if s.is_dir():
                rel = f"{base}/{sub.name}/src"
                if rel in exclude:
                    continue
                dirs.append(str(s))
    return dirs


# 派发门：常量定义在 crates/protocol/src/messages.rs（若计入会假阳），故排除之。
DISPATCH_EXCLUDE = ("mir2-rs/crates/protocol/src",)

# ---------------------------------------------------------------------------
# 内容加载映射表：oracle 方法名 → 内容件。
# kind: envir(Envir/下文件) | notice(Notice/下文件) | dir(子目录) | db(库表) | other(非文件)
# anchor: oracle 源锚（文件:行），随 oracle 漂移时同步更新。
# card: 该缺口在红名单卡（可被 cards.json 覆盖）。
# ---------------------------------------------------------------------------
CONTENT_MAP = {
    "LoadGameLogItemNameList":   dict(kind="envir", file="GameLogItemNameList.txt", anchor="M2Share.cs:1512", card="openmir2-envir-redlist-batch"),
    "LoadDenyIPAddrList":        dict(kind="envir", file="DenyIPAddrList.txt", anchor="M2Share.cs:1564"),
    "LoadDenyAccountList":       dict(kind="envir", file="DenyAccountList.txt", anchor="M2Share.cs:1689"),
    "LoadDenyChrNameList":       dict(kind="envir", file="DenyChrNameList.txt", anchor="M2Share.cs:1626", card="openmir2-envir-redlist-batch"),
    "LoadNoClearMonList":        dict(kind="envir", file="NoClearMonList.txt", anchor="M2Share.cs:1746"),
    "LoadItemsDB":               dict(kind="db", anchor="MySqlDB.LoadItemsDB"),
    "LoadMinMap":                dict(kind="envir", file="MiniMap.txt", anchor="Maps.cs:387-414", card="openmir2-envir-redlist-batch"),
    "LoadMapInfo":               dict(kind="envir", file="MapInfo.txt", anchor="Maps.cs:16"),
    "LoadMonsterDB":             dict(kind="db", anchor="MySqlDB.LoadMonsterDB"),
    "LoadMagicDB":               dict(kind="db", anchor="MySqlDB.LoadMagicDB"),
    "LoadMonGen":                dict(kind="envir", file="MonGen.txt", anchor="LocalDB.cs:501-508"),
    "LoadMonSayMsg":             dict(kind="envir", file="GenMsg.txt", anchor="M2Share.cs:1826"),
    "LoadDisableTakeOffList":    dict(kind="envir", file="DisableTakeOffList.txt", anchor="M2Share.cs:1434"),
    "LoadMonDropLimitList":      dict(kind="envir", file="MonDropLimitList.txt", anchor="M2Share.cs:1378"),
    "LoadDisableMakeItem":       dict(kind="envir", file="DisableMakeItem.txt", anchor="M2Share.cs:1145"),
    "LoadEnableMakeItem":        dict(kind="envir", file="EnableMakeItem.txt", anchor="M2Share.cs:1244"),
    "LoadAllowSellOffItem":      dict(kind="envir", file="DisableSellOffItem.txt", anchor="M2Share.cs:1307"),
    "LoadDisableMoveMap":        dict(kind="envir", file="DisableMoveMap.txt", anchor="M2Share.cs:1278"),
    "LoadCustomItemName":        dict(kind="envir", file="ItemNameList.txt", anchor="CustomItemSystem.cs:85", card="openmir2-envir-redlist-batch"),
    "LoadDisableSendMsgList":    dict(kind="envir", file="DisableSendMsgList.txt", anchor="M2Share.cs:1357", card="openmir2-envir-redlist-batch"),
    "LoadItemBindIPaddr":        dict(kind="envir", file="ItemBindIPaddr.txt", anchor="M2Share.cs:943"),
    "LoadItemBindAccount":       dict(kind="envir", file="ItemBindAccount.txt", anchor="M2Share.cs:1009"),
    "LoadItemBindChrName":       dict(kind="envir", file="ItemBindChrName.txt", anchor="M2Share.cs:1077"),
    "LoadUnMasterList":          dict(kind="envir", file="UnMaster.txt", anchor="M2Share.cs:1180"),
    "LoadUnForceMasterList":     dict(kind="envir", file="UnForceMaster.txt", anchor="M2Share.cs:1215"),
    "LoadUnbindList":            dict(kind="envir", file="UnbindList.txt", anchor="LocalDB.cs:875-884", card="openmir2-envir-redlist-batch"),
    "LoadSellOffItemList":       dict(kind="db", anchor="MySqlDB.LoadSellOffItemList"),
    "LoadMapQuest":              dict(kind="envir", file="MapQuest.txt", anchor="LocalDB.cs:341-352"),
    "LoadQuestDiary":            dict(kind="dir", file="QuestDiary", anchor="LocalDB.cs:733-744"),
    "LoadLineNotice":            dict(kind="notice", file="LineNotice.txt", anchor="GameApp.cs:204", card="openmir2-envir-redlist-batch"),
    "LoadAdminList":             dict(kind="envir", file="AdminList.txt", anchor="LocalDB.cs:16-28", card="openmir2-envir-redlist-batch"),
    "LoadMapDoor":               dict(kind="other", anchor="MapManager.cs:182（随地图文件，非独立内容件）"),
    "LoadMerchant":              dict(kind="envir", file="Merchant.txt", anchor="LocalDB.cs:412-424"),
    "LoadNpcs":                  dict(kind="envir", file="Npcs.txt", anchor="LocalDB.cs:655-664", card="openmir2-npcs-fixed-loading"),
    "LoadMakeItem":              dict(kind="envir", file="MakeItem.txt", anchor="LocalDB.cs:149-158", card="openmir2-envir-redlist-batch"),
    "LoadStartPoint":            dict(kind="envir", file="StartPoint.txt", anchor="LocalDB.cs:822-832"),
    "LoadGuardList":             dict(kind="envir", file="GuardList.txt", anchor="LocalDB.cs:100-109"),
    "LoadGuildInfo":             dict(kind="db", anchor="GuildMgr.LoadGuildInfo"),
    "LoadCastleList":            dict(kind="other", anchor="CastleMgr.LoadCastleList（Envir/Castle 段，另行对账）"),
}

# GameApp.cs 里非内容加载的同名/近似调用（helper），从发现集中剔除以去噪。
CONTENT_CALL_DENYLIST = {"LoadFromFile"}

# 未被 `Load*` 调用点覆盖、但 oracle 确实加载的显式声明行（随红名单维护）。
DECLARED_CONTENT = [
    dict(symbol="RobotManage.LoadRobot", kind="envir", file="Robot.txt",
         anchor="RobotManage.cs:25（GameApp.cs:58 `new RobotManage()` 触发）",
         card="openmir2-envir-redlist-batch", declared=True),
]

# ---------------------------------------------------------------------------
# 通用：I/O
# ---------------------------------------------------------------------------
def read_text(path: Path) -> str:
    """读文本，utf-8 优先，失败回落 gb2312（线上多为 GBK）。"""
    raw = path.read_bytes()
    for enc in ("utf-8", "gb2312", "latin-1"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", "replace")


def strip_code(text: str, keep_strings: bool = False) -> str:
    """去 Rust/C 风格注释与字符串/字符字面量；**保留换行**（行号不漂移）。

    keep_strings=True 时保留字符串字面量原文、只去注释——用于探测「文件名字面量」（它本身在
    字符串里），而标识符探测（CM_*/SM_*）应保持 keep_strings=False 以剔除「注释式」字符串提及。
    """
    out = []
    i, n = 0, len(text)
    while i < n:
        c = text[i]
        two = text[i + 1] if i + 1 < n else ""
        if c == "/" and two == "/":
            j = text.find(NL, i)
            i = n if j == -1 else j  # 保留换行给下一轮
        elif c == "/" and two == "*":
            j = text.find("*/", i + 2)
            if j == -1:
                j = n
            seg = text[i:j + 2]
            out.append(NL * seg.count(NL))
            i = j + 2
        elif c == QT:
            start = i
            i += 1
            while i < n:
                if text[i] == BS:
                    if i + 1 < n and text[i + 1] == NL:
                        out.append(NL)
                    i += 2
                    continue
                if text[i] == QT:
                    i += 1
                    break
                if text[i] == NL:
                    out.append(NL)
                i += 1
            out.append(text[start:i] if keep_strings else (QT + QT))
        elif c == "'":
            # 字符字面量（'a' / '\n'）或生命周期（'a）；只吞闭合很快的。
            if i + 2 < n and (text[i + 2] == "'" or text[i + 1] == BS):
                start = i
                i += 1
                while i < n:
                    if text[i] == BS:
                        if i + 1 < n and text[i + 1] == NL:
                            out.append(NL)
                        i += 2
                        continue
                    if text[i] == "'":
                        i += 1
                        break
                    if text[i] == NL:
                        out.append(NL)
                    i += 1
                out.append(text[start:i] if keep_strings else "'x'")
            else:
                out.append(c)
                i += 1
        else:
            out.append(c)
            i += 1
    return "".join(out)


class RustCorpus:
    """一个 Rust 源码语料，支持两类存在性查询：

    - `present(ident)`：标识符整词存在（在**去字符串**的代码里查）——用于 CM_/SM_ 派发臂。
    - `present_literal(lit)`：字符串字面量存在（在**保留字符串、去注释**的代码里查）——用于文件名。
    """

    def __init__(self, dirs):
        self.code = []   # (path, lineno, code_line)  去注释+去字符串
        self.text = []   # (path, lineno, text_line)  去注释+保留字符串
        for d in dirs:
            root = Path(d)
            if not root.is_dir():
                continue
            for p in sorted(root.rglob("*.rs")):
                raw = read_text(p)
                for no, code in enumerate(strip_code(raw).split(NL), 1):
                    if code.strip():
                        self.code.append((str(p), no, code))
                for no, txt in enumerate(strip_code(raw, keep_strings=True).split(NL), 1):
                    if txt.strip():
                        self.text.append((str(p), no, txt))

    def find(self, ident: str):
        """返回第一个含 `ident`（整词）的 (path, lineno)，无则 None。"""
        pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(ident) + r"(?![A-Za-z0-9_])")
        for path, no, code in self.code:
            if pat.search(code):
                return (path, no)
        return None

    def present(self, ident: str) -> bool:
        return self.find(ident) is not None

    def present_literal(self, lit: str) -> bool:
        """字符串字面量 `lit`（含引号）在（去注释、保留字符串的）代码里出现。"""
        needle = QT + lit + QT
        return any(needle in txt for _, _, txt in self.text)


# ---------------------------------------------------------------------------
# cards.json
# ---------------------------------------------------------------------------
def load_cards(path: Path) -> dict:
    if not path.is_file():
        return {"known_cards": [], "assignments": {}}
    data = json.loads(read_text(path))
    data.setdefault("known_cards", [])
    data.setdefault("assignments", {})
    return data


def card_for(cards: dict, key: str):
    """缺口 key 指向的卡 id（须存在于 known_cards），否则 None。"""
    cid = cards.get("assignments", {}).get(key)
    if cid and cid in set(cards.get("known_cards", [])):
        return cid
    return None


# ---------------------------------------------------------------------------
# 内容加载门
# ---------------------------------------------------------------------------
RE_CONTENT_CALL = re.compile(r"([A-Za-z_][A-Za-z0-9_.]*)\.(Load[A-Za-z0-9_]+)\s*\(")


def parse_gameapp_calls(text: str):
    """返回 [(line, method, qualifier)]：GameApp.cs 里的 `X.LoadY(` 调用点。"""
    calls = []
    for no, line in enumerate(text.split(NL), 1):
        s = line.strip()
        if s.startswith("//") or s.startswith("///"):
            continue
        m = RE_CONTENT_CALL.search(line)
        if m:
            calls.append((no, m.group(2), m.group(1)))
    return calls


def find_deployed(deployed_dir: Path, name: str):
    """大小写不敏感地在部署目录里找文件/子目录（Windows 语义），返回 Path 或 None。"""
    if not deployed_dir.is_dir():
        return None
    low = name.lower()
    for entry in deployed_dir.iterdir():
        if entry.name.lower() == low:
            return entry
    return None


def deployed_is_empty(path) -> bool:
    """空文件 / 空目录 = 空加载（oracle `File.Exists` 门后读 0 行 ⇒ 行为无差）。"""
    if path is None:
        return False
    try:
        if path.is_dir():
            return not any(path.iterdir())
        return path.stat().st_size == 0
    except OSError:
        return False


def deploy_prereq_problems(envir_dir, notice_dir):
    """部署目录前置检查（fail-closed）：目录须**存在、是目录、可枚举**。

    对抗验证 v5 F1（高·静默假绿 fail-open）：部署目录缺失时 `find_deployed` 对每件都返回 None
    ⇒ 全表 `n/a-deployed`（非红）⇒ 门报「无缺口」且退出 0。故把「判不了」与「判为无缺口」分开：
    返回问题清单（空 = 通过），由调用方决定 PREREQ_RED 还是（显式 `--allow-missing-deploy` 时）WARN。
    """
    problems = []
    for label, d in (("Envir", envir_dir), ("Notice", notice_dir)):
        p = Path(d)
        try:
            if not p.exists():
                problems.append((label, str(p), "目录不存在"))
            elif not p.is_dir():
                problems.append((label, str(p), "不是目录"))
            else:
                next(p.iterdir(), None)   # 可枚举性探针（不可读 ⇒ PermissionError/OSError）
        except OSError as exc:
            problems.append((label, str(p), "目录不可读（%s: %s）" % (type(exc).__name__, exc)))
    return problems


def emit_deploy_prereq(problems, allow_missing):
    """按前置问题印 PREREQ_MISSING / WARN，返回退出码：None = 未触发 PREREQ（可继续）；2 = 拒跑。

    `allow_missing=True`（`--allow-missing-deploy`，仅诊断用）⇒ 降级为 WARN 放行，返回 None。
    """
    if not problems:
        return None
    if allow_missing:
        for label, path, reason in problems:
            print("WARN 部署%s目录 %s —— %s；因 --allow-missing-deploy 降级放行（仅诊断，缺口结论不可作数）"
                  % (label, path, reason))
        return None
    for label, path, reason in problems:
        print("PREREQ_MISSING: 部署%s目录 %s —— %s（判不了缺口，fail-closed 拒跑；诊断可用 --allow-missing-deploy）"
              % (label, path, reason))
    print("RESULT: PREREQ_RED")
    return 2


def content_row_status(spec: dict, deployed: bool, rust: bool, cards: dict, empty: bool = False) -> str:
    kind = spec.get("kind")
    if kind not in ("envir", "notice", "dir"):
        return "n/a-" + kind
    if rust:
        return "implemented"
    key = "content:" + spec["file"]
    if card_for(cards, key):
        return "tracked"
    if not deployed:
        return "n/a-deployed"
    if empty:
        # 部署件为空 ⇒ 加载 0 行，行为与「不加载」无差（对齐 §1 备注：空文件本部署无差）。
        return "n/a-empty"
    return "RED"


def build_content_rows(repo: Path, envir_dir: Path, notice_dir: Path, cards: dict):
    """返回 (rows, unmapped, corpus_present)。rows 为 dict 列表。"""
    oracle = repo / ORACLE_CONTENT
    corpus = RustCorpus(rust_source_dirs(repo))
    rows = []
    seen = set()

    calls = parse_gameapp_calls(read_text(oracle))
    for line, method, qual in calls:
        if method in CONTENT_CALL_DENYLIST:
            continue
        spec = CONTENT_MAP.get(method)
        if spec is None:
            rows.append(dict(key="call:" + method, oracle=f"GameApp.cs:{line} {qual}.{method}",
                             file="-", kind="unmapped", deployed="-", rust="-",
                             status="unmapped", card=None, note="未在 CONTENT_MAP 登记，需人工归类"))
            continue
        rows.append(_make_content_row(repo, envir_dir, notice_dir, corpus, cards, spec,
                                      oracle_anchor=f"GameApp.cs:{line} {method}()"))
        seen.add(spec.get("file") or method)

    for spec in DECLARED_CONTENT:
        rows.append(_make_content_row(repo, envir_dir, notice_dir, corpus, cards, spec,
                                      oracle_anchor=spec["anchor"]))

    return rows


def _make_content_row(repo, envir_dir, notice_dir, corpus, cards, spec, oracle_anchor):
    kind = spec.get("kind")
    name = spec.get("file")
    path = None
    if kind == "envir":
        path = find_deployed(envir_dir, name)
    elif kind == "notice":
        path = find_deployed(notice_dir, name)
    elif kind == "dir":
        path = find_deployed(envir_dir, name)
    deployed = path is not None
    empty = deployed_is_empty(path)
    probe = spec.get("probe") or name
    rust = bool(probe) and corpus.present_literal(probe)
    status = content_row_status(spec, deployed, rust, cards, empty)
    key = "content:" + name if name else ("call:" + oracle_anchor)
    return dict(key=key, oracle=oracle_anchor, file=name or "-", kind=kind,
                deployed=("yes" if deployed else "no") + ("(empty)" if empty else ""),
                rust=("yes" if rust else "no"), status=status,
                card=(card_for(cards, key) or spec.get("card")),
                anchor=spec.get("anchor", ""))


# ---------------------------------------------------------------------------
# 协议派发门
# ---------------------------------------------------------------------------
RE_DISPATCH_CASE = re.compile(r"^\s*case\s+Messages\.((?:CM|RM)_[A-Za-z0-9_]+)\s*:")
RE_SM_REF = re.compile(r"Messages\.(SM_[A-Za-z0-9_]+)")


def parse_dispatch(text: str):
    """解析 PlayObject.Message.cs 的派发 switch。

    返回 (cm: {name: line}, rm_groups: {rm: set(sm)}, sm_all: set(sm))。
    分组规则：连续的 `case` 标签（无 body）折叠为一组；遇 `break;` 收组。
    """
    cm, rm_groups = {}, {}
    cur_labels, cur_sms = [], set()

    def flush(labels, sms):
        for lab in labels:
            if lab.startswith("RM_"):
                rm_groups.setdefault(lab, set()).update(sms)

    for no, line in enumerate(text.split(NL), 1):
        m = RE_DISPATCH_CASE.match(line)
        if m:
            lab = m.group(1)
            if cur_sms:  # 上一组有 body ⇒ 收组
                flush(cur_labels, cur_sms)
                cur_labels, cur_sms = [], set()
            cur_labels.append(lab)
            if lab.startswith("CM_"):
                cm.setdefault(lab, no)
            continue
        if cur_labels:
            cur_sms.update(RE_SM_REF.findall(line))
            if "break;" in line:
                flush(cur_labels, cur_sms)
                cur_labels, cur_sms = [], set()
    if cur_labels:
        flush(cur_labels, cur_sms)

    # 出站 SM 帧全集 = 整文件（去注释/字符串后）引用的全部 SM_*。
    # 既含 RM_* 支产出的帧，也含 CM_* 支的应答帧——它们都是本派发模块的出站编码器责任面。
    sm_all = set(RE_SM_REF.findall(strip_code(text)))
    return cm, rm_groups, sm_all


def dispatch_item_status(kind: str, name: str, rust: bool, cards: dict) -> str:
    if rust:
        return "implemented"
    if card_for(cards, f"{kind}:{name}"):
        return "tracked"
    return "RED"


def build_dispatch_rows(repo: Path, cards: dict):
    oracle = repo / ORACLE_DISPATCH
    corpus = RustCorpus(rust_source_dirs(repo, exclude=DISPATCH_EXCLUDE))
    cm, rm_groups, sm_all = parse_dispatch(read_text(oracle))

    rows = []
    for name, line in sorted(cm.items()):
        rust = corpus.find(name)
        rows.append(dict(bucket="inbound", key="inbound:" + name, name=name,
                         oracle=f"Message.cs:{line}",
                         rust=("yes" if rust else "no"),
                         rust_at=(f"{Path(rust[0]).name}:{rust[1]}" if rust else "-"),
                         status=dispatch_item_status("inbound", name, bool(rust), cards),
                         card=card_for(cards, "inbound:" + name),
                         emitted_by="-"))
    for sm in sorted(sm_all):
        rust = corpus.find(sm)
        rm_users = sorted(r for r, s in rm_groups.items() if sm in s)
        rows.append(dict(bucket="outbound", key="outbound:" + sm, name=sm,
                         oracle="(RM→SM)", rust=("yes" if rust else "no"),
                         rust_at=(f"{Path(rust[0]).name}:{rust[1]}" if rust else "-"),
                         status=dispatch_item_status("outbound", sm, bool(rust), cards),
                         card=card_for(cards, "outbound:" + sm),
                         emitted_by=",".join(rm_users[:4]) + ("…" if len(rm_users) > 4 else "")))
    return rows, cm, rm_groups, sm_all


# ---------------------------------------------------------------------------
# 汇总/打印
# ---------------------------------------------------------------------------
def summarize(rows):
    counts = {}
    reds = []
    for r in rows:
        counts[r["status"]] = counts.get(r["status"], 0) + 1
        if r["status"] == "RED":
            reds.append(r)
    return counts, reds


def print_table(rows, columns):
    if not rows:
        print("  (empty)")
        return
    widths = {c: max(len(c), max(len(str(r.get(c, ""))) for r in rows)) for c in columns}
    header = "  ".join(c.ljust(widths[c]) for c in columns)
    print(header)
    print("  ".join("-" * widths[c] for c in columns))
    for r in rows:
        print("  ".join(str(r.get(c, "")).ljust(widths[c]) for c in columns))


def emit_json(path, payload):
    Path(path).write_text(json.dumps(payload, ensure_ascii=False, indent=2) + NL, encoding="utf-8")


# ---------------------------------------------------------------------------
# 子命令
# ---------------------------------------------------------------------------
def cmd_content(args):
    repo = Path(args.repo)
    envir_dir, notice_dir = Path(args.envir_dir), Path(args.notice_dir)
    prereq_rc = emit_deploy_prereq(deploy_prereq_problems(envir_dir, notice_dir), args.allow_missing_deploy)
    if prereq_rc is not None:
        return prereq_rc
    rows = build_content_rows(repo, envir_dir, notice_dir, load_cards(Path(args.cards)))
    counts, reds = summarize(rows)
    print("== 内容加载对账门（§4-1）==")
    print(f"oracle={repo / ORACLE_CONTENT}")
    print(f"deployed Envir={args.envir_dir}  Notice={args.notice_dir}")
    print(f"cards={args.cards}")
    print()
    print_table(rows, ["oracle", "file", "kind", "deployed", "rust", "status", "card"])
    print()
    print("summary:", json.dumps(counts, ensure_ascii=False))
    if args.json:
        emit_json(args.json, dict(kind="content", counts=counts, rows=rows))
    return 1 if reds else 0


def cmd_dispatch(args):
    repo = Path(args.repo)
    rows, cm, rm_groups, sm_all = build_dispatch_rows(repo, load_cards(Path(args.cards)))
    counts, reds = summarize(rows)
    print("== 协议派发对账门（§4-2）==")
    print(f"oracle={repo / ORACLE_DISPATCH}")
    print(f"inbound case 全集={len(cm)}  outbound SM 帧全集={len(sm_all)}（来自 {len(rm_groups)} 个 RM 支）")
    print(f"cards={args.cards}")
    print()
    print_table(rows, ["bucket", "name", "oracle", "rust", "rust_at", "status", "card", "emitted_by"])
    if args.show_rm:
        print()
        print("-- RM→SM 映射（advisory，供 R2 定点）--")
        for rm in sorted(rm_groups):
            print(f"  {rm:28s} -> {', '.join(sorted(rm_groups[rm])) or '(无 SM)'}")
    print()
    print("summary:", json.dumps(counts, ensure_ascii=False))
    if args.json:
        emit_json(args.json, dict(kind="dispatch", counts=counts, rows=rows,
                                  rm_to_sm={k: sorted(v) for k, v in rm_groups.items()}))
    return 1 if reds else 0


# ---------------------------------------------------------------------------
# selftest
# ---------------------------------------------------------------------------
class _FakeCorpus:
    def __init__(self, present):
        self._set = set(present)

    def find(self, ident):
        return ("fake.rs", 1) if ident in self._set else None

    def present(self, ident):
        return ident in self._set


def selftest():
    ok = True
    lines = []

    def check(name, cond):
        nonlocal ok
        ok = ok and cond
        lines.append(("PASS" if cond else "FAIL") + "  " + name)

    cards = {
        "known_cards": ["openmir2-inbound-dispatch-red-batch", "openmir2-outbound-encoders-red-batch",
                        "openmir2-envir-redlist-batch"],
        "assignments": {
            "inbound:CM_CARDED": "openmir2-inbound-dispatch-red-batch",
            "outbound:SM_CARDED": "openmir2-outbound-encoders-red-batch",
            "content:Carded.txt": "openmir2-envir-redlist-batch",
        },
    }

    # ---- dispatch: 解析 ----
    oracle = NL.join([
        "switch (processMsg.wIdent)",
        "{",
        "  case Messages.CM_WALK:",
        "    x = Messages.SM_WALK;",
        "    break;",
        "  case Messages.CM_DROPITEM:",
        "  case Messages.CM_PICKUP:",
        "    y = Messages.SM_ITEMSHOW;",
        "    break;",
        "  case Messages.CM_NEWGAP:",
        "    z = Messages.SM_NEWGAP;",
        "    break;",
        "  case Messages.RM_TURN:",
        "    w = Messages.SM_TURN;",
        "    break;",
        "}",
    ])
    cm, rm_groups, sm_all = parse_dispatch(oracle)
    check("dispatch 解析：入站 case 全集 = {WALK,DROPITEM,PICKUP,NEWGAP}",
          set(cm) == {"CM_WALK", "CM_DROPITEM", "CM_PICKUP", "CM_NEWGAP"})
    check("dispatch 解析：fall-through 组 {DROPITEM,PICKUP} -> SM_ITEMSHOW",
          rm_groups.get("RM_TURN") == {"SM_TURN"} and "SM_ITEMSHOW" in sm_all)

    # teeth 1：构造新 CM_ 缺口（Rust 无臂、无卡）⇒ RED
    corpus = _FakeCorpus({"CM_WALK", "CM_DROPITEM", "CM_PICKUP", "SM_WALK", "SM_ITEMSHOW", "SM_NEWGAP"})
    imported = {name: dispatch_item_status("inbound", name, corpus.present(name), cards) for name in cm}
    check("teeth：构造 CM_NEWGAP 缺口（Rust 无臂 + 无卡）⇒ RED",
          imported.get("CM_NEWGAP") == "RED")
    check("dispatch：已实现 CM_WALK ⇒ implemented", imported.get("CM_WALK") == "implemented")

    # teeth 2：已卡缺口 ⇒ tracked 而非 RED
    check("dispatch：已卡 CM_CARDED ⇒ tracked（非 RED）",
          dispatch_item_status("inbound", "CM_CARDED", False, cards) == "tracked")
    check("dispatch：已卡 outbound SM_CARDED ⇒ tracked",
          dispatch_item_status("outbound", "SM_CARDED", False, cards) == "tracked")

    # teeth 3：出站构造缺口 ⇒ RED
    check("teeth：构造 outbound SM_NEWGAP 缺口 ⇒ RED",
          dispatch_item_status("outbound", "SM_NEWGAP", False, cards) == "RED")

    # ---- content: teeth ----
    # 已实现 ⇒ implemented
    spec_ok = dict(kind="envir", file="MiniMap.txt", anchor="x")
    check("content：Rust 有实现 ⇒ implemented",
          content_row_status(spec_ok, deployed=True, rust=True, cards=cards) == "implemented")
    # oracle∧部署∧¬rust∧无卡 ⇒ RED
    spec_gap = dict(kind="envir", file="SomeNewList.txt", anchor="x")
    check("teeth：oracle∧部署有件∧Rust 无∧无卡 ⇒ RED",
          content_row_status(spec_gap, deployed=True, rust=False, cards=cards) == "RED")
    # 无卡但部署缺件 ⇒ 不红（n/a-deployed）
    check("content：部署缺件 ⇒ n/a-deployed（不红）",
          content_row_status(spec_gap, deployed=False, rust=False, cards=cards) == "n/a-deployed")
    # 部署有件但为空 ⇒ 不红（n/a-empty；空加载行为无差）
    check("content：部署件为空 ⇒ n/a-empty（不红）",
          content_row_status(spec_gap, deployed=True, rust=False, cards=cards, empty=True) == "n/a-empty")
    # 部署件非空且无卡 ⇒ 仍 RED（empty 不能盖掉非空缺口）
    check("teeth：非空部署件 + 无卡 ⇒ RED（empty 默认不豁免）",
          content_row_status(spec_gap, deployed=True, rust=False, cards=cards, empty=False) == "RED")
    # 有卡 ⇒ tracked
    spec_card = dict(kind="envir", file="Carded.txt", anchor="x")
    check("content：有卡 ⇒ tracked（非 RED）",
          content_row_status(spec_card, deployed=True, rust=False, cards=cards) == "tracked")
    # 去卡 ⇒ 同一件转 RED（单调性牙）
    cards_nocard = dict(known_cards=[], assignments={})
    check("teeth：撤卡后同一件转 RED（单调性）",
          content_row_status(spec_card, deployed=True, rust=False, cards=cards_nocard) == "RED")

    # ---- content: 解析（GameApp 调用点发现）----
    ga = NL.join([
        "M2Share.LoadGameLogItemNameList();",
        "nCode = GameShare.LocalDb.LoadMonGen(out int mongenCount);",
        "loadList.LoadFromFile(sFileName);",
    ])
    calls = [(ln, m) for ln, m, _ in parse_gameapp_calls(ga)]
    check("content 解析：发现 Load* 调用点（含 LoadFromFile，去噪留给 denylist）",
          ("LoadGameLogItemNameList" in [m for _, m in calls]) and
          ("LoadMonGen" in [m for _, m in calls]) and
          ("LoadFromFile" in [m for _, m in calls]))

    # ---- strip_code 正确性 ----
    src = NL.join([
        'let a = "CM_NOT_A_CASE"; // CM_COMMENT',
        '/* CM_BLOCK */ let b = messages::CM_REAL;',
        'let c = "SM_X";',
    ])
    stripped = strip_code(src)
    check("strip_code：字符串/行注/块注被去除且保留行数",
          stripped.count(NL) == src.count(NL) and "CM_COMMENT" not in stripped
          and "CM_BLOCK" not in stripped and "CM_REAL" in stripped)

    # ---- deploy 前置门（fail-closed；对抗验证 v5 F1）----
    import contextlib
    import io
    import shutil
    import tempfile
    miss_e, miss_n = Path("E:/no-such-envir-zzz"), Path("E:/no-such-notice-zzz")
    check("deploy 前置：不存在的部署目录被识别为问题（2 条）",
          len(deploy_prereq_problems(miss_e, miss_n)) == 2)

    # 1) 缺部署目录 ⇒ fail-closed：rc=2 且印 PREREQ_MISSING + RESULT: PREREQ_RED（短路在构表之前）
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        rc_missing = main(["content", "--envir-dir", str(miss_e), "--notice-dir", str(miss_n)])
    out_missing = buf.getvalue()
    check("teeth：content 缺部署目录 ⇒ rc=2（非零）且 PREREQ_MISSING + RESULT: PREREQ_RED",
          rc_missing == 2 and "PREREQ_MISSING" in out_missing and "RESULT: PREREQ_RED" in out_missing)

    # 2) --allow-missing-deploy ⇒ 降级：rc≠2 且印 WARN 且**继续构表**（出 summary）。用最小临时仓供构表。
    tmp_repo = Path(tempfile.mkdtemp(prefix="recon-self-"))
    try:
        (tmp_repo / "src/GameSrv").mkdir(parents=True)
        (tmp_repo / ORACLE_CONTENT).write_text("GameShare.LocalDb.LoadGuardList();" + NL, encoding="utf-8")
        for d in ("mir2-rs/crates/rune/src", "mir2-rs/bins/game-svr/src"):
            (tmp_repo / d).mkdir(parents=True, exist_ok=True)
        tmp_cards = tmp_repo / "cards.json"
        tmp_cards.write_text('{"known_cards": [], "assignments": {}}', encoding="utf-8")
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc_allow = main(["content", "--repo", str(tmp_repo), "--envir-dir", str(miss_e),
                             "--notice-dir", str(miss_n), "--cards", str(tmp_cards),
                             "--allow-missing-deploy"])
        out_allow = buf.getvalue()
    finally:
        shutil.rmtree(tmp_repo, ignore_errors=True)
    check("teeth：--allow-missing-deploy ⇒ 降级（rc≠2）且印 WARN 且继续出表",
          rc_allow != 2 and "WARN" in out_allow and "summary:" in out_allow)

    print("== recon_gates selftest ==")
    for l in lines:
        print("  " + l)
    print("RESULT:", "ALL PASS" if ok else "FAILED")
    return 0 if ok else 1


def cmd_mutate(args):
    """变异实测：对**真实输入**做三处自设计变异，门必须转红（证明判据读侧/方向正确）。

    M1 dispatch 注入：临时 oracle 加一条合成 `CM_ZZMUT_GAP` ⇒ 派发门新增 1 条 RED。
    M2 撤卡：把 `outbound:SM_DISAPPEAR` 的卡指派生删 ⇒ 该项 tracked→RED。
    M3 读侧方向：空 Rust 语料 ⇒ 真仓库里 implemented 的 `GuardList.txt` 必转 RED（证明门真读 Rust 代码，
       而非只看 oracle/部署面）。
    """
    import shutil
    import tempfile

    repo = Path(args.repo)
    cards = load_cards(Path(args.cards))
    prereq_rc = emit_deploy_prereq(deploy_prereq_problems(Path(args.envir_dir), Path(args.notice_dir)),
                                  args.allow_missing_deploy)
    if prereq_rc is not None:
        return prereq_rc
    results = []
    ok = True

    def add(name, cond):
        nonlocal ok
        ok = ok and cond
        results.append((name, cond))

    # M1 —— 派发门注入合成缺口
    oracle_text = read_text(repo / ORACLE_DISPATCH)
    base_cm, _, _ = parse_dispatch(oracle_text)
    injected = oracle_text + NL + NL.join([
        "                case Messages.CM_ZZMUT_GAP:",
        "                    break;",
        "",
    ]) + NL
    cm2, _, _ = parse_dispatch(injected)
    corpus = RustCorpus(rust_source_dirs(repo, exclude=DISPATCH_EXCLUDE))
    st = dispatch_item_status("inbound", "CM_ZZMUT_GAP", corpus.present("CM_ZZMUT_GAP"), cards)
    add("M1 dispatch 注入合成 CM_ZZMUT_GAP ⇒ 新 RED", (set(cm2) - set(base_cm)) == {"CM_ZZMUT_GAP"} and st == "RED")

    # M2 —— 撤销卡指派
    before = dispatch_item_status("outbound", "SM_DISAPPEAR", False, cards)
    cards2 = json.loads(json.dumps(cards))
    cards2["assignments"].pop("outbound:SM_DISAPPEAR", None)
    after = dispatch_item_status("outbound", "SM_DISAPPEAR", False, cards2)
    add("M2 撤卡 outbound:SM_DISAPPEAR ⇒ tracked→RED", before == "tracked" and after == "RED")

    # M3 —— 读侧方向（空 Rust 语料）
    tmp = Path(tempfile.mkdtemp(prefix="recon-mut-"))
    try:
        (tmp / "src/GameSrv").mkdir(parents=True)
        shutil.copy(repo / ORACLE_CONTENT, tmp / "src/GameSrv/GameApp.cs")
        for d in ("mir2-rs/bins/game-svr/src", "mir2-rs/crates/data/src"):
            (tmp / d).mkdir(parents=True, exist_ok=True)
        rows = build_content_rows(tmp, Path(args.envir_dir), Path(args.notice_dir), cards)
        gr = [r for r in rows if r.get("file") == "GuardList.txt"]
        add("M3 空 Rust 语料 ⇒ GuardList.txt implemented→RED（读侧方向牙）",
            bool(gr) and gr[0]["status"] == "RED")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    print("== recon_gates mutate（变异实测：构造缺陷必红）==")
    for name, passed in results:
        print(("  PASS  " if passed else "  FAIL  ") + name)
    print("RESULT:", "ALL PASS" if ok else "FAILED")
    return 0 if ok else 1


# ---------------------------------------------------------------------------
def main(argv=None):
    ap = argparse.ArgumentParser(description="OpenMir2→Rust 两道对账门")
    sub = ap.add_subparsers(dest="cmd", required=True)

    def common(p):
        p.add_argument("--repo", default=str(REPO_DEFAULT))
        p.add_argument("--cards", default=str(DEFAULT_CARDS))
        p.add_argument("--json", default=None)

    pc = sub.add_parser("content", help="内容加载对账门")
    common(pc)
    pc.add_argument("--envir-dir", default=str(DEFAULT_ENVIR_DIR))
    pc.add_argument("--notice-dir", default=str(DEFAULT_NOTICE_DIR))
    pc.add_argument("--allow-missing-deploy", action="store_true",
                    help="诊断用：部署目录缺失时降级为 WARN 放行（默认 fail-closed 拒跑，退出码 2）")
    pc.set_defaults(func=cmd_content)

    pd = sub.add_parser("dispatch", help="协议派发对账门")
    common(pd)
    pd.add_argument("--show-rm", action="store_true", help="附 RM→SM 映射")
    pd.set_defaults(func=cmd_dispatch)

    ps = sub.add_parser("selftest", help="内置夹具自测")
    common(ps)  # 容错：与其它子命令同参（本体不用 repo/cards）
    ps.set_defaults(func=lambda a: selftest())

    pm = sub.add_parser("mutate", help="变异实测（构造缺陷必红）")
    common(pm)
    pm.add_argument("--envir-dir", default=str(DEFAULT_ENVIR_DIR))
    pm.add_argument("--notice-dir", default=str(DEFAULT_NOTICE_DIR))
    pm.add_argument("--allow-missing-deploy", action="store_true",
                    help="诊断用：部署目录缺失时降级为 WARN 放行（默认 fail-closed 拒跑，退出码 2）")
    pm.set_defaults(func=cmd_mutate)

    args = ap.parse_args(argv)
    return args.func(args)


if __name__ == "__main__":
    sys.exit(main())
