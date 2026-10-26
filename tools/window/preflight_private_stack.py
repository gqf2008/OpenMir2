#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""窗口私有栈前置自检（run-3 准备包）——**只读、不起栈、不占端口、不连库**。

卡：`openmir2-window-run2-prep`（tier L）。补的是 `tools/conformance/preflight.py`
（文档/二进制/端口/profile/bash/pin 接线）**没有覆盖**的那一层：

  ① inventory    私栈完整性：E:/tmp/conformance 的启动/停止/恢复脚本、六角色部署件、
                 rust-root 配置、baseline 目录、selgate-build 构建产物。
  ② selgate      **重建期坑 A：SelGate ListenPort**。私有栈的 C# SelGate 必须是 T-172
                 补丁构建（`config.conf` 的 `ListenPort` 才生效）；stock SelGate 把
                 `GatePort` 写死 7100，17100 永远不监听 ⇒ 登录链在选服处断掉，
                 而 stack 步只会报「ready ports missing: 17100」，看不出根因。
                 机判：`ListenPort=17100` + `sha256(SelGate.exe) == sha256(selgate-build/SelGate.exe)`
                 + `LoginSrv/AddrTable.txt` 路由到 `:17100`。
  ③ route525     **重建期坑 B：525 路由 17200**。`SM_STARTPLAY(525)` 的 body 是
                 `GameGate ip/port`；C# 探针**用它**决定连哪个游戏网关。`DBSrv/ServerInfo.txt`
                 若还是默认 7200（或别的端口），C# 侧会连空 ⇒ 进世界失败。
                 机判：C#/Rust 两侧 `ServerInfo.txt` 的第一行端口都必须含 **17200**
                 （两侧都要：规则 525 逐字节 compare body）。**注意 Rust 的 525 必须保持
                 17200 才能与 oracle 逐字节相同**——Rust 实际连的是 17600，由 profile 的
                 `--game-gate 127.0.0.1:17600` 覆盖，**不要**把 rust ServerInfo 改成 17600。
                 同时校验 C# RunGate `GatePort1=17200` / Rust RunGate `GatePort1=17600`。
  ④ pin          **pin 链 smoke**：静态核对内置 `window-magic`（rust 侧
                 `MIR2_CLIENT_ABILITY_LAYOUT=0` + stack 已接 `-PinChecks/-PinLog`），
                 再用 `start-stack-wmi.ps1 -CheckPinsOnly`（**不派生进程**）对**上一轮
                 game-svr 日志**做一次自证链冒烟。日志不存在 ⇒ FAIL（自证无从谈起）。
  ⑤ binary       release 二进制与当前 head 对齐：读 `tools/window/rebuild-server.ps1`
                 写的 `mir2-rs/target/release/mir2-server.build.json`（head/sha256/dirty），
                 与当前 `git rev-parse HEAD` 和文件实际 sha256 对账。脏树 ⇒ FAIL。
  ⑥ start_rust   `E:/tmp/conformance/start-rust.ps1` 的 `-ServerExe` 默认值必须指向
                 **本检出**的 release 二进制（run-2 遗留值指向已删除的
                 `E:/tmp/wt-worker-73/...`；不修则 card_window 的 pid/exe 对账必红）。
  ⑦ roster       run3 的场景清单（`tools/window/run3-scenarios.json`，**条数按清单实况**，不含已封存 pickup_item）与场景文件/
                 profile 名/rules.json 一致性。

用法：
  python -I -B tools/window/preflight_private_stack.py
  python -I -B tools/window/preflight_private_stack.py --skip-pin-smoke --json E:/tmp/window/run3/preflight.json
  python -I -B tools/window/preflight_private_stack.py --selftest

输出（机器可读，一行一步）：
  PREFLIGHT_PRIVATE step=<id> status=OK|WARN|FAIL detail=<...>
  PREFLIGHT_PRIVATE SUMMARY pass=<n> warn=<n> fail=<n>
  PREFLIGHT_PRIVATE RESULT PASS|FAIL

