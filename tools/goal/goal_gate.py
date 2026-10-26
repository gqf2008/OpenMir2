#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""goal_gate.py - OpenMir2 终点四条件机判（只读、零副作用、可 --json）。

把 `docs/leader-goal-20261023.md` §1 的四条终点条件变成一条可复算命令：

  条件1 脚本派发 100%      复算 `cargo run -p mir2-script-tool -- parse-stats ... --require-implemented`
                           stdout 的「语料派发目标覆盖」行（**绝不硬编码 106/106**）
  条件2 R 卡证据           `walgit collab board --format json` 枚举 tier=R 卡，逐张判
                           「已合并 / 有 window_required / 有 whitelist T-* 登记 / 三者皆无」
  条件3 B 类清零           `tests/parity/coverage/recon_gates.py dispatch` 的 tracked 清单，
                           逐条判「有 T-* 登记 / 有卡 / 皆无」
  条件4 门禁恒绿           fmt / clippy / test --workspace / msgcodegen / anchor gate /
                           registry audit，逐项 rc；外加 `git ls-remote walgit refs/heads/master`
                           与本地 master 比对

三态语义（输出只允许这三个）：
  PASS     判据机判成立（有实测证据，不是猜的）
  FAIL     判据机判不成立（列出具体缺口）
  需人工   脚本无法机判（缺输入 / 未跑 / 证据链不可信），detail 写明缺什么

只读承诺：不起栈、不连数据库、不占端口、不写库、不改仓库。
  * 所有 git 只读命令（for-each-ref / cat-file / rev-parse / ls-remote）；
  * cargo 一律带 `--locked`（防止改写 Cargo.lock），构建产物只落 target/（gitignored）；
  * recon_gates.py 的 --json 输出写到系统临时目录并在读完后删除；
  * 绝不 `git fetch`（fetch 会写 refs），本地协作视图的新鲜度由调用方负责，见 §5。

用法：
  python tools/goal/goal_gate.py                 # 快速模式（默认；跳过 cargo test 与 parse-stats 构建）
  python tools/goal/goal_gate.py --json          # 机器可读（stdout 只有 JSON）
  python tools/goal/goal_gate.py --full          # 完整模式（= --tests --script）
  python tools/goal/goal_gate.py selftest        # 合成输入自检（PASS/FAIL/需人工 分支 + 牙）

