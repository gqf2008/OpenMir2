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


def project(repo, now=None, board=None):
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
    for tid, items in cards.items():
        items.sort(key=lambda p: (p[1].get("ts") or 0, p[0]))
        status = None
        owner = None
        tier = None
        tier_why = None
        for oid, e in items:
            if e.get("kind") != "status":
                continue
            body = e.get("body") or {}
            if body.get("status"):
                status = body["status"]
                owner = body.get("owner", owner)
                tier = body.get("tier", tier)
                tier_why = body.get("tier_why", tier_why)
        if status is None:
            for oid, e in items:
                if e.get("kind") == "issue":
                    body = e.get("body") or {}
                    status = body.get("status") or "open"
                    owner = body.get("owner", owner)
                    tier = body.get("tier", tier)
                    tier_why = body.get("tier_why", tier_why)
                    break
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
        column = None
        for col in columns:
            if col["status"] is None or col["status"] == status:
                column = col["name"]
                break
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
            "cards": result, "unparseable_refs": bad, "fetched_at": now,
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


def _entry(tid, kind, actor, ts, body):
    return {"version": 1, "kind": kind, "id": tid, "actor": actor, "ts": ts,
            "parent": "", "body": body, "sig": "ed25519:SELFTEST"}


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
        assert by_alt['t2']['column'] is None, by_alt['t2']

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
