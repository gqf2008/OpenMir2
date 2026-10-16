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

判红口径（缺一即不红）
----------------------
  content : oracle 有该 Load 调用 ∧ 部署有该内容文件 ∧ Rust 无实现 ∧ 无卡  ⇒ RED
  dispatch: oracle 有 case ∧ **该帧可达** ∧ Rust 无派发臂/编码器 ∧ 无卡   ⇒ RED

「无卡」= `coverage/cards.json` 的 `assignments` 未给该缺口指派一个存在于 `known_cards` 的卡 id。
「Rust 实现」= 在 Rust 语料（去注释/去字符串字面量后）里出现该标识符（派发）
              / **同一 src 文件里同时出现文件名字面量与「真装载调用」标记**（内容，见下）。
「可达」（派发·出站） = 触发该 SM 帧的**任一 `RM_*`** 在 `src/**/*.cs` 里有**生产者用法**（见下）
  ⇒ 可达。`RM_*` 全仓**每一处**出现都只是 `const` 定义或 `case` 标签（即确证无任何生产者）⇒ oracle
  死支 ⇒ 该帧永不发出 ⇒ 不判 RED（豁免 `n/a-unreachable`）。这修掉 SM_41/SM_43/SM_PASSWORDSTATUS
  三条「门误报」假红。**口径保守（对抗验证 v3 修正）**：旧口径「整行同时含 `Send*(` 与该 RM 字面量」
  会把经动态形参/辅助函数间接发出的 RM（`Walk(Messages.RM_WALK)`、`wIdent = Messages.RM_BIGHIT`
  后 `SendRefMsg(wIdent,…)`）误判为死支 ⇒ 静默豁免真可达帧；现改为**对无法文本判定者一律按可达处理**，
  只在能证明「每一处出现都是 const 定义或 case 标签」时才允许豁免（详见 README 判据节）。
「真装载调用」（内容·收紧了旧口径） = 旧口径「文件名串在源码里出现」可被**报错串/死常量**假绿；新口径要求
  某 src 文件**同时**命中：①文件名字面量 ②装载标记（`load…(` 调用 / `fn load…` 定义），或镜像模块路径
  （文件 stem = 内容件模块名且含函数定义）。⇒ 注释-only、纯报错串不再算实现。
「豁免清单」= `cards.json` 的 `known_unreachable` / `known_false`（人工登记的死支/门误报，与自动可达性互为兜底）。