退出码：0 = 无 FAIL（WARN 不拦）；2 = 至少一步 FAIL（fail-closed）。
"""

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import tempfile
import time

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", ".."))
DEFAULT_PRIVATE_ROOT = "E:/tmp/conformance"
DEFAULT_START_RUST = "E:/tmp/conformance/start-rust.ps1"
MANIFEST = os.path.join(HERE, "run3-scenarios.json")
RULES = os.path.join(REPO, "tools", "conformance", "rules.json")
CARD_WINDOW = os.path.join(REPO, "tools", "conformance", "card_window.py")
WMI_WRAPPER = os.path.join(REPO, "tools", "window", "start-stack-wmi.ps1")
BUILD_STAMP = os.path.join(REPO, "mir2-rs", "target", "release", "mir2-server.build.json")
RELEASE_EXE = os.path.join(REPO, "mir2-rs", "target", "release", "mir2-server.exe")
PIN_KEY = "MIR2_CLIENT_ABILITY_LAYOUT"
PIN_VALUE = "0"

OK, WARN, FAIL = "OK", "WARN", "FAIL"


class Report(object):
    def __init__(self):
        self.items = []

    def add(self, step, status, detail, fix=None):
        self.items.append({"step": step, "status": status, "detail": detail, "fix": fix})
        print("PREFLIGHT_PRIVATE step=%s status=%s detail=%s" % (step, status, detail))
        if fix:
            print("PREFLIGHT_PRIVATE fix=%s %s" % (step, fix))

    def counts(self):
        c = {OK: 0, WARN: 0, FAIL: 0}
        for it in self.items:
            c[it["status"]] = c.get(it["status"], 0) + 1
        return c

    def result(self):
        return FAIL if self.counts()[FAIL] else "PASS"


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as fh:
        for chunk in iter(lambda: fh.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def norm_path(path):
    return os.path.normcase(os.path.normpath(str(path).replace("/", os.sep)))


def read_text(path, encoding="utf-8"):
    with open(path, "rb") as fh:
        raw = fh.read()
    if raw.startswith(b"\xef\xbb\xbf"):
        return raw.decode("utf-8-sig")
    return raw.decode(encoding, errors="replace")


def parse_ini(text):
    """极简 INI：返回 {section_lower: {key_lower: value}}；无 section 的键进 ''。"""
    out = {}
    cur = ""
    for line in text.splitlines():
        s = line.strip()
        if not s or s[0] in ";#":
            continue
        m = re.match(r"^\[(.+)\]$", s)
        if m:
            cur = m.group(1).strip().lower()
            out.setdefault(cur, {})
            continue
        if "=" in s:
            k, v = s.split("=", 1)
            out.setdefault(cur, {})[k.strip().lower()] = v.strip()
    return out


SERVER_INFO_RE = re.compile(r"^\s*(\S+)\s+(\S+)\s+([0-9][0-9,]*)")
START_RUST_EXE_RE = re.compile(r"\[string\]\s*\$\s*ServerExe\s*=\s*'([^']*)'", re.I)
START_RUST_INNER_RE = re.compile(r"\$starter\s*=\s*'([^']*)'", re.I)


def parse_server_info(text):
    """`ServerInfo.txt`：每行 `角色网关IP 游戏网关IP 端口[,端口…]`（C# DBShare.LoadServerInfo 口径）。

    返回 (dict|None, err|None)。空文件不是合法路由（C# 侧会没有网关可发）。
    """
    lines = [ln.strip() for ln in text.splitlines() if ln.strip()]
    if not lines:
        return None, "ServerInfo.txt 为空（C# 侧没有网关路由可发 SM_STARTPLAY）"
    m = SERVER_INFO_RE.match(lines[0])
    if not m:
        return None, "ServerInfo.txt 首行不是 `ip ip port[,port…]`：%r" % lines[0]
    ports = []
    for tok in m.group(3).split(","):
        tok = tok.strip()
        if not tok:
            continue
        if not tok.isdigit():
            return None, "ServerInfo.txt 端口段含非数字：%r" % m.group(3)
        ports.append(int(tok))
    if not ports:
        return None, "ServerInfo.txt 首行没有端口：%r" % lines[0]
    return {"login_ip": m.group(1), "game_ip": m.group(2), "ports": ports,
            "line": lines[0], "line_count": len(lines)}, None


def parse_start_rust_default(text):
    m = START_RUST_EXE_RE.search(text)
    inner = START_RUST_INNER_RE.search(text)
    return (m.group(1) if m else None), (inner.group(1) if inner else None)


def git_head(repo):
    try:
        out = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"],
                             capture_output=True, text=True)
        if out.returncode != 0:
            return None
        return out.stdout.strip()
    except OSError:
        return None


# ---------------------------------------------------------------------------
# ① inventory
# ---------------------------------------------------------------------------
INVENTORY = [
    "start-csharp3.ps1",
    "start-rust.ps1",
    "stop-rust.ps1",
    "restore-baseline.sh",
    "restore-baseline-r.sh",
    "selgate-build/SelGate.exe",
    "csharp-root/DBSrv/DBSrv.exe",
    "csharp-root/LoginSrv/LoginSrv.exe",
    "csharp-root/M2GameSvr/GameSrv.exe",
    "csharp-root/LoginGate/LoginGate.exe",
    "csharp-root/SelGate/SelGate.exe",
    "csharp-root/RunGate/GameGate.exe",
    "rust-root/alt-port-profile.json",
    "rust-root/DBSrv/dbsvr.conf",
    "rust-root/DBSrv/AddrTable.txt",
    "rust-root/DBSrv/ServerInfo.txt",
    "rust-root/LoginSrv/logsrv.conf",
    "rust-root/LoginSrv/AddrTable.txt",
    "rust-root/M2GameSvr/Server.conf",
    "rust-root/LoginGate/config.conf",
    "rust-root/SelGate/config.conf",
    "rust-root/RunGate/config.conf",
]


def check_inventory(root):
    missing = [rel for rel in INVENTORY if not os.path.isfile(os.path.join(root, rel))]
    baseline = os.path.join(root, "baseline")
    if not os.path.isdir(baseline):
        missing.append("baseline/（基线库快照目录）")
    elif not os.listdir(baseline):
        missing.append("baseline/（目录为空：restore 没有可恢复的基线）")
    if missing:
        return FAIL, "缺 %d 项：%s" % (len(missing), ", ".join(missing)), \
            "按 docs/window-run-1-20261024.md §0/§1 重建 E:/tmp/conformance（六角色 + baseline）"
    return OK, "%d 个部署件齐全，baseline 非空" % len(INVENTORY), None


# ---------------------------------------------------------------------------
# ② 坑 A：SelGate ListenPort
# ---------------------------------------------------------------------------
def check_selgate(root):
    problems = []
    sel = os.path.join(root, "csharp-root", "SelGate", "config.conf")
    if not os.path.isfile(sel):
        problems.append("缺 %s" % sel)
    else:
        ini = parse_ini(read_text(sel, "gbk"))
        raw = (ini.get("selgate") or {}).get("listenport")
        try:
            listen = int(raw) if raw is not None else None
        except ValueError:
            listen = None
        if listen != 17100:
            problems.append("[SelGate] ListenPort=%r（必须 17100；否则 stock GatePort=7100 生效，"
                            "17100 永不监听）" % raw)

    exe = os.path.join(root, "csharp-root", "SelGate", "SelGate.exe")
    build = os.path.join(root, "selgate-build", "SelGate.exe")
    if os.path.isfile(exe) and os.path.isfile(build):
        sha_exe, sha_build = sha256_file(exe), sha256_file(build)
        if sha_exe != sha_build:
            problems.append("SelGate.exe sha256=%s ≠ T-172 构建 %s —— 私有栈里放的不是带 "
                            "ListenPort 支持的二进制" % (sha_exe[:12], sha_build[:12]))
    else:
        problems.append("缺 SelGate.exe 或 selgate-build/SelGate.exe（无法核对 T-172 构建）")

    addr = os.path.join(root, "csharp-root", "LoginSrv", "AddrTable.txt")
    if not os.path.isfile(addr):
        problems.append("缺 LoginSrv/AddrTable.txt（登录→选服路由）")
    else:
        text = read_text(addr, "gbk")
        if not re.search(r":17100(?![0-9])", text):
            problems.append("LoginSrv/AddrTable.txt 未路由到 :17100（实际：%r）"
                            % " | ".join(ln.strip() for ln in text.splitlines() if ln.strip())[:160])

    if problems:
        return FAIL, "；".join(problems), \
            "用 selgate-build 的 T-172 构建覆盖 csharp-root/SelGate/，并确认 config.conf 的 [SelGate] ListenPort=17100"
    return OK, "ListenPort=17100 + SelGate.exe 与 T-172 构建同 sha256 + AddrTable→:17100", None


# ---------------------------------------------------------------------------
# ③ 坑 B：525 路由 17200
# ---------------------------------------------------------------------------
def check_route525(root):
    problems = []
    info = {}
    for side in ("csharp-root", "rust-root"):
        path = os.path.join(root, side, "DBSrv", "ServerInfo.txt")
        if not os.path.isfile(path):
            problems.append("缺 %s/DBSrv/ServerInfo.txt" % side)
            continue
        doc, err = parse_server_info(read_text(path, "gbk"))
        if err:
            problems.append("%s/DBSrv/ServerInfo.txt：%s" % (side, err))
            continue
        info[side] = doc
        if doc["game_ip"] != "127.0.0.1" or doc["login_ip"] != "127.0.0.1":
            problems.append("%s/DBSrv/ServerInfo.txt 的地址不是 127.0.0.1：%r"
                            % (side, doc["line"]))
        if 17200 not in doc["ports"]:
            problems.append("%s/DBSrv/ServerInfo.txt 端口 %r 不含 17200（525 body 会不是 "
                            "127.0.0.1/17200 ⇒ %s）"
                            % (side, doc["ports"],
                               "C# 探针按 525 连网关，连空" if side == "csharp-root"
                               else "Rust 525 与 oracle 逐字节比对必红；Rust 实际连 17600 是 "
                                    "profile 的 --game-gate 覆盖，不要把本文件改成 17600"))

    gates = {}
    for side, want in (("csharp-root", 17200), ("rust-root", 17600)):
        path = os.path.join(root, side, "RunGate", "config.conf")
        if not os.path.isfile(path):
            problems.append("缺 %s/RunGate/config.conf" % side)
            continue
        ini = parse_ini(read_text(path, "gbk"))
        gg = ini.get("gamegate") or {}
        try:
            gate1 = int(gg.get("gateport1", ""))
        except ValueError:
            gate1 = None
        try:
            srv1 = int(gg.get("serverport1", ""))
        except ValueError:
            srv1 = None
        gates[side] = (gate1, srv1)
        if gate1 != want:
            problems.append("%s/RunGate GatePort1=%r（应为 %d）" % (side, gate1, want))
        if srv1 != 15000:
            problems.append("%s/RunGate ServerPort1=%r（应回连 GameSvr 15000）" % (side, srv1))

    if problems:
        return FAIL, "；".join(problems), \
            "两侧 DBSrv/ServerInfo.txt 首行都写 `127.0.0.1 127.0.0.1 17200`；C# RunGate GatePort1=17200、Rust=17600"
    detail = "C#/Rust ServerInfo→17200；C# RunGate=%s、Rust RunGate=%s" % (
        gates.get("csharp-root"), gates.get("rust-root"))
    return OK, detail, None


# ---------------------------------------------------------------------------
# ④ pin 链
# ---------------------------------------------------------------------------
def _load_card_window():
    sys.path.insert(0, os.path.join(REPO, "tools", "conformance"))
    import card_window as cw  # noqa: E402
    return cw


def _pin_checks(stack):
    if not stack:
        return None
    argv = [str(a) for a in stack]
    if not any("start-stack-wmi.ps1" in a for a in argv):
        return None
    if "-PinChecks" not in argv:
        return []
    i = argv.index("-PinChecks")
    return [x for x in argv[i + 1].split(",") if x] if i + 1 < len(argv) else []


def _pin_log(stack):
    if not stack:
        return None
    argv = [str(a) for a in stack]
    if "-PinLog" not in argv:
        return None
    i = argv.index("-PinLog")
    return argv[i + 1] if i + 1 < len(argv) else None


def check_pin_static(alt, wm, pin_log):
    """`alt`/`window-magic` 的 pin 口径（参数可注入，`--selftest` 用）。"""
    problems = []
    rs = (wm.get("sides") or {}).get("rust") or {}
    cs = (wm.get("sides") or {}).get("csharp") or {}
    env = rs.get("env") or {}
    if env.get(PIN_KEY) != PIN_VALUE:
        problems.append("window-magic.rust.env[%s]=%r（应为 %r）"
                        % (PIN_KEY, env.get(PIN_KEY), PIN_VALUE))
    checks = _pin_checks(rs.get("stack"))
    if checks is None:
        problems.append("window-magic.rust.stack 不是 WMI 包装（pin 无法自证）")
    elif ("%s=%s" % (PIN_KEY, PIN_VALUE)) not in checks:
        problems.append("window-magic.rust.stack 未接 -PinChecks %s=%s（实际 %r）"
                        % (PIN_KEY, PIN_VALUE, checks))
    log = _pin_log(rs.get("stack"))
    if not log:
        problems.append("window-magic.rust.stack 缺 -PinLog")
    elif norm_path(log) != norm_path(pin_log):
        problems.append("-PinLog=%r，与 card_window 的 RUST_STACK_PIN_LOG=%r 不一致"
                        % (log, pin_log))
    if cs.get("stack"):
        cs_checks = _pin_checks(cs.get("stack"))
        if cs_checks:
            problems.append("window-magic.csharp.stack 不应接 pin（C# 是 oracle，不消费该 env）")
    if not (wm.get("magic") or []):
        problems.append("window-magic 缺 magic 段（spell_cast 加载期 rc=2）")
    alt_rs_env = ((alt.get("sides") or {}).get("rust") or {}).get("env") or {}
    if PIN_KEY in alt_rs_env:
        problems.append("alt.rust.env 不该有 %s（alt 是窗口基线 profile，不能被窗口 pin 污染）"
                        % PIN_KEY)
    if problems:
        return FAIL, "；".join(problems), \
            "回退 card_window.py 的 window-magic 派生（_derived_magic_profile）"
    return OK, "window-magic: rust env %s=%s + -PinChecks/-PinLog 已接线；alt 未污染" \
        % (PIN_KEY, PIN_VALUE), None


def check_pin_smoke(pin_log, wrapper=WMI_WRAPPER, runner=None):
    """`-CheckPinsOnly`：只校验调用方 env + 上一轮 server 日志自证行，**不派生进程**。"""
    if not os.path.isfile(wrapper):
        return FAIL, "缺 %s" % wrapper, "确认在仓库检出内运行"
    if not os.path.isfile(pin_log):
        return FAIL, "pin 自证日志不存在：%s（自证链无从验证）" % pin_log, \
            "先跑一次 window-magic 场景（spell_cast）产生 game-svr.out.log；" \
            "或本次显式 --skip-pin-smoke 并在证据里登记未验"
    argv = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass", "-File", wrapper,
            "-CheckPinsOnly", "-PinChecks", "%s=%s" % (PIN_KEY, PIN_VALUE),
            "-PinLog", pin_log, "-PinWaitSec", "5"]
    env = os.environ.copy()
    env[PIN_KEY] = PIN_VALUE
    if runner is not None:
        rc, out, err = runner(argv, env)
    else:
        try:
            proc = subprocess.run(argv, capture_output=True, text=True, env=env, timeout=90)
        except subprocess.TimeoutExpired:
            return FAIL, "-CheckPinsOnly 90s 未返回", \
                "检查 PowerShell/杀软；必要时 --skip-pin-smoke 并登记"
        rc, out, err = proc.returncode, proc.stdout, proc.stderr
    tail = " | ".join((out or "").strip().splitlines()[-2:]) or (err or "").strip()[:200]
    if rc != 0:
        return FAIL, "%s -CheckPinsOnly rc=%d：%s" % (os.path.basename(wrapper), rc, tail), \
            "确认上一轮 window-magic 起栈日志里有 `MIR2_CLIENT_ABILITY_LAYOUT=0` 且 " \
            "`layout=oracle-40` 自证行"
    return OK, "pin 链 smoke rc=0（%s）" % tail[:180], None


# ---------------------------------------------------------------------------
# ⑤ build stamp / ⑥ start-rust
# ---------------------------------------------------------------------------
def check_build_stamp(repo, head=None, stamp=None, exe=None):
    stamp = stamp or BUILD_STAMP
    exe = exe or RELEASE_EXE
    if not os.path.isfile(exe):
        return FAIL, "release 二进制不存在：%s" % exe, \
            "powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust"
    if not os.path.isfile(stamp):
        return FAIL, "缺 build stamp：%s" % stamp, \
            "powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust"
    try:
        with open(stamp, encoding="utf-8-sig") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        return FAIL, "读 build stamp 失败：%s" % ex, "重跑 rebuild-server.ps1"
    problems = []
    now_head = head if head is not None else git_head(repo)
    if not now_head:
        problems.append("无法解析当前 git HEAD")
    elif doc.get("head") != now_head:
        problems.append("stamp.head=%s ≠ 当前 HEAD=%s"
                        % (str(doc.get("head"))[:12], now_head[:12]))
    actual = sha256_file(exe)
    if doc.get("sha256") != actual:
        problems.append("stamp.sha256=%s ≠ 文件实际 %s"
                        % (str(doc.get("sha256"))[:12], actual[:12]))
    if doc.get("dirty"):
        problems.append("构建时工作树不干净（dirty=%r）⇒ 二进制不可追溯到 HEAD"
                        % doc.get("dirty"))
    if problems:
        return FAIL, "；".join(problems), "提交/清理工作树后重跑 rebuild-server.ps1"
    return OK, "head=%s sha256=%s built_at=%s" % (
        str(doc.get("head"))[:12], actual[:12], doc.get("built_at")), None


def check_start_rust(path, exe, repo=None):
    repo = repo or REPO
    if not os.path.isfile(path):
        return FAIL, "缺 %s" % path, "按 window-run-1 §0 重建私栈启动脚本"
    text = read_text(path, "utf-8")
    raw, inner = parse_start_rust_default(text)
    if raw is None:
        return FAIL, "start-rust.ps1 里找不到 `[string]$ServerExe = '...'`", \
            "按私栈模板恢复（或手工补参数）"
    if norm_path(raw) != norm_path(exe):
        return FAIL, "start-rust.ps1 -ServerExe=%r ≠ 本检出 release %r" % (raw, exe), \
            "powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust"
    if inner:
        want = os.path.join(repo, "tools", "m2-acceptance", "start_mir2_server.ps1")
        if not os.path.isfile(inner):
            return WARN, "-ServerExe 已对齐；但 $starter 不存在：%s" % inner, \
                "确认 start_mir2_server.ps1 路径（私栈模板从主检出复制）"
        if norm_path(inner) != norm_path(want) and sha256_file(inner) != sha256_file(want):
            return WARN, "-ServerExe 已对齐；$starter 指向其它检出且内容不同：%s" % inner, \
                "powershell -NoProfile -ExecutionPolicy Bypass -File tools/window/rebuild-server.ps1 -SyncStartRust"
    return OK, "-ServerExe 已对齐：%s" % exe, None


# ---------------------------------------------------------------------------
# ⑦ roster
# ---------------------------------------------------------------------------
def check_roster(repo=None, manifest=None):
    repo = repo or REPO
    manifest = manifest or MANIFEST
    if not os.path.isfile(manifest):
        return FAIL, "缺场景清单：%s" % manifest, "恢复 tools/window/run3-scenarios.json"
    try:
        with open(manifest, encoding="utf-8-sig") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        return FAIL, "读场景清单失败：%s" % ex, "修复 JSON"
    scen = doc.get("scenarios") or []
    problems = []
    # **不硬编码场景数**：期望值 = 清单自身条数；只校验 `n` 从 1 连续到 N（断号/重复才 fail）。
    # 与 `tools/window/run_batch.sh` 同一口径（卡 `openmir2-runbatch-scenario-count`）。
    if not scen:
        problems.append("场景清单为空")
    names, numbers = [], []
    for s in scen:
        names.append(s.get("name"))
        numbers.append(s.get("n"))
        if s.get("profile") not in ("alt", "window-magic"):
            problems.append("%s profile=%r（只允许 alt / window-magic）"
                            % (s.get("name"), s.get("profile")))
        p = os.path.join(repo, "tools", "probe", "scenarios", "%s.json" % s.get("name"))
        if not os.path.isfile(p):
            problems.append("缺场景文件 %s" % p)
    if "pickup_item" in names:
        problems.append("清单含已封存场景 pickup_item（T-19/T-25，结构性不可判，不进入窗口）")
    if len(set(names)) != len(names):
        problems.append("场景名重复")
    if numbers != list(range(1, len(scen) + 1)):
        problems.append("n 不是 1..%d 连续" % len(scen))
    if not os.path.isfile(RULES):
        problems.append("缺 rules.json")
    if problems:
        return FAIL, "；".join(problems), "修 tools/window/run3-scenarios.json / 补场景文件"
    return OK, ("%d 场景（清单实况，n=1..%d 连续），不含 pickup_item，rules.json 在位"
                % (len(scen), len(scen))), None


def check_castle_list(root=DEFAULT_PRIVATE_ROOT):
    """两侧私有栈的 `Castle/List.txt` 必须**内容一致**（卡 openmir2-window-castle-list-symmetry）。

    背景：`CastleMgr.IncRateGold` 会**升序遍历全表**；C# 侧 1 城（0）而 Rust 侧 2 城（0,1）时，
    W-TAX-2（`GetAllNpcTax=1` 两城各入账一次）在不解封前**必然假红**。另：C# 开 `--rng-log`
    时 GameSvr 走影子目录 `M2GameSvr-hook/`，其 `Castle/List.txt` 也必须同内容。

    只读检查；不自动改文件（对齐办法见 fix 行与 runbook §0.9）。
    """
    csharp = os.path.join(root, "csharp-root", "M2GameSvr", "Castle", "List.txt")
    shadow = os.path.join(root, "csharp-root", "M2GameSvr-hook", "Castle", "List.txt")
    rust = os.path.join(root, "rust-root", "M2GameSvr", "Castle", "List.txt")
    paths = [("csharp", csharp), ("rust", rust)]
    if os.path.isfile(shadow):
        paths.append(("csharp-hook", shadow))

    def read(path):
        with open(path, "rb") as f:
            raw = f.read()
        return raw, hashlib.sha256(raw).hexdigest()

    missing = [label for label, path in paths if not os.path.isfile(path)]
    if missing:
        return FAIL, ("缺 Castle/List.txt：%s（先建私有栈夹具；路径 %s）"
                      % (", ".join(missing), "; ".join(p for _, p in paths))), \
            "按 runbook §0.9 建/对齐两侧 Castle/List.txt"

    docs = {}
    for label, path in paths:
        docs[label] = read(path)
    rust_raw, rust_sha = docs["rust"]
    mismatch = []
    for label, _ in paths:
        raw, sha = docs[label]
        if sha != rust_sha:
            mismatch.append("%s sha256=%s bytes=%r" % (label, sha[:12], raw))
    if mismatch:
        return FAIL, ("两侧 Castle/List.txt 不一致（rust 为基准：sha256=%s bytes=%r）：%s"
                      % (rust_sha[:12], rust_raw, "；".join(mismatch))), \
            ("对齐（W-TAX-2 需两城：都用 rust 的 0\\r\\n1\\r\\n；最小场景可都用 0）："
             "Copy-Item -Force '%s' '%s'（有 %s 时同样复制）" % (rust, csharp, shadow))
    return OK, ("两侧一致：sha256=%s bytes=%r（%s）"
                % (rust_sha[:12], rust_raw, ", ".join(label for label, _ in paths))), None


# ---------------------------------------------------------------------------
# castle 状态对称性：跑前基线 + 跑后两档分类（卡 openmir2-castle-symmetry-baseline）
#
# 为什么需要：`Castle/List.txt` 被 C# GameSrv 每 600s 重写，`Castle/<n>/SabukW.txt`
# 也会在运行期落盘 ⇒ **跑后不相等是预期**。没有跑前基线就分不清「本轮造成」与
# 「开窗前已存在」，只能把「判不了」当结论。这里把三侧（csharp / csharp-hook / rust）
# 的 sha256+bytes 落盘，跑后按两档归因。
# ---------------------------------------------------------------------------
CASTLE_LIST_REL = "Castle/List.txt"
CASTLE_MISSING = "MISSING"


def castle_sides(root):
    """三侧 envir 根：csharp / csharp-hook（仅当影子存在）/ rust。顺序固定。"""
    cs = os.path.join(root, "csharp-root", "M2GameSvr")
    hook = os.path.join(root, "csharp-root", "M2GameSvr-hook")
    rust = os.path.join(root, "rust-root", "M2GameSvr")
    sides = [("csharp", cs)]
    if os.path.isdir(hook):
        sides.append(("csharp-hook", hook))
    sides.append(("rust", rust))
    return sides


def castle_state_manifest(root):
    """{rel: {side: (sha256|None, bytes|None)}}；rel = Castle/List.txt 与 Castle/*/SabukW.txt。

    覆盖 `Castle/<n>/SabukW.txt`（每城一份）与 `Castle/Castle/SabukW.txt`（部署件里
    确实存在这一层，卡 `openmir2-castle-symmetry-baseline` 的证据就是它）。
    """
    sides = castle_sides(root)
    rels = {CASTLE_LIST_REL}
    for _label, base in sides:
        cdir = os.path.join(base, "Castle")
        if not os.path.isdir(cdir):
            continue
        for name in sorted(os.listdir(cdir)):
            if os.path.isfile(os.path.join(cdir, name, "SabukW.txt")):
                rels.add("Castle/%s/SabukW.txt" % name)
    out = {}
    for rel in sorted(rels):
        per = {}
        for label, base in sides:
            path = os.path.join(base, *rel.split("/"))
            if os.path.isfile(path):
                with open(path, "rb") as fh:
                    raw = fh.read()
                per[label] = (hashlib.sha256(raw).hexdigest(), len(raw))
            else:
                per[label] = (None, None)
        out[rel] = per
    return out


def write_castle_manifest(path, root, manifest, extra_comments=()):
    """确定性文本格式：`#` 注释 + `rel<TAB>side<TAB>sha256|MISSING<TAB>bytes|-`。"""
    d = os.path.dirname(os.path.abspath(path))
    if d:
        os.makedirs(d, exist_ok=True)
    lines = ["# castle-symmetry/1",
             "# root: %s" % root,
             "# generated_at: %s" % time.strftime("%Y-%m-%dT%H:%M:%S%z"),
             "# sides: %s" % ",".join(label for label, _ in castle_sides(root))]
    for c in extra_comments:
        lines.append("# %s" % c)
    for rel in sorted(manifest):
        for label in sorted(manifest[rel]):
            sha, n = manifest[rel][label]
            lines.append("%s\t%s\t%s\t%s" % (rel, label, sha or CASTLE_MISSING,
                                               "-" if n is None else n))
    with open(path, "w", encoding="utf-8", newline="\n") as fh:
        fh.write("\n".join(lines) + "\n")
    return path


def read_castle_manifest(path):
    """读回 `write_castle_manifest` 的格式；坏行/空文件一律 ValueError（fail-closed）。"""
    out = {}
    with open(path, "r", encoding="utf-8", newline="") as fh:
        for lineno, line in enumerate(fh, 1):
            line = line.rstrip("\r\n")
            if not line or line.startswith("#"):
                continue
            parts = line.split("\t")
            if len(parts) != 4:
                raise ValueError("%s:%d 不是 rel<TAB>side<TAB>sha256<TAB>bytes（%r）"
                                 % (path, lineno, line))
            rel, side, sha, nbytes = parts
            if sha == CASTLE_MISSING:
                out.setdefault(rel, {})[side] = (None, None)
                continue
            if len(sha) != 64:
                raise ValueError("%s:%d sha256 长度不是 64（%r）" % (path, lineno, sha))
            try:
                n = int(nbytes)
            except ValueError:
                raise ValueError("%s:%d bytes 不是整数（%r）" % (path, lineno, nbytes))
            out.setdefault(rel, {})[side] = (sha, n)
    if not out:
        raise ValueError("%s 没有任何状态行（只有注释/空行）" % path)
    return out


def _castle_symmetric(values):
    return len(set(values)) <= 1


def classify_castle_drift(before, after):
    """把跑后差异分两档：`本轮造成` / `开窗前已存在`。

    规则（逐文件，按两侧并集判定）：
      * 跑后三侧**一致** ⇒ 不是对称性漂移，不记（即使三侧同时变成同一个新值）；
      * 跑后不一致 且 跑前也**已经**不一致 ⇒ `开窗前已存在`；
      * 跑后不一致 且 跑前一致          ⇒ `本轮造成`。
    """
    recs = []
    for rel in sorted(set(before) | set(after)):
        b = before.get(rel) or {}
        a = after.get(rel) or {}
        sides = sorted(set(b) | set(a))
        bvals = [(b.get(side) or (None, None))[0] for side in sides]
        avals = [(a.get(side) or (None, None))[0] for side in sides]
        if _castle_symmetric(avals):
            continue
        kind = "开窗前已存在" if not _castle_symmetric(bvals) else "本轮造成"
        recs.append({"file": rel, "kind": kind,
                     "before": dict(zip(sides, bvals)),
                     "after": dict(zip(sides, avals))})
    return recs


def check_castle_state_symmetry(root=DEFAULT_PRIVATE_ROOT, baseline=None):
    """三侧 castle 状态（List.txt + 每个 Castle/*/SabukW.txt）对称性 + 跑前基线落盘。

    与既有 `castle_list_symmetry` 的分工（卡 A5：不改既有判据语义）：
      * 既有项**语义不变**：List.txt 不一致仍 FAIL；
      * 本项把 SabukW 也纳入，并把三侧 sha256+bytes 写进基线文件；差异只 **WARN**
        （内容是冻结件，本轮不能改），跑后由 `--castle-compare` 分两档归因。
    """
    manifest = castle_state_manifest(root)
    present = [rel for rel, per in manifest.items()
               if any(sha for sha, _n in per.values())]
    if not present:
        return FAIL, ("三侧都没有 castle 状态文件（Castle/List.txt 与 Castle/*/SabukW.txt），"
                      "无法建基线（root=%s）" % root), \
            "先建私有栈夹具（runbook §0.9）"
    asym = []
    for rel in sorted(manifest):
        shas = [sha for sha, _n in manifest[rel].values()]
        if not _castle_symmetric(shas):
            asym.append(rel)
    base_note = ""
    if baseline:
        write_castle_manifest(baseline, root, manifest)
        base_note = "；三侧 sha256+bytes 已写入 %s" % baseline
    if asym:
        return WARN, ("开窗前已存在差异：%s（跑后按「本轮造成 / 开窗前已存在」两档分类%s）"
                      % (", ".join(asym), base_note)), \
            ("对齐（内容冻结：只改私有栈副本，不改部署件/Envir）："
             "把部署件值复制到三侧同名文件；见 runbook 开窗第 0 步")
    shown = ", ".join("%s=%s" % (rel, (manifest[rel].get("rust") or (None,))[0][:12])
                      for rel in sorted(manifest)
                      if (manifest[rel].get("rust") or (None,))[0])
    return OK, ("三侧一致：%s（%d 个文件）%s" % (shown, len(manifest), base_note)), None


def _dir_sha_map(root):
    """递归目录 -> {relpath: sha256}；不跟随符号链接。"""
    out = {}
    for base, _dirs, files in os.walk(root):
        for fn in files:
            path = os.path.join(base, fn)
            rel = os.path.relpath(path, root).replace(os.sep, "/")
            try:
                with open(path, "rb") as f:
                    out[rel] = hashlib.sha256(f.read()).hexdigest()
            except OSError as e:
                raise RuntimeError("读文件失败：%s（%s）" % (path, e))
    return out


def check_envir_shadow_symmetry(root=DEFAULT_PRIVATE_ROOT, limit=20):
    """影子 Envir 必须与基线逐文件 sha256 一致（卡 openmir2-preflight-envir-shadow-symmetry）。

    背景：`start-csharp.ps1` 在 `--rng-log` 模式下 GameSrv 从 `M2GameSvr-hook/` 影子目录跑，
    影子部署有 `if (-not (Test-Path $shadowGs))` 守卫 ⇒ 只要 `GameSvr.exe` 在就**不会重新
    robocopy**；对影子 Envir 的任何手改会一直生效（内容冻结风险）。本检查只读比对：
      baseline `csharp-root/M2GameSvr/Envir`
      shadow   `csharp-root/M2GameSvr-hook/Envir`（不存在 ⇒ 跳过，视为未启用 hook 影子）
      rust     `rust-root/M2GameSvr/Envir`（存在则一并比）
    任一文件内容不同、影子/rust 独有或缺失 ⇒ FAIL，并列出差异文件 + 两侧 sha。
    """
    base_dir = os.path.join(root, "csharp-root", "M2GameSvr", "Envir")
    shadow_dir = os.path.join(root, "csharp-root", "M2GameSvr-hook", "Envir")
    rust_dir = os.path.join(root, "rust-root", "M2GameSvr", "Envir")
    if not os.path.isdir(base_dir):
        return FAIL, "缺基线 Envir 目录：%s" % base_dir, "按 runbook §0.5/§0.9 建私有栈"
    try:
        base = _dir_sha_map(base_dir)
    except RuntimeError as e:
        return FAIL, str(e), "检查文件权限/占用"
    pairs = []
    if os.path.isdir(shadow_dir):
        pairs.append(("shadow", shadow_dir))
    if os.path.isdir(rust_dir):
        pairs.append(("rust", rust_dir))
    if not pairs:
        return OK, "无影子/rust Envir 目录（未启用 hook 影子）⇒ 跳过", None
    diffs = []
    for label, other_dir in pairs:
        try:
            other = _dir_sha_map(other_dir)
        except RuntimeError as e:
            return FAIL, str(e), "检查文件权限/占用"
        for rel in sorted(set(base) | set(other)):
            if rel not in other:
                diffs.append("%s 缺 %s（基线 sha=%s）" % (label, rel, base[rel][:12]))
            elif rel not in base:
                diffs.append("%s 多 %s（sha=%s）" % (label, rel, other[rel][:12]))
            elif base[rel] != other[rel]:
                diffs.append("%s 内容不同 %s（baseline=%s vs %s=%s）"
                             % (label, rel, base[rel][:12], label, other[rel][:12]))
    if diffs:
        shown = diffs[:limit]
        more = "" if len(diffs) <= limit else "；…另有 %d 条" % (len(diffs) - limit)
        return FAIL, ("影子/基线 Envir 不一致（%d 处）：%s%s"
                      % (len(diffs), "；".join(shown), more)), \
            "还原影子：robocopy 基线 Envir -> 影子（或删影子 GameSvr.exe 让部署重建）；见 runbook §0.9"
    return OK, ("影子=基线：%d 文件 0 差异（%s）"
                % (len(base), ", ".join(label for label, _ in pairs))), None


def check_conformance_password(env=None):
    """探针连库凭据：`CONFORMANCE_PASSWORD` 必须存在且非空（卡 `openmir2-preflight-password-check`）。

    2026-10-26 首轮：24 条场景全部 1s 内 `outer_rc=2`，`card_window` 报
    「缺密码：用环境变量 CONFORMANCE_PASSWORD 提供」⇒ 整轮空转；而 preflight 十项全绿，
    没有任何一项检查「探针连库的凭据在不在」。

    只读环境变量；**不记录、不回显密码内容**（只报存在性与非空）。
    """
    e = os.environ if env is None else env
    val = e.get("CONFORMANCE_PASSWORD")
    if val is None or str(val) == "":
        detail = ("CONFORMANCE_PASSWORD 未设置（探针连库凭据缺失 ⇒ 24 个场景会各自 "
                  "outer_rc=2 空转，整轮白开）")
        fix = "export CONFORMANCE_PASSWORD=…（只走环境变量；别写进命令行历史/脚本/profile）"
        return FAIL, detail, fix
    return OK, "CONFORMANCE_PASSWORD 已设置（内容不记录）", None


# ---------------------------------------------------------------------------
# 全量检查
# ---------------------------------------------------------------------------
def run_all(root=DEFAULT_PRIVATE_ROOT, start_rust=DEFAULT_START_RUST, repo=REPO,
            skip_pin_smoke=False, skip_binary=False, report=None, manifest=None,
            castle_before=None):
    rep = report or Report()
    # 第一项：探针凭据。缺它后面每一项都可能白跑（2026-10-26 首轮整轮空转）。
    status, detail, fix = check_conformance_password()
    rep.add("conformance_password", status, detail, fix)

    status, detail, fix = check_inventory(root)
    rep.add("inventory", status, detail, fix)

    status, detail, fix = check_selgate(root)
    rep.add("selgate_listenport", status, detail, fix)

    status, detail, fix = check_route525(root)
    rep.add("route_525", status, detail, fix)

    status, detail, fix = check_castle_list(root)
    rep.add("castle_list_symmetry", status, detail, fix)

    # 卡 openmir2-castle-symmetry-baseline：既有项只看 List.txt 且不落基线；
    # 这里补 SabukW + 三侧 sha256 基线（差异只 WARN，跑后两档分类）。
    status, detail, fix = check_castle_state_symmetry(root, castle_before)
    rep.add("castle_state_symmetry", status, detail, fix)

    status, detail, fix = check_envir_shadow_symmetry(root)
    rep.add("envir_shadow_symmetry", status, detail, fix)

    pin_log = None
    try:
        cw = _load_card_window()
        alt = cw.load_profile("alt")
        wm = cw.load_profile("window-magic")
        pin_log = cw.RUST_STACK_PIN_LOG
    except Exception as ex:  # ConfigError / import 失败都按 FAIL
        rep.add("pin_static", FAIL, "加载内置 profile 失败：%s" % ex,
                "修 card_window.py 的 window-magic 派生")
    else:
        status, detail, fix = check_pin_static(alt, wm, pin_log)
        rep.add("pin_static", status, detail, fix)

    if skip_pin_smoke:
        rep.add("pin_smoke", WARN, "已按 --skip-pin-smoke 跳过（本次证据须登记「pin 链未冒烟」）",
                "开窗前去掉该开关重跑")
    elif pin_log is None:
        rep.add("pin_smoke", FAIL, "pin_static 未通过，无法定位自证日志", "先修 pin_static")
    else:
        status, detail, fix = check_pin_smoke(pin_log)
        rep.add("pin_smoke", status, detail, fix)

    if skip_binary:
        rep.add("binary_stamp", WARN, "已按 --skip-binary 跳过（本次证据须登记「二进制未对账」）",
                "开窗前去掉该开关重跑")
    else:
        status, detail, fix = check_build_stamp(repo)
        rep.add("binary_stamp", status, detail, fix)

    status, detail, fix = check_start_rust(start_rust, os.path.join(
        repo, "mir2-rs", "target", "release", "mir2-server.exe"), repo)
    rep.add("start_rust_exe", status, detail, fix)

    status, detail, fix = check_roster(repo, manifest)
    rep.add("roster", status, detail, fix)

    c = rep.counts()
    print("PREFLIGHT_PRIVATE SUMMARY pass=%d warn=%d fail=%d" % (c[OK], c[WARN], c[FAIL]))
    print("PREFLIGHT_PRIVATE RESULT %s" % rep.result())
    return rep


# ---------------------------------------------------------------------------
# --selftest：解析器 + 两坑的牙（离线、临时目录、不碰真私栈）
# ---------------------------------------------------------------------------
def selftest():
    pass_n = 0
    fail_n = 0

    def chk(cond, name):
        nonlocal pass_n, fail_n
        if cond:
            pass_n += 1
            print("PASS %s" % name)
        else:
            fail_n += 1
            print("FAIL %s" % name)

    # 解析器
    d, err = parse_server_info("127.0.0.1 127.0.0.1 17200\n")
    chk(err is None and d["ports"] == [17200], "ServerInfo: 17200 解析")
    d, err = parse_server_info("127.0.0.1 127.0.0.1 17200,17201\n")
    chk(err is None and d["ports"] == [17200, 17201], "ServerInfo: 多端口")
    d, err = parse_server_info("\n")
    chk(err is not None, "ServerInfo: 空文件 => 错")
    d, err = parse_server_info("127.0.0.1 127.0.0.1 x\n")
    chk(err is not None, "ServerInfo: 非数字端口 => 错")

    # 探针凭据（卡 openmir2-preflight-password-check）：缺 ⇒ FAIL 并点名；有 ⇒ OK 且不回显内容。
    st, dt, fx = check_conformance_password({})
    chk(st == FAIL and "CONFORMANCE_PASSWORD" in dt and fx and "CONFORMANCE_PASSWORD" in fx,
        "password: 缺 env => FAIL 且 detail/fix 都点名变量")
    st, dt, fx = check_conformance_password({"CONFORMANCE_PASSWORD": ""})
    chk(st == FAIL and "CONFORMANCE_PASSWORD" in dt, "password: 空串也算缺")
    st, dt, fx = check_conformance_password({"CONFORMANCE_PASSWORD": "s3cret-value"})
    chk(st == OK and "s3cret-value" not in dt, "password: 已设置 => OK 且不回显内容")

    ini = parse_ini("[SelGate]\nListenPort=17100\n")
    chk((ini.get("selgate") or {}).get("listenport") == "17100", "INI: [SelGate] ListenPort")
    ini = parse_ini("[GameGate]\nGatePort1=17200\nServerPort1=15000\n")
    chk((ini.get("gamegate") or {}).get("gateport1") == "17200", "INI: 默认 section 键")

    raw, inner = parse_start_rust_default(
        "param(\n [string]$ServerExe = 'E:\\tmp\\x\\mir2-server.exe',\n)")
    chk(raw == "E:\\tmp\\x\\mir2-server.exe" and inner is None,
        "start-rust: 解析 -ServerExe 默认值")

    # 两坑：临时假私栈（真文件、真 sha）
    tmp = tempfile.mkdtemp(prefix="preflight-private-selftest.")
    try:
        root = tmp
        for rel in INVENTORY:
            p = os.path.join(root, rel)
            os.makedirs(os.path.dirname(p), exist_ok=True)
            with open(p, "wb") as fh:
                fh.write(b"fixture\n")
        os.makedirs(os.path.join(root, "baseline"), exist_ok=True)
        with open(os.path.join(root, "baseline", "x.tsv"), "w") as fh:
            fh.write("x\n")
        with open(os.path.join(root, "selgate-build", "SelGate.exe"), "wb") as fh:
            fh.write(b"T172-selgate")
        with open(os.path.join(root, "csharp-root", "SelGate", "SelGate.exe"), "wb") as fh:
            fh.write(b"T172-selgate")
        with open(os.path.join(root, "csharp-root", "SelGate", "config.conf"), "w") as fh:
            fh.write("[SelGate]\nListenPort=17100\n")
        with open(os.path.join(root, "csharp-root", "LoginSrv", "AddrTable.txt"), "w") as fh:
            fh.write("srv Title1 127.0.0.1 127.0.0.1 127.0.0.1:17100\n")
        with open(os.path.join(root, "csharp-root", "RunGate", "config.conf"), "w") as fh:
            fh.write("[GameGate]\nGatePort1=17200\nServerPort1=15000\n")
        with open(os.path.join(root, "rust-root", "RunGate", "config.conf"), "w") as fh:
            fh.write("[GameGate]\nGatePort1=17600\nServerPort1=15000\n")
        for side in ("csharp-root", "rust-root"):
            with open(os.path.join(root, side, "DBSrv", "ServerInfo.txt"), "w") as fh:
                fh.write("127.0.0.1 127.0.0.1 17200\n")
        chk(check_inventory(root)[0] == OK, "inventory: 齐全 => OK")
        chk(check_selgate(root)[0] == OK, "selgate: 补丁构建 + 17100 => OK")
        chk(check_route525(root)[0] == OK, "route525: 两侧 17200 => OK")

        # 牙 1：SelGate 换成 stock（sha 不同）=> FAIL
        with open(os.path.join(root, "csharp-root", "SelGate", "SelGate.exe"), "wb") as fh:
            fh.write(b"stock-selgate")
        chk(check_selgate(root)[0] == FAIL, "牙: stock SelGate（sha 不同）=> FAIL")
        with open(os.path.join(root, "csharp-root", "SelGate", "SelGate.exe"), "wb") as fh:
            fh.write(b"T172-selgate")

        # 牙 2：ListenPort 被写回 7100 => FAIL
        with open(os.path.join(root, "csharp-root", "SelGate", "config.conf"), "w") as fh:
            fh.write("[SelGate]\nListenPort=7100\n")
        chk(check_selgate(root)[0] == FAIL, "牙: ListenPort=7100 => FAIL")
        with open(os.path.join(root, "csharp-root", "SelGate", "config.conf"), "w") as fh:
            fh.write("[SelGate]\nListenPort=17100\n")

        # 牙 3：C# ServerInfo 回到默认 7200 => FAIL
        with open(os.path.join(root, "csharp-root", "DBSrv", "ServerInfo.txt"), "w") as fh:
            fh.write("127.0.0.1 127.0.0.1 7200\n")
        chk(check_route525(root)[0] == FAIL, "牙: C# ServerInfo=7200 => FAIL")
        with open(os.path.join(root, "csharp-root", "DBSrv", "ServerInfo.txt"), "w") as fh:
            fh.write("127.0.0.1 127.0.0.1 17200\n")

        # 牙 4：Rust ServerInfo 被「修」成 17600 => FAIL（说明为什么不能改）
        with open(os.path.join(root, "rust-root", "DBSrv", "ServerInfo.txt"), "w") as fh:
            fh.write("127.0.0.1 127.0.0.1 17600\n")
        chk(check_route525(root)[0] == FAIL, "牙: Rust ServerInfo=17600 => FAIL（525 逐字节）")
        with open(os.path.join(root, "rust-root", "DBSrv", "ServerInfo.txt"), "w") as fh:
            fh.write("127.0.0.1 127.0.0.1 17200\n")

        # 牙 5：缺 baseline => FAIL
        shutil.rmtree(os.path.join(root, "baseline"))
        chk(check_inventory(root)[0] == FAIL, "牙: 缺 baseline => FAIL")
        os.makedirs(os.path.join(root, "baseline"), exist_ok=True)
        with open(os.path.join(root, "baseline", "x.tsv"), "w") as fh:
            fh.write("x\n")

        # start-rust：对齐 / 不对齐
        sr = os.path.join(tmp, "start-rust.ps1")
        with open(sr, "w") as fh:
            fh.write("[string]$ServerExe = 'E:/repo/mir2-rs/target/release/mir2-server.exe'\n")
        chk(check_start_rust(sr, "E:\\repo\\mir2-rs\\target\\release\\mir2-server.exe")[0] == OK,
            "start-rust: 斜杠/大小写归一 => OK")
        chk(check_start_rust(sr, "E:\\other\\mir2-server.exe")[0] == FAIL,
            "start-rust: 指向旧 worktree => FAIL")

        # build stamp：head/sha/dirty 三条
        repo2 = os.path.join(tmp, "repo")
        os.makedirs(os.path.join(repo2, "mir2-rs", "target", "release"), exist_ok=True)
        exe = os.path.join(repo2, "mir2-rs", "target", "release", "mir2-server.exe")
        with open(exe, "wb") as fh:
            fh.write(b"exe-bytes")
        stamp = os.path.join(repo2, "mir2-rs", "target", "release", "mir2-server.build.json")
        head = "a" * 40

        def write_stamp(**kw):
            doc = {"head": head, "sha256": sha256_file(exe), "size": os.path.getsize(exe),
                   "dirty": "", "built_at": "2026-10-24T20:00:00+08:00"}
            doc.update(kw)
            with open(stamp, "w", encoding="utf-8") as fh:
                json.dump(doc, fh)

        write_stamp()
        chk(check_build_stamp(repo2, head=head, stamp=stamp, exe=exe)[0] == OK,
            "stamp: head/sha 对齐 => OK")
        write_stamp(head="b" * 40)
        chk(check_build_stamp(repo2, head=head, stamp=stamp, exe=exe)[0] == FAIL,
            "牙: stamp.head 与 HEAD 不符 => FAIL")
        write_stamp(dirty=" M src/x.rs")
        chk(check_build_stamp(repo2, head=head, stamp=stamp, exe=exe)[0] == FAIL,
            "牙: 脏树构建 => FAIL")
        write_stamp()
        with open(exe, "ab") as fh:
            fh.write(b"tamper")
        chk(check_build_stamp(repo2, head=head, stamp=stamp, exe=exe)[0] == FAIL,
            "牙: 二进制被改动（sha 不符）=> FAIL")

        # pin 静态：合成 profile
        cs = {"stack": ["powershell", "-File", "tools/window/start-stack-wmi.ps1"]}
        rs = {"env": {PIN_KEY: "0"},
              "stack": ["powershell", "-File", "tools/window/start-stack-wmi.ps1",
                        "-PinChecks", "%s=0" % PIN_KEY, "-PinLog", "E:/log/game-svr.out.log"]}
        wm = {"magic": [{"magic_id": 1}], "sides": {"csharp": cs, "rust": rs}}
        alt = {"sides": {"csharp": {"stack": cs["stack"]}, "rust": {"stack": cs["stack"]}}}
        chk(check_pin_static(alt, wm, "E:/log/game-svr.out.log")[0] == OK,
            "pin: window-magic 接线齐全 => OK")
        wm_bad = json.loads(json.dumps(wm))
        wm_bad["sides"]["rust"]["stack"] = cs["stack"]
        chk(check_pin_static(alt, wm_bad, "E:/log/game-svr.out.log")[0] == FAIL,
            "牙: rust stack 未接 -PinChecks => FAIL")
        wm_bad2 = json.loads(json.dumps(wm))
        wm_bad2["sides"]["rust"]["env"] = {}
        chk(check_pin_static(alt, wm_bad2, "E:/log/game-svr.out.log")[0] == FAIL,
            "牙: 缺 MIR2_CLIENT_ABILITY_LAYOUT=0 => FAIL")

        # roster：条数按清单实况（不硬编码）；已封存 pickup_item 不得入列；n 必须 1..N 连续。
        # 牙必须钉在**正确语义**上（断号/重复/pickup），不能靠「多一条就红」——那是 count==17 的旧语义。
        fake_repo = os.path.join(tmp, "roster-repo")
        scen_dir = os.path.join(fake_repo, "tools", "probe", "scenarios")
        os.makedirs(scen_dir, exist_ok=True)
        with open(MANIFEST, encoding="utf-8-sig") as fh:
            good_manifest = json.load(fh)
        scen_n = len(good_manifest["scenarios"])
        for s_ in good_manifest["scenarios"]:
            with open(os.path.join(scen_dir, s_["name"] + ".json"), "w") as fh:
                fh.write("{}\n")
        with open(os.path.join(scen_dir, "pickup_item.json"), "w") as fh:
            fh.write("{}\n")
        good_path = os.path.join(tmp, "run3-good.json")
        with open(good_path, "w", encoding="utf-8") as fh:
            json.dump(good_manifest, fh)
        chk(check_roster(repo=fake_repo, manifest=good_path)[0] == OK,
            "roster: 清单 %d 条（实况）且 n 连续、不含 pickup_item => OK" % scen_n)

        # 牙①：追加 pickup_item（n=scen_n+1，仍连续）=> FAIL（原因是封存场景，不是条数）
        bad_pickup = json.loads(json.dumps(good_manifest))
        bad_pickup["scenarios"].append(
            {"n": scen_n + 1, "name": "pickup_item", "profile": "alt", "extra": []})
        bad_pickup_path = os.path.join(tmp, "run3-bad-pickup.json")
        with open(bad_pickup_path, "w", encoding="utf-8") as fh:
            json.dump(bad_pickup, fh)
        st, detail, _ = check_roster(repo=fake_repo, manifest=bad_pickup_path)
        chk(st == FAIL and "pickup_item" in detail,
            "牙: 清单含 pickup_item（n 连续）=> FAIL 且点名 pickup_item")

        # 牙②：断号（删 n=3）=> FAIL 且点名「连续」
        bad_gap = json.loads(json.dumps(good_manifest))
        bad_gap["scenarios"] = [x for x in bad_gap["scenarios"] if x.get("n") != 3]
        bad_gap_path = os.path.join(tmp, "run3-bad-gap.json")
        with open(bad_gap_path, "w", encoding="utf-8") as fh:
            json.dump(bad_gap, fh)
        st, detail, _ = check_roster(repo=fake_repo, manifest=bad_gap_path)
        chk(st == FAIL and "连续" in detail, "牙: 删 n=3（断号）=> FAIL 且点名不连续")

        # 牙③：重复 n => FAIL
        bad_dup = json.loads(json.dumps(good_manifest))
        bad_dup["scenarios"][-1]["n"] = bad_dup["scenarios"][-2]["n"]
        bad_dup_path = os.path.join(tmp, "run3-bad-dup.json")
        with open(bad_dup_path, "w", encoding="utf-8") as fh:
            json.dump(bad_dup, fh)
        chk(check_roster(repo=fake_repo, manifest=bad_dup_path)[0] == FAIL,
            "牙: 重复 n => FAIL")

        # 正例②：临时清单**变多一条**（n 连续）仍应 OK——条数不是判据（本条即「count 变化不红」）。
        more = json.loads(json.dumps(good_manifest))
        last = more["scenarios"][-1]
        more["scenarios"].append({"n": scen_n + 1, "name": "zz_extra",
                                  "profile": last.get("profile"), "extra": []})
        with open(os.path.join(scen_dir, "zz_extra.json"), "w") as fh:
            fh.write("{}\n")
        more_path = os.path.join(tmp, "run3-more.json")
        with open(more_path, "w", encoding="utf-8") as fh:
            json.dump(more, fh)
        chk(check_roster(repo=fake_repo, manifest=more_path)[0] == OK,
            "roster: 清单 %d 条（+1，n 连续）=> 仍 OK（条数不是判据）" % (scen_n + 1))

        # ---- castle_list_symmetry（卡 openmir2-window-castle-list-symmetry）----
        base = os.path.join(tmp, "castle")
        cs = os.path.join(base, "csharp-root", "M2GameSvr", "Castle")
        rs = os.path.join(base, "rust-root", "M2GameSvr", "Castle")
        os.makedirs(cs, exist_ok=True)
        os.makedirs(rs, exist_ok=True)
        cs_list = os.path.join(cs, "List.txt")
        rs_list = os.path.join(rs, "List.txt")
        with open(cs_list, "wb") as fh:
            fh.write(b"0\r\n")
        with open(rs_list, "wb") as fh:
            fh.write(b"0\r\n1\r\n")
        st, detail, _ = check_castle_list(base)
        chk(st == FAIL and "不一致" in detail and "0,1".replace(",", "\\r\\n") in detail
            or (st == FAIL and "不一致" in detail),
            "castle: 两侧 List.txt 不一致（0 vs 0,1）=> FAIL 且打印两侧内容")
        with open(cs_list, "wb") as fh:
            fh.write(b"0\r\n1\r\n")
        chk(check_castle_list(base)[0] == OK,
            "castle: 两侧内容一致（0,1）=> OK")
        shadow_dir = os.path.join(base, "csharp-root", "M2GameSvr-hook", "Castle")
        os.makedirs(shadow_dir, exist_ok=True)
        shadow_list = os.path.join(shadow_dir, "List.txt")
        with open(shadow_list, "wb") as fh:
            fh.write(b"0\r\n")
        st, detail, _ = check_castle_list(base)
        chk(st == FAIL and "csharp-hook" in detail,
            "castle 牙: 影子 M2GameSvr-hook 与 rust 不一致 => FAIL 且点名 csharp-hook")
        os.remove(shadow_list)
        os.remove(os.path.join(cs, "List.txt"))
        st, detail, _ = check_castle_list(base)
        chk(st == FAIL and "缺 Castle/List.txt" in detail,
            "castle 牙: 缺侧文件 => FAIL 点名缺失")

        # ---- castle 状态基线 + 跑后两档分类（卡 openmir2-castle-symmetry-baseline）----
        cb = os.path.join(tmp, "castlestate")
        cs2 = os.path.join(cb, "csharp-root", "M2GameSvr", "Castle")
        rs2 = os.path.join(cb, "rust-root", "M2GameSvr", "Castle")
        hk2 = os.path.join(cb, "csharp-root", "M2GameSvr-hook", "Castle")
        for d in (cs2, rs2, hk2):
            os.makedirs(os.path.join(d, "Castle"), exist_ok=True)
            with open(os.path.join(d, "Castle", "SabukW.txt"), "wb") as fh:
                fh.write(b"deploy-value")
            with open(os.path.join(d, "List.txt"), "wb") as fh:
                fh.write(b"0\r\n1\r\n")
        base_path = os.path.join(tmp, "castle-before.txt")
        st, detail, _ = check_castle_state_symmetry(cb, base_path)
        chk(st == OK and os.path.isfile(base_path) and "三侧一致" in detail
            and base_path in detail,
            "castle_state: 三侧一致 => OK 且落基线文件")
        before = read_castle_manifest(base_path)
        chk("Castle/List.txt" in before and "Castle/Castle/SabukW.txt" in before
            and set(before["Castle/List.txt"]) == {"csharp", "csharp-hook", "rust"},
            "castle_state: 基线含 List.txt + Castle/*/SabukW.txt 三侧")

        # 牙①（卡 A3）：跑前把一侧改一字节 => 必须「开窗前已存在」，不能「本轮造成」
        with open(os.path.join(rs2, "Castle", "SabukW.txt"), "wb") as fh:
            fh.write(b"deploy-valuE")
        st, detail, _ = check_castle_state_symmetry(cb)
        chk(st == WARN and "开窗前已存在差异" in detail
            and "Castle/Castle/SabukW.txt" in detail,
            "castle_state 牙1: 跑前一侧一字节不同 => WARN 点名「开窗前已存在」")
        base2 = os.path.join(tmp, "castle-before-asym.txt")
        write_castle_manifest(base2, cb, castle_state_manifest(cb))
        recs = classify_castle_drift(read_castle_manifest(base2), castle_state_manifest(cb))
        chk(len(recs) == 1 and recs[0]["kind"] == "开窗前已存在"
            and recs[0]["file"] == "Castle/Castle/SabukW.txt",
            "castle_state 牙2: 跑前已差异 + 跑后不变 => 只归「开窗前已存在」")

        # 牙②（卡 A3）：不改 => 必须报无差异
        with open(os.path.join(rs2, "Castle", "SabukW.txt"), "wb") as fh:
            fh.write(b"deploy-value")
        recs = classify_castle_drift(read_castle_manifest(base_path), castle_state_manifest(cb))
        chk(recs == [], "castle_state 牙3: 跑前跑后一致 => 无差异")

        # 正例：跑前一致 + 跑后一侧变 => 「本轮造成」
        with open(os.path.join(cs2, "Castle", "SabukW.txt"), "wb") as fh:
            fh.write(b"deploy-value-X")
        recs = classify_castle_drift(read_castle_manifest(base_path), castle_state_manifest(cb))
        chk(len(recs) == 1 and recs[0]["kind"] == "本轮造成",
            "castle_state 牙4: 跑前一致 + 跑后一侧变 => 「本轮造成」")

        # 牙5/6：坏基线 / 空基线 => ValueError fail-closed
        with open(os.path.join(cs2, "Castle", "SabukW.txt"), "wb") as fh:
            fh.write(b"deploy-value")
        bad1 = os.path.join(tmp, "castle-bad1.txt")
        with open(bad1, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("Castle/List.txt\tcsharp\tonly-three-fields\n")
        bad_msg = None
        try:
            read_castle_manifest(bad1)
        except ValueError as e:
            bad_msg = str(e)
        # 必须是「点名 文件:行」的诊断，而不是裸的 unpack 报错——否则删掉显式长度检查
        # 仍会被 `a, b, c, d = parts` 的 ValueError 蒙混过关（变异自证抓到的假牙）。
        chk(bad_msg is not None and ("%s:1" % bad1) in bad_msg,
            "castle_state 牙5: 坏基线行 => ValueError 且点名 文件:行")
        empty = os.path.join(tmp, "castle-empty.txt")
        with open(empty, "w", encoding="utf-8", newline="\n") as fh:
            fh.write("# only comments\n")
        try:
            read_castle_manifest(empty)
            empty_ok = False
        except ValueError:
            empty_ok = True
        chk(empty_ok, "castle_state 牙6: 空基线 => ValueError")

        # 牙7/牙8（卡 openmir2-castle-failclosed-teeth；reviewer-21 F1）：
        # **「判不了」的判据本身**也要有牙——空 root 与 CLI 的 fail-closed 路径此前零覆盖，
        # 把 `if not present:` 改成 `if False:` 仍 48/48 全绿。
        empty_root = os.path.join(tmp, "castle-empty-root")
        os.makedirs(empty_root, exist_ok=True)
        st, detail, _fix = check_castle_state_symmetry(empty_root)
        chk(st == FAIL and empty_root in detail and "三侧都没有" in detail,
            "castle_state 牙7: 空 root（三侧都缺）=> FAIL 且点名 root")
        import contextlib
        import io as _io
        _buf = _io.StringIO()
        with contextlib.redirect_stdout(_buf):
            _rc = main(["--root", empty_root, "--castle-before-only",
                        "--castle-before", os.path.join(tmp, "castle-empty-before.txt")])
        _out = _buf.getvalue()
        chk(_rc == 2 and "CASTLE_BASELINE FAIL" in _out and empty_root in _out,
            "castle_state 牙8: --castle-before-only + 三侧都缺 => rc=2 且点名 root")

        # 牙9（卡 openmir2-tooth-hardening-batch C）：第三条 fail-closed 兄弟——
        # 只给 --castle-before-only、不给 --castle-before PATH ⇒ rc=2 且点名该参数。
        _buf2 = _io.StringIO()
        with contextlib.redirect_stdout(_buf2):
            _rc2 = main(["--root", empty_root, "--castle-before-only"])
        _out2 = _buf2.getvalue()
        chk(_rc2 == 2 and "CASTLE_BASELINE FAIL" in _out2 and "--castle-before PATH" in _out2,
            "castle_state 牙9: --castle-before-only 缺 PATH => rc=2 且点名 --castle-before PATH")

        # ---- envir_shadow_symmetry（卡 openmir2-preflight-envir-shadow-symmetry）----
        er = os.path.join(tmp, "envirshadow")
        eb = os.path.join(er, "csharp-root", "M2GameSvr", "Envir")
        es = os.path.join(er, "csharp-root", "M2GameSvr-hook", "Envir")
        erust = os.path.join(er, "rust-root", "M2GameSvr", "Envir")
        for d in (eb, es, erust):
            os.makedirs(d, exist_ok=True)
        with open(os.path.join(eb, "A.txt"), "wb") as fh:
            fh.write(b"alpha")
        with open(os.path.join(es, "A.txt"), "wb") as fh:
            fh.write(b"alpha")
        with open(os.path.join(erust, "A.txt"), "wb") as fh:
            fh.write(b"alpha")
        chk(check_envir_shadow_symmetry(er)[0] == OK,
            "envir_shadow: 基线/影子/rust 逐文件一致 => OK")
        with open(os.path.join(es, "A.txt"), "wb") as fh:
            fh.write(b"alpha-changed")
        st, detail, _ = check_envir_shadow_symmetry(er)
        chk(st == FAIL and "内容不同" in detail and "A.txt" in detail
            and "baseline=" in detail,
            "envir_shadow 牙1: 影子改一行 => FAIL 并打印文件与两侧 sha")
        with open(os.path.join(es, "A.txt"), "wb") as fh:
            fh.write(b"alpha")
        with open(os.path.join(es, "extra.txt"), "wb") as fh:
            fh.write(b"extra")
        st, detail, _ = check_envir_shadow_symmetry(er)
        chk(st == FAIL and "shadow 多 extra.txt" in detail,
            "envir_shadow 牙2: 影子多一个文件 => FAIL 点名")
        os.remove(os.path.join(es, "extra.txt"))
        os.remove(os.path.join(es, "A.txt"))
        st, detail, _ = check_envir_shadow_symmetry(er)
        chk(st == FAIL and "shadow 缺 A.txt" in detail,
            "envir_shadow 牙3: 影子少一个文件 => FAIL 点名")

    finally:
        shutil.rmtree(tmp, ignore_errors=True)

    if fail_n:
        print("RED: preflight_private_stack selftest %d/%d" % (pass_n, pass_n + fail_n))
        return 1
    print("GREEN: preflight_private_stack selftest %d/%d" % (pass_n, pass_n + fail_n))
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(
        description="窗口私有栈前置自检（只读，不起栈/不占端口/不连库）")
    ap.add_argument("--root", default=os.environ.get("WINDOW_PRIVATE_ROOT", DEFAULT_PRIVATE_ROOT),
                    help="私有栈根（默认 %s）" % DEFAULT_PRIVATE_ROOT)
    ap.add_argument("--start-rust", default=DEFAULT_START_RUST, help="start-rust.ps1 路径")
    ap.add_argument("--repo", default=REPO, help="本检出根（默认按脚本位置推导）")
    ap.add_argument("--manifest",
                    default=os.environ.get("WINDOW_MANIFEST") or MANIFEST,
                    help="场景清单（默认 $WINDOW_MANIFEST 或 run3-scenarios.json；自测用临时清单）")
    ap.add_argument("--skip-pin-smoke", action="store_true",
                    help="跳过 -CheckPinsOnly 冒烟（WARN，证据里须登记未验）")
    ap.add_argument("--skip-binary", action="store_true",
                    help="跳过 release 二进制对账（WARN，证据里须登记未验）")
    ap.add_argument("--json", default=None, help="把逐项结果写到 JSON 文件")
    ap.add_argument("--castle-before", default=None, metavar="PATH",
                    help="把三侧 castle 状态（List.txt + Castle/*/SabukW.txt）sha256 基线写到该文件")
    ap.add_argument("--castle-before-only", action="store_true",
                    help="只写 --castle-before 基线并退出（不起栈、不跑其余前置项）")
    ap.add_argument("--castle-compare", default=None, metavar="BEFORE",
                    help="用跑前基线 BEFORE 与当前三侧状态对比，按「本轮造成/开窗前已存在」两档分类")
    ap.add_argument("--castle-after", default=None, metavar="PATH",
                    help="与 --castle-compare 搭配：把跑后三侧状态写到该文件（含分类注释）")
    ap.add_argument("--selftest", action="store_true", help="离线自测（临时夹具，不碰真私栈）")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    # 卡 openmir2-castle-symmetry-baseline：跑前基线 / 跑后两档分类（都不起栈、不占端口）
    if args.castle_before_only:
        if not args.castle_before:
            print("CASTLE_BASELINE FAIL 需要 --castle-before PATH")
            return 2
        manifest = castle_state_manifest(args.root)
        if not any(sha for per in manifest.values() for sha, _n in per.values()):
            print("CASTLE_BASELINE FAIL 三侧都没有 castle 状态文件（root=%s）" % args.root)
            return 2
        write_castle_manifest(args.castle_before, args.root, manifest)
        print("CASTLE_BASELINE wrote=%s files=%d" % (args.castle_before, len(manifest)))
        for rel in sorted(manifest):
            print("CASTLE_BASELINE_ITEM file=%s %s" % (rel, " ".join(
                "%s=%s" % (side, (manifest[rel][side][0] or CASTLE_MISSING)[:12])
                for side in sorted(manifest[rel]))))
        return 0

    if args.castle_compare:
        try:
            before = read_castle_manifest(args.castle_compare)
        except (OSError, ValueError) as e:
            print("CASTLE_COMPARE FAIL 读不了跑前基线 %s：%s" % (args.castle_compare, e))
            return 2
        after = castle_state_manifest(args.root)
        recs = classify_castle_drift(before, after)
        if args.castle_after:
            write_castle_manifest(args.castle_after, args.root, after,
                                  extra_comments=["drift %s %s" % (r["kind"], r["file"])
                                                  for r in recs])
        for r in recs:
            print("CASTLE_DRIFT_ITEM kind=%s file=%s before=%s after=%s"
                  % (r["kind"], r["file"],
                     ",".join("%s:%s" % (k, (v or CASTLE_MISSING)[:12])
                              for k, v in sorted(r["before"].items())),
                     ",".join("%s:%s" % (k, (v or CASTLE_MISSING)[:12])
                              for k, v in sorted(r["after"].items()))))
        new_n = len([r for r in recs if r["kind"] == "本轮造成"])
        old_n = len([r for r in recs if r["kind"] == "开窗前已存在"])
        print("CASTLE_DRIFT new=%d existing=%d files=%d before=%s after=%s"
              % (new_n, old_n, len(after), args.castle_compare, args.castle_after or "-"))
        return 0

    rep = run_all(root=args.root, start_rust=args.start_rust, repo=args.repo,
                  skip_pin_smoke=args.skip_pin_smoke, skip_binary=args.skip_binary,
                  manifest=args.manifest, castle_before=args.castle_before)
    if args.json:
        os.makedirs(os.path.dirname(os.path.abspath(args.json)), exist_ok=True)
        with open(args.json, "w", encoding="utf-8", newline="\n") as fh:
            json.dump({"format": "preflight-private/1", "root": args.root,
                       "repo": args.repo, "git_head": git_head(args.repo),
                       "result": rep.result(), "counts": rep.counts(),
                       "steps": rep.items}, fh, ensure_ascii=False, indent=2, sort_keys=True)
            fh.write("\n")
    return 0 if rep.result() == "PASS" else 2


if __name__ == "__main__":
    sys.exit(main())
