#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""card_window.py — 收一张 R 卡的窗口项：一条命令跑完双栈对拍并归档证据。

把 2026-10-23 手工做过 3 次的流程固化成一条命令（语义与 tools/conformance/run_dual.sh 一致，
**run_dual.sh 本身不改**）：

    restore baseline -> 起 C# 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
                     -> restore baseline -> 起 Rust 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
                     -> compare -> 归档（report.json / README.txt / card-window.json）

只调用、不修改三个既有工具：
  * tools/probe/mir2_probe.py          （无头驱动器，--out 落 frames.ndjson / run.json）
  * tools/conformance/compare_runs.py  （帧流 + 库后态比较器，写 report.json）
  * tools/dbsnap/dbsnap.py             （库快照，写 <side>/db/）

产物布局（固定）：
    <out>/csharp/frames.ndjson
    <out>/csharp/run.json
    <out>/csharp/db/                  （dbsnap manifest.json + tsv）
    <out>/csharp/db_snapshot.json     （{"snapshot_dir": "db"}，给比较器定位库快照）
    <out>/rust/...                    （同上）
    <out>/report.json                 （compare_runs.py 的原样报告）
    <out>/card-window.json            （本工具的编排摘要）
    <out>/card-window.log             （逐条命令 + stdout/stderr）
    <out>/README.txt                  （人读：sha256 / 命令全文 / 时间戳 / verdict）

安全口径（fail-closed；退出码 0 = PASS，其余一律 2）：
  * **起每一侧栈之前**检查该侧端口占用；被占 => rc=2 并点名占用 pid（不启动栈）。
  * 起栈命令返回 0 之后再**轮询就绪端口**；栈没起来（端口不监听）=> rc=2，绝不算对拍通过。
  * probe 跑完检查 frames.ndjson / run.json 存在且非空；0 帧 => rc=2。
  * 库快照失败 / 缺 manifest => rc=2（库判据不可缺）。
  * 停栈在 finally 里执行：无论恢复/起栈/probe/抓库哪一步失败都会尝试停栈。
  * **不碰 MySQL 服务本身**：停栈命令若含 mysqld / Stop-Service / net stop / mysqladmin 直接拒绝。
  * 默认拒绝指向生产库（mir2_db / mir2_account / mir2_data / database=Mir2）的配置；
    只有显式 --allow-production-db 才放行（内容冻结，窗口只跑测试库）。

profile：
  * 内置 `alt`（默认）= 2026-10-23 双栈窗口所用的备用端口剖面
    （E:/tmp/conformance 私有根 + mir2_test_* / mir2_test_*_r 测试库）。
  * 内置 `csharp-default` = C# 默认端口栈（E:/MirServer，**生产库**）。默认拒绝运行，
    必须显式 --allow-production-db，且只有 owner 授权的窗口才允许（见 docs 说明）。
  * `--profile <路径.json>` 也可直接加载自定义 profile（format = card-window-profile/1）。

用法：
  CONFORMANCE_PASSWORD=... python -I -B tools/conformance/card_window.py \
      --card openmir2-window-529-dualrun \
      --scenario tools/probe/scenarios/login_only.json \
      --out E:/tmp/conformance/runs/529-card-window \
      --profile alt

  # 只打印将要执行的完整命令序列（不起栈、不建目录、不需要密码）
  python -I -B tools/conformance/card_window.py --card X --scenario S --out O --dry-run

  # 离线自检（桩命令 + 合成 frames，不连网/不连库/不起真栈）
  python -I -B tools/conformance/card_window.py --selftest

  # 跑完自动在该卡发一条签名 comment（只发 comment，绝不改卡状态）
  ... --post-comment

退出码：0 = compare PASS 且所有编排步骤成功；2 = 其它一切（含 compare FAIL / UNDECIDABLE /
        栈没起来 / 端口被占 / 帧缺失 / 库快照失败 / comment 失败）。compare 的真实 rc（0/1/2）
        记录在 <out>/report.json 与 <out>/card-window.json 里。