退出码：0 = 无 RED（缺口全部「已实现或已卡」）；1 = 存在 RED；2 = 用法/环境错误或前置缺失
（PREREQ_RED：部署目录不存在/不可读/不是目录 ⇒ 印 `PREREQ_MISSING` + `RESULT: PREREQ_RED`，**拒跑而不
静默放行**；判不了缺口时绝不报「无缺口」假绿。诊断需要看表可加 `--allow-missing-deploy` 显式降级为 WARN，
但此时结论不可作数）。
"""

from __future__ import annotations

import argparse
import json
import re
import subprocess
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
# 出站帧「触发 RM 可达性」判据的 oracle 扫描根：src/**/*.cs 全仓（含各 Actor/NPC/Script 的发送点）。
ORACLE_SRC_DIR = "src"
# 可达判据（**保守**口径，见 oracle_rm_producers）：一个 RM_* 只要在 `src/**/*.cs` 里出现任何
# **非「纯 const 定义」、非「纯 case 标签」** 的用法（作为 Send*/SendRefMsg/Walk/ClientChangeDir/任意
# 调用的实参、赋给局部变量后经动态形参间接发送、参与比较等）即视为「可能被发出」⇒ 可达。
# 只有「该 RM_* 的每一处出现都只是 `const int RM_x = …;` 定义或 `case Messages.RM_x:` 标签」才判为
# **确证死支**并允许豁免。⇒ 动态形参 / 间接发送一律不漏（宁多判可达，绝不静默豁免真可达帧）。
RE_RM_REF = re.compile(r"\bRM_[A-Za-z0-9_]+\b")
# 纯 `case Messages.RM_x:` 标签 / 纯 `const int RM_x` 定义行 —— 这两类出现**不算**「生产者用法」。
RE_CASE_LABEL = re.compile(r"case\s+(?:Messages\.)?(RM_[A-Za-z0-9_]+)\s*:")
RE_CONST_DEF = re.compile(r"const\s+int\s+(RM_[A-Za-z0-9_]+)")
# 内容门「真装载调用」标记：load 调用/定义形态。
RE_LOAD_CALL = re.compile(r"(?<![A-Za-z0-9_])load[a-z0-9_]*\s*[(<]")
RE_LOAD_FN = re.compile(r"(?<![A-Za-z0-9_])fn\s+load[a-z0-9_]*")
# 「装载函数名」捕获（判断装载函数是否被 server 二进制引用 —— 见 RustCorpus.loader_wired）。
RE_LOAD_FN_NAME = re.compile(r"(?<![A-Za-z0-9_])fn\s+(load[a-z0-9_]*)")
# 入站 no-op 臂结构判据（片 D1-②）：本臂「恒假/恒空」的可机判形态。
#   (a) handler 输出绑到 `_` 前缀名（形态 `let (decision, _frames) = ...`）⇒ 输出被丢弃。
RE_NOOP_UNDERSCORE_BIND = re.compile(r"let\s*\([^)]*\b_\w+")
#   (b) 用裸字面量（false/true/整数）喂 `client_*` handler —— 会话态未挂、被常量顶死。
RE_CLIENT_LITERAL_ARG = re.compile(r"(?<![A-Za-z0-9_])client_[a-z0-9_]+\s*\([^)]*?(?:\bfalse\b|\btrue\b|(?<![A-Za-z0-9_.])\d+(?![A-Za-z0-9_.]))")


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
    "LoadMapDoor":               dict(kind="derived", anchor="MapManager.cs:182（随地图文件，非独立内容件）", probe_ident="add_map_door"),
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
        self.file_text = {}   # path -> 去注释+保留字符串 的整篇文本（供「真装载调用」同文件判据）
        self.bins_code = []   # code 里来自 server 二进制（*/bins/*/src）的行
        for d in dirs:
            root = Path(d)
            if not root.is_dir():
                continue
            for p in sorted(root.rglob("*.rs")):
                raw = read_text(p)
                is_bins = "bins" in Path(str(p)).parts
                for no, code in enumerate(strip_code(raw).split(NL), 1):
                    if code.strip():
                        self.code.append((str(p), no, code))
                        if is_bins:
                            self.bins_code.append((str(p), no, code))
                kept = strip_code(raw, keep_strings=True)
                self.file_text[str(p)] = kept
                for no, txt in enumerate(kept.split(NL), 1):
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

    def files_with_literal(self, lit: str):
        """含字符串字面量 `lit`（含引号）的 src 文件路径列表（去注释、保留字符串）。"""
        needle = QT + lit + QT
        return [p for p, t in sorted(self.file_text.items()) if needle in t]

    def file_has_loader_marker(self, path: str, item_stem: str = "") -> bool:
        """该文件是否含「真装载调用」标记：`load…(`/`fn load…`，或镜像模块路径（stem 命中且含函数）。"""
        t = self.file_text.get(path, "")
        if RE_LOAD_CALL.search(t) or RE_LOAD_FN.search(t):
            return True
        if item_stem and Path(path).stem.lower() == item_stem and re.search(r"\bfn\s+", t):
            return True
        return False

    def is_bins_file(self, path: str) -> bool:
        """该文件是否属 server 二进制（`*/bins/*/src`）。"""
        return "bins" in Path(path).parts

    def bins_ref_other(self, name: str, selfpath: str) -> bool:
        """标识符 `name` 是否在**另一个** server 二进制源文件里被引用（= 被接线）。"""
        pat = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")
        for p, _no, code in self.bins_code:
            if p == selfpath:
                continue
            if pat.search(code):
                return True
        return False

    def loader_wired(self, path: str, item_stem: str = "") -> bool:
        """「装载点被 server 二进制接线」判据（片 D1-① 的**死注册表**牙）。

        一个装载点算「真装载」还要求它不是**死注册表**：
          · 该文件里定义的装载函数（`fn load…`）被另一个 `*/bins/*/src` 源文件引用；或
          · 镜像模块路径（文件 stem = 内容件 stem）时，模块名被 `*/bins/*/src` 引用（`use …::stem`）；
          · 该文件本身就在 bin 里且直接调 crate 装载函数。
        只写进 crate 内部、二进制侧零引用的装载是死注册表（例：`crates/data/src/lists.rs` 全仓零引用）
        ⇒ 不算实现、须判 RED。据此 `AdminList/GameLogItemNameList/ItemNameList/DisableSendMsgList/
        DenyChrNameList/NoClearMonList` 由 `implemented` 降级（见 `content_rust_implemented` 判据）。
        """
        text = self.file_text.get(path, "")
        names = RE_LOAD_FN_NAME.findall(text)
        for nm in names:
            if self.bins_ref_other(nm, path):
                return True
        if item_stem and Path(path).stem.lower() == item_stem and self.bins_ref_other(Path(path).stem, path):
            return True
        if not names and self.is_bins_file(path) and RE_LOAD_CALL.search(text):
            return True
        return False


# ---------------------------------------------------------------------------
# cards.json
# ---------------------------------------------------------------------------
def load_cards(path: Path) -> dict:
    if not path.is_file():
        return {"known_cards": [], "assignments": {}, "known_unreachable": [], "known_false": []}
    data = json.loads(read_text(path))
    data.setdefault("known_cards", [])
    data.setdefault("assignments", {})
    data.setdefault("known_unreachable", [])
    data.setdefault("known_false", [])
    # 片 D1：oracle 侧 no-op / Rust 死消费点 证据桶（key -> 证据串，须含 `file:line`）。
    data.setdefault("known_oracle_noop", {})
    data.setdefault("known_dead_consumer", {})
    return data


# 证据行须形如 `path.cs:NNN` / `path.rs:NNN`（含文件:行）——「豁免必须逐条带证据行」的机判形态。
RE_EVIDENCE_LINE = re.compile(r"[^\s:]+\.(?:cs|rs):\d+")


def _evidence_ok(ev) -> bool:
    return isinstance(ev, str) and bool(ev.strip()) and bool(RE_EVIDENCE_LINE.search(ev))


def oracle_noop_evidence(cards: dict, key: str):
    """key 在 `known_oracle_noop` 且证据行合法 ⇒ 返回证据串；否则 None（未豁免）。

    片 D1-①：oracle 该加载点/消费点整体是**注释掉的 no-op**（body 全注释 / getter 恒空）⇒ 特性停用，
    不该要求 Rust 实现。豁免**必须逐条带证据行**（`file:line`）；空证据 / 无行号证据一律不生效
    （⇒ 落回普通判据，未豁免即 RED）。
    """
    ev = cards.get("known_oracle_noop", {}).get(key)
    return ev if _evidence_ok(ev) else None


def dead_consumer_evidence(cards: dict, key: str):
    """key 在 `known_dead_consumer` 且证据行合法 ⇒ 证据串；否则 None。

    片 D1-①：Rust 装载点已接入二进制，但**消费点延后/未接**（例：MiniMap 下发帧 / LineNotice 滚动公告）
    ⇒ 仍算缺口（不判 `implemented`），落回 tracked/RED。
    """
    ev = cards.get("known_dead_consumer", {}).get(key)
    return ev if _evidence_ok(ev) else None


def board_card_ids(repo: Path):
    """看板上存在的卡 id 集合（`walgit collab ls`）。取不到（walgit 不可用）⇒ None（不判缺卡）。"""
    try:
        out = subprocess.run(["walgit", "collab", "ls", "--repo", str(repo)],
                             capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.SubprocessError):
        return None
    if out.returncode != 0:
        return None
    ids = set()
    for line in (out.stdout or "").splitlines():
        line = line.strip()
        if line.startswith("openmir2-"):
            ids.add(line)
    return ids or None


def resolve_cards(cards: dict, repo: Path, board_lookup=None):
    """校验 `known_cards` 引用的卡在**看板上存在**（片 D1-③）。

    `board_lookup(repo) -> set|None`（默认 `board_card_ids`）。取不到看板 ⇒ 不判缺卡（`_board_probed=False`，
    只印 WARN）。存在的卡写入 `_valid_cards`（`card_for` 以它为准）；看板上不存在的写入 `_missing_cards`。
    """
    lookup = board_lookup or board_card_ids
    known = list(cards.get("known_cards", []) or [])
    board = lookup(repo)
    cards["_board_probed"] = board is not None
    if board is None:
        cards["_valid_cards"] = known
        cards["_missing_cards"] = []
        return cards
    valid = [c for c in known if c in board]
    missing = [c for c in known if c not in board]
    # assignments 里引用的卡也须存在（未列入 known_cards 的由 card_for 门兜底）。
    for cid in set((cards.get("assignments", {}) or {}).values()):
        if isinstance(cid, str) and cid.startswith("openmir2-") and cid not in board and cid not in missing:
            missing.append(cid)
    cards["_valid_cards"] = valid
    cards["_missing_cards"] = missing
    return cards


def emit_card_prereq(cards: dict) -> int:
    """印缺卡点名（片 D1-③）：引用了看板上不存在的卡 ⇒ 印 PREREQ_CARD_MISSING，返回 1（计 RED）。"""
    missing = cards.get("_missing_cards", []) or []
    if not cards.get("_board_probed", False):
        print("WARN 卡存在性未校验（walgit collab ls 不可用）⇒ 缺卡判据本次不生效")
        return 0
    for cid in missing:
        print("PREREQ_CARD_MISSING: 卡 %s 在看板上无条目（walgit collab thread 为空）⇒ 该卡所辖缺口按未立卡处理（RED）" % cid)
    return 1 if missing else 0


def card_for(cards: dict, key: str):
    """缺口 key 指向的卡 id（须是**看板上存在**的卡，片 D1-③），否则 None。

    `known_cards` 中在看板上不存在的卡不算数 ⇒ 该缺口按「未立卡」处理（RED）。
    """
    cid = cards.get("assignments", {}).get(key)
    if cid and cid in set(cards.get("_valid_cards", cards.get("known_cards", []))):
        return cid
    return None


def is_exempted(cards: dict, key: str) -> bool:
    """人工豁免登记：key 在 `known_unreachable` / `known_false` 里 ⇒ 不判 RED。"""
    for bucket in ("known_unreachable", "known_false"):
        if key in set(cards.get(bucket, []) or []):
            return True
    return False


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
    """部署目录前置检查（fail-closed）：目录须**存在、是目录、可枚举、且非空**。

    对抗验证 v5 F1（高·静默假绿 fail-open）：部署目录缺失时 `find_deployed` 对每件都返回 None
    ⇒ 全表 `n/a-deployed`（非红）⇒ 门报「无缺口」且退出 0。**片 D1-⑤**：目录「存在但为空」
    （路径笔误指向新建目录 / 父目录）同样 ⇒ 每件 `find_deployed→None` ⇒ 全表 `n/a-deployed` 假绿，
    故空目录也判 PREREQ_RED。返回问题清单（空 = 通过），由调用方决定 rc=2 还是（显式
    `--allow-missing-deploy` 时）WARN。
    """
    problems = []
    for label, d in (("Envir", envir_dir), ("Notice", notice_dir)):
        p = Path(d)
        try:
            if not p.exists():
                problems.append((label, str(p), "目录不存在"))
            elif not p.is_dir():
                problems.append((label, str(p), "不是目录"))
            elif next(p.iterdir(), None) is None:
                problems.append((label, str(p), "目录为空（无任何条目；路径可能指错）"))
        except OSError as exc:
            problems.append((label, str(p), "目录不可读（%s: %s）" % (type(exc).__name__, exc)))
    return problems


def oracle_prereq_problems(repo: Path, rel_paths):
    """oracle 源文件前置检查（片 D1-⑥，fail-closed）。

    缺 oracle 文件时旧版直接 `read_text` ⇒ 未捕获 `FileNotFoundError` traceback + rc=1（与
    content/mutate 的 fail-closed 口径不一致）。现改为构表前校验：缺文件 ⇒ PREREQ_RED rc=2。
    """
    problems = []
    for rel in rel_paths:
        p = repo / rel
        if not p.exists():
            problems.append(("oracle", str(p), "文件不存在"))
        elif not p.is_file():
            problems.append(("oracle", str(p), "不是文件"))
    return problems


def emit_deploy_prereq(problems, allow_missing):
    """按前置问题印 PREREQ_MISSING / WARN，返回退出码：None = 未触发 PREREQ（可继续）；2 = 拒跑。

    `allow_missing=True`（`--allow-missing-deploy`，仅诊断用）⇒ 降级为 WARN 放行，返回 None。
    """
    if not problems:
        return None
    if allow_missing:
        for label, path, reason in problems:
            print("WARN 部署%s %s —— %s；因 --allow-missing-deploy 降级放行（仅诊断，缺口结论不可作数）"
                  % (label, path, reason))
        return None
    for label, path, reason in problems:
        print("PREREQ_MISSING: 部署%s %s —— %s（判不了缺口，fail-closed 拒跑；诊断可用 --allow-missing-deploy）"
              % (label, path, reason))
    print("RESULT: PREREQ_RED")
    return 2


def emit_oracle_prereq(problems):
    """oracle 源缺失（片 D1-⑥）：恒 rc=2 PREREQ_RED（`--allow-missing-deploy` 不豁免 oracle 面）。"""
    if not problems:
        return None
    for label, path, reason in problems:
        print("PREREQ_MISSING: %s %s —— %s（判不了缺口，fail-closed 拒跑）" % (label, path, reason))
    print("RESULT: PREREQ_RED")
    return 2


def _item_stem(filename: str) -> str:
    """内容件名 → 模块 stem（小写、去 .txt、去非字母数字）：MiniMap.txt → minimap。"""
    s = (filename or "").strip()
    if s.lower().endswith(".txt"):
        s = s[:-4]
    return re.sub(r"[^a-z0-9]", "", s.lower())


def content_rust_implemented(corpus, filename: str) -> bool:
    """内容门「真装载调用」判据（片 D1-① 收紧：**死注册表**不再算实现）。

    判为已实现 ⟺ 存在**一个 src 文件**同时命中：
      ① 文件名字符串字面量（去注释后仍在，即非注释-only）；
      ② 装载标记（`load…(`/`fn load…` 或镜像模块路径 + 函数定义）；
      ③ **装载点被 server 二进制接线**（`RustCorpus.loader_wired`：装载函数被另一 `*/bins/*/src`
         文件引用）。⇒ 只写进 crate 内部、二进制侧零引用的装载是**死注册表**（旧口径把它判
         `implemented`，critic 假绿 #1），现降级 ⇒ 落回 tracked/RED。
    """
    if not filename:
        return False
    stem = _item_stem(filename)
    for path in corpus.files_with_literal(filename):
        if corpus.file_has_loader_marker(path, stem) and corpus.loader_wired(path, stem):
            return True
    return False


def content_row_status(spec: dict, deployed: bool, rust: bool, cards: dict, empty: bool = False,
                       dead: bool = False, key: str = None) -> str:
    kind = spec.get("kind")
    if key is None:
        key = ("content:" + spec["file"]) if spec.get("file") else ("call:" + str(spec.get("anchor", "")))
    # 片 D1-①：oracle 侧整体 no-op（body 全注释 / getter 恒空，逐条带证据行）⇒ 非缺口，也不算 implemented。
    if oracle_noop_evidence(cards, key):
        return "n/a-oracle-noop"
    if kind not in ("envir", "notice", "dir", "derived"):
        return "n/a-" + kind
    if dead and dead_consumer_evidence(cards, key):
        pass  # 片 D1-①：装载点已接二进制但消费点延后（例 MiniMap/LineNotice）⇒ 不算实现，落回 tracked/RED
    elif rust:
        return "implemented"
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
    # 片 D1-④：derived（无独立内容件、随已部署的源内容加载，例 `LoadMapDoor` 随地图文件）——
    # 「部署有」恒真；Rust 实现改按 `probe_ident` 标识符存在性判（不再静默 n/a-other）。
    deployed = True if kind == "derived" else (path is not None)
    empty = deployed_is_empty(path)
    if spec.get("probe_ident"):
        rust = corpus.present(spec["probe_ident"])
    else:
        probe = spec.get("probe") or name
        rust = content_rust_implemented(corpus, probe)
    key = ("content:" + name) if name else ("call:" + oracle_anchor)
    dead = bool(name and dead_consumer_evidence(cards, key))
    status = content_row_status(spec, deployed, rust, cards, empty, dead=dead, key=key)
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


def dispatch_item_status(kind: str, name: str, rust: bool, cards: dict, reachable=None, noop=False) -> str:
    """缺口状态。reachable：出站帧触发 RM 的可达性（True/False/None=不可判）；noop：入站臂是否恒假/恒空。

    次序：oracle-no-op → 已实现（且非 no-op 臂）→ 已卡(tracked) → 豁免(不可达/人工登记) → RED。
    `noop=True`（片 D1-②：臂在但不干活——恒零出帧 / 恒走 FAIL 支）⇒ **不判 implemented**，落回 tracked/RED。
    `reachable is False`（触发 RM 每一个都是确证死支）或人工豁免登记 ⇒ `n/a-unreachable`（不判 RED）。
    `reachable is None`（无触发 RM）**不豁免**——保守按可达处理。
    """
    key = f"{kind}:{name}"
    if oracle_noop_evidence(cards, key):
        return "n/a-oracle-noop"
    if rust and not noop:
        return "implemented"
    if card_for(cards, key):
        return "tracked"
    if reachable is False or is_exempted(cards, key):
        return "n/a-unreachable"
    return "RED"


# 匹配 `messages::CM_X =>`（含 `mir2_protocol::` 前缀）的派发臂头。
RE_ARM_HEAD = re.compile(r"^\s*(?:mir2_protocol::)?messages::(CM_[A-Za-z0-9_]+)\s*=>")


def _file_code_lines(corpus):
    """按文件还原（去注释/去字符串后的）逐行代码（空行补 ""），供匹配臂的括号配对。"""
    by_file = {}
    for p, no, code in corpus.code:
        by_file.setdefault(p, {})[no] = code
    out = {}
    for p, m in by_file.items():
        n = max(m) if m else 0
        out[p] = [m.get(i, "") for i in range(1, n + 1)]
    return out


def extract_cm_arms(corpus):
    """返回 {CM_name: [arm_body_text, …]}：从 Rust 语料抽取 `messages::CM_X => { … }` 匹配臂体。"""
    arms = {}
    for _p, lines in _file_code_lines(corpus).items():
        for i, line in enumerate(lines):
            m = RE_ARM_HEAD.match(line)
            if not m:
                continue
            name = m.group(1)
            if "{" not in line:
                arms.setdefault(name, []).append(line.strip())
                continue
            depth, started, body = 0, False, []
            for j in range(i, min(i + 200, len(lines))):
                body.append(lines[j])
                depth += lines[j].count("{") - lines[j].count("}")
                if "{" in lines[j]:
                    started = True
                if started and depth <= 0:
                    break
            arms.setdefault(name, []).append(" ".join(body))
    return arms


def inbound_arm_noop(corpus, name: str, arms: dict = None) -> bool:
    """片 D1-②：入站 `CM_*` 的派发臂是否**恒假/恒空 no-op**（可机判形态，**非手写黑名单**）。

    形态判据（命中任一即 no-op）：
      (a) handler 输出被丢弃——`let (decision, _frames) = …`（绑到 `_` 前缀名）；
      (b) 用裸字面量（`false`/`true`/整数）喂 `client_*` handler——会话态未挂、被常量顶死
          （`client_open_door(false)` / `client_get_min_map(0)` / `client_adjust_bonus(cmd.recog, 0)`）。
    臂不存在 ⇒ False（交由既有「缺臂」判据）；所有抽到的臂都命中形态 ⇒ True（⇒ 不算 implemented）。
    `arms` 可传入 `extract_cm_arms(corpus)` 的缓存（build_dispatch_rows 逐臂复用，避免重复抽取）。
    """
    if arms is None:
        arms = extract_cm_arms(corpus)
    bodies = arms.get(name)
    if not bodies:
        return False
    return all((RE_NOOP_UNDERSCORE_BIND.search(b) or RE_CLIENT_LITERAL_ARG.search(b)) for b in bodies)


def oracle_rm_producers(repo: Path):
    """扫 `src/**/*.cs`，返回**可能被发出**的 RM_* 全集（= 可达触发点）。

    口径（**保守 / 欠豁免**）——一个 RM_* 只要在某文件里出现**任何非「纯 const 定义」、非「纯 case
    标签」的用法**即计入，包括：
      · 作为任意调用（`Send*` / `SendRefMsg` / `Walk(RM_WALK)` / `ClientChangeDir` / …）的实参；
      · 赋给局部变量后经**动态形参**间接发送（`wIdent = Messages.RM_BIGHIT; … SendRefMsg(wIdent,…)`）；
      · 参与比较、传递、作数组下标等任何其它代码用法。
    只有「该 RM_* 的**每一处**出现都只是 `const int RM_x = …;` 定义或 `case Messages.RM_x:` 标签」
    才**不**计入——即该 RM_* 全仓无任何生产者，判为**确证死支**。

    为何保守：旧口径「整行同时含 `Send<名>(` 与该 RM 字面量」会把经动态形参 / 辅助函数间接发出的 RM
    误判为死支 ⇒ 静默豁免真可达帧（对抗验证 v3 判 refuted：真语料里 10 条真可达帧被误判死支）。
    本口径对**无法文本判定者一律按可达处理**，只在能证明「每一处出现都是 const 定义或 case 标签」时
    才允许豁免。注释 / 字符串字面量已被 `strip_code` 剔除 ⇒ 文档里提一句 `RM_x` 不会误判可达。
    """
    producers = set()
    src = repo / ORACLE_SRC_DIR
    if not src.is_dir():
        return producers
    for p in sorted(src.rglob("*.cs")):
        code = strip_code(read_text(p))
        for line in code.split(NL):
            toks = set(RE_RM_REF.findall(line))
            if not toks:
                continue
            inert = set(RE_CASE_LABEL.findall(line)) | set(RE_CONST_DEF.findall(line))
            producers |= (toks - inert)
    return producers


def outbound_reachability(rm_users, rm_producers) -> bool:
    """出站帧可达性：True=任一触发 RM 是「可能被发出」（含经动态形参 / 间接调用）；False=有触发 RM 但
    每一个都是确证死支（全仓仅 const 定义 + case 标签）；None=无触发 RM（判不了 ⇒ 保守按可达处理）。"""
    rms = [r for r in rm_users if r]
    if not rms:
        return None
    return any(r in rm_producers for r in rms)


def build_dispatch_rows(repo: Path, cards: dict):
    oracle = repo / ORACLE_DISPATCH
    corpus = RustCorpus(rust_source_dirs(repo, exclude=DISPATCH_EXCLUDE))
    cm, rm_groups, sm_all = parse_dispatch(read_text(oracle))
    rm_producers = oracle_rm_producers(repo)

    rows = []
    arms = extract_cm_arms(corpus)
    for name, line in sorted(cm.items()):
        rust = corpus.find(name)
        noop = inbound_arm_noop(corpus, name, arms)
        rows.append(dict(bucket="inbound", key="inbound:" + name, name=name,
                         oracle=f"Message.cs:{line}",
                         rust=("yes" if rust else "no"),
                         rust_at=(f"{Path(rust[0]).name}:{rust[1]}" if rust else "-"),
                         status=dispatch_item_status("inbound", name, bool(rust), cards, noop=noop),
                         card=card_for(cards, "inbound:" + name),
                         reach="-",
                         noop=("noop" if noop else "-"),
                         emitted_by="-"))
    for sm in sorted(sm_all):
        rust = corpus.find(sm)
        rm_users = sorted(r for r, s in rm_groups.items() if sm in s)
        reach = outbound_reachability(rm_users, rm_producers)
        rows.append(dict(bucket="outbound", key="outbound:" + sm, name=sm,
                         oracle="(RM→SM)", rust=("yes" if rust else "no"),
                         rust_at=(f"{Path(rust[0]).name}:{rust[1]}" if rust else "-"),
                         status=dispatch_item_status("outbound", sm, bool(rust), cards, reachable=reach),
                         card=card_for(cards, "outbound:" + sm),
                         reach=("yes" if reach else ("no" if reach is False else "-")),
                         emitted_by=",".join(rm_users[:4]) + ("…" if len(rm_users) > 4 else "")))
    return rows, cm, rm_groups, sm_all, rm_producers



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
    prereq_rc = emit_oracle_prereq(oracle_prereq_problems(repo, [ORACLE_CONTENT]))
    if prereq_rc is not None:
        return prereq_rc
    prereq_rc = emit_deploy_prereq(deploy_prereq_problems(envir_dir, notice_dir), args.allow_missing_deploy)
    if prereq_rc is not None:
        return prereq_rc
    cards = resolve_cards(load_cards(Path(args.cards)), repo)
    card_rc = emit_card_prereq(cards)
    rows = build_content_rows(repo, envir_dir, notice_dir, cards)
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
        emit_json(args.json, dict(kind="content", counts=counts, rows=rows,
                                  missing_cards=cards.get("_missing_cards", [])))
    return 1 if (reds or card_rc) else 0


def cmd_dispatch(args):
    repo = Path(args.repo)
    prereq_rc = emit_oracle_prereq(oracle_prereq_problems(repo, [ORACLE_DISPATCH]))
    if prereq_rc is not None:
        return prereq_rc
    cards = resolve_cards(load_cards(Path(args.cards)), repo)
    card_rc = emit_card_prereq(cards)
    rows, cm, rm_groups, sm_all, rm_producers = build_dispatch_rows(repo, cards)
    counts, reds = summarize(rows)
    print("== 协议派发对账门（§4-2）==")
    print(f"oracle={repo / ORACLE_DISPATCH}")
    print(f"inbound case 全集={len(cm)}  outbound SM 帧全集={len(sm_all)}（来自 {len(rm_groups)} 个 RM 支；"
          f"src/** 生产者 RM 数={len(rm_producers)}）")
    print(f"cards={args.cards}")
    print()
    print_table(rows, ["bucket", "name", "oracle", "rust", "rust_at", "reach", "noop", "status", "card", "emitted_by"])
    if args.show_rm:
        print()
        print("-- RM→SM 映射（advisory，供 R2 定点；← 死支 = 全仓每一处出现都只是 const 定义/case 标签）--")
        for rm in sorted(rm_groups):
            mark = "" if rm in rm_producers else "   ← 死支（确证无生产者）"
            print(f"  {rm:28s} -> {', '.join(sorted(rm_groups[rm])) or '(无 SM)'}{mark}")
    print()
    print("summary:", json.dumps(counts, ensure_ascii=False))
    if args.json:
        emit_json(args.json, dict(kind="dispatch", counts=counts, rows=rows,
                                  rm_to_sm={k: sorted(v) for k, v in rm_groups.items()},
                                  rm_producers=sorted(rm_producers),
                                  missing_cards=cards.get("_missing_cards", [])))
    return 1 if (reds or card_rc) else 0


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

    # ---- dispatch: RM 可达性判据（C 类死支不红）----
    _prods = {"RM_ALIVE", "RM_WALK"}
    check("reach：触发 RM 有生产者用法 ⇒ 可达", outbound_reachability(["RM_ALIVE"], _prods) is True)
    check("reach：触发 RM 全无生产者（确证死支）⇒ 不可达", outbound_reachability(["RM_DEADBRANCH"], _prods) is False)
    check("reach：无 RM 生产者 ⇒ 不可判（None，保守不豁免）", outbound_reachability([], _prods) is None)
    check("teeth：真不可达 outbound 帧（RM 无发送点 + 无卡）⇒ n/a-unreachable（不红）",
          dispatch_item_status("outbound", "SM_DEADBRANCH", False,
                               {"known_cards": [], "assignments": {}}, reachable=False) == "n/a-unreachable")
    check("teeth：可达 outbound 帧 + Rust 无编码器 + 无卡 ⇒ RED",
          dispatch_item_status("outbound", "SM_LIVE", False,
                               {"known_cards": [], "assignments": {}}, reachable=True) == "RED")
    check("teeth：可达性 None（无生产者）不豁免 ⇒ RED",
          dispatch_item_status("outbound", "SM_UNK", False,
                               {"known_cards": [], "assignments": {}}, reachable=None) == "RED")
    check("teeth：人工豁免登记（known_unreachable）⇒ n/a-unreachable",
          dispatch_item_status("outbound", "SM_MANUAL", False,
                               {"known_cards": [], "assignments": {},
                                "known_unreachable": ["outbound:SM_MANUAL"]}) == "n/a-unreachable")

    # 端到端合成：死支 RM 帧不红、可达 RM 帧必红（门真读 src/** 发送点，非只看 oracle case）
    _tmp_reach = Path(tempfile.mkdtemp(prefix="recon-reach-"))
    try:
        (_tmp_reach / "src/M2Server/Player").mkdir(parents=True)
        (_tmp_reach / ORACLE_DISPATCH).write_text(NL.join([
            "switch (w) {",
            "  case Messages.RM_ZZLIVE:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZLIVE, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZDEAD:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZDEAD, 0, 0, 0, 0); break;",
            "}",
        ]) + NL, encoding="utf-8")
        (_tmp_reach / "src/M2Server/Other.cs").write_text(
            "p.SendMsg(Messages.RM_ZZLIVE, 0, 0, 0, 0);" + NL, encoding="utf-8")
        (_tmp_reach / "mir2-rs/bins/game-svr/src").mkdir(parents=True, exist_ok=True)
        _rows_r, _, _, _, _senders_r = build_dispatch_rows(
            _tmp_reach, {"known_cards": [], "assignments": {}})
        _st_r = {r["name"]: r["status"] for r in _rows_r}
    finally:
        shutil.rmtree(_tmp_reach, ignore_errors=True)
    check("teeth：端到端 RM_ZZLIVE 被判为发送点", "RM_ZZLIVE" in _senders_r)
    check("teeth：端到端 死支帧 SM_ZZDEAD ⇒ n/a-unreachable（不红）", _st_r.get("SM_ZZDEAD") == "n/a-unreachable")
    check("teeth：端到端 可达帧 SM_ZZLIVE + 无编码器 ⇒ RED", _st_r.get("SM_ZZLIVE") == "RED")

    # ---- dispatch: RM 可达性**保守口径**（对抗验证 v3 修正：动态形参 / 辅助函数间接发送不漏）----
    _tmp_dyn = Path(tempfile.mkdtemp(prefix="recon-reach-dyn-"))
    try:
        (_tmp_dyn / "src/M2Server/Player").mkdir(parents=True)
        (_tmp_dyn / ORACLE_DISPATCH).write_text(NL.join([
            "switch (w) {",
            "  case Messages.RM_ZZWALK:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZWALK, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZHIT:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZHIT, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZDEAD2:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZDEAD2, 0, 0, 0, 0); break;",
            "}",
        ]) + NL, encoding="utf-8")
        (_tmp_dyn / "src/M2Server/Helper.cs").write_text(
            "if (Walk(Messages.RM_ZZWALK)) { }" + NL                     # 辅助函数实参（本行无 Send*(）
            + "int wIdent = Messages.RM_ZZHIT;" + NL                     # 动态形参来源
            + "p.SendRefMsg(wIdent, Dir, X, Y, 0, \"\");" + NL          # 下游发送（本行无 RM 字面量）
            + "public const int RM_ZZDEAD2 = 9999;" + NL, encoding="utf-8")  # 确证死支：仅 const + case
        (_tmp_dyn / "mir2-rs/bins/game-svr/src").mkdir(parents=True, exist_ok=True)
        _rows_dyn, _, _, _, _prod_dyn = build_dispatch_rows(
            _tmp_dyn, {"known_cards": [], "assignments": {}})
        _st_dyn = {r["name"]: r["status"] for r in _rows_dyn}
    finally:
        shutil.rmtree(_tmp_dyn, ignore_errors=True)
    check("reach：辅助函数形参（Walk(RM_ZZWALK)）⇒ 生产者（旧「同行 Send*(」口径会误判死支）",
          "RM_ZZWALK" in _prod_dyn)
    check("reach：动态形参（wIdent=RM_ZZHIT; SendRefMsg(wIdent,…)）⇒ 生产者（旧口径会误判死支）",
          "RM_ZZHIT" in _prod_dyn)
    check("reach：确证死支（仅 const 定义 + case 标签）⇒ 非生产者", "RM_ZZDEAD2" not in _prod_dyn)
    check("teeth：辅助函数形参可达帧 SM_ZZWALK + 无编码器 + 无卡 ⇒ RED（不误豁免）",
          _st_dyn.get("SM_ZZWALK") == "RED")
    check("teeth：动态形参可达帧 SM_ZZHIT + 无编码器 + 无卡 ⇒ RED（不误豁免）",
          _st_dyn.get("SM_ZZHIT") == "RED")
    check("teeth：确证死支帧 SM_ZZDEAD2 ⇒ n/a-unreachable（不红）",
          _st_dyn.get("SM_ZZDEAD2") == "n/a-unreachable")

    # ---- content: 「真装载调用」判据（收紧旧「文件名串出现」口径；片 D1-① 加死注册表牙）----
    _tmp_ct = Path(tempfile.mkdtemp(prefix="recon-content-"))
    try:
        _cdir = _tmp_ct / "mir2-rs/crates/envir/src"
        _cdir.mkdir(parents=True)
        _bdir = _tmp_ct / "mir2-rs/bins/game-svr/src"
        _bdir.mkdir(parents=True)
        _bw = _bdir / "wiring.rs"
        _bw.write_text("pub fn boot() {}" + NL, encoding="utf-8")
        _f = _cdir / "minimap.rs"
        _f.write_text("// MiniMap.txt 仅注释提及" + NL + "pub fn x() {}" + NL, encoding="utf-8")
        check("teeth：内容=注释-only ⇒ 不算实现",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is False)
        _f.write_text('pub const FAKE: &str = "MiniMap.txt";' + NL + "pub struct Marker;" + NL, encoding="utf-8")
        check("teeth：内容=文件名串（精确引号字面量）无装载调用 ⇒ 不算实现（旧口径会假绿）",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is False)
        # 片 D1-①（死注册表牙）：文件名串 + 真装载调用，但装载函数只在 crate 内、二进制侧零引用
        # ⇒ 是死注册表 ⇒ **不算实现**（旧口径会假绿成 implemented）。
        _f.write_text('pub const F: &str = "MiniMap.txt";' + NL + "pub fn load_minimap() {}" + NL, encoding="utf-8")
        check("teeth：文件名串 + 真装载调用 但二进制零引用（死注册表）⇒ 不算实现（旧口径假绿）",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is False)
        # 接线（bins 引用该装载函数）后 ⇒ 实现
        _bw.write_text("mir2_envir::load_minimap();" + NL, encoding="utf-8")
        check("teeth：装载函数被 bins 接线 ⇒ 实现",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is True)
        # 镜像模块路径（minimap.rs 含函数、无 load 名）+ 模块名被 bins 引用 ⇒ 实现
        _f.write_text('pub const F: &str = "MiniMap.txt";' + NL + "pub fn parse_minimap() {}" + NL, encoding="utf-8")
        _bw.write_text("use mir2_envir::minimap;" + NL, encoding="utf-8")
        check("teeth：镜像模块路径（minimap.rs 含函数，无 load 名）+ 模块被 bins 引用 ⇒ 实现",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is True)
        # 未接线的镜像模块 ⇒ 不算实现
        _bw.write_text("pub fn boot() {}" + NL, encoding="utf-8")
        check("teeth：镜像模块路径但模块未被 bins 引用（死注册表）⇒ 不算实现",
              content_rust_implemented(RustCorpus(rust_source_dirs(_tmp_ct)), "MiniMap.txt") is False)
    finally:
        shutil.rmtree(_tmp_ct, ignore_errors=True)
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

    # ---- 片 D1-⑤：空部署目录 ⇒ PREREQ_RED rc=2（旧版 fail-open rc=0）----
    _tmp_empty = Path(tempfile.mkdtemp(prefix="recon-empty-"))
    try:
        _ee, _en = _tmp_empty / "Envir", _tmp_empty / "Notice"
        _ee.mkdir(); _en.mkdir()
        check("deploy 前置：存在但为空的部署目录被识别为问题（2 条）",
              len(deploy_prereq_problems(_ee, _en)) == 2)
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc_empty = main(["content", "--envir-dir", str(_ee), "--notice-dir", str(_en)])
        out_empty = buf.getvalue()
        check("teeth：content 空部署目录 ⇒ rc=2 且 PREREQ_MISSING + RESULT: PREREQ_RED（非 fail-open rc=0）",
              rc_empty == 2 and "PREREQ_MISSING" in out_empty and "RESULT: PREREQ_RED" in out_empty)
    finally:
        shutil.rmtree(_tmp_empty, ignore_errors=True)

    # ---- 片 D1-⑥：缺 oracle 文件 ⇒ PREREQ_RED rc=2（旧版 traceback + rc=1）----
    _no_repo = Path("E:/no-such-repo-zzz")
    for _sub, _args in (("dispatch", ["dispatch", "--repo", str(_no_repo)]),
                        ("content", ["content", "--repo", str(_no_repo)])):
        buf = io.StringIO()
        with contextlib.redirect_stdout(buf):
            rc_no_oracle = main(_args)
        out_no_oracle = buf.getvalue()
        check("teeth（片D1-⑥）：%s 缺 oracle 文件 ⇒ rc=2（非 traceback/rc1）且 PREREQ_MISSING + RESULT: PREREQ_RED" % _sub,
              rc_no_oracle == 2 and "PREREQ_MISSING" in out_no_oracle and "RESULT: PREREQ_RED" in out_no_oracle)

    # ---- 片 D1-②：入站 no-op 臂（恒假/恒空）判据 + 状态降级 ----
    _tmp_noop = Path(tempfile.mkdtemp(prefix="recon-noop-"))
    try:
        (_tmp_noop / "mir2-rs/bins/game-svr/src").mkdir(parents=True)
        (_tmp_noop / "mir2-rs/bins/game-svr/src/main.rs").write_text(NL.join([
            "match ident {",
            "  messages::CM_ZZDEAD => { let (d, _f) = client_open_door(false); glog!(\"x\"); }",
            "  messages::CM_ZZDEAD2 => { let f = client_get_min_map(0); emit(f); }",
            "  messages::CM_ZZLIVE => { let i = client_live(cmd.param); emit(i); }",
            "}",
        ]) + NL, encoding="utf-8")
        _nc = RustCorpus(rust_source_dirs(_tmp_noop))
        check("teeth（片D1-②）：no-op 臂（绑 `_` 丢弃 handler 输出）⇒ inbound_arm_noop=True",
              inbound_arm_noop(_nc, "CM_ZZDEAD") is True)
        check("teeth（片D1-②）：no-op 臂（常量喂 client_* handler，恒走 FAIL 支）⇒ inbound_arm_noop=True",
              inbound_arm_noop(_nc, "CM_ZZDEAD2") is True)
        check("对照（片D1-②）：动态入参 + 出帧臂 ⇒ inbound_arm_noop=False",
              inbound_arm_noop(_nc, "CM_ZZLIVE") is False)
    finally:
        shutil.rmtree(_tmp_noop, ignore_errors=True)
    check("teeth（片D1-②）：no-op 臂虽 rust=yes ⇒ 不判 implemented（已卡 ⇒ tracked）",
          dispatch_item_status("inbound", "CM_CARDED", True, cards, noop=True) == "tracked")
    check("对照（片D1-②）：非 no-op 臂 rust=yes ⇒ implemented",
          dispatch_item_status("inbound", "CM_CARDED", True, cards, noop=False) == "implemented")

    # ---- 片 D1-①：oracle-noop 证据桶（豁免必须逐条带证据行）----
    check("teeth（片D1-①）：known_oracle_noop 带证据行 ⇒ n/a-oracle-noop（不判 implemented）",
          content_row_status(dict(kind="envir", file="X.txt"), deployed=True, rust=True,
                             cards={"known_oracle_noop": {"content:X.txt": "src/A.cs:12"}}) == "n/a-oracle-noop")
    check("teeth（片D1-①）：known_oracle_noop 证据为空 ⇒ 不生效（rust 无 ⇒ RED）",
          content_row_status(dict(kind="envir", file="X.txt"), deployed=True, rust=False,
                             cards={"known_oracle_noop": {"content:X.txt": ""}}) == "RED")
    check("teeth（片D1-①）：known_oracle_noop 证据无 `file:line` ⇒ 不生效（落回 tracked）",
          content_row_status(dict(kind="envir", file="Carded.txt"), deployed=True, rust=False,
                             cards={"known_cards": ["openmir2-envir-redlist-batch"],
                                    "assignments": {"content:Carded.txt": "openmir2-envir-redlist-batch"},
                                    "known_oracle_noop": {"content:Carded.txt": "看代码就知道是 no-op"}}) == "tracked")
    check("teeth（片D1-①）：known_dead_consumer（装载已接二进制、消费延后）⇒ 不判 implemented（落 tracked）",
          content_row_status(dict(kind="envir", file="Carded.txt"), deployed=True, rust=True, dead=True,
                             cards={"known_cards": ["openmir2-envir-redlist-batch"],
                                    "assignments": {"content:Carded.txt": "openmir2-envir-redlist-batch"},
                                    "known_dead_consumer": {"content:Carded.txt": "src/m.rs:9"}}) == "tracked")
    check("teeth（片D1-④）：derived 内容项（无独立件、随地图加载）Rust 无 ⇒ RED（不再静默 n/a-other）",
          content_row_status(dict(kind="derived", anchor="x"), deployed=True, rust=False,
                             cards={"known_cards": [], "assignments": {}}) == "RED")

    # ---- 片 D1-④ 接线牙：走**整表路径**（CONTENT_MAP 注册 → _make_content_row → status）----
    # 只测 content_row_status(kind="derived") 覆盖不到「把 CONTENT_MAP 的 LoadMapDoor 注册改回
    # kind=other」这种回退：那样 deployed 会变 False，整行静默落 n/a-other（RED 消失）而 selftest 仍绿。
    class _StubCorpus:
        def __init__(self, has: bool):
            self._has = has

        def present(self, ident):
            return self._has

    _map_door = CONTENT_MAP["LoadMapDoor"]
    check("teeth（片D1-④ 接线）：CONTENT_MAP['LoadMapDoor'] 注册为 derived + probe_ident",
          _map_door.get("kind") == "derived" and _map_door.get("probe_ident") == "add_map_door")
    _row_no = _make_content_row(Path("."), Path("."), Path("."), _StubCorpus(False),
                                {"known_cards": [], "assignments": {}}, _map_door, "MapManager.cs:182")
    check("teeth（片D1-④ 接线）：整表路径 —— Rust 无 probe_ident ⇒ RED（部署恒真）",
          _row_no["status"] == "RED" and _row_no["kind"] == "derived" and _row_no["deployed"] == "yes")
    _row_yes = _make_content_row(Path("."), Path("."), Path("."), _StubCorpus(True),
                                 {"known_cards": [], "assignments": {}}, _map_door, "MapManager.cs:182")
    check("teeth（片D1-④ 接线）：整表路径 —— Rust 有 probe_ident ⇒ implemented（不误报）",
          _row_yes["status"] == "implemented")

    # ---- 片 D1-③：cards.json 引用看板上不存在的卡 ⇒ RED + 点名 ----
    _cards_bogus = {"known_cards": ["openmir2-real-zzz", "openmir2-bogus-zzz"],
                    "assignments": {"content:SomeNew.txt": "openmir2-bogus-zzz"}}
    resolve_cards(_cards_bogus, Path("."), board_lookup=lambda r: {"openmir2-real-zzz"})
    check("teeth（片D1-③）：引用看板不存在的卡 ⇒ 记入 _missing_cards（点名）",
          _cards_bogus["_missing_cards"] == ["openmir2-bogus-zzz"])
    check("teeth（片D1-③）：指向不存在卡的缺口 ⇒ card_for=None（按未立卡 = RED）",
          card_for(_cards_bogus, "content:SomeNew.txt") is None)
    buf = io.StringIO()
    with contextlib.redirect_stdout(buf):
        _card_rc = emit_card_prereq(_cards_bogus)
    check("teeth（片D1-③）：缺卡 ⇒ emit_card_prereq 返回 1 且印 PREREQ_CARD_MISSING + 点名 bogus 卡",
          _card_rc == 1 and "PREREQ_CARD_MISSING" in buf.getvalue() and "openmir2-bogus-zzz" in buf.getvalue())
    _cards_none = {"known_cards": ["openmir2-x"], "assignments": {}}
    resolve_cards(_cards_none, Path("."), board_lookup=lambda r: None)
    check("对照（片D1-③）：看板不可用 ⇒ 不判缺卡（_missing_cards 空、_board_probed=False）",
          _cards_none["_missing_cards"] == [] and _cards_none["_board_probed"] is False)

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
    prereq_rc = emit_oracle_prereq(oracle_prereq_problems(repo, [ORACLE_CONTENT, ORACLE_DISPATCH]))
    if prereq_rc is not None:
        return prereq_rc
    prereq_rc = emit_deploy_prereq(deploy_prereq_problems(Path(args.envir_dir), Path(args.notice_dir)),
                                  args.allow_missing_deploy)
    if prereq_rc is not None:
        return prereq_rc
    cards = resolve_cards(load_cards(Path(args.cards)), repo)
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


    # M4 —— RM 可达性判据（合成 oracle：一个死支 RM + 一个可达 RM；门须区分）
    _tmp_m4 = Path(tempfile.mkdtemp(prefix="recon-reach-mut-"))
    try:
        (_tmp_m4 / "src/M2Server/Player").mkdir(parents=True)
        (_tmp_m4 / ORACLE_DISPATCH).write_text(NL.join([
            "switch (w) {",
            "  case Messages.RM_ZZLIVE:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZLIVE, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZDEAD:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZDEAD, 0, 0, 0, 0); break;",
            "}",
        ]) + NL, encoding="utf-8")
        (_tmp_m4 / "src/M2Server/Other.cs").write_text(
            "p.SendMsg(Messages.RM_ZZLIVE, 0, 0, 0, 0);" + NL, encoding="utf-8")
        (_tmp_m4 / "mir2-rs/bins/game-svr/src").mkdir(parents=True, exist_ok=True)
        _rows_m4, _, _, _, _ = build_dispatch_rows(_tmp_m4, {"known_cards": [], "assignments": {}})
        _st_m4 = {r["name"]: r["status"] for r in _rows_m4}
        add("M4a 死支 RM（全仓无发送点）帧 SM_ZZDEAD ⇒ n/a-unreachable（不红）",
            _st_m4.get("SM_ZZDEAD") == "n/a-unreachable")
        add("M4b 可达 RM_ZZLIVE 帧 + Rust 无编码器 ⇒ RED（可达性不误豁免）",
            _st_m4.get("SM_ZZLIVE") == "RED")
    finally:
        shutil.rmtree(_tmp_m4, ignore_errors=True)

    # M4c —— 真语料回归（对抗验证 v3）：经动态形参 / 辅助函数间接发出的 RM 必须判「可达」。
    #         旧口径「整行同时含 Send*( 与该 RM 字面量」把这 10 条真可达帧误判死支 ⇒ 静默豁免。
    _prod_real = oracle_rm_producers(repo)
    _dyn_real = ["RM_WALK", "RM_RUN", "RM_HORSERUN", "RM_BIGHIT", "RM_SPELL2",
                 "RM_WIDEHIT", "RM_LONGHIT", "RM_FIREHIT", "RM_CRSHIT", "RM_TWINHIT"]
    _miss = [r for r in _dyn_real if r not in _prod_real]
    add("M4c 真语料：10 条经动态形参/辅助函数发出的 RM 全判可达（缺=%s）" % (_miss or "无"),
        not _miss)

    # M4d —— 合成动态形参 / 辅助函数形态：无 Rust 编码器 + 无卡 ⇒ 必 RED（不误豁免）；确证死支 ⇒ 不红。
    _tmp_m4d = Path(tempfile.mkdtemp(prefix="recon-reach-mut-dyn-"))
    try:
        (_tmp_m4d / "src/M2Server/Player").mkdir(parents=True)
        (_tmp_m4d / ORACLE_DISPATCH).write_text(NL.join([
            "switch (w) {",
            "  case Messages.RM_ZZWALK:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZWALK, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZHIT:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZHIT, 0, 0, 0, 0); break;",
            "  case Messages.RM_ZZDEAD2:",
            "    ClientMsg = Messages.MakeMessage(Messages.SM_ZZDEAD2, 0, 0, 0, 0); break;",
            "}",
        ]) + NL, encoding="utf-8")
        (_tmp_m4d / "src/M2Server/Helper.cs").write_text(
            "if (Walk(Messages.RM_ZZWALK)) { }" + NL
            + "int wIdent = Messages.RM_ZZHIT;" + NL
            + "p.SendRefMsg(wIdent, Dir, X, Y, 0, \"\");" + NL
            + "public const int RM_ZZDEAD2 = 9999;" + NL, encoding="utf-8")
        (_tmp_m4d / "mir2-rs/bins/game-svr/src").mkdir(parents=True, exist_ok=True)
        _rows_m4d, _, _, _, _prod_m4d = build_dispatch_rows(_tmp_m4d, {"known_cards": [], "assignments": {}})
        _st_m4d = {r["name"]: r["status"] for r in _rows_m4d}
        add("M4d 辅助函数形参 Walk(RM_ZZWALK) ⇒ 生产者 + 无编码器 ⇒ RED（不误豁免）",
            "RM_ZZWALK" in _prod_m4d and _st_m4d.get("SM_ZZWALK") == "RED")
        add("M4d 动态形参 wIdent=RM_ZZHIT; SendRefMsg(wIdent,…) ⇒ 生产者 + 无编码器 ⇒ RED（不误豁免）",
            "RM_ZZHIT" in _prod_m4d and _st_m4d.get("SM_ZZHIT") == "RED")
        add("M4d 确证死支（仅 const 定义 + case 标签）RM_ZZDEAD2 ⇒ 非生产者 ⇒ n/a-unreachable（不红）",
            "RM_ZZDEAD2" not in _prod_m4d and _st_m4d.get("SM_ZZDEAD2") == "n/a-unreachable")
    finally:
        shutil.rmtree(_tmp_m4d, ignore_errors=True)

    # M5 —— 内容门「真装载调用」收紧：文件名串出现但无装载调用 ⇒ 必红（旧「文件名串出现」口径会假绿）；
    #        且装载点必须被 bins 接线（死注册表不算实现，片 D1-①）。
    _tmp_m5 = Path(tempfile.mkdtemp(prefix="recon-content-mut-"))
    try:
        (_tmp_m5 / "src/GameSrv").mkdir(parents=True)
        shutil.copy(repo / ORACLE_CONTENT, _tmp_m5 / "src/GameSrv/GameApp.cs")
        _cdir5 = _tmp_m5 / "mir2-rs/crates/envir/src"
        _cdir5.mkdir(parents=True)
        _bdir5 = _tmp_m5 / "mir2-rs/bins/game-svr/src"
        _bdir5.mkdir(parents=True)
        _bw5 = _bdir5 / "wiring.rs"
        _bw5.write_text("pub fn boot() {}" + NL, encoding="utf-8")
        (_cdir5 / "guardlist.rs").write_text(
            'pub const FAKE: &str = "GuardList.txt";' + NL + "pub struct G;" + NL, encoding="utf-8")
        _rows_m5 = build_content_rows(_tmp_m5, Path(args.envir_dir), Path(args.notice_dir), cards)
        _gr5 = [r for r in _rows_m5 if r.get("file") == "GuardList.txt"]
        add("M5 内容门收紧：文件名串出现但无装载调用 ⇒ GuardList.txt 必红（旧口径会假绿）",
            bool(_gr5) and _gr5[0]["status"] == "RED")
        # 有装载调用但二进制零引用 ⇒ 死注册表 ⇒ 仍红（片 D1-①）
        (_cdir5 / "guardlist.rs").write_text(
            'pub const F: &str = "GuardList.txt";' + NL + "pub fn load_guard_list() {}" + NL,
            encoding="utf-8")
        _rows_m5a = build_content_rows(_tmp_m5, Path(args.envir_dir), Path(args.notice_dir), cards)
        _gr5a = [r for r in _rows_m5a if r.get("file") == "GuardList.txt"]
        add("M5 死注册表：装载调用存在但未被 bins 接线 ⇒ GuardList.txt 仍红",
            bool(_gr5a) and _gr5a[0]["status"] == "RED")
        # 接线 ⇒ implemented
        _bw5.write_text("game_svr::guardlist::load_guard_list();" + NL, encoding="utf-8")
        _rows_m5b = build_content_rows(_tmp_m5, Path(args.envir_dir), Path(args.notice_dir), cards)
        _gr5b = [r for r in _rows_m5b if r.get("file") == "GuardList.txt"]
        add("M5 对照：文件名串 + 装载调用 + bins 接线 ⇒ GuardList.txt implemented",
            bool(_gr5b) and _gr5b[0]["status"] == "implemented")
    finally:
        shutil.rmtree(_tmp_m5, ignore_errors=True)

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
