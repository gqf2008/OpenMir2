#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""开窗前置检查（`docs/window-runbook-20261023.md` §0.5 的一条命令版）。

卡：`openmir2-window-preflight-script`（tier=L）。**只读**：不起栈、不连库；端口只做 connect 探测。

用法：
  python -I -B tools/conformance/preflight.py [--profile alt|<json 路径>]
  python -I -B tools/conformance/preflight.py --with-dry-run --with-selftest
  python -I -B tools/conformance/preflight.py --selftest

退出码（fail-closed）：全部步骤 OK ⇒ 0；任一步 FAIL ⇒ 2（并给出修复建议）。

五步（与 runbook §0.5 一一对应）：
  ① docs    runbook/README 的键口径断言（复用 §0.5 第 0 步的 DOC-KEYS：有 `probe_timeout_ms`、
            没有 profile 里的 `"timeout_ms"`）；
  ② binary  调 `tools/m2-acceptance/start_mir2_server.ps1 -WhatIf` 并解析 `SERVER_EXE*` 三行；
            构建类型必须是 `Release`，打印的 sha256 必须与本地重算一致（不另写选路逻辑）；
  ③ ports   从 profile 的 `sides[*].ports` 取端口**逐个探测**（不写死端口表），要求全空；
  ④ profile profile 键存在性：`probe_timeout_ms` / 每侧 `pid_file` / `require_pid_binaries`；
  ⑤ 可选    `--with-dry-run` 委托 `card_window.py --dry-run`；`--with-selftest` 委托 `--selftest`
            （默认不跑，避免 preflight 本身变慢）。
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import socket
import subprocess
import sys

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
RUNBOOK = os.path.join(REPO, "docs", "window-runbook-20261023.md")
SCEN_README = os.path.join(REPO, "tools", "probe", "scenarios", "README.md")
BIN_SCRIPT = os.path.join(REPO, "tools", "m2-acceptance", "start_mir2_server.ps1")
CARD_WINDOW = os.path.join(HERE, "card_window.py")
PROFILE_KEYS = ("probe_timeout_ms", "ready_timeout_s")
SIDE_KEYS = ("pid_file", "require_pid_binaries")

_TID = re.compile(r'"timeout_ms"\s*:')


def _rel(path):
    """相对仓库的展示路径；**跨盘**（仓库在 E:、TEMP 在 C:）时 relpath 会抛 ValueError
    （review 368ab3a2 blocking），退回绝对路径即可。"""
    try:
        return os.path.relpath(path, REPO)
    except ValueError:
        return str(path)


class StepFail(Exception):
    """单步失败：消息即 fix 建议之前的原因。"""


# ---------------------------------------------------------------------------
# ① 文档口径（复用 §0.5 第 0 步的 DOC-KEYS）
# ---------------------------------------------------------------------------
def check_docs(runbook=RUNBOOK, readme=SCEN_README):
    """runbook 用 DOC-KEYS 全口径（含「不得出现 profile 的 `"timeout_ms"`」）；
    scenarios/README 只要求提到 `probe_timeout_ms`——它有合法的**步骤级** `timeout_ms` DSL 文档，
    不能套用同名负向断言（review 本卡时会误红）。"""
    problems = []
    with open(runbook, encoding="utf-8") as fh:
        text = fh.read()
    if "probe_timeout_ms" not in text:
        problems.append("%s 缺 `probe_timeout_ms`（profile 键被静默忽略的老写法？）"
                        % _rel(runbook))
    if _TID.search(text):
        problems.append("%s 里又出现 profile 的 `\"timeout_ms\"`（会被**静默忽略**，probe 仍按 180s 被杀）"
                        % _rel(runbook))
    with open(readme, encoding="utf-8") as fh:
        text = fh.read()
    if "probe_timeout_ms" not in text:
        problems.append("%s 缺 `probe_timeout_ms`（步骤级 `timeout_ms` 与 profile 键是两回事）"
                        % _rel(readme))
    if problems:
        raise StepFail("；".join(problems) + " ⇒ 按 runbook §0.5 第 0 步改回 `probe_timeout_ms`")
    return "%s + %s 口径 OK（DOC-KEYS）" % (
        _rel(runbook), _rel(readme))


