#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""wl_status_audit — **只读**普查 whitelist 行上的「待独立 principal 复核」状态。

卡：`openmir2-whitelist-status-audit`（tier=L）。背景：`mir2-rs/tests/parity/whitelist.md`
里有 84 行写着「待独立 principal 复核」、另有一行散文「（`useitem.rs`，待审）」。
这些字样是否属实，不能按卡号猜，必须读 **collab 卡线程** 的 `review`/`merge_result` 条目：
早期 review 用 `body.verdict`、近期用 `body.decision`，**两个字段都要认**（只认一个会把
「有审的卡」误判成「没审」）。

**判定域（为什么是只读）**：本工具只输出普查表（markdown/json），不改 whitelist、不改任何
卡片状态。改不改行、改成什么，由人工按表裁定并另提 patch。

**证据口径**：
  * 线程来源：`refs/collab/inbox/*` 的 blob（`walgit collab thread` 的同一份数据）。
  * 写者：`kind=patch` 的 `actor` 集合（无 patch 时退回 `issue` 的 actor）。
  * 独立 approve：`kind=review` 且 `body.decision|body.verdict == "approve"` 且 actor **不在**写者集合。
  * 合并：`kind=merge_result` 且 `body.merged` 为真；合并 oid 取 `body.oid`（老条目是 9 位短 oid）。
  * 行 → 卡：行内 `卡 openmir2-…`（反引号包住）直接可用；**行内引用不一定是交付卡**（可能是「承接卡」），
    这类由 `CARD_HINTS` 显式给出并附依据；无卡/无线程一律判「证据不足」，**保持原样**。

用法：
  python -I -B tools/whitelist/wl_status_audit.py census [--repo .] [--whitelist PATH] [--json OUT]
  python -I -B tools/whitelist/wl_status_audit.py selftest
