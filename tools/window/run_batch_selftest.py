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
  T5 跑后对称性复核**按清单 profile 去重**（卡 `openmir2-postrun-drift-tooth-t5`；完全离线，
     不读真私有根）：从 run_batch.sh 按标记提取跑后复核块（块文本与交付文件逐字节一致），
     用与脚本内部同格式的选中行（US/\x1f 六字段）直接喂块；profile 用 `load_profile` 本体
     从内置 alt 深拷贝出 pa/pb/pc 三份合成 JSON（`envir_root` 指向自建假根）⇒「各查各根」可判：
     T5.1 提取块含 `POSTRUN_PROFILES` 接线（防块形状漂移）；
     T5.2 去重行 = pa/pb/pc 首现序、恰一次；
     T5.3 pa（两假根同内容）恰一次且 **OK**；
     T5.4 pb（`Castle/List.txt` 漂移）恰一次且 DRIFT **点名 Castle/List.txt**；
     T5.5 pc（`Envir/extra.txt` 漂移）恰一次且 DRIFT **点名 Envir/extra.txt**（与 T5.3 互为
          「假根真空转 ⇒ 恒 OK」的负例对照——比较器不读文件的话三条会全 OK）；
     T5.6 profile 复核行总数恰 3（多刷/漏刷都红）；
     T5.7 空选中 ⇒ 块 die **rc=2**（fail-closed，不许静默跳过）；
     T5.8 坏 profile（名字解析不了）⇒ **ERROR 行点名**；
     T5.9 坏 profile **不断链**：排其后的 pa 仍被复核且 OK。
  T9 `--tier R` + stamp 存在 ⇒ rc=0 且 `<run-dir>-dry-run/build.json` 落盘（head/sha256）；
  T10 `--tier R` + stamp 缺失 ⇒ rc=2 且点名 `binary_identity=missing`（R 级证据缺身份不许 PASS）；
  T11 `--tier L`（默认）+ stamp 缺失 ⇒ rc=0 仅标记；T12 L 级 + stamp 存在 ⇒ build.json=stamp 拷贝。
  （卡 `openmir2-harness-stop-and-evidence`；stamp 路径用 `WINDOW_BUILD_STAMP` 覆盖，不碰真实 target/。）

红检（变异见卡 `openmir2-runbatch-dryrun-password` 的 patch body；T5 的变异见卡
  `openmir2-postrun-drift-tooth-t5` 的 patch body）：
  M1 删掉 `DRY_RUN` 分支（= 退回修复前）⇒ T1 红；M2 真跑分支也放行 ⇒ T2 红；
  M3 把 WRN 改成无条件打印 ⇒ T3 红；M4 把门搬到 `--list` 之前 ⇒ T4 红；
  M5 把 run_batch.sh 的跑后复核块退回写死 `load_profile("alt")`（即 F1 修复前形态）⇒
  T5.1..T5.9 除空选中外的命名断言全红（去重行/各 profile 行消失或只剩 alt、ERROR 通道消失、
  空选中不再 die）——实跑并还原，还原后 blob sha 必须等于交付 head（工作树无 MUTATION 残留）。

