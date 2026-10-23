#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""selftest_mutations.py — card_window.py 的「牙」harness（可复跑）。

每颗牙对 `card_window.py` 做若干处**精确替换**（每处必须恰好命中 1 次），然后跑
`card_window.py --selftest`，要求：

  1. 退出码非 0（超时按失败：selftest 子进程带 timeout，`TimeoutExpired` => rc=124）；
  2. stdout 里至少一条 `FAIL `（是断言失守，不是异常崩）；
  3. 这批 FAIL 里**至少一条命中该牙的 `expect` 子串**——锁住「FAIL 落在该牙对应的判据组」，
     防止某颗牙只是被别的兜底断言顺带抓到（N-2）；
  4. 输出里没有 `Traceback`（干净的 RED，不是异常噪声）；
  5. 还原后的文件 sha256 == 变异前 sha256（逐字节还原）；**不一致 => 整体 rc≠0**（N-3）。

为什么需要它：本项目反复栽在「只断言函数、不断言调用点」——把调用点改回旧实现后
selftest 仍然全绿。每颗牙都对应一处**曾经绿过**的缺陷，改坏必红才算判据有效。

用法（离线；不连网/不连库/不起真栈/不占端口）：

    python -I -B tools/conformance/selftest_mutations.py
    python -I -B tools/conformance/selftest_mutations.py --list
    python -I -B tools/conformance/selftest_mutations.py --only T1,E2
    python -I -B tools/conformance/selftest_mutations.py --selfcheck

`--selfcheck` 校验 harness 自身的 fail-closed：final_rc（含 sha 不一致）与子进程超时路径。

