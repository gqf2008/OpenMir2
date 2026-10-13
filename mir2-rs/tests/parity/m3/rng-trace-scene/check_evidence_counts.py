#!/usr/bin/env python3
"""核对证据报告「证据头行」里声明的件数/verify/eolcheck 计数是否与**真实证据包**一致。

背景（同族坑复发）：本仓库的交付报告常在「证据：<dir>（N 件 + SHA256SUMS；`verify --check-extra`
GREEN N/N，`eolcheck` GREEN M/M）」这一行**硬编码**计数；而证据包在同一个片里还会继续增删文件
⇒ 交付时忘记回改，头行立刻变成 doc-vs-事实（已复发多次：reviewer-5 F-1 `82/82`→实际 `81/82`、
reviewer-4 O-2、reviewer-6 在 `72c9258dc` 携带的 M-1 `12/12`→实际 `15/15`）。本工具把这条
「声明 vs 真实包」变成**机判**（fail-closed），并在报告**没有**硬编码计数时放行（只写计数规则
的写法是允许的、也是推荐写法）。

口径（唯一真源 = 证据包本身）：
- `N 件`        ⇒ 证据目录下的**文件总数**（含 `SHA256SUMS.txt` / `README.md`）；
- `verify N/N`  ⇒ `SHA256SUMS.txt` 的**条目数**（非空且非 `#` 注释行）；
- `eolcheck M/M`⇒ 证据目录下的**文件总数**（与 `N 件` 同口径）。

用法：
    python -I check_evidence_counts.py check \
        --report M3RNG...报告-20261013.md \
        --dir    evidence-m3-rng-trace-scene-groups-20261013-groups-expected
    python -I check_evidence_counts.py selftest

退出码：0 = GREEN（一致 / 报告未硬编码计数）；1 = RED（计数与真实包不符，点名）；2 = PREREQ（缺件 / 坏输入）。
"""

import argparse
import os
import re
import shutil
import sys
import tempfile

HERE = os.path.dirname(os.path.abspath(__file__))


def _sha_entries(path):
    """`SHA256SUMS.txt` 的条目数（非空、非 `#` 注释行）。"""
    n = 0
    with open(path, "r", encoding="utf-8-sig", newline="\n") as fh:
        for line in fh:
            s = line.strip()
            if s and not s.startswith("#"):
                n += 1
    return n


def _count_files(root):
    n = 0
    for _dirpath, _dirnames, filenames in os.walk(root):
        n += len(filenames)
    return n


def _slice_after(text, needle, width=600):
    idx = text.find(needle)
    if idx < 0:
        return ""
    return text[idx:idx + width]


RE_FILES = re.compile(r"(\d+)\s*件")
RE_VERIFY = re.compile(r"verify\s*--check-extra[^\n]{0,60}?GREEN\s*(\d+)\s*/\s*(\d+)")
RE_EOLCHECK = re.compile(r"eolcheck[^\n]{0,60}?GREEN\s*(\d+)\s*/\s*(\d+)")


def parse_declared(snippet):
    """从证据头行片段里取声明计数；缺项返回 None（只写计数规则 ⇒ 允许）。"""
    d = {}
    m = RE_FILES.search(snippet)
    if m:
        d["files"] = int(m.group(1))
    m = RE_VERIFY.search(snippet)
    if m:
        d["verify"] = (int(m.group(1)), int(m.group(2)))
    m = RE_EOLCHECK.search(snippet)
    if m:
        d["eolcheck"] = (int(m.group(1)), int(m.group(2)))
    return d


def check(report_path, ev_dir):
    if not report_path or not os.path.exists(report_path):
        return 2, ["PREREQ_MISSING: 找不到报告 %s" % (report_path or "(空)",)]
    if not ev_dir or not os.path.isdir(ev_dir):
        return 2, ["PREREQ_MISSING: 找不到证据目录 %s" % (ev_dir or "(空)",)]
    with open(report_path, "r", encoding="utf-8-sig") as fh:
        text = fh.read()
    base = os.path.basename(os.path.normpath(ev_dir))
    snippet = _slice_after(text, base)
    if not snippet:
        return 2, ["PREREQ_MISSING: 报告里没有引用证据目录名 %r" % base]
    declared = parse_declared(snippet)
    actual_files = _count_files(ev_dir)
    sums_path = os.path.join(ev_dir, "SHA256SUMS.txt")
    if "verify" in declared and not os.path.exists(sums_path):
        return 2, ["PREREQ_MISSING: 报告声明了 verify 计数，但 %s 不存在" % sums_path]
    actual_sha = _sha_entries(sums_path) if os.path.exists(sums_path) else None

    problems = []
    if not declared:
        print("EVIDENCE_COUNTS dir=%s declared=(none) actual_files=%d actual_verify=%s"
              % (ev_dir, actual_files, actual_sha))
        print("  OK 报告未硬编码证据计数（只写计数规则 ⇒ 允许；真源 = SHA256SUMS/目录）")
        return 0, []
    if "files" in declared and declared["files"] != actual_files:
        problems.append("declared_files=%d != actual_files=%d" % (declared["files"], actual_files))
    if "verify" in declared:
        vn, vd = declared["verify"]
        if vn != vd:
            problems.append("declared_verify=%d/%d 自相矛盾（分子≠分母）" % (vn, vd))
        elif vn != actual_sha:
            problems.append("declared_verify=%d/%d != actual_verify=%d/%d (SHA256SUMS 条目数)"
                            % (vn, vd, actual_sha, actual_sha))
    if "eolcheck" in declared:
        en, ed = declared["eolcheck"]
        if en != ed:
            problems.append("declared_eolcheck=%d/%d 自相矛盾（分子≠分母）" % (en, ed))
        elif en != actual_files:
            problems.append("declared_eolcheck=%d/%d != actual_eolcheck=%d/%d (目录文件数)"
                            % (en, ed, actual_files, actual_files))
    print("EVIDENCE_COUNTS dir=%s declared=%s actual_files=%d actual_verify=%s"
          % (ev_dir, declared, actual_files, actual_sha))
    if problems:
        for p in problems:
            print("  RED %s" % p)
        return 1, problems
    print("  OK 声明计数与真实证据包一致")
    return 0, []


