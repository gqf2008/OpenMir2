#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""`T-*` 编号**预约取号器**（卡 `openmir2-t-number-reservation`，tier=L）。

## 解决什么

`mir2-rs/tests/parity/whitelist.md` 的 `T-<n>` 是**全仓共用的一个线性编号空间**，谁先写谁占。
2026-10-23 一天撞号 4 次（T-174..T-179 vs T-174..T-183、T-4 重复、T-6 同号不同义、
T-194 被 menu-ok 与 gm-admin 同时认领），根因是每条并行车道都在**开工时**自己 grep 一次
「当前最大号」——并行时这个 grep 必然读到旧值，而现有门（`m3_whitelist_registry_audit.py`）
只做**事后**检测。本件把取号改成**事前预约 + 事后校验**。

## 用法（唯一取号入口）

    python -I tools/whitelist/next_t.py --card <卡 id>            # 取号并原子追加认领行
    python -I tools/whitelist/next_t.py --card <id> --dry-run     # 只看号，不写
    python -I tools/whitelist/next_t.py --bootstrap               # 历史登记批量补「已落地」
    python -I tools/whitelist/next_t.py selftest                  # 自检（含并发竞速）

stdout 只输出一行 `T-<n>`（便于 `N=$(python -I ... --card x)`），人读信息走 stderr。

## 原子性怎么保证（四层）

1. **跨进程互斥锁**：`msvcrt.locking`（Windows）/ `fcntl.flock`（POSIX）锁住一个由
   **预约表绝对路径** hash 出来的锁文件（放在系统临时目录）。锁在「读表 -> 算号 -> 追加」
   全程持有；进程崩溃由 OS 自动释放，不会留死锁。
2. **单次追加 + fsync**：认领行是**一次** `write`（一行、以换行结尾、`newline="\\n"` 不翻译），
   随后 `flush` + `os.fsync`，不存在「写了一半」的中间态。
3. **写后回读校验**：追加完立刻重读，确认该行存在且能被解析；解析不到 ⇒ 非零退出并报错。
4. **跨 worktree 新鲜度**：号 = `max(本地预约表, 本地 whitelist, walgit/master 预约表,
   walgit/master whitelist) + 1`。并行车道各自在自己的 worktree 里也拿不到同一个号
   （`--remote` 默认 `walgit`，取不到时降级为本地并在 stderr 明说，不静默）。
   **注意**：跨 worktree 互斥依赖远端快照，不是分布式锁；最终兜底是门禁
   `t_reservation_gate.py`（合并前必跑）。

号只增不减：`已释放` 的号**不再回收**（历史 commit / 文档里可能已有引用，回收 = 制造歧义）。

## 幂等

同一 `--card` 重复调用 ⇒ 直接返回它**已有的**「在用」号，不重复追加（防止误跑烧号）。
要换号用 `--force-new`；不同 `--card` 的重复/并发调用必得不同号。
"""
from __future__ import annotations

import argparse
import datetime as _dt
import hashlib
import os
import re
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
# 表头 / 分隔行天然不匹配（第 4 列不是合法状态）；``` 围栏代码块里的示例行也不算登记行。
ROW_ONE = re.compile(
    r"^\|\s*(T-\d+)\s*\|\s*([^|]*?)\s*\|\s*([^|]*?)\s*\|\s*(在用|已落地|已释放)\s*\|\s*$")
WROW_ONE = re.compile(r"^\|\s*(T-\d+)\s*\|")
NUM_RE = re.compile(r"^T-(\d+)$")

HISTORY_CARD = "historical-pre-reservation"
HISTORY_TS = "2026-10-23T00:00:00Z"


# --------------------------------------------------------------------------- 解析
def _lines(text):
    """逐行产出 (line_text, line_no)，跳过 ``` 围栏代码块（示例行不是登记行）。"""
    in_fence = False
    for idx, line in enumerate(text.splitlines(), 1):
        if line.lstrip().startswith("```"):
            in_fence = not in_fence
            continue
        if in_fence:
            continue
        yield line, idx


