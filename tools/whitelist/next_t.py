#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`T-*` 编号**预约取号器**（卡 `openmir2-t-number-reservation`，tier=L）。

## 解决什么

`mir2-rs/tests/parity/whitelist.md` 的 `T-<n>` 是**全仓共用的一个线性编号空间**，谁先写谁占。
2026-10-23 一天撞号 4 次，根因是每条并行车道都在**开工时**自己 grep 一次「当前最大号」——
并行时必然读到旧值。本件把取号改成**事前预约 + 事后校验**。

## 用法（唯一取号入口）

    python -I tools/whitelist/next_t.py --card <卡 id>            # 取号并原子追加认领行
    python -I tools/whitelist/next_t.py --card <id> --dry-run     # 只看号，不写
    python -I tools/whitelist/next_t.py --card <id> --force-new   # 换号（旧「在用」行改「已释放」）
    python -I tools/whitelist/next_t.py --bootstrap               # 历史登记批量补「已落地」
    python -I tools/whitelist/next_t.py selftest                  # 自检（含并发/跨 worktree/远端新鲜度）

stdout 只输出一行 `T-<n>`（便于 `N=$(python -I ... --card x)`），人读信息走 stderr。

## 原子性怎么保证（五层）

1. **同机跨 worktree 共享 journal（真互斥）**：认领先写进 `git rev-parse --git-common-dir`
   下的 `walgit/t-reservations.journal`（一行一认领：号 / 时间 / 卡 / worktree）。**所有 worktree
   共用同一个 common dir**，所以拿到的号是同一台机器上的全局序号，不是「各自表里的局部最大值」。
   journal 不进仓库、不影响 diff；号只增不减，崩溃最多浪费一个号，不会重号。
2. **跨进程互斥锁**：`msvcrt.locking` / `fcntl.flock` 锁住 journal 文件（无 journal 时锁预约表），
   在「读表 -> 算号 -> 写 journal -> 写表」全程持有；进程死亡由 OS 释放，无死锁。
3. **单次追加 + fsync**：journal 与认领行都是**一次** `write`（一行、`newline="\\n"` 不翻译），
   随后 `flush` + `os.fsync`；写后**回读校验**，解析不出 ⇒ 非零退出。
4. **远端新鲜度（fail-closed）**：锁内 `git ls-remote` 取 `walgit/master` 的**当前** oid，与本地
   remote-tracking ref 比对；不一致就 `git fetch` 更新后再读。**读到的东西永远是远端当前 master**。
   `ls-remote` / `fetch` 失败 ⇒ **拒绝发号**（rc≠0）；只有显式 `--allow-stale` 才降级为「本地视图 +
   大声告警」，且此时仍受第 1 层 journal 保护。
5. **号 = max(journal, 本地预约表, 本地 whitelist, 远端表, 远端 whitelist) + 1**。号只增不减，
   `已释放` 不回收（历史 commit / 文档里可能已有引用，回收 = 制造歧义）。

