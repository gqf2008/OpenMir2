#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""collab_read — 只读投影 `refs/collab/*`（wg 聚合故障期间的替代读路径）。

**为什么存在**（卡 `openmir2-collab-interface-wg-migration`）
  walgit 0.14.0 起协作接口由 `walgit collab` 迁到 `wg`。`wg collab entry`（写）可用，
  但 `wg collab board|thread|report` 在本仓库（14,529 条 inbox ref）一律
  `GitFailed`。trace2 证据（`GIT_TRACE2_EVENT`）：wg 起 `git cat-file --batch`
  后**只写不读**——14,529 × 41B ≈ 595KB 远超管道缓冲，与 git 的 stdout 互锁，
  子进程永不退出，wg 超时后报 GitFailed。本工具用**分块写读**规避该互锁。

**边界（必须知道）**
  * 只读：不写 refs、不签名、不推送。
  * **不验证签名**：每个条目带 `verified: null`。信任判定仍是 `wg` 的职责
    （它的聚合会在读回后核对 `verified: true`）；本工具只做投影。
  * 列谓词取自 `<repo>/.walgit/board.toml`：卡片进**第一个** `status` 匹配的列。
  * claim 只报**候选**：host 规定胜者是「读回后 `card.claim.oid` 与自己发布的一致」，
    本工具只能给「未过期 claim 里 ts 最大者」，标 `advisory: true`。

用法：
  python -I -B tools/orchestration/collab_read.py board  [--repo .] [--json]
  python -I -B tools/orchestration/collab_read.py thread <id> [--repo .] [--json]
  python -I -B tools/orchestration/collab_read.py --selftest
