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


def load_entries(repo, limit=DEFAULT_LIMIT, git_cmd=("git",)):
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
    return entries[:limit], seen


#: 这些字段**空了就是错**（一条 patch/review 声称有 base/head 却留空 ⇒ 无从对账）。
EMPTY_IS_RED = {"refs.base", "refs.head", "body.base", "body.head",
                "body.reviewed_head", "body.oid", "body.merge_oid"}
#: 这些字段空/假是**正常状态**（未合并 / 根条目无 parent），跳过不判。
EMPTY_IS_OK = {"body.merged", "body.merged.head", "parent"}


def load_entry_ids(repo, git_cmd=("git",)):
    """所有条目的 **walgit 条目 id**（ref 名末段，32 位 hex；**不是 git 对象名**）。"""
    out = _git(repo, ["for-each-ref", "--format=%(refname)", "refs/collab/inbox/"],
               git_cmd=git_cmd).stdout
    return {ln.rsplit("/", 1)[-1] for ln in out.splitlines() if ln.strip()}


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


def check_entries(entries, repo, git_cmd=("git",), entry_ids=None):
    """四态判定。返回 (rc, detail, stats)。"""
    stats = {"entries": len(entries), "oids": 0, "ok": 0, "short": 0,
             "non_commit": 0, "bad": 0, "empty": 0}
    bad, short_hits, skipped = [], [], []
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
                        bad.append("空值：%s / %s / %s / 条目 %s ⇒ 空不等于短，判红"
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
                        bad.append("解析不到：%s / %s / %s=%s（条 %s，原文 %r）"
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
              "空值 %d、解析不到 %d"
              % (stats["entries"], stats["oids"], stats["ok"], stats["short"],
                 stats["non_commit"], stats["empty"], stats["bad"]))
    if bad:
        return 1, "；".join(bad[:12]) + ("（共 %d 处）" % len(bad)) + " || " + detail, stats
    if short_hits:
        detail += "；info：短 oid %d 处（可解析即放行）：%s" % (
            len(short_hits), "、".join(short_hits[:6]))
    if skipped:
        detail += "；info：跳过 %d 处（根条目无 parent / 未合并 / 自由文本 / 非 commit）" % len(skipped)
    return 0, detail, stats


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


def main(argv=None):
    ap = argparse.ArgumentParser(description="collab 条目 oid 可解析性门（只读，只报不改）")
    ap.add_argument("cmd", nargs="?", choices=("check", "verify", "verify-entry"))
    ap.add_argument("oids", nargs="*")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--limit", type=int, default=DEFAULT_LIMIT)
    ap.add_argument("--json")
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
    elif args.cmd == "check":
        try:
            entries, total = load_entries(repo, max(1, args.limit))
        except GitUnavailable as ex:
            print("oid_gate: 判不了：%s" % ex)
            return 2
        try:
            ids = load_entry_ids(repo)
        except GitUnavailable as ex:
            print("oid_gate: 判不了：%s" % ex)
            return 2
        rc, detail, stats = check_entries(entries, repo, entry_ids=ids)
        detail = "（库内共 %d 条，本次扫最近 %d 条）%s" % (total, len(entries), detail)
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
    print("%s: oid_gate selftest %d/%d" % ("GREEN" if not bad else "RED", ok, ok + bad))
    return 0 if not bad else 1


if __name__ == "__main__":
    sys.exit(main())