# ---------------------------------------------------------------------------
# ② 二进制（委托 start_mir2_server.ps1 -WhatIf）
# ---------------------------------------------------------------------------
def _sha256(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def parse_whatif(stdout):
    """从 `-WhatIf` 的三行输出里取 (exe, sha256, build)。"""
    exe = sha = build = None
    for line in stdout.splitlines():
        line = line.strip()
        if line.startswith("SERVER_EXE_SHA256"):
            sha = line.split(None, 1)[1].strip() if len(line.split(None, 1)) > 1 else ""
        elif line.startswith("SERVER_EXE_BUILD"):
            build = line.split(None, 1)[1].strip() if len(line.split(None, 1)) > 1 else ""
        elif line.startswith("SERVER_EXE"):
            exe = line.split(None, 1)[1].strip() if len(line.split(None, 1)) > 1 else ""
    return exe, sha, build


def _run_ps(argv):
    """默认 runner：起 powershell 跑 `-WhatIf`，返回 (rc, stdout, stderr)。"""
    proc = subprocess.run(argv, capture_output=True, text=True)
    return proc.returncode, proc.stdout, proc.stderr


def check_binary(script=BIN_SCRIPT, root="E:/tmp/conformance/rust-root",
                 log_dir="E:/tmp/conformance/rust-logs", ports_profile=None,
                 require_release=True, runner=None):
    """`runner` 只为 `--selftest` 注入桩：签名 (argv) -> (rc, stdout, stderr)。"""
    if not os.path.isfile(script):
        raise StepFail("找不到 %s ⇒ 确认在正确的检出里跑（该脚本按自身 $PSScriptRoot 解析 target/）"
                       % _rel(script))
    argv = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", script,
            "-Root", root, "-LogDir", log_dir, "-WhatIf"]
    if ports_profile:
        argv += ["-PortsProfile", ports_profile]
    rc, out, err = (runner or _run_ps)(argv)
    exe, sha, build = parse_whatif(out)
    if rc != 0:
        raise StepFail("start_mir2_server.ps1 -WhatIf rc=%d：%s"
                       % (rc, (err or out).strip()[:300]))
    if not exe or build is None:
        raise StepFail("-WhatIf 输出缺 SERVER_EXE/SERVER_EXE_BUILD 行（脚本变了？）")
    if build != "Release":
        if require_release:
            raise StepFail("SERVER_EXE_BUILD=%s ⇒ 在这棵树里 `cargo build --release -p mir2-server`"
                           "（Debug-Fallback 很可能不是当前 master）" % build)
        return "SERVER_EXE_BUILD=%s（按 --allow-debug 降级为 WARN）" % build
    if not sha or sha == "MISSING":
        raise StepFail("SERVER_EXE_SHA256=MISSING ⇒ 路径解析到了但文件不在：先构建或用 -ServerExe 指定")
    actual = _sha256(exe)
    if actual != sha.lower():
        raise StepFail("打印的 sha256 与本地重算不一致（%s vs %s）⇒ 二进制在 -WhatIf 之后变过，重跑本步"
                       % (sha[:12], actual[:12]))
    return "%s（sha256=%s…）" % (exe, actual[:12])


# ---------------------------------------------------------------------------
# ③ 端口（从 profile 读，不写死）
# ---------------------------------------------------------------------------
def profile_ports(prof):
    ports = []
    for side, sc in (prof.get("sides") or {}).items():
        for p in sc.get("ports") or []:
            p = int(p)
            if p not in ports:
                ports.append(p)
    return ports


def _listening(port, timeout=0.35):
    try:
        with socket.create_connection(("127.0.0.1", port), timeout=timeout):
            return True
    except OSError:
        return False


def check_ports(prof, probe=_listening):
    ports = profile_ports(prof)
    if not ports:
        raise StepFail("profile 没有 sides[*].ports ⇒ 先补 profile（本步不写死端口表）")
    busy = [p for p in ports if probe(p)]
    if busy:
        raise StepFail("端口被占：%r ⇒ 上一轮栈没停净；`card_window` 起栈前也会 rc=2 点名"
                       % busy)
    return "%d 个端口全空（%s）" % (len(ports), ",".join(str(p) for p in ports[:6]) + ("…" if len(ports) > 6 else ""))