退出码：0 全绿；1 有红；**2 = 判不了**（本机探测不到 Git bash ⇒ 裸 `bash` 会命中 WSL stub）。
"""

import copy
import importlib.util
import json
import os
import shutil
import subprocess
import sys
import tempfile

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__))))
RUN_BATCH = os.path.join("tools", "window", "run_batch.sh")
WRN_MARK = "WRN: 缺 CONFORMANCE_PASSWORD"
TIMEOUT_SEC = 180  # dry-run 实测 < 2s；给足余量但**必须有上限**（挂住不许冒充跑完）

# T5（卡 openmir2-postrun-drift-tooth-t5）：跑后复核块的提取标记与 card_window 路径。
# 起始标记 = run_batch.sh 里复核块的注释行前缀；结束 = 该块 python heredoc 的终止行。
POSTRUN_BLOCK_START = "# ---- 跑后对称性复核"
POSTRUN_BLOCK_END = "PY"
CARD_WINDOW_PATH = os.path.join(REPO_ROOT, "tools", "conformance", "card_window.py")


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


# --- T5 helpers（卡 openmir2-postrun-drift-tooth-t5）------------------------------------
def extract_postrun_block():
    """按标记从 run_batch.sh 切出跑后复核块（文本与交付文件逐字节一致）。

    标记缺失/块不完整抛 RuntimeError ⇒ 调用方记 FAIL（牙齿的目标没了也必须红，不许静默跳过）。
    """
    with open(RUN_BATCH, encoding="utf-8") as fh:
        lines = fh.read().splitlines()
    start = next((i for i, l in enumerate(lines) if l.startswith(POSTRUN_BLOCK_START)), None)
    if start is None:
        raise RuntimeError("run_batch.sh 找不到跑后复核块起始标记 %r" % POSTRUN_BLOCK_START)
    end = next((i for i in range(start, len(lines)) if lines[i] == POSTRUN_BLOCK_END), None)
    if end is None:
        raise RuntimeError("跑后复核块的 heredoc 终止行 %r 找不到（块不完整？）" % POSTRUN_BLOCK_END)
    return "\n".join(lines[start:end + 1]) + "\n"


def make_fake_root(base, list_content, envir_extra=False):
    """造一份最小假 envir 根：Castle/List.txt（内容参数化）+ Castle/0|1/SabukW.txt + Envir/a.txt。"""
    for d in (os.path.join(base, "Castle", "0"), os.path.join(base, "Castle", "1"),
              os.path.join(base, "Envir")):
        os.makedirs(d, exist_ok=True)
    files = {
        os.path.join(base, "Castle", "List.txt"): list_content,
        os.path.join(base, "Castle", "0", "SabukW.txt"): "total=0\n",
        os.path.join(base, "Castle", "1", "SabukW.txt"): "total=0\n",
        os.path.join(base, "Envir", "a.txt"): "x\n",
    }
    if envir_extra:
        files[os.path.join(base, "Envir", "extra.txt")] = "extra\n"
    for path, content in files.items():
        with open(path, "w", encoding="utf-8", newline="") as fh:
            fh.write(content)


def synthesize_postrun_profiles(tmp):
    """从内置 alt 深拷贝出 pa/pb/pc 三份合成 profile，envir_root 指向自建假根。

    用 `load_profile` 本体生成 ⇒ 天然通过块内同一个加载校验（手写最小 JSON 会复制一遍校验规则，
    规则一改牙就假红）。返回 {名字: profile json 路径}；card_window 导入失败抛异常 ⇒ 调用方判红。
    """
    spec = importlib.util.spec_from_file_location("_cw_t5", CARD_WINDOW_PATH)
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    roots = {
        "pa": (os.path.join(tmp, "root_pa_cs"), os.path.join(tmp, "root_pa_ru"), False, False),
        "pb": (os.path.join(tmp, "root_pb_cs"), os.path.join(tmp, "root_pb_ru"), True, False),
        "pc": (os.path.join(tmp, "root_pc_cs"), os.path.join(tmp, "root_pc_ru"), False, True),
    }
    out = {}
    for name, (cs_dir, ru_dir, list_drift, envir_drift) in roots.items():
        make_fake_root(cs_dir, "0\r\n1\r\n")
        make_fake_root(ru_dir, "0\r\n" if list_drift else "0\r\n1\r\n",
                       envir_extra=envir_drift)
        prof = copy.deepcopy(mod.load_profile("alt"))
        # envir_root = 假根目录本身（Castle/Envir 就建在其下）；影子探测找的是
        # dirname(root)/M2GameSvr-hook ⇒ tmp 下不存在，不影响判定。
        prof["sides"]["csharp"]["envir_root"] = cs_dir.replace(os.sep, "/")
        prof["sides"]["rust"]["envir_root"] = ru_dir.replace(os.sep, "/")
        p = os.path.join(tmp, name + ".json").replace(os.sep, "/")
        with open(p, "w", encoding="utf-8", newline="\n") as fh:
            json.dump(prof, fh, ensure_ascii=False)
        out[name] = p
    return out


def run_postrun_block(bash, tmp, block_text, rows):
    """把选中行（US 六字段，与 run_batch.sh 内部行格式一致）直接喂给提取出的复核块。

    离线判定的关键：块只经 profile 的 envir_root 读到**合成假根**，不读真私有根/不连库/不起栈。
    """
    block_path = os.path.join(tmp, "postrun-block.sh").replace(os.sep, "/")
    rows_path = os.path.join(tmp, "selected-rows.txt").replace(os.sep, "/")
    driver_path = os.path.join(tmp, "t5-driver.sh").replace(os.sep, "/")
    with open(block_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(block_text)
    with open(rows_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("".join(r + "\n" for r in rows))
    # 行经文件传入（不经 shell 引号拼接）；log/die 与 run_batch.sh 内联版同语义。
    # 注意：driver 含字面 `%s`（bash printf），不能用 % 格式化（相邻字面量会先拼接）。
    driver = (
        "log() { printf 'LOG %s\\n' \"$*\"; }\n"
        "die() { printf 'DIE %s\\n' \"$*\" >&2; exit 2; }\n"
        "selected=()\n"
        "while IFS= read -r _r; do\n"
        "    [ -n \"$_r\" ] && selected+=(\"$_r\")\n"
        "done < '" + rows_path + "'\n"
        "source '" + block_path + "'\n"
    )
    with open(driver_path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write(driver)
    env = dict(os.environ)
    env["PY"] = sys.executable.replace(os.sep, "/")
    env["CARD_WINDOW"] = CARD_WINDOW_PATH.replace(os.sep, "/")
    try:
        r = subprocess.run([bash, driver_path], cwd=REPO_ROOT, env=env,
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

        # ---- T5：跑后对称性复核按清单 profile 去重（卡 openmir2-postrun-drift-tooth-t5）----
        block_text = None
        try:
            block_text = extract_postrun_block()
        except RuntimeError as ex:
            check("T5.1 提取块含 POSTRUN_PROFILES 接线", False, str(ex))
        if block_text is not None:
            check("T5.1 提取块含 POSTRUN_PROFILES 接线", "POSTRUN_PROFILES" in block_text,
                  "块里没有 POSTRUN_PROFILES（复核块的 profile 去重接线不见了）")
            try:
                profs = synthesize_postrun_profiles(tmp)
            except Exception as ex:  # noqa: BLE001
                profs = None
                check("T5.2 去重行 = pa/pb/pc 首现序、恰一次", False,
                      "合成 profile 失败：%s: %s" % (type(ex).__name__, ex))
                for nm in ("T5.3 pa（两假根同内容）恰一次且 OK",
                           "T5.4 pb（List.txt 漂移）恰一次且 DRIFT 点名 Castle/List.txt",
                           "T5.5 pc（Envir 漂移）恰一次且 DRIFT 点名 Envir/extra.txt",
                           "T5.6 profile 复核行总数恰 3（多刷/漏刷都红）",
                           "T5.7 空选中 ⇒ die rc=2",
                           "T5.8 坏 profile ⇒ ERROR 行点名",
                           "T5.9 坏 profile 不断链：排其后的 pa 仍复核且 OK"):
                    check(nm, False, "合成 profile 失败，断言未执行")
            if profs is not None:
                pa, pb, pc = profs["pa"], profs["pb"], profs["pc"]
                # 选中行：2x pa + 1x pb + 1x pc ⇒ 去重后 pa/pb/pc 各一次（首现序）。
                us = "\x1f"

                def _row(n, name, prof):
                    return us.join((n, name, prof, "", "0", "-"))

                rc1, out1, err1 = run_postrun_block(BASH, tmp, block_text,
                                                    [_row("1", "prof_a", pa),
                                                     _row("2", "prof_a2", pa),
                                                     _row("3", "prof_b", pb),
                                                     _row("4", "prof_c", pc)])
                dedup_line = "跑后对称性 profile（按选中行去重）：%s %s %s" % (pa, pb, pc)
                check("T5.2 去重行 = pa/pb/pc 首现序、恰一次",
                      rc1 == 0 and out1.count(dedup_line) == 1,
                      "rc=%r 去重行出现 %d 次 stderr=%s" % (rc1, out1.count(dedup_line), err1[-300:]))
                pa_ok = "跑后对称性[%s]：OK" % pa
                check("T5.3 pa（两假根同内容）恰一次且 OK", out1.count(pa_ok) == 1,
                      "出现 %d 次：%s" % (out1.count(pa_ok), out1[-400:]))
                pb_lines = [l for l in out1.splitlines()
                            if l.startswith("LOG 跑后对称性[%s]：DRIFT" % pb)]
                check("T5.4 pb（List.txt 漂移）恰一次且 DRIFT 点名 Castle/List.txt",
                      len(pb_lines) == 1 and "Castle/List.txt" in pb_lines[0],
                      "DRIFT 行 %d 条：%r" % (len(pb_lines), pb_lines))
                pc_lines = [l for l in out1.splitlines()
                            if l.startswith("LOG 跑后对称性[%s]：DRIFT" % pc)]
                check("T5.5 pc（Envir 漂移）恰一次且 DRIFT 点名 Envir/extra.txt",
                      len(pc_lines) == 1 and "Envir/extra.txt" in pc_lines[0],
                      "DRIFT 行 %d 条：%r" % (len(pc_lines), pc_lines))
                check("T5.6 profile 复核行总数恰 3（多刷/漏刷都红）",
                      out1.count("跑后对称性[") == 3,
                      "复核行 %d 条：%s" % (out1.count("跑后对称性["), out1[-400:]))
                # 负例一：空选中 ⇒ 块内 die rc=2（不许静默跳过）。
                rc2, _o2, e2 = run_postrun_block(BASH, tmp, block_text, [])
                check("T5.7 空选中 ⇒ die rc=2", rc2 == 2, "rc=%r stderr=%s" % (rc2, e2[-300:]))
                # 负例二：坏 profile ⇒ ERROR 行点名，且不断链（排其后的 pa 仍复核）。
                rc3, out3, _e3 = run_postrun_block(BASH, tmp, block_text,
                                                   [_row("1", "bad", "no-such-profile"),
                                                    _row("2", "prof_a", pa)])
                check("T5.8 坏 profile ⇒ ERROR 行点名",
                      "跑后对称性[no-such-profile]：ERROR" in out3,
                      "out=%s" % out3[-400:])
                check("T5.9 坏 profile 不断链：排其后的 pa 仍复核且 OK",
                      out3.count("跑后对称性[%s]：OK" % pa) == 1,
                      "out=%s" % out3[-400:])

        # ---- T9–T12：二进制身份 build.json（卡 openmir2-harness-stop-and-evidence）----
        # WINDOW_BUILD_STAMP 覆盖 stamp 路径（临时文件，不碰真实 target/）；
        # --dry-run 下 build.json 落 `<run-dir>-dry-run/build.json`（与真跑的 <run_dir>/ 同名同义）。
        stamp_ok2 = os.path.join(tmp, "ok.build.json")
        with open(stamp_ok2, "w", encoding="utf-8", newline="\n") as fh:
            fh.write(json.dumps({"head": "a" * 40, "sha256": "b" * 64,
                                 "dirty": "", "built_at": "2026-10-29T00:00:00"}))
        stamp_missing2 = os.path.join(tmp, "missing.build.json")

        # T9：R 级 + stamp 存在 ⇒ rc=0 且 build.json 落盘（内容=stamp）。
        rd9 = os.path.join(tmp, "t9")
        os.environ["WINDOW_BUILD_STAMP"] = stamp_ok2
        try:
            rc, out, err = run_batch(["--dry-run", "--no-preflight", "--tier", "R",
                                      "--run-dir", rd9], with_password=False)
            build9 = os.path.join(rd9 + "-dry-run", "build.json")
            content9 = open(build9, encoding="utf-8").read() if os.path.isfile(build9) else ""
            check("T9 R 级 stamp 存在 ⇒ rc=0 且 <dry-run>/build.json 落盘（含 head/sha256）",
                  rc == 0 and '"head"' in content9 and '"sha256"' in content9,
                  "rc=%r build9=%s" % (rc, content9[:200]))
        finally:
            os.environ.pop("WINDOW_BUILD_STAMP", None)

        # T10：R 级 + stamp 缺失 ⇒ rc=2 且点名 binary_identity=missing（不许 PASS）。
        rd10 = os.path.join(tmp, "t10")
        os.environ["WINDOW_BUILD_STAMP"] = stamp_missing2
        try:
            rc, out, err = run_batch(["--dry-run", "--no-preflight", "--tier", "R",
                                      "--run-dir", rd10], with_password=False)
            check("T10 R 级 stamp 缺失 ⇒ rc=2 且点名 binary_identity=missing（die 走 stderr）",
                  rc == 2 and "binary_identity=missing" in (out + err),
                  "rc=%r out=%s err=%s" % (rc, out[-300:], err[-300:]))
        finally:
            os.environ.pop("WINDOW_BUILD_STAMP", None)

        # T11：L 级（默认）+ stamp 缺失 ⇒ rc=0（仅标记、不阻断）且 build.json 写 missing。
        rd11 = os.path.join(tmp, "t11")
        os.environ["WINDOW_BUILD_STAMP"] = stamp_missing2
        try:
            rc, out, err = run_batch(["--dry-run", "--no-preflight",
                                      "--run-dir", rd11], with_password=False)
            build11 = os.path.join(rd11 + "-dry-run", "build.json")
            content11 = open(build11, encoding="utf-8").read() if os.path.isfile(build11) else ""
            check("T11 L 级 stamp 缺失 ⇒ rc=0（仅标记）且 stdout 点名 binary_identity=missing",
                  rc == 0 and "binary_identity=missing" in out, "rc=%r out=%s" % (rc, out[-400:]))
            check("T11 L 级 stamp 缺失 ⇒ build.json 写 binary_identity=missing",
                  '"binary_identity": "missing"' in content11, content11[:200])
        finally:
            os.environ.pop("WINDOW_BUILD_STAMP", None)

        # T12：L 级 + stamp 存在 ⇒ rc=0 且 build.json 就是 stamp 原文拷贝。
        rd12 = os.path.join(tmp, "t12")
        os.environ["WINDOW_BUILD_STAMP"] = stamp_ok2
        try:
            rc, out, err = run_batch(["--dry-run", "--no-preflight",
                                      "--run-dir", rd12], with_password=False)
            build12 = os.path.join(rd12 + "-dry-run", "build.json")
            same12 = (open(build12, "rb").read() == open(stamp_ok2, "rb").read()
                      if os.path.isfile(build12) else False)
            check("T12 L 级 stamp 存在 ⇒ rc=0 且 build.json=stamp 原样拷贝", rc == 0 and same12,
                  "rc=%r" % (rc,))
        finally:
            os.environ.pop("WINDOW_BUILD_STAMP", None)
    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    failed = [n for n, ok, _ in results if not ok]
    print("%s: run_batch_selftest %d/%d" % ("GREEN" if not failed else "RED",
                                            len(results) - len(failed), len(results)))
    return 0 if not failed else 1


if __name__ == "__main__":
    sys.exit(main())
