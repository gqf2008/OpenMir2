#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""run_batch_selftest.py — `tools/window/run_batch.sh` 的**离线**自检
（卡 `openmir2-runbatch-dryrun-password`）。

只读、不起栈、不连库、不占端口：每次调用都自带独立临时 `--run-dir`，结束即删。
`--no-preflight` 一律带上——本自检判的是**密码门的位置**，不是前置自检；带上它还能顺带证明
卡面 A4（`--no-preflight` 不绕过真跑路径的密码门）。

牙（四态，缺一条就会漏掉一类回归）：

  T1 `--dry-run` + **无** `CONFORMANCE_PASSWORD` ⇒ **rc=0** 且 stdout 含 WRN 标记串
     （放行是对的：dry-run 不起栈不连库，不可能「整轮白开」；告警必须显式且在 **stdout**）。
  T2 同环境**去掉** `--dry-run` ⇒ **rc=2** 且**零场景启动**——不只看 rc：还断言
     run-dir **没有被创建**、stdout 里**没有** `start card=`（否则「rc=2 但已经跑了两条」也照样绿）。
  T3 **带**密码的 `--dry-run` ⇒ rc=0 且 **无** WRN 标记串（防 WRN 恒亮——恒亮的告警等于没有告警）。
  T4 `--list` + 无密码 ⇒ rc=0（该门本就在 `--list` 的 `exit 0` 之后；这条是回归保护，
     防止有人把门整体搬到 `--list` 前面）。

红检（变异见卡 `openmir2-runbatch-dryrun-password` 的 patch body）：
  M1 删掉 `DRY_RUN` 分支（= 退回修复前）⇒ T1 红；M2 真跑分支也放行 ⇒ T2 红；
  M3 把 WRN 改成无条件打印 ⇒ T3 红；M4 把门搬到 `--list` 之前 ⇒ T4 红。