# ---------------------------------------------------------------------------
# ④ profile 键
# ---------------------------------------------------------------------------
def check_profile_keys(prof):
    problems = []
    for key in PROFILE_KEYS:
        if key not in prof:
            problems.append("缺 `%s`" % key)
    timeout = prof.get("probe_timeout_ms")
    if timeout is not None and (not isinstance(timeout, int) or timeout <= 0):
        problems.append("`probe_timeout_ms`=%r 不是正整数" % (timeout,))
    sides = prof.get("sides") or {}
    guarded = [name for name, sc in sides.items() if sc.get("require_pid_binaries")]
    for side, sc in sides.items():
        # `pid_file` 只在开了 `require_pid_binaries` 的侧必须存在（C# 侧不参与 pid 对账，
        # 强求它会让 `alt` 剖面误红）。两者都开了却缺 `pid_file` ⇒ 起栈后无法对账。
        if sc.get("require_pid_binaries") and not sc.get("pid_file"):
            problems.append("side `%s` 开了 require_pid_binaries 但缺 `pid_file`" % side)
        if "require_pid_binaries" in sc and not isinstance(sc["require_pid_binaries"], bool):
            problems.append("side `%s` 的 `require_pid_binaries` 不是布尔" % side)
    if not guarded:
        problems.append("没有任何侧开启 `require_pid_binaries`（二进制对账会整段失效）")
    if problems:
        raise StepFail("；".join(problems) + " ⇒ 按 runbook §0.5 第 3 步补 profile 键")
    return "probe_timeout_ms=%s；%d 侧 pid_file/require_pid_binaries 齐全" % (
        timeout, len(prof.get("sides") or {}))


# ---------------------------------------------------------------------------
# ⑤ 可选委托
# ---------------------------------------------------------------------------
def check_dry_run(profile_value, scenario, out_dir):
    argv = [sys.executable, "-I", "-B", CARD_WINDOW, "--profile", profile_value,
            "--scenario", scenario, "--out", out_dir, "--dry-run"]
    proc = subprocess.run(argv, capture_output=True, text=True)
    if proc.returncode != 0:
        raise StepFail("card_window --dry-run rc=%d：%s"
                       % (proc.returncode, (proc.stderr or proc.stdout).strip()[-300:]))
    return "card_window --dry-run rc=0"


def check_selftest():
    argv = [sys.executable, "-I", "-B", CARD_WINDOW, "--selftest"]
    proc = subprocess.run(argv, capture_output=True, text=True)
    if proc.returncode != 0:
        raise StepFail("card_window --selftest rc=%d：%s"
                       % (proc.returncode, (proc.stdout or proc.stderr).strip()[-300:]))
    return "card_window --selftest rc=0"


# ---------------------------------------------------------------------------
# 调度
# ---------------------------------------------------------------------------
def load_profile(value):
    """profile：内建名（导入 card_window 的 BUILTIN_PROFILES）或 JSON 路径（只读）。"""
    sys.path.insert(0, HERE)
    import card_window as cw  # 延迟导入：--selftest 不需要
    return cw.load_profile(value)


def run_steps(profile_value, root, log_dir, ports_profile, with_dry_run, with_selftest,
              allow_debug=False, reporter=None):
    report = reporter or (lambda ok, step, detail: print("[%s] %s: %s"
                                                        % ("OK" if ok else "FAIL", step, detail)))
    results = []

    def one(step, fn):
        try:
            detail = fn()
            results.append(True)
            report(True, step, detail)
        except StepFail as ex:
            results.append(False)
            report(False, step, str(ex))
        except Exception as ex:                     # 任何异常都 fail-closed
            results.append(False)
            report(False, step, "%s: %s" % (type(ex).__name__, ex))

    prof = None
    try:
        prof = load_profile(profile_value)
    except Exception as ex:
        report(False, "profile", "加载失败：%s" % ex)
        results.append(False)

    one("docs", check_docs)
    one("binary", lambda: check_binary(root=root, log_dir=log_dir,
                                       ports_profile=ports_profile,
                                       require_release=not allow_debug))
    if prof is None:
        report(False, "ports", "profile 未加载，无法取端口")
        report(False, "profile", "profile 未加载")
        results += [False, False]
    else:
        one("ports", lambda: check_ports(prof))
        one("profile", lambda: check_profile_keys(prof))
    if with_dry_run:
        scen = os.path.join(REPO, "tools", "probe", "scenarios", "login_only.json")
        one("dry-run", lambda: check_dry_run(profile_value, scen,
                                             os.path.join(os.path.abspath("E:/tmp"),
                                                          "preflight-dry")))
    if with_selftest:
        one("selftest", check_selftest)
    return results