退出码：census 0=产表成功 / 2=判不了（缺 collab refs、行数漂移等）；selftest 0=全过 / 2=有失败。
"""
import argparse
import json
import os
import re
import subprocess
import sys

PHRASE = "待独立 principal 复核"
PROSE_PHRASE = "（`useitem.rs`，待审）"
ROW_RE = re.compile(r"^\|\s*(T-\d+)\s*\|")
CANON_RE = re.compile(r"卡\s*`(openmir2-[a-z0-9][a-z0-9-]*)`")
BARE_RE = re.compile(r"卡\s+([A-Za-z0-9#][A-Za-z0-9# _/\-]*)")
PRINCIPAL_RE = re.compile(
    r"^openmir2-(?:worker|svc|dev|reviewer|coordinator|orchestrator|leader)(?:-\d+)?$")

#: 行内没有「交付卡」的 canonical 卡号时，显式登记「本行实际交付卡 + 依据」。
#: key = T 号（或 `PROSE-useitem`）；value = (cards, why)。**不许按关键词猜**——
#: 每条 why 都必须能在线程里复核（patch blob / 证据目录 / 卡标题）。
CARD_HINTS = {
    "T-50": (["openmir2-m3-drop-exp-fixture-botsrv"],
             "行内 `openmir2-m3-rng-scene-driver` 是**承接卡**；本行证据目录 "
             "`m3/drop-exp/evidence-drop-exp-fixture-botsrv-20261014/**` 即交付卡，"
             "且该卡 patch `2c1d8d24` 自述「whitelist.md 的 M3④ 段新增 T-50」"),
    "T-57": (["openmir2-m3-drop-exp-wiring"],
             "patch `220019d7` 自述「whitelist T-57/T-58（12 文件）」，同一 X2 片"),
    "T-58": (["openmir2-m3-drop-exp-wiring"],
             "行内 `openmir2-m3-quest-state-dump` 是**承接卡**；交付卡见 patch `220019d7`"
             "「whitelist T-57/T-58」"),
    "T-119": (["openmir2-gategate-keepalive"],
              "patch `e1984470` 登记 T-119（merge 注「重基时 T-118 冲突 ⇒ 重编号 T-119」）"),
    "T-125": (["openmir2-castle-war-area-impl"], "行内卡 shorthand `castle-war-impl`；patch 自述「whitelist.md（T-125..T-132）」"),
    "T-126": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-128": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-129": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-130": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-131": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-132": (["openmir2-castle-war-area-impl"], "同上（T-125..T-132）"),
    "T-134": (["openmir2-monster-ref-drains"], "行内卡 shorthand `monster-ref-drains`；issue 正文列 T-134/T-135/T-148"),
    "T-135": (["openmir2-monster-ref-drains"], "同上"),
    "T-138": (["openmir2-t138-request-castle-war"],
              "行内 `audit #3` 指向另一卡；T-138 的交付卡标题即「T-138 GuildOfficial @requestcastlewarnowN」，"
              "patch `dbdceb92` 自述 T-138"),
    "T-139": (["openmir2-guild-official-npc"],
              "行内「卡 audit #3 / 车道 guildofficial」；`openmir2-guild-official-npc` 标题含「（audit #3）」"),
    "T-142": (["openmir2-script-playdice-openybdeal"],
              "行内 `scriptpd` 是车道名；patch `b04975f6` 自述「登记 T-142 与红检四件套」"),
    "T-145": (["openmir2-market-def-impl"],
              "行内 `marketdef` 是车道名；patch `66a8a4c3` 自述「T-145 登记」"),
    "T-148": (["openmir2-monster-ref-drains"], "行内卡 shorthand `monster-ref-drains`"),
    "T-152": (["openmir2-a22-castle-persistence"],
              "行内「A-22-P1 castle-persist-world」；patch `7531d74e` 自述「whitelist.md（T-152/T-153）」"),
    "T-153": (["openmir2-a22-castle-persistence"], "同上（T-152/T-153）"),
    "T-155": (["openmir2-npc-ctor-constants", "openmir2-guard-ctor-constants"],
              "本行含两个分项卡：`npc-ctor`（主）+ `guard-ctor`（分项）；两卡 patch 均登记 T-155"),
    "T-156": (["openmir2-gm-commands-b1"],
              "行内卡 shorthand `gm-commands-b1`；patch `af4d3d10` 自述「T-156 登记」"),
    "PROSE-useitem": (["openmir2-m3-bag-equip-alignment"],
                      "散文所在 M3 段的段头卡即 `openmir2-m3-bag-equip-alignment`；"
                      "④ useitem.rs 由 merge `03f40600`（approve reviewer-6 `f069955a`）合入"),
}


class AuditError(Exception):
    """判不了（不是「证伪」）：缺 collab refs / 行数漂移 / 形状不对。rc=2。"""


#: 某些卡在生命周期里有多次 review/merge（不同分片），但**本行**只由其中一次覆盖。
#: 这里把普查表钉到「覆盖本行的那一条」，避免把别的分片的 approve 算到本行头上。
#: key = T 号；value = (approve oid 前缀列表, merge oid 前缀列表)。
PIN_EVIDENCE = {
    "T-50": (["8198aec1"], ["30c52f85"]),
    "T-57": (["8fcd4392"], ["07dd178c"]),
    "T-58": (["8fcd4392"], ["07dd178c"]),
    "PROSE-useitem": (["f069955a"], ["60095f65"]),
}


def parse_rows(text):
    """从 whitelist 文本取待普查行。返回 [{'line','tid','text','target'}]。"""
    rows = []
    for lineno, line in enumerate(text.split("\n"), 1):
        if PHRASE in line:
            m = ROW_RE.match(line)
            tid = m.group(1) if m else None
            rows.append({"line": lineno, "tid": tid, "text": line, "target": PHRASE})
        elif PROSE_PHRASE in line:
            rows.append({"line": lineno, "tid": "PROSE-useitem", "text": line,
                         "target": PROSE_PHRASE})
    return rows


def resolve_cards(row, thread_ids):
    """行 → 交付卡列表。先 CARD_HINTS，再行内 canonical 卡号；其余返回 []（证据不足）。"""
    tid = row["tid"]
    if tid in CARD_HINTS:
        return list(CARD_HINTS[tid][0])
    text = row["text"]
    cards = []
    for m in CANON_RE.finditer(text):
        c = m.group(1)
        # 保留「命名了但线程不存在」的卡号：那是 NO_THREAD（证据不足），不是 NO_CARD
        if not PRINCIPAL_RE.match(c) and c not in cards:
            cards.append(c)
    if cards:
        return cards
    # 行内 bare shorthand：只接受唯一解析（精确 / 后缀），歧义即视为证据不足
    for m in BARE_RE.finditer(text):
        name = m.group(1).strip().rstrip("/").strip()
        if not name or name.startswith("openmir2-"):
            continue
        cand = [t for t in thread_ids if t == name or t.endswith("-" + name)]
        if len(cand) == 1 and cand[0] not in cards:
            cards.append(cand[0])
    return cards


def thread_status(entries):
    """单线程 → {authors, approves, merges}。approve 两个字段名都认。"""
    authors = {e.get("actor") for e, _ in entries if e.get("kind") == "patch"}
    if not authors:
        authors = {e.get("actor") for e, _ in entries if e.get("kind") == "issue"}
    approves, merges = [], []
    for e, oid in sorted(entries, key=lambda x: x[0].get("ts", 0)):
        body = e.get("body") or {}
        if e.get("kind") == "review":
            if "decision" in body:
                fld, val = "decision", body.get("decision")
            elif "verdict" in body:
                fld, val = "verdict", body.get("verdict")
            else:
                fld, val = None, None
            if val == "approve" and e.get("actor") not in authors:
                approves.append({"actor": e.get("actor"), "field": fld,
                                 "oid": oid, "tier": body.get("tier"),
                                 "window_required": body.get("window_required")})
        elif e.get("kind") == "merge_result":
            merged = body.get("merged")
            if merged is True or merged == "true":
                merges.append({"actor": e.get("actor"), "oid": body.get("oid") or oid,
                               "tier": body.get("tier")})
    return {"authors": authors, "approves": approves, "merges": merges}


def classify(row, threads):
    """单行判定 → dict(kind, cards, status, evidence)。"""
    cards = resolve_cards(row, set(threads))
    if not cards:
        return {"kind": "NO_CARD", "cards": [], "status": [],
                "evidence": "行内无交付卡号（旧轮次 / legacy）"}
    per, missing = [], []
    for c in cards:
        ents = threads.get(c)
        if not ents:
            missing.append(c)
            continue
        st = thread_status(ents)
        pin = PIN_EVIDENCE.get(row["tid"])
        if pin:
            aps, mps = pin
            st = dict(st)
            st["approves"] = [a for a in st["approves"]
                              if any(a["oid"].startswith(p) for p in aps)]
            st["merges"] = [m for m in st["merges"]
                            if any(str(m["oid"]).startswith(p) for p in mps)]
        per.append({"card": c, **st})
    if missing:
        return {"kind": "NO_THREAD", "cards": cards, "status": per,
                "evidence": "卡在 refs/collab 里无线程：%s" % ", ".join(missing)}
    ok = all(p["approves"] and p["merges"] for p in per)
    if ok:
        return {"kind": "APPROVED_MERGED", "cards": cards, "status": per,
                "evidence": "每个卡都有非作者 approve 且 merged"}
    if any(p["approves"] for p in per):
        return {"kind": "APPROVED_UNMERGED", "cards": cards, "status": per,
                "evidence": "有 approve 但缺 merged（或并非每个卡都有）"}
    return {"kind": "UNREVIEWED", "cards": cards, "status": per,
            "evidence": "线程里没有非作者 approve"}


def read_threads(repo):
    """读 refs/collab/inbox/* 的 entry blob → {card_id: [(entry, blob_oid)]}。"""
    try:
        refs = subprocess.run(
            ["git", "-C", repo, "for-each-ref", "--format=%(refname)",
             "refs/collab/inbox/"], capture_output=True, text=True, check=True).stdout.split()
    except (OSError, subprocess.CalledProcessError) as exc:
        raise AuditError("读不到 git refs：%s" % exc)
    if not refs:
        raise AuditError("refs/collab/inbox/* 为空；先 "
                         "`git fetch walgit '+refs/collab/inbox/*:refs/collab/inbox/*'`")
    inp = ("\n".join(refs) + "\n").encode()
    raw = subprocess.run(["git", "-C", repo, "cat-file", "--batch"],
                         input=inp, capture_output=True, check=True).stdout
    byid, pos = {}, 0
    while pos < len(raw):
        nl = raw.find(b"\n", pos)
        if nl < 0:
            break
        hdr = raw[pos:nl]
        pos = nl + 1
        parts = hdr.split()
        if len(parts) != 3:
            continue
        oid, typ, size = parts[0].decode(), parts[1].decode(), int(parts[2])
        content = raw[pos:pos + size]
        pos += size + 1
        if typ != "blob":
            continue
        try:
            e = json.loads(content.decode("utf-8"))
        except Exception:
            continue
        if not isinstance(e, dict) or not e.get("id"):
            continue
        byid.setdefault(e["id"], []).append((e, oid))
    return byid


def fmt_approve(p):
    if not p["approves"]:
        return "—"
    parts = []
    for a in p["approves"]:
        s = "%s `%s`（%s）" % (a["actor"], a["oid"][:8], a["field"] or "?")
        if a.get("tier"):
            s += " tier=%s" % a["tier"]
        if a.get("window_required") is True:
            s += " window_required"
        parts.append(s)
    return "<br>".join(parts)


def fmt_merge(p):
    if not p["merges"]:
        return "—"
    return "<br>".join("`%s`" % m["oid"] for m in p["merges"])


def verdict_text(kind):
    return {
        "APPROVED_MERGED": "已审已合 ⇒ 改成已审已合",
        "APPROVED_UNMERGED": "有 approve 无合并 ⇒ 保持原样",
        "UNREVIEWED": "无独立 approve ⇒ 保持原样",
        "NO_THREAD": "卡无对应线程 ⇒ 保持原样（证据不足）",
        "NO_CARD": "无交付卡号 ⇒ 保持原样（证据不足）",
    }[kind]


def build_census(rows, threads):
    out = []
    for row in rows:
        out.append({"line": row["line"], "tid": row["tid"], "text": row["text"],
                    "target": row["target"], "result": classify(row, threads)})
    return out


def render_markdown(census):
    counts = {}
    lines = ["| 行 | T 号 | 交付卡 | 独立 approve（字段 + 条目 oid） | merge oid | 当前文字 | 判定 |",
             "|---:|---|---|---|---|---|---|"]
    for item in census:
        res = item["result"]
        counts[res["kind"]] = counts.get(res["kind"], 0) + 1
        tids = item["tid"] or "（非 T 行）"
        cards = "<br>".join(res["cards"]) if res["cards"] else "—"
        appr = "<br>".join(fmt_approve(p) for p in res["status"]) if res["status"] else "—"
        merg = "<br>".join(fmt_merge(p) for p in res["status"]) if res["status"] else "—"
        cur = "待独立 principal 复核" if item["target"] == PHRASE else PROSE_PHRASE
        lines.append("| %d | %s | %s | %s | %s | %s | %s |"
                     % (item["line"], tids, cards, appr, merg, cur, verdict_text(res["kind"])))
    return "\n".join(lines), counts


def read_whitelist(repo, path, at):
    """取 whitelist 文本：默认读工作树文件；`--at REV` 从 git 读快照（变更前后都可复跑）。"""
    if not at:
        with open(path, encoding="utf-8") as f:
            return f.read()
    try:
        return subprocess.run(
            ["git", "-C", repo, "show", "%s:mir2-rs/tests/parity/whitelist.md" % at],
            capture_output=True, text=True, check=True).stdout
    except subprocess.CalledProcessError as exc:
        raise AuditError("取不到 %s 的 whitelist：%s" % (at, exc))


def cmd_census(args):
    repo = os.path.abspath(args.repo)
    wl = args.whitelist or os.path.join(repo, "mir2-rs", "tests", "parity", "whitelist.md")
    try:
        text = read_whitelist(repo, wl, args.at)
        rows = parse_rows(text)
        threads = read_threads(repo)
    except (OSError, AuditError) as exc:
        print("NEED: %s" % exc, file=sys.stderr)
        return 2
    if not rows:
        print("NEED: whitelist 里没有目标字样（口径漂移）", file=sys.stderr)
        return 2
    census = build_census(rows, threads)
    table, counts = render_markdown(census)
    print(table)
    print()
    print("合计 %d 行：%s" % (len(census), "，".join(
        "%s=%d" % (k, counts[k]) for k in sorted(counts))))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"repo": repo, "whitelist": wl, "counts": counts,
                       "rows": census}, f, ensure_ascii=False, indent=2)
            f.write("\n")
    return 0


# ---------------------------------------------------------------- selftest

def _mk(kind, actor, body, ts):
    return ({"kind": kind, "actor": actor, "body": body, "ts": ts}, "0" * 40)


def _selftest():
    ok = 0
    fail = []

    def check(name, cond):
        nonlocal ok
        if cond:
            ok += 1
        else:
            fail.append(name)

    wl = (
        "| T-1 | x | y | z | 作者登记；**待独立 principal 复核** |\n"
        "| T-2 | x | y | z | 作者登记；**待独立 principal 复核** |\n"
        "| T-3 | x | y | z | 作者登记；**待独立 principal 复核** |\n"
        "| T-4 | x | y | z | 作者登记；**待独立 principal 复核** |\n"
        "> ④ **使用药品**（`useitem.rs`，待审）；⑤ x\n"
        "| D-1 | 不在普查域（无目标字样） |\n"
    )
    rows = parse_rows(wl)
    check("parse 4 T rows + 1 prose", len(rows) == 5)
    check("prose tid", rows[-1]["tid"] == "PROSE-useitem")

    threads = {
        "openmir2-card-a": [
            _mk("patch", "openmir2-worker-1", {}, 1),
            _mk("review", "openmir2-reviewer-1", {"verdict": "approve"}, 2),
            _mk("merge_result", "openmir2-orchestrator", {"merged": True, "oid": "abcdef123"}, 3),
        ],
        "openmir2-card-b": [
            _mk("patch", "openmir2-worker-1", {}, 1),
            _mk("review", "openmir2-reviewer-2", {"decision": "approve"}, 2),
            _mk("merge_result", "openmir2-orchestrator", {"merged": True, "oid": "123456789"}, 3),
        ],
        "openmir2-card-c": [
            _mk("patch", "openmir2-worker-1", {}, 1),
            _mk("review", "openmir2-worker-1", {"verdict": "approve"}, 2),
            _mk("merge_result", "openmir2-orchestrator", {"merged": True, "oid": "x"}, 3),
        ],
        "openmir2-card-d": [
            _mk("patch", "openmir2-worker-1", {}, 1),
            _mk("review", "openmir2-reviewer-3", {"verdict": "request_changes"}, 2),
            _mk("merge_result", "openmir2-orchestrator", {"merged": True, "oid": "y"}, 3),
        ],
    }
    row_a = {"tid": "T-1", "text": "| T-1 | 卡 `openmir2-card-a` |", "target": PHRASE}
    row_b = {"tid": "T-2", "text": "| T-2 | 卡 `openmir2-card-b` |", "target": PHRASE}
    row_c = {"tid": "T-3", "text": "| T-3 | 卡 `openmir2-card-c` |", "target": PHRASE}
    row_d = {"tid": "T-4", "text": "| T-4 | 卡 `openmir2-card-d` |", "target": PHRASE}
    check("verdict approve 被认（早期字段）",
          classify(row_a, threads)["kind"] == "APPROVED_MERGED")
    check("decision approve 被认（近期字段）",
          classify(row_b, threads)["kind"] == "APPROVED_MERGED")
    check("作者自审不算独立 approve",
          classify(row_c, threads)["kind"] == "UNREVIEWED")
    check("request_changes 不是 approve",
          classify(row_d, threads)["kind"] == "UNREVIEWED")

    check("无卡 ⇒ 证据不足",
          classify({"tid": "T-9", "text": "legacy 行", "target": PHRASE}, threads)["kind"] == "NO_CARD")
    check("卡无线程 ⇒ 证据不足",
          classify({"tid": "T-9", "text": "| T-9 | 卡 `openmir2-nope` |", "target": PHRASE},
                   threads)["kind"] == "NO_THREAD")

    ids = {"openmir2-card-a"}
    r1 = {"tid": "T-1", "text": "卡 `openmir2-card-a` 与 `openmir2-worker-1`", "target": PHRASE}
    check("principal 名不当作卡号", resolve_cards(r1, ids) == ["openmir2-card-a"])
    r2 = {"tid": "T-2", "text": "卡 card-a", "target": PHRASE}
    check("唯一后缀 shorthand 解析", resolve_cards(r2, ids) == ["openmir2-card-a"])
    amb = {"openmir2-c-x", "openmir2-d-x"}
    r3 = {"tid": "T-3", "text": "卡 x", "target": PHRASE}
    check("歧义 shorthand 不猜", resolve_cards(r3, amb) == [])
    r4 = {"tid": "T-50", "text": "卡 `openmir2-card-a`", "target": PHRASE}
    check("hint 覆盖行内承接卡",
          resolve_cards(r4, ids) == ["openmir2-m3-drop-exp-fixture-botsrv"])

    t2 = {"openmir2-card-a": [
        _mk("patch", "openmir2-worker-1", {}, 1),
        _mk("review", "openmir2-reviewer-1", {"decision": "approve"}, 2),
    ]}
    check("approve 无 merge ⇒ APPROVED_UNMERGED",
          classify({"tid": "T-1", "text": "卡 `openmir2-card-a`", "target": PHRASE}, t2)["kind"]
          == "APPROVED_UNMERGED")

    check("空 whitelist 无行", parse_rows("| D-1 | x |\n") == [])

    if fail:
        print("SELFTEST FAIL %d/%d: %s" % (len(fail), ok + len(fail), ", ".join(fail)),
              file=sys.stderr)
        return 2
    print("SELFTEST PASS %d/%d" % (ok, ok))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    sub = ap.add_subparsers(dest="cmd")
    p = sub.add_parser("census", help="只读普查并打印 markdown 表")
    p.add_argument("--repo", default=os.path.dirname(os.path.dirname(os.path.dirname(
        os.path.abspath(__file__)))))
    p.add_argument("--whitelist", default=None)
    p.add_argument("--json", default=None)
    p.add_argument("--at", default=None,
                   help="从该 git rev 读 whitelist 快照（例如普查改写前的 base）")
    sub.add_parser("selftest", help="合成夹具自测（不需要 git/collab）")
    args = ap.parse_args(argv)
    if args.cmd == "census":
        return cmd_census(args)
    if args.cmd == "selftest":
        return _selftest()
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