退出码：0 全绿；1 有红；**2 = 判不了**（本机探测不到 Git bash ⇒ 裸 `bash` 会命中 WSL stub）。
"""

import os
import shutil
import subprocess
import sys
import tempfile

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUN_BATCH = os.path.join("tools", "window", "run_batch.sh")
WRN_MARK = "WRN: 缺 CONFORMANCE_PASSWORD"
TIMEOUT_SEC = 180  # dry-run 实测 < 2s；给足余量但**必须有上限**（挂住不许冒充跑完）


# --- bash 必须是 Git bash 绝对路径（同 card_window.detect_git_bash 的口径）-------------
# CreateProcess 对裸 `bash` 的搜索顺序是「应用目录 → 当前目录 → **System32** → Windows 目录 →
# PATH」，System32 先于 PATH：装了 WSL 功能的机器上裸 `bash` 会命中 WSL 启动器 stub
# （本机实测：rc=1 + HCS_E_HYPERV_NOT_INSTALLED），整条自检会全假红。
def _is_wsl_stub(token):
    p = os.path.normcase(str(token).replace("/", os.sep))
    return (os.path.basename(p).startswith("bash")
            and ("system32" in os.path.dirname(p) or "sysnative" in os.path.dirname(p)))


GIT_BASH_CANDIDATES = (
    "C:/Program Files/Git/usr/bin/bash.exe",
    "C:/Program Files (x86)/Git/usr/bin/bash.exe",
    os.path.expandvars(r"%LocalAppData%/Programs/Git/usr/bin/bash.exe"),
)


def find_bash():
    """探测 Git bash 绝对路径：`RUN_BATCH_SELFTEST_BASH` → `CARD_WINDOW_GIT_BASH` →
    常见安装位 → PATH（都要跳过 WSL stub）。找不到返回 None ⇒ 本脚本报 NEED(rc=2)。"""
    for key in ("RUN_BATCH_SELFTEST_BASH", "CARD_WINDOW_GIT_BASH"):
        c = os.environ.get(key, "").strip()
        if c and os.path.isfile(c) and not _is_wsl_stub(c):
            return c.replace(os.sep, "/")
    for c in GIT_BASH_CANDIDATES:
        if c and os.path.isfile(c) and not _is_wsl_stub(c):
            return c.replace(os.sep, "/")
    found = shutil.which("bash")
    if found and os.path.isfile(found) and not _is_wsl_stub(found):
        return found.replace(os.sep, "/")
    return None


BASH = find_bash()

results = []


def check(name, ok, detail=""):
    results.append((name, ok, detail))
    print(("PASS " if ok else "FAIL ") + name + ("" if ok else "  " + detail))


def make_shims(shim_dir):
    """在 `shim_dir` 里造 `python` / `powershell` 两个**垫片**，往 `$RB_SHIM_LOG` 追一行 `argv`。

    为什么需要它（卡 `openmir2-runbatch-dryrun-password` F3）：T2 的旧断言「run-dir 未创建」只能
    证明**没建目录**，证明不了「没调用外部命令」。加了垫片后，M2（把真跑分支也放行）**可以安全重放**——
    真跑到 `card_window` / 起栈那步只会打到垫片，不会真起栈/连库/占端口；而垫片日志里出现
    `card_window.py` / `powershell` 本身就是「零启动」被破坏的证据。

    **两类垫片行为不同**（这是 F3 能诚实工作的关键）：
      * `python`：**只拦 `card_window.py`**（跑场景），其余（清单解析等）**exec 真 python**——
        否则清单解析被打成空输出，run_batch 会在**密码门之前**就 die，M2 就永远观察不到；
      * `powershell`：一律只记日志 + exit 0（起/停栈绝不落到真机）。
    """
    os.makedirs(shim_dir, exist_ok=True)
    sh_py = (
        '#!/bin/sh\n'
        'printf "python %s\\n" "$*" >> "$RB_SHIM_LOG"\n'
        'case "$*" in\n'
        '  *card_window.py*) exit 0 ;;\n'
        'esac\n'
        'exec "$RB_REAL_PYTHON" "$@"\n'
    )
    sh_ps = (
        '#!/bin/sh\n'
        'printf "powershell %s\\n" "$*" >> "$RB_SHIM_LOG"\n'
        'exit 0\n'
    )
    for name, body in (("python", sh_py), ("powershell", sh_ps)):
        p = os.path.join(shim_dir, name)
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(body)
        os.chmod(p, 0o755)


def run_batch(args, with_password, shim_dir=None, shim_log=None):
    """跑一次 run_batch.sh：`with_password=False` 时从环境里**删掉** CONFORMANCE_PASSWORD。

    `shim_dir` 非空 ⇒ 把它插到 PATH 最前（再加 `RB_SHIM_LOG`），外部命令全部打到垫片。
    """
    env = dict(os.environ)
    env.pop("CONFORMANCE_PASSWORD", None)
    if with_password:
        env["CONFORMANCE_PASSWORD"] = "selftest-not-a-real-password"
    if shim_dir:
        env["PATH"] = shim_dir + os.pathsep + env.get("PATH", "")
        env["RB_SHIM_LOG"] = shim_log
        env["RB_REAL_PYTHON"] = sys.executable or "python"
    try:
        r = subprocess.run([BASH, RUN_BATCH] + args, cwd=REPO_ROOT, env=env,
                           capture_output=True, text=True, encoding="utf-8",
                           errors="replace", timeout=TIMEOUT_SEC)
        return r.returncode, (r.stdout or ""), (r.stderr or "")
    except subprocess.TimeoutExpired:
        return None, "", "timeout after %ds" % TIMEOUT_SEC


def main():
    if BASH is None:
        # 本机没有可用的 Git bash ⇒ 判不了（NEED），不是工具红了。
        print("NEED: 本机未探测到 Git bash（裸 bash 会命中 WSL stub）——装 Git for Windows "
              "或设 RUN_BATCH_SELFTEST_BASH / CARD_WINDOW_GIT_BASH")
        return 2
    print("run_batch_selftest: bash=%s" % BASH)
    tmp = tempfile.mkdtemp(prefix="run_batch_selftest_")
    try:
        # ---- T1：dry-run 无密码 ⇒ rc=0 + WRN（stdout）----
        rd1 = os.path.join(tmp, "t1")
        rc, out, err = run_batch(["--dry-run", "--no-preflight", "--run-dir", rd1], with_password=False)
        check("T1 dry-run 无密码 ⇒ rc=0", rc == 0, "rc=%r stderr=%s" % (rc, err[-300:]))
        check("T1 dry-run 无密码 ⇒ stdout 含 WRN 标记串", WRN_MARK in out, out[-400:])

        # ---- T2：同环境去掉 dry-run ⇒ rc=2 且**零启动**（run-dir + PATH 垫片双判据）----
        rd2 = os.path.join(tmp, "t2")
        shim_dir = os.path.join(tmp, "shim")
        shim_log = os.path.join(tmp, "shim-calls.log")
        make_shims(shim_dir)
        rc, out, err = run_batch(["--no-preflight", "--run-dir", rd2], with_password=False,
                                 shim_dir=shim_dir, shim_log=shim_log)
        check("T2 真跑无密码 ⇒ rc=2（fail-closed，--no-preflight 不绕过）", rc == 2, "rc=%r" % (rc,))
        check("T2 真跑无密码 ⇒ 零场景启动（run-dir 未创建）", not os.path.exists(rd2),
              "run-dir 竟然存在：%s" % rd2)
        check("T2 真跑无密码 ⇒ stdout 无 `start card=`", "start card=" not in out, out[-400:])
        # F3：外部命令（card_window 走的 python / 起栈走的 powershell）**一次都不许被调用**。
        # 这条让 M2（真跑分支也放行）可以在不启动任何东西的前提下重放。
        calls = ""
        if os.path.exists(shim_log):
            calls = open(shim_log, encoding="utf-8", errors="replace").read()
        # 允许**一次**预期调用：清单解析（`python -I -B - <run3-scenarios.json>`，发生在密码门之前，
        # 不启动任何东西）。**不许**出现 `card_window.py`（跑场景）或 `powershell`（起/停栈）。
        lines = [l for l in calls.splitlines() if l.strip()]
        scenario_calls = [l for l in lines if "card_window.py" in l]
        stack_calls = [l for l in lines if l.startswith("powershell")]
        manifest_calls = [l for l in lines if "run3-scenarios.json" in l]
        check("T2 真跑无密码 ⇒ 垫片里没有 card_window 调用（零场景）", not scenario_calls,
              "card_window 被调用：\n%s" % "\n".join(scenario_calls)[:400])
        check("T2 真跑无密码 ⇒ 垫片里没有 powershell 调用（零起栈/停栈）", not stack_calls,
              "powershell 被调用：\n%s" % "\n".join(stack_calls)[:400])
        # **地板钉死为 1**（reviewer-20 实测：自建垫片直跑真 `run_batch.sh`，清单解析恒调一次 python）。
        # 只写上限（<= 1）时，**垫片 0 次调用**会让三条新断言全部空过——那正是「PATH 注入失效
        # （`python3` / 绝对解释器路径 / `exec` 改动）⇒ `card_window` 真的执行」这种假绿。
        # 0 次调用必须红，不许绿。
        check("T2 真跑无密码 ⇒ 垫片恰好一次清单解析（地板=1，0 次视为 PATH 注入失效）",
              len(lines) == 1 and len(manifest_calls) == 1,
              "垫片调用 %d 次（期望 1 次清单解析）：\n%s" % (len(lines), calls[:400]))

        # ---- T3：带密码的 dry-run ⇒ rc=0 且无 WRN（防恒亮）----
        rd3 = os.path.join(tmp, "t3")
        rc, out, err = run_batch(["--dry-run", "--no-preflight", "--run-dir", rd3], with_password=True)
        check("T3 dry-run 带密码 ⇒ rc=0", rc == 0, "rc=%r stderr=%s" % (rc, err[-300:]))
        check("T3 dry-run 带密码 ⇒ 无 WRN（防恒亮）", WRN_MARK not in out, out[-400:])

        # ---- T4：--list 无密码 ⇒ rc=0（门本就在 list 之后）----
        rc, out, err = run_batch(["--list"], with_password=False)
        check("T4 --list 无密码 ⇒ rc=0", rc == 0, "rc=%r stderr=%s" % (rc, err[-300:]))
        check("T4 --list 无密码 ⇒ 有清单行", "场景数" in out, out[-300:])
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    failed = [n for n, ok, _ in results if not ok]
    print("%s: run_batch_selftest %d/%d" % ("GREEN" if not failed else "RED",
                                            len(results) - len(failed), len(results)))
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