# ---------------------------------------------------------------------------
# --selftest（五个失败分支各 rc=2 且点名；检查在 rc 判定之前）
# ---------------------------------------------------------------------------
def selftest():
    """用桩验证：① docs 红 ② 二进制 Debug-Fallback 红 ③ 端口被占红 ④ profile 缺键红 ⑤ 全绿 rc=0。

    自检自身也遵守纪律 15：所有断言先收集，最后统一判定（不存在「先 return 1 再跑检查」）。
    """
    import tempfile
    tmp = tempfile.mkdtemp(prefix="preflight-selftest-")
    checks = []

    def chk(cond, msg):
        checks.append((bool(cond), msg))

    good_runbook = os.path.join(tmp, "runbook.md")
    bad_runbook = os.path.join(tmp, "runbook-bad.md")
    with open(good_runbook, "w", encoding="utf-8") as fh:
        fh.write("设 `probe_timeout_ms` 到 720000\n")
    with open(bad_runbook, "w", encoding="utf-8") as fh:
        fh.write('profile 里写 "timeout_ms": 720000\n')

    # ① docs
    errs = []
    try:
        check_docs(good_runbook, good_runbook)
    except StepFail as ex:
        errs.append(str(ex))
    chk(not errs, "①a 正确口径不报错")
    try:
        check_docs(bad_runbook, good_runbook)
        chk(False, "①b 旧键 `\"timeout_ms\"` 必须红")
    except StepFail as ex:
        chk("timeout_ms" in str(ex), "①b 旧键 `\"timeout_ms\"` 必须红并点名（%s）" % str(ex)[:60])

    # ② 二进制（桩 ps1）

    exe = os.path.join(tmp, "mir2-server.exe")
    with open(exe, "wb") as fh:
        fh.write(b"stub-binary")
    sha = _sha256(exe)
    # **真测被测函数**（review 368ab3a2 F2）：用 stub runner 调 `check_binary` 本体，
    # 而不是在测试里重写一遍判定——删掉 `check_binary` 的 Release 硬门/sha 比对，这里必须红。
    ps_stub = os.path.join(tmp, "start_mir2_server.ps1")
    with open(ps_stub, "w", encoding="utf-8") as fh:
        fh.write("# stub\n")

    def stub_runner(stdout, rc=0):
        def run(_argv):
            return rc, stdout, ""
        return run

    good_out = ("SERVER_EXE         %s\nSERVER_EXE_SHA256  %s\nSERVER_EXE_BUILD   Release\n"
                % (exe, sha))
    debug_out = ("SERVER_EXE         %s\nSERVER_EXE_SHA256  %s\nSERVER_EXE_BUILD   Debug-Fallback\n"
                 % (exe, sha))

    def run_bin(stdout, rc=0, require_release=True):
        return check_binary(script=ps_stub, root="r", log_dir="l", ports_profile=None,
                            require_release=require_release,
                            runner=stub_runner(stdout, rc))

    try:
        run_bin(good_out)
        chk(True, "②a Release + sha 一致 ⇒ 通过（调 check_binary 本体）")
    except StepFail as ex:
        chk(False, "②a Release + sha 一致 ⇒ 通过（%s）" % ex)
    try:
        run_bin(debug_out)
        chk(False, "②b Debug-Fallback 必须红")
    except StepFail as ex:
        chk("Debug-Fallback" in str(ex), "②b Debug-Fallback 必须红并点名（%s）" % str(ex)[:60])
    try:
        run_bin(debug_out, require_release=False)
        chk(True, "②b' --allow-debug 显式降级 ⇒ 不抛（策略开关生效）")
    except StepFail as ex:
        chk(False, "②b' --allow-debug 显式降级 ⇒ 不抛（%s）" % ex)
    try:
        run_bin(good_out.replace(sha, "0" * 64))
        chk(False, "②c 打印 sha 与本地重算不符必须红")
    except StepFail as ex:
        chk("sha256" in str(ex), "②c 打印 sha 与本地重算不符必须红（%s）" % str(ex)[:60])
    try:
        run_bin(good_out, rc=2)
        chk(False, "②d -WhatIf rc≠0 必须红")
    except StepFail as ex:
        chk("rc=2" in str(ex), "②d -WhatIf rc≠0 必须红（%s）" % str(ex)[:60])

    # ③ 端口
    prof_ports = {"sides": {"a": {"ports": [15000, 17000]}, "b": {"ports": [17000, 17600]}}}
    chk(profile_ports(prof_ports) == [15000, 17000, 17600], "③a 端口从 profile 取并去重")
    try:
        check_ports(prof_ports, probe=lambda p: p == 17000)
        chk(False, "③b 端口被占必须红")
    except StepFail as ex:
        chk("17000" in str(ex), "③b 端口被占必须红并点名 17000（%s）" % str(ex)[:60])
    chk(check_ports(prof_ports, probe=lambda p: False).startswith("3 个端口全空"),
        "③c 全空 ⇒ 通过")

    # ④ profile 键
    good_prof = {"probe_timeout_ms": 20000, "ready_timeout_s": 120,
                 "sides": {"csharp": {"pid_file": "x", "require_pid_binaries": True}}}
    chk(check_profile_keys(good_prof).startswith("probe_timeout_ms=20000"), "④a 键齐全 ⇒ 通过")
    bad_prof = {"ready_timeout_s": 120,
                "sides": {"csharp": {"pid_file": "x", "require_pid_binaries": True}}}
    try:
        check_profile_keys(bad_prof)
        chk(False, "④b 缺 probe_timeout_ms 必须红")
    except StepFail as ex:
        chk("probe_timeout_ms" in str(ex), "④b 缺 probe_timeout_ms 必须红并点名（%s）" % str(ex)[:60])

    # ⑤ 汇总：run_steps 在桩下必须 rc=2（任何一步红），全绿必须 rc=0
    rep = []
    res = run_steps("no-such-profile", "r", "l", None, False, False,
                    reporter=lambda ok, s, d: rep.append((ok, s, d)))
    chk(False in res and any(s == "profile" for ok, s, _ in rep),
        "⑤a 缺 profile ⇒ 至少一步红（fail-closed）")

    shutil.rmtree(tmp, ignore_errors=True)
    failed = [msg for ok, msg in checks if not ok]
    for ok, msg in checks:
        print("  [%s] %s" % ("ok" if ok else "FAIL", msg))
    if failed:
        print("preflight selftest: FAIL")
        for msg in failed:
            print("  -", msg)
        return 2
    print("preflight selftest: OK (%d checks)" % len(checks))
    return 0


