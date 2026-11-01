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

**行 → 卡的三种证据（由强到弱）**：
  1. 行内 `卡 openmir2-…`（反引号包住）——直接可用；但**行内引用可能是「承接卡」**，由
     `CARD_HINTS` 显式覆盖并附依据（每条 hint 必须给 commit/线程 oid）。
  2. **git 溯源（review d9fda1af F1）**：对行内无卡号的 T 行，按 master first-parent 的
     **merge 窗口**（上一 merge .. 本 merge）逐窗口 `git diff` 找 `+| T-N |` 行：
       * 该行必须是**实质变更**（不是只改「谁审的」列；后者是 `whitelist-card-link-debt`
         给所有 legacy 行追加标记的动作，不能算「本行被审」）；
       * 该行的**首列内容前 60 字符**必须与当前行一致（防止 T 号复用/重编号把别的行算进来）；
       * 行内仍含「待独立 principal 复核」（合并当时确实还是待审状态）；
       * 窗口所属卡有**非作者 approve** 且该 merge 是快照 base 的祖先。
     满足 ⇒ 已审已合；窗口加了该行但卡**没有**独立 approve ⇒ 已合未审（保持原样）。
  3. 以上都失败 ⇒ `行内无卡号且 git 溯源未闭合`，保持原样。

**证据口径**：
  * 线程：`refs/collab/inbox/*` 的 blob（`walgit collab thread` 的同一份数据）。
  * 写者：`kind=patch` 的 `actor` 集合（无 patch 时退回 `issue` 的 actor）。
  * 独立 approve：`kind=review` 且 `body.decision|body.verdict == "approve"` 且 actor **不在**写者集合。
  * 合并：`kind=merge_result` 且 `body.merged` 为真；`body.oid` 用 `git cat-file --batch-check`
    批量解析成 commit（老条目是 9 位短 oid）。

用法：
  python -I -B tools/whitelist/wl_status_audit.py census [--repo .] [--whitelist PATH]
         [--at REV] [--json OUT]
  python -I -B tools/whitelist/wl_status_audit.py selftest