注意：脚本会临时改写同目录的 `card_window.py`，跑完立即按原始字节还原并校验 sha256；
即使中途异常也会在 finally 里还原。**不要**在 `card_window.py` 有未提交改动时并行跑它。
"""
import argparse
import hashlib
import io
import os
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
TARGET = os.path.join(HERE, "card_window.py")
SELFTEST_TIMEOUT_S = 300


def sha256_text(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


# 牙：(id, 说明, expect 子串, [(old, new), ...])
TEETH = [
    ("T1", "F-2 停栈兜底改回旧式（只判辅助函数、不判调用点）",
     "F-2 stop_rc=None => 工具 rc=2",
     [('''        if not stop_ok(stack_attempted, res["stop_rc"]):''',
       '''        if res["rc"] == 0 and res["stop_rc"] not in (0, None):''')]),
    ("T2", "F-5 run_id 写成常量（跨运行不可鉴别）",
     "F-5 两次运行 run_id 必须不同",
     [('''        "run_id": uuid.uuid4().hex,''',
       '''        "run_id": "fixed-run-id",''')]),
    ("T3", "F-3 允许 binaries=[]（PASS 产物无二进制 sha256）",
     "F-3 binaries=[] => 加载期 ConfigError",
     [('''        bins = sc.get("binaries")
        if not isinstance(bins, list) or not bins or any(
                not isinstance(x, str) or not x for x in bins):''',
       '''        bins = sc.get("binaries", [])
        if not isinstance(bins, list) or any(
                not isinstance(x, str) or not x for x in bins):''')]),
    ("T4", "F-4 缺二进制分支退回只落 report.json（半套产物）",
     "F-4 缺二进制分支产物集完整",
     [('''        summary["commands"] = runner.commands
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        # F-4 选「完整产物集」：report.json + README.txt + card-window.json + card-window.log 全落，
        # 不写半套证据（--post-comment 仍只在真正跑完编排时才发）。
        write_artifacts(out_dir, summary)
        if not write_summary_json(out_dir, summary):
            summary["exit_code"] = 2
        return summary''',
       '''        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        return summary''')]),
    ("T5", "F-1 提示改回「在 db.databases 里显式声明」（照做仍失败）",
     "F-1 显式声明未知库",
     [('''            "这些库名是**永久拒绝**——在 db.databases 里再声明一次同样不放行，"
            "--allow-production-db 与 --dry-run 也不放行。"
            "请换用 preset=test 或 mir2_test_* 命名的测试库" % ", ".join(unknown))''',
       '''            "请在 db.databases 里显式声明测试库名（或改用 preset=test / mir2_test_*）"
            % ", ".join(unknown))''')]),
    ("T6", "H-1 write_artifacts 提前到 exit_code 之前且去掉 finalize 兜底（README 写失败被 PASS 覆盖）",
     "H-1 PASS 路径 README 写失败",
     [('''    # H-1：先定稿 verdict/exit_code，再写产物——write_artifacts 在 README 写失败时置 rc=2，
    # 绝不能被后面的 `0 if ok else 2` 覆盖回 0（否则 PASS 运行缺 README 还报绿）。
    ok = (summary["verdict"] == "PASS" and summary.get("compare_rc") == 0
          and discovery_evidence_ok
          and all((summary["sides"].get(s) or {}).get("rc") == 0 for s in SIDES))
    summary["exit_code"] = 0 if ok else 2

    # 归档：report.json + 两侧产物 sha256 + README.txt（与缺二进制分支共用同一实现）
    artifacts_ok = write_artifacts(out_dir, summary)''',
       '''    artifacts_ok = write_artifacts(out_dir, summary)

    ok = (summary["verdict"] == "PASS" and summary.get("compare_rc") == 0
          and discovery_evidence_ok
          and all((summary["sides"].get(s) or {}).get("rc") == 0 for s in SIDES))
    summary["exit_code"] = 0 if ok else 2'''),
      ('''    finalize_ok = artifacts_ok and json_ok
    if not finalize_ok:
        summary["exit_code"] = 2''',
       '''    finalize_ok = artifacts_ok and json_ok''')]),
    ("T7", "H-2 归档目录固定 _stale-fixed（第三次运行覆盖第一次存档）",
     "H-2 第三次运行",
     [('''    ts = time.strftime("%Y%m%dT%H%M%S")
    dest = os.path.join(out_dir, "_stale-%s-%d" % (ts, os.getpid()))
    n = 1
    while os.path.exists(dest):
        dest = os.path.join(out_dir, "_stale-%s-%d-%d" % (ts, os.getpid(), n))
        n += 1
    os.makedirs(dest)''',
       '''    dest = os.path.join(out_dir, "_stale-fixed")
    os.makedirs(dest, exist_ok=True)''')]),
    ("T8", "N-1 comment 判定看不到 README 写失败（等价于先写账本再发 comment）",
     "N-1 README 写失败 => comment verdict=UNDECIDABLE",
     [('''    artifacts_ok = write_artifacts(out_dir, summary)''',
       '''    write_artifacts(out_dir, summary)
    artifacts_ok = True
    summary["exit_code"] = 0''')]),
    ("T9", "N-1 comment 的 verdict 固定写 PASS（伪造账本）",
     "N-1 README 写失败 => comment verdict=UNDECIDABLE",
     [('''            "verdict": comment_verdict,''',
       '''            "verdict": "PASS",''')]),
    ("T10", "Y1 finalize_ok 只依赖 artifacts_ok（忽略首版 card-window.json 写失败）",
     "Y1 首版 card-window.json 写失败",
     [('''    finalize_ok = artifacts_ok and json_ok''',
       '''    finalize_ok = artifacts_ok''')]),
    ("T11", "D-1 证据缺件不再降级（exit!=0/finalize 失败仍判 PASS）",
     "N-1 README 写失败",
     [('''        elif summary["exit_code"] != 0 or not finalize_ok:
            comment_verdict = "UNDECIDABLE"
            verdict_reason = blockers[0] if blockers else "evidence_incomplete"''',
       '''        elif False:
            comment_verdict = "UNDECIDABLE"
            verdict_reason = blockers[0] if blockers else "evidence_incomplete"''')]),
    ("T12", "D-1 compare FAIL 被压成 UNDECIDABLE（丢掉「确定失败」）",
     "D-1 compare FAIL",
     [('''        if compare_verdict == "FAIL":''',
       '''        if False:''')]),
    ("T13", "M-1 FAIL 分支的 verdict_reason 写成 ok（账本自相矛盾）",
     "D-1 compare FAIL",
     [('''            verdict_reason = "compare_fail"''',
       '''            verdict_reason = "ok"''')]),
    ("T14", "M-2 嵌套 compare 删掉 whitelisted（诊断字段缺失）",
     "M-2 嵌套 compare 字段集合完整",
     [('''                "whitelisted": summary["whitelisted"],
''', '''''')]),
    ("T15", "M-2 嵌套 compare 删掉 db_state（库后态不可复盘）",
     "M-2 嵌套 compare 字段集合完整",
     [('''                "db_state": summary["db_state"],
''', '''''')]),
    ("T16", "M-2 嵌套 compare 删掉 undecidable（差异分类丢失）",
     "M-2 嵌套 compare 字段集合完整",
     [('''                "undecidable": summary["undecidable"],
''', '''''')]),
    ("T17", "P-1 db_state 写错（诊断值与 report.json 不一致）",
     "P-1 PASS db_state",
     [('''                "db_state": summary["db_state"],''',
       '''                "db_state": "bogus",''')]),
    ("T18", "P-1 diffs 与 db_state 互换（值错位）",
     "P-1 PASS diffs",
     [('''                "db_state": summary["db_state"],
                "diffs": summary["diffs"],''',
       '''                "db_state": summary["diffs"],
                "diffs": summary["db_state"],''')]),
    ("T19", "P-1 diffs 固定 999（硬编码假值）",
     "P-1 PASS diffs",
     [('''                "diffs": summary["diffs"],''',
       '''                "diffs": 999,''')]),
    ("T20", "P-1 undecidable 固定 -1（硬编码假值）",
     "P-1 PASS undecidable",
     [('''                "undecidable": summary["undecidable"],''',
       '''                "undecidable": -1,''')]),
    ("T21", "F-1 side doc ok 的两处兜底都丢掉（回到复审缺陷：probe 失败仍 ok=True）",
     "F-1 discovery probe 失败",
     [('''        "ok": (probe_rc == 0 and not missing),''',
       '''        "ok": not missing,'''),
      ('''            if derr:
                doc["error"] = derr
                doc["ok"] = False
''',
       '''            if derr:
                doc["error"] = derr
''')]),
    ("E1", "额外牙：fallback report 的 run_id 写成常量",
     "仍落本次 fallback report",
     [('''        "format": "card-window-report/1",
        "run_id": summary.get("run_id"),''',
       '''        "format": "card-window-report/1",
        "run_id": "fixed",''')]),
    ("E2", "额外牙：allow_production_db 放行未知库名（放宽安全判定）",
     "F-1 显式声明未知库",
     [('''    if unknown:''',
       '''    if unknown and not allow_production_db:''')]),
    ("E3", "额外牙：write_summary_json 不落 card-window.json（半套产物）",
     "产物布局齐全（缺 ['card-window.json']）",
     [('''    try:
        summary_path = os.path.join(out_dir, "card-window.json")''',
       '''    try:
        return False
        summary_path = os.path.join(out_dir, "card-window.json")''')]),
    ("E4", "额外牙：口令抹除失效",
     "日志里的 --password 已抹掉",
     [('''        if redact:
            out[i] = "***"''',
       '''        if redact:
            pass''')]),
    ("E5", "额外牙（H-3）：write_artifacts 不写 README.txt（缺文件时必须是干净 FAIL）",
     "产物布局齐全（缺 ['README.txt']）",
     [('''        write_readme(readme_path, summary)''',
       '''        pass''')]),
    ("E6", "额外牙（H-1b）：两处 write_summary_json 失败判据都被忽略（首版 + 回执）",
     "H-1b card-window.json 写失败 => 调用方 rc=2",
     [('''    json_ok = write_summary_json(out_dir, summary)
    finalize_ok = artifacts_ok and json_ok
    if not finalize_ok:
        summary["exit_code"] = 2''',
       '''    write_summary_json(out_dir, summary)
    json_ok = True
    finalize_ok = artifacts_ok and json_ok
    if not finalize_ok:
        summary["exit_code"] = 2'''),
      ('''    if not write_summary_json(out_dir, summary):
        summary["exit_code"] = 2
    return summary''',
       '''    write_summary_json(out_dir, summary)
    return summary''')]),
    ("E7", "额外牙（N-1）：comment body 不带 exit_code",
     "N-1 README 写失败 => comment verdict=UNDECIDABLE",
     [('''            "exit_code": summary["exit_code"],
''', '''''')]),
    ("D1", "发现接线：probe 命令丢掉 --var（值推出来了却没传给场景）",
     "DISC-1 csharp 的 probe 命令带本侧发现的 --var",
     [(('''            vars_argv = param_discovery.render_vars(values)
            res["vars"] = vars_argv'''),
       ('''            vars_argv = []
            res["vars"] = vars_argv'''))]),
    ("D2", "发现失败时静默用空值继续跑（fail-closed 被绕过）",
     "DISC-4 推导失败 => rc=2",
     [(('''            if missing:
                res["error"] = ("参数发现失败：缺 %s（推导不出就 fail-closed，"
                                "不拿空值/默认值跑场景）" % ", ".join(missing))
                return res'''),
       ('''            if missing:
                values = {}
                missing = []'''))]),
    # --- 角色预置位（卡 openmir2-window-character-placement）---------------------
    # 这四颗牙锁的都是「调用点」：预置阶段被跳过 / 校验被摘掉时，selftest 必须落在
    # PL-* 判据上变红（而不是被别的兜底断言顺带抓到）。
    ("P1", "预置阶段被跳过（run_side 不再调用 run_placement）",
     "PL-1 已就位 => 不执行 UPDATE",
     [('''        if placement:
            pres = run_placement(runner, side, prof, sc, placement, env)''',
       '''        if False and placement:
            pres = run_placement(runner, side, prof, sc, placement, env)''')]),
    ("P2", "预置位的可走性校验被摘掉（落在墙格也照样改库）",
     "PL-8 目标格是墙",
     [('''    if not cell["walkable"]:''',
       '''    if False and not cell["walkable"]:''')]),
    ("P3", "预置位不再核对「只改了需要的列」（连带写坏其它列也放行）",
     "PL-10 预置连带改坏其它列",
     [('''        if after["fp"] != before["fp"]:''',
       '''        if False and after["fp"] != before["fp"]:''')]),
    ("P4", "两侧库配对校验被摘掉（拿 C# 的库给 Rust 侧预置）",
     "PL-12 角色库与该侧部署不配对",
     [('''    if chars_db != expected:''',
       '''    if False and chars_db != expected:''')]),
    ("P5", "预置证据没进 summary（README 的 placement 段退化成 (not run)）",
     "PL-5 README 的 placement 段",
     [('''        if res.get("placement"):
            # README/card-window.json 的 placement 段读这份（与 discovery 同口径）
            summary["placement"]["sides"][side] = res["placement"]''',
       '''        if False and res.get("placement"):
            summary["placement"]["sides"][side] = res["placement"]''')]),
]


def run_selftest(timeout_s=None):
    """跑一次 card_window --selftest；超时按失败返回 rc=124（绝不当绿）。"""
    env = os.environ.copy()
    env["PYTHONIOENCODING"] = "utf-8"
    t = SELFTEST_TIMEOUT_S if timeout_s is None else timeout_s
    try:
        p = subprocess.run([sys.executable, "-I", "-B", TARGET, "--selftest"],
                           cwd=HERE, capture_output=True, text=True,
                           encoding="utf-8", errors="replace", env=env, timeout=t)
    except subprocess.TimeoutExpired as e:
        return 124, [], "TIMEOUT after %ss\n%s\n%s" % (t, e.stdout or "", e.stderr or "")
    out = (p.stdout or "") + (p.stderr or "")
    fails = [l for l in out.splitlines() if l.startswith("FAIL ")]
    return p.returncode, fails, out


def final_rc(failures, final_sha, base_sha):
    """整体退出码：任何未达标牙、或还原后 sha256 不一致 => 1（N-3，绝不走绿）。"""
    if failures or final_sha != base_sha:
        return 1
    return 0


def selfcheck():
    """校验 harness 自身的 fail-closed 逻辑（不依赖变异）。"""
    base = "a" * 64
    checks = [
        (final_rc([], base, base) == 0, "final_rc：无失败且 sha 一致 => 0"),
        (final_rc([], "b" * 64, base) == 1,
         "final_rc：还原 sha 不一致 => 1（N-3，不许走绿）"),
        (final_rc([("T1", "x")], base, base) == 1, "final_rc：有未达标牙 => 1"),
    ]
    rc, fails, out = run_selftest(timeout_s=0.001)
    checks.append((rc == 124 and not fails and "TIMEOUT" in out,
                   "run_selftest 超时 => rc=124（按失败，不走绿）"))
    bad = [msg for ok, msg in checks if not ok]
    for ok, msg in checks:
        print("%s %s" % ("PASS" if ok else "FAIL", msg))
    if bad:
        print("SELFCHECK RED: %d/%d" % (len(checks) - len(bad), len(checks)))
        return 1
    print("SELFCHECK GREEN: %d/%d" % (len(checks), len(checks)))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(
        prog="selftest_mutations.py",
        description="card_window.py 的变异牙 harness：每颗牙改坏必红 + expect 锁定判据组 + "
                    "逐字节还原。")
    ap.add_argument("--list", action="store_true", help="只列出牙，不执行")
    ap.add_argument("--only", help="只跑指定牙（逗号分隔的 id，如 T1,E2）")
    ap.add_argument("--selfcheck", action="store_true",
                    help="校验 harness 自身（final_rc / 超时按失败）")
    args = ap.parse_args(argv)

    if args.selfcheck:
        return selfcheck()

    wanted = None
    if args.only:
        wanted = [x.strip() for x in args.only.split(",") if x.strip()]
        known = {t[0] for t in TEETH}
        bad = [x for x in wanted if x not in known]
        if bad:
            print("未知牙：%s（可选 %s）" % (",".join(bad), ",".join(sorted(known))))
            return 2

    if args.list:
        for tid, desc, expect, _pairs in TEETH:
            print("%-4s %s\n     expect: %s" % (tid, desc, expect))
        return 0

    with io.open(TARGET, "r", encoding="utf-8", newline="") as f:
        base = f.read()
    base_sha = sha256_text(base)
    print("target: %s" % TARGET)
    print("baseline sha256 = %s" % base_sha)

    rc0, _fails0, out0 = run_selftest()
    if rc0 != 0 or "Traceback" in out0:
        tail = [l for l in out0.splitlines() if l.startswith(("FAIL ", "RED", "GREEN", "TIMEOUT"))]
        print("baseline selftest 不是全绿（rc=%s）：%s" % (rc0, tail[-3:]))
        return 2
    print("baseline selftest GREEN; 开始逐颗变异")

    failures = []
    selected = [t for t in TEETH if wanted is None or t[0] in wanted]
    final_sha = base_sha
    try:
        for tid, desc, expect, pairs in selected:
            mutated = base
            bad_pairs = []
            for idx, (old, new) in enumerate(pairs, 1):
                n = mutated.count(old)
                if n != 1:
                    bad_pairs.append("第 %d 处命中 %d 次" % (idx, n))
                    continue
                mutated = mutated.replace(old, new, 1)
            if bad_pairs:
                failures.append((tid, "; ".join(bad_pairs)))
                print("BAD  %-3s %s :: %s" % (tid, desc, "; ".join(bad_pairs)))
                continue
            try:
                with io.open(TARGET, "w", encoding="utf-8", newline="") as f:
                    f.write(mutated)
                rc, fails, out = run_selftest()
            finally:
                with io.open(TARGET, "w", encoding="utf-8", newline="") as f:
                    f.write(base)
            with io.open(TARGET, "r", encoding="utf-8", newline="") as f:
                restored_sha = sha256_text(f.read())
            clean = "Traceback" not in out
            expect_ok = any(expect in f for f in fails)
            ok = rc != 0 and bool(fails) and clean and expect_ok and restored_sha == base_sha
            if not ok:
                why = []
                if rc == 0:
                    why.append("rc=0（牙没咬到）")
                elif rc == 124:
                    why.append("自检超时（按失败）")
                if not fails:
                    why.append("没有 FAIL 行")
                if not clean:
                    why.append("输出含 Traceback（不干净的 RED）")
                if not expect_ok:
                    why.append("FAIL 未落在判据组 expect=%r" % expect)
                if restored_sha != base_sha:
                    why.append("还原 sha256 不一致")
                failures.append((tid, "; ".join(why)))
            hit = next((f for f in fails if expect in f), fails[0] if fails else "(无 FAIL)")
            print("%s %-3s %s :: rc=%s :: %s"
                  % ("RED " if ok else "MISS", tid, desc, rc, hit))
    finally:
        with io.open(TARGET, "w", encoding="utf-8", newline="") as f:
            f.write(base)
        with io.open(TARGET, "r", encoding="utf-8", newline="") as f:
            final_sha = sha256_text(f.read())
        print("restored sha256 = %s%s"
              % (final_sha, " (== baseline)" if final_sha == base_sha else " (!! DIFF !!)"))

    total = len(selected)
    rc = final_rc(failures, final_sha, base_sha)
    if failures:
        print("MUTATION HARNESS: %d/%d 咬到；未达标：%s"
              % (total - len(failures), total,
                 "; ".join("%s(%s)" % (t, w) for t, w in failures)))
    elif final_sha != base_sha:
        print("MUTATION HARNESS: 牙全红，但还原 sha256 与 baseline 不一致 => 失败（N-3）")
    else:
        print("MUTATION HARNESS GREEN: %d/%d 颗牙全红（expect 锁定判据组）+ 逐字节还原"
              % (total, total))
    return rc


if __name__ == "__main__":
    sys.exit(main())