退出码：0 = 四条全 PASS；1 = 有 FAIL；2 = 无 FAIL 但有「需人工」；3 = 脚本自身错误。
"""

from __future__ import annotations

import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

PASS = "pass"
FAIL = "fail"
NEED = "need_human"
NOT_MERGED = "not_merged"  # 内部第4态：未开工/未合并，不计 PASS 也不计 FAIL
STATE_LABEL = {PASS: "PASS", FAIL: "FAIL", NEED: "需人工", NOT_MERGED: "未合并"}

DEFAULT_ENVIR = "E:/MirServer/M2GameSvr/Envir"

# 条件1 的复算口径：parse-stats 覆盖报告行（见 docs/rewrite-remaining-inventory-20261022.md §1.2）
COVERAGE_RE = re.compile(
    r"语料派发目标覆盖[：:]\s*已实现\s*(\d+)\s*个[（(]\s*(\d+)\s*次调用[）)]\s*[；;]\s*未实现\s*(\d+)\s*个"
)

CARD_ID_RE = re.compile(r"openmir2-[A-Za-z0-9](?:[A-Za-z0-9._-]*[A-Za-z0-9])?")
# 卡号「归属」写法：紧跟「卡」/「卡片」的 id（可带冒号/空格/反引号）；行内其它 id 视为**引用**。
CARD_OWNER_RE = re.compile(r"卡(?:片)? *[:：]? *`?(" + CARD_ID_RE.pattern + r")`?")
# 引用线索：出现在「卡」前 REF_CUE_WINDOW 个字符内 ⇒ 不是本行卡（见/承接/原卡/已由/立卡…）。
# 「作者登记（`principal`，`card`, tier=R）」也是归属写法（老行不写「卡」字）。
CARD_REGISTER_GROUP_RE = re.compile(r"(?:作者登记|登记)\s*（([^）]*)）")
REF_CUES = ("见", "原", "已由", "承接", "立卡", "引用", "占用", "详见", "另见")
REF_CUE_WINDOW = 6
T_ID_RE = re.compile(r"\bT-\d+\b")


def _decode(raw: bytes) -> str:
    """脚本/rust 输出混用 UTF-8 与 GBK，按 UTF-8 -> GBK -> 替换 顺序解码。"""
    if not raw:
        return ""
    for enc in ("utf-8", "gbk"):
        try:
            return raw.decode(enc)
        except UnicodeDecodeError:
            continue
    return raw.decode("utf-8", "replace")


def run_cmd(cmd, cwd, timeout):
    """运行一个只读命令，返回记录（不抛异常，超时/缺失都变成状态）。"""
    cmd = [str(c) for c in cmd]
    rec = {"cmd": cmd, "cwd": str(cwd), "rc": None, "stdout": "", "stderr": "",
           "timed_out": False, "error": None, "duration_s": 0.0}
    started = time.monotonic()
    try:
        proc = subprocess.run(cmd, cwd=str(cwd), capture_output=True, timeout=timeout)
        rec["rc"] = proc.returncode
        rec["stdout"] = _decode(proc.stdout)
        rec["stderr"] = _decode(proc.stderr)
    except subprocess.TimeoutExpired as exc:
        rec["timed_out"] = True
        rec["stdout"] = _decode(exc.stdout or b"")
        rec["stderr"] = _decode(exc.stderr or b"")
    except OSError as exc:  # FileNotFoundError / PermissionError ...
        rec["error"] = str(exc)
    rec["duration_s"] = round(time.monotonic() - started, 2)
    return rec


def git_bytes(repo: Path, args, stdin_bytes=None, timeout=600):
    """只读 git 调用，返回 (stdout_bytes|None, rc, stderr_text)。"""
    try:
        proc = subprocess.run(["git", "-C", str(repo), *args], capture_output=True,
                              input=stdin_bytes, timeout=timeout)
        if proc.returncode != 0:
            return None, proc.returncode, _decode(proc.stderr)
        return proc.stdout, 0, ""
    except subprocess.TimeoutExpired:
        return None, -1, "timeout"
    except OSError as exc:
        return None, -1, str(exc)


# --------------------------------------------------------------------------
# 条件1：脚本派发覆盖率（复算 parse-stats 的覆盖行）
# --------------------------------------------------------------------------

CORPUS_COUNTER_RE = re.compile(r"文件:\s*(\d+)\s*加载:\s*(\d+)\s*缺失:\s*(\d+)\s*异常:\s*(\d+)")
DECODE_ABNORMAL_RE = re.compile(r"解码异常文件[:：]\s*(\d+)")


def parse_coverage_line(text: str):
    """从 stdout 抽「已实现 N 个（M 次调用）；未实现 K 个」。抽不到返回 None。"""
    match = COVERAGE_RE.search(text or "")
    if not match:
        return None
    return {"implemented": int(match.group(1)),
            "calls": int(match.group(2)),
            "unimplemented": int(match.group(3))}


def parse_corpus_counters(text: str):
    """parse-stats 的语料完整性计数（缺失/异常/解码异常）。抽不到返回 None。

    语料缺文件/解码异常会让「参与统计的目标」变少，此时即使打印「未实现 0 个」也不能算 PASS。
    """
    text = text or ""
    file_count = CORPUS_COUNTER_RE.search(text)
    decode_bad = DECODE_ABNORMAL_RE.search(text)
    if not file_count and not decode_bad:
        return None
    out = {"files": None, "loaded": None, "missing": None, "abnormal": None, "decode_abnormal": None}
    if file_count:
        out.update(files=int(file_count.group(1)), loaded=int(file_count.group(2)),
                   missing=int(file_count.group(3)), abnormal=int(file_count.group(4)))
    if decode_bad:
        out["decode_abnormal"] = int(decode_bad.group(1))
    return out


def judge_script_coverage(rec, parsed, counters=None):
    """纯判定：PASS/FAIL/需人工。rec 为 run_cmd 记录，parsed 为覆盖行，counters 为语料完整性计数。"""
    if rec is None:
        return NEED, "未运行 parse-stats（快速模式未启用 --script，或用 --script-bin 指定预编译二进制）"
    if rec.get("timed_out"):
        return NEED, "parse-stats 超时，覆盖结论未知"
    if rec.get("error"):
        return NEED, "parse-stats 无法启动：%s" % rec["error"]
    if parsed is None:
        return NEED, "stdout 里没有「语料派发目标覆盖」行（rc=%s），覆盖结论未知" % rec.get("rc")
    if parsed["unimplemented"] > 0:
        return FAIL, "语料派发目标未实现 %d 个（目标 %d/%d；调用 %d 次）" % (
            parsed["unimplemented"], parsed["implemented"],
            parsed["implemented"] + parsed["unimplemented"], parsed["calls"])
    if parsed["implemented"] <= 0:
        return NEED, "覆盖行 implemented=0，疑似语料路径/解析异常"
    if rec.get("rc") != 0:
        return FAIL, "覆盖行未实现=0，但 parse-stats rc=%s（--require-implemented 失败/语料异常）" % rec.get("rc")
    if counters is None:
        return NEED, "stdout 里没有语料完整性计数（文件/加载/缺失/异常），无法确认统计面完整"
    corpus_bad = []
    if counters.get("missing"):
        corpus_bad.append("缺失 %d" % counters["missing"])
    if counters.get("abnormal"):
        corpus_bad.append("异常 %d" % counters["abnormal"])
    if counters.get("decode_abnormal"):
        corpus_bad.append("解码异常文件 %d" % counters["decode_abnormal"])
    if corpus_bad:
        return NEED, "语料不完整（%s），参与统计的目标可能变少，覆盖结论不可信" % "；".join(corpus_bad)
    target = parsed["implemented"] + parsed["unimplemented"]
    return PASS, "parse-stats 实测：目标 %d/%d（去重派发目标）；调用 %d 次；未实现 0 个" % (
        parsed["implemented"], target, parsed["calls"])


def run_script_coverage(mir2_root: Path, envir: str, script_bin: str, timeout):
    """跑 parse-stats（只读语料；临时 json 落在系统临时目录并删除）。"""
    envir_path = Path(envir)
    if not envir_path.is_dir():
        empty = {"cmd": [], "cwd": str(mir2_root), "rc": None, "stdout": "", "stderr": "",
                 "timed_out": False, "error": None, "duration_s": 0.0}
        return empty, None, None, "语料目录不存在：%s（用 --envir 指定）" % envir
    tmpdir = Path(tempfile.mkdtemp(prefix="goal-gate-parsestats-"))
    json_out = tmpdir / "parsestats.json"
    try:
        if script_bin:
            cmd = [script_bin, "parse-stats", str(envir_path), "--json", str(json_out),
                   "--require-implemented"]
        else:
            cmd = ["cargo", "run", "--release", "--locked", "-p", "mir2-script-tool", "--",
                   "parse-stats", str(envir_path), "--json", str(json_out), "--require-implemented"]
        rec = run_cmd(cmd, mir2_root, timeout)
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)
    return rec, parse_coverage_line(rec.get("stdout", "")), parse_corpus_counters(rec.get("stdout", "")), None


# --------------------------------------------------------------------------
# 协作视图：board + 条目索引（只读 git 对象）
# --------------------------------------------------------------------------

def load_board(repo: Path, timeout=180):
    """`walgit collab board --format json` -> ({card_id: card}, rec)。"""
    rec = run_cmd(["walgit", "collab", "board", "--format", "json"], repo, timeout)
    if rec.get("rc") != 0:
        return None, rec
    try:
        data = json.loads(rec["stdout"])
    except (ValueError, TypeError) as exc:
        rec["error"] = "board json 解析失败：%s" % exc
        return None, rec
    cards = {}
    for column in data.get("columns", []) or []:
        for card in column.get("cards", []) or []:
            card = dict(card)
            card["_column"] = column.get("name")
            cards[card["id"]] = card
    return cards, rec


def _add_entry(index, seen, oid, entry):
    if not isinstance(entry, dict):
        return False
    thread_id = entry.get("id")
    if not thread_id:
        return False
    key = (thread_id, oid)
    if key in seen:
        return False
    seen.add(key)
    index.setdefault(thread_id, []).append((oid, entry))
    return True


def load_entry_index(repo: Path, timeout=900):
    """读 snapshot（折叠区）+ inbox tail（refs/collab/inbox/*），聚合为 {thread_id: [(oid, entry)]}。

    与 `walgit collab board` 同源（snapshot ∪ tail），但不做签名验证；
    信任度由 board 每卡的 verified/unverified 计数承担（见条件2）。
    """
    index = {}
    seen = set()
    meta = {"snapshot": False, "snapshot_entries": 0, "tail_refs": 0,
            "tail_entries": 0, "duplicates": 0, "notes": []}

    snap, rc, err = git_bytes(repo, ["rev-parse", "--verify", "refs/collab/meta/snapshot"], timeout=timeout)
    if rc == 0 and snap:
        blob, rc2, err2 = git_bytes(repo, ["cat-file", "-p", snap.decode().strip()], timeout=timeout)
        if rc2 == 0 and blob:
            try:
                data = json.loads(blob.decode("utf-8"))
                for item in data.get("entries", []) or []:
                    try:
                        entry = json.loads(item.get("json", "{}"))
                    except ValueError:
                        meta["notes"].append("snapshot 条目 json 解析失败：%s" % item.get("oid"))
                        continue
                    if _add_entry(index, seen, item.get("oid") or "", entry):
                        meta["snapshot_entries"] += 1
                meta["snapshot"] = True
            except ValueError as exc:
                meta["notes"].append("snapshot 解析失败：%s" % exc)
        else:
            meta["notes"].append("snapshot 读取失败：%s" % (err2 or rc2))
    elif rc != 0:
        meta["notes"].append("snapshot ref 不可用：%s" % err)

    refs_out, rc, err = git_bytes(
        repo, ["for-each-ref", "--format=%(objectname)", "refs/collab/inbox"], timeout=timeout)
    refs = []
    if rc == 0 and refs_out:
        refs = [line.strip() for line in refs_out.decode().splitlines() if line.strip()]
    elif rc != 0:
        meta["notes"].append("refs/collab/inbox 枚举失败：%s" % err)
    meta["tail_refs"] = len(refs)

    if refs:
        batch_out, rc, err = git_bytes(
            repo, ["cat-file", "--batch"], stdin_bytes=("\n".join(refs) + "\n").encode(), timeout=timeout)
        if rc == 0 and batch_out:
            pos, total = 0, len(batch_out)
            while pos < total:
                nl = batch_out.find(b"\n", pos)
                if nl < 0:
                    break
                header = batch_out[pos:nl].decode(errors="replace").split()
                pos = nl + 1
                if len(header) < 3 or header[1] != "blob":
                    continue
                size = int(header[2])
                body = batch_out[pos:pos + size]
                pos += size + 1
                try:
                    entry = json.loads(body.decode("utf-8"))
                except ValueError:
                    continue
                if _add_entry(index, seen, header[0], entry):
                    meta["tail_entries"] += 1
                else:
                    meta["duplicates"] += 1
        else:
            meta["notes"].append("cat-file --batch 失败：%s" % err)

    meta["threads"] = len(index)
    meta["entries"] = sum(len(v) for v in index.values())
    return index, meta


def tier_is_r(tier) -> bool:
    """tier 归一：'R' / 'R（战斗数值/时序/持久化面）...' 都算 R（问题单里出现过散文式 tier）。"""
    if not isinstance(tier, str):
        return False
    return tier.strip().lstrip("`'\"").upper().startswith("R")


# --------------------------------------------------------------------------
# whitelist.md：T-* 行 -> 关联卡；以及卡 -> T-*（行内点名）
# --------------------------------------------------------------------------

def load_principals(repo: Path, timeout=120):
    """协作身份名（refs/collab/meta/principals/*）；用来把 principal 从卡 id 匹配里排掉。"""
    out, rc, _err = git_bytes(
        repo, ["for-each-ref", "--format=%(refname)", "refs/collab/meta/principals"], timeout=timeout)
    if rc != 0 or not out:
        return set()
    names = set()
    for line in out.decode().splitlines():
        line = line.strip()
        if line:
            names.add(line.rsplit("/", 1)[-1])
    return names


def split_table_cells(line):
    """按「未被反斜杠转义、且不在 `代码段` 内」的竖线切列（markdown 表格）。

    whitelist 的单元格里大量出现 `` `a|b` `` 形式的代码段与反斜杠竖线转义；
    直接 str.split('|') 会把一行切成 8~13 列，是「列结构」判定的噪声源。
    """
    text = line.strip()
    if text.startswith("|"):
        text = text[1:]
    if text.endswith("|"):
        text = text[:-1]
    cells, cur, escaped, in_code = [], [], False, False
    for ch in text:
        if escaped:
            cur.append(ch)
            escaped = False
            continue
        if ch == chr(92):
            cur.append(ch)
            escaped = True
            continue
        if ch == "`":
            in_code = not in_code
            cur.append(ch)
            continue
        if ch == "|" and not in_code:
            cells.append("".join(cur).strip())
            cur = []
            continue
        cur.append(ch)
    cells.append("".join(cur).strip())
    return cells


def _ref_cued(text, pos):
    """pos 处的「卡」是否被引用线索修饰（见卡/承接卡/原卡/已由卡…）。"""
    head = text[max(0, pos - REF_CUE_WINDOW):pos]
    return any(cue in head for cue in REF_CUES)


def _owner_cards(cells, exclude_ids):
    """结构化取本行的**归属卡**（一行多卡 ⇒ 全部计入），不整行扫。

    规则（两条合取，逐条可复算）：
      1) 任何列里形如「卡 `openmir2-…`」的 id，且「卡」前 REF_CUE_WINDOW 字符内没有引用线索；
      2) 第二列（改动说明列）里出现的 id（很多老行只写 id 不写「卡」字）。
    其它位置的裸 id（审查列里的登记人/被引用卡）一律**不关联**，记进 row["refs"] 供人核。
    """
    owners = set()
    for idx, cell in enumerate(cells):
        if idx == 0:  # 第 0 列是 T-* 编号
            continue
        for match in CARD_OWNER_RE.finditer(cell):
            if _ref_cued(cell, match.start()):
                continue
            cid = match.group(1)
            if cid in exclude_ids:
                continue
            owners.add(cid)
    for idx, cell in enumerate(cells):
        if idx == 0:
            continue
        for grp in CARD_REGISTER_GROUP_RE.finditer(cell):
            for match in CARD_ID_RE.finditer(grp.group(1)):
                cid = match.group(0)
                if cid in exclude_ids:
                    continue
                owners.add(cid)
    if len(cells) > 1:
        for match in CARD_ID_RE.finditer(cells[1]):
            if _ref_cued(cells[1], match.start()):
                continue
            cid = match.group(0)
            if cid in exclude_ids:
                continue
            owners.add(cid)
    return owners


#: 「本条是旧轮次登记、当前无对应工作项」的显式标记（卡 `openmir2-whitelist-card-link-debt`）。
#: 认它 = 该行**已被人工处置过**（不是漏填）；条件② 的核算盲区判据按「既无卡号也无 marker」算。
#:
#: **F1（reviewer-21）**：标记必须是**固定形态** `**legacy**：`（或半角冒号），不能「正文出现 legacy
#: 这个词就算」——否则往判据文件里写一句话（例如「本行描述 legacy 减法行为，没有卡号」）就能把
#: 核算盲区关掉。收紧前现场已有活口：`legacy` 词出现 **101** 行 vs 真标记 **100** 行（多出来的是 T-12，
#: 它提到环境变量 `MIR2_BOT_LEGACY_SHARED_AUTOPLAY`；该行另有卡号，所以不影响计数，但形态必须钉死）。
LEGACY_MARK_RE = re.compile(r"\*\*legacy\*\*\s*[:：]", re.I)


def build_whitelist(path: Path, exclude_ids=None):
    """解析 whitelist.md 的 T-* 行（结构化关联，卡号只从固定列/「卡 `X`」写法取）。

    同一 T-* 编号出现多行（历史遗留重复号）时**并集**：卡集合取并、行文本拼接，
    避免后写覆盖把前一行的卡↔T 强关联吃掉（T-4/T-6 重复行的真实损害）。
    `exclude_ids` 用于排掉 principal 名（openmir2-worker-5 等），它们不是卡 id。

    关联口径（卡 `openmir2-goal-gate-card-association`）：
      * 归属卡 = 「卡 `X`」写法（未被见/承接/原卡/已由/立卡/引用/占用/详见/另见修饰）∪ 第二列里的 id；
      * 一行多卡 ⇒ 全部计入；
      * 行内其它 id 只进 `refs`（不关联，防「行内引用别的卡」误关联）；
      * 无法结构化关联的行 / 列数 != 4 的行 ⇒ 进 `warnings` **点名**，不再静默整行扫描。
    """
    exclude_ids = set(exclude_ids or ())
    info = {"path": str(path), "rows": {}, "card_to_t": {}, "duplicates": [],
            "warnings": [], "refs": {}, "unassociated": [], "bad_columns": [],
            # 卡 `openmir2-whitelist-card-link-debt`：`unassociated` = 没有卡号（含已标 legacy 的行）；
            # `unowned` = **既没卡号也没 legacy** = 条件② 的真核算盲区（必须为 0）。
            "legacy": [], "unowned": [], "error": None}
    try:
        text = path.read_text(encoding="utf-8")
    except OSError as exc:
        info["error"] = str(exc)
        return info
    for line in text.splitlines():
        match = re.match(r"\|\s*(T-\d+)\s*\|", line)
        if not match:
            continue
        t_id = match.group(1)
        cells = split_table_cells(line)
        if len(cells) != 4:
            info["bad_columns"].append(t_id)
            info["warnings"].append(
                "whitelist 行 %s 列数=%d（期望 4）：%s" % (t_id, len(cells), line[:80]))
        cards = _owner_cards(cells, exclude_ids)
        all_ids = {c for c in CARD_ID_RE.findall(line) if c not in exclude_ids}
        info["refs"][t_id] = sorted(all_ids - cards)
        is_legacy = bool(LEGACY_MARK_RE.search(line))
        if is_legacy:
            info["legacy"].append(t_id)
        if not cards:
            info["unassociated"].append(t_id)
            if not is_legacy:
                info["unowned"].append(t_id)
                info["warnings"].append(
                    "whitelist 行 %s 既无『卡 `openmir2-…`』也无 `legacy` 标记（条件② 核算盲区）：%s"
                    % (t_id, line[:80]))
        row = info["rows"].get(t_id)
        if row is None:
            info["rows"][t_id] = {"line": line, "cards": set(cards),
                                  "refs": info["refs"][t_id], "lines": [line]}
        else:
            info["duplicates"].append(t_id)
            row["cards"].update(cards)
            row["lines"].append(line)
            row["line"] = row["line"] + chr(10) + line
    for t_id, row in info["rows"].items():
        for card in row["cards"]:
            info["card_to_t"].setdefault(card, set()).add(t_id)
    return info


def _t_sort_key(t_id):
    return int(t_id.split("-", 1)[1])


def whitelist_unowned_note(whitelist):
    """条件② 核算盲区的机判文案：**既无卡号也无 `legacy`** 的行数 > 0 ⇒ 返回点名串，否则 None。

    卡 `openmir2-whitelist-card-link-debt`：条件② 按**卡**核算 R 卡证据，没有归属的行既不算
    「有证据」也不算「无证据」⇒ 登记可以永远躺着而门仍 PASS。所以这条必须能**判红**，
    基线是 0（2026-10-27 修完 105 行后）。
    """
    unowned = sorted(set(whitelist.get("unowned") or []), key=_t_sort_key)
    if not unowned:
        return None
    return ("whitelist.md 有 %d 行既无卡号也无 `legacy` 标记（条件② 核算盲区：不计入任何卡）：%s"
            % (len(unowned), ", ".join(unowned)))




#: 「无归属行」进 `fail_items` 时的 reason 文案（单一来源：判据与牙共用同一句）。
UNOWNED_REASON = "既无『卡 `openmir2-…`』也无 `legacy` 标记（条件② 核算盲区）"


def unowned_gate_decision(state, detail, note):
    """条件② 的「无归属行」判据 —— **NEED 不降级**（卡 `openmir2-goal-gate-wiring-teeth` A2）。

    `NEED` 表示 board/索引不可用、本来就没法判 ⇒ 即使 `note` 非空也**保持 NEED**，不许冒充成红；
    否则 `note` 非空 ⇒ `FAIL`，并把 `note` 以 `；` 接到 `detail`。

    返回 `(state, detail, failed)`；`failed` 供 [`unowned_fail_items`] 决定要不要产出条目。
    """
    if not note or state == NEED:
        return state, detail, False
    merged = (detail + "；" if detail else "") + note
    return FAIL, merged, True


def unowned_fail_items(failed, unowned):
    """把「无归属行」转成 `fail_items` 条目（卡 `openmir2-goal-gate-wiring-teeth` A1）。

    **只在 `failed` 为真时**产出（`failed` 由 [`unowned_gate_decision`] 给，NEED 时恒 False）；
    `item` 用 `whitelist.md:<T-id>`——它不是卡，别让「按卡」的消费者把它当卡。
    """
    if not failed:
        return []
    return [{"condition": 2, "item": "whitelist.md:%s" % t_id, "reason": UNOWNED_REASON,
             "last_status_ts": 0}
            for t_id in sorted(set(unowned or []), key=_t_sort_key)]


def _names_in_whitelist_rows(name: str, whitelist):
    """条目名（SM_* / CM_*）是否出现在某个 T-* 登记行里。"""
    if not name:
        return []
    pattern = re.compile(r"(?<![A-Za-z0-9_])" + re.escape(name) + r"(?![A-Za-z0-9_])")
    hits = [t_id for t_id, row in whitelist["rows"].items() if pattern.search(row["line"])]
    return sorted(hits, key=_t_sort_key)


# --------------------------------------------------------------------------
# 条件2：R 卡证据
# --------------------------------------------------------------------------

EVIDENCE_MARK_RE = re.compile(r"VERDICT\s*[=:：]\s*PASS", re.I)
APPROVE_WORDS = {"approve", "approved", "pass", "passed", "lgtm"}


def _review_decision(body):
    """review 的结论字段有 `decision` 与 `verdict` 两种写法，都要认。"""
    for key in ("decision", "verdict"):
        val = body.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip().lower()
    return None


def _is_approve(body):
    return _review_decision(body) in APPROVE_WORDS


def _first_str(body, keys):
    for key in keys:
        val = body.get(key)
        if isinstance(val, str) and val.strip():
            return val.strip()
    return ""


# 免窗口短语白名单（I-1）：必须命中其一，才允许用 tier_why 回退认定「不需要窗口」
WAIVER_PHRASES = ("不需要窗口", "无需窗口", "不占窗口", "不涉窗口", "免窗口",
                  "不可能只在真机暴露", "零 .rs", "零.rs", "零生产代码", "纯文档",
                  "纯夹具", "纯仓库卫生", "纯离线", "纯逻辑", "纯文本", "纯工具",
                  "生产不可达", "等价即放行", "零行为", "无需真机")
# 否决词：出现且**未被否定**即不算免窗口（I-1 实证：tier_why=「时序/竞态…」曾被误判为登记）
VETO_TERMS = ("时序", "竞态", "RNG", "落库", "战斗数值", "数值", "持久化", "随机", "tick")
NEG_CUES = ("不触", "不涉及", "不涉", "不含", "不包括", "不改变", "不影响", "无", "零", "未", "非", "免", "勿")


# J-1：子句切分含中文逗号与转折词；顿号（、）不切，以保留「不触 RNG、时序、落库」的统辖列表
CLAUSE_SPLIT_RE = re.compile(r"[，,。；;\n]|但是|然而|不过|可是|但")

def _waiver_ok(text):
    """tier_why 回退是否构成「不需要窗口」：命中白名单短语，且否决词在同一子句内未被否定。"""
    if not text or not any(phrase in text for phrase in WAIVER_PHRASES):
        return False
    for clause in CLAUSE_SPLIT_RE.split(text):
        for term in VETO_TERMS:
            start = 0
            while True:
                idx = clause.find(term, start)
                if idx < 0:
                    break
                if not any(cue in clause[:idx] for cue in NEG_CUES):
                    return False
                start = idx + len(term)
    return True


def collect_r_cards(index):
    """R 卡枚举 = **任一**条目的 body.tier 归一为 R（不只 issue）。

    86/161 张卡的 tier 只出现在 review/patch/status 里（issue 漏写），只认 issue 会形成
    「新 R 卡漏写 tier ⇒ 整张卡不进 R 总体 ⇒ 条件2 仍然 PASS」的假 PASS 路径。
    """
    cards = set()
    for thread_id, entries in index.items():
        for _oid, entry in entries:
            body = entry.get("body")
            if isinstance(body, dict) and tier_is_r(body.get("tier")):
                cards.add(thread_id)
                break
    return sorted(cards)


def collect_r_evidence(card, index, board_cards, whitelist, allow_verdict_grep=False):
    entries = index.get(card, [])
    merged = False
    window_required = False
    last_status_ts = 0
    last_any_ts = 0
    reviews = []
    evidence_entries = []
    for oid, entry in entries:
        body = entry.get("body")
        ts = entry.get("ts") or 0
        last_any_ts = max(last_any_ts, ts)
        if entry.get("kind") == "status":
            last_status_ts = max(last_status_ts, ts)
        if not isinstance(body, dict):
            continue
        if entry.get("kind") == "merge_result" and body.get("merged") is True:
            merged = True
        if body.get("window_required") is True:
            window_required = True
        if entry.get("kind") == "review":
            reviews.append((ts, oid, body))
        # H-4：VERDICT=PASS 是文本子串，默认不采信（--allow-verdict-grep 才开）
        if allow_verdict_grep and entry.get("kind") in ("comment", "review"):
            if EVIDENCE_MARK_RE.search(json.dumps(body, ensure_ascii=False)):
                evidence_entries.append(oid)

    # 已裁定登记：按 ts 倒序取**第一条带 window_required 布尔值**的 approve review。
    # 不能对全部历史 review 做 any(false)：同一卡多次 review 的 window_required 会不一致
    # （gm-commands-design 最新 approve 是 true、早前一条是 false）。
    adjudicated = None
    for ts, oid, body in sorted(reviews, key=lambda x: (x[0], x[1]), reverse=True):
        if not _is_approve(body):
            continue
        wr = body.get("window_required")
        if not isinstance(wr, bool):
            continue
        if wr is True:
            reason = _first_str(body, ("why_not_window", "window_waiver", "tier_why"))
            adjudicated = {"kind": "window_required", "ts": ts, "oid": oid, "why": reason}
        else:
            # 专用免窗口字段优先；回退 tier_why 必须命中免窗口短语且否决词被否定（I-1）
            special = _first_str(body, ("why_not_window", "window_waiver"))
            if special:
                adjudicated = {"kind": "no_window", "ts": ts, "oid": oid,
                               "why": special, "via": "why_not_window"}
            else:
                tier_why = _first_str(body, ("tier_why",))
                if _waiver_ok(tier_why):
                    adjudicated = {"kind": "no_window", "ts": ts, "oid": oid,
                                   "why": tier_why, "via": "tier_why"}
        break

    strong = sorted(whitelist["card_to_t"].get(card, set()), key=_t_sort_key)
    thread_ts = set()
    for _oid, entry in entries:
        thread_ts |= set(T_ID_RE.findall(json.dumps(entry, ensure_ascii=False)))
    weak = sorted((thread_ts & set(whitelist["rows"])) - set(strong), key=_t_sort_key)
    board_card = (board_cards or {}).get(card)
    column = board_card.get("_column") if board_card else None
    board_status = board_card.get("status") if board_card else None
    board_closed = bool(board_card) and (column == "已完成" or board_status == "closed")
    # I-5：未开工/未合并的卡不判证据（条件②约束的对象是已合入代码的卡）。
    # 机器代理：merge_result / board closed / patch 条目 / 非 open 的 status 任一存在 = 已开工或已合并。
    kinds = {entry.get("kind") for _oid, entry in entries}
    has_patch = "patch" in kinds
    has_review = "review" in kinds
    has_comment = "comment" in kinds
    # 已开工信号：合并记录 / 补丁 / 复核 / 讨论。只有 issue + claim（或只有 status）的新卡不算开工。
    judged = board_closed or merged or has_patch or has_review or has_comment
    return {"card": card, "merged": merged, "window_required": window_required,
            "has_patch": has_patch, "has_review": has_review, "has_comment": has_comment,
            "judged": judged,
            "strong_t": strong, "weak_t": weak,
            "adjudicated": adjudicated, "evidence_entries": evidence_entries,
            "in_board": board_card is not None,
            "board_column": column, "board_status": board_status,
            "board_closed": board_closed,
            "unverified": board_card.get("unverified") if board_card else None,
            "last_status_ts": last_status_ts or last_any_ts,
            "entries": len(entries)}


#: **基础设施阻塞排除项**（owner 裁定；只在有签名裁定时填充，**不是静默豁免**）。
#: 每项必须带 `adjudication_oid` —— 指向一条记录 owner 裁定的 collab 条目，可复核。
#: 判据口径见 `docs/leader-goal-20261023.md` §1。
MERGED_NOT_CLOSED_EXEMPT = {
    "openmir2-gm-batch-5b-cross-server": {
        "blocked_by": "G11（跨服 PlanesServer/PlanesClient + 主/分服拓扑；仓内无实现）",
        "adjudication_oid": "ab2a1cf8d37cc3ae02dc1297d4520193fbcaffe8",
        "owner_decision": "先跳过（owner 2026-10-25 会话内直接回复，orchestrator 记录）",
    },
}


def r_category(ev):
    """单卡归类（顺序即判据优先级）。

    「已裁定登记」（window_required / whitelist 强关联 / approve+window_required:false+理由）
    必须排在 merged_not_closed **之前**：有登记 ⇒ PASS；merged 未 closed 只作为卫生清单单列。
    """
    if not ev.get("in_board"):
        return "not_in_board"
    if ev.get("board_closed"):
        return "closed"
    if not ev.get("judged", True):
        return "not_merged"
    if ev.get("unverified"):
        return "unverified"
    adj = ev.get("adjudicated")
    if adj and adj.get("kind") != "window_required":
        # 复核裁定说 false+理由 ⇒ 已裁定不需窗口
        return "adjudicated"
    # `window_required` 登记（历史的或 approve 裁定的）若**尚未合并**，单列一类：
    # 窗口在合并前根本跑不了，混进 window_required 会让「窗口欠账」失真（卡
    # `openmir2-goal-gate-unmerged-window-bucket`）。**仍然是 PASS**，只拆计数。
    if (ev.get("window_required") or (adj and adj.get("kind") == "window_required")):
        return "window_required_unmerged" if not ev.get("merged") else "window_required"
    if ev.get("strong_t"):
        return "whitelist_t"
    if ev.get("evidence_entries"):
        return "comment_evidence"
    if ev.get("merged") and ev["card"] in MERGED_NOT_CLOSED_EXEMPT:
        return "merged_not_closed_exempt"
    if ev.get("merged"):
        return "merged_not_closed"
    if ev.get("weak_t"):
        return "weak_t"
    return "none"


def classify_r_evidence(ev):
    """判定：已合并 / 有显式登记（window_required、whitelist、已裁定）/ 需人工 / 三者皆无。"""
    category = r_category(ev)
    if category == "not_in_board":
        hints = []
        if ev.get("merged"):
            hints.append("索引内有 merge_result merged=true")
        if ev.get("window_required"):
            hints.append("索引内有 window_required=true")
        suffix = "；" + "、".join(hints) if hints else ""
        return NEED, "board 未列出该卡（本地协作视图未同步或线程状态异常），证据无法核对" + suffix
    if category == "unverified":
        return NEED, "board 报 %s 条未验签条目，证据不可信" % ev["unverified"]
    if category == "not_merged":
        return NOT_MERGED, ("未开工/在飞（无 merge_result / patch / 非 open status）："
                            "条件②只对已开工或已合并的卡判证据与登记，本卡不计 PASS 也不计 FAIL")
    if category == "closed":
        why = "board 已完成/closed"
        if ev.get("merged"):
            why += "；线程有 merge_result merged=true"
        if ev.get("evidence_entries"):
            why += "；comment/review 内有 VERDICT=PASS"
        return PASS, why
    if category == "window_required_unmerged":
        return PASS, ("有 window_required 登记但**尚未合并**（窗口在合并前跑不了，单列计数；"
                      "登记本身有效，仍算 PASS）")
    if category == "window_required":
        return PASS, "有 window_required=true 登记"
    if category == "whitelist_t":
        return PASS, "whitelist 登记 %s（登记行点名本卡）" % ",".join(ev["strong_t"])
    if category == "adjudicated":
        adj = ev["adjudicated"]
        if adj["kind"] == "no_window":
            return PASS, "已裁定登记：review %s approve + window_required=false + 理由（%s）" % (
                adj["oid"][:8], adj["why"][:60])
        return PASS, "已裁定登记：review %s approve + window_required=true" % adj["oid"][:8]
    if category == "comment_evidence":
        return PASS, "comment/review 内有 VERDICT=PASS 证据（%s；--allow-verdict-grep 开启）" % ",".join(
            oid[:8] for oid in ev["evidence_entries"])
    if category == "merged_not_closed_exempt":
        ex = MERGED_NOT_CLOSED_EXEMPT[ev["card"]]
        return PASS, ("**基础设施阻塞排除项**：blocked_by=%s；owner 裁定「%s」；裁定条目 %s"
                      % (ex["blocked_by"], ex["owner_decision"], ex["adjudication_oid"][:8]))
    if category == "merged_not_closed":
        return NEED, ("merge_result merged=true 但 board 状态=%s（非 closed）且无显式登记；"
                      "需确认未重开新工作或无证据" % (ev.get("board_status") or ev.get("board_column") or "?"))
    if category == "weak_t":
        return NEED, "线程提到 %s，但登记行未点名本卡（弱关联，需人判是否算登记）" % ",".join(ev["weak_t"])
    return FAIL, "三者皆无（未合并 / 无 window_required / 无 whitelist T-* 登记 / 无已裁定登记）"


def judge_r_cards(evidence_list):
    """聚合条件2。need_items 按 last_status_ts 升序（最旧先）；FAIL 顺序不动。"""
    per_card, fail_items, need_items = [], [], []
    counts = {}
    for ev in evidence_list:
        state, why = classify_r_evidence(ev)
        category = r_category(ev)
        counts[category] = counts.get(category, 0) + 1
        record = {"card": ev["card"], "state": state, "reason": why, "category": category,
                  "merged": ev["merged"], "window_required": ev["window_required"],
                  "strong_t": ev["strong_t"], "weak_t": ev["weak_t"],
                  "board_status": ev["board_status"], "board_column": ev["board_column"],
                  "adjudicated": ev.get("adjudicated"),
                  "comment_evidence": list(ev.get("evidence_entries") or []),
                  "last_status_ts": ev.get("last_status_ts", 0),
                  "unverified": ev["unverified"]}
        per_card.append(record)
        if state == FAIL:
            fail_items.append({"card": ev["card"], "reason": why,
                               "last_status_ts": ev.get("last_status_ts", 0)})
        elif state == NEED:
            need_items.append({"card": ev["card"], "reason": why,
                               "last_status_ts": ev.get("last_status_ts", 0)})
    need_items.sort(key=lambda x: (x["last_status_ts"], x["card"]))
    if fail_items:
        state = FAIL
    elif need_items:
        state = NEED
    else:
        state = PASS
    detail = ("R 卡 %d 张：已完成 %d / window_required %d（另未合并 %d） / whitelist %d / "
              "已裁定登记 %d / comment 证据 %d / 三者皆无 %d / "
              "merged 未 closed 且无登记(需人工) %d / 未开工或未合并 %d（不计缺口）/ 其它需人工 %d" % (
                  len(per_card), counts.get("closed", 0), counts.get("window_required", 0),
                  counts.get("window_required_unmerged", 0),
                  counts.get("whitelist_t", 0), counts.get("adjudicated", 0),
                  counts.get("comment_evidence", 0), counts.get("none", 0),
                  counts.get("merged_not_closed", 0), counts.get("not_merged", 0),
                  counts.get("not_in_board", 0) + counts.get("unverified", 0)
                  + counts.get("weak_t", 0)))
    exempt_now = [t for t in MERGED_NOT_CLOSED_EXEMPT
                  if next((c for c in per_card if c["card"] == t), None) is not None
                  and next(c for c in per_card if c["card"] == t)["category"] == "merged_not_closed_exempt"]
    if exempt_now:
        detail += ("；**另排除 %d 张基础设施阻塞：%s**（owner 裁定，裁定条目 %s）"
                   % (len(exempt_now), ", ".join(exempt_now),
                      MERGED_NOT_CLOSED_EXEMPT[exempt_now[0]]["adjudication_oid"][:8]))
    stale_exempt = [t for t in MERGED_NOT_CLOSED_EXEMPT if t not in exempt_now]
    if stale_exempt:
        detail += ("；**⚠ 豁免项已失效**：%s 不再是 merged_not_closed ⇒ 请同步删除 "
                   "MERGED_NOT_CLOSED_EXEMPT 条目" % ", ".join(stale_exempt))
    if fail_items:
        detail += "；FAIL 卡：" + ", ".join(x["card"] for x in fail_items)
    if need_items:
        detail += "；需人工 %d 张（按 last_status_ts 最旧优先，见 JSON need_items）" % len(need_items)
    return state, detail, per_card, fail_items, need_items


# --------------------------------------------------------------------------
# 条件3：B 类（recon dispatch tracked）清零
# --------------------------------------------------------------------------

def load_recon_dispatch(mir2_root: Path, timeout=300):
    tmpdir = Path(tempfile.mkdtemp(prefix="goal-gate-recon-"))
    json_path = tmpdir / "dispatch.json"
    try:
        rec = run_cmd([sys.executable, "-I", "-B", "tests/parity/coverage/recon_gates.py",
                       "dispatch", "--repo", "..", "--json", str(json_path)], mir2_root, timeout)
        if rec.get("rc") != 0:
            return None, rec, json_path.exists()
        try:
            data = json.loads(json_path.read_text(encoding="utf-8"))
        except (OSError, ValueError) as exc:
            rec["error"] = "dispatch json 读取失败：%s" % exc
            return None, rec, False
        return data, rec, True
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)


def classify_b_row(row, whitelist, board_cards):
    """单条 tracked 判定：T-* 登记 / 有卡 / 皆无。返回 (category, state, detail)。"""
    name = row.get("name") or ""
    card = row.get("card") or ""
    t_hits = _names_in_whitelist_rows(name, whitelist)
    if t_hits:
        return "t", PASS, "%s 出现在 %s 登记行" % (name, ",".join(t_hits))
    if card:
        if board_cards is None:
            return "card", NEED, "有卡 %s，但 board 不可用（无法确认卡存在）" % card
        board_card = board_cards.get(card)
        if board_card is None:
            return "card", NEED, "有卡 %s，但 board 未列出该卡" % card
        return "card", PASS, "有卡 %s（board 状态 %s）" % (card, board_card.get("status"))
    return "none", FAIL, "%s 既无 T-* 登记也无卡" % name


def judge_b_tracked(rows, whitelist, board_cards, counts=None):
    """条件3 聚合。查不到 / schema 漂移 / counts 不一致一律「需人工」，不冒出 PASS。

    但 **B 类真正清零**时 recon 会给出 rows 非空、counts.tracked=0 —— 这一情形必须 PASS，
    否则终点永远无法达成（H-3）。所以 counts 校验在前、清零判定在后。
    """
    if not isinstance(rows, list) or not rows:
        return NEED, "recon dispatch 未给出任何行（输出为空/解析失败），无法判定 tracked 清单", [], [], []
    missing_status = [row for row in rows if not isinstance(row, dict) or "status" not in row]
    if missing_status:
        return NEED, "recon 行缺 status 字段（schema 漂移：%d/%d 行），无法判定 tracked" % (
            len(missing_status), len(rows)), [], [], []
    if not isinstance(counts, dict) or counts.get("tracked") is None:
        return NEED, "recon JSON 缺 counts.tracked，无法与 tracked 行数交叉校验", [], [], []
    try:
        expected = int(counts["tracked"])
    except (TypeError, ValueError):
        return NEED, "recon counts.tracked 不是整数（%r）" % (counts.get("tracked"),), [], [], []
    tracked = [row for row in rows if row.get("status") == "tracked"]
    if expected != len(tracked):
        return NEED, "recon counts.tracked=%d 与 tracked 行数 %d 不一致（分母漂移）" % (
            expected, len(tracked)), [], [], []
    if expected == 0:
        return PASS, "B 类清零：recon tracked=0（rows %d 条非空、counts 一致）" % len(rows), [], [], []

    per_item, fail_items, need_items = [], [], []
    for row in tracked:
        category, state, detail = classify_b_row(row, whitelist, board_cards)
        item = {"name": row.get("name"), "bucket": row.get("bucket"),
                "card": row.get("card"), "category": category,
                "state": state, "detail": detail}
        per_item.append(item)
        if state == FAIL:
            fail_items.append(item)
        elif state == NEED:
            need_items.append(item)
    if fail_items:
        state = FAIL
    elif need_items:
        state = NEED
    else:
        state = PASS
    t_n = sum(1 for i in per_item if i["category"] == "t")
    card_n = sum(1 for i in per_item if i["category"] == "card")
    none_n = sum(1 for i in per_item if i["category"] == "none")
    detail = "tracked %d 条（counts 一致）：有 T-* 登记 %d / 有卡 %d / 皆无 %d" % (
        len(per_item), t_n, card_n, none_n)
    if fail_items:
        detail += "；皆无：" + ", ".join(i["name"] for i in fail_items)
    if need_items:
        detail += "；需人工：" + ", ".join(i["name"] for i in need_items)
    return state, detail, per_item, fail_items, need_items


# --------------------------------------------------------------------------
# 条件4：门禁恒绿 + master 同步
# --------------------------------------------------------------------------

def judge_gate_checks(checks):
    """checks: [{name, state, detail}]。任一 FAIL -> FAIL；否则任一 NEED -> NEED；否则 PASS。"""
    if any(c["state"] == FAIL for c in checks):
        return FAIL
    if any(c["state"] == NEED for c in checks):
        return NEED
    return PASS


def judge_remote(local_master, remote_sha, remote_rec):
    """master 同步判定。返回 (state, detail)。"""
    if remote_rec.get("rc") != 0 or not remote_sha:
        return NEED, "git ls-remote walgit refs/heads/master 不可用（rc=%s）：无法比对面" % remote_rec.get("rc")
    if not local_master:
        return NEED, "本地 master 不存在/不可解析"
    if local_master == remote_sha:
        return PASS, "本地 master == walgit master（%s）" % local_master[:12]
    return FAIL, "本地 master %s != walgit master %s" % (local_master[:12], remote_sha[:12])


TOOL_GATE_REL = "tools/hygiene/tool_selftest_gate.py"
#: 工具门的「只有需人工」退出码（卡 openmir2-docs-gate-rc-semantics 定的：红/未分类⇒2、
#: **只有 NEED⇒3**、否则 0）。goal_gate 必须把它映射成 NEED，不能折叠成 FAIL。
TOOL_GATE_NEED_RC = 3


def tool_gate_check(repo, run=None, timeout=600):
    """条件④ 的工具自测门（2026-10-26，卡 openmir2-goal-gate-include-tool-selftest）。

    四态（**不许把「跑不了」当 PASS，也不许把「判不了」当证伪**）：
      * rc=0 ⇒ PASS（普通全跑已含 `--inventory` 未分类检查，A3）；
      * rc=3（工具门自报「只有需人工/判不了」）⇒ **NEED**，detail 点名 `NEED <tool>` 行；
      * rc=2 / 其它 ⇒ FAIL，detail 从输出里点名 `FAIL <tool>` / `UNCLASSIFIED <path>`；
      * 工具缺失 / 无法启动 / 超时 ⇒ NEED（判不了）。

    rc=3 这一档是卡 `openmir2-goalgate-need-rc3` 补的：工具门在「某个工具自述判不了（rc=2）」
    时只退 3 ⇒ 若这里仍按「非零即红」处理，条件④ 就会把「判不了」报成「证伪」——正是
    `docs_delivery_gate` 假红那族病的换层复发。

    `run` 可注入（selftest 用假 runner 验三态）；默认 `run_cmd`。
    """
    run = run_cmd if run is None else run
    path = Path(repo) / TOOL_GATE_REL
    if not path.is_file():
        return {"name": "tool_selftest_gate", "state": NEED, "rc": None,
                "detail": "工具缺失：%s（判不了，不是 PASS）" % TOOL_GATE_REL}
    rec = run([sys.executable, "-I", "-B", TOOL_GATE_REL], repo, timeout)
    if rec.get("timed_out"):
        return {"name": "tool_selftest_gate", "state": NEED, "rc": rec.get("rc"),
                "detail": "tool_selftest_gate 超时（%ss）：判不了，不是 PASS" % timeout}
    if rec.get("error"):
        return {"name": "tool_selftest_gate", "state": NEED, "rc": rec.get("rc"),
                "detail": "tool_selftest_gate 无法启动：%s（判不了，不是 PASS）" % rec["error"]}
    if rec.get("rc") == 0:
        return {"name": "tool_selftest_gate", "state": PASS, "rc": 0,
                "detail": "tool_selftest_gate rc=0（%.1fs；含 --inventory 未分类检查）"
                          % rec.get("duration_s", 0.0)}
    lines = ((rec.get("stdout") or "") + "\n" + (rec.get("stderr") or "")).splitlines()
    if rec.get("rc") == TOOL_GATE_NEED_RC:
        need_named = [ln.strip() for ln in lines if ln.strip().startswith("NEED ")]
        detail = ("tool_selftest_gate rc=3（只有「需人工」/判不了，**不是**证伪；%.1fs）"
                  % rec.get("duration_s", 0.0))
        detail += ("；点名：" + " | ".join(need_named[:3])[:400]) if need_named else "；见输出"
        return {"name": "tool_selftest_gate", "state": NEED, "rc": TOOL_GATE_NEED_RC,
                "detail": detail}
    named = [ln.strip() for ln in lines
             if ln.strip().startswith(("FAIL ", "UNCLASSIFIED "))]
    detail = "tool_selftest_gate rc=%s（%.1fs）" % (rec.get("rc"), rec.get("duration_s", 0.0))
    detail += ("；点名：" + " | ".join(named[:6])[:520]) if named else "；输出无 FAIL/UNCLASSIFIED 行"
    return {"name": "tool_selftest_gate", "state": FAIL, "rc": rec.get("rc"),
            "detail": detail}


def run_gate_checks(repo: Path, mir2_root: Path, with_tests: bool, timeout, test_timeout):
    checks = []
    gates = [
        ("fmt", ["cargo", "fmt", "--all", "--", "--check"], timeout),
        ("clippy", ["cargo", "clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"], timeout),
        ("msgcodegen", ["cargo", "run", "--locked", "-p", "msgcodegen", "--", "--check"], timeout),
        ("whitelist_anchor_gate", [sys.executable, "-I", "-B",
                                   "tests/parity/m3/whitelist_anchor_gate.py", "check"], timeout),
        ("m3_whitelist_registry_audit", [sys.executable, "-I", "-B",
                                         "tests/parity/m3/m3_whitelist_registry_audit.py", "audit"], timeout),
    ]
    for name, cmd, to in gates:
        rec = run_cmd(cmd, mir2_root, to)
        if rec.get("timed_out"):
            state, detail = NEED, "%s 超时（%ss）" % (name, to)
        elif rec.get("error"):
            state, detail = NEED, "%s 无法启动：%s" % (name, rec["error"])
        elif rec.get("rc") == 0:
            state, detail = PASS, "%s rc=0（%.1fs）" % (name, rec["duration_s"])
        else:
            state, detail = FAIL, "%s rc=%s（%.1fs）" % (name, rec["rc"], rec["duration_s"])
        checks.append({"name": name, "state": state, "detail": detail, "rc": rec.get("rc")})

    # 第 8 项：工具自测门（cwd=仓库根；工具门自身总墙钟 420s，外层给 600s）
    checks.append(tool_gate_check(repo, run=run_cmd, timeout=600))

    if with_tests:
        rec = run_cmd(["cargo", "test", "--workspace", "--locked"], mir2_root, test_timeout)
        if rec.get("timed_out"):
            state, detail = NEED, "cargo test --workspace 超时（%ss）" % test_timeout
        elif rec.get("error"):
            state, detail = NEED, "cargo test 无法启动：%s" % rec["error"]
        elif rec.get("rc") == 0:
            state, detail = PASS, "cargo test --workspace rc=0（%.1fs）" % rec["duration_s"]
        else:
            state, detail = FAIL, "cargo test --workspace rc=%s（%.1fs）" % (rec["rc"], rec["duration_s"])
        checks.append({"name": "cargo_test_workspace", "state": state, "detail": detail,
                       "rc": rec.get("rc")})
    else:
        checks.append({"name": "cargo_test_workspace", "state": NEED,
                       "detail": "未跑（快速模式默认跳过；用 --tests / --full 完整判）", "rc": None})

    local_rec = run_cmd(["git", "rev-parse", "master"], repo, 120)
    local_master = local_rec.get("stdout", "").strip() if local_rec.get("rc") == 0 else ""
    remote_rec = run_cmd(["git", "ls-remote", "walgit", "refs/heads/master"], repo, 180)
    remote_sha = ""
    if remote_rec.get("rc") == 0:
        for line in remote_rec.get("stdout", "").splitlines():
            parts = line.split()
            if len(parts) >= 2 and parts[1] == "refs/heads/master":
                remote_sha = parts[0]
                break
    state, detail = judge_remote(local_master, remote_sha, remote_rec)
    checks.append({"name": "master_sync", "state": state, "detail": detail,
                   "local_master": local_master, "remote_master": remote_sha})
    return checks


# --------------------------------------------------------------------------
# 汇总 / 输出
# --------------------------------------------------------------------------

def overall_state(states):
    if any(s == FAIL for s in states):
        return FAIL
    if any(s == NEED for s in states):
        return NEED
    return PASS


def build_condition(idx, name, state, detail, evidence=None):
    return {"id": idx, "name": name, "state": state, "label": STATE_LABEL[state],
            "detail": detail, "evidence": evidence or {}}


def _truncate(text, width):
    text = (text or "").replace("\n", " ")
    if len(text) <= width:
        return text
    return text[: width - 1] + "…"


def render_table(report):
    lines = ["OpenMir2 终点四条件机判（只读；repo=%s）" % report["repo"]]
    mode = report["mode"]
    lines.append("模式：%s（cargo test=%s，parse-stats=%s）" % (
        "完整" if mode["tests"] and mode["script_coverage"] else "快速",
        "跑" if mode["tests"] else "跳过",
        "跑" if mode["script_coverage"] else "跳过"))
    lines.append("")
    lines.append("%-4s %-26s %-6s %s" % ("#", "条件", "状态", "摘要"))
    lines.append("-" * 100)
    for cond in report["conditions"]:
        lines.append("%-4s %-26s %-6s %s" % (
            cond["id"], cond["name"], cond["label"], _truncate(cond["detail"], 160)))
    lines.append("-" * 100)
    lines.append("总体：%s" % STATE_LABEL[report["overall"]])
    if report["warnings"]:
        lines.append("")
        lines.append("提示：")
        for warning in report["warnings"]:
            lines.append("  - " + warning)
    if report["fail_items"] or report["need_items"]:
        lines.append("")
        lines.append("缺口明细：")
        for item in report["fail_items"]:
            lines.append("  FAIL [条件%s] %s：%s" % (item["condition"], item["item"], item["reason"]))
        limit = 20
        groups = {}
        for item in report["need_items"]:
            groups.setdefault(item["condition"], []).append(item)
        for cond_id in sorted(groups):
            items = groups[cond_id]
            for item in items[:limit]:
                lines.append("  需人工 [条件%s] %s：%s" % (cond_id, item["item"], item["reason"]))
            if len(items) > limit:
                lines.append("  … 条件%s 另有 %d 条「需人工」未在表中列出（--json 的 need_items 有全量，"
                             "按 last_status_ts 最旧优先）" % (cond_id, len(items) - limit))
    if report.get("not_merged_items"):
        lines.append("")
        lines.append("未开工/在飞（未合并，不计 PASS/FAIL，仅清单）：")
        for item in report["not_merged_items"][:20]:
            lines.append("  未合并 %s（last_status_ts=%s）" % (item["card"], item["last_status_ts"]))
        rest = len(report["not_merged_items"]) - 20
        if rest > 0:
            lines.append("  … 另有 %d 条未合并卡（见 --json 的 not_merged_items）" % rest)
    return "\n".join(lines)


# --------------------------------------------------------------------------
# 主流程
# --------------------------------------------------------------------------

def _cond(item_state, cond_id, item, reason, fail_items, need_items):
    if item_state == FAIL:
        fail_items.append({"condition": cond_id, "item": item, "reason": reason})
    elif item_state == NEED:
        need_items.append({"condition": cond_id, "item": item, "reason": reason})


def collect_report(repo: Path, envir: str, script_bin: str, with_tests: bool,
                   with_script: bool, timeout: int, test_timeout: int,
                   allow_verdict_grep: bool = False):
    mir2_root = repo / "mir2-rs"
    warnings, conditions, fail_items, need_items = [], [], [], []

    # ---- 条件1
    script_source = ("cargo run --release -p mir2-script-tool -- parse-stats <Envir> "
                     "--json <tmp> --require-implemented")
    if with_script or script_bin:
        rec, parsed, counters, pre_error = run_script_coverage(mir2_root, envir, script_bin, timeout)
        if pre_error:
            state, counters, detail = NEED, None, pre_error
        else:
            state, detail = judge_script_coverage(rec, parsed, counters)
        evidence = {"command": rec.get("cmd"), "rc": rec.get("rc"), "parsed": parsed,
                    "counters": counters, "envir": envir, "source": script_source,
                    "note": "数字不硬编码：每次从 parse-stats 的『语料派发目标覆盖』行实测"}
    else:
        rec, parsed, counters = None, None, None
        state, detail = judge_script_coverage(None, None)
        evidence = {"source": script_source, "envir": envir,
                    "note": "快速模式默认跳过（构建慢）；用 --script 复算，或用 --script-bin 指定预编译二进制"}
    conditions.append(build_condition(1, "脚本派发 100%", state, detail, evidence))
    _cond(state, 1, "script-dispatch", detail, fail_items, need_items)

    # ---- 协作视图（条件2/3 需要）
    board_cards, board_rec = load_board(repo)
    index, index_meta = load_entry_index(repo)
    if board_cards is None:
        warnings.append("board 不可用（rc=%s）：%s" % (
            board_rec.get("rc"), (board_rec.get("stderr") or board_rec.get("error") or "").strip()[:200]))
    if not index:
        warnings.append("本地 refs/collab 条目索引为空：先 `git fetch walgit "
                        "'+refs/collab/inbox/*:refs/collab/inbox/*' "
                        "'+refs/collab/meta/*:refs/collab/meta/*'`")
    if board_cards and index:
        missing = sorted(set(board_cards) - set(index))
        if missing:
            warnings.append("board 有 %d 张卡不在本地条目索引（可能时点差）：%s" % (
                len(missing), ", ".join(missing[:5])))

    principals = load_principals(repo)
    whitelist = build_whitelist(mir2_root / "tests/parity/whitelist.md", exclude_ids=principals)
    if whitelist["error"]:
        warnings.append("whitelist.md 读取失败：%s" % whitelist["error"])
    if whitelist["duplicates"]:
        warnings.append("whitelist.md 有重复 T-* 编号：%s" % ", ".join(sorted(set(whitelist["duplicates"]))))
    if whitelist["bad_columns"]:
        warnings.append("whitelist.md 有 %d 行列数≠4（格式异常，按已解析列处理）：%s" % (
            len(whitelist["bad_columns"]), ", ".join(sorted(set(whitelist["bad_columns"]), key=_t_sort_key))))
    unowned_note = whitelist_unowned_note(whitelist)
    if unowned_note:
        warnings.append(unowned_note)

    # ---- 条件2
    unowned_failed = False  # 由下面的条件② 分支置位；fail_items 汇总段要用（F2）
    r_cards = collect_r_cards(index) if index else []
    evidence_list = [collect_r_evidence(card, index, board_cards, whitelist,
                                          allow_verdict_grep=allow_verdict_grep)
                     for card in r_cards]
    if not index or not board_cards:
        state = NEED
        detail = "协作视图不可用（board=%s，entries=%d），R 卡证据无法枚举" % (
            "ok" if board_cards else "fail", len(index))
        per_card, r_fails, r_needs = [], [], []
    elif not r_cards:
        state = NEED
        detail = "本地条目索引中未找到任何 tier=R 的 issue（视图可能未同步）"
        per_card, r_fails, r_needs = [], [], []
    else:
        state, detail, per_card, r_fails, r_needs = judge_r_cards(evidence_list)
        # 卡 `openmir2-whitelist-card-link-debt` / `openmir2-goal-gate-wiring-teeth`：无归属行是**条件② 自身**的
        # 核算盲区 ⇒ 判据抽成纯函数（`NEED` 不降级；否则 FAIL），牙直接钉它，不再埋在内联代码里。
        state, detail, unowned_failed = unowned_gate_decision(state, detail, unowned_note)
    # 卡 `openmir2-goal-gate-unmerged-selftest`：把 `window_required_unmerged` 一并纳入。它是
    # `merged=false` 的桶，永远不会命下面 `rec["merged"]` 的门 ⇒ 行为不变；纳入只是让「登记类
    # 分类集合」保持一个枚举（避免下次新增分类时又漏），不在卫生清单里制造条目。
    hygiene_categories = {"window_required", "window_required_unmerged", "whitelist_t",
                          "adjudicated", "comment_evidence"}
    merged_open_hygiene = sorted(
        ({"card": rec["card"], "board_status": rec["board_status"],
          "category": rec["category"], "last_status_ts": rec["last_status_ts"]}
         for rec in per_card
         if rec["merged"] and rec["board_status"] != "closed" and rec["category"] in hygiene_categories),
        key=lambda x: (x["last_status_ts"], x["card"]))
    if merged_open_hygiene:
        detail += ("；merged 未 closed 但已凭登记 PASS：%d 张（卫生清单，见 evidence.merged_open_hygiene；"
                   "登记型 PASS ≠ 已有真机证据）" % len(merged_open_hygiene))
    not_merged_items = sorted(
        ({"card": rec["card"], "last_status_ts": rec["last_status_ts"]}
         for rec in per_card if rec["category"] == "not_merged"),
        key=lambda x: (x["last_status_ts"], x["card"]))
    conditions.append(build_condition(2, "R 卡证据", state, detail, {
        "board_cards": len(board_cards) if board_cards else None,
        "index_threads": index_meta.get("threads"), "index_entries": index_meta.get("entries"),
        "snapshot": index_meta.get("snapshot"), "tail_refs": index_meta.get("tail_refs"),
        "whitelist_t_rows": len(whitelist["rows"]),
        "whitelist_duplicates": sorted(set(whitelist["duplicates"])),
        "principals_excluded": len(principals),
        "enumeration": "任一 entry body.tier 归一为 R（不只 issue）",
        "merged_open_hygiene": merged_open_hygiene,
        "merged_open_hygiene_note": "merged=true 但 board 非 closed、已凭登记 PASS 的卫生清单（不计条件2 缺口）",
        "not_merged": not_merged_items,
        "not_merged_note": "未开工/在飞（无 merge_result/patch/非 open status）：不计 PASS 也不计 FAIL，只列清单",
        "allow_verdict_grep": allow_verdict_grep,
        "r_cards": per_card, "notes": index_meta.get("notes")}))
    for item in r_fails:
        fail_items.append({"condition": 2, "item": item["card"], "reason": item["reason"],
                           "last_status_ts": item.get("last_status_ts", 0)})
    # 卡 `openmir2-whitelist-card-link-debt` F2 / `openmir2-goal-gate-wiring-teeth` A1：`unowned` 判红时**也要进
    # `fail_items`**（与其它条件同构；否则 `--json` 消费者与「缺口明细」表里看不到这条红）。条目形状在
    # [`unowned_fail_items`] 里单点定义，牙直接钉它。
    fail_items.extend(unowned_fail_items(unowned_failed, whitelist.get("unowned")))
    for item in r_needs:
        need_items.append({"condition": 2, "item": item["card"], "reason": item["reason"],
                           "last_status_ts": item.get("last_status_ts", 0)})

    # ---- 条件3
    recon, recon_rec, _recon_ok = load_recon_dispatch(mir2_root)
    if recon is None:
        state = NEED
        detail = "recon dispatch 不可用（rc=%s）：%s" % (
            recon_rec.get("rc"),
            (recon_rec.get("stderr") or recon_rec.get("error") or "").strip()[:200])
        b_items, b_fails, b_needs = [], [], []
    else:
        rows = recon.get("rows", []) or []
        state, detail, b_items, b_fails, b_needs = judge_b_tracked(
            rows, whitelist, board_cards, counts=recon.get("counts"))
        detail += "（recon 实测 tracked=%d）" % sum(1 for r in rows if r.get("status") == "tracked")
        if recon.get("missing_cards"):
            warnings.append("recon 报缺卡：%s" % ", ".join(map(str, recon["missing_cards"])))
    conditions.append(build_condition(3, "B 类清零", state, detail, {
        "command": "python -I -B tests/parity/coverage/recon_gates.py dispatch --repo .. --json <tmp>",
        "counts": (recon or {}).get("counts"), "items": b_items}))
    for item in b_fails:
        _cond(FAIL, 3, item["name"], item["detail"], fail_items, need_items)
    for item in b_needs:
        _cond(NEED, 3, item["name"], item["detail"], fail_items, need_items)

    # ---- 条件4
    gate_checks = run_gate_checks(repo, mir2_root, with_tests, timeout, test_timeout)
    gate_state = judge_gate_checks(gate_checks)
    failed = [c for c in gate_checks if c["state"] == FAIL]
    needed = [c for c in gate_checks if c["state"] == NEED]
    detail = "门禁 %d 项：绿 %d / 红 %d / 需人工 %d" % (
        len(gate_checks), sum(1 for c in gate_checks if c["state"] == PASS), len(failed), len(needed))
    if failed:
        detail += "；红：" + ", ".join(c["name"] for c in failed)
    if needed:
        detail += "；未判：" + ", ".join(c["name"] for c in needed)
    conditions.append(build_condition(4, "门禁恒绿", gate_state, detail, {
        "checks": gate_checks, "with_tests": with_tests}))
    for check in failed:
        _cond(FAIL, 4, check["name"], check["detail"], fail_items, need_items)
    for check in needed:
        _cond(NEED, 4, check["name"], check["detail"], fail_items, need_items)

    overall = overall_state([c["state"] for c in conditions])
    return {"schema": 1, "generated_ts": int(time.time()), "repo": str(repo), "envir": envir,
            "mode": {"tests": with_tests, "script_coverage": bool(with_script or script_bin)},
            "conditions": conditions, "overall": overall,
            "overall_label": STATE_LABEL[overall], "fail_items": fail_items,
            "need_items": need_items, "not_merged_items": not_merged_items,
            "warnings": warnings}


# --------------------------------------------------------------------------
# selftest：合成输入验证三态分支 + 牙
# --------------------------------------------------------------------------

SYNTH_WHITELIST = "\n".join([
    "# synthetic whitelist",
    "| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |",
    "| --- | --- | --- | --- |",
    "| T-1 | 卡 `openmir2-card-alpha` 的差异 | 无害 | reviewer |",
    "| T-2 | 与条目 `SM_DOOR_OK` 相关的差异 | 无害 | reviewer |",
    "| T-3 | 卡 `openmir2-card-gamma` 的差异 | 无害 | reviewer |",
    "| T-9 | 卡 `openmir2-card-alpha` 的甲行 | 无害 | reviewer |",
    "| T-9 | 卡 `openmir2-card-beta` 的乙行 | 无害 | reviewer |",
    # 卡 openmir2-goal-gate-card-association 的三颗牙用夹具：
    "| T-10 | 卡 `openmir2-card-alpha`；同批还有卡 `openmir2-card-delta` | 无害 | reviewer |",
    "| T-11 | 卡 `openmir2-card-kappa` 的差异；原由卡 `openmir2-card-zeta` 占用，详见卡 `openmir2-card-eta` | 无害 | reviewer |",
    "| T-12 | 没有卡号的一行 | 无害 |",
    "| T-13 | 改 `x`（卡 `openmir2-card-iota`） | 无害 | 审 | 多余列 |",
])


def _synth_index():
    def ent(kind, body, ts=0):
        return (kind, {"kind": kind, "ts": ts, "body": body})
    return {
        "openmir2-card-alpha": [ent("issue", {"tier": "R"}),
                                ent("merge_result", {"merged": True})],
        "openmir2-card-beta": [ent("issue", {"tier": "R（战斗数值面）"}),
                               ent("review", {"window_required": True}),
                               ent("merge_result", {"merged": True})],
        "openmir2-card-gamma": [ent("issue", {"tier": "R"}),
                                ent("review", {"decision": "approve"}),
                                ent("merge_result", {"merged": True})],
        "openmir2-card-delta": [ent("issue", {"tier": "R"}), ent("status", {}, ts=100)],
        "openmir2-card-eps": [ent("issue", {"tier": "R"}), ent("comment", {"note": "见 T-1"}),
                              ent("merge_result", {"merged": True}),
                              ent("status", {}, ts=500)],
        "openmir2-card-eta": [ent("issue", {"tier": "R"}),
                              ent("merge_result", {"merged": True}),
                              ent("status", {}, ts=900)],
        "openmir2-card-zeta": [ent("issue", {"tier": "R"})],
        "openmir2-card-theta": [ent("issue", {"tier": "R"}),
                                ent("comment", {"summary": "窗口跑完：VERDICT=PASS（exit 0）"}),
                                ent("merge_result", {"merged": True})],
        "openmir2-card-iota": [ent("issue", {}), ent("review", {"tier": "R"})],
        "openmir2-card-lambda": [ent("issue", {"tier": "R"}),
                                 ent("review", {"decision": "approve", "window_required": False,
                                                "tier_why": "纯夹具工具，不触 RNG/时序/持久化，无需窗口"}),
                                 ent("merge_result", {"merged": True})],
        "openmir2-card-kappa": [ent("issue", {"tier": "R"}),
                                ent("review", {"decision": "approve", "window_required": False,
                                               "tier_why": "早前判定"}, ts=100),
                                ent("review", {"verdict": "approve", "window_required": True,
                                               "tier_why": "最新判定"}, ts=200),
                                ent("merge_result", {"merged": True})],
        "openmir2-card-mu": [ent("issue", {"tier": "R"}),
                             ent("review", {"decision": "approve", "window_required": False}),
                             ent("merge_result", {"merged": True}),
                             ent("status", {}, ts=50)],
        # I-1 实证：approve=false 但 tier_why 说的是「时序/竞态」⇒ 不算免窗口，落回 merged_not_closed
        "openmir2-card-nu": [ent("issue", {"tier": "R"}),
                             ent("review", {"decision": "approve", "window_required": False,
                                            "tier_why": "时序/竞态（控制帧 flush 与进程退出）"}),
                             ent("merge_result", {"merged": True})],
        # I-1 专用字段 why_not_window 优先，无需命中 tier_why 短语
        "openmir2-card-xi": [ent("issue", {"tier": "R"}),
                             ent("review", {"decision": "approve", "window_required": False,
                                            "why_not_window": "本片生产不可达，不占窗口"}),
                             ent("merge_result", {"merged": True})],
        # I-1：命中白名单短语但否决词未被否定 ⇒ 仍不算免窗口
        "openmir2-card-nv": [ent("issue", {"tier": "R"}),
                             ent("review", {"decision": "approve", "window_required": False,
                                            "tier_why": "纯逻辑片，但涉及时序竞态"}),
                             ent("merge_result", {"merged": True})],
        # I-1：没有任何免窗口短语（也没否决词）⇒ 不算登记
        "openmir2-card-nw": [ent("issue", {"tier": "R"}),
                             ent("review", {"decision": "approve", "window_required": False,
                                            "tier_why": "控制帧 flush 与进程退出"}),
                             ent("merge_result", {"merged": True})],
        # I-5：已合并但无任何证据/登记 ⇒ 真 FAIL
        "openmir2-card-omicron": [ent("issue", {"tier": "R"}),
                                  ent("patch", {"head": "deadbeef"})],
        # 卡 `openmir2-goal-gate-unmerged-selftest`：窗口登记但**未合并** ⇒ 单列
        # `window_required_unmerged`（pi），补上 `merge_result` 后回到 `window_required`（rho）。
        "openmir2-card-pi": [ent("issue", {"tier": "R"}),
                             ent("review", {"window_required": True})],
        "openmir2-card-rho": [ent("issue", {"tier": "R"}),
                              ent("review", {"window_required": True}),
                              ent("merge_result", {"merged": True})],
    }


def _synth_board():
    in_progress = "进行中"
    return {
        "openmir2-card-alpha": {"id": "openmir2-card-alpha", "unverified": 0,
                                "status": "closed", "_column": "已完成"},
        "openmir2-card-beta": {"id": "openmir2-card-beta", "unverified": 0,
                               "status": "in-progress", "_column": in_progress},
        "openmir2-card-gamma": {"id": "openmir2-card-gamma", "unverified": 0,
                                "status": "needs-review", "_column": "待审"},
        "openmir2-card-delta": {"id": "openmir2-card-delta", "unverified": 0,
                                "status": "in-progress", "_column": in_progress},
        "openmir2-card-eps": {"id": "openmir2-card-eps", "unverified": 1,
                              "status": "in-progress", "_column": in_progress},
        "openmir2-card-eta": {"id": "openmir2-card-eta", "unverified": 0,
                              "status": "in-progress", "_column": in_progress},
        "openmir2-card-zeta": {"id": "openmir2-card-zeta", "unverified": 0,
                               "status": "closed", "_column": "已完成"},
        "openmir2-card-theta": {"id": "openmir2-card-theta", "unverified": 0,
                                "status": "in-progress", "_column": in_progress},
        "openmir2-card-iota": {"id": "openmir2-card-iota", "unverified": 0,
                               "status": "closed", "_column": "已完成"},
        "openmir2-card-lambda": {"id": "openmir2-card-lambda", "unverified": 0,
                                 "status": "in-progress", "_column": in_progress},
        "openmir2-card-kappa": {"id": "openmir2-card-kappa", "unverified": 0,
                                "status": "in-progress", "_column": in_progress},
        "openmir2-card-mu": {"id": "openmir2-card-mu", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-nu": {"id": "openmir2-card-nu", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-xi": {"id": "openmir2-card-xi", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-nv": {"id": "openmir2-card-nv", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-nw": {"id": "openmir2-card-nw", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-omicron": {"id": "openmir2-card-omicron", "unverified": 0,
                                  "status": "in-progress", "_column": in_progress},
        "openmir2-card-pi": {"id": "openmir2-card-pi", "unverified": 0,
                             "status": "in-progress", "_column": in_progress},
        "openmir2-card-rho": {"id": "openmir2-card-rho", "unverified": 0,
                              "status": "in-progress", "_column": in_progress},
    }


def selftest(verbose=False):
    results = []

    def check(name, fn):
        try:
            ok, detail = fn()
        except Exception as exc:  # noqa: BLE001 - selftest 要报告任何异常
            ok, detail = False, "异常：%r" % (exc,)
        results.append({"name": name, "ok": bool(ok), "detail": detail})
        if verbose:
            print("%s %s%s" % ("ok  " if ok else "FAIL", name, "" if ok else " -> " + detail))
        return ok

    tmpdir = Path(tempfile.mkdtemp(prefix="goal-gate-selftest-"))
    try:
        wl_path = tmpdir / "whitelist.md"
        wl_path.write_text(SYNTH_WHITELIST, encoding="utf-8")
        wl = build_whitelist(wl_path)

        check("whitelist 解析出 8 个 T 行", lambda: (len(wl["rows"]) == 8, str(len(wl["rows"]))))
        check("whitelist 重复号 T-9 并集（卡不丢；T-10 的一行两卡也并入 alpha）", lambda: (
            wl["card_to_t"].get("openmir2-card-alpha") == {"T-1", "T-9", "T-10"}
            and wl["card_to_t"].get("openmir2-card-beta") == {"T-9"}
            and wl["duplicates"] == ["T-9"], str(wl["card_to_t"])))
        check("whitelist exclude_ids 排掉 principal 名", lambda: (
            "openmir2-card-alpha" not in build_whitelist(
                wl_path, exclude_ids={"openmir2-card-alpha"})["card_to_t"], "excluded"))

        # ---- 卡 openmir2-goal-gate-card-association：结构化关联的三颗牙 ----
        check("牙① 一行两卡 ⇒ 两张都计入（T-10）", lambda: (
            wl["card_to_t"].get("openmir2-card-alpha", set()) >= {"T-1", "T-9", "T-10"}
            and wl["card_to_t"].get("openmir2-card-delta") == {"T-10"},
            str({k: sorted(v) for k, v in wl["card_to_t"].items() if k.endswith(("alpha", "delta"))})))
        check("牙② 行内引用别的卡 ⇒ 不关联（zeta/eta）", lambda: (
            "openmir2-card-zeta" not in wl["card_to_t"]
            and "openmir2-card-eta" not in wl["card_to_t"]
            and wl["refs"].get("T-11") == ["openmir2-card-eta", "openmir2-card-zeta"],
            str(wl["refs"].get("T-11"))))
        check("牙③ 无卡号/格式异常 ⇒ 进 unassociated/bad_columns 并点名", lambda: (
            "T-12" in wl["unassociated"] and "T-13" in wl["bad_columns"]
            and any("T-12" in w for w in wl["warnings"])
            and any("T-13" in w for w in wl["warnings"])
            and "openmir2-card-iota" in wl["card_to_t"],
            str(wl["warnings"])))
        check("结构化后 gamma 仍只关联 T-3（引用不串卡）", lambda: (
            wl["card_to_t"].get("openmir2-card-gamma") == {"T-3"},
            str(wl["card_to_t"].get("openmir2-card-gamma"))))

        # ---- 卡 openmir2-whitelist-card-link-debt：`legacy` 与「无归属」的牙（合成夹具，不碰真实仓库）----
        def _wl_with(rows):
            p = tmpdir / ("wl-%d.md" % abs(hash(tuple(rows))))
            p.write_text("| 编号 | 差异/豁免点 | 为什么无害 | 谁审的 |\n| --- | --- | --- | --- |\n"
                         + "\n".join(rows) + "\n", encoding="utf-8")
            return build_whitelist(p)

        def _legacy_tooth():
            owned = "| T-1 | 差异 | 无害 | 卡 `openmir2-xx` |"
            legacy = "| T-2 | 差异 | 无害 | 作者登记；**legacy**：旧轮次登记，保留作判据 |"
            bare = "| T-3 | 差异 | 无害 | 作者登记 |"
            w = _wl_with([owned, legacy, bare])
            ok = (w["unowned"] == ["T-3"] and w["legacy"] == ["T-2"]
                  and "T-1" not in w["unowned"] and "T-2" not in w["unowned"])
            note = whitelist_unowned_note(w)
            ok = ok and note is not None and "T-3" in note and "T-2" not in note
            clean = whitelist_unowned_note(_wl_with([owned, legacy]))
            ok = ok and clean is None
            # 反面：把 legacy 标记删掉 ⇒ 同一行立刻变成盲区（这条钉住「标 legacy 才算出域」）
            no_mark = "| T-2 | 差异 | 无害 | 作者登记：旧轮次登记，保留作判据 |"
            w2 = _wl_with([owned, no_mark])
            ok = ok and w2["unowned"] == ["T-2"] and w2["legacy"] == []
            # **F1 第三态（reviewer-21 的致命反例）**：只**提到** legacy 这个词、没有标记形态的行
            # 仍必须算 `unowned`——否则「写一句话就能把核算盲区关掉」。
            mention = "| T-4 | 差异 | 无害 | 本行描述 legacy 减法行为，没有卡号（提到词 ≠ 标记） |"
            w3 = _wl_with([owned, legacy, mention])
            ok = ok and w3["unowned"] == ["T-4"] and w3["legacy"] == ["T-2"]
            return ok, "unowned=%s legacy=%s note=%r" % (w["unowned"], w["legacy"], note)

        check("whitelist: 卡号/legacy/无归属三态分明，且删 legacy 标记即变盲区", _legacy_tooth)

        # ---- 卡 `openmir2-goal-gate-wiring-teeth`：两处「接线」的牙（A1/A2 + 接线断言）----
        def _wiring_teeth():
            """A1 无归属行 ⇒ `fail_items` 条目（`failed=False`/空 unowned 时不得产出）；
            A2 `NEED` **不降级**（`state == NEED` 且 `unowned` 非空 ⇒ 仍 NEED、不判红），
               以及三个常规分支；再加**接线断言**（运行段必须真的调这两个函数，
               删任一调用 ⇒ 本 check 红）。

            规则 19 退化输入逐条（「还能不能红」）：
              * **空 unowned**（`note is None`）⇒ A1 的 `failed=False`/空表两支 + A2 的 `(FAIL, d, None)` 支；
                接线断言仍咬（删调用照样红）。
              * **NEED**（board/索引不可用）⇒ A2 的 `(NEED, d, "n")` 支：必须**不**判红。
              * **NEED 与非空 unowned 同时在场**（tester-1 的跨前提负例）⇒ 同上，仍是 `(NEED, …, False)`；
                把守卫改成恒真（去掉 `state == NEED` 短路）⇒ 该支变 `(FAIL, …)` ⇒ 本 check 红。
            """
            items = unowned_fail_items(True, ["T-9", "T-3"])
            ok = (len(items) == 2
                  and items[0]["item"] == "whitelist.md:T-3"
                  and items[1]["item"] == "whitelist.md:T-9"
                  and all(i["reason"] == UNOWNED_REASON and i["condition"] == 2 for i in items))
            ok = ok and unowned_fail_items(False, ["T-9"]) == []
            ok = ok and unowned_fail_items(True, []) == []

            ok = ok and unowned_gate_decision(NEED, "d", "n") == (NEED, "d", False)
            ok = ok and unowned_gate_decision(PASS, "d", "n") == (FAIL, "d；n", True)
            ok = ok and unowned_gate_decision(PASS, "", "n") == (FAIL, "n", True)
            ok = ok and unowned_gate_decision(FAIL, "d", None) == (FAIL, "d", False)

            # 接线断言：needle 用拼接构造（本 check 自己的源码里也会出现这些片段，原样写会命中自己）。
            src = open(__file__, encoding="utf-8").read()
            gate_needle = "unowned_" + "gate_decision(state, detail, unowned_note)"
            ext_needle = "fail_items.extend(" + "unowned_fail_items("
            ok = ok and src.count(gate_needle) == 1 and src.count(ext_needle) == 1
            return ok, "items=%d gate=%d ext=%d" % (
                len(items), src.count(gate_needle), src.count(ext_needle))

        check("goal_gate: 无归属行的两处接线（fail_items 追加 / NEED 不降级 / 调用点仍接）", _wiring_teeth)

        check("J-1 否定不跨转折：『不涉及时序，但涉及 RNG』=> 拒", lambda: (
            _waiver_ok("纯逻辑，不涉及时序，但涉及 RNG") is False, "cross-conjunction RNG"))
        check("J-1 否定不跨转折：『不涉及时序，但涉及落库』=> 拒", lambda: (
            _waiver_ok("纯逻辑片，不涉及时序，但涉及落库") is False, "cross-conjunction 落库"))
        check("J-1 否定不跨逗号：『无卡，涉及时序』=> 拒", lambda: (
            _waiver_ok("纯逻辑，无卡，涉及时序") is False, "comma scope"))
        check("J-1 顿号列表仍被否定统辖：『不触 RNG、时序、落库』=> 放行", lambda: (
            _waiver_ok("零 .rs，不触 RNG、时序、落库") is True, "list governed"))

        ok_stdout = ("文件: 639 加载: 639 缺失: 0 异常: 0\n"
                     "解析错误: 427；解码异常文件: 0\n"
                     "语料派发目标覆盖：已实现 106 个（11829 次调用）；未实现 0 个\n")
        bad_stdout = ("文件: 639 加载: 639 缺失: 0 异常: 0\n"
                      "解析错误: 427；解码异常文件: 0\n"
                      "语料派发目标覆盖：已实现 103 个（11000 次调用）；未实现 3 个\n")
        parsed_ok = parse_coverage_line(ok_stdout)
        parsed_bad = parse_coverage_line(bad_stdout)
        counters_ok = parse_corpus_counters(ok_stdout)
        counters_bad = dict(counters_ok, missing=2)
        rec_ok = {"rc": 0, "timed_out": False, "error": None}
        rec_bad = {"rc": 1, "timed_out": False, "error": None}

        check("条件1 覆盖行解析（已实现/调用/未实现）", lambda: (
            parsed_ok == {"implemented": 106, "calls": 11829, "unimplemented": 0}, str(parsed_ok)))
        check("条件1 语料完整性计数解析", lambda: (
            counters_ok == {"files": 639, "loaded": 639, "missing": 0, "abnormal": 0,
                            "decode_abnormal": 0}, str(counters_ok)))
        check("条件1 PASS 分支（打印目标 N/N）", lambda: (
            judge_script_coverage(rec_ok, parsed_ok, counters_ok)[0] == PASS
            and "目标 106/106" in judge_script_coverage(rec_ok, parsed_ok, counters_ok)[1],
            str(judge_script_coverage(rec_ok, parsed_ok, counters_ok))))
        check("条件1 FAIL 分支（未实现>0）", lambda: (
            judge_script_coverage(rec_bad, parsed_bad, counters_ok)[0] == FAIL,
            str(judge_script_coverage(rec_bad, parsed_bad, counters_ok))))
        check("条件1 FAIL 分支（rc=1 且未实现=0：--require-implemented 耦合）", lambda: (
            judge_script_coverage(rec_bad, parsed_ok, counters_ok)[0] == FAIL,
            str(judge_script_coverage(rec_bad, parsed_ok, counters_ok))))
        check("条件1 需人工分支（无覆盖行）", lambda: (
            judge_script_coverage(rec_ok, parse_coverage_line("no coverage here"),
                                  counters_ok)[0] == NEED, "no coverage line"))
        check("条件1 需人工分支（未启用）", lambda: (
            judge_script_coverage(None, None)[0] == NEED, "skipped"))
        check("条件1 需人工分支（超时）", lambda: (
            judge_script_coverage({"rc": None, "timed_out": True, "error": None},
                                  parsed_ok, counters_ok)[0] == NEED, "timeout"))
        check("条件1 需人工分支（语料不完整：缺失>0）", lambda: (
            judge_script_coverage(rec_ok, parsed_ok, counters_bad)[0] == NEED,
            str(judge_script_coverage(rec_ok, parsed_ok, counters_bad))))
        check("条件1 需人工分支（缺完整性计数）", lambda: (
            judge_script_coverage(rec_ok, parsed_ok, None)[0] == NEED, "no counters"))

        # ---------- 条件2 ----------
        idx = _synth_index()
        board = _synth_board()
        r_cards = collect_r_cards(idx)
        evs = {c: collect_r_evidence(c, idx, board, wl) for c in r_cards}
        evs_grep = {c: collect_r_evidence(c, idx, board, wl, allow_verdict_grep=True)
                    for c in r_cards}

        def card_state(card):
            return classify_r_evidence(evs[card])

        check("条件2 识别 19 张 tier=R（任一条目，含 tier 只写在 review）", lambda: (
            r_cards == sorted(board), str(r_cards)))
        check("条件2 board 已完成 -> PASS（不依赖 merge_result）", lambda: (
            card_state("openmir2-card-alpha")[0] == PASS
            and r_category(evs["openmir2-card-alpha"]) == "closed",
            card_state("openmir2-card-alpha")[1]))
        check("条件2 closed 且无 merge_result -> PASS", lambda: (
            card_state("openmir2-card-zeta")[0] == PASS
            and r_category(evs["openmir2-card-zeta"]) == "closed",
            card_state("openmir2-card-zeta")[1]))
        check("条件2 window_required 分支", lambda: (
            card_state("openmir2-card-beta")[0] == PASS, card_state("openmir2-card-beta")[1]))
        check("未合并窗口桶① window_required + 无 merge_result -> window_required_unmerged（仍 PASS）", lambda: (
            r_category(evs["openmir2-card-pi"]) == "window_required_unmerged"
            and card_state("openmir2-card-pi")[0] == PASS,
            card_state("openmir2-card-pi")[1]))
        check("未合并窗口桶② 同卡补 merge_result -> 回到 window_required", lambda: (
            r_category(evs["openmir2-card-rho"]) == "window_required"
            and card_state("openmir2-card-rho")[0] == PASS,
            card_state("openmir2-card-rho")[1]))
        check("条件2 whitelist T-* 分支（登记行点名）", lambda: (
            card_state("openmir2-card-gamma")[0] == PASS
            and evs["openmir2-card-gamma"]["strong_t"] == ["T-3"],
            card_state("openmir2-card-gamma")[1]))
        check("条件2 merge_result 但 board 非 closed -> 需人工", lambda: (
            card_state("openmir2-card-eta")[0] == NEED
            and r_category(evs["openmir2-card-eta"]) == "merged_not_closed",
            card_state("openmir2-card-eta")[1]))
        check("H-1 已裁定登记（approve+window_required:false+理由）-> PASS", lambda: (
            card_state("openmir2-card-lambda")[0] == PASS
            and r_category(evs["openmir2-card-lambda"]) == "adjudicated",
            card_state("openmir2-card-lambda")[1]))
        check("H-1 取最新 approve（早 false / 新 true，verdict 写法）", lambda: (
            (evs["openmir2-card-kappa"].get("adjudicated") or {}).get("kind") == "window_required",
            str(evs["openmir2-card-kappa"].get("adjudicated"))))
        check("H-1 approve+false 但无理由 -> 不算登记（仍需人工）", lambda: (
            card_state("openmir2-card-mu")[0] == NEED
            and r_category(evs["openmir2-card-mu"]) == "merged_not_closed",
            card_state("openmir2-card-mu")[1]))
        check("I-1 approve+false+tier_why=「时序/竞态」-> 需人工（不冒充免窗口）", lambda: (
            card_state("openmir2-card-nu")[0] == NEED
            and r_category(evs["openmir2-card-nu"]) == "merged_not_closed"
            and evs["openmir2-card-nu"]["adjudicated"] is None,
            card_state("openmir2-card-nu")[1]))
        check("I-1 命中白名单短语但含未否定否决词（时序）-> 需人工", lambda: (
            card_state("openmir2-card-nv")[0] == NEED
            and evs["openmir2-card-nv"]["adjudicated"] is None,
            card_state("openmir2-card-nv")[1]))
        check("I-1 无免窗口短语（只写改动性质）-> 不算登记", lambda: (
            card_state("openmir2-card-nw")[0] == NEED
            and evs["openmir2-card-nw"]["adjudicated"] is None,
            card_state("openmir2-card-nw")[1]))
        check("I-1 专用 why_not_window 字段优先 -> 已裁定 PASS", lambda: (
            card_state("openmir2-card-xi")[0] == PASS
            and r_category(evs["openmir2-card-xi"]) == "adjudicated"
            and (evs["openmir2-card-xi"].get("adjudicated") or {}).get("via") == "why_not_window",
            card_state("openmir2-card-xi")[1]))
        check("H-4 默认不认 VERDICT=PASS 子串（theta 不因子串转 PASS）", lambda: (
            r_category(evs["openmir2-card-theta"]) == "merged_not_closed"
            and card_state("openmir2-card-theta")[0] == NEED,
            card_state("openmir2-card-theta")[1]))
        check("H-4 --allow-verdict-grep 才认（theta -> PASS）", lambda: (
            classify_r_evidence(evs_grep["openmir2-card-theta"])[0] == PASS
            and r_category(evs_grep["openmir2-card-theta"]) == "comment_evidence",
            classify_r_evidence(evs_grep["openmir2-card-theta"])[1]))
        check("I-5 未开工/只建卡的卡 -> 未合并（不计 PASS/FAIL）", lambda: (
            card_state("openmir2-card-delta")[0] == NOT_MERGED
            and r_category(evs["openmir2-card-delta"]) == "not_merged",
            card_state("openmir2-card-delta")[1]))
        check("条件2 已开工却无任何登记 -> FAIL", lambda: (
            card_state("openmir2-card-omicron")[0] == FAIL
            and r_category(evs["openmir2-card-omicron"]) == "none",
            card_state("openmir2-card-omicron")[1]))
        check("条件2 未验签 -> 需人工", lambda: (
            card_state("openmir2-card-eps")[0] == NEED, card_state("openmir2-card-eps")[1]))
        check("条件2 弱关联 T 不冒认 PASS", lambda: (
            classify_r_evidence({"in_board": True, "unverified": 0, "merged": False,
                                 "window_required": False, "strong_t": [],
                                 "weak_t": ["T-1"]})[0] == NEED, "weak T"))
        check("条件2 board 缺失 -> 需人工", lambda: (
            classify_r_evidence({"in_board": False, "unverified": None, "merged": True,
                                 "window_required": False, "strong_t": [],
                                 "weak_t": []})[0] == NEED, "not in board"))

        state2, detail2, per2, fail2, need2 = judge_r_cards(list(evs.values()))
        check("条件2 聚合状态 FAIL（已开工无登记）", lambda: (
            state2 == FAIL
            and any(x["card"] == "openmir2-card-omicron" for x in fail2)
            and not any(x["card"] == "openmir2-card-delta" for x in fail2), detail2))
        check("H-2 need_items 按 last_status_ts 升序（最旧先）", lambda: (
            [x["card"] for x in need2]
            == ["openmir2-card-nu", "openmir2-card-nv", "openmir2-card-nw",
                "openmir2-card-theta", "openmir2-card-mu", "openmir2-card-eps",
                "openmir2-card-eta"]
            and [x["last_status_ts"] for x in need2] == sorted(x["last_status_ts"] for x in need2),
            str([(x["card"], x["last_status_ts"]) for x in need2])))
        # 卡 `openmir2-goal-gate-wording-fix`：window_required 与
        # window_required_unmerged 是**两个独立计数器**（`r_category` 只返回其一），
        # 所以 detail 必须写「另未合并」而不是「其中未合并」——「其中」会被读成
        # 「已含在那 N 张里」，实际这 1 张并不在 window_required 的 3 张里。
        check("条件2 detail 用「另未合并」且不含「其中未合并」（两个计数器不相交）", lambda: (
            "（另未合并 1）" in detail2 and "其中未合并" not in detail2, detail2))

        # ---------- 条件3 ----------
        rows = [
            {"status": "tracked", "name": "SM_DOOR_OK", "card": "openmir2-card-delta", "bucket": "outbound"},
            {"status": "tracked", "name": "CM_NO_T", "card": "openmir2-card-alpha", "bucket": "inbound"},
            {"status": "tracked", "name": "CM_ORPHAN", "card": "", "bucket": "inbound"},
            {"status": "implemented", "name": "SM_IGNORED", "card": "", "bucket": "outbound"},
        ]
        counts_ok = {"tracked": 3}
        b_state, b_detail, b_items, b_fails, b_needs = judge_b_tracked(rows, wl, board, counts_ok)
        by_name = {i["name"]: i for i in b_items}
        check("条件3 T-* 登记优先", lambda: (by_name["SM_DOOR_OK"]["category"] == "t", b_detail))
        check("条件3 有卡分支", lambda: (by_name["CM_NO_T"]["category"] == "card", b_detail))
        check("条件3 皆无 -> FAIL", lambda: (
            by_name["CM_ORPHAN"]["category"] == "none" and b_state == FAIL, b_detail))
        check("条件3 只算 tracked 不计 implemented", lambda: (
            "SM_IGNORED" not in by_name and len(b_items) == 3, str(len(b_items))))
        check("条件3 rows 为空 -> 需人工", lambda: (
            judge_b_tracked([], wl, board, {"tracked": 0})[0] == NEED, "empty rows"))
        check("条件3 rows 为 None -> 需人工", lambda: (
            judge_b_tracked(None, wl, board, {"tracked": 0})[0] == NEED, "none rows"))
        check("条件3 缺 counts -> 需人工", lambda: (
            judge_b_tracked(rows, wl, board, None)[0] == NEED, "no counts"))
        check("条件3 counts 不一致 -> 需人工", lambda: (
            judge_b_tracked(rows, wl, board, {"tracked": 15})[0] == NEED, "counts mismatch"))
        check("条件3 schema 漂移（status->state）-> 需人工", lambda: (
            judge_b_tracked([{"state": "tracked", "name": "X", "card": ""}], wl, board,
                            {"tracked": 1})[0] == NEED, "schema drift"))
        check("条件3 counts 一致时按条目判定", lambda: (
            judge_b_tracked(rows, wl, board, counts_ok)[0] == FAIL, b_detail))
        cleared_rows = [{"status": "implemented", "name": "SM_A", "card": ""},
                        {"status": "implemented", "name": "SM_B", "card": ""}]
        check("H-3 rows 非空且 counts.tracked=0 -> PASS（B 类清零）", lambda: (
            judge_b_tracked(cleared_rows, wl, board, {"tracked": 0})[0] == PASS
            and "B 类清零" in judge_b_tracked(cleared_rows, wl, board, {"tracked": 0})[1],
            str(judge_b_tracked(cleared_rows, wl, board, {"tracked": 0})[:2])))
        check("H-3 rows 非空但 counts.tracked>0 -> 需人工", lambda: (
            judge_b_tracked(cleared_rows, wl, board, {"tracked": 2})[0] == NEED,
            "counts mismatch"))

        # ---------- 条件4 ----------
        def chk(name, state):
            return {"name": name, "state": state, "detail": name}

        check("条件4 全绿 -> PASS", lambda: (
            judge_gate_checks([chk("fmt", PASS), chk("clippy", PASS)]) == PASS, "all pass"))
        check("条件4 一项红 -> FAIL", lambda: (
            judge_gate_checks([chk("fmt", PASS), chk("clippy", FAIL)]) == FAIL, "one fail"))
        check("条件4 一项未跑 -> 需人工", lambda: (
            judge_gate_checks([chk("fmt", PASS), chk("cargo_test_workspace", NEED)]) == NEED, "one need"))
        check("条件4 master 一致 -> PASS", lambda: (
            judge_remote("a" * 40, "a" * 40, {"rc": 0})[0] == PASS, "equal"))
        check("条件4 master 漂移 -> FAIL", lambda: (
            judge_remote("a" * 40, "b" * 40, {"rc": 0})[0] == FAIL, "drift"))
        check("条件4 ls-remote 不可用 -> 需人工", lambda: (
            judge_remote("a" * 40, "", {"rc": 1})[0] == NEED, "no remote"))
        # ---------- 条件4 工具自测门（三态牙；卡 openmir2-goal-gate-include-tool-selftest）----------
        import tempfile as _tempfile

        _tg_root = _tempfile.mkdtemp(prefix="goal-toolgate-")
        _tg_tool = Path(_tg_root, "tools", "hygiene", "tool_selftest_gate.py")
        _tg_tool.parent.mkdir(parents=True, exist_ok=True)
        _tg_tool.write_text("", encoding="utf-8")
        _tg_missing_root = _tempfile.mkdtemp(prefix="goal-toolgate-miss-")

        def _fake_run(rec):
            return lambda cmd, cwd, to: rec

        def _no_run(cmd, cwd, to):
            raise AssertionError("工具缺失时不应尝试运行")

        _tg_red = {"rc": 2, "duration_s": 9.0,
                   "stdout": "FAIL card_window rc=2  x\nUNCLASSIFIED tools/x.py\nTOTAL\n",
                   "stderr": ""}
        check("条件4 工具门 rc=0 -> PASS", lambda: (
            tool_gate_check(_tg_root, run=_fake_run(
                {"rc": 0, "duration_s": 0.1, "stdout": "GREEN\n", "stderr": ""}))["state"] == PASS,
            "green"))
        check("条件4 工具门 rc≠0 -> FAIL 且点名工具/未分类项", lambda: (
            tool_gate_check(_tg_root, run=_fake_run(_tg_red))["state"] == FAIL
            and "card_window" in tool_gate_check(_tg_root, run=_fake_run(_tg_red))["detail"]
            and "UNCLASSIFIED tools/x.py" in tool_gate_check(_tg_root, run=_fake_run(_tg_red))["detail"],
            "red"))
        check("条件4 工具门红 -> 条件④整体 FAIL（可进 fail_items）", lambda: (
            judge_gate_checks([tool_gate_check(_tg_root, run=_fake_run(_tg_red))]) == FAIL,
            "propagate"))
        check("条件4 工具缺失 -> NEED（不是 PASS）", lambda: (
            tool_gate_check(_tg_missing_root, run=_no_run)["state"] == NEED
            and "工具缺失" in tool_gate_check(_tg_missing_root, run=_no_run)["detail"],
            "missing"))
        check("条件4 工具无法启动 -> NEED", lambda: (
            tool_gate_check(_tg_root, run=_fake_run(
                {"rc": None, "error": "OSError: python 不可用"}))["state"] == NEED,
            "unavailable"))
        check("条件4 工具门超时 -> NEED", lambda: (
            tool_gate_check(_tg_root, run=_fake_run({"rc": None, "timed_out": True}))["state"] == NEED,
            "timeout"))

        # ---------- rc=3 ⇒ NEED（卡 openmir2-goalgate-need-rc3）----------
        _tg_need = {"rc": 3, "duration_s": 7.0,
                    "stdout": ("NEED docs_delivery_gate rc=2 1.6s  docs_delivery_gate selftest: 判不了\n"
                               "TOTAL 50 项：绿 49 / 红 0 / 需人工 1 / 跳过 0\n"),
                    "stderr": ""}
        check("条件4 工具门 rc=3（只有需人工）-> NEED 且点名 NEED 行", lambda: (
            tool_gate_check(_tg_root, run=_fake_run(_tg_need))["state"] == NEED
            and tool_gate_check(_tg_root, run=_fake_run(_tg_need))["rc"] == 3
            and "需人工" in tool_gate_check(_tg_root, run=_fake_run(_tg_need))["detail"]
            and "NEED docs_delivery_gate" in tool_gate_check(_tg_root, run=_fake_run(_tg_need))["detail"],
            "need-rc3"))
        check("条件4 工具门 rc=3 -> 条件④整体 NEED（不是 FAIL）", lambda: (
            judge_gate_checks([tool_gate_check(_tg_root, run=_fake_run(_tg_need))]) == NEED,
            "propagate-need"))
        check("条件4 rc=3 的归属：进 need_items、不进 fail_items（_cond 生产代码）", lambda: (
            (lambda c, fi, ni: (_cond(c["state"], 4, c["name"], c["detail"], fi, ni),
                                (not fi and len(ni) == 1 and ni[0]["item"] == "tool_selftest_gate"
                                 and ni[0]["condition"] == 4))[1])(
                tool_gate_check(_tg_root, run=_fake_run(_tg_need)), [], []),
            "need-attribution"))
        check("条件4 rc=2 仍 FAIL 且进 fail_items（三态不许把红放宽）", lambda: (
            (lambda c, fi, ni: (_cond(c["state"], 4, c["name"], c["detail"], fi, ni),
                                (c["state"] == FAIL and len(fi) == 1 and not ni))[1])(
                tool_gate_check(_tg_root, run=_fake_run(_tg_red)), [], []),
            "red-still-red"))

        # ---------- 组装级牙（A1，卡 openmir2-goalgate-quickmode-tooth）----------
        # 6 条三态牙都直接调 tool_gate_check()；这里钉「run_gate_checks 的组装顺序」：
        # 第 8 项必须与 with_tests 无关（--skip-tests 也跑）。把 append 缩进进
        # `if with_tests:` ⇒ 本牙红（其余 77 条仍绿，正是要抓的盲区）。
        _orig_run_cmd = globals()["run_cmd"]
        _gate_calls = []

        def _fake_gate_run(cmd, cwd, to):
            _gate_calls.append(list(cmd))
            return {"rc": 0, "duration_s": 0.0, "stdout": "", "stderr": "",
                    "cmd": list(cmd), "cwd": str(cwd)}

        globals()["run_cmd"] = _fake_gate_run
        try:
            _checks_quick = run_gate_checks(Path(_tg_root), Path(_tg_root),
                                            with_tests=False, timeout=5, test_timeout=5)
        finally:
            globals()["run_cmd"] = _orig_run_cmd
        _by = {c["name"]: c for c in _checks_quick}
        check("条件4 组装牙：快速模式仍跑第 8 项 tool_selftest_gate 且 PASS", lambda: (
            "tool_selftest_gate" in _by and _by["tool_selftest_gate"]["state"] == PASS,
            str(sorted(_by))))
        check("条件4 组装牙：快速模式 cargo_test_workspace 仍是 NEED", lambda: (
            _by.get("cargo_test_workspace", {}).get("state") == NEED,
            str(_by.get("cargo_test_workspace"))))

        check("总体状态优先级 FAIL > 需人工 > PASS", lambda: (
            overall_state([PASS, NEED]) == NEED
            and overall_state([PASS, NEED, FAIL]) == FAIL
            and overall_state([PASS, PASS]) == PASS, "precedence"))

        # ---------- 牙：把判定改坏必须能被上面的断言抓住 ----------
        def mutant_cov(_rec, parsed, counters=None):
            return (PASS, "mutant") if parsed else (NEED, "mutant")

        check("牙：忽略未实现计数的退化判定会被 FAIL 断言判死", lambda: (
            mutant_cov(rec_bad, parsed_bad, counters_ok)[0] == PASS
            and judge_script_coverage(rec_bad, parsed_bad, counters_ok)[0] == FAIL,
            "mutant detected"))

        def mutant_cov_no_rc(_rec, parsed, counters=None):
            return (PASS, "mutant") if parsed else (NEED, "mutant")

        check("牙：删掉 rc!=0 守卫会被『rc=1 且未实现=0 ⇒ FAIL』断言抓住", lambda: (
            mutant_cov_no_rc(rec_bad, parsed_ok, counters_ok)[0] == PASS
            and judge_script_coverage(rec_bad, parsed_ok, counters_ok)[0] == FAIL,
            "mutant detected"))

        def mutant_r(ev):
            _state, why = classify_r_evidence(ev)
            return (PASS, why)

        none_ev = {"in_board": True, "unverified": 0, "merged": False,
                   "window_required": False, "strong_t": [], "weak_t": []}
        check("牙：把三者皆无判成 PASS 的退化判定会被抓住", lambda: (
            mutant_r(none_ev)[0] == PASS and classify_r_evidence(none_ev)[0] == FAIL,
            "mutant detected"))

        def mutant_empty_tracked(rows_, wl_, board_, counts_=None):
            return (PASS, "mutant")

        check("牙：把空 tracked 判成 PASS 会被『查不到⇒需人工』断言抓住", lambda: (
            mutant_empty_tracked([], wl, board, {"tracked": 0})[0] == PASS
            and judge_b_tracked([], wl, board, {"tracked": 0})[0] == NEED, "mutant detected"))

        def mutant_zero_tracked(rows_, wl_, board_, counts_=None):
            return (NEED, "mutant：旧口径把 tracked=0 一律判需人工")

        def mutant_not_merged(ev):
            _state, why = classify_r_evidence(ev)
            return (FAIL, why)

        check("牙：把未开工卡判成 FAIL 会被 I-5 断言抓住", lambda: (
            mutant_not_merged(evs["openmir2-card-delta"])[0] == FAIL
            and classify_r_evidence(evs["openmir2-card-delta"])[0] == NOT_MERGED,
            "mutant detected"))

        check("牙：把 tracked=0 一律判需人工会被 H-3 PASS 断言抓住", lambda: (
            mutant_zero_tracked(cleared_rows, wl, board, {"tracked": 0})[0] == NEED
            and judge_b_tracked(cleared_rows, wl, board, {"tracked": 0})[0] == PASS,
            "mutant detected"))
    finally:
        shutil.rmtree(tmpdir, ignore_errors=True)

    failed = [r for r in results if not r["ok"]]
    print("selftest：%d 条断言，%d 通过，%d 失败" % (len(results), len(results) - len(failed), len(failed)))
    for item in failed:
        print("  FAIL %s -> %s" % (item["name"], item["detail"]))
    return 0 if not failed else 1


# --------------------------------------------------------------------------
# CLI
# --------------------------------------------------------------------------

def main(argv=None):
    parser = argparse.ArgumentParser(prog="goal_gate.py",
                                     description="OpenMir2 终点四条件机判（只读、零副作用）")
    parser.add_argument("command", nargs="?", default="run", choices=["run", "selftest"],
                        help="run（默认）= 机判四条件；selftest = 合成输入自检")
    parser.add_argument("--repo", default=None, help="仓库根（默认：本脚本所在仓库）")
    parser.add_argument("--envir", default=DEFAULT_ENVIR, help="脚本语料 Envir 目录（条件1）")
    parser.add_argument("--script-bin", default=None,
                        help="预编译 mir2-script-tool 路径（给了就跳过 cargo 构建，仍复算覆盖行）")
    parser.add_argument("--json", action="store_true", help="stdout 只输出 JSON")
    parser.add_argument("--tests", dest="with_tests", action="store_true",
                        help="跑 cargo test --workspace（完整判条件4）")
    parser.add_argument("--skip-tests", dest="skip_tests", action="store_true",
                        help="显式跳过 cargo test（快速模式默认已跳过；保留此开关表达意图）")
    parser.add_argument("--script", dest="with_script", action="store_true",
                        help="构建并运行 parse-stats（完整判条件1）")
    parser.add_argument("--skip-script", dest="skip_script", action="store_true",
                        help="显式跳过 parse-stats（快速模式默认已跳过）")
    parser.add_argument("--full", action="store_true", help="完整模式（= --tests --script）")
    parser.add_argument("--timeout", type=int, default=1800, help="单命令超时秒数（默认 1800）")
    parser.add_argument("--test-timeout", type=int, default=7200, help="cargo test 超时秒数（默认 7200）")
    parser.add_argument("--allow-verdict-grep", action="store_true",
                        help="条件2 采信 comment/review 里的 VERDICT=PASS 文本标记（默认关闭：子串匹配易假 PASS）")
    parser.add_argument("--verbose", action="store_true", help="selftest 逐条打印")
    args = parser.parse_args(argv)

    try:
        sys.stdout.reconfigure(errors="replace")
    except (AttributeError, ValueError):
        pass

    if args.command == "selftest":
        return selftest(verbose=args.verbose)

    if args.with_tests and args.skip_tests:
        print("--tests 与 --skip-tests 互斥", file=sys.stderr)
        return 3
    if args.with_script and args.skip_script:
        print("--script 与 --skip-script 互斥", file=sys.stderr)
        return 3

    repo = Path(args.repo).resolve() if args.repo else Path(__file__).resolve().parents[2]
    if not (repo / "mir2-rs" / "Cargo.toml").is_file():
        print("找不到 %s/mir2-rs/Cargo.toml；用 --repo 指定仓库根" % repo, file=sys.stderr)
        return 3

    with_tests = bool(args.with_tests or args.full) and not args.skip_tests
    with_script = bool(args.with_script or args.full or args.script_bin) and not args.skip_script

    report = collect_report(repo=repo, envir=args.envir, script_bin=args.script_bin,
                            with_tests=with_tests, with_script=with_script,
                            timeout=args.timeout, test_timeout=args.test_timeout,
                            allow_verdict_grep=args.allow_verdict_grep)

    if args.json:
        print(json.dumps(report, ensure_ascii=False, indent=2))
    else:
        print(render_table(report))

    if report["overall"] == PASS:
        return 0
    if report["overall"] == FAIL:
        return 1
    return 2


if __name__ == "__main__":
    sys.exit(main())