退出码：census 0=产表成功 / 2=判不了（缺 collab refs、行数漂移等）；selftest 0=全过 / 2=有失败。
"""
import argparse
import bisect
import json
import os
import re
import subprocess
import sys

PHRASE = "待独立 principal 复核"
PROSE_PHRASE = "（`useitem.rs`，待审）"
WL_REL = "mir2-rs/tests/parity/whitelist.md"
ROW_RE = re.compile(r"^\|\s*(T-\d+)\s*\|")
CANON_RE = re.compile(r"卡\s*`(openmir2-[a-z0-9][a-z0-9-]*)`")
BARE_RE = re.compile(r"卡\s+([A-Za-z0-9#][A-Za-z0-9# _/\-]*)")
PRINCIPAL_RE = re.compile(
    r"^openmir2-(?:worker|svc|dev|reviewer|coordinator|orchestrator|leader)(?:-\d+)?$")
OID40_RE = re.compile(r"^[0-9a-f]{40}$")

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

#: 某些卡在生命周期里有多次 review/merge（不同分片），但**本行**只由其中一次覆盖。
#: 这里把普查表钉到「覆盖本行的那一条」，避免把别的分片的 approve 算到本行头上。
PIN_EVIDENCE = {
    "T-50": (["8198aec1"], ["30c52f85"]),
    "T-57": (["8fcd4392"], ["07dd178c"]),
    "T-58": (["8fcd4392"], ["07dd178c"]),
    "PROSE-useitem": (["f069955a"], ["60095f65"]),
}


class AuditError(Exception):
    """判不了（不是「证伪」）：缺 collab refs / 行数漂移 / 形状不对。rc=2。"""


# ---------------------------------------------------------------- 纯函数（可 selftest）

def first_cell(line):
    """markdown 行的第 2 列（「差异/豁免点」）；取不到返回空串。"""
    parts = line.split("|")
    return parts[2].strip() if len(parts) >= 3 else ""


def substantive_part(line):
    """去掉「编号」与「谁审的」两列后的实质内容。"""
    parts = line.strip().strip("|").split("|")
    return "|".join(c.strip() for c in parts[1:-1])


def is_substantive(removed_lines, added_line):
    """该 `+` 行是不是**实质变更**（不是只改最后一列）。

    反例：`openmir2-whitelist-card-link-debt` 给 100 个 legacy 行追加「**legacy**：…」
    ——去掉首尾列后与 `-` 行完全相同 ⇒ 不算本行被审。新行（无 `-` 行）⇒ 实质性成立。
    """
    if not removed_lines:
        return True
    return any(substantive_part(r) != substantive_part(added_line) for r in removed_lines)


def content_matches(cur_cell, added_cell, n=60):
    """当前行与窗口内 `+` 行的首列前 n 字符一致 ⇒ 是同一行（防 T 号复用/重编号）。"""
    if not cur_cell or not added_cell:
        return False
    k = min(n, len(cur_cell), len(added_cell))
    return k > 0 and cur_cell[:k] == added_cell[:k]


def choose_evidence(candidates, cur_cell, phrase, ancestor_ok, status_of):
    """候选窗口（**按时间从旧到新**）→ (kind, evidence) 或 (None, {})。

    kind: `APPROVED_MERGED` / `MERGED_NO_REVIEW`。优先取「有非作者 approve」的最早窗口；
    没有 approve 时保留最早的「已合」窗口（供分类为已合未审）。
    """
    unapproved = None
    for cand in candidates:
        card, merge, added = cand["card"], cand["merge"], cand["added"]
        if phrase not in added:
            continue
        if not content_matches(cur_cell, first_cell(added)):
            continue
        if not ancestor_ok(merge):
            continue
        approves = status_of(card).get("approves") or []
        if approves:
            return "APPROVED_MERGED", {"path": "window", "card": card, "merge": merge,
                                       "approve": approves[0], "added": added}
        if unapproved is None:
            unapproved = {"path": "window", "card": card, "merge": merge,
                          "approve": None, "added": added}
    if unapproved:
        return "MERGED_NO_REVIEW", unapproved
    return None, {}


# ---------------------------------------------------------------- git 溯源

class Trace:
    """按 first-parent merge 窗口反解「行 → 交付卡 → approve/merge」。只读。"""

    def __init__(self, repo, base_rev, threads):
        self.repo = repo
        self.base = base_rev
        self.threads = threads
        self.cands = {}      # tid -> [{card, merge, added}]
        self._built = False

    def _git(self, *args, text=True):
        return subprocess.run(["git", "-C", self.repo, *args],
                              capture_output=True, text=True, check=True).stdout

    def build(self):
        if self._built:
            return
        merges = []
        for card, ents in self.threads.items():
            for e, _oid in ents:
                body = e.get("body") or {}
                if e.get("kind") == "merge_result" and \
                        (body.get("merged") is True or body.get("merged") == "true") and body.get("oid"):
                    merges.append((card, str(body["oid"])))
        if not merges:
            self.cands = {}
            self._built = True
            return
        inp = "\n".join(o + "^{commit}" for _, o in merges) + "\n"
        out = subprocess.run(["git", "-C", self.repo, "cat-file", "--batch-check"],
                             input=inp, capture_output=True, text=True).stdout.split("\n")
        commit_card = {}
        for (card, _oid), line in zip(merges, out):
            p = line.split()
            if len(p) >= 2 and p[1] == "commit":
                commit_card[p[0]] = card
        chain = self._git("rev-list", "--first-parent", self.base).split()
        pos_of = {c: i for i, c in enumerate(chain)}
        mcommit = {c: card for c, card in commit_card.items() if c in pos_of}
        msorted = sorted(mcommit.items(), key=lambda kv: pos_of[kv[0]])  # 新→旧
        self.chain, self.pos_of, self.msorted = chain, pos_of, msorted
        self.mpos = [pos_of[c] for c, _ in msorted]
        self.merges_by_card = {}
        for c, card in commit_card.items():
            self.merges_by_card.setdefault(card, []).append(c)
        self.head2card = {}
        for card, ents in self.threads.items():
            for e, _oid in ents:
                refs = e.get("refs") or {}
                if e.get("kind") == "patch" and OID40_RE.match(refs.get("head", "")):
                    self.head2card[refs["head"]] = card
        windows, prev = [], None
        for commit, card in reversed(msorted):                           # 旧→新
            windows.append((prev, commit, card))
            prev = commit
        self.cands = {}
        for prev, cur, card in windows:
            a = prev if prev else cur + "^"
            try:
                diff = self._git("diff", "--unified=0", a, cur, "--", WL_REL)
            except subprocess.CalledProcessError:
                continue
            removed, added = {}, []
            for ln in diff.split("\n"):
                if ln.startswith("-| "):
                    m = ROW_RE.match(ln[1:])
                    if m:
                        removed.setdefault(m.group(1), []).append(ln[1:])
                elif ln.startswith("+| "):
                    m = ROW_RE.match(ln[1:])
                    if m:
                        added.append((m.group(1), ln[1:]))
            for tid, aline in added:
                if is_substantive(removed.get(tid, []), aline):
                    self.cands.setdefault(tid, []).append(
                        {"card": card, "merge": cur, "added": aline})
        self._row_cache = {}
        self._built = True

    def _delivery_card(self, commit):
        """commit 所在 first-parent merge 窗口（新→旧序号）的交付卡。"""
        if commit in self.pos_of:
            i = bisect.bisect_right(self.mpos, self.pos_of[commit]) - 1
            return self.msorted[i][1] if i >= 0 else None
        lo, hi, ans = 0, len(self.msorted) - 1, None
        while lo <= hi:
            mid = (lo + hi) // 2
            if self._is_ancestor(commit, self.msorted[mid][0]):
                ans = mid
                lo = mid + 1
            else:
                hi = mid - 1
        return self.msorted[ans][1] if ans is not None else None

    def _intro_adds(self, tid):
        """`git log -S "| T-N |"` 里真正把该行 0→1 加进去的 commit（新→旧）。"""
        if not hasattr(self, "_intro_cache"):
            self._intro_cache = {}
        if tid in self._intro_cache:
            return self._intro_cache[tid]
        commits = self._git("log", "-S", "| %s |" % tid, "--format=%H", "--", WL_REL).split()
        adds = []
        for c in commits:
            try:
                at = self._git("show", "%s:%s" % (c, WL_REL))
                par = self._git("show", "%s^:%s" % (c, WL_REL))
            except subprocess.CalledProcessError:
                continue
            if ("\n| %s |" % tid) in ("\n" + at) and ("\n| %s |" % tid) not in ("\n" + par):
                adds.append(c)
        self._intro_cache[tid] = adds
        return adds

    def _row_at(self, commit, tid):
        key = (commit, tid)
        if key in self._row_cache:
            return self._row_cache[key]
        try:
            text = self._git("show", "%s:%s" % (commit, WL_REL))
        except subprocess.CalledProcessError:
            text = ""
        line = None
        for ln in text.split("\n"):
            if ln.startswith("| %s |" % tid):
                line = ln
                break
        self._row_cache[key] = line
        return line

    def _is_ancestor(self, a, b):
        return subprocess.run(["git", "-C", self.repo, "merge-base", "--is-ancestor",
                               a, b]).returncode == 0

    def _parents(self, commit):
        out = self._git("rev-list", "--parents", "-n", "1", commit).split()
        return out[1:] if out else []

    def merge_info(self, oid):
        """oid 是否解析、父数、类型（真实合并 / 账本值 / 未解析）。"""
        try:
            full = self._git("rev-parse", "--verify", "--quiet",
                             str(oid) + "^{commit}").strip()
        except subprocess.CalledProcessError:
            full = ""
        if not full:
            return {"oid": str(oid), "full": None, "resolved": False, "kind": "未解析"}
        ps = self._parents(full)
        return {"oid": str(oid), "full": full, "resolved": True,
                "kind": "真实合并" if len(ps) >= 2 else "账本值/单亲"}

    def _real_merge(self, intro):
        """把 intro 首次并入 first-parent 主线的 commit（可判定时返回）。

        在 first-parent 链上二分找**第一个包含 intro 的 commit**：side-branch 的 patch head
        被合并时，这条 commit 就是真正的 merge commit（例如 s1 的 1894ccd50）。
        """
        if not intro or not OID40_RE.match(intro) or not self.chain:
            return None
        lo, hi, ans = 0, len(self.chain) - 1, None
        while lo <= hi:                      # chain[0]=base（最新），向后是更旧
            mid = (lo + hi) // 2
            if self._is_ancestor(intro, self.chain[mid]):
                ans = mid                    # True 区在“更新”的一侧；最后一个 True = 引入 merge
                lo = mid + 1
            else:
                hi = mid - 1
        if ans is None:
            return None
        M = self.chain[ans]
        if M == intro:                       # intro 本身就在 first-parent 链上
            return M if len(self._parents(M)) >= 2 else None
        ps = self._parents(M)
        if len(ps) >= 2 and self._is_ancestor(intro, ps[1]):
            return M
        return None

    def _trace_intro(self, tid, current_line):
        """`git log -S` 引入 commit → patch head/delivery 卡 → merge（reviewer 口径）。"""
        cur_cell = first_cell(current_line)
        for c in self._intro_adds(tid):
            # 引入 commit 自身的那一行必须与当前行同内容（防 T 号复用/重编号）
            intro_line = self._row_at(c, tid)
            if intro_line is None or PHRASE not in intro_line:
                continue
            if not content_matches(cur_cell, first_cell(intro_line)):
                continue
            cards = []
            if c in self.head2card:
                cards.append(self.head2card[c])
            d = self._delivery_card(c)
            if d and d not in cards:
                cards.append(d)
            for card in cards:
                for merge in self.merges_by_card.get(card, []):
                    if not self._is_ancestor(merge, self.base):
                        continue
                    if not self._is_ancestor(c, merge):
                        continue
                    line = self._row_at(merge, tid)
                    if line is None or PHRASE not in line:
                        continue
                    if not content_matches(cur_cell, first_cell(line)):
                        continue
                    st = thread_status(self.threads.get(card) or [])
                    if st["approves"]:
                        return "APPROVED_MERGED", {
                            "path": "intro", "intro": c, "card": card,
                            "merge": merge, "real_merge": self._real_merge(c),
                            "approve": st["approves"][0]}
                    return "MERGED_NO_REVIEW", {
                        "path": "intro", "intro": c, "card": card,
                        "merge": merge, "real_merge": self._real_merge(c),
                        "approve": None}
        return None, {}

    def trace(self, tid, current_line):
        self.build()
        if not OID40_RE.match(self.base):
            return None, {}
        def ancestor_ok(commit):
            return subprocess.run(
                ["git", "-C", self.repo, "merge-base", "--is-ancestor", commit, self.base]
            ).returncode == 0
        def status_of(card):
            return thread_status(self.threads.get(card) or [])
        kind, ev = self._trace_intro(tid, current_line)
        if kind:
            return kind, ev
        return choose_evidence(self.cands.get(tid, []), first_cell(current_line),
                               PHRASE, ancestor_ok, status_of)


# ---------------------------------------------------------------- 行/线程解析

def parse_rows(text):
    rows = []
    for lineno, line in enumerate(text.split("\n"), 1):
        if PHRASE in line:
            m = ROW_RE.match(line)
            rows.append({"line": lineno, "tid": m.group(1) if m else None,
                         "text": line, "target": PHRASE})
        elif PROSE_PHRASE in line:
            rows.append({"line": lineno, "tid": "PROSE-useitem", "text": line,
                         "target": PROSE_PHRASE})
    return rows


def resolve_cards(row, thread_ids):
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
    for m in BARE_RE.finditer(text):
        name = m.group(1).strip().rstrip("/").strip()
        if not name or name.startswith("openmir2-"):
            continue
        cand = [t for t in thread_ids if t == name or t.endswith("-" + name)]
        if len(cand) == 1 and cand[0] not in cards:
            cards.append(cand[0])
    return cards


def thread_status(entries):
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
                approves.append({"actor": e.get("actor"), "field": fld, "oid": oid,
                                 "tier": body.get("tier"),
                                 "window_required": body.get("window_required")})
        elif e.get("kind") == "merge_result":
            merged = body.get("merged")
            if merged is True or merged == "true":
                merges.append({"actor": e.get("actor"), "oid": body.get("oid") or oid,
                               "tier": body.get("tier")})
    return {"authors": sorted(a for a in authors if a), "approves": approves, "merges": merges}


def read_threads(repo):
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


# ---------------------------------------------------------------- 分类/渲染

def classify(row, threads, trace):
    cards = resolve_cards(row, set(threads))
    if cards:
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
            if trace is not None:
                for m in st.get("merges", []):
                    mi = trace.merge_info(m["oid"])
                    m["kind"] = mi["kind"]
                    m["resolved"] = mi["resolved"]
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

    if row["tid"] and row["tid"] != "PROSE-useitem" and trace is not None:
        kind, ev = trace.trace(row["tid"], row["text"])
        if kind in ("APPROVED_MERGED", "MERGED_NO_REVIEW"):
            mi = trace.merge_info(ev["merge"])
            rm = ev.get("real_merge")
            merge_status = {"oid": ev["merge"], "full": mi.get("full"),
                            "kind": mi["kind"], "resolved": mi["resolved"],
                            "real_merge": rm, "path": ev.get("path")}
            if ev.get("path") == "intro":
                if rm and rm != mi.get("full"):
                    actual = "；实际 merge `%s`" % rm[:8]
                elif rm:
                    actual = ""
                else:
                    actual = "；引入 merge 未解析"
                evidence = ("引入 commit `%s` → 卡 `%s` → merge_result oid `%s`（账本值%s）"
                            % (ev.get("intro", "")[:8], ev["card"], ev["merge"][:8],
                               actual))
            else:
                evidence = ("merge 窗口 `%s` 加了本行（内容锚 + 实质变更 + 短语；%s）"
                            % (ev["merge"][:8], mi["kind"]))
            if kind == "MERGED_NO_REVIEW":
                evidence += "；该卡无线程 review"
            return {"kind": kind, "cards": [ev["card"]],
                    "status": [{"card": ev["card"],
                                "approves": [ev["approve"]] if ev.get("approve") else [],
                                "merges": [merge_status]}],
                    "evidence": evidence}
    if row["tid"] is None:
        return {"kind": "PROSE_UNCLOSED", "cards": [], "status": [],
                "evidence": "非 T 表行（引用块散文），无行级卡号"}
    return {"kind": "TRACE_UNCLOSED", "cards": [], "status": [],
            "evidence": "行内无卡号，git 溯源未闭合（窗口/内容锚/approve 任一未命中）"}


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
    parts = []
    for m in p["merges"]:
        oid, full = m["oid"], m.get("full")
        rm = m.get("real_merge")
        kind = m.get("kind", "未解析")
        if rm and full and rm != full:
            parts.append("实际 merge `%s`（引入）；账本 merge_result oid `%s`"
                         % (rm[:8], oid[:8]))
        elif rm and full and rm == full:
            parts.append("`%s`（真实合并，引入）" % oid[:8])
        elif kind == "真实合并":
            parts.append("`%s`（真实合并；引入 merge 未解析）" % oid[:8])
        elif m.get("path") == "intro":
            parts.append("`%s`（%s；引入 merge 未解析）" % (oid, kind))
        else:
            parts.append("`%s`（%s）" % (oid, kind))
    return "<br>".join(parts)


def verdict_text(kind):
    return {
        "APPROVED_MERGED": "已审已合 ⇒ 改写",
        "APPROVED_UNMERGED": "有 approve 无合并 ⇒ 保持原样",
        "UNREVIEWED": "无独立 approve ⇒ 保持原样",
        "MERGED_NO_REVIEW": "已合无独立 approve ⇒ 保持原样",
        "TRACE_UNCLOSED": "行内无卡号且 git 溯源未闭合 ⇒ 保持原样",
        "PROSE_UNCLOSED": "非 T 表行（引用块散文）⇒ 保持原样",
        "NO_THREAD": "卡无对应线程 ⇒ 保持原样（证据不足）",
        "NO_CARD": "无交付卡号 ⇒ 保持原样（证据不足）",
    }[kind]


def render_markdown(census):
    counts = {}
    lines = ["| 行 | T 号 | 交付卡 | 独立 approve（字段 + 条目 oid） | merge_result oid（类型） | 当前文字 | 判定 | 溯源 |",
             "|---:|---|---|---|---|---|---|---|"]
    for item in census:
        res = item["result"]
        counts[res["kind"]] = counts.get(res["kind"], 0) + 1
        tids = item["tid"] or "（非 T 行）"
        cards = "<br>".join(res["cards"]) if res["cards"] else "—"
        appr = "<br>".join(fmt_approve(p) for p in res["status"]) if res["status"] else "—"
        merg = "<br>".join(fmt_merge(p) for p in res["status"]) if res["status"] else "—"
        cur = "待独立 principal 复核" if item["target"] == PHRASE else PROSE_PHRASE
        lines.append("| %d | %s | %s | %s | %s | %s | %s | %s |"
                     % (item["line"], tids, cards, appr, merg, cur,
                        verdict_text(res["kind"]), res["evidence"]))
    return "\n".join(lines), counts


def read_whitelist(repo, path, at):
    if not at:
        with open(path, encoding="utf-8") as f:
            return f.read()
    try:
        return subprocess.run(
            ["git", "-C", repo, "show", "%s:%s" % (at, WL_REL)],
            capture_output=True, text=True, check=True).stdout
    except subprocess.CalledProcessError as exc:
        raise AuditError("取不到 %s 的 whitelist：%s" % (at, exc))


def cmd_census(args):
    repo = os.path.abspath(args.repo)
    wl = args.whitelist or os.path.join(repo, WL_REL)
    try:
        text = read_whitelist(repo, wl, args.at)
        rows = parse_rows(text)
        threads = read_threads(repo)
        base = subprocess.run(["git", "-C", repo, "rev-parse", args.at or "HEAD"],
                              capture_output=True, text=True, check=True).stdout.strip()
        trace = Trace(repo, base, threads)
    except (OSError, AuditError, subprocess.CalledProcessError) as exc:
        print("NEED: %s" % exc, file=sys.stderr)
        return 2
    if not rows:
        print("NEED: whitelist 里没有目标字样（口径漂移）", file=sys.stderr)
        return 2
    census = [{"line": r["line"], "tid": r["tid"], "text": r["text"],
               "target": r["target"], "result": classify(r, threads, trace)} for r in rows]
    table, counts = render_markdown(census)
    print(table)
    print()
    print("合计 %d 行：%s" % (len(census), "，".join(
        "%s=%d" % (k, counts[k]) for k in sorted(counts))))
    if args.json:
        with open(args.json, "w", encoding="utf-8", newline="\n") as f:
            json.dump({"repo": repo, "at": args.at, "counts": counts, "rows": census},
                      f, ensure_ascii=False, indent=2)
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
          classify(row_a, threads, None)["kind"] == "APPROVED_MERGED")
    check("decision approve 被认（近期字段）",
          classify(row_b, threads, None)["kind"] == "APPROVED_MERGED")
    check("作者自审不算独立 approve",
          classify(row_c, threads, None)["kind"] == "UNREVIEWED")
    check("request_changes 不是 approve",
          classify(row_d, threads, None)["kind"] == "UNREVIEWED")

    check("无卡 ⇒ 证据不足",
          classify({"tid": "T-9", "text": "legacy 行", "target": PHRASE}, threads, None)["kind"]
          == "TRACE_UNCLOSED")
    check("卡无线程 ⇒ 证据不足",
          classify({"tid": "T-9", "text": "| T-9 | 卡 `openmir2-nope` |", "target": PHRASE},
                   threads, None)["kind"] == "NO_THREAD")

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
          classify({"tid": "T-1", "text": "卡 `openmir2-card-a`", "target": PHRASE}, t2, None)["kind"]
          == "APPROVED_UNMERGED")
    check("空 whitelist 无行", parse_rows("| D-1 | x |\n") == [])

    # ---- git 溯源纯逻辑（review d9fda1af F1）----
    cur = "| T-9 | **Substantive title**：detail | why | 谁审的 |"
    good = "| T-9 | **Substantive title**：detail | why | 作者登记；**待独立 principal 复核** |"
    other = "| T-9 | **Different row title**：other detail | why | x |"
    legacy_only_old = "| T-9 | **Substantive title**：detail | why | 作者登记；**待独立 principal 复核** |"
    legacy_only_new = "| T-9 | **Substantive title**：detail | why | 作者登记；**待独立 principal 复核**；**legacy**：x |"
    check("实质变更：新增行成立", is_substantive([], good))
    check("实质变更：只改最后一列不成立",
          not is_substantive([legacy_only_old], legacy_only_new))
    check("内容锚：同首列成立",
          content_matches(first_cell(cur), first_cell(good)))
    check("内容锚：不同行（T 号复用）不成立",
          not content_matches(first_cell(cur), first_cell(other)))
    cands = [{"card": "openmir2-card-a", "merge": "m1", "added": good}]
    st = {"openmir2-card-a": {"approves": [{"actor": "r", "oid": "o", "field": "decision"}]}}
    kind, ev = choose_evidence(cands, first_cell(cur), PHRASE, lambda m: True, lambda c: st[c])
    check("溯源：内容+短语+approve ⇒ 已审已合", kind == "APPROVED_MERGED" and ev["card"] == "openmir2-card-a")
    kind2, _ = choose_evidence(cands, first_cell(cur), PHRASE, lambda m: True, lambda c: {"approves": []})
    check("溯源：无 approve ⇒ 已合未审", kind2 == "MERGED_NO_REVIEW")
    kind3, _ = choose_evidence(cands, first_cell(cur), PHRASE, lambda m: False, lambda c: st[c])
    check("溯源：merge 非 base 祖先 ⇒ 不认", kind3 is None)
    kind4, _ = choose_evidence([{"card": "c", "merge": "m", "added": other}],
                               first_cell(cur), PHRASE, lambda m: True, lambda c: st[c])
    check("溯源：内容锚不符 ⇒ 不认（防 T 号复用）", kind4 is None)
    kind5, _ = choose_evidence([], first_cell(cur), PHRASE, lambda m: True, lambda c: st[c])
    check("溯源：空候选 ⇒ 不认（退化形态）", kind5 is None)

    # 主路径 `Trace._trace_intro`（F4）：合成假 Trace，删掉/置空该分支即红。
    class _FakeTrace(Trace):
        def __init__(self):
            self.repo = "."
            self.base = "b" * 40
            self.threads = {"openmir2-card-t": [
                ({"kind": "patch", "actor": "w", "body": {}, "ts": 1}, "0" * 40),
                ({"kind": "review", "actor": "r", "body": {"decision": "approve"}, "ts": 2},
                 "0" * 40)]}
            self.cands = {}
            self.head2card = {"c" * 40: "openmir2-card-t"}
            self.merges_by_card = {"openmir2-card-t": ["d" * 40]}
            self.chain = ["d" * 40]
            self.pos_of = {"d" * 40: 0}
            self.msorted = [("d" * 40, "openmir2-card-t")]
            self.mpos = [0]
            self._row_cache = {}
            self._intro_cache = {}
            self._built = True

        def build(self):
            pass

        def _git(self, *args, text=True):
            if args[:2] == ("log", "-S"):
                return "c" * 40 + "\n"
            if args[0] == "show":
                spec = args[1]
                if spec.startswith("c" * 40 + "^:"):
                    return "\n"
                if spec.startswith("c" * 40 + ":") or spec.startswith("d" * 40 + ":"):
                    return ("| T-X | **Title**：detail | why | 待独立 principal 复核 |\n")
            return ""

        def _is_ancestor(self, a, b):
            return True

    ft = _FakeTrace()
    fk, fe = ft.trace("T-X", "| T-X | **Title**：detail | why | 待独立 principal 复核 |")
    check("主路径 _trace_intro 覆盖（置空该分支必红）",
          fk == "APPROVED_MERGED" and fe.get("card") == "openmir2-card-t"
          and fe.get("path") == "intro")

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
    p.add_argument("--at", default=None,
                   help="从该 git rev 读 whitelist 快照（例如普查改写前的 base）")
    p.add_argument("--json", default=None)
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
