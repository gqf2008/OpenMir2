#!/usr/bin/env python3
# -*- coding: utf-8 -*-
"""card_window.py — 收一张 R 卡的窗口项：一条命令跑完双栈对拍并归档证据。

把 2026-10-23 手工做过 3 次的流程固化成一条命令（语义与 tools/conformance/run_dual.sh 一致，
**run_dual.sh 本身不改**）：

    restore baseline -> 起 C# 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
                     -> restore baseline -> 起 Rust 栈 -> 等就绪 -> probe -> 抓库 -> 停栈
                     -> compare -> 归档（report.json / README.txt / card-window.json）

只调用、不修改既有工具（参数发现借用 tools/probe/param_discovery.py 的推导表，只 import）：
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
  * probe 跑完检查 frames.ndjson 非空；run.json 必须非空且 `exit.code == probe rc == 0`；0 帧 => rc=2。
  * 库快照失败 / 缺 manifest => rc=2（库判据不可缺）。
  * 停栈在 finally 里执行：无论恢复/起栈/probe/抓库哪一步失败都会尝试停栈；**起过栈而
    停栈没跑成（stop_rc=None）或 rc!=0 一律判失败**，绝不当成功。
  * `stack` / `stop` / `restore` / `db.command` / `probe` 必须是**非空字符串数组**（加载期校验；
    字符串 / [] / null 直接 ConfigError），避免停栈 argv 非法被静默当成「已停」。
  * `binaries` 是**必需**项（两侧各至少一条非空路径，加载期校验）：缺省/空列表 => ConfigError，
    杜绝「PASS 产物里没有任何二进制 sha256」。二进制文件缺失时提前收尾，但**产物集仍然完整**
    （report.json + README.txt + card-window.json + card-window.log），不写半套证据。
  * **先定稿 verdict/exit_code，再写产物**：README.txt / card-window.json 写失败 =>
    exit=2（verdict 仍如实记录，但绝不因为写产物失败而报绿）。
  * **`--post-comment` 只记「证据包完整」的 PASS**：comment 在全部产物写完之后才发。
    顶层只有一个三态 `verdict`：compare FAIL => `FAIL`；无结论或证据缺件 =>
    `UNDECIDABLE`；完整且 PASS 才 `PASS`。同时带 `verdict_reason` / `blockers` /
    `exit_code` / `finalize_ok`，compare 的事实收进嵌套 `compare`（不设同级
    `report_verdict`——N-1/D-1：曾出现「exit=2 但 comment 报 PASS」的假账）。
  * **不碰 MySQL 服务本身**：停栈命令先做归一化（去 `.exe`、折叠大小写/空白）再按效果判定 ——
    任何 mysql/mysqld 提及、或服务控制族（Stop-Service/spsv/net stop/sc stop）直接拒绝。
  * 默认拒绝生产库：把 db 规格（preset / tables / allow_db / databases / command 的
    --db/--table、以及 env 里的 `database=`）**展开成最终库名集合**再判定
    （`prod` 预设 / mir2_db / mir2_account / mir2_data / Mir2 均算生产库；无法判定为测试库的
    未知库名是**永久拒绝**：`db.databases` 再声明一次也不放行，`--allow-production-db` 与
    `--dry-run` 同样不放行，请换用 `preset=test` / `mir2_test_*` 命名）。
    只有显式 --allow-production-db 才放行生产库本身。
  * 真跑 **--out 必填**（不允许把证据静默写进 cwd）；out 里若已有上次运行的产物，开跑前整体
    归档到 `_stale-<ts>-<pid>/`，并**无条件**写本次的 report.json（compare 报告或 fallback），
    杜绝旧 PASS 冒充本次证据。
  * **参数发现 fail-closed**（场景声明 `required` 时自动启用，`--discover` 可强制）：每侧
    先跑一次 discovery（默认 `npc_click_auto`：点一次 NPC 收 `SM_MERCHANTSAY(643)`），从
    **本侧**帧推导 required，再用本侧的值拼 `--var` 跑真正的场景。任一侧推不出 ⇒ 该侧
    rc=2 并**点名缺哪个变量**；两侧 `kind` 不一致（`derived` vs `rule`）或非会话变量
    （坐标等）取值不同 ⇒ rc=2。**绝不拿另一侧的值、绝不用默认值/空值填空**；每侧结果落
    `<out>/<side>/discovered.json`，两侧汇总落 `<out>/discovered.json`。

profile：
  * 内置 `alt`（默认）= 2026-10-23 双栈窗口所用的备用端口剖面
    （E:/tmp/conformance 私有根 + mir2_test_* / mir2_test_*_r 测试库）。
  * 内置 `csharp-default` = C# 默认端口栈（E:/MirServer，**生产库**）。默认拒绝运行，
    必须显式 --allow-production-db，且只有 owner 授权的窗口才允许（见 docs 说明）。
  * `--profile <路径.json>` 也可直接加载自定义 profile（format = card-window-profile/1）。
    自定义 db 快照命令（`db.command`）必须同时用 `db.databases` 声明它读的库名
    （静态安全判定；无法判定为测试库的库名会被**永久拒绝**，声明了也不放行）。

用法：
  CONFORMANCE_PASSWORD=... python -I -B tools/conformance/card_window.py \
      --card openmir2-window-529-dualrun \
      --scenario tools/probe/scenarios/login_only.json \
      --out E:/tmp/conformance/runs/529-card-window \
      --profile alt

  # 只打印将要执行的完整命令序列（不起栈、不建目录、不需要密码）
  python -I -B tools/conformance/card_window.py --card X --scenario S --out O --dry-run

  # 带 required 的场景自动先跑 discovery；离线预览带**真实值**的 --var 命令序列
  # （用已录帧回放推导，不起栈；推不出 => rc=2 并点名缺哪个变量）
  python -I -B tools/conformance/card_window.py --card X \
      --scenario tools/probe/scenarios/merchant_dialog.json --out O --dry-run \
      --discovery-frames 'E:/tmp/conformance/runs/w24-{side}-npc_auto/frames.ndjson'

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
import uuid

TOOL = "tools/conformance/card_window.py"
HERE = os.path.dirname(os.path.abspath(__file__))
REPO_ROOT = os.path.dirname(os.path.dirname(HERE))
PROBE = os.path.join(REPO_ROOT, "tools", "probe", "mir2_probe.py")
COMPARE = os.path.join(HERE, "compare_runs.py")
DBSNAP = os.path.join(REPO_ROOT, "tools", "dbsnap", "dbsnap.py")
DEFAULT_RULES = os.path.join(HERE, "rules.json")

# 参数发现（tools/probe/param_discovery.py，卡 openmir2-window-param-discovery）。
# `-I` 不会把脚本目录放进 sys.path，这里显式加；只借用它的推导表/渲染函数，不改它。
PROBE_DIR = os.path.dirname(PROBE)
if PROBE_DIR not in sys.path:
    sys.path.insert(0, PROBE_DIR)
import param_discovery  # noqa: E402

FORMAT = "card-window/1"
PROFILE_FORMAT = "card-window-profile/1"
DISCOVERY_FORMAT = "param-discovery/1"
DISCOVERED_FORMAT = "discovered/1"
#: 默认 discovery 场景：进世界后点「最近一条非自己的 SM_TURN」→ 收 SM_MERCHANTSAY(643)。
#: 643 是 NPC **应答点击**才发的面板，登录期收帧里没有 ⇒ 必须点一次才能推导 merchant_actor。
DEFAULT_DISCOVERY_SCENARIO = os.path.join(
    REPO_ROOT, "tools", "probe", "scenarios", "npc_click_auto.json")
#: 会话作用域变量：ActorId 每会话不同，两侧**本来就该不同**（compare_runs 默认掩码 recog）。
#: 其余变量两侧必须一致，否则说明两侧根本不在同一起点。
SESSION_SCOPED_VARS = ("king_actor", "merchant_actor", "npc_actor")
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
def validate_cmd_argv(value, where):
    """命令必须是**非空字符串数组**。加载期就拒，避免停栈时才发现 argv 非法。"""
    if not isinstance(value, list):
        raise ConfigError("%s 必须是 argv 字符串数组（实际 %s）；不接受字符串/null"
                          % (where, type(value).__name__))
    if not value:
        raise ConfigError("%s 不能是空数组（会导致该步静默不执行）" % where)
    for a in value:
        if not isinstance(a, str) or not a:
            raise ConfigError("%s 的元素必须是非空字符串（实际 %r）" % (where, a))
    return value


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
            validate_cmd_argv(sc[key], "profile.sides.%s.%s" % (side, key))
        if sc.get("probe") is not None:
            validate_cmd_argv(sc["probe"], "profile.sides.%s.probe" % side)
        extra = sc.get("probe_extra")
        if extra is not None:
            if not isinstance(extra, list) or any(not isinstance(x, str) for x in extra):
                raise ConfigError("profile.sides.%s.probe_extra 必须是字符串数组" % side)
        db = sc.get("db")
        if not isinstance(db, dict):
            raise ConfigError("profile.sides.%s 缺 db 配置（preset / tables / command）" % side)
        if db.get("command") is not None:
            validate_cmd_argv(db["command"], "profile.sides.%s.db.command" % side)
        if db.get("preset") is not None:
            if not isinstance(db["preset"], str) or not db["preset"]:
                raise ConfigError("profile.sides.%s.db.preset 必须是非空字符串" % side)
        for key in ("tables", "allow_db", "databases"):
            v = db.get(key)
            if v is not None:
                if not isinstance(v, list) or any(not isinstance(x, str) or not x for x in v):
                    raise ConfigError("profile.sides.%s.db.%s 必须是非空字符串数组"
                                      % (side, key))
        if db.get("command") is None and not (db.get("preset") or db.get("tables")):
            raise ConfigError("profile.sides.%s.db 需要 preset / tables（或 command + databases）"
                              % side)
        if db.get("command") is not None and not (
                db.get("databases") or db.get("preset") or db.get("tables")):
            raise ConfigError("profile.sides.%s.db.command 必须同时声明 databases"
                              "（静态安全判定用；否则无法确认它读的是测试库。"
                              "所声明的库名还必须是 preset=test / mir2_test_* 一类的测试库）"
                              % side)
        for key in ("ports", "ready_ports"):
            v = sc.get(key, [])
            if not isinstance(v, list):
                raise ConfigError("profile.sides.%s.%s 必须是数组" % (side, key))
            for p in v:
                if not isinstance(p, int) or isinstance(p, bool) or not (0 < p < 65536):
                    raise ConfigError("profile.sides.%s.%s 含非法端口 %r" % (side, key, p))
        bins = sc.get("binaries")
        if not isinstance(bins, list) or not bins or any(
                not isinstance(x, str) or not x for x in bins):
            raise ConfigError(
                "profile.sides.%s.binaries 必须是**非空**字符串数组：窗口证据要求两侧二进制 sha256，"
                "空列表/缺省会让 PASS 报告里没有任何二进制指纹" % side)
        env = sc.get("env")
        if env is not None:
            if not isinstance(env, dict) or any(
                    not isinstance(k, str) or not isinstance(v, str) for k, v in env.items()):
                raise ConfigError("profile.sides.%s.env 必须是字符串到字符串的对象" % side)
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


PROD_DB_NAMES = frozenset({"mir2_db", "mir2_account", "mir2_data", "mir2"})
DB_IN_CONN_RE = re.compile(r"database\s*=\s*([A-Za-z0-9_]+)", re.I)
SERVICE_CONTROL_RE = re.compile(
    r"\b(stop-service|spsv|restart-service|rssv|suspend-service|resume-service|"
    r"set-service|new-service)\b"
    r"|\bnet1?\s+stop\b|\bsc\s+stop\b", re.I)


def classify_db_name(name):
    """结构性判定库名类别：'prod' / 'test' / 'unknown'（不看原始命令文本）。"""
    n = str(name or "").strip().strip("`'\"")
    if not n:
        return "unknown"
    low = n.casefold()
    if low in ("prod", "production"):
        return "prod"
    if low == "test":
        return "test"
    if low in PROD_DB_NAMES:
        return "prod"
    if low.startswith("mir2_test_") or "_test" in low:
        return "test"
    if low.endswith(("_r", "_oracle", "_rust")):
        return "test"
    return "unknown"


def _norm_stop_text(argv):
    txt = " ".join(str(x) for x in argv).lower()
    txt = re.sub(r"\.exe\b", "", txt)
    return re.sub(r"\s+", " ", txt).strip()


def _collect_db_specs(prof):
    """把 profile 里所有可能决定「读哪个库」的值展开成 (库名, 来源) 列表。

    覆盖：db.preset / db.tables / db.allow_db / db.databases / db.command 的
    --db/--database/--table/--allow-db，以及 sides.*.env 里 `database=<name>` 的连接串。
    """
    specs = []
    for side in SIDES:
        sc = prof["sides"][side]
        db = sc.get("db") or {}
        if db.get("preset") is not None:
            specs.append((str(db["preset"]), "sides.%s.db.preset" % side))
        for key in ("tables", "allow_db", "databases"):
            for v in db.get(key) or []:
                name = str(v)
                if key == "tables" and "." in name:
                    name = name.split(".", 1)[0]
                specs.append((name, "sides.%s.db.%s" % (side, key)))
        cmd = db.get("command")
        if isinstance(cmd, list):
            for i, a in enumerate(cmd):
                if a in ("--db", "--database", "--allow-db") and i + 1 < len(cmd):
                    specs.append((str(cmd[i + 1]), "sides.%s.db.command %s" % (side, a)))
                elif a.startswith("--db=") or a.startswith("--database="):
                    specs.append((a.split("=", 1)[1], "sides.%s.db.command %s" % (side, a)))
                elif a == "--table" and i + 1 < len(cmd):
                    specs.append((str(cmd[i + 1]).split(".", 1)[0],
                                  "sides.%s.db.command --table" % side))
                elif a.startswith("--table="):
                    specs.append((a.split("=", 1)[1].split(".", 1)[0],
                                  "sides.%s.db.command --table" % side))
        for k, v in (sc.get("env") or {}).items():
            for m in DB_IN_CONN_RE.finditer(str(v)):
                specs.append((m.group(1), "sides.%s.env.%s" % (side, k)))
    return specs


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
        for key in ("tables", "allow_db", "databases"):
            for t in db.get(key) or []:
                texts.append(str(t))
        texts.append(str(db.get("preset") or ""))
        for k, v in (sc.get("env") or {}).items():
            texts.append("%s=%s" % (k, v))
    texts.append(" ".join(str(x) for x in prof.get("compare_extra") or []))
    return "\n".join(texts)


def check_profile_safety(prof, allow_production_db, dry_run=False):
    """(a) 停栈不许碰 MySQL； (b) 生产库默认拒绝（结构性判定）。失败 => ConfigError。"""
    warnings = []
    # I-2：按「效果」判，而不是字面子串。归一化（去 .exe / 折叠空白/大小写）后，
    # 任何 mysql/mysqld 提及、或服务控制族（Stop-Service/spsv/net stop/sc stop）一律拒。
    for side in SIDES:
        txt = _norm_stop_text(prof["sides"][side]["stop"])
        if "mysql" in txt:
            raise ConfigError(
                "profile.sides.%s.stop 提到 MySQL/mysqld：停栈命令不允许操作 MySQL 服务本身"
                "（只停游戏栈；MySQL 由 owner 保持运行）" % side)
        if SERVICE_CONTROL_RE.search(txt):
            raise ConfigError(
                "profile.sides.%s.stop 使用服务控制（Stop-Service/spsv/net stop/sc stop 等）："
                "可能误停 MySQL；停栈只允许按进程名/pid 停游戏栈" % side)

    # I-3：先把 db 规格「展开」成最终库名集合再判定，不看原始文本是否含字面量。
    prod_hits, unknown = [], []
    for name, src in _collect_db_specs(prof):
        kind = classify_db_name(name)
        if kind == "prod":
            prod_hits.append("%s(%s)" % (name, src))
        elif kind == "unknown":
            unknown.append("%s(%s)" % (name, src))
    if prod_hits:
        warnings.append("db 规格展开后命中生产库：%s" % ", ".join(prod_hits))
    if unknown:
        raise ConfigError(
            "profile 的库规格无法静态判定为测试库（保守拒绝）：%s；"
            "这些库名是**永久拒绝**——在 db.databases 里再声明一次同样不放行，"
            "--allow-production-db 与 --dry-run 也不放行。"
            "请换用 preset=test 或 mir2_test_* 命名的测试库" % ", ".join(unknown))
    text = _any_argv_text(prof)
    hits = sorted(set(m.group(0) for m in PROD_DB_RE.finditer(text))
                  | set(m.group(0) for m in PROD_LOGIN_RE.finditer(text)))
    prod_all = prod_hits + hits
    if (prod_all or prof.get("production_db")) and not allow_production_db:
        detail = ("命中 %s" % ", ".join(prod_all)) if prod_all else "profile 标记 production_db=true"
        if dry_run:
            warnings.append("dry-run 未实际访问生产库，但 profile 指向生产库（%s）；"
                            "真跑会被拒绝，除非 --allow-production-db" % detail)
        else:
            raise ConfigError(
                "拒绝在窗口里碰生产库（%s）：内容冻结，窗口只跑测试库。"
                "确需 owner 授权的生产窗口时才可加 --allow-production-db。" % detail)
    if prod_all and allow_production_db:
        warnings.append("已显式放行生产库：%s" % ", ".join(prod_all))
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


def side_ctx(side, out_dir, scenario, card, discovery=None):
    """一侧的路径/场景上下文；`discovery` 描述发现阶段（needed/scenario/required/frames）。"""
    side_dir = os.path.join(out_dir, side)
    d = discovery or {}
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
        "discovery": {
            "needed": bool(d.get("needed")),
            "forced": bool(d.get("forced")),
            "scenario": d.get("scenario"),
            "required": list(d.get("required") or []),
            "frames": d.get("frames"),
        },
    }


def discovery_ctx(ctx):
    """discovery run 的 ctx：产物落 `<side>/discovery/`，场景换成「点一次 NPC」的那条。

    `scenario` 在 ctx 里同时被 `{scenario}` 占位符与 `--scenario` 使用，所以这里把它换成
    discovery 场景，并用 `real_scenario` 记住真正要跑的场景（落进 discovered.json）。
    """
    disc_dir = os.path.join(ctx["side_dir"], "discovery")
    d = dict(ctx)
    d["side_dir"] = disc_dir
    d["db_dir"] = os.path.join(disc_dir, "db")
    d["real_scenario"] = ctx["scenario"]
    d["scenario"] = ctx["discovery"]["scenario"]
    d["discovery_dir"] = disc_dir
    return d


def build_probe_argv(side_cfg, ctx, prof, password, vars_argv=None):
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
    # --var 只来自本侧 discovery 的推导结果（render_vars 已按变量名排序，确定性）
    argv += list(vars_argv or [])
    return argv


def scenario_required(path):
    """读场景声明的 `required`（发现阶段要推哪些变量）。坏文件 => ConfigError（rc=2）。"""
    try:
        with open(path, "r", encoding="utf-8") as fh:
            doc = json.load(fh)
    except (OSError, ValueError) as ex:
        raise ConfigError("%s: cannot load scenario: %s" % (path, ex))
    if not isinstance(doc, dict):
        raise ConfigError("%s: scenario must be a JSON object" % path)
    required = doc.get("required")
    if required is None:
        return []
    if not isinstance(required, list) or any(
            not isinstance(x, str) or not x for x in required):
        raise ConfigError("%s: scenario.required must be a list of non-empty strings, got %r"
                          % (path, required))
    return list(required)


def missing_reason(var):
    """缺变量原因：直接复用 param_discovery 自己登记的说明（不在本工具另造一套口径）。"""
    return (param_discovery.REASONS.get(var)
            or param_discovery.UNDERIVABLE.get(var)
            or "规则表无此变量（不猜）")


def derive_from_frames(frames_path, required):
    """从本侧 discovery 帧推导 required。坏输入 => param_discovery.DiscoveryError（rc=2）。"""
    frames = param_discovery.load_frames(frames_path)
    return param_discovery.derive(frames, required)


def write_json_file(path, doc):
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(doc, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")
    return path


def side_discovery_doc(side, ctx, values, missing, probe_rc, probe_argv):
    """单侧 discovered.json：值 + 推导来源 + 哪些推导失败（kind/source 必须原样落盘）。"""
    frames_path = os.path.join(ctx["discovery_dir"], "frames.ndjson")
    real_scenario = ctx.get("real_scenario") or ctx["scenario"]
    disc_scenario = ctx["discovery"]["scenario"]
    return {
        "format": DISCOVERY_FORMAT,
        "tool": TOOL,
        "side": side,
        # F-1：ok 必须同时包含 discovery probe 的 rc——`--discover` 强制 + 场景无 required 时
        # missing 为空，只看 missing 会把「probe 失败、没有帧」写成 ok=True。
        "ok": (probe_rc == 0 and not missing),
        "scenario": real_scenario,
        "scenario_sha256": (sha256_file(real_scenario)
                            if real_scenario and os.path.isfile(real_scenario) else None),
        "discovery_scenario": disc_scenario,
        "discovery_scenario_sha256": (sha256_file(disc_scenario)
                                      if disc_scenario and os.path.isfile(disc_scenario)
                                      else None),
        "required": list(ctx["discovery"]["required"]),
        "values": values,
        "argv": param_discovery.render_vars(values),
        "missing": list(missing),
        "missing_reasons": dict((v, missing_reason(v)) for v in missing),
        "derive": "param_discovery.derive",
        "frames": frames_path,
        "frames_sha256": sha256_file(frames_path) if os.path.isfile(frames_path) else None,
        "frames_count": frame_count(frames_path),
        "discovery_probe_rc": probe_rc,
        "discovery_probe_argv": _redact_argv(probe_argv),
    }


def cross_side_consistency(side_docs, required, session_scoped=SESSION_SCOPED_VARS):
    """两侧发现结果的一致性（fail-closed；任一不符 => 调用方 rc=2）。

    两条判据——「不许拿一侧的值跑另一侧」：
      1) 同一变量的 **kind** 必须两侧一致：一侧 `derived`、另一侧 `rule`
         == 「一侧可推、另一侧只能靠部署假设」⇒ 不一致；
      2) **value**：会话作用域变量（ActorId，每会话不同、比较器掩码 recog）允许不同；
         其余变量（坐标等）两侧必须相同，否则说明两侧根本不在同一起点。

    返回 (ok, rows)；rows 每行 {var, ok, reason, csharp, rust, ...} 可直接进 discovered.json。
    """
    rows = []
    ok = True
    for var in required:
        cs = ((side_docs.get("csharp") or {}).get("values") or {}).get(var)
        rs = ((side_docs.get("rust") or {}).get("values") or {}).get(var)
        row = {"var": var, "session_scoped": var in session_scoped}
        if cs is None or rs is None:
            row.update(ok=False, reason="missing",
                       csharp=None if cs is None else cs.get("value"),
                       rust=None if rs is None else rs.get("value"))
            ok = False
            rows.append(row)
            continue
        cv, rv = cs.get("value"), rs.get("value")
        ck, rk = cs.get("kind"), rs.get("kind")
        row.update(csharp=cv, rust=rv, csharp_kind=ck, rust_kind=rk)
        if cv is None or rv is None:
            row.update(ok=False, reason="missing")
            ok = False
        elif ck != rk:
            row.update(ok=False, reason="kind_mismatch")
            ok = False
        elif cv != rv and var not in session_scoped:
            row.update(ok=False, reason="value_mismatch")
            ok = False
        elif cv != rv:
            row.update(ok=True, reason="differs_expected(session_scoped)")
        else:
            row.update(ok=True, reason="same")
        rows.append(row)
    return ok, rows


def build_discovered_doc(summary, side_docs, required, complete, consistent, rows):
    """顶层 <out>/discovered.json：两侧各自的发现结果 + 跨侧一致性判定。"""
    return {
        "format": DISCOVERED_FORMAT,
        "tool": TOOL,
        "run_id": summary.get("run_id"),
        "card": summary.get("card"),
        "scenario": summary.get("scenario"),
        "scenario_sha256": summary.get("scenario_sha256"),
        "required": list(required),
        "session_scoped": list(SESSION_SCOPED_VARS),
        "sides": dict((side, side_docs.get(side)) for side in SIDES),
        "complete": bool(complete),
        "consistent": bool(consistent),
        "checks": rows,
        "policy": ("两侧各自发现（ActorId 每会话不同，compare_runs 掩码 recog）；"
                   "kind 不一致（derived vs rule）或非会话变量取值不同 => rc=2，"
                   "绝不拿一侧的值跑另一侧。"),
    }


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


def check_run_json(path, probe_rc):
    """M-1：run.json 必须非空、可解析，且 exit.code 与 probe rc 一致（且为 0）。"""
    if not os.path.exists(path):
        return "probe 未落 run.json：%s" % path
    try:
        if os.path.getsize(path) == 0:
            return "run.json 为空：%s" % path
        with open(path, "r", encoding="utf-8-sig") as f:
            data = json.load(f)
    except (OSError, ValueError) as e:
        return "run.json 不可解析：%s（%s）" % (path, e)
    code = (data.get("exit") or {}).get("code") if isinstance(data, dict) else None
    if not isinstance(code, int):
        return "run.json 缺 exit.code：%s" % path
    if code != probe_rc:
        return "run.json exit.code=%s 与 probe rc=%s 不一致" % (code, probe_rc)
    if code != 0:
        return "run.json exit.code=%s（probe 侧未完成）" % code
    return None


def stop_ok(attempted, stop_rc):
    """停栈成功判据：没起过栈 => 无需停；起过栈 => 必须真的执行且 rc=0。

    stop_rc=None 表示「停栈命令没跑成」（argv 非法/启动失败），**绝不当成功**。
    """
    if not attempted:
        return True
    return stop_rc == 0


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
        "discover_rc": None,
        "discovered": None,
        "vars": None,
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

        # 5) 参数发现（场景声明了 required，或显式 --discover）
        #    **两侧各自发现**：每侧跑自己的 discovery run、从本侧帧推导、用自己的值；
        #    任一侧推不出 => 该侧 rc=2 并点名缺哪个变量（不拿空值/默认值/另一侧的值）。
        probe_timeout = max(180, int(prof.get("probe_timeout_ms", 20000) / 1000) + 120)
        vars_argv = []
        disco = ctx.get("discovery") or {}
        if disco.get("needed"):
            dctx = discovery_ctx(ctx)
            os.makedirs(dctx["side_dir"], exist_ok=True)
            disc_argv = build_probe_argv(sc, dctx, prof, password)
            disc_env = dict(env)
            disc_env.update({"CARD_WINDOW_SIDE_DIR": dctx["side_dir"],
                             "CARD_WINDOW_DB_DIR": dctx["db_dir"],
                             "CARD_WINDOW_SCENARIO": dctx["scenario"]})
            res["discover_rc"] = runner.run(side, "discover", disc_argv, env=disc_env,
                                            timeout=probe_timeout)
            values, missing = {}, list(disco.get("required") or [])
            derr = None
            if res["discover_rc"] != 0:
                derr = ("discovery probe 失败（rc=%s）：收不到帧就无法推导参数"
                        % res["discover_rc"])
            else:
                frames_path = os.path.join(dctx["discovery_dir"], "frames.ndjson")
                try:
                    values, missing = derive_from_frames(frames_path,
                                                         disco.get("required") or [])
                except param_discovery.DiscoveryError as e:
                    derr = "discovery 输入坏掉：%s" % e
            doc = side_discovery_doc(side, dctx, values, missing, res["discover_rc"],
                                     disc_argv)
            if derr:
                doc["error"] = derr
                doc["ok"] = False
            try:
                write_json_file(os.path.join(side_dir, "discovered.json"), doc)
            except OSError as e:
                derr = ((derr + "；") if derr else "") + "写 discovered.json 失败：%s" % e
            res["discovered"] = doc
            if derr:
                res["error"] = derr
                return res
            if missing:
                res["error"] = ("参数发现失败：缺 %s（推导不出就 fail-closed，"
                                "不拿空值/默认值跑场景）" % ", ".join(missing))
                return res
            vars_argv = param_discovery.render_vars(values)
            res["vars"] = vars_argv

        # 6) probe（带上本侧 discovery 推导出的 --var）
        probe_argv = build_probe_argv(sc, ctx, prof, password, vars_argv=vars_argv)
        res["probe_rc"] = runner.run(side, "probe", probe_argv, env=env,
                                     timeout=probe_timeout)
        # 7) 抓库（probe 失败也拍，保留现场；缺快照 => 失败）
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
        run_err = check_run_json(os.path.join(side_dir, "run.json"), res["probe_rc"])
        if run_err:
            res["error"] = run_err
            return res
        if res["db_rc"] != 0:
            return res
        res["rc"] = 0
        return res
    finally:
        # 8) 停栈：只要尝试过起栈就执行（无论成败）；不做这一步会占端口、污染下一侧
        if stack_attempted:
            try:
                stop_argv = expand_argv(sc["stop"], ctx)
                res["stop_rc"] = runner.run(side, "stop", stop_argv, env=env, timeout=180)
            except (ConfigError, OSError) as e:
                res["stop_rc"] = None
                msg = "停栈命令无法执行：%s" % e
                res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
        if not stop_ok(stack_attempted, res["stop_rc"]):
            msg = ("停栈未成功（stack_attempted=%s stop_rc=%s）：端口可能仍被占用"
                   % (stack_attempted, res["stop_rc"]))
            res["error"] = (res["error"] + "；" + msg) if res["error"] else msg
            if res["rc"] == 0:
                res["rc"] = 2


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
    lines.append("run_id: %s" % summary.get("run_id"))
    lines.append("profile: %s" % summary.get("profile"))
    if summary.get("profile_description"):
        lines.append("profile_description: %s" % summary["profile_description"])
    lines.append("report_kind: %s" % summary.get("report_kind"))
    if summary.get("stale_archive"):
        lines.append("stale_archive: %s" % summary["stale_archive"])
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
    disc = summary.get("discovery") or {}
    if disc.get("enabled"):
        lines.append("discovery: scenario=%s required=%s complete=%s consistent=%s"
                     % (disc.get("scenario"),
                        ",".join(disc.get("required") or []) or "-",
                        disc.get("complete"), disc.get("consistent")))
        for side in SIDES:
            d = (disc.get("sides") or {}).get(side) or {}
            lines.append("  %-6s discovery ok=%s argv=%s missing=%s"
                         % (side, d.get("ok"), " ".join(d.get("argv") or []) or "-",
                            ",".join(d.get("missing") or []) or "-"))
        for row in disc.get("checks") or []:
            if not row.get("ok"):
                lines.append("         inconsistent %s: %s (csharp=%r rust=%r)"
                             % (row.get("var"), row.get("reason"),
                                row.get("csharp"), row.get("rust")))
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
        lines.append("  %-6s rc=%s restore=%s stack=%s ready_missing=%s discover=%s "
                     "probe=%s db=%s stop=%s frames=%s"
                     % (side, r.get("rc"), r.get("restore_rc"), r.get("stack_rc"),
                        r.get("ready", {}).get("missing"), r.get("discover_rc"),
                        r.get("probe_rc"), r.get("db_rc"), r.get("stop_rc"),
                        r.get("frames")))
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
def archive_existing_out(out_dir):
    """把 out 里上一次运行的产物整体挪进 _stale-<ts>-<pid>/。

    保证本次运行结束后目录里的 report.json（或 fallback）**只可能属于本次**，
    不会把上一次的 PASS 报告当成本次证据归档。
    """
    entries = sorted(e for e in os.listdir(out_dir) if not e.startswith("_stale-"))
    if not entries:
        return None
    ts = time.strftime("%Y%m%dT%H%M%S")
    dest = os.path.join(out_dir, "_stale-%s-%d" % (ts, os.getpid()))
    n = 1
    while os.path.exists(dest):
        dest = os.path.join(out_dir, "_stale-%s-%d-%d" % (ts, os.getpid(), n))
        n += 1
    os.makedirs(dest)
    for e in entries:
        os.replace(os.path.join(out_dir, e), os.path.join(dest, e))
    return dest


def write_fallback_report(out_dir, summary, reason):
    """编排失败/compare 没出报告时，**无条件**写本次的失败 report.json。"""
    doc = {
        "tool": TOOL,
        "format": "card-window-report/1",
        "run_id": summary.get("run_id"),
        "verdict": "UNDECIDABLE",
        "exit_code": 2,
        "compare": None,
        "compare_rc": summary.get("compare_rc"),
        "reason": reason,
        "notes": summary.get("notes"),
    }
    path = os.path.join(out_dir, "report.json")
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        json.dump(doc, f, ensure_ascii=False, indent=2, sort_keys=True)
        f.write("\n")
    return path


ARTIFACT_FILES = ("report.json", "discovered.json",
                  "csharp/frames.ndjson", "csharp/run.json", "csharp/discovered.json",
                  "csharp/db/manifest.json",
                  "rust/frames.ndjson", "rust/run.json", "rust/discovered.json",
                  "rust/db/manifest.json")


def write_artifacts(out_dir, summary):
    """登记 report.json / 两侧产物 sha256 并写 README.txt；返回 README 是否写成。

    F-4：所有收尾分支（含「缺二进制」提前收尾）共用这一步，保证产物集完整，
    绝不留下「只有 report.json + log」的半套证据。N-1：返回值供调用方把
    「最终化失败」如实写进 comment，而不是先发 PASS 账本再发现缺件。
    """
    readme_ok = True
    artifacts = {}
    for rel in ARTIFACT_FILES:
        p = os.path.join(out_dir, rel.replace("/", os.sep))
        if os.path.exists(p):
            artifacts[rel] = {"path": p, "sha256": sha256_file(p)}
        else:
            artifacts[rel] = {"path": p, "sha256": None}
    summary["artifacts"] = artifacts
    readme_path = os.path.join(out_dir, "README.txt")
    try:
        write_readme(readme_path, summary)
        summary["artifacts"]["README.txt"] = {"path": readme_path,
                                              "sha256": sha256_file(readme_path)}
    except OSError as e:
        summary["notes"].append("写 README.txt 失败：%s" % e)
        summary["exit_code"] = 2
        readme_ok = False
    return readme_ok


def write_summary_json(out_dir, summary):
    """最后写 card-window.json（包含 comment 结果）；返回是否写成。

    H-1：card-window.json 也是卡面承诺的产物之一，写失败必须让调用方置 rc=2，
    不能再静默吞掉（否则「完整产物集」的承诺与实际退出码不一致）。
    """
    try:
        summary_path = os.path.join(out_dir, "card-window.json")
        with open(summary_path, "w", encoding="utf-8", newline="\n") as f:
            json.dump(summary, f, ensure_ascii=False, indent=2, sort_keys=True)
            f.write("\n")
        summary["artifacts"]["card-window.json"] = {"path": summary_path,
                                                    "sha256": sha256_file(summary_path)}
        return True
    except OSError as e:
        summary["notes"].append("写 card-window.json 失败：%s" % e)
        return False


def execute(cfg):
    """cfg: dict(card, scenario, out, profile, password, post_comment, dry_run,
                 allow_production_db, rules, timeout_ms, ...) -> summary"""
    dry_run = bool(cfg.get("dry_run"))
    prof = load_profile(cfg.get("profile") if cfg.get("profile") is not None else "alt")
    prof = apply_overrides(prof, cfg)
    warnings = check_profile_safety(prof, bool(cfg.get("allow_production_db")), dry_run)

    raw_out = cfg.get("out")
    if raw_out:
        out_dir = os.path.abspath(raw_out)
    else:
        out_dir = "<out>" if dry_run else ""
    scenario = os.path.abspath(cfg.get("scenario") or "")
    card = cfg.get("card") or ""
    rules = os.path.abspath(cfg.get("rules") or DEFAULT_RULES)

    # 参数发现：场景声明了 required（或显式 --discover）就先跑 discovery。
    # 场景文件不存在时（--dry-run 允许）不读；真跑会在下面报「场景文件不存在」。
    required = []
    if scenario and os.path.isfile(scenario):
        required = scenario_required(scenario)
    discover = bool(cfg.get("discover"))
    discovery_scenario = os.path.abspath(
        cfg.get("discovery_scenario") or DEFAULT_DISCOVERY_SCENARIO)
    needs_discovery = discover or bool(required)
    discovery_cfg = {"needed": needs_discovery, "forced": discover,
                     "scenario": discovery_scenario, "required": required,
                     "frames": cfg.get("discovery_frames")}

    started_at = iso_now()
    t0 = time.monotonic()
    summary = {
        "format": FORMAT,
        "tool": TOOL,
        "run_id": uuid.uuid4().hex,
        "card": card,
        "profile": prof.get("name") or "custom",
        "profile_description": prof.get("description"),
        "scenario": scenario,
        "scenario_sha256": None,
        "out": out_dir,
        "stale_archive": None,
        "report_kind": None,
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
        "discovery": {
            "enabled": needs_discovery,
            "forced": discover,
            "scenario": discovery_scenario if needs_discovery else None,
            "scenario_sha256": (sha256_file(discovery_scenario)
                                if needs_discovery and os.path.isfile(discovery_scenario)
                                else None),
            "required": list(required),
            "session_scoped": list(SESSION_SCOPED_VARS),
            "sides": {},
            "complete": None,
            "consistent": None,
            "checks": [],
        },
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
        if not raw_out:
            summary["notes"].append(
                "缺 --out：真跑必须显式给产物目录（不允许把整套证据静默写进 cwd）")
            return summary
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
        if needs_discovery and not os.path.isfile(discovery_scenario):
            summary["notes"].append(
                "发现阶段场景不存在：%s（discovery 必须点一次 NPC 才收得到 SM_MERCHANTSAY(643)）"
                % discovery_scenario)
            return summary
        if cfg.get("discovery_frames"):
            summary["notes"].append(
                "--discovery-frames 只用于 --dry-run 预览；真跑必须由本次 discovery 推导")
            return summary
        try:
            os.makedirs(out_dir, exist_ok=True)
            # I-1：上一次运行的产物整体归档，保证本次 report.json 不会冒充旧证据
            stale = archive_existing_out(out_dir)
            summary["stale_archive"] = stale
            if stale:
                summary["notes"].append(
                    "out 原有内容已归档到 %s（本次 report.json 只属于本次运行）" % stale)
            log_path = os.path.join(out_dir, "card-window.log")
            with open(log_path, "w", encoding="utf-8", newline="\n") as f:
                f.write("%s card_window: start run_id=%s card=%s profile=%s scenario=%s\n"
                        % (time.strftime("%H:%M:%S"), summary["run_id"], card,
                           prof.get("name") or "custom", scenario))
        except OSError as e:
            summary["notes"].append("建不了/清理不了产物目录 %s：%s" % (out_dir, e))
            return summary
        runner = Runner(out_dir, log_path, dry_run=False)

    # dry-run：不起栈、不建目录、不校验文件是否存在，只打印完整命令序列
    if dry_run:
        preview_failed = False
        for side in SIDES:
            ctx = side_ctx(side, out_dir, scenario, card, discovery=discovery_cfg)
            sc = prof["sides"][side]
            runner.run(side, "restore", expand_argv(sc["restore"], ctx))
            runner.log("[%s] 端口预检：%s" % (side, sc.get("ports") or []))
            runner.log("[%s] 就绪等待：%s（timeout %ss）"
                       % (side, sc.get("ready_ports") or [],
                          prof.get("ready_timeout_s", 120)))
            runner.run(side, "stack", expand_argv(sc["stack"], ctx))
            vars_argv = []
            side_failed = False
            if needs_discovery:
                dctx = discovery_ctx(ctx)
                disc_argv = build_probe_argv(sc, dctx, prof, password)
                runner.run(side, "discover", disc_argv)
                pattern = cfg.get("discovery_frames")
                if required and pattern:
                    frames_path = expand_argv([str(pattern)], dctx)[0]
                    try:
                        values, missing = derive_from_frames(frames_path, required)
                    except param_discovery.DiscoveryError as e:
                        values, missing = {}, list(required)
                        runner.log("[%s] discovery 帧不可用：%s" % (side, e))
                    doc = side_discovery_doc(side, dctx, values, missing, 0, disc_argv)
                    summary["discovery"]["sides"][side] = doc
                    if missing:
                        runner.log("[%s] 参数发现失败：缺 %s ⇒ 真跑 rc=2（不执行场景）"
                                   % (side, ", ".join(missing)))
                        preview_failed = True
                        side_failed = True
                    else:
                        vars_argv = doc["argv"]
                elif required:
                    # 没给 --discovery-frames：只预览接线，值用占位符（绝不发明数值）
                    for var in sorted(required):
                        vars_argv += ["--var", "%s=<discovered:%s>" % (var, var)]
                    runner.log("[%s] 未给 --discovery-frames：--var 以 <discovered:NAME> 占位预览"
                               % side)
            if not side_failed:
                runner.run(side, "probe",
                           build_probe_argv(sc, ctx, prof, password, vars_argv=vars_argv))
                runner.run(side, "db", build_db_argv(sc, ctx))
            runner.run(side, "stop", expand_argv(sc["stop"], ctx))
        compare_argv = [sys.executable, "-I", "-B", COMPARE,
                        "--oracle", os.path.join(out_dir, "csharp"),
                        "--rust", os.path.join(out_dir, "rust"),
                        "--out", os.path.join(out_dir, "report.json"),
                        "--rules", rules]
        compare_argv += [str(x) for x in prof.get("compare_extra") or []]
        if not preview_failed:
            runner.run("compare", "compare", compare_argv)
        else:
            summary["notes"].append(
                "dry-run 预览：参数发现失败 ⇒ 真跑会 rc=2（命令序列到 stop 为止）")
        summary["commands"] = runner.commands
        summary["verdict"] = "DRY-RUN"
        summary["exit_code"] = 2 if preview_failed else 0
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        return summary

    # 二进制 sha256 是产物的一部分：配置了就必须存在（否则窗口证据不完整）
    missing_bins = [b["path"] for s in SIDES for b in summary["binaries"][s]
                    if not b["exists"]]
    if missing_bins:
        summary["notes"].append("缺两侧二进制 => 不跑（证据不完整）：%s" % ", ".join(missing_bins))
        summary["verdict"] = "UNDECIDABLE"
        summary["exit_code"] = 2
        try:
            write_fallback_report(out_dir, summary, "missing_binaries")
            summary["report_kind"] = "fallback"
        except OSError as e:
            summary["notes"].append("写 fallback report.json 失败：%s" % e)
        summary["commands"] = runner.commands
        summary["finished_at"] = iso_now()
        summary["duration_s"] = _fmt_duration(time.monotonic() - t0)
        # F-4 选「完整产物集」：report.json + README.txt + card-window.json + card-window.log 全落，
        # 不写半套证据（--post-comment 仍只在真正跑完编排时才发）。
        write_artifacts(out_dir, summary)
        if not write_summary_json(out_dir, summary):
            summary["exit_code"] = 2
        return summary

    sides_ok = True
    for side in SIDES:
        ctx = side_ctx(side, out_dir, scenario, card, discovery=discovery_cfg)
        try:
            res = run_side(runner, side, prof, ctx, password)
        except (ConfigError, OSError) as e:
            res = {"side": side, "rc": 2, "error": "配置/执行错误：%s" % e}
            summary["notes"].append("%s: %s" % (side, e))
        except Exception as e:  # 未预期异常也要落失败报告，不能留下半截证据
            res = {"side": side, "rc": 2, "error": "内部错误：%s: %s"
                   % (type(e).__name__, e)}
            summary["notes"].append("%s: 内部错误 %s: %s" % (side, type(e).__name__, e))
        summary["sides"][side] = res
        if res.get("discovered"):
            summary["discovery"]["sides"][side] = res["discovered"]
        if res.get("rc") != 0:
            sides_ok = False
            summary["notes"].append("%s 侧未完成：%s"
                                    % (side, res.get("error") or "rc=%s" % res.get("rc")))
            break

    # 两侧发现结果汇总：跨侧一致性（fail-closed）+ <out>/discovered.json（证据）
    discovery_evidence_ok = True
    if needs_discovery:
        side_docs = summary["discovery"]["sides"]
        complete = all(bool((side_docs.get(s) or {}).get("ok")) for s in SIDES)
        consistent, rows = (False, [])
        if complete:
            consistent, rows = cross_side_consistency(side_docs, required)
        summary["discovery"]["complete"] = complete
        summary["discovery"]["consistent"] = consistent if complete else None
        summary["discovery"]["checks"] = rows
        if sides_ok and not complete:
            # 走完两侧却没有两份 ok 的发现结果 => 异常态，保守判红（不 fail-open）
            sides_ok = False
            summary["notes"].append("参数发现结果不完整：两侧都必须有 ok 的 discovered 结果")
        if complete and not consistent:
            sides_ok = False
            bad = ["%s:%s" % (r.get("var"), r.get("reason")) for r in rows if not r.get("ok")]
            summary["notes"].append(
                "两侧参数发现不一致（%s）：不许拿一侧的值跑另一侧" % ", ".join(bad))
        try:
            write_json_file(os.path.join(out_dir, "discovered.json"),
                            build_discovered_doc(summary, side_docs, required,
                                                 complete, consistent, rows))
        except OSError as e:
            summary["notes"].append("写 discovered.json 失败：%s" % e)
            discovery_evidence_ok = False

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
            try:
                write_fallback_report(out_dir, summary, "compare_no_report")
                summary["report_kind"] = "fallback"
            except OSError as e:
                summary["notes"].append("写 fallback report.json 失败：%s" % e)
        else:
            summary["report_kind"] = "compare"
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
        # 编排没跑完：**无条件**覆盖成本次的 fallback report.json（显式标注不是比较报告）。
        # out 开跑前已归档旧产物，这里绝不会留下上一次的 report.json。
        summary["verdict"] = "UNDECIDABLE"
        try:
            write_fallback_report(out_dir, summary, "orchestration_incomplete")
            summary["report_kind"] = "fallback"
        except OSError as e:
            summary["notes"].append("写 fallback report.json 失败：%s" % e)

    summary["commands"] = runner.commands
    summary["finished_at"] = iso_now()
    summary["duration_s"] = _fmt_duration(time.monotonic() - t0)

    # H-1：先定稿 verdict/exit_code，再写产物——write_artifacts 在 README 写失败时置 rc=2，
    # 绝不能被后面的 `0 if ok else 2` 覆盖回 0（否则 PASS 运行缺 README 还报绿）。
    ok = (summary["verdict"] == "PASS" and summary.get("compare_rc") == 0
          and discovery_evidence_ok
          and all((summary["sides"].get(s) or {}).get("rc") == 0 for s in SIDES))
    summary["exit_code"] = 0 if ok else 2

    # 归档：report.json + 两侧产物 sha256 + README.txt（与缺二进制分支共用同一实现）
    artifacts_ok = write_artifacts(out_dir, summary)

    # N-1：comment 必须在**全部产物写完之后**发。card-window.json 先落一版（此时还没有
    # comment 回执），这样「最终化失败」能被 comment 如实记账，而不是先写账本再发现缺件。
    json_ok = write_summary_json(out_dir, summary)
    finalize_ok = artifacts_ok and json_ok
    if not finalize_ok:
        summary["exit_code"] = 2

    # --post-comment：只发 comment，绝不改卡状态
    if cfg.get("post_comment") and card:
        # N-1 + D-1：顶层只留**一个**三态 verdict，compare 的事实全部收进嵌套 compare。
        # 三态：compare FAIL => FAIL（确定失败）；无结论或证据缺件 => UNDECIDABLE；
        # 只有证据包完整且 compare PASS 才是 PASS。绝不让同级 report_verdict=PASS 与
        # verdict=UNDECIDABLE 并排出现（只扫 PASS 的读者会误读成「窗口通过」）。
        blockers = []
        if not artifacts_ok:
            blockers.append("evidence_incomplete: README.txt write failed")
        if not json_ok:
            blockers.append("evidence_incomplete: card-window.json write failed")
        if summary["compare_rc"] not in (None, 0):
            blockers.append("compare_rc=%s" % summary["compare_rc"])
        compare_verdict = summary["verdict"]
        if compare_verdict == "FAIL":
            comment_verdict = "FAIL"
            verdict_reason = "compare_fail"
            blockers.append("compare_fail")
        elif compare_verdict != "PASS":
            comment_verdict = "UNDECIDABLE"
            verdict_reason = "compare_%s" % str(compare_verdict).lower()
        elif summary["exit_code"] != 0 or not finalize_ok:
            comment_verdict = "UNDECIDABLE"
            verdict_reason = blockers[0] if blockers else "evidence_incomplete"
        else:
            comment_verdict = "PASS"
            verdict_reason = "ok"
        body = {
            "tool": TOOL,
            "run_id": summary["run_id"],
            "card": card,
            "profile": summary["profile"],
            "verdict": comment_verdict,
            "verdict_reason": verdict_reason,
            "blockers": blockers,
            "exit_code": summary["exit_code"],
            "report_kind": summary["report_kind"],
            "finalize_ok": finalize_ok,
            "discovery": ({
                "enabled": True,
                "required": summary["discovery"].get("required"),
                "complete": summary["discovery"].get("complete"),
                "consistent": summary["discovery"].get("consistent"),
                "sides": dict((s, {
                    "ok": (summary["discovery"].get("sides", {}).get(s) or {}).get("ok"),
                    "missing": (summary["discovery"].get("sides", {}).get(s) or {}).get("missing"),
                    "argv": (summary["discovery"].get("sides", {}).get(s) or {}).get("argv"),
                }) for s in SIDES),
            } if needs_discovery else None),
            "compare": {
                "verdict": compare_verdict,
                "rc": summary["compare_rc"],
                "frames": summary["frames"],
                "db_state": summary["db_state"],
                "diffs": summary["diffs"],
                "undecidable": summary["undecidable"],
                "whitelisted": summary["whitelisted"],
            },
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
                "discovered": (summary["artifacts"].get("discovered.json", {}).get("path")
                               if needs_discovery else None),
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

    # 摘要把 comment 回执追加进去（第二版）。这一版**只追加回执、不再新增证据**：
    # comment 判定所依据的产物（report.json / README.txt / card-window.json 首版）在
    # comment 之前已全部落盘（N-1）。回执写失败仍 => rc=2 + note。
    if not write_summary_json(out_dir, summary):
        summary["exit_code"] = 2
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
    run_exit = 0
    runjson_empty = False
    for i, a in enumerate(rest):
        if a == "--out" and i + 1 < len(rest):
            out = rest[i + 1]
        if a == "--mutate":
            mutate = True
        if a == "--stub-exit-code" and i + 1 < len(rest):
            run_exit = int(rest[i + 1])
        if a == "--stub-runjson-empty":
            runjson_empty = True
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
        with open(os.path.join(out, "stub-argv.json"), "w", encoding="utf-8") as f:
            json.dump({"side": side, "argv": sys.argv}, f, sort_keys=True)
        if os.path.basename(os.path.normpath(out)) == "discovery":
            if os.environ.get("STUB_DISCOVERY_FAIL") == side:
                sys.stderr.write("stub discovery: simulated probe failure\n")
                return 3
            self_x = 7 if side == "csharp" else 8
            npc = 119 if side == "csharp" else 129
            frames = [
                {"seq": 1, "ident": 50, "recog": 191, "param": self_x, "tag": 12},
                {"seq": 2, "ident": 10, "recog": 191, "param": 0, "tag": 0},
                {"seq": 3, "ident": 10, "recog": npc, "param": 0, "tag": 0},
            ]
            if not os.environ.get("STUB_DISCOVERY_NO_643"):
                frames.append({"seq": 4, "ident": 643, "recog": npc,
                               "param": 0, "tag": 0})
            with open(os.path.join(out, "frames.ndjson"), "w", encoding="utf-8",
                      newline="\n") as f:
                for frame in frames:
                    f.write(json.dumps(frame, sort_keys=True) + "\n")
            with open(os.path.join(out, "run.json"), "w", encoding="utf-8") as f:
                json.dump({"exit": {"code": 0, "reason": "stub-discovery"}}, f,
                          sort_keys=True)
            return 0
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
            if runjson_empty:
                f.write("")
            else:
                json.dump({"argv": sys.argv, "started_at": "stub", "ended_at": "stub",
                           "steps": [],
                           "exit": {"code": run_exit, "reason": "stub"}}, f, sort_keys=True)
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
            "db": {"command": [py, stub, "db", name, log, "{db_dir}"],
                   "databases": ["mir2_test_db"]},
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
        try:
            with open(os.path.join(out, "README.txt"), encoding="utf-8") as f:
                readme = f.read()
        except OSError:
            readme = ""
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

        # 7b) 参数发现接线（卡 openmir2-window-discovery-wiring）：required 场景 =>
        #     每侧先 discover 再 probe；probe 命令必须真的带上 --var，值来自**本侧**发现。
        scen_npc = os.path.join(tmp, "scen-npc.json")
        scen_miss = os.path.join(tmp, "scen-miss.json")
        scen_selfx = os.path.join(tmp, "scen-selfx.json")
        for path, req in ((scen_npc, ["npc_actor"]), (scen_miss, ["make_index"]),
                          (scen_selfx, ["self_x"])):
            with open(path, "w", encoding="utf-8") as f:
                json.dump({"name": os.path.basename(path), "required": req,
                           "steps": [{"wait_for": 50, "timeout_ms": 10}]}, f)

        def probe_argv_of(out_dir, side):
            try:
                with open(os.path.join(out_dir, side, "stub-argv.json"),
                          encoding="utf-8") as f:
                    return json.load(f).get("argv") or []
            except (OSError, ValueError):
                return []

        fresh_log()
        out_disc = os.path.join(tmp, "run-disc")
        s_disc = execute(base_cfg(out_disc, _selftest_profile(tmp, stub, log),
                                  scenario=scen_npc))
        disc_steps = [c["step"] for c in s_disc["commands"]]
        chk(s_disc["exit_code"] == 0 and s_disc["verdict"] == "PASS",
            "DISC-1 可推导场景 => rc=0/PASS（实际 %s/%s）"
            % (s_disc["exit_code"], s_disc["verdict"]))
        chk(disc_steps == ["restore", "stack", "discover", "probe", "db", "stop"] * 2
            + ["compare"],
            "DISC-1 命令序列 discover 先于 probe（实际 %s）" % disc_steps)
        argv_c = probe_argv_of(out_disc, "csharp")
        argv_r = probe_argv_of(out_disc, "rust")
        chk("--var" in argv_c and "npc_actor=119" in argv_c,
            "DISC-1 csharp 的 probe 命令带本侧发现的 --var npc_actor=119（实际 %s）"
            % argv_c[-4:])
        chk("--var" in argv_r and "npc_actor=129" in argv_r,
            "DISC-2 两侧各自发现各自的值（rust --var npc_actor=129，实际 %s）"
            % argv_r[-4:])
        try:
            with open(os.path.join(out_disc, "csharp", "discovered.json"),
                      encoding="utf-8") as f:
                d_c = json.load(f)
        except (OSError, ValueError):
            d_c = {}
        chk(d_c.get("ok") is True and d_c.get("missing") == []
            and (d_c.get("values", {}).get("npc_actor") or {}).get("value") == 119
            and (d_c.get("values", {}).get("npc_actor") or {}).get("kind") == "derived"
            and (d_c.get("values", {}).get("npc_actor") or {}).get("source")
            == {"ident": 643, "field": "recog", "seq": 4}
            and d_c.get("argv") == ["--var", "npc_actor=119"]
            and d_c.get("missing_reasons") == {},
            "DISC-3 单侧 discovered.json 含值/kind/source/argv（%s）"
            % {k: d_c.get(k) for k in ("ok", "missing", "argv")})
        try:
            with open(os.path.join(out_disc, "discovered.json"), encoding="utf-8") as f:
                d_all = json.load(f)
        except (OSError, ValueError):
            d_all = {}
        chk(d_all.get("format") == "discovered/1"
            and set(d_all.get("sides") or {}) == {"csharp", "rust"}
            and d_all.get("complete") is True and d_all.get("consistent") is True
            and bool(d_all.get("checks"))
            and d_all["checks"][0].get("reason") == "differs_expected(session_scoped)",
            "DISC-3 顶层 discovered.json 字段齐全，会话作用域差异不算不一致（%s）"
            % (d_all.get("checks"),))

        fresh_log()
        out_miss = os.path.join(tmp, "run-disc-miss")
        s_miss = execute(base_cfg(out_miss, _selftest_profile(tmp, stub, log),
                                  scenario=scen_miss))
        err_miss = (s_miss["sides"].get("csharp") or {}).get("error") or ""
        miss_steps = [c["step"] for c in s_miss["commands"]]
        chk(s_miss["exit_code"] == 2 and "make_index" in err_miss,
            "DISC-4 推导失败 => rc=2 且点名缺哪个变量（%r）" % err_miss)
        chk(miss_steps == ["restore", "stack", "discover", "stop"],
            "DISC-4 推导失败 => 不跑场景（不静默用空值，%s）" % miss_steps)
        try:
            with open(os.path.join(out_miss, "csharp", "discovered.json"),
                      encoding="utf-8") as f:
                d_miss = json.load(f)
        except (OSError, ValueError):
            d_miss = {}
        chk(d_miss.get("ok") is False and d_miss.get("missing") == ["make_index"]
            and bool((d_miss.get("missing_reasons") or {}).get("make_index"))
            and d_miss.get("argv") == [],
            "DISC-4 失败也留证：missing/missing_reasons/空 argv（%s）"
            % {k: d_miss.get(k) for k in ("ok", "missing", "argv")})

        fresh_log()
        out_selfx = os.path.join(tmp, "run-disc-selfx")
        s_selfx = execute(base_cfg(out_selfx, _selftest_profile(tmp, stub, log),
                                   scenario=scen_selfx))
        chk(s_selfx["exit_code"] == 2
            and s_selfx["discovery"].get("consistent") is False,
            "DISC-5 非会话变量两侧取值不同 => rc=2/consistent=False（实际 %s/%s）"
            % (s_selfx["exit_code"], s_selfx["discovery"].get("consistent")))
        chk(any("两侧参数发现不一致" in n and "self_x" in n
                for n in s_selfx.get("notes") or []),
            "DISC-5 不一致在 notes 里点名变量（%s）"
            % [n for n in s_selfx.get("notes") or [] if "不一致" in n])

        prof_kind = _selftest_profile(tmp, stub, log)
        prof_kind["sides"]["rust"]["env"] = {"STUB_DISCOVERY_NO_643": "1"}
        fresh_log()
        out_kind = os.path.join(tmp, "run-disc-kind")
        s_kind = execute(base_cfg(out_kind, prof_kind, scenario=scen_npc))
        chk(s_kind["exit_code"] == 2
            and any("kind_mismatch" in n for n in s_kind.get("notes") or []),
            "DISC-6 一侧 derived / 另一侧 rule => rc=2（一侧可推另一侧不可，%s）"
            % [n for n in s_kind.get("notes") or [] if "不一致" in n])

        replay = os.path.join(tmp, "replay.ndjson")
        with open(replay, "w", encoding="utf-8") as f:
            for rec in ({"seq": 1, "ident": 50, "recog": 191, "param": 7, "tag": 12},
                        {"seq": 2, "ident": 10, "recog": 119},
                        {"seq": 3, "ident": 643, "recog": 119}):
                f.write(json.dumps(rec) + "\n")
        out_dry2 = os.path.join(tmp, "run-dry-disc")
        s_dry2 = execute(base_cfg(out_dry2, _selftest_profile(tmp, stub, log),
                                  scenario=scen_npc, dry_run=True,
                                  discovery_frames=replay))
        dry_probe = [c for c in s_dry2["commands"]
                     if c["side"] == "csharp" and c["step"] == "probe"]
        dry_steps = [c["step"] for c in s_dry2["commands"]]
        chk(len(dry_probe) == 1 and "--var" in dry_probe[0]["argv"]
            and "npc_actor=119" in dry_probe[0]["argv"],
            "DISC-7 --dry-run 的命令序列里确实带上了 --var 且值来自发现（%s）"
            % (dry_probe[0]["argv"][-4:] if dry_probe else None))
        chk(dry_steps == ["restore", "stack", "discover", "probe", "db", "stop"] * 2
            + ["compare"],
            "DISC-7 --dry-run 命令序列 discover 先于 probe（%s）" % dry_steps)
        chk(not os.path.exists(out_dry2), "DISC-7 --dry-run 仍不创建产物目录")

        s_dry_miss = execute(base_cfg(os.path.join(tmp, "run-dry-miss"),
                                      _selftest_profile(tmp, stub, log),
                                      scenario=scen_miss, dry_run=True,
                                      discovery_frames=replay))
        miss_dry_steps = [c["step"] for c in s_dry_miss["commands"]]
        chk(s_dry_miss["exit_code"] == 2
            and miss_dry_steps[:4] == ["restore", "stack", "discover", "stop"]
            and "probe" not in miss_dry_steps,
            "DISC-8 --dry-run 回放推不出 => rc=2 且不打印场景命令（%s/%s）"
            % (s_dry_miss["exit_code"], miss_dry_steps[:6]))

        s_dry_prev = execute(base_cfg(os.path.join(tmp, "run-dry-prev"),
                                      _selftest_profile(tmp, stub, log),
                                      scenario=scen_npc, dry_run=True))
        prev_probe = [c for c in s_dry_prev["commands"]
                      if c["side"] == "csharp" and c["step"] == "probe"][0]
        chk(any(a.startswith("npc_actor=<discovered:") for a in prev_probe["argv"])
            and s_dry_prev["exit_code"] == 0,
            "DISC-8 无 --discovery-frames => 占位符预览（不发明数值，%s）"
            % prev_probe["argv"][-2:])

        # F-1：--discover 强制 + 场景**无 required** + discovery probe 失败 =>
        #      side doc 必须 ok=False（不能因为 missing 为空就写 ok=True），
        #      顶层 discovered.json 必须 complete=False/consistent=False。
        scen_noreq = os.path.join(tmp, "scen-no-required.json")
        with open(scen_noreq, "w", encoding="utf-8") as f:
            json.dump({"name": "no-required", "steps": []}, f)
        prof_f1 = _selftest_profile(tmp, stub, log)
        prof_f1["sides"]["csharp"]["env"] = {"STUB_DISCOVERY_FAIL": "csharp"}
        fresh_log()
        out_f1 = os.path.join(tmp, "run-f1")
        s_f1 = execute(base_cfg(out_f1, prof_f1, scenario=scen_noreq, discover=True))
        try:
            with open(os.path.join(out_f1, "csharp", "discovered.json"),
                      encoding="utf-8") as f:
                d_f1 = json.load(f)
        except (OSError, ValueError):
            d_f1 = {}
        try:
            with open(os.path.join(out_f1, "discovered.json"), encoding="utf-8") as f:
                d_f1_all = json.load(f)
        except (OSError, ValueError):
            d_f1_all = {}
        chk(s_f1["exit_code"] == 2 and d_f1.get("discovery_probe_rc") == 3
            and d_f1.get("ok") is False and bool(d_f1.get("error")),
            "F-1 discovery probe 失败（required 为空）=> side ok=False/probe_rc=3（%s）"
            % {k: d_f1.get(k) for k in ("ok", "discovery_probe_rc", "error")})
        chk(d_f1_all.get("complete") is False
            and d_f1_all.get("consistent") is False,
            "F-1 discovery probe 失败 => 顶层 complete=False/consistent=False（%s）"
            % {k: d_f1_all.get(k) for k in ("complete", "consistent")})

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
        chk(captured.get("exit_code") == 0
            and captured.get("report_kind") == "compare"
            and captured.get("finalize_ok") is True
            and (captured.get("compare") or {}).get("verdict") == "PASS"
            and captured.get("verdict_reason") == "ok"
            and captured.get("blockers") == [],
            "D-1 正常 comment：verdict=PASS/verdict_reason=ok/blockers=[]/"
            "compare.verdict=PASS（N-1 字段齐全）")
        chk("report_verdict" not in captured and "compare_rc" not in captured
            and "frames" not in captured and "diffs" not in captured,
            "D-1 顶层不再有 report_verdict/compare_rc/frames/diffs（收进嵌套 compare）")
        chk(set((captured.get("compare") or {}).keys())
            == {"verdict", "rc", "frames", "db_state", "diffs", "undecidable",
                "whitelisted"},
            "M-2 嵌套 compare 字段集合完整（%s）"
            % sorted((captured.get("compare") or {}).keys()))
        # P-1：值断言，一律从本次 report.json 实读（不硬编码）。
        rep_c = read_report(os.path.join(out_c, "report.json")) or {}
        cmp_c = captured.get("compare") or {}
        chk(cmp_c.get("db_state") == (rep_c.get("db") or {}).get("state") == "compared",
            "P-1 PASS db_state = report.db.state = compared（实际 %r / %r）"
            % (cmp_c.get("db_state"), (rep_c.get("db") or {}).get("state")))
        chk(cmp_c.get("diffs") == len(rep_c.get("diffs") or []) == 0,
            "P-1 PASS diffs = len(report.diffs) = 0（实际 %r / %r）"
            % (cmp_c.get("diffs"), len(rep_c.get("diffs") or [])))
        chk(cmp_c.get("undecidable") == len(rep_c.get("undecidable") or []) == 0,
            "P-1 PASS undecidable = len(report.undecidable) = 0（实际 %r / %r）"
            % (cmp_c.get("undecidable"), len(rep_c.get("undecidable") or [])))
        chk(cmp_c.get("whitelisted") == len(rep_c.get("whitelisted") or []),
            "P-1 PASS whitelisted = len(report.whitelisted)（实际 %r / %r）"
            % (cmp_c.get("whitelisted"), len(rep_c.get("whitelisted") or [])))
        chk("status" not in captured
            and captured.get("card") == "openmir2-card-window-tool-selftest",
            "comment body 不含 status（绝不改卡状态），card 正确")
        chk(((captured.get("compare") or {}).get("frames") or {}).get("compared") == 1
            and captured.get("scenario_sha256") == sha256_file(scenario)
            and (captured.get("evidence") or {}).get("report"),
            "comment 带帧数（compare.frames）/ 场景 sha256 / 证据路径")

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

        # 12) B-1：停栈 argv 非法（字符串 / [] / null）=> 加载期 ConfigError（绝不当成功）
        for label, bad_stop in (("字符串", "powershell -File stop-rust.ps1"),
                                ("空数组", []),
                                ("null", None)):
            fresh_log()
            p_b1 = _selftest_profile(tmp, stub, log)
            p_b1["sides"]["rust"]["stop"] = bad_stop
            b1_refused = False
            try:
                execute(base_cfg(os.path.join(tmp, "run-badstop"), p_b1))
            except ConfigError:
                b1_refused = True
            chk(b1_refused, "B-1 停栈 argv=%s => 加载期 ConfigError（不假绿）" % label)
            chk(not os.path.exists(log), "B-1 停栈 argv=%s => 一步都没执行" % label)
        chk(not stop_ok(True, None) and not stop_ok(True, 4) and stop_ok(True, 0)
            and stop_ok(False, None),
            "B-1 兜底：起过栈时 stop_rc=None/非 0 一律判失败")

        # 12b) F-2（核心）：停栈兜底必须挂在 **run_side 调用点**（finally），不能只测 stop_ok()。
        #      做法：monkeypatch Runner.run，只让 step=stop 返回 None / 非 0，其余步骤照常走桩；
        #      再走完整 execute（真调用 run_side），断言该侧 rc=2、工具 rc=2。
        #      牙：把 finally 的判断改回旧式 `res["rc"]==0 and stop_rc not in (0,None)`
        #      ⇒ stop_rc=None 不再判失败 ⇒ 本组必红（这正是上一轮 62/62 全绿的盲区）。
        orig_runner_run = Runner.run
        stop_state = {"value": "orig"}

        def patched_runner_run(self, side, step, argv, env=None, timeout=600):
            if step == "stop" and stop_state["value"] != "orig":
                safe = _redact_argv(argv)
                self.commands.append({"side": side, "step": step, "argv": safe})
                self.steps.append("%s:%s" % (side, step))
                self.log("== %s/%s :: %s" % (side, step, _join_argv(safe)))
                self.log("   %s/%s rc=%s" % (side, step, stop_state["value"]))
                return stop_state["value"]
            return orig_runner_run(self, side, step, argv, env=env, timeout=timeout)

        Runner.run = patched_runner_run
        try:
            for label, stop_rc in (("None", None), ("rc=9", 9)):
                stop_state["value"] = stop_rc
                fresh_log()
                tag = "none" if stop_rc is None else "rc9"
                out_stop = os.path.join(tmp, "run-stop-" + tag)
                s_stop = execute(base_cfg(out_stop, _selftest_profile(tmp, stub, log)))
                cs = s_stop["sides"].get("csharp") or {}
                rep_stop = read_report(os.path.join(out_stop, "report.json")) or {}
                chk(s_stop["exit_code"] == 2,
                    "F-2 stop_rc=%s => 工具 rc=2（实际 %s）" % (label, s_stop["exit_code"]))
                chk(cs.get("rc") == 2,
                    "F-2 stop_rc=%s => run_side 调用点判失败 rc=2（实际 %s）"
                    % (label, cs.get("rc")))
                chk("停栈未成功" in (cs.get("error") or ""),
                    "F-2 stop_rc=%s => 错误写明停栈未成功（%r）"
                    % (label, (cs.get("error") or "")[:80]))
                chk(rep_stop.get("reason") == "orchestration_incomplete"
                    and rep_stop.get("run_id") == s_stop.get("run_id"),
                    "F-2 stop_rc=%s => 仍落本次 fallback report（不吞证据）" % label)
        finally:
            Runner.run = orig_runner_run
            stop_state["value"] = "orig"

        # 13) I-1：同一 out 先 PASS 后失败 => 本次 report 是 fallback，旧 PASS 进 _stale-*
        fresh_log()
        out_reuse = os.path.join(tmp, "run-reuse")
        s_first = execute(base_cfg(out_reuse, _selftest_profile(tmp, stub, log)))
        old_report = os.path.join(out_reuse, "report.json")
        old_sha = sha256_file(old_report)
        old_verdict = (read_report(old_report) or {}).get("verdict")
        s_second = execute(base_cfg(
            out_reuse, _selftest_profile(tmp, stub, log, stack_fail="csharp")))
        new_rep = read_report(os.path.join(out_reuse, "report.json")) or {}
        stale_dir = s_second.get("stale_archive") or ""
        stale_rep = read_report(os.path.join(stale_dir, "report.json")) if stale_dir else None
        chk(s_first["exit_code"] == 0 and old_verdict == "PASS", "I-1 前置：第一次 PASS")
        chk(s_second["exit_code"] == 2 and new_rep.get("verdict") == "UNDECIDABLE"
            and new_rep.get("format") == "card-window-report/1",
            "I-1 第二次失败 => report.json 是本次 fallback UNDECIDABLE（不冒充旧 PASS）")
        first_rid = s_first.get("run_id")
        second_rid = s_second.get("run_id")
        stale_summary = {}
        try:
            with open(os.path.join(stale_dir, "card-window.json"),
                      encoding="utf-8-sig") as f:
                stale_summary = json.load(f)
        except (OSError, ValueError):
            stale_summary = {}
        chk(bool(first_rid) and bool(second_rid) and first_rid != second_rid,
            "F-5 两次运行 run_id 必须不同（%r / %r）" % (first_rid, second_rid))
        chk(new_rep.get("run_id") == second_rid and new_rep.get("run_id") != first_rid,
            "F-5 fallback report 的 run_id = 本次（且不是上次）")
        chk(stale_summary.get("run_id") == first_rid
            and stale_summary.get("run_id") != second_rid,
            "F-5 _stale-* 里上次运行的 card-window.json run_id = 上次（不是本次，%r）"
            % stale_summary.get("run_id"))
        chk(bool(stale_dir) and (stale_rep or {}).get("verdict") == "PASS"
            and sha256_file(os.path.join(stale_dir, "report.json")) == old_sha,
            "I-1 旧 PASS report 已原样归档到 _stale-*")
        chk((s_second.get("artifacts") or {}).get("report.json", {}).get("sha256") != old_sha,
            "I-1 归档的 report sha256 指向本次文件（不是旧 PASS）")

        # H-2：第三次复用同一 out => 两个 _stale-* 各自独立，第一次的 PASS 存档不被覆盖
        s_third = execute(base_cfg(out_reuse, _selftest_profile(tmp, stub, log)))
        stale_names = sorted(d for d in os.listdir(out_reuse) if d.startswith("_stale-"))
        stale_dirs = [os.path.join(out_reuse, d) for d in stale_names]
        third_stale = s_third.get("stale_archive") or ""
        pass_copy = [d for d in stale_dirs
                     if os.path.exists(os.path.join(d, "report.json"))
                     and sha256_file(os.path.join(d, "report.json")) == old_sha]
        fallback_copy = []
        for d in stale_dirs:
            try:
                with open(os.path.join(d, "report.json"), encoding="utf-8-sig") as f:
                    r = json.load(f)
            except (OSError, ValueError):
                continue
            if (r.get("reason") == "orchestration_incomplete"
                    and r.get("run_id") == second_rid):
                fallback_copy.append(d)
        chk(s_third["exit_code"] == 0 and len(stale_dirs) == 2,
            "H-2 第三次运行 => 两个 _stale-* 各自独立（%s）" % stale_names)
        chk(third_stale in stale_dirs and bool(pass_copy) and bool(fallback_copy)
            and pass_copy[0] != fallback_copy[0],
            "H-2 归档不互相覆盖：第一次 PASS 与第二次 fallback 各在一处（%s）"
            % [os.path.basename(d) for d in stale_dirs])

        # 13b) F-4：缺二进制提前收尾也必须产出**完整产物集**（report.json + README.txt +
        #      card-window.json + card-window.log），不写半套证据；文档写明选了「完整产物集」。
        fresh_log()
        out_nobin = os.path.join(tmp, "run-nobin")
        p_nobin = _selftest_profile(tmp, stub, log)
        missing_bin = os.path.join(tmp, "no-such-binary.exe")
        p_nobin["sides"]["rust"]["binaries"] = [missing_bin]
        s_nobin = execute(base_cfg(out_nobin, p_nobin))
        nobin_files = ["report.json", "README.txt", "card-window.json", "card-window.log"]
        missing_out = [f for f in nobin_files
                       if not os.path.exists(os.path.join(out_nobin, f))]
        nobin_rep = read_report(os.path.join(out_nobin, "report.json")) or {}
        chk(s_nobin["exit_code"] == 2 and s_nobin["verdict"] == "UNDECIDABLE",
            "F-4 缺二进制 => rc=2/UNDECIDABLE（实际 %s/%s）"
            % (s_nobin["exit_code"], s_nobin["verdict"]))
        chk(not missing_out, "F-4 缺二进制分支产物集完整（缺 %s）" % missing_out)
        chk(nobin_rep.get("reason") == "missing_binaries"
            and nobin_rep.get("run_id") == s_nobin.get("run_id"),
            "F-4 缺二进制 => fallback report 写明 missing_binaries 且 run_id 属于本次")
        try:
            with open(os.path.join(out_nobin, "README.txt"), encoding="utf-8") as f:
                nobin_readme = f.read()
        except OSError:
            nobin_readme = ""
        chk(missing_bin in nobin_readme and "UNDECIDABLE" in nobin_readme,
            "F-4 缺二进制 => README 点名缺失二进制并写明 verdict")
        chk(bool((s_nobin.get("artifacts") or {}).get("README.txt", {}).get("sha256"))
            and bool((s_nobin.get("artifacts") or {}).get("card-window.json", {}).get("sha256")),
            "F-4 缺二进制 => 摘要登记 README/card-window.json 的 sha256")
        chk("stop csharp" not in read_log() and "stack csharp" not in read_log(),
            "F-4 缺二进制 => 一步都没执行（不擅自起栈）")

        # 13c) H-1：正常 PASS 路径上 README 写失败 => exit=2（写产物失败不能被 PASS 覆盖回 0）
        orig_write_readme = globals()["write_readme"]

        def boom_readme(path, summary):
            raise OSError("simulated disk full")

        globals()["write_readme"] = boom_readme
        try:
            fresh_log()
            out_h1 = os.path.join(tmp, "run-h1")
            s_h1 = execute(base_cfg(out_h1, _selftest_profile(tmp, stub, log)))
        finally:
            globals()["write_readme"] = orig_write_readme
        chk(s_h1["exit_code"] == 2 and s_h1["verdict"] == "PASS",
            "H-1 PASS 路径 README 写失败 => exit=2（实际 %s/%s）"
            % (s_h1["exit_code"], s_h1["verdict"]))
        chk(any("写 README.txt 失败" in n for n in (s_h1.get("notes") or []))
            and not os.path.exists(os.path.join(out_h1, "README.txt")),
            "H-1 README 缺失被记为失败证据（notes 有记录、文件确实不在）")
        fresh_log()
        out_h1ok = os.path.join(tmp, "run-h1-ok")
        s_h1ok = execute(base_cfg(out_h1ok, _selftest_profile(tmp, stub, log)))
        chk(s_h1ok["exit_code"] == 0
            and os.path.exists(os.path.join(out_h1ok, "README.txt")),
            "H-1 对照组：README 正常写入 => exit=0 且文件存在")

        # 13d) H-1b：card-window.json 写失败 => rc=2（不再静默吞掉）
        probe_summary = {"notes": [], "artifacts": {}, "exit_code": 0}
        wsj_ok = write_summary_json(os.path.join(tmp, "no-such-dir"), probe_summary)
        chk(wsj_ok is False
            and any("card-window.json 失败" in n for n in probe_summary["notes"]),
            "H-1b write_summary_json 写失败 => 返回 False 并记 note")
        orig_wsj = globals()["write_summary_json"]
        globals()["write_summary_json"] = lambda out_dir, summary: False
        try:
            fresh_log()
            out_h1w = os.path.join(tmp, "run-h1w")
            s_h1w = execute(base_cfg(out_h1w, _selftest_profile(tmp, stub, log)))
        finally:
            globals()["write_summary_json"] = orig_wsj
        chk(s_h1w["exit_code"] == 2,
            "H-1b card-window.json 写失败 => 调用方 rc=2（实际 %s）" % s_h1w["exit_code"])

        # 13e) N-1：最终化失败 => comment 必须是 UNDECIDABLE + 带 exit_code/report_kind，
        #      且 comment 发生在**产物写完之后**（post 时 card-window.json 已存在）。
        captured_n1 = {}
        seen_n1 = {}
        out_n1 = os.path.join(tmp, "run-n1")
        orig_post_n1 = globals()["post_comment"]

        def capture_post_n1(card, body, repo):
            captured_n1.update(body)
            seen_n1["card_window_exists"] = os.path.exists(
                os.path.join(out_n1, "card-window.json"))
            seen_n1["log_exists"] = os.path.exists(
                os.path.join(out_n1, "card-window.log"))
            return True, "stub-n1-oid"

        globals()["write_readme"] = boom_readme
        globals()["post_comment"] = capture_post_n1
        try:
            fresh_log()
            s_n1 = execute(base_cfg(out_n1, _selftest_profile(tmp, stub, log),
                                    post_comment=True))
        finally:
            globals()["write_readme"] = orig_write_readme
            globals()["post_comment"] = orig_post_n1
        chk(captured_n1.get("verdict") == "UNDECIDABLE"
            and captured_n1.get("exit_code") == 2,
            "N-1 README 写失败 => comment verdict=UNDECIDABLE + exit_code=2（实际 %r/%r）"
            % (captured_n1.get("verdict"), captured_n1.get("exit_code")))
        chk(captured_n1.get("report_kind") == "compare"
            and (captured_n1.get("compare") or {}).get("verdict") == "PASS"
            and captured_n1.get("finalize_ok") is False
            and any("README.txt" in b for b in (captured_n1.get("blockers") or []))
            and str(captured_n1.get("verdict_reason") or "").startswith(
                "evidence_incomplete"),
            "N-1 comment 带 compare.verdict/blockers/verdict_reason（不掩盖 compare 结论）")
        chk(seen_n1.get("card_window_exists") is True
            and seen_n1.get("log_exists") is True,
            "N-1 comment 发生在产物写完之后（post 时 card-window.json/log 已存在，%s）"
            % seen_n1)
        chk(s_n1["exit_code"] == 2
            and (s_n1.get("comment") or {}).get("posted") is True,
            "N-1 最终化失败仍把结论记到卡上（posted=True）且工具 rc=2")

        # 13f) Y1：首版 card-window.json 写失败（README 正常）=> comment 也必须 UNDECIDABLE。
        #      N-1 的不变量是 finalize_ok = artifacts_ok AND json_ok，两半各要一颗牙。
        calls_y1 = {"n": 0}
        orig_wsj_y1 = globals()["write_summary_json"]

        def fail_first_wsj(out_dir_, summary_):
            calls_y1["n"] += 1
            if calls_y1["n"] == 1:
                summary_["notes"].append("写 card-window.json 失败：simulated")
                return False
            return orig_wsj_y1(out_dir_, summary_)

        captured_y1 = {}
        orig_post_y1 = globals()["post_comment"]

        def capture_post_y1(card, body, repo):
            captured_y1.update(body)
            return True, "stub-y1-oid"

        globals()["write_summary_json"] = fail_first_wsj
        globals()["post_comment"] = capture_post_y1
        try:
            fresh_log()
            out_y1 = os.path.join(tmp, "run-y1")
            s_y1 = execute(base_cfg(out_y1, _selftest_profile(tmp, stub, log),
                                    post_comment=True))
        finally:
            globals()["write_summary_json"] = orig_wsj_y1
            globals()["post_comment"] = orig_post_y1
        chk(captured_y1.get("verdict") == "UNDECIDABLE"
            and captured_y1.get("exit_code") == 2
            and captured_y1.get("finalize_ok") is False,
            "Y1 首版 card-window.json 写失败 => comment UNDECIDABLE/exit=2/finalize_ok=False"
            "（实际 %r/%r/%r）" % (captured_y1.get("verdict"),
                                   captured_y1.get("exit_code"),
                                   captured_y1.get("finalize_ok")))
        chk(any("card-window.json" in b for b in (captured_y1.get("blockers") or []))
            and (captured_y1.get("compare") or {}).get("verdict") == "PASS",
            "Y1 comment 的 blockers 点名 card-window.json 缺件，compare 事实仍如实")
        chk(s_y1["exit_code"] == 2
            and (s_y1.get("comment") or {}).get("posted") is True,
            "Y1 首版摘要写失败仍发 comment 记账且工具 rc=2")

        # 13g) D-1：compare FAIL => comment 顶层 verdict=FAIL（不是 UNDECIDABLE）
        captured_f = {}
        orig_post_f = globals()["post_comment"]

        def capture_post_f(card, body, repo):
            captured_f.update(body)
            return True, "stub-f-oid"

        globals()["post_comment"] = capture_post_f
        try:
            fresh_log()
            out_f = os.path.join(tmp, "run-fail-comment")
            s_f = execute(base_cfg(out_f, _selftest_profile(tmp, stub, log,
                                                           mutate_rust=True),
                                   post_comment=True))
        finally:
            globals()["post_comment"] = orig_post_f
        chk(captured_f.get("verdict") == "FAIL"
            and captured_f.get("verdict_reason") == "compare_fail"
            and (captured_f.get("compare") or {}).get("verdict") == "FAIL"
            and (captured_f.get("compare") or {}).get("rc") == 1
            and captured_f.get("exit_code") == 2
            and "compare_fail" in (captured_f.get("blockers") or []),
            "D-1 compare FAIL => comment verdict=FAIL + verdict_reason=compare_fail"
            " + compare.rc=1 + blockers 含 compare_fail（实际 %r/%r/%r）"
            % (captured_f.get("verdict"), captured_f.get("verdict_reason"),
               (captured_f.get("compare") or {}).get("verdict")))
        chk(s_f["exit_code"] == 2, "D-1 compare FAIL => 工具 rc=2")
        # P-1：FAIL 侧同样从本次 report.json 实读（diffs=1）。
        rep_f = read_report(os.path.join(out_f, "report.json")) or {}
        cmp_f = captured_f.get("compare") or {}
        chk(cmp_f.get("db_state") == (rep_f.get("db") or {}).get("state") == "compared",
            "P-1 FAIL db_state = report.db.state = compared（实际 %r / %r）"
            % (cmp_f.get("db_state"), (rep_f.get("db") or {}).get("state")))
        chk(cmp_f.get("diffs") == len(rep_f.get("diffs") or []) == 1,
            "P-1 FAIL diffs = len(report.diffs) = 1（实际 %r / %r）"
            % (cmp_f.get("diffs"), len(rep_f.get("diffs") or [])))
        chk(cmp_f.get("undecidable") == len(rep_f.get("undecidable") or []) == 0,
            "P-1 FAIL undecidable = len(report.undecidable) = 0（实际 %r / %r）"
            % (cmp_f.get("undecidable"), len(rep_f.get("undecidable") or [])))
        chk(cmp_f.get("whitelisted") == len(rep_f.get("whitelisted") or []),
            "P-1 FAIL whitelisted = len(report.whitelisted)（实际 %r / %r）"
            % (cmp_f.get("whitelisted"), len(rep_f.get("whitelisted") or [])))

        # 14) I-2：停栈禁词按效果归一（net.exe stop MySQL / spsv / Stop-Service 全拒）
        for label, bad_stop in (
                ("net.exe stop MySQL", ["net.exe", "stop", "MySQL"]),
                ("spsv MySQL", ["powershell", "-Command", "spsv", "-Name", "MySQL"]),
                ("Stop-Service *", ["powershell", "-Command", "Stop-Service", "*"]),
                ("taskkill mysqld.exe", ["taskkill", "/F", "/IM", "mysqld.exe"])):
            p_i2 = _selftest_profile(tmp, stub, log)
            p_i2["sides"]["csharp"]["stop"] = bad_stop
            i2_refused = False
            try:
                check_profile_safety(load_profile(p_i2), False)
            except ConfigError:
                i2_refused = True
            chk(i2_refused, "I-2 停栈 %s => 拒绝（.exe 归一 + 服务控制族）" % label)
        i2_ok = True
        try:
            check_profile_safety(load_profile(_selftest_profile(tmp, stub, log)), False)
        except ConfigError:
            i2_ok = False
        chk(i2_ok, "I-2 正常 Stop-Process 停栈仍放行")

        # 15) I-3：生产库按展开后的库名集合判定（preset=prod / --db prod / env / 未知库）
        def _set_db(p, spec):
            p["sides"]["csharp"]["db"] = spec

        i3_cases = [
            ("db.preset=prod", lambda p: _set_db(p, {"preset": "prod"})),
            ("db.command --db prod", lambda p: _set_db(
                p, {"command": ["x", "--db", "prod"], "databases": ["mir2_test_db"]})),
            ("env database=Mir2", lambda p: p["sides"]["csharp"].__setitem__(
                "env", {"MIR2_CONN": "server=127.0.0.1;database=Mir2"})),
            ("未知 preset", lambda p: _set_db(p, {"preset": "mystery_db"})),
            ("db.command 未声明 databases", lambda p: _set_db(p, {"command": ["x"]})),
        ]
        for label, mutate in i3_cases:
            p_i3 = _selftest_profile(tmp, stub, log)
            mutate(p_i3)
            i3_refused = False
            try:
                check_profile_safety(load_profile(p_i3), False)
            except ConfigError:
                i3_refused = True
            chk(i3_refused, "I-3 %s => 拒绝（结构性判定）" % label)
        i3_ok = True
        try:
            check_profile_safety(load_profile("alt"), False)
        except ConfigError:
            i3_ok = False
        chk(i3_ok, "I-3 内置 alt（preset=test + mir2_test_*_r）仍放行")

        # 15b) F-3：binaries 是窗口证据的必需项（空列表/缺省 => 加载期 ConfigError，不许 PASS）
        for label, mutate in (
                ("binaries=[]", lambda p: p["sides"]["csharp"].__setitem__("binaries", [])),
                ("缺 binaries", lambda p: p["sides"]["rust"].pop("binaries", None))):
            fresh_log()
            p_f3 = _selftest_profile(tmp, stub, log)
            mutate(p_f3)
            f3_refused = False
            try:
                execute(base_cfg(os.path.join(tmp, "run-f3"), p_f3))
            except ConfigError as e:
                f3_refused = "binaries" in str(e)
            chk(f3_refused, "F-3 %s => 加载期 ConfigError（不判 PASS）" % label)
            chk(not os.path.exists(log), "F-3 %s => 一步都没执行" % label)
        prof_f3ok = load_profile(_selftest_profile(tmp, stub, log))
        both_bins = {s: bin_info(prof_f3ok["sides"][s]) for s in SIDES}
        chk(all(both_bins[s] and both_bins[s][0]["sha256"] == sha256_file(stub)
                for s in SIDES),
            "F-3 正常两侧 => 加载通过且两侧二进制 sha256 都算得出")

        # 15c) F-1：未知库名是**永久拒绝**——在 db.databases 里显式声明也不放行
        #      （--allow-production-db / --dry-run 同样），提示不再教人做无用功。
        p_f1 = _selftest_profile(tmp, stub, log)
        p_f1["sides"]["csharp"]["db"] = {"command": ["x", "--db", "mystery_db"],
                                         "databases": ["mystery_db"]}
        f1_msg = ""
        try:
            check_profile_safety(load_profile(p_f1), True, dry_run=True)
        except ConfigError as e:
            f1_msg = str(e)
        chk("mystery_db" in f1_msg and "永久拒绝" in f1_msg,
            "F-1 显式声明未知库 + allow-production-db + dry-run => 仍拒绝（%r）"
            % f1_msg[:100])
        chk("在 db.databases 里显式声明" not in f1_msg
            and "mir2_test_*" in f1_msg,
            "F-1 提示不再教「在 db.databases 里显式声明」，改为指名换库")

        # 16) I-4：真跑缺 --out => rc=2，且不往 cwd 写任何证据
        old_cwd = os.getcwd()
        os.chdir(tmp)
        try:
            s_noout = execute(base_cfg(None, _selftest_profile(tmp, stub, log)))
            cwd_leak = any(os.path.exists(os.path.join(tmp, f)) for f in
                           ("report.json", "card-window.json", "README.txt", "card-window.log"))
        finally:
            os.chdir(old_cwd)
        chk(s_noout["exit_code"] == 2
            and any("--out" in n for n in s_noout["notes"]),
            "I-4 真跑缺 --out => rc=2")
        chk(not cwd_leak, "I-4 缺 --out 不往 cwd 写证据")

        # 17) M-1：run.json 必须非空且 exit.code == probe rc（0）
        p_m1 = _selftest_profile(tmp, stub, log)
        p_m1["sides"]["csharp"]["probe_extra"] = ["--stub-exit-code", "3"]
        s_m1 = execute(base_cfg(os.path.join(tmp, "run-m1"), p_m1))
        err_m1 = (s_m1["sides"].get("csharp") or {}).get("error") or ""
        chk(s_m1["exit_code"] == 2 and "exit.code" in err_m1,
            "M-1 run.json exit.code=3 与 probe rc=0 不一致 => rc=2")
        p_m1b = _selftest_profile(tmp, stub, log)
        p_m1b["sides"]["csharp"]["probe_extra"] = ["--stub-runjson-empty"]
        s_m1b = execute(base_cfg(os.path.join(tmp, "run-m1b"), p_m1b))
        err_m1b = (s_m1b["sides"].get("csharp") or {}).get("error") or ""
        chk(s_m1b["exit_code"] == 2 and "run.json 为空" in err_m1b,
            "M-1 空 run.json => rc=2")

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
    ap.add_argument("--out", help="产物目录（真跑必填；--dry-run 可省）")
    ap.add_argument("--profile", default="alt",
                    help="profile：alt（默认）| csharp-default | <profile.json 路径>")
    ap.add_argument("--post-comment", action="store_true",
                    help="跑完在该卡发一条签名 comment（含 VERDICT/帧数/sha256/证据路径；不改卡状态）")
    ap.add_argument("--dry-run", action="store_true",
                    help="只打印完整命令序列，不起栈/不建目录/不需要密码")
    ap.add_argument("--discover", action="store_true",
                    help="强制先跑 discovery（参数发现）再跑场景；场景声明 required 时自动启用")
    ap.add_argument("--discovery-scenario",
                    help="发现阶段的场景（默认 tools/probe/scenarios/npc_click_auto.json；"
                         "必须点击一次 NPC 才收得到 SM_MERCHANTSAY(643)）")
    ap.add_argument("--discovery-frames",
                    help="[仅 --dry-run] 用已录 frames 回放推导，打印带真实值的 --var 命令序列")
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
        "discover": args.discover,
        "discovery_scenario": args.discovery_scenario,
        "discovery_frames": args.discovery_frames,
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
        if summary.get("exit_code") != 0:
            for note in summary.get("notes") or []:
                print("card_window.py: %s" % note, file=sys.stderr)
        return int(summary.get("exit_code") or 0)
    if summary.get("exit_code") != 0:
        for note in summary.get("notes") or []:
            print("card_window.py: %s" % note, file=sys.stderr)
    print("VERDICT: %s (compare_rc=%s, exit=%s)  out=%s"
          % (summary.get("verdict"), summary.get("compare_rc"),
             summary.get("exit_code"), summary.get("out")))
    return int(summary.get("exit_code") or 2)


if __name__ == "__main__":
    sys.exit(main())