def parse_table(text):
    """⇒ [{num:int, token:str, card:str, ts:str, status:str, line:int}]（文件序）。"""
    out = []
    for line, no in _lines(text):
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
    """whitelist.md 里出现过的所有 `T-<n>` 号（去重）。"""
    return {int(NUM_RE.match(m.group(1)).group(1))
            for line, _no in _lines(text) for m in [WROW_ONE.match(line)] if m}


def read_text(path):
    with open(path, "r", encoding="utf-8", errors="replace", newline="") as fh:
        return fh.read()


def now_utc():
    return _dt.datetime.now(_dt.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


# --------------------------------------------------------------------------- 远端快照
def git_show(repo, ref, rel, timeout=30):
    """读 `ref:rel` 的文本；取不到（无远端 / 无该文件 / git 不在 PATH）⇒ None。"""
    try:
        proc = subprocess.run(
            ["git", "-C", repo, "show", "{0}:{1}".format(ref, rel)],
            capture_output=True, text=True, encoding="utf-8", errors="replace",
            timeout=timeout,
        )
    except (OSError, subprocess.SubprocessError):
        return None
    return proc.stdout if proc.returncode == 0 else None


def remote_snapshot(repo, remote):
    """⇒ (table_text|None, whitelist_text|None)。`--remote none` 跳过。"""
    if not remote or remote.lower() in ("none", "off", "-"):
        return None, None
    ref = "{0}/master".format(remote)
    return git_show(repo, ref, TABLE_REL), git_show(repo, ref, WHITELIST_REL)


# --------------------------------------------------------------------------- 互斥锁
class FileLock(object):
    """跨进程排他锁（OS 级 advisory lock，进程死亡自动释放）。

    锁文件按**预约表绝对路径** hash 落在系统临时目录：同一张表的所有取号进程共用一把，
    不同表互不干扰；锁文件本身不进仓库（`git status` 永远干净）。
    """

    def __init__(self, table_path, timeout=30.0):
        real = os.path.abspath(table_path)
        digest = hashlib.sha1(real.lower().encode("utf-8")).hexdigest()[:12]
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


# --------------------------------------------------------------------------- 取号
def used_numbers(table_text, whitelist_text, extra=()):
    """已被占用的号：预约表任意状态（含已释放，号不回收）+ whitelist 现状 + 远端快照。"""
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


def format_row(num, card, ts, status):
    return "| T-{0} | {1} | {2} | {3} |".format(num, card, ts, status)


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
    """写后回读：每一行都必须能被解析出来（防「写了半行」/编码错）。"""
    text = read_text(table_path)
    parsed = {format_row(r["num"], r["card"], r["ts"], r["status"]) for r in parse_table(text)}
    missing = [ln for ln in expected_lines if ln not in parsed]
    if missing:
        raise RuntimeError("追加后回读失败（行不可解析）：{0}".format("; ".join(missing)))


def allocate(card, table_path=DEFAULT_TABLE, whitelist_path=DEFAULT_WHITELIST,
             repo=ROOT, remote="walgit", force_new=False, dry_run=False, log=None):
    """⇒ {"num", "token", "appended":bool, "remote":bool}。同 card 已有「在用」⇒ 幂等返回。"""
    log = log or (lambda s: print(s, file=sys.stderr))
    if not card or "|" in card or "\n" in card:
        raise ValueError("--card 必须是非空单行标识（不得含 `|`）")
    table_text = read_text(table_path) if os.path.exists(table_path) else ""
    whitelist_text = read_text(whitelist_path) if os.path.exists(whitelist_path) else ""
    if not whitelist_text:
        raise RuntimeError("whitelist 不存在或为空：{0}".format(whitelist_path))

    existing = [r for r in parse_table(table_text) if r["card"] == card and r["status"] == LIVE]
    if existing and not force_new:
        log("NOTE card `{0}` 已有「在用」认领 {1}（第 {2} 行）⇒ 幂等返回，未追加新行"
            "（要换号加 --force-new）".format(card, existing[0]["token"], existing[0]["line"]))
        return {"num": existing[0]["num"], "token": existing[0]["token"],
                "appended": False, "remote": False}
    if dry_run:
        r_text, w_text = remote_snapshot(repo, remote)
        num = next_free(table_text, whitelist_text, [(r_text, w_text)])
        log("DRY-RUN 下一个空闲号 = T-{0}（未写预约表）".format(num))
        return {"num": num, "token": "T-{0}".format(num), "appended": False,
                "remote": r_text is not None}

    # 锁内重新读表 -> 算号 -> 追加（读-改-写整段串行）
    with FileLock(table_path):
        table_text = read_text(table_path) if os.path.exists(table_path) else ""
        whitelist_text = read_text(whitelist_path)
        r_text, w_text = remote_snapshot(repo, remote)
        if remote and remote.lower() not in ("none", "off", "-") and r_text is None:
            log("NOTE 读不到 {0}/master 的预约表（首次落地 / 离线）⇒ 仅按本地取号；"
                "跨 worktree 兜底靠合并前门禁".format(remote))
        existing = [r for r in parse_table(table_text) if r["card"] == card and r["status"] == LIVE]
        if existing and not force_new:
            log("NOTE card `{0}` 已有「在用」认领 {1} ⇒ 幂等返回".format(card,
                                                                        existing[0]["token"]))
            return {"num": existing[0]["num"], "token": existing[0]["token"],
                    "appended": False, "remote": r_text is not None}
        num = next_free(table_text, whitelist_text, [(r_text, w_text)])
        line = format_row(num, card, now_utc(), LIVE)
        append_rows(table_path, [line])
        verify_appended(table_path, [line])
    return {"num": num, "token": "T-{0}".format(num), "appended": True,
            "remote": r_text is not None}


def bootstrap(table_path=DEFAULT_TABLE, whitelist_path=DEFAULT_WHITELIST,
              dry_run=False, log=None):
    """把 whitelist.md 里**已有但预约表没有**的号批量登记为「已落地」（不追溯、不重排）。"""
    log = log or (lambda s: print(s, file=sys.stderr))
    with FileLock(table_path):
        table_text = read_text(table_path) if os.path.exists(table_path) else ""
        whitelist_text = read_text(whitelist_path)
        known = {r["num"] for r in parse_table(table_text)}
        # 同一号在 whitelist 里出现多次也只登记一行
        missing = sorted(n for n in parse_whitelist_numbers(whitelist_text) if n not in known)
        lines = [format_row(n, HISTORY_CARD, HISTORY_TS, LANDED) for n in missing]
        if lines and not dry_run:
            append_rows(table_path, lines)
            verify_appended(table_path, lines)
    log("{0} 批量补登 {1} 条历史登记为「已落地」{2}".format(
        "DRY-RUN" if dry_run else "BOOTSTRAP", len(lines), "" if dry_run else "（已写盘）"))
    return lines


# --------------------------------------------------------------------------- 自检
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
    table = os.path.join(tmp, "T-RESERVATIONS.md")
    wl = os.path.join(tmp, "whitelist.md")
    header = ("# 预约表\n\n| 号 | 卡 id | 认领时间 | 状态 |\n| --- | --- | --- | --- |\n")
    with open(table, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(header + "| T-1 | a | 2026-01-01T00:00:00Z | 已落地 |\n"
                         "| T-2 | b | 2026-01-01T00:00:00Z | 已落地 |\n"
                         "| T-3 | c | 2026-01-01T00:00:00Z | 已释放 |\n")
    with open(wl, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("| T-1 | x |\n| T-2 | y |\n| T-9 | z |\n")

    t_text = read_text(table)
    w_text = read_text(wl)
    expect("解析预约表 3 行", len(parse_table(t_text)) == 3, str(parse_table(t_text)))
    expect("解析 whitelist 号 {1,2,9}", parse_whitelist_numbers(w_text) == {1, 2, 9})
    expect("号 = max(表, whitelist)+1 = 10（whitelist 的 T-9 也计入）",
           next_free(t_text, w_text) == 10, str(next_free(t_text, w_text)))
    expect("已释放的号不回收（T-3 在 max 之下，不会分到 3）",
           3 in used_numbers(t_text, w_text))

    r1 = allocate("card/one", table_path=table, whitelist_path=wl, remote="none",
                  log=lambda s: None)
    expect("第一次取号 = 10", r1["num"] == 10 and r1["appended"], str(r1))
    r2 = allocate("card/two", table_path=table, whitelist_path=wl, remote="none",
                  log=lambda s: None)
    expect("第二次取号 = 11（不同号）", r2["num"] == 11 and r2["appended"], str(r2))
    r3 = allocate("card/one", table_path=table, whitelist_path=wl, remote="none",
                  log=lambda s: None)
    expect("同 card 重复取号幂等（返回 10、不再追加）",
           r3["num"] == 10 and r3["appended"] is False, str(r3))
    rows = parse_table(read_text(table))
    expect("预约表实际落盘 2 条新认领（+3 条历史）", len(rows) == 5, str(len(rows)))
    expect("落盘行的状态是「在用」",
           all(r["status"] == LIVE for r in rows if r["card"] in ("card/one", "card/two")))

    # ---- 并发竞速：4 个进程同时取号，必须拿到 4 个互不相同的号
    race_table = os.path.join(tmp, "race.md")
    race_wl = os.path.join(tmp, "race-wl.md")
    with open(race_table, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(header)
    with open(race_wl, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("| T-1 | x |\n")
    procs = []
    for i in range(4):
        procs.append(subprocess.Popen(
            [sys.executable, "-I", os.path.abspath(__file__), "--card", "race/{0}".format(i),
             "--table", race_table, "--whitelist", race_wl, "--remote", "none"],
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
    lines = bootstrap(table_path=boot_table, whitelist_path=boot_wl, log=lambda s: None)
    brow = parse_table(read_text(boot_table))
    expect("bootstrap 只补缺的号（2 条，去重）",
           len(lines) == 2 and len(brow) == 3, str(len(lines)))
    expect("bootstrap 行状态 = 已落地",
           all(r["status"] == LANDED for r in brow if r["card"] == HISTORY_CARD))
    expect("bootstrap 不动已有行（T-5 保持原卡 id / 时间）",
           brow[0]["card"] == "old" and brow[0]["ts"] == "2025-01-01T00:00:00Z")

    print("SELFTEST {0} {1}/{2}".format("GREEN" if ok == total else "RED", ok, total))
    return 0 if ok == total else 1


def main(argv=None):
    p = argparse.ArgumentParser(
        description="T-* 编号预约取号器（唯一取号入口；原子追加认领行）")
    p.add_argument("--card", default=None, help="卡 id（写进预约表第二列）")
    p.add_argument("--table", default=DEFAULT_TABLE)
    p.add_argument("--whitelist", default=DEFAULT_WHITELIST)
    p.add_argument("--repo", default=ROOT, help="用于读远端快照的仓库路径")
    p.add_argument("--remote", default="walgit",
                   help="读该远端的 master 快照做并集（默认 walgit；none = 只看本地）")
    p.add_argument("--force-new", action="store_true", help="即使本卡已有「在用」也另取新号")
    p.add_argument("--dry-run", action="store_true", help="只算号，不写预约表")
    p.add_argument("--bootstrap", action="store_true",
                   help="把 whitelist.md 已有而预约表缺的号批量登记为「已落地」"
                        "（历史迁移，一次性）")
    sub = p.add_subparsers(dest="cmd")
    sub.add_parser("selftest", help="合成表 + 并发竞速自检").set_defaults(func=cmd_selftest)
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
                       repo=args.repo, remote=args.remote, force_new=args.force_new,
                       dry_run=args.dry_run)
    except (RuntimeError, ValueError) as exc:
        print("NEXT_T_RED {0}".format(exc), file=sys.stderr)
        return 1
    print(res["token"])
    print("NEXT_T {0} card={1} appended={2} remote={3}".format(
        res["token"], args.card, res["appended"], res["remote"]), file=sys.stderr)
    return 0


if __name__ == "__main__":
    sys.exit(main())