退出码：**0** 成功；**1** 用法错；**2** 判不了（git 失败 / 缺 board.toml / 无该线程）。
"""
import argparse
import json
import os
import shutil
import subprocess
import sys
import tempfile
import time

#: cat-file 每批写入的 oid 数。整批先写后读：64 × 41B ≈ 2.6KB，远小于管道缓冲，
#: git 的 stdout 再大也不会与我们的写端互锁（这正是 wg 的故障点）。
CHUNK = 64


class EnvError(Exception):
    """环境/仓库问题 ⇒ rc=2（判不了），不当作「判据被证伪」。"""


def _git(repo, *args):
    return subprocess.run(["git", "-C", repo] + list(args),
                          stdout=subprocess.PIPE, stderr=subprocess.PIPE)


def list_refs(repo, prefix):
    r = _git(repo, "for-each-ref", "--format=%(refname) %(objectname)", prefix)
    if r.returncode != 0:
        raise EnvError("for-each-ref %s 失败: %s" % (prefix, r.stderr.decode("utf-8", "replace").strip()))
    out = []
    for line in r.stdout.decode("utf-8", "replace").splitlines():
        name, _, oid = line.rpartition(" ")
        if name:
            out.append((name, oid))
    return out


def read_blobs(repo, oids):
    """分块写读，返回与 oids 等长的 list[bytes|None]（None = 对象缺失）。"""
    if not oids:
        return []
    p = subprocess.Popen(["git", "-C", repo, "cat-file", "--batch"],
                         stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                         stderr=subprocess.DEVNULL)
    out = []
    try:
        for i in range(0, len(oids), CHUNK):
            chunk = oids[i:i + CHUNK]
            p.stdin.write(b"".join(o.encode() + b"\n" for o in chunk))
            p.stdin.flush()
            for _ in chunk:
                hdr = p.stdout.readline()
                if not hdr:
                    raise EnvError("git cat-file --batch 提前结束（已读 %d/%d）" % (len(out), len(oids)))
                parts = hdr.split()
                if len(parts) < 3 or parts[1] != b"blob":
                    out.append(None)
                    continue
                size = int(parts[2])
                data = p.stdout.read(size)
                p.stdout.read(1)  # 结尾换行
                out.append(data)
    finally:
        try:
            p.stdin.close()
        except Exception:
            pass
        try:
            p.wait(timeout=30)
        except subprocess.TimeoutExpired:
            p.kill()
    if len(out) != len(oids):
        raise EnvError("cat-file 返回 %d 条，期望 %d" % (len(out), len(oids)))
    return out


def _kill_tree(p):
    """超时后按**进程树**终止。孙进程（git cat-file）会一直攥着 stdout，
    只杀直接子进程会让 communicate() 等 EOF 永久挂住——那会把「回归」变成「门挂住」。"""
    try:
        if os.name == "nt":
            subprocess.run(["taskkill", "/T", "/F", "/PID", str(p.pid)],
                           stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        else:
            import signal
            os.killpg(os.getpgid(p.pid), signal.SIGKILL)
    except Exception:  # noqa: BLE001
        try:
            p.kill()
        except Exception:
            pass


def run_bounded(cmd, timeout):
    """跑子进程并保证**有界**返回：超时 ⇒ (None, b"", b"timeout")。"""
    p = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                         start_new_session=(os.name != "nt"))
    try:
        out, err = p.communicate(timeout=timeout)
        return p.returncode, out, err
    except subprocess.TimeoutExpired:
        _kill_tree(p)
        try:
            p.communicate(timeout=10)
        except Exception:  # noqa: BLE001
            pass
        return None, b"", b"timeout"


def load_board_def(repo, board=None):
    """`board` 可覆盖列定义（对应旧 `wg collab board --board <path>`）。"""
    path = board or os.path.join(repo, ".walgit", "board.toml")
    if not os.path.exists(path):
        raise EnvError("缺 %s（看板列定义随仓库版本化，不能猜）" % path)
    try:
        import tomllib
        with open(path, "rb") as f:
            data = tomllib.load(f)
    except Exception as exc:  # noqa: BLE001
        raise EnvError("%s 解析失败: %s" % (path, exc))
    cols = []
    for c in data.get("column", []):
        cols.append({"name": c.get("name", ""), "status": c.get("status")})
    if not cols:
        raise EnvError("%s 没有任何 [[column]]" % path)
    return cols


def load_entries(repo):
    """返回 [(refname, oid, entry_dict|None)]。解析失败的条目不丢弃，标 None 以便点名。"""
    refs = list_refs(repo, "refs/collab/inbox")
    blobs = read_blobs(repo, [oid for _, oid in refs])
    out = []
    for (name, oid), blob in zip(refs, blobs):
        entry = None
        if blob is not None:
            try:
                entry = json.loads(blob.decode("utf-8", "replace"))
            except ValueError:
                entry = None
        out.append((name, oid, entry))
    return out


#: 非卡片条目类型：**只有**这些条目的线程不进看板（wg 实测：ci_* 与 card_claim-only 都隐藏）。
NON_CARD_KINDS = frozenset(("ci_claim", "ci_result", "card_claim"))


def _merged_flag(value):
    return value is True or value == 1 or (isinstance(value, str) and value.lower() == "true")


def _chain_order(items):
    """按 **parent 链序**给线程条目定序（wg 实测：同 ts 时链序胜出，不是 oid 序）。

    断链/环的条目按 (ts, oid) 追加到末尾，保证不丢条也不死循环。
    """
    by_oid = {oid: e for oid, e in items}
    children = {}
    roots = []
    for oid, e in items:
        parent = e.get("parent") or ""
        if isinstance(parent, str) and parent and parent in by_oid:
            children.setdefault(parent, []).append(oid)
        else:
            roots.append(oid)
    order, seen = [], set()
    roots.sort(key=lambda o: (by_oid[o].get('ts') or 0, o))
    stack = list(reversed(roots))
    while stack:
        oid = stack.pop()
        if oid in seen:
            continue
        seen.add(oid)
        order.append(oid)
        for ch in sorted(children.get(oid, []), reverse=True):
            stack.append(ch)
    rest = [oid for oid, _e in items if oid not in seen]
    rest.sort(key=lambda o: (by_oid[o].get("ts") or 0, o))
    return order + rest


def _is_state_entry(e):
    body = e.get("body") or {}
    if e.get("kind") == "status":
        return isinstance(body.get("status"), str)
    if e.get("kind") == "merge_result":
        return _merged_flag(body.get("merged"))
    return False


def _effective_state_oid(items):
    """wg 实测规则（reviewer 18 组夹具 / 63 张卡）：

    先取**全局最新 state**（ts 最大，同 ts 用 parent 链序），再在它的 state 后代子树里
    取 ts 最大的**终端**；其它分支一律忽略。

    与「所有 leaf 取最大 ts」的差别在分支形状：`in-progress10 → [closed40, in-progress60 → needs-review30]`
    这种，wg 看的是 in-progress60 那条支的终端 needs-review30，而不是旁支的 closed40。
    """
    by_oid = {oid: e for oid, e in items}
    rank = {oid: i for i, oid in enumerate(_chain_order(items))}
    states = {oid for oid, e in items if _is_state_entry(e)}
    if not states:
        return None

    def ts(oid):
        return by_oid[oid].get("ts") or 0

    def state_ancestors(oid):
        out, seen = [], set()
        parent = by_oid[oid].get("parent") or ""
        while isinstance(parent, str) and parent and parent in by_oid and parent not in seen:
            seen.add(parent)
            if parent in states:
                out.append(parent)
            parent = by_oid[parent].get("parent") or ""
        return out

    newest = max(states, key=lambda o: (ts(o), rank[o]))
    subtree = [o for o in states if o == newest or newest in state_ancestors(o)]
    terminals = [o for o in subtree
                 if not any(o in state_ancestors(x) for x in subtree if x != o)]
    return max(terminals, key=lambda o: (ts(o), rank[o]))


def project(repo, now=None, board=None):
    """按 `wg collab board` 的折叠语义投影（2026-11-07 用 20+ 条夹具对照 wg 实测）：

      1. 只有 `ci_claim`/`ci_result` 条目的线程**不显示**；
      2. 有效状态 = `status` 条目（`body.status`）与 `merge_result{merged 真}` 条目
         中 `ts` 最大者，后者记作 `merged`——`issue.body.status` **完全忽略**；
      3. 状态只看**未被后续状态覆盖的 leaf**（wg 实测），没有 state ⇒ `open`；
      4. 命中不了任何列的线程**不显示**（与 wg 一致，不是留在 column=None）。
    """
    now = int(now if now is not None else time.time())
    columns = load_board_def(repo, board)
    entries = load_entries(repo)

    cards = {}
    bad = []
    for name, oid, e in entries:
        if not isinstance(e, dict) or not e.get("id"):
            bad.append(name)
            continue
        cards.setdefault(e["id"], []).append((oid, e))

    result = []
    hidden = 0
    for tid, items in cards.items():
        items.sort(key=lambda p: (p[1].get("ts") or 0, p[0]))
        if all((e.get("kind") in NON_CARD_KINDS) for _o, e in items):
            hidden += 1
            continue
        by_oid = dict(items)
        # wg 实测：**ts 为主键**，同 ts 时用 parent 链序破平局（不是 oid 序，也不是深度优先）。
        rank = {oid: i for i, oid in enumerate(_chain_order(items))}
        ordered = sorted(items, key=lambda p: (p[1].get("ts") or 0, rank[p[0]]))
        winner_oid = _effective_state_oid(items)
        effective = None
        owner = ""          # wg 对「无 owner」输出空串，不是 null
        tier = None
        tier_why = None
        for oid, e in ordered:
            kind = e.get("kind")
            body = e.get("body") or {}
            ts = e.get("ts") or 0
            if kind == "status":
                # 显式 null 视为「没有该字段」，不得覆盖；空串是有效值（wg 实测）
                if body.get("owner") is not None:
                    owner = body["owner"]
                if body.get("tier") is not None:
                    tier = body["tier"]
                if body.get("tier_why") is not None:
                    tier_why = body["tier_why"]
                if oid == winner_oid and isinstance(body.get("status"), str):
                    effective = (ts, oid, body["status"])
            elif kind == "merge_result" and oid == winner_oid and _merged_flag(body.get("merged")):
                effective = (ts, oid, "merged")
        # owner/tier 跟随胜出的 leaf（wg 实测：非 leaf 高 ts 的 owner 不算数）。
        if effective:
            winner = by_oid[effective[1]].get("body") or {}
            if winner.get("owner") is not None:
                owner = winner["owner"]
            if winner.get("tier") is not None:
                tier = winner["tier"]
            if winner.get("tier_why") is not None:
                tier_why = winner["tier_why"]
        # 不做归一化：显式未知状态的线程匹配不到任何列 ⇒ 隐藏（wg 实测，不是回落 open）。
        status = effective[2] if effective else "open"
        column = None
        for col in columns:
            if col["status"] is None or col["status"] == status:
                column = col["name"]
                break
        if column is None:
            hidden += 1
            continue
        claims = []
        for oid, e in items:
            if e.get("kind") != "card_claim":
                continue
            body = e.get("body") or {}
            ttl = body.get("ttl")
            ts = e.get("ts") or 0
            live = ttl is None or (ts + int(ttl)) > now
            claims.append({"oid": oid, "actor": e.get("actor"), "ts": ts,
                           "ttl": ttl, "live": live,
                           "worktree": body.get("worktree"), "branch": body.get("branch")})
        live_claims = [c for c in claims if c["live"]]
        candidate = max(live_claims, key=lambda c: (c["ts"], c["oid"])) if live_claims else None
        last_ts = max((e.get("ts") or 0) for oid, e in items)
        result.append({
            "id": tid,
            "status": status,
            "column": column,
            "owner": owner,
            "tier": tier,
            "tier_why": tier_why,
            "entries": len(items),
            "last_ts": last_ts,
            "claim": {"advisory": True, "candidate": candidate, "live": live_claims},
            "verified": None,
        })
    result.sort(key=lambda c: (-c["last_ts"], c["id"]))
    return {"repo": os.path.abspath(repo), "columns": [c["name"] for c in columns],
            "cards": result, "hidden_threads": hidden,
            "unparseable_refs": bad, "fetched_at": now,
            "note": "只读投影；未验证签名（verified=null）。claim 只是候选，胜者须按 host 规范读回 claim oid 确认。"}


def render_board(board):
    lines = []
    lines.append("列: " + " | ".join(board["columns"]))
    lines.append("%-12s %-40s %-14s %-6s %s" % ("列", "卡片", "owner", "tier", "entries"))
    for c in board["cards"]:
        lines.append("%-12s %-40s %-14s %-6s %d" % (
            c["column"] or "-", c["id"], c["owner"] or "-", c["tier"] or "-", c["entries"]))
    if board["unparseable_refs"]:
        lines.append("不可解析条目: %d" % len(board["unparseable_refs"]))
    return "\n".join(lines)


def render_thread(thread):
    lines = []
    for e in thread["entries"]:
        body = e["body"] or {}
        first = body.get("work") or body.get("goal") or body.get("status") or ""
        first = " ".join(str(first).split())[:110]
        lines.append("%s  %-12s %-24s %s" % (e["ts"], e["kind"], e["actor"], first))
    return "\n".join(lines)


def read_thread(repo, tid):
    load_board_def(repo)  # 同一仓库口径；缺定义则 rc=2
    rows = [(oid, e) for name, oid, e in load_entries(repo)
            if isinstance(e, dict) and e.get("id") == tid]
    if not rows:
        raise EnvError("线程 %s 不存在（refs/collab/inbox 里没有该 id）" % tid)
    rows.sort(key=lambda p: (p[1].get("ts") or 0, p[0]))
    return {"id": tid, "entries": [{"oid": oid, "kind": e.get("kind"), "actor": e.get("actor"),
                                    "ts": e.get("ts"), "body": e.get("body"),
                                    "verified": None} for oid, e in rows]}


# ---------------------------------------------------------------- selftest

def _sh(repo, *args, **kw):
    r = subprocess.run(["git", "-C", repo] + list(args), stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if r.returncode != 0 and not kw.get("allow_fail"):
        raise RuntimeError("git %s: %s" % (args, r.stderr.decode("utf-8", "replace")))
    return r


def _write_entry(repo, idx, entry):
    blob = json.dumps(entry, ensure_ascii=False).encode()
    r = subprocess.run(["git", "-C", repo, "hash-object", "-w", "--stdin"],
                       input=blob, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
    if r.returncode != 0:
        raise RuntimeError("hash-object 失败")
    oid = r.stdout.decode().strip()
    _sh(repo, "update-ref", "refs/collab/inbox/selftest/%04d" % idx, oid)
    return oid


def _mk_repo(tmp, board=True):
    repo = tempfile.mkdtemp(dir=tmp)
    _sh(repo, "init", "-q", ".")
    if board:
        os.makedirs(os.path.join(repo, ".walgit"), exist_ok=True)
        with open(os.path.join(repo, ".walgit", "board.toml"), "w", encoding="utf-8", newline="\n") as f:
            f.write('[sort]\nby = "ts"\ndirection = "desc"\n\n'
                    '[[column]]\nname = "待认领"\nstatus = "open"\n\n'
                    '[[column]]\nname = "进行中"\nstatus = "in-progress"\n\n'
                    '[[column]]\nname = "已完成"\nstatus = "closed"\n')
    return repo


def _entry(tid, kind, actor, ts, body, parent=""):
    return {"version": 1, "kind": kind, "id": tid, "actor": actor, "ts": ts,
            "parent": parent, "body": body, "sig": "ed25519:SELFTEST"}


def selftest():
    checks = 0
    tmp = tempfile.mkdtemp(prefix="collab_read_selftest_")
    try:
        # 1) 空仓库：0 卡片，不报错
        repo = _mk_repo(tmp)
        b = project(repo)
        checks += 1
        assert b["cards"] == [], "空仓库应无卡片"

        # 2) 投影：status 决定列；过期 claim 不算，未过期 claim 作候选
        now = 1_800_000_000
        _write_entry(repo, 1, _entry("t1", "issue", "w1", 100, {"status": "open", "owner": None}))
        _write_entry(repo, 2, _entry("t1", "status", "w1", 200, {"status": "in-progress", "owner": "w1", "tier": "L"}))
        _write_entry(repo, 3, _entry("t1", "card_claim", "w2", now - 1000, {"ttl": 10, "worktree": "old"}))
        _write_entry(repo, 4, _entry("t1", "card_claim", "w3", now - 10, {"ttl": 3600, "worktree": "new"}))
        _write_entry(repo, 5, _entry("t2", "issue", "w1", 150, {"status": "open"}))
        b = project(repo, now=now)
        by = {c["id"]: c for c in b["cards"]}
        checks += 1
        assert set(by) == {"t1", "t2"}, by.keys()
        checks += 1
        assert by["t1"]["column"] == "进行中", by["t1"]["column"]
        checks += 1
        assert by["t1"]["tier"] == "L", by["t1"]
        checks += 1
        assert by["t2"]["column"] == "待认领", by["t2"]["column"]
        checks += 1
        assert len(by["t1"]["claim"]["live"]) == 1, by["t1"]["claim"]
        checks += 1
        assert by["t1"]["claim"]["candidate"]["worktree"] == "new", by["t1"]["claim"]
        checks += 1
        assert by["t1"]["claim"]["advisory"] is True
        checks += 1
        assert by["t1"]["verified"] is None, "不验证签名必须显式标 null"
        checks += 1
        assert "进行中" in render_board(b)

        # 3) thread：按 ts 升序、条目数正确
        t = read_thread(repo, "t1")
        checks += 1
        assert [e["kind"] for e in t["entries"]] == ["issue", "status", "card_claim", "card_claim"], t
        checks += 1
        assert [e["ts"] for e in t["entries"]] == sorted(e["ts"] for e in t["entries"])
        checks += 1
        assert "card_claim" in render_thread(t)

        # 4) 缺 board.toml ⇒ EnvError（rc=2），不能猜列
        repo_nb = _mk_repo(tmp, board=False)
        _write_entry(repo_nb, 1, _entry("x", "issue", "w1", 1, {"status": "open"}))
        try:
            project(repo_nb)
            raise AssertionError("缺 board.toml 应抛 EnvError")
        except EnvError:
            checks += 1

        # 4b) --board 覆盖列定义（等价旧 `wg collab board --board <path>`）
        alt = os.path.join(repo, 'alt-board.toml')
        with open(alt, 'w', encoding='utf-8', newline='') as fh:
            fh.write('[[column]]' + chr(10) + 'name = "WIP"' + chr(10) + 'status = "in-progress"' + chr(10))
        by_alt = {c['id']: c for c in project(repo, now=now, board=alt)['cards']}
        checks += 1
        assert by_alt['t1']['column'] == 'WIP', by_alt['t1']
        checks += 1
        assert 't2' not in by_alt and project(repo, now=now, board=alt)['hidden_threads'] >= 1, by_alt

        # 5) 未过期/无 ttl 的 claim 都算活
        _write_entry(repo, 6, _entry("t3", "issue", "w1", now, {"status": "open"}))
        _write_entry(repo, 7, _entry("t3", "card_claim", "w4", now - 10, {}))
        by = {c["id"]: c for c in project(repo, now=now)["cards"]}
        checks += 1
        assert len(by["t3"]["claim"]["live"]) == 1, by["t3"]

        # 6) 规模牙：2000 条 ref。
        #    互锁的根在「一次写入多少字节」：CHUNK × ~41B 必须远小于管道缓冲（64KB）。
        #    这条不变量直接钉住修复契约——把 CHUNK 调大会立刻咬住，而不是把门挂住。
        checks += 1
        assert CHUNK * 41 <= 8192, (
            "批量写入 %d 条约 %d B，逼近管道缓冲 ⇒ 会与 git stdout 互锁" % (CHUNK, CHUNK * 41))
        big = _mk_repo(tmp)
        blobs = {}
        for k in range(50):
            payload = json.dumps(_entry("bulk-%d" % k, "comment", "w1", k, {"work": "x"}),
                                 ensure_ascii=False).encode()
            blobs[k] = subprocess.run(["git", "-C", big, "hash-object", "-w", "--stdin"],
                                      input=payload, stdout=subprocess.PIPE).stdout.decode().strip()
        lines = "".join(("create refs/collab/inbox/selftest/bulk%05d %s" + chr(10)) % (i2, blobs[i2 % 50])
                        for i2 in range(2000))
        rr = subprocess.run(["git", "-C", big, "update-ref", "--stdin"], input=lines.encode(),
                            stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        assert rr.returncode == 0, rr.stderr.decode("utf-8", "replace")
        # 同一进程内直读 2000 条：分块写读 ⇒ 秒级返回。若退化回整批写，这里会被
        # tool_selftest_gate 的进程树超时终止（红），不会假绿。
        oids = [oid for _n, oid in list_refs(big, "refs/collab/inbox")]
        checks += 1
        assert len(oids) == 2000, len(oids)
        got = read_blobs(big, oids)
        checks += 1
        assert len(got) == 2000 and all(g is not None for g in got)
        checks += 1
        assert len(project(big)["cards"]) == 50
        # 7) wg 投影语义（差分夹具固化；wg 在同形 20+ 条夹具上的输出与这里逐 id 一致）
        nl = chr(10)
        wgboard = os.path.join(tmp, 'wg-board.toml')
        with open(wgboard, 'w', encoding='utf-8', newline='') as fh:
            fh.write('[[column]]' + nl + 'name = "待认领"' + nl + 'status = "open"' + nl
                     + '[[column]]' + nl + 'name = "进行中"' + nl + 'status = "in-progress"' + nl
                     + '[[column]]' + nl + 'name = "已完成"' + nl + 'status = "closed"' + nl
                     + '[[column]]' + nl + 'name = "待审"' + nl + 'status = "needs-review"' + nl
                     + '[[column]]' + nl + 'name = "受阻"' + nl + 'status = "blocked"' + nl
                     + '[[column]]' + nl + 'name = "已合并未收口"' + nl + 'status = "merged"' + nl)
        r2 = _mk_repo(tmp)
        _write_entry(r2, 1, _entry('m1', 'issue', 'a', 1, {'status': 'needs-review'}))
        _write_entry(r2, 2, _entry('m1', 'status', 'a', 10, {'status': 'needs-review'}))
        _write_entry(r2, 3, _entry('m1', 'merge_result', 'a', 20, {'merged': True}))
        _write_entry(r2, 4, _entry('m2', 'status', 'a', 10, {'status': 'closed'}))
        _write_entry(r2, 5, _entry('m2', 'merge_result', 'a', 20, {'merged': True}))
        _write_entry(r2, 6, _entry('m3', 'issue', 'a', 1, {'status': 'open'}))
        _write_entry(r2, 7, _entry('m3', 'merge_result', 'a', 5, {'merged': False}))
        _write_entry(r2, 8, _entry('m4', 'status', 'a', 1, {'status': 'weird'}))
        _write_entry(r2, 9, _entry('m5', 'ci_claim', 'a', 1, {}))
        _write_entry(r2, 10, _entry('m6', 'issue', 'a', 1, {'status': 'blocked'}))
        _write_entry(r2, 11, _entry('m7', 'card_claim', 'a', 1, {}))
        # 同 ts 定序：wg 用 parent 链序破平局（不是 oid 序）
        mo = _write_entry(r2, 20, _entry('m8', 'merge_result', 'a', 5, {'merged': True}))
        _write_entry(r2, 21, _entry('m8', 'status', 'a', 5, {'status': 'closed'}, parent=mo))
        so = _write_entry(r2, 22, _entry('m9', 'status', 'a', 5, {'status': 'closed'}))
        _write_entry(r2, 23, _entry('m9', 'merge_result', 'a', 5, {'merged': True}, parent=so))
        # ts 为主键：分叉时更晚 ts 的浅支胜出（wg 实测）
        fo = _write_entry(r2, 30, _entry('m10', 'issue', 'a', 1, {}))
        f1o = _write_entry(r2, 31, _entry('m10', 'status', 'a', 10, {'status': 'in-progress'}, parent=fo))
        f2o = _write_entry(r2, 32, _entry('m10', 'status', 'a', 20, {'status': 'closed'}, parent=f1o))
        _write_entry(r2, 33, _entry('m10', 'status', 'a', 25, {'status': 'needs-review'}, parent=f2o))
        _write_entry(r2, 34, _entry('m10', 'status', 'a', 30, {'status': 'in-progress'}, parent=f1o))
        # leaf state：非 leaf 的高 ts 不得压过链尾（wg 实测最小反例）
        lo = _write_entry(r2, 40, _entry('m11', 'issue', 'a', 1, {}))
        l1 = _write_entry(r2, 41, _entry('m11', 'status', 'a', 50, {'status': 'in-progress'}, parent=lo))
        _write_entry(r2, 42, _entry('m11', 'status', 'a', 40, {'status': 'closed'}, parent=l1))
        # owner/tier 跟随 leaf（wgowner o1 / wgowner3 q1,q7 实测）
        oo = _write_entry(r2, 50, _entry('m12', 'issue', 'a', 1, {}))
        o1 = _write_entry(r2, 51, _entry('m12', 'status', 'a', 60,
                                        {'status': 'in-progress', 'owner': 'A', 'tier': 'T1',
                                         'tier_why': 'why1'}, parent=oo))
        _write_entry(r2, 52, _entry('m12', 'status', 'a', 55,
                                    {'status': 'closed', 'owner': 'B', 'tier': 'T2',
                                     'tier_why': 'why2'}, parent=o1))
        # leaf 无 tier ⇒ 回退到非 leaf 的 tier
        n1 = _write_entry(r2, 60, _entry('m13', 'issue', 'a', 1, {}))
        n2 = _write_entry(r2, 61, _entry('m13', 'status', 'a', 60,
                                        {'status': 'in-progress', 'tier': 'T1',
                                         'tier_why': 'why1'}, parent=n1))
        _write_entry(r2, 62, _entry('m13', 'status', 'a', 55, {'status': 'closed'}, parent=n2))
        # 显式 owner:null 视为「无 owner」，回退到更早的非 null
        u1 = _write_entry(r2, 70, _entry('m14', 'issue', 'a', 1, {}))
        u2 = _write_entry(r2, 71, _entry('m14', 'status', 'a', 50,
                                        {'status': 'in-progress', 'owner': 'A'}, parent=u1))
        _write_entry(r2, 72, _entry('m14', 'status', 'a', 40,
                                    {'status': 'closed', 'owner': None}, parent=u2))
        # 循环级 null 守卫：胜出 leaf 本身 owner=null 时不得把它记成 owner
        x0 = _write_entry(r2, 80, _entry('m15', 'issue', 'a', 1, {}))
        x1 = _write_entry(r2, 81, _entry('m15', 'status', 'a', 10, {'status': 'in-progress', 'owner': 'A'}, parent=x0))
        _write_entry(r2, 82, _entry('m15', 'status', 'a', 40, {'status': 'closed'}, parent=x1))
        _write_entry(r2, 83, _entry('m15', 'status', 'a', 60, {'status': 'in-progress', 'owner': None}, parent=x1))
        # 空串是有效 owner；ownerless 输出空串；显式 tier:null 回退
        e0 = _write_entry(r2, 90, _entry('m16', 'issue', 'a', 1, {}))
        e1 = _write_entry(r2, 91, _entry('m16', 'status', 'a', 50, {'status': 'in-progress', 'owner': 'A'}, parent=e0))
        _write_entry(r2, 92, _entry('m16', 'status', 'a', 40, {'status': 'closed', 'owner': ''}, parent=e1))
        f0 = _write_entry(r2, 93, _entry('m17', 'issue', 'a', 1, {}))
        f1o = _write_entry(r2, 94, _entry('m17', 'status', 'a', 50, {'status': 'in-progress', 'tier': 'T1', 'tier_why': 'why1'}, parent=f0))
        _write_entry(r2, 95, _entry('m17', 'status', 'a', 40, {'status': 'closed', 'tier': None, 'tier_why': None}, parent=f1o))
        _write_entry(r2, 96, _entry('m18', 'status', 'a', 30, {'status': 'in-progress'}))
        # 循环级 tier/tier_why 的 null 守卫：胜出状态无 tier 时，更晚的 tier:null 不得清掉更早的值
        g0 = _write_entry(r2, 100, _entry('m19', 'issue', 'a', 1, {}))
        g1 = _write_entry(r2, 101, _entry('m19', 'status', 'a', 10, {'status': 'in-progress', 'tier': 'T1', 'tier_why': 'why1'}, parent=g0))
        _write_entry(r2, 102, _entry('m19', 'status', 'a', 40, {'status': 'closed'}, parent=g1))
        _write_entry(r2, 103, _entry('m19', 'status', 'a', 60, {'status': 'in-progress', 'tier': None, 'tier_why': None}, parent=g1))
        # 循环级 owner 的空串分支：晚到的 owner="" 必须覆盖更早的 A
        h0 = _write_entry(r2, 110, _entry('m20', 'issue', 'a', 1, {}))
        h1 = _write_entry(r2, 111, _entry('m20', 'status', 'a', 10, {'status': 'in-progress', 'owner': 'A'}, parent=h0))
        _write_entry(r2, 112, _entry('m20', 'status', 'a', 40, {'status': 'closed'}, parent=h1))
        _write_entry(r2, 113, _entry('m20', 'status', 'a', 60, {'status': 'in-progress', 'owner': ''}, parent=h1))
        # 循环级 owner 守卫（不被 leaf 覆盖掩盖）：胜出状态不带 owner 键
        j0 = _write_entry(r2, 120, _entry('m21', 'issue', 'a', 1, {}))
        j1 = _write_entry(r2, 121, _entry('m21', 'status', 'a', 10, {'status': 'in-progress', 'owner': 'A'}, parent=j0))
        _write_entry(r2, 122, _entry('m21', 'status', 'a', 40, {'status': 'closed'}, parent=j1))
        j3 = _write_entry(r2, 123, _entry('m21', 'status', 'a', 60, {'status': 'in-progress', 'owner': ''}, parent=j1))
        _write_entry(r2, 124, _entry('m21', 'status', 'a', 70, {'status': 'in-progress'}, parent=j3))
        # 8) 最新 state → 其 state 子树终端（reviewer 的 y3/n3/k1/k2 四个反例）
        r0 = _write_entry(r2, 200, _entry('y3', 'issue', 'a', 1, {}))
        r1 = _write_entry(r2, 201, _entry('y3', 'status', 'a', 10, {'status': 'in-progress'}, parent=r0))
        _write_entry(r2, 202, _entry('y3', 'status', 'a', 40, {'status': 'closed'}, parent=r1))
        r3 = _write_entry(r2, 203, _entry('y3', 'status', 'a', 60, {'status': 'in-progress'}, parent=r1))
        _write_entry(r2, 204, _entry('y3', 'status', 'a', 30, {'status': 'needs-review'}, parent=r3))
        q0 = _write_entry(r2, 210, _entry('n3', 'issue', 'a', 1, {}))
        q1 = _write_entry(r2, 211, _entry('n3', 'status', 'a', 10, {'status': 'in-progress'}, parent=q0))
        _write_entry(r2, 212, _entry('n3', 'status', 'a', 90, {'status': 'closed'}, parent=q1))
        q3 = _write_entry(r2, 213, _entry('n3', 'status', 'a', 95, {'status': 'in-progress'}, parent=q1))
        _write_entry(r2, 214, _entry('n3', 'status', 'a', 30, {'status': 'blocked'}, parent=q3))
        t0 = _write_entry(r2, 220, _entry('k1', 'issue', 'a', 1, {}))
        t1 = _write_entry(r2, 221, _entry('k1', 'merge_result', 'a', 70, {'merged': True}, parent=t0))
        _write_entry(r2, 222, _entry('k1', 'status', 'a', 30, {'status': 'closed'}, parent=t1))
        _write_entry(r2, 223, _entry('k1', 'status', 'a', 50, {'status': 'needs-review'}, parent=t0))
        v0 = _write_entry(r2, 230, _entry('k2', 'issue', 'a', 1, {}))
        v1 = _write_entry(r2, 231, _entry('k2', 'status', 'a', 70, {'status': 'in-progress'}, parent=v0))
        _write_entry(r2, 232, _entry('k2', 'merge_result', 'a', 30, {'merged': True}, parent=v1))
        _write_entry(r2, 233, _entry('k2', 'status', 'a', 50, {'status': 'needs-review'}, parent=v0))
        by = {c['id']: c for c in project(r2, board=wgboard)['cards']}
        checks += 1; assert by['m1']['column'] == '已合并未收口', by.get('m1')
        checks += 1; assert by['m2']['column'] == '已合并未收口', by.get('m2')
        checks += 1; assert by['m3']['column'] == '待认领', by.get('m3')
        checks += 1; assert 'm4' not in by, '显式未知状态必须隐藏（wg 实测），不得回落 open'
        checks += 1; assert 'm5' not in by, '只有 ci_* 条目的线程必须隐藏'
        checks += 1; assert by['m6']['column'] == '待认领', by.get('m6')
        checks += 1; assert 'm7' not in by, '只有 card_claim 的线程必须隐藏'
        checks += 1; assert by['m8']['column'] == '已完成', by.get('m8')
        checks += 1; assert by['m9']['column'] == '已合并未收口', by.get('m9')
        checks += 1; assert by['m10']['column'] == '进行中', by.get('m10')
        checks += 1; assert by['m11']['column'] == '已完成', by.get('m11')
        checks += 1; assert by['m12']['owner'] == 'B', by.get('m12')
        checks += 1; assert by['m12']['tier'] == 'T2', by.get('m12')
        checks += 1; assert by['m13']['tier'] == 'T1', by.get('m13')
        checks += 1; assert by['m14']['owner'] == 'A', by.get('m14')
        checks += 1; assert by['m15']['owner'] == 'A', by.get('m15')
        checks += 1; assert by['m12']['tier_why'] == 'why2', by.get('m12')
        checks += 1; assert by['m13']['tier_why'] == 'why1', by.get('m13')
        checks += 1; assert by['m16']['owner'] == '', by.get('m16')
        checks += 1; assert by['m17']['tier'] == 'T1', by.get('m17')
        checks += 1; assert by['m17']['tier_why'] == 'why1', by.get('m17')
        checks += 1; assert by['m18']['owner'] == '', by.get('m18')
        checks += 1; assert by['m19']['tier'] == 'T1', by.get('m19')
        checks += 1; assert by['m19']['tier_why'] == 'why1', by.get('m19')
        checks += 1; assert by['m20']['owner'] == '', by.get('m20')
        checks += 1; assert by['m21']['owner'] == '', by.get('m21')
        checks += 1; assert by['y3']['column'] == '待审', by.get('y3')
        checks += 1; assert by['n3']['column'] == '受阻', by.get('n3')
        checks += 1; assert by['k1']['column'] == '已完成', by.get('k1')
        checks += 1; assert by['k2']['column'] == '已合并未收口', by.get('k2')
        checks += 1; assert project(r2, board=wgboard)['hidden_threads'] == 3
        onlyclosed = os.path.join(tmp, 'onlyclosed.toml')
        with open(onlyclosed, 'w', encoding='utf-8', newline='') as fh:
            fh.write('[[column]]' + nl + 'name = "已完成"' + nl + 'status = "closed"' + nl)
        r3 = _mk_repo(tmp)
        _write_entry(r3, 1, _entry('x', 'issue', 'a', 1, {}))
        b3 = project(r3, board=onlyclosed)
        checks += 1; assert b3['cards'] == [] and b3['hidden_threads'] == 1, b3
        # 期望条数锚：删掉任何一条 check 都会让这个数字变，避免『删了仍绿』
        assert checks == 53, 'checks=%d（改动用例后同步这个锚）' % checks

    finally:
        shutil.rmtree(tmp, ignore_errors=True)
    print("SELFTEST_PASS=%d" % checks)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(add_help=True)
    ap.add_argument("--selftest", action="store_true")
    ap.add_argument("--repo", default=".")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--board", default=None,
                    help="覆盖列定义路径（等价旧 `wg collab board --board <path>`）")
    ap.add_argument("cmd", nargs="?")
    ap.add_argument("arg", nargs="?")
    a = ap.parse_args(argv)
    if a.selftest:
        return selftest()
    if a.cmd not in ("board", "thread"):
        print("用法：collab_read.py board|thread <id> [--repo .] [--json] | --selftest", file=sys.stderr)
        return 1
    try:
        if a.cmd == "board":
            data = project(a.repo, board=a.board)
            text = render_board(data)
        else:
            if not a.arg:
                print("thread 需要 <id>", file=sys.stderr)
                return 1
            data = read_thread(a.repo, a.arg)
            text = render_thread(data)
    except EnvError as exc:
        print("collab_read: 判不了：%s" % exc, file=sys.stderr)
        return 2
    print(json.dumps(data, ensure_ascii=False, indent=1) if a.json else text)
    return 0


if __name__ == "__main__":
    sys.exit(main())
