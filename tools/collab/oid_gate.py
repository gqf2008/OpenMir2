#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""oid_gate — collab 条目里处于「commit 位置」的 oid 必须解析到真实对象（卡 `openmir2-collab-oid-resolvability-gate`）。

**为什么有这道门**：walgit 对条目 body/refs 里的 oid **零校验**。今晚同一类错误在五个入口各犯一次
（review 的 `head` 手补 31 位、patch 的 `head`、`--parent` 截成 9 位 ×2、`--base/--head` 短号），
**服务端照收、三视图全绿**；直到有人手动 `git cat-file` 才发现那条 head 指向空气。
评审与合并的全部可复算性都建立在「oid 指向真实对象」上 ⇒ 把这条做成机判。

**本工具只报不改**：历史条目是签名事实，发现错误只点名，绝不回写。

**四态**（判不了 ≠ 证伪）：
  * 解析不到 ⇒ **rc=1**（点名 principal / oid / 字段 / 线程）；
  * git 不可用 / 仓库读不了 ⇒ **rc=2**；
  * oid 解析到 **blob/tree**（不在 commit 位置的意义上）⇒ **跳过并计数**（info）；
  * 全部可解析 ⇒ rc=0。
  短 oid（7 位以上、能解析）**放行并记 info** —— 今晚出事的从来不是「写短号」，是「**手补长号**」。
  空串 / 缺字段 **判红**（空不等于短）——但只对**声称带 oid 的字段**（`refs.base/head`、
  `body.head/base/reviewed_head/oid/merge_oid`）；`parent: ""` 是 walgit 的「根条目」约定、
  `merged: ""`/`false` 是「未合并」，两者都跳过并计数。

**字段面**：`entry.refs.base` / `entry.refs.head`、`body.base` / `body.head` / `body.oid` /
`body.merged` / `body.merged.head` / `body.reviewed_head` / `body.merge_oid`（commit 位置）；
`entry.parent`（**条目 oid**：必须是存在的对象）。

**写侧**（`verify` 子命令，给签名脚本用）：输入必须是全 40 位小写 hex 且 `git rev-parse <oid>^{commit}`
能解析，否则 **rc=1 拒绝**——把「拒绝短号」从纪律变成工具行为。

与 [`entryrefs`](entryrefs.py) 互补：`entryrefs` 管「脚本**读**条目时取哪个字段」，本工具管
「**写**进去的值是否真实存在」。

用法：
  python -I tools/collab/oid_gate.py check [--limit N] [--json out.json]
  python -I tools/collab/oid_gate.py verify <oid> [...]        # 全 oid 才过，打印解析结果
  python -I tools/collab/oid_gate.py --selftest                # 合成夹具自证