# ----------------------------- selftest -----------------------------

def _write(path, text):
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(text)


def _mk_pkg(root, name, n_data):
    d = os.path.join(root, name)
    os.makedirs(d)
    for i in range(n_data):
        _write(os.path.join(d, "%02d-x.txt" % i), "x\n")
    _write(os.path.join(d, "README.md"), "# r\n")
    sums = "".join("%s *%02d-x.txt\n" % ("0" * 64, i) for i in range(n_data))
    sums += "%s *README.md\n" % ("0" * 64)
    _write(os.path.join(d, "SHA256SUMS.txt"), sums)
    # actual_files = n_data + README + SHA256SUMS = n_data + 2 ; sha = n_data + 1
    return d


def cmd_selftest(_args):
    tmp = tempfile.mkdtemp(prefix="evcount-")
    ok = 0
    failed = []

    def case(name, fn):
        nonlocal ok
        try:
            fn()
            ok += 1
        except AssertionError as exc:
            failed.append((name, str(exc)))

    try:
        d = _mk_pkg(tmp, "ev-good", 3)          # files=5, sha=4
        rep_ok = os.path.join(tmp, "ok.md")
        _write(rep_ok,
               "- 证据：`ev-good/`（5 件含 SHA256SUMS.txt；`verify --check-extra` GREEN 4/4，"
               "`eolcheck` GREEN 5/5）。\n")

        def _green():
            rc, _ = check(rep_ok, d)
            assert rc == 0, "expected GREEN rc=0 got %d" % rc
        case("consistent-green", _green)

        def _files_red():
            p = os.path.join(tmp, "files.md")
            _write(p, "- 证据：`ev-good/`（4 件含 SHA256SUMS.txt；`verify --check-extra` GREEN 4/4，"
                      "`eolcheck` GREEN 5/5）。\n")
            rc, probs = check(p, d)
            assert rc == 1 and any("declared_files" in x for x in probs), (rc, probs)
        case("files-drift-red", _files_red)

        def _verify_red():
            p = os.path.join(tmp, "verify.md")
            _write(p, "- 证据：`ev-good/`（5 件含 SHA256SUMS.txt；`verify --check-extra` GREEN 5/5，"
                      "`eolcheck` GREEN 5/5）。\n")
            rc, probs = check(p, d)
            assert rc == 1 and any("declared_verify" in x for x in probs), (rc, probs)
        case("verify-drift-red", _verify_red)

        def _eol_red():
            p = os.path.join(tmp, "eol.md")
            _write(p, "- 证据：`ev-good/`（5 件含 SHA256SUMS.txt；`verify --check-extra` GREEN 4/4，"
                      "`eolcheck` GREEN 6/6）。\n")
            rc, probs = check(p, d)
            assert rc == 1 and any("declared_eolcheck" in x for x in probs), (rc, probs)
        case("eolcheck-drift-red", _eol_red)

        def _rule_only_green():
            p = os.path.join(tmp, "rule.md")
            _write(p, "- 证据：`ev-good/`（件数/verify 以 `SHA256SUMS.txt` 为准，本行不硬编码）。\n")
            rc, _ = check(p, d)
            assert rc == 0, "rule-only report should be GREEN rc=0 got %d" % rc
        case("rule-only-green", _rule_only_green)

        def _missing_dir_prereq():
            rc, _ = check(rep_ok, os.path.join(tmp, "nope"))
            assert rc == 2, "missing dir should be PREREQ rc=2 got %d" % rc
        case("missing-dir-prereq", _missing_dir_prereq)

        # 改坏必红：真表的等价再现——把「一致」报告的证据包多塞一件（真实包长大）⇒ 声明的 5 件立刻失真。
        def _package_grew_red():
            p = os.path.join(tmp, "grew.md")
            _write(p, "- 证据：`ev-good/`（5 件含 SHA256SUMS.txt；`verify --check-extra` GREEN 4/4，"
                      "`eolcheck` GREEN 5/5）。\n")
            _write(os.path.join(d, "99-late.txt"), "y\n")
            rc, probs = check(p, d)
            assert rc == 1 and any("declared_files" in x for x in probs), (rc, probs)
        case("package-grew-after-declaration-red", _package_grew_red)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    total = ok + len(failed)
    if failed:
        print("SELFTEST RED: check_evidence_counts selftest %d/%d" % (ok, total))
        for name, msg in failed:
            print("  FAIL %s: %s" % (name, msg))
        return 1
    print("SELFTEST GREEN: check_evidence_counts selftest %d/%d（一致绿 / 件数·verify·eolcheck 漂移必红且点名 / "
          "只写计数规则绿 / 缺件 PREREQ / 交付后证据包长大必红）" % (ok, total))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description="核对报告证据头行计数 vs 真实证据包")
    sub = ap.add_subparsers(dest="cmd")
    c = sub.add_parser("check")
    c.add_argument("--report", required=True)
    c.add_argument("--dir", required=True)
    sub.add_parser("selftest")
    args = ap.parse_args(argv)
    if args.cmd == "check":
        rc, _ = check(args.report, args.dir)
        return rc
    if args.cmd == "selftest":
        return cmd_selftest(args)
    ap.print_help()
    return 2


if __name__ == "__main__":
    sys.exit(main())
