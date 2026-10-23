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

CARD_ID_RE = re.compile(r"openmir2-[A-Za-z0-9][A-Za-z0-9._-]*")
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


def build_whitelist(path: Path, exclude_ids=None):
    """解析 whitelist.md 的 T-* 行。

    同一 T-* 编号出现多行（历史遗留重复号）时**并集**：卡集合取并、行文本拼接，
    避免后写覆盖把前一行的卡↔T 强关联吃掉（T-4/T-6 重复行的真实损害）。
    `exclude_ids` 用于排掉 principal 名（openmir2-worker-5 等），它们不是卡 id。
    """
    exclude_ids = set(exclude_ids or ())
    info = {"path": str(path), "rows": {}, "card_to_t": {}, "duplicates": [], "error": None}
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
        cards = {c for c in CARD_ID_RE.findall(line) if c not in exclude_ids}
        row = info["rows"].get(t_id)
        if row is None:
            info["rows"][t_id] = {"line": line, "cards": set(cards), "lines": [line]}
        else:
            info["duplicates"].append(t_id)
            row["cards"].update(cards)
            row["lines"].append(line)
            row["line"] = row["line"] + "\n" + line
    for t_id, row in info["rows"].items():
        for card in row["cards"]:
            info["card_to_t"].setdefault(card, set()).add(t_id)
    return info


def _t_sort_key(t_id):
    return int(t_id.split("-", 1)[1])


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
    if adj:
        # 复核裁定优先于历史条目：最新 approve 说 true ⇒ window_required；说 false+理由 ⇒ 已裁定不需窗口
        return "window_required" if adj.get("kind") == "window_required" else "adjudicated"
    if ev.get("window_required"):
        return "window_required"
    if ev.get("strong_t"):
        return "whitelist_t"
    if ev.get("evidence_entries"):
        return "comment_evidence"
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
    detail = ("R 卡 %d 张：已完成 %d / window_required %d / whitelist %d / 已裁定登记 %d / "
              "comment 证据 %d / 三者皆无 %d / merged 未 closed 且无登记(需人工) %d / "
              "未开工或未合并 %d（不计缺口）/ 其它需人工 %d" % (
                  len(per_card), counts.get("closed", 0), counts.get("window_required", 0),
                  counts.get("whitelist_t", 0), counts.get("adjudicated", 0),
                  counts.get("comment_evidence", 0), counts.get("none", 0),
                  counts.get("merged_not_closed", 0), counts.get("not_merged", 0),
                  counts.get("not_in_board", 0) + counts.get("unverified", 0)
                  + counts.get("weak_t", 0)))
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

    # ---- 条件2
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
    hygiene_categories = {"window_required", "whitelist_t", "adjudicated", "comment_evidence"}
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

        check("whitelist 解析出 4 个 T 行", lambda: (len(wl["rows"]) == 4, str(len(wl["rows"]))))
        check("whitelist 重复号 T-9 并集（卡不丢）", lambda: (
            wl["card_to_t"].get("openmir2-card-alpha") == {"T-1", "T-9"}
            and wl["card_to_t"].get("openmir2-card-beta") == {"T-9"}
            and wl["duplicates"] == ["T-9"], str(wl["card_to_t"])))
        check("whitelist exclude_ids 排掉 principal 名", lambda: (
            "openmir2-card-alpha" not in build_whitelist(
                wl_path, exclude_ids={"openmir2-card-alpha"})["card_to_t"], "excluded"))

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

        check("条件2 识别 17 张 tier=R（任一条目，含 tier 只写在 review）", lambda: (
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