"""
import argparse
import io
import json
import os
import re
import subprocess
import sys

FULL_OID_RE = re.compile(r"^[0-9a-f]{40}$")
#: 嵌套自测保护：`gate` 会调 `selfcheck()`，而「接线牙」又要在夹具里调 `main(["gate",…])`——
#: 不挡住会无限递归。嵌套时跳过夹具（**真仓库 check 仍照常跑**，那才是要验的东西）。
_IN_SELFTEST = False
DEFAULT_LIMIT = 200
COMMIT_FIELDS = ("base", "head", "oid", "merged", "merged.head", "reviewed_head", "merge_oid")


class GitUnavailable(Exception):
    """git 跑不起来 / 仓库读不了 ⇒ 判不了（rc=2），不是证伪。"""


def _clean_env():
    drop = {"GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE", "GIT_COMMON_DIR",
            "GIT_OBJECT_DIRECTORY", "GIT_ALTERNATE_OBJECT_DIRECTORIES"}
    return {k: v for k, v in os.environ.items() if k not in drop}


def _git(repo, args, git_cmd=("git",), timeout=120):
    try:
        p = subprocess.run(list(git_cmd) + ["-C", str(repo)] + list(args),
                           capture_output=True, text=True, timeout=timeout, env=_clean_env())
    except (OSError, subprocess.SubprocessError) as ex:
        raise GitUnavailable("git 跑不起来（%s）" % ex)
    # 128 = `fatal: Not a valid object name`（对象不存在，属于**解析不到**而不是 git 坏了）
    if p.returncode not in (0, 1, 128):
        raise GitUnavailable("git %s rc=%d：%s" % (" ".join(args), p.returncode,
                                                  (p.stderr or "").strip()[:160]))
    return p


def load_entries(repo, limit=DEFAULT_LIMIT, git_cmd=("git",), since=None):
    """读最近的条目（按 ts 倒序取 limit 条）。返回 (entries, 库内可读条目总数)。"""
    out = _git(repo, ["for-each-ref", "--format=%(refname) %(objectname)",
                      "refs/collab/inbox/"], git_cmd=git_cmd).stdout
    # 注意：ref 名末段是 **walgit 的 32 位条目 id**（不是 git 对象名；`cat-file --batch`
    # 不接受缩写，会报 missing）⇒ 必须用 `%(objectname)` 的 40 位 blob oid 去批量读。
    refs = [ln.split() for ln in out.splitlines() if ln.strip()]
    if not refs:
        return [], 0
    names = [r[0] for r in refs]
    oids = [r[1] for r in refs]
    payload = "".join(o + "\n" for o in oids).encode()
    try:
        p = subprocess.run(list(git_cmd) + ["-C", str(repo), "cat-file", "--batch"],
                           input=payload, capture_output=True, timeout=300, env=_clean_env())
    except (OSError, subprocess.SubprocessError) as ex:
        raise GitUnavailable("git cat-file --batch 失败（%s）" % ex)
    buf, pos, entries, seen = p.stdout, 0, [], 0
    while pos < len(buf):
        nl = buf.find(b"\n", pos)
        if nl < 0:
            break
        header = buf[pos:nl].decode("utf-8", "replace").split()
        pos = nl + 1
        if len(header) < 3:
            continue
        oid, typ, size = header[0], header[1], int(header[2])
        body = buf[pos:pos + size]
        pos += size + 1
        if typ != "blob":
            continue
        try:
            e = json.loads(body.decode("utf-8"))
        except ValueError:
            continue
        seen += 1
        idx = oids.index(oid)
        principal = names[idx].split("/")[3] if names[idx].count("/") >= 3 else "?"
        entries.append((principal, e.get("id", "?"), oid, e))
    entries.sort(key=lambda t: int(t[3].get("ts") or 0), reverse=True)
    return _window(entries, limit, since), seen


#: 这些字段**空了就是错**（一条 patch/review 声称有 base/head 却留空 ⇒ 无从对账）。
EMPTY_IS_RED = {"refs.base", "refs.head", "body.base", "body.head",
                "body.reviewed_head", "body.oid", "body.merge_oid"}
#: 这些字段空/假是**正常状态**（未合并 / 根条目无 parent），跳过不判。
EMPTY_IS_OK = {"body.merged", "body.merged.head", "parent"}


def _window(entries, limit, since):
    """扫描窗口：`--since` 给定时**绝不按 `--limit` 截断**（A4：窗口缩小也必须抓得到新坏条目）。

    内部先按 ts 倒序（与 `load_entries` 同序），保证 `limit` 截的是「最新的 N 条」。
    """
    entries = sorted(entries, key=lambda e: int(e[3].get("ts") or 0), reverse=True)
    if since is not None:
        return [e for e in entries if int(e[3].get("ts") or 0) >= int(since)]
    return entries[:limit]


def load_entry_ids(repo, git_cmd=("git",)):
    """所有条目的 **walgit 条目 id**（ref 名末段，32 位 hex；**不是 git 对象名**）。"""
    out = _git(repo, ["for-each-ref", "--format=%(refname)", "refs/collab/inbox/"],
               git_cmd=git_cmd).stdout
    return {ln.rsplit("/", 1)[-1] for ln in out.splitlines() if ln.strip()}


def load_baseline(path):
    """读 baseline 豁免清单（**逐条、可审计**）。

    缺失 / 读不了 / 空清单 ⇒ 抛 `GitUnavailable`（rc=2：**判不了**，不许静默全绿）；
    非法条目（通配 / 空值 / 缺字段）⇒ 抛 `ValueError`（rc=1：豁免必须带**具体** oid 与理由）。
    """
    if not path:
        raise GitUnavailable("未给 baseline（历史坏条目无处登记，判不了）")
    if not os.path.isfile(path):
        raise GitUnavailable("baseline 文件缺失：%s" % path)
    try:
        with io.open(path, encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError) as ex:
        raise GitUnavailable("baseline 读不了（%s）" % ex)
    entries = data.get("entries")
    if not isinstance(entries, list) or not entries:
        raise GitUnavailable("baseline 为空（不知道豁免什么 ⇒ 等于没校验）：%s" % path)
    out = []
    for e in entries:
        if not isinstance(e, dict):
            raise ValueError("baseline 条目不是对象：%r" % (e,))
        missing = [k for k in ("principal", "thread", "entry_oid", "field", "value", "reason")
                   if not e.get(k)]
        if missing:
            raise ValueError("baseline 条目缺字段 %s（豁免必须逐条带 principal/thread/entry_oid/"
                             "field/value/reason）" % missing)
        if not re.match(r"^[0-9a-f]{40}$", str(e["entry_oid"])):
            raise ValueError("baseline 的 entry_oid 必须是 40 位 blob oid（%r）" % (e["entry_oid"],))
        v = str(e["value"])
        if "*" in v or "…" in v or "..." in v:
            raise ValueError("baseline 不许通配（%s）" % v)
        if not re.match(r"^[0-9a-fA-F]{7,40}$", v):
            raise ValueError("baseline 的 value 不是 hex oid（%r）" % v)
        out.append((e["principal"], e["thread"], str(e["entry_oid"]), e["field"], v))
    return out


def oid_fields(entry):
    """条目里处于 commit 位置的 oid，+ 每条目一次 parent（条目 oid）。

    返回值里的值可能是**自由文本**（编排常常写 `"base": "master `cf5f2c630`"`）⇒
    真正取 oid 的动作在 `check_entries` 里做（用 `[0-9a-f]{7,40}` 抠出来逐个解析）。
    """
    out = []
    refs = entry.get("refs") or {}
    body = entry.get("body") or {}
    if isinstance(refs, dict):
        for k in ("base", "head"):
            if k in refs:
                out.append(("refs." + k, refs[k], "commit"))
    for k in COMMIT_FIELDS:
        cur, ok = body, True
        for part in k.split("."):
            if isinstance(cur, dict) and part in cur:
                cur = cur[part]
            else:
                ok = False
                break
        if ok and cur is not None:
            out.append(("body." + k, cur, "commit"))
    if "parent" in entry:
        out.append(("parent", entry["parent"], "entry"))
    return out


def _resolve(repo, oid, want_commit, git_cmd):
    """返回 'commit' / 'other' / None（解析不到）；git 不可用 ⇒ 抛 GitUnavailable。"""
    if not isinstance(oid, str) or not oid.strip():
        return None
    spec = oid.strip()
    p = _git(repo, ["cat-file", "-t", spec + "^{commit}" if want_commit else spec],
             git_cmd=git_cmd)
    if p.returncode == 0:
        return "commit"
    q = _git(repo, ["cat-file", "-t", spec], git_cmd=git_cmd)
    return "other" if q.returncode == 0 else None


def check_entries(entries, repo, git_cmd=("git",), entry_ids=None, baseline=None):
    """四态判定。返回 (rc, detail, stats)。"""
    stats = {"entries": len(entries), "oids": 0, "ok": 0, "short": 0,
             "non_commit": 0, "bad": 0, "empty": 0, "exempted": 0}
    bad, short_hits, skipped, exempt = [], [], [], []
    baseline = set(baseline or [])
    used_baseline = set()

    def _flag(principal, thread, entry_oid, field, value, message):
        """判红前先看这条是不是**逐条登记过**的历史豁免（A1：不许一刀切忽略 parent）。

        **键必须含 `thread` 与 `entry_oid`**（reviewer-21 实测：只按 (principal,field,value)
        豁免时，**新卡复用旧坏 parent 也被豁免** ⇒ 豁免外溢）。所以豁免精确到「这条条目」。
        """
        key = (principal, thread, entry_oid, field, value)
        if key in baseline:
            stats["exempted"] += 1
            used_baseline.add(key)
            exempt.append("豁免 %s / %s / 条目 %s / %s=%s"
                          % (principal, thread, entry_oid[:9], field, value))
        else:
            bad.append(message)
    oid_re = re.compile(r"[0-9a-f]{7,40}")
    try:
        for principal, thread, eoid, entry in entries:
            for field, value, kind in oid_fields(entry):
                text = value if isinstance(value, str) else ""
                if not text.strip():
                    if field in EMPTY_IS_OK:
                        skipped.append("%s 空（未合并/根条目，正常）" % field)
                    elif field in EMPTY_IS_RED:
                        stats["oids"] += 1
                        stats["empty"] += 1
                        _flag(principal, thread, eoid, field, "",
                              "空值：%s / %s / %s / 条目 %s ⇒ 空不等于短，判红"
                              % (principal, thread, field, str(eoid)[:9]))
                    continue
                cands = oid_re.findall(text)
                if not cands:
                    skipped.append("%s 不含 oid（自由文本：%r）" % (field, text[:40]))
                    continue
                for cand in cands:
                    stats["oids"] += 1
                    if kind == "entry":
                        # parent 指的是**条目本身**——它是 git **blob** 对象（thread 视图里的 oid 就是
                        # blob oid），所以判据是「对象存在」，而不是「是 commit」。
                        state = _resolve(repo, cand, False, git_cmd)
                    else:
                        state = _resolve(repo, cand, True, git_cmd)
                    if state is None:
                        stats["bad"] += 1
                        _flag(principal, thread, eoid, field, cand,
                              "解析不到：%s / %s / %s=%s（条 %s，原文 %r）"
                              % (principal, thread, field, cand, str(eoid)[:9], text[:48]))
                    elif state == "other":
                        stats["non_commit"] += 1
                        skipped.append("%s=%s（非 commit ⇒ 跳过计数）" % (field, cand))
                    else:
                        stats["ok"] += 1
                        if not FULL_OID_RE.match(cand):
                            stats["short"] += 1
                            short_hits.append("%s=%s" % (field, cand))
    except GitUnavailable as ex:
        return 2, "判不了：%s" % ex, stats
    if not entries:
        return 2, "扫描到 0 条条目 ⇒ 判不了（扫了 0 个不许算绿）", stats
    detail = ("扫描 %d 条条目 / %d 个 oid：可解析 %d（其中短 oid %d）、非 commit 跳过 %d、"
              "空值 %d、解析不到 %d（其中已豁免 %d）"
              % (stats["entries"], stats["oids"], stats["ok"], stats["short"],
                 stats["non_commit"], stats["empty"], stats["bad"],
                 len(exempt)))
    stats["exempted"] = len(exempt)
    stats["baseline_total"] = len(baseline)
    stats["baseline_unused"] = len(baseline - used_baseline)
    if exempt:
        detail = ("豁免 %d 处（逐条）：%s || " % (len(exempt), "；".join(exempt[:8]))) + detail
    if baseline:
        detail += "；baseline 共 %d 条（命中 %d，未命中 %d）" % (
            stats["baseline_total"], len(used_baseline), stats["baseline_unused"])
    if bad:
        return 1, "；".join(bad[:12]) + ("（共 %d 处）" % len(bad)) + " || " + detail, stats
    if short_hits:
        detail += "；info：短 oid %d 处（可解析即放行）：%s" % (
            len(short_hits), "、".join(short_hits[:6]))
    if skipped:
        detail += "；info：跳过 %d 处（根条目无 parent / 未合并 / 自由文本 / 非 commit）" % len(skipped)
    return 0, detail, stats


def real_check(repo, limit=DEFAULT_LIMIT, baseline_path=None, since=None, git_cmd=("git",)):
    """真仓库检查（`check` 与 `gate` 共用）。返回 (rc, detail, stats)。

    **这是合并门真正该调的那条路**：`--selftest` 只跑合成夹具，不等于这条已被调用（规则 36）。
    """
    try:
        entries, total = load_entries(repo, limit, git_cmd=git_cmd, since=since)
        ids = load_entry_ids(repo, git_cmd)
        baseline = load_baseline(baseline_path) if baseline_path else []
    except GitUnavailable as ex:
        return 2, "判不了：%s" % ex, {}
    except ValueError as ex:
        return 1, "baseline 非法：%s" % ex, {}
    rc, detail, stats = check_entries(entries, repo, entry_ids=ids, baseline=baseline)
    scope = ("ts >= %s" % since) if since is not None else ("最近 %d 条" % len(entries))
    detail = "（库内共 %d 条；范围 %s，实扫 %d 条）%s" % (total, scope, len(entries), detail)
    return rc, detail, stats


def verify_oids(repo, values, git_cmd=("git",)):
    """写侧：全 40 位小写 hex 且能解析到 commit；否则拒绝（rc=1）。"""
    problems, resolved = [], []
    for v in values:
        if not isinstance(v, str) or not FULL_OID_RE.match(v.strip()):
            problems.append("拒绝非全 oid：%r（要 40 位小写 hex）" % (v,))
            continue
        try:
            state = _resolve(repo, v.strip(), True, git_cmd)
        except GitUnavailable as ex:
            return 2, "判不了：%s" % ex, []
        if state != "commit":
            problems.append("拒绝解析不到 commit 的 oid：%s" % v.strip())
        else:
            resolved.append(v.strip())
    if problems:
        return 1, "；".join(problems), resolved
    return 0, "全部 oid 可解析（%d 个）" % len(resolved), resolved


def verify_parent(repo, values, git_cmd=("git",)):
    """写侧（父条目）：必须是非空 hex 且 `git cat-file -t` 解析得到对象（条目 id 是 32 位缩写）。"""
    problems = []
    for v in values:
        if not isinstance(v, str) or not v.strip():
            problems.append("拒绝空 parent：%r（根条目请显式写空并说明，不要塞占位）" % (v,))
            continue
        if not re.match(r"^[0-9a-f]{7,40}$", v.strip().lower()):
            problems.append("拒绝非 hex parent：%r" % (v,))
            continue
        try:
            state = _resolve(repo, v.strip(), False, git_cmd)
        except GitUnavailable as ex:
            return 2, "判不了：%s" % ex
        if state is None:
            problems.append("拒绝解析不到的 parent（不是存在的对象）：%s" % v.strip())
    if problems:
        return 1, "；".join(problems)
    return 0, "parent 全部可解析（%d 个）" % len(values)


def _baseline_count(path):
    try:
        with io.open(path, encoding="utf-8") as fh:
            data = json.load(fh)
        return len(data.get("entries") or [])
    except Exception:
        return 0

def audit_baseline(repo, path, git_cmd=("git",)):
    """逐条核 baseline 的 `entry_oid`（卡 `openmir2-oid-gate-baseline-audit` A1）。

    每条必须：`git cat-file -t <entry_oid>` **是 blob**；解析后 `actor == principal`、
    `id == thread`、`field` 的值等于登记的 `value`。

    返回 `(rc, red, undec)`：
      * `rc=0` 全部可核；`rc=1` 有**不一致**（逐条点名）；`rc=2` **判不了**
        （文件缺失/读不了/条目缺字段/blob 解析不了）。
    **手抄错 entry_oid 会让豁免静默失效**——本模式就是把那一点变成红。
    """
    if not path or not os.path.isfile(path):
        return 2, [], ["baseline 文件缺失/未给：%s" % path]
    try:
        with io.open(path, encoding="utf-8") as fh:
            data = json.load(fh)
    except (OSError, ValueError) as ex:
        return 2, [], ["baseline 读不了：%s" % ex]
    entries = data.get("entries")
    if not isinstance(entries, list) or not entries:
        return 2, [], ["baseline 为空：没有任何可核对象（不等于通过）"]
    red, undec, ok = [], [], 0
    for i, e in enumerate(entries):
        tag = "entries[%d]" % i
        if not isinstance(e, dict):
            undec.append("%s 不是对象（判不了）" % tag)
            continue
        # `value` 允许空串（条目字段本身为空）⇒ 只要求**键存在**；其余要求非空
        missing = [k for k in ("principal", "thread", "entry_oid", "field", "value", "reason")
                   if k not in e or (k != "value" and not e.get(k))]
        if missing:
            undec.append("%s 缺字段 %s（判不了）" % (tag, missing))
            continue
        oid = str(e["entry_oid"])
        if not re.match(r"^[0-9a-f]{40}$", oid):
            undec.append("%s entry_oid 不是 40 位 hex：%r（判不了）" % (tag, oid))
            continue
        p = _git(repo, ["cat-file", "-t", oid], git_cmd=git_cmd)
        if p.returncode != 0:
            red.append("%s entry_oid %s 解析不到（不是现有对象）" % (tag, oid))
            continue
        kind = (p.stdout or "").strip()
        if kind != "blob":
            red.append("%s entry_oid %s 是 %s，不是 blob" % (tag, oid, kind))
            continue
        q = _git(repo, ["cat-file", "-p", oid], git_cmd=git_cmd)
        try:
            raw = json.loads(q.stdout)
        except ValueError as ex:
            undec.append("%s blob 解析不了：%s" % (tag, ex))
            continue
        entry = raw.get("entry") if isinstance(raw, dict) and isinstance(raw.get("entry"), dict) else raw
        if not isinstance(entry, dict):
            undec.append("%s blob 不是条目对象（判不了）" % tag)
            continue
        problems = []
        if entry.get("actor") != e["principal"]:
            problems.append("principal 登记 %r ≠ 条目 actor %r" % (e["principal"], entry.get("actor")))
        if entry.get("id") != e["thread"]:
            problems.append("thread 登记 %r ≠ 条目 id %r" % (e["thread"], entry.get("id")))
        want_field = e["field"]
        want_value = str(e["value"]).strip().lower()
        have = None
        for name, val, _kind in oid_fields(entry):
            if name == want_field:
                have = str(val)
                break
        if have is None:
            problems.append("条目里没有字段 %s" % want_field)
        elif have.strip().lower() != want_value:
            problems.append("字段 %s 登记 %r ≠ 条目 %r" % (want_field, e["value"], have))
        if problems:
            red.append("%s（entry_oid %s）：%s" % (tag, oid, "；".join(problems)))
        else:
            ok += 1
    rc = 1 if red else (2 if undec else 0)
    return rc, red, undec


def _audit_teeth():
    """baseline 审计的五态牙（卡 `openmir2-oid-gate-baseline-audit` A2）。临时仓库，不碰真仓库。"""
    import shutil
    import tempfile
    ok = []

    def chk(cond, name):
        ok.append((name, bool(cond)))

    tmp = tempfile.mkdtemp(prefix="oid-audit-")
    env = {k: v for k, v in os.environ.items()
           if k not in ("GIT_DIR", "GIT_WORK_TREE", "GIT_INDEX_FILE")}
    try:
        subprocess.run(["git", "init", "-q", tmp], check=True, env=env)
        entry = {"version": 1, "kind": "status", "id": "card-a", "actor": "p",
                 "parent": "", "sig": "", "ts": 0, "body": {"x": 1}}
        blob = json.dumps(entry).encode()
        h = subprocess.run(["git", "-C", tmp, "hash-object", "-w", "--stdin"],
                           input=blob, capture_output=True, check=True, env=env).stdout.decode().strip()
        good = {"principal": "p", "thread": "card-a", "entry_oid": h, "field": "parent",
                "value": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "reason": "r"}

        def run_with(entries):
            path = os.path.join(tmp, "baseline.json")
            with io.open(path, "w", encoding="utf-8", newline="") as fh:
                fh.write(json.dumps({"format": "oid-gate-baseline/1", "entries": entries}))
            return audit_baseline(tmp, path)

        # ① 全 0 entry_oid ⇒ rc=1
        zero = dict(good, entry_oid="0" * 40)
        rc, red, undec = run_with([zero])
        chk(rc == 1 and red, "牙①：全 0 entry_oid ⇒ rc=1")
        # ② thread 改错 ⇒ rc=1
        rc, red, undec = run_with([dict(good, thread="card-OTHER")])
        chk(rc == 1 and any("thread" in r for r in red), "牙②：thread 改错 ⇒ rc=1")
        # ③ value 改错 ⇒ rc=1
        rc, red, undec = run_with([dict(good, value="cafebabecafebabecafebabecafebabecafebabe")])
        chk(rc == 1 and any("字段" in r for r in red), "牙③：value 改错 ⇒ rc=1")
        # ④ 完全正确 ⇒ rc=0（entry 的 parent 为空串，登记值也应为空串）
        exact = dict(good, value="")
        rc, red, undec = run_with([dict(exact, value="x")])
        # parent 为空 ⇒ 与登记 "x" 不符 ⇒ rc=1；正确形态用空值登记：
        ok_entry = dict(good, value="")
        rc2, red2, undec2 = run_with([ok_entry])
        chk(rc == 1, "牙④a：value 与条目不符（空 vs x）⇒ rc=1")
        chk(rc2 == 0 and not red2 and not undec2, "牙④b：完全正确 ⇒ rc=0")
        # N3：`field = refs.head` 也要能核（现有只覆盖 parent）
        entry_r = {"version": 1, "kind": "patch", "id": "card-r", "actor": "p",
                   "parent": "", "sig": "", "ts": 0,
                   "refs": {"base": "0" * 0 + "a" * 40, "head": "b" * 40}, "body": {}}
        h_r = subprocess.run(["git", "-C", tmp, "hash-object", "-w", "--stdin"],
                             input=json.dumps(entry_r).encode(), capture_output=True,
                             check=True, env=env).stdout.decode().strip()
        rc_r, red_r, _ = run_with([{"principal": "p", "thread": "card-r", "entry_oid": h_r,
                                    "field": "refs.head", "value": "b" * 40, "reason": "r"}])
        chk(rc_r == 0 and not red_r, "牙⑥：field=refs.head 绑定一致 ⇒ rc=0")
        rc_r2, red_r2, _ = run_with([{"principal": "p", "thread": "card-r", "entry_oid": h_r,
                                      "field": "refs.head", "value": "c" * 40, "reason": "r"}])
        chk(rc_r2 == 1 and red_r2, "牙⑥b：field=refs.head 值不符 ⇒ rc=1")

        # C1：**不带 `--baseline`** 时，`check --audit-baseline` 的 rc 必须等于审计 rc
        #（旧实现会把 bpath 丢掉 ⇒ 真仓库 check 按「已豁免 0」全红、覆盖审计返回码）
        bad_entry = {"version": 1, "kind": "status", "id": "card-a", "actor": "p",
                     "parent": "deadbeefdeadbeefdeadbeefdeadbeefdeadbeef", "sig": "", "ts": 99,
                     "body": {}}
        h_bad = subprocess.run(["git", "-C", tmp, "hash-object", "-w", "--stdin"],
                               input=json.dumps(bad_entry).encode(), capture_output=True,
                               check=True, env=env).stdout.decode().strip()
        subprocess.run(["git", "-C", tmp, "update-ref", "refs/collab/inbox/p/card-a-1", h_bad],
                       check=True, env=env)
        os.makedirs(os.path.join(tmp, "tools", "collab"))
        with io.open(os.path.join(tmp, "tools", "collab", "oid_gate_baseline.json"),
                     "w", encoding="utf-8", newline="") as fh:
            fh.write(json.dumps({"format": "oid-gate-baseline/1",
                                 "entries": [dict(good, entry_oid=h_bad)]}))
        rc_c1 = main(["check", "--audit-baseline", "--repo", tmp])
        chk(rc_c1 == 0, "C1 牙：不带 --baseline 时 check --audit-baseline 仍 = 审计 rc（0）")

        # ⑤ baseline 缺失 ⇒ rc=2
        rc3, red3, undec3 = audit_baseline(tmp, os.path.join(tmp, "no-such.json"))
        chk(rc3 == 2, "牙⑤：baseline 文件缺失 ⇒ rc=2")
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    return ok

def main(argv=None):
    ap = argparse.ArgumentParser(description="collab 条目 oid 可解析性门（只读，只报不改）")
    ap.add_argument("cmd", nargs="?", choices=("check", "gate", "verify", "verify-entry"))
    ap.add_argument("oids", nargs="*")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    ap.add_argument("--json")
    ap.add_argument("--baseline", help="逐条豁免清单（历史坏条目）；缺失/空 ⇒ rc=2")
    ap.add_argument("--audit-baseline", action="store_true",
                    help="逐条核 baseline 的 entry_oid：必须是 blob 且 principal/thread/field/value 绑定一致"
                         "（0 全核过 / 1 不一致并点名 / 2 判不了）")
    ap.add_argument("--since", type=int, default=None, help="只查 ts >= N 的新条目（给了就**不按 --limit 截断**）")
    ap.add_argument("--selftest", action="store_true")
    args = ap.parse_args(argv)
    if args.selftest:
        return selfcheck()
    repo = os.path.abspath(args.repo)
    if args.cmd == "verify-entry":
        rc, detail = verify_parent(repo, args.oids)
    elif args.cmd == "verify":
        rc, detail, resolved = verify_oids(repo, args.oids)
        if rc == 0:
            for r in resolved:
                print(r)
    elif args.cmd in ("check", "gate"):
        if args.audit_baseline:
            bpath = args.baseline or os.path.join(
                repo, "tools", "collab", "oid_gate_baseline.json")
            # **C1（reviewer-21 `5b2b7171`）**：审计通过后 main 会继续跑真仓库 check；
            # 必须把 bpath 一起传下去，否则 args.baseline=None ⇒ 历史豁免按「已豁免 0」全红、
            # 把 `--audit-baseline` 的返回码覆盖掉。
            args.baseline = bpath
            arc, red, undec = audit_baseline(repo, bpath)
            for r in red:
                print("AUDIT-RED   " + r)
            for u in undec:
                print("AUDIT-NEED  " + u)
            if arc != 0:
                print(("PASS " if arc == 0 else ("NEED " if arc == 2 else "FAIL "))
                      + "baseline audit（%d 条不一致 / %d 条判不了）" % (len(red), len(undec)))
                return arc
            # N1：通过也必须说话（静默成功与「没跑」在日志里分不开）
            print("AUDIT-OK %d 条（entry_oid 均为 blob，principal/thread/field/value 与条目一致）"
                  % _baseline_count(bpath))
        if args.cmd == "gate":
            st = selfcheck()
            if st != 0:
                return st
        rc, detail, stats = real_check(repo, max(1, args.limit), args.baseline, args.since)
        if args.json:
            with io.open(args.json, "w", encoding="utf-8", newline="") as fh:
                fh.write(json.dumps({"rc": rc, "detail": detail, "stats": stats},
                                    ensure_ascii=False, indent=1) + chr(10))
    else:
        ap.print_help()
        return 2
    print(("PASS " if rc == 0 else ("NEED " if rc == 2 else "FAIL ")) + detail)
    return rc


def selfcheck():
    """合成夹具自证：临时 git 仓库，**不碰真仓库**。"""
    global _IN_SELFTEST
    if _IN_SELFTEST:
        return 0        # 由 `gate` 间接调用时不再跑夹具（防递归）；真仓库 check 不受影响
    _IN_SELFTEST = True
    import tempfile
    ok = bad = 0

    def chk(cond, name):
        nonlocal ok, bad
        if cond:
            ok += 1
            print("PASS " + name)
        else:
            bad += 1
            print("FAIL " + name)

    tmp = tempfile.mkdtemp(prefix="oid-gate-")
    env = dict(_clean_env(), GIT_AUTHOR_NAME="t", GIT_AUTHOR_EMAIL="t@t",
               GIT_COMMITTER_NAME="t", GIT_COMMITTER_EMAIL="t@t")
    subprocess.run(["git", "init", "-q", tmp], check=True, env=env)
    subprocess.run(["git", "-C", tmp, "commit", "-q", "--allow-empty", "-m", "x"],
                   check=True, env=env)
    real = _git(tmp, ["rev-parse", "HEAD"]).stdout.strip()
    missing = "0" * 40

    def mk(refs, body, parent=None):
        e = {"id": "t", "ts": 1, "refs": refs, "body": body}
        if parent is not None:
            e["parent"] = parent
        return e

    cases = [
        ("A4-① 不存在的 head ⇒ 红并点名",
         [("p", "th", "e" * 40, mk({"head": missing}, {}))], 1, "解析不到"),
        ("A4-② 短 oid（能解析）⇒ 放行 + info",
         [("p", "th", "e" * 40, mk({"head": real[:7]}, {}))], 0, "短 oid"),
        ("A4-③ 空串 ⇒ 红（空不等于短）",
         [("p", "th", "e" * 40, mk({"head": ""}, {}))], 1, "空值"),
        ("parent 空 ⇒ 放行 + info（walgit 的「根条目无 parent」约定，不是空 oid）",
         [("p", "th", "e" * 40, mk({}, {}, parent=""))], 0, "根条目无 parent"),
        ("parent 指向不存在的条目 id ⇒ 红",
         [("p", "th", "e" * 40, mk({"head": real}, {}, parent="deadbeef" * 4))], 1, "解析不到"),
        ("全 oid ⇒ rc=0",
         [("p", "th", "e" * 40, mk({"head": real, "base": real}, {}))], 0, "可解析"),
        ("扫描 0 条 ⇒ rc=2（扫了 0 个不许算绿）", [], 2, "0 条条目"),
    ]
    for name, entries, want, needle in cases:
        rc, detail, _ = check_entries(entries, tmp, entry_ids={"dead" + "beef" * 7})
        chk(rc == want and needle in detail, "%s（rc=%d，%r）" % (name, rc, detail[:110]))
    rc, detail, _ = check_entries([("p", "th", "e" * 40, mk({"head": real}, {}))], tmp,
                                 git_cmd=("no-such-git-exe",))
    chk(rc == 2 and "判不了" in detail, "A4-④ git 不可用 ⇒ rc=2 不是红（%r）" % (detail[:80],))
    rc_s, d_s, _ = verify_oids(tmp, [real[:9]])
    rc_x, d_x, _ = verify_oids(tmp, ["zz" * 20])
    rc_m, d_m, _ = verify_oids(tmp, [missing])
    rc_f, d_f, got = verify_oids(tmp, [real])
    chk(rc_s == 1 and "非全 oid" in d_s, "写侧 短号 ⇒ 拒绝（%r）" % (d_s[:60],))
    chk(rc_x == 1 and "非全 oid" in d_x, "写侧 非 hex ⇒ 拒绝（%r）" % (d_x[:60],))
    chk(rc_m == 1 and "解析不到" in d_m, "写侧 解析不到 ⇒ 拒绝（%r）" % (d_m[:60],))
    chk(rc_f == 0 and got == [real], "写侧 全号 ⇒ 通过并回吐全 oid")
    # ---- baseline（A1/A3）：逐条豁免 + 未豁免的新坏条目仍红 + 缺失/空 ⇒ rc=2 ----
    import json as _json_b
    _bad_entry = ("dev", "th-bad", "a" * 40, mk({"head": missing}, {}))
    _bl_path = os.path.join(tmp, "baseline.json")
    with io.open(_bl_path, "w", encoding="utf-8", newline="") as fh:
        fh.write(_json_b.dumps({"entries": [{
            "principal": "dev", "thread": "th-bad", "entry_oid": "a" * 40, "field": "refs.head",
            "value": missing, "reason": "历史坏条目（夹具）", "added_at": "2026-10-27",
            "added_by": "t"}]}))
    rc_b, detail_b, stats_b = real_check(tmp, baseline_path=_bl_path)
    chk(rc_b == 2 and "判不了" in detail_b,
        "baseline 生效前：真仓库没有条目可读 ⇒ rc=2（不是绿；%r）" % (detail_b[:80],))
    # 直接喂条目：① baseline 命中 ⇒ 豁免可见；② 加一条不在 baseline 的 ⇒ 仍红
    bl = load_baseline(_bl_path)
    rc1, d1, st1 = check_entries([_bad_entry], tmp, baseline=bl)
    chk(rc1 == 0 and "豁免 1 处" in d1 and "baseline 共 1 条" in d1,
        "A3① baseline 里那条不存在 oid ⇒ 被豁免且审计输出可见（%r）" % (d1[:130],))
    _new_bad = ("dev", "th-new", "b" * 40, mk({"head": "1" * 40}, {}))
    rc2, d2, _ = check_entries([_bad_entry, _new_bad], tmp, baseline=bl)
    chk(rc2 == 1 and ("th-new" in d2) and "1" * 40 in d2,
        "A3② 不在 baseline 的新坏条目 ⇒ 仍红并点名（%r）" % (d2[:130],))
    try:
        load_baseline(os.path.join(tmp, "no-such-baseline.json"))
        _bl_missing = 0
    except GitUnavailable:
        _bl_missing = 2
    chk(_bl_missing == 2, "A3③ baseline 缺失 ⇒ rc=2 判不了（不许静默全绿）")
    with io.open(_bl_path, "w", encoding="utf-8", newline="") as fh:
        fh.write(_json_b.dumps({"entries": []}))
    try:
        load_baseline(_bl_path)
        _bl_empty = 0
    except GitUnavailable:
        _bl_empty = 2
    chk(_bl_empty == 2, "A3③b baseline 为空 ⇒ rc=2（不知道豁免什么等于没校验）")
    with io.open(_bl_path, "w", encoding="utf-8", newline="") as fh:
        fh.write(_json_b.dumps({"entries": [{"principal": "*", "thread": "*", "field": "parent",
                                            "value": "*", "reason": "x"}]}))
    try:
        load_baseline(_bl_path)
        _bl_wild = 0
    except ValueError:
        _bl_wild = 1
    chk(_bl_wild == 1, "红线① 通配豁免（`principal:\"*\"` / `value:\"*\"`）⇒ 拒绝（不许一刀切）")

    # ---- 豁免键必须含 thread + entry_oid（reviewer-21 的「复用旧坏 parent」攻击面）----
    # ① 同 principal / 同 thread / 同字段 / 同值，**但条目不同**（entry_oid 不同）⇒ 必须仍红。
    _same_value_other_entry = ("dev", "th-bad", "c" * 40, mk({"head": missing}, {}))
    rc_e1, d_e1, st_e1 = check_entries([_bad_entry, _same_value_other_entry], tmp, baseline=bl)
    chk(rc_e1 == 1 and st_e1.get("exempted") == 1,
        "牙① 同 principal/field/value 但**不同 entry_oid** ⇒ 仍红（豁免只落在这条条目上；rc=%d，豁免 %d）"
        % (rc_e1, st_e1.get("exempted", -1)))
    # ② 同 principal / 同 entry_oid / 同字段 / 同值，**但 thread 不同** ⇒ 必须仍红。
    _same_value_other_thread = ("dev", "th-other", "a" * 40, mk({"head": missing}, {}))
    rc_e2, d_e2, st_e2 = check_entries([_bad_entry, _same_value_other_thread], tmp, baseline=bl)
    chk(rc_e2 == 1 and st_e2.get("exempted") == 1,
        "牙② 同 principal/entry_oid/field/value 但**不同 thread** ⇒ 仍红（rc=%d，豁免 %d）"
        % (rc_e2, st_e2.get("exempted", -1)))
    # ③ 对照：键完全相同的同一条 ⇒ 仍然豁免（不是「一律不豁免」）
    rc_e3, _d3, st_e3 = check_entries([_bad_entry], tmp, baseline=bl)
    chk(rc_e3 == 0 and st_e3.get("exempted") == 1,
        "牙③ 对照：键完全相同 ⇒ 豁免仍生效（rc=%d）" % rc_e3)

    # ---- since（A4）：过滤新条目，且**不给 limit 悄悄截断** ----
    _old = ("dev", "th-old", "c" * 40, dict(mk({"head": missing}, {}), ts=10))
    _new = ("dev", "th-new2", "d" * 40, dict(mk({"head": "2" * 40}, {}), ts=100))
    rc_s, d_s, st_s = check_entries([_old, _new], tmp)
    chk(rc_s == 1, "since 前置：未过滤时两条都红（%r）" % (d_s[:60],))

    _w_all = _window([_old, _new], 1, None)
    _w_since = _window([_old, _new], 1, 50)
    chk(len(_w_all) == 1 and _w_all[0][1] == "th-new2",
        "since 前置②：不给 since 时 `--limit 1` 只留最新一条")
    chk(len(_w_since) == 1 and _w_since[0][1] == "th-new2",
        "A4：`--since 50` 过滤掉旧条目；**`--limit 1` 不会把它再截掉**（窗口缩小仍抓得到新坏条目）")
    _w_since2 = _window([_old, _new], 1, 200)
    chk(_w_since2 == [], "A4b：`--since` 之后没有条目 ⇒ 空（上层据此 rc=2，不许算绿）")
    for _name, _good in _audit_teeth():
        chk(_good, "audit " + _name)

    # 端到端牙（规则 36 的硬证据）：在**真 git 仓库**里写一条带坏 head 的 collab ref，
    # `real_check` 必须**读得到并判红**；再把那条逐条写进 baseline ⇒ 变绿且豁免可见。
    _entry_json = _json_b.dumps({"id": "th-fixture", "ts": 999, "refs": {"head": missing},
                                 "body": {}}).encode()
    blob = subprocess.run(["git", "-C", tmp, "hash-object", "-w", "--stdin"],
                          input=_entry_json, capture_output=True, env=env)
    assert blob.returncode == 0, blob.stderr
    blob_oid = blob.stdout.decode().strip()
    subprocess.run(["git", "-C", tmp, "update-ref", "refs/collab/inbox/dev/fixture-bad-entry",
                    blob_oid], check=True, env=env)
    rc_real, d_real, _ = real_check(tmp)
    chk(rc_real == 1 and "th-fixture" in d_real,
        "端到端牙：真仓库里带坏 head 的 collab 条目 ⇒ `real_check` 红并点名（%r）" % (d_real[:120],))
    rc_real2, d_real2, _ = real_check(tmp, baseline_path=_bl_path)
    chk(rc_real2 == 1, "端到端牙②：baseline 里只有别的条目 ⇒ 这条新坏条目**仍红**（豁免不外溢）")
    with io.open(_bl_path, "w", encoding="utf-8", newline="") as fh:
        fh.write(_json_b.dumps({"entries": [{
            "principal": "dev", "thread": "th-fixture", "entry_oid": blob_oid, "field": "refs.head",
            "value": missing, "reason": "历史坏条目（端到端夹具）", "added_at": "2026-10-27",
            "added_by": "t"}]}))
    rc_real3, d_real3, _ = real_check(tmp, baseline_path=_bl_path)
    chk(rc_real3 == 0 and "豁免 1 处" in d_real3,
        "端到端牙③：把**这条**逐条登记进 baseline ⇒ 变绿且豁免可见（%r）" % (d_real3[:120],))
    subprocess.run(["git", "-C", tmp, "update-ref", "-d", "refs/collab/inbox/dev/fixture-bad-entry"],
                   check=True, env=env)

    # 接线牙（规则 36）：`gate` 子命令**必须真的调用真仓库 check**，不是只跑自测。
    with io.open(_bl_path, "w", encoding="utf-8", newline="") as fh:   # 恢复成合法 baseline
        fh.write(_json_b.dumps({"entries": [{
            "principal": "dev", "thread": "th-bad", "entry_oid": "a" * 40, "field": "refs.head",
            "value": missing, "reason": "历史坏条目（夹具）", "added_at": "2026-10-27",
            "added_by": "t"}]}))
    import contextlib as _cl_gate
    _orig_check = check_entries
    try:
        globals()["check_entries"] = lambda *a, **k: (1, "INJECTED-REAL-CHECK-RED", {})
        _buf_gate = io.StringIO()
        with _cl_gate.redirect_stdout(_buf_gate):
            rc_gate = main(["gate", "--repo", tmp, "--baseline", _bl_path])
    finally:
        globals()["check_entries"] = _orig_check
    chk(rc_gate == 1, "接线牙：把真仓库 check 换成必红 ⇒ `gate` 必须红（只跑自测不算接入；rc=%d）" % rc_gate)

    print("%s: oid_gate selftest %d/%d" % ("GREEN" if not bad else "RED", ok, ok + bad))
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