# ---------------------------------------------------------------------------
def main(argv=None):
    ap = argparse.ArgumentParser(description="开窗前置检查（runbook §0.5 一条命令版）")
    ap.add_argument("--profile", default="alt", help="内建 profile 名或 profile JSON 路径")
    ap.add_argument("--root", default="E:/tmp/conformance/rust-root", help="传给 start_mir2_server.ps1 -Root")
    ap.add_argument("--log-dir", default="E:/tmp/conformance/rust-logs", help="-LogDir")
    ap.add_argument("--ports-profile", default=None, help="-PortsProfile（可选）")
    ap.add_argument("--allow-debug", action="store_true", help="Debug-Fallback 降级为 WARN（默认红）")
    ap.add_argument("--with-dry-run", action="store_true", help="额外跑 card_window --dry-run")
    ap.add_argument("--with-selftest", action="store_true", help="额外跑 card_window --selftest")
    ap.add_argument("--selftest", action="store_true", help="自测（离线，用桩）")
    args = ap.parse_args(argv)
    if args.selftest:
        return selftest()
    results = run_steps(args.profile, args.root, args.log_dir, args.ports_profile,
                        args.with_dry_run, args.with_selftest, args.allow_debug)
    ok = bool(results) and all(results)
    print("PREFLIGHT %s" % ("PASS" if ok else "FAIL"))
    return 0 if ok else 2


if __name__ == "__main__":
    sys.exit(main())