"""
import argparse
import copy
import hashlib
import json
import os
import re
import socket
import subprocess
import sys
import tempfile
import time
import traceback

TOOL = "tools/conformance/card_window.py"
HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
PROBE = os.path.join(REPO_ROOT, "tools", "probe", "mir2_probe.py")
COMPARE = os.path.join(HERE, "compare_runs.py")
DBSNAP = os.path.join(REPO_ROOT, "tools", "dbsnap", "dbsnap.py")
DEFAULT_RULES = os.path.join(HERE, "rules.json")

FORMAT = "card-window/1"
PROFILE_FORMAT = "card-window-profile/1"
SIDES = ("csharp", "rust")

# dbsnap 默认表集（与 tools/dbsnap/dbsnap.py 的 DEFAULT_TABLES 同口径；这里只用于
# 生成 Rust 侧 *_r 克隆库的表名，不复制比较口径）。
ACCOUNT_TABLES = ["mir2_account.account", "mir2_account.account_protection"]
CHARACTER_TABLES = [
    "mir2_db.characters",
    "mir2_db.characters_ablity",
    "mir2_db.characters_bonusability",
    "mir2_db.characters_item",
    "mir2_db.characters_item_attr",
    "mir2_db.characters_magic",
    "mir2_db.characters_bagitem",
    "mir2_db.characters_storageitem",
    "mir2_db.characters_status",
    "mir2_db.characters_quest",
    "mir2_db.characters_indexes",
]
RUST_DB_TABLES = (
    [t.replace("mir2_account.", "mir2_test_account_r.") for t in ACCOUNT_TABLES]
    + [t.replace("mir2_db.", "mir2_test_db_r.") for t in CHARACTER_TABLES]
)

POWERSHELL = ["powershell", "-NoProfile", "-ExecutionPolicy", "Bypass"]
CSHARP_STOP = POWERSHELL + [
    "-Command",
    "Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -like "
    "'E:\\tmp\\conformance\\csharp-root\\*' } | ForEach-Object { "
    "Stop-Process -Id $_.ProcessId -Force }",
]

BUILTIN_PROFILES = {
    "alt": {
        "format": PROFILE_FORMAT,
        "name": "alt",
        "description": ("备用端口剖面（2026-10-23 双栈窗口）：E:/tmp/conformance 私有根，"
                        "测试库 mir2_test_* / mir2_test_*_r，同一组 15xxx/17xxx 端口，两栈串行。"),
        "login_gate": "127.0.0.1:17000",
        "account": "winplay0",
        "server_name": "热血传奇",
        "char_name": "winplay0",
        "probe_timeout_ms": 20000,
        "ready_timeout_s": 120,
        "compare_extra": [],
        "sides": {
            "csharp": {
                "label": "C# oracle",
                "stack": POWERSHELL + ["-File", "E:/tmp/conformance/start-csharp3.ps1"],
                "stop": CSHARP_STOP,
                "restore": ["bash", "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [15000, 15100, 15500, 15600, 15700, 16000, 17000, 17100,
                          17200, 17201, 17202],
                "ready_ports": [15000, 17000, 17100, 17200],
                "probe_extra": [],
                "binaries": ["E:/tmp/conformance/csharp-root/M2GameSvr/GameSrv.exe"],
                "db": {"preset": "test"},
            },
            "rust": {
                "label": "Rust",
                "stack": POWERSHELL + ["-File", "E:/tmp/conformance/start-rust.ps1"],
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust.ps1"],
                "restore": ["bash", "E:/tmp/conformance/restore-baseline-r.sh"],
                "ports": [15000, 15100, 15500, 15600, 15700, 16000, 17000, 17100, 17600],
                "ready_ports": [15000, 17000, 17100, 17600],
                "probe_extra": ["--game-gate", "127.0.0.1:17600"],
                "binaries": ["E:/tmp/conformance/rust-root/M2GameSvr/GameSrv.exe"],
                "db": {"tables": RUST_DB_TABLES,
                       "allow_db": ["mir2_test_db_r", "mir2_test_account_r"]},
            },
        },
    },
    "csharp-default": {
        "format": PROFILE_FORMAT,
        "name": "csharp-default",
        "description": ("C# 默认端口栈：E:/MirServer（5000/5100/5500/5600/5700/6000/7000/7100/7200），"
                        "ConnctionString 指向**生产库** mir2_db / mir2_data / Mir2。"
                        "默认拒绝运行（内容冻结，窗口只跑测试库）。"),
        "production_db": True,
        "login_gate": "127.0.0.1:7000",
        "probe_timeout_ms": 20000,
        "ready_timeout_s": 120,
        "compare_extra": [],
        "sides": {
            "csharp": {
                "label": "C# oracle（默认端口/生产库）",
                "stack": POWERSHELL + ["-File", "E:/MirServer/start-all.ps1"],
                "stop": POWERSHELL + ["-File", "E:/MirServer/start-all.ps1", "-Stop"],
                "restore": ["bash", "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200],
                "ready_ports": [5000, 7000, 7100, 7200],
                "probe_extra": [],
                "binaries": ["E:/MirServer/M2GameSvr/GameSrv.exe"],
                "db": {"preset": "prod"},
            },
            "rust": {
                "label": "Rust（默认端口，需部署在 E:/tmp/conformance/rust-default-root）",
                "stack": POWERSHELL + ["-File", "E:/tmp/conformance/start-rust-default.ps1"],
                "stop": POWERSHELL + ["-File", "E:/tmp/conformance/stop-rust-default.ps1"],
                "restore": ["bash", "E:/tmp/conformance/restore-baseline.sh"],
                "ports": [5000, 5100, 5500, 5600, 5700, 6000, 7000, 7100, 7200],
                "ready_ports": [5000, 7000, 7100, 7200],
                "probe_extra": [],
                "binaries": ["E:/tmp/conformance/rust-default-root/M2GameSvr/GameSrv.exe"],
                "db": {"preset": "test"},
            },
        },
    },
}

PROD_DB_RE = re.compile(r"(?<![A-Za-z0-9_])mir2_(db|account|data)(?![A-Za-z0-9_])", re.I)
PROD_LOGIN_RE = re.compile(r"database\s*=\s*Mir2(?![A-Za-z0-9_])", re.I)
STOP_FORBIDDEN = ("mysqld", "stop-service", "net stop", "mysqladmin", "restart-service")


class ConfigError(Exception):
    """配置/用法错误 => 退出码 2（fail-closed）。"""


# ---------------------------------------------------------------------------
# 基础工具
# ---------------------------------------------------------------------------
def sha256_file(path):
    h = hashlib.sha256()
    with open(path, "rb") as f:
        for chunk in iter(lambda: f.read(65536), b""):
            h.update(chunk)
    return h.hexdigest()


def iso_now():
    return time.strftime("%Y-%m-%dT%H:%M:%S%z", time.localtime())


def _fmt_duration(sec):
    return "%.1fs" % sec


def _join_argv(argv):
    out = []
    for a in argv:
        a = str(a)
        if not a or any(c in a for c in " \t\"'|&;<>()$`"):
            out.append('"%s"' % a.replace('"', '\\"'))
        else:
            out.append(a)
    return " ".join(out)


def _redact_argv(argv):
    """命令行进日志/README 前把 --password 的值抹掉（真跑仍用原 argv）。"""
    out = list(argv)
    redact = False
    for i, a in enumerate(out):
        if redact:
            out[i] = "***"
            redact = False
        elif a == "--password":
            redact = True
    return out


def expand_argv(argv, ctx):
    """把 {side}/{out}/{side_dir}/{db_dir}/{scenario}/{repo}/{card}/{python} 占位符展开。"""
    if isinstance(argv, str):
        raise ConfigError("命令必须是 argv 数组（不要用字符串，避免 shell 注入/引号歧义）：%r" % argv)
    if not isinstance(argv, list) or not argv:
        raise ConfigError("命令必须是非空 argv 数组：%r" % (argv,))
    out = []
    for a in argv:
        if not isinstance(a, str):
            raise ConfigError("argv 元素必须是字符串：%r" % (a,))
        for k, v in ctx.items():
            a = a.replace("{" + k + "}", str(v))
        out.append(a)
    return out


def netstat_table():
    """返回 [(port, pid)]（LISTEN 状态）；取不到时返回 []（不静默当作空闲）。"""
    try:
        p = subprocess.run(["netstat", "-ano"], capture_output=True, text=True,
                           encoding="utf-8", errors="replace", timeout=30)
    except (OSError, subprocess.SubprocessError):
        return []
    rows = []
    for line in p.stdout.splitlines():
        parts = line.split()
        if len(parts) < 5:
            continue
        if not any(x.upper().startswith("LISTEN") for x in parts):
            continue
        local = None
        for tok in parts[1:4]:
            if re.match(r"^(\[[^\]]+\]|[^:]+):\d+$", tok):
                local = tok
                break
        if not local:
            continue
        try:
            port = int(local.rsplit(":", 1)[1])
        except ValueError:
            continue
        pid = None
        for tok in reversed(parts):
            if tok.isdigit():
                pid = int(tok)
                break
        rows.append((port, pid))
    return rows


def port_busy(port):
    """占用判定：能连上，或在 0.0.0.0 上绑不上。"""
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(0.3)
    try:
        if s.connect_ex(("127.0.0.1", int(port))) == 0:
            return True
    finally:
        s.close()
    b = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    try:
        if hasattr(socket, "SO_EXCLUSIVEADDRUSE"):
            b.setsockopt(socket.SOL_SOCKET, socket.SO_EXCLUSIVEADDRUSE, 1)
        b.bind(("0.0.0.0", int(port)))
        return False
    except OSError:
        return True
    finally:
        b.close()


def port_open(port):
    s = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    s.settimeout(0.3)
    try:
        return s.connect_ex(("127.0.0.1", int(port))) == 0
    finally:
        s.close()


def check_ports_free(ports):
    """返回 busy 列表 [{port, pid}]；pid 可能为 None（取不到时点名端口）。"""
    if not ports:
        return []
    pid_by_port = {}
    for port, pid in netstat_table():
        if pid is not None and port not in pid_by_port:
            pid_by_port[port] = pid
    busy = []
    for port in ports:
        if port_busy(port):
            busy.append({"port": int(port), "pid": pid_by_port.get(int(port))})
    return busy


def wait_ready(ports, timeout_s):
    """轮询直到所有端口可连或超时；返回未就绪端口列表。"""
    pending = [int(p) for p in ports or []]
    if not pending:
        return []
    deadline = time.monotonic() + max(0.0, float(timeout_s))
    while True:
        pending = [p for p in pending if not port_open(p)]
        if not pending:
            return []
        if time.monotonic() >= deadline:
            return pending
        time.sleep(0.4)


# ---------------------------------------------------------------------------
# profile
# ---------------------------------------------------------------------------
def load_profile(value):
    if isinstance(value, dict):
        prof = copy.deepcopy(value)
    else:
        name = str(value or "").strip()
        if name.lower() in BUILTIN_PROFILES:
            prof = copy.deepcopy(BUILTIN_PROFILES[name.lower()])
        elif os.path.isfile(name):
            try:
                with open(name, "r", encoding="utf-8-sig") as f:
                    prof = json.load(f)
            except (OSError, ValueError) as e:
                raise ConfigError("profile 读不了：%s（%s）" % (name, e))
        else:
            raise ConfigError("未知 --profile %r：可选 %s，或给一个 profile JSON 路径"
                              % (value, "|".join(sorted(BUILTIN_PROFILES))))
    if not isinstance(prof, dict):
        raise ConfigError("profile 顶层必须是 JSON 对象")
    fmt = prof.get("format")
    if fmt is not None and fmt != PROFILE_FORMAT:
        raise ConfigError("profile.format 必须是 %r（实际 %r）" % (PROFILE_FORMAT, fmt))
    sides = prof.get("sides")
    if not isinstance(sides, dict):
        raise ConfigError("profile 缺 sides 对象")
    for side in SIDES:
        sc = sides.get(side)
        if not isinstance(sc, dict):
            raise ConfigError("profile.sides 缺 %r" % side)
        for key in ("stack", "stop", "restore"):
            if key not in sc:
                raise ConfigError("profile.sides.%s 缺 %r 命令" % (side, key))
        if "db" not in sc:
            raise ConfigError("profile.sides.%s 缺 db 配置（preset / tables / command）" % side)
        for key in ("ports", "ready_ports", "binaries"):
            if not isinstance(sc.get(key, []), list):
                raise ConfigError("profile.sides.%s.%s 必须是数组" % (side, key))
    return prof


def apply_overrides(prof, cfg):
    prof = copy.deepcopy(prof)
    for key in ("login_gate", "account", "server_name", "char_name"):
        val = cfg.get(key)
        if val not in (None, ""):
            prof[key] = val
    if cfg.get("timeout_ms") not in (None, ""):
        prof["probe_timeout_ms"] = cfg["timeout_ms"]
    env_map = {
        "login_gate": "CONFORMANCE_LOGIN_GATE",
        "account": "CONFORMANCE_ACCOUNT",
        "server_name": "CONFORMANCE_SERVER_NAME",
        "char_name": "CONFORMANCE_CHAR_NAME",
    }
    for key, env_name in env_map.items():
        val = os.environ.get(env_name)
        if val and not cfg.get(key):
            prof[key] = val
    return prof


def _any_argv_text(prof):
    texts = []
    for side in SIDES:
        sc = prof["sides"][side]
        for key in ("stack", "stop", "restore"):
            argv = sc.get(key)
            if isinstance(argv, list):
                texts.append(" ".join(str(x) for x in argv))
        db = sc.get("db") or {}
        if isinstance(db.get("command"), list):
            texts.append(" ".join(str(x) for x in db["command"]))
        for t in db.get("tables") or []:
            texts.append(str(t))
        for d in db.get("allow_db") or []:
            texts.append(str(d))
        texts.append(str(db.get("preset") or ""))
    texts.append(" ".join(str(x) for x in prof.get("compare_extra") or []))
    return "\n".join(texts)


def check_profile_safety(prof, allow_production_db, dry_run=False):
    """(a) 停栈命令不许碰 mysqld； (b) 默认拒绝生产库。失败 => ConfigError。"""
    warnings = []
    for side in SIDES:
        stop_argv = prof["sides"][side].get("stop")
        if isinstance(stop_argv, list):
            low = " ".join(str(x) for x in stop_argv).lower()
            for bad in STOP_FORBIDDEN:
                if bad in low:
                    raise ConfigError(
                        "profile.sides.%s.stop 含 %r：停栈命令不允许操作 MySQL 服务本身"
                        "（只停游戏栈；MySQL 由 owner 保持运行）" % (side, bad))
    text = _any_argv_text(prof)
    hits = sorted(set(m.group(0) for m in PROD_DB_RE.finditer(text))
                  | set(m.group(0) for m in PROD_LOGIN_RE.finditer(text)))
    if (hits or prof.get("production_db")) and not allow_production_db:
        detail = ("命中 %s" % ", ".join(hits)) if hits else "profile 标记 production_db=true"
        if dry_run:
            warnings.append("dry-run 未实际访问生产库，但 profile 指向生产库（%s）；"
                            "真跑会被拒绝，除非 --allow-production-db" % detail)
        else:
            raise ConfigError(
                "拒绝在窗口里碰生产库（%s）：内容冻结，窗口只跑测试库。"
                "确需 owner 授权的生产窗口时才可加 --allow-production-db。" % detail)
    if hits and allow_production_db:
        warnings.append("已显式放行生产库：%s" % ", ".join(hits))
    return warnings


# ---------------------------------------------------------------------------
# 执行引擎
# ---------------------------------------------------------------------------
class Runner(object):
    def __init__(self, out_dir, log_path, dry_run=False):
        self.out_dir = out_dir
        self.log_path = log_path
        self.dry_run = dry_run
        self.commands = []
        self.steps = []

    def log(self, msg):
        line = "%s card_window: %s" % (time.strftime("%H:%M:%S"), msg)
        try:
            print(line, file=sys.stderr)
        except Exception:
            pass
        if self.log_path and not self.dry_run:
            with open(self.log_path, "a", encoding="utf-8", newline="\n") as f:
                f.write(line + "\n")

    def run(self, side, step, argv, env=None, timeout=600):
        safe = _redact_argv(argv)
        self.commands.append({"side": side, "step": step, "argv": safe})
        self.steps.append("%s:%s" % (side, step))
        self.log("== %s/%s :: %s" % (side, step, _join_argv(safe)))
        if self.dry_run:
            return 0
        try:
            p = subprocess.run(argv, capture_output=True, text=True, encoding="utf-8",
                               errors="replace", timeout=timeout,
                               env=env, cwd=REPO_ROOT)
            rc = p.returncode
            out = (p.stdout or "") + (p.stderr or "")
        except subprocess.TimeoutExpired as e:
            rc = 124
            out = "TIMEOUT after %ss\n%s\n%s" % (
                timeout, e.stdout or "", e.stderr or "")
        except OSError as e:
            rc = 127
            out = "OSError: %s" % e
        if out:
            with open(self.log_path, "a", encoding="utf-8", newline="\n") as f:
                f.write("---- %s/%s rc=%s ----\n%s\n" % (side, step, rc, out))
        self.log("   %s/%s rc=%s" % (side, step, rc))
        return rc


def side_ctx(side, out_dir, scenario, card):
    side_dir = os.path.join(out_dir, side)
    return {
        "repo": REPO_ROOT,
        "out": out_dir,
        "side": side,
        "side_dir": side_dir,
        "db_dir": os.path.join(side_dir, "db"),
        "scenario": scenario,
        "card": card,
        "python": sys.executable,
        "probe": PROBE,
        "dbsnap": DBSNAP,
    }


def build_probe_argv(side_cfg, ctx, prof, password):
    base = side_cfg.get("probe") or [sys.executable, "-I", "-B", PROBE]
    argv = expand_argv(base, ctx)
    argv += ["--login-gate", str(prof.get("login_gate") or "")]
    argv += ["--account", str(prof.get("account") or "")]
    argv += ["--password", str(password)]
    argv += ["--scenario", ctx["scenario"]]
    argv += ["--out", ctx["side_dir"]]
    if prof.get("server_name"):
        argv += ["--server-name", str(prof["server_name"])]
    if prof.get("char_name"):
        argv += ["--char-name", str(prof["char_name"])]
    if prof.get("probe_timeout_ms"):
        argv += ["--timeout-ms", str(int(prof["probe_timeout_ms"]))]
    for extra in side_cfg.get("probe_extra") or []:
        argv.append(str(extra))
    return argv


def build_db_argv(side_cfg, ctx):
    db = side_cfg.get("db") or {}
    if db.get("command"):
        return expand_argv(db["command"], ctx)
    argv = [sys.executable, "-I", "-B", DBSNAP, "snap", "--out", ctx["db_dir"]]
    if db.get("preset"):
        argv += ["--db", str(db["preset"])]
    else:
        tables = db.get("tables") or []
        if not tables:
            raise ConfigError("profile db 既无 preset 也无 tables/command：库判据不可缺")
        for t in tables:
            argv += ["--table", str(t)]
        for d in db.get("allow_db") or []:
            argv += ["--allow-db", str(d)]
    return argv


def write_db_pointer(side_dir):
    path = os.path.join(side_dir, "db_snapshot.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump({"snapshot_dir": "db", "format": "card-window-db-pointer/1"},
                  f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")


def frame_count(path):
    n = 0
    try:
        with open(path, "r", encoding="utf-8", errors="replace") as f:
            for line in f:
                if line.strip():
                    n += 1
    except OSError:
        return None
    return n


def run_side(runner, side, prof, ctx, password):
    sc = prof["sides"][side]
    side_dir = ctx["side_dir"]
    db_dir = ctx["db_dir"]
    res = {
        "side": side,
        "label": sc.get("label") or side,
        "restore_rc": None,
        "ports": {"checked": list(sc.get("ports") or []), "busy": []},
        "stack_rc": None,
        "ready": {"ports": list(sc.get("ready_ports") or []), "missing": [],
                  "timeout_s": prof.get("ready_timeout_s", 120)},
        "probe_rc": None,
        "db_rc": None,
        "stop_rc": None,
        "frames": None,
        "rc": 2,
        "error": None,
    }
    env = os.environ.copy()
    env.update({
        "CARD_WINDOW_SIDE": side,
        "CARD_WINDOW_OUT": ctx["out"],
        "CARD_WINDOW_SIDE_DIR": side_dir,
        "CARD_WINDOW_DB_DIR": db_dir,
        "CARD_WINDOW_SCENARIO": ctx["scenario"],
        "CARD_WINDOW_CARD": ctx["card"],
    })
    for k, v in (sc.get("env") or {}).items():
        env[str(k)] = str(v)

    os.makedirs(side_dir, exist_ok=True)
    os.makedirs(db_dir, exist_ok=True)

    stack_attempted = False
    try:
        # 1) 恢复基线
        restore_argv = expand_argv(sc["restore"], ctx)
        res["restore_rc"] = runner.run(side, "restore", restore_argv, env=env)
        if res["restore_rc"] != 0:
            res["error"] = "基线恢复失败（restore rc=%s）：不起栈、不跑场景" % res["restore_rc"]
            return res

        # 2) 起栈前端口检查（有人占 => rc=2 并点名 pid；不启动栈）
        busy = check_ports_free(sc.get("ports") or [])
        res["ports"]["busy"] = busy
        if busy:
            parts = ["%d(pid %s)" % (b["port"], b["pid"] if b["pid"] is not None else "?")
                     for b in busy]
            res["error"] = ("起栈前端口已被占用：%s —— 不启动 %s 栈（先停掉占用方）"
                            % (", ".join(parts), side))
            return res

        # 3) 起栈
        stack_argv = expand_argv(sc["stack"], ctx)
        stack_attempted = True
        res["stack_rc"] = runner.run(side, "stack", stack_argv, env=env)
        if res["stack_rc"] != 0:
            res["error"] = "起栈命令失败（rc=%s）：不跑场景（栈没起来不能算对拍通过）" % res["stack_rc"]
            return res

        # 4) 等就绪（栈命令返回 0 也可能没真起来）
        missing = wait_ready(sc.get("ready_ports") or [],
                             prof.get("ready_timeout_s", 120))
        res["ready"]["missing"] = missing
        if missing:
            res["error"] = ("栈未就绪：端口 %s 在 %ss 内未监听（stack rc=0 不算起来）"
                            % (", ".join(str(p) for p in missing),
                               prof.get("ready_timeout_s", 120)))
            return res

        # 5) probe
        probe_argv = build_probe_argv(sc, ctx, prof, password)
        probe_timeout = max(180, int(prof.get("probe_timeout_ms", 20000) / 1000) + 120)
        res["probe_rc"] = runner.run(side, "probe", probe_argv, env=env,
                                     timeout=probe_timeout)
        # 6) 抓库（probe 失败也拍，保留现场；缺快照 => 失败）
        db_argv = build_db_argv(sc, ctx)
        res["db_rc"] = runner.run(side, "db", db_argv, env=env, timeout=600)
        frames_path = os.path.join(side_dir, "frames.ndjson")
        res["frames"] = frame_count(frames_path)
        if res["db_rc"] == 0:
            if os.path.exists(os.path.join(db_dir, "manifest.json")):
                write_db_pointer(side_dir)
            else:
                res["db_rc"] = 2
                res["error"] = "库快照命令返回 0 但没有 manifest.json：%s" % db_dir
        else:
            res["error"] = "库快照失败（db rc=%s）：库判据不可缺" % res["db_rc"]

        if res["probe_rc"] != 0:
            msg = "probe 失败（rc=%s）" % res["probe_rc"]
            res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
            return res
        if res["frames"] is None:
            res["error"] = "缺 frames.ndjson：%s" % frames_path
            return res
        if res["frames"] == 0:
            res["error"] = "probe 产出 0 帧（frames.ndjson 为空）：不能算对拍通过"
            return res
        if not os.path.exists(os.path.join(side_dir, "run.json")):
            res["error"] = "probe 未落 run.json：%s" % os.path.join(side_dir, "run.json")
            return res
        if res["db_rc"] != 0:
            return res
        res["rc"] = 0
        return res
    finally:
        # 7) 停栈：只要尝试过起栈就执行（无论成败）；不做这一步会占端口、污染下一侧
        if stack_attempted:
            try:
                stop_argv = expand_argv(sc["stop"], ctx)
                res["stop_rc"] = runner.run(side, "stop", stop_argv, env=env, timeout=180)
            except (ConfigError, OSError) as e:
                res["stop_rc"] = None
                res["error"] = (res["error"] or "") + "；停栈命令失败：%s" % e
        if res["rc"] == 0 and res["stop_rc"] not in (0, None):
            res["rc"] = 2
            res["error"] = "停栈失败（rc=%s）：端口可能仍被占用" % res["stop_rc"]


def run_compare(runner, out_dir, prof, rules):
    compare_argv = [sys.executable, "-I", "-B", COMPARE,
                    "--oracle", os.path.join(out_dir, "csharp"),
                    "--rust", os.path.join(out_dir, "rust"),
                    "--out", os.path.join(out_dir, "report.json"),
                    "--rules", rules]
    for extra in prof.get("compare_extra") or []:
        compare_argv.append(str(extra))
    return runner.run("compare", "compare", compare_argv, timeout=600)


def read_report(path):
    try:
        with open(path, "r", encoding="utf-8") as f:
            return json.load(f)
    except (OSError, ValueError):
        return None


def bin_info(sc):
    out = []
    for path in sc.get("binaries") or []:
        path = str(path)
        item = {"path": path, "sha256": None, "exists": os.path.exists(path)}
        if item["exists"]:
            try:
                item["sha256"] = sha256_file(path)
            except OSError:
                item["sha256"] = None
        out.append(item)
    return out


def write_readme(path, summary):
    lines = []
    lines.append("CARD WINDOW EVIDENCE")
    lines.append("====================")
    lines.append("card: %s" % summary.get("card"))
    lines.append("tool: %s" % summary.get("tool"))
    lines.append("profile: %s" % summary.get("profile"))
    if summary.get("profile_description"):
        lines.append("profile_description: %s" % summary["profile_description"])
    lines.append("scenario: %s" % summary.get("scenario"))
    lines.append("scenario_sha256: %s" % summary.get("scenario_sha256"))
    lines.append("started_at: %s" % summary.get("started_at"))
    lines.append("finished_at: %s" % summary.get("finished_at"))
    lines.append("duration_s: %s" % summary.get("duration_s"))
    lines.append("verdict: %s" % summary.get("verdict"))
    lines.append("compare_rc: %s" % summary.get("compare_rc"))
    lines.append("exit_code: %s" % summary.get("exit_code"))
    frames = summary.get("frames") or {}
    lines.append("frames: csharp=%s rust=%s compared=%s"
                 % (frames.get("oracle_total"), frames.get("rust_total"),
                    frames.get("compared")))
    lines.append("db_state: %s" % summary.get("db_state"))
    lines.append("diffs: %s   undecidable: %s   whitelisted: %s"
                 % (summary.get("diffs"), summary.get("undecidable"),
                    summary.get("whitelisted")))
    lines.append("")
    lines.append("binaries (sha256):")
    any_bin = False
    for side in SIDES:
        for b in (summary.get("binaries") or {}).get(side, []):
            any_bin = True
            lines.append("  %-6s %s" % (side, b.get("path")))
            lines.append("         sha256=%s exists=%s" % (b.get("sha256"), b.get("exists")))
    if not any_bin:
        lines.append("  (none configured)")
    lines.append("")
    lines.append("artifacts:")
    for name, info in sorted((summary.get("artifacts") or {}).items()):
        if isinstance(info, dict):
            lines.append("  %-24s %s" % (name, info.get("path")))
            if info.get("sha256"):
                lines.append("  %-24s sha256=%s" % ("", info["sha256"]))
    lines.append("")
    lines.append("command line (full):")
    lines.append("  %s" % summary.get("tool_command_line"))
    lines.append("")
    lines.append("orchestration commands (full):")
    for cmd in summary.get("commands") or []:
        lines.append("  [%s] %-7s %s" % (cmd.get("side"), cmd.get("step"),
                                         _join_argv(cmd.get("argv") or [])))
    lines.append("")
    lines.append("side results:")
    for side in SIDES:
        r = (summary.get("sides") or {}).get(side) or {}
        lines.append("  %-6s rc=%s restore=%s stack=%s ready_missing=%s probe=%s db=%s stop=%s frames=%s"
                     % (side, r.get("rc"), r.get("restore_rc"), r.get("stack_rc"),
                        r.get("ready", {}).get("missing"), r.get("probe_rc"),
                        r.get("db_rc"), r.get("stop_rc"), r.get("frames")))
        if r.get("ports", {}).get("busy"):
            lines.append("         busy_ports=%s" % r["ports"]["busy"])
        if r.get("error"):
            lines.append("         error=%s" % r["error"])
    lines.append("")
    lines.append("notes:")
    for note in summary.get("notes") or []:
        lines.append("  - %s" % note)
    lines.append("")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.write("\n".join(lines))
    return path


def git_head(repo):
    try:
        p = subprocess.run(["git", "-C", repo, "rev-parse", "HEAD"], capture_output=True,
                           text=True, encoding="utf-8", errors="replace", timeout=30)
        return p.stdout.strip() if p.returncode == 0 else None
    except (OSError, subprocess.SubprocessError):
        return None


def post_comment(card, body, repo):
    """在该卡发一条签名 comment；只写 kind=comment，绝不改卡状态。返回 (ok, detail)。"""
    thread_cmd = ["walgit", "collab", "thread", card, "--repo", repo]
    parent = ""
    detail = ""
    for attempt in (1, 2):
        try:
            p = subprocess.run(thread_cmd, capture_output=True, text=True,
                               encoding="utf-8", errors="replace", timeout=120)
            if p.returncode == 0:
                entries = json.loads(p.stdout or "[]")
                if entries:
                    parent = entries[-1].get("oid") or ""
                break
            detail = "读线程失败 rc=%s: %s" % (p.returncode, (p.stderr or "")[:300])
        except (OSError, subprocess.SubprocessError, ValueError) as e:
            detail = "读线程失败: %s" % e
        if attempt == 1:
            time.sleep(2)
    else:
        return False, detail

    entry_cmd = ["walgit", "collab", "entry", "--repo", repo, "--kind", "comment",
                 "--id", card, "--parent", parent,
                 "--body", json.dumps(body, ensure_ascii=False, separators=(",", ":")),
                 "--push", "walgit"]
    last = ""
    for attempt in (1, 2):
        try:
            p = subprocess.run(entry_cmd, capture_output=True, text=True,
                               encoding="utf-8", errors="replace", timeout=180)
            if p.returncode == 0:
                return True, (p.stdout or "").strip()
            last = "发 comment 失败 rc=%s: %s" % (p.returncode,
                                                  (p.stderr or p.stdout or "")[:400])
        except (OSError, subprocess.SubprocessError) as e:
            last = "发 comment 失败: %s" % e
        if attempt == 1:
            time.sleep(2)
    return False, last


# ---------------------------------------------------------------------------
# 主流程
# ---------------------------------------------------------------------------
def execute(cfg):
    """cfg: dict(card, scenario, out, profile, password, post_comment, dry_run,
                 allow_production_db, rules, timeout_ms, ...) -> summary"""
    dry_run = bool(cfg.get("dry_run"))
    prof = load_profile(cfg.get("profile") if cfg.get("profile") is not None else "alt")
    prof = apply_overrides(prof, cfg)
    warnings = check_profile_safety(prof, bool(cfg.get("allow_production_db")), dry_run)

    out_dir = os.path.abspath(cfg.get("out") or ".")
    scenario = os.path.abspath(cfg.get("scenario") or "")
    card = cfg.get("card") or ""
    rules = os.path.abspath(cfg.get("rules") or DEFAULT_RULES)

    started_at = iso_now()
    t0 = time.monotonic()
    summary = {
        "format": FORMAT,
        "tool": TOOL,
        "card": card,
        "profile": prof.get("name") or "custom",
        "profile_description": prof.get("description"),
        "scenario": scenario,
        "scenario_sha256": None,
        "out": out_dir,
        "started_at": started_at,
        "finished_at": None,
        "duration_s": None,
        "git_head": git_head(REPO_ROOT),
        "verdict": "UNDECIDABLE",
        "compare_rc": None,
        "frames": {},
        "db_state": None,
        "diffs": None,
        "undecidable": None,
        "whitelisted": None,
        "binaries": {side: bin_info(prof["sides"][side]) for side in SIDES},
        "sides": {},
        "artifacts": {},
        "commands": [],
        "tool_command_line": _join_argv(sys.argv if sys.argv else [TOOL]),
        "notes": [
            "编排语义与 tools/conformance/run_dual.sh 一致；run_dual.sh / compare_runs.py / "
            "mir2_probe.py / dbsnap.py 均未改动（只调用）。",
            "起栈前端口检查、就绪等待、失败也停栈；栈没起来 / 0 帧 / 缺库快照一律不判 PASS。",
            "不碰 MySQL 服务本身（停栈命令禁含 mysqld/Stop-Service/net stop/mysqladmin）。",
        ] + list(warnings),
        "exit_code": 2,
    }

    if scenario and os.path.isfile(scenario):
        summary["scenario_sha256"] = sha256_file(scenario)

    password = cfg.get("password") or os.environ.get("CONFORMANCE_PASSWORD")
    if not password:
        if dry_run:
            password = "<CONFORMANCE_PASSWORD unset>"
        else:
            summary["notes"].append(
                "缺密码：用环境变量 CONFORMANCE_PASSWORD 提供（不要写进命令行历史）")
            return summary

    if dry_run:
        runner = Runner(out_dir, None, dry_run=True)
    else:
        if not card:
            summary["notes"].append("缺 --card")
            return summary
        if not scenario:
            summary["notes"].append("缺 --scenario")
            return summary
        if not os.path.isfile(scenario):
            summary["notes"].append("场景文件不存在：%s" % scenario)
            return summary
        if not os.path.isfile(rules):
            summary["notes"].append("规则表不存在：%s" % rules)
            return summary
        try:
            os.makedirs(out_dir, exist_ok=True)
            log_path = os.path.join(out_dir, "card-window.log")
            with open(log_path, "w", encoding="utf-8", newline="\n") as f:
                f.write("%s card_window: start card=%s profile=%s scenario=%s\n"
                        % (time.strftime("%H:%M:%S"), card, prof.get("name") or "custom",
                           scenario))
        except OSError as e:
            summary["notes"].append("建不了产物目录 %s：%s" % (out_dir, e))
            return summary
        runner = Runner(out_dir, log_path, dry_run=False)

    # dry-run：不起栈、不建目录、不校验文件是否存在，只打印完整命令序列
    if dry_run:
        for side in SIDES:
            ctx = side_ctx(side, out_dir, scenario, card)
            sc = prof["sides"][side]
            runner.run(side, "restore", expand_argv(sc["restore"], ctx))
            runner.log("[%s] 端口预检：%s" % (side, sc.get("ports") or []))
            runner.log("[%s] 就绪等待：%s（timeout %ss）"
                       % (side, sc.get("ready_ports") or [],
                          prof.get("ready_timeout_s", 120)))
            runner.run(side, "stack", expand_argv(sc["stack"], ctx))
            runner.run(side, "probe", build_probe_argv(sc, ctx, prof, password))
            runner.run(side, "db", build_db_argv(sc, ctx))
            runner.run(side, "stop", expand_argv(sc["stop"], ctx))
        compare_argv = [sys.executable, "-I", "-B", COMPARE,
                        "--oracle", os.path.join(out_dir, "csharp"),
                        "--rust", os.path.join(out_dir, "rust"),
                        "--out", os.path.join(out_dir, "report.json"),
                        "--rules", rules]
        compare_argv += [str(x) for x in prof.get("compare_extra") or []]
        runner.run("compare", "compare", compare_argv)
        summary["commands"] = runner.commands
        summary["verdict"] = "DRY-RUN"
        summary["exit_code"] = 0
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        return summary

    # 二进制 sha256 是产物的一部分：配置了就必须存在（否则窗口证据不完整）
    missing_bins = [b["path"] for s in SIDES for b in summary["binaries"][s]
                    if not b["exists"]]
    if missing_bins:
        summary["notes"].append("缺两侧二进制 => 不跑（证据不完整）：%s" % ", ".join(missing_bins))
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        return summary

    sides_ok = True
    for side in SIDES:
        ctx = side_ctx(side, out_dir, scenario, card)
        try:
            res = run_side(runner, side, prof, ctx, password)
        except (ConfigError, OSError) as e:
            res = {"side": side, "rc": 2, "error": "配置/执行错误：%s" % e}
            summary["notes"].append("%s: %s" % (side, e))
        summary["sides"][side] = res
        if res.get("rc") != 0:
            sides_ok = False
            summary["notes"].append("%s 侧未完成：%s"
                                    % (side, res.get("error") or "rc=%s" % res.get("rc")))
            break

    if sides_ok:
        try:
            cmp_rc = run_compare(runner, out_dir, prof, rules)
        except (ConfigError, OSError) as e:
            cmp_rc = 2
            summary["notes"].append("compare 启动失败：%s" % e)
        summary["compare_rc"] = cmp_rc
        rep = read_report(os.path.join(out_dir, "report.json"))
        if rep is None:
            summary["verdict"] = "UNDECIDABLE"
            summary["notes"].append("compare 未产出 report.json")
        else:
            summary["verdict"] = rep.get("verdict") or "UNDECIDABLE"
            fr = rep.get("frames") or {}
            summary["frames"] = {"oracle_total": fr.get("oracle_total"),
                                 "rust_total": fr.get("rust_total"),
                                 "compared": fr.get("compared")}
            db = rep.get("db") or {}
            summary["db_state"] = db.get("state")
            summary["diffs"] = len(rep.get("diffs") or [])
            summary["undecidable"] = len(rep.get("undecidable") or [])
            summary["whitelisted"] = len(rep.get("whitelisted") or [])
            if summary["verdict"] != "PASS":
                summary["notes"].append("compare verdict=%s（见 report.json）"
                                        % summary["verdict"])
    else:
        # 编排没跑完：写一份 fallback report.json（显式标注不是比较报告），绝不留空白
        fallback = {
            "tool": TOOL,
            "format": "card-window-report/1",
            "verdict": "UNDECIDABLE",
            "exit_code": 2,
            "compare": None,
            "reason": "orchestration_incomplete",
            "notes": summary["notes"],
        }
        path = os.path.join(out_dir, "report.json")
        try:
            if not os.path.exists(path):
                with open(path, "w", encoding="utf-8", newline="\n") as f:
                    json.dump(fallback, f, ensure_ascii=False, indent=2, sort_keys=True)
                    f.write("\n")
        except OSError:
            pass
        summary["verdict"] = "UNDECIDABLE"

    summary["commands"] = runner.commands
    summary["finished_at"] = iso_now()
    summary["duration_s"] = _fmt_duration(time.monotonic() - t0)

    # 归档：report.json + 两侧产物 sha256
    artifacts = {}
    for rel in ("report.json", "csharp/frames.ndjson", "csharp/run.json",
                "csharp/db/manifest.json", "rust/frames.ndjson", "rust/run.json",
                "rust/db/manifest.json"):
        p = os.path.join(out_dir, rel.replace("/", os.sep))
        if os.path.exists(p):
            artifacts[rel] = {"path": p, "sha256": sha256_file(p)}
        else:
            artifacts[rel] = {"path": p, "sha256": None}
    summary["artifacts"] = artifacts

    ok = (summary["verdict"] == "PASS" and summary.get("compare_rc") == 0
          and all((summary["sides"].get(s) or {}).get("rc") == 0 for s in SIDES))
    summary["exit_code"] = 0 if ok else 2

    readme_path = os.path.join(out_dir, "README.txt")
    try:
        write_readme(readme_path, summary)
        summary["artifacts"]["README.txt"] = {"path": readme_path,
                                              "sha256": sha256_file(readme_path)}
    except OSError as e:
        summary["notes"].append("写 README.txt 失败：%s" % e)
        summary["exit_code"] = 2

    # --post-comment：只发 comment，绝不改卡状态
    if cfg.get("post_comment") and card:
        body = {
            "tool": TOOL,
            "card": card,
            "profile": summary["profile"],
            "verdict": summary["verdict"],
            "compare_rc": summary["compare_rc"],
            "frames": summary["frames"],
            "db_state": summary["db_state"],
            "diffs": summary["diffs"],
            "undecidable": summary["undecidable"],
            "scenario": summary["scenario"],
            "scenario_sha256": summary["scenario_sha256"],
            "binaries_sha256": {
                side: [{"path": b["path"], "sha256": b["sha256"]}
                       for b in summary["binaries"][side]]
                for side in SIDES
            },
            "evidence": {
                "out": summary["out"],
                "report": summary["artifacts"].get("report.json", {}).get("path"),
                "readme": summary["artifacts"].get("README.txt", {}).get("path"),
                "csharp_frames": summary["artifacts"].get("csharp/frames.ndjson", {}).get("path"),
                "rust_frames": summary["artifacts"].get("rust/frames.ndjson", {}).get("path"),
            },
            "started_at": summary["started_at"],
            "finished_at": summary["finished_at"],
            "tool_command_line": summary["tool_command_line"],
            "note": "本条目是 comment，只登记窗口证据；卡片状态未改动。",
        }
        ok_comment, detail = post_comment(card, body, cfg.get("repo") or REPO_ROOT)
        summary["comment"] = {"posted": ok_comment, "detail": detail}
        if ok_comment:
            summary["notes"].append("已在卡片发 comment（不改卡状态）：%s" % detail)
        else:
            summary["notes"].append("--post-comment 失败：%s" % detail)
            summary["exit_code"] = 2

    # 摘要最后写（包含 comment 结果），失败不影响退出码语义
    try:
        summary_path = os.path.join(out_dir, "card-window.json")
        with open(summary_path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(summary, f, ensure_ascii=False, indent=2, sort_keys=True)
            f.write("\n")
        summary["artifacts"]["card-window.json"] = {"path": summary_path,
                                                    "sha256": sha256_file(summary_path)}
    except OSError:
        pass
    return summary


# ---------------------------------------------------------------------------
# selftest（离线：桩命令 + 合成 frames，不连网/不连库/不起真栈）
# ---------------------------------------------------------------------------
STUB_SOURCE = r'''#!/usr/bin/env python3
import hashlib
import json
import os
import sys

def main():
    mode, side, log = sys.argv[1], sys.argv[2], sys.argv[3]
    rest = sys.argv[4:]
    out = None
    mutate = False
    for i, a in enumerate(rest):
        if a == "--out" and i + 1 < len(rest):
            out = rest[i + 1]
        if a == "--mutate":
            mutate = True
    with open(log, "a", encoding="utf-8") as f:
        f.write("%s %s\n" % (mode, side))
        f.flush()
    if mode == "restore":
        return 0
    if mode == "stack":
        if os.environ.get("STUB_STACK_FAIL") == side:
            return 3
        return 0
    if mode == "stop":
        return 0
    if mode == "probe":
        if out is None:
            sys.stderr.write("stub probe: missing --out\n")
            return 2
        os.makedirs(out, exist_ok=True)
        body = b"no" if (mutate and side == "rust") else b"ok"
        rec = {
            "seq": 1, "t_ms": 1, "phase": "notice", "conn": side,
            "ident": 658, "recog": 0, "param": 0, "tag": 0, "series": 0,
            "body_len": len(body), "body_hex": body.hex(),
            "body_sha256": hashlib.sha256(body).hexdigest(),
        }
        with open(os.path.join(out, "frames.ndjson"), "w", encoding="utf-8", newline="\n") as f:
            f.write(json.dumps(rec, sort_keys=True) + "\n")
        with open(os.path.join(out, "run.json"), "w", encoding="utf-8", newline="\n") as f:
            json.dump({"argv": sys.argv, "started_at": "stub", "ended_at": "stub",
                       "steps": []}, f, sort_keys=True)
        return 0
    if mode == "db":
        d = rest[0]
        os.makedirs(d, exist_ok=True)
        cols = ["Id", "PlayerId", "Position", "MakeIndex", "StdIndex", "Dura", "DuraMax"]
        payload = ("1\t1\t0\t0\t0\t0\t0\n").encode("utf-8")
        fname = "mir2_db__characters_bagitem.tsv"
        with open(os.path.join(d, fname), "wb") as f:
            f.write(payload)
        man = {"format": "dbsnap/1", "source": "card-window-selftest",
               "tables": [{"table": "mir2_db.characters_bagitem", "file": fname,
                           "rows": 1, "sha256": hashlib.sha256(payload).hexdigest(),
                           "columns": [{"name": c} for c in cols]}]}
        with open(os.path.join(d, "manifest.json"), "w", encoding="utf-8", newline="\n") as f:
            json.dump(man, f, ensure_ascii=False, indent=2)
        return 0
    return 2

if __name__ == "__main__":
    sys.exit(main())
'''


def _selftest_profile(tmp, stub, log, mutate_rust=False, stack_fail=None,
                      busy_port=None, ready_ports=None):
    py = sys.executable

    def side(name):
        prof_side = {
            "label": name,
            "stack": [py, stub, "stack", name, log],
            "stop": [py, stub, "stop", name, log],
            "restore": [py, stub, "restore", name, log],
            "probe": [py, stub, "probe", name, log],
            "db": {"command": [py, stub, "db", name, log, "{db_dir}"]},
            "ports": [],
            "ready_ports": [],
            "probe_extra": [],
            "binaries": [stub],
        }
        if name == "csharp" and (busy_port or ready_ports):
            prof_side["ports"] = [busy_port] if busy_port else []
            prof_side["ready_ports"] = list(ready_ports or [])
        if name == "rust" and mutate_rust:
            prof_side["probe_extra"] = ["--mutate"]
        if name == "csharp" and stack_fail == "csharp":
            prof_side["env"] = {"STUB_STACK_FAIL": "csharp"}
        return prof_side

    return {
        "format": PROFILE_FORMAT,
        "name": "selftest",
        "description": "offline stub profile",
        "login_gate": "127.0.0.1:1",
        "account": "stub",
        "server_name": "stub",
        "char_name": "stub",
        "probe_timeout_ms": 1000,
        "ready_timeout_s": 0.8,
        "compare_extra": [],
        "sides": {"csharp": side("csharp"), "rust": side("rust")},
    }


def selftest():
    tmp = tempfile.mkdtemp(prefix="card-window-selftest-")
    stub = os.path.join(tmp, "stub.py")
    with open(stub, "w", encoding="utf-8", newline="\n") as f:
        f.write(STUB_SOURCE)
    log = os.path.join(tmp, "order.log")
    scenario = os.path.join(tmp, "scenario.json")
    with open(scenario, "w", encoding="utf-8") as f:
        f.write("{}\n")

    # 占用一个端口（本进程监听），用于 fail-closed 牙
    holder = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    holder.bind(("127.0.0.1", 0))
    holder.listen(1)
    busy_port = holder.getsockname()[1]
    # 一个没人监听的端口（用于就绪等待牙）
    probe_sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
    probe_sock.bind(("127.0.0.1", 0))
    free_port = probe_sock.getsockname()[1]
    probe_sock.close()

    pass_n = 0
    fail_n = 0

    def chk(cond, msg):
        nonlocal pass_n, fail_n
        if cond:
            pass_n += 1
            print("PASS %s" % msg)
        else:
            fail_n += 1
            print("FAIL %s" % msg)

    def fresh_log():
        try:
            os.remove(log)
        except OSError:
            pass

    def read_log():
        try:
            with open(log, "r", encoding="utf-8") as f:
                return f.read()
        except OSError:
            return ""

    def base_cfg(out, profile, **kw):
        cfg = {
            "card": "openmir2-card-window-tool-selftest",
            "scenario": scenario,
            "out": out,
            "profile": profile,
            "password": "stub",
            "post_comment": False,
            "dry_run": False,
            "allow_production_db": False,
            "rules": DEFAULT_RULES,
        }
        cfg.update(kw)
        return cfg

    try:
        # 1) 正常路径：编排顺序 + 产物布局 + PASS
        fresh_log()
        out = os.path.join(tmp, "run-ok")
        prof = _selftest_profile(tmp, stub, log)
        summary = execute(base_cfg(out, prof))
        order = [l.strip() for l in read_log().splitlines() if l.strip()]
        expected = ["restore csharp", "stack csharp", "probe csharp", "db csharp",
                    "stop csharp", "restore rust", "stack rust", "probe rust",
                    "db rust", "stop rust"]
        chk(summary["exit_code"] == 0, "全流程 => 退出码 0（实际 %s）" % summary["exit_code"])
        chk(summary["verdict"] == "PASS", "compare verdict=PASS（实际 %s）" % summary["verdict"])
        chk(order == expected,
            "编排顺序 = restore/stack/probe/db/stop x2（实际 %s）" % order)
        rep = read_report(os.path.join(out, "report.json")) or {}
        fr = rep.get("frames") or {}
        chk(fr.get("oracle_total") == 1 and fr.get("rust_total") == 1
            and fr.get("compared") == 1, "合成 frames 1/1 帧参与比较（实际 %s）" % fr)
        chk((rep.get("db") or {}).get("state") == "compared", "库后态两侧都拍到并比较")
        layout = ["csharp/frames.ndjson", "csharp/run.json", "csharp/db/manifest.json",
                  "csharp/db_snapshot.json", "rust/frames.ndjson", "rust/run.json",
                  "rust/db/manifest.json", "rust/db_snapshot.json",
                  "report.json", "README.txt", "card-window.json", "card-window.log"]
        missing = [r for r in layout
                   if not os.path.exists(os.path.join(out, r.replace("/", os.sep)))]
        chk(not missing, "产物布局齐全（缺 %s）" % missing)
        with open(os.path.join(out, "README.txt"), encoding="utf-8") as f:
            readme = f.read()
        chk("scenario_sha256:" in readme and "started_at:" in readme
            and "command line (full):" in readme and "sha256=" in readme,
            "README 含场景 sha256/时间戳/命令全文/二进制 sha256")
        chk(summary["scenario_sha256"] == sha256_file(scenario),
            "摘要里的 scenario sha256 正确")
        with open(os.path.join(out, "card-window.log"), encoding="utf-8") as f:
            run_log = f.read()
        chk("--password ***" in run_log and "--password stub" not in run_log,
            "日志里的 --password 已抹掉")
        chk("--password ***" in readme and "--password stub" not in readme,
            "README 里的 --password 已抹掉")

        # 2) 牙 A：Rust 侧差一个字节 => compare FAIL，工具 rc=2（失败不得判绿）
        fresh_log()
        out_bad = os.path.join(tmp, "run-diff")
        prof_bad = _selftest_profile(tmp, stub, log, mutate_rust=True)
        s_bad = execute(base_cfg(out_bad, prof_bad))
        rep_bad = read_report(os.path.join(out_bad, "report.json")) or {}
        chk(s_bad["exit_code"] == 2,
            "b 侧差一字节 => 工具 rc=2（实际 %s）" % s_bad["exit_code"])
        chk(rep_bad.get("verdict") == "FAIL" and rep_bad.get("exit_code") == 1,
            "b 侧差一字节 => report verdict=FAIL/exit_code=1（实际 %s/%s）"
            % (rep_bad.get("verdict"), rep_bad.get("exit_code")))

        # 3) 牙 B：起栈前端口被占 => rc=2、点名 pid、且不启动栈
        fresh_log()
        out_busy = os.path.join(tmp, "run-busy")
        prof_busy = _selftest_profile(tmp, stub, log, busy_port=busy_port)
        s_busy = execute(base_cfg(out_busy, prof_busy))
        busy_log = read_log()
        err = (s_busy["sides"].get("csharp") or {}).get("error") or ""
        chk(s_busy["exit_code"] == 2, "端口被占 => rc=2（实际 %s）" % s_busy["exit_code"])
        chk(str(busy_port) in err and "pid" in err,
            "端口被占 => 错误里点名端口与 pid（%r）" % err)
        chk("stack csharp" not in busy_log, "端口被占 => 起栈命令未被调用（起栈前预检）")
        chk("stop csharp" not in busy_log, "端口被占 => 没起栈也就不停栈")

        # 4) fail-closed：起栈失败 => rc=2、不跑场景、仍尝试停栈、不起第二侧
        fresh_log()
        out_fail = os.path.join(tmp, "run-stackfail")
        prof_fail = _selftest_profile(tmp, stub, log, stack_fail="csharp")
        s_fail = execute(base_cfg(out_fail, prof_fail))
        fail_log = read_log()
        chk(s_fail["exit_code"] == 2, "起栈失败 => rc=2（实际 %s）" % s_fail["exit_code"])
        chk("probe csharp" not in fail_log, "起栈失败 => 不跑场景")
        chk("stop csharp" in fail_log, "起栈失败 => 仍然尝试停栈")
        chk("stack rust" not in fail_log, "起栈失败 => 不再起第二侧（端口冲突）")

        # 5) 栈返回 0 但端口不监听 => rc=2（栈没起来不能算通过），且仍然停栈
        fresh_log()
        out_ready = os.path.join(tmp, "run-notready")
        prof_ready = _selftest_profile(tmp, stub, log, ready_ports=[free_port])
        s_ready = execute(base_cfg(out_ready, prof_ready))
        ready_log = read_log()
        err_ready = (s_ready["sides"].get("csharp") or {}).get("error") or ""
        chk(s_ready["exit_code"] == 2,
            "栈没真起来（端口不监听）=> rc=2（实际 %s）" % s_ready["exit_code"])
        chk("未就绪" in err_ready, "栈没真起来 => 错误写明端口未监听")
        chk("probe csharp" not in ready_log and "stop csharp" in ready_log,
            "栈没真起来 => 不跑场景，但仍停栈")

        # 6) 缺场景 => rc=2
        s_missing = execute(base_cfg(os.path.join(tmp, "run-missing"),
                                     _selftest_profile(tmp, stub, log),
                                     scenario=os.path.join(tmp, "nope.json")))
        chk(s_missing["exit_code"] == 2, "场景文件不存在 => rc=2")

        # 7) dry-run：打印命令序列、不建目录、不执行
        fresh_log()
        out_dry = os.path.join(tmp, "run-dry")
        s_dry = execute(base_cfg(out_dry, _selftest_profile(tmp, stub, log), dry_run=True))
        steps = [c["step"] for c in s_dry["commands"]]
        chk(s_dry["exit_code"] == 0 and s_dry["verdict"] == "DRY-RUN",
            "dry-run => rc=0/DRY-RUN")
        chk(not os.path.exists(out_dry), "dry-run => 不创建产物目录")
        chk(not os.path.exists(log), "dry-run => 不执行任何桩命令")
        chk(steps == ["restore", "stack", "probe", "db", "stop"] * 2 + ["compare"],
            "dry-run => 命令序列 restore/stack/probe/db/stop x2 + compare（实际 %s）" % steps)

        # 8) 停栈命令碰 mysqld => 拒绝（不碰 MySQL 服务本身）
        prof_mysql = _selftest_profile(tmp, stub, log)
        prof_mysql["sides"]["csharp"]["stop"] = ["powershell", "-Command",
                                                 "Stop-Service mysqld"]
        refused = False
        try:
            execute(base_cfg(os.path.join(tmp, "run-mysql"), prof_mysql))
        except ConfigError as e:
            refused = "MySQL" in str(e)
        chk(refused, "停栈命令含 mysqld => ConfigError 拒绝（不碰 MySQL 服务）")

        # 9) 生产库 => 默认拒绝
        prof_prod = _selftest_profile(tmp, stub, log)
        prof_prod["sides"]["csharp"]["restore"] = ["bash", "-c", "mysql mir2_db < x"]
        refused_prod = False
        try:
            execute(base_cfg(os.path.join(tmp, "run-prod"), prof_prod))
        except ConfigError as e:
            refused_prod = "生产库" in str(e)
        chk(refused_prod, "命令指向生产库 mir2_db => 默认拒绝（内容冻结）")

        # 10) profile 结构错误 => ConfigError（缺 stop）
        prof_broken = _selftest_profile(tmp, stub, log)
        del prof_broken["sides"]["rust"]["stop"]
        broken_refused = False
        try:
            load_profile(prof_broken)
        except ConfigError:
            broken_refused = True
        chk(broken_refused, "profile 缺 stop 命令 => ConfigError")

        # 11) --post-comment：只发 comment（body 不含 status），成功不改 PASS；失败 => rc=2
        captured = {}
        orig_post = globals()["post_comment"]

        def fake_ok(card, body, repo):
            captured.update(body)
            return True, "stub-comment-oid"

        fresh_log()
        globals()["post_comment"] = fake_ok
        try:
            out_c = os.path.join(tmp, "run-comment")
            s_c = execute(base_cfg(out_c, _selftest_profile(tmp, stub, log),
                                   post_comment=True))
        finally:
            globals()["post_comment"] = orig_post
        chk(s_c["exit_code"] == 0 and captured.get("verdict") == "PASS",
            "--post-comment 成功 => rc=0 且 comment 带 verdict=PASS")
        chk("status" not in captured
            and captured.get("card") == "openmir2-card-window-tool-selftest",
            "comment body 不含 status（绝不改卡状态），card 正确")
        chk((captured.get("frames") or {}).get("compared") == 1
            and captured.get("scenario_sha256") == sha256_file(scenario)
            and (captured.get("evidence") or {}).get("report"),
            "comment 带帧数 / 场景 sha256 / 证据路径")

        def fake_fail(card, body, repo):
            return False, "boom"

        fresh_log()
        globals()["post_comment"] = fake_fail
        try:
            out_cf = os.path.join(tmp, "run-comment-fail")
            s_cf = execute(base_cfg(out_cf, _selftest_profile(tmp, stub, log),
                                    post_comment=True))
        finally:
            globals()["post_comment"] = orig_post
        chk(s_cf["exit_code"] == 2
            and (s_cf.get("comment") or {}).get("posted") is False,
            "--post-comment 失败 => rc=2（不静默丢回执）")

    finally:
        holder.close()
        import shutil
        shutil.rmtree(tmp, ignore_errors=True)

    total = pass_n + fail_n
    if fail_n:
        print("RED: card_window.py selftest %d/%d" % (pass_n, total))
        return 1
    print("GREEN: card_window.py selftest %d/%d" % (pass_n, total))
    return 0


# ---------------------------------------------------------------------------
# CLI
# ---------------------------------------------------------------------------
def build_parser():
    ap = argparse.ArgumentParser(
        prog="card_window.py",
        description="窗口批处理：restore -> 起 C# 栈 -> probe -> 停栈 -> 起 Rust -> probe -> "
                    "停栈 -> compare -> 归档（语义同 run_dual.sh，run_dual.sh 不改）。")
    ap.add_argument("--card", help="卡片 id（--post-comment 时必填；写入 README/摘要）")
    ap.add_argument("--scenario", help="场景 JSON（tools/probe/scenarios/*.json）")
    ap.add_argument("--out", help="产物目录")
    ap.add_argument("--profile", default="alt",
                    help="profile：alt（默认）| csharp-default | <profile.json 路径>")
    ap.add_argument("--post-comment", action="store_true",
                    help="跑完在该卡发一条签名 comment（含 VERDICT/帧数/sha256/证据路径；不改卡状态）")
    ap.add_argument("--dry-run", action="store_true",
                    help="只打印完整命令序列，不起栈/不建目录/不需要密码")
    ap.add_argument("--selftest", action="store_true", help="离线自检（桩命令 + 合成 frames）")
    ap.add_argument("--allow-production-db", action="store_true",
                    help="显式放行生产库配置（仅 owner 授权的窗口；默认拒绝）")
    ap.add_argument("--rules", default=DEFAULT_RULES, help="compare_runs.py 规则表")
    ap.add_argument("--login-gate", help="覆盖 profile 的 LoginGate HOST:PORT")
    ap.add_argument("--account", help="覆盖 profile 的账号（也可用 CONFORMANCE_ACCOUNT）")
    ap.add_argument("--server-name", help="覆盖 profile 的服务器名")
    ap.add_argument("--char-name", help="覆盖 profile 的角色名")
    ap.add_argument("--timeout-ms", type=int, help="覆盖 profile 的 probe --timeout-ms")
    return ap


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.selftest:
        return selftest()
    cfg = {
        "card": args.card,
        "scenario": args.scenario,
        "out": args.out,
        "profile": args.profile,
        "post_comment": args.post_comment,
        "dry_run": args.dry_run,
        "allow_production_db": args.allow_production_db,
        "rules": args.rules,
        "login_gate": args.login_gate,
        "account": args.account,
        "server_name": args.server_name,
        "char_name": args.char_name,
        "timeout_ms": args.timeout_ms,
    }
    try:
        summary = execute(cfg)
    except ConfigError as e:
        print("card_window.py: %s" % e, file=sys.stderr)
        return 2
    except KeyboardInterrupt:
        print("card_window.py: 中断（停栈已由 finally 尝试执行）", file=sys.stderr)
        return 2
    except Exception:
        traceback.print_exc()
        return 2
    if summary.get("verdict") == "DRY-RUN":
        for cmd in summary.get("commands") or []:
            print("[%s] %s: %s" % (cmd["side"], cmd["step"], _join_argv(cmd["argv"])))
        return 0
    if summary.get("exit_code") != 0:
        for note in summary.get("notes") or []:
            print("card_window.py: %s" % note, file=sys.stderr)
    print("VERDICT: %s (compare_rc=%s, exit=%s)  out=%s"
          % (summary.get("verdict"), summary.get("compare_rc"),
             summary.get("exit_code"), summary.get("out")))
    return int(summary.get("exit_code") or 2)


if __name__ == "__main__":
    sys.exit(main())