解析口径：逐行、只认**行首** `| T-<n> |`（正文里的 `T-<n>` 引用不查）；``` 围栏代码块内的示例行不算
登记行；**围栏数不成对 ⇒ 拒绝取号**（否则围栏后的号会被静默跳过，可能发出已占用的号）。

## 幂等

同一 `--card` 重复调用 ⇒ 直接返回它**已有的**「在用」号，不重复追加。`--force-new` 会先把该卡
旧的「在用」行改写成「已释放」，再追加新行（因此不会留下同卡两个「在用」）。
"""
from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
ROOT = os.path.normpath(os.path.join(HERE, "..", ".."))
DEFAULT_TABLE = os.path.join(ROOT, "mir2-rs", "tests", "parity", "T-RESERVATIONS.md")
DEFAULT_WHITELIST = os.path.join(ROOT, "mir2-rs", "tests", "parity", "whitelist.md")
TABLE_REL = "mir2-rs/tests/parity/T-RESERVATIONS.md"
WHITELIST_REL = "mir2-rs/tests/parity/whitelist.md"

LIVE, LANDED, RELEASED = "在用", "已落地", "已释放"
STATUSES = (LIVE, LANDED, RELEASED)

# 预约表一行：| T-196 | card/x | 2026-10-23T12:00:00Z | 在用 |
ROW_ONE = re.compile(
    r"^\|\s*(T-\d+)\s*\|\s*([^|]*?)\s*\|\s*([^|]*?)\s*\|\s*(在用|已落地|已释放)\s*\|\s*$")
WROW_ONE = re.compile(r"^\|\s*(T-\d+)\s*\|")
NUM_RE = re.compile(r"^T-(\d+)$")
JROW_RE = re.compile(r"^T-(\d+)\t")

HISTORY_CARD = "historical-pre-reservation"
HISTORY_TS = "2026-10-23T00:00:00Z"


# --------------------------------------------------------------------------- 解析
def scan_lines(text):
    """⇒ (lines, fence_ok)。lines=[(line_text, line_no)]，跳过 ``` 围栏内的行。

    `fence_ok=False` = 围栏不成对（坏输入，**必须** fail-closed：否则围栏后的登记行被静默跳过）。
    """
    out, in_fence = [], False
    for idx, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        out.append((line, idx))
    return out, not in_fence


def parse_table(text):
    """⇒ [{num:int, token, card, ts, status, line}]（文件序）。坏行/围栏由调用方另行检查。"""
    rows, _ok = scan_lines(text)
    out = []
    for line, no in rows:
        m = ROW_ONE.match(line)
        if m:
            out.append({
                "num": int(NUM_RE.match(m.group(1)).group(1)),
                "token": m.group(1),
                "card": m.group(2).strip(),
                "ts": m.group(3).strip(),
                "status": m.group(4),
                "line": no,
            })
    return out


def parse_whitelist_numbers(text):
    """whitelist.md 登记行（行首 `| T-<n> |`）里出现过的号（去重）。"""
    rows, _ok = scan_lines(text)
    return {int(NUM_RE.match(m.group(1)).group(1))
            for line, _no in rows for m in [WROW_ONE.match(line)] if m}


def read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        return fh.read()


def now_utc():
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# --------------------------------------------------------------------------- journal
def git_common_dir(repo):
    """`git rev-parse --git-common-dir`（**所有 worktree 共用**的那个目录）⇒ 绝对路径 | None。"""
    try:
        proc = subprocess.run(["git", "-C", repo, "rev-parse", "--git-common-dir"],
                              capture_output=True, text=True, encoding="utf-8",
                              errors="replace", timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    if proc.returncode != 0:
        return None
    path = proc.stdout.strip()
    if not path:
        return None
    if not os.path.isabs(path):
        path = os.path.normpath(os.path.join(repo, path))
    return os.path.abspath(path)


def journal_path(repo, spec="auto"):
    """`auto` ⇒ `<git-common-dir>/walgit/t-reservations.journal`；`none` ⇒ None；否则按路径。"""
    if not spec or spec.lower() in ("none", "off", "-"):
        return None
    if spec != "auto":
        return os.path.abspath(spec)
    common = git_common_dir(repo)
    if not common:
        return None
    return os.path.join(common, "walgit", "t-reservations.journal")


def journal_numbers(path):
    if not path or not os.path.exists(path):
        return set()
    nums = set()
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        for line in fh:
            m = JROW_RE.match(line)
            if m:
                nums.add(int(m.group(1)))
    return nums


def journal_append(path, num, card, worktree=None):
    """一行一认领；单次 write + fsync。返回写入的行（供回读校验）。"""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    if worktree is None:
        worktree = os.path.basename(os.path.abspath(ROOT))
    line = chr(9).join(["T-{0}".format(num), now_utc(), card, worktree]) + chr(10)
    with open(path, "a", encoding="utf-8", newline=chr(10)) as fh:
        fh.write(line)
        fh.flush()
        os.fsync(fh.fileno())
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        if not any(row.rstrip(chr(10)) == line.rstrip(chr(10)) for row in fh):
            raise RuntimeError("journal 回读失败：{0}".format(line.strip()))
    return line.strip()


# --------------------------------------------------------------------------- 远端新鲜度
def _run_git(repo, args, timeout=90):
    try:
        return subprocess.run(["git", "-C", repo] + args, capture_output=True, text=True,
                              encoding="utf-8", errors="replace", timeout=timeout)
    except (OSError, subprocess.SubprocessError) as exc:
        return None, str(exc)


def _rev_parse(repo, ref):
    proc = _run_git(repo, ["rev-parse", "--verify", "--quiet", ref])
    if isinstance(proc, tuple):
        return None
    return proc.stdout.strip() or None


def _show(repo, oid, rel):
    proc = _run_git(repo, ["show", "{0}:{1}".format(oid, rel)])
    if isinstance(proc, tuple) or proc.returncode != 0:
        return None
    return proc.stdout


def remote_view(repo, remote, timeout=90):
    """读远端**当前** master 的两份文件。

    ⇒ {"state": "fresh"|"unreachable"|"stale"|"none", "oid", "table", "whitelist", "detail"}
    `fresh` 的含义是「读的就是远端此刻的 master」：先 `ls-remote` 拿 oid，与本地 remote-tracking
    ref 不一致就 fetch，fetch 后再比对；对不上就是 `stale`（拒绝发号）。
    """
    if not remote or remote.lower() in ("none", "off", "-"):
        return {"state": "none", "oid": None, "table": None, "whitelist": None,
                "detail": "--remote none（只看本地）"}
    ls = _run_git(repo, ["ls-remote", "--exit-code", remote, "refs/heads/master"], timeout)
    if isinstance(ls, tuple):
        return {"state": "unreachable", "oid": None, "table": None, "whitelist": None,
                "detail": "ls-remote 无法执行：{0}".format(ls[1])}
    if ls.returncode != 0:
        return {"state": "unreachable", "oid": None, "table": None, "whitelist": None,
                "detail": "ls-remote rc={0}：{1}".format(
                    ls.returncode, (ls.stderr or "").strip()[:200])}
    parts = ls.stdout.split()
    if not parts:
        return {"state": "unreachable", "oid": None, "table": None, "whitelist": None,
                "detail": "ls-remote 输出为空"}
    oid = parts[0]
    tracking = "refs/remotes/{0}/master".format(remote)
    fetched = False
    local = _rev_parse(repo, tracking)
    if local != oid:
        for attempt in range(2):
            fr = _run_git(repo, ["fetch", "--quiet", "--no-tags", remote,
                                 "+refs/heads/master:{0}".format(tracking)], timeout)
            if not isinstance(fr, tuple) and fr.returncode == 0:
                fetched = True
                break
            time.sleep(0.1 * (attempt + 1))
        local = _rev_parse(repo, tracking)
    if local != oid:
        return {"state": "stale", "oid": oid, "table": None, "whitelist": None,
                "detail": "远端 master={0} 但本地 tracking ref={1}（fetch 失败/未生效）".format(
                    oid[:12], (local or "<none>")[:12])}
    return {"state": "fresh", "oid": oid, "fetched": fetched,
            "table": _show(repo, oid, TABLE_REL), "whitelist": _show(repo, oid, WHITELIST_REL),
            "detail": "master={0}{1}".format(oid[:12], "（已 fetch）" if fetched else "")}


# --------------------------------------------------------------------------- 互斥锁
class FileLock(object):
    """跨进程排他锁（OS 级 advisory lock，进程死亡自动释放）。

    锁文件按**锁目标绝对路径** hash 落在系统临时目录。锁目标 = 共享 journal（默认）——
    所有 worktree 的 journal 路径相同 ⇒ 同一把锁；无 journal 时退化为预约表路径。
    """

    def __init__(self, target, timeout=60.0):
        digest = hashlib.sha1(os.path.abspath(target).lower().encode("utf-8")).hexdigest()[:12]
        self.path = os.path.join(tempfile.gettempdir(),
                                 "openmir2-t-reserve-{0}.lock".format(digest))
        self.timeout = timeout
        self._fh = None

    def __enter__(self):
        self._fh = open(self.path, "a+b")
        deadline = time.time() + self.timeout
        while True:
            try:
                self._lock_once()
                return self
            except (IOError, OSError):
                if time.time() >= deadline:
                    self._fh.close()
                    self._fh = None
                    raise RuntimeError(
                        "取号锁超时 {0}s（锁文件 {1}）——并发取号过多或锁实现失效".format(
                            self.timeout, self.path))
                time.sleep(0.02)

    def _lock_once(self):
        if os.name == "nt":
            import msvcrt
            self._fh.seek(0)
            msvcrt.locking(self._fh.fileno(), msvcrt.LK_NBLCK, 1)
        else:
            import fcntl
            fcntl.flock(self._fh.fileno(), fcntl.LOCK_EX | fcntl.LOCK_NB)

    def __exit__(self, exc_type, exc, tb):
        if self._fh is None:
            return False
        try:
            if os.name == "nt":
                import msvcrt
                self._fh.seek(0)
                msvcrt.locking(self._fh.fileno(), msvcrt.LK_UNLCK, 1)
            else:
                import fcntl
                fcntl.flock(self._fh.fileno(), fcntl.LOCK_UN)
        finally:
            self._fh.close()
            self._fh = None
        return False


# --------------------------------------------------------------------------- 写盘
def append_rows(table_path, lines):
    """一次 write 追加（必要时补前导换行）+ fsync；不做任何格式化改写。"""
    if not lines:
        return
    prefix = ""
    if os.path.exists(table_path) and os.path.getsize(table_path) > 0:
        with open(table_path, "rb") as fh:
            fh.seek(-1, os.SEEK_END)
            if fh.read(1) != b"\n":
                prefix = "\n"
    blob = prefix + "".join(line + "\n" for line in lines)
    with open(table_path, "a", encoding="utf-8", newline="\n") as fh:
        fh.write(blob)
        fh.flush()
        os.fsync(fh.fileno())


def verify_appended(table_path, expected_lines):
    parsed = {format_row(r["num"], r["card"], r["ts"], r["status"])
              for r in parse_table(read_text(table_path))}
    missing = [ln for ln in expected_lines if ln not in parsed]
    if missing:
        raise RuntimeError("追加后回读失败（行不可解析）：{0}".format("; ".join(missing)))


def release_live_rows(table_path, card):
    """把该卡已有的「在用」行改写成「已释放」（只改这些行，其余字节不动）。⇒ 改到的行号。"""
    text = read_text(table_path)
    parts = text.split("\n")
    changed = []
    for i, line in enumerate(parts):
        m = ROW_ONE.match(line)
        if m and m.group(2).strip() == card and m.group(4) == LIVE:
            parts[i] = "| {0} | {1} | {2} | {3} |".format(
                m.group(1), m.group(2).strip(), m.group(3).strip(), RELEASED)
            changed.append(i + 1)
    if changed:
        tmp = table_path + ".release.tmp"
        with open(tmp, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("\n".join(parts))
            fh.flush()
            os.fsync(fh.fileno())
        os.replace(tmp, table_path)
    return changed


def format_row(num, card, ts, status):
    return "| T-{0} | {1} | {2} | {3} |".format(num, card, ts, status)


# --------------------------------------------------------------------------- 取号
def used_numbers(table_text, whitelist_text, extra=()):
    """已被占用的号：预约表任意状态（含已释放，号不回收）+ whitelist 现状 + journal + 远端。"""
    nums = {r["num"] for r in parse_table(table_text)}
    nums |= parse_whitelist_numbers(whitelist_text)
    for t_text, w_text in extra:
        if t_text:
            nums |= {r["num"] for r in parse_table(t_text)}
        if w_text:
            nums |= parse_whitelist_numbers(w_text)
    return nums


def next_free(table_text, whitelist_text, extra=()):
    return max(used_numbers(table_text, whitelist_text, extra), default=0) + 1


def allocate(card, table_path=DEFAULT_TABLE, whitelist_path=DEFAULT_WHITELIST,
             repo=ROOT, remote="walgit", journal="auto", force_new=False, dry_run=False,
             allow_stale=False, log=None):
    """⇒ {"num","token","appended","journal","remote_state"}。同 card 已有「在用」⇒ 幂等返回。"""
    log = log or (lambda s: print(s, file=sys.stderr))
    if not card or "|" in card or "\n" in card or "\t" in card:
        raise ValueError("--card 必须是非空单行标识（不得含 `|` 或制表符）")
    jpath = journal_path(repo, journal)

    # 锁外先做一次廉价幂等判断（不碰远端）
    table_text = read_text(table_path) if os.path.exists(table_path) else ""
    whitelist_text = read_text(whitelist_path) if os.path.exists(whitelist_path) else ""
    if not whitelist_text:
        raise RuntimeError("whitelist 不存在或为空：{0}".format(whitelist_path))
    rows, fence_ok = scan_lines(table_text)
    wl_rows, wl_fence_ok = scan_lines(whitelist_text)
    if not fence_ok:
        raise RuntimeError("预约表围栏 ``` 不成对（无法判定哪些行是登记行）⇒ 拒绝取号")
    if not wl_fence_ok:
        raise RuntimeError("whitelist 围栏 ``` 不成对（会漏掉围栏后的号）⇒ 拒绝取号")
    existing = [r for r in parse_table(table_text) if r["card"] == card and r["status"] == LIVE]
    if existing and not force_new:
        log("NOTE card `{0}` 已有「在用」认领 {1}（第 {2} 行）⇒ 幂等返回，未追加新行"
            "（要换号加 --force-new）".format(card, existing[0]["token"], existing[0]["line"]))
        return {"num": existing[0]["num"], "token": existing[0]["token"], "appended": False,
                "journal": "skipped", "remote_state": "skipped"}

    lock_target = jpath or table_path
    with FileLock(lock_target):
        # 锁内重读，保证读-改-写串行
        table_text = read_text(table_path) if os.path.exists(table_path) else ""
        whitelist_text = read_text(whitelist_path)
        rows, fence_ok = scan_lines(table_text)
        wl_rows, wl_fence_ok = scan_lines(whitelist_text)
        if not fence_ok or not wl_fence_ok:
            raise RuntimeError("锁内复核：围栏 ``` 不成对（预约表={0} whitelist={1}）⇒ 拒绝取号"
                               .format(fence_ok, wl_fence_ok))
        existing = [r for r in parse_table(table_text)
                    if r["card"] == card and r["status"] == LIVE]
        if existing and not force_new:
            log("NOTE card `{0}` 已有「在用」认领 {1} ⇒ 幂等返回".format(card,
                                                                        existing[0]["token"]))
            return {"num": existing[0]["num"], "token": existing[0]["token"], "appended": False,
                    "journal": "skipped", "remote_state": "skipped"}

        view = remote_view(repo, remote)
        state = view["state"]
        if state == "unreachable":
            msg = ("远端快照不可用：{0}。拒绝发号（fail-closed）——"
                   "确知无远端时用 --remote none，或显式 --allow-stale 接受过期风险"
                   .format(view["detail"]))
            if not allow_stale:
                raise RuntimeError(msg)
            log("WARNING STALE-REMOTE {0} ⇒ --allow-stale 放行；同机由共享 journal 兜底"
                .format(msg))
        elif state == "stale":
            msg = "远端快照过期：{0}".format(view["detail"])
            if not allow_stale:
                raise RuntimeError(msg + " ⇒ 拒绝发号（fail-closed）")
            log("WARNING STALE-REMOTE {0} ⇒ --allow-stale 放行".format(msg))
        elif state == "fresh":
            log("NOTE 远端 master={0}{1}".format(view["oid"][:12],
                                                 "（已 fetch 更新）" if view.get("fetched") else ""))

        jnums = journal_numbers(jpath) if jpath else set()
        extra = [(view.get("table"), view.get("whitelist"))] if state == "fresh" else []
        num = next_free(table_text, whitelist_text, extra)
        if jnums:
            num = max(num, max(jnums) + 1)
        if dry_run:
            log("DRY-RUN 下一个空闲号 = T-{0}（未写任何文件；journal={1} remote={2}）".format(
                num, "on" if jpath else "off", state))
            return {"num": num, "token": "T-{0}".format(num), "appended": False,
                    "journal": "dry-run", "remote_state": state}

        released = release_live_rows(table_path, card) if force_new else []
        if released:
            log("NOTE --force-new：把同卡旧的「在用」行（L{0}）改成「已释放」".format(
                ",".join(str(x) for x in released)))
        line = format_row(num, card, now_utc(), LIVE)
        if jpath:
            journal_append(jpath, num, card)   # 先占号：崩在写表之前也只会浪费号，不会重号
        append_rows(table_path, [line])
        verify_appended(table_path, [line])
    return {"num": num, "token": "T-{0}".format(num), "appended": True,
            "journal": "on" if jpath else "off", "remote_state": state}


def bootstrap(table_path=DEFAULT_TABLE, whitelist_path=DEFAULT_WHITELIST,
              dry_run=False, log=None):
    """把 whitelist.md 里**已有但预约表没有**的号批量登记为「已落地」（不追溯、不重排）。"""
    log = log or (lambda s: print(s, file=sys.stderr))
    with FileLock(table_path):
        table_text = read_text(table_path) if os.path.exists(table_path) else ""
        whitelist_text = read_text(whitelist_path)
        _rows, fence_ok = scan_lines(table_text)
        _wrows, wl_fence_ok = scan_lines(whitelist_text)
        if not fence_ok or not wl_fence_ok:
            raise RuntimeError("围栏 ``` 不成对 ⇒ 拒绝 bootstrap（会漏号）")
        known = {r["num"] for r in parse_table(table_text)}
        missing = sorted(n for n in parse_whitelist_numbers(whitelist_text) if n not in known)
        lines = [format_row(n, HISTORY_CARD, HISTORY_TS, LANDED) for n in missing]
        if lines and not dry_run:
            append_rows(table_path, lines)
            verify_appended(table_path, lines)
    log("{0} 批量补登 {1} 条历史登记为「已落地」{2}".format(
        "DRY-RUN" if dry_run else "BOOTSTRAP", len(lines), "" if dry_run else "（已写盘）"))
    return lines


# --------------------------------------------------------------------------- 自检
def _git(args, cwd=None, check=True):
    proc = subprocess.run(["git"] + args, cwd=cwd, capture_output=True, text=True,
                          encoding="utf-8", errors="replace", timeout=120)
    if check and proc.returncode != 0:
        raise RuntimeError("git {0} 失败：{1}".format(" ".join(args), proc.stderr[:300]))
    return proc


def _make_remote_fixture(tmp):
    """造 remote.git + 两个克隆：wtB 先 push 一条 T-196 认领，wtA 保持未 fetch 的旧视图。"""
    ident = ["-c", "user.name=selftest", "-c", "user.email=selftest@example.invalid"]
    nl = chr(10)
    remote = os.path.join(tmp, "remote.git")
    _git(["init", "-q", "--bare", remote])
    _git(["-C", remote, "symbolic-ref", "HEAD", "refs/heads/master"])
    seed = os.path.join(tmp, "seed")
    os.makedirs(seed, exist_ok=True)
    _git(["init", "-q", seed])
    _git(["-C", seed, "symbolic-ref", "HEAD", "refs/heads/master"])
    _git(["-C", seed, "remote", "add", "origin", remote])
    os.makedirs(os.path.join(seed, "mir2-rs", "tests", "parity"), exist_ok=True)
    table = os.path.join(seed, TABLE_REL)
    wl = os.path.join(seed, WHITELIST_REL)
    header = nl.join(["| 号 | 卡 id | 认领时间 | 状态 |", "| --- | --- | --- | --- |"]) + nl
    with open(table, "w", encoding="utf-8", newline=nl) as fh:
        fh.write(header + "| T-195 | seed | 2026-01-01T00:00:00Z | 已落地 |" + nl)
    with open(wl, "w", encoding="utf-8", newline=nl) as fh:
        fh.write("| T-195 | seed |" + nl)
    _git(["-C", seed] + ident + ["add", "-A"])
    _git(["-C", seed] + ident + ["commit", "-q", "-m", "seed"])
    _git(["-C", seed, "push", "-q", "origin", "HEAD:master"])
    wt_a = os.path.join(tmp, "wtA")
    wt_b = os.path.join(tmp, "wtB")
    _git(["clone", "-q", remote, wt_a])
    _git(["clone", "-q", remote, wt_b])
    # wtB 在远端认领 T-196（wtA 不知道，未 fetch）
    with open(os.path.join(wt_b, TABLE_REL), "a", encoding="utf-8", newline=nl) as fh:
        fh.write("| T-196 | lane/B | 2026-01-01T00:00:00Z | 在用 |" + nl)
    _git(["-C", wt_b] + ident + ["add", "-A"])
    _git(["-C", wt_b] + ident + ["commit", "-q", "-m", "lane B claims T-196"])
    _git(["-C", wt_b, "push", "-q", "origin", "HEAD:master"])
    return remote, wt_a, wt_b


def cmd_selftest(_args):
    ok = total = 0

    def expect(name, cond, detail=""):
        nonlocal ok, total
        total += 1
        if cond:
            ok += 1
            print("  SELFTEST ok  {0}".format(name))
        else:
            print("  SELFTEST FAIL {0}{1}".format(name, ("  <- " + detail) if detail else ""))

    tmp = tempfile.mkdtemp(prefix="nextt-selftest-")
    try:
        table = os.path.join(tmp, "T-RESERVATIONS.md")
        wl = os.path.join(tmp, "whitelist.md")
        header = "# 预约表\n\n| 号 | 卡 id | 认领时间 | 状态 |\n| --- | --- | --- | --- |\n"
        with open(table, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(header + "| T-1 | a | 2026-01-01T00:00:00Z | 已落地 |\n"
                             "| T-2 | b | 2026-01-01T00:00:00Z | 已落地 |\n"
                             "| T-3 | c | 2026-01-01T00:00:00Z | 已释放 |\n")
        with open(wl, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("| T-1 | x |\n| T-2 | y |\n| T-9 | z |\n")
        quiet = lambda s: None

        t_text = read_text(table)
        w_text = read_text(wl)
        expect("解析预约表 3 行", len(parse_table(t_text)) == 3)
        expect("解析 whitelist 号 {1,2,9}", parse_whitelist_numbers(w_text) == {1, 2, 9})
        expect("号 = max(表, whitelist)+1 = 10（whitelist 的 T-9 也计入）",
               next_free(t_text, w_text) == 10)
        expect("已释放的号不回收", 3 in used_numbers(t_text, w_text))

        r1 = allocate("card/one", table_path=table, whitelist_path=wl, remote="none",
                      journal="none", log=quiet)
        expect("第一次取号 = 10", r1["num"] == 10 and r1["appended"])
        r2 = allocate("card/two", table_path=table, whitelist_path=wl, remote="none",
                      journal="none", log=quiet)
        expect("第二次取号 = 11（不同号）", r2["num"] == 11 and r2["appended"])
        r3 = allocate("card/one", table_path=table, whitelist_path=wl, remote="none",
                      journal="none", log=quiet)
        expect("同 card 重复取号幂等（返回 10、不再追加）",
               r3["num"] == 10 and r3["appended"] is False)
        rows = parse_table(read_text(table))
        expect("预约表实际落盘 2 条新认领（+3 条历史）", len(rows) == 5)

        # ---- N-1：--force-new 必须把旧「在用」行改「已释放」，不得同卡双「在用」
        rf = allocate("card/one", table_path=table, whitelist_path=wl, remote="none",
                      journal="none", force_new=True, log=quiet)
        rows = parse_table(read_text(table))
        live_one = [r for r in rows if r["card"] == "card/one" and r["status"] == LIVE]
        rel_one = [r for r in rows if r["card"] == "card/one" and r["status"] == RELEASED]
        expect("--force-new 取新号（T-12）", rf["num"] == 12 and rf["appended"])
        expect("--force-new 后同卡只剩 1 个「在用」、旧的变「已释放」",
               len(live_one) == 1 and live_one[0]["num"] == 12 and len(rel_one) == 1,
               str([(r["num"], r["status"]) for r in rows]))

        # ---- I-3：围栏不成对 ⇒ 拒绝取号（fail-closed），不得静默跳过
        bad_table = os.path.join(tmp, "bad-fence.md")
        with open(bad_table, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(header + "| T-1 | a | 2026-01-01T00:00:00Z | 已落地 |\n```\n")
        try:
            allocate("card/bad", table_path=bad_table, whitelist_path=wl, remote="none",
                     journal="none", log=quiet)
            expect("预约表围栏不成对 ⇒ 拒绝取号（rc≠0）", False, "竟然发号了")
        except RuntimeError as exc:
            expect("预约表围栏不成对 ⇒ 拒绝取号（rc≠0）", "围栏" in str(exc), str(exc))
        bad_wl = os.path.join(tmp, "bad-wl.md")
        with open(bad_wl, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("| T-1 | x |\n```\n")
        try:
            allocate("card/bad", table_path=table, whitelist_path=bad_wl, remote="none",
                     journal="none", log=quiet)
            expect("whitelist 围栏不成对 ⇒ 拒绝取号（rc≠0）", False, "竟然发号了")
        except RuntimeError as exc:
            expect("whitelist 围栏不成对 ⇒ 拒绝取号（rc≠0）", "围栏" in str(exc), str(exc))

        # ---- I-1：跨 worktree 共享 journal 真互斥（两条表 + 一份 journal，模拟两个 worktree）
        wt_a_table = os.path.join(tmp, "wtA.md")
        wt_b_table = os.path.join(tmp, "wtB.md")
        shared = os.path.join(tmp, "shared.journal")
        for p in (wt_a_table, wt_b_table):
            with open(p, "w", encoding="utf-8", newline="\n") as fh:
                fh.write(header + "| T-195 | seed | 2026-01-01T00:00:00Z | 已落地 |\n")
        ra = allocate("lane/a", table_path=wt_a_table, whitelist_path=wl, remote="none",
                      journal=shared, log=quiet)
        rb = allocate("lane/b", table_path=wt_b_table, whitelist_path=wl, remote="none",
                      journal=shared, log=quiet)
        expect("两个 worktree 各自取号 = 196 / 197（共享 journal 真互斥）",
               ra["num"] == 196 and rb["num"] == 197, str((ra["num"], rb["num"])))
        expect("journal 记录 2 条认领", len(journal_numbers(shared)) == 2)
        expect("两张表各只有自己的号",
               [r["num"] for r in parse_table(read_text(wt_a_table))][-1] == 196
               and [r["num"] for r in parse_table(read_text(wt_b_table))][-1] == 197)

        # ---- I-4：远端新鲜度（不 fetch 也不许发旧号；远端不可达 ⇒ fail-closed）
        remote, wt_a, wt_b = _make_remote_fixture(tmp)
        res = allocate("lane/clone", table_path=os.path.join(wt_a, TABLE_REL),
                       whitelist_path=os.path.join(wt_a, WHITELIST_REL),
                       repo=wt_a, remote="origin", journal="none", log=quiet)
        expect("未 fetch 的克隆也能看到远端 T-196 ⇒ 发 T-197（不撞号）",
               res["num"] == 197 and res["remote_state"] == "fresh", str(res))
        try:
            allocate("lane/dead", table_path=os.path.join(wt_a, TABLE_REL),
                     whitelist_path=os.path.join(wt_a, WHITELIST_REL), repo=wt_a,
                     remote="file:///nonexistent/remote.git", journal="none", log=quiet)
            expect("远端不可达 ⇒ 拒绝发号（fail-closed，rc≠0）", False, "竟然发号了")
        except RuntimeError as exc:
            expect("远端不可达 ⇒ 拒绝发号（fail-closed，rc≠0）", "拒绝发号" in str(exc),
                   str(exc)[:120])
        before = read_text(os.path.join(wt_a, TABLE_REL))
        ok_stale = allocate("lane/stale", table_path=os.path.join(wt_a, TABLE_REL),
                            whitelist_path=os.path.join(wt_a, WHITELIST_REL), repo=wt_a,
                            remote="file:///nonexistent/remote.git", journal="none",
                            allow_stale=True, log=quiet)
        expect("--allow-stale 显式放行时仍能取号（告警不静默）",
               ok_stale["num"] == 198 and ok_stale["appended"])
        _ = remote, wt_b, before

        # ---- 并发竞速：4 个进程同时取号，必须拿到 4 个互不相同的号
        race_table = os.path.join(tmp, "race.md")
        race_wl = os.path.join(tmp, "race-wl.md")
        race_j = os.path.join(tmp, "race.journal")
        with open(race_table, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(header)
        with open(race_wl, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("| T-1 | x |\n")
        procs = []
        for i in range(4):
            procs.append(subprocess.Popen(
                [sys.executable, "-I", os.path.abspath(__file__), "--card", "race/{0}".format(i),
                 "--table", race_table, "--whitelist", race_wl, "--remote", "none",
                 "--journal", race_j],
                stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True,
                encoding="utf-8", errors="replace"))
        outs = [p.communicate(timeout=120)[0].strip() for p in procs]
        nums = [o for o in outs if o.startswith("T-")]
        expect("并发 4 进程：4 个进程都成功输出号", len(nums) == 4, str(outs))
        expect("并发 4 进程：号互不相同（锁生效）", len(set(nums)) == 4, str(nums))
        expect("并发后预约表恰好 4 行", len(parse_table(read_text(race_table))) == 4)

        # ---- bootstrap：whitelist 有、预约表没有 ⇒ 补「已落地」，且不重排已有行
        boot_table = os.path.join(tmp, "boot.md")
        boot_wl = os.path.join(tmp, "boot-wl.md")
        with open(boot_table, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(header + "| T-5 | old | 2025-01-01T00:00:00Z | 已落地 |\n")
        with open(boot_wl, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("| T-5 | a |\n| T-6 | b |\n| T-7 | c |\n| T-7 | c2 |\n")
        lines = bootstrap(table_path=boot_table, whitelist_path=boot_wl, log=quiet)
        brow = parse_table(read_text(boot_table))
        expect("bootstrap 只补缺的号（2 条，去重）", len(lines) == 2 and len(brow) == 3)
        expect("bootstrap 行状态 = 已落地",
               all(r["status"] == LANDED for r in brow if r["card"] == HISTORY_CARD))
        expect("bootstrap 不动已有行（T-5 保持原卡 id / 时间）",
               brow[0]["card"] == "old" and brow[0]["ts"] == "2025-01-01T00:00:00Z")

        print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
        return 0 if ok == total else 1
    finally:
        shutil.rmtree(tmp, ignore_errors=True)          # N-4：不泄临时目录


def main(argv=None):
    p = argparse.ArgumentParser(
        description="T-* 编号预约取号器（唯一取号入口；原子追加认领行）")
    p.add_argument("--card", default=None, help="卡 id（写进预约表第二列）")
    p.add_argument("--table", default=DEFAULT_TABLE)
    p.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    p.add_argument("--repo", default=ROOT, help="用于 journal / 远端快照的仓库路径")
    p.add_argument("--remote", default="walgit",
                   help="远端 master（默认 walgit；none = 只看本地）")
    p.add_argument("--journal", default="auto",
                   help="共享 journal：auto（<git-common-dir>/walgit/t-reservations.journal）| "
                        "none | 显式路径")
    p.add_argument("--force-new", action="store_true",
                   help="换号：先把本卡旧的「在用」行改「已释放」，再取新号")
    p.add_argument("--dry-run", action="store_true", help="只算号，不写任何文件")
    p.add_argument("--allow-stale", action="store_true",
                   help="远端不可达/过期时仍发号（大声告警；默认 fail-closed 拒绝）")
    p.add_argument("--bootstrap", action="store_true",
                   help="把 whitelist.md 已有而预约表缺的号批量登记为「已落地」"
                        "（历史迁移，一次性）")
    sub = p.add_subparsers(dest="cmd")
    sub.add_parser("selftest", help="合成表 + 跨 worktree journal + 远端新鲜度自检") \
        .set_defaults(func=cmd_selftest)
    args = p.parse_args(argv)
    if getattr(args, "func", None):
        return args.func(args)
    try:
        if args.bootstrap:
            bootstrap(table_path=args.table, whitelist_path=args.whitelist,
                      dry_run=args.dry_run)
            return 0
        if not args.card:
            p.error("需要 --card <卡 id>（或用 --bootstrap / selftest）")
        res = allocate(args.card, table_path=args.table, whitelist_path=args.whitelist,
                       repo=args.repo, remote=args.remote, journal=args.journal,
                       force_new=args.force_new, dry_run=args.dry_run,
                       allow_stale=args.allow_stale)
    except (RuntimeError, ValueError) as exc:
        print("NEXT_T_RED {0}".format(exc), file=sys.stderr)
        return 1
    print(res["token"])
    print("NEXT_T {0} card={1} appended={2} journal={3} remote={4}".format(
        res["token"], args.card, res["appended"], res["journal"], res["remote_state"]),
        file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
