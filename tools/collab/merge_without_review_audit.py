#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""merge_without_review_audit — 只读普查：已合并但缺「非作者 approve」的卡。

卡：``openmir2-merge-without-review-audit``（tier=L）。目标是在 ``refs/collab/inbox/*``
的全部卡线程里，找出 ``merge_result.merged == true`` 却**没有**任何「非作者 approve」
review 的卡，供 owner/Leader 决定是否补做追溯审查。

**只读**：本工具只调用只读 git 子命令（``for-each-ref`` / ``cat-file`` / ``rev-parse`` /
``rev-list`` / ``merge-base``）。``_git()`` 对任何其它子命令 fail-closed（``ReadOnlyViolation``），
所以「跑普查」不可能顺手写 refs / 签条目 / 改卡状态。

## 判据（卡面 A1，逐条可复核）

1. **已合并**：该线程任一 ``kind=merge_result`` 且 ``body.merged is True``。
2. **approve**：``kind=review`` 的条目，verdict 取 ``body.decision`` 或 ``body.verdict``
   （两者都认；卡面要求）。值 == ``"approve"``。
   * 第三个承载字段 ``body.review``（reviewer-20/21 有 9 条）**不计入主判据**（卡面只要求
     verdict+decision），但单独统计并在报告里列出被它「救回」的卡，避免把真实 approve 藏掉。
3. **作者集合** = 该线程 ``kind in {patch, issue}`` 的 actor 集合（卡面口径）。approve 的
   actor 必须**不在**作者集合里；否则算作者自审。
4. **三张表**：
   * ① 已合并且有 >=1 非作者 approve —— 只给计数；
   * ② 已合并但**零 review**（线程里没有任何 ``kind=review``）—— 逐卡；
   * ③ 已合并且**只有作者自审**（有 review，但每一条的 actor 都在作者集合里）—— 逐卡。
   另有第四桶（非作者 review 存在但无 approve）不在卡面 ②/③，单独列出。
5. **真实合并提交**：对每条 ``kind=patch`` 的 ``entry.refs.head``（patch 的 head 在
   ``entry.refs``，不在 ``body``）解析成 commit，再取
   ``git rev-list --merges --ancestry-path <head>..<snapshot>`` **最老的一个**
   （= 把该 head 第一次并进主线的 merge）。求不出就写「未解析」+ 原因，**绝不猜**。
   * 账本 ``merge_result.oid`` 与真实合并提交不同时，单独标出（``ledger_ne_merge``）。
     已知例子：``openmir2-s1-oracle-defects`` 的账本 oid 其实是当时 master tip，
     真实合并提交不同。

## 用法

    python -I -B tools/collab/merge_without_review_audit.py report [--ref refs/remotes/walgit/master]
    python -I -B tools/collab/merge_without_review_audit.py report --json /tmp/audit.json
    python -I -B tools/collab/merge_without_review_audit.py selftest

退出码：``0`` 正常产出（普查本身不判红——它是在报告事实）；``2`` 用法/环境错（判不了）。
``selftest`` 全过 rc=0，失败 rc=1。
"""
from __future__ import annotations

import argparse
import datetime
import json
import os
import re
import subprocess
import sys

#: 允许的只读 git 子命令。_git() 对其它一律抛 ReadOnlyViolation。
READ_ONLY_SUBCOMMANDS = frozenset({
    "for-each-ref", "cat-file", "rev-parse", "rev-list", "merge-base",
})

#: 承载 approve verdict 的字段：主判据（卡面要求）与「另计」的第三个字段。
VERDICT_FIELDS = ("decision", "verdict")
LEGACY_VERDICT_FIELDS = ("review",)

#: 构成「作者集合」的条目 kind（卡面口径：patch/issue 作者）。
AUTHOR_KINDS = ("patch", "issue")

OID_RE = re.compile(r"^[0-9a-f]{40}$")
HEX_RE = re.compile(r"^[0-9a-f]{7,40}$")

#: 本地时区（项目 evidence 一律 +0800）。
LOCAL_TZ = datetime.timezone(datetime.timedelta(hours=8))


class ReadOnlyViolation(RuntimeError):
    """调用了非只读 git 子命令 —— 普查必须 fail-closed。"""


class GitUnavailable(RuntimeError):
    """git 调用失败（仓库损坏/对象缺失），属「判不了」。"""


# --------------------------------------------------------------------------- git
def repo_root(explicit=None):
    if explicit:
        return os.path.abspath(explicit)
    return os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))


def _git_proc(repo, args, stdin=None):
    sub = args[0] if args else ""
    if sub not in READ_ONLY_SUBCOMMANDS:
        raise ReadOnlyViolation("普查只允许只读 git 子命令，拒绝 %r" % (sub,))
    cmd = ["git", "-C", repo] + list(args)
    return subprocess.run(cmd, capture_output=True, input=stdin)


def _git(repo, args, check=True, binary=False, stdin=None):
    proc = _git_proc(repo, args, stdin=stdin)
    if check and proc.returncode != 0:
        raise GitUnavailable("%s 失败 rc=%d：%s"
                             % (" ".join(args), proc.returncode,
                                proc.stderr.decode("utf-8", "replace")[:300]))
    if binary:
        return proc
    return proc.stdout.decode("utf-8", "replace")


def load_entries(repo):
    """读 ``refs/collab/inbox/*`` 的全部条目 blob。

    返回 list[dict]，每条是**扁平** entry JSON（walgit 的 inbox blob 没有 ``entry`` 包层），
    并补 ``_oid``（blob oid）与 ``_ref``。
    """
    out = _git(repo, ["for-each-ref", "--format=%(refname)", "refs/collab/inbox"])
    refs = [r for r in out.splitlines() if r.strip()]
    if not refs:
        return []
    proc = _git(repo, ["cat-file", "--batch"], binary=True,
                stdin=("\n".join(refs) + "\n").encode())
    if proc.returncode != 0:
        raise GitUnavailable("cat-file --batch 失败 rc=%d" % proc.returncode)
    data = proc.stdout
    entries = []
    pos = 0
    idx = 0
    while pos < len(data):
        nl = data.find(b"\n", pos)
        if nl < 0:
            break
        hdr = data[pos:nl].split()
        if len(hdr) != 3:
            break
        sha, _typ, size = hdr[0].decode(), hdr[1], int(hdr[2])
        content = data[nl + 1:nl + 1 + size]
        pos = nl + 1 + size + 1
        try:
            ent = json.loads(content.decode("utf-8", "replace"))
        except (ValueError, UnicodeDecodeError):
            idx += 1
            continue
        ent["_oid"] = sha
        ent["_ref"] = refs[idx] if idx < len(refs) else None
        entries.append(ent)
        idx += 1
    return entries


def group_threads(entries):
    by_id = {}
    for e in entries:
        by_id.setdefault(e.get("id"), []).append(e)
    return by_id


# ------------------------------------------------------------------------ classify
def _verdict(body, fields):
    """返回 (field, value)；body 不是 dict 时返回 (None, None)。"""
    if not isinstance(body, dict):
        return None, None
    for f in fields:
        if f in body:
            return f, body.get(f)
    return None, None


def _is_approve(body, fields):
    _f, val = _verdict(body, fields)
    return val == "approve"


def authors_of(entries):
    return sorted({e.get("actor") for e in entries if e.get("kind") in AUTHOR_KINDS})


def patch_authors_of(entries):
    return sorted({e.get("actor") for e in entries if e.get("kind") == "patch"})


def issue_authors_of(entries):
    return sorted({e.get("actor") for e in entries if e.get("kind") == "issue"})


def merge_results(entries):
    return [e for e in entries if e.get("kind") == "merge_result"]


def is_merged(entries):
    return any(isinstance(e.get("body"), dict) and e["body"].get("merged") is True
               for e in merge_results(entries))


def effective_tier(entries):
    """(tier, source)。R 优先于 L；来源依次 issue -> review -> merge_result。"""
    def tiers(kind):
        vals = []
        for e in entries:
            if e.get("kind") != kind:
                continue
            b = e.get("body")
            if isinstance(b, dict) and isinstance(b.get("tier"), str):
                vals.append(b["tier"])
        return vals

    for kind in ("issue", "review", "merge_result"):
        for v in tiers(kind):
            if v.strip().upper().startswith("R"):
                return "R", kind
    for kind in ("issue", "review", "merge_result"):
        for v in tiers(kind):
            if v.strip().upper().startswith("L"):
                return "L", kind
    return "未标注", "—"


def classify_thread(entries):
    """单线程分类。返回 dict（含 bucket / 作者 / review / approve 明细）。"""
    authors = set(authors_of(entries))
    reviews = [e for e in entries if e.get("kind") == "review"]
    nonauthor_approves = [e for e in reviews
                          if e.get("actor") not in authors
                          and _is_approve(e.get("body"), VERDICT_FIELDS)]
    legacy_approves = [e for e in reviews
                       if e.get("actor") not in authors
                       and _is_approve(e.get("body"), LEGACY_VERDICT_FIELDS)
                       and not _is_approve(e.get("body"), VERDICT_FIELDS)]
    nonauthor_reviews = [e for e in reviews if e.get("actor") not in authors]
    if nonauthor_approves:
        bucket = "with_nonauthor_approve"
    elif not reviews:
        bucket = "zero_review"
    elif all(e.get("actor") in authors for e in reviews):
        bucket = "only_self_review"
    elif nonauthor_reviews:
        bucket = "nonauthor_review_no_approve"
    else:  # 理论上不可达：有 review 但既非全作者、又无任何非作者条目
        bucket = "only_self_review"
    return {
        "authors": sorted(authors),
        "patch_authors": patch_authors_of(entries),
        "issue_authors": issue_authors_of(entries),
        "reviews": reviews,
        "nonauthor_approves": nonauthor_approves,
        "legacy_approves": legacy_approves,
        "bucket": bucket,
    }


# --------------------------------------------------------------------- merge resolve
def resolve_commit(repo, spec):
    """把 patch head 说明解析成 commit oid。返回 (oid|None, reason)。"""
    if not isinstance(spec, str) or not spec.strip():
        return None, "head 为空"
    spec = spec.strip()
    cands = []
    if spec.startswith("refs/"):
        cands.append(spec)
        if spec.startswith("refs/heads/"):
            base = spec[len("refs/heads/"):]
            cands.append("refs/remotes/walgit/" + base)
            cands.append("refs/remotes/origin/" + base)
    elif HEX_RE.match(spec):
        cands.append(spec + "^{commit}")
    else:
        cands.append(spec)
    for c in cands:
        p = _git_proc(repo, ["rev-parse", "--verify", "--quiet", c])
        if p.returncode == 0:
            oid = p.stdout.decode("utf-8", "replace").strip()
            t = _git(repo, ["cat-file", "-t", oid], check=False).strip()
            if t == "commit":
                return oid, None
    return None, "head 无法解析为 commit：%r" % spec[:60]


def real_merge(repo, head_spec, snapshot):
    """head -> 把该 head 第一次并进主线的 merge 提交。返回 (oid|None, reason)。"""
    oid, reason = resolve_commit(repo, head_spec)
    if not oid:
        return None, reason
    p = _git_proc(repo, ["merge-base", "--is-ancestor", oid, snapshot])
    if p.returncode != 0:
        return None, "head=%s 不是快照 %s 的祖先" % (oid[:12], snapshot[:12])
    out = _git(repo, ["rev-list", "--merges", "--ancestry-path",
                      oid + ".." + snapshot], check=False).split()
    if not out:
        return None, "无 merge 提交（fast-forward 或直接提交到主线）"
    return out[-1], None  # rev-list 新到旧 => 末项最老（第一次并入）


def patch_heads(entries):
    """patch 条目的 head 列表（head 在 entry.refs，不在 body；兼容 body.head 兜底）。"""
    heads = []
    for e in entries:
        if e.get("kind") != "patch":
            continue
        refs = e.get("refs")
        h = refs.get("head") if isinstance(refs, dict) else None
        if not h:
            b = e.get("body")
            if isinstance(b, dict):
                h = b.get("head")
        if isinstance(h, str) and h.strip():
            heads.append(h.strip())
    return heads


def _prefix_equal(a, b):
    """短/长 oid 的等价比较：一方是另一方的前缀（都小写 hex）时视为同一对象。"""
    if not isinstance(a, str) or not isinstance(b, str):
        return False
    a, b = a.strip().lower(), b.strip().lower()
    if not (HEX_RE.match(a) and HEX_RE.match(b)):
        return False
    return a.startswith(b) or b.startswith(a)


def resolve_card_merges(repo, entries, snapshot):
    """返回 (merges, reasons)。merges = 去重后的真实合并提交（保序）。"""
    heads = patch_heads(entries)
    if not heads:
        return [], ["无线程 patch 条目（patch head 不可得）"]
    merges, reasons = [], []
    for h in heads:
        oid, why = real_merge(repo, h, snapshot)
        if oid:
            if oid not in merges:
                merges.append(oid)
        elif why:
            reasons.append(why)
    return merges, reasons


def _ts(ts):
    if not isinstance(ts, (int, float)):
        return ""
    return datetime.datetime.fromtimestamp(ts, LOCAL_TZ).strftime("%Y-%m-%d %H:%M")


def card_row(repo, card_id, entries, snapshot):
    cls = classify_thread(entries)
    mrs = [e for e in merge_results(entries) if isinstance(e.get("body"), dict)
           and e["body"].get("merged") is True]
    mr_oids = []
    for e in mrs:
        oid = e["body"].get("oid")
        if isinstance(oid, str) and oid:
            mr_oids.append(oid)
    merges, reasons = resolve_card_merges(repo, entries, snapshot)
    tier, tier_src = effective_tier(entries)
    ts_vals = [e.get("ts") for e in mrs if isinstance(e.get("ts"), (int, float))]
    ts = max(ts_vals) if ts_vals else None
    if merges:
        ledger_ne_merge = not any(_prefix_equal(o, m) for o in mr_oids for m in merges)
    else:
        ledger_ne_merge = None
    return {
        "card": card_id,
        "merge_result_oids": mr_oids,
        "real_merges": merges,
        "real_merge_reason": "; ".join(reasons) if reasons else "",
        "patch_authors": cls["patch_authors"],
        "issue_authors": cls["issue_authors"],
        "authors": cls["authors"],
        "is_r": tier,
        "tier_source": tier_src,
        "merge_ts": _ts(ts),
        "review_actors": sorted({e.get("actor") for e in cls["reviews"]}),
        "legacy_approves": [(e.get("actor"), e.get("_oid")) for e in cls["legacy_approves"]],
        "ledger_ne_merge": ledger_ne_merge,
    }


def build_report(repo, ref="refs/remotes/walgit/master"):
    snapshot = _git(repo, ["rev-parse", "--verify", ref]).strip()
    if not OID_RE.match(snapshot):
        raise GitUnavailable("ref %r 不解析为 40 位 commit：%r" % (ref, snapshot[:60]))
    entries = load_entries(repo)
    threads = group_threads(entries)
    counts = {
        "entries": len(entries),
        "threads_total": len(threads),
        "merged_threads": 0,
        "with_nonauthor_approve": 0,
        "zero_review": 0,
        "only_self_review": 0,
        "nonauthor_review_no_approve": 0,
        "legacy_field_approve": 0,
    }
    tables = {"zero_review": [], "only_self_review": [],
              "nonauthor_review_no_approve": []}
    for card_id, es in sorted(threads.items(), key=lambda kv: str(kv[0])):
        if not is_merged(es):
            continue
        counts["merged_threads"] += 1
        cls = classify_thread(es)
        bucket = cls["bucket"]
        if bucket == "with_nonauthor_approve":
            counts["with_nonauthor_approve"] += 1
            continue
        counts[bucket] = counts.get(bucket, 0) + 1
        if bucket == "nonauthor_review_no_approve" and cls["legacy_approves"]:
            counts["legacy_field_approve"] += 1
        tables[bucket].append(card_row(repo, card_id, es, snapshot))
    return {"snapshot": snapshot, "ref": ref, "counts": counts, "tables": tables}


# ------------------------------------------------------------------------ render
def _fmt_list(vals):
    return ", ".join(vals) if vals else "—"


def render_markdown(data):
    lines = []
    c = data["counts"]
    lines.append("快照：`%s`（`%s`）" % (data["snapshot"], data["ref"]))
    lines.append("")
    lines.append("条数：线程 %d；已合并 %d；非作者 approve %d；"
                 "零 review %d；仅作者自审 %d；非作者 review 但无 approve %d。"
                 % (c["threads_total"], c["merged_threads"],
                    c["with_nonauthor_approve"], c["zero_review"],
                    c["only_self_review"], c["nonauthor_review_no_approve"]))
    lines.append("")
    header = ("| 卡 id | merge_result.oid | 真实合并提交 | patch 作者 | 是否 R 卡 | 时间 |")
    sep = "|---|---|---|---|---|---|"

    def table(title, rows):
        lines.append("### %s（%d）" % (title, len(rows)))
        lines.append("")
        lines.append(header)
        lines.append(sep)
        for r in rows:
            mr = _fmt_list(r["merge_result_oids"]) if r["merge_result_oids"] else "—"
            if r["real_merges"]:
                real = ", ".join(r["real_merges"])
                if r["ledger_ne_merge"]:
                    real += "（账本 oid ≠ 合并提交）"
            else:
                real = "未解析：%s" % (r["real_merge_reason"] or "原因未知")
            authors = _fmt_list(r["patch_authors"])
            if not r["patch_authors"] and r["issue_authors"]:
                authors = "—（无线程 patch；issue 作者 " + ", ".join(r["issue_authors"]) + "）"
            lines.append("| %s | %s | %s | %s | %s | %s |"
                         % (r["card"], mr, real, authors, r["is_r"], r["merge_ts"]))
        lines.append("")

    table("② 已合并但零 review", data["tables"]["zero_review"])
    table("③ 已合并但只有作者自审", data["tables"]["only_self_review"])
    self_rows = data["tables"]["only_self_review"]
    if self_rows:
        lines.append("### ③ 补充：自审者与作者集合的关系")
        lines.append("")
        lines.append("| 卡 id | review actor | 是 patch 作者 | 是 issue 作者 |")
        lines.append("|---|---|---|---|")
        for r in self_rows:
            for actor in r["review_actors"]:
                lines.append("| %s | %s | %s | %s |"
                             % (r["card"], actor,
                                "是" if actor in r["patch_authors"] else "否",
                                "是" if actor in r["issue_authors"] else "否"))
        lines.append("")
    table("④ 非作者 review 存在但无 approve（卡面 ②/③ 之外）",
          data["tables"]["nonauthor_review_no_approve"])
    legacy = [r for r in data["tables"]["nonauthor_review_no_approve"] if r["legacy_approves"]]
    if legacy:
        lines.append("其中 %d 张的 approve 写在 `body.review`（非 card 面要求的 decision/verdict）："
                     % len(legacy))
        for r in legacy:
            lines.append("- %s：%s" % (r["card"],
                                       ", ".join("%s `%s`" % (a, o) for a, o in r["legacy_approves"])))
        lines.append("")
    return "\n".join(lines)


# ------------------------------------------------------------------------ selftest
def _raw_git(cwd, *args, env_extra=None):
    env = dict(os.environ)
    if env_extra:
        env.update(env_extra)
    return subprocess.run(["git", "-C", cwd] + list(args), capture_output=True,
                          text=True, env=env)


def _mk_repo(tmp):
    _raw_git(tmp, "init", "-q", "-b", "master")
    _raw_git(tmp, "config", "user.email", "selftest@example.invalid")
    _raw_git(tmp, "config", "user.name", "selftest")
    return tmp


def _commit(tmp, path, content, msg):
    with open(os.path.join(tmp, path), "w", encoding="utf-8") as f:
        f.write(content)
    _raw_git(tmp, "add", path)
    _raw_git(tmp, "commit", "-q", "-m", msg)
    return _raw_git(tmp, "rev-parse", "HEAD").stdout.strip()


def selftest():
    import tempfile
    ok = 0

    def check(cond, label):
        nonlocal ok
        if not cond:
            raise AssertionError("SELFTEST FAIL: " + label)
        ok += 1

    # --- verdict 两种字段都认 ---
    check(_is_approve({"decision": "approve"}, VERDICT_FIELDS), "decision approve")
    check(_is_approve({"verdict": "approve"}, VERDICT_FIELDS), "verdict approve")
    check(not _is_approve({"decision": "request_changes"}, VERDICT_FIELDS),
          "request_changes not approve")
    check(not _is_approve({}, VERDICT_FIELDS), "empty body not approve")
    check(_is_approve({"review": "approve"}, LEGACY_VERDICT_FIELDS),
          "legacy review field recognised separately")
    check(not _is_approve({"review": "approve"}, VERDICT_FIELDS),
          "legacy field must NOT count in main predicate")

    # --- 作者集合：patch 与 issue 都算 ---
    es = [
        {"kind": "issue", "actor": "coord", "body": {}, "_oid": "a1"},
        {"kind": "patch", "actor": "worker", "refs": {}, "_oid": "a2"},
        {"kind": "review", "actor": "worker", "body": {"decision": "approve"}, "_oid": "a3"},
        {"kind": "review", "actor": "coord", "body": {"verdict": "approve"}, "_oid": "a4"},
    ]
    cls = classify_thread(es)
    check(cls["bucket"] == "only_self_review", "patch+issue author self-review excluded")
    check(cls["authors"] == ["coord", "worker"], "author set = patch+issue actors")

    # --- 非作者 approve 命中 -- 主桶 ---
    es2 = [
        {"kind": "issue", "actor": "coord", "body": {}, "_oid": "b1"},
        {"kind": "patch", "actor": "worker", "refs": {}, "_oid": "b2"},
        {"kind": "review", "actor": "rev", "body": {"decision": "approve"}, "_oid": "b3"},
    ]
    check(classify_thread(es2)["bucket"] == "with_nonauthor_approve", "nonauthor ok bucket")

    # --- 零 review ---
    es3 = [{"kind": "issue", "actor": "coord", "body": {}, "_oid": "c1"}]
    check(classify_thread(es3)["bucket"] == "zero_review", "zero_review bucket")

    # --- 非作者 review 但无 approve（且 legacy 可救回）---
    es4 = [
        {"kind": "patch", "actor": "worker", "refs": {}, "_oid": "d1"},
        {"kind": "review", "actor": "rev", "body": {"decision": "request_changes"}, "_oid": "d2"},
    ]
    c4 = classify_thread(es4)
    check(c4["bucket"] == "nonauthor_review_no_approve", "no-approve bucket")
    check(c4["legacy_approves"] == [], "no legacy here")
    es5 = [
        {"kind": "patch", "actor": "worker", "refs": {}, "_oid": "e1"},
        {"kind": "review", "actor": "rev", "body": {"review": "approve"}, "_oid": "e2"},
    ]
    c5 = classify_thread(es5)
    check(c5["bucket"] == "nonauthor_review_no_approve", "legacy still 4th bucket")
    check(len(c5["legacy_approves"]) == 1, "legacy approve detected")

    # --- 空输入 ---
    check(classify_thread([])["bucket"] == "zero_review", "empty -> zero_review")
    check(effective_tier([]) == ("未标注", "—"), "empty tier")

    # --- tier ---
    check(effective_tier([{"kind": "issue", "body": {"tier": "R"}}])[0] == "R", "tier R")
    check(effective_tier([{"kind": "issue", "body": {"tier": "L"}}])[0] == "L", "tier L")
    check(effective_tier([
        {"kind": "issue", "body": {}},
        {"kind": "review", "body": {"tier": "R"}},
    ])[0] == "R", "tier falls back to review")

    # --- 渲染：无线程 patch 时仍显示 issue 作者上下文（避免空 patch 作者被读成「无人」）---
    row_nopatch = {
        "card": "card-np", "merge_result_oids": ["deadbeef"], "real_merges": [],
        "real_merge_reason": "无线程 patch 条目（patch head 不可得）",
        "patch_authors": [], "issue_authors": ["coord"], "authors": ["coord"],
        "is_r": "未标注", "tier_source": "—", "merge_ts": "2026-01-01 00:00",
        "review_actors": [], "legacy_approves": [], "ledger_ne_merge": None,
    }
    md = render_markdown({
        "snapshot": "0" * 40, "ref": "x",
        "counts": {"threads_total": 1, "merged_threads": 1,
                   "with_nonauthor_approve": 0, "zero_review": 1,
                   "only_self_review": 0, "nonauthor_review_no_approve": 0},
        "tables": {"zero_review": [row_nopatch], "only_self_review": [],
                   "nonauthor_review_no_approve": []},
    })
    check("无线程 patch；issue 作者 coord" in md, "render shows issue author fallback")
    check("未解析" in md, "render marks unresolved merge")

    # --- 只读守卫 ---
    try:
        _git(".", ["commit", "-m", "x"])
        raise AssertionError("SELFTEST FAIL: write subcommand not rejected")
    except ReadOnlyViolation:
        ok += 1
    try:
        _git(".", ["push", "walgit", "master"])
        raise AssertionError("SELFTEST FAIL: push not rejected")
    except ReadOnlyViolation:
        ok += 1

    # --- 真实合并提交解析（临时仓库）---
    with tempfile.TemporaryDirectory() as tmp:
        _mk_repo(tmp)
        base = _commit(tmp, "f.txt", "0\n", "base")
        _raw_git(tmp, "checkout", "-q", "-b", "work")
        patch = _commit(tmp, "f.txt", "1\n", "patch")
        _raw_git(tmp, "checkout", "-q", "master")
        _raw_git(tmp, "merge", "--no-ff", "-q", "-m", "merge work", "work")
        merge = _raw_git(tmp, "rev-parse", "HEAD").stdout.strip()
        got, why = real_merge(tmp, patch, merge)
        check(why is None, "merge resolve no reason")
        check(got == merge, "merge resolve == merge commit")
        check(got != patch, "merge resolve != patch head")
        # 短 oid 也能解析
        got2, _ = real_merge(tmp, patch[:9], merge)
        check(got2 == merge, "short oid resolves")
        # fast-forward：主线直接前进到 head，无 merge 提交 -> 未解析（带原因，不猜）
        _raw_git(tmp, "checkout", "-q", "-b", "ff")
        ff = _commit(tmp, "g.txt", "1\n", "ff")
        _raw_git(tmp, "checkout", "-q", "master")
        _raw_git(tmp, "merge", "--ff-only", "-q", "ff")
        snap = _raw_git(tmp, "rev-parse", "HEAD").stdout.strip()
        ff_merge, ff_why = real_merge(tmp, ff, snap)
        check(ff_merge is None and "fast-forward" in (ff_why or ""), "ff -> unresolved reason")
        # 不存在/不可解析 head -> 未解析（不猜）
        bad, bad_why = real_merge(tmp, "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", merge)
        check(bad is None and bad_why, "unresolvable head -> reason, not guess")

        # 端到端：合成一个卡线程，走 build_report 的纯逻辑部分
        snapshot = _raw_git(tmp, "rev-parse", "master").stdout.strip()
        entries = [
            {"id": "card-x", "kind": "issue", "actor": "coord", "body": {}, "ts": 1,
             "_oid": "x1"},
            {"id": "card-x", "kind": "patch", "actor": "worker",
             "refs": {"base": base, "head": patch}, "body": {}, "ts": 2, "_oid": "x2"},
            {"id": "card-x", "kind": "merge_result", "actor": "coord",
             "body": {"merged": True, "oid": patch}, "ts": 3, "_oid": "x3"},
        ]
        check(is_merged(entries), "is_merged true")
        merges, _ = resolve_card_merges(tmp, entries, snapshot)
        check(len(merges) == 1 and merges[0] == merge, "resolve_card_merges end-to-end")
        row = card_row(tmp, "card-x", entries, snapshot)
        check(row["ledger_ne_merge"] is True, "ledger oid != merge commit flagged")
        check(row["is_r"] == "未标注", "untagged tier reported")
        # 账本 oid == 合并提交时不应标红
        entries[2]["body"]["oid"] = merge
        row2 = card_row(tmp, "card-x", entries, snapshot)
        check(row2["ledger_ne_merge"] is False, "equal ledger oid not flagged")

    print("SELFTEST PASS %d/%d" % (ok, ok))
    return 0


# ---------------------------------------------------------------------------- main
def main(argv=None):
    ap = argparse.ArgumentParser(
        prog="merge_without_review_audit",
        description="只读普查：已合并但缺非作者 approve 的卡")
    sub = ap.add_subparsers(dest="cmd")
    rep = sub.add_parser("report", help="产出三张表")
    rep.add_argument("--repo", default=None)
    rep.add_argument("--ref", default="refs/remotes/walgit/master")
    rep.add_argument("--json", default=None, help="把完整结果写成 JSON")
    sub.add_parser("selftest", help="跑合成自测")
    args = ap.parse_args(argv)
    if args.cmd == "selftest":
        return selftest()
    if args.cmd != "report":
        ap.print_help(sys.stderr)
        return 2
    repo = repo_root(args.repo)
    try:
        data = build_report(repo, args.ref)
    except (GitUnavailable, ReadOnlyViolation) as exc:
        print("判不了：%s" % exc, file=sys.stderr)
        return 2
    print(render_markdown(data))
    if args.json:
        with open(args.json, "w", encoding="utf-8") as f:
            json.dump(data, f, ensure_ascii=False, indent=1)
    return 0


if __name__ == "__main__":
    sys.exit(main())
