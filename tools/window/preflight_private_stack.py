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
  ⑦ roster       run3 的 17 场景清单（`tools/window/run3-scenarios.json`，不含已封存 pickup_item）与场景文件/
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
        problems.append("alt.rust.env 不该有 %s（alt 是 17 场景基线，不能被窗口 pin 污染）"
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
    if len(scen) != 17:
        problems.append("场景数 %d（应为 17）" % len(scen))
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
    return OK, "17 场景（alt×16 + window-magic×1：spell_cast），不含 pickup_item，rules.json 在位", None


# ---------------------------------------------------------------------------
# 全量检查
# ---------------------------------------------------------------------------
def run_all(root=DEFAULT_PRIVATE_ROOT, start_rust=DEFAULT_START_RUST, repo=REPO,
            skip_pin_smoke=False, skip_binary=False, report=None):
    rep = report or Report()
    status, detail, fix = check_inventory(root)
    rep.add("inventory", status, detail, fix)

    status, detail, fix = check_selgate(root)
    rep.add("selgate_listenport", status, detail, fix)

    status, detail, fix = check_route525(root)
    rep.add("route_525", status, detail, fix)

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

    status, detail, fix = check_roster(repo)
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

        # roster：17 场景清单；已封存 pickup_item 不得入列（reviewer-21 W-1）
        fake_repo = os.path.join(tmp, "roster-repo")
        scen_dir = os.path.join(fake_repo, "tools", "probe", "scenarios")
        os.makedirs(scen_dir, exist_ok=True)
        with open(MANIFEST, encoding="utf-8-sig") as fh:
            good_manifest = json.load(fh)
        for s_ in good_manifest["scenarios"]:
            with open(os.path.join(scen_dir, s_["name"] + ".json"), "w") as fh:
                fh.write("{}\n")
        with open(os.path.join(scen_dir, "pickup_item.json"), "w") as fh:
            fh.write("{}\n")
        good_path = os.path.join(tmp, "run3-good.json")
        with open(good_path, "w", encoding="utf-8") as fh:
            json.dump(good_manifest, fh)
        chk(check_roster(repo=fake_repo, manifest=good_path)[0] == OK,
            "roster: 17 场景且不含 pickup_item => OK")
        bad18 = json.loads(json.dumps(good_manifest))
        bad18["scenarios"].append({"n": 18, "name": "pickup_item", "profile": "alt", "extra": []})
        bad18_path = os.path.join(tmp, "run3-bad18.json")
        with open(bad18_path, "w", encoding="utf-8") as fh:
            json.dump(bad18, fh)
        chk(check_roster(repo=fake_repo, manifest=bad18_path)[0] == FAIL,
            "牙: 清单加回 pickup_item（18 条）=> FAIL")
        bad17 = json.loads(json.dumps(good_manifest))
        bad17["scenarios"][-1]["name"] = "pickup_item"
        bad17_path = os.path.join(tmp, "run3-bad17.json")
        with open(bad17_path, "w", encoding="utf-8") as fh:
            json.dump(bad17, fh)
        chk(check_roster(repo=fake_repo, manifest=bad17_path)[0] == FAIL,
            "牙: 17 条但含 pickup_item => FAIL")
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
    ap.add_argument("--skip-pin-smoke", action="store_true",
                    help="跳过 -CheckPinsOnly 冒烟（WARN，证据里须登记未验）")
    ap.add_argument("--skip-binary", action="store_true",
                    help="跳过 release 二进制对账（WARN，证据里须登记未验）")
    ap.add_argument("--json", default=None, help="把逐项结果写到 JSON 文件")
    ap.add_argument("--selftest", action="store_true", help="离线自测（临时夹具，不碰真私栈）")
    args = ap.parse_args(argv)

    if args.selftest:
        return selftest()

    rep = run_all(root=args.root, start_rust=args.start_rust, repo=args.repo,
                  skip_pin_smoke=args.skip_pin_smoke, skip_binary=args.skip_binary)
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
